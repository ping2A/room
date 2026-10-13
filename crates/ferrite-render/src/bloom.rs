//! True bloom mip downsample/upsample chain (M10).

use crate::device::GpuContext;
use bytemuck::{Pod, Zeroable, bytes_of};

const BLOOM_WGSL: &str = include_str!("shaders/bloom.wgsl");

/// Number of half-res bloom levels (mip0 = half screen).
pub const BLOOM_LEVELS: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct BloomUniforms {
    /// `x` = mode (0 extract/down, 1 upsample), `y` = threshold, `z/w` = 1/src size.
    params: [f32; 4],
}

/// One mip level texture + view.
struct BloomLevel {
    #[allow(dead_code)]
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

/// GPU bloom pyramid: extract → downsample chain → upsample to full-res.
pub struct BloomPass {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    ubo: wgpu::Buffer,
    sampler: wgpu::Sampler,
    levels: Vec<BloomLevel>,
    /// Full-res bloom composite.
    composite: Option<BloomLevel>,
    format: wgpu::TextureFormat,
}

impl BloomPass {
    /// Create bloom pipelines for `format`.
    pub fn new(gpu: &GpuContext, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bloom"),
            source: wgpu::ShaderSource::Wgsl(BLOOM_WGSL.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom-bgl"),
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
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bloom-pipe"),
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
            label: Some("bloom-ubo"),
            size: std::mem::size_of::<BloomUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bloom-samp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self {
            pipeline,
            bgl,
            ubo,
            sampler,
            levels: Vec::new(),
            composite: None,
            format,
        }
    }

    /// Ensure mip pyramid matches `width`×`height` (full-res scene).
    pub fn ensure_size(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        let w0 = (width / 2).max(1);
        let h0 = (height / 2).max(1);
        let need_rebuild = self.levels.len() != BLOOM_LEVELS
            || self
                .levels
                .first()
                .is_none_or(|l| l.width != w0 || l.height != h0)
            || self
                .composite
                .as_ref()
                .is_none_or(|c| c.width != width || c.height != height);
        if !need_rebuild {
            return;
        }
        self.levels.clear();
        let mut w = w0;
        let mut h = h0;
        for i in 0..BLOOM_LEVELS {
            self.levels.push(make_level(
                &gpu.device,
                self.format,
                w,
                h,
                &format!("bloom-mip{i}"),
            ));
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        self.composite = Some(make_level(
            &gpu.device,
            self.format,
            width.max(1),
            height.max(1),
            "bloom-composite",
        ));
    }

    /// Number of allocated mip levels (0 until [`Self::ensure_size`]).
    #[must_use]
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    /// Width of mip level `i` (after [`Self::ensure_size`]).
    #[must_use]
    pub fn level_width(&self, i: usize) -> Option<u32> {
        self.levels.get(i).map(|l| l.width)
    }

    /// Extract thresholded bloom from `src`, blur through mips, write full-res composite.
    ///
    /// Returns the composite view when bloom ran; `None` if threshold ≤ 0.
    pub fn apply<'a>(
        &'a mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        src_w: u32,
        src_h: u32,
        threshold: f32,
    ) -> Option<&'a wgpu::TextureView> {
        if threshold <= 0.0 {
            return None;
        }
        self.ensure_size(gpu, src_w, src_h);
        if self.levels.is_empty() {
            return None;
        }

        // mip0: extract + downsample from full-res.
        self.pass(gpu, encoder, src, src_w, src_h, 0, 0.0, threshold);
        // Successive half-res downsamples.
        self.downsample_chain(gpu, encoder);
        // Upsample finest mip to full-res composite.
        let (mw, mh) = (self.levels[0].width, self.levels[0].height);
        self.upsample_to_composite(gpu, encoder, mw, mh);

        Some(&self.composite.as_ref()?.view)
    }

    fn downsample_chain(&self, gpu: &GpuContext, encoder: &mut wgpu::CommandEncoder) {
        for i in 1..self.levels.len() {
            let (pw, ph) = (self.levels[i - 1].width, self.levels[i - 1].height);
            let u = BloomUniforms {
                params: [0.0, 0.0, 1.0 / pw as f32, 1.0 / ph as f32],
            };
            gpu.queue.write_buffer(&self.ubo, 0, bytes_of(&u));
            let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom-down-bg"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.ubo.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.levels[i - 1].view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            {
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("bloom-down-rp"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.levels[i].view,
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

    fn upsample_to_composite(
        &self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        mw: u32,
        mh: u32,
    ) {
        let Some(comp) = &self.composite else {
            return;
        };
        let u = BloomUniforms {
            params: [1.0, 0.0, 1.0 / mw as f32, 1.0 / mh as f32],
        };
        gpu.queue.write_buffer(&self.ubo, 0, bytes_of(&u));
        let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom-up-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.ubo.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.levels[0].view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom-up-rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &comp.view,
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

    fn pass(
        &self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        src_w: u32,
        src_h: u32,
        dst_level: usize,
        mode: f32,
        threshold: f32,
    ) {
        let Some(dst) = self.levels.get(dst_level) else {
            return;
        };
        let u = BloomUniforms {
            params: [
                mode,
                threshold,
                1.0 / src_w.max(1) as f32,
                1.0 / src_h.max(1) as f32,
            ],
        };
        gpu.queue.write_buffer(&self.ubo, 0, bytes_of(&u));
        let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom-bg"),
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
            ],
        });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom-rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst.view,
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

fn make_level(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    label: &str,
) -> BloomLevel {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    BloomLevel {
        tex,
        view,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn bloom_levels_halve() {
        let gpu = match pollster::block_on(GpuContext::headless()) {
            Ok(g) => g,
            Err(_) => {
                eprintln!("skipping bloom_levels_halve: no wgpu");
                return;
            }
        };
        let mut bloom = BloomPass::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        bloom.ensure_size(&gpu, 320, 200);
        assert_eq!(bloom.level_count(), BLOOM_LEVELS);
        assert_eq!(bloom.level_width(0), Some(160));
        assert_eq!(bloom.level_width(1), Some(80));
        assert_eq!(bloom.level_width(2), Some(40));
        assert_eq!(bloom.level_width(3), Some(20));
    }
}
