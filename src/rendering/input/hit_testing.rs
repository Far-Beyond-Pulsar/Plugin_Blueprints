// Coordinate conversion and graph hit testing shared by graph input handlers.

use crate::core::types::{BlueprintNode, NodeType};
use crate::core::types::PinDataType as DataType;
use crate::editor::panel::ResizeHandle;
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::rendering::graph::{NodeGraphRenderer, PIN_SIZE};
use gpui::{CursorStyle, *};
use ui::PixelsExt;
// ─── coordinate conversion ────────────────────────────────────────────────────

pub(super) fn to_canvas(window_pos: Point<Pixels>, canvas: &GraphCanvasPanel) -> Point<f32> {
    let o = *canvas.canvas_origin.borrow();
    Point::new(window_pos.x.as_f32() - o.x, window_pos.y.as_f32() - o.y)
}

pub(super) fn to_graph(cp: Point<f32>, canvas: &GraphCanvasPanel) -> Point<f32> {
    let z = canvas.graph.zoom_level;
    Point::new(
        cp.x / z - canvas.graph.pan_offset.x,
        cp.y / z - canvas.graph.pan_offset.y,
    )
}

#[inline]
fn within_radius(a: Point<f32>, b: Point<f32>, radius: f32) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy <= radius * radius
}

fn pin_hit_radius(canvas: &GraphCanvasPanel, scale: f32, min_distance: f32) -> f32 {
    (PIN_SIZE * canvas.graph.zoom_level * scale).max(min_distance)
}

// ─── hit testing ─────────────────────────────────────────────────────────────

fn hit_node_ref<'a>(gp: Point<f32>, canvas: &'a GraphCanvasPanel) -> Option<&'a BlueprintNode> {
    for node in canvas.graph.nodes.iter().rev() {
        let nl = node.position.x;
        let nt = node.position.y;
        let nr = nl + node.size.width;
        let nb = nt + node.size.height;
        if gp.x >= nl && gp.x <= nr && gp.y >= nt && gp.y <= nb {
            return Some(node);
        }
    }
    None
}

pub(super) fn hit_node<'a>(gp: Point<f32>, canvas: &'a GraphCanvasPanel) -> Option<&'a str> {
    hit_node_ref(gp, canvas).map(|node| node.id.as_str())
}

fn output_pin_on_node(
    cp: Point<f32>,
    canvas: &GraphCanvasPanel,
    node: &BlueprintNode,
) -> Option<(String, String)> {
    let r = if node.node_type == NodeType::Reroute {
        (node.size.width.max(node.size.height) * 0.5 * canvas.graph.zoom_level) * 0.4
    } else {
        pin_hit_radius(canvas, 0.9, 6.0)
    };
    node.outputs.iter().enumerate().find_map(|(i, pin)| {
        let center = NodeGraphRenderer::pin_canvas_pos_index(
            node,
            false,
            i,
            pin.id.as_str(),
            &canvas.graph,
        );
        within_radius(cp, center, r).then(|| (node.id.clone(), pin.id.clone()))
    })
}

fn input_pin_on_node(
    cp: Point<f32>,
    canvas: &GraphCanvasPanel,
    node: &BlueprintNode,
    skip_node: Option<&str>,
    src_type: Option<&DataType>,
) -> Option<(String, String)> {
    if skip_node == Some(node.id.as_str()) {
        return None;
    }
    let r = if node.node_type == NodeType::Reroute {
        (node.size.width.max(node.size.height) * 0.5 * canvas.graph.zoom_level) * 0.4
    } else {
        pin_hit_radius(canvas, if src_type.is_some() { 1.3 } else { 1.2 }, 8.0)
    };
    node.inputs.iter().enumerate().find_map(|(i, pin)| {
        if src_type.is_some_and(|source| !source.is_compatible_with(&pin.data_type)) {
            return None;
        }
        let center = NodeGraphRenderer::pin_canvas_pos_index(
            node,
            true,
            i,
            pin.id.as_str(),
            &canvas.graph,
        );
        within_radius(cp, center, r).then(|| (node.id.clone(), pin.id.clone()))
    })
}

