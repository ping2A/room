//! Soft / SSIM golden for GPU instanced particles.
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-render --test golden_particles`

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render::{
    Camera, GpuContext, OffscreenTarget, ParticleInstancedPass, ParticleStore, ParticleStyle,
    encode_png, read_rgba,
};
use glam::Vec3;
use image::{ImageBuffer, RgbaImage};
use std::path::PathBuf;

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/goldens/particles_64x64.png")
}

fn rgba_to_image(width: u32, height: u32, rgba: &[u8]) -> RgbaImage {
    ImageBuffer::from_raw(width, height, rgba.to_vec()).expect("rgba size")
}

fn render_particles(gpu: &GpuContext) -> Vec<u8> {
    let target = OffscreenTarget::new(gpu, 64, 64);
    let mut pass = ParticleInstancedPass::new(gpu, target.format);
    let camera = Camera {
        eye: Vec3::new(0.0, -64.0, 0.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    let mut store = ParticleStore::default();
    store.spawn_styled(
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::ZERO,
        40.0,
        1.0,
        100,
        ParticleStyle::Additive,
    );
    pass.draw_store(
        gpu,
        &target.color_view,
        &target.depth_view,
        &camera,
        &store,
        true,
    );
    read_rgba(gpu, &target).expect("readback")
}

#[test]
fn golden_particles_ssim() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping golden_particles_ssim: no wgpu adapter");
            return;
        }
    };
    let rgba = render_particles(&gpu);
    let got = rgba_to_image(64, 64, &rgba);
    let path = golden_path();
    if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut png = Vec::new();
        encode_png(64, 64, &rgba, &mut png).unwrap();
        std::fs::write(&path, png).unwrap();
        eprintln!("wrote {}", path.display());
        return;
    }
    if !path.exists() {
        // Soft mean check when golden missing (CI without committed PNG yet).
        let mean = rgba.chunks_exact(4).map(|c| u64::from(c[0])).sum::<u64>() as f32
            / (rgba.len() / 4) as f32;
        eprintln!("golden missing {}; soft mean_r={mean:.1}", path.display());
        assert!(mean > 5.0, "expected particle glow mean_r={mean}");
        return;
    }
    let expected = image::open(&path).unwrap().into_rgba8();
    assert_eq!(expected.dimensions(), (64, 64));
    let score = image_compare::rgba_hybrid_compare(&expected, &got)
        .expect("compare")
        .score;
    eprintln!("particles golden SSIM-ish score = {score}");
    assert!(score > 0.75, "particles golden score {score} too low");
}
