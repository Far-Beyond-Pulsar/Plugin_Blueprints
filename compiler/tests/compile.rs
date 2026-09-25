//! Compile Blueprint graphs and run them on the script VM.

// `#[derive(Reflectable)]` names its type-info static after the type.
#![allow(non_upper_case_globals)]

use std::collections::HashMap;
use std::sync::Arc;

use blueprint_compiler::{compile, ClassSource, Diagnostic, VariableSource};
use graphy::{Connection, ConnectionType, DataType, GraphDescription, NodeInstance, Pin, PinInstance, PinType, Position};
use pulsar_reflection::Reflectable;
use pulsar_scenedb::{component_methods, Entity, World};
use pulsar_script_vm::{
    script_component, Budget, Host, Instance, NativeFn, NativeRegistry, Program, Value, Vm,
};
use serde_json::json;

// ---- a tiny graph builder -------------------------------------------------

#[derive(Default)]
struct Graph {
    nodes: Vec<NodeInstance>,
    connections: Vec<Connection>,
}

enum P {
    ExecIn,
    ExecOut(&'static str),
    In(&'static str, &'static str),
    Out(&'static str, &'static str),
}

impl Graph {
    fn node(&mut self, id: &str, node_type: &str, pins: &[P]) -> &mut Self {
        let pin = |id: &str, data_type: DataType, pin_type: PinType| PinInstance {
            id: id.into(),
            pin: Pin { id: id.into(), name: id.into(), data_type, pin_type },
        };
        let mut node = NodeInstance {
            id: id.into(),
            node_type: node_type.into(),
            position: Position::new(0.0, 0.0),
            inputs: Vec::new(),
            outputs: Vec::new(),
            properties: HashMap::new(),
            typed_properties: HashMap::new(),
        };
        for p in pins {
            match p {
                P::ExecIn => node.inputs.push(pin("exec", DataType::Exec, PinType::Input)),
                P::ExecOut(name) => node.outputs.push(pin(name, DataType::Exec, PinType::Output)),
                P::In(name, ty) => node.inputs.push(pin(name, DataType::typed(*ty), PinType::Input)),
                P::Out(name, ty) => node.outputs.push(pin(name, DataType::typed(*ty), PinType::Output)),
            }
        }
        self.nodes.push(node);
        self
    }

    fn prop(&mut self, node: &str, pin: &str, value: serde_json::Value) -> &mut Self {
        let n = self.nodes.iter_mut().find(|n| n.id == node).unwrap();
        n.properties.insert(pin.into(), value);
        self
    }

    fn exec(&mut self, from: &str, pin: &str, to: &str) -> &mut Self {
        self.connections.push(Connection {
            source_node: from.into(),
            source_pin: pin.into(),
            target_node: to.into(),
            target_pin: "exec".into(),
            connection_type: ConnectionType::Execution,
        });
        self
    }

    fn data(&mut self, from: &str, from_pin: &str, to: &str, to_pin: &str) -> &mut Self {
        self.connections.push(Connection {
            source_node: from.into(),
            source_pin: from_pin.into(),
            target_node: to.into(),
            target_pin: to_pin.into(),
            connection_type: ConnectionType::Data,
        });
        self
    }

    fn build(&self) -> GraphDescription {
        let mut g = GraphDescription::new("test");
        for n in &self.nodes {
            g.nodes.insert(n.id.clone(), n.clone());
        }
        g.connections = self.connections.clone();
        g
    }

    // Common nodes.
    fn event(&mut self, id: &str, ty: &str) -> &mut Self {
        self.node(id, ty, &[P::ExecOut("Body")])
    }

    fn set_var(&mut self, id: &str, var: &str, ty: &'static str) -> &mut Self {
        self.node(id, &format!("set_{var}"), &[P::ExecIn, P::In("value", ty), P::ExecOut("exec_out"), P::Out("value", ty)])
    }

    fn get_var(&mut self, id: &str, var: &str, ty: &'static str) -> &mut Self {
        self.node(id, &format!("get_{var}"), &[P::Out("value", ty)])
    }

    fn add(&mut self, id: &str) -> &mut Self {
        self.node(id, "add", &[P::In("a", "i64"), P::In("b", "i64"), P::Out("result", "i64")])
    }

    /// `log = append(log, text)` on the exec wire.
    fn log(&mut self, id: &str, text: &str) -> &mut Self {
        let get = format!("{id}_get");
        let append = format!("{id}_append");
        self.get_var(&get, "log", "String");
        self.node(&append, "append", &[P::In("a", "String"), P::In("b", "String"), P::Out("result", "String")]);
        self.prop(&append, "b", json!(text));
        self.data(&get, "value", &append, "a");
        self.set_var(id, "log", "String");
        self.data(&append, "result", id, "value")
    }
}

// ---- natives standing in for pulsar_std, and a test component ---------------

#[derive(Clone, Debug, Default, PartialEq, Reflectable)]
pub struct Health {
    pub value: f32,
}

#[component_methods]
impl Health {
    #[reflect_method]
    fn damage(&mut self, amount: f32) -> f32 {
        self.value -= amount;
        self.value
    }
}

script_component!(Health);

fn natives() -> NativeRegistry {
    let mut r = NativeRegistry::with_engine_natives();
    let mut add = |n: NativeFn| r.register(n).unwrap();
    add(NativeFn::builder("std::add").pure().params(["a", "b"]).build(|a: i64, b: i64| a + b));
    add(NativeFn::builder("std::append").pure().params(["a", "b"]).build(|a: String, b: String| a + &b));
    add(NativeFn::builder("std::less").pure().params(["a", "b"]).build(|a: i64, b: i64| a < b));
    // An impure node with a result (has an exec input in the graph).
    add(NativeFn::builder("std::roll").build(|| 4i64));
    r
}

fn var(name: &str, ty: &str, default: Option<serde_json::Value>) -> VariableSource {
    VariableSource { name: name.into(), type_name: ty.into(), default }
}

fn log_vars() -> Vec<VariableSource> {
    vec![var("log", "String", None), var("count", "i64", Some(json!(0)))]
}

struct Run {
    program: Program,
    world: World,
    entity: Entity,
    vm: Vm,
}

impl Run {
    fn new(graph: &Graph, variables: &[VariableSource]) -> Self {
        let registry = natives();
        let g = graph.build();
        let module = compile(&ClassSource { name: "Test", graph: &g, variables, events: &[], known_events: &[] }, &registry)
            .unwrap_or_else(|d| panic!("compile failed: {d:?}"));
        let program = Program::link(Arc::new(module), &registry).expect("link");
        let mut world = World::new();
        let entity = world.spawn();
        Self { program, world, entity, vm: Vm::new() }
    }

