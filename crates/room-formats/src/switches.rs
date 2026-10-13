//! Boom SWITCHES lump.

use crate::error::FormatError;

/// One SWITCHES record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchRecord {
    /// Off texture.
    pub off: String,
    /// On texture.
    pub on: String,
    /// Episode (1=shareware, 2=registered, 3=Doom II).
    pub episode: i16,
}

fn c_name(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).to_ascii_uppercase()
}

/// Parse SWITCHES until episode 0.
pub fn parse_switches(data: &[u8]) -> Result<Vec<SwitchRecord>, FormatError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 20 <= data.len() {
        let episode = i16::from_le_bytes([data[i + 18], data[i + 19]]);
        if episode == 0 {
            break;
        }
        out.push(SwitchRecord {
            off: c_name(&data[i..i + 9]),
            on: c_name(&data[i + 9..i + 18]),
            episode,
        });
        i += 20;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn one_switch() {
        let mut b = vec![0u8; 40];
        b[..6].copy_from_slice(b"SW1OFF");
        b[9..14].copy_from_slice(b"SW1ON");
        b[18..20].copy_from_slice(&1i16.to_le_bytes());
        let recs = parse_switches(&b).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].off, "SW1OFF");
        assert_eq!(recs[0].on, "SW1ON");
    }
}
