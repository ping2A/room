//! Strong multiplayer / coop lockstep tests.
//!
//! Covers: dual-seat spawn, independent movement, UDP lobby+lockstep, checksum
//! parity, friendly-fire block, and intentional desync detection.
//!
//! Skips when neither `testdata/doom/doom1.wad` nor Freedoom is present.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_decorate::parse_decorate;
use room_game::{GameOptions, GameState, Headless, Skill};
use room_map::load_map;
use room_net::{
    ABI_MAJOR, ENGINE_VERSION, Hello, LobbyHost, LobbyMsg, LockstepSession, NetError, NetTicCmd,
    PeerId, SharedSignalBus, SignalMeshTransport, TimedCmd, UdpTransport,
};
use room_nodebuild::ensure_nodes;
use room_sim::{Buttons, ClassRegistry, StateTable, TicCmd, damage_mobj};
use room_vfs::VfsBuilder;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn doom1() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    p.is_file().then_some(p)
}

fn freedoom2() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    p.is_file().then_some(p)
}

/// Prefer Doom E1M1; fall back to Freedoom MAP01.
fn coop_wad_and_map() -> Option<(PathBuf, &'static str)> {
    if let Some(p) = doom1() {
        return Some((p, "E1M1"));
    }
    freedoom2().map(|p| (p, "MAP01"))
}

fn doom_basic_defs() -> (Arc<ClassRegistry>, Arc<StateTable>) {
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).expect("decorate");
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).expect("classes"));
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    (classes, states)
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

fn start_coop(
    wad: &PathBuf,
    map_name: &str,
    classes: Arc<ClassRegistry>,
    states: Arc<StateTable>,
    seed: u32,
) -> Headless {
    let vfs = VfsBuilder::new().mount_path(wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, map_name).unwrap();
    ensure_nodes(&mut map);
    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: seed,
        record_demo: false,
        coop_players: 2,
        ..Default::default()
    });
    game.set_defs(classes, states);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    game
}

fn player_xy(game: &Headless, seat: usize) -> (f64, f64) {
    let w = game.world().expect("world");
    let mo = w.players[seat].as_ref().expect("seat").mo.expect("mo");
    let a = w.actors.get(mo).expect("actor");
    (a.pos.x(), a.pos.y())
}

fn player_hp(game: &Headless, seat: usize) -> i32 {
    let w = game.world().expect("world");
    let mo = w.players[seat].as_ref().expect("seat").mo.expect("mo");
    w.actors.get(mo).expect("actor").health
}

fn seed_tic0(session: &mut LockstepSession) {
    for i in 0..2u8 {
        session.push_remote(TimedCmd {
            tic: 0,
            peer: PeerId(i),
            cmd: NetTicCmd::default(),
        });
    }
}

fn lockstep_frame(
    s0: &mut LockstepSession,
    s1: &mut LockstepSession,
    t0: &mut impl room_net::Transport,
    t1: &mut impl room_net::Transport,
    c0: NetTicCmd,
    c1: NetTicCmd,
) -> std::collections::BTreeMap<PeerId, NetTicCmd> {
    let m0 = s0.submit_local(c0);
    s0.broadcast(t0, &m0);
    s1.recv(t1);
    let m1 = s1.submit_local(c1);
    s1.broadcast(t1, &m1);
    s0.recv(t0);
    let f0 = s0.try_advance().expect("peer0 frame");
    let f1 = s1.try_advance().expect("peer1 frame");
    assert_eq!(f0, f1, "cmd frame mismatch");
    f0
}

fn apply_dual(game: &mut Headless, frame: &std::collections::BTreeMap<PeerId, NetTicCmd>) {
    let cmds = [
        net_to_tic(*frame.get(&PeerId(0)).unwrap_or(&NetTicCmd::default())),
        net_to_tic(*frame.get(&PeerId(1)).unwrap_or(&NetTicCmd::default())),
    ];
    game.tick(&cmds);
}

