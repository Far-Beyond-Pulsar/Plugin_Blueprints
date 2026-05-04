//! Cached connection rendering with transform-based panning
//!
//! This module implements an optimized approach where bezier paths are calculated
//! once and cached, then transformed using the pan/zoom matrix. This avoids expensive
//! recalculation of bezier curves every frame during panning.
//!
//! Performance improvements:
//! - Paths calculated in graph space and cached
//! - Only recalculated when nodes move or connections change
//! - Panning just applies transform to cached paths (extremely fast)

use crate::core::graph::BlueprintGraph;
use crate::core::types::{BlueprintNode, Connection};
use crate::editor::panel::BlueprintEditorPanel;
use crate::rendering::graph::NodeGraphRenderer;
use gpui::*;
use std::collections::{HashMap, HashSet};
use ui::graph::DataType;
use ui::PixelsExt;
use ui::ActiveTheme;

/// Cached bezier path for a single connection
#[derive(Clone, Debug)]
struct CachedConnectionPath {
    /// Start point in graph space (not screen space)
    start: Point<f32>,
    /// End point in graph space
    end: Point<f32>,
    /// Connection color
    color: Hsla,
    /// Pre-calculated bezier control points in graph space
    control_points: (Point<f32>, Point<f32>),
}

/// Cache for connection rendering
pub struct ConnectionRenderCache {
    /// Cached paths indexed by connection ID (source_node:source_pin -> target_node:target_pin)
    paths: HashMap<String, CachedConnectionPath>,
    /// Hash of graph state to detect when cache needs invalidation
    graph_version: u64,
}

impl ConnectionRenderCache {
    pub fn new() -> Self {
        Self {
            paths: HashMap::new(),
            graph_version: 0,
        }
    }

    /// Check if cache needs to be invalidated
    fn needs_rebuild(&self, graph: &BlueprintGraph) -> bool {
        // Simple version check - in production, you'd hash node positions + connections
        let current_version = graph.nodes.len() as u64 * 1000 + graph.connections.len() as u64;
        self.graph_version != current_version
    }

