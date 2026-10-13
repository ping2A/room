//! Headless smoke: load Freedoom Phase 2 MAP01 with doom_basic actors and tick.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_core::{Angle, DVec2, DVec3, LineId};
use room_decorate::parse_decorate;
use room_defs::Complevel;
use room_game::{
    DemoBundle, DemoPlayer, GameOptions, GameState, Headless, Skill, VanillaLmp,
    load_embedded_engine_data,
};
use room_map::{MapGeometry, load_map};
use room_net::{
    LockstepSession, NetTicCmd, PeerId, SharedSignalBus, SignalMeshTransport, TimedCmd,
};
use room_nodebuild::apply_nodes;

/// Keep IWAD/`ZNODES` trees when present — rebuilt nodes still mis-classify some
/// sectors (DEMO1 MAP03 ledge west of x=160 → floor 8 instead of 16).
fn ensure_nodes(map: &mut MapGeometry) {
    if map.game_nodes.is_none() {
        apply_nodes(map);
    }
}
use room_sim::{
    ActivationType, Buttons, ClassRegistry, SimMathMode, StateTable, TicCmd, activate_line,
    cross_special_lines, use_lines,
};
use room_vfs::{Namespace, VfsBuilder};
use std::path::PathBuf;
use std::sync::Arc;

fn freedoom2() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    p.is_file().then_some(p)
}

fn freedoom1() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom1.wad");
    p.is_file().then_some(p)
}

fn doom_basic_defs() -> (Arc<ClassRegistry>, Arc<StateTable>) {
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    (classes, states)
}

/// Exit_Normal (Hexen 243) after XLAT of Doom 11 / 52.
const EXIT_NORMAL: i16 = 243;

fn activate_exit_to_intermission(game: &mut Headless) {
    let line_idx = game
        .world()
        .unwrap()
        .map
        .lines
        .iter()
        .position(|l| l.special == EXIT_NORMAL);
    if line_idx.is_none() {
        // Boss / scripted maps (e.g. MAP30) have no linedef exit — request level end.
        game.world_mut().unwrap().level.exiting = true;
        game.tick(&[]);
        assert_eq!(game.state, GameState::Intermission);
        return;
    }
    let line_idx = line_idx.unwrap();
    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    assert!(
        activate_line(
            game.world_mut().unwrap(),
            LineId::from_raw(line_idx as u32),
            Some(mo),
            ActivationType::Use,
        ),
        "Exit_Normal should activate"
    );
    game.tick(&[]);
    assert_eq!(game.state, GameState::Intermission);
}

#[test]
fn freedoom2_map01_headless_ticks_and_demo_verifies() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing (run scripts/fetch-freedoom.sh)");
        return;
    };

    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));

    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map);

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        record_demo: true,
        ..Default::default()
    });
    game.set_defs(Arc::clone(&classes), Arc::clone(&states));
    game.start_custom_map(map, Skill::new(3)).unwrap();
    assert_eq!(game.state, GameState::Level);

    let world = game.world().unwrap();
    assert!(world.players[0].is_some(), "player start possessed");
    assert!(
        world.thinkers.default.len() > 1,
        "expected map things spawned"
    );
    let c0 = game.checksum().unwrap();

    let cmd = TicCmd {
        forward_move: 25 * 256,
        buttons: Buttons(Buttons::ATTACK),
        ..TicCmd::default()
    };
    for _ in 0..70 {
        game.tick(&[cmd]);
    }
    assert_eq!(game.state, GameState::Level);
    assert_ne!(c0, game.checksum().unwrap());

    let demo = game.take_demo().unwrap();
    assert_eq!(demo.tics.len(), 70);
    assert_eq!(demo.map, "MAP01");

    if std::env::var_os("ROOM_WRITE_DEMO").is_some() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/demos/freedoom2_map01_smoke.fdem");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, demo.to_fdem()).unwrap();
        eprintln!("wrote {}", path.display());
    }

    // Fresh world, same defs/map → checksum samples must match.
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map2 = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map2);
    let mut world2 = room_sim::World::from_map_with_skill(map2, classes, states, 3).unwrap();
    world2.rng.set_seed(1993);
    let result = DemoPlayer { demo: &demo }.verify(&mut world2).unwrap();
    assert_eq!(result.first_mismatch, None, "demo verify desync");
    assert_eq!(result.tics_ok, 70);
}

/// Longer MAP01 session: walk + fire (recorded/verified), then use a translated door.
#[test]
fn freedoom2_map01_real_play_records_and_verifies() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing (run scripts/fetch-freedoom.sh)");
        return;
    };

    let (classes, states) = doom_basic_defs();
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map);

    const DOOR_RAISE: i16 = 12;
    let line = map
        .lines
        .iter()
        .find(|l| l.special == DOOR_RAISE)
        .expect("MAP01 should have a Door_Raise after XLAT");
    let v1 = map.vertices[line.v1.raw() as usize].pos;
    let v2 = map.vertices[line.v2.raw() as usize].pos;
    let mid_x = (v1.x() + v2.x()) * 0.5;
    let mid_y = (v1.y() + v2.y()) * 0.5;
    let dx = v2.x() - v1.x();
    let dy = v2.y() - v1.y();
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let nx = -dy / len;
    let ny = dx / len;

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        record_demo: true,
        ..Default::default()
    });
    game.set_defs(Arc::clone(&classes), Arc::clone(&states));
    game.start_custom_map(map, Skill::new(3)).unwrap();

    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    let start = {
        let a = game.world().unwrap().actors.get(mo).unwrap();
        (a.pos.x(), a.pos.y())
    };

    let walk_fire = TicCmd {
        forward_move: 25 * 256,
        buttons: Buttons(Buttons::ATTACK),
        ..TicCmd::default()
    };
    for _ in 0..175 {
        game.tick(&[walk_fire]);
    }

    let end = {
        let a = game.world().unwrap().actors.get(mo).unwrap();
        (a.pos.x(), a.pos.y())
    };
    let moved = (end.0 - start.0).abs() + (end.1 - start.1).abs();
    assert!(moved > 8.0, "player should have moved on MAP01 (Δ={moved})");

    let demo = game.take_demo().unwrap();
    assert_eq!(demo.map, "MAP01");
    assert_eq!(demo.tics.len(), 175);

    let mut map2 = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map2);
    let mut world2 =
        room_sim::World::from_map_with_skill(map2, Arc::clone(&classes), Arc::clone(&states), 3)
            .unwrap();
    world2.rng.set_seed(demo.rng_seed);
    let result = DemoPlayer { demo: &demo }.verify(&mut world2).unwrap();
    assert_eq!(result.first_mismatch, None, "real-play demo desync");

    // Door use is a separate playsim check (position warp is not in TicCmds).
    {
        let a = game.world_mut().unwrap().actors.get_mut(mo).unwrap();
        a.pos = DVec3::new(mid_x + nx * 32.0, mid_y + ny * 32.0, a.pos.z());
        a.angle = Angle::from_radians((-ny).atan2(-nx));
    }
    assert!(
        use_lines(game.world_mut().unwrap(), mo),
        "use should open a MAP01 door"
    );
    for _ in 0..40 {
        game.tick(&[]);
    }
    assert!(
        !game.world().unwrap().doors.is_empty(),
        "expected door thinker after use"
    );
}

