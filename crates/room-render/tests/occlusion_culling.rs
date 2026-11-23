//! Occlusion culling of the player's view ([`walk_bsp_occluded`]) must never
//! drop a subsector the eye can see.
//!
//! For many eye positions on real maps, every subsector the culled walk
//! leaves out is checked against plain 2D line-of-sight: a straight line from
//! the eye to a point inside that subsector has to cross a solid (one-sided)
//! wall. A single unobstructed point is a culling bug: the player would see a
//! hole in the world. The check is exact geometry, independent of the
//! clipper it validates.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_map::{MapGeometry, load_map};
use room_nodebuild::ensure_nodes;
use room_render::{walk_bsp, walk_bsp_occluded, walk_bsp_occluded_in_view};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

fn wad(name: &str) -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/freedoom")
        .join(name);
    p.exists().then_some(p)
}

fn load(wad_name: &str, map_name: &str) -> Option<MapGeometry> {
    let Some(path) = wad(wad_name) else {
        eprintln!("skip: testdata/freedoom/{wad_name} missing");
        return None;
    };
    let vfs = VfsBuilder::new()
        .mount_path(&path)
        .unwrap()
        .build()
        .unwrap();
    let mut map = load_map(&vfs, map_name).expect("map");
    ensure_nodes(&mut map);
    Some(map)
}

/// Solid walls as segments.
fn solid_walls(map: &MapGeometry) -> Vec<[(f64, f64); 2]> {
    map.lines
        .iter()
        .filter(|l| l.back_sector.is_none())
        .filter_map(|l| {
            let a = map.vertices.get(l.v1.raw() as usize)?.pos;
            let b = map.vertices.get(l.v2.raw() as usize)?.pos;
            Some([(a.x(), a.y()), (b.x(), b.y())])
        })
        .collect()
}

/// Proper crossing of segments `p→q` and `a→b` (touching at an end counts:
/// the wall is grown by a hair so a ray grazing a wall corner is blocked,
/// which can only make the check stricter about real gaps).
fn crosses(p: (f64, f64), q: (f64, f64), a: (f64, f64), b: (f64, f64)) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-9);
    let grow = 0.05 / len;
    let a = (a.0 - dx * grow, a.1 - dy * grow);
    let b = (b.0 + dx * grow, b.1 + dy * grow);
    let orient = |o: (f64, f64), u: (f64, f64), v: (f64, f64)| {
        (u.0 - o.0) * (v.1 - o.1) - (u.1 - o.1) * (v.0 - o.0)
    };
    let d1 = orient(a, b, p);
    let d2 = orient(a, b, q);
    let d3 = orient(p, q, a);
    let d4 = orient(p, q, b);
    (d1 * d2 <= 0.0) && (d3 * d4 <= 0.0)
}

/// Points inside subsector `ss`, each just in front of one of its segs: the
/// seg's midpoint and both ends, moved 1 unit along the seg (inward) and
/// 1.5 units onto its front side, where the subsector lies. A ray that
/// reaches such a point from the front crosses no wall of the subsector.
fn probe_points(map: &MapGeometry, ss: u32) -> Vec<(f64, f64)> {
    let nodes = map.game_nodes.as_ref().unwrap();
    let sub = &nodes.subsectors[ss as usize];
    let start = sub.first_seg as usize;
    let mut out = Vec::new();
    for s in &nodes.segs[start..start + sub.num_segs as usize] {
        let (Some(a), Some(b)) = (
            map.vertices.get(s.v1.raw() as usize),
            map.vertices.get(s.v2.raw() as usize),
        ) else {
            continue;
        };
        let (a, b) = ((a.pos.x(), a.pos.y()), (b.pos.x(), b.pos.y()));
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy);
        if len < 4.0 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        // Front (right of v1→v2).
        let (nx, ny) = (uy, -ux);
        for t in [1.0, len * 0.5, len - 1.0] {
            out.push((a.0 + ux * t + nx * 1.5, a.1 + uy * t + ny * 1.5));
        }
    }
    out
}

/// Deterministic eye positions inside the map's open sectors.
fn eyes(map: &MapGeometry, count: usize) -> Vec<(f64, f64)> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for v in &map.vertices {
        x0 = x0.min(v.pos.x());
        y0 = y0.min(v.pos.y());
        x1 = x1.max(v.pos.x());
        y1 = y1.max(v.pos.y());
    }
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut out = Vec::new();
    for _ in 0..count * 50 {
        if out.len() == count {
            break;
        }
        let (x, y) = (x0 + (x1 - x0) * next(), y0 + (y1 - y0) * next());
        let Some(s) = room_nodebuild::point_in_sector(map, x, y) else {
            continue;
        };
        let sec = &map.sectors[s.raw() as usize];
        if sec.ceiling_height > sec.floor_height {
            out.push((x, y));
        }
    }
    out
}

