//! HUD pictures drawn on the GPU (the weapon and its overlays) and the
//! full-screen colour tints (pain / pickup flashes, `FadeTo`).
//!
//! The CPU canvas ([`crate::CanvasGpu`]) scaled the weapon to screen size,
//! blended it into a screen-sized buffer and uploaded the rows it covered on
//! every frame it moved, and a pain flash made it blend and upload the whole
//! screen. On a phone that was most of a frame. Here each picture is
//! uploaded once at its own size and drawn as a quad; the shader reads the
//! same texels and does the same byte arithmetic as the CPU path, so the
//! pixels match.

use crate::device::GpuContext;
use std::collections::HashMap;

const WGSL: &str = r#"
struct Quad {
    // x, y, w, h (framebuffer px)
    rect: vec4<i32>,
    // texture w, h, flags (1 flip, 2 mirror, 4 smooth), unused
    src: vec4<u32>,
    // light, alpha, tint r, tint g
    mul: vec4<f32>,
    // tint b, screen w, screen h, 1 = encode for an sRGB target
    more: vec4<f32>,
};

@group(0) @binding(0) var<uniform> q: Quad;
@group(1) @binding(0) var tex: texture_2d<f32>;

@vertex
fn vs_quad(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var corner = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    let c = corner[i];
    let px = f32(q.rect.x) + c.x * f32(q.rect.z);
    let py = f32(q.rect.y) + c.y * f32(q.rect.w);
    return vec4(px / q.more.y * 2.0 - 1.0, 1.0 - py / q.more.z * 2.0, 0.0, 1.0);
}

fn to_linear(c: f32) -> f32 {
    if (c <= 0.04045) {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

@fragment
fn fs_quad(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(pos.xy)) - q.rect.xy;
    if (p.x < 0 || p.y < 0 || p.x >= q.rect.z || p.y >= q.rect.w) {
        discard;
    }
    if ((q.src.z & 4u) != 0u) {
        return sample_smooth(p);
    }
    var dx = u32(p.x);
    var dy = u32(p.y);
    let w = u32(q.rect.z);
    let h = u32(q.rect.w);
    if ((q.src.z & 1u) != 0u) {
        dx = w - 1u - dx;
    }
    if ((q.src.z & 2u) != 0u) {
        dy = h - 1u - dy;
    }
    // `SoftPatch::scale_rgba`: texel = dest * src / dest_size.
    let sx = dx * q.src.x / w;
    let sy = dy * q.src.y / h;
    let t = round(textureLoad(tex, vec2<u32>(sx, sy), 0) * 255.0);
    if (t.a < 0.5) {
        discard;
    }
    // Byte arithmetic of the CPU path: light, then tint, each truncated.
    var rgb = floor(t.rgb * q.mul.x);
    rgb = floor(rgb * vec3(q.mul.z, q.mul.w, q.more.x));
    let a = floor(t.a * q.mul.y);
    // The canvas overlay drops nearly transparent pixels.
    if (a / 255.0 < 0.02) {
        discard;
    }
    var c = rgb / 255.0;
    if (q.more.w > 0.5) {
        c = vec3(to_linear(c.r), to_linear(c.g), to_linear(c.b));
    }
    return vec4(c, a / 255.0);
}

// Bilinear sampling (large pictures stretched over the screen: launcher
// backdrops), alpha times `mul.y`.
fn sample_smooth(p: vec2<i32>) -> vec4<f32> {
    let src = vec2<f32>(f32(q.src.x), f32(q.src.y));
    let f = (vec2<f32>(p) + 0.5) * src / vec2<f32>(f32(q.rect.z), f32(q.rect.w)) - 0.5;
    let fc = clamp(f, vec2(0.0), src - 1.0);
    let i0 = vec2<u32>(floor(fc));
    let i1 = min(i0 + 1u, vec2<u32>(q.src.xy) - 1u);
    let t = fc - floor(fc);
    let a = mix(textureLoad(tex, i0, 0), textureLoad(tex, vec2(i1.x, i0.y), 0), t.x);
    let b = mix(textureLoad(tex, vec2(i0.x, i1.y), 0), textureLoad(tex, i1, 0), t.x);
    let c = mix(a, b, t.y);
    var rgb = c.rgb;
    if (q.more.w > 0.5) {
        rgb = vec3(to_linear(rgb.r), to_linear(rgb.g), to_linear(rgb.b));
    }
    return vec4(rgb, c.a * q.mul.y);
}

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(pos[i], 0.0, 1.0);
}

