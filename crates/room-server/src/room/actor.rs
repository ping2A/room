//! Room actor task.

use crate::config::ServerConfig;
use crate::limits::TokenBucket;
use crate::registry::RoomRegistry;
use crate::room::duke_v2::DukeV2State;
use crate::room::legacy::stamp_timed_cmd_peer;
use crate::room::spectate::{FrameLog, MAX_SPECTATORS};
use crate::room::v2::V2State;
use crate::util::{new_resume_token, random_u32};
use bytes::Bytes;
use room_net::{
    MSG_DATA, MSG_HELLO, MSG_VOICE, PeerId, pack_peer_join, pack_peer_leave, pack_reject,
    pack_server_data, pack_server_voice, pack_welcome, parse_relay,
};
use room_proto::{
    BinaryMsg, ControlMsg, HelloMsg, IceServerEntry, MediaInfo, RejectMsg, RoomModRef, RosterSeat,
    SeatState, WelcomeMsg, WelcomeRoom, encode_binary, encode_control,
};
use room_sfu::{MediaSession, SignalIn, SignalOut, ice_servers, ice_servers_long_term};
#[cfg(feature = "builtin-turn")]
use room_sfu::{issue_turn_user, revoke_turn_user};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

/// Default resume grace while still in lobby/loading (docs/22 §4).
const RESUME_GRACE: Duration = Duration::from_secs(60);

/// Opaque connection id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnId(pub u64);

/// Why a seat left.
#[derive(Clone, Debug)]
pub enum LeaveReason {
    /// Client closed the socket.
    Disconnect,
    /// Kicked by policy / admin.
    Kick(String),
    /// Idle / handshake timeout.
    Timeout,
}

/// Outbound bytes to one player (binary WebSocket payload).
pub type PlayerTx = mpsc::Sender<Bytes>;

/// Commands into the room actor.
pub enum RoomCmd {
    /// Legacy FTRL client joined (seat assigned by actor).
    LegacyJoin {
        /// Connection.
        conn: ConnId,
        /// Outbound queue.
        tx: PlayerTx,
        /// Remote socket address.
        remote: SocketAddr,
    },
    /// Legacy FTRL frame from a connection.
    LegacyFrame {
        /// Connection.
        conn: ConnId,
        /// Raw WebSocket binary (FTRL).
        data: Bytes,
    },
    /// v2 client joined (pre-hello; waiting for JSON hello).
    V2Join {
        /// Connection.
        conn: ConnId,
        /// Outbound queue.
        tx: PlayerTx,
        /// Remote socket address.
        remote: SocketAddr,
    },
    /// v2 binary or text already classified.
    V2Binary {
        /// Connection.
        conn: ConnId,
        /// Parsed message.
        msg: BinaryMsg,
    },
    /// v2 JSON control.
    V2Control {
        /// Connection.
        conn: ConnId,
        /// Parsed control.
        msg: ControlMsg,
    },
    /// Connection gone.
    Leave {
        /// Connection.
        conn: ConnId,
        /// Reason.
        reason: LeaveReason,
    },
    /// Operator action (admin API).
    Admin(AdminCmd),
    /// 1 Hz housekeeping.
    Tick,
}

/// Admin → room actor commands.
#[derive(Debug)]
pub enum AdminCmd {
    /// Kick a seat.
    Kick {
        /// Seat index.
        seat: u8,
        /// Reason shown to the client.
        reason: String,
    },
    /// Mute flags for a seat.
    Mute {
        /// Seat index.
        seat: u8,
        /// Audio muted.
        audio: bool,
        /// Video muted.
        video: bool,
        /// Chat muted.
        chat: bool,
    },
    /// Broadcast a notice to the room.
    Notice {
        /// Notice text.
        text: String,
    },
    /// Close the room (all clients bye + actor stop).
    Close {
        /// Reason.
        reason: String,
    },
    /// Snapshot seats / poses / outline for the admin UI.
    Inspect {
        /// Reply channel.
        reply: oneshot::Sender<RoomInspect>,
    },
}

/// One seat row for admin inspect.
#[derive(Clone, Debug, Serialize)]
pub struct SeatInspect {
    /// Seat index.
    pub seat: u8,
    /// Display name.
    pub name: String,
    /// Lifecycle (`lobby` / `loading` / `ready` / `playing` / `away`).
    pub state: String,
    /// Remote IP (or `—`).
    pub remote_ip: String,
    /// Resume token (reconnect key).
    pub resume_token: String,
    /// Join ticket id when minted; otherwise raw ticket / room id.
    pub ticket_id: String,
    /// IWAD fingerprint hex from hello.
    pub iwad_fingerprint: String,
    /// Per-seat TURN username (empty when unused).
    pub turn_username: String,
    /// Server mute flags.
    pub muted_audio: bool,
    /// Video mute.
    pub muted_video: bool,
    /// Chat mute.
    pub muted_chat: bool,
    /// Latest pose (`None` until telemetry).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pose: Option<PoseInspect>,
}

/// One spectator row for admin inspect.
#[derive(Clone, Debug, Serialize)]
pub struct SpectatorInspect {
    /// Display name.
    pub name: String,
    /// Remote IP.
    pub remote_ip: String,
    /// Frames delivered so far.
    pub frames_sent: usize,
}

/// Pose for admin minimap.
#[derive(Clone, Debug, Serialize)]
pub struct PoseInspect {
    /// Map X.
    pub x: i32,
    /// Map Y.
    pub y: i32,
    /// Yaw degrees.
    pub angle: i16,
}

/// Full room inspect payload for admin.
#[derive(Clone, Debug, Serialize)]
pub struct RoomInspect {
    /// Room id.
    pub id: String,
    /// Start map.
    pub map: String,
    /// Skill.
    pub skill: u8,
    /// Capacity.
    pub max_players: u8,
    /// RNG seed.
    pub rng_seed: u32,
    /// Lockstep started.
    pub started: bool,
    /// Pinned IWAD fingerprint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iwad_fingerprint: Option<String>,
    /// Verifier flag.
    pub enable_verifier: bool,
    /// Gameplay mod name.
    #[serde(rename = "mod", skip_serializing_if = "Option::is_none")]
    pub game_mod: Option<String>,
    /// Content packs (e.g. `voxel`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<String>,
    /// Occupied (live) seats.
    pub occupied: usize,
    /// Per-seat detail.
    pub seats: Vec<SeatInspect>,
    /// Connected spectators `(name, ip)`.
    pub spectators: Vec<SpectatorInspect>,
    /// Flat outline `[x1,y1,x2,y2,…]` in map units.
    pub outline: Vec<i16>,
}

/// Settings chosen at room creation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomSettings {
    /// Start map.
    pub map: String,
    /// Skill 1..=5.
    pub skill: u8,
    /// Max seats 2..=8.
    pub max_players: u8,
    /// Atomic Duke `ud.coop`: 0 = DukeMatch, 1 = co-op, 2 = no spawn.
    #[serde(default = "default_duke_game_mode")]
    pub duke_game_mode: u8,
    /// Atomic Duke `ud.ffire`; ignored for Doom rooms.
    #[serde(default)]
    pub duke_friendly_fire: bool,
    /// Public listing (unused in M1).
    #[serde(default)]
    pub visibility: String,
    /// Optional headless verifier (docs/22 phase 3); off by default.
    #[serde(default)]
    pub enable_verifier: bool,
    /// Gameplay mod from the node's catalog (every seat mounts it).
    #[serde(default, rename = "mod", skip_serializing_if = "Option::is_none")]
    pub game_mod: Option<RoomModRef>,
    /// Content packs stacked with the IWAD / gameplay mod (e.g. `voxel`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<String>,
    /// Match time limit in minutes; the room closes when it runs out.
    #[serde(default = "default_time_limit")]
    pub time_limit: u16,
    /// Content every seat loads: `doom` (doom1.wad) or `duke3d` (duke3d.grp).
    #[serde(default = "default_game")]
    pub game: String,
    /// Empty seats are sim bots on every client.
    #[serde(default)]
    pub fill_bots: bool,
    /// Quick-play queue: start after timeout with bots for empty seats.
    #[serde(default)]
    pub quick_play: bool,
    /// Library files every seat mounts after the IWAD (from the preset).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<room_proto::RoomContentRef>,
    /// Library IWAD replacing the seats' base game (from the preset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iwad: Option<room_proto::RoomContentRef>,
    /// Preset title the room was created from (empty = none).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preset: String,
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self {
            map: "E1M1".into(),
            skill: 3,
            max_players: 2,
            duke_game_mode: 1,
            duke_friendly_fire: false,
            visibility: "public".into(),
            enable_verifier: false,
            game_mod: None,
            packs: Vec::new(),
            time_limit: default_time_limit(),
            game: default_game(),
            fill_bots: false,
            quick_play: false,
            content: Vec::new(),
            iwad: None,
            preset: String::new(),
        }
    }
}

