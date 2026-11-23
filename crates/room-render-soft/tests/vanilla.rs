//! Pixel parity with Doom's own renderer (`docs/40` SR1 / SR2): frames
//! written by `scripts/vanilla-soft-capture.sh` (doomgeneric) for the poses
//! in `testdata/soft/poses.txt` — the world alone, and the whole view with
//! the things and weapon the capture listed. Skipped when the captures or
//! IWADs are not present.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render_soft::{SoftFrame, SoftPSprite, SoftThing, SoftView, sprite_name};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn vanilla_captures_match() {
    run(false);
}

#[test]
fn vanilla_things_match() {
    run(true);
}

/// Things, weapon and `fuzzpos` from a `-things.txt` capture listing.
fn read_things(text: &str, frame: &mut SoftFrame) -> usize {
    let mut fuzzpos = 0;
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |i: usize| -> i64 { f[i].parse().unwrap() };
        match f.first().copied() {
            Some("fuzzpos") => fuzzpos = num(1) as usize,
            Some("psprite_fuzz") => frame.psprite_fuzz = num(1) != 0,
            Some("thing") => frame.things.push(SoftThing {
                x: num(1) as i32,
                y: num(2) as i32,
                z: num(3) as i32,
                angle: num(4) as u32,
                sprite: sprite_name(f[5]),
                frame: num(6) as u32,
                flags: num(7) as u32,
                sector: num(8) as u32,
                ..SoftThing::default()
            }),
            Some("psprite") => frame.psprites.push(SoftPSprite {
                sprite: sprite_name(f[1]),
                frame: num(2) as u32,
                sx: num(3) as i32,
                sy: num(4) as i32,
            }),
            _ => {}
        }
    }
    fuzzpos
}

fn run(things: bool) {
    let poses = std::fs::read_to_string(root().join("testdata/soft/poses.txt")).unwrap();
    let mut checked = 0;
    let mut failures = Vec::new();
    for line in poses.lines().filter(|l| !l.trim_start().starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() != 7 {
            continue;
        }
        let (iwad, map, name) = (f[0], f[1], f[2]);
        let suffix = if things { "-things" } else { "" };
        let capture = root().join(format!("testdata/soft/{iwad}/{map}-{name}{suffix}.idx"));
        let listing = root().join(format!("testdata/soft/{iwad}/{map}-{name}-things.txt"));
        if things && !listing.is_file() {
            continue;
        }
        let wad = root().join(format!("testdata/freedoom/{iwad}"));
        if !capture.is_file() || !wad.is_file() {
            continue;
        }
        let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
        let mut geom = room_map::load_map(&vfs, map).unwrap();
        room_nodebuild::ensure_nodes(&mut geom);
        let x: f64 = f[3].parse().unwrap();
        let y: f64 = f[4].parse().unwrap();
        let z = if f[5] == "auto" {
            room_map::portals::sector_at(&geom, x, y)
                .and_then(|s| geom.sectors.get(s.raw() as usize))
                .map_or(0.0, |s| s.floor_height)
                + 41.0
        } else {
            f[5].parse().unwrap()
        };
        let angle: f64 = f[6].parse().unwrap();
        let mut view = SoftView::new(&vfs, &geom, 320, 200).unwrap();
        view.renderer.vanilla_limits = true;
        view.renderer.boom = false;
        let sky_name = if map.starts_with('E') {
            format!("SKY{}", &map[1..2])
        } else {
            let n: u32 = map[3..].parse().unwrap();
            if n >= 21 {
                "SKY3"
            } else if n >= 12 {
                "SKY2"
            } else {
                "SKY1"
            }
            .to_owned()
        };
        let sky = view.sky(&vfs, &sky_name);
        let mut frame = SoftFrame {
            x: (x * 65536.0) as i32,
            y: (y * 65536.0) as i32,
            z: (z * 65536.0) as i32,
            angle: (angle / 360.0 * 4_294_967_296.0) as u64 as u32,
            extralight: 0,
            fixed_colormap: None,
            sectors: None,
            sky,
            ..SoftFrame::default()
        };
        if things {
            let fuzzpos = read_things(&std::fs::read_to_string(&listing).unwrap(), &mut frame);
            view.renderer.set_fuzzpos(fuzzpos);
            view.prepare(&vfs, &frame);
        }
        let ours = view.render(&frame);
        let theirs = std::fs::read(&capture).unwrap();
        checked += 1;
        let diff: Vec<usize> = ours
            .pixels
            .iter()
            .zip(&theirs)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect();
        if !diff.is_empty() {
            let i = diff[0];
            failures.push(format!(
                "{iwad} {map} {name}{suffix}: {} pixels differ, first at ({}, {}): ours {} vanilla {}",
                diff.len(),
                i % 320,
                i / 320,
                ours.pixels[i],
                theirs[i]
            ));
        }
    }
    if checked == 0 {
        eprintln!("skip: no captures (run scripts/vanilla-soft-capture.sh)");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
