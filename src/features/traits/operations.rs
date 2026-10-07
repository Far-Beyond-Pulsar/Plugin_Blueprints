use crate::editor::panel::BlueprintEditorPanel;
use gpui::{Context, Window};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

impl BlueprintEditorPanel {
    /// Refreshes the project-relative `.trait.json` catalog using the active
    /// virtual filesystem provider.
    pub fn refresh_implemented_trait_catalog(&mut self, cx: &mut Context<Self>) {
        if self.implemented_trait_catalog_loading {
            self.implemented_trait_catalog_refresh_pending = true;
            return;
        }
        self.implemented_trait_catalog_refresh_pending = false;
        let root = self.project_root.clone().or_else(|| {
            self.current_class_path
                .as_deref()
                .and_then(crate::features::class_dirs::project_root_of)
        });
        let Some(root) = root else {
            self.implemented_trait_catalog.clear();
            self.implemented_trait_catalog_error =
                Some("The project root is not available for trait discovery".to_owned());
            self.implemented_trait_catalog_loaded = true;
            self.sync_implemented_trait_picker(cx);
            return;
        };
        self.implemented_trait_catalog_loading = true;
        self.implemented_trait_catalog_error = None;
        let project_root = root.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_trait_catalog(&root) })
                .await;
            let _ = this.update(cx, |editor, cx| {
                editor.implemented_trait_catalog_loading = false;
                editor.implemented_trait_catalog_loaded = true;
                editor.project_root = Some(project_root);
                match result {
                    Ok(catalog) => {
                        editor.implemented_trait_catalog = catalog;
                        editor.implemented_trait_catalog_error = None;
                    }
                    Err(error) => {
                        editor.implemented_trait_catalog.clear();
                        editor.implemented_trait_catalog_error = Some(error);
                    }
                }
                editor.sync_implemented_trait_picker(cx);
                cx.notify();
                if std::mem::take(&mut editor.implemented_trait_catalog_refresh_pending) {
                    editor.refresh_implemented_trait_catalog(cx);
                }
            });
        })
        .detach();
    }

    /// Refresh the catalog after a virtual filesystem event that can change
    /// trait discovery. Any project-relative `.trait.json` asset can be a trait;
    /// new files are still created in the canonical `types/traits` directory.
    pub fn refresh_for_trait_asset_event(&mut self, path: &Path, cx: &mut Context<Self>) {
        if !is_trait_catalog_event_path(path, self.project_root.as_deref()) {
            return;
        }
        self.implemented_trait_catalog_loaded = false;
        self.refresh_implemented_trait_catalog(cx);
    }

    pub fn add_implemented_trait(
        &mut self,
        trait_path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(trait_path) = normalize_trait_path(trait_path) else {
            self.implemented_trait_status =
                Some("Trait path is not a canonical project-relative .trait.json path".to_owned());
            cx.notify();
            return;
        };
        if !self
            .implemented_trait_catalog
            .iter()
            .any(|asset| asset.path == trait_path)
        {
            self.implemented_trait_status = Some(format!(
                "Trait asset '{}' is not available in this project",
                trait_path
            ));
            cx.notify();
            return;
        }

        let selected = &mut self.blueprint_metadata.implemented_traits;
        if selected.iter().any(|path| path == &trait_path) {
            return;
        }
        selected.push(trait_path);
        selected.sort();
        self.sync_implemented_trait_picker(cx);
        self.persist_trait_assignments(window, cx);
    }

    pub fn remove_implemented_trait(
        &mut self,
        trait_path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(trait_path) = normalize_trait_path(trait_path) else {
            self.implemented_trait_status = Some("Trait path is not canonical".to_owned());
            cx.notify();
            return;
        };
        self.blueprint_metadata
            .implemented_traits
            .retain(|path| path != &trait_path);
        self.sync_implemented_trait_picker(cx);
        self.persist_trait_assignments(window, cx);
    }

    fn sync_implemented_trait_picker(&self, cx: &mut Context<Self>) {
        let selected = &self.blueprint_metadata.implemented_traits;
        let available = self
            .implemented_trait_catalog
            .iter()
            .filter(|asset| !selected.iter().any(|path| path == &asset.path))
            .cloned()
            .collect();
        self.implemented_trait_picker
            .update(cx, |picker, cx| picker.set_items(available, cx));
    }

    fn persist_trait_assignments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.get_graph_file_path() else {
            self.implemented_trait_status =
                Some("Cannot save trait selection without a Blueprint file path".to_owned());
            cx.notify();
            return;
        };
        match self.save_to_path(&path, window, cx) {
            Ok(()) => {
                self.is_dirty = false;
                self.implemented_trait_status = Some("Trait selection saved".to_owned());
            }
            Err(error) => {
                self.is_dirty = true;
                self.implemented_trait_status =
                    Some(format!("Could not save trait selection: {error}"));
            }
        }
        cx.notify();
    }
}

fn is_trait_catalog_event_path(path: &Path, project_root: Option<&Path>) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let normalized = normalized.trim_end_matches('/').to_ascii_lowercase();
    if let Some(root) = project_root {
        let root = root
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase();
        let Some(relative) = normalized.strip_prefix(&root) else {
            return false;
        };
        let relative = relative.trim_start_matches('/');
        return relative.ends_with(".trait.json");
    }

    normalized.ends_with(".trait.json")
}

fn load_trait_catalog(
    root: &Path,
) -> Result<Vec<crate::features::traits::TraitAssetSummary>, String> {
    let manifest = engine_fs::virtual_fs::manifest(root)
        .map_err(|error| format!("Could not scan project traits: {error}"))?;
    let mut assets = Vec::new();
    let mut paths = BTreeSet::new();
    for entry in manifest.into_iter().filter(|entry| !entry.is_dir) {
        let Some(path) = normalize_trait_path(&entry.path) else {
            continue;
        };
        if !paths.insert(path.clone()) {
            continue;
        }
        let file_path = root.join(&path);
        let bytes = match engine_fs::virtual_fs::read_file(&file_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(path = %path, %error, "Skipping unreadable trait asset");
                continue;
            }
        };
        let value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(path = %path, %error, "Skipping malformed trait asset");
                continue;
            }
        };
        let Some(name) = value.get("name").and_then(serde_json::Value::as_str) else {
            tracing::warn!(path = %path, "Skipping trait asset without a name");
            continue;
        };
        let display_name = value
            .get("displayName")
            .or_else(|| value.get("display_name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(name)
            .to_owned();
        let description = value
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        assets.push(crate::features::traits::TraitAssetSummary {
            path,
            name: name.to_owned(),
            display_name,
            description,
        });
    }
    assets.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then(left.path.cmp(&right.path))
    });
    Ok(assets)
}

fn normalize_trait_path(raw: &str) -> Option<String> {
    let normalized = raw.replace('\\', "/");
    let path = PathBuf::from(&normalized);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    let parts = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    let path = parts.join("/");
    path.ends_with(".trait.json").then_some(path)
}
