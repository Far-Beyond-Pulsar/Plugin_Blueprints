//! Unified operations for graph entities (nodes and comments)

use crate::core::graph_entity::{DragState, EntitySelection, GraphEntity};
use crate::core::types::{BlueprintComment, BlueprintNode};
use crate::editor::panel::BlueprintEditorPanel;
use crate::rendering::graph::NodeGraphRenderer;
use gpui::*;

impl BlueprintEditorPanel {
    /// Get all currently selected entities as a unified list
    pub fn get_selected_entities(&self) -> Vec<EntitySelection> {
        let mut selections = Vec::new();

        for node_id in &self.graph.selected_nodes {
            selections.push(EntitySelection::Node(node_id.clone()));
        }

        for comment_id in &self.graph.selected_comments {
            selections.push(EntitySelection::Comment(comment_id.clone()));
        }

        selections
    }

    /// Start dragging any entity (unified for nodes and comments)
    pub fn start_entity_drag(
        &mut self,
        dragged_entity: EntitySelection,
        mouse_pos: Point<f32>,
        _cx: &mut Context<Self>,
    ) {
        println!(
            "[DRAG] Starting drag for {:?} at {:?}",
            dragged_entity, mouse_pos
        );

        // Clear previous drag state
        self.initial_drag_positions.clear();
        self.initial_comment_drag_positions.clear();

        // Check if the dragged entity is selected
        let is_selected = match &dragged_entity {
            EntitySelection::Node(id) => self.graph.selected_nodes.contains(id),
            EntitySelection::Comment(id) => self.graph.selected_comments.contains(id),
        };

        if is_selected {
            // Multi-select drag: store all selected entities
            let selections = self.get_selected_entities();
            println!(
                "[DRAG] Multi-select: dragging {} entities",
                selections.len()
            );

            for selection in selections {
                match selection {
                    EntitySelection::Node(ref id) => {
                        if let Some(node) = self.graph.nodes.iter().find(|n| n.id() == id) {
                            self.initial_drag_positions
                                .insert(id.clone(), node.position());
                        }
                    }
                    EntitySelection::Comment(ref id) => {
                        if let Some(comment) = self.graph.comments.iter().find(|c| c.id() == id) {
                            self.initial_comment_drag_positions
                                .insert(id.clone(), comment.position());
                        }
                    }
                }
            }
        } else {
            // Single drag
            match &dragged_entity {
                EntitySelection::Node(id) => {
                    if let Some(node) = self.graph.nodes.iter().find(|n| n.id() == id) {
                        self.initial_drag_positions
                            .insert(id.clone(), node.position());
                    }
                }
                EntitySelection::Comment(id) => {
                    if let Some(comment) = self.graph.comments.iter().find(|c| c.id() == id) {
                        self.initial_comment_drag_positions
                            .insert(id.clone(), comment.position());
                    }
                }
            }
        }

        // Set drag offset
        match &dragged_entity {
            EntitySelection::Node(id) => {
                if let Some(node) = self.graph.nodes.iter().find(|n| n.id() == id) {
                    let pos = node.position();
                    self.drag_offset = Point::new(mouse_pos.x - pos.x, mouse_pos.y - pos.y);
                    self.dragging_node = Some(id.clone());
                }
            }
            EntitySelection::Comment(id) => {
                if let Some(comment) = self.graph.comments.iter().find(|c| c.id() == id) {
                    let pos = comment.position();
                    self.drag_offset = Point::new(mouse_pos.x - pos.x, mouse_pos.y - pos.y);
                    self.dragging_comment = Some(id.clone());
                }
            }
        }
    }

    /// Update entity drag (unified for all entity types)
    pub fn update_entity_drag(&mut self, mouse_pos: Point<f32>, cx: &mut Context<Self>) {
        // Calculate new position
        let raw_position = Point::new(
            mouse_pos.x - self.drag_offset.x,
            mouse_pos.y - self.drag_offset.y,
        );

        // Determine which entity is being dragged
        let dragged_id = if let Some(node_id) = &self.dragging_node {
            Some(EntitySelection::Node(node_id.clone()))
        } else if let Some(comment_id) = &self.dragging_comment {
            Some(EntitySelection::Comment(comment_id.clone()))
        } else {
            None
        };

        if let Some(dragged) = dragged_id {
            // Get initial position of dragged entity
            let initial_pos = match &dragged {
                EntitySelection::Node(id) => self.initial_drag_positions.get(id).copied(),
                EntitySelection::Comment(id) => {
                    self.initial_comment_drag_positions.get(id).copied()
                }
            };

            if let Some(initial_pos) = initial_pos {
                // Calculate delta based on dragged entity type
                let snapped_pos = match &dragged {
                    EntitySelection::Node(_) => {
                        NodeGraphRenderer::snap_to_grid(raw_position, self.graph.zoom_level)
                    }
                    EntitySelection::Comment(_) => self.snap_comment_position(raw_position),
                };

                let delta =
                    Point::new(snapped_pos.x - initial_pos.x, snapped_pos.y - initial_pos.y);

                // Move all nodes in the selection
                for (node_id, initial_position) in &self.initial_drag_positions.clone() {
                    if let Some(node) = self.graph.nodes.iter_mut().find(|n| n.id() == node_id) {
                        let new_pos =
                            Point::new(initial_position.x + delta.x, initial_position.y + delta.y);
                        node.set_position(NodeGraphRenderer::snap_to_grid(
                            new_pos,
                            self.graph.zoom_level,
                        ));
                    }
                }

                // Move all comments in the selection
                for (comment_id, initial_position) in &self.initial_comment_drag_positions.clone() {
                    let new_pos =
                        Point::new(initial_position.x + delta.x, initial_position.y + delta.y);
                    let snapped_pos = self.snap_comment_position(new_pos);

                    if let Some(comment) = self
                        .graph
                        .comments
                        .iter_mut()
                        .find(|c| c.id() == comment_id)
                    {
                        comment.set_position(snapped_pos);
                    }
                }

                cx.notify();
            }
        }
    }

