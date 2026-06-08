//! Clipboard operations for copy-paste functionality

use crate::core::types::{BlueprintComment, BlueprintNode, Connection};
use gpui::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Clipboard data structure for serializing copied graph entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardData {
    pub nodes: Vec<SerializableNode>,
    pub comments: Vec<SerializableComment>,
    pub connections: Vec<SerializableConnection>,
}

/// Serializable version of BlueprintNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableNode {
    pub id: String,
    pub definition_id: String,
    pub title: String,
    pub icon: String,
    pub node_type: crate::core::types::NodeType,
    pub position: (f32, f32),
    pub size: (f32, f32),
    pub inputs: Vec<SerializablePin>,
    pub outputs: Vec<SerializablePin>,
    pub properties: HashMap<String, String>,
    pub description: String,
    pub color: Option<String>,
}

/// Serializable version of Pin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializablePin {
    pub id: String,
    pub name: String,
    pub pin_type: crate::core::types::PinType,
    pub data_type: String, // Serialized as string
}

/// Serializable version of BlueprintComment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableComment {
    pub id: String,
    pub text: String,
    pub position: (f32, f32),
    pub size: (f32, f32),
    pub color: (f32, f32, f32, f32), // HSLA as tuple
    pub contained_node_ids: Vec<String>,
}

/// Serializable version of Connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableConnection {
    pub id: String,
    pub source_node: String,
    pub source_pin: String,
    pub target_node: String,
    pub target_pin: String,
    pub connection_type: String, // "Execution" or "Data"
}

