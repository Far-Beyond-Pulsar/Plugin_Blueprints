// BpRenderer — owns all five WGPU render pipelines and per-frame GPU state.
//
// Pipelines:
//   grid   — one full-screen quad, uniform-only
//   nodes  — instanced node quads (6 verts × node count)
//   wires  — flat vertex buffer of CPU-tessellated bezier geometry
//   pins   — instanced pin quads (6 verts × pin count)
//   text   — glyph atlas, one quad per visible character

use super::text::{TextAlign, TextRenderer};
use super::types::{GraphUniforms, NodeInstance, PinInstance, SelectionInstance, WireVertex};

// ─── pipeline containers ──────────────────────────────────────────────────────

struct GridState {
    pipeline:   wgpu::RenderPipeline,
    uni_buf:    wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

struct NodeState {
    pipeline:    wgpu::RenderPipeline,
    uni_buf:     wgpu::Buffer,
    uni_bg:      wgpu::BindGroup,
    inst_buf:    wgpu::Buffer,
    inst_cap:    u64, // bytes
}

struct WireState {
    pipeline:  wgpu::RenderPipeline,
    uni_buf:   wgpu::Buffer,
    uni_bg:    wgpu::BindGroup,
    vert_buf:  wgpu::Buffer,
    vert_cap:  u64,
}

struct PinState {
    pipeline:  wgpu::RenderPipeline,
    uni_buf:   wgpu::Buffer,
    uni_bg:    wgpu::BindGroup,
    inst_buf:  wgpu::Buffer,
    inst_cap:  u64,
}

// ─── public renderer ──────────────────────────────────────────────────────────

pub struct BpRenderer {
    grid:  Option<GridState>,
    nodes: Option<NodeState>,
    wires: Option<WireState>,
    pins:  Option<PinState>,
    text:  TextRenderer,
}

impl BpRenderer {
    pub fn new() -> Self {
        Self { grid: None, nodes: None, wires: None, pins: None, text: TextRenderer::new() }
    }

