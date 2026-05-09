//! Variables feature module
//!
//! This module contains everything related to class variables:
//! - Type definitions (ClassVariable, VariableDrag, TypeItem)
//! - Variable lifecycle operations (create, delete, get/set nodes)
//! - Variables panel UI
//! - Variable list rendering

pub mod operations;
pub mod panel;
pub mod rendering;
pub mod types;

// Re-export commonly used types
pub use panel::VariablesPanel;
pub use rendering::VariablesRenderer;
pub use types::{ClassVariable, TypeItem, VariableDrag};
