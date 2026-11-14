//! Every example Blueprint the plugin ships (`examples/` in its folder) loads
//! and compiles with the real Blueprint compiler, and the basics are run on the
//! script VM, their variables checked against what their comments promise.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use blueprint_editor_plugin::io::formats::{deserialize_blueprint, strip_header_comments};
use blueprint_editor_plugin::validation::compile_project_classes;
use pulsar_script_vm::{
    Budget, Completion, Host, Module, NativeRegistry, Program, Value, Vm,
};

/// The embedded files written out as a project folder; every example under
/// `<root>/examples/...`.
fn materialize(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("bp_examples_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, bytes) in blueprint_editor_plugin::example_files() {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }
    root
}

fn class_dirs(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.extension().and_then(|e| e.to_str()) == Some("class") {
                    out.push(path);
                } else {
                    walk(&path, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

fn name_of(dir: &Path) -> String {
    dir.file_stem().unwrap().to_string_lossy().into_owned()
}

#[test]
fn the_embedded_examples_are_well_formed() {
    let files = blueprint_editor_plugin::example_files();
    let paths: BTreeSet<&str> = files.iter().map(|(path, _)| *path).collect();
    assert_eq!(paths.len(), files.len(), "an example path is embedded twice");
    for (path, bytes) in &files {
        assert!(path.starts_with("examples/"), "{path} is outside examples/");
        assert!(!bytes.is_empty(), "{path} is empty");
    }
    assert!(paths.contains("examples/README.md"), "the examples index is missing");
}

#[test]
fn every_example_class_loads_and_is_commented() {
    let root = materialize("load");
    let classes = class_dirs(&root);
    assert!(!classes.is_empty(), "no example class is embedded");
    let mut names = BTreeSet::new();
    for dir in &classes {
        let name = name_of(dir);
        assert!(names.insert(name.clone()), "class name {name} is used twice");
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit()),
            "{name} is not a valid class (Rust module) name"
        );
        let text = std::fs::read_to_string(dir.join("graph_save.json")).unwrap();
        let asset = deserialize_blueprint(&strip_header_comments(&text))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            !asset.main_graph.comments.is_empty(),
            "{name}: an example explains itself with comment boxes"
        );
        assert!(
            !asset.blueprint_metadata.description.trim().is_empty(),
            "{name}: describe the example in its blueprint metadata"
        );
        for comment in &asset.main_graph.comments {
            assert!(!comment.text.trim().is_empty(), "{name}: an empty comment box");
        }
    }
}

/// What each example leaves in its variables after `begin_play` (and the
/// events it triggers) ran.
fn expectations() -> Vec<(&'static str, &'static str, Value)> {
    include!("examples_expectations.in")
}

#[test]
fn every_example_compiles_and_runs() {
    let root = materialize("compile");
    let classes = class_dirs(&root);
    let natives = NativeRegistry::with_engine_natives();
    let failures = compile_project_classes(&root, &natives);
    assert!(
        failures.is_empty(),
        "examples that do not compile:\n{}",
        failures
            .iter()
            .map(|d| format!("{d:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let expectations = expectations();
    for dir in &classes {
        let name = name_of(dir);
        // Only the basics run here (they need nothing from a game world); every
        // set is compiled above.
        if !dir.components().any(|c| c.as_os_str() == "basics") {
            continue;
        }
        // Engine timers need the running game's event hub: compile-only.
        const COMPILE_ONLY: &[&str] = &["Timers"];
        if COMPILE_ONLY.contains(&name.as_str()) {
            continue;
        }
        let json = std::fs::read_to_string(dir.join("events/.build/module.json"))
            .unwrap_or_else(|e| panic!("{name}: no compiled module: {e}"));
        let module = Module::from_json(&json).unwrap_or_else(|e| panic!("{name}: {e}"));
        let program = Program::link(Arc::new(module), &natives)
            .unwrap_or_else(|e| panic!("{name}: link: {e:?}"));
        let mut world = pulsar_scenedb::World::new();
        let entity = world.spawn();
        let mut vm = Vm::default();
        let mut instance = program.instantiate();
        // Entry points in the order a game runs them.
        for (entry, args) in [
            ("begin_play", vec![]),
            ("main", vec![]),
            ("tick", vec![Value::Float(0.016)]),
            ("tick", vec![Value::Float(0.016)]),
            ("end_play", vec![]),
        ] {
            let Some(func) = program.entry(entry) else { continue };
            let mut completion = vm
                .start(
                    &program,
                    &mut instance,
                    func,
                    &args,
                    &mut Host::new(&mut world, entity),
                    &mut Budget::new(2_000_000),
                )
                .unwrap_or_else(|e| panic!("{name}: {entry} failed: {e}"));
            // A latent node (`delay`, `while_loop`) waits; the test does not
            // wait in game time, it resumes at once.
            let mut resumes = 0;
            while let Completion::Waiting { continuation, .. } = completion {
                resumes += 1;
                assert!(resumes < 100_000, "{name}: {entry} never finishes");
                completion = vm
                    .resume(
                        &program,
                        &mut instance,
                        continuation,
                        &mut Host::new(&mut world, entity),
                        &mut Budget::new(2_000_000),
                    )
                    .unwrap_or_else(|e| panic!("{name}: {entry} failed after waiting: {e}"));
            }
        }
        for (class, variable, expected) in expectations.iter().filter(|(c, ..)| *c == name) {
            let index = program
                .variable(variable)
                .unwrap_or_else(|| panic!("{class}: no variable `{variable}`"));
            assert_eq!(
                program.var(&instance, index),
                Some(expected),
                "{class}: variable `{variable}`"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&root);
}