    /// Called every frame by `graph.rs`.
    /// `text_calls`: (text, screen_x, screen_y, size_px, rgba_color, center_align)
    pub fn render_frame(
        &mut self,
        device:     &wgpu::Device,
        queue:      &wgpu::Queue,
        view:       &wgpu::TextureView,
        w: u32, h: u32,
        fmt:        wgpu::TextureFormat,
        uniforms:   &GraphUniforms,
        nodes:      &[NodeInstance],
        wires:      &[WireVertex],
        pins:       &[PinInstance],
        text_calls: &[(String, f32, f32, f32, [f32;4], bool)],
    ) {
        // Lazy init / re-init if format changed
        if self.grid.is_none() {
            self.grid  = Some(Self::create_grid(device, fmt));
            self.nodes = Some(Self::create_nodes(device, fmt));
            self.wires = Some(Self::create_wires(device, fmt));
            self.pins  = Some(Self::create_pins(device, fmt));
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
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.055, g: 0.055, b: 0.058, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // ── Grid ───────────────────────────────────────────────────────────
            if let Some(g) = &self.grid {
                queue.write_buffer(&g.uni_buf, 0, uni_bytes);
                pass.set_pipeline(&g.pipeline);
                pass.set_bind_group(0, &g.bind_group, &[]);
                pass.draw(0..6, 0..1);
            }

            // ── Nodes ──────────────────────────────────────────────────────────
            if !nodes.is_empty() {
                if let Some(ns) = &mut self.nodes {
                    queue.write_buffer(&ns.uni_buf, 0, uni_bytes);
                    let node_bytes = bytemuck::cast_slice(nodes);
                    Self::ensure_buf(device, &mut ns.inst_buf, &mut ns.inst_cap, node_bytes, wgpu::BufferUsages::VERTEX);
                    queue.write_buffer(&ns.inst_buf, 0, node_bytes);
                    pass.set_pipeline(&ns.pipeline);
                    pass.set_bind_group(0, &ns.uni_bg, &[]);
                    pass.set_vertex_buffer(0, ns.inst_buf.slice(..));
                    pass.draw(0..6, 0..nodes.len() as u32);
                }
            }

            // ── Wires ──────────────────────────────────────────────────────────
            if !wires.is_empty() {
                if let Some(ws) = &mut self.wires {
                    queue.write_buffer(&ws.uni_buf, 0, uni_bytes);
                    let wire_bytes = bytemuck::cast_slice(wires);
                    Self::ensure_buf(device, &mut ws.vert_buf, &mut ws.vert_cap, wire_bytes, wgpu::BufferUsages::VERTEX);
                    queue.write_buffer(&ws.vert_buf, 0, wire_bytes);
                    pass.set_pipeline(&ws.pipeline);
                    pass.set_bind_group(0, &ws.uni_bg, &[]);
                    pass.set_vertex_buffer(0, ws.vert_buf.slice(..));
                    pass.draw(0..wires.len() as u32, 0..1);
                }
            }

            // ── Pins ───────────────────────────────────────────────────────────
            if !pins.is_empty() {
                if let Some(ps) = &mut self.pins {
                    queue.write_buffer(&ps.uni_buf, 0, uni_bytes);
                    let pin_bytes = bytemuck::cast_slice(pins);
                    Self::ensure_buf(device, &mut ps.inst_buf, &mut ps.inst_cap, pin_bytes, wgpu::BufferUsages::VERTEX);
                    queue.write_buffer(&ps.inst_buf, 0, pin_bytes);
                    pass.set_pipeline(&ps.pipeline);
                    pass.set_bind_group(0, &ps.uni_bg, &[]);
                    pass.set_vertex_buffer(0, ps.inst_buf.slice(..));
                    pass.draw(0..6, 0..pins.len() as u32);
                }
            }

            // ── Text ───────────────────────────────────────────────────────────
            // Queue all text calls, then flush into this render pass.
            for (text, sx, sy, size, color, center) in text_calls {
                let align = if *center { TextAlign::Center } else { TextAlign::Left };
                self.text.queue(text, *sx, *sy, *size, *color, align);
            }
            // Need a shared uniform buffer/BGL for the text pipeline.
            // Lazily use the grid pipeline's uni_buf since it has the same layout.
            if let Some(ref g) = self.grid {
                // Ensure atlas is uploaded before flushing
                self.text.atlas.upload_if_needed(device, queue);
                // Rebuild text bind-group infra if needed (done inside flush)
                // We pass the grid's bgl+buf as the shared uniform binding.
                self.text.flush_with_external_uni(
                    device, queue, &mut pass, &g.uni_buf, fmt,
                );
            }
        } // end render pass

        queue.submit(std::iter::once(encoder.finish()));
    }

    // ── buffer helpers ────────────────────────────────────────────────────────

