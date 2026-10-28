//! Macro operations — creating, opening, editing, and placing macro instances.

mod collapsed;

use crate::core::subgraph_ref::SubGraphReference;
use crate::editor::graph_canvas_panel::GraphCanvasPanel;
use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::tabs::TabDoc;
use crate::editor::GraphTab;
use gpui::*;
use graphy::{
    DataType, GraphCommand, GraphDescription, NodeInstance, Pin, PinInstance, PinType, Position,
};

/// Keep one boundary node of the requested role (`macro_entry` / `macro_exit`)
/// and redirect any connected duplicates to it. Older collapsed graphs could
/// acquire a second, empty macro entry/exit when opened because their sentinel
/// IDs differ from those generated for macros.
pub(crate) fn normalize_subgraph_boundary(
    graph: &mut GraphDescription,
    node_type: &str,
) -> Option<String> {
    let sentinel_ids: Vec<String> = graph
        .nodes
        .values()
        .filter(|node| node.node_type == node_type)
        .map(|node| node.id.clone())
        .collect();
    let keep_id = sentinel_ids
        .iter()
        .max_by_key(|id| {
            graph
                .connections
                .iter()
                .filter(|c| {
                    c.source_node.as_str() == id.as_str() || c.target_node.as_str() == id.as_str()
                })
                .count()
        })?
        .clone();
    for duplicate in sentinel_ids
        .iter()
        .filter(|id| id.as_str() != keep_id.as_str())
    {
        for connection in &mut graph.connections {
            if connection.source_node == *duplicate {
                connection.source_node = keep_id.clone();
            }
            if connection.target_node == *duplicate {
                connection.target_node = keep_id.clone();
            }
        }
        graph.nodes.remove(duplicate);
    }
    Some(keep_id)
}

/// The document type string of a pin type, as the interface stores it.
pub(crate) fn type_string(data_type: &DataType) -> String {
    match data_type {
        DataType::Exec => "execution".to_string(),
        DataType::Data(info) => info.type_string.clone(),
    }
}

pub(crate) fn pin_instance(id: &str, name: &str, data_type: DataType, kind: PinType) -> PinInstance {
    PinInstance {
        id: id.to_string(),
        pin: Pin {
            id: id.to_string(),
            name: name.to_string(),
            data_type,
            pin_type: kind,
        },
    }
}

fn interface_pins(pins: &[graphy_blueprint::SubGraphPin], kind: PinType) -> Vec<PinInstance> {
    pins.iter()
        .map(|p| {
            pin_instance(
                &p.id,
                &p.name,
                crate::canvas_profile::datatype_from_str(&p.data_type.to_string()),
                kind,
            )
        })
        .collect()
}

/// The inputs and outputs a call node of `definition` shows.
pub(crate) fn call_node_pins(
    definition: &graphy_blueprint::SubGraph,
) -> (Vec<PinInstance>, Vec<PinInstance>) {
    (
        interface_pins(&definition.interface.inputs, PinType::Input),
        interface_pins(&definition.interface.outputs, PinType::Output),
    )
}

fn pins_match(a: &[PinInstance], b: &[PinInstance]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.id == y.id && x.pin.name == y.pin.name && x.pin.data_type == y.pin.data_type
        })
}

/// Commands that bring the macro's Entry and Exit nodes in line with its
/// interface, creating them when absent. User nodes are untouched.
pub(crate) fn boundary_sync_commands(
    graph: &GraphDescription,
    macro_def: &graphy_blueprint::SubGraph,
) -> Vec<GraphCommand> {
    let entry_outputs = interface_pins(&macro_def.interface.inputs, PinType::Output);
    let exit_inputs = interface_pins(&macro_def.interface.outputs, PinType::Input);
    let mut commands = Vec::new();

    let find = |kind: &str| {
        let mut ids: Vec<&String> = graph
            .nodes
            .values()
            .filter(|n| n.node_type == kind)
            .map(|n| &n.id)
            .collect();
        ids.sort();
        ids.first().and_then(|id| graph.nodes.get(*id))
    };

    match find("macro_entry") {
        Some(node) if !pins_match(&node.outputs, &entry_outputs) => {
            commands.push(GraphCommand::SetPins {
                node: node.id.clone(),
                inputs: node.inputs.clone(),
                outputs: entry_outputs,
            })
        }
        Some(_) => {}
        None => {
            let mut node = NodeInstance::new(
                format!("macro_entry_{}", macro_def.id),
                "macro_entry",
                Position::new(60.0, 180.0),
            );
            node.outputs = entry_outputs;
            commands.push(GraphCommand::AddNode(node));
        }
    }
    match find("macro_exit") {
        Some(node) if !pins_match(&node.inputs, &exit_inputs) => {
            commands.push(GraphCommand::SetPins {
                node: node.id.clone(),
                inputs: exit_inputs,
                outputs: node.outputs.clone(),
            })
        }
        Some(_) => {}
        None => {
            let mut node = NodeInstance::new(
                format!("macro_exit_{}", macro_def.id),
                "macro_exit",
                Position::new(820.0, 180.0),
            );
            node.inputs = exit_inputs;
            commands.push(GraphCommand::AddNode(node));
        }
    }
    commands
}

