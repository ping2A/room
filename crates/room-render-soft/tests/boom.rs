//! SR4 checks (`docs/40`): Boom / MBF renderer features on synthetic maps —
//! animated textures and flats, flat panning, `Transfer_Heights` (242) fake
//! planes and colormaps, light transfers, sky transfers, translucency
//! (lines and things), and Boom's wall-column wrap.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{
    MF_TRANSLUCENT, SoftData, SoftFrame, SoftTarget, SoftThing, SoftView, boom_tranmap, sprite_name,
};
use room_vfs::{Vfs, VfsBuilder};

const FRAC: f64 = 65536.0;

fn fx(v: f64) -> i32 {
    (v * FRAC) as i32
}

fn bam(deg: f64) -> u32 {
    ((deg / 360.0) * 4_294_967_296.0) as u64 as u32
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

/// A Doom picture `w` × `h` whose column `c` is filled by `color(c, row)`.
fn picture(w: u16, h: u8, color: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&u16::from(h).to_le_bytes());
    out.extend_from_slice(&0i16.to_le_bytes());
    out.extend_from_slice(&0i16.to_le_bytes());
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

fn sprite(color: u8) -> Vec<u8> {
    let mut p = picture(4, 8, |_, _| color);
    // Offsets: centred, standing on its bottom.
    p[4..6].copy_from_slice(&2i16.to_le_bytes());
    p[6..8].copy_from_slice(&8i16.to_le_bytes());
    p
}

/// `PNAMES` + `TEXTURE1` with one single-patch texture per `(name, w, h)`,
/// the patch named like the texture.
fn textures(list: &[(&str, i16, i16)]) -> (Vec<u8>, Vec<u8>) {
    let mut pn = (list.len() as i32).to_le_bytes().to_vec();
    for (name, _, _) in list {
        let mut n = [0u8; 8];
        n[..name.len()].copy_from_slice(name.as_bytes());
        pn.extend_from_slice(&n);
    }
    let mut t1 = (list.len() as i32).to_le_bytes().to_vec();
    let head = 4 + 4 * list.len();
    let entry = 22 + 10;
    for i in 0..list.len() {
        t1.extend_from_slice(&((head + i * entry) as i32).to_le_bytes());
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

/// Identity colormaps.
fn identity_colormap() -> Vec<u8> {
    (0..34 * 256).map(|i| (i % 256) as u8).collect()
}

/// Colormap whose row `r` paints everything `r`: a pixel tells its light.
fn light_colormap() -> Vec<u8> {
    (0..34 * 256).map(|i| (i / 256) as u8).collect()
}

fn flat(color: u8) -> Vec<u8> {
    vec![color; 4096]
}

struct Setup {
    vfs: Vfs,
    map: room_map::MapGeometry,
}

/// Room `a` (0..512 × 0..512, floor 0, ceiling 128, flat FLOOR colour 100)
/// and a small room `c` east of it (512..576), plus `extra` lumps.
fn setup(extra: Vec<(&str, Vec<u8>)>) -> Setup {
    let builder = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .room(
            "c",
            room_testkit::rect(512.0, 0.0, 576.0, 512.0),
            0.0,
            128.0,
        )
        .sector_textures("a", "FLOOR", "CEIL")
        .sector_textures("c", "WATER", "CEIL")
        .player_start(256.0, 256.0, 0);
    let mut lumps: Vec<(&str, Vec<u8>)> = vec![
        ("F_START", Vec::new()),
        ("FLOOR", flat(100)),
        ("CEIL", flat(110)),
        ("WATER", flat(150)),
        ("F_END", Vec::new()),
    ];
    let has_colormap = extra.iter().any(|(n, _)| *n == "COLORMAP");
    lumps.extend(extra);
    if !has_colormap {
        lumps.insert(0, ("COLORMAP", identity_colormap()));
    }
    let vfs = VfsBuilder::new()
        .mount_memory("t.wad", builder.build_udmf_wad("MAP01").unwrap())
        .unwrap()
        .mount_memory("r.wad", wad(&lumps))
        .unwrap()
        .build()
        .unwrap();
    let mut map = room_map::load_map(&vfs, "MAP01").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    Setup { vfs, map }
}

/// Sector indices of rooms `a` and `c` (by centre x).
fn rooms(map: &room_map::MapGeometry) -> (usize, usize) {
    let centre = |i: usize| {
        let lines = &map.sectors[i].lines;
        let xs: Vec<f64> = lines
            .iter()
            .map(|l| {
                map.vertices[map.lines[l.raw() as usize].v1.raw() as usize]
                    .pos
                    .x()
            })
            .collect();
        xs.iter().sum::<f64>() / xs.len() as f64
    };
    let a = (0..map.sectors.len()).find(|&i| centre(i) < 400.0).unwrap();
    let c = (0..map.sectors.len()).find(|&i| centre(i) > 500.0).unwrap();
    (a, c)
}

/// The two-sided line between `a` and `c`, with `c` as its front sector
/// (the control side of a 242 line).
fn shared_line(map: &mut room_map::MapGeometry, c: usize) -> usize {
    let li = map
        .lines
        .iter()
        .position(|l| l.back_sector.is_some())
        .unwrap();
    let line = &mut map.lines[li];
    if line.front_sector.map(|s| s.raw() as usize) != Some(c) {
        line.sides.swap(0, 1);
        std::mem::swap(&mut line.front_sector, &mut line.back_sector);
    }
    li
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

fn draw(view: &mut SoftView, vfs: &Vfs, f: &SoftFrame) -> SoftTarget {
    view.prepare(vfs, f);
    view.render(f)
}

fn first_row_with(t: &SoftTarget, x: usize, color: u8) -> Option<usize> {
    let w = t.width as usize;
    (0..t.height as usize).find(|&y| t.pixels[y * w + x] == color)
}

#[test]
fn vanilla_and_boom_animations_follow_leveltime() {
    // No ANIMATED lump: vanilla's list; NUKAGE1..3 cycle every 8 tics.
    let s = setup(vec![
        ("F_START", Vec::new()),
        ("NUKAGE1", flat(1)),
        ("NUKAGE2", flat(2)),
        ("NUKAGE3", flat(3)),
        ("F_END", Vec::new()),
    ]);
    let mut data = SoftData::load(&s.vfs);
    let n1 = data.flat(&s.vfs, "NUKAGE1");
    let n2 = data.flat(&s.vfs, "NUKAGE2");
    let n3 = data.flat(&s.vfs, "NUKAGE3");
    // `basepic + ((leveltime / 8 + i) % 3)` with `i` the flat's number
    // (NUKAGE1 is flat 3 after FLOOR, CEIL, WATER).
    let base = 3u32;
    for lt in [0u32, 7, 8, 16, 24, 100] {
        let want = [n1, n2, n3][((lt / 8 + base) % 3) as usize];
        assert_eq!(data.anim_flat(n1, lt), want, "leveltime {lt}");
    }
    assert_ne!(data.anim_flat(n1, 0), data.anim_flat(n1, 8));

    // Boom ANIMATED: a texture range at speed 4.
    let (pn, t1) = textures(&[("ANIMA", 8, 128), ("ANIMB", 8, 128), ("ANIMC", 8, 128)]);
    let mut animated = Vec::new();
    animated.push(1u8);
    let name = |s: &str| {
        let mut n = [0u8; 9];
        n[..s.len()].copy_from_slice(s.as_bytes());
        n
    };
    animated.extend_from_slice(&name("ANIMC"));
    animated.extend_from_slice(&name("ANIMA"));
    animated.extend_from_slice(&4i32.to_le_bytes());
    animated.push(0xff);
    let s = setup(vec![
        ("PNAMES", pn),
        ("TEXTURE1", t1),
        ("ANIMATED", animated),
        ("P_START", Vec::new()),
        ("ANIMA", picture(8, 128, |_, _| 10)),
        ("ANIMB", picture(8, 128, |_, _| 20)),
        ("ANIMC", picture(8, 128, |_, _| 30)),
        ("P_END", Vec::new()),
    ]);
    let mut data = SoftData::load(&s.vfs);
    let a = data.texture(&s.vfs, "ANIMA");
    let b = data.texture(&s.vfs, "ANIMB");
    let c = data.texture(&s.vfs, "ANIMC");
    assert_eq!(data.anims.len(), 1);
    // Texture numbers: "-" is 0, ANIMA 1 — the vanilla `+ i` quirk.
    assert_eq!(data.anim_texture(a, 0), b);
    assert_eq!(data.anim_texture(a, 4), c);
    assert_eq!(data.anim_texture(a, 8), a);
    assert_eq!(data.anim_texture(c, 0), a);

    // A wall with ANIMA changes colour with the frame's leveltime.
    let mut map = s.map.clone();
    for side in &mut map.sides {
        side.middle = "ANIMA".into();
    }
    let mut view = SoftView::new(&s.vfs, &map, 320, 200).unwrap();
    let mut f = frame(256.0, 256.0, 41.0, 180.0);
    let colour_at =
        |view: &mut SoftView, f: &SoftFrame| draw(view, &s.vfs, f).pixels[100 * 320 + 160];
    let c0 = colour_at(&mut view, &f);
    f.leveltime = 4;
    let c4 = colour_at(&mut view, &f);
    f.leveltime = 8;
    let c8 = colour_at(&mut view, &f);
    assert_eq!((c0, c4, c8), (20, 30, 10));
}

#[test]
fn flat_panning_moves_the_floor_and_splits_planes() {
    let s = setup(vec![("COLORMAP", identity_colormap())]);
    let mut map = s.map.clone();
    // A floor with a pattern so panning shows.
    let (a, _) = rooms(&map);
    map.sectors[a].floor_pic = "PATT".into();
    let mut pat = vec![0u8; 4096];
    for (i, p) in pat.iter_mut().enumerate() {
        *p = (i % 64) as u8;
    }
    let vfs = VfsBuilder::new()
        .mount_memory(
            "t.wad",
            room_testkit::MapBuilder::new()
                .room("a", room_testkit::rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
                .room(
                    "c",
                    room_testkit::rect(512.0, 0.0, 576.0, 512.0),
                    0.0,
                    128.0,
                )
                .player_start(256.0, 256.0, 0)
                .build_udmf_wad("MAP01")
                .unwrap(),
        )
        .unwrap()
        .mount_memory(
            "r.wad",
            wad(&[
                ("COLORMAP", identity_colormap()),
                ("F_START", Vec::new()),
                ("PATT", pat),
                ("F_END", Vec::new()),
            ]),
        )
        .unwrap()
        .build()
        .unwrap();
    let _ = s;
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let f0 = frame(256.0, 256.0, 41.0, 90.0);
    let base = draw(&mut view, &vfs, &f0);
    let mut sectors = view.level.sectors.clone();
    sectors[a].floor_xoffs = 64 << 16;
    let f64_ = SoftFrame {
        sectors: Some(sectors.clone()),
        ..f0.clone()
    };
    assert_eq!(draw(&mut view, &vfs, &f64_).pixels, base.pixels, "64 wraps");
    sectors[a].floor_xoffs = 16 << 16;
    let f16 = SoftFrame {
        sectors: Some(sectors),
        ..f0
    };
    let moved = draw(&mut view, &vfs, &f16);
    assert_ne!(moved.pixels, base.pixels, "16 units of panning show");
    // Rows of the floor shift by 16 texels: the pattern value moves by 16.
    let w = 320;
    let y = 190;
    let diffs: Vec<i32> = (0..w)
        .map(|x| {
            (i32::from(moved.pixels[y * w + x]) - i32::from(base.pixels[y * w + x])).rem_euclid(64)
        })
        .collect();
    assert!(
        diffs.iter().filter(|&&d| d == 16).count() > w * 9 / 10,
        "{diffs:?}"
    );
}

#[test]
fn transfer_heights_draws_the_control_planes_and_colormaps() {
    // WATERMAP paints everything 7; the 242 line names it below the floor.
    let mut watermap = vec![7u8; 34 * 256];
    watermap[32 * 256..].fill(7);
    let mut s = setup(vec![
        ("C_START", Vec::new()),
        ("WATERMAP", watermap),
        ("C_END", Vec::new()),
        ("S_START", Vec::new()),
        ("TESTA0", sprite(200)),
        ("S_END", Vec::new()),
    ]);
    let (a, c) = rooms(&s.map);
    let li = shared_line(&mut s.map, c);
    // Control sector c: floor 32 (the water surface), ceiling 96.
    s.map.sectors[c].floor_height = 32.0;
    s.map.sectors[c].ceiling_height = 96.0;
    s.map.sectors[a].tags = [5].into_iter().collect();
    s.map.lines[li].special = 209; // Transfer_Heights
    s.map.lines[li].args = [5, 0, 0, 0, 0];
    let front_side = s.map.lines[li].sides[0].unwrap().raw() as usize;
    s.map.sides[front_side].lower = "WATERMAP".into();

    let mut view = SoftView::new(&s.vfs, &s.map, 320, 200).unwrap();
    assert_eq!(view.level.heightsec[a], Some(c as u32));
    assert_eq!(view.level.colormaps[c][0], 1, "bottommap = WATERMAP");
    assert_eq!(
        view.level.sides[front_side].bottom, 0,
        "a colormap is no texture"
    );

    // Above the water: room a's floor (its own flat) is drawn at 32, the
    // control floor, so it starts nearer the horizon than at 0.
    let above = draw(&mut view, &s.vfs, &frame(256.0, 256.0, 64.0, 180.0));
    let water_row = first_row_with(&above, 160, 100).expect("floor at the fake height");
    let mut plain = s.map.clone();
    plain.lines[li].special = 0;
    let mut pview = SoftView::new(&s.vfs, &plain, 320, 200).unwrap();
    let real = draw(&mut pview, &s.vfs, &frame(256.0, 256.0, 64.0, 180.0));
    let floor_row = first_row_with(&real, 160, 100).expect("real floor");
    assert!(
        water_row < floor_row,
        "fake floor {water_row} vs real {floor_row}"
    );

    // Below the water (eye under 32): the bottom colormap paints the view.
    let under = draw(&mut view, &s.vfs, &frame(256.0, 256.0, 20.0, 180.0));
    // (Untextured walls stay unfilled, 0.)
    let painted = under.pixels.iter().filter(|&&p| p == 7).count();
    assert!(painted > under.pixels.len() * 8 / 10, "{painted}");
    assert!(under.pixels.iter().all(|&p| p == 7 || p == 0));
    // Between the planes: the mid colormap (COLORMAP) leaves colours alone.
    assert!(above.pixels.iter().all(|&p| p != 7));

    // A thing standing on the real floor, under the water surface, is
    // clipped at it when seen from above (Boom deep-water clipping).
    let mut f = frame(256.0, 256.0, 64.0, 180.0);
    f.things.push(SoftThing {
        x: fx(160.0),
        y: fx(256.0),
        z: 0,
        sprite: sprite_name("TEST"),
        sector: a as u32,
        ..SoftThing::default()
    });
    assert!(
        !draw(&mut view, &s.vfs, &f).pixels.contains(&200),
        "under water"
    );
    assert!(
        draw(&mut pview, &s.vfs, &f).pixels.contains(&200),
        "no water: seen"
    );
}

#[test]
fn light_transfers_light_floors_and_ceilings_apart() {
    let mut s = setup(vec![("COLORMAP", light_colormap())]);
    let (a, c) = rooms(&s.map);
    let li = shared_line(&mut s.map, c);
    s.map.sectors[a].light = 255;
    s.map.sectors[c].light = 64;
    s.map.sectors[a].tags = [7].into_iter().collect();
    s.map.lines[li].special = 210; // Transfer_FloorLight
    s.map.lines[li].args = [7, 0, 0, 0, 0];
    let mut view = SoftView::new(&s.vfs, &s.map, 320, 200).unwrap();
    assert_eq!(view.level.floorlightsec[a], Some(c as u32));
    let t = draw(&mut view, &s.vfs, &frame(256.0, 256.0, 41.0, 180.0));
    // With COLORMAP rows painting their own number, darker = larger.
    let floor = t.pixels[190 * 320 + 160];
    let ceiling = t.pixels[10 * 320 + 160];
    assert!(
        floor > ceiling + 4,
        "floor row {floor} vs ceiling row {ceiling}"
    );
}

#[test]
fn sky_transfer_uses_the_line_upper_texture() {
    let (pn, t1) = textures(&[("SKY1", 256, 128), ("SKYB", 256, 128)]);
    let mut s = setup(vec![
        ("PNAMES", pn),
        ("TEXTURE1", t1),
        ("P_START", Vec::new()),
        ("SKY1", picture(256, 128, |_, _| 230)),
        (
            "SKYB",
            picture(256, 128, |c, _| if c < 128 { 220 } else { 221 }),
        ),
        ("P_END", Vec::new()),
    ]);
    let (a, c) = rooms(&s.map);
    let li = shared_line(&mut s.map, c);
    s.map.sectors[a].ceiling_pic = "F_SKY1".into();
    s.map.sectors[a].tags = [9].into_iter().collect();
    s.map.lines[li].special = 190; // Static_Init (tag, Init_TransferSky)
    s.map.lines[li].args = [9, 255, 0, 0, 0];
    let front_side = s.map.lines[li].sides[0].unwrap().raw() as usize;
    s.map.sides[front_side].upper = "SKYB".into();
    let mut view = SoftView::new(&s.vfs, &s.map, 320, 200).unwrap();
    assert!(view.level.sky[a].is_some());
    let sky1 = view.sky(&s.vfs, "SKY1");
    let mut f = frame(256.0, 256.0, 41.0, 180.0);
    f.sky = sky1;
    let t = draw(&mut view, &s.vfs, &f);
    let top = &t.pixels[0..320];
    assert!(top.iter().all(|&p| p == 220 || p == 221), "transferred sky");
    assert!(!top.contains(&230));
    // Without the transfer, the map's sky.
    let mut plain = s.map.clone();
    plain.lines[li].special = 0;
    let mut pview = SoftView::new(&s.vfs, &plain, 320, 200).unwrap();
    f.sky = pview.sky(&s.vfs, "SKY1");
    let t = draw(&mut pview, &s.vfs, &f);
    assert!(t.pixels[0..320].iter().all(|&p| p == 230));
}

#[test]
fn translucent_lines_and_things_blend_through_the_table() {
    let (pn, t1) = textures(&[("GLASS", 64, 128), ("WALL", 64, 128)]);
    let mut s = setup(vec![
        ("PNAMES", pn),
        ("TEXTURE1", t1),
        ("P_START", Vec::new()),
        ("GLASS", picture(64, 128, |_, _| 200)),
        ("WALL", picture(64, 128, |_, _| 40)),
        ("P_END", Vec::new()),
        ("S_START", Vec::new()),
        ("TESTA0", sprite(200)),
        ("S_END", Vec::new()),
    ]);
    let (_, c) = rooms(&s.map);
    let li = shared_line(&mut s.map, c);
    for side in &mut s.map.sides {
        side.middle = "WALL".into();
    }
    for sd in s.map.lines[li].sides.into_iter().flatten() {
        s.map.sides[sd.raw() as usize].middle = "GLASS".into();
    }
    s.map.lines[li].special = 208; // TranslucentLine (self)
    s.map.lines[li].args = [0, 168, 0, 0, 0];
    let mut view = SoftView::new(&s.vfs, &s.map, 320, 200).unwrap();
    assert_eq!(view.level.line_tranmap[li], Some(0));
    // Grey palette (no PLAYPAL): 66 % of 200 over 34 % of the wall behind.
    let want = boom_tranmap(&view.data.palette, 66)[(40 << 8) + 200];
    assert!(want != 200 && want != 40);
    let t = draw(&mut view, &s.vfs, &frame(256.0, 256.0, 41.0, 0.0));
    assert_eq!(t.pixels[100 * 320 + 160], want, "glass over the far wall");

    // MF_TRANSLUCENT things too.
    let mut f = frame(256.0, 256.0, 41.0, 180.0);
    f.things.push(SoftThing {
        x: fx(128.0),
        y: fx(256.0),
        z: 0,
        sprite: sprite_name("TEST"),
        flags: MF_TRANSLUCENT,
        sector: rooms(&s.map).0 as u32,
        ..SoftThing::default()
    });
    let t = draw(&mut view, &s.vfs, &f);
    assert!(!t.pixels.contains(&200), "no opaque sprite pixels");
}

#[test]
fn boom_wraps_short_textures_at_their_height() {
    // 72 rows: rows 0..8 colour 50, the rest 60. A 128-high wall shows the
    // texture again from row 72 in Boom; vanilla's `& 127` reads past it.
    let (pn, t1) = textures(&[("SHORT", 64, 72)]);
    let s = setup(vec![
        ("PNAMES", pn),
        ("TEXTURE1", t1),
        ("P_START", Vec::new()),
        ("SHORT", picture(64, 72, |_, r| if r < 8 { 50 } else { 60 })),
        ("P_END", Vec::new()),
    ]);
    let mut map = s.map.clone();
    for side in &mut map.sides {
        side.middle = "SHORT".into();
    }
    let mut view = SoftView::new(&s.vfs, &map, 320, 200).unwrap();
    // Close to the west wall so it fills the view's height.
    let f = frame(48.0, 256.0, 64.0, 180.0);
    let boom = draw(&mut view, &s.vfs, &f);
    let col: Vec<u8> = (0..200).map(|y| boom.pixels[y * 320 + 160]).collect();
    let bands = col.windows(2).filter(|w| w[0] == 60 && w[1] == 50).count();
    assert!(bands >= 1, "texture repeats below row 72: {col:?}");
    view.renderer.boom = false;
    let vanilla = draw(&mut view, &s.vfs, &f);
    assert_ne!(vanilla.pixels, boom.pixels);
}
