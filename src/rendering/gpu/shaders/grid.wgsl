// Blueprint graph background — dark canvas with infinite two-level grid lines.

struct GraphUniforms {
    pan:      vec2<f32>,
    zoom:     f32,
    time:     f32,
    viewport: vec2<f32>,
    _pad1:    vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: GraphUniforms;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0)       uv:  vec2<f32>,
}

// Full-screen quad — covers entire NDC space, so the grid is infinite.
var<private> VERTS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(-1.0, -1.0), vec2( 1.0, -1.0), vec2(-1.0,  1.0),
    vec2(-1.0,  1.0), vec2( 1.0, -1.0), vec2( 1.0,  1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    let p = VERTS[vi];
    var o: VOut;
    o.pos = vec4(p, 0.0, 1.0);
    // Convert NDC to screen pixels (y=0 at top, matching pan convention).
    o.uv  = (p * 0.5 + 0.5) * vec2(1.0, -1.0) + vec2(0.0, 1.0);
    o.uv *= u.viewport;
    return o;
}

// ── helpers ───────────────────────────────────────────────────────────────────

// Euclidean modulo — always returns a value in [0, step), even for negative input.
// WGSL's built-in % can return negative values (like C fmod), which breaks the
// grid on the left and top sides where screen_px < pan_offset.
fn emod(a: f32, b: f32) -> f32 {
    return ((a % b) + b) % b;
}

fn grid_alpha(screen_px: vec2<f32>, step: f32, line_w: f32) -> f32 {
    // Grid origin follows pan: graph coordinate 0 maps to screen position pan*zoom.
    let origin = u.pan * u.zoom;
    let local  = vec2(emod(screen_px.x - origin.x, step),
                      emod(screen_px.y - origin.y, step));
    // Distance to nearest grid line on either axis.
    let dx = min(local.x, step - local.x);
    let dy = min(local.y, step - local.y);
    let d  = min(dx, dy);
    return 1.0 - smoothstep(0.0, line_w, d);
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    var col = vec4(0.055, 0.055, 0.058, 1.0);

    // Minor grid — every 10 graph units
    let minor_step = clamp(10.0 * u.zoom, 4.0, 200.0);
    if minor_step >= 4.0 {
        let a = grid_alpha(in.uv, minor_step, 0.75) * 0.28;
        col = mix(col, vec4(0.20, 0.20, 0.22, 1.0), a);
    }

    // Major grid — every 50 graph units
    let major_step = clamp(50.0 * u.zoom, 8.0, 1000.0);
    if major_step >= 8.0 {
        let a = grid_alpha(in.uv, major_step, 1.0) * 0.52;
        col = mix(col, vec4(0.28, 0.28, 0.30, 1.0), a);
    }

    return col;
}
