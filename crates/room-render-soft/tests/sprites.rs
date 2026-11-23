//! SR2 checks: things, the weapon, fuzz and player colours on a synthetic
//! room with a hand-made sprite, and Freedoom E1M1 with its map things.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{
    FF_FULLBRIGHT, MF_SHADOW, SoftFrame, SoftPSprite, SoftTarget, SoftThing, SoftView, sprite_name,
};
use room_vfs::{Vfs, VfsBuilder};
use std::path::PathBuf;

const FRAC: f64 = 65536.0;

fn fx(v: f64) -> i32 {
    (v * FRAC) as i32
}

fn bam(deg: f64) -> u32 {
    ((deg / 360.0) * 4_294_967_296.0) as u64 as u32
}

/// A Doom picture: `w` × `h`, every pixel `color`, with offsets.
fn patch(w: u16, h: u8, left: i16, top: i16, color: u8) -> Vec<u8> {
    post_patch(w, h, 0, h, left, top, color)
}

/// A Doom picture whose columns hold one post: rows `from..from + len`.
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

struct Room {
    vfs: Vfs,
    view: SoftView,
}

/// A 512² room (floor 0, ceiling 128) and sprites `TEST` (one picture,
/// 4×8 of colour 200, centred, standing on its bottom), `GRNE` (colour
/// 0x70, the green player ramp) and `ROTT` with rotations 1 and 2/8
/// (flipped) in different colours.
fn room() -> Room {
    let map = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(64.0, 256.0, 0)
        .build_udmf_wad("MAP01")
        .unwrap();
    let pics = wad(&[
        ("COLORMAP", colormap()),
        ("S_START", Vec::new()),
        ("TESTA0", patch(4, 8, 2, 8, 200)),
        ("GRNEA0", patch(4, 8, 2, 8, 0x70)),
        ("ROTTA1", patch(4, 8, 2, 8, 150)),
        ("ROTTA2A8", patch(4, 8, 0, 8, 151)),
        ("ROTTA3A7", patch(4, 8, 2, 8, 152)),
        ("ROTTA4A6", patch(4, 8, 2, 8, 153)),
        ("ROTTA5", patch(4, 8, 2, 8, 154)),
        ("S_END", Vec::new()),
    ]);
    let vfs = VfsBuilder::new()
        .mount_memory("t.wad", map)
        .unwrap()
        .mount_memory("sprites.wad", pics)
        .unwrap()
        .build()
        .unwrap();
    let mut map = room_map::load_map(&vfs, "MAP01").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    Room { vfs, view }
}

/// `TEXTURE1` / `PNAMES` for one texture made of one patch.
fn texture_lumps(tex: &str, w: i16, h: i16, patch: &str) -> (Vec<u8>, Vec<u8>) {
    let mut t1 = Vec::new();
    t1.extend_from_slice(&1i32.to_le_bytes());
    t1.extend_from_slice(&8i32.to_le_bytes());
    let mut n = [0u8; 8];
    n[..tex.len()].copy_from_slice(tex.as_bytes());
    t1.extend_from_slice(&n);
    t1.extend_from_slice(&0i32.to_le_bytes());
    t1.extend_from_slice(&w.to_le_bytes());
    t1.extend_from_slice(&h.to_le_bytes());
    t1.extend_from_slice(&0i32.to_le_bytes());
    t1.extend_from_slice(&1i16.to_le_bytes());
    for v in [0i16, 0, 0, 1, 0] {
        t1.extend_from_slice(&v.to_le_bytes());
    }
    let mut pn = 1i32.to_le_bytes().to_vec();
    let mut n = [0u8; 8];
    n[..patch.len()].copy_from_slice(patch.as_bytes());
    pn.extend_from_slice(&n);
    (t1, pn)
}

fn base_frame() -> SoftFrame {
    SoftFrame {
        x: fx(64.0),
        y: fx(256.0),
        z: fx(41.0),
        angle: 0,
        ..SoftFrame::default()
    }
}

