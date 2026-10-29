//! The authored schema must survive the trip through the editing document with
//! every id, field and ordering intact.

use blueprint_editor_plugin::io::canvas_doc::{from_authored, to_authored};
use graphy_blueprint as authored;
use graphy_blueprint::codec::deserialize_blueprint;

const CURRENT: &str = include_str!(
    "../../../../crates/third-party/graphy/crates/graphy-blueprint/tests/fixtures/graph_save.json"
);

/// A pre-unified graph: object-form comment size and HSLA colour.
const LEGACY: &str = r#"{
  "nodes": {
    "a": {"id": "a", "node_type": "begin_play", "position": {"x": 1.5, "y": -2.0}, "properties": {"k": "v"},
          "inputs": [],
          "outputs": [{"id": "exec", "name": "Exec", "pin_type": "Output", "data_type": "Execution", "connected_to": ["e1"]}]},
    "b": {"id": "b", "node_type": "print_string", "position": {"x": 40.0, "y": 8.0}, "properties": {},
          "inputs": [{"id": "in", "name": "In", "pin_type": "Input", "data_type": "Execution", "connected_to": ["e1"]}],
          "outputs": []}
  },
  "connections": [{"id": "e1", "source_node": "a", "source_pin": "exec", "target_node": "b", "target_pin": "in", "connection_type": "Execution"}],
  "metadata": {"name": "Legacy", "description": "", "version": "1.0.0", "created_at": "c", "modified_at": "m"},
  "comments": [{"id": "c1", "text": "hello", "position": {"x": 0.0, "y": 0.0}, "size": {"width": 100.0, "height": 50.0},
                "color": {"h": 0.5, "s": 0.5, "l": 0.5, "a": 1.0}, "contained_node_ids": ["a"]}]
}"#;

fn json<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap()
}

fn strip_backlinks(n: &authored::NodeInstance) -> authored::NodeInstance {
    let mut n = n.clone();
    for p in n.inputs.iter_mut().chain(n.outputs.iter_mut()) {
        p.pin.connected_to.clear();
    }
    n
}

fn assert_lossless(graph: &authored::GraphDescription) {
    let (doc, side) = from_authored(graph);
    let back = to_authored(&doc, &side);

    assert_eq!(graph.nodes.len(), back.nodes.len());
    for (key, n) in &graph.nodes {
        let b = &back.nodes[key];
        // `connected_to` is derived from the connections and rebuilt on save
        // (the editor has always written it consistent); everything else is
        // compared verbatim.
        assert_eq!(json(&strip_backlinks(n)), json(&strip_backlinks(b)), "node {key}");
    }
    assert_eq!(json(&graph.connections), json(&back.connections), "connection ids and order");
    for c in &back.connections {
        let src = &back.nodes[&c.source_node].outputs.iter().find(|p| p.id == c.source_pin).unwrap().pin;
        let dst = &back.nodes[&c.target_node].inputs.iter().find(|p| p.id == c.target_pin).unwrap().pin;
        assert!(src.connected_to.contains(&c.id) && dst.connected_to.contains(&c.id));
    }
    assert_eq!(json(&graph.comments), json(&back.comments), "comments");
    assert_eq!(json(&graph.metadata), json(&back.metadata), "metadata");
    assert_eq!(json(&graph.custom_event_defs), json(&back.custom_event_defs));

    // The document is a fixed point.
    let (doc2, side2) = from_authored(&back);
    assert_eq!(json(&to_authored(&doc2, &side2)), json(&back));
}

#[test]
fn current_fixture_round_trips_through_the_editing_document() {
    let asset = deserialize_blueprint(CURRENT).expect("fixture parses");
    assert_lossless(&asset.main_graph);
    assert!(!asset.subgraphs.is_empty());
    for sub in &asset.subgraphs {
        assert_lossless(&sub.graph);
    }
}

#[test]
fn legacy_graph_round_trips_through_the_editing_document() {
    let graph = authored::legacy::try_parse_legacy_format(LEGACY).expect("legacy parses");
    assert_lossless(&graph);
    let (doc, _) = from_authored(&graph);
    assert_eq!(doc.comments[0].id, "c1");
    assert_eq!(doc.connections[0].id.as_deref(), Some("e1"));
}

#[test]
fn session_made_edges_get_stable_deterministic_ids() {
    use graphy::{Connection, DataType, GraphDescription, NodeInstance, Position};
    let mut doc = GraphDescription::new("t");
    let mut a = NodeInstance::new("a", "x", Position::zero());
    a.add_output_pin("o", DataType::Exec);
    let mut b = NodeInstance::new("b", "x", Position::zero());
    b.add_input_pin("i", DataType::Exec);
    doc.add_node(a);
    doc.add_node(b);
    doc.add_connection(Connection::execution("a", "o", "b", "i"));
    let side = Default::default();
    let first = to_authored(&doc, &side);
    let second = to_authored(&doc, &side);
    assert_eq!(first.connections[0].id, second.connections[0].id);
    assert_eq!(
        first.nodes["a"].outputs[0].pin.connected_to,
        vec![first.connections[0].id.clone()]
    );
}
