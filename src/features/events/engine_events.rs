//! Engine events in the Blueprint editor (Pulsar-Native#924).
//!
//! The palette offers, for every event a class can see, an "On <Event>"
//! entry point and "Send <Event> to", "Broadcast <Event>" and "Send <Event>
//! to Class" calls, grouped under `Events/<category>`:
//!
//! - the engine's built-in events (`pulsar_events::builtin`: lifecycle,
//!   world, input, physics, gameplay);
//! - the custom events of **other** classes in the project, read from their
//!   compiled modules (`src/classes/<Class>/events/.build/module.json`);
//! - this class's own custom events (its event definitions), declared as
//!   `<Class>.<Event>` when it compiles.
//!
//! Node shapes come from `blueprint_compiler::palette::event_nodes`, which
//! is what the compiler reads back.

use std::path::{Path, PathBuf};

use blueprint_compiler::palette::{event_nodes, EventNode, PaletteEvent};
use plugin_editor_api::pulsar_events::gamma::{EventDescriptor, FieldType};
use pulsar_script_vm::{EventField, EventSignature, Module, Type};

use crate::core::definitions::{NodeDefinition, PinDefinition};
use crate::core::types::{PinDataType, PinType};

/// A script's view of an engine descriptor (`u64` fields are entities).
/// `None` for events with fields scripts cannot hold (raw bytes).
pub fn signature_of(descriptor: &EventDescriptor) -> Option<EventSignature> {
    let fields = descriptor
        .fields
        .iter()
        .map(|(name, ty)| {
            let ty = match ty {
                FieldType::Bool => Type::Bool,
                FieldType::I64 => Type::Int,
                FieldType::F64 => Type::Float,
                FieldType::Str => Type::Str,
                FieldType::U64 => Type::Entity,
                FieldType::Bytes => return None,
            };
            Some(EventField::new(name.clone(), ty))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(EventSignature {
        id: descriptor.id,
        name: descriptor.namespace.as_ref().map_or_else(
            || descriptor.name.clone(),
            |ns| format!("{ns}::{}", descriptor.name),
        ),
        fields,
    })
}

/// The engine's built-in events.
pub fn builtin_events() -> Vec<PaletteEvent> {
    plugin_editor_api::pulsar_events::builtin::builtin_events()
        .into_iter()
        .filter_map(|(descriptor, category)| {
            Some(PaletteEvent {
                signature: signature_of(&descriptor)?,
                category: category.to_string(),
                declared_here: false,
            })
        })
        .collect()
}

/// Custom events declared by the compiled modules of every class in the
/// project except `current` (the class being edited).
pub fn project_events(class_path: Option<&Path>) -> Vec<PaletteEvent> {
    let Some(class_path) = class_path else {
        return Vec::new();
    };
    let current = crate::features::class_dirs::class_name_of(class_path);
    let classes: Vec<PathBuf> = crate::features::class_dirs::sibling_class_dirs(class_path);
    let mut events = Vec::new();
    for class in classes {
        let name = crate::features::class_dirs::class_name_of(&class).unwrap_or_default();
        if Some(&name) == current.as_ref() {
            continue;
        }
        let module = class.join("events").join(".build").join("module.json");
        let Ok(json) = std::fs::read_to_string(&module) else {
            continue;
        };
        let Ok(module) = Module::from_json(&json) else {
            tracing::debug!("unreadable script module {}", module.display());
            continue;
        };
        for signature in blueprint_compiler::declared_events(&module) {
            events.push(PaletteEvent {
                signature,
                category: format!("Custom/{name}"),
                declared_here: false,
            });
        }
    }
    events
}

// ---- input action events -----------------------------------------------------

/// The phases of an input action, each raising `input::<Action>.<phase>`.
const INPUT_PHASES: [&str; 5] = ["started", "ongoing", "triggered", "completed", "canceled"];

/// The fields every input action event carries (`entity` is the object the
/// mapping is attached to; the rest describe the value).
const INPUT_FIELDS: [&str; 7] = [
    "entity",
    "value",
    "x",
    "y",
    "z",
    "elapsed",
    "triggered_for",
];

/// The name an action's events go by: its `identifier`, else its file name
/// made into one. Mirrors `input_plugin::model::action::to_identifier`; this
/// editor pins the engine by revision, so it reads the `.inputaction` files
/// instead of linking the plugin.
fn input_action_name(identifier: &str, file_stem: &str) -> String {
    let identifier = identifier.trim();
    if !identifier.is_empty() {
        return identifier.to_owned();
    }
    let mut out = String::new();
    let mut upper_next = true;
    for c in file_stem.trim().chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(if upper_next { c.to_ascii_uppercase() } else { c });
            upper_next = false;
        } else {
            upper_next = true;
        }
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// The events of the input actions in the project at `root`, as the input
/// runtime registers them: five per action, in the `input` namespace.
fn read_input_events(root: &Path) -> Vec<EventDescriptor> {
    let mut names: Vec<String> = engine_fs::virtual_fs::find_by_extension(root, "inputaction")
        .iter()
        .filter_map(|path| {
            let text = engine_fs::virtual_fs::read_file(path).ok()?;
            let asset: serde_json::Value = serde_json::from_slice(&text).ok()?;
            let identifier = asset.get("identifier").and_then(|v| v.as_str()).unwrap_or("");
            let stem = path.file_stem()?.to_str()?;
            Some(input_action_name(identifier, stem))
        })
        .collect();
    names.sort();
    names.dedup();
    names
        .iter()
        .flat_map(|name| INPUT_PHASES.map(|phase| format!("{name}.{phase}")))
        .filter_map(|event| {
            EventDescriptor::dynamic_in(
                "input",
                event,
                INPUT_FIELDS.map(|field| {
                    let ty = if field == "entity" { FieldType::U64 } else { FieldType::F64 };
                    (field, ty)
                }),
            )
            .ok()
        })
        .collect()
}

/// How long a read of the project's input actions is reused: the palette and
/// every compile ask, and the project is not walked each time.
const INPUT_CACHE: std::time::Duration = std::time::Duration::from_secs(3);

/// Input action events of the project the class at `class_path` belongs to,
/// one "On" node per phase of every action: a character Blueprint handles
/// `input::Move.triggered` here, with `Self` scope (the first field is
/// `entity`, so that is the node's default), and hears only its own player.
/// The mapping that raises them is the one the level's player controller
/// brings to the character at possession. "Send"/"Broadcast" nodes are not
/// offered: input events are the runtime's to raise.
pub fn input_events(class_path: Option<&Path>) -> Vec<PaletteEvent> {
    use std::sync::Mutex;
    use std::time::Instant;
    static CACHE: Mutex<Option<(PathBuf, Instant, Vec<EventDescriptor>)>> = Mutex::new(None);

    let Some(root) = class_path.and_then(crate::features::class_dirs::project_root_of) else {
        return Vec::new();
    };
    let descriptors = {
        let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
        match cache.as_ref() {
            Some((cached, at, found)) if *cached == root && at.elapsed() < INPUT_CACHE => {
                found.clone()
            }
            _ => {
                let found = read_input_events(&root);
                *cache = Some((root, Instant::now(), found.clone()));
                found
            }
        }
    };
    descriptors
        .iter()
        .filter_map(|descriptor| {
            let action = descriptor.name.split('.').next().unwrap_or_default();
            Some(PaletteEvent {
                signature: signature_of(descriptor)?,
                category: format!("Input/{action}"),
                declared_here: false,
            })
        })
        .collect()
}

fn animation_catalog(
    class_path: Option<&Path>,
    referenced_assets: &std::collections::HashSet<String>,
) -> Vec<(String, EventDescriptor)> {
    let Some(root) = class_path.and_then(crate::features::class_dirs::project_root_of) else {
        return Vec::new();
    };
    if referenced_assets.is_empty() {
        return Vec::new();
    }
    // Asset-source-aware: only the assets the component names (its graph's
    // closure, or its single sequence) are read, through the asset index.
    let catalog = engine_backend::component_animation_notify_events(&root, referenced_assets);
    for diagnostic in catalog.diagnostics {
        tracing::warn!(path = %diagnostic.path.display(), error = %diagnostic.error,
            "could not discover animation notify events");
    }
    catalog.events
}

/// Animation schemas are available to a class only when its prefab has a
/// typed skeletal animation asset reference. This keeps the event palette
/// aligned with the asset refs in the class's SceneDB component defaults.
fn referenced_animation_asset_ids(class_path: &Path) -> std::collections::HashSet<String> {
    let path = class_path.join("prefab.json");
    let Ok(bytes) = engine_fs::virtual_fs::read_file(&path) else {
        return std::collections::HashSet::new();
    };
    let Ok(prefab) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return std::collections::HashSet::new();
    };
    referenced_animation_asset_ids_from_json(&prefab)
}

fn referenced_animation_asset_ids_from_json(
    prefab: &serde_json::Value,
) -> std::collections::HashSet<String> {
    prefab
        .get("components")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|component| {
            component
                .get("class_name")
                .and_then(serde_json::Value::as_str)
                == Some("SkeletalMeshComponent")
                && component
                    .get("enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true)
        })
        .filter_map(|component| {
            let data = component.get("data")?;
            // the Animation Graph is the authored source; `animation` is the
            // one-release compatibility key of the old single sequence
            let id = data
                .get("animation_graph")
                .and_then(serde_json::Value::as_str)
                .or_else(|| data.get("animation")?.as_str())?;
            uuid::Uuid::parse_str(id).ok().map(|id| id.to_string())
        })
        .collect()
}

/// Animation assets named by this class's current prefab component defaults.
/// The in-memory prefab is authoritative while its inspector has unsaved edits.
pub fn animation_asset_ids_from_prefab(
    prefab: &crate::features::prefabs::PrefabAsset,
) -> std::collections::HashSet<String> {
    prefab
        .components
        .iter()
        .filter(|component| component.enabled && component.class_name == "SkeletalMeshComponent")
        .filter_map(|component| {
            let id = component
                .data
                .get("animation_graph")
                .and_then(serde_json::Value::as_str)
                .or_else(|| component.data.get("animation")?.as_str())?;
            uuid::Uuid::parse_str(id).ok().map(|id| id.to_string())
        })
        .collect()
}

fn animation_palette_event(descriptor: &EventDescriptor) -> Option<PaletteEvent> {
    Some(PaletteEvent {
        signature: signature_of(descriptor)?,
        category: "Animation".into(),
        declared_here: false,
    })
}

/// saved animation assets use the same descriptors as the live player.
pub fn animation_events(class_path: Option<&Path>) -> Vec<PaletteEvent> {
    let Some(class_path) = class_path else {
        return Vec::new();
    };
    animation_catalog(
        Some(class_path),
        &referenced_animation_asset_ids(class_path),
    )
    .iter()
    .filter_map(|(_, descriptor)| animation_palette_event(descriptor))
    .collect()
}

/// This class's own custom events, as the compiler will declare them.
pub fn local_events(
    class_name: &str,
    defs: &[crate::core::graph::EventDefinition],
) -> Vec<PaletteEvent> {
    defs.iter()
        .filter(|d| !d.name.trim().is_empty())
        .filter_map(|d| {
            let fields = d
                .fields
                .iter()
                .map(|f| {
                    Some(EventField::new(
                        f.name.clone(),
                        blueprint_compiler::script_type(&f.type_name)?,
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(PaletteEvent {
                signature: EventSignature {
                    id: 0,
                    name: blueprint_compiler::qualified_event_name(class_name, &d.name),
                    fields,
                },
                category: format!("Custom/{class_name}"),
                declared_here: true,
            })
        })
        .collect()
}

/// What the compiler checks event nodes against: built-ins and other
/// classes' events.
pub fn known_event_signatures(class_path: Option<&Path>) -> Vec<EventSignature> {
    builtin_events()
        .into_iter()
        .chain(project_events(class_path))
        .chain(input_events(class_path))
        .chain(animation_events(class_path))
        .map(|e| e.signature)
        .collect()
}

/// Typed component event signatures from the host catalog, combined with
/// built-in and project-defined events for graph validation/linking. The
/// host snapshot is authoritative across editor DLL boundaries.
pub fn known_event_signatures_with_components(
    class_path: Option<&Path>,
    component_events: &[plugin_editor_api::ComponentEventMetadata],
) -> Vec<EventSignature> {
    known_event_signatures(class_path)
        .into_iter()
        .chain(component_event_signatures(component_events))
        .collect()
}

/// Compile against the class's current in-memory animation asset references.
/// This matters while the prefab inspector has unsaved reference changes.
pub fn known_event_signatures_with_components_and_animation_assets(
    class_path: Option<&Path>,
    component_events: &[plugin_editor_api::ComponentEventMetadata],
    animation_assets: &std::collections::HashSet<String>,
) -> Vec<EventSignature> {
    let mut signatures = builtin_events()
        .into_iter()
        .chain(project_events(class_path))
        .chain(input_events(class_path))
        .chain(
            animation_catalog(class_path, animation_assets)
                .iter()
                .filter_map(|(_, descriptor)| animation_palette_event(descriptor)),
        )
        .map(|event| event.signature)
        .collect::<Vec<_>>();
    signatures.extend(component_event_signatures(component_events));
    signatures
}

/// The editor's event definitions as compiler input.
pub fn event_sources(
    defs: &[crate::core::graph::EventDefinition],
) -> Vec<blueprint_compiler::EventSource> {
    defs.iter()
        .map(|d| blueprint_compiler::EventSource {
            uid: d.uid.clone(),
            name: d.name.clone(),
            fields: d
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.type_name.clone()))
                .collect(),
        })
        .collect()
}

fn pin(id: &str, ty: &str, pin_type: PinType) -> PinDefinition {
    PinDefinition {
        id: id.to_string(),
        name: id.to_string(),
        data_type: PinDataType::from_type_str(
            graphy_blueprint::DataType::from_type_str(ty).to_string(),
        ),
        pin_type,
    }
}

/// A palette definition for one event node.
pub fn node_definition(node: &EventNode) -> NodeDefinition {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    if node.is_event {
        outputs.push(pin("Body", "execution", PinType::Output));
    } else {
        inputs.push(pin("exec", "execution", PinType::Input));
        outputs.push(pin("exec_out", "execution", PinType::Output));
    }
    inputs.extend(
        node.inputs
            .iter()
            .map(|(id, ty)| pin(id, ty, PinType::Input)),
    );
    outputs.extend(
        node.outputs
            .iter()
            .map(|(id, ty)| pin(id, ty, PinType::Output)),
    );
    NodeDefinition {
        id: node.node_type.clone(),
        name: node.name.clone(),
        icon: if node.is_event { "📡" } else { "📨" }.to_string(),
        description: format!("{} ({})", node.name, node.category),
        documentation: node.doc.clone(),
        inputs,
        outputs,
        properties: node.properties.iter().cloned().collect(),
        color: Some(if node.is_event { "#C0392B" } else { "#00A8E8" }.to_string()),
        is_event: node.is_event,
    }
}

/// Every event node for the class at `class_path` (named `class_name`,
/// with event definitions `defs`), by category.
pub fn event_node_definitions(
    class_path: Option<&Path>,
    class_name: &str,
    defs: &[crate::core::graph::EventDefinition],
    animation_assets: &std::collections::HashSet<String>,
) -> Vec<(String, NodeDefinition)> {
    let mut events = builtin_events();
    events.extend(project_events(class_path));
    events.extend(local_events(class_name, defs));
    let mut definitions = event_nodes(&events)
        .iter()
        .map(|node| (node.category.clone(), node_definition(node)))
        .collect::<Vec<_>>();
    // Input actions: entry points only.
    definitions.extend(
        event_nodes(&input_events(class_path))
            .iter()
            .filter(|node| node.is_event)
            .map(|node| (node.category.clone(), node_definition(node))),
    );
    definitions.extend(animation_notify_component_nodes(
        class_path,
        animation_assets,
    ));
    definitions
}

/// Animation notifies are raised by a particular `SkeletalMeshComponent` and
/// are identified by the animation asset and notify IDs. They must use the
/// existing component-reference subscription path, never the generic event
/// palette's Global/Broadcast nodes.
fn animation_notify_component_nodes(
    class_path: Option<&Path>,
    animation_assets: &std::collections::HashSet<String>,
) -> Vec<(String, NodeDefinition)> {
    animation_event_nodes(animation_catalog(class_path, animation_assets))
}

/// Entry points for canonical animation event descriptors: `animseq` ones are
/// notifies and notify states of a Sequence, `montage` ones are what a Montage
/// owns (its own notifies and states, and its lifecycle events). Each is
/// filtered to its source asset by its canonical name and bound to one
/// `SkeletalMeshComponent` reference.
pub fn animation_event_nodes(catalog: Vec<(String, EventDescriptor)>) -> Vec<(String, NodeDefinition)> {
    let mut nodes = Vec::new();
    for (label, descriptor) in catalog {
        let Some(signature) = signature_of(&descriptor) else {
            continue;
        };
        let source_id = descriptor
            .name
            .split_once('.')
            .map(|(source, _)| source)
            .unwrap_or_default();
        let source_label: String = source_id.chars().take(8).collect();
        let kind = descriptor.namespace.as_deref().unwrap_or("animseq");
        let (group, source_noun) = if kind == "montage" {
            ("Montage", "Montage")
        } else if kind == "animgraph" {
            ("Animation Graph", "Animation Graph")
        } else {
            ("Animation", "animation sequence")
        };
        let mut outputs = vec![pin("Body", "execution", PinType::Output)];
        outputs.extend(signature.fields.iter().map(|field| {
            pin(
                &field.name,
                &blueprint_compiler::palette::pin_type_name(&field.ty),
                PinType::Output,
            )
        }));
        let title = format!("On {label}");
        nodes.push((
            format!(
                "Events/Components/SkeletalMeshComponent/{group}/{source_label}"
            ),
            NodeDefinition {
                // `component_event_name` strips the component qualifier for
                // animation asset events, leaving the canonical asset name.
                id: format!(
                    "event::on_component::SkeletalMeshComponent.{}",
                    signature.name
                ),
                name: title.clone(),
                icon: "📡".to_owned(),
                description: format!(
                    "Runs for this SkeletalMeshComponent reference when {source_noun} {source_id} emits {label}."
                ),
                documentation: format!(
                    "Asset-owned event `{}` of {source_noun} `{source_id}`. Connect a `SkeletalMeshComponent` reference to `component_ref` to subscribe to that SceneDB component instance. The event is delivered on the referenced actor's entity channel; it is not a global broadcast. Notify states raise it for each phase: check `notify_phase` (`begin`, `tick`, `end`) and `end_reason`.",
                    signature.name
                ),
                inputs: vec![pin(
                    "component_ref",
                    "SkeletalMeshComponent",
                    PinType::Input,
                )],
                outputs,
                properties: std::collections::HashMap::from([
                    ("component_type".to_owned(), "SkeletalMeshComponent".to_owned()),
                    ("animation_sequence_id".to_owned(), source_id.to_owned()),
                    ("animation_source_kind".to_owned(), kind.to_owned()),
                ]),
                color: Some("#C0392B".to_owned()),
                is_event: true,
            },
        ));
    }
    nodes.sort_by(|a, b| (&a.0, &a.1.name).cmp(&(&b.0, &b.1.name)));
    nodes
}

/// Build typed Blueprint entry points for host-owned component events. The
/// entry point subscribes to the component on the script instance's root
/// entity and exposes the declaration's original typed fields as outputs.
pub fn component_event_node_definitions(
    events: &[plugin_editor_api::ComponentEventMetadata],
) -> Vec<(String, NodeDefinition)> {
    let mut nodes = Vec::with_capacity(events.len());
    for event in events {
        let Some((owner, member)) = event.event.name.split_once('.') else {
            tracing::warn!(event = %event.event.name, "component event name is not class-qualified");
            continue;
        };
        if owner != event.component_class {
            tracing::warn!(
                event = %event.event.name,
                registered_component = %event.component_class,
                event_component = owner,
                "component event owner does not match its stable name"
            );
            continue;
        }

        let title = format!("On {} ({})", title_case(member), event.component_class);
        let mut outputs = vec![pin("Body", "execution", PinType::Output)];
        outputs.extend(event.event.fields.iter().map(|field| {
            pin(
                &field.name,
                &blueprint_compiler::palette::pin_type_name(&field.ty),
                PinType::Output,
            )
        }));

        nodes.push((
            format!("Events/Components/{}", event.component_class),
            NodeDefinition {
                id: format!("event::on_component::{}", event.event.name),
                name: title.clone(),
                icon: "📡".to_string(),
                description: format!("{title} — subscribe to `{}` on this entity's component", event.event.name),
                documentation: format!(
                    "Runs when `{}` is emitted by this entity's `{}` component. The component reference is resolved from the owning entity at runtime.",
                    event.event.name, event.component_class
                ),
                inputs: Vec::new(),
                outputs,
                properties: std::collections::HashMap::from([(
                    "component_type".to_owned(),
                    event.component_class.clone(),
                )]),
                color: Some("#C0392B".to_string()),
                is_event: true,
            },
        ));
    }
    nodes.sort_by(|a, b| (&a.0, &a.1.name).cmp(&(&b.0, &b.1.name)));
    nodes
}

/// Preserve local field types, including structured `Type::Object` values;
/// Gamma's `Bytes` descriptor does not contain enough information to rebuild
/// those Blueprint pins.
pub fn component_event_signatures(
    events: &[plugin_editor_api::ComponentEventMetadata],
) -> Vec<EventSignature> {
    events
        .iter()
        .map(|event| EventSignature::from(&event.event))
        .collect()
}

/// Title and whether it is an entry point, for an `event::<kind>::<name>`
/// node loaded from a file.
pub fn describe_node_type(node_type: &str) -> Option<(String, bool)> {
    if let Some(event_name) = node_type.strip_prefix("event::on_component::") {
        if event_name.starts_with("SkeletalMeshComponent.animseq::") {
            return Some(("On Animation Notify".to_owned(), true));
        }
        if event_name.starts_with("SkeletalMeshComponent.animgraph::") {
            return Some(("On Animation State".to_owned(), true));
        }
        if event_name.starts_with("SkeletalMeshComponent.montage::") {
            let lifecycle = event_name.contains(".lifecycle_");
            return Some((if lifecycle { "On Montage Event" } else { "On Montage Notify" }.to_owned(), true));
        }
        let (component, member) = event_name.split_once('.')?;
        return Some((format!("On {} ({component})", title_case(member)), true));
    }
    // Keep legacy serialized component event node IDs readable. Versions in
    // the wild used either a colon or a dot between component and event names.
    if let Some(legacy) = node_type.strip_prefix("event:on_component:") {
        let legacy = legacy.trim_start_matches(':');
        let (component, member) = legacy.split_once(':').or_else(|| legacy.split_once('.'))?;
        if component.is_empty() || member.is_empty() {
            return None;
        }
        return Some((format!("On {} ({component})", title_case(member)), true));
    }
    let rest = node_type.strip_prefix("event::")?;
    let (kind, name) = rest.split_once("::")?;
    Some(match kind {
        "on" => (format!("On {name}"), true),
        "send" => (format!("Send {name} to"), false),
        "broadcast" => (format!("Broadcast {name}"), false),
        "to_class" => (format!("Send {name} to Class"), false),
        _ => return None,
    })
}

fn title_case(snake: &str) -> String {
    snake
        .split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod animation_event_tests {
    use super::*;

    fn montage_descriptor(item: &str) -> EventDescriptor {
        EventDescriptor::dynamic_in(
            "montage",
            format!("11111111-2222-4333-8444-555555555555.{item}"),
            [
                ("entity".to_owned(), FieldType::U64),
                ("source".to_owned(), FieldType::U64),
                ("result".to_owned(), FieldType::Str),
                ("instance".to_owned(), FieldType::I64),
            ],
        )
        .expect("montage is a valid namespace")
    }

    #[test]
    fn state_machine_events_become_component_bound_entry_points_of_their_graph() {
        let descriptor = EventDescriptor::dynamic_in(
            "animgraph",
            "11111111-2222-4333-8444-555555555555.state_entered",
            [("entity".to_owned(), FieldType::U64), ("source".to_owned(), FieldType::U64), ("machine".to_owned(), FieldType::Str)],
        )
        .expect("animgraph is a valid namespace");
        let nodes = animation_event_nodes(vec![("Locomotion state entered".to_owned(), descriptor)]);
        assert_eq!(nodes.len(), 1);
        let (category, node) = &nodes[0];
        assert_eq!(category, "Events/Components/SkeletalMeshComponent/Animation Graph/11111111");
        assert!(node.id.starts_with("event::on_component::SkeletalMeshComponent.animgraph::11111111-2222-4333-8444-555555555555."));
        assert_eq!(node.inputs[0].id, "component_ref");
        assert_eq!(node.properties.get("animation_source_kind").map(String::as_str), Some("animgraph"));
        assert_eq!(describe_node_type(&node.id), Some(("On Animation State".to_owned(), true)));
    }

    #[test]
    fn montage_events_become_component_bound_entry_points_filtered_to_their_asset() {
        let catalog = vec![
            ("Swing ended".to_owned(), montage_descriptor("lifecycle_ended")),
            ("Swing.Hit [abc]".to_owned(), montage_descriptor("hit")),
        ];
        let nodes = animation_event_nodes(catalog);
        assert_eq!(nodes.len(), 2);
        for (category, node) in &nodes {
            assert_eq!(category, "Events/Components/SkeletalMeshComponent/Montage/11111111");
            assert!(node.is_event);
            assert!(
                node.id.starts_with("event::on_component::SkeletalMeshComponent.montage::11111111-2222-4333-8444-555555555555."),
                "{}",
                node.id
            );
            // bound to one component reference, never a global broadcast
            assert_eq!(node.inputs.len(), 1);
            assert_eq!(node.inputs[0].id, "component_ref");
            assert_eq!(node.properties.get("component_type").map(String::as_str), Some("SkeletalMeshComponent"));
            assert_eq!(node.properties.get("animation_source_kind").map(String::as_str), Some("montage"));
            assert_eq!(
                node.properties.get("animation_sequence_id").map(String::as_str),
                Some("11111111-2222-4333-8444-555555555555")
            );
            let outputs: Vec<_> = node.outputs.iter().map(|p| p.id.as_str()).collect();
            assert_eq!(outputs, ["Body", "entity", "source", "result", "instance"]);
        }
    }

    #[test]
    fn sequence_notifies_and_states_stay_in_the_animation_group() {
        let descriptor = EventDescriptor::dynamic_in(
            "animseq",
            "99999999-2222-4333-8444-555555555555.swing",
            [("entity".to_owned(), FieldType::U64), ("notify_phase".to_owned(), FieldType::Str)],
        )
        .unwrap();
        let nodes = animation_event_nodes(vec![("Walk.Swing".to_owned(), descriptor)]);
        assert_eq!(nodes[0].0, "Events/Components/SkeletalMeshComponent/Animation/99999999");
        assert_eq!(nodes[0].1.properties.get("animation_source_kind").map(String::as_str), Some("animseq"));
    }

    #[test]
    fn saved_montage_nodes_are_described_as_entry_points() {
        let notify = "event::on_component::SkeletalMeshComponent.montage::m1.hit";
        assert_eq!(describe_node_type(notify), Some(("On Montage Notify".to_owned(), true)));
        let lifecycle = "event::on_component::SkeletalMeshComponent.montage::m1.lifecycle_ended";
        assert_eq!(describe_node_type(lifecycle), Some(("On Montage Event".to_owned(), true)));
        let sequence = "event::on_component::SkeletalMeshComponent.animseq::s1.step";
        assert_eq!(describe_node_type(sequence), Some(("On Animation Notify".to_owned(), true)));
    }
}
