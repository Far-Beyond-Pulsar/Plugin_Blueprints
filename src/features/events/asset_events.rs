//! Events a class's components get from the assets they name.
//!
//! A component that names an asset in a property (`AssetRef<K>`) raises the
//! events that asset kind's source declares: an animator, the notifies of its
//! graph; a player input component, the actions of its mapping. Which
//! properties are asset references comes from reflection, who answers for a
//! kind from the plugin registry (`engine_backend::AssetEventSource`); this
//! module only reads the class's prefab, asks, and draws what comes back.
//! It names no component class, no property and no asset kind.
//!
//! The answers are cached by the host (`engine_backend::EventCache`), so
//! calling this on every palette rebuild and every compile does no file
//! access once a catalog has been read.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use blueprint_compiler::palette::PaletteEvent;
use engine_backend::{component_events, AssetEvents, ComponentData};
use pulsar_class::prefab::PrefabAsset;

use super::engine_events::{pin, signature_of};
use crate::core::definitions::NodeDefinition;
use crate::core::types::PinType;

/// The events the components of `prefab` get from their assets. The prefab is
/// the in-memory one while its inspector has unsaved edits.
pub fn of_prefab(class_path: Option<&Path>, prefab: &PrefabAsset) -> Vec<AssetEvents> {
    let Some(root) = class_path.and_then(crate::features::class_dirs::project_root_of) else {
        return Vec::new();
    };
    let components: Vec<ComponentData<'_>> = prefab
        .components
        .iter()
        .map(|component| ComponentData {
            slot_id: &component.slot_id,
            class_name: &component.class_name,
            enabled: component.enabled,
            data: &component.data,
        })
        .collect();
    let sets = component_events(&root, &components);
    for set in &sets {
        for (path, error) in &set.catalog.diagnostics {
            tracing::warn!(path = %path.display(), %error,
                "could not discover a component's asset events");
        }
    }
    sets
}

/// The events of the class at `class_path`, from its saved prefab. A read path:
/// it never rewrites `prefab.json`.
pub fn of_class(class_path: Option<&Path>) -> Vec<AssetEvents> {
    let Some(path) = class_path else {
        return Vec::new();
    };
    match PrefabAsset::load_from_dir(path) {
        Ok(prefab) => of_prefab(class_path, &prefab),
        Err(error) => {
            tracing::warn!("Could not read the class prefab for asset events: {error}");
            Vec::new()
        }
    }
}

/// What the compiler checks event nodes against.
pub fn palette_events(sets: &[AssetEvents]) -> Vec<PaletteEvent> {
    sets.iter()
        .flat_map(|set| {
            let family_events = set.catalog.families.iter().flat_map(|family| {
                family
                    .events
                    .iter()
                    .map(move |descriptor| (descriptor, family.group.clone()))
            });
            set.catalog
                .events
                .iter()
                .map(|event| (&event.descriptor, event.view.group.clone()))
                .chain(family_events)
        })
        .filter_map(|(descriptor, category)| {
            Some(PaletteEvent {
                signature: signature_of(descriptor)?,
                category,
                declared_here: false,
            })
        })
        .collect()
}

/// Entry points for the events the assets declare. Each is bound to one
/// component reference (the `component_ref` input): it is delivered on that
/// component's actor channel, never as a global broadcast. A class with two
/// components of one class offers each event once; the reference chooses which.
pub fn node_definitions(sets: &[AssetEvents]) -> Vec<(String, NodeDefinition)> {
    let mut seen = HashSet::new();
    let mut nodes = Vec::new();
    for set in sets {
        let class = &set.used.class_name;
        for event in &set.catalog.events {
            let Some(signature) = signature_of(&event.descriptor) else {
                continue;
            };
            let id = format!("event::on_component::{class}.{}", signature.name);
            if !seen.insert(id.clone()) {
                continue;
            }
            let mut outputs = vec![pin("Body", "execution", PinType::Output)];
            outputs.extend(signature.fields.iter().map(|field| {
                pin(
                    &field.name,
                    &blueprint_compiler::palette::pin_type_name(&field.ty),
                    PinType::Output,
                )
            }));
            let mut properties =
                std::collections::HashMap::from([("component_type".to_owned(), class.clone())]);
            properties.extend(event.view.properties.iter().cloned());
            nodes.push((
                format!("Events/Components/{class}/{}", event.view.group),
                NodeDefinition {
                    id,
                    name: format!("On {}", event.label),
                    icon: "📡".to_owned(),
                    description: event.view.description.clone(),
                    documentation: event.view.documentation.clone(),
                    inputs: vec![pin("component_ref", class, PinType::Input)],
                    outputs,
                    properties,
                    color: Some("#C0392B".to_owned()),
                    is_event: true,
                },
            ));
        }
    }
    nodes.sort_by(|a, b| (&a.0, &a.1.name).cmp(&(&b.0, &b.1.name)));
    nodes
}

