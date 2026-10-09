//! Headless benchmark for the graph spatial index (Pulsar-Native#1072).
//!
//!   cargo run --release --example spatial_index_stress
//!
//! For graphs of 1k, 5k and 10k nodes it prints, per pointer query, how many
//! candidates the index returns and how long the query takes, next to a
//! linear scan of every node and wire. It also times a single-node drag
//! update (the incremental path) against a full rebuild. Each size runs with
//! local wires only, then with one wire in twenty crossing the graph: a long
//! wire's bounding box covers much of the graph, so it is a candidate for
//! many pointer positions.
//!
//! There is no pass/fail threshold; this is for profiling only.

use blueprint_editor_plugin::{
    BlueprintGraph, BlueprintNode, Connection, GraphRect, GraphSpatialIndex, NodeType, Pin,
    PinDataType, PinType,
};
use blueprint_graph::ConnectionType;
use gpui::{Point, Size};
use std::collections::HashMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

const SIZES: &[usize] = &[1_000, 5_000, 10_000];
/// Wires per node, roughly what dense gameplay graphs have.
const WIRES_PER_NODE: usize = 3;
const QUERIES: usize = 20_000;
/// Hover threshold for wires and the pin search radius at zoom 1.
const POINTER_RADIUS: f32 = 12.0;
const COLUMN_STRIDE: f32 = 260.0;
const ROW_STRIDE: f32 = 140.0;

struct Lcg(u64);

