//! Eight-byte WAD-style short names.

/// Uppercased, NUL-padded 8-byte lump name. Compared as `u64`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShortName([u8; 8]);

impl ShortName {
    /// Empty / invalid short name.
    pub const NONE: Self = Self([0; 8]);

    /// Parse from a WAD directory field (may lack NUL; mid-NUL ends the name).
    #[must_use]
    pub fn from_wad_bytes(raw: &[u8; 8]) -> Self {
        let mut out = [0u8; 8];
        for (i, &b) in raw.iter().enumerate() {
            if b == 0 {
                break;
            }
            out[i] = b.to_ascii_uppercase();
        }
        Self(out)
    }

    /// From a file stem or path segment: uppercase ASCII, truncate to 8 bytes.
    #[must_use]
    pub fn from_stem(stem: &str) -> Self {
        let mut out = [0u8; 8];
        let bytes = stem.as_bytes();
        let n = bytes.len().min(8);
        for (i, &b) in bytes.iter().take(n).enumerate() {
            out[i] = b.to_ascii_uppercase();
        }
        Self(out)
    }

    /// Raw 8 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    /// As `u64` for map keys (little-endian load of the 8 bytes).
    #[must_use]
    pub fn as_u64(self) -> u64 {
        u64::from_le_bytes(self.0)
    }

    /// Display string without trailing NULs.
    #[must_use]
    pub fn as_str(&self) -> &str {
        let end = self.0.iter().position(|&b| b == 0).unwrap_or(8);
        std::str::from_utf8(&self.0[..end]).unwrap_or("")
    }

    /// True if all zeros.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == [0; 8]
    }
}

impl core::fmt::Debug for ShortName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ShortName({:?})", self.as_str())
    }
}

impl core::fmt::Display for ShortName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialOrd for ShortName {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ShortName {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}
