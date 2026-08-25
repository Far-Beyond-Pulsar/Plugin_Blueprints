//! Feature modules - each feature (nodes, connections, etc.) is self-contained
//!
//! Each feature module typically contains:
//! - types.rs: Feature-specific types
//! - operations.rs: Business logic and state mutations
//! - rendering.rs: GPUI rendering code
//! - panel.rs: Dockable panel (if applicable)

pub mod clipboard;
pub mod comments;
pub mod compilation;
pub mod connections;
pub mod debug;
pub mod events;
pub mod macros;
pub mod nodes;
pub mod prefabs;
pub mod undo;
pub mod variables;
pub mod viewport;
