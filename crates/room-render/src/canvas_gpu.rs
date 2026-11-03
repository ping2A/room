//! GPU upload / blit of a CPU [`Canvas`] RGBA overlay.

use crate::canvas::{Canvas, CanvasCmd};
use crate::device::GpuContext;
use crate::fonts::{Fon1Face, TextDraw};

const OVERLAY_WGSL: &str = r#"
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    // Fullscreen triangle.
    var pos = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    var uv = array<vec2<f32>, 3>(
        vec2(0.0, 1.0),
        vec2(2.0, 1.0),
        vec2(0.0, -1.0),
    );
    var o: VsOut;
    o.clip = vec4(pos[i], 0.0, 1.0);
    o.uv = uv[i];
    return o;
}

@fragment
fn fs_main(v: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, v.uv);
    // Holes stay holes. Partial alpha must blend: the pain flash is a red
    // fill around a=0.1..0.9, and forcing it to 1 painted a solid red sheet.
    if (c.a < 0.02) {
        discard;
    }
    return c;
}
"#;

/// CPU RGBA scratch + GPU texture for 2D overlay.
pub struct CanvasGpu {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    texture: wgpu::Texture,
    /// Kept alive for `bind`.
    _view: wgpu::TextureView,
    /// Kept alive for `bind`.
    _sampler: wgpu::Sampler,
    bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl CanvasGpu {
    /// Create an overlay matching the swapchain size.
    pub fn new(gpu: &GpuContext, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let device = &gpu.device;
        // Doom PLAYPAL bytes are display-referred. On an sRGB swapchain, store the
        // overlay as sRGB so sample→linear→encode round-trips. On a non-sRGB
        // WebGPU canvas (`Bgra8Unorm`), an sRGB overlay texture would decode to
        // linear and be shown un-encoded (STBAR / weapon look crushed-dark).
        let overlay_format = if format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("canvas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: overlay_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canvas-bgl"),
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
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas-overlay"),
            source: wgpu::ShaderSource::Wgsl(OVERLAY_WGSL.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("canvas-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("canvas-pipe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        Self {
            width,
            height,
            rgba: vec![0u8; (width * height * 4) as usize],
            texture,
            _view: view,
            _sampler: sampler,
            bind,
            pipeline,
        }
    }

    /// Rasterize canvas commands (optionally with a FON1 face for text).
    pub fn rasterize(&mut self, canvas: &Canvas, font: Option<&Fon1Face>) {
        self.rgba.fill(0);
        for cmd in &canvas.cmds {
            match cmd {
                CanvasCmd::Clear { color } => {
                    let c = rgba8(*color);
                    for px in self.rgba.chunks_exact_mut(4) {
                        px.copy_from_slice(&c);
                    }
                }
                CanvasCmd::FillRect { x, y, w, h, color } => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x0 = (*x as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y0 = (*y as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x1 = ((*x + *w) as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y1 = ((*y + *h) as i32).max(0) as u32;
                    let sr = color[0].clamp(0.0, 1.0);
                    let sg = color[1].clamp(0.0, 1.0);
                    let sb = color[2].clamp(0.0, 1.0);
                    let sa = color[3].clamp(0.0, 1.0);
                    for py in y0..y1.min(self.height) {
                        for px in x0..x1.min(self.width) {
                            let o = ((py * self.width + px) * 4) as usize;
                            if sa >= 0.999 {
                                self.rgba[o..o + 4].copy_from_slice(&rgba8(*color));
                                continue;
                            }
                            if sa <= 0.001 {
                                continue;
                            }
                            // Straight alpha over (src over dst) in the overlay buffer.
                            let dr = f32::from(self.rgba[o]) / 255.0;
                            let dg = f32::from(self.rgba[o + 1]) / 255.0;
                            let db = f32::from(self.rgba[o + 2]) / 255.0;
                            let da = f32::from(self.rgba[o + 3]) / 255.0;
                            let out_a = sa + da * (1.0 - sa);
                            let out_r = if out_a > 0.0 {
                                (sr * sa + dr * da * (1.0 - sa)) / out_a
                            } else {
                                0.0
                            };
                            let out_g = if out_a > 0.0 {
                                (sg * sa + dg * da * (1.0 - sa)) / out_a
                            } else {
                                0.0
                            };
                            let out_b = if out_a > 0.0 {
                                (sb * sa + db * da * (1.0 - sa)) / out_a
                            } else {
                                0.0
                            };
                            self.rgba[o..o + 4]
                                .copy_from_slice(&rgba8([out_r, out_g, out_b, out_a]));
                        }
                    }
                }
                CanvasCmd::BlitRgba { x, y, w, h, pixels } => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x0 = (*x as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y0 = (*y as i32).max(0) as u32;
                    let src_w = *w;
                    let src_h = *h;
                    let need = (src_w as usize)
                        .saturating_mul(src_h as usize)
                        .saturating_mul(4);
                    if src_w == 0 || src_h == 0 || pixels.len() < need {
                        continue;
                    }
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x_off = (*x as i32).min(0).unsigned_abs();
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y_off = (*y as i32).min(0).unsigned_abs();
                    for row in y_off..src_h {
                        let dy = y0 + (row - y_off);
                        if dy >= self.height {
                            break;
                        }
                        for col in x_off..src_w {
                            let dx = x0 + (col - x_off);
                            if dx >= self.width {
                                break;
                            }
                            let si = ((row * src_w + col) * 4) as usize;
                            let a = pixels[si + 3];
                            if a < 128 {
                                continue;
                            }
                            let o = ((dy * self.width + dx) * 4) as usize;
                            self.rgba[o..o + 4].copy_from_slice(&pixels[si..si + 4]);
                        }
                    }
                }
                CanvasCmd::Text { x, y, text, color } => {
                    if let Some(face) = font {
                        let c = rgba8(*color);
                        #[allow(clippy::cast_possible_truncation)]
                        let mut dest = TextDraw {
                            rgba: &mut self.rgba,
                            buf_w: self.width,
                            buf_h: self.height,
                            x: *x as i32,
                            y: *y as i32,
                            color: c,
                        };
                        face.draw_text(&mut dest, text);
                    } else {
                        // Last-resort 5×7 bitmap so HUD/menu never go fully blank
                        // when SoftFont failed to resolve (prefer Patch STTNUM/STCFN).
                        #[allow(clippy::cast_possible_truncation)]
                        paint_fallback_text(
                            &mut self.rgba,
                            self.width,
                            self.height,
                            *x as i32,
                            *y as i32,
                            text,
                            rgba8(*color),
                        );
                    }
                }
            }
        }
    }

    /// Upload CPU pixels and draw the overlay (no depth).
    pub fn blit(&self, gpu: &GpuContext, pass: &mut wgpu::RenderPass<'_>) {
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn rgba8(c: [f32; 4]) -> [u8; 4] {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

/// Tiny 5×7 uppercase bitmap for `CanvasCmd::Text` when no FON1 face is bound.
fn paint_fallback_text(
    rgba: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    origin_x: i32,
    origin_y: i32,
    text: &str,
    color: [u8; 4],
) {
    // Columns packed in low 5 bits, 7 rows; bit0 = leftmost.
    fn glyph(ch: u8) -> [u8; 7] {
        match ch.to_ascii_uppercase() {
            b'0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
            b'1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
            b'2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
            b'3' => [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E],
            b'4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
            b'5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
            b'6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
            b'7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
            b'8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
            b'9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
            b'%' => [0x19, 0x1A, 0x04, 0x08, 0x13, 0x03, 0x00],
            b'-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
            b'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
            b'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
            b'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
            b'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
            b'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
            b'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
            b'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
            b'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
            b'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
            b'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
            b'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
            b'M' => [0x11, 0x1B, 0x15, 0x11, 0x11, 0x11, 0x11],
            _ => [0x00; 7],
        }
    }
    let mut cx = origin_x;
    for ch in text.bytes() {
        if ch == b' ' {
            cx += 6;
            continue;
        }
        let rows = glyph(ch);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) == 0 {
                    continue;
                }
                let px = cx + col;
                let py = origin_y + row as i32;
                if px < 0 || py < 0 || px >= buf_w as i32 || py >= buf_h as i32 {
                    continue;
                }
                let o = ((py as u32 * buf_w + px as u32) * 4) as usize;
                if o + 4 <= rgba.len() {
                    rgba[o..o + 4].copy_from_slice(&color);
                }
            }
        }
        cx += 6;
    }
}
