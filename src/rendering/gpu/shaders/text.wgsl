// Glyph atlas text renderer.
// Vertices are pre-positioned in screen pixels by the CPU.
// Fragment shader samples the coverage atlas and multiplies by the text colour.

struct GraphUniforms {
    pan:      vec2<f32>,
    zoom:     f32,
    time:     f32,
    viewport: vec2<f32>,
    _pad1:    vec2<f32>,
}

@group(0) @binding(0) var<uniform>  u:       GraphUniforms;
@group(1) @binding(0) var          t_atlas:  texture_2d<f32>;
@group(1) @binding(1) var          s_atlas:  sampler;

struct Vert {
    // All coordinates are already in screen pixels (no graph→screen conversion needed).
    @location(0) pos:   vec2<f32>,
    @location(1) uv:    vec2<f32>, // into the atlas texture (0..1)
    @location(2) color: vec4<f32>,
}

struct VOut {
    @builtin(position) pos:   vec4<f32>,
    @location(0)       uv:    vec2<f32>,
    @location(1)       color: vec4<f32>,
}

fn screen_to_ndc(p: vec2<f32>) -> vec2<f32> {
    return vec2(
        p.x / u.viewport.x * 2.0 - 1.0,
       -(p.y / u.viewport.y * 2.0 - 1.0),
    );
}

@vertex
fn vs_main(v: Vert) -> VOut {
    var o: VOut;
    o.pos   = vec4(screen_to_ndc(v.pos), 0.0, 1.0);
    o.uv    = v.uv;
    o.color = v.color;
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // Atlas stores 8-bit coverage in the red channel.
    let coverage = textureSample(t_atlas, s_atlas, in.uv).r;
    // Thin-stroke gamma correction — improves readability at small sizes.
    let a = pow(coverage, 0.75);
    return vec4(in.color.rgb, in.color.a * a);
}
