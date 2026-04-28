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

impl BlueprintEditorPanel {
    /// Save the current blueprint to its file path
    pub fn plugin_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.current_class_path.clone() {
            match self.save_to_path(&path, window, cx) {
                Ok(()) => {
                    tracing::info!("Blueprint saved successfully to {:?}", path);
                    self.is_dirty = false;
                    cx.notify();
                }
                Err(e) => {
                    tracing::error!("Failed to save blueprint: {}", e);
                    self.compilation_status.state = CompilationState::Error;
                    self.compilation_status.message = format!("Save failed: {}", e);
                    cx.notify();
                }
            }
        } else {
            tracing::warn!("No save path set - cannot save blueprint");
            self.compilation_status.state = CompilationState::Error;
            self.compilation_status.message = "Save failed: no file path set".to_string();
            cx.notify();
        }
    }

    /// Reload the blueprint from its file path
    pub fn plugin_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.current_class_path.clone() {
            match self.load_from_path(&path, window, cx) {
                Ok(()) => {
                    tracing::info!("Blueprint reloaded successfully from {:?}", path);
                    self.is_dirty = false;
                    cx.notify();
                }
                Err(e) => {
                    tracing::error!("Failed to reload blueprint: {}", e);
                    self.compilation_status.state = CompilationState::Error;
                    self.compilation_status.message = format!("Reload failed: {}", e);
                    cx.notify();
                }
            }
        } else {
            tracing::warn!("No file path set - cannot reload blueprint");
        }
    }

    /// Save blueprint to a specific path
    pub fn save_to_path(
        &mut self,
        path: &Path,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Result<(), String> {
        // Convert current graph state to BlueprintAsset
        let asset = self.to_blueprint_asset()?;

        // Serialize to JSON with header
        let content = formats::serialize_blueprint_with_header(&asset)?;

        // Write to file
        std::fs::write(path, content).map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(())
    }

    /// Load blueprint from a specific path
    pub fn load_from_path(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        // Read file content
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

        // Try to deserialize as current format first
        let asset = match formats::deserialize_blueprint(&content) {
            Ok(asset) => asset,
            Err(_) => {
                // Try legacy format
                tracing::info!("Trying to load as legacy format...");
                let legacy_graph = legacy::try_parse_legacy_format(&content)?;

                // Convert legacy graph to current format
                formats::BlueprintAsset {
                    format_version: formats::current_format_version(),
                    main_graph: legacy_graph,
                    local_macros: Vec::new(),
                    variables: Vec::new(),
                    editor_state: None,
                    blueprint_metadata: Default::default(),
                }
            }
        };

        // Load the asset into the editor
        self.load_blueprint_asset(asset, window, cx)?;

        Ok(())
    }

    /// Convert current editor state to BlueprintAsset
    fn to_blueprint_asset(&self) -> Result<formats::BlueprintAsset, String> {
        let main_graph = self.convert_graph_to_description(&self.graph)?;

        // Convert local ClassVariable to ui::ClassVariable
        let variables: Vec<ui::graph::ClassVariable> = self
            .class_variables
            .iter()
            .enumerate()
            .map(|(i, v)| ui::graph::ClassVariable {
                id: format!("var_{}", i),
                name: v.name.clone(),
                data_type: ui::graph::DataType::from_type_str(&v.var_type),
                default_value: v.default_value.clone(),
                description: String::new(),
            })
            .collect();

        let graph_view_states = self
            .open_tabs
            .iter()
            .map(|tab| {
                (
                    tab.id.clone(),
                    ui::graph::GraphViewState {
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
            local_macros: self.local_macros.clone(),
            variables,
            editor_state: Some(formats::BlueprintEditorState {
                open_tab_ids: self.open_tabs.iter().map(|tab| tab.id.clone()).collect(),
                active_tab_index: self.active_tab_index,
                graph_view_states,
            }),
            blueprint_metadata: Default::default(),
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

        self.graph = self.convert_graph_description_to_blueprint(&asset.main_graph, window, cx)?;
        self.comment_color_bindings_dirty = true;
        self.local_macros = asset.local_macros;

        // Convert ui::ClassVariable to local ClassVariable
        self.class_variables = asset
            .variables
            .iter()
            .map(|v| crate::features::variables::ClassVariable {
                name: v.name.clone(),
                var_type: format!("{:?}", v.data_type),
                default_value: v.default_value.clone(),
            })
            .collect();

        self.open_tabs = vec![GraphTab::new_main(self.graph.clone())];
        self.active_tab_index = 0;

        if let Some(editor_state) = asset.editor_state {
            for tab_id in &editor_state.open_tab_ids {
                if tab_id == "main" {
                    continue;
                }

                let macro_data = self
                    .local_macros
                    .iter()
                    .find(|m| &m.id == tab_id)
                    .map(|m| (m.id.clone(), m.name.clone(), m.graph.clone()));

                if let Some((macro_id, macro_name, macro_graph_desc)) = macro_data {
                    if let Ok(mut macro_graph) =
                        self.convert_graph_description_to_blueprint(&macro_graph_desc, window, cx)
                    {
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
                self.graph.pan_offset = Point::new(main_view.pan_offset_x, main_view.pan_offset_y);
                self.graph.zoom_level = main_view.zoom;
                if let Some(main_tab) = self.open_tabs.get_mut(0) {
                    main_tab.graph.pan_offset = self.graph.pan_offset;
                    main_tab.graph.zoom_level = self.graph.zoom_level;
                }
            }

            self.active_tab_index = editor_state
                .active_tab_index
                .min(self.open_tabs.len().saturating_sub(1));

            if let Some(active_tab) = self.open_tabs.get(self.active_tab_index) {
                self.graph = active_tab.graph.clone();
                self.comment_color_bindings_dirty = true;
            }
        }

        cx.notify();
        Ok(())
    }

    /// Autosave - called periodically to save work in progress
    pub fn autosave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_dirty {
            return; // No changes to save
        }

        if let Some(path) = &self.current_class_path {
            // Create autosave path (same location with .autosave extension)
            let autosave_path = path.with_extension("blueprint.autosave");

            match self.save_to_path(&autosave_path, window, cx) {
                Ok(()) => {
                    tracing::debug!("Autosaved to {:?}", autosave_path);
                }
                Err(e) => {
                    tracing::error!("Autosave failed: {}", e);
                }
            }
        }
    }

    /// Check if an autosave file exists for the current path
    pub fn has_autosave(&self) -> bool {
        if let Some(path) = &self.current_class_path {
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
        if let Some(path) = &self.current_class_path {
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
        if !self.is_dirty {
            self.is_dirty = true;
            cx.notify();
        }
    }

    /// Export blueprint to a different format or location
    pub fn export_blueprint(
        &self,
        export_path: &Path,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let asset = self.to_blueprint_asset()?;
        let content = formats::serialize_blueprint_with_header(&asset)?;

        std::fs::write(export_path, content)
            .map_err(|e| format!("Failed to export blueprint: {}", e))?;

        tracing::info!("Blueprint exported to {:?}", export_path);
        Ok(())
    }
}

/// Utility functions for file path handling
impl BlueprintEditorPanel {
    /// Get the display name for the current blueprint
    pub fn get_display_name(&self) -> String {
        if let Some(path) = &self.current_class_path {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string()
        } else {
            "Untitled Blueprint".to_string()
        }
    }

    /// Get the full path as a string
    pub fn get_path_string(&self) -> Option<String> {
        self.current_class_path
            .as_ref()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
    }

    /// Set the current file path
    pub fn set_path(&mut self, path: PathBuf) {
        self.current_class_path = Some(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autosave_path_generation() {
        let path = PathBuf::from("/path/to/test.blueprint");
        let autosave_path = path.with_extension("blueprint.autosave");
        assert_eq!(
            autosave_path.to_str().unwrap(),
            "/path/to/test.blueprint.autosave"
        );
    }
}