/// Parse IWAD `DEMO1` lump (Doom 1.9 .lmp) — sync playback lands with vanilla math.
#[test]
fn freedoom2_demo1_lmp_parses() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let id = vfs.find("DEMO1", Namespace::Global).expect("DEMO1 lump");
    let bytes = vfs.read(id).unwrap();
    let lmp = VanillaLmp::from_lmp(&bytes).unwrap();
    assert_eq!(lmp.version, 109);
    assert_eq!(lmp.map_name(true), "MAP03");
    assert!(lmp.tics.len() > 100, "tics={}", lmp.tics.len());
    let rec = lmp.to_recorder(true);
    assert_eq!(rec.complevel, Complevel::Vanilla);
    assert_eq!(rec.map, "MAP03");
    assert_eq!(rec.tics.len(), lmp.tics.len());
}

/// Smoke-play DEMO1 cmds on MAP03 under Complevel::Vanilla with full room actors.
#[test]
fn freedoom2_demo1_vanilla_smoke_ticks() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let id = vfs.find("DEMO1", Namespace::Global).expect("DEMO1 lump");
    let lmp = VanillaLmp::from_lmp(&vfs.read(id).unwrap()).unwrap();
    let eng = match load_embedded_engine_data() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("skip: embedded engine data unavailable ({e})");
            return;
        }
    };
    let mut map = load_map(&vfs, "MAP03").unwrap();
    ensure_nodes(&mut map);
    let mut game = Headless::new(GameOptions {
        skill: Skill::new(lmp.skill),
        rng_seed: 0,
        record_demo: false,
        complevel: Complevel::Vanilla,
        ..Default::default()
    });
    game.set_defs(eng.classes, eng.states);
    game.start_custom_map(map, Skill::new(lmp.skill)).unwrap();
    assert_eq!(game.world().unwrap().math_mode, SimMathMode::VanillaFixed);
    let mo0 = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    let start = game.world().unwrap().actors.get(mo0).unwrap().pos;
    let n = lmp.tics.len().min(70);
    for cmds in lmp.tics.iter().take(n) {
        game.tick(cmds);
        assert_eq!(game.state, GameState::Level);
    }
    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    let pos = game.world().unwrap().actors.get(mo).unwrap().pos;
    let ang = game
        .world()
        .unwrap()
        .actors
        .get(mo)
        .unwrap()
        .angle
        .degrees();
    assert!(
        (pos.x() - start.x()).abs() > 1.0 || (pos.y() - start.y()).abs() > 1.0,
        "expected motion under DEMO1 cmds, start={start:?} pos={pos:?}"
    );
    // Chocolate Doom 3.1.1 (patched P_Ticker dump) on Freedoom2 DEMO1 @ leveltime 70.
    // External: `scripts/demo1-chocolate-timedemo.sh`. Re-print: ROOM_WRITE_DEMO1_POS=1.
    const CHOCO_X: f64 = 527.958;
    const CHOCO_Y: f64 = 989.147;
    const TOL: f64 = 0.5;
    if std::env::var_os("ROOM_WRITE_DEMO1_POS").is_some() {
        eprintln!(
            "DEMO1@70 pos=({:.6},{:.6}) ang={:.6}",
            pos.x(),
            pos.y(),
            ang
        );
    } else {
        assert!(
            (pos.x() - CHOCO_X).abs() < TOL && (pos.y() - CHOCO_Y).abs() < TOL,
            "DEMO1@70 pos=({:.3},{:.3}) expected Chocolate ~({CHOCO_X},{CHOCO_Y}) ±{TOL} — set ROOM_WRITE_DEMO1_POS=1 to print",
            pos.x(),
            pos.y()
        );
    }
}

