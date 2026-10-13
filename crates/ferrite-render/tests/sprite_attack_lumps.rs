//! Attack pose lumps must resolve from doom1.wad (same path as MaterialCatalog).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ferrite_formats::decode_image;
use ferrite_render::sprites::sprite_lump_candidates_rot;
use ferrite_vfs::{Namespace, VfsBuilder};
use std::path::PathBuf;

#[test]
fn doom1_zombie_attack_lumps_decode_all_rotations() {
    let wad = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    if !wad.is_file() {
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut missing = Vec::new();
    for frame in [4u8, 5u8] {
        for rot in 1..=8u8 {
            let cands = sprite_lump_candidates_rot("POSS", frame, rot);
            let found = cands.iter().find_map(|(n, _)| {
                let id = vfs
                    .find(n, Namespace::Sprites)
                    .or_else(|| vfs.find_any_ns(n))?;
                let data = vfs.read(id).ok()?;
                decode_image(&data).ok()?;
                Some(n.clone())
            });
            match found {
                Some(n) => eprintln!("ok frame={frame} rot={rot} → {n}"),
                None => missing.push(format!("frame={frame} rot={rot} tried={cands:?}")),
            }
        }
    }
    for m in &missing {
        eprintln!("MISSING {m}");
    }
    assert!(
        missing.is_empty(),
        "{} attack rotations failed",
        missing.len()
    );
}

#[test]
fn doom1_zombie_attack_offsets_reasonable() {
    let wad = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    if !wad.is_file() {
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    for name in ["POSSA1", "POSSE1", "POSSF1", "POSSE2E8", "POSSF2F8"] {
        let id = vfs
            .find(name, Namespace::Sprites)
            .or_else(|| vfs.find_any_ns(name))
            .unwrap();
        let data = vfs.read(id).unwrap();
        let img = decode_image(&data).unwrap();
        eprintln!(
            "{name}: {}x{} left={} top={}",
            match &img.pixels {
                ferrite_formats::ImagePixels::Paletted(p) => p.width,
                _ => 0,
            },
            match &img.pixels {
                ferrite_formats::ImagePixels::Paletted(p) => p.height,
                _ => 0,
            },
            img.left_offset,
            img.top_offset
        );
    }
}
