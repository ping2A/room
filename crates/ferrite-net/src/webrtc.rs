//! WebRTC mesh via [`matchbox_socket`] (`docs/17` §1.3).
//!
//! Enable with `--features webrtc`. Callers must drive the returned
//! `MessageLoopFuture` (async runtime or per-frame poll). For CI / headless
//! lockstep without WebRTC crypto, use [`crate::SignalMeshTransport`].

use crate::{NetError, PeerId, Transport};
use matchbox_socket::{PeerId as MbPeerId, WebRtcSocket, WebRtcSocketBuilder};
use std::collections::BTreeMap;

/// Build an unreliable full-mesh socket for `room_url` (e.g. `ws://127.0.0.1:3536/room`).
#[must_use]
pub fn connect_unreliable(
    room_url: impl Into<String>,
) -> (MatchboxTransport, matchbox_socket::MessageLoopFuture) {
    let (socket, loop_fut) = WebRtcSocketBuilder::new(room_url)
        .add_unreliable_channel()
        .build();
    (MatchboxTransport::new(socket), loop_fut)
}

/// [`Transport`] adapter over a [`WebRtcSocket`] channel 0.
pub struct MatchboxTransport {
    socket: WebRtcSocket,
    /// Ferrite seat → matchbox peer.
    to_mb: BTreeMap<PeerId, MbPeerId>,
    /// Matchbox peer → Ferrite seat.
    from_mb: BTreeMap<MbPeerId, PeerId>,
    /// Connected seats (including self once assigned).
    peers: Vec<PeerId>,
    self_id: Option<PeerId>,
    next_seat: u8,
}

impl MatchboxTransport {
    /// Wrap a built socket (channel 0 must exist).
    #[must_use]
    pub fn new(socket: WebRtcSocket) -> Self {
        Self {
            socket,
            to_mb: BTreeMap::new(),
            from_mb: BTreeMap::new(),
            peers: Vec::new(),
            self_id: None,
            next_seat: 0,
        }
    }

    /// Poll peer join/leave; assign Ferrite seats in join order.
    pub fn update_peers(&mut self) {
        for (mb, state) in self.socket.update_peers() {
            match state {
                matchbox_socket::PeerState::Connected => {
                    if self.from_mb.contains_key(&mb) {
                        continue;
                    }
                    let seat = PeerId(self.next_seat);
                    self.next_seat = self.next_seat.saturating_add(1);
                    self.from_mb.insert(mb, seat);
                    self.to_mb.insert(seat, mb);
                    if !self.peers.contains(&seat) {
                        self.peers.push(seat);
                        self.peers.sort();
                    }
                }
                matchbox_socket::PeerState::Disconnected => {
                    if let Some(seat) = self.from_mb.remove(&mb) {
                        self.to_mb.remove(&seat);
                        self.peers.retain(|p| *p != seat);
                    }
                }
            }
        }
        if self.self_id.is_none() {
            if let Some(mb) = self.socket.id() {
                // Prefer seat 0 for the local peer when still free.
                let seat = if self.to_mb.contains_key(&PeerId(0)) {
                    let s = PeerId(self.next_seat);
                    self.next_seat = self.next_seat.saturating_add(1);
                    s
                } else {
                    if self.next_seat == 0 {
                        self.next_seat = 1;
                    }
                    PeerId(0)
                };
                self.self_id = Some(seat);
                self.to_mb.insert(seat, mb);
                self.from_mb.insert(mb, seat);
                if !self.peers.contains(&seat) {
                    self.peers.push(seat);
                    self.peers.sort();
                }
            }
        }
    }

    /// Local Ferrite seat once the signalling handshake finished.
    #[must_use]
    pub fn self_id(&self) -> Option<PeerId> {
        self.self_id
    }

    /// Look up Ferrite seat for a matchbox peer.
    #[must_use]
    pub fn seat_of(&self, mb: MbPeerId) -> Option<PeerId> {
        self.from_mb.get(&mb).copied()
    }
}

impl Transport for MatchboxTransport {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        let Some(&mb) = self.to_mb.get(&peer) else {
            return;
        };
        let packet = bytes.to_vec().into_boxed_slice();
        self.socket.channel_mut(0).send(packet, mb);
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        self.update_peers();
        for (mb, packet) in self.socket.channel_mut(0).receive() {
            if let Some(seat) = self.from_mb.get(&mb).copied() {
                out.push((seat, packet.to_vec()));
            }
        }
    }

    fn peers(&self) -> &[PeerId] {
        &self.peers
    }
}

impl MatchboxTransport {
    /// Map an already-known matchbox peer to a fixed Ferrite seat (lobby assign).
    ///
    /// # Errors
    ///
    /// Seat already bound to a different peer.
    pub fn bind_seat(&mut self, seat: PeerId, mb: MbPeerId) -> Result<(), NetError> {
        if let Some(existing) = self.to_mb.get(&seat) {
            if *existing != mb {
                return Err(NetError::Reject("seat already bound".into()));
            }
            return Ok(());
        }
        self.to_mb.insert(seat, mb);
        self.from_mb.insert(mb, seat);
        if !self.peers.contains(&seat) {
            self.peers.push(seat);
            self.peers.sort();
        }
        if seat.0 >= self.next_seat {
            self.next_seat = seat.0.saturating_add(1);
        }
        Ok(())
    }
}