/// Commands that re-pin every call node of `macro_def` in `graph`.
pub(crate) fn instance_sync_commands(
    graph: &GraphDescription,
    macro_def: &graphy_blueprint::SubGraph,
    subgraphs: &[graphy_blueprint::SubGraph],
) -> Vec<GraphCommand> {
    let macro_ref = SubGraphReference::new(&macro_def.id, graphy_blueprint::SubGraphKind::Macro);
    let (inputs, outputs) = call_node_pins(macro_def);
    let mut ids: Vec<&String> = graph.nodes.keys().collect();
    ids.sort();
    ids.into_iter()
        .filter_map(|id| graph.nodes.get(id))
        .filter(|node| {
            crate::core::subgraph_ref::definition_id_matches(&node.node_type, &macro_ref, subgraphs)
                && crate::core::types::NodeType::of(&node.node_type)
                    == crate::core::types::NodeType::SubGraphCall
        })
        .filter(|node| !pins_match(&node.inputs, &inputs) || !pins_match(&node.outputs, &outputs))
        .map(|node| GraphCommand::SetPins {
            node: node.id.clone(),
            inputs: inputs.clone(),
            outputs: outputs.clone(),
        })
        .collect()
}

/// Applies commands to a tab document that has no canvas yet.
pub(crate) fn apply_to_doc(doc: &mut TabDoc, commands: &[GraphCommand]) {
    for command in commands {
        let _ = command.apply(&mut doc.graph);
    }
}

impl BlueprintEditorPanel {
    // ─── Queries ──────────────────────────────────────────────────────────────

    /// Returns the macro ID currently being edited in the active tab, if any.
    pub fn current_editing_macro_id(&self) -> Option<&str> {
        let tab = self.open_tabs.get(self.active_tab_index)?;
        if !tab.is_main {
            Some(tab.id.as_str())
        } else {
            None
        }
    }

    /// True when the given macro would be nested inside itself in the active tab.
    pub fn would_nest_macro(&self, macro_id: &str) -> bool {
        self.current_editing_macro_id() == Some(macro_id)
    }

    // ─── Opening macros ───────────────────────────────────────────────────────

    /// Open a local macro for editing in a new tab (or switch to it if already open).
    pub fn open_local_macro(
        &mut self,
        macro_id: String,
        macro_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Switch to existing tab if open.
        if let Some(index) = self.open_tabs.iter().position(|tab| tab.id == macro_id) {
            self.switch_to_tab(index, window, cx);
            return;
        }

        // Materialize the saved graph before opening the tab. Collapsed graphs
        // are stored with their body in the same subgraph format as macros;
        // starting from an empty graph made the call node look like it had no
        // contents when opened.
        let Some(definition) = self
            .subgraphs
            .iter()
            .find(|definition| definition.id == macro_id)
            .cloned()
        else {
            tracing::warn!(
                "Could not open subgraph '{}': definition was not found",
                macro_name
            );
            return;
        };
        let mut doc = Self::tab_doc_from_authored(&definition.graph);
        normalize_subgraph_boundary(&mut doc.graph, "macro_entry");
        normalize_subgraph_boundary(&mut doc.graph, "macro_exit");
        // Seed entry/exit nodes from the interface (macros only; collapsed
        // graphs keep the sentinels they were created with).
        if definition.kind == graphy_blueprint::SubGraphKind::Macro {
            let commands = boundary_sync_commands(&doc.graph, &definition);
            apply_to_doc(&mut doc, &commands);
        }

        self.open_tabs
            .push(GraphTab::new_local_macro(macro_id, macro_name, doc));
        self.active_tab_index = self.open_tabs.len() - 1;
        self.sync_profile_data();
        self.graph_workspace_tabs_dirty = true;
        self.refresh_graph_workspace_tabs(window, cx);
        cx.notify();
    }

