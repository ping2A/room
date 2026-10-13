//! Boom ANIMATED lump.

use crate::error::FormatError;

/// One ANIMATED record (flat or texture range).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimatedRecord {
    /// 0 = flat, 1 = texture, 2 = texture with decals allowed.
    pub kind: i8,
    /// Last pic name.
    pub last: String,
    /// First pic name.
    pub first: String,
    /// Speed in tics.
    pub speed: i32,
}

fn c_name(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).to_ascii_uppercase()
}

/// Parse ANIMATED until type `0xFF`.
pub fn parse_animated(data: &[u8]) -> Result<Vec<AnimatedRecord>, FormatError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 23 <= data.len() {
        let kind = data[i] as i8;
        if kind == -1 || data[i] == 0xFF {
            break;
        }
        let last = c_name(&data[i + 1..i + 10]);
        let first = c_name(&data[i + 10..i + 19]);
        let speed = i32::from_le_bytes(data[i + 19..i + 23].try_into().unwrap_or([0; 4]));
        out.push(AnimatedRecord {
            kind,
            last,
            first,
            speed,
        });
        i += 23;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn one_record_then_end() {
        let mut b = vec![0u8; 23 + 1];
        b[0] = 0; // flat
        b[1..8].copy_from_slice(b"NUKAGE3");
        b[10..17].copy_from_slice(b"NUKAGE1");
        b[19..23].copy_from_slice(&8i32.to_le_bytes());
        b[23] = 0xFF;
        let recs = parse_animated(&b).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].first, "NUKAGE1");
        assert_eq!(recs[0].speed, 8);
    }
}