fn check_map(wad_name: &str, map_name: &str, eye_count: usize) {
    let Some(map) = load(wad_name, map_name) else {
        return;
    };
    let walls = solid_walls(&map);
    let total = map.game_nodes.as_ref().unwrap().subsectors.len();
    let mut culled_total = 0usize;
    for (x, y) in eyes(&map, eye_count) {
        let all = walk_bsp(&map, x, y);
        assert_eq!(
            all.subsectors.len(),
            total,
            "{map_name}: without blockrendering lines the plain walk lists every subsector"
        );
        let vis = walk_bsp_occluded(&map, x, y);
        let mut seen = vec![false; total];
        for &ss in &vis.subsectors {
            assert!(
                !seen[ss as usize],
                "{map_name}: subsector {ss} listed twice"
            );
            seen[ss as usize] = true;
        }
        for ss in 0..total as u32 {
            if seen[ss as usize] {
                continue;
            }
            culled_total += 1;
            for p in probe_points(&map, ss) {
                let blocked = walls.iter().any(|w| crosses((x, y), p, w[0], w[1]));
                assert!(
                    blocked,
                    "{map_name}: eye ({x:.1}, {y:.1}) sees ({:.1}, {:.1}) in culled subsector {ss}",
                    p.0, p.1
                );
            }
        }
    }
    // The culling must actually cull, or the frame cost is back.
    assert!(
        culled_total > total * eye_count / 4,
        "{map_name}: only {culled_total} subsectors culled over {eye_count} eyes of {total}"
    );
}

#[test]
fn occlusion_culling_never_hides_visible_space_map01() {
    check_map("freedoom2.wad", "MAP01", 24);
}

#[test]
fn occlusion_culling_never_hides_visible_space_map10() {
    check_map("freedoom2.wad", "MAP10", 16);
}

#[test]
fn occlusion_culling_never_hides_visible_space_map20() {
    check_map("freedoom2.wad", "MAP20", 16);
}

#[test]
fn occlusion_culling_never_hides_visible_space_e1m4() {
    check_map("freedoom1.wad", "E1M4", 24);
}

