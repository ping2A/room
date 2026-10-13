//! Native UDP transport (`docs/17` §1.3).

use crate::{NetError, PeerId, Transport};
use std::collections::BTreeMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};

/// Non-blocking UDP peer transport.
pub struct UdpTransport {
    sock: UdpSocket,
    self_id: PeerId,
    peers: Vec<PeerId>,
    addrs: BTreeMap<PeerId, SocketAddr>,
    /// Reverse map for inbound demux (unknown senders ignored).
    by_addr: BTreeMap<SocketAddr, PeerId>,
}

impl UdpTransport {
    /// Bind `self_id` on `addr` (use `127.0.0.1:0` for an ephemeral port).
    ///
    /// # Errors
    ///
    /// Socket bind / non-blocking setup failure.
    pub fn bind(self_id: PeerId, addr: SocketAddr) -> Result<Self, NetError> {
        let sock = UdpSocket::bind(addr).map_err(NetError::from_io)?;
        sock.set_nonblocking(true).map_err(NetError::from_io)?;
        Ok(Self {
            sock,
            self_id,
            peers: vec![self_id],
            addrs: BTreeMap::new(),
            by_addr: BTreeMap::new(),
        })
    }

    /// Local socket address.
    ///
    /// # Errors
    ///
    /// `getsockname` failure.
    pub fn local_addr(&self) -> Result<SocketAddr, NetError> {
        self.sock.local_addr().map_err(NetError::from_io)
    }

    /// Register a remote peer address.
    pub fn add_peer(&mut self, id: PeerId, addr: SocketAddr) {
        if id == self.self_id {
            return;
        }
        if !self.peers.contains(&id) {
            self.peers.push(id);
            self.peers.sort();
        }
        self.addrs.insert(id, addr);
        self.by_addr.insert(addr, id);
    }

    /// Local peer id.
    #[must_use]
    pub fn self_id(&self) -> PeerId {
        self.self_id
    }

    /// Update local peer id after lobby seat assignment (keeps the same socket).
    pub fn set_self_id(&mut self, id: PeerId) {
        self.self_id = id;
        if !self.peers.contains(&id) {
            self.peers.push(id);
            self.peers.sort();
        }
    }

