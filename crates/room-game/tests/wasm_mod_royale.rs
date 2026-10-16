//! The Battle Royale example mod (`mods/doom1-royale/royale.wad`) end to end:
//! monsters removed, fists only, players scattered, PvP on, one life each,
//! shrinking zone, last player standing ends the match (final standings).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bytes::Bytes;
use room_core::DVec3;
use room_game::{
    GameOptions, GameState, Headless, Skill, discover_mods, load_embedded_engine_data,
};
use room_map::{MapGeometry, load_map};
use room_nodebuild::ensure_nodes;
use room_sim::script_mods::ModSource;
use room_sim::{ActorId, Buttons, MatchOver, MessageTo, PlayerState, TicCmd, World};
use room_testkit::{MapBuilder, rect};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

const ROYALE_WAD: &[u8] = include_bytes!("../../../mods/doom1-royale/royale.wad");
const DM_STARTS: [(f64, f64); 4] = [
    (200.0, 200.0),
    (1850.0, 1850.0),
    (200.0, 1850.0),
    (1850.0, 200.0),
];
const LOOT: [&str; 15] = [
    "HealthBonus",
    "Shotgun",
    "Chaingun",
    "RocketLauncher",
    "Chainsaw",
    "Shell",
    "ShellBox",
    "Clip",
    "ClipBox",
    "RocketAmmo",
    "Stimpack",
    "Medikit",
    "GreenArmor",
    "ArmorBonus",
    "Berserk",
];

fn royale_mods() -> Vec<ModSource> {
    let vfs = VfsBuilder::new()
        .mount_memory("royale.wad", Bytes::from_static(ROYALE_WAD))
        .unwrap()
        .build()
        .unwrap();
    let (mods, errors) = discover_mods(&vfs);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(mods.len(), 1);
    mods
}

/// 2048² room, two player starts, four zombies spread out; with `dm`, four
/// deathmatch starts near the corners.
fn arena_with(dm: bool) -> MapGeometry {
    let mut b = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .player_start(128.0, 128.0, 0)
        .thing(2, 192.0, 128.0, 0);
    for (x, y) in [
        (1800.0, 1800.0),
        (1800.0, 300.0),
        (300.0, 1800.0),
        (1000.0, 1000.0),
        (1000.0, 300.0),
        (300.0, 1000.0),
        (1700.0, 1000.0),
        (1000.0, 1700.0),
    ] {
        b = b.thing(3004, x, y, 0);
    }
    if dm {
        for (x, y) in DM_STARTS {
            b = b.thing(11, x, y, 0);
        }
    }
    let mut map = b.build_udmf("MAP01").unwrap();
    room_nodebuild::apply_nodes(&mut map);
    map
}

fn arena() -> MapGeometry {
    arena_with(false)
}

fn start(map: MapGeometry, players: u8, with_mod: bool) -> Headless {
    start_timed(map, players, with_mod, 0)
}

