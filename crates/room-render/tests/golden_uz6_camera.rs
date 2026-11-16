//! UZ6 (docs/36): `SetViewPos(…, VPSF_ORTHOGRAPHIC)` camera projection.
//! Golden `testdata/goldens/uz6_ortho.txt`: clip-space x/y of points at
//! growing depth — orthographic keeps them fixed, perspective shrinks them.
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-render --test golden_uz6_camera`

#![allow(clippy::unwrap_used)]

use glam::{Vec3, Vec4};
use room_render::Camera;
use std::fmt::Write as _;

#[test]
fn uz6_ortho_projection_matches_golden() {
    let base = Camera {
        eye: Vec3::new(0.0, 0.0, 0.0),
        forward: Vec3::new(1.0, 0.0, 0.0),
        aspect: 1.6,
        ..Camera::default()
    };
    let ortho = Camera {
        ortho: Some(256.0),
        ..base
    };
    let mut out = String::new();
    let mut ortho_x = Vec::new();
    for depth in [64.0f32, 256.0, 1024.0] {
        // 64 units left and 32 up of the view axis.
        let p = Vec4::new(depth, 64.0, 32.0, 1.0);
        for (label, cam) in [("persp", &base), ("ortho", &ortho)] {
            let c = cam.view_proj() * p;
            let (x, y) = (c.x / c.w, c.y / c.w);
            let _ = writeln!(out, "{label} depth={depth:<5} x={x:.4} y={y:.4}");
            if label == "ortho" {
                ortho_x.push((x, y));
            }
        }
    }
    assert!(
        ortho_x
            .windows(2)
            .all(|w| (w[0].0 - w[1].0).abs() < 1e-4 && (w[0].1 - w[1].1).abs() < 1e-4),
        "orthographic projection ignores depth: {out}"
    );
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/goldens/uz6_ortho.txt");
    if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &out).unwrap();
    }
    assert_eq!(out, std::fs::read_to_string(&path).unwrap());
}
