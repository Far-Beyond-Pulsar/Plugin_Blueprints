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
    let catalog = engine_backend::project_animation_notify_events(&root);
    for diagnostic in catalog.diagnostics {
        tracing::warn!(path = %diagnostic.path.display(), error = %diagnostic.error,
            "could not discover animation notify events");
    }
    catalog
        .events
        .into_iter()
        .filter(|(_, descriptor)| {
            descriptor
                .name
                .split_once('.')
                .is_some_and(|(sequence_id, _)| referenced_assets.contains(sequence_id))
        })
        .collect()
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
            let id = component.get("data")?.get("animation")?.as_str()?;
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
            let id = component.data.get("animation")?.as_str()?;
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
    let mut nodes = Vec::new();
    for (label, descriptor) in animation_catalog(class_path, animation_assets) {
        let Some(signature) = signature_of(&descriptor) else {
            continue;
        };
        let sequence_id = descriptor
            .name
            .split_once('.')
            .map(|(sequence, _)| sequence)
            .unwrap_or_default();
        let sequence_label: String = sequence_id.chars().take(8).collect();
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
                "Events/Components/SkeletalMeshComponent/Animation/{sequence_label}"
            ),
            NodeDefinition {
                // `component_event_name` strips the component qualifier for
                // animation notify nodes, leaving the canonical animseq name.
                id: format!(
                    "event::on_component::SkeletalMeshComponent.{}",
                    signature.name
                ),
                name: title.clone(),
                icon: "📡".to_owned(),
                description: format!(
                    "Runs for this SkeletalMeshComponent reference when animation asset {sequence_id} emits {label}."
                ),
                documentation: format!(
                    "Asset-owned notify `{}` from animation sequence `{sequence_id}`. Connect a `SkeletalMeshComponent` reference to `component_ref` to subscribe to that SceneDB component instance. The event is delivered on the referenced actor's entity channel; it is not a global broadcast.",
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
                    ("animation_sequence_id".to_owned(), sequence_id.to_owned()),
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
