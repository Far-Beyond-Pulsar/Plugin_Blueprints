// GPU glyph atlas text renderer.
//
// Architecture:
//   GlyphAtlas  — packs fontdue-rasterised coverage bitmaps into a 2048×2048
//                 R8Unorm GPU texture.  New glyphs are appended on demand;
//                 the texture is re-uploaded only when the atlas grows.
//
//   TextRenderer — accepts screen-space draw calls, generates a flat vertex
//                  buffer of glyph quads, and issues a single draw call with
//                  the atlas texture bound.
//
// Text position is in surface-pixel coordinates (after graph→screen transform),
// so the vertex shader only needs viewport→NDC conversion.

use std::collections::HashMap;
use wgpu::util::DeviceExt;

// ─── glyph atlas ──────────────────────────────────────────────────────────────

const ATLAS_W: u32 = 2048;
const ATLAS_H: u32 = 2048;

#[derive(Clone, Copy)]
struct GlyphSlot {
    /// UV rect inside the atlas (0..1)
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    /// Glyph metrics in pixels at the rasterised size
    width: u32,
    height: u32,
    bearing_x: f32,
    bearing_y: f32,
    advance: f32,
}

/// Key: (Unicode scalar, font-size quantised to 0.5 px steps)
type GlyphKey = (u32, u32);

pub struct GlyphAtlas {
    font: fontdue::Font,
    slots: HashMap<GlyphKey, GlyphSlot>,
    /// CPU-side atlas image (R8, row-major)
    data: Vec<u8>,
    cursor_x: u32,
    cursor_y: u32,
    row_h: u32,

    // GPU objects (created lazily)
    pub texture: Option<wgpu::Texture>,
    pub texture_view: Option<wgpu::TextureView>,
    pub sampler: Option<wgpu::Sampler>,
    pub dirty: bool,
}

impl GlyphAtlas {
    pub fn new() -> Self {
        let font_data = Self::load_font();
        let font = fontdue::Font::from_bytes(font_data, fontdue::FontSettings::default())
            .expect("failed to parse embedded font");
        Self {
            font,
            slots: HashMap::new(),
            data: vec![0u8; (ATLAS_W * ATLAS_H) as usize],
            cursor_x: 1, // leave 1px border so clamped UV never hits edge
            cursor_y: 1,
            row_h: 0,
            texture: None,
            texture_view: None,
            sampler: None,
            dirty: true,
        }
    }

    /// Get or rasterize a glyph, returning its atlas slot.
    pub fn glyph(&mut self, ch: char, size_px: f32) -> Option<GlyphSlot> {
        let key: GlyphKey = (ch as u32, (size_px * 2.0).round() as u32);
        if let Some(&s) = self.slots.get(&key) {
            return Some(s);
        }

        // Rasterize with fontdue
        let (metrics, bitmap) = self.font.rasterize(ch, size_px);
        if metrics.width == 0 || metrics.height == 0 {
            // Whitespace / missing glyph — store a zero-size slot for the advance
            let slot = GlyphSlot {
                uv_min: [0.0; 2],
                uv_max: [0.0; 2],
                width: 0,
                height: 0,
                bearing_x: metrics.xmin as f32,
                bearing_y: metrics.ymin as f32,
                advance: metrics.advance_width,
            };
            self.slots.insert(key, slot);
            return Some(slot);
        }

        let gw = metrics.width as u32;
        let gh = metrics.height as u32;
        let pad = 1u32;

        // Row-advance if needed
        if self.cursor_x + gw + pad > ATLAS_W {
            self.cursor_y += self.row_h + pad;
            self.cursor_x = 1;
            self.row_h = 0;
        }
        if self.cursor_y + gh + pad > ATLAS_H {
            // Atlas full — return None (caller renders nothing)
            return None;
        }

        // Blit coverage bitmap into atlas
        for row in 0..gh {
            let src_off = (row * gw) as usize;
            let dst_off = ((self.cursor_y + row) * ATLAS_W + self.cursor_x) as usize;
            self.data[dst_off..dst_off + gw as usize]
                .copy_from_slice(&bitmap[src_off..src_off + gw as usize]);
        }
        self.row_h = self.row_h.max(gh);

        let slot = GlyphSlot {
            uv_min: [
                self.cursor_x as f32 / ATLAS_W as f32,
                self.cursor_y as f32 / ATLAS_H as f32,
            ],
            uv_max: [
                (self.cursor_x + gw) as f32 / ATLAS_W as f32,
                (self.cursor_y + gh) as f32 / ATLAS_H as f32,
            ],
            width: gw,
            height: gh,
            bearing_x: metrics.xmin as f32,
            bearing_y: metrics.ymin as f32,
            advance: metrics.advance_width,
        };

        self.cursor_x += gw + pad;
        self.dirty = true;
        self.slots.insert(key, slot);
        Some(slot)
    }

