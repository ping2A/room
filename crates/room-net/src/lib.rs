//! Lockstep netcode and transports (`docs/17-netplay-demos-saves.md` §1).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod desync;
mod frame;
#[cfg(not(target_arch = "wasm32"))]
mod http_signal;
mod lobby;
#[cfg(all(feature = "noise", not(target_arch = "wasm32")))]
mod noise_wrap;
mod relay;
mod signal_mesh;
#[cfg(not(target_arch = "wasm32"))]
mod tcp;
#[cfg(not(target_arch = "wasm32"))]
mod tcp_server;
#[cfg(not(target_arch = "wasm32"))]
mod udp;
#[cfg(all(feature = "webrtc", not(target_arch = "wasm32")))]
mod webrtc;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub use desync::{DesyncCollector, PeerChecksum};
pub use frame::{MAX_FRAME, encode_frame, try_decode_frame};
#[cfg(not(target_arch = "wasm32"))]
pub use http_signal::HttpSignalBus;
pub use lobby::{
    ABI_MAJOR, ENGINE_VERSION, Hello, LobbyHost, LobbyMsg, Ping, Pong, Ready, Reject, Start,
    Welcome, delay_for_rtts, validate_hello,
};
#[cfg(all(feature = "noise", not(target_arch = "wasm32")))]
pub use noise_wrap::{NOISE_PATTERN, NoiseKeys, NoiseTransport};
pub use relay::{
    MAGIC as RELAY_MAGIC, MSG_DATA, MSG_HELLO, MSG_PEER_JOIN, MSG_PEER_LEAVE, MSG_REJECT,
    MSG_VOICE, MSG_WELCOME, pack_client_data, pack_client_voice, pack_hello, pack_peer_join,
    pack_peer_leave, pack_reject, pack_server_data, pack_server_voice, pack_welcome,
    parse_peer_join, parse_peer_leave, parse_relay,
};
pub use signal_mesh::{InMemorySignalBus, SharedSignalBus, SignalBus, SignalMeshTransport};
#[cfg(not(target_arch = "wasm32"))]
pub use tcp::TcpTransport;
#[cfg(not(target_arch = "wasm32"))]
pub use tcp_server::{TcpRelayClient, TcpRelayServer};
#[cfg(not(target_arch = "wasm32"))]
pub use udp::UdpTransport;
#[cfg(all(feature = "webrtc", not(target_arch = "wasm32")))]
pub use webrtc::{MatchboxTransport, connect_unreliable};

/// Opaque peer identifier (stable for a session).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerId(pub u8);

/// Compact per-tic input exchanged between peers (mirrors playsim `TicCmd`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetTicCmd {
    /// Button mask.
    pub buttons: u32,
    /// Pitch delta.
    pub pitch: i16,
    /// Yaw delta.
    pub yaw: i16,
    /// Roll delta.
    pub roll: i16,
    /// Forward move.
    pub forward_move: i16,
    /// Side move.
    pub side_move: i16,
    /// Up move.
    pub up_move: i16,
    /// Weapon select (`0` = none).
    pub weapon_select: u8,
}

/// One peer's command for a specific tic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimedCmd {
    /// Simulation tic.
    pub tic: u32,
    /// Peer that produced the command.
    pub peer: PeerId,
    /// Command payload.
    pub cmd: NetTicCmd,
}

/// Unreliable byte transport (`docs/17` §1.3).
pub trait Transport {
    /// Queue bytes to `peer`.
    fn send(&mut self, peer: PeerId, bytes: &[u8]);
    /// Drain inbound packets into `out`.
    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>);
    /// Connected peers (excluding self).
    fn peers(&self) -> &[PeerId];
}

const AUX_PACKET_MAGIC: &[u8; 4] = b"FRAU";
const MAX_AUX_PACKET_LEN: usize = 64;
const MAX_QUEUED_AUX_PACKETS: usize = 128;

/// In-process full-mesh transport (tests / single-process multi-seat).
#[derive(Debug, Default)]
pub struct LoopbackMesh {
    peers: Vec<PeerId>,
    /// `inbox[recv][send]` queues.
    inboxes: BTreeMap<PeerId, VecDeque<(PeerId, Vec<u8>)>>,
}

impl LoopbackMesh {
    /// Create a mesh for `peers` (must include every seat).
    #[must_use]
    pub fn new(peers: impl IntoIterator<Item = PeerId>) -> Self {
        let peers: Vec<PeerId> = peers.into_iter().collect();
        let mut inboxes = BTreeMap::new();
        for &p in &peers {
            inboxes.insert(p, VecDeque::new());
        }
        Self { peers, inboxes }
    }

    /// Mutable endpoint for one peer.
    pub fn endpoint_mut(&mut self, self_id: PeerId) -> LoopbackEndpointMut<'_> {
        LoopbackEndpointMut {
            self_id,
            mesh: self,
        }
    }
}