impl ClipboardData {
    /// Create clipboard data from selected entities
    pub fn from_selection(
        nodes: &[BlueprintNode],
        comments: &[BlueprintComment],
        connections: &[Connection],
        selected_node_ids: &[String],
        selected_comment_ids: &[String],
    ) -> Self {
        let selected_node_ids: HashSet<&str> =
            selected_node_ids.iter().map(|id| id.as_str()).collect();
        let selected_comment_ids: HashSet<&str> =
            selected_comment_ids.iter().map(|id| id.as_str()).collect();

        // Filter selected nodes
        let selected_nodes: Vec<_> = nodes
            .iter()
            .filter(|n| selected_node_ids.contains(n.id.as_str()))
            .collect();

        // Filter selected comments
        let selected_comments: Vec<_> = comments
            .iter()
            .filter(|c| selected_comment_ids.contains(c.id.as_str()))
            .collect();

        // Filter connections that are between selected nodes
        let selected_connections: Vec<_> = connections
            .iter()
            .filter(|conn| {
                selected_node_ids.contains(conn.source_node.as_str())
                    && selected_node_ids.contains(conn.target_node.as_str())
            })
            .collect();

        // Convert to serializable format
        let serializable_nodes = selected_nodes
            .iter()
            .map(|node| SerializableNode {
                id: node.id.clone(),
                definition_id: node.definition_id.clone(),
                title: node.title.clone(),
                icon: node.icon.clone(),
                node_type: node.node_type.clone(),
                position: (node.position.x, node.position.y),
                size: (node.size.width, node.size.height),
                inputs: node
                    .inputs
                    .iter()
                    .map(|pin| SerializablePin {
                        id: pin.id.clone(),
                        name: pin.name.clone(),
                        pin_type: pin.pin_type.clone(),
                        data_type: pin.data_type.to_string(),
                    })
                    .collect(),
                outputs: node
                    .outputs
                    .iter()
                    .map(|pin| SerializablePin {
                        id: pin.id.clone(),
                        name: pin.name.clone(),
                        pin_type: pin.pin_type.clone(),
                        data_type: pin.data_type.to_string(),
                    })
                    .collect(),
                properties: node.properties.clone(),
                description: node.description.clone(),
                color: node.color.clone(),
            })
            .collect();

        let serializable_comments = selected_comments
            .iter()
            .map(|comment| SerializableComment {
                id: comment.id.clone(),
                text: comment.text.clone(),
                position: (comment.position.x, comment.position.y),
                size: (comment.size.width, comment.size.height),
                color: (
                    comment.color.h,
                    comment.color.s,
                    comment.color.l,
                    comment.color.a,
                ),
                contained_node_ids: comment.contained_node_ids.clone(),
            })
            .collect();

        let serializable_connections = selected_connections
            .iter()
            .map(|conn| SerializableConnection {
                id: conn.id.clone(),
                source_node: conn.source_node.clone(),
                source_pin: conn.source_pin.clone(),
                target_node: conn.target_node.clone(),
                target_pin: conn.target_pin.clone(),
                connection_type: format!("{:?}", conn.connection_type),
            })
            .collect();

        Self {
            nodes: serializable_nodes,
            comments: serializable_comments,
            connections: serializable_connections,
        }
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Convert back to graph entities with new IDs and offset positions
    pub fn to_graph_entities<E: 'static>(
        &self,
        offset: Point<f32>,
        window: &mut Window,
        cx: &mut Context<E>,
    ) -> (Vec<BlueprintNode>, Vec<BlueprintComment>, Vec<Connection>) {
        use crate::core::types::PinDataType as DataType;

        // Generate ID mapping (old ID -> new ID)
        let mut id_map: HashMap<String, String> = HashMap::new();
        for node in &self.nodes {
            id_map.insert(node.id.clone(), uuid::Uuid::new_v4().to_string());
        }
        for comment in &self.comments {
            id_map.insert(comment.id.clone(), uuid::Uuid::new_v4().to_string());
        }

        // Convert nodes
        let nodes: Vec<BlueprintNode> = self
            .nodes
            .iter()
            .filter_map(|snode| {
                let new_id = id_map.get(&snode.id)?;
                Some(BlueprintNode {
                    id: new_id.clone(),
                    definition_id: snode.definition_id.clone(),
                    title: snode.title.clone(),
                    icon: snode.icon.clone(),
                    node_type: snode.node_type.clone(),
                    position: Point::new(snode.position.0 + offset.x, snode.position.1 + offset.y),
                    size: Size::new(snode.size.0, snode.size.1),
                    inputs: snode
                        .inputs
                        .iter()
                        .map(|pin| crate::core::types::Pin {
                            id: pin.id.clone(),
                            name: pin.name.clone(),
                            pin_type: pin.pin_type.clone(),
                            data_type: DataType::from_type_str(&pin.data_type),
                        })
                        .collect(),
                    outputs: snode
                        .outputs
                        .iter()
                        .map(|pin| crate::core::types::Pin {
                            id: pin.id.clone(),
                            name: pin.name.clone(),
                            pin_type: pin.pin_type.clone(),
                            data_type: DataType::from_type_str(&pin.data_type),
                        })
                        .collect(),
                    properties: snode.properties.clone(),
                    is_selected: false,
                    description: snode.description.clone(),
                    color: snode.color.clone(),
                })
            })
            .collect();

        // Convert comments
        let comments: Vec<BlueprintComment> = self
            .comments
            .iter()
            .filter_map(|scomment| {
                let new_id = id_map.get(&scomment.id)?;

                // Update contained node IDs to use new IDs
                let new_contained_ids: Vec<String> = scomment
                    .contained_node_ids
                    .iter()
                    .filter_map(|old_id| id_map.get(old_id).cloned())
                    .collect();

                Some(BlueprintComment {
                    id: new_id.clone(),
                    text: scomment.text.clone(),
                    position: Point::new(
                        scomment.position.0 + offset.x,
                        scomment.position.1 + offset.y,
                    ),
                    size: Size::new(scomment.size.0, scomment.size.1),
                    color: Hsla {
                        h: scomment.color.0,
                        s: scomment.color.1,
                        l: scomment.color.2,
                        a: scomment.color.3,
                    },
                    contained_node_ids: new_contained_ids,
                    is_selected: false,
                    color_picker_state: Some(
                        cx.new(|cx| ui::color_picker::ColorPickerState::new(window, cx)),
                    ),
                })
            })
            .collect();

        // Convert connections (update IDs to new ones)
        let connections: Vec<Connection> = self
            .connections
            .iter()
            .filter_map(|sconn| {
                let new_source = id_map.get(&sconn.source_node)?;
                let new_target = id_map.get(&sconn.target_node)?;

                // Parse connection type
                let connection_type = if sconn.connection_type.contains("Execution") {
                    ui::graph::ConnectionType::Execution
                } else {
                    ui::graph::ConnectionType::Data
                };

                Some(Connection {
                    id: uuid::Uuid::new_v4().to_string(),
                    source_node: new_source.clone(),
                    source_pin: sconn.source_pin.clone(),
                    target_node: new_target.clone(),
                    target_pin: sconn.target_pin.clone(),
                    connection_type,
                })
            })
            .collect();

        (nodes, comments, connections)
    }
}
