//! Compiler - Compile blueprints to Rust code or PBGC bytecode

use crate::editor::panel::{BlueprintEditorPanel, CompilationHistoryEntry};
use crate::{CompilationState, CompilationStatus};
use gpui::*;

/// Normalizes property literals that may be JSON-string-encoded one or more
/// times by the editor/serialization path.
///
/// Example: `"\"2\""` -> `2`
fn normalize_property_literal(raw: &str) -> String {
    let mut out = raw.trim().to_string();

    // Decode nested JSON string encoding up to a small fixed depth.
    for _ in 0..3 {
        match serde_json::from_str::<String>(&out) {
            Ok(decoded) => out = decoded,
            Err(_) => break,
        }
    }

    out
}

// Convert a pulsar_graph DataType into the PBGC DataType the compiler expects.
fn to_graphy_datatype(dt: &ui::graph::DataType) -> pbgc::DataType {
    use pbgc::DataType as GD;
    use ui::graph::DataType as PG;
    match dt {
        PG::Execution => GD::Execution,
        PG::Typed(ti) => GD::Typed(pbgc::TypeInfo::new(ti.to_string())),
        PG::Any => GD::Any,
        PG::String => GD::String,
        PG::Number => GD::Number,
        PG::Boolean => GD::Boolean,
        PG::Vector2 => GD::Vector2,
        PG::Vector3 => GD::Vector3,
        PG::Color => GD::Color,
        PG::Object => GD::Typed(pbgc::TypeInfo::new("Object")),
        PG::Array(inner) => GD::Typed(pbgc::TypeInfo::new(format!("Vec<{}>", inner))),
    }
}

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
            PropertyValue,
        };

        let mut graph = GraphDescription::new("Blueprint Graph");

        // Nodes
        for bp_node in &self.graph.nodes {
            let mut node = NodeInstance {
                id: bp_node.id.clone(),
                node_type: bp_node.definition_id.clone(),
                position: Position {
                    x: bp_node.position.x as f64,
                    y: bp_node.position.y as f64,
                },
                inputs: Vec::new(),
                outputs: Vec::new(),
                properties: bp_node
                    .properties
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            PropertyValue::String(normalize_property_literal(v)),
                        )
                    })
                    .collect(),
            };

            for pin in &bp_node.inputs {
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

            graph.nodes.insert(bp_node.id.clone(), node);
        }

        // Connections
        for conn in &self.graph.connections {
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

    /// Compile current graph → PBGC bytecode programs.
    ///
    /// Returns one `BpProgram` per entry-point (event) found in the graph.
    pub fn compile_to_bytecode(&self) -> Result<Vec<pbgc::BpProgram>, String> {
        let graph = self.build_graphy_description()?;
        pbgc::compile_graph_to_bytecode(&graph)
            .map_err(|e| format!("Bytecode compilation failed: {}", e))
    }

    /// Execute bytecode programs immediately against the embedded native cdylib.
    ///
    /// Extracts `pulsar_std` to a temp file, loads it via `BpExecutor`, patches
    /// all `fn_ptr` slots, and runs each program through `pbgc::vm::run`.
    ///
    /// # Safety
    ///
    /// The `BpExecutor` (and its backing `TempLib`) must remain alive for the
    /// entire duration of `vm::run`. Both are kept on the stack here.
    pub fn execute_bytecode_programs(
        &self,
        programs: Vec<pbgc::BpProgram>,
    ) -> Result<(), String> {
        let tmp = pulsar_std_bundle::extract_to_tempfile()
            .map_err(|e| format!("Failed to extract pulsar_std: {}", e))?;
        let executor = pulsar_bp_executor::BpExecutor::load(&tmp.path)
            .map_err(|e| format!("Failed to load pulsar_std: {}", e))?;

        for mut prog in programs {
            executor
                .prepare(&mut prog)
                .map_err(|e| format!("Dispatch resolve failed: {}", e))?;
            pbgc::vm::run(&prog)
                .map_err(|e| format!("VM run failed: {}", e))?;
        }
        Ok(())
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
        let generated = pbgc::generate_blueprint_actor_source(blueprint_name, &generated_logic);

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
                    panel.compile_to_class_directory()
                }
                CompileMode::BytecodeVm => {
                    panel.push_compilation_history(
                        CompilationState::Compiling,
                        "build",
                        "Compiling to PBGC bytecode and executing via native VM",
                        Some("Steps: build graph description, compile to bytecode, load pulsar_std cdylib, resolve dispatch symbols, vm::run".to_string()),
                    );
                    cx.notify();
                    panel
                        .compile_to_bytecode()
                        .and_then(|programs| panel.execute_bytecode_programs(programs))
                }
            }
        });

        if let Ok(compile_result) = result {
            match compile_result {
                Ok(()) => {
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
                                    .map(|p| p.join("events").join("events.rs").display().to_string())
                                    .unwrap_or_else(|| "events/events.rs".to_string());
                                let output_mod = panel
                                    .current_class_path
                                    .as_ref()
                                    .map(|p| p.join("events").join("mod.rs").display().to_string())
                                    .unwrap_or_else(|| "events/mod.rs".to_string());
                                format!("Duration: {} ms | Outputs: {}, {}", elapsed_ms, output_events, output_mod)
                            }
                            CompileMode::BytecodeVm => {
                                format!("Duration: {} ms | Mode: Bytecode VM (pulsar_std cdylib)", elapsed_ms)
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