/// Mutable [`Transport`] handle into a [`LoopbackMesh`].
pub struct LoopbackEndpointMut<'a> {
    self_id: PeerId,
    mesh: &'a mut LoopbackMesh,
}

impl Transport for LoopbackEndpointMut<'_> {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        if peer == self.self_id {
            return;
        }
        if let Some(q) = self.mesh.inboxes.get_mut(&peer) {
            q.push_back((self.self_id, bytes.to_vec()));
        }
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        if let Some(q) = self.mesh.inboxes.get_mut(&self.self_id) {
            while let Some(p) = q.pop_front() {
                out.push(p);
            }
        }
    }

    fn peers(&self) -> &[PeerId] {
        // Filter self out for callers that iterate remotes.
        // Stored list includes self; expose all and let session skip.
        &self.mesh.peers
    }
}

/// Lossy wrapper: drops packets with probability `drop_permille` / 1000 (deterministic LCG).
pub struct LossyTransport<T> {
    inner: T,
    drop_permille: u32,
    rng: u32,
}

impl<T> LossyTransport<T> {
    /// Wrap `inner`; `drop_permille` in `0..=1000`.
    #[must_use]
    pub fn new(inner: T, drop_permille: u32, seed: u32) -> Self {
        Self {
            inner,
            drop_permille: drop_permille.min(1000),
            rng: seed | 1,
        }
    }

    fn next_u32(&mut self) -> u32 {
        // LCG (Numerical Recipes)
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        self.rng
    }

    fn should_drop(&mut self) -> bool {
        if self.drop_permille == 0 {
            return false;
        }
        (self.next_u32() % 1000) < self.drop_permille
    }
}

impl<T: Transport> Transport for LossyTransport<T> {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        if self.should_drop() {
            return;
        }
        self.inner.send(peer, bytes);
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        self.inner.poll(out);
    }

    fn peers(&self) -> &[PeerId] {
        self.inner.peers()
    }
}

/// Encode a [`TimedCmd`] to wire bytes (v1, little-endian, fixed layout).
#[must_use]
pub fn encode_timed_cmd(t: &TimedCmd) -> Vec<u8> {
    let mut v = Vec::with_capacity(23);
    v.push(1); // version
    v.push(t.peer.0);
    v.extend_from_slice(&t.tic.to_le_bytes());
    v.extend_from_slice(&t.cmd.buttons.to_le_bytes());
    v.extend_from_slice(&t.cmd.pitch.to_le_bytes());
    v.extend_from_slice(&t.cmd.yaw.to_le_bytes());
    v.extend_from_slice(&t.cmd.roll.to_le_bytes());
    v.extend_from_slice(&t.cmd.forward_move.to_le_bytes());
    v.extend_from_slice(&t.cmd.side_move.to_le_bytes());
    v.extend_from_slice(&t.cmd.up_move.to_le_bytes());
    v.push(t.cmd.weapon_select);
    v
}

/// Append an optional post-tic checksum report to a timed command. The
/// 23-byte input prefix remains unchanged so older peers can still decode it.
#[must_use]
pub fn encode_timed_cmd_with_checksum(timed: &TimedCmd, checksum: Option<(u32, u64)>) -> Vec<u8> {
    let mut bytes = encode_timed_cmd(timed);
    if let Some((tic, value)) = checksum {
        bytes.extend_from_slice(b"CS");
        bytes.extend_from_slice(&tic.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn decode_checksum_extension(bytes: &[u8]) -> Option<(u32, u64)> {
    if bytes.len() != 37 || bytes.get(23..25)? != b"CS" {
        return None;
    }
    let tic = u32::from_le_bytes(bytes.get(25..29)?.try_into().ok()?);
    let checksum = u64::from_le_bytes(bytes.get(29..37)?.try_into().ok()?);
    Some((tic, checksum))
}

/// Decode wire bytes to [`TimedCmd`].
///
/// # Errors
///
/// Truncated / bad version.
pub fn decode_timed_cmd(bytes: &[u8]) -> Result<TimedCmd, NetError> {
    if bytes.len() < 23 || bytes[0] != 1 {
        return Err(NetError::BadPacket);
    }
    let peer = PeerId(bytes[1]);
    let tic = u32::from_le_bytes(bytes[2..6].try_into().map_err(|_| NetError::BadPacket)?);
    let buttons = u32::from_le_bytes(bytes[6..10].try_into().map_err(|_| NetError::BadPacket)?);
    let pitch = i16::from_le_bytes(bytes[10..12].try_into().map_err(|_| NetError::BadPacket)?);
    let yaw = i16::from_le_bytes(bytes[12..14].try_into().map_err(|_| NetError::BadPacket)?);
    let roll = i16::from_le_bytes(bytes[14..16].try_into().map_err(|_| NetError::BadPacket)?);
    let forward_move =
        i16::from_le_bytes(bytes[16..18].try_into().map_err(|_| NetError::BadPacket)?);
    let side_move = i16::from_le_bytes(bytes[18..20].try_into().map_err(|_| NetError::BadPacket)?);
    let up_move = i16::from_le_bytes(bytes[20..22].try_into().map_err(|_| NetError::BadPacket)?);
    let weapon_select = bytes[22];
    Ok(TimedCmd {
        tic,
        peer,
        cmd: NetTicCmd {
            buttons,
            pitch,
            yaw,
            roll,
            forward_move,
            side_move,
            up_move,
            weapon_select,
        },
    })
}

/// Netplay errors.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    /// Malformed packet.
    #[error("bad packet")]
    BadPacket,
    /// Consistency check failed.
    #[error("desync at tic {tic}")]
    Desync {
        /// Tic where checksums disagreed.
        tic: u32,
    },
    /// Lobby rejected the peer.
    #[error("rejected: {0}")]
    Reject(String),
    /// I/O failure (UDP bind/send).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl NetError {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn from_io(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl PartialEq for NetError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::BadPacket, Self::BadPacket) => true,
            (Self::Desync { tic: a }, Self::Desync { tic: b }) => a == b,
            (Self::Reject(a), Self::Reject(b)) => a == b,
            (Self::Io(a), Self::Io(b)) => a.kind() == b.kind(),
            _ => false,
        }
    }
}

