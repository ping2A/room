//! Combat arena floors must cover the fight space (MapBuilder Doom winding).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_nodebuild::apply_nodes;
use room_render::{Camera, build_scene};
use room_testkit::combat_arena;

fn point_in_tri(p: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let sign = |p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)| {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };
    let b1 = sign(p, a, b) < 0.0;
    let b2 = sign(p, b, c) < 0.0;
    let b3 = sign(p, c, a) < 0.0;
    b1 == b2 && b2 == b3
}

fn floor_covers(scene: &room_render::SceneLists, x: f64, y: f64) -> bool {
    for tri in scene.flats.chunks_exact(3) {
        let zavg = (tri[0].pos[2] + tri[1].pos[2] + tri[2].pos[2]) / 3.0;
        if zavg > 4.0 && (zavg - 24.0).abs() > 2.0 {
            continue;
        }
        let a = (f64::from(tri[0].pos[0]), f64::from(tri[0].pos[1]));
        let b = (f64::from(tri[1].pos[0]), f64::from(tri[1].pos[1]));
        let c = (f64::from(tri[2].pos[0]), f64::from(tri[2].pos[1]));
        if point_in_tri((x, y), a, b, c) {
            return true;
        }
    }
    false
}

#[test]
fn combat_arena_bsp_matches_vanilla_in_arena() {
    let mut map = combat_arena().build_udmf("MAP01").unwrap();
    apply_nodes(&mut map);
    for (x, y) in [
        (1280.0, 500.0),
        (1280.0, 900.0),
        (1280.0, 1300.0),
        (800.0, 900.0),
        (1800.0, 900.0),
    ] {
        let bsp = room_nodebuild::point_in_sector(&map, x, y);
        let vanilla = room_nodebuild::point_in_sector_vanilla(&map, x, y);
        assert_eq!(
            bsp, vanilla,
            "BSP/vanilla sector mismatch at ({x},{y}): {bsp:?} vs {vanilla:?}"
        );
        let sid = bsp.expect("point must be in a sector");
        let pic = &map.sectors[sid.raw() as usize].floor_pic;
        assert!(
            pic.eq_ignore_ascii_case("FLOOR4_8"),
            "expected main arena floor at ({x},{y}), got {pic} (sec {})",
            sid.raw()
        );
    }
}

#[test]
fn combat_arena_forward_floor_coverage() {
    let mut map = combat_arena().build_udmf("MAP01").unwrap();
    apply_nodes(&mut map);
    let eye = (1280.0_f64, 900.0_f64);
    let cam = Camera {
        eye: Vec3::new(eye.0 as f32, eye.1 as f32, 41.0),
        forward: Vec3::new(0.0, 1.0, -0.15).normalize(),
        ..Camera::default()
    };
    let scene = build_scene(&map, &cam, None);
    assert!(
        scene
            .flat_ranges
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case("FLOOR4_8")),
        "main arena FLOOR4_8 must be emitted, got {:?}",
        scene
            .flat_ranges
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>()
    );

    let mut miss = 0u32;
    let mut hit = 0u32;
    let mut samples = Vec::new();
    for yi in 0..20 {
        for xi in 0..16 {
            let x = 600.0 + f64::from(xi) * (1960.0 - 600.0) / 15.0;
            let y = eye.1 + 40.0 + f64::from(yi) * 40.0;
            if room_nodebuild::point_in_sector(&map, x, y).is_none() {
                continue;
            }
            if floor_covers(&scene, x, y) {
                hit += 1;
            } else {
                miss += 1;
                if samples.len() < 12 {
                    samples.push((x, y));
                }
            }
        }
    }
    assert!(
        miss * 10 < hit + miss,
        "visible forward floor has holes: hit={hit} miss={miss} samples={samples:?}"
    );
}
