//! Full-screen 2D pages drawn the way Chocolate Duke draws them: `rotatesprite`
//! in the 320×200 plane plus the three ART fonts (`menutext` BIGALPHANUM,
//! `gametext` STARTALPHANUM, `minitext` MINIFONT). Used by the bonus
//! screen and other front-end pages that are not part of the 3D view.

use room_build_engine::soft::{SoftFrame, SoftTiles};
use room_build_engine::world::{
    HudGameTextDraw, HudNumberDraw, HudScreenTextDraw, HudSpriteDraw, HudTextDraw,
};

const BIGALPHANUM: i16 = 2940;
/// `rotatesprite` orientation bits for translucency and its reversed table.
const HUD_TRANSLUCENT: i32 = 1;
const HUD_REVERSE_TRANSLUCENT: i32 = 32;
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
        if let Some(colors) = self.frame.true_color.as_mut() {
            colors.fill(0);
        }
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
                let index = (dy * fw + dx) as usize;
                self.frame.pixels[index] = p;
                self.frame
                    .set_true_color(index, self.tiles.base_true_color_at(pic, pal, shade, u, v));
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
                let color = self
                    .tiles
                    .base_true_color_at(pic, palette, shade, u as u32, v as u32);
                let index = (screen_y * fw + screen_x) as usize;
                let background = self.frame.pixels[index];
                self.frame.pixels[index] = if no_mask {
                    pixel
                } else {
                    self.tiles
                        .composite_hud_pixel(background, pixel, draw.orientation)
                };
                // `rotatesprite` translucency (orientation 1, reversed with 32)
                // blends in true colour too; opaque texels replace the pixel.
                if !no_mask && draw.orientation & HUD_TRANSLUCENT != 0 {
                    self.frame.blend_true_color_pixel(
                        self.tiles,
                        index,
                        background,
                        pixel,
                        color,
                        draw.orientation & HUD_REVERSE_TRANSLUCENT != 0,
                    );
                } else {
                    self.frame.set_true_color(index, color);
                }
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
        // `Menu_Init`: the minifont is uppercased unless all lowercase glyph
        // tiles (`a`..`z` after the ASCII-order block) are present.
        let lowercase = (b'a'..=b'z').all(|byte| {
            self.tiles
                .dimensions(MINIFONT + i16::from(byte - b'!'))
                .is_some()
        });
        let yline = self.font_line_height(MINIFONT);
        self.draw_screentext(&minitext_as_screentext(draw, yline, lowercase));
    }

    /// Draw EDuke32 `gametext` / `gametextz` using the supplied ASCII-order
    /// font tiles, legacy x coordinate, quote palette escapes and clip window.
    pub fn draw_gametext(&mut self, draw: &HudGameTextDraw) {
        let yline = self.font_line_height(STARTALPHANUM);
        self.draw_screentext(&gametext_as_screentext(draw, yline));
    }

    /// EDuke32 sets a menu font's empty-character height to its first glyph
    /// tile's height (`Menu_Init`), falling back to the static metric.
    fn font_line_height(&self, first_tile: i16) -> i32 {
        self.tiles
            .dimensions(first_tile)
            .map_or(0, |(_, height)| i32::from(height))
    }

    /// Draw EDuke32 `screentext` with the common alignment, ASCII/digit tile,
    /// palette escape, spacing, rotation, alpha and clipping options.
    pub fn draw_screentext(&mut self, draw: &HudScreenTextDraw) {
        const FULL16: i32 = 2048;
        const TOPLEFT: i32 = 16;
        const TEXT_XRIGHT: i32 = 1;
        const TEXT_XCENTER: i32 = 2;
        const TEXT_YBOTTOM: i32 = 4;
        const TEXT_YCENTER: i32 = 8;
        const TEXT_TILESPACE: i32 = 32;
        const TEXT_INTERNALLINE: i32 = 64;
        const TEXT_TILELINE: i32 = 128;
        const TEXT_XOFFSETZERO: i32 = 256;
        const TEXT_XJUSTIFY: i32 = 512;
        const TEXT_YOFFSETZERO: i32 = 1024;
        const TEXT_YJUSTIFY: i32 = 2048;
        const TEXT_INTERNALSPACE: i32 = 16;
        const TEXT_CONSTWIDTHNUMS: i32 = 1 << 18;
        const TEXT_UPPERCASE: i32 = 1 << 13;
        const TEXT_INVERTCASE: i32 = 1 << 14;
        const TEXT_IGNOREESCAPE: i32 = 1 << 15;
        const TEXT_LITERALESCAPE: i32 = 1 << 16;
        const TEXT_BACKWARDS: i32 = 1 << 17;
        const TEXT_DIGITALNUMBER: i32 = 1 << 19;
        const TEXT_BIGALPHANUM: i32 = 1 << 20;
        const TEXT_GRAYFONT: i32 = 1 << 21;
        const TEXT_VARHEIGHT: i32 = 1 << 23;
        const TEXT_CENTERCONSTWIDTH: i32 = 1 << 24;
        if draw.zoom == 0 || draw.alpha >= 255 {
            return;
        }
        let Ok(first_tile) = i16::try_from(draw.tile) else {
            return;
        };
        let zoom = f64::from(draw.zoom.unsigned_abs()) / 65536.0;
        if !zoom.is_finite() || zoom > 4096.0 {
            return;
        }

        #[derive(Clone, Copy)]
        struct Glyph {
            tile: Option<i16>,
            width: f64,
            height: f64,
            palette: u8,
            space: bool,
        }

        let flags = draw.text_flags;
        let mut palette = draw.palette.clamp(0, i32::from(u8::MAX)) as u8;
        let mut lines = vec![Vec::<Glyph>::new()];
        let mut bytes = draw.text.bytes().peekable();
        while let Some(mut byte) = bytes.next() {
            if byte == b'^'
                && flags & (TEXT_IGNOREESCAPE | TEXT_LITERALESCAPE) == 0
                && bytes.peek().is_some_and(u8::is_ascii_digit)
            {
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
            if byte == b'\r' {
                continue;
            }
            if byte == b'\n' {
                lines.push(Vec::new());
                continue;
            }
            if flags & (TEXT_UPPERCASE | TEXT_INVERTCASE) == (TEXT_UPPERCASE | TEXT_INVERTCASE) {
                byte = byte.to_ascii_lowercase();
            } else if flags & TEXT_UPPERCASE != 0 {
                byte = byte.to_ascii_uppercase();
            } else if flags & TEXT_INVERTCASE != 0 && byte.is_ascii_alphabetic() {
                byte = if byte.is_ascii_lowercase() {
                    byte.to_ascii_uppercase()
                } else {
                    byte.to_ascii_lowercase()
                };
            }
            if byte == b' ' {
                if let Some(line) = lines.last_mut() {
                    line.push(Glyph {
                        tile: None,
                        width: f64::from(draw.xspace),
                        height: 0.0,
                        palette,
                        space: true,
                    });
                }
                continue;
            }

            let is_digit = byte.is_ascii_digit();
            let tile = if flags & TEXT_BIGALPHANUM != 0 {
                let relative_tile = match byte {
                    b'0'..=b'9' => i16::from(byte - b'0'),
                    b'A'..=b'Z' => 10 + i16::from(byte - b'A'),
                    b'a'..=b'z' if flags & TEXT_UPPERCASE != 0 => 10 + i16::from(byte - b'a'),
                    b'a'..=b'z' => 36 + i16::from(byte - b'a'),
                    b'-' => -1,
                    b'.' => 72,
                    b',' => 73,
                    b'!' => 74,
                    b'?' => 75,
                    b';' => 76,
                    b':' => 77,
                    b'\'' => 92,
                    _ => -2,
                };
                if relative_tile < 0 && relative_tile != -1 {
                    None
                } else {
                    first_tile.checked_add(relative_tile)
                }
            } else if flags & TEXT_GRAYFONT != 0 {
                let relative_tile = match byte {
                    b'0'..=b'9' => -10 + i16::from(byte - b'0'),
                    b'A'..=b'Z' => i16::from(byte - b'A'),
                    b'a'..=b'z' if flags & TEXT_UPPERCASE != 0 => i16::from(byte - b'a'),
                    b'a'..=b'z' => -26 + i16::from(byte - b'a'),
                    b'-' => -37,
                    b'.' => 36,
                    b',' => 37,
                    b'!' => 38,
                    b'?' => 39,
                    b';' => 40,
                    b':' => 41,
                    b'\'' => 56,
                    _ => i16::MIN,
                };
                (relative_tile != i16::MIN)
                    .then(|| first_tile.checked_add(relative_tile))
                    .flatten()
            } else if flags & TEXT_DIGITALNUMBER != 0 {
                is_digit
                    .then(|| first_tile.checked_add(i16::from(byte - b'0')))
                    .flatten()
            } else if (b'!'..=b'~').contains(&byte) {
                first_tile.checked_add(i16::from(byte - b'!'))
            } else {
                None
            };
            let (mut width, height) = tile
                .and_then(|tile| self.tiles.dimensions(tile))
                .map_or((0.0, 0.0), |(width, height)| {
                    (f64::from(width), f64::from(height))
                });
            if is_digit && flags & TEXT_CONSTWIDTHNUMS != 0 {
                let zero_tile = if flags & (TEXT_DIGITALNUMBER | TEXT_BIGALPHANUM) != 0 {
                    Some(first_tile)
                } else if flags & TEXT_GRAYFONT != 0 {
                    first_tile.checked_sub(10)
                } else {
                    first_tile.checked_add(i16::from(b'0' - b'!'))
                };
                width = zero_tile
                    .and_then(|tile| self.tiles.dimensions(tile))
                    .map_or(width, |(width, _)| f64::from(width));
            }
            if flags & TEXT_TILESPACE != 0 && byte == b'~' {
                // EDuke32 uses the character following '~' as the reference
                // tile when selecting a custom font's space width. The marker
                // itself is not drawn.
                let reference_width = bytes.peek().and_then(|next| {
                    let next_tile =
                        first_tile.checked_add(i16::from(*next).saturating_sub(i16::from(b'!')))?;
                    self.tiles
                        .dimensions(next_tile)
                        .map(|(width, _)| f64::from(width))
                });
                if let Some(line) = lines.last_mut() {
                    line.push(Glyph {
                        tile: None,
                        width: reference_width.unwrap_or(width) + f64::from(draw.xspace),
                        height: 0.0,
                        palette,
                        space: true,
                    });
                }
                continue;
            }
            if let Some(line) = lines.last_mut() {
                line.push(Glyph {
                    tile,
                    width,
                    height,
                    palette,
                    space: false,
                });
            }
        }

        if flags & TEXT_BACKWARDS != 0 {
            for line in &mut lines {
                line.reverse();
            }
        }
        let internal_space = if flags & TEXT_INTERNALSPACE != 0 {
            lines
                .iter()
                .flatten()
                .find(|glyph| glyph.tile.is_some())
                .map_or(0.0, |glyph| glyph.width / 2.0)
        } else {
            0.0
        };
        for glyph in lines.iter_mut().flatten().filter(|glyph| glyph.space) {
            glyph.width += internal_space;
        }
        let measure_line = |line: &[Glyph]| -> f64 {
            if line.is_empty() {
                return 0.0;
            }
            if flags & TEXT_XJUSTIFY != 0 {
                return f64::from(draw.xbetween);
            }
            if flags & TEXT_XOFFSETZERO != 0 {
                line.len() as f64 * f64::from(draw.xbetween)
            } else {
                line.iter()
                    .map(|glyph| glyph.width + f64::from(draw.xbetween))
                    .sum::<f64>()
                    - f64::from(draw.xbetween)
            }
        };
        let line_widths: Vec<f64> = lines.iter().map(|line| measure_line(line) * zoom).collect();
        let tile_line_height = lines
            .iter()
            .flat_map(|line| line.iter().map(|glyph| glyph.height))
            .fold(0.0_f64, f64::max);
        let internal_line = if flags & TEXT_INTERNALLINE != 0 {
            tile_line_height
        } else {
            0.0
        };
        let tile_line = if flags & TEXT_TILELINE != 0 {
            tile_line_height
        } else {
            0.0
        };
        let line_height = f64::from(draw.yline) + internal_line + tile_line;
        let natural_line_step = (line_height + f64::from(draw.ybetween)) * zoom;
        let measured_height = lines
            .iter()
            .flat_map(|line| line.iter().map(|glyph| glyph.height))
            .fold(line_height, f64::max);
        let text_height = if lines.is_empty() {
            0.0
        } else {
            (lines.len().saturating_sub(1) as f64) * natural_line_step + measured_height * zoom
        };
        let line_step = if flags & TEXT_YJUSTIFY != 0 && lines.len() > 1 {
            ((f64::from(draw.ybetween) - measured_height) / (lines.len() - 1) as f64) * zoom
        } else if flags & TEXT_YOFFSETZERO != 0 {
            f64::from(draw.ybetween) * zoom
        } else {
            natural_line_step
        };
        let text_height = if flags & TEXT_YJUSTIFY != 0 && lines.len() > 1 {
            f64::from(draw.ybetween) * zoom
        } else {
            text_height
        };

        let full16 = draw.orientation & FULL16 != 0;
        let coordinate_scale = if full16 { 65536.0 } else { 1.0 };
        let anchor_x = if full16 {
            f64::from(draw.x) / 65536.0
        } else {
            f64::from(draw.x)
        };
        let anchor_y = if full16 {
            f64::from(draw.y) / 65536.0
        } else {
            f64::from(draw.y)
        };
        let vertical_offset = if flags & TEXT_YBOTTOM != 0 {
            -text_height
        } else if flags & TEXT_YCENTER != 0 {
            -text_height / 2.0
        } else {
            0.0
        };
        let block_angle =
            f64::from(draw.block_angle.rem_euclid(2048)) * std::f64::consts::TAU / 2048.0;
        let (block_sin, block_cos) = block_angle.sin_cos();
        // EDuke32 classic `dorotatesprite`: alpha only applies to a draw that
        // is not already translucent; above 84 it selects the translucency
        // table and above 168 the more transparent one (255 is skipped above).
        let mut orientation = draw.orientation;
        let alpha = draw.alpha.clamp(0, 255);
        if orientation & 1 == 0 && alpha > 84 {
            orientation |= 1;
            if alpha > 168 {
                orientation |= 32;
            } else {
                orientation &= !32;
            }
        }
        let character_angle = draw.block_angle.wrapping_add(draw.char_angle);
        let shade = draw.shade;
        for (line_index, line) in lines.iter().enumerate() {
            let line_width = line_widths.get(line_index).copied().unwrap_or(0.0);
            let horizontal_offset = if flags & TEXT_XRIGHT != 0 {
                -line_width
            } else if flags & TEXT_XCENTER != 0 {
                -line_width / 2.0
            } else {
                0.0
            };
            let glyph_count = line.len();
            let content_width = line.iter().map(|glyph| glyph.width).sum::<f64>();
            let justify_step = if flags & TEXT_XJUSTIFY != 0 && glyph_count > 1 {
                (f64::from(draw.xbetween) - content_width) / (glyph_count - 1) as f64
            } else {
                0.0
            };
            let constant_advance = f64::from(draw.xbetween);
            let mut cursor = horizontal_offset;
            for (glyph_index, glyph) in line.iter().enumerate() {
                if let Some(tile) = glyph.tile {
                    let x = cursor
                        + if flags & TEXT_CENTERCONSTWIDTH != 0 && flags & TEXT_XOFFSETZERO != 0 {
                            (constant_advance - glyph.width) / 2.0
                        } else {
                            0.0
                        };
                    let y = vertical_offset + line_index as f64 * line_step;
                    let (local_x, local_y) = if draw.orientation & TOPLEFT != 0 {
                        (x, y)
                    } else {
                        (x + glyph.width * zoom / 2.0, y + glyph.height * zoom / 2.0)
                    };
                    let rotated_x = anchor_x + local_x * block_cos - local_y * block_sin;
                    let rotated_y = anchor_y + local_x * block_sin + local_y * block_cos;
                    let var_height_offset =
                        if flags & TEXT_VARHEIGHT != 0 && draw.orientation & TOPLEFT == 0 {
                            glyph.height * zoom / 2.0
                        } else {
                            0.0
                        };
                    let rotated_y = rotated_y + var_height_offset;
                    let to_coord = |value: f64| {
                        (value * coordinate_scale)
                            .round()
                            .clamp(f64::from(i32::MIN), f64::from(i32::MAX))
                            as i32
                    };
                    self.rotatesprite(HudSpriteDraw {
                        x: to_coord(rotated_x),
                        y: to_coord(rotated_y),
                        zoom: draw.zoom,
                        angle: character_angle,
                        tile: i32::from(tile),
                        shade,
                        palette: i32::from(glyph.palette),
                        orientation,
                        clip: draw.clip,
                        unique_id: 0,
                    });
                }
                let advance = if flags & TEXT_XOFFSETZERO != 0 {
                    constant_advance
                } else if flags & TEXT_XJUSTIFY != 0 {
                    glyph.width
                } else {
                    glyph.width + f64::from(draw.xbetween)
                };
                cursor += if flags & TEXT_XOFFSETZERO != 0 {
                    advance * zoom
                } else if glyph_index + 1 < glyph_count {
                    (advance + justify_step) * zoom
                } else {
                    advance * zoom
                };
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
    fn gametext_breaks_lines_like_g_screentext() {
        let mut tiles = SoftTiles::new();
        tiles.insert(1032, 1, 1, vec![55]);
        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).draw_gametext(&HudGameTextDraw {
            tile: 1000,
            x: 20,
            y: 10,
            text: "A\nA".to_owned(),
            shade: 0,
            palette: 0,
            orientation: 16,
            clip: [0, 0, 319, 199],
            zoom: 65536,
        });
        // Without a loaded STARTALPHANUM tile the blue font's 7-pixel line holds.
        assert_eq!(frame.pixels[10 * 320 + 10], 55);
        assert_eq!(frame.pixels[17 * 320 + 10], 55);
    }

    #[test]
    fn minitext_uppercases_unless_the_lowercase_minifont_is_loaded() {
        let upper = MINIFONT + i16::from(b'A' - b'!');
        let draw = HudTextDraw {
            x: 5,
            y: 7,
            text: "a".to_owned(),
            shade: 0,
            palette: 0,
        };
        let mut tiles = SoftTiles::new();
        tiles.insert(upper, 1, 1, vec![43]);
        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).draw_minitext(&draw);
        assert_eq!(frame.pixels[7 * 320 + 5], 43);

        for byte in b'a'..=b'z' {
            tiles.insert(MINIFONT + i16::from(byte - b'!'), 1, 1, vec![44]);
        }
        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).draw_minitext(&draw);
        assert_eq!(frame.pixels[7 * 320 + 5], 44);
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

    #[test]
    fn screentext_uppercases_and_centers_ascii_font_tiles() {
        let mut tiles = SoftTiles::new();
        tiles.insert(1032, 1, 1, vec![61]);
        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).draw_screentext(&HudScreenTextDraw {
            tile: 1000,
            x: 160,
            y: 10,
            zoom: 65536,
            block_angle: 0,
            char_angle: 0,
            text: "a".to_owned(),
            shade: 0,
            palette: 0,
            orientation: 16,
            alpha: 0,
            xspace: 4,
            yline: 8,
            xbetween: 0,
            ybetween: 0,
            text_flags: 2 | (1 << 13),
            clip: [0, 0, 319, 199],
        });

        assert_eq!(frame.pixels[10 * 320 + 160], 61);
    }

    #[test]
    fn screentext_big_and_gray_font_modes_follow_case_and_tile_order() {
        let mut tiles = SoftTiles::new();
        tiles.insert(2940, 1, 1, vec![71]);
        tiles.insert(2966, 1, 1, vec![72]);
        let draw = |tile, flags| HudScreenTextDraw {
            tile,
            x: 0,
            y: 0,
            zoom: 65536,
            block_angle: 0,
            char_angle: 0,
            text: "Aa".to_owned(),
            shade: 0,
            palette: 0,
            orientation: 16,
            alpha: 0,
            xspace: 4,
            yline: 8,
            xbetween: 0,
            ybetween: 0,
            text_flags: flags,
            clip: [0, 0, 319, 199],
        };

        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).draw_screentext(&draw(2930, 1 << 20));
        assert_eq!(frame.pixels[0], 71); // uppercase uses red BIGALPHANUM
        assert_eq!(frame.pixels[1], 72); // lowercase uses the gray alphabet

        tiles.insert(2966, 1, 1, vec![81]);
        tiles.insert(2940, 1, 1, vec![82]);
        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).draw_screentext(&draw(2966, 1 << 21));
        assert_eq!(frame.pixels[0], 81); // uppercase uses the gray alphabet
        assert_eq!(frame.pixels[1], 82); // lowercase uses red letter tiles
    }

    #[test]
    fn screentext_centers_glyphs_in_constant_width_slots() {
        let mut tiles = SoftTiles::new();
        tiles.insert(1032, 2, 1, vec![91, 92]);
        tiles.insert(1033, 1, 1, vec![93]);
        let mut frame = SoftFrame::new(320, 200);
        UiPage::new(&tiles, &mut frame).draw_screentext(&HudScreenTextDraw {
            tile: 1000,
            x: 0,
            y: 0,
            zoom: 65536,
            block_angle: 0,
            char_angle: 0,
            text: "AB".to_owned(),
            shade: 0,
            palette: 0,
            orientation: 16,
            alpha: 0,
            xspace: 0,
            yline: 8,
            xbetween: 4,
            ybetween: 0,
            text_flags: (1 << 8) | (1 << 24),
            clip: [0, 0, 319, 199],
        });

        assert_eq!(frame.pixels[1], 91);
        assert_eq!(frame.pixels[2], 92);
        assert_eq!(frame.pixels[6], 93);
    }
}

