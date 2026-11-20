//! Texture upscaling at upload (UZDoom `gl_texture_hqresize*`).
//!
//! Upscaled pixels replace the GPU image only; the texture keeps its logical
//! size, so UVs, offsets and texture scaling are unchanged.
//!
//! Modes (`gl_texture_hqresizemode`): 0 off, 1 Scale2x / 3x / 4x (EPX /
//! AdvMAME), 2 hqNx, 3 xBRZ, 4 old xBRZ, 5 NormalNx (plain pixel copies).
//! Room implements Scale*x and NormalNx; hqNx and both xBRZ modes use
//! Scale*x instead.

/// The `gl_texture_hqresize*` settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HqResize {
    /// `gl_texture_hqresizemode`.
    pub mode: u8,
    /// `gl_texture_hqresizemult` (1–6).
    pub mult: u8,
    /// `gl_texture_hqresize_maxinputsize`: larger images stay as they are.
    pub max_input: u32,
    /// `gl_texture_hqresize_targets`: 1 textures, 2 sprites, 4 fonts, 8 skins.
    pub targets: u8,
}

impl Default for HqResize {
    fn default() -> Self {
        Self {
            mode: 0,
            mult: 2,
            max_input: 512,
            targets: 15,
        }
    }
}

/// Target bit: wall textures and flats.
pub const TARGET_TEXTURES: u8 = 1;
/// Target bit: sprites.
pub const TARGET_SPRITES: u8 = 2;
/// Target bit: fonts.
pub const TARGET_FONTS: u8 = 4;
/// Target bit: model skins.
pub const TARGET_SKINS: u8 = 8;

impl HqResize {
    /// The integer factor this setting scales an image of `w`×`h` by
    /// (1 = left alone).
    #[must_use]
    pub fn factor(&self, w: u32, h: u32, target: u8) -> u32 {
        if self.mode == 0 || self.mult < 2 || self.targets & target == 0 {
            return 1;
        }
        if w > self.max_input || h > self.max_input || w == 0 || h == 0 {
            return 1;
        }
        let cap = if self.mode == 5 { 6 } else { 4 };
        u32::from(self.mult).min(cap)
    }

    /// Upscale `rgba` (`w`×`h`); returns the new size and pixels.
    #[must_use]
    pub fn apply(&self, w: u32, h: u32, rgba: Vec<u8>, target: u8) -> (u32, u32, Vec<u8>) {
        let n = self.factor(w, h, target);
        if n < 2 || rgba.len() < (w * h * 4) as usize {
            return (w, h, rgba);
        }
        if self.mode == 5 {
            return normal_nx(w, h, &rgba, n);
        }
        match n {
            2 => scale2x(w, h, &rgba),
            3 => scale3x(w, h, &rgba),
            _ => {
                let (w2, h2, p) = scale2x(w, h, &rgba);
                scale2x(w2, h2, &p)
            }
        }
    }
}

