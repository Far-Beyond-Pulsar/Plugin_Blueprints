//! Versioned, byte-oriented asset loading shared by editor and build tools.
use crate::BlueprintAsset;
use std::collections::HashSet;

pub const FORMAT_VERSION: u32 = 2;

pub fn current_format_version() -> u32 {
    FORMAT_VERSION
}
pub fn is_version_supported(version: u32) -> bool {
    matches!(version, 1 | FORMAT_VERSION)
}

pub fn strip_header_comments(content: &str) -> String {
    content
        .lines()
        .skip_while(|line| line.trim().is_empty() || line.trim().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn deserialize_blueprint(content: &str) -> Result<BlueprintAsset, String> {
    let mut value: serde_json::Value = serde_json::from_str(&strip_header_comments(content))
        .map_err(|e| format!("Failed to deserialize blueprint: {e}"))?;
    let version = value
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .ok_or_else(|| "Blueprint asset is missing a valid format_version".to_owned())?;
    if !is_version_supported(version) {
        return Err(format!(
            "Unsupported blueprint format version {version} (expected {FORMAT_VERSION})"
        ));
    }
    if version == 1 {
        // Version 1's schema contract stores comment color arrays as RGBA.
        // The editor's legacy HSL representation is the named `{h, s, l, a}`
        // object form, which the schema deserializer converts explicitly.
        // Untagged arrays cannot be safely distinguished as HSLA or RGBA, so
        // preserve them according to the v1 contract instead of guessing.
        value["format_version"] = serde_json::Value::from(FORMAT_VERSION);
    }
    let asset: BlueprintAsset = serde_json::from_value(value)
        .map_err(|e| format!("Failed to deserialize blueprint: {e}"))?;
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