    /// End entity drag and create undo command (unified for all drags)
    pub fn end_entity_drag(&mut self, cx: &mut Context<Self>) {
        // Create move command for undo/redo
        if !self.initial_drag_positions.is_empty()
            || !self.initial_comment_drag_positions.is_empty()
        {
            let mut node_moves = Vec::new();
            let mut comment_moves = Vec::new();

            // Collect node moves
            for (node_id, old_pos) in &self.initial_drag_positions {
                if let Some(node) = self.graph.nodes.iter().find(|n| &n.id == node_id) {
                    if node.position != *old_pos {
                        node_moves.push((node_id.clone(), *old_pos, node.position));
                    }
                }
            }

            // Collect comment moves
            for (comment_id, old_pos) in &self.initial_comment_drag_positions {
                if let Some(comment) = self.graph.comments.iter().find(|c| &c.id == comment_id) {
                    if comment.position != *old_pos {
                        comment_moves.push((comment_id.clone(), *old_pos, comment.position));
                    }
                }
            }

            // Only push command if something actually moved
            if !node_moves.is_empty() || !comment_moves.is_empty() {
                println!(
                    "[UNDO] Creating move command: {} nodes, {} comments",
                    node_moves.len(),
                    comment_moves.len()
                );
                // Use new_executed because the move already happened during the drag
                let cmd = crate::features::undo::MoveEntitiesCommand::new_executed(
                    node_moves,
                    comment_moves,
                );
                self.push_undo_command(crate::features::undo::Command::MoveEntities(cmd));
            }
        }

        // Update comment containment after drag
        for comment in self.graph.comments.iter_mut() {
            comment.update_contained_nodes(&self.graph.nodes);
        }

        // Clear drag state
        self.dragging_node = None;
        self.dragging_comment = None;
        cx.notify();
    }

    /// Delete all selected entities (unified)
    pub fn delete_selected_entities(&mut self, cx: &mut Context<Self>) {
        let node_count = self.graph.selected_nodes.len();
        let comment_count = self.graph.selected_comments.len();

        if node_count == 0 && comment_count == 0 {
            println!("[DELETE] No entities selected");
            return;
        }

        println!(
            "[DELETE] Deleting {} nodes, {} comments",
            node_count, comment_count
        );

        // Create a batch command for multiple deletions
        let mut batch = crate::features::undo::BatchCommand::new(format!(
            "Delete {} entities",
            node_count + comment_count
        ));

        // Add delete commands for each selected node
        for node_id in &self.graph.selected_nodes.clone() {
            if let Some(node) = self.graph.nodes.iter().find(|n| &n.id == node_id).cloned() {
                let connections: Vec<_> = self
                    .graph
                    .connections
                    .iter()
                    .filter(|c| &c.source_node == node_id || &c.target_node == node_id)
                    .cloned()
                    .collect();

                batch.add_command(crate::features::undo::Command::DeleteNode(
                    crate::features::undo::DeleteNodeCommand::new(node, connections),
                ));
            }
        }

        // Add delete commands for each selected comment
        for comment_id in &self.graph.selected_comments.clone() {
            if let Some(comment) = self
                .graph
                .comments
                .iter()
                .find(|c| &c.id == comment_id)
                .cloned()
            {
                batch.add_command(crate::features::undo::Command::DeleteComment(
                    crate::features::undo::DeleteCommentCommand::new(comment),
                ));
            }
        }

        // Execute the batch command
        batch.execute(self, cx);
        self.push_undo_command(crate::features::undo::Command::Batch(batch));

        // Clear selection
        self.graph.selected_nodes.clear();
        self.graph.selected_comments.clear();
    }
}
