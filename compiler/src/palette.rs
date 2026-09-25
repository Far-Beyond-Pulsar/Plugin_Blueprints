//! Node shapes for `native::<name>` nodes, derived from the native
//! registry: what the editor palette offers, and exactly what
//! [`compile`](crate::compile) expects to find on such a node.
//!
//! Every native except pulsar_std's own `std::` functions (which keep
//! their existing nodes) becomes a node. Methods are grouped by the type
//! they are called on, so the editor can list "everything callable on a
//! reference to X" (its receiver pin has type X) and also show them in the
//! global list.

use pulsar_script_vm::{NativeFn, NativeRegistry, Type, TypeRegistry};

/// One palette node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeNode {
    /// Graph node type: `native::<native name>`.
    pub node_type: String,
    /// Display name, e.g. `Damage` or `Get Value`.
    pub name: String,
    /// Palette category: `Components/<C>`, `Types/<T>`, or the native's
    /// `category` attribute / namespace.
    pub category: String,
    pub doc: String,
    /// Whether the node has exec pins (`exec` in, `exec_out` out): every
    /// native not marked side-effect free.
    pub exec: bool,
    /// Data inputs: `(pin id, pin type)`, named after the parameters; a
    /// method's receiver is `self`.
    pub inputs: Vec<(String, String)>,
    /// Data outputs: `result` for the return value, and each `inout`
    /// parameter under its own name.
    pub outputs: Vec<(String, String)>,
    /// The type this is a method of, for "methods on a reference" queries.
    pub receiver: Option<String>,
}

/// The pin type string for a script type, as graph pins spell types.
pub fn pin_type_name(ty: &Type) -> String {
    match ty {
        Type::Unit => "()".into(),
        Type::Bool => "bool".into(),
        Type::Int => "i64".into(),
        Type::Float => "f64".into(),
        Type::Str => "String".into(),
        Type::Entity => "Entity".into(),
        Type::Component(name) | Type::Object(name) => name.clone(),
    }
}

/// Every palette node for `natives`, sorted by category then name.
pub fn native_nodes(natives: &NativeRegistry) -> Vec<NativeNode> {
    let mut nodes: Vec<NativeNode> = natives
        .functions()
        .filter(|n| !n.name.starts_with("std::"))
        .map(|n| native_node(n))
        .collect();
    nodes.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));
    nodes
}

/// The palette nodes callable on a value or reference of pin type
/// `type_name`.
pub fn methods_for<'n>(nodes: &'n [NativeNode], type_name: &'n str) -> impl Iterator<Item = &'n NativeNode> {
    nodes.iter().filter(move |n| n.receiver.as_deref() == Some(type_name))
}

fn native_node(native: &NativeFn) -> NativeNode {
    let (owner, member) = native.name.rsplit_once("::").unwrap_or(("", &native.name));
    let inputs = native
        .param_names
        .iter()
        .zip(&native.sig.params)
        .map(|(name, p)| (name.clone(), pin_type_name(&p.ty)))
        .collect();
    let mut outputs = Vec::new();
    if native.sig.ret != Type::Unit {
        outputs.push(("result".to_owned(), pin_type_name(&native.sig.ret)));
    }
    for (name, param) in native.param_names.iter().zip(&native.sig.params) {
        if param.inout {
            outputs.push((name.clone(), pin_type_name(&param.ty)));
        }
    }
    NativeNode {
        node_type: format!("native::{}", native.name),
        name: title_case(member),
        category: category(native, owner),
        doc: native.doc.clone(),
        exec: !native.flags.side_effect_free,
        inputs,
        outputs,
        receiver: native.receiver.as_ref().map(pin_type_name),
    }
}

fn category(native: &NativeFn, owner: &str) -> String {
    let types = TypeRegistry::global();
    let owner_type = match &native.receiver {
        Some(Type::Component(name)) => Some(Type::Component(name.clone())),
        Some(Type::Object(name)) => Some(Type::Object(name.clone())),
        _ if types.component(owner).is_some() => Some(Type::Component(owner.to_owned())),
        _ if types.is_known(&Type::Object(owner.to_owned())) => Some(Type::Object(owner.to_owned())),
        _ => None,
    };
    match owner_type {
        Some(Type::Component(name)) => format!("Components/{name}"),
        Some(Type::Object(name)) => format!("Types/{name}"),
        _ => native
            .attr("category")
            .map(str::to_owned)
            .unwrap_or_else(|| title_case(owner)),
    }
}

