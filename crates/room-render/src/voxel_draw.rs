//! Draw VOXELDEF sprites as KVX meshes (GZDoom model matrix, Z-up world).

use crate::kvx::{kvx_to_mesh, parse_kvx};
use crate::models::{ModelError, ModelMesh};
use crate::scene::SceneLists;
use glam::{Mat4, Vec3};
use room_vfs::{Namespace, Vfs};
use std::collections::BTreeMap;

/// One actor whose current sprite frame is a voxel.
#[derive(Clone, Debug)]
pub struct VoxelActorPose {
    /// Voxel file from VOXELDEF.
    pub file: String,
    /// `OverridePalette`: index colours through `PLAYPAL`.
    pub override_palette: bool,
    /// Feet in map space.
    pub origin: Vec3,
    /// Actor yaw in degrees (Doom: 0 = east).
    pub yaw_deg: f32,
    /// Extra pitch in degrees (already signed the way GZDoom sums it).
    pub pitch_deg: f32,
    /// Actor roll in degrees.
    pub roll_deg: f32,
    /// Spin offset in degrees for this tic (`PlacedSpin` / `DroppedSpin`).
    pub spin_deg: f32,
    /// Actor `scale_x` / `scale_y`.
    pub actor_scale: [f32; 2],
    /// VOXELDEF `Scale`.
    pub voxel_scale: f32,
    /// VOXELDEF angle (includes the Build +90°).
    pub angle_offset: f32,
    /// `XOffset` / `YOffset` / `ZOffset`.
    pub offset: [f32; 3],
    /// Sector light 0..=1.
    pub light: f32,
}

/// Cached KVX mesh plus the 256×1 palette texture name.
#[derive(Clone, Debug)]
struct CachedVoxel {
    mesh: ModelMesh,
    /// Material name (`__VOXEL…`).
    palette_name: String,
    /// RGBA8 256×1.
    palette_rgba: Vec<u8>,
    uploaded: bool,
}

/// KVX meshes keyed by file path and palette choice.
#[derive(Clone, Debug, Default)]
pub struct VoxelCatalog {
    meshes: BTreeMap<String, CachedVoxel>,
}

impl VoxelCatalog {
    /// Empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember that `palette_name` reached the GPU.
    pub fn mark_palette_uploaded(&mut self, name: &str) {
        for entry in self.meshes.values_mut() {
            if entry.palette_name == name {
                entry.uploaded = true;
            }
        }
    }

    /// `true` when `file` parses. Failures stay uncached so a sprite can show.
    pub fn has_file(&mut self, vfs: &Vfs, file: &str, override_palette: bool) -> bool {
        self.ensure(vfs, file, override_palette).is_ok()
    }

    /// Append posed voxels into the opaque wall list.
    ///
    /// Returns palette images that still need a GPU upload (`name`, 256×1 RGBA).
    pub fn append_poses(
        &mut self,
        lists: &mut SceneLists,
        vfs: &Vfs,
        poses: &[VoxelActorPose],
    ) -> Vec<(String, Vec<u8>)> {
        let mut uploads = Vec::new();
        for pose in poses {
            let Ok(key) = self.ensure(vfs, &pose.file, pose.override_palette) else {
                continue;
            };
            let Some(entry) = self.meshes.get_mut(&key) else {
                continue;
            };
            if !entry.uploaded {
                let name = entry.palette_name.clone();
                if !uploads.iter().any(|(n, _)| n == &name) {
                    uploads.push((name, entry.palette_rgba.clone()));
                }
            }
            let model = voxel_model_matrix(pose);
            let verts = entry.mesh.to_wall_vertices_transformed(model, pose.light);
            if verts.is_empty() {
                continue;
            }
            #[allow(clippy::cast_possible_truncation)]
            let start = lists.opaque_walls.len() as u32;
            #[allow(clippy::cast_possible_truncation)]
            let count = verts.len() as u32;
            let name = entry.palette_name.clone();
            lists.opaque_walls.extend_from_slice(&verts);
            lists
                .wall_ranges
                .push(crate::scene::MaterialRange { name, start, count });
        }
        uploads
    }
}