fn any_pin_on_node(
    cp: Point<f32>,
    canvas: &GraphCanvasPanel,
    node: &BlueprintNode,
) -> Option<(String, String)> {
    let r = if node.node_type == NodeType::Reroute {
        (node.size.width.max(node.size.height) * 0.5 * canvas.graph.zoom_level) * 0.4
    } else {
        pin_hit_radius(canvas, 1.2, 8.0)
    };
    for (is_input, pins) in [(true, &node.inputs), (false, &node.outputs)] {
        if let Some((_, pin)) = pins.iter().enumerate().find(|(i, pin)| {
            let center = NodeGraphRenderer::pin_canvas_pos_index(
                node,
                is_input,
                *i,
                pin.id.as_str(),
                &canvas.graph,
            );
            within_radius(cp, center, r)
        }) {
            return Some((node.id.clone(), pin.id.clone()));
        }
    }
    None
}

pub(super) fn hit_output_pin(
    cp: Point<f32>,
    canvas: &GraphCanvasPanel,
) -> Option<(String, String)> {
    if let Some(node) = hit_node_ref(to_graph(cp, canvas), canvas) {
        return output_pin_on_node(cp, canvas, node);
    }
    canvas
        .graph
        .nodes
        .iter()
        .rev()
        .find_map(|node| output_pin_on_node(cp, canvas, node))
}

pub(super) fn hit_input_pin(
    cp: Point<f32>,
    canvas: &GraphCanvasPanel,
    skip_node: &str,
    src_type: &DataType,
) -> Option<(String, String)> {
    if let Some(node) = hit_node_ref(to_graph(cp, canvas), canvas) {
        return input_pin_on_node(cp, canvas, node, Some(skip_node), Some(src_type));
    }
    canvas.graph.nodes.iter().rev().find_map(|node| {
        input_pin_on_node(cp, canvas, node, Some(skip_node), Some(src_type))
    })
}

pub(super) fn hit_any_pin(cp: Point<f32>, canvas: &GraphCanvasPanel) -> Option<(String, String)> {
    if let Some(node) = hit_node_ref(to_graph(cp, canvas), canvas) {
        return any_pin_on_node(cp, canvas, node);
    }
    canvas
        .graph
        .nodes
        .iter()
        .rev()
        .find_map(|node| any_pin_on_node(cp, canvas, node))
}

pub(super) fn hit_comment<'a>(gp: Point<f32>, canvas: &'a GraphCanvasPanel) -> Option<&'a str> {
    for comment in canvas.graph.comments.iter().rev() {
        let left = comment.position.x;
        let top = comment.position.y;
        let right = left + comment.size.width;
        let bottom = top + comment.size.height;
        if gp.x >= left && gp.x <= right && gp.y >= top && gp.y <= bottom {
            return Some(&comment.id);
        }
    }
    None
}

pub(super) fn hit_comment_header<'a>(
    gp: Point<f32>,
    canvas: &'a GraphCanvasPanel,
) -> Option<&'a str> {
    let header_h = (30.0 / canvas.graph.zoom_level.max(0.25)).clamp(18.0, 44.0);
    for comment in canvas.graph.comments.iter().rev() {
        let left = comment.position.x;
        let top = comment.position.y;
        let right = left + comment.size.width;
        let bottom = top + header_h;
        if gp.x >= left && gp.x <= right && gp.y >= top && gp.y <= bottom {
            return Some(&comment.id);
        }
    }
    None
}

pub(super) fn hit_comment_title<'a>(
    gp: Point<f32>,
    canvas: &'a GraphCanvasPanel,
) -> Option<&'a str> {
    let header_h = (30.0 / canvas.graph.zoom_level.max(0.25)).clamp(18.0, 44.0);
    let pad_x = 12.0;
    let title_top = 2.0;
    let title_bottom = header_h - 3.0;
    for comment in canvas.graph.comments.iter().rev() {
        let left = comment.position.x + pad_x;
        let top = comment.position.y + title_top;
        let right = comment.position.x + comment.size.width - pad_x;
        let bottom = comment.position.y + title_bottom;
        if gp.x >= left && gp.x <= right && gp.y >= top && gp.y <= bottom {
            return Some(&comment.id);
        }
    }
    None
}

#[inline]
fn comment_resize_edge(canvas: &GraphCanvasPanel) -> f32 {
    // Keep edges reachable without swallowing title double-clicks.
    (6.0 / canvas.graph.zoom_level.max(0.25)).clamp(3.0, 12.0)
}

