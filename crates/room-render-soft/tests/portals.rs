//! SR7 checks (`docs/40`): portals and skies — a line portal shows the room
//! it leads to, a mirror shows the room reversed, a horizon runs the planes
//! to the horizon, a sky viewpoint and a stacked sector show other places on
//! planes, nested mirrors stop at the depth limit, Hexen double skies.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{
    SoftFrame, SoftLinePortal, SoftSectorPortal, SoftTarget, SoftThing, SoftView, sprite_name,
};
use room_vfs::{Vfs, VfsBuilder};

fn fx(v: f64) -> i32 {
    (v * 65536.0) as i32
}

#[allow(dead_code)]
fn bam(deg: f64) -> u32 {
    ((deg.rem_euclid(360.0) / 360.0) * 4_294_967_296.0) as u64 as u32
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

/// A Doom picture `w` × `h`; pixel colour `color(column, row)`; offsets.
fn picture(w: u16, h: u8, left: i16, top: i16, color: impl Fn(usize, usize) -> u8) -> Vec<u8> {
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
        out.extend_from_slice(&[0, h, 0]);
        out.extend((0..usize::from(h)).map(|r| color(c, r)));
        out.extend_from_slice(&[0, 0xff]);
    }
    out
}

/// `PNAMES` + `TEXTURE1`: one single-patch texture per `(name, w, h)`.
fn textures(list: &[(&str, i16, i16)]) -> (Vec<u8>, Vec<u8>) {
    let mut pn = (list.len() as i32).to_le_bytes().to_vec();
    for (name, _, _) in list {
        let mut n = [0u8; 8];
        n[..name.len()].copy_from_slice(name.as_bytes());
        pn.extend_from_slice(&n);
    }
    let mut t1 = (list.len() as i32).to_le_bytes().to_vec();
    let head = 4 + 4 * list.len();
    for i in 0..list.len() {
        t1.extend_from_slice(&((head + i * 32) as i32).to_le_bytes());
    }
    for (i, (name, w, h)) in list.iter().enumerate() {
        let mut n = [0u8; 8];
        n[..name.len()].copy_from_slice(name.as_bytes());
        t1.extend_from_slice(&n);
        t1.extend_from_slice(&0i32.to_le_bytes());
        t1.extend_from_slice(&w.to_le_bytes());
        t1.extend_from_slice(&h.to_le_bytes());
        t1.extend_from_slice(&0i32.to_le_bytes());
        t1.extend_from_slice(&1i16.to_le_bytes());
        for v in [0i16, 0, i as i16, 1, 0] {
            t1.extend_from_slice(&v.to_le_bytes());
        }
    }
    (pn, t1)
}

fn identity_colormap() -> Vec<u8> {
    (0..34 * 256).map(|i| (i % 256) as u8).collect()
}

const AWALL: u8 = 30;
const BWALL: u8 = 60;
const WWALL: u8 = 90;
const FLOORA: u8 = 100;
const FLOORB: u8 = 110;
const CEILA: u8 = 120;
const SKY: u8 = 150;
const SKY2: u8 = 160;

/// Builds `m` with the test pictures: room A (x < 1000) walls `AWALL`, its
/// west wall (x = 0) `WWALL`, room B walls `BWALL`; floors `FLOORA` /
/// `FLOORB`, ceilings `CEILA`; `SKY1` (top half colour 0) and `SKY2`.
fn load(m: &room_testkit::PolyMap) -> (Vfs, room_map::MapGeometry) {
    let map = wad(&[
        ("MAP01", Vec::new()),
        ("TEXTMAP", m.textmap().into_bytes()),
        ("ENDMAP", Vec::new()),
    ]);
    let (pn, t1) = textures(&[
        ("AWALL", 64, 128),
        ("BWALL", 64, 128),
        ("WWALL", 64, 128),
        ("SKY1", 256, 128),
        ("SKY2", 256, 128),
    ]);
    let res = wad(&[
        ("COLORMAP", identity_colormap()),
        ("PNAMES", pn),
        ("TEXTURE1", t1),
        ("P_START", Vec::new()),
        ("AWALL", picture(64, 128, 0, 0, |_, _| AWALL)),
        ("BWALL", picture(64, 128, 0, 0, |_, _| BWALL)),
        ("WWALL", picture(64, 128, 0, 0, |_, _| WWALL)),
        (
            "SKY1",
            picture(256, 128, 0, 0, |_, r| if r < 64 { 0 } else { SKY }),
        ),
        ("SKY2", picture(256, 128, 0, 0, |_, _| SKY2)),
        ("P_END", Vec::new()),
        ("F_START", Vec::new()),
        ("FLOORA", vec![FLOORA; 4096]),
        ("FLOORB", vec![FLOORB; 4096]),
        ("CEILA", vec![CEILA; 4096]),
        ("F_SKY1", vec![1; 4096]),
        ("F_END", Vec::new()),
        ("S_START", Vec::new()),
        ("TESTA0", picture(8, 16, 4, 16, |_, _| 200)),
        ("S_END", Vec::new()),
    ]);
    let vfs = VfsBuilder::new()
        .mount_memory("t.wad", map)
        .unwrap()
        .mount_memory("r.wad", res)
        .unwrap()
        .build()
        .unwrap();
    let mut geom = room_map::load_map(&vfs, "MAP01").unwrap();
    room_nodebuild::ensure_nodes(&mut geom);
    for (i, line) in geom.lines.clone().iter().enumerate() {
        let (a, b) = (
            geom.vertices[line.v1.raw() as usize].pos,
            geom.vertices[line.v2.raw() as usize].pos,
        );
        let name = if a.x() > 1000.0 {
            "BWALL"
        } else if a.x() == 0.0 && b.x() == 0.0 {
            "WWALL"
        } else {
            "AWALL"
        };
        for s in line.sides.iter().flatten() {
            geom.sides[s.raw() as usize].middle = if line.back_sector.is_some() {
                "-".into()
            } else {
                name.into()
            };
        }
        let _ = i;
    }
    for s in &mut geom.sectors {
        if s.floor_pic == "FLOOR0_1" {
            s.floor_pic = "FLOORA".into();
        }
        if s.ceiling_pic == "CEIL1_1" {
            s.ceiling_pic = "CEILA".into();
        }
    }
    (vfs, geom)
}

