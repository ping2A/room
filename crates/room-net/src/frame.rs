//! Length-prefixed framing for TCP / Noise streams.

use crate::NetError;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Read, Write};

/// Max payload size (lockstep + lobby comfortably under this).
pub const MAX_FRAME: usize = 64 * 1024;

/// Encode `payload` as `u32_le_len || bytes`.
#[must_use]
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len());
    let len = u32::try_from(payload.len().min(MAX_FRAME)).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&payload[..len as usize]);
    out
}

/// Try to pull one complete frame from `buf`; returns payload and consumes bytes.
pub fn try_decode_frame(buf: &mut Vec<u8>) -> Result<Option<Vec<u8>>, NetError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if len > MAX_FRAME {
        return Err(NetError::BadPacket);
    }
    if buf.len() < 4 + len {
        return Ok(None);
    }
    let payload = buf[4..4 + len].to_vec();
    buf.drain(..4 + len);
    Ok(Some(payload))
}

/// Write one frame to a stream. Returns `Ok(true)` if fully written.
///
/// On non-blocking sockets prefer [`write_buf`] with a persistent byte buffer so
/// partial writes are not retried from the start.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> Result<bool, NetError> {
    let frame = encode_frame(payload);
    write_buf(w, &frame)
}

/// Write all of `buf` (or none / WouldBlock). Returns whether the full buffer was sent.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_buf<W: Write>(w: &mut W, buf: &[u8]) -> Result<bool, NetError> {
    let mut sent = 0usize;
    while sent < buf.len() {
        match w.write(&buf[sent..]) {
            Ok(0) => {
                return Err(NetError::from_io(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "tcp write zero",
                )));
            }
            Ok(n) => sent += n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                return Ok(false);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(NetError::from_io(e)),
        }
    }
    Ok(true)
}

/// Read available bytes into `acc` and return any complete frames.
///
/// Prefer **non-blocking** sockets. After one successful `Ok(n)` we stop and
/// decode — a second blocking `read` would hang when the peer has nothing more
/// to send. Callers that poll frequently (TCP pump / transport poll) drain the
/// rest on subsequent calls until `WouldBlock`.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_frames<R: Read>(r: &mut R, acc: &mut Vec<u8>) -> Result<Vec<Vec<u8>>, NetError> {
    let mut tmp = [0u8; 4096];
    loop {
        match r.read(&mut tmp) {
            Ok(0) => break, // EOF
            Ok(n) => {
                acc.extend_from_slice(&tmp[..n]);
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(NetError::from_io(e)),
        }
    }
    let mut out = Vec::new();
    while let Some(p) = try_decode_frame(acc)? {
        out.push(p);
    }
    Ok(out)
}
