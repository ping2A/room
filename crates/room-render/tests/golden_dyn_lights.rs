//! SSIM golden for GPU dyn lights (skipped without GPU / golden file).
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-render --test golden_dyn_lights`

#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use image::{ImageBuffer, RgbaImage};
use room_nodebuild::apply_nodes;
use room_render::{
    Camera, DynLightInstance, GpuContext, OffscreenTarget, UnlitPass, build_scene, encode_png,
    read_rgba, render_offscreen,
};
use room_testkit::{MapBuilder, rect};
use std::path::PathBuf;

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/goldens/dyn_lights_320x200.png")
}

fn rgba_to_image(width: u32, height: u32, rgba: &[u8]) -> RgbaImage {
    ImageBuffer::from_raw(width, height, rgba.to_vec()).expect("rgba size")
}

fn lit_scene_rgba(gpu: &GpuContext) -> (Camera, Vec<u8>) {
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
    let mut scene = build_scene(&map, &camera, None);
    scene.dyn_lights.clear();
    scene.dyn_lights.push(DynLightInstance {
        pos: Vec3::new(128.0, 100.0, 40.0),
        color: [1.0, 1.0, 1.0],
        radius: 200.0,
        subtractive: false,
        spot_dir: None,
        spot_outer_cos: 1.0,
        spot_inner_cos: 1.0,
    });
    let target = OffscreenTarget::new(gpu, 320, 200);
    let mut pass = UnlitPass::new(gpu, target.format);
    render_offscreen(gpu, &mut pass, &target, &camera, &scene);
    let rgba = read_rgba(gpu, &target).expect("readback");
    (camera, rgba)
}

#[test]
fn dyn_lights_brighten_nearby_wall() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping dyn_lights_brighten_nearby_wall: no wgpu adapter");
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
    let mut dark = build_scene(&map, &camera, None);
    dark.dyn_lights.clear();
    let mut lit = dark.clone();
    lit.dyn_lights.push(DynLightInstance {
        pos: Vec3::new(128.0, 100.0, 40.0),
        color: [1.0, 1.0, 1.0],
        radius: 200.0,
        subtractive: false,
        spot_dir: None,
        spot_outer_cos: 1.0,
        spot_inner_cos: 1.0,
    });

    let target = OffscreenTarget::new(&gpu, 320, 200);
    let mut pass = UnlitPass::new(&gpu, target.format);
    render_offscreen(&gpu, &mut pass, &target, &camera, &dark);
    let dark_rgba = read_rgba(&gpu, &target).expect("readback dark");
    render_offscreen(&gpu, &mut pass, &target, &camera, &lit);
    let lit_rgba = read_rgba(&gpu, &target).expect("readback lit");

    let mean = |px: &[u8]| -> f32 {
        let mut s = 0u64;
        let mut n = 0u64;
        for c in px.chunks_exact(4) {
            s += u64::from(c[0]) + u64::from(c[1]) + u64::from(c[2]);
            n += 3;
        }
        s as f32 / n as f32
    };
    let md = mean(&dark_rgba);
    let ml = mean(&lit_rgba);
    eprintln!("dyn lights mean luma dark={md:.2} lit={ml:.2}");
    assert!(
        ml > md + 0.5,
        "expected dyn light to brighten frame (dark={md} lit={ml})"
    );
}

#[test]
fn golden_dyn_lights_ssim() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping golden_dyn_lights_ssim: no wgpu adapter");
            return;
        }
    };
    let (_camera, rgba) = lit_scene_rgba(&gpu);
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
            "skipping golden_dyn_lights_ssim: {} missing (set ROOM_UPDATE_GOLDEN=1 to create)",
            path.display()
        );
        return;
    }
    let expected = image::open(&path).unwrap().into_rgba8();
    assert_eq!(expected.dimensions(), (320, 200));
    let score = image_compare::rgba_hybrid_compare(&expected, &got)
        .expect("compare")
        .score;
    eprintln!("dyn lights golden SSIM-ish score = {score}");
    assert!(
        score > 0.85,
        "golden score {score} too low (expected > 0.85)"
    );
}
