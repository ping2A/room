//! F1 acceptance: every Freedoom monster sprite prefix has an anchor
//! (shipped `HEADANCHORS` or estimator fallback).

#![allow(clippy::unwrap_used)]

use room_faceswap::{
    ClassHint, estimate_anchor, lump_pixel_hash, parse_headanchors, resolve_anchor,
};
use room_faceswap::{CompositeOptions, PalettedSprite};
use room_formats::patch::parse_patch;
use room_vfs::{Namespace, Vfs};
use std::collections::BTreeSet;
use std::path::PathBuf;

const PREFIXES: &[&str] = &[
    "POSS", "SPOS", "CPOS", "TROO", "SARG", "HEAD", "SKUL", "PAIN", "BOSS", "BOS2",
    "SKEL", "FATT", "BSPI", "CYBR", "SPID", "VILE", "SSWV", "KEEN", "PLAY",
];

fn freedoom2() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    p.is_file().then_some(p)
}

#[test]
fn freedoom_monster_sprites_have_anchors() {
    let Some(path) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let shipped = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/room/HEADANCHORS"),
    )
    .ok();
    #[allow(clippy::unwrap_used)]
    let file = shipped
        .as_deref()
        .map(|t| parse_headanchors(t).unwrap());
    let vfs = Vfs::builder().mount_path(&path).unwrap().build().unwrap();
    let mut seen_prefix = BTreeSet::new();
    let mut checked = 0u32;
    for id in vfs.iter_lumps() {
        let info = vfs.lump(id).unwrap();
        if info.namespace != Namespace::Sprites && info.namespace != Namespace::Global {
            // Sprites may be Global in IWAD without markers in some mounts.
        }
        let name = info.short_name.to_string().to_ascii_uppercase();
        if name.len() < 6 {
            continue;
        }
        let prefix = &name[..4];
        if !PREFIXES.contains(&prefix) {
            continue;
        }
        seen_prefix.insert(prefix.to_string());
        let Ok(bytes) = vfs.read(id) else { continue };
        let Ok(patch) = parse_patch(&bytes) else { continue };
        let (indices, alpha) = patch.to_indices_masked();
        let hash = lump_pixel_hash(&indices, &alpha);
        let spr = PalettedSprite {
            width: u32::from(patch.width),
            height: u32::from(patch.height),
            left_offset: i32::from(patch.left_offset),
            top_offset: i32::from(patch.top_offset),
            indices,
            alpha,
        };
        let a = resolve_anchor(
            file.as_ref(),
            &name,
            &spr,
            hash,
            ClassHint::default(),
        );
        // Estimator / shipped must produce finite positive radius for visible.
        if !matches!(a.visible, room_faceswap::Visibility::Hidden) {
            assert!(a.radius.0 > 0.5 && a.radius.1 > 0.5, "{name} bad radius");
        }
        checked += 1;
        let _ = CompositeOptions::default();
        let _ = estimate_anchor(spr.width, spr.height, &spr.alpha, &name, ClassHint::default());
    }
    assert!(checked > 100, "expected many sprites, got {checked}");
    for p in ["TROO", "POSS", "PLAY"] {
        assert!(seen_prefix.contains(p), "missing prefix {p}");
    }
}
