//! Blueprint graphs → Pulsar engine script modules.
//!
//! Input is the macro-expanded [`GraphDescription`] the editor already
//! builds for a class (see the plugin's `build_graphy_description`), plus
//! the class variables. Output is a [`pulsar_script_vm::Module`] that the
//! engine verifies, links against its native registry and runs. Everything
//! Blueprint-specific (exec pins, node kinds, pin naming) is resolved here;
//! the module knows nothing about graphs.
//!
//! ## Mapping
//!
//! - **Events** become exported functions: `begin_play`, `on_tick` →
//!   `tick(delta_time)`, `on_end_play` → `end_play`, `main`, input events,
//!   and custom events (`on_<uid>`, parameters from their data outputs).
//!   Several nodes of one event run in turn.
//! - **Exec chains** compile to straight-line code. A chain ends where its
//!   exec wire ends, returning to the enclosing construct (a loop body
//!   returns to the loop). Where two paths reach one node, its code is
//!   emitted on each path; an exec path that revisits a node on itself is
//!   an error (use a loop node).
//! - **Flow nodes** (`branch`, switches, `for_loop`, `while_loop`,
//!   `sequence`, `gate`, `multi_gate`, `flip_flop`, `do_once`, `do_n`,
//!   `delay`, `retriggerable_delay`) are intrinsics with real jumps.
//!   Stateful ones keep their state in hidden per-instance variables, so
//!   instances never share it. Delays suspend the call (the VM's `Wait`);
//!   the script runtime resumes it after that much game time.
//! - **Other flow nodes** run their own pulsar_std body through a selector
//!   native (`std::<node>` with an `exec_outputs` attribute) that reports
//!   which exec output fired; the compiler jumps there.
//! - **Other nodes** call the native `std::<node_type>` (pulsar_std's
//!   functions). Pure nodes are evaluated where their value is used;
//!   impure nodes' outputs keep the value from their last execution.
//! - **Variables**: `get_<name>` / `set_<name>` load and store instance
//!   variables.
//! - **`native::<name>`** calls any registered native (component and
//!   value-type methods, accessors, the VM stdlib): inputs are pins named
//!   after its parameters, `inout` parameters come back out on the output
//!   pin of the same name, the return value is `result`, and an
//!   unconnected method receiver means this entity's component.
//! - **Component nodes**: `comp_get_prop::C::p`, `comp_set_prop::C::p`,
//!   `comp_call::C::m` and `get_component_ref::C[::index]` call the natives
//!   `C::get_p`, `C::set_p`, `C::m` and `C::of`. An unconnected component
//!   input means this entity's component; a `get_component_ref` whose
//!   entity input is wired takes that object's component. A
//!   `get_component_ref` with a `slot_id` property (a prefab slot UUID)
//!   reads a hidden `__slot:<uuid>` handle variable instead, which the
//!   engine fills once when the instance is bound to its placed class.
//! - **Scene lookups**: `find_object_by_stable_id`, `find_object_by_name` and
//!   `object_ref_literal` produce an entity through `world::find_by_*`.
//! - **Engine events** (#924): the class's custom events are declared as
//!   engine events named `<Class>.<Event>` ([`qualified_event_name`]), and
//!   each one's `on_<uid>` handler also subscribes to it on the object's own
//!   channel, so other objects can send it. `event::on::<Event>` nodes
//!   ("On <Event>") compile to handler functions plus a module
//!   subscription (scope from the node's `scope` property: `self`,
//!   `global` or `class`; see [`default_scope`]); their data outputs are
//!   the event's fields. `event::send::<Event>` ("Send <Event> to"),
//!   `event::broadcast::<Event>` ("Broadcast <Event>") and
//!   `event::to_class::<Event>` ("Send <Event> to Class") call the
//!   `event::send` / `event::emit` / `event::emit_to_class` natives with the
//!   event's fields. Events are looked up in [`ClassSource::events`] and
//!   [`ClassSource::known_events`]. The old placeholder nodes `emit_event`,
//!   `on_event` and `remove_event_listener` are rejected with a pointer to
//!   these.

pub mod palette;

use std::collections::HashMap;

use graphy::{ConnectionType, DataType, GraphDescription, NodeInstance};
use pulsar_script_vm::{
    verify, BinOp, Constant, EventDecl, EventField, EventRef, EventSignature, Function, Import,
    Instr, Module, NativeRegistry, Param, Reg, Signature, Subscription, SubscriptionScope, Type,
    TypeRegistry, UnOp, Variable,
};
use serde_json::Value as Json;

/// Prefix of the hidden per-slot handle variables a module declares (one per
/// component slot its graph uses): `__slot:<slot uuid>`. The engine fills
/// them when it binds a script instance to a placed class
/// (`pulsar_class::SLOT_VARIABLE_PREFIX` is the same spelling).
pub const SLOT_VARIABLE_PREFIX: &str = "__slot:";

/// The hidden handle variable for component slot `slot_id`.
pub fn slot_variable_name(slot_id: &str) -> String {
    format!("{SLOT_VARIABLE_PREFIX}{slot_id}")
}

/// The component slots a compiled module refers to: `(slot id, component
/// class)` for every hidden slot variable.
pub fn module_slots(module: &Module) -> Vec<(String, String)> {
    module
        .variables
        .iter()
        .filter_map(|v| {
            let slot = v.name.strip_prefix(SLOT_VARIABLE_PREFIX)?;
            match &v.ty {
                Type::Component(class) => Some((slot.to_owned(), class.clone())),
                _ => None,
            }
        })
        .collect()
}

/// A class variable as the editor declares it.
#[derive(Clone, Debug)]
pub struct VariableSource {
    pub name: String,
    /// Rust-style type name, e.g. `"f32"`, `"String"`, `"Health"`.
    pub type_name: String,
    pub default: Option<Json>,
}

/// A custom event the class declares (the editor's event definitions).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventSource {
    /// The editor's id: its handler node is `on_<uid>`.
    pub uid: String,
    /// Display name; the engine event is `<Class>.<name>`.
    pub name: String,
    /// `(field name, Blueprint type name)`.
    pub fields: Vec<(String, String)>,
}

/// Everything needed to compile one Blueprint class.
pub struct ClassSource<'a> {
    /// Module (class) name.
    pub name: &'a str,
    /// The class graph with macros already expanded.
    pub graph: &'a GraphDescription,
    pub variables: &'a [VariableSource],
    /// Custom events this class declares.
    pub events: &'a [EventSource],
    /// Every other engine event the graph may handle or send: the built-in
    /// events and other classes' and plugins' declared events.
    pub known_events: &'a [EventSignature],
}

/// The engine name of event `event` declared by class `class`.
pub fn qualified_event_name(class: &str, event: &str) -> String {
    format!("{class}.{event}")
}

/// The events a compiled module declares, as signatures (for other
/// classes' palettes and compiles).
pub fn declared_events(module: &Module) -> Vec<EventSignature> {
    module.events.iter().map(EventSignature::from).collect()
}

/// The scope an "On <Event>" node listens on when its `scope` property is
/// not set: the object's own channel for events about one object (whose
/// first field is an entity called `entity` or `target`, like `Hit` and
/// `Damage`), for `TimerFired`, and for the class's own events; the global
/// channel otherwise.
pub fn default_scope(event: &EventSignature, declared_here: bool) -> SubscriptionScope {
    let about_one_object = event
        .fields
        .first()
        .is_some_and(|f| f.ty == Type::Entity && (f.name == "entity" || f.name == "target"));
    if declared_here || about_one_object || event.name == "TimerFired" {
        SubscriptionScope::Self_
    } else {
        SubscriptionScope::Global
    }
}

/// Parse a node's `scope` property.
pub fn parse_scope(value: &str) -> Option<SubscriptionScope> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "self" => SubscriptionScope::Self_,
        "global" => SubscriptionScope::Global,
        "class" => SubscriptionScope::Class,
        _ => return None,
    })
}