    fn call(&mut self, instance: &mut Instance, name: &str, args: &[Value]) {
        let func = self.program.entry(name).unwrap_or_else(|| panic!("no entry {name}"));
        let mut host = Host::new(&mut self.world, self.entity);
        self.vm
            .call(&self.program, instance, func, args, &mut host, &mut Budget::new(100_000))
            .unwrap_or_else(|e| panic!("{name} failed: {e}"));
    }

    fn var(&self, instance: &Instance, name: &str) -> Value {
        self.program.var(instance, self.program.variable(name).unwrap()).unwrap().clone()
    }
}

fn errors(graph: &Graph, variables: &[VariableSource]) -> Vec<Diagnostic> {
    let g = graph.build();
    compile(&ClassSource { name: "Test", graph: &g, variables, events: &[], known_events: &[] }, &natives()).expect_err("compile should fail")
}

// ---- tests ------------------------------------------------------------------

#[test]
fn begin_play_sets_a_variable_from_a_pure_chain() {
    let mut g = Graph::default();
    g.event("bp", "begin_play").set_var("set", "count", "i64").get_var("get", "count", "i64").add("add");
    g.prop("add", "b", json!(5)).data("get", "value", "add", "a").data("add", "result", "set", "value");
    g.exec("bp", "Body", "set");
    let mut run = Run::new(&g, &log_vars());
    let mut i = run.program.instantiate();
    run.call(&mut i, "begin_play", &[]);
    run.call(&mut i, "begin_play", &[]);
    assert_eq!(run.var(&i, "count"), Value::Int(10));
}

#[test]
fn tick_receives_delta_time() {
    let mut g = Graph::default();
    g.event("tick", "on_tick").node("dt", "get_delta_time", &[P::Out("result", "f32")]);
    g.set_var("set", "elapsed", "f64").get_var("get", "elapsed", "f64");
    g.node("sum", "fadd", &[P::In("a", "f64"), P::In("b", "f64"), P::Out("result", "f64")]);
    g.data("get", "value", "sum", "a").data("dt", "result", "sum", "b").data("sum", "result", "set", "value");
    g.exec("tick", "Body", "set");
    let vars = [var("elapsed", "f64", None)];
    let registry = {
        let mut r = natives();
        r.register(NativeFn::builder("std::fadd").pure().params(["a", "b"]).build(|a: f64, b: f64| a + b)).unwrap();
        r
    };
    let built = g.build();
    let module = compile(&ClassSource { name: "Ticker", graph: &built, variables: &vars, events: &[], known_events: &[] }, &registry).unwrap();
    let program = Program::link(Arc::new(module), &registry).unwrap();
    let mut world = World::new();
    let e = world.spawn();
    let mut vm = Vm::new();
    let mut inst = program.instantiate();
    let tick = program.entry("tick").unwrap();
    for dt in [0.5, 0.25] {
        let mut host = Host::new(&mut world, e);
        vm.call(&program, &mut inst, tick, &[Value::Float(dt)], &mut host, &mut Budget::new(1000)).unwrap();
    }
    assert_eq!(program.var(&inst, program.variable("elapsed").unwrap()), Some(&Value::Float(0.75)));
}

#[test]
fn branches_take_one_path() {
    let mut g = Graph::default();
    g.node("bp", "on_check", &[P::ExecOut("Body"), P::Out("flag", "bool")]);
    g.node("br", "branch", &[P::ExecIn, P::In("condition", "bool"), P::ExecOut("True"), P::ExecOut("False")]);
    g.data("bp", "flag", "br", "condition").exec("bp", "Body", "br");
    g.log("yes", "Y").log("no", "N");
    g.exec("br", "True", "yes").exec("br", "False", "no");
    let mut run = Run::new(&g, &log_vars());
    let mut i = run.program.instantiate();
    run.call(&mut i, "on_check", &[Value::Bool(true)]);
    run.call(&mut i, "on_check", &[Value::Bool(false)]);
    run.call(&mut i, "on_check", &[Value::Bool(true)]);
    assert_eq!(run.var(&i, "log"), Value::from("YNY"));
}

#[test]
fn loops_run_their_body_every_iteration() {
    let mut g = Graph::default();
    g.event("bp", "begin_play");
    g.node("for", "for_loop", &[P::ExecIn, P::In("count", "i64"), P::ExecOut("Body")]).prop("for", "count", json!(7));
    g.log("body", "x");
    g.exec("bp", "Body", "for").exec("for", "Body", "body");
    // while count < 3 { count += 1 }
    g.event("bp2", "on_count");
    g.get_var("c1", "count", "i64").node("lt", "less", &[P::In("a", "i64"), P::In("b", "i64"), P::Out("result", "bool")]);
    g.prop("lt", "b", json!(3)).data("c1", "value", "lt", "a");
    g.node("wh", "while_loop", &[P::ExecIn, P::In("condition", "bool"), P::ExecOut("Body")]).data("lt", "result", "wh", "condition");
    g.get_var("c2", "count", "i64").add("inc").prop("inc", "b", json!(1)).data("c2", "value", "inc", "a");
    g.set_var("setc", "count", "i64").data("inc", "result", "setc", "value");
    g.exec("bp2", "Body", "wh").exec("wh", "Body", "setc");

    let mut run = Run::new(&g, &log_vars());
    let mut i = run.program.instantiate();
    run.call(&mut i, "begin_play", &[]);
    assert_eq!(run.var(&i, "log"), Value::from("xxxxxxx"));
    run.call(&mut i, "on_count", &[]);
    assert_eq!(run.var(&i, "count"), Value::Int(3));
}

#[test]
fn sequence_and_merging_paths() {
    let mut g = Graph::default();
    g.event("bp", "begin_play");
    g.node("seq", "sequence", &[P::ExecIn, P::ExecOut("Then0"), P::ExecOut("Then1"), P::ExecOut("Then2"), P::ExecOut("Then3")]);
    g.log("a", "A").log("b", "B").log("tail", "!");
    g.exec("bp", "Body", "seq").exec("seq", "Then0", "a").exec("seq", "Then1", "b");
    // Both chains continue into the same node: it runs once per path.
    g.exec("a", "exec_out", "tail").exec("b", "exec_out", "tail");
    let mut run = Run::new(&g, &log_vars());
    let mut i = run.program.instantiate();
    run.call(&mut i, "begin_play", &[]);
    assert_eq!(run.var(&i, "log"), Value::from("A!B!"));
}

#[test]
fn stateful_flow_nodes_are_per_instance() {
    let mut g = Graph::default();
    g.event("ev", "on_fire");
    g.node("once", "do_once", &[P::ExecIn, P::In("reset", "bool"), P::ExecOut("Then")]);
    g.node("ff", "flip_flop", &[P::ExecIn, P::ExecOut("A"), P::ExecOut("B")]);
    g.log("o", "o").log("fa", "a").log("fb", "b");
    g.exec("ev", "Body", "once").exec("once", "Then", "o");
    g.exec("o", "exec_out", "ff");
    g.exec("ev", "Body", "ff");
    g.exec("ff", "A", "fa").exec("ff", "B", "fb");
    let mut run = Run::new(&g, &log_vars());
    let mut one = run.program.instantiate();
    let mut two = run.program.instantiate();
    for _ in 0..3 {
        run.call(&mut one, "on_fire", &[]);
    }
    run.call(&mut two, "on_fire", &[]);
    // Call 1: do_once logs "o" and chains into flip_flop ("b"), then the
    // event's second wire flips again ("a"). Calls 2 and 3 only flip.
    // flip_flop starts on B; do_once fires once per instance.
    assert_eq!(run.var(&one, "log"), Value::from("obaba"));
    assert_eq!(run.var(&two, "log"), Value::from("oba"));
}

#[test]
fn switches_pick_the_matching_case() {
    let mut g = Graph::default();
    g.node("ev", "on_pick", &[P::ExecOut("Body"), P::Out("n", "i64")]);
    g.node("sw", "switch_on_int", &[P::ExecIn, P::In("value", "i64"), P::ExecOut("Case0"), P::ExecOut("Case1"), P::ExecOut("Default")]);
    g.data("ev", "n", "sw", "value").exec("ev", "Body", "sw");
    g.log("c0", "0").log("c1", "1").log("d", "d");
    g.exec("sw", "Case0", "c0").exec("sw", "Case1", "c1").exec("sw", "Default", "d");
    let mut run = Run::new(&g, &log_vars());
    let mut i = run.program.instantiate();
    for n in [1, 0, 9] {
        run.call(&mut i, "on_pick", &[Value::Int(n)]);
    }
    assert_eq!(run.var(&i, "log"), Value::from("10d"));
}

#[test]
fn impure_results_and_custom_events() {
    let mut g = Graph::default();
    // begin_play: n = roll(); dispatch on_add(n + 1)
    g.event("bp", "begin_play");
    g.node("roll", "roll", &[P::ExecIn, P::ExecOut("exec_out"), P::Out("result", "i64")]);
    g.add("plus").prop("plus", "b", json!(1)).data("roll", "result", "plus", "a");
    g.node("emit", "emit_custom_event", &[P::ExecIn, P::In("amount", "i64"), P::ExecOut("exec_out")]);
    g.prop("emit", "event_uid", json!("add-it")).data("plus", "result", "emit", "amount");
    g.exec("bp", "Body", "roll").exec("roll", "exec_out", "emit");
    // on_add_it(amount): count = count + amount
    g.node("handler", "on_add_it", &[P::ExecOut("Body"), P::Out("amount", "i64")]);
    g.get_var("c", "count", "i64").add("sum").data("c", "value", "sum", "a").data("handler", "amount", "sum", "b");
    g.set_var("set", "count", "i64").data("sum", "result", "set", "value");
    g.exec("handler", "Body", "set");
    let mut run = Run::new(&g, &log_vars());
    let mut i = run.program.instantiate();
    run.call(&mut i, "begin_play", &[]);
    assert_eq!(run.var(&i, "count"), Value::Int(5));
}

#[test]
fn component_nodes_default_to_this_entity() {
    let mut g = Graph::default();
    g.event("bp", "begin_play");
    g.node("call", "comp_call::Health::damage", &[P::ExecIn, P::In("component_ref", "Health"), P::In("amount", "f32"), P::ExecOut("exec_out"), P::Out("return_value", "f32")]);
    g.prop("call", "amount", json!(2.5));
    g.node("set", "comp_set_prop::Health::value", &[P::ExecIn, P::In("component_ref", "Health"), P::In("value", "f32"), P::ExecOut("exec_out")]);
    g.node("get", "comp_get_prop::Health::value", &[P::In("component_ref", "Health"), P::Out("value", "f32")]);
    g.node("double", "fadd2", &[P::In("a", "f64"), P::Out("result", "f64")]);
    g.data("get", "value", "double", "a").data("double", "result", "set", "value");
    g.exec("bp", "Body", "call").exec("call", "exec_out", "set");
    let registry = {
        let mut r = natives();
        r.register(NativeFn::builder("std::fadd2").pure().params(["a"]).build(|a: f64| a * 2.0)).unwrap();
        r
    };
    let built = g.build();
    let module = compile(&ClassSource { name: "Hurt", graph: &built, variables: &[], events: &[], known_events: &[] }, &registry).unwrap();
    let program = Program::link(Arc::new(module), &registry).unwrap();
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health { value: 10.0 });
    let mut inst = program.instantiate();
    let mut host = Host::new(&mut world, e);
    Vm::new()
        .call(&program, &mut inst, program.entry("begin_play").unwrap(), &[], &mut host, &mut Budget::new(1000))
        .unwrap();
    assert_eq!(world.get::<Health>(e).unwrap().value, 15.0);
}

