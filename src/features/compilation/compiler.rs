//! Compiler - Compile blueprints to Rust code or PBGC bytecode

use crate::editor::panel::{BlueprintEditorPanel, CompilationHistoryEntry};
use crate::{CompilationState, CompilationStatus};
use gpui::*;
use std::collections::HashMap;
use std::path::PathBuf;

// ── Bytecode file format ──────────────────────────────────────────────────────

/// JSON output written to `<class>/events/.build/bytecode.json`.
///
/// Field layout is intentionally compatible with
/// `pulsar_game::blueprint_runtime::CompiledBytecode` so the game runtime can
/// deserialise it without knowing about this type.
#[derive(serde::Serialize)]
struct BytecodeFileOutput {
    /// Format version — must stay 1 unless the runtime is updated in lock-step.
    version: u32,
    /// Blueprint class name (used as the key in `BlueprintDispatcher`).
    source_class: String,
    /// Variable descriptors — currently empty; the runtime initialises an arena
    /// large enough for the programs' combined `arena_size` without needing
    /// explicit layout here.
    variables: Vec<serde_json::Value>,
    /// One compiled program per event entry-point, keyed by event name
    /// ("begin_play", "tick", …).  Function pointers are zero here; the game
    /// runtime's `BpExecutor::prepare` patches them from `pulsar_std`.
    event_programs: HashMap<String, pbgc::BpProgram>,
    /// Bytes needed for the per-instance state arena.
    arena_size: usize,
}

// ── Property normalisation (shared by both compile paths) ────────────────────

/// Normalizes property literals that may be JSON-string-encoded one or more
/// times by the editor/serialization path.
///
/// Example: `"\"2\""` -> `2`
fn normalize_property_literal(raw: &str) -> String {
    let mut out = raw.trim().to_string();

    // If the value contains escaped quotes (e.g. \"2\"), collapse those
    // first so the JSON string decode loop below can unwrap it.
    if out.contains("\\\"") {
        out = out.replace("\\\"", "\"");
    }

    // Decode nested JSON string encoding up to a small fixed depth.
    for _ in 0..3 {
        match serde_json::from_str::<String>(&out) {
            Ok(decoded) => out = decoded,
            Err(_) => break,
        }
    }

    out
}

fn property_value_from_raw(raw: &str) -> pbgc::JsonValue {
    let s = normalize_property_literal(raw);
    let lower = s.to_ascii_lowercase();

    if lower == "true" {
        return pbgc::JsonValue::Bool(true);
    }
    if lower == "false" {
        return pbgc::JsonValue::Bool(false);
    }

    if let Ok(n) = s.parse::<f64>() {
        if n.is_finite() {
            if let Some(number) = serde_json::Number::from_f64(n) {
                return pbgc::JsonValue::Number(number);
            }
        }
    }

    pbgc::JsonValue::String(s)
}

