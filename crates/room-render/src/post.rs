//! Post-process chain bootstrap (M10): tonemap, gamma, multi-scale bloom, FXAA, screen blend.

use crate::device::GpuContext;
use bytemuck::{Pod, Zeroable, bytes_of};

const POST_WGSL: &str = include_str!("shaders/post.wgsl");

/// Tonemap operator (`gl_tonemap` subset).
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
    /// Quantize to PLAYPAL (nearest RGB).
    Palette = 4,
}

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
    }
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
    /// `x` = tonemap mode, `y` = exposure, `z` = gamma, `w` = bloom threshold.
    pub params: [f32; 4],
    /// RGBA screen blend (damage / bonus / suit).
    pub blend: [f32; 4],
    /// `x` = FXAA enable, `y` = 1/width, `z` = 1/height.
    pub fxaa: [f32; 4],
}

impl Default for PostUniforms {
    fn default() -> Self {
        Self {
            params: [TonemapMode::Reinhard as u32 as f32, 1.0, 1.0, 0.0],
            blend: [0.0, 0.0, 0.0, 0.0],
            fxaa: [0.0, 1.0 / 1280.0, 1.0 / 720.0, 0.0],
        }
    }
}

impl PostUniforms {
    /// Build from controls.
    #[must_use]
    pub fn new(
        mode: TonemapMode,
        exposure: f32,
        gamma: f32,
        bloom_threshold: f32,
        blend: [f32; 4],
    ) -> Self {
        Self {
            params: [
                mode as u32 as f32,
                exposure,
                gamma,
                bloom_threshold.max(0.0),
            ],
            blend,
            fxaa: [0.0, 1.0 / 1280.0, 1.0 / 720.0, 0.0],
        }
    }

    /// Enable/disable FXAA and set framebuffer size for texel offsets.
    pub fn set_fxaa(&mut self, enabled: bool, width: u32, height: u32) {
        self.fxaa[0] = if enabled { 1.0 } else { 0.0 };
        self.fxaa[1] = 1.0 / (width.max(1) as f32);
        self.fxaa[2] = 1.0 / (height.max(1) as f32);
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
        Self {
            pipeline,
            bgl,
            ubo,
            sampler,
            pal_tex,
            pal_view,
            uniforms: PostUniforms::default(),
        }
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
    }

    /// Tonemap `src` into `dst_view` (same size expected).
    pub fn apply(
        &self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
    ) {
        gpu.queue
            .write_buffer(&self.ubo, 0, bytes_of(&self.uniforms));
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
}
