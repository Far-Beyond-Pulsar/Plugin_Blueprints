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
        let module = compile(&ClassSource { name: "Test", graph: &g, variables }, &registry)
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
    compile(&ClassSource { name: "Test", graph: &g, variables }, &natives()).expect_err("compile should fail")
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
    let module = compile(&ClassSource { name: "Ticker", graph: &built, variables: &vars }, &registry).unwrap();
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
    let module = compile(&ClassSource { name: "Hurt", graph: &built, variables: &[] }, &registry).unwrap();
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

    // Latent nodes are reported, not silently dropped.
    let mut g = Graph::default();
    g.event("bp", "begin_play").node("wait", "delay", &[P::ExecIn, P::In("milliseconds", "i64"), P::ExecOut("Completed")]);
    g.exec("bp", "Body", "wait");
    let d = errors(&g, &[]);
    assert!(d.iter().any(|d| d.message.contains("not supported")), "{d:?}");

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
    let module = compile(&ClassSource { name: "Natives", graph: &built, variables: &vars }, &registry)
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