/// Play the full Freedoom2 DEMO1 under VanillaFixed (tic count matches Chocolate Doom).
#[test]
fn freedoom2_demo1_vanilla_full_playback() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let id = vfs.find("DEMO1", Namespace::Global).expect("DEMO1 lump");
    let lmp = VanillaLmp::from_lmp(&vfs.read(id).unwrap()).unwrap();
    assert_eq!(lmp.tics.len(), 2664, "Freedoom2 DEMO1 tic count");
    let eng = match load_embedded_engine_data() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("skip: embedded engine data unavailable ({e})");
            return;
        }
    };
    let mut map = load_map(&vfs, "MAP03").unwrap();
    ensure_nodes(&mut map);
    let mut game = Headless::new(GameOptions {
        skill: Skill::new(lmp.skill),
        rng_seed: 0,
        record_demo: false,
        complevel: Complevel::Vanilla,
        ..Default::default()
    });
    game.set_defs(eng.classes, eng.states);
    game.start_custom_map(map, Skill::new(lmp.skill)).unwrap();
    let dump_rng = std::env::var_os("ROOM_DUMP_DEMO1_RNG").is_some();
    let dump_path = std::env::var_os("ROOM_DUMP_DEMO1_PATH").is_some();
    for (i, cmds) in lmp.tics.iter().enumerate() {
        game.tick(cmds);
        let t = i + 1;
        assert_eq!(game.state, GameState::Level, "left Level at tic {}", t);

        if std::env::var_os("ROOM_DUMP_IMP").is_some() && (200..=500).contains(&t) && t % 5 == 0
        {
            let w = game.world().unwrap();
            for &id in &w.thinkers.default {
                let Some(a) = w.actors.get(id) else { continue };
                let Some(c) = w.classes.get(a.class) else {
                    continue;
                };
                if !c.name.eq_ignore_ascii_case("DoomImp") {
                    continue;
                }
                if (a.pos.x() + 810.0).abs() > 80.0 || (a.pos.y() - 420.0).abs() > 80.0 {
                    continue;
                }
                let act = a
                    .state
                    .and_then(|s| w.states.get(s))
                    .and_then(|s| s.action.as_deref())
                    .unwrap_or("-");
                eprintln!(
                    "IMP@{} ({:.0},{:.0}) hp={} tgt={} act={} idx={}",
                    t,
                    a.pos.x(),
                    a.pos.y(),
                    a.health,
                    a.target.is_some(),
                    act,
                    w.vanilla_rng.prndindex
                );
            }
        }
        if std::env::var_os("ROOM_DUMP_CHASE").is_some()
            && (300..=500).contains(&t)
            && (t % 10 == 0 || (494..=500).contains(&t))
        {
            let w = game.world().unwrap();
            for &id in &w.thinkers.default {
                let Some(a) = w.actors.get(id) else { continue };
                if a.target.is_none() || a.health <= 0 {
                    continue;
                }
                let Some(c) = w.classes.get(a.class) else {
                    continue;
                };
                if !(c.name.contains("Zombie")
                    || c.name.contains("Shotgun")
                    || c.name.contains("Imp")
                    || c.name.contains("Demon"))
                {
                    continue;
                }
                let act = a
                    .state
                    .and_then(|s| w.states.get(s))
                    .and_then(|s| s.action.as_deref())
                    .unwrap_or("-");
                eprintln!(
                    "CHASE@{} {} ({:.0},{:.0}) md={} mc={} tgt={} st={} act={}",
                    t,
                    c.name,
                    a.pos.x(),
                    a.pos.y(),
                    a.move_dir,
                    a.move_count,
                    a.target.is_some(),
                    a.state.map(|s| s.raw()).unwrap_or(0),
                    act
                );
            }
        }

        if dump_rng && ((300..=342).contains(&t) || (340..=455).contains(&t)) {
            let w = game.world().unwrap();
            let mo = w.players[0].as_ref().unwrap().mo.unwrap();
            let p = w.actors.get(mo).unwrap();
            let pl = w.players[0].as_ref().unwrap();
            let atk = pl.cmd.buttons.0 & 1; // ATTACK bit
            let pst = pl
                .pspr_state
                .and_then(|s| w.states.get(s))
                .map(|s| {
                    format!(
                        "{}:{}",
                        s.sprite.as_str(),
                        s.action.as_deref().unwrap_or("-")
                    )
                })
                .unwrap_or_else(|| "-".into());
            eprintln!(
                "DETAIL@{} p=({:.1},{:.1}) ang={:.1} idx={} atk={} pspr={} tics={}",
                t,
                p.pos.x(),
                p.pos.y(),
                p.angle.degrees(),
                w.vanilla_rng.prndindex,
                atk,
                pst,
                pl.pspr_tics
            );
            for &id in &w.thinkers.default {
                let Some(a) = w.actors.get(id) else { continue };
                let Some(c) = w.classes.get(a.class) else {
                    continue;
                };
                if c.name.eq_ignore_ascii_case("ShotgunGuy")
                    && (a.spawn_point.x() - 320.0).abs() < 1.0
                    && (a.spawn_point.y() - 640.0).abs() < 1.0
                {
                    eprintln!(
                        "SGDET@{} ({:.1},{:.1}) hp={} mc={} md={} rt={} idx={}",
                        t,
                        a.pos.x(),
                        a.pos.y(),
                        a.health,
                        a.move_count,
                        a.move_dir,
                        a.reaction_time,
                        w.vanilla_rng.prndindex
                    );
                }
            }
            if dump_rng && (338..=345).contains(&t) {
                for (i, fl) in w.flickers.iter().enumerate() {
                    eprintln!(
                        "FLICK@{} i={i} fire={} count={} bright={} light={}",
                        t,
                        fl.fire,
                        fl.count,
                        fl.bright,
                        w.sectors
                            .get(fl.sector.raw() as usize)
                            .map(|s| s.light)
                            .unwrap_or(-1)
                    );
                }
            }
            if (404..=412).contains(&t) {
                for &id in &w.thinkers.default {
                    let Some(a) = w.actors.get(id) else { continue };
                    if a.destroyed || a.target.is_none() || a.health <= 0 {
                        continue;
                    }
                    let Some(c) = w.classes.get(a.class) else {
                        continue;
                    };
                    if (c.flags & room_sim::FLAG_COUNTKILL) == 0 {
                        continue;
                    }
                    let st = a
                        .state
                        .and_then(|s| w.states.get(s))
                        .map(|s| {
                            format!(
                                "{}{}:{}",
                                s.sprite,
                                s.frame,
                                s.action.as_deref().unwrap_or("-")
                            )
                        })
                        .unwrap_or_else(|| "-".into());
                    eprintln!(
                        "MOB@{} {} ({:.0},{:.0}) mc={} md={} hp={} tics={} st={} idx={}",
                        t,
                        c.name,
                        a.pos.x(),
                        a.pos.y(),
                        a.move_count,
                        a.move_dir,
                        a.health,
                        a.tics,
                        st,
                        w.vanilla_rng.prndindex
                    );
                }
            }
        }
        // Chocolate Doom DEMO1 timedemo `prndindex` checkpoints (Freedoom2).
        // Shotgun fire aligns with Chocolate (`SHOTGUN@338` during think → DETAIL@339 idx=171).
        if matches!(t, 300 | 330 | 340 | 400 | 406 | 411) {
            let idx = game.world().unwrap().vanilla_rng.prndindex;
            let expect = match t {
                300 => 84,
                330 => 111,
                340 => 171,
                400 => 3,
                406 => 12,
                411 => 72,
                _ => unreachable!(),
            };
            assert_eq!(idx, expect, "DEMO1 prndindex@{t}");
        }
        if std::env::var_os("ROOM_DUMP_NEAR_MOB").is_some()
            && ((410..=456).contains(&t)
                || (460..=470).contains(&t)
                || (490..=510).contains(&t)
                || (520..=535).contains(&t)
                || (740..=810).contains(&t)
                || (885..=895).contains(&t)
                || (1000..=1015).contains(&t))
        {
            let w = game.world().unwrap();
            let mo = w.players[0].as_ref().unwrap().mo.unwrap();
            let p = w.actors.get(mo).unwrap();
            for &id in &w.thinkers.default {
                let Some(a) = w.actors.get(id) else { continue };
                if a.destroyed {
                    continue;
                }
                let Some(c) = w.classes.get(a.class) else {
                    continue;
                };
                if !matches!(
                    c.name.as_str(),
                    "ZombieMan" | "ShotgunGuy" | "DoomImp" | "Shotgun" | "Clip" | "Shell"
                ) {
                    continue;
                }
                let dx = a.pos.x() - p.pos.x();
                let dy = a.pos.y() - p.pos.y();
                if dx * dx + dy * dy > 400.0 * 400.0 {
                    continue;
                }
                let st = a
                    .state
                    .and_then(|s| w.states.get(s))
                    .map(|s| s.action.as_deref().unwrap_or("-"))
                    .unwrap_or("-");
                let solid = (a.flags & 1) != 0; // FLAG_SOLID
                eprintln!(
                    "MOB@{} {} ({:.0},{:.0},{:.0}) hp={} solid={} h={:.0} r={:.0} st={} idx={}",
                    t,
                    c.name,
                    a.pos.x(),
                    a.pos.y(),
                    a.pos.z(),
                    a.health,
                    solid,
                    a.height,
                    a.radius,
                    st,
                    w.vanilla_rng.prndindex
                );
            }
        }
        if dump_path
            && (matches!(
                t,
                70 | 100
                    | 200
                    | 280
                    | 290
                    | 299
                    | 300
                    | 320
                    | 340
                    | 360
                    | 380
                    | 400
                    | 441
                    | 450
                    | 455
                    | 456
                    | 457
                    | 458
                    | 459
                    | 460
                    | 470
                    | 480
                    | 490
                    | 500
                    | 520
                    | 530
                    | 540
                    | 600
                    | 800
                    | 1000
                    | 2664
            ) || (400..=2100).contains(&t)
                || (80..=200).contains(&t)
                || ((100..=400).contains(&t) && t % 20 == 0)
                || (1..=80).contains(&t)
                || t == 1500
                || t == 2000
                || t == 2664)
        {
            let w = game.world().unwrap();
            let mo = w.players[0].as_ref().unwrap().mo.unwrap();
            let p = w.actors.get(mo).unwrap();
            let bam = p.angle.to_bam().0;
            let mx = room_core::Fixed::from_f64(p.vel.x()).0;
            let my = room_core::Fixed::from_f64(p.vel.y()).0;
            let px = room_core::Fixed::from_f64(p.pos.x()).0;
            let py = room_core::Fixed::from_f64(p.pos.y()).0;
            eprintln!(
                "FER@{} x={:.6} y={:.6} z={:.3} ang={:.1} mom=({:.3},{:.3}) hp={} idx={} bam={} momi=({},{}) xy=({},{})",
                t,
                p.pos.x(),
                p.pos.y(),
                p.pos.z(),
                p.angle.degrees(),
                p.vel.x(),
                p.vel.y(),
                p.health,
                w.vanilla_rng.prndindex,
                bam,
                mx,
                my,
                px,
                py
            );
        }
        // Player path vs Chocolate (IWAD nodes; CHOCO timedemo).
        if matches!(t, 441 | 450 | 500 | 520 | 550) {
            let w = game.world().unwrap();
            let mo = w.players[0].as_ref().unwrap().mo.unwrap();
            let p = w.actors.get(mo).unwrap();
            let (ex, ey, tol) = match t {
                441 => (222.09, 731.05, 0.5),
                450 => (234.9, 801.4, 1.0),
                500 => (119.80, 766.14, 1.0),
                520 => (-106.28, 643.24, 1.5),
                550 => (-141.05, 549.06, 1.5),
                _ => unreachable!(),
            };
            assert!(
                (p.pos.x() - ex).abs() < tol && (p.pos.y() - ey).abs() < tol,
                "DEMO1@{t} pos=({:.2},{:.2}) expected Chocolate ~({ex},{ey})",
                p.pos.x(),
                p.pos.y()
            );
        }
        if dump_rng
            && matches!(
                t,
                1 | 10
                    | 50
                    | 70
                    | 80
                    | 90
                    | 100
                    | 200
                    | 300
                    | 310
                    | 320
                    | 330
                    | 340
                    | 350
                    | 360
                    | 370
                    | 380
                    | 390
                    | 400
                    | 410
                    | 420
                    | 430
                    | 440
                    | 441
                    | 442
            )
        {
            let w = game.world().unwrap();
            eprintln!("RND@{} idx={}", t, w.vanilla_rng.prndindex);
            for &id in &w.thinkers.default {
                let Some(a) = w.actors.get(id) else {
                    continue;
                };
                let Some(c) = w.classes.get(a.class) else {
                    continue;
                };
                if !c.name.eq_ignore_ascii_case("ZombieMan") {
                    continue;
                }
                // West ambush pair near (-224,672) / (-64,736).
                if a.spawn_point.x() > -250.0 && a.spawn_point.x() < 100.0 {
                    let tgt = a
                        .target
                        .and_then(|tid| w.actors.get(tid))
                        .map(|ta| format!("({:.1},{:.1})", ta.pos.x(), ta.pos.y()))
                        .unwrap_or_else(|| "-".into());
                    eprintln!(
                        "ZOMBIE@{} ({:.1},{:.1}) mc={} md={} hp={} spawn=({:.0},{:.0}) tgt={}",
                        t,
                        a.pos.x(),
                        a.pos.y(),
                        a.move_count,
                        a.move_dir,
                        a.health,
                        a.spawn_point.x(),
                        a.spawn_point.y(),
                        tgt
                    );
                }
            }
            let mo = w.players[0].as_ref().unwrap().mo.unwrap();
            let p = w.actors.get(mo).unwrap();
            eprintln!(
                "POS@{} ({:.3},{:.3}) hp={}",
                t,
                p.pos.x(),
                p.pos.y(),
                p.health
            );
            if matches!(t, 1 | 50 | 70 | 100 | 200 | 300 | 320 | 330 | 340) {
                let pl = w.players[0].as_ref().unwrap();
                let wpn = pl
                    .ready_weapon
                    .and_then(|id| w.classes.get(id))
                    .map(|c| c.name.as_str())
                    .unwrap_or("?");
                let shot = w
                    .classes
                    .find_name("Shotgun")
                    .map(|cid| w.inventory.count(mo, cid))
                    .unwrap_or(0);
                let mut map_shots = 0;
                for &id in &w.thinkers.default {
                    let Some(a) = w.actors.get(id) else { continue };
                    if a.destroyed {
                        continue;
                    }
                    let Some(c) = w.classes.get(a.class) else {
                        continue;
                    };
                    if c.name.eq_ignore_ascii_case("Shotgun") {
                        map_shots += 1;
                    }
                }
                let pending = pl
                    .pending_weapon
                    .and_then(|id| w.classes.get(id))
                    .map(|c| c.name.as_str())
                    .unwrap_or("-");
                eprintln!(
                    "SHOTINV@{} ready={} pending={} y={:.0} inv={} pos=({:.1},{:.1})",
                    t,
                    wpn,
                    pending,
                    pl.pspr_y,
                    shot,
                    p.pos.x(),
                    p.pos.y()
                );
            }
            for &id in &w.thinkers.default {
                let Some(a) = w.actors.get(id) else { continue };
                let Some(c) = w.classes.get(a.class) else {
                    continue;
                };
                if c.name.eq_ignore_ascii_case("ShotgunGuy")
                    && (a.spawn_point.x() - 320.0).abs() < 1.0
                    && (a.spawn_point.y() - 640.0).abs() < 1.0
                {
                    eprintln!(
                        "SGUN@{} ({:.1},{:.1}) tgt={} hp={} mc={} md={}",
                        t,
                        a.pos.x(),
                        a.pos.y(),
                        a.target.is_some(),
                        a.health,
                        a.move_count,
                        a.move_dir
                    );
                }
            }
        }
    }
    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    let a = game.world().unwrap().actors.get(mo).unwrap();
    // Chocolate DEMO1 ends dead at bit-exact XY (hp=-2).
    const END_X: f64 = 1468.330_582;
    const END_Y: f64 = -315.818_542;
    if std::env::var_os("ROOM_WRITE_DEMO1_POS").is_some() {
        eprintln!(
            "DEMO1@{} pos=({:.6},{:.6}) ang={:.6} hp={}",
            lmp.tics.len(),
            a.pos.x(),
            a.pos.y(),
            a.angle.degrees(),
            a.health
        );
    } else if std::env::var_os("ROOM_DUMP_DEMO1_PATH").is_none() {
        assert_eq!(a.health, -2, "DEMO1@end hp");
        assert!(
            (a.pos.x() - END_X).abs() < 1e-6 && (a.pos.y() - END_Y).abs() < 1e-6,
            "DEMO1@end pos=({:.6},{:.6}) expected ({END_X},{END_Y})",
            a.pos.x(),
            a.pos.y()
        );
    }
}

