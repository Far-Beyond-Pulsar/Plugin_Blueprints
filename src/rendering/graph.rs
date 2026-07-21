// NodeGraphRenderer — mounts the WGPU surface and drives the four-pipeline GPU
// renderer every frame.
//
// CPU responsibilities (minimal by design):
//   1. Viewport culling — skip off-screen nodes/connections.
//   2. Build instance arrays for nodes, pins, wires (CPU bezier tessellation).
//   3. Queue text labels into the GPU text renderer.
//   4. Upload everything and fire a single render_frame() call.
//   5. Coordinate utility functions used by input.rs and the features layer.
//
// GPU does all actual drawing: grid, node bodies, wires, pins, text glyphs.
// No GPUI canvas overlay is used for graph content — including text.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::*;
use crate::core::types::PinDataType as DataType;
use ui::ActiveTheme;
use ui::PixelsExt;

use crate::core::graph::BlueprintGraph;
use crate::core::types::{BlueprintComment, BlueprintNode, Connection, NodeType, Pin};
use crate::editor::workspace_panels::GraphCanvasPanel;
use crate::features::connections::operations::ConnectionDrag;
use crate::rendering::gpu::{ GraphUniforms, NodeInstance, PinInstance, WireInstance, WireVertex,
};
use crate::rendering::layout;

// shared with hit-testing in input.rs
pub const HEADER_H: f32 = layout::HEADER_H;
pub const SEP_H: f32 = layout::SEP_H;
pub const BODY_PAD: f32 = layout::BODY_PAD;
pub const PIN_ROW_H: f32 = layout::PIN_ROW_H;
pub const PIN_GAP: f32 = layout::PIN_GAP;
pub const PIN_SIZE: f32 = layout::PIN_SIZE;

const WIRE_SEGS: usize = 32;
const WIRE_THICKNESS: f32 = 2.8;
const HEADER_FONT: f32 = 12.5;
const PIN_FONT: f32 = 10.5;
const HEADER_PAD_X: f32 = 9.0;
const COMMENT_TITLE_PAD_X: f32 = 12.0;
const COMMENT_TITLE_PAD_Y: f32 = 6.0;

pub struct NodeGraphRenderer;

// ─── coordinate utilities ─────────────────────────────────────────────────────

impl NodeGraphRenderer {
    #[inline]
    pub fn graph_to_screen_pos(p: Point<f32>, graph: &BlueprintGraph) -> Point<f32> {
        Point::new(
            (p.x + graph.pan_offset.x) * graph.zoom_level,
            (p.y + graph.pan_offset.y) * graph.zoom_level,
        )
    }

    #[inline]
    pub fn screen_to_graph_pos(p: Point<Pixels>, graph: &BlueprintGraph) -> Point<f32> {
        Point::new(
            p.x.as_f32() / graph.zoom_level - graph.pan_offset.x,
            p.y.as_f32() / graph.zoom_level - graph.pan_offset.y,
        )
    }

    pub fn window_to_graph_element_pos(
        window_pos: Point<Pixels>,
        canvas: &GraphCanvasPanel,
    ) -> Point<Pixels> {
        let o = *canvas.canvas_origin.borrow();
        Point::new(window_pos.x - px(o.x), window_pos.y - px(o.y))
    }

    pub fn window_to_graph_element_pos_for_view(
        window_pos: Point<Pixels>,
        canvas: &GraphCanvasPanel,
        _view_id: &str,
    ) -> Point<Pixels> {
        Self::window_to_graph_element_pos(window_pos, canvas)
    }

    pub fn snap_to_grid(pos: Point<f32>) -> Point<f32> {
        let g = layout::GRID_SNAP;
        Point::new((pos.x / g).round() * g, (pos.y / g).round() * g)
    }

    pub fn pin_canvas_pos(
        node: &BlueprintNode,
        is_input: bool,
        row: usize,
        pin_id: Option<&str>,
        graph: &BlueprintGraph,
    ) -> Point<f32> {
        // Special case: __return__ pin renders in the header (right side)
        if pin_id == Some("__return__") {
            let scr = Self::graph_to_screen_pos(node.position, graph);
            let px_ = scr.x + (node.size.width - 24.0) * graph.zoom_level;
            let py = scr.y + HEADER_H * 0.5 * graph.zoom_level;
            return Point::new(px_, py);
        }
        let zoom = graph.zoom_level;
        let scr = Self::graph_to_screen_pos(node.position, graph);
        let py = scr.y
            + (HEADER_H + SEP_H + BODY_PAD) * zoom
            + row as f32 * (PIN_ROW_H + PIN_GAP) * zoom
            + PIN_ROW_H * 0.5 * zoom;
        let px_ = if is_input {
            scr.x + BODY_PAD * zoom
        } else {
            scr.x + (node.size.width - BODY_PAD) * zoom
        };
        Point::new(px_, py)
    }

    pub fn calculate_pin_position(
        node: &BlueprintNode,
        pin_id: &str,
        is_input: bool,
        graph: &BlueprintGraph,
    ) -> Option<Point<f32>> {
        if node.node_type == NodeType::Reroute {
            return Some(Self::graph_to_screen_pos(node.position, graph));
        }
        let row = if is_input {
            node.inputs.iter().position(|p| p.id == pin_id)?
        } else {
            node.outputs.iter().position(|p| p.id == pin_id)?
        };
        Some(Self::pin_canvas_pos(node, is_input, row, Some(pin_id), graph))
    }

    pub fn calculate_pin_position_graph_space(
        node: &BlueprintNode,
        is_input: bool,
        row: usize,
        _graph: &BlueprintGraph,
    ) -> Point<f32> {
        let py = node.position.y
            + HEADER_H
            + SEP_H
            + BODY_PAD
            + row as f32 * (PIN_ROW_H + PIN_GAP)
            + PIN_ROW_H * 0.5;
        let px_ = if is_input {
            node.position.x + BODY_PAD
        } else {
            node.position.x + node.size.width - BODY_PAD
        };
        Point::new(px_, py)
    }

    /// Backwards-compat: is this node inside the viewport?
    pub fn is_node_visible_simple(node: &BlueprintNode, graph: &BlueprintGraph) -> bool {
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

    pub fn is_connection_visible_simple(conn: &Connection, graph: &BlueprintGraph) -> bool {
        let from = graph.nodes.iter().find(|n| n.id == conn.source_node);
        let to = graph.nodes.iter().find(|n| n.id == conn.target_node);
        match (from, to) {
            (Some(f), Some(t)) => {
                Self::is_node_visible_simple(f, graph) || Self::is_node_visible_simple(t, graph)
            }
            _ => false,
        }
    }

    pub fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
        let hex = hex.trim_start_matches('#');
        let p = |s: &str| u8::from_str_radix(s, 16).ok().map(|v| v as f32 / 255.0);
        if hex.len() == 6 {
            Some(gpui::Hsla::from(gpui::Rgba {
                r: p(&hex[0..2])?,
                g: p(&hex[2..4])?,
                b: p(&hex[4..6])?,
                a: 1.0,
            }))
        } else if hex.len() == 8 {
            Some(gpui::Hsla::from(gpui::Rgba {
                r: p(&hex[0..2])?,
                g: p(&hex[2..4])?,
                b: p(&hex[4..6])?,
                a: p(&hex[6..8])?,
            }))
        } else {
            None
        }
    }
}

// ─── colour helpers ───────────────────────────────────────────────────────────

