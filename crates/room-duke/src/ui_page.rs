//! Full-screen 2D pages drawn the way Chocolate Duke draws them: `rotatesprite`
//! in the 320×200 plane plus the three ART fonts (`menutext` BIGALPHANUM,
//! `gametext` STARTALPHANUM, `minitext` MINIFONT). Used by the bonus
//! screen and other front-end pages that are not part of the 3D view.

use room_build_engine::soft::{SoftFrame, SoftTiles};
use room_build_engine::world::{HudGameTextDraw, HudNumberDraw, HudSpriteDraw, HudTextDraw};

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
        Self {
            tiles,
            frame,
            sx,
            sy,
        }
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
    pub fn sprite(
        &mut self,
        x: i32,
        y: i32,
        pic: i16,
        shade: i8,
        pal: u8,
        top_left: bool,
        opaque: bool,
    ) {
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
                let p = self
                    .tiles
                    .shade_palette(shade, self.tiles.remap_palette(pal, p));
                self.frame.pixels[(dy * fw + dx) as usize] = p;
            }
        }
    }

    /// Draw an EDuke32 CON `rotatesprite` command. Build's HUD plane stays
    /// 320×200 at every framebuffer size; rotation and zoom are presentation
    /// math and do not enter the deterministic playsim.
    pub fn rotatesprite(&mut self, draw: HudSpriteDraw) {
        let Ok(pic) = i16::try_from(draw.tile) else {
            return;
        };
        let Some((tile_width, tile_height)) = self.tiles.dimensions(pic) else {
            return;
        };
        if tile_width == 0 || tile_height == 0 || draw.zoom == 0 {
            return;
        }

        const FULL16: i32 = 2048;
        const YFLIP: i32 = 4;
        const TOPLEFT: i32 = 16;
        const NOMASK: i32 = 64;
        let x = if draw.orientation & FULL16 != 0 {
            f64::from(draw.x) / 65536.0
        } else {
            f64::from(draw.x)
        };
        let y = if draw.orientation & FULL16 != 0 {
            f64::from(draw.y) / 65536.0
        } else {
            f64::from(draw.y)
        };
        let zoom = f64::from(draw.zoom.unsigned_abs()) / 65536.0;
        if !zoom.is_finite() || zoom > 4096.0 {
            return;
        }
        let angle = f64::from(draw.angle.rem_euclid(2048)) * std::f64::consts::TAU / 2048.0;
        let (sin, cos) = angle.sin_cos();
        let (tw, th) = (f64::from(tile_width), f64::from(tile_height));
        let (offset_x, offset_y) = self.tiles.sprite_offset(pic);
        let (offset_x, offset_y) = if draw.orientation & TOPLEFT != 0 {
            (0.0, 0.0)
        } else {
            (f64::from(offset_x), f64::from(offset_y))
        };
        let left = if draw.orientation & TOPLEFT != 0 {
            x
        } else {
            x - tw / 2.0 - offset_x
        };
        let top = if draw.orientation & TOPLEFT != 0 {
            y
        } else {
            y - th / 2.0 - offset_y
        };

        // Bound the forward-transformed tile, then inverse-map each output
        // pixel so rotated sprites do not leave holes between source pixels.
        let corners = [(0.0, 0.0), (tw, 0.0), (0.0, th), (tw, th)];
        let mut bounds = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for (u, v) in corners {
            let px = left + u * zoom;
            let py = top + v * zoom;
            let rx = x + (px - x) * cos - (py - y) * sin;
            let ry = y + (px - x) * sin + (py - y) * cos;
            bounds[0] = bounds[0].min(rx);
            bounds[1] = bounds[1].min(ry);
            bounds[2] = bounds[2].max(rx);
            bounds[3] = bounds[3].max(ry);
        }
        let clip = draw.clip;
        let (min_x, min_y, max_x, max_y) = (
            f64::from(clip[0].min(clip[2])).max(bounds[0]),
            f64::from(clip[1].min(clip[3])).max(bounds[1]),
            f64::from(clip[0].max(clip[2]) + 1).min(bounds[2]),
            f64::from(clip[1].max(clip[3]) + 1).min(bounds[3]),
        );
        let x0 = (min_x * self.sx).floor().max(0.0) as i32;
        let y0 = (min_y * self.sy).floor().max(0.0) as i32;
        let x1 = (max_x * self.sx).ceil().min(f64::from(self.frame.width)) as i32;
        let y1 = (max_y * self.sy).ceil().min(f64::from(self.frame.height)) as i32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let fw = self.frame.width as i32;
        let shade = draw.shade.clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8;
        let palette = draw.palette.clamp(0, i32::from(u8::MAX)) as u8;
        let no_mask = draw.orientation & NOMASK != 0;
        for screen_y in y0..y1 {
            let hud_y = (f64::from(screen_y) + 0.5) / self.sy;
            for screen_x in x0..x1 {
                let hud_x = (f64::from(screen_x) + 0.5) / self.sx;
                if hud_x < f64::from(clip[0].min(clip[2]))
                    || hud_x >= f64::from(clip[0].max(clip[2]) + 1)
                    || hud_y < f64::from(clip[1].min(clip[3]))
                    || hud_y >= f64::from(clip[1].max(clip[3]) + 1)
                {
                    continue;
                }
                let rx = hud_x - x;
                let ry = hud_y - y;
                let px = x + rx * cos + ry * sin;
                let py = y - rx * sin + ry * cos;
                let u = (px - left) / zoom;
                let mut v = (py - top) / zoom;
                if u < 0.0 || v < 0.0 || u >= tw || v >= th {
                    continue;
                }
                if draw.orientation & YFLIP != 0 {
                    v = th - 1.0 - v;
                }
                let Some(pixel) = self.tiles.sample(pic, u as u32, v as u32) else {
                    continue;
                };
                if pixel == room_build::art::TRANSPARENT && !no_mask {
                    continue;
                }
                let pixel = self
                    .tiles
                    .shade_palette(shade, self.tiles.remap_palette(palette, pixel));
                let index = (screen_y * fw + screen_x) as usize;
                self.frame.pixels[index] = if no_mask {
                    pixel
                } else {
                    self.tiles.composite_hud_pixel(
                        self.frame.pixels[index],
                        pixel,
                        draw.orientation,
                    )
                };
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
                width += if c.is_ascii_digit() {
                    8
                } else {
                    self.tile_width(ac)
                };
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
            x += if c.is_ascii_digit() {
                8
            } else {
                self.tile_width(ac)
            };
        }
    }

    /// Chocolate `minitext` (flag 16): uppercase MINIFONT, 4 px advance.
    pub fn minitext(&mut self, mut x: i32, y: i32, text: &str, pal: u8) {
        for c in text.bytes().map(|c| c.to_ascii_uppercase()) {
            if c == b' ' {
                x += 5;
                continue;
            }
            self.sprite(
                x,
                y,
                MINIFONT + i16::from(c) - i16::from(b'!'),
                0,
                pal,
                true,
                false,
            );
            x += 4;
        }
    }

    /// Draw a captured EDuke32 `minitext` quote with its shade and palette.
    pub fn draw_minitext(&mut self, draw: &HudTextDraw) {
        let mut x = draw.x;
        let mut palette = draw.palette.clamp(0, i32::from(u8::MAX)) as u8;
        let shade = draw.shade.clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8;
        let mut chars = draw.text.bytes().peekable();
        while let Some(byte) = chars.next() {
            if byte == b'^' && chars.peek().is_some_and(u8::is_ascii_digit) {
                let mut color = 0_u16;
                for _ in 0..3 {
                    if !chars.peek().is_some_and(u8::is_ascii_digit) {
                        break;
                    }
                    if let Some(digit) = chars.next() {
                        color = color
                            .saturating_mul(10)
                            .saturating_add(u16::from(digit - b'0'));
                    }
                }
                palette = color.min(u16::from(u8::MAX)) as u8;
                continue;
            }
            let byte = byte.to_ascii_uppercase();
            if byte == b' ' {
                x += 5;
                continue;
            }
            if (b'!'..=b'~').contains(&byte) {
                let tile = MINIFONT + i16::from(byte - b'!');
                self.sprite(x, draw.y, tile, shade, palette, true, false);
                x += 4;
            }
        }
    }

    /// Draw EDuke32 `gametext` / `gametextz` using the supplied ASCII-order
    /// font tiles, legacy x coordinate, quote palette escapes and clip window.
    pub fn draw_gametext(&mut self, draw: &HudGameTextDraw) {
        const FULL16: i32 = 2048;
        const TOPLEFT: i32 = 16;
        if draw.zoom == 0 {
            return;
        }
        let Ok(first_tile) = i16::try_from(draw.tile) else {
            return;
        };
        let zoom = f64::from(draw.zoom.unsigned_abs()) / 65536.0;
        if !zoom.is_finite() || zoom > 4096.0 {
            return;
        }

        let mut palette = draw.palette.clamp(0, i32::from(u8::MAX)) as u8;
        let mut chars = Vec::new();
        let mut bytes = draw.text.bytes().peekable();
        while let Some(byte) = bytes.next() {
            if byte == b'^' && bytes.peek().is_some_and(u8::is_ascii_digit) {
                let mut color = 0_u16;
                for _ in 0..3 {
                    if !bytes.peek().is_some_and(u8::is_ascii_digit) {
                        break;
                    }
                    if let Some(digit) = bytes.next() {
                        color = color
                            .saturating_mul(10)
                            .saturating_add(u16::from(digit - b'0'));
                    }
                }
                palette = color.min(u16::from(u8::MAX)) as u8;
                continue;
            }
            if byte == b' ' {
                chars.push((None, 5.0, palette));
                continue;
            }
            if !(b'!'..=b'~').contains(&byte) {
                break;
            }
            let Some(tile) = first_tile.checked_add(i16::from(byte - b'!')) else {
                break;
            };
            let width = if byte.is_ascii_digit() {
                8.0
            } else {
                self.tiles
                    .dimensions(tile)
                    .map_or(0.0, |(width, _)| f64::from(width))
            };
            chars.push((Some((tile, byte)), width, palette));
        }

        let text_width = chars.iter().map(|(_, width, _)| width * zoom).sum::<f64>();
        let full16 = draw.orientation & FULL16 != 0;
        let coordinate_scale = if full16 { 65536.0 } else { 1.0 };
        let x = if draw.x == 320 {
            160.0 - text_width / 2.0
        } else {
            f64::from(draw.x / 2)
        };
        let y = f64::from(draw.y);
        let mut left = x;
        let shade = draw.shade;
        for (glyph, width, palette) in chars {
            if let Some((tile, byte)) = glyph {
                let (tile_width, tile_height) = self
                    .tiles
                    .dimensions(tile)
                    .map_or((width, 8.0), |(tile_width, tile_height)| {
                        (f64::from(tile_width), f64::from(tile_height))
                    });
                let (glyph_x, glyph_y) = if draw.orientation & TOPLEFT != 0 {
                    (left, y)
                } else {
                    (left + tile_width * zoom / 2.0, y + tile_height * zoom / 2.0)
                };
                let to_coord = |value: f64| {
                    (value * coordinate_scale)
                        .round()
                        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
                };
                self.rotatesprite(HudSpriteDraw {
                    x: to_coord(glyph_x),
                    y: to_coord(glyph_y),
                    zoom: draw.zoom,
                    angle: 0,
                    tile: i32::from(tile),
                    shade,
                    palette: i32::from(palette),
                    orientation: draw.orientation,
                    clip: draw.clip,
                    unique_id: 0,
                });
                left += if byte.is_ascii_digit() {
                    8.0 * zoom
                } else {
                    width * zoom
                };
            } else {
                left += width * zoom;
            }
        }
    }

    /// Draw an EDuke32 `digitalnumber` / `digitalnumberz` value using the
    /// consecutive 0–9 ART tile range. The x coordinate is the text centre;
    /// the optional zoom scales glyphs and spacing in the 320×200 HUD plane.
    pub fn draw_digitalnumber(&mut self, draw: HudNumberDraw) {
        const FULL16: i32 = 2048;
        const TOPLEFT: i32 = 16;
        if draw.zoom == 0 {
            return;
        }
        let Ok(first_tile) = i16::try_from(draw.tile) else {
            return;
        };
        let zoom = f64::from(draw.zoom.unsigned_abs()) / 65536.0;
        if !zoom.is_finite() || zoom > 4096.0 {
            return;
        }

        let mut glyphs = Vec::with_capacity(11);
        if draw.number < 0 {
            if let Some(tile) = first_tile.checked_sub(1) {
                glyphs.push(tile);
            }
        }
        for digit in draw.number.unsigned_abs().to_string().bytes() {
            let Some(tile) = first_tile.checked_add(i16::from(digit - b'0')) else {
                return;
            };
            glyphs.push(tile);
        }

        // EDuke32's default digital-number layout advances by glyph width plus
        // one pixel and centres the complete value around x.
        let metrics: Vec<_> = glyphs
            .iter()
            .map(|&tile| {
                let (width, height) = self
                    .tiles
                    .dimensions(tile)
                    .map_or((4.0, 8.0), |(width, height)| {
                        (f64::from(width), f64::from(height))
                    });
                (tile, width, height)
            })
            .collect();
        let total_width = metrics
            .iter()
            .map(|(_, width, _)| (width + 1.0) * zoom)
            .sum::<f64>()
            - zoom;
        let full16 = draw.orientation & FULL16 != 0;
        let coordinate_scale = if full16 { 65536.0 } else { 1.0 };
        let mut left = if full16 {
            f64::from(draw.x) / 65536.0 - total_width / 2.0
        } else {
            f64::from(draw.x) - total_width / 2.0
        };
        let y = if full16 {
            f64::from(draw.y) / 65536.0
        } else {
            f64::from(draw.y)
        };

        for (tile, width, height) in metrics {
            let (x, y) = if draw.orientation & TOPLEFT != 0 {
                (left, y)
            } else {
                (left + width * zoom / 2.0, y + height * zoom / 2.0)
            };
            let to_coord = |value: f64| {
                (value * coordinate_scale)
                    .round()
                    .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
            };
            self.rotatesprite(HudSpriteDraw {
                x: to_coord(x),
                y: to_coord(y),
                zoom: draw.zoom,
                angle: 0,
                tile: i32::from(tile),
                shade: draw.shade,
                palette: draw.palette,
                orientation: draw.orientation,
                clip: draw.clip,
                unique_id: 0,
            });
            left += (width + 1.0) * zoom;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotatesprite_draws_a_masked_tile_on_the_hud_plane() {
        let mut tiles = SoftTiles::new();
        tiles.insert(9000, 1, 1, vec![37]);
        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).rotatesprite(HudSpriteDraw {
            x: 160,
            y: 100,
            zoom: 65536,
            angle: 0,
            tile: 9000,
            shade: 0,
            palette: 0,
            orientation: 16,
            clip: [0, 0, 319, 199],
            unique_id: 0,
        });
        assert_eq!(frame.pixels[100 * 320 + 160], 37);
        assert_eq!(frame.pixels[0], 0);
    }

    #[test]
    fn minitext_draws_quote_bytes_with_the_requested_palette_path() {
        let mut tiles = SoftTiles::new();
        tiles.insert(MINIFONT + i16::from(b'A' - b'!'), 1, 1, vec![43]);
        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).draw_minitext(&HudTextDraw {
            x: 5,
            y: 7,
            text: "A".to_owned(),
            shade: 0,
            palette: 2,
        });
        assert_eq!(frame.pixels[7 * 320 + 5], 43);
    }

    #[test]
    fn digitalnumber_draws_consecutive_digit_tiles_centered_on_x() {
        let mut tiles = SoftTiles::new();
        tiles.insert(500, 1, 1, vec![30]);
        tiles.insert(501, 1, 1, vec![31]);
        tiles.insert(502, 1, 1, vec![32]);
        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).draw_digitalnumber(HudNumberDraw {
            tile: 500,
            x: 10,
            y: 7,
            number: 12,
            shade: 0,
            palette: 0,
            orientation: 16,
            clip: [0, 0, 319, 199],
            zoom: 65536,
        });

        assert_eq!(frame.pixels[7 * 320 + 9], 31);
        assert_eq!(frame.pixels[7 * 320 + 11], 32);
        assert_eq!(frame.pixels[7 * 320 + 10], 0);
    }

    #[test]
    fn gametext_uses_ascii_font_tiles_and_legacy_half_x() {
        let mut tiles = SoftTiles::new();
        tiles.insert(1032, 1, 1, vec![55]);
        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).draw_gametext(&HudGameTextDraw {
            tile: 1000,
            x: 20,
            y: 10,
            text: "A".to_owned(),
            shade: 0,
            palette: 0,
            orientation: 16,
            clip: [0, 0, 319, 199],
            zoom: 65536,
        });

        assert_eq!(frame.pixels[10 * 320 + 10], 55);
    }
}
