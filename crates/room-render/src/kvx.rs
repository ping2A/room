//! Build-engine KVX voxel → triangle mesh (M10 bootstrap).

use crate::models::{ModelError, ModelMesh, ModelVertex};

/// Parsed KVX dimensions and pivot (map units after scale).
#[derive(Clone, Debug, PartialEq)]
pub struct KvxHeader {
    /// Size in voxels (X).
    pub x_size: u32,
    /// Size in voxels (Y).
    pub y_size: u32,
    /// Size in voxels (Z).
    pub z_size: u32,
    /// Pivot (voxel space).
    pub pivot: [f32; 3],
}

/// One solid voxel with palette index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KvxVoxel {
    /// X index.
    pub x: u8,
    /// Y index.
    pub y: u8,
    /// Z index.
    pub z: u8,
    /// Colour index (0..=255).
    pub color: u8,
}

/// CPU KVX contents.
#[derive(Clone, Debug, Default)]
pub struct KvxModel {
    /// Header.
    pub header: Option<KvxHeader>,
    /// Solid voxels (mip 0).
    pub voxels: Vec<KvxVoxel>,
    /// Embedded 768-byte palette when present.
    pub palette: Option<[u8; 768]>,
}

/// Parse a KVX file (mip 0 only).
///
/// SLAB6 / GZDoom layout: each mip starts with `int32 numbytes` (the mip
/// payload, not including that dword). The payload is the Ken header
/// (`xsiz,ysiz,zsiz` + pivots + `xoffset` + `xyoffset` + slabs). `xoffset`
/// values are relative to the start of the x-offset table, and `xoffset[0]`
/// equals that table's size. The last 768 bytes of the file are the palette.
pub fn parse_kvx(data: &[u8]) -> Result<KvxModel, ModelError> {
    if data.len() <= 768 + 4 {
        return Err(ModelError::Message("KVX truncated header".into()));
    }
    let mut raw = 0usize;
    let mut maxmip = data.len() - 768 - 4;
    let mut mip0: Option<(KvxHeader, Vec<KvxVoxel>)> = None;
    let mut mips = 0u32;
    // GZDoom `MAXVOXMIPS`.
    for _ in 0..5 {
        if raw + 4 > data.len() {
            break;
        }
        let numbytes = read_i32(data, raw)?;
        if numbytes < 24 || numbytes as usize > maxmip {
            break;
        }
        let numbytes = numbytes as usize;
        let base = raw + 4;
        let header = read_mip(data, base, numbytes)?;
        if mips == 0 {
            mip0 = Some(header);
        }
        raw += 4 + numbytes;
        maxmip = maxmip.saturating_sub(numbytes + 4);
        mips += 1;
    }
    if mips == 0 || raw != data.len() - 768 {
        return Err(ModelError::Message("KVX bad mip chain".into()));
    }
    let (header, voxels) = mip0.ok_or_else(|| ModelError::Message("KVX empty".into()))?;
    let mut palette = [0u8; 768];
    palette.copy_from_slice(&data[data.len() - 768..]);
    Ok(KvxModel {
        header: Some(header),
        voxels,
        palette: Some(palette),
    })
}