/// Pitch and roll for a voxel pose (GZDoom `MDL_PITCHFROMMOMENTUM` /
/// `USEACTORPITCH` / `USEACTORROLL`). Returns `(pitch_deg, roll_deg)`.
#[must_use]
pub fn voxel_view_angles(
    yaw_deg: f32,
    actor_pitch_deg: f32,
    actor_roll_deg: f32,
    vel: [f32; 3],
    pitch_from_momentum: bool,
    use_actor_pitch: bool,
    use_actor_roll: bool,
) -> (f32, f32) {
    let mut pitch = 0.0f32;
    if pitch_from_momentum {
        let [x, y, z] = vel;
        let h2 = x * x + y * y;
        let h = h2.sqrt();
        if h2 + z * z > 1.0e-8 && (z != 0.0 || x != 0.0 || y != 0.0) {
            pitch = (z / h.max(1.0e-4)).atan().to_degrees();
            if h > 1.0e-4 {
                let ang = yaw_deg.to_radians();
                if (x * ang.cos() + y * ang.sin()) / h < 0.0 {
                    pitch = -pitch;
                }
            } else {
                pitch = pitch.abs();
            }
        }
    }
    if use_actor_pitch {
        pitch -= actor_pitch_deg;
    }
    let roll = if use_actor_roll { actor_roll_deg } else { 0.0 };
    (pitch, roll)
}

/// GZDoom `rotationSpeed = spin / 55.55`, then
/// `turns = tic / (200 / rotationSpeed)` → degrees.
#[must_use]
pub fn voxel_spin_degrees(spin: i32, tic: f32) -> f32 {
    if spin == 0 {
        return 0.0;
    }
    let speed = spin as f32 / 55.55;
    if speed.abs() < 1.0e-8 {
        return 0.0;
    }
    let turns = tic / (200.0 / speed);
    let frac = turns - turns.floor();
    frac * 360.0
}

impl VoxelCatalog {
    fn ensure(
        &mut self,
        vfs: &Vfs,
        file: &str,
        override_palette: bool,
    ) -> Result<String, ModelError> {
        let key = format!(
            "{}|{}",
            file.replace('\\', "/").to_ascii_lowercase(),
            u8::from(override_palette)
        );
        if self.meshes.contains_key(&key) {
            return Ok(key);
        }
        let bytes = read_voxel(vfs, file)?;
        let kvx = parse_kvx(&bytes)?;
        let mesh = kvx_to_mesh(&kvx);
        if mesh.indices.is_empty() {
            return Err(ModelError::Message(format!("empty voxel `{file}`")));
        }
        let rgba = palette_rgba(kvx.palette.as_ref(), override_palette, &game_palette(vfs));
        let palette_name = format!("__VOXEL{key}");
        self.meshes.insert(
            key.clone(),
            CachedVoxel {
                mesh,
                palette_name,
                palette_rgba: rgba,
                uploaded: false,
            },
        );
        Ok(key)
    }
}

fn read_voxel(vfs: &Vfs, file: &str) -> Result<Vec<u8>, ModelError> {
    let norm = file.replace('\\', "/");
    let stem = norm.rsplit('/').next().unwrap_or(norm.as_str());
    let stem = stem
        .strip_suffix(".kvx")
        .or_else(|| stem.strip_suffix(".KVX"))
        .unwrap_or(stem);
    let candidates = [
        norm.clone(),
        format!("{norm}.kvx"),
        format!("voxels/{stem}.kvx"),
        format!("voxels/{stem}"),
    ];
    for path in &candidates {
        if let Some(id) = vfs.find_full(path) {
            let bytes = vfs
                .read(id)
                .map_err(|e| ModelError::Message(e.to_string()))?;
            return Ok(bytes.to_vec());
        }
    }
    if let Some(id) = vfs
        .find(stem, Namespace::Voxels)
        .or_else(|| vfs.find_any_ns(stem))
    {
        let bytes = vfs
            .read(id)
            .map_err(|e| ModelError::Message(e.to_string()))?;
        return Ok(bytes.to_vec());
    }
    Err(ModelError::Message(format!("voxel not in VFS: `{file}`")))
}

fn game_palette(vfs: &Vfs) -> Option<Vec<u8>> {
    let id = vfs
        .find("PLAYPAL", Namespace::Global)
        .or_else(|| vfs.find_any_ns("PLAYPAL"))?;
    let bytes = vfs.read(id).ok()?;
    if bytes.len() < 768 {
        None
    } else {
        Some(bytes[..768].to_vec())
    }
}

