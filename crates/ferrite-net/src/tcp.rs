//! Native TCP transport with length-prefixed frames (`docs/17` §1.3).

use crate::frame::{read_frames, write_frame};
use crate::{NetError, PeerId, Transport};
use std::collections::BTreeMap;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

struct PeerConn {
    stream: TcpStream,
    rx: Vec<u8>,
}

/// Non-blocking TCP mesh: one listening socket (optional) + outbound peers.
///
/// For multi-player relay topology prefer [`crate::TcpRelayServer`] /
/// [`crate::TcpRelayClient`].
pub struct TcpTransport {
    self_id: PeerId,
    listener: Option<TcpListener>,
    peers: Vec<PeerId>,
    conns: BTreeMap<PeerId, PeerConn>,
    /// Pending accept assigner (host assigns next seat).
    next_seat: u8,
    max_peers: u8,
}

impl TcpTransport {
    /// Bind a listening TCP socket as peer `self_id` (usually host seat 0).
    pub fn bind(self_id: PeerId, addr: SocketAddr, max_peers: u8) -> Result<Self, NetError> {
        let listener = TcpListener::bind(addr).map_err(NetError::from_io)?;
        listener.set_nonblocking(true).map_err(NetError::from_io)?;
        Ok(Self {
            self_id,
            listener: Some(listener),
            peers: vec![self_id],
            conns: BTreeMap::new(),
            next_seat: self_id.0.saturating_add(1),
            max_peers: max_peers.clamp(2, 8),
        })
    }

    /// Connect to `addr` as `self_id` (joiner). Host must already be listening.
    pub fn connect(self_id: PeerId, host: PeerId, addr: SocketAddr) -> Result<Self, NetError> {
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))
            .map_err(NetError::from_io)?;
        stream.set_nonblocking(true).map_err(NetError::from_io)?;
        stream.set_nodelay(true).map_err(NetError::from_io)?;
        let mut conns = BTreeMap::new();
        conns.insert(
            host,
            PeerConn {
                stream,
                rx: Vec::new(),
            },
        );
        Ok(Self {
            self_id,
            listener: None,
            peers: {
                let mut p = vec![self_id, host];
                p.sort();
                p.dedup();
                p
            },
            conns,
            next_seat: 0,
            max_peers: 8,
        })
    }

    /// Local listen address (host only).
    pub fn local_addr(&self) -> Result<SocketAddr, NetError> {
        self.listener
            .as_ref()
            .ok_or_else(|| NetError::Reject("not listening".into()))?
            .local_addr()
            .map_err(NetError::from_io)
    }

    /// Accept pending connections and assign seats (host).
    pub fn accept_pending(&mut self) -> Result<Vec<PeerId>, NetError> {
        let Some(listener) = self.listener.as_ref() else {
            return Ok(Vec::new());
        };
        let mut accepted = Vec::new();
        loop {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    if self.conns.len() + 1 >= usize::from(self.max_peers) {
                        drop(stream);
                        continue;
                    }
                    stream.set_nonblocking(true).map_err(NetError::from_io)?;
                    stream.set_nodelay(true).map_err(NetError::from_io)?;
                    let id = PeerId(self.next_seat);
                    self.next_seat = self.next_seat.saturating_add(1);
                    if !self.peers.contains(&id) {
                        self.peers.push(id);
                        self.peers.sort();
                    }
                    self.conns.insert(
                        id,
                        PeerConn {
                            stream,
                            rx: Vec::new(),
                        },
                    );
                    accepted.push(id);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(NetError::from_io(e)),
            }
        }
        Ok(accepted)
    }

    /// Register an already-connected stream as `id`.
    pub fn add_stream(&mut self, id: PeerId, stream: TcpStream) -> Result<(), NetError> {
        stream.set_nonblocking(true).map_err(NetError::from_io)?;
        stream.set_nodelay(true).map_err(NetError::from_io)?;
        if !self.peers.contains(&id) {
            self.peers.push(id);
            self.peers.sort();
        }
        self.conns.insert(
            id,
            PeerConn {
                stream,
                rx: Vec::new(),
            },
        );
        Ok(())
    }

    /// Local peer id.
    #[must_use]
    pub fn self_id(&self) -> PeerId {
        self.self_id
    }
}

impl Transport for TcpTransport {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        if let Some(c) = self.conns.get_mut(&peer) {
            let _ = write_frame(&mut c.stream, bytes);
        }
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        let _ = self.accept_pending();
        let ids: Vec<PeerId> = self.conns.keys().copied().collect();
        for id in ids {
            let Some(c) = self.conns.get_mut(&id) else {
                continue;
            };
            match read_frames(&mut c.stream, &mut c.rx) {
                Ok(frames) => {
                    for f in frames {
                        out.push((id, f));
                    }
                }
                Err(_) => {
                    // Drop broken peer.
                    self.conns.remove(&id);
                    self.peers.retain(|p| *p != id);
                }
            }
        }
    }

    fn peers(&self) -> &[PeerId] {
        &self.peers
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{NetTicCmd, TimedCmd, decode_timed_cmd, encode_timed_cmd};
    use std::net::Ipv4Addr;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn tcp_host_client_exchanges_timed_cmd() {
        let mut host =
            TcpTransport::bind(PeerId(0), SocketAddr::from((Ipv4Addr::LOCALHOST, 0)), 4).unwrap();
        let addr = host.local_addr().unwrap();
        let mut client = TcpTransport::connect(PeerId(1), PeerId(0), addr).unwrap();

        // Host accepts.
        for _ in 0..50 {
            let got = host.accept_pending().unwrap();
            if !got.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(host.peers().contains(&PeerId(1)));

        let timed = TimedCmd {
            tic: 3,
            peer: PeerId(1),
            cmd: NetTicCmd {
                forward_move: 42,
                ..Default::default()
            },
        };
        client.send(PeerId(0), &encode_timed_cmd(&timed));

        let mut got = None;
        for _ in 0..50 {
            let mut out = Vec::new();
            host.poll(&mut out);
            if let Some((_, bytes)) = out.into_iter().next() {
                got = Some(decode_timed_cmd(&bytes).unwrap());
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(got, Some(timed));
    }
}
