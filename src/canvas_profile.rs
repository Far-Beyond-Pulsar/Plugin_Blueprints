//! Blueprint's `graphy-gpui` profile: what the shared canvas needs to know
//! about Blueprint that is not generic graph editing.
//!
//! The node catalogue, pin colours (reflection-backed), type compatibility,
//! reroute type inference, subgraph navigation and node menus live here. The
//! canvas owns the document, layout, drawing, hit-testing and input; this
//! profile only answers questions about *Blueprint* meaning.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use graphy::{
    Compatibility, DataType, GraphDescription, NodeInstance, Pin, PinInstance, PinType, Position,
    TypeResolver,
};
use graphy_blueprint::{LibraryManager, SubGraph};
use graphy_gpui::{
    Action, CatalogEntry, GraphProfile, MacroTarget, MenuItem, MenuTarget, PinUpdate,
};
use graphy_render::{NodeShape, PinSide};
use graphy_render::presenter::{
    category_accent, CommentPresentation, NodePresentation, PinShape, PinStyle, Presenter, WireStyle,
};
use graphy_render::WireRouting;
use graphy_render::geom::rgba;

use crate::core::definitions::{NodeDefinition, NodeDefinitions};
use crate::core::subgraph_ref::SubGraphReference;
use crate::core::types::PinDataType;

/// Menu item ids this profile adds.
pub const MENU_OPEN_GRAPH: &str = "blueprint.open_graph";

fn parse_hex(hex: &str) -> Option<[f32; 4]> {
    let hex = hex.trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some(rgba(byte(0)?, byte(2)?, byte(4)?, 1.0)),
        8 => Some(rgba(byte(0)?, byte(2)?, byte(4)?, f32::from(byte(6)?) / 255.0)),
        _ => None,
    }
}

/// What the profile reads from the editor that owns the asset: subgraph
/// definitions (for call-node titles), the library, custom event names and the
/// nodes currently executing. The editor writes it; the profile reads it.
#[derive(Default)]
pub struct ProfileData {
    pub subgraphs: Vec<SubGraph>,
    pub library: LibraryManager,
    /// Custom event uid -> display name.
    pub event_names: HashMap<String, String>,
    pub running: HashSet<String>,
    /// Draw wires as right-angle (circuit-style) routes.
    pub angular_wires: bool,
}

pub type SharedProfileData = Arc<RwLock<ProfileData>>;

/// Blueprint's pin type judgement, on the canvas document's pin types.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlueprintResolver;

impl TypeResolver for BlueprintResolver {
    fn compatibility(&self, from: &DataType, to: &DataType) -> Compatibility {
        match (from, to) {
            (DataType::Exec, DataType::Exec) => Compatibility::Exact,
            (DataType::Exec, _) | (_, DataType::Exec) => {
                Compatibility::Incompatible("execution pins only connect to execution pins".into())
            }
            (DataType::Data(a), DataType::Data(b)) => {
                let (a, b) = (
                    PinDataType::from_type_str(a.type_string.clone()),
                    PinDataType::from_type_str(b.type_string.clone()),
                );
                if a.is_wildcard() || b.is_wildcard() {
                    Compatibility::Wildcard
                } else if a.is_compatible_with(&b) {
                    Compatibility::Exact
                } else {
                    Compatibility::Incompatible(format!(
                        "{} cannot connect to {}",
                        a.type_name, b.type_name
                    ))
                }
            }
        }
    }
}

/// Presentation of Blueprint nodes and pins.
pub struct BlueprintPresenter {
    definitions: &'static NodeDefinitions,
    data: SharedProfileData,
    /// A foreign graph kind's profile, which may title nodes from their properties.
    editor_profile: Option<Arc<dyn crate::profile::EditorProfile>>,
}

fn title_case(s: &str) -> String {
    s.replace('-', " ")
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl BlueprintPresenter {
    fn definition(&self, node_type: &str) -> Option<&'static NodeDefinition> {
        self.definitions.get_node_definition(node_type)
    }

    /// Title of a macro/collapsed call node.
    fn subgraph_title(&self, node_type: &str) -> Option<String> {
        let data = self.data.read().unwrap_or_else(|e| e.into_inner());
        let reference = SubGraphReference::decode(node_type, &data.subgraphs)?;
        let name = data
            .subgraphs
            .iter()
            .find(|m| m.id == reference.id && m.kind == reference.kind)
            .or_else(|| {
                data.library
                    .get_subgraph(&reference.id)
                    .filter(|s| s.kind == reference.kind)
            })
            .map(|m| m.name.clone())
            .unwrap_or_else(|| title_case(&reference.id));
        Some(name)
    }

    fn event_title(&self, uid: &str) -> String {
        let data = self.data.read().unwrap_or_else(|e| e.into_inner());
        data.event_names
            .get(uid)
            .cloned()
            .unwrap_or_else(|| title_case(uid))
    }
}

