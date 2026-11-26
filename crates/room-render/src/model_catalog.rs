//! MODELDEF path → mesh cache and actor wiring helpers (M10).

use crate::iqm_anim::{
    BoneOverride, IqmAnim, ModelAnim, Trs, absolute_matrices, anim_frames, apply_overrides,
    blend_poses, frame_pose,
};
use crate::models::{
    IqmModel, MD2_IDENT, MD3_IDENT, ModelError, ModelMesh, ModelVertex, parse_iqm_full, parse_md2,
    parse_md3, parse_model_bytes,
};
use crate::scene::SceneLists;
use crate::ue1::parse_ue1;
use glam::{Mat4, Vec3};
use room_defs::{ModelDef, ModelDefs, ModelFrameBind, mdl};
use room_vfs::{Namespace, Vfs};
use std::collections::BTreeMap;

/// Every animation frame of a vertex-animated model (MD2, MD3, UE1; OBJ
/// and other static formats have one).
#[derive(Clone, Debug, Default)]
pub struct FramedModel {
    /// One mesh per frame, same topology.
    pub frames: Vec<ModelMesh>,
    /// Frame names (MD2 / MD3), for MODELDEF `Frame`.
    pub names: Vec<String>,
}

impl FramedModel {
    /// Frame index of a MODELDEF binding (`FrameIndex` number or `Frame` name).
    #[must_use]
    pub fn frame_of(&self, bind: &ModelFrameBind) -> usize {
        if let Some(k) = bind.keyframe {
            return k as usize;
        }
        bind.frame_name
            .as_ref()
            .and_then(|n| self.names.iter().position(|m| m.eq_ignore_ascii_case(n)))
            .unwrap_or(0)
    }

    /// Frame `a` blended toward frame `b` by `t` (UZDoom model frame
    /// interpolation); frames out of range clamp to the last one.
    #[must_use]
    pub fn blended(&self, a: usize, b: Option<usize>, t: f32) -> Option<ModelMesh> {
        let last = self.frames.len().checked_sub(1)?;
        let fa = &self.frames[a.min(last)];
        let Some(fb) = b.map(|b| &self.frames[b.min(last)]) else {
            return Some(fa.clone());
        };
        if t <= 0.0 || fa.vertices.len() != fb.vertices.len() {
            return Some(fa.clone());
        }
        let lerp = |x: [f32; 3], y: [f32; 3]| {
            [
                x[0] + (y[0] - x[0]) * t,
                x[1] + (y[1] - x[1]) * t,
                x[2] + (y[2] - x[2]) * t,
            ]
        };
        Some(ModelMesh {
            vertices: fa
                .vertices
                .iter()
                .zip(&fb.vertices)
                .map(|(va, vb)| ModelVertex {
                    pos: lerp(va.pos, vb.pos),
                    uv: va.uv,
                    normal: lerp(va.normal, vb.normal),
                })
                .collect(),
            indices: fa.indices.clone(),
        })
    }
}

/// Cached meshes keyed by resolved path (lowercase).
#[derive(Clone, Debug, Default)]
pub struct ModelCatalog {
    /// Path → mesh.
    meshes: BTreeMap<String, ModelMesh>,
    /// Path → skinned IQM (`None` = not an IQM or unreadable).
    iqms: BTreeMap<String, Option<IqmModel>>,
    /// Path → all frames (`None` = unreadable).
    framed: BTreeMap<String, Option<FramedModel>>,
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
            return Ok(self.meshes.entry(key).or_insert(mesh));
        }
        self.meshes
            .get(&key)
            .ok_or_else(|| ModelError::Message(format!("model {key} vanished from cache")))
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

    /// Load (or return cached) every frame of a vertex-animated model.
    pub fn load_frames(&mut self, vfs: &Vfs, path: &str) -> Option<&FramedModel> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        if !self.framed.contains_key(&key) {
            let model = load_framed(vfs, &key).ok();
            self.framed.insert(key.clone(), model);
        }
        self.framed.get(&key).and_then(Option::as_ref)
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
    if base.is_empty() || file.starts_with('/') {
        file
    } else {
        format!("{base}/{file}")
    }
}

