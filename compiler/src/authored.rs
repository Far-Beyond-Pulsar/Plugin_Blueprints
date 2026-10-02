//! Lower the versioned authored schema into Graphy compiler IR.
//! This is the only saved-graph boundary used by editor and headless callers.
use blueprint_graph as authored;

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

pub fn property_value_from_raw(raw: &str) -> graphy::JsonValue {
    let s = normalize_property_literal(raw);
    let lower = s.to_ascii_lowercase();

    if lower == "true" {
        return graphy::JsonValue::Bool(true);
    }
    if lower == "false" {
        return graphy::JsonValue::Bool(false);
    }

    if let Ok(n) = s.parse::<f64>() {
        if n.is_finite() {
            if let Some(number) = serde_json::Number::from_f64(n) {
                return graphy::JsonValue::Number(number);
            }
        }
    }

    graphy::JsonValue::String(s)
}

fn to_graphy_datatype(dt: &blueprint_graph::DataType) -> graphy::DataType {
    use graphy::DataType as GD;
    use blueprint_graph::DataType as PG;
    match dt {
        PG::Execution => GD::Exec,
        PG::Data(ti) => GD::typed(ti.to_string()),
    }
}

pub fn lower_graph(ui_graph: &authored::GraphDescription) -> Result<graphy::GraphDescription, String> {
    use graphy::Connection as GConnection;
    use graphy::{ConnectionType, GraphDescription, NodeInstance, Pin, PinInstance, PinType, Position};
    use std::collections::{HashMap, HashSet};

    let mut graph = GraphDescription::new(&ui_graph.metadata.name);
    let mut valid_input_pins: HashMap<String, HashSet<String>> = HashMap::new();
    let mut valid_output_pins: HashMap<String, HashSet<String>> = HashMap::new();

    for (node_id, node_instance) in &ui_graph.nodes {
        if node_id != &node_instance.id {
            return Err(format!("node key {node_id} does not match its id {}", node_instance.id));
        }
        for pins in [&node_instance.inputs, &node_instance.outputs] {
            let mut ids = HashSet::new();
            if pins.iter().any(|pin| pin.id.is_empty() || !ids.insert(&pin.id)) {
                return Err(format!("node {node_id} has empty or duplicate pin ids"));
            }
        }
        let node_type = match node_instance.node_type.as_str() {
            "macro_entry" => "subgraph_entry".to_string(),
            "macro_exit" => "subgraph_exit".to_string(),
            other if other.starts_with("custom_event:") => format!("on_{}", other.trim_start_matches("custom_event:").replace('-', "_")),
            other if other.starts_with("custom_event_dispatch:") => "emit_custom_event".to_owned(),
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
            return Err(format!("connection {} has a missing node or pin", conn.id));
        };
        let Some(target_pins) = valid_input_pins.get(&conn.target_node) else {
            return Err(format!("connection {} has a missing node or pin", conn.id));
        };
        if !source_pins.contains(&conn.source_pin) || !target_pins.contains(&conn.target_pin) {
            return Err(format!("connection {} has a missing node or pin", conn.id));
        }
        let conn_type = match conn.connection_type {
            blueprint_graph::ConnectionType::Execution => ConnectionType::Execution,
            blueprint_graph::ConnectionType::Data => ConnectionType::Data,
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


/// Lower and expand macros using the same path in editor and headless compilation.
pub fn expand_graph(graph: &authored::GraphDescription, macros: impl IntoIterator<Item = (String, authored::GraphDescription)>) -> Result<graphy::GraphDescription, String> {
    let mut graph = lower_graph(graph)?;
    let mut library = std::collections::HashMap::new();
    for (id, body) in macros {
        if library.insert(id.clone(), lower_graph(&body)?).is_some() {
            return Err(format!("duplicate macro id {id}"));
        }
    }
    graphy::SubGraphExpander::new().expand_all_flat(&mut graph, &library).map_err(|e| format!("Sub-graph expansion failed: {e}"))?;
    collapse_interfaces(&mut graph)?;
    Ok(graph)
}

/// Graphy retains macro interface nodes as routing points. Execution backends
/// consume a flat graph: splice each interface pin through to its consumers.
fn collapse_interfaces(graph: &mut graphy::GraphDescription) -> Result<(), String> {
    let mut interfaces: Vec<_> = graph.nodes.values()
        .filter(|n| matches!(n.node_type.as_str(), "subgraph_entry" | "subgraph_exit"))
        .map(|n| n.id.clone()).collect();
    interfaces.sort();
    for id in interfaces {
        let outgoing: Vec<_> = graph.connections.iter().filter(|c| c.source_node == id).cloned().collect();
        let incoming: Vec<_> = graph.connections.iter().filter(|c| c.target_node == id).cloned().collect();
        for output in outgoing {
            let sources: Vec<_> = incoming.iter().filter(|c| c.target_pin == output.source_pin).collect();
            if matches!(output.connection_type, graphy::ConnectionType::Data) && sources.len() > 1 {
                return Err(format!("macro interface {id}:{} has multiple data sources", output.source_pin));
            }
            for input in sources {
                graph.connections.push(graphy::Connection {
                    source_node: input.source_node.clone(), source_pin: input.source_pin.clone(),
                    target_node: output.target_node.clone(), target_pin: output.target_pin.clone(),
                    connection_type: output.connection_type,
                });
            }
        }
        graph.connections.retain(|c| c.source_node != id && c.target_node != id);
        graph.nodes.remove(&id);
    }
    Ok(())
}
