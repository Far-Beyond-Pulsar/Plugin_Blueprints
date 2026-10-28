use anyhow::{anyhow, Result};
use graphy::core::GraphComment;
use graphy::{Connection, ConnectionType, DataType, GraphDescription, NodeInstance};
use graphy_gpui::GraphProfile as _;
use graphy_render::{FixedAdvance, LayoutMetrics, Scene};
use plugin_editor_api::{AiToolDefinition, PluginError};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tool_registry::{PluginToolRegistry, ToolContext, ToolRegistry};
use tool_registry_macros::tool;
use tracing::debug;

use crate::core::definitions::NodeDefinitions;
use crate::canvas_profile::BlueprintProfile;
use crate::core::types::NodeType;
use crate::features::macros::operations::type_string;
use crate::io::canvas_doc::{connection_id as edge_id, property_string, property_value};

#[derive(Clone)]
struct BlueprintAiSession {
    file_key: PathBuf,
    graph: GraphDescription,
    dirty: bool,
}

pub(crate) struct AiGraphUpdate {
    pub before: GraphDescription,
    pub after: GraphDescription,
    response: smol::channel::Sender<Result<AiGraphUpdateAck, String>>,
}

pub(crate) struct AiGraphUpdateAck {
    pub graph: GraphDescription,
    pub applied: bool,
}

impl AiGraphUpdate {
    pub(crate) async fn respond(self, result: Result<AiGraphUpdateAck, String>) {
        let _ = self.response.send(result).await;
    }
}

#[derive(Default)]
struct RuntimeState {
    active_file: Option<PathBuf>,
    sessions: HashMap<PathBuf, BlueprintAiSession>,
    live_editors: HashMap<PathBuf, smol::channel::Sender<AiGraphUpdate>>,
}

static STATE: OnceLock<Mutex<RuntimeState>> = OnceLock::new();
thread_local! {
    static REQUESTED_FILE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

struct RequestedFileScope(Option<PathBuf>);

impl RequestedFileScope {
    fn enter(file_path: &Path) -> Self {
        let requested = normalize_file_key(file_path);
        let previous = REQUESTED_FILE.with(|slot| slot.replace(Some(requested)));
        Self(previous)
    }
}

impl Drop for RequestedFileScope {
    fn drop(&mut self) {
        REQUESTED_FILE.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

fn state() -> &'static Mutex<RuntimeState> {
    STATE.get_or_init(|| Mutex::new(RuntimeState::default()))
}

fn normalize_file_key(file_path: &Path) -> PathBuf {
    let candidate = if file_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "graph_save.json")
    {
        file_path.parent().unwrap_or(file_path).to_path_buf()
    } else {
        file_path.to_path_buf()
    };

    candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.to_path_buf())
}

pub(crate) fn same_file_key(left: &Path, right: &Path) -> bool {
    normalize_file_key(left) == normalize_file_key(right)
}

pub fn upsert_session(file_path: PathBuf, graph: GraphDescription) {
    let key = normalize_file_key(&file_path);
    if let Ok(mut guard) = state().lock() {
        guard.active_file = Some(key.clone());
        guard.sessions.insert(
            key.clone(),
            BlueprintAiSession {
                file_key: key,
                graph,
                dirty: false,
            },
        );
    }
}

/// Register the UI-thread bridge used to apply AI edits to the live canvas.
pub(crate) fn register_live_editor(
    file_path: PathBuf,
    graph: GraphDescription,
) -> smol::channel::Receiver<AiGraphUpdate> {
    let key = normalize_file_key(&file_path);
    let (sender, receiver) = smol::channel::unbounded();
    if let Ok(mut guard) = state().lock() {
        guard.active_file = Some(key.clone());
        guard.sessions.insert(
            key.clone(),
            BlueprintAiSession {
                file_key: key.clone(),
                graph,
                dirty: false,
            },
        );
        guard.live_editors.insert(key, sender);
    }
    receiver
}

