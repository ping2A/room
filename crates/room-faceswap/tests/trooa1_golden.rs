//! F0 acceptance: composite a synthetic face onto Freedoom `TROOA1`.
//!
//! Skips when `testdata/freedoom/freedoom2.wad` is missing.

#![allow(clippy::unwrap_used)]

use room_faceswap::{
    AlphaMask, CompositeOptions, Consent, FaceId, FaceProfile, FaceView, HeadAnchor, ManualPoints,
    PalettedSprite, RgbaImage, align_manual, composite_front, lump_pixel_hash,
};
use room_formats::patch::parse_patch;
use room_formats::playpal::parse_playpal;
use room_vfs::{Namespace, Vfs};
use std::path::PathBuf;

fn freedoom2() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    p.is_file().then_some(p)
}

fn load_trooa1() -> Option<(PalettedSprite, [u8; 768], u64)> {
    let path = freedoom2()?;
    let vfs = Vfs::builder().mount_path(&path).ok()?.build().ok()?;
    let patch_id = vfs
        .find("TROOA1", Namespace::Sprites)
        .or_else(|| vfs.find_any_ns("TROOA1"))?;
    let pal_id = vfs
        .find("PLAYPAL", Namespace::Global)
        .or_else(|| vfs.find_any_ns("PLAYPAL"))?;
    let patch = parse_patch(&vfs.read(patch_id).ok()?).ok()?;
    let playpal = parse_playpal(&vfs.read(pal_id).ok()?).ok()?;
    let pal = playpal.palettes.first()?.rgb;
    let (indices, alpha) = patch.to_indices_masked();
    let hash = lump_pixel_hash(&indices, &alpha);
    let sprite = PalettedSprite {
        width: u32::from(patch.width),
        height: u32::from(patch.height),
        left_offset: i32::from(patch.left_offset),
        top_offset: i32::from(patch.top_offset),
        indices,
        alpha,
    };
    Some((sprite, pal, hash))
}

fn synthetic_profile() -> FaceProfile {
    let mut src = RgbaImage::new(160, 200);
    let cx = 80.0_f32;
    let cy = 100.0_f32;
    for y in 0..200u32 {
        for x in 0..160u32 {
            let nx = (x as f32 - cx) / 55.0;
            let ny = (y as f32 - cy) / 70.0;
            if nx * nx + ny * ny <= 1.0 {
                // Warm skin.
                src.put(x, y, [210, 170, 140, 255]);
            }
        }
    }
    // Eyes + mouth.
    for (ex, ey) in [(55.0_f32, 85.0), (105.0, 85.0)] {
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                let x = (ex as i32 + dx).clamp(0, 159) as u32;
                let y = (ey as i32 + dy).clamp(0, 199) as u32;
                src.put(x, y, [30, 30, 50, 255]);
            }
        }
    }
    for x in 65..95 {
        src.put(x, 130, [160, 80, 80, 255]);
    }
    align_manual(
        &src,
        ManualPoints {
            left_eye: (55.0, 85.0),
            right_eye: (105.0, 85.0),
            chin: (80.0, 160.0),
        },
        "synthetic",
        FaceId(0xFACE_0000_0000_0001),
        Consent::SelfCamera,
        0,
    )
    .unwrap()
}

#[test]
fn trooa1_composite_preserves_geometry() {
    let Some((sprite, pal, _hash)) = load_trooa1() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let profile = synthetic_profile();
    // Doc example ellipse for TROOA1 (Freedoom art; tune in F1 anchors).
    let anchor = HeadAnchor::front((21.0, 12.0), (8.0, 9.0), 0.2);
    let out = composite_front(
        &sprite,
        &pal,
        &anchor,
        &profile,
        &CompositeOptions::default(),
    )
    .unwrap();
    assert_eq!(out.width, sprite.width);
    assert_eq!(out.height, sprite.height);
    assert_eq!(out.left_offset, sprite.left_offset);
    assert_eq!(out.top_offset, sprite.top_offset);
    assert_eq!(out.indices.len(), sprite.indices.len());
    // Alpha ⊆ original.
    for (a, b) in out.alpha.iter().zip(sprite.alpha.iter()) {
        assert!(*a == 0 || *b != 0, "composited alpha outside silhouette");
        if *b == 0 {
            assert_eq!(*a, 0);
        }
    }
    // Some pixels inside the head region must have changed.
    let mut changed = 0u32;
    for i in 0..sprite.indices.len() {
        if sprite.alpha[i] != 0 && out.indices[i] != sprite.indices[i] {
            changed += 1;
        }
    }
    assert!(
        changed > 20,
        "expected face pixels to change, got {changed}"
    );
}

#[test]
fn trooa1_composite_is_deterministic() {
    let Some((sprite, pal, _)) = load_trooa1() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let profile = synthetic_profile();
    let anchor = HeadAnchor::front((21.0, 12.0), (8.0, 9.0), 0.2);
    let opts = CompositeOptions::default();
    let a = composite_front(&sprite, &pal, &anchor, &profile, &opts).unwrap();
    let b = composite_front(&sprite, &pal, &anchor, &profile, &opts).unwrap();
    assert_eq!(a, b);
}

#[test]
fn face_view_rejects_bad_size() {
    let rgba = RgbaImage::new(64, 64);
    let mask = AlphaMask::zeros(64, 64);
    assert!(FaceView::new(rgba, mask).is_err());
}
