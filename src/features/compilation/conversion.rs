//! Graph conversion - Convert between BlueprintGraph and GraphDescription formats

use crate::editor::panel::BlueprintEditorPanel;
use crate::rendering::layout;
use crate::{
    BlueprintComment, BlueprintGraph, BlueprintNode, Connection, NodeDefinitions, NodeType, Pin,
    PinType,
};
use gpui::*;
use ui::graph::{self as graph_types, GraphDescription, NodeInstance, PinInstance, Position};

impl BlueprintEditorPanel {
    /// Convert current blueprint graph to graph description
    pub(crate) fn convert_to_graph_description(&self, graph: &crate::core::graph::BlueprintGraph) -> Result<GraphDescription, String> {
        self.convert_graph_to_description(graph)
    }

    /// Convert any blueprint graph to graph description
    pub(crate) fn convert_graph_to_description(
        &self,
        graph: &BlueprintGraph,
    ) -> Result<GraphDescription, String> {
        let mut graph_desc = GraphDescription::new("Blueprint Graph");

        // Convert nodes
        for bp_node in &graph.nodes {
            let mut node_instance = NodeInstance::new(
                &bp_node.id,
                &self.get_node_type_from_blueprint(bp_node)?,
                Position {
                    x: bp_node.position.x,
                    y: bp_node.position.y,
                },
            );

            // Convert pins — preserve both UUID id and human-readable name.
            // add_input_pin/add_output_pin use their single argument as both id
            // and name, so we push PinInstances directly to keep them separate.
            for pin in &bp_node.inputs {
                node_instance.inputs.push(PinInstance {
                    id: pin.id.clone(),
                    pin: graph_types::Pin {
                        name: pin.name.clone(),
                        pin_type: graph_types::PinType::Input,
                        data_type: graph_types::DataType::from_type_str(&pin.data_type.type_name),
                        connected_to: Vec::new(),
                    },
                });
            }
            for pin in &bp_node.outputs {
                node_instance.outputs.push(PinInstance {
                    id: pin.id.clone(),
                    pin: graph_types::Pin {
                        name: pin.name.clone(),
                        pin_type: graph_types::PinType::Output,
                        data_type: graph_types::DataType::from_type_str(&pin.data_type.type_name),
                        connected_to: Vec::new(),
                    },
                });
            }

            // Convert properties
            for (key, value) in &bp_node.properties {
                let prop_value = if let Ok(n) = value.parse::<f64>() {
                    serde_json::json!(n)
                } else if let Ok(b) = value.parse::<bool>() {
                    serde_json::json!(b)
                } else {
                    serde_json::json!(value)
                };
                node_instance.set_property(key, prop_value);
            }

            graph_desc.add_node(node_instance);
        }

        // Convert connections
        for connection in &graph.connections {
            let conn_type = graph
                .nodes
                .iter()
                .find(|n| n.id == connection.source_node)
                .and_then(|node| node.outputs.iter().find(|p| p.id == connection.source_pin))
                .map(|pin| {
                    if pin.data_type.is_execution() {
                        graph_types::ConnectionType::Execution
                    } else {
                        graph_types::ConnectionType::Data
                    }
                })
                .unwrap_or(graph_types::ConnectionType::Data);

            let graph_connection = graph_types::Connection::new(
                &connection.id,
                &connection.source_node,
                &connection.source_pin,
                &connection.target_node,
                &connection.target_pin,
                conn_type,
            );
            graph_desc.add_connection(graph_connection);
        }

        // Convert comments
        graph_desc.comments = graph
            .comments
            .iter()
            .map(|c| graph_types::BlueprintComment {
                id: c.id.clone(),
                text: c.text.clone(),
                position: (c.position.x, c.position.y),
                size: (c.size.width, c.size.height),
                color: [c.color.h, c.color.s, c.color.l, c.color.a],
                contained_node_ids: c.contained_node_ids.clone(),
            })
            .collect();

        Ok(graph_desc)
    }

    /// Get node type from blueprint node
    fn get_node_type_from_blueprint(&self, bp_node: &BlueprintNode) -> Result<String, String> {
        Ok(bp_node.definition_id.clone())
    }

