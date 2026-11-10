//! Soft golden: spectre fuzz darkens / modulates a sprite (skipped without GPU).
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-render --test golden_fuzz`

#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use image::{ImageBuffer, RgbaImage};
use room_nodebuild::apply_nodes;
use room_render::{
    Camera, FuzzType, GpuContext, OffscreenTarget, SceneLists, SpriteInstance, UnlitPass,
    encode_png, read_rgba, render_offscreen,
};
use room_testkit::{MapBuilder, rect};
use std::path::PathBuf;

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/goldens/fuzz_320x200.png")
}

fn rgba_to_image(width: u32, height: u32, rgba: &[u8]) -> RgbaImage {
    ImageBuffer::from_raw(width, height, rgba.to_vec()).expect("rgba size")
}

#[test]
fn fuzz_modulates_sprite_mean() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping fuzz_modulates_sprite_mean: no wgpu adapter");
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
    let mut scene = SceneLists::default();
    // Large white sprite in front of the camera (uses default white albedo).
    let start = 0u32;
    let quad = room_render::emit_sprite_quad(
        SpriteInstance {
            pos: Vec3::new(128.0, 120.0, 40.0),
            width: 64.0,
            height: 64.0,
            light: 1.0,
            alpha: 1.0,
            fuzz: true,
            texture: "-".into(),
            flip_u: false,
            anchor_u: 0.5,
            orient: room_render::SpriteOrient::Billboard,
            additive: false,
            roll: 0.0,
            fx: Default::default(),
        },
        camera.forward,
    );
    scene.sprites.extend_from_slice(&quad);
    scene.sprite_ranges.push(room_render::MaterialRange {
        name: "-".into(),
        start,
        count: 6,
        flags: 0,
    });

    let target = OffscreenTarget::new(&gpu, 320, 200);
    let mut pass = UnlitPass::new(&gpu, target.format);
    pass.fuzz_type = FuzzType::Shadow;
    pass.time = 0.25;
    render_offscreen(&gpu, &mut pass, &target, &camera, &scene);
    let rgba = read_rgba(&gpu, &target).expect("readback");

    let mean = |px: &[u8]| -> f32 {
        let mut s = 0u64;
        let mut n = 0u64;
        for c in px.chunks_exact(4) {
            s += u64::from(c[0]) + u64::from(c[1]) + u64::from(c[2]);
            n += 3;
        }
        s as f32 / n as f32
    };
    let m = mean(&rgba);
    eprintln!("fuzz frame mean luma={m:.2}");
    // Clear is dark (~13); fuzzed sprite should lift mean above clear-only.
    assert!(
        m > 15.0,
        "expected fuzzed sprite to brighten frame (mean={m})"
    );

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
            "skipping fuzz SSIM: {} missing (set ROOM_UPDATE_GOLDEN=1 to create)",
            path.display()
        );
        return;
    }
    let got = rgba_to_image(320, 200, &rgba);
    let expected = image::open(&path).unwrap().into_rgba8();
    assert_eq!(expected.dimensions(), (320, 200));
    let score = image_compare::rgba_hybrid_compare(&expected, &got)
        .expect("compare")
        .score;
    eprintln!("fuzz golden SSIM-ish score = {score}");
    assert!(score > 0.75, "fuzz golden score {score} too low");
}
