//! SR8 checks (`docs/40`): KVX voxel models (placed, turned, hiding their
//! own back), particles, dynamic lights in true colour.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{
    SoftFrame, SoftLight, SoftParticle, SoftTarget, SoftThing, SoftView, SoftVoxelPose, sprite_name,
};
use room_vfs::{Vfs, VfsBuilder};

fn bam(deg: f64) -> u32 {
    ((deg.rem_euclid(360.0) / 360.0) * 4_294_967_296.0) as u64 as u32
}

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

/// A KVX file: `size` voxels, every column one full-height slab coloured
/// `color(x, y)`, pivot at the bottom centre.
fn kvx(size: [usize; 3], color: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    let [xs, ys, zs] = size;
    let offsetsize = (xs + 1) * 4 + xs * (ys + 1) * 2;
    let slab = 3 + zs;
    let mut xoff = Vec::new();
    let mut xyoff = Vec::new();
    let mut slabs = Vec::new();
    for x in 0..xs {
        xoff.push((offsetsize + slabs.len()) as i32);
        let start = slabs.len();
        for y in 0..ys {
            xyoff.push((slabs.len() - start) as u16);
            slabs.push(0u8);
            slabs.push(zs as u8);
            slabs.push(0x3f);
            slabs.extend(std::iter::repeat_n(color(x, y), zs));
        }
        xyoff.push((slabs.len() - start) as u16);
        let _ = slab;
    }
    xoff.push((offsetsize + slabs.len()) as i32);
    let mut mip = Vec::new();
    for v in [xs as i32, ys as i32, zs as i32] {
        mip.extend_from_slice(&v.to_le_bytes());
    }
    // Pivot (8.8): centre in x / y, bottom in z (KVX z runs down).
    for v in [xs as i32 * 128, ys as i32 * 128, zs as i32 * 256] {
        mip.extend_from_slice(&v.to_le_bytes());
    }
    for v in xoff {
        mip.extend_from_slice(&v.to_le_bytes());
    }
    for v in xyoff {
        mip.extend_from_slice(&v.to_le_bytes());
    }
    mip.extend(slabs);
    let mut out = (mip.len() as i32).to_le_bytes().to_vec();
    out.extend(mip);
    out.extend(std::iter::repeat_n(0u8, 768));
    out
}

