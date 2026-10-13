//! GPU instanced particle draw smoke (skipped without adapter).

#![allow(clippy::unwrap_used)]

use ferrite_render::{
    Camera, GpuContext, OffscreenTarget, ParticleInstancedPass, ParticleStore, ParticleStyle,
    read_rgba,
};
use glam::Vec3;

#[test]
fn particle_instanced_draws_pixels() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping particle_instanced_draws_pixels: no wgpu adapter");
            return;
        }
    };
    let target = OffscreenTarget::new(&gpu, 64, 64);
    let mut pass = ParticleInstancedPass::new(&gpu, target.format);
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
        32.0,
        1.0,
        100,
        ParticleStyle::Additive,
    );
    pass.draw_store(
        &gpu,
        &target.color_view,
        &target.depth_view,
        &camera,
        &store,
        true,
    );
    let rgba = read_rgba(&gpu, &target).expect("readback");
    let lit = rgba
        .chunks_exact(4)
        .filter(|c| c[0] > 10 || c[1] > 10 || c[2] > 10)
        .count();
    assert!(lit > 10, "expected particle pixels, got lit={lit}");
}
