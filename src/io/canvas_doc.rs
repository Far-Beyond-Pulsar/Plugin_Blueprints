//! Boundary between the authored `.blueprint` schema and the editing document.
//!
//! The editor edits one in-memory `graphy::GraphDescription` (owned by the
//! Graphy canvas). The authored schema (`graphy_blueprint::GraphDescription`) is
//! read once on load and rebuilt once on save; nothing else converts between
//! the two. Everything the authored graph carries that the canvas document has
//! no field for lives in [`AuthoredSidecar`] and is written back unchanged.
//!
//! Identifiers survive the round trip: node ids and keys, pin ids, connection
//! ids, comment ids, and the pin back-links (`connected_to`, which are derived
//! from the connection ids and rebuilt here in connection order).

use std::collections::HashMap;

use graphy::core::GraphComment;
use graphy::{Connection, ConnectionType, DataType, GraphDescription, NodeInstance, Pin, PinInstance, PinType, Position};
use graphy_blueprint as authored;

/// Authored fields the canvas document does not model.
#[derive(Clone, Debug, Default)]
pub struct AuthoredSidecar {
    pub custom_event_defs: HashMap<String, authored::CustomEventDefDescription>,
}

fn datatype_in(dt: &authored::DataType) -> DataType {
    match dt {
        authored::DataType::Execution => DataType::Exec,
        authored::DataType::Data(info) => DataType::typed(info.to_string()),
    }
}

fn datatype_out(dt: &DataType) -> authored::DataType {
    match dt {
        DataType::Exec => authored::DataType::Execution,
        DataType::Data(info) => authored::DataType::from_type_str(&info.type_string),
    }
}

/// Reads an authored graph into the editing document.
pub fn from_authored(graph: &authored::GraphDescription) -> (GraphDescription, AuthoredSidecar) {
    let mut doc = GraphDescription::new(&graph.metadata.name);
    doc.metadata.description = graph.metadata.description.clone();
    doc.metadata.version = graph.metadata.version.clone();
    doc.metadata.created_at = graph.metadata.created_at.clone();
    doc.metadata.modified_at = graph.metadata.modified_at.clone();

    for (key, node) in &graph.nodes {
        let mut out = NodeInstance::new(
            &node.id,
            &node.node_type,
            Position::new(f64::from(node.position.x), f64::from(node.position.y)),
        );
        out.properties = node.properties.clone();
        let pin = |p: &authored::PinInstance, kind: PinType| PinInstance {
            id: p.id.clone(),
            pin: Pin {
                id: p.id.clone(),
                name: p.pin.name.clone(),
                data_type: datatype_in(&p.pin.data_type),
                pin_type: kind,
            },
        };
        out.inputs = node.inputs.iter().map(|p| pin(p, PinType::Input)).collect();
        out.outputs = node.outputs.iter().map(|p| pin(p, PinType::Output)).collect();
        doc.nodes.insert(key.clone(), out);
    }

    doc.connections = graph
        .connections
        .iter()
        .map(|c| {
            let mut out = Connection::new(
                &c.source_node,
                &c.source_pin,
                &c.target_node,
                &c.target_pin,
                match c.connection_type {
                    authored::ConnectionType::Execution => ConnectionType::Execution,
                    authored::ConnectionType::Data => ConnectionType::Data,
                },
            );
            out.id = Some(c.id.clone());
            out
        })
        .collect();

    doc.comments = graph
        .comments
        .iter()
        .map(|c| {
            let mut out = GraphComment::new(
                &c.text,
                Position::new(f64::from(c.position.0), f64::from(c.position.1)),
                (f64::from(c.size.0), f64::from(c.size.1)),
            );
            out.id = c.id.clone();
            out.color = c.color;
            out.contained_node_ids = c.contained_node_ids.clone();
            out
        })
        .collect();
    doc.zip_ties = graph
        .zip_ties
        .iter()
        .map(|tie| graphy::GraphZipTie {
            position: Position::new(f64::from(tie.position.0), f64::from(tie.position.1)),
            straight_length: f64::from(tie.straight_length),
            wires: tie
                .wires
                .iter()
                .map(|edge| graphy::ZipTieEdge {
                    source_node: edge.source_node.clone(),
                    source_pin: edge.source_pin.clone(),
                    target_node: edge.target_node.clone(),
                    target_pin: edge.target_pin.clone(),
                })
                .collect(),
        })
        .collect();

    let sidecar = AuthoredSidecar {
        custom_event_defs: graph.custom_event_defs.clone(),
    };
    (doc, sidecar)
}

