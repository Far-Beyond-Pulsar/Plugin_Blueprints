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
use ui::graph::DataType;
use ui::ActiveTheme;
use ui::PixelsExt;

use crate::core::graph::BlueprintGraph;
use crate::core::types::{BlueprintNode, Connection, NodeType, Pin};
use crate::editor::panel::BlueprintEditorPanel;
use crate::features::connections::operations::ConnectionDrag;
use crate::rendering::gpu::{
    BpRenderer, GraphUniforms, NodeInstance, PinInstance, TextRenderer, WireVertex,
};
use crate::rendering::layout;

// shared with hit-testing in input.rs
pub const HEADER_H:  f32 = layout::HEADER_H;
pub const SEP_H:     f32 = layout::SEP_H;
pub const BODY_PAD:  f32 = layout::BODY_PAD;
pub const PIN_ROW_H: f32 = layout::PIN_ROW_H;
pub const PIN_GAP:   f32 = layout::PIN_GAP;
pub const PIN_SIZE:  f32 = layout::PIN_SIZE;

const WIRE_SEGS:      usize = 32;
const WIRE_THICKNESS: f32   = 2.8;
const HEADER_FONT:    f32   = 12.5;
const PIN_FONT:       f32   = 10.5;
const HEADER_PAD_X:   f32   = 9.0;

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
        panel:      &BlueprintEditorPanel,
    ) -> Point<Pixels> {
        let o = *panel.canvas_origin.borrow();
        Point::new(window_pos.x - px(o.x), window_pos.y - px(o.y))
    }

    pub fn window_to_graph_element_pos_for_view(
        window_pos: Point<Pixels>,
        panel:      &BlueprintEditorPanel,
        _view_id:   &str,
    ) -> Point<Pixels> {
        Self::window_to_graph_element_pos(window_pos, panel)
    }

    pub fn snap_to_grid(pos: Point<f32>) -> Point<f32> {
        let g = layout::GRID_SNAP;
        Point::new((pos.x / g).round() * g, (pos.y / g).round() * g)
    }

    pub fn pin_canvas_pos(
        node: &BlueprintNode, is_input: bool, row: usize, graph: &BlueprintGraph,
    ) -> Point<f32> {
        let zoom = graph.zoom_level;
        let scr  = Self::graph_to_screen_pos(node.position, graph);
        let py   = scr.y
            + (HEADER_H + SEP_H + BODY_PAD) * zoom
            + row as f32 * (PIN_ROW_H + PIN_GAP) * zoom
            + PIN_ROW_H * 0.5 * zoom;
        let px_  = if is_input { scr.x + BODY_PAD * zoom } else { scr.x + (node.size.width - BODY_PAD) * zoom };
        Point::new(px_, py)
    }

    pub fn calculate_pin_position(
        node: &BlueprintNode, pin_id: &str, is_input: bool, graph: &BlueprintGraph,
    ) -> Option<Point<f32>> {
        if node.node_type == NodeType::Reroute {
            return Some(Self::graph_to_screen_pos(node.position, graph));
        }
        let row = if is_input {
            node.inputs.iter().position(|p| p.id == pin_id)?
        } else {
            node.outputs.iter().position(|p| p.id == pin_id)?
        };
        Some(Self::pin_canvas_pos(node, is_input, row, graph))
    }

    pub fn calculate_pin_position_graph_space(
        node: &BlueprintNode, is_input: bool, row: usize, _graph: &BlueprintGraph,
    ) -> Point<f32> {
        let py = node.position.y + HEADER_H + SEP_H + BODY_PAD
            + row as f32 * (PIN_ROW_H + PIN_GAP) + PIN_ROW_H * 0.5;
        let px_ = if is_input { node.position.x + BODY_PAD } else { node.position.x + node.size.width - BODY_PAD };
        Point::new(px_, py)
    }

    /// Backwards-compat: is this node inside the viewport?
    pub fn is_node_visible_simple(node: &BlueprintNode, graph: &BlueprintGraph) -> bool {
        let pad = 260.0 / graph.zoom_level.max(0.05);
        let vl = -graph.pan_offset.x - pad;
        let vt = -graph.pan_offset.y - pad;
        let vr = -graph.pan_offset.x + 3840.0 / graph.zoom_level + pad;
        let vb = -graph.pan_offset.y + 2160.0 / graph.zoom_level + pad;
        !(node.position.x > vr || node.position.x + node.size.width  < vl
        || node.position.y > vb || node.position.y + node.size.height < vt)
    }

    pub fn is_connection_visible_simple(conn: &Connection, graph: &BlueprintGraph) -> bool {
        let from = graph.nodes.iter().find(|n| n.id == conn.source_node);
        let to   = graph.nodes.iter().find(|n| n.id == conn.target_node);
        match (from, to) {
            (Some(f), Some(t)) => Self::is_node_visible_simple(f, graph) || Self::is_node_visible_simple(t, graph),
            _ => false,
        }
    }

    pub fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
        let hex = hex.trim_start_matches('#');
        let p   = |s: &str| u8::from_str_radix(s, 16).ok().map(|v| v as f32 / 255.0);
        if hex.len() == 6 {
            Some(gpui::Hsla::from(gpui::Rgba { r: p(&hex[0..2])?, g: p(&hex[2..4])?, b: p(&hex[4..6])?, a: 1.0 }))
        } else if hex.len() == 8 {
            Some(gpui::Hsla::from(gpui::Rgba { r: p(&hex[0..2])?, g: p(&hex[2..4])?, b: p(&hex[4..6])?, a: p(&hex[6..8])? }))
        } else { None }
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
        NodeType::Event         => [0.72, 0.12, 0.10, 1.0],
        NodeType::Logic         => [0.13, 0.38, 0.78, 1.0],
        NodeType::Math          => [0.16, 0.62, 0.28, 1.0],
        NodeType::Object        => [0.78, 0.42, 0.08, 1.0],
        NodeType::Reroute       => [0.40, 0.40, 0.42, 1.0],
        NodeType::MacroEntry
        | NodeType::MacroExit   => [0.44, 0.18, 0.72, 1.0],
        NodeType::MacroInstance => [0.32, 0.12, 0.52, 1.0],
    }
}

