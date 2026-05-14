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
use std::fs;
use std::sync::Mutex;
use std::{path::Path, path::PathBuf, sync::Arc};
use ui::dock::PanelView;

// Module declarations
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
                "graph": {
                    "nodes": [],
                    "connections": [],
                    "comments": [],
                    "metadata": {
                        "version": "0.1.0"
                    }
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
        vec![
            AiToolDefinition::new(
                "blueprint_inspect_graph",
                "Inspect the blueprint graph file and return summary counts and metadata.",
                json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            )
            .with_category("analysis"),
            AiToolDefinition::new(
                "blueprint_set_graph_metadata",
                "Set graph.metadata[key] in the blueprint graph_save.json file.",
                json!({
                    "type": "object",
                    "properties": {
                        "key": { "type": "string" },
                        "value": {}
                    },
                    "required": ["key", "value"]
                }),
            )
            .with_category("editing"),
            AiToolDefinition::new(
                "blueprint_add_event_stub",
                "Create an event stub file under the blueprint's events folder.",
                json!({
                    "type": "object",
                    "properties": {
                        "event_name": { "type": "string" },
                        "overwrite": { "type": "boolean" }
                    },
                    "required": ["event_name"]
                }),
            )
            .with_category("generation"),
        ]
    }

    fn capabilities_for_file(&self, file_path: &Path) -> Vec<String> {
        if is_blueprint_file_or_folder(file_path) {
            self.ai_tools().into_iter().map(|tool| tool.name).collect()
        } else {
            Vec::new()
        }
    }

    fn execute_ai_tool(
        &self,
        file_path: &Path,
        tool_name: &str,
        tool_args: serde_json::Value,
    ) -> Result<serde_json::Value, PluginError> {
        let graph_path = resolve_blueprint_graph_path(file_path)?;

        match tool_name {
            "blueprint_inspect_graph" => {
                let raw = fs::read_to_string(&graph_path).map_err(|error| PluginError::FileLoadError {
                    path: graph_path.clone(),
                    message: error.to_string(),
                })?;
                let graph: serde_json::Value =
                    serde_json::from_str(&raw).map_err(|error| PluginError::InvalidFormat {
                        expected: "blueprint graph JSON".to_string(),
                        message: error.to_string(),
                    })?;

                let node_count = graph
                    .get("graph")
                    .and_then(|g| g.get("nodes"))
                    .and_then(|nodes| nodes.as_array())
                    .map(|nodes| nodes.len())
                    .unwrap_or(0);
                let connection_count = graph
                    .get("graph")
                    .and_then(|g| g.get("connections"))
                    .and_then(|nodes| nodes.as_array())
                    .map(|nodes| nodes.len())
                    .unwrap_or(0);
                let comment_count = graph
                    .get("graph")
                    .and_then(|g| g.get("comments"))
                    .and_then(|nodes| nodes.as_array())
                    .map(|nodes| nodes.len())
                    .unwrap_or(0);

                Ok(json!({
                    "ok": true,
                    "graph_path": graph_path.display().to_string(),
                    "summary": {
                        "node_count": node_count,
                        "connection_count": connection_count,
                        "comment_count": comment_count
                    },
                    "metadata": graph
                        .get("graph")
                        .and_then(|g| g.get("metadata"))
                        .cloned()
                        .unwrap_or(json!({})),
                }))
            }
            "blueprint_set_graph_metadata" => {
                let key = tool_args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .ok_or_else(|| PluginError::Other {
                        message: "Missing required parameter: key".to_string(),
                    })?;
                let value = tool_args.get("value").cloned().ok_or_else(|| PluginError::Other {
                    message: "Missing required parameter: value".to_string(),
                })?;

                let raw = fs::read_to_string(&graph_path).map_err(|error| PluginError::FileLoadError {
                    path: graph_path.clone(),
                    message: error.to_string(),
                })?;
                let mut graph: serde_json::Value =
                    serde_json::from_str(&raw).map_err(|error| PluginError::InvalidFormat {
                        expected: "blueprint graph JSON".to_string(),
                        message: error.to_string(),
                    })?;

                if !graph.get("graph").is_some_and(|g| g.is_object()) {
                    graph["graph"] = json!({});
                }
                if !graph["graph"].get("metadata").is_some_and(|m| m.is_object()) {
                    graph["graph"]["metadata"] = json!({});
                }
                graph["graph"]["metadata"][key] = value.clone();

                let serialized = serde_json::to_string_pretty(&graph).map_err(|error| PluginError::Other {
                    message: error.to_string(),
                })?;
                fs::write(&graph_path, serialized).map_err(|error| PluginError::FileSaveError {
                    path: graph_path.clone(),
                    message: error.to_string(),
                })?;

                Ok(json!({
                    "ok": true,
                    "graph_path": graph_path.display().to_string(),
                    "updated": { key: value }
                }))
            }
            "blueprint_add_event_stub" => {
                let event_name = tool_args
                    .get("event_name")
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .ok_or_else(|| PluginError::Other {
                        message: "Missing required parameter: event_name".to_string(),
                    })?;
                let overwrite = tool_args
                    .get("overwrite")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let blueprint_root = graph_path.parent().ok_or_else(|| PluginError::Other {
                    message: "Could not determine blueprint root directory".to_string(),
                })?;
                let events_dir = blueprint_root.join("events");
                fs::create_dir_all(&events_dir).map_err(|error| PluginError::FileSaveError {
                    path: events_dir.clone(),
                    message: error.to_string(),
                })?;

                let sanitized_name = sanitize_event_name(event_name);
                let event_file = events_dir.join(format!("{}.rs", sanitized_name));
                if event_file.exists() && !overwrite {
                    return Err(PluginError::Other {
                        message: format!(
                            "Event file already exists: {} (set overwrite=true to replace)",
                            event_file.display()
                        ),
                    });
                }

                let source = format!(
                    "// Auto-generated by Blueprint AI tooling\\n\\
pub fn {name}() {{\\n    // TODO: implement event logic\\n}}\\n",
                    name = sanitized_name
                );

                fs::write(&event_file, source).map_err(|error| PluginError::FileSaveError {
                    path: event_file.clone(),
                    message: error.to_string(),
                })?;

                Ok(json!({
                    "ok": true,
                    "event_name": sanitized_name,
                    "event_file": event_file.display().to_string(),
                    "graph_path": graph_path.display().to_string(),
                }))
            }
            _ => Err(PluginError::Other {
                message: format!("Unknown blueprint tool: {}", tool_name),
            }),
        }
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
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("Failed to create blueprint panel: {}", e);
                        // Return a default panel on error
                        BlueprintEditorPanel::new(window, cx)
                    }
                }
            });

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