fn default_time_limit() -> u16 {
    20
}

fn default_game() -> String {
    GAMES[0].into()
}

fn default_duke_game_mode() -> u8 {
    1
}

/// Games a room can run; the first is the default.
pub const GAMES: [&str; 2] = ["doom", "duke3d"];

/// `POST /api/rooms` body.
#[derive(Clone, Debug, Deserialize)]
pub struct CreateRoomRequest {
    /// Map lump.
    #[serde(default = "default_map")]
    pub map: String,
    /// Skill.
    #[serde(default = "default_skill")]
    pub skill: u8,
    /// Max players.
    #[serde(default = "default_max_players")]
    pub max_players: u8,
    /// Atomic Duke `ud.coop`: 0 = DukeMatch, 1 = co-op, 2 = no spawn.
    #[serde(default = "default_duke_game_mode")]
    pub duke_game_mode: u8,
    /// Atomic Duke `ud.ffire`; ignored for Doom rooms.
    #[serde(default)]
    pub duke_friendly_fire: bool,
    /// `public` (listed in lobby) or `private` (join by id only).
    #[serde(default = "default_visibility")]
    pub visibility: String,
    /// Request server-side verifier when the node allows it (`--enable-verifier`).
    #[serde(default)]
    pub enable_verifier: bool,
    /// Catalog mod name (`GET /api/mods`); empty / absent = vanilla.
    #[serde(default, rename = "mod")]
    pub game_mod: Option<String>,
    /// Content packs (`voxel`, …); stacked with the gameplay mod.
    #[serde(default)]
    pub packs: Vec<String>,
    /// Match time limit in minutes (1..=180); absent / 0 = the node default.
    #[serde(default)]
    pub time_limit: Option<u16>,
    /// One of [`GAMES`]; absent = `doom`.
    #[serde(default)]
    pub game: Option<String>,
    /// Fill empty seats with sim bots (BE8).
    #[serde(default)]
    pub fill_bots: bool,
    /// Quick-play queue room (`S-BR4`): start after timeout, not only when full.
    #[serde(default)]
    pub quick_play: bool,
    /// Admin preset id (`GET /api/presets`): its IWAD, content files and map
    /// list. Required when the node sets `require_preset`.
    #[serde(default)]
    pub preset: Option<String>,
}

impl Default for CreateRoomRequest {
    fn default() -> Self {
        Self {
            map: default_map(),
            skill: default_skill(),
            max_players: default_max_players(),
            duke_game_mode: default_duke_game_mode(),
            duke_friendly_fire: false,
            visibility: default_visibility(),
            enable_verifier: false,
            game_mod: None,
            packs: Vec::new(),
            time_limit: None,
            game: None,
            fill_bots: false,
            quick_play: false,
            preset: None,
        }
    }
}

fn default_map() -> String {
    "E1M1".into()
}
fn default_skill() -> u8 {
    3
}
fn default_max_players() -> u8 {
    2
}
fn default_visibility() -> String {
    "public".into()
}

/// `POST /api/rooms` response.
#[derive(Clone, Debug, Serialize)]
pub struct CreateRoomResponse {
    /// Room id (hex).
    pub id: String,
    /// Legacy WebSocket path.
    pub ws_legacy: String,
    /// v2 WebSocket path.
    pub ws_v2: String,
    /// Settings.
    pub settings: RoomSettings,
    /// RNG seed.
    pub rng_seed: u32,
}

/// Handle for sending commands to a room.
#[derive(Clone)]
pub struct RoomHandle {
    /// Room id.
    pub id: String,
    /// Settings snapshot.
    pub settings: RoomSettings,
    /// RNG seed.
    pub rng_seed: u32,
    /// Command sender.
    pub tx: mpsc::Sender<RoomCmd>,
    /// Live (non-away) seats — updated by the room actor for the public lobby.
    pub occupied: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// True after lockstep `Start`.
    pub started: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

struct SeatSlot {
    tx: Option<PlayerTx>,
    name: String,
    resume_token: String,
    state: SeatState,
    /// True after FTRL HELLO / v2 hello.
    welcomed: bool,
    /// Set when the socket left but the seat is still reserved for resume.
    away_since: Option<Instant>,
    /// Server mute (audio/video/chat).
    muted_audio: bool,
    muted_video: bool,
    muted_chat: bool,
    /// Chat rate limit (1 msg/s, burst 5).
    chat_bucket: TokenBucket,
    /// Last seen remote IP for this seat.
    remote_ip: String,
    /// Join ticket id (or room id / raw ticket).
    ticket_id: String,
    /// IWAD fingerprint from hello.
    iwad_fingerprint: String,
    /// Latest client pose telemetry.
    pose: Option<PoseInspect>,
    /// Built-in TURN username issued for this seat (revoked on leave).
    turn_username: Option<String>,
}

struct PendingV2 {
    tx: PlayerTx,
    remote_ip: String,
}

/// Seatless watcher: receives every frame from tic 0, never sends input.
struct Spectator {
    tx: PlayerTx,
    name: String,
    remote_ip: String,
    iwad_fingerprint: String,
    /// Next [`FrameLog`] index to send.
    sent: usize,
}

/// Outbound slots kept free for control messages while streaming catch-up.
const SPECTATOR_RESERVED_SLOTS: usize = 4;

struct RoomState {
    id: String,
    settings: RoomSettings,
    rng_seed: u32,
    cfg: ServerConfig,
    registry: RoomRegistry,
    seats: BTreeMap<u8, SeatSlot>,
    /// conn → seat (assigned after hello for v2; immediately for legacy)
    conn_seat: BTreeMap<ConnId, u8>,
    /// v2 connections waiting for hello.
    pending_v2: BTreeMap<ConnId, PendingV2>,
    /// Fingerprint pinned by first hello (v2).
    iwad_fingerprint: Option<String>,
    /// Sparse automap outline from a client (admin minimap).
    map_outline: Vec<i16>,
    v2: V2State,
    duke_v2: DukeV2State,
    started: bool,
    spectators: BTreeMap<ConnId, Spectator>,
    frame_log: FrameLog,
    media: MediaSession,
    /// Client checksums by tic → seat (cleared after compare).
    checksums: BTreeMap<u32, BTreeMap<u8, u64>>,
    /// Authoritative checksums from the Headless verifier (feature `verifier`).
    server_checksums: BTreeMap<u32, u64>,
    #[cfg(feature = "verifier")]
    verifier: Option<VerifierHandles>,
    occupied: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    started_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Room creation, then lockstep start (time limit clock).
    clock_from: Instant,
    /// Latest `result` report per seat.
    results: BTreeMap<u8, MatchReport>,
    /// Result recorded on the leaderboard (once per room).
    recorded: bool,
    /// The room closes at this instant (match over / time up).
    close_at: Option<Instant>,
    /// Set when the actor should stop (closed by admin, drain or match end).
    closed: bool,
    /// Quick-play: wall clock to force start (`S-BR4`).
    quick_play_deadline: Option<Instant>,
    /// Human seat count at start (bots from this index).
    start_bot_first_seat: u8,
}

/// One seat's view of the match end (all honest seats send the same).
#[derive(Clone, Debug, PartialEq, Eq)]
struct MatchReport {
    winner: Option<u8>,
    time_up: bool,
    standings: Vec<room_proto::ResultRow>,
}

/// Wall-clock slack past the time limit before the server closes the room
/// itself (seat sims end the match on their tic clock, which can lag).
const TIME_LIMIT_SLACK: Duration = Duration::from_secs(30);

#[cfg(feature = "verifier")]
struct VerifierHandles {
    frames: VerifierFrameSink,
    checksums: tokio::sync::mpsc::Receiver<crate::verifier::VerifierChecksum>,
}

#[cfg(feature = "verifier")]
enum VerifierFrameSink {
    Doom(tokio::sync::mpsc::Sender<room_proto::TicFrameMsg>),
    Duke(tokio::sync::mpsc::Sender<room_proto::DukeSyncFrameMsg>),
}

/// Spawn a room actor; returns its handle.
pub fn spawn_room(
    id: String,
    settings: RoomSettings,
    rng_seed: u32,
    cfg: ServerConfig,
    registry: RoomRegistry,
) -> RoomHandle {
    let (tx, rx) = mpsc::channel(cfg.room_cmd_capacity);
    let occupied = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let started_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let handle = RoomHandle {
        id: id.clone(),
        settings: settings.clone(),
        rng_seed,
        tx: tx.clone(),
        occupied: occupied.clone(),
        started: started_flag.clone(),
    };
    tokio::spawn(async move {
        let mut state = RoomState {
            id: id.clone(),
            settings,
            rng_seed,
            cfg,
            registry,
            seats: BTreeMap::new(),
            conn_seat: BTreeMap::new(),
            pending_v2: BTreeMap::new(),
            iwad_fingerprint: None,
            map_outline: Vec::new(),
            v2: V2State::new(),
            duke_v2: DukeV2State::new(),
            started: false,
            spectators: BTreeMap::new(),
            frame_log: FrameLog::new(),
            media: MediaSession::new(id.clone()),
            checksums: BTreeMap::new(),
            server_checksums: BTreeMap::new(),
            #[cfg(feature = "verifier")]
            verifier: None,
            occupied,
            started_flag,
            clock_from: Instant::now(),
            results: BTreeMap::new(),
            recorded: false,
            close_at: None,
            closed: false,
            quick_play_deadline: None,
            start_bot_first_seat: 0,
        };
        room_loop(&mut state, rx).await;
        state.registry.clear_quick_play_br(&id);
        state.registry.remove(&id);
        info!(room_id = %id, "room actor stopped");
    });
    // Housekeeping ticker.
    let tick_tx = handle.tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if tick_tx.send(RoomCmd::Tick).await.is_err() {
                break;
            }
        }
    });
    handle
}

