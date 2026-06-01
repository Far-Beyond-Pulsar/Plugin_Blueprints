// Instanced pin renderer.
// Data pins → SDF circle.  Exec pins → SDF pentagon (chevron arrow).
// One instance per pin; vertex shader emits 6 verts covering the pin's bounding square.

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
    @location(0)       uv:         vec2<f32>,  // -1..1 within bounding square
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

// ── helpers ───────────────────────────────────────────────────────────────────

// Signed distance to a circle of radius r centred at origin.
fn sdf_circle(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}

// Exec pin: pentagon pointing right (output) or left (input).
// p is normalised: (-1,-1)..(1,1) within bounding square.
// Returns signed distance (negative = inside shape).
fn sdf_exec(p: vec2<f32>, is_input: bool) -> f32 {
    // Flip x for input (points left)
    var q = p;
    if is_input { q.x = -q.x; }

    // Body rectangle: x in [-1, 0.25], y in [-1, 1]
    let body_right = 0.25;
    let d_rect = max(
        max(-q.x - 1.0, q.x - body_right),
        abs(q.y) - 1.0,
    );

    // Triangle tip: x in [0.25, 1], tapers to point at x=1
    let tip_progress = (q.x - body_right) / (1.0 - body_right); // 0..1
    let half_h       = 1.0 - tip_progress;                       // 1..0
    let d_tri = max(
        -(q.x - body_right),
        abs(q.y) - half_h,
    );

    // Union of body and tip
    return min(d_rect, d_tri);
}

// ── vertex ────────────────────────────────────────────────────────────────────

var<private> CX: array<f32, 6> = array<f32, 6>(-1.0, 1.0, -1.0, -1.0, 1.0, 1.0);
var<private> CY: array<f32, 6> = array<f32, 6>(-1.0, -1.0, 1.0, 1.0, -1.0, 1.0);

@vertex
fn vs_main(inst: PinInst, @builtin(vertex_index) vi: u32) -> VOut {
    let uv  = vec2(CX[vi], CY[vi]);
    let half = inst.size * 0.5 * u.zoom;

    // Expand bounding square around centre (screen space)
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

// ── fragment ──────────────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // Compatible override colour (clean neon cyan)
    let fill_col = select(
        in.color,
        vec4(0.30, 0.92, 0.86, 1.0),
        in.compatible != 0u,
    );

    var d: f32;
    if in.kind == 0u {
        // Circle
        d = sdf_circle(in.uv, 0.82); // slightly inside bounding square
    } else {
        // Exec arrow
        d = sdf_exec(in.uv, in.is_input != 0u);
    }

    // Outer ring and gentle emissive shell.
    let border_w = 0.11;
    let border_col = vec4(0.035, 0.04, 0.05, 1.0);

    // Compute final colour
    let inside  = 1.0 - smoothstep(-0.02, 0.02, d);
    let ring    = smoothstep(-0.02, 0.02, d + border_w)
                * (1.0 - inside);
    let shell   = smoothstep(0.14, -0.02, d) * (1.0 - inside) * 0.28;

    if inside + ring + shell <= 0.01 { discard; }

    var col = mix(border_col, fill_col, inside / (inside + ring + 0.0001));
    col = vec4(col.rgb + fill_col.rgb * shell * 0.55, col.a);
    col.a *= (inside + ring + shell);

    // Crisp top highlight for circular data pins.
    if in.kind == 0u {
        let high = smoothstep(0.0, -0.6, d) * smoothstep(-0.2, 0.05, in.uv.y) * 0.28;
        col = vec4(col.rgb + high, col.a);
    }

    return col;
}
