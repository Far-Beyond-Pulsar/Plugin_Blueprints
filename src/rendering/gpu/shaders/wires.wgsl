// Bezier wire renderer.
// CPU tessellates each wire into polyline segments and uploads them as a flat
// vertex buffer.  The fragment shader applies a glow / gloss effect using the
// UV coordinates packed by the CPU tessellator.
//
// Vertex layout (per vertex):
//   pos:   graph-space position (CPU already expanded to thick quad corners)
//   uv.x:  0 = left edge, 1 = right edge (used for glow gradient)
//   uv.y:  0..1 along the wire (unused currently, available for future fx)
//   color: wire colour

struct GraphUniforms {
    pan:      vec2<f32>,
    zoom:     f32,
    time:     f32,
    viewport: vec2<f32>,
    _pad1:    vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: GraphUniforms;

struct Vert {
    @location(0) pos:   vec2<f32>,
    @location(1) uv:    vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VOut {
    @builtin(position) pos:   vec4<f32>,
    @location(0)       uv:    vec2<f32>,
    @location(1)       color: vec4<f32>,
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

@vertex
fn vs_main(v: Vert) -> VOut {
    let scr = graph_to_screen(v.pos);
    var o: VOut;
    o.pos   = vec4(screen_to_ndc(scr), 0.0, 1.0);
    o.uv    = v.uv;
    o.color = v.color;
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // Signed distance from centre of wire (0 = centre, 1 = edge)
    let edge_dist = abs(in.uv.x * 2.0 - 1.0); // 0 at centre, 1 at edge

    // Outer glow — diffuse band around the wire
    let glow = smoothstep(1.0, 0.0, edge_dist) * 0.18;

    // Main body — sharper falloff inside ±0.65 of wire width
    let body = smoothstep(1.0, 0.65, edge_dist);

    // Bright highlight stripe — very narrow band at centre
    let highlight_w = 0.28;
    let highlight   = smoothstep(highlight_w, 0.0, edge_dist)
                    * smoothstep(1.0, 0.8,   edge_dist);

    let base_col  = in.color.rgb;
    let glow_col  = base_col;
    let high_col  = mix(base_col, vec3(1.0), 0.45);

    var col = vec3(0.0);
    col += glow_col * glow;
    col  = mix(col, base_col, body);
    col  = mix(col, high_col, highlight * 0.55);

    let total_a = in.color.a * max(body, glow * 0.6);
    return vec4(col, total_a);
}
