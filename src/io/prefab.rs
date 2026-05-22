//! Prefab sidecar file I/O.

use crate::features::prefabs::PrefabAsset;
use std::path::Path;

pub fn load_prefab(path: &Path) -> Result<PrefabAsset, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read prefab sidecar: {}", e))?;
    serde_json::from_str::<PrefabAsset>(&content)
        .map_err(|e| format!("Failed to parse prefab sidecar: {}", e))
}

pub fn save_prefab(path: &Path, prefab: &PrefabAsset) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create prefab directory: {}", e))?;
    }

    let json = serde_json::to_string_pretty(prefab)
        .map_err(|e| format!("Failed to serialize prefab: {}", e))?;

    std::fs::write(path, json)
        .map_err(|e| format!("Failed to write prefab sidecar: {}", e))
}
