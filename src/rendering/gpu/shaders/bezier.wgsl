const WIRE_SEGS: u32 = 32u;

struct GraphUniforms {
    pan:      vec2<f32>,
    zoom:     f32,
    time:     f32,
    viewport: vec2<f32>,
    _pad1:    vec2<f32>,
}
@group(0) @binding(0) var<uniform> u: GraphUniforms;

// ── instance attributes (one set shared across all WIRE_SEGS*6 verts) ─────────
struct WireInst {
    @location(0) start:     vec2<f32>,
    @location(1) ctrl1:     vec2<f32>,
    @location(2) ctrl2:     vec2<f32>,
    @location(3) to:        vec2<f32>,
    @location(4) color:     vec4<f32>,
    @location(5) thickness: f32,
    @location(6) flags:     u32,
    @location(7) phase:     f32,
    @location(8) _pad:      f32,
}

struct VOut {
    @builtin(position) pos:   vec4<f32>,
    @location(0)       uv:    vec2<f32>,
    @location(1)       color: vec4<f32>,
    @location(2)       flags: u32,
    @location(3)       phase: f32,
}

fn graph_to_screen(p: vec2<f32>) -> vec2<f32> {
    return (p + u.pan) * u.zoom;
}
fn screen_to_ndc(p: vec2<f32>) -> vec2<f32> {
    return vec2(p.x / u.viewport.x * 2.0 - 1.0,
               -(p.y / u.viewport.y * 2.0 - 1.0));
}

fn bezier(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t: f32) -> vec2<f32> {
    let u_ = 1.0 - t;
    return u_*u_*u_*p0 + 3.0*u_*u_*t*p1 + 3.0*u_*t*t*p2 + t*t*t*p3;
}

fn bezier_tangent(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t: f32) -> vec2<f32> {
    let u_ = 1.0 - t;
    return 3.0*u_*u_*(p1-p0) + 6.0*u_*t*(p2-p1) + 3.0*t*t*(p3-p2);
}

@vertex
fn vs_main(inst: WireInst, @builtin(vertex_index) vi: u32) -> VOut {
    let seg    = vi / 6u;
    let corner = vi % 6u;

    let use_t1    = (corner == 2u) || (corner == 3u) || (corner == 5u);
    let use_right = (corner == 1u) || (corner == 4u) || (corner == 5u);

    let t = select(f32(seg), f32(seg + 1u), use_t1) / f32(WIRE_SEGS);

    let gp   = bezier(inst.start, inst.ctrl1, inst.ctrl2, inst.to, t);
    let gtan = bezier_tangent(inst.start, inst.ctrl1, inst.ctrl2, inst.to, t);

    let slen = length(gtan * u.zoom);
    var normal_screen = vec2(0.0, 1.0);
    if slen > 0.001 {
        let stan = normalize(gtan * u.zoom);
        normal_screen = vec2(-stan.y, stan.x);
    }

    let half_px  = inst.thickness * u.zoom;
    let side     = select(-1.0, 1.0, use_right);
    let scr_pos  = graph_to_screen(gp) + normal_screen * half_px * side;

    var o: VOut;
    o.pos   = vec4(screen_to_ndc(scr_pos), 0.0, 1.0);
    o.uv    = vec2(select(0.0, 1.0, use_right), t);
    o.color = inst.color;
    o.flags = inst.flags;
    o.phase = inst.phase;
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let is_active = (in.flags & 1u) != 0u;
    let is_hidden = (in.flags & 2u) != 0u;
    let edge_dist = abs(in.uv.x * 2.0 - 1.0);
    let luma_w = vec3(0.299, 0.587, 0.114);
    let src_luma = dot(in.color.rgb, luma_w);
    let tint = mix(vec3(src_luma), in.color.rgb, 0.90);

    let edge_soft = smoothstep(1.0, 0.56, edge_dist);
    let center = smoothstep(0.44, 0.0, edge_dist);
    var col = mix(tint * 0.78, tint * 0.96, edge_soft);
    col = mix(col, vec3(0.94), center * 0.08);
    var alpha = in.color.a * edge_soft;

    if is_active {
        let pulse_t = fract(in.uv.y * 5.0 - u.time * 2.1 + in.phase);
        let pulse_d = abs(pulse_t - 0.5);
        let pulse = smoothstep(0.20, 0.0, pulse_d) * smoothstep(0.08, 0.0, pulse_d);
        col = mix(col, vec3(0.96), pulse * center * 0.92);
        alpha = min(1.0, alpha + pulse * 0.26);
    }

    if is_hidden {
        let luma = dot(col, luma_w);
        col = mix(vec3(luma), col, 0.10);
        alpha *= 0.16;
    }

    return vec4(col, alpha);
}