impl Presenter for BlueprintPresenter {
    fn node(&self, node: &NodeInstance) -> NodePresentation {
        let ty = node.node_type.as_str();
        let mut p = if node.is_reroute() {
            let mut p = NodePresentation::card("", rgba(0x8b, 0x8b, 0x8b, 1.0));
            p.shape = NodeShape::Reroute;
            p
        } else if let Some(conversion) = ty
            .strip_prefix("conversion:")
            .and_then(|id| pulsar_reflection::CONVERSION_REGISTRY.get(id))
        {
            let mut p = NodePresentation::card(conversion.label, rgba(0x60, 0xa5, 0xfa, 1.0));
            p.shape = NodeShape::Pill;
            p
        } else if ty.starts_with("conversion:") {
            let mut p = NodePresentation::card("Conversion", rgba(0x60, 0xa5, 0xfa, 1.0));
            p.shape = NodeShape::Pill;
            p
        } else if ty == "macro_entry" || ty == "macro_exit" {
            let title = if ty == "macro_entry" { "Subgraph Entry" } else { "Subgraph Exit" };
            let mut p = NodePresentation::card(title, rgba(0x7c, 0x3a, 0xed, 1.0));
            p.icon = Some(if ty == "macro_entry" { '▶' } else { '◀' });
            p
        } else if let Some(uid) = ty.strip_prefix("custom_event_dispatch:") {
            let mut p = NodePresentation::card(
                format!("Dispatch {}", self.event_title(uid)),
                rgba(0x00, 0xa8, 0xe8, 1.0),
            );
            p.icon = Some('📡');
            p
        } else if let Some(uid) = ty.strip_prefix("custom_event:") {
            let mut p = NodePresentation::card(
                format!("On {}", self.event_title(uid)),
                rgba(0xc0, 0x39, 0x2b, 1.0),
            );
            p.icon = Some('📡');
            p
        } else if let Some(title) = self.subgraph_title(ty) {
            let mut p = NodePresentation::card(title, rgba(0x9b, 0x59, 0xb6, 1.0));
            p.icon = Some('📦');
            p
        } else if let Some((title, is_event)) =
            crate::features::events::engine_events::describe_node_type(ty)
        {
            let accent = if is_event {
                rgba(0xc0, 0x39, 0x2b, 1.0)
            } else {
                rgba(0x00, 0xa8, 0xe8, 1.0)
            };
            let mut p = NodePresentation::card(title, accent);
            p.icon = Some(if is_event { '📡' } else { '📨' });
            p
        } else if let Some(def) = self.definition(ty) {
            let accent = def
                .color
                .as_deref()
                .and_then(parse_hex)
                .or_else(|| {
                    self.definitions
                        .get_category_for_node(&def.id)
                        .and_then(|c| parse_hex(&c.color))
                })
                .unwrap_or_else(|| category_accent(&def.id));
            let mut p = NodePresentation::card(def.name.clone(), accent);
            p.icon = def.icon.chars().next();
            p
        } else {
            NodePresentation::card(ty.replace('_', " "), category_accent(ty))
        };
        if let Some(title) = self.editor_profile.as_ref().and_then(|profile| {
            profile.node_title(ty, &crate::canvas_profile::properties_as_text(node))
        }) {
            p.title = title;
        }
        // A `__return__` output is drawn in the header instead of a body row.
        p.header_output = node.outputs.iter().position(|o| o.id == "__return__");
        p.active = self
            .data
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .running
            .contains(&node.id);
        p
    }

    fn wire(&self, conn: &graphy::Connection, from: &PinStyle) -> WireStyle {
        let angular = self
            .data
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .angular_wires;
        WireStyle {
            color: from.color,
            routing: if angular { WireRouting::Angular } else { WireRouting::Bezier },
            exec: conn.connection_type == graphy::ConnectionType::Execution,
        }
    }

    fn comment(&self, _index: usize, comment: &graphy::core::GraphComment) -> CommentPresentation {
        CommentPresentation {
            accent: comment.color,
        }
    }

