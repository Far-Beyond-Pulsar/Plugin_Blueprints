//! Undo/redo entries for a graph.
//!
//! Every edit is a batch of Graphy commands applied by the canvas, which hands
//! back the exact `ChangeSet` it made. An entry keeps both: the change set to
//! revert and the commands to replay.

use graphy::{ChangeSet, GraphCommand};

/// One undoable edit of a graph.
#[derive(Debug, Clone)]
pub struct Command {
    pub label: String,
    pub commands: Vec<GraphCommand>,
    pub changes: ChangeSet,
}

impl Command {
    pub fn new(label: impl Into<String>, commands: Vec<GraphCommand>, changes: ChangeSet) -> Self {
        Self {
            label: label.into(),
            commands,
            changes,
        }
    }
}

/// A graph's undo history entry. The history itself is the host's
/// (`plugin_editor_api::undo`, Pulsar-Native#931): one per graph of a
/// blueprint, shared by every view of it.
impl plugin_editor_api::UndoEntry for Command {
    fn label(&self) -> String {
        self.label.clone()
    }
}

/// A new graph history, kept until the editor learns its document.
pub fn new_history() -> plugin_editor_api::SharedHistory<Command> {
    std::sync::Arc::new(std::sync::Mutex::new(plugin_editor_api::History::with_capacity(100)))
}
