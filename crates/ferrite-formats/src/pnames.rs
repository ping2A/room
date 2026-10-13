//! Doom `PNAMES` lump (patch name table for TEXTURE1/2).

use crate::error::FormatError;

/// Parsed `PNAMES` table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pnames {
    /// Patch names in order (uppercase, NUL-trimmed).
    pub names: Vec<String>,
}

/// Parse a `PNAMES` lump: `i32` count + `[u8;8]` × count.
pub fn parse_pnames(data: &[u8]) -> Result<Pnames, FormatError> {
    if data.len() < 4 {
        return Err(FormatError::Truncated("PNAMES header"));
    }
    let count = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if count < 0 {
        return Err(FormatError::Invalid("PNAMES negative count"));
    }
    let count = count as usize;
    let need = 4usize
        .checked_add(
            count
                .checked_mul(8)
                .ok_or(FormatError::Invalid("PNAMES size"))?,
        )
        .ok_or(FormatError::Invalid("PNAMES size"))?;
    if data.len() < need {
        return Err(FormatError::Truncated("PNAMES entries"));
    }
    let mut names = Vec::with_capacity(count);
    for i in 0..count {
        let o = 4 + i * 8;
        let raw = &data[o..o + 8];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(8);
        let s = std::str::from_utf8(&raw[..end])
            .map_err(|_| FormatError::Invalid("PNAMES utf8"))?
            .trim()
            .to_ascii_uppercase();
        names.push(s);
    }
    Ok(Pnames { names })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn two_names() {
        let mut data = Vec::new();
        data.extend_from_slice(&2i32.to_le_bytes());
        data.extend_from_slice(b"WALL01\0\0");
        data.extend_from_slice(b"DOOR1\0\0\0");
        let p = parse_pnames(&data).unwrap();
        assert_eq!(p.names, ["WALL01", "DOOR1"]);
    }
}