/// Node types that replaced the placeholder event nodes (#872).
const RETIRED_EVENT_NODES: [&str; 3] = ["emit_event", "on_event", "remove_event_listener"];

/// A compile error, attached to a node where there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub node: Option<String>,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.node {
            Some(node) => write!(f, "node `{node}`: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// Upper bound on one function's instructions (inlined exec merges can
/// grow code; this catches pathological graphs).
const MAX_CODE: usize = 200_000;

/// Compile a class. `natives` is the registry the module will be linked
/// against: every native call is emitted with the signature found there.
pub fn compile(source: &ClassSource<'_>, natives: &NativeRegistry) -> Result<Module, Vec<Diagnostic>> {
    let mut c = Compiler::new(source, natives);
    c.declare_variables();
    c.declare_engine_events();
    c.declare_events();
    c.compile_events();
    if !c.diagnostics.is_empty() {
        return Err(c.diagnostics);
    }
    if let Err(err) = verify(&c.module) {
        return Err(vec![Diagnostic { node: None, message: format!("internal compiler error: {err}") }]);
    }
    Ok(c.module)
}

/// The script type for a Blueprint pin or variable type name.
pub fn script_type(type_name: &str) -> Option<Type> {
    let compact: String = type_name.chars().filter(|c| !c.is_whitespace()).collect();
    let name = compact.trim_start_matches('&');
    Some(match name {
        "()" => Type::Unit,
        "bool" => Type::Bool,
        "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize" => Type::Int,
        "f32" | "f64" => Type::Float,
        "String" | "str" => Type::Str,
        "Entity" | "pulsar_scenedb::Entity" => Type::Entity,
        other => {
            let types = TypeRegistry::global();
            if types.component(other).is_some() {
                Type::Component(other.to_owned())
            } else if types.is_known(&Type::Object(other.to_owned())) {
                Type::Object(other.to_owned())
            } else {
                return None;
            }
        }
    })
}

/// Event nodes with fixed names and signatures.
fn builtin_event(node_type: &str) -> Option<(&'static str, Vec<(&'static str, Type)>)> {
    Some(match node_type {
        "begin_play" => ("begin_play", vec![]),
        "on_tick" => ("tick", vec![("delta_time", Type::Float)]),
        "on_end_play" => ("end_play", vec![]),
        "main" => ("main", vec![]),
        "on_input_key" => ("on_input_key", vec![("key", Type::Str), ("pressed", Type::Bool)]),
        "on_input_action" => ("on_input_action", vec![("action", Type::Str), ("pressed", Type::Bool)]),
        _ => return None,
    })
}

const DELTA_TIME: &str = "__bp_delta_time";

type PinKey = (String, String);

/// One function being compiled.
struct Func {
    registers: Vec<Type>,
    code: Vec<Instr>,
    /// Registers holding node outputs that persist across the function:
    /// impure node results, event parameters, loop state.
    outputs: HashMap<PinKey, Reg>,
    /// Pure values computed for the node currently being emitted.
    memo: HashMap<PinKey, Reg>,
    /// Exec path from the event to the node being emitted.
    path: Vec<String>,
}

impl Func {
    fn new(params: &[Type]) -> Self {
        Self {
            registers: params.to_vec(),
            code: Vec::new(),
            outputs: HashMap::new(),
            memo: HashMap::new(),
            path: Vec::new(),
        }
    }

    fn reg(&mut self, ty: Type) -> Reg {
        self.registers.push(ty);
        (self.registers.len() - 1) as Reg
    }

    fn ty(&self, reg: Reg) -> &Type {
        &self.registers[usize::from(reg)]
    }

    fn emit(&mut self, instr: Instr) -> usize {
        self.code.push(instr);
        self.code.len() - 1
    }

    fn here(&self) -> u32 {
        self.code.len() as u32
    }

    /// Point the jump or branch at `at` to `target` (`then` side for
    /// branches unless `otherwise`).
    fn patch(&mut self, at: usize, target: u32, otherwise: bool) {
        match &mut self.code[at] {
            Instr::Jump { target: t } => *t = target,
            Instr::Branch { then, otherwise: o, .. } => {
                if otherwise {
                    *o = target;
                } else {
                    *then = target;
                }
            }
            other => unreachable!("patching {other:?}"),
        }
    }
}

struct EventFn {
    name: String,
    params: Vec<Type>,
    /// Event nodes whose chains make up the body, with their parameter pins.
    nodes: Vec<String>,
    /// For each parameter: the pin ids that read it.
    param_pins: Vec<Vec<String>>,
    /// Engine event subscriptions this function handles.
    subscriptions: Vec<(String, SubscriptionScope)>,
}

