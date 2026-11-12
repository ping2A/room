//! Full-screen 2D pages drawn the way Chocolate Duke draws them: `rotatesprite`
//! in the 320×200 plane plus the three ART fonts (`menutext` BIGALPHANUM,
//! `gametext` STARTALPHANUM, `minitext` MINIFONT). Used by the bonus
//! screen and other front-end pages that are not part of the 3D view.

use room_build_engine::soft::{SoftFrame, SoftTiles};

const BIGALPHANUM: i16 = 2940;
const STARTALPHANUM: i16 = 2822;
const ENDALPHANUM: i16 = 2915;
const MINIFONT: i16 = 3072;
const BIGPERIOD: i16 = 3002;
const BIGCOMMA: i16 = 3003;
const BIGX: i16 = 3004;
const BIGQ: i16 = 3005;
const BIGSEMI: i16 = 3006;
const BIGCOLIN: i16 = 3007;
const BIGAPPOS: i16 = 3022;

/// A palette-indexed page with Chocolate's 320×200 `rotatesprite` mapping.
pub struct UiPage<'a> {
    tiles: &'a SoftTiles,
    frame: &'a mut SoftFrame,
    sx: f64,
    sy: f64,
}

impl<'a> UiPage<'a> {
    /// Wrap `frame` (any size) as a 320×200 page.
    pub fn new(tiles: &'a SoftTiles, frame: &'a mut SoftFrame) -> Self {
        let sx = f64::from(frame.width) / 320.0;
        let sy = f64::from(frame.height) / 200.0;
        Self { tiles, frame, sx, sy }
    }

    /// Fill the page with one palette index.
    pub fn clear(&mut self, index: u8) {
        self.frame.pixels.fill(index);
    }

    /// Tile width in ART pixels (0 when missing).
    #[must_use]
    pub fn tile_width(&self, pic: i16) -> i32 {
        self.tiles.dimensions(pic).map_or(0, |(w, _)| i32::from(w))
    }

    /// `rotatesprite` at zoom 65536: `top_left` is flag 16; otherwise (x, y)
    /// is the tile centre plus its picanm offset. `opaque` is flag 64.
    pub fn sprite(&mut self, x: i32, y: i32, pic: i16, shade: i8, pal: u8, top_left: bool, opaque: bool) {
        let Some((tw, th)) = self.tiles.dimensions(pic) else {
            return;
        };
        let (mut left, mut top) = (x, y);
        if !top_left {
            let (ox, oy) = self.tiles.sprite_offset(pic);
            left -= i32::from(ox) + (i32::from(tw) >> 1);
            top -= i32::from(oy) + (i32::from(th) >> 1);
        }
        let x0 = (f64::from(left) * self.sx).round() as i32;
        let y0 = (f64::from(top) * self.sy).round() as i32;
        let x1 = (f64::from(left + i32::from(tw)) * self.sx).round() as i32;
        let y1 = (f64::from(top + i32::from(th)) * self.sy).round() as i32;
        let (dw, dh) = ((x1 - x0).max(1), (y1 - y0).max(1));
        let (fw, fh) = (self.frame.width as i32, self.frame.height as i32);
        for yy in 0..dh {
            let dy = y0 + yy;
            if dy < 0 || dy >= fh {
                continue;
            }
            let v = (i64::from(yy) * i64::from(th) / i64::from(dh)) as u32;
            for xx in 0..dw {
                let dx = x0 + xx;
                if dx < 0 || dx >= fw {
                    continue;
                }
                let u = (i64::from(xx) * i64::from(tw) / i64::from(dw)) as u32;
                let Some(p) = self.tiles.sample(pic, u, v) else {
                    continue;
                };
                if p == room_build::art::TRANSPARENT && !opaque {
                    continue;
                }
                let p = self.tiles.shade_palette(shade, self.tiles.remap_palette(pal, p));
                self.frame.pixels[(dy * fw + dx) as usize] = p;
            }
        }
    }

    fn big_glyph(c: u8, colon: i16) -> Option<i16> {
        match c {
            b'0'..=b'9' => Some(BIGALPHANUM - 10 + i16::from(c - b'0')),
            b'a'..=b'z' => Some(BIGALPHANUM + i16::from(c.to_ascii_uppercase() - b'A')),
            b'A'..=b'Z' => Some(BIGALPHANUM + i16::from(c - b'A')),
            b'-' => Some(BIGALPHANUM - 11),
            b'.' => Some(BIGPERIOD),
            b',' => Some(BIGCOMMA),
            b'!' => Some(BIGX),
            b'\'' => Some(BIGAPPOS),
            b'?' => Some(BIGQ),
            b';' => Some(BIGSEMI),
            b':' => Some(colon),
            _ => None,
        }
    }

    /// Chocolate `menutext`: `y -= 12`; x == 160 centres the line.
    pub fn menutext(&mut self, mut x: i32, y: i32, shade: i8, pal: u8, text: &str) {
        let y = y - 12;
        if x == 160 {
            let mut centre = 0;
            for c in text.bytes() {
                // The measuring pass maps ':' to BIGSEMI like Chocolate.
                match Self::big_glyph(c, BIGSEMI) {
                    Some(ac) if c != b' ' => centre += self.tile_width(ac) - 1,
                    _ => centre += 5,
                }
            }
            if centre != 0 {
                x = (320 - centre - 10) >> 1;
            }
        }
        for c in text.bytes() {
            let Some(ac) = (c != b' ').then(|| Self::big_glyph(c, BIGCOLIN)).flatten() else {
                x += 5;
                continue;
            };
            self.sprite(x, y, ac, shade, pal, true, false);
            x += self.tile_width(ac);
        }
    }

    /// Chocolate `gametext` with flags 2+8+16 (top-left glyphs); x == 160
    /// centres. Digits advance 8 px.
    pub fn gametext(&mut self, mut x: i32, y: i32, text: &str, shade: i8) {
        let glyph = |c: u8| -> Option<i16> {
            let ac = i16::from(c) - i16::from(b'!') + STARTALPHANUM;
            (STARTALPHANUM..=ENDALPHANUM).contains(&ac).then_some(ac)
        };
        if x == 160 {
            let mut width = 0;
            for c in text.bytes() {
                if c == b' ' {
                    width += 5;
                    continue;
                }
                let Some(ac) = glyph(c) else { break };
                width += if c.is_ascii_digit() { 8 } else { self.tile_width(ac) };
            }
            x = 160 - (width >> 1);
        }
        for c in text.bytes() {
            if c == b' ' {
                x += 5;
                continue;
            }
            let Some(ac) = glyph(c) else { break };
            self.sprite(x, y, ac, shade, 0, true, false);
            x += if c.is_ascii_digit() { 8 } else { self.tile_width(ac) };
        }
    }

    /// Chocolate `minitext` (flag 16): uppercase MINIFONT, 4 px advance.
    pub fn minitext(&mut self, mut x: i32, y: i32, text: &str, pal: u8) {
        for c in text.bytes().map(|c| c.to_ascii_uppercase()) {
            if c == b' ' {
                x += 5;
                continue;
            }
            self.sprite(x, y, MINIFONT + i16::from(c) - i16::from(b'!'), 0, pal, true, false);
            x += 4;
        }
    }
}
