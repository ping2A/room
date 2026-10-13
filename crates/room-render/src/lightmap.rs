//! ZDRay / GZDoom `LIGHTMAP` lump header + atlas sample bootstrap (M10).

use crate::device::GpuContext;
use bytemuck::{Pod, Zeroable, bytes_of};
use std::collections::BTreeMap;
use thiserror::Error;

const LIGHTMAP_WGSL: &str = include_str!("shaders/lightmap.wgsl");

/// Lightmap parse errors.
#[derive(Debug, Error)]
pub enum LightmapError {
    /// Truncated or invalid header.
    #[error("{0}")]
    Message(String),
}

/// Magic for Room's stretch lightmap header (`FLMP`).
pub const LIGHTMAP_MAGIC: &[u8; 4] = b"FLMP";

/// Parsed lightmap atlas header (bootstrap — full ZDRay format TBD).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightmapHeader {
    /// Format version.
    pub version: u32,
    /// Atlas width.
    pub width: u32,
    /// Atlas height.
    pub height: u32,
    /// Number of surface entries following the header.
    pub surface_count: u32,
}

/// Parse a stretch `FLMP` lightmap header (16 bytes after magic).
///
/// Layout: `FLMP` + u32 version + u32 width + u32 height + u32 surface_count.
pub fn parse_lightmap_header(data: &[u8]) -> Result<LightmapHeader, LightmapError> {
    if data.len() < 20 {
        return Err(LightmapError::Message("LIGHTMAP truncated".into()));
    }
    if &data[0..4] != LIGHTMAP_MAGIC {
        return Err(LightmapError::Message(
            "LIGHTMAP bad magic (want FLMP)".into(),
        ));
    }
    let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let width = u32::from_le_bytes(data[8..12].try_into().unwrap());
    let height = u32::from_le_bytes(data[12..16].try_into().unwrap());
    let surface_count = u32::from_le_bytes(data[16..20].try_into().unwrap());
    if version == 0 || width == 0 || height == 0 {
        return Err(LightmapError::Message(
            "LIGHTMAP bad dimensions/version".into(),
        ));
    }
    if width > 8192 || height > 8192 || surface_count > 1_000_000 {
        return Err(LightmapError::Message("LIGHTMAP limits exceeded".into()));
    }
    Ok(LightmapHeader {
        version,
        width,
        height,
        surface_count,
    })
}

/// One surface's atlas UV rect (ZDRay / FLMP stretch entry, 16 bytes).
#[derive(Clone, Debug, PartialEq)]
pub struct LightmapSurface {
    /// Atlas U0 (0..=1).
    pub u0: f32,
    /// Atlas V0.
    pub v0: f32,
    /// Atlas U1.
    pub u1: f32,
    /// Atlas V1.
    pub v1: f32,
}

/// Parse `surface_count` lightmap surface UV rects after a valid header.
///
/// Each entry: 4× f32 LE (`u0,v0,u1,v1`). Returns header + surfaces.
pub fn parse_lightmap_lump(
    data: &[u8],
) -> Result<(LightmapHeader, Vec<LightmapSurface>), LightmapError> {
    let header = parse_lightmap_header(data)?;
    let need = 20 + header.surface_count as usize * 16;
    if data.len() < need {
        return Err(LightmapError::Message("LIGHTMAP surfaces truncated".into()));
    }
    let mut surfaces = Vec::with_capacity(header.surface_count as usize);
    for i in 0..header.surface_count as usize {
        let o = 20 + i * 16;
        let u0 = f32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let v0 = f32::from_le_bytes(data[o + 4..o + 8].try_into().unwrap());
        let u1 = f32::from_le_bytes(data[o + 8..o + 12].try_into().unwrap());
        let v1 = f32::from_le_bytes(data[o + 12..o + 16].try_into().unwrap());
        surfaces.push(LightmapSurface { u0, v0, u1, v1 });
    }
    Ok((header, surfaces))
}

/// Side / sector → lightmap surface index (ZDRay binding table bootstrap).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LightmapBindings {
    /// Sidedef raw id → surface index into [`LightmapAtlas::surfaces`].
    pub sides: BTreeMap<u32, u32>,
    /// Sector raw id → surface index (floors/ceilings).
    pub sectors: BTreeMap<u32, u32>,
}

impl LightmapBindings {
    /// Empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a sidedef to a surface index.
    pub fn bind_side(&mut self, side_raw: u32, surface: u32) {
        self.sides.insert(side_raw, surface);
    }

    /// Bind a sector to a surface index.
    pub fn bind_sector(&mut self, sector_raw: u32, surface: u32) {
        self.sectors.insert(sector_raw, surface);
    }

    /// Surface index for a sidedef, if mapped.
    #[must_use]
    pub fn surface_for_side(&self, side_raw: u32) -> Option<u32> {
        self.sides.get(&side_raw).copied()
    }

    /// Surface index for a sector, if mapped.
    #[must_use]
    pub fn surface_for_sector(&self, sector_raw: u32) -> Option<u32> {
        self.sectors.get(&sector_raw).copied()
    }

