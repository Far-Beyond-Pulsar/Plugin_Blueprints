//! Core type definitions for blueprint nodes, pins, and connections.
//!
//! This module defines the fundamental data structures used throughout the blueprint
//! editor, including nodes, pins, connections, and comments. These types represent
//! the visual and logical structure of a blueprint graph.

use gpui::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ui::color_picker::ColorPickerState;
use ui::graph::DataType;

use crate::rendering::layout;

// ============================================================================
// Compilation Status
// ============================================================================

/// Compilation status tracking for UI feedback
#[derive(Clone, Debug, PartialEq)]
pub enum CompilationState {
    Idle,
    Compiling,
    Success,
    Error,
}

#[derive(Clone, Debug)]
pub struct CompilationStatus {
    pub state: CompilationState,
    pub message: String,
    pub progress: f32, // 0.0 to 1.0
    pub is_compiling: bool,
}

impl Default for CompilationStatus {
    fn default() -> Self {
        Self {
            state: CompilationState::Idle,
            message: "Ready to compile".to_string(),
            progress: 0.0,
            is_compiling: false,
        }
    }
}

// ============================================================================
// Node Types
// ============================================================================

/// A node in the blueprint graph representing a function, operation, or event.
#[derive(Clone, Debug)]
pub struct BlueprintNode {
    pub id: String,
    pub definition_id: String, // ID from NodeDefinition to restore metadata
    pub title: String,
    pub icon: String,
    pub node_type: NodeType,
    pub position: Point<f32>,
    pub size: Size<f32>,
    pub inputs: Vec<Pin>,
    pub outputs: Vec<Pin>,
    pub properties: HashMap<String, String>,
    pub is_selected: bool,
    pub description: String,   // Markdown documentation for the node
    pub color: Option<String>, // Custom color from blueprint attribute
}

/// Categorization of node behavior and appearance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeType {
    Event,
    Logic,
    Math,
    Object,
    Reroute,       // Visual pass-through node for organizing connections
    MacroEntry,    // Entry point for macro graphs (replaces generic subgraph_input)
    MacroExit,     // Exit point for macro graphs (replaces generic subgraph_output)
    MacroInstance, // Instance of a macro in parent graph
}

// ============================================================================
// Pin Types
// ============================================================================

/// A connection point on a node for data flow or execution flow.
#[derive(Clone, Debug)]
pub struct Pin {
    pub id: String,
    pub name: String,
    pub pin_type: PinType,
    pub data_type: DataType,
}

/// Direction of data flow for a pin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PinType {
    Input,
    Output,
}

// ============================================================================
// Connection Types
// ============================================================================

/// A connection between two pins in the blueprint graph.
#[derive(Clone, Debug)]
pub struct Connection {
    pub id: String,
    pub source_node: String,
    pub source_pin: String,
    pub target_node: String,
    pub target_pin: String,
    pub connection_type: ui::graph::ConnectionType,
}

// ============================================================================
// Comment Types
// ============================================================================

/// A visual comment box that can group and annotate nodes in the graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlueprintComment {
    pub id: String,
    pub text: String,
    #[serde(with = "crate::core::serialization::point_serde")]
    pub position: Point<f32>,
    #[serde(with = "crate::core::serialization::size_serde")]
    pub size: Size<f32>,
    #[serde(with = "crate::core::serialization::hsla_serde")]
    pub color: Hsla, // Background color
    pub contained_node_ids: Vec<String>, // Nodes fully contained in this comment
    #[serde(skip)]
    pub is_selected: bool,
    #[serde(skip)]
    pub color_picker_state: Option<gpui::Entity<ColorPickerState>>,
}

