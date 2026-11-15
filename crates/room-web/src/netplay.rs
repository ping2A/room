//! Web lockstep coop over legacy FTRL (`room-ws`) or protocol v2 (`room-server`).

use crate::voice_hub;
use crate::ws_net::{PresenceEvent, WsTransport};
use crate::ws_v2::WsV2Transport;
use room_input::{InputSample, Key, build_ticcmd};
use room_net::{
    ABI_MAJOR, ENGINE_VERSION, Hello, LobbyHost, LobbyMsg, LockstepSession, NetTicCmd, PeerId,
    Ready, Start, TimedCmd, Transport, Welcome, decode_timed_cmd, validate_hello,
};
use room_sim::{Buttons, TicCmd};
use std::collections::{BTreeMap, BTreeSet};

/// Active coop socket (legacy FTRL or docs/22 v2).
pub enum CoopSocket {
    /// `room-ws` / legacy-ftrl gateway.
    Legacy(WsTransport),
    /// `/v2/rooms/{id}/ws` FTR2 + JSON control.
    V2(WsV2Transport),
}

impl CoopSocket {
    fn poll_presence(&mut self, out: &mut Vec<PresenceEvent>) {
        match self {
            Self::Legacy(t) => t.poll_presence(out),
            Self::V2(t) => t.poll_presence(out),
        }
    }

    fn is_closed(&self) -> bool {
        match self {
            Self::Legacy(t) => t.is_closed(),
            Self::V2(t) => t.is_closed(),
        }
    }

    fn try_seat(&self) -> Result<Option<PeerId>, String> {
        match self {
            Self::Legacy(t) => t.try_seat(),
            Self::V2(t) => t.try_seat(),
        }
    }

    fn set_peer_count(&mut self, peer_count: u8) {
        match self {
            Self::Legacy(t) => t.set_peer_count(peer_count),
            Self::V2(t) => t.set_peer_count(peer_count),
        }
    }

    fn data_stats(&self) -> (u32, u32) {
        match self {
            Self::Legacy(t) => t.data_stats(),
            Self::V2(t) => t.data_stats(),
        }
    }

    fn send_ready_v2(&mut self) {
        if let Self::V2(t) = self {
            t.send_ready();
        }
    }

    fn send_pose_v2(&mut self, x: i32, y: i32, angle: i16) {
        if let Self::V2(t) = self {
            t.send_pose(x, y, angle);
        }
    }

    fn send_map_outline_v2(&mut self, segs: Vec<i16>) {
        if let Self::V2(t) = self {
            t.send_map_outline(segs);
        }
    }

    fn take_welcome_v2(&mut self) -> Option<room_proto::WelcomeMsg> {
        match self {
            Self::V2(t) => t.take_welcome(),
            Self::Legacy(_) => None,
        }
    }

    fn take_start_v2(&mut self) -> Option<(u32, u32, bool, u8)> {
        match self {
            Self::V2(t) => t.take_start(),
            Self::Legacy(_) => None,
        }
    }

    fn take_reject_v2(&mut self) -> Option<String> {
        match self {
            Self::V2(t) => t.take_reject(),
            Self::Legacy(_) => None,
        }
    }

    fn send_rtc_v2(&mut self, msg: &room_proto::ControlMsg) {
        if let Self::V2(t) = self {
            t.send_rtc(msg);
        }
    }

    fn poll_rtc_v2(&mut self, out: &mut Vec<room_proto::ControlMsg>) {
        if let Self::V2(t) = self {
            t.poll_rtc(out);
        }
    }

    fn is_v2(&self) -> bool {
        matches!(self, Self::V2(_))
    }

    fn starts_in_secs(&self) -> Option<u32> {
        match self {
            Self::V2(t) => t.starts_in_secs(),
            Self::Legacy(_) => None,
        }
    }

    pub fn send_duke_input(&mut self, tic: u32, input: room_duke::DukeInput) {
        if let Self::V2(t) = self {
            t.send_duke_tic_cmd(
                tic,
                room_proto::DukeSyncInput {
                    avel: input.avel,
                    horz: input.horz,
                    fvel: input.fvel,
                    svel: input.svel,
                    bits: input.bits,
                },
            );
        }
    }

    pub fn poll_duke_frames(&mut self, out: &mut Vec<room_proto::DukeSyncFrameMsg>) {
        if let Self::V2(t) = self {
            t.poll_duke_frames(out);
        }
    }

    fn keepalive_v2(&mut self) {
        if let Self::V2(t) = self {
            t.keepalive();
        }
    }
}

/// Server tic period in milliseconds.
const TIC_MS: f64 = 1000.0 / 35.0;
/// Cap on input tics sent per call (catch-up after a hitch).
const MAX_SENDS_PER_CALL: u32 = 8;
/// Never stream input more than this many tics past the newest frame.
const MAX_LEAD_TICS: u32 = 16;
const PING_INTERVAL_MS: f64 = 500.0;

/// Tics of input lead so cmds reach the server before their slot is released.
fn lead_tics(rtt_ms: f64) -> u32 {
    ((rtt_ms + 15.0) / TIC_MS).ceil().clamp(1.0, 10.0) as u32
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

fn pump_rtc(transport: &mut CoopSocket) {
    if !crate::rtc_bridge::available() {
        return;
    }
    let mut inbound = Vec::new();
    transport.poll_rtc_v2(&mut inbound);
    for msg in inbound {
        crate::rtc_bridge::handle_inbound(&msg);
    }
    for msg in crate::rtc_bridge::take_outbound() {
        transport.send_rtc_v2(&msg);
    }
}

impl Transport for CoopSocket {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        match self {
            Self::Legacy(t) => t.send(peer, bytes),
            Self::V2(t) => t.send(peer, bytes),
        }
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        match self {
            Self::Legacy(t) => t.poll(out),
            Self::V2(t) => t.poll(out),
        }
    }

    fn peers(&self) -> &[PeerId] {
        match self {
            Self::Legacy(t) => t.peers(),
            Self::V2(t) => t.peers(),
        }
    }
}

/// URL / lobby parameters for a WebSocket coop session.
#[derive(Clone, Debug)]
pub struct NetJoinConfig {
    /// `ws://host:port/` relay URL.
    pub url: String,
    /// Expected player count (2..=8).
    pub players: u8,
    /// Display name.
    pub name: String,
    /// Load-order fingerprint (must match peers).
    pub fingerprint: u64,
    /// Start map.
    pub map: String,
    /// Skill 1..=5.
    pub skill: u8,
    /// Atomic Duke `ud.coop` mode from room welcome (0/1/2).
    pub duke_game_mode: u8,
    /// Atomic Duke friendly-fire setting from room welcome.
    pub duke_friendly_fire: bool,
    /// Doom deathmatch from room welcome.
    pub doom_deathmatch: bool,
    /// Host RNG seed (seat 0); joiners take seed from lobby Welcome/Start.
    pub rng_seed: u32,
    /// Optional minted join ticket (v2); defaults to room id from the URL.
    pub ticket: Option<String>,
    /// Join as a seatless spectator (v2 only).
    pub spectate: bool,
    /// Gameplay mod named by the server's `welcome` (v2); mounted after the IWAD.
    pub game_mod: Option<room_proto::RoomModRef>,
    /// Content packs from `welcome.room.packs` (e.g. `voxel`).
    pub packs: Vec<String>,
    /// Server library files from the room's preset (`welcome.room.content`).
    pub content: Vec<room_proto::RoomContentRef>,
    /// Preset IWAD replacing the local base game (`welcome.room.iwad`).
    pub iwad: Option<room_proto::RoomContentRef>,
    /// Match time limit in seconds from `welcome` (0 = none).
    pub time_limit: u32,
    /// Bot fill for empty seats (BE8).
    pub fill_bots: bool,
    /// First bot seat index from server `start`.
    pub bot_first_seat: u8,
}

