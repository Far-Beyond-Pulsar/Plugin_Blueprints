//! File I/O and persistence layer
//!
//! Handles saving and loading blueprint files, including:
//! - Blueprint graph serialization/deserialization
//! - Legacy format support
//! - Format conversion utilities
//! - Autosave functionality

pub mod formats;
pub mod legacy;
pub mod prefab;
pub mod save_load;

// Re-export main types and functions
pub use formats::{
    deserialize_blueprint, serialize_blueprint_with_header, BlueprintAsset, BlueprintEditorState,
};
pub use legacy::{is_legacy_format, try_parse_legacy_format};