impl Lcg {
    fn unit(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn below(&mut self, n: usize) -> usize {
        ((self.unit() * n as f32) as usize).min(n - 1)
    }
}

fn pin(id: &str, pin_type: PinType) -> Pin {
    Pin {
        id: id.to_string(),
        name: id.to_string(),
        pin_type,
        data_type: PinDataType::from_type_str("f32"),
    }
}

fn node(index: usize, x: f32, y: f32) -> BlueprintNode {
    BlueprintNode {
        id: format!("n{index}"),
        definition_id: "bench".to_string(),
        title: format!("Node {index}"),
        icon: String::new(),
        node_type: NodeType::Logic,
        position: Point::new(x, y),
        size: Size::new(160.0, 80.0),
        inputs: vec![pin("in0", PinType::Input), pin("in1", PinType::Input)],
        outputs: vec![pin("out0", PinType::Output), pin("out1", PinType::Output)],
        properties: HashMap::new(),
        is_selected: false,
        description: String::new(),
        color: None,
    }
}

fn graph(node_count: usize, long_every: Option<usize>, rng: &mut Lcg) -> (BlueprintGraph, usize) {
    let columns = (node_count as f32).sqrt().ceil() as usize;
    let nodes = (0..node_count)
        .map(|i| {
            node(
                i,
                (i % columns) as f32 * COLUMN_STRIDE + rng.unit() * 60.0,
                (i / columns) as f32 * ROW_STRIDE + rng.unit() * 30.0,
            )
        })
        .collect::<Vec<_>>();
    let connections = (0..node_count * WIRES_PER_NODE)
        .map(|i| {
            let from = rng.below(node_count);
            let to = if long_every.is_some_and(|every| i % every == 0) {
                rng.below(node_count)
            } else {
                // A neighbour one or two columns right, up to a row away.
                let column = (from % columns + 1 + rng.below(2)).min(columns - 1);
                let row = (from / columns + rng.below(3)).saturating_sub(1);
                (row * columns + column).min(node_count - 1)
            };
            Connection {
                id: format!("c{i}"),
                source_node: nodes[from].id.clone(),
                source_pin: if i % 2 == 0 { "out0" } else { "out1" }.to_string(),
                target_node: nodes[to].id.clone(),
                target_pin: if i % 3 == 0 { "in0" } else { "in1" }.to_string(),
                connection_type: ConnectionType::Data,
            }
        })
        .collect::<Vec<_>>();
    let graph = BlueprintGraph {
        nodes: nodes.into(),
        connections: connections.into(),
        zoom_level: 1.0,
        ..Default::default()
    };
    (graph, columns)
}

fn per_query(total: Duration) -> String {
    format!("{:.2} us", total.as_secs_f64() * 1e6 / QUERIES as f64)
}

fn run(node_count: usize, long_every: Option<usize>) {
    let mut rng = Lcg(node_count as u64);
    let (mut graph, columns) = graph(node_count, long_every, &mut rng);
    let rows = node_count.div_ceil(columns);
    let points = (0..QUERIES)
        .map(|_| {
            Point::new(
                rng.unit() * columns as f32 * COLUMN_STRIDE,
                rng.unit() * rows as f32 * ROW_STRIDE,
            )
        })
        .collect::<Vec<_>>();

    let start = Instant::now();
    let mut index = GraphSpatialIndex::default();
    index.ensure_current(&graph);
    let build = start.elapsed();

    // Node hit test (pointer inside a node) and pin search radius.
    let (mut node_candidates, mut wire_candidates) = (0usize, 0usize);
    let start = Instant::now();
    for &point in &points {
        node_candidates +=
            black_box(index.nodes_intersecting(GraphRect::around(point, POINTER_RADIUS), true))
                .len();
    }
    let node_time = start.elapsed();
    let start = Instant::now();
    for &point in &points {
        wire_candidates +=
            black_box(index.wires_intersecting(GraphRect::around(point, POINTER_RADIUS))).len();
    }
    let wire_time = start.elapsed();

    // A linear scan per query: every node's bounds and every wire's bounds,
    // with endpoints resolved once up front (the pre-index hover path also
    // searched the node list for each wire's endpoints).
    let ids = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.clone(), i))
        .collect::<HashMap<_, _>>();
    let start = Instant::now();
    let mut linear_hits = 0usize;
    for &point in points.iter().take(QUERIES / 20) {
        let rect = GraphRect::around(point, POINTER_RADIUS);
        linear_hits += graph
            .nodes
            .iter()
            .filter(|n| {
                n.position.x <= rect.max_x
                    && rect.min_x <= n.position.x + n.size.width
                    && n.position.y <= rect.max_y
                    && rect.min_y <= n.position.y + n.size.height
            })
            .count();
        for connection in graph.connections.iter() {
            let (Some(&a), Some(&b)) = (
                ids.get(&connection.source_node),
                ids.get(&connection.target_node),
            ) else {
                continue;
            };
            let (a, b) = (&graph.nodes[a].position, &graph.nodes[b].position);
            let pad = 30.0 + 220.0;
            if a.x.min(b.x) - pad <= rect.max_x
                && rect.min_x <= a.x.max(b.x) + pad
                && a.y.min(b.y) - pad <= rect.max_y
                && rect.min_y <= a.y.max(b.y) + pad
            {
                linear_hits += 1;
            }
        }
    }
    black_box(linear_hits);
    let linear_time = start.elapsed() * 20;

    // One node dragged by one step: the editor's incremental update.
    let moves = 2_000;
    let start = Instant::now();
    for step in 0..moves {
        let i = step * 7919 % node_count;
        let previous = graph.nodes.revision();
        graph.nodes.with_mut(|nodes| nodes[i].position.x += 10.0);
        let previous_comments = graph.comments.revision();
        assert!(index.sync_geometry_after_batch(&graph, previous, &[i], previous_comments, &[]));
    }
    let incremental = start.elapsed() / moves as u32;
    // The same edit through `DerefMut` (undo/redo of a move) rebuilds.
    let rebuilds = 20;
    let start = Instant::now();
    for step in 0..rebuilds {
        graph.nodes[step].position.y += 10.0;
        index.ensure_current(&graph);
    }
    let rebuild = start.elapsed() / rebuilds as u32;

    println!(
        "{node_count:>6} nodes, {:>6} wires, {:<7} | build {:>9.2?} | node query {:>9} ({:.2} candidates) | wire query {:>9} ({:.2} candidates) | linear scan {:>10} | 1-node move: incremental {:>9.2?}, rebuild {:>9.2?}",
        graph.connections.len(),
        if long_every.is_some() { "5% long" } else { "local" },
        build,
        per_query(node_time),
        node_candidates as f64 / QUERIES as f64,
        per_query(wire_time),
        wire_candidates as f64 / QUERIES as f64,
        per_query(linear_time),
        incremental,
        rebuild,
    );
}

fn main() {
    if cfg!(debug_assertions) {
        println!("debug build: run with --release for representative times");
    }
    for &size in SIZES {
        run(size, None);
        run(size, Some(20));
    }
}