/// Ready lockstep session + match settings (hand off to [`crate::play::PlaySession`]).
pub struct NetReady {
    /// Transport.
    pub transport: CoopSocket,
    /// Lockstep scheduler.
    pub session: LockstepSession,
    /// Local seat.
    pub local: PeerId,
    /// Peer count.
    pub peer_count: u8,
    /// Start map.
    pub map: String,
    /// Skill.
    pub skill: u8,
    /// RNG seed.
    pub rng_seed: u32,
    /// Peers that announced Ready while we were still in the lobby.
    pub early_ready: BTreeSet<PeerId>,
    /// Load-order fingerprint (host re-Welcome during Ready gate).
    pub fingerprint: u64,
    /// Peer display names from the relay.
    pub peer_names: BTreeMap<PeerId, String>,
    /// Seatless spectator (never sends input).
    pub spectator: bool,
    /// Match time limit in seconds (0 = none).
    pub time_limit: u32,
    /// Bot fill (BE8).
    pub fill_bots: bool,
    /// First bot seat from server `start`.
    pub bot_first_seat: u8,
    /// Doom deathmatch from `welcome`.
    pub deathmatch: bool,
}

/// Native Duke v2 input/frame channel. The server frame is authoritative and
/// includes every seat's input in stable seat order.
pub struct DukeNetPlay {
    transport: CoopSocket,
    pub peer_count: u8,
    /// This client's seat (its player's heading rotates local input).
    pub local_seat: usize,
    spectator: bool,
    next_send_tic: u32,
    frames: BTreeMap<u32, Vec<room_duke::DukeInput>>,
}

impl DukeNetPlay {
    #[must_use]
    pub fn from_ready(ready: NetReady) -> Self {
        Self {
            transport: ready.transport,
            peer_count: ready.peer_count,
            local_seat: usize::from(ready.local.0),
            spectator: ready.spectator,
            next_send_tic: 0,
            frames: BTreeMap::new(),
        }
    }

    /// Seat this client sees and hears: its own, or seat 0 for a spectator.
    #[must_use]
    pub fn view_seat(&self) -> usize {
        if self.spectator || self.local_seat >= usize::from(self.peer_count) {
            0
        } else {
            self.local_seat
        }
    }

    pub fn submit(&mut self, input: room_duke::DukeInput) {
        if !self.spectator {
            self.transport.send_duke_input(self.next_send_tic, input);
        }
        self.next_send_tic = self.next_send_tic.wrapping_add(1);
    }

    pub fn keepalive(&mut self) {
        self.transport.keepalive_v2();
    }

    /// Watching (no seat, nothing to send).
    #[must_use]
    pub fn is_spectator(&self) -> bool {
        self.spectator
    }

    /// Frames received and ready to run from `tic` on, without a gap.
    pub fn backlog(&mut self, tic: u32) -> usize {
        self.poll();
        let mut n = 0usize;
        while self.frames.contains_key(&tic.wrapping_add(n as u32)) {
            n += 1;
        }
        n
    }

    pub fn take_frame(&mut self, tic: u32) -> Option<Vec<room_duke::DukeInput>> {
        self.poll();
        self.frames.remove(&tic)
    }

    fn poll(&mut self) {
        let mut incoming = Vec::new();
        self.transport.poll_duke_frames(&mut incoming);
        for frame in incoming {
            let mut cmds = vec![room_duke::DukeInput::default(); usize::from(self.peer_count)];
            let mut idx = 0;
            for seat in 0..8u8 {
                if frame.seat_mask & (1 << seat) == 0 {
                    continue;
                }
                let Some(cmd) = frame.cmds.get(idx).copied() else {
                    break;
                };
                idx += 1;
                if let Some(slot) = cmds.get_mut(usize::from(seat)) {
                    *slot = room_duke::DukeInput {
                        avel: cmd.avel,
                        horz: cmd.horz,
                        fvel: cmd.fvel,
                        svel: cmd.svel,
                        bits: cmd.bits,
                    };
                }
            }
            self.frames.insert(frame.tic, cmds);
        }
    }
}

/// Connecting / lobby phases before the sim starts.
pub struct NetLobby {
    cfg: NetJoinConfig,
    transport: Option<CoopSocket>,
    local: Option<PeerId>,
    host: Option<LobbyHost>,
    welcomed: bool,
    started_seed: Option<u32>,
    /// Frames since last joiner HELLO (retry until Welcome — packets can race).
    hello_age: u32,
    /// Lockstep cmds that arrived before we leave the lobby (do not drop).
    early_cmds: Vec<TimedCmd>,
    /// Ready announcements seen before map load (host may finish first).
    early_ready: BTreeSet<PeerId>,
    /// Display names from relay PEER_JOIN (for console).
    peer_names: BTreeMap<PeerId, String>,
    /// v2: JSON `ready` sent after local map prepare (not at welcome).
    v2_ready_sent: bool,
    /// Welcome received; host should prepare map then Ready.
    v2_welcomed: bool,
}

impl NetLobby {
    /// Begin WebSocket connect + HELLO.
    pub fn begin(cfg: NetJoinConfig) -> Result<Self, String> {
        let transport = if cfg.url.contains("/v2/") {
            let ticket = cfg
                .ticket
                .clone()
                .or_else(|| room_id_from_v2_url(&cfg.url))
                .unwrap_or_else(|| cfg.url.clone());
            CoopSocket::V2(WsV2Transport::start(
                &cfg.url,
                &ticket,
                &cfg.name,
                cfg.fingerprint,
                cfg.players,
                None,
                cfg.spectate,
            )?)
        } else if cfg.spectate {
            return Err("spectating needs a room-server (/v2/rooms/…) link".into());
        } else {
            CoopSocket::Legacy(WsTransport::start(&cfg.url, &cfg.name, cfg.players)?)
        };
        Ok(Self {
            cfg,
            transport: Some(transport),
            local: None,
            host: None,
            welcomed: false,
            started_seed: None,
            hello_age: 0,
            early_cmds: Vec::new(),
            early_ready: BTreeSet::new(),
            peer_names: BTreeMap::new(),
            v2_ready_sent: false,
            v2_welcomed: false,
        })
    }

    /// True when v2 welcome arrived and we still need to prepare the map + send Ready.
    #[must_use]
    pub fn needs_map_prepare(&self) -> bool {
        self.transport.as_ref().is_some_and(CoopSocket::is_v2)
            && self.v2_welcomed
            && !self.v2_ready_sent
            && self.started_seed.is_none()
    }

    /// Send outline + v2 `ready` after the local map is loaded.
    pub fn send_v2_ready_after_prepare(&mut self, outline: Vec<i16>) {
        if self.v2_ready_sent {
            return;
        }
        if self.cfg.spectate {
            self.v2_ready_sent = true;
            return;
        }
        if let Some(transport) = self.transport.as_mut() {
            transport.send_map_outline_v2(outline);
            transport.send_ready_v2();
            self.v2_ready_sent = true;
            web_sys::console::log_1(&"room-web: v2 ready sent (map prepared)".into());
        }
    }

    /// Mod the room plays with (known after `welcome`).
    #[must_use]
    pub fn game_mod(&self) -> Option<&room_proto::RoomModRef> {
        self.cfg.game_mod.as_ref()
    }

    /// Content packs from `welcome` (e.g. `voxel`).
    #[must_use]
    pub fn packs(&self) -> &[String] {
        &self.cfg.packs
    }

    /// Preset library files from `welcome`, in mount order.
    #[must_use]
    pub fn content(&self) -> &[room_proto::RoomContentRef] {
        &self.cfg.content
    }

    /// Preset IWAD from `welcome`, if the room replaces the base game.
    #[must_use]
    pub fn preset_iwad(&self) -> Option<&room_proto::RoomContentRef> {
        self.cfg.iwad.as_ref()
    }

    /// Match time limit in seconds (known after `welcome`; 0 = none).
    #[must_use]
    pub fn time_limit(&self) -> u32 {
        self.cfg.time_limit
    }

    /// Bot fill from `welcome` / `start` (BE8).
    #[must_use]
    pub fn fill_bots(&self) -> bool {
        self.cfg.fill_bots
    }

    /// Doom deathmatch from `welcome`.
    #[must_use]
    pub fn doom_deathmatch(&self) -> bool {
        self.cfg.doom_deathmatch
    }

    /// First bot seat from server `start`.
    #[must_use]
    pub fn bot_first_seat(&self) -> u8 {
        self.cfg.bot_first_seat
    }

    /// Relay WebSocket URL (mod downloads use the same host).
    #[must_use]
    pub fn ws_url(&self) -> &str {
        &self.cfg.url
    }

