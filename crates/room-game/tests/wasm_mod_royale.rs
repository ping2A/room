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
use room_sim::mod_hud::{WidgetKey, kind, layer};
use room_sim::script_mods::ModSource;
use room_sim::{
    ActorId, Buttons, MatchOver, MessageTo, PlayerState, TicCmd, World, give_inventory,
};
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
/// Pre-game countdown + deploy before loot / PvP (`BR1`).
fn past_pregame_tics() -> u32 {
    (10 + 3) * 35
}

fn past_pregame(game: &mut Headless, cmds: &[TicCmd], msgs: &mut Vec<String>) {
    run(game, past_pregame_tics() + 35, cmds, msgs);
}

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
    start_timed(map, players, with_mod, 0, false, 1)
}

fn start_timed(
    map: MapGeometry,
    players: u8,
    with_mod: bool,
    limit_tics: u32,
    fill_bots: bool,
    bot_first_seat: u8,
) -> Headless {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(4);
    let mut game = Headless::new(GameOptions {
        skill,
        coop_players: players,
        time_limit_tics: limit_tics,
        fill_bots,
        bot_first_seat,
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

fn spectate_seat(w: &World, victim: usize) -> Option<usize> {
    let cam = w.players[victim].as_ref()?.camera?;
    (0..8).find(|&s| w.players[s].as_ref().and_then(|p| p.mo) == Some(cam))
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

fn tele_flat(w: &mut World, slot: usize, x: f64, y: f64) {
    let mo = body(w, slot);
    let z = w.actors.get(mo).unwrap().pos.z();
    w.actors.get_mut(mo).unwrap().pos = DVec3::new(x, y, z);
}

fn flat(a: DVec3, b: DVec3) -> f64 {
    let (dx, dy) = (a.x() - b.x(), a.y() - b.y());
    (dx * dx + dy * dy).sqrt()
}

fn owns(w: &World, slot: usize, class: &str) -> bool {
    let cls = w.classes.find_name(class).unwrap();
    w.inventory.count(body(w, slot), cls) > 0
}

fn royale_widget<'a>(w: &'a World, seat: u8, id: u16) -> Option<&'a room_sim::mod_hud::HudWidget> {
    let key = WidgetKey {
        module: 0,
        seat1: seat + 1,
        id,
    };
    w.mod_hud.iter().find(|(k, _)| **k == key).map(|(_, w)| w)
}

fn hud_text_has(w: &World, seat: u8, id: u16, needle: &str) -> bool {
    royale_widget(w, seat, id)
        .is_some_and(|wid| wid.kind == kind::TEXT && wid.text.contains(needle))
}

#[test]
fn players_are_placed_apart_with_no_drop_choice() {
    let mut game = start(arena_with(true), 2, true);
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 2];
    run(&mut game, 10 * 35 + 5, &idle, &mut msgs);
    assert!(
        msgs.iter().any(|m| m == "DEPLOY"),
        "deploy right after pregame: {msgs:?}"
    );
    assert!(!msgs.iter().any(|m| m.contains("DROP")), "{msgs:?}");
    let w = game.world().unwrap();
    assert!(
        !w.mod_hud.iter().any(|(_, wid)| wid.layer == layer::MODAL),
        "no pick screen"
    );
    let (a, b) = (
        w.actors.get(body(w, 0)).unwrap().pos,
        w.actors.get(body(w, 1)).unwrap().pos,
    );
    assert!(flat(a, b) > 1000.0, "spread out: {a:?} {b:?}");
    run(&mut game, 3 * 35 + 10, &idle, &mut msgs);
    let w = game.world().unwrap();
    let markers = w
        .mod_hud
        .iter()
        .filter(|(k, wid)| k.seat1 == 0 && wid.kind == kind::MARKER)
        .count();
    let guns: usize = ["Shotgun", "Chaingun", "RocketLauncher", "Chainsaw"]
        .iter()
        .map(|g| items_of(w, g).len())
        .sum();
    assert_eq!(markers, guns, "one minimap marker per gun on the ground");
}

#[test]
fn mod_hud_shows_alive_and_zone_after_pregame() {
    let mut game = start(arena(), 2, true);
    let cmds = [TicCmd::default(); 2];
    past_pregame(&mut game, &cmds, &mut Vec::new());
    run(&mut game, 10, &cmds, &mut Vec::new());
    let w = game.world().unwrap();
    assert!(
        hud_text_has(w, 0, 1, "ALIVE"),
        "seat0 alive counter: {:?}",
        royale_widget(w, 0, 1).map(|x| &x.text)
    );
    assert!(
        hud_text_has(w, 0, 4, "KILLS"),
        "seat0 kills: {:?}",
        royale_widget(w, 0, 4).map(|x| &x.text)
    );
    assert!(
        hud_text_has(w, 0, 2, "ZONE IN"),
        "zone timer: {:?}",
        royale_widget(w, 0, 2).map(|x| &x.text)
    );
    assert!(
        hud_text_has(w, 1, 1, "ALIVE"),
        "seat1 alive counter missing"
    );
}

#[test]
fn mod_hud_lockstep_checksum_with_widgets() {
    let mut a = start(arena(), 2, true);
    let mut b = start(arena(), 2, true);
    let cmds = [TicCmd::default(); 2];
    past_pregame(&mut a, &cmds, &mut Vec::new());
    past_pregame(&mut b, &cmds, &mut Vec::new());
    run(&mut a, 200, &cmds, &mut Vec::new());
    run(&mut b, 200, &cmds, &mut Vec::new());
    let wa = a.world().unwrap();
    let wb = b.world().unwrap();
    assert!(hud_text_has(wa, 0, 1, "ALIVE"));
    assert!(hud_text_has(wb, 0, 2, "ZONE"));
    assert_eq!(wa.checksum(), wb.checksum());
}

#[test]
fn royale_clears_monsters_strips_guns_and_scatters_players() {
    let mut game = start(arena(), 2, true);
    let before = loot_count(game.world().unwrap());
    let mut msgs = Vec::new();
    let cmds = [TicCmd::default(); 2];
    run(&mut game, 40, &cmds, &mut msgs);
    let w = game.world().unwrap();
    assert!(
        msgs.iter()
            .any(|m| m.starts_with("BATTLE ROYALE - 2 players")),
        "{msgs:?}"
    );
    assert!(msgs.iter().any(|m| m == "PREGAME"), "{msgs:?}");
    assert_eq!(alive_monsters(w), 0, "no enemies");
    assert!(!w.deathmatch, "pregame: no PvP yet");
    assert!(w.no_player_respawn);
    let fist = w.classes.find_name("Fist");
    for slot in 0..2 {
        assert!(!owns(w, slot, "Pistol"), "seat {slot} has no pistol");
        assert!(!owns(w, slot, "Clip"), "seat {slot} has no bullets");
        assert!(owns(w, slot, "Fist"));
        assert_eq!(w.players[slot].as_ref().unwrap().ready_weapon, fist);
    }
    assert_eq!(loot_count(w), 0, "no loot before the drop");
    past_pregame(&mut game, &cmds, &mut msgs);
    let w = game.world().unwrap();
    assert!(w.deathmatch);
    assert!(loot_count(w) > before, "loot placed on monster spots");
    let (a, b) = (
        w.actors.get(body(w, 0)).unwrap().pos,
        w.actors.get(body(w, 1)).unwrap().pos,
    );
    let d = (a - b).length();
    assert!(d > 800.0, "players scattered to monster spots ({d})");
}

#[test]
fn pregame_ignores_damage() {
    let mut game = start(arena(), 2, true);
    let mut msgs = Vec::new();
    run(&mut game, 100, &[TicCmd::default(); 2], &mut msgs);
    let w = game.world_mut().unwrap();
    assert!(!w.deathmatch);
    let (p0, p1) = (body(w, 0), body(w, 1));
    assert_eq!(room_sim::damage_mobj(w, p1, Some(p0), Some(p0), 50), 0);
    assert_eq!(w.actors.get(p1).unwrap().health, 100);
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
    let cmds = [TicCmd::default(); 2];
    past_pregame(&mut game, &cmds, &mut msgs);
    let mut cmds = [TicCmd::default(); 2];
    {
        let w = game.world_mut().unwrap();
        let (p0, p1) = (body(w, 0), body(w, 1));
        w.players[1].as_mut().unwrap().team = 1;
        assert!(room_sim::damage_mobj(w, p1, Some(p0), Some(p0), 10) > 0);
        assert!(room_sim::damage_mobj(w, p1, Some(p0), Some(p0), 500) > 0);
    }
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
        msgs.iter().any(|m| m == "Player 1 WINS THE BATTLE ROYALE!"),
        "{msgs:?}"
    );
    run(&mut game, 8 * 35, &cmds, &mut msgs);
    let w = game.world().unwrap();
    assert!(matches!(
        w.match_over,
        Some(MatchOver {
            winner: Some(0),
            time_up: false,
            ..
        })
    ));
    assert_eq!(w.match_standings.len(), 2);
    let p0 = w
        .match_standings
        .iter()
        .find(|r| r.seat == 0)
        .expect("seat 0");
    assert_eq!(p0.kills, 1);
    assert!(p0.damage > 0);
    assert!(msgs.iter().any(|m| m == "LAST ONE STANDING"), "{msgs:?}");
    assert_eq!(game.state, GameState::Level, "no next map: the room ends");
    let table = w.standings();
    assert_eq!(table.len(), 2);
    assert_eq!((table[0].slot, table[0].place, table[0].frags), (0, 1, 1));
    assert!(!table[1].alive);
    assert!(!msgs.iter().any(|m| m.contains("disabled")), "{msgs:?}");
}

