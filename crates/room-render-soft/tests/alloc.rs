//! SR9: no allocation per frame in the steady state (`docs/40` SR9). A
//! counting allocator around the renderer's frame on Freedoom 2 MAP01.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// A counting global allocator needs `unsafe` (test-only).
#![allow(unsafe_code)]

use room_render_soft::{SoftFrame, SoftRenderer, SoftTarget, SoftView, map_things};
use room_vfs::VfsBuilder;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator, counting calls.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn frames_allocate_nothing_once_warm() {
    renderer_frames();
    #[cfg(feature = "sim")]
    bridge_frames();
}

/// The renderer alone (Freedoom 2 MAP01).
fn renderer_frames() {
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
            ..SoftFrame::default()
        })
        .collect();
    for f in &frames {
        view.prepare(&vfs, f);
    }
    for truecolor in [false, true] {
        let mut r = SoftRenderer::new_view(640, 400, 90.0);
        r.truecolor = truecolor;
        view.renderer = r;
        let mut target = SoftTarget::new(640, 400);
        // Warm up: every view once (buffers grow to their size).
        for _ in 0..2 {
            for f in &frames {
                view.renderer
                    .render(&view.level, &view.data, f, &mut target);
            }
        }
        let before = ALLOCS.load(Ordering::Relaxed);
        for f in &frames {
            view.renderer
                .render(&view.level, &view.data, f, &mut target);
        }
        let n = ALLOCS.load(Ordering::Relaxed) - before;
        assert_eq!(
            n, 0,
            "allocations in 8 warm frames (true colour {truecolor})"
        );
    }
}

/// The playsim bridge as the shells call it (Freedoom 1 E1M1), into a
/// reused RGBA buffer.
#[cfg(feature = "sim")]
fn bridge_frames() {
    use room_render_soft::sim::{SoftCamera, SoftWorldView};
    use room_sim::{ClassRegistry, StateTable, World};
    use std::sync::Arc;
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/freedoom/freedoom1.wad");
    if !p.is_file() {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&p).unwrap().build().unwrap();
    let mut map = room_map::load_map(&vfs, "E1M1").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let src = include_str!("../../../wadsrc/room/actors/doom.txt");
    let ir = room_decorate::parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let world = World::from_map_with_skill(map.clone(), classes, states, 3).unwrap();
    let mo = world.players[0].as_ref().and_then(|p| p.mo).unwrap();
    let pawn = world.actors.get(mo).unwrap();
    let mut sw = SoftWorldView::new(&vfs, &world.map, 640, 400).unwrap();
    let cams: Vec<SoftCamera> = [0.0, 90.0, 180.0, 270.0]
        .iter()
        .map(|&a| SoftCamera {
            x: pawn.pos.x(),
            y: pawn.pos.y(),
            z: pawn.pos.z() + 41.0,
            angle_deg: a,
            hide: Some(mo),
            sky: "SKY1".into(),
            ..SoftCamera::default()
        })
        .collect();
    let mut rgba = Vec::new();
    for truecolor in [false, true] {
        sw.set_view(640, 400, 90.0, truecolor);
        for _ in 0..2 {
            for c in &cams {
                sw.draw_fit(&vfs, &world, &map, c, 1280, 720, &mut rgba);
            }
        }
        let before = ALLOCS.load(Ordering::Relaxed);
        for c in &cams {
            sw.draw_fit(&vfs, &world, &map, c, 1280, 720, &mut rgba);
        }
        let n = ALLOCS.load(Ordering::Relaxed) - before;
        assert_eq!(
            n, 0,
            "bridge allocations in 4 warm frames (true colour {truecolor})"
        );
        assert_eq!(rgba.len(), 1280 * 720 * 4);
    }
}
