//! Doom/ZDoom FON1 rasterization onto RGBA bitmaps (2D canvas).

use room_formats::Fon1;

/// Destination + pen for [`Fon1Face::draw_text`].
pub struct TextDraw<'a> {
    /// RGBA buffer.
    pub rgba: &'a mut [u8],
    /// Buffer width.
    pub buf_w: u32,
    /// Buffer height.
    pub buf_h: u32,
    /// Left X.
    pub x: i32,
    /// Top Y.
    pub y: i32,
    /// RGBA ink.
    pub color: [u8; 4],
}

/// CPU font face backed by a parsed `FON1` lump.
#[derive(Clone, Debug)]
pub struct Fon1Face {
    fon: Fon1,
}

impl Fon1Face {
    /// Wrap a parsed FON1.
    #[must_use]
    pub fn new(fon: Fon1) -> Self {
        Self { fon }
    }

    /// Glyph cell size.
    #[must_use]
    pub fn cell(&self) -> (u16, u16) {
        (self.fon.char_width, self.fon.char_height)
    }

    /// Blit ASCII/Latin-1 `text` into an RGBA buffer.
    pub fn draw_text(&self, dest: &mut TextDraw<'_>, text: &str) {
        let (cw, _ch) = (
            i32::from(self.fon.char_width),
            i32::from(self.fon.char_height),
        );
        let mut cx = dest.x;
        for ch_u in text.chars() {
            let code = u32::from(ch_u) as usize;
            if code >= self.fon.glyphs.len() {
                cx += cw;
                continue;
            }
            let glyph = &self.fon.glyphs[code];
            for gy in 0..i32::from(glyph.height) {
                for gx in 0..i32::from(glyph.width) {
                    let idx = (gy as usize) * usize::from(glyph.width) + gx as usize;
                    if glyph.indices.get(idx).copied().unwrap_or(0) == 0 {
                        continue;
                    }
                    let px = cx + gx;
                    let py = dest.y + gy;
                    if px < 0 || py < 0 {
                        continue;
                    }
                    let px = px as u32;
                    let py = py as u32;
                    if px >= dest.buf_w || py >= dest.buf_h {
                        continue;
                    }
                    let o = ((py * dest.buf_w + px) * 4) as usize;
                    if o + 3 < dest.rgba.len() {
                        dest.rgba[o] = dest.color[0];
                        dest.rgba[o + 1] = dest.color[1];
                        dest.rgba[o + 2] = dest.color[2];
                        dest.rgba[o + 3] = dest.color[3];
                    }
                }
            }
            cx += cw;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_formats::{Fon1, FontGlyph};

    fn tiny_fon1() -> Fon1 {
        let mut glyphs = Vec::with_capacity(256);
        for c in 0..256u16 {
            let mut indices = vec![0u8; 8];
            if c == u16::from(b'A') {
                indices = vec![1, 1, 1, 1, 1, 1, 1, 1];
            }
            glyphs.push(FontGlyph {
                width: 2,
                height: 4,
                indices,
            });
        }
        Fon1 {
            char_width: 2,
            char_height: 4,
            glyphs,
        }
    }

    #[test]
    fn draws_opaque_pixels() {
        let face = Fon1Face::new(tiny_fon1());
        let mut buf = vec![0u8; 16 * 8 * 4];
        let mut dest = TextDraw {
            rgba: &mut buf,
            buf_w: 16,
            buf_h: 8,
            x: 0,
            y: 0,
            color: [255, 0, 0, 255],
        };
        face.draw_text(&mut dest, "A");
        assert!(buf.contains(&255));
    }
}
