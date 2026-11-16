//! Classic cylinder / cubemap sky meshes (software-renderer style).

use crate::mesh::WallVertex;
use glam::{Vec2, Vec3};

/// Add `du` to every sky vertex U (ACS `SetSkyScrollSpeed` / MAPINFO scroll).
pub fn scroll_sky_u(verts: &mut [WallVertex], du: f32) {
    if du == 0.0 {
        return;
    }
    for v in verts {
        v.uv[0] += du;
    }
}

/// Build a closed sky shell: tall cylinder + top/bottom caps around the origin.
///
/// `radius` / `height` are map units. `segments` ≥ 3. Yaw scroll via [`scroll_sky_u`].
/// Height must cover the vertical FOV (`atan(height/2 / radius)`); the scene uses a
/// tall shell so outdoor look-up does not fall off into clear colour.
#[must_use]
pub fn cylinder_sky(radius: f32, height: f32, segments: u32, light: f32) -> Vec<WallVertex> {
    let segs = segments.max(3);
    let mut out = Vec::with_capacity((segs * VERTS_PER_SEG) as usize);
    let half = height * 0.5;
    for i in 0..segs {
        let a0 = (i as f32 / segs as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segs as f32) * std::f32::consts::TAU;
        let (s0, c0) = (a0.sin(), a0.cos());
        let (s1, c1) = (a1.sin(), a1.cos());
        let u0 = i as f32 / segs as f32;
        let u1 = (i + 1) as f32 / segs as f32;
        let p00 = Vec3::new(c0 * radius, s0 * radius, -half);
        let p10 = Vec3::new(c1 * radius, s1 * radius, -half);
        let p11 = Vec3::new(c1 * radius, s1 * radius, half);
        let p01 = Vec3::new(c0 * radius, s0 * radius, half);
        let a = WallVertex::new(p00, Vec2::new(u0, 1.0), light);
        let b = WallVertex::new(p10, Vec2::new(u1, 1.0), light);
        let c = WallVertex::new(p11, Vec2::new(u1, 0.0), light);
        let d = WallVertex::new(p01, Vec2::new(u0, 0.0), light);
        // Inward-facing (camera inside): reverse winding.
        out.extend_from_slice(&[a, d, c, a, c, b]);
    }
    // Caps so steep look-up / look-down stay inside the shell: concentric
    // rings over the [`sky_cap_image`] gradient (v = 0 at the rim is the
    // sky's edge row, fading to its average at the centre). All top
    // triangles, then all bottom ones (see [`cylinder_sky_ranges`]).
    let v_max = (CAP_ROWS as f32 - 0.5) / CAP_ROWS as f32;
    for top in [true, false] {
        let z = if top { half } else { -half };
        for j in 0..CAP_ROWS {
            let ring = |r: u32, k: u32| {
                let a = (k as f32 / segs as f32) * std::f32::consts::TAU;
                let f = r as f32 / CAP_ROWS as f32;
                let rr = radius * (1.0 - f);
                WallVertex::new(
                    Vec3::new(a.cos() * rr, a.sin() * rr, z),
                    Vec2::new(k as f32 / segs as f32, f * v_max),
                    light,
                )
            };
            for i in 0..segs {
                let (o0, o1) = (ring(j, i), ring(j, i + 1));
                let (n0, n1) = (ring(j + 1, i), ring(j + 1, i + 1));
                // Inward winding: down from +Z, up from −Z.
                if top {
                    out.extend_from_slice(&[o0, n1, o1, o0, n0, n1]);
                } else {
                    out.extend_from_slice(&[o0, o1, n1, o0, n1, n0]);
                }
            }
        }
    }
    out
}

/// Rings per sky cap, and rows of its [`sky_cap_image`] gradient.
pub const CAP_ROWS: u32 = 16;
const VERTS_PER_SEG: u32 = 6 + 2 * 6 * CAP_ROWS;

