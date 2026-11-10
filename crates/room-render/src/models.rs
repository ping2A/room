//! MODELDEF mesh load bootstrap (OBJ / MD2 / MD3 / IQM).

use crate::mesh::WallVertex;
use crate::scene::{MaterialRange, SceneLists};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec2, Vec3};
use thiserror::Error;

const MD2_IDENT: i32 = 0x3250_4449; // "IDP2" little-endian
const MD2_VERSION: i32 = 8;
const MD3_IDENT: i32 = 0x3350_4449; // "IDP3" little-endian
const MD3_VERSION: i32 = 15;
const MD3_XYZ_SCALE: f32 = 1.0 / 64.0;
const IQM_MAGIC: &[u8; 16] = b"INTERQUAKEMODEL\0";
const IQM_VERSION: u32 = 2;
/// IQM vertex array type: position.
const IQM_POSITION: u32 = 0;
/// IQM vertex array type:texcoord.
const IQM_TEXCOORD: u32 = 1;
/// IQM vertex array type: normal.
const IQM_NORMAL: u32 = 2;
/// IQM blend bone indexes (ubyte×4).
const IQM_BLENDINDEXES: u32 = 4;
/// IQM blend weights (ubyte×4).
const IQM_BLENDWEIGHTS: u32 = 5;
/// IQM vertex format: unsigned byte.
const IQM_UBYTE: u32 = 1;
/// IQM vertex format: float.
const IQM_FLOAT: u32 = 7;

/// Mesh load / parse errors.
#[derive(Debug, Error)]
pub enum ModelError {
    /// Bad or unsupported data.
    #[error("{0}")]
    Message(String),
}

/// Interleaved model vertex for GPU upload.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ModelVertex {
    /// Position.
    pub pos: [f32; 3],
    /// UV.
    pub uv: [f32; 2],
    /// Normal.
    pub normal: [f32; 3],
}

/// CPU mesh (one surface).
#[derive(Clone, Debug, Default)]
pub struct ModelMesh {
    /// Vertices.
    pub vertices: Vec<ModelVertex>,
    /// Triangle indices.
    pub indices: Vec<u32>,
}

impl ModelMesh {
    /// Axis-aligned bounds (empty mesh → zero).
    #[must_use]
    pub fn bounds(&self) -> (Vec3, Vec3) {
        if self.vertices.is_empty() {
            return (Vec3::ZERO, Vec3::ZERO);
        }
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in &self.vertices {
            let p = Vec3::from_array(v.pos);
            min = min.min(p);
            max = max.max(p);
        }
        (min, max)
    }

    /// Expand indexed tris to unlit [`WallVertex`] list (GPU draw via existing pass).
    #[must_use]
    pub fn to_wall_vertices(&self, origin: Vec3, scale: Vec3, light: f32) -> Vec<WallVertex> {
        self.to_wall_vertices_transformed(
            Mat4::from_translation(origin) * Mat4::from_scale(scale),
            light,
        )
    }

    /// Same as [`Self::to_wall_vertices`] with a full model-to-world matrix.
    #[must_use]
    pub fn to_wall_vertices_transformed(&self, model: Mat4, light: f32) -> Vec<WallVertex> {
        let light = light.clamp(0.0, 1.0);
        let mut out = Vec::with_capacity(self.indices.len());
        for &i in &self.indices {
            let Some(v) = self.vertices.get(i as usize) else {
                continue;
            };
            let p = model.transform_point3(Vec3::from_array(v.pos));
            out.push(WallVertex::new(p, Vec2::from_array(v.uv), light));
        }
        out
    }
}

/// Append a model mesh into the opaque wall lists (unlit pipeline bootstrap).
pub fn append_model(
    lists: &mut SceneLists,
    mesh: &ModelMesh,
    origin: Vec3,
    scale: Vec3,
    light: f32,
    texture: &str,
) {
    let verts = mesh.to_wall_vertices(origin, scale, light);
    if verts.is_empty() {
        return;
    }
    #[allow(clippy::cast_possible_truncation)]
    let start = lists.opaque_walls.len() as u32;
    #[allow(clippy::cast_possible_truncation)]
    let count = verts.len() as u32;
    lists.opaque_walls.extend_from_slice(&verts);
    lists.wall_ranges.push(MaterialRange {
        name: texture.to_string(),
        start,
        count,
        flags: 0,
    });
}

/// Parse a Wavefront OBJ (triangles / quads → tris). Ignores materials.
pub fn parse_obj(text: &str) -> Result<ModelMesh, ModelError> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut texcoords: Vec<[f32; 2]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut vertices: Vec<ModelVertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    // Deduplicate (v/vt/vn) triples within this mesh.
    let mut key_to_index: std::collections::BTreeMap<(i32, i32, i32), u32> =
        std::collections::BTreeMap::new();

    for (line_no, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(tag) = parts.next() else {
            continue;
        };
        match tag {
            "v" => {
                let x: f32 = parse_f(parts.next(), line_no, "v.x")?;
                let y: f32 = parse_f(parts.next(), line_no, "v.y")?;
                let z: f32 = parse_f(parts.next(), line_no, "v.z")?;
                positions.push([x, y, z]);
            }
            "vt" => {
                let u: f32 = parse_f(parts.next(), line_no, "vt.u")?;
                let v: f32 = match parts.next() {
                    Some(t) => t.parse::<f32>().map_err(|_| {
                        ModelError::Message(format!("OBJ line {}: bad float `{t}`", line_no + 1))
                    })?,
                    None => 0.0,
                };
                texcoords.push([u, v]);
            }
            "vn" => {
                let x: f32 = parse_f(parts.next(), line_no, "vn.x")?;
                let y: f32 = parse_f(parts.next(), line_no, "vn.y")?;
                let z: f32 = parse_f(parts.next(), line_no, "vn.z")?;
                normals.push([x, y, z]);
            }
            "f" => {
                let mut corners: Vec<(i32, i32, i32)> = Vec::new();
                for tok in parts {
                    corners.push(parse_face_vert(tok, line_no)?);
                }
                if corners.len() < 3 {
                    return Err(ModelError::Message(format!(
                        "OBJ line {}: face needs ≥3 verts",
                        line_no + 1
                    )));
                }
                // Fan triangulation.
                for i in 1..corners.len() - 1 {
                    for &c in &[corners[0], corners[i], corners[i + 1]] {
                        let idx = *key_to_index.entry(c).or_insert_with(|| {
                            let pos = resolve_index(&positions, c.0);
                            let uv = if c.1 != 0 {
                                resolve_index(&texcoords, c.1)
                            } else {
                                [0.0, 0.0]
                            };
                            let normal = if c.2 != 0 {
                                resolve_index(&normals, c.2)
                            } else {
                                [0.0, 0.0, 1.0]
                            };
                            let i = vertices.len() as u32;
                            vertices.push(ModelVertex { pos, uv, normal });
                            i
                        });
                        indices.push(idx);
                    }
                }
            }
            _ => {}
        }
    }

    if indices.is_empty() {
        return Err(ModelError::Message("OBJ has no faces".into()));
    }
    Ok(ModelMesh { vertices, indices })
}