    fn pin(&self, _node: &NodeInstance, pin: &PinInstance, _side: PinSide) -> PinStyle {
        match pin.data_type() {
            DataType::Exec => PinStyle {
                shape: PinShape::Arrow,
                ringed: false,
                color: rgba(0xe4, 0xe4, 0xe7, 1.0),
            },
            DataType::Data(info) => {
                let ty = PinDataType::from_type_str(info.type_string.clone());
                PinStyle {
                    shape: if ty.is_wildcard() {
                        PinShape::Diamond
                    } else {
                        PinShape::Circle
                    },
                    ringed: info.type_string.starts_with("Vec<") || info.type_string.starts_with('['),
                    color: ty.display_color(),
                }
            }
        }
    }

    fn pin_label(&self, _node: &NodeInstance, pin: &PinInstance, _side: PinSide) -> String {
        pin.pin.name.clone()
    }
}

/// Blueprint's graph profile. Create one per open asset: the subgraph list and
/// the running-node set are shared with the editor that owns the asset.
pub struct BlueprintProfile {
    id: String,
    presenter: BlueprintPresenter,
    resolver: BlueprintResolver,
}

impl BlueprintProfile {
    pub fn new(
        definitions: &'static NodeDefinitions,
        data: SharedProfileData,
        editor_profile: Option<Arc<dyn crate::profile::EditorProfile>>,
    ) -> Self {
        Self {
            id: "blueprint".to_string(),
            presenter: BlueprintPresenter {
                definitions,
                data,
                editor_profile,
            },
            resolver: BlueprintResolver,
        }
    }

    /// The profile for a Blueprint asset, over the Blueprint node catalogue.
    pub fn blueprint(data: SharedProfileData) -> Self {
        Self::new(NodeDefinitions::load(), data, None)
    }

    fn instantiate(&self, def: &NodeDefinition, id: &str, pos: Position) -> NodeInstance {
        let mut node = NodeInstance::new(id, def.id.as_str(), pos);
        let pin = |p: &crate::core::definitions::PinDefinition, kind: PinType| PinInstance {
            id: p.id.clone(),
            pin: Pin {
                id: p.id.clone(),
                name: p.name.clone(),
                data_type: if p.data_type.is_execution() {
                    DataType::Exec
                } else {
                    DataType::typed(p.data_type.type_name.clone())
                },
                pin_type: kind,
            },
        };
        node.inputs = def.inputs.iter().map(|p| pin(p, PinType::Input)).collect();
        node.outputs = def.outputs.iter().map(|p| pin(p, PinType::Output)).collect();
        for (k, v) in &def.properties {
            node.set_property(k.clone(), blueprint_compiler::authored::property_value_from_raw(v));
        }
        node
    }

    /// The type a reroute should have, from what it is connected to.
    fn inferred_reroute_type(graph: &GraphDescription, node: &str) -> Option<DataType> {
        for c in &graph.connections {
            if c.source_node == node {
                let pin = graph
                    .nodes
                    .get(&c.target_node)?
                    .inputs
                    .iter()
                    .find(|p| p.id == c.target_pin)?;
                return Some(pin.data_type().clone());
            }
            if c.target_node == node {
                let pin = graph
                    .nodes
                    .get(&c.source_node)?
                    .outputs
                    .iter()
                    .find(|p| p.id == c.source_pin)?;
                return Some(pin.data_type().clone());
            }
        }
        None
    }
}

impl GraphProfile for BlueprintProfile {
    fn id(&self) -> &str {
        &self.id
    }

    fn presenter(&self) -> &dyn Presenter {
        &self.presenter
    }

    fn type_resolver(&self) -> &dyn TypeResolver {
        &self.resolver
    }

    fn catalog(&self) -> Vec<CatalogEntry> {
        self.presenter
            .definitions
            .categories
            .iter()
            .flat_map(|category| {
                category.nodes.iter().map(|n| CatalogEntry {
                    definition: n.id.clone(),
                    title: n.name.clone(),
                    category: category.name.clone(),
                    keywords: vec![n.description.clone()],
                })
            })
            .collect()
    }

    fn create_node(&self, definition: &str, id: &str, pos: Position) -> Option<NodeInstance> {
        if let Some(conversion) = definition
            .strip_prefix("conversion:")
            .and_then(|c| pulsar_reflection::CONVERSION_REGISTRY.get(c))
        {
            let mut node = NodeInstance::new(id, definition, pos);
            node.inputs.push(PinInstance {
                id: "from".into(),
                pin: Pin {
                    id: "from".into(),
                    name: String::new(),
                    data_type: DataType::typed(conversion.source_type_name),
                    pin_type: PinType::Input,
                },
            });
            node.outputs.push(PinInstance {
                id: "into".into(),
                pin: Pin {
                    id: "into".into(),
                    name: String::new(),
                    data_type: DataType::typed(conversion.target_type_name),
                    pin_type: PinType::Output,
                },
            });
            node.set_property("conversion_id", conversion.id);
            return Some(node);
        }
        let def = self.presenter.definitions.get_node_definition(definition)?;
        Some(self.instantiate(def, id, pos))
    }

