// GPU-side data structures — all repr(C) for safe byte-casting to WGPU buffers.

/// Uploaded once per frame as a uniform. Shared by all four pipelines.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GraphUniforms {
    pub pan:      [f32; 2],
    pub zoom:     f32,
    pub _pad0:    f32,
    pub viewport: [f32; 2], // render-target pixels (surface w/h)
    pub _pad1:    [f32; 2],
}

// ── Node instances ─────────────────────────────────────────────────────────────
// One per visible node.  Vertex shader expands to 6 verts covering the node rect.

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NodeInstance {
    pub pos:           [f32; 2], // graph space top-left
    pub size:          [f32; 2], // graph space size
    pub header_color:  [f32; 4],
    pub body_color:    [f32; 4],
    pub border_color:  [f32; 4],
    pub sep_color:     [f32; 4],
    /// header height as fraction of total node height (0..1)
    pub header_h_frac: f32,
    /// corner radius in graph-space units
    pub corner_r:      f32,
    /// bit 0: is_selected  bit 1: is_reroute
    pub flags:         u32,
    pub _pad:          u32,
}

// ── Wire vertices (CPU-tessellated bezier) ─────────────────────────────────────
// CPU evaluates the bezier and emits 6 verts per segment (triangle quads).
// UV.x = 0/1 for left/right edge — used for glow in fs.

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WireVertex {
    pub pos:   [f32; 2], // graph space
    pub uv:    [f32; 2], // u=edge(0=left,1=right), v=along(0..1)
    pub color: [f32; 4],
}

// ── Pin instances ──────────────────────────────────────────────────────────────
// One per visible pin.  Vertex shader expands to 6 verts (bounding square).

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PinInstance {
    pub center:     [f32; 2], // graph space
    pub size:       f32,      // diameter (graph units)
    pub _pad0:      f32,
    pub color:      [f32; 4],
    /// 0 = circle (data pin)  1 = exec arrow
    pub kind:       u32,
    /// 1 = input side (arrow points left)  0 = output
    pub is_input:   u32,
    /// 1 = highlighted compatible-drop target
    pub compatible: u32,
    pub _pad1:      u32,
}

// ── Selection box ──────────────────────────────────────────────────────────────
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SelectionInstance {
    pub pos:   [f32; 2], // graph space
    pub size:  [f32; 2], // graph space
}
