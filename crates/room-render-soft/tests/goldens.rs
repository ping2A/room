//! SR10 software goldens (`docs/40` SR10): fixed views drawn with no GPU and
//! compared with committed PNGs in `testdata/goldens/soft/`. They run with
//! plain `cargo test` on every CI system (the GPU goldens in `room-render`
//! all stay as they are).
//!
//! - Vanilla 8-bit views (Freedoom E1M1 and MAP01 when
//!   `scripts/fetch-freedoom.sh` has run, and a synthetic room that always
//!   runs) must match exactly: the vanilla path is integer maths.
//! - Modern views (true colour, field of view, slopes, 3D floors, portals)
//!   allow 0.5 % of pixels to differ: their tables come from `f64`
//!   trigonometry, whose last bit can differ between platforms.
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-render-soft --test
//! goldens` (and look at the PNGs before committing them).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::shot::{ShotOptions, encode_png, shoot, shoot_map};
use room_render_soft::{SoftFrame, SoftLinePortal, SoftRenderer, SoftSlope, SoftView};
use room_vfs::{Vfs, VfsBuilder};
use std::path::PathBuf;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/goldens/soft")
}

fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let dec = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba, "goldens are RGBA");
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

/// Compare with (or, with `ROOM_UPDATE_GOLDEN`, write) `name`; at most
/// `tolerance` of the pixels may differ.
fn check(name: &str, width: u32, height: u32, rgba: &[u8], tolerance: f64) {
    let path = golden_dir().join(format!("{name}.png"));
    if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, encode_png(width, height, rgba).unwrap()).unwrap();
        eprintln!("wrote {}", path.display());
        return;
    }
    let bytes = std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "{} missing (ROOM_UPDATE_GOLDEN=1 writes it)",
            path.display()
        )
    });
    let (w, h, want) = decode_png(&bytes);
    assert_eq!((w, h), (width, height), "{name}: size");
    let differ = want
        .chunks_exact(4)
        .zip(rgba.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    let share = differ as f64 / f64::from(width * height);
    eprintln!("{name}: {differ} pixels differ ({:.3} %)", share * 100.0);
    if share > tolerance {
        let out = std::env::temp_dir().join(format!("{name}-got.png"));
        let _ = std::fs::write(&out, encode_png(width, height, rgba).unwrap());
        panic!(
            "{name}: {differ} pixels differ ({:.3} %, allowed {:.3} %); got {}",
            share * 100.0,
            tolerance * 100.0,
            out.display()
        );
    }
}

fn freedoom(file: &str) -> Option<Vfs> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/freedoom")
        .join(file);
    if !p.is_file() {
        eprintln!("skip: testdata/freedoom/{file} missing (scripts/fetch-freedoom.sh)");
        return None;
    }
    Some(VfsBuilder::new().mount_path(&p).unwrap().build().unwrap())
}

#[test]
fn freedoom_vanilla_views_match_exactly() {
    for (file, map, angles) in [
        ("freedoom1.wad", "E1M1", &[None, Some(180.0)][..]),
        (
            "freedoom2.wad",
            "MAP01",
            &[None, Some(90.0), Some(225.0)][..],
        ),
    ] {
        let Some(vfs) = freedoom(file) else {
            continue;
        };
        for (i, angle) in angles.iter().enumerate() {
            let opts = ShotOptions {
                angle: *angle,
                ..ShotOptions::default()
            };
            let shot = shoot_map(&vfs, map, &opts).unwrap();
            let (w, h, rgba) = shot.rgba(None);
            check(
                &format!("{}_{i}_320x200", map.to_ascii_lowercase()),
                w,
                h,
                &rgba,
                0.0,
            );
        }
    }
}

#[test]
fn freedoom_modern_views_match() {
    let Some(vfs) = freedoom("freedoom2.wad") else {
        return;
    };
    let opts = ShotOptions {
        width: 640,
        height: 360,
        fov: Some(100.0),
        truecolor: true,
        ..ShotOptions::default()
    };
    let shot = shoot_map(&vfs, "MAP01", &opts).unwrap();
    let (w, h, rgba) = shot.rgba(None);
    check("map01_truecolor_fov100_640x360", w, h, &rgba, 0.005);
}

