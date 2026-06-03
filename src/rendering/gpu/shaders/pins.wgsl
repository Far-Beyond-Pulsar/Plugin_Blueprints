struct GraphUniforms {
    pan:      vec2<f32>,
    zoom:     f32,
    time:     f32,
    viewport: vec2<f32>,
    _pad1:    vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: GraphUniforms;

struct PinInst {
    @location(0) center:     vec2<f32>,
    @location(1) size:       f32,
    @location(2) _pad0:      f32,
    @location(3) color:      vec4<f32>,
    @location(4) kind:       u32,   // 0=circle 1=exec
    @location(5) is_input:   u32,
    @location(6) compatible: u32,
    @location(7) _pad1:      u32,
}

struct VOut {
    @builtin(position) pos:        vec4<f32>,
    @location(0)       uv:         vec2<f32>,
    @location(1)       color:      vec4<f32>,
    @location(2)       kind:       u32,
    @location(3)       is_input:   u32,
    @location(4)       compatible: u32,
}

fn graph_to_screen(p: vec2<f32>) -> vec2<f32> {
    return (p + u.pan) * u.zoom;
}

fn screen_to_ndc(p: vec2<f32>) -> vec2<f32> {
    return vec2(
        p.x / u.viewport.x * 2.0 - 1.0,
       -(p.y / u.viewport.y * 2.0 - 1.0),
    );
}

fn sdf_circle(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn sdf_exec(p: vec2<f32>, is_input: bool) -> f32 {
    var q = p;
    if is_input { q.x = -q.x; }

    let body_right = 0.25;
    let d_rect = max(
        max(-q.x - 1.0, q.x - body_right),
        abs(q.y) - 1.0,
    );

    let tip_progress = (q.x - body_right) / (1.0 - body_right);
    let half_h = 1.0 - tip_progress;
    let d_tri = max(
        -(q.x - body_right),
        abs(q.y) - half_h,
    );

    return min(d_rect, d_tri);
}

var<private> CX: array<f32, 6> = array<f32, 6>(-1.0, 1.0, -1.0, -1.0, 1.0, 1.0);
var<private> CY: array<f32, 6> = array<f32, 6>(-1.0, -1.0, 1.0, 1.0, -1.0, 1.0);

@vertex
fn vs_main(inst: PinInst, @builtin(vertex_index) vi: u32) -> VOut {
    let uv  = vec2(CX[vi], CY[vi]);
    let half = inst.size * 0.5 * u.zoom;

    let scr_center = graph_to_screen(inst.center);
    let scr_pos    = scr_center + uv * half;

    var o: VOut;
    o.pos        = vec4(screen_to_ndc(scr_pos), 0.0, 1.0);
    o.uv         = uv;
    o.color      = inst.color;
    o.kind       = inst.kind;
    o.is_input   = inst.is_input;
    o.compatible = inst.compatible;
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let luma_w = vec3(0.299, 0.587, 0.114);
    let src_luma = dot(in.color.rgb, luma_w);
    let accent = mix(vec3(src_luma), in.color.rgb, 0.92);
    let compat_accent = vec3(0.50, 0.63, 0.88);
    let active_accent = select(accent, compat_accent, in.compatible != 0u);

    let ring_col = vec3(0.24, 0.27, 0.31);
    let fill_col = mix(vec3(0.12, 0.13, 0.15), active_accent, 0.74);
    let glow_col = mix(active_accent, vec3(1.0), 0.20);

    var d: f32;
    if in.kind == 0u {
        d = sdf_circle(in.uv, 0.78);
    } else {
        d = sdf_exec(in.uv, in.is_input != 0u);
    }

    let aa = max(fwidth(d), 0.02);
    let ring_w = 0.13;
    let inside = 1.0 - smoothstep(-aa, aa, d);
    let ring_outer = 1.0 - smoothstep(-aa, aa, d + ring_w);
    let ring = clamp(ring_outer - inside, 0.0, 1.0);
    if inside + ring <= 0.01 { discard; }

    var base = mix(ring_col, fill_col, inside / (inside + ring + 0.0001));

    if in.kind != 0u {
        var q = in.uv;
        if in.is_input != 0u { q.x = -q.x; }
        let stripe = smoothstep(0.22, 0.03, abs(q.y)) * smoothstep(0.02, 0.52, q.x);
        base = mix(base, active_accent, stripe * 0.30);
    }

    let glow = smoothstep(0.36, 0.0, d) * smoothstep(-aa, aa, d);
    base = mix(base, glow_col, glow * 0.16);

    let alpha = clamp(inside + ring, 0.0, 1.0);
    if in.compatible != 0u {
        let pulse = 0.90 + 0.10 * sin(u.time * 3.0);
        base = mix(base, glow_col, 0.18) * pulse;
    }

    return vec4(base, alpha);
}
