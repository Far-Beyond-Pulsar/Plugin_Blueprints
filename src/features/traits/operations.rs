use crate::editor::panel::BlueprintEditorPanel;
use gpui::{Context, Window};
use std::collections::BTreeSet;
use std::path::Path;

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
                        editor.reconcile_trait_references();
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

    /// Assign the trait with this id. Both the id (authoritative) and the
    /// trait's current path (a hint for readers that predate ids) are stored.
    pub fn add_implemented_trait(
        &mut self,
        trait_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(asset) = self
            .implemented_trait_catalog
            .iter()
            .find(|asset| asset.id == trait_id)
            .cloned()
        else {
            self.implemented_trait_status = Some(format!(
                "Trait '{}' is not available in this project",
                trait_id
            ));
            cx.notify();
            return;
        };

        let metadata = &mut self.blueprint_metadata;
        if metadata.implemented_trait_ids.iter().any(|id| id == &asset.id) {
            return;
        }
        metadata.implemented_trait_ids.push(asset.id.clone());
        metadata.implemented_trait_ids.sort();
        if is_trait_file(&asset.path) && !metadata.implemented_traits.contains(&asset.path) {
            metadata.implemented_traits.push(asset.path.clone());
            metadata.implemented_traits.sort();
        }
        self.sync_implemented_trait_picker(cx);
        self.persist_trait_assignments(window, cx);
    }

    /// Remove an assigned trait. `trait_ref` is a trait id, or the path of a
    /// reference that names no trait any more.
    pub fn remove_implemented_trait(
        &mut self,
        trait_ref: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_path = self
            .implemented_trait_catalog
            .iter()
            .find(|asset| asset.id == trait_ref)
            .map(|asset| asset.path.clone());
        let metadata = &mut self.blueprint_metadata;
        metadata.implemented_trait_ids.retain(|id| id != trait_ref);
        metadata
            .implemented_traits
            .retain(|path| path != trait_ref && Some(path) != current_path.as_ref());
        self.sync_implemented_trait_picker(cx);
        self.persist_trait_assignments(window, cx);
    }

    /// What the traits panel lists: the ids of assigned traits, then the
    /// paths of references that resolve to no trait.
    pub fn implemented_trait_refs(&self) -> Vec<String> {
        let mut refs = self.blueprint_metadata.implemented_trait_ids.clone();
        refs.extend(
            self.blueprint_metadata
                .implemented_traits
                .iter()
                .filter(|path| {
                    !self
                        .implemented_trait_catalog
                        .iter()
                        .any(|asset| &asset.path == *path)
                })
                .cloned(),
        );
        refs
    }

    /// Bring the Blueprint's trait references in step with the project's
    /// traits, in memory: path-only references (assets saved before trait ids
    /// existed) gain the id of the trait they name, and every path becomes the
    /// trait's current one after a rename or move. Nothing is marked dirty;
    /// the next save writes the result. A path that names no trait stays so it
    /// is shown as missing, unless every id resolves, which makes it only a
    /// stale hint.
    pub fn reconcile_trait_references(&mut self) {
        if self.implemented_trait_catalog.is_empty() {
            return;
        }
        let catalog = &self.implemented_trait_catalog;
        let metadata = &mut self.blueprint_metadata;
        let ids_cover_paths = !metadata.implemented_trait_ids.is_empty()
            && metadata
                .implemented_trait_ids
                .iter()
                .all(|id| catalog.iter().any(|asset| &asset.id == id));

        let mut ids: BTreeSet<String> = metadata.implemented_trait_ids.iter().cloned().collect();
        let mut unresolved_paths = BTreeSet::new();
        for path in &metadata.implemented_traits {
            match catalog.iter().find(|asset| &asset.path == path) {
                Some(asset) => {
                    ids.insert(asset.id.clone());
                }
                None if !ids_cover_paths => {
                    unresolved_paths.insert(path.clone());
                }
                None => {}
            }
        }
        let mut paths = unresolved_paths;
        paths.extend(
            ids.iter()
                .filter_map(|id| catalog.iter().find(|asset| &asset.id == id))
                .filter(|asset| is_trait_file(&asset.path))
                .map(|asset| asset.path.clone()),
        );
        metadata.implemented_trait_ids = ids.into_iter().collect();
        metadata.implemented_traits = paths.into_iter().collect();
    }

    fn sync_implemented_trait_picker(&self, cx: &mut Context<Self>) {
        let selected = &self.blueprint_metadata.implemented_trait_ids;
        let available = self
            .implemented_trait_catalog
            .iter()
            .filter(|asset| !selected.iter().any(|id| id == &asset.id))
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

/// The project's traits from the trait index (which also gives any trait
/// that lacks one a stable id).
fn load_trait_catalog(
    root: &Path,
) -> Result<Vec<crate::features::traits::TraitAssetSummary>, String> {
    let index = engine_fs::BlueprintTraitIndex::load_or_rebuild(root)
        .map_err(|error| format!("Could not scan project traits: {error:#}"))?;
    let mut assets: Vec<_> = index
        .traits()
        .iter()
        .map(|record| crate::features::traits::TraitAssetSummary {
            id: record.id.clone(),
            path: record.path.clone(),
            name: record.name.clone(),
            display_name: record.display_name.clone(),
            description: record.description.clone().unwrap_or_default(),
        })
        .collect();
    assets.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then(left.path.cmp(&right.path))
    });
    Ok(assets)
}

fn is_trait_file(path: &str) -> bool {
    path.ends_with(".trait.json")
}