#[test]
fn diagnostics() {
    // Unknown node.
    let mut g = Graph::default();
    g.event("bp", "begin_play").node("x", "no_such_node", &[P::ExecIn, P::ExecOut("exec_out")]).exec("bp", "Body", "x");
    let d = errors(&g, &[]);
    assert!(d.iter().any(|d| d.node.as_deref() == Some("x") && d.message.contains("not available")), "{d:?}");

    // Exec cycle without a loop node.
    let mut g = Graph::default();
    g.event("bp", "begin_play").log("a", "a").log("b", "b");
    g.exec("bp", "Body", "a").exec("a", "exec_out", "b").exec("b", "exec_out", "a");
    let d = errors(&g, &log_vars());
    assert!(d.iter().any(|d| d.message.contains("loops back")), "{d:?}");

    // Type mismatch on a wire.
    let mut g = Graph::default();
    g.event("bp", "begin_play").get_var("get", "log", "String").set_var("set", "count", "i64");
    g.data("get", "value", "set", "value").exec("bp", "Body", "set");
    let d = errors(&g, &log_vars());
    assert!(d.iter().any(|d| d.message.contains("expected int, got string")), "{d:?}");


    // Unknown variable type.
    let d = errors(&Graph::default(), &[var("v", "Vec<Mystery>", None)]);
    assert!(d[0].message.contains("not available to scripts"), "{d:?}");
}

