//! Editor profiles: reuse the graph editor for another kind of graph.
//!
//! The Blueprint editor is a node-graph canvas with a palette, wires, undo,
//! selection and a property panel. A *profile* lets another plugin keep all of
//! that and replace what is specific to Blueprints: the node catalogue, how
//! pins are drawn, the file format, and what "compile" means. The side panels
//! for variables, macros, events, traits and prefabs are left out.
//!
//! A profile is installed once per library, before the first editor opens.
//! Plugins link their own copy of this crate, so a profile installed by one
//! plugin is not seen by the Blueprint editor or by another plugin.

use std::sync::{Arc, OnceLock};

use crate::core::definitions::NodeDefinitions;
use crate::core::graph::BlueprintGraph;
use crate::io::formats::BlueprintAsset;

/// Outline of a pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinShape {
    Circle,
    /// Right-pointing arrow, drawn for execution pins.
    Arrow,
    Diamond,
    Square,
    Hexagon,
}

impl PinShape {
    /// The `kind` the pin shader switches on.
    pub(crate) fn shader_kind(self) -> u32 {
        match self {
            PinShape::Circle => 0,
            PinShape::Arrow => 1,
            PinShape::Diamond => 3,
            PinShape::Square => 4,
            PinShape::Hexagon => 5,
        }
    }
}

/// How one pin type is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinStyle {
    pub shape: PinShape,
    /// A second outline, for data that outlives the frame.
    pub ringed: bool,
    /// RGBA fill; `None` keeps the type's default colour.
    pub color: Option<[f32; 4]>,
}

/// What a profile replaces. Every method with a default may be left alone.
pub trait EditorProfile: Send + Sync + 'static {
    /// The node palette. Called once, the first time definitions are needed.
    fn catalog(&self) -> NodeDefinitions;

    /// Reads a saved document into the editor's graph model.
    fn decode(&self, text: &str) -> Result<BlueprintAsset, String>;

    /// Writes the editor's graph model as a document.
    fn encode(&self, asset: &BlueprintAsset) -> Result<String, String>;

    /// How a pin whose type is named `type_name` is drawn.
    fn pin_style(&self, type_name: &str) -> PinStyle;

    /// Checks the graph; each entry is one problem, errors first. This is what
    /// the toolbar's compile button runs. Empty means the graph is sound.
    fn validate(&self, graph: &BlueprintGraph) -> Vec<String>;

    /// Whether the variables, macros, events, traits and prefab panels are
    /// shown. A profile that has none of those leaves this `false`.
    fn side_panels(&self) -> bool {
        false
    }

    /// The editable parameters of the node type `definition_id`, in display
    /// order. Nodes of other types show their properties read-only.
    fn parameters(&self, _definition_id: &str) -> Vec<ParamField> {
        Vec::new()
    }

    /// Called after a parameter of node `node_id` was edited, so the profile can bring
    /// the graph in line: pins named after a resource change, and the wires on
    /// them must follow.
    fn parameter_changed(&self, _graph: &mut BlueprintGraph, _node_id: &str) {}
}

static PROFILE: OnceLock<Arc<dyn EditorProfile>> = OnceLock::new();

/// Installs the profile for this library. Fails if one is already installed or
/// the node definitions were already loaded without it.
pub fn install_profile(profile: Arc<dyn EditorProfile>) -> Result<(), &'static str> {
    if NodeDefinitions::is_loaded() {
        return Err("node definitions were already loaded; install the profile before opening an editor");
    }
    PROFILE.set(profile).map_err(|_| "an editor profile is already installed")
}

/// The installed profile, if this library was set up with one.
pub fn profile() -> Option<&'static Arc<dyn EditorProfile>> {
    PROFILE.get()
}

/// How a parameter is edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamKind {
    /// Free text; numbers are edited as text too.
    Text,
    Bool,
    /// One of a fixed set.
    Choice(Vec<String>),
}

/// An editable node parameter, stored as text in the node's properties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamField {
    pub key: String,
    pub label: String,
    pub kind: ParamKind,
}
