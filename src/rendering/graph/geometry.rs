//! Pure geometry and display helpers used by the node graph renderer.

use crate::core::types::PinDataType as DataType;
use crate::core::types::{BlueprintNode, Connection, NodeType};
use crate::rendering::gpu::WireVertex;

use super::{BODY_PAD, HEADER_H, PIN_GAP, PIN_ROW_H, SEP_H};

const WIRE_SEGS: usize = 32;

pub(super) fn pin_canvas_pos(
    node: &BlueprintNode,
    is_input: bool,
    row: usize,
    pin_id: Option<&str>,
    graph: &crate::core::graph::BlueprintGraph,
) -> gpui::Point<f32> {
    if node.node_type == NodeType::Conversion {
        return graph_to_screen_pos(
            gpui::Point::new(
                node.position.x + if is_input { 0.0 } else { node.size.width },
                node.position.y + node.size.height * 0.5,
            ),
            graph,
        );
    }
    if node.node_type == NodeType::Reroute {
        let cx = node.position.x + node.size.width * 0.5;
        let cy = node.position.y + node.size.height * 0.5;
        return graph_to_screen_pos(gpui::Point::new(cx, cy), graph);
    }
    if pin_id == Some("__return__") {
        let scr = graph_to_screen_pos(node.position, graph);
        let px = scr.x + (node.size.width - 24.0) * graph.zoom_level;
        let py = scr.y + HEADER_H * 0.5 * graph.zoom_level;
        return gpui::Point::new(px, py);
    }
    let zoom = graph.zoom_level;
    let scr = graph_to_screen_pos(node.position, graph);
    let py = scr.y
        + (HEADER_H + SEP_H + BODY_PAD) * zoom
        + row as f32 * (PIN_ROW_H + PIN_GAP) * zoom
        + PIN_ROW_H * 0.5 * zoom;
    let px = if is_input {
        scr.x + BODY_PAD * zoom
    } else {
        scr.x + (node.size.width - BODY_PAD) * zoom
    };
    gpui::Point::new(px, py)
}

pub(super) fn calculate_pin_position(
    node: &BlueprintNode,
    pin_id: &str,
    is_input: bool,
    graph: &crate::core::graph::BlueprintGraph,
) -> Option<gpui::Point<f32>> {
    if node.node_type == NodeType::Conversion {
        return Some(graph_to_screen_pos(
            gpui::Point::new(
                node.position.x + if is_input { 0.0 } else { node.size.width },
                node.position.y + node.size.height * 0.5,
            ),
            graph,
        ));
    }
    if node.node_type == NodeType::Reroute {
        let cx = node.position.x + node.size.width * 0.5;
        let cy = node.position.y + node.size.height * 0.5;
        return Some(graph_to_screen_pos(gpui::Point::new(cx, cy), graph));
    }
    let row = if is_input {
        node.inputs.iter().position(|pin| pin.id == pin_id)?
    } else {
        node.outputs.iter().position(|pin| pin.id == pin_id)?
    };
    Some(pin_canvas_pos(node, is_input, row, Some(pin_id), graph))
}

pub(super) fn calculate_pin_position_graph_space(
    node: &BlueprintNode,
    is_input: bool,
    row: usize,
    _graph: &crate::core::graph::BlueprintGraph,
) -> gpui::Point<f32> {
    if node.node_type == NodeType::Conversion {
        return gpui::Point::new(
            node.position.x + if is_input { 0.0 } else { node.size.width },
            node.position.y + node.size.height * 0.5,
        );
    }
    let py = node.position.y
        + HEADER_H
        + SEP_H
        + BODY_PAD
        + row as f32 * (PIN_ROW_H + PIN_GAP)
        + PIN_ROW_H * 0.5;
    let px = if is_input {
        node.position.x + BODY_PAD
    } else {
        node.position.x + node.size.width - BODY_PAD
    };
    gpui::Point::new(px, py)
}

pub(super) fn is_node_visible_simple(
    node: &BlueprintNode,
    graph: &crate::core::graph::BlueprintGraph,
) -> bool {
    let pad = 260.0 / graph.zoom_level.max(0.05);
    let vl = -graph.pan_offset.x - pad;
    let vt = -graph.pan_offset.y - pad;
    let vr = -graph.pan_offset.x + 3840.0 / graph.zoom_level + pad;
    let vb = -graph.pan_offset.y + 2160.0 / graph.zoom_level + pad;
    !(node.position.x > vr
        || node.position.x + node.size.width < vl
        || node.position.y > vb
        || node.position.y + node.size.height < vt)
}

