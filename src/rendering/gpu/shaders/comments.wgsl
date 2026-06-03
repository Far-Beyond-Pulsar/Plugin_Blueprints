struct GraphUniforms {
    pan:      vec2<f32>,
    zoom:     f32,
    time:     f32,
    viewport: vec2<f32>,
    _pad1:    vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: GraphUniforms;

struct CommentInst {
    @location(0) pos:          vec2<f32>,
    @location(1) size:         vec2<f32>,
    @location(2) fill_color:   vec4<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) corner_r:     f32,
    @location(5) flags:        u32,
    @location(6) _pad0:        u32,
    @location(7) _pad1:        u32,
}

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0)       uv:  vec2<f32>,
    @location(1)       size_px: vec2<f32>,
    @location(2)       fill_color: vec4<f32>,
    @location(3)       border_color: vec4<f32>,
    @location(4)       corner_r_px: f32,
    @location(5)       flags: u32,
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
fn vs_main(inst: CommentInst, @builtin(vertex_index) vi: u32) -> VOut {
    let uv = vec2(CX[vi], CY[vi]);
    let graph_pos = inst.pos + uv * inst.size;
    let scr = graph_to_screen(graph_pos);

    var o: VOut;
    o.pos = vec4(screen_to_ndc(scr), 0.0, 1.0);
    o.uv = uv;
    o.size_px = inst.size * u.zoom;
    o.fill_color = inst.fill_color;
    o.border_color = inst.border_color;
    o.corner_r_px = inst.corner_r * u.zoom;
    o.flags = inst.flags;
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let d = sdf_rrect(in.uv * in.size_px, in.size_px, in.corner_r_px);
    let aa = 1.0 - smoothstep(-0.5, 0.5, d);
    if aa <= 0.0 { discard; }

    let border_px = 1.2;
    let border = smoothstep(-border_px - 0.5, -border_px + 0.5, d) * smoothstep(-0.5, 0.5, -d);
    var col = mix(in.fill_color, in.border_color, border);

    if (in.flags & 1u) != 0u {
        let selected = vec4(mix(in.border_color.rgb, vec3(1.0), 0.22), 1.0);
        let sel_border = smoothstep(-2.4 - 0.5, -2.4 + 0.5, d) * smoothstep(-0.5, 0.5, -d);
        col = mix(col, selected, sel_border);
    }

    return vec4(col.rgb, col.a * aa);
}
