//! Join tickets (HMAC-SHA256; docs/22 §5).
//!
//! Format: `base64url(payload).base64url(mac)` where payload is
//! `room_id|ticket_id|name|exp_unix` (ASCII). One-time use is tracked by the registry.

use crate::util::{UtilError, hex_encode};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Default ticket lifetime (long enough for IWAD download + WASM boot).
pub const TICKET_TTL_SECS: u64 = 600;

/// Issued join ticket claims.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TicketClaims {
    /// Room id.
    pub room_id: String,
    /// Unique ticket id (hex).
    pub ticket_id: String,
    /// Display name.
    pub name: String,
    /// Expiry unix seconds.
    pub exp: u64,
}

/// Ticket encode / verify errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TicketError {
    /// Bad encoding or MAC.
    #[error("invalid ticket")]
    Invalid,
    /// Past expiry.
    #[error("ticket expired")]
    Expired,
    /// Room id mismatch.
    #[error("wrong room")]
    WrongRoom,
    /// RNG failure while minting.
    #[error("rng")]
    Rng,
}

/// HMAC-SHA256 ticket key (32 bytes recommended).
#[derive(Clone)]
pub struct TicketKey([u8; 32]);

impl TicketKey {
    /// Derive a key from an arbitrary secret string (dev / CLI).
    #[must_use]
    pub fn from_secret(secret: &str) -> Self {
        let mut h = Sha256::new();
        h.update(secret.as_bytes());
        let out = h.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&out);
        Self(key)
    }

    /// Random key from OS CSPRNG.
    ///
    /// # Errors
    ///
    /// RNG failure.
    pub fn random() -> Result<Self, UtilError> {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|_| UtilError::Rng)?;
        Ok(Self(key))
    }
}

/// Mint a one-time join ticket.
///
/// # Errors
///
/// RNG failure.
pub fn mint_ticket(
    key: &TicketKey,
    room_id: &str,
    name: &str,
    now_unix: u64,
) -> Result<(String, TicketClaims), TicketError> {
    let mut tid = [0u8; 16];
    getrandom::fill(&mut tid).map_err(|_| TicketError::Rng)?;
    let ticket_id = hex_encode(&tid);
    let exp = now_unix.saturating_add(TICKET_TTL_SECS);
    let claims = TicketClaims {
        room_id: room_id.to_owned(),
        ticket_id: ticket_id.clone(),
        name: name.to_owned(),
        exp,
    };
    let payload = format!("{room_id}|{ticket_id}|{name}|{exp}");
    let mac = hmac_sha256(&key.0, payload.as_bytes());
    let token = format!("{}.{}", b64url(payload.as_bytes()), b64url(&mac));
    Ok((token, claims))
}

/// Verify a ticket string for `expected_room` at `now_unix`.
///
/// # Errors
///
/// Invalid / expired / wrong room.
pub fn verify_ticket(
    key: &TicketKey,
    token: &str,
    expected_room: &str,
    now_unix: u64,
) -> Result<TicketClaims, TicketError> {
    let (payload_b64, mac_b64) = token.split_once('.').ok_or(TicketError::Invalid)?;
    let payload = b64url_decode(payload_b64).ok_or(TicketError::Invalid)?;
    let mac = b64url_decode(mac_b64).ok_or(TicketError::Invalid)?;
    let expect = hmac_sha256(&key.0, &payload);
    if !const_eq(&expect, &mac) {
        return Err(TicketError::Invalid);
    }
    let text = std::str::from_utf8(&payload).map_err(|_| TicketError::Invalid)?;
    let mut parts = text.splitn(4, '|');
    let room_id = parts.next().ok_or(TicketError::Invalid)?;
    let ticket_id = parts.next().ok_or(TicketError::Invalid)?;
    let name = parts.next().ok_or(TicketError::Invalid)?;
    let exp: u64 = parts
        .next()
        .ok_or(TicketError::Invalid)?
        .parse()
        .map_err(|_| TicketError::Invalid)?;
    if room_id != expected_room {
        return Err(TicketError::WrongRoom);
    }
    if now_unix > exp {
        return Err(TicketError::Expired);
    }
    Ok(TicketClaims {
        room_id: room_id.to_owned(),
        ticket_id: ticket_id.to_owned(),
        name: name.to_owned(),
        exp,
    })
}

fn hmac_sha256(key: &[u8; 32], data: &[u8]) -> [u8; 32] {
    // HMAC-SHA256 (RFC 2104) without an extra crate.
    const BLK: usize = 64;
    let mut k0 = [0u8; BLK];
    k0[..32].copy_from_slice(key);
    let mut ipad = [0x36u8; BLK];
    let mut opad = [0x5cu8; BLK];
    for i in 0..BLK {
        ipad[i] ^= k0[i];
        opad[i] ^= k0[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    let out = outer.finalize();
    let mut mac = [0u8; 32];
    mac.copy_from_slice(&out);
    mac
}

fn b64url(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n =
            (u32::from(bytes[i]) << 16) | (u32::from(bytes[i + 1]) << 8) | u32::from(bytes[i + 2]);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = u32::from(bytes[i]) << 16;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
    } else if rem == 2 {
        let n = (u32::from(bytes[i]) << 16) | (u32::from(bytes[i + 1]) << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
    }
    out
}

fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let a = val(bytes[i])?;
        let b = val(bytes[i + 1])?;
        let c = val(bytes[i + 2])?;
        let d = val(bytes[i + 3])?;
        out.push((a << 2) | (b >> 4));
        out.push(((b & 0xf) << 4) | (c >> 2));
        out.push(((c & 0x3) << 6) | d);
        i += 4;
    }
    let rem = bytes.len() - i;
    if rem == 0 {
        return Some(out);
    }
    if rem == 2 {
        let a = val(bytes[i])?;
        let b = val(bytes[i + 1])?;
        out.push((a << 2) | (b >> 4));
        return Some(out);
    }
    if rem == 3 {
        let a = val(bytes[i])?;
        let b = val(bytes[i + 1])?;
        let c = val(bytes[i + 2])?;
        out.push((a << 2) | (b >> 4));
        out.push(((b & 0xf) << 4) | (c >> 2));
        return Some(out);
    }
    None
}

fn const_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        d |= x ^ y;
    }
    d == 0
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let key = TicketKey::from_secret("test-secret");
        let (tok, claims) = mint_ticket(&key, "roomabc", "alice", 1_000).unwrap();
        let got = verify_ticket(&key, &tok, "roomabc", 1_010).unwrap();
        assert_eq!(got, claims);
        assert!(verify_ticket(&key, &tok, "roomabc", 1_700).is_err());
        assert!(verify_ticket(&key, &tok, "other", 1_010).is_err());
    }
}
