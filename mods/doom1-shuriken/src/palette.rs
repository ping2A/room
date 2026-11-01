//! PLAYPAL → nearest-colour LUT (`docs/32` §3.3).

#![deny(clippy::float_arithmetic)]

use alloc::vec::Vec;

/// RGB triplet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

/// Palette + 32³ nearest-index LUT.
pub struct Palette {
    /// 256 colours from PLAYPAL slot 0.
    #[allow(dead_code)]
    pub colours: [Rgb; 256],
    /// LUT: index = `(r>>3)<<10 | (g>>3)<<5 | (b>>3)`.
    lut: [u8; 32 * 32 * 32],
}

impl Palette {
    /// Build from raw PLAYPAL bytes (≥ 768). Index 247 is never chosen as a
    /// solid colour (often a cyan “hole” in Doom tools).
    #[must_use]
    pub fn from_playpal(bytes: &[u8]) -> Self {
        let mut colours = [Rgb { r: 0, g: 0, b: 0 }; 256];
        let n = (bytes.len() / 3).min(256);
        for i in 0..n {
            colours[i] = Rgb {
                r: bytes[i * 3],
                g: bytes[i * 3 + 1],
                b: bytes[i * 3 + 2],
            };
        }
        let mut lut = [0u8; 32 * 32 * 32];
        for ri in 0..32u32 {
            for gi in 0..32u32 {
                for bi in 0..32u32 {
                    let r = (ri * 255 / 31) as u8;
                    let g = (gi * 255 / 31) as u8;
                    let b = (bi * 255 / 31) as u8;
                    lut[((ri << 10) | (gi << 5) | bi) as usize] = nearest(&colours, r, g, b);
                }
            }
        }
        Self { colours, lut }
    }

    /// Fallback steel/grey ramp when PLAYPAL is unavailable (native tests).
    #[must_use]
    pub fn fallback() -> Self {
        let mut bytes = Vec::with_capacity(768);
        for i in 0..256u16 {
            let v = (i * 255 / 255) as u8;
            // Bias mid greys toward cool steel.
            let r = v.saturating_sub(8);
            let g = v.saturating_sub(4);
            let b = v;
            bytes.push(r);
            bytes.push(g);
            bytes.push(b);
        }
        Self::from_playpal(&bytes)
    }

    /// Nearest palette index for an 8-bit RGB triple.
    #[must_use]
    pub fn index(&self, r: u8, g: u8, b: u8) -> u8 {
        self.lut[(((r as usize) >> 3) << 10) | (((g as usize) >> 3) << 5) | ((b as usize) >> 3)]
    }
}

fn nearest(colours: &[Rgb; 256], r: u8, g: u8, b: u8) -> u8 {
    let mut best = 0u8;
    let mut best_d = u32::MAX;
    for (i, c) in colours.iter().enumerate() {
        if i == 247 {
            continue;
        }
        let dr = i32::from(c.r) - i32::from(r);
        let dg = i32::from(c.g) - i32::from(g);
        let db = i32::from(c.b) - i32::from(b);
        let d = (dr * dr + dg * dg + db * db) as u32;
        if d < best_d {
            best_d = d;
            best = i as u8;
        }
    }
    best
}

/// Steel ramp: 0 dark → 7 bright edge.
#[must_use]
pub fn steel_rgb(level: u8) -> (u8, u8, u8) {
    const RAMP: [(u8, u8, u8); 8] = [
        (40, 44, 52),
        (60, 66, 78),
        (88, 96, 110),
        (120, 128, 142),
        (150, 158, 170),
        (180, 188, 198),
        (210, 216, 224),
        (235, 240, 245),
    ];
    RAMP[usize::from(level.min(7))]
}
