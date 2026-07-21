//! Event operations — CRUD and sync for custom events.
//!
//! Mirrors the macro operations pattern: event definitions live on the panel
//! (`local_event_defs`) and graph nodes (On / Dispatch) are synced from them.

use crate::core::graph::{CustomEventField, EventDefinition};
use crate::core::types::{BlueprintNode, NodeType, Pin, PinDataType as DataType, PinType};
use crate::editor::panel::BlueprintEditorPanel;
use crate::editor::workspace_panels::GraphCanvasPanel;
use gpui::*;
use std::collections::HashMap;
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

    /// Build pin lists from an event definition for On and Dispatch nodes.
    pub fn event_output_pins(def: &EventDefinition) -> Vec<Pin> {
        let mut pins = vec![Pin {
            id: "Body".to_string(),
            name: "Body".to_string(),
            pin_type: PinType::Output,
            data_type: DataType::execution(),
        }];
        for field in &def.fields {
            pins.push(Pin {
                id: field.name.clone(),
                name: field.name.clone(),
                pin_type: PinType::Output,
                data_type: DataType::from_type_str(&field.type_name),
            });
        }
        pins
    }

    pub fn event_dispatch_input_pins(def: &EventDefinition) -> Vec<Pin> {
        let mut pins = vec![Pin {
            id: "exec".to_string(),
            name: String::new(),
            pin_type: PinType::Input,
            data_type: DataType::execution(),
        }];
        for field in &def.fields {
            pins.push(Pin {
                id: field.name.clone(),
                name: field.name.clone(),
                pin_type: PinType::Input,
                data_type: DataType::from_type_str(&field.type_name),
            });
        }
        pins
    }

    // ─── Sync (associated functions, not &self methods) ────────────────────────

    /// Ensure the On node for `uid` exists in `graph` with the correct pins.
    /// Pass the event definition explicitly to avoid borrow conflicts.
    pub fn sync_event_on_node_for_def(
        def: &EventDefinition,
        uid: &str,
        graph: &mut crate::core::graph::BlueprintGraph,
    ) -> Option<String> {
        let on_def_id = format!("custom_event:{}", uid);

        let existing_id = graph
            .nodes
            .iter()
            .find(|n| n.definition_id == on_def_id)
            .map(|n| n.id.clone());

        let on_inputs = vec![Pin {
            id: "__return__".to_string(),
            name: String::new(),
            pin_type: PinType::Input,
            data_type: DataType::from_type_str("?"),
        }];
        let outputs = Self::event_output_pins(def);

        if let Some(ref nid) = existing_id {
            if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == *nid) {
                node.title = format!("On {}", def.name);
                node.icon = "📡".to_string();
                node.definition_id = on_def_id;
                node.node_type = NodeType::CustomEvent;
                node.inputs = on_inputs;
                node.outputs = outputs;
                node.description = format!("Custom event listener for '{}'", uid);
                node.properties.insert("event_uid".to_string(), uid.to_string());
            }
            existing_id
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            graph.nodes.push(BlueprintNode {
                id: id.clone(),
                definition_id: on_def_id,
                title: format!("On {}", def.name),
                icon: "📡".to_string(),
                node_type: NodeType::CustomEvent,
                position: Point::new(200.0, 200.0),
                size: gpui::Size::new(240.0, 60.0),
                inputs: on_inputs,
                outputs,
                properties: {
                    let mut m = HashMap::new();
                    m.insert("event_uid".to_string(), uid.to_string());
                    m
                },
                is_selected: false,
                description: format!("Custom event listener for '{}'", uid),
                color: None,
            });
            Some(id)
        }
    }

    /// Sync all Dispatch nodes referencing `uid` in `graph` with the event
    /// definition's pins.  Does NOT take `&self`.
    pub fn sync_dispatch_nodes_for_def(
        def: &EventDefinition,
        uid: &str,
        graph: &mut crate::core::graph::BlueprintGraph,
    ) {
        let dispatch_def_id = format!("custom_event_dispatch:{}", uid);
        let inputs = Self::event_dispatch_input_pins(def);

        for node in graph.nodes.iter_mut() {
            if node.definition_id == dispatch_def_id && node.node_type == NodeType::CustomEventDispatch {
                node.inputs = inputs.clone();
                node.title = format!("Dispatch {}", def.name);
                node.properties.insert("event_uid".to_string(), uid.to_string());
            }
        }
    }

    /// Full sync: ensure all event On/Dispatch nodes match their definitions
    /// across all open tabs and live canvases.
    pub fn sync_all_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let defs: Vec<EventDefinition> = self.local_event_defs.clone();
        let active_uids: Vec<String> = defs.iter().map(|d| d.uid.clone()).collect();

        // Sync tab snapshots (graph is separate from self)
        for tab in self.open_tabs.iter_mut() {
            for def in &defs {
                Self::sync_event_on_node_for_def(def, &def.uid, &mut tab.graph);
                Self::sync_dispatch_nodes_for_def(def, &def.uid, &mut tab.graph);
            }
            tab.graph.nodes.retain(|n| {
                if n.node_type == NodeType::CustomEvent {
                    if let Some(uid) = n.definition_id.strip_prefix("custom_event:") {
                        return active_uids.contains(&uid.to_string());
                    }
                }
                true
            });
        }

        // Sync live canvas graphs (deferred)
        let canvases: Vec<Entity<GraphCanvasPanel>> =
            self.graph_panels.iter().map(|(_, c)| c.clone()).collect();
        let defs2 = defs.clone();
        let active_uids2 = active_uids.clone();
        cx.defer(move |cx| {
            for canvas in &canvases {
                canvas.update(cx, |canvas_panel, _cx| {
                    for def in &defs2 {
                        BlueprintEditorPanel::sync_event_on_node_for_def(def, &def.uid, &mut canvas_panel.graph);
                        BlueprintEditorPanel::sync_dispatch_nodes_for_def(def, &def.uid, &mut canvas_panel.graph);
                    }
                    canvas_panel.graph.nodes.retain(|n| {
                        if n.node_type == NodeType::CustomEvent {
                            if let Some(uid) = n.definition_id.strip_prefix("custom_event:") {
                                return active_uids2.contains(&uid.to_string());
                            }
                        }
                        true
                    });
                });
            }
        });

        if let Some(tab) = self.open_tabs.get(self.active_tab_index) {
            self.graph = tab.graph.clone();
        }

        cx.notify();
    }

    // ─── Convenience methods on &mut self ─────────────────────────────────────

    pub fn sync_event_on_node(
        &self,
        uid: &str,
        graph: &mut crate::core::graph::BlueprintGraph,
    ) -> Option<String> {
        let def = self.find_event_def(uid)?;
        Self::sync_event_on_node_for_def(def, uid, graph)
    }

    pub fn sync_event_dispatch_nodes(
        &self,
        uid: &str,
        graph: &mut crate::core::graph::BlueprintGraph,
    ) {
        let Some(def) = self.find_event_def(uid) else { return };
        Self::sync_dispatch_nodes_for_def(def, uid, graph);
    }

    pub fn remove_event_on_node(&self, uid: &str, graph: &mut crate::core::graph::BlueprintGraph) {
        let on_def_id = format!("custom_event:{}", uid);
        graph.nodes.retain(|n| n.definition_id != on_def_id);
    }
}