/// MD2 frame mesh (Quake II / GZDoom MODELDEF).
#[derive(Clone, Debug)]
pub struct Md2Model {
    /// Skin width (texels).
    pub skin_width: u32,
    /// Skin height (texels).
    pub skin_height: u32,
    /// Frame names (16-byte ASCII, trimmed).
    pub frame_names: Vec<String>,
    /// One [`ModelMesh`] per frame (same topology, different positions).
    pub frames: Vec<ModelMesh>,
}

impl Md2Model {
    /// Mesh for `frame`, if present.
    #[must_use]
    pub fn frame_mesh(&self, frame: usize) -> Option<&ModelMesh> {
        self.frames.get(frame)
    }
}

/// Parse an MD2 binary (`IDP2` / version 8). Emits all frames.
pub fn parse_md2(data: &[u8]) -> Result<Md2Model, ModelError> {
    if data.len() < 68 {
        return Err(ModelError::Message("MD2 truncated header".into()));
    }
    let ident = i32::from_le_bytes(data[0..4].try_into().unwrap());
    let version = i32::from_le_bytes(data[4..8].try_into().unwrap());
    if ident != MD2_IDENT || version != MD2_VERSION {
        return Err(ModelError::Message(format!(
            "MD2 bad ident/version ({ident:#x}/{version})"
        )));
    }
    let skin_width = i32::from_le_bytes(data[8..12].try_into().unwrap());
    let skin_height = i32::from_le_bytes(data[12..16].try_into().unwrap());
    let _framesize = i32::from_le_bytes(data[16..20].try_into().unwrap());
    let _num_skins = i32::from_le_bytes(data[20..24].try_into().unwrap());
    let num_xyz = i32::from_le_bytes(data[24..28].try_into().unwrap());
    let num_st = i32::from_le_bytes(data[28..32].try_into().unwrap());
    let num_tris = i32::from_le_bytes(data[32..36].try_into().unwrap());
    let _num_glcmds = i32::from_le_bytes(data[36..40].try_into().unwrap());
    let num_frames = i32::from_le_bytes(data[40..44].try_into().unwrap());
    let _ofs_skins = i32::from_le_bytes(data[44..48].try_into().unwrap());
    let ofs_st = i32::from_le_bytes(data[48..52].try_into().unwrap()) as usize;
    let ofs_tris = i32::from_le_bytes(data[52..56].try_into().unwrap()) as usize;
    let ofs_frames = i32::from_le_bytes(data[56..60].try_into().unwrap()) as usize;
    if skin_width <= 0
        || skin_height <= 0
        || num_xyz <= 0
        || num_st <= 0
        || num_tris <= 0
        || num_frames <= 0
    {
        return Err(ModelError::Message("MD2 invalid counts".into()));
    }
    let num_xyz = num_xyz as usize;
    let num_st = num_st as usize;
    let num_tris = num_tris as usize;
    let num_frames = num_frames as usize;
    let sw = skin_width as f32;
    let sh = skin_height as f32;

    // ST
    let st_end = ofs_st
        .checked_add(num_st * 4)
        .ok_or_else(|| ModelError::Message("MD2 ST overflow".into()))?;
    if st_end > data.len() {
        return Err(ModelError::Message("MD2 ST truncated".into()));
    }
    let mut sts = Vec::with_capacity(num_st);
    for i in 0..num_st {
        let o = ofs_st + i * 4;
        let s = i16::from_le_bytes(data[o..o + 2].try_into().unwrap()) as f32 / sw;
        let t = i16::from_le_bytes(data[o + 2..o + 4].try_into().unwrap()) as f32 / sh;
        sts.push([s, t]);
    }

    // Tris
    let tri_end = ofs_tris
        .checked_add(num_tris * 12)
        .ok_or_else(|| ModelError::Message("MD2 tris overflow".into()))?;
    if tri_end > data.len() {
        return Err(ModelError::Message("MD2 tris truncated".into()));
    }
    let mut tris: Vec<([u16; 3], [u16; 3])> = Vec::with_capacity(num_tris);
    for i in 0..num_tris {
        let o = ofs_tris + i * 12;
        let mut v = [0u16; 3];
        let mut st = [0u16; 3];
        for k in 0..3 {
            v[k] = u16::from_le_bytes(data[o + k * 2..o + k * 2 + 2].try_into().unwrap());
            st[k] = u16::from_le_bytes(data[o + 6 + k * 2..o + 6 + k * 2 + 2].try_into().unwrap());
        }
        tris.push((v, st));
    }

    let frame_stride = 40 + num_xyz * 4;
    let frames_end = ofs_frames
        .checked_add(num_frames * frame_stride)
        .ok_or_else(|| ModelError::Message("MD2 frames overflow".into()))?;
    if frames_end > data.len() {
        return Err(ModelError::Message("MD2 frames truncated".into()));
    }

    let mut frame_names = Vec::with_capacity(num_frames);
    let mut frames = Vec::with_capacity(num_frames);
    for fi in 0..num_frames {
        let base = ofs_frames + fi * frame_stride;
        let scale = [
            f32::from_le_bytes(data[base..base + 4].try_into().unwrap()),
            f32::from_le_bytes(data[base + 4..base + 8].try_into().unwrap()),
            f32::from_le_bytes(data[base + 8..base + 12].try_into().unwrap()),
        ];
        let translate = [
            f32::from_le_bytes(data[base + 12..base + 16].try_into().unwrap()),
            f32::from_le_bytes(data[base + 16..base + 20].try_into().unwrap()),
            f32::from_le_bytes(data[base + 20..base + 24].try_into().unwrap()),
        ];
        let name_bytes = &data[base + 24..base + 40];
        let name_end = name_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(name_bytes.len());
        frame_names.push(String::from_utf8_lossy(&name_bytes[..name_end]).into_owned());

        let mut positions = Vec::with_capacity(num_xyz);
        for vi in 0..num_xyz {
            let o = base + 40 + vi * 4;
            let vx = data[o] as f32 * scale[0] + translate[0];
            let vy = data[o + 1] as f32 * scale[1] + translate[1];
            let vz = data[o + 2] as f32 * scale[2] + translate[2];
            positions.push([vx, vy, vz]);
        }

        // Expand indexed tris to unique (xyz,st) vertices (MD2 can share xyz with different ST).
        let mut vertices = Vec::with_capacity(num_tris * 3);
        let mut indices = Vec::with_capacity(num_tris * 3);
        let mut key_map: std::collections::BTreeMap<(u16, u16), u32> =
            std::collections::BTreeMap::new();
        for &(vs, sts_i) in &tris {
            for k in 0..3 {
                let key = (vs[k], sts_i[k]);
                let idx = *key_map.entry(key).or_insert_with(|| {
                    let pos = positions
                        .get(vs[k] as usize)
                        .copied()
                        .unwrap_or([0.0, 0.0, 0.0]);
                    let uv = sts.get(sts_i[k] as usize).copied().unwrap_or([0.0, 0.0]);
                    let i = vertices.len() as u32;
                    vertices.push(ModelVertex {
                        pos,
                        uv,
                        normal: [0.0, 0.0, 1.0],
                    });
                    i
                });
                indices.push(idx);
            }
        }
        frames.push(ModelMesh { vertices, indices });
    }

    Ok(Md2Model {
        skin_width: skin_width as u32,
        skin_height: skin_height as u32,
        frame_names,
        frames,
    })
}

