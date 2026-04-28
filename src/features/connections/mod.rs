//! Connection management feature module
//!
//! This module handles all connection-related functionality including:
//! - Connection dragging and creation
//! - Connection rendering with bezier curves
//! - Type compatibility checking
//! - Reroute node type inference

pub mod compatibility;
pub mod operations;
pub mod rendering;

pub use compatibility::*;
pub use operations::*;
pub use rendering::*;