// ---- native::<name> nodes ------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Reflectable)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

#[pulsar_reflection::reflect_methods]
impl V2 {
    #[reflect_method(pure)]
    fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

pulsar_script_vm::script_value_type!(V2);

#[test]
fn native_nodes_call_any_registered_native() {
    // begin_play:
    //   Health.damage(self, 1)          (component method, receiver defaults to self)
    //   v = V2 default; V2::set_x(v, 3); V2::set_y(v', 4)   (inout, chained through outputs)
    //   count = FloatToInt(length(v''))  via a pure native chain into set_count
    let mut g = Graph::default();
    g.event("bp", "begin_play");
    g.node("dmg", "native::Health::damage", &[P::ExecIn, P::In("self", "Health"), P::In("amount", "f64"), P::ExecOut("exec_out"), P::Out("result", "f64")]);
    g.prop("dmg", "amount", json!(1.0));
    g.node("sx", "native::V2::set_x", &[P::In("self", "V2"), P::In("value", "f64"), P::Out("self", "V2")]);
    g.prop("sx", "value", json!(3.0));
    g.node("sy", "native::V2::set_y", &[P::In("self", "V2"), P::In("value", "f64"), P::Out("self", "V2")]);
    g.prop("sy", "value", json!(4.0)).data("sx", "self", "sy", "self");
    g.node("len", "native::V2::length", &[P::In("self", "V2"), P::Out("result", "f64")]).data("sy", "self", "len", "self");
    g.node("round", "to_int", &[P::In("x", "f64"), P::Out("result", "i64")]).data("len", "result", "round", "x");
    g.set_var("set", "count", "i64").data("round", "result", "set", "value");
    g.exec("bp", "Body", "dmg").exec("dmg", "exec_out", "set");

    let registry = {
        let mut r = natives();
        r.register(NativeFn::builder("std::to_int").pure().params(["x"]).build(|x: f64| x.round() as i64)).unwrap();
        r
    };
    let built = g.build();
    let vars = log_vars();
    let module = compile(&ClassSource { name: "Natives", graph: &built, variables: &vars, events: &[], known_events: &[] }, &registry)
        .unwrap_or_else(|d| panic!("{d:?}"));
    let program = Program::link(Arc::new(module), &registry).unwrap();
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health { value: 10.0 });
    let mut inst = program.instantiate();
    let mut host = Host::new(&mut world, e);
    Vm::new()
        .call(&program, &mut inst, program.entry("begin_play").unwrap(), &[], &mut host, &mut Budget::new(1000))
        .unwrap();
    assert_eq!(world.get::<Health>(e).unwrap().value, 9.0);
    assert_eq!(program.var(&inst, program.variable("count").unwrap()), Some(&Value::Int(5)));
}

#[test]
fn palette_lists_methods_by_reference_type_and_globally() {
    use blueprint_compiler::palette::{methods_for, native_nodes};
    let nodes = native_nodes(&natives());
    // pulsar_std functions keep their own nodes.
    assert!(nodes.iter().all(|n| !n.node_type.starts_with("native::std::")));

    let health: Vec<_> = methods_for(&nodes, "Health").map(|n| n.node_type.as_str()).collect();
    assert!(health.contains(&"native::Health::damage"), "{health:?}");
    assert!(health.contains(&"native::Health::get_value"), "{health:?}");

    let damage = nodes.iter().find(|n| n.node_type == "native::Health::damage").unwrap();
    assert_eq!(damage.category, "Components/Health");
    assert_eq!(damage.name, "Damage");
    assert!(damage.exec);
    assert_eq!(damage.inputs, [("self".to_string(), "Health".to_string()), ("amount".to_string(), "f64".to_string())]);
    assert_eq!(damage.outputs, [("result".to_string(), "f64".to_string())]);

    let set_x = nodes.iter().find(|n| n.node_type == "native::V2::set_x").unwrap();
    assert_eq!(set_x.category, "Types/V2");
    assert!(!set_x.exec, "value-type setters are pure");
    assert_eq!(set_x.outputs, [("self".to_string(), "V2".to_string())]);

    // Associated functions and the stdlib are in the global list too.
    let of = nodes.iter().find(|n| n.node_type == "native::Health::of").unwrap();
    assert_eq!(of.category, "Components/Health");
    assert!(of.receiver.is_none());
    assert!(nodes.iter().any(|n| n.node_type == "native::math::sin" && n.category == "Math"));
}

