//! Viewport operations - pan, zoom, and camera controls

use super::coordinates::screen_to_graph_pos;
use crate::core::BlueprintGraph;
use crate::editor::workspace_panels::GraphCanvasPanel;
use gpui::*;

impl GraphCanvasPanel {
    /// Start panning the viewport
    pub fn start_panning(&mut self, start_pos: Point<f32>, cx: &mut Context<Self>) {
        self.is_panning = true;
        self.pan_start = start_pos;
        self.pan_start_offset = self.graph.pan_offset;
        cx.notify();
    }

    /// Check if currently panning
    pub fn is_panning(&self) -> bool {
        self.is_panning
    }

    /// Update pan position during a pan gesture
    pub fn update_pan(&mut self, current_pos: Point<f32>, cx: &mut Context<Self>) {
        if self.is_panning {
            let delta = Point::new(
                current_pos.x - self.pan_start.x,
                current_pos.y - self.pan_start.y,
            );
            self.graph.pan_offset = Point::new(
                self.pan_start_offset.x + delta.x / self.graph.zoom_level,
                self.pan_start_offset.y + delta.y / self.graph.zoom_level,
            );
            cx.notify();
        }
    }

    /// End panning gesture
    pub fn end_panning(&mut self, cx: &mut Context<Self>) {
        self.is_panning = false;
        cx.notify();
    }

    /// Handle zoom with mouse wheel
    pub fn handle_zoom(&mut self, delta_y: f32, screen_pos: Point<Pixels>, cx: &mut Context<Self>) {
        let screen: Point<f32> = Point::new(screen_pos.x.into(), screen_pos.y.into());

        // Get graph position under cursor before zoom
        let focus_graph_pos =
            screen_to_graph_pos(Point::new(px(screen.x), px(screen.y)), &self.graph);

        // Calculate new zoom level (inverted scroll direction)
        let zoom_factor = if delta_y > 0.0 { 1.1 } else { 0.9 };
        let new_zoom = (self.graph.zoom_level * zoom_factor).clamp(0.05, 6.0);

        // Calculate new pan to keep focus point under cursor
        let mut new_pan_offset = Point::new(
            (screen.x / new_zoom) - focus_graph_pos.x,
            (screen.y / new_zoom) - focus_graph_pos.y,
        );

        // Apply temporarily to measure coordinate differences
        self.graph.zoom_level = new_zoom;
        self.graph.pan_offset = new_pan_offset;

        // Measure screen position after zoom
        let screen_after = graph_to_screen_pos_internal(focus_graph_pos, &self.graph);
        let diff_x = screen_after.x - screen.x;
        let diff_y = screen_after.y - screen.y;

        // Correct pan to compensate for coordinate system differences
        new_pan_offset.x -= diff_x / new_zoom;
        new_pan_offset.y -= diff_y / new_zoom;

        // Commit corrected values
        self.graph.zoom_level = new_zoom;
        self.graph.pan_offset = new_pan_offset;

        cx.notify();
    }
}

/// Internal helper for graph-to-screen conversion used during zoom
fn graph_to_screen_pos_internal(graph_pos: Point<f32>, graph: &BlueprintGraph) -> Point<f32> {
    Point::new(
        (graph_pos.x + graph.pan_offset.x) * graph.zoom_level,
        (graph_pos.y + graph.pan_offset.y) * graph.zoom_level,
    )
}