struct Compiler<'a> {
    source: &'a ClassSource<'a>,
    natives: &'a NativeRegistry,
    module: Module,
    imports: HashMap<String, u32>,
    vars: HashMap<String, (u32, Type)>,
    /// Data input (node, pin) → source (node, pin).
    data_in: HashMap<PinKey, PinKey>,
    /// Exec output (node, pin) → target nodes, in connection order.
    exec_out: HashMap<PinKey, Vec<String>>,
    events: Vec<EventFn>,
    /// Engine events by name: this class's (qualified) and the known ones.
    event_sigs: HashMap<String, EventSignature>,
    /// uid → qualified name of this class's declared events.
    declared_by_uid: HashMap<String, String>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Compiler<'a> {
    fn new(source: &'a ClassSource<'a>, natives: &'a NativeRegistry) -> Self {
        let mut data_in = HashMap::new();
        let mut exec_out: HashMap<PinKey, Vec<String>> = HashMap::new();
        for c in &source.graph.connections {
            match c.connection_type {
                ConnectionType::Data => {
                    data_in.insert(
                        (c.target_node.clone(), c.target_pin.clone()),
                        (c.source_node.clone(), c.source_pin.clone()),
                    );
                }
                ConnectionType::Execution => {
                    exec_out
                        .entry((c.source_node.clone(), c.source_pin.clone()))
                        .or_default()
                        .push(c.target_node.clone());
                }
            }
        }
        Self {
            source,
            natives,
            module: Module::new(source.name),
            imports: HashMap::new(),
            vars: HashMap::new(),
            data_in,
            exec_out,
            events: Vec::new(),
            event_sigs: HashMap::new(),
            declared_by_uid: HashMap::new(),
            diagnostics: Vec::new(),
        }
    }

    fn error(&mut self, node: Option<&str>, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic { node: node.map(str::to_owned), message: message.into() });
    }

    fn node(&self, id: &str) -> Option<&'a NodeInstance> {
        self.source.graph.nodes.get(id)
    }

    // ---- declarations --------------------------------------------------

    fn declare_variables(&mut self) {
        for var in self.source.variables {
            let Some(ty) = script_type(&var.type_name) else {
                self.error(None, format!("variable `{}`: type `{}` is not available to scripts", var.name, var.type_name));
                continue;
            };
            let default = match &var.default {
                Some(json) => match constant(json, &ty) {
                    Ok(c) => c,
                    Err(message) => {
                        self.error(None, format!("variable `{}` default: {message}", var.name));
                        None
                    }
                },
                None => None,
            };
            self.add_var(&var.name, ty, default);
        }
    }

    fn add_var(&mut self, name: &str, ty: Type, default: Option<Constant>) -> u32 {
        if let Some((index, _)) = self.vars.get(name) {
            return *index;
        }
        let index = self.module.variables.len() as u32;
        self.module.variables.push(Variable { name: name.to_owned(), ty: ty.clone(), default });
        self.vars.insert(name.to_owned(), (index, ty));
        index
    }

    /// A per-instance variable backing one node's hidden state.
    fn hidden_var(&mut self, node: &str, purpose: &str, ty: Type) -> u32 {
        self.add_var(&format!("__bp_{purpose}_{node}"), ty, None)
    }

    /// The class's custom events become engine events; index every event
    /// the graph can use by name.
    fn declare_engine_events(&mut self) {
        for known in self.source.known_events {
            self.event_sigs.insert(known.name.clone(), known.clone());
        }
        for event in self.source.events {
            if event.name.trim().is_empty() {
                self.error(None, format!("custom event {} has no name", event.uid));
                continue;
            }
            let name = qualified_event_name(self.source.name, &event.name);
            let mut fields = Vec::new();
            for (field, type_name) in &event.fields {
                match script_type(type_name).filter(|t| matches!(t, Type::Bool | Type::Int | Type::Float | Type::Str | Type::Entity)) {
                    Some(ty) => fields.push(EventField::new(field.clone(), ty)),
                    None => self.error(None, format!("event `{name}` field `{field}`: type `{type_name}` cannot be an event field (bool, integers, floats, String, Entity)")),
                }
            }
            if self.module.events.iter().any(|e| e.name == name) {
                self.error(None, format!("event `{name}` declared twice"));
                continue;
            }
            let decl = EventDecl { name: name.clone(), fields };
            self.event_sigs.insert(name.clone(), EventSignature::from(&decl));
            self.declared_by_uid.insert(event.uid.replace('-', "_"), name);
            self.module.events.push(decl);
        }
    }

    fn event_sig(&mut self, node: &str, name: &str) -> Option<EventSignature> {
        match self.event_sigs.get(name) {
            Some(sig) => Some(sig.clone()),
            None => {
                self.error(Some(node), format!("no event `{name}` is declared in this project or by the engine"));
                None
            }
        }
    }

    fn declare_events(&mut self) {
        let mut nodes: Vec<&NodeInstance> = self.source.graph.nodes.values().collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for node in nodes {
            if RETIRED_EVENT_NODES.contains(&node.node_type.as_str()) {
                self.error(
                    Some(&node.id),
                    format!(
                        "`{}` was a placeholder and is gone: use the \"On <Event>\", \"Send <Event> to\" and \"Broadcast <Event>\" event nodes",
                        node.node_type
                    ),
                );
                continue;
            }
            let mut subscription = None;
            let (name, params, param_pins) = if let Some(event) = node.node_type.strip_prefix("event::on::") {
                let Some(sig) = self.event_sig(&node.id, event) else { continue };
                let declared_here = self.module.events.iter().any(|e| e.name == event);
                let scope = match node.properties.get("scope") {
                    Some(Json::String(s)) if !s.trim().is_empty() => match parse_scope(s) {
                        Some(scope) => scope,
                        None => {
                            self.error(Some(&node.id), format!("scope `{s}` is not self, global or class"));
                            continue;
                        }
                    },
                    _ => default_scope(&sig, declared_here),
                };
                subscription = Some((event.to_owned(), scope));
                let scope_tag = match scope {
                    SubscriptionScope::Self_ => "self",
                    SubscriptionScope::Global => "global",
                    SubscriptionScope::Class => "class",
                };
                let fn_name = format!("on_event__{}__{scope_tag}", sanitize(event));
                let pins = sig.fields.iter().map(|f| vec![f.name.clone()]).collect();
                (fn_name, sig.field_types(), pins)
            } else if let Some((name, params)) = builtin_event(&node.node_type) {
                // Parameter pins: the declared name, with or without a
                // leading underscore (pulsar_std spells them `_delta_time`).
                let pins = params
                    .iter()
                    .map(|(p, _)| vec![(*p).to_owned(), format!("_{p}")])
                    .collect();
                (name.to_owned(), params.into_iter().map(|(_, t)| t).collect::<Vec<_>>(), pins)
            } else if self.is_custom_event(node) {
                let mut params = Vec::new();
                let mut pins = Vec::new();
                for pin in &node.outputs {
                    if let DataType::Data(info) = &pin.pin.data_type {
                        match script_type(&info.type_string) {
                            Some(ty) => {
                                params.push(ty);
                                pins.push(vec![pin.id.clone()]);
                            }
                            None => self.error(Some(&node.id), format!("event parameter type `{}` is not available to scripts", info.type_string)),
                        }
                    }
                }
                (node.node_type.clone(), params, pins)
            } else {
                continue;
            };
            // A custom event's handler also handles the engine event the
            // class declares for it, sent to this object.
            if subscription.is_none() {
                if let Some(qualified) = node.node_type.strip_prefix("on_").and_then(|uid| self.declared_by_uid.get(uid)) {
                    let declared = self.event_sigs[qualified].field_types();
                    if declared == params {
                        subscription = Some((qualified.clone(), SubscriptionScope::Self_));
                    }
                }
            }
            match by_name.get(&name) {
                Some(&index) => {
                    if self.events[index].params != params {
                        self.error(Some(&node.id), format!("event `{name}` declared twice with different parameters"));
                        continue;
                    }
                    self.events[index].nodes.push(node.id.clone());
                }
                None => {
                    by_name.insert(name.clone(), self.events.len());
                    let subscriptions = subscription.into_iter().collect();
                    self.events.push(EventFn { name, params, nodes: vec![node.id.clone()], param_pins, subscriptions });
                }
            }
        }
        // Function indices follow `events` order.
        for (index, event) in self.events.iter().enumerate() {
            for (name, scope) in &event.subscriptions {
                self.module.subscriptions.push(Subscription {
                    event: EventRef::Name(name.clone()),
                    handler: index as u32,
                    scope: *scope,
                });
            }
            self.module.functions.push(Function {
                name: event.name.clone(),
                exported: true,
                params: event.params.clone(),
                ret: Type::Unit,
                registers: event.params.clone(),
                code: Vec::new(),
            });
        }
        if self.events.iter().any(|e| e.name == "tick")
            || self.source.graph.nodes.values().any(|n| n.node_type == "get_delta_time")
        {
            self.add_var(DELTA_TIME, Type::Float, None);
        }
    }

    fn is_custom_event(&self, node: &NodeInstance) -> bool {
        node.node_type.starts_with("on_")
            && builtin_event(&node.node_type).is_none()
            && self.natives.get(&format!("std::{}", node.node_type)).is_none()
    }

    // ---- bodies --------------------------------------------------------

    fn compile_events(&mut self) {
        for index in 0..self.events.len() {
            let event = &self.events[index];
            let (nodes, params, param_pins, name) =
                (event.nodes.clone(), event.params.clone(), event.param_pins.clone(), event.name.clone());
            let mut f = Func::new(&params);
            for (param, pins) in param_pins.iter().enumerate() {
                for node in &nodes {
                    for pin in pins {
                        f.outputs.insert((node.clone(), pin.clone()), param as Reg);
                    }
                }
            }
            if name == "tick" {
                let (var, _) = self.vars[DELTA_TIME].clone();
                f.emit(Instr::StoreVar { var, src: 0 });
            }
            for node in &nodes {
                self.follow_all_exec(&mut f, node);
            }
            f.emit(Instr::Return { value: None });
            let function = &mut self.module.functions[index];
            function.registers = f.registers;
            function.code = f.code;
        }
    }

    /// Run every exec output of `node`, in pin order.
    fn follow_all_exec(&mut self, f: &mut Func, node: &str) {
        let Some(n) = self.node(node) else { return };
        let pins: Vec<String> = n
            .outputs
            .iter()
            .filter(|p| matches!(p.pin.data_type, DataType::Exec))
            .map(|p| p.id.clone())
            .collect();
        for pin in pins {
            self.follow(f, node, &pin);
        }
    }

    /// Run the chain wired to exec output `pin` of `node`. Pure values
    /// computed inside the chain only exist on its path, so the caller's
    /// memo is restored afterwards.
    fn follow(&mut self, f: &mut Func, node: &str, pin: &str) {
        let targets = self.exec_out.get(&(node.to_owned(), pin.to_owned())).cloned().unwrap_or_default();
        let saved = std::mem::take(&mut f.memo);
        for target in targets {
            self.exec(f, &target);
        }
        f.memo = saved;
    }

    fn exec(&mut self, f: &mut Func, id: &str) {
        if f.path.iter().any(|n| n == id) {
            self.error(Some(id), "execution loops back to this node; use a loop node instead");
            return;
        }
        if f.code.len() > MAX_CODE {
            self.error(Some(id), "graph too large (exec paths merge too often)");
            return;
        }
        let Some(node) = self.node(id) else {
            self.error(Some(id), "exec wire to a missing node");
            return;
        };
        f.path.push(id.to_owned());
        f.memo.clear();
        self.exec_node(f, node);
        f.path.pop();
    }

    fn exec_node(&mut self, f: &mut Func, node: &'a NodeInstance) {
        let id = node.id.as_str();
        let ty = node.node_type.as_str();

        if ty == "reroute" {
            return self.follow_all_exec(f, id);
        }
        if let Some(var) = ty.strip_prefix("set_").filter(|v| self.vars.contains_key(*v)) {
            let (index, var_ty) = self.vars[var].clone();
            if let Some(value) = self.input(f, node, "value", &var_ty) {
                f.emit(Instr::StoreVar { var: index, src: value });
                for pin in data_outputs(node) {
                    f.outputs.insert((id.to_owned(), pin), value);
                }
            }
            return self.follow_all_exec(f, id);
        }
        if self.flow(f, node) {
            return;
        }
        if self.selector(f, node) {
            return;
        }
        if ty == "emit_custom_event" {
            self.emit_custom_event(f, node);
            return self.follow_all_exec(f, id);
        }
        if let Some((kind, event)) = ty.strip_prefix("event::").and_then(|rest| rest.split_once("::")) {
            if kind != "on" {
                self.send_event(f, node, kind, event);
                return self.follow_all_exec(f, id);
            }
        }
        if let Some(name) = ty.strip_prefix("native::") {
            self.native_call(f, node, name);
            return self.follow_all_exec(f, id);
        }
        if let Some(rest) = ty.strip_prefix("comp_set_prop::") {
            if rest.split_once("::").is_none() {
                self.error(Some(id), "malformed property node");
            }
            if let Some((class, prop)) = rest.split_once("::") {
                let component = self.component_input(f, node, class);
                let native = format!("{class}::set_{prop}");
                if let (Some(component), Some(sig)) = (component, self.native_sig(id, &native)) {
                    let value_ty = sig.params[1].ty.clone();
                    if let Some(value) = self.input(f, node, "value", &value_ty) {
                        self.call(f, &native, vec![component, value], None);
                    }
                }
            }
            return self.follow_all_exec(f, id);
        }
        if let Some(rest) = ty.strip_prefix("comp_call::") {
            match rest.split_once("::") {
                Some((class, method)) => {
                    self.component_call(f, node, class, method);
                }
                None => self.error(Some(id), "malformed method node"),
            }
            return self.follow_all_exec(f, id);
        }
        let is_value_node = ty.starts_with("comp_get_prop::")
            || ty.starts_with("get_component_ref::")
            || matches!(ty, "find_object_by_stable_id" | "find_object_by_name" | "object_ref_literal")
            || ty == "get_delta_time"
            || ty.strip_prefix("get_").is_some_and(|v| self.vars.contains_key(v));
        if is_value_node {
            // Value nodes placed on an exec wire: evaluate for their
            // outputs, then continue.
            for pin in data_outputs(node) {
                self.value(f, id, &pin);
            }
            return self.follow_all_exec(f, id);
        }
        self.std_call(f, node);
        self.follow_all_exec(f, id);
    }

    /// A `std::<node_type>` call: arguments from the pins named after the
    /// native's parameters, the result into the node's `result` pin.
    fn std_call(&mut self, f: &mut Func, node: &'a NodeInstance) -> Option<Reg> {
        let id = node.id.as_str();
        let native_name = format!("std::{}", node.node_type);
        let Some(native) = self.natives.get(&native_name).cloned() else {
            self.error(Some(id), format!("`{}` is not available to scripts", node.node_type));
            return None;
        };
        let mut args = Vec::with_capacity(native.sig.params.len());
        for (name, param) in native.param_names.iter().zip(&native.sig.params) {
            let pin = node
                .inputs
                .iter()
                .find(|p| p.id == *name || p.id.trim_start_matches('_') == name)
                .map(|p| p.id.clone())
                .unwrap_or_else(|| name.clone());
            args.push(self.input(f, node, &pin, &param.ty)?);
        }
        let dst = (native.sig.ret != Type::Unit).then(|| self.output_reg(f, id, "result", native.sig.ret.clone()));
        self.call(f, &native_name, args, dst);
        dst
    }

    fn component_call(&mut self, f: &mut Func, node: &'a NodeInstance, class: &str, method: &str) -> Option<Reg> {
        let id = node.id.as_str();
        let native_name = format!("{class}::{method}");
        let native = self.natives.get(&native_name).cloned();
        let Some(native) = native else {
            self.error(Some(id), format!("`{native_name}` is not available to scripts"));
            return None;
        };
        let component = self.component_input(f, node, class)?;
        let mut args = vec![component];
        for (name, param) in native.param_names.iter().zip(&native.sig.params).skip(1) {
            args.push(self.input(f, node, name, &param.ty)?);
        }
        let dst = (native.sig.ret != Type::Unit)
            .then(|| self.output_reg(f, id, "return_value", native.sig.ret.clone()));
        self.call(f, &native_name, args, dst);
        dst
    }

    /// A `native::<name>` node: any registered native, pins named after its
    /// parameters. A method's unconnected receiver (`self`) is this entity's
    /// component, or the type's default value. `inout` parameters are
    /// passed as copies and come back out on the output pin of the same
    /// name; the return value is `result`.
    fn native_call(&mut self, f: &mut Func, node: &'a NodeInstance, name: &str) -> Option<()> {
        let id = node.id.as_str();
        let Some(native) = self.natives.get(name).cloned() else {
            self.error(Some(id), format!("`{name}` is not available to scripts"));
            return None;
        };
        let mut args = Vec::with_capacity(native.sig.params.len());
        for (index, (pin, param)) in native.param_names.iter().zip(&native.sig.params).enumerate() {
            let wired = self.data_in.contains_key(&(id.to_owned(), pin.clone()));
            let reg = match (&param.ty, index == 0 && native.receiver.is_some() && !wired) {
                (Type::Component(class), true) => self.self_component(f, id, class)?,
                _ => self.input(f, node, pin, &param.ty)?,
            };
            let reg = if param.inout {
                // The callee writes the argument back: never into the
                // register some other node produced.
                let copy = f.reg(param.ty.clone());
                f.emit(Instr::Move { dst: copy, src: reg });
                f.outputs.insert((id.to_owned(), pin.clone()), copy);
                copy
            } else {
                reg
            };
            args.push(reg);
        }
        let dst = (native.sig.ret != Type::Unit).then(|| self.output_reg(f, id, "result", native.sig.ret.clone()));
        self.call(f, name, args, dst);
        Some(())
    }

    /// "Send <Event> to" (`send`), "Broadcast <Event>" (`broadcast`) and
    /// "Send <Event> to Class" (`to_class`): the event's fields from the
    /// pins named after them.
    fn send_event(&mut self, f: &mut Func, node: &'a NodeInstance, kind: &str, event: &str) {
        let id = node.id.as_str();
        let (native, leading): (&str, Vec<(&str, Type)>) = match kind {
            "send" => ("event::send", vec![("target", Type::Entity)]),
            "broadcast" => ("event::emit", vec![]),
            "to_class" => ("event::emit_to_class", vec![("class", Type::Str)]),
            _ => return self.error(Some(id), format!("unknown event node kind `{kind}`")),
        };
        let Some(sig) = self.event_sig(id, event) else { return };
        let mut args = Vec::new();
        let mut params = Vec::new();
        for (pin, ty) in &leading {
            let Some(reg) = self.input(f, node, pin, ty) else { return };
            args.push(reg);
            params.push(Param::new(ty.clone()));
        }
        args.push(self.konst(f, Constant::Str(event.to_owned())));
        params.push(Param::new(Type::Str));
        for field in &sig.fields {
            let Some(reg) = self.input(f, node, &field.name, &field.ty) else { return };
            args.push(reg);
            params.push(Param::new(field.ty.clone()));
        }
        let import = format!("{native}@{event}");
        self.call_with_sig(f, &import, Signature::new(params, Type::Unit), args, None);
    }

    fn emit_custom_event(&mut self, f: &mut Func, node: &'a NodeInstance) {
        let id = node.id.as_str();
        let uid = match node.properties.get("event_uid") {
            Some(Json::String(uid)) => uid.clone(),
            _ => return self.error(Some(id), "custom event call without an event"),
        };
        let target = format!("on_{}", uid.replace('-', "_"));
        let Some(index) = self.events.iter().position(|e| e.name == target) else {
            return self.error(Some(id), "custom event has no handler in this graph");
        };
        let params = self.events[index].params.clone();
        let pins: Vec<String> = data_inputs(node);
        if pins.len() != params.len() {
            return self.error(Some(id), format!("custom event takes {} values, the call passes {}", params.len(), pins.len()));
        }
        let mut args = Vec::new();
        for (pin, ty) in pins.iter().zip(&params) {
            match self.input(f, node, pin, ty) {
                Some(reg) => args.push(reg),
                None => return,
            }
        }
        f.emit(Instr::Call { func: index as u32, args, dst: None });
    }

    // ---- flow intrinsics -----------------------------------------------

    /// Emit a flow node; `false` if `node` is not one.
    fn flow(&mut self, f: &mut Func, node: &'a NodeInstance) -> bool {
        let id = node.id.as_str();
        match node.node_type.as_str() {
            "branch" | "switch_on_bool" => {
                let pin = if node.node_type == "branch" { "condition" } else { "value" };
                if let Some(cond) = self.input(f, node, pin, &Type::Bool) {
                    self.if_else(f, cond, |c, f| c.follow(f, id, "True"), |c, f| c.follow(f, id, "False"));
                }
            }
            "multi_branch" => {
                let mut cases = Vec::new();
                for (i, pin) in ["condition1", "condition2", "condition3"].iter().enumerate() {
                    cases.push((Case::Pin(pin), format!("Branch{}", i + 1)));
                }
                self.cases(f, node, cases, Some("Else"));
            }
            "switch_on_int" => {
                if let Some(v) = self.input(f, node, "value", &Type::Int) {
                    let cases = (0..4).map(|k| (Case::Eq(v, Constant::Int(k)), format!("Case{k}"))).collect();
                    self.cases(f, node, cases, Some("Default"));
                }
            }
            "switch_on_string" => {
                if let Some(v) = self.input(f, node, "value", &Type::Str) {
                    let cases = (1..=3)
                        .map(|k| (Case::Eq(v, Constant::Str(format!("option{k}"))), format!("Option{k}")))
                        .collect();
                    self.cases(f, node, cases, Some("Default"));
                }
            }
            "range_switch" => {
                if let Some(v) = self.input(f, node, "value", &Type::Float) {
                    let cases = [(0.0, "Negative"), (10.0, "Low"), (50.0, "Medium"), (100.0, "High")]
                        .into_iter()
                        .map(|(limit, pin)| (Case::Lt(v, Constant::Float(limit)), pin.to_owned()))
                        .collect();
                    self.cases(f, node, cases, Some("Extreme"));
                }
            }
            "string_contains_switch" => {
                if let Some(text) = self.input(f, node, "text", &Type::Str) {
                    let mut cases = Vec::new();
                    for k in 1..=3 {
                        if let Some(pattern) = self.input(f, node, &format!("pattern{k}"), &Type::Str) {
                            cases.push((Case::Contains(text, pattern), format!("Contains{k}")));
                        }
                    }
                    self.cases(f, node, cases, Some("None"));
                }
            }
            "sequence" => {
                for k in 0..4 {
                    self.follow(f, id, &format!("Then{k}"));
                }
            }
            "for_loop" => {
                let Some(count) = self.input(f, node, "count", &Type::Int) else { return true };
                let i = self.konst(f, Constant::Int(0));
                let one = self.konst(f, Constant::Int(1));
                let head = f.here();
                let cond = f.reg(Type::Bool);
                f.emit(Instr::Binary { op: BinOp::Lt, dst: cond, a: i, b: count });
                let at = f.emit(Instr::Branch { cond, then: 0, otherwise: 0 });
                let body = f.here();
                f.patch(at, body, false);
                self.follow(f, id, "Body");
                f.emit(Instr::Binary { op: BinOp::Add, dst: i, a: i, b: one });
                f.emit(Instr::Jump { target: head });
                let exit = f.here();
                f.patch(at, exit, true);
            }
            "while_loop" => {
                let head = f.here();
                // Re-evaluate the condition every iteration.
                f.memo.clear();
                let Some(cond) = self.input(f, node, "condition", &Type::Bool) else { return true };
                let at = f.emit(Instr::Branch { cond, then: 0, otherwise: 0 });
                let body = f.here();
                f.patch(at, body, false);
                self.follow(f, id, "Body");
                f.emit(Instr::Jump { target: head });
                let exit = f.here();
                f.patch(at, exit, true);
            }
            "gate" => {
                let state = self.hidden_var(id, "gate_open", Type::Bool);
                if let Some(open) = self.input(f, node, "open", &Type::Bool) {
                    let t = self.konst(f, Constant::Bool(true));
                    self.if_then(f, open, |_, f| {
                        f.emit(Instr::StoreVar { var: state, src: t });
                    });
                }
                if let Some(close) = self.input(f, node, "close", &Type::Bool) {
                    let no = self.konst(f, Constant::Bool(false));
                    self.if_then(f, close, |_, f| {
                        f.emit(Instr::StoreVar { var: state, src: no });
                    });
                }
                let is_open = self.load(f, state, Type::Bool);
                self.if_then(f, is_open, |c, f| c.follow(f, id, "Then"));
            }
            "multi_gate" => {
                let index = self.hidden_var(id, "multi_gate_index", Type::Int);
                let Some(reset) = self.input(f, node, "reset", &Type::Bool) else { return true };
                self.if_else(
                    f,
                    reset,
                    |c, f| {
                        let zero = c.konst(f, Constant::Int(0));
                        f.emit(Instr::StoreVar { var: index, src: zero });
                    },
                    |c, f| {
                        let current = c.load(f, index, Type::Int);
                        let four = c.konst(f, Constant::Int(4));
                        let one = c.konst(f, Constant::Int(1));
                        let slot = f.reg(Type::Int);
                        f.emit(Instr::Binary { op: BinOp::Rem, dst: slot, a: current, b: four });
                        let next = f.reg(Type::Int);
                        f.emit(Instr::Binary { op: BinOp::Add, dst: next, a: current, b: one });
                        f.emit(Instr::StoreVar { var: index, src: next });
                        let cases = (0..4).map(|k| (Case::Eq(slot, Constant::Int(k)), format!("Output{k}"))).collect();
                        c.cases(f, node, cases, None);
                    },
                );
            }
            "flip_flop" => {
                let state = self.hidden_var(id, "flip_flop", Type::Bool);
                let current = self.load(f, state, Type::Bool);
                let flipped = f.reg(Type::Bool);
                f.emit(Instr::Unary { op: UnOp::Not, dst: flipped, src: current });
                f.emit(Instr::StoreVar { var: state, src: flipped });
                self.if_else(f, current, |c, f| c.follow(f, id, "A"), |c, f| c.follow(f, id, "B"));
            }
            "do_once" => {
                let done = self.hidden_var(id, "do_once", Type::Bool);
                let Some(reset) = self.input(f, node, "reset", &Type::Bool) else { return true };
                self.if_else(
                    f,
                    reset,
                    |c, f| {
                        let no = c.konst(f, Constant::Bool(false));
                        f.emit(Instr::StoreVar { var: done, src: no });
                    },
                    |c, f| {
                        let was_done = c.load(f, done, Type::Bool);
                        let first = f.reg(Type::Bool);
                        f.emit(Instr::Unary { op: UnOp::Not, dst: first, src: was_done });
                        c.if_then(f, first, |c, f| {
                            let yes = c.konst(f, Constant::Bool(true));
                            f.emit(Instr::StoreVar { var: done, src: yes });
                            c.follow(f, id, "Then");
                        });
                    },
                );
            }
            // Latent: the call waits (the runtime resumes it after the
            // delay). While a delay is counting down, `delay` ignores new
            // triggers and `retriggerable_delay` restarts the countdown.
            "delay" => {
                let active = self.hidden_var(id, "delay_active", Type::Bool);
                let Some(seconds) = self.seconds_input(f, node, "milliseconds") else { return true };
                let busy = self.load(f, active, Type::Bool);
                let idle = f.reg(Type::Bool);
                f.emit(Instr::Unary { op: UnOp::Not, dst: idle, src: busy });
                self.if_then(f, idle, |c, f| {
                    let yes = c.konst(f, Constant::Bool(true));
                    f.emit(Instr::StoreVar { var: active, src: yes });
                    f.emit(Instr::Wait { seconds });
                    let no = c.konst(f, Constant::Bool(false));
                    f.emit(Instr::StoreVar { var: active, src: no });
                    c.follow_all_exec(f, id);
                });
            }
            "retriggerable_delay" => {
                let active = self.hidden_var(id, "retrigger_active", Type::Bool);
                let deadline = self.hidden_var(id, "retrigger_deadline", Type::Float);
                let Some(seconds) = self.seconds_input(f, node, "delay_ms") else { return true };
                let now = f.reg(Type::Float);
                f.emit(Instr::Now { dst: now });
                let until = f.reg(Type::Float);
                f.emit(Instr::Binary { op: BinOp::Add, dst: until, a: now, b: seconds });
                f.emit(Instr::StoreVar { var: deadline, src: until });
                let busy = self.load(f, active, Type::Bool);
                let idle = f.reg(Type::Bool);
                f.emit(Instr::Unary { op: UnOp::Not, dst: idle, src: busy });
                self.if_then(f, idle, |c, f| {
                    let yes = c.konst(f, Constant::Bool(true));
                    f.emit(Instr::StoreVar { var: active, src: yes });
                    // Wait until the (possibly extended) deadline passes.
                    let zero = c.konst(f, Constant::Float(0.0));
                    let head = f.here();
                    let now = f.reg(Type::Float);
                    f.emit(Instr::Now { dst: now });
                    let target = c.load(f, deadline, Type::Float);
                    let remaining = f.reg(Type::Float);
                    f.emit(Instr::Binary { op: BinOp::Sub, dst: remaining, a: target, b: now });
                    let pending = f.reg(Type::Bool);
                    f.emit(Instr::Binary { op: BinOp::Gt, dst: pending, a: remaining, b: zero });
                    let at = f.emit(Instr::Branch { cond: pending, then: 0, otherwise: 0 });
                    let wait = f.here();
                    f.patch(at, wait, false);
                    f.emit(Instr::Wait { seconds: remaining });
                    f.emit(Instr::Jump { target: head });
                    let done = f.here();
                    f.patch(at, done, true);
                    let no = c.konst(f, Constant::Bool(false));
                    f.emit(Instr::StoreVar { var: active, src: no });
                    c.follow_all_exec(f, id);
                });
            }
            "do_n" => {
                let counter = self.hidden_var(id, "do_n", Type::Int);
                let (Some(n), Some(reset)) = (self.input(f, node, "n", &Type::Int), self.input(f, node, "reset", &Type::Bool))
                else {
                    return true;
                };
                self.if_else(
                    f,
                    reset,
                    |c, f| {
                        let zero = c.konst(f, Constant::Int(0));
                        f.emit(Instr::StoreVar { var: counter, src: zero });
                    },
                    |c, f| {
                        let count = c.load(f, counter, Type::Int);
                        let below = f.reg(Type::Bool);
                        f.emit(Instr::Binary { op: BinOp::Lt, dst: below, a: count, b: n });
                        c.if_then(f, below, |c, f| {
                            let one = c.konst(f, Constant::Int(1));
                            let next = f.reg(Type::Int);
                            f.emit(Instr::Binary { op: BinOp::Add, dst: next, a: count, b: one });
                            f.emit(Instr::StoreVar { var: counter, src: next });
                            c.follow(f, id, "Then");
                        });
                    },
                );
            }
            _ => return false,
        }
        true
    }

    /// A milliseconds input as seconds (`float`).
    fn seconds_input(&mut self, f: &mut Func, node: &'a NodeInstance, pin: &str) -> Option<Reg> {
        let ms = self.input(f, node, pin, &Type::Int)?;
        let ms = self.coerce(f, &node.id, ms, &Type::Float)?;
        let thousand = self.konst(f, Constant::Float(1000.0));
        let seconds = f.reg(Type::Float);
        f.emit(Instr::Binary { op: BinOp::Div, dst: seconds, a: ms, b: thousand });
        Some(seconds)
    }

    /// Any other control-flow node: its selector native (`std::<node>`
    /// with an `exec_outputs` attribute) runs the node's own body and
    /// reports which exec output fired; jump there. A value the node
    /// returns comes back on `result`. `false` if the node has none.
    fn selector(&mut self, f: &mut Func, node: &'a NodeInstance) -> bool {
        let id = node.id.as_str();
        let name = format!("std::{}", node.node_type);
        let Some(native) = self.natives.get(&name).cloned() else { return false };
        let Some(labels) = native.attr("exec_outputs") else { return false };
        let labels: Vec<String> = labels.split(',').map(str::to_owned).collect();
        let mut args = Vec::with_capacity(native.sig.params.len());
        for (pin, param) in native.param_names.iter().zip(&native.sig.params) {
            let reg = if param.inout && pin == "result" {
                self.output_reg(f, id, "result", param.ty.clone())
            } else {
                let input = node
                    .inputs
                    .iter()
                    .find(|p| p.id == *pin || p.id.trim_start_matches('_') == pin)
                    .map(|p| p.id.clone())
                    .unwrap_or_else(|| pin.clone());
                match self.input(f, node, &input, &param.ty) {
                    Some(reg) => reg,
                    None => return true,
                }
            };
            args.push(reg);
        }
        let fired = f.reg(Type::Int);
        self.call(f, &name, args, Some(fired));
        let cases = labels
            .into_iter()
            .enumerate()
            .map(|(index, label)| (Case::Eq(fired, Constant::Int(index as i64)), label))
            .collect();
        self.cases(f, node, cases, None);
        true
    }

    fn if_then(&mut self, f: &mut Func, cond: Reg, then: impl FnOnce(&mut Self, &mut Func)) {
        self.if_else(f, cond, then, |_, _| {});
    }

    fn if_else(
        &mut self,
        f: &mut Func,
        cond: Reg,
        then: impl FnOnce(&mut Self, &mut Func),
        otherwise: impl FnOnce(&mut Self, &mut Func),
    ) {
        let at = f.emit(Instr::Branch { cond, then: 0, otherwise: 0 });
        let then_pc = f.here();
        f.patch(at, then_pc, false);
        then(self, f);
        let skip = f.emit(Instr::Jump { target: 0 });
        let else_pc = f.here();
        f.patch(at, else_pc, true);
        otherwise(self, f);
        let end = f.here();
        f.patch(skip, end, false);
    }

    /// `if case1 {pin1} else if case2 {pin2} .. else {default}`.
    fn cases(&mut self, f: &mut Func, node: &'a NodeInstance, cases: Vec<(Case<'_>, String)>, default: Option<&str>) {
        let id = node.id.as_str();
        let mut ends = Vec::new();
        for (case, pin) in cases {
            let cond = match case {
                Case::Pin(pin) => match self.input(f, node, pin, &Type::Bool) {
                    Some(reg) => reg,
                    None => continue,
                },
                Case::Eq(value, constant) => {
                    let k = self.konst(f, constant);
                    let cond = f.reg(Type::Bool);
                    f.emit(Instr::Binary { op: BinOp::Eq, dst: cond, a: value, b: k });
                    cond
                }
                Case::Lt(value, constant) => {
                    let k = self.konst(f, constant);
                    let cond = f.reg(Type::Bool);
                    f.emit(Instr::Binary { op: BinOp::Lt, dst: cond, a: value, b: k });
                    cond
                }
                Case::Contains(text, pattern) => {
                    let cond = f.reg(Type::Bool);
                    self.call(f, "string::contains", vec![text, pattern], Some(cond));
                    cond
                }
            };
            let at = f.emit(Instr::Branch { cond, then: 0, otherwise: 0 });
            let then_pc = f.here();
            f.patch(at, then_pc, false);
            self.follow(f, id, &pin);
            ends.push(f.emit(Instr::Jump { target: 0 }));
            let next = f.here();
            f.patch(at, next, true);
        }
        if let Some(default) = default {
            self.follow(f, id, default);
        }
        let end = f.here();
        for at in ends {
            f.patch(at, end, false);
        }
    }

    // ---- values --------------------------------------------------------

    /// The register holding output `pin` of `node` across the function.
    fn output_reg(&mut self, f: &mut Func, node: &str, pin: &str, ty: Type) -> Reg {
        let key = (node.to_owned(), pin.to_owned());
        if let Some(&reg) = f.outputs.get(&key) {
            return reg;
        }
        let reg = f.reg(ty);
        f.outputs.insert(key, reg);
        reg
    }

    /// Input `pin` of `node` as a value of type `ty`: its wire, its
    /// literal, or the type's default.
    fn input(&mut self, f: &mut Func, node: &'a NodeInstance, pin: &str, ty: &Type) -> Option<Reg> {
        let key = (node.id.clone(), pin.to_owned());
        if let Some((src_node, src_pin)) = self.data_in.get(&key).cloned() {
            let reg = self.value(f, &src_node, &src_pin)?;
            return self.coerce(f, &node.id, reg, ty);
        }
        if let Some(json) = node.properties.get(pin) {
            return match constant(json, ty) {
                Ok(Some(c)) => Some(self.konst(f, c)),
                Ok(None) => Some(f.reg(ty.clone())),
                Err(message) => {
                    self.error(Some(&node.id), format!("pin `{pin}`: {message}"));
                    None
                }
            };
        }
        Some(f.reg(ty.clone()))
    }

    fn coerce(&mut self, f: &mut Func, node: &str, reg: Reg, ty: &Type) -> Option<Reg> {
        let have = f.ty(reg).clone();
        if &have == ty {
            return Some(reg);
        }
        match (&have, ty) {
            (Type::Int, Type::Float) => {
                let dst = f.reg(Type::Float);
                f.emit(Instr::Unary { op: UnOp::IntToFloat, dst, src: reg });
                Some(dst)
            }
            (_, Type::Str) if !matches!(have, Type::Object(_)) => {
                let dst = f.reg(Type::Str);
                f.emit(Instr::Unary { op: UnOp::ToStr, dst, src: reg });
                Some(dst)
            }
            _ => {
                self.error(Some(node), format!("expected {ty}, got {have}"));
                None
            }
        }
    }

    /// The value of output `pin` of `node`.
    fn value(&mut self, f: &mut Func, id: &str, pin: &str) -> Option<Reg> {
        let key = (id.to_owned(), pin.to_owned());
        if let Some(&reg) = f.outputs.get(&key).or_else(|| f.memo.get(&key)) {
            return Some(reg);
        }
        let Some(node) = self.node(id) else {
            self.error(Some(id), "data wire from a missing node");
            return None;
        };
        let ty = node.node_type.as_str();
        let reg = if ty == "reroute" {
            let source = data_inputs(node)
                .into_iter()
                .next()
                .and_then(|input| self.data_in.get(&(id.to_owned(), input)).cloned());
            let Some((src_node, src_pin)) = source else {
                self.error(Some(id), "reroute has no input");
                return None;
            };
            self.value(f, &src_node, &src_pin)?
        } else if let Some(var) = ty.strip_prefix("get_").filter(|v| self.vars.contains_key(*v)) {
            let (index, var_ty) = self.vars[var].clone();
            self.load(f, index, var_ty)
        } else if ty == "get_delta_time" {
            let (index, _) = self.vars[DELTA_TIME].clone();
            self.load(f, index, Type::Float)
        } else if let Some(rest) = ty.strip_prefix("comp_get_prop::") {
            let Some((class, prop)) = rest.split_once("::") else {
                self.error(Some(id), "malformed property node");
                return None;
            };
            let native = format!("{class}::get_{prop}");
            let sig = self.native_sig(id, &native)?;
            let component = self.component_input(f, node, class)?;
            let dst = f.reg(sig.ret.clone());
            self.call(f, &native, vec![component], Some(dst));
            dst
        } else if let Some(rest) = ty.strip_prefix("get_component_ref::") {
            // `Class` or `Class::index`. A node with a `slot_id` property
            // names one component slot of this class (a UUID from the
            // class's prefab). The module declares a hidden variable of the
            // slot's component type (`__slot:<uuid>`, see
            // [`slot_variable_name`]); the runtime fills it once, when the
            // script instance is bound to a placed class, with a handle to
            // that instance's real component. The node then just reads the
            // handle. Older nodes have no slot id and resolve by class name
            // on this entity (the index cannot apply: SceneDB holds one
            // component per type per entity). A wired entity input means
            // another object: slot ids are this class's own, so that object's
            // component is found by class name.
            let class = rest.split("::").next().unwrap_or(rest);
            let slot = node
                .properties
                .get("slot_id")
                .and_then(Json::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            let wired = data_inputs(node).into_iter().find(|p| self.data_in.contains_key(&(id.to_owned(), p.clone())));
            match (wired, slot) {
                (Some(pin), _) => {
                    let entity = self.input(f, node, &pin, &Type::Entity)?;
                    self.component_of(f, id, class, entity)?
                }
                (None, None) => self.self_component(f, id, class)?,
                (None, Some(slot)) => {
                    // The class must be a script-visible component type.
                    self.native_sig(id, &format!("{class}::of"))?;
                    let ty = Type::Component(class.to_owned());
                    let var = self.add_var(&slot_variable_name(&slot), ty.clone(), None);
                    self.load(f, var, ty)
                }
            }
        } else if matches!(ty, "find_object_by_stable_id" | "find_object_by_name" | "object_ref_literal") {
            let native = if ty == "find_object_by_name" { "world::find_by_name" } else { "world::find_by_stable_id" };
            self.native_sig(id, native)?;
            let needle = if ty == "object_ref_literal" {
                // `stable_id` property: a string, or the saved
                // `{stable_id, class_name, component_index}` object.
                let stable_id = match node.properties.get("stable_id").or_else(|| node.properties.get("object")) {
                    Some(Json::String(s)) => s.clone(),
                    Some(Json::Object(o)) => o.get("stable_id").and_then(Json::as_str).unwrap_or_default().to_owned(),
                    _ => {
                        self.error(Some(id), "object reference without a stable id");
                        return None;
                    }
                };
                self.konst(f, Constant::Str(stable_id))
            } else {
                let Some(pin) = data_inputs(node).into_iter().next() else {
                    self.error(Some(id), "lookup node without an input");
                    return None;
                };
                self.input(f, node, &pin, &Type::Str)?
            };
            let entity = f.reg(Type::Entity);
            self.call(f, native, vec![needle], Some(entity));
            entity
        } else if let Some(rest) = ty.strip_prefix("comp_call::") {
            // A call's return value read before (or without) the call runs
            // on this path: its last value.
            let Some((class, method)) = rest.split_once("::") else {
                self.error(Some(id), "malformed method node");
                return None;
            };
            let sig = self.native_sig(id, &format!("{class}::{method}"))?;
            self.output_reg(f, id, pin, sig.ret)
        } else if let Some(name) = ty.strip_prefix("native::") {
            let Some(native) = self.natives.get(name).cloned() else {
                self.error(Some(id), format!("`{name}` is not available to scripts"));
                return None;
            };
            let pin_ty = if pin == "result" && native.sig.ret != Type::Unit {
                native.sig.ret.clone()
            } else {
                match native.param_names.iter().zip(&native.sig.params).find(|(n, p)| *n == pin && p.inout) {
                    Some((_, param)) => param.ty.clone(),
                    None => {
                        self.error(Some(id), format!("`{name}` has no output `{pin}`"));
                        return None;
                    }
                }
            };
            if node_has_exec_input(node) {
                // Impure: the value from its last execution.
                self.output_reg(f, id, pin, pin_ty)
            } else {
                self.native_call(f, node, name)?;
                // Pure: recomputed per consuming node, so move its outputs
                // from the function-wide map into this node's memo.
                let outputs: Vec<PinKey> =
                    f.outputs.keys().filter(|(n, _)| n == id).cloned().collect();
                for out in outputs {
                    let reg = f.outputs.remove(&out).expect("listed");
                    f.memo.insert(out, reg);
                }
                *f.memo.get(&key).expect("native_call defines every output")
            }
        } else if builtin_event(ty).is_some() || self.is_custom_event(node) {
            self.error(Some(id), "event parameters can only be read inside that event");
            return None;
        } else {
            let native = self.natives.get(&format!("std::{ty}")).cloned();
            match native {
                Some(native) if node_has_exec_input(node) => {
                    // Impure: the value from its last execution.
                    self.output_reg(f, id, pin, native.sig.ret.clone())
                }
                Some(_) => {
                    let reg = self.std_call(f, node)?;
                    // Pure results are recomputed per consuming node.
                    f.outputs.remove(&key);
                    reg
                }
                None => {
                    self.error(Some(id), format!("`{ty}` is not available to scripts"));
                    return None;
                }
            }
        };
        f.memo.insert(key, reg);
        Some(reg)
    }

    /// The component reference a component node acts on: its
    /// `component_ref` input, or this entity's `class` component.
    fn component_input(&mut self, f: &mut Func, node: &'a NodeInstance, class: &str) -> Option<Reg> {
        let ty = Type::Component(class.to_owned());
        if self.data_in.contains_key(&(node.id.clone(), "component_ref".to_owned())) {
            return self.input(f, node, "component_ref", &ty);
        }
        self.self_component(f, &node.id, class)
    }

    /// `Class::of(entity)`.
    fn component_of(&mut self, f: &mut Func, node: &str, class: &str, entity: Reg) -> Option<Reg> {
        let native = format!("{class}::of");
        self.native_sig(node, &native)?;
        let component = f.reg(Type::Component(class.to_owned()));
        self.call(f, &native, vec![entity], Some(component));
        Some(component)
    }

    fn self_component(&mut self, f: &mut Func, node: &str, class: &str) -> Option<Reg> {
        let native = format!("{class}::of");
        self.native_sig(node, &native)?;
        let entity = f.reg(Type::Entity);
        f.emit(Instr::SelfEntity { dst: entity });
        let component = f.reg(Type::Component(class.to_owned()));
        self.call(f, &native, vec![entity], Some(component));
        Some(component)
    }

    fn native_sig(&mut self, node: &str, name: &str) -> Option<Signature> {
        match self.natives.get(name) {
            Some(native) => Some(native.sig.clone()),
            None => {
                self.error(Some(node), format!("`{name}` is not available to scripts"));
                None
            }
        }
    }

    /// Call an import with an explicit signature (a polymorphic native
    /// under a tagged name).
    fn call_with_sig(&mut self, f: &mut Func, name: &str, sig: Signature, args: Vec<Reg>, dst: Option<Reg>) {
        let import = match self.imports.get(name) {
            Some(&index) => index,
            None => {
                self.module.imports.push(Import { name: name.to_owned(), sig });
                let index = (self.module.imports.len() - 1) as u32;
                self.imports.insert(name.to_owned(), index);
                index
            }
        };
        f.emit(Instr::CallNative { import, args, dst });
    }

    fn call(&mut self, f: &mut Func, name: &str, args: Vec<Reg>, dst: Option<Reg>) {
        let import = match self.imports.get(name) {
            Some(&index) => index,
            None => {
                let sig = self.natives.get(name).map(|n| n.sig.clone()).expect("checked by callers");
                self.module.imports.push(Import { name: name.to_owned(), sig });
                let index = (self.module.imports.len() - 1) as u32;
                self.imports.insert(name.to_owned(), index);
                index
            }
        };
        f.emit(Instr::CallNative { import, args, dst });
    }

    fn konst(&mut self, f: &mut Func, constant: Constant) -> Reg {
        let index = match self.module.constants.iter().position(|c| *c == constant) {
            Some(index) => index,
            None => {
                self.module.constants.push(constant.clone());
                self.module.constants.len() - 1
            }
        } as u32;
        let dst = f.reg(constant.ty());
        f.emit(Instr::Const { dst, index });
        dst
    }

    fn load(&mut self, f: &mut Func, var: u32, ty: Type) -> Reg {
        let dst = f.reg(ty);
        f.emit(Instr::LoadVar { dst, var });
        dst
    }
}

