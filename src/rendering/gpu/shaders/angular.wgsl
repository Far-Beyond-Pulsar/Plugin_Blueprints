const WIRE_SEGS: u32 = 32u;

struct GraphUniforms {
    pan:      vec2<f32>,
    zoom:     f32,
    time:     f32,
    viewport: vec2<f32>,
    _pad1:    vec2<f32>,
}
@group(0) @binding(0) var<uniform> u: GraphUniforms;

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
    @location(2) @interpolate(flat) flags: u32,
    @location(3)       phase: f32,
}

struct RouteSample {
    point: vec2<f32>,
    distance: f32,
}

fn graph_to_screen(p: vec2<f32>) -> vec2<f32> {
    return (p + u.pan) * u.zoom;
}

fn screen_to_ndc(p: vec2<f32>) -> vec2<f32> {
    return vec2(p.x / u.viewport.x * 2.0 - 1.0,
               -(p.y / u.viewport.y * 2.0 - 1.0));
}

// Keep chamfers large enough to rasterize cleanly. When the available space is
// too short at the current zoom, use a square elbow rather than a subpixel
// diagonal that changes apparent angle as the graph is zoomed.
fn route_bevel(p0: vec2<f32>, p5: vec2<f32>) -> f32 {
    let delta = p5 - p0;
    let half_horizontal = abs(delta.x) * 0.5;
    let vertical = abs(delta.y);
    let max_bevel = min(18.0, min(half_horizontal * 0.45, vertical * 0.45));
    return select(0.0, max_bevel, max_bevel * u.zoom >= 6.0);
}

// Five runs: horizontal, 45-degree chamfer, vertical, 45-degree chamfer,
// horizontal. Tessellation boundaries land exactly on each corner.
fn route_points(p0: vec2<f32>, p5: vec2<f32>) -> array<vec2<f32>, 6> {
    let delta = p5 - p0;
    let mid_x = (p0.x + p5.x) * 0.5;
    let sx = select(-1.0, 1.0, delta.x > 0.0);
    let sy = select(-1.0, 1.0, delta.y > 0.0);
    let bevel = route_bevel(p0, p5);

    if abs(delta.x) < 0.001 || abs(delta.y) < 0.001 {
        return array<vec2<f32>, 6>(p0, p0, p0, p0, p5, p5);
    }

    let a = vec2(mid_x - sx * bevel, p0.y);
    let b = vec2(mid_x, p0.y + sy * bevel);
    let c = vec2(mid_x, p5.y - sy * bevel);
    let d = vec2(mid_x + sx * bevel, p5.y);
    return array<vec2<f32>, 6>(p0, a, b, c, d, p5);
}

fn route_tangent(p0: vec2<f32>, p5: vec2<f32>, section: u32) -> vec2<f32> {
    let delta = p5 - p0;
    if length(delta) < 0.001 {
        return vec2(1.0, 0.0);
    }
    if abs(delta.x) < 0.001 || abs(delta.y) < 0.001 {
        return normalize(delta);
    }
    let sx = select(-1.0, 1.0, delta.x > 0.0);
    let sy = select(-1.0, 1.0, delta.y > 0.0);
    if route_bevel(p0, p5) == 0.0 {
        if section == 0u || section == 4u {
            return vec2(sx, 0.0);
        }
        return vec2(0.0, sy);
    }
    if section == 0u || section == 4u {
        return vec2(sx, 0.0);
    }
    if section == 1u || section == 3u {
        return normalize(vec2(sx, sy));
    }
    return vec2(0.0, sy);
}

fn route_sample(p0: vec2<f32>, p5: vec2<f32>, step: u32) -> RouteSample {
    let points = route_points(p0, p5);
    let delta = p5 - p0;
    if abs(delta.x) < 0.001 || abs(delta.y) < 0.001 {
        let t = f32(step) / f32(WIRE_SEGS);
        return RouteSample(mix(p0, p5, t), length(delta) * t);
    }

    let lengths = array<f32, 5>(
        length(points[1] - points[0]),
        length(points[2] - points[1]),
        length(points[3] - points[2]),
        length(points[4] - points[3]),
        length(points[5] - points[4]),
    );
    var point = points[5];
    var along = lengths[0] + lengths[1] + lengths[2] + lengths[3] + lengths[4];
    if step <= 6u {
        let t = f32(step) / 6.0;
        point = mix(points[0], points[1], t);
        along = lengths[0] * t;
    } else if step <= 11u {
        let t = f32(step - 6u) / 5.0;
        point = mix(points[1], points[2], t);
        along = lengths[0] + lengths[1] * t;
    } else if step <= 21u {
        let t = f32(step - 11u) / 10.0;
        point = mix(points[2], points[3], t);
        along = lengths[0] + lengths[1] + lengths[2] * t;
    } else if step <= 26u {
        let t = f32(step - 21u) / 5.0;
        point = mix(points[3], points[4], t);
        along = lengths[0] + lengths[1] + lengths[2] + lengths[3] * t;
    } else {
        let t = f32(step - 26u) / 6.0;
        point = mix(points[4], points[5], t);
        along = lengths[0] + lengths[1] + lengths[2] + lengths[3] + lengths[4] * t;
    }
    return RouteSample(point, along);
}