fn darken(c: [f32;4], f: f32) -> [f32;4] { [c[0]*f, c[1]*f, c[2]*f, c[3]] }
fn lighten(c: [f32;4], f: f32) -> [f32;4] {
    [1.0-(1.0-c[0])*f, 1.0-(1.0-c[1])*f, 1.0-(1.0-c[2])*f, c[3]]
}
fn pin_color(dt: &DataType) -> [f32;4] {
    let ps = dt.generate_pin_style();
    [ps.color.r, ps.color.g, ps.color.b, ps.color.a]
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
    let py = node.position.y + HEADER_H + SEP_H + BODY_PAD
        + row as f32 * (PIN_ROW_H + PIN_GAP) + PIN_ROW_H * 0.5;
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
    let row = if is_input {
        node.inputs.iter().position(|p| p.id == pin_id)?
    } else {
        node.outputs.iter().position(|p| p.id == pin_id)?
    };
    Some(pin_gpos_row(node, is_input, row))
}

fn bezier(p0:(f32,f32),p1:(f32,f32),p2:(f32,f32),p3:(f32,f32),t:f32)->(f32,f32){
    let u=1.0-t; let a=u*u*u; let b=3.0*u*u*t; let c=3.0*u*t*t; let d=t*t*t;
    (a*p0.0+b*p1.0+c*p2.0+d*p3.0, a*p0.1+b*p1.1+c*p2.1+d*p3.1)
}