    /// Open a global/engine library macro.
    pub fn open_global_macro(
        &mut self,
        macro_id: String,
        macro_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.open_tabs.iter().position(|tab| tab.id == macro_id) {
            self.switch_to_tab(index, window, cx);
            return;
        }
        if let Some(lib_id) = self.get_macro_library_id(&macro_id) {
            self.request_open_engine_library(
                lib_id,
                "Engine Library".to_string(),
                Some(macro_id),
                Some(macro_name),
                cx,
            );
        }
    }

    /// Open the graph backing a placed macro instance.
    ///
    /// Local macros open/switch to a local macro tab.
    /// Library macros route through `OpenEngineLibraryRequest`.
    pub fn open_macro_from_instance(
        &mut self,
        macro_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.subgraphs.iter().find(|m| m.id == macro_id) {
            self.open_local_macro(local.id.clone(), local.name.clone(), window, cx);
            return;
        }

        if let Some(lib_id) = self.get_macro_library_id(macro_id) {
            let macro_name = self
                .library_manager
                .get_libraries()
                .get(&lib_id)
                .and_then(|lib| {
                    lib.subgraphs
                        .iter()
                        .find(|sg| sg.id == macro_id)
                        .map(|sg| sg.name.clone())
                })
                .unwrap_or_else(|| "Macro".to_string());

            self.request_open_engine_library(
                lib_id,
                "Engine Library".to_string(),
                Some(macro_id.to_string()),
                Some(macro_name),
                cx,
            );
        }
    }

    /// Return the library ID that owns `macro_id`, or `None` if it is a local macro.
    pub fn get_macro_library_id(&self, macro_id: &str) -> Option<String> {
        if self.subgraphs.iter().any(|m| m.id == macro_id) {
            return None;
        }
        self.library_manager
            .get_libraries()
            .iter()
            .find(|(_, lib)| lib.subgraphs.iter().any(|sg| sg.id == macro_id))
            .map(|(id, _)| id.clone())
    }

    /// Emit an `OpenEngineLibraryRequest` event.
    pub fn request_open_engine_library(
        &self,
        library_id: String,
        library_name: String,
        macro_id: Option<String>,
        macro_name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(crate::OpenEngineLibraryRequest {
            library_id,
            library_name,
            macro_id,
            macro_name,
        });
    }

    // ─── Creating macros ──────────────────────────────────────────────────────

    /// Create a new empty local macro and open it for editing.
    pub fn create_new_local_macro(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let macro_count = self
            .subgraphs
            .iter()
            .filter(|subgraph| subgraph.kind == graphy_blueprint::SubGraphKind::Macro)
            .count();
        let macro_name = format!("Macro {}", macro_count + 1);
        let macro_id = uuid::Uuid::new_v4().to_string();

        let macro_def = graphy_blueprint::SubGraph {
            id: macro_id.clone(),
            kind: graphy_blueprint::SubGraphKind::Macro,
            name: macro_name.clone(),
            description: "New macro".to_string(),
            graph: graphy_blueprint::GraphDescription::new(&macro_name),
            interface: graphy_blueprint::SubGraphInterface {
                inputs: Vec::new(),
                outputs: Vec::new(),
            },
            metadata: graphy_blueprint::SubGraphMetadata {
                created_at: chrono::Utc::now().to_rfc3339(),
                modified_at: chrono::Utc::now().to_rfc3339(),
                author: Some(String::new()),
                tags: Vec::new(),
            },
            macro_config: graphy_blueprint::MacroConfiguration::default(),
        };

        self.subgraphs.push(macro_def);
        self.open_local_macro(macro_id, macro_name, window, cx);
        self.invalidate_palette(cx);
    }

    /// Rename a local macro in-place.
    pub fn rename_local_macro(&mut self, macro_id: &str, new_name: String, cx: &mut Context<Self>) {
        if let Some(m) = self
            .subgraphs
            .iter_mut()
            .find(|m| m.id == macro_id && m.kind == graphy_blueprint::SubGraphKind::Macro)
        {
            m.name = new_name.clone();
        } else {
            return;
        }
        // Update the tab name too.
        if let Some(tab) = self.open_tabs.iter_mut().find(|t| t.id == macro_id) {
            tab.name = new_name;
        }
        self.invalidate_palette(cx);
        cx.notify();
    }