/// Room 0..512 square, walls `WALL` (10), floor 100, ceiling 120, light
/// 128; voxels `BOX` (4 × 4 × 4, x = 0 face colour 201, the rest 202) and
/// `BAR` (8 × 2 × 4, colour 200); sprite `TEST` (8 × 16, colour 200).
fn setup() -> (Vfs, room_map::MapGeometry) {
    let map = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .sector_textures("a", "FLOOR", "CEIL")
        .sector_light("a", 128)
        .player_start(64.0, 256.0, 0)
        .build_udmf_wad("MAP01")
        .unwrap();
    let (pn, t1) = textures(&[("WALL", 64, 128)]);
    let mut pal = Vec::new();
    for i in 0..256u32 {
        let v = i as u8;
        pal.extend_from_slice(&[v, v, v]);
    }
    let res = wad(&[
        ("PLAYPAL", pal),
        ("COLORMAP", identity_colormap()),
        ("PNAMES", pn),
        ("TEXTURE1", t1),
        ("P_START", Vec::new()),
        ("WALL", picture(64, 128, 0, 0, |_, _| 100)),
        ("P_END", Vec::new()),
        ("F_START", Vec::new()),
        ("FLOOR", vec![100; 4096]),
        ("CEIL", vec![120; 4096]),
        ("F_END", Vec::new()),
        ("S_START", Vec::new()),
        ("TESTA0", picture(8, 16, 4, 16, |_, _| 200)),
        ("S_END", Vec::new()),
        ("VX_START", Vec::new()),
        ("BOX", kvx([4, 4, 4], |x, _| if x == 0 { 201 } else { 202 })),
        ("BAR", kvx([8, 2, 4], |_, _| 200)),
        ("VX_END", Vec::new()),
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
    (vfs, geom)
}

fn frame(z: f64, things: Vec<SoftThing>) -> SoftFrame {
    SoftFrame {
        x: fx(64.0),
        y: fx(256.0),
        z: fx(z),
        angle: 0,
        things,
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
        std::fs::write(format!("{dir}/v{n}.pgm"), out).unwrap();
    }
    t
}

fn count(t: &SoftTarget, c: u8) -> usize {
    t.pixels.iter().filter(|&&p| p == c).count()
}

/// Width and height of the box holding colours `cs`.
fn extent(t: &SoftTarget, cs: &[u8]) -> (usize, usize) {
    let w = t.width as usize;
    let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
    for (i, p) in t.pixels.iter().enumerate() {
        if cs.contains(p) {
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

fn voxel_thing(
    view: &mut SoftView,
    vfs: &Vfs,
    file: &str,
    x: f64,
    angle: f64,
    scale: f64,
) -> SoftThing {
    let id = view.data.voxel(vfs, file, true).expect("voxel loads");
    SoftThing {
        x: fx(x),
        y: fx(256.0),
        z: 0,
        angle: bam(angle),
        sprite: sprite_name("TEST"),
        sector: view.level.point_in_sector(fx(x), fx(256.0)).unwrap() as u32,
        voxel: Some(SoftVoxelPose {
            id,
            scale: [fx(scale); 3],
            ..SoftVoxelPose::default()
        }),
        ..SoftThing::default()
    }
}

#[test]
fn a_voxel_model_stands_where_its_thing_is() {
    let (vfs, map) = setup();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let t = voxel_thing(&mut view, &vfs, "voxels/box.kvx", 200.0, 0.0, 8.0);
    let shot = render(&mut view, &vfs, &frame(16.0, vec![t]));
    // 32 units wide and tall, about 140 units away (within the splats'
    // overlap).
    let (w, h) = extent(&shot, &[201, 202]);
    let expect = 32.0 * 160.0 / 140.0;
    for (what, v) in [("width", w), ("height", h)] {
        assert!(
            (v as f64 - expect).abs() < expect * 0.25,
            "{what} {v} vs {expect:.1}"
        );
    }
    // Seen straight on, only the near face shows.
    let (near, back) = (count(&shot, 201), count(&shot, 202));
    assert!(near > 500, "front face: {near}");
    assert!(back * 20 < near, "back hidden: {back} of {near}");
}

#[test]
fn a_voxel_model_turns_with_its_thing() {
    let (vfs, map) = setup();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    // A bar 8 voxels long along its x: end on, then side on.
    let along = voxel_thing(&mut view, &vfs, "bar", 200.0, 0.0, 4.0);
    let across = SoftThing {
        angle: bam(90.0),
        ..along
    };
    let w0 = extent(&render(&mut view, &vfs, &frame(41.0, vec![along])), &[200]).0;
    let w1 = extent(&render(&mut view, &vfs, &frame(41.0, vec![across])), &[200]).0;
    assert!(w1 > w0 * 2, "side on {w1} vs end on {w0}");
}

#[test]
fn particles_are_coloured_squares() {
    let (vfs, map) = setup();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let p = SoftParticle {
        x: 160.0,
        y: 256.0,
        z: 41.0,
        size: 8.0,
        color: 0xc8c8c8,
        alpha: 255,
        light: 255,
        ..SoftParticle::default()
    };
    let f = SoftFrame {
        particles: vec![p],
        ..frame(41.0, Vec::new())
    };
    let t = render(&mut view, &vfs, &f);
    let grey = view.data.nearest_rgb([200, 200, 200]);
    let (w, h) = extent(&t, &[grey]);
    // 8 units at 96 → about 13 pixels.
    assert!((10..=16).contains(&w) && w == h, "{w} × {h}");
    // Additive in true colour: brighter than the wall behind.
    view.renderer.truecolor = true;
    let plain = render(&mut view, &vfs, &frame(41.0, Vec::new()));
    let add = render(
        &mut view,
        &vfs,
        &SoftFrame {
            particles: vec![SoftParticle {
                additive: true,
                alpha: 128,
                ..p
            }],
            ..frame(41.0, Vec::new())
        },
    );
    let i = 100 * 320 + 160;
    assert!(add.rgb[i] & 0xff > plain.rgb[i] & 0xff);
}

#[test]
fn dynamic_lights_brighten_walls_floors_and_things_in_true_colour() {
    let (vfs, map) = setup();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    view.renderer.truecolor = true;
    let thing = SoftThing {
        x: fx(300.0),
        y: fx(256.0),
        sprite: sprite_name("TEST"),
        sector: 0,
        ..SoftThing::default()
    };
    let f = frame(41.0, vec![thing]);
    let dark = render(&mut view, &vfs, &f);
    // One light before the far wall, one just over the floor in view.
    let light = SoftLight {
        x: 480.0,
        y: 256.0,
        z: 40.0,
        radius: 128.0,
        color: [1.0, 0.5, 0.0],
        subtractive: false,
    };
    let low = SoftLight {
        x: 190.0,
        y: 256.0,
        z: 12.0,
        radius: 96.0,
        ..light
    };
    let lit = render(
        &mut view,
        &vfs,
        &SoftFrame {
            lights: vec![light, low],
            ..f.clone()
        },
    );
    let r = |t: &SoftTarget, x: usize, y: usize| (t.rgb[y * 320 + x] >> 16) & 255;
    let b = |t: &SoftTarget, x: usize, y: usize| t.rgb[y * 320 + x] & 255;
    // The far wall (x = 512), the floor before it, the thing.
    for (x, y, what) in [(160, 95, "wall"), (160, 150, "floor")] {
        assert!(r(&lit, x, y) > r(&dark, x, y) + 10, "{what} redder");
        assert_eq!(
            b(&lit, x, y),
            b(&dark, x, y),
            "{what}: no blue in the light"
        );
    }
    // The thing, with a light at its feet.
    let at_thing = SoftLight {
        x: 300.0,
        y: 240.0,
        z: 8.0,
        radius: 64.0,
        ..light
    };
    let lit_thing = render(
        &mut view,
        &vfs,
        &SoftFrame {
            lights: vec![at_thing],
            ..f.clone()
        },
    );
    view.renderer.truecolor = false;
    let where_8bit = render(&mut view, &vfs, &f);
    view.renderer.truecolor = true;
    let i = where_8bit.pixels.iter().position(|&p| p == 200).unwrap();
    assert!(
        (lit_thing.rgb[i] >> 16) & 255 > ((dark.rgb[i] >> 16) & 255) + 10,
        "thing redder"
    );
    // 8-bit: no dynamic lights.
    view.renderer.truecolor = false;
    let p0 = render(&mut view, &vfs, &f);
    let p1 = render(
        &mut view,
        &vfs,
        &SoftFrame {
            lights: vec![light],
            ..f
        },
    );
    assert_eq!(p0.pixels, p1.pixels);
}
