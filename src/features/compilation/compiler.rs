//! Compiler - Compile blueprints to engine script modules or Rust code

use crate::editor::panel::{BlueprintEditorPanel, CompilationHistoryEntry};
use crate::{CompilationState, CompilationStatus};
use gpui::*;
use std::collections::HashMap;
use std::path::PathBuf;

// ── Property normalisation (shared by both compile paths) ────────────────────

/// Normalizes property literals that may be JSON-string-encoded one or more
/// times by the editor/serialization path.
///
/// Example: `"\"2\""` -> `2`
/// Every native the engine registers (pulsar_std, world components,
/// reflected methods), built once: what compiled modules link against.
pub(crate) fn script_natives() -> &'static pulsar_script_vm::NativeRegistry {
    static NATIVES: std::sync::OnceLock<pulsar_script_vm::NativeRegistry> =
        std::sync::OnceLock::new();
    NATIVES.get_or_init(pulsar_script_vm::NativeRegistry::with_engine_natives)
}

pub(crate) use blueprint_compiler::authored::property_value_from_raw;

// ── BlueprintEditorPanel helpers ──────────────────────────────────────────────

impl BlueprintEditorPanel {
    fn push_compilation_history(
        &mut self,
        state: CompilationState,
        stage: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        const MAX_HISTORY_ENTRIES: usize = 2000;

        let now = chrono::Local::now();
        self.compilation_history.push(CompilationHistoryEntry {
            timestamp: now.format("%H:%M:%S").to_string(),
            state,
            stage: stage.into(),
            message: message.into(),
            detail,
        });

        if self.compilation_history.len() > MAX_HISTORY_ENTRIES {
            let overflow = self.compilation_history.len() - MAX_HISTORY_ENTRIES;
            self.compilation_history.drain(0..overflow);
        }
    }

    /// The main event-graph tab — compilation always targets this graph, never
    /// whatever tab the user happens to have focused (e.g. a macro/subgraph
    /// tab, which legitimately has no event nodes and would otherwise trip the
    /// "No event nodes found in graph" check).
    fn main_graph_tab(&self) -> &crate::editor::tabs::GraphTab {
        self.open_tabs
            .iter()
            .find(|t| t.is_main)
            .unwrap_or(&self.open_tabs[0])
    }

    /// Dump the active graph (editor view + the `graphy::GraphDescription` that gets
    /// sent to the compiler) to `blueprint_graph_debug.json` in the working
    /// directory, so event-node detection mismatches can be diagnosed by
    /// comparing the editor's classification against PBGC's metadata lookup.
    fn dump_graph_debug_info(&self, graph: &graphy::GraphDescription) {
        #[derive(serde::Serialize)]
        struct EditorNodeDebug {
            id: String,
            definition_id: String,
            title: String,
            editor_node_type: String,
            definition_is_event: Option<bool>,
            metadata_found: bool,
            metadata_node_type: Option<String>,
        }

        #[derive(serde::Serialize)]
        struct GraphDescNodeDebug {
            id: String,
            node_type: String,
        }

        #[derive(serde::Serialize)]
        struct GraphDebugDump {
            active_tab: String,
            editor_nodes: Vec<EditorNodeDebug>,
            graph_description_nodes: Vec<GraphDescNodeDebug>,
        }

        let node_definitions = crate::core::definitions::NodeDefinitions::load();
        let metadata = crate::core::definitions::extract_canonical_node_metadata();
        let main_tab = self.main_graph_tab();

        let editor_nodes = main_tab
            .graph
            .nodes
            .iter()
            .map(|n| {
                let def = node_definitions.get_node_definition(&n.definition_id);
                let meta = metadata.get(&n.definition_id);
                EditorNodeDebug {
                    id: n.id.clone(),
                    definition_id: n.definition_id.clone(),
                    title: n.title.clone(),
                    editor_node_type: format!("{:?}", n.node_type),
                    definition_is_event: def.map(|d| d.is_event),
                    metadata_found: meta.is_some(),
                    metadata_node_type: meta.map(|m| format!("{:?}", m.node_type)),
                }
            })
            .collect();

        let graph_description_nodes = graph
            .nodes
            .values()
            .map(|n| GraphDescNodeDebug {
                id: n.id.clone(),
                node_type: n.node_type.clone(),
            })
            .collect();

        let dump = GraphDebugDump {
            active_tab: main_tab.name.clone(),
            editor_nodes,
            graph_description_nodes,
        };

        match serde_json::to_string_pretty(&dump) {
            Ok(json) => match std::fs::write("blueprint_graph_debug.json", &json) {
                Ok(()) => tracing::info!(
                    "[PBGC debug] Wrote graph snapshot to ./blueprint_graph_debug.json"
                ),
                Err(e) => tracing::warn!("[PBGC debug] Failed to write graph debug dump: {}", e),
            },
            Err(e) => tracing::warn!("[PBGC debug] Failed to serialize graph debug dump: {}", e),
        }
    }

