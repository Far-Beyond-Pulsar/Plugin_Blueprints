//! Collapse and expand private collapsed graph regions.

use super::*;

impl crate::editor::workspace_panels::GraphCanvasPanel {
    /// Expand a private collapsed graph back into its containing graph.
    pub fn expand_collapsed_node(
        &mut self,
        node_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::core::types::Connection;

        let Some(call_node) = self
            .graph
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .cloned()
        else {
            return;
        };
        let Some(graph_id) = crate::core::subgraph_ref::SubGraphReference::id_from_definition_id(
            &call_node.definition_id,
        )
        .map(str::to_owned) else {
            return;
        };
        let Some(panel) = self.panel.upgrade() else {
            return;
        };

        let mut expanded = None;
        panel.update(cx, |editor, panel_cx| {
            let Some(definition) = editor
                .subgraphs
                .iter()
                .find(|definition| {
                    definition.id == graph_id
                        && definition.kind == blueprint_graph::SubGraphKind::Collapsed
                })
                .cloned()
            else {
                return;
            };
            let tab_graph = editor
                .open_tabs
                .iter()
                .find(|tab| tab.id == graph_id)
                .map(|tab| tab.graph.clone())
                .filter(|graph| {
                    let has_entry = graph
                        .nodes
                        .iter()
                        .any(|node| node.node_type == NodeType::MacroEntry);
                    let has_exit = graph
                        .nodes
                        .iter()
                        .any(|node| node.node_type == NodeType::MacroExit);
                    let has_body = graph.nodes.iter().any(|node| {
                        node.node_type != NodeType::MacroEntry
                            && node.node_type != NodeType::MacroExit
                    });
                    let has_collapsed_sentinels = graph.nodes.iter().any(|node| {
                        node.node_type == NodeType::MacroEntry
                            && node.id.starts_with("collapsed_entry_")
                    }) && graph.nodes.iter().any(|node| {
                        node.node_type == NodeType::MacroExit
                            && node.id.starts_with("collapsed_exit_")
                    });
                    has_entry && has_exit && (has_body || has_collapsed_sentinels)
                });
            let Some(mut inner) = tab_graph.or_else(|| {
                editor
                    .convert_graph_description_to_blueprint(&definition.graph, window, panel_cx)
                    .ok()
            }) else {
                return;
            };
            normalize_subgraph_boundary(&mut inner, NodeType::MacroEntry);
            normalize_subgraph_boundary(&mut inner, NodeType::MacroExit);
            let Some(entry_id) = inner
                .nodes
                .iter()
                .find(|node| node.node_type == NodeType::MacroEntry)
                .map(|node| node.id.clone())
            else {
                return;
            };
            let Some(exit_id) = inner
                .nodes
                .iter()
                .find(|node| node.node_type == NodeType::MacroExit)
                .map(|node| node.id.clone())
            else {
                return;
            };

            let incoming: std::collections::HashMap<String, Vec<Connection>> = self
                .graph
                .connections
                .iter()
                .filter(|connection| connection.target_node == node_id)
                .fold(std::collections::HashMap::new(), |mut map, connection| {
                    map.entry(connection.target_pin.clone())
                        .or_default()
                        .push(connection.clone());
                    map
                });
            let outgoing: std::collections::HashMap<String, Vec<Connection>> = self
                .graph
                .connections
                .iter()
                .filter(|connection| connection.source_node == node_id)
                .fold(std::collections::HashMap::new(), |mut map, connection| {
                    map.entry(connection.source_pin.clone())
                        .or_default()
                        .push(connection.clone());
                    map
                });

            let mut restored_connections = self
                .graph
                .connections
                .iter()
                .filter(|connection| {
                    connection.source_node != node_id && connection.target_node != node_id
                })
                .cloned()
                .collect::<Vec<_>>();
            for connection in &inner.connections {
                if connection.source_node.as_str() == entry_id.as_str() {
                    if let Some(outer) = incoming.get(&connection.source_pin) {
                        for outer in outer {
                            restored_connections.push(Connection {
                                id: uuid::Uuid::new_v4().to_string(),
                                source_node: outer.source_node.clone(),
                                source_pin: outer.source_pin.clone(),
                                target_node: connection.target_node.clone(),
                                target_pin: connection.target_pin.clone(),
                                connection_type: connection.connection_type.clone(),
                            });
                        }
                    }
                } else if connection.target_node.as_str() == exit_id.as_str() {
                    if let Some(outer) = outgoing.get(&connection.target_pin) {
                        for outer in outer {
                            restored_connections.push(Connection {
                                id: uuid::Uuid::new_v4().to_string(),
                                source_node: connection.source_node.clone(),
                                source_pin: connection.source_pin.clone(),
                                target_node: outer.target_node.clone(),
                                target_pin: outer.target_pin.clone(),
                                connection_type: connection.connection_type.clone(),
                            });
                        }
                    }
                } else {
                    restored_connections.push(connection.clone());
                }
            }

            inner.nodes.retain(|node| {
                node.node_type != NodeType::MacroEntry && node.node_type != NodeType::MacroExit
            });

            // A collapsed instance can be duplicated like any other macro
            // call. Expanding a second instance must not insert the same node
            // IDs as the first expansion, or the canvas will render both sets
            // at once and connections will target the wrong copy.
            let remapped_ids: std::collections::HashMap<String, String> = inner
                .nodes
                .iter()
                .map(|node| (node.id.clone(), uuid::Uuid::new_v4().to_string()))
                .collect();
            for node in &mut inner.nodes {
                if let Some(new_id) = remapped_ids.get(&node.id) {
                    node.id = new_id.clone();
                }
            }
            for connection in &mut restored_connections {
                if let Some(new_id) = remapped_ids.get(&connection.source_node) {
                    connection.source_node = new_id.clone();
                }
                if let Some(new_id) = remapped_ids.get(&connection.target_node) {
                    connection.target_node = new_id.clone();
                }
            }
            for node in &mut inner.nodes {
                node.position.x += call_node.position.x;
                node.position.y += call_node.position.y;
                node.is_selected = true;
            }
            expanded = Some((inner.nodes, restored_connections));

            // Collapsed definitions are private to their call sites. Keep the
            // definition while another instance still references it, but
            // remove its saved graph and tab after the last instance expands.
            let references_graph = |nodes: &[BlueprintNode]| {
                nodes.iter().any(|node| {
                    node.id != node_id
                        && crate::core::subgraph_ref::SubGraphReference::id_from_definition_id(
                            &node.definition_id,
                        ) == Some(graph_id.as_str())
                })
            };
            let has_other_instance = references_graph(&self.graph.nodes)
                || editor
                    .open_tabs
                    .iter()
                    .filter(|tab| tab.id != self.id)
                    .any(|tab| references_graph(&tab.graph.nodes))
                || editor
                    .subgraphs
                    .iter()
                    .filter(|subgraph| subgraph.id != graph_id)
                    .any(|subgraph| {
                        subgraph.graph.nodes.values().any(|node| {
                            crate::core::subgraph_ref::SubGraphReference::id_from_definition_id(
                                &node.node_type,
                            ) == Some(graph_id.as_str())
                        })
                    });

            if !has_other_instance {
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
                editor.invalidate_palette(panel_cx);
                editor.refresh_graph_workspace_tabs(window, panel_cx);
            }
            panel_cx.notify();
        });

        let Some((nodes, connections)) = expanded else {
            return;
        };
        self.graph.nodes.retain(|node| node.id != node_id);
        let restored_ids = nodes.iter().map(|node| node.id.clone()).collect::<Vec<_>>();
        self.graph.nodes.extend(nodes);
        self.graph.connections = connections.into();
        self.graph.selected_nodes = restored_ids;
        self.is_dirty = true;
        cx.notify();
    }

