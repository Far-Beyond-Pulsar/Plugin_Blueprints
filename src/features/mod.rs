//! Feature modules - each feature (nodes, connections, etc.) is self-contained
//!
//! Each feature module typically contains:
//! - types.rs: Feature-specific types
//! - operations.rs: Business logic and state mutations
//! - rendering.rs: GPUI rendering code
//! - panel.rs: Dockable panel (if applicable)

pub mod class_dirs;
pub mod compilation;
pub mod debug;
pub mod events;
pub mod macros;
pub mod prefabs;
pub mod script_problems;
pub mod traits;
pub mod undo;
pub mod validation;
pub mod variables;
pub mod viewport;