// --- Synthetic scenes (no WAD needed) ---------------------------------

/// A WAD from `(name, data)` lumps.
fn wad(lumps: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut dir = Vec::new();
    for (name, bytes) in lumps {
        let ofs = 12 + data.len() as u32;
        data.extend_from_slice(bytes);
        let mut n = [0u8; 8];
        n[..name.len()].copy_from_slice(name.as_bytes());
        dir.push((ofs, bytes.len() as u32, n));
    }
    let mut out = b"PWAD".to_vec();
    out.extend_from_slice(&(lumps.len() as u32).to_le_bytes());
    out.extend_from_slice(&(12 + data.len() as u32).to_le_bytes());
    out.extend(data);
    for (ofs, len, n) in dir {
        out.extend_from_slice(&ofs.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&n);
    }
    out
}

/// A Doom picture `w` × `h` coloured by `f(x, y)`.
/// A patch; `h` must stay below 255 rows (no tall-patch offsets).
fn picture(w: u16, h: u16, f: impl Fn(u16, u16) -> u8) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&(w as i16 / 2).to_le_bytes());
    out.extend_from_slice(&(h as i16).to_le_bytes());
    let header = 8 + 4 * u32::from(w);
    let mut cols = Vec::new();
    let mut offs = Vec::new();
    for x in 0..w {
        offs.push(header + cols.len() as u32);
        // Posts of opaque runs (index 0 is transparent), at most 128 rows.
        let mut y = 0;
        while y < h {
            if f(x, y) == 0 {
                y += 1;
                continue;
            }
            let mut n = 0;
            while y + n < h && n < 128 && f(x, y + n) != 0 {
                n += 1;
            }
            cols.push(y as u8);
            cols.push(n as u8);
            cols.push(0);
            cols.extend((y..y + n).map(|yy| f(x, yy)));
            cols.push(0);
            y += n;
        }
        cols.push(0xff);
    }
    for o in offs {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend(cols);
    out
}

/// A palette of eight hue ramps (32 shades each), and a COLORMAP darkening
/// each ramp towards its own black, so light shows in 8-bit.
fn palette_and_colormap() -> (Vec<u8>, Vec<u8>) {
    let hues: [[u32; 3]; 8] = [
        [255, 255, 255],
        [255, 64, 64],
        [64, 255, 64],
        [64, 64, 255],
        [255, 255, 64],
        [64, 255, 255],
        [255, 64, 255],
        [255, 160, 64],
    ];
    let mut pal = Vec::new();
    for h in hues {
        for s in 0..32u32 {
            for c in h {
                pal.push((c * (s + 1) / 32) as u8);
            }
        }
    }
    let mut cm = Vec::new();
    for row in 0..34u32 {
        for i in 0..256u32 {
            let (ramp, shade) = (i / 32, i % 32);
            let s = if row >= 32 {
                shade
            } else {
                shade * (32 - row) / 32
            };
            cm.push((ramp * 32 + s) as u8);
        }
    }
    (pal, cm)
}