// Convert a pulsar_graph DataType into the PBGC DataType the compiler expects.
fn to_graphy_datatype(dt: &ui::graph::DataType) -> pbgc::DataType {
    use pbgc::DataType as GD;
    use ui::graph::DataType as PG;
    match dt {
        PG::Execution => GD::Execution,
        PG::Data(ti) => GD::Typed(pbgc::TypeInfo::new(ti.to_string())),
    }
}

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

    /// Build a `pbgc::GraphDescription` directly from the current BlueprintGraph.
    /// This is the single source-of-truth conversion; both compile functions use it.
    fn build_graphy_description(&self) -> Result<pbgc::GraphDescription, String> {
        use pbgc::Connection as GConnection;
        use pbgc::{
            ConnectionType, GraphDescription, NodeInstance, Pin, PinInstance, PinType, Position,
        };
        use std::collections::{HashMap, HashSet};

        let mut graph = GraphDescription::new("Blueprint Graph");
        let mut skipped_nodes: HashSet<String> = HashSet::new();
        let mut valid_input_pins: HashMap<String, HashSet<String>> = HashMap::new();
        let mut valid_output_pins: HashMap<String, HashSet<String>> = HashMap::new();

        // Nodes
        for bp_node in &self.open_tabs[self.active_tab_index].graph.nodes {
            // Runtime component reference nodes are editor-only wiring helpers.
            if bp_node.definition_id.starts_with("get_component_ref::") {
                skipped_nodes.insert(bp_node.id.clone());
                continue;
            }

            let node_type = bp_node.definition_id.clone();
            let is_component_method_node = node_type.starts_with("comp_get_prop::")
                || node_type.starts_with("comp_set_prop::")
                || node_type.starts_with("comp_call::");
            let mut node = NodeInstance {
                id: bp_node.id.clone(),
                node_type,
                position: Position {
                    x: bp_node.position.x as f64,
                    y: bp_node.position.y as f64,
                },
                inputs: Vec::new(),
                outputs: Vec::new(),
                properties: bp_node
                    .properties
                    .iter()
                    .map(|(k, v)| (k.clone(), property_value_from_raw(v)))
                    .collect(),
            };

            for pin in &bp_node.inputs {
                // The UI exposes component-ref pins, but current PBGC node handlers
                // still compile component nodes by class/property/method id.
                // Strip the editor-only target ref pin before codegen.
                if is_component_method_node && pin.id == "component_ref" {
                    continue;
                }
                node.inputs.push(PinInstance {
                    id: pin.id.clone(),
                    pin: Pin {
                        id: pin.id.clone(),
                        name: pin.name.clone(),
                        data_type: to_graphy_datatype(&pin.data_type),
                        pin_type: PinType::Input,
                    },
                });
            }
            for pin in &bp_node.outputs {
                node.outputs.push(PinInstance {
                    id: pin.id.clone(),
                    pin: Pin {
                        id: pin.id.clone(),
                        name: pin.name.clone(),
                        data_type: to_graphy_datatype(&pin.data_type),
                        pin_type: PinType::Output,
                    },
                });
            }

            valid_input_pins.insert(
                bp_node.id.clone(),
                node.inputs.iter().map(|p| p.id.clone()).collect(),
            );
            valid_output_pins.insert(
                bp_node.id.clone(),
                node.outputs.iter().map(|p| p.id.clone()).collect(),
            );
            graph.nodes.insert(bp_node.id.clone(), node);
        }

        // Connections
        for conn in &self.open_tabs[self.active_tab_index].graph.connections {
            if skipped_nodes.contains(&conn.source_node)
                || skipped_nodes.contains(&conn.target_node)
            {
                continue;
            }
            let Some(source_pins) = valid_output_pins.get(&conn.source_node) else {
                continue;
            };
            let Some(target_pins) = valid_input_pins.get(&conn.target_node) else {
                continue;
            };
            if !source_pins.contains(&conn.source_pin) || !target_pins.contains(&conn.target_pin) {
                continue;
            }
            let conn_type = match conn.connection_type {
                ui::graph::ConnectionType::Execution => ConnectionType::Execution,
                ui::graph::ConnectionType::Data => ConnectionType::Data,
            };
            graph.connections.push(GConnection {
                source_node: conn.source_node.clone(),
                source_pin: conn.source_pin.clone(),
                target_node: conn.target_node.clone(),
                target_pin: conn.target_pin.clone(),
                connection_type: conn_type,
            });
        }

        Ok(graph)
    }

    /// Compile current graph → raw PBGC bytecode programs (one per event entry-point).
    pub fn compile_to_bytecode(&self) -> Result<Vec<pbgc::BpProgram>, String> {
        let variables: std::collections::HashMap<String, String> = self
            .class_variables
            .iter()
            .map(|v| (v.name.clone(), v.var_type.clone()))
            .collect();

        let graph = self.build_graphy_description()?;
        pbgc::compile_graph_to_bytecode_with_variables(&graph, variables)
            .map_err(|e| format!("Bytecode compilation failed: {}", e))
    }

    /// Compile the current graph and write the result to
    /// `<class_path>/events/.build/bytecode.json`.
    ///
    /// The produced file can be loaded by
    /// `pulsar_game::blueprint_runtime::BlueprintDispatcher` at game startup —
    /// the game runtime handles `BpExecutor::prepare` (function-pointer patching)
    /// and drives `begin_play` / `tick` / `end_play` through the `TickLoop`.
    pub fn compile_to_bytecode_files(&self) -> Result<PathBuf, String> {
        let class_path = self
            .current_class_path
            .as_ref()
            .ok_or("No class loaded — cannot compile")?;

        let programs = self.compile_to_bytecode()?;

        if programs.is_empty() {
            return Err(
                "No event entry-points found in graph — add a BeginPlay or Tick node".to_string(),
            );
        }

        // Map programs by event name.  BpProgram::name carries the event type
        // ("begin_play", "tick", …) set by the bytecode codegen.
        let arena_size = programs
            .iter()
            .map(|p| p.arena_size)
            .max()
            .unwrap_or(0)
            .max(1024); // minimum 1 KiB so the runtime always has headroom

        let event_programs: HashMap<String, pbgc::BpProgram> =
            programs.into_iter().map(|p| (p.name.clone(), p)).collect();

        let blueprint_name = class_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed_blueprint")
            .to_owned();

        let output = BytecodeFileOutput {
            version: 1,
            source_class: blueprint_name,
            variables: Vec::new(),
            event_programs,
            arena_size,
        };

        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| format!("Failed to serialise bytecode: {}", e))?;

        // Ensure .build directory exists under events/
        let build_dir = class_path.join("events").join(".build");
        std::fs::create_dir_all(&build_dir)
            .map_err(|e| format!("Failed to create .build directory: {}", e))?;

        let out_path = build_dir.join("bytecode.json");
        std::fs::write(&out_path, json)
            .map_err(|e| format!("Failed to write bytecode.json: {}", e))?;

        tracing::info!("Bytecode written to {}", out_path.display());
        Ok(out_path)
    }

    /// Compile current graph to Rust source code
    pub fn compile_to_rust(&self) -> Result<String, String> {
        let graph = self.build_graphy_description()?;
        let blueprint_name = self
            .current_class_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("compiled_blueprint");

        pbgc::compile_graph_to_actor_source(blueprint_name, &graph)
            .map_err(|e| format!("Compilation failed: {}", e))
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
            .open_tabs[self.active_tab_index]
            .graph
            .nodes
            .iter()
            .any(|n| n.node_type == crate::NodeType::Event);
        if !has_events {
            return Err("No event nodes found in graph".to_string());
        }

        // Build the graph and compile in one pass through PBGC.
        // Wrap raw generated logic into an Actor class so we always emit
        // a struct with `#[derive(EngineClass)]`.
        let graph = self.build_graphy_description()?;
        let variables: std::collections::HashMap<String, String> = self
            .class_variables
            .iter()
            .map(|v| (v.name.clone(), v.var_type.clone()))
            .collect();

        let generated_logic = pbgc::compile_graph_with_variables(&graph, variables)
            .map_err(|e| format!("Compilation failed: {}", e))?;

        let blueprint_name = class_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("compiled_blueprint");

        // Extract component data from the prefab sidecar so the generated actor
        // can initialise and drive its components during begin_play / tick.
        let compiled_components: Vec<pbgc::CompiledComponent> = self
            .prefab_asset
            .components
            .iter()
            .map(|c| pbgc::CompiledComponent {
                class_name: c.class_name.clone(),
                property_defaults: c.data.clone(),
                enabled: c.enabled,
            })
            .collect();

        let generated = pbgc::generate_blueprint_actor_source_with_components(
            blueprint_name,
            &generated_logic,
            compiled_components,
        );

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
                    panel.compile_to_class_directory().map(|_| None::<PathBuf>)
                }
                CompileMode::BytecodeVm => {
                    panel.push_compilation_history(
                        CompilationState::Compiling,
                        "build",
                        "Compiling to PBGC bytecode",
                        Some(
                            "Steps: build graph description, compile to bytecode programs, \
                             write events/.build/bytecode.json"
                                .to_string(),
                        ),
                    );
                    cx.notify();
                    panel.compile_to_bytecode_files().map(Some)
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
                                    .unwrap_or_else(|| "events/.build/bytecode.json".to_string());
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
