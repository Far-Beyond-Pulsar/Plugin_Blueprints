//! Undo/redo system for graph operations
//!
//! Implements a command pattern for reversible operations on the graph.

use crate::core::types::{BlueprintComment, BlueprintNode, Connection};
use gpui::*;
use std::collections::HashSet;

/// A command that can be executed and undone
#[derive(Debug, Clone)]
pub enum Command {
    AddNode(AddNodeCommand),
    DeleteNode(DeleteNodeCommand),
    AddComment(AddCommentCommand),
    DeleteComment(DeleteCommentCommand),
    DeleteEntities(DeleteEntitiesCommand),
    MoveEntities(MoveEntitiesCommand),
    AddConnection(AddConnectionCommand),
    DeleteConnection(DeleteConnectionCommand),
    Batch(BatchCommand),
}

impl Command {
    /// Execute the command
    pub fn execute(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        match self {
            Command::AddNode(cmd) => cmd.execute(panel, cx),
            Command::DeleteNode(cmd) => cmd.execute(panel, cx),
            Command::AddComment(cmd) => cmd.execute(panel, cx),
            Command::DeleteComment(cmd) => cmd.execute(panel, cx),
            Command::DeleteEntities(cmd) => cmd.execute(panel, cx),
            Command::MoveEntities(cmd) => cmd.execute(panel, cx),
            Command::AddConnection(cmd) => cmd.execute(panel, cx),
            Command::DeleteConnection(cmd) => cmd.execute(panel, cx),
            Command::Batch(cmd) => cmd.execute(panel, cx),
        }
    }

    /// Undo the command
    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        match self {
            Command::AddNode(cmd) => cmd.undo(panel, cx),
            Command::DeleteNode(cmd) => cmd.undo(panel, cx),
            Command::AddComment(cmd) => cmd.undo(panel, cx),
            Command::DeleteComment(cmd) => cmd.undo(panel, cx),
            Command::DeleteEntities(cmd) => cmd.undo(panel, cx),
            Command::MoveEntities(cmd) => cmd.undo(panel, cx),
            Command::AddConnection(cmd) => cmd.undo(panel, cx),
            Command::DeleteConnection(cmd) => cmd.undo(panel, cx),
            Command::Batch(cmd) => cmd.undo(panel, cx),
        }
    }

    /// Get a description of this command
    pub fn description(&self) -> String {
        match self {
            Command::AddNode(cmd) => cmd.description(),
            Command::DeleteNode(cmd) => cmd.description(),
            Command::AddComment(cmd) => cmd.description(),
            Command::DeleteComment(cmd) => cmd.description(),
            Command::DeleteEntities(cmd) => cmd.description(),
            Command::MoveEntities(cmd) => cmd.description(),
            Command::AddConnection(cmd) => cmd.description(),
            Command::DeleteConnection(cmd) => cmd.description(),
            Command::Batch(cmd) => cmd.description(),
        }
    }
}

/// Add node command
#[derive(Debug, Clone)]
pub struct AddNodeCommand {
    pub node: BlueprintNode,
    executed: bool,
}

impl AddNodeCommand {
    pub fn new(node: BlueprintNode) -> Self {
        Self {
            node,
            executed: false,
        }
    }

    pub fn execute(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if !self.executed {
            println!("[UNDO] Executing AddNode: {}", self.node.id);
            panel.graph.nodes.push(self.node.clone());
            self.executed = true;
            cx.notify();
        }
    }

    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if self.executed {
            println!("[UNDO] Undoing AddNode: {}", self.node.id);
            panel.graph.nodes.retain(|n| n.id != self.node.id);
            panel.graph.selected_nodes.retain(|id| *id != self.node.id);
            self.executed = false;
            cx.notify();
        }
    }

    pub fn description(&self) -> String {
        format!("Add node: {}", self.node.title)
    }
}

/// Delete node command
#[derive(Debug, Clone)]
pub struct DeleteNodeCommand {
    pub node: BlueprintNode,
    pub connections: Vec<Connection>,
    executed: bool,
}

