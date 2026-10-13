//! SSIM golden for the synthetic test room (skipped without GPU / golden file).
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-render --test golden_room`

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_nodebuild::apply_nodes;
use room_render::{
    Camera, GpuContext, OffscreenTarget, UnlitPass, build_scene, encode_png, read_rgba,
    render_offscreen,
};
use room_testkit::{MapBuilder, rect};
use glam::Vec3;
use image::{ImageBuffer, RgbaImage};
use std::path::PathBuf;

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/goldens/room_320x200.png")
}

fn rgba_to_image(width: u32, height: u32, rgba: &[u8]) -> RgbaImage {
    ImageBuffer::from_raw(width, height, rgba.to_vec()).expect("rgba size")
}

#[test]
fn golden_room_ssim() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping golden_room_ssim: no wgpu adapter");
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
    let target = OffscreenTarget::new(&gpu, 320, 200);
    let mut pass = UnlitPass::new(&gpu, target.format);
    render_offscreen(&gpu, &mut pass, &target, &camera, &scene);
    let rgba = read_rgba(&gpu, &target).expect("readback");
    let got = rgba_to_image(320, 200, &rgba);
    let path = golden_path();
    if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut png = Vec::new();
        encode_png(320, 200, &rgba, &mut png).unwrap();
        std::fs::write(&path, png).unwrap();
        eprintln!("wrote {}", path.display());
        return;
    }
    if !path.exists() {
        eprintln!(
            "skipping golden_room_ssim: {} missing (set ROOM_UPDATE_GOLDEN=1 to create)",
            path.display()
        );
        return;
    }
    let expected = image::open(&path).unwrap().into_rgba8();
    assert_eq!(expected.dimensions(), (320, 200));
    let score = image_compare::rgba_hybrid_compare(&expected, &got)
        .expect("compare")
        .score;
    eprintln!("room golden SSIM-ish score = {score}");
    // Loose threshold: Metal vs lavapipe differ; catch total regressions.
    assert!(
        score > 0.85,
        "golden score {score} too low (expected > 0.85)"
    );
}