impl Eq for NetError {}

/// Deterministic lockstep scheduler: advances only when every peer has a cmd for `next_tic`.
#[derive(Debug)]
pub struct LockstepSession {
    local: PeerId,
    peers: BTreeSet<PeerId>,
    /// Input delay in tics (local cmd submitted for `sim_tic + delay`).
    delay: u32,
    /// Next simulation tic to release.
    next_tic: u32,
    /// Local wall tic counter (increments on each [`Self::submit_local`]).
    local_tic: u32,
    /// `(tic, peer) → cmd`.
    buffer: BTreeMap<(u32, PeerId), NetTicCmd>,
    /// Last checksum reported per peer for a tic.
    checksums: BTreeMap<(u32, PeerId), u64>,
    /// Tics whose checksums every peer reported and agreed on.
    verified_checksums: u32,
    /// Recently sent local commands, oldest first, for
    /// [`Self::rebroadcast_recent`] after a lost packet.
    sent_local: VecDeque<(TimedCmd, Option<(u32, u64)>)>,
    /// Each peer's command in the last released frame (rollback guesses
    /// that a peer keeps doing what it did).
    last_released: BTreeMap<PeerId, NetTicCmd>,
    /// Rollback window (`0` = blocking lockstep), see
    /// [`Self::set_rollback_window`].
    rollback_window: u32,
    /// Bounded non-simulation packets received alongside lockstep commands.
    auxiliary_packets: VecDeque<(PeerId, Vec<u8>)>,
}

/// Tics a reported checksum is kept while waiting for every peer's report.
pub const CHECKSUM_WINDOW: u32 = 128;

impl LockstepSession {
    /// New session. `peers` must include `local`.
    ///
    /// `delay` is input delay in tics (`0` = submit for the current `next_tic`;
    /// web coop uses `0` to avoid a seeded tic-0 / outstanding-submit race).
    #[must_use]
    pub fn new(local: PeerId, peers: impl IntoIterator<Item = PeerId>, delay: u32) -> Self {
        let peers: BTreeSet<PeerId> = peers.into_iter().collect();
        Self {
            local,
            peers,
            delay,
            next_tic: 0,
            local_tic: 0,
            buffer: BTreeMap::new(),
            checksums: BTreeMap::new(),
            verified_checksums: 0,
            sent_local: VecDeque::new(),
            last_released: BTreeMap::new(),
            rollback_window: 0,
            auxiliary_packets: VecDeque::new(),
        }
    }

    /// Local peer id.
    #[must_use]
    pub fn local(&self) -> PeerId {
        self.local
    }

    /// Next sim tic awaiting a full command set.
    #[must_use]
    pub fn next_tic(&self) -> u32 {
        self.next_tic
    }

    /// Input delay in tics.
    #[must_use]
    pub fn delay(&self) -> u32 {
        self.delay
    }

    /// Local commands submitted but not yet released, oldest first (client
    /// prediction plays these on top of the confirmed world).
    #[must_use]
    pub fn pending_local(&self) -> Vec<NetTicCmd> {
        self.buffer
            .range((self.next_tic, PeerId(0))..)
            .filter(|((_, peer), _)| *peer == self.local)
            .map(|(_, cmd)| *cmd)
            .collect()
    }

    /// Rollback play: local commands may run up to `window` tics ahead of
    /// the next released frame. Widens the resend window to match.
    pub fn set_rollback_window(&mut self, window: u32) {
        self.rollback_window = window;
    }

