//! Actions and events for blueprint editor interactions.
//!
//! This module defines all GPUI actions and event types used for user interactions
//! with the blueprint editor, including context menu operations, node manipulation,
//! and cross-panel communication.

use gpui::*;

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