fn category_color(node: &BlueprintNode) -> [f32; 4] {
    if let Some(ref hex) = node.color {
        let h = hex.trim_start_matches('#');
        let p = |s: &str| u8::from_str_radix(s, 16).ok().map(|v| v as f32 / 255.0);
        if h.len() == 6 {
            if let (Some(r), Some(g), Some(b)) = (p(&h[0..2]), p(&h[2..4]), p(&h[4..6])) {
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
        NodeType::MacroEntry | NodeType::MacroExit => [0.44, 0.18, 0.72, 1.0],
        NodeType::MacroInstance => [0.32, 0.12, 0.52, 1.0],
        NodeType::CustomEvent => [0.90, 0.50, 0.10, 1.0],       // orange
        NodeType::CustomEventDispatch => [0.10, 0.60, 0.85, 1.0], // cyan
    }
}

fn pin_color(dt: &DataType) -> [f32; 4] {
    dt.display_color()
}

fn wire_phase(conn: &Connection) -> f32 {
    let mut h: u32 = 2166136261;
    for b in conn
        .source_node
        .bytes()
        .chain(conn.source_pin.bytes())
        .chain(conn.target_node.bytes())
        .chain(conn.target_pin.bytes())
    {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    (h as f32 / u32::MAX as f32) * 2.0
}

// ─── geometry helpers — all positions in GRAPH SPACE ─────────────────────────
// The GPU vertex shaders apply graph→screen transform (pan+zoom).
// CPU must NOT pre-apply pan or zoom to positions used by the GPU pipelines.
// Exception: text positions are in screen space because text.wgsl uses NDC direct.

/// Graph-space pin centre for a given node row (input or output side).
/// No pan or zoom applied — the GPU shader handles the transform.
fn pin_gpos_row(node: &BlueprintNode, is_input: bool, row: usize) -> (f32, f32) {
    if node.node_type == NodeType::Reroute {
        return (node.position.x, node.position.y);
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

/// Graph-space pin centre addressed by pin ID.
fn pin_gpos_id(node: &BlueprintNode, pin_id: &str, is_input: bool) -> Option<(f32, f32)> {
    if node.node_type == NodeType::Reroute {
        return Some((node.position.x, node.position.y));
    }
    if pin_id == "__return__" {
        return Some((
            node.position.x + node.size.width - 24.0,
            node.position.y + HEADER_H * 0.5,
        ));
    }
    let row = if is_input {
        node.inputs.iter().position(|p| p.id == pin_id)?
    } else {
        node.outputs.iter().position(|p| p.id == pin_id)?
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

/// Tessellate a bezier wire into thick-quad segments — positions in GRAPH SPACE.
/// half_thick is in graph units (not multiplied by zoom — shader handles scale).
fn tessellate_wire(
    from: (f32, f32),
    to: (f32, f32),
    color: [f32; 4],
    half_thick: f32,
) -> Vec<WireVertex> {
    let hd = (to.0 - from.0).abs();
    // Control point offset in graph units — keeps wire shape consistent at all zoom levels.
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
        out.push(WireVertex {
            pos: [prev.0 + nx, prev.1 + ny],
            uv: [0.0, v0],
            color,
        });
        out.push(WireVertex {
            pos: [prev.0 - nx, prev.1 - ny],
            uv: [1.0, v0],
            color,
        });
        out.push(WireVertex {
            pos: [cur.0 + nx, cur.1 + ny],
            uv: [0.0, v1],
            color,
        });
        out.push(WireVertex {
            pos: [cur.0 + nx, cur.1 + ny],
            uv: [0.0, v1],
            color,
        });
        out.push(WireVertex {
            pos: [prev.0 - nx, prev.1 - ny],
            uv: [1.0, v0],
            color,
        });
        out.push(WireVertex {
            pos: [cur.0 - nx, cur.1 - ny],
            uv: [1.0, v1],
            color,
        });
        prev = cur;
    }
    out
}

/// Tessellate a straight line segment — no bezier, no S-curves.
/// Used for the selection box where all edges must be perfectly straight.
fn tessellate_line(
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

// ─── main render ──────────────────────────────────────────────────────────────

type TextCall = (String, f32, f32, f32, [f32; 4], bool); // (text, x, y, size, color, center)

impl NodeGraphRenderer {
    pub fn render(
        canvas: &mut GraphCanvasPanel,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> impl IntoElement {
        // Animated pan interpolation (500ms cubic ease-out)
        if let Some(target) = canvas.pan_anim_target {
            if let Some((start, start_time)) = &canvas.pan_anim_start {
                let elapsed = start_time.elapsed().as_secs_f32();
                let t = (elapsed / 0.5).min(1.0);
                let eased = 1.0 - (1.0 - t).powi(3);
                canvas.graph.pan_offset.x = start.x + (target.x - start.x) * eased;
                canvas.graph.pan_offset.y = start.y + (target.y - start.y) * eased;
                if t >= 1.0 {
                    canvas.pan_anim_target = None;
                    canvas.pan_anim_start = None;
                } else {
                    cx.notify();
                }
            }
        }

        let canvas_entity = cx.entity().clone();
        let zoom = canvas.graph.zoom_level;
        let pan_x = canvas.graph.pan_offset.x;
        let pan_y = canvas.graph.pan_offset.y;
        let wire_active_mode = canvas.wire_active_test_mode;
        let wire_hidden_mode = canvas.wire_hidden_test_mode;
        let anim_time = canvas.graph_anim_start.elapsed().as_secs_f32();

        // viewport culling
        let (vw, vh) = canvas
            .element_bounds
            .map(|b| {
                (
                    b.size.width.as_f32().max(1.0),
                    b.size.height.as_f32().max(1.0),
                )
            })
            .unwrap_or((3840.0, 2160.0));
        let pad = (260.0 / zoom.max(0.05)).max(120.0);
        let (vl, vt, vr, vb) = (
            -pan_x - pad,
            -pan_y - pad,
            -pan_x + vw / zoom + pad,
            -pan_y + vh / zoom + pad,
        );
        let visible = |n: &BlueprintNode| {
            !(n.position.x > vr
                || n.position.x + n.size.width < vl
                || n.position.y > vb
                || n.position.y + n.size.height < vt)
        };

        let dragging_conn = canvas.dragging_connection.clone();
        let selected_nodes: std::collections::HashSet<&str> = canvas
            .graph
            .selected_nodes
            .iter()
            .map(|id| id.as_str())
            .collect();
        let running_nodes: std::collections::HashSet<&str> =
            canvas.running_nodes.iter().map(|id| id.as_str()).collect();
        let node_is_active = |node_id: &str| {
            running_nodes.contains(node_id)
                || (wire_active_mode && selected_nodes.contains(node_id))
        };

        let mut comment_instances: Vec<crate::rendering::gpu::CommentInstance> = Vec::new();
        let mut comment_text_calls: Vec<TextCall> = Vec::new();
        let mut comment_refs: Vec<&BlueprintComment> = canvas
            .graph
            .comments
            .iter()
            .filter(|comment| {
                comment.position.x + comment.size.width >= vl
                    && comment.position.x <= vr
                    && comment.position.y + comment.size.height >= vt
                    && comment.position.y <= vb
            })
            .collect();
        comment_refs.sort_by(|a, b| {
            let area_a = a.size.width * a.size.height;
            let area_b = b.size.width * b.size.height;
            area_b.partial_cmp(&area_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        for comment in comment_refs {
            let selected = canvas.graph.selected_comments.contains(&comment.id);
            let fill: gpui::Rgba = comment.color.into();
            let fill_rgba = [fill.r, fill.g, fill.b, fill.a];
            let border_alpha = if selected { 0.98 } else { 0.78 };
            let border_rgba = [fill.r * 0.72, fill.g * 0.72, fill.b * 0.72, border_alpha];
            let luma = fill.r * 0.299 + fill.g * 0.587 + fill.b * 0.114;
            let text_rgba = if luma < 0.55 {
                [1.0, 1.0, 1.0, 0.95]
            } else {
                [0.07, 0.07, 0.08, 0.98]
            };
            comment_instances.push(crate::rendering::gpu::CommentInstance {
                pos: [comment.position.x, comment.position.y],
                size: [comment.size.width, comment.size.height],
                fill_color: fill_rgba,
                border_color: border_rgba,
                corner_r: 10.0 / zoom,
                flags: selected as u32,
                _pad0: 0,
                _pad1: 0,
            });

            if zoom >= 0.22 && canvas.editing_comment.as_deref() != Some(comment.id.as_str()) {
                let scr = Self::graph_to_screen_pos(comment.position, &canvas.graph);
                comment_text_calls.push((
                    comment.text.clone(),
                    scr.x + 12.0 * zoom,
                    scr.y + 16.0 * zoom,
                    12.5 * zoom,
                    text_rgba,
                    false,
                ));
            }
        }

        let mut node_instances: Vec<NodeInstance> = Vec::new();
        let mut pin_instances: Vec<PinInstance> = Vec::new();
        let mut text_calls: Vec<TextCall> = Vec::new();

        for node in &canvas.graph.nodes {
            if !visible(node) {
                continue;
            }

            let is_sel = selected_nodes.contains(node.id.as_str());
            let is_reroute = node.node_type == NodeType::Reroute;
            let cat = category_color(node);
            let hdr = [
                (cat[0] * 0.85 + 0.12).min(1.0),
                (cat[1] * 0.85 + 0.12).min(1.0),
                (cat[2] * 0.85 + 0.12).min(1.0),
                1.0,
            ];
            let body = [0.102, 0.117, 0.138, 1.0]; // dark slate
            let bord = if is_sel {
                [
                    0.56 + cat[0] * 0.24,
                    0.58 + cat[1] * 0.24,
                    0.62 + cat[2] * 0.26,
                    1.0,
                ]
            } else {
                [0.205, 0.218, 0.246, 1.0]
            };
            let sep = [0.086, 0.098, 0.116, 1.0];

            let max_rows = node.inputs.len().max(node.outputs.len()).max(1);
            let gw = layout::snap_to_grid(node.size.width);
            let gh = layout::snap_to_grid(layout::node_height_for_pin_rows(max_rows));
            let hdr_frac = (HEADER_H + SEP_H) / gh;
            let is_running = node_is_active(node.id.as_str());
            let flags = (is_reroute as u32) | ((is_sel as u32) << 1) | ((is_running as u32) << 2);

            node_instances.push(NodeInstance {
                pos: [node.position.x, node.position.y],
                size: [gw, gh],
                header_color: hdr,
                body_color: body,
                border_color: bord,
                sep_color: sep,
                header_h_frac: hdr_frac,
                corner_r: 6.8 / zoom,
                flags,
                _pad: 0,
            });

            // ── LOD: above this zoom level render pins, text, and labels.
            // Below it we only draw node bodies and wires — much cheaper at scale.
            const LOD_FULL: f32 = 0.35;
            const LOD_TITLES: f32 = 0.18; // show title text but still no pins

            // Node title
            if zoom >= LOD_TITLES && !is_reroute {
                let scr = Self::graph_to_screen_pos(node.position, &canvas.graph);
                text_calls.push((
                    node.title.clone(),
                    scr.x + HEADER_PAD_X * zoom,
                    scr.y + HEADER_H * zoom * 0.5 + HEADER_FONT * zoom * 0.35,
                    HEADER_FONT * zoom,
                    [0.88, 0.90, 0.95, 0.98],
                    false,
                ));
            }

            // Pins and pin labels — only at full detail zoom
            if zoom >= LOD_FULL {
                for (is_input, pins) in [
                    (true, node.inputs.as_slice()),
                    (false, node.outputs.as_slice()),
                ] {
                    for (i, pin) in pins.iter().enumerate() {
                        let is_return = pin.id == "__return__";
                        let is_fn_ptr = pin.id == "__fn_ptr__";
                        let is_special = is_return || is_fn_ptr;
                        let (cgx, cgy) = if is_return {
                            // Header position: right side, middle of header
                            let hx = node.position.x + node.size.width - 24.0;
                            let hy = node.position.y + HEADER_H * 0.5;
                            (hx, hy)
                        } else {
                            pin_gpos_row(node, is_input, i)
                        };
                        let pc = pin_color(&pin.data_type);
                        let exe = pin.data_type == DataType::execution();
                        let compat = dragging_conn.as_ref().map_or(false, |d| {
                            is_input
                                && node.id != d.source_node
                                && pin.data_type.is_compatible_with(&d.source_pin_type)
                        });
                        pin_instances.push(PinInstance {
                            center: [cgx, cgy],
                            size: PIN_SIZE,
                            _pad0: 0.0,
                            color: if is_special {
                                [1.0, 0.2, 0.2, 1.0]
                            } else {
                                pc
                            },
                            kind: if is_special { 2 } else { exe as u32 },
                            is_input: is_input as u32,
                            compatible: compat as u32,
                            _pad1: 0,
                        });
                        if !pin.name.is_empty() && !is_reroute {
                            let scr_x = (cgx + pan_x) * zoom;
                            let scr_y = (cgy + pan_y) * zoom;
                            let lx = if is_input {
                                scr_x + (PIN_SIZE * zoom * 0.5 + 5.0)
                            } else {
                                scr_x - (PIN_SIZE * zoom * 0.5 + 5.0)
                            };
                            text_calls.push((
                                pin.name.clone(),
                                lx,
                                scr_y + PIN_FONT * zoom * 0.45,
                                PIN_FONT * zoom,
                                [0.78, 0.81, 0.87, 0.98],
                                !is_input,
                            ));
                        }
                    }

                }
            }
        }

        text_calls.extend(comment_text_calls);

        // ── Bezier wire instances — one struct per connection, GPU does all tessellation ──
        // No CPU bezier evaluation: just compute four control points and hand off to GPU.
        let mut wire_instances: Vec<WireInstance> = Vec::new();
        let half_thick = WIRE_THICKNESS * 0.5; // graph-space half-thickness; shader × zoom → px

        let node_map: std::collections::HashMap<&str, &BlueprintNode> = canvas
            .graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n))
            .collect();
        let vis_ids: std::collections::HashSet<&str> = canvas
            .graph
            .nodes
            .iter()
            .filter(|n| visible(n))
            .map(|n| n.id.as_str())
            .collect();

        // Helper: build a WireInstance from two graph-space endpoints.
        let make_wire = |fp: (f32, f32),
                         tp: (f32, f32),
                         color: [f32; 4],
                         thick: f32,
                         flags: u32,
                         pulse_phase: f32|
         -> WireInstance {
            let hd = (tp.0 - fp.0).abs();
            let ctl = (hd * 0.45).max(55.0).min(220.0);
            WireInstance {
                from: [fp.0, fp.1],
                ctrl1: [fp.0 + ctl, fp.1],
                ctrl2: [tp.0 - ctl, tp.1],
                to: [tp.0, tp.1],
                color,
                thickness: thick,
                flags,
                pulse_phase,
                _pad: 0.0,
            }
        };

        for conn in &canvas.graph.connections {
            if !vis_ids.contains(conn.source_node.as_str())
                && !vis_ids.contains(conn.target_node.as_str())
            {
                continue;
            }
            let (fn_, tn) = (
                node_map.get(conn.source_node.as_str()),
                node_map.get(conn.target_node.as_str()),
            );
            if let (Some(fn_), Some(tn)) = (fn_, tn) {
                let src = fn_
                    .outputs
                    .iter()
                    .find(|p| p.id == conn.source_pin)
                    .map_or([0.8, 0.8, 0.8, 1.0], |p| pin_color(&p.data_type));
                let mut fc = [src[0], src[1], src[2], 1.0];
                let mut wire_flags = 0_u32;
                let mut thick = half_thick;
                let is_runtime_active = node_is_active(conn.source_node.as_str())
                    || node_is_active(conn.target_node.as_str());
                if wire_hidden_mode {
                    wire_flags |= 2;
                    fc[3] = 0.18;
                    thick *= 0.88;
                } else if is_runtime_active {
                    wire_flags |= 1;
                    fc[3] = 0.96;
                    thick *= 1.05;
                } else {
                    fc[3] = 0.76;
                }
                if canvas
                    .hovered_connection
                    .as_deref()
                    .map_or(false, |hovered_id| hovered_id == conn.id)
                {
                    fc = [0.98, 0.84, 0.10, fc[3]];
                    thick *= 1.12;
                }
                if let (Some(fp), Some(tp)) = (
                    pin_gpos_id(fn_, &conn.source_pin, false),
                    pin_gpos_id(tn, &conn.target_pin, true),
                ) {
                    wire_instances.push(make_wire(fp, tp, fc, thick, wire_flags, wire_phase(conn)));
                }
            }
        }

        // Drag wire preview — source pin in graph space, mouse pos converted from canvas space.
        if let Some(ref drag) = canvas.dragging_connection.clone() {
            if let Some(fn_) = node_map.get(drag.source_node.as_str()) {
                if let Some(fp) = pin_gpos_id(fn_, &drag.source_pin, false) {
                    let dc = pin_color(&drag.source_pin_type);
                    let mp = drag.current_mouse_pos;
                    let tp = (mp.x / zoom - pan_x, mp.y / zoom - pan_y);
                    let drag_active = node_is_active(drag.source_node.as_str());
                    let drag_flags = if wire_hidden_mode {
                        2
                    } else if drag_active {
                        1
                    } else {
                        0
                    };
                    let drag_alpha = if wire_hidden_mode {
                        0.18
                    } else if drag_active {
                        0.92
                    } else {
                        0.70
                    };
                    wire_instances.push(make_wire(
                        fp,
                        tp,
                        [dc[0], dc[1], dc[2], drag_alpha],
                        half_thick * 0.85,
                        drag_flags,
                        0.0,
                    ));
                }
            }
        }

        // ── Selection box — straight lines only, CPU-tessellated (4 lines = 24 verts) ──
        // Kept as a vertex buffer because there are at most 4 segments and no GPU
        // instancing overhead is worth it for that count.
        let mut line_verts: Vec<WireVertex> = Vec::new();
        if let (Some(start), Some(end)) = (canvas.selection_start, canvas.selection_end) {
            let (sx, sy, ex, ey) = (start.x, start.y, end.x, end.y);
            let sc = [0.30, 0.55, 0.90, 0.80_f32];
            let ht = 1.0 / zoom; // constant ~2 screen pixels
            line_verts.extend(tessellate_line((sx, sy), (ex, sy), sc, ht)); // top
            line_verts.extend(tessellate_line((sx, ey), (ex, ey), sc, ht)); // bottom
            line_verts.extend(tessellate_line((sx, sy), (sx, ey), sc, ht)); // left
            line_verts.extend(tessellate_line((ex, sy), (ex, ey), sc, ht)); // right
        }

        let uniforms = GraphUniforms {
            pan: [pan_x, pan_y],
            zoom,
            time: anim_time,
            viewport: [vw, vh],
            _pad1: [0.0; 2],
        };

        let focus_handle = canvas.focus_handle().clone();
        // ── WGPU surface display ──────────────────────────────────────────────
        // wgpu_surface() composites the GPU texture into the GPUI scene.
        // It must be present in the element tree for anything to appear.
        // On the first frame bp_surface is None so we show a dark placeholder;
        // the canvas prepaint creates the surface and requests a re-render,
        // so frame 2 immediately shows the GPU output.
        let gpu_display: AnyElement = if let Some(ref s) = canvas.surface {
            wgpu_surface(s.clone())
                .defer_resize_until_mouse_up(true)
                .absolute()
                .inset_0()
                .into_any_element()
        } else {
            div()
                .absolute()
                .inset_0()
                .bg(gpui::Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.055,
                    a: 1.0,
                })
                .into_any_element()
        };

        // ── Canvas: creates surface in prepaint (has window), renders in paint ─
        let driver = {
            let pe_pre = canvas_entity.clone();
            let pe_paint = canvas_entity.clone();
            gpui::canvas(
                // Prepaint: surface creation (first frame only).
                // Called before paint — window is available here.
                move |bounds, window, cx| {
                    // Capture element bounds for coordinate conversion
                    let ox = bounds.origin.x.as_f32();
                    let oy = bounds.origin.y.as_f32();
                    let sw = bounds.size.width.as_f32() as u32;
                    let sh = bounds.size.height.as_f32() as u32;

                    pe_pre.update(cx, |canvas, cx| {
                        *canvas.canvas_origin.borrow_mut() = Point::new(ox, oy);
                        let b = gpui::Bounds {
                            origin: gpui::Point {
                                x: px(ox),
                                y: px(oy),
                            },
                            size: gpui::Size {
                                width: px(sw as f32),
                                height: px(sh as f32),
                            },
                        };
                        canvas.element_bounds = Some(b);

                        // Create surface on first call — triggers re-render via notify
                        if canvas.surface.is_none() {
                            if let Some(s) = window.create_wgpu_surface(
                                sw.max(64),
                                sh.max(64),
                                wgpu::TextureFormat::Bgra8UnormSrgb,
                            ) {
                                canvas.surface = Some(s);
                                cx.notify(); // re-render to pick up wgpu_surface() element
                            }
                        }
                    });
                },
                // Paint: render GPU frame every frame.
                move |_bounds, _pre, _window, cx| {
                    pe_paint.update(cx, |canvas, cx| {
                        let Some(ref surface) = canvas.surface else {
                            return;
                        };
                        if surface.is_resize_pending() {
                            return;
                        }
                        let Some((view, (w, h))) = surface.back_view_with_size() else {
                            return;
                        };

                        let frame_uni = GraphUniforms {
                            viewport: [w as f32, h as f32],
                            ..uniforms
                        };
                        canvas.renderer.render_frame(
                                            surface.device(),
                                            surface.queue(),
                                            &view,
                                            w,
                                            h,
                                            surface.format(),
                                            &frame_uni,
                                            &comment_instances,
                                            &node_instances,
                                            &wire_instances, // one struct per bezier connection
                            &line_verts,     // selection box straight lines only
                            &pin_instances,
                            &text_calls,
                        );
                        drop(view);
                        surface.swap_buffers();
                        if !canvas.running_nodes.is_empty()
                            || (canvas.wire_active_test_mode
                                && !canvas.graph.selected_nodes.is_empty())
                        {
                            cx.notify();
                        }
                    });
                },
            )
            .absolute()
            .inset_0()
            .size_full()
        };

        // Wrap the canvas in DropAreas so sidebars can drag items directly onto the graph.
        ui::drop_area::DropArea::<crate::features::prefabs::ComponentDrag>::new(
            "blueprint-canvas-component-drop",
        )
        .can_accept(|_| true)
        .on_drop(cx.listener(
            |canvas, payload: &crate::features::prefabs::ComponentDrag, window, cx| {
                let mouse_pos = window.mouse_position();
                canvas.finish_dragging_component(payload.clone(), mouse_pos, cx);
            },
        ))
        .child(
            ui::drop_area::DropArea::<crate::features::macros::MacroDrag>::new(
                "blueprint-canvas-macro-drop",
            )
            .can_accept(|_| true)
            .on_drop(cx.listener(
                |canvas, payload: &crate::features::macros::MacroDrag, window, cx| {
                    canvas.dragging_macro = Some(payload.clone());
                    let mouse_pos = window.mouse_position();
                    canvas.finish_dragging_macro(mouse_pos, cx);
                },
            ))
            .child(
                div()
                    .size_full()
                    .relative()
                    .overflow_hidden()
                    .track_focus(&focus_handle)
                    .key_context("BlueprintGraph")
                    .child(gpu_display) // wgpu_surface() or dark placeholder — MUST be first
                    .child(driver) // invisible canvas that drives GPU rendering
                    // GPUI-only overlays (palette + context menus) sit on top
                    .child(Self::render_quick_palette_overlay_inner(
                        canvas.quick_palette_open,
                        canvas.quick_palette_screen_pos,
                        canvas.quick_palette_view.clone(),
                        canvas.quick_palette_focus_pending,
                        cx,
                    ))
                    .child(Self::render_node_context_menu(canvas, cx))
                    .child(Self::render_pin_context_menu(canvas, cx))
                    .child(Self::render_breakpoint_badges(canvas, cx))
                    .child(Self::render_debug_hud(canvas, cx))
                    .child(Self::render_macro_pin_editor(canvas, cx))
                    .child(Self::render_comment_title_editor(canvas, cx))
                    // input
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |canvas, _, window, cx| {
                            canvas.focus_handle().focus(window, cx);
                            if canvas.variable_drop_menu_position.is_some() {
                                canvas.variable_drop_menu_position = None;
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        crate::rendering::input::on_mouse_down_right(cx),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        crate::rendering::input::on_mouse_down_left(cx),
                    )
                    .on_mouse_move(crate::rendering::input::on_mouse_move(cx))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        crate::rendering::input::on_mouse_up_left(cx),
                    )
                    .on_mouse_up_out(
                        gpui::MouseButton::Left,
                        crate::rendering::input::on_mouse_up_left(cx),
                    )
                    .on_mouse_up(
                        gpui::MouseButton::Right,
                        crate::rendering::input::on_mouse_up_right(cx),
                    )
                    .on_mouse_up_out(
                        gpui::MouseButton::Right,
                        crate::rendering::input::on_mouse_up_right(cx),
                    )
                    .on_scroll_wheel(crate::rendering::input::on_scroll_wheel(cx))
                    .on_key_down(crate::rendering::input::on_key_down(cx)),
            ), // close macro DropArea child
        ) // close component DropArea child
    }

    fn render_quick_palette_overlay_inner(
        open: bool,
        screen_pos: Point<Pixels>,
        palette_view: gpui::Entity<crate::ui_components::palette_view::NodePaletteView>,
        focus_pending: bool,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        if !open {
            return div().into_any_element();
        }
        let canvas_entity = cx.entity().clone();
        deferred(
            anchored()
                .position(screen_pos)
                .snap_to_window_with_margin(px(8.0))
                .anchor(gpui::Corner::TopLeft)
                .child(
                    div()
                        .occlude()
                        .w(px(320.0))
                        .h(px(480.0))
                        .shadow_lg()
                        .rounded(px(6.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(palette_view)
                        .on_children_prepainted({
                            let pe = canvas_entity.clone();
                            move |_, window, cx| {
                                pe.update(cx, |canvas, cx| {
                                    if !canvas.quick_palette_focus_pending {
                                        return;
                                    }
                                    let h =
                                        canvas.quick_palette_view.read(cx).search_focus_handle(cx);
                                    canvas.quick_palette_focus_pending = false;
                                    window.focus(&h, cx);
                                });
                            }
                        })
                        .on_mouse_down_out(move |_, _, cx| {
                            canvas_entity.update(cx, |canvas, cx| {
                                canvas.quick_palette_open = false;
                                canvas.quick_palette_focus_pending = false;
                                canvas.quick_palette_connection_source = None;
                                canvas.popup_palette_graph_pos = None;
                                cx.notify();
                            });
                        }),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }

    fn render_comment_title_editor(
        canvas: &GraphCanvasPanel,
        _cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        let Some(comment_id) = canvas.editing_comment.as_deref() else {
            return div().into_any_element();
        };
        let Some(comment) = canvas.graph.comments.iter().find(|c| c.id == comment_id) else {
            return div().into_any_element();
        };

        let zoom = canvas.graph.zoom_level;
        let title_h = (30.0 * zoom).clamp(18.0, 44.0);
        let title_w = (comment.size.width * zoom - COMMENT_TITLE_PAD_X * 2.0 * zoom).max(80.0);
        let scr = Self::graph_to_screen_pos(comment.position, &canvas.graph);
        let canvas_origin = *canvas.canvas_origin.borrow();
        let window_pos = Point::new(
            px(canvas_origin.x + scr.x + COMMENT_TITLE_PAD_X * zoom),
            px(canvas_origin.y + scr.y + COMMENT_TITLE_PAD_Y * zoom),
        );

        deferred(
            anchored()
                .position(window_pos)
                .snap_to_window_with_margin(px(4.0))
                .anchor(gpui::Corner::TopLeft)
                .child(
                    div()
                        .occlude()
                        .w(px(title_w))
                        .h(px(title_h * 0.68))
                        .child(
                            ui::input::TextInput::new(&canvas.comment_text_input)
                                .appearance(false)
                                .bordered(false)
                                .focus_bordered(false),
                        ),
                ),
        )
        .with_priority(2)
        .into_any_element()
    }

    // ── Macro Pin Editor ──────────────────────────────────────────────────────
    //
    // Shown as a top-right overlay when the active tab is a local macro.
    // Lets the user add and remove interface pins (inputs / outputs) and reflects
    // changes immediately in the Macro Entry / Exit nodes and all instances.

    fn render_macro_pin_editor(
        canvas: &GraphCanvasPanel,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        use ui::{h_flex, v_flex, ActiveTheme};

        // Only show when we're inside a macro graph tab.
        if canvas.is_main {
            return div().into_any_element();
        }
        let macro_id = canvas.id.clone();

        // local_macros live on the shared panel
        let local_macros = canvas
            .panel
            .upgrade()
            .map(|p| p.read(cx).local_macros.clone())
            .unwrap_or_default();
        let macro_def = match local_macros.iter().find(|m| m.id == macro_id) {
            Some(m) => m.clone(),
            None => return div().into_any_element(),
        };

        // UI inputs for pin name / type also live on the shared panel
        let (name_input_opt, type_dd_opt) = canvas
            .panel
            .upgrade()
            .map(|p| {
                let r = p.read(cx);
                (r.variable_name_input.clone(), r.variable_type_dropdown.clone())
            })
            .unzip();
        let (Some(name_input_shared), Some(type_dd_shared)) = (name_input_opt, type_dd_opt)
        else {
            return div().into_any_element();
        };

        let pe = cx.entity().clone();
        let mid = macro_id.clone();

        // ── Add-pin form (shown when macro_pin_add_mode is Some) ──────────────
        let add_form = if let Some(is_input) = canvas.macro_pin_add_mode {
            let pe_submit = pe.clone();
            let pe_cancel = pe.clone();
            let mid2 = mid.clone();
            let name_input = name_input_shared.clone();
            let type_dd = type_dd_shared.clone();
            let dir_label = if is_input { "Input" } else { "Output" };

            h_flex()
                .gap(px(6.0))
                .items_center()
                .px(px(8.0))
                .py(px(4.0))
                .bg(cx.theme().muted.opacity(0.4))
                .rounded(px(4.0))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("+ {} pin:", dir_label)),
                )
                .child(
                    div()
                        .w(px(110.0))
                        .h(px(24.0))
                        .child(ui::input::TextInput::new(&name_input)),
                )
                .child(
                    div()
                        .w(px(90.0))
                        .h(px(24.0))
                        .child(ui::dropdown::Dropdown::new(&type_dd)),
                )
                // Confirm
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(3.0))
                        .text_size(px(10.0))
                        .rounded(px(3.0))
                        .bg(gpui::rgba(0x1A4A1AFF))
                        .text_color(gpui::rgba(0x88FF88FF))
                        .cursor_pointer()
                        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                            pe_submit.update(cx, |canvas, cx| {
                                let name = canvas
                                    .panel
                                    .upgrade()
                                    .map(|p| {
                                        p.read(cx)
                                            .variable_name_input
                                            .read(cx)
                                            .text()
                                            .to_string()
                                    })
                                    .unwrap_or_default()
                                    .trim()
                                    .to_string();
                                let type_str = canvas
                                    .panel
                                    .upgrade()
                                    .and_then(|p| {
                                        p.read(cx)
                                            .variable_type_dropdown
                                            .read(cx)
                                            .selected_value()
                                            .map(|v| v.to_string())
                                    })
                                    .unwrap_or_else(|| "f32".to_string());
                                if !name.is_empty() {
                                    if let Some(p) = canvas.panel.upgrade() {
                                        p.update(cx, |panel, cx| panel.add_macro_pin(&mid2, name, type_str, is_input, cx));
                                    }
                                }
                                canvas.macro_pin_add_mode = None;
                                cx.notify();
                            });
                        })
                        .child("✓ Add"),
                )
                // Cancel
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(3.0))
                        .text_size(px(10.0))
                        .rounded(px(3.0))
                        .bg(cx.theme().muted.opacity(0.3))
                        .text_color(cx.theme().muted_foreground)
                        .cursor_pointer()
                        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                            pe_cancel.update(cx, |canvas, cx| {
                                canvas.macro_pin_add_mode = None;
                                cx.notify();
                            });
                        })
                        .child("✕"),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };

        // ── Pin list ──────────────────────────────────────────────────────────
        let inputs_col = {
            let pe_add = pe.clone();
            let mid_add = mid.clone();
            let mut col = v_flex().gap(px(3.0)).min_w(px(140.0)).child(
                h_flex()
                    .gap(px(4.0))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(cx.theme().success)
                            .child("INPUTS"),
                    )
                    .child(
                        div()
                            .px(px(6.0))
                            .py(px(2.0))
                            .text_size(px(9.0))
                            .rounded(px(3.0))
                            .bg(cx.theme().success.opacity(0.15))
                            .text_color(cx.theme().success)
                            .cursor_pointer()
                            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                pe_add.update(cx, |canvas, cx| {
                                    canvas.macro_pin_add_mode = Some(true);
                                    if let Some(p) = canvas.panel.upgrade() { p.update(cx, |panel, cx| panel.start_creating_variable(window, cx)); };
                                    cx.notify();
                                });
                            })
                            .child("+ Add"),
                    ),
            );
            for pin in &macro_def.interface.inputs {
                let pe2 = pe.clone();
                let mid2 = mid.clone();
                let pin_id = pin.id.clone();
                let pin_name = pin.name.clone();
                let type_label = format!("{:?}", pin.data_type)
                    .split('(')
                    .next()
                    .unwrap_or("?")
                    .to_string();
                col = col.child(
                    h_flex()
                        .gap(px(4.0))
                        .items_center()
                        .px(px(4.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .bg(cx.theme().muted.opacity(0.2))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(cx.theme().foreground)
                                .child(pin_name),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(":{}", type_label)),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .w(px(14.0))
                                .h(px(14.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(9.0))
                                .text_color(cx.theme().muted_foreground)
                                .cursor_pointer()
                                .hover(|s| s.text_color(gpui::rgba(0xFF6666FF)))
                                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                    pe2.update(cx, |canvas, cx| {
                                        if let Some(p) = canvas.panel.upgrade() {
                                            p.update(cx, |panel, cx| panel.remove_macro_pin(&mid2, &pin_id, true, cx));
                                        }
                                    });
                                })
                                .child("✕"),
                        ),
                );
            }
            col
        };

        let outputs_col = {
            let pe_add = pe.clone();
            let mid_add = mid.clone();
            let mut col = v_flex().gap(px(3.0)).min_w(px(140.0)).child(
                h_flex()
                    .gap(px(4.0))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(cx.theme().warning)
                            .child("OUTPUTS"),
                    )
                    .child(
                        div()
                            .px(px(6.0))
                            .py(px(2.0))
                            .text_size(px(9.0))
                            .rounded(px(3.0))
                            .bg(cx.theme().warning.opacity(0.15))
                            .text_color(cx.theme().warning)
                            .cursor_pointer()
                            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                pe_add.update(cx, |canvas, cx| {
                                    canvas.macro_pin_add_mode = Some(false);
                                    if let Some(p) = canvas.panel.upgrade() { p.update(cx, |panel, cx| panel.start_creating_variable(window, cx)); };
                                    cx.notify();
                                });
                            })
                            .child("+ Add"),
                    ),
            );
            for pin in &macro_def.interface.outputs {
                let pe2 = pe.clone();
                let mid2 = mid.clone();
                let pin_id = pin.id.clone();
                let pin_name = pin.name.clone();
                let type_label = format!("{:?}", pin.data_type)
                    .split('(')
                    .next()
                    .unwrap_or("?")
                    .to_string();
                col = col.child(
                    h_flex()
                        .gap(px(4.0))
                        .items_center()
                        .px(px(4.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .bg(cx.theme().muted.opacity(0.2))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(cx.theme().foreground)
                                .child(pin_name),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(":{}", type_label)),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .w(px(14.0))
                                .h(px(14.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(9.0))
                                .text_color(cx.theme().muted_foreground)
                                .cursor_pointer()
                                .hover(|s| s.text_color(gpui::rgba(0xFF6666FF)))
                                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                    pe2.update(cx, |canvas, cx| {
                                        if let Some(p) = canvas.panel.upgrade() {
                                            p.update(cx, |panel, cx| panel.remove_macro_pin(&mid2, &pin_id, false, cx));
                                        }
                                    });
                                })
                                .child("✕"),
                        ),
                );
            }
            col
        };

        // ── Outer panel (top-right of canvas) ─────────────────────────────────
        div()
            .absolute()
            .top(px(8.0))
            .right(px(8.0))
            .child(
                v_flex()
                    .occlude()
                    .gap(px(6.0))
                    .p(px(10.0))
                    .bg(cx.theme().popover.opacity(0.95))
                    .border_1()
                    .border_color(gpui::rgba(0x9B59B644))
                    .shadow_lg()
                    .rounded(px(8.0))
                    .child(
                        h_flex()
                            .gap(px(6.0))
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(gpui::rgba(0xC084FCFF))
                                    .child("📦 Macro Interface"),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("— {}", macro_def.name)),
                            ),
                    )
                    .child(add_form)
                    .child(
                        h_flex()
                            .gap(px(16.0))
                            .items_start()
                            .child(inputs_col)
                            .child(
                                div()
                                    .w(px(1.0))
                                    .self_stretch()
                                    .bg(cx.theme().border.opacity(0.4)),
                            )
                            .child(outputs_col),
                    ),
            )
            .into_any_element()
    }

    // ── Breakpoint stop-sign badges ───────────────────────────────────────────
    //
    // For every node that has a breakpoint we render a small red octagon badge
    // in its top-left corner.  The badge is a GPUI overlay element (not GPU
    // shader) positioned using the same graph→screen coordinate transform used
    // by the GPU renderer.

    fn render_breakpoint_badges(
        canvas: &GraphCanvasPanel,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        if canvas.breakpoints.is_empty() {
            return div().into_any_element();
        }

        let origin = *canvas.canvas_origin.borrow();
        let paused_node = canvas
            .debug_session
            .as_ref()
            .and_then(|s| s.current())
            .map(|f| f.node_id.clone());

        // Collect badge positions for all breakpointed nodes that are on screen.
        let badges: Vec<(Point<Pixels>, bool)> = canvas
            .graph
            .nodes
            .iter()
            .filter(|n| canvas.breakpoints.contains(&n.id))
            .map(|n| {
                let scr = Self::graph_to_screen_pos(n.position, &canvas.graph);
                let win = Point::new(
                    px(scr.x + origin.x - 4.0), // slightly outside left edge
                    px(scr.y + origin.y - 4.0), // slightly above top edge
                );
                let is_current = paused_node.as_deref() == Some(n.id.as_str());
                (win, is_current)
            })
            .collect();

        if badges.is_empty() {
            return div().into_any_element();
        }

        let mut container = div();
        for (pos, is_current) in badges {
            let badge = deferred(
                anchored()
                    .position(pos)
                    .anchor(gpui::Corner::TopLeft)
                    .child(
                        div()
                            // Red octagon badge using a square with a colored border
                            .w(px(18.0))
                            .h(px(18.0))
                            .rounded(px(3.0))
                            .bg(if is_current {
                                gpui::rgba(0xFF4400FF) // bright orange-red when active
                            } else {
                                gpui::rgba(0xCC1111FF) // darker red when inactive
                            })
                            .border_1()
                            .border_color(gpui::rgba(0xFF8888FF))
                            .flex()
                            .items_center()
                            .justify_center()
                            .shadow_md()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(gpui::white())
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("⏹"),
                            ),
                    ),
            )
            .with_priority(3)
            .into_any_element();
            container = container.child(badge);
        }

        // Pulsing ring around the currently-paused node
        if let Some(ref node_id) = paused_node {
            if let Some(node) = canvas.graph.nodes.iter().find(|n| &n.id == node_id) {
                let scr = Self::graph_to_screen_pos(node.position, &canvas.graph);
                let w = node.size.width * canvas.graph.zoom_level;
                let h = node.size.height * canvas.graph.zoom_level;
                let ring_pos = Point::new(px(scr.x + origin.x - 2.0), px(scr.y + origin.y - 2.0));
                let ring = deferred(
                    anchored()
                        .position(ring_pos)
                        .anchor(gpui::Corner::TopLeft)
                        .child(
                            div()
                                .w(px(w + 4.0))
                                .h(px(h + 4.0))
                                .rounded(px(8.0))
                                .border_2()
                                .border_color(gpui::rgba(0xFF440088))
                                .shadow_lg(),
                        ),
                )
                .with_priority(2)
                .into_any_element();
                container = container.child(ring);
            }
        }

        container.into_any_element()
    }

    // ── Debug HUD ─────────────────────────────────────────────────────────────
    //
    // Shown at the bottom-centre of the graph canvas when a debug session is
    // paused.  Displays the current frame's pin values and navigation controls.

    fn render_debug_hud(
        canvas: &GraphCanvasPanel,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        use ui::{h_flex, v_flex, ActiveTheme};

        let Some(ref session) = canvas.debug_session else {
            return div().into_any_element();
        };
        let Some(ref frame) = session.current() else {
            return div().into_any_element();
        };

        let frame_title = frame.node_title.clone();
        let frame_step = frame.step_index;
        let pin_values = frame.pin_values.clone();
        let current_idx = session.current_frame;
        let total_frames = session.frames.len();
        let can_back = session.can_step_backward();
        let can_fwd = session.can_step_forward();
        let is_paused = session.is_paused;

        let pe = cx.entity().clone();
        let pe_continue = pe.clone();
        let pe_fwd = pe.clone();
        let pe_back = pe.clone();
        let pe_stop = pe.clone();

        let hud = div()
            .absolute()
            .bottom(px(16.0))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                v_flex()
                    .occlude()
                    .w(px(520.0))
                    .bg(cx.theme().popover.opacity(0.96))
                    .border_1()
                    .border_color(gpui::rgba(0xFF440044))
                    .shadow_xl()
                    .rounded(px(8.0))
                    .overflow_hidden()
                    // ── Header bar ──────────────────────────────────────────────
                    .child(
                        h_flex()
                            .px(px(12.0))
                            .py(px(6.0))
                            .bg(gpui::rgba(0x330000CC))
                            .justify_between()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap(px(6.0))
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(gpui::rgba(0xFF6666FF))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .child("⏹ BREAKPOINT"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(cx.theme().popover_foreground)
                                            .child(frame_title.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "Frame {}/{} · Step {}",
                                        current_idx + 1,
                                        total_frames,
                                        frame_step
                                    )),
                            ),
                    )
                    // ── Pin values ───────────────────────────────────────────────
                    .when(!pin_values.is_empty(), |el| {
                        el.child(v_flex().px(px(12.0)).py(px(8.0)).gap(px(3.0)).children(
                            pin_values.iter().take(8).map(|pv| {
                                let pv = pv.clone();
                                h_flex()
                                    .gap(px(8.0))
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(cx.theme().muted_foreground)
                                            .w(px(80.0))
                                            .child(pv.pin_name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(cx.theme().muted_foreground.opacity(0.7))
                                            .w(px(60.0))
                                            .child(format!("({})", pv.type_label)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(gpui::rgba(0x88FF88FF))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child(pv.value.clone()),
                                    )
                            }),
                        ))
                    })
                    .when(pin_values.is_empty(), |el| {
                        el.child(
                            div()
                                .px(px(12.0))
                                .py(px(8.0))
                                .text_size(px(11.0))
                                .text_color(cx.theme().muted_foreground)
                                .child("No data pins on this node"),
                        )
                    })
                    // ── Control bar ──────────────────────────────────────────────
                    .child(
                        h_flex()
                            .px(px(12.0))
                            .py(px(8.0))
                            .gap(px(8.0))
                            .items_center()
                            .border_t_1()
                            .border_color(cx.theme().border.opacity(0.4))
                            // ◀ History (navigate recorded frames backward)
                            .child(
                                div()
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .text_size(px(11.0))
                                    .rounded(px(4.0))
                                    .bg(if can_back {
                                        cx.theme().secondary
                                    } else {
                                        cx.theme().muted.opacity(0.3)
                                    })
                                    .text_color(if can_back {
                                        cx.theme().secondary_foreground
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .cursor_pointer()
                                    .when(can_back, |el| {
                                        el.on_mouse_down(gpui::MouseButton::Left, {
                                            let pe = pe_back.clone();
                                            move |_, _, cx| {
                                                pe.update(cx, |canvas, cx| {
                                                    canvas.debug_step_backward(cx);
                                                });
                                            }
                                        })
                                    })
                                    .child("◀ History"),
                            )
                            // ▶ Step — advances the executor by one graph node.
                            // Only active when execution is suspended (is_paused).
                            .child(
                                div()
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .text_size(px(11.0))
                                    .rounded(px(4.0))
                                    .bg(if can_fwd {
                                        gpui::rgba(0x1A3A4AFF) // blue tint to distinguish from Continue
                                    } else {
                                        gpui::rgba(0x22222233)
                                    })
                                    .text_color(if can_fwd {
                                        gpui::rgba(0x88CCFFFF)
                                    } else {
                                        gpui::rgba(0x66666688)
                                    })
                                    .cursor_pointer()
                                    .when(can_fwd, |el| {
                                        el.on_mouse_down(gpui::MouseButton::Left, {
                                            let pe = pe_fwd.clone();
                                            move |_, _, cx| {
                                                pe.update(cx, |canvas, cx| {
                                                    canvas.debug_step_forward(cx);
                                                });
                                            }
                                        })
                                    })
                                    .child("Step →"),
                            )
                            // ▶▶ Continue (only if paused)
                            .when(is_paused, |el| {
                                el.child(
                                    div()
                                        .px(px(10.0))
                                        .py(px(4.0))
                                        .text_size(px(11.0))
                                        .rounded(px(4.0))
                                        .bg(gpui::rgba(0x1A4A1AFF))
                                        .text_color(gpui::rgba(0x88FF88FF))
                                        .cursor_pointer()
                                        .on_mouse_down(gpui::MouseButton::Left, {
                                            let pe = pe_continue.clone();
                                            move |_, _, cx| {
                                                pe.update(cx, |canvas, cx| {
                                                    canvas.debug_continue(cx);
                                                });
                                            }
                                        })
                                        .child("▶ Continue"),
                                )
                            })
                            // flex spacer
                            .child(div().flex_1())
                            // ■ Stop
                            .child(
                                div()
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .text_size(px(11.0))
                                    .rounded(px(4.0))
                                    .bg(gpui::rgba(0x4A1A1AFF))
                                    .text_color(gpui::rgba(0xFF8888FF))
                                    .cursor_pointer()
                                    .on_mouse_down(gpui::MouseButton::Left, {
                                        move |_, _, cx| {
                                            pe_stop.update(cx, |canvas, cx| {
                                                canvas.debug_stop(cx);
                                            });
                                        }
                                    })
                                    .child("■ Stop"),
                            ),
                    ),
            );

        hud.into_any_element()
    }

    // ── Node context menu ─────────────────────────────────────────────────────

    fn render_node_context_menu(
        canvas: &GraphCanvasPanel,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        let Some((ref node_id, pos)) = canvas.node_context_menu else {
            return div().into_any_element();
        };
        let node_id = node_id.clone();
        let has_bp = canvas.has_breakpoint(&node_id);
        let bp_label = if has_bp {
            "Remove Breakpoint"
        } else {
            "Add Breakpoint  ⏹"
        };

        let pe = cx.entity().clone();
        let pe2 = pe.clone();
        let pe3 = pe.clone();
        let pe4 = pe.clone();
        let nid_dup = node_id.clone();
        let nid_copy = node_id.clone();
        let nid_del = node_id.clone();
        let nid_bp = node_id.clone();

        deferred(
            anchored()
                .position(pos)
                .snap_to_window_with_margin(px(8.0))
                .anchor(gpui::Corner::TopLeft)
                .child(
                    div()
                        .occlude()
                        .w(px(200.0))
                        .bg(cx.theme().popover)
                        .border_1()
                        .border_color(cx.theme().border)
                        .shadow_lg()
                        .rounded(px(6.0))
                        .py(px(4.0))
                        // ── Breakpoint section ─────────────────────────────────
                        .child(Self::menu_item_colored(
                            bp_label,
                            if has_bp {
                                gpui::rgba(0xFF9999FF)
                            } else {
                                gpui::rgba(0xFF6666FF)
                            },
                            cx,
                            {
                                let pe = pe4.clone();
                                move |_, _, cx| {
                                    pe.update(cx, |canvas, cx| {
                                        canvas.toggle_breakpoint(nid_bp.clone(), cx);
                                        canvas.node_context_menu = None;
                                        cx.notify();
                                    });
                                }
                            },
                        ))
                        .child(Self::menu_divider(cx))
                        // ── Standard edit actions ──────────────────────────────
                        .child(Self::menu_item("Duplicate Node", cx, {
                            let pe = pe.clone();
                            move |_, _, cx| {
                                pe.update(cx, |canvas, cx| {
                                    canvas.duplicate_node(nid_dup.clone(), cx);
                                    canvas.node_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .child(Self::menu_item("Copy Node", cx, {
                            let pe = pe2.clone();
                            move |_, _, cx| {
                                pe.update(cx, |canvas, cx| {
                                    canvas.copy_node(nid_copy.clone(), cx);
                                    canvas.node_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .child(Self::menu_divider(cx))
                        .child(Self::menu_item("Delete Node", cx, {
                            let pe = pe3.clone();
                            move |_, _, cx| {
                                pe.update(cx, |canvas, cx| {
                                    canvas.delete_node(nid_del.clone(), cx);
                                    canvas.node_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .on_mouse_down_out(move |_, _, cx| {
                            pe.update(cx, |canvas, cx| {
                                canvas.node_context_menu = None;
                                cx.notify();
                            });
                        }),
                ),
        )
        .with_priority(2)
        .into_any_element()
    }

    // ── Pin context menu ──────────────────────────────────────────────────────

    fn render_pin_context_menu(
        canvas: &GraphCanvasPanel,
        cx: &mut Context<GraphCanvasPanel>,
    ) -> AnyElement {
        let Some((ref node_id, ref pin_id, pos)) = canvas.pin_context_menu else {
            return div().into_any_element();
        };
        let node_id = node_id.clone();
        let pin_id = pin_id.clone();
        let pe = cx.entity().clone();
        let pe2 = pe.clone();

        deferred(
            anchored()
                .position(pos)
                .snap_to_window_with_margin(px(8.0))
                .anchor(gpui::Corner::TopLeft)
                .child(
                    div()
                        .occlude()
                        .w(px(180.0))
                        .bg(cx.theme().popover)
                        .border_1()
                        .border_color(cx.theme().border)
                        .shadow_lg()
                        .rounded(px(6.0))
                        .py(px(4.0))
                        .child(Self::menu_item("Disconnect Pin", cx, {
                            let pe = pe.clone();
                            move |_, _, cx| {
                                pe.update(cx, |canvas, cx| {
                                    canvas.disconnect_pin(node_id.clone(), pin_id.clone(), cx);
                                    canvas.pin_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .on_mouse_down_out(move |_, _, cx| {
                            pe2.update(cx, |canvas, cx| {
                                canvas.pin_context_menu = None;
                                cx.notify();
                            });
                        }),
                ),
        )
        .with_priority(2)
        .into_any_element()
    }

    // ── Shared menu primitives ────────────────────────────────────────────────

    fn menu_item(
        label: &str,
        cx: &mut Context<GraphCanvasPanel>,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        div()
            .px(px(12.0))
            .py(px(6.0))
            .text_sm()
            .text_color(cx.theme().popover_foreground)
            .cursor_pointer()
            .hover(|s| s.bg(cx.theme().accent.opacity(0.12)))
            .on_mouse_down(gpui::MouseButton::Left, handler)
            .child(label.to_string())
    }

    fn menu_item_colored(
        label: &str,
        color: gpui::Rgba,
        cx: &mut Context<GraphCanvasPanel>,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        div()
            .px(px(12.0))
            .py(px(6.0))
            .text_sm()
            .text_color(color)
            .cursor_pointer()
            .hover(|s| s.bg(gpui::rgba(0xFF000020)))
            .on_mouse_down(gpui::MouseButton::Left, handler)
            .child(label.to_string())
    }

    fn menu_divider(cx: &mut Context<GraphCanvasPanel>) -> impl IntoElement {
        div()
            .my(px(4.0))
            .mx(px(8.0))
            .h(px(1.0))
            .bg(cx.theme().border)
    }
}
