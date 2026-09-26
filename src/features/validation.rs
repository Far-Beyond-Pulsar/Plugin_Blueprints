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
                let problems = validate_asset(&asset, path.parent());
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
/// structural checks, macro expansion and a script module compile dry-run.
///
/// `class_dir` is the class's directory (`src/classes/<Class>`): its name
/// qualifies the class's custom events and its siblings' compiled modules
/// supply the other classes' events.
pub(crate) fn validate_asset(
    asset: &crate::io::formats::BlueprintAsset,
    class_dir: Option<&std::path::Path>,
) -> Vec<String> {
    compile_asset(asset, class_dir, crate::features::compilation::compiler::script_natives())
        .problems
        .into_iter()
        .map(|(_, message)| message)
        .collect()
}

/// What compiling one saved class produced.
pub(crate) struct AssetCompile {
    /// The module, when the class compiled with no problem at all.
    pub module: Option<pulsar_script_vm::Module>,
    /// Every problem, as `(graph node, message)`.
    pub problems: Vec<(Option<String>, String)>,
}

/// Compile a saved class the way Play runs it: structural checks, the
/// UI -> PBGC conversion, local macro expansion, data-flow analysis and the
/// script module compile against `natives`. No GPUI state is involved.
pub(crate) fn compile_asset(
    asset: &crate::io::formats::BlueprintAsset,
    class_dir: Option<&std::path::Path>,
    natives: &pulsar_script_vm::NativeRegistry,
) -> AssetCompile {
    let mut report = ValidationReport::default();
    check_ui_graph(&asset.main_graph, &mut report);

    let mut graph =
        crate::features::compilation::compiler::convert_ui_graph_description_to_pbgc(
            &asset.main_graph,
        );
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
        if let Err(e) = graphy::SubGraphExpander::new().expand_all_flat(&mut graph, &library) {
            report.push(format!("sub-graph expansion failed: {e}"));
            return AssetCompile {
                module: None,
                problems: report.diagnostics.into_iter().map(|m| (None, m)).collect(),
            };
        }
    }
    check_pbgc_graph(&graph, &mut report);
    let mut problems: Vec<(Option<String>, String)> =
        report.diagnostics.into_iter().map(|m| (None, m)).collect();

    // Compile exactly what Play will run.
    let variables: Vec<blueprint_compiler::VariableSource> = asset
        .variables
        .iter()
        .map(|v| blueprint_compiler::VariableSource {
            name: v.name.clone(),
            type_name: v.data_type.to_string(),
            default: v
                .default_value
                .as_deref()
                .map(crate::features::compilation::compiler::property_value_from_raw),
        })
        .collect();
    let class_name = class_dir
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("validation")
        .to_owned();
    let events: Vec<blueprint_compiler::EventSource> = asset
        .local_events
        .iter()
        .map(|e| blueprint_compiler::EventSource {
            uid: e.uid.clone(),
            name: e.name.clone(),
            fields: e.fields.iter().map(|f| (f.name.clone(), f.type_name.clone())).collect(),
        })
        .collect();
    let known_events = crate::features::events::engine_events::known_event_signatures(class_dir);
    let source = blueprint_compiler::ClassSource {
        name: &class_name,
        graph: &graph,
        variables: &variables,
        events: &events,
        known_events: &known_events,
    };
    let module = match blueprint_compiler::compile(&source, natives) {
        Ok(module) => Some(module),
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                problems.push((diagnostic.node.clone(), format!("compile: {diagnostic}")));
            }
            None
        }
    };
    AssetCompile { module: module.filter(|_| problems.is_empty()), problems }
}

/// The class directories under `<root>/src/classes` holding a saved graph
/// (`graph_save.json`), sorted.
pub fn project_class_dirs(root: &Path) -> Vec<std::path::PathBuf> {
    let classes = root.join("src").join("classes");
    let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&classes)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.join(GRAPH_FILE).is_file())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs
}

const GRAPH_FILE: &str = "graph_save.json";

/// Compile one class directory's saved graph and write its module to
/// `events/.build/module.json`. On failure no module is left behind (a
/// stale one would run old code).
fn compile_class_dir(
    dir: &Path,
    natives: &pulsar_script_vm::NativeRegistry,
) -> Result<(), Vec<plugin_editor_api::CompileDiagnostic>> {
    use plugin_editor_api::CompileDiagnostic;
    let class = dir.file_name().and_then(|n| n.to_str()).map(str::to_owned);
    let graph_file = dir.join(GRAPH_FILE);
    let out_dir = dir.join("events").join(".build");
    let out = out_dir.join("module.json");
    let fail = |message: String| {
        let _ = std::fs::remove_file(&out);
        vec![CompileDiagnostic::error(class.clone(), Some(graph_file.clone()), message)]
    };
    let text = std::fs::read_to_string(&graph_file).map_err(|e| fail(format!("failed to read: {e}")))?;
    let asset = crate::io::formats::deserialize_blueprint(&crate::io::formats::strip_header_comments(&text))
        .map_err(|e| fail(format!("failed to parse blueprint asset: {e}")))?;
    let compiled = compile_asset(&asset, Some(dir), natives);
    let Some(module) = compiled.module else {
        let _ = std::fs::remove_file(&out);
        return Err(compiled
            .problems
            .into_iter()
            .map(|(node, message)| CompileDiagnostic {
                location: node.map(|n| format!("node {n}")),
                ..CompileDiagnostic::error(class.clone(), Some(graph_file.clone()), message)
            })
            .collect());
    };
    let json = module.to_json().map_err(|e| fail(format!("failed to serialise module: {e}")))?;
    std::fs::create_dir_all(&out_dir).map_err(|e| fail(format!("failed to create {}: {e}", out_dir.display())))?;
    std::fs::write(&out, json).map_err(|e| fail(format!("failed to write {}: {e}", out.display())))?;
    Ok(())
}

/// Compile every Blueprint class of the project at `root` headlessly
/// (#879): each `src/classes/<Class>/graph_save.json` to its
/// `events/.build/module.json`, linking native calls against `natives`.
///
/// Classes may handle each other's events, which they learn from their
/// siblings' compiled modules; a class that fails because a sibling was
/// not compiled yet is retried after the others, for as long as a round
/// makes progress. Returns the problems of the classes that still fail.
pub fn compile_project_classes(
    root: &Path,
    natives: &pulsar_script_vm::NativeRegistry,
) -> Vec<plugin_editor_api::CompileDiagnostic> {
    let mut pending = project_class_dirs(root);
    let mut failures = Vec::new();
    while !pending.is_empty() {
        failures.clear();
        let mut failed = Vec::new();
        for dir in &pending {
            match compile_class_dir(dir, natives) {
                Ok(()) => tracing::info!(class = %dir.display(), "Blueprint class compiled"),
                Err(problems) => {
                    failures.extend(problems);
                    failed.push(dir.clone());
                }
            }
        }
        if failed.len() == pending.len() {
            break;
        }
        pending = failed;
    }
    failures
}

