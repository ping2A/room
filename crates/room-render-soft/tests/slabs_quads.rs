//! SR6 checks (`docs/40`): 3D floors (`Sector_Set3DFloor`) — slab planes
//! from above and below, sides, things hidden under a slab, light under a
//! slab, translucent slabs —, things hidden behind a sloped rise, wall /
//! flat / rolled sprites and decals.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{
    SoftDecal, SoftFrame, SoftSlope, SoftSpriteKind, SoftTarget, SoftThing, SoftView, sprite_name,
};
use room_vfs::{Vfs, VfsBuilder};

fn fx(v: f64) -> i32 {
    (v * 65536.0) as i32
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

const FLOOR: u8 = 100;
const SLABTOP: u8 = 130;
const SLABBOT: u8 = 140;
const SIDE: u8 = 50;

/// Room `a` (0..256 square, viewer) opens on room `t` (256..512, tag 5)
/// whose 3D floor comes from the far control room `c` (floor 32, ceiling
/// 64; `SLABBOT` / `SLABTOP`, side texture `SIDE`), as `Sector_Set3DFloor
/// (5, 1, 0, alpha)` on one of `c`'s walls. `alpha` 0 = no 3D floor line.
fn setup(alpha: i32) -> (Vfs, room_map::MapGeometry) {
    let map = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .room(
            "t",
            room_testkit::rect(256.0, 0.0, 512.0, 256.0),
            0.0,
            128.0,
        )
        .room(
            "c",
            room_testkit::rect(2000.0, 2000.0, 2064.0, 2064.0),
            32.0,
            64.0,
        )
        .sector_textures("a", "FLOOR", "CEIL")
        .sector_textures("t", "FLOOR", "CEIL")
        .sector_textures("c", "SLABBOT", "SLABTOP")
        .sector_tag("t", 5)
        .sector_light("t", 64)
        .sector_light("c", 255)
        .player_start(64.0, 128.0, 0)
        .build_udmf_wad("MAP01")
        .unwrap();
    let (pn, t1) = textures(&[("WALL", 64, 128), ("SIDE", 64, 128)]);
    let res = wad(&[
        ("COLORMAP", identity_colormap()),
        ("PNAMES", pn),
        ("TEXTURE1", t1),
        ("P_START", Vec::new()),
        ("WALL", picture(64, 128, 0, 0, |_, _| 10)),
        ("SIDE", picture(64, 128, 0, 0, |_, _| SIDE)),
        ("P_END", Vec::new()),
        ("F_START", Vec::new()),
        ("FLOOR", vec![FLOOR; 4096]),
        ("CEIL", vec![120; 4096]),
        ("SLABTOP", vec![SLABTOP; 4096]),
        ("SLABBOT", vec![SLABBOT; 4096]),
        ("F_END", Vec::new()),
        ("S_START", Vec::new()),
        ("TESTA0", picture(8, 16, 4, 16, |_, _| 200)),
        (
            "FLIPA0",
            picture(8, 16, 4, 16, |_, r| if r < 8 { 201 } else { 202 }),
        ),
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
    for side in &mut geom.sides {
        side.middle = "WALL".into();
    }
    if alpha > 0 {
        let control = geom
            .lines
            .iter()
            .position(|l| {
                l.back_sector.is_none() && geom.vertices[l.v1.raw() as usize].pos.x() > 1000.0
            })
            .unwrap();
        let line = &mut geom.lines[control];
        line.special = 160;
        line.args = [5, 1, 0, alpha, 0];
        let side = line.sides[0].unwrap().raw() as usize;
        geom.sides[side].middle = "SIDE".into();
    }
    // Two-sided lines show no middle texture.
    for l in &geom.lines {
        if l.back_sector.is_some() {
            for s in l.sides.iter().flatten() {
                geom.sides[s.raw() as usize].middle = "-".into();
            }
        }
    }
    (vfs, geom)
}

fn frame(z: f64) -> SoftFrame {
    SoftFrame {
        x: fx(64.0),
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
        std::fs::write(format!("{dir}/f{n}.pgm"), out).unwrap();
    }
    t
}

fn count(t: &SoftTarget, c: u8) -> usize {
    t.pixels.iter().filter(|&&p| p == c).count()
}

fn sector_t(view: &SoftView) -> u32 {
    view.level.point_in_sector(fx(400.0), fx(128.0)).unwrap() as u32
}

/// A thing on `t`'s floor, under the slab.
fn under_slab(view: &SoftView) -> SoftThing {
    SoftThing {
        x: fx(400.0),
        y: fx(128.0),
        z: 0,
        sprite: sprite_name("TEST"),
        sector: sector_t(view),
        ..SoftThing::default()
    }
}

#[test]
fn a_3d_floor_is_seen_from_above_and_below() {
    let (vfs, plain) = setup(0);
    let (_, slab) = setup(255);
    let mut a = SoftView::new(&vfs, &plain, 320, 200).unwrap();
    let mut b = SoftView::new(&vfs, &slab, 320, 200).unwrap();
    assert!(
        b.level.ffloors.iter().any(|f| !f.is_empty()),
        "3D floor found"
    );
    // Above: the slab's top replaces t's floor; its side faces the viewer.
    let above = frame(100.0);
    let (pa, pb) = (render(&mut a, &vfs, &above), render(&mut b, &vfs, &above));
    assert_eq!(count(&pa, SLABTOP), 0);
    assert!(count(&pb, SLABTOP) > 1000, "top: {}", count(&pb, SLABTOP));
    assert!(count(&pb, SIDE) > 200, "side: {}", count(&pb, SIDE));
    assert!(
        count(&pb, FLOOR) < count(&pa, FLOOR),
        "t's floor is covered"
    );
    assert_eq!(count(&pb, SLABBOT), 0, "bottom unseen from above");
    // Below: the bottom is seen over the viewer's head, the top is not.
    let below = frame(20.0);
    let pb = render(&mut b, &vfs, &below);
    assert!(count(&pb, SLABBOT) > 300, "bottom: {}", count(&pb, SLABBOT));
    assert_eq!(count(&pb, SLABTOP), 0, "top unseen from below");
    assert!(
        count(&pb, SIDE) > 200,
        "side from below: {}",
        count(&pb, SIDE)
    );
    // Every pixel is drawn.
    let mut t = SoftTarget::new(320, 200);
    t.pixels.fill(255);
    b.renderer.render(&b.level, &b.data, &below, &mut t);
    assert!(!t.pixels.contains(&255), "no gaps");
}

#[test]
fn a_thing_under_a_slab_is_hidden_from_above() {
    let (vfs, slab) = setup(255);
    let mut view = SoftView::new(&vfs, &slab, 320, 200).unwrap();
    let thing = under_slab(&view);
    let above = SoftFrame {
        things: vec![thing],
        ..frame(100.0)
    };
    assert_eq!(count(&render(&mut view, &vfs, &above), 200), 0, "hidden");
    let below = SoftFrame {
        things: vec![thing],
        ..frame(20.0)
    };
    assert!(
        count(&render(&mut view, &vfs, &below), 200) > 20,
        "seen under it"
    );
    // Without the slab it shows from above.
    let (vfs, plain) = setup(0);
    let mut view = SoftView::new(&vfs, &plain, 320, 200).unwrap();
    assert!(count(&render(&mut view, &vfs, &above), 200) > 20);
}

#[test]
fn a_slab_lights_what_is_under_it() {
    let lum = |alpha: i32| {
        let (vfs, map) = setup(alpha);
        let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
        view.renderer.truecolor = true;
        let f = SoftFrame {
            things: vec![under_slab(&view)],
            ..frame(20.0)
        };
        let t = render(&mut view, &vfs, &f);
        let empty = render(&mut view, &vfs, &frame(20.0));
        // The thing's brightest pixel (where it changes the frame).
        let sum = |c: u32| ((c >> 16) & 255) + ((c >> 8) & 255) + (c & 255);
        (0..t.rgb.len())
            .filter(|&i| t.rgb[i] != empty.rgb[i])
            .map(|i| sum(t.rgb[i]))
            .max()
            .unwrap()
    };
    // Under the slab the thing (and t's floor) take c's light 255
    // instead of t's 64.
    assert!(lum(255) > lum(0), "{} vs {}", lum(255), lum(0));
}

#[test]
fn a_translucent_slab_blends_over_what_is_under_it() {
    let shot = |alpha: i32, truecolor: bool| {
        let (vfs, map) = setup(alpha);
        let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
        view.renderer.truecolor = truecolor;
        render(&mut view, &vfs, &frame(100.0))
    };
    let solid = shot(255, false);
    let i = solid.pixels.iter().position(|&p| p == SLABTOP).unwrap();
    let (half, opaque, none) = (shot(128, true), shot(255, true), shot(0, true));
    let c = half.rgb[i];
    assert_ne!(c, opaque.rgb[i], "not the slab alone");
    assert_ne!(c, none.rgb[i], "not the floor alone");
    let ch = |v: u32, s: u32| (v >> s) & 255;
    for s in [0, 8, 16] {
        let (lo, hi) = (
            ch(opaque.rgb[i], s).min(ch(none.rgb[i], s)),
            ch(opaque.rgb[i], s).max(ch(none.rgb[i], s)),
        );
        assert!((lo..=hi).contains(&ch(c, s)), "between the two");
    }
    // 8-bit: the nearest palette colour of the mix.
    let p8 = shot(128, false);
    assert_ne!(p8.pixels[i], SLABTOP);
}

#[test]
fn a_thing_behind_a_sloped_rise_is_hidden() {
    let (vfs, map) = setup(0);
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let thing = under_slab(&view);
    let f = SoftFrame {
        things: vec![thing],
        ..frame(20.0)
    };
    assert!(
        count(&render(&mut view, &vfs, &f), 200) > 20,
        "seen on flat floors"
    );
    // Room a rises towards +x: 48 units high at its far side (x = 256),
    // above the viewer's eye; the thing behind it in t is out of sight.
    let a = view.level.point_in_sector(fx(64.0), fx(128.0)).unwrap();
    view.level.sectors[a].floor_slope = Some(SoftSlope {
        z0: -8.0,
        dzdx: 0.21875,
        dzdy: 0.0,
    });
    let seen = count(&render(&mut view, &vfs, &f), 200);
    assert_eq!(seen, 0, "hidden by the rise: {seen}");
}

fn bam(deg: f64) -> u32 {
    ((deg.rem_euclid(360.0) / 360.0) * 4_294_967_296.0) as u64 as u32
}

/// Width and height of the box holding colour `c`.
fn extent(t: &SoftTarget, c: u8) -> (usize, usize) {
    let w = t.width as usize;
    let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
    for (i, &p) in t.pixels.iter().enumerate() {
        if p == c {
            x0 = x0.min(i % w);
            x1 = x1.max(i % w);
            y0 = y0.min(i / w);
            y1 = y1.max(i / w);
        }
    }
    if x0 > x1 {
        (0, 0)
    } else {
        (x1 - x0 + 1, y1 - y0 + 1)
    }
}

fn thing_at(view: &SoftView, x: f64, z: f64) -> SoftThing {
    SoftThing {
        x: fx(x),
        y: fx(128.0),
        z: fx(z),
        sprite: sprite_name("TEST"),
        sector: view.level.point_in_sector(fx(x), fx(128.0)).unwrap() as u32,
        ..SoftThing::default()
    }
}

fn with_things(z: f64, things: Vec<SoftThing>) -> SoftFrame {
    SoftFrame { things, ..frame(z) }
}

#[test]
fn wall_sprites_lie_in_their_plane() {
    let (vfs, map) = setup(0);
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let bill = thing_at(&view, 200.0, 0.0);
    let billboard = count(
        &render(&mut view, &vfs, &with_things(41.0, vec![bill])),
        200,
    );
    // Facing the viewer (normal along -x / +x): the billboard's size.
    let facing = SoftThing {
        kind: SoftSpriteKind::Wall,
        angle: bam(180.0),
        ..bill
    };
    let wall = count(
        &render(&mut view, &vfs, &with_things(41.0, vec![facing])),
        200,
    );
    assert!(
        wall * 10 >= billboard * 8 && wall * 10 <= billboard * 12,
        "wall {wall} vs billboard {billboard}"
    );
    // Edge on: (almost) nothing.
    let edge = SoftThing {
        angle: bam(90.0),
        ..facing
    };
    let thin = count(
        &render(&mut view, &vfs, &with_things(41.0, vec![edge])),
        200,
    );
    assert!(thin * 10 < billboard, "edge on: {thin}");
}

#[test]
fn flat_sprites_lie_on_the_floor() {
    let (vfs, map) = setup(0);
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let flat = SoftThing {
        kind: SoftSpriteKind::Flat,
        ..thing_at(&view, 120.0, 0.0)
    };
    let seen = render(&mut view, &vfs, &with_things(41.0, vec![flat]));
    let (w, h) = extent(&seen, 200);
    assert!(count(&seen, 200) > 20, "seen from above");
    // Foreshortened: much wider than tall although the picture is 8 × 16.
    assert!(w > h, "{w} × {h}");
    // At eye height it is edge on.
    let level = SoftThing {
        z: fx(41.0),
        ..flat
    };
    assert_eq!(
        count(
            &render(&mut view, &vfs, &with_things(41.0, vec![level])),
            200
        ),
        0
    );
}

#[test]
fn rolled_sprites_turn_in_the_picture_plane() {
    let (vfs, map) = setup(0);
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let t = thing_at(&view, 150.0, 30.0);
    let (w0, h0) = extent(&render(&mut view, &vfs, &with_things(41.0, vec![t])), 200);
    assert!(h0 > w0, "upright: {w0} × {h0}");
    let rolled = SoftThing {
        roll: bam(90.0),
        roll_center: true,
        ..t
    };
    let (w1, h1) = extent(
        &render(&mut view, &vfs, &with_things(41.0, vec![rolled])),
        200,
    );
    assert!(w1 > h1, "on its side: {w1} × {h1}");
    assert!(
        w1.abs_diff(h0) <= 2 && h1.abs_diff(w0) <= 2,
        "{w1}×{h1} vs {w0}×{h0}"
    );
}

#[test]
fn decals_sit_on_their_wall_part() {
    let (vfs, map) = setup(0);
    // The far wall of t, x = 512.
    let line = map
        .lines
        .iter()
        .position(|l| {
            l.back_sector.is_none()
                && map.vertices[l.v1.raw() as usize].pos.x() == 512.0
                && map.vertices[l.v2.raw() as usize].pos.x() == 512.0
        })
        .unwrap() as u32;
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let decal = SoftDecal {
        picture: "TESTA0".into(),
        x: 512.0,
        y: 128.0,
        z: 64.0,
        width: 32.0,
        height: 32.0,
        line: Some(line),
        alpha: 255,
        ..SoftDecal::default()
    };
    let f = |d: Vec<SoftDecal>| SoftFrame {
        decals: d,
        ..frame(41.0)
    };
    let full = count(&render(&mut view, &vfs, &f(vec![decal.clone()])), 200);
    assert!(full > 30, "on the wall: {full}");
    // Half past the wall's end (y = 256) is cut off.
    let edge = SoftDecal {
        y: 256.0,
        ..decal.clone()
    };
    let half = count(&render(&mut view, &vfs, &f(vec![edge])), 200);
    assert!(
        half * 10 >= full * 3 && half * 10 <= full * 7,
        "{half} of {full}"
    );
    // Half above the ceiling (128) is cut off too.
    let high = SoftDecal {
        z: 128.0,
        ..decal.clone()
    };
    let top = count(&render(&mut view, &vfs, &f(vec![high])), 200);
    assert!(
        top * 10 >= full * 3 && top * 10 <= full * 7,
        "{top} of {full}"
    );
    // A floor decal.
    let splat = SoftDecal {
        x: 200.0,
        z: 0.0,
        line: None,
        flat: true,
        ..decal
    };
    assert!(count(&render(&mut view, &vfs, &f(vec![splat])), 200) > 20);
}

#[test]
fn a_sloped_slab_top_follows_its_slope() {
    let (vfs, slab) = setup(255);
    let mut view = SoftView::new(&vfs, &slab, 320, 200).unwrap();
    let first_top = |t: &SoftTarget| {
        (0..200)
            .find(|&y| t.pixels[y * 320 + 160] == SLABTOP)
            .unwrap()
    };
    let flat = first_top(&render(&mut view, &vfs, &frame(100.0)));
    // The control ceiling (the slab's top) rises 1 in 8 towards +x from
    // 64 at x = 256.
    let c = view.level.point_in_sector(fx(2032.0), fx(2032.0)).unwrap();
    view.level.sectors[c].ceiling_slope = Some(SoftSlope {
        z0: 32.0,
        dzdx: 0.125,
        dzdy: 0.0,
    });
    let t = render(&mut view, &vfs, &frame(100.0));
    let sloped = first_top(&t);
    assert!(sloped < flat, "far edge higher: {sloped} vs {flat}");
    assert!(count(&t, SIDE) > 200, "side follows: {}", count(&t, SIDE));
}

#[test]
fn slab_planes_outside_the_sector_are_not_drawn() {
    let (vfs, slab) = setup(255);
    let mut view = SoftView::new(&vfs, &slab, 320, 200).unwrap();
    // Move the slab above t's ceiling (128): 150..170.
    let c = view.level.point_in_sector(fx(2032.0), fx(2032.0)).unwrap();
    view.level.sectors[c].floor = fx(150.0);
    view.level.sectors[c].ceiling = fx(170.0);
    let t = render(&mut view, &vfs, &frame(41.0));
    assert_eq!(count(&t, SLABBOT), 0);
}

#[test]
fn y_flipped_sprites_are_upside_down() {
    let (vfs, map) = setup(0);
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let t = SoftThing {
        sprite: sprite_name("FLIP"),
        ..thing_at(&view, 150.0, 20.0)
    };
    let top_colour = |p: &SoftTarget| *p.pixels.iter().find(|&&c| c == 201 || c == 202).unwrap();
    let upright = render(&mut view, &vfs, &with_things(41.0, vec![t]));
    assert_eq!(top_colour(&upright), 201);
    let flipped = SoftThing { yflip: true, ..t };
    let down = render(&mut view, &vfs, &with_things(41.0, vec![flipped]));
    assert_eq!(top_colour(&down), 202);
    assert_eq!(extent(&upright, 201).0, extent(&down, 201).0, "same place");
}