fn hit_comment_resize(
    gp: Point<f32>,
    canvas: &GraphCanvasPanel,
    comment_id: &str,
) -> Option<ResizeHandle> {
    let comment = canvas.graph.comments.iter().find(|c| c.id == comment_id)?;
    let left = comment.position.x;
    let top = comment.position.y;
    let right = left + comment.size.width;
    let bottom = top + comment.size.height;
    let edge = comment_resize_edge(canvas);
    let near_left = (gp.x - left).abs() <= edge;
    let near_right = (gp.x - right).abs() <= edge;
    let near_top = (gp.y - top).abs() <= edge;
    let near_bottom = (gp.y - bottom).abs() <= edge;

    match (near_left, near_right, near_top, near_bottom) {
        (true, _, true, _) => Some(ResizeHandle::TopLeft),
        (_, true, true, _) => Some(ResizeHandle::TopRight),
        (true, _, _, true) => Some(ResizeHandle::BottomLeft),
        (_, true, _, true) => Some(ResizeHandle::BottomRight),
        (_, _, true, _) => Some(ResizeHandle::Top),
        (_, _, _, true) => Some(ResizeHandle::Bottom),
        (true, _, _, _) => Some(ResizeHandle::Left),
        (_, true, _, _) => Some(ResizeHandle::Right),
        _ => None,
    }
}

pub(super) fn hit_any_comment_resize(
    gp: Point<f32>,
    canvas: &GraphCanvasPanel,
) -> Option<(String, ResizeHandle)> {
    let edge = comment_resize_edge(canvas);
    for comment in canvas.graph.comments.iter().rev() {
        let left = comment.position.x - edge;
        let top = comment.position.y - edge;
        let right = comment.position.x + comment.size.width + edge;
        let bottom = comment.position.y + comment.size.height + edge;
        if gp.x < left || gp.x > right || gp.y < top || gp.y > bottom {
            continue;
        }
        if let Some(handle) = hit_comment_resize(gp, canvas, &comment.id) {
            return Some((comment.id.clone(), handle));
        }
    }
    None
}

fn cursor_for_resize_handle(handle: &ResizeHandle) -> CursorStyle {
    match handle {
        ResizeHandle::TopLeft | ResizeHandle::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeHandle::TopRight | ResizeHandle::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
        ResizeHandle::Top | ResizeHandle::Bottom => CursorStyle::ResizeUpDown,
        ResizeHandle::Left | ResizeHandle::Right => CursorStyle::ResizeLeftRight,
    }
}

pub(super) fn update_graph_cursor(
    window: &mut Window,
    canvas: &GraphCanvasPanel,
    cp: Point<f32>,
    gp: Point<f32>,
) {
    let cursor = if let Some((_, handle)) = &canvas.resizing_comment {
        cursor_for_resize_handle(handle)
    } else if canvas.dragging_comment.is_some()
        || canvas.dragging_node.is_some()
        || canvas.is_panning()
    {
        CursorStyle::ClosedHand
    } else if canvas.dragging_connection.is_some() {
        CursorStyle::DragLink
    } else if let Some((_, handle)) = hit_any_comment_resize(gp, canvas) {
        cursor_for_resize_handle(&handle)
    } else if hit_any_pin(cp, canvas).is_some() {
        CursorStyle::PointingHand
    } else if hit_node(gp, canvas).is_some() {
        CursorStyle::OpenHand
    } else if hit_comment_header(gp, canvas).is_some() {
        CursorStyle::OpenHand
    } else if let Some(comment_id) = hit_comment(gp, canvas) {
        if let Some(handle) = hit_comment_resize(gp, canvas, comment_id) {
            cursor_for_resize_handle(&handle)
        } else {
            CursorStyle::Arrow
        }
    } else if canvas.is_selecting() {
        CursorStyle::Crosshair
    } else {
        CursorStyle::Arrow
    };

    window.set_window_cursor_style(cursor);
}

pub fn refresh_graph_cursor(window: &mut Window, canvas: &GraphCanvasPanel) {
    let cp = to_canvas(window.mouse_position(), canvas);
    let gp = to_graph(cp, canvas);
    update_graph_cursor(window, canvas, cp, gp);
}
