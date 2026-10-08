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
        vec![crate::blueprint_file_type()]
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
        editor_context: &EditorContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Arc<dyn PanelView>, PluginError> {
        let panel = cx.new(|cx| {
            match crate::BlueprintEditorPanel::new_with_path(file_path.clone(), window, cx) {
                Ok(panel) => panel,
                Err(e) => {
                    tracing::error!("Failed to create blueprint panel: {}", e);
                    crate::BlueprintEditorPanel::new_with_load_error(
                        file_path.clone(),
                        e.to_string(),
                        window,
                        cx,
                    )
                }
            }
        });
        if let Some(project_root) = editor_context.project_root.clone() {
            panel.update(cx, |panel, _cx| {
                panel.project_root = Some(project_root);
            });
        }

        if panel.read(cx).load_error.is_none() {
            panel.update(cx, |panel, cx| {
                panel.attach_ai_graph_updates(file_path.clone(), cx);
            });
        }

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
