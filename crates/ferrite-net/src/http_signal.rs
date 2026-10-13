//! HTTP [`SignalBus`] client for `ferrite-signal` rooms.

use crate::NetError;
use crate::signal_mesh::SignalBus;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Client for `http://{host}/room/...` (see `ferrite-signal`).
#[derive(Clone, Debug)]
pub struct HttpSignalBus {
    /// `host:port` (no scheme).
    host: String,
}

impl HttpSignalBus {
    /// Connect to a signalling server at `host` (`127.0.0.1:3535`).
    #[must_use]
    pub fn new(host: impl Into<String>) -> Self {
        Self { host: host.into() }
    }
}

impl SignalBus for HttpSignalBus {
    fn join(&mut self, room: &str) -> Result<String, NetError> {
        let path = format!("/room/{room}/join");
        let body = http_exchange(&self.host, "POST", &path, "")?;
        json_field(&body, "peer").ok_or_else(|| NetError::Reject("join: no peer".into()))
    }

    fn signal(&mut self, room: &str, from: &str, to: &str, payload: &[u8]) -> Result<(), NetError> {
        let path = format!("/room/{room}/signal");
        let hex = bytes_to_hex(payload);
        let body = format!(
            "{{\"from\":{},\"to\":{},\"payload\":{}}}",
            json_quote(from),
            json_quote(to),
            json_quote(&hex)
        );
        let _ = http_exchange(&self.host, "POST", &path, &body)?;
        Ok(())
    }

    fn poll(&mut self, room: &str, peer: &str) -> Result<Vec<(String, Vec<u8>)>, NetError> {
        let path = format!("/room/{room}/poll?peer={peer}");
        let body = http_exchange(&self.host, "GET", &path, "")?;
        Ok(parse_messages(&body))
    }
}

fn http_exchange(host: &str, method: &str, path: &str, body: &str) -> Result<String, NetError> {
    let mut stream = TcpStream::connect(host).map_err(NetError::from_io)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(NetError::from_io)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(NetError::from_io)?;
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(NetError::from_io)?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).map_err(NetError::from_io)?;
    let text = String::from_utf8_lossy(&buf);
    let body = text
        .split("\r\n\r\n")
        .nth(1)
        .or_else(|| text.split("\n\n").nth(1))
        .unwrap_or("")
        .to_owned();
    if !text.contains("200 OK") {
        return Err(NetError::Reject(format!(
            "http: {}",
            text.lines().next().unwrap_or("?")
        )));
    }
    Ok(body)
}

fn bytes_to_hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push(HEX[(x >> 4) as usize] as char);
        s.push(HEX[(x & 0xf) as usize] as char);
    }
    s
}

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Some(out)
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

fn json_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":");
    let after = body.split(&needle).nth(1)?.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = after.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '"' => return Some(out),
            c => out.push(c),
        }
    }
    None
}

/// Parse `{"messages":[{"from":"..","payload":"hex"},...]}`.
fn parse_messages(body: &str) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(idx) = rest.find("\"from\":") {
        rest = &rest[idx..];
        let Some(from) = json_field(rest, "from") else {
            break;
        };
        let Some(payload_hex) = json_field(rest, "payload") else {
            rest = &rest[7..];
            continue;
        };
        if let Some(bytes) = hex_to_bytes(&payload_hex) {
            out.push((from, bytes));
        }
        rest = &rest[7..];
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{
        LockstepSession, NetTicCmd, PeerId, SignalMeshTransport, TimedCmd, Transport,
        encode_timed_cmd,
    };
    use ferrite_signal::{SignalState, serve_listener};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn hex_roundtrip() {
        let b = [0u8, 1, 255, 16];
        assert_eq!(hex_to_bytes(&bytes_to_hex(&b)).unwrap(), b);
    }

    #[test]
    fn http_signal_mesh_lockstep() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let _ = serve_listener(listener, SignalState::new());
        });
        thread::sleep(Duration::from_millis(50));

        let host = addr.to_string();
        let p0 = PeerId(0);
        let p1 = PeerId(1);
        let mut t0 =
            SignalMeshTransport::join(HttpSignalBus::new(host.clone()), "game", p0).unwrap();
        let mut t1 = SignalMeshTransport::join(HttpSignalBus::new(host), "game", p1).unwrap();
        let n0 = t0.signal_name().to_owned();
        let n1 = t1.signal_name().to_owned();
        t0.add_remote(p1, n1);
        t1.add_remote(p0, n0);

        let mut s0 = LockstepSession::new(p0, [p0, p1], 1);
        let mut s1 = LockstepSession::new(p1, [p0, p1], 1);
        for s in [&mut s0, &mut s1] {
            s.push_remote(TimedCmd {
                tic: 0,
                peer: p0,
                cmd: NetTicCmd::default(),
            });
            s.push_remote(TimedCmd {
                tic: 0,
                peer: p1,
                cmd: NetTicCmd::default(),
            });
        }

        let msg = s0.submit_local(NetTicCmd {
            yaw: 9,
            ..Default::default()
        });
        s0.broadcast(&mut t0, &msg);
        s1.recv(&mut t1);
        let msg1 = s1.submit_local(NetTicCmd {
            yaw: 11,
            ..Default::default()
        });
        s1.broadcast(&mut t1, &msg1);
        s0.recv(&mut t0);

        assert!(s0.try_advance().is_some());
        assert!(s1.try_advance().is_some());
        let f0 = s0.try_advance().unwrap();
        let f1 = s1.try_advance().unwrap();
        assert_eq!(f0, f1);
        assert_eq!(f0[&p0].yaw, 9);
        assert_eq!(f0[&p1].yaw, 11);

        // Direct send still works over HTTP hex payload.
        let mut out = Vec::new();
        t0.send(p1, &encode_timed_cmd(&msg));
        t1.poll(&mut out);
        assert_eq!(out.len(), 1);
    }
}
