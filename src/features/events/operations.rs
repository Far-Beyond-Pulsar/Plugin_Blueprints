//! Event operations — CRUD and sync for custom events.
//!
//! Mirrors the macro operations pattern: event definitions live on the panel
//! (`local_event_defs`) and graph nodes (On / Dispatch) are synced from them.

use crate::core::graph::{CustomEventField, EventDefinition};
use crate::core::types::NodeType;
use crate::editor::graph_canvas_panel::GraphCanvasPanel;
use crate::editor::panel::BlueprintEditorPanel;
use crate::features::macros::operations::{apply_to_doc, pin_instance};
use gpui::*;
use graphy::{DataType, GraphCommand, GraphDescription, NodeInstance, PinInstance, PinType, Position};
use uuid::Uuid;

impl BlueprintEditorPanel {
    // ─── CRUD ──────────────────────────────────────────────────────────────────

    pub fn create_event_def(&mut self, name: String, return_type: String) -> String {
        let uid = Uuid::new_v4().to_string();

        let def = EventDefinition {
            uid: uid.clone(),
            name,
            fields: Vec::new(),
            return_type,
        };
        self.local_event_defs.push(def);
        self.selected_event = Some(self.local_event_defs.len() - 1);
        self.is_dirty = true;
        uid
    }

    pub fn delete_event_def(&mut self, uid: &str) {
        let was_selected = self
            .selected_event
            .and_then(|i| self.local_event_defs.get(i))
            .map(|d| d.uid.as_str())
            == Some(uid);

        self.local_event_defs.retain(|d| d.uid != uid);
        if was_selected {
            self.selected_event = None;
        }
        self.is_dirty = true;
    }

    pub fn rename_event_def(&mut self, uid: &str, new_name: String) {
        if let Some(def) = self.local_event_defs.iter_mut().find(|d| d.uid == uid) {
            def.name = new_name;
            self.is_dirty = true;
        }
    }

    pub fn add_event_field(&mut self, uid: &str, name: String, type_name: String) {
        if let Some(def) = self.local_event_defs.iter_mut().find(|d| d.uid == uid) {
            def.fields.push(CustomEventField { name, type_name });
            self.is_dirty = true;
        }
    }

    pub fn remove_event_field(&mut self, uid: &str, field_name: &str) {
        if let Some(def) = self.local_event_defs.iter_mut().find(|d| d.uid == uid) {
            def.fields.retain(|f| f.name != field_name);
            self.is_dirty = true;
        }
    }

    pub fn set_event_return_type(&mut self, uid: &str, return_type: String) {
        if let Some(def) = self.local_event_defs.iter_mut().find(|d| d.uid == uid) {
            def.return_type = return_type;
            self.is_dirty = true;
        }
    }

    // ─── Helpers ───────────────────────────────────────────────────────────────

    /// Look up an event definition by uid (local events only; library events
    /// not supported yet).
    pub fn find_event_def(&self, uid: &str) -> Option<&EventDefinition> {
        self.local_event_defs.iter().find(|d| d.uid == uid)
    }

    /// The output pins of an event's On node.
    pub fn event_output_pins(def: &EventDefinition) -> Vec<PinInstance> {
        let mut pins = vec![pin_instance("Body", "Body", DataType::Exec, PinType::Output)];
        for field in &def.fields {
            pins.push(pin_instance(
                &field.name,
                &field.name,
                crate::canvas_profile::datatype_from_str(&field.type_name),
                PinType::Output,
            ));
        }
        pins
    }

    /// The input pins of an event's Dispatch node.
    pub fn event_dispatch_input_pins(def: &EventDefinition) -> Vec<PinInstance> {
        let mut pins = vec![pin_instance("exec", "", DataType::Exec, PinType::Input)];
        for field in &def.fields {
            pins.push(pin_instance(
                &field.name,
                &field.name,
                crate::canvas_profile::datatype_from_str(&field.type_name),
                PinType::Input,
            ));
        }
        pins
    }

    // ─── Sync (command builders over a graph) ──────────────────────────────────

    fn same_pins(a: &[PinInstance], b: &[PinInstance]) -> bool {
        a.len() == b.len()
            && a.iter().zip(b).all(|(x, y)| {
                x.id == y.id && x.pin.name == y.pin.name && x.pin.data_type == y.pin.data_type
            })
    }

    /// Commands ensuring the On node for `def` exists in `graph` with the
    /// correct pins.
    pub fn event_on_node_commands(
        def: &EventDefinition,
        graph: &GraphDescription,
    ) -> Vec<GraphCommand> {
        let on_type = format!("custom_event:{}", def.uid);
        let on_inputs = vec![pin_instance(
            "__return__",
            "",
            crate::canvas_profile::datatype_from_str("?"),
            PinType::Input,
        )];
        let outputs = Self::event_output_pins(def);
        let existing = graph
            .nodes
            .values()
            .filter(|n| n.node_type == on_type)
            .min_by(|a, b| a.id.cmp(&b.id));
        match existing {
            Some(node) => {
                let mut commands = Vec::new();
                if !Self::same_pins(&node.inputs, &on_inputs)
                    || !Self::same_pins(&node.outputs, &outputs)
                {
                    commands.push(GraphCommand::SetPins {
                        node: node.id.clone(),
                        inputs: on_inputs,
                        outputs,
                    });
                }
                if node.properties.get("event_uid").and_then(|v| v.as_str())
                    != Some(def.uid.as_str())
                {
                    commands.push(GraphCommand::SetProperty {
                        node: node.id.clone(),
                        key: "event_uid".to_string(),
                        value: Some(graphy::JsonValue::String(def.uid.clone())),
                    });
                }
                commands
            }
            None => {
                let mut node = NodeInstance::new(
                    Uuid::new_v4().to_string(),
                    on_type,
                    Position::new(200.0, 200.0),
                );
                node.inputs = on_inputs;
                node.outputs = outputs;
                node.set_property("event_uid", def.uid.clone());
                vec![GraphCommand::AddNode(node)]
            }
        }
    }