    /// Replace the selected nodes with a single call node backed by a private
    /// collapsed graph. Every edge crossing the selection boundary becomes an
    /// explicit graph interface pin, so the parent and nested graphs retain
    /// the same connections.
    pub fn collapse_selected_nodes(&mut self, cx: &mut Context<Self>) {
        use crate::core::types::{BlueprintNode, Connection, NodeType, Pin, PinType};
        use blueprint_graph::{DataType as GraphDataType, SubGraphPin};

        let selected: std::collections::HashSet<String> = self
            .graph
            .selected_nodes
            .iter()
            .filter(|id| self.graph.nodes.iter().any(|n| n.id == **id))
            .cloned()
            .collect();
        if selected.is_empty() {
            return;
        }

        let mut moved_nodes: Vec<BlueprintNode> = self
            .graph
            .nodes
            .iter()
            .filter(|node| selected.contains(&node.id))
            .cloned()
            .collect();
        // Macro sentinels belong to their containing graph and cannot be
        // nested as ordinary nodes in a collapsed region.
        if moved_nodes
            .iter()
            .any(|node| matches!(node.node_type, NodeType::MacroEntry | NodeType::MacroExit))
        {
            return;
        }
        let Some(min_x) = moved_nodes.iter().map(|n| n.position.x).reduce(f32::min) else {
            return;
        };
        let min_y = moved_nodes
            .iter()
            .map(|n| n.position.y)
            .fold(f32::INFINITY, f32::min);
        let max_x = moved_nodes
            .iter()
            .map(|n| n.position.x + n.size.width)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = moved_nodes
            .iter()
            .map(|n| n.position.y + n.size.height)
            .fold(f32::NEG_INFINITY, f32::max);
        let graph_id = uuid::Uuid::new_v4().to_string();
        let graph_name = format!("Collapsed {}", self.graph.selected_nodes.len());

        for node in &mut moved_nodes {
            node.position.x -= min_x;
            node.position.y -= min_y;
            node.is_selected = false;
        }

        let crossing: Vec<Connection> = self
            .graph
            .connections
            .iter()
            .filter(|connection| {
                selected.contains(&connection.source_node)
                    != selected.contains(&connection.target_node)
            })
            .cloned()
            .collect();
        let mut interface = blueprint_graph::SubGraphInterface {
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        let mut internal_connections = self
            .graph
            .connections
            .iter()
            .filter(|connection| {
                selected.contains(&connection.source_node)
                    && selected.contains(&connection.target_node)
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut parent_connections = self
            .graph
            .connections
            .iter()
            .filter(|connection| {
                !selected.contains(&connection.source_node)
                    && !selected.contains(&connection.target_node)
            })
            .cloned()
            .collect::<Vec<_>>();

        let entry_id = format!("collapsed_entry_{graph_id}");
        let exit_id = format!("collapsed_exit_{graph_id}");
        let mut entry_outputs = Vec::new();
        let mut exit_inputs = Vec::new();
        let mut instance_inputs = Vec::new();
        let mut instance_outputs = Vec::new();

        for (index, connection) in crossing.iter().enumerate() {
            let pin_id = format!("port_{index}");
            if selected.contains(&connection.target_node) {
                let target = moved_nodes
                    .iter()
                    .find(|node| node.id == connection.target_node)
                    .and_then(|node| {
                        node.inputs
                            .iter()
                            .find(|pin| pin.id == connection.target_pin)
                    });
                let source = self
                    .graph
                    .nodes
                    .iter()
                    .find(|node| node.id == connection.source_node)
                    .and_then(|node| {
                        node.outputs
                            .iter()
                            .find(|pin| pin.id == connection.source_pin)
                    });
                let Some(target) = target else { return };
                let Some(source) = source else { return };
                let name = format!("{}.{}", source.name, target.name);
                let data_type = target.data_type.type_name.clone();
                interface.inputs.push(SubGraphPin {
                    id: pin_id.clone(),
                    name: name.clone(),
                    data_type: GraphDataType::from_type_str(&data_type),
                    description: None,
                    default_value: None,
                    is_instance_editable: false,
                    category: None,
                });
                instance_inputs.push(Pin {
                    id: pin_id.clone(),
                    name,
                    pin_type: PinType::Input,
                    data_type: target.data_type.clone(),
                });
                entry_outputs.push(Pin {
                    id: pin_id.clone(),
                    name: target.name.clone(),
                    pin_type: PinType::Output,
                    data_type: target.data_type.clone(),
                });
                internal_connections.push(Connection {
                    id: uuid::Uuid::new_v4().to_string(),
                    source_node: entry_id.clone(),
                    source_pin: pin_id,
                    target_node: connection.target_node.clone(),
                    target_pin: connection.target_pin.clone(),
                    connection_type: connection.connection_type.clone(),
                });
                parent_connections.push(Connection {
                    id: uuid::Uuid::new_v4().to_string(),
                    source_node: connection.source_node.clone(),
                    source_pin: connection.source_pin.clone(),
                    target_node: format!("collapsed_{graph_id}"),
                    target_pin: format!("port_{index}"),
                    connection_type: connection.connection_type.clone(),
                });
            } else {
                let source = moved_nodes
                    .iter()
                    .find(|node| node.id == connection.source_node)
                    .and_then(|node| {
                        node.outputs
                            .iter()
                            .find(|pin| pin.id == connection.source_pin)
                    });
                let target = self
                    .graph
                    .nodes
                    .iter()
                    .find(|node| node.id == connection.target_node)
                    .and_then(|node| {
                        node.inputs
                            .iter()
                            .find(|pin| pin.id == connection.target_pin)
                    });
                let Some(source) = source else { return };
                let Some(target) = target else { return };
                let name = format!("{}.{}", source.name, target.name);
                let data_type = source.data_type.type_name.clone();
                interface.outputs.push(SubGraphPin {
                    id: pin_id.clone(),
                    name: name.clone(),
                    data_type: GraphDataType::from_type_str(&data_type),
                    description: None,
                    default_value: None,
                    is_instance_editable: false,
                    category: None,
                });
                instance_outputs.push(Pin {
                    id: pin_id.clone(),
                    name,
                    pin_type: PinType::Output,
                    data_type: source.data_type.clone(),
                });
                exit_inputs.push(Pin {
                    id: pin_id.clone(),
                    name: source.name.clone(),
                    pin_type: PinType::Input,
                    data_type: source.data_type.clone(),
                });
                internal_connections.push(Connection {
                    id: uuid::Uuid::new_v4().to_string(),
                    source_node: connection.source_node.clone(),
                    source_pin: connection.source_pin.clone(),
                    target_node: exit_id.clone(),
                    target_pin: pin_id,
                    connection_type: connection.connection_type.clone(),
                });
                parent_connections.push(Connection {
                    id: uuid::Uuid::new_v4().to_string(),
                    source_node: format!("collapsed_{graph_id}"),
                    source_pin: format!("port_{index}"),
                    target_node: connection.target_node.clone(),
                    target_pin: connection.target_pin.clone(),
                    connection_type: connection.connection_type.clone(),
                });
            }
        }

        let rows = entry_outputs.len().max(exit_inputs.len()).max(1);
        let internal_graph = BlueprintGraph {
            nodes: {
                let mut nodes = moved_nodes;
                nodes.push(BlueprintNode {
                    id: entry_id.clone(),
                    definition_id: "macro_entry".to_string(),
                    title: graph_name.clone(),
                    icon: "▶".to_string(),
                    node_type: NodeType::MacroEntry,
                    position: Point::new(-220.0, (max_y - min_y) / 2.0),
                    size: Size::new(
                        180.0,
                        crate::rendering::layout::node_height_for_pin_rows(
                            entry_outputs.len().max(1),
                        ),
                    ),
                    inputs: Vec::new(),
                    outputs: entry_outputs,
                    properties: std::collections::HashMap::new(),
                    is_selected: false,
                    description: format!("Entry — {graph_name}"),
                    color: Some("#7C3AED".to_string()),
                });
                nodes.push(BlueprintNode {
                    id: exit_id.clone(),
                    definition_id: "macro_exit".to_string(),
                    title: format!("{graph_name} (Return)"),
                    icon: "◀".to_string(),
                    node_type: NodeType::MacroExit,
                    position: Point::new(max_x - min_x + 40.0, (max_y - min_y) / 2.0),
                    size: Size::new(
                        180.0,
                        crate::rendering::layout::node_height_for_pin_rows(
                            exit_inputs.len().max(1),
                        ),
                    ),
                    inputs: exit_inputs,
                    outputs: Vec::new(),
                    properties: std::collections::HashMap::new(),
                    is_selected: false,
                    description: format!("Exit — {graph_name}"),
                    color: Some("#7C3AED".to_string()),
                });
                nodes.into()
            },
            connections: internal_connections.into(),
            comments: Vec::new().into(),
            selected_nodes: Vec::new(),
            selected_comments: Vec::new(),
            zoom_level: 1.0,
            pan_offset: Point::new(0.0, 0.0),
            virtualization_stats: crate::VirtualizationStats::default(),
        };
        let instance_id = format!("collapsed_{graph_id}");
        let replacement = BlueprintNode {
            id: instance_id.clone(),
            definition_id: crate::core::subgraph_ref::SubGraphReference::new(
                graph_id.clone(),
                blueprint_graph::SubGraphKind::Collapsed,
            )
            .encode(),
            title: graph_name.clone(),
            icon: "▣".to_string(),
            node_type: NodeType::SubGraphCall,
            position: Point::new(min_x, min_y),
            size: Size::new(
                200.0,
                crate::rendering::layout::node_height_for_pin_rows(rows),
            ),
            inputs: instance_inputs,
            outputs: instance_outputs,
            properties: std::collections::HashMap::new(),
            is_selected: true,
            description: format!("Collapsed graph '{graph_name}'"),
            color: Some("#7C3AED".to_string()),
        };

        let Some(panel) = self.panel.upgrade() else {
            return;
        };
        let mut definition = None;
        panel.update(cx, |editor, panel_cx| {
            let Ok(graph) = editor.convert_graph_to_description(&internal_graph) else {
                return;
            };
            definition = Some(blueprint_graph::SubGraph {
                id: graph_id.clone(),
                kind: blueprint_graph::SubGraphKind::Collapsed,
                name: graph_name.clone(),
                description: "Collapsed graph region".to_string(),
                graph,
                interface,
                metadata: blueprint_graph::SubGraphMetadata {
                    created_at: chrono::Utc::now().to_rfc3339(),
                    modified_at: chrono::Utc::now().to_rfc3339(),
                    author: None,
                    tags: vec!["collapsed".to_string()],
                },
                macro_config: blueprint_graph::MacroConfiguration {
                    hide_in_palette: true,
                    category: "Collapsed".to_string(),
                    ..Default::default()
                },
            });
            editor.subgraphs.push(definition.clone().unwrap());
            editor.is_dirty = true;
            editor.invalidate_palette(panel_cx);
            panel_cx.notify();
        });
        if definition.is_none() {
            return;
        }

        self.graph.nodes.retain(|node| !selected.contains(&node.id));
        self.graph.nodes.push(replacement);
        self.graph.connections = parent_connections.into();
        self.graph.selected_nodes = vec![instance_id];
        self.is_dirty = true;
        cx.notify();
    }
}
