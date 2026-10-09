//! Public validation facade.
//!
//! The real engine lives in [`crate::features::validation`]; this module keeps
//! the crate-root path stable for external consumers (PiE preflight calls
//! `blueprint_editor_plugin::validation::validate_project_classes`).

pub use crate::features::validation::{
    compile_project_classes, compile_project_classes_with_component_events, project_class_dirs,
    validate_project_classes, validate_project_classes_with_component_events, ValidationReport,
    ValidationTarget,
};
