//! PLAYPAL lump (14 Doom palettes × 768 bytes).

use crate::error::FormatError;

/// One 256-colour RGB palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    /// RGB triples, 256 × 3.
    pub rgb: [u8; 768],
}

/// PLAYPAL contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playpal {
    /// Palettes in order (usually 14).
    pub palettes: Vec<Palette>,
}

/// Parse a PLAYPAL lump. Accepts any multiple of 768 bytes ≥ 768.
pub fn parse_playpal(data: &[u8]) -> Result<Playpal, FormatError> {
    if data.len() < 768 || data.len() % 768 != 0 {
        return Err(FormatError::Truncated("PLAYPAL size must be N×768"));
    }
    let mut palettes = Vec::with_capacity(data.len() / 768);
    for chunk in data.chunks_exact(768) {
        let mut rgb = [0u8; 768];
        rgb.copy_from_slice(chunk);
        palettes.push(Palette { rgb });
    }
    Ok(Playpal { palettes })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn one_palette() {
        let data = vec![0u8; 768];
        let p = parse_playpal(&data).unwrap();
        assert_eq!(p.palettes.len(), 1);
    }
}