    /// Resolve surface for a side, else sector, else `None`.
    #[must_use]
    pub fn resolve(&self, side_raw: Option<u32>, sector_raw: Option<u32>) -> Option<u32> {
        side_raw
            .and_then(|s| self.surface_for_side(s))
            .or_else(|| sector_raw.and_then(|s| self.surface_for_sector(s)))
    }
}

/// Map local `[0,1]` UV into a surface's atlas rect.
#[must_use]
pub fn surface_uv(surf: &LightmapSurface, local_u: f32, local_v: f32) -> [f32; 2] {
    let u = surf.u0 + (surf.u1 - surf.u0) * local_u.clamp(0.0, 1.0);
    let v = surf.v0 + (surf.v1 - surf.v0) * local_v.clamp(0.0, 1.0);
    [u, v]
}

/// World XY → surface atlas UV (`tile` map-units per atlas repeat).
#[must_use]
pub fn world_xy_to_surface_uv(surf: &LightmapSurface, x: f32, y: f32, tile: f32) -> [f32; 2] {
    let t = tile.max(1.0);
    let lu = (x / t).rem_euclid(1.0);
    let lv = (y / t).rem_euclid(1.0);
    surface_uv(surf, lu, lv)
}

/// Sample atlas texel indices for a UV in `[0,1]` (nearest, bootstrap).
#[must_use]
pub fn lightmap_atlas_texel(header: &LightmapHeader, u: f32, v: f32) -> (u32, u32) {
    let u = u.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let x = ((u * header.width as f32) as u32).min(header.width.saturating_sub(1));
    let y = ((v * header.height as f32) as u32).min(header.height.saturating_sub(1));
    (x, y)
}

/// CPU-side lightmap atlas (header + surfaces + RGBA8 texels).
#[derive(Clone, Debug, PartialEq)]
pub struct LightmapAtlas {
    /// Header.
    pub header: LightmapHeader,
    /// Per-surface UV rects.
    pub surfaces: Vec<LightmapSurface>,
    /// `width * height * 4` RGBA8.
    pub rgba: Vec<u8>,
}

impl LightmapAtlas {
    /// Build from FLMP lump bytes + raw RGBA atlas following the surface table.
    pub fn from_lump_and_rgba(lump: &[u8], rgba: &[u8]) -> Result<Self, LightmapError> {
        let (header, surfaces) = parse_lightmap_lump(lump)?;
        let need = header.width as usize * header.height as usize * 4;
        if rgba.len() < need {
            return Err(LightmapError::Message("LIGHTMAP rgba truncated".into()));
        }
        Ok(Self {
            header,
            surfaces,
            rgba: rgba[..need].to_vec(),
        })
    }

    /// Sample RGB (0..=1) for surface `surf` at local UV `u,v` in `[0,1]` (bilinear).
    #[must_use]
    pub fn sample(&self, surf: usize, u: f32, v: f32) -> [f32; 3] {
        let Some(s) = self.surfaces.get(surf) else {
            return [0.0, 0.0, 0.0];
        };
        let au = s.u0 + (s.u1 - s.u0) * u.clamp(0.0, 1.0);
        let av = s.v0 + (s.v1 - s.v0) * v.clamp(0.0, 1.0);
        sample_rgba_bilinear(&self.rgba, self.header.width, self.header.height, au, av)
    }
}

fn sample_rgba_bilinear(rgba: &[u8], w: u32, h: u32, u: f32, v: f32) -> [f32; 3] {
    if w == 0 || h == 0 || rgba.len() < 4 {
        return [0.0, 0.0, 0.0];
    }
    let x = u.clamp(0.0, 1.0) * (w - 1) as f32;
    let y = v.clamp(0.0, 1.0) * (h - 1) as f32;
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let fetch = |xx: u32, yy: u32| -> [f32; 3] {
        let i = ((yy * w + xx) * 4) as usize;
        [
            rgba[i] as f32 / 255.0,
            rgba[i + 1] as f32 / 255.0,
            rgba[i + 2] as f32 / 255.0,
        ]
    };
    let c00 = fetch(x0, y0);
    let c10 = fetch(x1, y0);
    let c01 = fetch(x0, y1);
    let c11 = fetch(x1, y1);
    let lerp = |a: [f32; 3], b: [f32; 3], t: f32| {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]
    };
    let top = lerp(c00, c10, fx);
    let bot = lerp(c01, c11, fx);
    lerp(top, bot, fy)
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct LmUniforms {
    params: [f32; 4],
}

/// GPU lightmap atlas bind + fullscreen sample pass (debug / bootstrap).
pub struct LightmapPass {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    ubo: wgpu::Buffer,
    sampler: wgpu::Sampler,
    atlas_tex: Option<wgpu::Texture>,
    atlas_view: Option<wgpu::TextureView>,
    width: u32,
    height: u32,
}