async fn room_loop(state: &mut RoomState, mut rx: mpsc::Receiver<RoomCmd>) {
    loop {
        let wake = if state.started {
            if state.settings.game == "duke3d" {
                state.duke_v2.next_wake()
            } else {
                state.v2.next_wake()
            }
        } else {
            None
        };
        let next = match wake {
            Some(at) => tokio::select! {
                c = rx.recv() => c,
                () = tokio::time::sleep_until(tokio::time::Instant::from_std(at)) => {
                    on_clock(state);
                    continue;
                }
            },
            None => rx.recv().await,
        };
        let Some(cmd) = next else {
            break;
        };
        match cmd {
            RoomCmd::LegacyJoin { conn, tx, remote } => on_legacy_join(state, conn, tx, remote),
            RoomCmd::LegacyFrame { conn, data } => on_legacy_frame(state, conn, data),
            RoomCmd::V2Join { conn, tx, remote } => on_v2_join(state, conn, tx, remote),
            RoomCmd::V2Binary { conn, msg } => on_v2_binary(state, conn, msg),
            RoomCmd::V2Control { conn, msg } => on_v2_control(state, conn, msg),
            RoomCmd::Leave { conn, reason } => on_leave(state, conn, reason),
            RoomCmd::Admin(cmd) => on_admin(state, cmd),
            RoomCmd::Tick => on_tick(state),
        }
        publish_lobby_stats(state);
        if state.closed {
            break;
        }
        if state.seats.is_empty() && state.started {
            // Idle empty room after play — stop actor.
            debug!(room_id = %state.id, "empty after start; stopping");
            break;
        }
    }
}

fn on_legacy_join(state: &mut RoomState, conn: ConnId, tx: PlayerTx, remote: SocketAddr) {
    expire_reserved(state);
    if live_seat_count(state) >= usize::from(state.settings.max_players) {
        let _ = tx.try_send(Bytes::from(pack_reject("lobby full")));
        return;
    }
    let Some(seat) = free_seat(state) else {
        let _ = tx.try_send(Bytes::from(pack_reject("no free seat")));
        return;
    };
    let token = new_resume_token().unwrap_or_else(|_| "0".repeat(64));
    state.seats.insert(
        seat,
        SeatSlot {
            tx: Some(tx.clone()),
            name: format!("player{seat}"),
            resume_token: token,
            state: SeatState::Lobby,
            welcomed: false,
            away_since: None,
            muted_audio: false,
            muted_video: false,
            muted_chat: false,
            chat_bucket: TokenBucket::new(state.cfg.chat_burst, state.cfg.chat_refill_per_sec),
            remote_ip: remote.ip().to_string(),
            ticket_id: String::new(),
            iwad_fingerprint: String::new(),
            pose: None,
            turn_username: None,
        },
    );
    state.conn_seat.insert(conn, seat);
    info!(room_id = %state.id, seat, conn = conn.0, "legacy join reserved");
}

fn on_legacy_frame(state: &mut RoomState, conn: ConnId, data: Bytes) {
    let Some(&seat) = state.conn_seat.get(&conn) else {
        return;
    };
    let Ok((ty, rest)) = parse_relay(&data) else {
        return;
    };
    match ty {
        MSG_HELLO => {
            let name = parse_hello_name(rest);
            if let Some(slot) = state.seats.get_mut(&seat) {
                slot.name = name.clone();
                slot.welcomed = true;
                if let Some(tx) = &slot.tx {
                    let _ = tx.try_send(Bytes::from(pack_welcome(
                        PeerId(seat),
                        state.settings.max_players,
                    )));
                }
            }
            // Roster of existing peers → newcomer.
            for (&other, slot) in &state.seats {
                if other == seat || !slot.welcomed {
                    continue;
                }
                if let Some(me) = state.seats.get(&seat) {
                    if let Some(tx) = &me.tx {
                        let _ = tx.try_send(Bytes::from(pack_peer_join(PeerId(other), &slot.name)));
                    }
                }
            }
            // Announce join.
            let packet = Bytes::from(pack_peer_join(PeerId(seat), &name));
            broadcast_except(state, seat, packet);
            info!(room_id = %state.id, seat, %name, "legacy HELLO");
        }
        MSG_DATA => {
            // Stamp seat into TimedCmd when possible (G6).
            let stamped = stamp_timed_cmd_peer(rest, seat);
            let packet = Bytes::from(pack_server_data(PeerId(seat), &stamped));
            broadcast_except(state, seat, packet);
        }
        MSG_VOICE => {
            if rest.len() > 8_192 {
                return;
            }
            let packet = Bytes::from(pack_server_voice(PeerId(seat), rest));
            broadcast_except(state, seat, packet);
        }
        _ => {}
    }
}

fn on_v2_join(state: &mut RoomState, conn: ConnId, tx: PlayerTx, remote: SocketAddr) {
    expire_reserved(state);
    state.pending_v2.insert(
        conn,
        PendingV2 {
            tx,
            remote_ip: remote.ip().to_string(),
        },
    );
    info!(room_id = %state.id, conn = conn.0, ip = %remote.ip(), "v2 pending hello");
}

