//! Node operations - create, delete, duplicate, copy/paste
//!
//! All operations related to node manipulation in the graph.

use crate::core::types::{BlueprintNode, Connection, NodeType};
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::rendering::graph::NodeGraphRenderer;
use gpui::*;
use crate::core::types::PinDataType as GraphDataType;

impl GraphCanvasPanel {
    /// Add a node to the graph
    pub fn add_node(&mut self, mut node: BlueprintNode, cx: &mut Context<Self>) {
        node.position = NodeGraphRenderer::snap_to_grid(node.position);
        println!(
            "Adding node: {} at position {:?}",
            node.title, node.position
        );

        // Create and execute undo command
        let mut cmd = crate::features::undo::AddNodeCommand::new(node.clone());
        cmd.execute(self, cx);
        self.push_undo_command(crate::features::undo::Command::AddNode(cmd));

        // Mark tab as dirty
        self.is_dirty = true;
    }

    /// Connect a newly added node to the active connection drag source.
    pub fn complete_connection_to_new_node(
        &mut self,
        source: crate::features::connections::operations::ConnectionDrag,
        new_node: &BlueprintNode,
        cx: &mut Context<Self>,
    ) {
        if let Some(target_pin) = new_node.inputs.iter().find(|pin| {
            crate::features::connections::compatibility::are_types_compatible(
                &source.source_pin_type,
                &pin.data_type,
            )
        }) {
            let pin_data_type = target_pin.data_type.clone();
            let target_pin_id = target_pin.id.clone();

            let source_is_reroute = self
                .graph
                .nodes
                .iter()
                .any(|n| n.id == source.source_node && n.node_type == NodeType::Reroute);
            let target_is_reroute = self
                .graph
                .nodes
                .iter()
                .any(|n| n.id == new_node.id && n.node_type == NodeType::Reroute);

            if source.source_pin_type == GraphDataType::execution() || source_is_reroute {
                self.graph.connections.retain(|conn| {
                    !(conn.source_node == source.source_node
                        && conn.source_pin == source.source_pin)
                });
            }

            if source.source_pin_type == GraphDataType::execution()
                || target_is_reroute
                || pin_data_type != GraphDataType::execution()
            {
                self.graph.connections.retain(|conn| {
                    !(conn.target_node == new_node.id && conn.target_pin == target_pin_id)
                });
            }

            println!(
                "Creating auto-connection from {}:{} to {}:{}",
                source.source_node, source.source_pin, new_node.id, target_pin_id
            );

            let connection_type = if pin_data_type == GraphDataType::execution() {
                ui::graph::ConnectionType::Execution
            } else {
                ui::graph::ConnectionType::Data
            };

            let connection = Connection {
                id: uuid::Uuid::new_v4().to_string(),
                source_node: source.source_node.clone(),
                source_pin: source.source_pin.clone(),
                target_node: new_node.id.clone(),
                target_pin: target_pin_id.clone(),
                connection_type,
            };

            let mut cmd = crate::features::undo::AddConnectionCommand::new(connection.clone());
            cmd.execute(self, cx);
            self.push_undo_command(crate::features::undo::Command::AddConnection(cmd));

            if target_is_reroute {
                self.propagate_reroute_types(new_node.id.clone(), source.source_pin_type, cx);
            } else if source_is_reroute {
                self.propagate_reroute_types(source.source_node.clone(), pin_data_type, cx);
            }

            cx.notify();
        }
    }

    /// Duplicate a node
    pub fn duplicate_node(&mut self, node_id: String, cx: &mut Context<Self>) {
        if let Some(node) = self.graph.nodes.iter().find(|n| n.id == node_id).cloned() {
            let mut new_node = node;
            new_node.id = uuid::Uuid::new_v4().to_string();
            new_node.position.x += 20.0;
            new_node.position.y += 20.0;
            new_node.is_selected = false;
            self.add_node(new_node, cx);
        }
    }

    /// Delete a node and its connections
    pub fn delete_node(&mut self, node_id: String, cx: &mut Context<Self>) {
        // MacroEntry and MacroExit nodes are structural — never allow deletion.
        if let Some(node) = self.graph.nodes.iter().find(|n| n.id == node_id) {
            if matches!(node.node_type, NodeType::MacroEntry | NodeType::MacroExit) {
                return;
            }
        }

        // Find the node and its connections before deleting
        if let Some(node) = self.graph.nodes.iter().find(|n| n.id == node_id).cloned() {
            let connections: Vec<_> = self
                .graph
                .connections
                .iter()
                .filter(|c| c.source_node == node_id || c.target_node == node_id)
                .cloned()
                .collect();

            // Create and execute undo command
            let mut cmd = crate::features::undo::DeleteNodeCommand::new(node, connections);
            cmd.execute(self, cx);
            self.push_undo_command(crate::features::undo::Command::DeleteNode(cmd));
        }
    }