/// Tessellate a bezier wire into thick-quad segments — positions in GRAPH SPACE.
/// half_thick is in graph units (not multiplied by zoom — shader handles scale).
fn tessellate_wire(from:(f32,f32), to:(f32,f32), color:[f32;4], half_thick:f32) -> Vec<WireVertex> {
    let hd  = (to.0-from.0).abs();
    // Control point offset in graph units — keeps wire shape consistent at all zoom levels.
    let ctl = (hd*0.45).max(55.0).min(220.0);
    let c1  = (from.0+ctl, from.1);
    let c2  = (to.0-ctl,   to.1);
    let mut out = Vec::with_capacity(WIRE_SEGS*6);
    let mut prev = from;
    for i in 1..=WIRE_SEGS {
        let t = i as f32/WIRE_SEGS as f32;
        let cur = bezier(from,c1,c2,to,t);
        let dx=cur.0-prev.0; let dy=cur.1-prev.1;
        let len=(dx*dx+dy*dy).sqrt();
        let (nx,ny)=if len>0.0{(-dy/len*half_thick,dx/len*half_thick)}else{(0.0,half_thick)};
        let v0=(i-1) as f32/WIRE_SEGS as f32;
        let v1=i as f32/WIRE_SEGS as f32;
        out.push(WireVertex{pos:[prev.0+nx,prev.1+ny],uv:[0.0,v0],color});
        out.push(WireVertex{pos:[prev.0-nx,prev.1-ny],uv:[1.0,v0],color});
        out.push(WireVertex{pos:[cur.0+nx, cur.1+ny], uv:[0.0,v1],color});
        out.push(WireVertex{pos:[cur.0+nx, cur.1+ny], uv:[0.0,v1],color});
        out.push(WireVertex{pos:[prev.0-nx,prev.1-ny],uv:[1.0,v0],color});
        out.push(WireVertex{pos:[cur.0-nx, cur.1-ny], uv:[1.0,v1],color});
        prev=cur;
    }
    out
}

/// Tessellate a straight line segment — no bezier, no S-curves.
/// Used for the selection box where all edges must be perfectly straight.
fn tessellate_line(from:(f32,f32), to:(f32,f32), color:[f32;4], half_thick:f32) -> Vec<WireVertex> {
    let dx = to.0-from.0; let dy = to.1-from.1;
    let len = (dx*dx+dy*dy).sqrt();
    if len < 0.0001 { return vec![]; }
    let (nx,ny) = (-dy/len*half_thick, dx/len*half_thick);
    vec![
        WireVertex{pos:[from.0+nx,from.1+ny],uv:[0.0,0.0],color},
        WireVertex{pos:[from.0-nx,from.1-ny],uv:[1.0,0.0],color},
        WireVertex{pos:[to.0+nx,  to.1+ny  ],uv:[0.0,1.0],color},
        WireVertex{pos:[to.0+nx,  to.1+ny  ],uv:[0.0,1.0],color},
        WireVertex{pos:[from.0-nx,from.1-ny],uv:[1.0,0.0],color},
        WireVertex{pos:[to.0-nx,  to.1-ny  ],uv:[1.0,1.0],color},
    ]
}

// ─── main render ──────────────────────────────────────────────────────────────

type TextCall = (String, f32, f32, f32, [f32;4], bool); // (text, x, y, size, color, center)

