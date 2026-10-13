//! Two lockstep clients on the same Freedoom MAP01 (runnable M12 demo).
//!
//! ```text
//! cargo run -p room-game --example two_clients_same_map
//! ```
//!
//! Both peers share an in-memory signal mesh, exchange ticcmds, and tick identical
//! Headless worlds. Exits non-zero on desync.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_decorate::parse_decorate;
use room_game::{GameOptions, GameState, Headless, Skill};
use room_map::load_map;
use room_net::{
    LockstepSession, NetTicCmd, PeerId, SharedSignalBus, SignalMeshTransport, TimedCmd,
};
use room_nodebuild::ensure_nodes;
use room_sim::{Buttons, ClassRegistry, StateTable, TicCmd};
use room_vfs::VfsBuilder;
use std::path::PathBuf;
use std::sync::Arc;

fn freedoom2() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    p.is_file().then_some(p)
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

fn main() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing (run scripts/fetch-freedoom.sh)");
        std::process::exit(0);
    };

    let (classes, states) = doom_basic_defs();
    let start = |wad: &PathBuf, classes: Arc<ClassRegistry>, states: Arc<StateTable>| {
        let vfs = VfsBuilder::new().mount_path(wad).unwrap().build().unwrap();
        let mut map = load_map(&vfs, "MAP01").unwrap();
        ensure_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            skill: Skill::new(3),
            rng_seed: 2026,
            record_demo: false,
            ..Default::default()
        });
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game
    };

    let mut g0 = start(&wad, Arc::clone(&classes), Arc::clone(&states));
    let mut g1 = start(&wad, classes, states);
    assert_eq!(g0.checksum(), g1.checksum(), "initial checksum mismatch");

    let bus = SharedSignalBus::new();
    let p0 = PeerId(0);
    let p1 = PeerId(1);
    let mut t0 = SignalMeshTransport::join(bus.clone(), "demo-map01", p0).unwrap();
    let mut t1 = SignalMeshTransport::join(bus, "demo-map01", p1).unwrap();
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

    println!("room: two clients on Freedoom MAP01 (35 tics lockstep)");
    let actors0 = g0.world().map(|w| w.thinkers.default.len()).unwrap_or(0);
    println!("  peer0 actors={actors0} checksum={:?}", g0.checksum());

    for tic in 0..35u32 {
        let local0 = if tic < 20 {
            NetTicCmd {
                forward_move: 25 * 256,
                buttons: Buttons::ATTACK,
                ..Default::default()
            }
        } else {
            NetTicCmd::default()
        };
        let msg0 = s0.submit_local(local0);
        s0.broadcast(&mut t0, &msg0);
        s1.recv(&mut t1);

        let msg1 = s1.submit_local(NetTicCmd::default());
        s1.broadcast(&mut t1, &msg1);
        s0.recv(&mut t0);

        let f0 = s0.try_advance().expect("peer0 frame");
        let f1 = s1.try_advance().expect("peer1 frame");
        assert_eq!(f0, f1, "cmd frame mismatch at tic {tic}");

        let cmd = net_to_tic(f0[&p0]);
        g0.tick(&[cmd]);
        g1.tick(&[cmd]);
        assert_eq!(
            g0.checksum(),
            g1.checksum(),
            "desync after lockstep tic {tic}"
        );
        s0.report_checksum(tic, p0, g0.checksum().unwrap());
        s1.report_checksum(tic, p1, g1.checksum().unwrap());
        s0.report_checksum(tic, p1, g1.checksum().unwrap());
        s1.report_checksum(tic, p0, g0.checksum().unwrap());
        s0.check_desync(tic).unwrap();
        s1.check_desync(tic).unwrap();

        if tic % 7 == 0 {
            let a0 = g0.world().map(|w| w.thinkers.default.len()).unwrap_or(0);
            let a1 = g1.world().map(|w| w.thinkers.default.len()).unwrap_or(0);
            println!(
                "  tic {tic:02}: actors peer0={a0} peer1={a1} checksum={:?}",
                g0.checksum()
            );
        }
    }

    assert_eq!(g0.state, GameState::Level);
    assert_eq!(g1.state, GameState::Level);
    println!(
        "ok: both clients stayed synced on MAP01 (final checksum={:?})",
        g0.checksum()
    );
}
