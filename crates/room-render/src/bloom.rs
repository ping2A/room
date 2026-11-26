//! True bloom mip downsample/upsample chain (M10 / `docs/29` SHR R1).

use crate::device::GpuContext;
use bytemuck::{Pod, Zeroable, bytes_of};

const BLOOM_WGSL: &str = include_str!("shaders/bloom.wgsl");

/// Number of half-res bloom levels (mip0 = half screen).
pub const BLOOM_LEVELS: usize = 4;

/// Extract + (LEVELS−1) downs + LEVELS upsamples (chain + composite) — own UBO each (R1).
const BLOOM_PASSES: usize = BLOOM_LEVELS * 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
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

/// GPU bloom pyramid: extract → downsample chain → additive upsample (R3).
pub struct BloomPass {
    pipeline: wgpu::RenderPipeline,
    /// Upsample with `One + One` so lower mips accumulate (docs/29 R3).
    up_pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    /// One uniform buffer per draw so every encode pass sees its own params (R1).
    ubos: Vec<wgpu::Buffer>,
    sampler: wgpu::Sampler,
    levels: Vec<BloomLevel>,
    /// Full-res bloom composite.
    composite: Option<BloomLevel>,
    format: wgpu::TextureFormat,
    /// Last uniforms written per pass slot (unit tests / debug).
    last_uniforms: Vec<BloomUniforms>,
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
        let make_pipe = |label: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
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
                        blend,
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
            })
        };
        let pipeline = make_pipe("bloom-pipe", None);
        let up_pipeline = make_pipe(
            "bloom-up-pipe",
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::REPLACE,
            }),
        );
        let mut ubos = Vec::with_capacity(BLOOM_PASSES);
        for i in 0..BLOOM_PASSES {
            ubos.push(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("bloom-ubo-{i}")),
                size: std::mem::size_of::<BloomUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
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
            up_pipeline,
            bgl,
            ubos,
            sampler,
            levels: Vec::new(),
            composite: None,
            format,
            last_uniforms: vec![BloomUniforms::zeroed(); BLOOM_PASSES],
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

    /// Uniforms last written for pass slot `i` (extract=0, downs=1.., upsample=last).
    #[must_use]
    pub fn last_pass_params(&self, i: usize) -> Option<[f32; 4]> {
        self.last_uniforms.get(i).map(|u| u.params)
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

        // mip0: extract + downsample from full-res (slot 0).
        self.pass(gpu, encoder, src, src_w, src_h, 0, 0.0, threshold, 0);
        // Successive half-res downsamples (slots 1..LEVELS-1).
        self.downsample_chain(gpu, encoder);
        // Upsample finest mip to full-res composite (slot LEVELS).
        let (mw, mh) = (self.levels[0].width, self.levels[0].height);
        self.upsample_to_composite(gpu, encoder, mw, mh);

        Some(&self.composite.as_ref()?.view)
    }

    fn write_ubo(&mut self, gpu: &GpuContext, slot: usize, u: BloomUniforms) {
        if let Some(buf) = self.ubos.get(slot) {
            gpu.queue.write_buffer(buf, 0, bytes_of(&u));
        }
        if let Some(slot_u) = self.last_uniforms.get_mut(slot) {
            *slot_u = u;
        }
    }

    fn downsample_chain(&mut self, gpu: &GpuContext, encoder: &mut wgpu::CommandEncoder) {
        for i in 1..self.levels.len() {
            let (pw, ph) = (self.levels[i - 1].width, self.levels[i - 1].height);
            let u = BloomUniforms {
                params: [0.0, 0.0, 1.0 / pw as f32, 1.0 / ph as f32],
            };
            let slot = i;
            self.write_ubo(gpu, slot, u);
            let ubo = &self.ubos[slot];
            let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom-down-bg"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: ubo.as_entire_binding(),
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
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        mw: u32,
        mh: u32,
    ) {
        if self.composite.is_none() || self.levels.is_empty() {
            return;
        }
        // R3: walk coarse → fine, adding upsample(mip[i+1]) into mip[i].
        let n = self.levels.len();
        for i in (0..n.saturating_sub(1)).rev() {
            let (sw, sh) = (self.levels[i + 1].width, self.levels[i + 1].height);
            let slot = BLOOM_LEVELS + (n - 2 - i);
            let u = BloomUniforms {
                params: [1.0, 0.0, 1.0 / sw as f32, 1.0 / sh as f32],
            };
            self.write_ubo(gpu, slot, u);
            let ubo = &self.ubos[slot];
            let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom-up-chain-bg"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: ubo.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.levels[i + 1].view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            {
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("bloom-up-chain-rp"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.levels[i].view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                rp.set_pipeline(&self.up_pipeline);
                rp.set_bind_group(0, &bg, &[]);
                rp.draw(0..3, 0..1);
            }
        }
        // Finest half-res → full-res composite.
        let slot = BLOOM_PASSES - 1;
        let u = BloomUniforms {
            params: [1.0, 0.0, 1.0 / mw as f32, 1.0 / mh as f32],
        };
        self.write_ubo(gpu, slot, u);
        let ubo = &self.ubos[slot];
        let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom-up-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ubo.as_entire_binding(),
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
        let Some(composite) = self.composite.as_ref() else {
            return;
        };
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom-up-rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &composite.view,
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
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        src_w: u32,
        src_h: u32,
        dst_level: usize,
        mode: f32,
        threshold: f32,
        ubo_slot: usize,
    ) {
        if self.levels.get(dst_level).is_none() {
            return;
        }
        let u = BloomUniforms {
            params: [
                mode,
                threshold,
                1.0 / src_w.max(1) as f32,
                1.0 / src_h.max(1) as f32,
            ],
        };
        self.write_ubo(gpu, ubo_slot, u);
        let ubo = &self.ubos[ubo_slot];
        let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ubo.as_entire_binding(),
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
                    view: &self.levels[dst_level].view,
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

    #[test]
    fn bloom_pass_ubos_keep_distinct_params() {
        let gpu = match pollster::block_on(GpuContext::headless()) {
            Ok(g) => g,
            Err(_) => {
                eprintln!("skipping bloom_pass_ubos_keep_distinct_params: no wgpu");
                return;
            }
        };
        let mut bloom = BloomPass::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        bloom.ensure_size(&gpu, 64, 64);
        // Synthetic writes as if extract + downsample + upsample ran.
        bloom.write_ubo(
            &gpu,
            0,
            BloomUniforms {
                params: [0.0, 0.8, 1.0 / 64.0, 1.0 / 64.0],
            },
        );
        bloom.write_ubo(
            &gpu,
            1,
            BloomUniforms {
                params: [0.0, 0.0, 1.0 / 32.0, 1.0 / 32.0],
            },
        );
        bloom.write_ubo(
            &gpu,
            BLOOM_LEVELS,
            BloomUniforms {
                params: [1.0, 0.0, 1.0 / 32.0, 1.0 / 32.0],
            },
        );
        let extract = bloom.last_pass_params(0).unwrap();
        let down = bloom.last_pass_params(1).unwrap();
        let up = bloom.last_pass_params(BLOOM_LEVELS).unwrap();
        assert!((extract[1] - 0.8).abs() < 1e-5, "extract keeps threshold");
        assert!((down[1]).abs() < 1e-5, "downsample threshold 0");
        assert!((up[0] - 1.0).abs() < 1e-5, "upsample mode");
        assert!((extract[1] - down[1]).abs() > 0.5, "slots stay independent");
    }
}