#[test]
fn component_refs_on_other_objects_and_scene_lookups() {
    // on_hit: find_object_by_name("target") -> get_component_ref::Health::0 -> set value
    let mut g = Graph::default();
    g.event("ev", "on_hit");
    g.node("find", "find_object_by_name", &[P::In("name", "String"), P::Out("object", "Entity")]).prop("find", "name", json!("target"));
    g.node("ref", "get_component_ref::Health::0", &[P::In("object", "Entity"), P::Out("component", "Health")]).data("find", "object", "ref", "object");
    g.node("set", "comp_set_prop::Health::value", &[P::ExecIn, P::In("component_ref", "Health"), P::In("value", "f32"), P::ExecOut("exec_out")]);
    g.prop("set", "value", json!(42.0)).data("ref", "component", "set", "component_ref").exec("ev", "Body", "set");
    // Stand-in for pulsar_game's world lookup: the entity at index 1.
    let registry = {
        let mut r = natives();
        r.register(NativeFn::builder("world::find_by_name").side_effect_free().params(["name"]).build(
            |host: &mut Host<'_>, name: String| {
                assert_eq!(name, "target");
                host.world.query::<&Health>().map(|(e, _)| e).find(|e| e.index() == 1).unwrap_or(Entity::DANGLING)
            },
        ))
        .unwrap();
        r
    };
    let built = g.build();
    let module = compile(&ClassSource { name: "Hit", graph: &built, variables: &[], events: &[], known_events: &[] }, &registry).unwrap_or_else(|d| panic!("{d:?}"));
    let program = Program::link(Arc::new(module), &registry).unwrap();
    let mut world = World::new();
    let me = world.spawn();
    let target = world.spawn();
    world.insert(me, Health { value: 1.0 });
    world.insert(target, Health { value: 1.0 });
    let mut inst = program.instantiate();
    let mut host = Host::new(&mut world, me);
    Vm::new().call(&program, &mut inst, program.entry("on_hit").unwrap(), &[], &mut host, &mut Budget::new(1000)).unwrap();
    assert_eq!(world.get::<Health>(target).unwrap().value, 42.0);
    assert_eq!(world.get::<Health>(me).unwrap().value, 1.0);
}

/// #921: a `get_component_ref` with a slot id (a prefab slot UUID) reads a
/// hidden handle variable the module declares; the engine fills it once
/// when binding the instance, so a second component of one class (on a
/// generated child) is reachable without any lookup by UUID. Nodes without
/// a slot id keep resolving by class name.
#[test]
fn component_refs_read_slot_handles() {
    const ROOT_SLOT: &str = "0b6f3c3e-2f7a-4d0e-9a55-6f1f0f4d8a01";
    const CHILD_SLOT: &str = "7d1a9b24-5c3e-4f11-8e2d-3a9c0b7e6f02";
    // on_hit: child slot -> value = 5; root slot -> value = 7;
    //         old-style node (no slot) -> damage(1)
    let mut g = Graph::default();
    g.event("ev", "on_hit");
    g.node("second", "get_component_ref::Health::1", &[P::Out("component", "Health")]).prop("second", "slot_id", json!(CHILD_SLOT));
    g.node("first", "get_component_ref::Health::0", &[P::Out("component", "Health")]).prop("first", "slot_id", json!(ROOT_SLOT));
    g.node("again", "get_component_ref::Health::1", &[P::Out("component", "Health")]).prop("again", "slot_id", json!(CHILD_SLOT));
    g.node("old", "get_component_ref::Health::0", &[P::Out("component", "Health")]);
    g.node("set2", "comp_set_prop::Health::value", &[P::ExecIn, P::In("component_ref", "Health"), P::In("value", "f32"), P::ExecOut("exec_out")]);
    g.node("set1", "comp_set_prop::Health::value", &[P::ExecIn, P::In("component_ref", "Health"), P::In("value", "f32"), P::ExecOut("exec_out")]);
    g.node("hit", "comp_call::Health::damage", &[P::ExecIn, P::In("component_ref", "Health"), P::In("amount", "f32"), P::ExecOut("exec_out"), P::Out("result", "f32")]);
    g.node("hit2", "comp_call::Health::damage", &[P::ExecIn, P::In("component_ref", "Health"), P::In("amount", "f32"), P::ExecOut("exec_out"), P::Out("result", "f32")]);
    g.prop("set2", "value", json!(5.0)).prop("set1", "value", json!(7.0)).prop("hit", "amount", json!(1.0)).prop("hit2", "amount", json!(2.0));
    g.data("second", "component", "set2", "component_ref");
    g.data("first", "component", "set1", "component_ref");
    g.data("old", "component", "hit", "component_ref");
    g.data("again", "component", "hit2", "component_ref");
    g.exec("ev", "Body", "set2").exec("set2", "exec_out", "set1").exec("set1", "exec_out", "hit").exec("hit", "exec_out", "hit2");

    let registry = natives();
    let built = g.build();
    let module = compile(&ClassSource { name: "Slots", graph: &built, variables: &[], events: &[], known_events: &[] }, &registry).unwrap_or_else(|d| panic!("{d:?}"));

    // One hidden handle per slot, typed by the slot's component class; no
    // native call by UUID.
    let mut slots = blueprint_compiler::module_slots(&module);
    slots.sort();
    assert_eq!(slots, [(ROOT_SLOT.to_string(), "Health".to_string()), (CHILD_SLOT.to_string(), "Health".to_string())]);
    assert!(module.imports.iter().all(|i| !i.name.contains("slot")), "{:?}", module.imports);

    let program = Program::link(Arc::new(module), &registry).unwrap();
    let mut world = World::new();
    let root = world.spawn();
    let child = world.spawn();
    world.insert(root, Health { value: 1.0 });
    world.insert(child, Health { value: 1.0 });
    let mut inst = program.instantiate();
    // What the engine does at bind time: fill each handle from the placement.
    let health = pulsar_scenedb::component_id::<Health>();
    for (slot, entity) in [(ROOT_SLOT, root), (CHILD_SLOT, child)] {
        let var = program.variable(&blueprint_compiler::slot_variable_name(slot)).unwrap();
        program
            .set_var(&mut inst, var, Value::Component(pulsar_scenedb::ComponentRef::new(entity, health)))
            .unwrap();
    }
    let mut host = Host::new(&mut world, root);
    Vm::new().call(&program, &mut inst, program.entry("on_hit").unwrap(), &[], &mut host, &mut Budget::new(1000)).unwrap();
    assert_eq!(world.get::<Health>(child).unwrap().value, 3.0, "child slot: set 5, then damaged by 2 through the same handle");
    assert_eq!(world.get::<Health>(root).unwrap().value, 6.0, "root slot set 7, then the old by-class node damaged it by 1");
}