fn corner_offset(
    previous_tangent: vec2<f32>,
    next_tangent: vec2<f32>,
    half_width: f32,
    side: f32,
) -> vec2<f32> {
    let previous_normal = vec2(-previous_tangent.y, previous_tangent.x);
    let next_normal = vec2(-next_tangent.y, next_tangent.x);
    let normal_sum = previous_normal + next_normal;
    let miter = select(next_normal, normalize(normal_sum), length(normal_sum) > 0.001);
    let miter_scale = half_width / max(abs(dot(miter, next_normal)), 0.25);
    return miter * miter_scale * side;
}

fn route_offset(p0: vec2<f32>, p5: vec2<f32>, step: u32, half_width: f32, side: f32) -> vec2<f32> {
    let delta = p5 - p0;
    if abs(delta.x) < 0.001 || abs(delta.y) < 0.001 {
        let tangent = select(vec2(1.0, 0.0), normalize(delta), length(delta) > 0.001);
        return vec2(-tangent.y, tangent.x) * half_width * side;
    }

    if route_bevel(p0, p5) == 0.0 {
        if step >= 6u && step <= 11u {
            return corner_offset(
                route_tangent(p0, p5, 0u),
                route_tangent(p0, p5, 1u),
                half_width,
                side,
            );
        }
        if step >= 21u && step <= 26u {
            return corner_offset(
                route_tangent(p0, p5, 2u),
                route_tangent(p0, p5, 4u),
                half_width,
                side,
            );
        }
        let tangent = route_tangent(p0, p5, select(2u, 0u, step < 6u || step > 26u));
        return vec2(-tangent.y, tangent.x) * half_width * side;
    }

    if step == 6u {
        return corner_offset(route_tangent(p0, p5, 0u), route_tangent(p0, p5, 1u), half_width, side);
    }
    if step == 11u {
        return corner_offset(route_tangent(p0, p5, 1u), route_tangent(p0, p5, 2u), half_width, side);
    }
    if step == 21u {
        return corner_offset(route_tangent(p0, p5, 2u), route_tangent(p0, p5, 3u), half_width, side);
    }
    if step == 26u {
        return corner_offset(route_tangent(p0, p5, 3u), route_tangent(p0, p5, 4u), half_width, side);
    }

    var section = 4u;
    if step < 6u {
        section = 0u;
    } else if step < 11u {
        section = 1u;
    } else if step < 21u {
        section = 2u;
    } else if step < 26u {
        section = 3u;
    }
    let tangent = route_tangent(p0, p5, section);
    return vec2(-tangent.y, tangent.x) * half_width * side;
}

@vertex
fn vs_main(inst: WireInst, @builtin(vertex_index) vi: u32) -> VOut {
    let seg = vi / 6u;
    let corner = vi % 6u;
    let step = select(seg, seg + 1u, (corner == 2u) || (corner == 3u) || (corner == 5u));
    let side = select(-1.0, 1.0, (corner == 1u) || (corner == 4u) || (corner == 5u));
    let sample = route_sample(inst.start, inst.to, step);
    let half_width = inst.thickness * u.zoom;
    let screen_position = graph_to_screen(sample.point) + route_offset(inst.start, inst.to, step, half_width, side);

    var out: VOut;
    out.pos = vec4(screen_to_ndc(screen_position), 0.0, 1.0);
    out.uv = vec2(select(0.0, 1.0, side > 0.0), sample.distance / max(route_sample(inst.start, inst.to, WIRE_SEGS).distance, 0.001));
    out.color = inst.color;
    out.flags = inst.flags;
    out.phase = inst.phase;
    return out;
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
