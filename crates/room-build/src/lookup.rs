//! `LOOKUP.DAT`: palette swap tables (`pal` on sprites and walls).
//!
//! Layout: `u8 count`, then `count × (u8 pal, [u8; 256] map)`, then five
//! 256-entry six-bit RGB palettes (water, night-vision/slime, title, 3D Realms,
//! ending). The final palettes are optional in older/custom files.

use crate::error::BuildError;
use std::collections::BTreeMap;

/// Palette swaps by `pal` number.
#[derive(Debug, Clone, Default)]
pub struct Lookups {
    tables: BTreeMap<u8, [u8; 256]>,
    /// Full-screen palettes following the lookup maps in Atomic `LOOKUP.DAT`.
    pub base_palettes: Option<BasePalettes>,
}

/// Full-screen RGB palettes stored after the sprite/wall remap tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasePalettes {
    /// Palette selected in underwater sectors (`lotag == 2`).
    pub water: [u8; 768],
    /// Palette selected by night vision and the FLOORSLIME ceiling.
    pub slime: [u8; 768],
    /// Title palette (file slot 2).
    pub title: [u8; 768],
    /// 3D Realms logo palette (file slot 3).
    pub drealms: [u8; 768],
    /// Ending palette (file slot 4).
    pub ending: [u8; 768],
}

impl Lookups {
    /// Iterator over the parsed palette swaps in file order-independent key order.
    pub fn iter(&self) -> impl Iterator<Item = (u8, &[u8; 256])> {
        self.tables.iter().map(|(&pal, map)| (pal, map))
    }

    /// Parse `LOOKUP.DAT`.
    ///
    /// # Errors
    ///
    /// Truncated table.
    pub fn parse(data: &[u8]) -> Result<Self, BuildError> {
        let count = *data.first().ok_or(BuildError::Truncated("LOOKUP.DAT"))?;
        let mut tables = BTreeMap::new();
        for i in 0..usize::from(count) {
            let at = 1 + i * 257;
            let entry = data
                .get(at..at + 257)
                .ok_or(BuildError::Truncated("LOOKUP.DAT table"))?;
            let mut map = [0u8; 256];
            map.copy_from_slice(&entry[1..]);
            tables.insert(entry[0], map);
        }
        let palettes_at = 1 + usize::from(count) * 257;
        let base_palettes = if data.len() >= palettes_at + 5 * 768 {
            let mut palettes: [[u8; 768]; 5] = std::array::from_fn(|_| [0; 768]);
            for (slot, palette) in palettes.iter_mut().enumerate() {
                let start = palettes_at + slot * 768;
                for (out, &raw) in palette.iter_mut().zip(&data[start..start + 768]) {
                    let six_bit = raw & 63;
                    *out = (six_bit << 2) | (six_bit >> 4);
                }
            }
            Some(BasePalettes {
                water: palettes[0],
                slime: palettes[1],
                title: palettes[2],
                drealms: palettes[3],
                ending: palettes[4],
            })
        } else {
            None
        };
        Ok(Self {
            tables,
            base_palettes,
        })
    }

    /// Remap tile pixels through swap `pal`; palette 0 or an unknown swap is
    /// the identity. Index 255 (transparent) is kept.
    #[must_use]
    pub fn apply(&self, pal: u8, pixels: &[u8]) -> Vec<u8> {
        match self.tables.get(&pal) {
            Some(map) if pal != 0 => pixels
                .iter()
                .map(|&p| if p == 255 { 255 } else { map[usize::from(p)] })
                .collect(),
            _ => pixels.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn swaps_colours_but_keeps_transparency() {
        let mut data = vec![1u8, 21];
        data.extend((0..=255u8).map(|i| i.wrapping_add(1)));
        let l = Lookups::parse(&data).unwrap();
        assert_eq!(l.apply(21, &[0, 7, 255]), [1, 8, 255]);
        assert_eq!(l.apply(3, &[7]), [7]);
        assert!(Lookups::parse(&data[..100]).is_err());
        assert!(Lookups::parse(&[]).is_err());
    }
}
