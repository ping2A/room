//! `LOOKUP.DAT`: palette swap tables (`pal` on sprites and walls).
//!
//! Layout: `u8 count`, then `count × (u8 pal, [u8; 256] map)`, then five
//! alternate palettes that Room does not use.

use crate::error::BuildError;
use std::collections::BTreeMap;

/// Palette swaps by `pal` number.
#[derive(Debug, Clone, Default)]
pub struct Lookups {
    tables: BTreeMap<u8, [u8; 256]>,
}

impl Lookups {
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
        Ok(Self { tables })
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
