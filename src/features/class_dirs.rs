//! Where a project's Blueprint classes live, and what they are called.
//!
//! A class is a directory holding `graph_save.json`: either
//! `<project>/src/classes/<Name>/` or a `<Name>.class/` folder anywhere in
//! the project (what the content browser creates). Its class name is the
//! folder name without the `.class` extension. The engine's
//! `pulsar_class` registry finds classes by the same rules.

use std::path::{Path, PathBuf};

const GRAPH_FILE: &str = "graph_save.json";

/// `name` without a trailing `.class` extension.
pub fn strip_class_ext(name: &str) -> &str {
    name.strip_suffix(".class").filter(|n| !n.is_empty()).unwrap_or(name)
}

/// The class name of the class directory `dir` (`Door.class/` → `Door`).
pub fn class_name_of(dir: &Path) -> Option<String> {
    let name = dir.file_name()?.to_str()?;
    Some(strip_class_ext(name).to_owned())
}

fn is_class_dir(dir: &Path) -> bool {
    dir.join(GRAPH_FILE).is_file()
}

fn skip_when_searching(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "target" | "node_modules" | "Content" | "build" | "dist")
}

/// The project a class directory belongs to: `<project>` for
/// `<project>/src/classes/<Name>`, otherwise the nearest ancestor holding a
/// `Pulsar/` directory or a `Cargo.toml`.
pub fn project_root_of(class_dir: &Path) -> Option<PathBuf> {
    let parent = class_dir.parent()?;
    if parent.file_name().and_then(|n| n.to_str()) == Some("classes") {
        if let Some(src) = parent.parent().filter(|p| p.file_name().and_then(|n| n.to_str()) == Some("src")) {
            if let Some(root) = src.parent() {
                return Some(root.to_path_buf());
            }
        }
    }
    parent
        .ancestors()
        .find(|dir| dir.join("Pulsar").is_dir() || dir.join("Cargo.toml").is_file())
        .map(Path::to_path_buf)
}

/// Every class directory of the project at `root`, sorted.
pub fn project_class_dirs(root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(root.join("src").join("classes"))
        .map(|entries| entries.flatten().map(|e| e.path()).filter(|p| is_class_dir(p)).collect())
        .unwrap_or_default();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if skip_when_searching(name) {
                continue;
            }
            let is_class_folder = path.extension().and_then(|e| e.to_str()) == Some("class");
            if is_class_folder && is_class_dir(&path) {
                found.push(path);
            } else if !is_class_folder && depth < 16 && !is_class_dir(&path) {
                stack.push((path, depth + 1));
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Every class directory of the project `class_dir` belongs to, falling
/// back to its sibling class directories when no project root is found.
pub fn sibling_class_dirs(class_dir: &Path) -> Vec<PathBuf> {
    if let Some(root) = project_root_of(class_dir) {
        return project_class_dirs(&root);
    }
    let Some(parent) = class_dir.parent() else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(parent)
        .map(|entries| entries.flatten().map(|e| e.path()).filter(|p| is_class_dir(p)).collect())
        .unwrap_or_default();
    dirs.sort();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_folders_anywhere_in_the_project_are_found_and_named_without_extension() {
        let dir = std::env::temp_dir().join(format!("bp_class_dirs_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.as_path();
        std::fs::create_dir_all(root.join("Pulsar")).unwrap();
        for dir in ["src/classes/Door", "content/blueprints/Lamp.class", "Enemy.class"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join(GRAPH_FILE), "{}").unwrap();
        }
        std::fs::create_dir_all(root.join("target/debug/Stale.class")).unwrap();
        std::fs::write(root.join("target/debug/Stale.class").join(GRAPH_FILE), "{}").unwrap();

        let dirs = project_class_dirs(root);
        let names: Vec<_> = dirs.iter().filter_map(|d| class_name_of(d)).collect();
        assert_eq!(dirs.len(), 3, "{dirs:?}");
        for name in ["Door", "Lamp", "Enemy"] {
            assert!(names.iter().any(|n| n == name), "{names:?}");
        }
        let lamp = root.join("content/blueprints/Lamp.class");
        assert_eq!(project_root_of(&lamp).as_deref(), Some(root));
        assert_eq!(sibling_class_dirs(&lamp).len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