    /// Delete a local macro and all its open tabs.
    pub fn delete_local_macro(&mut self, macro_id: &str, cx: &mut Context<Self>) {
        if !self
            .subgraphs
            .iter()
            .any(|m| m.id == macro_id && m.kind == graphy_blueprint::SubGraphKind::Macro)
        {
            return;
        }
        self.subgraphs.retain(|m| m.id != macro_id);
        let before = self.open_tabs.len();
        self.open_tabs.retain(|t| t.id != macro_id);
        if self.open_tabs.len() < before {
            self.active_tab_index = self
                .active_tab_index
                .min(self.open_tabs.len().saturating_sub(1));
        }
        self.invalidate_palette(cx);
        cx.notify();
    }

    // ─── Interface pin management ─────────────────────────────────────────────

    /// Add a pin to a local macro's interface.
    ///
    /// `is_input` — true for an input pin on the macro instance (exposed via
    /// the Macro Entry node inside the graph), false for an output.
    pub fn add_macro_pin(
        &mut self,
        macro_id: &str,
        pin_name: String,
        type_str: String,
        is_input: bool,
        cx: &mut Context<Self>,
    ) {
        if !self
            .subgraphs
            .iter()
            .any(|m| m.id == macro_id && m.kind == graphy_blueprint::SubGraphKind::Macro)
        {
            return;
        }
        let pin = graphy_blueprint::SubGraphPin {
            id: uuid::Uuid::new_v4().to_string(),
            name: pin_name,
            data_type: graphy_blueprint::DataType::from_type_str(&type_str),
            description: None,
            default_value: None,
            is_instance_editable: false,
            category: None,
        };

        if let Some(m) = self.subgraphs.iter_mut().find(|m| m.id == macro_id) {
            if is_input {
                m.interface.inputs.push(pin);
            } else {
                m.interface.outputs.push(pin);
            }
        }

        let macro_id = macro_id.to_string();
        self.sync_entry_exit_in_active_graph(&macro_id, cx);
        self.sync_all_macro_instances(&macro_id, cx);
        self.invalidate_palette(cx);
    }

    /// Remove a pin from a local macro's interface.
    pub fn remove_macro_pin(
        &mut self,
        macro_id: &str,
        pin_id: &str,
        is_input: bool,
        cx: &mut Context<Self>,
    ) {
        if !self
            .subgraphs
            .iter()
            .any(|m| m.id == macro_id && m.kind == graphy_blueprint::SubGraphKind::Macro)
        {
            return;
        }
        if let Some(m) = self.subgraphs.iter_mut().find(|m| m.id == macro_id) {
            if is_input {
                m.interface.inputs.retain(|p| p.id != pin_id);
            } else {
                m.interface.outputs.retain(|p| p.id != pin_id);
            }
        }

        let macro_id = macro_id.to_string();
        self.sync_entry_exit_in_active_graph(&macro_id, cx);
        self.sync_all_macro_instances(&macro_id, cx);
        self.invalidate_palette(cx);
    }

    // ─── Entry / Exit synchronisation ────────────────────────────────────────

    /// Ensure the Macro Entry and Macro Exit nodes in the macro's graph reflect
    /// the current interface of `macro_id`. Only runs when the active tab
    /// belongs to this macro; user nodes are preserved.
    pub fn sync_entry_exit_in_active_graph(&mut self, macro_id: &str, cx: &mut Context<Self>) {
        let Some(tab) = self.open_tabs.get(self.active_tab_index) else {
            return;
        };
        if tab.id != macro_id || tab.is_main {
            return;
        }
        let Some(macro_def) = self
            .subgraphs
            .iter()
            .find(|m| m.id == macro_id && m.kind == graphy_blueprint::SubGraphKind::Macro)
            .cloned()
        else {
            return;
        };
        self.sync_profile_data();
        if let Some(canvas) = self.active_canvas().cloned() {
            // Called from inside canvas updates (the "Add pin" button), so
            // defer the canvas edit until the current update has unwound.
            cx.defer(move |cx| {
                canvas.update(cx, |canvas, cx| {
                    let commands = boundary_sync_commands(canvas.graph(cx), &macro_def);
                    canvas.apply_system("Sync macro interface", commands, cx);
                });
            });
        } else if let Some(doc) = self
            .open_tabs
            .get_mut(self.active_tab_index)
            .and_then(|tab| tab.pending.as_mut())
        {
            let commands = boundary_sync_commands(&doc.graph, &macro_def);
            apply_to_doc(doc, &commands);
        }
        cx.notify();
    }