/// An unfilled slot handle is `none`: using it fails instead of silently
/// falling back to another component.
#[test]
fn unbound_slot_handles_do_not_fall_back() {
    let mut g = Graph::default();
    g.event("ev", "on_hit");
    g.node("second", "get_component_ref::Health::1", &[P::Out("component", "Health")]).prop("second", "slot_id", json!("7d1a9b24-5c3e-4f11-8e2d-3a9c0b7e6f02"));
    g.node("hit", "comp_call::Health::damage", &[P::ExecIn, P::In("component_ref", "Health"), P::In("amount", "f32"), P::ExecOut("exec_out"), P::Out("result", "f32")]);
    g.prop("hit", "amount", json!(1.0)).data("second", "component", "hit", "component_ref").exec("ev", "Body", "hit");
    let registry = natives();
    let built = g.build();
    let module = compile(&ClassSource { name: "Slots", graph: &built, variables: &[], events: &[], known_events: &[] }, &registry).unwrap();
    let program = Program::link(Arc::new(module), &registry).unwrap();
    let mut world = World::new();
    let me = world.spawn();
    world.insert(me, Health { value: 1.0 });
    let mut inst = program.instantiate();
    let mut host = Host::new(&mut world, me);
    let result = Vm::new().call(&program, &mut inst, program.entry("on_hit").unwrap(), &[], &mut host, &mut Budget::new(1000));
    assert!(result.is_err(), "a none handle must not resolve");
    assert_eq!(world.get::<Health>(me).unwrap().value, 1.0, "the entity's own component was not touched");
}

#[test]
fn delays_suspend_until_game_time_passes() {
    use pulsar_script_vm::Completion;
    // on_fire: log "a"; delay 500ms; log "b"
    let mut g = Graph::default();
    g.event("ev", "on_fire");
    g.log("a", "a").log("b", "b");
    g.node("wait", "delay", &[P::ExecIn, P::In("milliseconds", "i64"), P::ExecOut("Completed")]).prop("wait", "milliseconds", json!(500));
    g.exec("ev", "Body", "a").exec("a", "exec_out", "wait").exec("wait", "Completed", "b");
    let registry = natives();
    let built = g.build();
    let vars = log_vars();
    let module = compile(&ClassSource { name: "Delay", graph: &built, variables: &vars, events: &[], known_events: &[] }, &registry).unwrap();
    let program = Program::link(Arc::new(module), &registry).unwrap();
    let mut world = World::new();
    let e = world.spawn();
    let mut vm = Vm::new();
    let mut inst = program.instantiate();
    let fire = program.entry("on_fire").unwrap();
    let log = program.variable("log").unwrap();

    let start = |vm: &mut Vm, inst: &mut Instance, world: &mut World, t: f64| {
        vm.start(&program, inst, fire, &[], &mut Host::at_time(world, e, t), &mut Budget::new(1000)).unwrap()
    };
    let Completion::Waiting { seconds, continuation } = start(&mut vm, &mut inst, &mut world, 0.0) else { panic!() };
    assert_eq!(seconds, 0.5);
    // Firing again while the delay counts down is ignored.
    assert!(matches!(start(&mut vm, &mut inst, &mut world, 0.1), Completion::Returned(_)));
    assert_eq!(program.var(&inst, log), Some(&Value::from("aa")));
    let done = vm.resume(&program, &mut inst, continuation, &mut Host::at_time(&mut world, e, 0.5), &mut Budget::new(1000)).unwrap();
    assert!(matches!(done, Completion::Returned(_)));
    assert_eq!(program.var(&inst, log), Some(&Value::from("aab")));
    // Ready again.
    assert!(matches!(start(&mut vm, &mut inst, &mut world, 1.0), Completion::Waiting { .. }));
}

#[test]
fn retriggerable_delays_restart_their_countdown() {
    use pulsar_script_vm::Completion;
    let mut g = Graph::default();
    g.event("ev", "on_fire");
    g.node("wait", "retriggerable_delay", &[P::ExecIn, P::In("delay_ms", "i64"), P::ExecOut("Completed")]).prop("wait", "delay_ms", json!(1000));
    g.log("done", "!");
    g.exec("ev", "Body", "wait").exec("wait", "Completed", "done");
    let registry = natives();
    let built = g.build();
    let vars = log_vars();
    let module = compile(&ClassSource { name: "Retrigger", graph: &built, variables: &vars, events: &[], known_events: &[] }, &registry).unwrap();
    let program = Program::link(Arc::new(module), &registry).unwrap();
    let mut world = World::new();
    let e = world.spawn();
    let mut vm = Vm::new();
    let mut inst = program.instantiate();
    let fire = program.entry("on_fire").unwrap();
    let log = program.variable("log").unwrap();

    let mut host = Host::at_time(&mut world, e, 0.0);
    let Completion::Waiting { seconds, continuation } =
        vm.start(&program, &mut inst, fire, &[], &mut host, &mut Budget::new(1000)).unwrap()
    else {
        panic!()
    };
    assert_eq!(seconds, 1.0);
    // Retrigger at t=0.8: the running countdown now ends at 1.8.
    let mut host = Host::at_time(&mut world, e, 0.8);
    assert!(matches!(vm.start(&program, &mut inst, fire, &[], &mut host, &mut Budget::new(1000)).unwrap(), Completion::Returned(_)));
    // Resumed at 1.0 it waits the remaining 0.8s instead of completing.
    let mut host = Host::at_time(&mut world, e, 1.0);
    let Completion::Waiting { seconds, continuation } =
        vm.resume(&program, &mut inst, continuation, &mut host, &mut Budget::new(1000)).unwrap()
    else {
        panic!("should keep waiting")
    };
    assert!((seconds - 0.8).abs() < 1e-9, "{seconds}");
    assert_eq!(program.var(&inst, log), Some(&Value::from("")));
    let mut host = Host::at_time(&mut world, e, 1.8);
    assert!(matches!(vm.resume(&program, &mut inst, continuation, &mut host, &mut Budget::new(1000)).unwrap(), Completion::Returned(_)));
    assert_eq!(program.var(&inst, log), Some(&Value::from("!")));
}