impl BlueprintComment {
    pub fn new(
        position: Point<f32>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<crate::editor::panel::BlueprintEditorPanel>,
    ) -> Self {
        let color_picker_state = Some(cx.new(|cx| ColorPickerState::new(window, cx)));
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            text: "Comment".to_string(),
            position,
            size: Size::new(300.0, 200.0),
            color: Hsla {
                h: 0.5,
                s: 0.3,
                l: 0.2,
                a: 0.3,
            }, // Default semi-transparent color
            contained_node_ids: Vec::new(),
            is_selected: false,
            color_picker_state,
        }
    }

    /// Check if a node is fully contained within this comment's bounds
    pub fn contains_node(&self, node: &BlueprintNode) -> bool {
        let node_left = node.position.x;
        let node_top = node.position.y;
        let node_right = node.position.x + node.size.width;
        let node_bottom = node.position.y + node.size.height;

        let comment_left = self.position.x;
        let comment_top = self.position.y;
        let comment_right = self.position.x + self.size.width;
        let comment_bottom = self.position.y + self.size.height;

        node_left >= comment_left
            && node_right <= comment_right
            && node_top >= comment_top
            && node_bottom <= comment_bottom
    }

    /// Update contained nodes based on current bounds
    pub fn update_contained_nodes(&mut self, nodes: &[BlueprintNode]) {
        self.contained_node_ids = nodes
            .iter()
            .filter(|node| self.contains_node(node))
            .map(|node| node.id.clone())
            .collect();
    }
}

// ============================================================================
// Virtualization Stats
// ============================================================================

/// Statistics for viewport virtualization and rendering optimization.
#[derive(Clone, Debug, Default)]
pub struct VirtualizationStats {
    pub total_nodes: usize,
    pub rendered_nodes: usize,
    pub total_connections: usize,
    pub rendered_connections: usize,
    pub last_update_ms: f32,
}

// ============================================================================
// Node Implementation
// ============================================================================

impl BlueprintNode {
    pub fn from_definition(
        definition: &crate::core::definitions::NodeDefinition,
        position: Point<f32>,
    ) -> Self {
        let inputs: Vec<Pin> = definition
            .inputs
            .iter()
            .map(|pin_def| Pin {
                id: pin_def.id.clone(),
                name: pin_def.name.clone(),
                pin_type: pin_def.pin_type.clone(),
                data_type: pin_def.data_type.clone(),
            })
            .collect();

        let outputs: Vec<Pin> = definition
            .outputs
            .iter()
            .map(|pin_def| Pin {
                id: pin_def.id.clone(),
                name: pin_def.name.clone(),
                pin_type: pin_def.pin_type.clone(),
                data_type: pin_def.data_type.clone(),
            })
            .collect();

        // Determine node type based on category
        let node_definitions = crate::core::definitions::NodeDefinitions::load();
        let category = node_definitions.get_category_for_node(&definition.id);
        let node_type = match category.map(|c| c.name.as_str()) {
            Some("Events") => NodeType::Event,
            Some("Logic") => NodeType::Logic,
            Some("Math") => NodeType::Math,
            Some("Object") => NodeType::Object,
            _ => NodeType::Logic,
        };

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            definition_id: definition.id.clone(),
            title: definition.name.clone(),
            icon: definition.icon.clone(),
            node_type,
            position,
            size: {
                // Width: nodes are wide by default like UE
                // Height: derived from pin count so the body fits snugly
                let max_pins = inputs.len().max(outputs.len());
                let height = layout::node_height_for_pin_rows(max_pins);
                Size::new(240.0, height)
            },
            inputs,
            outputs,
            properties: definition.properties.clone(),
            is_selected: false,
            description: definition.description.clone(),
            color: definition.color.clone(),
        }
    }

    /// Create a typeless reroute node at the given position
    /// The type will be inferred from the first connection made to it
    pub fn create_reroute(position: Point<f32>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            definition_id: "reroute".to_string(),
            title: "Reroute".to_string(),
            icon: "•".to_string(),
            node_type: NodeType::Reroute,
            position,
            size: Size::new(16.0, 16.0), // Small size for reroute nodes
            inputs: vec![Pin {
                id: "input".to_string(),
                name: "".to_string(),
                pin_type: PinType::Input,
                data_type: DataType::Any, // Start as typeless
            }],
            outputs: vec![Pin {
                id: "output".to_string(),
                name: "".to_string(),
                pin_type: PinType::Output,
                data_type: DataType::Any, // Start as typeless
            }],
            properties: HashMap::new(),
            is_selected: false,
            description: "Reroute node for organizing connections".to_string(),
            color: None,
        }
    }
}
