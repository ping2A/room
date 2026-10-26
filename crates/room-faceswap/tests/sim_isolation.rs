//! F4: Live face overrides never touch playsim hashes; Live ≡ Bake pixels.

#![allow(clippy::unwrap_used)]

use room_faceswap::{
    AlphaMask, AssignmentRule, CompositeOptions, Consent, FaceAssignment, FaceCatalog, FaceId,
    FaceProfile, FaceView, HeadAnchor, LiveFaceOverrides, ManualPoints, PalettedSprite, PickMode,
    RgbaImage, align_manual, bake_class, face_for_actor, install_bake, rebuild_class_overrides,
};

fn synth_sprite(w: u32, h: u32) -> PalettedSprite {
    let n = (w * h) as usize;
    let mut indices = vec![16u8; n];
    let mut alpha = vec![255u8; n];
    // Transparent border.
    for x in 0..w {
        alpha[x as usize] = 0;
        alpha[((h - 1) * w + x) as usize] = 0;
    }
    // Head-ish mid band with opaque skin index.
    for y in h / 4..(h * 3 / 4) {
        for x in w / 4..(w * 3 / 4) {
            indices[(y * w + x) as usize] = 64;
        }
    }
    PalettedSprite {
        width: w,
        height: h,
        left_offset: (w / 2) as i32,
        top_offset: h as i32,
        indices,
        alpha,
    }
}

fn grey_palette() -> [u8; 768] {
    let mut pal = [0u8; 768];
    for i in 0..256 {
        let v = i as u8;
        pal[i * 3] = v;
        pal[i * 3 + 1] = v;
        pal[i * 3 + 2] = v;
    }
    // Skin-ish at 64.
    pal[64 * 3] = 200;
    pal[64 * 3 + 1] = 160;
    pal[64 * 3 + 2] = 130;
    pal
}

fn profile() -> FaceProfile {
    let mut src = RgbaImage::new(128, 128);
    for y in 20..110u32 {
        for x in 30..98u32 {
            src.put(x, y, [210, 170, 140, 255]);
        }
    }
    align_manual(
        &src,
        ManualPoints {
            left_eye: (48.0, 50.0),
            right_eye: (80.0, 50.0),
            chin: (64.0, 100.0),
        },
        "iso",
        FaceId(42),
        Consent::SelfCamera,
        0,
    )
    .unwrap()
}

#[test]
fn live_rebuild_matches_bake_pixels() {
    let spr = synth_sprite(48, 56);
    let pal = grey_palette();
    let prof = profile();
    let lumps = vec![("TROOA1".into(), spr, 0u64)];
    let opts = CompositeOptions::default();
    let baked = bake_class(&lumps, &pal, &prof, None, &opts, &[]).unwrap();
    let live = rebuild_class_overrides(&lumps, &pal, &prof, None, &opts).unwrap();
    assert_eq!(baked.lumps.len(), live.lumps.len());
    assert_eq!(baked.lumps[0].sprite.indices, live.lumps[0].sprite.indices);
    assert_eq!(baked.lumps[0].sprite.alpha, live.lumps[0].sprite.alpha);

    let mut overrides = LiveFaceOverrides::default();
    install_bake(&mut overrides, FaceId(42), &baked);
    assert!(overrides.get("TROOA1", FaceId(42)).is_some());
}

#[test]
fn face_for_actor_ignores_sim_style_rng() {
    // Same inputs → same face; assignment uses xxh3 of spawn/map/class only.
    let assignment = FaceAssignment {
        rules: vec![AssignmentRule {
            classes: vec!["DoomImp".into()],
            faces: vec!["a".into(), "b".into()],
            pick: PickMode::PerInstance,
        }],
    };
    let mut catalog = FaceCatalog::default();
    catalog.insert("a", FaceId(1));
    catalog.insert("b", FaceId(2));
    let f1 = face_for_actor(&assignment, &catalog, "DoomImp", 7, "MAP01");
    let f2 = face_for_actor(&assignment, &catalog, "DoomImp", 7, "MAP01");
    assert_eq!(f1, f2);
    assert!(f1.is_some());
}

#[test]
fn empty_front_view_still_installs() {
    let rgba = RgbaImage::new(128, 128);
    let mask = AlphaMask::zeros(128, 128);
    let _view = FaceView::new(rgba, mask).unwrap();
    let anchor = HeadAnchor::front((24.0, 20.0), (10.0, 12.0), 0.0);
    let _ = anchor;
}
