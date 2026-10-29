//! Tab system for multiple graph views
//!
//! Allows opening multiple graphs (main graph + macros) in tabs

use graphy::GraphDescription;

use crate::io::canvas_doc::AuthoredSidecar;

/// The document of a tab whose canvas has not been created yet. The canvas
/// takes it over when it is mounted, so a graph is only ever held by one owner.
#[derive(Clone, Debug)]
pub struct TabDoc {
    pub graph: GraphDescription,
    pub sidecar: AuthoredSidecar,
    /// Saved `(pan, zoom)`.
    pub view: Option<((f32, f32), f32)>,
}

impl TabDoc {
    pub fn new(graph: GraphDescription, sidecar: AuthoredSidecar) -> Self {
        Self {
            graph,
            sidecar,
            view: None,
        }
    }

    pub fn empty(name: &str) -> Self {
        Self::new(GraphDescription::new(name), AuthoredSidecar::default())
    }
}

/// Represents a tab in the blueprint editor (main graph or macro)
#[derive(Clone, Debug)]
pub struct GraphTab {
    pub id: String,
    pub name: String,
    /// Present until the tab's canvas panel exists.
    pub pending: Option<TabDoc>,
    pub is_main: bool,
    pub is_dirty: bool,
    pub is_library_macro: bool,
    pub library_id: Option<String>,
}

impl GraphTab {
    pub fn new(id: String, name: String, doc: TabDoc) -> Self {
        Self {
            id,
            name,
            pending: Some(doc),
            is_main: false,
            is_dirty: false,
            is_library_macro: false,
            library_id: None,
        }
    }

    pub fn new_main(doc: TabDoc) -> Self {
        Self {
            id: "main".to_string(),
            name: "EventGraph".to_string(),
            is_main: true,
            ..Self::new(String::new(), String::new(), doc)
        }
    }

    pub fn new_local_macro(id: String, name: String, doc: TabDoc) -> Self {
        Self::new(id, name, doc)
    }

    pub fn new_library_macro(id: String, name: String, library_id: String, doc: TabDoc) -> Self {
        Self {
            is_library_macro: true,
            library_id: Some(library_id),
            ..Self::new(id, name, doc)
        }
    }
}
