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
use std::{path::PathBuf, sync::Arc};
use ui::dock::PanelView;

// Module declarations
mod ai_tools;
pub mod canvas_profile;
mod core;
mod editor;
mod examples;
mod features;
pub mod io;
mod palette_catalog;
pub mod std_nodes;
mod theme;
mod ui_components;
pub mod validation;

// Re-export main types for plugin API compatibility
pub use core::definitions::*;
pub use core::events::*;
pub use core::graph::{CustomEventField, EventDefinition};
pub use core::types::*;
pub use editor::panel::BlueprintEditorPanel;
pub use features::viewport::parse_hex_color;

#[cfg(feature = "builtin")]
mod builtin_provider;
#[cfg(feature = "builtin")]
pub use builtin_provider::BlueprintEditorBuiltinProvider;

pub fn upsert_ai_session(file_path: PathBuf, graph: graphy::GraphDescription) {
    ai_tools::upsert_session(file_path, graph);
}

pub fn execute_compiled_tool(
    file_path: &std::path::Path,
    tool_name: &str,
    tool_args: serde_json::Value,
) -> Result<serde_json::Value, PluginError> {
    ai_tools::execute_compiled_tool(file_path, tool_name, tool_args)
}

/// Shared `.class` file metadata for the dynamic and built-in editor providers.
pub(crate) fn blueprint_file_type() -> FileTypeDefinition {
    FileTypeDefinition {
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
            "format_version": 2,
            "main_graph": {
                "nodes": {},
                "connections": [],
                "metadata": {
                    "name": "EventGraph",
                    "description": "",
                    "version": "1.0.0",
                    "created_at": plugin_editor_api::CREATION_TIMESTAMP_PLACEHOLDER,
                    "modified_at": plugin_editor_api::CREATION_TIMESTAMP_PLACEHOLDER
                },
                "comments": []
            },
            "subgraphs": [],
            "variables": [],
            "blueprint_metadata": {
                "blueprint_type": "Generic",
                "parent_class": null,
                "description": "",
                "category": "Uncategorized",
                "tags": []
            }
        }),
        creation_directory: None,
        cook: plugin_editor_api::file_types::copy_file_for_cook,
        categories: vec!["Blueprints".to_string()],
    }
}

/// The Blueprint Editor Plugin
#[derive(Default)]
pub struct BlueprintEditorPlugin;

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
        vec![blueprint_file_type()]
    }

    fn editors(&self) -> Vec<EditorMetadata> {
        vec![EditorMetadata {
            id: EditorId::new("blueprint-editor"),
            display_name: "Blueprint Editor".into(),
            supported_file_types: vec![FileTypeId::new("class")],
        }]
    }

    fn on_load(&mut self) {
        log::info!("Blueprint Editor Plugin loaded");
    }
}

impl BlueprintEditorPlugin {
    fn create_blueprint_editor(
        &'static self,
        file_path: PathBuf,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Arc<dyn PanelView>, PluginError> {
        log::info!("Creating blueprint editor for {:?}", file_path);

        let file_path_clone = file_path.clone();

        let panel = cx.new(|cx| {
            match BlueprintEditorPanel::new_with_path(file_path_clone.clone(), window, cx) {
                Ok(p) => {
                    tracing::info!(
                        "create_blueprint_editor: new_with_path succeeded, current_class_path={:?}",
                        p.current_class_path,
                    );
                    p
                }
                Err(e) => {
                    tracing::error!("create_blueprint_editor: new_with_path FAILED: {}", e);
                    BlueprintEditorPanel::new_with_load_error(
                        file_path_clone.clone(),
                        e.to_string(),
                        window,
                        cx,
                    )
                }
            }
        });

        log::info!("Created blueprint editor for {:?}", file_path);

        Ok(Arc::new(panel))
    }
}

impl EditorPluginEditor for BlueprintEditorPlugin {
    fn register_editors(&'static self, registry: &mut EditorFactoryRegistry) {
        registry.register_fn(
            EditorId::new("blueprint-editor"),
            |file_path, window, cx| self.create_blueprint_editor(file_path, window, cx),
        );
    }
}

impl EditorPluginStatusbar for BlueprintEditorPlugin {}
impl EditorPluginAi for BlueprintEditorPlugin {
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
}
impl EditorPluginComponents for BlueprintEditorPlugin {
    fn component_definitions(&self) -> Vec<ComponentDefinition> {
        Vec::new()
    }
}
/// Blueprints as a scripting language: classes compile to engine script
/// modules (see `blueprint_compiler`); before Play the editor validates
/// every saved class graph through this.
pub struct BlueprintLanguage;

impl ScriptLanguage for BlueprintLanguage {
    /// A class's `compile_target` key of its saved graph: `"rust"` generates
    /// the class into the game crate.
    fn compile_target(&self, class_dir: &std::path::Path) -> Result<Option<String>, String> {
        #[derive(serde::Deserialize)]
        struct Graph {
            #[serde(default)]
            compile_target: Option<String>,
        }
        let graph = class_dir.join(blueprint_compiler::GRAPH_FILE);
        let Ok(text) = engine_fs::virtual_fs::read_to_string(&graph) else {
            return Ok(None);
        };
        serde_json::from_str::<Graph>(&text)
            .map(|graph| graph.compile_target)
            .map_err(|error| format!("{}: {error}", graph.display()))
    }

