//! Soft CPU budget for ~10k sprite billboards (nuts.wad stand-in, `13` §14).

#![allow(clippy::unwrap_used)]

use glam::Vec3;
use room_render::{Camera, SceneLists, SpriteInstance, append_sprites};
use std::time::Instant;

#[test]
fn append_10k_sprites_under_budget() {
    let camera = Camera {
        eye: Vec3::new(0.0, -200.0, 40.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    let sprites: Vec<SpriteInstance> = (0..10_000)
        .map(|i| {
            let a = i as f32 * 0.01;
            SpriteInstance {
                pos: Vec3::new(a.sin() * 500.0, a.cos() * 500.0, 0.0),
                width: 32.0,
                height: 56.0,
                light: 1.0,
                alpha: 1.0,
                fuzz: false,
                texture: "TROOA1".into(),
                flip_u: false,
                anchor_u: 0.5,
                orient: room_render::SpriteOrient::Billboard,
            }
        })
        .collect();
    let mut lists = SceneLists::default();
    append_sprites(&mut lists, &sprites, &camera);
    assert_eq!(lists.sprites.len(), 10_000 * 6);

    let t0 = Instant::now();
    let n = 10;
    for _ in 0..n {
        let mut lists = SceneLists::default();
        append_sprites(&mut lists, &sprites, &camera);
    }
    let per_ms = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    eprintln!("append 10k sprites mean = {per_ms:.3} ms over {n} runs");
    assert!(
        per_ms < 50.0,
        "10k sprite append {per_ms:.3} ms exceeds 50 ms soft budget"
    );
}