fn read_model_bytes(vfs: &Vfs, path: &str) -> Result<Vec<u8>, ModelError> {
    if let Some(id) = vfs.find_full(path) {
        let bytes = vfs
            .read(id)
            .map_err(|e| ModelError::Message(e.to_string()))?;
        return Ok(bytes.to_vec());
    }
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

fn load_framed(vfs: &Vfs, path: &str) -> Result<FramedModel, ModelError> {
    if path.ends_with("_d.3d") || path.ends_with("_a.3d") {
        let (d, a) = if path.ends_with("_d.3d") {
            (path.to_string(), path.replacen("_d.3d", "_a.3d", 1))
        } else {
            (path.replacen("_a.3d", "_d.3d", 1), path.to_string())
        };
        let model = parse_ue1(&read_model_bytes(vfs, &d)?, &read_model_bytes(vfs, &a)?)?;
        return Ok(FramedModel {
            frames: model.frames,
            names: Vec::new(),
        });
    }
    let data = read_model_bytes(vfs, path)?;
    let ident = data
        .get(0..4)
        .map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if ident == MD2_IDENT {
        let m = parse_md2(&data)?;
        return Ok(FramedModel {
            frames: m.frames,
            names: m.frame_names,
        });
    }
    if ident == MD3_IDENT {
        let m = parse_md3(&data)?;
        return Ok(FramedModel {
            frames: m.frames,
            names: m.frame_names,
        });
    }
    Ok(FramedModel {
        frames: vec![parse_model_bytes(&data, path)?],
        names: Vec::new(),
    })
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
    /// Actor pitch, degrees (positive looks down).
    pub pitch_deg: f32,
    /// Actor roll, degrees.
    pub roll_deg: f32,
    /// Actor velocity (for `PITCHFROMMOMENTUM`).
    pub vel: Vec3,
    /// Actor `Scale` (x, y); zero means 1.
    pub scale: [f32; 2],
    /// Next state's sprite and frame letter, and the blend toward it
    /// (`room_sim::model_frame_lerp`); `None` = no frame interpolation.
    pub next: Option<(String, char, f32)>,
    /// `A_SetModelFlag` flags replacing the block's (`None` = MODELDEF's).
    pub flags: Option<u32>,
    /// Time in tics for `ROTATING` (wall clock, like UZDoom's `I_GetTime`).
    pub clock_tic: f64,
}

/// True when `class` is drawn as a model in this sprite frame (otherwise the
/// shell keeps drawing its sprite, as UZDoom does).
#[must_use]
pub fn actor_has_model(defs: &ModelDefs, class: &str, sprite: &str, frame: char) -> bool {
    defs.frame(class, sprite, frame).is_some()
}

/// UZDoom `RenderModel`'s object-to-world matrix for one MODELDEF block,
/// built in its Y-up render axes and returned in Room's Z-up space (model
/// vertices are Z-up, as Room loads them).
#[must_use]
pub fn model_matrix(def: &ModelDef, pose: &ModelActorPose, flags: u32) -> Mat4 {
    let actor_scale = |v: f32| if v.abs() < 1.0e-6 { 1.0 } else { v };
    let (asx, asy) = (actor_scale(pose.scale[0]), actor_scale(pose.scale[1]));
    let safe = |v: f32| if v.abs() < 1.0e-6 { 1.0 } else { v };
    let (dsx, dsy, dsz) = (safe(def.scale[0]), safe(def.scale[1]), safe(def.scale[2]));
    let angle = pose.angle_deg;
    let mut pitch = 0.0f32;
    if flags & mdl::PITCHFROMMOMENTUM != 0 {
        let v = pose.vel;
        let h = (v.x * v.x + v.y * v.y).sqrt();
        if v.length_squared() > 1.0e-8 {
            pitch = (v.z / h.max(1.0e-6)).atan().to_degrees();
            if h > 1.0e-6 {
                let a = angle.to_radians();
                if (v.x * a.cos() + v.y * a.sin()) / h < 0.0 {
                    pitch = -pitch;
                }
            } else {
                pitch = pitch.abs();
            }
        }
    }
    let mut roll = 0.0f32;
    let bad = flags & mdl::BADROTATION != 0;
    if flags & mdl::USEACTORPITCH != 0 {
        pitch += if bad { pose.pitch_deg } else { -pose.pitch_deg };
    }
    if flags & mdl::USEACTORROLL != 0 {
        roll += if bad { pose.roll_deg } else { -pose.roll_deg };
    }
    let rotate_offset = if flags & mdl::ROTATING != 0 && def.rotation_speed.abs() > 1.0e-10 {
        let turns = pose.clock_tic / (200.0 / f64::from(def.rotation_speed));
        #[allow(clippy::cast_possible_truncation)]
        let deg = ((turns - turns.floor()) * 360.0) as f32;
        deg
    } else {
        0.0
    };
    let [rcx, rcy, rcz] = def.rotation_center;
    let o = pose.origin;
    let rad = f32::to_radians;
    // Render axes: x, up, Room y.
    let mut m = Mat4::from_translation(Vec3::new(o.x, o.z, o.y));
    let use_center = flags & mdl::USEROTATIONCENTER != 0;
    if use_center {
        m *= Mat4::from_translation(Vec3::new(rcx, rcz, rcy));
    }
    m *= Mat4::from_rotation_y(rad(-angle));
    m *= Mat4::from_rotation_z(rad(pitch));
    m *= Mat4::from_rotation_x(rad(-roll));
    if use_center {
        m *= Mat4::from_translation(Vec3::new(-rcx, -rcz, -rcy));
    }
    if rotate_offset != 0.0 {
        let axis = Vec3::from_array(def.rotation_vector)
            .try_normalize()
            .unwrap_or(Vec3::Y);
        m *= Mat4::from_translation(Vec3::new(rcx, rcy, rcz))
            * Mat4::from_axis_angle(axis, rad(rotate_offset))
            * Mat4::from_translation(Vec3::new(-rcx, -rcy, -rcz));
    }
    m *= Mat4::from_scale(Vec3::new(asx * dsx, asy * dsz, asx * dsy));
    m *= Mat4::from_translation(Vec3::new(
        def.offset[0] / dsx,
        (def.offset[2] + def.z_offset) / dsz,
        def.offset[1] / dsy,
    ));
    m *= Mat4::from_rotation_y(rad(-def.angle_offset));
    m *= Mat4::from_rotation_z(rad(def.pitch_offset));
    m *= Mat4::from_rotation_x(rad(-def.roll_offset));
    let to_render = Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, // room X → render X
        0.0, 0.0, 1.0, 0.0, // room Y → render Z
        0.0, 1.0, 0.0, 0.0, // room Z → render Y
        0.0, 0.0, 0.0, 1.0,
    ]);
    to_render.transpose() * m * to_render
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
    actor_pose_lerp(model, anim_source, pose, keyframe, None, 0.0)
}

