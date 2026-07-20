//! Core type definitions for blueprint nodes, pins, and connections.
//!
//! This module defines the fundamental data structures used throughout the blueprint
//! editor, including nodes, pins, connections, and comments. These types represent
//! the visual and logical structure of a blueprint graph.

use gpui::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ui::color_picker::ColorPickerState;

use crate::core::graph_entity::GraphEntity;
use crate::rendering::layout;

// ============================================================================
// Pin Data Type — canonical reflection-backed type representation
// ============================================================================

/// Canonical representation of a pin's data type.
///
/// Stores only the registry-stable `type_name` (e.g. `"f32"`, `"Vec<Entity>"`,
/// `"execution"`, `"?"`). All display (color/icon) and compatibility logic
/// resolves through `pulsar_reflection::RUNTIME_TYPE_REGISTRY` from this name —
/// `RuntimeTypeInfo`/`TypeId` is the single canonical type system; this struct
/// is just the serialization-stable handle into it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PinDataType {
    pub type_name: String,
}

impl PinDataType {
    /// Construct from a type name string (e.g. from `NodeParameter::ty` or a
    /// saved blueprint file). Stored verbatim; resolution happens on demand.
    pub fn from_type_str(type_name: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
        }
    }

    /// The execution-flow pseudo-type (white triangle pins, not data-bearing).
    pub fn execution() -> Self {
        Self::from_type_str("execution")
    }

    /// A typeless wildcard pin (e.g. a freshly-placed reroute node) whose type
    /// will be inferred from the first connection made to it.
    pub fn wildcard() -> Self {
        Self::from_type_str("?")
    }

    pub fn is_execution(&self) -> bool {
        self.type_name == "execution"
    }

    /// Whether this pin matches any type — either a literal wildcard marker
    /// (`?`/`_`/single uppercase letter, used by reroutes/generics before
    /// inference) or a type that resolves to `RuntimeTypeInfo::wildcard()`.
    pub fn is_wildcard(&self) -> bool {
        if self.is_execution() {
            return false;
        }
        if matches!(self.type_name.as_str(), "?" | "_") {
            return true;
        }
        if self.type_name.len() == 1 && self.type_name.chars().next().unwrap().is_uppercase() {
            return true;
        }
        self.runtime_type().map_or(false, |info| info.is_wildcard())
    }

    /// Resolve to the canonical reflection type info, if registered.
    pub fn runtime_type(&self) -> Option<&'static pulsar_reflection::RuntimeTypeInfo> {
        if self.is_execution() {
            return None;
        }
        pulsar_reflection::RUNTIME_TYPE_REGISTRY.get_by_name(&self.type_name)
    }

    /// Resolve to the reflection `TypeId`, if registered — the key used to
    /// look up registered property editors and check structural compatibility.
    pub fn type_id(&self) -> Option<std::any::TypeId> {
        self.runtime_type().map(|info| info.type_id)
    }

    /// Resolve this type's display color as RGBA in `[0.0, 1.0]`.
    ///
    /// The single source of truth for pin/badge color: execution pins are
    /// red, everything else defers to `RuntimeTypeInfo::resolve_color()`
    /// (declared `#[reflect(color = "...")]` value, or a deterministic
    /// hash-based fallback) — never computed by a UI framework crate.
    pub fn display_color(&self) -> [f32; 4] {
        if self.is_execution() {
            return [1.0, 0.0, 0.0, 1.0];
        }
        match self.runtime_type() {
            Some(info) => info.resolve_color(),
            None => [1.0, 1.0, 1.0, 1.0],
        }
    }

    /// Whether a connection between pins of these two types is allowed.
    ///
    /// Rules (preserved from the legacy string-based `DataType` system):
    /// - Execution pins only connect to execution pins.
    /// - Wildcards (typeless reroutes/generics) connect to anything.
    /// - Otherwise types must resolve to the exact same registered `TypeId`
    ///   (no implicit numeric/string conversions).
    pub fn is_compatible_with(&self, other: &PinDataType) -> bool {
        if self.is_execution() || other.is_execution() {
            return self.is_execution() && other.is_execution();
        }
        if self.is_wildcard() || other.is_wildcard() {
            return true;
        }
        match (self.type_id(), other.type_id()) {
            (Some(a), Some(b)) => a == b,
            _ => self.type_name == other.type_name,
        }
    }
}

