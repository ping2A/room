//! Battle-royale zone wall + ground ring (`docs/28` §9 BE1).

use std::mem::size_of;

use crate::camera::Camera;
use crate::device::GpuContext;
use crate::mesh::ViewUniforms;
use bytemuck::{Pod, Zeroable, bytes_of};
use room_map::MapGeometry;
use room_nodebuild::point_in_sector;

/// Draw flags (same bits as `room_sim::mod_zones::flags`).
pub mod flags {
    /// Do not draw (caller should skip).
    pub const HIDDEN: u8 = 1 << 0;
    /// Wall visible through map geometry (depth test off).
    pub const THROUGH_WALLS: u8 = 1 << 1;
}

/// One zone circle ready for meshing (already interpolated for display).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneDraw {
    /// Centre X (map units).
    pub cx: f32,
    /// Centre Y.
    pub cy: f32,
    /// Radius (map units).
    pub r: f32,
    /// RGBA guest u32 (R high byte).
    pub rgba: u32,
    /// [`flags`] bits.
    pub flags: u8,
}

/// GPU vertex for the zone pass.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ZoneVertex {
    /// XYZ map units.
    pub pos: [f32; 3],
    /// UV (animation).
    pub uv: [f32; 2],
    /// Premultiplied-ready RGBA 0–1.
    pub color: [f32; 4],
}

/// Built wall + ring geometry for one frame.
#[derive(Clone, Debug, Default)]
pub struct ZoneMeshes {
    /// Vertical cylinder quads (both depth modes batched separately at draw).
    pub wall_depth: Vec<ZoneVertex>,
    /// Same, drawn with depth test disabled.
    pub wall_through: Vec<ZoneVertex>,
    /// Ground ring (depth tested).
    pub ring: Vec<ZoneVertex>,
}

/// wgpu pass for mod zone walls (after main world, same depth buffer).
pub struct ModZonePass {
    pipeline_depth: wgpu::RenderPipeline,
    pipeline_through: wgpu::RenderPipeline,
    view_buf: wgpu::Buffer,
    view_bind: wgpu::BindGroup,
    wall_depth_buf: wgpu::Buffer,
    wall_through_buf: wgpu::Buffer,
    ring_buf: wgpu::Buffer,
    wall_depth_cap: usize,
    wall_through_cap: usize,
    ring_cap: usize,
}

impl ModZonePass {
    /// Create pipelines for `target_format` (swapchain / offscreen colour).
    #[must_use]
    pub fn new(gpu: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("zone-shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/zone.wgsl").into()),
            });
        let view_layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zone-view-layout"),
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
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("zone-layout"),
                bind_group_layouts: &[&view_layout],
                push_constant_ranges: &[],
            });
        let pipeline_depth = make_zone_pipeline(
            &gpu.device,
            &shader,
            &layout,
            target_format,
            "zone-depth",
            wgpu::CompareFunction::LessEqual,
        );
        let pipeline_through = make_zone_pipeline(
            &gpu.device,
            &shader,
            &layout,
            target_format,
            "zone-through",
            wgpu::CompareFunction::Always,
        );
        let view_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zone-view"),
            size: size_of::<ViewUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zone-view-bind"),
            layout: &view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_buf.as_entire_binding(),
            }],
        });
        let mk = |label: &str| {
            gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Self {
            pipeline_depth,
            pipeline_through,
            view_buf,
            view_bind,
            wall_depth_buf: mk("zone-wall-depth"),
            wall_through_buf: mk("zone-wall-through"),
            ring_buf: mk("zone-ring"),
            wall_depth_cap: 0,
            wall_through_cap: 0,
            ring_cap: 0,
        }
    }

    /// Upload view matrix + animation time.
    pub fn prepare(&self, gpu: &GpuContext, camera: &Camera, time_sec: f32) {
        let view_proj = camera.view_proj().to_cols_array_2d();
        let uniforms = ViewUniforms {
            view_proj,
            eye: [camera.eye.x, camera.eye.y, camera.eye.z, time_sec],
            light_params: [0.0; 4],
            clip_plane: [0.0; 4],
            lightmap_params: [0.0; 4],
        };
        gpu.queue
            .write_buffer(&self.view_buf, 0, bytes_of(&uniforms));
    }

    /// Stream mesh data built by [`build_zone_meshes`].
    pub fn upload(&mut self, gpu: &GpuContext, meshes: &ZoneMeshes) {
        self.wall_depth_buf = ensure_vb(
            &gpu.device,
            &gpu.queue,
            &self.wall_depth_buf,
            &mut self.wall_depth_cap,
            "zone-wall-depth",
            &meshes.wall_depth,
        );
        self.wall_through_buf = ensure_vb(
            &gpu.device,
            &gpu.queue,
            &self.wall_through_buf,
            &mut self.wall_through_cap,
            "zone-wall-through",
            &meshes.wall_through,
        );
        self.ring_buf = ensure_vb(
            &gpu.device,
            &gpu.queue,
            &self.ring_buf,
            &mut self.ring_cap,
            "zone-ring",
            &meshes.ring,
        );
    }

    /// Draw into an open render pass (depth loaded from the world pass).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, meshes: &ZoneMeshes) {
        pass.set_bind_group(0, &self.view_bind, &[]);
        if !meshes.wall_depth.is_empty() {
            pass.set_pipeline(&self.pipeline_depth);
            pass.set_vertex_buffer(0, self.wall_depth_buf.slice(..));
            #[allow(clippy::cast_possible_truncation)]
            pass.draw(0..meshes.wall_depth.len() as u32, 0..1);
        }
        if !meshes.ring.is_empty() {
            pass.set_pipeline(&self.pipeline_depth);
            pass.set_vertex_buffer(0, self.ring_buf.slice(..));
            #[allow(clippy::cast_possible_truncation)]
            pass.draw(0..meshes.ring.len() as u32, 0..1);
        }
        if !meshes.wall_through.is_empty() {
            pass.set_pipeline(&self.pipeline_through);
            pass.set_vertex_buffer(0, self.wall_through_buf.slice(..));
            #[allow(clippy::cast_possible_truncation)]
            pass.draw(0..meshes.wall_through.len() as u32, 0..1);
        }
    }
}