    /// Match settings after welcome (for map prepare).
    #[must_use]
    pub fn prepare_opts(&self) -> Option<(String, u8, u32, u8, u8, u8, bool)> {
        let seat = self.local?.0;
        Some((
            self.cfg.map.clone(),
            self.cfg.skill,
            self.cfg.rng_seed,
            self.cfg.players,
            seat,
            self.cfg.duke_game_mode,
            self.cfg.duke_friendly_fire,
        ))
    }

    /// Roster display names for session setup (BE3).
    #[must_use]
    pub fn session_names(&self) -> Vec<(u8, String)> {
        self.peer_names
            .iter()
            .map(|(p, n)| (p.0, n.clone()))
            .collect()
    }

    /// Lobby panel fields for the waiting UI.
    #[must_use]
    pub fn lobby_panel(&self) -> LobbyPanel {
        let seat = if self.cfg.spectate {
            None
        } else {
            self.local.map(|s| s.0)
        };
        let mut seats: Vec<(u8, String, bool)> = Vec::new();
        for i in 0..self.cfg.players {
            let name = self.peer_names.get(&PeerId(i)).cloned().unwrap_or_else(|| {
                if Some(i) == seat && !self.cfg.name.is_empty() {
                    self.cfg.name.clone()
                } else {
                    format!("seat {i}")
                }
            });
            let present = self.peer_names.contains_key(&PeerId(i)) || Some(i) == seat;
            seats.push((i, name, present));
        }
        // Ensure local seat is marked present; keep roster name when set.
        if let Some(s) = seat {
            if let Some(row) = seats.iter_mut().find(|r| r.0 == s) {
                row.2 = true;
                if let Some(n) = self.peer_names.get(&PeerId(s)) {
                    if !n.is_empty() {
                        row.1 = n.clone();
                    }
                } else if !self.cfg.name.is_empty() {
                    row.1 = self.cfg.name.clone();
                }
            }
        }
        let present_n = seats.iter().filter(|r| r.2).count();
        let room_id = room_id_from_ws_url(&self.cfg.url);
        let invite_url = invite_url_from_ws(&self.cfg.url);
        LobbyPanel {
            map: self.cfg.map.clone(),
            skill: self.cfg.skill,
            max_players: self.cfg.players,
            local_seat: seat,
            local_name: self.cfg.name.clone(),
            seats,
            present: present_n,
            ready_sent: self.v2_ready_sent
                || !self.transport.as_ref().is_some_and(CoopSocket::is_v2),
            started: self.started_seed.is_some(),
            connecting: self.local.is_none(),
            invite_url,
            room_id,
            spectator: self.cfg.spectate,
            starts_in_secs: self.transport.as_ref().and_then(CoopSocket::starts_in_secs),
            fill_bots: self.cfg.fill_bots,
        }
    }

    /// Status for the HTML overlay.
    #[must_use]
    pub fn status(&self) -> String {
        let p = self.lobby_panel();
        if p.connecting {
            return format!("Connecting to {}…", self.cfg.url);
        }
        if p.started {
            return format!("Starting {}…", p.map);
        }
        if !p.ready_sent {
            return format!("Loading map {}…", p.map);
        }
        if p.spectator {
            return format!(
                "Spectating — {}/{} players · waiting for the match to start",
                p.present, p.max_players
            );
        }
        format!(
            "Seat {}/{} — {}/{} players · waiting to start {}",
            p.local_seat.unwrap_or(0),
            p.max_players,
            p.present,
            p.max_players,
            p.map
        )
    }

