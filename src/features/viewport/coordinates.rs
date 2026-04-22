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

use gpui::*;
use crate::core::BlueprintGraph;
use crate::editor::panel::BlueprintEditorPanel;
use ui::PixelsExt;

// ============================================================================
// Window ↔ Element Coordinate Conversions
// ============================================================================

/// Convert window-relative coordinates to graph element coordinates
///
/// Mouse events from GPUI are relative to window origin.
/// This function converts them to coordinates relative to the graph canvas element.
/// Used for graph operations: clicking nodes, selection box, dragging, etc.
///
/// # Algorithm
/// Simple subtraction: element_pos = window_pos - element_origin
///
/// # Arguments
/// * `window_pos` - Position relative to window origin
/// * `panel` - Editor panel containing the graph element bounds
///
/// # Returns
/// Position relative to the graph element origin
pub fn window_to_graph_element_pos(window_pos: Point<Pixels>, panel: &BlueprintEditorPanel) -> Point<Pixels> {
    if let Some(bounds) = &panel.graph_element_bounds {
        // Direct subtraction: mouse relative to element = mouse relative to window - element origin relative to window
        Point::new(
            window_pos.x - bounds.origin.x,
            window_pos.y - bounds.origin.y,
        )
    } else {
        // On first event before bounds captured, just return window pos as-is
        // This will be corrected on the next event after bounds are set
        window_pos
    }
}

/// Convert window-relative coordinates to panel coordinates
///
/// Used for UI elements positioned at panel level: menus, tooltips, etc.
/// Currently shares the same coordinate space as the graph element.
///
/// # Arguments
/// * `window_pos` - Position relative to window origin
/// * `panel` - Editor panel containing bounds information
///
/// # Returns
/// Position relative to the panel origin
pub fn window_to_panel_pos(window_pos: Point<Pixels>, panel: &BlueprintEditorPanel) -> Point<Pixels> {
    // Same calculation as graph element since they share the same coordinate space
    window_to_graph_element_pos(window_pos, panel)
}

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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_to_graph_conversion() {
        let mut graph = BlueprintGraph {
            nodes: vec![],
            connections: vec![],
            comments: vec![],
            selected_nodes: vec![],
            selected_comments: vec![],
            zoom_level: 1.0,
            pan_offset: Point::new(0.0, 0.0),
            virtualization_stats: Default::default(),
        };

        // No zoom, no pan - identity conversion
        let screen_pos = Point::new(px(100.0), px(200.0));
        let graph_pos = screen_to_graph_pos(screen_pos, &graph);
        assert_eq!(graph_pos.x, 100.0);
        assert_eq!(graph_pos.y, 200.0);

        // With pan offset
        graph.pan_offset = Point::new(50.0, 75.0);
        let graph_pos = screen_to_graph_pos(screen_pos, &graph);
        assert_eq!(graph_pos.x, 50.0);
        assert_eq!(graph_pos.y, 125.0);

        // With zoom
        graph.zoom_level = 2.0;
        let graph_pos = screen_to_graph_pos(screen_pos, &graph);
        assert_eq!(graph_pos.x, 0.0);
        assert_eq!(graph_pos.y, 25.0);
    }

    #[test]
    fn test_graph_to_screen_conversion() {
        let mut graph = BlueprintGraph {
            nodes: vec![],
            connections: vec![],
            comments: vec![],
            selected_nodes: vec![],
            selected_comments: vec![],
            zoom_level: 1.0,
            pan_offset: Point::new(0.0, 0.0),
            virtualization_stats: Default::default(),
        };

        // No zoom, no pan - identity conversion
        let graph_pos = Point::new(100.0, 200.0);
        let screen_pos = graph_to_screen_pos(graph_pos, &graph);
        assert_eq!(screen_pos.x, 100.0);
        assert_eq!(screen_pos.y, 200.0);

        // With pan offset
        graph.pan_offset = Point::new(50.0, 75.0);
        let screen_pos = graph_to_screen_pos(graph_pos, &graph);
        assert_eq!(screen_pos.x, 150.0);
        assert_eq!(screen_pos.y, 275.0);

        // With zoom
        graph.zoom_level = 2.0;
        let screen_pos = graph_to_screen_pos(graph_pos, &graph);
        assert_eq!(screen_pos.x, 300.0);
        assert_eq!(screen_pos.y, 550.0);
    }

    #[test]
    fn test_snap_to_grid() {
        let pos = Point::new(123.0, 456.0);
        let snapped = snap_to_grid(pos, 2.0);
        assert_eq!(snapped.x, 120.0);
        assert_eq!(snapped.y, 460.0);

        let snapped = snap_to_grid(pos, 0.4);
        assert_eq!(snapped.x, 120.0);
        assert_eq!(snapped.y, 460.0);

        let snapped = snap_to_grid(pos, 0.2);
        assert_eq!(snapped.x, 120.0);
        assert_eq!(snapped.y, 460.0);
    }

    #[test]
    fn test_parse_hex_color() {
        // Valid 6-digit RGB
        let color = parse_hex_color("#4A90E2").unwrap();
        let rgba = gpui::Rgba::from(color);
        assert!((rgba.r - 0.290).abs() < 0.01); // 0x4A / 255
        assert!((rgba.g - 0.565).abs() < 0.01); // 0x90 / 255
        assert!((rgba.b - 0.886).abs() < 0.01); // 0xE2 / 255
        assert_eq!(rgba.a, 1.0);

        // Valid 8-digit RGBA
        let color = parse_hex_color("#FF000080").unwrap();
        let rgba = gpui::Rgba::from(color);
        assert_eq!(rgba.r, 1.0);
        assert_eq!(rgba.g, 0.0);
        assert_eq!(rgba.b, 0.0);
        assert!((rgba.a - 0.502).abs() < 0.01); // 0x80 / 255

        // Without # prefix
        let color = parse_hex_color("00FF00").unwrap();
        let rgba = gpui::Rgba::from(color);
        assert_eq!(rgba.r, 0.0);
        assert_eq!(rgba.g, 1.0);
        assert_eq!(rgba.b, 0.0);

        // Invalid formats
        assert!(parse_hex_color("123").is_none());
        assert!(parse_hex_color("#12345").is_none());
        assert!(parse_hex_color("GGGGGG").is_none());
    }
}