impl std::fmt::Display for PinDataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.type_name)
    }
}

// ============================================================================
// Compilation Status
// ============================================================================

/// Which backend the compile button targets.
///
/// - `DirectRust` — existing path: graph → Rust source written to
///   `events/events.rs` inside the class directory.
/// - `BytecodeVm` — new path: graph → PBGC bytecode executed immediately
///   against the embedded `pulsar_std` native cdylib.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum CompileMode {
    #[default]
    DirectRust,
    BytecodeVm,
}

impl CompileMode {
    pub fn label(&self) -> &'static str {
        match self {
            CompileMode::DirectRust => "Rust",
            CompileMode::BytecodeVm => "VM",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            CompileMode::DirectRust => CompileMode::BytecodeVm,
            CompileMode::BytecodeVm => CompileMode::DirectRust,
        }
    }
}

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
    CustomEvent,   // On-node for a custom event definition (definition_id: "custom_event:<uid>")
    CustomEventDispatch, // Dispatch node for a custom event (definition_id: "custom_event_dispatch:<uid>")
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
    pub data_type: PinDataType,
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
    pub fn new<E: 'static>(
        position: Point<f32>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<E>,
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

    /// Check if another comment is fully contained within this comment's bounds.
    pub fn contains_comment(&self, other: &BlueprintComment) -> bool {
        let other_left = other.position.x;
        let other_top = other.position.y;
        let other_right = other.position.x + other.size.width;
        let other_bottom = other.position.y + other.size.height;

        let comment_left = self.position.x;
        let comment_top = self.position.y;
        let comment_right = self.position.x + self.size.width;
        let comment_bottom = self.position.y + self.size.height;

        other_left >= comment_left
            && other_right <= comment_right
            && other_top >= comment_top
            && other_bottom <= comment_bottom
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

        // Determine node type. Event entry-points are identified by their
        // underlying Blueprint node type (not by category — events such as
        // `on_input_key`/`on_input_action` live in the "Input" category).
        // Everything else falls back to a category-based visual grouping.
        let node_definitions = crate::core::definitions::NodeDefinitions::load();
        let node_type = if definition.is_event {
            NodeType::Event
        } else {
            let category = node_definitions.get_category_for_node(&definition.id);
            match category.map(|c| c.name.as_str()) {
                Some("Logic") => NodeType::Logic,
                Some("Math") => NodeType::Math,
                Some("Object") => NodeType::Object,
                _ => NodeType::Logic,
            }
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
                data_type: PinDataType::wildcard(), // Start as typeless
            }],
            outputs: vec![Pin {
                id: "output".to_string(),
                name: "".to_string(),
                pin_type: PinType::Output,
                data_type: PinDataType::wildcard(), // Start as typeless
            }],
            properties: HashMap::new(),
            is_selected: false,
            description: "Reroute node for organizing connections".to_string(),
            color: None,
        }
    }
}

// ============================================================================
// GraphEntity Trait Implementations
// ============================================================================

impl GraphEntity for BlueprintNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn position(&self) -> Point<f32> {
        self.position
    }

    fn set_position(&mut self, pos: Point<f32>) {
        self.position = pos;
    }

    fn size(&self) -> Size<f32> {
        self.size
    }
}

impl GraphEntity for BlueprintComment {
    fn id(&self) -> &str {
        &self.id
    }

    fn position(&self) -> Point<f32> {
        self.position
    }

    fn set_position(&mut self, pos: Point<f32>) {
        self.position = pos;
    }

    fn size(&self) -> Size<f32> {
        self.size
    }
}