impl GraphCanvasPanel {
    /// Create a Dispatch node for an event definition (called from palette).
    pub fn create_custom_event_dispatch_node(
        &mut self,
        uid: String,
        position: Point<f32>,
        cx: &mut Context<Self>,
    ) {
        let inputs = if let Some(panel) = self.panel.upgrade() {
            panel.read(cx).find_event_def(&uid).map_or_else(
                || {
                    vec![Pin {
                        id: "exec".to_string(),
                        name: String::new(),
                        pin_type: PinType::Input,
                        data_type: DataType::execution(),
                    }]
                },
                |def| BlueprintEditorPanel::event_dispatch_input_pins(def),
            )
        } else {
            return;
        };

        let dispatch_def_id = format!("custom_event_dispatch:{}", uid);

        let node = BlueprintNode {
            id: uuid::Uuid::new_v4().to_string(),
            definition_id: dispatch_def_id,
            title: format!("Dispatch {}", uid),
            icon: "📡".to_string(),
            node_type: NodeType::CustomEventDispatch,
            position,
            size: gpui::Size::new(240.0, 60.0),
            inputs,
            outputs: Vec::new(),
            properties: {
                let mut m = HashMap::new();
                m.insert("event_uid".to_string(), uid);
                m
            },
            is_selected: false,
            description: String::new(),
            color: None,
        };
        self.add_node(node, cx);
    }
}
