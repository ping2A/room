//! Lockstep coop (`--host` / `--join`) over UDP, TCP, or TCP packet-server.
//! Optional Noise_XX encryption via `--noise`.

use room_net::{
    ABI_MAJOR, ENGINE_VERSION, Hello, LobbyHost, LobbyMsg, LockstepSession, NetTicCmd, NoiseKeys,
    NoiseTransport, PeerId, Ping, Pong, TcpRelayClient, TcpRelayServer, TcpTransport, TimedCmd,
    Transport, UdpTransport, delay_for_rtts,
};
use room_sim::{Buttons, TicCmd};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Wire transport selected by `--transport`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum NetTransportKind {
    /// Native UDP peer-to-peer (default; GZDoom-like).
    #[default]
    Udp,
    /// TCP peer-to-peer mesh (length-prefixed frames).
    Tcp,
    /// TCP packet-server / relay on the host (more players; one reachable address).
    #[cfg_attr(feature = "cli", value(name = "tcp-server"))]
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
    /// Deathmatch instead of co-op (sent to joiners in Welcome).
    pub deathmatch: bool,
    /// Bot seats after the player seats.
    pub bots: u8,
}

/// Active lockstep session after lobby completes.
pub struct NetPlay {
    /// Local seat (`0` = host).
    pub local: PeerId,
    transport: Box<dyn Transport + Send>,
    session: LockstepSession,
    /// Total seats.
    pub peer_count: u8,
    /// Shared RNG seed from lobby.
    pub rng_seed: u32,
    /// Seat display names from lobby start (BE3).
    pub seat_names: Vec<String>,
    /// Match rules from the host: deathmatch (else co-op).
    pub deathmatch: bool,
    /// Bot seats after the player seats, simulated by every peer.
    pub bots: u8,
    /// First tic where peers' world checksums disagreed.
    desync_tic: Option<u32>,
    /// Whether [`Self::take_desync`] already handed the desync out.
    desync_reported: bool,
    /// Rollback play (`cl_rollback`): window in tics, `0` = blocking lockstep.
    rollback_window: u32,
    /// Rollback: world checksum to send with the next local command.
    rollback_checksum: Option<(u32, u64)>,
    /// Rollback: frames released but not yet applied by the caller.
    rollback_applied: u32,
    /// Rollback: local tics in a row that waited for a full window.
    rollback_waits: u32,
}

