//! Blueprint graph container and state management.
//!
//! This module defines the main `BlueprintGraph` type that holds all nodes,
//! connections, comments, and view state for a single blueprint document.

use super::types::{BlueprintComment, BlueprintNode, Connection, VirtualizationStats};
use gpui::*;
use std::collections::HashMap;

/// A single field in an event definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomEventField {
    pub name: String,
    pub type_name: String,
}

/// Shared definition for a custom event, stored at the panel level (mirrors
/// how macros are stored).  The On node and all Dispatch nodes derive their
/// pins from this definition.
#[derive(Clone, Debug, Default)]
pub struct EventDefinition {
    pub uid: String,
    pub name: String,
    pub fields: Vec<CustomEventField>,
    pub return_type: String,
}

/// The main container for a blueprint graph, including all nodes, connections,
/// comments, selection state, and viewport information.
#[derive(Clone, Debug, Default)]
pub struct BlueprintGraph {
    pub nodes: Vec<BlueprintNode>,
    pub connections: Vec<Connection>,
    pub comments: Vec<BlueprintComment>,
    pub selected_nodes: Vec<String>,
    pub selected_comments: Vec<String>,
    pub zoom_level: f32,
    pub pan_offset: Point<f32>,
    pub virtualization_stats: VirtualizationStats,
}
