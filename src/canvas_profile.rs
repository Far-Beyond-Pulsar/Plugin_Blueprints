//! Blueprint's `graphy-gpui` profile: what the shared canvas needs to know
//! about Blueprint that is not generic graph editing.
//!
//! The node catalogue, pin colours (reflection-backed), type compatibility,
//! reroute type inference, subgraph navigation and node menus live here. The
//! canvas owns the document, layout, drawing, hit-testing and input; this
//! profile only answers questions about *Blueprint* meaning.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use graphy::{
    Compatibility, DataType, GraphDescription, NodeInstance, Pin, PinInstance, PinType, Position,
    TypeResolver,
};
use graphy_blueprint::SubGraph;
use graphy_gpui::{
    Action, CatalogEntry, GraphProfile, MacroTarget, MenuItem, MenuTarget, PinUpdate,
};
use graphy_render::{NodeShape, PinSide};
use graphy_render::presenter::{category_accent, prettify, NodePresentation, PinShape, PinStyle, Presenter};
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
    subgraphs: Arc<RwLock<Vec<SubGraph>>>,
    running: Arc<RwLock<HashSet<String>>>,
}

impl BlueprintPresenter {
    fn definition(&self, node_type: &str) -> Option<&'static NodeDefinition> {
        self.definitions.get_node_definition(node_type)
    }

    fn subgraph_of(&self, node_type: &str) -> Option<SubGraph> {
        let subgraphs = self.subgraphs.read().unwrap_or_else(|e| e.into_inner());
        let reference = SubGraphReference::decode(node_type, &subgraphs)?;
        subgraphs.iter().find(|s| s.id == reference.id).cloned()
    }
}

impl Presenter for BlueprintPresenter {
    fn node(&self, node: &NodeInstance) -> NodePresentation {
        let mut p = if node.is_reroute() {
            let mut p = NodePresentation::card("", rgba(0x8b, 0x8b, 0x8b, 1.0));
            p.shape = NodeShape::Reroute;
            p
        } else if let Some(conversion) = node
            .node_type
            .strip_prefix("conversion:")
            .and_then(|id| pulsar_reflection::CONVERSION_REGISTRY.get(id))
        {
            let mut p = NodePresentation::card(conversion.label, rgba(0x60, 0xa5, 0xfa, 1.0));
            p.shape = NodeShape::Pill;
            p
        } else if let Some(def) = self.definition(&node.node_type) {
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
        } else if let Some(sub) = self.subgraph_of(&node.node_type) {
            NodePresentation::card(sub.name, rgba(0xc0, 0x84, 0xfc, 1.0))
        } else {
            match node.node_type.as_str() {
                "macro_entry" => NodePresentation::card("Subgraph Entry", rgba(0x4a, 0xde, 0x80, 1.0)),
                "macro_exit" => NodePresentation::card("Subgraph Exit", rgba(0xf4, 0x72, 0xb6, 1.0)),
                other => NodePresentation::card(
                    prettify(other),
                    category_accent(other),
                ),
            }
        };
        p.active = self
            .running
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&node.id);
        p
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
        subgraphs: Arc<RwLock<Vec<SubGraph>>>,
        running: Arc<RwLock<HashSet<String>>>,
    ) -> Self {
        Self {
            id: "blueprint".to_string(),
            presenter: BlueprintPresenter {
                definitions,
                subgraphs,
                running,
            },
            resolver: BlueprintResolver,
        }
    }

    /// The profile for a Blueprint asset, over the Blueprint node catalogue.
    pub fn blueprint(
        subgraphs: Arc<RwLock<Vec<SubGraph>>>,
        running: Arc<RwLock<HashSet<String>>>,
    ) -> Self {
        Self::new(NodeDefinitions::load(), subgraphs, running)
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

    fn macro_target(&self, node: &NodeInstance) -> Option<MacroTarget> {
        let subgraphs = self
            .presenter
            .subgraphs
            .read()
            .unwrap_or_else(|e| e.into_inner());
        SubGraphReference::decode(&node.node_type, &subgraphs).map(|r| MacroTarget { graph_id: r.id })
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
