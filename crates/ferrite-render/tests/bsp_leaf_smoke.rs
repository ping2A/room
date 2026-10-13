//! BSP leaf polygon smoke: clipped leaf must contain its own seg midpoints.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ferrite_map::load_map;
use ferrite_nodebuild::ensure_nodes;
use ferrite_render::{VisibleSet, build_scene_from_visible};
use ferrite_vfs::VfsBuilder;
use std::path::PathBuf;

#[test]
fn e1m1_bsp_leaves_cover_own_seg_mids() {
    let wad = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    if !wad.is_file() {
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let nodes = map.game_nodes.as_ref().unwrap();
    let vis = VisibleSet {
        subsectors: (0..nodes.subsectors.len() as u32).collect(),
    };
    let scene = build_scene_from_visible(&map, &vis);
    // Count floor tris and how many ss have flats near their seg midpoint
    let mut miss = 0u32;
    let mut checked = 0u32;
    for (ssi, sub) in nodes.subsectors.iter().enumerate() {
        let start = sub.first_seg as usize;
        let segs = &nodes.segs[start..start + sub.num_segs as usize];
        if segs.is_empty() {
            continue;
        }
        let mut mx = 0.0;
        let mut my = 0.0;
        let mut n = 0.0;
        for s in segs {
            let a = &map.vertices[s.v1.raw() as usize];
            let b = &map.vertices[s.v2.raw() as usize];
            mx += (a.pos.x() + b.pos.x()) * 0.5;
            my += (a.pos.y() + b.pos.y()) * 0.5;
            n += 1.0;
        }
        mx /= n;
        my /= n;
        // nudge toward sector interior ~4 units using first seg normal
        let s0 = &segs[0];
        let a = &map.vertices[s0.v1.raw() as usize];
        let b = &map.vertices[s0.v2.raw() as usize];
        let dx = b.pos.x() - a.pos.x();
        let dy = b.pos.y() - a.pos.y();
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        // front of seg faces sector interior (right-hand relative to seg dir)
        let nx = dy / len;
        let ny = -dx / len;
        let px = mx + nx * 4.0;
        let py = my + ny * 4.0;
        checked += 1;
        let hit = scene.flats.chunks_exact(3).any(|tri| {
            let ax = f64::from(tri[0].pos[0]);
            let ay = f64::from(tri[0].pos[1]);
            let bx = f64::from(tri[1].pos[0]);
            let by = f64::from(tri[1].pos[1]);
            let cx = f64::from(tri[2].pos[0]);
            let cy = f64::from(tri[2].pos[1]);
            let nz = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
            if nz <= 0.0 {
                return false;
            }
            let sign = |p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)| {
                (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
            };
            let p = (px, py);
            let b1 = sign(p, (ax, ay), (bx, by)) < 0.0;
            let b2 = sign(p, (bx, by), (cx, cy)) < 0.0;
            let b3 = sign(p, (cx, cy), (ax, ay)) < 0.0;
            b1 == b2 && b2 == b3
        });
        if !hit {
            miss += 1;
            if miss <= 8 {
                eprintln!("ss {ssi} mid-nudge ({px:.0},{py:.0}) no floor");
            }
        }
    }
    eprintln!("seg-mid floor cover: checked={checked} miss={miss}");
    assert!(
        miss * 10 < checked,
        "BSP leaf flats miss too many seg mids: {miss}/{checked}"
    );
}
