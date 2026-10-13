//! TCP packet-server / relay for larger sessions (`docs/17` §1.1 `-netmode 1`).
//!
//! Host (or a dedicated process) runs [`TcpRelayServer`]. Clients use
//! [`TcpRelayClient`]. Every payload from a peer is tagged with its seat and
//! forwarded to all other seats — lockstep stays identical to P2P, but only one
//! reachable address is required.

use crate::frame::{encode_frame, read_frames, write_frame};
use crate::{NetError, PeerId, Transport};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

const MAGIC: &[u8; 4] = b"FTRL"; // Ferrite TCP Relay
const MSG_HELLO: u8 = 1;
const MSG_WELCOME: u8 = 2;
const MSG_DATA: u8 = 3;
const MSG_REJECT: u8 = 4;

fn pack_hello(name: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_HELLO);
    let b = name.as_bytes();
    let n = u16::try_from(b.len().min(64)).unwrap_or(64);
    v.extend_from_slice(&n.to_le_bytes());
    v.extend_from_slice(&b[..usize::from(n)]);
    v
}

fn pack_welcome(peer: PeerId, peer_count: u8) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_WELCOME);
    v.push(peer.0);
    v.push(peer_count);
    v
}

fn pack_reject(reason: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_REJECT);
    let b = reason.as_bytes();
    let n = u16::try_from(b.len().min(200)).unwrap_or(200);
    v.extend_from_slice(&n.to_le_bytes());
    v.extend_from_slice(&b[..usize::from(n)]);
    v
}

/// Client → server: payload only (sender identity is the TCP connection).
fn pack_client_data(payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_DATA);
    v.extend_from_slice(payload);
    v
}

/// Server → clients: tagged with origin seat.
fn pack_server_data(from: PeerId, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(MAGIC);
    v.push(MSG_DATA);
    v.push(from.0);
    v.extend_from_slice(payload);
    v
}

fn parse_relay(bytes: &[u8]) -> Result<(u8, &[u8]), NetError> {
    if bytes.len() < 5 || &bytes[0..4] != MAGIC {
        return Err(NetError::BadPacket);
    }
    Ok((bytes[4], &bytes[5..]))
}

struct Client {
    stream: TcpStream,
    rx: Vec<u8>,
    tx: Vec<Vec<u8>>,
    name: String,
}

fn enqueue_frame(c: &mut Client, payload: &[u8]) {
    c.tx.push(encode_frame(payload));
}

fn flush_client(c: &mut Client) {
    // Prefer blocking writes for control/data frames so lobby WELCOME is not lost
    // on WouldBlock (clients block waiting for it).
    let _ = c.stream.set_nonblocking(false);
    while let Some(frame) = c.tx.first() {
        match c.stream.write_all(frame) {
            Ok(()) => {
                c.tx.remove(0);
            }
            Err(_) => break,
        }
    }
    let _ = c.stream.set_nonblocking(true);
}

/// Host-side TCP relay (packet-server). Seat 0 is the local host player.
pub struct TcpRelayServer {
    listener: TcpListener,
    max_players: u8,
    next_seat: u8,
    clients: BTreeMap<PeerId, Client>,
    peers: Vec<PeerId>,
    /// Inbound payloads for the local host (from clients), after demux.
    local_inbox: Vec<(PeerId, Vec<u8>)>,
    self_id: PeerId,
}

impl TcpRelayServer {
    /// Bind relay on `addr` for up to `max_players` seats (2..=8), including host.
    pub fn bind(addr: SocketAddr, max_players: u8) -> Result<Self, NetError> {
        let listener = TcpListener::bind(addr).map_err(NetError::from_io)?;
        listener.set_nonblocking(true).map_err(NetError::from_io)?;
        let max_players = max_players.clamp(2, 8);
        Ok(Self {
            listener,
            max_players,
            next_seat: 1,
            clients: BTreeMap::new(),
            peers: vec![PeerId(0)],
            local_inbox: Vec::new(),
            self_id: PeerId(0),
        })
    }

    /// Listen address.
    pub fn local_addr(&self) -> Result<SocketAddr, NetError> {
        self.listener.local_addr().map_err(NetError::from_io)
    }

    /// Connected seats (including host).
    #[must_use]
    pub fn peer_count(&self) -> u8 {
        self.peers.len() as u8
    }

    /// Accept + read + relay one poll cycle. Also fills host inbox.
    pub fn pump(&mut self) -> Result<(), NetError> {
        self.accept_once()?;
        self.read_and_relay()?;
        Ok(())
    }

