//! Macros feature - Local and global macro/subgraph management
//!
//! This module handles:
//! - Opening and creating local macros
//! - Navigating to engine library macros
//! - Macro panel rendering
//! - HierarchyItem implementation for hierarchical list view

pub mod hierarchy_delegate;
pub mod hierarchy_item;
pub mod operations;
pub mod panel;

// Re-export commonly used types
pub use hierarchy_item::{MacroDrag, MacroHierarchyItem};
