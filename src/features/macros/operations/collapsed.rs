//! Collapse and expand private collapsed graph regions.

use super::*;
use crate::core::types::NodeType;
use graphy::Connection;
use std::collections::{HashMap, HashSet};

impl GraphCanvasPanel {
    /// Expand a private collapsed graph back into its containing graph.
    pub fn expand_collapsed_node(
        &mut self,
        node_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let graph = self.graph(cx).clone();
        let Some(call_node) = graph.nodes.get(&node_id).cloned() else {
            return;
        };
        let Some(graph_id) = SubGraphReference::id_from_definition_id(&call_node.node_type)
            .map(str::to_owned)
        else {
            return;
        };
        let Some(panel) = self.panel.upgrade() else {
            return;
        };
        let this_tab = self.id.clone();

        // The collapsed graph's body: its open tab if that still looks like a
        // collapsed graph, otherwise the saved definition.
        let editor = panel.read(cx);
        let Some(definition) = editor
            .subgraphs
            .iter()
            .find(|d| d.id == graph_id && d.kind == graphy_blueprint::SubGraphKind::Collapsed)
            .cloned()
        else {
            return;
        };
        let authored_inner = editor
            .tab_authored_graph(&graph_id, cx)
            .filter(|g| {
                let has = |ty: &str| g.nodes.values().any(|n| n.node_type == ty);
                let has_body = g
                    .nodes
                    .values()
                    .any(|n| n.node_type != "macro_entry" && n.node_type != "macro_exit");
                let has_collapsed_sentinels = g
                    .nodes
                    .values()
                    .any(|n| n.node_type == "macro_entry" && n.id.starts_with("collapsed_entry_"))
                    && g.nodes.values().any(|n| {
                        n.node_type == "macro_exit" && n.id.starts_with("collapsed_exit_")
                    });
                has("macro_entry") && has("macro_exit") && (has_body || has_collapsed_sentinels)
            })
            .unwrap_or_else(|| definition.graph.clone());
        let mut inner = BlueprintEditorPanel::tab_doc_from_authored(&authored_inner).graph;
        normalize_subgraph_boundary(&mut inner, "macro_entry");
        normalize_subgraph_boundary(&mut inner, "macro_exit");
        let find = |ty: &str| inner.nodes.values().find(|n| n.node_type == ty).map(|n| n.id.clone());
        let (Some(entry_id), Some(exit_id)) = (find("macro_entry"), find("macro_exit")) else {
            return;
        };

        // Wires that crossed the call node's pins, by pin.
        let mut incoming: HashMap<String, Vec<Connection>> = HashMap::new();
        let mut outgoing: HashMap<String, Vec<Connection>> = HashMap::new();
        for c in &graph.connections {
            if c.target_node == node_id {
                incoming.entry(c.target_pin.clone()).or_default().push(c.clone());
            }
            if c.source_node == node_id {
                outgoing.entry(c.source_pin.clone()).or_default().push(c.clone());
            }
        }

        // A collapsed instance can be duplicated like any other macro call.
        // Expanding a second instance must not insert the same node IDs as the
        // first expansion.
        let remap: HashMap<String, String> = inner
            .nodes
            .keys()
            .filter(|id| **id != entry_id && **id != exit_id)
            .map(|id| (id.clone(), uuid::Uuid::new_v4().to_string()))
            .collect();

        let mut commands = vec![GraphCommand::RemoveNode {
            id: node_id.clone(),
        }];
        let mut body: Vec<&NodeInstance> = inner
            .nodes
            .values()
            .filter(|n| n.id != entry_id && n.id != exit_id)
            .collect();
        body.sort_by(|a, b| a.id.cmp(&b.id));
        for node in &body {
            let mut node = (*node).clone();
            node.id = remap[&node.id].clone();
            node.position.x += call_node.position.x;
            node.position.y += call_node.position.y;
            commands.push(GraphCommand::AddNode(node));
        }
        let mapped = |id: &str| remap.get(id).cloned().unwrap_or_else(|| id.to_string());
        let mut connect = |source: (&str, &str), target: (&str, &str), like: &Connection| {
            let mut c = Connection::new(source.0, source.1, target.0, target.1, like.connection_type);
            c.id = Some(uuid::Uuid::new_v4().to_string());
            commands.push(GraphCommand::Connect(c));
        };
        for c in &inner.connections {
            if c.source_node == entry_id {
                for outer in incoming.get(&c.source_pin).into_iter().flatten() {
                    connect(
                        (&outer.source_node, &outer.source_pin),
                        (&mapped(&c.target_node), &c.target_pin),
                        c,
                    );
                }
            } else if c.target_node == exit_id {
                for outer in outgoing.get(&c.target_pin).into_iter().flatten() {
                    connect(
                        (&mapped(&c.source_node), &c.source_pin),
                        (&outer.target_node, &outer.target_pin),
                        c,
                    );
                }
            } else {
                connect(
                    (&mapped(&c.source_node), &c.source_pin),
                    (&mapped(&c.target_node), &c.target_pin),
                    c,
                );
            }
        }
        let restored: Vec<String> = remap.values().cloned().collect();

        // Collapsed definitions are private to their call sites. Keep the
        // definition while another instance still references it, and remove
        // its saved graph and tab after the last instance expands.
        let references = |ty: &str| SubGraphReference::id_from_definition_id(ty) == Some(graph_id.as_str());
        let editor = panel.read(cx);
        let other_in_this_graph = graph.nodes.values().any(|n| n.id != node_id && references(&n.node_type));
        let other_in_tabs = editor
            .graph_panels
            .iter()
            .filter(|(id, _)| *id != this_tab && *id != graph_id)
            .any(|(_, canvas)| {
                canvas
                    .read(cx)
                    .graph(cx)
                    .nodes
                    .values()
                    .any(|n| references(&n.node_type))
            })
            || editor.open_tabs.iter().filter(|t| t.id != this_tab).any(|t| {
                t.pending
                    .as_ref()
                    .is_some_and(|doc| doc.graph.nodes.values().any(|n| references(&n.node_type)))
            });
        let other_in_definitions = editor
            .subgraphs
            .iter()
            .filter(|s| s.id != graph_id)
            .any(|s| s.graph.nodes.values().any(|n| references(&n.node_type)));
        let remove_definition = !(other_in_this_graph || other_in_tabs || other_in_definitions);

        let _ = self.apply("Expand collapsed graph", commands, cx);
        self.canvas.update(cx, |canvas, cx| canvas.select_nodes(restored, cx));

        if remove_definition {
            panel.update(cx, |editor, panel_cx| {
                let active_tab_id = editor
                    .open_tabs
                    .get(editor.active_tab_index)
                    .map(|tab| tab.id.clone());
                editor.subgraphs.retain(|subgraph| subgraph.id != graph_id);
                editor.open_tabs.retain(|tab| tab.id != graph_id);
                editor.active_tab_index = active_tab_id
                    .and_then(|id| editor.open_tabs.iter().position(|tab| tab.id == id))
                    .unwrap_or_else(|| {
                        editor
                            .active_tab_index
                            .min(editor.open_tabs.len().saturating_sub(1))
                    });
                editor.graph_workspace_tabs_dirty = true;
                editor.is_dirty = true;
                editor.sync_profile_data();
                editor.invalidate_palette(panel_cx);
                editor.refresh_graph_workspace_tabs(window, panel_cx);
                panel_cx.notify();
            });
        }
    }