/// Textures: `BRICK` (64 × 128, ramp 7 checks), `STONE` (ramp 0), `SIDE`
/// (ramp 5); flats `FLOOR` (ramp 4 checks), `CEIL` (ramp 0), `SLAB`
/// (ramp 2); sprite `TROOA0` (ramp 1 diamond).
fn resources() -> Vec<u8> {
    let (pal, cm) = palette_and_colormap();
    let tex = |names: &[(&str, u16, u16)]| {
        let mut pn = (names.len() as u32).to_le_bytes().to_vec();
        for (n, _, _) in names {
            let mut b = [0u8; 8];
            b[..n.len()].copy_from_slice(n.as_bytes());
            pn.extend_from_slice(&b);
        }
        let mut t1 = (names.len() as u32).to_le_bytes().to_vec();
        let base = 4 + 4 * names.len() as u32;
        let mut body = Vec::new();
        let mut offs = Vec::new();
        for (i, (n, w, h)) in names.iter().enumerate() {
            offs.push(base + body.len() as u32);
            let mut b = [0u8; 8];
            b[..n.len()].copy_from_slice(n.as_bytes());
            body.extend_from_slice(&b);
            body.extend_from_slice(&0u32.to_le_bytes());
            body.extend_from_slice(&w.to_le_bytes());
            body.extend_from_slice(&h.to_le_bytes());
            body.extend_from_slice(&0u32.to_le_bytes());
            body.extend_from_slice(&1u16.to_le_bytes());
            body.extend_from_slice(&0i16.to_le_bytes());
            body.extend_from_slice(&0i16.to_le_bytes());
            body.extend_from_slice(&(i as u16).to_le_bytes());
            body.extend_from_slice(&1u16.to_le_bytes());
            body.extend_from_slice(&0u16.to_le_bytes());
        }
        for o in offs {
            t1.extend_from_slice(&o.to_le_bytes());
        }
        t1.extend(body);
        (pn, t1)
    };
    let (pn, t1) = tex(&[("BRICK", 64, 128), ("STONE", 64, 128), ("SIDE", 64, 128)]);
    let check = |ramp: u8, x: u16, y: u16| {
        let on = ((x / 16) + (y / 16)) % 2 == 0;
        ramp * 32 + if on { 31 } else { 20 }
    };
    let flat = |f: &dyn Fn(u16, u16) -> u8| {
        (0..64u16)
            .flat_map(|y| (0..64u16).map(move |x| (x, y)))
            .map(|(x, y)| f(x, y))
            .collect::<Vec<u8>>()
    };
    wad(&[
        ("PLAYPAL", pal),
        ("COLORMAP", cm),
        ("PNAMES", pn),
        ("TEXTURE1", t1),
        ("P_START", Vec::new()),
        ("BRICK", picture(64, 128, |x, y| check(7, x, y))),
        ("STONE", picture(64, 128, |x, y| (x + y) as u8 % 12 + 18)),
        ("SIDE", picture(64, 128, |x, y| check(5, x, y))),
        ("P_END", Vec::new()),
        ("F_START", Vec::new()),
        ("FLOOR", flat(&|x, y| check(4, x, y))),
        ("CEIL", flat(&|x, y| ((x ^ y) % 8) as u8 + 22)),
        ("SLAB", flat(&|x, y| 64 + check(0, x, y) % 32)),
        ("F_END", Vec::new()),
        ("S_START", Vec::new()),
        (
            "TROOA0",
            picture(24, 40, |x, y| {
                let d = (i32::from(x) - 12).abs() + (i32::from(y) - 20).abs();
                if d < 12 { 32 + 31 - (d as u8) } else { 0 }
            }),
        ),
        ("S_END", Vec::new()),
    ])
}

/// Room `a` (512 × 512, light 192) opening east into `b` (light 176, floor
/// 24 lower), a far room `c` for a 3D floor's control sector.
fn synthetic() -> (Vfs, room_map::MapGeometry) {
    let map = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 512.0, 512.0), 0.0, 160.0)
        .room(
            "b",
            room_testkit::rect(512.0, 0.0, 1024.0, 512.0),
            -24.0,
            128.0,
        )
        .room(
            "c",
            room_testkit::rect(4000.0, 4000.0, 4064.0, 4064.0),
            40.0,
            56.0,
        )
        .sector_textures("a", "FLOOR", "CEIL")
        .sector_textures("b", "FLOOR", "CEIL")
        .sector_textures("c", "SLAB", "SLAB")
        .sector_light("a", 192)
        .sector_light("b", 176)
        .sector_tag("b", 7)
        .player_start(96.0, 160.0, 0)
        .build_udmf_wad("MAP01")
        .unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("map.wad", map)
        .unwrap()
        .mount_memory("res.wad", resources())
        .unwrap()
        .build()
        .unwrap();
    let mut geom = room_map::load_map(&vfs, "MAP01").unwrap();
    room_nodebuild::ensure_nodes(&mut geom);
    for line in geom.lines.clone() {
        let a = geom.vertices[line.v1.raw() as usize].pos;
        for s in line.sides.iter().flatten() {
            let side = &mut geom.sides[s.raw() as usize];
            if line.back_sector.is_some() {
                side.middle = "-".into();
                side.lower = "STONE".into();
                side.upper = "STONE".into();
            } else {
                side.middle = if a.x() > 3000.0 { "SIDE" } else { "BRICK" }.into();
            }
        }
    }
    // Things: imps across both rooms (Doom type 3001).
    for (x, y) in [(300.0, 260.0), (700.0, 120.0), (860.0, 380.0)] {
        geom.things.push(room_map::RawThing {
            x: x as i16,
            y: y as i16,
            angle: 180,
            type_id: 3001,
            // Easy, medium and hard.
            flags: 7,
            ..room_map::RawThing::default()
        });
    }
    (vfs, geom)
}

