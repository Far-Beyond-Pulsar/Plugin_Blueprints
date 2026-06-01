// Instanced node body renderer.
// One draw-instance per node; vertex shader expands to 6 verts covering the node rect.
// Fragment shader does SDF rounded-rect, paints header / separator / body regions,
// draws a 1-px border and an optional selection glow.

struct GraphUniforms {
    pan:      vec2<f32>,
    zoom:     f32,
    time:     f32,
    viewport: vec2<f32>,
    _pad1:    vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: GraphUniforms;

// ── instance attributes ────────────────────────────────────────────────────────
struct NodeInst {
    @location(0) pos:           vec2<f32>,
    @location(1) size:          vec2<f32>,
    @location(2) header_color:  vec4<f32>,
    @location(3) body_color:    vec4<f32>,
    @location(4) border_color:  vec4<f32>,
    @location(5) sep_color:     vec4<f32>,
    @location(6) header_h_frac: f32,
    @location(7) corner_r:      f32,
    @location(8) flags:         u32,
    @location(9) _pad:          u32,
}

struct VOut {
    @builtin(position) pos:          vec4<f32>,
    @location(0)       uv:           vec2<f32>, // 0..1 within node
    @location(1)       size_px:      vec2<f32>, // node size in screen pixels
    @location(2)       header_color: vec4<f32>,
    @location(3)       body_color:   vec4<f32>,
    @location(4)       border_color: vec4<f32>,
    @location(5)       sep_color:    vec4<f32>,
    @location(6)       header_h_frac: f32,
    @location(7)       corner_r_px:  f32,
    @location(8)       flags:        u32,
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn graph_to_screen(p: vec2<f32>) -> vec2<f32> {
    return (p + u.pan) * u.zoom;
}

fn screen_to_ndc(p: vec2<f32>) -> vec2<f32> {
    return vec2(
        p.x / u.viewport.x * 2.0 - 1.0,
       -(p.y / u.viewport.y * 2.0 - 1.0),
    );
}

// Signed-distance to a rounded rectangle.
// p = pixel position within node (0..size_px)
// d < 0 = inside, d > 0 = outside.
fn sdf_rrect(p: vec2<f32>, size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p - size * 0.5) - size * 0.5 + vec2(r);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Slight top-edge highlight to give headers a subtle bevel.
fn header_bevel(uv_y: f32, header_h_frac: f32) -> f32 {
    let rel = uv_y / header_h_frac; // 0=top of header 1=bottom
    return smoothstep(0.35, 0.0, rel) * 0.12;
}

// ── vertex ────────────────────────────────────────────────────────────────────

// 6-vertex quad — corner mapping:
//   0: TL  1: TR  2: BL  3: BL  4: TR  5: BR
var<private> CX: array<f32, 6> = array<f32, 6>(0.0, 1.0, 0.0, 0.0, 1.0, 1.0);
var<private> CY: array<f32, 6> = array<f32, 6>(0.0, 0.0, 1.0, 1.0, 0.0, 1.0);

@vertex
fn vs_main(inst: NodeInst, @builtin(vertex_index) vi: u32) -> VOut {
    let uv       = vec2(CX[vi], CY[vi]);
    let graph_pos = inst.pos + uv * inst.size;
    let scr       = graph_to_screen(graph_pos);

    var o: VOut;
    o.pos           = vec4(screen_to_ndc(scr), 0.0, 1.0);
    o.uv            = uv;
    o.size_px       = inst.size * u.zoom;
    o.header_color  = inst.header_color;
    o.body_color    = inst.body_color;
    o.border_color  = inst.border_color;
    o.sep_color     = inst.sep_color;
    o.header_h_frac = inst.header_h_frac;
    o.corner_r_px   = inst.corner_r * u.zoom;
    o.flags         = inst.flags;
    return o;
}

// ── fragment ──────────────────────────────────────────────────────────────────

const SEP_PX: f32 = 1.5;  // separator bar height (screen px)
const BORDER_PX: f32 = 1.0;
const GLOW_OUTER_PX: f32 = 7.5;
const GLOW_INNER_PX: f32 = 1.4;

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let is_reroute  = (in.flags & 1u) != 0u;
    let is_selected = (in.flags & 2u) != 0u;

    // Pixel position within node quad
    let local_px = in.uv * in.size_px;
    let d = sdf_rrect(local_px, in.size_px, in.corner_r_px);

    // Discard outside with a tiny anti-alias fringe
    let alpha = 1.0 - smoothstep(-0.5, 0.5, d);
    if alpha <= 0.0 { discard; }

    // Reroute: simple filled circle/dot
    if is_reroute {
        var col = in.header_color;
        if is_selected {
            let border_a = smoothstep(-BORDER_PX - 0.5, -BORDER_PX + 0.5, d) * (1.0 - smoothstep(-0.5, 0.5, d));
            col = mix(col, in.border_color, border_a);
        }
        return vec4(col.rgb, col.a * alpha);
    }

    // ── Region selection ───────────────────────────────────────────────────────
    let header_h_px = in.header_h_frac * in.size_px.y;
    let sep_top     = header_h_px;
    let sep_bottom  = sep_top + SEP_PX;
    let y           = local_px.y;

    var base: vec4<f32>;
    if y < sep_top {
        // Header region — apply subtle bevel at top
        let bevel = header_bevel(in.uv.y, in.header_h_frac);
        base = vec4(in.header_color.rgb + bevel, in.header_color.a);
    } else if y < sep_bottom {
        // Separator bar
        base = in.sep_color;
    } else {
        // Body
        base = in.body_color;
    }

    // Clean futuristic finish: subtle vertical falloff + rim tint from separator color.
    let grad = mix(1.06, 0.93, in.uv.y);
    base = vec4(base.rgb * grad, base.a);
    let edge_uv = min(min(in.uv.x, 1.0 - in.uv.x), min(in.uv.y, 1.0 - in.uv.y));
    let rim = smoothstep(0.08, 0.0, edge_uv) * 0.09;
    base = vec4(mix(base.rgb, base.rgb + in.sep_color.rgb * 0.32, rim), base.a);

    // ── Border (1px inside edge) ───────────────────────────────────────────────
    let border_a = smoothstep(-BORDER_PX - 0.5, -BORDER_PX + 0.5, d)
                 * smoothstep(-0.5, 0.5, -d);   // only inside
    base = mix(base, in.border_color, border_a * (1.0 - f32(is_selected)));

    // ── Selection glow (outside + bright inner border) ────────────────────────
    if is_selected {
        // Outer glow
        let glow_a = smoothstep(GLOW_OUTER_PX, 0.0, d) * smoothstep(-0.5, 0.5, d);
        let glow_c = vec4(in.border_color.rgb, in.border_color.a * glow_a * 0.4);
        base = mix(base, glow_c, glow_a * 0.4);

        // Sharp inner border
        let inner_a = smoothstep(-GLOW_INNER_PX - 0.5, -GLOW_INNER_PX + 0.5, d)
                    * smoothstep(-0.5, 0.5, -d);
        base = mix(base, in.border_color, inner_a);
    }

    return vec4(base.rgb, base.a * alpha);
}