    fn create_reroute(&self, id: &str, pos: Position, data_type: &DataType) -> NodeInstance {
        let mut n = NodeInstance::new(id, "reroute", pos);
        for (pin, kind) in [("input", PinType::Input), ("output", PinType::Output)] {
            let instance = PinInstance {
                id: pin.into(),
                pin: Pin {
                    id: pin.into(),
                    name: String::new(),
                    data_type: data_type.clone(),
                    pin_type: kind,
                },
            };
            match kind {
                PinType::Input => n.inputs.push(instance),
                PinType::Output => n.outputs.push(instance),
            }
        }
        n
    }

    fn refresh_pins(&self, graph: &GraphDescription, node: &str) -> Option<PinUpdate> {
        let n = graph.nodes.get(node)?;
        if !n.is_reroute() {
            return None;
        }
        let ty = Self::inferred_reroute_type(graph, node)?;
        let retype = |pins: &[PinInstance]| {
            pins.iter()
                .map(|p| {
                    let mut p = p.clone();
                    p.pin.data_type = ty.clone();
                    p
                })
                .collect()
        };
        Some(PinUpdate {
            inputs: retype(&n.inputs),
            outputs: retype(&n.outputs),
        })
    }

    fn is_deletable(&self, node: &NodeInstance) -> bool {
        !matches!(node.node_type.as_str(), "macro_entry" | "macro_exit")
    }

    fn macro_target(&self, node: &NodeInstance) -> Option<MacroTarget> {
        let data = self.presenter.data.read().unwrap_or_else(|e| e.into_inner());
        SubGraphReference::decode(&node.node_type, &data.subgraphs).map(|r| MacroTarget { graph_id: r.id })
    }

    fn menu_items(&self, graph: &GraphDescription, target: &MenuTarget) -> Vec<MenuItem> {
        match target {
            MenuTarget::Node(id) => graph
                .nodes
                .get(id)
                .and_then(|n| self.macro_target(n))
                .map(|_| vec![MenuItem::new(MENU_OPEN_GRAPH, "Open Graph").separated()])
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn run_menu_item(
        &self,
        graph: &GraphDescription,
        target: &MenuTarget,
        item: &str,
    ) -> Vec<Action> {
        match (target, item) {
            (MenuTarget::Node(id), MENU_OPEN_GRAPH) => graph
                .nodes
                .get(id)
                .and_then(|n| self.macro_target(n))
                .map(|t| vec![Action::OpenGraph(t)])
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
}

impl graphy_gpui::canvas::GpuiProfile for BlueprintProfile {}

/// Fonts for the canvas text: a readable system font, with a compiled-in
/// fallback so the editor works on unusual setups.
pub fn canvas_fonts() -> graphy_render_wgpu::FontSet {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/Helvetica.ttc",
            "/Library/Fonts/Arial.ttf",
            "/System/Library/Fonts/SFNSText.ttf",
        ]
    } else if cfg!(target_os = "windows") {
        &[
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\arial.ttf",
            r"C:\Windows\Fonts\calibri.ttf",
        ]
    } else {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        ]
    };
    let regular = candidates
        .iter()
        .find_map(|p| std::fs::read(p).ok())
        .unwrap_or_else(|| include_bytes!("../assets/fallback_font.ttf").to_vec());
    graphy_render_wgpu::FontSet {
        regular,
        bold: None,
    }
}

/// A pin type from a document type string (`execution` or a Rust type).
pub fn datatype_from_str(s: &str) -> DataType {
    match graphy_blueprint::DataType::from_type_str(s) {
        graphy_blueprint::DataType::Execution => DataType::Exec,
        graphy_blueprint::DataType::Data(info) => DataType::typed(info.to_string()),
    }
}

/// A node's properties as the text editor profiles read them.
pub fn properties_as_text(node: &NodeInstance) -> HashMap<String, String> {
    node.properties
        .iter()
        .map(|(k, v)| (k.clone(), crate::io::canvas_doc::property_string(v)))
        .collect()
}