    /// Grow a buffer if it's too small.
    fn ensure_buf(
        device:  &wgpu::Device,
        buf:     &mut wgpu::Buffer,
        cap:     &mut u64,
        data:    &[u8],
        usage:   wgpu::BufferUsages,
    ) {
        let needed = data.len() as u64;
        if needed > *cap {
            *cap = (needed * 2).max(256);
            *buf = device.create_buffer(&wgpu::BufferDescriptor {
                label:              None,
                size:               *cap,
                usage:              usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
    }

    // ── pipeline creators ─────────────────────────────────────────────────────

    fn uni_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("bp_uni_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty:         wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   None,
                },
                count: None,
            }],
        })
    }

    fn uni_buf_and_bg(device: &wgpu::Device, bgl: &wgpu::BindGroupLayout) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("bp_uni"),
            size:               std::mem::size_of::<GraphUniforms>() as u64,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("bp_uni_bg"),
            layout:  bgl,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: buf.as_entire_binding(),
            }],
        });
        (buf, bg)
    }

    fn alpha_blend_target(fmt: wgpu::TextureFormat) -> wgpu::ColorTargetState {
        wgpu::ColorTargetState {
            format: fmt,
            blend:  Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        }
    }

    // ── grid pipeline ─────────────────────────────────────────────────────────
    fn create_grid(device: &wgpu::Device, fmt: wgpu::TextureFormat) -> GridState {
        let src = wgpu::ShaderModuleDescriptor {
            label:  Some("grid"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/grid.wgsl").into()),
        };
        let shader = device.create_shader_module(src);
        let bgl    = Self::uni_bind_group_layout(device);
        let (uni_buf, bind_group) = Self::uni_buf_and_bg(device, &bgl);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("grid_layout"),
            bind_group_layouts:   &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:         Some("grid"),
            layout:        Some(&layout),
            vertex:        wgpu::VertexState {
                module:      &shader,
                entry_point: Some("vs_main"),
                buffers:     &[],
                compilation_options: Default::default(),
            },
            fragment:      Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: Some("fs_main"),
                targets:     &[Some(Self::alpha_blend_target(fmt))],
                compilation_options: Default::default(),
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });
        GridState { pipeline, uni_buf, bind_group }
    }

    // ── nodes pipeline ────────────────────────────────────────────────────────
    fn create_nodes(device: &wgpu::Device, fmt: wgpu::TextureFormat) -> NodeState {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("nodes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/nodes.wgsl").into()),
        });
        let bgl = Self::uni_bind_group_layout(device);
        let (uni_buf, uni_bg) = Self::uni_buf_and_bg(device, &bgl);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("nodes_layout"),
            bind_group_layouts:   &[&bgl],
            push_constant_ranges: &[],
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
            step_mode:    wgpu::VertexStepMode::Instance,
            attributes:   &node_attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("nodes"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module:      &shader,
                entry_point: Some("vs_main"),
                buffers:     &[vbl],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: Some("fs_main"),
                targets:     &[Some(Self::alpha_blend_target(fmt))],
                compilation_options: Default::default(),
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        let init_cap = 256 * std::mem::size_of::<NodeInstance>() as u64;
        let inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("node_inst"),
            size:               init_cap,
            usage:              wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        NodeState { pipeline, uni_buf, uni_bg, inst_buf, inst_cap: init_cap }
    }

    // ── wires pipeline ────────────────────────────────────────────────────────
    fn create_wires(device: &wgpu::Device, fmt: wgpu::TextureFormat) -> WireState {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("wires"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/wires.wgsl").into()),
        });
        let bgl = Self::uni_bind_group_layout(device);
        let (uni_buf, uni_bg) = Self::uni_buf_and_bg(device, &bgl);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("wires_layout"),
            bind_group_layouts:   &[&bgl],
            push_constant_ranges: &[],
        });

        let wire_attrs = wgpu::vertex_attr_array![
            0 => Float32x2,  // pos
            1 => Float32x2,  // uv
            2 => Float32x4,  // color
        ];
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WireVertex>() as wgpu::BufferAddress,
            step_mode:    wgpu::VertexStepMode::Vertex,
            attributes:   &wire_attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("wires"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module:      &shader,
                entry_point: Some("vs_main"),
                buffers:     &[vbl],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: Some("fs_main"),
                targets:     &[Some(Self::alpha_blend_target(fmt))],
                compilation_options: Default::default(),
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        let init_cap = 4096 * std::mem::size_of::<WireVertex>() as u64;
        let vert_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("wire_verts"),
            size:               init_cap,
            usage:              wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        WireState { pipeline, uni_buf, uni_bg, vert_buf, vert_cap: init_cap }
    }

    // ── pins pipeline ─────────────────────────────────────────────────────────
    fn create_pins(device: &wgpu::Device, fmt: wgpu::TextureFormat) -> PinState {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("pins"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/pins.wgsl").into()),
        });
        let bgl = Self::uni_bind_group_layout(device);
        let (uni_buf, uni_bg) = Self::uni_buf_and_bg(device, &bgl);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("pins_layout"),
            bind_group_layouts:   &[&bgl],
            push_constant_ranges: &[],
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
            step_mode:    wgpu::VertexStepMode::Instance,
            attributes:   &pin_attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("pins"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module:      &shader,
                entry_point: Some("vs_main"),
                buffers:     &[vbl],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: Some("fs_main"),
                targets:     &[Some(Self::alpha_blend_target(fmt))],
                compilation_options: Default::default(),
            }),
            primitive:     wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        let init_cap = 1024 * std::mem::size_of::<PinInstance>() as u64;
        let inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("pin_inst"),
            size:               init_cap,
            usage:              wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        PinState { pipeline, uni_buf, uni_bg, inst_buf, inst_cap: init_cap }
    }
}