/// MD3 model (Quake III / GZDoom) — first surface, all frames.
#[derive(Clone, Debug)]
pub struct Md3Model {
    /// Surface name.
    pub surface_name: String,
    /// Frame names from the header frame list.
    pub frame_names: Vec<String>,
    /// One mesh per animation frame (shared topology).
    pub frames: Vec<ModelMesh>,
}

impl Md3Model {
    /// Mesh for `frame`, if present.
    #[must_use]
    pub fn frame_mesh(&self, frame: usize) -> Option<&ModelMesh> {
        self.frames.get(frame)
    }
}

/// Parse an MD3 binary (`IDP3` / version 15). Uses the first surface only.
pub fn parse_md3(data: &[u8]) -> Result<Md3Model, ModelError> {
    if data.len() < 108 {
        return Err(ModelError::Message("MD3 truncated header".into()));
    }
    let ident = i32::from_le_bytes(data[0..4].try_into().unwrap());
    let version = i32::from_le_bytes(data[4..8].try_into().unwrap());
    if ident != MD3_IDENT || version != MD3_VERSION {
        return Err(ModelError::Message(format!(
            "MD3 bad ident/version ({ident:#x}/{version})"
        )));
    }
    let num_frames = i32::from_le_bytes(data[72..76].try_into().unwrap());
    let _num_tags = i32::from_le_bytes(data[76..80].try_into().unwrap());
    let num_surfaces = i32::from_le_bytes(data[80..84].try_into().unwrap());
    let ofs_frames = i32::from_le_bytes(data[88..92].try_into().unwrap()) as usize;
    let ofs_surfaces = i32::from_le_bytes(data[96..100].try_into().unwrap()) as usize;
    if num_frames <= 0 || num_surfaces <= 0 {
        return Err(ModelError::Message("MD3 empty frames/surfaces".into()));
    }
    let num_frames = num_frames as usize;

    // Frame names (56-byte md3Frame_t: bounds + origin + radius + name[16])
    let mut frame_names = Vec::with_capacity(num_frames);
    let frame_stride = 56usize;
    let frames_end = ofs_frames
        .checked_add(num_frames * frame_stride)
        .ok_or_else(|| ModelError::Message("MD3 frames overflow".into()))?;
    if frames_end > data.len() {
        return Err(ModelError::Message("MD3 frames truncated".into()));
    }
    for fi in 0..num_frames {
        let o = ofs_frames + fi * frame_stride + 40; // name at +40
        let nb = &data[o..o + 16];
        let end = nb.iter().position(|&b| b == 0).unwrap_or(16);
        frame_names.push(String::from_utf8_lossy(&nb[..end]).into_owned());
    }

    // First surface header (108 bytes typical)
    if ofs_surfaces + 100 > data.len() {
        return Err(ModelError::Message("MD3 surface truncated".into()));
    }
    let surf = ofs_surfaces;
    let surf_ident = i32::from_le_bytes(data[surf..surf + 4].try_into().unwrap());
    if surf_ident != MD3_IDENT {
        return Err(ModelError::Message("MD3 surface bad ident".into()));
    }
    let name_bytes = &data[surf + 4..surf + 68];
    let name_end = name_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(name_bytes.len());
    let surface_name = String::from_utf8_lossy(&name_bytes[..name_end]).into_owned();
    let surf_num_frames =
        i32::from_le_bytes(data[surf + 72..surf + 76].try_into().unwrap()) as usize;
    let num_shaders = i32::from_le_bytes(data[surf + 76..surf + 80].try_into().unwrap());
    let num_verts = i32::from_le_bytes(data[surf + 80..surf + 84].try_into().unwrap());
    let num_tris = i32::from_le_bytes(data[surf + 84..surf + 88].try_into().unwrap());
    let ofs_tris = i32::from_le_bytes(data[surf + 88..surf + 92].try_into().unwrap()) as usize;
    let _ofs_shaders = i32::from_le_bytes(data[surf + 92..surf + 96].try_into().unwrap());
    let ofs_st = i32::from_le_bytes(data[surf + 96..surf + 100].try_into().unwrap()) as usize;
    let ofs_xyz = i32::from_le_bytes(data[surf + 100..surf + 104].try_into().unwrap()) as usize;
    if num_verts <= 0 || num_tris <= 0 || surf_num_frames == 0 {
        return Err(ModelError::Message("MD3 surface empty".into()));
    }
    let num_verts = num_verts as usize;
    let num_tris = num_tris as usize;
    let _ = num_shaders;

    // Triangles: 3 × i32 indices, relative to surface
    let tri_abs = surf + ofs_tris;
    let tri_end = tri_abs
        .checked_add(num_tris * 12)
        .ok_or_else(|| ModelError::Message("MD3 tris overflow".into()))?;
    if tri_end > data.len() {
        return Err(ModelError::Message("MD3 tris truncated".into()));
    }
    let mut indices = Vec::with_capacity(num_tris * 3);
    for i in 0..num_tris {
        let o = tri_abs + i * 12;
        for k in 0..3 {
            let idx = i32::from_le_bytes(data[o + k * 4..o + k * 4 + 4].try_into().unwrap());
            if idx < 0 || idx as usize >= num_verts {
                return Err(ModelError::Message("MD3 index OOB".into()));
            }
            indices.push(idx as u32);
        }
    }

    // ST: num_verts × (f32, f32)
    let st_abs = surf + ofs_st;
    let st_end = st_abs
        .checked_add(num_verts * 8)
        .ok_or_else(|| ModelError::Message("MD3 ST overflow".into()))?;
    if st_end > data.len() {
        return Err(ModelError::Message("MD3 ST truncated".into()));
    }
    let mut sts = Vec::with_capacity(num_verts);
    for i in 0..num_verts {
        let o = st_abs + i * 8;
        let s = f32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let t = f32::from_le_bytes(data[o + 4..o + 8].try_into().unwrap());
        sts.push([s, t]);
    }

    // XYZ normals: per frame, num_verts × 8 bytes (i16×3 + u8 lat + u8 lng)
    let xyz_abs = surf + ofs_xyz;
    let vert_stride = 8usize;
    let frame_vert_bytes = num_verts * vert_stride;
    let nframes = surf_num_frames.min(num_frames);
    let xyz_end = xyz_abs
        .checked_add(nframes * frame_vert_bytes)
        .ok_or_else(|| ModelError::Message("MD3 xyz overflow".into()))?;
    if xyz_end > data.len() {
        return Err(ModelError::Message("MD3 xyz truncated".into()));
    }

    let mut frames = Vec::with_capacity(nframes);
    for fi in 0..nframes {
        let mut vertices = Vec::with_capacity(num_verts);
        let base = xyz_abs + fi * frame_vert_bytes;
        for vi in 0..num_verts {
            let o = base + vi * vert_stride;
            let x = i16::from_le_bytes(data[o..o + 2].try_into().unwrap()) as f32 * MD3_XYZ_SCALE;
            let y =
                i16::from_le_bytes(data[o + 2..o + 4].try_into().unwrap()) as f32 * MD3_XYZ_SCALE;
            let z =
                i16::from_le_bytes(data[o + 4..o + 6].try_into().unwrap()) as f32 * MD3_XYZ_SCALE;
            let lat = f32::from(data[o + 6]) * (std::f32::consts::PI / 128.0);
            let lng = f32::from(data[o + 7]) * (std::f32::consts::PI / 128.0);
            let normal = [lat.cos() * lng.sin(), lng.cos(), lat.sin() * lng.sin()];
            vertices.push(ModelVertex {
                pos: [x, y, z],
                uv: sts[vi],
                normal,
            });
        }
        frames.push(ModelMesh {
            vertices,
            indices: indices.clone(),
        });
    }

    Ok(Md3Model {
        surface_name,
        frame_names,
        frames,
    })
}

