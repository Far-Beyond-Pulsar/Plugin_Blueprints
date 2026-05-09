//! Viewport feature module - pan, zoom, and coordinate transformations
//!
//! This module provides all viewport-related functionality for the blueprint editor:
//! - Pan operations (start, update, end) - implemented as methods on BlueprintEditorPanel
//! - Zoom operations (mouse wheel zoom centered on cursor) - implemented as methods on BlueprintEditorPanel
//! - Coordinate conversions (window ↔ graph, screen ↔ graph)
//! - Grid snapping and utility functions
//!
//! ## Architecture
//! - `operations.rs`: Pan and zoom state mutations (impl methods on BlueprintEditorPanel)
//! - `coordinates.rs`: Coordinate conversion utilities (free functions)

pub mod coordinates;
pub mod operations;

// Re-export commonly used coordinate conversion functions
pub use coordinates::{
    graph_to_screen_pos, parse_hex_color, screen_to_graph_pos, snap_to_grid,
    window_to_graph_element_pos, window_to_panel_pos,
};
