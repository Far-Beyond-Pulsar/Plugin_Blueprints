//! Node operations and rendering module
//!
//! This module handles all node-related functionality:
//! - Node manipulation (add, delete, duplicate, copy/paste)
//! - Node dragging operations
//! - Node rendering (standard nodes, reroute nodes, pins)
//! - Node selection

pub mod operations;
pub mod rendering;
pub mod rendering_spans;
pub mod selection;

// Re-export commonly used items
pub use operations::*;
pub use rendering::*;
pub use selection::*;
