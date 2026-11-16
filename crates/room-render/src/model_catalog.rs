//! MODELDEF path → mesh cache and actor wiring helpers (M10).

use crate::iqm_anim::{
    BoneOverride, IqmAnim, ModelAnim, Trs, absolute_matrices, anim_frames, apply_overrides,
    blend_poses, frame_pose,
};
use crate::models::{
    IqmModel, ModelError, ModelMesh, append_model, parse_iqm_full, parse_model_bytes,
};
use crate::scene::SceneLists;
use crate::ue1::parse_ue1;
use glam::Vec3;
use room_defs::ModelDefs;
use room_vfs::{Namespace, Vfs};
use std::collections::BTreeMap;

/// Cached meshes keyed by resolved path (lowercase).
#[derive(Clone, Debug, Default)]
pub struct ModelCatalog {
    /// Path → mesh.
    meshes: BTreeMap<String, ModelMesh>,
    /// Path → skinned IQM (`None` = not an IQM or unreadable).
    iqms: BTreeMap<String, Option<IqmModel>>,
}

impl ModelCatalog {
    /// Empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load (or return cached) mesh for `path` (MODELDEF `Path` + model file).
    pub fn load(&mut self, vfs: &Vfs, path: &str) -> Result<&ModelMesh, ModelError> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        if !self.meshes.contains_key(&key) {
            let mesh = if key.ends_with("_d.3d") || key.ends_with("_a.3d") {
                load_ue1_pair(vfs, &key)?
            } else {
                let data = read_model_bytes(vfs, &key)?;
                parse_model_bytes(&data, &key)?
            };
            self.meshes.insert(key.clone(), mesh);
        }
        Ok(self.meshes.get(&key).expect("inserted above"))
    }

    /// Load (or return cached) skinned IQM for `path`.
    pub fn load_iqm(&mut self, vfs: &Vfs, path: &str) -> Option<&IqmModel> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        if !key.ends_with(".iqm") {
            return None;
        }
        if !self.iqms.contains_key(&key) {
            let model = read_model_bytes(vfs, &key)
                .ok()
                .and_then(|data| parse_iqm_full(&data).ok());
            self.iqms.insert(key.clone(), model);
        }
        self.iqms.get(&key).and_then(Option::as_ref)
    }

    /// Cached mesh if already loaded.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&ModelMesh> {
        self.meshes
            .get(&path.replace('\\', "/").to_ascii_lowercase())
    }
}

/// Join MODELDEF `Path` + model filename.
#[must_use]
pub fn join_model_path(base: &str, file: &str) -> String {
    let base = base.replace('\\', "/").trim_end_matches('/').to_string();
    let file = file.replace('\\', "/");
    if base.is_empty() {
        file
    } else if file.starts_with('/') {
        file
    } else {
        format!("{base}/{file}")
    }
}

fn read_model_bytes(vfs: &Vfs, path: &str) -> Result<Vec<u8>, ModelError> {
    let name = path.rsplit('/').next().unwrap_or(path);
    for candidate in [path, name] {
        if let Some(id) = vfs
            .find(candidate, Namespace::Global)
            .or_else(|| vfs.find_any_ns(candidate))
        {
            let bytes = vfs
                .read(id)
                .map_err(|e| ModelError::Message(e.to_string()))?;
            return Ok(bytes.to_vec());
        }
    }
    Err(ModelError::Message(format!("model not in VFS: `{path}`")))
}

/// Load UE1 pair (`*_d.3d` + `*_a.3d`); returns frame 0 mesh.
fn load_ue1_pair(vfs: &Vfs, path: &str) -> Result<ModelMesh, ModelError> {
    let (d_path, a_path) = if path.ends_with("_d.3d") {
        (path.to_string(), path.replacen("_d.3d", "_a.3d", 1))
    } else if path.ends_with("_a.3d") {
        (path.replacen("_a.3d", "_d.3d", 1), path.to_string())
    } else {
        return Err(ModelError::Message("UE1 path must end _d.3d/_a.3d".into()));
    };
    let d = read_model_bytes(vfs, &d_path)?;
    let a = read_model_bytes(vfs, &a_path)?;
    let model = parse_ue1(&d, &a)?;
    model
        .frame_mesh(0)
        .cloned()
        .ok_or_else(|| ModelError::Message("UE1 has no frames".into()))
}