#[test]
fn other_flow_nodes_jump_where_their_selector_says() {
    // A selector native standing in for a pulsar_std control-flow node:
    // `pick(n)` fires output n % 3 and returns n * 10.
    let mut registry = natives();
    registry
        .register(
            NativeFn::builder("std::pick")
                .attr("exec_outputs", "X,Y,Z")
                .params(["n", "result"])
                .build_raw(
                    pulsar_script_vm::Signature::new(
                        [pulsar_script_vm::Param::new(pulsar_script_vm::Type::Int), pulsar_script_vm::Param::inout(pulsar_script_vm::Type::Int)],
                        pulsar_script_vm::Type::Int,
                    ),
                    Box::new(|_, args| {
                        let n = args[0].as_int().unwrap();
                        args[1] = Value::Int(n * 10);
                        Ok(Value::Int(n % 3))
                    }),
                ),
        )
        .unwrap();
    let mut g = Graph::default();
    g.node("ev", "on_pick", &[P::ExecOut("Body"), P::Out("n", "i64")]);
    g.node("pick", "pick", &[P::ExecIn, P::In("n", "i64"), P::ExecOut("X"), P::ExecOut("Y"), P::ExecOut("Z"), P::Out("result", "i64")]);
    g.data("ev", "n", "pick", "n").exec("ev", "Body", "pick");
    g.log("x", "x").log("y", "y").log("z", "z");
    g.exec("pick", "X", "x").exec("pick", "Y", "y").exec("pick", "Z", "z");
    // count = result, after the chosen chain
    g.set_var("setc", "count", "i64").data("pick", "result", "setc", "value").exec("z", "exec_out", "setc");
    let built = g.build();
    let vars = log_vars();
    let module = compile(&ClassSource { name: "Pick", graph: &built, variables: &vars, events: &[], known_events: &[] }, &registry).unwrap_or_else(|d| panic!("{d:?}"));
    let program = Program::link(Arc::new(module), &registry).unwrap();
    let mut world = World::new();
    let e = world.spawn();
    let mut inst = program.instantiate();
    let pick = program.entry("on_pick").unwrap();
    for n in [4, 0, 5] {
        let mut host = Host::new(&mut world, e);
        Vm::new().call(&program, &mut inst, pick, &[Value::Int(n)], &mut host, &mut Budget::new(1000)).unwrap();
    }
    assert_eq!(program.var(&inst, program.variable("log").unwrap()), Some(&Value::from("yxz")));
    assert_eq!(program.var(&inst, program.variable("count").unwrap()), Some(&Value::Int(50)));
}

// ---- engine events (#924) ----------------------------------------------------

mod engine_events {
    use super::*;
    use blueprint_compiler::palette::{event_nodes, PaletteEvent};
    use blueprint_compiler::EventSource;
    use pulsar_script_vm::{
        EventCatalog, EventField, EventRef, EventSignature, EventSink, EventTarget, SubscriptionScope, Type,
    };
    use std::sync::Mutex;

    fn hit() -> EventSignature {
        EventSignature {
            id: 7,
            name: "Hit".into(),
            fields: vec![
                EventField::new("entity", Type::Entity),
                EventField::new("other", Type::Entity),
                EventField::new("impulse", Type::Float),
            ],
        }
    }

    fn level_loaded() -> EventSignature {
        EventSignature { id: 8, name: "LevelLoaded".into(), fields: vec![EventField::new("level", Type::Str)] }
    }

    struct Catalog(Vec<EventSignature>);
    impl EventCatalog for Catalog {
        fn event_by_name(&self, name: &str) -> Option<EventSignature> {
            self.0.iter().find(|e| e.name == name).cloned()
        }
        fn event_by_id(&self, id: u64) -> Option<EventSignature> {
            self.0.iter().find(|e| e.id == id).cloned()
        }
    }

    #[derive(Default)]
    struct Sink(Mutex<Vec<(EventTarget, String, Vec<Value>)>>);
    impl EventSink for Sink {
        fn emit(&self, target: EventTarget, name: &str, fields: &[Value]) -> Result<(), String> {
            self.0.lock().unwrap().push((target, name.into(), fields.to_vec()));
            Ok(())
        }
    }

    /// "On Hit" counts hits and remembers `other`; the class declares
    /// `Door.Opened(by: Entity, code: i64)` with a handler; `begin_play`
    /// sends Opened to the last hitter, broadcasts LevelLoaded and sends
    /// Opened to the Door class.
    fn door_graph() -> Graph {
        let mut g = Graph::default();
        g.node("on_hit", "event::on::Hit", &[
            P::ExecOut("Body"),
            P::Out("entity", "Entity"),
            P::Out("other", "Entity"),
            P::Out("impulse", "f64"),
        ]);
        g.get_var("hits_get", "hits", "i64");
        g.node("one", "add", &[P::In("a", "i64"), P::In("b", "i64"), P::Out("result", "i64")]);
        g.prop("one", "b", json!(1));
        g.data("hits_get", "value", "one", "a");
        g.set_var("hits_set", "hits", "i64");
        g.data("one", "result", "hits_set", "value");
        g.exec("on_hit", "Body", "hits_set");
        g.set_var("other_set", "last", "Entity");
        g.data("on_hit", "other", "other_set", "value");
        g.exec("hits_set", "exec_out", "other_set");

        // The class's own custom event handler (`on_<uid>`).
        g.node("on_opened", "on_opened_uid", &[P::ExecOut("Body"), P::Out("by", "Entity"), P::Out("code", "i64")]);
        g.set_var("code_set", "code", "i64");
        g.data("on_opened", "code", "code_set", "value");
        g.exec("on_opened", "Body", "code_set");

        g.event("bp", "begin_play");
        g.get_var("last_get", "last", "Entity");
        g.node("send", "event::send::Door.Opened", &[P::ExecIn, P::In("target", "Entity"), P::In("by", "Entity"), P::In("code", "i64"), P::ExecOut("exec_out")]);
        g.data("last_get", "value", "send", "target");
        g.prop("send", "code", json!(42));
        g.exec("bp", "Body", "send");
        g.node("bcast", "event::broadcast::LevelLoaded", &[P::ExecIn, P::In("level", "String"), P::ExecOut("exec_out")]);
        g.prop("bcast", "level", json!("x.level"));
        g.exec("send", "exec_out", "bcast");
        g.node("toclass", "event::to_class::Door.Opened", &[P::ExecIn, P::In("class", "String"), P::In("by", "Entity"), P::In("code", "i64"), P::ExecOut("exec_out")]);
        g.prop("toclass", "class", json!("Door"));
        g.prop("toclass", "code", json!(7));
        g.exec("bcast", "exec_out", "toclass");
        g
    }

