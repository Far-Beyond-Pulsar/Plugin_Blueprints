//! The example Blueprints the plugin ships, served from the plugin's folder in
//! the virtual file system as `examples/...`.
//!
//! Every example is a real `.class` folder (`graph_save.json` plus the
//! `events/` folder the editor expects) kept under `examples_assets/` and
//! embedded with `include_bytes!`:
//!
//! | Set | Module | Assets |
//! |---|---|---|
//! | basics and intermediate | [`basics`] | `examples_assets/basics/**` |
//! | advanced | [`advanced`] | `examples_assets/advanced/**` |
//!
//! The folder is read-only like every plugin folder; open an example and use
//! "Save a copy" (or copy the `.class` folder into the project) to edit it.
//! `tests/examples.rs` loads every embedded example and compiles it with the
//! real Blueprint compiler.

use std::sync::{Arc, OnceLock};

use engine_fs::{EmbeddedFile, FolderSource, MemoryFolder, PluginFolder};

pub(crate) mod advanced;
pub(crate) mod basics;

/// Every embedded example file, at its path inside the plugin's folder
/// (`examples/basics/...`, `examples/advanced/...`, `examples/README.md`).
pub(crate) fn embedded() -> &'static [EmbeddedFile] {
    static ALL: OnceLock<Vec<EmbeddedFile>> = OnceLock::new();
    ALL.get_or_init(|| {
        basics::FILES
            .iter()
            .chain(advanced::FILES.iter())
            .copied()
            .collect()
    })
}

/// The plugin's folder: the embedded examples, the same for every project.
pub(crate) fn open(folder: PluginFolder) -> Arc<dyn FolderSource> {
    Arc::new(MemoryFolder::with_files(folder, embedded()))
}