fn on_v2_control(state: &mut RoomState, conn: ConnId, msg: ControlMsg) {
    if state.spectators.contains_key(&conn) {
        return;
    }
    match msg {
        ControlMsg::Hello(h) => v2_hello(state, conn, h),
        ControlMsg::Watch {
            iwad_fingerprint,
            name,
            ..
        } => v2_watch(state, conn, &iwad_fingerprint, name.as_deref()),
        ControlMsg::Ready {} => {
            let Some(&seat) = state.conn_seat.get(&conn) else {
                return;
            };
            if let Some(slot) = state.seats.get_mut(&seat) {
                slot.state = SeatState::Ready;
            }
            push_roster(state);
            maybe_v2_start(state);
        }
        ControlMsg::Pose { x, y, angle } => {
            let Some(&seat) = state.conn_seat.get(&conn) else {
                return;
            };
            if let Some(slot) = state.seats.get_mut(&seat) {
                slot.pose = Some(PoseInspect { x, y, angle });
            }
        }
        ControlMsg::MapOutline { segs } => {
            if segs.len() > 800 || segs.len() % 4 != 0 {
                return;
            }
            // First non-empty outline wins (typically seat 0 after map prepare).
            if state.map_outline.is_empty() && !segs.is_empty() {
                state.map_outline = segs;
            }
        }
        ControlMsg::Rtc {
            kind,
            sdp,
            candidate,
            mid,
        } => {
            let Some(&seat) = state.conn_seat.get(&conn) else {
                return;
            };
            on_rtc(state, seat, kind, sdp, candidate, mid);
        }
        ControlMsg::Chat { text, .. } => {
            let Some(&seat) = state.conn_seat.get(&conn) else {
                return;
            };
            if state.seats.get(&seat).is_some_and(|s| s.muted_chat) {
                return;
            }
            let allowed = state
                .seats
                .get_mut(&seat)
                .is_some_and(|s| s.chat_bucket.try_take());
            if !allowed {
                return;
            }
            let text = text.chars().take(200).collect::<String>();
            if text.is_empty() {
                return;
            }
            let out = ControlMsg::Chat {
                text,
                from: Some(seat),
                ts: Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                ),
            };
            if let Ok(s) = encode_control(&out) {
                broadcast_all(state, Bytes::from(s.into_bytes()));
            }
        }
        ControlMsg::FaceShare {
            name,
            face_id,
            rgba_b64,
            ..
        } => {
            // docs/26 §8: opt-in share; never persist; default off on public lobbies.
            if !state.cfg.face_share_enabled {
                return;
            }
            let Some(&seat) = state.conn_seat.get(&conn) else {
                return;
            };
            if rgba_b64.len() > 96_000 || name.chars().count() > 64 {
                return;
            }
            let out = ControlMsg::FaceShare {
                from: Some(seat),
                name: name.chars().take(64).collect(),
                face_id,
                rgba_b64,
            };
            if let Ok(s) = encode_control(&out) {
                broadcast_all(state, Bytes::from(s.into_bytes()));
            }
        }
        ControlMsg::Result {
            winner,
            time_up,
            standings,
        } => {
            let Some(&seat) = state.conn_seat.get(&conn) else {
                return;
            };
            on_result(
                state,
                seat,
                MatchReport {
                    winner,
                    time_up,
                    standings,
                },
            );
        }
        _ => {}
    }
}

fn v2_hello(state: &mut RoomState, conn: ConnId, h: HelloMsg) {
    let pending = state.pending_v2.remove(&conn);
    let (tx, remote_ip) = if let Some(p) = pending {
        (p.tx, p.remote_ip)
    } else if let Some(tx) = state
        .conn_seat
        .get(&conn)
        .and_then(|s| state.seats.get(s))
        .and_then(|slot| slot.tx.clone())
    {
        let ip = state
            .conn_seat
            .get(&conn)
            .and_then(|s| state.seats.get(s))
            .map(|s| s.remote_ip.clone())
            .unwrap_or_default();
        (tx, ip)
    } else {
        return;
    };

    // Resume reclaim (lobby/loading only in M2 without verifier).
    if let Some(token) = h.resume_token.as_deref() {
        if let Some((&seat, _)) = state
            .seats
            .iter()
            .find(|(_, s)| s.away_since.is_some() && s.resume_token == token)
        {
            if state.started && !state.settings.enable_verifier {
                send_reject_tx(&tx, "resume_denied", "resume after start requires verifier");
                return;
            }
            if let Some(slot) = state.seats.get_mut(&seat) {
                if let Some(hit) = state.registry.admin().ban_hit(&crate::bans::BanSubject {
                    ip: &remote_ip,
                    name: &slot.name,
                    fingerprint: &h.iwad_fingerprint,
                    ticket_id: &slot.ticket_id,
                    turn_user: slot.turn_username.as_deref().unwrap_or(""),
                    resume_token: token,
                }) {
                    send_reject_tx(&tx, "banned", &format!("{} ({})", hit.reason, hit.scope));
                    return;
                }
                slot.tx = Some(tx);
                slot.away_since = None;
                slot.state = SeatState::Loading;
                slot.welcomed = true;
                if !remote_ip.is_empty() {
                    slot.remote_ip = remote_ip;
                }
                if !h.iwad_fingerprint.is_empty() {
                    slot.iwad_fingerprint = h.iwad_fingerprint.clone();
                }
            }
            state.conn_seat.insert(conn, seat);
            finish_v2_welcome(state, seat);
            return;
        }
        send_reject_tx(&tx, "resume_denied", "unknown resume token");
        return;
    }

    let (display_from_ticket, ticket_id) = if state.cfg.require_tickets {
        match state.registry.consume_ticket(&h.ticket, &state.id) {
            Ok(claims) => (Some(claims.name), claims.ticket_id),
            Err(e) => {
                send_reject_tx(&tx, "ticket", &e.to_string());
                return;
            }
        }
    } else if h.ticket != state.id && !h.ticket.is_empty() {
        // Dev mode: accept room id; if it looks like a minted ticket, try verify.
        match state.registry.consume_ticket(&h.ticket, &state.id) {
            Ok(claims) => (Some(claims.name), claims.ticket_id),
            Err(_) => (None, h.ticket.clone()),
        }
    } else {
        (None, h.ticket.clone())
    };

    expire_reserved(state);
    if live_seat_count(state) >= usize::from(state.settings.max_players) {
        send_reject_tx(&tx, "lobby_full", "lobby full");
        return;
    }
    let Some(seat) = free_seat(state) else {
        send_reject_tx(&tx, "lobby_full", "no free seat");
        return;
    };

    if let Some(fp) = &state.iwad_fingerprint {
        if fp != &h.iwad_fingerprint {
            send_reject_tx(&tx, "fingerprint", "iwad fingerprint mismatch");
            return;
        }
    } else {
        state.iwad_fingerprint = Some(h.iwad_fingerprint.clone());
    }

    let display = display_from_ticket
        .as_deref()
        .and_then(crate::names::sanitize_display_name)
        .or_else(|| {
            h.name
                .as_deref()
                .and_then(crate::names::sanitize_display_name)
        })
        .unwrap_or_else(|| format!("player{seat}"));
    let display = unique_seat_name(state, &display);
    // Enforce bans (name / IP / fingerprint / ticket / auth keys) before seating.
    if let Some(hit) = state.registry.admin().ban_hit(&crate::bans::BanSubject {
        ip: &remote_ip,
        name: &display,
        fingerprint: &h.iwad_fingerprint,
        ticket_id: &ticket_id,
        ..Default::default()
    }) {
        warn!(
            room_id = %state.id,
            ban = %hit.id,
            scope = %hit.scope,
            "hello refused (banned)"
        );
        send_reject_tx(&tx, "banned", &format!("{} ({})", hit.reason, hit.scope));
        return;
    }
    let token = new_resume_token().unwrap_or_else(|_| "0".repeat(64));
    state.seats.insert(
        seat,
        SeatSlot {
            tx: Some(tx),
            name: display,
            resume_token: token,
            state: SeatState::Loading,
            welcomed: true,
            away_since: None,
            muted_audio: false,
            muted_video: false,
            muted_chat: false,
            chat_bucket: TokenBucket::new(state.cfg.chat_burst, state.cfg.chat_refill_per_sec),
            remote_ip,
            ticket_id,
            iwad_fingerprint: h.iwad_fingerprint.clone(),
            pose: None,
            turn_username: None,
        },
    );
    state.conn_seat.insert(conn, seat);
    if state.settings.quick_play && state.quick_play_deadline.is_none() {
        state.quick_play_deadline =
            Some(Instant::now() + Duration::from_secs(state.cfg.quick_play_timeout_secs));
    }
    finish_v2_welcome(state, seat);
}

