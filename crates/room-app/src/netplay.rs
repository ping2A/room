//! Lockstep coop (`--host` / `--join`) over UDP, TCP, or TCP packet-server.
//! Optional Noise_XX encryption via `--noise`.

use room_net::{
    ABI_MAJOR, ENGINE_VERSION, Hello, LobbyHost, LobbyMsg, LockstepSession, NetTicCmd, NoiseKeys,
    NoiseTransport, PeerId, TcpRelayClient, TcpRelayServer, TcpTransport, TimedCmd, Transport,
    UdpTransport,
};
use room_sim::{Buttons, TicCmd};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Wire transport selected by `--transport`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum NetTransportKind {
    /// Native UDP peer-to-peer (default; GZDoom-like).
    #[default]
    Udp,
    /// TCP peer-to-peer mesh (length-prefixed frames).
    Tcp,
    /// TCP packet-server / relay on the host (more players; one reachable address).
    #[value(name = "tcp-server")]
    TcpServer,
}

/// Host lobby parameters.
pub struct HostOpts<'a> {
    /// Bind address.
    pub bind_addr: SocketAddr,
    /// Seat count (clamped per transport).
    pub max_players: u8,
    /// Load-order fingerprint.
    pub fingerprint: u64,
    /// Shared RNG seed.
    pub rng_seed: u32,
    /// Start map.
    pub map: &'a str,
    /// Skill 1..=5.
    pub skill: u8,
    /// Wire transport.
    pub kind: NetTransportKind,
    /// Wrap lockstep with Noise_XX.
    pub noise: bool,
}

/// Active lockstep session after lobby completes.
pub struct NetPlay {
    /// Local seat (`0` = host).
    pub local: PeerId,
    transport: Box<dyn Transport>,
    session: LockstepSession,
    /// Total seats.
    pub peer_count: u8,
    /// Shared RNG seed from lobby.
    pub rng_seed: u32,
    /// Seat display names from lobby start (BE3).
    pub seat_names: Vec<String>,
}

impl NetPlay {
    /// Host: bind, wait for joiners, broadcast start.
    pub fn host(opts: HostOpts<'_>) -> Result<Self, String> {
        let max_players = match opts.kind {
            NetTransportKind::TcpServer => opts.max_players.clamp(2, 8),
            _ => opts.max_players.clamp(2, 4),
        };
        match opts.kind {
            NetTransportKind::Udp => Self::host_udp(
                opts.bind_addr,
                max_players,
                opts.fingerprint,
                opts.rng_seed,
                opts.map,
                opts.skill,
                opts.noise,
            ),
            NetTransportKind::Tcp => Self::host_tcp(
                opts.bind_addr,
                max_players,
                opts.fingerprint,
                opts.rng_seed,
                opts.map,
                opts.skill,
                opts.noise,
            ),
            NetTransportKind::TcpServer => Self::host_tcp_server(
                opts.bind_addr,
                max_players,
                opts.fingerprint,
                opts.rng_seed,
                opts.map,
                opts.skill,
                opts.noise,
            ),
        }
    }