    /// Build a `graphy::GraphDescription` for the whole blueprint file: the main
    /// event graph with every subgraph call node (typed in new saves and
    /// accepted from legacy `macro:<id>` saves) inlined via graphy's
    /// `SubGraphExpander`, using a library assembled from this file's local
    /// macros plus any shared library macros they reference.
    ///
    /// This is the single source-of-truth conversion; both compile functions
    /// use it. Compiling only the directly-authored event-graph nodes (the
    /// previous behaviour) silently dropped macro bodies — PBGC's metadata
    /// provider doesn't recognise `"macro:<id>"` as a node type, so those
    /// instances would compile to nothing.
    ///
    /// `pub(crate)` since #656 — the validation stage runs the SAME expanded
    /// graph codegen consumes.
    pub(crate) fn build_graphy_description(&self) -> Result<graphy::GraphDescription, String> {
        let authored = self.convert_graph_to_description(&self.main_graph_tab().graph)?;
        let graph =
            blueprint_compiler::authored::expand_graph(&authored, self.collect_macro_library()?)?;

        self.dump_graph_debug_info(&graph);

        Ok(graph)
    }

    /// Run the #656 preflight validation stage against the live graph:
    /// structural checks on the UI-level graph (dangling connections, empty
    /// node types), then PBGC's own data-flow analysis over the converted and
    /// macro-expanded graph — exactly what codegen is about to consume.
    pub(crate) fn run_validation_stage(
        &mut self,
        target: crate::features::validation::ValidationTarget,
    ) -> crate::features::validation::ValidationReport {
        use crate::features::validation::{
            check_ui_graph_diagnostics, ValidationReport, ValidationTarget,
        };

        let mut report = ValidationReport {
            target: Some(target),
            diagnostics: Vec::new(),
        };

        // Structural pass on the UI-level description of the main tab.
        let main_tab = self.main_graph_tab();
        match self.convert_graph_to_description(&main_tab.graph) {
            Ok(description) => {
                report.merge(check_ui_graph_diagnostics(&description));
            }
            Err(e) => {
                report.push(format!("live graph conversion failed: {e}"));
                return report;
            }
        }

        // Conversion + macro expansion + data-flow analysis on the exact
        // pipeline codegen uses.
        match self.build_graphy_description() {
            Ok(graph) => {
                let metadata_provider = pbgc::metadata::BlueprintMetadataProvider::new();
                if let Err(e) = graphy::DataResolver::build(&graph, &metadata_provider) {
                    report.push(format!("data flow analysis failed: {e}"));
                }
                let _ = graphy::ExecutionRouting::build_from_graph(&graph);
            }
            Err(e) => report.push(format!("graph build failed: {e}")),
        }

        report
    }