/// EDuke32 CON `gametext` is `G_PrintGameText`: `G_ScreenText` at half the
/// CON x with the blue font's metrics (5×7 empty character, no extra
/// spacing), constant-width digits, line breaks and `^` palette escapes,
/// centered when that x lands on 160, drawn with orientation `2 | o`.
fn gametext_as_screentext(draw: &HudGameTextDraw, yline: i32) -> HudScreenTextDraw {
    const FULL16: i32 = 2048;
    const TEXT_XCENTER: i32 = 2;
    const TEXT_CONSTWIDTHNUMS: i32 = 1 << 18;
    const BLUEFONT_EMPTY_WIDTH: i32 = 5;
    const BLUEFONT_EMPTY_HEIGHT: i32 = 7;
    let x = draw.x >> 1;
    let center = if draw.orientation & FULL16 != 0 {
        x == 160 << 16
    } else {
        x == 160
    };
    HudScreenTextDraw {
        tile: draw.tile,
        x,
        y: draw.y,
        zoom: draw.zoom,
        block_angle: 0,
        char_angle: 0,
        text: draw.text.clone(),
        shade: draw.shade,
        palette: draw.palette,
        orientation: draw.orientation | 2,
        alpha: 0,
        xspace: BLUEFONT_EMPTY_WIDTH,
        yline: if yline > 0 {
            yline
        } else {
            BLUEFONT_EMPTY_HEIGHT
        },
        xbetween: 0,
        ybetween: 0,
        text_flags: TEXT_CONSTWIDTHNUMS | if center { TEXT_XCENTER } else { 0 },
        clip: draw.clip,
    }
}

