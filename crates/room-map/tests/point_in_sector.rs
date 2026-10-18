//! Point-in-sector grid sampling vs original BSP nodes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_map::load_map;
use room_nodebuild::{point_in_sector, point_in_sector_vanilla};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

fn freedoom2() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    p.is_file().then_some(p)
}

fn map_bounds(map: &room_map::MapGeometry) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for v in &map.vertices {
        min_x = min_x.min(v.pos.x());
        min_y = min_y.min(v.pos.y());
        max_x = max_x.max(v.pos.x());
        max_y = max_y.max(v.pos.y());
    }
    (min_x, min_y, max_x, max_y)
}

#[test]
fn freedoom_map01_grid_consistent() {
    let Some(path) = freedoom2() else {
        eprintln!("skip: freedoom2.wad missing");
        return;
    };
    let vfs = VfsBuilder::new()
        .mount_path(&path)
        .unwrap()
        .build()
        .unwrap();
    let map = load_map(&vfs, "MAP01").unwrap();
    let (min_x, min_y, max_x, max_y) = map_bounds(&map);

    let step = 64.0;
    let mut hits = 0u32;
    let mut x = min_x + 16.0;
    while x < max_x {
        let mut y = min_y + 16.0;
        while y < max_y {
            if let Some(sa) = point_in_sector(&map, x, y) {
                assert!(
                    (sa.raw() as usize) < map.sectors.len(),
                    "sector out of range at ({x},{y})"
                );
                hits += 1;
            }
            y += step;
        }
        x += step;
    }
    assert!(hits > 50, "expected many in-map samples, got {hits}");
}

/// Float BSP walk must match Doom `R_PointOnSide` / `R_PointInSubsector` on the
/// IWAD's original node tree (roadmap: point-in-sector parity vs original nodes).
#[test]
fn freedoom_map01_parity_float_vs_vanilla_nodes() {
    let Some(path) = freedoom2() else {
        eprintln!("skip: freedoom2.wad missing");
        return;
    };
    let vfs = VfsBuilder::new()
        .mount_path(&path)
        .unwrap()
        .build()
        .unwrap();
    let map = load_map(&vfs, "MAP01").unwrap();
    assert!(
        map.game_nodes.is_some(),
        "Freedoom MAP01 should ship classic nodes"
    );

    let (min_x, min_y, max_x, max_y) = map_bounds(&map);
    let step = 32.0;
    let mut compared = 0u32;
    let mut mismatches = 0u32;
    let mut x = min_x + 8.0;
    while x < max_x {
        let mut y = min_y + 8.0;
        while y < max_y {
            let a = point_in_sector(&map, x, y);
            let b = point_in_sector_vanilla(&map, x, y);
            if a != b {
                mismatches += 1;
            }
            compared += 1;
            y += step;
        }
        x += step;
    }
    assert!(
        compared > 100,
        "expected dense grid, got {compared} samples"
    );
    let rate = 1.0 - f64::from(mismatches) / f64::from(compared);
    eprintln!(
        "MAP01 float↔vanilla BSP sector parity: {compared} samples, {mismatches} mismatches ({:.2}% agree)",
        rate * 100.0
    );
    assert!(
        rate >= 0.995,
        "sector parity too low: {:.2}% (mismatches {mismatches}/{compared})",
        rate * 100.0
    );
}

#[test]
fn map03_ledge_sector_west_of_160() {
    let Some(path) = freedoom2() else {
        return;
    };
    let vfs = VfsBuilder::new()
        .mount_path(&path)
        .unwrap()
        .build()
        .unwrap();
    let map = load_map(&vfs, "MAP03").unwrap();
    // West of x=160 between y=736..800 should be floor 16 (back of L21).
    let sid = point_in_sector(&map, 143.0, 777.0).expect("sector");
    let floor = map.sectors[sid.raw() as usize].floor_height;
    let sid_v = point_in_sector_vanilla(&map, 143.0, 777.0).expect("vanilla sector");
    let floor_v = map.sectors[sid_v.raw() as usize].floor_height;
    eprintln!("game_nodes floor={floor} vanilla_walk floor={floor_v} sid={sid:?} sid_v={sid_v:?}");
    assert_eq!(floor_v, 16.0, "vanilla nodes should report floor 16");
    // After rebuild:
    let mut rebuilt = load_map(&vfs, "MAP03").unwrap();
    room_nodebuild::apply_nodes(&mut rebuilt);
    let sid_r = point_in_sector(&rebuilt, 143.0, 777.0).expect("rebuilt");
    let floor_r = rebuilt.sectors[sid_r.raw() as usize].floor_height;
    eprintln!("rebuilt floor={floor_r}");
    assert_eq!(
        floor, 16.0,
        "IWAD game_nodes should report floor 16 at (143,777)"
    );
    // Both sides of a two-sided line once shared a rebuilt leaf (floor 8 here).
    assert_eq!(floor_r, 16.0, "rebuilt nodes should report floor 16");
}
