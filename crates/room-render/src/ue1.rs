//! Unreal Engine 1 vertex mesh (`*_d.3d` + `*_a.3d`) bootstrap (M10).

// Binary headers are read as fixed-width slices of a length-checked buffer, so the
// `try_into()` on each slice cannot fail.
#![allow(clippy::unwrap_used)]

use crate::models::{ModelError, ModelMesh, ModelVertex};
use glam::Vec3;

/// Parsed UE1 model (all animation frames).
#[derive(Clone, Debug, Default)]
pub struct Ue1Model {
    /// Vertex count (shared topology).
    pub num_vertices: u32,
    /// Triangle records from `_d.3d`.
    pub tris: Vec<Ue1Tri>,
    /// One [`ModelMesh`] per frame (expanded verts).
    pub frames: Vec<ModelMesh>,
}

/// One triangle from the data file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ue1Tri {
    /// Vertex indices into the aniv frame.
    pub verts: [u16; 3],
    /// UE1 mesh type byte.
    pub kind: u8,
    /// Texture slot.
    pub tex: u8,
    /// Per-corner UVs on a 0..=255 grid (`[vert][uv]`).
    pub uv: [[u8; 2]; 3],
}

impl Ue1Model {
    /// Mesh for `frame`, if present.
    #[must_use]
    pub fn frame_mesh(&self, frame: usize) -> Option<&ModelMesh> {
        self.frames.get(frame)
    }
}

/// Decode a packed UE1 aniv vertex (`X:11 Y:11 Z:10` signed bitfields).
#[must_use]
pub fn decompress_ue1_vertex(packed: u32) -> [f32; 3] {
    let mut x = (packed & 0x7ff) as i32;
    if x >= 0x400 {
        x -= 0x800;
    }
    let mut y = ((packed >> 11) & 0x7ff) as i32;
    if y >= 0x400 {
        y -= 0x800;
    }
    let mut z = ((packed >> 22) & 0x3ff) as i32;
    if z >= 0x200 {
        z -= 0x400;
    }
    [x as f32, y as f32, z as f32]
}