    /// Measure a string's pixel width at a given size.
    pub fn measure_width(&mut self, text: &str, size_px: f32) -> f32 {
        text.chars().fold(0.0, |acc, ch| {
            acc + self.glyph(ch, size_px).map_or(0.0, |s| s.advance)
        })
    }

    /// Upload atlas to GPU if dirty.
    pub fn upload_if_needed(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.texture.is_none() {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("glyph_atlas"),
                size: wgpu::Extent3d {
                    width: ATLAS_W,
                    height: ATLAS_H,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.texture_view = Some(tex.create_view(&Default::default()));
            self.texture = Some(tex);
            self.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("glyph_sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }));
        }

        if self.dirty {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: self.texture.as_ref().unwrap(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &self.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ATLAS_W),
                    rows_per_image: Some(ATLAS_H),
                },
                wgpu::Extent3d {
                    width: ATLAS_W,
                    height: ATLAS_H,
                    depth_or_array_layers: 1,
                },
            );
            self.dirty = false;
        }
    }

    /// Load a reasonable system font or fall back to something minimal.
    fn load_font() -> &'static [u8] {
        // Try OS-specific fonts; if all fail we embed a minimal one below.
        #[cfg(target_os = "macos")]
        {
            let paths = [
                "/System/Library/Fonts/Helvetica.ttc",
                "/Library/Fonts/Arial.ttf",
                "/System/Library/Fonts/SFNSText.ttf",
            ];
            for p in paths {
                if let Ok(data) = std::fs::read(p) {
                    // Safety: we leak the vec to get a 'static ref — acceptable for a
                    // single-process lifetime singleton.
                    return Box::leak(data.into_boxed_slice());
                }
            }
        }
        #[cfg(target_os = "windows")]
        {
            let paths = [
                r"C:\Windows\Fonts\segoeui.ttf",
                r"C:\Windows\Fonts\arial.ttf",
                r"C:\Windows\Fonts\calibri.ttf",
            ];
            for p in paths {
                if let Ok(data) = std::fs::read(p) {
                    return Box::leak(data.into_boxed_slice());
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            let paths = [
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
                "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            ];
            for p in paths {
                if let Ok(data) = std::fs::read(p) {
                    return Box::leak(data.into_boxed_slice());
                }
            }
        }

        // Last-resort: included subset — compiled in via env!() trick at build time
        // so the binary always works even on unusual setups.
        include_bytes!("fallback_font.ttf")
    }
}

// ─── text vertex ─────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextVertex {
    pub pos: [f32; 2], // screen pixels
    pub uv: [f32; 2],  // atlas UV
    pub color: [f32; 4],
}

// ─── text renderer ────────────────────────────────────────────────────────────

pub enum TextAlign {
    Left,
    Center,
    Right,
}

pub struct TextRenderer {
    pub atlas: GlyphAtlas,
    verts: Vec<TextVertex>,
    pipeline: Option<wgpu::RenderPipeline>,
    vert_buf: Option<wgpu::Buffer>,
    vert_cap: u64,
    atlas_bg: Option<wgpu::BindGroup>,
    atlas_bgl: Option<wgpu::BindGroupLayout>,
    uni_bgl: Option<wgpu::BindGroupLayout>,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            atlas: GlyphAtlas::new(),
            verts: Vec::new(),
            pipeline: None,
            vert_buf: None,
            vert_cap: 0,
            atlas_bg: None,
            atlas_bgl: None,
            uni_bgl: None,
        }
    }

    /// Queue a text string to be rendered this frame.
    /// `screen_x/y` is the baseline origin in surface-pixel coordinates.
    /// Call this before `flush()`.
    pub fn queue(
        &mut self,
        text: &str,
        screen_x: f32,
        screen_y: f32,
        size_px: f32,
        color: [f32; 4],
        align: TextAlign,
    ) {
        if text.is_empty() || size_px < 2.0 {
            return;
        }

        // Measure for alignment offset
        let total_w = self.atlas.measure_width(text, size_px);
        let x_off = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => -total_w * 0.5,
            TextAlign::Right => -total_w,
        };

        let mut cx = screen_x + x_off;

        for ch in text.chars() {
            let Some(slot) = self.atlas.glyph(ch, size_px) else {
                continue;
            };
            if slot.width == 0 {
                cx += slot.advance;
                continue;
            }

            let x0 = cx + slot.bearing_x;
            let y0 = screen_y - slot.bearing_y - slot.height as f32;
            let x1 = x0 + slot.width as f32;
            let y1 = y0 + slot.height as f32;
            let [u0, v0] = slot.uv_min;
            let [u1, v1] = slot.uv_max;

            // Two triangles (CCW)
            let verts = [
                TextVertex {
                    pos: [x0, y0],
                    uv: [u0, v0],
                    color,
                },
                TextVertex {
                    pos: [x1, y0],
                    uv: [u1, v0],
                    color,
                },
                TextVertex {
                    pos: [x0, y1],
                    uv: [u0, v1],
                    color,
                },
                TextVertex {
                    pos: [x0, y1],
                    uv: [u0, v1],
                    color,
                },
                TextVertex {
                    pos: [x1, y0],
                    uv: [u1, v0],
                    color,
                },
                TextVertex {
                    pos: [x1, y1],
                    uv: [u1, v1],
                    color,
                },
            ];
            self.verts.extend_from_slice(&verts);

            cx += slot.advance;
        }
    }

    /// Upload queued geometry + atlas, issue draw call, clear queue.
    pub fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass,
        uni_buf: &wgpu::Buffer,
        uni_bgl: &wgpu::BindGroupLayout,
        uni_bg: &wgpu::BindGroup,
        fmt: wgpu::TextureFormat,
    ) {
        if self.verts.is_empty() {
            return;
        }

        self.atlas.upload_if_needed(device, queue);

        // (Re)create atlas bind group if atlas texture changed
        let need_atlas_bg = self.atlas_bg.is_none() || self.atlas.dirty; // after upload dirty=false but we check before upload
                                                                         // Actually check view exists and bg is stale:
        self.rebuild_atlas_bg_if_needed(device);

        if self.pipeline.is_none() {
            self.pipeline = Some(Self::create_pipeline(
                device,
                fmt,
                uni_bgl,
                self.atlas_bgl.as_ref().unwrap(),
            ));
        }

        // Upload vertex buffer
        let bytes = bytemuck::cast_slice(&self.verts);
        let needed = bytes.len() as u64;
        if needed > self.vert_cap {
            self.vert_cap = (needed * 2).max(4096);
            self.vert_buf = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text_verts"),
                size: self.vert_cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if let Some(vb) = &self.vert_buf {
            queue.write_buffer(vb, 0, bytes);
        }

        // Draw
        if let (Some(pipeline), Some(vb), Some(atlas_bg)) =
            (&self.pipeline, &self.vert_buf, &self.atlas_bg)
        {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, uni_bg, &[]);
            pass.set_bind_group(1, atlas_bg, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.draw(0..self.verts.len() as u32, 0..1);
        }

        self.verts.clear();
    }

    fn rebuild_atlas_bg_if_needed(&mut self, device: &wgpu::Device) {
        if self.atlas_bgl.is_none() {
            self.atlas_bgl = Some(device.create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("atlas_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                },
            ));
        }

        if self.atlas_bg.is_none() {
            if let (Some(view), Some(sampler)) = (&self.atlas.texture_view, &self.atlas.sampler) {
                self.atlas_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("atlas_bg"),
                    layout: self.atlas_bgl.as_ref().unwrap(),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                }));
            }
        }
    }

    fn create_pipeline(
        device: &wgpu::Device,
        fmt: wgpu::TextureFormat,
        uni_bgl: &wgpu::BindGroupLayout,
        atlas_bgl: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text_layout"),
            bind_group_layouts: &[Some(uni_bgl), Some(atlas_bgl)],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![
            0 => Float32x2, // pos
            1 => Float32x2, // uv
            2 => Float32x4, // color
        ];
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TextVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attrs,
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text"),
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
                targets: &[Some(wgpu::ColorTargetState {
                    format: fmt,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    /// Variant used by BpRenderer: the uniform buffer is shared with other pipelines.
    /// A fresh bind-group is created from the external buffer using our own BGL.
    pub fn flush_with_external_uni(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass,
        uni_buf: &wgpu::Buffer,
        fmt: wgpu::TextureFormat,
    ) {
        if self.verts.is_empty() {
            return;
        }

        self.rebuild_atlas_bg_if_needed(device);

        // Build a uniform BGL + BG matching the text shader's group(0)
        if self.uni_bgl.is_none() {
            self.uni_bgl = Some(device.create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("text_uni_bgl"),
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
                },
            ));
        }

        let uni_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text_uni_bg_ext"),
            layout: self.uni_bgl.as_ref().unwrap(),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uni_buf.as_entire_binding(),
            }],
        });

        if self.pipeline.is_none() {
            self.pipeline = Some(Self::create_pipeline(
                device,
                fmt,
                self.uni_bgl.as_ref().unwrap(),
                self.atlas_bgl.as_ref().unwrap(),
            ));
        }

        let bytes = bytemuck::cast_slice(&self.verts);
        let needed = bytes.len() as u64;
        if needed > self.vert_cap {
            self.vert_cap = (needed * 2).max(4096);
            self.vert_buf = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text_verts"),
                size: self.vert_cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if let Some(vb) = &self.vert_buf {
            queue.write_buffer(vb, 0, bytes);
        }

        if let (Some(pipeline), Some(vb), Some(atlas_bg)) =
            (&self.pipeline, &self.vert_buf, &self.atlas_bg)
        {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &uni_bg, &[]);
            pass.set_bind_group(1, atlas_bg, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.draw(0..self.verts.len() as u32, 0..1);
        }

        self.verts.clear();
    }
}
