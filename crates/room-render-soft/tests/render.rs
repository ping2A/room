//! Software renderer checks: a synthetic room (no IWAD needed) and, when
//! `testdata/freedoom/freedoom1.wad` is present, full Freedoom views with no
//! hall-of-mirrors gaps.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{SoftFrame, SoftTarget, SoftView};
use room_vfs::{Vfs, VfsBuilder};
use std::path::PathBuf;

fn frame(x: f64, y: f64, z: f64, angle_deg: f64, sky: u16) -> SoftFrame {
    let fx = |v: f64| (v * 65536.0) as i32;
    SoftFrame {
        x: fx(x),
        y: fx(y),
        z: fx(z),
        angle: ((angle_deg / 360.0) * 4_294_967_296.0) as u64 as u32,
        extralight: 0,
        fixed_colormap: None,
        sectors: None,
        sky,
        ..SoftFrame::default()
    }
}

/// Render into a target pre-filled with `fill`.
fn draw(view: &mut SoftView, f: &SoftFrame, fill: u8) -> SoftTarget {
    let mut t = SoftTarget::new(view.renderer.width(), view.renderer.height());
    t.pixels.fill(fill);
    view.renderer.render(&view.level, &view.data, f, &mut t);
    t
}

#[test]
fn synthetic_room_floor_and_ceiling_fill_their_halves() {
    let wad = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(256.0, 256.0, 0)
        .build_udmf_wad("MAP01")
        .unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("t.wad", wad)
        .unwrap()
        .build()
        .unwrap();
    let mut map = room_map::load_map(&vfs, "MAP01").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let t = draw(&mut view, &frame(256.0, 256.0, 41.0, 0.0, 0), 7);
    // No PLAYPAL / COLORMAP / flats: identity colormap, grey (96) flats.
    let px = |x: usize, y: usize| t.pixels[y * 320 + x];
    for x in [0, 100, 160, 319] {
        assert_eq!(px(x, 0), 96, "ceiling at column {x}");
        assert_eq!(px(x, 199), 96, "floor at column {x}");
    }
    // Untextured walls draw nothing (vanilla), so the band around the
    // horizon keeps the fill colour.
    assert_eq!(px(160, 100), 7);
}

fn freedoom() -> Option<Vfs> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom1.wad");
    p.is_file()
        .then(|| VfsBuilder::new().mount_path(&p).unwrap().build().unwrap())
}

#[test]
fn freedoom_e1m1_views_have_no_gaps_and_are_repeatable() {
    let Some(vfs) = freedoom() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    let mut map = room_map::load_map(&vfs, "E1M1").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let start = map.things.iter().find(|t| t.type_id == 1).unwrap();
    let (sx, sy) = (f64::from(start.x), f64::from(start.y));
    for (w, h) in [(320, 200), (320, 168), (640, 400)] {
        let mut view = SoftView::new(&vfs, &map, w, h).unwrap();
        let sky = view.sky(&vfs, "SKY1");
        for angle in [0.0, 45.0, 90.0, 180.0, 270.0, 300.0] {
            let f = frame(sx, sy, 41.0, angle, sky);
            // Every pixel written: two different fills give the same frame.
            let a = draw(&mut view, &f, 0);
            let b = draw(&mut view, &f, 255);
            let gaps = a
                .pixels
                .iter()
                .zip(&b.pixels)
                .filter(|(p, q)| p != q)
                .count();
            assert_eq!(gaps, 0, "{w}×{h} angle {angle}: {gaps} pixels not drawn");
            let mut seen = [false; 256];
            for &p in &a.pixels {
                seen[usize::from(p)] = true;
            }
            assert!(
                seen.iter().filter(|s| **s).count() > 20,
                "{w}×{h} angle {angle}: too flat"
            );
            // Light: darker far away — a whole frame is never one colormap.
            let c = draw(&mut view, &f, 0);
            assert_eq!(a.pixels, c.pixels, "repeatable");
        }
    }
}

#[test]
fn freedoom_invulnerability_uses_the_fixed_colormap() {
    let Some(vfs) = freedoom() else {
        return;
    };
    let mut map = room_map::load_map(&vfs, "E1M1").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let start = map.things.iter().find(|t| t.type_id == 1).unwrap();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let mut f = frame(f64::from(start.x), f64::from(start.y), 41.0, 90.0, 0);
    f.fixed_colormap = Some(32);
    let t = draw(&mut view, &f, 0);
    // Every wall / flat pixel is a value of colormap 32 (the inverse map).
    let inv: std::collections::HashSet<u8> = view.data.colormap(32).iter().copied().collect();
    let off = t.pixels.iter().filter(|p| !inv.contains(p)).count();
    assert_eq!(off, 0, "{off} pixels outside the invulnerability colormap");
}
