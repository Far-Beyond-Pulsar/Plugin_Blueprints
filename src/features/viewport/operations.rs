//! Viewport operations - pan, zoom, and camera controls

use super::coordinates::screen_to_graph_pos;
use crate::editor::workspace_panels::GraphCanvasPanel;
use gpui::*;

const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 6.0;
// Scroll deltas are normalized to pixels in the input handler. This makes a
// 60 px wheel detent approximately a 9% zoom step while keeping trackpads smooth.
const ZOOM_SENSITIVITY: f32 = 0.0015;

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
        if !delta_y.is_finite() || delta_y == 0.0 {
            return;
        }

        let screen: Point<f32> = Point::new(screen_pos.x.into(), screen_pos.y.into());

        // Get graph position under cursor before zoom
        let focus_graph_pos =
            screen_to_graph_pos(Point::new(px(screen.x), px(screen.y)), &self.graph);

        // Scale continuously with the scroll delta. Positive deltas zoom in,
        // matching the editor's existing wheel direction.
        let zoom_factor = (delta_y * ZOOM_SENSITIVITY).exp();
        let new_zoom = (self.graph.zoom_level * zoom_factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if new_zoom == self.graph.zoom_level {
            return;
        }

        // Calculate new pan to keep focus point under cursor
        let new_pan_offset = Point::new(
            (screen.x / new_zoom) - focus_graph_pos.x,
            (screen.y / new_zoom) - focus_graph_pos.y,
        );

        self.graph.zoom_level = new_zoom;
        self.graph.pan_offset = new_pan_offset;

        cx.notify();
    }
}