#[test]
fn synthetic_vanilla_view_matches_exactly() {
    let (vfs, map) = synthetic();
    let shot = shoot(&vfs, "MAP01", &map, &ShotOptions::default()).unwrap();
    let (w, h, rgba) = shot.rgba(None);
    check("synthetic_vanilla_320x200", w, h, &rgba, 0.0);
    // The display screenshot (tall pixels, bars).
    let (w, h, rgba) = shot.rgba(Some((640, 360)));
    check("synthetic_vanilla_display_640x360", w, h, &rgba, 0.0);
}

#[test]
fn synthetic_modern_features_match() {
    let rgba = modern_scene(7, 0xe000_0000);
    check("synthetic_modern_640x400", 640, 400, &rgba, 0.005);
}

/// `mask`: 1 = 3D floor, 2 = mirror, 4 = slope.
fn modern_scene(mask: u8, angle: u32) -> Vec<u8> {
    let (vfs, mut map) = synthetic();
    // A 3D floor in b (control c) and a mirror on a's south wall.
    let control = map
        .lines
        .iter()
        .position(|l| l.back_sector.is_none() && map.vertices[l.v1.raw() as usize].pos.x() > 3000.0)
        .unwrap();
    if mask & 1 != 0 {
        map.lines[control].special = 160;
        map.lines[control].args = [7, 1, 0, 255, 0];
    }
    let mut view = SoftView::new(&vfs, &map, 640, 400).unwrap();
    view.renderer = SoftRenderer::new_view(640, 400, 100.0);
    view.renderer.truecolor = true;
    let south = map
        .lines
        .iter()
        .position(|l| {
            let (p, q) = (
                map.vertices[l.v1.raw() as usize].pos,
                map.vertices[l.v2.raw() as usize].pos,
            );
            l.back_sector.is_none() && p.y() == 0.0 && q.y() == 0.0 && p.x().max(q.x()) <= 512.0
        })
        .unwrap_or_else(|| {
            let ls: Vec<_> = map
                .lines
                .iter()
                .map(|l| {
                    (
                        map.vertices[l.v1.raw() as usize].pos,
                        map.vertices[l.v2.raw() as usize].pos,
                        l.back_sector.is_some(),
                    )
                })
                .collect();
            panic!("no south wall: {ls:?}")
        });
    let front = map.lines[south].sides[0].unwrap().raw();
    if mask & 2 != 0 {
        view.level.line_portals[south] = Some((SoftLinePortal::Mirror, front));
    }
    // Room b's floor slopes up to the east.
    let b = view.level.point_in_sector(700 << 16, 200 << 16).unwrap();
    if mask & 4 != 0 {
        view.level.sectors[b].floor_slope = Some(SoftSlope {
            z0: -24.0 - 512.0 * 0.05,
            dzdx: 0.05,
            dzdy: 0.0,
        });
    }
    let things = room_render_soft::map_things(&map, &view.level);
    let sky = view.sky(&vfs, "SKY1");
    let frame = SoftFrame {
        x: 96 << 16,
        y: 300 << 16,
        z: 64 << 16,
        // Facing south-east: the mirror and the opening.
        angle,
        things,
        sky,
        psprites: Vec::new(),
        ..SoftFrame::default()
    };
    view.prepare(&vfs, &frame);
    let t = view.render(&frame);
    t.to_rgba(&view.data.palette)
}
