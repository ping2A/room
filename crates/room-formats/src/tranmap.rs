//! Boom `TRANMAP` / Heretic `TINTTAB` / Strife `XLATAB` — 256×256 palette blend tables.

use crate::error::FormatError;

/// Exact byte size of a Boom TRANMAP lump.
pub const TRANMAP_SIZE: usize = 256 * 256;

/// Palette-index translucency table (`result = table[fg][bg]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tranmap {
    /// Row-major: `table[fg * 256 + bg]` → blended palette index.
    pub table: Box<[u8; TRANMAP_SIZE]>,
}

impl Tranmap {
    /// Parse a 65536-byte TRANMAP / TINTTAB / XLATAB lump.
    pub fn parse(data: &[u8]) -> Result<Self, FormatError> {
        if data.len() != TRANMAP_SIZE {
            return Err(FormatError::Truncated(
                "TRANMAP must be exactly 65536 bytes",
            ));
        }
        let mut table = Box::new([0u8; TRANMAP_SIZE]);
        table.copy_from_slice(data);
        Ok(Self { table })
    }

    /// Blend foreground palette index over background → result index.
    #[must_use]
    pub fn blend(&self, fg: u8, bg: u8) -> u8 {
        self.table[usize::from(fg) * 256 + usize::from(bg)]
    }

    /// Build a 50% average blend table from a PLAYPAL palette (synthetic default).
    ///
    /// Used when a WAD has no TRANMAP; not identical to Boom's precomputed table
    /// but sufficient for palette-mode translucency smoke tests.
    #[must_use]
    pub fn generate_average(palette: &[[u8; 3]; 256]) -> Self {
        let mut table = Box::new([0u8; TRANMAP_SIZE]);
        for fg in 0..256u16 {
            for bg in 0..256u16 {
                let a = palette[fg as usize];
                let b = palette[bg as usize];
                let tr = (u16::from(a[0]) + u16::from(b[0])) / 2;
                let tg = (u16::from(a[1]) + u16::from(b[1])) / 2;
                let tb = (u16::from(a[2]) + u16::from(b[2])) / 2;
                table[fg as usize * 256 + bg as usize] =
                    nearest_palette_index(palette, tr as u8, tg as u8, tb as u8);
            }
        }
        Self { table }
    }
}

fn nearest_palette_index(palette: &[[u8; 3]; 256], r: u8, g: u8, b: u8) -> u8 {
    let mut best = 0u8;
    let mut best_d = u32::MAX;
    for (i, c) in palette.iter().enumerate() {
        let dr = i32::from(c[0]) - i32::from(r);
        let dg = i32::from(c[1]) - i32::from(g);
        let db = i32::from(c[2]) - i32::from(b);
        let d = (dr * dr + dg * dg + db * db) as u32;
        if d < best_d {
            best_d = d;
            best = i as u8;
            if d == 0 {
                break;
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parse_exact_size() {
        let mut data = vec![0u8; TRANMAP_SIZE];
        data[0] = 7;
        data[256 + 1] = 9;
        let t = Tranmap::parse(&data).unwrap();
        assert_eq!(t.blend(0, 0), 7);
        assert_eq!(t.blend(1, 1), 9);
        assert!(Tranmap::parse(&[0u8; 100]).is_err());
    }

    #[test]
    fn generate_average_identity_on_same_color() {
        let mut pal = [[0u8; 3]; 256];
        for (i, c) in pal.iter_mut().enumerate() {
            let v = i as u8;
            *c = [v, v, v];
        }
        let t = Tranmap::generate_average(&pal);
        assert_eq!(t.blend(40, 40), 40);
        // Midpoint of 0 and 100 ≈ 50.
        assert_eq!(t.blend(0, 100), 50);
    }
}