impl DeleteNodeCommand {
    pub fn new(node: BlueprintNode, connections: Vec<Connection>) -> Self {
        Self {
            node,
            connections,
            executed: false,
        }
    }

    pub fn execute(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if !self.executed {
            println!("[UNDO] Executing DeleteNode: {}", self.node.id);
            panel.graph.nodes.retain(|n| n.id != self.node.id);
            panel
                .graph
                .connections
                .retain(|c| c.source_node != self.node.id && c.target_node != self.node.id);
            panel.graph.selected_nodes.retain(|id| *id != self.node.id);
            self.executed = true;
            cx.notify();
        }
    }

    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if self.executed {
            println!("[UNDO] Undoing DeleteNode: {}", self.node.id);
            panel.graph.nodes.push(self.node.clone());
            for conn in &self.connections {
                panel.graph.connections.push(conn.clone());
            }
            self.executed = false;
            cx.notify();
        }
    }

    pub fn description(&self) -> String {
        format!("Delete node: {}", self.node.title)
    }
}

/// Add comment command
#[derive(Debug, Clone)]
pub struct AddCommentCommand {
    pub comment: BlueprintComment,
    executed: bool,
}

impl AddCommentCommand {
    pub fn new(comment: BlueprintComment) -> Self {
        Self {
            comment,
            executed: false,
        }
    }

    pub fn execute(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if !self.executed {
            println!("[UNDO] Executing AddComment: {}", self.comment.id);
            panel.graph.comments.push(self.comment.clone());
            self.executed = true;
            cx.notify();
        }
    }

    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if self.executed {
            println!("[UNDO] Undoing AddComment: {}", self.comment.id);
            panel.graph.comments.retain(|c| c.id != self.comment.id);
            panel
                .graph
                .selected_comments
                .retain(|id| *id != self.comment.id);
            self.executed = false;
            cx.notify();
        }
    }

    pub fn description(&self) -> String {
        format!("Add comment")
    }
}

/// Delete comment command
#[derive(Debug, Clone)]
pub struct DeleteCommentCommand {
    pub comment: BlueprintComment,
    executed: bool,
}

impl DeleteCommentCommand {
    pub fn new(comment: BlueprintComment) -> Self {
        Self {
            comment,
            executed: false,
        }
    }

    pub fn execute(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if !self.executed {
            println!("[UNDO] Executing DeleteComment: {}", self.comment.id);
            panel.graph.comments.retain(|c| c.id != self.comment.id);
            panel
                .graph
                .selected_comments
                .retain(|id| *id != self.comment.id);
            self.executed = true;
            cx.notify();
        }
    }

    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if self.executed {
            println!("[UNDO] Undoing DeleteComment: {}", self.comment.id);
            panel.graph.comments.push(self.comment.clone());
            self.executed = false;
            cx.notify();
        }
    }

    pub fn description(&self) -> String {
        format!("Delete comment")
    }
}

/// Delete multiple entities in one pass (optimized for large selections)
#[derive(Debug, Clone)]
pub struct DeleteEntitiesCommand {
    pub nodes: Vec<BlueprintNode>,
    pub comments: Vec<BlueprintComment>,
    pub connections: Vec<Connection>,
    executed: bool,
}

impl DeleteEntitiesCommand {
    pub fn new(
        nodes: Vec<BlueprintNode>,
        comments: Vec<BlueprintComment>,
        connections: Vec<Connection>,
    ) -> Self {
        Self {
            nodes,
            comments,
            connections,
            executed: false,
        }
    }

