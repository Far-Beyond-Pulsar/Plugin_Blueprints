//! Core module for the Blueprint Editor
//!
//! This module contains the fundamental types, data structures, and functionality
//! that power the blueprint graph system. It is organized into several submodules:
//!
//! - `types`: Core data structures (nodes, pins, connections, comments)
//! - `graph`: The main blueprint graph container and state management
//! - `definitions`: Node definition system for loading and managing node metadata
//! - `events`: Actions and event types for editor interactions
//! - `serialization`: Serde helpers for GPUI types and blueprint persistence

pub mod definitions;
pub mod events;
pub mod graph;
pub mod serialization;
pub mod types;

// Re-export commonly used types for convenience
pub use types::{
    BlueprintComment, BlueprintNode, CompilationState, CompilationStatus, Connection, NodeType,
    Pin, PinType, VirtualizationStats,
};

pub use graph::BlueprintGraph;

pub use definitions::{NodeCategory, NodeDefinition, NodeDefinitions, PinDefinition};

pub use events::{
    CopyNode, DeleteNode, DisconnectPin, DuplicateNode, OpenAddNodeMenu, OpenEngineLibraryRequest,
    PasteNode,
};
