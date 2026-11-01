//! Post-process chain bootstrap (M10): tonemap, gamma, multi-scale bloom, FXAA, screen blend.

use crate::device::GpuContext;
use bytemuck::{Pod, Zeroable, bytes_of};

const POST_WGSL: &str = include_str!("shaders/post.wgsl");

/// Tonemap operator (`gl_tonemap` + Room extensions, `docs/29` A3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum TonemapMode {
    /// Passthrough (exposure only).
    #[default]
    None = 0,
    /// Reinhard `c / (c + 1)`.
    Reinhard = 1,
    /// Linear clamp after exposure.
    Linear = 2,
    /// Compact Hejl-Dawson filmic.
    Hejl = 3,
    /// Quantize to PLAYPAL (nearest RGB / 3D LUT).
    Palette = 4,
    /// Uncharted 2 filmic (GZDoom set).
    Uncharted2 = 5,
    /// AgX (punchy midtones).
    Agx = 6,
    /// Khronos PBR Neutral.
    KhronosNeutral = 7,
    /// Previous in-shader soft toe + sRGB (web look); use when scene is linear.
    RoomSoft = 8,
}

impl TonemapMode {
    /// Map a `gl_tonemap` integer (unknown → [`Self::None`]).
    #[must_use]
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Reinhard,
            2 => Self::Linear,
            3 => Self::Hejl,
            4 => Self::Palette,
            5 => Self::Uncharted2,
            6 => Self::Agx,
            7 => Self::KhronosNeutral,
            8 => Self::RoomSoft,
            _ => Self::None,
        }
    }
}

/// Edge length of the RGB→palette 3D LUT (`docs/29` A4).
pub const PALETTE_LUT_SIZE: u32 = 32;

/// CPU-side tonemap (tests / offline). Palette mode without a table ≈ linear clamp.
#[must_use]
pub fn tonemap_rgb(rgb: [f32; 3], mode: TonemapMode, exposure: f32) -> [f32; 3] {
    tonemap_rgb_pal(rgb, mode, exposure, None)
}

/// Tonemap with optional PLAYPAL palette 0 (`rgb` = 768 bytes).
#[must_use]
pub fn tonemap_rgb_pal(
    rgb: [f32; 3],
    mode: TonemapMode,
    exposure: f32,
    playpal: Option<&[u8; 768]>,
) -> [f32; 3] {
    let e = exposure.max(0.01);
    let x = [rgb[0] * e, rgb[1] * e, rgb[2] * e];
    match mode {
        TonemapMode::None => x,
        TonemapMode::Reinhard => [
            x[0] / (x[0] + 1.0),
            x[1] / (x[1] + 1.0),
            x[2] / (x[2] + 1.0),
        ],
        TonemapMode::Linear => [
            x[0].clamp(0.0, 1.0),
            x[1].clamp(0.0, 1.0),
            x[2].clamp(0.0, 1.0),
        ],
        TonemapMode::Hejl => {
            let map = |c: f32| {
                let a = (c - 0.004).max(0.0);
                (a * (6.2 * a + 0.5)) / (a * (6.2 * a + 1.7) + 0.06)
            };
            [map(x[0]), map(x[1]), map(x[2])]
        }
        TonemapMode::Palette => match playpal {
            Some(pal) => quantize_playpal(x, pal),
            None => [
                x[0].clamp(0.0, 1.0),
                x[1].clamp(0.0, 1.0),
                x[2].clamp(0.0, 1.0),
            ],
        },
        TonemapMode::Uncharted2 => {
            let map = |c: f32| uncharted2_tonemap(c) / uncharted2_tonemap(2.0);
            [map(x[0]), map(x[1]), map(x[2])]
        }
        TonemapMode::Agx => agx_tonemap(x),
        TonemapMode::KhronosNeutral => khronos_neutral(x),
        TonemapMode::RoomSoft => room_soft_tonemap(x),
    }
}

fn uncharted2_tonemap(x: f32) -> f32 {
    const A: f32 = 0.15;
    const B: f32 = 0.50;
    const C: f32 = 0.10;
    const D: f32 = 0.20;
    const E: f32 = 0.02;
    const F: f32 = 0.30;
    ((x * (A * x + C * B) + D * E) / (x * (A * x + B) + D * F)) - E / F
}

/// Soft toe previously inline in `unlit.wgsl` (linear out; encode is separate).
fn room_soft_tonemap(rgb: [f32; 3]) -> [f32; 3] {
    [
        rgb[0] * (0.65 + 0.35 / (rgb[0] + 0.35)),
        rgb[1] * (0.65 + 0.35 / (rgb[1] + 0.35)),
        rgb[2] * (0.65 + 0.35 / (rgb[2] + 0.35)),
    ]
}

