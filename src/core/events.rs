//! Actions and events for blueprint editor interactions.
//!
//! This module defines all GPUI actions and event types used for user interactions
//! with the blueprint editor, including context menu operations, node manipulation,
//! and cross-panel communication.

use gpui::*;
use schemars::JsonSchema;
use serde::Deserialize;

// ============================================================================
// Context Menu Actions
// ============================================================================

/// Action to duplicate a node in the blueprint graph.
#[derive(Action, Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[action(namespace = blueprint_editor)]
pub struct DuplicateNode {
    pub node_id: String,
}

/// Action to delete a node from the blueprint graph.
#[derive(Action, Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[action(namespace = blueprint_editor)]
pub struct DeleteNode {
    pub node_id: String,
}

/// Action to copy a node to the clipboard.
#[derive(Action, Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[action(namespace = blueprint_editor)]
pub struct CopyNode {
    pub node_id: String,
}

/// Action to paste a node from the clipboard.
#[derive(Action, Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[action(namespace = blueprint_editor)]
pub struct PasteNode;

/// Action to disconnect a specific pin from all connections.
#[derive(Action, Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[action(namespace = blueprint_editor)]
pub struct DisconnectPin {
    pub node_id: String,
    pub pin_id: String,
}

/// Action to open the "Add Node" menu at the current cursor position.
#[derive(Action, Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[action(namespace = blueprint_editor)]
pub struct OpenAddNodeMenu;

// ============================================================================
// Cross-Panel Events
// ============================================================================

/// Event for requesting to open an engine library in main tabs.
/// Used for communication between the library browser and main editor.
#[derive(Clone, Debug)]
pub struct OpenEngineLibraryRequest {
    pub library_id: String,
    pub library_name: String,
    pub macro_id: Option<String>, // If specified, open this macro after opening library
    pub macro_name: Option<String>,
}

/// Event for requesting to show the node picker at a specific graph position.
/// Used for communication between the editor and the global command palette.
#[derive(Clone, Debug)]
pub struct ShowNodePickerRequest {
    pub graph_position: Point<f32>,
}