/// The id a connection is saved under: its authored id, or a deterministic one
/// derived from its endpoints for edges made in this session (so saving twice
/// does not rename them).
pub fn connection_id(c: &Connection) -> String {
    c.id.clone().unwrap_or_else(|| {
        format!(
            "{}.{}->{}.{}",
            c.source_node, c.source_pin, c.target_node, c.target_pin
        )
    })
}

/// Builds the authored graph for saving or compiling.
pub fn to_authored(doc: &GraphDescription, sidecar: &AuthoredSidecar) -> authored::GraphDescription {
    let mut out = authored::GraphDescription::new(&doc.metadata.name);
    out.metadata.description = doc.metadata.description.clone();
    out.metadata.version = doc.metadata.version.clone();
    out.metadata.created_at = doc.metadata.created_at.clone();
    out.metadata.modified_at = doc.metadata.modified_at.clone();
    out.custom_event_defs = sidecar.custom_event_defs.clone();

    for (key, node) in &doc.nodes {
        let pin = |p: &PinInstance, kind: authored::PinType| authored::PinInstance {
            id: p.id.clone(),
            pin: authored::Pin {
                name: p.pin.name.clone(),
                pin_type: kind,
                data_type: datatype_out(&p.pin.data_type),
                connected_to: Vec::new(),
            },
        };
        out.nodes.insert(
            key.clone(),
            authored::NodeInstance {
                id: node.id.clone(),
                node_type: node.node_type.clone(),
                position: authored::Position {
                    x: node.position.x as f32,
                    y: node.position.y as f32,
                },
                properties: node.properties.clone(),
                inputs: node.inputs.iter().map(|p| pin(p, authored::PinType::Input)).collect(),
                outputs: node.outputs.iter().map(|p| pin(p, authored::PinType::Output)).collect(),
            },
        );
    }

    for c in &doc.connections {
        let id = connection_id(c);
        if let Some(n) = out.nodes.get_mut(&c.source_node) {
            if let Some(p) = n.outputs.iter_mut().find(|p| p.id == c.source_pin) {
                p.pin.connected_to.push(id.clone());
            }
        }
        if let Some(n) = out.nodes.get_mut(&c.target_node) {
            if let Some(p) = n.inputs.iter_mut().find(|p| p.id == c.target_pin) {
                p.pin.connected_to.push(id.clone());
            }
        }
        out.connections.push(authored::Connection {
            id,
            source_node: c.source_node.clone(),
            source_pin: c.source_pin.clone(),
            target_node: c.target_node.clone(),
            target_pin: c.target_pin.clone(),
            connection_type: match c.connection_type {
                ConnectionType::Execution => authored::ConnectionType::Execution,
                ConnectionType::Data => authored::ConnectionType::Data,
            },
        });
    }

    out.comments = doc
        .comments
        .iter()
        .enumerate()
        .map(|(i, c)| authored::BlueprintComment {
            id: if c.id.is_empty() {
                format!("comment_{i}")
            } else {
                c.id.clone()
            },
            text: c.text.clone(),
            position: (c.position.x as f32, c.position.y as f32),
            size: (c.size.0 as f32, c.size.1 as f32),
            color: c.color,
            contained_node_ids: c.contained_node_ids.clone(),
        })
        .collect();
    out.zip_ties = doc
        .zip_ties
        .iter()
        .map(|tie| authored::BlueprintZipTie {
            position: (tie.position.x as f32, tie.position.y as f32),
            straight_length: tie.straight_length as f32,
            wires: tie
                .wires
                .iter()
                .map(|edge| authored::BlueprintZipTieEdge {
                    source_node: edge.source_node.clone(),
                    source_pin: edge.source_pin.clone(),
                    target_node: edge.target_node.clone(),
                    target_pin: edge.target_pin.clone(),
                })
                .collect(),
        })
        .collect();
    out
}


/// A node property as the editor's text fields hold it, as the document stores it.
pub fn property_value(raw: &str) -> graphy::JsonValue {
    blueprint_compiler::authored::property_value_from_raw(raw)
}

/// A stored node property as editor text.
pub fn property_string(value: &graphy::JsonValue) -> String {
    match value {
        graphy::JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}