/// Built map: a wall splits two rooms; from the first room the second is
/// culled, from the doorway both are kept.
#[test]
fn occlusion_culling_hides_the_room_behind_a_wall() {
    use room_testkit::{MapBuilder, rect};
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .room("b", rect(512.0, 0.0, 768.0, 256.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    ensure_nodes(&mut map);
    let all = walk_bsp(&map, 128.0, 128.0).subsectors.len();
    let vis = walk_bsp_occluded(&map, 128.0, 128.0).subsectors.len();
    assert!(
        vis < all,
        "the sealed second room is culled ({vis} of {all})"
    );
}

/// A polyobject's lines never occlude: they sit in the BSP at the anchor
/// spot, not where the polyobject is drawn.
#[test]
fn polyobject_lines_never_occlude() {
    use room_testkit::{MapBuilder, rect};
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .room("b", rect(512.0, 0.0, 768.0, 256.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    ensure_nodes(&mut map);
    let all = walk_bsp(&map, 128.0, 128.0).subsectors.len();
    // Pretend room a's walls belong to a polyobject.
    let lines = (0..map.lines.len() as u32)
        .filter(|&i| {
            let l = &map.lines[i as usize];
            map.vertices[l.v1.raw() as usize].pos.x() < 300.0
        })
        .map(room_core::LineId::from_raw)
        .collect();
    map.setup.polyobjects.push(room_map::PolyobjStatic {
        id: 1,
        vertices: Vec::new(),
        lines,
        mirror: 0,
        crush: false,
    });
    let vis = walk_bsp_occluded(&map, 128.0, 128.0).subsectors.len();
    assert_eq!(vis, all, "walls of a polyobject hide nothing");
}

fn scene_counts(lists: &room_render::SceneLists) -> (usize, usize) {
    (lists.opaque_walls.len(), lists.flats.len())
}

fn camera_at(x: f32, y: f32, z: f32) -> room_render::Camera {
    room_render::Camera {
        eye: glam::Vec3::new(x, y, z),
        ..room_render::Camera::default()
    }
}

/// The scene builder culls only when asked (the main view) and only for an
/// eye inside its sector's open height; above the ceiling (a free camera)
/// it sees over every wall and keeps the whole map.
#[test]
fn scene_culls_only_main_view_with_eye_inside_the_sector() {
    use room_render::build_scene_with_sky_ex_shared as build;
    use room_testkit::{MapBuilder, rect};
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .room("b", rect(512.0, 0.0, 768.0, 256.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    ensure_nodes(&mut map);
    let inside = camera_at(128.0, 128.0, 41.0);
    let full = scene_counts(&build(&map, &inside, None, None, None, None, false));
    let culled = scene_counts(&build(&map, &inside, None, None, None, None, true));
    assert!(
        culled.0 < full.0 && culled.1 < full.1,
        "main view culls room b: {culled:?} vs {full:?}"
    );
    let above = camera_at(128.0, 128.0, 400.0);
    let over = scene_counts(&build(&map, &above, None, None, None, None, true));
    assert_eq!(
        over,
        scene_counts(&build(&map, &above, None, None, None, None, false))
    );
    assert_eq!(over, full, "an eye above the ceiling keeps every room");
    // Looking down, so only walls could cull: seen from the void every
    // solid wall faces away.
    let void = room_render::Camera {
        forward: glam::Vec3::new(0.001, 0.0, -1.0).normalize(),
        ..camera_at(384.0, 128.0, 41.0)
    };
    let outside = scene_counts(&build(&map, &void, None, None, None, None, true));
    assert_eq!(
        outside, full,
        "an eye outside the map (portal-like) keeps every room"
    );
}

/// On the counter-clockwise arc `(from, to)`.
fn on_arc(a: f64, (from, to): (f64, f64)) -> bool {
    (a - from).rem_euclid(core::f64::consts::TAU) <= to - from
}

/// View-direction culling only ever adds to wall culling, and every
/// subsector it drops is either hidden by walls or entirely outside the
/// view: no point of it in the view arc can be seen from the eye.
fn check_view_map(wad_name: &str, map_name: &str, eye_count: usize) {
    let Some(map) = load(wad_name, map_name) else {
        return;
    };
    let walls = solid_walls(&map);
    let total = map.game_nodes.as_ref().unwrap().subsectors.len();
    let (mut walls_only, mut with_view) = (0usize, 0usize);
    for (k, (x, y)) in eyes(&map, eye_count).into_iter().enumerate() {
        let yaw = k as f64 * 2.399_963; // golden angle: every heading
        let half = 0.9;
        let arc = (yaw - half, yaw + half);
        let occluded = walk_bsp_occluded(&map, x, y);
        let vis = walk_bsp_occluded_in_view(&map, x, y, Some(arc));
        assert_eq!(
            walk_bsp_occluded_in_view(&map, x, y, None).subsectors,
            occluded.subsectors,
            "no arc = walls only"
        );
        walls_only += occluded.subsectors.len();
        with_view += vis.subsectors.len();
        let mut seen = vec![false; total];
        for &ss in &vis.subsectors {
            seen[ss as usize] = true;
        }
        for &ss in &vis.subsectors {
            assert!(
                occluded.subsectors.contains(&ss),
                "{map_name}: the view arc only removes"
            );
        }
        for ss in 0..total as u32 {
            if seen[ss as usize] {
                continue;
            }
            for p in probe_points(&map, ss) {
                if !on_arc((p.1 - y).atan2(p.0 - x), arc) {
                    continue;
                }
                let blocked = walls.iter().any(|w| crosses((x, y), p, w[0], w[1]));
                assert!(
                    blocked,
                    "{map_name}: eye ({x:.1}, {y:.1}) facing {yaw:.2} sees ({:.1}, {:.1}) in culled subsector {ss}",
                    p.0, p.1
                );
            }
        }
    }
    assert!(
        with_view * 10 < walls_only * 8,
        "{map_name}: a ~100° view keeps {with_view} of {walls_only} wall-visible subsectors"
    );
}

#[test]
fn view_culling_never_hides_visible_space_map01() {
    check_view_map("freedoom2.wad", "MAP01", 24);
}

#[test]
fn view_culling_never_hides_visible_space_map20() {
    check_view_map("freedoom2.wad", "MAP20", 16);
}

#[test]
fn view_culling_never_hides_visible_space_e1m4() {
    check_view_map("freedoom1.wad", "E1M4", 24);
}

/// End to end: the main view leaves out the room behind the camera, keeps
/// it when facing it, and keeps everything when looking straight down.
#[test]
fn scene_leaves_out_what_is_behind_the_camera() {
    use room_render::build_scene_with_sky_ex_shared as build;
    use room_testkit::{MapBuilder, rect};
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .room("b", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .room("c", rect(512.0, 0.0, 768.0, 256.0), 0.0, 128.0)
        .player_start(384.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    ensure_nodes(&mut map);
    let looking = |forward: glam::Vec3| room_render::Camera {
        eye: glam::Vec3::new(384.0, 128.0, 41.0),
        forward: forward.normalize(),
        ..room_render::Camera::default()
    };
    let walls = |cam: &room_render::Camera, cull: bool| {
        build(&map, cam, None, None, None, None, cull)
            .opaque_walls
            .len()
    };
    // West and east edge (map x) of the drawn walls.
    let span = |cam: &room_render::Camera| {
        let l = build(&map, cam, None, None, None, None, true);
        l.opaque_walls
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), v| {
                (a.min(v.pos[0]), b.max(v.pos[0]))
            })
    };
    let east = looking(glam::Vec3::X);
    let all = walls(&east, false);
    let ahead = walls(&east, true);
    assert!(
        ahead < all,
        "room a, behind the camera, is left out ({ahead} of {all})"
    );
    let west = looking(-glam::Vec3::X);
    assert!(walls(&west, true) < all);
    let (e0, e1) = span(&east);
    let (w0, w1) = span(&west);
    assert!(
        e0 >= 256.0 && e1 >= 768.0,
        "facing east keeps rooms b and c: {e0}..{e1}"
    );
    assert!(
        w0 <= 0.0 && w1 <= 512.0,
        "facing west keeps rooms a and b: {w0}..{w1}"
    );
    let down = looking(glam::Vec3::new(0.001, 0.0, -1.0));
    assert_eq!(walls(&down, true), all, "looking down keeps every room");
}