fn make_zone_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
    label: &str,
    depth_compare: wgpu::CompareFunction,
) -> wgpu::RenderPipeline {
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<ZoneVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: 12,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: 20,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[vertex_layout],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            // MRT: colour + dummy R8 (same layout as UnlitPass / world pass).
            entry_point: Some("fs_main_mrt"),
            targets: &[
                Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    // WebGL2: same blend/write_mask on both targets.
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DEPTH_STENCIL_FORMAT,
            depth_write_enabled: false,
            depth_compare,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: -4,
                slope_scale: -1.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn ensure_vb(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buf: &wgpu::Buffer,
    cap: &mut usize,
    label: &str,
    data: &[ZoneVertex],
) -> wgpu::Buffer {
    let need = data.len().max(1) * size_of::<ZoneVertex>();
    if need > *cap {
        *cap = need.next_power_of_two().max(64);
        let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: *cap as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !data.is_empty() {
            queue.write_buffer(&new_buf, 0, bytemuck::cast_slice(data));
        }
        new_buf
    } else if data.is_empty() {
        buf.clone()
    } else {
        queue.write_buffer(buf, 0, bytemuck::cast_slice(data));
        buf.clone()
    }
}

fn rgba_f32(c: u32) -> [f32; 4] {
    [
        ((c >> 24) & 0xff) as f32 / 255.0,
        ((c >> 16) & 0xff) as f32 / 255.0,
        ((c >> 8) & 0xff) as f32 / 255.0,
        (c & 0xff) as f32 / 255.0,
    ]
}

fn sector_heights(map: &MapGeometry, x: f64, y: f64) -> (f32, f32) {
    let Some(sid) = point_in_sector(map, x, y) else {
        return (0.0, 128.0);
    };
    let sec = &map.sectors[sid.raw() as usize];
    (sec.floor_height as f32, sec.ceiling_height as f32)
}

/// Build cylinder wall + ground ring for all visible zones.
#[must_use]
pub fn build_zone_meshes(map: &MapGeometry, zones: &[ZoneDraw]) -> ZoneMeshes {
    let mut out = ZoneMeshes::default();
    const WALL_SEGS: u32 = 48;
    const RING_SEGS: u32 = 64;
    const RING_HALF_W: f32 = 6.0;

    for zone in zones {
        if (zone.flags & flags::HIDDEN) != 0 || zone.r <= 0.0 {
            continue;
        }
        let color = rgba_f32(zone.rgba);
        let through = (zone.flags & flags::THROUGH_WALLS) != 0;
        let wall_target = if through {
            &mut out.wall_through
        } else {
            &mut out.wall_depth
        };
        let cx = f64::from(zone.cx);
        let cy = f64::from(zone.cy);
        let r = f64::from(zone.r);

        for i in 0..WALL_SEGS {
            let a0 = (i as f64 / f64::from(WALL_SEGS)) * std::f64::consts::TAU;
            let a1 = ((i + 1) as f64 / f64::from(WALL_SEGS)) * std::f64::consts::TAU;
            let (x0, y0) = (cx + a0.cos() * r, cy + a0.sin() * r);
            let (x1, y1) = (cx + a1.cos() * r, cy + a1.sin() * r);
            let (f0, c0) = sector_heights(map, x0, y0);
            let (f1, c1) = sector_heights(map, x1, y1);
            let u0 = i as f32 / WALL_SEGS as f32;
            let u1 = (i + 1) as f32 / WALL_SEGS as f32;
            let bl = ZoneVertex {
                pos: [x0 as f32, y0 as f32, f0],
                uv: [u0, 1.0],
                color,
            };
            let tl = ZoneVertex {
                pos: [x0 as f32, y0 as f32, c0],
                uv: [u0, 0.0],
                color,
            };
            let tr = ZoneVertex {
                pos: [x1 as f32, y1 as f32, c1],
                uv: [u1, 0.0],
                color,
            };
            let br = ZoneVertex {
                pos: [x1 as f32, y1 as f32, f1],
                uv: [u1, 1.0],
                color,
            };
            wall_target.extend_from_slice(&[bl, tl, tr, bl, tr, br]);
        }

        for i in 0..RING_SEGS {
            let a0 = (i as f64 / f64::from(RING_SEGS)) * std::f64::consts::TAU;
            let a1 = ((i + 1) as f64 / f64::from(RING_SEGS)) * std::f64::consts::TAU;
            let (ci0, si0) = (a0.cos(), a0.sin());
            let (ci1, si1) = (a1.cos(), a1.sin());
            let ri = r - f64::from(RING_HALF_W);
            let ro = r + f64::from(RING_HALF_W);
            let (x_in0, y_in0) = (cx + ci0 * ri, cy + si0 * ri);
            let (x_out0, y_out0) = (cx + ci0 * ro, cy + si0 * ro);
            let (x_in1, y_in1) = (cx + ci1 * ri, cy + si1 * ri);
            let (x_out1, y_out1) = (cx + ci1 * ro, cy + si1 * ro);
            let (fz, _) = sector_heights(map, cx + ci0 * r, cy + si0 * r);
            let z = fz + 2.0;
            let mut ring_color = color;
            ring_color[3] = (ring_color[3] * 0.85).clamp(0.0, 1.0);
            let u0 = i as f32 / RING_SEGS as f32;
            let u1 = (i + 1) as f32 / RING_SEGS as f32;
            let inner0 = ZoneVertex {
                pos: [x_in0 as f32, y_in0 as f32, z],
                uv: [u0, 0.0],
                color: ring_color,
            };
            let outer0 = ZoneVertex {
                pos: [x_out0 as f32, y_out0 as f32, z],
                uv: [u0, 1.0],
                color: ring_color,
            };
            let outer1 = ZoneVertex {
                pos: [x_out1 as f32, y_out1 as f32, z],
                uv: [u1, 1.0],
                color: ring_color,
            };
            let inner1 = ZoneVertex {
                pos: [x_in1 as f32, y_in1 as f32, z],
                uv: [u1, 0.0],
                color: ring_color,
            };
            out.ring
                .extend_from_slice(&[inner0, outer0, outer1, inner0, outer1, inner1]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn build_zone_meshes_nonempty() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(256.0, 256.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let zones = [ZoneDraw {
            cx: 256.0,
            cy: 256.0,
            r: 128.0,
            rgba: 0x00aaffcc,
            flags: 0,
        }];
        let m = build_zone_meshes(&map, &zones);
        assert!(!m.wall_depth.is_empty());
        assert!(!m.ring.is_empty());
        assert!(m.wall_depth.len().is_multiple_of(6));
    }

    #[test]
    fn through_walls_separate_batch() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let zones = [ZoneDraw {
            cx: 128.0,
            cy: 128.0,
            r: 64.0,
            rgba: 0xff000080,
            flags: flags::THROUGH_WALLS,
        }];
        let m = build_zone_meshes(&map, &zones);
        assert!(m.wall_depth.is_empty());
        assert!(!m.wall_through.is_empty());
    }
}