    fn accept_once(&mut self) -> Result<(), NetError> {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if self.peers.len() as u8 >= self.max_players {
                        let _ = write_frame(&mut &stream, &pack_reject("lobby full"));
                        continue;
                    }
                    stream.set_nonblocking(true).map_err(NetError::from_io)?;
                    stream.set_nodelay(true).map_err(NetError::from_io)?;
                    // Wait for HELLO on next pump reads — stash as provisional seat.
                    let id = PeerId(self.next_seat);
                    self.next_seat = self.next_seat.saturating_add(1);
                    if !self.peers.contains(&id) {
                        self.peers.push(id);
                        self.peers.sort();
                    }
                    self.clients.insert(
                        id,
                        Client {
                            stream,
                            rx: Vec::new(),
                            tx: Vec::new(),
                            name: String::new(),
                        },
                    );
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(NetError::from_io(e)),
            }
        }
        Ok(())
    }

    fn read_and_relay(&mut self) -> Result<(), NetError> {
        let ids: Vec<PeerId> = self.clients.keys().copied().collect();
        let mut drop_list = Vec::new();
        let mut fanout: Vec<(PeerId, Vec<u8>)> = Vec::new();

        for id in ids {
            let Some(c) = self.clients.get_mut(&id) else {
                continue;
            };
            flush_client(c);
            let frames = match read_frames(&mut c.stream, &mut c.rx) {
                Ok(f) => f,
                Err(_) => {
                    drop_list.push(id);
                    continue;
                }
            };
            for frame in frames {
                match parse_relay(&frame) {
                    Ok((MSG_HELLO, rest)) => {
                        if rest.len() >= 2 {
                            let n = u16::from_le_bytes([rest[0], rest[1]]) as usize;
                            if rest.len() >= 2 + n {
                                c.name = String::from_utf8_lossy(&rest[2..2 + n]).into_owned();
                            }
                        }
                        enqueue_frame(c, &pack_welcome(id, self.max_players));
                        flush_client(c);
                    }
                    Ok((MSG_DATA, payload)) => {
                        // Deliver to host local inbox (connection id = seat).
                        self.local_inbox.push((id, payload.to_vec()));
                        fanout.push((id, payload.to_vec()));
                    }
                    _ => {}
                }
            }
        }

        for id in drop_list {
            self.clients.remove(&id);
            self.peers.retain(|p| *p != id);
        }

        // Relay DATA to every client except the originator.
        for (from, payload) in fanout {
            let packet = pack_server_data(from, &payload);
            let targets: Vec<PeerId> = self
                .clients
                .keys()
                .copied()
                .filter(|p| *p != from)
                .collect();
            for tid in targets {
                if let Some(c) = self.clients.get_mut(&tid) {
                    enqueue_frame(c, &packet);
                    flush_client(c);
                }
            }
        }
        Ok(())
    }

    /// Broadcast raw payload from the host (seat 0) to every client.
    pub fn broadcast_host(&mut self, payload: &[u8]) {
        let packet = pack_server_data(self.self_id, payload);
        let ids: Vec<PeerId> = self.clients.keys().copied().collect();
        for id in ids {
            if let Some(c) = self.clients.get_mut(&id) {
                enqueue_frame(c, &packet);
                flush_client(c);
            }
        }
    }
}

impl Transport for TcpRelayServer {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        if peer == self.self_id {
            return;
        }
        if let Some(c) = self.clients.get_mut(&peer) {
            let packet = pack_server_data(self.self_id, bytes);
            enqueue_frame(c, &packet);
            flush_client(c);
        } else {
            // Unknown peer → broadcast (host→all).
            self.broadcast_host(bytes);
        }
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        let _ = self.pump();
        out.append(&mut self.local_inbox);
    }

    fn peers(&self) -> &[PeerId] {
        &self.peers
    }
}

/// Client connected to a [`TcpRelayServer`].
pub struct TcpRelayClient {
    self_id: PeerId,
    stream: TcpStream,
    rx: Vec<u8>,
    peers: Vec<PeerId>,
    /// Pending outbound if write blocks.
    pending: Vec<Vec<u8>>,
}

impl TcpRelayClient {
    /// Connect, send HELLO, wait for WELCOME.
    ///
    /// The server must be pumping (`TcpRelayServer::pump`) on another thread while
    /// this blocks — otherwise WELCOME never arrives.
    pub fn connect(addr: SocketAddr, name: &str, max_players: u8) -> Result<Self, NetError> {
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))
            .map_err(NetError::from_io)?;
        stream.set_nodelay(true).map_err(NetError::from_io)?;

        // Brief blocking write for HELLO, then non-blocking poll for WELCOME.
        stream.set_nonblocking(false).map_err(NetError::from_io)?;
        stream
            .write_all(&encode_frame(&pack_hello(name)))
            .map_err(NetError::from_io)?;
        stream.flush().map_err(NetError::from_io)?;
        stream.set_nonblocking(true).map_err(NetError::from_io)?;

        let mut rx = Vec::new();
        let mut self_id = PeerId(255);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if std::time::Instant::now() > deadline {
                return Err(NetError::Reject("welcome timeout".into()));
            }
            let frames = read_frames(&mut stream, &mut rx)?;
            for frame in frames {
                match parse_relay(&frame)? {
                    (MSG_WELCOME, rest) if rest.len() >= 2 => {
                        self_id = PeerId(rest[0]);
                        break;
                    }
                    (MSG_REJECT, rest) => {
                        let reason = if rest.len() >= 2 {
                            let n = u16::from_le_bytes([rest[0], rest[1]]) as usize;
                            String::from_utf8_lossy(
                                &rest[2..2 + n.min(rest.len().saturating_sub(2))],
                            )
                            .into_owned()
                        } else {
                            "rejected".into()
                        };
                        return Err(NetError::Reject(reason));
                    }
                    _ => {}
                }
            }
            if self_id.0 != 255 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let mut peers: Vec<PeerId> = (0..max_players.max(2)).map(PeerId).collect();
        if !peers.contains(&self_id) {
            peers.push(self_id);
        }
        peers.sort();
        Ok(Self {
            self_id,
            stream,
            rx,
            peers,
            pending: Vec::new(),
        })
    }

    /// Assigned seat.
    #[must_use]
    pub fn self_id(&self) -> PeerId {
        self.self_id
    }
}

