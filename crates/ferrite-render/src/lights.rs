//! Dynamic light GPU upload + CPU cluster assignment (M10).

use crate::scene::DynLightInstance;
use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};

/// Max lights uploaded per frame (uniform-array friendly).
pub const MAX_GPU_LIGHTS: usize = 32;

/// Samples per light in the 1D shadowmap atlas (GZDoom `gl_shadowmap_quality` subset).
pub const SHADOWMAP_RES: usize = 512;

/// Cluster grid X (docs/13 §5.2 — GZDoom-like 16×9×24).
pub const CLUSTER_X: usize = 16;
/// Cluster grid Y.
pub const CLUSTER_Y: usize = 9;
/// Cluster grid Z (depth slices).
pub const CLUSTER_Z: usize = 24;
/// Total clusters.
pub const CLUSTER_COUNT: usize = CLUSTER_X * CLUSTER_Y * CLUSTER_Z;

/// One GPU light (`pos.xyz` + `radius` in `pos.w`; `color.rgb` + subtractive flag in `color.w`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuLight {
    /// XYZ position, W = radius.
    pub pos_radius: [f32; 4],
    /// RGB intensity, W = `1` if subtractive else `0`.
    pub color_flags: [f32; 4],
    /// Spotlight direction XYZ + outer cone cos (`w ≥ 1` = omni).
    pub spot_dir_outer: [f32; 4],
    /// Inner cone cos in `x`; `yzw` pad.
    pub spot_inner: [f32; 4],
}

/// Per-frame dyn-light uniform block (+ cluster volume params).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LightUniforms {
    /// `x` = light count, `y` = `1` if cluster masks active, `z` = `1` if 1D shadowmaps.
    pub count: [u32; 4],
    /// Cluster volume origin (XYZ), pad.
    pub origin: [f32; 4],
    /// Cluster cell size (XYZ), pad.
    pub cell: [f32; 4],
    /// Active lights (trailing slots ignored).
    pub lights: [GpuLight; MAX_GPU_LIGHTS],
}

impl Default for LightUniforms {
    fn default() -> Self {
        Self::ZERO
    }
}

impl LightUniforms {
    /// All-zero block.
    pub const ZERO: Self = Self {
        count: [0; 4],
        origin: [0.0; 4],
        cell: [1.0, 1.0, 1.0, 0.0],
        lights: [GpuLight {
            pos_radius: [0.0; 4],
            color_flags: [0.0; 4],
            spot_dir_outer: [0.0, 0.0, -1.0, 1.0],
            spot_inner: [1.0, 0.0, 0.0, 0.0],
        }; MAX_GPU_LIGHTS],
    };

    /// Pack CPU light list (truncated to [`MAX_GPU_LIGHTS`]).
    #[must_use]
    pub fn from_instances(instances: &[DynLightInstance]) -> Self {
        let mut out = Self::ZERO;
        let n = instances.len().min(MAX_GPU_LIGHTS);
        out.count[0] = n as u32;
        for (i, lit) in instances.iter().take(n).enumerate() {
            let (dir, outer) = match lit.spot_dir {
                Some(d) if lit.spot_outer_cos < 0.999 => (d, lit.spot_outer_cos),
                _ => (Vec3::new(0.0, 0.0, -1.0), 1.0),
            };
            out.lights[i] = GpuLight {
                pos_radius: [lit.pos.x, lit.pos.y, lit.pos.z, lit.radius.max(1.0)],
                color_flags: [
                    lit.color[0],
                    lit.color[1],
                    lit.color[2],
                    if lit.subtractive { 1.0 } else { 0.0 },
                ],
                spot_dir_outer: [dir.x, dir.y, dir.z, outer],
                spot_inner: [lit.spot_inner_cos.max(outer), 0.0, 0.0, 0.0],
            };
        }
        out
    }
}

