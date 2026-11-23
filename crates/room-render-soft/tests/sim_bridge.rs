//! SR3 bridge: the playsim's actors and sectors reach the software frame
//! (`room-render-soft` feature `sim`).
#![cfg(feature = "sim")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_core::{Angle, DVec3};
use room_render_soft::SoftTarget;
use room_render_soft::sim::{SoftCamera, SoftWorldView};
use room_sim::{ClassRegistry, StateTable, World};
use room_vfs::{Vfs, VfsBuilder};
use std::sync::Arc;

/// A Doom picture: `w` × `h`, every pixel `color`, with offsets.
fn patch(w: u16, h: u8, left: i16, top: i16, color: u8) -> Vec<u8> {
    post_patch(w, h, 0, h, left, top, color)
}

/// A Doom picture whose columns hold one post: rows `from..from + len`.
#[allow(dead_code)]
fn post_patch(w: u16, h: u8, from: u8, len: u8, left: i16, top: i16, color: u8) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&u16::from(h).to_le_bytes());
    out.extend_from_slice(&left.to_le_bytes());
    out.extend_from_slice(&top.to_le_bytes());
    let cols_at = out.len();
    out.resize(cols_at + 4 * usize::from(w), 0);
    for c in 0..usize::from(w) {
        let ofs = out.len() as u32;
        out[cols_at + c * 4..cols_at + c * 4 + 4].copy_from_slice(&ofs.to_le_bytes());
        out.extend_from_slice(&[from, len, 0]);
        out.extend(std::iter::repeat_n(color, usize::from(len)));
        out.extend_from_slice(&[0, 0xff]);
    }
    out
}

fn wad(lumps: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut dir = Vec::new();
    for (name, bytes) in lumps {
        let pos = 12 + data.len() as u32;
        data.extend_from_slice(bytes);
        dir.extend_from_slice(&pos.to_le_bytes());
        dir.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        let mut n = [0u8; 8];
        n[..name.len()].copy_from_slice(name.as_bytes());
        dir.extend_from_slice(&n);
    }
    let mut out = b"PWAD".to_vec();
    out.extend_from_slice(&(lumps.len() as u32).to_le_bytes());
    out.extend_from_slice(&(12 + data.len() as u32).to_le_bytes());
    out.extend(data);
    out.extend(dir);
    out
}

/// COLORMAP: identity rows, except row 6 (fuzz) which maps everything to 1.
fn colormap() -> Vec<u8> {
    (0..34 * 256)
        .map(|i| if i / 256 == 6 { 1 } else { (i % 256) as u8 })
        .collect()
}

fn setup() -> (Vfs, World, room_map::MapGeometry) {
    let builder = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(64.0, 256.0, 0);
    let pics = wad(&[
        ("COLORMAP", colormap()),
        ("S_START", Vec::new()),
        ("POSSA0", patch(4, 8, 2, 8, 200)),
        ("POSSB0", patch(4, 8, 2, 8, 200)),
        ("S_END", Vec::new()),
    ]);
    let vfs = VfsBuilder::new()
        .mount_memory("t.wad", builder.build_udmf_wad("MAP01").unwrap())
        .unwrap()
        .mount_memory("p.wad", pics)
        .unwrap()
        .build()
        .unwrap();
    let mut map = room_map::load_map(&vfs, "MAP01").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let src = include_str!("../../../wadsrc/room/actors/doom.txt");
    let ir = room_decorate::parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let world = World::from_map_with_skill(map.clone(), classes, states, 3).unwrap();
    (vfs, world, map)
}

fn count(t: &SoftTarget, color: u8) -> usize {
    t.pixels.iter().filter(|&&p| p == color).count()
}

fn camera(world: &World) -> SoftCamera {
    let mo = world.players[0].as_ref().and_then(|p| p.mo);
    SoftCamera {
        x: 64.0,
        y: 256.0,
        z: 41.0,
        angle_deg: 0.0,
        hide: mo,
        sky: "SKY1".into(),
        ..SoftCamera::default()
    }
}

#[test]
fn playsim_actors_are_drawn_and_the_viewer_is_not() {
    let (vfs, mut world, map) = setup();
    let mut sw = SoftWorldView::new(&vfs, &map, 320, 200).unwrap();
    let cam = camera(&world);
    assert_eq!(count(&sw.draw(&vfs, &world, &map, &cam), 200), 0);
    let cid = world.classes.find_name("ZombieMan").unwrap();
    let id = world
        .spawn_at(cid, DVec3::new(192.0, 256.0, 0.0), Angle::ZERO)
        .unwrap();
    let t = sw.draw(&vfs, &world, &map, &cam);
    assert_eq!(count(&t, 200), 50, "5 × 10 pixels at 128 units");
    // The viewer's own body (here: the zombie as `hide`) is skipped.
    let mut hidden = cam.clone();
    hidden.hide = Some(id);
    assert_eq!(count(&sw.draw(&vfs, &world, &map, &hidden), 200), 0);
    let frame = sw.frame(&vfs, &world, &map, &cam);
    assert!(frame.things.iter().any(|t| &t.sprite == b"POSS"));
}