pub(super) fn is_connection_visible_simple(
    conn: &Connection,
    graph: &crate::core::graph::BlueprintGraph,
) -> bool {
    let from = graph.nodes.iter().find(|node| node.id == conn.source_node);
    let to = graph.nodes.iter().find(|node| node.id == conn.target_node);
    match (from, to) {
        (Some(from), Some(to)) => {
            is_node_visible_simple(from, graph) || is_node_visible_simple(to, graph)
        }
        _ => false,
    }
}

pub(super) fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim_start_matches('#');
    let parse = |s: &str| {
        u8::from_str_radix(s, 16)
            .ok()
            .map(|value| value as f32 / 255.0)
    };
    if hex.len() == 6 {
        Some(gpui::Hsla::from(gpui::Rgba {
            r: parse(&hex[0..2])?,
            g: parse(&hex[2..4])?,
            b: parse(&hex[4..6])?,
            a: 1.0,
        }))
    } else if hex.len() == 8 {
        Some(gpui::Hsla::from(gpui::Rgba {
            r: parse(&hex[0..2])?,
            g: parse(&hex[2..4])?,
            b: parse(&hex[4..6])?,
            a: parse(&hex[6..8])?,
        }))
    } else {
        None
    }
}

pub(super) fn category_color(node: &BlueprintNode) -> [f32; 4] {
    if let Some(ref hex) = node.color {
        let hex = hex.trim_start_matches('#');
        let parse = |s: &str| {
            u8::from_str_radix(s, 16)
                .ok()
                .map(|value| value as f32 / 255.0)
        };
        if hex.len() == 6 {
            if let (Some(r), Some(g), Some(b)) =
                (parse(&hex[0..2]), parse(&hex[2..4]), parse(&hex[4..6]))
            {
                return [r, g, b, 1.0];
            }
        }
    }
    match node.node_type {
        NodeType::Event => [0.72, 0.12, 0.10, 1.0],
        NodeType::Logic => [0.13, 0.38, 0.78, 1.0],
        NodeType::Math => [0.16, 0.62, 0.28, 1.0],
        NodeType::Object => [0.78, 0.42, 0.08, 1.0],
        NodeType::Reroute => [0.40, 0.40, 0.42, 1.0],
        NodeType::Conversion => [0.24, 0.47, 0.65, 1.0],
        NodeType::MacroEntry | NodeType::MacroExit => [0.44, 0.18, 0.72, 1.0],
        NodeType::SubGraphCall => [0.32, 0.12, 0.52, 1.0],
        NodeType::CustomEvent => [0.90, 0.50, 0.10, 1.0],
        NodeType::CustomEventDispatch => [0.10, 0.60, 0.85, 1.0],
    }
}

pub(super) fn pin_color(data_type: &DataType) -> [f32; 4] {
    data_type.display_color()
}