/// Room A 0..256 × 0..256 alone, and room B far away (1024..1280).
fn two_rooms() -> room_testkit::PolyMap {
    let mut m = room_testkit::PolyMap::new();
    m.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 0);
    let b = m.rect(1024.0, 0.0, 1280.0, 256.0, 0.0, 128.0, 0);
    m.textures(b, "FLOORB", "CEILA");
    m.player(128.0, 128.0, 0);
    m
}

fn frame(x: f64, z: f64) -> SoftFrame {
    SoftFrame {
        x: fx(x),
        y: fx(128.0),
        z: fx(z),
        angle: 0,
        ..SoftFrame::default()
    }
}

fn render(view: &mut SoftView, vfs: &Vfs, f: &SoftFrame) -> SoftTarget {
    view.prepare(vfs, f);
    let t = view.render(f);
    if let Ok(dir) = std::env::var("FFDUMP") {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let mut out = format!("P5 {} {} 255\n", t.width, t.height).into_bytes();
        out.extend(&t.pixels);
        std::fs::write(format!("{dir}/p{n}.pgm"), out).unwrap();
    }
    t
}

fn count(t: &SoftTarget, c: u8) -> usize {
    t.pixels.iter().filter(|&&p| p == c).count()
}

/// The line from `a` to `b` (either way round) and its front side.
fn line_at(map: &room_map::MapGeometry, a: (f64, f64), b: (f64, f64)) -> (usize, u32) {
    let i = map
        .lines
        .iter()
        .position(|l| {
            let p = map.vertices[l.v1.raw() as usize].pos;
            let q = map.vertices[l.v2.raw() as usize].pos;
            let at = |v: room_core::DVec2, w: (f64, f64)| v.x() == w.0 && v.y() == w.1;
            (at(p, a) && at(q, b)) || (at(p, b) && at(q, a))
        })
        .unwrap();
    (i, map.lines[i].sides[0].unwrap().raw())
}

#[test]
fn a_line_portal_shows_the_room_it_leads_to() {
    let (vfs, map) = load(&room_testkit::linked_line_portal_rooms(0));
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let visual = view
        .level
        .line_portals
        .iter()
        .flatten()
        .filter(|(p, _)| matches!(p, SoftLinePortal::Visual { .. }))
        .count();
    assert_eq!(visual, 2, "both portal lines");
    let t = render(&mut view, &vfs, &frame(128.0, 41.0));
    // Room B's far wall (x = 1280) through the portal at x = 256.
    assert!(count(&t, BWALL) > 1000, "B seen: {}", count(&t, BWALL));
    // Without the portal: the strip behind A's east wall.
    view.level.line_portals.iter_mut().for_each(|p| *p = None);
    let t = render(&mut view, &vfs, &frame(128.0, 41.0));
    assert_eq!(count(&t, BWALL), 0);
}

