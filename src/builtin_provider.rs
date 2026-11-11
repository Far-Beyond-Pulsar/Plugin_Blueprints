//! The Blueprint editor as a built-in editor provider.
//!
//! Built into a binary (feature `builtin`) the plugin registers itself
//! through its root's `build` with `plugin_manager::PluginEditorProvider` (offered only
//! while the plugin is enabled for the project), exactly like
//! any other editor that is part of a build: the editor shell does not
//! depend on this crate, and a build without it simply has no Blueprint
//! editor and no Blueprint scripting language. All types, vtables and drop
//! glue live in the same binary, so there is no FFI boundary.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, AppContext, Window};
use plugin_editor_api::*;
use plugin_manager::{BuiltinEditorProvider, EditorContext, PluginEditorProvider};
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
        let undo = editor_context.services.undo.clone();
        panel.update(cx, |panel, cx| panel.set_undo_service(undo, cx));

        Ok(Arc::new(panel))
    }
}

/// Registers the editor provider, the trait implementer source and the
/// script language.
pub(crate) fn build(p: &mut pulsar_plugins::PluginBuilder) {
    p.add(PluginEditorProvider {
        origin: p.origin(),
        id: "com.pulsar.blueprint-editor",
        create: || Arc::new(BlueprintEditorBuiltinProvider),
    });

    // Blueprints are trait implementers: the project's trait index finds them
    // through this source (`features::traits::source`), and each binds to the
    // script class it compiles to.
    p.add(pulsar_traits::TraitImplementerSource {
        origin: p.origin(),
        kind: crate::features::traits::source::BLUEPRINT_KIND,
        label: "Blueprints",
        claims: Some(crate::features::traits::source::is_blueprint_asset),
        scan: Some(crate::features::traits::source::scan),
        list: None,
    });

    // The headless tools (`pulsar package`, CI) find languages through the same
    // registration, so they select this compiler by this id.
    p.add(plugin_editor_api::LinkedScriptLanguage {
        origin: p.origin(),
        id: crate::LANGUAGE_ID,
        create: crate::script_language,
    });
}