pub(super) fn wire_phase(conn: &Connection) -> f32 {
    let mut hash: u32 = 2166136261;
    for byte in conn
        .source_node
        .bytes()
        .chain(conn.source_pin.bytes())
        .chain(conn.target_node.bytes())
        .chain(conn.target_pin.bytes())
    {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    (hash as f32 / u32::MAX as f32) * 2.0
}

pub(super) fn pin_gpos_row(node: &BlueprintNode, is_input: bool, row: usize) -> (f32, f32) {
    if node.node_type == NodeType::Conversion {
        return (
            node.position.x + if is_input { 0.0 } else { node.size.width },
            node.position.y + node.size.height * 0.5,
        );
    }
    if node.node_type == NodeType::Reroute {
        let cx = node.position.x + node.size.width * 0.5;
        let cy = node.position.y + node.size.height * 0.5;
        return (cx, cy);
    }
    let py = node.position.y
        + HEADER_H
        + SEP_H
        + BODY_PAD
        + row as f32 * (PIN_ROW_H + PIN_GAP)
        + PIN_ROW_H * 0.5;
    let px = if is_input {
        node.position.x + BODY_PAD
    } else {
        node.position.x + node.size.width - BODY_PAD
    };
    (px, py)
}

pub(super) fn pin_gpos_id(
    node: &BlueprintNode,
    pin_id: &str,
    is_input: bool,
) -> Option<(f32, f32)> {
    if node.node_type == NodeType::Conversion {
        return Some((
            node.position.x + if is_input { 0.0 } else { node.size.width },
            node.position.y + node.size.height * 0.5,
        ));
    }
    if node.node_type == NodeType::Reroute {
        let cx = node.position.x + node.size.width * 0.5;
        let cy = node.position.y + node.size.height * 0.5;
        return Some((cx, cy));
    }
    if pin_id == "__return__" {
        return Some((
            node.position.x + node.size.width - 24.0,
            node.position.y + HEADER_H * 0.5,
        ));
    }
    let row = if is_input {
        node.inputs.iter().position(|pin| pin.id == pin_id)?
    } else {
        node.outputs.iter().position(|pin| pin.id == pin_id)?
    };
    Some(pin_gpos_row(node, is_input, row))
}

pub(crate) fn bezier(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    t: f32,
) -> (f32, f32) {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    (
        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
    )
}

pub(super) fn tessellate_wire(
    from: (f32, f32),
    to: (f32, f32),
    color: [f32; 4],
    half_thick: f32,
) -> Vec<WireVertex> {
    let hd = (to.0 - from.0).abs();
    let ctl = (hd * 0.45).max(55.0).min(220.0);
    let c1 = (from.0 + ctl, from.1);
    let c2 = (to.0 - ctl, to.1);
    let mut out = Vec::with_capacity(WIRE_SEGS * 6);
    let mut prev = from;
    for i in 1..=WIRE_SEGS {
        let t = i as f32 / WIRE_SEGS as f32;
        let cur = bezier(from, c1, c2, to, t);
        let dx = cur.0 - prev.0;
        let dy = cur.1 - prev.1;
        let len = (dx * dx + dy * dy).sqrt();
        let (nx, ny) = if len > 0.0 {
            (-dy / len * half_thick, dx / len * half_thick)
        } else {
            (0.0, half_thick)
        };
        let v0 = (i - 1) as f32 / WIRE_SEGS as f32;
        let v1 = i as f32 / WIRE_SEGS as f32;
        out.extend([
            WireVertex {
                pos: [prev.0 + nx, prev.1 + ny],
                uv: [0.0, v0],
                color,
            },
            WireVertex {
                pos: [prev.0 - nx, prev.1 - ny],
                uv: [1.0, v0],
                color,
            },
            WireVertex {
                pos: [cur.0 + nx, cur.1 + ny],
                uv: [0.0, v1],
                color,
            },
            WireVertex {
                pos: [cur.0 + nx, cur.1 + ny],
                uv: [0.0, v1],
                color,
            },
            WireVertex {
                pos: [prev.0 - nx, prev.1 - ny],
                uv: [1.0, v0],
                color,
            },
            WireVertex {
                pos: [cur.0 - nx, cur.1 - ny],
                uv: [1.0, v1],
                color,
            },
        ]);
        prev = cur;
    }
    out
}

pub(super) fn tessellate_line(
    from: (f32, f32),
    to: (f32, f32),
    color: [f32; 4],
    half_thick: f32,
) -> Vec<WireVertex> {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.0001 {
        return vec![];
    }
    let (nx, ny) = (-dy / len * half_thick, dx / len * half_thick);
    vec![
        WireVertex {
            pos: [from.0 + nx, from.1 + ny],
            uv: [0.0, 0.0],
            color,
        },
        WireVertex {
            pos: [from.0 - nx, from.1 - ny],
            uv: [1.0, 0.0],
            color,
        },
        WireVertex {
            pos: [to.0 + nx, to.1 + ny],
            uv: [0.0, 1.0],
            color,
        },
        WireVertex {
            pos: [to.0 + nx, to.1 + ny],
            uv: [0.0, 1.0],
            color,
        },
        WireVertex {
            pos: [from.0 - nx, from.1 - ny],
            uv: [1.0, 0.0],
            color,
        },
        WireVertex {
            pos: [to.0 - nx, to.1 - ny],
            uv: [1.0, 1.0],
            color,
        },
    ]
}

pub(super) fn cached_text_width(
    renderer: &mut crate::rendering::gpu::BpRenderer,
    cache: &mut std::collections::HashMap<(String, u32), f32>,
    text: &str,
    size: f32,
) -> f32 {
    let key = (text.to_owned(), size.to_bits());
    if let Some(width) = cache.get(&key) {
        return *width;
    }
    let width = renderer.measure_text_width(text, size);
    cache.insert(key, width);
    width
}

fn graph_to_screen_pos(
    point: gpui::Point<f32>,
    graph: &crate::core::graph::BlueprintGraph,
) -> gpui::Point<f32> {
    gpui::Point::new(
        (point.x + graph.pan_offset.x) * graph.zoom_level,
        (point.y + graph.pan_offset.y) * graph.zoom_level,
    )
}
