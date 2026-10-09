//! Per-canvas spatial indexes for graph interaction and viewport rendering.
//!
//! Queries are O(log n + k) for R-tree candidates (`k` is the local result
//! count). Revisions are carried by the graph's Vec-compatible collections,
//! so mutation paths invalidate the corresponding index without a full-graph
//! fingerprint on every pointer event.

use super::graph::BlueprintGraph;
use super::types::{BlueprintNode, Connection};
use crate::rendering::graph::NodeGraphRenderer;
use gpui::Point;
use rstar::{RTree, RTreeObject, AABB};
use std::collections::HashMap;

const WIRE_HIT_PADDING: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl GraphRect {
    pub fn around(point: Point<f32>, radius: f32) -> Self {
        let radius = radius.max(0.0);
        Self {
            min_x: point.x - radius,
            min_y: point.y - radius,
            max_x: point.x + radius,
            max_y: point.y + radius,
        }
    }

    pub fn from_corners(min: Point<f32>, max: Point<f32>) -> Self {
        Self {
            min_x: min.x.min(max.x),
            min_y: min.y.min(max.y),
            max_x: min.x.max(max.x),
            max_y: min.y.max(max.y),
        }
    }

    fn envelope(self) -> Option<AABB<[f32; 2]>> {
        let values = [self.min_x, self.min_y, self.max_x, self.max_y];
        if values.iter().all(|value| value.is_finite()) {
            Some(AABB::from_corners(
                [self.min_x, self.min_y],
                [self.max_x, self.max_y],
            ))
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct IndexedBounds {
    index: usize,
    envelope: AABB<[f32; 2]>,
}

impl RTreeObject for IndexedBounds {
    type Envelope = AABB<[f32; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope.clone()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WireGeometry {
    pub from: Point<f32>,
    pub to: Point<f32>,
}

/// Compact adjacency for node-to-wire updates. Two flat buffers avoid one
/// heap allocation and a `Vec` header for every node, including isolated
/// nodes in large pasted graphs.
#[derive(Default)]
struct ConnectionAdjacency {
    offsets: Vec<usize>,
    edges: Vec<usize>,
}

impl ConnectionAdjacency {
    fn edges_for(&self, node_index: usize) -> &[usize] {
        let Some((&start, &end)) = self
            .offsets
            .get(node_index)
            .zip(self.offsets.get(node_index + 1))
        else {
            return &[];
        };
        &self.edges[start..end]
    }

    fn build(graph: &BlueprintGraph, node_indices: &HashMap<String, usize>) -> Self {
        let mut offsets = vec![0; graph.nodes.len() + 1];
        for connection in &graph.connections {
            if let Some(&source) = node_indices.get(&connection.source_node) {
                offsets[source + 1] += 1;
            }
            if connection.target_node != connection.source_node {
                if let Some(&target) = node_indices.get(&connection.target_node) {
                    offsets[target + 1] += 1;
                }
            }
        }
        for index in 1..offsets.len() {
            offsets[index] += offsets[index - 1];
        }

        let mut edges = vec![0; *offsets.last().unwrap_or(&0)];
        for (edge_index, connection) in graph.connections.iter().enumerate() {
            if let Some(&source) = node_indices.get(&connection.source_node) {
                let cursor = offsets[source];
                edges[cursor] = edge_index;
                offsets[source] += 1;
            }
            if connection.target_node != connection.source_node {
                if let Some(&target) = node_indices.get(&connection.target_node) {
                    let cursor = offsets[target];
                    edges[cursor] = edge_index;
                    offsets[target] += 1;
                }
            }
        }
        // Filling advanced each node's start offset to its end offset. Shift
        // those cumulative ends right in place to recover the prefix table.
        for index in (0..graph.nodes.len()).rev() {
            offsets[index + 1] = offsets[index];
        }
        offsets[0] = 0;
        Self { offsets, edges }
    }
}

#[derive(Default)]
pub struct GraphSpatialIndex {
    initialized: bool,
    node_revision: u64,
    connection_revision: u64,
    comment_revision: u64,
    node_collection_id: u64,
    connection_collection_id: u64,
    comment_collection_id: u64,
    nodes: RTree<IndexedBounds>,
    comments: RTree<IndexedBounds>,
    wires: RTree<IndexedBounds>,
    // Keep only the old rectangle needed for R-tree removal. The tree owns
    // its own entry, so storing another full IndexedBounds per graph item
    // needlessly doubled the dominant spatial-index memory cost.
    node_entries: Vec<Option<GraphRect>>,
    comment_entries: Vec<Option<GraphRect>>,
    wire_entries: Vec<Option<GraphRect>>,
    node_indices_by_id: HashMap<String, usize>,
    comment_indices_by_id: HashMap<String, usize>,
    connections_by_node: ConnectionAdjacency,
    wire_geometry: Vec<Option<WireGeometry>>,
}

impl GraphSpatialIndex {
    /// Synchronize only structures whose tracked collection changed.
    pub fn ensure_current(&mut self, graph: &BlueprintGraph) {
        let nodes_changed = !self.initialized
            || self.node_collection_id != graph.nodes.collection_id()
            || self.node_revision != graph.nodes.revision();
        if nodes_changed {
            self.rebuild_nodes_and_wires(graph);
            self.rebuild_comments(graph);
            self.initialized = true;
            return;
        }

        if self.connection_collection_id != graph.connections.collection_id()
            || self.connection_revision != graph.connections.revision()
        {
            self.rebuild_wires(graph);
        }
        if self.comment_collection_id != graph.comments.collection_id()
            || self.comment_revision != graph.comments.revision()
        {
            self.rebuild_comments(graph);
        }
    }

    pub fn nodes_intersecting(&self, rect: GraphRect, reverse_order: bool) -> Vec<usize> {
        let Some(envelope) = rect.envelope() else {
            return Vec::new();
        };
        let mut result = self
            .nodes
            .locate_in_envelope_intersecting(&envelope)
            .map(|entry| entry.index)
            .collect::<Vec<_>>();
        sort_candidates(&mut result, reverse_order);
        result
    }

    pub fn comments_intersecting(&self, rect: GraphRect, reverse_order: bool) -> Vec<usize> {
        let Some(envelope) = rect.envelope() else {
            return Vec::new();
        };
        let mut result = self
            .comments
            .locate_in_envelope_intersecting(&envelope)
            .map(|entry| entry.index)
            .collect::<Vec<_>>();
        sort_candidates(&mut result, reverse_order);
        result
    }

    pub fn wires_intersecting(&self, rect: GraphRect) -> Vec<usize> {
        let Some(envelope) = rect.envelope() else {
            return Vec::new();
        };
        let mut result = self
            .wires
            .locate_in_envelope_intersecting(&envelope)
            .map(|entry| entry.index)
            .collect::<Vec<_>>();
        // Existing connection hit testing returns the first matching edge.
        result.sort_unstable();
        result
    }

    pub fn wire(&self, index: usize) -> Option<WireGeometry> {
        self.wire_geometry.get(index).copied().flatten()
    }

    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.node_indices_by_id.get(id).copied()
    }

    pub fn comment_index(&self, id: &str) -> Option<usize> {
        self.comment_indices_by_id.get(id).copied()
    }

    /// Incrementally update moved/resized nodes and their incident wires.
    /// `previous_revision` must match the index's current node revision; this
    /// prevents a local update from accidentally accepting an unrelated edit.
    pub fn sync_nodes_after_batch(
        &mut self,
        graph: &BlueprintGraph,
        previous_revision: u64,
        indices: &[usize],
    ) -> bool {
        if !self.initialized
            || self.node_revision != previous_revision
            || self.node_collection_id != graph.nodes.collection_id()
            || self.connection_collection_id != graph.connections.collection_id()
            || self.connection_revision != graph.connections.revision()
            || self.comment_collection_id != graph.comments.collection_id()
            || self.comment_revision != graph.comments.revision()
            || self.node_revision == graph.nodes.revision()
            || indices.is_empty()
        {
            return false;
        }

        let mut affected_connections = Vec::new();
        for &index in indices {
            let Some(node) = graph.nodes.get(index) else {
                return false;
            };
            Self::replace_entry(
                &mut self.nodes,
                &mut self.node_entries,
                index,
                node_bounds(node),
            );
            affected_connections.extend_from_slice(self.connections_by_node.edges_for(index));
        }
        affected_connections.sort_unstable();
        affected_connections.dedup();
        for index in affected_connections {
            self.update_wire(graph, index);
        }
        self.node_revision = graph.nodes.revision();
        true
    }

    /// Synchronize node and comment edits committed together by a drag batch.
    /// The old revisions are checked before either index is changed, so the
    /// update stays atomic from the cache's point of view.
    pub fn sync_geometry_after_batch(
        &mut self,
        graph: &BlueprintGraph,
        previous_node_revision: u64,
        node_indices: &[usize],
        previous_comment_revision: u64,
        comment_indices: &[usize],
    ) -> bool {
        if !self.initialized
            || self.node_revision != previous_node_revision
            || self.comment_revision != previous_comment_revision
            || self.node_collection_id != graph.nodes.collection_id()
            || self.comment_collection_id != graph.comments.collection_id()
            || self.connection_collection_id != graph.connections.collection_id()
            || self.connection_revision != graph.connections.revision()
            || (self.node_revision == graph.nodes.revision()
                && self.comment_revision == graph.comments.revision())
            || ((self.node_revision != graph.nodes.revision()) != !node_indices.is_empty())
            || ((self.comment_revision != graph.comments.revision()) != !comment_indices.is_empty())
        {
            return false;
        }

        for &index in comment_indices {
            let Some(comment) = graph.comments.get(index) else {
                return false;
            };
            Self::replace_entry(
                &mut self.comments,
                &mut self.comment_entries,
                index,
                rect_from_xywh(
                    comment.position.x,
                    comment.position.y,
                    comment.size.width,
                    comment.size.height,
                ),
            );
        }

        let mut affected_connections = Vec::new();
        for &index in node_indices {
            let Some(node) = graph.nodes.get(index) else {
                return false;
            };
            Self::replace_entry(
                &mut self.nodes,
                &mut self.node_entries,
                index,
                node_bounds(node),
            );
            affected_connections.extend_from_slice(self.connections_by_node.edges_for(index));
        }
        affected_connections.sort_unstable();
        affected_connections.dedup();
        for index in affected_connections {
            self.update_wire(graph, index);
        }

        self.node_revision = graph.nodes.revision();
        self.comment_revision = graph.comments.revision();
        true
    }

    /// Incrementally update comment rectangles after a drag or resize batch.
    pub fn sync_comments_after_batch(
        &mut self,
        graph: &BlueprintGraph,
        previous_revision: u64,
        indices: &[usize],
    ) -> bool {
        if !self.initialized
            || self.comment_revision != previous_revision
            || self.comment_collection_id != graph.comments.collection_id()
            || self.node_collection_id != graph.nodes.collection_id()
            || self.connection_collection_id != graph.connections.collection_id()
            || self.node_revision != graph.nodes.revision()
            || self.connection_revision != graph.connections.revision()
            || self.comment_revision == graph.comments.revision()
            || indices.is_empty()
        {
            return false;
        }
        for &index in indices {
            let Some(comment) = graph.comments.get(index) else {
                return false;
            };
            Self::replace_entry(
                &mut self.comments,
                &mut self.comment_entries,
                index,
                rect_from_xywh(
                    comment.position.x,
                    comment.position.y,
                    comment.size.width,
                    comment.size.height,
                ),
            );
        }
        self.comment_revision = graph.comments.revision();
        true
    }

    fn rebuild_nodes_and_wires(&mut self, graph: &BlueprintGraph) {
        self.node_entries = vec![None; graph.nodes.len()];
        self.node_indices_by_id.clear();
        let mut entries = Vec::with_capacity(graph.nodes.len());
        for (index, node) in graph.nodes.iter().enumerate() {
            self.node_indices_by_id
                .entry(node.id.clone())
                .or_insert(index);
            if let Some(envelope) = node_bounds(node).envelope() {
                let entry = IndexedBounds { index, envelope };
                self.node_entries[index] = Some(node_bounds(node));
                entries.push(entry);
            }
        }
        self.nodes = RTree::bulk_load(entries);
        self.node_revision = graph.nodes.revision();
        self.node_collection_id = graph.nodes.collection_id();
        self.rebuild_wires(graph);
    }

    fn rebuild_comments(&mut self, graph: &BlueprintGraph) {
        self.comment_entries = vec![None; graph.comments.len()];
        self.comment_indices_by_id.clear();
        let mut entries = Vec::with_capacity(graph.comments.len());
        for (index, comment) in graph.comments.iter().enumerate() {
            self.comment_indices_by_id
                .entry(comment.id.clone())
                .or_insert(index);
            if let Some(envelope) = rect_from_xywh(
                comment.position.x,
                comment.position.y,
                comment.size.width,
                comment.size.height,
            )
            .envelope()
            {
                let entry = IndexedBounds { index, envelope };
                self.comment_entries[index] = Some(rect_from_xywh(
                    comment.position.x,
                    comment.position.y,
                    comment.size.width,
                    comment.size.height,
                ));
                entries.push(entry);
            }
        }
        self.comments = RTree::bulk_load(entries);
        self.comment_revision = graph.comments.revision();
        self.comment_collection_id = graph.comments.collection_id();
    }

    fn rebuild_wires(&mut self, graph: &BlueprintGraph) {
        self.wire_entries = vec![None; graph.connections.len()];
        self.wire_geometry = vec![None; graph.connections.len()];
        self.connections_by_node = ConnectionAdjacency::build(graph, &self.node_indices_by_id);
        let mut entries = Vec::with_capacity(graph.connections.len());
        for (index, connection) in graph.connections.iter().enumerate() {
            let Some(geometry) = connection_geometry(connection, graph, &self.node_indices_by_id)
            else {
                continue;
            };
            self.wire_geometry[index] = Some(geometry);
            let bounds =
                rect_with_padding(wire_bounds(geometry.from, geometry.to), WIRE_HIT_PADDING);
            if let Some(envelope) = bounds.envelope() {
                let entry = IndexedBounds { index, envelope };
                self.wire_entries[index] = Some(bounds);
                entries.push(entry);
            }
        }
        self.wires = RTree::bulk_load(entries);
        self.connection_revision = graph.connections.revision();
        self.connection_collection_id = graph.connections.collection_id();
    }

    fn update_wire(&mut self, graph: &BlueprintGraph, index: usize) {
        Self::remove_entry(&mut self.wires, &mut self.wire_entries, index);
        let Some(connection) = graph.connections.get(index) else {
            if let Some(geometry) = self.wire_geometry.get_mut(index) {
                *geometry = None;
            }
            return;
        };
        let Some(geometry) = connection_geometry(connection, graph, &self.node_indices_by_id)
        else {
            if let Some(geometry) = self.wire_geometry.get_mut(index) {
                *geometry = None;
            }
            return;
        };
        self.wire_geometry[index] = Some(geometry);
        Self::insert_entry(
            &mut self.wires,
            &mut self.wire_entries,
            index,
            rect_with_padding(wire_bounds(geometry.from, geometry.to), WIRE_HIT_PADDING),
        );
    }

    fn insert_entry(
        tree: &mut RTree<IndexedBounds>,
        entries: &mut Vec<Option<GraphRect>>,
        index: usize,
        rect: GraphRect,
    ) {
        if index >= entries.len() {
            entries.resize(index + 1, None);
        }
        if let Some(envelope) = rect.envelope() {
            let entry = IndexedBounds { index, envelope };
            tree.insert(entry);
            entries[index] = Some(rect);
        }
    }

    fn remove_entry(
        tree: &mut RTree<IndexedBounds>,
        entries: &mut Vec<Option<GraphRect>>,
        index: usize,
    ) {
        if let Some(rect) = entries.get_mut(index).and_then(Option::take) {
            if let Some(envelope) = rect.envelope() {
                tree.remove(&IndexedBounds { index, envelope });
            }
        }
    }

    fn replace_entry(
        tree: &mut RTree<IndexedBounds>,
        entries: &mut Vec<Option<GraphRect>>,
        index: usize,
        rect: GraphRect,
    ) {
        Self::remove_entry(tree, entries, index);
        Self::insert_entry(tree, entries, index, rect);
    }
}

fn sort_candidates(indices: &mut Vec<usize>, reverse_order: bool) {
    indices.sort_unstable();
    if reverse_order {
        indices.reverse();
    }
}

fn node_bounds(node: &BlueprintNode) -> GraphRect {
    rect_from_xywh(
        node.position.x,
        node.position.y,
        node.size.width,
        node.size.height,
    )
}

fn rect_from_xywh(x: f32, y: f32, width: f32, height: f32) -> GraphRect {
    GraphRect::from_corners(
        Point::new(x, y),
        Point::new(x + width.max(0.0), y + height.max(0.0)),
    )
}

fn rect_with_padding(rect: GraphRect, padding: f32) -> GraphRect {
    GraphRect {
        min_x: rect.min_x - padding,
        min_y: rect.min_y - padding,
        max_x: rect.max_x + padding,
        max_y: rect.max_y + padding,
    }
}

fn connection_geometry(
    connection: &Connection,
    graph: &BlueprintGraph,
    node_indices: &HashMap<String, usize>,
) -> Option<WireGeometry> {
    let from_index = *node_indices.get(&connection.source_node)?;
    let to_index = *node_indices.get(&connection.target_node)?;
    let from_node = graph.nodes.get(from_index)?;
    let to_node = graph.nodes.get(to_index)?;
    let from = NodeGraphRenderer::calculate_pin_position_graph_space(
        from_node,
        &connection.source_pin,
        false,
    )
    .unwrap_or_else(|| {
        Point::new(
            from_node.position.x + from_node.size.width,
            from_node.position.y + from_node.size.height * 0.5,
        )
    });
    let to = NodeGraphRenderer::calculate_pin_position_graph_space(
        to_node,
        &connection.target_pin,
        true,
    )
    .unwrap_or_else(|| {
        Point::new(
            to_node.position.x,
            to_node.position.y + to_node.size.height * 0.5,
        )
    });
    Some(WireGeometry { from, to })
}

fn wire_bounds(from: Point<f32>, to: Point<f32>) -> GraphRect {
    let control_offset = ((to.x - from.x).abs() * 0.45).clamp(55.0, 220.0);
    let c1 = Point::new(from.x + control_offset, from.y);
    let c2 = Point::new(to.x - control_offset, to.y);
    GraphRect::from_corners(
        Point::new(
            from.x.min(c1.x).min(c2.x).min(to.x),
            from.y.min(c1.y).min(c2.y).min(to.y),
        ),
        Point::new(
            from.x.max(c1.x).max(c2.x).max(to.x),
            from.y.max(c1.y).max(c2.y).max(to.y),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{BlueprintComment, NodeType, Pin, PinDataType, PinType};
    use crate::rendering::graph::bezier;
    use blueprint_graph::ConnectionType;
    use gpui::{Hsla, Size};
    use std::collections::BTreeSet;

    // Deterministic generator so failures reproduce.
    struct Lcg(u64);

    impl Lcg {
        fn next_u32(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }

        fn unit(&mut self) -> f32 {
            self.next_u32() as f32 / (1u64 << 31) as f32
        }

        fn below(&mut self, n: usize) -> usize {
            self.next_u32() as usize % n
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

    fn node(id: &str, x: f32, y: f32) -> BlueprintNode {
        BlueprintNode {
            id: id.to_string(),
            definition_id: "test".to_string(),
            title: id.to_string(),
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

    fn reroute(id: &str, x: f32, y: f32) -> BlueprintNode {
        let mut node = BlueprintNode::create_reroute(Point::new(x, y));
        node.id = id.to_string();
        node
    }

    fn connection(index: usize, from: (&str, &str), to: (&str, &str)) -> Connection {
        Connection {
            id: format!("c{index}"),
            source_node: from.0.to_string(),
            source_pin: from.1.to_string(),
            target_node: to.0.to_string(),
            target_pin: to.1.to_string(),
            connection_type: ConnectionType::Data,
        }
    }

    fn comment(id: &str, x: f32, y: f32, width: f32, height: f32) -> BlueprintComment {
        BlueprintComment {
            id: id.to_string(),
            text: id.to_string(),
            position: Point::new(x, y),
            size: Size::new(width, height),
            color: Hsla::default(),
            contained_node_ids: Vec::new(),
            is_selected: false,
            color_picker_state: None,
        }
    }

    fn graph(
        nodes: Vec<BlueprintNode>,
        connections: Vec<Connection>,
        comments: Vec<BlueprintComment>,
    ) -> BlueprintGraph {
        BlueprintGraph {
            nodes: nodes.into(),
            connections: connections.into(),
            comments: comments.into(),
            zoom_level: 1.0,
            ..Default::default()
        }
    }

    /// `node_count` nodes scattered over a grid, `connection_count` random
    /// wires between them (including reroutes and the `__return__` header
    /// pin), and a few comments.
    fn random_graph(node_count: usize, connection_count: usize, seed: u64) -> BlueprintGraph {
        let mut rng = Lcg(seed);
        let columns = (node_count as f32).sqrt().ceil() as usize;
        let nodes = (0..node_count)
            .map(|i| {
                let x = (i % columns) as f32 * 260.0 + rng.unit() * 60.0;
                let y = (i / columns) as f32 * 140.0 + rng.unit() * 30.0;
                if i % 17 == 0 {
                    reroute(&format!("n{i}"), x, y)
                } else {
                    node(&format!("n{i}"), x, y)
                }
            })
            .collect::<Vec<_>>();
        let connections = (0..connection_count)
            .map(|i| {
                let from = rng.below(node_count);
                // Mostly local wires, as real graphs have, plus some long ones.
                let to = if i % 10 == 0 {
                    rng.below(node_count)
                } else {
                    (from + 1 + rng.below(columns + 1)).min(node_count - 1)
                };
                let output = match rng.below(3) {
                    0 => "out0",
                    1 => "out1",
                    _ => "__return__",
                };
                let input = if rng.below(2) == 0 { "in0" } else { "in1" };
                connection(i, (&nodes[from].id, output), (&nodes[to].id, input))
            })
            .collect::<Vec<_>>();
        let comments = (0..node_count / 50 + 1)
            .map(|i| {
                comment(
                    &format!("m{i}"),
                    rng.unit() * columns as f32 * 260.0,
                    rng.unit() * columns as f32 * 140.0,
                    300.0 + rng.unit() * 400.0,
                    200.0 + rng.unit() * 300.0,
                )
            })
            .collect::<Vec<_>>();
        graph(nodes, connections, comments)
    }

    fn built(graph: &BlueprintGraph) -> GraphSpatialIndex {
        let mut index = GraphSpatialIndex::default();
        index.ensure_current(graph);
        index
    }

    fn intersects(a: GraphRect, b: GraphRect) -> bool {
        a.min_x <= b.max_x && b.min_x <= a.max_x && a.min_y <= b.max_y && b.min_y <= a.max_y
    }

    fn probes(graph: &BlueprintGraph, seed: u64) -> Vec<GraphRect> {
        let (mut max_x, mut max_y) = (0.0f32, 0.0f32);
        for node in graph.nodes.iter() {
            max_x = max_x.max(node.position.x + node.size.width);
            max_y = max_y.max(node.position.y + node.size.height);
        }
        let mut rng = Lcg(seed);
        (0..300)
            .map(|i| {
                let center = Point::new(rng.unit() * max_x, rng.unit() * max_y);
                // Points, pointer-sized radii and viewport-sized rectangles.
                let radius = [0.0, 12.0, 30.0, 400.0][i % 4];
                GraphRect::around(center, radius)
            })
            .collect()
    }

    /// `incremental` answers every query exactly as a fresh build would.
    fn assert_matches_rebuild(incremental: &GraphSpatialIndex, graph: &BlueprintGraph) {
        let fresh = built(graph);
        for index in 0..graph.connections.len() {
            let (a, b) = (incremental.wire(index), fresh.wire(index));
            assert_eq!(
                a.map(|w| (w.from, w.to)),
                b.map(|w| (w.from, w.to)),
                "wire {index} geometry"
            );
        }
        for (i, probe) in probes(graph, 7).into_iter().enumerate() {
            assert_eq!(
                incremental.nodes_intersecting(probe, true),
                fresh.nodes_intersecting(probe, true),
                "nodes, probe {i}"
            );
            assert_eq!(
                incremental.comments_intersecting(probe, false),
                fresh.comments_intersecting(probe, false),
                "comments, probe {i}"
            );
            assert_eq!(
                incremental.wires_intersecting(probe),
                fresh.wires_intersecting(probe),
                "wires, probe {i}"
            );
        }
    }

    #[test]
    fn node_and_comment_queries_match_a_linear_scan() {
        let graph = random_graph(400, 900, 1);
        let index = built(&graph);
        for (i, probe) in probes(&graph, 2).into_iter().enumerate() {
            let expected_nodes = graph
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| intersects(node_bounds(node), probe))
                .map(|(index, _)| index)
                .rev()
                .collect::<Vec<_>>();
            assert_eq!(
                index.nodes_intersecting(probe, true),
                expected_nodes,
                "probe {i}"
            );
            let expected_comments = graph
                .comments
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    let rect =
                        rect_from_xywh(c.position.x, c.position.y, c.size.width, c.size.height);
                    intersects(rect, probe)
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            assert_eq!(
                index.comments_intersecting(probe, false),
                expected_comments,
                "probe {i}"
            );
        }
    }

    #[test]
    fn every_point_on_a_wire_finds_that_wire() {
        // The hover test only checks the wires the index returns, so a point
        // within the hover threshold of a curve must return that curve.
        const THRESHOLD: f32 = 12.0;
        let graph = random_graph(200, 500, 3);
        let index = built(&graph);
        for (wire_index, _) in graph.connections.iter().enumerate() {
            let wire = index.wire(wire_index).expect("both endpoints exist");
            let offset = ((wire.to.x - wire.from.x).abs() * 0.45).clamp(55.0, 220.0);
            let c1 = (wire.from.x + offset, wire.from.y);
            let c2 = (wire.to.x - offset, wire.to.y);
            for step in 0..=32 {
                let (x, y) = bezier(
                    (wire.from.x, wire.from.y),
                    c1,
                    c2,
                    (wire.to.x, wire.to.y),
                    step as f32 / 32.0,
                );
                for (dx, dy) in [(0.0, 0.0), (THRESHOLD, 0.0), (0.0, -THRESHOLD)] {
                    let candidates = index.wires_intersecting(GraphRect::around(
                        Point::new(x + dx, y + dy),
                        THRESHOLD,
                    ));
                    assert!(
                        candidates.contains(&wire_index),
                        "wire {wire_index} missing at t={step}/32 offset ({dx}, {dy})"
                    );
                }
            }
        }
        // Far from every node, nothing is a candidate.
        assert!(index
            .wires_intersecting(GraphRect::around(Point::new(-5000.0, -5000.0), THRESHOLD))
            .is_empty());
    }

    #[test]
    fn overlapping_nodes_return_the_topmost_first() {
        let graph = graph(
            vec![
                node("below", 0.0, 0.0),
                node("far", 1000.0, 0.0),
                node("above", 40.0, 20.0),
            ],
            Vec::new(),
            Vec::new(),
        );
        let index = built(&graph);
        let point = GraphRect::around(Point::new(60.0, 40.0), 0.0);
        // Later nodes draw on top, so hit testing walks them first.
        assert_eq!(index.nodes_intersecting(point, true), vec![2, 0]);
        assert_eq!(index.nodes_intersecting(point, false), vec![0, 2]);
        assert_eq!(index.node_index("above"), Some(2));
        assert_eq!(index.node_index("missing"), None);
        assert!(index
            .nodes_intersecting(GraphRect::around(Point::new(500.0, 500.0), 10.0), true)
            .is_empty());
    }

    #[test]
    fn special_pins_and_reroutes_place_wire_endpoints() {
        let mut conversion = node("conv", 400.0, 0.0);
        conversion.node_type = NodeType::Conversion;
        conversion.size = Size::new(60.0, 24.0);
        let graph = graph(
            vec![node("a", 0.0, 0.0), reroute("r", 300.0, 200.0), conversion],
            vec![
                connection(0, ("a", "__return__"), ("r", "input")),
                connection(1, ("r", "output"), ("conv", "in0")),
                connection(2, ("conv", "out0"), ("a", "missing_pin")),
                connection(3, ("a", "out0"), ("deleted", "in0")),
            ],
            Vec::new(),
        );
        let index = built(&graph);

        // `__return__` sits in the header, 24 units in from the right edge.
        let wire = index.wire(0).unwrap();
        assert_eq!(
            wire.from,
            Point::new(160.0 - 24.0, crate::rendering::graph::HEADER_H * 0.5)
        );
        // A reroute's pins are both its centre.
        assert_eq!(wire.to, Point::new(315.0, 215.0));
        assert_eq!(index.wire(1).unwrap().from, Point::new(315.0, 215.0));
        // A conversion pill's pins are the middle of its left and right edges.
        assert_eq!(index.wire(1).unwrap().to, Point::new(400.0, 12.0));
        assert_eq!(index.wire(2).unwrap().from, Point::new(460.0, 12.0));
        // An unknown pin falls back to the node's left edge middle.
        assert_eq!(index.wire(2).unwrap().to, Point::new(0.0, 40.0));
        // A wire to a deleted node has no geometry and is never a candidate.
        assert!(index.wire(3).is_none());
        let everything = GraphRect::from_corners(Point::new(-1e4, -1e4), Point::new(1e4, 1e4));
        assert_eq!(index.wires_intersecting(everything), vec![0, 1, 2]);
        // A reroute is found by a point at its centre.
        assert_eq!(
            index.nodes_intersecting(GraphRect::around(Point::new(315.0, 215.0), 0.0), true),
            vec![1]
        );
    }

    #[test]
    fn pin_queries_widen_with_zoom_out() {
        // Pin hit tests query nodes within a 12-pixel screen radius, which is
        // 12 / zoom graph units (`nearby_node_indices`). A pointer just
        // outside a node reaches it when zoomed out, not when zoomed in.
        let graph = graph(vec![node("a", 0.0, 0.0)], Vec::new(), Vec::new());
        let index = built(&graph);
        let outside = Point::new(160.0 + 20.0, 40.0);
        for (zoom, expected) in [
            (2.0f32, vec![]),
            (1.0, vec![]),
            (0.5, vec![0]),
            (0.1, vec![0]),
        ] {
            let radius = 12.0 / zoom.max(0.05);
            assert_eq!(
                index.nodes_intersecting(GraphRect::around(outside, radius), true),
                expected,
                "zoom {zoom}"
            );
        }
    }

    #[test]
    fn incremental_node_moves_and_resizes_match_a_rebuild() {
        let mut graph = random_graph(300, 700, 4);
        let mut index = built(&graph);
        let mut rng = Lcg(5);
        for round in 0..25 {
            // A single-node drag, a multi-node drag, then a resize (the
            // renderer widens nodes whose labels do not fit).
            let count = [1, 12, 3][round % 3];
            let indices = (0..count)
                .map(|_| rng.below(graph.nodes.len()))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let previous = graph.nodes.revision();
            let (dx, dy) = (rng.unit() * 2000.0 - 1000.0, rng.unit() * 2000.0 - 1000.0);
            let grow = rng.unit() * 120.0;
            graph.nodes.with_mut(|nodes| {
                for &i in &indices {
                    if round % 3 == 2 {
                        nodes[i].size.width += grow;
                    } else {
                        nodes[i].position.x += dx;
                        nodes[i].position.y += dy;
                    }
                }
            });
            assert!(
                index.sync_nodes_after_batch(&graph, previous, &indices),
                "round {round} fell back to a rebuild"
            );
            assert_matches_rebuild(&index, &graph);
        }
    }

    #[test]
    fn incremental_drag_of_nodes_and_comments_matches_a_rebuild() {
        let mut graph = random_graph(250, 600, 6);
        let mut index = built(&graph);
        let mut rng = Lcg(8);
        for round in 0..20 {
            let node_indices = if round % 4 == 3 {
                Vec::new()
            } else {
                vec![rng.below(graph.nodes.len())]
            };
            // The editor only syncs when something moved.
            let comment_indices = if round % 2 == 0 || node_indices.is_empty() {
                vec![rng.below(graph.comments.len())]
            } else {
                Vec::new()
            };
            let previous_nodes = graph.nodes.revision();
            let previous_comments = graph.comments.revision();
            let (dx, dy) = (rng.unit() * 600.0 - 300.0, rng.unit() * 600.0 - 300.0);
            // Same shape as the editor's drag: one `with_mut` per collection,
            // only for collections that have something to move.
            if !node_indices.is_empty() {
                graph.nodes.with_mut(|nodes| {
                    for &i in &node_indices {
                        nodes[i].position.x += dx;
                        nodes[i].position.y += dy;
                    }
                });
            }
            if !comment_indices.is_empty() {
                graph.comments.with_mut(|comments| {
                    for &i in &comment_indices {
                        comments[i].position.x += dx;
                        comments[i].size.height += dy.abs();
                    }
                });
            }
            assert!(
                index.sync_geometry_after_batch(
                    &graph,
                    previous_nodes,
                    &node_indices,
                    previous_comments,
                    &comment_indices,
                ),
                "round {round} fell back to a rebuild"
            );
            assert_matches_rebuild(&index, &graph);
        }

        let previous = graph.comments.revision();
        graph
            .comments
            .with_mut(|comments| comments[0].size.width += 50.0);
        assert!(index.sync_comments_after_batch(&graph, previous, &[0]));
        assert_matches_rebuild(&index, &graph);
    }

    #[test]
    fn stale_or_unrelated_edits_refuse_the_incremental_path() {
        let mut graph = random_graph(60, 120, 9);
        let mut index = built(&graph);

        // A connection edit since the last sync: the wire set changed, so a
        // node-only update must not be accepted.
        let previous = graph.nodes.revision();
        graph.connections.pop();
        graph.nodes.with_mut(|nodes| nodes[3].position.x += 500.0);
        assert!(!index.sync_nodes_after_batch(&graph, previous, &[3]));
        index.ensure_current(&graph);
        assert_matches_rebuild(&index, &graph);

        // A node edit the index has not seen yet (through `DerefMut`, as
        // undoing a move does) before the caller's batch.
        graph.nodes[5].position.y += 300.0;
        let previous = graph.nodes.revision();
        graph.nodes.with_mut(|nodes| nodes[6].position.y += 300.0);
        assert!(!index.sync_nodes_after_batch(&graph, previous, &[6]));
        index.ensure_current(&graph);
        assert_matches_rebuild(&index, &graph);

        // A replaced collection (undo, tab switch) always rebuilds.
        let previous = graph.nodes.revision();
        graph.nodes = graph.nodes.iter().cloned().collect();
        assert!(!index.sync_nodes_after_batch(&graph, previous, &[0]));
        index.ensure_current(&graph);
        assert_matches_rebuild(&index, &graph);

        // Nothing changed: there is nothing to sync.
        let previous = graph.nodes.revision();
        assert!(!index.sync_nodes_after_batch(&graph, previous, &[0]));
    }
}