/// Freedoom CI subset: Complevel::Vanilla / Boom stamp round-trips demo verify.
#[test]
fn freedoom2_map01_complevel_vanilla_and_boom_demo_verifies() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let (classes, states) = doom_basic_defs();
    for (complevel, expect_math) in [
        (Complevel::Vanilla, SimMathMode::VanillaFixed),
        (Complevel::Boom, SimMathMode::Double),
    ] {
        let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
        let mut map = load_map(&vfs, "MAP01").unwrap();
        ensure_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            skill: Skill::new(3),
            rng_seed: 1993,
            record_demo: true,
            complevel,
            ..Default::default()
        });
        game.set_defs(Arc::clone(&classes), Arc::clone(&states));
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert_eq!(game.world().unwrap().math_mode, expect_math);
        let cmd = TicCmd {
            forward_move: 25 * 256,
            buttons: Buttons(Buttons::ATTACK),
            ..TicCmd::default()
        };
        for _ in 0..35 {
            game.tick(&[cmd]);
        }
        let demo = game.take_demo().unwrap();
        assert_eq!(demo.complevel, complevel);
        assert_eq!(demo.tics.len(), 35);

        let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
        let mut map2 = load_map(&vfs, "MAP01").unwrap();
        ensure_nodes(&mut map2);
        let mut world2 = room_sim::World::from_map_with_skill(
            map2,
            Arc::clone(&classes),
            Arc::clone(&states),
            3,
        )
        .unwrap();
        world2.rng.set_seed(1993);
        let result = DemoPlayer { demo: &demo }.verify(&mut world2).unwrap();
        assert_eq!(result.first_mismatch, None, "{complevel:?} demo desync");
        assert_eq!(world2.math_mode, expect_math);
    }
}