    /// How many tics the newest local command runs ahead of the next frame
    /// to release (`0` = nothing pending). Rollback play stops submitting
    /// new commands once this reaches its window.
    #[must_use]
    pub fn local_lead(&self) -> u32 {
        self.local_tic
            .saturating_add(self.delay)
            .saturating_sub(self.next_tic)
    }

    /// Rollback: the frames from [`Self::next_tic`] up to the newest local
    /// command, oldest first. Commands already received are used as they
    /// are; a peer whose command has not arrived is guessed to repeat its
    /// newest earlier command (received or released). The second value per
    /// frame says whether every command in it is real.
    #[must_use]
    pub fn speculative_frames(&self) -> Vec<(BTreeMap<PeerId, NetTicCmd>, bool)> {
        let end = self.local_tic.saturating_add(self.delay);
        self.speculative_frames_with(end, |_| None)
    }

    /// [`Self::speculative_frames`] up to `end` (exclusive), for clients
    /// whose own commands are not in the session buffer (web protocol v2
    /// sends them to the server instead): `local(tic)` supplies the local
    /// seat's command for a tic not in a frame yet.
    #[must_use]
    pub fn speculative_frames_with(
        &self,
        end: u32,
        local: impl Fn(u32) -> Option<NetTicCmd>,
    ) -> Vec<(BTreeMap<PeerId, NetTicCmd>, bool)> {
        let mut latest = self.last_released.clone();
        (self.next_tic..end)
            .map(|tic| {
                let mut known = true;
                let mut frame = BTreeMap::new();
                for &p in &self.peers {
                    let real = self
                        .buffer
                        .get(&(tic, p))
                        .copied()
                        .or_else(|| (p == self.local).then(|| local(tic)).flatten());
                    let cmd = if let Some(cmd) = real {
                        latest.insert(p, cmd);
                        cmd
                    } else {
                        known = false;
                        latest.get(&p).copied().unwrap_or_default()
                    };
                    frame.insert(p, cmd);
                }
                (frame, known)
            })
            .collect()
    }

    /// Submit local input for the current local tic (stored at `local_tic + delay`).
    pub fn submit_local(&mut self, cmd: NetTicCmd) -> TimedCmd {
        let tic = self.local_tic.saturating_add(self.delay);
        self.local_tic = self.local_tic.saturating_add(1);
        let t = TimedCmd {
            tic,
            peer: self.local,
            cmd,
        };
        self.buffer.insert((t.tic, t.peer), t.cmd);
        if self.sent_local.len() >= self.resend_depth() {
            self.sent_local.pop_front();
        }
        self.sent_local.push_back((t, None));
        t
    }

    /// How many recent local commands [`Self::rebroadcast_recent`] resends.
    /// A peer still waiting on one of ours is at most `2 × delay + 1` tics
    /// behind our newest command (it blocks us `delay` tics later).
    fn resend_depth(&self) -> usize {
        // With rollback a peer can also run up to the window ahead before
        // it stalls, and we can be a window ahead of it.
        let ahead = self.delay.saturating_add(self.rollback_window);
        usize::try_from(ahead.saturating_mul(2).saturating_add(4))
            .unwrap_or(usize::MAX)
            .clamp(8, 64)
    }

    /// Resend every recently submitted local command, oldest first, the
    /// each with the checksum it was reported with ([`Self::report_checksum`]
    /// for the local peer right after [`Self::submit_local`]); `checksum`
    /// stands in for the newest one's. Call while stalled: on a lossy transport (UDP)
    /// a dropped command would otherwise leave a peer waiting forever, since
    /// [`Self::broadcast`] only ever sends the newest one.
    pub fn rebroadcast_recent<T: Transport + ?Sized>(
        &self,
        transport: &mut T,
        checksum: Option<(u32, u64)>,
    ) {
        let count = self.sent_local.len();
        for (index, (timed, sent)) in self.sent_local.iter().enumerate() {
            // Each command goes out again with the checksum it first
            // carried, so a dropped report is recovered too.
            let checksum = if index + 1 == count {
                checksum.or(*sent)
            } else {
                *sent
            };
            self.broadcast_with_checksum(transport, timed, checksum);
        }
    }

    /// Ingest a remote (or replayed) timed command.
    pub fn push_remote(&mut self, t: TimedCmd) {
        self.buffer.insert((t.tic, t.peer), t.cmd);
    }

    /// Broadcast `timed` on `transport` to every other peer.
    pub fn broadcast<T: Transport + ?Sized>(&self, transport: &mut T, timed: &TimedCmd) {
        self.broadcast_with_checksum(transport, timed, None);
    }

