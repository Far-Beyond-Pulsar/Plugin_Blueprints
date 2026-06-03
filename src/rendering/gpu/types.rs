// GPU-side data structures — all repr(C) for safe byte-casting to WGPU buffers.

/// Uploaded once per frame as a uniform. Shared by all four pipelines.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GraphUniforms {
    pub pan: [f32; 2],
    pub zoom: f32,
    pub time: f32,
    pub viewport: [f32; 2], // render-target pixels (surface w/h)
    pub _pad1: [f32; 2],
}

// ── Node instances ─────────────────────────────────────────────────────────────
// One per visible node.  Vertex shader expands to 6 verts covering the node rect.

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NodeInstance {
    pub pos: [f32; 2],  // graph space top-left
    pub size: [f32; 2], // graph space size
    pub header_color: [f32; 4],
    pub body_color: [f32; 4],
    pub border_color: [f32; 4],
    pub sep_color: [f32; 4],
    /// header height as fraction of total node height (0..1)
    pub header_h_frac: f32,
    /// corner radius in graph-space units
    pub corner_r: f32,
    /// bit 0: is_reroute  bit 1: is_selected  bit 2: is_running
    pub flags: u32,
    pub _pad: u32,
}

// ── Bezier wire instances ──────────────────────────────────────────────────────
// One struct per connection.  The vertex shader generates WIRE_SEGS × 6 vertices
// per instance, evaluating the cubic bezier entirely on GPU.  No CPU tessellation.

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WireInstance {
    pub from: [f32; 2],  // graph-space start
    pub ctrl1: [f32; 2], // first bezier control point
    pub ctrl2: [f32; 2], // second bezier control point
    pub to: [f32; 2],    // graph-space end
    pub color: [f32; 4],
    pub thickness: f32,   // half-thickness in graph units
    pub flags: u32,       // bit 0 = active pulse, bit 1 = hidden/dim
    pub pulse_phase: f32, // deterministic per-wire pulse offset
    pub _pad: f32,
}

// ── Straight-line vertices (selection box, drag preview) ───────────────────────
// CPU emits a flat quad per segment via tessellate_line(); used only for the
// selection rectangle — tiny upload, not worth a separate instanced pipeline.

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WireVertex {
    pub pos: [f32; 2], // graph space
    pub uv: [f32; 2],  // u=edge(0=left,1=right), v=along(0..1)
    pub color: [f32; 4],
}

// ── Pin instances ──────────────────────────────────────────────────────────────
// One per visible pin.  Vertex shader expands to 6 verts (bounding square).

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PinInstance {
    pub center: [f32; 2], // graph space
    pub size: f32,        // diameter (graph units)
    pub _pad0: f32,
    pub color: [f32; 4],
    /// 0 = circle (data pin)  1 = exec arrow
    pub kind: u32,
    /// 1 = input side (arrow points left)  0 = output
    pub is_input: u32,
    /// 1 = highlighted compatible-drop target
    pub compatible: u32,
    pub _pad1: u32,
}

// ── Comment instances ──────────────────────────────────────────────────────────
// One per visible comment box. Vertex shader expands to 6 verts covering the rect.

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CommentInstance {
    pub pos: [f32; 2], // graph space top-left
    pub size: [f32; 2],
    pub fill_color: [f32; 4],
    pub border_color: [f32; 4],
    pub corner_r: f32,
    pub flags: u32, // bit 0: selected
    pub _pad0: u32,
    pub _pad1: u32,
}

// ── Selection box ──────────────────────────────────────────────────────────────
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SelectionInstance {
    pub pos: [f32; 2],  // graph space
    pub size: [f32; 2], // graph space
}
