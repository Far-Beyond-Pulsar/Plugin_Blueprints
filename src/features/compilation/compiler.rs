//! Compiler - Compile blueprints to Rust code

use crate::editor::panel::{BlueprintEditorPanel, CompilationHistoryEntry};
use crate::{CompilationState, CompilationStatus};
use gpui::*;
use ui::compiler;

// Convert a pulsar_graph DataType into the graphy DataType the compiler expects.
fn to_graphy_datatype(dt: &ui::graph::DataType) -> graphy::DataType {
    use graphy::DataType as GD;
    use ui::graph::DataType as PG;
    match dt {
        PG::Execution => GD::Execution,
        PG::Typed(ti) => GD::Typed(graphy::core::TypeInfo::new(ti.to_string())),
        PG::Any => GD::Any,
        PG::String => GD::String,
        PG::Number => GD::Number,
        PG::Boolean => GD::Boolean,
        PG::Vector2 => GD::Vector2,
        PG::Vector3 => GD::Vector3,
        PG::Color => GD::Color,
        PG::Object => GD::Typed(graphy::core::TypeInfo::new("Object")),
        PG::Array(inner) => GD::Typed(graphy::core::TypeInfo::new(format!("Vec<{}>", inner))),
    }
}

impl BlueprintEditorPanel {
    /// Build a `graphy::GraphDescription` directly from the current BlueprintGraph.
    /// This is the single source-of-truth conversion; both compile functions use it.
    fn build_graphy_description(&self) -> Result<graphy::GraphDescription, String> {
        use graphy::Connection as GConnection;
        use graphy::{
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
                    .map(|(k, v)| (k.clone(), PropertyValue::String(v.clone())))
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

    /// Compile current graph to Rust source code
    pub fn compile_to_rust(&self) -> Result<String, String> {
        let graph = self.build_graphy_description()?;
        compiler::compile_graph(&graph).map_err(|e| format!("Compilation failed: {}", e))
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

        // Build the graphy graph and compile all events in one pass
        let graph = self.build_graphy_description()?;
        let variables: std::collections::HashMap<String, String> = self
            .class_variables
            .iter()
            .map(|v| (v.name.clone(), v.var_type.clone()))
            .collect();

        let generated = compiler::compile_graph_with_variables(&graph, variables)
            .map_err(|e| format!("Compilation failed: {}", e))?;

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
        // Set compiling state
        let result = panel_entity.update(cx, |panel, cx| {
            panel.compilation_status = CompilationStatus {
                state: CompilationState::Compiling,
                message: "Compiling blueprint...".to_string(),
                progress: 0.0,
                is_compiling: true,
            };
            cx.notify();
            panel.compile_to_class_directory()
        });

        if let Ok(compile_result) = result {
            match compile_result {
                Ok(()) => {
                    // Success
                    smol::Timer::after(std::time::Duration::from_millis(500)).await;
                    let _ = panel_entity.update(cx, |panel, cx| {
                        panel.compilation_status = CompilationStatus {
                            state: CompilationState::Success,
                            message: "✓ Compilation successful".to_string(),
                            progress: 1.0,
                            is_compiling: false,
                        };

                        // Add to history
                        let now = chrono::Local::now();
                        panel.compilation_history.push(CompilationHistoryEntry {
                            timestamp: now.format("%H:%M:%S").to_string(),
                            state: CompilationState::Success,
                            message: "Compilation successful".to_string(),
                        });

                        cx.notify();
                    });
                }
                Err(e) => {
                    // Compilation error
                    let _ = panel_entity.update(cx, |panel, cx| {
                        panel.compilation_status = CompilationStatus {
                            state: CompilationState::Error,
                            message: format!("✗ Compilation failed: {}", e),
                            progress: 0.0,
                            is_compiling: false,
                        };

                        // Add to history
                        let now = chrono::Local::now();
                        panel.compilation_history.push(CompilationHistoryEntry {
                            timestamp: now.format("%H:%M:%S").to_string(),
                            state: CompilationState::Error,
                            message: format!("Compilation failed: {}", e),
                        });

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
