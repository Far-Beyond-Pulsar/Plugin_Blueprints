//! The advanced example Blueprints (`examples/advanced/` in the plugin's folder) are real, loadable
//! classes: every embedded file matches `examples_assets/advanced/`, every graph loads through the
//! editor's loader, is commented, and the whole set compiles with the real Blueprint compiler and
//! passes the project validation (trait conformance included).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use blueprint_editor_plugin::example_files;
use blueprint_editor_plugin::io::formats::deserialize_blueprint;

const PREFIX: &str = "examples/advanced/";

/// `(path below examples/advanced/, bytes)` of every advanced example file.
fn advanced_files() -> BTreeMap<String, &'static [u8]> {
    example_files()
        .into_iter()
        .filter_map(|(path, bytes)| path.strip_prefix(PREFIX).map(|rest| (rest.to_owned(), bytes)))
        .collect()
}

fn class_graphs() -> Vec<(String, String)> {
    advanced_files()
        .into_iter()
        .filter(|(path, _)| path.ends_with(".class/graph_save.json"))
        .map(|(path, bytes)| {
            let class = path.split('/').next().unwrap().trim_end_matches(".class").to_owned();
            (class, String::from_utf8(bytes.to_vec()).expect("graph_save.json is UTF-8"))
        })
        .collect()
}

#[test]
fn the_embedded_set_matches_the_asset_folder() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples_assets/advanced");
    let mut on_disk = Vec::new();
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                out.push(path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    walk(&root, &root, &mut on_disk);
    on_disk.sort();
    let embedded: Vec<String> = advanced_files().into_keys().collect();
    assert_eq!(embedded, on_disk, "every asset file is embedded, and nothing else is");
}

#[test]
fn every_class_loads_and_is_commented() {
    let classes = class_graphs();
    assert!(classes.len() >= 12, "the advanced set ships many classes, found {}", classes.len());
    for (class, json) in &classes {
        let asset = deserialize_blueprint(json).unwrap_or_else(|e| panic!("{class}: {e}"));
        assert!(!asset.main_graph.nodes.is_empty(), "{class}: has nodes");
        assert!(!asset.main_graph.metadata.description.is_empty(), "{class}: graph description");
        assert!(!asset.blueprint_metadata.description.is_empty(), "{class}: class description");
        assert!(
            asset.main_graph.comments.len() >= 3,
            "{class}: an introduction note and step boxes ({} comments)",
            asset.main_graph.comments.len()
        );
        for variable in &asset.variables {
            assert!(!variable.description.is_empty(), "{class}: variable {} is documented", variable.name);
        }
        for comment in &asset.main_graph.comments {
            assert!(comment.text.len() > 30, "{class}: comment {} says something", comment.id);
            for id in &comment.contained_node_ids {
                assert!(asset.main_graph.nodes.contains_key(id), "{class}: comment names node {id}");
            }
        }
        // Wires are consistent with the pins they touch.
        for c in &asset.main_graph.connections {
            let source = &asset.main_graph.nodes[&c.source_node];
            let target = &asset.main_graph.nodes[&c.target_node];
            assert!(source.outputs.iter().any(|p| p.id == c.source_pin), "{class}: {}:{}", c.source_node, c.source_pin);
            assert!(target.inputs.iter().any(|p| p.id == c.target_pin), "{class}: {}:{}", c.target_node, c.target_pin);
        }
        for macro_graph in &asset.subgraphs {
            assert!(!macro_graph.description.is_empty(), "{class}: macro {} is documented", macro_graph.name);
            assert!(!macro_graph.graph.comments.is_empty(), "{class}: macro {} is commented", macro_graph.name);
        }
    }
}

#[test]
fn the_readme_indexes_every_class() {
    let files = advanced_files();
    let readme = String::from_utf8(files["README.md"].to_vec()).unwrap();
    for (class, _) in class_graphs() {
        assert!(readme.contains(&format!("{class}.class")), "README lists {class}");
    }
    assert!(readme.contains("AdvInteractable.trait.json"));
}

fn write_project(root: &Path) {
    for (path, bytes) in advanced_files() {
        let target: PathBuf = if let Some(trait_file) = path.strip_prefix("traits/") {
            root.join("types/traits").join(trait_file)
        } else if path.ends_with(".class/graph_save.json") {
            root.join("src/classes").join(&path)
        } else {
            continue;
        };
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, bytes).unwrap();
    }
}

/// Compile every class the way the editor's Compile All does, and validate the project.
#[test]
fn the_whole_set_compiles_and_validates() {
    let root = std::env::temp_dir().join(format!("bp_advanced_examples_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write_project(&root);

    let natives = pulsar_script_vm::NativeRegistry::with_engine_natives();
    let failures = blueprint_editor_plugin::validation::compile_project_classes(&root, &natives);
    let mut lines: Vec<String> = failures
        .iter()
        .map(|d| {
            format!(
                "{} {}: {}",
                d.class.as_deref().unwrap_or("?"),
                d.location.as_deref().unwrap_or("-"),
                d.message
            )
        })
        .collect();
    lines.sort();
    lines.dedup();
    assert!(failures.is_empty(), "compile problems:\n{}", lines.join("\n"));
    let compiled = blueprint_editor_plugin::validation::project_class_dirs(&root)
        .iter()
        .filter(|dir| dir.join("events/.build/module.json").is_file())
        .count();
    assert_eq!(compiled, class_graphs().len(), "every class produced a module");

    blueprint_editor_plugin::validation::validate_project_classes(&root)
        .unwrap_or_else(|problems| panic!("{problems}"));
    let _ = std::fs::remove_dir_all(&root);
}
