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
}
