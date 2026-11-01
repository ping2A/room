//! The example WASM mod (`mods/doom1-survival/survival.wad`) end to end:
//! discovered from a PWAD, classes merged, waves spawned on schedule per skill.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bytes::Bytes;
use room_game::{GameOptions, Headless, Skill, discover_mods, load_embedded_engine_data};
use room_map::{MapGeometry, load_map};
use room_nodebuild::ensure_nodes;
use room_sim::TicCmd;
use room_sim::mod_hud::layer;
use room_sim::script_mods::ModSource;
use room_testkit::{MapBuilder, rect};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

const SURVIVAL_WAD: &[u8] = include_bytes!("../../../mods/doom1-survival/survival.wad");

fn survival_mods() -> Vec<ModSource> {
    let vfs = VfsBuilder::new()
        .mount_memory("survival.wad", Bytes::from_static(SURVIVAL_WAD))
        .unwrap()
        .build()
        .unwrap();
    let (mods, errors) = discover_mods(&vfs);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(mods.len(), 1);
    mods
}

fn arena() -> MapGeometry {
    let mut b = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .player_start(128.0, 128.0, 0);
    for (x, y) in [
        (1800.0, 1800.0),
        (1800.0, 300.0),
        (300.0, 1800.0),
        (1000.0, 1000.0),
    ] {
        b = b.thing(2007, x, y, 0);
    }
    let mut map = b.build_udmf("MAP01").unwrap();
    room_nodebuild::apply_nodes(&mut map);
    map
}

/// Two-seat arena with Player 2 start (DoomEdNum 2).
fn arena_coop() -> MapGeometry {
    let mut b = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .player_start(128.0, 128.0, 0)
        .thing(2, 192.0, 128.0, 0);
    for (x, y) in [
        (1800.0, 1800.0),
        (1800.0, 300.0),
        (300.0, 1800.0),
        (1000.0, 1000.0),
    ] {
        b = b.thing(2007, x, y, 0);
    }
    let mut map = b.build_udmf("MAP01").unwrap();
    room_nodebuild::apply_nodes(&mut map);
    map
}

