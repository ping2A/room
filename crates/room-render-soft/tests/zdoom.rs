//! SR6 checks (`docs/40`): ZDoom map features in the software renderer —
//! UDMF wall and flat scaling / rotation, slopes, polyobjects, render styles,
//! sprite scaling and mirroring.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{
    FlatXform, SoftFrame, SoftSlope, SoftStyle, SoftTarget, SoftThing, SoftView, sprite_name,
};
use room_vfs::{Vfs, VfsBuilder};

fn fx(v: f64) -> i32 {
    (v * 65536.0) as i32
}

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

/// Stripes: columns 0..32 colour 10, 32..64 colour 20.
fn stripes(c: usize, _r: usize) -> u8 {
    if c % 64 < 32 { 10 } else { 20 }
}

/// Room `a` 0..512 square (floor 0, ceiling 128, FLOOR flat with x
/// stripes) and a small closed room `box` far away at 1024..1056, every
/// wall STRIPES, plus a sprite TEST (4 × 8: left half 200, right half 201).
fn setup() -> (Vfs, room_map::MapGeometry) {
    let map = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .room(
            "box",
            room_testkit::rect(1024.0, 1024.0, 1056.0, 1056.0),
            0.0,
            128.0,
        )
        .sector_textures("a", "FLOOR", "CEIL")
        .sector_textures("box", "FLOOR", "CEIL")
        .player_start(64.0, 256.0, 0)
        .build_udmf_wad("MAP01")
        .unwrap();
    let (pn, t1) = textures(&[("STRIPES", 64, 128), ("BOXWALL", 64, 128)]);
    let mut floor = vec![0u8; 4096];
    for (i, p) in floor.iter_mut().enumerate() {
        *p = if i % 64 < 32 { 100 } else { 110 };
    }
    let res = wad(&[
        ("COLORMAP", identity_colormap()),
        ("PNAMES", pn),
        ("TEXTURE1", t1),
        ("P_START", Vec::new()),
        ("STRIPES", picture(64, 128, 0, 0, stripes)),
        ("BOXWALL", picture(64, 128, 0, 0, |_, _| 77)),
        ("P_END", Vec::new()),
        ("F_START", Vec::new()),
        ("FLOOR", floor),
        ("CEIL", vec![120; 4096]),
        ("F_END", Vec::new()),
        ("S_START", Vec::new()),
        (
            "TESTA0",
            picture(4, 8, 2, 8, |c, _| if c < 2 { 200 } else { 201 }),
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
        side.middle = "STRIPES".into();
    }
    (vfs, geom)
}

fn frame(x: f64, y: f64, z: f64, angle: f64) -> SoftFrame {
    SoftFrame {
        x: fx(x),
        y: fx(y),
        z: fx(z),
        angle: bam(angle),
        ..SoftFrame::default()
    }
}

fn render(view: &mut SoftView, vfs: &Vfs, f: &SoftFrame) -> SoftTarget {
    view.prepare(vfs, f);
    view.render(f)
}

/// Colour changes along row `y`.
fn transitions(t: &SoftTarget, y: usize) -> usize {
    let w = t.width as usize;
    t.pixels[y * w..(y + 1) * w]
        .windows(2)
        .filter(|p| p[0] != p[1])
        .count()
}

fn sector_a(view: &SoftView) -> usize {
    // The room holding the player start.
    view.level.point_in_sector(fx(64.0), fx(256.0)).unwrap()
}

#[test]
fn udmf_wall_scale_repeats_the_texture() {
    let (vfs, mut map) = setup();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let f = frame(64.0, 256.0, 64.0, 0.0);
    let base = transitions(&render(&mut view, &vfs, &f), 100);
    for side in &mut map.sides {
        side.scale_x = 2.0;
    }
    let mut scaled = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let more = transitions(&render(&mut scaled, &vfs, &f), 100);
    assert!(
        more >= base * 2 - 1,
        "scale 2: {more} vs {base} stripes edges"
    );
}

#[test]
fn udmf_flat_scale_and_rotation() {
    let (vfs, map) = setup();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let a = sector_a(&view);
    let f = frame(64.0, 256.0, 41.0, 90.0);
    let row = 180;
    let base = transitions(&render(&mut view, &vfs, &f), row);
    view.level.sectors[a].floor_xform = FlatXform {
        xscale: 2 << 16,
        ..FlatXform::default()
    };
    let scaled = transitions(&render(&mut view, &vfs, &f), row);
    assert!(scaled > base, "scale 2: {scaled} vs {base}");
    // Rotating the x stripes by 90° turns them into y stripes: looking
    // along +y the near rows no longer change colour across the row.
    view.level.sectors[a].floor_xform = FlatXform {
        angle: bam(90.0),
        ..FlatXform::default()
    };
    let rotated = render(&mut view, &vfs, &f);
    assert!(transitions(&rotated, row) < base, "rotation");
}

#[test]
fn a_sloped_floor_rises_on_screen_and_leaves_no_gaps() {
    let (vfs, map) = setup();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let a = sector_a(&view);
    let f = frame(64.0, 256.0, 41.0, 0.0);
    let flat = render(&mut view, &vfs, &f);
    let first_floor = |t: &SoftTarget| {
        (0..200)
            .find(|&y| matches!(t.pixels[y * 320 + 160], 100 | 110))
            .unwrap()
    };
    // A zero slope draws the flat floor (through the tilted-span path).
    view.level.sectors[a].floor_slope = Some(SoftSlope::default());
    let zero = render(&mut view, &vfs, &f);
    let same = zero
        .pixels
        .iter()
        .zip(&flat.pixels)
        .filter(|(p, q)| p == q)
        .count();
    assert!(same > 64000 * 95 / 100, "zero slope: {same} of 64000 match");
    // Rising 1 unit per 8 towards +x: the floor meets the far wall higher
    // up, so it starts higher on screen.
    view.level.sectors[a].floor_slope = Some(SoftSlope {
        z0: 0.0,
        dzdx: 0.125,
        dzdy: 0.0,
    });
    let mut t = SoftTarget::new(320, 200);
    t.pixels.fill(255);
    let fr = SoftFrame {
        z: fx(41.0),
        ..f.clone()
    };
    view.renderer.render(&view.level, &view.data, &fr, &mut t);
    let col: Vec<u8> = (0..200).map(|y| t.pixels[y * 320 + 160]).collect();
    eprintln!("{col:?}");
    let zc: Vec<u8> = (0..200).map(|y| zero.pixels[y * 320 + 160]).collect();
    eprintln!("zero {zc:?}");
    assert!(first_floor(&t) < first_floor(&flat), "slope rises");
    assert!(!t.pixels.contains(&255), "every pixel drawn");
}

#[test]
fn render_styles_blend_in_true_colour() {
    let (vfs, map) = setup();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    view.renderer.truecolor = true;
    for s in &mut view.level.sectors {
        s.light = 255;
    }
    let a = sector_a(&view) as u32;
    let thing = |style, alpha, fill| SoftThing {
        x: fx(192.0),
        y: fx(256.0),
        z: fx(20.0),
        sprite: sprite_name("TEST"),
        sector: a,
        style,
        alpha,
        fill,
        ..SoftThing::default()
    };
    let f = frame(64.0, 256.0, 24.0, 0.0);
    let bg = render(&mut view, &vfs, &f);
    let probe = 100 * 320 + 158;
    let at = |t: &SoftTarget| {
        let c = t.rgb[probe];
        [(c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff]
    };
    let back = at(&bg);
    let mut with = |style, alpha, fill| {
        let mut fr = f.clone();
        fr.things.push(thing(style, alpha, fill));
        at(&render(&mut view, &vfs, &fr))
    };
    // Grey test palette: texel 200 is (200, 200, 200).
    let normal = with(SoftStyle::Normal, 255, 0);
    assert!(normal[0] > 150, "{normal:?}");
    let add = with(SoftStyle::Add, 255, 0);
    assert_eq!(add[0], (back[0] + normal[0]).min(255), "add");
    let sub = with(SoftStyle::Subtract, 255, 0);
    assert_eq!(sub[0], back[0].saturating_sub(normal[0]), "subtract");
    let half = with(SoftStyle::Translucent, 128, 0);
    assert!(half[0] > back[0].min(normal[0]) && half[0] < back[0].max(normal[0]) + 1);
    let red = with(SoftStyle::Stencil, 255, 0xff_0000);
    assert!(
        red[0] > 200 && red[1] < 10 && red[2] < 10,
        "stencil {red:?}"
    );
}

#[test]
fn render_styles_blend_in_8_bit() {
    let (vfs, map) = setup();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let a = sector_a(&view) as u32;
    let f = frame(64.0, 256.0, 24.0, 0.0);
    let probe = 100 * 320 + 158;
    let bg = render(&mut view, &vfs, &f).pixels[probe];
    let mut fr = f.clone();
    fr.things.push(SoftThing {
        x: fx(192.0),
        y: fx(256.0),
        z: fx(20.0),
        sprite: sprite_name("TEST"),
        sector: a,
        style: SoftStyle::Translucent,
        alpha: 128,
        ..SoftThing::default()
    });
    let mixed = render(&mut view, &vfs, &fr).pixels[probe];
    // Grey palette: the nearest index to the 50 % mix lies between both.
    assert!(mixed != 200 && mixed != bg, "{mixed} (bg {bg})");
    assert!(mixed > bg.min(200) && mixed < bg.max(200));
}

#[test]
fn sprites_scale_and_mirror() {
    let (vfs, map) = setup();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let a = sector_a(&view) as u32;
    let f = frame(64.0, 256.0, 41.0, 0.0);
    let count = |t: &SoftTarget| t.pixels.iter().filter(|&&p| p == 200 || p == 201).count();
    let first = |t: &SoftTarget| {
        (0..320)
            .find_map(|x| {
                (0..200)
                    .map(|y| t.pixels[y * 320 + x])
                    .find(|p| *p == 200 || *p == 201)
            })
            .unwrap()
    };
    let thing = SoftThing {
        x: fx(192.0),
        y: fx(256.0),
        z: 0,
        sprite: sprite_name("TEST"),
        sector: a,
        ..SoftThing::default()
    };
    let mut fr = f.clone();
    fr.things.push(thing);
    let one = render(&mut view, &vfs, &fr);
    assert_eq!(first(&one), 200, "left half first");
    let mut fr = f.clone();
    fr.things.push(SoftThing {
        scale_x: 2 << 16,
        scale_y: 2 << 16,
        ..thing
    });
    let two = render(&mut view, &vfs, &fr);
    let (n1, n2) = (count(&one), count(&two));
    assert!(n2 >= n1 * 3 && n2 <= n1 * 5, "scaled ×2: {n2} vs {n1}");
    let mut fr = f.clone();
    fr.things.push(SoftThing {
        xflip: true,
        ..thing
    });
    assert_eq!(first(&render(&mut view, &vfs, &fr)), 201, "mirrored");
}

#[test]
fn polyobjects_are_drawn_where_they_are() {
    let (vfs, mut map) = setup();
    // Make the far box a polyobject: its walls (turned to face out, as a
    // polyobject's do) carry BOXWALL and move into room a.
    let box_sector = map
        .sectors
        .iter()
        .position(|s| {
            s.lines.iter().any(|l| {
                let v = map.lines[l.raw() as usize].v1.raw() as usize;
                map.vertices[v].pos.x() > 1000.0
            })
        })
        .unwrap();
    let lines = map.sectors[box_sector].lines.clone();
    let mut verts = Vec::new();
    for l in &lines {
        let line = &mut map.lines[l.raw() as usize];
        std::mem::swap(&mut line.v1, &mut line.v2);
        let side = line.sides[0].unwrap().raw() as usize;
        map.sides[side].middle = "BOXWALL".into();
        verts.push(line.v1);
        verts.push(line.v2);
    }
    map.setup.polyobjects.push(room_map::setup::PolyobjStatic {
        id: 1,
        vertices: verts.clone(),
        lines: lines.clone(),
        mirror: 0,
        crush: false,
    });
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let f = frame(64.0, 256.0, 41.0, 0.0);
    let boxwall = |t: &SoftTarget| t.pixels.iter().filter(|&&p| p == 77).count();
    assert_eq!(boxwall(&render(&mut view, &vfs, &f)), 0, "far away: unseen");
    // Move it 800 units to (224..256, 224..256), in front of the viewer.
    let mut moved: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for v in &verts {
        if moved.insert(v.raw()) {
            let p = &mut map.vertices[v.raw() as usize].pos;
            *p = room_core::DVec2::new(p.x() - 800.0, p.y() - 800.0);
        }
    }
    view.level.place_polyobjects(&map);
    let seen = boxwall(&render(&mut view, &vfs, &f));
    assert!(seen > 500, "polyobject drawn: {seen} pixels");
}
