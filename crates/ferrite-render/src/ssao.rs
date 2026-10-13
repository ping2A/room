//! Screen-space ambient occlusion bootstrap (`gl_ssao`).

use crate::device::GpuContext;
use bytemuck::{Pod, Zeroable, bytes_of};

const SSAO_WGSL: &str = include_str!("shaders/ssao.wgsl");

/// Per-frame SSAO controls.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SsaoUniforms {
    /// `x` = enabled, `y` = radius, `z` = bias, `w` = intensity.
    pub params: [f32; 4],
    /// `x` = 1/width, `y` = 1/height.
    pub rcp: [f32; 4],
}

impl Default for SsaoUniforms {
    fn default() -> Self {
        Self {
            params: [0.0, 1.0, 0.002, 1.0],
            rcp: [1.0 / 1280.0, 1.0 / 720.0, 0.0, 0.0],
        }
    }
}

impl SsaoUniforms {
    /// Configure for a framebuffer size (`blur_sigma` ≈ depth units, default 0.02).
    pub fn configure(&mut self, enabled: bool, width: u32, height: u32) {
        self.configure_ex(enabled, width, height, 0.02);
    }

    /// Configure with an explicit bilateral depth sigma.
    pub fn configure_ex(&mut self, enabled: bool, width: u32, height: u32, blur_sigma: f32) {
        self.params[0] = if enabled { 1.0 } else { 0.0 };
        self.rcp[0] = 1.0 / (width.max(1) as f32);
        self.rcp[1] = 1.0 / (height.max(1) as f32);
        self.rcp[2] = blur_sigma.max(0.0);
    }
}

/// Depth-only view for SSAO sampling from a depth/stencil texture.
#[must_use]
pub fn depth_only_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("ssao-depth"),
        aspect: wgpu::TextureAspect::DepthOnly,
        ..Default::default()
    })
}

/// CPU occlusion weight for unit tests (depth 0 = near, 1 = far).
#[must_use]
pub fn ssao_edge_occlusion(center: f32, neighbour: f32, bias: f32) -> f32 {
    let delta = center - neighbour;
    if delta <= bias {
        0.0
    } else {
        ((delta - bias) / 0.05).clamp(0.0, 1.0)
    }
}

/// Bilateral depth weight `exp(-(Δd)² / (2σ²))` (matches SSAO blur shader).
#[must_use]
pub fn ssao_bilateral_weight(d0: f32, d1: f32, sigma: f32) -> f32 {
    let s = sigma.max(1e-4);
    let dd = d0 - d1;
    (-(dd * dd) / (2.0 * s * s)).exp()
}

/// Fullscreen SSAO pass (colour + depth → darkened colour).
pub struct SsaoPass {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    ubo: wgpu::Buffer,
    sampler: wgpu::Sampler,
    /// Dedicated half-res AO buffer (R8); rebuilt in [`Self::ensure_ao`].
    ao_tex: Option<wgpu::Texture>,
    /// View of [`Self::ao_tex`].
    ao_view: Option<wgpu::TextureView>,
    ao_w: u32,
    ao_h: u32,
    /// Active settings.
    pub uniforms: SsaoUniforms,
}

impl SsaoPass {
    /// Create against a colour target format. Depth must be `Depth24Plus` / `Depth24PlusStencil8`.
    pub fn new(gpu: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ssao"),
            source: wgpu::ShaderSource::Wgsl(SSAO_WGSL.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ssao-bgl"),
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
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ssao-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ssao-pipe"),
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
            label: Some("ssao-ubo"),
            size: std::mem::size_of::<SsaoUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ssao-samp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            bgl,
            ubo,
            sampler,
            ao_tex: None,
            ao_view: None,
            ao_w: 0,
            ao_h: 0,
            uniforms: SsaoUniforms::default(),
        }
    }

    /// Ensure a half-res R8 AO buffer exists for `full_w`×`full_h`.
    pub fn ensure_ao(&mut self, gpu: &GpuContext, full_w: u32, full_h: u32) {
        let w = (full_w / 2).max(1);
        let h = (full_h / 2).max(1);
        if self.ao_w == w && self.ao_h == h && self.ao_tex.is_some() {
            return;
        }
        let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ssao-ao"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.ao_view = Some(tex.create_view(&wgpu::TextureViewDescriptor::default()));
        self.ao_tex = Some(tex);
        self.ao_w = w;
        self.ao_h = h;
    }

    /// Half-res AO view, if [`Self::ensure_ao`] has been called.
    #[must_use]
    pub fn ao_view(&self) -> Option<&wgpu::TextureView> {
        self.ao_view.as_ref()
    }

    /// Darken `color` using `depth` into `dst`.
    ///
    /// `depth` must be a **depth-only** view (`TextureAspect::DepthOnly`) of a
    /// `Depth24Plus` / `Depth24PlusStencil8` texture with `TEXTURE_BINDING`.
    pub fn apply(
        &self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        dst: &wgpu::TextureView,
    ) {
        gpu.queue
            .write_buffer(&self.ubo, 0, bytes_of(&self.uniforms));
        let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssao-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.ubo.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(color),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ssao-rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst,
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
    fn farther_neighbour_no_occlusion() {
        // neighbour farther (higher depth) → delta negative
        assert!((ssao_edge_occlusion(0.5, 0.8, 0.002) - 0.0).abs() < 1e-5);
    }

    #[test]
    fn closer_neighbour_occludes() {
        let o = ssao_edge_occlusion(0.5, 0.4, 0.002);
        assert!(o > 0.5);
    }

    #[test]
    fn configure_sets_rcp() {
        let mut u = SsaoUniforms::default();
        u.configure(true, 800, 600);
        assert!((u.params[0] - 1.0).abs() < 1e-5);
        assert!((u.rcp[0] - 1.0 / 800.0).abs() < 1e-6);
        assert!((u.rcp[2] - 0.02).abs() < 1e-6);
    }

    #[test]
    fn bilateral_same_depth_is_one() {
        assert!((ssao_bilateral_weight(0.5, 0.5, 0.02) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn bilateral_far_depth_decays() {
        let w = ssao_bilateral_weight(0.5, 0.7, 0.02);
        assert!(w < 0.01, "expected strong decay, got {w}");
    }
}
