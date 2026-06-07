//! Connection operations - dragging and managing connections between nodes

use crate::core::types::{BlueprintNode, Connection, NodeType};
use crate::editor::workspace_panels::GraphCanvasPanel;
use gpui::*;
use ui::graph::DataType as GraphDataType;

/// Connection drag state
#[derive(Clone, Debug)]
pub struct ConnectionDrag {
    pub source_node: String,
    pub source_pin: String,
    pub source_pin_type: GraphDataType,
    pub current_mouse_pos: Point<f32>,
    pub target_pin: Option<(String, String)>,
}

impl GraphCanvasPanel {
    /// Start dragging a connection from a pin
    pub fn start_connection_drag_from_pin(
        &mut self,
        node_id: String,
        pin_id: String,
        mouse_pos: Point<f32>,
        cx: &mut Context<Self>,
    ) {
        if let Some(node) = self.graph.nodes.iter().find(|n| n.id == node_id) {
            if let Some(pin) = node.outputs.iter().find(|p| p.id == pin_id) {
                tracing::info!(
                    "Starting connection drag from pin {} on node {}",
                    pin_id,
                    node_id
                );
                self.dragging_connection = Some(ConnectionDrag {
                    source_node: node_id,
                    source_pin: pin_id,
                    source_pin_type: pin.data_type.clone(),
                    current_mouse_pos: mouse_pos,
                    target_pin: None,
                });

                // Close tooltips when starting connection drag
                // Tooltip removed - use node picker instead
                cx.notify();
            }
        }
    }

    /// Update connection drag position
    pub fn update_connection_drag(&mut self, mouse_pos: Point<f32>, cx: &mut Context<Self>) {
        if let Some(ref mut drag) = self.dragging_connection {
            drag.current_mouse_pos = mouse_pos;
            cx.notify();
        }
    }

    /// Cancel connection drag
    pub fn cancel_connection_drag(&mut self, cx: &mut Context<Self>) {
        self.dragging_connection = None;
        cx.notify();
    }

    /// Set connection target (hovering over a pin)
    pub fn set_connection_target(
        &mut self,
        target_node_id: Option<String>,
        target_pin_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(ref mut drag) = self.dragging_connection {
            drag.target_pin = target_node_id.zip(target_pin_id);
            cx.notify();
        }
    }

    /// Complete connection on a pin
    pub fn complete_connection_on_pin(
        &mut self,
        node_id: String,
        pin_id: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(drag) = self.dragging_connection.take() {
            // Validate connection
            if let Some(node) = self.graph.nodes.iter().find(|n| n.id == node_id) {
                if let Some(pin) = node.inputs.iter().find(|p| p.id == pin_id) {
                    // Clone pin data type before mutable operations
                    let pin_data_type = pin.data_type.clone();

                    // Check compatibility and not same node
                    if super::compatibility::are_types_compatible(
                        &drag.source_pin_type,
                        &pin_data_type,
                    ) && drag.source_node != node_id
                    {
                        // Check if source or target is a reroute node
                        let source_is_reroute =
                            self.graph.nodes.iter().any(|n| {
                                n.id == drag.source_node && n.node_type == NodeType::Reroute
                            });
                        let target_is_reroute = self
                            .graph
                            .nodes
                            .iter()
                            .any(|n| n.id == node_id && n.node_type == NodeType::Reroute);

                        // Remove old connections based on pin types
                        if drag.source_pin_type == GraphDataType::Execution || source_is_reroute {
                            // Execution pins and reroute outputs: single connection from source
                            tracing::info!(
                                "Removing old connection from source {}:{}",
                                drag.source_node,
                                drag.source_pin
                            );
                            self.graph.connections.retain(|conn| {
                                !(conn.source_node == drag.source_node
                                    && conn.source_pin == drag.source_pin)
                            });
                        }

                        if drag.source_pin_type == GraphDataType::Execution
                            || target_is_reroute
                            || pin_data_type != GraphDataType::Execution
                        {
                            // Execution targets, reroute inputs, or data inputs: single connection to target
                            tracing::info!(
                                "Removing old connection to target {}:{}",
                                node_id,
                                pin_id
                            );
                            self.graph.connections.retain(|conn| {
                                !(conn.target_node == node_id && conn.target_pin == pin_id)
                            });
                        }

                        println!(
                            "Creating connection from {}:{} to {}:{}",
                            drag.source_node, drag.source_pin, node_id, pin_id
                        );
                        tracing::info!(
                            "Creating connection from {}:{} to {}:{}",
                            drag.source_node,
                            drag.source_pin,
                            node_id,
                            pin_id
                        );

                        // Create new connection
                        let connection_type = if pin_data_type == GraphDataType::Execution {
                            ui::graph::ConnectionType::Execution
                        } else {
                            ui::graph::ConnectionType::Data
                        };

                        let connection = Connection {
                            id: uuid::Uuid::new_v4().to_string(),
                            source_node: drag.source_node.clone(),
                            source_pin: drag.source_pin.clone(),
                            target_node: node_id.clone(),
                            target_pin: pin_id.clone(),
                            connection_type,
                        };

                        // Create and execute undo command
                        let mut cmd =
                            crate::features::undo::AddConnectionCommand::new(connection.clone());
                        cmd.execute(self, cx);
                        self.push_undo_command(crate::features::undo::Command::AddConnection(cmd));

                        tracing::info!("Connection created successfully!");

                        // Propagate types through reroute nodes
                        if target_is_reroute {
                            self.propagate_reroute_types(node_id.clone(), drag.source_pin_type, cx);
                        } else if source_is_reroute {
                            self.propagate_reroute_types(
                                drag.source_node.clone(),
                                pin_data_type,
                                cx,
                            );
                        }

                        cx.notify();
                    } else {
                        tracing::info!("Incompatible pin types or same node");
                    }
                }
            }
        }
    }

