//! Unified operations for graph entities (nodes and comments)

use crate::core::graph_entity::EntitySelection;
use crate::core::types::NodeType;
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::rendering::graph::NodeGraphRenderer;
use gpui::*;
use std::collections::HashSet;

impl GraphCanvasPanel {
    /// Get all currently selected entities as a unified list
    pub fn get_selected_entities(&self) -> Vec<EntitySelection> {
        let mut selections: Vec<EntitySelection> = Vec::new();

        for node_id in &self.graph.selected_nodes {
            selections.push(EntitySelection::Node(node_id.as_str().to_owned()));
        }

        for comment_id in &self.graph.selected_comments {
            selections.push(EntitySelection::Comment(comment_id.as_str().to_owned()));
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

            let mut comment_ids: HashSet<String> = HashSet::new();
            let mut node_ids: HashSet<String> = HashSet::new();
            for selection in selections {
                match selection {
                    EntitySelection::Node(ref id) => {
                        node_ids.insert(id.clone());
                    }
                    EntitySelection::Comment(ref id) => {
                        self.collect_comment_drag_group(id, &mut comment_ids, &mut node_ids);
                    }
                }
            }

            for id in node_ids {
                if let Some(node) = self.graph.nodes.iter().find(|n| n.id.as_str() == id.as_str()) {
                    self.initial_drag_positions.insert(id, node.position);
                }
            }
            for id in comment_ids {
                if let Some(comment) = self.graph.comments.iter().find(|c| c.id.as_str() == id.as_str()) {
                    self.initial_comment_drag_positions.insert(id, comment.position);
                }
            }
        } else {
            // Single drag
            match &dragged_entity {
                EntitySelection::Node(id) => {
                    if let Some(node) = self.graph.nodes.iter().find(|n| n.id.as_str() == id.as_str()) {
                        self.initial_drag_positions
                            .insert(id.clone(), node.position);
                    }
                }
                EntitySelection::Comment(id) => {
                    self.start_comment_drag(id.clone(), mouse_pos, _cx);
                    return;
                }
            }
        }

        // Set drag offset
        match &dragged_entity {
            EntitySelection::Node(id) => {
                if let Some(node) = self.graph.nodes.iter().find(|n| n.id.as_str() == id.as_str()) {
                    let pos = node.position;
                    self.drag_offset = Point::new(mouse_pos.x - pos.x, mouse_pos.y - pos.y);
                    self.dragging_node = Some(id.clone());
                }
            }
            EntitySelection::Comment(id) => {
                if let Some(comment) = self.graph.comments.iter().find(|c| c.id.as_str() == id.as_str()) {
                    let pos = comment.position;
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
                    EntitySelection::Node(_) => NodeGraphRenderer::snap_to_grid(raw_position),
                    EntitySelection::Comment(_) => self.snap_comment_position(raw_position),
                };

                let delta =
                    Point::new(snapped_pos.x - initial_pos.x, snapped_pos.y - initial_pos.y);

                // Move all nodes in the selection
                for (node_id, initial_position) in &self.initial_drag_positions.clone() {
                    if let Some(node) = self.graph.nodes.iter_mut().find(|n| n.id.as_str() == node_id) {
                        let new_pos =
                            Point::new(initial_position.x + delta.x, initial_position.y + delta.y);
                        node.position = NodeGraphRenderer::snap_to_grid(new_pos);
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
                        .find(|c| c.id.as_str() == comment_id.as_str())
                    {
                        comment.position = snapped_pos;
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
            let nodes = self.graph.nodes.clone();
            comment.update_contained_nodes(&nodes);
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

        let selected_node_ids: HashSet<&str> = self
            .graph
            .selected_nodes
            .iter()
            .map(|id: &String| id.as_str())
            .collect();
        let selected_comment_ids: HashSet<&str> = self
            .graph
            .selected_comments
            .iter()
            .map(|id: &String| id.as_str())
            .collect();

        let deleted_nodes: Vec<_> = self
            .graph
            .nodes
            .iter()
            .filter(|node| {
                selected_node_ids.contains(node.id.as_str())
                    && !matches!(node.node_type, NodeType::MacroEntry | NodeType::MacroExit)
            })
            .cloned()
            .collect();
        let deleted_comments: Vec<_> = self
            .graph
            .comments
            .iter()
            .filter(|comment| selected_comment_ids.contains(comment.id.as_str()))
            .cloned()
            .collect();
        let deleted_connections: Vec<_> = self
            .graph
            .connections
            .iter()
            .filter(|connection| {
                selected_node_ids.contains(connection.source_node.as_str())
                    || selected_node_ids.contains(connection.target_node.as_str())
            })
            .cloned()
            .collect();

        let mut command = crate::features::undo::DeleteEntitiesCommand::new(
            deleted_nodes,
            deleted_comments,
            deleted_connections,
        );
        command.execute(self, cx);
        self.push_undo_command(crate::features::undo::Command::DeleteEntities(command));
    }
}