fn read_mip(
    data: &[u8],
    base: usize,
    numbytes: usize,
) -> Result<(KvxHeader, Vec<KvxVoxel>), ModelError> {
    if base + 24 > data.len() {
        return Err(ModelError::Message("KVX truncated header".into()));
    }
    let x_size = read_i32(data, base)?;
    let y_size = read_i32(data, base + 4)?;
    let z_size = read_i32(data, base + 8)?;
    if x_size <= 0 || y_size <= 0 || z_size <= 0 || x_size > 256 || y_size > 256 || z_size > 256 {
        return Err(ModelError::Message("KVX bad dimensions".into()));
    }
    let xs = x_size as usize;
    let ys = y_size as usize;
    let zs = z_size as usize;
    let xpivot = read_i32(data, base + 12)? as f32 / 256.0;
    let ypivot = read_i32(data, base + 16)? as f32 / 256.0;
    let zpivot = read_i32(data, base + 20)? as f32 / 256.0;
    let offsetsize = (xs + 1) * 4 + xs * (ys + 1) * 2;
    if numbytes < 24 + offsetsize {
        return Err(ModelError::Message("KVX offsets truncated".into()));
    }
    let voxdatasize = numbytes - 24 - offsetsize;
    let xoff0 = read_i32(data, base + 24)?;
    if xoff0 != offsetsize as i32 {
        return Err(ModelError::Message("KVX bad xoffset".into()));
    }
    let xlast_at = base + 24 + xs * 4;
    if xlast_at + 4 > data.len() {
        return Err(ModelError::Message("KVX xoffset truncated".into()));
    }
    let xlast = read_i32(data, xlast_at)?;
    if xlast != (offsetsize + voxdatasize) as i32 {
        return Err(ModelError::Message("KVX bad xoffset".into()));
    }
    let mut xoffsets = Vec::with_capacity(xs + 1);
    for i in 0..=xs {
        let o = base + 24 + i * 4;
        xoffsets.push(read_i32(data, o)? as usize);
    }
    let yoff_base = base + 24 + (xs + 1) * 4;
    if yoff_base + xs * (ys + 1) * 2 > data.len() {
        return Err(ModelError::Message("KVX yoffset truncated".into()));
    }
    let mut voxels = Vec::new();
    for x in 0..xs {
        let x0 = xoffsets[x];
        let x1 = xoffsets[x + 1];
        if x1 < x0 {
            return Err(ModelError::Message("KVX bad xoffset".into()));
        }
        let yo = yoff_base + x * (ys + 1) * 2;
        for y in 0..ys {
            let ystart = usize::from(read_u16(data, yo + y * 2)?);
            let yend = usize::from(read_u16(data, yo + (y + 1) * 2)?);
            if yend < ystart || x0 + yend > xoffsets[xs] {
                return Err(ModelError::Message("KVX bad yoffset".into()));
            }
            // `xoffset` is relative to the x-offset table at `base + 24`.
            let mut p = base + 24 + x0 + ystart;
            let end = base + 24 + x0 + yend;
            if end > data.len() {
                return Err(ModelError::Message("KVX slab OOB".into()));
            }
            while p + 3 <= end {
                let ztop = data[p] as usize;
                let zleng = data[p + 1] as usize;
                p += 3;
                if ztop.saturating_add(zleng) > zs || p + zleng > end {
                    return Err(ModelError::Message("KVX slab colours truncated".into()));
                }
                for dz in 0..zleng {
                    voxels.push(KvxVoxel {
                        x: x as u8,
                        y: y as u8,
                        z: (ztop + dz) as u8,
                        color: data[p + dz],
                    });
                }
                p += zleng;
            }
        }
    }
    Ok((
        KvxHeader {
            x_size: x_size as u32,
            y_size: y_size as u32,
            z_size: z_size as u32,
            pivot: [xpivot, ypivot, zpivot],
        },
        voxels,
    ))
}

/// Emit a triangle mesh for solid voxels (greedy face merge).
///
/// Positions are centred on the KVX pivot; 1 voxel = 1 map unit. Adjacent
/// exposed faces of the same palette index are merged into larger quads.
///
/// Axes match GZDoom's voxel model in Room's Z-up world: KVX X stays X,
/// KVX Y becomes −Y, KVX Z becomes −Z (their Y-up mesh, then swapped).
#[must_use]
pub fn kvx_to_mesh(kvx: &KvxModel) -> ModelMesh {
    kvx_to_mesh_greedy(kvx)
}

/// Naive per-voxel cubes (no merge) — useful for debugging / comparisons.
#[must_use]
pub fn kvx_to_mesh_naive(kvx: &KvxModel) -> ModelMesh {
    let Some(h) = &kvx.header else {
        return ModelMesh::default();
    };
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for vox in &kvx.voxels {
        let [ox, oy, oz] = kvx_point_to_room(
            [f32::from(vox.x), f32::from(vox.y), f32::from(vox.z)],
            h.pivot,
        );
        let u = (f32::from(vox.color) + 0.5) / 256.0;
        push_cube(&mut vertices, &mut indices, ox, oy, oz, u);
    }
    ModelMesh { vertices, indices }
}

