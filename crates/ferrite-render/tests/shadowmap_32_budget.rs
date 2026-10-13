//! Soft CPU budget for 1D shadowmap bake (32 lights × occluders).

#![allow(clippy::unwrap_used)]

use ferrite_render::{DynLightInstance, ShadowOccluder, bake_1d_shadowmaps};
use glam::Vec3;
use std::time::Instant;

#[test]
fn bake_32_lights_shadowmaps_under_budget() {
    let lights: Vec<DynLightInstance> = (0..32)
        .map(|i| {
            let a = i as f32 * 0.2;
            DynLightInstance::point(
                Vec3::new(a.sin() * 200.0, a.cos() * 200.0, 40.0),
                [1.0, 0.9, 0.7],
                128.0,
            )
        })
        .collect();
    let occluders: Vec<ShadowOccluder> = (0..64)
        .map(|i| {
            let t = i as f32;
            ShadowOccluder {
                a: glam::Vec2::new(t * 10.0 - 300.0, -50.0),
                b: glam::Vec2::new(t * 10.0 - 300.0, 50.0),
            }
        })
        .collect();
    let _ = bake_1d_shadowmaps(&lights, &occluders);
    let t0 = Instant::now();
    let n = 10;
    for _ in 0..n {
        let _ = bake_1d_shadowmaps(&lights, &occluders);
    }
    let per_ms = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    eprintln!("bake 32 lights × 64 occ mean = {per_ms:.3} ms over {n} runs");
    assert!(
        per_ms < 50.0,
        "shadowmap bake {per_ms:.3} ms exceeds 50 ms soft budget"
    );
}
