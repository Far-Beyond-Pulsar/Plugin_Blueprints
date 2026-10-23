//! Editor profiles: reuse the graph editor for another kind of graph.
//!
//! The Blueprint editor is a node-graph canvas with a palette, wires, undo,
//! selection and a property panel. A *profile* lets another plugin keep all of
//! that and replace what is specific to Blueprints: the node catalogue, how
//! pins are drawn, the file format, and what "compile" means. The side panels
//! for variables, macros, events, traits and prefabs are left out.
//!
//! Profiles are installed per library; an editor opened on a file takes the
//! profile claiming its extension, so several profiles and the Blueprint editor
//! can share one binary.

use std::sync::{Arc, Mutex, RwLock};

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
    /// Stable id of this profile.
    fn id(&self) -> &str;

    /// File extensions (without the dot) whose documents this profile edits.
    fn extensions(&self) -> Vec<String>;

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

/// Every profile installed in this library, in installation order.
static PROFILES: RwLock<Vec<Arc<dyn EditorProfile>>> = RwLock::new(Vec::new());

/// Registers a profile. Several can coexist (and coexist with the Blueprint
/// editor in one binary): an editor opened on a file takes the profile that
/// claims the file's extension, see [`profile_for_path`]. Fails if a profile
/// with the same id, or one claiming the same extension, is already installed.
pub fn install_profile(profile: Arc<dyn EditorProfile>) -> Result<(), String> {
    let mut profiles = PROFILES.write().unwrap_or_else(|poisoned| poisoned.into_inner());
    if profiles.iter().any(|existing| existing.id() == profile.id()) {
        return Err(format!("an editor profile `{}` is already installed", profile.id()));
    }
    for extension in profile.extensions() {
        if profiles.iter().any(|p| p.extensions().iter().any(|e| e.eq_ignore_ascii_case(&extension))) {
            return Err(format!("another editor profile already claims `.{extension}`"));
        }
    }
    profiles.push(profile);
    Ok(())
}

/// The profile that claims the extension of `path`, if any. Files no profile
/// claims are Blueprints.
pub fn profile_for_path(path: &std::path::Path) -> Option<Arc<dyn EditorProfile>> {
    let extension = path.extension()?.to_str()?;
    PROFILES
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .find(|profile| profile.extensions().iter().any(|e| e.eq_ignore_ascii_case(extension)))
        .cloned()
}

/// The node definitions of `profile`, built once and kept for the process.
pub fn catalog_of(profile: &Arc<dyn EditorProfile>) -> &'static NodeDefinitions {
    static CATALOGS: Mutex<Vec<(String, &'static NodeDefinitions)>> = Mutex::new(Vec::new());
    let mut catalogs = CATALOGS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, catalog)) = catalogs.iter().find(|(id, _)| id == profile.id()) {
        return catalog;
    }
    let catalog: &'static NodeDefinitions = Box::leak(Box::new(profile.catalog()));
    catalogs.push((profile.id().to_string(), catalog));
    catalog
}

/// The definitions an editor with `profile` offers: the profile's, or the
/// Blueprint node catalogue.
pub(crate) fn definitions_for(profile: Option<&Arc<dyn EditorProfile>>) -> &'static NodeDefinitions {
    profile.map_or_else(NodeDefinitions::load, catalog_of)
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
