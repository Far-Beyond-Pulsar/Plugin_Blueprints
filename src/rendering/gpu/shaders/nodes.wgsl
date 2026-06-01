struct GraphUniforms {
    pan:      vec2<f32>,
    zoom:     f32,
    time:     f32,
    viewport: vec2<f32>,
    _pad1:    vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: GraphUniforms;

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
    @builtin(position) pos:           vec4<f32>,
    @location(0)       uv:            vec2<f32>,
    @location(1)       size_px:       vec2<f32>,
    @location(2)       header_color:  vec4<f32>,
    @location(3)       body_color:    vec4<f32>,
    @location(4)       border_color:  vec4<f32>,
    @location(5)       sep_color:     vec4<f32>,
    @location(6)       header_h_frac: f32,
    @location(7)       corner_r_px:   f32,
    @location(8)       flags:         u32,
}

fn graph_to_screen(p: vec2<f32>) -> vec2<f32> {
    return (p + u.pan) * u.zoom;
}

fn screen_to_ndc(p: vec2<f32>) -> vec2<f32> {
    return vec2(p.x / u.viewport.x * 2.0 - 1.0, -(p.y / u.viewport.y * 2.0 - 1.0));
}

fn sdf_rrect(p: vec2<f32>, size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p - size * 0.5) - size * 0.5 + vec2(r);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

var<private> CX: array<f32, 6> = array<f32, 6>(0.0, 1.0, 0.0, 0.0, 1.0, 1.0);
var<private> CY: array<f32, 6> = array<f32, 6>(0.0, 0.0, 1.0, 1.0, 0.0, 1.0);

@vertex
fn vs_main(inst: NodeInst, @builtin(vertex_index) vi: u32) -> VOut {
    let uv = vec2(CX[vi], CY[vi]);
    let graph_pos = inst.pos + uv * inst.size;
    let scr = graph_to_screen(graph_pos);

    var o: VOut;
    o.pos = vec4(screen_to_ndc(scr), 0.0, 1.0);
    o.uv = uv;
    o.size_px = inst.size * u.zoom;
    o.header_color = inst.header_color;
    o.body_color = inst.body_color;
    o.border_color = inst.border_color;
    o.sep_color = inst.sep_color;
    o.header_h_frac = inst.header_h_frac;
    o.corner_r_px = inst.corner_r * u.zoom;
    o.flags = inst.flags;
    return o;
}

const SEP_PX: f32 = 1.0;
const BORDER_PX: f32 = 1.0;
const RUNNING_BORDER_PX: f32 = 2.2;
const SELECT_GLOW_PX: f32 = 4.0;

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let is_reroute = (in.flags & 1u) != 0u;
    let is_selected = (in.flags & 2u) != 0u;
    let is_running = (in.flags & 4u) != 0u;

    let local_px = in.uv * in.size_px;
    let d = sdf_rrect(local_px, in.size_px, in.corner_r_px);
    let aa = 1.0 - smoothstep(-0.5, 0.5, d);
    if aa <= 0.0 { discard; }

    if is_reroute {
        let edge = smoothstep(-BORDER_PX - 0.5, -BORDER_PX + 0.5, d) * smoothstep(-0.5, 0.5, -d);
        let dot_col = mix(in.body_color, in.border_color, edge);
        return vec4(dot_col.rgb, dot_col.a * aa);
    }

    let y = local_px.y;
    let header_h_px = in.header_h_frac * in.size_px.y;
    let sep_top = header_h_px;
    let sep_bottom = sep_top + SEP_PX;

    var base = in.body_color;
    if y < sep_top {
        base = in.header_color;
    } else if y < sep_bottom {
        base = in.sep_color;
    }

    let border = smoothstep(-BORDER_PX - 0.5, -BORDER_PX + 0.5, d) * smoothstep(-0.5, 0.5, -d);
    base = mix(base, in.border_color, border);

    if is_running {
        let pulse = 0.55 + 0.45 * sin(u.time * 6.0);
        let run_border = vec4(mix(in.sep_color.rgb, vec3(1.0), 0.18), 1.0);
        let thick_border =
            smoothstep(-RUNNING_BORDER_PX - 0.5, -RUNNING_BORDER_PX + 0.5, d)
            * smoothstep(-0.5, 0.5, -d);
        base = mix(base, run_border, thick_border * (0.72 + pulse * 0.28));
    }

    if is_selected {
        let sel_border = vec4(mix(in.border_color.rgb, vec3(1.0), 0.22), in.border_color.a);
        base = mix(base, sel_border, border);
        let glow = smoothstep(SELECT_GLOW_PX, 0.0, d) * smoothstep(-0.5, 0.5, d);
        base = vec4(base.rgb + sel_border.rgb * glow * 0.25, base.a);
    }

    return vec4(base.rgb, base.a * aa);
}