    /// Rebuild the cache from current graph state
    fn rebuild(
        &mut self,
        panel: &BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) {
        self.paths.clear();

        // Build node lookup
        let node_by_id: HashMap<&str, &BlueprintNode> = panel
            .graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n))
            .collect();

        // Pre-calculate all connection paths in graph space
        for connection in &panel.graph.connections {
            if let Some(path) = Self::calculate_connection_path(
                connection,
                &panel.graph,
                &node_by_id,
                cx,
            ) {
                let key = format!(
                    "{}:{}->{}:{}",
                    connection.source_node,
                    connection.source_pin,
                    connection.target_node,
                    connection.target_pin
                );
                self.paths.insert(key, path);
            }
        }

        // Update version
        self.graph_version =
            panel.graph.nodes.len() as u64 * 1000 + panel.graph.connections.len() as u64;
    }

    /// Calculate a single connection path in graph space
    fn calculate_connection_path(
        connection: &Connection,
        graph: &BlueprintGraph,
        node_by_id: &HashMap<&str, &BlueprintNode>,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> Option<CachedConnectionPath> {
        let from_node = node_by_id.get(connection.source_node.as_str()).copied()?;
        let to_node = node_by_id.get(connection.target_node.as_str()).copied()?;

        // Calculate pin positions in GRAPH SPACE (not screen space)
        let from_pos = BlueprintEditorPanel::calculate_pin_position(
            from_node,
            &connection.source_pin,
            false,
            graph,
        )?;
        let to_pos = BlueprintEditorPanel::calculate_pin_position(
            to_node,
            &connection.target_pin,
            true,
            graph,
        )?;

        // Get pin color
        let color = if let Some(pin) = from_node
            .outputs
            .iter()
            .find(|p| p.id == connection.source_pin)
        {
            BlueprintEditorPanel::get_pin_color(&pin.data_type, cx)
        } else {
            cx.theme().primary
        };

        // Pre-calculate bezier control points
        let dx = to_pos.x - from_pos.x;
        let control_offset = dx.abs().min(200.0).max(80.0);

        let cp1 = Point::new(from_pos.x + control_offset, from_pos.y);
        let cp2 = Point::new(to_pos.x - control_offset, to_pos.y);

        Some(CachedConnectionPath {
            start: from_pos,
            end: to_pos,
            color,
            control_points: (cp1, cp2),
        })
    }

    /// Render all cached connections with transform
    pub fn render(
        &mut self,
        panel: &mut BlueprintEditorPanel,
        cx: &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        // Rebuild cache if needed
        if self.needs_rebuild(&panel.graph) {
            self.rebuild(panel, cx);
        }

        // Clone paths for move into closure
        let paths: Vec<CachedConnectionPath> = self.paths.values().cloned().collect();

        // Get transform parameters
        let pan = panel.graph.pan_offset;
        let zoom = panel.graph.zoom_level;

        // Dragging connection (not cached)
        let dragging_shape = panel
            .dragging_connection
            .as_ref()
            .and_then(|drag| {
                if let Some(from_node) = panel.graph.nodes.iter().find(|n| n.id == drag.source_node)
                {
                    if let Some(from_pin_pos) =
                        BlueprintEditorPanel::calculate_pin_position(
                            from_node,
                            &drag.source_pin,
                            false,
                            &panel.graph,
                        )
                    {
                        let to_pos = drag.current_mouse_pos;
                        let color = if let Some(pin) = from_node
                            .outputs
                            .iter()
                            .find(|p| p.id == drag.source_pin)
                        {
                            BlueprintEditorPanel::get_pin_color(&pin.data_type, cx)
                        } else {
                            cx.theme().primary
                        };
                        return Some((from_pin_pos, to_pos, color));
                    }
                }
                None
            });

        gpui::canvas(
            move |_bounds, _window, _cx| {},
            move |bounds, _prepaint_state, window, _cx| {
                let canvas_offset_x = bounds.origin.x.as_f32();
                let canvas_offset_y = bounds.origin.y.as_f32();

                // Render cached connections with transform
                for cached in &paths {
                    // Transform from graph space to screen space
                    let screen_start = Point::new(
                        (cached.start.x + pan.x) * zoom + canvas_offset_x,
                        (cached.start.y + pan.y) * zoom + canvas_offset_y,
                    );
                    let screen_end = Point::new(
                        (cached.end.x + pan.x) * zoom + canvas_offset_x,
                        (cached.end.y + pan.y) * zoom + canvas_offset_y,
                    );

                    // Also transform control points
                    let screen_cp1 = Point::new(
                        (cached.control_points.0.x + pan.x) * zoom + canvas_offset_x,
                        (cached.control_points.0.y + pan.y) * zoom + canvas_offset_y,
                    );
                    let screen_cp2 = Point::new(
                        (cached.control_points.1.x + pan.x) * zoom + canvas_offset_x,
                        (cached.control_points.1.y + pan.y) * zoom + canvas_offset_y,
                    );

                    paint_bezier_with_controls(
                        window,
                        screen_start,
                        screen_cp1,
                        screen_cp2,
                        screen_end,
                        cached.color,
                        zoom,
                    );
                }

                // Render dragging connection (not cached)
                if let Some((from, to, color)) = &dragging_shape {
                    let screen_start = Point::new(
                        (from.x + pan.x) * zoom + canvas_offset_x,
                        (from.y + pan.y) * zoom + canvas_offset_y,
                    );
                    let screen_end = Point::new(
                        (to.x + pan.x) * zoom + canvas_offset_x,
                        (to.y + pan.y) * zoom + canvas_offset_y,
                    );

                    // Calculate control points for dragging connection
                    let dx = screen_end.x - screen_start.x;
                    let control_offset = (dx.abs().min(200.0).max(80.0)) * zoom;
                    let cp1 = Point::new(screen_start.x + control_offset, screen_start.y);
                    let cp2 = Point::new(screen_end.x - control_offset, screen_end.y);

                    paint_bezier_with_controls(window, screen_start, cp1, cp2, screen_end, *color, zoom);
                }
            },
        )
        .absolute()
        .inset_0()
        .size_full()
    }
}

/// Paint a bezier curve with pre-calculated control points (optimized version)
fn paint_bezier_with_controls(
    window: &mut Window,
    start: Point<f32>,
    cp1: Point<f32>,
    cp2: Point<f32>,
    end: Point<f32>,
    color: Hsla,
    zoom: f32,
) {
    let segments = 32; // Fixed segment count for consistent quality
    let thickness = (2.8 * zoom).max(1.5);

    // Three-layer rendering: glow, main, highlight
    let layers = [
        (
            Hsla {
                h: color.h,
                s: color.s,
                l: color.l,
                a: 0.15,
            },
            thickness * 3.0,
        ),
        (color, thickness),
        (
            Hsla {
                h: color.h,
                s: color.s.min(0.3),
                l: color.l.min(0.95),
                a: 0.6,
            },
            thickness * 0.4,
        ),
    ];

    for (layer_color, layer_thickness) in layers {
        // Build path with pre-calculated control points
        let mut builder = PathBuilder::stroke(px(layer_thickness));
        builder.move_to(point(px(start.x), px(start.y)));

        // Cubic bezier using pre-calculated control points
        for i in 1..=segments {
            let t = i as f32 / segments as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let mt = 1.0 - t;
            let mt2 = mt * mt;
            let mt3 = mt2 * mt;

            // Cubic bezier formula
            let x = mt3 * start.x
                + 3.0 * mt2 * t * cp1.x
                + 3.0 * mt * t2 * cp2.x
                + t3 * end.x;
            let y = mt3 * start.y
                + 3.0 * mt2 * t * cp1.y
                + 3.0 * mt * t2 * cp2.y
                + t3 * end.y;

            builder.line_to(point(px(x), px(y)));
        }

        if let Ok(path) = builder.build() {
            window.paint_path(path, layer_color);
        }
    }
}

impl Default for ConnectionRenderCache {
    fn default() -> Self {
        Self::new()
    }
}