    /// Drive connect / lobby. `Some` when lockstep can start (consumes the socket).
    pub fn poll(&mut self) -> Result<Option<NetReady>, String> {
        if self.transport.is_none() {
            return Ok(None);
        }
        // Presence + seat probe without overlapping `&mut self` borrows.
        let (presence, closed, new_seat) = {
            let Some(transport) = self.transport.as_mut() else {
                return Ok(None);
            };
            if let CoopSocket::Legacy(t) = transport {
                voice_hub::pump(t);
            }
            let Some(transport) = self.transport.as_mut() else {
                return Ok(None);
            };
            let mut evs = Vec::new();
            transport.poll_presence(&mut evs);
            let closed = transport.is_closed();
            let new_seat = if self.local.is_none() {
                transport.try_seat()?
            } else {
                None
            };
            (evs, closed, new_seat)
        };
        for ev in presence {
            self.on_presence(ev);
        }
        if closed {
            return Err("WebSocket closed (relay disconnected)".into());
        }
        if let Some(transport) = self.transport.as_mut() {
            if let Some(err) = transport.take_reject_v2() {
                return Err(format!("v2 rejected: {err}"));
            }
            if let Some(w) = transport.take_welcome_v2() {
                self.local = Some(PeerId(w.seat));
                self.welcomed = true;
                self.v2_welcomed = true;
                self.cfg.rng_seed = w.room.rng_seed;
                self.cfg.map = w.room.map;
                self.cfg.skill = w.room.skill;
                self.cfg.players = w.room.max_players;
                self.cfg.duke_game_mode = w.room.duke_game_mode;
                self.cfg.duke_friendly_fire = w.room.duke_friendly_fire;
                self.cfg.doom_deathmatch = w.room.doom_deathmatch;
                self.cfg.game_mod = w.room.game_mod.clone();
                self.cfg.packs = w.room.packs.clone();
                self.cfg.content = w.room.content.clone();
                self.cfg.iwad = w.room.iwad.clone();
                self.cfg.time_limit = w.room.time_limit;
                self.cfg.fill_bots = w.room.fill_bots;
                transport.set_peer_count(w.room.max_players);
                for seat in &w.roster {
                    self.peer_names.insert(PeerId(seat.seat), seat.name.clone());
                }
                // Prefer server-sanitized callsign from roster / ticket over URL.
                if let Some(mine) = w.roster.iter().find(|s| s.seat == w.seat) {
                    if !mine.name.is_empty() {
                        self.cfg.name = mine.name.clone();
                    }
                }
                self.peer_names
                    .entry(PeerId(w.seat))
                    .or_insert_with(|| self.cfg.name.clone());
                web_sys::console::log_1(
                    &format!(
                        "room-web: v2 welcome seat={}/{} name={:?} map={} seed={} ice={}",
                        w.seat,
                        w.room.max_players,
                        self.cfg.name,
                        self.cfg.map,
                        self.cfg.rng_seed,
                        crate::rtc_bridge::ice_summary(&w.media.ice_servers)
                    )
                    .into(),
                );
                if w.media.enabled && crate::rtc_bridge::available() {
                    crate::rtc_bridge::start_from_media(&w.media, w.seat);
                }
                // Ready is sent only after the local map is prepared (see lib.rs lobby pump).
            }
            if let CoopSocket::V2(t) = transport {
                t.keepalive();
                if let Some((room, roster)) = t.take_watching() {
                    self.local = Some(PeerId(0));
                    self.welcomed = true;
                    self.v2_welcomed = true;
                    self.cfg.rng_seed = room.rng_seed;
                    self.cfg.map = room.map;
                    self.cfg.skill = room.skill;
                    self.cfg.players = room.max_players;
                    // Same game rules as the players, or the replay drifts
                    // (bots, deathmatch scoring, Duke modes).
                    self.cfg.duke_game_mode = room.duke_game_mode;
                    self.cfg.duke_friendly_fire = room.duke_friendly_fire;
                    self.cfg.doom_deathmatch = room.doom_deathmatch;
                    self.cfg.fill_bots = room.fill_bots;
                    self.cfg.game_mod = room.game_mod.clone();
                    self.cfg.packs = room.packs.clone();
                    self.cfg.content = room.content.clone();
                    self.cfg.iwad = room.iwad.clone();
                    self.cfg.time_limit = room.time_limit;
                    t.set_peer_count(room.max_players);
                    for seat in roster {
                        self.peer_names.insert(PeerId(seat.seat), seat.name);
                    }
                    web_sys::console::log_1(
                        &format!(
                            "room-web: spectating {} ({} seats)",
                            self.cfg.map, self.cfg.players
                        )
                        .into(),
                    );
                }
            }
            pump_rtc(transport);
            if let Some((tic0, seed, fill_bots, bot_first)) = transport.take_start_v2() {
                web_sys::console::log_1(
                    &format!(
                        "room-web: v2 Start tic0={tic0} seed={seed} fill_bots={fill_bots} bot_first={bot_first}"
                    )
                    .into(),
                );
                self.cfg.fill_bots = fill_bots;
                self.cfg.bot_first_seat = bot_first;
                self.started_seed = Some(seed);
            }
        }

        if let Some(seat) = new_seat {
            if seat.0 >= self.cfg.players {
                return Err(format!(
                    "relay assigned seat {} but this match only has {} seats — restart room-ws",
                    seat.0, self.cfg.players
                ));
            }
            web_sys::console::log_1(
                &format!("room-web: relay seat {} / {}", seat.0, self.cfg.players).into(),
            );
            self.local = Some(seat);
            let is_v2 = self.transport.as_ref().is_some_and(CoopSocket::is_v2);
            if seat.0 == 0 && !is_v2 {
                self.host = Some(LobbyHost::new(
                    self.cfg.fingerprint,
                    self.cfg.players,
                    1,
                    self.cfg.rng_seed,
                    self.cfg.map.clone(),
                    self.cfg.skill,
                ));
                self.welcomed = true;
            }
        }
        if self.local.is_none() {
            return Ok(None);
        }

        let Some(transport) = self.transport.as_mut() else {
            return Ok(None);
        };
        let local = self.local.unwrap_or(PeerId(0));
        let v2 = transport.is_v2();

        // Joiners: re-send HELLO until Welcome (desktop TCP client does the same).
        if !v2 && !self.welcomed && local.0 != 0 {
            if self.hello_age % 15 == 0 {
                let hello = LobbyMsg::Hello(Hello {
                    engine_version: ENGINE_VERSION.into(),
                    abi_major: ABI_MAJOR,
                    fingerprint: self.cfg.fingerprint,
                    name: self.cfg.name.clone(),
                });
                uplink(transport, local, &hello.encode());
            }
            self.hello_age = self.hello_age.saturating_add(1);
        }

        let mut raw = Vec::new();
        if !v2 {
            transport.poll(&mut raw);
        }
        for (from, bytes) in raw {
            match LobbyMsg::decode(&bytes) {
                Ok(LobbyMsg::Hello(hello)) if local.0 == 0 => {
                    if let Some(h) = self.host.as_mut() {
                        let reply = h.on_hello_at(from, hello);
                        // Relay fans out; clients filter Welcome by peer_id.
                        uplink(transport, local, &reply.encode());
                        if let LobbyMsg::Welcome(w) = &reply {
                            web_sys::console::log_1(
                                &format!(
                                    "room-web: welcomed peer {} ({}/{})",
                                    w.peer_id.0,
                                    h.seats().len(),
                                    self.cfg.players
                                )
                                .into(),
                            );
                        } else if let LobbyMsg::Reject(r) = &reply {
                            web_sys::console::warn_1(
                                &format!("room-web: rejected peer {}: {}", from.0, r.reason).into(),
                            );
                        }
                    }
                }
                Ok(LobbyMsg::Welcome(w)) if w.peer_id == local => {
                    web_sys::console::log_1(
                        &format!(
                            "room-web: lobby welcome peer {}/{} map={} seed={}",
                            w.peer_id.0, w.peer_count, w.map, w.rng_seed
                        )
                        .into(),
                    );
                    self.welcomed = true;
                    transport.set_peer_count(w.peer_count);
                    self.cfg.rng_seed = w.rng_seed;
                    self.cfg.map = w.map;
                    self.cfg.skill = w.skill;
                    self.cfg.players = w.peer_count;
                }
                Ok(LobbyMsg::Reject(r)) => {
                    // Reject has no target seat; ignore after we already joined,
                    // and never treat peer-retry noise as fatal for the host.
                    if self.welcomed || local.0 == 0 {
                        web_sys::console::warn_1(
                            &format!("room-web: ignore lobby reject: {}", r.reason).into(),
                        );
                    } else {
                        return Err(format!("lobby rejected: {}", r.reason));
                    }
                }
                Ok(LobbyMsg::Start(s)) => {
                    web_sys::console::log_1(
                        &format!("room-web: lobby Start seed={}", s.rng_seed).into(),
                    );
                    self.started_seed = Some(s.rng_seed);
                }
                Ok(LobbyMsg::Ready(r)) => {
                    web_sys::console::log_1(
                        &format!("room-web: lobby saw Ready seat={}", r.peer_id.0).into(),
                    );
                    self.early_ready.insert(r.peer_id);
                }
                Ok(_) => {}
                Err(_) => {
                    // Host may already be in lockstep while we finish lobby/Start —
                    // keep TimedCmds for the session buffer instead of dropping them.
                    match decode_timed_cmd(&bytes) {
                        Ok(t) => {
                            web_sys::console::log_1(
                                &format!(
                                    "room-web: lobby stash timed cmd tic={} peer={}",
                                    t.tic, t.peer.0
                                )
                                .into(),
                            );
                            self.early_cmds.push(t);
                        }
                        Err(_) => {
                            web_sys::console::warn_1(
                                &format!(
                                    "room-web: lobby ignore {} bytes from seat {}",
                                    bytes.len(),
                                    from.0
                                )
                                .into(),
                            );
                        }
                    }
                }
            }
        }

        if !v2 && local.0 == 0 {
            if let Some(h) = self.host.as_ref() {
                if h.seats().len() >= usize::from(self.cfg.players) && self.started_seed.is_none() {
                    uplink(transport, local, &h.start_msg().encode());
                    self.started_seed = Some(self.cfg.rng_seed);
                }
            }
        }

        if let Some(seed) = self.started_seed {
            if !self.welcomed && local.0 != 0 {
                return Ok(None);
            }
            let peer_count = self.cfg.players;
            let map = self.cfg.map.clone();
            let skill = self.cfg.skill;
            let Some(mut transport) = self.transport.take() else {
                return Ok(None);
            };
            transport.set_peer_count(peer_count);
            let peers: Vec<PeerId> = (0..peer_count).map(PeerId).collect();
            // delay=0: each submit is for `next_tic` (no seeded tic-0). That matches
            // one-outstanding-cmd lockstep and avoids stalling web movement.
            let mut session = LockstepSession::new(local, peers, 0);
            let early_cmds = std::mem::take(&mut self.early_cmds);
            for t in early_cmds.iter().copied() {
                // v2 frames carry the server's authoritative copy of our cmd too.
                if v2 || t.peer != local {
                    session.push_remote(t);
                }
            }
            web_sys::console::log_1(
                &format!(
                    "room-web: lockstep start seat={} peers={} seed={seed} delay=0 early={}",
                    local.0,
                    peer_count,
                    early_cmds.len()
                )
                .into(),
            );
            return Ok(Some(NetReady {
                transport,
                session,
                local,
                peer_count,
                map,
                skill,
                rng_seed: seed,
                early_ready: std::mem::take(&mut self.early_ready),
                fingerprint: self.cfg.fingerprint,
                peer_names: std::mem::take(&mut self.peer_names),
                spectator: self.cfg.spectate,
                time_limit: self.cfg.time_limit,
                fill_bots: self.cfg.fill_bots,
                bot_first_seat: self.cfg.bot_first_seat,
                deathmatch: self.cfg.doom_deathmatch,
            }));
        }
        Ok(None)
    }

    fn on_presence(&mut self, ev: PresenceEvent) {
        match ev {
            PresenceEvent::PeerJoin { seat, name } => {
                self.peer_names.insert(seat, name.clone());
                web_sys::console::log_1(
                    &format!(
                        "room-web: lobby peer online seat={} name={name:?} (known={})",
                        seat.0,
                        self.peer_names.len()
                    )
                    .into(),
                );
            }
            PresenceEvent::PeerLeave { seat } => {
                let name = self.peer_names.remove(&seat).unwrap_or_else(|| "?".into());
                web_sys::console::warn_1(
                    &format!(
                        "room-web: lobby peer offline seat={} name={name:?} (known={})",
                        seat.0,
                        self.peer_names.len()
                    )
                    .into(),
                );
            }
            PresenceEvent::SocketClose { code, reason } => {
                web_sys::console::warn_1(
                    &format!("room-web: lobby socket closed code={code} reason={reason:?}").into(),
                );
            }
            PresenceEvent::Welcome { seat, peer_count } => {
                web_sys::console::log_1(
                    &format!(
                        "room-web: lobby relay welcome seat={}/{}",
                        seat.0, peer_count
                    )
                    .into(),
                );
            }
            PresenceEvent::SocketOpen | PresenceEvent::SocketError => {}
        }
    }
}