    /// Convert graph description to blueprint graph
    pub(crate) fn convert_graph_description_to_blueprint(
        &mut self,
        graph_desc: &GraphDescription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<BlueprintGraph, String> {
        let mut nodes = Vec::new();
        let mut connections = Vec::new();

        let node_definitions = NodeDefinitions::load();

        // Convert nodes
        for (node_id, node_instance) in &graph_desc.nodes {
            let definition_id = node_instance.node_type.clone();
            let node_def = node_definitions.get_node_definition(&definition_id);

            let (title, icon, description, node_type, color) = if definition_id == "reroute" {
                (
                    "Reroute".to_string(),
                    "•".to_string(),
                    "Reroute node for organizing connections".to_string(),
                    NodeType::Reroute,
                    None,
                )
            } else if definition_id.starts_with("custom_event:") {
                let uid = definition_id.trim_start_matches("custom_event:");
                let name = uid.replace('-', " ");
                let name = name
                    .split_whitespace()
                    .map(|w| {
                        let mut c = w.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().to_string() + c.as_str(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                (
                    format!("On {}", name),
                    "📡".to_string(),
                    format!("Custom event listener for '{}'", uid),
                    NodeType::CustomEvent,
                    None,
                )
            } else if definition_id.starts_with("custom_event_dispatch:") {
                let uid = definition_id.trim_start_matches("custom_event_dispatch:");
                let name = uid.replace('-', " ");
                let name = name
                    .split_whitespace()
                    .map(|w| {
                        let mut c = w.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().to_string() + c.as_str(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                (
                    format!("Dispatch {}", name),
                    "📡".to_string(),
                    format!("Dispatch the '{}' custom event", uid),
                    NodeType::CustomEventDispatch,
                    None,
                )
            } else if let Some(def) = node_def {
                // Event entry-points are identified by their underlying Blueprint
                // node type, not by category — events such as `on_input_key`/
                // `on_input_action` live in the "Input" category, not "Events".
                let node_type = if def.is_event {
                    NodeType::Event
                } else {
                    let category = node_definitions.get_category_for_node(&def.id);
                    match category.map(|c| c.name.as_str()) {
                        Some("Logic") => NodeType::Logic,
                        Some("Math") => NodeType::Math,
                        Some("Object") => NodeType::Object,
                        _ => NodeType::Logic,
                    }
                };
                (
                    def.name.clone(),
                    def.icon.clone(),
                    def.description.clone(),
                    node_type,
                    def.color.clone(),
                )
            } else {
                (
                    definition_id.replace('_', " "),
                    "⚙️".to_string(),
                    String::new(),
                    NodeType::Logic,
                    None,
                )
            };

            let bp_node = BlueprintNode {
                id: node_id.clone(),
                definition_id,
                title,
                icon,
                node_type,
                position: Point::new(node_instance.position.x, node_instance.position.y),
                size: {
                    let max_pins = node_instance.inputs.len().max(node_instance.outputs.len());
                    let height = layout::node_height_for_pin_rows(max_pins);
                    crate::Size::new(240.0, height)
                },
                inputs: node_instance
                    .inputs
                    .iter()
                    .map(|pin_inst| {
                        let pin = &pin_inst.pin;
                        let canonical = graph_types::DataType::from_type_str(&pin.data_type.to_string());
                        Pin {
                            id: pin_inst.id.clone(),
                            name: pin.name.clone(),
                            pin_type: match pin.pin_type {
                                graph_types::PinType::Input => PinType::Input,
                                graph_types::PinType::Output => PinType::Output,
                            },
                            data_type: crate::core::types::PinDataType::from_type_str(canonical.to_string()),
                        }
                    })
                    .collect(),
                outputs: node_instance
                    .outputs
                    .iter()
                    .map(|pin_inst| {
                        let pin = &pin_inst.pin;
                        let canonical = graph_types::DataType::from_type_str(&pin.data_type.to_string());
                        Pin {
                            id: pin_inst.id.clone(),
                            name: pin.name.clone(),
                            pin_type: match pin.pin_type {
                                graph_types::PinType::Input => PinType::Input,
                                graph_types::PinType::Output => PinType::Output,
                            },
                            data_type: crate::core::types::PinDataType::from_type_str(canonical.to_string()),
                        }
                    })
                    .collect(),
                properties: node_instance
                    .properties
                    .iter()
                    .map(|(k, v)| {
                        let value_str = if let Some(s) = v.as_str() {
                            s.to_string()
                        } else if let Some(n) = v.as_f64() {
                            n.to_string()
                        } else if let Some(b) = v.as_bool() {
                            b.to_string()
                        } else {
                            v.to_string()
                        };
                        (k.clone(), value_str)
                    })
                    .collect(),
                is_selected: false,
                description,
                color,
            };
            nodes.push(bp_node);
        }

        // Convert connections
        for connection in &graph_desc.connections {
            let bp_connection = Connection {
                id: connection.id.clone(),
                source_node: connection.source_node.clone(),
                source_pin: connection.source_pin.clone(),
                target_node: connection.target_node.clone(),
                target_pin: connection.target_pin.clone(),
                connection_type: connection.connection_type.clone(),
            };
            connections.push(bp_connection);
        }

        // Convert comments with initialized color picker states
        let comments: Vec<BlueprintComment> = graph_desc
            .comments
            .iter()
            .map(|c| {
                let color = Hsla {
                    h: c.color[0],
                    s: c.color[1],
                    l: c.color[2],
                    a: c.color[3],
                };
                let color_picker_state =
                    Some(cx.new(|cx| ui::color_picker::ColorPickerState::new(window, cx)));

                BlueprintComment {
                    id: c.id.clone(),
                    text: c.text.clone(),
                    position: Point::new(c.position.0, c.position.1),
                    size: crate::Size::new(c.size.0, c.size.1),
                    color,
                    contained_node_ids: c.contained_node_ids.clone(),
                    is_selected: false,
                    color_picker_state,
                }
            })
            .collect();

        Ok(BlueprintGraph {
            nodes,
            connections,
            comments,
            selected_nodes: Vec::new(),
            selected_comments: Vec::new(),
            zoom_level: 1.0,
            pan_offset: Point::new(0.0, 0.0),
            virtualization_stats: crate::VirtualizationStats::default(),
            custom_event_defs: std::collections::HashMap::new(),
        })
    }
}
