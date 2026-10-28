//! The Blueprint profile answers the canvas's questions about Blueprint meaning.

use std::sync::Arc;

use blueprint_editor_plugin::canvas_profile::BlueprintProfile;
use graphy::{Connection, DataType, GraphDescription, NodeInstance, Position};
use graphy_gpui::GraphProfile;
use graphy_render::{FixedAdvance, LayoutMetrics, Scene};

fn profile() -> BlueprintProfile {
    BlueprintProfile::blueprint(Arc::default(), Arc::default())
}

#[test]
fn catalogue_nodes_instantiate_and_lay_out() {
    let profile = profile();
    let catalog = profile.catalog();
    assert!(!catalog.is_empty(), "Blueprint catalogue is available");

    let mut graph = GraphDescription::new("t");
    for (i, entry) in catalog.iter().take(50).enumerate() {
        let node = profile
            .create_node(&entry.definition, &format!("n{i}"), Position::new(i as f64 * 300.0, 0.0))
            .unwrap_or_else(|| panic!("{} instantiates", entry.definition));
        assert_eq!(node.node_type, entry.definition);
        graph.add_node(node);
    }
    let mut scene = Scene::new(LayoutMetrics::default());
    scene.rebuild(&graph, profile.presenter(), &FixedAdvance(7.0));
    assert_eq!(scene.nodes.len(), graph.nodes.len());
}

#[test]
fn connection_rules_follow_blueprint_type_compatibility() {
    let profile = profile();
    let mut graph = GraphDescription::new("t");
    let mut a = NodeInstance::new("a", "x", Position::zero());
    a.add_output_pin("f", DataType::typed("f32"));
    a.add_output_pin("exec", DataType::Exec);
    let mut b = NodeInstance::new("b", "x", Position::zero());
    b.add_input_pin("f", DataType::typed("f32"));
    b.add_input_pin("s", DataType::typed("String"));
    b.add_input_pin("exec", DataType::Exec);
    graph.add_node(a);
    graph.add_node(b);
    let pin = |n: &str, p: &str| graphy::PinRef { node: n.into(), pin: p.into() };

    assert!(profile.can_connect(&graph, &pin("a", "f"), &pin("b", "f")).is_ok());
    assert!(profile.can_connect(&graph, &pin("a", "f"), &pin("b", "s")).is_err());
    assert!(profile.can_connect(&graph, &pin("a", "exec"), &pin("b", "f")).is_err());
    assert!(profile.can_connect(&graph, &pin("a", "exec"), &pin("b", "exec")).is_ok());
    assert!(profile.can_connect(&graph, &pin("a", "f"), &pin("a", "f")).is_err());
}

#[test]
fn reroute_takes_the_type_of_what_it_is_wired_to() {
    let profile = profile();
    let mut graph = GraphDescription::new("t");
    let mut src = NodeInstance::new("src", "x", Position::zero());
    src.add_output_pin("v", DataType::typed("f32"));
    graph.add_node(src);
    graph.add_node(profile.create_reroute("r", Position::zero(), &DataType::typed("?")));
    graph.add_connection(Connection::data("src", "v", "r", "input"));

    let update = profile.refresh_pins(&graph, "r").expect("reroute retypes");
    assert_eq!(update.inputs[0].data_type(), &DataType::typed("f32"));
    assert_eq!(update.outputs[0].data_type(), &DataType::typed("f32"));
    assert!(profile.refresh_pins(&graph, "src").is_none());
}

#[test]
fn subgraph_call_nodes_open_their_graph() {
    let profile = profile();
    let node = NodeInstance::new("m", "subgraph:macro:once-macro", Position::zero());
    assert_eq!(profile.macro_target(&node).map(|t| t.graph_id).as_deref(), Some("once-macro"));
    let plain = NodeInstance::new("p", "print_string", Position::zero());
    assert!(profile.macro_target(&plain).is_none());
}