#[test]
fn freedoom2_map01_committed_demo_verifies() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing (run scripts/fetch-freedoom.sh)");
        return;
    };
    let demo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/demos/freedoom2_map01_smoke.fdem");
    if !demo_path.is_file() {
        eprintln!(
            "skip: {} missing (ROOM_WRITE_DEMO=1 cargo test -p room-game --test freedoom_smoke freedoom2_map01_headless_ticks_and_demo_verifies)",
            demo_path.display()
        );
        return;
    }

    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));

    let bytes = std::fs::read(&demo_path).unwrap();
    let demo = room_game::DemoRecorder::from_fdem(&bytes).unwrap();
    assert_eq!(demo.map, "MAP01");
    assert_eq!(demo.rng_seed, 1993);
    assert_eq!(demo.tics.len(), 70);

    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map);
    let mut world = room_sim::World::from_map_with_skill(map, classes, states, 3).unwrap();
    world.rng.set_seed(demo.rng_seed);
    let result = DemoPlayer { demo: &demo }.verify(&mut world).unwrap();
    assert_eq!(
        result.first_mismatch, None,
        "committed demo desync at tic {:?}",
        result.first_mismatch
    );
    assert_eq!(result.tics_ok, 70);
}

#[test]
fn freedoom2_map01_use_opens_translated_door() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };

    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));

    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map);

    // Door_Raise (Hexen 12) after XLAT of Doom 1 / 63 / 90 / 117…
    const DOOR_RAISE: i16 = 12;
    let line = map.lines.iter().find(|l| l.special == DOOR_RAISE).unwrap();
    let v1 = map.vertices[line.v1.raw() as usize].pos;
    let v2 = map.vertices[line.v2.raw() as usize].pos;
    let mid_x = (v1.x() + v2.x()) * 0.5;
    let mid_y = (v1.y() + v2.y()) * 0.5;
    let dx = v2.x() - v1.x();
    let dy = v2.y() - v1.y();
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let nx = -dy / len;
    let ny = dx / len;

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        record_demo: false,
        ..Default::default()
    });
    game.set_defs(classes, states);
    game.start_custom_map(map, Skill::new(3)).unwrap();

    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    {
        let a = game.world_mut().unwrap().actors.get_mut(mo).unwrap();
        a.pos = DVec3::new(mid_x + nx * 32.0, mid_y + ny * 32.0, 0.0);
        a.angle = Angle::from_radians((-ny).atan2(-nx));
    }

    assert!(
        use_lines(game.world_mut().unwrap(), mo),
        "use should activate Door_Raise on MAP01"
    );
    assert!(
        !game.world().unwrap().doors.is_empty(),
        "expected a door thinker after use"
    );
    for _ in 0..40 {
        game.tick(&[]);
    }
}

