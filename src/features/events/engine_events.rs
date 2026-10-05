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
    Some(EventSignature { id: descriptor.id, name: descriptor.name.clone(), fields })
}

/// The engine's built-in events.
pub fn builtin_events() -> Vec<PaletteEvent> {
    plugin_editor_api::pulsar_events::builtin::builtin_events()
        .into_iter()
        .filter_map(|(descriptor, category)| {
            Some(PaletteEvent { signature: signature_of(&descriptor)?, category: category.to_string(), declared_here: false })
        })
        .collect()
}

/// Custom events declared by the compiled modules of every class in the
/// project except `current` (the class being edited).
pub fn project_events(class_path: Option<&Path>) -> Vec<PaletteEvent> {
    let Some(class_path) = class_path else { return Vec::new() };
    let current = crate::features::class_dirs::class_name_of(class_path);
    let classes: Vec<PathBuf> = crate::features::class_dirs::sibling_class_dirs(class_path);
    let mut events = Vec::new();
    for class in classes {
        let name = crate::features::class_dirs::class_name_of(&class).unwrap_or_default();
        if Some(&name) == current.as_ref() {
            continue;
        }
        let module = class.join("events").join(".build").join("module.json");
        let Ok(json) = std::fs::read_to_string(&module) else { continue };
        let Ok(module) = Module::from_json(&json) else {
            tracing::debug!("unreadable script module {}", module.display());
            continue;
        };
        for signature in blueprint_compiler::declared_events(&module) {
            events.push(PaletteEvent { signature, category: format!("Custom/{name}"), declared_here: false });
        }
    }
    events
}

/// This class's own custom events, as the compiler will declare them.
pub fn local_events(class_name: &str, defs: &[crate::core::graph::EventDefinition]) -> Vec<PaletteEvent> {
    defs.iter()
        .filter(|d| !d.name.trim().is_empty())
        .filter_map(|d| {
            let fields = d
                .fields
                .iter()
                .map(|f| Some(EventField::new(f.name.clone(), blueprint_compiler::script_type(&f.type_name)?)))
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
    builtin_events().into_iter().chain(project_events(class_path)).map(|e| e.signature).collect()
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

/// The editor's event definitions as compiler input.
pub fn event_sources(defs: &[crate::core::graph::EventDefinition]) -> Vec<blueprint_compiler::EventSource> {
    defs.iter()
        .map(|d| blueprint_compiler::EventSource {
            uid: d.uid.clone(),
            name: d.name.clone(),
            fields: d.fields.iter().map(|f| (f.name.clone(), f.type_name.clone())).collect(),
        })
        .collect()
}

fn pin(id: &str, ty: &str, pin_type: PinType) -> PinDefinition {
    PinDefinition {
        id: id.to_string(),
        name: id.to_string(),
        data_type: PinDataType::from_type_str(blueprint_graph::DataType::from_type_str(ty).to_string()),
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
    inputs.extend(node.inputs.iter().map(|(id, ty)| pin(id, ty, PinType::Input)));
    outputs.extend(node.outputs.iter().map(|(id, ty)| pin(id, ty, PinType::Output)));
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
) -> Vec<(String, NodeDefinition)> {
    let mut events = builtin_events();
    events.extend(project_events(class_path));
    events.extend(local_events(class_name, defs));
    event_nodes(&events).iter().map(|n| (n.category.clone(), node_definition(n))).collect()
}

/// Build typed Blueprint entry points for host-owned component events. Each
/// node requires a reference to the emitting component and exposes the
/// declaration's original typed fields as outputs.
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
                description: format!("{title} — subscribe to `{}` on a component instance", event.event.name),
                documentation: format!(
                    "Runs when `{}` is emitted by the connected `{}` component instance. The component reference must come from a matching `Get {}` node.",
                    event.event.name, event.component_class, event.component_class
                ),
                inputs: vec![pin("component_ref", &event.component_class, PinType::Input)],
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
    events.iter().map(|event| EventSignature::from(&event.event)).collect()
}

/// Title and whether it is an entry point, for an `event::<kind>::<name>`
/// node loaded from a file.
pub fn describe_node_type(node_type: &str) -> Option<(String, bool)> {
    if let Some(event_name) = node_type.strip_prefix("event::on_component::") {
        let (component, member) = event_name.split_once('.')?;
        return Some((format!("On {} ({component})", title_case(member)), true));
    }
    // Keep old serialized component event nodes readable. Their IDs used
    // colons as separators before the canonical double-colon format shipped.
    if let Some(legacy) = node_type.strip_prefix("event:on_component:") {
        let (component, member) = legacy.split_once(':')?;
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
            chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}