    /// Commands bringing every Dispatch node of `def` in line with its pins.
    pub fn event_dispatch_commands(
        def: &EventDefinition,
        graph: &GraphDescription,
    ) -> Vec<GraphCommand> {
        let dispatch_type = format!("custom_event_dispatch:{}", def.uid);
        let inputs = Self::event_dispatch_input_pins(def);
        let mut ids: Vec<&String> = graph
            .nodes
            .values()
            .filter(|n| n.node_type == dispatch_type && !Self::same_pins(&n.inputs, &inputs))
            .map(|n| &n.id)
            .collect();
        ids.sort();
        ids.into_iter()
            .map(|id| GraphCommand::SetPins {
                node: id.clone(),
                inputs: inputs.clone(),
                outputs: graph.nodes[id].outputs.clone(),
            })
            .collect()
    }

    /// Commands removing On nodes of events that no longer exist.
    fn event_cleanup_commands(
        graph: &GraphDescription,
        active_uids: &[String],
    ) -> Vec<GraphCommand> {
        let mut ids: Vec<&String> = graph
            .nodes
            .values()
            .filter(|n| {
                NodeType::of(&n.node_type) == NodeType::CustomEvent
                    && n.node_type
                        .strip_prefix("custom_event:")
                        .is_some_and(|uid| !active_uids.iter().any(|a| a == uid))
            })
            .map(|n| &n.id)
            .collect();
        ids.sort();
        ids.into_iter()
            .map(|id| GraphCommand::RemoveNode { id: id.clone() })
            .collect()
    }

    /// All commands that make `graph` agree with the event definitions.
    fn event_sync_commands(
        defs: &[EventDefinition],
        graph: &GraphDescription,
    ) -> Vec<GraphCommand> {
        let active: Vec<String> = defs.iter().map(|d| d.uid.clone()).collect();
        let mut working = graph.clone();
        let mut all = Vec::new();
        for def in defs {
            for commands in [
                Self::event_on_node_commands(def, &working),
                Self::event_dispatch_commands(def, &working),
            ] {
                for command in commands {
                    if command.apply(&mut working).is_ok() {
                        all.push(command);
                    }
                }
            }
        }
        all.extend(Self::event_cleanup_commands(&working, &active));
        all
    }

    /// Ensure the On node for `uid` exists in the active graph (new events).
    pub fn sync_event_to_active_graph(&mut self, uid: &str, cx: &mut Context<Self>) {
        if let Some(tab_id) = self.open_tabs.get(self.active_tab_index).map(|t| t.id.clone()) {
            self.sync_event_to_tab(uid, &tab_id, cx);
        }
    }

    /// Ensure the On node for `uid` exists in the graph of tab `tab_id`.
    pub fn sync_event_to_tab(&mut self, uid: &str, tab_id: &str, cx: &mut Context<Self>) {
        let Some(def) = self.find_event_def(uid).cloned() else {
            return;
        };
        self.sync_profile_data();
        let canvas = self
            .graph_panels
            .iter()
            .find(|(id, _)| id == tab_id)
            .map(|(_, canvas)| canvas.clone());
        if let Some(canvas) = canvas {
            cx.defer(move |cx| {
                canvas.update(cx, |canvas, cx| {
                    let commands =
                        BlueprintEditorPanel::event_on_node_commands(&def, canvas.graph(cx));
                    canvas.apply_system("Sync event", commands, cx);
                });
            });
        } else if let Some(doc) = self
            .open_tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id)
            .and_then(|tab| tab.pending.as_mut())
        {
            let commands = Self::event_on_node_commands(&def, &doc.graph);
            apply_to_doc(doc, &commands);
        }
        cx.notify();
    }

    /// Full sync: ensure all event On/Dispatch nodes match their definitions
    /// across all open tabs and live canvases.
    pub fn sync_all_events(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let defs: Vec<EventDefinition> = self.local_event_defs.clone();
        self.sync_profile_data();

        for tab in self.open_tabs.iter_mut() {
            if let Some(doc) = tab.pending.as_mut() {
                let commands = Self::event_sync_commands(&defs, &doc.graph);
                apply_to_doc(doc, &commands);
            }
        }

        for (_, canvas) in self.graph_panels.clone() {
            let defs = defs.clone();
            cx.defer(move |cx| {
                canvas.update(cx, |canvas, cx| {
                    let commands =
                        BlueprintEditorPanel::event_sync_commands(&defs, canvas.graph(cx));
                    canvas.apply_system("Sync events", commands, cx);
                    canvas.refresh_presentation(cx);
                });
            });
        }
        cx.notify();
    }
}

impl GraphCanvasPanel {
    /// Create a Dispatch node for an event definition (called from palette).
    pub fn create_custom_event_dispatch_node(
        &mut self,
        uid: String,
        position: graphy_render::Vec2,
        cx: &mut Context<Self>,
    ) {
        let Some(panel) = self.panel.upgrade() else {
            return;
        };
        let inputs = panel.read(cx).find_event_def(&uid).map_or_else(
            || vec![pin_instance("exec", "", DataType::Exec, PinType::Input)],
            BlueprintEditorPanel::event_dispatch_input_pins,
        );
        let mut node = NodeInstance::new(
            Uuid::new_v4().to_string(),
            format!("custom_event_dispatch:{uid}"),
            Position::new(f64::from(position.x), f64::from(position.y)),
        );
        node.inputs = inputs;
        node.set_property("event_uid", uid);
        self.add_node(node, cx);
    }
}