fn set_active_file(file_path: &Path) {
    if let Ok(mut guard) = state().lock() {
        guard.active_file = Some(normalize_file_key(file_path));
    }
}

fn resolve_session_key(guard: &RuntimeState, requested: &Path) -> Option<PathBuf> {
    let requested = normalize_file_key(requested);
    if guard.sessions.contains_key(&requested) {
        return Some(requested);
    }
    None
}

fn with_session<R>(f: impl FnOnce(&BlueprintAiSession) -> Result<R>) -> Result<R> {
    let guard = state()
        .lock()
        .map_err(|_| anyhow!("AI session state lock poisoned"))?;
    let requested = requested_file(&guard)?;
    let key = resolve_session_key(&guard, &requested).ok_or_else(|| {
        anyhow!(
            "Blueprint is not open in editor: {}. Call open_file_in_default_editor first.",
            requested.display()
        )
    })?;
    let session = guard
        .sessions
        .get(&key)
        .ok_or_else(|| anyhow!("Blueprint is not open in editor: {}", key.display()))?;
    f(session)
}

fn with_session_mut<R>(f: impl FnOnce(&mut BlueprintAiSession) -> Result<R>) -> Result<R> {
    let mut guard = state()
        .lock()
        .map_err(|_| anyhow!("AI session state lock poisoned"))?;
    let requested = requested_file(&guard)?;
    let key = resolve_session_key(&guard, &requested).ok_or_else(|| {
        anyhow!(
            "Blueprint is not open in editor: {}. Call open_file_in_default_editor first.",
            requested.display()
        )
    })?;
    let route = guard
        .live_editors
        .get(&key)
        .cloned()
        .ok_or_else(|| anyhow!("No live Blueprint editor is attached to {}", key.display()))?;
    let session = guard
        .sessions
        .get_mut(&key)
        .ok_or_else(|| anyhow!("Blueprint is not open in editor: {}", key.display()))?;
    let before = session.graph.clone();
    let was_dirty = session.dirty;
    let result = match f(session) {
        Ok(result) => result,
        Err(error) => {
            session.graph = before;
            session.dirty = was_dirty;
            return Err(error);
        }
    };
    if !graphs_match(&session.graph, &before) {
        let (response, receiver) = smol::channel::bounded(1);
        if route
            .send_blocking(AiGraphUpdate {
                before: before.clone(),
                after: session.graph.clone(),
                response,
            })
            .is_err()
        {
            session.graph = before;
            session.dirty = was_dirty;
            return Err(anyhow!("Live Blueprint editor is no longer available"));
        }
        match receiver.recv_blocking() {
            Ok(Ok(ack)) if ack.applied => {
                session.graph = ack.graph;
            }
            Ok(Ok(ack)) => {
                session.graph = ack.graph;
                session.dirty = true;
                return Err(anyhow!("The live graph changed since the AI snapshot. The session was refreshed; retry the edit."));
            }
            Ok(Err(error)) => {
                session.graph = before;
                session.dirty = was_dirty;
                return Err(anyhow!(
                    "Could not apply AI edit to live Blueprint: {error}"
                ));
            }
            Err(_) => {
                session.graph = before;
                session.dirty = was_dirty;
                return Err(anyhow!(
                    "Live Blueprint editor did not acknowledge the AI edit"
                ));
            }
        }
    }
    Ok(result)
}

fn requested_file(guard: &RuntimeState) -> Result<PathBuf> {
    REQUESTED_FILE
        .with(|slot| slot.borrow().clone().or_else(|| guard.active_file.clone()))
        .ok_or_else(|| anyhow!("No active blueprint file in AI session"))
}