#[test]
fn freedoom2_map01_cross_teleport_moves_player() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };

    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));

    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map);

    // Teleport (Hexen 70) after XLAT of Doom 97.
    const TELEPORT: i16 = 70;
    let line = map.lines.iter().find(|l| l.special == TELEPORT).unwrap();
    let v1 = map.vertices[line.v1.raw() as usize].pos;
    let v2 = map.vertices[line.v2.raw() as usize].pos;
    let mid_x = (v1.x() + v2.x()) * 0.5;
    let mid_y = (v1.y() + v2.y()) * 0.5;
    let dx = v2.x() - v1.x();
    let dy = v2.y() - v1.y();
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let nx = -dy / len;
    let ny = dx / len;

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        record_demo: false,
        ..Default::default()
    });
    game.set_defs(classes, states);
    game.start_custom_map(map, Skill::new(3)).unwrap();

    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    let from = DVec2::new(mid_x + nx * 24.0, mid_y + ny * 24.0);
    let to = DVec2::new(mid_x - nx * 24.0, mid_y - ny * 24.0);
    {
        let a = game.world_mut().unwrap().actors.get_mut(mo).unwrap();
        a.pos = DVec3::new(from.x(), from.y(), 0.0);
    }
    let before = game.world().unwrap().actors.get(mo).unwrap().pos;
    cross_special_lines(game.world_mut().unwrap(), mo, from, to);
    let after = game.world().unwrap().actors.get(mo).unwrap().pos;
    let moved = (after.x() - before.x()).hypot(after.y() - before.y());
    assert!(
        moved > 8.0,
        "crossing teleport line should move player (delta {moved})"
    );
}

#[test]
fn freedoom2_map01_use_exit_enters_intermission() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };

    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));

    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map);

    // Exit_Normal (Hexen 243) after XLAT of Doom 11 / 52.
    const EXIT_NORMAL: i16 = 243;
    let line_idx = map
        .lines
        .iter()
        .position(|l| l.special == EXIT_NORMAL)
        .unwrap();

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        record_demo: false,
        ..Default::default()
    });
    game.set_defs(classes, states);
    game.start_custom_map(map, Skill::new(3)).unwrap();

    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    assert!(
        activate_line(
            game.world_mut().unwrap(),
            LineId::from_raw(line_idx as u32),
            Some(mo),
            ActivationType::Use,
        ),
        "Exit_Normal should activate"
    );
    assert!(
        game.world().unwrap().level.exiting,
        "exit special should request level exit"
    );
    // MAP01 exit uses SW1COMP on the lower texture.
    assert!(
        game.world()
            .unwrap()
            .switch_lowers
            .values()
            .any(|t| t.eq_ignore_ascii_case("SW2COMP")),
        "exit use should flip SW1COMP → SW2COMP"
    );
    game.tick(&[]);
    assert_eq!(game.state, GameState::Intermission);
    let tallies = game.intermission_stats().unwrap();
    assert_eq!(tallies.map_name, "MAP01");
    assert_eq!(tallies.next_map.as_deref(), Some("MAP02"));
    assert_eq!(tallies.par_tics, 30 * 35);
    assert!(!tallies.secret_exit);

    // Advance into MAP02 via the suggested next map.
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map2 = load_map(&vfs, "MAP02").unwrap();
    ensure_nodes(&mut map2);
    game.continue_to_map(map2).unwrap();
    assert_eq!(game.state, GameState::Level);
    assert_eq!(game.map_name(), "MAP02");
}

#[test]
fn freedoom2_map01_secret_sector_counts_when_entered() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };

    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));

    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map);

    const ST_SECRET: i16 = 9;
    let secret_idx = map
        .sectors
        .iter()
        .position(|s| s.special == ST_SECRET || (s.special & 0x80) != 0)
        .expect("MAP01 should have at least one secret sector");
    let sec = &map.sectors[secret_idx];
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut n = 0.0;
    for &lid in &sec.lines {
        let line = &map.lines[lid.raw() as usize];
        let v1 = &map.vertices[line.v1.raw() as usize];
        let v2 = &map.vertices[line.v2.raw() as usize];
        sx += v1.pos.x() + v2.pos.x();
        sy += v1.pos.y() + v2.pos.y();
        n += 2.0;
    }
    assert!(n > 0.0, "secret sector has geometry");
    let (px, py) = (sx / n, sy / n);

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        record_demo: false,
        ..Default::default()
    });
    game.set_defs(classes, states);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    assert!(game.world().unwrap().level.total_secrets >= 1);

    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
        let z = a.pos.z();
        a.pos = DVec3::new(px, py, z);
    }
    game.tick(&[]);
    assert!(
        game.world().unwrap().level.found_secrets >= 1,
        "entering secret sector should increment found_secrets"
    );
}