/// Snapshot for the waiting-room overlay.
#[derive(Clone, Debug)]
pub struct LobbyPanel {
    /// Map lump.
    pub map: String,
    /// Skill.
    pub skill: u8,
    /// Seat capacity.
    pub max_players: u8,
    /// Local seat if known.
    pub local_seat: Option<u8>,
    /// Local display name.
    pub local_name: String,
    /// `(seat, name, present)`.
    pub seats: Vec<(u8, String, bool)>,
    /// Present seat count.
    pub present: usize,
    /// Local Ready sent.
    pub ready_sent: bool,
    /// Server Start received.
    pub started: bool,
    /// Still connecting.
    pub connecting: bool,
    /// Lobby invite URL for a friend (`/?join=<room>`), when known.
    pub invite_url: Option<String>,
    /// Room id extracted from the WebSocket URL.
    pub room_id: Option<String>,
    /// Watching without a seat.
    pub spectator: bool,
    /// Seconds until the server starts the match on its timer.
    pub starts_in_secs: Option<u32>,
    /// Empty slots become bots at the start.
    pub fill_bots: bool,
}

/// Extract `room_id` from a v2 WebSocket URL (`…/v2/rooms/{id}/ws`).
#[must_use]
pub fn room_id_from_ws_url(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url);
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    while let Some(p) = parts.next() {
        if p == "rooms" {
            let id = parts.next()?;
            if id.is_empty() || id == "ws" {
                return None;
            }
            return Some(id.to_owned());
        }
    }
    None
}

/// Public lobby invite that mints a fresh ticket when opened (`http(s)://host/?join=id`).
#[must_use]
pub fn invite_url_from_ws(ws_url: &str) -> Option<String> {
    let room_id = room_id_from_ws_url(ws_url)?;
    let after_scheme = ws_url.split("://").nth(1)?;
    let host = after_scheme.split('/').next().unwrap_or("");
    if host.is_empty() {
        return None;
    }
    let proto = if ws_url.starts_with("wss:") {
        "https:"
    } else {
        "http:"
    };
    Some(format!("{proto}//{host}/?join={room_id}"))
}

#[cfg(test)]
mod invite_url_tests {
    use super::{invite_url_from_ws, room_id_from_ws_url};

    #[test]
    fn parses_v2_room_id() {
        assert_eq!(
            room_id_from_ws_url("ws://127.0.0.1:8765/v2/rooms/abc123/ws"),
            Some("abc123".into())
        );
        assert_eq!(
            invite_url_from_ws("wss://game.example/v2/rooms/deadbeef/ws"),
            Some("https://game.example/?join=deadbeef".into())
        );
    }
}

/// Match settings retained so the host can re-Welcome a joiner that missed Start.
struct HostResume {
    fingerprint: u64,
    map: String,
    skill: u8,
    rng_seed: u32,
}

/// Live lockstep driver (after lobby).
pub struct NetPlay {
    transport: CoopSocket,
    session: LockstepSession,
    /// Peer count.
    pub peer_count: u8,
    /// Host-only resume info for late HELLO during the Ready gate.
    host_resume: Option<HostResume>,
    /// Seats that finished map load (`LobbyMsg::Ready`).
    ready_peers: BTreeSet<PeerId>,
    sent_ready: bool,
    frames_since_ready_bcast: u32,
    /// Submitted cmd awaiting a full frame (rebroadcast until advance).
    pending: Option<TimedCmd>,
    frames_since_rebroadcast: u32,
    /// Frames spent waiting on remotes for the current pending tic.
    frames_waiting: u32,
    /// Seats that left the relay mid-match.
    gone_peers: BTreeSet<PeerId>,
    peer_names: BTreeMap<PeerId, String>,
    /// Last tic we logged an advance for (throttle console spam).
    last_logged_tic: Option<u32>,
    socket_dead: bool,
    /// Frames since last admin pose telemetry.
    pose_age: u32,
    /// v2: next tic to stamp local input for.
    next_send_tic: u32,
    last_ping_ms: f64,
    /// v2: look deltas sent but not yet simulated `(tic, yaw, pitch)`.
    sent_look: std::collections::VecDeque<(u32, i16, i16)>,
    /// v2: every command sent but not yet simulated, for client prediction.
    sent_cmds: std::collections::VecDeque<(u32, NetTicCmd)>,
    /// Seatless watcher: never uplinks input.
    spectator: bool,
    /// Join / leave lines for the HUD event feed.
    feed: Vec<String>,
    /// v2: server-clock estimate, input horizon and playout buffer.
    clock: crate::net_clock::NetClock,
}

impl NetPlay {
    /// Build from a finished lobby.
    #[must_use]
    pub fn from_ready(r: NetReady) -> Self {
        let host_resume = (r.local.0 == 0).then(|| HostResume {
            fingerprint: r.fingerprint,
            map: r.map.clone(),
            skill: r.skill,
            rng_seed: r.rng_seed,
        });
        Self {
            transport: r.transport,
            session: r.session,
            peer_count: r.peer_count,
            host_resume,
            ready_peers: r.early_ready,
            sent_ready: false,
            frames_since_ready_bcast: 0,
            pending: None,
            frames_since_rebroadcast: 0,
            frames_waiting: 0,
            gone_peers: BTreeSet::new(),
            peer_names: r.peer_names,
            last_logged_tic: None,
            socket_dead: false,
            pose_age: 0,
            next_send_tic: 0,
            last_ping_ms: f64::NEG_INFINITY,
            sent_look: std::collections::VecDeque::new(),
            sent_cmds: std::collections::VecDeque::new(),
            spectator: r.spectator,
            feed: Vec::new(),
            clock: crate::net_clock::NetClock::new(),
        }
    }

    /// Watching without a seat.
    #[must_use]
    pub fn is_spectator(&self) -> bool {
        self.spectator
    }

    /// Display name for `seat` (roster), if known.
    #[must_use]
    pub fn peer_name(&self, seat: u8) -> Option<&str> {
        self.peer_names.get(&PeerId(seat)).map(String::as_str)
    }

    /// Roster callsigns for session setup (`(seat, name)`).
    #[must_use]
    pub fn session_names(&self) -> Vec<(u8, String)> {
        self.peer_names
            .iter()
            .map(|(p, n)| (p.0, n.clone()))
            .collect()
    }

    /// Seat disconnected mid-match.
    #[must_use]
    pub fn is_gone(&self, seat: u8) -> bool {
        self.gone_peers.contains(&PeerId(seat))
    }

    /// Drain join / leave lines (and server notices) for the event feed.
    pub fn take_feed(&mut self) -> Vec<String> {
        if let CoopSocket::V2(t) = &mut self.transport {
            self.feed.extend(t.take_notices());
        }
        std::mem::take(&mut self.feed)
    }

    /// Report the match result to the server (seats only, v2 only).
    pub fn send_result(&mut self, msg: &room_proto::ControlMsg) {
        if self.spectator {
            return;
        }
        if let CoopSocket::V2(t) = &mut self.transport {
            t.send_result(msg);
        }
    }

    /// Why the server closed the room (`bye` reason), if it did.
    #[must_use]
    pub fn closed_reason(&self) -> Option<String> {
        match &self.transport {
            CoopSocket::V2(t) => t.bye().map(|(_, reason)| reason),
            CoopSocket::Legacy(_) => None,
        }
    }

    /// Send pose to the server for the admin minimap (throttled).
    pub fn maybe_send_pose(&mut self, x: i32, y: i32, angle: i16) {
        self.pose_age = self.pose_age.saturating_add(1);
        if self.pose_age < 12 {
            return;
        }
        self.pose_age = 0;
        self.transport.send_pose_v2(x, y, angle);
    }

