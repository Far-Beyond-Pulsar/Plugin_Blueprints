// Blueprint graph background — dark canvas with two-level grid lines.

struct GraphUniforms {
    pan:      vec2<f32>,
    zoom:     f32,
    _pad0:    f32,
    viewport: vec2<f32>,
    _pad1:    vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: GraphUniforms;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0)       uv:  vec2<f32>,
}

// Full-screen quad — no vertex buffer needed.
var<private> VERTS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(-1.0, -1.0), vec2( 1.0, -1.0), vec2(-1.0,  1.0),
    vec2(-1.0,  1.0), vec2( 1.0, -1.0), vec2( 1.0,  1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    let p = VERTS[vi];
    var o: VOut;
    o.pos = vec4(p, 0.0, 1.0);
    // UV in screen pixels, y=0 at top
    o.uv  = (p * 0.5 + 0.5) * vec2(1.0, -1.0) + vec2(0.0, 1.0);
    o.uv *= u.viewport;
    return o;
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn grid_alpha(screen_px: vec2<f32>, step: f32, line_w: f32) -> f32 {
    // Wrapping offset so the grid scrolls with pan
    let offset = vec2(u.pan.x * u.zoom, u.pan.y * u.zoom);
    let local   = (screen_px - offset) % step;
    // Distance to nearest grid line (horizontal or vertical)
    let dx = min(local.x, step - local.x);
    let dy = min(local.y, step - local.y);
    let d  = min(dx, dy);
    return 1.0 - smoothstep(0.0, line_w, d);
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // Deep dark background matching the BP editor aesthetic
    var col = vec4(0.055, 0.055, 0.058, 1.0);

    // Minor grid (every 10 graph units)
    let minor_step = clamp(10.0 * u.zoom, 4.0, 200.0);
    if minor_step >= 4.0 {
        let a = grid_alpha(in.uv, minor_step, 0.7) * 0.30;
        col = mix(col, vec4(0.22, 0.22, 0.24, 1.0), a);
    }

    // Major grid (every 50 graph units)
    let major_step = clamp(50.0 * u.zoom, 8.0, 1000.0);
    if major_step >= 8.0 {
        let a = grid_alpha(in.uv, major_step, 0.9) * 0.55;
        col = mix(col, vec4(0.30, 0.30, 0.32, 1.0), a);
    }

    return col;
}