    /// Join host at `host_addr`. Returns session + map name from welcome.
    pub fn join(
        host_addr: SocketAddr,
        fingerprint: u64,
        name: &str,
        kind: NetTransportKind,
        noise: bool,
        max_players: u8,
    ) -> Result<(Self, String), String> {
        match kind {
            NetTransportKind::Udp => Self::join_udp(host_addr, fingerprint, name, noise),
            NetTransportKind::Tcp => Self::join_tcp(host_addr, fingerprint, name, noise),
            NetTransportKind::TcpServer => {
                Self::join_tcp_server(host_addr, fingerprint, name, noise, max_players)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn host_udp(
        bind_addr: SocketAddr,
        max_players: u8,
        fingerprint: u64,
        rng_seed: u32,
        map: &str,
        skill: u8,
        noise: bool,
    ) -> Result<Self, String> {
        let mut transport = UdpTransport::bind(PeerId(0), bind_addr).map_err(|e| e.to_string())?;
        let local_addr = transport.local_addr().map_err(|e| e.to_string())?;
        eprintln!(
            "room: hosting {max_players}-player UDP coop on {local_addr} (map={map} skill={skill})"
        );

        let mut lobby = LobbyHost::new(fingerprint, max_players, 1, rng_seed, map, skill);
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut known_addrs = std::collections::BTreeSet::new();
        let mut welcome_by_addr = std::collections::BTreeMap::<SocketAddr, Vec<u8>>::new();

        while lobby.seats().len() < usize::from(max_players) {
            if Instant::now() > deadline {
                return Err("host lobby timed out waiting for players".into());
            }
            let mut raw = Vec::new();
            transport.poll_raw(&mut raw);
            for (addr, bytes) in raw {
                if let Ok(LobbyMsg::Hello(hello)) = LobbyMsg::decode(&bytes) {
                    if known_addrs.contains(&addr) {
                        if let Some(prev) = welcome_by_addr.get(&addr) {
                            let _ = transport.send_raw(addr, prev);
                        }
                        continue;
                    }
                    let reply = lobby.on_hello(hello);
                    let encoded = reply.encode();
                    let _ = transport.send_raw(addr, &encoded);
                    if let LobbyMsg::Welcome(w) = &reply {
                        known_addrs.insert(addr);
                        welcome_by_addr.insert(addr, encoded);
                        transport.add_peer(w.peer_id, addr);
                        eprintln!(
                            "room: peer {} joined from {addr} ({}/{})",
                            w.peer_id.0,
                            lobby.seats().len(),
                            max_players
                        );
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let start_msg = lobby.start_msg();
        let seat_names = match &start_msg {
            LobbyMsg::Start(s) => s.names.clone(),
            _ => Vec::new(),
        };
        let start = start_msg.encode();
        for &addr in &known_addrs {
            let _ = transport.send_raw(addr, &start);
        }

        let peers: Vec<PeerId> = lobby.seats().iter().map(|(id, _)| *id).collect();
        let mut session = LockstepSession::new(PeerId(0), peers.clone(), 1);
        seed_tic0(&mut session, max_players);
        let transport = maybe_noise(transport, PeerId(0), &peers, noise)?;

        Ok(Self {
            local: PeerId(0),
            transport,
            session,
            peer_count: max_players,
            rng_seed,
            seat_names,
        })
    }

    fn join_udp(
        host_addr: SocketAddr,
        fingerprint: u64,
        name: &str,
        noise: bool,
    ) -> Result<(Self, String), String> {
        let bind = SocketAddr::from(([0, 0, 0, 0], 0));
        let mut transport = UdpTransport::bind(PeerId(254), bind).map_err(|e| e.to_string())?;
        let local_addr = transport.local_addr().map_err(|e| e.to_string())?;
        let hello = LobbyMsg::Hello(Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR,
            fingerprint,
            name: name.into(),
        });
        eprintln!("room: joining UDP {host_addr} from {local_addr} …");

        let deadline = Instant::now() + Duration::from_secs(60);
        let mut welcome = None;
        let mut start_seed = None;
        let mut seat_names = Vec::new();
        while welcome.is_none() || start_seed.is_none() {
            if Instant::now() > deadline {
                return Err("join lobby timed out".into());
            }
            if welcome.is_none() {
                let _ = transport.send_raw(host_addr, &hello.encode());
            }
            let mut raw = Vec::new();
            transport.poll_raw(&mut raw);
            for (_addr, bytes) in raw {
                match LobbyMsg::decode(&bytes) {
                    Ok(LobbyMsg::Welcome(w)) => {
                        eprintln!(
                            "room: welcomed as peer {}/{} map={} seed={}",
                            w.peer_id.0, w.peer_count, w.map, w.rng_seed
                        );
                        welcome = Some(w);
                    }
                    Ok(LobbyMsg::Reject(r)) => {
                        return Err(format!("host rejected: {}", r.reason));
                    }
                    Ok(LobbyMsg::Start(s)) => {
                        start_seed = Some(s.rng_seed);
                        seat_names = s.names;
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let w = welcome.ok_or_else(|| "missing welcome".to_string())?;
        let rng_seed = start_seed.unwrap_or(w.rng_seed);
        let map = w.map.clone();
        let peer_count = w.peer_count;
        let local = w.peer_id;

        transport.set_self_id(local);
        transport.add_peer(PeerId(0), host_addr);

        let peers: Vec<PeerId> = (0..peer_count).map(PeerId).collect();
        let mut session = LockstepSession::new(local, peers.clone(), 1);
        seed_tic0(&mut session, peer_count);
        let transport = maybe_noise(transport, local, &peers, noise)?;

        Ok((
            Self {
                local,
                transport,
                session,
                peer_count,
                rng_seed,
                seat_names,
            },
            map,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn host_tcp(
        bind_addr: SocketAddr,
        max_players: u8,
        fingerprint: u64,
        rng_seed: u32,
        map: &str,
        skill: u8,
        noise: bool,
    ) -> Result<Self, String> {
        let mut transport =
            TcpTransport::bind(PeerId(0), bind_addr, max_players).map_err(|e| e.to_string())?;
        let local_addr = transport.local_addr().map_err(|e| e.to_string())?;
        eprintln!(
            "room: hosting {max_players}-player TCP coop on {local_addr} (map={map} skill={skill})"
        );

        let mut lobby = LobbyHost::new(fingerprint, max_players, 1, rng_seed, map, skill);
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut welcomed = std::collections::BTreeSet::<PeerId>::new();

        while lobby.seats().len() < usize::from(max_players) {
            if Instant::now() > deadline {
                return Err("host lobby timed out waiting for players".into());
            }
            let accepted = transport.accept_pending().map_err(|e| e.to_string())?;
            for id in accepted {
                // Provisional seat: wait for Hello on that stream.
                eprintln!("room: TCP accept seat {}", id.0);
                let _ = id;
            }
            let mut raw = Vec::new();
            transport.poll(&mut raw);
            for (from, bytes) in raw {
                if welcomed.contains(&from) {
                    continue;
                }
                if let Ok(LobbyMsg::Hello(hello)) = LobbyMsg::decode(&bytes) {
                    let reply = lobby.on_hello_at(from, hello);
                    let encoded = reply.encode();
                    transport.send(from, &encoded);
                    if let LobbyMsg::Welcome(w) = &reply {
                        welcomed.insert(from);
                        eprintln!(
                            "room: peer {} joined via TCP ({}/{})",
                            w.peer_id.0,
                            lobby.seats().len(),
                            max_players
                        );
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let start_msg = lobby.start_msg();
        let seat_names = match &start_msg {
            LobbyMsg::Start(s) => s.names.clone(),
            _ => Vec::new(),
        };
        let start = start_msg.encode();
        for &id in &welcomed {
            transport.send(id, &start);
        }

        let peers: Vec<PeerId> = lobby.seats().iter().map(|(id, _)| *id).collect();
        let mut session = LockstepSession::new(PeerId(0), peers.clone(), 1);
        seed_tic0(&mut session, max_players);
        let transport = maybe_noise(transport, PeerId(0), &peers, noise)?;

        Ok(Self {
            local: PeerId(0),
            transport,
            session,
            peer_count: max_players,
            rng_seed,
            seat_names,
        })
    }

    fn join_tcp(
        host_addr: SocketAddr,
        fingerprint: u64,
        name: &str,
        noise: bool,
    ) -> Result<(Self, String), String> {
        // Seat is assigned by host Welcome; connect claiming temporary peer 1 until welcome.
        let mut transport =
            TcpTransport::connect(PeerId(1), PeerId(0), host_addr).map_err(|e| e.to_string())?;
        let hello = LobbyMsg::Hello(Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR,
            fingerprint,
            name: name.into(),
        });
        eprintln!("room: joining TCP {host_addr} …");

        let deadline = Instant::now() + Duration::from_secs(60);
        let mut welcome = None;
        let mut start_seed = None;
        let mut seat_names = Vec::new();
        while welcome.is_none() || start_seed.is_none() {
            if Instant::now() > deadline {
                return Err("join lobby timed out".into());
            }
            if welcome.is_none() {
                transport.send(PeerId(0), &hello.encode());
            }
            let mut raw = Vec::new();
            transport.poll(&mut raw);
            for (_from, bytes) in raw {
                match LobbyMsg::decode(&bytes) {
                    Ok(LobbyMsg::Welcome(w)) => {
                        eprintln!(
                            "room: welcomed as peer {}/{} map={} seed={}",
                            w.peer_id.0, w.peer_count, w.map, w.rng_seed
                        );
                        welcome = Some(w);
                    }
                    Ok(LobbyMsg::Reject(r)) => {
                        return Err(format!("host rejected: {}", r.reason));
                    }
                    Ok(LobbyMsg::Start(s)) => {
                        start_seed = Some(s.rng_seed);
                        seat_names = s.names;
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let w = welcome.ok_or_else(|| "missing welcome".to_string())?;
        let rng_seed = start_seed.unwrap_or(w.rng_seed);
        let map = w.map.clone();
        let peer_count = w.peer_count;
        let local = w.peer_id;

        let peers: Vec<PeerId> = (0..peer_count).map(PeerId).collect();
        let mut session = LockstepSession::new(local, peers.clone(), 1);
        seed_tic0(&mut session, peer_count);
        let transport = maybe_noise(transport, local, &peers, noise)?;

        Ok((
            Self {
                local,
                transport,
                session,
                peer_count,
                rng_seed,
                seat_names,
            },
            map,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn host_tcp_server(
        bind_addr: SocketAddr,
        max_players: u8,
        fingerprint: u64,
        rng_seed: u32,
        map: &str,
        skill: u8,
        noise: bool,
    ) -> Result<Self, String> {
        let mut server = TcpRelayServer::bind(bind_addr, max_players).map_err(|e| e.to_string())?;
        let local_addr = server.local_addr().map_err(|e| e.to_string())?;
        eprintln!(
            "room: hosting {max_players}-player TCP packet-server on {local_addr} (map={map} skill={skill})"
        );

        let mut lobby = LobbyHost::new(fingerprint, max_players, 1, rng_seed, map, skill);
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut welcomed = std::collections::BTreeSet::<PeerId>::new();

        // Wait until all client seats connected + Hello accepted (host seat already counted).
        while lobby.seats().len() < usize::from(max_players) {
            if Instant::now() > deadline {
                return Err("host lobby timed out waiting for players".into());
            }
            let _ = server.pump();
            let mut raw = Vec::new();
            // Drain host inbox without double-pump: use Transport::poll.
            Transport::poll(&mut server, &mut raw);
            for (from, bytes) in raw {
                if welcomed.contains(&from) {
                    continue;
                }
                if let Ok(LobbyMsg::Hello(hello)) = LobbyMsg::decode(&bytes) {
                    let reply = lobby.on_hello_at(from, hello);
                    let encoded = reply.encode();
                    server.send(from, &encoded);
                    if let LobbyMsg::Welcome(w) = &reply {
                        welcomed.insert(from);
                        eprintln!(
                            "room: peer {} joined via relay ({}/{})",
                            w.peer_id.0,
                            lobby.seats().len(),
                            max_players
                        );
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let start_msg = lobby.start_msg();
        let seat_names = match &start_msg {
            LobbyMsg::Start(s) => s.names.clone(),
            _ => Vec::new(),
        };
        server.broadcast_host(&start_msg.encode());

        let peers: Vec<PeerId> = lobby.seats().iter().map(|(id, _)| *id).collect();
        let mut session = LockstepSession::new(PeerId(0), peers.clone(), 1);
        seed_tic0(&mut session, max_players);
        let transport = maybe_noise(server, PeerId(0), &peers, noise)?;

        Ok(Self {
            local: PeerId(0),
            transport,
            session,
            peer_count: max_players,
            rng_seed,
            seat_names,
        })
    }

    fn join_tcp_server(
        host_addr: SocketAddr,
        fingerprint: u64,
        name: &str,
        noise: bool,
        max_players: u8,
    ) -> Result<(Self, String), String> {
        let max_players = max_players.clamp(2, 8);
        // Relay handshake needs the host pumping; spawn a short retry loop.
        let mut client =
            TcpRelayClient::connect(host_addr, name, max_players).map_err(|e| e.to_string())?;
        eprintln!(
            "room: joined TCP relay as seat {} at {host_addr}",
            client.self_id().0
        );

        let hello = LobbyMsg::Hello(Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR,
            fingerprint,
            name: name.into(),
        });

        let deadline = Instant::now() + Duration::from_secs(60);
        let mut welcome = None;
        let mut start_seed = None;
        let mut seat_names = Vec::new();
        while welcome.is_none() || start_seed.is_none() {
            if Instant::now() > deadline {
                return Err("join lobby timed out".into());
            }
            if welcome.is_none() {
                // Fan-out dedupe uses lowest remote seat (0).
                client.send(PeerId(0), &hello.encode());
            }
            let mut raw = Vec::new();
            client.poll(&mut raw);
            for (_from, bytes) in raw {
                match LobbyMsg::decode(&bytes) {
                    Ok(LobbyMsg::Welcome(w)) => {
                        eprintln!(
                            "room: welcomed as peer {}/{} map={} seed={}",
                            w.peer_id.0, w.peer_count, w.map, w.rng_seed
                        );
                        welcome = Some(w);
                    }
                    Ok(LobbyMsg::Reject(r)) => {
                        return Err(format!("host rejected: {}", r.reason));
                    }
                    Ok(LobbyMsg::Start(s)) => {
                        start_seed = Some(s.rng_seed);
                        seat_names = s.names;
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let w = welcome.ok_or_else(|| "missing welcome".to_string())?;
        let rng_seed = start_seed.unwrap_or(w.rng_seed);
        let map = w.map.clone();
        let peer_count = w.peer_count;
        let local = client.self_id();

        let peers: Vec<PeerId> = (0..peer_count).map(PeerId).collect();
        let mut session = LockstepSession::new(local, peers.clone(), 1);
        seed_tic0(&mut session, peer_count);
        let transport = maybe_noise(client, local, &peers, noise)?;

        Ok((
            Self {
                local,
                transport,
                session,
                peer_count,
                rng_seed,
                seat_names,
            },
            map,
        ))
    }

    /// Submit local cmd; wait for all peers; return seat-ordered [`TicCmd`]s.
    pub fn advance_tic(&mut self, local_cmd: NetTicCmd) -> Result<Vec<TicCmd>, String> {
        self.advance_tic_with_checksum(local_cmd, None)
    }

    fn advance_tic_with_checksum(
        &mut self,
        local_cmd: NetTicCmd,
        checksum: Option<(u32, u64)>,
    ) -> Result<Vec<TicCmd>, String> {
        let msg = self.session.submit_local(local_cmd);
        if let Some((tic, value)) = checksum {
            self.session.report_checksum(tic, self.local, value);
        }
        self.session
            .broadcast_with_checksum(&mut *self.transport, &msg, checksum);
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut last_rebroadcast = Instant::now();
        loop {
            self.session.recv(&mut *self.transport);
            if let Some(frame) = self.session.try_advance() {
                let completed_tic = self.session.next_tic().saturating_sub(1);
                if self.session.checksums_complete(completed_tic) {
                    self.session
                        .check_desync(completed_tic)
                        .map_err(|error| error.to_string())?;
                }
                let mut cmds = vec![TicCmd::default(); usize::from(self.peer_count)];
                for (peer, net) in frame {
                    let i = usize::from(peer.0);
                    if i < cmds.len() {
                        cmds[i] = net_to_tic(net);
                    }
                }
                return Ok(cmds);
            }
            if last_rebroadcast.elapsed() > Duration::from_millis(50) {
                if checksum.is_some() {
                    self.session
                        .broadcast_with_checksum(&mut *self.transport, &msg, checksum);
                } else {
                    self.session.broadcast(&mut *self.transport, &msg);
                }
                last_rebroadcast = Instant::now();
            }
            if Instant::now() > deadline {
                return Err(format!("lockstep stall at tic {}", self.session.next_tic()));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Submit a native Duke sync input and return the seat-ordered inputs for
    /// the same lockstep tic. The existing transport record has fields with
    /// matching widths, so this mapping preserves all ten bytes of Duke input.
    pub fn advance_duke_tic(
        &mut self,
        input: room_duke::DukeInput,
        checksum: Option<(u32, u64)>,
    ) -> Result<Vec<room_duke::DukeInput>, String> {
        let frame = self.advance_tic_with_checksum(
            NetTicCmd {
                buttons: input.bits,
                pitch: i16::from(input.horz),
                yaw: i16::from(input.avel),
                roll: 0,
                forward_move: input.fvel,
                side_move: input.svel,
                up_move: 0,
                weapon_select: 0,
            },
            checksum,
        )?;
        Ok(frame
            .into_iter()
            .map(|cmd| room_duke::DukeInput {
                fvel: cmd.forward_move,
                svel: cmd.side_move,
                avel: cmd.yaw.clamp(i16::from(i8::MIN), i16::from(i8::MAX)) as i8,
                horz: cmd.pitch.clamp(i16::from(i8::MIN), i16::from(i8::MAX)) as i8,
                bits: cmd.buttons.0,
            })
            .collect())
    }

    /// Broadcast one presentation-only Atomic remote-ridicule sound request.
    pub fn broadcast_duke_rts(&mut self, index: usize) -> Result<(), String> {
        if index >= 10 {
            return Err("Duke RTS index must be 0..=9".to_owned());
        }
        self.session
            .broadcast_auxiliary(&mut *self.transport, &[b'R', index as u8])
            .map_err(|error| error.to_string())
    }

    /// Drain validated native Duke RTS requests from connected peers.
    pub fn drain_duke_rts(&mut self) -> Vec<(PeerId, usize)> {
        self.session
            .drain_auxiliary()
            .into_iter()
            .filter_map(|(peer, payload)| {
                if payload.len() != 2 || payload[0] != b'R' || !(1..=10).contains(&payload[1]) {
                    return None;
                }
                Some((peer, usize::from(payload[1] - 1)))
            })
            .collect()
    }
}

fn maybe_noise<T: Transport + 'static>(
    inner: T,
    local: PeerId,
    peers: &[PeerId],
    noise: bool,
) -> Result<Box<dyn Transport>, String> {
    if !noise {
        return Ok(Box::new(inner));
    }
    eprintln!("room: enabling Noise_XX ({})", room_net::NOISE_PATTERN);
    let keys = NoiseKeys::generate().map_err(|e| e.to_string())?;
    let mut nt = NoiseTransport::new(inner, local, keys);
    nt.complete_handshake(peers, Instant::now() + Duration::from_secs(15))
        .map_err(|e| e.to_string())?;
    Ok(Box::new(nt))
}

fn seed_tic0(session: &mut LockstepSession, peer_count: u8) {
    for i in 0..peer_count {
        session.push_remote(TimedCmd {
            tic: 0,
            peer: PeerId(i),
            cmd: NetTicCmd::default(),
        });
    }
}

fn net_to_tic(n: NetTicCmd) -> TicCmd {
    TicCmd {
        buttons: Buttons(n.buttons),
        pitch: n.pitch,
        yaw: n.yaw,
        roll: n.roll,
        forward_move: n.forward_move,
        side_move: n.side_move,
        up_move: n.up_move,
        weapon_select: n.weapon_select,
    }
}

/// Convert a playsim [`TicCmd`] to wire form.
#[must_use]
pub fn tic_to_net(c: TicCmd) -> NetTicCmd {
    NetTicCmd {
        buttons: c.buttons.0,
        pitch: c.pitch,
        yaw: c.yaw,
        roll: c.roll,
        forward_move: c.forward_move,
        side_move: c.side_move,
        up_move: c.up_move,
        weapon_select: c.weapon_select,
    }
}

/// Resolve bind address (`5029` or `0.0.0.0:5029`).
pub fn parse_host_bind(s: &str) -> Result<SocketAddr, String> {
    if let Ok(port) = s.parse::<u16>() {
        return Ok(SocketAddr::from(([0, 0, 0, 0], port)));
    }
    s.to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or_else(|| format!("cannot resolve {s}"))
}

/// Resolve join address (`127.0.0.1:5029`).
pub fn parse_join_addr(s: &str) -> Result<SocketAddr, String> {
    s.to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or_else(|| format!("cannot resolve {s}"))
}

/// Lobby fingerprint from IWAD path + map + skill.
#[must_use]
pub fn session_fingerprint(iwad: &str, map: &str, skill: u8) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    iwad.hash(&mut h);
    map.to_ascii_uppercase().hash(&mut h);
    skill.hash(&mut h);
    h.finish()
}
