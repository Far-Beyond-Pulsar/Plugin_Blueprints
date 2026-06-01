use anyhow::{anyhow, Context, Result};
use gpui::{Point, Size};
use plugin_editor_api::{AiToolDefinition, PluginError};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tool_registry::{PluginToolRegistry, ToolContext, ToolRegistry};
use tool_registry_macros::tool;
use tracing::debug;
use ui::graph::ConnectionType;

use crate::core::definitions::NodeDefinitions;
use crate::core::graph::BlueprintGraph;
use crate::core::types::{BlueprintComment, BlueprintNode, Connection};

#[derive(Clone)]
struct BlueprintAiSession {
    file_key: PathBuf,
    graph: BlueprintGraph,
    dirty: bool,
}

#[derive(Default)]
struct RuntimeState {
    active_file: Option<PathBuf>,
    sessions: HashMap<PathBuf, BlueprintAiSession>,
}

static STATE: OnceLock<Mutex<RuntimeState>> = OnceLock::new();

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

pub fn upsert_session(file_path: PathBuf, graph: BlueprintGraph) {
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

    if let Some(active) = guard.active_file.as_ref() {
        if guard.sessions.contains_key(active) {
            return Some(active.clone());
        }
    }

    if guard.sessions.len() == 1 {
        return guard.sessions.keys().next().cloned();
    }

    None
}

fn active_file() -> Result<PathBuf> {
    state()
        .lock()
        .map_err(|_| anyhow!("AI session state lock poisoned"))?
        .active_file
        .clone()
        .ok_or_else(|| anyhow!("No active blueprint file in AI session"))
}

fn with_session<R>(f: impl FnOnce(&BlueprintAiSession) -> Result<R>) -> Result<R> {
    let guard = state()
        .lock()
        .map_err(|_| anyhow!("AI session state lock poisoned"))?;
    let requested = guard
        .active_file
        .clone()
        .ok_or_else(|| anyhow!("No active blueprint file in AI session"))?;
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
    let requested = guard
        .active_file
        .clone()
        .ok_or_else(|| anyhow!("No active blueprint file in AI session"))?;
    let key = resolve_session_key(&guard, &requested).ok_or_else(|| {
        anyhow!(
            "Blueprint is not open in editor: {}. Call open_file_in_default_editor first.",
            requested.display()
        )
    })?;
    let session = guard
        .sessions
        .get_mut(&key)
        .ok_or_else(|| anyhow!("Blueprint is not open in editor: {}", key.display()))?;
    f(session)
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

fn node_to_json(node: &BlueprintNode) -> Value {
    json!({
        "id": node.id,
        "definition_id": node.definition_id,
        "title": node.title,
        "node_type": format!("{:?}", node.node_type),
        "position": { "x": node.position.x, "y": node.position.y },
        "size": { "width": node.size.width, "height": node.size.height },
        "inputs": node.inputs.iter().map(|pin| json!({
            "id": pin.id,
            "name": pin.name,
            "pin_type": format!("{:?}", pin.pin_type),
            "data_type": pin.data_type.to_string(),
        })).collect::<Vec<_>>(),
        "outputs": node.outputs.iter().map(|pin| json!({
            "id": pin.id,
            "name": pin.name,
            "pin_type": format!("{:?}", pin.pin_type),
            "data_type": pin.data_type.to_string(),
        })).collect::<Vec<_>>(),
        "properties": node.properties,
        "description": node.description,
    })
}

fn comment_to_json(comment: &BlueprintComment) -> Value {
    json!({
        "id": comment.id,
        "text": comment.text,
        "position": { "x": comment.position.x, "y": comment.position.y },
        "size": { "width": comment.size.width, "height": comment.size.height },
        "contained_node_ids": comment.contained_node_ids,
    })
}

fn connection_to_json(connection: &Connection) -> Value {
    json!({
        "id": connection.id,
        "source_node": connection.source_node,
        "source_pin": connection.source_pin,
        "target_node": connection.target_node,
        "target_pin": connection.target_pin,
        "connection_type": format!("{:?}", connection.connection_type),
    })
}

fn update_comment_containment(graph: &mut BlueprintGraph) {
    let nodes = graph.nodes.clone();
    for comment in &mut graph.comments {
        comment.update_contained_nodes(&nodes);
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

fn parse_connection_type(kind: Option<String>) -> ConnectionType {
    match kind
        .unwrap_or_else(|| "data".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "execution" | "exec" => ConnectionType::Execution,
        _ => ConnectionType::Data,
    }
}

fn tool_registry() -> &'static ToolRegistry {
    static REGISTRY: OnceLock<ToolRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = ToolRegistry::new();
        registry.merge_plugin(&PluginToolRegistry::from_namespace(module_path!()));
        registry
    })
}

