//! Undo/redo operations for the graph canvas

use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::features::undo::Command;
use gpui::*;
use plugin_editor_api::undo::lock;

impl GraphCanvasPanel {
    /// Undo the last operation
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        let history = self.history.clone();
        let undone = lock(&history)
            .undo(|command| {
                command.undo(self, cx);
                true
            })
            .is_some();
        if undone {
            self.is_dirty = true;
            cx.notify();
        }
    }

    /// Redo the last undone operation
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        let history = self.history.clone();
        let redone = lock(&history)
            .redo(|command| {
                command.execute(self, cx);
                true
            })
            .is_some();
        if redone {
            self.is_dirty = true;
            cx.notify();
        }
    }

    /// Push a command to the undo stack
    pub fn push_undo_command(&mut self, command: Command) {
        let mut history = lock(&self.history);
        history.push(command);
        history.seal();
        self.is_dirty = true;
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        lock(&self.history).can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        lock(&self.history).can_redo()
    }
}

impl crate::editor::panel::BlueprintEditorPanel {
    /// Use the host's undo histories (Pulsar-Native#931): each graph's
    /// history is registered there under that graph of this blueprint.
    pub fn set_undo_service(&mut self, service: plugin_editor_api::UndoService, cx: &mut App) {
        self.undo_service = Some(service);
        for (id, canvas) in self.graph_panels.clone() {
            canvas.update(cx, |canvas, _| self.adopt_graph_history(&id, &canvas.history));
        }
    }

    /// Register a graph's `history` with the host, once the editor has the
    /// host's histories and knows its blueprint. Another window's editor of
    /// the same blueprint keeps its own (`UndoService::adopt`).
    pub(crate) fn adopt_graph_history(
        &self,
        graph_id: &str,
        history: &plugin_editor_api::SharedHistory<Command>,
    ) {
        if let (Some(service), Some(path)) = (&self.undo_service, &self.current_class_path) {
            let blueprint = plugin_editor_api::DocumentId::for_path(path);
            let graph = plugin_editor_api::DocumentId::new(format!("{blueprint}#{graph_id}"));
            service.adopt(&graph, history);
        }
    }
}
