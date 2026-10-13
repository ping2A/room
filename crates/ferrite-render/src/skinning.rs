//! IQM GPU skinning bootstrap (bone palette + skinned vertex pipeline).

use crate::camera::Camera;
use crate::device::GpuContext;
use crate::mesh::ViewUniforms;
use crate::models::IqmModel;
use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};
use glam::{Mat4, Vec3};

const SKINNED_WGSL: &str = include_str!("shaders/skinned.wgsl");

/// Maximum bones in the GPU palette (uniform array size).
pub const MAX_BONES: usize = 64;

/// Interleaved skinned vertex (bind-pose positions + blend data).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SkinnedVertex {
    /// Bind-pose position.
    pub pos: [f32; 3],
    /// UV.
    pub uv: [f32; 2],
    /// Sector light 0..=1.
    pub light: f32,
    /// Bone indices (as f32 for vertex attr ease).
    pub bone_indices: [f32; 4],
    /// Bone weights (0..=1).
    pub bone_weights: [f32; 4],
}

impl SkinnedVertex {
    /// Construct from bind-pose data.
    #[must_use]
    pub fn new(
        pos: [f32; 3],
        uv: [f32; 2],
        light: f32,
        bone_indices: [u8; 4],
        bone_weights: [u8; 4],
    ) -> Self {
        Self {
            pos,
            uv,
            light,
            bone_indices: [
                f32::from(bone_indices[0]),
                f32::from(bone_indices[1]),
                f32::from(bone_indices[2]),
                f32::from(bone_indices[3]),
            ],
            bone_weights: [
                f32::from(bone_weights[0]) / 255.0,
                f32::from(bone_weights[1]) / 255.0,
                f32::from(bone_weights[2]) / 255.0,
                f32::from(bone_weights[3]) / 255.0,
            ],
        }
    }
}

/// GPU bone palette (std140-ish: count + 64 mat4).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct BoneUniforms {
    /// `x` = bone count.
    pub count: [u32; 4],
    /// Skin matrices `frame * inv_bind` (column-major).
    pub mats: [[[f32; 4]; 4]; MAX_BONES],
}

impl BoneUniforms {
    /// Pack skin matrices (`len` capped at [`MAX_BONES`]).
    #[must_use]
    pub fn from_skin_mats(mats: &[Mat4]) -> Self {
        let mut out = Self::zeroed();
        let n = mats.len().min(MAX_BONES);
        out.count = [n as u32, 0, 0, 0];
        for (i, m) in mats.iter().take(n).enumerate() {
            out.mats[i] = m.to_cols_array_2d();
        }
        // Remaining stay identity via zeroed→fix with identity below.
        for m in out.mats.iter_mut().take(MAX_BONES).skip(n) {
            *m = Mat4::IDENTITY.to_cols_array_2d();
        }
        out
    }
}

/// Build `frame_abs[i] * inv_base[i]` skin matrices for GPU upload.
#[must_use]
pub fn skin_matrices(iqm: &IqmModel, frame_abs: &[Mat4]) -> Vec<Mat4> {
    let n = iqm.base_mats.len().min(frame_abs.len()).min(MAX_BONES);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(frame_abs[i] * iqm.inv_base_mats[i]);
    }
    out
}

/// Expand an IQM to GPU skinned vertices (bind pose + blends), world-transformed.
#[must_use]
pub fn iqm_to_skinned_vertices(
    iqm: &IqmModel,
    origin: Vec3,
    scale: Vec3,
    light: f32,
) -> Vec<SkinnedVertex> {
    if !iqm.is_skinned() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(iqm.mesh.indices.len());
    for &ii in &iqm.mesh.indices {
        let i = ii as usize;
        let v = &iqm.mesh.vertices[i];
        let p = [
            v.pos[0] * scale.x + origin.x,
            v.pos[1] * scale.y + origin.y,
            v.pos[2] * scale.z + origin.z,
        ];
        out.push(SkinnedVertex::new(
            p,
            v.uv,
            light,
            iqm.blend_indexes[i],
            iqm.blend_weights[i],
        ));
    }
    out
}

/// Owns a skinned-model draw pipeline (IQM GPU skinning bootstrap).
pub struct SkinnedPass {
    pipeline: wgpu::RenderPipeline,
    view_buf: wgpu::Buffer,
    bone_buf: wgpu::Buffer,
    view_bind: wgpu::BindGroup,
    mat_bgl: wgpu::BindGroupLayout,
    mat_bind: wgpu::BindGroup,
    _default_tex: wgpu::Texture,
    _default_view: wgpu::TextureView,
    _default_samp: wgpu::Sampler,
    vert_buf: wgpu::Buffer,
    vert_cap: u64,
}