impl LightmapPass {
    /// Create against `format`.
    pub fn new(gpu: &GpuContext, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lightmap"),
            source: wgpu::ShaderSource::Wgsl(LIGHTMAP_WGSL.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lm-bgl"),
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
            label: Some("lm-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lm-pipe"),
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let ubo = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lm-ubo"),
            size: std::mem::size_of::<LmUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("lm-samp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            bgl,
            ubo,
            sampler,
            atlas_tex: None,
            atlas_view: None,
            width: 0,
            height: 0,
        }
    }

    /// Upload / replace atlas pixels from a [`LightmapAtlas`].
    pub fn upload(&mut self, gpu: &GpuContext, atlas: &LightmapAtlas) {
        let w = atlas.header.width.max(1);
        let h = atlas.header.height.max(1);
        let device = &gpu.device;
        let queue = &gpu.queue;
        if self.width != w || self.height != h || self.atlas_tex.is_none() {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("lm-atlas"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.atlas_view = Some(tex.create_view(&wgpu::TextureViewDescriptor::default()));
            self.atlas_tex = Some(tex);
            self.width = w;
            self.height = h;
        }
        let Some(tex) = &self.atlas_tex else {
            return;
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Draw atlas fullscreen into `color` (debug visualization).
    pub fn draw_debug(&self, gpu: &GpuContext, color: &wgpu::TextureView, strength: f32) {
        let Some(view) = &self.atlas_view else {
            return;
        };
        let u = LmUniforms {
            params: [self.width as f32, self.height as f32, 0.0, strength],
        };
        gpu.queue.write_buffer(&self.ubo, 0, bytes_of(&u));
        let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lm-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.ubo.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("lm-enc"),
            });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lm-rp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
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
        gpu.queue.submit(Some(encoder.finish()));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_header() {
        let mut buf = Vec::new();
        buf.extend_from_slice(LIGHTMAP_MAGIC);
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&256u32.to_le_bytes());
        buf.extend_from_slice(&128u32.to_le_bytes());
        buf.extend_from_slice(&10u32.to_le_bytes());
        let h = parse_lightmap_header(&buf).unwrap();
        assert_eq!(h.width, 256);
        assert_eq!(h.height, 128);
        assert_eq!(h.surface_count, 10);
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(parse_lightmap_header(&[0u8; 20]).is_err());
    }

    #[test]
    fn parses_surfaces_and_samples() {
        let mut buf = Vec::new();
        buf.extend_from_slice(LIGHTMAP_MAGIC);
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&64u32.to_le_bytes());
        buf.extend_from_slice(&64u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        let (h, surfs) = parse_lightmap_lump(&buf).unwrap();
        assert_eq!(surfs.len(), 1);
        assert!((surfs[0].u1 - 1.0).abs() < 1e-5);
        let (x, y) = lightmap_atlas_texel(&h, 0.5, 0.25);
        assert_eq!(x, 32);
        assert_eq!(y, 16);
    }

    #[test]
    fn bindings_resolve_side_then_sector() {
        let mut b = LightmapBindings::new();
        b.bind_side(3, 7);
        b.bind_sector(1, 2);
        assert_eq!(b.surface_for_side(3), Some(7));
        assert_eq!(b.resolve(Some(3), Some(1)), Some(7));
        assert_eq!(b.resolve(None, Some(1)), Some(2));
        assert_eq!(b.resolve(Some(99), None), None);
    }

    #[test]
    fn world_xy_maps_into_surface_rect() {
        let s = LightmapSurface {
            u0: 0.25,
            v0: 0.25,
            u1: 0.75,
            v1: 0.75,
        };
        let uv = world_xy_to_surface_uv(&s, 0.0, 0.0, 256.0);
        assert!((uv[0] - 0.25).abs() < 1e-4);
        assert!((uv[1] - 0.25).abs() < 1e-4);
        let mid = surface_uv(&s, 0.5, 0.5);
        assert!((mid[0] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn atlas_bilinear_sample() {
        let mut lump = Vec::new();
        lump.extend_from_slice(LIGHTMAP_MAGIC);
        lump.extend_from_slice(&1u32.to_le_bytes());
        lump.extend_from_slice(&2u32.to_le_bytes());
        lump.extend_from_slice(&2u32.to_le_bytes());
        lump.extend_from_slice(&1u32.to_le_bytes());
        lump.extend_from_slice(&0.0f32.to_le_bytes());
        lump.extend_from_slice(&0.0f32.to_le_bytes());
        lump.extend_from_slice(&1.0f32.to_le_bytes());
        lump.extend_from_slice(&1.0f32.to_le_bytes());
        // 2×2: red, green / blue, white
        let rgba = [
            255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let atlas = LightmapAtlas::from_lump_and_rgba(&lump, &rgba).unwrap();
        let c = atlas.sample(0, 0.0, 0.0);
        assert!(c[0] > 0.9 && c[1] < 0.1);
        let mid = atlas.sample(0, 0.5, 0.5);
        assert!(mid[0] > 0.2 && mid[1] > 0.2 && mid[2] > 0.2);
    }
}
