//! Core type definitions for blueprint nodes, pins, and connections.
//!
//! This module defines the fundamental data structures used throughout the blueprint
//! editor, including nodes, pins, connections, and comments. These types represent
//! the visual and logical structure of a blueprint graph.

use serde::{Deserialize, Serialize};


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

    /// Pin colours for the script math value types. Presentation metadata:
    /// the VM and reflection know nothing about colours, and these types
    /// have no `#[reflect(color)]` of their own. Vectors share a warm hue
    /// family; rotations and matrices are
    /// distinct.
    fn math_type_color(type_name: &str) -> Option<[f32; 4]> {
        Some(match type_name {
            "Vec2" => [0.98, 0.78, 0.18, 1.0],
            "Vec3" | "DVec3" => [0.98, 0.62, 0.10, 1.0],
            "Vec4" => [0.95, 0.45, 0.10, 1.0],
            "Quat" => [0.66, 0.40, 0.95, 1.0],
            "Mat4" => [0.30, 0.70, 0.78, 1.0],
            _ => return None,
        })
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
        if let Some(color) = Self::math_type_color(&self.type_name) {
            return color;
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
/// - `BytecodeVm` — graph → engine script module (`events/.build/module.json`)
///   run by the game's script runtime.
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
// Node classification
// ============================================================================

/// What a node *is* in Blueprint terms, derived from its `node_type` string.
/// The document stores only that string; behaviour that depends on the kind
/// (macro entry/exit, subgraph calls, custom events) asks here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeType {
    Reroute,
    /// Typed conversion node rendered as a FROM/INTO pill.
    Conversion,
    /// Entry point for macro/collapsed graphs.
    MacroEntry,
    /// Exit point for macro/collapsed graphs.
    MacroExit,
    /// Call to a macro or collapsed subgraph.
    SubGraphCall,
    /// On-node of a custom event (`custom_event:<uid>`).
    CustomEvent,
    /// Dispatch node of a custom event (`custom_event_dispatch:<uid>`).
    CustomEventDispatch,
    /// Everything else (catalogue nodes, engine events).
    Other,
}

impl NodeType {
    pub fn of(node_type: &str) -> NodeType {
        match node_type {
            "reroute" => NodeType::Reroute,
            "macro_entry" => NodeType::MacroEntry,
            "macro_exit" => NodeType::MacroExit,
            t if t.starts_with("conversion:") => NodeType::Conversion,
            t if t.starts_with("custom_event_dispatch:") => NodeType::CustomEventDispatch,
            t if t.starts_with("custom_event:") => NodeType::CustomEvent,
            t if crate::core::subgraph_ref::SubGraphReference::id_from_definition_id(t).is_some() => {
                NodeType::SubGraphCall
            }
            _ => NodeType::Other,
        }
    }
}

/// Direction of data flow for a pin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PinType {
    Input,
    Output,
}