    /// Submit local input once, then try to release that lockstep tic.
    ///
    /// Peers gate on [`LobbyMsg::Ready`] after map load so the host does not
    /// advance (or fill defaults) while a joiner is still loading — that desync
    /// made remotes look idle and hid hitscan impacts.
    ///
    /// The submitted cmd is **immutable** until the tic advances.
    pub fn try_advance(
        &mut self,
        held: &BTreeSet<&'static str>,
        attack: bool,
        weapon: &mut u8,
        weapon_cycle: &mut i32,
        mouse_yaw: &mut i32,
        mouse_pitch: &mut i32,
    ) -> Option<Vec<TicCmd>> {
        if let CoopSocket::Legacy(t) = &mut self.transport {
            voice_hub::pump(t);
        }
        pump_rtc(&mut self.transport);
        self.drain_presence();
        self.poll_inbox();

        if self.socket_dead || self.transport.is_closed() {
            if !self.socket_dead {
                self.socket_dead = true;
                web_sys::console::error_1(
                    &"room-web: lockstep aborted — local WebSocket closed".into(),
                );
            }
            return None;
        }

        if self.transport.is_v2() {
            return self.advance_v2(held, attack, weapon, weapon_cycle, mouse_yaw, mouse_pitch);
        }
        if !self.lockstep_peers_ready() {
            self.ensure_ready_announced();
            return None;
        }
        if self.frames_waiting == 0 && self.pending.is_none() && self.session.next_tic() == 0 {
            let ready: Vec<u8> = self.ready_peers.iter().map(|p| p.0).collect();
            web_sys::console::log_1(
                &format!(
                    "room-web: all {} peers Ready {ready:?} — lockstep tic 0",
                    self.peer_count
                )
                .into(),
            );
        }

        if self.pending.is_none() {
            // Consume look/weapon only when submitting — keep deltas that arrive
            // while waiting for the peer so the next tic still feels them.
            let sample = sample_input(
                held,
                attack,
                *weapon,
                weapon_cycle,
                *mouse_yaw,
                *mouse_pitch,
            );
            *mouse_yaw = 0;
            *mouse_pitch = 0;
            *weapon = 0;
            let bits = build_ticcmd(&sample);
            let local_cmd = NetTicCmd {
                buttons: bits.buttons,
                pitch: bits.pitch,
                yaw: bits.yaw,
                roll: bits.roll,
                forward_move: bits.forward_move,
                side_move: bits.side_move,
                up_move: bits.up_move,
                weapon_select: bits.weapon_select,
            };
            let msg = self.session.submit_local(local_cmd);
            // v2: one TIC_CMD to the server (peer fan-out is server-side).
            // Always uplink even if `peers()` is empty/mis-sized.
            if self.transport.is_v2() {
                uplink(
                    &mut self.transport,
                    self.session.local(),
                    &room_net::encode_timed_cmd(&msg),
                );
            } else {
                self.session.broadcast(&mut self.transport, &msg);
            }
            self.pending = Some(msg);
            self.frames_since_rebroadcast = 0;
            self.frames_waiting = 0;
        }

        // Always poll again after submit — packet may already be queued.
        self.poll_inbox();

        self.frames_since_rebroadcast = self.frames_since_rebroadcast.saturating_add(1);
        if self.frames_since_rebroadcast >= 2 {
            if let Some(msg) = &self.pending {
                // Same bytes every time (deterministic lockstep).
                if self.transport.is_v2() {
                    uplink(
                        &mut self.transport,
                        self.session.local(),
                        &room_net::encode_timed_cmd(msg),
                    );
                } else {
                    self.session.broadcast(&mut self.transport, msg);
                }
            }
            self.frames_since_rebroadcast = 0;
        }

        if let Some(frame) = self.session.try_advance() {
            let tic = self.session.next_tic().saturating_sub(1);
            self.pending = None;
            self.frames_waiting = 0;
            self.log_advance(tic, &frame);
            return Some(frame_to_cmds(frame, self.peer_count));
        }

        self.frames_waiting = self.frames_waiting.saturating_add(1);
        // Peer left → fill soon. Otherwise only after a long stall.
        let fill_after = if self.gone_peers.is_empty() {
            30 * 60
        } else {
            45
        };
        if self.frames_waiting == 90 && self.gone_peers.is_empty() {
            let (rx, tx) = self.transport.data_stats();
            web_sys::console::warn_1(
                &format!(
                    "room-web: lockstep waiting tic={} pend={} frames={} data_rx={} data_tx={}",
                    self.session.next_tic(),
                    self.pending.is_some(),
                    self.frames_waiting,
                    rx,
                    tx
                )
                .into(),
            );
        }
        if self.frames_waiting >= fill_after {
            self.session.fill_defaults_for_next_tic();
            web_sys::console::warn_1(
                &format!(
                    "room-web: lockstep stall fill at tic {} (gone={:?} silent)",
                    self.session.next_tic(),
                    self.gone_peers.iter().map(|p| p.0).collect::<Vec<_>>()
                )
                .into(),
            );
            if let Some(frame) = self.session.try_advance() {
                let tic = self.session.next_tic().saturating_sub(1);
                self.pending = None;
                self.frames_waiting = 0;
                self.log_advance(tic, &frame);
                return Some(frame_to_cmds(frame, self.peer_count));
            }
        }
        None
    }

    /// v2 pipeline (fallback / spectators): send input, then simulate the next
    /// frame if it is here.
    fn advance_v2(
        &mut self,
        held: &BTreeSet<&'static str>,
        attack: bool,
        weapon: &mut u8,
        weapon_cycle: &mut i32,
        mouse_yaw: &mut i32,
        mouse_pitch: &mut i32,
    ) -> Option<Vec<TicCmd>> {
        if !self.pump_v2(now_ms(), held, attack, weapon, weapon_cycle, mouse_yaw, mouse_pitch) {
            return None;
        }
        let cmds = self.take_v2_frame();
        if cmds.is_none() {
            self.frames_waiting = self.frames_waiting.saturating_add(1);
        }
        cmds
    }

    /// v2: network housekeeping + stream local input up to the input horizon.
    ///
    /// With the clock running (`net_clock`), commands go out one per tic of
    /// local time, `rtt + margin` ahead of the server's clock, whatever the
    /// frame arrival pattern; before the first frame, `lead` tics ahead of the
    /// newest frame as before. Never blocks.
    #[allow(clippy::too_many_arguments)]
    pub fn pump_v2(
        &mut self,
        now: f64,
        held: &BTreeSet<&'static str>,
        attack: bool,
        weapon: &mut u8,
        weapon_cycle: &mut i32,
        mouse_yaw: &mut i32,
        mouse_pitch: &mut i32,
    ) -> bool {
        pump_rtc(&mut self.transport);
        self.drain_presence();
        if self.socket_dead || self.transport.is_closed() {
            if !self.socket_dead {
                self.socket_dead = true;
                web_sys::console::error_1(
                    &"room-web: lockstep aborted — local WebSocket closed".into(),
                );
            }
            return false;
        }
        let CoopSocket::V2(t) = &mut self.transport else {
            return false;
        };
        if now - self.last_ping_ms >= PING_INTERVAL_MS {
            t.send_ping();
            self.last_ping_ms = now;
        }
        for (tic, at) in t.take_frame_times() {
            self.clock.on_frame(tic, at);
        }
        if let Some(rtt) = t.rtt_ms() {
            self.clock.set_rtt(rtt);
        }
        self.clock.decay(now);
        let first_unreceived = t.last_frame_tic().map_or(0, |x| x.saturating_add(1));
        self.next_send_tic = self.next_send_tic.max(first_unreceived);
        let target = if self.spectator {
            self.next_send_tic
        } else if let Some(h) = self.clock.input_horizon(now) {
            // Never fall behind what the server already released.
            (h.floor() as u32).saturating_add(1).max(first_unreceived)
        } else {
            first_unreceived.saturating_add(lead_tics(t.rtt_ms().unwrap_or(100.0)))
        };
        // Cap a burst (tab was hidden, clock jump): the rest goes next frame.
        let target = target.min(first_unreceived.saturating_add(MAX_LEAD_TICS));
        let mut sent = 0;
        while self.next_send_tic < target && sent < MAX_SENDS_PER_CALL {
            let sample = sample_input(
                held,
                attack,
                *weapon,
                weapon_cycle,
                *mouse_yaw,
                *mouse_pitch,
            );
            *mouse_yaw = 0;
            *mouse_pitch = 0;
            *weapon = 0;
            let bits = build_ticcmd(&sample);
            let cmd = NetTicCmd {
                buttons: bits.buttons,
                pitch: bits.pitch,
                yaw: bits.yaw,
                roll: bits.roll,
                forward_move: bits.forward_move,
                side_move: bits.side_move,
                up_move: bits.up_move,
                weapon_select: bits.weapon_select,
            };
            t.send_tic_cmd(self.next_send_tic, &cmd);
            if !self.spectator {
                if self.sent_cmds.len() >= 128 {
                    self.sent_cmds.pop_front();
                }
                self.sent_cmds.push_back((self.next_send_tic, cmd));
            }
            if cmd.yaw != 0 || cmd.pitch != 0 {
                if self.sent_look.len() >= 64 {
                    self.sent_look.pop_front();
                }
                self.sent_look
                    .push_back((self.next_send_tic, cmd.yaw, cmd.pitch));
            }
            self.next_send_tic = self.next_send_tic.saturating_add(1);
            sent += 1;
        }
        true
    }