    /// Broadcast a command with an optional checksummed playsim snapshot.
    pub fn broadcast_with_checksum<T: Transport + ?Sized>(
        &self,
        transport: &mut T,
        timed: &TimedCmd,
        checksum: Option<(u32, u64)>,
    ) {
        let bytes = encode_timed_cmd_with_checksum(timed, checksum);
        let peers: Vec<PeerId> = transport.peers().to_vec();
        for p in peers {
            if p != self.local {
                transport.send(p, &bytes);
            }
        }
    }

    /// Broadcast a bounded presentation-only packet without placing it in
    /// deterministic tic input. Payloads are queued by remote peers on receive.
    pub fn broadcast_auxiliary<T: Transport + ?Sized>(
        &self,
        transport: &mut T,
        payload: &[u8],
    ) -> Result<(), NetError> {
        if payload.is_empty() || payload.len() > MAX_AUX_PACKET_LEN {
            return Err(NetError::BadPacket);
        }
        let mut bytes = Vec::with_capacity(AUX_PACKET_MAGIC.len() + payload.len());
        bytes.extend_from_slice(AUX_PACKET_MAGIC);
        bytes.extend_from_slice(payload);
        let peers = transport.peers().to_vec();
        for peer in peers {
            if peer != self.local {
                transport.send(peer, &bytes);
            }
        }
        Ok(())
    }

    /// Drain bounded presentation-only packets received since the prior call.
    pub fn drain_auxiliary(&mut self) -> Vec<(PeerId, Vec<u8>)> {
        self.auxiliary_packets.drain(..).collect()
    }

    /// Poll transport and decode into the buffer.
    pub fn recv<T: Transport + ?Sized>(&mut self, transport: &mut T) {
        let mut pkts = Vec::new();
        transport.poll(&mut pkts);
        for (from, bytes) in pkts {
            if let Ok(t) = decode_timed_cmd(&bytes) {
                if let Some((tic, checksum)) = decode_checksum_extension(&bytes) {
                    self.report_checksum(tic, t.peer, checksum);
                }
                // Resent commands for tics already released are stale.
                if t.tic >= self.next_tic {
                    self.push_remote(t);
                }
            } else if bytes.len() > AUX_PACKET_MAGIC.len()
                && bytes.len() <= AUX_PACKET_MAGIC.len() + MAX_AUX_PACKET_LEN
                && bytes.starts_with(AUX_PACKET_MAGIC)
                && from != self.local
                && self.peers.contains(&from)
            {
                if self.auxiliary_packets.len() == MAX_QUEUED_AUX_PACKETS {
                    self.auxiliary_packets.pop_front();
                }
                self.auxiliary_packets
                    .push_back((from, bytes[AUX_PACKET_MAGIC.len()..].to_vec()));
            }
        }
    }

    /// If every peer has a command for [`Self::next_tic`], pop and advance.
    pub fn try_advance(&mut self) -> Option<BTreeMap<PeerId, NetTicCmd>> {
        let tic = self.next_tic;
        for &p in &self.peers {
            if !self.buffer.contains_key(&(tic, p)) {
                return None;
            }
        }
        let mut frame = BTreeMap::new();
        for &p in &self.peers {
            if let Some(cmd) = self.buffer.remove(&(tic, p)) {
                frame.insert(p, cmd);
            }
        }
        for (&p, &cmd) in &frame {
            self.last_released.insert(p, cmd);
        }
        self.next_tic = self.next_tic.saturating_add(1);
        let next_tic = self.next_tic;
        // Peers report a tic's checksum alongside a later cmd, so keep a
        // window wide enough for any input delay and a few late packets.
        self.checksums
            .retain(|(tic, _), _| tic.saturating_add(CHECKSUM_WINDOW) >= next_tic);
        Some(frame)
    }

    /// Insert default cmds for any peer missing [`Self::next_tic`] (web stall recovery).
    pub fn fill_defaults_for_next_tic(&mut self) {
        let tic = self.next_tic;
        for &p in &self.peers {
            self.buffer.entry((tic, p)).or_default();
        }
    }

    /// Whether every peer already has a cmd buffered for [`Self::next_tic`].
    #[must_use]
    pub fn next_tic_ready(&self) -> bool {
        let tic = self.next_tic;
        self.peers
            .iter()
            .all(|&p| self.buffer.contains_key(&(tic, p)))
    }

    /// Record a world checksum for desync detection.
    pub fn report_checksum(&mut self, tic: u32, peer: PeerId, checksum: u64) {
        if peer == self.local
            && let Some((_, sent)) = self.sent_local.back_mut()
        {
            *sent = Some((tic, checksum));
        }
        self.checksums.insert((tic, peer), checksum);
    }

    /// Compare checksums for `tic` across all peers that reported.
    ///
    /// # Errors
    ///
    /// [`NetError::Desync`] when two peers disagree.
    pub fn check_desync(&self, tic: u32) -> Result<(), NetError> {
        let mut iter = self
            .checksums
            .iter()
            .filter(|((t, _), _)| *t == tic)
            .map(|(_, c)| *c);
        let Some(first) = iter.next() else {
            return Ok(());
        };
        for c in iter {
            if c != first {
                return Err(NetError::Desync { tic });
            }
        }
        Ok(())
    }

