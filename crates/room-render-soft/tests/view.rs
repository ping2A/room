//! SR5 checks (`docs/40`): the modern view — widescreen, field of view,
//! free look by shearing, `r_visibility` — and the true-colour drawers with
//! sector light colours and fades. The vanilla view stays identical.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{SoftFrame, SoftRenderer, SoftTarget, SoftView};
use room_vfs::{Vfs, VfsBuilder};
use std::path::PathBuf;

fn fx(v: f64) -> i32 {
    (v * 65536.0) as i32
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

/// A palette where index `i` is (i, i, i) except 200 = pure red-ish grey
/// steps are kept simple; and a COLORMAP whose row `r` paints `r`.
fn light_colormap() -> Vec<u8> {
    (0..34 * 256).map(|i| (i / 256) as u8).collect()
}

fn identity_colormap() -> Vec<u8> {
    (0..34 * 256).map(|i| (i % 256) as u8).collect()
}

/// Room 0..512 square, floor 0 (FLOOR, colour 100), ceiling 128 (CEIL, 110).
fn room(colormap: Vec<u8>) -> (Vfs, room_map::MapGeometry) {
    let map = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .sector_textures("a", "FLOOR", "CEIL")
        .player_start(256.0, 256.0, 0)
        .build_udmf_wad("MAP01")
        .unwrap();
    let res = wad(&[
        ("COLORMAP", colormap),
        ("F_START", Vec::new()),
        ("FLOOR", vec![100; 4096]),
        ("CEIL", vec![110; 4096]),
        ("F_END", Vec::new()),
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
    (vfs, geom)
}

fn frame(angle: f64) -> SoftFrame {
    SoftFrame {
        x: fx(256.0),
        y: fx(256.0),
        z: fx(41.0),
        angle: bam(angle),
        ..SoftFrame::default()
    }
}

fn render(view: &mut SoftView, f: &SoftFrame) -> SoftTarget {
    let mut t = SoftTarget::new(view.renderer.width(), view.renderer.height());
    view.renderer.render(&view.level, &view.data, f, &mut t);
    t
}

/// First row (from the top) of `color` in column `x`.
fn first_row(t: &SoftTarget, x: usize, color: u8) -> Option<usize> {
    let w = t.width as usize;
    (0..t.height as usize).find(|&y| t.pixels[y * w + x] == color)
}

fn freedoom() -> Option<(Vfs, room_map::MapGeometry)> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom1.wad");
    if !p.is_file() {
        return None;
    }
    let vfs = VfsBuilder::new().mount_path(&p).unwrap().build().unwrap();
    let mut map = room_map::load_map(&vfs, "E1M1").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    Some((vfs, map))
}

fn e1m1_frame(
    vfs: &Vfs,
    map: &room_map::MapGeometry,
    view: &mut SoftView,
    angle: f64,
) -> SoftFrame {
    let start = map.things.iter().find(|t| t.type_id == 1).unwrap();
    SoftFrame {
        x: fx(f64::from(start.x)),
        y: fx(f64::from(start.y)),
        z: fx(41.0),
        angle: bam(angle),
        sky: view.sky(vfs, "SKY1"),
        ..SoftFrame::default()
    }
}

#[test]
fn modern_view_at_vanilla_size_is_vanilla() {
    let Some((vfs, map)) = freedoom() else {
        eprintln!("skip: freedoom1.wad missing");
        return;
    };
    let mut a = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let mut b = SoftView::new(&vfs, &map, 320, 200).unwrap();
    b.renderer = SoftRenderer::new_view(320, 200, 90.0);
    for angle in [0.0, 90.0, 200.0] {
        let f = e1m1_frame(&vfs, &map, &mut a, angle);
        assert_eq!(
            render(&mut a, &f).pixels,
            render(&mut b, &f).pixels,
            "{angle}"
        );
    }
}

#[test]
fn widescreen_adds_columns_at_the_sides() {
    let Some((vfs, map)) = freedoom() else {
        eprintln!("skip: freedoom1.wad missing");
        return;
    };
    let mut narrow = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let mut wide = SoftView::new(&vfs, &map, 320, 200).unwrap();
    // 16:9 with vanilla-shaped pixels: 200 × 1.2 × 16 / 9 ≈ 427.
    wide.renderer = SoftRenderer::new_view(426, 200, 90.0);
    let f = e1m1_frame(&vfs, &map, &mut narrow, 30.0);
    let n = render(&mut narrow, &f);
    let w = render(&mut wide, &f);
    // The middle 320 columns are the 4:3 view (offset by 53).
    let mut same = 0;
    for y in 0..200 {
        for x in 0..320 {
            if n.pixels[y * 320 + x] == w.pixels[y * 426 + x + 53] {
                same += 1;
            }
        }
    }
    assert!(same > 64000 * 97 / 100, "{same} of 64000 match");
}

#[test]
fn field_of_view_and_free_look_move_the_picture() {
    let (vfs, map) = room(identity_colormap());
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let f = frame(0.0);
    view.renderer = SoftRenderer::new_view(320, 200, 90.0);
    let base = render(&mut view, &f);
    let floor_90 = first_row(&base, 160, 100).unwrap();
    // Wider field of view: things look smaller, the floor starts lower
    // on screen... closer to the horizon row 100.
    view.renderer = SoftRenderer::new_view(320, 200, 110.0);
    let wide = render(&mut view, &f);
    let floor_110 = first_row(&wide, 160, 100).unwrap();
    assert!(floor_110 < floor_90, "110° {floor_110} vs 90° {floor_90}");
    // Free look: shearing 40 rows moves the floor's first row down by 40.
    view.renderer = SoftRenderer::new_view(320, 200, 90.0);
    let mut up = f.clone();
    up.yshear = 40;
    let looked = render(&mut view, &up);
    let floor_up = first_row(&looked, 160, 100).unwrap();
    assert!(
        (floor_up as i32 - floor_90 as i32 - 40).abs() <= 1,
        "{floor_up} vs {floor_90}"
    );
    // And back: the shear is per frame.
    assert_eq!(render(&mut view, &f).pixels, base.pixels);
}

#[test]
fn visibility_lights_distant_planes() {
    let (vfs, map) = room(light_colormap());
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    for s in &mut view.level.sectors {
        s.light = 160;
    }
    let f = frame(0.0);
    // Row 130 is the farthest floor under the east wall: with COLORMAP
    // rows painting their number, a brighter (lower) row means more
    // visibility.
    let dim = render(&mut view, &f).pixels[130 * 320 + 160];
    view.renderer.visibility = 4.0;
    let lit = render(&mut view, &f).pixels[130 * 320 + 160];
    assert!(lit < dim, "visibility 4: row {lit} vs {dim}");
}

#[test]
fn true_colour_shades_tints_and_fades() {
    let (vfs, map) = room(identity_colormap());
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    view.renderer.truecolor = true;
    for s in &mut view.level.sectors {
        s.light = 255;
    }
    let f = frame(0.0);
    let t = render(&mut view, &f);
    assert_eq!(t.rgb.len(), 320 * 200);
    // Grey palette (no PLAYPAL): the nearest floor row is about (100,100,100).
    let near = t.rgb[199 * 320 + 160];
    let r = (near >> 16) & 0xff;
    assert!((90..=100).contains(&r), "{near:06x}");
    assert_eq!(r, near & 0xff);
    // Red light: only the red channel stays.
    for s in &mut view.level.sectors {
        s.color = 0xff_0000;
    }
    let red = render(&mut view, &f).rgb[199 * 320 + 160];
    assert!(red >> 16 > 80 && red & 0xffff == 0, "{red:06x}");
    // Dark sector, white fade: far pixels go towards white.
    for s in &mut view.level.sectors {
        s.color = 0;
        s.light = 0;
        s.fade = 0xff_ffff;
    }
    let fog = render(&mut view, &f).rgb[130 * 320 + 160];
    assert!(fog & 0xff > 200, "{fog:06x}");
    // RGBA output uses the true-colour pixels.
    let rgba = render(&mut view, &f).to_rgba(&view.data.palette);
    assert_eq!(rgba[(130 * 320 + 160) * 4], (fog >> 16) as u8);
}

/// An RGBA PNG, `w` × `h`, every pixel `rgb`.
fn png_image(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let px: Vec<u8> = (0..w * h)
            .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
            .collect();
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }
    out
}

/// The room with every wall a red PNG texture (`TX_START`).
fn red_room() -> (Vfs, room_map::MapGeometry) {
    let map = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .sector_textures("a", "FLOOR", "CEIL")
        .player_start(256.0, 256.0, 0)
        .build_udmf_wad("MAP01")
        .unwrap();
    let res = wad(&[
        ("COLORMAP", identity_colormap()),
        ("F_START", Vec::new()),
        ("FLOOR", vec![100; 4096]),
        ("CEIL", vec![110; 4096]),
        ("F_END", Vec::new()),
        ("TX_START", Vec::new()),
        ("REDWALL", png_image(64, 128, [255, 0, 0])),
        ("TX_END", Vec::new()),
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
        side.middle = "REDWALL".into();
    }
    (vfs, geom)
}

#[test]
fn true_colour_pictures_keep_their_colours() {
    let (vfs, map) = red_room();
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    let wall = view.data.texture(&vfs, "REDWALL");
    assert!(!view.data.textures[usize::from(wall)].rgb.is_empty());
    for s in &mut view.level.sectors {
        s.light = 255;
    }
    let f = frame(0.0);
    // 8-bit: the grey test palette has no red; true colour keeps it.
    let paletted = render(&mut view, &f);
    let i = 100 * 320 + 160;
    view.renderer.truecolor = true;
    let tc = render(&mut view, &f);
    let [r, g, b] = [
        (tc.rgb[i] >> 16) & 0xff,
        (tc.rgb[i] >> 8) & 0xff,
        tc.rgb[i] & 0xff,
    ];
    assert!(r > 100 && g == 0 && b == 0, "{:06x}", tc.rgb[i]);
    assert!(paletted.pixels[i] != 0);
    // Desaturation 255 turns it grey.
    for s in &mut view.level.sectors {
        s.desaturation = 255;
    }
    let grey = render(&mut view, &f).rgb[i];
    let [r, g, b] = [(grey >> 16) & 0xff, (grey >> 8) & 0xff, grey & 0xff];
    assert!(r == g && g == b && r > 0, "{grey:06x}");
}

#[test]
fn true_colour_light_is_smooth() {
    let (vfs, map) = room(identity_colormap());
    let mut view = SoftView::new(&vfs, &map, 320, 200).unwrap();
    for s in &mut view.level.sectors {
        s.light = 144;
    }
    let f = frame(0.0);
    let column = |t: &SoftTarget, tc: bool| {
        let mut seen = std::collections::HashSet::new();
        for y in 130..200 {
            let i = y * 320 + 160;
            seen.insert(if tc { t.rgb[i] } else { u32::from(t.pixels[i]) });
        }
        seen.len()
    };
    let steps_8 = column(&render(&mut view, &f), false);
    view.renderer.truecolor = true;
    let steps_tc = column(&render(&mut view, &f), true);
    assert!(
        steps_tc > steps_8 * 2,
        "8-bit {steps_8} steps, true colour {steps_tc}"
    );
}