#[test]
fn sector_heights_come_from_the_playsim() {
    let (vfs, mut world, map) = setup();
    let mut sw = SoftWorldView::new(&vfs, &map, 320, 200).unwrap();
    let cam = camera(&world);
    let before = sw.frame(&vfs, &world, &map, &cam).sectors.clone().unwrap();
    world.sectors[0].ceiling_z = 96.0;
    world.sectors[0].light = 100;
    let after = sw.frame(&vfs, &world, &map, &cam).sectors.clone().unwrap();
    assert_eq!(before[0].ceiling, 128 << 16);
    assert_eq!(after[0].ceiling, 96 << 16);
    assert_eq!(after[0].light, 100);
    // A changed picture name on the render map is looked up again.
    let mut map2 = map.clone();
    map2.sectors[0].floor_pic = "F_SKY1".into();
    let sky_floor = sw.frame(&vfs, &world, &map2, &cam).sectors.clone().unwrap();
    assert_eq!(sky_floor[0].floor_pic, room_render_soft::level::SKY_FLAT);
}

#[test]
fn freedoom_e1m1_from_the_playsim() {
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
    let mut seen_things = 0;
    for angle in [0.0, 90.0, 180.0, 270.0] {
        let cam = SoftCamera {
            x: pawn.pos.x(),
            y: pawn.pos.y(),
            z: pawn.pos.z() + 41.0,
            angle_deg: angle,
            hide: Some(mo),
            sky: "SKY1".into(),
            ..SoftCamera::default()
        };
        let frame = sw.frame(&vfs, &world, &map, &cam).clone();
        assert!(frame.things.len() > 10, "map things reach the frame");
        assert!(
            frame
                .things
                .iter()
                .all(|t| t.sector < map.sectors.len() as u32)
        );
        seen_things += frame.things.len();
        let a = sw.draw(&vfs, &world, &map, &cam);
        let mut seen = [false; 256];
        for &px in &a.pixels {
            seen[usize::from(px)] = true;
        }
        assert!(seen.iter().filter(|s| **s).count() > 20, "angle {angle}");
        let rgba = a.to_rgba_fit(&sw.view.data.palette, 1280, 720);
        assert_eq!(rgba.len(), 1280 * 720 * 4);
    }
    assert!(seen_things > 0);
    // Frame cost at 640×400 (printed; the budget is checked on the reference
    // machine, `docs/40` SR3).
    let cam = SoftCamera {
        x: pawn.pos.x(),
        y: pawn.pos.y(),
        z: pawn.pos.z() + 41.0,
        angle_deg: 90.0,
        hide: Some(mo),
        sky: "SKY1".into(),
        ..SoftCamera::default()
    };
    let start = std::time::Instant::now();
    for _ in 0..20 {
        let t = sw.draw(&vfs, &world, &map, &cam);
        let _ = t.to_rgba_fit(&sw.view.data.palette, 1920, 1080);
    }
    eprintln!(
        "software 640×400 + 1080p scale: {:.2} ms/frame",
        start.elapsed().as_secs_f64() * 1000.0 / 20.0
    );
}

#[test]
fn camera_textures_show_their_camera_s_view() {
    let (vfs, mut world, mut map) = setup();
    // The west wall (x = 0) is the canvas.
    for line in &map.lines {
        let (a, b) = (
            map.vertices[line.v1.raw() as usize].pos,
            map.vertices[line.v2.raw() as usize].pos,
        );
        if a.x() == 0.0 && b.x() == 0.0 {
            let side = line.sides[0].unwrap().raw() as usize;
            map.sides[side].middle = "CAMTEX".into();
        }
    }
    let mut sw = SoftWorldView::new(&vfs, &map, 320, 200).unwrap();
    sw.set_camera_canvases([("CAMTEX".to_owned(), 64, 64)]);
    let cid = world.classes.find_name("ZombieMan").unwrap();
    // A zombie behind the viewer, and a camera looking at it.
    world
        .spawn_at(cid, DVec3::new(480.0, 256.0, 0.0), Angle::ZERO)
        .unwrap();
    let camera = world
        .spawn_at(cid, DVec3::new(300.0, 256.0, 0.0), Angle::ZERO)
        .unwrap();
    let cam = SoftCamera {
        x: 400.0,
        y: 256.0,
        z: 41.0,
        angle_deg: 180.0,
        hide: Some(camera),
        sky: "SKY1".into(),
        ..SoftCamera::default()
    };
    assert_eq!(count(&sw.draw(&vfs, &world, &map, &cam), 200), 0, "unbound");
    world.camera_textures.insert(
        "CAMTEX".into(),
        room_sim::CameraTextureBind { camera, fov: 90.0 },
    );
    let seen = count(&sw.draw(&vfs, &world, &map, &cam), 200);
    assert!(seen > 0, "the zombie on the canvas: {seen}");
}