    fn door_vars() -> Vec<VariableSource> {
        vec![var("hits", "i64", None), var("last", "Entity", None), var("code", "i64", None)]
    }

    fn door_events() -> Vec<EventSource> {
        vec![EventSource {
            uid: "opened-uid".into(),
            name: "Opened".into(),
            fields: vec![("by".into(), "Entity".into()), ("code".into(), "i64".into())],
        }]
    }

    #[test]
    fn event_nodes_compile_to_subscriptions_and_sends() {
        let registry = natives();
        let graph = door_graph().build();
        let vars = door_vars();
        let events = door_events();
        let known = [hit(), level_loaded()];
        let source = ClassSource { name: "Door", graph: &graph, variables: &vars, events: &events, known_events: &known };
        let module = compile(&source, &registry).unwrap_or_else(|d| panic!("{d:?}"));

        assert_eq!(module.events.len(), 1);
        assert_eq!(module.events[0].name, "Door.Opened");
        let subs: Vec<(String, SubscriptionScope, String)> = module
            .subscriptions
            .iter()
            .map(|s| {
                let EventRef::Name(name) = &s.event else { panic!() };
                (name.clone(), s.scope, module.functions[s.handler as usize].name.clone())
            })
            .collect();
        assert!(subs.contains(&("Hit".into(), SubscriptionScope::Self_, "on_event__Hit__self".into())), "{subs:?}");
        assert!(subs.contains(&("Door.Opened".into(), SubscriptionScope::Self_, "on_opened_uid".into())), "{subs:?}");

        // Links against the engine's catalog (the class's own events are
        // declared by the engine first; here the catalog knows them).
        let mut catalog = known.to_vec();
        catalog.push(EventSignature::from(&module.events[0]));
        let program = pulsar_script_vm::Program::link_with_events(Arc::new(module), &registry, Some(&Catalog(catalog)))
            .unwrap_or_else(|e| panic!("{e}"));

        // Run the Hit handler, then begin_play.
        let mut world = World::new();
        let me = world.spawn();
        let hitter = world.spawn();
        let mut instance = program.instantiate();
        let sink = Sink::default();
        let mut vm = Vm::new();
        let on_hit = program.subscriptions()[0].handler;
        let mut host = Host::new(&mut world, me).with_events(Some(&sink));
        vm.call(&program, &mut instance, on_hit, &[Value::Entity(me), Value::Entity(hitter), Value::Float(1.0)], &mut host, &mut Budget::new(10_000))
            .unwrap();
        assert_eq!(program.var(&instance, program.variable("hits").unwrap()), Some(&Value::Int(1)));
        let begin = program.entry("begin_play").unwrap();
        vm.call(&program, &mut instance, begin, &[], &mut host, &mut Budget::new(10_000)).unwrap();
        let sent = sink.0.lock().unwrap().clone();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0].0, EventTarget::Entity(hitter), "sent to the last hitter");
        assert_eq!(sent[0].1, "Door.Opened");
        assert_eq!(sent[0].2[1], Value::Int(42));
        assert_eq!((sent[1].0.clone(), sent[1].1.as_str()), (EventTarget::Global, "LevelLoaded"));
        assert_eq!(sent[1].2, vec![Value::Str("x.level".into())]);
        assert_eq!(sent[2].0, EventTarget::Class("Door".into()));
    }

    #[test]
    fn scope_property_and_unknown_events() {
        let registry = natives();
        let vars = door_vars();
        let known = [hit(), level_loaded()];
        let mut g = Graph::default();
        g.node("on_ll", "event::on::LevelLoaded", &[P::ExecOut("Body"), P::Out("level", "String")]);
        g.node("on_hit_class", "event::on::Hit", &[P::ExecOut("Body")]);
        g.prop("on_hit_class", "scope", json!("class"));
        let graph = g.build();
        let module = compile(&ClassSource { name: "S", graph: &graph, variables: &vars, events: &[], known_events: &known }, &registry)
            .unwrap_or_else(|d| panic!("{d:?}"));
        let scopes: Vec<SubscriptionScope> = module.subscriptions.iter().map(|s| s.scope).collect();
        assert!(scopes.contains(&SubscriptionScope::Global), "LevelLoaded defaults to global");
        assert!(scopes.contains(&SubscriptionScope::Class), "the scope property wins");

        let mut g = Graph::default();
        g.node("on_nope", "event::on::Nope", &[P::ExecOut("Body")]);
        g.node("old", "emit_event", &[P::ExecIn]);
        let graph = g.build();
        let errors = compile(&ClassSource { name: "S", graph: &graph, variables: &vars, events: &[], known_events: &known }, &registry)
            .unwrap_err();
        let text: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(text.iter().any(|e| e.contains("no event `Nope`")), "{text:?}");
        assert!(text.iter().any(|e| e.contains("placeholder")), "{text:?}");
    }

    #[test]
    fn palette_lists_event_nodes_by_category() {
        let events = [
            PaletteEvent { signature: hit(), category: "Physics".into(), declared_here: false },
            PaletteEvent { signature: level_loaded(), category: "Lifecycle".into(), declared_here: false },
        ];
        let nodes = event_nodes(&events);
        assert_eq!(nodes.len(), 8);
        let on_hit = nodes.iter().find(|n| n.node_type == "event::on::Hit").unwrap();
        assert_eq!(on_hit.category, "Events/Physics");
        assert!(on_hit.is_event);
        assert_eq!(on_hit.properties, vec![("scope".to_owned(), "self".to_owned())]);
        assert_eq!(on_hit.outputs[1], ("other".to_owned(), "Entity".to_owned()));
        let send = nodes.iter().find(|n| n.node_type == "event::send::Hit").unwrap();
        assert_eq!(send.name, "Send Hit to");
        assert_eq!(send.inputs[0], ("target".to_owned(), "Entity".to_owned()));
        let ll = nodes.iter().find(|n| n.node_type == "event::on::LevelLoaded").unwrap();
        assert_eq!(ll.properties[0].1, "global");
    }
}
