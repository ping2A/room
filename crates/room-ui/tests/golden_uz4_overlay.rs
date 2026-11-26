//! UZ4 acceptance: a weapon with a second overlay layer using pivot, rotate
//! and scale (`A_OverlayPivot`, `A_OverlayRotate`, `A_OverlayScale`) goes
//! through the shells' CPU rasterizer and is compared against
//! `testdata/goldens/uz4_overlay_pivot_rotate.txt`, a character picture of
//! the 160×100 result (one character per 2×2 pixels).
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-ui --test golden_uz4_overlay`

// Test helpers: a failed setup step should panic the test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_ui::{
    CanvasSink, PatchCache, SoftPatch, UiDrawList, blit_draw_list_full, draw_weapon_tinted,
};

const W: usize = 160;
const H: usize = 100;

/// A straight-alpha RGBA framebuffer.
struct Fb(Vec<[f32; 3]>);

impl CanvasSink for Fb {
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) {
        let (x0, y0) = (x.round().max(0.0) as usize, y.round().max(0.0) as usize);
        let (x1, y1) = (
            ((x + w).round() as usize).min(W),
            ((y + h).round() as usize).min(H),
        );
        for yy in y0..y1 {
            for xx in x0..x1 {
                let p = &mut self.0[yy * W + xx];
                for k in 0..3 {
                    p[k] = p[k] * (1.0 - c[3]) + c[k] * c[3];
                }
            }
        }
    }
    fn text(&mut self, _: f32, _: f32, _: &str, _: [f32; 4]) {}
}

/// An RGBA patch: `paint(x, y)` gives the pixel.
fn patch(w: u16, h: u16, left: i16, top: i16, paint: impl Fn(u16, u16) -> [u8; 4]) -> SoftPatch {
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            rgba.extend(paint(x, y));
        }
    }
    SoftPatch {
        width: w,
        height: h,
        left_offset: left,
        top_offset: top,
        indices: vec![0; usize::from(w) * usize::from(h)],
        alpha: vec![255; usize::from(w) * usize::from(h)],
        rgba: Some(rgba),
    }
}

/// Dominant colour of a pixel as a character.
fn glyph(p: [f32; 3]) -> char {
    let [r, g, b] = p;
    if r.max(g).max(b) < 0.2 {
        '.'
    } else if r > 0.5 && g > 0.5 {
        'Y'
    } else if r >= g && r >= b {
        'R'
    } else if g >= b {
        'G'
    } else {
        'B'
    }
}

#[test]
fn weapon_overlay_with_pivot_rotate_and_scale_matches_its_golden() {
    let mut cache = PatchCache::default();
    // Gun: 32×16, red left half, blue right half, a green top row.
    cache.insert(
        "TGUNA0",
        patch(32, 16, -64, -100, |x, y| {
            if y == 0 {
                [0, 255, 0, 255]
            } else if x < 16 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        }),
    );
    // Overlay: 16×8, yellow with a green right column.
    cache.insert(
        "TFLAA0",
        patch(16, 8, -120, -60, |x, _| {
            if x == 15 {
                [0, 255, 0, 255]
            } else {
                [255, 255, 0, 255]
            }
        }),
    );
    let mut list = UiDrawList::new();
    let gun = cache.get("TGUNA0").cloned();
    let flash = cache.get("TFLAA0").cloned();
    // Layer 1: the gun, `A_OverlayScale(1, 1.5)`, `A_OverlayRotate(1, 15)`,
    // centre pivot.
    draw_weapon_tinted(
        &mut list,
        "TGUNA0",
        true,
        gun.as_ref(),
        0.0,
        0.0,
        1.0,
        true,
        false,
        1.0,
        1.5,
        1.5,
        false,
        0.5,
        0.5,
        15.0,
        1.0,
        1.0,
        1.0,
        false,
        true,
    );
    // Layer 2: `A_OverlayPivot(2, 0, 0)`, `A_OverlayScale(2, 2, 1)`,
    // `A_OverlayRotate(2, 90)`: turned a quarter turn about its top-left.
    draw_weapon_tinted(
        &mut list,
        "TFLAA0",
        true,
        flash.as_ref(),
        0.0,
        0.0,
        1.0,
        true,
        false,
        1.0,
        2.0,
        1.0,
        false,
        0.0,
        0.0,
        90.0,
        1.0,
        1.0,
        1.0,
        false,
        true,
    );
    let mut fb = Fb(vec![[0.0; 3]; W * H]);
    blit_draw_list_full(&list, W as f32, H as f32, None, Some(&cache), &mut fb);
    let mut picture = String::new();
    for y in (0..H).step_by(2) {
        for x in (0..W).step_by(2) {
            picture.push(glyph(fb.0[y * W + x]));
        }
        picture.push('\n');
    }
    // Both layers are on screen and the rotated overlay stands upright (it
    // was 2:1 wide; turned 90° it is taller than wide).
    assert!(
        picture.contains('R') && picture.contains('B') && picture.contains('Y'),
        "\n{picture}"
    );
    let rows_with_y = picture.lines().filter(|l| l.contains('Y')).count();
    let widest_y = picture
        .lines()
        .map(|l| l.matches('Y').count())
        .max()
        .unwrap_or(0);
    assert!(rows_with_y > widest_y, "overlay not turned:\n{picture}");
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/goldens/uz4_overlay_pivot_rotate.txt");
    if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &picture).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        picture,
        want,
        "overlay picture differs from {}",
        path.display()
    );
}