fn is_blueprint_file_or_folder(file_path: &Path) -> bool {
    if file_path.is_dir() && file_path.join("graph_save.json").exists() {
        return true;
    }

    if file_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "graph_save.json")
    {
        return true;
    }

    file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "class")
}

fn resolve_blueprint_graph_path(file_path: &Path) -> Result<PathBuf, PluginError> {
    let mut candidates = Vec::new();

    if file_path.is_dir() {
        candidates.push(file_path.join("graph_save.json"));
    }

    if file_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "graph_save.json")
    {
        candidates.push(file_path.to_path_buf());
    }

    if file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "class")
    {
        candidates.push(file_path.join("graph_save.json"));
    }

    if let Some(parent) = file_path.parent() {
        candidates.push(parent.join("graph_save.json"));
    }

    if let Some(existing) = candidates.iter().find(|candidate| candidate.exists()) {
        return Ok(existing.clone());
    }

    candidates.into_iter().next().ok_or_else(|| PluginError::Other {
        message: format!(
            "Could not resolve graph_save.json for path {}",
            file_path.display()
        ),
    })
}

fn sanitize_event_name(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        let mapped = if ch.is_ascii_alphanumeric() || ch == '_' {
            ch
        } else {
            '_'
        };
        out.push(mapped);
    }

    if out.is_empty() {
        "event_stub".to_string()
    } else if out
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        format!("event_{}", out)
    } else {
        out
    }
}

// Export the plugin using the provided macro
//export_plugin!(BlueprintEditorPlugin);