fn start_timed(map: MapGeometry, players: u8, with_mod: bool, limit_tics: u32) -> Headless {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(4);
    let mut game = Headless::new(GameOptions {
        skill,
        coop_players: players,
        time_limit_tics: limit_tics,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    if with_mod {
        assert_eq!(game.set_script_mods(royale_mods()).unwrap(), ["royale"]);
    }
    game.start_custom_map(map, skill).unwrap();
    game
}

fn run(game: &mut Headless, tics: u32, cmds: &[TicCmd], msgs: &mut Vec<String>) {
    for _ in 0..tics {
        game.tick(cmds);
        if let Some(w) = game.world_mut() {
            msgs.extend(w.drain_messages());
        }
        msgs.extend(game.drain_script_log());
    }
}

fn body(w: &World, slot: usize) -> ActorId {
    w.players[slot].as_ref().unwrap().mo.unwrap()
}

fn alive_monsters(w: &World) -> usize {
    w.thinkers
        .default
        .iter()
        .filter(|&&a| {
            w.actors.get(a).is_some_and(|a| {
                !a.destroyed && a.health > 0 && a.flags & room_sim::FLAG_COUNTKILL != 0
            })
        })
        .count()
}

fn loot_count(w: &World) -> usize {
    let ids: Vec<_> = LOOT.iter().filter_map(|n| w.classes.find_name(n)).collect();
    w.thinkers
        .default
        .iter()
        .filter(|&&a| {
            w.actors
                .get(a)
                .is_some_and(|a| !a.destroyed && ids.contains(&a.class))
        })
        .count()
}

fn items_of(w: &World, class: &str) -> Vec<DVec3> {
    let cls = w.classes.find_name(class).unwrap();
    w.thinkers
        .default
        .iter()
        .filter_map(|&a| w.actors.get(a))
        .filter(|a| !a.destroyed && a.class == cls)
        .map(|a| a.pos)
        .collect()
}

fn flat(a: DVec3, b: DVec3) -> f64 {
    let (dx, dy) = (a.x() - b.x(), a.y() - b.y());
    (dx * dx + dy * dy).sqrt()
}

fn owns(w: &World, slot: usize, class: &str) -> bool {
    let cls = w.classes.find_name(class).unwrap();
    w.inventory.count(body(w, slot), cls) > 0
}

#[test]
fn royale_clears_monsters_strips_guns_and_scatters_players() {
    let mut game = start(arena(), 2, true);
    let before = loot_count(game.world().unwrap());
    let mut msgs = Vec::new();
    run(&mut game, 40, &[TicCmd::default(); 2], &mut msgs);
    let w = game.world().unwrap();
    assert!(
        msgs.iter()
            .any(|m| m.starts_with("BATTLE ROYALE - 2 players")),
        "{msgs:?}"
    );
    assert_eq!(alive_monsters(w), 0, "no enemies");
    assert!(w.deathmatch && w.no_player_respawn);
    assert!(loot_count(w) > before, "loot dropped on monster spots");
    let fist = w.classes.find_name("Fist");
    for slot in 0..2 {
        assert!(!owns(w, slot, "Pistol"), "seat {slot} has no pistol");
        assert!(!owns(w, slot, "Clip"), "seat {slot} has no bullets");
        assert!(owns(w, slot, "Fist"));
        assert_eq!(w.players[slot].as_ref().unwrap().ready_weapon, fist);
    }
    let (a, b) = (
        w.actors.get(body(w, 0)).unwrap().pos,
        w.actors.get(body(w, 1)).unwrap().pos,
    );
    let d = (a - b).length();
    assert!(d > 800.0, "players scattered to monster spots ({d})");
}

#[test]
fn players_hurt_each_other_and_the_last_one_standing_wins() {
    // Vanilla coop: no player-vs-player damage.
    let mut vanilla = start(arena(), 2, false);
    vanilla.tick(&[TicCmd::default(); 2]);
    let w = vanilla.world_mut().unwrap();
    let (p0, p1) = (body(w, 0), body(w, 1));
    assert_eq!(room_sim::damage_mobj(w, p1, Some(p0), Some(p0), 10), 0);

    let mut game = start(arena(), 2, true);
    let mut msgs = Vec::new();
    run(&mut game, 2, &[TicCmd::default(); 2], &mut msgs);
    let w = game.world_mut().unwrap();
    let (p0, p1) = (body(w, 0), body(w, 1));
    assert!(room_sim::damage_mobj(w, p1, Some(p0), Some(p0), 10) > 0);
    assert!(room_sim::damage_mobj(w, p1, Some(p0), Some(p0), 500) > 0);

    // Seat 1 hammers Use: no respawn in a battle royale.
    let mut cmds = [TicCmd::default(); 2];
    cmds[1].buttons = Buttons(Buttons::USE);
    let mut hud = Vec::new();
    for _ in 0..70 {
        game.tick(&cmds);
        hud.extend(game.world_mut().unwrap().drain_hud_messages());
    }
    let to = |text: &str| hud.iter().find(|m| m.text.starts_with(text)).map(|m| m.to);
    assert_eq!(to("Player 1 fragged"), Some(MessageTo::All), "{hud:?}");
    assert_eq!(to("Eliminated - you placed #2"), Some(MessageTo::Seat(1)));
    assert_eq!(to("Winner!"), Some(MessageTo::Seat(0)));
    msgs.extend(hud.into_iter().map(|m| m.text));
    let w = game.world().unwrap();
    assert_eq!(
        w.players[1].as_ref().unwrap().player_state,
        PlayerState::Dead
    );
    assert_eq!(w.players[0].as_ref().unwrap().frags[1], 1);
    assert!(
        msgs.iter()
            .any(|m| m == "Player 1 fragged Player 2 - 1 left"),
        "{msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m == "PLAYER 1 WINS THE BATTLE ROYALE!"),
        "{msgs:?}"
    );
    assert_eq!(game.world().unwrap().match_over, None, "winner time first");
    run(&mut game, 3 * 35, &cmds, &mut msgs);
    let w = game.world().unwrap();
    assert!(matches!(
        w.match_over,
        Some(MatchOver {
            winner: Some(0),
            time_up: false,
            ..
        })
    ));
    assert_eq!(game.state, GameState::Level, "no next map: the room ends");
    let table = w.standings();
    assert_eq!(table.len(), 2);
    assert_eq!((table[0].slot, table[0].place, table[0].frags), (0, 1, 1));
    assert!(table[0].alive);
    assert_eq!((table[1].slot, table[1].place, table[1].deaths), (1, 2, 1));
    assert!(!table[1].alive);
    assert!(!msgs.iter().any(|m| m.contains("disabled")), "{msgs:?}");
}

#[test]
fn players_spawn_on_deathmatch_starts_and_every_gun_has_ammo() {
    let mut game = start(arena_with(true), 2, true);
    let mut msgs = Vec::new();
    run(&mut game, 3, &[TicCmd::default(); 2], &mut msgs);
    let w = game.world().unwrap();
    let starts: Vec<DVec3> = DM_STARTS
        .iter()
        .map(|&(x, y)| DVec3::new(x, y, 0.0))
        .collect();
    let mut spawns = Vec::new();
    for slot in 0..2 {
        let p = w.actors.get(body(w, slot)).unwrap().pos;
        assert!(
            starts.iter().any(|&s| flat(s, p) < 1.0),
            "seat {slot} on a deathmatch start ({p:?})"
        );
        spawns.push(p);
    }
    assert!(flat(spawns[0], spawns[1]) > 2000.0, "opposite corners");
    // Two players: one rocket launcher, at the drop nearest the zone centre.
    assert_eq!(items_of(w, "RocketLauncher").len(), 1);
    // Everyone has a shotgun close to their start.
    let shotguns = items_of(w, "Shotgun");
    for (slot, &p) in spawns.iter().enumerate() {
        let near = shotguns
            .iter()
            .map(|&g| flat(g, p))
            .fold(f64::MAX, f64::min);
        assert!(near < 1100.0, "seat {slot}: nearest shotgun {near}");
    }
    for (gun, ammo) in [
        ("Shotgun", "Shell"),
        ("Chaingun", "Clip"),
        ("RocketLauncher", "RocketAmmo"),
    ] {
        let ammo = items_of(w, ammo);
        for g in items_of(w, gun) {
            assert!(
                ammo.iter().any(|&a| flat(a, g) <= 32.0),
                "{gun} at {g:?} has ammo beside it"
            );
        }
    }
}

#[test]
fn the_time_limit_ends_the_match_on_every_seat_alike() {
    // Vanilla: the sim alone ends the match when time runs out.
    let mut vanilla = start_timed(arena(), 2, false, 10 * 35);
    let mut msgs = Vec::new();
    run(
        &mut vanilla,
        10 * 35 - 1,
        &[TicCmd::default(); 2],
        &mut msgs,
    );
    assert_eq!(vanilla.world().unwrap().match_over, None);
    assert_eq!(vanilla.world().unwrap().match_tics_left, 1);
    run(&mut vanilla, 1, &[TicCmd::default(); 2], &mut msgs);
    assert!(matches!(
        vanilla.world().unwrap().match_over,
        Some(MatchOver {
            winner: None,
            time_up: true,
            ..
        })
    ));

    // Royale: the mod calls it one tic early; equal survivors = no winner.
    let mut game = start_timed(arena(), 2, true, 20 * 35);
    run(&mut game, 20 * 35, &[TicCmd::default(); 2], &mut msgs);
    let w = game.world().unwrap();
    assert!(msgs.iter().any(|m| m == "TIME!"), "{msgs:?}");
    assert!(matches!(w.match_over, Some(MatchOver { winner: None, .. })));
    assert!(w.standings().iter().all(|s| s.alive));
}

#[test]
fn a_solo_death_ends_practice() {
    let mut game = start(arena(), 1, true);
    let mut msgs = Vec::new();
    run(&mut game, 2, &[TicCmd::default()], &mut msgs);
    let w = game.world_mut().unwrap();
    let p0 = body(w, 0);
    assert!(room_sim::damage_mobj(w, p0, None, None, 500) > 0);
    run(&mut game, 4 * 35, &[TicCmd::default()], &mut msgs);
    assert!(msgs.iter().any(|m| m == "Practice over"), "{msgs:?}");
    let w = game.world().unwrap();
    assert!(matches!(w.match_over, Some(MatchOver { winner: None, .. })));
    assert!(!w.standings()[0].alive);
}

#[test]
fn the_zone_closes_and_hurts_players_outside() {
    let mut game = start(arena(), 2, true);
    let mut msgs = Vec::new();
    // Grace 45 s, then phases of 25 s hold / 20 s shrink.
    run(
        &mut game,
        (45 + 25) * 35,
        &[TicCmd::default(); 2],
        &mut msgs,
    );
    let w = game.world().unwrap();
    assert!(
        (0..2).all(|s| w.actors.get(body(w, s)).unwrap().health == 100),
        "nobody hurt before the zone moves"
    );
    run(&mut game, 65 * 35, &[TicCmd::default(); 2], &mut msgs);
    assert!(
        msgs.iter().any(|m| m == "Zone phase 1 - it is closing in!"),
        "{msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.starts_with("Outside the zone! Run ")),
        "{msgs:?}"
    );
    // Neither player moves: the one left outside is worn down and the other wins.
    assert!(
        msgs.iter()
            .any(|m| m.ends_with("was swallowed by the zone - 1 left")),
        "{msgs:?}"
    );
    assert!(msgs.iter().any(|m| m.ends_with("WINS THE BATTLE ROYALE!")));
}

#[test]
fn royale_is_deterministic() {
    let mut a = start(arena(), 2, true);
    let mut b = start(arena(), 2, true);
    let mut msgs = Vec::new();
    run(&mut a, 3000, &[TicCmd::default(); 2], &mut msgs);
    run(&mut b, 3000, &[TicCmd::default(); 2], &mut msgs);
    assert_eq!(a.world().unwrap().checksum(), b.world().unwrap().checksum());
}

#[test]
fn royale_on_doom1_e1m1() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    if !p.is_file() {
        eprintln!("skip: testdata/doom/doom1.wad missing");
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&p).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let vanilla = start(map.clone(), 2, false);
    assert!(alive_monsters(vanilla.world().unwrap()) > 0);
    let mut game = start(map, 2, true);
    let mut msgs = Vec::new();
    run(&mut game, 70, &[TicCmd::default(); 2], &mut msgs);
    let w = game.world().unwrap();
    assert_eq!(alive_monsters(w), 0, "E1M1 without enemies");
    assert!(loot_count(w) > 5, "loot on E1M1");
    for slot in 0..2 {
        assert!(!owns(w, slot, "Pistol"));
        assert_eq!(
            w.actors.get(body(w, slot)).unwrap().health,
            100,
            "seat {slot} alive"
        );
    }
    let d =
        (w.actors.get(body(w, 0)).unwrap().pos - w.actors.get(body(w, 1)).unwrap().pos).length();
    assert!(d > 256.0, "players apart on E1M1 ({d})");
    let dm: Vec<DVec3> = w
        .map
        .things
        .iter()
        .filter(|t| t.type_id == 11)
        .map(|t| DVec3::new(f64::from(t.x), f64::from(t.y), 0.0))
        .collect();
    assert!(!dm.is_empty(), "E1M1 has deathmatch starts");
    let shotguns = items_of(w, "Shotgun");
    for slot in 0..2 {
        let p = w.actors.get(body(w, slot)).unwrap().pos;
        assert!(
            dm.iter().any(|&s| flat(s, p) < 1.0),
            "seat {slot} on a DM start"
        );
        let near = shotguns
            .iter()
            .map(|&g| flat(g, p))
            .fold(f64::MAX, f64::min);
        eprintln!("seat {slot} at {p:?}: nearest shotgun {near:.0}");
        assert!(near < 1500.0, "seat {slot}: shotgun within reach ({near})");
    }
    let rockets = items_of(w, "RocketLauncher").len();
    eprintln!(
        "E1M1 loot: {} drops, {} shotguns, {} chainguns, {rockets} rocket launchers",
        loot_count(w),
        shotguns.len(),
        items_of(w, "Chaingun").len()
    );
    assert_eq!(
        rockets, 1,
        "the map's own launcher counts: two players share one"
    );
}
