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
