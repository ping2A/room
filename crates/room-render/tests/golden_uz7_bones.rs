//! UZ7 (docs/36): IQM animations, animation-switch blending and bone
//! overrides on a synthetic two-joint IQM written by this test.
//!
//! - `uz7_blend`: a pose between two frames is the per-bone lerp / slerp of
//!   those frames (UZDoom `InterpolateBone`).
//! - Golden `testdata/goldens/uz7_bones.txt`: the skinned tip vertex of the
//!   arm while an animation plays, during the blend out of another, and
//!   with `SB_ADD` / `SB_REPLACE` bone overrides easing in.
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-render --test golden_uz7_bones`

#![allow(clippy::unwrap_used)]

use glam::{Quat, Vec3};
use room_render::{
    AnimRequest, BoneComponent, BoneOverride, ModelActorPose, Trs, frame_pose, interpolate_bone,
    parse_iqm_full, posed_mesh,
};
use std::fmt::Write as _;

/// IQM v2 with a root joint at the origin and an "arm" joint one unit up
/// the X axis, one triangle skinned to the arm, and two animations:
/// `swing` (frames 0–2, the arm turning 0° / 45° / 90° about Z) and `lift`
/// (frames 3–4, the arm moving up Z by 0 / 2).
fn iqm_bytes() -> Vec<u8> {
    let mut text = vec![0u8];
    let mut name = |s: &str| {
        let at = text.len() as u32;
        text.extend_from_slice(s.as_bytes());
        text.push(0);
        at
    };
    let (n_root, n_arm, n_swing, n_lift) = (name("root"), name("arm"), name("swing"), name("lift"));
    // Vertices: three at the arm tip, all on joint 1.
    let positions: [[f32; 3]; 3] = [[2.0, 0.0, 0.0], [2.0, 0.5, 0.0], [2.0, 0.0, 0.5]];
    let blend_idx = [[1u8, 0, 0, 0]; 3];
    let blend_w = [[255u8, 0, 0, 0]; 3];
    // Frames: per joint 10 channels (t xyz, r xyzw, s xyz), all animated.
    let frame = |rz: f32, tz: f32| -> [[f32; 10]; 2] {
        let r = Quat::from_rotation_z(rz);
        [
            [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            [1.0, 0.0, tz, r.x, r.y, r.z, r.w, 1.0, 1.0, 1.0],
        ]
    };
    let q = std::f32::consts::FRAC_PI_4;
    let frames = [
        frame(0.0, 0.0),
        frame(q, 0.0),
        frame(2.0 * q, 0.0),
        frame(0.0, 0.0),
        frame(0.0, 2.0),
    ];
    // Quantize: offset = -4, scale = 8 / 65535 per channel.
    let (off, scale) = (-4.0f32, 8.0f32 / 65535.0);

    let mut out = vec![0u8; 124];
    let mut put = |buf: &mut Vec<u8>, bytes: &[u8]| -> u32 {
        let at = buf.len() as u32;
        buf.extend_from_slice(bytes);
        at
    };
    let ofs_text = put(&mut out, &text);
    // Vertex data.
    let pos_bytes: Vec<u8> = positions
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let ofs_pos = put(&mut out, &pos_bytes);
    let ofs_bi = put(&mut out, &blend_idx.concat());
    let ofs_bw = put(&mut out, &blend_w.concat());
    let mut va = Vec::new();
    for (ty, fmt, size, ofs) in [
        (0u32, 7u32, 3u32, ofs_pos),
        (4, 1, 4, ofs_bi),
        (5, 1, 4, ofs_bw),
    ] {
        for v in [ty, 0, fmt, size, ofs] {
            va.extend_from_slice(&v.to_le_bytes());
        }
    }
    let ofs_va = put(&mut out, &va);
    let tri: Vec<u8> = [0u32, 1, 2].iter().flat_map(|v| v.to_le_bytes()).collect();
    let ofs_tri = put(&mut out, &tri);
    // Joints: name, parent, t, r, s (bind: root at origin, arm at x = 1).
    let mut joints = Vec::new();
    for (nm, parent, t) in [(n_root, -1i32, 0.0f32), (n_arm, 0, 1.0)] {
        joints.extend_from_slice(&nm.to_le_bytes());
        joints.extend_from_slice(&parent.to_le_bytes());
        for v in [t, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0] {
            joints.extend_from_slice(&v.to_le_bytes());
        }
    }
    let ofs_joints = put(&mut out, &joints);
    let mut poses = Vec::new();
    for parent in [-1i32, 0] {
        poses.extend_from_slice(&parent.to_le_bytes());
        poses.extend_from_slice(&0x3ffu32.to_le_bytes());
        for _ in 0..10 {
            poses.extend_from_slice(&off.to_le_bytes());
        }
        for _ in 0..10 {
            poses.extend_from_slice(&scale.to_le_bytes());
        }
    }
    let ofs_poses = put(&mut out, &poses);
    let mut anims = Vec::new();
    for (nm, first, count, rate) in [(n_swing, 0u32, 3u32, 35.0f32), (n_lift, 3, 2, 35.0)] {
        anims.extend_from_slice(&nm.to_le_bytes());
        anims.extend_from_slice(&first.to_le_bytes());
        anims.extend_from_slice(&count.to_le_bytes());
        anims.extend_from_slice(&rate.to_le_bytes());
        anims.extend_from_slice(&0u32.to_le_bytes());
    }
    let ofs_anims = put(&mut out, &anims);
    let mut fdata = Vec::new();
    for f in &frames {
        for joint in f {
            for v in joint {
                let qv = ((v - off) / scale).round().clamp(0.0, 65535.0) as u16;
                fdata.extend_from_slice(&qv.to_le_bytes());
            }
        }
    }
    let ofs_frames = put(&mut out, &fdata);
    let filesize = out.len() as u32;
    out[..16].copy_from_slice(b"INTERQUAKEMODEL\0");
    let header: [(usize, u32); 22] = [
        (16, 2),
        (20, filesize),
        (28, text.len() as u32),
        (32, ofs_text),
        (44, 3),
        (48, 3),
        (52, ofs_va),
        (56, 1),
        (60, ofs_tri),
        (68, 2),
        (72, ofs_joints),
        (76, 2),
        (80, ofs_poses),
        (84, 2),
        (88, ofs_anims),
        (92, frames.len() as u32),
        (96, 20),
        (100, ofs_frames),
        (36, 0),
        (40, 0),
        (64, 0),
        (104, 0),
    ];
    for (at, v) in header {
        out[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

fn tip(model: &room_render::models::IqmModel, pose: &ModelActorPose) -> Vec3 {
    let mesh = posed_mesh(model, model, pose, None);
    Vec3::from_array(mesh.vertices[0].pos)
}

fn req(name: &str, start_tic: f64, interp: i32, flags: i32) -> AnimRequest {
    AnimRequest {
        name: name.into(),
        framerate: -1.0,
        start_frame: -1,
        loop_frame: -1,
        end_frame: -1,
        interpolate_tics: interp,
        flags,
        start_tic,
    }
}

#[test]
fn uz7_parses_names_animations_and_frames() {
    let m = parse_iqm_full(&iqm_bytes()).unwrap();
    assert_eq!(m.anim.joint_names, vec!["root", "arm"]);
    assert_eq!(m.anim.anims.len(), 2);
    assert_eq!(m.anim.anim("LIFT").unwrap().first_frame, 3);
    assert_eq!(m.anim.frame_count(2), 5);
    assert!(m.is_skinned());
}

#[test]
fn uz7_blend_is_the_lerp_of_frames() {
    let m = parse_iqm_full(&iqm_bytes()).unwrap();
    let a = frame_pose(&m, 0, 0, 0.0);
    let b = frame_pose(&m, 2, 2, 0.0);
    let mid = frame_pose(&m, 0, 2, 0.5);
    for j in 0..2 {
        let want: Trs = interpolate_bone(&a[j], &b[j], 0.5);
        assert!((mid[j].translation - want.translation).length() < 1e-5);
        assert!(mid[j].rotation.dot(want.rotation).abs() > 1.0 - 1e-5);
    }
    // Halfway between 0° and 90°: the arm at 45°.
    let angle = mid[1].rotation.to_axis_angle().1;
    assert!(
        (angle - std::f32::consts::FRAC_PI_4).abs() < 1e-3,
        "{angle}"
    );
}

#[test]
fn uz7_bones_match_golden() {
    let m = parse_iqm_full(&iqm_bytes()).unwrap();
    let mut out = String::new();
    let mut line = |label: &str, p: Vec3| {
        let z = |v: f32| if v.abs() < 5e-4 { 0.0 } else { v };
        let _ = writeln!(
            out,
            "{label:<34} {:>7.3} {:>7.3} {:>7.3}",
            z(p.x),
            z(p.y),
            z(p.z)
        );
    };
    let base = ModelActorPose::default();
    line("bind / no animation", tip(&m, &base));
    // `swing` at 35 fps, 1 frame a tic, instant start at tic 0.
    for t in [0.0, 1.0, 1.5, 2.0] {
        let p = ModelActorPose {
            tic: t,
            animation: Some(req("swing", 0.0, -1, 1)),
            ..ModelActorPose::default()
        };
        line(&format!("swing tic {t}"), tip(&m, &p));
    }
    // Switch from `swing` (frozen at its 90° frame) to `lift` at tic 10 with
    // a 4-tic blend.
    for t in [10.0, 12.0, 14.0, 15.0] {
        let p = ModelActorPose {
            tic: t,
            animation: Some(req("lift", 10.0, 4, 0)),
            prev_animation: Some((req("swing", 0.0, -1, 1), 10.0)),
            ..ModelActorPose::default()
        };
        line(&format!("swing->lift tic {t}"), tip(&m, &p));
    }
    // Bone overrides on the arm (joint 1, by name): add a quarter turn about
    // Z eased over 10 tics, then replace the translation.
    let quarter = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    let rot = BoneComponent {
        mode: 1,
        prev_mode: 0,
        switch_tic: 0.0,
        interp_len: 10.0,
        prev: [0.0, 0.0, 0.0, 1.0],
        cur: [quarter.x, quarter.y, quarter.z, quarter.w],
    };
    for t in [0.0, 5.0, 10.0] {
        let p = ModelActorPose {
            tic: t,
            bones: vec![(
                None,
                "arm".into(),
                BoneOverride {
                    rotation: Some(rot),
                    ..BoneOverride::default()
                },
            )],
            ..ModelActorPose::default()
        };
        line(&format!("add rot arm tic {t}"), tip(&m, &p));
    }
    let tr = BoneComponent {
        mode: 2,
        prev_mode: 0,
        switch_tic: 0.0,
        interp_len: 0.0,
        prev: [0.0; 4],
        cur: [0.0, 3.0, 0.0, 0.0],
    };
    let p = ModelActorPose {
        tic: 1.0,
        bones: vec![(
            Some(1),
            String::new(),
            BoneOverride {
                translation: Some(tr),
                ..BoneOverride::default()
            },
        )],
        ..ModelActorPose::default()
    };
    line("replace translation arm", tip(&m, &p));
    // Spot checks: bind tip at x = 2; a quarter turn puts it at (1, 1).
    assert!(
        out.contains("bind / no animation                  2.000   0.000   0.000"),
        "{out}"
    );
    assert!(
        out.contains("add rot arm tic 10                   1.000   1.000   0.000"),
        "{out}"
    );
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/goldens/uz7_bones.txt");
    if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &out).unwrap();
    }
    assert_eq!(out, std::fs::read_to_string(&path).unwrap());
}
