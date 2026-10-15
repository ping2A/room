//! Headless offscreen smoke (skipped when no GPU adapter).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_nodebuild::apply_nodes;
use room_render::{
    Camera, GpuContext, OffscreenTarget, UnlitPass, build_scene, encode_png, read_rgba,
    render_offscreen, render_offscreen_stenciled,
};
use room_testkit::{MapBuilder, rect};

#[test]
fn offscreen_room_png() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping offscreen_room_png: no wgpu adapter");
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
        eye: Vec3::new(128.0, 64.0, 41.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    let scene = build_scene(&map, &camera, None);
    assert!(!scene.opaque_walls.is_empty());
    let target = OffscreenTarget::new(&gpu, 320, 200);
    let mut pass = UnlitPass::new(&gpu, target.format);
    render_offscreen(&gpu, &mut pass, &target, &camera, &scene);
    let rgba = read_rgba(&gpu, &target).expect("readback");
    assert_eq!(rgba.len(), 320 * 200 * 4);
    let clear = [13u8, 13, 20, 255];
    let differs = rgba.chunks_exact(4).any(|p| {
        (i16::from(p[0]) - i16::from(clear[0])).abs() > 8
            || (i16::from(p[1]) - i16::from(clear[1])).abs() > 8
            || (i16::from(p[2]) - i16::from(clear[2])).abs() > 8
    });
    assert!(differs, "expected rendered geometry in offscreen buffer");
    let mut png = Vec::new();
    encode_png(320, 200, &rgba, &mut png).unwrap();
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    let mut hist = [0u32; 64];
    for px in rgba.chunks_exact(4) {
        let bucket = (usize::from(px[0]) >> 2) ^ (usize::from(px[1]) >> 4);
        hist[bucket & 63] += 1;
    }
    let used = hist.iter().filter(|&&c| c > 0).count();
    assert!(used >= 3, "expected colour variety, got {used} buckets");
}

#[test]
fn offscreen_stencil_pipelines_smoke() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping offscreen_stencil_pipelines_smoke: no wgpu adapter");
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
        eye: Vec3::new(128.0, 64.0, 41.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    let scene = build_scene(&map, &camera, None);
    let target = OffscreenTarget::new(&gpu, 64, 64);
    let mut pass = UnlitPass::new(&gpu, target.format);

    render_offscreen_stenciled(&gpu, &mut pass, &target, &camera, &scene, &[]);
    let empty = read_rgba(&gpu, &target).expect("readback");
    let px0 = [empty[0], empty[1], empty[2], empty[3]];
    assert!(
        empty.chunks_exact(4).all(|p| p == px0),
        "empty stencil mask must be a flat clear"
    );

    assert!(!scene.wall_ranges.is_empty());
    render_offscreen_stenciled(
        &gpu,
        &mut pass,
        &target,
        &camera,
        &scene,
        &scene.wall_ranges,
    );
    let rgba = read_rgba(&gpu, &target).expect("readback");
    let differs = rgba.chunks_exact(4).any(|p| p != px0);
    assert!(differs, "stencil MRT mask+Equal should rasterize");
}