/// One actor pose for MODELDEF draw.
#[derive(Clone, Debug, Default)]
pub struct ModelActorPose {
    /// Class name (MODELDEF key).
    pub class: String,
    /// World origin (feet).
    pub origin: Vec3,
    /// Sector light 0..=1.
    pub light: f32,
    /// Facing in degrees (plus MODELDEF `AngleOffset`).
    pub angle_deg: f32,
    /// Current sprite and frame letter, for MODELDEF `FrameIndex`.
    pub sprite: String,
    /// Frame letter.
    pub frame: char,
    /// Map time in tics (fractional), for animations and bone overrides.
    pub tic: f64,
    /// `A_SetAnimation` request playing now.
    pub animation: Option<AnimRequest>,
    /// The animation before it and the tic of the switch.
    pub prev_animation: Option<(AnimRequest, f64)>,
    /// Bone overrides by index or name.
    pub bones: Vec<(Option<i32>, String, BoneOverride)>,
    /// `A_ChangeModel` animation source file (MODELDEF-relative path).
    pub animation_file: Option<String>,
}

/// `A_SetAnimation(name, framerate, startFrame, loopFrame, endFrame,
/// interpolateTics, flags)` as the playsim stored it (`-1` = default).
#[derive(Clone, Debug, PartialEq)]
pub struct AnimRequest {
    /// Animation name in the IQM.
    pub name: String,
    /// Frames per second (`< 0` = the animation's).
    pub framerate: f64,
    /// Start frame inside the animation (`< 0` = 0).
    pub start_frame: i32,
    /// Loop frame (`< 0` = the first).
    pub loop_frame: i32,
    /// End frame (`< 0` = the last).
    pub end_frame: i32,
    /// Blend-in tics from the previous animation (`< 0` = 1).
    pub interpolate_tics: i32,
    /// `SAF_*` (`SAF_INSTANT` 1, `SAF_LOOP` 2).
    pub flags: i32,
    /// Tic the animation started.
    pub start_tic: f64,
}

/// Resolve an [`AnimRequest`] against the model's animation list.
#[must_use]
pub fn resolve_anim(anims: &[IqmAnim], req: &AnimRequest) -> Option<ModelAnim> {
    let a = anims
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(&req.name))?;
    let first = a.first_frame as i32;
    let count = a.num_frames.max(1) as i32;
    let last = if req.end_frame >= 0 {
        first + req.end_frame.min(count - 1)
    } else {
        first + count - 1
    };
    let loop_frame = if req.loop_frame >= 0 {
        first + req.loop_frame.min(count - 1)
    } else {
        first
    };
    Some(ModelAnim {
        first_frame: first,
        last_frame: last,
        loop_frame,
        framerate: if req.framerate >= 0.0 {
            req.framerate as f32
        } else {
            a.framerate
        },
        start_frame: f64::from(req.start_frame.max(0)),
        // `SAF_LOOP`; without it the animation holds its last frame.
        looped: req.flags & 2 != 0,
        // UZDoom: unless `SAF_INSTANT`, the animation starts after the
        // `interpolateTics` blend out of the previous one.
        start_tic: if req.flags & 1 != 0 {
            req.start_tic
        } else {
            req.start_tic + f64::from(req.interpolate_tics.max(1))
        },
    })
}

/// The local pose of `model` for an actor: its `A_SetAnimation` (blended
/// out of the previous one over `interpolateTics`), else the MODELDEF frame
/// `keyframe`, then the bone overrides.
#[must_use]
pub fn actor_pose(
    model: &IqmModel,
    anim_source: &IqmModel,
    pose: &ModelActorPose,
    keyframe: Option<u32>,
) -> Vec<Trs> {
    let anims = &anim_source.anim.anims;
    let sample = |req: &AnimRequest, tic: f64| -> Option<Vec<Trs>> {
        let anim = resolve_anim(anims, req)?;
        let (f1, f2, t) = anim_frames(&anim, tic);
        Some(frame_pose(anim_source, f1, f2, t))
    };
    let mut local = match &pose.animation {
        Some(req) => {
            let now = sample(req, pose.tic);
            let blend_tics = if req.flags & 1 != 0 {
                0.0
            } else {
                f64::from(req.interpolate_tics.max(1))
            };
            match (&now, &pose.prev_animation) {
                (Some(now), Some((prev, switch)))
                    if blend_tics > 0.0 && pose.tic < switch + blend_tics =>
                {
                    let t = ((pose.tic - switch) / blend_tics).clamp(0.0, 1.0) as f32;
                    match sample(prev, *switch) {
                        Some(from) => blend_poses(&from, now, t),
                        None => now.clone(),
                    }
                }
                _ => now.unwrap_or_else(|| frame_pose(anim_source, 0, 0, 0.0)),
            }
        }
        None => {
            let k = keyframe.unwrap_or(0) as usize;
            frame_pose(anim_source, k, k, 0.0)
        }
    };
    if local.len() != model.joints.len() {
        local = frame_pose(model, 0, 0, 0.0);
    }
    let overrides: Vec<(usize, BoneOverride)> = pose
        .bones
        .iter()
        .filter_map(|(index, name, ov)| {
            let bone = match index {
                Some(i) => usize::try_from(*i).ok(),
                None => model.anim.joint(name),
            }?;
            Some((bone, *ov))
        })
        .collect();
    apply_overrides(&mut local, &overrides, pose.tic);
    local
}