    /// Assemble a `GraphLibrary` (keyed by macro id) covering every sub-graph
    /// this blueprint file can reference: local macros — overlaid with any
    /// open-tab edits not yet flushed back to `subgraphs` (mirroring
    /// `to_blueprint_asset`'s save snapshot) — plus shared library macros.
    fn collect_macro_library(
        &self,
    ) -> Result<HashMap<String, blueprint_graph::GraphDescription>, String> {
        let mut macros = self.subgraphs.clone();
        for tab in self
            .open_tabs
            .iter()
            .filter(|tab| !tab.is_main && !tab.is_library_macro)
        {
            if let Some(macro_def) = macros.iter_mut().find(|m| m.id == tab.id) {
                macro_def.graph = self.convert_graph_to_description(&tab.graph)?;
            }
        }

        let mut library = HashMap::new();
        for macro_def in &macros {
            library.insert(macro_def.id.clone(), macro_def.graph.clone());
        }
        for subgraph in self.library_manager.get_all_subgraphs() {
            library
                .entry(subgraph.id.clone())
                .or_insert_with(|| subgraph.graph.clone());
        }

        Ok(library)
    }

    /// Compile the class to an engine script module: the one language-neutral
    /// result both compile targets are built from (see `blueprint_compiler`).
    /// `Err` carries every diagnostic.
    fn compile_module(&self) -> Result<pulsar_script_vm::Module, String> {
        let class_path = self
            .current_class_path
            .as_ref()
            .ok_or("No class loaded — cannot compile")?;
        let class_name = crate::features::class_dirs::class_name_of(class_path)
            .unwrap_or_else(|| "unnamed_blueprint".to_owned());
        let variables: Vec<blueprint_compiler::VariableSource> = self
            .class_variables
            .iter()
            .map(|v| blueprint_compiler::VariableSource {
                id: Some(v.id.clone()),
                name: v.name.clone(),
                type_name: v.var_type.clone(),
                default: v.default_value.as_deref().map(property_value_from_raw),
            })
            .collect();
        let graph = self.build_graphy_description()?;
        // Engine events (#924): this class's custom events are declared;
        // event nodes are checked against the built-in and other classes'.
        let events = crate::features::events::engine_events::event_sources(&self.local_event_defs);
        let known_events =
            crate::features::events::engine_events::known_event_signatures_with_components(
                Some(class_path.as_path()),
                &self.component_event_metadata,
            );
        let source = blueprint_compiler::ClassSource {
            name: &class_name,
            graph: &graph,
            variables: &variables,
            events: &events,
            known_events: &known_events,
            version: 0,
        };
        blueprint_compiler::compile(&source, script_natives()).map_err(|diagnostics| {
            let lines: Vec<String> = diagnostics.iter().map(ToString::to_string).collect();
            format!("Script module compilation failed:\n{}", lines.join("\n"))
        })
    }

    /// Compile the class to an engine script module and write it to
    /// `<class_path>/events/.build/module.json`: the language-neutral
    /// bytecode `pulsar_script_runtime` loads (see `blueprint_compiler`).
    pub fn compile_to_script_module(&self) -> Result<PathBuf, String> {
        let class_path = self
            .current_class_path
            .as_ref()
            .ok_or("No class loaded — cannot compile")?;
        let build_dir = class_path.join("events").join(".build");
        let out_path = build_dir.join("module.json");
        crate::features::class_dirs::check_language(class_path)?;
        let module = self.compile_module().inspect_err(|_| {
            // Never leave a module from an older graph behind.
            let _ = std::fs::remove_file(&out_path);
        })?;

        let json = module
            .to_json()
            .map_err(|e| format!("Failed to serialise module: {e}"))?;
        std::fs::create_dir_all(&build_dir)
            .map_err(|e| format!("Failed to create .build directory: {e}"))?;
        std::fs::write(&out_path, json).map_err(|e| format!("Failed to write module.json: {e}"))?;
        crate::features::class_dirs::mark_language(class_path)
            .map_err(|e| format!("Failed to record the language: {e}"))?;
        tracing::info!("Script module written to {}", out_path.display());
        Ok(out_path)
    }

    /// The prefab components the exported actor hydrates at `begin_play`.
    fn export_components(&self) -> Vec<pulsar_script_codegen::actor::ComponentSpec> {
        self.prefab_asset
            .components
            .iter()
            .map(|c| pulsar_script_codegen::actor::ComponentSpec {
                class_name: c.class_name.clone(),
                property_defaults_json: c.data.to_string(),
                enabled: c.enabled,
            })
            .collect()
    }