fn pixels(w: u32, h: u32, rgba: &[u8]) -> Vec<u32> {
    rgba[..(w * h * 4) as usize]
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn bytes(px: &[u32]) -> Vec<u8> {
    px.iter().flat_map(|p| p.to_le_bytes()).collect()
}

/// Nearest-neighbour `n`× copy (NormalNx).
#[must_use]
pub fn normal_nx(w: u32, h: u32, rgba: &[u8], n: u32) -> (u32, u32, Vec<u8>) {
    let src = pixels(w, h, rgba);
    let (ow, oh) = (w * n, h * n);
    let mut out = vec![0u32; (ow * oh) as usize];
    for y in 0..oh {
        for x in 0..ow {
            out[(y * ow + x) as usize] = src[((y / n) * w + x / n) as usize];
        }
    }
    (ow, oh, bytes(&out))
}

/// Neighbours of (x, y), clamped at the edges: A B C / D E F / G H I.
fn around(src: &[u32], w: u32, h: u32, x: u32, y: u32) -> [u32; 9] {
    let at = |dx: i32, dy: i32| {
        let xx = (x as i32 + dx).clamp(0, w as i32 - 1) as u32;
        let yy = (y as i32 + dy).clamp(0, h as i32 - 1) as u32;
        src[(yy * w + xx) as usize]
    };
    [
        at(-1, -1),
        at(0, -1),
        at(1, -1),
        at(-1, 0),
        at(0, 0),
        at(1, 0),
        at(-1, 1),
        at(0, 1),
        at(1, 1),
    ]
}

/// EPX / Scale2x.
#[must_use]
pub fn scale2x(w: u32, h: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let src = pixels(w, h, rgba);
    let ow = w * 2;
    let mut out = vec![0u32; (ow * h * 2) as usize];
    for y in 0..h {
        for x in 0..w {
            let [_, b, _, d, e, f, _, hh, _] = around(&src, w, h, x, y);
            let mut q = [e; 4];
            if b != hh && d != f {
                if d == b {
                    q[0] = d;
                }
                if b == f {
                    q[1] = f;
                }
                if d == hh {
                    q[2] = d;
                }
                if hh == f {
                    q[3] = f;
                }
            }
            let o = (y * 2 * ow + x * 2) as usize;
            out[o] = q[0];
            out[o + 1] = q[1];
            out[o + ow as usize] = q[2];
            out[o + ow as usize + 1] = q[3];
        }
    }
    (ow, h * 2, bytes(&out))
}

/// AdvMAME3x / Scale3x.
#[must_use]
pub fn scale3x(w: u32, h: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let src = pixels(w, h, rgba);
    let ow = w * 3;
    let mut out = vec![0u32; (ow * h * 3) as usize];
    for y in 0..h {
        for x in 0..w {
            let [a, b, c, d, e, f, g, hh, i] = around(&src, w, h, x, y);
            let mut q = [e; 9];
            if b != hh && d != f {
                q[0] = if d == b { d } else { e };
                q[1] = if (d == b && e != c) || (b == f && e != a) {
                    b
                } else {
                    e
                };
                q[2] = if b == f { f } else { e };
                q[3] = if (d == b && e != g) || (d == hh && e != a) {
                    d
                } else {
                    e
                };
                q[5] = if (b == f && e != i) || (hh == f && e != c) {
                    f
                } else {
                    e
                };
                q[6] = if d == hh { d } else { e };
                q[7] = if (d == hh && e != i) || (hh == f && e != g) {
                    hh
                } else {
                    e
                };
                q[8] = if hh == f { f } else { e };
            }
            for (k, v) in q.iter().enumerate() {
                let (kx, ky) = (k as u32 % 3, k as u32 / 3);
                out[((y * 3 + ky) * ow + x * 3 + kx) as usize] = *v;
            }
        }
    }
    (ow, h * 3, bytes(&out))
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: [u8; 4] = [0, 0, 0, 255];
    const W: [u8; 4] = [255, 255, 255, 255];

    fn img(rows: &[&str]) -> (u32, u32, Vec<u8>) {
        let h = rows.len() as u32;
        let w = rows[0].len() as u32;
        let px = rows
            .iter()
            .flat_map(|r| r.chars().flat_map(|c| if c == '#' { K } else { W }))
            .collect();
        (w, h, px)
    }

    fn show(w: u32, px: &[u8]) -> Vec<String> {
        px.chunks_exact(4 * w as usize)
            .map(|row| {
                row.chunks_exact(4)
                    .map(|p| if p[0] == 0 { '#' } else { '.' })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn scale2x_rounds_a_diagonal() {
        let (w, h, px) = img(&["#.", ".#"]);
        let (ow, oh, out) = scale2x(w, h, &px);
        assert_eq!((ow, oh), (4, 4));
        // EPX joins the diagonal into one stroke instead of two blocks.
        assert_eq!(show(ow, &out), ["##..", "#.#.", ".#.#", "..##"]);
    }

    #[test]
    fn flat_areas_and_settings() {
        let (w, h, px) = img(&["##", "##"]);
        let (ow, _, out) = scale3x(w, h, &px);
        assert_eq!(ow, 6);
        assert!(out.chunks_exact(4).all(|p| p == K));
        let s = HqResize {
            mode: 1,
            mult: 4,
            ..HqResize::default()
        };
        assert_eq!(s.apply(w, h, px.clone(), TARGET_TEXTURES).0, 8);
        assert_eq!(s.factor(1024, 8, TARGET_TEXTURES), 1, "over max input size");
        let fonts_only = HqResize {
            targets: TARGET_FONTS,
            ..s
        };
        assert_eq!(fonts_only.factor(8, 8, TARGET_SPRITES), 1);
        let normal = HqResize {
            mode: 5,
            mult: 3,
            ..s
        };
        let (nw, _, np) = normal.apply(w, h, px, TARGET_TEXTURES);
        assert_eq!((nw, np.len()), (6, 6 * 6 * 4));
        assert_eq!(
            HqResize::default().factor(8, 8, TARGET_TEXTURES),
            1,
            "off by default"
        );
    }
}
