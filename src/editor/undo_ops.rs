//! Undo/redo wiring between the editor and the host's histories.

use crate::features::undo::Command;
use gpui::*;

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