    /// Whether all peers reported a checksum for `tic`.
    #[must_use]
    pub fn checksums_complete(&self, tic: u32) -> bool {
        self.peers
            .iter()
            .all(|peer| self.checksums.contains_key(&(tic, *peer)))
    }

    /// Compare every tic that all peers have reported, oldest first, and
    /// forget it (a tic that disagrees is reported once). Call after each [`Self::try_advance`]:
    /// unlike [`Self::check_desync`] on the tic just released, this also
    /// catches checksums that arrived a few tics late.
    ///
    /// # Errors
    ///
    /// [`NetError::Desync`] for the first tic where two peers disagree.
    pub fn check_complete_desyncs(&mut self) -> Result<(), NetError> {
        let tics: BTreeSet<u32> = self.checksums.keys().map(|(tic, _)| *tic).collect();
        for tic in tics {
            if !self.checksums_complete(tic) {
                continue;
            }
            let result = self.check_desync(tic);
            // Forget the tic either way so one desync is reported once.
            self.checksums.retain(|(t, _), _| *t != tic);
            result?;
            self.verified_checksums = self.verified_checksums.saturating_add(1);
        }
        Ok(())
    }

    /// Tics verified by [`Self::check_complete_desyncs`] so far.
    #[must_use]
    pub fn verified_checksums(&self) -> u32 {
        self.verified_checksums
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::field_reassign_with_default
)]
mod tests {
    use super::*;

    #[test]
    fn pending_local_lists_unreleased_local_input_oldest_first() {
        let (p0, p1) = (PeerId(0), PeerId(1));
        let mut s = LockstepSession::new(p0, [p0, p1], 3);
        for t in 0..3 {
            for p in [p0, p1] {
                s.push_remote(TimedCmd {
                    tic: t,
                    peer: p,
                    cmd: NetTicCmd::default(),
                });
            }
        }
        for f in 1..=4 {
            let _ = s.submit_local(NetTicCmd {
                forward_move: f,
                ..NetTicCmd::default()
            });
        }
        // Tics 3..=6 hold the four local inputs; nothing released yet.
        let fm: Vec<i16> = s.pending_local().iter().map(|c| c.forward_move).collect();
        assert_eq!(fm, vec![0, 0, 0, 1, 2, 3, 4]);
        assert!(s.try_advance().is_some());
        assert_eq!(s.pending_local().len(), 6);
        assert_eq!(s.delay(), 3);
    }

    #[test]
    fn lockstep_does_not_advance_until_all_peers_present() {
        let p0 = PeerId(0);
        let p1 = PeerId(1);
        let mut s0 = LockstepSession::new(p0, [p0, p1], 1);
        s0.push_remote(TimedCmd {
            tic: 0,
            peer: p0,
            cmd: NetTicCmd::default(),
        });
        // Missing peer 1 for tic 0.
        assert!(s0.try_advance().is_none());
        s0.push_remote(TimedCmd {
            tic: 0,
            peer: p1,
            cmd: NetTicCmd {
                forward_move: 5,
                ..Default::default()
            },
        });
        let frame = s0.try_advance().expect("full frame");
        assert_eq!(frame[&p1].forward_move, 5);
    }

    #[test]
    fn two_peer_lockstep_advances_in_order() {
        let p0 = PeerId(0);
        let p1 = PeerId(1);
        let mut mesh = LoopbackMesh::new([p0, p1]);
        let mut s0 = LockstepSession::new(p0, [p0, p1], 1);
        let mut s1 = LockstepSession::new(p1, [p0, p1], 1);

        let c0 = NetTicCmd {
            forward_move: 10,
            ..Default::default()
        };
        let c1 = NetTicCmd {
            forward_move: 20,
            ..Default::default()
        };

        // Local tic 0 → stored at tic 1 (delay 1). Need tic 0 filled first.
        // Seed tic 0 with empty cmds for both (sim start).
        s0.push_remote(TimedCmd {
            tic: 0,
            peer: p0,
            cmd: NetTicCmd::default(),
        });
        s0.push_remote(TimedCmd {
            tic: 0,
            peer: p1,
            cmd: NetTicCmd::default(),
        });
        s1.push_remote(TimedCmd {
            tic: 0,
            peer: p0,
            cmd: NetTicCmd::default(),
        });
        s1.push_remote(TimedCmd {
            tic: 0,
            peer: p1,
            cmd: NetTicCmd::default(),
        });

        let t0 = s0.submit_local(c0);
        {
            let mut ep = mesh.endpoint_mut(p0);
            s0.broadcast(&mut ep, &t0);
        }
        let t1 = s1.submit_local(c1);
        {
            let mut ep = mesh.endpoint_mut(p1);
            s1.broadcast(&mut ep, &t1);
        }
        {
            let mut ep = mesh.endpoint_mut(p0);
            s0.recv(&mut ep);
        }
        {
            let mut ep = mesh.endpoint_mut(p1);
            s1.recv(&mut ep);
        }

        let f0 = s0.try_advance().expect("tic 0");
        let f1 = s1.try_advance().expect("tic 0");
        assert_eq!(f0, f1);
        assert_eq!(f0.get(&p0), Some(&NetTicCmd::default()));

        let f0 = s0.try_advance().expect("tic 1");
        let f1 = s1.try_advance().expect("tic 1");
        assert_eq!(f0, f1);
        assert_eq!(f0[&p0].forward_move, 10);
        assert_eq!(f0[&p1].forward_move, 20);
    }