/// 256×1 RGBA. KVX palettes are 6-bit; GZDoom expands `(c<<2)|(c>>4)`.
fn palette_rgba(
    kvx: Option<&[u8; 768]>,
    override_palette: bool,
    playpal: &Option<Vec<u8>>,
) -> Vec<u8> {
    let mut rgba = vec![255u8; 256 * 4];
    let use_kvx = kvx.filter(|_| !override_palette);
    for i in 0..256 {
        let (r, g, b) = if let Some(pal) = use_kvx {
            let o = i * 3;
            (expand6(pal[o]), expand6(pal[o + 1]), expand6(pal[o + 2]))
        } else if let Some(pal) = playpal {
            let o = i * 3;
            (pal[o], pal[o + 1], pal[o + 2])
        } else {
            let g = i as u8;
            (g, g, g)
        };
        let o = i * 4;
        rgba[o] = r;
        rgba[o + 1] = g;
        rgba[o + 2] = b;
        rgba[o + 3] = 255;
    }
    rgba
}

fn expand6(c: u8) -> u8 {
    let c = c & 63;
    (c << 2) | (c >> 4)
}

/// Model matrix: GZDoom `ObjectToWorldMatrix` for a voxel, expressed in Room's
/// Z-up space. Render Y-up `(x, y, z)` is Room `(x, z, y)`.
fn voxel_model_matrix(pose: &VoxelActorPose) -> Mat4 {
    let vox = if pose.voxel_scale.abs() < 1.0e-6 {
        1.0
    } else {
        pose.voxel_scale
    };
    let sfx = pose.actor_scale[0] * vox;
    let sfy = pose.actor_scale[0] * vox;
    let sfz = pose.actor_scale[1] * vox;
    let to_render = Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, // room X → render X
        0.0, 0.0, 1.0, 0.0, // room Y → render Z
        0.0, 1.0, 0.0, 0.0, // room Z → render Y
        0.0, 0.0, 0.0, 1.0,
    ]);
    let mut m = Mat4::from_rotation_y((-pose.angle_offset).to_radians());
    m = Mat4::from_translation(Vec3::new(
        pose.offset[0] / vox,
        pose.offset[2] / vox,
        pose.offset[1] / vox,
    )) * m;
    m = Mat4::from_scale(Vec3::new(sfx, sfz, sfy)) * m;
    m = Mat4::from_rotation_y(pose.spin_deg.to_radians()) * m;
    m = Mat4::from_rotation_x((-pose.roll_deg).to_radians()) * m;
    m = Mat4::from_rotation_z(pose.pitch_deg.to_radians()) * m;
    m = Mat4::from_rotation_y((-pose.yaw_deg).to_radians()) * m;
    m = Mat4::from_translation(Vec3::new(pose.origin.x, pose.origin.z, pose.origin.y)) * m;
    to_render.transpose() * m * to_render
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kvx::parse_kvx;
    use room_defs::DefinitionsBuilder;
    use room_vfs::VfsBuilder;

    #[test]
    fn spin_150_turns_within_a_couple_of_seconds() {
        let a = voxel_spin_degrees(150, 0.0);
        let b = voxel_spin_degrees(150, 35.0);
        assert!(a.abs() < 1e-3);
        let delta = (b - a).rem_euclid(360.0);
        // ~170° per second at 35 tics.
        assert!(delta > 140.0 && delta < 200.0, "{delta}");
    }

    #[test]
    fn cheello_voxel_doom_pack() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/voxel/cheello_voxels.zip");
        if !path.is_file() {
            return;
        }
        let vfs = VfsBuilder::new()
            .filters(["game-doom", "doom", "doom.id", "doom.id.doom2"])
            .mount_path(&path)
            .unwrap()
            .build()
            .unwrap();
        let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
        let poss = defs.voxeldefs.for_sprite("POSS", 0).unwrap();
        assert_eq!(poss.file, "possa");
        assert!((poss.angle_offset - 90.0).abs() < 1e-3);
        let shell = defs.voxeldefs.for_sprite("PINS", 4).unwrap();
        assert_eq!(shell.file, "voxels/pins_shell.kvx");
        assert_eq!(shell.placed_spin, 15);
        // Doom 2 filter (arch-vile frame G).
        assert!(defs.voxeldefs.for_sprite("VILE", 6).is_some());
        assert!(defs.voxeldefs.by_name.len() > 100);

        let mut cat = VoxelCatalog::new();
        assert!(cat.has_file(&vfs, &poss.file, false));
        assert!(cat.has_file(&vfs, &shell.file, false));
        let mut kvx_count = 0u32;
        for (id, info) in vfs.iter() {
            if !info.full_path.ends_with(".kvx") {
                continue;
            }
            let bytes = vfs.read(id).unwrap();
            parse_kvx(&bytes).unwrap_or_else(|e| panic!("{}: {e}", info.full_path));
            kvx_count += 1;
        }
        assert!(kvx_count > 300, "{kvx_count}");
    }
}
