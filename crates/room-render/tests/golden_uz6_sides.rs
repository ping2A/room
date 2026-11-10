//! UZ6 (docs/36): UDMF sidedef `alpha` (overrides the linedef alpha for this
//! side's mid texture) and `blockrendering` (the viewed side occludes like a
//! one-sided wall). Golden `testdata/goldens/uz6_sides.txt`.
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-render --test golden_uz6_sides`

#![allow(clippy::unwrap_used)]

use glam::Vec3;
use room_map::MapGeometry;
use room_nodebuild::apply_nodes;
use room_render::{Camera, build_scene, walk_bsp};
use room_testkit::{LineSpecialSpec, MapBuilder, rect};
use std::fmt::Write as _;

/// Two rooms joined by a two-sided line at x = 256 (special 0 marks it).
fn two_rooms() -> MapGeometry {
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .room("b", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .special_between(
            "a",
            "b",
            LineSpecialSpec {
                special: 0,
                args: [0; 5],
                id: 0,
            },
        )
        .player_start(64.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    map
}

/// A centre room with a neighbour on each side (all joined two-sided).
fn plus_rooms() -> MapGeometry {
    let spec = || LineSpecialSpec {
        special: 0,
        args: [0; 5],
        id: 0,
    };
    let mut map = MapBuilder::new()
        .room("c", rect(256.0, 256.0, 512.0, 512.0), 0.0, 128.0)
        .room("n", rect(256.0, 512.0, 512.0, 768.0), 0.0, 128.0)
        .room("s", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .room("e", rect(512.0, 256.0, 768.0, 512.0), 0.0, 128.0)
        .room("w", rect(0.0, 256.0, 256.0, 512.0), 0.0, 128.0)
        .special_between("c", "n", spec())
        .special_between("c", "s", spec())
        .special_between("c", "e", spec())
        .special_between("c", "w", spec())
        .player_start(384.0, 384.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    map
}

fn map_line_two_sided(lines: &[room_map::Line], i: usize) -> bool {
    lines[i].back_sector.is_some()
}

/// Sector index whose bounding box holds `(x, y)` strictly inside.
fn room_sector(map: &MapGeometry, x: f64, y: f64) -> usize {
    (0..map.sectors.len())
        .find(|&si| {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for side in map.sides.iter().filter(|s| s.sector.raw() as usize == si) {
                let l = &map.lines[side.line.raw() as usize];
                for v in [l.v1, l.v2] {
                    let p = map.vertices[v.raw() as usize].pos;
                    x0 = x0.min(p.x());
                    y0 = y0.min(p.y());
                    x1 = x1.max(p.x());
                    y1 = y1.max(p.y());
                }
            }
            x > x0 && x < x1 && y > y0 && y < y1
        })
        .unwrap()
}

fn shared_line(map: &MapGeometry) -> usize {
    map.lines.iter().position(|l| l.back_sector.is_some()).unwrap()
}

#[test]
fn uz6_side_alpha_and_blockrendering_match_golden() {
    let camera = Camera {
        eye: Vec3::new(64.0, 128.0, 41.0),
        forward: Vec3::new(1.0, 0.0, 0.0),
        ..Camera::default()
    };
    let mut out = String::new();
    for (label, line_alpha, side_alpha) in [
        ("line 1.0 side -", 1.0f32, None),
        ("line 0.5 side -", 0.5, None),
        ("line 1.0 side 0.25", 1.0, Some(0.25f32)),
        ("line 0.5 side 0.75", 0.5, Some(0.75)),
    ] {
        let mut map = two_rooms();
        let li = shared_line(&map);
        map.lines[li].mid_alpha = line_alpha;
        for s in map.lines[li].sides.into_iter().flatten() {
            let side = &mut map.sides[s.raw() as usize];
            side.middle = "MIDGRATE".into();
            side.alpha = side_alpha;
        }
        let scene = build_scene(&map, &camera, None);
        let mut alphas: Vec<String> = scene
            .translucent_walls
            .iter()
            .map(|v| format!("{:.2}", v._pad[0].abs()))
            .collect();
        alphas.dedup();
        let opaque_mid = scene
            .wall_ranges
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case("MIDGRATE"));
        let _ = writeln!(
            out,
            "{label:<20} translucent_alpha={alphas:?} opaque_mid={opaque_mid}"
        );
    }
    for block in [false, true] {
        let mut map = plus_rooms();
        // Every side facing the centre room: seen from it, nothing behind
        // them is drawn, so the walk stops at the centre subsector.
        let centre_sector = room_sector(&map, 384.0, 384.0);
        for s in &mut map.sides {
            let two_sided = map_line_two_sided(&map.lines, s.line.raw() as usize);
            if two_sided && s.sector.raw() as usize == centre_sector {
                s.block_rendering = block;
            }
        }
        let vis = walk_bsp(&map, 384.0, 384.0);
        let _ = writeln!(
            out,
            "blockrendering={block:<5} visible_subsectors={}",
            vis.subsectors.len()
        );
    }
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/goldens/uz6_sides.txt");
    if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &out).unwrap();
    }
    assert_eq!(out, std::fs::read_to_string(&path).unwrap());
}