/// Summarize graph-wide structure, counts, and selection state.
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
                "selected_nodes": graph.selected_nodes.len(),
                "selected_comments": graph.selected_comments.len(),
            },
            "viewport": {
                "zoom_level": graph.zoom_level,
                "pan_offset": { "x": graph.pan_offset.x, "y": graph.pan_offset.y },
            }
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

        let matched = session
            .graph
            .nodes
            .iter()
            .filter(|node| {
                if let Some(def) = definition_id.as_ref() {
                    if &node.definition_id != def {
                        return false;
                    }
                }

                if let Some(q) = query_norm.as_ref() {
                    let haystack = format!(
                        "{} {} {} {}",
                        node.id,
                        node.title,
                        node.definition_id,
                        node.properties
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
            .map(|node| node_to_json(node))
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
        let node = session.graph.nodes.iter().find(|n| n.id == id);
        Ok(json!({
            "ok": true,
            "found": node.is_some(),
            "node": node.map(node_to_json),
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
    with_session_mut(|session| {
        let definitions = NodeDefinitions::load();
        let definition = definitions
            .get_node_definition(&definition_id)
            .ok_or_else(|| anyhow!("Unknown node definition_id: {}", definition_id))?;

        let mut node = BlueprintNode::from_definition(definition, Point::new(x as f32, y as f32));

        if let Some(title) = title {
            if !title.trim().is_empty() {
                node.title = title;
            }
        }

        let properties_map = parse_properties(properties)?;
        for (key, value) in properties_map {
            node.properties.insert(key, value);
        }

        let node_id = node.id.clone();
        session.graph.nodes.push(node.clone());
        session.graph.selected_nodes.clear();
        session.graph.selected_nodes.push(node_id.clone());
        update_comment_containment(&mut session.graph);
        session.dirty = true;

        Ok(json!({
            "ok": true,
            "node_id": node_id,
            "node": node_to_json(&node),
            "counts": {
                "nodes": session.graph.nodes.len(),
                "connections": session.graph.connections.len(),
                "comments": session.graph.comments.len(),
            }
        }))
    })
}

/// Update node title/position/properties.
#[tool(category = "blueprint")]
pub fn blueprint_update_node(
    id: String,
    title: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    properties_merge: Option<Value>,
    replace_properties: Option<bool>,
) -> Result<Value> {
    with_session_mut(|session| {
        let node_json = {
            let node = session
                .graph
                .nodes
                .iter_mut()
                .find(|n| n.id == id)
                .ok_or_else(|| anyhow!("Node not found: {}", id))?;

            if let Some(title) = title {
                node.title = title;
            }
            if let Some(x) = x {
                node.position.x = x as f32;
            }
            if let Some(y) = y {
                node.position.y = y as f32;
            }

            if let Some(props) = properties_merge {
                let parsed = parse_properties(Some(props))?;
                if replace_properties.unwrap_or(false) {
                    node.properties = parsed;
                } else {
                    for (k, v) in parsed {
                        node.properties.insert(k, v);
                    }
                }
            }

            node_to_json(node)
        };

        update_comment_containment(&mut session.graph);
        session.dirty = true;

        Ok(json!({
            "ok": true,
            "node": node_json,
        }))
    })
}

/// Remove a node and all attached connections.
#[tool(category = "blueprint")]
pub fn blueprint_remove_node(id: String) -> Result<Value> {
    with_session_mut(|session| {
        let before_nodes = session.graph.nodes.len();
        let before_connections = session.graph.connections.len();

        session.graph.nodes.retain(|n| n.id != id);
        session
            .graph
            .connections
            .retain(|c| c.source_node != id && c.target_node != id);
        session.graph.selected_nodes.retain(|sid| sid != &id);
        update_comment_containment(&mut session.graph);

        let removed = before_nodes != session.graph.nodes.len();
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

        let items = session
            .graph
            .nodes
            .iter()
            .filter(|node| comment.contains_node(node))
            .map(node_to_json)
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
        let comment = BlueprintComment {
            id: uuid::Uuid::new_v4().to_string(),
            text,
            position: Point::new(x as f32, y as f32),
            size: Size::new(
                width.unwrap_or(300.0) as f32,
                height.unwrap_or(200.0) as f32,
            ),
            color: gpui::hsla(0.5, 0.3, 0.2, 0.3),
            contained_node_ids: Vec::new(),
            is_selected: false,
            color_picker_state: None,
        };

        let id = comment.id.clone();
        session.graph.comments.push(comment.clone());
        update_comment_containment(&mut session.graph);
        session.graph.selected_comments.clear();
        session.graph.selected_comments.push(id.clone());
        session.dirty = true;

        Ok(json!({ "ok": true, "comment_id": id, "comment": comment_to_json(&comment) }))
    })
}

/// Remove a comment box.
#[tool(category = "blueprint")]
pub fn blueprint_remove_comment(comment_id: String) -> Result<Value> {
    with_session_mut(|session| {
        let before = session.graph.comments.len();
        session.graph.comments.retain(|c| c.id != comment_id);
        session
            .graph
            .selected_comments
            .retain(|id| id != &comment_id);
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
        let source = session
            .graph
            .nodes
            .iter()
            .find(|n| n.id == source_node)
            .ok_or_else(|| anyhow!("Source node not found: {}", source_node))?;
        let target = session
            .graph
            .nodes
            .iter()
            .find(|n| n.id == target_node)
            .ok_or_else(|| anyhow!("Target node not found: {}", target_node))?;

        if !source.outputs.iter().any(|pin| pin.id == source_pin) {
            return Err(anyhow!(
                "Source pin '{}' not found on node '{}'",
                source_pin,
                source_node
            ));
        }
        if !target.inputs.iter().any(|pin| pin.id == target_pin) {
            return Err(anyhow!(
                "Target pin '{}' not found on node '{}'",
                target_pin,
                target_node
            ));
        }

        let conn = Connection {
            id: uuid::Uuid::new_v4().to_string(),
            source_node,
            source_pin,
            target_node,
            target_pin,
            connection_type: parse_connection_type(connection_type),
        };

        let conn_id = conn.id.clone();
        session.graph.connections.push(conn.clone());
        session.dirty = true;

        Ok(json!({ "ok": true, "connection_id": conn_id, "connection": connection_to_json(&conn) }))
    })
}

/// Remove a connection by id.
#[tool(category = "blueprint")]
pub fn blueprint_remove_connection(connection_id: String) -> Result<Value> {
    with_session_mut(|session| {
        let before = session.graph.connections.len();
        session.graph.connections.retain(|c| c.id != connection_id);
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
        let node_set = node_ids
            .into_iter()
            .collect::<std::collections::HashSet<_>>();

        let nodes = session
            .graph
            .nodes
            .iter()
            .filter(|n| node_set.contains(&n.id))
            .map(node_to_json)
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
                    session
                        .graph
                        .nodes
                        .iter()
                        .any(|node| node_set.contains(&node.id) && comment.contains_node(node))
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