/// CPU cluster assignment result (bitmasks + volume).
#[derive(Clone, Debug)]
pub struct ClusterAssignment {
    /// Uniforms with lights + volume params (`count.y = 1` when clustered).
    pub uniforms: LightUniforms,
    /// Per-cluster light bitmasks (`CLUSTER_COUNT` entries).
    pub masks: Vec<u32>,
}

impl ClusterAssignment {
    /// Empty / no lights.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            uniforms: LightUniforms::ZERO,
            masks: vec![0u32; CLUSTER_COUNT],
        }
    }

    /// Build a world-space cluster volume around `eye` and assign lights.
    ///
    /// Volume is axis-aligned, centred on the camera XY with Z from floor-ish to
    /// ceiling-ish extents derived from light radii (fallback 2048²×1024).
    #[must_use]
    pub fn build(eye: Vec3, instances: &[DynLightInstance]) -> Self {
        let mut uniforms = LightUniforms::from_instances(instances);
        let n = uniforms.count[0] as usize;
        if n == 0 {
            return Self {
                uniforms,
                masks: vec![0u32; CLUSTER_COUNT],
            };
        }

        // Volume: cover lights + a pad around the eye.
        let mut min = eye - Vec3::new(1024.0, 1024.0, 256.0);
        let mut max = eye + Vec3::new(1024.0, 1024.0, 768.0);
        for lit in instances.iter().take(n) {
            let r = lit.radius.max(1.0);
            min = min.min(lit.pos - Vec3::splat(r));
            max = max.max(lit.pos + Vec3::splat(r));
        }
        let extent = (max - min).max(Vec3::splat(64.0));
        let cell = Vec3::new(
            extent.x / CLUSTER_X as f32,
            extent.y / CLUSTER_Y as f32,
            extent.z / CLUSTER_Z as f32,
        )
        .max(Vec3::splat(1.0));

        uniforms.origin = [min.x, min.y, min.z, 0.0];
        uniforms.cell = [cell.x, cell.y, cell.z, 0.0];
        uniforms.count[1] = 1; // clustered path enabled

        let mut masks = vec![0u32; CLUSTER_COUNT];
        for (li, lit) in instances.iter().take(n).enumerate() {
            let bit = 1u32 << li;
            let r = lit.radius.max(1.0);
            let lo = lit.pos - Vec3::splat(r);
            let hi = lit.pos + Vec3::splat(r);
            let (x0, y0, z0) = world_to_cluster(lo, min, cell);
            let (x1, y1, z1) = world_to_cluster(hi, min, cell);
            for z in z0..=z1 {
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let idx = cluster_index(x, y, z);
                        masks[idx] |= bit;
                    }
                }
            }
        }

        Self { uniforms, masks }
    }
}

fn world_to_cluster(p: Vec3, origin: Vec3, cell: Vec3) -> (usize, usize, usize) {
    let lx = ((p.x - origin.x) / cell.x).floor() as i32;
    let ly = ((p.y - origin.y) / cell.y).floor() as i32;
    let lz = ((p.z - origin.z) / cell.z).floor() as i32;
    (
        lx.clamp(0, CLUSTER_X as i32 - 1) as usize,
        ly.clamp(0, CLUSTER_Y as i32 - 1) as usize,
        lz.clamp(0, CLUSTER_Z as i32 - 1) as usize,
    )
}

#[must_use]
fn cluster_index(x: usize, y: usize, z: usize) -> usize {
    x + y * CLUSTER_X + z * CLUSTER_X * CLUSTER_Y
}

/// Texture size used to store cluster masks (64×54 = 3456).
#[must_use]
pub fn cluster_tex_dims() -> (u32, u32) {
    (64, 54)
}