/// IQM joint (bind-pose local TRS).
#[derive(Clone, Debug)]
pub struct IqmJoint {
    /// Parent index (`< 0` = root).
    pub parent: i32,
    /// Local translation.
    pub translate: Vec3,
    /// Local rotation (xyzw).
    pub rotate: Quat,
    /// Local scale.
    pub scale: Vec3,
}

/// IQM mesh with optional CPU skinning data.
#[derive(Clone, Debug)]
pub struct IqmModel {
    /// Bind-pose mesh.
    pub mesh: ModelMesh,
    /// Skeleton joints (may be empty).
    pub joints: Vec<IqmJoint>,
    /// Absolute bind-pose matrices.
    pub base_mats: Vec<Mat4>,
    /// Inverse bind-pose matrices.
    pub inv_base_mats: Vec<Mat4>,
    /// Per-vertex bone indexes (empty if unskinned).
    pub blend_indexes: Vec<[u8; 4]>,
    /// Per-vertex bone weights 0..=255 (empty if unskinned).
    pub blend_weights: Vec<[u8; 4]>,
    /// Joint names, animations and decoded frames (UZ7).
    pub anim: crate::iqm_anim::IqmAnimData,
}

impl IqmModel {
    /// `true` when blend data is present for every vertex.
    #[must_use]
    pub fn is_skinned(&self) -> bool {
        !self.blend_indexes.is_empty()
            && self.blend_indexes.len() == self.mesh.vertices.len()
            && self.blend_weights.len() == self.mesh.vertices.len()
            && !self.base_mats.is_empty()
    }

