//! Blueprint-level definitions shared by the editor panels.
//!
//! The graph itself is the Graphy canvas document (`graphy::GraphDescription`,
//! owned by each tab's canvas); nothing here models nodes or wires.

/// A single field in an event definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomEventField {
    pub name: String,
    pub type_name: String,
}

/// Shared definition for a custom event, stored at the panel level (mirrors
/// how macros are stored).  The On node and all Dispatch nodes derive their
/// pins from this definition.
#[derive(Clone, Debug, Default)]
pub struct EventDefinition {
    pub uid: String,
    pub name: String,
    pub fields: Vec<CustomEventField>,
    pub return_type: String,
}