/// [`actor_pose`] with the MODELDEF keyframe blended toward `next` by `t`.
#[must_use]
pub fn actor_pose_lerp(
    model: &IqmModel,
    anim_source: &IqmModel,
    pose: &ModelActorPose,
    keyframe: Option<u32>,
    next: Option<u32>,
    t: f32,
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
            match next {
                Some(n) if t > 0.0 => frame_pose(anim_source, k, n as usize, t),
                _ => frame_pose(anim_source, k, k, 0.0),
            }
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

/// Append MODELDEF meshes for posed actors: every model slot the current
/// sprite frame binds (UZDoom `FindModelFrame`, later blocks first), with its
/// skin, keyframe blended toward the next state's frame, and UZDoom's model
/// matrix (actor angle / pitch / roll, `MDL_*` flags, scale and offsets).
/// Actors whose frame has no binding are skipped: the shell draws their
/// sprite ([`actor_has_model`]).
pub fn append_actor_models(
    lists: &mut SceneLists,
    catalog: &mut ModelCatalog,
    vfs: &Vfs,
    defs: &ModelDefs,
    poses: &[ModelActorPose],
) {
    for pose in poses {
        let Some((def, binds)) = defs.frame(&pose.class, &pose.sprite, pose.frame) else {
            continue;
        };
        let flags = pose.flags.unwrap_or_else(|| def.mdl_flags());
        // The next state's binding in the same block, per model slot.
        let next_binds: Vec<&ModelFrameBind> = match &pose.next {
            Some((sprite, frame, t)) if *t > 0.0 && flags & mdl::NOINTERPOLATION == 0 => defs
                .frame(&pose.class, sprite, *frame)
                .filter(|(d, _)| std::ptr::eq(*d, def))
                .map(|(_, b)| b)
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let inter = pose.next.as_ref().map_or(0.0, |n| n.2);
        let matrix = model_matrix(def, pose, flags);
        for bind in binds {
            let Some(file) = def.models.get(&bind.model_index) else {
                continue;
            };
            let full = join_model_path(&def.path, file);
            let skin = def
                .skins
                .get(&bind.model_index)
                .or_else(|| def.surface_skins.get(&(bind.model_index, 0)))
                .map_or_else(|| "__MODEL".to_owned(), |s| join_model_path(&def.path, s));
            let next = next_binds
                .iter()
                .find(|b| b.model_index == bind.model_index)
                .copied();
            let mesh = if let Some(model) = catalog.load_iqm(vfs, &full).cloned() {
                let anim_path = pose
                    .animation_file
                    .as_ref()
                    .map(|file| join_model_path(&def.path, file));
                let source = anim_path
                    .as_deref()
                    .and_then(|p| catalog.load_iqm(vfs, p).cloned())
                    .filter(|src| src.joints.len() == model.joints.len())
                    .unwrap_or_else(|| model.clone());
                if model.is_skinned() {
                    let local = actor_pose_lerp(
                        &model,
                        &source,
                        pose,
                        bind.keyframe,
                        next.and_then(|n| n.keyframe),
                        inter,
                    );
                    let abs = absolute_matrices(&model, &local);
                    Some(model.skin_cpu(&abs))
                } else {
                    Some(model.mesh.clone())
                }
            } else {
                catalog.load_frames(vfs, &full).and_then(|fm| {
                    let a = fm.frame_of(bind);
                    let b = next.map(|n| fm.frame_of(n));
                    fm.blended(a, b, inter)
                })
            };
            let Some(mesh) = mesh else {
                continue;
            };
            let verts = mesh.to_wall_vertices_transformed(matrix, pose.light);
            if verts.is_empty() {
                continue;
            }
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use room_defs::parse_modeldef;
    use room_vfs::VfsBuilder;

    const TRI: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nf 1/1 2/2 3/3\n";

    fn vfs() -> Vfs {
        let zip = room_vfs::zip_from_files(&[
            ("models/imp/body.obj", TRI.as_bytes()),
            ("models/imp/ball.obj", TRI.as_bytes()),
        ])
        .unwrap();
        VfsBuilder::new()
            .mount_memory("m.pk3", zip)
            .unwrap()
            .build()
            .unwrap()
    }

    fn pose(sprite: &str, frame: char) -> ModelActorPose {
        ModelActorPose {
            class: "Imp".into(),
            sprite: sprite.into(),
            frame,
            light: 1.0,
            ..ModelActorPose::default()
        }
    }

    #[test]
    fn frames_pick_blocks_slots_and_skins() {
        let defs = parse_modeldef(
            r#"
Model Imp { Path "models/imp" Model 0 "body.obj" Skin 0 "body.png" FrameIndex TROO A 0 0 }
Model Imp { Path "models/imp" Model 0 "body.obj" Model 1 "ball.obj" Skin 1 "ball.png"
            FrameIndex TROO E 0 0 FrameIndex TROO E 1 0 }
"#,
        )
        .unwrap();
        let vfs = vfs();
        let mut cat = ModelCatalog::new();
        let mut lists = SceneLists::default();
        // TROO B has no binding: the sprite is drawn instead.
        assert!(!actor_has_model(&defs, "imp", "TROO", 'B'));
        append_actor_models(&mut lists, &mut cat, &vfs, &defs, &[pose("TROO", 'B')]);
        assert!(lists.wall_ranges.is_empty());
        append_actor_models(&mut lists, &mut cat, &vfs, &defs, &[pose("TROO", 'A')]);
        assert_eq!(lists.wall_ranges.len(), 1);
        assert_eq!(lists.wall_ranges[0].name, "models/imp/body.png");
        append_actor_models(&mut lists, &mut cat, &vfs, &defs, &[pose("TROO", 'E')]);
        let names: Vec<&str> = lists.wall_ranges.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            ["models/imp/body.png", "__MODEL", "models/imp/ball.png"]
        );
        assert_eq!(lists.opaque_walls.len(), 9);
    }

    fn def() -> ModelDef {
        ModelDef {
            scale: [1.0; 3],
            rotation_speed: 1.0,
            rotation_vector: [0.0, 1.0, 0.0],
            ..ModelDef::default()
        }
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1.0e-4
    }

    #[test]
    fn model_matrix_follows_uzdoom() {
        let d = def();
        let mut p = ModelActorPose {
            origin: Vec3::new(100.0, 50.0, 8.0),
            angle_deg: 90.0,
            ..ModelActorPose::default()
        };
        let m = model_matrix(&d, &p, 0);
        // Facing north: model +X points along world +Y, up stays up.
        assert!(close(
            m.transform_point3(Vec3::X),
            Vec3::new(100.0, 51.0, 8.0)
        ));
        assert!(close(
            m.transform_point3(Vec3::Z),
            Vec3::new(100.0, 50.0, 9.0)
        ));
        // Offsets turn with the model and scale with the actor.
        let mut d2 = def();
        d2.offset = [4.0, 0.0, 0.0];
        d2.z_offset = 2.0;
        p.scale = [2.0, 2.0];
        let m = model_matrix(&d2, &p, 0);
        assert!(close(
            m.transform_point3(Vec3::ZERO),
            Vec3::new(100.0, 58.0, 12.0)
        ));
        // PITCHFROMMOMENTUM: climbing tilts the nose up.
        let climb = ModelActorPose {
            vel: Vec3::new(1.0, 0.0, 1.0),
            ..ModelActorPose::default()
        };
        let m = model_matrix(&def(), &climb, mdl::PITCHFROMMOMENTUM);
        let nose = m.transform_point3(Vec3::X);
        assert!(nose.z > 0.7 && nose.x > 0.7, "{nose}");
        // USEACTORPITCH: positive actor pitch (looking down) tilts it down.
        let down = ModelActorPose {
            pitch_deg: 30.0,
            ..ModelActorPose::default()
        };
        let nose = model_matrix(&def(), &down, mdl::USEACTORPITCH).transform_point3(Vec3::X);
        assert!(nose.z < -0.4, "{nose}");
        // ROTATING: a quarter of 200 tics turns a quarter about the up axis.
        let spin = ModelActorPose {
            clock_tic: 50.0,
            ..ModelActorPose::default()
        };
        let nose = model_matrix(&def(), &spin, mdl::ROTATING).transform_point3(Vec3::X);
        assert!(nose.z.abs() < 1.0e-4 && (nose.x.abs() < 1.0e-4), "{nose}");
    }

    #[test]
    fn framed_models_blend_toward_the_next_frame() {
        let f = |x: f32| ModelMesh {
            vertices: vec![ModelVertex {
                pos: [x, 0.0, 0.0],
                uv: [0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            }],
            indices: vec![0],
        };
        let fm = FramedModel {
            frames: vec![f(0.0), f(10.0)],
            names: vec!["stand".into(), "walk".into()],
        };
        assert!((fm.blended(0, Some(1), 0.25).unwrap().vertices[0].pos[0] - 2.5).abs() < 1e-5);
        assert_eq!(fm.blended(0, None, 0.5).unwrap().vertices[0].pos[0], 0.0);
        let named = ModelFrameBind {
            sprite: "TROO".into(),
            frame: 'A',
            model_index: 0,
            keyframe: None,
            frame_name: Some("WALK".into()),
        };
        assert_eq!(fm.frame_of(&named), 1);
    }

    #[test]
    fn join_paths() {
        assert_eq!(
            join_model_path("models/doom", "imp.md3"),
            "models/doom/imp.md3"
        );
        assert_eq!(join_model_path("", "x.obj"), "x.obj");
    }
}
