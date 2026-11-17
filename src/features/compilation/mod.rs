//! Compilation feature - Compiling blueprints to Rust code
//!
//! This module handles:
//! - Compiling graphs to Rust source code
//! - Converting between BlueprintGraph and GraphDescription formats
//! - Compilation status tracking and history

pub mod compiler;
pub mod conversion;
mod trait_contracts;
