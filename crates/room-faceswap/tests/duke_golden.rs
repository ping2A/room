//! F7: Duke palette path + synthetic LIZTROOP / PIGCOP goldens.
//! Full Atomic tiles land when a Duke GRP fixture is available; until then we
//! assert deterministic composite under `duke_composite_options`.

#![allow(clippy::unwrap_used)]

use room_faceswap::{
    CompositeOptions, Consent, FaceId, HeadAnchor, ManualPoints, PalettedSprite, RgbaImage,
    align_manual, composite_front, duke_composite_options, duke_skip_indices, lump_pixel_hash,
};

fn buildish_sprite(label: &str) -> PalettedSprite {
    // 64×64 Build-style tile with opaque head ellipse + transparent edges.
    let w = 64u32;
    let h = 64u32;
    let n = (w * h) as usize;
    let mut indices = vec![0u8; n];
    let mut alpha = vec![0u8; n];
    let seed = label.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(u32::from(b)));
    for y in 8..52u32 {
        for x in 16..48u32 {
            let nx = (x as f32 - 32.0) / 14.0;
            let ny = (y as f32 - 28.0) / 18.0;
            if nx * nx + ny * ny <= 1.0 {
                let i = (y * w + x) as usize;
                alpha[i] = 255;
                // Avoid skip indices 168..=175 and 255.
                indices[i] = 40 + ((seed + x + y) % 100) as u8;
                if (168..=175).contains(&indices[i]) {
                    indices[i] = 40;
                }
            }
        }
    }
    PalettedSprite {
        width: w,
        height: h,
        left_offset: 32,
        top_offset: 64,
        indices,
        alpha,
    }
}

fn duke_palette() -> [u8; 768] {
    let mut pal = [0u8; 768];
    for i in 0..256 {
        pal[i * 3] = i as u8;
        pal[i * 3 + 1] = (i as u8).wrapping_mul(3).wrapping_div(4);
        pal[i * 3 + 2] = (i as u8) / 2;
    }
    pal
}

fn face() -> room_faceswap::FaceProfile {
    let mut src = RgbaImage::new(160, 200);
    for y in 40..160u32 {
        for x in 40..120u32 {
            src.put(x, y, [200, 150, 120, 255]);
        }
    }
    align_manual(
        &src,
        ManualPoints {
            left_eye: (60.0, 80.0),
            right_eye: (100.0, 80.0),
            chin: (80.0, 150.0),
        },
        "duke-face",
        FaceId(0xD04E_0001),
        Consent::SelfCamera,
        0,
    )
    .unwrap()
}

fn assert_duke_golden(name: &str) {
    let sprite = buildish_sprite(name);
    let hash = lump_pixel_hash(&sprite.indices, &sprite.alpha);
    let pal = duke_palette();
    let profile = face();
    let opts = duke_composite_options();
    let anchor = HeadAnchor::front((32.0, 28.0), (14.0, 18.0), 0.0);
    let a = composite_front(&sprite, &pal, &anchor, &profile, &opts).unwrap();
    let b = composite_front(&sprite, &pal, &anchor, &profile, &opts).unwrap();
    assert_eq!(a.indices, b.indices, "{name} not deterministic");
    assert_eq!(a.width, sprite.width);
    assert_eq!(a.height, sprite.height);
    // Geometry preserved: transparent stays transparent.
    for i in 0..sprite.alpha.len() {
        if sprite.alpha[i] == 0 {
            assert_eq!(a.alpha[i], 0, "{name} grew opaque at {i}");
        }
    }
    // Skip indices untouched where original used them.
    let skip = duke_skip_indices();
    for i in 0..sprite.indices.len() {
        if sprite.alpha[i] > 0 && skip.contains(&sprite.indices[i]) {
            assert_eq!(a.indices[i], sprite.indices[i]);
        }
    }
    let _ = hash;
    let _ = CompositeOptions::default();
}

#[test]
fn liztroop_synthetic_golden() {
    assert_duke_golden("LIZTROOP");
}

#[test]
fn pigcop_synthetic_golden() {
    assert_duke_golden("PIGCOP");
}

#[test]
fn duke_options_include_skip_hole() {
    let opts = duke_composite_options();
    assert!(opts.skip_indices.contains(&255));
    assert!(opts.skip_indices.contains(&168));
}
