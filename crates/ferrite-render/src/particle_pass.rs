//! GPU instanced particle draw (`ParticleInstancedPass`).

use crate::camera::Camera;
use crate::device::GpuContext;
use crate::particles::{ParticleGpuInstance, ParticleStore, pack_particle_instances};
use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};
use glam::Vec3;

const PARTICLES_WGSL: &str = include_str!("shaders/particles.wgsl");

/// View uniforms for particle billboards (right/up from camera).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ParticleViewUniforms {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
}

/// Unit-quad corner (−0.5…0.5) for one particle.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct CornerVert {
    xy: [f32; 2],
}

/// Instanced particle render pass (additive + translucent use alpha blend).
pub struct ParticleInstancedPass {
    pipeline: wgpu::RenderPipeline,
    view_buf: wgpu::Buffer,
    view_bind: wgpu::BindGroup,
    quad_buf: wgpu::Buffer,
    instance_buf: wgpu::Buffer,
    instance_cap: u64,
}

impl ParticleInstancedPass {
    /// Create against `target_format`.
    pub fn new(gpu: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particles"),
            source: wgpu::ShaderSource::Wgsl(PARTICLES_WGSL.into()),
        });
        let view_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle-view-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle-layout"),
            bind_group_layouts: &[&view_bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particle-pipe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<CornerVert>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<ParticleGpuUpload>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            1 => Float32x4,
                            2 => Float32x4,
                            3 => Float32,
                        ],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let view_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-view"),
            size: std::mem::size_of::<ParticleViewUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle-view-bg"),
            layout: &view_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_buf.as_entire_binding(),
            }],
        });

        // Two triangles: corners in (−0.5…0.5).
        let corners: [CornerVert; 6] = [
            CornerVert { xy: [-0.5, -0.5] },
            CornerVert { xy: [0.5, -0.5] },
            CornerVert { xy: [0.5, 0.5] },
            CornerVert { xy: [-0.5, -0.5] },
            CornerVert { xy: [0.5, 0.5] },
            CornerVert { xy: [-0.5, 0.5] },
        ];
        let quad_buf = device.create_buffer_init_compat(cast_slice(&corners));

        let instance_cap = 256u64;
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-instances"),
            size: instance_cap * std::mem::size_of::<ParticleGpuUpload>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            view_buf,
            view_bind,
            quad_buf,
            instance_buf,
            instance_cap,
        }
    }

    /// Draw packed instances into `color` / `depth`.
    pub fn draw(
        &mut self,
        gpu: &GpuContext,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        camera: &Camera,
        instances: &[ParticleGpuInstance],
        clear: bool,
    ) {
        if instances.is_empty() {
            return;
        }
        let device = &gpu.device;
        let queue = &gpu.queue;
        let need = instances.len() as u64;
        if need > self.instance_cap {
            self.instance_cap = need.next_power_of_two().max(256);
            self.instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particle-instances"),
                size: self.instance_cap * std::mem::size_of::<ParticleGpuUpload>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        let upload = pack_instances_for_gpu(instances);
        queue.write_buffer(&self.instance_buf, 0, cast_slice(&upload));

        let right = camera_right(camera);
        let up = camera_up(camera);
        let view_u = ParticleViewUniforms {
            view_proj: camera.view_proj().to_cols_array_2d(),
            eye: [camera.eye.x, camera.eye.y, camera.eye.z, 0.0],
            right: [right.x, right.y, right.z, 0.0],
            up: [up.x, up.y, up.z, 0.0],
        };
        queue.write_buffer(&self.view_buf, 0, bytes_of(&view_u));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("particle-enc"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("particle-rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if clear {
                            wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.02,
                                g: 0.02,
                                b: 0.04,
                                a: 1.0,
                            })
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: if clear {
                            wgpu::LoadOp::Clear(1.0)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.view_bind, &[]);
            pass.set_vertex_buffer(0, self.quad_buf.slice(..));
            pass.set_vertex_buffer(1, self.instance_buf.slice(..));
            pass.draw(0..6, 0..instances.len() as u32);
        }
        queue.submit(Some(encoder.finish()));
    }

    /// Pack + draw from a [`ParticleStore`].
    pub fn draw_store(
        &mut self,
        gpu: &GpuContext,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        camera: &Camera,
        store: &ParticleStore,
        clear: bool,
    ) {
        let packed = pack_particle_instances(store);
        self.draw(gpu, color, depth, camera, &packed, clear);
    }
}

/// GPU upload layout: pos_size, color_alpha, style as f32, pad×3 f32 (matches attr stride).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ParticleGpuUpload {
    pos_size: [f32; 4],
    color_alpha: [f32; 4],
    style_f: f32,
    _pad: [f32; 3],
}

fn pack_instances_for_gpu(instances: &[ParticleGpuInstance]) -> Vec<ParticleGpuUpload> {
    instances
        .iter()
        .map(|p| ParticleGpuUpload {
            pos_size: p.pos_size,
            color_alpha: p.color_alpha,
            style_f: p.style as f32,
            _pad: [0.0; 3],
        })
        .collect()
}

fn camera_right(camera: &Camera) -> Vec3 {
    let f = camera.forward.normalize_or_zero();
    let world_up = Vec3::Z;
    let r = f.cross(world_up);
    if r.length_squared() < 1e-8 {
        Vec3::X
    } else {
        r.normalize()
    }
}

fn camera_up(camera: &Camera) -> Vec3 {
    let f = camera.forward.normalize_or_zero();
    let r = camera_right(camera);
    r.cross(f).normalize_or_zero()
}

trait BufferInitCompat {
    fn create_buffer_init_compat(&self, contents: &[u8]) -> wgpu::Buffer;
}

impl BufferInitCompat for wgpu::Device {
    fn create_buffer_init_compat(&self, contents: &[u8]) -> wgpu::Buffer {
        let buf = self.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-quad"),
            size: contents.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        buf.slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(contents);
        buf.unmap();
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ParticleStyle;

    #[test]
    fn pack_upload_preserves_style() {
        let inst = ParticleGpuInstance {
            pos_size: [1.0, 2.0, 3.0, 4.0],
            color_alpha: [1.0, 1.0, 1.0, 0.5],
            style: ParticleStyle::Soft as u32,
            _pad: [0; 3],
        };
        let u = pack_instances_for_gpu(&[inst]);
        assert!((u[0].style_f - 2.0).abs() < 1e-4);
    }
}
