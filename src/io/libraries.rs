//! Project I/O stays in the host; the shared library model accepts bytes.
use blueprint_graph::SubGraphLibrary;
use engine_fs::virtual_fs;
use std::path::Path;

pub fn load_directory(path: &Path) -> Result<Vec<SubGraphLibrary>, Box<dyn std::error::Error>> {
    if !virtual_fs::exists(path)? {
        return Ok(Vec::new());
    }
    let mut entries = virtual_fs::list_dir(path)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let mut libraries = Vec::new();
    for entry in entries {
        let file = path.join(entry.name);
        if !entry.is_dir && file.extension().and_then(|s| s.to_str()) == Some("json") {
            let bytes = virtual_fs::read_file(&file)?;
            let library = serde_json::from_slice(&bytes)
                .map_err(|error| format!("Invalid macro library {}: {error}", file.display()))?;
            libraries.push(library);
        }
    }
    Ok(libraries)
}