    /// Disconnect a pin
    pub fn disconnect_pin(&mut self, node_id: String, pin_id: String, cx: &mut Context<Self>) {
        // Collect connections to delete
        let connections_to_delete: Vec<_> = self
            .graph
            .connections
            .iter()
            .filter(|conn| {
                (conn.source_node == node_id && conn.source_pin == pin_id)
                    || (conn.target_node == node_id && conn.target_pin == pin_id)
            })
            .cloned()
            .collect();

        if !connections_to_delete.is_empty() {
            // Create batch command if multiple connections
            if connections_to_delete.len() == 1 {
                let mut cmd = crate::features::undo::DeleteConnectionCommand::new(
                    connections_to_delete[0].clone(),
                );
                cmd.execute(self, cx);
                self.push_undo_command(crate::features::undo::Command::DeleteConnection(cmd));
            } else {
                let mut batch =
                    crate::features::undo::BatchCommand::new("Disconnect pin".to_string());
                for connection in connections_to_delete {
                    batch.add_command(crate::features::undo::Command::DeleteConnection(
                        crate::features::undo::DeleteConnectionCommand::new(connection),
                    ));
                }
                batch.execute(self, cx);
                self.push_undo_command(crate::features::undo::Command::Batch(batch));
            }
        }
    }

    /// Propagate types through connected reroute nodes
    pub fn propagate_reroute_types(
        &mut self,
        start_node_id: String,
        data_type: GraphDataType,
        cx: &mut Context<Self>,
    ) {
        use std::collections::{HashSet, VecDeque};

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start_node_id);

