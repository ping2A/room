//! Anti-aliased text for the launcher screen (`fonts/`, SIL OFL): Big
//! Shoulders Display for titles, Barlow for everything else. Rendered to
//! RGBA images the launcher caches; no game data needed.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use std::sync::OnceLock;

/// A typeface of the launcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Face {
    /// Big Shoulders Display ExtraBold: game and page titles.
    Display,
    /// Barlow Medium: rows.
    Body,
    /// Barlow SemiBold: the selected row, buttons.
    Strong,
}

static DISPLAY: &[u8] = include_bytes!("../fonts/BigShouldersDisplay-ExtraBold.ttf");
static BODY: &[u8] = include_bytes!("../fonts/Barlow-Medium.ttf");
static STRONG: &[u8] = include_bytes!("../fonts/Barlow-SemiBold.ttf");

fn font(face: Face) -> Option<&'static FontRef<'static>> {
    static FONTS: OnceLock<[Option<FontRef<'static>>; 3]> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        [
            FontRef::try_from_slice(DISPLAY).ok(),
            FontRef::try_from_slice(BODY).ok(),
            FontRef::try_from_slice(STRONG).ok(),
        ]
    });
    fonts[match face {
        Face::Display => 0,
        Face::Body => 1,
        Face::Strong => 2,
    }]
    .as_ref()
}

/// Width of `text` at `px` (pixels per em), with kerning.
pub(crate) fn width(text: &str, face: Face, px: f32) -> f32 {
    let Some(font) = font(face) else {
        return 0.0;
    };
    let f = font.as_scaled(PxScale::from(px));
    let mut w = 0.0;
    let mut prev = None;
    for c in text.chars() {
        let id = f.glyph_id(c);
        if let Some(p) = prev {
            w += f.kern(p, id);
        }
        w += f.h_advance(id);
        prev = Some(id);
    }
    w
}

/// Line height (ascent + descent) at `px`.
pub(crate) fn line_height(face: Face, px: f32) -> f32 {
    font(face).map_or(px, |font| {
        let f = font.as_scaled(PxScale::from(px));
        f.ascent() - f.descent()
    })
}

/// `text` cut with "…" to fit `max_w` pixels.
pub(crate) fn fit(text: &str, face: Face, px: f32, max_w: f32) -> String {
    if width(text, face, px) <= max_w {
        return text.to_owned();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let cut: String = chars.iter().collect::<String>().trim_end().to_owned() + "…";
        if width(&cut, face, px) <= max_w {
            return cut;
        }
    }
    String::new()
}

/// `text` as straight-alpha RGBA in `color` (alpha scaled by coverage),
/// `line_height` tall, plus its width and height.
pub(crate) fn render(text: &str, face: Face, px: f32, color: [u8; 4]) -> (u32, u32, Vec<u8>) {
    let Some(font) = font(face) else {
        return (1, 1, vec![0; 4]);
    };
    let f = font.as_scaled(PxScale::from(px));
    let w = width(text, face, px).ceil().max(1.0) as u32 + 2;
    let h = (f.ascent() - f.descent()).ceil().max(1.0) as u32;
    let mut cover = vec![0f32; (w * h) as usize];
    let mut x = 1.0f32;
    let mut prev = None;
    for c in text.chars() {
        let id = f.glyph_id(c);
        if let Some(p) = prev {
            x += f.kern(p, id);
        }
        let glyph = id.with_scale_and_position(PxScale::from(px), ab_glyph::point(x, f.ascent()));
        x += f.h_advance(id);
        prev = Some(id);
        let Some(outline) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outline.px_bounds();
        outline.draw(|gx, gy, v| {
            let px_x = bounds.min.x as i32 + gx as i32;
            let px_y = bounds.min.y as i32 + gy as i32;
            if px_x >= 0 && px_y >= 0 && (px_x as u32) < w && (px_y as u32) < h {
                let i = (px_y as u32 * w + px_x as u32) as usize;
                cover[i] = (cover[i] + v).min(1.0);
            }
        });
    }
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for (i, c) in cover.iter().enumerate() {
        if *c <= 0.0 {
            continue;
        }
        let o = i * 4;
        rgba[o..o + 3].copy_from_slice(&color[..3]);
        rgba[o + 3] = (f32::from(color[3]) * c).round() as u8;
    }
    (w, h, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fonts_load_and_draw() {
        for face in [Face::Display, Face::Body, Face::Strong] {
            assert!(font(face).is_some(), "{face:?}");
            let (w, h, rgba) = render("Doom II", face, 32.0, [255, 255, 255, 255]);
            assert_eq!(rgba.len(), (w * h * 4) as usize);
            assert!(w > 40 && h >= 28, "{face:?} {w}x{h}");
            // Some pixels fully covered, some partly (anti-aliased).
            let alphas: Vec<u8> = rgba.chunks_exact(4).map(|p| p[3]).collect();
            assert!(alphas.contains(&255));
            assert!(alphas.iter().any(|&a| a > 0 && a < 255));
        }
    }

    #[test]
    fn long_text_is_cut_to_fit() {
        let long = "Freedoom: Phase 2 with a very long name indeed";
        let cut = fit(long, Face::Body, 20.0, 150.0);
        assert!(cut.ends_with('…'), "{cut}");
        assert!(width(&cut, Face::Body, 20.0) <= 150.0);
        assert_eq!(fit("Doom", Face::Body, 20.0, 500.0), "Doom");
    }
}