/// True when the colour target already applies sRGB on store (`*Srgb` formats).
#[must_use]
pub fn format_is_srgb(format: wgpu::TextureFormat) -> bool {
    format.is_srgb()
}

fn agx_tonemap(rgb: [f32; 3]) -> [f32; 3] {
    // Compact AgX-like sigmoid (not bit-identical to Blender; good lookalike).
    let map = |c: f32| {
        let x = (c * 0.8).max(0.0);
        let y = x / (x + 1.0);
        y.powf(1.1)
    };
    [map(rgb[0]), map(rgb[1]), map(rgb[2])]
}

fn khronos_neutral(rgb: [f32; 3]) -> [f32; 3] {
    // Khronos PBR Neutral (simplified scalar form).
    let map = |x: f32| {
        let x = x.max(0.0);
        let f = 0.04;
        if x < f {
            x
        } else {
            (x - f) / (1.0 + (x - f)) + f
        }
    };
    [map(rgb[0]), map(rgb[1]), map(rgb[2])]
}

/// Build a dense `SIZE³` RGB→nearest-PLAYPAL colour LUT (RGBA8, nearest sample).
#[must_use]
pub fn build_palette_lut(playpal: &[u8; 768], size: u32) -> Vec<u8> {
    let n = size.max(2);
    let mut out = vec![0u8; (n * n * n * 4) as usize];
    let mut i = 0usize;
    for bz in 0..n {
        for by in 0..n {
            for bx in 0..n {
                let rgb = [
                    bx as f32 / (n - 1) as f32,
                    by as f32 / (n - 1) as f32,
                    bz as f32 / (n - 1) as f32,
                ];
                let q = quantize_playpal(rgb, playpal);
                out[i] = (q[0] * 255.0).round() as u8;
                out[i + 1] = (q[1] * 255.0).round() as u8;
                out[i + 2] = (q[2] * 255.0).round() as u8;
                out[i + 3] = 255;
                i += 4;
            }
        }
    }
    out
}

/// Nearest PLAYPAL colour (linear RGB 0..=1 in/out).
#[must_use]
pub fn quantize_playpal(rgb: [f32; 3], pal: &[u8; 768]) -> [f32; 3] {
    let r = (rgb[0].clamp(0.0, 1.0) * 255.0).round();
    let g = (rgb[1].clamp(0.0, 1.0) * 255.0).round();
    let b = (rgb[2].clamp(0.0, 1.0) * 255.0).round();
    let mut best = 0usize;
    let mut best_d = f32::MAX;
    for i in 0..256 {
        let pr = f32::from(pal[i * 3]);
        let pg = f32::from(pal[i * 3 + 1]);
        let pb = f32::from(pal[i * 3 + 2]);
        let d = (pr - r) * (pr - r) + (pg - g) * (pg - g) + (pb - b) * (pb - b);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    [
        f32::from(pal[best * 3]) / 255.0,
        f32::from(pal[best * 3 + 1]) / 255.0,
        f32::from(pal[best * 3 + 2]) / 255.0,
    ]
}

/// Per-frame post uniforms.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PostUniforms {
    /// `x` = tonemap mode, `y` = exposure, `z` = gamma, `w` = bloom intensity.
    pub params: [f32; 4],
    /// RGBA screen blend (damage / bonus / suit).
    pub blend: [f32; 4],
    /// `x` = FXAA enable, `y` = 1/width, `z` = 1/height, `w` = time (s).
    pub fxaa: [f32; 4],
    /// `x` = vignette, `y` = grain, `z` = chromatic, `w` = crt amount.
    pub extras: [f32; 4],
    /// `x` = dither, `y` = banding, `z` = sharpen (0..1), `w` = damage vignette pulse.
    pub extras2: [f32; 4],
    /// `x` = encode sRGB in-shader (1) when the colour target is not `*Srgb`.
    pub flags: [f32; 4],
}

impl Default for PostUniforms {
    fn default() -> Self {
        Self {
            params: [TonemapMode::Reinhard as u32 as f32, 1.0, 1.0, 0.0],
            blend: [0.0, 0.0, 0.0, 0.0],
            fxaa: [0.0, 1.0 / 1280.0, 1.0 / 720.0, 0.0],
            extras: [0.0; 4],
            extras2: [0.0; 4],
            flags: [0.0; 4],
        }
    }
}

impl PostUniforms {
    /// Build from controls. `bloom_intensity` adds the bloom composite (0 = off).
    #[must_use]
    pub fn new(
        mode: TonemapMode,
        exposure: f32,
        gamma: f32,
        bloom_intensity: f32,
        blend: [f32; 4],
    ) -> Self {
        Self {
            params: [
                mode as u32 as f32,
                exposure,
                gamma,
                bloom_intensity.max(0.0),
            ],
            blend,
            fxaa: [0.0, 1.0 / 1280.0, 1.0 / 720.0, 0.0],
            extras: [0.0; 4],
            extras2: [0.0; 4],
            flags: [0.0; 4],
        }
    }