    /// Compile current graph to Rust source code: the class's script module,
    /// exported as an `Actor` (see `pulsar_script_codegen`).
    pub fn compile_to_rust(&self) -> Result<String, String> {
        let blueprint_name = self
            .current_class_path
            .as_deref()
            .and_then(crate::features::class_dirs::class_name_of)
            .unwrap_or_else(|| "compiled_blueprint".to_owned());
        let module = self.compile_module()?;
        let mut source = pulsar_script_codegen::actor::generate_actor(
            &blueprint_name,
            &module,
            &self.export_components(),
        )
        .map_err(|e| format!("Compilation failed: {e}"))?;
        if !self.blueprint_metadata.implemented_traits.is_empty() {
            let class_path = self
                .current_class_path
                .as_deref()
                .ok_or("No class loaded — cannot resolve Blueprint traits")?;
            let project_root = self
                .project_root
                .clone()
                .or_else(|| crate::features::class_dirs::project_root_of(class_path))
                .ok_or("Could not determine project root to resolve Blueprint traits")?;
            let declarations = crate::features::compilation::trait_contracts::generate_module(
                &project_root,
                &self.blueprint_metadata.implemented_traits,
            )?;
            source.push('\n');
            source.push_str(&declarations);
        }
        Ok(source)
    }

    /// Compile and save events to class directory structure
    pub fn compile_to_class_directory(&self) -> Result<(), String> {
        let class_path = self
            .current_class_path
            .as_ref()
            .ok_or("No class loaded - cannot compile")?;

        // Ensure variables are persisted first
        self.save_variables_to_class()?;
        self.generate_vars_module()?;

        let events_dir = class_path.join("events");
        std::fs::create_dir_all(&events_dir)
            .map_err(|e| format!("Failed to create events directory: {}", e))?;

        let has_events = self
            .main_graph_tab()
            .graph
            .nodes
            .iter()
            .any(|n| n.node_type == crate::NodeType::Event);
        if !has_events {
            return Err("No event nodes found in graph".to_string());
        }

        let blueprint_name = crate::features::class_dirs::class_name_of(class_path)
            .unwrap_or_else(|| "compiled_blueprint".to_owned());
        let blueprint_name = blueprint_name.as_str();

        // The same module the VM target writes, exported as an Actor class.
        // Classes the exporter cannot represent are refused with a reason
        // rather than exported with behaviour silently missing.
        let generated = self.compile_to_rust()?;

        // Write all events into a single file
        let events_file = events_dir.join("events.rs");
        std::fs::write(&events_file, &generated)
            .map_err(|e| format!("Failed to write events.rs: {}", e))?;

        // Write mod.rs that re-exports everything from events.rs
        let now = chrono::Local::now();
        let version = ui::ENGINE_VERSION;
        let mod_content = format!(
            "//! Auto Generated by the Pulsar Blueprint Editor\n\
             //! DO NOT EDIT MANUALLY - YOUR CHANGES WILL BE OVERWRITTEN\n\
             //! Generated on {} - Engine version {}\n\
             //!\n\
             //! To modify events, open the class in the Pulsar Blueprint Editor.\n\n\
             pub mod events;\n\
             pub use events::*;\n",
            now.format("%Y-%m-%d %H:%M:%S"),
            version
        );
        let mod_path = events_dir.join("mod.rs");
        std::fs::write(&mod_path, mod_content)
            .map_err(|e| format!("Failed to write mod.rs: {}", e))?;

        // ── Update <Class>/mod.rs ─────────────────────────────────────────────
        // Overwrite the class root module so it cleanly declares vars + events
        // and re-exports the actor type. The old stub (hand-written or from an
        // earlier engine version) may contain a duplicate struct definition that
        // conflicts with the one in events/events.rs.
        let class_mod = class_path.join("mod.rs");
        let class_mod_content = format!(
            "//! {blueprint_name} — generated by Pulsar Blueprint Editor.\n\
             //! DO NOT EDIT MANUALLY - YOUR CHANGES WILL BE OVERWRITTEN\n\n\
             pub mod vars;\n\
             pub mod events;\n\
             pub use events::*;\n"
        );
        std::fs::write(&class_mod, class_mod_content)
            .map_err(|e| format!("Failed to write {blueprint_name}/mod.rs: {e}"))?;

        // ── Ensure src/classes/mod.rs declares this class ─────────────────────
        if let Some(classes_dir) = class_path.parent() {
            let classes_mod = classes_dir.join("mod.rs");
            let mod_decl = format!("pub mod {blueprint_name};");
            let existing = std::fs::read_to_string(&classes_mod).unwrap_or_default();
            if !existing.contains(&mod_decl) {
                // Append the declaration; preserve any existing hand-written content.
                let updated = if existing.trim().is_empty() {
                    format!("//! Generated by Pulsar Blueprint Editor.\n\n{mod_decl}\n")
                } else {
                    format!("{}\n{}\n", existing.trim_end(), mod_decl)
                };
                std::fs::write(&classes_mod, updated)
                    .map_err(|e| format!("Failed to update classes/mod.rs: {e}"))?;
            }
        }

        tracing::info!("Compiled blueprint events to {}", events_dir.display());
        Ok(())
    }

