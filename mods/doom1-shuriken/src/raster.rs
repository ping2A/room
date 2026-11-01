//! Integer polygon rasteriser (`docs/32` §3.3).

#![deny(clippy::float_arithmetic)]

use crate::fixed;
use crate::palette::{self, Palette};
use alloc::vec;
use alloc::vec::Vec;

/// One vertex in 1/16-pixel fixed grid (not Q16.16 — use `<< 12` from pixels).
pub type Grid = i32;

/// Pixel buffer: palette index + mask (0 = transparent).
pub struct Canvas {
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
    /// Row-major indices.
    pub indices: Vec<u8>,
    /// Row-major opacity.
    pub mask: Vec<u8>,
}

impl Canvas {
    /// Transparent canvas.
    #[must_use]
    pub fn new(w: u32, h: u32) -> Self {
        let n = (w as usize).saturating_mul(h as usize);
        Self {
            w,
            h,
            indices: vec![0; n],
            mask: vec![0; n],
        }
    }

    fn put(&mut self, x: i32, y: i32, idx: u8) {
        if x < 0 || y < 0 || x as u32 >= self.w || y as u32 >= self.h {
            return;
        }
        let i = y as usize * self.w as usize + x as usize;
        self.indices[i] = idx;
        self.mask[i] = 255;
    }

    /// Even-odd fill of a polygon in pixel space (vertices in pixels ×16).
    pub fn fill_poly(&mut self, verts: &[(Grid, Grid)], colour: u8) {
        if verts.len() < 3 {
            return;
        }
        let (mut min_y, mut max_y) = (i32::MAX, i32::MIN);
        for &(_, y) in verts {
            let py = y >> 4;
            min_y = min_y.min(py);
            max_y = max_y.max(py);
        }
        min_y = min_y.max(0);
        max_y = max_y.min(self.h as i32 - 1);
        for y in min_y..=max_y {
            let scan = (y << 4) + 8; // pixel centre
            let mut xs: [i32; 32] = [0; 32];
            let mut n = 0usize;
            for i in 0..verts.len() {
                let (x0, y0) = verts[i];
                let (x1, y1) = verts[(i + 1) % verts.len()];
                if (y0 > scan) == (y1 > scan) {
                    continue;
                }
                let dy = y1 - y0;
                if dy == 0 {
                    continue;
                }
                let x = x0 + (x1 - x0) * (scan - y0) / dy;
                if n < xs.len() {
                    xs[n] = x >> 4;
                    n += 1;
                }
            }
            xs[..n].sort_unstable();
            let mut i = 0;
            while i + 1 < n {
                let a = xs[i];
                let b = xs[i + 1];
                for x in a..=b {
                    self.put(x, y, colour);
                }
                i += 2;
            }
        }
    }

    /// 1-px dark outline around opaque pixels.
    pub fn outline(&mut self, dark: u8) {
        let w = self.w as i32;
        let h = self.h as i32;
        let mut border = vec![false; self.mask.len()];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if self.mask[i] == 0 {
                    continue;
                }
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        border[i] = true;
                        break;
                    }
                    let ni = (ny * w + nx) as usize;
                    if self.mask[ni] == 0 {
                        border[i] = true;
                        break;
                    }
                }
            }
        }
        for (i, &b) in border.iter().enumerate() {
            if b {
                self.indices[i] = dark;
            }
        }
    }

    /// Brighten edge pixels one step (highlight).
    pub fn highlight_edge(&mut self, pal: &Palette) {
        let w = self.w as i32;
        let h = self.h as i32;
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if self.mask[i] == 0 {
                    continue;
                }
                let mut edge = false;
                for (dx, dy) in [(-1, -1), (1, -1), (-1, 1), (1, 1)] {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        edge = true;
                        break;
                    }
                    if self.mask[(ny * w + nx) as usize] == 0 {
                        edge = true;
                        break;
                    }
                }
                if edge {
                    let (r, g, b) = palette::steel_rgb(7);
                    self.indices[i] = pal.index(r, g, b);
                }
            }
        }
    }
}

/// 4-point star polygon in a `size`×`size` pixel box, rotated by table angle.
#[must_use]
pub fn star_verts(size: u32, ang: u8) -> [(Grid, Grid); 8] {
    let cx = ((size as i32) << 4) / 2;
    let cy = cx;
    let outer = ((size as i32) << 4) * 7 / 16;
    let inner = outer / 3;
    let mut out = [(0i32, 0i32); 8];
    for i in 0..8 {
        // 45° steps on the 256-entry circle (256/8 = 32).
        let a = ang.wrapping_add((i as u8).wrapping_mul(32));
        let r = if i % 2 == 0 { outer } else { inner };
        let s = fixed::sin(a);
        let c = fixed::cos(a);
        let gx = cx + ((c as i64 * i64::from(r)) >> 16) as i32;
        let gy = cy + ((s as i64 * i64::from(r)) >> 16) as i32;
        out[i] = (gx, gy);
    }
    out
}

/// Paint a steel star into a new canvas.
#[must_use]
pub fn paint_star(pal: &Palette, size: u32, ang: u8, level: u8) -> Canvas {
    let mut c = Canvas::new(size, size);
    let (r, g, b) = palette::steel_rgb(level);
    let idx = pal.index(r, g, b);
    let dark = pal.index(24, 28, 32);
    let verts = star_verts(size, ang);
    c.fill_poly(&verts, idx);
    c.outline(dark);
    c.highlight_edge(pal);
    c
}

/// Convert canvas → Doom patch bytes via SDK encoder.
pub fn to_patch(c: &Canvas, left: i16, top: i16) -> Result<Vec<u8>, &'static str> {
    room_script_sdk::art::encode_patch(
        c.w as u16,
        c.h as u16,
        left,
        top,
        &c.indices,
        &c.mask,
    )
}
