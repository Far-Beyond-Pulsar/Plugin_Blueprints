use blueprint_compiler::{authored::expand_graph, compile, ClassSource, VariableSource};
use blueprint_graph::codec::deserialize_blueprint;
use pulsar_script_vm::{Budget, Host, NativeRegistry, Program, Value, Vm};
use std::sync::Arc;

const FIXTURE: &str = include_str!("../../graph/tests/fixtures/graph_save.json");

#[test]
fn saved_graph_runs_headlessly_and_macro_state_is_stable_and_independent() {
    let asset = deserialize_blueprint(FIXTURE).unwrap();
    let expand = || {
        expand_graph(
            &asset.main_graph,
            asset
                .local_macros
                .iter()
                .map(|m| (m.id.clone(), m.graph.clone())),
        )
        .unwrap()
    };
    let graph = expand();
    assert!(graph.nodes.contains_key("first__once"));
    assert!(graph.nodes.contains_key("second__once"));
    let natives = NativeRegistry::new();
    let vars = [VariableSource {
        id: Some("count-id".into()),
        name: "count".into(),
        type_name: "i64".into(),
        default: Some(serde_json::json!(0)),
    }];
    let source = ClassSource {
        name: "Fixture",
        graph: &graph,
        variables: &vars,
        events: &[],
        known_events: &[],
        version: 0,
    };
    let module = compile(&source, &natives).unwrap();
    let repeated = expand();
    let again = compile(
        &ClassSource {
            graph: &repeated,
            ..source
        },
        &natives,
    )
    .unwrap();
    assert_eq!(module, again);
    let hidden: Vec<_> = module
        .variables
        .iter()
        .filter(|v| v.name.starts_with("__bp_"))
        .map(|v| v.name.as_str())
        .collect();
    assert_eq!(hidden.len(), 2);
    assert!(hidden.iter().any(|name| name.ends_with("first__once")));
    assert!(hidden.iter().any(|name| name.ends_with("second__once")));
    let program = Program::link(Arc::new(module), &natives).unwrap();
    let mut world = pulsar_scenedb::World::new();
    let entity = world.spawn();
    let mut vm = Vm::default();
    let mut first = program.instantiate();
    let mut second = program.instantiate();
    let entry = program.entry("begin_play").unwrap();
    let count = program.variable("count").unwrap();
    vm.start(
        &program,
        &mut first,
        entry,
        &[],
        &mut Host::new(&mut world, entity),
        &mut Budget::new(1000),
    )
    .unwrap();
    assert_eq!(program.var(&first, count), Some(&Value::Int(42)));
    assert_eq!(program.var(&second, count), Some(&Value::Int(0)));
    vm.start(
        &program,
        &mut second,
        entry,
        &[],
        &mut Host::new(&mut world, entity),
        &mut Budget::new(1000),
    )
    .unwrap();
    assert_eq!(program.var(&second, count), Some(&Value::Int(42)));
}

#[test]
fn macro_expansion_cannot_overwrite_an_authored_node() {
    let mut asset = deserialize_blueprint(FIXTURE).unwrap();
    let mut conflicting = asset.main_graph.nodes["set"].clone();
    conflicting.id = "first__once".into();
    asset
        .main_graph
        .nodes
        .insert(conflicting.id.clone(), conflicting);
    let error = expand_graph(
        &asset.main_graph,
        asset
            .local_macros
            .iter()
            .map(|m| (m.id.clone(), m.graph.clone())),
    )
    .unwrap_err();
    assert!(error.contains("overwrite node 'first__once'"));
}

#[test]
fn invalid_connections_and_missing_macros_are_errors_not_dropped_edges() {
    let mut asset = deserialize_blueprint(FIXTURE).unwrap();
    asset.main_graph.connections[0].source_pin = "missing".into();
    assert!(expand_graph(&asset.main_graph, [])
        .unwrap_err()
        .contains("c1"));
    asset = deserialize_blueprint(FIXTURE).unwrap();
    assert!(expand_graph(&asset.main_graph, []).is_err());
}
