//! Per-map compatibility database (`docs/19-compatibility.md` §3).
//!
//! Parses GZDoom-style `compatibility.txt` (`flags.txt` in `ferrite.pk3/compat/`).

use crate::error::MapError;
use std::collections::BTreeMap;

/// MD5 digest as lowercase hex (32 chars).
pub type MapMd5Hex = String;

/// Parsed `compatibility.txt` / `flags.txt`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompatDatabase {
    /// Map MD5 (lowercase hex) → flag names (`shorttex`, `trace`, …).
    pub entries: BTreeMap<MapMd5Hex, Vec<String>>,
}

impl CompatDatabase {
    /// Parse a `compatibility.txt` body.
    pub fn parse(text: &str) -> Result<Self, MapError> {
        let mut entries = BTreeMap::new();
        let mut pending: Vec<MapMd5Hex> = Vec::new();
        let mut lines = text.lines().peekable();
        while let Some(raw) = lines.next() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if line == "{" {
                let flags = parse_flag_block(&mut lines)?;
                if pending.is_empty() {
                    return Err(MapError::Message(
                        "compat flags.txt: `{` without preceding MD5".into(),
                    ));
                }
                for md5 in pending.drain(..) {
                    entries.insert(md5, flags.clone());
                }
                continue;
            }
            // One or more MD5 tokens on the line (space-separated), optional trailing comment already stripped.
            for tok in line.split_whitespace() {
                if looks_like_md5(tok) {
                    pending.push(tok.to_ascii_lowercase());
                }
            }
        }
        if !pending.is_empty() {
            // Trailing MD5s without a block inherit an empty flag list (GZDoom allows grouping).
            for md5 in pending {
                entries.entry(md5).or_default();
            }
        }
        Ok(Self { entries })
    }

    /// Flags for a map MD5 (lowercase hex), if any.
    #[must_use]
    pub fn flags_for(&self, md5_hex: &str) -> Option<&[String]> {
        self.entries
            .get(&md5_hex.to_ascii_lowercase())
            .map(Vec::as_slice)
    }

    /// Merge named flags into GZDoom `compatflags` / `compatflags2` bitfields.
    ///
    /// Unknown names are ignored (forward compatible with new GZDoom flags).
    #[must_use]
    pub fn bits_for(&self, md5_hex: &str) -> (u32, u32) {
        let Some(flags) = self.flags_for(md5_hex) else {
            return (0, 0);
        };
        let mut f1 = 0u32;
        let mut f2 = 0u32;
        for name in flags {
            if let Some(b) = compat_flag_bit(name) {
                f1 |= b;
            } else if let Some(b) = compat_flag2_bit(name) {
                f2 |= b;
            }
        }
        (f1, f2)
    }
}

fn strip_comment(line: &str) -> &str {
    line.split("//").next().unwrap_or(line)
}

fn looks_like_md5(tok: &str) -> bool {
    tok.len() == 32 && tok.chars().all(|c| c.is_ascii_hexdigit())
}

fn parse_flag_block<'a>(
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Result<Vec<String>, MapError> {
    let mut flags = Vec::new();
    for raw in lines.by_ref() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line == "}" {
            return Ok(flags);
        }
        for tok in line.split(|c: char| c.is_whitespace() || c == ';') {
            let t = tok.trim();
            if !t.is_empty() {
                flags.push(t.to_ascii_lowercase());
            }
        }
    }
    Err(MapError::Message(
        "compat flags.txt: unclosed `{` block".into(),
    ))
}

/// Map GZDoom `compatibility.txt` names → `COMPATF_*` bits.
fn compat_flag_bit(name: &str) -> Option<u32> {
    Some(match name {
        "shorttex" => 1 << 0,
        "stairs" => 1 << 1,
        "limitpain" => 1 << 2,
        "silentpickup" => 1 << 3,
        "nopassover" => 1 << 4,
        "soundslots" | "magicsilence" => 1 << 5,
        "wallrun" => 1 << 6,
        "notossdrops" => 1 << 7,
        "useblocking" => 1 << 8,
        "nodoorlight" => 1 << 9,
        "ravenscroll" => 1 << 10,
        "soundtarget" => 1 << 11,
        "dehhealth" => 1 << 12,
        "trace" => 1 << 13,
        "dropoff" => 1 << 14,
        "boomscroll" => 1 << 15,
        "invisibility" => 1 << 16,
        "silentinstantfloors" => 1 << 17,
        "sectorsounds" => 1 << 18,
        "missileclip" => 1 << 19,
        "crossdropoff" => 1 << 20,
        "anybossdeath" => 1 << 21,
        "minotaur" => 1 << 22,
        "mushroom" => 1 << 23,
        "mbfmonstermove" => 1 << 24,
        "corpsegibs" | "vileghosts" => 1 << 25,
        "noblockfriends" => 1 << 26,
        "spritesort" => 1 << 27,
        "hitscan" => 1 << 28,
        "light" => 1 << 29,
        "polyobj" => 1 << 30,
        "maskedmidtex" => 1 << 31,
        _ => return None,
    })
}

fn compat_flag2_bit(name: &str) -> Option<u32> {
    Some(match name {
        "badangles" => 1 << 0,
        "floormove" => 1 << 1,
        "soundcutoff" => 1 << 2,
        "pointonline" => 1 << 3,
        "multiexit" => 1 << 4,
        "teleport" => 1 << 5,
        "pushwindow" => 1 << 6,
        "checkswitchrange" => 1 << 7,
        "explode1" => 1 << 8,
        "explode2" => 1 << 9,
        "railing" => 1 << 10,
        "scriptwait" => 1 << 11,
        "avoidhazards" => 1 << 12,
        "stayonlift" => 1 << 13,
        "nombf21" => 1 << 14,
        "voodoozombies" => 1 << 15,
        "fdteleport" => 1 << 16,
        _ => return None,
    })
}

/// MD5 of raw bytes as lowercase hex (GZDoom map identity).
#[must_use]
pub fn md5_hex(data: &[u8]) -> MapMd5Hex {
    use md5::{Digest, Md5};
    let d = Md5::digest(data);
    let mut out = [0u8; 16];
    out.copy_from_slice(&d);
    format_md5(&out)
}

/// Format a 16-byte MD5 digest as lowercase hex.
#[must_use]
pub fn format_md5(digest: &[u8; 16]) -> MapMd5Hex {
    let mut s = String::with_capacity(32);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_grouped_md5_and_flags() {
        let text = r#"
B2D8DA03489D1C67F60DC87FBC4EA338 // map01
801304DA3784308D333951B5E0CF8E9E // map02
{
	setslopeoverflow
	resetplayerspeed
}

6DA6FCBA8089161BDEC6A1D3F6C8D60F // Eternal Doom MAP25
{
	stairs
	maskedmidtex
	vileghosts
}
"#;
        let db = CompatDatabase::parse(text).unwrap();
        assert!(db.flags_for("b2d8da03489d1c67f60dc87fbc4ea338").is_some());
        assert_eq!(
            db.flags_for("6da6fcba8089161bdec6a1d3f6c8d60f")
                .unwrap()
                .len(),
            3
        );
        let (f1, _) = db.bits_for("6da6fcba8089161bdec6a1d3f6c8d60f");
        assert_ne!(f1 & (1 << 1), 0); // stairs
        assert_ne!(f1 & (1 << 31), 0); // maskedmidtex
        assert_ne!(f1 & (1 << 25), 0); // vileghosts
    }

    #[test]
    fn md5_hex_stable() {
        assert_eq!(md5_hex(b"hello"), "5d41402abc4b2a76b9719d911017c592");
    }
}
