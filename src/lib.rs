#![recursion_limit = "512"]

//! # Blueprint Editor Plugin
//!
//! Visual scripting editor for creating blueprint classes through node-based programming.
//!
//! ## Architecture
//!
//! The plugin is organized into several modules:
//!
//! - **core**: Core data types (BlueprintNode, BlueprintGraph, Connection, etc.)
//! - **editor**: Main editor state container and lifecycle management
//! - **features**: Feature modules (nodes, connections, comments, variables, macros, viewport, compilation)
//! - **rendering**: Visual rendering layer (graph canvas, input handling, styling)
//! - **ui**: Reusable UI components and panels
//! - **io**: File I/O and persistence

use gpui::*;
use plugin_editor_api::*;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;
use std::{path::PathBuf, sync::Arc};
use ui::dock::PanelView;

// Module declarations
mod ai_tools;
mod core;
mod editor;
mod features;
mod io;
mod rendering;
mod ui_components;

// Re-export main types for plugin API compatibility
pub use core::definitions::*;
pub use core::events::*;
pub use core::graph::*;
pub use core::types::*;
pub use editor::panel::BlueprintEditorPanel;

pub fn upsert_ai_session(file_path: PathBuf, graph: BlueprintGraph) {
    ai_tools::upsert_session(file_path, graph);
}

pub fn execute_compiled_tool(
    file_path: &std::path::Path,
    tool_name: &str,
    tool_args: serde_json::Value,
) -> Result<serde_json::Value, PluginError> {
    ai_tools::execute_compiled_tool(file_path, tool_name, tool_args)
}

/// Storage for editor instances owned by the plugin
struct EditorStorage {
    panel: Arc<dyn ui::dock::PanelView>,
}

/// The Blueprint Editor Plugin
pub struct BlueprintEditorPlugin {
    /// CRITICAL: Plugin owns ALL editor instances to prevent memory leaks!
    /// The main app only gets raw pointers - it NEVER owns the Arc or Box.
    editors: Arc<Mutex<HashMap<usize, EditorStorage>>>,
    next_editor_id: Arc<Mutex<usize>>,
}

impl Default for BlueprintEditorPlugin {
    fn default() -> Self {
        Self {
            editors: Arc::new(Mutex::new(HashMap::new())),
            next_editor_id: Arc::new(Mutex::new(0)),
        }
    }
}

impl EditorPlugin for BlueprintEditorPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: PluginId::new("com.pulsar.blueprint-editor"),
            name: "Blueprint Editor".into(),
            version: "0.1.0".into(),
            author: "Pulsar Team".into(),
            description: "Visual scripting editor for creating blueprint classes".into(),
        }
    }

    fn file_types(&self) -> Vec<FileTypeDefinition> {
        vec![FileTypeDefinition {
            id: FileTypeId::new("class"),
            extension: "class".to_string(),
            display_name: "Blueprint Class".to_string(),
            icon: ui::IconName::Component,
            color: gpui::rgb(0x9C27B0).into(),
            structure: FileStructure::FolderBased {
                marker_file: "graph_save.json".to_string(),
                template_structure: vec![PathTemplate::Folder {
                    path: "events".into(),
                }],
            },
            default_content: json!({
                "format_version": 1,
                "main_graph": {
                    "nodes": {},
                    "connections": [],
                    "metadata": {
                        "name": "EventGraph",
                        "description": "",
                        "version": "1.0.0",
                        "created_at": "2024-01-01T00:00:00+00:00",
                        "modified_at": "2024-01-01T00:00:00+00:00"
                    },
                    "comments": []
                },
                "local_macros": [],
                "variables": [],
                "blueprint_metadata": {
                    "blueprint_type": "Generic",
                    "parent_class": null,
                    "description": "",
                    "category": "Uncategorized",
                    "tags": []
                }
            }),
            categories: vec!["Blueprints".to_string()],
        }]
    }

    fn editors(&self) -> Vec<EditorMetadata> {
        vec![EditorMetadata {
            id: EditorId::new("blueprint-editor"),
            display_name: "Blueprint Editor".into(),
            supported_file_types: vec![FileTypeId::new("class")],
        }]
    }

    fn ai_tools(&self) -> Vec<AiToolDefinition> {
        ai_tools::ai_tools()
    }

    fn capabilities_for_file(&self, file_path: &std::path::Path) -> Vec<String> {
        ai_tools::capabilities_for_file(file_path)
    }

    fn execute_ai_tool(
        &self,
        file_path: &std::path::Path,
        tool_name: &str,
        tool_args: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError> {
        ai_tools::execute_ai_tool(file_path, tool_name, tool_args)
    }

    fn create_editor(
        &self,
        editor_id: EditorId,
        file_path: PathBuf,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Arc<dyn PanelView>, PluginError> {
        log::info!("Creating blueprint editor with ID: {}", editor_id.as_str());

        if editor_id.as_str() == "blueprint-editor" {
            let file_path_clone = file_path.clone();

            // Create a view context for the panel
            let panel = cx.new(|cx| {
                match BlueprintEditorPanel::new_with_path(file_path_clone.clone(), window, cx) {
                    Ok(p) => {
                        tracing::info!(
                            ">>> create_editor: new_with_path succeeded, graph has {} nodes, current_class_path={:?}",
                            p.graph.nodes.len(),
                            p.current_class_path,
                        );
                        p
                    }
                    Err(e) => {
                        tracing::error!(">>> create_editor: new_with_path FAILED: {}", e);
                        // Return a default panel on error
                        let p = BlueprintEditorPanel::new(window, cx);
                        tracing::warn!(
                            ">>> create_editor: fell back to empty panel, graph has {} nodes",
                            p.graph.nodes.len(),
                        );
                        p
                    }
                }
            });

            // Register/refresh AI session state from the live editor graph.
            let graph_snapshot = panel.read(cx).graph.clone();
            tracing::info!(
                ">>> create_editor: AI session snapshot has {} nodes",
                graph_snapshot.nodes.len(),
            );
            ai_tools::upsert_session(file_path.clone(), graph_snapshot);

            // Wrap the panel in Arc - will be shared with main app
            let panel_arc: Arc<dyn ui::dock::PanelView> = Arc::new(panel.clone());

            // Generate unique ID for this editor
            let id = {
                let mut next_id = self.next_editor_id.lock().unwrap();
                let id = *next_id;
                *next_id += 1;
                id
            };

            // CRITICAL: Store Arc in plugin's HashMap to keep it alive!
            self.editors.lock().unwrap().insert(
                id,
                EditorStorage {
                    panel: panel_arc.clone(),
                },
            );

            log::info!(
                "Created blueprint editor instance {} for {:?}",
                id,
                file_path
            );

            Ok(panel_arc)
        } else {
            Err(PluginError::EditorNotFound { editor_id })
        }
    }

    fn on_load(&mut self) {
        log::info!("Blueprint Editor Plugin loaded");
    }
}

// Export the plugin using the provided macro
export_plugin!(BlueprintEditorPlugin);