/// Everything the AI snapshot compares: nodes, wires and comments in a stable
/// order, independent of hash-map iteration order.
fn graph_content_fingerprint(graph: &GraphDescription) -> String {
    let mut ids: Vec<&String> = graph.nodes.keys().collect();
    ids.sort();
    let nodes = ids
        .into_iter()
        .map(|id| {
            let node = &graph.nodes[id];
            let pins = |pins: &[graphy::PinInstance]| {
                pins.iter()
                    .map(|p| format!("{}:{}:{}", p.id, p.pin.name, type_string(p.data_type())))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let mut props: Vec<String> = node
                .properties
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            props.sort();
            format!(
                "{}|{}|{:?}|{}|{}|{}",
                node.id,
                node.node_type,
                (node.position.x, node.position.y),
                pins(&node.inputs),
                pins(&node.outputs),
                props.join(";")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let connections = graph
        .connections
        .iter()
        .map(|c| {
            format!(
                "{}|{}|{}|{}|{}|{:?}",
                edge_id(c),
                c.source_node,
                c.source_pin,
                c.target_node,
                c.target_pin,
                c.connection_type
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let comments = graph
        .comments
        .iter()
        .map(|c| {
            format!(
                "{}|{}|{:?}|{:?}|{:?}|{:?}",
                c.id,
                c.text,
                (c.position.x, c.position.y),
                c.size,
                c.color,
                c.contained_node_ids
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{nodes}\n--connections--\n{connections}\n--comments--\n{comments}")
}

pub(crate) fn graphs_match(left: &GraphDescription, right: &GraphDescription) -> bool {
    graph_content_fingerprint(left) == graph_content_fingerprint(right)
}

fn is_blueprint_file(file_path: &Path) -> bool {
    if file_path.is_dir() {
        return true;
    }

    file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "class")
        || file_path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == "graph_save.json")
}

fn blueprint_profile() -> BlueprintProfile {
    BlueprintProfile::blueprint(Default::default())
}

/// The title a node shows: its catalogue name, or its type.
fn node_title(node: &NodeInstance) -> String {
    NodeDefinitions::load()
        .get_node_definition(&node.node_type)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| node.node_type.replace('_', " "))
}

fn properties_as_text(node: &NodeInstance) -> HashMap<String, String> {
    node.properties
        .iter()
        .map(|(k, v)| (k.clone(), property_string(v)))
        .collect()
}

/// Laid-out rectangles of the graph's nodes and comments, with text measured
/// by a fixed advance (tools have no GPU fonts).
fn laid_out(graph: &GraphDescription) -> Scene {
    let profile = blueprint_profile();
    let mut scene = Scene::new(LayoutMetrics::default());
    scene.rebuild(graph, profile.presenter(), &FixedAdvance::default());
    scene
}

fn node_size(scene: &Scene, id: &str) -> (f32, f32) {
    scene
        .node(id)
        .map_or((0.0, 0.0), |n| (n.layout.size.x, n.layout.size.y))
}

fn node_to_json(node: &NodeInstance, scene: &Scene) -> Value {
    let (width, height) = node_size(scene, &node.id);
    let pin = |pin: &graphy::PinInstance, kind: &str| {
        json!({
            "id": pin.id,
            "name": pin.pin.name,
            "pin_type": kind,
            "data_type": type_string(pin.data_type()),
        })
    };
    json!({
        "id": node.id,
        "definition_id": node.node_type,
        "title": node_title(node),
        "node_type": format!("{:?}", NodeType::of(&node.node_type)),
        "position": { "x": node.position.x, "y": node.position.y },
        "size": { "width": width, "height": height },
        "inputs": node.inputs.iter().map(|p| pin(p, "Input")).collect::<Vec<_>>(),
        "outputs": node.outputs.iter().map(|p| pin(p, "Output")).collect::<Vec<_>>(),
        "properties": properties_as_text(node),
        "description": NodeDefinitions::load()
            .get_node_definition(&node.node_type)
            .map(|d| d.description.clone())
            .unwrap_or_default(),
    })
}

fn comment_to_json(comment: &GraphComment) -> Value {
    json!({
        "id": comment.id,
        "text": comment.text,
        "position": { "x": comment.position.x, "y": comment.position.y },
        "size": { "width": comment.size.0, "height": comment.size.1 },
        "contained_node_ids": comment.contained_node_ids,
    })
}

fn connection_to_json(connection: &Connection) -> Value {
    json!({
        "id": edge_id(connection),
        "source_node": connection.source_node,
        "source_pin": connection.source_pin,
        "target_node": connection.target_node,
        "target_pin": connection.target_pin,
        "connection_type": format!("{:?}", connection.connection_type),
    })
}

/// Recomputes which nodes each comment box fully contains.
fn update_comment_containment(graph: &mut GraphDescription) {
    if graph.comments.is_empty() {
        return;
    }
    let scene = laid_out(graph);
    let mut contained: Vec<Vec<String>> = Vec::with_capacity(graph.comments.len());
    for placed in &scene.comments {
        let mut ids: Vec<String> = scene
            .nodes
            .iter()
            .filter(|n| {
                let r = n.rect();
                r.min.x >= placed.rect.min.x
                    && r.min.y >= placed.rect.min.y
                    && r.max.x <= placed.rect.max.x
                    && r.max.y <= placed.rect.max.y
            })
            .map(|n| n.id.clone())
            .collect();
        ids.sort();
        contained.push(ids);
    }
    for (comment, ids) in graph.comments.iter_mut().zip(contained) {
        comment.contained_node_ids = ids;
    }
}

fn is_reroute(graph: &GraphDescription, id: &str) -> bool {
    graph.nodes.get(id).is_some_and(NodeInstance::is_reroute)
}

fn propagate_reroute_types(graph: &mut GraphDescription, start_node_id: &str, data_type: DataType) {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([start_node_id.to_owned()]);

    while let Some(node_id) = queue.pop_front() {
        if !visited.insert(node_id.clone()) {
            continue;
        }
        if !is_reroute(graph, &node_id) {
            continue;
        }
        if let Some(node) = graph.nodes.get_mut(&node_id) {
            for pin in node.inputs.iter_mut().chain(node.outputs.iter_mut()) {
                pin.pin.data_type = data_type.clone();
            }
        }
        for connection in &graph.connections {
            let neighbor = if connection.source_node == node_id {
                Some(&connection.target_node)
            } else if connection.target_node == node_id {
                Some(&connection.source_node)
            } else {
                None
            };
            if let Some(neighbor) = neighbor {
                if is_reroute(graph, neighbor) {
                    queue.push_back(neighbor.clone());
                }
            }
        }
    }
}

fn parse_properties(value: Option<Value>) -> Result<HashMap<String, String>> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };

    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("properties must be a JSON object"))?;
    let mut map = HashMap::new();
    for (key, val) in obj {
        if let Some(s) = val.as_str() {
            map.insert(key.clone(), s.to_string());
        } else {
            map.insert(key.clone(), val.to_string());
        }
    }
    Ok(map)
}

fn parse_connection_type(
    kind: Option<String>,
    source_type: &DataType,
    target_type: &DataType,
) -> Result<ConnectionType> {
    if source_type.is_execution() != target_type.is_execution() {
        return Err(anyhow!("Execution pins can only connect to execution pins"));
    }

    let expected = if target_type.is_execution() {
        ConnectionType::Execution
    } else {
        ConnectionType::Data
    };
    let Some(kind) = kind else {
        return Ok(expected);
    };

    let requested = match kind.trim().to_ascii_lowercase().as_str() {
        "execution" | "exec" => ConnectionType::Execution,
        "data" => ConnectionType::Data,
        _ => return Err(anyhow!("connection_type must be `execution` or `data`")),
    };
    if requested != expected {
        return Err(anyhow!(
            "connection_type does not match the connected pin types (expected {:?})",
            expected
        ));
    }
    Ok(requested)
}

fn tool_registry() -> &'static ToolRegistry {
    static REGISTRY: OnceLock<ToolRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = ToolRegistry::new();
        registry.merge_plugin(&PluginToolRegistry::from_namespace(module_path!()));
        registry
    })
}

fn sorted_nodes(graph: &GraphDescription) -> Vec<&NodeInstance> {
    let mut nodes: Vec<&NodeInstance> = graph.nodes.values().collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    nodes
}

/// Summarize graph-wide structure and counts.
#[tool(category = "blueprint")]
pub fn blueprint_query_graph() -> Result<Value> {
    with_session(|session| {
        let graph = &session.graph;
        Ok(json!({
            "ok": true,
            "apply_mode": "editor_state",
            "open_file": session.file_key.display().to_string(),
            "dirty": session.dirty,
            "counts": {
                "nodes": graph.nodes.len(),
                "connections": graph.connections.len(),
                "comments": graph.comments.len(),
            },
        }))
    })
}

/// List nodes with optional filtering and pagination.
#[tool(category = "blueprint")]
pub fn blueprint_list_nodes(
    query: Option<String>,
    definition_id: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<Value> {
    with_session(|session| {
        let offset = offset.unwrap_or(0).max(0) as usize;
        let limit = limit.unwrap_or(200).clamp(1, 1000) as usize;
        let query_norm = query.as_ref().map(|q| q.to_ascii_lowercase());
        let scene = laid_out(&session.graph);

        let matched = sorted_nodes(&session.graph)
            .into_iter()
            .filter(|node| {
                if let Some(def) = definition_id.as_ref() {
                    if &node.node_type != def {
                        return false;
                    }
                }

                if let Some(q) = query_norm.as_ref() {
                    let haystack = format!(
                        "{} {} {} {}",
                        node.id,
                        node_title(node),
                        node.node_type,
                        properties_as_text(node)
                            .iter()
                            .map(|(k, v)| format!("{}:{}", k, v))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                    .to_ascii_lowercase();
                    haystack.contains(q)
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();

        let items = matched
            .iter()
            .skip(offset)
            .take(limit)
            .map(|node| node_to_json(node, &scene))
            .collect::<Vec<_>>();

        Ok(json!({
            "ok": true,
            "total_matches": matched.len(),
            "offset": offset,
            "limit": limit,
            "items": items,
        }))
    })
}

/// Get one node by id.
#[tool(category = "blueprint")]
pub fn blueprint_get_node(id: String) -> Result<Value> {
    with_session(|session| {
        let node = session.graph.nodes.get(&id);
        let scene = laid_out(&session.graph);
        Ok(json!({
            "ok": true,
            "found": node.is_some(),
            "node": node.map(|n| node_to_json(n, &scene)),
        }))
    })
}

/// Add a node from node definitions to the current graph.
#[tool(category = "blueprint")]
pub fn blueprint_add_node(
    definition_id: String,
    x: f64,
    y: f64,
    title: Option<String>,
    properties: Option<Value>,
) -> Result<Value> {
    // Titles are derived from the node's type and are not stored.
    let _ = title;
    with_session_mut(|session| {
        let mut node = blueprint_profile()
            .create_node(
                &definition_id,
                &uuid::Uuid::new_v4().to_string(),
                graphy::Position::new(x, y),
            )
            .ok_or_else(|| anyhow!("Unknown node definition_id: {}", definition_id))?;

        for (key, value) in parse_properties(properties)? {
            node.properties.insert(key, property_value(&value));
        }

        let node_id = node.id.clone();
        session.graph.add_node(node);
        update_comment_containment(&mut session.graph);
        session.dirty = true;

        let scene = laid_out(&session.graph);
        let node = &session.graph.nodes[&node_id];
        Ok(json!({
            "ok": true,
            "node_id": node_id,
            "node": node_to_json(node, &scene),
            "counts": {
                "nodes": session.graph.nodes.len(),
                "connections": session.graph.connections.len(),
                "comments": session.graph.comments.len(),
            }
        }))
    })
}

/// Update node position/properties.
#[tool(category = "blueprint")]
pub fn blueprint_update_node(
    id: String,
    title: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    properties_merge: Option<Value>,
    replace_properties: Option<bool>,
) -> Result<Value> {
    // Titles are derived from the node's type and are not stored.
    let _ = title;
    with_session_mut(|session| {
        {
            let node = session
                .graph
                .nodes
                .get_mut(&id)
                .ok_or_else(|| anyhow!("Node not found: {}", id))?;

            if let Some(x) = x {
                node.position.x = x;
            }
            if let Some(y) = y {
                node.position.y = y;
            }

            if let Some(props) = properties_merge {
                let parsed = parse_properties(Some(props))?;
                if replace_properties.unwrap_or(false) {
                    node.properties.clear();
                }
                for (k, v) in parsed {
                    node.properties.insert(k, property_value(&v));
                }
            }
        }

        update_comment_containment(&mut session.graph);
        session.dirty = true;

        let scene = laid_out(&session.graph);
        Ok(json!({
            "ok": true,
            "node": node_to_json(&session.graph.nodes[&id], &scene),
        }))
    })
}

/// Remove a node and all attached connections.
#[tool(category = "blueprint")]
pub fn blueprint_remove_node(id: String) -> Result<Value> {
    with_session_mut(|session| {
        // Macro entry/exit nodes are structural — never allow deletion.
        if let Some(node) = session.graph.nodes.get(&id) {
            if matches!(
                NodeType::of(&node.node_type),
                NodeType::MacroEntry | NodeType::MacroExit
            ) {
                return Ok(json!({
                    "ok": false,
                    "error": "Cannot delete macro entry or return nodes",
                    "removed": false,
                }));
            }
        }

        let before_connections = session.graph.connections.len();
        let removed = session.graph.remove_node(&id);
        update_comment_containment(&mut session.graph);
        if removed {
            session.dirty = true;
        }

        Ok(json!({
            "ok": true,
            "removed": removed,
            "removed_connections": before_connections.saturating_sub(session.graph.connections.len()),
            "counts": {
                "nodes": session.graph.nodes.len(),
                "connections": session.graph.connections.len(),
                "comments": session.graph.comments.len(),
            }
        }))
    })
}

/// List comments and their contained node counts.
#[tool(category = "blueprint")]
pub fn blueprint_list_comments() -> Result<Value> {
    with_session(|session| {
        let items = session
            .graph
            .comments
            .iter()
            .map(|comment| {
                let mut value = comment_to_json(comment);
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "contained_node_count".to_string(),
                        json!(comment.contained_node_ids.len()),
                    );
                }
                value
            })
            .collect::<Vec<_>>();

        Ok(json!({ "ok": true, "items": items, "total": items.len() }))
    })
}

/// List node details contained by a comment box.
#[tool(category = "blueprint")]
pub fn blueprint_nodes_in_comment(comment_id: String) -> Result<Value> {
    with_session(|session| {
        let comment = session
            .graph
            .comments
            .iter()
            .find(|c| c.id == comment_id)
            .ok_or_else(|| anyhow!("Comment not found: {}", comment_id))?;

        let scene = laid_out(&session.graph);
        let items = sorted_nodes(&session.graph)
            .into_iter()
            .filter(|node| comment.contained_node_ids.contains(&node.id))
            .map(|node| node_to_json(node, &scene))
            .collect::<Vec<_>>();

        Ok(json!({
            "ok": true,
            "comment": comment_to_json(comment),
            "nodes": items,
            "count": items.len(),
        }))
    })
}

/// Add a new comment box.
#[tool(category = "blueprint")]
pub fn blueprint_add_comment(
    text: String,
    x: f64,
    y: f64,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<Value> {
    with_session_mut(|session| {
        let mut comment = GraphComment::new(
            text,
            graphy::Position::new(x, y),
            (width.unwrap_or(300.0), height.unwrap_or(200.0)),
        );
        comment.id = uuid::Uuid::new_v4().to_string();
        comment.color = [0.5, 0.3, 0.2, 0.3];

        let id = comment.id.clone();
        session.graph.comments.push(comment);
        update_comment_containment(&mut session.graph);
        session.dirty = true;

        let comment = session.graph.comments.last().expect("just pushed");
        Ok(json!({ "ok": true, "comment_id": id, "comment": comment_to_json(comment) }))
    })
}

/// Remove a comment box.
#[tool(category = "blueprint")]
pub fn blueprint_remove_comment(comment_id: String) -> Result<Value> {
    with_session_mut(|session| {
        let before = session.graph.comments.len();
        session.graph.comments.retain(|c| c.id != comment_id);
        let removed = before != session.graph.comments.len();
        if removed {
            session.dirty = true;
        }

        Ok(json!({
            "ok": true,
            "removed": removed,
            "comments": session.graph.comments.len(),
        }))
    })
}

/// List connections with optional node filter.
#[tool(category = "blueprint")]
pub fn blueprint_list_connections(node_id: Option<String>) -> Result<Value> {
    with_session(|session| {
        let items = session
            .graph
            .connections
            .iter()
            .filter(|conn| {
                if let Some(id) = node_id.as_ref() {
                    &conn.source_node == id || &conn.target_node == id
                } else {
                    true
                }
            })
            .map(connection_to_json)
            .collect::<Vec<_>>();

        Ok(json!({ "ok": true, "total": items.len(), "items": items }))
    })
}

/// Add a connection between two node pins.
#[tool(category = "blueprint")]
pub fn blueprint_add_connection(
    source_node: String,
    source_pin: String,
    target_node: String,
    target_pin: String,
    connection_type: Option<String>,
) -> Result<Value> {
    with_session_mut(|session| {
        let source_type = session
            .graph
            .nodes
            .get(&source_node)
            .ok_or_else(|| anyhow!("Source node not found: {}", source_node))
            .and_then(|node| {
                node.outputs
                    .iter()
                    .find(|pin| pin.id == source_pin)
                    .map(|pin| pin.data_type().clone())
                    .ok_or_else(|| {
                        anyhow!(
                            "Source pin '{}' not found on node '{}'",
                            source_pin,
                            source_node
                        )
                    })
            })?;
        let target_type = session
            .graph
            .nodes
            .get(&target_node)
            .ok_or_else(|| anyhow!("Target node not found: {}", target_node))
            .and_then(|node| {
                node.inputs
                    .iter()
                    .find(|pin| pin.id == target_pin)
                    .map(|pin| pin.data_type().clone())
                    .ok_or_else(|| {
                        anyhow!(
                            "Target pin '{}' not found on node '{}'",
                            target_pin,
                            target_node
                        )
                    })
            })?;

        let kind = parse_connection_type(connection_type, &source_type, &target_type)?;
        blueprint_profile()
            .can_connect(
                &session.graph,
                &graphy::PinRef {
                    node: source_node.clone(),
                    pin: source_pin.clone(),
                },
                &graphy::PinRef {
                    node: target_node.clone(),
                    pin: target_pin.clone(),
                },
            )
            .map_err(|error| anyhow!("Invalid connection: {error}"))?;

        if session.graph.connections.iter().any(|existing| {
            existing.source_node == source_node
                && existing.source_pin == source_pin
                && existing.target_node == target_node
                && existing.target_pin == target_pin
        }) {
            return Err(anyhow!("This connection already exists"));
        }

        let mut conn = Connection::new(&source_node, &source_pin, &target_node, &target_pin, kind);
        conn.id = Some(loop {
            let candidate = uuid::Uuid::new_v4().to_string();
            if !session
                .graph
                .connections
                .iter()
                .any(|existing| edge_id(existing) == candidate)
            {
                break candidate;
            }
        });

        // Match interactive pin cardinality: execution and reroute outputs
        // have one outgoing edge; data/execution inputs and reroute inputs
        // have one incoming edge.
        let source_is_reroute = is_reroute(&session.graph, &source_node);
        let target_is_reroute = is_reroute(&session.graph, &target_node);
        if source_type.is_execution() || source_is_reroute {
            session.graph.connections.retain(|existing| {
                !(existing.source_node == source_node && existing.source_pin == source_pin)
            });
        }
        if source_type.is_execution() || target_is_reroute || !target_type.is_execution() {
            session.graph.connections.retain(|existing| {
                !(existing.target_node == target_node && existing.target_pin == target_pin)
            });
        }

        session.graph.connections.push(conn.clone());
        if target_is_reroute {
            propagate_reroute_types(&mut session.graph, &target_node, source_type);
        } else if source_is_reroute {
            propagate_reroute_types(&mut session.graph, &source_node, target_type);
        }
        session.dirty = true;

        Ok(json!({ "ok": true, "connection_id": edge_id(&conn), "connection": connection_to_json(&conn) }))
    })
}

/// Remove a connection by id.
#[tool(category = "blueprint")]
pub fn blueprint_remove_connection(connection_id: String) -> Result<Value> {
    with_session_mut(|session| {
        let before = session.graph.connections.len();
        session
            .graph
            .connections
            .retain(|c| edge_id(c) != connection_id);
        let removed = before != session.graph.connections.len();
        if removed {
            session.dirty = true;
        }

        Ok(json!({
            "ok": true,
            "removed": removed,
            "connections": session.graph.connections.len(),
        }))
    })
}

/// Extract a partial subgraph view by node ids.
#[tool(category = "blueprint")]
pub fn blueprint_get_subgraph(
    node_ids: Vec<String>,
    include_comments: Option<bool>,
) -> Result<Value> {
    with_session(|session| {
        let node_set = node_ids.into_iter().collect::<HashSet<_>>();
        let scene = laid_out(&session.graph);

        let nodes = sorted_nodes(&session.graph)
            .into_iter()
            .filter(|n| node_set.contains(&n.id))
            .map(|n| node_to_json(n, &scene))
            .collect::<Vec<_>>();

        let connections = session
            .graph
            .connections
            .iter()
            .filter(|c| node_set.contains(&c.source_node) && node_set.contains(&c.target_node))
            .map(connection_to_json)
            .collect::<Vec<_>>();

        let comments = if include_comments.unwrap_or(true) {
            session
                .graph
                .comments
                .iter()
                .filter(|comment| {
                    comment
                        .contained_node_ids
                        .iter()
                        .any(|id| node_set.contains(id))
                })
                .map(comment_to_json)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        Ok(json!({
            "ok": true,
            "nodes": nodes,
            "connections": connections,
            "comments": comments,
        }))
    })
}

pub fn ai_tools() -> Vec<AiToolDefinition> {
    tool_registry()
        .definitions()
        .into_iter()
        .map(|definition| {
            let mut out = AiToolDefinition::new(
                definition.name,
                definition.description,
                definition.parameters_schema,
            );
            if let Some(category) = definition.category {
                out = out.with_category(category);
            }
            out
        })
        .collect()
}

pub fn capabilities_for_file(file_path: &Path) -> Vec<String> {
    if !is_blueprint_file(file_path) {
        return Vec::new();
    }

    tool_registry()
        .names()
        .into_iter()
        .map(|name| name.to_string())
        .collect()
}

pub fn execute_compiled_tool(
    file_path: &Path,
    tool_name: &str,
    tool_args: Value,
) -> Result<Value, PluginError> {
    let started_at = Instant::now();
    debug!(tool = tool_name, file = %file_path.display(), "blueprint execute_compiled_tool start");
    set_active_file(file_path);
    let _requested_file_scope = RequestedFileScope::enter(file_path);
    let ctx = ToolContext::new()
        .with_current_file(file_path)
        .with_workspace(file_path.parent().unwrap_or(file_path).to_path_buf());

    tool_registry()
        .execute(tool_name, tool_args, &ctx)
        .map_err(|error| PluginError::Other {
            message: format!("{}", error),
        })
        .map(|value| {
            debug!(tool = tool_name, file = %file_path.display(), elapsed_ms = started_at.elapsed().as_millis() as u64, "blueprint execute_compiled_tool end");
            value
        })
}

pub fn execute_ai_tool(
    file_path: &Path,
    tool_name: &str,
    tool_args: Value,
) -> Result<Value, PluginError> {
    execute_compiled_tool(file_path, tool_name, tool_args)
}
