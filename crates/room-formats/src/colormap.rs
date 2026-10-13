//! COLORMAP lump (34 maps × 256 bytes typically).

use crate::error::FormatError;

/// COLORMAP contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Colormap {
    /// Each map is 256 palette indices.
    pub maps: Vec<[u8; 256]>,
}

impl Colormap {
    /// Parse COLORMAP. Size must be a multiple of 256.
    pub fn parse(data: &[u8]) -> Result<Self, FormatError> {
        if data.is_empty() || data.len() % 256 != 0 {
            return Err(FormatError::Truncated("COLORMAP size must be N×256"));
        }
        let mut maps = Vec::with_capacity(data.len() / 256);
        for chunk in data.chunks_exact(256) {
            let mut m = [0u8; 256];
            m.copy_from_slice(chunk);
            maps.push(m);
        }
        Ok(Self { maps })
    }
}