/// EDuke32 CON `minitext` is `minitext_`: `G_ScreenText` with the minifont
/// (4×5 empty character, one pixel between glyphs and lines), drawn with
/// orientation `2 | 8 | 16` at 320×200 coordinates over the whole screen.
fn minitext_as_screentext(draw: &HudTextDraw, yline: i32, lowercase: bool) -> HudScreenTextDraw {
    const TEXT_UPPERCASE: i32 = 1 << 13;
    const MINIFONT_EMPTY_WIDTH: i32 = 4;
    const MINIFONT_EMPTY_HEIGHT: i32 = 5;
    HudScreenTextDraw {
        tile: i32::from(MINIFONT),
        x: draw.x,
        y: draw.y,
        zoom: 65_536,
        block_angle: 0,
        char_angle: 0,
        text: draw.text.clone(),
        shade: draw.shade,
        palette: draw.palette,
        orientation: 2 | 8 | 16,
        alpha: 0,
        xspace: MINIFONT_EMPTY_WIDTH,
        yline: if yline > 0 {
            yline
        } else {
            MINIFONT_EMPTY_HEIGHT
        },
        xbetween: 1,
        ybetween: 1,
        text_flags: if lowercase { 0 } else { TEXT_UPPERCASE },
        clip: [0, 0, 319, 199],
    }
}
