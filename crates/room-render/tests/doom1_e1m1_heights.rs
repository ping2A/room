//! Offscreen E1M1: wall/flat Z parity and floor coverage (IWAD nodes).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_render::{Camera, VisibleSet, build_scene, build_scene_from_visible};
use room_vfs::VfsBuilder;
use glam::Vec3;
use std::path::PathBuf;

fn doom1() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    p.is_file().then_some(p)
}

/// Load E1M1 keeping shipped IWAD nodes (do not rebuild — that destroys coverage).
fn load_e1m1() -> Option<room_map::MapGeometry> {
    let wad = doom1()?;
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    Some(map)
}

fn player_cam(map: &room_map::MapGeometry) -> Camera {
    let start = map.things.iter().find(|t| t.type_id == 1).unwrap();
    let sid = room_nodebuild::point_in_sector(map, f64::from(start.x), f64::from(start.y));
    let floor = sid
        .and_then(|s| map.sectors.get(s.raw() as usize))
        .map(|s| s.floor_height)
        .unwrap_or(0.0);
    Camera {
        eye: Vec3::new(
            f32::from(start.x),
            f32::from(start.y),
            (floor + 41.0) as f32,
        ),
        forward: Vec3::new(1.0, 0.0, 0.0),
        ..Camera::default()
    }
}

fn in_tri(p: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let sign = |p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)| {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };
    let b1 = sign(p, a, b) < 0.0;
    let b2 = sign(p, b, c) < 0.0;
    let b3 = sign(p, c, a) < 0.0;
    b1 == b2 && b2 == b3
}

fn floor_tris(scene: &room_render::SceneLists) -> Vec<([(f64, f64); 3], f64)> {
    let mut out = Vec::new();
    for tri in scene.flats.chunks_exact(3) {
        let ax = f64::from(tri[0].pos[0]);
        let ay = f64::from(tri[0].pos[1]);
        let az = f64::from(tri[0].pos[2]);
        let bx = f64::from(tri[1].pos[0]);
        let by = f64::from(tri[1].pos[1]);
        let bz = f64::from(tri[1].pos[2]);
        let cx = f64::from(tri[2].pos[0]);
        let cy = f64::from(tri[2].pos[1]);
        let cz = f64::from(tri[2].pos[2]);
        let nz = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if nz <= 0.0 {
            continue;
        }
        out.push(([(ax, ay), (bx, by), (cx, cy)], (az + bz + cz) / 3.0));
    }
    out
}

#[test]
fn e1m1_step_wall_z_matches_sector_floors() {
    let Some(map) = load_e1m1() else {
        return;
    };
    let cam = player_cam(&map);
    let scene = build_scene(&map, &cam, None);
    eprintln!(
        "E1M1 scene: walls={} flats={} sprites={} vis_ss={}",
        scene.opaque_walls.len(),
        scene.flats.len(),
        scene.sprites.len(),
        scene.visible_subsectors.len()
    );
    assert!(!scene.flats.is_empty());
    assert!(!scene.opaque_walls.is_empty());

    let sid = room_nodebuild::point_in_sector(&map, f64::from(cam.eye.x), f64::from(cam.eye.y));
    let floor = sid
        .and_then(|s| map.sectors.get(s.raw() as usize))
        .map(|s| s.floor_height)
        .unwrap_or(0.0);
    let player_flat = (floor * 10.0).round() as i32;
    let flat_z: std::collections::BTreeSet<i32> = scene
        .flats
        .iter()
        .map(|v| (v.pos[2] * 10.0).round() as i32)
        .collect();
    assert!(
        flat_z.contains(&player_flat),
        "player floor {floor} missing from flats; have {flat_z:?}"
    );
}

#[test]
fn e1m1_floor_tri_centroids_match_own_sector_height() {
    let Some(map) = load_e1m1() else {
        return;
    };
    let scene = build_scene(&map, &player_cam(&map), None);
    assert!(
        scene.flats.len() % 3 == 0,
        "flats must be triangles, got {}",
        scene.flats.len()
    );

    let mut bad = 0u32;
    let mut checked = 0u32;
    let mut examples = Vec::new();
    for (tri, mz) in floor_tris(&scene) {
        let mx = (tri[0].0 + tri[1].0 + tri[2].0) / 3.0;
        let my = (tri[0].1 + tri[1].1 + tri[2].1) / 3.0;
        checked += 1;
        let Some(got) = room_nodebuild::point_in_sector(&map, mx, my) else {
            // BSP leaves can extend into the void; ignore exterior centroids.
            continue;
        };
        let Some(sec) = map.sectors.get(got.raw() as usize) else {
            bad += 1;
            continue;
        };
        if (sec.floor_height - mz).abs() > 1.0 {
            bad += 1;
            if examples.len() < 12 {
                examples.push(format!(
                    "centroid ({mx:.0},{my:.0}) sec {} floor={} flat_z={mz:.1}",
                    got.raw(),
                    sec.floor_height
                ));
            }
        }
    }
    eprintln!("floor centroid check: tris={checked} mismatches={bad}");
    for e in &examples {
        eprintln!("  {e}");
    }
    assert_eq!(
        bad, 0,
        "{bad} floor triangles drawn at the wrong sector height"
    );
    assert!(checked > 50, "expected many floor tris, got {checked}");
}

#[test]
fn e1m1_mapthings_have_floor_tris() {
    let Some(map) = load_e1m1() else {
        return;
    };
    let nodes = map.game_nodes.as_ref().unwrap();
    let vis = VisibleSet {
        subsectors: (0..nodes.subsectors.len() as u32).collect(),
    };
    let scene = build_scene_from_visible(&map, &vis);
    let tris = floor_tris(&scene);
    let mut holes = Vec::new();
    let mut wrong = Vec::new();
    for t in &map.things {
        let x = f64::from(t.x);
        let y = f64::from(t.y);
        let Some(sid) = room_nodebuild::point_in_sector(&map, x, y) else {
            continue;
        };
        let sec = &map.sectors[sid.raw() as usize];
        if room_render::is_sky_flat(&sec.floor_pic) {
            continue;
        }
        let mut hit_z = None;
        for (tri, z) in &tris {
            if in_tri((x, y), tri[0], tri[1], tri[2]) {
                hit_z = Some(*z);
                break;
            }
        }
        match hit_z {
            None => holes.push(format!(
                "type={} ({x:.0},{y:.0}) sec {} floor={}",
                t.type_id,
                sid.raw(),
                sec.floor_height
            )),
            Some(z) if (z - sec.floor_height).abs() > 1.5 => wrong.push(format!(
                "type={} ({x:.0},{y:.0}) expect={} got={z:.1}",
                t.type_id, sec.floor_height
            )),
            _ => {}
        }
    }
    eprintln!(
        "mapthing floors: holes={} wrong={}",
        holes.len(),
        wrong.len()
    );
    for h in holes.iter().chain(wrong.iter()).take(20) {
        eprintln!("  {h}");
    }
    assert!(
        wrong.is_empty(),
        "{} mapthings on wrong floor Z",
        wrong.len()
    );
    assert!(
        holes.is_empty(),
        "{} mapthings sit on empty floor",
        holes.len()
    );
}
