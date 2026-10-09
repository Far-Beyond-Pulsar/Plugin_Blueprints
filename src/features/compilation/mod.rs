//! Compilation feature - Compiling blueprints to Rust code
//!
//! This module handles:
//! - Compiling graphs to Rust source code
//! - Converting between BlueprintGraph and GraphDescription formats
//! - Compilation status tracking and history
//! - Compiler panel rendering

pub mod compiler;
pub mod conversion;
pub mod panel;
mod trait_contracts;
