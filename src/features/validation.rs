//! Graph validation stage (#656): bad graphs never reach codegen.
//!
//! Runs the same UI→PBGC conversion and macro expansion codegen uses, then
//! exercises PBGC's data-flow resolver so broken connections, dangling pins
//! and unresolvable node types surface as diagnostics *before* any artifact
//! is produced.

use std::collections::HashMap;
use std::path::Path;

/// Validate every saved class graph under `root` (the project directory).
///
/// Returns `Ok(())` when no class files exist or all of them compile cleanly;
/// returns `Err(summary)` listing each failing class otherwise. Used by PiE's
/// build preflight so bad graphs stop Play before the project dylib builds.
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
        let problem = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read file: {e}"))
            .and_then(|content| {
                crate::io::formats::deserialize_blueprint(
                    &crate::io::formats::strip_header_comments(&content),
                )
                .map_err(|e| format!("failed to parse blueprint asset: {e}"))
            })
            .and_then(|asset| {
                let problems = validate_asset(&asset);
                if problems.is_empty() {
                    Ok(())
                } else {
                    Err(problems.join("\n    "))
                }
            });

        if let Err(problem) = problem {
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

/// Which compile pipeline a validation pass is guarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationTarget {
    DirectRust,
    BytecodeVm,
}

impl ValidationTarget {
    pub fn label(self) -> &'static str {
        match self {
            ValidationTarget::DirectRust => "DirectRust",
            ValidationTarget::BytecodeVm => "BytecodeVm",
        }
    }
}

/// Result of one validation pass over the panel's current graph.
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub target: Option<ValidationTarget>,
    /// Human-readable problems; empty means the graph is clean.
    pub diagnostics: Vec<String>,
}

impl ValidationReport {
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn summary(&self) -> String {
        if self.diagnostics.is_empty() {
            "all checks passed".to_string()
        } else if self.diagnostics.len() == 1 {
            "1 error".to_string()
        } else {
            format!("{} errors", self.diagnostics.len())
        }
    }

    pub(crate) fn push(&mut self, message: String) {
        self.diagnostics.push(message);
    }

    pub(crate) fn merge(&mut self, other: Vec<String>) {
        self.diagnostics.extend(other);
    }
}

/// Structural diagnostics for a UI-level graph description.
pub(crate) fn check_ui_graph_diagnostics(
    graph: &ui::graph::GraphDescription,
) -> Vec<String> {
    let mut report = ValidationReport::default();
    check_ui_graph(graph, &mut report);
    report.diagnostics
}

/// Structural checks on the raw UI graph — the checks conversion silently
/// skips (dangling endpoints) are reported loudly here instead.
fn check_ui_graph(
    graph: &ui::graph::GraphDescription,
    report: &mut ValidationReport,
) {
    for (id, node) in &graph.nodes {
        if node.node_type.trim().is_empty() {
            report.push(format!(
                "node `{id}` has an empty node type and will compile to nothing"
            ));
        }
    }

    for conn in &graph.connections {
        let Some(source) = graph.nodes.get(&conn.source_node) else {
            report.push(format!(
                "connection {}:{} → {}:{} references missing source node",
                conn.source_node, conn.source_pin, conn.target_node, conn.target_pin
            ));
            continue;
        };
        let Some(target) = graph.nodes.get(&conn.target_node) else {
            report.push(format!(
                "connection {}:{} → {}:{} references missing target node",
                conn.source_node, conn.source_pin, conn.target_node, conn.target_pin
            ));
            continue;
        };
        if !source.outputs.iter().any(|p| p.id == conn.source_pin) {
            report.push(format!(
                "connection {}:{} → {}:{} references missing output pin on source",
                conn.source_node, conn.source_pin, conn.target_node, conn.target_pin
            ));
        }
        if !target.inputs.iter().any(|p| p.id == conn.target_pin) {
            report.push(format!(
                "connection {}:{} → {}:{} references missing input pin on target",
                conn.source_node, conn.source_pin, conn.target_node, conn.target_pin
            ));
        }
    }
}

/// Validate an already-converted + expanded PBGC graph with the same
/// data-flow analysis codegen runs. Errors here are hard failures that
/// would otherwise abort compilation mid-codegen.
fn check_pbgc_graph(graph: &pbgc::GraphDescription, report: &mut ValidationReport) {
    let metadata_provider = pbgc::metadata::BlueprintMetadataProvider::new();
    match graphy::DataResolver::build(graph, &metadata_provider) {
        Ok(_) => {}
        Err(e) => report.push(format!("data flow analysis failed: {e}")),
    }
    let _ = graphy::ExecutionRouting::build_from_graph(graph);
}

/// Validate a saved [`crate::io::formats::BlueprintAsset`] end-to-end:
/// structural checks, macro expansion and full bytecode dry-run.
pub(crate) fn validate_asset(
    asset: &crate::io::formats::BlueprintAsset,
) -> Vec<String> {
    let mut report = ValidationReport::default();
    check_ui_graph(&asset.main_graph, &mut report);

    let mut graph =
        crate::features::compilation::compiler::convert_ui_graph_description_to_pbgc(
            &asset.main_graph,
        );
    {
        let library: HashMap<String, pbgc::GraphDescription> = asset
            .local_macros
            .iter()
            .map(|macro_def| {
                (
                    macro_def.id.clone(),
                    crate::features::compilation::compiler::convert_ui_graph_description_to_pbgc(
                        &macro_def.graph,
                    ),
                )
            })
            .collect();
        if !library.is_empty() {
            if let Err(e) =
                graphy::SubGraphExpander::new().expand_all_flat(&mut graph, &library)
            {
                report.push(format!("sub-graph expansion failed: {e}"));
                return report.diagnostics;
            }
        }
        check_pbgc_graph(&graph, &mut report);

        let variables: HashMap<String, String> = asset
            .variables
            .iter()
            .map(|v| (v.name.clone(), v.data_type.to_string()))
            .collect();
        if let Err(e) = pbgc::compile_graph_to_bytecode_with_variables(&graph, variables) {
            report.push(format!("bytecode dry-run failed: {e}"));
        }
    }

    report.diagnostics
}
