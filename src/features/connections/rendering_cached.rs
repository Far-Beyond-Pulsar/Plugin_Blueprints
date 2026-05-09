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
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
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
    /// Hash of node positions to detect when cache needs invalidation
    node_positions_hash: u64,
    /// Number of connections to detect when connections change
    connection_count: usize,
}

impl ConnectionRenderCache {
    pub fn new() -> Self {
        Self {
            paths: HashMap::new(),
            node_positions_hash: 0,
            connection_count: 0,
        }
    }

    /// Check if cache needs to be invalidated
    fn needs_rebuild(&self, graph: &BlueprintGraph) -> bool {
        // Hash node positions to detect movement
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for node in &graph.nodes {
            // Hash node ID and position
            use std::hash::{Hash, Hasher};
            node.id.hash(&mut hasher);
            (node.position.x as i32).hash(&mut hasher);
            (node.position.y as i32).hash(&mut hasher);
        }
        let current_hash = hasher.finish();

        // Rebuild if positions changed or connection count changed
        current_hash != self.node_positions_hash || graph.connections.len() != self.connection_count
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

        // Update hash and count
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for node in &panel.graph.nodes {
            use std::hash::{Hash, Hasher};
            node.id.hash(&mut hasher);
            (node.position.x as i32).hash(&mut hasher);
            (node.position.y as i32).hash(&mut hasher);
        }
        self.node_positions_hash = hasher.finish();
        self.connection_count = panel.graph.connections.len();
    }

    /// Calculate pin position in graph space (without zoom/pan transform)
    fn calculate_pin_position_graph_space(
        node: &BlueprintNode,
        pin_id: &str,
        is_input: bool,
    ) -> Option<Point<f32>> {
        use crate::core::types::NodeType;
        use crate::rendering::layout;

        // Reroute nodes are a single dot at their graph position
        if node.node_type == NodeType::Reroute {
            return Some(node.position);
        }

        let row = if is_input {
            node.inputs.iter().position(|p| p.id == pin_id)?
        } else {
            node.outputs.iter().position(|p| p.id == pin_id)?
        };

        // Calculate Y position in graph space (no zoom multiplier)
        let pin_y = node.position.y
            + layout::HEADER_H
            + layout::SEP_H
            + layout::BODY_PAD
            + row as f32 * (layout::PIN_ROW_H + layout::PIN_GAP)
            + layout::PIN_ROW_H * 0.5;

        // Calculate X position in graph space
        let pin_x = if is_input {
            node.position.x + layout::BODY_PAD
        } else {
            node.position.x + node.size.width - layout::BODY_PAD
        };

        Some(Point::new(pin_x, pin_y))
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
        let from_pos = Self::calculate_pin_position_graph_space(
            from_node,
            &connection.source_pin,
            false,
        )?;
        let to_pos = Self::calculate_pin_position_graph_space(
            to_node,
            &connection.target_pin,
            true,
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

        // Pre-calculate bezier control points in graph space
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

        // Dragging connection (not cached - in graph space)
        let dragging_shape = panel
            .dragging_connection
            .as_ref()
            .and_then(|drag| {
                if let Some(from_node) = panel.graph.nodes.iter().find(|n| n.id == drag.source_node)
                {
                    if let Some(from_pin_pos) =
                        Self::calculate_pin_position_graph_space(
                            from_node,
                            &drag.source_pin,
                            false,
                        )
                    {
                        let to_pos = drag.current_mouse_pos; // Already in graph space
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