    /// CPU-skin with absolute joint matrices (`len` = joint count).
    ///
    /// `out = Σ wᵢ · (frameᵢ · inv_baseᵢ) · bind_pos`
    #[must_use]
    pub fn skin_cpu(&self, frame_abs: &[Mat4]) -> ModelMesh {
        if !self.is_skinned() || frame_abs.len() != self.base_mats.len() {
            return self.mesh.clone();
        }
        let mut skin_mats = Vec::with_capacity(self.base_mats.len());
        for (i, &frame) in frame_abs.iter().enumerate() {
            skin_mats.push(frame * self.inv_base_mats[i]);
        }
        let mut vertices = self.mesh.vertices.clone();
        for (vi, v) in vertices.iter_mut().enumerate() {
            let idx = self.blend_indexes[vi];
            let wraw = self.blend_weights[vi];
            let mut wsum = 0.0f32;
            let ws = [
                f32::from(wraw[0]) / 255.0,
                f32::from(wraw[1]) / 255.0,
                f32::from(wraw[2]) / 255.0,
                f32::from(wraw[3]) / 255.0,
            ];
            for w in ws {
                wsum += w;
            }
            if wsum < 1e-6 {
                continue;
            }
            let p = Vec3::from_array(v.pos);
            let mut out = Vec3::ZERO;
            for k in 0..4 {
                let ji = idx[k] as usize;
                if ji >= skin_mats.len() || ws[k] <= 0.0 {
                    continue;
                }
                out += skin_mats[ji].transform_point3(p) * (ws[k] / wsum);
            }
            v.pos = out.to_array();
        }
        ModelMesh {
            vertices,
            indices: self.mesh.indices.clone(),
        }
    }

    /// Skin with bind-pose matrices (should ≈ identity).
    #[must_use]
    pub fn skin_bind_pose(&self) -> ModelMesh {
        self.skin_cpu(&self.base_mats)
    }
}

/// Parse an IQM v2 mesh (bind pose; skinning data when present).
pub fn parse_iqm(data: &[u8]) -> Result<ModelMesh, ModelError> {
    Ok(parse_iqm_full(data)?.mesh)
}

