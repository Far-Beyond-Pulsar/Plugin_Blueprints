// BpRenderer — owns seven WGPU render pipelines and per-frame GPU state.
//
// Pipelines:
//   grid    — full-screen quad, uniform-only
//   comments— instanced comment quads (6 verts × comment count)
//   nodes   — instanced node quads (6 verts × node count)
//   bezier  — instanced bezier wires (WIRE_SEGS*6 verts × connection count)
//             GPU evaluates cubic bezier in vertex shader — zero CPU tessellation
//   lines   — vertex-buffer straight quads for selection-box outline (tiny)
//   pins    — instanced pin quads (6 verts × pin count)
//   text    — glyph atlas, one quad per visible character

use super::text::{TextAlign, TextRenderer};
use super::types::{
    CommentInstance, GraphUniforms, NodeInstance, PinInstance, WireInstance, WireVertex,
};

const WIRE_SEGS: u32 = 32;

// ─── pipeline containers ──────────────────────────────────────────────────────

struct GridState {
    pipeline: wgpu::RenderPipeline,
    uni_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

struct NodeState {
    pipeline: wgpu::RenderPipeline,
    uni_buf: wgpu::Buffer,
    uni_bg: wgpu::BindGroup,
    inst_buf: wgpu::Buffer,
    inst_cap: u64,
}

struct CommentState {
    pipeline: wgpu::RenderPipeline,
    uni_buf: wgpu::Buffer,
    uni_bg: wgpu::BindGroup,
    inst_buf: wgpu::Buffer,
    inst_cap: u64,
}

/// Instanced bezier wire pipeline — one instance per connection.
struct BezierState {
    pipeline: wgpu::RenderPipeline,
    uni_buf: wgpu::Buffer,
    uni_bg: wgpu::BindGroup,
    inst_buf: wgpu::Buffer,
    inst_cap: u64,
}

/// Vertex-buffer straight-line pipeline — used only for selection box outline.
struct LineState {
    pipeline: wgpu::RenderPipeline,
    uni_buf: wgpu::Buffer,
    uni_bg: wgpu::BindGroup,
    vert_buf: wgpu::Buffer,
    vert_cap: u64,
}

struct PinState {
    pipeline: wgpu::RenderPipeline,
    uni_buf: wgpu::Buffer,
    uni_bg: wgpu::BindGroup,
    inst_buf: wgpu::Buffer,
    inst_cap: u64,
}

// ─── public renderer ──────────────────────────────────────────────────────────

pub struct BpRenderer {
    grid: Option<GridState>,
    comments: Option<CommentState>,
    nodes: Option<NodeState>,
    bezier: Option<BezierState>,
    lines: Option<LineState>,
    pins: Option<PinState>,
    text: TextRenderer,
}

impl BpRenderer {
    pub fn new() -> Self {
        Self {
            grid: None,
            comments: None,
            nodes: None,
            bezier: None,
            lines: None,
            pins: None,
            text: TextRenderer::new(),
        }
    }