    pub fn execute(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if !self.executed {
            println!(
                "[UNDO] Executing DeleteEntities: {} nodes, {} comments, {} connections",
                self.nodes.len(),
                self.comments.len(),
                self.connections.len()
            );

            let node_ids: HashSet<&str> = self.nodes.iter().map(|node| node.id.as_str()).collect();
            let comment_ids: HashSet<&str> = self
                .comments
                .iter()
                .map(|comment| comment.id.as_str())
                .collect();

            panel
                .graph
                .nodes
                .retain(|node| !node_ids.contains(node.id.as_str()));
            panel
                .graph
                .comments
                .retain(|comment| !comment_ids.contains(comment.id.as_str()));
            panel.graph.connections.retain(|connection| {
                !node_ids.contains(connection.source_node.as_str())
                    && !node_ids.contains(connection.target_node.as_str())
            });

            panel.graph.selected_nodes.clear();
            panel.graph.selected_comments.clear();
            self.executed = true;
            cx.notify();
        }
    }

    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if self.executed {
            println!(
                "[UNDO] Undoing DeleteEntities: {} nodes, {} comments, {} connections",
                self.nodes.len(),
                self.comments.len(),
                self.connections.len()
            );

            panel.graph.nodes.extend(self.nodes.clone());
            panel.graph.comments.extend(self.comments.clone());
            panel.graph.connections.extend(self.connections.clone());

            self.executed = false;
            cx.notify();
        }
    }

    pub fn description(&self) -> String {
        format!("Delete {} entities", self.nodes.len() + self.comments.len())
    }
}

/// Move entities command (supports multi-selection)
#[derive(Debug, Clone)]
pub struct MoveEntitiesCommand {
    pub node_moves: Vec<(String, Point<f32>, Point<f32>)>, // (id, old_pos, new_pos)
    pub comment_moves: Vec<(String, Point<f32>, Point<f32>)>, // (id, old_pos, new_pos)
    executed: bool,
}

impl MoveEntitiesCommand {
    pub fn new(
        node_moves: Vec<(String, Point<f32>, Point<f32>)>,
        comment_moves: Vec<(String, Point<f32>, Point<f32>)>,
    ) -> Self {
        Self {
            node_moves,
            comment_moves,
            executed: false,
        }
    }

    /// Create a move command that's already been executed (for recording past moves)
    pub fn new_executed(
        node_moves: Vec<(String, Point<f32>, Point<f32>)>,
        comment_moves: Vec<(String, Point<f32>, Point<f32>)>,
    ) -> Self {
        Self {
            node_moves,
            comment_moves,
            executed: true,
        }
    }

    pub fn execute(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if !self.executed {
            println!(
                "[UNDO] Executing MoveEntities: {} nodes, {} comments",
                self.node_moves.len(),
                self.comment_moves.len()
            );

            for (node_id, _old_pos, new_pos) in &self.node_moves {
                if let Some(node) = panel.graph.nodes.iter_mut().find(|n| &n.id == node_id) {
                    node.position = *new_pos;
                }
            }

            for (comment_id, _old_pos, new_pos) in &self.comment_moves {
                if let Some(comment) = panel
                    .graph
                    .comments
                    .iter_mut()
                    .find(|c| &c.id == comment_id)
                {
                    comment.position = *new_pos;
                }
            }

            self.executed = true;
            cx.notify();
        }
    }

    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if self.executed {
            println!(
                "[UNDO] Undoing MoveEntities: {} nodes, {} comments",
                self.node_moves.len(),
                self.comment_moves.len()
            );

            for (node_id, old_pos, _new_pos) in &self.node_moves {
                if let Some(node) = panel.graph.nodes.iter_mut().find(|n| &n.id == node_id) {
                    node.position = *old_pos;
                }
            }

            for (comment_id, old_pos, _new_pos) in &self.comment_moves {
                if let Some(comment) = panel
                    .graph
                    .comments
                    .iter_mut()
                    .find(|c| &c.id == comment_id)
                {
                    comment.position = *old_pos;
                }
            }

            self.executed = false;
            cx.notify();
        }
    }

    pub fn description(&self) -> String {
        format!(
            "Move {} entities",
            self.node_moves.len() + self.comment_moves.len()
        )
    }
}

/// Add connection command
#[derive(Debug, Clone)]
pub struct AddConnectionCommand {
    pub connection: Connection,
    executed: bool,
}

