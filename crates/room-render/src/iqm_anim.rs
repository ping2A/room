//! IQM skeletal animation (UZ7, docs/36): joint names, animations and
//! decoded frames from the IQM file, UZDoom's frame / animation-switch
//! interpolation (`models_iqm.cpp` `CalculateBonesIQM`) and bone overrides
//! (`bonecomponents.h` `BoneOverride`), then the skinning matrices.

use crate::models::{IqmJoint, IqmModel};
use glam::{Mat4, Quat, Vec3};

/// One bone transform (UZDoom `TRS`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trs {
    /// Translation.
    pub translation: Vec3,
    /// Rotation.
    pub rotation: Quat,
    /// Scaling.
    pub scaling: Vec3,
}

impl Default for Trs {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scaling: Vec3::ONE,
        }
    }
}

impl Trs {
    /// Local matrix `T · R · S`.
    #[must_use]
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scaling, self.rotation, self.translation)
    }
}

/// An IQM animation (`iqmanim`).
#[derive(Clone, Debug, PartialEq)]
pub struct IqmAnim {
    /// Name.
    pub name: String,
    /// First frame in the model's frame list.
    pub first_frame: u32,
    /// Frame count.
    pub num_frames: u32,
    /// Frames per second.
    pub framerate: f32,
    /// `IQM_LOOP` (1).
    pub looped: bool,
}

/// Joint names, animations and frames of an IQM model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IqmAnimData {
    /// Joint names (same order as the joints).
    pub joint_names: Vec<String>,
    /// Animations.
    pub anims: Vec<IqmAnim>,
    /// Frame-major local bone transforms: `frames[f * joints + j]`.
    pub frames: Vec<Trs>,
}

impl IqmAnimData {
    /// Number of frames.
    #[must_use]
    pub fn frame_count(&self, joints: usize) -> usize {
        if joints == 0 {
            0
        } else {
            self.frames.len() / joints
        }
    }

    /// Animation by name (case-insensitive).
    #[must_use]
    pub fn anim(&self, name: &str) -> Option<&IqmAnim> {
        self.anims.iter().find(|a| a.name.eq_ignore_ascii_case(name))
    }

    /// Joint index by name (case-insensitive).
    #[must_use]
    pub fn joint(&self, name: &str) -> Option<usize> {
        self.joint_names.iter().position(|n| n.eq_ignore_ascii_case(name))
    }
}