    /// Whether the fragment should `linear_to_srgb` (non-sRGB colour targets).
    pub fn set_encode_srgb(&mut self, encode: bool) {
        self.flags[0] = if encode { 1.0 } else { 0.0 };
    }

    /// Enable/disable FXAA and set framebuffer size for texel offsets.
    pub fn set_fxaa(&mut self, enabled: bool, width: u32, height: u32) {
        self.fxaa[0] = if enabled { 1.0 } else { 0.0 };
        self.fxaa[1] = 1.0 / (width.max(1) as f32);
        self.fxaa[2] = 1.0 / (height.max(1) as f32);
    }

    /// Optional look FX (all default off).
    pub fn set_look_fx(
        &mut self,
        vignette: f32,
        grain: f32,
        chromatic: f32,
        crt: f32,
        dither: bool,
        banding: bool,
        sharpen: f32,
        damage_pulse: f32,
        time: f32,
    ) {
        self.extras = [
            vignette.clamp(0.0, 1.0),
            grain.clamp(0.0, 1.0),
            chromatic.clamp(0.0, 1.0),
            crt.clamp(0.0, 1.0),
        ];
        self.extras2 = [
            if dither { 1.0 } else { 0.0 },
            if banding { 1.0 } else { 0.0 },
            sharpen.clamp(0.0, 1.0),
            damage_pulse.clamp(0.0, 1.0),
        ];
        self.fxaa[3] = time;
    }
}

/// Fullscreen post pass (samples scene colour → destination).
pub struct PostPass {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    ubo: wgpu::Buffer,
    sampler: wgpu::Sampler,
    /// 256×1 PLAYPAL palette 0 (or greyscale fallback).
    pal_tex: wgpu::Texture,
    pal_view: wgpu::TextureView,
    /// 32³ RGB→palette colour LUT.
    lut_tex: wgpu::Texture,
    lut_view: wgpu::TextureView,
    lut_samp: wgpu::Sampler,
    /// 1×1 black when bloom is off (always bind binding 6).
    #[allow(dead_code)]
    black_tex: wgpu::Texture,
    black_view: wgpu::TextureView,
    /// Destination colour format (for encode_srgb).
    target_format: wgpu::TextureFormat,
    /// Active settings (uploaded each [`Self::apply`]).
    pub uniforms: PostUniforms,
}

impl PostPass {
    /// Create against a destination colour format.
    pub fn new(gpu: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post"),
            source: wgpu::ShaderSource::Wgsl(POST_WGSL.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post-pipe"),
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
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let ubo = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post-ubo"),
            size: std::mem::size_of::<PostUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post-samp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pal_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("post-playpal"),
            size: wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let pal_view = pal_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let lut_extent = wgpu::Extent3d {
            width: PALETTE_LUT_SIZE,
            height: PALETTE_LUT_SIZE,
            depth_or_array_layers: PALETTE_LUT_SIZE,
        };
        let lut_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("post-pal-lut"),
            size: lut_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let lut_view = lut_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let lut_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post-pal-lut-samp"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        // Greyscale identity until [`Self::set_playpal`].
        let mut grey = vec![0u8; 256 * 4];
        for i in 0..256 {
            let g = i as u8;
            grey[i * 4] = g;
            grey[i * 4 + 1] = g;
            grey[i * 4 + 2] = g;
            grey[i * 4 + 3] = 255;
        }
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &pal_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &grey,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256 * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let mut identity_pal = [0u8; 768];
        for i in 0..256 {
            let g = i as u8;
            identity_pal[i * 3] = g;
            identity_pal[i * 3 + 1] = g;
            identity_pal[i * 3 + 2] = g;
        }
        let lut_bytes = build_palette_lut(&identity_pal, PALETTE_LUT_SIZE);
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &lut_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &lut_bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(PALETTE_LUT_SIZE * 4),
                rows_per_image: Some(PALETTE_LUT_SIZE),
            },
            lut_extent,
        );
        let black_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("post-bloom-black"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let black_view = black_tex.create_view(&wgpu::TextureViewDescriptor::default());
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &black_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0u8, 0, 0, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let mut uniforms = PostUniforms::default();
        uniforms.set_encode_srgb(!format_is_srgb(target_format));
        Self {
            pipeline,
            bgl,
            ubo,
            sampler,
            pal_tex,
            pal_view,
            lut_tex,
            lut_view,
            lut_samp,
            black_tex,
            black_view,
            target_format,
            uniforms,
        }
    }

