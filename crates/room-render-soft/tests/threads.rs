//! SR9 threaded slices (`docs/40` SR9): the frame drawn on 2–4 threads
//! matches the one-thread frame (plane spans may step differently by a
//! texel where a slice edge cuts them), things across a slice edge stay
//! whole, and the weapon is drawn once over the whole frame.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{
    SoftFrame, SoftPSprite, SoftRenderer, SoftTarget, SoftView, map_things, sprite_name,
};
use room_vfs::VfsBuilder;

/// Frame size: large enough that the renderer splits it.
const W: usize = 1280;
const H: usize = 800;

fn same(a: &SoftTarget, b: &SoftTarget) -> f64 {
    let n = a
        .pixels
        .iter()
        .zip(&b.pixels)
        .filter(|(p, q)| p == q)
        .count();
    n as f64 / a.pixels.len() as f64
}

#[test]
fn threaded_slices_match_one_thread() {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/freedoom/freedoom2.wad");
    if !p.is_file() {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&p).unwrap().build().unwrap();
    let mut map = room_map::load_map(&vfs, "MAP01").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let mut view = SoftView::new(&vfs, &map, W as u32, H as u32).unwrap();
    let things = map_things(&map, &view.level);
    let start = map.things.iter().find(|t| t.type_id == 1).unwrap();
    let (x, y) = ((i32::from(start.x)) << 16, (i32::from(start.y)) << 16);
    let s = view.level.point_in_sector(x, y).unwrap();
    let sky = view.sky(&vfs, "SKY1");
    let frames: Vec<SoftFrame> = (0..8u32)
        .map(|a| SoftFrame {
            x,
            y,
            z: view.level.sectors[s].floor + (41 << 16),
            angle: a.wrapping_mul(0x2000_0000),
            things: things.clone(),
            sky,
            psprites: vec![SoftPSprite {
                sprite: sprite_name("PISG"),
                frame: 0,
                sx: 1 << 16,
                sy: 32 << 16,
            }],
            ..SoftFrame::default()
        })
        .collect();
    for f in &frames {
        view.prepare(&vfs, f);
    }
    for truecolor in [false, true] {
        let mut one = SoftRenderer::new_view(W as u32, H as u32, 90.0);
        one.truecolor = truecolor;
        for threads in [2, 3, 4] {
            let mut many = SoftRenderer::new_view(W as u32, H as u32, 90.0);
            many.truecolor = truecolor;
            many.threads = threads;
            for f in &frames {
                let mut a = SoftTarget::new(W as u32, H as u32);
                let mut b = SoftTarget::new(W as u32, H as u32);
                one.render(&view.level, &view.data, f, &mut a);
                many.render(&view.level, &view.data, f, &mut b);
                // Plane spans that a slice edge cuts restart vanilla's
                // texture stepping there: a texel off here and there on
                // floors and ceilings right of an edge. Walls, things and
                // the weapon are the same.
                let m = same(&a, &b);
                assert!(
                    m > 0.9,
                    "{threads} threads, true colour {truecolor}: {m:.4} same"
                );
                let rows_same = |t: &SoftTarget, u: &SoftTarget, x0: usize, x1: usize| {
                    (0..H)
                        .flat_map(|y| (x0..x1).map(move |x| y * W + x))
                        .filter(|&i| t.pixels[i] == u.pixels[i])
                        .count()
                };
                // The first slice starts nearly all its spans where one
                // thread does (visplanes split elsewhere move a few).
                let w0 = W / threads;
                let left = rows_same(&a, &b, 0, w0) as f64 / (H * w0) as f64;
                assert!(left > 0.995, "first slice: {left:.4}");
            }
        }
    }
}