impl NetPlay {
    /// Host: bind, wait for joiners, broadcast start.
    pub fn host(opts: HostOpts<'_>) -> Result<Self, String> {
        reset_lobby();
        let max_players = match opts.kind {
            NetTransportKind::TcpServer => opts.max_players.clamp(2, 64),
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
                (opts.deathmatch, opts.bots),
            ),
            NetTransportKind::Tcp => Self::host_tcp(
                opts.bind_addr,
                max_players,
                opts.fingerprint,
                opts.rng_seed,
                opts.map,
                opts.skill,
                opts.noise,
                (opts.deathmatch, opts.bots),
            ),
            NetTransportKind::TcpServer => Self::host_tcp_server(
                opts.bind_addr,
                max_players,
                opts.fingerprint,
                opts.rng_seed,
                opts.map,
                opts.skill,
                opts.noise,
                (opts.deathmatch, opts.bots),
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
        reset_lobby();
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
        rules: (bool, u8),
    ) -> Result<Self, String> {
        let mut transport = UdpTransport::bind(PeerId(0), bind_addr).map_err(|e| e.to_string())?;
        let local_addr = transport.local_addr().map_err(|e| e.to_string())?;
        eprintln!(
            "room: hosting {max_players}-player UDP coop on {local_addr} (map={map} skill={skill})"
        );

        let mut lobby = LobbyHost::new(
            fingerprint,
            max_players,
            net_input_delay(),
            rng_seed,
            map,
            skill,
        );
        lobby.set_rules(rules.0, rules.1);
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut known_addrs = std::collections::BTreeSet::new();
        let mut welcome_by_addr = std::collections::BTreeMap::<SocketAddr, Vec<u8>>::new();

        while lobby.seats().len() < usize::from(max_players) {
            if lobby_cancelled() {
                return Err(LOBBY_CANCELLED.into());
            }
            if Instant::now() > deadline {
                return Err("host lobby timed out waiting for players".into());
            }
            set_lobby_progress(lobby.seats().len(), max_players);
            let mut raw = Vec::new();
            transport.poll_raw(&mut raw);
            for (addr, bytes) in raw {
                // Players on the same network looking for games (docs/41 M7).
                if crate::lan::is_probe(&bytes) {
                    let seats = u8::try_from(lobby.seats().len()).unwrap_or(u8::MAX);
                    let _ = transport.send_raw(
                        addr,
                        &crate::lan::reply(seats, max_players, fingerprint, map),
                    );
                    continue;
                }
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

        let mut probe = PingProbe::new();
        while !probe.finished(lobby.seats()) {
            if let Some(ping) = probe.ping_due() {
                for &addr in &known_addrs {
                    let _ = transport.send_raw(addr, &ping);
                }
            }
            let mut raw = Vec::new();
            transport.poll_raw(&mut raw);
            for (_addr, bytes) in raw {
                probe.on_bytes(&bytes);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let delay = probe.negotiated_delay();
        let start_msg = lobby.start_msg_with_delay(delay);
        let seat_names = match &start_msg {
            LobbyMsg::Start(s) => s.names.clone(),
            _ => Vec::new(),
        };
        let start = start_msg.encode();
        // UDP may drop it: send a few copies (joiners act on the first).
        for _ in 0..3 {
            for &addr in &known_addrs {
                let _ = transport.send_raw(addr, &start);
            }
        }

        let peers: Vec<PeerId> = lobby.seats().iter().map(|(id, _)| *id).collect();
        let mut session = LockstepSession::new(PeerId(0), peers.clone(), delay);
        seed_tic0(&mut session, max_players);
        let transport = maybe_noise(transport, PeerId(0), &peers, noise)?;

        Ok(Self {
            local: PeerId(0),
            transport,
            session,
            peer_count: max_players,
            rng_seed,
            seat_names,
            desync_tic: None,
            desync_reported: false,
            deathmatch: rules.0,
            bots: rules.1,
            rollback_window: 0,
            rollback_checksum: None,
            rollback_applied: 0,
            rollback_waits: 0,
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
        let mut start_delay: Option<u32> = None;
        let mut seat_names = Vec::new();
        while welcome.is_none() || start_seed.is_none() {
            if lobby_cancelled() {
                return Err(LOBBY_CANCELLED.into());
            }
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
                        start_delay = s.delay;
                        seat_names = s.names;
                    }
                    Ok(LobbyMsg::Ping(Ping { nonce })) => {
                        // The host measures our round trip to pick the delay.
                        if let Some(w) = welcome.as_ref() {
                            let pong = LobbyMsg::Pong(Pong {
                                peer_id: w.peer_id,
                                nonce,
                            })
                            .encode();
                            let _ = transport.send_raw(host_addr, &pong);
                        }
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
        // The host's delay (sent in Welcome) wins: every seat must agree.
        let mut session = LockstepSession::new(
            local,
            peers.clone(),
            joined_delay(start_delay.unwrap_or(w.delay)),
        );
        let rules_from_welcome = (w.deathmatch != 0, w.bots);
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
                desync_tic: None,
                desync_reported: false,
                deathmatch: rules_from_welcome.0,
                bots: rules_from_welcome.1,
                rollback_window: 0,
                rollback_checksum: None,
                rollback_applied: 0,
                rollback_waits: 0,
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
        rules: (bool, u8),
    ) -> Result<Self, String> {
        let mut transport =
            TcpTransport::bind(PeerId(0), bind_addr, max_players).map_err(|e| e.to_string())?;
        let local_addr = transport.local_addr().map_err(|e| e.to_string())?;
        eprintln!(
            "room: hosting {max_players}-player TCP coop on {local_addr} (map={map} skill={skill})"
        );

        let mut lobby = LobbyHost::new(
            fingerprint,
            max_players,
            net_input_delay(),
            rng_seed,
            map,
            skill,
        );
        lobby.set_rules(rules.0, rules.1);
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut welcomed = std::collections::BTreeSet::<PeerId>::new();

        while lobby.seats().len() < usize::from(max_players) {
            set_lobby_progress(lobby.seats().len(), max_players);
            if lobby_cancelled() {
                return Err(LOBBY_CANCELLED.into());
            }
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

        let mut probe = PingProbe::new();
        while !probe.finished(lobby.seats()) {
            if let Some(ping) = probe.ping_due() {
                for &id in &welcomed {
                    transport.send(id, &ping);
                }
            }
            let mut raw = Vec::new();
            transport.poll(&mut raw);
            for (_from, bytes) in raw {
                probe.on_bytes(&bytes);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let delay = probe.negotiated_delay();
        let start_msg = lobby.start_msg_with_delay(delay);
        let seat_names = match &start_msg {
            LobbyMsg::Start(s) => s.names.clone(),
            _ => Vec::new(),
        };
        let start = start_msg.encode();
        for &id in &welcomed {
            transport.send(id, &start);
        }

        let peers: Vec<PeerId> = lobby.seats().iter().map(|(id, _)| *id).collect();
        let mut session = LockstepSession::new(PeerId(0), peers.clone(), delay);
        seed_tic0(&mut session, max_players);
        let transport = maybe_noise(transport, PeerId(0), &peers, noise)?;

        Ok(Self {
            local: PeerId(0),
            transport,
            session,
            peer_count: max_players,
            rng_seed,
            seat_names,
            desync_tic: None,
            desync_reported: false,
            deathmatch: rules.0,
            bots: rules.1,
            rollback_window: 0,
            rollback_checksum: None,
            rollback_applied: 0,
            rollback_waits: 0,
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
        let mut start_delay: Option<u32> = None;
        let mut seat_names = Vec::new();
        while welcome.is_none() || start_seed.is_none() {
            if lobby_cancelled() {
                return Err(LOBBY_CANCELLED.into());
            }
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
                        start_delay = s.delay;
                        seat_names = s.names;
                    }
                    Ok(LobbyMsg::Ping(Ping { nonce })) => {
                        // The host measures our round trip to pick the delay.
                        if let Some(w) = welcome.as_ref() {
                            let pong = LobbyMsg::Pong(Pong {
                                peer_id: w.peer_id,
                                nonce,
                            })
                            .encode();
                            transport.send(PeerId(0), &pong);
                        }
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
        // The host's delay (sent in Welcome) wins: every seat must agree.
        let mut session = LockstepSession::new(
            local,
            peers.clone(),
            joined_delay(start_delay.unwrap_or(w.delay)),
        );
        let rules_from_welcome = (w.deathmatch != 0, w.bots);
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
                desync_tic: None,
                desync_reported: false,
                deathmatch: rules_from_welcome.0,
                bots: rules_from_welcome.1,
                rollback_window: 0,
                rollback_checksum: None,
                rollback_applied: 0,
                rollback_waits: 0,
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
        rules: (bool, u8),
    ) -> Result<Self, String> {
        let mut server = TcpRelayServer::bind(bind_addr, max_players).map_err(|e| e.to_string())?;
        let local_addr = server.local_addr().map_err(|e| e.to_string())?;
        eprintln!(
            "room: hosting {max_players}-player TCP packet-server on {local_addr} (map={map} skill={skill})"
        );

        let mut lobby = LobbyHost::new(
            fingerprint,
            max_players,
            net_input_delay(),
            rng_seed,
            map,
            skill,
        );
        lobby.set_rules(rules.0, rules.1);
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut welcomed = std::collections::BTreeSet::<PeerId>::new();

        // Wait until all client seats connected + Hello accepted (host seat already counted).
        while lobby.seats().len() < usize::from(max_players) {
            set_lobby_progress(lobby.seats().len(), max_players);
            if lobby_cancelled() {
                return Err(LOBBY_CANCELLED.into());
            }
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

        let mut probe = PingProbe::new();
        while !probe.finished(lobby.seats()) {
            let _ = server.pump();
            if let Some(ping) = probe.ping_due() {
                server.broadcast_host(&ping);
            }
            let mut raw = Vec::new();
            Transport::poll(&mut server, &mut raw);
            for (_from, bytes) in raw {
                probe.on_bytes(&bytes);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let delay = probe.negotiated_delay();
        let start_msg = lobby.start_msg_with_delay(delay);
        let seat_names = match &start_msg {
            LobbyMsg::Start(s) => s.names.clone(),
            _ => Vec::new(),
        };
        server.broadcast_host(&start_msg.encode());

        let peers: Vec<PeerId> = lobby.seats().iter().map(|(id, _)| *id).collect();
        let mut session = LockstepSession::new(PeerId(0), peers.clone(), delay);
        seed_tic0(&mut session, max_players);
        let transport = maybe_noise(server, PeerId(0), &peers, noise)?;

        Ok(Self {
            local: PeerId(0),
            transport,
            session,
            peer_count: max_players,
            rng_seed,
            seat_names,
            desync_tic: None,
            desync_reported: false,
            deathmatch: rules.0,
            bots: rules.1,
            rollback_window: 0,
            rollback_checksum: None,
            rollback_applied: 0,
            rollback_waits: 0,
        })
    }

    fn join_tcp_server(
        host_addr: SocketAddr,
        fingerprint: u64,
        name: &str,
        noise: bool,
        max_players: u8,
    ) -> Result<(Self, String), String> {
        let max_players = max_players.clamp(2, 64);
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
        let mut start_delay: Option<u32> = None;
        let mut seat_names = Vec::new();
        while welcome.is_none() || start_seed.is_none() {
            if lobby_cancelled() {
                return Err(LOBBY_CANCELLED.into());
            }
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
                        start_delay = s.delay;
                        seat_names = s.names;
                    }
                    Ok(LobbyMsg::Ping(Ping { nonce })) => {
                        // The host measures our round trip to pick the delay.
                        if let Some(w) = welcome.as_ref() {
                            let pong = LobbyMsg::Pong(Pong {
                                peer_id: w.peer_id,
                                nonce,
                            })
                            .encode();
                            client.send(PeerId(0), &pong);
                        }
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
        // The host's delay (sent in Welcome) wins: every seat must agree.
        let mut session = LockstepSession::new(
            local,
            peers.clone(),
            joined_delay(start_delay.unwrap_or(w.delay)),
        );
        let rules_from_welcome = (w.deathmatch != 0, w.bots);
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
                desync_tic: None,
                desync_reported: false,
                deathmatch: rules_from_welcome.0,
                bots: rules_from_welcome.1,
                rollback_window: 0,
                rollback_checksum: None,
                rollback_applied: 0,
                rollback_waits: 0,
            },
            map,
        ))
    }

    /// Local commands sent but not released yet, oldest first (for
    /// `cl_predict`).
    pub fn pending_local_cmds(&self) -> Vec<TicCmd> {
        self.session
            .pending_local()
            .into_iter()
            .map(net_to_tic)
            .collect()
    }

    /// Submit the local command, wait for every peer's, and return the
    /// seat-ordered frame (blocking lockstep). Reports the Doom world checksum
    /// every [`DOOM_CHECKSUM_INTERVAL`] lockstep tics so peers catch a
    /// desync. `world_checksum` runs only when one is due and must describe
    /// the world after every frame released so far (before this one runs).
    pub fn advance_doom_tic(
        &mut self,
        local_cmd: NetTicCmd,
        world_checksum: impl FnOnce() -> Option<u64>,
    ) -> Result<Vec<TicCmd>, String> {
        let tic = self.session.next_tic();
        let checksum = if tic % DOOM_CHECKSUM_INTERVAL == 0 {
            world_checksum().map(|value| (tic, value))
        } else {
            None
        };
        self.advance_tic_with_checksum(local_cmd, checksum)
    }

    /// Turn rollback play on (`window` tics, 1..=16) or off (`0`). Each
    /// peer picks it on its own: the wire format and the confirmed game are
    /// the same either way, only how far ahead this machine runs differs.
    pub fn set_rollback_window(&mut self, window: u32) {
        let window = window.min(16);
        if window > 0 && self.rollback_window == 0 {
            // Frames released so far were all run by the caller.
            self.rollback_applied = self.session.next_tic();
        }
        self.rollback_window = window;
        self.session.set_rollback_window(window);
    }

    /// Rollback play is on.
    pub fn rollback(&self) -> bool {
        self.rollback_window > 0
    }

    /// Rollback, one local tic: send `local_cmd` (unless local commands
    /// already run a full window ahead, then wait for the others), and
    /// return every frame that is now complete, oldest first. Never blocks
    /// otherwise. Call [`Self::rollback_frame_applied`] after running each.
    pub fn rollback_tic(&mut self, local_cmd: NetTicCmd) -> Vec<Vec<TicCmd>> {
        self.session.recv(&mut *self.transport);
        if self.session.local_lead() < self.rollback_window {
            let msg = self.session.submit_local(local_cmd);
            let checksum = self.rollback_checksum.take();
            if let Some((tic, value)) = checksum {
                self.session.report_checksum(tic, self.local, value);
            }
            self.session
                .broadcast_with_checksum(&mut *self.transport, &msg, checksum);
            self.rollback_waits = 0;
        } else {
            // Window full: the others are late. Resend ours in case they
            // were lost, and wait (this tic's input is dropped, as a stall
            // would have delayed it).
            self.rollback_waits += 1;
            if self.rollback_waits % 2 == 0 {
                self.session.rebroadcast_recent(&mut *self.transport, None);
            }
        }
        self.session.recv(&mut *self.transport);
        let mut out = Vec::new();
        while let Some(frame) = self.session.try_advance() {
            if let Err(room_net::NetError::Desync { tic }) = self.session.check_complete_desyncs() {
                self.desync_tic.get_or_insert(tic);
                eprintln!("room: netplay DESYNC: peers disagree at tic {tic}");
            }
            let mut cmds = vec![TicCmd::default(); usize::from(self.peer_count)];
            for (peer, net) in frame {
                if let Some(slot) = cmds.get_mut(usize::from(peer.0)) {
                    *slot = net_to_tic(net);
                }
            }
            out.push(cmds);
        }
        out
    }

    /// Rollback: the caller ran one released frame on the confirmed game.
    /// `world_checksum` (that game's `World::checksum`) is read every
    /// [`DOOM_CHECKSUM_INTERVAL`] tics and goes out with the next command.
    pub fn rollback_frame_applied(&mut self, world_checksum: impl FnOnce() -> Option<u64>) {
        self.rollback_applied = self.rollback_applied.saturating_add(1);
        let tic = self.rollback_applied;
        if tic % DOOM_CHECKSUM_INTERVAL == 0
            && let Some(value) = world_checksum()
        {
            self.rollback_checksum = Some((tic, value));
        }
    }

    /// Rollback: the frames from the next unconfirmed tic to the newest
    /// local command, with guessed commands for peers not heard from yet.
    pub fn speculative_frames(&self) -> Vec<Vec<TicCmd>> {
        self.session
            .speculative_frames()
            .into_iter()
            .map(|(frame, _)| {
                let mut cmds = vec![TicCmd::default(); usize::from(self.peer_count)];
                for (peer, net) in frame {
                    if let Some(slot) = cmds.get_mut(usize::from(peer.0)) {
                        *slot = net_to_tic(net);
                    }
                }
                cmds
            })
            .collect()
    }

    /// First tic where peers' world checksums disagreed, once.
    pub fn take_desync(&mut self) -> Option<u32> {
        if self.desync_reported {
            return None;
        }
        let tic = self.desync_tic?;
        self.desync_reported = true;
        Some(tic)
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
                if let Err(room_net::NetError::Desync { tic }) =
                    self.session.check_complete_desyncs()
                {
                    // Keep playing (the frame is already released) and let
                    // the caller warn the players once.
                    self.desync_tic.get_or_insert(tic);
                    eprintln!("room: netplay DESYNC: peers disagree at tic {tic}");
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
                // Resend the recent window, not just `msg`: a command lost
                // a few tics ago is what a stalled peer may be waiting on.
                self.session
                    .rebroadcast_recent(&mut *self.transport, checksum);
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

/// Doom netplay reports a world checksum once a second.
pub const DOOM_CHECKSUM_INTERVAL: u32 = 35;

fn maybe_noise<T: Transport + Send + 'static>(
    inner: T,
    local: PeerId,
    peers: &[PeerId],
    noise: bool,
) -> Result<Box<dyn Transport + Send>, String> {
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

/// Lockstep input delay in tics: `ROOM_NET_DELAY` (1..=8, default 1). Every
/// peer must use the same value. More delay hides network latency without
/// stalls; client prediction (`cl_predict 1`) hides the delay itself.
pub fn net_input_delay() -> u32 {
    std::env::var("ROOM_NET_DELAY")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(1)
        .clamp(1, 8)
}

/// Measures the host's round trip to every joined seat once the lobby is
/// full, to pick the input delay ([`delay_for_rtts`]) unless
/// `ROOM_NET_DELAY` sets it.
struct PingProbe {
    began: Instant,
    last_sent: Option<Instant>,
    /// Best (lowest) round trip per seat, ms, and how many answers.
    best: std::collections::BTreeMap<u8, (u32, u32)>,
}

impl PingProbe {
    /// Answers wanted from every seat.
    const SAMPLES: u32 = 4;
    /// Give up waiting for slow or silent seats after this long.
    const MAX_WAIT: Duration = Duration::from_millis(1500);

    fn new() -> Self {
        Self {
            began: Instant::now(),
            last_sent: None,
            best: std::collections::BTreeMap::new(),
        }
    }

    fn now_ms(&self) -> u32 {
        u32::try_from(self.began.elapsed().as_millis()).unwrap_or(u32::MAX)
    }

    /// Encoded ping when one is due (every 40 ms).
    fn ping_due(&mut self) -> Option<Vec<u8>> {
        if self
            .last_sent
            .is_some_and(|at| at.elapsed() < Duration::from_millis(40))
        {
            return None;
        }
        self.last_sent = Some(Instant::now());
        Some(
            LobbyMsg::Ping(Ping {
                nonce: self.now_ms(),
            })
            .encode(),
        )
    }

    fn on_bytes(&mut self, bytes: &[u8]) {
        if let Ok(LobbyMsg::Pong(Pong { peer_id, nonce })) = LobbyMsg::decode(bytes) {
            let rtt = self.now_ms().saturating_sub(nonce);
            let e = self.best.entry(peer_id.0).or_insert((u32::MAX, 0));
            e.0 = e.0.min(rtt);
            e.1 += 1;
        }
    }

    /// Every joined seat answered enough, ping is forced, or time is up.
    fn finished(&self, seats: &[(PeerId, String)]) -> bool {
        if std::env::var_os("ROOM_NET_DELAY").is_some() || self.began.elapsed() > Self::MAX_WAIT {
            return true;
        }
        seats.iter().filter(|(id, _)| id.0 != 0).all(|(id, _)| {
            self.best
                .get(&id.0)
                .is_some_and(|(_, n)| *n >= Self::SAMPLES)
        })
    }

    /// `ROOM_NET_DELAY` when set, else from the measured round trips.
    fn negotiated_delay(&self) -> u32 {
        if std::env::var_os("ROOM_NET_DELAY").is_some() {
            return net_input_delay();
        }
        let rtts: Vec<u32> = self.best.values().map(|(rtt, _)| *rtt).collect();
        let delay = delay_for_rtts(&rtts);
        eprintln!("room: round trips {rtts:?} ms → input delay {delay}");
        delay
    }
}

/// Input delay a joiner uses: the host's, from Welcome (`1..=8`). An old
/// host that sent `1` while this side set `ROOM_NET_DELAY` still gets `1`,
/// since every seat has to agree.
fn joined_delay(host_delay: u32) -> u32 {
    let delay = host_delay.clamp(1, 8);
    if delay != net_input_delay() && std::env::var_os("ROOM_NET_DELAY").is_some() {
        eprintln!("room: using the host's input delay {delay} (ROOM_NET_DELAY ignored)");
    }
    delay
}

/// Neutral commands for every seat for the tics before the first local input
/// lands (`0..delay`).
fn seed_tic0(session: &mut LockstepSession, peer_count: u8) {
    for tic in 0..session.delay() {
        for i in 0..peer_count {
            session.push_remote(TimedCmd {
                tic,
                peer: PeerId(i),
                cmd: NetTicCmd::default(),
            });
        }
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

/// Lobby fingerprint: game file, mods in load order, map and skill. Files
/// count by name (case-insensitive), not by folder, so a phone and a desktop
/// keeping the same game in different places match. FNV-1a, so the value
/// does not depend on the Rust version either build was made with.
#[must_use]
pub fn session_fingerprint(iwad: &str, mods: &[String], map: &str, skill: u8) -> u64 {
    fn file_name(path: &str) -> String {
        path.rsplit(['/', '\\'])
            .next()
            .unwrap_or(path)
            .to_ascii_lowercase()
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for &b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // Field separator, so "ab"+"c" differs from "a"+"bc".
        h ^= 0xff;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    feed(file_name(iwad).as_bytes());
    for m in mods {
        feed(file_name(m).as_bytes());
    }
    feed(map.to_ascii_uppercase().as_bytes());
    feed(&[skill]);
    h
}

/// The lobby fingerprint for a start configuration: game, mods, map and
/// skill as [`crate::app`] uses them (the launcher compares it with games
/// found on the network).
#[must_use]
pub fn config_fingerprint(args: &crate::config::AppConfig) -> u64 {
    let iwad = args
        .iwad
        .as_ref()
        .map_or_else(|| "builtin".to_owned(), |p| p.display().to_string());
    let map = if args.native_duke {
        format!(
            "{}|duke-mode={}|duke-friendly-fire={}",
            args.map, args.duke_game_mode, args.duke_friendly_fire
        )
    } else {
        args.map.clone()
    };
    let mods: Vec<String> = args
        .file
        .iter()
        .chain(&args.grp)
        .map(|p| p.display().to_string())
        .collect();
    session_fingerprint(&iwad, &mods, &map, args.skill.clamp(1, 5))
}

/// Error text of a lobby left with [`cancel_lobby`].
pub const LOBBY_CANCELLED: &str = "cancelled";

/// Bumped by [`cancel_lobby`]; a lobby is cancelled once it differs from
/// the value its thread saw when it started.
static LOBBY_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

thread_local! {
    static LOBBY_STARTED_AT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
/// `seats << 8 | max` while a host waits for players.
static LOBBY_PROGRESS: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

/// `ROOM_TEST_AWAY="frame:seconds"` → `(frame, seconds)`.
pub fn parse_away_test(v: &str) -> Option<(u64, f32)> {
    let (frame, secs) = v.split_once(':')?;
    let frame = frame.trim().parse().ok()?;
    let secs: f32 = secs.trim().parse().ok()?;
    (secs > 0.0 && secs.is_finite()).then_some((frame, secs))
}

/// Stop the host or join wait running on another thread (the launcher's
/// Cancel). It returns [`LOBBY_CANCELLED`] within a few milliseconds.
/// Lobbies started after the call are not affected.
pub fn cancel_lobby() {
    LOBBY_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn lobby_cancelled() -> bool {
    LOBBY_GENERATION.load(std::sync::atomic::Ordering::Relaxed) != LOBBY_STARTED_AT.get()
}

fn reset_lobby() {
    LOBBY_STARTED_AT.set(LOBBY_GENERATION.load(std::sync::atomic::Ordering::Relaxed));
    LOBBY_PROGRESS.store(0, std::sync::atomic::Ordering::Relaxed);
}

fn set_lobby_progress(seats: usize, max: u8) {
    let seats = u16::try_from(seats).unwrap_or(255).min(255);
    LOBBY_PROGRESS.store(
        seats << 8 | u16::from(max),
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// `(players in, players waited for)` while hosting; `None` before the
/// lobby is up.
#[must_use]
pub fn lobby_progress() -> Option<(u8, u8)> {
    let v = LOBBY_PROGRESS.load(std::sync::atomic::Ordering::Relaxed);
    let [seats, max] = v.to_be_bytes();
    (max > 0).then_some((seats, max))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {

    #[test]
    fn away_test_hook_parses() {
        assert_eq!(parse_away_test("120:5"), Some((120, 5.0)));
        assert_eq!(parse_away_test(" 3 : 0.5 "), Some((3, 0.5)));
        assert_eq!(parse_away_test("120"), None);
        assert_eq!(parse_away_test("x:5"), None);
        assert_eq!(parse_away_test("1:0"), None);
    }

    use super::*;

    fn free_port(udp: bool) -> SocketAddr {
        if udp {
            std::net::UdpSocket::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
        } else {
            std::net::TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
        }
    }

    /// Lobby tests run one at a time: cancelling one must not hit another.
    static LOBBY_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Host and joiner agree on the delay the host measured (loopback: 1).
    fn lobby_agrees_on_the_delay(kind: NetTransportKind, udp: bool) {
        if std::env::var_os("ROOM_NET_DELAY").is_some() {
            return;
        }
        let _one_at_a_time = LOBBY_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let addr = free_port(udp);
        let host = std::thread::spawn(move || {
            NetPlay::host(HostOpts {
                bind_addr: addr,
                max_players: 2,
                fingerprint: 7,
                rng_seed: 99,
                map: "MAP01",
                skill: 3,
                kind,
                noise: false,
                deathmatch: true,
                bots: 2,
            })
            .map(|host| host.session.delay())
        });
        std::thread::sleep(Duration::from_millis(100));
        let (joined, map) = NetPlay::join(addr, 7, "Bob", kind, false, 2).expect("join");
        let host_delay = host.join().unwrap().expect("host");
        assert_eq!(map, "MAP01");
        assert_eq!(host_delay, joined.session.delay());
        assert_eq!(host_delay, 1, "loopback ping is tiny");
        assert!(joined.deathmatch, "rules come from the host");
        assert_eq!(joined.bots, 2);
        assert_eq!(joined.rng_seed, 99);
    }

    #[test]
    fn udp_lobby_negotiates_the_input_delay() {
        lobby_agrees_on_the_delay(NetTransportKind::Udp, true);
    }

    #[test]
    fn tcp_relay_lobby_negotiates_the_input_delay() {
        lobby_agrees_on_the_delay(NetTransportKind::TcpServer, false);
    }

    fn udp_host(addr: SocketAddr) -> std::thread::JoinHandle<Result<NetPlay, String>> {
        std::thread::spawn(move || {
            NetPlay::host(HostOpts {
                bind_addr: addr,
                max_players: 2,
                fingerprint: 42,
                rng_seed: 1,
                map: "MAP01",
                skill: 3,
                kind: NetTransportKind::Udp,
                noise: false,
                deathmatch: false,
                bots: 0,
            })
        })
    }

    #[test]
    fn a_waiting_udp_host_answers_lan_probes_and_can_be_cancelled() {
        let _one_at_a_time = LOBBY_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let addr = free_port(true);
        let host = udp_host(addr);
        let found = crate::lan::discover(&[addr], Duration::from_millis(700)).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].addr, addr);
        assert_eq!((found[0].players, found[0].max_players), (1, 2));
        assert_eq!((found[0].fingerprint, found[0].map.as_str()), (42, "MAP01"));
        let asked = Instant::now();
        cancel_lobby();
        let result = host.join().unwrap();
        assert_eq!(result.err().as_deref(), Some(LOBBY_CANCELLED));
        assert!(asked.elapsed() < Duration::from_secs(1), "stops at once");
        // A lobby started after the cancel is not affected: it gets a player.
        let addr = free_port(true);
        let host = udp_host(addr);
        std::thread::sleep(Duration::from_millis(100));
        let (joined, _) =
            NetPlay::join(addr, 42, "Ann", NetTransportKind::Udp, false, 2).expect("join");
        assert!(host.join().unwrap().is_ok());
        assert_eq!(joined.peer_count, 2);
    }

    #[test]
    fn the_fingerprint_ignores_folders_but_not_mods() {
        let phone = session_fingerprint(
            "/data/user/0/io.github.roomengine.room/files/bundle/freedoom2.wad",
            &["/data/x/brutal.pk3".into()],
            "map01",
            3,
        );
        let desktop = session_fingerprint(
            "testdata/freedoom/FREEDOOM2.WAD",
            &["C:\\games\\Brutal.pk3".into()],
            "MAP01",
            3,
        );
        assert_eq!(phone, desktop);
        assert_ne!(phone, session_fingerprint("freedoom2.wad", &[], "MAP01", 3));
        assert_ne!(
            phone,
            session_fingerprint("freedoom2.wad", &["brutal.pk3".into()], "MAP01", 4)
        );
        // Pinned: builds with other Rust versions must agree.
        assert_eq!(
            session_fingerprint("doom2.wad", &[], "MAP01", 3),
            0xc579_c4de_0bc1_f609
        );
    }
}
