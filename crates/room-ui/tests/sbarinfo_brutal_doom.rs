//! Brutal Doom's SBARINFO against the real lumps (gated: set
//! `ROOM_BRUTALDOOM_PK3` and `ROOM_DOOM2_WAD`). Renders the normal and
//! fullscreen bars at 1280×720 through the shells' CPU rasterizer and checks
//! the magazine panel and numbers land where UZDoom puts them. With
//! `ROOM_SBAR_PNG=dir` the frames are written there as PNGs.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_defs::DefinitionsBuilder;
use room_ui::{
    CanvasSink, DrawCmd, PatchCache, PatchDims, SbarInfoBar, SbarInfoFrame, SbarSnapshot,
    blit_draw_list_full,
};
use room_vfs::VfsBuilder;

const W: usize = 1280;
const H: usize = 720;

struct Fb(Vec<[u8; 4]>);

impl CanvasSink for Fb {
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) {
        let (x0, y0) = (x.round().max(0.0) as usize, y.round().max(0.0) as usize);
        let (x1, y1) = (
            ((x + w).round().max(0.0) as usize).min(W),
            ((y + h).round().max(0.0) as usize).min(H),
        );
        for yy in y0..y1 {
            for xx in x0..x1 {
                let p = &mut self.0[yy * W + xx];
                for k in 0..3 {
                    p[k] = (f32::from(p[k]) * (1.0 - c[3]) + c[k] * 255.0 * c[3]) as u8;
                }
            }
        }
    }
    fn text(&mut self, _: f32, _: f32, _: &str, _: [f32; 4]) {}
    fn blit_rgba(&mut self, x: f32, y: f32, w: u32, h: u32, px: &[u8]) {
        let (x0, y0) = (x.round() as i64, y.round() as i64);
        for row in 0..h as i64 {
            for col in 0..w as i64 {
                let (fx, fy) = (x0 + col, y0 + row);
                if fx < 0 || fy < 0 || fx >= W as i64 || fy >= H as i64 {
                    continue;
                }
                let i = ((row * i64::from(w) + col) * 4) as usize;
                let a = f32::from(px[i + 3]) / 255.0;
                if a <= 0.0 {
                    continue;
                }
                let p = &mut self.0[fy as usize * W + fx as usize];
                for k in 0..3 {
                    p[k] = (f32::from(p[k]) * (1.0 - a) + f32::from(px[i + k]) * a) as u8;
                }
            }
        }
    }
}

fn write_png(path: &std::path::Path, fb: &Fb) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W as u32, H as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().unwrap();
    let bytes: Vec<u8> = fb.0.iter().flatten().copied().collect();
    w.write_image_data(&bytes).unwrap();
}

#[test]
fn brutal_doom_status_bar_shows_the_magazine() {
    let (Ok(pk3), Ok(wad)) = (
        std::env::var("ROOM_BRUTALDOOM_PK3"),
        std::env::var("ROOM_DOOM2_WAD"),
    ) else {
        eprintln!("skip: set ROOM_BRUTALDOOM_PK3 and ROOM_DOOM2_WAD");
        return;
    };
    let vfs = VfsBuilder::new()
        .mount_path(&wad)
        .unwrap()
        .mount_path(&pk3)
        .unwrap()
        .build()
        .unwrap();
    let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
    let info = defs.sbarinfo.clone().expect("Brutal Doom ships SBARINFO");
    let bar = SbarInfoBar::new(info, &defs.fonts);
    let snap = SbarSnapshot {
        health: 87,
        face: "STFST01".into(),
        inventory: vec![
            ("Rifle".into(), 1, 1),
            ("Clip2".into(), 120, 300),
            ("RifleAmmo".into(), 23, 31),
            ("BasicArmor".into(), 45, 200),
            ("GenderMale".into(), 1, 1),
            ("sbartype1".into(), 1, 1),
            ("IsPlayer".into(), 1, 1),
        ],
        icons: vec![
            ("Clip2".into(), "CLIPA0".into()),
            ("RifleAmmo".into(), "CLIPA0".into()),
            ("BasicArmor".into(), "ARM1A0".into()),
        ],
        ready_weapon: Some("Rifle".into()),
        ammo1: Some("Clip2".into()),
        ammo2: Some("RifleAmmo".into()),
        slots: [
            false, true, true, false, true, false, false, false, false, false,
        ],
        keys: 1 | 32,
        ..SbarSnapshot::default()
    };
    let mut cache = PatchCache::from_vfs(&vfs);
    for name in bar.wanted_patches(&snap) {
        let _ = cache.ensure(&vfs, &name);
    }
    let out_dir = std::env::var("ROOM_SBAR_PNG").ok();
    for fullscreen in [false, true] {
        let dims = |name: &str| {
            cache.get(name).map(|p| PatchDims {
                w: f32::from(p.width),
                h: f32::from(p.height),
                left: f32::from(p.left_offset),
                top: f32::from(p.top_offset),
            })
        };
        let frame = SbarInfoFrame {
            snap: &snap,
            fullscreen,
            screen: (W as f32, H as f32),
            deathmatch: false,
            teamplay: false,
            multiplayer: false,
            player_class: "",
            hud_aspectscale: false,
            dims: &dims,
        };
        let out = bar.draw(&frame).expect("bar for the mode");
        let names: Vec<&str> = out
            .list
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Patch { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        eprintln!(
            "{} bar: {} patches, origin {:?} size {:?}",
            if fullscreen { "fullscreen" } else { "normal" },
            names.len(),
            out.origin,
            out.size
        );
        let missing: Vec<&&str> = names.iter().filter(|n| cache.get(n).is_none()).collect();
        // Brutal Doom names a few lumps it does not ship (`STGNU22`…);
        // UZDoom draws nothing for them either.
        eprintln!("  not in the archives: {missing:?}");
        if !fullscreen {
            assert!(names.contains(&"STBARBD"), "{names:?}");
            assert!(names.contains(&"STMAG"), "magazine panel: {names:?}");
            // Magazine 23 and reserve 120 in the big font, health 87.
            let digits: String = names
                .iter()
                .filter_map(|n| n.strip_prefix("STTNUM"))
                .collect();
            assert!(digits.starts_with("23120"), "{digits}");
        }
        let mut fb = Fb(vec![[40, 40, 48, 255]; W * H]);
        blit_draw_list_full(
            &out.list,
            out.size.0,
            out.size.1,
            None,
            Some(&cache),
            &mut Offset {
                fb: &mut fb,
                ox: out.origin.0,
                oy: out.origin.1,
            },
        );
        if let Some(dir) = &out_dir {
            let path = std::path::Path::new(dir).join(if fullscreen {
                "bd_fullscreen.png"
            } else {
                "bd_normal.png"
            });
            write_png(&path, &fb);
        }
    }
}

struct Offset<'a> {
    fb: &'a mut Fb,
    ox: f32,
    oy: f32,
}

impl CanvasSink for Offset<'_> {
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) {
        self.fb.fill_rect(x + self.ox, y + self.oy, w, h, c);
    }
    fn text(&mut self, _: f32, _: f32, _: &str, _: [f32; 4]) {}
    fn blit_rgba(&mut self, x: f32, y: f32, w: u32, h: u32, px: &[u8]) {
        self.fb.blit_rgba(x + self.ox, y + self.oy, w, h, px);
    }
}
