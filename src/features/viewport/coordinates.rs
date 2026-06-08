//! Coordinate conversion utilities
//!
//! This module provides functions for converting between different coordinate spaces:
//! - Window coordinates: Relative to the application window
//! - Element coordinates: Relative to the graph canvas element
//! - Panel coordinates: Relative to the editor panel
//! - Graph coordinates: Logical positions in the blueprint graph
//! - Screen coordinates: Physical pixel positions after zoom/pan transformation
//!
//! It also includes utility functions for grid snapping and color parsing.

use crate::core::BlueprintGraph;
use gpui::*;
use ui::PixelsExt;

// ============================================================================
// Screen ↔ Graph Coordinate Conversions
// ============================================================================

/// Convert screen coordinates to graph coordinates
///
/// Transforms physical screen positions (after zoom/pan) to logical graph positions.
/// This is the inverse of `graph_to_screen_pos`.
///
/// # Formula
/// ```text
/// graph_x = (screen_x / zoom) - pan_x
/// graph_y = (screen_y / zoom) - pan_y
/// ```
///
/// # Arguments
/// * `screen_pos` - Physical position in screen space (pixels)
/// * `graph` - Blueprint graph containing zoom and pan state
///
/// # Returns
/// Logical position in graph coordinate space
pub fn screen_to_graph_pos(screen_pos: Point<Pixels>, graph: &BlueprintGraph) -> Point<f32> {
    Point::new(
        (screen_pos.x.as_f32() / graph.zoom_level) - graph.pan_offset.x,
        (screen_pos.y.as_f32() / graph.zoom_level) - graph.pan_offset.y,
    )
}

/// Convert graph coordinates to screen coordinates
///
/// Transforms logical graph positions to physical screen positions.
/// Applies zoom and pan transformations.
/// This is the inverse of `screen_to_graph_pos`.
///
/// # Formula
/// ```text
/// screen_x = (graph_x + pan_x) * zoom
/// screen_y = (graph_y + pan_y) * zoom
/// ```
///
/// # Arguments
/// * `graph_pos` - Logical position in graph space
/// * `graph` - Blueprint graph containing zoom and pan state
///
/// # Returns
/// Physical position in screen space
pub fn graph_to_screen_pos(graph_pos: Point<f32>, graph: &BlueprintGraph) -> Point<f32> {
    Point::new(
        (graph_pos.x + graph.pan_offset.x) * graph.zoom_level,
        (graph_pos.y + graph.pan_offset.y) * graph.zoom_level,
    )
}

// ============================================================================
// Grid Snapping
// ============================================================================

/// Snaps a position to the fixed 10px graph grid.
///
/// # Arguments
/// * `pos` - Position to snap to grid
/// * `zoom_level` - Unused; retained for API compatibility
///
/// # Returns
/// Position snapped to the nearest grid point
pub fn snap_to_grid(pos: Point<f32>, _zoom_level: f32) -> Point<f32> {
    let grid_size = 10.0;

    Point::new(
        (pos.x / grid_size).round() * grid_size,
        (pos.y / grid_size).round() * grid_size,
    )
}

// ============================================================================
// Color Utilities
// ============================================================================

/// Parses a hex color string (e.g., "#4A90E2") into a GPUI Hsla color
///
/// Supports both 6-digit RGB format (#RRGGBB) and 8-digit RGBA format (#RRGGBBAA).
///
/// # Arguments
/// * `hex` - Hex color string (with or without leading '#')
///
/// # Returns
/// * `Some(Hsla)` - Parsed color in HSLA format
/// * `None` - Invalid hex string
///
/// # Examples
/// ```
/// # use gpui::*;
/// let blue = parse_hex_color("#4A90E2");
/// let transparent_red = parse_hex_color("#FF000080");
/// ```
pub fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim_start_matches('#');

    // Parse RGB values
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;

        let rgba = gpui::Rgba { r, g, b, a: 1.0 };
        Some(gpui::Hsla::from(rgba))
    } else if hex.len() == 8 {
        // Support RGBA format as well
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
        let a = u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0;

        let rgba = gpui::Rgba { r, g, b, a };
        Some(gpui::Hsla::from(rgba))
    } else {
        None
    }
}