/// Pack masks into RGBA8 texels (little-endian u32 per texel).
#[must_use]
pub fn pack_cluster_rgba(masks: &[u32]) -> Vec<u8> {
    let (w, h) = cluster_tex_dims();
    let need = (w * h) as usize;
    let mut rgba = vec![0u8; need * 4];
    for (i, &m) in masks.iter().take(need).enumerate() {
        let o = i * 4;
        rgba[o] = (m & 0xFF) as u8;
        rgba[o + 1] = ((m >> 8) & 0xFF) as u8;
        rgba[o + 2] = ((m >> 16) & 0xFF) as u8;
        rgba[o + 3] = ((m >> 24) & 0xFF) as u8;
    }
    rgba
}

/// One XY wall segment that blocks dynamic light (map units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowOccluder {
    /// Segment start.
    pub a: Vec2,
    /// Segment end.
    pub b: Vec2,
}

/// 1D shadowmap atlas dims (`SHADOWMAP_RES` × `MAX_GPU_LIGHTS`).
#[must_use]
pub fn shadowmap_tex_dims() -> (u32, u32) {
    (SHADOWMAP_RES as u32, MAX_GPU_LIGHTS as u32)
}

/// Bake per-light 1D occlusion distances (row-major, one row per light).
///
/// For each of [`SHADOWMAP_RES`] directions in the XY plane, stores the distance
/// to the nearest occluder (or the light radius if none). Matches GZDoom's
/// `gl_light_shadowmap` idea at CPU quality (GPU ray BVH still open).
#[must_use]
pub fn bake_1d_shadowmaps(
    instances: &[DynLightInstance],
    occluders: &[ShadowOccluder],
) -> Vec<f32> {
    let n = instances.len().min(MAX_GPU_LIGHTS);
    let mut atlas = vec![0.0f32; SHADOWMAP_RES * MAX_GPU_LIGHTS];
    if n == 0 || occluders.is_empty() {
        // No occlusion → full radius (set after we know each light).
        for (li, lit) in instances.iter().take(n).enumerate() {
            let r = lit.radius.max(1.0);
            let row = &mut atlas[li * SHADOWMAP_RES..(li + 1) * SHADOWMAP_RES];
            row.fill(r);
        }
        return atlas;
    }
    for (li, lit) in instances.iter().take(n).enumerate() {
        let origin = lit.pos.truncate();
        let radius = lit.radius.max(1.0);
        let row = &mut atlas[li * SHADOWMAP_RES..(li + 1) * SHADOWMAP_RES];
        for s in 0..SHADOWMAP_RES {
            let ang = -std::f32::consts::PI
                + (s as f32 + 0.5) * (std::f32::consts::TAU / SHADOWMAP_RES as f32);
            let dir = Vec2::new(ang.cos(), ang.sin());
            let mut best = radius;
            for occ in occluders {
                if let Some(t) = ray_segment_hit(origin, dir, occ.a, occ.b) {
                    if t > 1e-3 && t < best {
                        best = t;
                    }
                }
            }
            row[s] = best;
        }
    }
    atlas
}

/// Ray `origin + t * dir` (`dir` unit) vs segment `a→b`. Returns `t` if hit.
#[must_use]
fn ray_segment_hit(origin: Vec2, dir: Vec2, a: Vec2, b: Vec2) -> Option<f32> {
    let e = b - a;
    let det = dir.x * e.y - dir.y * e.x;
    if det.abs() < 1e-8 {
        return None;
    }
    let f = a - origin;
    let t = (f.x * e.y - f.y * e.x) / det;
    let u = (f.x * dir.y - f.y * dir.x) / det;
    if t > 1e-4 && (0.0..=1.0).contains(&u) {
        Some(t)
    } else {
        None
    }
}