#[test]
fn loot_is_limited_and_every_gun_has_ammo() {
    let mut game = start(arena_with(true), 2, true);
    let mut msgs = Vec::new();
    past_pregame(&mut game, &[TicCmd::default(); 2], &mut msgs);
    let w = game.world().unwrap();
    let mut spawns = Vec::new();
    for slot in 0..2 {
        let p = w.actors.get(body(w, slot)).unwrap().pos;
        spawns.push(p);
    }
    assert!(
        flat(spawns[0], spawns[1]) > 200.0,
        "players apart after drop deploy"
    );
    // Two players: a fixed, limited set of guns (picked up or on the map).
    for (gun, want) in [
        ("RocketLauncher", 1),
        ("Chaingun", 1),
        ("Shotgun", 2),
        ("Chainsaw", 1),
    ] {
        let owned = (0..2).filter(|&s| owns(w, s, gun)).count();
        assert_eq!(items_of(w, gun).len() + owned, want, "{gun}");
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
    let mut vanilla = start_timed(arena(), 2, false, 10 * 35, false, 1);
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
    let mut game = start_timed(arena(), 2, true, 20 * 35, false, 1);
    run(
        &mut game,
        20 * 35 + 8 * 35,
        &[TicCmd::default(); 2],
        &mut msgs,
    );
    let w = game.world().unwrap();
    assert!(msgs.iter().any(|m| m == "TIME!"), "{msgs:?}");
    assert!(matches!(w.match_over, Some(MatchOver { winner: None, .. })));
    assert!(w.standings().iter().all(|s| s.alive));
    assert_eq!(w.match_standings.len(), 2);
}

#[test]
fn a_solo_death_ends_practice() {
    let mut game = start(arena(), 1, true);
    let mut msgs = Vec::new();
    let cmds = [TicCmd::default()];
    past_pregame(&mut game, &cmds, &mut msgs);
    let w = game.world_mut().unwrap();
    let p0 = body(w, 0);
    assert!(room_sim::damage_mobj(w, p0, None, None, 500) > 0);
    run(&mut game, 8 * 35, &cmds, &mut msgs);
    assert!(msgs.iter().any(|m| m == "Practice over"), "{msgs:?}");
    let w = game.world().unwrap();
    assert!(matches!(w.match_over, Some(MatchOver { winner: None, .. })));
    assert!(!w.standings()[0].alive);
}

#[test]
fn the_zone_closes_and_hurts_players_outside() {
    let mut game = start(arena(), 2, true);
    let mut msgs = Vec::new();
    let cmds = [TicCmd::default(); 2];
    past_pregame(&mut game, &cmds, &mut msgs);
    {
        let w = game.world_mut().unwrap();
        tele_flat(w, 1, 64.0, 64.0);
    }
    // Grace 60 s, then phase 1 hold 60 s before the first shrink.
    run(&mut game, (60 + 60) * 35, &cmds, &mut msgs);
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
    run(&mut game, 8 * 35, &cmds, &mut msgs);
    assert!(game.world().unwrap().match_over.is_some());
}

#[test]
fn no_supply_drops_or_new_loot_during_the_match() {
    let mut game = start(arena(), 2, true);
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 2];
    past_pregame(&mut game, &idle, &mut msgs);
    let placed = loot_count(game.world().unwrap());
    run(&mut game, 90 * 35, &idle, &mut msgs);
    let w = game.world().unwrap();
    assert!(
        !msgs
            .iter()
            .any(|m| m.to_ascii_lowercase().contains("supply")),
        "{msgs:?}"
    );
    assert!(w.classes.find_name("SupplyCrate").is_none());
    assert!(loot_count(w) <= placed, "nothing added after the start");
}

