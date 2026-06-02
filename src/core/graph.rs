//! Blueprint graph container and state management.
//!
//! This module defines the main `BlueprintGraph` type that holds all nodes,
//! connections, comments, and view state for a single blueprint document.

use super::types::{BlueprintComment, BlueprintNode, Connection, VirtualizationStats};
use gpui::*;

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
