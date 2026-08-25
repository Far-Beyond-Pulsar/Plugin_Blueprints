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
        PG::Execution => GD::Exec,
        PG::Data(ti) => GD::typed(ti.to_string()),
    }
}

// Convert a blueprint pin's reflection-backed `PinDataType` into the PBGC
// `DataType` the compiler expects — `type_name` is already the canonical
// string identity (matches `RuntimeTypeInfo::type_name`).
fn pin_data_type_to_graphy(dt: &crate::core::types::PinDataType) -> pbgc::DataType {
    if dt.is_execution() {
        pbgc::DataType::Exec
    } else {
        pbgc::DataType::typed(dt.type_name.clone())
    }
}

/// Convert a serialised sub-graph (`ui::graph::GraphDescription`, as stored in
/// `SubGraphDefinition::graph` for both local and library macros) into the
/// `pbgc::GraphDescription` shape the compiler and `SubGraphExpander` operate
/// on.
///
/// Identity-reference nodes (`get_component_ref::`, `find_object_by_*`,
/// `object_ref_literal`) and `comp_*` nodes' synthetic `component_ref` pins
/// compile like any other node/pin since #654 — PBGC routes them to runtime
/// reference resolution. This function additionally remaps the editor's
/// `macro_entry`/`macro_exit` interface nodes to the
/// `subgraph_entry`/`subgraph_exit` node-type strings that
/// `graphy::NodeInstance::kind()` recognises as macro interface points — the
/// expander rewires call-site connections through these during inlining.
///
/// `pub(crate)` since #656: the disk-level PIE preflight reuses this exact
/// conversion so validation sees byte-identical graphs to codegen.
pub(crate) fn convert_ui_graph_description_to_pbgc(ui_graph: &ui::graph::GraphDescription) -> pbgc::GraphDescription {
    use pbgc::Connection as GConnection;
    use pbgc::{ConnectionType, GraphDescription, NodeInstance, Pin, PinInstance, PinType, Position};
    use std::collections::{HashMap, HashSet};

    let mut graph = GraphDescription::new("Subgraph");
    let mut valid_input_pins: HashMap<String, HashSet<String>> = HashMap::new();
    let mut valid_output_pins: HashMap<String, HashSet<String>> = HashMap::new();

    for (node_id, node_instance) in &ui_graph.nodes {
        let node_type = match node_instance.node_type.as_str() {
            "macro_entry" => "subgraph_entry".to_string(),
            "macro_exit" => "subgraph_exit".to_string(),
            other => other.to_string(),
        };
        let mut node = NodeInstance {
            id: node_id.clone(),
            node_type,
            position: Position {
                x: node_instance.position.x as f64,
                y: node_instance.position.y as f64,
            },
            inputs: Vec::new(),
            outputs: Vec::new(),
            properties: node_instance
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            typed_properties: std::collections::HashMap::new(),
        };

        for pin_inst in &node_instance.inputs {
            node.inputs.push(PinInstance {
                id: pin_inst.id.clone(),
                pin: Pin {
                    id: pin_inst.id.clone(),
                    name: pin_inst.pin.name.clone(),
                    data_type: to_graphy_datatype(&pin_inst.pin.data_type),
                    pin_type: PinType::Input,
                },
            });
        }
        for pin_inst in &node_instance.outputs {
            node.outputs.push(PinInstance {
                id: pin_inst.id.clone(),
                pin: Pin {
                    id: pin_inst.id.clone(),
                    name: pin_inst.pin.name.clone(),
                    data_type: to_graphy_datatype(&pin_inst.pin.data_type),
                    pin_type: PinType::Output,
                },
            });
        }

        valid_input_pins.insert(
            node_id.clone(),
            node.inputs.iter().map(|p| p.id.clone()).collect(),
        );
        valid_output_pins.insert(
            node_id.clone(),
            node.outputs.iter().map(|p| p.id.clone()).collect(),
        );
        graph.nodes.insert(node_id.clone(), node);
    }

    for conn in &ui_graph.connections {
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

    graph
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

    /// Dump the active graph (editor view + the `pbgc::GraphDescription` that gets
    /// sent to the compiler) to `blueprint_graph_debug.json` in the working
    /// directory, so event-node detection mismatches can be diagnosed by
    /// comparing the editor's classification against PBGC's metadata lookup.
    fn dump_graph_debug_info(&self, graph: &pbgc::GraphDescription) {
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

    /// Build a `pbgc::GraphDescription` for the whole blueprint file: the main
    /// event graph with every `MacroInstance`/`SubgraphCall` node
    /// (`definition_id: "macro:<id>"`) inlined via graphy's
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
    pub(crate) fn build_graphy_description(&self) -> Result<pbgc::GraphDescription, String> {
        use pbgc::Connection as GConnection;
        use pbgc::{
            ConnectionType, GraphDescription, NodeInstance, Pin, PinInstance, PinType, Position,
        };
        use std::collections::HashSet;

        let mut graph = GraphDescription::new("Blueprint Graph");
        let mut valid_input_pins: HashMap<String, HashSet<String>> = HashMap::new();
        let mut valid_output_pins: HashMap<String, HashSet<String>> = HashMap::new();
        let main_tab = self.main_graph_tab();

        // Nodes
        for bp_node in &main_tab.graph.nodes {
            // Custom event On nodes → treat as event entry points named after the uid.
            // Custom event Dispatch nodes → emit_custom_event with an event_uid property.
            let node_type = if bp_node.definition_id.starts_with("custom_event:") {
                let uid = bp_node.definition_id.trim_start_matches("custom_event:");
                format!("on_{}", uid.replace('-', "_"))
            } else if bp_node.definition_id.starts_with("custom_event_dispatch:") {
                "emit_custom_event".to_string()
            } else {
                bp_node.definition_id.clone()
            };
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
                typed_properties: std::collections::HashMap::new(),
            };

            for pin in &bp_node.inputs {
                node.inputs.push(PinInstance {
                    id: pin.id.clone(),
                    pin: Pin {
                        id: pin.id.clone(),
                        name: pin.name.clone(),
                        data_type: pin_data_type_to_graphy(&pin.data_type),
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
                        data_type: pin_data_type_to_graphy(&pin.data_type),
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
        for conn in &main_tab.graph.connections {
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

        let library = self.collect_macro_library();
        if !library.is_empty() {
            graphy::SubGraphExpander::new()
                .expand_all_flat(&mut graph, &library)
                .map_err(|e| format!("Sub-graph expansion failed: {}", e))?;
        }

        self.dump_graph_debug_info(&graph);

        Ok(graph)
    }

    /// Assemble a `GraphLibrary` (keyed by macro id) covering every sub-graph
    /// this blueprint file can reference: local macros — overlaid with any
    /// open-tab edits not yet flushed back to `local_macros` (mirroring
    /// `to_blueprint_asset`'s save snapshot) — plus shared library macros.
    fn collect_macro_library(&self) -> HashMap<String, pbgc::GraphDescription> {
        let mut macros = self.local_macros.clone();
        for tab in self
            .open_tabs
            .iter()
            .filter(|tab| !tab.is_main && !tab.is_library_macro)
        {
            if let Some(macro_def) = macros.iter_mut().find(|m| m.id == tab.id) {
                if let Ok(desc) = self.convert_graph_to_description(&tab.graph) {
                    macro_def.graph = desc;
                }
            }
        }

        let mut library = HashMap::new();
        for macro_def in &macros {
            library.insert(
                macro_def.id.clone(),
                convert_ui_graph_description_to_pbgc(&macro_def.graph),
            );
        }
        for subgraph in self.library_manager.get_all_subgraphs() {
            library
                .entry(subgraph.id.clone())
                .or_insert_with(|| convert_ui_graph_description_to_pbgc(&subgraph.graph));
        }

        library
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
            .main_graph_tab()
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
                    panel.sync_all_canvases_to_tabs(cx);
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
                    panel.sync_all_canvases_to_tabs(cx);
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