impl NodeGraphRenderer {
    pub fn render(
        panel:   &mut BlueprintEditorPanel,
        view_id: &str,
        cx:      &mut Context<BlueprintEditorPanel>,
    ) -> impl IntoElement {
        let panel_entity = cx.entity().clone();
        let zoom   = panel.graph.zoom_level;
        let pan_x  = panel.graph.pan_offset.x;
        let pan_y  = panel.graph.pan_offset.y;

        // viewport culling
        let (vw, vh) = panel.graph_element_bounds
            .map(|b|(b.size.width.as_f32().max(1.0),b.size.height.as_f32().max(1.0)))
            .unwrap_or((3840.0,2160.0));
        let pad = (260.0/zoom.max(0.05)).max(120.0);
        let (vl,vt,vr,vb) = (-pan_x-pad, -pan_y-pad, -pan_x+vw/zoom+pad, -pan_y+vh/zoom+pad);
        let visible = |n:&BlueprintNode|{
            !(n.position.x>vr||n.position.x+n.size.width<vl
            ||n.position.y>vb||n.position.y+n.size.height<vt)
        };

        let dragging_conn   = panel.dragging_connection.clone();
        let selected_nodes  = panel.graph.selected_nodes.clone();

        let mut node_instances: Vec<NodeInstance> = Vec::new();
        let mut pin_instances:  Vec<PinInstance>  = Vec::new();
        let mut text_calls:     Vec<TextCall>      = Vec::new();

        for node in &panel.graph.nodes {
            if !visible(node) { continue; }

            let is_sel    = selected_nodes.contains(&node.id);
            let is_reroute = node.node_type == NodeType::Reroute;
            let cat  = category_color(node);
            let hdr  = darken(cat, 0.60);
            let body = [0.07, 0.07, 0.075, 1.0_f32];
            let bord = if is_sel { lighten(cat, 0.42) } else { [0.18, 0.18, 0.19, 1.0] };
            let sep  = lighten(cat, 0.70);

            let max_rows = node.inputs.len().max(node.outputs.len()).max(1);
            let gw = layout::snap_to_grid(node.size.width);
            let gh = layout::snap_to_grid(layout::node_height_for_pin_rows(max_rows));
            let hdr_frac = (HEADER_H + SEP_H) / gh;
            let flags = (is_reroute as u32) | ((is_sel as u32) << 1);

            node_instances.push(NodeInstance {
                pos:           [node.position.x, node.position.y],
                size:          [gw, gh],
                header_color:  hdr,
                body_color:    body,
                border_color:  bord,
                sep_color:     sep,
                header_h_frac: hdr_frac,
                corner_r:      6.0 / zoom,
                flags,
                _pad:          0,
            });

            // header title — text is in screen space (text shader skips graph transform)
            if !is_reroute {
                let scr = Self::graph_to_screen_pos(node.position, &panel.graph);
                text_calls.push((
                    node.title.clone(),
                    scr.x + HEADER_PAD_X * zoom,
                    scr.y + HEADER_H * zoom * 0.5 + HEADER_FONT * zoom * 0.35,
                    HEADER_FONT * zoom,
                    [1.0, 1.0, 1.0, 0.95],
                    false,
                ));
            }

            // pins — centers in GRAPH SPACE (GPU shader applies pan+zoom)
            for (is_input, pins) in [
                (true,  node.inputs.as_slice()),
                (false, node.outputs.as_slice()),
            ] {
                for (i, pin) in pins.iter().enumerate() {
                    let (cgx, cgy) = pin_gpos_row(node, is_input, i);
                    let pc  = pin_color(&pin.data_type);
                    let exe = pin.data_type == DataType::Execution;
                    let compat = dragging_conn.as_ref().map_or(false, |d|{
                        is_input && node.id != d.source_node
                            && pin.data_type.is_compatible_with(&d.source_pin_type)
                    });
                    pin_instances.push(PinInstance {
                        center:     [cgx, cgy],  // graph space — no pan/zoom applied
                        size:       PIN_SIZE,
                        _pad0:      0.0,
                        color:      pc,
                        kind:       exe as u32,
                        is_input:   is_input as u32,
                        compatible: compat as u32,
                        _pad1:      0,
                    });
                    // Pin labels — convert graph pos to screen for text renderer
                    if !pin.name.is_empty() && !is_reroute {
                        let scr_x = (cgx + pan_x) * zoom;
                        let scr_y = (cgy + pan_y) * zoom;
                        let lx = if is_input { scr_x + (PIN_SIZE*zoom*0.5 + 5.0) }
                                 else        { scr_x - (PIN_SIZE*zoom*0.5 + 5.0) };
                        text_calls.push((
                            pin.name.clone(), lx,
                            scr_y + PIN_FONT * zoom * 0.35,
                            PIN_FONT * zoom,
                            [0.88, 0.88, 0.90, 1.0],
                            !is_input,
                        ));
                    }
                }
            }
        }

        // wires
        let mut wire_verts: Vec<WireVertex> = Vec::new();
        let half_thick = WIRE_THICKNESS * zoom * 0.5;
        let node_map: std::collections::HashMap<&str,&BlueprintNode> =
            panel.graph.nodes.iter().map(|n|(n.id.as_str(),n)).collect();
        let vis_ids: std::collections::HashSet<&str> =
            panel.graph.nodes.iter().filter(|n|visible(n)).map(|n|n.id.as_str()).collect();

        for conn in &panel.graph.connections {
            if !vis_ids.contains(conn.source_node.as_str())
            && !vis_ids.contains(conn.target_node.as_str()) { continue; }
            let (fn_, tn) = (node_map.get(conn.source_node.as_str()),
                             node_map.get(conn.target_node.as_str()));
            if let (Some(fn_), Some(tn)) = (fn_, tn) {
                let fc = fn_.outputs.iter().find(|p|p.id==conn.source_pin)
                    .map_or([0.8,0.8,0.8,1.0], |p|pin_color(&p.data_type));
                if let (Some(fp),Some(tp)) = (
                    Self::calculate_pin_position(fn_, &conn.source_pin, false, &panel.graph),
                    Self::calculate_pin_position(tn,  &conn.target_pin, true,  &panel.graph),
                ) {
                    wire_verts.extend(tessellate_wire((fp.x,fp.y),(tp.x,tp.y),fc,half_thick));
                }
            }
        }

        // drag wire
        if let Some(ref drag) = panel.dragging_connection.clone() {
            if let Some(fn_) = node_map.get(drag.source_node.as_str()) {
                if let Some(fp) = Self::calculate_pin_position(fn_,&drag.source_pin,false,&panel.graph) {
                    let dc = pin_color(&drag.source_pin_type);
                    let tp = drag.current_mouse_pos;
                    wire_verts.extend(tessellate_wire(
                        (fp.x,fp.y),(tp.x,tp.y),
                        [dc[0],dc[1],dc[2],0.75],
                        half_thick*0.85,
                    ));
                }
            }
        }

        // selection box outline (as wire rect)
        if let (Some(start), Some(end)) = (panel.selection_start, panel.selection_end) {
            let sp = Self::graph_to_screen_pos(start, &panel.graph);
            let ep = Self::graph_to_screen_pos(end,   &panel.graph);
            let (sx,sy,ex,ey) = (sp.x,sp.y,ep.x,ep.y);
            let sc = [0.30,0.55,0.90,0.80_f32];
            let ht = 0.85_f32;
            wire_verts.extend(tessellate_wire((sx,sy),(ex,sy),sc,ht));
            wire_verts.extend(tessellate_wire((sx,ey),(ex,ey),sc,ht));
            wire_verts.extend(tessellate_wire((sx,sy),(sx,ey),sc,ht));
            wire_verts.extend(tessellate_wire((ex,sy),(ex,ey),sc,ht));
        }

        let uniforms = GraphUniforms {
            pan:      [pan_x, pan_y],
            zoom,
            _pad0:    0.0,
            viewport: [vw, vh],
            _pad1:    [0.0;2],
        };

        let focus_handle = panel.focus_handle().clone();
        let view_id = view_id.to_string();

        // ── WGPU surface display ──────────────────────────────────────────────
        // wgpu_surface() composites the GPU texture into the GPUI scene.
        // It must be present in the element tree for anything to appear.
        // On the first frame bp_surface is None so we show a dark placeholder;
        // the canvas prepaint creates the surface and requests a re-render,
        // so frame 2 immediately shows the GPU output.
        let gpu_display: AnyElement = if let Some(ref s) = panel.bp_surface {
            wgpu_surface(s.clone())
                .defer_resize_until_mouse_up(true)
                .absolute()
                .inset_0()
                .into_any_element()
        } else {
            div()
                .absolute()
                .inset_0()
                .bg(gpui::Hsla { h: 0.0, s: 0.0, l: 0.055, a: 1.0 })
                .into_any_element()
        };

        // ── Canvas: creates surface in prepaint (has window), renders in paint ─
        let driver = {
            let pe_pre = panel_entity.clone();
            let pe_paint = panel_entity.clone();
            gpui::canvas(
                // Prepaint: surface creation (first frame only).
                // Called before paint — window is available here.
                move |bounds, window, cx| {
                    // Capture element bounds for coordinate conversion
                    let ox = bounds.origin.x.as_f32();
                    let oy = bounds.origin.y.as_f32();
                    let sw = bounds.size.width.as_f32()  as u32;
                    let sh = bounds.size.height.as_f32() as u32;

                    pe_pre.update(cx, |panel, cx| {
                        *panel.canvas_origin.borrow_mut() = Point::new(ox, oy);
                        let b = gpui::Bounds {
                            origin: gpui::Point { x: px(ox), y: px(oy) },
                            size:   gpui::Size  { width: px(sw as f32), height: px(sh as f32) },
                        };
                        panel.graph_element_bounds = Some(b);

                        // Create surface on first call — triggers re-render via notify
                        if panel.bp_surface.is_none() {
                            if let Some(s) = window.create_wgpu_surface(
                                sw.max(64), sh.max(64),
                                wgpu::TextureFormat::Bgra8UnormSrgb,
                            ) {
                                panel.bp_surface = Some(s);
                                cx.notify(); // re-render to pick up wgpu_surface() element
                            }
                        }
                    });
                },
                // Paint: render GPU frame every frame.
                move |_bounds, _pre, _window, cx| {
                    pe_paint.update(cx, |panel, _| {
                        let Some(ref surface) = panel.bp_surface else { return };
                        if surface.is_resize_pending() { return; }
                        let Some((view,(w,h))) = surface.back_view_with_size() else { return };

                        let frame_uni = GraphUniforms { viewport:[w as f32, h as f32], ..uniforms };
                        panel.bp_renderer.render_frame(
                            surface.device(), surface.queue(),
                            &view, w, h, surface.format(),
                            &frame_uni,
                            &node_instances, &wire_verts, &pin_instances, &text_calls,
                        );
                        drop(view);
                        surface.swap_buffers();
                    });
                },
            )
            .absolute().inset_0().size_full()
        };

        div()
            .size_full().relative().overflow_hidden()
            .track_focus(&focus_handle)
            .key_context("BlueprintGraph")
            .child(gpu_display)   // wgpu_surface() or dark placeholder — MUST be first
            .child(driver)        // invisible canvas that drives GPU rendering
            // GPUI-only overlays (palette + context menus) sit on top

            // GPUI-only overlays (palette + context menus)
            .child(Self::render_quick_palette_overlay_inner(
                panel.quick_palette_open,
                panel.quick_palette_screen_pos,
                panel.quick_palette_view.clone(),
                panel.quick_palette_focus_pending,
                cx,
            ))
            .child(Self::render_node_context_menu(panel, cx))
            .child(Self::render_pin_context_menu(panel, cx))
            // input
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(move|panel,_,window,cx|{
                panel.focus_handle().focus(window,cx);
                if panel.editing_comment.is_some() { panel.finish_comment_editing(cx); }
                if panel.variable_drop_menu_position.is_some() {
                    panel.variable_drop_menu_position=None; cx.notify();
                }
            }))
            .on_mouse_down(gpui::MouseButton::Right,
                crate::rendering::input::on_mouse_down_right(view_id.clone(), cx))
            .on_mouse_down(gpui::MouseButton::Left,
                crate::rendering::input::on_mouse_down_left(view_id.clone(), cx))
            .on_mouse_move(crate::rendering::input::on_mouse_move(view_id.clone(), cx))
            .on_mouse_up(gpui::MouseButton::Left,
                crate::rendering::input::on_mouse_up_left(view_id.clone(), cx))
            .on_mouse_up_out(gpui::MouseButton::Left,
                crate::rendering::input::on_mouse_up_left(view_id.clone(), cx))
            .on_mouse_up(gpui::MouseButton::Right,
                crate::rendering::input::on_mouse_up_right(view_id.clone(), cx))
            .on_mouse_up_out(gpui::MouseButton::Right,
                crate::rendering::input::on_mouse_up_right(view_id.clone(), cx))
            .on_scroll_wheel(crate::rendering::input::on_scroll_wheel(view_id.clone(), cx))
            .on_key_down(crate::rendering::input::on_key_down(view_id, cx))
    }

    fn render_quick_palette_overlay_inner(
        open:           bool,
        screen_pos:     Point<Pixels>,
        palette_view:   gpui::Entity<crate::ui_components::palette_view::NodePaletteView>,
        focus_pending:  bool,
        cx:             &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        if !open { return div().into_any_element(); }
        let panel_entity = cx.entity().clone();
        deferred(
            anchored()
                .position(screen_pos)
                .snap_to_window_with_margin(px(8.0))
                .anchor(gpui::Corner::TopLeft)
                .child(
                    div().occlude()
                        .w(px(320.0)).h(px(480.0))
                        .shadow_lg().rounded(px(6.0)).overflow_hidden()
                        .border_1().border_color(cx.theme().border)
                        .child(palette_view)
                        .on_children_prepainted({
                            let pe = panel_entity.clone();
                            move |_, window, cx| {
                                pe.update(cx, |panel, cx|{
                                    if !panel.quick_palette_focus_pending { return; }
                                    let h = panel.quick_palette_view.read(cx).search_focus_handle(cx);
                                    panel.quick_palette_focus_pending = false;
                                    window.focus(&h, cx);
                                });
                            }
                        })
                        .on_mouse_down_out(move |_,_,cx|{
                            panel_entity.update(cx, |panel, cx|{
                                panel.quick_palette_open = false;
                                panel.quick_palette_focus_pending = false;
                                panel.quick_palette_connection_source = None;
                                panel.popup_palette_graph_pos = None;
                                cx.notify();
                            });
                        }),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }

    // ── Node context menu ─────────────────────────────────────────────────────

    fn render_node_context_menu(
        panel: &BlueprintEditorPanel,
        cx:    &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some((ref node_id, pos)) = panel.node_context_menu else {
            return div().into_any_element();
        };
        let node_id    = node_id.clone();
        let pe         = cx.entity().clone();
        let pe2        = pe.clone();
        let pe3        = pe.clone();
        let nid_dup    = node_id.clone();
        let nid_copy   = node_id.clone();
        let nid_del    = node_id.clone();

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
                        .child(Self::menu_item("Duplicate Node", cx, {
                            let pe = pe.clone();
                            move |_, _, cx| {
                                pe.update(cx, |panel, cx| {
                                    panel.duplicate_node(nid_dup.clone(), cx);
                                    panel.node_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .child(Self::menu_item("Copy Node", cx, {
                            let pe = pe2.clone();
                            move |_, _, cx| {
                                pe.update(cx, |panel, cx| {
                                    panel.copy_node(nid_copy.clone(), cx);
                                    panel.node_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .child(Self::menu_divider(cx))
                        .child(Self::menu_item("Delete Node", cx, {
                            let pe = pe3.clone();
                            move |_, _, cx| {
                                pe.update(cx, |panel, cx| {
                                    panel.delete_node(nid_del.clone(), cx);
                                    panel.node_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .on_mouse_down_out(move |_, _, cx| {
                            pe.update(cx, |panel, cx| {
                                panel.node_context_menu = None;
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
        panel: &BlueprintEditorPanel,
        cx:    &mut Context<BlueprintEditorPanel>,
    ) -> AnyElement {
        let Some((ref node_id, ref pin_id, pos)) = panel.pin_context_menu else {
            return div().into_any_element();
        };
        let node_id = node_id.clone();
        let pin_id  = pin_id.clone();
        let pe      = cx.entity().clone();
        let pe2     = pe.clone();

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
                                pe.update(cx, |panel, cx| {
                                    panel.disconnect_pin(node_id.clone(), pin_id.clone(), cx);
                                    panel.pin_context_menu = None;
                                    cx.notify();
                                });
                            }
                        }))
                        .on_mouse_down_out(move |_, _, cx| {
                            pe2.update(cx, |panel, cx| {
                                panel.pin_context_menu = None;
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
        label:   &str,
        cx:      &mut Context<BlueprintEditorPanel>,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        div()
            .px(px(12.0)).py(px(6.0))
            .text_sm()
            .text_color(cx.theme().popover_foreground)
            .cursor_pointer()
            .hover(|s| s.bg(cx.theme().accent.opacity(0.12)))
            .on_mouse_down(gpui::MouseButton::Left, handler)
            .child(label.to_string())
    }

    fn menu_divider(cx: &mut Context<BlueprintEditorPanel>) -> impl IntoElement {
        div()
            .my(px(4.0))
            .mx(px(8.0))
            .h(px(1.0))
            .bg(cx.theme().border)
    }
}