fn title_case(snake: &str) -> String {
    snake
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---- engine events (#924) ------------------------------------------------------

/// An engine event the palette offers nodes for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteEvent {
    pub signature: pulsar_script_vm::EventSignature,
    /// e.g. `Physics`, `Lifecycle`, `Custom/Door`.
    pub category: String,
    /// Declared by the class being edited (its "On" node listens on the
    /// object's own channel by default).
    pub declared_here: bool,
}

/// One event palette node: an "On <Event>" entry point, or a "Send <Event>
/// to", "Broadcast <Event>" or "Send <Event> to Class" call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventNode {
    /// `event::on::<name>`, `event::send::<name>`, `event::broadcast::<name>`
    /// or `event::to_class::<name>`.
    pub node_type: String,
    pub name: String,
    /// `Events/<category>`.
    pub category: String,
    pub doc: String,
    /// An entry point (an "On" node): exec output `Body`, no exec input.
    pub is_event: bool,
    /// Data inputs `(pin id, pin type)`.
    pub inputs: Vec<(String, String)>,
    /// Data outputs `(pin id, pin type)`.
    pub outputs: Vec<(String, String)>,
    /// Default node properties (the "On" node's `scope`).
    pub properties: Vec<(String, String)>,
}

/// Every event node for `events`, sorted by category then name. Pins are
/// named after the event's fields, which is what
/// [`compile`](crate::compile) reads.
pub fn event_nodes(events: &[PaletteEvent]) -> Vec<EventNode> {
    let mut nodes = Vec::new();
    for event in events {
        let sig = &event.signature;
        let category = format!("Events/{}", event.category);
        let fields: Vec<(String, String)> =
            sig.fields.iter().map(|f| (f.name.clone(), pin_type_name(&f.ty))).collect();
        let scope = match crate::default_scope(sig, event.declared_here) {
            pulsar_script_vm::SubscriptionScope::Self_ => "self",
            pulsar_script_vm::SubscriptionScope::Global => "global",
            pulsar_script_vm::SubscriptionScope::Class => "class",
        };
        nodes.push(EventNode {
            node_type: format!("event::on::{}", sig.name),
            name: format!("On {}", sig.name),
            category: category.clone(),
            doc: format!(
                "Runs when `{}` arrives. Scope (the `scope` property): `self` = sent to this object, `global` = broadcast, `class` = sent to every instance of this class.",
                sig.name
            ),
            is_event: true,
            inputs: Vec::new(),
            outputs: fields.clone(),
            properties: vec![("scope".into(), scope.into())],
        });
        let mut to = vec![("target".to_owned(), "Entity".to_owned())];
        to.extend(fields.iter().cloned());
        nodes.push(EventNode {
            node_type: format!("event::send::{}", sig.name),
            name: format!("Send {} to", sig.name),
            category: category.clone(),
            doc: format!("Send `{}` to one object; its \"On {}\" handlers with scope `self` run. Delivered at the next event flush.", sig.name, sig.name),
            is_event: false,
            inputs: to,
            outputs: Vec::new(),
            properties: Vec::new(),
        });
        nodes.push(EventNode {
            node_type: format!("event::broadcast::{}", sig.name),
            name: format!("Broadcast {}", sig.name),
            category: category.clone(),
            doc: format!("Broadcast `{}` on the global channel. Delivered at the next event flush.", sig.name),
            is_event: false,
            inputs: fields.clone(),
            outputs: Vec::new(),
            properties: Vec::new(),
        });
        let mut to_class = vec![("class".to_owned(), "String".to_owned())];
        to_class.extend(fields);
        nodes.push(EventNode {
            node_type: format!("event::to_class::{}", sig.name),
            name: format!("Send {} to Class", sig.name),
            category,
            doc: format!("Send `{}` to every instance of a class (name or GUID) listening with scope `class`.", sig.name),
            is_event: false,
            inputs: to_class,
            outputs: Vec::new(),
            properties: Vec::new(),
        });
    }
    nodes.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));
    nodes
}
