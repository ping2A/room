//! Web lockstep coop over [`crate::ws_net::WsTransport`] + `room-ws` relay.

use crate::ws_net::WsTransport;
use room_input::{InputSample, Key, build_ticcmd};
use room_net::{
    ABI_MAJOR, ENGINE_VERSION, Hello, LobbyHost, LobbyMsg, LockstepSession, NetTicCmd, PeerId,
    TimedCmd, Transport,
};
use room_sim::{Buttons, TicCmd};
use std::collections::BTreeSet;

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
    /// Host RNG seed (seat 0); joiners take seed from lobby Welcome/Start.
    pub rng_seed: u32,
}

/// Ready lockstep session + match settings (hand off to [`crate::play::PlaySession`]).
pub struct NetReady {
    /// Transport.
    pub transport: WsTransport,
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
}

/// Connecting / lobby phases before the sim starts.
pub struct NetLobby {
    cfg: NetJoinConfig,
    transport: Option<WsTransport>,
    local: Option<PeerId>,
    host: Option<LobbyHost>,
    welcomed: bool,
    started_seed: Option<u32>,
    /// Frames since last joiner HELLO (retry until Welcome — packets can race).
    hello_age: u32,
}

impl NetLobby {
    /// Begin WebSocket connect + HELLO.
    pub fn begin(cfg: NetJoinConfig) -> Result<Self, String> {
        let transport = WsTransport::start(&cfg.url, &cfg.name, cfg.players)?;
        Ok(Self {
            cfg,
            transport: Some(transport),
            local: None,
            host: None,
            welcomed: false,
            started_seed: None,
            hello_age: 0,
        })
    }

    /// Status for the HTML overlay.
    #[must_use]
    pub fn status(&self) -> String {
        match self.local {
            None => format!(
                "Connecting to {} ({} players)…",
                self.cfg.url, self.cfg.players
            ),
            Some(seat) => {
                let n = self.host.as_ref().map(|h| h.seats().len()).unwrap_or(1);
                format!(
                    "Coop lobby seat {} — {}/{} · waiting for peers (demo until start)",
                    seat.0, n, self.cfg.players
                )
            }
        }
    }

    /// Drive connect / lobby. `Some` when lockstep can start (consumes the socket).
    pub fn poll(&mut self) -> Result<Option<NetReady>, String> {
        let Some(transport) = self.transport.as_mut() else {
            return Ok(None);
        };

        if self.local.is_none() {
            if let Some(seat) = transport.try_seat()? {
                if seat.0 >= self.cfg.players {
                    return Err(format!(
                        "relay assigned seat {} but this match only has {} seats — restart room-ws",
                        seat.0, self.cfg.players
                    ));
                }
                web_sys::console::log_1(
                    &format!(
                        "room-web: relay seat {} / {}",
                        seat.0, self.cfg.players
                    )
                    .into(),
                );
                self.local = Some(seat);
                if seat.0 == 0 {
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
                // Fall through and process any inbox packets already queued.
            } else {
                return Ok(None);
            }
        }
        let local = self.local.unwrap_or(PeerId(0));

        // Joiners: re-send HELLO until Welcome (desktop TCP client does the same).
        if !self.welcomed && local.0 != 0 {
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
        transport.poll(&mut raw);
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
                                &format!("room-web: rejected peer {}: {}", from.0, r.reason)
                                    .into(),
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
                    return Err(format!("lobby rejected: {}", r.reason));
                }
                Ok(LobbyMsg::Start(s)) => {
                    self.started_seed = Some(s.rng_seed);
                }
                Ok(_) => {}
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

        if local.0 == 0 {
            if let Some(h) = self.host.as_ref() {
                if h.seats().len() >= usize::from(self.cfg.players) && self.started_seed.is_none()
                {
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
            let mut session = LockstepSession::new(local, peers, 1);
            for i in 0..peer_count {
                session.push_remote(TimedCmd {
                    tic: 0,
                    peer: PeerId(i),
                    cmd: NetTicCmd::default(),
                });
            }
            web_sys::console::log_1(
                &format!(
                    "room-web: lockstep start seat={} peers={} seed={seed}",
                    local.0, peer_count
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
            }));
        }
        Ok(None)
    }
}

/// Live lockstep driver (after lobby).
pub struct NetPlay {
    transport: WsTransport,
    session: LockstepSession,
    /// Peer count.
    pub peer_count: u8,
    /// Submitted cmd awaiting a full frame (rebroadcast until advance).
    pending: Option<TimedCmd>,
    frames_since_rebroadcast: u32,
}

impl NetPlay {
    /// Build from a finished lobby.
    #[must_use]
    pub fn from_ready(r: NetReady) -> Self {
        Self {
            transport: r.transport,
            session: r.session,
            peer_count: r.peer_count,
            pending: None,
            frames_since_rebroadcast: 0,
        }
    }

    /// Submit local input once, then try to release the next lockstep tic.
    ///
    /// Returns `None` while waiting on remotes (caller must not burn another
    /// local submit or sim tic).
    pub fn try_advance(
        &mut self,
        held: &BTreeSet<&'static str>,
        attack: bool,
        weapon: u8,
        mouse_yaw: i32,
        mouse_pitch: i32,
    ) -> Option<Vec<TicCmd>> {
        if self.pending.is_none() {
            let sample = sample_input(held, attack, weapon, mouse_yaw, mouse_pitch);
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
            self.session.broadcast(&mut self.transport, &msg);
            self.pending = Some(msg);
            self.frames_since_rebroadcast = 0;
        }

        self.session.recv(&mut self.transport);
        self.frames_since_rebroadcast = self.frames_since_rebroadcast.saturating_add(1);
        if self.frames_since_rebroadcast >= 3 {
            if let Some(msg) = self.pending {
                self.session.broadcast(&mut self.transport, &msg);
            }
            self.frames_since_rebroadcast = 0;
        }

        let frame = self.session.try_advance()?;
        self.pending = None;
        let mut cmds = vec![TicCmd::default(); usize::from(self.peer_count)];
        for (peer, net) in frame {
            let i = usize::from(peer.0);
            if i < cmds.len() {
                cmds[i] = TicCmd {
                    buttons: Buttons(net.buttons),
                    pitch: net.pitch,
                    yaw: net.yaw,
                    roll: net.roll,
                    forward_move: net.forward_move,
                    side_move: net.side_move,
                    up_move: net.up_move,
                    weapon_select: net.weapon_select,
                };
            }
        }
        Some(cmds)
    }
}

/// Packet-server uplink: one send fans out to every other seat.
fn uplink(transport: &mut WsTransport, local: PeerId, bytes: &[u8]) {
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

fn sample_input(
    held: &BTreeSet<&'static str>,
    attack: bool,
    weapon: u8,
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
    match weapon {
        1 => keys.push(Key::Weapon1),
        2 => keys.push(Key::Weapon2),
        3 => keys.push(Key::Weapon3),
        4 => keys.push(Key::Weapon4),
        5 => keys.push(Key::Weapon5),
        6 => keys.push(Key::Weapon6),
        7 => keys.push(Key::Weapon7),
        _ => {}
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