    /// v2: the next frame's commands if it has arrived. Detects our own
    /// commands that reached the server too late (it relayed a stand-in) and
    /// widens the input margin.
    pub fn take_v2_frame(&mut self) -> Option<Vec<TicCmd>> {
        self.poll_inbox();
        let tic = self.session.next_tic();
        let frame = self.session.try_advance()?;
        self.frames_waiting = 0;
        if !self.spectator {
            let local = self.session.local();
            let sent = self
                .sent_cmds
                .iter()
                .find(|(t, _)| *t == tic)
                .map(|(_, c)| *c);
            if let (Some(sent), Some(relayed)) = (sent, frame.get(&local)) {
                if sent != *relayed {
                    self.clock.on_late_input(now_ms());
                }
            }
        }
        let next = self.session.next_tic();
        while self.sent_look.front().is_some_and(|(t, _, _)| *t < next) {
            self.sent_look.pop_front();
        }
        while self.sent_cmds.front().is_some_and(|(t, _)| *t < next) {
            self.sent_cmds.pop_front();
        }
        self.log_advance(tic, &frame);
        Some(frame_to_cmds(frame, self.peer_count))
    }

    /// v2: fractional tic to display now (playout clock), once the clock has
    /// seen a frame. `None` → pace by arrival (old behaviour).
    #[must_use]
    pub fn playout_pos(&self, now: f64) -> Option<f64> {
        if !self.transport.is_v2() {
            return None;
        }
        self.clock.playout_pos(now)
    }

    /// v2: fraction of the current input tic elapsed (`0..1`), used to glide
    /// the predicted view between commands. `1.0` without a clock.
    #[must_use]
    pub fn input_frac(&self, now: f64) -> f64 {
        self.clock
            .input_horizon(now)
            .map_or(1.0, |h| h - h.floor())
    }

    /// The playout wanted a frame that has not arrived.
    pub fn on_stall(&mut self, now: f64) {
        self.clock.on_stall(now);
    }

    /// Next tic the sim will run.
    #[must_use]
    pub fn next_tic(&self) -> u32 {
        self.session.next_tic()
    }

    /// `(input margin ms, playout buffer ms, late inputs, stalls)` for the
    /// debug overlay.
    #[must_use]
    pub fn clock_stats(&self) -> (f64, f64, u32, u32) {
        (
            self.clock.input_margin_ms(),
            self.clock.buffer_ms(),
            self.clock.late_inputs,
            self.clock.stalls,
        )
    }

    /// v2: frames received but not yet simulated.
    #[must_use]
    pub fn backlog(&self) -> u32 {
        match &self.transport {
            CoopSocket::V2(t) => t.last_frame_tic().map_or(0, |x| {
                x.saturating_add(1).saturating_sub(self.session.next_tic())
            }),
            CoopSocket::Legacy(_) => 0,
        }
    }

    /// Look deltas already sent but not yet applied by the sim (angle units),
    /// so the camera can preview them instead of lagging by the input lead.
    #[must_use]
    pub fn unapplied_look(&self) -> (i32, i32) {
        let next = self.session.next_tic();
        self.sent_look
            .iter()
            .filter(|(t, _, _)| *t >= next)
            .fold((0, 0), |(y, p), (_, dy, dp)| {
                (y + i32::from(*dy), p + i32::from(*dp))
            })
    }

    /// Local commands sent but not yet simulated, oldest first: what client
    /// prediction plays on top of the last confirmed tic (`docs/38`).
    #[must_use]
    pub fn unapplied_cmds(&self) -> Vec<TicCmd> {
        if self.spectator {
            return Vec::new();
        }
        let next = self.session.next_tic();
        let raw: Vec<NetTicCmd> = match &self.transport {
            CoopSocket::V2(_) => self
                .sent_cmds
                .iter()
                .filter(|(t, _)| *t >= next)
                .map(|(_, c)| *c)
                .collect(),
            CoopSocket::Legacy(_) => self.session.pending_local(),
        };
        raw.into_iter().map(net_cmd_to_tic).collect()
    }

    /// Ping (ms) of `seat` as the server last shared it (v2 rooms).
    #[must_use]
    pub fn seat_ping(&self, seat: u8) -> Option<u16> {
        match &self.transport {
            CoopSocket::V2(t) => t.seat_ping(seat),
            CoopSocket::Legacy(_) => None,
        }
    }

    /// Smoothed RTT in ms (v2 only).
    #[must_use]
    pub fn rtt_ms(&self) -> Option<f64> {
        match &self.transport {
            CoopSocket::V2(t) => t.rtt_ms(),
            CoopSocket::Legacy(_) => None,
        }
    }

    fn drain_presence(&mut self) {
        let mut evs = Vec::new();
        self.transport.poll_presence(&mut evs);
        for ev in evs {
            match ev {
                PresenceEvent::PeerJoin { seat, name } => {
                    let was_gone = self.gone_peers.remove(&seat);
                    let known = self.peer_names.get(&seat) == Some(&name);
                    if was_gone || !known {
                        self.feed.push(format!("{name} joined"));
                    }
                    self.peer_names.insert(seat, name.clone());
                    web_sys::console::log_1(
                        &format!(
                            "room-web: peer online seat={} name={name:?} ready={}/{}",
                            seat.0,
                            self.ready_peers.len(),
                            self.peer_count
                        )
                        .into(),
                    );
                }
                PresenceEvent::PeerLeave { seat } => {
                    let name = self
                        .peer_names
                        .get(&seat)
                        .cloned()
                        .unwrap_or_else(|| "?".into());
                    self.feed.push(format!("{name} left the game"));
                    self.gone_peers.insert(seat);
                    self.ready_peers.remove(&seat);
                    web_sys::console::warn_1(
                        &format!(
                            "room-web: peer DISCONNECTED seat={} name={name:?} — filling idle cmds soon",
                            seat.0
                        )
                        .into(),
                    );
                }
                PresenceEvent::SocketClose { code, reason } => {
                    self.socket_dead = true;
                    web_sys::console::error_1(
                        &format!(
                            "room-web: local socket closed during play code={code} reason={reason:?}"
                        )
                        .into(),
                    );
                }
                PresenceEvent::SocketError => {
                    web_sys::console::error_1(&"room-web: local socket error during play".into());
                }
                PresenceEvent::SocketOpen | PresenceEvent::Welcome { .. } => {}
            }
        }
    }

    fn log_advance(&mut self, tic: u32, frame: &std::collections::BTreeMap<PeerId, NetTicCmd>) {
        // Tic 0 and every ~5 s: console logging every tic stutters the page.
        if !(tic == 0 || tic % 175 == 0) {
            return;
        }
        if self.last_logged_tic == Some(tic) {
            return;
        }
        self.last_logged_tic = Some(tic);
        let summary: Vec<String> = frame
            .iter()
            .map(|(p, c)| {
                format!(
                    "p{}:f={} s={} btn={:#x}",
                    p.0, c.forward_move, c.side_move, c.buttons
                )
            })
            .collect();
        let (rx, tx) = self.transport.data_stats();
        web_sys::console::log_1(
            &format!(
                "room-web: lockstep tic={tic} [{}] data_rx={rx} data_tx={tx}",
                summary.join(" | ")
            )
            .into(),
        );
    }

