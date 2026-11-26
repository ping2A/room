//! Packet-server relay framing shared by TCP and WebSocket transports.
//!
//! Wire layout (one datagram / WebSocket binary message):
//! `FTRL` magic + msg type + payload. TCP wraps this in a length-prefixed
//! [`crate::frame`]; WebSocket sends the FTRL blob as a single binary frame.

use crate::{NetError, PeerId};

/// Room TCP/WebSocket Relay magic.
pub const MAGIC: &[u8; 4] = b"FTRL";
/// Client → server hello (display name).
pub const MSG_HELLO: u8 = 1;
/// Server → client seat assignment.
pub const MSG_WELCOME: u8 = 2;
/// Bidirectional payload (lockstep / lobby bytes).
pub const MSG_DATA: u8 = 3;
/// Server → client reject.
pub const MSG_REJECT: u8 = 4;
/// Bidirectional live voice (PCM/Opus payload; not lockstep).
pub const MSG_VOICE: u8 = 5;
/// Server → clients: peer joined (seat + display name).
pub const MSG_PEER_JOIN: u8 = 6;
/// Server → clients: peer left / disconnected (seat).
pub const MSG_PEER_LEAVE: u8 = 7;

/// Client → server: HELLO with player name.
#[must_use]
pub fn pack_hello(name: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_HELLO);
    let b = name.as_bytes();
    let n = u16::try_from(b.len().min(64)).unwrap_or(64);
    v.extend_from_slice(&n.to_le_bytes());
    v.extend_from_slice(&b[..usize::from(n)]);
    v
}

/// Server → client: WELCOME with seat and expected peer count.
#[must_use]
pub fn pack_welcome(peer: PeerId, peer_count: u8) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_WELCOME);
    v.push(peer.0);
    v.push(peer_count);
    v
}

/// Server → client: reject with reason.
#[must_use]
pub fn pack_reject(reason: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_REJECT);
    let b = reason.as_bytes();
    let n = u16::try_from(b.len().min(200)).unwrap_or(200);
    v.extend_from_slice(&n.to_le_bytes());
    v.extend_from_slice(&b[..usize::from(n)]);
    v
}

/// Client → server: lockstep / lobby payload (sender = TCP/WS connection).
#[must_use]
pub fn pack_client_data(payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_DATA);
    v.extend_from_slice(payload);
    v
}

/// Server → clients: payload tagged with origin seat.
#[must_use]
pub fn pack_server_data(from: PeerId, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_DATA);
    v.push(from.0);
    v.extend_from_slice(payload);
    v
}

/// Client → server: live voice payload (same framing as data).
#[must_use]
pub fn pack_client_voice(payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_VOICE);
    v.extend_from_slice(payload);
    v
}

/// Server → clients: voice tagged with origin seat.
#[must_use]
pub fn pack_server_voice(from: PeerId, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_VOICE);
    v.push(from.0);
    v.extend_from_slice(payload);
    v
}

/// Server → clients: peer joined with display name.
#[must_use]
pub fn pack_peer_join(seat: PeerId, name: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_PEER_JOIN);
    v.push(seat.0);
    let b = name.as_bytes();
    let n = u16::try_from(b.len().min(64)).unwrap_or(64);
    v.extend_from_slice(&n.to_le_bytes());
    v.extend_from_slice(&b[..usize::from(n)]);
    v
}

/// Server → clients: peer disconnected.
#[must_use]
pub fn pack_peer_leave(seat: PeerId) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_PEER_LEAVE);
    v.push(seat.0);
    v
}

/// Parse `MSG_PEER_JOIN` payload → `(seat, name)`.
///
/// # Errors
///
/// Truncated / bad length.
pub fn parse_peer_join(rest: &[u8]) -> Result<(PeerId, String), NetError> {
    if rest.is_empty() {
        return Err(NetError::BadPacket);
    }
    let seat = PeerId(rest[0]);
    if rest.len() < 3 {
        return Err(NetError::BadPacket);
    }
    let n = u16::from_le_bytes([rest[1], rest[2]]) as usize;
    let end = 3 + n.min(rest.len().saturating_sub(3));
    let name = String::from_utf8_lossy(&rest[3..end]).into_owned();
    Ok((seat, name))
}

/// Parse `MSG_PEER_LEAVE` payload → seat.
///
/// # Errors
///
/// Empty payload.
pub fn parse_peer_leave(rest: &[u8]) -> Result<PeerId, NetError> {
    if rest.is_empty() {
        return Err(NetError::BadPacket);
    }
    Ok(PeerId(rest[0]))
}

/// Parse a relay datagram into `(msg_type, rest)`.
///
/// # Errors
///
/// [`NetError::BadPacket`] when magic is wrong or buffer is too short.
pub fn parse_relay(bytes: &[u8]) -> Result<(u8, &[u8]), NetError> {
    if bytes.len() < 5 || &bytes[0..4] != MAGIC {
        return Err(NetError::BadPacket);
    }
    Ok((bytes[4], &bytes[5..]))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn hello_welcome_roundtrip_shape() {
        let h = pack_hello("doomguy");
        let (t, rest) = parse_relay(&h).unwrap();
        assert_eq!(t, MSG_HELLO);
        assert!(rest.len() >= 2);
        let w = pack_welcome(PeerId(1), 2);
        let (t, rest) = parse_relay(&w).unwrap();
        assert_eq!(t, MSG_WELCOME);
        assert_eq!(rest, &[1, 2]);
    }

    #[test]
    fn voice_roundtrip_shape() {
        let pcm = [1u8, 2, 3, 4];
        let c = pack_client_voice(&pcm);
        let (t, rest) = parse_relay(&c).unwrap();
        assert_eq!(t, MSG_VOICE);
        assert_eq!(rest, &pcm);
        let s = pack_server_voice(PeerId(2), &pcm);
        let (t, rest) = parse_relay(&s).unwrap();
        assert_eq!(t, MSG_VOICE);
        assert_eq!(rest[0], 2);
        assert_eq!(&rest[1..], &pcm);
    }

    #[test]
    fn peer_join_leave_roundtrip() {
        let j = pack_peer_join(PeerId(1), "p2");
        let (t, rest) = parse_relay(&j).unwrap();
        assert_eq!(t, MSG_PEER_JOIN);
        let (seat, name) = parse_peer_join(rest).unwrap();
        assert_eq!(seat, PeerId(1));
        assert_eq!(name, "p2");
        let l = pack_peer_leave(PeerId(1));
        let (t, rest) = parse_relay(&l).unwrap();
        assert_eq!(t, MSG_PEER_LEAVE);
        assert_eq!(parse_peer_leave(rest).unwrap(), PeerId(1));
    }
}