/// Parse IQM including joints + blend weights for CPU skinning.
pub fn parse_iqm_full(data: &[u8]) -> Result<IqmModel, ModelError> {
    if data.len() < 16 + 15 * 4 {
        return Err(ModelError::Message("IQM truncated header".into()));
    }
    if &data[0..16] != IQM_MAGIC {
        return Err(ModelError::Message("IQM bad magic".into()));
    }
    let version = u32::from_le_bytes(data[16..20].try_into().unwrap());
    if version != IQM_VERSION {
        return Err(ModelError::Message(format!(
            "IQM unsupported version {version}"
        )));
    }
    let num_vertexarrays = u32::from_le_bytes(data[44..48].try_into().unwrap()) as usize;
    let num_vertexes = u32::from_le_bytes(data[48..52].try_into().unwrap()) as usize;
    let ofs_vertexarrays = u32::from_le_bytes(data[52..56].try_into().unwrap()) as usize;
    let num_triangles = u32::from_le_bytes(data[56..60].try_into().unwrap()) as usize;
    let ofs_triangles = u32::from_le_bytes(data[60..64].try_into().unwrap()) as usize;
    let num_joints = u32::from_le_bytes(data[68..72].try_into().unwrap()) as usize;
    let ofs_joints = u32::from_le_bytes(data[72..76].try_into().unwrap()) as usize;
    if num_vertexes == 0 || num_triangles == 0 {
        return Err(ModelError::Message("IQM empty mesh".into()));
    }

    let va_end = ofs_vertexarrays
        .checked_add(num_vertexarrays * 20)
        .ok_or_else(|| ModelError::Message("IQM va overflow".into()))?;
    if va_end > data.len() {
        return Err(ModelError::Message("IQM vertexarrays truncated".into()));
    }
    let mut positions: Option<&[u8]> = None;
    let mut texcoords: Option<&[u8]> = None;
    let mut normals: Option<&[u8]> = None;
    let mut blend_idx: Option<&[u8]> = None;
    let mut blend_w: Option<&[u8]> = None;
    for i in 0..num_vertexarrays {
        let o = ofs_vertexarrays + i * 20;
        let ty = u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let format = u32::from_le_bytes(data[o + 8..o + 12].try_into().unwrap());
        let size = u32::from_le_bytes(data[o + 12..o + 16].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(data[o + 16..o + 20].try_into().unwrap()) as usize;
        let elem = match format {
            IQM_FLOAT => 4usize,
            IQM_UBYTE => 1usize,
            _ => continue,
        };
        let bytes = num_vertexes * size * elem;
        let end = offset
            .checked_add(bytes)
            .ok_or_else(|| ModelError::Message("IQM attr overflow".into()))?;
        if end > data.len() {
            return Err(ModelError::Message("IQM attr truncated".into()));
        }
        let slice = &data[offset..end];
        match (ty, format) {
            (IQM_POSITION, IQM_FLOAT) if size >= 3 => positions = Some(slice),
            (IQM_TEXCOORD, IQM_FLOAT) if size >= 2 => texcoords = Some(slice),
            (IQM_NORMAL, IQM_FLOAT) if size >= 3 => normals = Some(slice),
            (IQM_BLENDINDEXES, IQM_UBYTE) if size >= 4 => blend_idx = Some(slice),
            (IQM_BLENDWEIGHTS, IQM_UBYTE) if size >= 4 => blend_w = Some(slice),
            _ => {}
        }
    }
    let Some(pos_bytes) = positions else {
        return Err(ModelError::Message("IQM missing position array".into()));
    };

    let mut vertices = Vec::with_capacity(num_vertexes);
    for vi in 0..num_vertexes {
        let po = vi * 12;
        let x = f32::from_le_bytes(pos_bytes[po..po + 4].try_into().unwrap());
        let y = f32::from_le_bytes(pos_bytes[po + 4..po + 8].try_into().unwrap());
        let z = f32::from_le_bytes(pos_bytes[po + 8..po + 12].try_into().unwrap());
        let uv = if let Some(tc) = texcoords {
            let to = vi * 8;
            [
                f32::from_le_bytes(tc[to..to + 4].try_into().unwrap()),
                f32::from_le_bytes(tc[to + 4..to + 8].try_into().unwrap()),
            ]
        } else {
            [0.0, 0.0]
        };
        let normal = if let Some(nr) = normals {
            let no = vi * 12;
            [
                f32::from_le_bytes(nr[no..no + 4].try_into().unwrap()),
                f32::from_le_bytes(nr[no + 4..no + 8].try_into().unwrap()),
                f32::from_le_bytes(nr[no + 8..no + 12].try_into().unwrap()),
            ]
        } else {
            [0.0, 0.0, 1.0]
        };
        vertices.push(ModelVertex {
            pos: [x, y, z],
            uv,
            normal,
        });
    }

    let mut blend_indexes = Vec::new();
    let mut blend_weights = Vec::new();
    if let (Some(bi), Some(bw)) = (blend_idx, blend_w) {
        blend_indexes.reserve(num_vertexes);
        blend_weights.reserve(num_vertexes);
        for vi in 0..num_vertexes {
            let o = vi * 4;
            blend_indexes.push([bi[o], bi[o + 1], bi[o + 2], bi[o + 3]]);
            blend_weights.push([bw[o], bw[o + 1], bw[o + 2], bw[o + 3]]);
        }
    }

    let tri_end = ofs_triangles
        .checked_add(num_triangles * 12)
        .ok_or_else(|| ModelError::Message("IQM tris overflow".into()))?;
    if tri_end > data.len() {
        return Err(ModelError::Message("IQM tris truncated".into()));
    }
    let mut indices = Vec::with_capacity(num_triangles * 3);
    for i in 0..num_triangles {
        let o = ofs_triangles + i * 12;
        for k in 0..3 {
            let idx = u32::from_le_bytes(data[o + k * 4..o + k * 4 + 4].try_into().unwrap());
            if idx as usize >= num_vertexes {
                return Err(ModelError::Message("IQM index OOB".into()));
            }
            indices.push(idx);
        }
    }

    let mut joints = Vec::new();
    let mut base_mats = Vec::new();
    let mut inv_base_mats = Vec::new();
    if num_joints > 0 {
        const JOINT_SIZE: usize = 48;
        let j_end = ofs_joints
            .checked_add(num_joints * JOINT_SIZE)
            .ok_or_else(|| ModelError::Message("IQM joints overflow".into()))?;
        if j_end > data.len() {
            return Err(ModelError::Message("IQM joints truncated".into()));
        }
        let mut locals = Vec::with_capacity(num_joints);
        for ji in 0..num_joints {
            let o = ofs_joints + ji * JOINT_SIZE;
            let parent = i32::from_le_bytes(data[o + 4..o + 8].try_into().unwrap());
            let t = Vec3::new(
                f32::from_le_bytes(data[o + 8..o + 12].try_into().unwrap()),
                f32::from_le_bytes(data[o + 12..o + 16].try_into().unwrap()),
                f32::from_le_bytes(data[o + 16..o + 20].try_into().unwrap()),
            );
            let r = Quat::from_xyzw(
                f32::from_le_bytes(data[o + 20..o + 24].try_into().unwrap()),
                f32::from_le_bytes(data[o + 24..o + 28].try_into().unwrap()),
                f32::from_le_bytes(data[o + 28..o + 32].try_into().unwrap()),
                f32::from_le_bytes(data[o + 32..o + 36].try_into().unwrap()),
            );
            let s = Vec3::new(
                f32::from_le_bytes(data[o + 36..o + 40].try_into().unwrap()),
                f32::from_le_bytes(data[o + 40..o + 44].try_into().unwrap()),
                f32::from_le_bytes(data[o + 44..o + 48].try_into().unwrap()),
            );
            joints.push(IqmJoint {
                parent,
                translate: t,
                rotate: r,
                scale: s,
            });
            locals.push(Mat4::from_scale_rotation_translation(s, r, t));
        }
        base_mats = vec![Mat4::IDENTITY; num_joints];
        for (i, j) in joints.iter().enumerate() {
            base_mats[i] = if j.parent >= 0 {
                base_mats[j.parent as usize] * locals[i]
            } else {
                locals[i]
            };
        }
        inv_base_mats = base_mats.iter().map(|m| m.inverse()).collect();
    }

    let anim = crate::iqm_anim::parse_iqm_anim_data(data, &joints).unwrap_or_default();
    Ok(IqmModel {
        mesh: ModelMesh { vertices, indices },
        joints,
        base_mats,
        inv_base_mats,
        blend_indexes,
        blend_weights,
        anim,
    })
}

/// Auto-detect mesh format from magic / extension hint and parse.
pub fn parse_model_bytes(data: &[u8], path_hint: &str) -> Result<ModelMesh, ModelError> {
    let lower = path_hint.to_ascii_lowercase();
    if data.len() >= 16 && &data[0..16] == IQM_MAGIC {
        return parse_iqm(data);
    }
    if data.len() >= 4 {
        let id = i32::from_le_bytes(data[0..4].try_into().unwrap());
        if id == MD2_IDENT {
            let md2 = parse_md2(data)?;
            return md2
                .frame_mesh(0)
                .cloned()
                .ok_or_else(|| ModelError::Message("MD2 has no frames".into()));
        }
        if id == MD3_IDENT {
            let md3 = parse_md3(data)?;
            return md3
                .frame_mesh(0)
                .cloned()
                .ok_or_else(|| ModelError::Message("MD3 has no frames".into()));
        }
    }
    if lower.ends_with(".obj") || data.starts_with(b"v ") || data.starts_with(b"#") {
        let text =
            std::str::from_utf8(data).map_err(|_| ModelError::Message("OBJ not UTF-8".into()))?;
        return parse_obj(text);
    }
    Err(ModelError::Message(format!(
        "unknown model format for `{path_hint}`"
    )))
}

fn parse_f(tok: Option<&str>, line: usize, what: &str) -> Result<f32, ModelError> {
    let t =
        tok.ok_or_else(|| ModelError::Message(format!("OBJ line {}: missing {what}", line + 1)))?;
    t.parse::<f32>()
        .map_err(|_| ModelError::Message(format!("OBJ line {}: bad float `{t}`", line + 1)))
}

fn parse_face_vert(tok: &str, line: usize) -> Result<(i32, i32, i32), ModelError> {
    // v, v/vt, v//vn, v/vt/vn
    let mut sp = tok.split('/');
    let v = sp
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ModelError::Message(format!("OBJ line {}: bad face `{tok}`", line + 1)))?;
    let vt = sp.next().and_then(|s| {
        if s.is_empty() {
            Some(0)
        } else {
            s.parse().ok()
        }
    });
    let vn = sp.next().and_then(|s| {
        if s.is_empty() {
            Some(0)
        } else {
            s.parse().ok()
        }
    });
    Ok((v, vt.unwrap_or(0), vn.unwrap_or(0)))
}

