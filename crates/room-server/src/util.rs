//! Small helpers.

use thiserror::Error;

/// Random / encoding errors.
#[derive(Debug, Error)]
pub enum UtilError {
    /// OS RNG failed.
    #[error("rng failed")]
    Rng,
}

/// 128-bit room id as lowercase hex (32 chars).
pub fn new_room_id() -> Result<String, UtilError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| UtilError::Rng)?;
    Ok(hex_encode(&bytes))
}

/// 256-bit resume token as lowercase hex.
pub fn new_resume_token() -> Result<String, UtilError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| UtilError::Rng)?;
    Ok(hex_encode(&bytes))
}

/// Random u32 seed.
pub fn random_u32() -> Result<u32, UtilError> {
    let mut bytes = [0u8; 4];
    getrandom::fill(&mut bytes).map_err(|_| UtilError::Rng)?;
    Ok(u32::from_le_bytes(bytes))
}

/// Lowercase hex encoding.
pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn room_id_length() {
        let id = new_room_id().unwrap();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
