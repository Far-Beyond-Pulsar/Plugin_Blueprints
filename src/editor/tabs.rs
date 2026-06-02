//! Tab system for multiple graph views
//!
//! Allows opening multiple graphs (main graph + macros) in tabs

use crate::core::graph::BlueprintGraph;

/// Represents a tab in the blueprint editor (main graph or macro)
#[derive(Clone, Debug)]
pub struct GraphTab {
    pub id: String,
    pub name: String,
    pub graph: BlueprintGraph,
    pub is_main: bool,
    pub is_dirty: bool,
    pub is_library_macro: bool,
    pub library_id: Option<String>,
}

impl GraphTab {
    pub fn new(id: String, name: String, graph: BlueprintGraph) -> Self {
        Self { id, name, graph, is_main: false, is_dirty: false, is_library_macro: false, library_id: None }
    }

    pub fn new_main(graph: BlueprintGraph) -> Self {
        Self { id: "main".to_string(), name: "EventGraph".to_string(), graph, is_main: true, is_dirty: false, is_library_macro: false, library_id: None }
    }

    pub fn new_local_macro(id: String, name: String, graph: BlueprintGraph) -> Self {
        Self { id, name, graph, is_main: false, is_dirty: false, is_library_macro: false, library_id: None }
    }

    pub fn new_library_macro(id: String, name: String, library_id: String, graph: BlueprintGraph) -> Self {
        Self { id, name, graph, is_main: false, is_dirty: false, is_library_macro: true, library_id: Some(library_id) }
    }
}