fn record_and_verify_bundle(wad: &PathBuf, maps: &[&str], out_name: &str) -> DemoBundle {
    let (classes, states) = doom_basic_defs();
    let vfs = VfsBuilder::new().mount_path(wad).unwrap().build().unwrap();
    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        record_demo: true,
        ..Default::default()
    });
    game.set_defs(Arc::clone(&classes), Arc::clone(&states));
    let walk = TicCmd {
        forward_move: 25 * 256,
        buttons: Buttons(Buttons::ATTACK),
        ..TicCmd::default()
    };
    for (i, name) in maps.iter().enumerate() {
        let mut map = load_map(&vfs, name).unwrap();
        ensure_nodes(&mut map);
        if i == 0 {
            game.start_custom_map(map, Skill::new(3)).unwrap();
        } else {
            game.continue_to_map(map).unwrap();
        }
        for _ in 0..35 {
            game.tick(&[walk]);
        }
        activate_exit_to_intermission(&mut game);
    }
    let bundle = game.take_demo_bundle();
    assert_eq!(bundle.demos.len(), maps.len());
    for (demo, name) in bundle.demos.iter().zip(maps) {
        assert_eq!(demo.map, *name);
    }
    if std::env::var_os("ROOM_WRITE_DEMO").is_some() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/demos")
            .join(out_name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, bundle.to_fdbn()).unwrap();
        eprintln!(
            "wrote {} fingerprint={:016x}",
            path.display(),
            bundle.fingerprint()
        );
    }
    for demo in &bundle.demos {
        let mut map = load_map(&vfs, &demo.map).unwrap();
        ensure_nodes(&mut map);
        let mut world = room_sim::World::from_map_with_skill(
            map,
            Arc::clone(&classes),
            Arc::clone(&states),
            3,
        )
        .unwrap();
        world.rng.set_seed(demo.rng_seed);
        let result = DemoPlayer { demo }.verify(&mut world).unwrap();
        assert_eq!(
            result.first_mismatch, None,
            "desync on {} at {:?}",
            demo.map, result.first_mismatch
        );
    }
    bundle
}

fn verify_committed_bundle(wad: &PathBuf, file: &str, expected_len: usize) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/demos")
        .join(file);
    if !path.is_file() {
        eprintln!(
            "skip: {} missing (ROOM_WRITE_DEMO=1 cargo test -p room-game --test freedoom_smoke)",
            path.display()
        );
        return;
    }
    let (classes, states) = doom_basic_defs();
    let vfs = VfsBuilder::new().mount_path(wad).unwrap().build().unwrap();
    let bundle = DemoBundle::from_fdbn(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(bundle.demos.len(), expected_len);
    for demo in &bundle.demos {
        let mut map = load_map(&vfs, &demo.map).unwrap();
        ensure_nodes(&mut map);
        let mut world = room_sim::World::from_map_with_skill(
            map,
            Arc::clone(&classes),
            Arc::clone(&states),
            3,
        )
        .unwrap();
        world.rng.set_seed(demo.rng_seed);
        let result = DemoPlayer { demo }.verify(&mut world).unwrap();
        assert_eq!(
            result.first_mismatch, None,
            "committed bundle desync on {}",
            demo.map
        );
    }
}

fn run_exit_chain_opt(wad: &PathBuf, maps: &[&str], after_last_next: Option<&str>) {
    let (classes, states) = doom_basic_defs();
    let vfs = VfsBuilder::new().mount_path(wad).unwrap().build().unwrap();
    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        record_demo: false,
        ..Default::default()
    });
    game.set_defs(Arc::clone(&classes), Arc::clone(&states));

    for (i, name) in maps.iter().enumerate() {
        let mut map = load_map(&vfs, name).unwrap();
        ensure_nodes(&mut map);
        if i == 0 {
            game.start_custom_map(map, Skill::new(3)).unwrap();
        } else {
            assert_eq!(game.state, GameState::Intermission);
            game.continue_to_map(map).unwrap();
        }
        assert_eq!(game.state, GameState::Level);
        assert_eq!(game.map_name(), *name);
        assert!(game.world().unwrap().players[0].is_some());
        activate_exit_to_intermission(&mut game);
        let next = game.intermission_stats().unwrap().next_map.clone();
        if i + 1 < maps.len() {
            assert_eq!(next.as_deref(), Some(maps[i + 1]));
        } else {
            assert_eq!(next.as_deref(), after_last_next);
            if after_last_next.is_none() {
                game.enter_finale();
                assert_eq!(game.state, GameState::Finale);
            }
        }
    }
}

fn run_exit_chain(wad: &PathBuf, maps: &[&str], after_last_next: &str) {
    run_exit_chain_opt(wad, maps, Some(after_last_next));
}

#[test]
fn freedoom2_map01_through_map05_exit_chain() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    run_exit_chain(
        &wad,
        &["MAP01", "MAP02", "MAP03", "MAP04", "MAP05"],
        "MAP06",
    );
}

#[test]
fn freedoom2_map01_through_map10_exit_chain() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    run_exit_chain(
        &wad,
        &[
            "MAP01", "MAP02", "MAP03", "MAP04", "MAP05", "MAP06", "MAP07", "MAP08", "MAP09",
            "MAP10",
        ],
        "MAP11",
    );
}

#[test]
fn freedoom1_e1m1_through_e1m3_exit_chain() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    run_exit_chain(&wad, &["E1M1", "E1M2", "E1M3"], "E1M4");
}

#[test]
fn freedoom1_e1m1_through_e1m8_exit_chain() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    // E1M8 is episode end (`ExM8` → next episode or finale).
    run_exit_chain_opt(
        &wad,
        &[
            "E1M1", "E1M2", "E1M3", "E1M4", "E1M5", "E1M6", "E1M7", "E1M8",
        ],
        Some("E2M1"),
    );
}

#[test]
fn freedoom1_e2m1_through_e2m8_exit_chain() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    run_exit_chain_opt(
        &wad,
        &[
            "E2M1", "E2M2", "E2M3", "E2M4", "E2M5", "E2M6", "E2M7", "E2M8",
        ],
        Some("E3M1"),
    );
}

#[test]
fn freedoom1_e3m1_through_e3m8_exit_chain() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    run_exit_chain_opt(
        &wad,
        &[
            "E3M1", "E3M2", "E3M3", "E3M4", "E3M5", "E3M6", "E3M7", "E3M8",
        ],
        Some("E4M1"),
    );
}

#[test]
fn freedoom1_e4m1_through_e4m8_exit_chain() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    run_exit_chain_opt(
        &wad,
        &[
            "E4M1", "E4M2", "E4M3", "E4M4", "E4M5", "E4M6", "E4M7", "E4M8",
        ],
        None,
    );
}

#[test]
fn freedoom2_map11_through_map20_exit_chain() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    run_exit_chain(
        &wad,
        &[
            "MAP11", "MAP12", "MAP13", "MAP14", "MAP15", "MAP16", "MAP17", "MAP18", "MAP19",
            "MAP20",
        ],
        "MAP21",
    );
}