/// Greedy-meshed faces (same colour, exposed only).
#[must_use]
pub fn kvx_to_mesh_greedy(kvx: &KvxModel) -> ModelMesh {
    let Some(h) = &kvx.header else {
        return ModelMesh::default();
    };
    let xs = h.x_size as usize;
    let ys = h.y_size as usize;
    let zs = h.z_size as usize;
    if xs == 0 || ys == 0 || zs == 0 {
        return ModelMesh::default();
    }
    // Occupancy: 0 = empty, else colour+1 (so colour 0 is valid).
    let mut grid = vec![0u16; xs * ys * zs];
    let idx = |x: usize, y: usize, z: usize| -> usize { (x * ys + y) * zs + z };
    for vox in &kvx.voxels {
        let x = vox.x as usize;
        let y = vox.y as usize;
        let z = vox.z as usize;
        if x < xs && y < ys && z < zs {
            grid[idx(x, y, z)] = u16::from(vox.color) + 1;
        }
    }
    let solid = |x: i32, y: i32, z: i32| -> u16 {
        if x < 0 || y < 0 || z < 0 {
            return 0;
        }
        let (xu, yu, zu) = (x as usize, y as usize, z as usize);
        if xu >= xs || yu >= ys || zu >= zs {
            return 0;
        }
        grid[idx(xu, yu, zu)]
    };

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let px = h.pivot[0];
    let py = h.pivot[1];
    let pz = h.pivot[2];

    // Six axis-aligned face directions: (axis, sign) where axis 0=X,1=Y,2=Z.
    for (axis, sign) in [(0i32, -1i32), (0, 1), (1, -1), (1, 1), (2, -1), (2, 1)] {
        let (u_axis, v_axis) = match axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let dims = [xs as i32, ys as i32, zs as i32];
        let u_dim = dims[u_axis as usize];
        let v_dim = dims[v_axis as usize];
        let slice_dim = dims[axis as usize];

        for slice in 0..slice_dim {
            let mut mask = vec![0u16; (u_dim * v_dim) as usize];
            for v in 0..v_dim {
                for u in 0..u_dim {
                    let mut c = [0i32; 3];
                    c[u_axis as usize] = u;
                    c[v_axis as usize] = v;
                    c[axis as usize] = slice;
                    let here = solid(c[0], c[1], c[2]);
                    if here == 0 {
                        continue;
                    }
                    let mut n = c;
                    n[axis as usize] += sign;
                    if solid(n[0], n[1], n[2]) != 0 {
                        continue; // occluded
                    }
                    mask[(v * u_dim + u) as usize] = here;
                }
            }
            // Greedy merge on this slice mask.
            let mut visited = vec![false; mask.len()];
            for v0 in 0..v_dim {
                for u0 in 0..u_dim {
                    let i0 = (v0 * u_dim + u0) as usize;
                    if visited[i0] || mask[i0] == 0 {
                        continue;
                    }
                    let color = mask[i0];
                    let mut w = 1i32;
                    while u0 + w < u_dim {
                        let i = (v0 * u_dim + u0 + w) as usize;
                        if visited[i] || mask[i] != color {
                            break;
                        }
                        w += 1;
                    }
                    let mut hgt = 1i32;
                    'grow: while v0 + hgt < v_dim {
                        for du in 0..w {
                            let i = ((v0 + hgt) * u_dim + u0 + du) as usize;
                            if visited[i] || mask[i] != color {
                                break 'grow;
                            }
                        }
                        hgt += 1;
                    }
                    for dv in 0..hgt {
                        for du in 0..w {
                            visited[((v0 + dv) * u_dim + u0 + du) as usize] = true;
                        }
                    }
                    let u_col = (color - 1) as u8;
                    let tex_u = (f32::from(u_col) + 0.5) / 256.0;
                    push_greedy_quad(
                        &mut vertices,
                        &mut indices,
                        axis,
                        sign,
                        slice,
                        u0,
                        v0,
                        w,
                        hgt,
                        u_axis,
                        v_axis,
                        px,
                        py,
                        pz,
                        tex_u,
                    );
                }
            }
        }
    }
    ModelMesh { vertices, indices }
}

