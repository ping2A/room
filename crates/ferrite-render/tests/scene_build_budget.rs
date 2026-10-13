//! Soft scene-build CPU budget check (docs/13 §14: ≤ 2 ms large maps — synthetic room).

#![allow(clippy::unwrap_used)]

use ferrite_nodebuild::apply_nodes;
use ferrite_render::{Camera, build_scene};
use ferrite_testkit::{MapBuilder, rect};
use glam::Vec3;
use std::time::Instant;

#[test]
fn synthetic_room_scene_build_under_budget() {
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(256.0, 256.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let camera = Camera {
        eye: Vec3::new(256.0, 128.0, 41.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    // Warm-up.
    let _ = build_scene(&map, &camera, None);
    let t0 = Instant::now();
    let n = 50;
    for _ in 0..n {
        let _ = build_scene(&map, &camera, None);
    }
    let elapsed = t0.elapsed();
    let per_ms = elapsed.as_secs_f64() * 1000.0 / f64::from(n);
    eprintln!("scene build mean = {per_ms:.3} ms over {n} runs");
    // Synthetic room should be well under the 2 ms large-map budget.
    assert!(
        per_ms < 2.0,
        "scene build {per_ms:.3} ms exceeds 2 ms soft budget"
    );
}