fn u32_at(data: &[u8], o: usize) -> Option<u32> {
    data.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn f32_at(data: &[u8], o: usize) -> Option<f32> {
    u32_at(data, o).map(f32::from_bits)
}

fn text_at(data: &[u8], ofs_text: usize, num_text: usize, idx: u32) -> String {
    let start = ofs_text + idx as usize;
    if idx as usize >= num_text || start >= data.len() {
        return String::new();
    }
    let end = data[start..]
        .iter()
        .position(|&b| b == 0)
        .map_or(data.len(), |p| start + p);
    String::from_utf8_lossy(&data[start..end]).into_owned()
}

/// Parse joint names, poses, animations and frames from IQM v2 bytes.
#[must_use]
pub fn parse_iqm_anim_data(data: &[u8], joints: &[IqmJoint]) -> Option<IqmAnimData> {
    let num_text = u32_at(data, 28)? as usize;
    let ofs_text = u32_at(data, 32)? as usize;
    let num_joints = u32_at(data, 68)? as usize;
    let ofs_joints = u32_at(data, 72)? as usize;
    let num_poses = u32_at(data, 76)? as usize;
    let ofs_poses = u32_at(data, 80)? as usize;
    let num_anims = u32_at(data, 84)? as usize;
    let ofs_anims = u32_at(data, 88)? as usize;
    let num_frames = u32_at(data, 92)? as usize;
    let num_channels = u32_at(data, 96)? as usize;
    let ofs_frames = u32_at(data, 100)? as usize;
    let mut out = IqmAnimData::default();
    for j in 0..num_joints.min(joints.len()) {
        let name = u32_at(data, ofs_joints + j * 48)?;
        out.joint_names.push(text_at(data, ofs_text, num_text, name));
    }
    for a in 0..num_anims {
        let o = ofs_anims + a * 20;
        out.anims.push(IqmAnim {
            name: text_at(data, ofs_text, num_text, u32_at(data, o)?),
            first_frame: u32_at(data, o + 4)?,
            num_frames: u32_at(data, o + 8)?,
            framerate: f32_at(data, o + 12)?,
            looped: u32_at(data, o + 16)? & 1 != 0,
        });
    }
    if num_poses == 0 || num_frames == 0 || num_poses != joints.len() {
        return Some(out);
    }
    // iqmpose: parent, channelmask, channeloffset[10], channelscale[10].
    let mut poses = Vec::with_capacity(num_poses);
    for p in 0..num_poses {
        let o = ofs_poses + p * 88;
        let mask = u32_at(data, o + 4)?;
        let mut offset = [0f32; 10];
        let mut scale = [0f32; 10];
        for k in 0..10 {
            offset[k] = f32_at(data, o + 8 + k * 4)?;
            scale[k] = f32_at(data, o + 48 + k * 4)?;
        }
        poses.push((mask, offset, scale));
    }
    let needed = num_frames * num_channels * 2;
    let raw = data.get(ofs_frames..ofs_frames + needed)?;
    let mut cursor = 0usize;
    out.frames.reserve(num_frames * num_poses);
    for _ in 0..num_frames {
        for (mask, offset, scale) in &poses {
            let mut v = *offset;
            for k in 0..10 {
                if mask & (1 << k) != 0 {
                    if cursor + 2 > raw.len() {
                        return Some(out);
                    }
                    let q = u16::from_le_bytes([raw[cursor], raw[cursor + 1]]);
                    cursor += 2;
                    v[k] += f32::from(q) * scale[k];
                }
            }
            out.frames.push(Trs {
                translation: Vec3::new(v[0], v[1], v[2]),
                rotation: Quat::from_xyzw(v[3], v[4], v[5], v[6]).normalize(),
                scaling: Vec3::new(v[7], v[8], v[9]),
            });
        }
    }
    Some(out)
}

/// UZDoom `InterpolateQuat`: shortest-arc normalized lerp.
#[must_use]
pub fn interpolate_quat(from: Quat, to: Quat, t: f32) -> Quat {
    let invt = 1.0 - t;
    let mut rot = from * invt;
    if rot.dot(to * t) < 0.0 {
        rot = -rot;
    }
    (rot + to * t).normalize()
}

/// UZDoom `InterpolateBone`.
#[must_use]
pub fn interpolate_bone(from: &Trs, to: &Trs, t: f32) -> Trs {
    let invt = 1.0 - t;
    Trs {
        translation: from.translation * invt + to.translation * t,
        rotation: interpolate_quat(from.rotation, to.rotation, t),
        scaling: from.scaling * invt + to.scaling * t,
    }
}

/// Blend two whole poses (animation layers / animation switch).
#[must_use]
pub fn blend_poses(a: &[Trs], b: &[Trs], t: f32) -> Vec<Trs> {
    a.iter().zip(b).map(|(x, y)| interpolate_bone(x, y, t)).collect()
}

/// The pose between `frame1` and `frame2` (`inter` 0 = frame1).
#[must_use]
pub fn frame_pose(model: &IqmModel, frame1: usize, frame2: usize, inter: f32) -> Vec<Trs> {
    let n = model.joints.len();
    let count = model.anim.frame_count(n);
    if n == 0 || count == 0 {
        return model
            .joints
            .iter()
            .map(|j| Trs {
                translation: j.translate,
                rotation: j.rotate,
                scaling: j.scale,
            })
            .collect();
    }
    let f1 = frame1.min(count - 1) * n;
    let f2 = frame2.min(count - 1) * n;
    (0..n)
        .map(|i| interpolate_bone(&model.anim.frames[f1 + i], &model.anim.frames[f2 + i], inter))
        .collect()
}

/// Animation playback state (UZDoom `ModelAnim`), in model frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelAnim {
    /// First frame of the playing range.
    pub first_frame: i32,
    /// Last frame.
    pub last_frame: i32,
    /// Frame to wrap back to when looping.
    pub loop_frame: i32,
    /// Frames per second.
    pub framerate: f32,
    /// Frame offset the animation starts at.
    pub start_frame: f64,
    /// `MODELANIM_LOOP`.
    pub looped: bool,
    /// Tic the animation started (fractional).
    pub start_tic: f64,
}

