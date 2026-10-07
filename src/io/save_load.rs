//! Save and load operations for blueprint files
//!
//! This module provides the main save/load functionality for blueprints,
//! including autosave, format detection, and legacy format migration.

use super::{formats, legacy};
use crate::core::types::CompilationState;
use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::tabs::GraphTab;
use gpui::*;
use std::path::{Path, PathBuf};

const GRAPH_SAVE_FILE_NAME: &str = "graph_save.json";

fn serialize_blueprint_asset(asset: &formats::BlueprintAsset) -> Result<String, String> {
    formats::serialize_blueprint_with_header(asset)
}

fn persist_blueprint_content(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        engine_fs::virtual_fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create directory: {error}"))?;
    }
    engine_fs::virtual_fs::write_file(path, content.as_bytes())
        .map_err(|error| format!("Failed to write file: {error}"))
}

fn persist_blueprint_asset(path: &Path, asset: &formats::BlueprintAsset) -> Result<usize, String> {
    let content = serialize_blueprint_asset(asset)?;
    persist_blueprint_content(path, &content)?;
    Ok(content.len())
}

fn persist_prefab_sidecar(path: &Path, json: &str) -> Result<(), String> {
    if let Some(class_dir) = path.parent() {
        std::fs::create_dir_all(class_dir)
            .map_err(|error| format!("Failed to create class directory: {error}"))?;
        crate::features::prefabs::ensure_class_id(class_dir)?;
    }
    std::fs::write(path, json).map_err(|error| format!("Failed to write prefab sidecar: {error}"))
}

fn refresh_blueprint_trait_index_at(project_root: Option<&Path>) {
    if let Some(project_root) = project_root {
        if let Err(error) = engine_fs::BlueprintTraitIndex::rebuild(project_root) {
            tracing::warn!(
                project_root = %project_root.display(),
                %error,
                "Failed to refresh Blueprint trait index after save"
            );
        }
    }
}

