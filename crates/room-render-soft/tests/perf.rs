//! SR9 frame cost (`docs/40` SR9): Freedoom 2 MAP01 from a set of views,
//! at 640×400 and 1920×1080, 8-bit and true colour. Ignored by default
//! (timing); run with `cargo test --release -p room-render-soft --test perf
//! -- --ignored --nocapture`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{SoftFrame, SoftRenderer, SoftTarget, SoftView, map_things};
use room_vfs::VfsBuilder;

/// Views: the player start and map things' places, four angles each.
fn views(map: &room_map::MapGeometry, view: &SoftView) -> Vec<SoftFrame> {
    let things = map_things(map, &view.level);
    let mut spots: Vec<(f64, f64)> = map
        .things
        .iter()
        .filter(|t| t.type_id == 1)
        .map(|t| (f64::from(t.x), f64::from(t.y)))
        .collect();
    spots.extend(
        map.things
            .iter()
            .step_by(9)
            .map(|t| (f64::from(t.x), f64::from(t.y))),
    );
    spots.truncate(8);
    let mut out = Vec::new();
    for (x, y) in spots {
        let (fx, fy) = ((x * 65536.0) as i32, (y * 65536.0) as i32);
        let Some(s) = view.level.point_in_sector(fx, fy) else {
            continue;
        };
        let z = view.level.sectors[s].floor + (41 << 16);
        for a in 0..4u32 {
            out.push(SoftFrame {
                x: fx,
                y: fy,
                z,
                angle: a.wrapping_mul(0x4000_0000).wrapping_add(0x1000_0000),
                things: things.clone(),
                ..SoftFrame::default()
            });
        }
    }
    out
}

fn bench(view: &mut SoftView, frames: &[SoftFrame], w: u32, h: u32, truecolor: bool) -> f64 {
    let mut r = SoftRenderer::new_view(w, h, 90.0);
    r.truecolor = truecolor;
    r.threads = std::env::var("SOFT_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    view.renderer = r;
    let mut target = SoftTarget::new(w, h);
    // Warm up, then time.
    for f in frames {
        view.renderer
            .render(&view.level, &view.data, f, &mut target);
    }
    let start = std::time::Instant::now();
    let rounds = 3;
    for _ in 0..rounds {
        for f in frames {
            view.renderer
                .render(&view.level, &view.data, f, &mut target);
        }
    }
    start.elapsed().as_secs_f64() * 1000.0 / (rounds * frames.len()) as f64
}

#[test]
#[ignore = "timing; run by hand"]
fn freedoom2_map01_frame_cost() {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/freedoom/freedoom2.wad");
    if !p.is_file() {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&p).unwrap().build().unwrap();
    let mut map = room_map::load_map(&vfs, "MAP01").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let mut view = SoftView::new(&vfs, &map, 640, 400).unwrap();
    let frames = views(&map, &view);
    for f in &frames {
        view.prepare(&vfs, f);
    }
    let sky = view.sky(&vfs, "SKY1");
    let frames: Vec<SoftFrame> = frames.into_iter().map(|f| SoftFrame { sky, ..f }).collect();
    let sizes: &[(u32, u32)] = if std::env::var("PERF_ONE").is_ok() {
        &[(1920, 1080)]
    } else {
        &[(640, 400), (1920, 1080)]
    };
    for &(w, h) in sizes {
        for tc in [false, true] {
            if std::env::var("PERF_ONE").is_ok() && tc != std::env::var("PERF_TC").is_ok() {
                continue;
            }
            let ms = bench(&mut view, &frames, w, h, tc);
            eprintln!(
                "MAP01 {w}×{h} {}: {ms:.2} ms/frame ({:.0} fps), {} views",
                if tc { "true colour" } else { "8-bit" },
                1000.0 / ms,
                frames.len()
            );
        }
    }
}