    /// Send raw bytes to a socket address (lobby before peer ids assigned).
    ///
    /// # Errors
    ///
    /// Send failure (other than WouldBlock).
    pub fn send_raw(&self, addr: SocketAddr, bytes: &[u8]) -> Result<(), NetError> {
        match self.sock.send_to(bytes, addr) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(NetError::from_io(e)),
        }
    }

    /// Poll raw packets (addr + bytes), including unknown senders.
    pub fn poll_raw(&self, out: &mut Vec<(SocketAddr, Vec<u8>)>) {
        let mut buf = [0u8; 2048];
        loop {
            match self.sock.recv_from(&mut buf) {
                Ok((n, addr)) => out.push((addr, buf[..n].to_vec())),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    }
}

impl Transport for UdpTransport {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        let Some(&addr) = self.addrs.get(&peer) else {
            return;
        };
        let _ = self.send_raw(addr, bytes);
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        let mut raw = Vec::new();
        self.poll_raw(&mut raw);
        for (addr, bytes) in raw {
            if let Some(&peer) = self.by_addr.get(&addr) {
                out.push((peer, bytes));
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
    fn udp_loopback_exchanges_timed_cmd() {
        let a = PeerId(0);
        let b = PeerId(1);
        let mut ta = UdpTransport::bind(a, SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let mut tb = UdpTransport::bind(b, SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let addr_a = ta.local_addr().unwrap();
        let addr_b = tb.local_addr().unwrap();
        ta.add_peer(b, addr_b);
        tb.add_peer(a, addr_a);

        let timed = TimedCmd {
            tic: 9,
            peer: a,
            cmd: NetTicCmd {
                forward_move: 100,
                ..Default::default()
            },
        };
        ta.send(b, &encode_timed_cmd(&timed));

        let mut got = None;
        for _ in 0..50 {
            let mut out = Vec::new();
            tb.poll(&mut out);
            if let Some((_, bytes)) = out.into_iter().next() {
                got = Some(decode_timed_cmd(&bytes).unwrap());
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(got, Some(timed));
    }

    #[test]
    fn udp_lobby_hello_welcome_then_lockstep_cmds() {
        use crate::{
            ABI_MAJOR, ENGINE_VERSION, Hello, LobbyHost, LobbyMsg, LockstepSession, TimedCmd,
        };

        let mut host =
            UdpTransport::bind(PeerId(0), SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let host_addr = host.local_addr().unwrap();
        let mut join =
            UdpTransport::bind(PeerId(254), SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();

        let mut lobby = LobbyHost::new(0x55, 2, 1, 99, "E1M1", 3);
        let hello = LobbyMsg::Hello(Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR,
            fingerprint: 0x55,
            name: "p1".into(),
        });
        join.send_raw(host_addr, &hello.encode()).unwrap();

        let mut welcomed = false;
        for _ in 0..100 {
            let mut raw = Vec::new();
            host.poll_raw(&mut raw);
            for (addr, bytes) in raw {
                if let Ok(LobbyMsg::Hello(h)) = LobbyMsg::decode(&bytes) {
                    let reply = lobby.on_hello(h);
                    host.send_raw(addr, &reply.encode()).unwrap();
                    if let LobbyMsg::Welcome(w) = reply {
                        host.add_peer(w.peer_id, addr);
                        join.set_self_id(w.peer_id);
                        join.add_peer(PeerId(0), host_addr);
                        welcomed = true;
                    }
                }
            }
            if welcomed {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(welcomed);
        let start = lobby.start_msg().encode();
        let others: Vec<PeerId> = host
            .peers()
            .iter()
            .copied()
            .filter(|p| *p != PeerId(0))
            .collect();
        for p in others {
            host.send(p, &start);
        }

        let mut s0 = LockstepSession::new(PeerId(0), [PeerId(0), PeerId(1)], 1);
        let mut s1 = LockstepSession::new(PeerId(1), [PeerId(0), PeerId(1)], 1);
        for s in [&mut s0, &mut s1] {
            for i in 0..2u8 {
                s.push_remote(TimedCmd {
                    tic: 0,
                    peer: PeerId(i),
                    cmd: NetTicCmd::default(),
                });
            }
        }
        let m0 = s0.submit_local(NetTicCmd {
            forward_move: 11,
            ..Default::default()
        });
        s0.broadcast(&mut host, &m0);
        let m1 = s1.submit_local(NetTicCmd {
            forward_move: 22,
            ..Default::default()
        });
        s1.broadcast(&mut join, &m1);

        let mut got0 = None;
        let mut got1 = None;
        for _ in 0..100 {
            s0.recv(&mut host);
            s1.recv(&mut join);
            if got0.is_none() {
                got0 = s0.try_advance();
            }
            if got1.is_none() {
                got1 = s1.try_advance();
            }
            if got0.is_some() && got1.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        // First advance is seeded tic 0.
        let f0 = got0.expect("host tic0");
        let f1 = got1.expect("join tic0");
        assert_eq!(f0, f1);

        got0 = None;
        got1 = None;
        for _ in 0..100 {
            s0.recv(&mut host);
            s1.recv(&mut join);
            if got0.is_none() {
                got0 = s0.try_advance();
            }
            if got1.is_none() {
                got1 = s1.try_advance();
            }
            if got0.is_some() && got1.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        let f0 = got0.expect("host tic1");
        let f1 = got1.expect("join tic1");
        assert_eq!(f0, f1);
        assert_eq!(f0[&PeerId(0)].forward_move, 11);
        assert_eq!(f0[&PeerId(1)].forward_move, 22);
    }
}