    /// Called every frame by `graph.rs`.
    ///
    /// - `comment_instances`: one per visible comment box
    /// - `wire_instances`: one per bezier connection — GPU evaluates the curve
    /// - `line_verts`:     pre-tessellated straight quads (selection box only)
    /// - `text_calls`:     (text, screen_x, screen_y, size_px, rgba, center)
    pub fn render_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        w: u32,
        h: u32,
        fmt: wgpu::TextureFormat,
        uniforms: &GraphUniforms,
        comment_instances: &[CommentInstance],
        nodes: &[NodeInstance],
        wire_instances: &[WireInstance],
        line_verts: &[WireVertex],
        pins: &[PinInstance],
        text_calls: &[(String, f32, f32, f32, [f32; 4], bool)],
    ) {
        if self.grid.is_none() {
            self.grid = Some(Self::create_grid(device, fmt));
            self.comments = Some(Self::create_comments(device, fmt));
            self.nodes = Some(Self::create_nodes(device, fmt));
            self.bezier = Some(Self::create_bezier(device, fmt));
            self.lines = Some(Self::create_lines(device, fmt));
            self.pins = Some(Self::create_pins(device, fmt));
        }

        let uni_bytes = bytemuck::bytes_of(uniforms);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bp_encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bp_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.055,
                            g: 0.055,
                            b: 0.058,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // ── 1. Grid ────────────────────────────────────────────────────────
            if let Some(g) = &self.grid {
                queue.write_buffer(&g.uni_buf, 0, uni_bytes);
                pass.set_pipeline(&g.pipeline);
                pass.set_bind_group(0, &g.bind_group, &[]);
                pass.draw(0..6, 0..1);
            }

            // ── 2. Comment boxes ───────────────────────────────────────────────
            if !comment_instances.is_empty() {
                if let Some(cs) = &mut self.comments {
                    queue.write_buffer(&cs.uni_buf, 0, uni_bytes);
                    let bytes = bytemuck::cast_slice(comment_instances);
                    Self::ensure_buf(
                        device,
                        &mut cs.inst_buf,
                        &mut cs.inst_cap,
                        bytes,
                        wgpu::BufferUsages::VERTEX,
                    );
                    queue.write_buffer(&cs.inst_buf, 0, bytes);
                    pass.set_pipeline(&cs.pipeline);
                    pass.set_bind_group(0, &cs.uni_bg, &[]);
                    pass.set_vertex_buffer(0, cs.inst_buf.slice(..));
                    pass.draw(0..6, 0..comment_instances.len() as u32);
                }
            }

            // ── 3. Bezier wire instances ───────────────────────────────────────
            // GPU evaluates cubic bezier in vertex shader — no CPU tessellation.
            if !wire_instances.is_empty() {
                if let Some(bs) = &mut self.bezier {
                    queue.write_buffer(&bs.uni_buf, 0, uni_bytes);
                    let bytes = bytemuck::cast_slice(wire_instances);
                    Self::ensure_buf(
                        device,
                        &mut bs.inst_buf,
                        &mut bs.inst_cap,
                        bytes,
                        wgpu::BufferUsages::VERTEX,
                    );
                    queue.write_buffer(&bs.inst_buf, 0, bytes);
                    pass.set_pipeline(&bs.pipeline);
                    pass.set_bind_group(0, &bs.uni_bg, &[]);
                    pass.set_vertex_buffer(0, bs.inst_buf.slice(..));
                    // WIRE_SEGS segments × 6 verts per instance
                    pass.draw(0..WIRE_SEGS * 6, 0..wire_instances.len() as u32);
                }
            }

            // ── 4. Straight line segments (selection box) ──────────────────────
            if !line_verts.is_empty() {
                if let Some(ls) = &mut self.lines {
                    queue.write_buffer(&ls.uni_buf, 0, uni_bytes);
                    let bytes = bytemuck::cast_slice(line_verts);
                    Self::ensure_buf(
                        device,
                        &mut ls.vert_buf,
                        &mut ls.vert_cap,
                        bytes,
                        wgpu::BufferUsages::VERTEX,
                    );
                    queue.write_buffer(&ls.vert_buf, 0, bytes);
                    pass.set_pipeline(&ls.pipeline);
                    pass.set_bind_group(0, &ls.uni_bg, &[]);
                    pass.set_vertex_buffer(0, ls.vert_buf.slice(..));
                    pass.draw(0..line_verts.len() as u32, 0..1);
                }
            }

            // ── 5. Nodes ───────────────────────────────────────────────────────
            if !nodes.is_empty() {
                if let Some(ns) = &mut self.nodes {
                    queue.write_buffer(&ns.uni_buf, 0, uni_bytes);
                    let bytes = bytemuck::cast_slice(nodes);
                    Self::ensure_buf(
                        device,
                        &mut ns.inst_buf,
                        &mut ns.inst_cap,
                        bytes,
                        wgpu::BufferUsages::VERTEX,
                    );
                    queue.write_buffer(&ns.inst_buf, 0, bytes);
                    pass.set_pipeline(&ns.pipeline);
                    pass.set_bind_group(0, &ns.uni_bg, &[]);
                    pass.set_vertex_buffer(0, ns.inst_buf.slice(..));
                    pass.draw(0..6, 0..nodes.len() as u32);
                }
            }

            // ── 6. Pins ────────────────────────────────────────────────────────
            if !pins.is_empty() {
                if let Some(ps) = &mut self.pins {
                    queue.write_buffer(&ps.uni_buf, 0, uni_bytes);
                    let bytes = bytemuck::cast_slice(pins);
                    Self::ensure_buf(
                        device,
                        &mut ps.inst_buf,
                        &mut ps.inst_cap,
                        bytes,
                        wgpu::BufferUsages::VERTEX,
                    );
                    queue.write_buffer(&ps.inst_buf, 0, bytes);
                    pass.set_pipeline(&ps.pipeline);
                    pass.set_bind_group(0, &ps.uni_bg, &[]);
                    pass.set_vertex_buffer(0, ps.inst_buf.slice(..));
                    pass.draw(0..6, 0..pins.len() as u32);
                }
            }

            // ── 7. Text ─────────────────────────────────────────────────────────
            // Queue all text calls, then flush into this render pass.
            for (text, sx, sy, size, color, center) in text_calls {
                let align = if *center {
                    TextAlign::Center
                } else {
                    TextAlign::Left
                };
                self.text.queue(text, *sx, *sy, *size, *color, align);
            }
            // Need a shared uniform buffer/BGL for the text pipeline.
            // Lazily use the grid pipeline's uni_buf since it has the same layout.
            if let Some(ref g) = self.grid {
                // Ensure atlas is uploaded before flushing
                self.text.atlas.upload_if_needed(device, queue);
                // Rebuild text bind-group infra if needed (done inside flush)
                // We pass the grid's bgl+buf as the shared uniform binding.
                self.text
                    .flush_with_external_uni(device, queue, &mut pass, &g.uni_buf, fmt);
            }
        } // end render pass