    /// Replace the selected nodes with a single call node backed by a private
    /// collapsed graph. Every edge crossing the selection boundary becomes an
    /// explicit graph interface pin, so the parent and nested graphs retain
    /// the same connections.
    pub fn collapse_selected_nodes(&mut self, cx: &mut Context<Self>) {
        use graphy_blueprint::{DataType as AuthoredType, SubGraphPin};

        let graph = self.graph(cx).clone();
        let selected: HashSet<String> = self
            .selected_nodes(cx)
            .into_iter()
            .filter(|id| graph.nodes.contains_key(id))
            .collect();
        if selected.is_empty() {
            return;
        }
        let mut moved: Vec<NodeInstance> = selected
            .iter()
            .filter_map(|id| graph.nodes.get(id).cloned())
            .collect();
        moved.sort_by(|a, b| a.id.cmp(&b.id));
        // Macro sentinels belong to their containing graph and cannot be
        // nested as ordinary nodes in a collapsed region.
        if moved
            .iter()
            .any(|n| matches!(NodeType::of(&n.node_type), NodeType::MacroEntry | NodeType::MacroExit))
        {
            return;
        }
        let rects: Vec<_> = {
            let canvas = self.canvas.read(cx);
            moved
                .iter()
                .filter_map(|n| canvas.scene().node(&n.id).map(|s| s.rect()))
                .collect()
        };
        if rects.is_empty() {
            return;
        }
        let min_x = rects.iter().map(|r| r.min.x).fold(f32::INFINITY, f32::min);
        let min_y = rects.iter().map(|r| r.min.y).fold(f32::INFINITY, f32::min);
        let max_x = rects.iter().map(|r| r.max.x).fold(f32::NEG_INFINITY, f32::max);
        let max_y = rects.iter().map(|r| r.max.y).fold(f32::NEG_INFINITY, f32::max);
        let graph_id = uuid::Uuid::new_v4().to_string();
        let graph_name = format!("Collapsed {}", selected.len());

        for node in &mut moved {
            node.position.x -= f64::from(min_x);
            node.position.y -= f64::from(min_y);
        }

        let crossing: Vec<Connection> = graph
            .connections
            .iter()
            .filter(|c| selected.contains(&c.source_node) != selected.contains(&c.target_node))
            .cloned()
            .collect();
        let mut internal_connections: Vec<Connection> = graph
            .connections
            .iter()
            .filter(|c| selected.contains(&c.source_node) && selected.contains(&c.target_node))
            .cloned()
            .collect();
        let mut parent_connections: Vec<Connection> = Vec::new();

        let entry_id = format!("collapsed_entry_{graph_id}");
        let exit_id = format!("collapsed_exit_{graph_id}");
        let instance_id = format!("collapsed_{graph_id}");
        let mut interface = graphy_blueprint::SubGraphInterface {
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        let mut entry_outputs = Vec::new();
        let mut exit_inputs = Vec::new();
        let mut instance_inputs = Vec::new();
        let mut instance_outputs = Vec::new();

        let new_connection = |s: (&str, &str), t: (&str, &str), like: &Connection| {
            let mut c = Connection::new(s.0, s.1, t.0, t.1, like.connection_type);
            c.id = Some(uuid::Uuid::new_v4().to_string());
            c
        };
        let pin_of = |node: &str, pin: &str, outputs: bool| -> Option<PinInstance> {
            let n = graph.nodes.get(node)?;
            let pins = if outputs { &n.outputs } else { &n.inputs };
            pins.iter().find(|p| p.id == pin).cloned()
        };
        let interface_pin = |id: &str, name: &str, data_type: &DataType| SubGraphPin {
            id: id.to_string(),
            name: name.to_string(),
            data_type: AuthoredType::from_type_str(&type_string(data_type)),
            description: None,
            default_value: None,
            is_instance_editable: false,
            category: None,
        };

        for (index, c) in crossing.iter().enumerate() {
            let pin_id = format!("port_{index}");
            if selected.contains(&c.target_node) {
                let (Some(target), Some(source)) = (
                    pin_of(&c.target_node, &c.target_pin, false),
                    pin_of(&c.source_node, &c.source_pin, true),
                ) else {
                    return;
                };
                let name = format!("{}.{}", source.pin.name, target.pin.name);
                interface
                    .inputs
                    .push(interface_pin(&pin_id, &name, &target.pin.data_type));
                instance_inputs.push(pin_instance(&pin_id, &name, target.pin.data_type.clone(), PinType::Input));
                entry_outputs.push(pin_instance(
                    &pin_id,
                    &target.pin.name,
                    target.pin.data_type.clone(),
                    PinType::Output,
                ));
                internal_connections.push(new_connection(
                    (&entry_id, &pin_id),
                    (&c.target_node, &c.target_pin),
                    c,
                ));
                parent_connections.push(new_connection(
                    (&c.source_node, &c.source_pin),
                    (&instance_id, &pin_id),
                    c,
                ));
            } else {
                let (Some(source), Some(target)) = (
                    pin_of(&c.source_node, &c.source_pin, true),
                    pin_of(&c.target_node, &c.target_pin, false),
                ) else {
                    return;
                };
                let name = format!("{}.{}", source.pin.name, target.pin.name);
                interface
                    .outputs
                    .push(interface_pin(&pin_id, &name, &source.pin.data_type));
                instance_outputs.push(pin_instance(&pin_id, &name, source.pin.data_type.clone(), PinType::Output));
                exit_inputs.push(pin_instance(
                    &pin_id,
                    &source.pin.name,
                    source.pin.data_type.clone(),
                    PinType::Input,
                ));
                internal_connections.push(new_connection(
                    (&c.source_node, &c.source_pin),
                    (&exit_id, &pin_id),
                    c,
                ));
                parent_connections.push(new_connection(
                    (&instance_id, &pin_id),
                    (&c.target_node, &c.target_pin),
                    c,
                ));
            }
        }

        // The nested graph.
        let mut inner = GraphDescription::new(&graph_name);
        for node in moved {
            inner.add_node(node);
        }
        let mut entry = NodeInstance::new(
            &entry_id,
            "macro_entry",
            Position::new(-220.0, f64::from((max_y - min_y) / 2.0)),
        );
        entry.outputs = entry_outputs;
        inner.add_node(entry);
        let mut exit = NodeInstance::new(
            &exit_id,
            "macro_exit",
            Position::new(f64::from(max_x - min_x + 40.0), f64::from((max_y - min_y) / 2.0)),
        );
        exit.inputs = exit_inputs;
        inner.add_node(exit);
        inner.connections = internal_connections;
        let authored_inner =
            crate::io::canvas_doc::to_authored(&inner, &crate::io::canvas_doc::AuthoredSidecar::default());

        let definition = graphy_blueprint::SubGraph {
            id: graph_id.clone(),
            kind: graphy_blueprint::SubGraphKind::Collapsed,
            name: graph_name.clone(),
            description: "Collapsed graph region".to_string(),
            graph: authored_inner,
            interface,
            metadata: graphy_blueprint::SubGraphMetadata {
                created_at: chrono::Utc::now().to_rfc3339(),
                modified_at: chrono::Utc::now().to_rfc3339(),
                author: None,
                tags: vec!["collapsed".to_string()],
            },
            macro_config: graphy_blueprint::MacroConfiguration {
                hide_in_palette: true,
                category: "Collapsed".to_string(),
                ..Default::default()
            },
        };

        let Some(panel) = self.panel.upgrade() else {
            return;
        };
        panel.update(cx, |editor, panel_cx| {
            editor.subgraphs.push(definition);
            editor.is_dirty = true;
            editor.sync_profile_data();
            editor.invalidate_palette(panel_cx);
            panel_cx.notify();
        });

        // Replace the selection with the call node, in one edit.
        let mut replacement = NodeInstance::new(
            &instance_id,
            SubGraphReference::new(graph_id, graphy_blueprint::SubGraphKind::Collapsed).encode(),
            Position::new(f64::from(min_x), f64::from(min_y)),
        );
        replacement.inputs = instance_inputs;
        replacement.outputs = instance_outputs;
        let mut ids: Vec<&String> = selected.iter().collect();
        ids.sort();
        let mut commands: Vec<GraphCommand> = ids
            .into_iter()
            .map(|id| GraphCommand::RemoveNode { id: id.clone() })
            .collect();
        commands.push(GraphCommand::AddNode(replacement));
        commands.extend(parent_connections.into_iter().map(GraphCommand::Connect));
        let _ = self.apply("Collapse to node", commands, cx);
        self.canvas
            .update(cx, |canvas, cx| canvas.select_nodes([instance_id], cx));
    }
}
