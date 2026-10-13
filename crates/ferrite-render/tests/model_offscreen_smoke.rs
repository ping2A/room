//! Offscreen smoke: OBJ model via [`append_model`] (skipped without GPU).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ferrite_nodebuild::apply_nodes;
use ferrite_render::{
    Camera, GpuContext, ModelMesh, ModelVertex, OffscreenTarget, UnlitPass, append_model,
    build_scene, read_rgba, render_offscreen,
};
use ferrite_testkit::{MapBuilder, rect};
use glam::Vec3;

#[test]
fn offscreen_append_model_visible() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping offscreen_append_model_visible: no wgpu adapter");
            return;
        }
    };
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let camera = Camera {
        eye: Vec3::new(128.0, 64.0, 48.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    let mut scene = build_scene(&map, &camera, None);
    let before = scene.opaque_walls.len();
    // Unit triangle in front of the camera.
    let mesh = ModelMesh {
        vertices: vec![
            ModelVertex {
                pos: [-16.0, 80.0, 32.0],
                uv: [0.0, 0.0],
                normal: [0.0, -1.0, 0.0],
            },
            ModelVertex {
                pos: [16.0, 80.0, 32.0],
                uv: [1.0, 0.0],
                normal: [0.0, -1.0, 0.0],
            },
            ModelVertex {
                pos: [0.0, 80.0, 56.0],
                uv: [0.5, 1.0],
                normal: [0.0, -1.0, 0.0],
            },
        ],
        indices: vec![0, 1, 2],
    };
    append_model(
        &mut scene,
        &mesh,
        Vec3::new(128.0, 0.0, 0.0),
        Vec3::ONE,
        1.0,
        "-",
    );
    assert_eq!(scene.opaque_walls.len(), before + 3);

    let target = OffscreenTarget::new(&gpu, 320, 200);
    let mut pass = UnlitPass::new(&gpu, target.format);
    render_offscreen(&gpu, &mut pass, &target, &camera, &scene);
    let rgba = read_rgba(&gpu, &target).expect("readback");
    let clear = [13u8, 13, 20, 255];
    let differs = rgba.chunks_exact(4).any(|p| {
        (i16::from(p[0]) - i16::from(clear[0])).abs() > 8
            || (i16::from(p[1]) - i16::from(clear[1])).abs() > 8
            || (i16::from(p[2]) - i16::from(clear[2])).abs() > 8
    });
    assert!(differs, "expected model+room geometry in offscreen buffer");
}
