//! The advanced example set: component-style actors, state machines, trait implementers, event
//! dispatchers, class inheritance, latent flows, data-driven logic, a small game and macros.
//!
//! Generated assets live under `examples_assets/advanced/` and are embedded with `include_bytes!`;
//! each is served at `examples/advanced/<path>` in the plugin's folder. `README.md` indexes them.
//! `tests/examples_advanced.rs` loads and compiles every graph with the real compiler.

use engine_fs::EmbeddedFile;

macro_rules! advanced {
    ($($path:literal),* $(,)?) => {
        &[$(EmbeddedFile {
            path: concat!("examples/advanced/", $path),
            bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/examples_assets/advanced/", $path)),
        }),*]
    };
}

/// The advanced example files, at their path inside the plugin's folder.
pub(crate) const FILES: &[EmbeddedFile] = advanced![
    "README.md",
    "traits/AdvInteractable.trait.json",
    "AdvActorLifecycle.class/graph_save.json",
    "AdvStateMachine.class/graph_save.json",
    "AdvInteractableDoor.class/graph_save.json",
    "AdvInteractableLamp.class/graph_save.json",
    "AdvEventDispatchers.class/graph_save.json",
    "AdvEnemyBase.class/graph_save.json",
    "AdvEnemyBoss.class/graph_save.json",
    "AdvAsyncFlows.class/graph_save.json",
    "AdvDataDriven.class/graph_save.json",
    "AdvPlayerStats.class/graph_save.json",
    "AdvPickup.class/graph_save.json",
    "AdvHazard.class/graph_save.json",
    "AdvMacroLibrary.class/graph_save.json",
];