#[test]
fn a_dead_player_drops_nothing() {
    let mut game = start(arena(), 2, true);
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 2];
    past_pregame(&mut game, &idle, &mut msgs);
    let before = {
        let w = game.world_mut().unwrap();
        let shotgun = w.classes.find_name("Shotgun").unwrap();
        give_inventory(w, body(w, 0), shotgun, 1);
        loot_count(w)
    };
    {
        let w = game.world_mut().unwrap();
        let (p0, p1) = (body(w, 0), body(w, 1));
        assert!(room_sim::damage_mobj(w, p0, Some(p1), Some(p1), 500) > 0);
    }
    run(&mut game, 5, &idle, &mut msgs);
    assert_eq!(loot_count(game.world().unwrap()), before, "no death drop");
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
    past_pregame(&mut game, &[TicCmd::default(); 2], &mut msgs);
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
    let spawns: Vec<_> = (0..2)
        .map(|slot| w.actors.get(body(w, slot)).unwrap().pos)
        .collect();
    assert!(
        flat(spawns[0], spawns[1]) > 256.0,
        "players apart on E1M1 after drop"
    );
    for slot in 0..2 {
        let p = spawns[slot];
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

fn three_seat_game() -> Headless {
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .player_start(128.0, 128.0, 0)
        .thing(2, 512.0, 128.0, 0)
        .thing(3, 896.0, 128.0, 0);
    for (x, y) in DM_STARTS {
        map = map.thing(11, x, y, 0);
    }
    let mut map = map.build_udmf("MAP01").unwrap();
    room_nodebuild::apply_nodes(&mut map);
    start(map, 3, true)
}

/// Seat 0 kills seat 2 (after some damage from seat 1).
fn kill_seat_two(game: &mut Headless) {
    let idle = [TicCmd::default(); 3];
    game.tick(&idle);
    let w = game.world_mut().unwrap();
    let (p0, p1, p2) = (body(w, 0), body(w, 1), body(w, 2));
    let _ = room_sim::damage_mobj(w, p2, Some(p1), Some(p1), 30);
    let _ = room_sim::damage_mobj(w, p2, Some(p0), Some(p0), 500);
}

#[test]
fn death_shows_stats_then_spectates_the_killer() {
    let mut game = three_seat_game();
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 3];
    past_pregame(&mut game, &idle, &mut msgs);
    kill_seat_two(&mut game);
    run(&mut game, 10, &idle, &mut msgs);
    let w = game.world().unwrap();
    assert!(hud_text_has(w, 2, 64, "ELIMINATED"), "stats title");
    assert!(hud_text_has(w, 2, 61, "#3 OF 3"), "place");
    assert!(hud_text_has(w, 2, 61, "KILLED BY Player 1"), "killer");
    assert!(
        hud_text_has(w, 2, 66, "KILLS") && hud_text_has(w, 2, 69, "0"),
        "own stats"
    );
    assert!(hud_text_has(w, 2, 65, "FIRE TO SKIP"));
    assert_eq!(spectate_seat(w, 2), None, "stats first, no spectating yet");
    run(&mut game, 5 * 35, &idle, &mut msgs);
    let w = game.world().unwrap();
    assert_eq!(spectate_seat(w, 2), Some(0), "then the killer is watched");
    assert!(royale_widget(w, 2, 64).is_none(), "stats screen gone");
    assert!(hud_text_has(w, 2, 62, "SPECTATING Player 1"), "banner");
    assert!(hud_text_has(w, 2, 63, "FIRE: NEXT PLAYER"));
    assert_eq!(
        royale_widget(w, 2, 30).map(|m| m.font),
        Some(1),
        "minimap follows the watched player"
    );
    let mut fire = idle;
    fire[2].buttons = Buttons(Buttons::ATTACK);
    run(&mut game, 2, &fire, &mut msgs);
    let w = game.world().unwrap();
    assert_eq!(spectate_seat(w, 2), Some(1), "FIRE: next player");
    assert!(hud_text_has(w, 2, 62, "SPECTATING Player 2"));
}