fn resolve_index<T: Copy>(list: &[T], idx: i32) -> T {
    let n = list.len() as i32;
    let i = if idx < 0 { n + idx } else { idx - 1 };
    list.get(i.clamp(0, n - 1) as usize)
        .copied()
        .unwrap_or_else(|| list[0])
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_tri_obj() {
        let obj = r#"
v 0 0 0
v 1 0 0
v 0 1 0
vt 0 0
vt 1 0
vt 0 1
vn 0 0 1
f 1/1/1 2/2/1 3/3/1
"#;
        let m = parse_obj(obj).unwrap();
        assert_eq!(m.indices.len(), 3);
        assert_eq!(m.vertices.len(), 3);
        let (min, max) = m.bounds();
        assert!((max.x - min.x - 1.0).abs() < 1e-5);
    }

    #[test]
    fn fans_quad() {
        let obj = "\
v 0 0 0
v 1 0 0
v 1 1 0
v 0 1 0
f 1 2 3 4
";
        let m = parse_obj(obj).unwrap();
        assert_eq!(m.indices.len(), 6);
    }

    #[test]
    fn iqm_rejects_bad_magic() {
        assert!(parse_iqm(&[0u8; 64]).is_err());
    }

    #[test]
    fn iqm_skin_bind_pose_is_identity() {
        let mesh = ModelMesh {
            vertices: vec![ModelVertex {
                pos: [1.0, 2.0, 3.0],
                uv: [0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            }],
            indices: vec![0, 0, 0],
        };
        let base = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let iqm = IqmModel {
            mesh: mesh.clone(),
            joints: vec![IqmJoint {
                parent: -1,
                translate: Vec3::new(10.0, 0.0, 0.0),
                rotate: Quat::IDENTITY,
                scale: Vec3::ONE,
            }],
            base_mats: vec![base],
            inv_base_mats: vec![base.inverse()],
            blend_indexes: vec![[0, 0, 0, 0]],
            blend_weights: vec![[255, 0, 0, 0]],
            anim: Default::default(),
        };
        let skinned = iqm.skin_bind_pose();
        let p = skinned.vertices[0].pos;
        assert!((p[0] - 1.0).abs() < 1e-4);
        assert!((p[1] - 2.0).abs() < 1e-4);
        assert!((p[2] - 3.0).abs() < 1e-4);
    }

    #[test]
    fn iqm_skin_translates_with_joint() {
        let mesh = ModelMesh {
            vertices: vec![ModelVertex {
                pos: [0.0, 0.0, 0.0],
                uv: [0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            }],
            indices: vec![0, 0, 0],
        };
        let base = Mat4::IDENTITY;
        let iqm = IqmModel {
            mesh,
            joints: vec![IqmJoint {
                parent: -1,
                translate: Vec3::ZERO,
                rotate: Quat::IDENTITY,
                scale: Vec3::ONE,
            }],
            base_mats: vec![base],
            inv_base_mats: vec![base.inverse()],
            blend_indexes: vec![[0, 0, 0, 0]],
            blend_weights: vec![[255, 0, 0, 0]],
            anim: Default::default(),
        };
        let posed = [Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0))];
        let out = iqm.skin_cpu(&posed);
        assert!((out.vertices[0].pos[0] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn parses_minimal_iqm() {
        // Header + 1 float position VA + 1 triangle, 3 verts.
        let num_vertexes = 3u32;
        let num_triangles = 1u32;
        let num_vertexarrays = 1u32;
        // magic[16] + 27×u32
        let header_size = 124usize;
        let ofs_vertexarrays = header_size as u32;
        let va_bytes = 20u32;
        let ofs_positions = ofs_vertexarrays + va_bytes;
        let pos_bytes = num_vertexes * 3 * 4;
        let ofs_triangles = ofs_positions + pos_bytes;
        let total = (ofs_triangles + num_triangles * 12) as usize;
        let mut data = vec![0u8; total];
        data[0..16].copy_from_slice(IQM_MAGIC);
        data[16..20].copy_from_slice(&IQM_VERSION.to_le_bytes());
        data[44..48].copy_from_slice(&num_vertexarrays.to_le_bytes());
        data[48..52].copy_from_slice(&num_vertexes.to_le_bytes());
        data[52..56].copy_from_slice(&ofs_vertexarrays.to_le_bytes());
        data[56..60].copy_from_slice(&num_triangles.to_le_bytes());
        data[60..64].copy_from_slice(&ofs_triangles.to_le_bytes());
        // VA: type=position, format=float, size=3, offset=ofs_positions
        let o = ofs_vertexarrays as usize;
        data[o..o + 4].copy_from_slice(&IQM_POSITION.to_le_bytes());
        data[o + 8..o + 12].copy_from_slice(&IQM_FLOAT.to_le_bytes());
        data[o + 12..o + 16].copy_from_slice(&3u32.to_le_bytes());
        data[o + 16..o + 20].copy_from_slice(&ofs_positions.to_le_bytes());
        // Positions
        let p = ofs_positions as usize;
        let verts = [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        for (i, v) in verts.iter().enumerate() {
            for (k, c) in v.iter().enumerate() {
                let off = p + i * 12 + k * 4;
                data[off..off + 4].copy_from_slice(&c.to_le_bytes());
            }
        }
        // Triangle
        let t = ofs_triangles as usize;
        for k in 0..3u32 {
            data[t + k as usize * 4..t + k as usize * 4 + 4].copy_from_slice(&k.to_le_bytes());
        }
        let m = parse_iqm(&data).unwrap();
        assert_eq!(m.indices.len(), 3);
        assert!((m.vertices[1].pos[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn md3_rejects_bad_ident() {
        let mut data = vec![0u8; 108];
        data[0..4].copy_from_slice(&0i32.to_le_bytes());
        assert!(parse_md3(&data).is_err());
    }

    #[test]
    fn parses_minimal_md3() {
        // Header + 1 frame + 1 surface with 3 verts / 1 tri.
        let num_frames = 1i32;
        let ofs_frames = 108i32;
        let ofs_surfaces = ofs_frames + 56; // one md3Frame
        let surf = ofs_surfaces as usize;
        // Surface layout after 108-byte surf header:
        // tris at +108 relative, st after tris, xyz after st
        let num_verts = 3i32;
        let num_tris = 1i32;
        let ofs_tris = 108i32;
        let ofs_st = ofs_tris + num_tris * 12;
        let ofs_xyz = ofs_st + num_verts * 8;
        let surf_end = ofs_xyz + num_verts * 8;
        let total = surf + surf_end as usize;
        let mut data = vec![0u8; total];
        data[0..4].copy_from_slice(&MD3_IDENT.to_le_bytes());
        data[4..8].copy_from_slice(&MD3_VERSION.to_le_bytes());
        data[72..76].copy_from_slice(&num_frames.to_le_bytes());
        data[80..84].copy_from_slice(&1i32.to_le_bytes()); // num_surfaces
        data[88..92].copy_from_slice(&ofs_frames.to_le_bytes());
        data[96..100].copy_from_slice(&ofs_surfaces.to_le_bytes());
        // Frame name
        data[ofs_frames as usize + 40..ofs_frames as usize + 44].copy_from_slice(b"base");
        // Surface
        data[surf..surf + 4].copy_from_slice(&MD3_IDENT.to_le_bytes());
        data[surf + 4..surf + 8].copy_from_slice(b"mesh");
        data[surf + 72..surf + 76].copy_from_slice(&1i32.to_le_bytes()); // surf frames
        data[surf + 80..surf + 84].copy_from_slice(&num_verts.to_le_bytes());
        data[surf + 84..surf + 88].copy_from_slice(&num_tris.to_le_bytes());
        data[surf + 88..surf + 92].copy_from_slice(&ofs_tris.to_le_bytes());
        data[surf + 96..surf + 100].copy_from_slice(&ofs_st.to_le_bytes());
        data[surf + 100..surf + 104].copy_from_slice(&ofs_xyz.to_le_bytes());
        // Triangle indices 0,1,2
        let to = surf + ofs_tris as usize;
        for k in 0..3i32 {
            data[to + k as usize * 4..to + k as usize * 4 + 4].copy_from_slice(&k.to_le_bytes());
        }
        // XYZ: (0,0,0), (64,0,0), (0,64,0) as i16 → 1.0 units
        let xo = surf + ofs_xyz as usize;
        data[xo + 8..xo + 10].copy_from_slice(&64i16.to_le_bytes());
        data[xo + 18..xo + 20].copy_from_slice(&64i16.to_le_bytes());
        let md3 = parse_md3(&data).unwrap();
        assert_eq!(md3.frames.len(), 1);
        assert_eq!(md3.frame_mesh(0).unwrap().indices.len(), 3);
        assert!((md3.frames[0].vertices[1].pos[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn to_wall_verts_expands_tris() {
        let m = parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let w = m.to_wall_vertices(Vec3::new(10.0, 0.0, 0.0), Vec3::ONE, 1.0);
        assert_eq!(w.len(), 3);
        assert!((w[0].pos[0] - 10.0).abs() < 1e-5);
    }

    #[test]
    fn parses_minimal_md2() {
        // 3 verts, 3 ST, 1 tri, 1 frame.
        let num_xyz = 3i32;
        let num_st = 3i32;
        let num_tris = 1i32;
        let num_frames = 1i32;
        let ofs_st = 68i32;
        let ofs_tris = ofs_st + num_st * 4;
        let ofs_frames = ofs_tris + num_tris * 12;
        let frame_stride = 40 + num_xyz * 4;
        let end = ofs_frames + frame_stride;
        let mut data = vec![0u8; end as usize];
        data[0..4].copy_from_slice(&MD2_IDENT.to_le_bytes());
        data[4..8].copy_from_slice(&MD2_VERSION.to_le_bytes());
        data[8..12].copy_from_slice(&64i32.to_le_bytes()); // skin w
        data[12..16].copy_from_slice(&64i32.to_le_bytes());
        data[16..20].copy_from_slice(&frame_stride.to_le_bytes());
        data[24..28].copy_from_slice(&num_xyz.to_le_bytes());
        data[28..32].copy_from_slice(&num_st.to_le_bytes());
        data[32..36].copy_from_slice(&num_tris.to_le_bytes());
        data[40..44].copy_from_slice(&num_frames.to_le_bytes());
        data[48..52].copy_from_slice(&ofs_st.to_le_bytes());
        data[52..56].copy_from_slice(&ofs_tris.to_le_bytes());
        data[56..60].copy_from_slice(&ofs_frames.to_le_bytes());
        // ST at corners
        for i in 0..3 {
            let o = ofs_st as usize + i * 4;
            let s = (i as i16) * 32;
            data[o..o + 2].copy_from_slice(&s.to_le_bytes());
        }
        // One triangle 0,1,2
        let to = ofs_tris as usize;
        for k in 0..3u16 {
            data[to + k as usize * 2..to + k as usize * 2 + 2].copy_from_slice(&k.to_le_bytes());
            data[to + 6 + k as usize * 2..to + 8 + k as usize * 2]
                .copy_from_slice(&k.to_le_bytes());
        }
        // Frame scale=1, translate=0, name "base"
        let fo = ofs_frames as usize;
        for i in 0..3 {
            data[fo + i * 4..fo + i * 4 + 4].copy_from_slice(&1.0f32.to_le_bytes());
        }
        data[fo + 24..fo + 28].copy_from_slice(b"base");
        // Packed verts (0,0,0), (1,0,0), (0,1,0)
        data[fo + 40] = 0;
        data[fo + 44] = 1;
        data[fo + 48] = 0;
        data[fo + 49] = 1;
        let md2 = parse_md2(&data).unwrap();
        assert_eq!(md2.frames.len(), 1);
        assert_eq!(md2.frame_names[0], "base");
        assert_eq!(md2.frame_mesh(0).unwrap().indices.len(), 3);
    }
}