@fragment
fn fs_tint() -> @location(0) vec4<f32> {
    return q.mul;
}
"#;

/// Gives a picture's RGBA at its own size, by texture key.
pub type FetchPicture<'a> = dyn FnMut(&str) -> Option<(u32, u32, Vec<u8>)> + 'a;

/// Quads one frame can draw (the weapon, its flash and overlay layers).
pub const MAX_QUADS: usize = 64;

/// Frames a picture may go undrawn before its texture is dropped.
const KEEP_FRAMES: u64 = 600;

/// One picture to draw, in framebuffer pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteQuad {
    /// Texture key (picture name plus palette variant).
    pub key: String,
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
    /// Light multiplier.
    pub light: f32,
    /// RGB tint multiplier.
    pub tint: [f32; 3],
    /// Alpha multiplier.
    pub alpha: f32,
    /// Mirror left to right.
    pub flip: bool,
    /// Mirror top to bottom.
    pub mirror: bool,
    /// Bilinear sampling (light and tint ignored; alpha multiplies).
    pub smooth: bool,
}

struct Picture {
    /// Kept alive for `bind`.
    _texture: wgpu::Texture,
    bind: wgpu::BindGroup,
    width: u32,
    height: u32,
    used: u64,
}

/// GPU HUD pictures and screen tints.
pub struct HudSprites {
    quad_pipeline: wgpu::RenderPipeline,
    tint_pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    uniform_bind: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    stride: u64,
    srgb: bool,
    pictures: HashMap<String, Picture>,
    frame: u64,
    /// Prepared draws: picture key and uniform offset.
    draws: Vec<(String, u32)>,
    /// Uniform offset of the prepared tint.
    tint: Option<u32>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadUniform {
    rect: [i32; 4],
    src: [u32; 4],
    mul: [f32; 4],
    more: [f32; 4],
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

impl HudSprites {
    /// Pipelines drawing onto `target` (the HUD pass's colour format).
    #[must_use]
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hud-sprites-quad"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<QuadUniform>() as u64
                    ),
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hud-sprites-texture"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let size = std::mem::size_of::<QuadUniform>() as u64;
        let stride = size.div_ceil(align) * align;
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hud-sprites-uniform"),
            size: stride * (MAX_QUADS as u64 + 1),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hud-sprites-uniform"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform,
                    offset: 0,
                    size: wgpu::BufferSize::new(size),
                }),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hud-sprites"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let quad_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hud-sprites-quad"),
            bind_group_layouts: &[&uniform_layout, &texture_layout],
            push_constant_ranges: &[],
        });
        let tint_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hud-sprites-tint"),
            bind_group_layouts: &[&uniform_layout],
            push_constant_ranges: &[],
        });
        let pipeline = |label, layout: &wgpu::PipelineLayout, vs, fs| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let quad_pipeline = pipeline("hud-sprites-quad", &quad_layout, "vs_quad", "fs_quad");
        let tint_pipeline = pipeline("hud-sprites-tint", &tint_layout, "vs_full", "fs_tint");
        Self {
            quad_pipeline,
            tint_pipeline,
            uniform,
            uniform_bind,
            texture_layout,
            stride,
            srgb: target.is_srgb(),
            pictures: HashMap::new(),
            frame: 0,
            draws: Vec::new(),
            tint: None,
        }
    }

    /// Drop every uploaded picture (new game data or palette).
    pub fn clear(&mut self) {
        self.pictures.clear();
    }

    /// Pictures currently uploaded.
    #[must_use]
    pub fn picture_count(&self) -> usize {
        self.pictures.len()
    }

    /// Upload missing pictures and write the uniforms for `quads` (at most
    /// [`MAX_QUADS`], drawn in order) and an optional full-screen `tint`
    /// (straight RGBA 0–1, display-referred like the canvas). `fetch` gives
    /// a picture's RGBA at its own size the first time a key is seen.
    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        screen: (u32, u32),
        quads: &[SpriteQuad],
        tint: Option<[f32; 4]>,
        fetch: &mut FetchPicture<'_>,
    ) {
        self.frame += 1;
        self.draws.clear();
        self.tint = None;
        let mut data: Vec<u8> = Vec::new();
        let stride = self.stride as usize;
        let slot = |data: &mut Vec<u8>, u: QuadUniform| -> u32 {
            let offset = data.len();
            data.extend_from_slice(bytemuck::bytes_of(&u));
            data.resize(offset + stride, 0);
            offset as u32
        };
        for quad in quads.iter().take(MAX_QUADS) {
            if quad.w == 0 || quad.h == 0 {
                continue;
            }
            if !self.pictures.contains_key(&quad.key) {
                let Some((w, h, rgba)) = fetch(&quad.key) else {
                    continue;
                };
                if w == 0 || h == 0 || rgba.len() < (w as usize) * (h as usize) * 4 {
                    continue;
                }
                let picture = self.upload(gpu, w, h, &rgba);
                self.pictures.insert(quad.key.clone(), picture);
            }
            let Some(picture) = self.pictures.get_mut(&quad.key) else {
                continue;
            };
            picture.used = self.frame;
            let flags = u32::from(quad.flip)
                | (u32::from(quad.mirror) << 1)
                | (u32::from(quad.smooth) << 2);
            let offset = slot(
                &mut data,
                QuadUniform {
                    rect: [quad.x, quad.y, quad.w as i32, quad.h as i32],
                    src: [picture.width, picture.height, flags, 0],
                    mul: [quad.light, quad.alpha, quad.tint[0], quad.tint[1]],
                    more: [
                        quad.tint[2],
                        screen.0.max(1) as f32,
                        screen.1.max(1) as f32,
                        if self.srgb { 1.0 } else { 0.0 },
                    ],
                },
            );
            self.draws.push((quad.key.clone(), offset));
        }
        if let Some(c) = tint {
            // Quantize like the canvas (bytes), drop what the overlay drops,
            // and decode to linear for an sRGB target.
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).floor() / 255.0;
            let a = byte(c[3]);
            if a >= 0.02 {
                let rgb = [byte(c[0]), byte(c[1]), byte(c[2])];
                let rgb = if self.srgb {
                    rgb.map(srgb_to_linear)
                } else {
                    rgb
                };
                let offset = slot(
                    &mut data,
                    QuadUniform {
                        rect: [0; 4],
                        src: [0; 4],
                        mul: [rgb[0], rgb[1], rgb[2], a],
                        more: [0.0; 4],
                    },
                );
                self.tint = Some(offset);
            }
        }
        if !data.is_empty() {
            gpu.queue.write_buffer(&self.uniform, 0, &data);
        }
        if self.pictures.len() > 256 {
            let frame = self.frame;
            self.pictures
                .retain(|_, p| frame.saturating_sub(p.used) < KEEP_FRAMES);
        }
    }

    fn upload(&self, gpu: &GpuContext, width: u32, height: u32, rgba: &[u8]) -> Picture {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hud-picture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Raw bytes: the shader does the canvas's byte arithmetic.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba[..(width as usize) * (height as usize) * 4],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hud-picture"),
            layout: &self.texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        Picture {
            _texture: texture,
            bind,
            width,
            height,
            used: self.frame,
        }
    }

    /// Draw what [`Self::prepare`] set up: the pictures, then the tint.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if !self.draws.is_empty() {
            pass.set_pipeline(&self.quad_pipeline);
            for (key, offset) in &self.draws {
                let Some(picture) = self.pictures.get(key) else {
                    continue;
                };
                pass.set_bind_group(0, &self.uniform_bind, &[*offset]);
                pass.set_bind_group(1, &picture.bind, &[]);
                pass.draw(0..6, 0..1);
            }
        }
        if let Some(offset) = self.tint {
            pass.set_pipeline(&self.tint_pipeline);
            pass.set_bind_group(0, &self.uniform_bind, &[offset]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(WGSL).expect("parse");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("valid");
    }

    #[test]
    fn tints_quantize_like_the_canvas() {
        // `HudSprites::prepare` turns 0..1 into whole bytes first.
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).floor() / 255.0;
        assert_eq!(byte(0.5), 127.0 / 255.0);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        assert!((srgb_to_linear(0.5) - 0.214).abs() < 1e-3);
    }
}
