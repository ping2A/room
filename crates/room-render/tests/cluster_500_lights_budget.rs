//! Soft CPU budget for clustered assignment of 500 lights (`13` §14 soft stand-in).

#![allow(clippy::unwrap_used)]

use glam::Vec3;
use room_render::{ClusterAssignment, DynLightInstance};
use std::time::Instant;

#[test]
fn cluster_assign_500_lights_under_budget() {
    let eye = Vec3::new(0.0, 0.0, 40.0);
    let lights: Vec<DynLightInstance> = (0..500)
        .map(|i| {
            let a = i as f32 * 0.13;
            DynLightInstance {
                pos: Vec3::new(a.sin() * 400.0, a.cos() * 400.0, 40.0),
                color: [1.0, 0.8, 0.5],
                radius: 64.0,
                subtractive: false,
                spot_dir: None,
                spot_outer_cos: 1.0,
                spot_inner_cos: 1.0,
            }
        })
        .collect();
    // Warm-up.
    let _ = ClusterAssignment::build(eye, &lights);
    let t0 = Instant::now();
    let n = 20;
    for _ in 0..n {
        let _ = ClusterAssignment::build(eye, &lights);
    }
    let per_ms = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    eprintln!("cluster 500 lights mean = {per_ms:.3} ms over {n} runs");
    // Soft desktop budget — far below a frame; CI floor is generous.
    assert!(
        per_ms < 16.0,
        "cluster assign {per_ms:.3} ms exceeds 16 ms soft budget"
    );
}
