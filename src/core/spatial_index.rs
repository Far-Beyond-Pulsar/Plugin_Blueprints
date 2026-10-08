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
    pub bounds: GraphRect,
}

#[derive(Default)]
struct NodePinPositions {
    inputs: HashMap<String, Point<f32>>,
    outputs: HashMap<String, Point<f32>>,
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
    node_entries: Vec<Option<IndexedBounds>>,
    comment_entries: Vec<Option<IndexedBounds>>,
    wire_entries: Vec<Option<IndexedBounds>>,
    node_indices_by_id: HashMap<String, usize>,
    node_pin_positions: Vec<NodePinPositions>,
    comment_indices_by_id: HashMap<String, usize>,
    connections_by_node: HashMap<String, Vec<usize>>,
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
            if index >= self.node_pin_positions.len() {
                return false;
            }
            let Some(node) = graph.nodes.get(index) else {
                return false;
            };
            Self::replace_entry(
                &mut self.nodes,
                &mut self.node_entries,
                index,
                node_bounds(node),
            );
            refresh_node_pin_positions(&mut self.node_pin_positions[index], node);
            if let Some(edges) = self.connections_by_node.get(&node.id) {
                affected_connections.extend_from_slice(edges);
            }
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
            if index >= self.node_pin_positions.len() {
                return false;
            }
            let Some(node) = graph.nodes.get(index) else {
                return false;
            };
            Self::replace_entry(
                &mut self.nodes,
                &mut self.node_entries,
                index,
                node_bounds(node),
            );
            refresh_node_pin_positions(&mut self.node_pin_positions[index], node);
            if let Some(edges) = self.connections_by_node.get(&node.id) {
                affected_connections.extend_from_slice(edges);
            }
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
        self.node_pin_positions = Vec::with_capacity(graph.nodes.len());
        self.node_indices_by_id.clear();
        let mut entries = Vec::with_capacity(graph.nodes.len());
        for (index, node) in graph.nodes.iter().enumerate() {
            self.node_pin_positions.push(node_pin_positions(node));
            self.node_indices_by_id
                .entry(node.id.clone())
                .or_insert(index);
            if let Some(envelope) = node_bounds(node).envelope() {
                let entry = IndexedBounds { index, envelope };
                self.node_entries[index] = Some(entry.clone());
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
                self.comment_entries[index] = Some(entry.clone());
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
        self.connections_by_node.clear();
        let mut entries = Vec::with_capacity(graph.connections.len());
        for (index, connection) in graph.connections.iter().enumerate() {
            self.connections_by_node
                .entry(connection.source_node.clone())
                .or_default()
                .push(index);
            if connection.target_node != connection.source_node {
                self.connections_by_node
                    .entry(connection.target_node.clone())
                    .or_default()
                    .push(index);
            }
            let Some(geometry) = connection_geometry(
                connection,
                graph,
                &self.node_indices_by_id,
                &self.node_pin_positions,
            ) else {
                continue;
            };
            self.wire_geometry[index] = Some(geometry);
            if let Some(envelope) = rect_with_padding(geometry.bounds, WIRE_HIT_PADDING).envelope()
            {
                let entry = IndexedBounds { index, envelope };
                self.wire_entries[index] = Some(entry.clone());
                entries.push(entry);
            }
        }
        self.wires = RTree::bulk_load(entries);
        self.connection_revision = graph.connections.revision();
        self.connection_collection_id = graph.connections.collection_id();
    }

    fn update_wire(&mut self, graph: &BlueprintGraph, index: usize) {
        if let Some(entry) = self.wire_entries.get_mut(index).and_then(Option::take) {
            self.wires.remove(&entry);
        }
        let Some(connection) = graph.connections.get(index) else {
            if let Some(geometry) = self.wire_geometry.get_mut(index) {
                *geometry = None;
            }
            return;
        };
        let Some(geometry) = connection_geometry(
            connection,
            graph,
            &self.node_indices_by_id,
            &self.node_pin_positions,
        ) else {
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
            rect_with_padding(geometry.bounds, WIRE_HIT_PADDING),
        );
    }

    fn insert_entry(
        tree: &mut RTree<IndexedBounds>,
        entries: &mut Vec<Option<IndexedBounds>>,
        index: usize,
        rect: GraphRect,
    ) {
        if index >= entries.len() {
            entries.resize(index + 1, None);
        }
        if let Some(envelope) = rect.envelope() {
            let entry = IndexedBounds { index, envelope };
            tree.insert(entry.clone());
            entries[index] = Some(entry);
        }
    }

    fn replace_entry(
        tree: &mut RTree<IndexedBounds>,
        entries: &mut Vec<Option<IndexedBounds>>,
        index: usize,
        rect: GraphRect,
    ) {
        if let Some(entry) = entries.get_mut(index).and_then(Option::take) {
            tree.remove(&entry);
        }
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

fn node_pin_positions(node: &BlueprintNode) -> NodePinPositions {
    let mut positions = NodePinPositions::default();
    refresh_node_pin_positions(&mut positions, node);
    positions
}

fn refresh_node_pin_positions(positions: &mut NodePinPositions, node: &BlueprintNode) {
    positions.inputs.reserve(node.inputs.len());
    positions.outputs.reserve(node.outputs.len() + 1);
    for (row, pin) in node.inputs.iter().enumerate() {
        let position =
            NodeGraphRenderer::calculate_pin_position_graph_space_for_row(node, &pin.id, true, row);
        if let Some(cached) = positions.inputs.get_mut(&pin.id) {
            *cached = position;
        } else {
            positions.inputs.insert(pin.id.clone(), position);
        }
    }
    for (row, pin) in node.outputs.iter().enumerate() {
        let position = NodeGraphRenderer::calculate_pin_position_graph_space_for_row(
            node, &pin.id, false, row,
        );
        if let Some(cached) = positions.outputs.get_mut(&pin.id) {
            *cached = position;
        } else {
            positions.outputs.insert(pin.id.clone(), position);
        }
    }
    if let Some(position) =
        NodeGraphRenderer::calculate_pin_position_graph_space(node, "__return__", false)
    {
        if let Some(cached) = positions.outputs.get_mut("__return__") {
            *cached = position;
        } else {
            positions.outputs.insert("__return__".to_owned(), position);
        }
    }
}

fn connection_geometry(
    connection: &Connection,
    graph: &BlueprintGraph,
    node_indices: &HashMap<String, usize>,
    pin_positions: &[NodePinPositions],
) -> Option<WireGeometry> {
    let from_index = *node_indices.get(&connection.source_node)?;
    let to_index = *node_indices.get(&connection.target_node)?;
    let from_node = graph.nodes.get(from_index)?;
    let to_node = graph.nodes.get(to_index)?;
    let from = pin_positions
        .get(from_index)?
        .outputs
        .get(&connection.source_pin)
        .copied()
        .unwrap_or_else(|| {
            Point::new(
                from_node.position.x + from_node.size.width,
                from_node.position.y + from_node.size.height * 0.5,
            )
        });
    let to = pin_positions
        .get(to_index)?
        .inputs
        .get(&connection.target_pin)
        .copied()
        .unwrap_or_else(|| {
            Point::new(
                to_node.position.x,
                to_node.position.y + to_node.size.height * 0.5,
            )
        });
    let control_offset = ((to.x - from.x).abs() * 0.45).clamp(55.0, 220.0);
    let c1 = Point::new(from.x + control_offset, from.y);
    let c2 = Point::new(to.x - control_offset, to.y);
    let bounds = GraphRect::from_corners(
        Point::new(
            from.x.min(c1.x).min(c2.x).min(to.x),
            from.y.min(c1.y).min(c2.y).min(to.y),
        ),
        Point::new(
            from.x.max(c1.x).max(c2.x).max(to.x),
            from.y.max(c1.y).max(c2.y).max(to.y),
        ),
    );
    Some(WireGeometry { from, to, bounds })
}