#[test]
fn fire_skips_the_stats_screen() {
    let mut game = three_seat_game();
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 3];
    past_pregame(&mut game, &idle, &mut msgs);
    kill_seat_two(&mut game);
    let mut fire = idle;
    fire[2].buttons = Buttons(Buttons::ATTACK);
    // Held from the moment of death: ignored until released.
    run(&mut game, 50, &fire, &mut msgs);
    assert_eq!(spectate_seat(game.world().unwrap(), 2), None);
    run(&mut game, 2, &idle, &mut msgs);
    run(&mut game, 2, &fire, &mut msgs);
    let w = game.world().unwrap();
    assert_eq!(
        spectate_seat(w, 2),
        Some(0),
        "skipped straight to spectating"
    );
    assert!(royale_widget(w, 2, 64).is_none());
}

#[test]
fn four_players_are_free_for_all() {
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .player_start(128.0, 128.0, 0)
        .thing(2, 128.0, 256.0, 0)
        .thing(3, 256.0, 128.0, 0)
        .thing(4, 256.0, 256.0, 0);
    for (x, y) in DM_STARTS {
        map = map.thing(11, x, y, 0);
    }
    let mut map = map.build_udmf("MAP01").unwrap();
    room_nodebuild::apply_nodes(&mut map);
    let mut game = start(map, 4, true);
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 4];
    past_pregame(&mut game, &idle, &mut msgs);
    let w = game.world_mut().unwrap();
    let (p0, p1) = (body(w, 0), body(w, 1));
    assert!(
        room_sim::damage_mobj(w, p1, Some(p0), Some(p0), 50) > 0,
        "no squads: seats 0 and 1 hurt each other"
    );
}