/// Pack f32 atlas for `R32Float` upload (raw little-endian bytes).
#[must_use]
pub fn pack_shadowmap_r32(atlas: &[f32]) -> Vec<u8> {
    let (w, h) = shadowmap_tex_dims();
    let need = (w * h) as usize;
    let mut out = vec![0u8; need * 4];
    for (i, &v) in atlas.iter().take(need).enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn packs_spotlight_cone() {
        let lit = DynLightInstance::spot(
            Vec3::ZERO,
            [1.0, 1.0, 1.0],
            64.0,
            Vec3::new(0.0, 1.0, 0.0),
            45.0,
            20.0,
        );
        let u = LightUniforms::from_instances(&[lit]);
        assert!(u.lights[0].spot_dir_outer[3] < 0.999);
        assert!(u.lights[0].spot_inner[0] > u.lights[0].spot_dir_outer[3]);
    }

    #[test]
    fn packs_and_truncates() {
        let mut many = Vec::new();
        for i in 0..40 {
            many.push(DynLightInstance {
                pos: Vec3::new(i as f32, 0.0, 0.0),
                color: [1.0, 0.5, 0.25],
                radius: 32.0,
                subtractive: false,
                spot_dir: None,
                spot_outer_cos: 1.0,
                spot_inner_cos: 1.0,
            });
        }
        let u = LightUniforms::from_instances(&many);
        assert_eq!(u.count[0], MAX_GPU_LIGHTS as u32);
        assert!((u.lights[0].pos_radius[0] - 0.0).abs() < 1e-5);
        assert!((u.lights[0].color_flags[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn cluster_assigns_nearby_light() {
        let eye = Vec3::new(0.0, 0.0, 40.0);
        let lights = [DynLightInstance {
            pos: Vec3::new(10.0, 0.0, 40.0),
            color: [1.0, 1.0, 1.0],
            radius: 64.0,
            subtractive: false,
            spot_dir: None,
            spot_outer_cos: 1.0,
            spot_inner_cos: 1.0,
        }];
        let a = ClusterAssignment::build(eye, &lights);
        assert_eq!(a.uniforms.count[0], 1);
        assert_eq!(a.uniforms.count[1], 1);
        assert_eq!(a.masks.len(), CLUSTER_COUNT);
        assert!(
            a.masks.iter().any(|&m| m & 1 != 0),
            "light should mark clusters"
        );
        let lit = a.masks.iter().filter(|&&m| m & 1 != 0).count();
        assert!(lit > 0 && lit < CLUSTER_COUNT, "not every cluster: {lit}");
    }

    #[test]
    fn pack_rgba_roundtrip_bits() {
        let mut masks = vec![0u32; CLUSTER_COUNT];
        masks[0] = 0xA1B2C3D4;
        let rgba = pack_cluster_rgba(&masks);
        assert_eq!(rgba[0], 0xD4);
        assert_eq!(rgba[1], 0xC3);
        assert_eq!(rgba[2], 0xB2);
        assert_eq!(rgba[3], 0xA1);
    }

    #[test]
    fn shadowmap_blocks_behind_wall() {
        let lights = [DynLightInstance {
            pos: Vec3::new(0.0, 0.0, 40.0),
            color: [1.0, 1.0, 1.0],
            radius: 200.0,
            subtractive: false,
            spot_dir: None,
            spot_outer_cos: 1.0,
            spot_inner_cos: 1.0,
        }];
        // Wall across +X at x=50.
        let occ = [ShadowOccluder {
            a: Vec2::new(50.0, -100.0),
            b: Vec2::new(50.0, 100.0),
        }];
        let atlas = bake_1d_shadowmaps(&lights, &occ);
        // Sample index for angle ≈ 0 (+X).
        let s0 = (SHADOWMAP_RES / 2) as usize;
        assert!(atlas[s0] < 60.0, "hit wall ~50, got {}", atlas[s0]);
        // Opposite direction (−X) should be free to radius.
        let s_opp = 0usize;
        assert!(
            (atlas[s_opp] - 200.0).abs() < 1.0,
            "open direction should be radius, got {}",
            atlas[s_opp]
        );
    }

    #[test]
    fn ray_hits_segment() {
        let t = ray_segment_hit(
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            Vec2::new(10.0, -5.0),
            Vec2::new(10.0, 5.0),
        );
        assert!((t.unwrap() - 10.0).abs() < 1e-4);
    }
}
