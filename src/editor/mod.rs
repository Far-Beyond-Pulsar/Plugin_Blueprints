//! Main editor state container and lifecycle management.
//!
//! This module contains the BlueprintEditorPanel - the central state container
//! for the blueprint editor, along with workspace, tabs, and toolbar.

pub mod panel;
pub mod panel_render;
pub mod tabs;
pub mod toolbar;
pub mod workspace;
pub mod workspace_panels;

pub use panel::BlueprintEditorPanel;
pub use tabs::GraphTab;
