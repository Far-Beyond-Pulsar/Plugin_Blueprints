//! Variables feature module
//!
//! This module contains everything related to class variables:
//! - Type definitions (ClassVariable, VariableDrag, TypeItem)
//! - Variable lifecycle operations (create, delete, get/set nodes)
//! - Variable list rendering
//! - HierarchyItem implementation for hierarchical list view

pub mod hierarchy_item;
pub mod operations;
pub mod rendering;
pub mod types;

// Re-export commonly used types
pub use types::{ClassVariable, TypeItem, VariableDrag};