    #[test]
    fn desync_detected() {
        let mut s = LockstepSession::new(PeerId(0), [PeerId(0), PeerId(1)], 1);
        s.report_checksum(5, PeerId(0), 111);
        s.report_checksum(5, PeerId(1), 222);
        assert!(matches!(
            s.check_desync(5),
            Err(NetError::Desync { tic: 5 })
        ));
    }

    #[test]
    fn speculative_frames_guess_missing_peers_from_their_last_command() {
        let (p0, p1) = (PeerId(0), PeerId(1));
        let mut s = LockstepSession::new(p0, [p0, p1], 1);
        let walk = |f| NetTicCmd {
            forward_move: f,
            ..NetTicCmd::default()
        };
        s.push_remote(TimedCmd {
            tic: 0,
            peer: p0,
            cmd: walk(1),
        });
        s.push_remote(TimedCmd {
            tic: 0,
            peer: p1,
            cmd: walk(9),
        });
        assert!(s.try_advance().is_some());
        // Local runs three tics ahead; peer 1 has sent only tic 1.
        for f in [2, 3, 4] {
            let _ = s.submit_local(walk(f));
        }
        s.push_remote(TimedCmd {
            tic: 1,
            peer: p1,
            cmd: walk(7),
        });
        assert_eq!(s.local_lead(), 3);
        let spec = s.speculative_frames();
        assert_eq!(spec.len(), 3, "tics 1..=3");
        assert_eq!(spec[0].0[&p0].forward_move, 2);
        assert_eq!(spec[0].0[&p1].forward_move, 7);
        assert!(spec[0].1, "tic 1 is all real");
        assert_eq!(spec[1].0[&p1].forward_move, 7, "guess: newest known");
        assert!(!spec[1].1);
        assert_eq!(spec[2].0[&p0].forward_move, 4);
    }

    #[test]
    fn speculative_frames_with_take_our_commands_from_outside_the_buffer() {
        // Web v2: our commands go to the server, not into the session.
        let (p0, p1) = (PeerId(0), PeerId(1));
        let mut s = LockstepSession::new(p1, [p0, p1], 0);
        let walk = |f| NetTicCmd {
            forward_move: f,
            ..NetTicCmd::default()
        };
        for p in [p0, p1] {
            s.push_remote(TimedCmd {
                tic: 0,
                peer: p,
                cmd: walk(1),
            });
        }
        assert!(s.try_advance().is_some());
        // The server's frame for tic 1 is here; tics 2 and 3 only exist as
        // our own sent commands.
        s.push_remote(TimedCmd {
            tic: 1,
            peer: p0,
            cmd: walk(5),
        });
        s.push_remote(TimedCmd {
            tic: 1,
            peer: p1,
            cmd: walk(6),
        });
        let ours = |tic: u32| (tic >= 2).then(|| walk(10 + i16::try_from(tic).unwrap()));
        let spec = s.speculative_frames_with(4, ours);
        assert_eq!(spec.len(), 3);
        assert!(spec[0].1);
        assert_eq!(spec[0].0[&p1].forward_move, 6, "the frame wins");
        assert_eq!(spec[1].0[&p1].forward_move, 12, "our sent command");
        assert_eq!(spec[1].0[&p0].forward_move, 5, "guess: p0 keeps going");
        assert!(!spec[1].1);
        assert_eq!(spec[2].0[&p1].forward_move, 13);
    }

