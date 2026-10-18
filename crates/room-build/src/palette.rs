//! `PALETTE.DAT`: 256 six-bit RGB triples, `numshades: u16`, then
//! `numshades` 256-byte shade (light) tables, brightest first.

use crate::error::BuildError;

/// Build palette plus shade tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// 8-bit RGB triples.
    pub rgb: [u8; 768],
    /// Shade tables (`shades[s * 256 + index]`).
    pub shades: Vec<u8>,
    /// Number of shade tables.
    pub numshades: usize,
}

impl Palette {
    /// Parse `PALETTE.DAT`.
    pub fn parse(data: &[u8]) -> Result<Self, BuildError> {
        let raw = data
            .get(..770)
            .ok_or(BuildError::Truncated("PALETTE.DAT"))?;
        let mut rgb = [0u8; 768];
        for (o, &v) in rgb.iter_mut().zip(raw) {
            let v = v & 63;
            *o = (v << 2) | (v >> 4);
        }
        let numshades = usize::from(u16::from_le_bytes([raw[768], raw[769]]));
        if numshades == 0 || numshades > 256 {
            return Err(BuildError::Invalid("PALETTE.DAT shade count"));
        }
        let shades = data
            .get(770..770 + numshades * 256)
            .ok_or(BuildError::Truncated("PALETTE.DAT shade tables"))?
            .to_vec();
        Ok(Self {
            rgb,
            shades,
            numshades,
        })
    }

    /// Doom `PLAYPAL`: the palette plus Doom's 13 damage / pickup / suit tints.
    #[must_use]
    pub fn playpal(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(14 * 768);
        out.extend_from_slice(&self.rgb);
        let tint = |out: &mut Vec<u8>, color: [u16; 3], num: u16, den: u16| {
            for (i, &c) in self.rgb.iter().enumerate() {
                let c = u16::from(c);
                let t = color[i % 3];
                #[allow(clippy::cast_possible_truncation)]
                out.push(((c * (den - num) + t * num) / den) as u8);
            }
        };
        for i in 1..=8 {
            tint(&mut out, [255, 0, 0], i, 9);
        }
        for i in 1..=4 {
            tint(&mut out, [215, 186, 69], i, 8);
        }
        tint(&mut out, [0, 256, 0], 1, 8);
        out
    }

    /// Doom `COLORMAP` (34 maps): shade tables 0–31 stretched over the
    /// available Build shades, map 32 grey (invulnerability), map 33 black.
    #[must_use]
    pub fn colormap(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(34 * 256);
        for i in 0..32 {
            let s = (i * self.numshades / 32).min(self.numshades - 1);
            out.extend_from_slice(&self.shades[s * 256..s * 256 + 256]);
        }
        let grey: Vec<u8> = (0..256)
            .map(|i| {
                let lum = u32::from(self.rgb[i * 3]) * 3
                    + u32::from(self.rgb[i * 3 + 1]) * 6
                    + u32::from(self.rgb[i * 3 + 2]);
                self.nearest([255 - lum / 10; 3])
            })
            .collect();
        out.extend_from_slice(&grey);
        let black = self.nearest([0, 0, 0]);
        out.extend(std::iter::repeat_n(black, 256));
        out
    }

    /// Palette index closest to `rgb` (index 255 is Build's transparent colour).
    #[must_use]
    pub fn nearest(&self, rgb: [u32; 3]) -> u8 {
        let mut best = (u32::MAX, 0u8);
        for i in 0..255u8 {
            let p = &self.rgb[usize::from(i) * 3..usize::from(i) * 3 + 3];
            let d: u32 = p
                .iter()
                .zip(rgb)
                .map(|(&a, b)| u32::from(a).abs_diff(b).pow(2))
                .sum();
            if d < best.0 {
                best = (d, i);
            }
        }
        best.1
    }
}

#[cfg(test)]
pub(crate) fn build_palette() -> Vec<u8> {
    let mut out: Vec<u8> = (0..768).map(|i| ((i / 3) % 64) as u8).collect();
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend((0..256).map(|i| i as u8));
    out.extend(std::iter::repeat_n(0u8, 256));
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn expands_six_bit_colours_and_builds_doom_tables() {
        let pal = Palette::parse(&build_palette()).unwrap();
        assert_eq!(pal.numshades, 2);
        assert_eq!(&pal.rgb[3..6], &[4, 4, 4]);
        assert_eq!(&pal.rgb[63 * 3..63 * 3 + 3], &[255, 255, 255]);
        assert_eq!(pal.playpal().len(), 14 * 768);
        let cm = pal.colormap();
        assert_eq!(cm.len(), 34 * 256);
        assert_eq!(cm[5], 5);
        assert_eq!(cm[31 * 256 + 5], 0);
    }

    #[test]
    fn short_palette_is_an_error() {
        assert!(Palette::parse(&[0; 100]).is_err());
        let mut p = build_palette();
        p.truncate(900);
        assert!(Palette::parse(&p).is_err());
    }
}