#[test]
fn coop_spawns_two_players_at_distinct_starts() {
    let Some((wad, map_name)) = coop_wad_and_map() else {
        eprintln!("skip: no doom1.wad / freedoom2.wad");
        return;
    };
    let (classes, states) = doom_basic_defs();
    let game = start_coop(&wad, map_name, classes, states, 42);
    let w = game.world().unwrap();
    assert!(w.players[0].is_some());
    assert!(w.players[1].is_some());
    let (x0, y0) = player_xy(&game, 0);
    let (x1, y1) = player_xy(&game, 1);
    let dist = ((x0 - x1).hypot(y0 - y1)).abs();
    assert!(
        dist > 8.0,
        "coop seats should spawn apart (p0=({x0},{y0}) p1=({x1},{y1}) dist={dist})"
    );
}

#[test]
fn coop_friendly_fire_blocked_on_real_map() {
    let Some((wad, map_name)) = coop_wad_and_map() else {
        eprintln!("skip: no doom1.wad / freedoom2.wad");
        return;
    };
    let (classes, states) = doom_basic_defs();
    let mut game = start_coop(&wad, map_name, classes, states, 7);
    let (mo0, mo1, hp1) = {
        let w = game.world_mut().unwrap();
        let mo0 = w.players[0].as_ref().unwrap().mo.unwrap();
        let mo1 = w.players[1].as_ref().unwrap().mo.unwrap();
        let hp1 = w.actors.get(mo1).unwrap().health;
        (mo0, mo1, hp1)
    };
    let dealt = {
        let w = game.world_mut().unwrap();
        damage_mobj(w, mo1, Some(mo0), Some(mo0), 100)
    };
    assert_eq!(dealt, 0, "coop must not allow player-vs-player damage");
    assert_eq!(player_hp(&game, 1), hp1);
}

/// Two seats, independent cmds over in-memory signal mesh; both worlds stay bit-identical.
#[test]
fn coop_two_seat_signal_mesh_independent_movement_stays_synced() {
    let Some((wad, map_name)) = coop_wad_and_map() else {
        eprintln!("skip: no doom1.wad / freedoom2.wad");
        return;
    };
    let (classes, states) = doom_basic_defs();
    let seed = 2026u32;
    let mut g0 = start_coop(
        &wad,
        map_name,
        Arc::clone(&classes),
        Arc::clone(&states),
        seed,
    );
    let mut g1 = start_coop(&wad, map_name, classes, states, seed);
    assert_eq!(g0.checksum(), g1.checksum(), "initial checksum");

    let p0 = PeerId(0);
    let p1 = PeerId(1);
    let bus = SharedSignalBus::new();
    let mut t0 = SignalMeshTransport::join(bus.clone(), "coop-mesh", p0).unwrap();
    let mut t1 = SignalMeshTransport::join(bus, "coop-mesh", p1).unwrap();
    let n0 = t0.signal_name().to_owned();
    let n1 = t1.signal_name().to_owned();
    t0.add_remote(p1, n1);
    t1.add_remote(p0, n0);

    let mut s0 = LockstepSession::new(p0, [p0, p1], 1);
    let mut s1 = LockstepSession::new(p1, [p0, p1], 1);
    seed_tic0(&mut s0);
    seed_tic0(&mut s1);

    let start0 = player_xy(&g0, 0);
    let start1 = player_xy(&g0, 1);

    for tic in 0..48u32 {
        let c0 = NetTicCmd {
            forward_move: 25 * 256,
            ..Default::default()
        };
        let c1 = NetTicCmd {
            side_move: 20 * 256,
            ..Default::default()
        };
        let frame = lockstep_frame(&mut s0, &mut s1, &mut t0, &mut t1, c0, c1);
        apply_dual(&mut g0, &frame);
        apply_dual(&mut g1, &frame);
        assert_eq!(
            g0.checksum(),
            g1.checksum(),
            "desync after mesh lockstep tic {tic}"
        );
        let sum = g0.checksum().unwrap();
        s0.report_checksum(tic, p0, sum);
        s0.report_checksum(tic, p1, sum);
        s1.report_checksum(tic, p0, sum);
        s1.report_checksum(tic, p1, sum);
        s0.check_desync(tic).unwrap();
        s1.check_desync(tic).unwrap();
    }

    let end0 = player_xy(&g0, 0);
    let end1 = player_xy(&g0, 1);
    assert!(
        (end0.0 - start0.0).hypot(end0.1 - start0.1) > 16.0,
        "player 0 should have moved forward"
    );
    assert!(
        (end1.0 - start1.0).hypot(end1.1 - start1.1) > 16.0,
        "player 1 should have moved sideways"
    );
    // Different input → seats must not stay glued together.
    assert!(
        (end0.0 - end1.0).hypot(end0.1 - end1.1) > 8.0,
        "independent cmds must separate seats"
    );
    assert_eq!(g0.state, GameState::Level);
    assert_eq!(g1.state, GameState::Level);
}