fn thing(sprite: &str, x: f64, y: f64) -> SoftThing {
    SoftThing {
        x: fx(x),
        y: fx(y),
        z: 0,
        sprite: sprite_name(sprite),
        ..SoftThing::default()
    }
}

fn draw(r: &mut Room, f: &SoftFrame) -> SoftTarget {
    r.view.prepare(&r.vfs, f);
    r.view.render(f)
}

/// Bounding box `(x0, y0, x1, y1)` and count of pixels equal to `color`.
fn find(t: &SoftTarget, color: u8) -> Option<(usize, usize, usize, usize, usize)> {
    let w = t.width as usize;
    let mut b: Option<(usize, usize, usize, usize, usize)> = None;
    for (i, &p) in t.pixels.iter().enumerate() {
        if p == color {
            let (x, y) = (i % w, i / w);
            let e = b.get_or_insert((x, y, x, y, 0));
            e.0 = e.0.min(x);
            e.1 = e.1.min(y);
            e.2 = e.2.max(x);
            e.3 = e.3.max(y);
            e.4 += 1;
        }
    }
    b
}

#[test]
fn a_thing_is_projected_scaled_and_stands_on_the_floor() {
    let mut r = room();
    let mut f = base_frame();
    // 128 units ahead: xscale = 160 / 128 = 1.25 → 5 × 10 pixels, centred.
    f.things.push(thing("TEST", 192.0, 256.0));
    let t = draw(&mut r, &f);
    let (x0, y0, x1, y1, n) = find(&t, 200).expect("sprite drawn");
    assert!(
        (157..=158).contains(&x0) && (161..=162).contains(&x1),
        "x {x0}..{x1}"
    );
    // Bottom at the floor: centery + 41 × 1.25 ≈ 151.
    assert!((150..=152).contains(&y1), "bottom row {y1}");
    assert_eq!(y1 - y0 + 1, 10, "height");
    assert_eq!(n, (x1 - x0 + 1) * 10);
    // Behind the viewer, or past the side clip: nothing.
    let mut f = base_frame();
    f.things.push(thing("TEST", 32.0, 256.0));
    f.things.push(thing("TEST", 70.0, 500.0));
    assert!(find(&draw(&mut r, &f), 200).is_none());
}

#[test]
fn sprite_rotations_follow_the_viewer_and_flip() {
    let mut r = room();
    let frames = {
        let f = base_frame();
        r.view.prepare(
            &r.vfs,
            &SoftFrame {
                things: vec![thing("ROTT", 0.0, 0.0)],
                ..f
            },
        );
        r.view.data.sprite_frames("ROTT").unwrap().to_vec()
    };
    assert!(frames[0].rotate);
    assert_eq!(
        frames[0].lump[1], frames[0].lump[7],
        "A2A8 shares one picture"
    );
    assert!(
        !frames[0].flip[1] && frames[0].flip[7],
        "the second name is mirrored"
    );
    // Facing the viewer (angle 180 while the viewer looks along +x) is
    // rotation 1; facing away is 5; side-on 3 or 7.
    for (angle, color) in [(180.0, 150), (0.0, 154), (90.0, 152), (270.0, 152)] {
        let mut f = base_frame();
        let mut t = thing("ROTT", 192.0, 256.0);
        t.angle = bam(angle);
        f.things.push(t);
        let img = draw(&mut r, &f);
        assert!(
            find(&img, color).is_some(),
            "angle {angle} → colour {color}"
        );
    }
}