/// The skinned mesh of `model` for `pose` (bind pose when unskinned).
#[must_use]
pub fn posed_mesh(
    model: &IqmModel,
    anim_source: &IqmModel,
    pose: &ModelActorPose,
    keyframe: Option<u32>,
) -> ModelMesh {
    if !model.is_skinned() {
        return model.mesh.clone();
    }
    let local = actor_pose(model, anim_source, pose, keyframe);
    let abs = absolute_matrices(model, &local);
    model.skin_cpu(&abs)
}

/// Append MODELDEF meshes for posed actors into the scene (unlit bootstrap).
pub fn append_actor_models(
    lists: &mut SceneLists,
    catalog: &mut ModelCatalog,
    vfs: &Vfs,
    defs: &ModelDefs,
    poses: &[ModelActorPose],
) {
    for pose in poses {
        let key = pose.class.to_ascii_lowercase();
        let Some(def) = defs.by_actor.get(&key) else {
            continue;
        };
        let Some(file) = def.models.get(&0) else {
            continue;
        };
        let full = join_model_path(&def.path, file);
        let scale = Vec3::from_array(def.scale);
        let origin =
            pose.origin + Vec3::new(def.offset[0], def.offset[1], def.offset[2] + def.z_offset);
        let skin = def
            .skins
            .get(&0)
            .cloned()
            .unwrap_or_else(|| "__MODEL".into());
        // Skinned IQM: animate, apply bone overrides, CPU-skin, face the actor.
        let keyframe = def
            .frames
            .iter()
            .find(|f| {
                f.sprite.eq_ignore_ascii_case(&pose.sprite)
                    && f.frame == pose.frame
                    && f.model_index == 0
            })
            .and_then(|f| f.keyframe);
        let anim_path = pose
            .animation_file
            .as_ref()
            .map(|file| join_model_path(&def.path, file));
        let posed = if let Some(model) = catalog.load_iqm(vfs, &full).cloned() {
            let source = anim_path
                .as_deref()
                .and_then(|p| catalog.load_iqm(vfs, p).cloned())
                .filter(|src| src.joints.len() == model.joints.len())
                .unwrap_or_else(|| model.clone());
            Some(posed_mesh(&model, &source, pose, keyframe))
        } else {
            None
        };
        if let Some(mesh) = posed {
            let m = glam::Mat4::from_translation(origin)
                * glam::Mat4::from_rotation_z((pose.angle_deg + def.angle_offset).to_radians())
                * glam::Mat4::from_scale(scale);
            let verts = mesh.to_wall_vertices_transformed(m, pose.light);
            if !verts.is_empty() {
                #[allow(clippy::cast_possible_truncation)]
                let start = lists.opaque_walls.len() as u32;
                #[allow(clippy::cast_possible_truncation)]
                let count = verts.len() as u32;
                lists.opaque_walls.extend_from_slice(&verts);
                lists.wall_ranges.push(crate::scene::MaterialRange {
                    name: skin,
                    start,
                    count,
                    flags: 0,
                });
            }
            continue;
        }
        let Ok(mesh) = catalog.load(vfs, &full) else {
            continue;
        };
        append_model(lists, mesh, origin, scale, pose.light, &skin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_paths() {
        assert_eq!(
            join_model_path("models/doom", "imp.md3"),
            "models/doom/imp.md3"
        );
        assert_eq!(join_model_path("", "x.obj"), "x.obj");
    }
}