/// Suffix `name` (" 2", " 3", …) until no other seat uses it (case-insensitive).
fn unique_seat_name(state: &RoomState, name: &str) -> String {
    let taken = |n: &str| state.seats.values().any(|s| s.name.eq_ignore_ascii_case(n));
    if !taken(name) {
        return name.to_owned();
    }
    for i in 2..=9u8 {
        let suffix = format!(" {i}");
        let base: String = name.chars().take(24 - suffix.len()).collect();
        let candidate = format!("{base}{suffix}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    name.to_owned()
}

fn v2_watch(state: &mut RoomState, conn: ConnId, fingerprint: &str, name: Option<&str>) {
    let Some(pending) = state.pending_v2.remove(&conn) else {
        return;
    };
    let tx = pending.tx;
    let spec_name = name
        .and_then(crate::names::sanitize_display_name)
        .unwrap_or_else(|| "spectator".into());
    if let Some(hit) = state.registry.admin().ban_hit(&crate::bans::BanSubject {
        ip: &pending.remote_ip,
        name: &spec_name,
        fingerprint,
        ..Default::default()
    }) {
        send_reject_tx(&tx, "banned", &format!("{} ({})", hit.reason, hit.scope));
        return;
    }
    if state
        .iwad_fingerprint
        .as_deref()
        .is_some_and(|fp| fp != fingerprint)
    {
        send_reject_tx(&tx, "fingerprint", "iwad fingerprint mismatch");
        return;
    }
    if state.spectators.len() >= MAX_SPECTATORS {
        send_reject_tx(&tx, "spectators_full", "too many spectators");
        return;
    }
    if state.frame_log.is_full() {
        send_reject_tx(&tx, "spectate_unavailable", "match too long to replay");
        return;
    }
    if state.started && state.settings.game == "duke3d" {
        send_reject_tx(
            &tx,
            "spectate_unavailable",
            "late-join Duke spectator replay is not available",
        );
        return;
    }
    let watching = ControlMsg::Watching {
        room: welcome_room(state),
        roster: roster_vec(state),
    };
    if let Ok(s) = encode_control(&watching) {
        let _ = tx.try_send(Bytes::from(s.into_bytes()));
    }
    if state.started {
        if let Ok(s) = encode_control(&start_control(state)) {
            let _ = tx.try_send(Bytes::from(s.into_bytes()));
        }
    }
    info!(room_id = %state.id, conn = conn.0, name = %spec_name, ip = %pending.remote_ip, "spectator joined");
    state.spectators.insert(
        conn,
        Spectator {
            tx,
            name: spec_name,
            remote_ip: pending.remote_ip,
            iwad_fingerprint: fingerprint.to_owned(),
            sent: 0,
        },
    );
    pump_spectators(state);
}

/// Stream logged frames to every spectator as fast as their queue allows.
fn pump_spectators(state: &mut RoomState) {
    let log = &state.frame_log;
    for sp in state.spectators.values_mut() {
        while sp.tx.capacity() > SPECTATOR_RESERVED_SLOTS {
            let Some((pkt, next)) = log.batch(sp.sent) else {
                break;
            };
            if sp.tx.try_send(Bytes::from(pkt)).is_err() {
                break;
            }
            sp.sent = next;
        }
    }
}

/// Log a released frame for spectators and stream it out.
fn spectate_frame(state: &mut RoomState, bin: &Bytes) {
    if state.frame_log.push(&bin[5..]) {
        pump_spectators(state);
        return;
    }
    // Log frozen: caught-up watchers get live frames directly; anyone still
    // replaying would have a gap, so they are dropped.
    let end = state.frame_log.len();
    let behind: Vec<ConnId> = state
        .spectators
        .iter()
        .filter(|(_, sp)| sp.sent < end)
        .map(|(c, _)| *c)
        .collect();
    for conn in behind {
        if let Some(sp) = state.spectators.remove(&conn) {
            send_bye_tx(&sp.tx, "spectate_unavailable", "match too long to replay");
        }
    }
    broadcast_spectators(state, bin);
}

fn send_bye_tx(tx: &PlayerTx, code: &str, reason: &str) {
    let bye = ControlMsg::Bye {
        code: code.into(),
        reason: reason.into(),
    };
    if let Ok(s) = encode_control(&bye) {
        let _ = tx.try_send(Bytes::from(s.into_bytes()));
    }
}

fn broadcast_spectators(state: &RoomState, packet: &Bytes) {
    for sp in state.spectators.values() {
        let _ = sp.tx.try_send(packet.clone());
    }
}

fn welcome_room(state: &RoomState) -> WelcomeRoom {
    WelcomeRoom {
        map: state.settings.map.clone(),
        skill: state.settings.skill,
        max_players: state.settings.max_players,
        duke_game_mode: state.settings.duke_game_mode,
        duke_friendly_fire: state.settings.duke_friendly_fire,
        rng_seed: state.rng_seed,
        delay: 0,
        game_mod: state.settings.game_mod.clone(),
        packs: state.settings.packs.clone(),
        time_limit: u32::from(state.settings.time_limit) * 60,
        fill_bots: state.settings.fill_bots,
        content: state.settings.content.clone(),
        iwad: state.settings.iwad.clone(),
        preset: state.settings.preset.clone(),
    }
}

fn start_control(state: &RoomState) -> ControlMsg {
    ControlMsg::Start {
        tic0: 0,
        rng_seed: state.rng_seed,
        fill_bots: state.settings.fill_bots,
        bot_first_seat: state.start_bot_first_seat,
    }
}

/// Wall-clock deadline: the time limit from the start (or from creation for
/// a room nobody started), plus slack for seats whose tic clock lags.
fn time_limit_deadline(state: &RoomState) -> Instant {
    let limit = Duration::from_secs(u64::from(state.settings.time_limit) * 60);
    state.clock_from + limit + TIME_LIMIT_SLACK
}

/// A seat reports the end of the match. Close once most seats agree; put it
/// on the leaderboard once every seat in the room agrees.
fn on_result(state: &mut RoomState, seat: u8, report: MatchReport) {
    if !state.started || report.standings.len() > usize::from(state.settings.max_players) {
        return;
    }
    state.results.insert(seat, report.clone());
    let seats: Vec<u8> = state
        .seats
        .iter()
        .filter(|(_, s)| s.tx.is_some())
        .map(|(&k, _)| k)
        .collect();
    let agree = seats
        .iter()
        .filter(|s| state.results.get(s) == Some(&report))
        .count();
    if agree * 2 > seats.len() && state.close_at.is_none() {
        let grace = state.cfg.match_end_grace_secs;
        state.close_at = Some(Instant::now() + Duration::from_secs(grace));
        info!(room_id = %state.id, ?report.winner, "match over");
        let text = format!("Match over - this room closes in {grace}s");
        if let Ok(s) = encode_control(&ControlMsg::Notice { text }) {
            let packet = Bytes::from(s.into_bytes());
            broadcast_all(state, packet.clone());
            broadcast_spectators(state, &packet);
        }
    }
    if state.recorded || agree < seats.len() || seats.len() < 2 {
        return;
    }
    state.recorded = true;
    let rows: Vec<crate::leaderboard::MatchRow> = report
        .standings
        .iter()
        .filter_map(|r| {
            let slot = state.seats.get(&r.seat)?;
            Some(crate::leaderboard::MatchRow {
                name: slot.name.clone(),
                won: report.winner == Some(r.seat),
                frags: r.frags,
                deaths: r.deaths,
            })
        })
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    state.registry.leaderboard().record(&rows, now);
    if state
        .settings
        .game_mod
        .as_ref()
        .is_some_and(|m| m.name == "royale")
    {
        let br_rows: Vec<crate::br_stats::BrMatchRow> = report
            .standings
            .iter()
            .filter_map(|r| {
                let slot = state.seats.get(&r.seat)?;
                Some(crate::br_stats::BrMatchRow {
                    name: slot.name.clone(),
                    place: r.place,
                    won: report.winner == Some(r.seat),
                    kills: r.frags,
                    damage: r.damage,
                })
            })
            .collect();
        state
            .registry
            .br_stats()
            .record_match(crate::br_stats::MODE_BR, &br_rows, now);
    }
}

/// Say goodbye to everyone and stop the actor.
fn close_room(state: &mut RoomState, code: &str, reason: &str) {
    let bye = ControlMsg::Bye {
        code: code.into(),
        reason: reason.into(),
    };
    if let Ok(s) = encode_control(&bye) {
        let packet = Bytes::from(s.into_bytes());
        broadcast_all(state, packet.clone());
        broadcast_spectators(state, &packet);
    }
    info!(room_id = %state.id, code, reason, "room closed");
    state.seats.clear();
    state.conn_seat.clear();
    state.pending_v2.clear();
    state.spectators.clear();
    state.closed = true;
}

fn finish_v2_welcome(state: &mut RoomState, seat: u8) {
    let resume = state
        .seats
        .get(&seat)
        .map(|s| s.resume_token.clone())
        .unwrap_or_default();
    let welcome = ControlMsg::Welcome(WelcomeMsg {
        seat,
        room: welcome_room(state),
        resume_token: resume,
        roster: roster_vec(state),
        media: media_info_for(state, seat),
    });
    send_control_seat(state, seat, &welcome);
    // Deliver any offers already published by peers (mesh signalling).
    for out in state.media.seat_joined(seat) {
        let (target, msg) = match out {
            room_sfu::SignalOut::Offer { seat: t, sdp } => (
                t,
                ControlMsg::Rtc {
                    kind: "offer".into(),
                    sdp: Some(sdp),
                    candidate: None,
                    mid: None,
                },
            ),
            room_sfu::SignalOut::Answer { seat: t, sdp } => (
                t,
                ControlMsg::Rtc {
                    kind: "answer".into(),
                    sdp: Some(sdp),
                    candidate: None,
                    mid: None,
                },
            ),
            room_sfu::SignalOut::Ice {
                seat: t,
                candidate,
                mid,
            } => (
                t,
                ControlMsg::Rtc {
                    kind: "ice".into(),
                    sdp: None,
                    candidate: Some(candidate),
                    mid,
                },
            ),
            room_sfu::SignalOut::Track { seat: t, mid, from } => (
                t,
                ControlMsg::Rtc {
                    kind: "track".into(),
                    sdp: Some(format!("from={from}")),
                    candidate: None,
                    mid: Some(mid),
                },
            ),
        };
        send_control_seat(state, target, &msg);
    }
    push_roster(state);
}

fn media_info_for(state: &mut RoomState, seat: u8) -> MediaInfo {
    #[cfg(feature = "builtin-turn")]
    if let Some(auth) = state.cfg.turn_auth.clone() {
        let username = format!("seat{seat}-{}", short_rand());
        let password = short_rand() + &short_rand();
        let (username, password) = issue_turn_user(&auth.registry, &auth.realm, username, password);
        if let Some(slot) = state.seats.get_mut(&seat) {
            if let Some(prev) = slot.turn_username.take() {
                revoke_turn_user(&auth.registry, &prev);
            }
            slot.turn_username = Some(username.clone());
        }
        let servers = ice_servers_long_term(&auth.stun_urls, &auth.turn_urls, &username, &password);
        return MediaInfo {
            enabled: true,
            ice_servers: servers
                .into_iter()
                .map(|s| IceServerEntry {
                    urls: s.urls,
                    username: s.username,
                    credential: s.credential,
                })
                .collect(),
        };
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let user = format!("seat{seat}");
    let stun_owned = if !state.cfg.stun_url.is_empty() {
        Some(state.cfg.stun_url.clone())
    } else if !state.cfg.stun_bind.is_empty() {
        let host = if state.cfg.turn_public_host.is_empty() {
            "127.0.0.1"
        } else {
            state.cfg.turn_public_host.as_str()
        };
        let port = state.cfg.stun_bind.split(':').next_back().unwrap_or("3478");
        Some(format!("stun:{host}:{port}"))
    } else {
        None
    };
    let turn_host = if state.cfg.turn_host.is_empty() {
        None
    } else {
        Some(state.cfg.turn_host.as_str())
    };
    let turn_secret = if state.cfg.turn_secret.is_empty() {
        None
    } else {
        Some(state.cfg.turn_secret.as_str())
    };
    let servers = ice_servers(stun_owned.as_deref(), turn_host, turn_secret, &user, now);
    MediaInfo {
        enabled: true,
        ice_servers: servers
            .into_iter()
            .map(|s| IceServerEntry {
                urls: s.urls,
                username: s.username,
                credential: s.credential,
            })
            .collect(),
    }
}

fn short_rand() -> String {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn on_rtc(
    state: &mut RoomState,
    seat: u8,
    kind: String,
    sdp: Option<String>,
    candidate: Option<String>,
    mid: Option<String>,
) {
    let signal = match kind.as_str() {
        "offer" => SignalIn::Offer {
            seat,
            sdp: sdp.unwrap_or_default(),
        },
        "answer" => SignalIn::Answer {
            seat,
            sdp: sdp.unwrap_or_default(),
        },
        "ice" => SignalIn::Ice {
            seat,
            candidate: candidate.unwrap_or_default(),
            mid,
        },
        "track" => SignalIn::Track {
            seat,
            mid: mid.unwrap_or_default(),
        },
        _ => return,
    };
    let Ok(outs) = state.media.handle(signal) else {
        return;
    };
    for out in outs {
        let (target, msg) = match out {
            SignalOut::Answer { seat, sdp } => (
                seat,
                ControlMsg::Rtc {
                    kind: "answer".into(),
                    sdp: Some(sdp),
                    candidate: None,
                    mid: None,
                },
            ),
            SignalOut::Offer { seat, sdp } => (
                seat,
                ControlMsg::Rtc {
                    kind: "offer".into(),
                    sdp: Some(sdp),
                    candidate: None,
                    mid: None,
                },
            ),
            SignalOut::Ice {
                seat,
                candidate,
                mid,
            } => (
                seat,
                ControlMsg::Rtc {
                    kind: "ice".into(),
                    sdp: None,
                    candidate: Some(candidate),
                    mid,
                },
            ),
            SignalOut::Track { seat, mid, from } => (
                seat,
                ControlMsg::Rtc {
                    kind: "track".into(),
                    sdp: Some(format!("from={from}")),
                    candidate: None,
                    mid: Some(mid),
                },
            ),
        };
        send_control_seat(state, target, &msg);
    }
}

fn send_reject_tx(tx: &PlayerTx, code: &str, reason: &str) {
    let reject = ControlMsg::Reject(RejectMsg {
        code: code.into(),
        reason: reason.into(),
    });
    if let Ok(s) = encode_control(&reject) {
        let _ = tx.try_send(Bytes::from(s.into_bytes()));
    }
}

fn maybe_v2_start(state: &mut RoomState) {
    if state.started {
        return;
    }
    let humans = live_seat_count(state);
    let quick_due = state.settings.quick_play
        && state
            .quick_play_deadline
            .is_some_and(|d| Instant::now() >= d);
    if quick_due {
        if humans == 0 {
            return;
        }
        for slot in state.seats.values_mut() {
            if slot.away_since.is_none() {
                slot.state = SeatState::Ready;
            }
        }
        state.start_bot_first_seat = humans as u8;
        do_v2_start(state);
        return;
    }
    let need = state.settings.max_players;
    if humans < usize::from(need) {
        return;
    }
    if !state
        .seats
        .values()
        .filter(|s| s.away_since.is_none())
        .all(|s| matches!(s.state, SeatState::Ready))
    {
        return;
    }
    if state.settings.fill_bots {
        state.start_bot_first_seat = humans as u8;
    }
    do_v2_start(state);
}

fn do_v2_start(state: &mut RoomState) {
    if state.started {
        return;
    }
    state.started = true;
    state.clock_from = Instant::now();
    for slot in state.seats.values_mut() {
        slot.state = SeatState::Playing;
    }
    state.v2.set_active_seats(state.settings.max_players);
    state
        .v2
        .set_grace(Duration::from_millis(state.cfg.stall_fill_ms));
    state.v2.reset_for_start();
    state.duke_v2.set_active_seats(state.settings.max_players);
    state
        .duke_v2
        .set_grace(Duration::from_millis(state.cfg.stall_fill_ms));
    state.duke_v2.reset_for_start();
    start_verifier_if_needed(state);
    // Watchers that joined before any player pinned the IWAD would desync.
    let pinned = state.iwad_fingerprint.clone().unwrap_or_default();
    let mismatched: Vec<ConnId> = state
        .spectators
        .iter()
        .filter(|(_, sp)| sp.iwad_fingerprint != pinned)
        .map(|(c, _)| *c)
        .collect();
    for conn in mismatched {
        if let Some(sp) = state.spectators.remove(&conn) {
            send_reject_tx(&sp.tx, "fingerprint", "iwad fingerprint mismatch");
        }
    }
    let start = start_control(state);
    if let Ok(s) = encode_control(&start) {
        let packet = Bytes::from(s.into_bytes());
        broadcast_all(state, packet.clone());
        broadcast_spectators(state, &packet);
    }
    info!(room_id = %state.id, "v2 start");
}

fn start_verifier_if_needed(state: &mut RoomState) {
    #[cfg(feature = "verifier")]
    {
        if !state.settings.enable_verifier {
            return;
        }
        if state.cfg.iwad_path.is_empty() {
            warn!(room_id = %state.id, "verifier enabled but --iwad empty");
            return;
        }
        if state.settings.game == "duke3d" {
            match crate::verifier::spawn_duke_verifier_with_output(
                std::path::PathBuf::from(&state.cfg.iwad_path),
                &state.settings.map,
                state.settings.skill,
                state.settings.max_players,
                state.settings.duke_game_mode,
                state.settings.duke_friendly_fire,
            ) {
                Ok((frames, checksums)) => {
                    state.verifier = Some(VerifierHandles {
                        frames: VerifierFrameSink::Duke(frames),
                        checksums,
                    });
                    info!(room_id = %state.id, "native Duke verifier attached");
                }
                Err(error) => {
                    warn!(room_id = %state.id, %error, "native Duke verifier failed to start")
                }
            }
            return;
        }
        let game_mod = state.settings.game_mod.as_ref().and_then(|m| {
            let entry = state.cfg.mods.get(&m.name)?;
            Some((m.name.clone(), entry.bytes.clone()))
        });
        match crate::verifier::spawn_verifier_with_output(
            std::path::PathBuf::from(&state.cfg.iwad_path),
            game_mod,
            &state.settings.map,
            state.settings.skill,
            state.rng_seed,
            state.settings.max_players,
            u32::from(state.settings.time_limit) * 60 * 35,
        ) {
            Ok((frames, checksums)) => {
                state.verifier = Some(VerifierHandles {
                    frames: VerifierFrameSink::Doom(frames),
                    checksums,
                });
                info!(room_id = %state.id, "Headless verifier attached");
            }
            Err(e) => warn!(room_id = %state.id, %e, "verifier failed to start"),
        }
    }
    #[cfg(not(feature = "verifier"))]
    {
        let _ = state;
    }
}

fn feed_verifier(state: &mut RoomState, frame: &room_proto::TicFrameMsg) {
    #[cfg(feature = "verifier")]
    if let Some(v) = &state.verifier {
        if let VerifierFrameSink::Doom(frames) = &v.frames {
            let _ = frames.try_send(frame.clone());
        }
    }
    #[cfg(not(feature = "verifier"))]
    {
        let _ = (state, frame);
    }
}

fn feed_duke_verifier(state: &mut RoomState, frame: &room_proto::DukeSyncFrameMsg) {
    #[cfg(feature = "verifier")]
    if let Some(v) = &state.verifier {
        if let VerifierFrameSink::Duke(frames) = &v.frames {
            let _ = frames.try_send(frame.clone());
        }
    }
    #[cfg(not(feature = "verifier"))]
    {
        let _ = (state, frame);
    }
}

fn drain_verifier(state: &mut RoomState) {
    #[cfg(feature = "verifier")]
    if let Some(v) = &mut state.verifier {
        while let Ok(c) = v.checksums.try_recv() {
            state.server_checksums.insert(c.tic, c.checksum);
        }
    }
    #[cfg(not(feature = "verifier"))]
    {
        let _ = state;
    }
}

fn pong_bytes(nonce: u64) -> Bytes {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);
    Bytes::from(encode_binary(&BinaryMsg::Pong {
        nonce,
        server_time_us: now,
    }))
}

fn on_v2_binary(state: &mut RoomState, conn: ConnId, msg: BinaryMsg) {
    if let Some(sp) = state.spectators.get(&conn) {
        if let BinaryMsg::Ping { nonce, .. } = msg {
            let _ = sp.tx.try_send(pong_bytes(nonce));
        }
        return;
    }
    let Some(&seat) = state.conn_seat.get(&conn) else {
        return;
    };
    if !state.started {
        return;
    }
    match msg {
        BinaryMsg::DukeTicCmd(cmd) if state.settings.game == "duke3d" => {
            match state.duke_v2.push_cmd_at(seat, cmd, Instant::now()) {
                Ok(Some(frame)) => {
                    feed_duke_verifier(state, &frame);
                    broadcast_duke_tic_frame(state, frame);
                }
                Ok(None) => {}
                Err(error) => {
                    warn!(room_id = %state.id, seat, ?error, "Duke tic cmd rejected");
                }
            }
            for frame in state.duke_v2.clock_emit(Instant::now()) {
                feed_duke_verifier(state, &frame);
                broadcast_duke_tic_frame(state, frame);
            }
        }
        BinaryMsg::TicCmd(cmd) if state.settings.game != "duke3d" => {
            match state.v2.push_cmd(seat, cmd) {
                Ok(Some(frame)) => {
                    feed_verifier(state, &frame);
                    broadcast_tic_frame(state, frame);
                }
                Ok(None) => {}
                Err(e) => {
                    warn!(room_id = %state.id, seat, ?e, "tic cmd rejected");
                }
            }
            // Drain any further completed tics.
            while let Some(frame) = state.v2.try_complete() {
                feed_verifier(state, &frame);
                broadcast_tic_frame(state, frame);
            }
        }
        BinaryMsg::TicCmd(_) => {
            warn!(room_id = %state.id, seat, "Doom tic command rejected in native Duke room");
        }
        BinaryMsg::Ping {
            nonce,
            server_time_us: _,
        } => {
            if let Some(slot) = state.seats.get(&seat) {
                if let Some(tx) = &slot.tx {
                    let _ = tx.try_send(pong_bytes(nonce));
                }
            }
        }
        BinaryMsg::Checksum(c) => {
            on_checksum(state, seat, c.tic, c.checksum);
        }
        _ => {}
    }
}

fn on_checksum(state: &mut RoomState, seat: u8, tic: u32, world_checksum: u64) {
    drain_verifier(state);
    let entry = state.checksums.entry(tic).or_default();
    entry.insert(seat, world_checksum);
    let need = state
        .seats
        .iter()
        .filter(|(_, s)| s.tx.is_some() && s.away_since.is_none())
        .count();
    if entry.len() < need || need == 0 {
        return;
    }
    let values: Vec<(u8, u64)> = entry.iter().map(|(&s, &c)| (s, c)).collect();
    state.checksums.remove(&tic);
    let reference = state
        .server_checksums
        .remove(&tic)
        .or_else(|| values.first().map(|(_, c)| *c));
    let Some(reference) = reference else {
        return;
    };
    let bad: Vec<u8> = values
        .iter()
        .filter(|(_, c)| *c != reference)
        .map(|(s, _)| *s)
        .collect();
    if bad.is_empty() {
        return;
    }
    warn!(room_id = %state.id, tic, ?bad, "desync");
    if let Ok(s) = encode_control(&ControlMsg::Desync { tic, seats: bad }) {
        broadcast_all(state, Bytes::from(s.into_bytes()));
    }
}

fn on_leave(state: &mut RoomState, conn: ConnId, reason: LeaveReason) {
    if let Some(sp) = state.spectators.remove(&conn) {
        info!(room_id = %state.id, conn = conn.0, name = %sp.name, ?reason, "spectator left");
        return;
    }
    if state.pending_v2.remove(&conn).is_some() {
        info!(room_id = %state.id, conn = conn.0, ?reason, "pending v2 leave");
        return;
    }
    let Some(seat) = state.conn_seat.remove(&conn) else {
        return;
    };
    let Some(slot) = state.seats.get_mut(&seat) else {
        return;
    };
    let name = slot.name.clone();
    let turn_user = slot.turn_username.take();
    // Reserve seat for resume while still in lobby/loading.
    if !state.started {
        // Keep TURN creds while the seat is reserved for resume.
        slot.turn_username = turn_user;
        slot.tx = None;
        slot.away_since = Some(Instant::now());
        slot.state = SeatState::Away;
        info!(room_id = %state.id, seat, %name, ?reason, "seat reserved for resume");
        push_roster(state);
        return;
    }
    state.seats.remove(&seat);
    #[cfg(feature = "builtin-turn")]
    if let (Some(auth), Some(user)) = (&state.cfg.turn_auth, turn_user) {
        revoke_turn_user(&auth.registry, &user);
    }
    info!(room_id = %state.id, seat, %name, ?reason, "leave");
    let packet = Bytes::from(pack_peer_leave(PeerId(seat)));
    broadcast_all(state, packet);
    push_roster(state);
}

fn on_tick(state: &mut RoomState) {
    expire_reserved(state);
    drain_verifier(state);
    if !state.started && state.settings.quick_play {
        maybe_v2_start(state);
    }
    if state.started {
        on_clock(state);
    }
    pump_spectators(state);
    let now = Instant::now();
    if state.close_at.is_some_and(|t| now >= t) {
        close_room(state, "over", "match over");
    } else if now >= time_limit_deadline(state) {
        close_room(state, "over", "time limit reached");
    }
}

/// Release every v2 frame due on the 35 Hz clock.
fn on_clock(state: &mut RoomState) {
    if !state.started {
        return;
    }
    if state.settings.game == "duke3d" {
        for frame in state.duke_v2.clock_emit(Instant::now()) {
            feed_duke_verifier(state, &frame);
            broadcast_duke_tic_frame(state, frame);
        }
        return;
    }
    for frame in state.v2.clock_emit(Instant::now()) {
        feed_verifier(state, &frame);
        broadcast_tic_frame(state, frame);
    }
}

fn broadcast_duke_tic_frame(state: &mut RoomState, frame: room_proto::DukeSyncFrameMsg) {
    let bin = Bytes::from(encode_binary(&BinaryMsg::DukeTicFrame(frame)));
    broadcast_all(state, bin.clone());
    // Duke frames use their own sync-input format; the Doom TIC_BATCH replay
    // log cannot encode them. Live Duke spectators receive the stream directly.
    broadcast_spectators(state, &bin);
}

fn broadcast_tic_frame(state: &mut RoomState, frame: room_proto::TicFrameMsg) {
    state.registry.note_tic_frame();
    if let Some(us) = state.v2.last_relay_us.take() {
        state.registry.note_relay_us(us);
    }
    let bin = Bytes::from(encode_binary(&BinaryMsg::TicFrame(frame)));
    broadcast_all(state, bin.clone());
    spectate_frame(state, &bin);
}

fn on_admin(state: &mut RoomState, cmd: AdminCmd) {
    match cmd {
        AdminCmd::Kick { seat, reason } => {
            let conn = state
                .conn_seat
                .iter()
                .find(|(_, s)| **s == seat)
                .map(|(c, _)| *c);
            send_control_seat(
                state,
                seat,
                &ControlMsg::Bye {
                    code: "kicked".into(),
                    reason: reason.clone(),
                },
            );
            if let Some(conn) = conn {
                on_leave(state, conn, LeaveReason::Kick(reason));
            } else {
                state.seats.remove(&seat);
                push_roster(state);
            }
        }
        AdminCmd::Mute {
            seat,
            audio,
            video,
            chat,
        } => {
            if let Some(slot) = state.seats.get_mut(&seat) {
                slot.muted_audio = audio;
                slot.muted_video = video;
                slot.muted_chat = chat;
            }
            let _ = state.media.set_mute(seat, audio, video);
            let text = format!("seat {seat} mute audio={audio} video={video} chat={chat}");
            if let Ok(s) = encode_control(&ControlMsg::Notice { text }) {
                broadcast_all(state, Bytes::from(s.into_bytes()));
            }
            push_roster(state);
        }
        AdminCmd::Notice { text } => {
            let text = text.chars().take(500).collect::<String>();
            if let Ok(s) = encode_control(&ControlMsg::Notice { text }) {
                let packet = Bytes::from(s.into_bytes());
                broadcast_all(state, packet.clone());
                broadcast_spectators(state, &packet);
            }
        }
        AdminCmd::Close { reason } => close_room(state, "closed", &reason),
        AdminCmd::Inspect { reply } => {
            let _ = reply.send(build_room_inspect(state));
        }
    }
}

fn seat_state_str(s: SeatState) -> &'static str {
    match s {
        SeatState::Lobby => "lobby",
        SeatState::Loading => "loading",
        SeatState::Ready => "ready",
        SeatState::Playing => "playing",
        SeatState::Away => "away",
    }
}

fn build_room_inspect(state: &RoomState) -> RoomInspect {
    let seats = state
        .seats
        .iter()
        .map(|(&seat, slot)| {
            let state_label = if slot.away_since.is_some() {
                "away"
            } else {
                seat_state_str(slot.state)
            };
            SeatInspect {
                seat,
                name: slot.name.clone(),
                state: state_label.into(),
                remote_ip: if slot.remote_ip.is_empty() {
                    "—".into()
                } else {
                    slot.remote_ip.clone()
                },
                resume_token: slot.resume_token.clone(),
                ticket_id: if slot.ticket_id.is_empty() {
                    "—".into()
                } else {
                    slot.ticket_id.clone()
                },
                iwad_fingerprint: if slot.iwad_fingerprint.is_empty() {
                    "—".into()
                } else {
                    slot.iwad_fingerprint.clone()
                },
                turn_username: slot.turn_username.clone().unwrap_or_default(),
                muted_audio: slot.muted_audio,
                muted_video: slot.muted_video,
                muted_chat: slot.muted_chat,
                pose: slot.pose.clone(),
            }
        })
        .collect();
    RoomInspect {
        id: state.id.clone(),
        map: state.settings.map.clone(),
        skill: state.settings.skill,
        max_players: state.settings.max_players,
        rng_seed: state.rng_seed,
        started: state.started,
        iwad_fingerprint: state.iwad_fingerprint.clone(),
        enable_verifier: state.settings.enable_verifier,
        game_mod: state.settings.game_mod.as_ref().map(|m| m.name.clone()),
        packs: state.settings.packs.clone(),
        occupied: state.occupied.load(std::sync::atomic::Ordering::Relaxed),
        seats,
        spectators: state
            .spectators
            .values()
            .map(|sp| SpectatorInspect {
                name: sp.name.clone(),
                remote_ip: sp.remote_ip.clone(),
                frames_sent: sp.sent,
            })
            .collect(),
        outline: state.map_outline.clone(),
    }
}

fn broadcast_except(state: &RoomState, except_seat: u8, packet: Bytes) {
    for (&seat, slot) in &state.seats {
        if seat == except_seat {
            continue;
        }
        if let Some(tx) = &slot.tx {
            let _ = tx.try_send(packet.clone());
        }
    }
}

fn broadcast_all(state: &RoomState, packet: Bytes) {
    for slot in state.seats.values() {
        if let Some(tx) = &slot.tx {
            let _ = tx.try_send(packet.clone());
        }
    }
}

fn send_control_seat(state: &RoomState, seat: u8, msg: &ControlMsg) {
    let Ok(s) = encode_control(msg) else {
        return;
    };
    if let Some(slot) = state.seats.get(&seat) {
        if let Some(tx) = &slot.tx {
            let _ = tx.try_send(Bytes::from(s.into_bytes()));
        }
    }
}

fn roster_vec(state: &RoomState) -> Vec<RosterSeat> {
    state
        .seats
        .iter()
        .map(|(&seat, s)| RosterSeat {
            seat,
            name: s.name.clone(),
            state: s.state,
            muted: s.muted_audio || s.muted_video,
        })
        .collect()
}

fn free_seat(state: &RoomState) -> Option<u8> {
    (0..state.settings.max_players).find(|s| !state.seats.contains_key(s))
}

fn live_seat_count(state: &RoomState) -> usize {
    state
        .seats
        .values()
        .filter(|s| s.away_since.is_none())
        .count()
}

fn publish_lobby_stats(state: &RoomState) {
    use std::sync::atomic::Ordering;
    state
        .occupied
        .store(live_seat_count(state), Ordering::Relaxed);
    state.started_flag.store(state.started, Ordering::Relaxed);
}

fn expire_reserved(state: &mut RoomState) {
    let now = Instant::now();
    let expired: Vec<u8> = state
        .seats
        .iter()
        .filter_map(|(&seat, s)| {
            s.away_since.and_then(|t| {
                if now.duration_since(t) >= RESUME_GRACE {
                    Some(seat)
                } else {
                    None
                }
            })
        })
        .collect();
    for seat in expired {
        state.seats.remove(&seat);
        info!(room_id = %state.id, seat, "resume grace expired");
    }
}

fn push_roster(state: &RoomState) {
    let msg = ControlMsg::Roster {
        seats: roster_vec(state),
    };
    if let Ok(s) = encode_control(&msg) {
        let packet = Bytes::from(s.into_bytes());
        broadcast_all(state, packet.clone());
        broadcast_spectators(state, &packet);
    }
}

fn parse_hello_name(rest: &[u8]) -> String {
    if rest.len() < 2 {
        return "player".into();
    }
    let n = u16::from_le_bytes([rest[0], rest[1]]) as usize;
    let end = 2 + n.min(rest.len().saturating_sub(2));
    String::from_utf8_lossy(&rest[2..end])
        .chars()
        .take(24)
        .collect()
}

/// Helper used by HTTP create.
pub fn make_create_response(id: &str, settings: RoomSettings, rng_seed: u32) -> CreateRoomResponse {
    CreateRoomResponse {
        id: id.to_owned(),
        ws_legacy: format!("/ws/{id}"),
        ws_v2: format!("/v2/rooms/{id}/ws"),
        settings,
        rng_seed,
    }
}

/// Generate seed for a new room.
pub fn new_rng_seed() -> u32 {
    random_u32().unwrap_or(1)
}