/// Sky cap gradient (`w` × [`CAP_ROWS`]) from a decoded sky `rgba` (`w` × `h`):
/// row 0 is the sky's top (or bottom) row, blending to the average of its
/// edge rows, which GZDoom fills above and below a sky texture.
#[must_use]
pub fn sky_cap_image(w: u32, h: u32, rgba: &[u8], top: bool) -> Option<Vec<u8>> {
    let (wu, hu) = (w as usize, h as usize);
    if wu == 0 || hu == 0 || rgba.len() < wu * hu * 4 {
        return None;
    }
    let rows = hu.min(4);
    let first = if top { 0 } else { hu - rows };
    let mut sum = [0u64; 3];
    let mut n = 0u64;
    for px in rgba[first * wu * 4..(first + rows) * wu * 4]
        .chunks_exact(4)
        .filter(|p| p[3] != 0)
    {
        for c in 0..3 {
            sum[c] += u64::from(px[c]);
        }
        n += 1;
    }
    let avg: [u32; 3] = std::array::from_fn(|c| u32::try_from(sum[c] / n.max(1)).unwrap_or(255));
    let edge = if top { 0 } else { hu - 1 };
    // Wrap-around prefix sums of the edge row (transparent texels → average):
    // each ring blurs it wider towards the centre instead of streaking.
    let mut prefix = vec![[0u64; 3]; 3 * wu + 1];
    for i in 0..3 * wu {
        let px = &rgba[(edge * wu + i % wu) * 4..][..4];
        for c in 0..3 {
            let v = if px[3] == 0 { avg[c] } else { u32::from(px[c]) };
            prefix[i + 1][c] = prefix[i][c] + u64::from(v);
        }
    }
    let last = CAP_ROWS - 1;
    let mut out = Vec::with_capacity(wu * CAP_ROWS as usize * 4);
    for r in 0..CAP_ROWS {
        let k = (wu * r as usize / (2 * CAP_ROWS as usize)).min(wu.saturating_sub(1) / 2);
        let span = (2 * k + 1) as u64;
        for x in 0..wu {
            let (lo, hi) = (wu + x - k, wu + x + k + 1);
            for c in 0..3 {
                let blurred = u32::try_from((prefix[hi][c] - prefix[lo][c]) / span).unwrap_or(255);
                let mixed = (blurred * (last - r) + avg[c] * r) / last;
                out.push(u8::try_from(mixed).unwrap_or(255));
            }
            out.push(255);
        }
    }
    Some(out)
}

/// Material name of a sky cap: a gradient from the sky's top (or bottom) row
/// to their average, as GZDoom fills above and below a sky texture.
#[must_use]
pub fn sky_cap_name(sky: &str, top: bool) -> String {
    format!("{}{}", if top { CAP_TOP } else { CAP_BOTTOM }, sky)
}

const CAP_TOP: &str = "__SKYCAP_T:";
const CAP_BOTTOM: &str = "__SKYCAP_B:";

/// Inverse of [`sky_cap_name`] on a normalized (upper-case) name.
#[must_use]
pub fn parse_sky_cap_name(name: &str) -> Option<(&str, bool)> {
    name.strip_prefix(CAP_TOP)
        .map(|s| (s, true))
        .or_else(|| name.strip_prefix(CAP_BOTTOM).map(|s| (s, false)))
        .filter(|(s, _)| !s.is_empty())
}

/// Side / top cap / bottom cap ranges of a [`cylinder_sky`] mesh of `len`
/// vertices textured with `sky`.
#[must_use]
pub fn cylinder_sky_ranges(sky: &str, len: usize) -> Vec<crate::scene::MaterialRange> {
    let segs = u32::try_from(len / VERTS_PER_SEG as usize).unwrap_or(0);
    if segs == 0 {
        return Vec::new();
    }
    let cap = segs * 6 * CAP_ROWS;
    [
        (sky.to_owned(), 0, segs * 6),
        (sky_cap_name(sky, true), segs * 6, cap),
        (sky_cap_name(sky, false), segs * 6 + cap, cap),
    ]
    .into_iter()
    .map(|(name, start, count)| crate::scene::MaterialRange {
        name,
        start,
        count,
        flags: 0,
    })
    .collect()
}

/// Six GLDEFS skybox face textures: north, east, south, west, top, bottom.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CubemapFaces {
    /// +Y face.
    pub north: String,
    /// +X face.
    pub east: String,
    /// −Y face.
    pub south: String,
    /// −X face.
    pub west: String,
    /// +Z face (`up` / `top`).
    pub top: String,
    /// −Z face (`down` / `bottom`).
    pub bottom: String,
}

impl CubemapFaces {
    /// True when all six faces have a texture name.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self.north.is_empty()
            && !self.east.is_empty()
            && !self.south.is_empty()
            && !self.west.is_empty()
            && !self.top.is_empty()
            && !self.bottom.is_empty()
    }

    /// Prefer north, else first non-empty (cylinder bootstrap).
    #[must_use]
    pub fn primary(&self) -> &str {
        [
            self.north.as_str(),
            self.east.as_str(),
            self.south.as_str(),
            self.west.as_str(),
            self.top.as_str(),
            self.bottom.as_str(),
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or("")
    }

    /// Build from a GLDEFS face map (`north`/`east`/…/`up`/`down`).
    #[must_use]
    pub fn from_gldefs(faces: &std::collections::BTreeMap<String, String>) -> Self {
        let get = |keys: &[&str]| {
            keys.iter()
                .find_map(|k| faces.get(*k).cloned())
                .unwrap_or_default()
        };
        Self {
            north: get(&["north"]),
            east: get(&["east"]),
            south: get(&["south"]),
            west: get(&["west"]),
            top: get(&["top", "up"]),
            bottom: get(&["bottom", "down"]),
        }
    }
}

