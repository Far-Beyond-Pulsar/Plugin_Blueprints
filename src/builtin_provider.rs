//! The Blueprint editor as a built-in editor provider.
//!
//! Built into a binary (feature `builtin`) the plugin registers itself at
//! link time through `plugin_manager::LinkedEditorProvider`, exactly like
//! any other editor that is part of a build: the editor shell does not
//! depend on this crate, and a build without it simply has no Blueprint
//! editor and no Blueprint scripting language. All types, vtables and drop
//! glue live in the same binary, so there is no FFI boundary.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, AppContext, Window};
use plugin_editor_api::*;
use plugin_manager::{BuiltinEditorProvider, EditorContext, LinkedEditorProvider};
use ui::dock::PanelView;

pub struct BlueprintEditorBuiltinProvider;

impl BuiltinEditorProvider for BlueprintEditorBuiltinProvider {
    fn provider_id(&self) -> &str {
        "com.pulsar.blueprint-editor"
    }

    fn file_types(&self) -> Vec<FileTypeDefinition> {
        use serde_json::json;

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

    fn can_handle(&self, editor_id: &EditorId) -> bool {
        editor_id.as_str() == "blueprint-editor"
    }

    fn ai_tools(&self) -> Vec<AiToolDefinition> {
        crate::BlueprintEditorPlugin::default().ai_tools()
    }

    fn script_languages(&self) -> Vec<Arc<dyn ScriptLanguage>> {
        vec![crate::script_language()]
    }

    fn capabilities_for_file(&self, file_path: &Path) -> Vec<String> {
        crate::BlueprintEditorPlugin::default().capabilities_for_file(file_path)
    }

    fn execute_ai_tool(
        &self,
        file_path: &Path,
        tool_name: &str,
        tool_args: JsonValue,
    ) -> Result<JsonValue, PluginError> {
        crate::execute_compiled_tool(file_path, tool_name, tool_args)
    }

    fn create_editor(
        &self,
        file_path: PathBuf,
        _editor_context: &EditorContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Arc<dyn PanelView>, PluginError> {
        let panel = cx.new(|cx| {
            match crate::BlueprintEditorPanel::new_with_path(file_path.clone(), window, cx) {
                Ok(panel) => panel,
                Err(e) => {
                    tracing::error!("Failed to create blueprint panel: {}", e);
                    crate::BlueprintEditorPanel::new(window, cx)
                }
            }
        });

        // Keep plugin AI tools aligned with the currently opened blueprint panel state.
        let graph_snapshot = panel.read(cx).graph.clone();
        crate::upsert_ai_session(file_path.clone(), graph_snapshot);

        Ok(Arc::new(panel))
    }
}

plugin_manager::inventory::submit! {
    LinkedEditorProvider { create: || Arc::new(BlueprintEditorBuiltinProvider) }
}

// The headless tools (`pulsar package`, CI) find languages through the same
// link-time registration, so they select this compiler by this id.
plugin_editor_api::inventory::submit! {
    plugin_editor_api::LinkedScriptLanguage { create: crate::script_language }
}