    // ─── Subgraph call placement ───────────────────────────────────────────────

    /// Create a subgraph call node at `position` for the local macro identified
    /// by `macro_id` on the active canvas.
    pub fn create_macro_instance_node(
        &mut self,
        macro_id: String,
        position: graphy_render::Vec2,
        cx: &mut Context<Self>,
    ) {
        if let Some(canvas) = self.active_canvas().cloned() {
            cx.defer(move |cx| {
                canvas.update(cx, |canvas, cx| {
                    canvas.create_macro_instance_node(macro_id, position, cx);
                });
            });
        }
    }

    /// Update the pins of every macro call node that references `macro_id`
    /// across all tabs (live canvases and tabs whose canvas does not exist yet).
    pub fn sync_all_macro_instances(&mut self, macro_id: &str, cx: &mut Context<Self>) {
        let Some(macro_def) = self
            .subgraphs
            .iter()
            .find(|m| m.id == macro_id && m.kind == graphy_blueprint::SubGraphKind::Macro)
            .cloned()
        else {
            return;
        };
        self.sync_profile_data();
        let subgraphs = self.subgraphs.clone();

        for tab in self.open_tabs.iter_mut() {
            if let Some(doc) = tab.pending.as_mut() {
                let commands = instance_sync_commands(&doc.graph, &macro_def, &subgraphs);
                apply_to_doc(doc, &commands);
            }
        }

        // Live canvases: deferred to avoid re-entrancy, as this can be called
        // from within a canvas update (add/remove macro pin buttons).
        for (_, canvas) in self.graph_panels.clone() {
            let (macro_def, subgraphs) = (macro_def.clone(), subgraphs.clone());
            cx.defer(move |cx| {
                canvas.update(cx, |canvas, cx| {
                    let commands = instance_sync_commands(canvas.graph(cx), &macro_def, &subgraphs);
                    canvas.apply_system("Sync macro instances", commands, cx);
                });
            });
        }
    }

    // ─── Palette invalidation ─────────────────────────────────────────────────

    /// Notify the active canvas's quick-palette to rebuild so local macros appear.
    pub fn invalidate_palette(&self, cx: &mut Context<Self>) {
        if let Some(canvas) = self.active_canvas().cloned() {
            cx.defer(move |cx| {
                canvas.update(cx, |canvas, cx| {
                    let v = canvas.quick_palette_view.clone();
                    cx.defer(move |cx| {
                        v.update(cx, |view, cx| view.rebuild_items(cx));
                    });
                });
            });
        }
    }
}


// ─── Canvas-side macro operations ────────────────────────────────────────────

impl GraphCanvasPanel {
    /// Place a subgraph call node built from an already-resolved macro definition.
    pub fn create_macro_instance_node(
        &mut self,
        macro_id: String,
        position: graphy_render::Vec2,
        cx: &mut Context<Self>,
    ) {
        let macro_def = self.panel.upgrade().and_then(|p| {
            p.read(cx)
                .subgraphs
                .iter()
                .find(|m| m.id == macro_id && m.kind == graphy_blueprint::SubGraphKind::Macro)
                .cloned()
        });
        let Some(macro_def) = macro_def else { return };
        let (inputs, outputs) = call_node_pins(&macro_def);
        let mut node = NodeInstance::new(
            uuid::Uuid::new_v4().to_string(),
            SubGraphReference::new(macro_id, graphy_blueprint::SubGraphKind::Macro).encode(),
            Position::new(f64::from(position.x), f64::from(position.y)),
        );
        node.inputs = inputs;
        node.outputs = outputs;
        self.add_node(node, cx);
    }

    /// Called when the user drops a `MacroDrag` payload onto this canvas.
    pub fn finish_dragging_macro(&mut self, window_pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.dragging_macro.take() else {
            return;
        };
        let graph_pos = self.canvas.read(cx).window_to_graph(window_pos);
        self.create_macro_instance_node(drag.macro_id, graph_pos, cx);
    }

    /// Cancel a pending macro drag without creating a node.
    pub fn cancel_dragging_macro(&mut self, cx: &mut Context<Self>) {
        self.dragging_macro = None;
        cx.notify();
    }
}
