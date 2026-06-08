//! Type compatibility and inference for connections

use crate::core::graph::BlueprintGraph;
use crate::core::types::{Connection, NodeType};
use crate::core::types::PinDataType as GraphDataType;

/// Check if two data types are compatible for connection
pub fn are_types_compatible(from_type: &GraphDataType, to_type: &GraphDataType) -> bool {
    from_type.is_compatible_with(to_type)
}

/// Infer the type of a reroute node based on its connections
pub fn infer_reroute_type(node_id: &str, graph: &BlueprintGraph) -> Option<GraphDataType> {
    // Check if this is actually a reroute node
    let node = graph.nodes.iter().find(|n| n.id == node_id)?;
    if node.node_type != NodeType::Reroute {
        return None;
    }

    // Look for any connection to/from this reroute node
    for connection in &graph.connections {
        if connection.source_node == node_id {
            // This reroute is a source - get the target pin type
            if let Some(target_node) = graph.nodes.iter().find(|n| n.id == connection.target_node) {
                if let Some(pin) = target_node
                    .inputs
                    .iter()
                    .find(|p| p.id == connection.target_pin)
                {
                    return Some(pin.data_type.clone());
                }
            }
        } else if connection.target_node == node_id {
            // This reroute is a target - get the source pin type
            if let Some(source_node) = graph.nodes.iter().find(|n| n.id == connection.source_node) {
                if let Some(pin) = source_node
                    .outputs
                    .iter()
                    .find(|p| p.id == connection.source_pin)
                {
                    return Some(pin.data_type.clone());
                }
            }
        }
    }

    // No connections found, default to wildcard data
    Some(GraphDataType::from_type_str("?"))
}

/// Validate that a connection is valid
pub fn validate_connection(connection: &Connection, graph: &BlueprintGraph) -> Result<(), String> {
    // Check that both nodes exist
    let source_node = graph
        .nodes
        .iter()
        .find(|n| n.id == connection.source_node)
        .ok_or_else(|| format!("Source node {} not found", connection.source_node))?;

    let target_node = graph
        .nodes
        .iter()
        .find(|n| n.id == connection.target_node)
        .ok_or_else(|| format!("Target node {} not found", connection.target_node))?;

    // Check that both pins exist
    let source_pin = source_node
        .outputs
        .iter()
        .find(|p| p.id == connection.source_pin)
        .ok_or_else(|| format!("Source pin {} not found", connection.source_pin))?;

    let target_pin = target_node
        .inputs
        .iter()
        .find(|p| p.id == connection.target_pin)
        .ok_or_else(|| format!("Target pin {} not found", connection.target_pin))?;

    // Check that types are compatible
    if !are_types_compatible(&source_pin.data_type, &target_pin.data_type) {
        return Err(format!(
            "Incompatible types: {:?} -> {:?}",
            source_pin.data_type, target_pin.data_type
        ));
    }

    // Check that we're not connecting a node to itself
    if connection.source_node == connection.target_node {
        return Err("Cannot connect a node to itself".to_string());
    }

    Ok(())
}

/// Get all connections from a specific pin
pub fn get_connections_from_pin<'a>(
    node_id: &str,
    pin_id: &str,
    graph: &'a BlueprintGraph,
) -> Vec<&'a Connection> {
    graph
        .connections
        .iter()
        .filter(|conn| conn.source_node == node_id && conn.source_pin == pin_id)
        .collect()
}

/// Get all connections to a specific pin
pub fn get_connections_to_pin<'a>(
    node_id: &str,
    pin_id: &str,
    graph: &'a BlueprintGraph,
) -> Vec<&'a Connection> {
    graph
        .connections
        .iter()
        .filter(|conn| conn.target_node == node_id && conn.target_pin == pin_id)
        .collect()
}

/// Check if a pin has any connections
pub fn is_pin_connected(
    node_id: &str,
    pin_id: &str,
    is_input: bool,
    graph: &BlueprintGraph,
) -> bool {
    if is_input {
        graph
            .connections
            .iter()
            .any(|conn| conn.target_node == node_id && conn.target_pin == pin_id)
    } else {
        graph
            .connections
            .iter()
            .any(|conn| conn.source_node == node_id && conn.source_pin == pin_id)
    }
}