/// Inward-facing cube verts + face texture names (each face = 6 verts, order NESWTB).
#[must_use]
pub fn cubemap_sky(half: f32, faces: &CubemapFaces, light: f32) -> (Vec<WallVertex>, [String; 6]) {
    let names = [
        faces.north.clone(),
        faces.east.clone(),
        faces.south.clone(),
        faces.west.clone(),
        faces.top.clone(),
        faces.bottom.clone(),
    ];
    let corners_list = [
        // +Y north
        [
            Vec3::new(-half, half, -half),
            Vec3::new(half, half, -half),
            Vec3::new(half, half, half),
            Vec3::new(-half, half, half),
        ],
        // +X east
        [
            Vec3::new(half, half, -half),
            Vec3::new(half, -half, -half),
            Vec3::new(half, -half, half),
            Vec3::new(half, half, half),
        ],
        // −Y south
        [
            Vec3::new(half, -half, -half),
            Vec3::new(-half, -half, -half),
            Vec3::new(-half, -half, half),
            Vec3::new(half, -half, half),
        ],
        // −X west
        [
            Vec3::new(-half, -half, -half),
            Vec3::new(-half, half, -half),
            Vec3::new(-half, half, half),
            Vec3::new(-half, -half, half),
        ],
        // +Z top
        [
            Vec3::new(-half, -half, half),
            Vec3::new(-half, half, half),
            Vec3::new(half, half, half),
            Vec3::new(half, -half, half),
        ],
        // −Z bottom
        [
            Vec3::new(-half, half, -half),
            Vec3::new(-half, -half, -half),
            Vec3::new(half, -half, -half),
            Vec3::new(half, half, -half),
        ],
    ];
    let mut verts = Vec::with_capacity(36);
    for corners in corners_list {
        let a = WallVertex::new(corners[0], Vec2::new(0.0, 1.0), light);
        let b = WallVertex::new(corners[1], Vec2::new(1.0, 1.0), light);
        let c = WallVertex::new(corners[2], Vec2::new(1.0, 0.0), light);
        let d = WallVertex::new(corners[3], Vec2::new(0.0, 0.0), light);
        verts.extend_from_slice(&[a, d, c, a, c, b]);
    }
    (verts, names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sky_has_tris() {
        let v = cylinder_sky(1024.0, 512.0, 16, 1.0);
        // Sides + top rings + bottom rings.
        assert_eq!(v.len(), (16 * VERTS_PER_SEG) as usize);
        let r = cylinder_sky_ranges("SKY1", v.len());
        assert_eq!(r[2].start + r[2].count, v.len() as u32);
    }

    #[test]
    fn sky_cap_fades_from_edge_row_to_average() {
        // 2×2 sky: top row black / white, bottom row grey.
        let rgba = [
            0, 0, 0, 255, 255, 255, 255, 255, 100, 100, 100, 255, 100, 100, 100, 255,
        ];
        let cap = sky_cap_image(2, 2, &rgba, true).unwrap();
        assert_eq!(cap.len(), 2 * CAP_ROWS as usize * 4);
        assert_eq!(&cap[..8], &[0, 0, 0, 255, 255, 255, 255, 255]);
        let last = &cap[(CAP_ROWS as usize - 1) * 8..];
        // Average of the (≤4) top rows: (0 + 255 + 100 + 100) / 4.
        assert_eq!(&last[..3], &[113, 113, 113]);
        assert_eq!(&last[4..7], &[113, 113, 113]);
    }

    #[test]
    fn sky_shell_covers_steep_look_up() {
        // Scene defaults: radius 4096, height 2*radius → ±45° at the rim, plus caps.
        let radius = 4096.0_f32;
        let height = radius * 2.0;
        let v = cylinder_sky(radius, height, 32, 1.0);
        let max_z = v.iter().map(|p| p.pos[2]).fold(f32::NEG_INFINITY, f32::max);
        let min_z = v.iter().map(|p| p.pos[2]).fold(f32::INFINITY, f32::min);
        assert!((max_z - radius).abs() < 1.0);
        assert!((min_z + radius).abs() < 1.0);
        // Cap centre verts exist.
        assert!(
            v.iter()
                .any(|p| { p.pos[0].abs() < 1e-3 && p.pos[1].abs() < 1e-3 && p.pos[2] > 0.0 })
        );
    }

    #[test]
    fn scroll_sky_shifts_u() {
        let mut v = cylinder_sky(1024.0, 512.0, 4, 1.0);
        let u0 = v[0].uv[0];
        scroll_sky_u(&mut v, 0.25);
        assert!((v[0].uv[0] - (u0 + 0.25)).abs() < 1e-6);
    }

    #[test]
    fn cubemap_six_faces() {
        let faces = CubemapFaces {
            north: "N".into(),
            east: "E".into(),
            south: "S".into(),
            west: "W".into(),
            top: "T".into(),
            bottom: "B".into(),
        };
        let (v, names) = cubemap_sky(1024.0, &faces, 1.0);
        assert_eq!(v.len(), 36);
        assert_eq!(names[0], "N");
        assert_eq!(names[5], "B");
        assert!(faces.is_complete());
    }
}
