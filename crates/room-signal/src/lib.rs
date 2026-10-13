//! In-memory signalling rooms for Room netplay (dev / CI).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

/// One signal envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    /// Sender peer id.
    pub from: String,
    /// Recipient peer id.
    pub to: String,
    /// Opaque payload (SDP / JSON).
    pub payload: String,
}

#[derive(Default)]
struct Room {
    next_peer: u32,
    inbox: BTreeMap<String, VecDeque<Envelope>>,
}

#[derive(Default)]
struct State {
    rooms: BTreeMap<String, Room>,
}

/// Shared signalling state.
#[derive(Clone, Default)]
pub struct SignalState {
    inner: Arc<Mutex<State>>,
}

impl SignalState {
    /// Empty state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Join a room → peer id.
    pub fn join(&self, room: &str) -> Result<String, String> {
        let mut g = self.inner.lock().map_err(|_| "lock".to_owned())?;
        let r = g.rooms.entry(room.to_owned()).or_default();
        let id = format!("p{}", r.next_peer);
        r.next_peer = r.next_peer.saturating_add(1);
        r.inbox.entry(id.clone()).or_default();
        Ok(id)
    }

    /// Queue a signal message.
    pub fn signal(&self, room: &str, env: Envelope) -> Result<(), String> {
        let mut g = self.inner.lock().map_err(|_| "lock".to_owned())?;
        let r = g.rooms.entry(room.to_owned()).or_default();
        r.inbox.entry(env.to.clone()).or_default().push_back(env);
        Ok(())
    }

    /// Drain inbox for `peer`.
    pub fn poll(&self, room: &str, peer: &str) -> Result<Vec<Envelope>, String> {
        let mut g = self.inner.lock().map_err(|_| "lock".to_owned())?;
        let r = g.rooms.entry(room.to_owned()).or_default();
        let q = r.inbox.entry(peer.to_owned()).or_default();
        Ok(q.drain(..).collect())
    }
}

/// Serve HTTP on `bind` until the process exits.
///
/// # Errors
///
/// Bind failure.
pub fn serve(bind: &str, state: SignalState) -> Result<(), String> {
    let listener = TcpListener::bind(bind).map_err(|e| e.to_string())?;
    eprintln!("room-signal listening on http://{bind}");
    serve_listener(listener, state)
}

/// Serve HTTP on an already-bound `listener` (tests / custom bind).
///
/// # Errors
///
/// Accept / I/O failures are logged; this only returns on listener failure.
pub fn serve_listener(listener: TcpListener, state: SignalState) -> Result<(), String> {
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let st = state.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_client(s, &st) {
                        eprintln!("room-signal: {e}");
                    }
                });
            }
            Err(e) => eprintln!("room-signal: accept: {e}"),
        }
    }
    Ok(())
}

fn handle_client(mut stream: TcpStream, state: &SignalState) -> Result<(), String> {
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(());
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let (method, path) = parse_request_line(&req).ok_or_else(|| "bad request line".to_owned())?;
    let body = body_of(&req);

    let (status, content_type, body_out) = route(method, path, body, state)?;

    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body_out}",
        body_out.len()
    );
    stream
        .write_all(resp.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn route(
    method: &str,
    path: &str,
    body: &str,
    state: &SignalState,
) -> Result<(&'static str, &'static str, String), String> {
    if method == "GET" && path == "/health" {
        return Ok(("200 OK", "text/plain", "ok".to_owned()));
    }
    if method == "POST" {
        if let Some(room) = path
            .strip_prefix("/room/")
            .and_then(|r| r.strip_suffix("/join"))
        {
            let peer = state.join(room)?;
            return Ok((
                "200 OK",
                "application/json",
                format!("{{\"peer\":{}}}", json_str(&peer)),
            ));
        }
        if let Some(room) = path
            .strip_prefix("/room/")
            .and_then(|r| r.strip_suffix("/signal"))
        {
            let from = json_field(body, "from").unwrap_or_default();
            let to = json_field(body, "to").unwrap_or_default();
            let payload = json_field(body, "payload").unwrap_or_default();
            if from.is_empty() || to.is_empty() {
                return Ok((
                    "400 Bad Request",
                    "application/json",
                    "{\"error\":\"from/to required\"}".to_owned(),
                ));
            }
            state.signal(room, Envelope { from, to, payload })?;
            return Ok(("200 OK", "application/json", "{\"ok\":true}".to_owned()));
        }
    }
    if method == "GET" {
        if let Some(rest) = path.strip_prefix("/room/") {
            if let Some((room, query)) = rest.split_once("/poll") {
                let peer = query
                    .trim_start_matches('?')
                    .split('&')
                    .find_map(|p| p.strip_prefix("peer="))
                    .unwrap_or("")
                    .to_owned();
                if peer.is_empty() {
                    return Ok((
                        "400 Bad Request",
                        "application/json",
                        "{\"error\":\"peer required\"}".to_owned(),
                    ));
                }
                let msgs = state.poll(room, &peer)?;
                let parts: Vec<String> = msgs
                    .iter()
                    .map(|e| {
                        format!(
                            "{{\"from\":{},\"to\":{},\"payload\":{}}}",
                            json_str(&e.from),
                            json_str(&e.to),
                            json_str(&e.payload)
                        )
                    })
                    .collect();
                return Ok((
                    "200 OK",
                    "application/json",
                    format!("{{\"messages\":[{}]}}", parts.join(",")),
                ));
            }
        }
    }
    Ok(("404 Not Found", "text/plain", "not found".to_owned()))
}

fn parse_request_line(req: &str) -> Option<(&str, &str)> {
    let line = req.lines().next()?;
    let mut parts = line.split_whitespace();
    Some((parts.next()?, parts.next()?))
}

fn body_of(req: &str) -> &str {
    req.split("\r\n\r\n")
        .nth(1)
        .or_else(|| req.split("\n\n").nth(1))
        .unwrap_or("")
}

fn json_str(s: &str) -> String {
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn room_relay() {
        let st = SignalState::new();
        let a = st.join("r1").unwrap();
        let b = st.join("r1").unwrap();
        assert_eq!(a, "p0");
        assert_eq!(b, "p1");
        st.signal(
            "r1",
            Envelope {
                from: a.clone(),
                to: b.clone(),
                payload: "sdp".into(),
            },
        )
        .unwrap();
        let msgs = st.poll("r1", &b).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].payload, "sdp");
        assert!(st.poll("r1", &b).unwrap().is_empty());
    }

    #[test]
    fn json_field_parses() {
        let body = r#"{"from":"p0","to":"p1","payload":"hi\"x"}"#;
        assert_eq!(json_field(body, "from").unwrap(), "p0");
        assert_eq!(json_field(body, "payload").unwrap(), "hi\"x");
    }
}
