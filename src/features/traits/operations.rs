use crate::editor::panel::BlueprintEditorPanel;
use gpui::{Context, Window};
use std::collections::BTreeSet;
use std::path::Path;

use super::events;

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
                        editor.refresh_trait_palette();
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
    /// Only the changed file is re-read, on a background thread
    /// (`BlueprintTraitIndex::update_path`); the catalog then reloads from the
    /// persisted index.
    pub fn refresh_for_trait_asset_event(&mut self, path: &Path, cx: &mut Context<Self>) {
        if !is_trait_catalog_event_path(path, self.project_root.as_deref()) {
            return;
        }
        self.update_trait_index_then_refresh(Some(path.to_path_buf()), cx);
    }

    /// Re-scan the whole project for traits (the panel's Refresh button), on
    /// a background thread.
    pub fn rescan_implemented_trait_catalog(&mut self, cx: &mut Context<Self>) {
        self.update_trait_index_then_refresh(None, cx);
    }

    fn update_trait_index_then_refresh(
        &mut self,
        changed: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let root = self.project_root.clone().or_else(|| {
            self.current_class_path
                .as_deref()
                .and_then(crate::features::class_dirs::project_root_of)
        });
        let Some(root) = root else {
            self.implemented_trait_catalog_loaded = false;
            self.refresh_implemented_trait_catalog(cx);
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match changed {
                        Some(path) => {
                            engine_fs::BlueprintTraitIndex::update_path(&root, &path).map(|_| ())
                        }
                        None => engine_fs::BlueprintTraitIndex::rebuild(&root).map(|_| ()),
                    }
                })
                .await;
            if let Err(error) = result {
                tracing::warn!("Could not update the trait index: {error:#}");
            }
            let _ = this.update(cx, |editor, cx| {
                editor.implemented_trait_catalog_loaded = false;
                editor.refresh_implemented_trait_catalog(cx);
            });
        })
        .detach();
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
        self.refresh_trait_palette();
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
        self.refresh_trait_palette();
        self.persist_trait_assignments(window, cx);
        let orphans = self.orphaned_trait_event_nodes(cx);
        if !orphans.is_empty() {
            self.implemented_trait_status = Some(format!(
                "Trait removed. {} placed event node(s) it provided are orphaned and will not compile until deleted or the trait is added back.",
                orphans.len()
            ));
            cx.notify();
        }
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

    /// Persist a trait change without blocking the UI.
    ///
    /// Adding or removing a trait only edits the Blueprint's metadata; the
    /// Add menu entries are derived from the catalog already in memory
    /// ([`refresh_trait_palette`](Self::refresh_trait_palette)). The save goes
    /// through the background `plugin_save` task (serialize, write, then
    /// `BlueprintTraitIndex::update_path` for this one file). Nothing here
    /// reads the project, rescans traits or compiles.
    fn persist_trait_assignments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.is_dirty = true;
        if self.get_graph_file_path().is_none() {
            self.implemented_trait_status =
                Some("Cannot save trait selection without a Blueprint file path".to_owned());
        } else if self.is_saving {
            // The running save does not include this change; the Blueprint
            // stays dirty, so autosave or the next save writes it.
            self.implemented_trait_status =
                Some("Trait selection changed; it will be saved with the next save".to_owned());
        } else {
            self.implemented_trait_status = Some("Trait selection saving…".to_owned());
            self.plugin_save(window, cx);
        }
        cx.notify();
    }

    /// The methods the Blueprint's declared traits require, from the
    /// in-memory catalog (no I/O).
    pub fn trait_method_specs(&self) -> Vec<events::TraitMethodSpec> {
        let mut seen = BTreeSet::new();
        self.blueprint_metadata
            .implemented_trait_ids
            .iter()
            .filter_map(|id| self.implemented_trait_catalog.iter().find(|asset| &asset.id == id))
            .flat_map(|asset| asset.methods.iter())
            .filter(|spec| seen.insert(spec.name.clone()))
            .cloned()
            .collect()
    }

    /// Republish the trait entries of the Add menu: one in-memory splice of
    /// the profile's catalog, leaving every other entry alone.
    pub fn refresh_trait_palette(&self) {
        self.refresh_trait_catalog();
    }

    /// `(node id, node type)` of every node placed in the Blueprint's main
    /// graph and in tabs not yet opened. Reads memory only.
    fn placed_nodes(&self, cx: &gpui::App) -> Vec<(String, String)> {
        let mut nodes = Vec::new();
        if let Some(canvas) = self.main_canvas() {
            nodes.extend(
                canvas
                    .read(cx)
                    .graph(cx)
                    .nodes
                    .values()
                    .map(|node| (node.id.clone(), node.node_type.clone())),
            );
        }
        for tab in &self.open_tabs {
            if let Some(doc) = &tab.pending {
                nodes.extend(
                    doc.graph
                        .nodes
                        .values()
                        .map(|node| (node.id.clone(), node.node_type.clone())),
                );
            }
        }
        nodes
    }

    /// Placed trait event nodes that no trait or custom event provides.
    /// They are kept (see `events`), reported, and fail the compile until
    /// deleted or the trait returns.
    pub fn orphaned_trait_event_nodes(&self, cx: &gpui::App) -> Vec<String> {
        // Every trait method known to the project, minus what is declared.
        let all: Vec<events::TraitMethodSpec> = self
            .implemented_trait_catalog
            .iter()
            .flat_map(|asset| asset.methods.iter().cloned())
            .collect();
        let class = self.trait_event_class_name();
        let local: Vec<String> = self.local_event_defs.iter().map(|d| d.name.clone()).collect();
        events::orphaned_events(&class, &all, &self.trait_method_specs(), &local, &self.placed_nodes(cx))
    }

    pub(crate) fn trait_event_class_name(&self) -> String {
        self.current_class_path
            .as_deref()
            .and_then(crate::features::class_dirs::class_name_of)
            .unwrap_or_else(|| "unnamed_blueprint".to_owned())
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
    // The persisted index is current: saves and trait events update it. A
    // project without one (or from an older engine) is scanned once.
    let index = engine_fs::BlueprintTraitIndex::load(root)
        .or_else(|_| engine_fs::BlueprintTraitIndex::rebuild(root))
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
            methods: super::events::specs_for(index.db(), std::slice::from_ref(&record.id)),
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
