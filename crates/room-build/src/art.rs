//! `TILESnnn.ART`: `version`, `numtiles`, `localtilestart`, `localtileend`,
//! then per tile `xsize: i16[]`, `ysize: i16[]`, `picanm: u32[]`, then the
//! column-major palette indices of every tile in order.

use crate::error::BuildError;
use std::collections::BTreeMap;

/// Highest tile number accepted (EDuke32's limit).
pub const MAX_TILES: u32 = 30720;

/// Palette index Build treats as transparent.
pub const TRANSPARENT: u8 = 255;

/// One tile; `pixels` is column-major (`pixels[x * height + y]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile<'a> {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Animation / offset bits (`picanm`).
    pub picanm: u32,
    /// Column-major palette indices.
    pub pixels: &'a [u8],
}

impl Tile<'_> {
    /// Sprite x offset from `picanm` bits 8–15.
    #[must_use]
    pub fn x_offset(&self) -> i8 {
        i8::from_le_bytes([(self.picanm >> 8) as u8])
    }

    /// Sprite y offset from `picanm` bits 16–23.
    #[must_use]
    pub fn y_offset(&self) -> i8 {
        i8::from_le_bytes([(self.picanm >> 16) as u8])
    }

    /// Row-major copy of the pixels.
    #[must_use]
    pub fn row_major(&self) -> Vec<u8> {
        let (w, h) = (usize::from(self.width), usize::from(self.height));
        let mut out = vec![0u8; w * h];
        for x in 0..w {
            for y in 0..h {
                out[y * w + x] = self.pixels[x * h + y];
            }
        }
        out
    }
}

/// Every tile of every loaded ART file, by tile number.
#[derive(Debug, Clone, Default)]
pub struct TileSet<'a> {
    tiles: BTreeMap<u16, Tile<'a>>,
}

impl<'a> TileSet<'a> {
    /// Empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add the tiles of one ART file (later files override earlier tiles).
    pub fn add_art(&mut self, data: &'a [u8]) -> Result<(), BuildError> {
        let rd = |i: usize| -> Result<u32, BuildError> {
            data.get(i * 4..i * 4 + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or(BuildError::Truncated("ART header"))
        };
        if rd(0)? != 1 {
            return Err(BuildError::Invalid("ART version"));
        }
        let (start, end) = (rd(2)?, rd(3)?);
        if end < start || end >= MAX_TILES {
            return Err(BuildError::Invalid("ART tile range"));
        }
        let count = (end - start + 1) as usize;
        let table = 16 + count * 8;
        if data.len() < table {
            return Err(BuildError::Truncated("ART tile table"));
        }
        let i16_at = |p: usize| i16::from_le_bytes([data[p], data[p + 1]]);
        let mut pos = table;
        for i in 0..count {
            let w = i16_at(16 + i * 2);
            let h = i16_at(16 + count * 2 + i * 2);
            let p = 16 + count * 4 + i * 4;
            let picanm = u32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]);
            if w <= 0 || h <= 0 {
                continue;
            }
            let (w, h) = (w.unsigned_abs(), h.unsigned_abs());
            let n = usize::from(w) * usize::from(h);
            let pixels = data
                .get(pos..pos + n)
                .ok_or(BuildError::Truncated("ART tile pixels"))?;
            pos += n;
            #[allow(clippy::cast_possible_truncation)]
            let num = (start as usize + i) as u16;
            self.tiles.insert(
                num,
                Tile {
                    width: w,
                    height: h,
                    picanm,
                    pixels,
                },
            );
        }
        Ok(())
    }

    /// Tile `num` when it has pixels.
    #[must_use]
    pub fn get(&self, num: i32) -> Option<&Tile<'a>> {
        u16::try_from(num).ok().and_then(|n| self.tiles.get(&n))
    }

    /// `picanm` animation of `tile` as `(tile, 35 Hz tics)` frames; empty when
    /// the tile does not animate. Build steps every `2^speed` 120 Hz clocks
    /// through `count + 1` tiles: oscillating, forward, or backward.
    #[must_use]
    pub fn animation(&self, tile: i32) -> Vec<(i32, i32)> {
        let Some(t) = self.get(tile) else {
            return Vec::new();
        };
        let count = (t.picanm & 63) as i32;
        let speed = (t.picanm >> 24) & 15;
        let tics = (((1i32 << speed) * 35 + 60) / 120).max(1);
        let steps: Vec<i32> = match (t.picanm >> 6) & 3 {
            _ if count == 0 => Vec::new(),
            1 => (0..=count).chain((1..count).rev()).collect(),
            2 => (0..=count).collect(),
            3 => (0..=count).map(|i| -i).collect(),
            _ => Vec::new(),
        };
        steps
            .into_iter()
            .map(|s| tile + s)
            .filter(|&f| self.get(f).is_some())
            .map(|f| (f, tics))
            .collect()
    }

    /// Number of tiles with pixels.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// `true` when no tile has pixels.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}

#[cfg(test)]
pub(crate) fn build_art(start: u32, tiles: &[(i16, i16, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [1, 0, start, start + tiles.len() as u32 - 1] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for t in tiles {
        out.extend_from_slice(&t.0.to_le_bytes());
    }
    for t in tiles {
        out.extend_from_slice(&t.1.to_le_bytes());
    }
    for _ in tiles {
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    for t in tiles {
        out.extend_from_slice(t.2);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn reads_tiles_and_skips_empty_ones() {
        let art = build_art(
            10,
            &[(2, 3, &[1, 2, 3, 4, 5, 6]), (0, 0, &[]), (1, 1, &[9])],
        );
        let mut set = TileSet::new();
        set.add_art(&art).unwrap();
        assert_eq!(set.len(), 2);
        let t = set.get(10).unwrap();
        assert_eq!((t.width, t.height), (2, 3));
        assert_eq!(t.row_major(), [1, 4, 2, 5, 3, 6]);
        assert!(set.get(11).is_none());
        assert_eq!(set.get(12).unwrap().pixels, [9]);
    }

    #[test]
    fn corrupt_art_is_an_error() {
        let mut art = build_art(0, &[(4, 4, &[0; 16])]);
        art.truncate(art.len() - 1);
        assert!(TileSet::new().add_art(&art).is_err());
        assert!(TileSet::new().add_art(&[1, 0, 0]).is_err());
        let mut bad = build_art(0, &[(1, 1, &[0])]);
        bad[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(TileSet::new().add_art(&bad).is_err());
    }
}