/// Parse paired `_d.3d` (data) + `_a.3d` (aniv) buffers into meshes.
pub fn parse_ue1(data_d: &[u8], data_a: &[u8]) -> Result<Ue1Model, ModelError> {
    if data_d.len() < 48 {
        return Err(ModelError::Message("UE1 _d.3d truncated header".into()));
    }
    let num_tris = u16::from_le_bytes(data_d[0..2].try_into().unwrap()) as usize;
    let num_verts = u16::from_le_bytes(data_d[2..4].try_into().unwrap()) as usize;
    if num_tris == 0 || num_verts < 3 {
        return Err(ModelError::Message("UE1 bad poly/vert count".into()));
    }
    // Skip 44-byte unused header tail after counts.
    let poly_base = 48usize;
    let poly_bytes = num_tris
        .checked_mul(16)
        .ok_or_else(|| ModelError::Message("UE1 poly table overflow".into()))?;
    if data_d.len() < poly_base + poly_bytes {
        return Err(ModelError::Message("UE1 _d.3d truncated polys".into()));
    }
    let mut tris = Vec::with_capacity(num_tris);
    for i in 0..num_tris {
        let o = poly_base + i * 16;
        let v0 = u16::from_le_bytes(data_d[o..o + 2].try_into().unwrap());
        let v1 = u16::from_le_bytes(data_d[o + 2..o + 4].try_into().unwrap());
        let v2 = u16::from_le_bytes(data_d[o + 4..o + 6].try_into().unwrap());
        let kind = data_d[o + 6];
        // o+7 = color (ignored)
        let uv = [
            [data_d[o + 8], data_d[o + 9]],
            [data_d[o + 10], data_d[o + 11]],
            [data_d[o + 12], data_d[o + 13]],
        ];
        let tex = data_d[o + 14];
        // o+15 = flags (ignored)
        for &vi in &[v0, v1, v2] {
            if vi as usize >= num_verts {
                return Err(ModelError::Message("UE1 vertex index OOB".into()));
            }
        }
        // Skip weapon placeholder (type 8) — invisible in GZDoom.
        if kind == 8 {
            continue;
        }
        tris.push(Ue1Tri {
            verts: [v0, v1, v2],
            kind,
            tex,
            uv,
        });
    }
    if tris.is_empty() {
        return Err(ModelError::Message("UE1 no draw triangles".into()));
    }

    if data_a.len() < 4 {
        return Err(ModelError::Message("UE1 _a.3d truncated header".into()));
    }
    let num_frames = u16::from_le_bytes(data_a[0..2].try_into().unwrap()) as usize;
    let frame_size = u16::from_le_bytes(data_a[2..4].try_into().unwrap()) as usize;
    if num_frames == 0 {
        return Err(ModelError::Message("UE1 no frames".into()));
    }
    if frame_size != num_verts * 4 {
        return Err(ModelError::Message(format!(
            "UE1 aniv frame_size {frame_size} != num_verts*4 ({})",
            num_verts * 4
        )));
    }
    let need = 4usize
        .checked_add(
            num_frames
                .checked_mul(frame_size)
                .ok_or_else(|| ModelError::Message("UE1 aniv overflow".into()))?,
        )
        .ok_or_else(|| ModelError::Message("UE1 aniv overflow".into()))?;
    if data_a.len() < need {
        return Err(ModelError::Message("UE1 _a.3d truncated frames".into()));
    }

    let mut frames = Vec::with_capacity(num_frames);
    for f in 0..num_frames {
        let base = 4 + f * frame_size;
        let mut positions = Vec::with_capacity(num_verts);
        for vi in 0..num_verts {
            let o = base + vi * 4;
            let packed = u32::from_le_bytes(data_a[o..o + 4].try_into().unwrap());
            positions.push(decompress_ue1_vertex(packed));
        }
        let mut vertices = Vec::with_capacity(tris.len() * 3);
        let mut indices = Vec::with_capacity(tris.len() * 3);
        for tri in &tris {
            let base_i = vertices.len() as u32;
            for c in 0..3 {
                let vi = tri.verts[c] as usize;
                let pos = positions[vi];
                let u = f32::from(tri.uv[c][0]) / 255.0;
                let v = 1.0 - f32::from(tri.uv[c][1]) / 255.0;
                vertices.push(ModelVertex {
                    pos,
                    uv: [u, v],
                    normal: [0.0, 0.0, 1.0],
                });
                indices.push(base_i + c as u32);
            }
            // Flat normal from triangle.
            let pa = Vec3::from_array(vertices[base_i as usize].pos);
            let pb = Vec3::from_array(vertices[base_i as usize + 1].pos);
            let pc = Vec3::from_array(vertices[base_i as usize + 2].pos);
            let n = (pb - pa).cross(pc - pa).normalize_or_zero();
            let n = if n.length_squared() < 1e-8 {
                [0.0, 0.0, 1.0]
            } else {
                n.to_array()
            };
            for k in 0..3 {
                vertices[base_i as usize + k].normal = n;
            }
        }
        frames.push(ModelMesh { vertices, indices });
    }

    Ok(Ue1Model {
        num_vertices: num_verts as u32,
        tris,
        frames,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn pack_vertex(x: i32, y: i32, z: i32) -> u32 {
        let xu = (x as u32) & 0x7ff;
        let yu = (y as u32) & 0x7ff;
        let zu = (z as u32) & 0x3ff;
        xu | (yu << 11) | (zu << 22)
    }

    fn make_fixture() -> (Vec<u8>, Vec<u8>) {
        // 1 triangle, 3 verts, 2 frames.
        let mut d = vec![0u8; 48];
        d[0..2].copy_from_slice(&1u16.to_le_bytes()); // tris
        d[2..4].copy_from_slice(&3u16.to_le_bytes()); // verts
        let mut poly = vec![0u8; 16];
        poly[0..2].copy_from_slice(&0u16.to_le_bytes());
        poly[2..4].copy_from_slice(&1u16.to_le_bytes());
        poly[4..6].copy_from_slice(&2u16.to_le_bytes());
        poly[6] = 0; // type
        poly[8] = 0;
        poly[9] = 0; // uv0
        poly[10] = 255;
        poly[11] = 0; // uv1
        poly[12] = 128;
        poly[13] = 255; // uv2
        poly[14] = 0; // tex
        d.extend_from_slice(&poly);

        let mut a = Vec::new();
        a.extend_from_slice(&2u16.to_le_bytes()); // frames
        a.extend_from_slice(&(3u16 * 4).to_le_bytes()); // frame size
        for frame in 0..2u32 {
            let zoff = frame as i32 * 10;
            a.extend_from_slice(&pack_vertex(0, 0, zoff).to_le_bytes());
            a.extend_from_slice(&pack_vertex(10, 0, zoff).to_le_bytes());
            a.extend_from_slice(&pack_vertex(0, 10, zoff).to_le_bytes());
        }
        (d, a)
    }

    #[test]
    fn decompress_roundtrip_axes() {
        let p = decompress_ue1_vertex(pack_vertex(5, -3, 7));
        assert!((p[0] - 5.0).abs() < 1e-5);
        assert!((p[1] + 3.0).abs() < 1e-5);
        assert!((p[2] - 7.0).abs() < 1e-5);
    }

    #[test]
    fn parse_ue1_two_frames() {
        let (d, a) = make_fixture();
        let m = parse_ue1(&d, &a).unwrap();
        assert_eq!(m.num_vertices, 3);
        assert_eq!(m.frames.len(), 2);
        assert_eq!(m.frames[0].indices.len(), 3);
        assert!((m.frames[0].vertices[0].pos[2] - 0.0).abs() < 1e-5);
        assert!((m.frames[1].vertices[0].pos[2] - 10.0).abs() < 1e-5);
        assert!((m.frames[0].vertices[1].uv[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn rejects_truncated() {
        assert!(parse_ue1(&[0u8; 8], &[0u8; 8]).is_err());
    }
}