#[test]
fn nearer_things_cover_farther_ones() {
    let mut r = room();
    // A floating near thing hides a small far one completely, whichever
    // order they are listed in: the scale sort decides.
    let mut near = thing("GRNE", 160.0, 256.0);
    near.z = fx(24.0);
    let far = thing("TEST", 320.0, 256.0);
    for list in [vec![near, far], vec![far, near]] {
        let mut f = base_frame();
        f.things = list;
        let t = draw(&mut r, &f);
        assert!(find(&t, 0x70).is_some(), "near thing drawn");
        assert!(find(&t, 200).is_none(), "far thing hidden");
    }
    let mut f = base_frame();
    f.things.push(far);
    assert!(
        find(&draw(&mut r, &f), 200).is_some(),
        "far thing alone shows"
    );
}

#[test]
fn player_colours_shadow_fuzz_and_fullbright() {
    let mut r = room();
    // Translation 2 maps the green ramp to brown (0x40 + i & 0xf).
    let mut f = base_frame();
    let mut t = thing("GRNE", 192.0, 256.0);
    t.flags = 2 << room_render_soft::MF_TRANSSHIFT;
    f.things.push(t);
    let img = draw(&mut r, &f);
    assert!(find(&img, 0x70).is_none());
    assert!(find(&img, 0x40).is_some(), "translated to the brown ramp");

    // MF_SHADOW: no sprite colour, only colormap-6 (here: 1) pixels inside
    // the sprite's columns.
    let plain = draw(&mut r, &base_frame());
    let mut f = base_frame();
    let mut t = thing("TEST", 192.0, 256.0);
    t.flags = MF_SHADOW;
    f.things.push(t);
    let img = draw(&mut r, &f);
    assert!(find(&img, 200).is_none());
    let w = img.width as usize;
    let changed: Vec<usize> = (0..img.pixels.len())
        .filter(|&i| img.pixels[i] != plain.pixels[i])
        .collect();
    assert!(!changed.is_empty(), "fuzz drew something");
    for &i in &changed {
        assert_eq!(img.pixels[i], 1, "fuzz reads colormap 6");
        assert!(
            (156..=163).contains(&(i % w)),
            "fuzz stays in the sprite's columns"
        );
    }

    // Fullbright and the fixed colormap take colormap 0 / the fixed row.
    let mut f = base_frame();
    let mut t = thing("TEST", 192.0, 256.0);
    t.frame = FF_FULLBRIGHT;
    f.things.push(t);
    assert!(find(&draw(&mut r, &f), 200).is_some());
}

#[test]
fn the_weapon_is_drawn_over_everything_and_can_fuzz() {
    let mut r = room();
    let mut f = base_frame();
    // A thing right in front, then the weapon over it.
    f.things.push(thing("GRNE", 100.0, 256.0));
    f.psprites.push(SoftPSprite {
        sprite: sprite_name("TEST"),
        frame: 0,
        sx: fx(160.0),
        sy: fx(32.0),
    });
    let t = draw(&mut r, &f);
    // texturemid = 100.5 − (32 − 8) = 76.5 → top row 100 − 76.5 = 23.5.
    let (x0, y0, x1, y1, _) = find(&t, 200).expect("weapon drawn");
    assert_eq!((x0, x1), (158, 161), "4 pixels wide at 320");
    assert_eq!((y0, y1), (24, 31));
    f.psprite_fuzz = true;
    let t = draw(&mut r, &f);
    assert!(find(&t, 200).is_none(), "invisible weapon fuzzes");
}