    /// Copy node into the in-memory clipboard
    pub fn copy_node(&mut self, node_id: String, _cx: &mut Context<Self>) {
        if let Some(node) = self.graph.nodes.iter().find(|n| n.id == node_id).cloned() {
            println!("Copied node: {}", node.title);
            self.node_clipboard = Some(node);
        } else {
            println!("Copy failed: node not found ({})", node_id);
        }
    }

    /// Paste node from the in-memory clipboard
    pub fn paste_node(&mut self, cx: &mut Context<Self>) {
        if let Some(mut node) = self.node_clipboard.clone() {
            node.id = uuid::Uuid::new_v4().to_string();
            node.position.x += 30.0;
            node.position.y += 30.0;
            node.is_selected = false;
            self.add_node(node, cx);
            println!("Pasted node from clipboard");
        } else {
            println!("Paste requested with empty clipboard");
        }
    }

    /// Start dragging a node
    pub fn start_drag(&mut self, node_id: String, mouse_pos: Point<f32>, cx: &mut Context<Self>) {
        println!(
            "[DRAG] Starting drag for node {} at mouse position {:?}",
            node_id, mouse_pos
        );

        if let Some(node) = self.graph.nodes.iter().find(|n| n.id == node_id) {
            self.dragging_node = Some(node_id.clone());
            self.drag_offset =
                Point::new(mouse_pos.x - node.position.x, mouse_pos.y - node.position.y);

            // Store initial positions for multi-select drag
            self.initial_drag_positions.clear();
            self.initial_comment_drag_positions.clear();

            if self.graph.selected_nodes.contains(&node_id) {
                // Drag all selected nodes
                println!(
                    "[DRAG] Multi-select: dragging {} selected nodes",
                    self.graph.selected_nodes.len()
                );
                for selected_id in &self.graph.selected_nodes {
                    if let Some(selected_node) =
                        self.graph.nodes.iter().find(|n| n.id == *selected_id)
                    {
                        self.initial_drag_positions
                            .insert(selected_id.clone(), selected_node.position);
                    }
                }

                // Also drag all selected comments
                println!(
                    "[DRAG] Multi-select: also dragging {} selected comments",
                    self.graph.selected_comments.len()
                );
                for comment_id in &self.graph.selected_comments {
                    if let Some(comment) = self.graph.comments.iter().find(|c| c.id == *comment_id)
                    {
                        self.initial_comment_drag_positions
                            .insert(comment_id.clone(), comment.position);
                    }
                }
            } else {
                // Drag only this node
                self.initial_drag_positions
                    .insert(node_id.clone(), node.position);
            }

            cx.notify();
        }
    }

    /// Update drag position
    pub fn update_drag(&mut self, mouse_pos: Point<f32>, cx: &mut Context<Self>) {
        if let Some(dragging_id) = &self.dragging_node.clone() {
            let raw_position = Point::new(
                mouse_pos.x - self.drag_offset.x,
                mouse_pos.y - self.drag_offset.y,
            );
            let new_position = NodeGraphRenderer::snap_to_grid(raw_position);

            if let Some(initial_pos) = self.initial_drag_positions.get(dragging_id) {
                let delta = Point::new(
                    new_position.x - initial_pos.x,
                    new_position.y - initial_pos.y,
                );

                // Move all nodes that were selected when dragging started
                for (node_id, initial_position) in &self.initial_drag_positions {
                    if let Some(node) = self.graph.nodes.iter_mut().find(|n| n.id == *node_id) {
                        node.position = NodeGraphRenderer::snap_to_grid(Point::new(
                            initial_position.x + delta.x,
                            initial_position.y + delta.y,
                        ));
                    }
                }

                // Move all comments that were selected when dragging started
                for (comment_id, initial_position) in &self.initial_comment_drag_positions.clone() {
                    if let Some(comment) =
                        self.graph.comments.iter_mut().find(|c| c.id == *comment_id)
                    {
                        comment.position = NodeGraphRenderer::snap_to_grid(Point::new(
                            initial_position.x + delta.x,
                            initial_position.y + delta.y,
                        ));
                    }
                }
            }

            cx.notify();
        }
    }

    /// End drag operation
    pub fn end_drag(&mut self, cx: &mut Context<Self>) {
        // Use unified entity drag end
        self.end_entity_drag(cx);
    }
}