/// `(frame1, frame2, inter)` of `anim` at `tic` (35 tics a second).
#[must_use]
pub fn anim_frames(anim: &ModelAnim, tic: f64) -> (usize, usize, f32) {
    let elapsed = ((tic - anim.start_tic).max(0.0) / 35.0) * f64::from(anim.framerate);
    let mut frame = f64::from(anim.first_frame) + anim.start_frame + elapsed;
    let last = f64::from(anim.last_frame.max(anim.first_frame));
    let loop_from = f64::from(anim.loop_frame.clamp(anim.first_frame, anim.last_frame.max(anim.first_frame)));
    if frame > last {
        if anim.looped {
            let span = last - loop_from + 1.0;
            frame = loop_from + (frame - loop_from).rem_euclid(span.max(1.0));
        } else {
            frame = last;
        }
    }
    let f1 = frame.floor();
    let inter = (frame - f1) as f32;
    let mut f2 = f1 + 1.0;
    if f2 > last {
        f2 = if anim.looped { loop_from } else { last };
    }
    (f1.max(0.0) as usize, f2.max(0.0) as usize, inter)
}

/// One component of a UZDoom bone override (`BoneOverrideComponent`):
/// `mode` 0 = none, 1 = add, 2 = replace, eased from `prev` / `prev_mode`
/// over `interp_len` tics from `switch_tic`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneComponent {
    /// Current mode.
    pub mode: i32,
    /// Mode being eased out of.
    pub prev_mode: i32,
    /// Tic of the last set.
    pub switch_tic: f64,
    /// Ease length in tics.
    pub interp_len: f64,
    /// Value being eased out of (xyzw / xyz).
    pub prev: [f32; 4],
    /// Value.
    pub cur: [f32; 4],
}

/// Bone override part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoneKind {
    /// Rotation (quaternion; add = multiply).
    Rotation,
    /// Translation (add = sum).
    Translation,
    /// Scaling (add = component product).
    Scaling,
}

impl BoneComponent {
    fn modify_value(kind: BoneKind, orig: [f32; 4], cur: [f32; 4], mode: i32) -> [f32; 4] {
        match mode {
            0 => orig,
            1 => match kind {
                BoneKind::Rotation => {
                    let q = (q_of(orig) * q_of(cur)).normalize();
                    [q.x, q.y, q.z, q.w]
                }
                BoneKind::Translation => [orig[0] + cur[0], orig[1] + cur[1], orig[2] + cur[2], 0.0],
                BoneKind::Scaling => [orig[0] * cur[0], orig[1] * cur[1], orig[2] * cur[2], 0.0],
            },
            _ => cur,
        }
    }

    fn lerp(kind: BoneKind, a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
        if kind == BoneKind::Rotation {
            let q = interpolate_quat(q_of(a), q_of(b), t);
            return [q.x, q.y, q.z, q.w];
        }
        let i = 1.0 - t;
        [a[0] * i + b[0] * t, a[1] * i + b[1] * t, a[2] * i + b[2] * t, 0.0]
    }

    /// UZDoom `BoneOverrideComponent::Modify`.
    #[must_use]
    pub fn modify(&self, kind: BoneKind, value: [f32; 4], tic: f64) -> [f32; 4] {
        let t = if self.interp_len > 0.0 {
            (((tic - self.switch_tic) / self.interp_len) as f32).clamp(0.0, 1.0)
        } else {
            1.0
        };
        if self.mode > 0 || (self.prev_mode > 0 && t < 1.0) {
            let from = Self::modify_value(kind, value, self.prev, self.prev_mode);
            let to = Self::modify_value(kind, value, self.cur, self.mode);
            Self::lerp(kind, from, to, t)
        } else {
            value
        }
    }
}

fn q_of(v: [f32; 4]) -> Quat {
    Quat::from_xyzw(v[0], v[1], v[2], v[3])
}

/// Bone overrides for one bone (rotation / translation / scaling).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoneOverride {
    /// Translation component.
    pub translation: Option<BoneComponent>,
    /// Rotation component.
    pub rotation: Option<BoneComponent>,
    /// Scaling component.
    pub scaling: Option<BoneComponent>,
}