/// Play sector + disconnected sealed closet (only spawn spot inside closet).
fn sealed_closet() -> MapGeometry {
    let mut map = MapBuilder::new()
        .room("play", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        // Gap so walls are one-sided — no walkable/shared edge.
        .room("closet", rect(2000.0, 0.0, 2304.0, 256.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .thing(2007, 2152.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    room_nodebuild::apply_nodes(&mut map);
    map
}

fn run(map: MapGeometry, skill: u8, tics: u32) -> (Headless, Vec<String>) {
    run_players(map, skill, tics, 1)
}

fn run_players(map: MapGeometry, skill: u8, tics: u32, players: u8) -> (Headless, Vec<String>) {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(skill);
    let mut game = Headless::new(GameOptions {
        skill,
        coop_players: players,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(game.set_script_mods(survival_mods()).unwrap(), ["survival"]);
    game.start_custom_map(map, skill).unwrap();
    let mut msgs = Vec::new();
    let cmds = vec![TicCmd::default(); players as usize];
    for _ in 0..tics {
        game.tick(&cmds);
        msgs.extend(game.world_mut().unwrap().drain_messages());
        msgs.extend(game.drain_script_log());
    }
    (game, msgs)
}

fn count(game: &Headless, class: &str) -> usize {
    let w = game.world().unwrap();
    let Some(id) = w.classes.find_name(class) else {
        return 0;
    };
    w.thinkers
        .default
        .iter()
        .filter(|&&a| {
            w.actors
                .get(a)
                .is_some_and(|a| a.class == id && a.health > 0)
        })
        .count()
}

#[test]
fn survival_mod_registers_classes_and_spawns_first_wave_on_skill_schedule() {
    // UV: 32 s between waves, first wave after half of that (560 tics).
    let (game, msgs) = run(arena(), 4, 540);
    assert!(game.classes().find_name("PhantomImp").is_some());
    assert!(game.classes().find_name("Juggernaut").is_some());
    assert!(
        msgs.iter()
            .any(|m| m == "SURVIVAL - skill 4 - first wave in 16s"),
        "{msgs:?}"
    );
    assert_eq!(count(&game, "ZombieMan"), 0, "no wave yet");

    // Announcer gaps team lines by 1.5 s — allow past ROUND for the Wave line.
    let (game, msgs) = run(arena(), 4, 700);
    assert!(msgs.iter().any(|m| m == "Wave 1 - 6 incoming"), "{msgs:?}");
    let zombies = count(&game, "ZombieMan");
    let elites =
        count(&game, "EliteZombie") + count(&game, "EliteImp") + count(&game, "EliteDemon");
    assert!(zombies + elites > 0, "wave 1 spawned monsters");
    assert_eq!(
        game.world().unwrap().level.total_kills as usize,
        zombies + elites,
        "rejected spawns are not counted"
    );
    assert!(!msgs.iter().any(|m| m.contains("disabled")), "{msgs:?}");

    // Nightmare: 25 s gap, first wave at 437 tics (+ announcer flush).
    let (_, msgs) = run(arena(), 5, 550);
    assert!(msgs.iter().any(|m| m.starts_with("Wave 1 - ")), "{msgs:?}");
}

#[test]
fn survival_mod_is_deterministic() {
    let (a, _) = run(arena(), 3, 900);
    let (b, _) = run(arena(), 3, 900);
    assert_eq!(a.world().unwrap().checksum(), b.world().unwrap().checksum());
}

#[test]
fn survival_coop_continues_when_player0_dead() {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(4);
    let mut game = Headless::new(GameOptions {
        skill,
        coop_players: 2,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(game.set_script_mods(survival_mods()).unwrap(), ["survival"]);
    game.start_custom_map(arena_coop(), skill).unwrap();
    assert!(game.world().unwrap().players[1].is_some());

    // Kill seat 0 before the first wave.
    {
        let w = game.world_mut().unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.health = 0;
        }
    }

    let mut msgs = Vec::new();
    let cmds = [TicCmd::default(), TicCmd::default()];
    for _ in 0..700 {
        game.tick(&cmds);
        msgs.extend(game.world_mut().unwrap().drain_messages());
        msgs.extend(game.drain_script_log());
    }
    assert!(
        msgs.iter().any(|m| m.starts_with("Wave 1 - ")),
        "wave must still spawn with seat 0 dead: {msgs:?}"
    );
    let zombies = count(&game, "ZombieMan");
    assert!(zombies > 0, "expected wave monsters with p0 dead");
    assert!(!msgs.iter().any(|m| m.contains("disabled")), "{msgs:?}");
}

#[test]
fn survival_stuck_closet_recycles_and_clears_wave() {
    // UV: first wave ~560 tics; stuck (12 s) or straggler (30 s) must end the round.
    // Stop soon after the first clear so a follow-up wave does not re-fill the map.
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(4);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(game.set_script_mods(survival_mods()).unwrap(), ["survival"]);
    game.start_custom_map(sealed_closet(), skill).unwrap();
    let mut msgs = Vec::new();
    let mut cleared_at = None;
    let mut wave_at = None;
    for t in 0..1800u32 {
        game.tick(&[TicCmd::default()]);
        let drained = game.world_mut().unwrap().drain_messages();
        msgs.extend(drained.iter().cloned());
        msgs.extend(game.drain_script_log());
        if wave_at.is_none() && drained.iter().any(|m| m.starts_with("Wave 1 - ")) {
            wave_at = Some(t);
        }
        if drained.iter().any(|m| m == "Wave 1 cleared!") {
            cleared_at = Some(t);
            break;
        }
    }
    let wave_at = wave_at.expect(&format!("expected wave spawn: {msgs:?}"));
    let cleared_at = cleared_at.expect(&format!("recycle should end the round: {msgs:?}"));
    // Stuck window is 10s; allow one check period of slack (≤15s acceptance).
    assert!(
        cleared_at - wave_at <= 15 * 35,
        "recycle took {} tics after spawn (limit 15s / wave_at={wave_at} cleared_at={cleared_at}): {msgs:?}",
        cleared_at - wave_at
    );
    assert_eq!(
        count(&game, "ZombieMan"),
        0,
        "wave leftovers must be destroyed on recycle"
    );
    assert!(!msgs.iter().any(|m| m.contains("disabled")), "{msgs:?}");
}

#[test]
fn survival_draft_modal_picks_with_weapon_select() {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(4);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(game.set_script_mods(survival_mods()).unwrap(), ["survival"]);
    game.start_custom_map(sealed_closet(), skill).unwrap();

    let mut cleared = false;
    for _ in 0..1800u32 {
        game.tick(&[TicCmd::default()]);
        let drained = game.world_mut().unwrap().drain_messages();
        if drained.iter().any(|m| m == "Wave 1 cleared!") {
            cleared = true;
            break;
        }
    }
    assert!(cleared, "need intermission for draft");

    let modal_before = {
        let w = game.world().unwrap();
        w.mod_hud
            .iter()
            .filter(|(_, wid)| wid.layer == layer::MODAL)
            .count()
    };
    assert!(
        modal_before >= 3,
        "expected draft MODAL widgets, got {modal_before}"
    );
    assert_ne!(
        game.world().unwrap().players[0]
            .as_ref()
            .unwrap()
            .input_capture,
        0,
        "draft should capture input"
    );
    assert!(
        game.world().unwrap().paused,
        "playsim must pause while the draft is open"
    );
    let maptime_paused = game.world().unwrap().level.maptime;
    game.tick(&[TicCmd::default()]);
    assert_eq!(
        game.world().unwrap().level.maptime,
        maptime_paused,
        "maptime must not advance while draft-paused"
    );
    assert!(game.world().unwrap().paused, "still paused before pick");
    let ready_before = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .ready_weapon;

    // Pick card 2.
    game.tick(&[TicCmd {
        weapon_select: 2,
        ..Default::default()
    }]);
    let w = game.world().unwrap();
    let modal_after = w
        .mod_hud
        .iter()
        .filter(|(_, wid)| wid.layer == layer::MODAL)
        .count();
    assert_eq!(modal_after, 0, "draft panels should close after pick");
    assert_eq!(
        w.players[0].as_ref().unwrap().input_capture,
        0,
        "capture released"
    );
    assert!(!w.paused, "playsim must unpause once the draft closes");
    assert_eq!(
        w.players[0].as_ref().unwrap().ready_weapon,
        ready_before,
        "weapon must not switch while menu captured"
    );
}

#[test]
fn survival_extraction_exits_after_final_round_itytd() {
    // Skill 1: final_round=2; sealed closet clears waves via stuck recycle.
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(1);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(game.set_script_mods(survival_mods()).unwrap(), ["survival"]);
    game.start_custom_map(sealed_closet(), skill).unwrap();

    let mut msgs = Vec::new();
    let mut exited = false;
    for _ in 0..8000u32 {
        // Auto-advance drafts if open (weapon_select 1).
        let cap = game.world().unwrap().players[0]
            .as_ref()
            .map(|p| p.input_capture)
            .unwrap_or(0);
        let cmd = if cap != 0 {
            TicCmd {
                weapon_select: 1,
                ..Default::default()
            }
        } else {
            TicCmd::default()
        };
        game.tick(&[cmd]);
        msgs.extend(game.world_mut().unwrap().drain_messages());
        if game.world().unwrap().level.exiting {
            exited = true;
            break;
        }
    }
    assert!(
        exited,
        "expected exit_level after extraction: msgs={msgs:?}"
    );
    assert!(
        msgs.iter()
            .any(|m| m.contains("EXTRACTION") || m.contains("Extracted")),
        "{msgs:?}"
    );
}

#[test]
fn survival_phase_messages_in_order() {
    let (_, msgs) = run(arena(), 4, 900);
    let idx = |prefix: &str| {
        msgs.iter()
            .position(|m| m.starts_with(prefix))
            .unwrap_or(usize::MAX)
    };
    let i_round = idx("ROUND 1 - ");
    let i_wave = idx("Wave 1 - ");
    assert!(i_round < usize::MAX, "missing ROUND banner: {msgs:?}");
    assert!(i_wave < usize::MAX, "missing Wave line: {msgs:?}");
    assert!(i_round < i_wave, "ROUND should precede Wave: {msgs:?}");
}

#[test]
fn survival_elite_spawns_by_round_3_on_uv() {
    let (game, msgs) = run(arena(), 4, 700);
    assert!(game.classes().find_name("EliteImp").is_some());
    assert!(game.classes().find_name("EliteZombie").is_some());
    assert!(game.classes().find_name("EliteDemon").is_some());
    let elites =
        count(&game, "EliteZombie") + count(&game, "EliteImp") + count(&game, "EliteDemon");
    let announced = msgs.iter().any(|m| {
        m.contains("Brute ")
            || m.contains("Haste ")
            || m.contains("Volatile ")
            || m.contains("Vampiric ")
            || m.contains("Splitter ")
            || m.contains("Summoner ")
            || m.contains("Phasing ")
            || m.contains("Regenerating ")
    });
    assert!(
        elites > 0 || announced,
        "expected an elite by wave 1 on UV: elites={elites} msgs={msgs:?}"
    );
}

#[test]
fn survival_announcer_one_team_message_per_gap() {
    // Drain messages per tic and assert no two team prints within 1.5 s (52 tics).
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(4);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(game.set_script_mods(survival_mods()).unwrap(), ["survival"]);
    game.start_custom_map(arena(), skill).unwrap();
    let mut last_team_tic: Option<u32> = None;
    for t in 0..700u32 {
        game.tick(&[TicCmd::default()]);
        let drained = game.world_mut().unwrap().drain_messages();
        if drained.is_empty() {
            continue;
        }
        // player_print to everyone lands as world messages; treat each drain
        // batch as one team print event at this tic.
        if let Some(prev) = last_team_tic {
            assert!(
                t - prev >= 52,
                "team messages closer than 1.5s: prev={prev} t={t} msgs={drained:?}"
            );
        }
        last_team_tic = Some(t);
    }
    assert!(last_team_tic.is_some(), "expected some announcer output");
}

#[test]
fn survival_wad_packs_track_b_lumps_and_decorate_class() {
    use room_decorate::parse_decorate;
    use room_sim::{ClassRegistry, StateTable};
    use std::sync::Arc;

    let vfs = VfsBuilder::new()
        .mount_memory("survival.wad", Bytes::from_static(SURVIVAL_WAD))
        .unwrap()
        .build()
        .unwrap();
    for lump in ["DECORATE", "GLDEFS", "SNDINFO"] {
        assert!(
            vfs.find_any_ns(lump).is_some(),
            "missing Track B lump {lump}"
        );
    }
    let dec = vfs.read(vfs.find_any_ns("DECORATE").unwrap()).unwrap();
    let text = std::str::from_utf8(&dec).unwrap();
    let ir = parse_decorate(text).expect("DECORATE parse");
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let _states = Arc::new(StateTable::from_decorate(&ir, &classes));
    assert!(
        classes.find_name("SurvivalBeacon").is_some(),
        "SurvivalBeacon from Track B DECORATE"
    );
    // Mod still loads with content lumps present.
    assert_eq!(
        Headless::new(GameOptions::default())
            .set_script_mods(survival_mods())
            .unwrap(),
        ["survival"]
    );
}

#[test]
fn survival_warden_phases_and_boss_bar() {
    // HNTR: final_round=10; sealed closet clears non-boss waves via stuck recycle → boss @ 5.
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(2);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(game.set_script_mods(survival_mods()).unwrap(), ["survival"]);
    assert!(game.classes().find_name("Warden").is_some());
    game.start_custom_map(sealed_closet(), skill).unwrap();

    let mut msgs = Vec::new();
    let mut boss_id = None;
    let mut max_hp = 0;
    for _ in 0..25_000u32 {
        let cap = game.world().unwrap().players[0]
            .as_ref()
            .map(|p| p.input_capture)
            .unwrap_or(0);
        let cmd = if cap != 0 {
            TicCmd {
                weapon_select: 1,
                ..Default::default()
            }
        } else {
            TicCmd::default()
        };
        game.tick(&[cmd]);
        msgs.extend(game.world_mut().unwrap().drain_messages());
        let saw_boss_round = msgs
            .iter()
            .any(|m| m.contains("Warden approaches") || m.contains("BOSS:"));
        if !saw_boss_round {
            continue;
        }
        let w = game.world().unwrap();
        let warden = w.classes.find_name("Warden");
        for &aid in &w.thinkers.default {
            if let Some(a) = w.actors.get(aid) {
                if a.health > 0 && Some(a.class) == warden {
                    boss_id = Some(aid);
                    max_hp = a.health;
                    break;
                }
            }
        }
        if boss_id.is_some() {
            break;
        }
    }
    let id = boss_id.expect(&format!("expected live Warden by wave 5: msgs={msgs:?}"));
    assert!(
        msgs.iter()
            .any(|m| m.contains("Warden arrives") || m.contains("Warden approaches")),
        "{msgs:?}"
    );
    {
        let w = game.world_mut().unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.health = (max_hp * 66) / 100;
        }
    }
    for _ in 0..35 {
        game.tick(&[TicCmd::default()]);
        msgs.extend(game.world_mut().unwrap().drain_messages());
    }
    {
        let w = game.world_mut().unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.health = (max_hp * 33) / 100;
        }
    }
    for _ in 0..70 {
        game.tick(&[TicCmd::default()]);
        msgs.extend(game.world_mut().unwrap().drain_messages());
    }
    assert!(
        msgs.iter()
            .any(|m| m.contains("reinforcements") || m.contains("WARDEN -")),
        "phase 2: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("ENRAG")),
        "phase 3: {msgs:?}"
    );
    let bar = game
        .world()
        .unwrap()
        .mod_hud
        .iter()
        .find(|(_, w)| w.kind == room_sim::mod_hud::kind::BAR && w.max > 0);
    assert!(bar.is_some(), "boss bar widget missing");
    let (_, bar) = bar.unwrap();
    assert!(bar.value <= bar.max);
    assert!(bar.value <= (max_hp * 33) / 100 + 1);
}

#[test]
fn survival_mod_on_doom1_e1m1() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    if !p.is_file() {
        eprintln!("skip: testdata/doom/doom1.wad missing");
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&p).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let before = {
        let (game, _) = run(map.clone(), 3, 1);
        count(&game, "ZombieMan") + count(&game, "ShotgunGuy")
    };
    // HMP: first wave after 20 s (+ announcer flush for the Wave line).
    let (game, msgs) = run(map, 3, 800);
    assert!(msgs.iter().any(|m| m == "Wave 1 - 5 incoming"), "{msgs:?}");
    let after = count(&game, "ZombieMan") + count(&game, "ShotgunGuy");
    assert!(after > before, "wave spawned on E1M1 ({before} → {after})");
}
