//! Real UDP 2-peer lockstep on Doom E1M1 (in-process host + join threads).
//!
//! ```text
//! cargo run -p room-game --example udp_coop_doom1
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_decorate::parse_decorate;
use room_game::{GameOptions, GameState, Headless, Skill};
use room_map::load_map;
use room_net::{
    ABI_MAJOR, ENGINE_VERSION, Hello, LobbyHost, LobbyMsg, LockstepSession, NetTicCmd, PeerId,
    TimedCmd, UdpTransport,
};
use room_nodebuild::ensure_nodes;
use room_sim::{Buttons, ClassRegistry, StateTable, TicCmd};
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

fn defs() -> (Arc<ClassRegistry>, Arc<StateTable>) {
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

fn start_game(
    wad: &PathBuf,
    classes: Arc<ClassRegistry>,
    states: Arc<StateTable>,
    seed: u32,
) -> Headless {
    let vfs = VfsBuilder::new().mount_path(wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
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

fn advance(
    session: &mut LockstepSession,
    transport: &mut UdpTransport,
    local: NetTicCmd,
) -> Vec<TicCmd> {
    let msg = session.submit_local(local);
    session.broadcast(transport, &msg);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        session.recv(transport);
        if let Some(frame) = session.try_advance() {
            let mut cmds = vec![TicCmd::default(); 2];
            for (peer, net) in frame {
                cmds[peer.0 as usize] = net_to_tic(net);
            }
            return cmds;
        }
        if Instant::now() > deadline {
            session.broadcast(transport, &msg);
        }
        if Instant::now() > deadline + Duration::from_secs(2) {
            panic!("lockstep stall at tic {}", session.next_tic());
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn main() {
    let Some(wad) = doom1() else {
        eprintln!("skip: testdata/doom/doom1.wad missing");
        std::process::exit(0);
    };
    let (classes, states) = defs();
    let fingerprint = 0xD001_E11u64;
    let rng_seed = 0xDEAD_1993u32;

    let mut host_tr =
        UdpTransport::bind(PeerId(0), SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    let host_addr = host_tr.local_addr().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel::<()>();

    let wad_j = wad.clone();
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
            if Instant::now() > deadline {
                panic!("join timeout");
            }
            thread::sleep(Duration::from_millis(5));
        }
        let w = welcome.unwrap();
        tr.set_self_id(w.peer_id);
        tr.add_peer(PeerId(0), host_addr);
        let peers = [PeerId(0), PeerId(1)];
        let mut session = LockstepSession::new(w.peer_id, peers, 1);
        for i in 0..2u8 {
            session.push_remote(TimedCmd {
                tic: 0,
                peer: PeerId(i),
                cmd: NetTicCmd::default(),
            });
        }
        let mut game = start_game(&wad_j, classes_j, states_j, rng_seed);
        assert!(
            game.world().unwrap().players[1].is_some(),
            "coop p2 missing"
        );
        ready_tx.send(()).unwrap();

        for tic in 0..70u32 {
            let local = if tic < 40 {
                NetTicCmd {
                    forward_move: 20 * 256,
                    ..Default::default()
                }
            } else {
                NetTicCmd::default()
            };
            let cmds = advance(&mut session, &mut tr, local);
            game.tick(&cmds);
        }
        let mo = game.world().unwrap().players[1]
            .as_ref()
            .unwrap()
            .mo
            .unwrap();
        let pos = game.world().unwrap().actors.get(mo).unwrap().pos;
        (game.checksum().unwrap(), pos.x(), pos.y())
    });

    // Host lobby.
    let mut lobby = LobbyHost::new(fingerprint, 2, 1, rng_seed, "E1M1", 3);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut joiner = None;
    while lobby.seats().len() < 2 {
        let mut raw = Vec::new();
        host_tr.poll_raw(&mut raw);
        for (addr, bytes) in raw {
            if let Ok(LobbyMsg::Hello(h)) = LobbyMsg::decode(&bytes) {
                if joiner.is_some() {
                    continue;
                }
                let reply = lobby.on_hello(h);
                let _ = host_tr.send_raw(addr, &reply.encode());
                if let LobbyMsg::Welcome(w) = &reply {
                    host_tr.add_peer(w.peer_id, addr);
                    joiner = Some(addr);
                }
            }
        }
        if Instant::now() > deadline {
            panic!("host lobby timeout");
        }
        thread::sleep(Duration::from_millis(5));
    }
    let start = lobby.start_msg().encode();
    let _ = host_tr.send_raw(joiner.unwrap(), &start);

    let mut session = LockstepSession::new(PeerId(0), [PeerId(0), PeerId(1)], 1);
    for i in 0..2u8 {
        session.push_remote(TimedCmd {
            tic: 0,
            peer: PeerId(i),
            cmd: NetTicCmd::default(),
        });
    }
    let mut game = start_game(&wad, classes, states, rng_seed);
    assert!(game.world().unwrap().players[0].is_some());
    assert!(game.world().unwrap().players[1].is_some());
    ready_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("joiner ready");

    println!("room: UDP coop Doom E1M1 — 70 lockstep tics");
    for tic in 0..70u32 {
        let local = if tic < 40 {
            NetTicCmd {
                forward_move: 25 * 256,
                buttons: Buttons::ATTACK,
                ..Default::default()
            }
        } else {
            NetTicCmd::default()
        };
        let cmds = advance(&mut session, &mut host_tr, local);
        game.tick(&cmds);
        if tic % 10 == 0 {
            println!("  tic {tic}: checksum={:?}", game.checksum());
        }
    }

    let (join_sum, jx, jy) = join_thread.join().unwrap();
    let host_sum = game.checksum().unwrap();
    assert_eq!(
        host_sum, join_sum,
        "desync host={host_sum:x} join={join_sum:x}"
    );
    assert_eq!(game.state, GameState::Level);
    let mo0 = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    let p0 = game.world().unwrap().actors.get(mo0).unwrap().pos;
    println!(
        "ok: UDP coop synced on E1M1 checksum={host_sum:x} p0=({:.0},{:.0}) p1=({jx:.0},{jy:.0})",
        p0.x(),
        p0.y()
    );
}