/// Real UDP sockets: lobby handshake + dual-seat lockstep on Doom/Freedoom map.
#[test]
fn coop_udp_lobby_lockstep_dual_seat_stays_synced() {
    let Some((wad, map_name)) = coop_wad_and_map() else {
        eprintln!("skip: no doom1.wad / freedoom2.wad");
        return;
    };
    let (classes, states) = doom_basic_defs();
    let fingerprint = 0xC00B_0001u64;
    let rng_seed = 0xDEAD_1993u32;

    let mut host_tr =
        UdpTransport::bind(PeerId(0), SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    let host_addr = host_tr.local_addr().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel::<()>();

    let wad_j = wad.clone();
    let map_j = map_name.to_owned();
    let classes_j = Arc::clone(&classes);
    let states_j = Arc::clone(&states);

    let join_thread = thread::spawn(move || {
        let mut tr =
            UdpTransport::bind(PeerId(254), SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let hello = LobbyMsg::Hello(Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR,
            fingerprint,
            name: "joiner".into(),
        });
        let mut welcome = None;
        let mut started = None;
        let deadline = Instant::now() + Duration::from_secs(10);
        while welcome.is_none() || started.is_none() {
            let _ = tr.send_raw(host_addr, &hello.encode());
            let mut raw = Vec::new();
            tr.poll_raw(&mut raw);
            for (_, bytes) in raw {
                match LobbyMsg::decode(&bytes) {
                    Ok(LobbyMsg::Welcome(w)) => welcome = Some(w),
                    Ok(LobbyMsg::Start(s)) => started = Some(s.rng_seed),
                    Ok(LobbyMsg::Reject(r)) => panic!("rejected: {}", r.reason),
                    _ => {}
                }
            }
            assert!(Instant::now() <= deadline, "join lobby timeout");
            thread::sleep(Duration::from_millis(2));
        }
        let w = welcome.unwrap();
        assert_eq!(w.peer_id, PeerId(1));
        assert_eq!(w.peer_count, 2);
        assert_eq!(w.map, map_j);
        assert_eq!(started.unwrap_or(w.rng_seed), rng_seed);
        tr.set_self_id(w.peer_id);
        tr.add_peer(PeerId(0), host_addr);

        let mut session = LockstepSession::new(PeerId(1), [PeerId(0), PeerId(1)], 1);
        seed_tic0(&mut session);
        let mut game = start_coop(&wad_j, &map_j, classes_j, states_j, rng_seed);
        assert!(game.world().unwrap().players[1].is_some());
        ready_tx.send(()).unwrap();

        let mut last_xy1 = player_xy(&game, 1);
        for tic in 0..60u32 {
            let local = NetTicCmd {
                forward_move: 22 * 256,
                ..Default::default()
            };
            let msg = session.submit_local(local);
            session.broadcast(&mut tr, &msg);
            let deadline = Instant::now() + Duration::from_secs(2);
            let frame = loop {
                session.recv(&mut tr);
                if let Some(f) = session.try_advance() {
                    break f;
                }
                if Instant::now() > deadline {
                    session.broadcast(&mut tr, &msg);
                }
                assert!(
                    Instant::now() <= deadline + Duration::from_secs(2),
                    "stall {tic}"
                );
                thread::sleep(Duration::from_millis(1));
            };
            apply_dual(&mut game, &frame);
            last_xy1 = player_xy(&game, 1);
        }
        (
            game.checksum().unwrap(),
            last_xy1,
            player_hp(&game, 0),
            player_hp(&game, 1),
        )
    });

    let mut lobby = LobbyHost::new(fingerprint, 2, 1, rng_seed, map_name, 3);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut joiner_addr = None;
    while lobby.seats().len() < 2 {
        let mut raw = Vec::new();
        host_tr.poll_raw(&mut raw);
        for (addr, bytes) in raw {
            if let Ok(LobbyMsg::Hello(h)) = LobbyMsg::decode(&bytes) {
                if joiner_addr.is_some() {
                    continue;
                }
                let reply = lobby.on_hello(h);
                let _ = host_tr.send_raw(addr, &reply.encode());
                if let LobbyMsg::Welcome(w) = &reply {
                    host_tr.add_peer(w.peer_id, addr);
                    joiner_addr = Some(addr);
                }
            }
        }
        assert!(Instant::now() <= deadline, "host lobby timeout");
        thread::sleep(Duration::from_millis(2));
    }
    let _ = host_tr.send_raw(joiner_addr.unwrap(), &lobby.start_msg().encode());

    let mut session = LockstepSession::new(PeerId(0), [PeerId(0), PeerId(1)], 1);
    seed_tic0(&mut session);
    let mut game = start_coop(&wad, map_name, classes, states, rng_seed);
    ready_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("joiner ready");

    let start0 = player_xy(&game, 0);
    for tic in 0..60u32 {
        let local = NetTicCmd {
            side_move: 18 * 256,
            buttons: Buttons::ATTACK,
            ..Default::default()
        };
        let msg = session.submit_local(local);
        session.broadcast(&mut host_tr, &msg);
        let deadline = Instant::now() + Duration::from_secs(2);
        let frame = loop {
            session.recv(&mut host_tr);
            if let Some(f) = session.try_advance() {
                break f;
            }
            if Instant::now() > deadline {
                session.broadcast(&mut host_tr, &msg);
            }
            assert!(
                Instant::now() <= deadline + Duration::from_secs(2),
                "host stall {tic}"
            );
            thread::sleep(Duration::from_millis(1));
        };
        apply_dual(&mut game, &frame);
        let sum = game.checksum().unwrap();
        session.report_checksum(tic, PeerId(0), sum);
        session.report_checksum(tic, PeerId(1), sum);
        session.check_desync(tic).unwrap();
    }

    let (join_sum, join_xy1, join_hp0, join_hp1) = join_thread.join().unwrap();
    let host_sum = game.checksum().unwrap();
    assert_eq!(
        host_sum, join_sum,
        "UDP desync host={host_sum:x} join={join_sum:x}"
    );
    let end0 = player_xy(&game, 0);
    let end1 = player_xy(&game, 1);
    assert!(
        (end0.0 - start0.0).hypot(end0.1 - start0.1) > 8.0,
        "host seat should move"
    );
    assert!(
        (end1.0 - join_xy1.0).abs() < 1.0 && (end1.1 - join_xy1.1).abs() < 1.0,
        "p1 position must match across peers: host={end1:?} join={join_xy1:?}"
    );
    // Attack held but FF blocked — both still at full-ish health (monsters may chip).
    assert_eq!(player_hp(&game, 0), join_hp0);
    assert_eq!(player_hp(&game, 1), join_hp1);
    assert_eq!(game.state, GameState::Level);
}

/// Checksums reported differently must trip desync detection.
#[test]
fn lockstep_desync_detected_on_checksum_mismatch() {
    let p0 = PeerId(0);
    let p1 = PeerId(1);
    let mut s = LockstepSession::new(p0, [p0, p1], 1);
    s.report_checksum(3, p0, 0xAAAA);
    s.report_checksum(3, p1, 0xBBBB);
    match s.check_desync(3) {
        Err(NetError::Desync { .. }) => {}
        other => panic!("expected Desync, got {other:?}"),
    }
}

#[test]
fn lockstep_matching_checksums_ok() {
    let p0 = PeerId(0);
    let p1 = PeerId(1);
    let mut s = LockstepSession::new(p0, [p0, p1], 1);
    s.report_checksum(1, p0, 0x1111);
    s.report_checksum(1, p1, 0x1111);
    s.check_desync(1).unwrap();
}