fn push_greedy_quad(
    vertices: &mut Vec<ModelVertex>,
    indices: &mut Vec<u32>,
    axis: i32,
    sign: i32,
    slice: i32,
    u0: i32,
    v0: i32,
    w: i32,
    h: i32,
    u_axis: i32,
    v_axis: i32,
    px: f32,
    py: f32,
    pz: f32,
    tex_u: f32,
) {
    // Face sits on the outer boundary of the voxel in `sign` direction.
    let face = if sign > 0 { slice + 1 } else { slice };
    let mut corners = [[0.0f32; 3]; 4];
    let u1 = u0 + w;
    let v1 = v0 + h;
    let uv = [(u0, v0), (u1, v0), (u1, v1), (u0, v1)];
    for (i, &(uu, vv)) in uv.iter().enumerate() {
        let mut p = [0.0f32; 3];
        p[u_axis as usize] = uu as f32;
        p[v_axis as usize] = vv as f32;
        p[axis as usize] = face as f32;
        corners[i] = kvx_point_to_room(p, [px, py, pz]);
    }
    // Winding: outward normal along `sign` on `axis`. Two axis flips
    // (Y and Z) keep the triangle orientation.
    let order: [usize; 6] = if sign > 0 {
        [0, 1, 2, 0, 2, 3]
    } else {
        [0, 3, 2, 0, 2, 1]
    };
    let mut kn = [0.0f32; 3];
    kn[axis as usize] = sign as f32;
    let normal = [kn[0], -kn[1], -kn[2]];
    let base = vertices.len() as u32;
    for c in &corners {
        vertices.push(ModelVertex {
            pos: *c,
            uv: [tex_u, 0.5],
            normal,
        });
    }
    for &i in &order {
        indices.push(base + i as u32);
    }
}

/// KVX voxel corner → Room map space (X east, Y north, Z up).
fn read_i32(data: &[u8], at: usize) -> Result<i32, ModelError> {
    let s = data
        .get(at..at + 4)
        .ok_or_else(|| ModelError::Message("KVX truncated".into()))?;
    let mut b = [0u8; 4];
    b.copy_from_slice(s);
    Ok(i32::from_le_bytes(b))
}

fn read_u16(data: &[u8], at: usize) -> Result<u16, ModelError> {
    let s = data
        .get(at..at + 2)
        .ok_or_else(|| ModelError::Message("KVX truncated".into()))?;
    let mut b = [0u8; 2];
    b.copy_from_slice(s);
    Ok(u16::from_le_bytes(b))
}

fn kvx_point_to_room(p: [f32; 3], pivot: [f32; 3]) -> [f32; 3] {
    let kx = p[0] - pivot[0];
    let ky = p[1] - pivot[1];
    let kz = p[2] - pivot[2];
    [kx, -ky, -kz]
}

