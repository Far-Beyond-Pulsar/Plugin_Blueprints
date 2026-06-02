//! Undo/redo operations for the graph canvas

use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::features::undo::Command;
use gpui::*;

impl GraphCanvasPanel {
    /// Undo the last operation
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        // Pop command from undo stack and perform undo
        if let Some(mut command) = self.undo_manager.undo_stack.pop() {
            println!("[UNDO] Undoing: {}", command.description());
            command.undo(self, cx);
            self.undo_manager.redo_stack.push(command);
            self.is_dirty = true;
        } else {
            println!("[UNDO] Nothing to undo");
        }
    }

    /// Redo the last undone operation
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        // Pop command from redo stack and perform redo
        if let Some(mut command) = self.undo_manager.redo_stack.pop() {
            println!("[UNDO] Redoing: {}", command.description());
            command.execute(self, cx);
            self.undo_manager.undo_stack.push(command);
            self.is_dirty = true;
        } else {
            println!("[UNDO] Nothing to redo");
        }
    }

    /// Push a command to the undo stack
    pub fn push_undo_command(&mut self, command: Command) {
        self.undo_manager.push(command);
        self.is_dirty = true;
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.undo_manager.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.undo_manager.can_redo()
    }
}