    #[test]
    fn rebroadcast_recovers_a_dropped_command() {
        let (p0, p1) = (PeerId(0), PeerId(1));
        let mut mesh = LoopbackMesh::new([p0, p1]);
        let mut s0 = LockstepSession::new(p0, [p0, p1], 1);
        let mut s1 = LockstepSession::new(p1, [p0, p1], 1);
        for s in [&mut s0, &mut s1] {
            for p in [p0, p1] {
                s.push_remote(TimedCmd {
                    tic: 0,
                    peer: p,
                    cmd: NetTicCmd::default(),
                });
            }
        }
        // Peer 0's command for tic 1 is lost on the wire.
        let _lost = s0.submit_local(NetTicCmd {
            forward_move: 5,
            ..NetTicCmd::default()
        });
        let m1 = s1.submit_local(NetTicCmd::default());
        s1.broadcast(&mut mesh.endpoint_mut(p1), &m1);
        s0.recv(&mut mesh.endpoint_mut(p0));
        assert!(s0.try_advance().is_some());
        assert!(s1.try_advance().is_some());
        assert!(s1.try_advance().is_none(), "tic 1 waits on peer 0");
        s0.rebroadcast_recent(&mut mesh.endpoint_mut(p0), None);
        s1.recv(&mut mesh.endpoint_mut(p1));
        let f1 = s1.try_advance().expect("tic 1 after resend");
        assert_eq!(f1[&p0].forward_move, 5);
        // A second resend of the released tic is ignored, not buffered.
        s0.rebroadcast_recent(&mut mesh.endpoint_mut(p0), None);
        s1.recv(&mut mesh.endpoint_mut(p1));
        assert!(s1.buffer.keys().all(|(tic, _)| *tic >= s1.next_tic()));
    }

    #[test]
    fn late_checksums_are_still_compared() {
        let (p0, p1) = (PeerId(0), PeerId(1));
        let mut s = LockstepSession::new(p0, [p0, p1], 1);
        s.report_checksum(0, p0, 7);
        // Release a few tics before the remote report for tic 0 lands.
        for tic in 0..4 {
            s.buffer.insert((tic, p0), NetTicCmd::default());
            s.buffer.insert((tic, p1), NetTicCmd::default());
            assert!(s.try_advance().is_some());
            s.check_complete_desyncs().expect("nothing complete yet");
        }
        s.report_checksum(0, p1, 7);
        s.check_complete_desyncs().expect("agree");
        assert_eq!(s.verified_checksums(), 1);
        s.report_checksum(35, p0, 1);
        s.report_checksum(35, p1, 2);
        assert!(matches!(
            s.check_complete_desyncs(),
            Err(NetError::Desync { tic: 35 })
        ));
    }

    #[test]
    fn lossy_drops_some_packets() {
        let p0 = PeerId(0);
        let p1 = PeerId(1);
        let mut mesh = LoopbackMesh::new([p0, p1]);
        let mut sent = 0u32;
        let mut got = 0u32;
        for i in 0..200u32 {
            let mut ep = LossyTransport::new(mesh.endpoint_mut(p0), 500, i.wrapping_mul(17));
            let bytes = encode_timed_cmd(&TimedCmd {
                tic: i,
                peer: p0,
                cmd: NetTicCmd::default(),
            });
            ep.send(p1, &bytes);
            sent += 1;
        }
        let mut out = Vec::new();
        mesh.endpoint_mut(p1).poll(&mut out);
        got += out.len() as u32;
        assert!(got < sent);
        assert!(got > 0);
    }

    #[test]
    fn codec_roundtrip() {
        let t = TimedCmd {
            tic: 42,
            peer: PeerId(3),
            cmd: NetTicCmd {
                buttons: 7,
                pitch: -1,
                yaw: 2,
                roll: 3,
                forward_move: 4,
                side_move: 5,
                up_move: 6,
                weapon_select: 2,
            },
        };
        let bytes = encode_timed_cmd(&t);
        assert_eq!(decode_timed_cmd(&bytes).unwrap(), t);
    }

    /// Web coop uses delay=0: one submit per tic, both peers required.
    #[test]
    fn web_delay0_lockstep_applies_forward() {
        let peers = [PeerId(0), PeerId(1)];
        let mut s0 = LockstepSession::new(PeerId(0), peers, 0);
        let mut s1 = LockstepSession::new(PeerId(1), peers, 0);
        let mut pending0: Option<TimedCmd> = None;
        let mut pending1: Option<TimedCmd> = None;
        let mut moved = false;
        for _ in 0..8 {
            if pending0.is_none() {
                let mut cmd = NetTicCmd::default();
                cmd.forward_move = 50 << 8;
                let msg = s0.submit_local(cmd);
                pending0 = Some(msg);
                s1.push_remote(msg);
            }
            if pending1.is_none() {
                let msg = s1.submit_local(NetTicCmd::default());
                pending1 = Some(msg);
                s0.push_remote(msg);
            }
            let a0 = s0.try_advance();
            let _a1 = s1.try_advance();
            if a0.is_some() {
                pending0 = None;
            }
            if _a1.is_some() {
                pending1 = None;
            }
            if let Some(frame) = a0 {
                if frame
                    .get(&PeerId(0))
                    .is_some_and(|c| c.forward_move == 50 << 8)
                {
                    moved = true;
                    break;
                }
            }
        }
        assert!(
            moved,
            "player0 forward_move never applied — lockstep stalled"
        );
    }
}