#[test]
fn royale_wad_packs_track_b_lumps() {
    let vfs = VfsBuilder::new()
        .mount_memory("royale.wad", Bytes::from_static(ROYALE_WAD))
        .unwrap()
        .build()
        .unwrap();
    for lump in ["SNDINFO"] {
        assert!(
            vfs.find_any_ns(lump).is_some(),
            "missing Track B lump {lump}"
        );
    }
    assert_eq!(
        Headless::new(GameOptions::default())
            .set_script_mods(royale_mods())
            .unwrap(),
        ["royale"]
    );
}

#[test]
fn royale01_in_royale_wad_loads_with_dm_starts_and_nodes() {
    let vfs = VfsBuilder::new()
        .mount_memory("royale.wad", Bytes::from_static(ROYALE_WAD))
        .unwrap()
        .build()
        .unwrap();
    assert!(
        vfs.find_any_ns("ROYALE01").is_some(),
        "royale.wad should embed map ROYALE01 (maps/ROYALE01.wad at pack time)"
    );
    let mut map = load_map(&vfs, "ROYALE01").expect("load ROYALE01");
    let dm = map.things.iter().filter(|t| t.type_id == 11).count();
    assert!(dm >= 16, "need >=16 DM starts, got {dm}");
    ensure_nodes(&mut map);
    assert!(map.game_nodes.is_some(), "nodes must build");
}

#[test]
fn eight_seat_all_bots_finish_match_deterministically() {
    let limit = 35 * 90;
    let mut a = start_timed(arena_with(true), 8, true, limit, true, 0);
    let mut b = start_timed(arena_with(true), 8, true, limit, true, 0);
    let cmds = [TicCmd::default(); 8];
    let mut msgs = Vec::new();
    run(&mut a, limit + 35 * 30, &cmds, &mut msgs);
    run(&mut b, limit + 35 * 30, &cmds, &mut Vec::new());
    let wa = a.world().unwrap();
    let wb = b.world().unwrap();
    for seat in 0..8 {
        assert!(
            wa.players[seat].as_ref().is_some_and(|p| p.is_bot),
            "seat {seat} should be bot"
        );
    }
    assert!(
        wa.match_over.is_some(),
        "match should end (elimination or time limit)"
    );
    assert_eq!(wa.checksum(), wb.checksum());
}

/// Play a whole 8-bot match on `map` (no time limit: the zone ends it) and
/// check every stage. Returns the final checksum.
fn full_bot_match(map: MapGeometry) -> u64 {
    let mut game = start_timed(map, 8, true, 0, true, 0);
    let idle = [TicCmd::default(); 8];
    let mut msgs = Vec::new();
    // Right after placement (bots start walking at once).
    run(&mut game, 10 * 35 + 2, &idle, &mut msgs);
    {
        let w = game.world().unwrap();
        assert!(loot_count(w) >= 15, "loot on the map: {}", loot_count(w));
        let guns: usize = ["Shotgun", "Chaingun", "RocketLauncher", "Chainsaw"]
            .iter()
            .map(|g| items_of(w, g).len())
            .sum();
        assert!(guns >= 8, "guns placed: {guns}");
        let pos: Vec<DVec3> = (0..8)
            .map(|s| w.actors.get(body(w, s)).unwrap().pos)
            .collect();
        let mut closest = f64::MAX;
        for a in 0..8 {
            for b in a + 1..8 {
                closest = closest.min(flat(pos[a], pos[b]));
            }
        }
        assert!(
            closest > 128.0,
            "players spread out (closest pair {closest})"
        );
    }
    // Grace + every zone phase is under 8 minutes; give it 10.
    let mut tics = 0;
    while game.world().unwrap().match_over.is_none() && tics < 10 * 60 * 35 {
        run(&mut game, 35, &idle, &mut msgs);
        tics += 35;
    }
    let w = game.world().unwrap();
    assert!(w.match_over.is_some(), "match ended within 10 minutes");
    let outs = msgs
        .iter()
        .filter(|m| {
            m.contains(" left")
                && (m.contains("fragged") || m.contains("zone") || m.contains("is out"))
        })
        .count();
    assert!(outs >= 6, "eliminations announced: {outs}");
    assert_eq!(w.match_standings.len(), 8, "standings for every seat");
    assert!(
        msgs.iter()
            .any(|m| m.contains("WINS THE BATTLE ROYALE") || m.contains("draw")),
        "result announced"
    );
    let errors: Vec<&String> = msgs
        .iter()
        .filter(|m| {
            let l = m.to_ascii_lowercase();
            l.contains("trap") || l.contains("panic") || l.contains("error")
        })
        .collect();
    assert!(errors.is_empty(), "script errors: {errors:?}");
    w.checksum()
}

#[test]
fn eight_bots_play_a_whole_match_on_royale01() {
    let vfs = VfsBuilder::new()
        .mount_memory("royale.wad", Bytes::from_static(ROYALE_WAD))
        .unwrap()
        .build()
        .unwrap();
    let mut map = load_map(&vfs, "ROYALE01").unwrap();
    ensure_nodes(&mut map);
    let a = full_bot_match(map.clone());
    let b = full_bot_match(map);
    assert_eq!(a, b, "deterministic");
}