#[test]
fn a_mirror_shows_the_room_reversed() {
    let (vfs, map) = load(&two_rooms());
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let thing = SoftThing {
        x: fx(200.0),
        y: fx(60.0),
        z: 0,
        sprite: sprite_name("TEST"),
        sector: view.level.point_in_sector(fx(200.0), fx(60.0)).unwrap() as u32,
        ..SoftThing::default()
    };
    let f = SoftFrame {
        things: vec![thing],
        ..frame(128.0, 41.0)
    };
    let plain = render(&mut view, &vfs, &f);
    assert_eq!(count(&plain, WWALL), 0, "the west wall is behind");
    let (li, front) = line_at(&map, (256.0, 0.0), (256.0, 256.0));
    view.level.line_portals[li] = Some((SoftLinePortal::Mirror, front));
    let t = render(&mut view, &vfs, &f);
    assert!(
        count(&t, WWALL) > 500,
        "west wall in the mirror: {}",
        count(&t, WWALL)
    );
    // The thing (to the right) and its reflection (smaller, further) are
    // both right of centre.
    let cols: Vec<usize> = (0..t.pixels.len())
        .filter(|&i| t.pixels[i] == 200)
        .map(|i| i % 320)
        .collect();
    assert!(
        cols.len() > count(&plain, 200),
        "the reflection adds pixels"
    );
    assert!(cols.iter().all(|&x| x > 160), "all on the right");
}

#[test]
fn facing_mirrors_stop_at_the_depth_limit() {
    let (vfs, map) = load(&two_rooms());
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    for (a, b) in [((256.0, 0.0), (256.0, 256.0)), ((0.0, 0.0), (0.0, 256.0))] {
        let (li, front) = line_at(&map, a, b);
        view.level.line_portals[li] = Some((SoftLinePortal::Mirror, front));
    }
    let t = render(&mut view, &vfs, &frame(128.0, 41.0));
    assert!(count(&t, FLOORA) > 1000);
    assert_eq!(room_render_soft::MAX_PORTAL_DEPTH, 3);
}

#[test]
fn a_horizon_runs_the_planes_to_the_horizon() {
    let (vfs, map) = load(&two_rooms());
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let t = render(&mut view, &vfs, &frame(128.0, 41.0));
    assert_eq!(t.pixels[101 * 320 + 160], AWALL);
    let (li, front) = line_at(&map, (256.0, 0.0), (256.0, 256.0));
    view.level.line_portals[li] = Some((SoftLinePortal::Horizon, front));
    let t = render(&mut view, &vfs, &frame(128.0, 41.0));
    assert_eq!(
        t.pixels[101 * 320 + 160],
        FLOORA,
        "floor just below the horizon"
    );
    assert_eq!(t.pixels[98 * 320 + 160], CEILA, "ceiling just above it");
    assert!(count(&t, AWALL) * 10 < 64000);
}

#[test]
fn a_sky_viewpoint_shows_another_place_in_the_sky() {
    let mut m = two_rooms();
    m.textures(0, "FLOORA", "F_SKY1");
    let (vfs, map) = load(&m);
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let sky = view.sky(&vfs, "SKY1");
    let f = SoftFrame {
        sky,
        ..frame(128.0, 120.0)
    };
    let plain = render(&mut view, &vfs, &f);
    assert!(count(&plain, SKY) > 100, "sky");
    let a = view.level.point_in_sector(fx(128.0), fx(128.0)).unwrap();
    let b = view.level.point_in_sector(fx(1152.0), fx(128.0)).unwrap();
    view.level.sector_portals.push(SoftSectorPortal::Skybox {
        x: 1152.0,
        y: 128.0,
        z_offset: 41.0,
        sector: Some(b as u32),
        angle: 0,
    });
    view.level.plane_portals[a][1] = Some((0, true));
    let t = render(&mut view, &vfs, &f);
    assert_eq!(count(&t, SKY), 0, "the sky is replaced");
    assert!(
        count(&t, BWALL) > 100,
        "room B in the sky: {}",
        count(&t, BWALL)
    );
}

#[test]
fn a_stacked_sector_shows_the_other_side_of_the_floor() {
    let (vfs, map) = load(&two_rooms());
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let a = view.level.point_in_sector(fx(128.0), fx(128.0)).unwrap();
    view.level.sector_portals.push(SoftSectorPortal::Stacked {
        dx: 1024.0,
        dy: 0.0,
    });
    view.level.plane_portals[a][0] = Some((0, false));
    let t = render(&mut view, &vfs, &frame(128.0, 41.0));
    assert_eq!(count(&t, FLOORA), 0);
    assert!(count(&t, FLOORB) > 1000, "B's floor: {}", count(&t, FLOORB));
}

#[test]
fn hexen_double_skies_show_both_layers() {
    let mut m = two_rooms();
    m.textures(0, "FLOORA", "F_SKY1");
    let (vfs, map) = load(&m);
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let (sky, sky2) = (view.sky(&vfs, "SKY1"), view.sky(&vfs, "SKY2"));
    let f = SoftFrame {
        sky,
        ..frame(128.0, 120.0)
    };
    let one = render(&mut view, &vfs, &f);
    assert_eq!(count(&one, SKY2), 0);
    let two = render(
        &mut view,
        &vfs,
        &SoftFrame {
            sky2: Some(sky2),
            ..f.clone()
        },
    );
    assert!(count(&two, SKY2) > 50, "back sky through the gaps");
    assert!(count(&two, SKY) > 50, "front sky");
    assert_eq!(count(&two, 0), 0, "no clear pixels left");
}
