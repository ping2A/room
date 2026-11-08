//! Ken Silverman's `.GRP` archive: `"KenSilverman"`, file count, then
//! 16-byte entries (12-byte name, `u32` size) followed by the file data.

use crate::error::BuildError;

const MAGIC: &[u8; 12] = b"KenSilverman";

/// `true` when `data` starts with the GRP signature.
#[must_use]
pub fn is_grp(data: &[u8]) -> bool {
    data.len() >= 16 && &data[..12] == MAGIC
}

/// A parsed GRP; file bodies borrow the archive bytes.
#[derive(Debug, Clone)]
pub struct Grp<'a> {
    files: Vec<(String, &'a [u8])>,
}

/// Ordered GRP archives; later archives override earlier duplicate names.
#[derive(Debug, Clone)]
pub struct GrpStack<'a> {
    archives: Vec<Grp<'a>>,
}

impl<'a> GrpStack<'a> {
    /// Parse a base archive followed by zero or more add-on archives.
    pub fn parse_layers(archives: &[&'a [u8]]) -> Result<Self, BuildError> {
        let archives = archives
            .iter()
            .map(|bytes| Grp::parse(bytes))
            .collect::<Result<Vec<_>, _>>()?;
        if archives.is_empty() {
            return Err(BuildError::Invalid("GRP stack has no base archive"));
        }
        Ok(Self { archives })
    }

    /// Look up a file, searching add-ons before the base archive.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&'a [u8]> {
        self.archives
            .iter()
            .rev()
            .find_map(|archive| archive.get(name))
    }

    /// Names in archive/load order. Duplicates are retained so format loaders
    /// can apply each archive's content in order while `get` resolves the
    /// winning bytes from the latest archive.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.archives.iter().flat_map(Grp::names)
    }
}

impl<'a> Grp<'a> {
    /// Parse the directory of `data`.
    pub fn parse(data: &'a [u8]) -> Result<Self, BuildError> {
        if !is_grp(data) {
            return Err(BuildError::Invalid("not a GRP (no KenSilverman header)"));
        }
        let count = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;
        if count > (data.len() - 16) / 16 {
            return Err(BuildError::Truncated("GRP directory"));
        }
        let mut pos = 16 + count * 16;
        let mut files = Vec::with_capacity(count);
        for i in 0..count {
            let e = &data[16 + i * 16..32 + i * 16];
            let end = e[..12].iter().position(|&b| b == 0).unwrap_or(12);
            let name: String = e[..end]
                .iter()
                .map(|&b| char::from(b).to_ascii_uppercase())
                .collect();
            let size = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
            let body = data
                .get(pos..pos.saturating_add(size))
                .ok_or(BuildError::Truncated("GRP file body"))?;
            files.push((name, body));
            pos += size;
        }
        Ok(Self { files })
    }

    /// File body by name (case-insensitive); the last duplicate wins.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&'a [u8]> {
        self.files
            .iter()
            .rev()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, b)| *b)
    }

    /// Upper-case file names in directory order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|(n, _)| n.as_str())
    }

    /// `Some` when this archive is a known non-Duke Build game.
    #[must_use]
    pub fn other_build_game(&self) -> Option<&'static str> {
        let has = |n: &str| self.get(n).is_some();
        if has("BLOOD.INI") || has("BLOOD.RFS") || has("BLOOD000.ART") {
            return Some("Blood");
        }
        if has("SW.CON") || has("SWCUSTOM.CON") || has("NINJA.CON") {
            return Some("Shadow Warrior");
        }
        if has("REDNECK.CON") || has("RR.CON") {
            return Some("Redneck Rampage");
        }
        if has("NAM.CON") {
            return Some("NAM");
        }
        None
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    pub(crate) fn build_grp(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&(files.len() as u32).to_le_bytes());
        for (name, body) in files {
            let mut n = [0u8; 12];
            n[..name.len()].copy_from_slice(name.as_bytes());
            out.extend_from_slice(&n);
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        }
        for (_, body) in files {
            out.extend_from_slice(body);
        }
        out
    }

    #[test]
    fn reads_directory_and_bodies() {
        let bytes = build_grp(&[("a.con", b"hello"), ("B.MAP", b"xy")]);
        let grp = Grp::parse(&bytes).unwrap();
        assert_eq!(grp.names().collect::<Vec<_>>(), ["A.CON", "B.MAP"]);
        assert_eq!(grp.get("A.con"), Some(&b"hello"[..]));
        assert_eq!(grp.get("b.map"), Some(&b"xy"[..]));
        assert!(grp.get("c").is_none());
        assert!(grp.other_build_game().is_none());
        let blood = build_grp(&[("BLOOD.INI", b"[Blood]")]);
        assert_eq!(
            Grp::parse(&blood).unwrap().other_build_game(),
            Some("Blood")
        );
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        assert!(Grp::parse(b"nope").is_err());
        let mut bytes = build_grp(&[("A", b"hello")]);
        bytes.truncate(bytes.len() - 2);
        assert!(Grp::parse(&bytes).is_err());
        let mut huge = MAGIC.to_vec();
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(Grp::parse(&huge).is_err());
    }
}
