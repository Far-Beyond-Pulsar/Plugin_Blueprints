//! Versioned, byte-oriented asset loading shared by editor and build tools.
use crate::BlueprintAsset;
use std::collections::HashSet;

pub const FORMAT_VERSION: u32 = 1;

pub fn current_format_version() -> u32 {
    FORMAT_VERSION
}
pub fn is_version_supported(version: u32) -> bool {
    version == FORMAT_VERSION
}

pub fn strip_header_comments(content: &str) -> String {
    content
        .lines()
        .skip_while(|line| line.trim().is_empty() || line.trim().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn deserialize_blueprint(content: &str) -> Result<BlueprintAsset, String> {
    let asset: BlueprintAsset = serde_json::from_str(&strip_header_comments(content))
        .map_err(|e| format!("Failed to deserialize blueprint: {e}"))?;
    if !is_version_supported(asset.format_version) {
        return Err(format!(
            "Unsupported blueprint format version {} (expected {FORMAT_VERSION})",
            asset.format_version
        ));
    }
    let mut ids = HashSet::new();
    for variable in &asset.variables {
        if variable.id.is_empty() || !ids.insert(&variable.id) {
            return Err(format!(
                "Variable {} has an empty or duplicate id",
                variable.name
            ));
        }
    }
    Ok(asset)
}