#[test]
fn a_masked_middle_texture_covers_things_behind_it() {
    // Two rooms joined at x = 256; the joining line gets FENCE, whose
    // bottom 28 rows are solid (colour 210) and the rest a hole.
    let map = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 256.0, 512.0), 0.0, 128.0)
        .room(
            "b",
            room_testkit::rect(256.0, 0.0, 512.0, 512.0),
            0.0,
            128.0,
        )
        .player_start(64.0, 256.0, 0)
        .build_udmf_wad("MAP01")
        .unwrap();
    let (t1, pn) = texture_lumps("FENCE", 8, 128, "FENCEP");
    let pics = wad(&[
        ("COLORMAP", colormap()),
        ("PNAMES", pn),
        ("TEXTURE1", t1),
        ("P_START", Vec::new()),
        ("FENCEP", post_patch(8, 128, 100, 28, 0, 0, 210)),
        ("P_END", Vec::new()),
        ("S_START", Vec::new()),
        ("TESTA0", patch(4, 8, 2, 8, 200)),
        ("S_END", Vec::new()),
    ]);
    let vfs = VfsBuilder::new()
        .mount_memory("t.wad", map)
        .unwrap()
        .mount_memory("p.wad", pics)
        .unwrap()
        .build()
        .unwrap();
    let mut map = room_map::load_map(&vfs, "MAP01").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let mut r = Room {
        view: SoftView::new(&vfs, &map, 320, 200).unwrap(),
        vfs,
    };
    let behind = |r: &mut Room| {
        let mut f = base_frame();
        f.things.push(thing("TEST", 384.0, 256.0));
        draw(r, &f)
    };
    assert!(
        find(&behind(&mut r), 200).is_some(),
        "seen through the open line"
    );
    let fence = r.view.data.texture(&r.vfs, "FENCE");
    assert_ne!(fence, 0);
    let two_sided: Vec<u32> = r
        .view
        .level
        .segs
        .iter()
        .filter(|s| s.back.is_some())
        .map(|s| s.side)
        .collect();
    assert!(!two_sided.is_empty());
    for side in two_sided {
        r.view.level.sides[side as usize].mid = fence;
    }
    let t = behind(&mut r);
    assert!(find(&t, 210).is_some(), "fence drawn");
    assert!(find(&t, 200).is_none(), "thing behind the fence is covered");
    // The hole above the fence still shows the far room (not the fence).
    let w = t.width as usize;
    assert_ne!(t.pixels[60 * w + 160], 210);
    // In front of the fence the thing shows.
    let mut f = base_frame();
    f.things.push(thing("TEST", 192.0, 256.0));
    assert!(find(&draw(&mut r, &f), 200).is_some());
}

fn freedoom() -> Option<Vfs> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom1.wad");
    p.is_file()
        .then(|| VfsBuilder::new().mount_path(&p).unwrap().build().unwrap())
}

#[test]
fn freedoom_e1m1_with_things_and_weapon_has_no_gaps() {
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
        let things = room_render_soft::map_things(&map, &view.level);
        assert!(things.len() > 10, "E1M1 has things");
        let mut drawn = 0;
        for angle in [0.0, 45.0, 90.0, 180.0, 270.0, 300.0] {
            let f = SoftFrame {
                x: fx(sx),
                y: fx(sy),
                z: fx(41.0),
                angle: bam(angle),
                sky,
                things: things.clone(),
                psprites: vec![SoftPSprite {
                    sprite: sprite_name("PISG"),
                    frame: 0,
                    sx: fx(1.0),
                    sy: fx(32.0),
                }],
                ..SoftFrame::default()
            };
            view.prepare(&vfs, &f);
            let mut a = SoftTarget::new(w, h);
            let mut b = SoftTarget::new(w, h);
            b.pixels.fill(255);
            view.renderer.render(&view.level, &view.data, &f, &mut a);
            view.renderer.render(&view.level, &view.data, &f, &mut b);
            let gaps = a
                .pixels
                .iter()
                .zip(&b.pixels)
                .filter(|(p, q)| p != q)
                .count();
            assert_eq!(gaps, 0, "{w}×{h} angle {angle}: {gaps} pixels not drawn");
            let bare = SoftFrame {
                things: Vec::new(),
                psprites: Vec::new(),
                ..f.clone()
            };
            let mut c = SoftTarget::new(w, h);
            view.renderer.render(&view.level, &view.data, &bare, &mut c);
            assert_ne!(a.pixels, c.pixels, "{w}×{h} angle {angle}: weapon drawn");
            drawn += 1;
        }
        assert_eq!(drawn, 6);
    }
}