impl Transport for TcpRelayClient {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        // Packet-server: one uplink fans out to everyone. Deduplicate LockstepSession
        // broadcast (which calls send once per remote peer) by only transmitting for
        // the lowest remote seat.
        let Some(first) = self
            .peers
            .iter()
            .copied()
            .filter(|p| *p != self.self_id)
            .min()
        else {
            return;
        };
        if peer != first {
            return;
        }
        let packet = pack_client_data(bytes);
        match write_frame(&mut self.stream, &packet) {
            Ok(true) => {}
            Ok(false) => self.pending.push(packet),
            Err(_) => {}
        }
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        // Flush pending.
        while let Some(p) = self.pending.first() {
            match write_frame(&mut self.stream, p) {
                Ok(true) => {
                    self.pending.remove(0);
                }
                _ => break,
            }
        }
        if let Ok(frames) = read_frames(&mut self.stream, &mut self.rx) {
            for frame in frames {
                if let Ok((MSG_DATA, rest)) = parse_relay(&frame) {
                    if rest.is_empty() {
                        continue;
                    }
                    let from = PeerId(rest[0]);
                    out.push((from, rest[1..].to_vec()));
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
    use crate::{LockstepSession, NetTicCmd, TimedCmd};
    use std::net::Ipv4Addr;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn tcp_relay_two_clients_exchange_via_host() {
        use std::sync::mpsc;

        let mut server =
            TcpRelayServer::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)), 3).unwrap();
        let addr = server.local_addr().unwrap();

        let (tx1, rx1) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx1.send(TcpRelayClient::connect(addr, "p1", 3));
        });
        thread::spawn(move || {
            let _ = tx2.send(TcpRelayClient::connect(addr, "p2", 3));
        });

        let mut c1 = None;
        let mut c2 = None;
        let mut err1 = None;
        let mut err2 = None;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && (c1.is_none() || c2.is_none()) {
            let _ = server.pump();
            if c1.is_none() {
                match rx1.try_recv() {
                    Ok(Ok(c)) => c1 = Some(c),
                    Ok(Err(e)) => err1 = Some(e),
                    Err(_) => {}
                }
            }
            if c2.is_none() {
                match rx2.try_recv() {
                    Ok(Ok(c)) => c2 = Some(c),
                    Ok(Err(e)) => err2 = Some(e),
                    Err(_) => {}
                }
            }
            if err1.is_some() || err2.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        let mut c1 = c1.unwrap_or_else(|| {
            panic!(
                "client1 connected (err={err1:?}, peers={}, clients={})",
                server.peer_count(),
                server.clients.len()
            )
        });
        let mut c2 = c2.unwrap_or_else(|| {
            panic!(
                "client2 connected (err={err2:?}, peers={}, clients={})",
                server.peer_count(),
                server.clients.len()
            )
        });
        let id_a = c1.self_id();
        let id_b = c2.self_id();
        assert_ne!(id_a, id_b);
        assert!(id_a.0 >= 1 && id_a.0 <= 2);
        assert!(id_b.0 >= 1 && id_b.0 <= 2);

        let payload = b"hello-from-a".to_vec();
        c1.send(PeerId(0), &payload);

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut got = None;
        while Instant::now() < deadline {
            let _ = server.pump();
            let mut out = Vec::new();
            c2.poll(&mut out);
            if let Some((from, bytes)) = out.into_iter().next() {
                got = Some((from, bytes));
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got, Some((id_a, payload)));

        server.broadcast_host(b"from-host");
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut saw = false;
        while Instant::now() < deadline {
            let _ = server.pump();
            let mut out = Vec::new();
            c1.poll(&mut out);
            if out
                .iter()
                .any(|(p, b)| *p == PeerId(0) && b.as_slice() == b"from-host")
            {
                saw = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(saw, "client should see host broadcast");
        let _ = LockstepSession::new(PeerId(0), [PeerId(0)], 1);
        let _ = TimedCmd {
            tic: 0,
            peer: PeerId(0),
            cmd: NetTicCmd::default(),
        };
    }
}
