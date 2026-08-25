//! Disk-level blueprint preflight validation (#656).
//!
//! Runs before PiE builds the project dylib so bad saved class graphs stop
//! Play with an actionable summary instead of surfacing as runtime failures
//! inside the embedded game. Uses the exact UI→PBGC conversion and bytecode
//! codegen the editor's Compile action runs, so what validates here is
//! byte-identical to what compiles there.

use std::collections::HashMap;
use std::path::Path;

use crate::features::compilation::compiler::convert_ui_graph_description_to_pbgc;
use crate::io::formats::{deserialize_blueprint, strip_header_comments};

/// Validate every saved class graph under `root` (the project directory).
///
/// Returns `Ok(())` when no class files exist or all of them compile cleanly;
/// returns `Err(summary)` listing each failing class otherwise.
pub fn validate_project_classes(root: &Path) -> Result<(), String> {
    let mut class_files = Vec::new();
    let classes_dir = root.join("src").join("classes");
    let scan_root = if classes_dir.is_dir() {
        classes_dir.as_path()
    } else {
        root
    };
    collect_blueprint_files(scan_root, &mut class_files);

    if class_files.is_empty() {
        return Ok(());
    }

    class_files.sort();
    let mut failures: Vec<String> = Vec::new();

    for path in class_files {
        if let Err(problem) = validate_class_file(&path) {
            failures.push(format!("  {}:\n    {}", path.display(), problem));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} class(es) failed validation:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

fn validate_class_file(path: &Path) -> Result<(), String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read file: {e}"))?;
    let asset = deserialize_blueprint(&strip_header_comments(&content))
        .map_err(|e| format!("failed to parse blueprint asset: {e}"))?;

    let mut graph = convert_ui_graph_description_to_pbgc(&asset.main_graph);

    let library: HashMap<String, pbgc::GraphDescription> = asset
        .local_macros
        .iter()
        .map(|macro_def| {
            (
                macro_def.id.clone(),
                convert_ui_graph_description_to_pbgc(&macro_def.graph),
            )
        })
        .collect();
    if !library.is_empty() {
        graphy::SubGraphExpander::new()
            .expand_all_flat(&mut graph, &library)
            .map_err(|e| format!("sub-graph expansion failed: {e}"))?;
    }

    let variables: HashMap<String, String> = asset
        .variables
        .iter()
        .map(|v| (v.name.clone(), v.data_type.to_string()))
        .collect();

    pbgc::compile_graph_to_bytecode_with_variables(&graph, variables)
        .map(|_| ())
        .map_err(|e| format!("bytecode compilation failed: {e}"))
}

fn collect_blueprint_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "target" | ".build" | "node_modules") {
                continue;
            }
            collect_blueprint_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("blueprint") {
            out.push(path);
        }
    }
}