impl BoneOverride {
    /// UZDoom `BoneOverride::Modify`.
    pub fn modify(&self, trs: &mut Trs, tic: f64) {
        if let Some(c) = &self.translation {
            let v = c.modify(BoneKind::Translation, [trs.translation.x, trs.translation.y, trs.translation.z, 0.0], tic);
            trs.translation = Vec3::new(v[0], v[1], v[2]);
        }
        if let Some(c) = &self.rotation {
            let r = trs.rotation;
            let v = c.modify(BoneKind::Rotation, [r.x, r.y, r.z, r.w], tic);
            trs.rotation = q_of(v);
        }
        if let Some(c) = &self.scaling {
            let v = c.modify(BoneKind::Scaling, [trs.scaling.x, trs.scaling.y, trs.scaling.z, 0.0], tic);
            trs.scaling = Vec3::new(v[0], v[1], v[2]);
        }
    }
}

/// Apply per-bone overrides (index = bone) to a pose.
pub fn apply_overrides(pose: &mut [Trs], overrides: &[(usize, BoneOverride)], tic: f64) {
    for (bone, ov) in overrides {
        if let Some(trs) = pose.get_mut(*bone) {
            ov.modify(trs, tic);
        }
    }
}

/// Absolute joint matrices of a local pose (parents before children).
#[must_use]
pub fn absolute_matrices(model: &IqmModel, pose: &[Trs]) -> Vec<Mat4> {
    let mut abs: Vec<Mat4> = Vec::with_capacity(pose.len());
    for (i, trs) in pose.iter().enumerate() {
        let local = trs.matrix();
        let parent = model.joints.get(i).map_or(-1, |j| j.parent);
        let m = usize::try_from(parent)
            .ok()
            .and_then(|p| abs.get(p).copied())
            .map_or(local, |p| p * local);
        abs.push(m);
    }
    abs
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn interpolate_quat_takes_the_short_arc() {
        let a = Quat::from_rotation_z(0.1);
        let b = -Quat::from_rotation_z(0.3); // same rotation, opposite sign
        let m = interpolate_quat(a, b, 0.5);
        // Same rotation as 0.2 rad about Z (either sign of the quaternion).
        let d = m.dot(Quat::from_rotation_z(0.2)).abs();
        assert!((d - 1.0).abs() < 1e-5, "{m:?}");
    }

    #[test]
    fn anim_frames_loop_and_hold() {
        let anim = ModelAnim {
            first_frame: 10,
            last_frame: 13,
            loop_frame: 11,
            framerate: 35.0,
            start_frame: 0.0,
            looped: true,
            start_tic: 0.0,
        };
        assert_eq!(anim_frames(&anim, 0.0), (10, 11, 0.0));
        assert_eq!(anim_frames(&anim, 3.0), (13, 11, 0.0));
        // 5 frames in: 15 → loops back over 11..=13 to 12.
        assert_eq!(anim_frames(&anim, 5.0).0, 12);
        let hold = ModelAnim { looped: false, ..anim };
        assert_eq!(anim_frames(&hold, 50.0), (13, 13, 0.0));
        let (f1, f2, t) = anim_frames(&anim, 1.5);
        assert_eq!((f1, f2), (11, 12));
        assert!((t - 0.5).abs() < 1e-6);
    }

    #[test]
    fn bone_add_mode_eases_in_and_replace_overrides() {
        let base = [0.0, 0.0, 0.0, 1.0];
        let quarter = {
            let q = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
            [q.x, q.y, q.z, q.w]
        };
        let c = BoneComponent {
            mode: 1,
            prev_mode: 0,
            switch_tic: 0.0,
            interp_len: 10.0,
            prev: base,
            cur: quarter,
        };
        // Halfway through the ease: an eighth of a turn.
        let half = q_of(c.modify(BoneKind::Rotation, base, 5.0));
        assert!((half.to_axis_angle().1 - std::f32::consts::FRAC_PI_4).abs() < 1e-3);
        let done = q_of(c.modify(BoneKind::Rotation, base, 20.0));
        assert!((done.to_axis_angle().1 - std::f32::consts::FRAC_PI_2).abs() < 1e-3);
        let t = BoneComponent {
            mode: 2,
            prev_mode: 0,
            switch_tic: 0.0,
            interp_len: 0.0,
            prev: [0.0; 4],
            cur: [4.0, 5.0, 6.0, 0.0],
        };
        assert_eq!(t.modify(BoneKind::Translation, [1.0, 1.0, 1.0, 0.0], 0.0), [4.0, 5.0, 6.0, 0.0]);
        let s = BoneComponent { mode: 1, ..t };
        assert_eq!(s.modify(BoneKind::Scaling, [2.0, 2.0, 2.0, 0.0], 0.0), [8.0, 10.0, 12.0, 0.0]);
    }
}