    /// Destination colour format this pipeline was built for.
    #[must_use]
    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    /// Upload PLAYPAL palette 0 (`rgb` = 768 bytes) for [`TonemapMode::Palette`].
    pub fn set_playpal(&self, gpu: &GpuContext, rgb: &[u8; 768]) {
        let mut rgba = vec![0u8; 256 * 4];
        for i in 0..256 {
            rgba[i * 4] = rgb[i * 3];
            rgba[i * 4 + 1] = rgb[i * 3 + 1];
            rgba[i * 4 + 2] = rgb[i * 3 + 2];
            rgba[i * 4 + 3] = 255;
        }
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.pal_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256 * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let lut_bytes = build_palette_lut(rgb, PALETTE_LUT_SIZE);
        let lut_extent = wgpu::Extent3d {
            width: PALETTE_LUT_SIZE,
            height: PALETTE_LUT_SIZE,
            depth_or_array_layers: PALETTE_LUT_SIZE,
        };
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.lut_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &lut_bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(PALETTE_LUT_SIZE * 4),
                rows_per_image: Some(PALETTE_LUT_SIZE),
            },
            lut_extent,
        );
    }

    /// Tonemap `src` (+ optional bloom composite) into `dst_view`.
    pub fn apply(
        &self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        bloom: Option<&wgpu::TextureView>,
        dst_view: &wgpu::TextureView,
    ) {
        let mut u = self.uniforms;
        u.set_encode_srgb(!format_is_srgb(self.target_format));
        gpu.queue.write_buffer(&self.ubo, 0, bytes_of(&u));
        let bloom_view = bloom.unwrap_or(&self.black_view);
        let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.ubo.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.pal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.lut_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.lut_samp),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(bloom_view),
                },
            ],
        });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post-rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(0, &bg, &[]);
            rp.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reinhard_compresses_bright() {
        let out = tonemap_rgb([2.0, 2.0, 2.0], TonemapMode::Reinhard, 1.0);
        assert!(out[0] < 1.0 && out[0] > 0.5);
        assert!((out[0] - 2.0 / 3.0).abs() < 1e-5);
    }

    #[test]
    fn linear_clamps() {
        let out = tonemap_rgb([2.0, 0.5, -0.1], TonemapMode::Linear, 1.0);
        assert!((out[0] - 1.0).abs() < 1e-5);
        assert!((out[1] - 0.5).abs() < 1e-5);
        assert!((out[2] - 0.0).abs() < 1e-5);
    }

    #[test]
    fn hejl_maps_midtones() {
        let out = tonemap_rgb([0.5, 0.5, 0.5], TonemapMode::Hejl, 1.0);
        assert!(out[0] > 0.0 && out[0] < 1.0);
    }

    #[test]
    fn uniforms_pack_mode() {
        let u = PostUniforms::new(TonemapMode::Hejl, 1.2, 2.2, 0.8, [1.0, 0.0, 0.0, 0.25]);
        assert!((u.params[0] - 3.0).abs() < 1e-5);
        assert!((u.params[1] - 1.2).abs() < 1e-5);
        assert!((u.blend[3] - 0.25).abs() < 1e-5);
    }

    #[test]
    fn set_fxaa_rcp() {
        let mut u = PostUniforms::default();
        u.set_fxaa(true, 640, 480);
        assert!((u.fxaa[0] - 1.0).abs() < 1e-5);
        assert!((u.fxaa[1] - 1.0 / 640.0).abs() < 1e-6);
        assert!((u.fxaa[2] - 1.0 / 480.0).abs() < 1e-6);
    }

    #[test]
    fn palette_snaps_to_entry() {
        let mut pal = [0u8; 768];
        // Index 5 = pure red.
        pal[5 * 3] = 255;
        let out = quantize_playpal([0.95, 0.05, 0.05], &pal);
        assert!((out[0] - 1.0).abs() < 1e-5);
        assert!(out[1].abs() < 1e-5);
    }

    #[test]
    fn room_soft_lifts_mids() {
        let out = tonemap_rgb([0.2, 0.2, 0.2], TonemapMode::RoomSoft, 1.0);
        assert!(out[0] > 0.0 && out[0] < 1.0);
    }

    #[test]
    fn palette_lut_size_matches() {
        let mut pal = [0u8; 768];
        for i in 0..256 {
            pal[i * 3] = i as u8;
            pal[i * 3 + 1] = i as u8;
            pal[i * 3 + 2] = i as u8;
        }
        let lut = build_palette_lut(&pal, 8);
        assert_eq!(lut.len(), 8 * 8 * 8 * 4);
    }

    #[test]
    fn tonemap_from_u32_covers_extensions() {
        assert_eq!(TonemapMode::from_u32(8), TonemapMode::RoomSoft);
        assert_eq!(TonemapMode::from_u32(6), TonemapMode::Agx);
        assert_eq!(TonemapMode::from_u32(99), TonemapMode::None);
    }
}