// ---- families ----------------------------------------------------------------

use engine_backend::{EventFamily, ParamValues, ResolvedFamily};

/// Every family the class's components get, by family id.
pub fn families(sets: &[AssetEvents]) -> std::collections::HashMap<String, EventFamily> {
    sets.iter()
        .flat_map(|set| set.catalog.families.iter())
        .map(|family| (family.id.clone(), family.clone()))
        .collect()
}

/// The title of a family node loaded from a file: the `title` it was saved with,
/// else what its saved `events` say it listens to (`input::IA_Move.started` is
/// "On IA_Move"), else its family's name. A node must read well before (and
/// without) the catalog that made it is known, and a file saved by an older build
/// has no `title`.
pub fn family_node_title(properties: &HashMap<String, serde_json::Value>) -> String {
    let text = |key: &str| {
        properties
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    if let Some(title) = text("title") {
        return title.to_owned();
    }
    // `Pin=namespace::name.phase,...`: the first event's name, before its phase.
    let from_events = text("events")
        .and_then(|events| events.split(',').next())
        .and_then(|pair| pair.split_once('=').map(|(_, event)| event))
        .and_then(|event| event.rsplit("::").next())
        .and_then(|name| name.rsplit_once('.').map(|(action, _)| action))
        .filter(|action| !action.is_empty());
    match from_events {
        Some(action) => format!("On {action}"),
        None => "On Event".to_owned(),
    }
}

/// Brings every family node of `graph` in line with the live `families`: its
/// title, snapshot (`events`, `lowering`) and pins, as when a parameter is edited.
/// A node whose family is not offered (the component is gone) or whose choice no
/// longer resolves is left exactly as it was saved. Returns how many nodes
/// changed. A graph opened from disk, and a compile, both run this, so what is
/// shown and what is compiled follow the mapping as it is now, not as it was
/// when the node was placed.
pub fn reconcile_family_nodes(
    graph: &mut graphy::GraphDescription,
    families: &HashMap<String, EventFamily>,
) -> usize {
    let ids: Vec<String> = graph.nodes.keys().cloned().collect();
    let mut changed = 0;
    for id in ids {
        let Some(node) = graph.nodes.get(&id) else {
            continue;
        };
        let Some(family) = family_of_node_type(&node.node_type).and_then(|f| families.get(f))
        else {
            continue;
        };
        let commands = crate::canvas_profile::family_commands(family, node);
        if commands.is_empty() {
            continue;
        }
        for command in commands {
            let _ = command.apply(graph);
        }
        changed += 1;
    }
    changed
}

/// The node type of a family.
pub fn family_node_type(family_id: &str) -> String {
    format!("{FAMILY_PREFIX}{family_id}")
}

/// What a node type says about its family: `Some(id)` for a family node.
pub fn family_of_node_type(node_type: &str) -> Option<&str> {
    // `event::family::<family>` or, for a node bound to one choice,
    // `event::family::<family>:<choice>`.
    node_type
        .strip_prefix(FAMILY_PREFIX)
        .map(|rest| rest.split(':').next().unwrap_or(rest))
}

const FAMILY_PREFIX: &str = "event::family::";

/// The properties a family node carries: its parameter values (what the user
/// chose), and the snapshot of what they resolved to that the compiler reads
/// (`events`: `pin=event` pairs; `lowering`: how each data pin is built). The
/// snapshot makes a saved graph self-describing: it compiles, and keeps its
/// pins and wires, even when the asset it named has changed.
pub fn family_properties(
    family: &EventFamily,
    values: &ParamValues,
    resolved: &ResolvedFamily,
    class: &str,
) -> std::collections::HashMap<String, String> {
    let mut properties: std::collections::HashMap<String, String> = values
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    properties.insert(
        "events".to_owned(),
        resolved
            .arms
            .iter()
            .map(|(pin, event)| format!("{pin}={event}"))
            .collect::<Vec<_>>()
            .join(","),
    );
    properties.insert(
        "lowering".to_owned(),
        resolved
            .outputs
            .iter()
            .map(|out| format!("{}={}", out.pin, out.lowering))
            .collect::<Vec<_>>()
            .join(";"),
    );
    properties.insert("family".to_owned(), family.id.clone());
    properties.insert("title".to_owned(), family_title(family, values));
    properties.insert("component_type".to_owned(), class.to_owned());
    // Only this object's own player, as a character expects.
    properties.insert("scope".to_owned(), "self".to_owned());
    properties
}

/// The pins a resolved family has: an execution output per arm, then the
/// typed data outputs, as `(id, type)` pairs (`execution` for exec).
pub fn family_output_pins(resolved: &ResolvedFamily) -> Vec<(String, String)> {
    resolved
        .arms
        .iter()
        .map(|(pin, _)| (pin.clone(), "execution".to_owned()))
        .chain(
            resolved
                .outputs
                .iter()
                .map(|out| (out.pin.clone(), out.type_name.clone())),
        )
        .collect()
}

/// The parameter a family's node is bound to when the palette offers one node per
/// choice (an input action): its first `Choice`.
pub fn primary_choice(family: &EventFamily) -> Option<&engine_backend::FamilyParam> {
    family
        .params
        .iter()
        .find(|param| param.kind == engine_backend::ParamKind::Choice)
}

/// The node's title: "On <the chosen option>" when the family has a primary
/// choice (`On Move`), else "On <the family>".
pub fn family_title(family: &EventFamily, values: &ParamValues) -> String {
    let chosen = primary_choice(family).and_then(|param| {
        let id = values.get(&param.name)?;
        param.options.iter().find(|option| &option.id == id)
    });
    match chosen {
        Some(option) => format!("On {}", option.label),
        None => format!("On {}", family.label),
    }
}

/// A family's palette entries: one node per choice of its primary parameter
/// (every action the mapping binds, each with its own typed value pin), or one
/// node when it has none, per component class. Each starts bound to its choice
/// and the parameters' defaults; the phases to hear are chosen on the node.
pub fn family_node_definitions(sets: &[AssetEvents]) -> Vec<(String, NodeDefinition)> {
    let mut seen = HashSet::new();
    let mut nodes = Vec::new();
    for set in sets {
        let class = &set.used.class_name;
        for family in &set.catalog.families {
            if !seen.insert((class.clone(), family.id.clone())) {
                continue;
            }
            let primary = primary_choice(family);
            let bound: Vec<Option<&engine_backend::FamilyOption>> = match primary {
                Some(param) => param.options.iter().map(Some).collect(),
                None => vec![None],
            };
            for option in bound {
                let mut values = family.defaults();
                if let (Some(param), Some(option)) = (primary, option) {
                    values.insert(param.name.clone(), option.id.clone());
                }
                // A choice whose parameters do not resolve has nothing to start from.
                let Ok(resolved) = (family.resolve)(family, &values) else {
                    continue;
                };
                let outputs = family_output_pins(&resolved)
                    .into_iter()
                    .map(|(id, ty)| pin(&id, &ty, PinType::Output))
                    .collect();
                let (id, description) = match option {
                    Some(option) => (
                        format!("{}:{}", family_node_type(&family.id), option.id),
                        format!(
                            "{} ({}: {})",
                            family.description, family.label, option.detail
                        ),
                    ),
                    None => (family_node_type(&family.id), family.description.clone()),
                };
                nodes.push((
                    format!("Events/Components/{class}/{}", family.group),
                    NodeDefinition {
                        id,
                        name: family_title(family, &values),
                        icon: "📡".to_owned(),
                        description,
                        documentation: family.documentation.clone(),
                        // Dragged off a component reference like an asset-owned
                        // event; left unwired it listens on the object it is
                        // placed in.
                        inputs: vec![pin("component_ref", class, PinType::Input)],
                        outputs,
                        properties: family_properties(family, &values, &resolved, class),
                        color: Some("#C0392B".to_owned()),
                        is_event: true,
                    },
                ));
            }
        }
    }
    nodes.sort_by(|a, b| (&a.0, &a.1.name).cmp(&(&b.0, &b.1.name)));
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_backend::{AssetUse, CatalogEvent, ComponentEventCatalog, EventView};
    use plugin_editor_api::pulsar_events::gamma::{EventDescriptor, FieldType};
    use std::sync::Arc;

    fn set(class: &str, events: Vec<CatalogEvent>) -> AssetEvents {
        AssetEvents {
            used: AssetUse {
                slot_id: "slot".into(),
                class_name: class.into(),
                property: "asset",
                kind: "any",
                asset_id: "id".into(),
            },
            catalog: Arc::new(ComponentEventCatalog {
                events,
                families: Vec::new(),
                diagnostics: Vec::new(),
            }),
        }
    }

    fn event(label: &str, item: &str) -> CatalogEvent {
        CatalogEvent {
            label: label.into(),
            descriptor: EventDescriptor::dynamic_in(
                "thing",
                format!("a1.{item}"),
                [
                    ("entity".to_owned(), FieldType::U64),
                    ("value".to_owned(), FieldType::F64),
                ],
            )
            .unwrap(),
            view: EventView {
                group: "Group/a1".into(),
                description: "does a thing".into(),
                documentation: "docs".into(),
                properties: vec![("source".into(), "a1".into())],
            },
        }
    }

    #[test]
    fn a_catalog_event_becomes_a_component_bound_entry_point() {
        let nodes = node_definitions(&[set("Widget", vec![event("Poke", "poke")])]);
        assert_eq!(nodes.len(), 1);
        let (category, node) = &nodes[0];
        assert_eq!(category, "Events/Components/Widget/Group/a1");
        assert_eq!(node.id, "event::on_component::Widget.thing::a1.poke");
        assert_eq!(node.name, "On Poke");
        assert!(node.is_event);
        assert_eq!(node.inputs.len(), 1);
        assert_eq!(node.inputs[0].id, "component_ref");
        let outputs: Vec<_> = node.outputs.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(outputs, ["Body", "entity", "value"]);
        assert_eq!(
            node.properties.get("component_type").map(String::as_str),
            Some("Widget")
        );
        assert_eq!(
            node.properties.get("source").map(String::as_str),
            Some("a1")
        );
    }

    #[test]
    fn two_components_of_one_class_offer_each_event_once() {
        let nodes = node_definitions(&[
            set("Widget", vec![event("Poke", "poke")]),
            set("Widget", vec![event("Poke", "poke")]),
        ]);
        assert_eq!(nodes.len(), 1, "the component reference picks the instance");
    }

    #[test]
    fn the_same_event_of_two_classes_is_two_entries() {
        let nodes = node_definitions(&[
            set("Widget", vec![event("Poke", "poke")]),
            set("Gadget", vec![event("Poke", "poke")]),
        ]);
        assert_eq!(nodes.len(), 2);
    }

    // ---- families ----

    use engine_backend::{
        FamilyOption, FamilyParam, ParamKind, ParamValues, ResolvedFamily, ResolvedOutput,
    };

    fn pick(family: &EventFamily, values: &ParamValues) -> Result<ResolvedFamily, String> {
        let thing = values.get("thing").map(String::as_str).unwrap_or_default();
        if !family
            .param("thing")
            .unwrap()
            .options
            .iter()
            .any(|o| o.id == thing)
        {
            return Err("that thing is gone".into());
        }
        let arms: Vec<(String, String)> = values
            .get("phases")
            .map(String::as_str)
            .unwrap_or_default()
            .split(',')
            .filter(|p| !p.trim().is_empty())
            .map(|p| (p.trim().to_owned(), format!("thing::{thing}.{}", p.trim())))
            .collect();
        if arms.is_empty() {
            return Err("no phase".into());
        }
        Ok(ResolvedFamily {
            arms,
            outputs: vec![ResolvedOutput {
                pin: "Value".into(),
                type_name: if thing == "wide" { "Vec3" } else { "Vec2" }.into(),
                lowering: "vec2:x,y".into(),
            }],
        })
    }

    fn family() -> EventFamily {
        let option = |id: &str| FamilyOption {
            id: id.into(),
            label: id.into(),
            detail: String::new(),
        };
        EventFamily {
            id: "thing.pick".into(),
            label: "Pick".into(),
            group: "Pickers".into(),
            description: "picks".into(),
            documentation: "docs".into(),
            params: vec![
                FamilyParam {
                    name: "thing".into(),
                    label: "Thing".into(),
                    kind: ParamKind::Choice,
                    options: vec![option("narrow"), option("wide")],
                    default: "narrow".into(),
                },
                FamilyParam {
                    name: "phases".into(),
                    label: "Phases".into(),
                    kind: ParamKind::Multi,
                    options: vec![option("up"), option("down")],
                    default: "up".into(),
                },
            ],
            events: Vec::new(),
            resolve: pick,
        }
    }

    fn with_family() -> AssetEvents {
        let mut sets = set("Widget", Vec::new());
        sets.catalog = Arc::new(ComponentEventCatalog {
            events: Vec::new(),
            families: vec![family()],
            diagnostics: Vec::new(),
        });
        sets
    }

    #[test]
    fn a_family_is_a_palette_node_per_choice_each_with_its_own_typed_pins() {
        let nodes = family_node_definitions(&[with_family(), with_family()]);
        assert_eq!(nodes.len(), 2, "one per choice, once per class and family");
        let titles: Vec<_> = nodes.iter().map(|(_, node)| node.name.as_str()).collect();
        assert_eq!(titles, ["On narrow", "On wide"]);
        let wide = nodes.iter().find(|(_, node)| node.name == "On wide").unwrap();
        assert_eq!(wide.1.id, "event::family::thing.pick:wide");
        assert_eq!(wide.1.properties.get("thing").map(String::as_str), Some("wide"));
        assert!(
            wide.1.outputs.iter().any(|p| p.id == "Value" && format!("{:?}", p.data_type).contains("Vec3")),
            "its value pin is typed from its choice"
        );
        assert_eq!(family_of_node_type(&wide.1.id), Some("thing.pick"));
        let (category, node) = &nodes[0];
        assert_eq!(category, "Events/Components/Widget/Pickers");
        assert_eq!(node.id, "event::family::thing.pick:narrow");
        assert_eq!(node.properties.get("title").map(String::as_str), Some("On narrow"));
        assert!(node.is_event);
        assert_eq!(
            node.inputs.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["component_ref"],
            "dragged off a component reference like any component event"
        );
        let outputs: Vec<_> = node.outputs.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(outputs, ["up", "Value"]);
        let property = |key: &str| node.properties.get(key).map(String::as_str);
        assert_eq!(property("thing"), Some("narrow"));
        assert_eq!(property("phases"), Some("up"));
        assert_eq!(property("events"), Some("up=thing::narrow.up"));
        assert_eq!(property("lowering"), Some("Value=vec2:x,y"));
        assert_eq!(property("scope"), Some("self"));
        assert_eq!(property("component_type"), Some("Widget"));
    }

    #[test]
    fn a_family_whose_defaults_do_not_resolve_offers_no_node() {
        let mut broken = with_family();
        let mut f = family();
        f.params[0].options.clear();
        Arc::get_mut(&mut broken.catalog).unwrap().families = vec![f];
        assert!(family_node_definitions(&[broken]).is_empty());
    }

    fn node_of_family() -> graphy::NodeInstance {
        let (_, def) = family_node_definitions(&[with_family()]).remove(0);
        let mut node =
            graphy::NodeInstance::new("n1", def.id.clone(), graphy::Position::new(0.0, 0.0));
        for pin in &def.outputs {
            node.add_output_pin(
                &pin.id,
                crate::canvas_profile::datatype_from_str(&pin.data_type.to_string()),
            );
        }
        for (key, value) in &def.properties {
            node.set_property(key, serde_json::Value::String(value.clone()));
        }
        node
    }

    #[test]
    fn changing_a_parameter_reshapes_the_pins_and_the_snapshot() {
        use crate::canvas_profile::family_commands;
        let f = family();
        let mut node = node_of_family();
        assert!(family_commands(&f, &node).is_empty(), "nothing to change");

        node.set_property("phases", serde_json::Value::String("up,down".into()));
        let commands = family_commands(&f, &node);
        let snapshot = commands.iter().find_map(|c| match c {
            graphy::GraphCommand::SetProperty { key, value, .. } if key == "events" => {
                value.clone()
            }
            _ => None,
        });
        assert_eq!(
            snapshot,
            Some(serde_json::Value::String(
                "up=thing::narrow.up,down=thing::narrow.down".into()
            ))
        );
        let pins = commands.iter().find_map(|c| match c {
            graphy::GraphCommand::SetPins { outputs, .. } => {
                Some(outputs.iter().map(|p| p.id.clone()).collect::<Vec<_>>())
            }
            _ => None,
        });
        assert_eq!(pins.unwrap(), ["up", "down", "Value"]);
    }

    #[test]
    fn a_new_choice_retypes_the_value_pin() {
        use crate::canvas_profile::family_commands;
        let f = family();
        let mut node = node_of_family();
        node.set_property("thing", serde_json::Value::String("wide".into()));
        let retyped = family_commands(&f, &node).into_iter().any(|c| match c {
            graphy::GraphCommand::SetPins { outputs, .. } => outputs
                .iter()
                .any(|p| p.id == "Value" && format!("{:?}", p.data_type()).contains("Vec3")),
            _ => false,
        });
        assert!(retyped);
    }

    #[test]
    fn parameters_that_do_not_resolve_leave_the_node_as_it_was() {
        use crate::canvas_profile::family_commands;
        let f = family();
        let mut node = node_of_family();
        node.set_property("thing", serde_json::Value::String("deleted".into()));
        assert!(
            family_commands(&f, &node).is_empty(),
            "keeps its pins, wires and snapshot"
        );
        node.set_property("thing", serde_json::Value::String("narrow".into()));
        node.set_property("phases", serde_json::Value::String(String::new()));
        assert!(family_commands(&f, &node).is_empty());
    }

    // ---- stability of family nodes loaded from disk ----

    fn props(pairs: &[(&str, &str)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
            .collect()
    }

    #[test]
    fn a_family_node_is_titled_from_what_it_was_saved_with() {
        assert_eq!(
            family_node_title(&props(&[("title", "On Move"), ("events", "Triggered=input::Jump.triggered")])),
            "On Move",
            "the saved title wins"
        );
        assert_eq!(
            family_node_title(&props(&[(
                "events",
                "Started=input::IA_Move.started,Ongoing=input::IA_Move.ongoing"
            )])),
            "On IA_Move",
            "a file saved before titles existed: read from the events it listens to"
        );
        assert_eq!(family_node_title(&props(&[("title", "  ")])), "On Event");
        assert_eq!(family_node_title(&HashMap::new()), "On Event");
        assert_eq!(
            family_node_title(&props(&[("events", "garbage")])),
            "On Event",
            "never panics on a damaged snapshot"
        );
    }

    fn graph_with(node: graphy::NodeInstance) -> graphy::GraphDescription {
        let mut graph = graphy::GraphDescription::new("t");
        graph.add_node(node);
        graph
    }

    #[test]
    fn opening_a_graph_brings_a_stale_family_node_in_line_with_the_mapping() {
        let f = family();
        let mut stale = node_of_family();
        // What an older build left: no title, the other action's snapshot.
        stale.properties.remove("title");
        stale.set_property("thing", serde_json::Value::String("wide".into()));
        let mut graph = graph_with(stale);
        let families = HashMap::from([(f.id.clone(), f)]);

        assert_eq!(reconcile_family_nodes(&mut graph, &families), 1);
        let node = graph.nodes.values().next().unwrap();
        let property = |key: &str| node.properties.get(key).and_then(|v| v.as_str());
        assert_eq!(property("title"), Some("On wide"));
        assert_eq!(property("events"), Some("up=thing::wide.up"));
        assert!(
            node.outputs.iter().any(|p| p.id == "Value"
                && format!("{:?}", p.data_type()).contains("Vec3")),
            "its value pin follows the action's type now"
        );

        // Settled: a second pass has nothing to do.
        assert_eq!(reconcile_family_nodes(&mut graph, &families), 0);
    }

    #[test]
    fn a_family_node_whose_action_is_gone_is_left_as_saved() {
        let f = family();
        let mut orphan = node_of_family();
        orphan.set_property("thing", serde_json::Value::String("deleted".into()));
        let before = orphan.clone();
        let mut graph = graph_with(orphan);
        let families = HashMap::from([(f.id.clone(), f)]);
        assert_eq!(reconcile_family_nodes(&mut graph, &families), 0);
        let after = graph.nodes.values().next().unwrap();
        assert_eq!(after.properties, before.properties);
        assert_eq!(after.outputs.len(), before.outputs.len(), "keeps its pins and wires");
    }

    #[test]
    fn a_family_node_whose_component_is_gone_is_left_as_saved() {
        let node = node_of_family();
        let before = node.properties.clone();
        let mut graph = graph_with(node);
        assert_eq!(reconcile_family_nodes(&mut graph, &HashMap::new()), 0);
        assert_eq!(graph.nodes.values().next().unwrap().properties, before);
    }

    #[test]
    fn the_compiler_is_given_every_signature() {
        let events = palette_events(&[set(
            "Widget",
            vec![event("Poke", "poke"), event("Prod", "prod")],
        )]);
        let names: Vec<_> = events.iter().map(|e| e.signature.name.as_str()).collect();
        assert_eq!(names, ["thing::a1.poke", "thing::a1.prod"]);
    }
}