        while let Some(node_id) = queue.pop_front() {
            if visited.contains(&node_id) {
                continue;
            }
            visited.insert(node_id.clone());

            if let Some(node) = self.graph.nodes.iter_mut().find(|n| n.id == node_id) {
                if node.node_type == NodeType::Reroute {
                    // Update pin types
                    for pin in &mut node.inputs {
                        pin.data_type = data_type.clone();
                    }
                    for pin in &mut node.outputs {
                        pin.data_type = data_type.clone();
                    }

                    // Find connected reroute nodes
                    for connection in &self.graph.connections {
                        if connection.source_node == node_id {
                            if let Some(target_node) = self
                                .graph
                                .nodes
                                .iter()
                                .find(|n| n.id == connection.target_node)
                            {
                                if target_node.node_type == NodeType::Reroute {
                                    queue.push_back(connection.target_node.clone());
                                }
                            }
                        } else if connection.target_node == node_id {
                            if let Some(source_node) = self
                                .graph
                                .nodes
                                .iter()
                                .find(|n| n.id == connection.source_node)
                            {
                                if source_node.node_type == NodeType::Reroute {
                                    queue.push_back(connection.source_node.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        cx.notify();
    }

    /// Get data type of a connection
    pub fn get_connection_data_type(&self, connection: &Connection) -> Option<GraphDataType> {
        let from_node = self
            .graph
            .nodes
            .iter()
            .find(|n| n.id == connection.source_node)?;
        let output_pin = from_node
            .outputs
            .iter()
            .find(|p| p.id == connection.source_pin)?;
        Some(output_pin.data_type.clone())
    }

    fn pin_graph_position(
        &self,
        node: &BlueprintNode,
        pin_id: &str,
        is_input: bool,
    ) -> Option<Point<f32>> {
        let row = if is_input {
            node.inputs.iter().position(|p| p.id == pin_id)
        } else {
            node.outputs.iter().position(|p| p.id == pin_id)
        };

        row.map(|row| {
            crate::rendering::graph::NodeGraphRenderer::calculate_pin_position_graph_space(
                node,
                is_input,
                row,
                &self.graph,
            )
        })
    }

    fn connection_endpoints(&self, connection: &Connection) -> Option<(Point<f32>, Point<f32>)> {
        let from_node = self.graph.nodes.iter().find(|n| n.id == connection.source_node)?;
        let to_node = self.graph.nodes.iter().find(|n| n.id == connection.target_node)?;

        let from_pos = self.pin_graph_position(from_node, &connection.source_pin, false)
            .unwrap_or_else(|| {
                Point::new(
                    from_node.position.x + from_node.size.width,
                    from_node.position.y + from_node.size.height / 2.0,
                )
            });

        let to_pos = self.pin_graph_position(to_node, &connection.target_pin, true)
            .unwrap_or_else(|| {
                Point::new(
                    to_node.position.x,
                    to_node.position.y + to_node.size.height / 2.0,
                )
            });

        Some((from_pos, to_pos))
    }

    fn bezier_control_points(
        from_pos: Point<f32>,
        to_pos: Point<f32>,
    ) -> (Point<f32>, Point<f32>) {
        const CONTROL_POINT_DISTANCE_RATIO: f32 = 0.45;
        const MIN_CONTROL_POINT_OFFSET: f32 = 55.0;
        const MAX_CONTROL_POINT_OFFSET: f32 = 220.0;

        let horizontal_distance = (to_pos.x - from_pos.x).abs();
        let control_point_offset =
            (horizontal_distance * CONTROL_POINT_DISTANCE_RATIO)
                .clamp(MIN_CONTROL_POINT_OFFSET, MAX_CONTROL_POINT_OFFSET);

        (
            Point::new(from_pos.x + control_point_offset, from_pos.y),
            Point::new(to_pos.x - control_point_offset, to_pos.y),
        )
    }

    fn distance_between_points(a: Point<f32>, b: Point<f32>) -> f32 {
        ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
    }

    fn closest_point_on_segment(
        point: Point<f32>,
        start: (f32, f32),
        end: (f32, f32),
    ) -> (f32, f32) {
        let dx = end.0 - start.0;
        let dy = end.1 - start.1;
        let segment_length_sq = dx * dx + dy * dy;

        if segment_length_sq == 0.0 {
            return start;
        }

        let unbounded_projection =
            ((point.x - start.0) * dx + (point.y - start.1) * dy) / segment_length_sq;
        let projection = unbounded_projection.clamp(0.0, 1.0);

        (
            start.0 + projection * dx,
            start.1 + projection * dy,
        )
    }

    fn point_distance_to_segment(
        point: Point<f32>,
        start: (f32, f32),
        end: (f32, f32),
    ) -> f32 {
        let (closest_x, closest_y) = Self::closest_point_on_segment(point, start, end);
        Self::distance_between_points(point, Point::new(closest_x, closest_y))
    }

    fn is_point_near_bezier_curve(
        &self,
        point: Point<f32>,
        from_pos: Point<f32>,
        to_pos: Point<f32>,
        samples: usize,
        threshold: f32,
    ) -> bool {
        let (c1, c2) = Self::bezier_control_points(from_pos, to_pos);
        let mut prev = (from_pos.x, from_pos.y);

        for i in 1..=samples {
            let t = i as f32 / samples as f32;
            let cur = crate::rendering::graph::bezier(
                (from_pos.x, from_pos.y),
                (c1.x, c1.y),
                (c2.x, c2.y),
                (to_pos.x, to_pos.y),
                t,
            );

            if Self::point_distance_to_segment(point, prev, cur) <= threshold {
                return true;
            }

            prev = cur;
        }

        false
    }

    /// Find connection near a point (for double-click reroute creation)
    pub fn find_connection_near_point(&self, point: Point<f32>) -> Option<Connection> {
        const CLICK_THRESHOLD: f32 = 30.0;

        for connection in &self.graph.connections {
            if let Some((from_pos, to_pos)) = self.connection_endpoints(connection) {
                if Self::point_near_bezier(point, from_pos, to_pos, CLICK_THRESHOLD) {
                    return Some(connection.clone());
                }
            }
        }

        None
    }

    /// Precise connection hit test using bezier sampling. Returns the first
    /// connection whose bezier curve is within `THRESHOLD` graph units of
    /// `point`. This is intended for hover interaction (smaller threshold
    /// and more accurate than the coarse test used for double-click).
    pub fn find_connection_near_point_precise(&self, point: Point<f32>) -> Option<Connection> {
        const SAMPLES: usize = 48;
        const THRESHOLD: f32 = 12.0;

        for connection in &self.graph.connections {
            if let Some((from_pos, to_pos)) = self.connection_endpoints(connection) {
                if self.is_point_near_bezier_curve(point, from_pos, to_pos, SAMPLES, THRESHOLD)
                {
                    return Some(connection.clone());
                }
            }
        }

        None
    }

    /// Check if point is near a bezier curve (simplified linear approximation)
    fn point_near_bezier(
        point: Point<f32>,
        start: Point<f32>,
        end: Point<f32>,
        threshold: f32,
    ) -> bool {
        // Simplified: check distance to line segment
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length_sq = dx * dx + dy * dy;

        if length_sq == 0.0 {
            let dist = ((point.x - start.x).powi(2) + (point.y - start.y).powi(2)).sqrt();
            return dist <= threshold;
        }

        let t = ((point.x - start.x) * dx + (point.y - start.y) * dy) / length_sq;
        let t = t.clamp(0.0, 1.0);

        let closest_x = start.x + t * dx;
        let closest_y = start.y + t * dy;

        let dist = ((point.x - closest_x).powi(2) + (point.y - closest_y).powi(2)).sqrt();
        dist <= threshold
    }
}