impl BlueprintEditorPanel {
    fn snapshot_blueprint_for_save(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<formats::BlueprintAsset, String> {
        self.sync_all_canvases_to_tabs(cx);
        self.to_blueprint_asset()
    }

    fn snapshot_prefab_sidecar_for_save(&mut self) -> Result<(PathBuf, String), String> {
        self.sync_prefab_to_script()?;
        let path = self
            .prefab_file_path()
            .ok_or_else(|| "No class path available for prefab save".to_string())?;
        self.prefab_asset.fill_missing_slot_ids();
        let json = serde_json::to_string_pretty(&self.prefab_asset)
            .map_err(|error| format!("Failed to serialize prefab: {error}"))?;
        Ok((path, json))
    }

    /// Save the current blueprint to its file path
    pub fn plugin_save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_saving {
            return;
        }

        let file_path = self.get_graph_file_path();
        tracing::info!(
            ">>> plugin_save called: current_class_path={:?}, file_path={:?}, is_dirty={}, graph_panels_count={}, open_tabs_count={}, self.graph.nodes={}",
            self.current_class_path,
            file_path,
            self.is_dirty,
            self.graph_panels.len(),
            self.open_tabs.len(),
            self.graph.nodes.len(),
        );

        let Some(path) = file_path else {
            tracing::warn!(">>> plugin_save: No save path set - cannot save blueprint");
            self.compilation_status.state = CompilationState::Error;
            self.compilation_status.message = "Save failed: no file path set".to_string();
            cx.notify();
            return;
        };

        // Snapshot live canvases and convert UI state before handing work off.
        // Serialization and all file writes then happen on a long-running task.
        let asset = match self.snapshot_blueprint_for_save(cx) {
            Ok(asset) => asset,
            Err(error) => {
                self.report_save_preparation_error(error, cx);
                return;
            }
        };

        let prefab_save = self.snapshot_prefab_sidecar_for_save();
        let prefab_save = match prefab_save {
            Ok(save) => Some(save),
            Err(error) => {
                tracing::warn!("Failed to prepare prefab sidecar save: {error}");
                None
            }
        };

        let target_path = Self::resolve_blueprint_path(&path);
        let project_root = self.project_root.clone().or_else(|| {
            self.current_class_path
                .as_deref()
                .and_then(crate::features::class_dirs::project_root_of)
        });
        let class_dir = self.current_class_path.clone();
        let publish_after_save = prefab_save.is_some();

        // Existing dirty flags describe the state being captured by this save.
        // Reset them now so edits made while the task runs remain dirty.
        self.is_dirty = false;
        for tab in &mut self.open_tabs {
            tab.is_dirty = false;
        }
        let canvases: Vec<_> = self
            .graph_panels
            .iter()
            .map(|(_, canvas)| canvas.clone())
            .collect();
        for canvas in canvases {
            canvas.update(cx, |canvas, cx| {
                canvas.is_dirty = false;
                cx.notify();
            });
        }

        self.is_saving = true;
        cx.notify();

        let (completion_tx, completion_rx) =
            smol::channel::bounded::<Result<Option<String>, String>>(1);
        let task_path = target_path.clone();
        editor_task_queue::global().submit(
            editor_task_queue::TaskDescription::new(
                format!(
                    "Save Blueprint: {}",
                    target_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("graph")
                ),
                "Blueprints",
                editor_task_queue::TaskDuration::Long,
            ),
            move |task| {
                let result = (|| -> Result<Option<String>, String> {
                    task.report_progress(0.05, "Serializing Blueprint graph");
                    if task.is_cancelled() {
                        return Err("Blueprint save cancelled".to_string());
                    }

                    let content = serialize_blueprint_asset(&asset)?;
                    task.report_progress(0.45, "Writing Blueprint graph");
                    persist_blueprint_content(&task_path, &content)?;

                    task.report_progress(0.7, "Writing prefab sidecar");
                    let prefab_error = prefab_save.and_then(|(prefab_path, json)| {
                        persist_prefab_sidecar(&prefab_path, &json).err()
                    });
                    if let Some(error) = &prefab_error {
                        tracing::warn!("Failed to save prefab sidecar: {error}");
                    }

                    task.report_progress(0.9, "Refreshing Blueprint index");
                    refresh_blueprint_trait_index_at(project_root.as_deref());

                    tracing::info!(">>> plugin_save: SUCCESS wrote to {:?}", task_path);
                    task.report_progress(1.0, "Blueprint saved");
                    Ok(prefab_error)
                })();
                match result {
                    Ok(prefab_warning) => {
                        let _ = completion_tx.try_send(Ok(prefab_warning));
                        Ok(())
                    }
                    Err(error) => {
                        let _ = completion_tx.try_send(Err(error.clone()));
                        Err(error)
                    }
                }
            },
        );

        cx.spawn(async move |this, cx| {
            let save_result = completion_rx.recv().await.unwrap_or_else(|_| {
                Err("Blueprint save task ended without reporting a result".to_string())
            });
            let _ = this.update(cx, |panel, cx| {
                panel.is_saving = false;
                match save_result {
                    Ok(prefab_error) => {
                        let prefab_saved = publish_after_save && prefab_error.is_none();
                        if let Some(error) = prefab_error {
                            tracing::warn!("Prefab sidecar save warning: {error}");
                        }
                        if prefab_saved {
                            if let Some(class_dir) = class_dir {
                                crate::features::prefabs::publish_class_updated(&class_dir);
                            }
                        }

                        let graph_edits_pending = panel
                            .graph_panels
                            .iter()
                            .any(|(_, canvas)| canvas.read(cx).is_dirty);
                        let tabs_edits_pending = panel.open_tabs.iter().any(|tab| tab.is_dirty);
                        panel.is_dirty =
                            panel.is_dirty || graph_edits_pending || tabs_edits_pending;
                        if !panel.is_dirty {
                            for tab in &mut panel.open_tabs {
                                tab.is_dirty = false;
                            }
                        }
                        tracing::info!(">>> plugin_save: async save completed");
                    }
                    Err(error) => {
                        tracing::error!(">>> plugin_save: FAILED to save blueprint: {error}");
                        panel.is_dirty = true;
                        panel.compilation_status.state = CompilationState::Error;
                        panel.compilation_status.message = format!("Save failed: {error}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn report_save_preparation_error(&mut self, error: String, cx: &mut Context<Self>) {
        tracing::error!(">>> plugin_save: FAILED to prepare blueprint save: {error}");
        self.compilation_status.state = CompilationState::Error;
        self.compilation_status.message = format!("Save failed: {error}");
        cx.notify();
    }

    /// Reload the blueprint from its file path
    pub fn plugin_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(
            ">>> plugin_reload called: current_class_path={:?}",
            self.current_class_path
        );
        if let Some(path) = self.get_graph_file_path() {
            match self.load_from_path(&path, window, cx) {
                Ok(()) => {
                    tracing::info!(">>> plugin_reload: SUCCESS reloaded from {:?}", path);
                    self.is_dirty = false;

                    // Clear dirty flags for all tabs after reload
                    for tab in &mut self.open_tabs {
                        tab.is_dirty = false;
                    }

                    cx.notify();
                }
                Err(e) => {
                    tracing::error!(">>> plugin_reload: FAILED: {}", e);
                    self.compilation_status.state = CompilationState::Error;
                    self.compilation_status.message = format!("Reload failed: {}", e);
                    cx.notify();
                }
            }
        } else {
            tracing::warn!(">>> plugin_reload: No file path set");
        }
    }

    /// Save blueprint to a specific path
    pub fn save_to_path(
        &mut self,
        path: &Path,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let target_path = Self::resolve_blueprint_path(path);
        tracing::info!(
            ">>> save_to_path: path={:?} resolved={:?}, graph_panels={}, open_tabs={}, self.graph.nodes={}",
            path, target_path,
            self.graph_panels.len(),
            self.open_tabs.len(),
            self.graph.nodes.len(),
        );

        // Log what's in the graph panels before sync
        for (tid, canvas) in &self.graph_panels {
            let cg = canvas.read(cx);
            tracing::info!(
                ">>> save_to_path: canvas tab={} nodes={} connections={}",
                tid,
                cg.graph.nodes.len(),
                cg.graph.connections.len()
            );
        }

        // Log what's in open_tabs before sync
        for tab in &self.open_tabs {
            tracing::info!(
                ">>> save_to_path: pre-sync tab={} is_main={} nodes={} connections={}",
                tab.id,
                tab.is_main,
                tab.graph.nodes.len(),
                tab.graph.connections.len()
            );
        }

        // Flush every open canvas's live graph into its tab snapshot before
        // serializing. Toolbar and synchronous saves share this snapshot path.
        let asset = self.snapshot_blueprint_for_save(cx)?;

        // Log what's in open_tabs after sync
        for tab in &self.open_tabs {
            tracing::info!(
                ">>> save_to_path: post-sync tab={} is_main={} nodes={} connections={}",
                tab.id,
                tab.is_main,
                tab.graph.nodes.len(),
                tab.graph.connections.len()
            );
        }

        tracing::info!(
            ">>> save_to_path: BlueprintAsset created: main_graph has {} nodes",
            asset.main_graph.nodes.len(),
        );

        let bytes_written = persist_blueprint_asset(&target_path, &asset)?;

        // The project index is derived from authored Blueprint metadata. Keep
        // it current for toolbar saves, assignment-panel saves and autosaves.
        let project_root = self.project_root.clone().or_else(|| {
            self.current_class_path
                .as_deref()
                .and_then(crate::features::class_dirs::project_root_of)
        });
        refresh_blueprint_trait_index_at(project_root.as_deref());

        tracing::info!(
            ">>> save_to_path: wrote {} bytes to {:?}",
            bytes_written,
            target_path
        );
        Ok(())
    }

    /// Load blueprint from a specific path
    pub fn load_from_path(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let source_path = Self::resolve_blueprint_path(path);
        tracing::info!(
            ">>> load_from_path: path={:?} resolved={:?}, current open_tabs={}",
            path,
            source_path,
            self.open_tabs.len(),
        );

        // Read file content
        let bytes = engine_fs::virtual_fs::read_file(&source_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;
        let content = String::from_utf8(bytes)
            .map_err(|e| format!("Blueprint file is not valid UTF-8: {}", e))?;
        tracing::info!(
            ">>> load_from_path: read {} bytes from {:?}",
            content.len(),
            source_path
        );

        // Try to deserialize as current format first
        let asset = match formats::deserialize_blueprint(&content) {
            Ok(asset) => {
                tracing::info!(
                    ">>> load_from_path: deserialized as current format, main_graph has {} nodes, {} subgraphs, {} variables",
                    asset.main_graph.nodes.len(),
                    asset.subgraphs.len(),
                    asset.variables.len(),
                );
                asset
            }
            Err(_) => {
                // Try legacy format
                tracing::info!(">>> load_from_path: Trying to load as legacy format...");
                let legacy_graph = legacy::try_parse_legacy_format(&content)?;

                // Convert legacy graph to current format
                formats::BlueprintAsset {
                    format_version: formats::current_format_version(),
                    main_graph: legacy_graph,
                    subgraphs: Vec::new(),
                    local_events: Vec::new(),
                    variables: Vec::new(),
                    editor_state: None,
                    blueprint_metadata: Default::default(),
                }
            }
        };

        // Load the asset into the editor
        self.load_blueprint_asset(asset, window, cx)?;
        self.set_path(source_path);
        self.load_prefab_sidecar()?;

        tracing::info!(
            ">>> load_from_path: done. open_tabs={}, graph_panels={}, self.graph.nodes={}",
            self.open_tabs.len(),
            self.graph_panels.len(),
            self.graph.nodes.len(),
        );

        Ok(())
    }

    /// Convert current editor state to BlueprintAsset
    pub(crate) fn to_blueprint_asset(&self) -> Result<formats::BlueprintAsset, String> {
        tracing::info!(
            ">>> to_blueprint_asset: open_tabs={}, self.graph.nodes={}, self.graph.connections={}",
            self.open_tabs.len(),
            self.graph.nodes.len(),
            self.graph.connections.len(),
        );

        // Always serialize the main event graph from the main tab snapshot,
        // not from `self.graph` (which may currently be a macro tab).
        let main_tab = self
            .open_tabs
            .iter()
            .find(|tab| tab.is_main)
            .ok_or("No main graph tab found")?;
        tracing::info!(
            ">>> to_blueprint_asset: main tab id={} nodes={} connections={}",
            main_tab.id,
            main_tab.graph.nodes.len(),
            main_tab.graph.connections.len(),
        );
        let main_graph = self.convert_graph_to_description(&main_tab.graph)?;

        // Serialize local macros from the open tab snapshots when those tabs are
        // present, so each graph is persisted independently.
        let mut subgraphs = self.subgraphs.clone();
        for tab in self
            .open_tabs
            .iter()
            .filter(|tab| !tab.is_main && !tab.is_library_macro)
        {
            if let Some(macro_def) = subgraphs.iter_mut().find(|m| m.id == tab.id) {
                tracing::info!(
                    ">>> to_blueprint_asset: saving macro tab id={} nodes={}",
                    tab.id,
                    tab.graph.nodes.len(),
                );
                macro_def.graph = self.convert_graph_to_description(&tab.graph)?;
            }
        }

        // Convert local ClassVariable to ui::ClassVariable
        let variables: Vec<blueprint_graph::ClassVariable> = self
            .class_variables
            .iter()
            .map(|v| blueprint_graph::ClassVariable {
                id: v.id.clone(),
                name: v.name.clone(),
                data_type: blueprint_graph::DataType::from_type_str(&v.var_type),
                default_value: v.default_value.clone(),
                description: v.description.clone(),
            })
            .collect();

        // Convert event defs to serializable format
        let local_events: Vec<formats::EventDefDescription> = self
            .local_event_defs
            .iter()
            .map(|def| formats::EventDefDescription {
                uid: def.uid.clone(),
                name: def.name.clone(),
                fields: def
                    .fields
                    .iter()
                    .map(|f| formats::EventFieldDescription {
                        name: f.name.clone(),
                        type_name: f.type_name.clone(),
                    })
                    .collect(),
                return_type: def.return_type.clone(),
            })
            .collect();

        let graph_view_states = self
            .open_tabs
            .iter()
            .map(|tab| {
                (
                    tab.id.clone(),
                    blueprint_graph::GraphViewState {
                        pan_offset_x: tab.graph.pan_offset.x,
                        pan_offset_y: tab.graph.pan_offset.y,
                        zoom: tab.graph.zoom_level,
                    },
                )
            })
            .collect();

        Ok(formats::BlueprintAsset {
            format_version: formats::current_format_version(),
            main_graph,
            subgraphs,
            local_events,
            variables,
            editor_state: Some(formats::BlueprintEditorState {
                open_tab_ids: self.open_tabs.iter().map(|tab| tab.id.clone()).collect(),
                active_tab_index: self.active_tab_index,
                graph_view_states,
            }),
            blueprint_metadata: self.blueprint_metadata.clone(),
        })
    }

    /// Load BlueprintAsset into the editor
    fn load_blueprint_asset(
        &mut self,
        asset: formats::BlueprintAsset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        // Check format version compatibility
        if !formats::is_version_supported(asset.format_version) {
            return Err(format!(
                "Unsupported blueprint format version: {}",
                asset.format_version
            ));
        }

        tracing::info!(
            ">>> load_blueprint_asset: format_version={}, main_graph has {} nodes, {} subgraphs, {} variables",
            asset.format_version,
            asset.main_graph.nodes.len(),
            asset.subgraphs.len(),
            asset.variables.len(),
        );

        let formats::BlueprintAsset {
            main_graph: graph_desc,
            subgraphs,
            local_events,
            variables,
            editor_state,
            format_version: _,
            blueprint_metadata,
        } = asset;

        self.blueprint_metadata = blueprint_metadata;

        self.subgraphs = subgraphs;

        // Convert event defs from serializable format
        self.local_event_defs = local_events
            .into_iter()
            .map(|ed| crate::core::graph::EventDefinition {
                uid: ed.uid,
                name: ed.name,
                fields: ed
                    .fields
                    .into_iter()
                    .map(|f| crate::core::graph::CustomEventField {
                        name: f.name,
                        type_name: f.type_name,
                    })
                    .collect(),
                return_type: ed.return_type,
            })
            .collect();

        // Convert ui::ClassVariable to local ClassVariable
        self.class_variables = variables
            .iter()
            .map(|v| crate::features::variables::ClassVariable {
                id: v.id.clone(),
                description: v.description.clone(),
                name: v.name.clone(),
                var_type: v.data_type.to_string(),
                default_value: v.default_value.clone(),
            })
            .collect();

        let main_graph = self.convert_graph_description_to_blueprint(&graph_desc, window, cx)?;
        tracing::info!(
            ">>> load_blueprint_asset: converted main graph: {} nodes, {} connections, {} comments",
            main_graph.nodes.len(),
            main_graph.connections.len(),
            main_graph.comments.len(),
        );

        self.comment_color_bindings_dirty = true;

        self.open_tabs = vec![GraphTab::new_main(main_graph)];
        self.active_tab_index = 0;

        if let Some(editor_state) = editor_state {
            for tab_id in &editor_state.open_tab_ids {
                if tab_id == "main" {
                    continue;
                }

                let macro_data = self
                    .subgraphs
                    .iter()
                    .find(|m| &m.id == tab_id)
                    .map(|m| (m.id.clone(), m.name.clone(), m.graph.clone()));

                if let Some((macro_id, macro_name, macro_graph_desc)) = macro_data {
                    if let Ok(mut macro_graph) =
                        self.convert_graph_description_to_blueprint(&macro_graph_desc, window, cx)
                    {
                        crate::features::macros::operations::normalize_subgraph_boundary(
                            &mut macro_graph,
                            crate::core::types::NodeType::MacroEntry,
                        );
                        crate::features::macros::operations::normalize_subgraph_boundary(
                            &mut macro_graph,
                            crate::core::types::NodeType::MacroExit,
                        );
                        if let Some(view_state) = editor_state.graph_view_states.get(tab_id) {
                            macro_graph.pan_offset =
                                Point::new(view_state.pan_offset_x, view_state.pan_offset_y);
                            macro_graph.zoom_level = view_state.zoom;
                        }

                        self.open_tabs.push(GraphTab::new_local_macro(
                            macro_id,
                            macro_name,
                            macro_graph,
                        ));
                    }
                }
            }

            if let Some(main_view) = editor_state.graph_view_states.get("main") {
                if let Some(main_tab) = self.open_tabs.get_mut(0) {
                    main_tab.graph.pan_offset =
                        Point::new(main_view.pan_offset_x, main_view.pan_offset_y);
                    main_tab.graph.zoom_level = main_view.zoom;
                }
            }

            self.active_tab_index = editor_state
                .active_tab_index
                .min(self.open_tabs.len().saturating_sub(1));

            self.comment_color_bindings_dirty = true;
        }

        tracing::info!(
            ">>> load_blueprint_asset: open_tabs now has {} tabs, active_tab_index={}",
            self.open_tabs.len(),
            self.active_tab_index,
        );
        for tab in &self.open_tabs {
            tracing::info!(
                ">>> load_blueprint_asset: tab id={} is_main={} nodes={} connections={}",
                tab.id,
                tab.is_main,
                tab.graph.nodes.len(),
                tab.graph.connections.len(),
            );
        }

        // Update self.graph shadow from the active tab.
        if let Some(tab) = self.open_tabs.get(self.active_tab_index) {
            self.graph = tab.graph.clone();
            tracing::info!(
                ">>> load_blueprint_asset: self.graph updated from active tab: {} nodes, {} connections",
                self.graph.nodes.len(),
                self.graph.connections.len(),
            );
        }

        // Rebuild workspace canvas panels from the freshly-loaded tabs.
        // For initial load the workspace doesn't exist yet — render() will call
        // initialize_workspace() which reads open_tabs directly.
        // For reload (workspace already exists) we rebuild panels in-place.
        self.graph_workspace_tabs_dirty = true;
        tracing::info!(">>> load_blueprint_asset: refreshing workspace tabs",);
        self.refresh_graph_workspace_tabs(window, cx);

        tracing::info!(
            ">>> load_blueprint_asset: done. graph_panels={}, self.graph.nodes={}",
            self.graph_panels.len(),
            self.graph.nodes.len(),
        );

        cx.notify();
        Ok(())
    }

    /// Autosave - called periodically to save work in progress
    pub fn autosave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(
            ">>> autosave: is_dirty={}, current_class_path={:?}, self.graph.nodes={}",
            self.is_dirty,
            self.current_class_path,
            self.graph.nodes.len(),
        );

        if !self.is_dirty {
            return; // No changes to save
        }

        if let Some(path) = self.get_graph_file_path() {
            // Create autosave path (same location with .autosave extension)
            let autosave_path = path.with_extension("blueprint.autosave");

            match self.save_to_path(&autosave_path, window, cx) {
                Ok(()) => {
                    tracing::info!(">>> autosave: SUCCESS saved to {:?}", autosave_path);
                }
                Err(e) => {
                    tracing::error!(">>> autosave: FAILED: {}", e);
                }
            }
        } else {
            tracing::warn!(">>> autosave: no file path set");
        }
    }

    /// Check if an autosave file exists for the current path
    pub fn has_autosave(&self) -> bool {
        if let Some(path) = self.get_graph_file_path() {
            let autosave_path = path.with_extension("blueprint.autosave");
            autosave_path.exists()
        } else {
            false
        }
    }

    /// Load from autosave file (recovery)
    pub fn load_autosave(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(path) = self.get_graph_file_path() {
            let autosave_path = path.with_extension("blueprint.autosave");
            self.load_from_path(&autosave_path, window, cx)?;

            // Delete autosave after successful recovery
            std::fs::remove_file(&autosave_path)
                .map_err(|e| format!("Failed to delete autosave file: {}", e))?;

            Ok(())
        } else {
            Err("No file path set - cannot load autosave".to_string())
        }
    }

    /// Mark the blueprint as dirty (has unsaved changes)
    pub fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        // Mark current tab as dirty
        if let Some(tab) = self.open_tabs.get_mut(self.active_tab_index) {
            tab.is_dirty = true;
        }

        // Update panel dirty flag
        if !self.is_dirty {
            self.is_dirty = true;
            cx.notify();
        }
    }

    /// Sync panel dirty flag with tab dirty flags
    /// Call this to update panel.is_dirty based on whether any tabs are dirty
    pub fn sync_dirty_flag(&mut self, cx: &mut Context<Self>) {
        let any_tab_dirty = self.open_tabs.iter().any(|tab| tab.is_dirty);

        if self.is_dirty != any_tab_dirty {
            self.is_dirty = any_tab_dirty;
            cx.notify();
        }
    }

    /// Export blueprint to a different format or location
    pub fn export_blueprint(
        &mut self,
        export_path: &Path,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        tracing::info!(
            ">>> export_blueprint: export_path={:?}, graph_panels={}, open_tabs={}, self.graph.nodes={}",
            export_path,
            self.graph_panels.len(),
            self.open_tabs.len(),
            self.graph.nodes.len(),
        );

        self.sync_all_canvases_to_tabs(cx);
        let asset = self.to_blueprint_asset()?;
        let content = formats::serialize_blueprint_with_header(&asset)?;

        std::fs::write(export_path, &content)
            .map_err(|e| format!("Failed to export blueprint: {}", e))?;

        tracing::info!(
            ">>> export_blueprint: wrote {} bytes to {:?}",
            content.len(),
            export_path
        );
        Ok(())
    }
}
/// Utility functions for file path handling
impl BlueprintEditorPanel {
    fn resolve_blueprint_path(path: &Path) -> PathBuf {
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(GRAPH_SAVE_FILE_NAME))
        {
            return path.to_path_buf();
        }

        if path.extension().is_none() {
            return path.join(GRAPH_SAVE_FILE_NAME);
        }

        path.to_path_buf()
    }

    pub(crate) fn get_graph_file_path(&self) -> Option<PathBuf> {
        self.current_class_path
            .as_ref()
            .map(|class_path| class_path.join(GRAPH_SAVE_FILE_NAME))
    }

    /// Get the display name for the current blueprint
    pub fn get_display_name(&self) -> String {
        if let Some(title) = &self.tab_title {
            return title.clone();
        }

        if let Some(path) = &self.current_class_path {
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string()
        } else {
            "Untitled Blueprint".to_string()
        }
    }

    /// Get the full path as a string
    pub fn get_path_string(&self) -> Option<String> {
        self.get_graph_file_path()
            .as_deref()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
    }

    /// Set the current file path
    pub fn set_path(&mut self, path: PathBuf) {
        let class_path = if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(GRAPH_SAVE_FILE_NAME))
        {
            path.parent().unwrap_or(&path).to_path_buf()
        } else {
            path
        };

        self.tab_title = class_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string());
        self.current_class_path = Some(class_path);
    }
}