impl SkinnedPass {
    /// Create skinned pipeline against `target_format`.
    pub fn new(gpu: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("skinned"),
            source: wgpu::ShaderSource::Wgsl(SKINNED_WGSL.into()),
        });
        let view_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skinned-view-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let mat_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skinned-mat-bgl"),
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skinned-layout"),
            bind_group_layouts: &[&view_bgl, &mat_bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("skinned-pipe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SkinnedVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3,
                        1 => Float32x2,
                        2 => Float32,
                        3 => Float32x4,
                        4 => Float32x4,
                    ],
                }],
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
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let view_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skinned-view"),
            size: std::mem::size_of::<ViewUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bone_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skinned-bones"),
            size: std::mem::size_of::<BoneUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("skinned-view-bg"),
            layout: &view_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: view_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bone_buf.as_entire_binding(),
                },
            ],
        });

        let (default_tex, default_view, default_samp) = make_white_tex(device, &gpu.queue);
        let mat_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("skinned-mat-bg"),
            layout: &mat_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&default_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&default_samp),
                },
            ],
        });

        let vert_cap = 256u64;
        let vert_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skinned-verts"),
            size: vert_cap * std::mem::size_of::<SkinnedVertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            view_buf,
            bone_buf,
            view_bind,
            mat_bgl,
            mat_bind,
            _default_tex: default_tex,
            _default_view: default_view,
            _default_samp: default_samp,
            vert_buf,
            vert_cap,
        }
    }

    /// Draw skinned triangles into `color` / `depth` views.
    pub fn draw(
        &mut self,
        gpu: &GpuContext,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        camera: &Camera,
        verts: &[SkinnedVertex],
        skin_mats: &[Mat4],
        clear: bool,
    ) {
        if verts.is_empty() {
            return;
        }
        let device = &gpu.device;
        let queue = &gpu.queue;
        let need = verts.len() as u64;
        if need > self.vert_cap {
            self.vert_cap = need.next_power_of_two().max(256);
            self.vert_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("skinned-verts"),
                size: self.vert_cap * std::mem::size_of::<SkinnedVertex>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.vert_buf, 0, cast_slice(verts));
        let view_u = ViewUniforms {
            view_proj: camera.view_proj().to_cols_array_2d(),
            eye: [camera.eye.x, camera.eye.y, camera.eye.z, 0.0],
            light_params: [0.0, 0.0, 0.0, 0.0],
            clip_plane: [0.0, 0.0, 0.0, 0.0],
            lightmap_params: [0.0, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.view_buf, 0, bytes_of(&view_u));
        let bones = BoneUniforms::from_skin_mats(skin_mats);
        queue.write_buffer(&self.bone_buf, 0, bytes_of(&bones));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("skinned-enc"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("skinned-rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if clear {
                            wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.05,
                                g: 0.05,
                                b: 0.08,
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
            pass.set_bind_group(1, &self.mat_bind, &[]);
            pass.set_vertex_buffer(0, self.vert_buf.slice(..));
            pass.draw(0..verts.len() as u32, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        let _ = &self.mat_bgl;
    }
}

fn make_white_tex(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("skinned-white"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255u8, 255, 255, 255],
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
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let samp = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    (tex, view, samp)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::models::{IqmJoint, ModelMesh, ModelVertex};
    use glam::{Quat, Vec3};

    #[test]
    fn skin_matrices_bind_pose_is_identity() {
        let base = Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0));
        let iqm = IqmModel {
            mesh: ModelMesh {
                vertices: vec![ModelVertex {
                    pos: [0.0, 0.0, 0.0],
                    uv: [0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                }],
                indices: vec![0, 0, 0],
            },
            joints: vec![IqmJoint {
                parent: -1,
                translate: Vec3::new(3.0, 0.0, 0.0),
                rotate: Quat::IDENTITY,
                scale: Vec3::ONE,
            }],
            base_mats: vec![base],
            inv_base_mats: vec![base.inverse()],
            blend_indexes: vec![[0, 0, 0, 0]],
            blend_weights: vec![[255, 0, 0, 0]],
        };
        let mats = skin_matrices(&iqm, &iqm.base_mats);
        let p = mats[0].transform_point3(Vec3::new(1.0, 2.0, 3.0));
        assert!((p.x - 1.0).abs() < 1e-4);
        assert!((p.y - 2.0).abs() < 1e-4);
        assert!((p.z - 3.0).abs() < 1e-4);
    }

    #[test]
    fn iqm_to_skinned_vertices_emits_tris() {
        let iqm = IqmModel {
            mesh: ModelMesh {
                vertices: vec![
                    ModelVertex {
                        pos: [0.0, 0.0, 0.0],
                        uv: [0.0, 0.0],
                        normal: [0.0, 0.0, 1.0],
                    },
                    ModelVertex {
                        pos: [1.0, 0.0, 0.0],
                        uv: [1.0, 0.0],
                        normal: [0.0, 0.0, 1.0],
                    },
                    ModelVertex {
                        pos: [0.0, 1.0, 0.0],
                        uv: [0.0, 1.0],
                        normal: [0.0, 0.0, 1.0],
                    },
                ],
                indices: vec![0, 1, 2],
            },
            joints: vec![IqmJoint {
                parent: -1,
                translate: Vec3::ZERO,
                rotate: Quat::IDENTITY,
                scale: Vec3::ONE,
            }],
            base_mats: vec![Mat4::IDENTITY],
            inv_base_mats: vec![Mat4::IDENTITY],
            blend_indexes: vec![[0, 0, 0, 0]; 3],
            blend_weights: vec![[255, 0, 0, 0]; 3],
        };
        let v = iqm_to_skinned_vertices(&iqm, Vec3::ZERO, Vec3::ONE, 1.0);
        assert_eq!(v.len(), 3);
        assert!((v[1].pos[0] - 1.0).abs() < 1e-5);
    }
}