/// Same on a real IWAD map: `ROYALE_PREVIEW_IWAD=/path/doom2.wad`
/// (`ROYALE_PREVIEW_MAP`, default MAP01).
#[test]
#[ignore = "needs ROYALE_PREVIEW_IWAD"]
fn eight_bots_play_a_whole_match_on_an_iwad_map() {
    let iwad = std::env::var("ROYALE_PREVIEW_IWAD").expect("ROYALE_PREVIEW_IWAD");
    let name = std::env::var("ROYALE_PREVIEW_MAP").unwrap_or_else(|_| "MAP01".into());
    let vfs = VfsBuilder::new()
        .mount_path(&iwad)
        .unwrap()
        .build()
        .unwrap();
    let mut map = load_map(&vfs, &name).unwrap();
    ensure_nodes(&mut map);
    let _ = full_bot_match(map);
}

/// Render the in-game minimap to images (manual check).
/// `ROYALE_PREVIEW_IWAD=/path/doom2.wad ROYALE_PREVIEW_MAP=MAP01`.
#[test]
#[ignore = "manual preview; needs ROYALE_PREVIEW_IWAD"]
fn render_overlay_preview() {
    use room_ui::{
        CanvasSink, DrawCmd, ModHudMapCtx, PatchCache, Rect, Rgba, UiDrawList, blit_draw_list_full,
        draw_mod_hud, player_map_poses,
    };
    struct Img {
        w: usize,
        h: usize,
        px: Vec<[f32; 3]>,
    }
    impl CanvasSink for Img {
        fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) {
            let (x0, y0) = (x.max(0.0) as usize, y.max(0.0) as usize);
            let x1 = ((x + w).ceil().max(0.0) as usize).min(self.w);
            let y1 = ((y + h).ceil().max(0.0) as usize).min(self.h);
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let p = &mut self.px[yy * self.w + xx];
                    for k in 0..3 {
                        p[k] = p[k] * (1.0 - c[3]) + c[k] * c[3];
                    }
                }
            }
        }
        fn text(&mut self, _: f32, _: f32, _: &str, _: [f32; 4]) {}
    }
    let iwad = std::env::var("ROYALE_PREVIEW_IWAD").expect("ROYALE_PREVIEW_IWAD");
    let map_name = std::env::var("ROYALE_PREVIEW_MAP").unwrap_or_else(|_| "MAP01".into());
    let vfs = VfsBuilder::new()
        .mount_path(&iwad)
        .unwrap()
        .mount_memory("royale.wad", Bytes::from_static(ROYALE_WAD))
        .unwrap()
        .build()
        .unwrap();
    let mut map = load_map(&vfs, &map_name).unwrap();
    ensure_nodes(&mut map);
    let mut game = start(map, 4, true);
    let mut patches = PatchCache::from_vfs(&vfs);
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 4];
    let shot = |game: &Headless, name: &str, patches: &mut PatchCache| {
        let w = game.world().unwrap();
        let mut list = UiDrawList::new();
        list.fill(
            Rect::new(0.0, 0.0, 320.0, 200.0),
            Rgba::new(0.22, 0.2, 0.18, 1.0),
        );
        let poses = player_map_poses(w);
        let ctx = ModHudMapCtx {
            map: Some(&w.map),
            zones: &w.mod_zones,
            player_pose: &poses,
            hud: &w.mod_hud,
            allmap: false,
            ticfrac_milli: 1000,
        };
        draw_mod_hud(
            &w.mod_hud,
            0,
            w.level.time as u32,
            0.0,
            &mut list,
            Some(&ctx),
        );
        for c in &list.cmds {
            if let DrawCmd::Patch { name, .. } = c {
                let _ = patches.ensure(&vfs, name);
            }
        }
        let (iw, ih) = (1280usize, 800usize);
        let mut img = Img {
            w: iw,
            h: ih,
            px: vec![[0.0; 3]; iw * ih],
        };
        blit_draw_list_full(&list, iw as f32, ih as f32, None, Some(patches), &mut img);
        let mut bytes = Vec::with_capacity(iw * ih * 3);
        for p in &img.px {
            for k in 0..3 {
                bytes.push((p[k].clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
        let path = format!("/tmp/royale-{name}.ppm");
        let mut out = format!("P6 {iw} {ih} 255\n").into_bytes();
        out.extend_from_slice(&bytes);
        std::fs::write(&path, out).unwrap();
        eprintln!("wrote {path}");
    };
    run(&mut game, 10 * 35 + 3 * 35 + 70, &idle, &mut msgs);
    shot(&game, "minimap", &mut patches);
    {
        let w = game.world_mut().unwrap();
        let (p0, p1) = (body(w, 0), body(w, 1));
        let _ = room_sim::damage_mobj(w, p0, Some(p1), Some(p1), 500);
    }
    run(&mut game, 20, &idle, &mut msgs);
    shot(&game, "dead", &mut patches);
    run(&mut game, 6 * 35, &idle, &mut msgs);
    shot(&game, "spectating", &mut patches);
    for victim in [1, 2] {
        let w = game.world_mut().unwrap();
        let (k, v) = (body(w, 3), body(w, victim));
        let _ = room_sim::damage_mobj(w, v, Some(k), Some(k), 500);
        run(&mut game, 5, &idle, &mut msgs);
    }
    run(&mut game, 40, &idle, &mut msgs);
    shot(&game, "victory", &mut patches);
}

/// Share of bot tics spent in the run animation while barely moving
/// (running on the spot), over 30 s of an 8-bot match.
fn running_in_place_share(map: MapGeometry) -> f64 {
    let mut game = start_timed(map, 8, true, 0, true, 0);
    let idle = [TicCmd::default(); 8];
    let mut msgs = Vec::new();
    run(&mut game, 14 * 35, &idle, &mut msgs);
    let mut last: Vec<Option<DVec3>> = vec![None; 8];
    let (mut bad, mut total) = (0usize, 0usize);
    for _ in 0..30 * 35 {
        run(&mut game, 1, &idle, &mut msgs);
        let w = game.world().unwrap();
        for s in 0..8 {
            let Some(mo) = w.players[s].as_ref().and_then(|p| p.mo) else {
                continue;
            };
            let Some(a) = w.actors.get(mo) else { continue };
            if a.health <= 0 {
                last[s] = None;
                continue;
            }
            if let Some(prev) = last[s] {
                total += 1;
                let running = a.sprite == "PLAY" && (1..=3).contains(&a.frame);
                if running && flat(prev, a.pos) < 0.5 {
                    bad += 1;
                }
            }
            last[s] = Some(a.pos);
        }
    }
    bad as f64 / total.max(1) as f64
}

#[test]
fn bots_pick_up_guns_and_do_not_run_on_the_spot() {
    let mut game = start_timed(arena_with(true), 8, true, 0, true, 0);
    let idle = [TicCmd::default(); 8];
    let mut msgs = Vec::new();
    run(&mut game, 30 * 35, &idle, &mut msgs);
    let w = game.world().unwrap();
    let armed = (0..8)
        .filter(|&s| {
            ["Shotgun", "Chaingun", "RocketLauncher", "Chainsaw"]
                .iter()
                .any(|g| {
                    w.players[s].as_ref().and_then(|p| p.mo).is_some_and(|mo| {
                        w.classes
                            .find_name(g)
                            .is_some_and(|c| w.inventory.count(mo, c) > 0)
                    })
                })
        })
        .count();
    assert!(armed >= 2, "bots fetch guns ({armed} armed)");
    let share = running_in_place_share(arena_with(true));
    assert!(
        share < 0.10,
        "running on the spot {:.0}% of the time",
        share * 100.0
    );
}

#[test]
fn nobody_is_hurt_before_the_match_starts() {
    let mut game = start_timed(arena_with(true), 8, true, 0, true, 0);
    let idle = [TicCmd::default(); 8];
    let mut msgs = Vec::new();
    // Pregame (10 s) + deploy (3 s): everyone stays at full health.
    for _ in 0..13 * 35 - 2 {
        run(&mut game, 1, &idle, &mut msgs);
        let w = game.world().unwrap();
        for s in 0..8 {
            let hp = w.actors.get(body(w, s)).unwrap().health;
            assert_eq!(hp, 100, "seat {s} hurt at tic {}", w.level.time);
        }
    }
}

#[test]
fn nothing_spawns_in_a_closed_off_room() {
    // A big room, and a closet with no way in (monsters stand in it).
    let mut b = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .room("closet", rect(2304.0, 0.0, 2560.0, 256.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .thing(2, 192.0, 128.0, 0);
    for (x, y) in DM_STARTS {
        b = b.thing(11, x, y, 0);
    }
    for (x, y) in [(1000.0, 1000.0), (300.0, 1700.0), (1700.0, 300.0)] {
        b = b.thing(3004, x, y, 0);
    }
    for (x, y) in [
        (2368.0, 64.0),
        (2432.0, 128.0),
        (2496.0, 192.0),
        (2368.0, 192.0),
    ] {
        b = b.thing(3004, x, y, 0).thing(2001, x + 16.0, y, 0);
    }
    let mut map = b.build_udmf("MAP01").unwrap();
    room_nodebuild::apply_nodes(&mut map);
    let mut game = start(map, 2, true);
    let idle = [TicCmd::default(); 2];
    let mut msgs = Vec::new();
    run(&mut game, 10 * 35 + 2, &idle, &mut msgs);
    let w = game.world().unwrap();
    assert!(
        !room_sim::reach::point_in_main_area(w, 2432.0, 128.0),
        "closet is cut off"
    );
    assert!(room_sim::reach::point_in_main_area(w, 1000.0, 1000.0));
    let in_closet = |p: DVec3| p.x() > 2200.0;
    for g in LOOT {
        for p in items_of(w, g) {
            assert!(!in_closet(p), "{g} placed in the closet at {p:?}");
        }
    }
    for s in 0..2 {
        let p = w.actors.get(body(w, s)).unwrap().pos;
        assert!(!in_closet(p), "seat {s} placed in the closet");
    }
    assert!(loot_count(w) > 0, "loot still placed in the main room");
}

/// Running-on-the-spot share on a real map: `ROYALE_PREVIEW_IWAD`.
#[test]
#[ignore = "needs ROYALE_PREVIEW_IWAD"]
fn bots_move_well_on_an_iwad_map() {
    let iwad = std::env::var("ROYALE_PREVIEW_IWAD").expect("ROYALE_PREVIEW_IWAD");
    let name = std::env::var("ROYALE_PREVIEW_MAP").unwrap_or_else(|_| "MAP01".into());
    let vfs = VfsBuilder::new()
        .mount_path(&iwad)
        .unwrap()
        .build()
        .unwrap();
    let mut map = load_map(&vfs, &name).unwrap();
    ensure_nodes(&mut map);
    let share = running_in_place_share(map);
    eprintln!(
        "{name}: running on the spot {:.1}% of bot tics",
        share * 100.0
    );
    assert!(share < 0.15);
}

/// Where players land on every map of an IWAD: always in the main area
/// (never behind a wall that lowers only when a switch is pressed).
/// `ROYALE_PREVIEW_IWAD=/path/doom2.wad`.
#[test]
#[ignore = "needs ROYALE_PREVIEW_IWAD"]
fn spawn_report_on_every_map() {
    let iwad = std::env::var("ROYALE_PREVIEW_IWAD").expect("ROYALE_PREVIEW_IWAD");
    let vfs = VfsBuilder::new()
        .mount_path(&iwad)
        .unwrap()
        .build()
        .unwrap();
    for i in 1..=32 {
        let name = format!("MAP{i:02}");
        let Ok(mut map) = load_map(&vfs, &name) else {
            continue;
        };
        ensure_nodes(&mut map);
        let mut game = start_timed(map, 8, true, 0, true, 0);
        let idle = [TicCmd::default(); 8];
        let mut msgs = Vec::new();
        run(&mut game, 10 * 35 + 2, &idle, &mut msgs);
        let w = game.world().unwrap();
        let mut line = format!("{name}:");
        for s in 0..8 {
            let p = w.actors.get(body(w, s)).unwrap().pos;
            let sec = room_sim::sector_at_xy(w, p.x(), p.y()).unwrap_or(usize::MAX);
            let sky = w
                .map
                .sectors
                .get(sec)
                .is_some_and(|x| x.ceiling_pic.eq_ignore_ascii_case("F_SKY1"));
            let main = room_sim::reach::point_in_main_area(w, p.x(), p.y());
            line += &format!(
                " [{:.0},{:.0} s{sec}{}{}]",
                p.x(),
                p.y(),
                if sky { " SKY" } else { "" },
                if main { "" } else { " OUT" }
            );
        }
        assert!(!line.contains(" OUT"), "{line}");
        let area = room_sim::reach::main_area(w).iter().filter(|b| **b).count();
        eprintln!("{line}  area {area}/{}", w.map.sectors.len());
    }
}

/// Battle Royale on maps from `room-mapgen`: every deathmatch start lies in
/// the engine's main area, and 8 bots play a whole match to a winner.
#[test]
fn royale_on_generated_maps() {
    use room_mapgen::{Biomes, Theme};
    for (seed, theme, biomes) in [
        (11, Theme::Tech, Biomes::Mixed),
        (5, Theme::Hell, Biomes::Caves),
        (21, Theme::Ruins, Biomes::Wilderness),
    ] {
        let opts = room_mapgen::Options {
            seed,
            players: 8,
            theme,
            biomes,
            ..Default::default()
        };
        let g = room_mapgen::generate(&opts).unwrap();
        let mut map = g.map.to_geometry().unwrap();
        ensure_nodes(&mut map);
        {
            let game = start(map.clone(), 2, false);
            let w = game.world().unwrap();
            for t in g.map.things.iter().filter(|t| t.type_id == 11) {
                assert!(
                    room_sim::reach::point_in_main_area(w, f64::from(t.x), f64::from(t.y)),
                    "seed {seed}: DM start ({}, {}) outside the main area",
                    t.x,
                    t.y
                );
            }
        }
        let _ = full_bot_match(map);
    }
}