    fn poll_inbox(&mut self) {
        let local = self.session.local();
        let mut raw = Vec::new();
        self.transport.poll(&mut raw);
        for (from, bytes) in raw {
            match LobbyMsg::decode(&bytes) {
                Ok(LobbyMsg::Ready(r)) => {
                    if self.ready_peers.insert(r.peer_id) {
                        web_sys::console::log_1(
                            &format!(
                                "room-web: Ready seat={} ({}/{})",
                                r.peer_id.0,
                                self.ready_peers.len(),
                                self.peer_count
                            )
                            .into(),
                        );
                    }
                    continue;
                }
                Ok(LobbyMsg::Hello(hello)) if local.0 == 0 => {
                    // Joiner reloaded / missed Start while host already left the lobby.
                    if let Some(h) = self.host_resume.as_ref() {
                        if validate_hello(&hello, h.fingerprint).is_ok() {
                            self.ready_peers.remove(&from);
                            let welcome = LobbyMsg::Welcome(Welcome {
                                peer_id: from,
                                peer_count: self.peer_count,
                                delay: 0,
                                rng_seed: h.rng_seed,
                                map: h.map.clone(),
                                skill: h.skill,
                            });
                            uplink(&mut self.transport, local, &welcome.encode());
                            uplink(
                                &mut self.transport,
                                local,
                                &LobbyMsg::Start(Start {
                                    rng_seed: h.rng_seed,
                                    names: Vec::new(),
                                })
                                .encode(),
                            );
                            web_sys::console::log_1(
                                &format!("room-web: re-welcome seat {} during Ready gate", from.0)
                                    .into(),
                            );
                        }
                    }
                    continue;
                }
                Ok(_) => continue,
                Err(_) => {}
            }
            if let Ok(t) = decode_timed_cmd(&bytes) {
                if t.tic % 350 == 0 {
                    web_sys::console::log_1(
                        &format!(
                            "room-web: recv TimedCmd tic={} peer={} fwd={}",
                            t.tic, t.peer.0, t.cmd.forward_move
                        )
                        .into(),
                    );
                }
                self.session.push_remote(t);
            }
        }
    }

    fn lockstep_peers_ready(&self) -> bool {
        // v2: server already required ControlMsg::Ready from every seat before Start.
        // Do not gate on legacy LobbyMsg::Ready (those bytes are not forwarded on v2).
        if matches!(self.transport, CoopSocket::V2(_)) {
            return true;
        }
        (0..self.peer_count).all(|i| self.ready_peers.contains(&PeerId(i)))
    }

    fn ensure_ready_announced(&mut self) {
        if matches!(self.transport, CoopSocket::V2(_)) {
            return;
        }
        let local = self.session.local();
        if !self.sent_ready {
            self.ready_peers.insert(local);
            self.sent_ready = true;
            self.frames_since_ready_bcast = 0;
            let msg = LobbyMsg::Ready(Ready { peer_id: local });
            uplink(&mut self.transport, local, &msg.encode());
            web_sys::console::log_1(&format!("room-web: lockstep ready seat={}", local.0).into());
        }
        self.frames_since_ready_bcast = self.frames_since_ready_bcast.saturating_add(1);
        if self.frames_since_ready_bcast >= 15 {
            let msg = LobbyMsg::Ready(Ready { peer_id: local });
            uplink(&mut self.transport, local, &msg.encode());
            // Also nudge Start so a joiner stuck before Welcome/Start can proceed.
            if let Some(h) = self.host_resume.as_ref() {
                uplink(
                    &mut self.transport,
                    local,
                    &LobbyMsg::Start(Start {
                        rng_seed: h.rng_seed,
                        names: Vec::new(),
                    })
                    .encode(),
                );
            }
            self.frames_since_ready_bcast = 0;
        }
    }

    /// Tic the session is waiting to release (for HUD / debug).
    #[must_use]
    pub fn waiting_tic(&self) -> u32 {
        self.session.next_tic()
    }

    /// Whether a local cmd is outstanding.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether every seat has announced map-load Ready.
    #[must_use]
    pub fn peers_ready(&self) -> bool {
        self.lockstep_peers_ready()
    }

    /// Seats that disconnected from the relay.
    #[must_use]
    pub fn gone_peers(&self) -> Vec<u8> {
        self.gone_peers.iter().map(|p| p.0).collect()
    }

    /// DATA rx/tx counters.
    #[must_use]
    pub fn data_stats(&self) -> (u32, u32) {
        self.transport.data_stats()
    }

    /// Frames spent waiting on the current pending tic (rAF count).
    #[must_use]
    pub fn frames_waiting(&self) -> u32 {
        self.frames_waiting
    }

    /// `true` when using protocol v2 (`room-server`).
    #[must_use]
    pub fn is_v2(&self) -> bool {
        self.transport.is_v2()
    }
}

fn net_cmd_to_tic(net: NetTicCmd) -> TicCmd {
    TicCmd {
        buttons: Buttons(net.buttons),
        pitch: net.pitch,
        yaw: net.yaw,
        roll: net.roll,
        forward_move: net.forward_move,
        side_move: net.side_move,
        up_move: net.up_move,
        weapon_select: net.weapon_select,
    }
}

fn frame_to_cmds(
    frame: std::collections::BTreeMap<PeerId, NetTicCmd>,
    peer_count: u8,
) -> Vec<TicCmd> {
    let mut cmds = vec![TicCmd::default(); usize::from(peer_count)];
    for (peer, net) in frame {
        let i = usize::from(peer.0);
        if i < cmds.len() {
            cmds[i] = net_cmd_to_tic(net);
        }
    }
    cmds
}

/// Packet-server uplink: one send fans out to every other seat.
fn uplink(transport: &mut CoopSocket, local: PeerId, bytes: &[u8]) {
    if transport.is_v2() {
        // v2 stamps seat server-side; peer arg is ignored by WsV2Transport::send.
        transport.send(local, bytes);
        return;
    }
    let Some(first) = transport
        .peers()
        .iter()
        .copied()
        .filter(|p| *p != local)
        .min()
    else {
        return;
    };
    transport.send(first, bytes);
}

fn room_id_from_v2_url(url: &str) -> Option<String> {
    // …/v2/rooms/{id}/ws
    let marker = "/v2/rooms/";
    let i = url.find(marker)?;
    let rest = &url[i + marker.len()..];
    let id = rest.split('/').next()?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_owned())
    }
}

fn sample_input(
    held: &BTreeSet<&'static str>,
    attack: bool,
    weapon: u8,
    weapon_cycle: &mut i32,
    mouse_yaw: i32,
    mouse_pitch: i32,
) -> InputSample {
    let mut keys = Vec::new();
    if held.contains("w") {
        keys.push(Key::Forward);
    }
    if held.contains("s") {
        keys.push(Key::Back);
    }
    if held.contains("a") {
        keys.push(Key::MoveLeft);
    }
    if held.contains("d") {
        keys.push(Key::MoveRight);
    }
    if held.contains("left") {
        keys.push(Key::Left);
    }
    if held.contains("right") {
        keys.push(Key::Right);
    }
    if held.contains("shift") {
        keys.push(Key::Speed);
    }
    if attack {
        keys.push(Key::Attack);
    }
    if held.contains("use") {
        keys.push(Key::Use);
    }
    if held.contains("up") {
        keys.push(Key::MoveUp);
    }
    if held.contains("down") {
        keys.push(Key::MoveDown);
    }
    match weapon {
        1 => keys.push(Key::Weapon1),
        2 => keys.push(Key::Weapon2),
        3 => keys.push(Key::Weapon3),
        4 => keys.push(Key::Weapon4),
        5 => keys.push(Key::Weapon5),
        6 => keys.push(Key::Weapon6),
        7 => keys.push(Key::Weapon7),
        8 => keys.push(Key::Weapon8),
        9 => keys.push(Key::Weapon9),
        _ => {}
    }
    if *weapon_cycle > 0 {
        keys.push(Key::NextWeapon);
        *weapon_cycle -= 1;
    } else if *weapon_cycle < 0 {
        keys.push(Key::PrevWeapon);
        *weapon_cycle += 1;
    }
    InputSample {
        keys,
        mouse_yaw,
        mouse_pitch,
    }
}

/// Stable fingerprint from IWAD bytes (must match across peers).
#[must_use]
pub fn iwad_fingerprint(iwad: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for &b in iwad {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h ^= iwad.len() as u64;
    h
}
