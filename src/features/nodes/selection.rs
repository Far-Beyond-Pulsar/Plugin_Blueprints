//! Selection operations - selection box and multi-selection

use crate::core::graph_entity::GraphEntity;
use crate::core::types::{BlueprintNode, Connection};
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::rendering::graph::NodeGraphRenderer;
use gpui::*;

impl GraphCanvasPanel {
    /// Select a single node (or clear selection if None)
    pub fn select_node(&mut self, node_id: Option<String>, cx: &mut Context<Self>) {
        self.graph.selected_nodes.clear();
        if let Some(id) = node_id {
            self.graph.selected_nodes.push(id);
        }
        cx.notify();
    }

    /// Start selection drag (selection box)
    pub fn start_selection_drag(
        &mut self,
        start_pos: Point<f32>,
        _add_to_selection: bool,
        cx: &mut Context<Self>,
    ) {
        self.selection_start = Some(start_pos);
        self.selection_end = Some(start_pos);
        cx.notify();
    }

    /// Check if currently selecting
    pub fn is_selecting(&self) -> bool {
        self.selection_start.is_some() && self.selection_end.is_some()
    }

    /// Update selection drag
    pub fn update_selection_drag(&mut self, current_pos: Point<f32>, cx: &mut Context<Self>) {
        if self.selection_start.is_some() {
            self.selection_end = Some(current_pos);
            self.update_node_selection_from_drag(cx);
        }
    }

    /// End selection drag
    pub fn end_selection_drag(&mut self, cx: &mut Context<Self>) {
        // If selection box was very small, treat as click and clear selection
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            let distance = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();
            if distance < 5.0 {
                self.graph.selected_nodes.clear();
                tracing::info!("[SELECTION] Cleared selection (click on empty space)");
            }
        }

        self.selection_start = None;
        self.selection_end = None;
        cx.notify();
    }

    /// Update node selection based on current drag area (using GraphEntity trait)
    fn update_node_selection_from_drag(&mut self, cx: &mut Context<Self>) {
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            let min = Point::new(start.x.min(end.x), start.y.min(end.y));
            let max = Point::new(start.x.max(end.x), start.y.max(end.y));

            self.graph.selected_nodes = self
                .graph
                .nodes
                .iter()
                .filter(|node| node.intersects_rect(min, max))
                .map(|node| node.id.clone())
                .collect();

            self.graph.selected_comments = self
                .graph
                .comments
                .iter()
                .filter(|comment| comment.intersects_rect(min, max))
                .map(|comment| comment.id.clone())
                .collect();

            cx.notify();
        }
    }

    /// Delete all selected nodes and comments (unified)
    pub fn delete_selected_nodes(&mut self, cx: &mut Context<Self>) {
        // Use unified entity deletion
        self.delete_selected_entities(cx);
    }

    /// Handle double-click on connection to create reroute node
    pub fn handle_empty_space_click(
        &mut self,
        graph_pos: Point<f32>,
        cx: &mut Context<Self>,
    ) -> bool {
        let now = std::time::Instant::now();
        let is_double_click = if let (Some(last_time), Some(last_pos)) =
            (self.last_click_time, self.last_click_pos)
        {
            let time_diff = now.duration_since(last_time).as_millis();
            let pos_diff =
                ((graph_pos.x - last_pos.x).powi(2) + (graph_pos.y - last_pos.y).powi(2)).sqrt();
            tracing::info!(
                "[REROUTE] Double-click check: time_diff={}ms, pos_diff={:.2}px",
                time_diff,
                pos_diff
            );
            time_diff < 500 && pos_diff < 50.0
        } else {
            false
        };

        if is_double_click {
            tracing::info!("[REROUTE] Double-click detected! Checking for nearby connections...");

            if let Some(connection) = self.find_connection_near_point(graph_pos) {
                tracing::info!("[REROUTE] Found connection near click point!");

                if let Some(data_type) = self.get_connection_data_type(&connection) {
                    // Create reroute node
                    let reroute_pos = NodeGraphRenderer::snap_to_grid(graph_pos);
                    let reroute_node = BlueprintNode::create_reroute(reroute_pos);
                    let reroute_id = reroute_node.id.clone();

                    self.graph.nodes.push(reroute_node);

                    // Split connection
                    let from_node = connection.source_node.clone();
                    let from_pin = connection.source_pin.clone();
                    let to_node = connection.target_node.clone();
                    let to_pin = connection.target_pin.clone();

                    // Remove original connection
                    self.graph.connections.retain(|c| c.id != connection.id);

                    // Create two new connections through reroute
                    self.graph.connections.push(Connection {
                        id: uuid::Uuid::new_v4().to_string(),
                        source_node: from_node,
                        source_pin: from_pin,
                        target_node: reroute_id.clone(),
                        target_pin: "input".to_string(),
                        connection_type: connection.connection_type.clone(),
                    });

                    self.graph.connections.push(Connection {
                        id: uuid::Uuid::new_v4().to_string(),
                        source_node: reroute_id.clone(),
                        source_pin: "output".to_string(),
                        target_node: to_node,
                        target_pin: to_pin,
                        connection_type: connection.connection_type.clone(),
                    });

                    // Propagate types
                    self.propagate_reroute_types(reroute_id, data_type, cx);

                    cx.notify();
                    self.last_click_time = None;
                    self.last_click_pos = None;
                    return true;
                }
            }

            self.last_click_time = None;
            self.last_click_pos = None;
        } else {
            self.last_click_time = Some(now);
            self.last_click_pos = Some(graph_pos);
        }

        false
    }
}