#[test]
fn freedoom2_map21_through_map29_exit_chain() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    run_exit_chain(
        &wad,
        &[
            "MAP21", "MAP22", "MAP23", "MAP24", "MAP25", "MAP26", "MAP27", "MAP28", "MAP29",
        ],
        "MAP30",
    );
}

#[test]
fn freedoom2_map30_exit_enters_finale() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    run_exit_chain_opt(&wad, &["MAP30"], None);
}

#[test]
fn freedoom2_map01_through_map03_demo_bundle() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let _ = record_and_verify_bundle(
        &wad,
        &["MAP01", "MAP02", "MAP03"],
        "freedoom2_map01_03.fdbn",
    );
}

#[test]
fn freedoom2_committed_map01_03_bundle_verifies() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    verify_committed_bundle(&wad, "freedoom2_map01_03.fdbn", 3);
}

#[test]
fn freedoom2_map01_through_map10_demo_bundle() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let maps = [
        "MAP01", "MAP02", "MAP03", "MAP04", "MAP05", "MAP06", "MAP07", "MAP08", "MAP09", "MAP10",
    ];
    let _ = record_and_verify_bundle(&wad, &maps, "freedoom2_map01_10.fdbn");
}

#[test]
fn freedoom2_committed_map01_10_bundle_verifies() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    verify_committed_bundle(&wad, "freedoom2_map01_10.fdbn", 10);
}

#[test]
fn freedoom2_map11_through_map20_demo_bundle() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let maps = [
        "MAP11", "MAP12", "MAP13", "MAP14", "MAP15", "MAP16", "MAP17", "MAP18", "MAP19", "MAP20",
    ];
    let _ = record_and_verify_bundle(&wad, &maps, "freedoom2_map11_20.fdbn");
}

#[test]
fn freedoom2_committed_map11_20_bundle_verifies() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    verify_committed_bundle(&wad, "freedoom2_map11_20.fdbn", 10);
}

#[test]
fn freedoom2_map21_through_map30_demo_bundle() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    let maps = [
        "MAP21", "MAP22", "MAP23", "MAP24", "MAP25", "MAP26", "MAP27", "MAP28", "MAP29", "MAP30",
    ];
    let _ = record_and_verify_bundle(&wad, &maps, "freedoom2_map21_30.fdbn");
}

#[test]
fn freedoom2_committed_map21_30_bundle_verifies() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing");
        return;
    };
    verify_committed_bundle(&wad, "freedoom2_map21_30.fdbn", 10);
}

#[test]
fn freedoom1_e1m1_headless_exit_to_e1m2() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    let (classes, states) = doom_basic_defs();
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        record_demo: false,
        ..Default::default()
    });
    game.set_defs(classes, states);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    assert_eq!(game.map_name(), "E1M1");
    activate_exit_to_intermission(&mut game);
    let tallies = game.intermission_stats().unwrap();
    assert_eq!(tallies.next_map.as_deref(), Some("E1M2"));
}

#[test]
fn freedoom1_e1_demo_bundle() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    let maps = [
        "E1M1", "E1M2", "E1M3", "E1M4", "E1M5", "E1M6", "E1M7", "E1M8",
    ];
    let _ = record_and_verify_bundle(&wad, &maps, "freedoom1_e1.fdbn");
}

#[test]
fn freedoom1_committed_e1_bundle_verifies() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    verify_committed_bundle(&wad, "freedoom1_e1.fdbn", 8);
}

#[test]
fn freedoom1_e2_demo_bundle() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    let maps = [
        "E2M1", "E2M2", "E2M3", "E2M4", "E2M5", "E2M6", "E2M7", "E2M8",
    ];
    let _ = record_and_verify_bundle(&wad, &maps, "freedoom1_e2.fdbn");
}

#[test]
fn freedoom1_committed_e2_bundle_verifies() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    verify_committed_bundle(&wad, "freedoom1_e2.fdbn", 8);
}

#[test]
fn freedoom1_e3_demo_bundle() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    let maps = [
        "E3M1", "E3M2", "E3M3", "E3M4", "E3M5", "E3M6", "E3M7", "E3M8",
    ];
    let _ = record_and_verify_bundle(&wad, &maps, "freedoom1_e3.fdbn");
}

#[test]
fn freedoom1_committed_e3_bundle_verifies() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    verify_committed_bundle(&wad, "freedoom1_e3.fdbn", 8);
}

#[test]
fn freedoom1_e4_demo_bundle() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    let maps = [
        "E4M1", "E4M2", "E4M3", "E4M4", "E4M5", "E4M6", "E4M7", "E4M8",
    ];
    let _ = record_and_verify_bundle(&wad, &maps, "freedoom1_e4.fdbn");
}

#[test]
fn freedoom1_committed_e4_bundle_verifies() {
    let Some(wad) = freedoom1() else {
        eprintln!("skip: testdata/freedoom/freedoom1.wad missing");
        return;
    };
    verify_committed_bundle(&wad, "freedoom1_e4.fdbn", 8);
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

/// Two peers exchange cmds over the signal mesh; both Headless MAP01 worlds stay in sync.
#[test]
fn freedoom2_map01_two_peer_lockstep_stays_synced() {
    let Some(wad) = freedoom2() else {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing (run scripts/fetch-freedoom.sh)");
        return;
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
    assert_eq!(g0.checksum(), g1.checksum());

    let bus = SharedSignalBus::new();
    let p0 = PeerId(0);
    let p1 = PeerId(1);
    let mut t0 = SignalMeshTransport::join(bus.clone(), "fd2", p0).unwrap();
    let mut t1 = SignalMeshTransport::join(bus, "fd2", p1).unwrap();
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

    // Peer0 drives forward+attack; peer1 is idle (single-player seats, same inputs applied).
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

        // Release one sim frame on each (first iteration releases seeded tic 0).
        let f0 = s0.try_advance().expect("peer0 frame");
        let f1 = s1.try_advance().expect("peer1 frame");
        assert_eq!(f0, f1);

        // Both worlds apply peer0's command (authoritative seat for this smoke).
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
        // Cross-check via the other peer id using the same checksum value.
        s0.report_checksum(tic, p1, g1.checksum().unwrap());
        s1.report_checksum(tic, p0, g0.checksum().unwrap());
        s0.check_desync(tic).unwrap();
        s1.check_desync(tic).unwrap();
    }
    assert_eq!(g0.state, GameState::Level);
    assert_ne!(g0.checksum(), Some(0));
}
