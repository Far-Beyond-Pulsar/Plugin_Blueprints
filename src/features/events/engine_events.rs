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

/// The directory holding the project's classes, from a class directory
/// (`<project>/src/classes/<Class>`).
fn classes_dir(class_path: &Path) -> Option<PathBuf> {
    class_path.parent().map(Path::to_path_buf)
}

/// Custom events declared by the compiled modules of every class in the
/// project except `current` (the class being edited).
pub fn project_events(class_path: Option<&Path>) -> Vec<PaletteEvent> {
    let Some(dir) = class_path.and_then(classes_dir) else { return Vec::new() };
    let current = class_path.and_then(|p| p.file_name()).and_then(|n| n.to_str()).map(str::to_owned);
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut events = Vec::new();
    let mut classes: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    classes.sort();
    for class in classes {
        let name = class.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
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
        data_type: PinDataType::from_type_str(ui::graph::DataType::from_type_str(ty).to_string()),
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

/// Title and whether it is an entry point, for an `event::<kind>::<name>`
/// node loaded from a file.
pub fn describe_node_type(node_type: &str) -> Option<(String, bool)> {
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
