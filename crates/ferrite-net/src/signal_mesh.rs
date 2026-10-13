//! Full-mesh transport over a signalling bus (stand-in for matchbox WebRTC).
//!
//! Uses the same room join/signal/poll model as `ferrite-signal`. Native WebRTC
//! via `matchbox_socket` lands behind the `webrtc` feature later (`docs/17` §1.3).

use crate::{NetError, PeerId, Transport};
use std::collections::{BTreeMap, VecDeque};

/// Abstract signalling bus (in-process or HTTP to `ferrite-signal`).
pub trait SignalBus {
    /// Join `room`, return opaque peer name.
    ///
    /// # Errors
    ///
    /// Bus failure.
    fn join(&mut self, room: &str) -> Result<String, NetError>;
    /// Send `payload` from `from` to `to`.
    ///
    /// # Errors
    ///
    /// Bus failure.
    fn signal(&mut self, room: &str, from: &str, to: &str, payload: &[u8]) -> Result<(), NetError>;
    /// Drain messages for `peer` → `(from, payload)`.
    ///
    /// # Errors
    ///
    /// Bus failure.
    fn poll(&mut self, room: &str, peer: &str) -> Result<Vec<(String, Vec<u8>)>, NetError>;
}

/// In-process signalling (tests / single-process seats).
#[derive(Debug, Default)]
pub struct InMemorySignalBus {
    rooms: BTreeMap<String, MemRoom>,
}

#[derive(Debug, Default)]
struct MemRoom {
    next: u32,
    inbox: BTreeMap<String, VecDeque<(String, Vec<u8>)>>,
}

impl SignalBus for InMemorySignalBus {
    fn join(&mut self, room: &str) -> Result<String, NetError> {
        let r = self.rooms.entry(room.to_owned()).or_default();
        let id = format!("p{}", r.next);
        r.next = r.next.saturating_add(1);
        r.inbox.entry(id.clone()).or_default();
        Ok(id)
    }

    fn signal(&mut self, room: &str, from: &str, to: &str, payload: &[u8]) -> Result<(), NetError> {
        let r = self.rooms.entry(room.to_owned()).or_default();
        r.inbox
            .entry(to.to_owned())
            .or_default()
            .push_back((from.to_owned(), payload.to_vec()));
        Ok(())
    }

    fn poll(&mut self, room: &str, peer: &str) -> Result<Vec<(String, Vec<u8>)>, NetError> {
        let r = self.rooms.entry(room.to_owned()).or_default();
        let q = r.inbox.entry(peer.to_owned()).or_default();
        Ok(q.drain(..).collect())
    }
}

/// Mesh transport: each seat has a [`PeerId`] and a signalling name.
pub struct SignalMeshTransport<B: SignalBus> {
    bus: B,
    room: String,
    self_id: PeerId,
    self_name: String,
    /// All seats including self.
    peers: Vec<PeerId>,
    /// `PeerId` → signal name.
    names: BTreeMap<PeerId, String>,
    /// signal name → `PeerId`.
    by_name: BTreeMap<String, PeerId>,
}

impl<B: SignalBus> SignalMeshTransport<B> {
    /// Join `room` as `self_id` on `bus`.
    ///
    /// # Errors
    ///
    /// Bus join failure.
    pub fn join(mut bus: B, room: impl Into<String>, self_id: PeerId) -> Result<Self, NetError> {
        let room = room.into();
        let self_name = bus.join(&room)?;
        let mut names = BTreeMap::new();
        let mut by_name = BTreeMap::new();
        names.insert(self_id, self_name.clone());
        by_name.insert(self_name.clone(), self_id);
        Ok(Self {
            bus,
            room,
            self_id,
            self_name,
            peers: vec![self_id],
            names,
            by_name,
        })
    }

    /// Register a remote seat that already joined the same room.
    pub fn add_remote(&mut self, id: PeerId, signal_name: impl Into<String>) {
        if id == self.self_id {
            return;
        }
        let name = signal_name.into();
        if !self.peers.contains(&id) {
            self.peers.push(id);
            self.peers.sort();
        }
        self.names.insert(id, name.clone());
        self.by_name.insert(name, id);
    }

    /// Local signalling name.
    #[must_use]
    pub fn signal_name(&self) -> &str {
        &self.self_name
    }

    /// Local peer id.
    #[must_use]
    pub fn self_id(&self) -> PeerId {
        self.self_id
    }
}

impl<B: SignalBus> Transport for SignalMeshTransport<B> {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        if peer == self.self_id {
            return;
        }
        let Some(to) = self.names.get(&peer).cloned() else {
            return;
        };
        let _ = self.bus.signal(&self.room, &self.self_name, &to, bytes);
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        let Ok(msgs) = self.bus.poll(&self.room, &self.self_name) else {
            return;
        };
        for (from, payload) in msgs {
            if let Some(&peer) = self.by_name.get(&from) {
                out.push((peer, payload));
            }
        }
    }

    fn peers(&self) -> &[PeerId] {
        &self.peers
    }
}

/// Shared in-memory bus for multi-seat tests (cloneable handle).
#[derive(Clone, Debug, Default)]
pub struct SharedSignalBus {
    inner: std::sync::Arc<std::sync::Mutex<InMemorySignalBus>>,
}

impl SharedSignalBus {
    /// Empty shared bus.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SignalBus for SharedSignalBus {
    fn join(&mut self, room: &str) -> Result<String, NetError> {
        self.inner
            .lock()
            .map_err(|_| NetError::Reject("signal lock".into()))?
            .join(room)
    }

    fn signal(&mut self, room: &str, from: &str, to: &str, payload: &[u8]) -> Result<(), NetError> {
        self.inner
            .lock()
            .map_err(|_| NetError::Reject("signal lock".into()))?
            .signal(room, from, to, payload)
    }

    fn poll(&mut self, room: &str, peer: &str) -> Result<Vec<(String, Vec<u8>)>, NetError> {
        self.inner
            .lock()
            .map_err(|_| NetError::Reject("signal lock".into()))?
            .poll(room, peer)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{LockstepSession, NetTicCmd, TimedCmd, decode_timed_cmd};

    #[test]
    fn signal_mesh_two_peer_lockstep() {
        let bus = SharedSignalBus::new();
        let p0 = PeerId(0);
        let p1 = PeerId(1);
        let mut t0 = SignalMeshTransport::join(bus.clone(), "game", p0).unwrap();
        let mut t1 = SignalMeshTransport::join(bus, "game", p1).unwrap();
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
            yaw: 5,
            ..Default::default()
        });
        s0.broadcast(&mut t0, &msg);
        s1.recv(&mut t1);
        // also deliver to self buffer already; peer1 needs remote
        let msg1 = s1.submit_local(NetTicCmd {
            yaw: 7,
            ..Default::default()
        });
        s1.broadcast(&mut t1, &msg1);
        s0.recv(&mut t0);

        assert!(s0.try_advance().is_some());
        assert!(s1.try_advance().is_some());
        let f0 = s0.try_advance().unwrap();
        let f1 = s1.try_advance().unwrap();
        assert_eq!(f0, f1);
        assert_eq!(f0[&p0].yaw, 5);
        assert_eq!(f0[&p1].yaw, 7);

        // Direct packet still decodes.
        use crate::Transport;
        let mut out = Vec::new();
        t0.send(p1, &crate::encode_timed_cmd(&msg));
        t1.poll(&mut out);
        assert!(!out.is_empty());
        assert_eq!(decode_timed_cmd(&out[0].1).unwrap().cmd.yaw, 5);
    }
}