        queue.submit(std::iter::once(encoder.finish()));
    }

    // ── buffer helpers ────────────────────────────────────────────────────────

    /// Grow a buffer if it's too small.
    fn ensure_buf(
        device: &wgpu::Device,
        buf: &mut wgpu::Buffer,
        cap: &mut u64,
        data: &[u8],
        usage: wgpu::BufferUsages,
    ) {
        let needed = data.len() as u64;
        if needed > *cap {
            *cap = (needed * 2).max(256);
            *buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: *cap,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
    }

    // ── pipeline creators ─────────────────────────────────────────────────────

    fn uni_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bp_uni_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    fn uni_buf_and_bg(
        device: &wgpu::Device,
        bgl: &wgpu::BindGroupLayout,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bp_uni"),
            size: std::mem::size_of::<GraphUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bp_uni_bg"),
            layout: bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        (buf, bg)
    }

    fn alpha_blend_target(fmt: wgpu::TextureFormat) -> wgpu::ColorTargetState {
        wgpu::ColorTargetState {
            format: fmt,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        }
    }

    fn create_comments(device: &wgpu::Device, fmt: wgpu::TextureFormat) -> CommentState {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("comments"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/comments.wgsl").into()),
        });
        let bgl = Self::uni_bind_group_layout(device);
        let (uni_buf, uni_bg) = Self::uni_buf_and_bg(device, &bgl);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("comments_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let comment_attrs = wgpu::vertex_attr_array![
            0 => Float32x2, // pos
            1 => Float32x2, // size
            2 => Float32x4, // fill_color
            3 => Float32x4, // border_color
            4 => Float32,   // corner_r
            5 => Uint32,    // flags
            6 => Uint32,    // _pad0
            7 => Uint32,    // _pad1
        ];
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CommentInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &comment_attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("comments_pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vbl],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(Self::alpha_blend_target(fmt))],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("comment_instances"),
            size: 256,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        CommentState {
            pipeline,
            uni_buf,
            uni_bg,
            inst_buf,
            inst_cap: 256,
        }
    }

    // ── grid pipeline ─────────────────────────────────────────────────────────
    fn create_grid(device: &wgpu::Device, fmt: wgpu::TextureFormat) -> GridState {
        let src = wgpu::ShaderModuleDescriptor {
            label: Some("grid"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/grid.wgsl").into()),
        };
        let shader = device.create_shader_module(src);
        let bgl = Self::uni_bind_group_layout(device);
        let (uni_buf, bind_group) = Self::uni_buf_and_bg(device, &bgl);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(Self::alpha_blend_target(fmt))],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        GridState {
            pipeline,
            uni_buf,
            bind_group,
        }
    }

    // ── nodes pipeline ────────────────────────────────────────────────────────
    fn create_nodes(device: &wgpu::Device, fmt: wgpu::TextureFormat) -> NodeState {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nodes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/nodes.wgsl").into()),
        });
        let bgl = Self::uni_bind_group_layout(device);
        let (uni_buf, uni_bg) = Self::uni_buf_and_bg(device, &bgl);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nodes_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        // Instance vertex buffer layout — 10 attributes from NodeInstance
        let node_attrs = wgpu::vertex_attr_array![
            0 => Float32x2,  // pos
            1 => Float32x2,  // size
            2 => Float32x4,  // header_color
            3 => Float32x4,  // body_color
            4 => Float32x4,  // border_color
            5 => Float32x4,  // sep_color
            6 => Float32,    // header_h_frac
            7 => Float32,    // corner_r
            8 => Uint32,     // flags
            9 => Uint32,     // _pad
        ];
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<NodeInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &node_attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("nodes"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vbl],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(Self::alpha_blend_target(fmt))],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let init_cap = 256 * std::mem::size_of::<NodeInstance>() as u64;
        let inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("node_inst"),
            size: init_cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        NodeState {
            pipeline,
            uni_buf,
            uni_bg,
            inst_buf,
            inst_cap: init_cap,
        }
    }

    // ── bezier wire pipeline (instanced) ─────────────────────────────────────
    fn create_bezier(device: &wgpu::Device, fmt: wgpu::TextureFormat) -> BezierState {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bezier"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/bezier.wgsl").into()),
        });
        let bgl = Self::uni_bind_group_layout(device);
        let (uni_buf, uni_bg) = Self::uni_buf_and_bg(device, &bgl);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bezier_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        // WireInstance layout:
        // from(2), ctrl1(2), ctrl2(2), to(2), color(4), thickness(1), flags(1u), pulse_phase(1), _pad(1)
        let attrs = wgpu::vertex_attr_array![
            0 => Float32x2,   // from      offset  0
            1 => Float32x2,   // ctrl1     offset  8
            2 => Float32x2,   // ctrl2     offset 16
            3 => Float32x2,   // to        offset 24
            4 => Float32x4,   // color     offset 32
            5 => Float32,     // thickness offset 48
            6 => Uint32,      // flags     offset 52
            7 => Float32,     // phase     offset 56
            8 => Float32,     // _pad      offset 60
        ];
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WireInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bezier"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vbl],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(Self::alpha_blend_target(fmt))],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let init_cap = 4096 * std::mem::size_of::<WireInstance>() as u64;
        let inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bezier_inst"),
            size: init_cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        BezierState {
            pipeline,
            uni_buf,
            uni_bg,
            inst_buf,
            inst_cap: init_cap,
        }
    }

    // ── straight-line pipeline (vertex buffer, selection box only) ────────────
    fn create_lines(device: &wgpu::Device, fmt: wgpu::TextureFormat) -> LineState {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lines"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/wires.wgsl").into()),
        });
        let bgl = Self::uni_bind_group_layout(device);
        let (uni_buf, uni_bg) = Self::uni_buf_and_bg(device, &bgl);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lines_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let attrs = wgpu::vertex_attr_array![
            0 => Float32x2,  // pos
            1 => Float32x2,  // uv
            2 => Float32x4,  // color
        ];
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WireVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lines"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vbl],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(Self::alpha_blend_target(fmt))],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let init_cap = 256 * std::mem::size_of::<WireVertex>() as u64;
        let vert_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("line_verts"),
            size: init_cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        LineState {
            pipeline,
            uni_buf,
            uni_bg,
            vert_buf,
            vert_cap: init_cap,
        }
    }

    // ── pins pipeline ─────────────────────────────────────────────────────────
    fn create_pins(device: &wgpu::Device, fmt: wgpu::TextureFormat) -> PinState {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pins"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/pins.wgsl").into()),
        });
        let bgl = Self::uni_bind_group_layout(device);
        let (uni_buf, uni_bg) = Self::uni_buf_and_bg(device, &bgl);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pins_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pin_attrs = wgpu::vertex_attr_array![
            0 => Float32x2,  // center
            1 => Float32,    // size
            2 => Float32,    // _pad0
            3 => Float32x4,  // color
            4 => Uint32,     // kind
            5 => Uint32,     // is_input
            6 => Uint32,     // compatible
            7 => Uint32,     // _pad1
        ];
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PinInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &pin_attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pins"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vbl],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(Self::alpha_blend_target(fmt))],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let init_cap = 1024 * std::mem::size_of::<PinInstance>() as u64;
        let inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pin_inst"),
            size: init_cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        PinState {
            pipeline,
            uni_buf,
            uni_bg,
            inst_buf,
            inst_cap: init_cap,
        }
    }
}
