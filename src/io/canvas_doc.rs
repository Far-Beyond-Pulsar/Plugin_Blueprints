//! Boundary between the authored `.blueprint` schema and the editing document.
//!
//! The editor edits one in-memory `graphy::GraphDescription` (owned by the
//! Graphy canvas). The authored schema (`graphy_blueprint::GraphDescription`) is
//! read once on load and rebuilt once on save; nothing else converts between
//! the two. The conversion itself is Graphy's (`graphy::authored`, shared with
//! the Animation Graph editor); this file adds what only a Blueprint carries
//! that the canvas document has no field for, [`AuthoredSidecar`], which is
//! written back unchanged.

use std::collections::HashMap;

use graphy::GraphDescription;
use graphy_blueprint as authored;

pub use graphy::authored::connection_id;

/// Authored fields the canvas document does not model.
#[derive(Clone, Debug, Default)]
pub struct AuthoredSidecar {
    pub custom_event_defs: HashMap<String, authored::CustomEventDefDescription>,
}

/// Reads an authored graph into the editing document.
pub fn from_authored(graph: &authored::GraphDescription) -> (GraphDescription, AuthoredSidecar) {
    let sidecar = AuthoredSidecar {
        custom_event_defs: graph.custom_event_defs.clone(),
    };
    (graphy::authored::from_authored(graph), sidecar)
}

/// Builds the authored graph for saving or compiling.
pub fn to_authored(doc: &GraphDescription, sidecar: &AuthoredSidecar) -> authored::GraphDescription {
    let mut out = graphy::authored::to_authored(doc);
    out.custom_event_defs = sidecar.custom_event_defs.clone();
    out
}

/// A node property as the editor's text fields hold it, as the document stores it.
pub fn property_value(raw: &str) -> graphy::JsonValue {
    blueprint_compiler::authored::property_value_from_raw(raw)
}

/// A stored node property as editor text.
pub fn property_string(value: &graphy::JsonValue) -> String {
    match value {
        graphy::JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}