    /// Start compilation (called from toolbar)
    pub fn start_compilation(&mut self, cx: &mut Context<Self>) {
        let panel_entity = cx.weak_entity();
        cx.spawn(async move |_entity, mut cx| {
            Self::compile_async(panel_entity, &mut cx).await;
        })
        .detach();
    }

    /// Compile in background with status updates
    pub async fn compile_async(panel_entity: gpui::WeakEntity<Self>, cx: &mut gpui::AsyncApp) {
        let started_at = std::time::Instant::now();

        // Capture compile mode before entering async context.
        let compile_mode = match panel_entity.update(cx, |panel, _cx| panel.compile_mode.clone()) {
            Ok(m) => m,
            Err(_) => return,
        };

        // Set compiling state
        let result = panel_entity.update(cx, |panel, cx| {
            panel.compilation_status = CompilationStatus {
                state: CompilationState::Compiling,
                message: "Compiling blueprint...".to_string(),
                progress: 0.0,
                is_compiling: true,
            };

            let class_path_display = panel
                .current_class_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<no class loaded>".to_string());

            panel.push_compilation_history(
                CompilationState::Compiling,
                "prepare",
                "Compilation started",
                Some(format!("Class path: {}", class_path_display)),
            );

            // ── Validation stage (#656): bad graphs never reach codegen ────
            use crate::features::validation::ValidationTarget;
            let validation_target = match &compile_mode {
                CompileMode::DirectRust => ValidationTarget::DirectRust,
                CompileMode::BytecodeVm => ValidationTarget::BytecodeVm,
            };
            let report = panel.run_validation_stage(validation_target);
            panel.validation_problems = report.diagnostics.clone();
            panel.push_compilation_history(
                if report.has_errors() {
                    CompilationState::Error
                } else {
                    CompilationState::Compiling
                },
                "validate",
                format!("Graph validation: {}", report.summary()),
                None,
            );
            if report.has_errors() {
                panel.compilation_status = CompilationStatus {
                    state: CompilationState::Error,
                    message: format!(
                        "✗ Validation failed: {} — see Problems",
                        report.summary()
                    ),
                    progress: 0.0,
                    is_compiling: false,
                };
                cx.notify();
                return Err(format!(
                    "Validation failed ({}). Fix the errors listed in the Problems panel.",
                    report.summary()
                ));
            }

            use crate::core::types::CompileMode;
            match &compile_mode {
                CompileMode::DirectRust => {
                    panel.push_compilation_history(
                        CompilationState::Compiling,
                        "build",
                        "Generating Rust event modules",
                        Some(
                            "Steps: validate event nodes, compile graph, write events/events.rs, write events/mod.rs, refresh vars module"
                                .to_string(),
                        ),
                    );
                    cx.notify();
                    panel.sync_all_canvases_to_tabs(cx);
                    panel.compile_to_class_directory().map(|_| None::<PathBuf>)
                }
                CompileMode::BytecodeVm => {
                    panel.push_compilation_history(
                        CompilationState::Compiling,
                        "build",
                        "Compiling to an engine script module",
                        Some(
                            "Steps: build graph description, compile to a script module \
                             (events/.build/module.json)"
                                .to_string(),
                        ),
                    );
                    cx.notify();
                    panel.sync_all_canvases_to_tabs(cx);
                    // The engine script module is what the game runtime runs.
                    // Remove bytecode left by the old PBGC VM path.
                    if let Some(class_path) = &panel.current_class_path {
                        let _ = std::fs::remove_file(class_path.join("events").join(".build").join("bytecode.json"));
                    }
                    panel.compile_to_script_module().map(Some)
                }
            }
        });

        if let Ok(compile_result) = result {
            match compile_result {
                Ok(maybe_path) => {
                    // Success
                    smol::Timer::after(std::time::Duration::from_millis(500)).await;
                    let _ = panel_entity.update(cx, |panel, cx| {
                        let elapsed_ms = started_at.elapsed().as_millis();

                        use crate::core::types::CompileMode;
                        let detail = match panel.compile_mode {
                            CompileMode::DirectRust => {
                                let output_events = panel
                                    .current_class_path
                                    .as_ref()
                                    .map(|p| {
                                        p.join("events").join("events.rs").display().to_string()
                                    })
                                    .unwrap_or_else(|| "events/events.rs".to_string());
                                let output_mod = panel
                                    .current_class_path
                                    .as_ref()
                                    .map(|p| p.join("events").join("mod.rs").display().to_string())
                                    .unwrap_or_else(|| "events/mod.rs".to_string());
                                format!(
                                    "Duration: {} ms | Outputs: {}, {}",
                                    elapsed_ms, output_events, output_mod
                                )
                            }
                            CompileMode::BytecodeVm => {
                                let out = maybe_path
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|| "events/.build/module.json".to_string());
                                format!(
                                    "Duration: {} ms | Output: {} | \
                                     Run `cargo run` in your project to execute via the VM runtime",
                                    elapsed_ms, out
                                )
                            }
                        };

                        panel.compilation_status = CompilationStatus {
                            state: CompilationState::Success,
                            message: "✓ Compilation successful".to_string(),
                            progress: 1.0,
                            is_compiling: false,
                        };
                        // The module is written: placed instances and a
                        // running game reload the class.
                        if let Some(class_dir) = panel.current_class_path.clone() {
                            crate::features::prefabs::publish_class_updated(&class_dir);
                        }

                        panel.push_compilation_history(
                            CompilationState::Success,
                            "complete",
                            "Compilation successful",
                            Some(detail),
                        );

                        cx.notify();
                    });
                }
                Err(e) => {
                    // Compilation error
                    let _ = panel_entity.update(cx, |panel, cx| {
                        let elapsed_ms = started_at.elapsed().as_millis();
                        let error_text = e;

                        panel.compilation_status = CompilationStatus {
                            state: CompilationState::Error,
                            message: format!("✗ Compilation failed: {}", error_text),
                            progress: 0.0,
                            is_compiling: false,
                        };

                        panel.push_compilation_history(
                            CompilationState::Error,
                            "error",
                            "Compilation failed",
                            Some(format!(
                                "Duration: {} ms | Reason: {}",
                                elapsed_ms, error_text
                            )),
                        );

                        cx.notify();
                    });
                }
            }
        } else {
            // Panel entity no longer exists - try to update anyway
            let _ = panel_entity.update(cx, |panel, cx| {
                panel.compilation_status = CompilationStatus {
                    state: CompilationState::Error,
                    message: "✗ Compilation failed: panel closed".to_string(),
                    progress: 0.0,
                    is_compiling: false,
                };
                panel.push_compilation_history(
                    CompilationState::Error,
                    "error",
                    "Compilation aborted",
                    Some("Editor panel closed before compile completed".to_string()),
                );
                cx.notify();
            });
        }

        // Clear status after 3 seconds
        smol::Timer::after(std::time::Duration::from_secs(3)).await;
        let _ = panel_entity.update(cx, |panel, cx| {
            if panel.compilation_status.state != CompilationState::Compiling {
                panel.compilation_status = CompilationStatus::default();
                cx.notify();
            }
        });
    }
}
