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
