//! Offscreen smoke: IQM GPU skinning pipeline (skipped without GPU).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render::{
    Camera, GpuContext, IqmJoint, IqmModel, ModelMesh, ModelVertex, OffscreenTarget, SkinnedPass,
    iqm_to_skinned_vertices, read_rgba, skin_matrices,
};
use glam::{Mat4, Quat, Vec3};

#[test]
fn skinned_iqm_translates_on_gpu() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping skinned_iqm_translates_on_gpu: no wgpu adapter");
            return;
        }
    };
    let iqm = IqmModel {
        mesh: ModelMesh {
            vertices: vec![
                ModelVertex {
                    pos: [-16.0, 0.0, -16.0],
                    uv: [0.0, 0.0],
                    normal: [0.0, -1.0, 0.0],
                },
                ModelVertex {
                    pos: [16.0, 0.0, -16.0],
                    uv: [1.0, 0.0],
                    normal: [0.0, -1.0, 0.0],
                },
                ModelVertex {
                    pos: [0.0, 0.0, 16.0],
                    uv: [0.5, 1.0],
                    normal: [0.0, -1.0, 0.0],
                },
            ],
            indices: vec![0, 1, 2],
        },
        joints: vec![IqmJoint {
            parent: -1,
            translate: Vec3::ZERO,
            rotate: Quat::IDENTITY,
            scale: Vec3::ONE,
        }],
        base_mats: vec![Mat4::IDENTITY],
        inv_base_mats: vec![Mat4::IDENTITY],
        blend_indexes: vec![[0, 0, 0, 0]; 3],
        blend_weights: vec![[255, 0, 0, 0]; 3],
    };
    let origin = Vec3::new(0.0, 80.0, 0.0);
    let verts = iqm_to_skinned_vertices(&iqm, origin, Vec3::ONE, 1.0);
    assert_eq!(verts.len(), 3);
    // Translate joint +80 in Y so the triangle sits in front of the camera.
    let posed = [Mat4::from_translation(Vec3::new(0.0, 0.0, 40.0))];
    let mats = skin_matrices(&iqm, &posed);

    let camera = Camera {
        eye: Vec3::new(0.0, 0.0, 48.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    let target = OffscreenTarget::new(&gpu, 160, 120);
    let mut pass = SkinnedPass::new(&gpu, target.format);
    pass.draw(
        &gpu,
        &target.color_view,
        &target.depth_view,
        &camera,
        &verts,
        &mats,
        true,
    );
    let rgba = read_rgba(&gpu, &target).expect("readback");
    let clear = [13u8, 13, 20]; // approx — we clear to 0.05,0.05,0.08
    let differs = rgba.chunks_exact(4).any(|p| {
        (i16::from(p[0]) - i16::from(clear[0])).abs() > 20
            || (i16::from(p[1]) - i16::from(clear[1])).abs() > 20
            || (i16::from(p[2]) - i16::from(clear[2])).abs() > 20
    });
    // Clear is ~13,13,20; white lit tris should be much brighter somewhere.
    let bright = rgba
        .chunks_exact(4)
        .any(|p| p[0] > 80 && p[1] > 80 && p[2] > 80);
    assert!(
        differs || bright,
        "expected skinned triangle in offscreen buffer (differs={differs} bright={bright})"
    );
}