    fn validate_project(&self, project_root: &std::path::Path) -> Result<(), String> {
        validation::validate_project_classes(project_root)
    }

    fn validate_project_with_component_events(
        &self,
        project_root: &std::path::Path,
        component_events: &[plugin_editor_api::ComponentEventMetadata],
    ) -> Result<(), String> {
        validation::validate_project_classes_with_component_events(project_root, component_events)
    }

    fn compile_project(
        &self,
        project_root: &std::path::Path,
        natives: &plugin_editor_api::NativeRegistry,
    ) -> Vec<plugin_editor_api::CompileDiagnostic> {
        validation::compile_project_classes(project_root, natives)
    }

    fn compile_project_with_component_events(
        &self,
        project_root: &std::path::Path,
        natives: &plugin_editor_api::NativeRegistry,
        component_events: &[plugin_editor_api::ComponentEventMetadata],
    ) -> Vec<plugin_editor_api::CompileDiagnostic> {
        validation::compile_project_classes_with_component_events(
            project_root,
            natives,
            component_events,
        )
    }
}

/// The Blueprint scripting language, for hosts that embed this plugin.
/// The Blueprint language's id.
pub const LANGUAGE_ID: &str = "blueprint";

pub fn script_language() -> Arc<dyn ScriptLanguage> {
    Arc::new(BlueprintLanguage)
}

/// Blueprints as a language: one registration with everything the host needs
/// (identity, the marker file of its classes, the graph compiler). The host
/// routes to it through `pulsar_language::LanguageRegistry`; a project that
/// disables this plugin has no Blueprint language, markers or compiler.
pub const LANGUAGE: pulsar_language::Language = pulsar_language::Language {
    origin: pulsar_plugins::origin!(),
    id: LANGUAGE_ID,
    display_name: "Blueprints",
    file_extensions: &[],
    language_ids: &[],
    highlight_id: "",
    class_markers: &[blueprint_compiler::GRAPH_FILE],
    config_files: &[],
    compiler: Some(script_language),
    server: None,
};

// Static built-ins are instantiated through the Rust API and must not emit the
// process-global dynamic loader symbols shared by every plugin.
#[cfg(not(feature = "builtin"))]
export_plugin!(BlueprintEditorPlugin);

// This plugin's descriptor (`pulsar-plugin.toml`): linking the crate installs it.
pulsar_plugins::installed!("pulsar-plugin.toml", build);

/// Everything the plugin registers (its language, its folder of example Blueprints, and the
/// editor provider with the `builtin` feature).
pub fn build(p: &mut pulsar_plugins::PluginBuilder) {
    // The icon the editor shows beside this plugin (Extensions window).
    p.add(pulsar_plugins::PluginIcon {
        origin: p.origin(),
        format: pulsar_plugins::IconFormat::Png,
        bytes: include_bytes!("../icon.png"),
    });
    // The plugin's folder: the example Blueprints (`examples/`), read-only.
    p.add(engine_fs::FileProvider {
        origin: p.origin(),
        open: examples::open,
        in_game: engine_fs::InGame::Snapshot,
    });
    // Its one language registration: identity, class marker, compiler.
    p.add(LANGUAGE);
    #[cfg(feature = "builtin")]
    builtin_provider::build(p);
}

/// The embedded example files as `(path inside the plugin folder, bytes)`:
/// `examples/README.md`, `examples/basics/<Name>.class/graph_save.json`, ...
/// For tests and tools that load the examples without mounting the folder.
pub fn example_files() -> Vec<(&'static str, &'static [u8])> {
    examples::embedded()
        .iter()
        .map(|file| (file.path, file.bytes))
        .collect()
}

// The plugin as a library of its own, loaded by the editor from `plugins/editor`
// (`cargo build -p <this package> --release --features dll`).
#[cfg(feature = "dll")]
plugin_editor_api::export_plugin_build!();