fn push_cube(
    vertices: &mut Vec<ModelVertex>,
    indices: &mut Vec<u32>,
    x: f32,
    y: f32,
    z: f32,
    u: f32,
) {
    let base = vertices.len() as u32;
    // 8 corners of unit cube [0,1]^3
    let corners = [
        [x, y, z],
        [x + 1.0, y, z],
        [x + 1.0, y + 1.0, z],
        [x, y + 1.0, z],
        [x, y, z + 1.0],
        [x + 1.0, y, z + 1.0],
        [x + 1.0, y + 1.0, z + 1.0],
        [x, y + 1.0, z + 1.0],
    ];
    for c in &corners {
        vertices.push(ModelVertex {
            pos: *c,
            uv: [u, 0.5],
            normal: [0.0, 0.0, 1.0],
        });
    }
    // 6 faces, 2 tris each (CCW outward-ish)
    const FACES: [[u32; 6]; 6] = [
        [0, 1, 2, 0, 2, 3], // -Z
        [4, 6, 5, 4, 7, 6], // +Z
        [0, 4, 5, 0, 5, 1], // -Y
        [2, 6, 7, 2, 7, 3], // +Y
        [0, 3, 7, 0, 7, 4], // -X
        [1, 5, 6, 1, 6, 2], // +X
    ];
    for face in &FACES {
        for &i in face {
            indices.push(base + i);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn empty_kvx_rejected() {
        assert!(parse_kvx(&[0u8; 8]).is_err());
    }

    #[test]
    fn slab6_length_prefix_one_voxel() {
        // 1×1×1, colour 4 at z = 0, pivot 0.5, plus a 768-byte palette.
        let xs = 1i32;
        let ys = 1i32;
        let zs = 1i32;
        let offsetsize = (xs + 1) * 4 + xs * (ys + 1) * 2;
        let slab = [0u8, 1, 0, 4];
        let numbytes = 24 + offsetsize + slab.len() as i32;
        let mut data = Vec::new();
        data.extend_from_slice(&numbytes.to_le_bytes());
        data.extend_from_slice(&xs.to_le_bytes());
        data.extend_from_slice(&ys.to_le_bytes());
        data.extend_from_slice(&zs.to_le_bytes());
        data.extend_from_slice(&128i32.to_le_bytes());
        data.extend_from_slice(&128i32.to_le_bytes());
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&offsetsize.to_le_bytes());
        data.extend_from_slice(&(offsetsize + slab.len() as i32).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&(slab.len() as u16).to_le_bytes());
        data.extend_from_slice(&slab);
        data.extend_from_slice(&[1u8; 768]);
        let kvx = parse_kvx(&data).unwrap();
        assert_eq!(kvx.voxels.len(), 1);
        assert_eq!(kvx.voxels[0].color, 4);
        assert!((kvx.header.as_ref().unwrap().pivot[0] - 0.5).abs() < 1e-4);
        let mesh = kvx_to_mesh(&kvx);
        assert_eq!(mesh.indices.len(), 36);
        // Room Z is up: the voxel sits on the negative Z side of a 0 pivot.
        let max_z = mesh
            .vertices
            .iter()
            .map(|v| v.pos[2])
            .fold(f32::MIN, f32::max);
        assert!(max_z <= 0.01, "{max_z}");
    }

    #[test]
    fn cube_mesh_has_tris() {
        let kvx = KvxModel {
            header: Some(KvxHeader {
                x_size: 1,
                y_size: 1,
                z_size: 1,
                pivot: [0.5, 0.5, 0.5],
            }),
            voxels: vec![KvxVoxel {
                x: 0,
                y: 0,
                z: 0,
                color: 16,
            }],
            palette: None,
        };
        let naive = kvx_to_mesh_naive(&kvx);
        assert_eq!(naive.indices.len(), 36); // 6 faces × 2 tris × 3
        assert_eq!(naive.vertices.len(), 8);
        let greedy = kvx_to_mesh(&kvx);
        // Single voxel: 6 quads × 2 tris × 3 = 36 indices, 4 verts/face.
        assert_eq!(greedy.indices.len(), 36);
        assert_eq!(greedy.vertices.len(), 24);
    }

    #[test]
    fn greedy_merges_2x2x1_slab() {
        let mut voxels = Vec::new();
        for x in 0..2u8 {
            for y in 0..2u8 {
                voxels.push(KvxVoxel {
                    x,
                    y,
                    z: 0,
                    color: 7,
                });
            }
        }
        let kvx = KvxModel {
            header: Some(KvxHeader {
                x_size: 2,
                y_size: 2,
                z_size: 1,
                pivot: [1.0, 1.0, 0.5],
            }),
            voxels,
            palette: None,
        };
        let naive = kvx_to_mesh_naive(&kvx);
        let greedy = kvx_to_mesh_greedy(&kvx);
        // Naive: 4 cubes × 36 = 144 indices.
        assert_eq!(naive.indices.len(), 144);
        // Greedy should emit far fewer (merged top/bottom + side strips).
        assert!(
            greedy.indices.len() < naive.indices.len(),
            "greedy {} vs naive {}",
            greedy.indices.len(),
            naive.indices.len()
        );
        // Top + bottom of 2×2 slab → 2 quads; sides → 4 quads of width 2 → 6 quads = 36 idx.
        assert_eq!(greedy.indices.len(), 36);
    }
}