impl AddConnectionCommand {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            executed: false,
        }
    }

    pub fn execute(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if !self.executed {
            println!("[UNDO] Executing AddConnection: {}", self.connection.id);
            panel.graph.connections.push(self.connection.clone());
            self.executed = true;
            cx.notify();
        }
    }

    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if self.executed {
            println!("[UNDO] Undoing AddConnection: {}", self.connection.id);
            panel
                .graph
                .connections
                .retain(|c| c.id != self.connection.id);
            self.executed = false;
            cx.notify();
        }
    }

    pub fn description(&self) -> String {
        format!("Add connection")
    }
}

/// Delete connection command
#[derive(Debug, Clone)]
pub struct DeleteConnectionCommand {
    pub connection: Connection,
    executed: bool,
}

impl DeleteConnectionCommand {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            executed: false,
        }
    }

    pub fn execute(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if !self.executed {
            println!("[UNDO] Executing DeleteConnection: {}", self.connection.id);
            panel
                .graph
                .connections
                .retain(|c| c.id != self.connection.id);
            self.executed = true;
            cx.notify();
        }
    }

    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if self.executed {
            println!("[UNDO] Undoing DeleteConnection: {}", self.connection.id);
            panel.graph.connections.push(self.connection.clone());
            self.executed = false;
            cx.notify();
        }
    }

    pub fn description(&self) -> String {
        format!("Delete connection")
    }
}

/// Batch command - groups multiple commands into one undoable action
#[derive(Debug, Clone)]
pub struct BatchCommand {
    pub commands: Vec<Command>,
    pub description: String,
    executed: bool,
}

impl BatchCommand {
    pub fn new(description: String) -> Self {
        Self {
            commands: Vec::new(),
            description,
            executed: false,
        }
    }

    pub fn add_command(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn execute(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if !self.executed {
            println!(
                "[UNDO] Executing Batch: {} ({} commands)",
                self.description,
                self.commands.len()
            );
            for command in &mut self.commands {
                command.execute(panel, cx);
            }
            self.executed = true;
        }
    }

    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) {
        if self.executed {
            println!(
                "[UNDO] Undoing Batch: {} ({} commands)",
                self.description,
                self.commands.len()
            );
            // Undo in reverse order
            for command in self.commands.iter_mut().rev() {
                command.undo(panel, cx);
            }
            self.executed = false;
        }
    }

    pub fn description(&self) -> String {
        self.description.clone()
    }
}

/// Undo/Redo manager
#[derive(Debug, Clone)]
pub struct UndoManager {
    pub(crate) undo_stack: Vec<Command>,
    pub(crate) redo_stack: Vec<Command>,
    max_undo_levels: usize,
}

impl UndoManager {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo_levels: 100,
        }
    }

    /// Push a new command to the undo stack (and clear redo stack)
    pub fn push(&mut self, command: Command) {
        println!("[UNDO] Pushing command: {}", command.description());
        self.undo_stack.push(command);

        // Limit stack size
        if self.undo_stack.len() > self.max_undo_levels {
            self.undo_stack.remove(0);
        }

        // Clear redo stack when new action is performed
        self.redo_stack.clear();
    }

    /// Undo the last command
    pub fn undo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) -> bool {
        if let Some(mut command) = self.undo_stack.pop() {
            println!("[UNDO] Undoing: {}", command.description());
            command.undo(panel, cx);
            self.redo_stack.push(command);
            true
        } else {
            println!("[UNDO] Nothing to undo");
            false
        }
    }

    /// Redo the last undone command
    pub fn redo(
        &mut self,
        panel: &mut crate::editor::workspace_panels::GraphCanvasPanel,
        cx: &mut Context<crate::editor::workspace_panels::GraphCanvasPanel>,
    ) -> bool {
        if let Some(mut command) = self.redo_stack.pop() {
            println!("[UNDO] Redoing: {}", command.description());
            command.execute(panel, cx);
            self.undo_stack.push(command);
            true
        } else {
            println!("[UNDO] Nothing to redo");
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all undo/redo history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for UndoManager {
    fn default() -> Self {
        Self::new()
    }
}