enum Case<'p> {
    /// A boolean input pin.
    Pin(&'p str),
    Eq(Reg, Constant),
    Lt(Reg, Constant),
    Contains(Reg, Reg),
}

/// An identifier-safe version of an event name.
fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

fn data_inputs(node: &NodeInstance) -> Vec<String> {
    node.inputs
        .iter()
        .filter(|p| !matches!(p.pin.data_type, DataType::Exec))
        .map(|p| p.id.clone())
        .collect()
}

fn data_outputs(node: &NodeInstance) -> Vec<String> {
    node.outputs
        .iter()
        .filter(|p| !matches!(p.pin.data_type, DataType::Exec))
        .map(|p| p.id.clone())
        .collect()
}

fn node_has_exec_input(node: &NodeInstance) -> bool {
    node.inputs.iter().any(|p| matches!(p.pin.data_type, DataType::Exec))
}

/// A literal for type `ty` from an editor property. `Ok(None)`: use the
/// type's default.
fn constant(json: &Json, ty: &Type) -> Result<Option<Constant>, String> {
    let bad = || format!("`{json}` is not a valid {ty}");
    Ok(Some(match ty {
        Type::Bool => match json {
            Json::Bool(b) => Constant::Bool(*b),
            Json::String(s) => Constant::Bool(s.trim().parse().map_err(|_| bad())?),
            _ => return Err(bad()),
        },
        Type::Int => match json {
            Json::Number(n) => Constant::Int(n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).ok_or_else(bad)?),
            Json::String(s) if s.trim().is_empty() => return Ok(None),
            Json::String(s) => Constant::Int(s.trim().parse().map_err(|_| bad())?),
            _ => return Err(bad()),
        },
        Type::Float => match json {
            Json::Number(n) => Constant::Float(n.as_f64().ok_or_else(bad)?),
            Json::String(s) if s.trim().is_empty() => return Ok(None),
            Json::String(s) => Constant::Float(s.trim().parse().map_err(|_| bad())?),
            _ => return Err(bad()),
        },
        Type::Str => match json {
            Json::String(s) => Constant::Str(s.clone()),
            other => Constant::Str(other.to_string()),
        },
        // References and objects have no literals; they start at their
        // default and are set through wires.
        _ => return Ok(None),
    }))
}
