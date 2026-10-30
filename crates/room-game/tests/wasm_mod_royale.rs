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
    ActorId, Buttons, MatchOver, MessageTo, PlayerState, TicCmd, World, count_inventory,
    give_inventory,
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
/// Pre-game countdown + drop select + deploy before loot / PvP (`BR1` / `BR3`).
fn past_pregame_tics() -> u32 {
    (10 + 15 + 3) * 35
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
fn drop_select_modal_map_and_confirm_lands_near_pick() {
    let mut game = start(arena_with(true), 2, true);
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 2];
    run(&mut game, 10 * 35 + 5, &idle, &mut msgs);
    assert!(msgs.iter().any(|m| m == "DROP SELECT"), "{msgs:?}");
    let w = game.world().unwrap();
    assert!(
        royale_widget(w, 0, 41)
            .is_some_and(|wid| { wid.kind == kind::MAP && wid.layer == layer::MODAL }),
        "seat0 modal map: {:?}",
        royale_widget(w, 0, 41)
    );
    let mut cmds = [TicCmd::default(); 2];
    cmds[0].forward_move = 127;
    cmds[1].forward_move = 64;
    for _ in 0..40 {
        game.tick(&cmds);
    }
    cmds[0].buttons = Buttons(Buttons::ATTACK);
    cmds[1].buttons = Buttons(Buttons::ATTACK);
    for _ in 0..8 {
        game.tick(&cmds);
        if let Some(w) = game.world_mut() {
            msgs.extend(w.drain_messages());
        }
    }
    assert!(msgs.iter().any(|m| m == "DEPLOY"), "{msgs:?}");
    run(&mut game, 3 * 35, &idle, &mut msgs);
    let w = game.world().unwrap();
    let pos = w.actors.get(body(w, 0)).unwrap().pos;
    let center = w.mod_zones.get(0).map(|z| (z.cx as f64, z.cy as f64));
    if let Some((cx, cy)) = center {
        let pick_y = cy + 127.0 / 127.0 * 10.0 * 40.0;
        let dx = pos.x() - cx;
        let dy = pos.y() - pick_y;
        let d = (dx * dx + dy * dy).sqrt();
        assert!(
            d <= 512.0,
            "seat0 should land within 512 of pick (d={d}, pos={pos:?}, pick≈({cx},{pick_y}))"
        );
    }
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
        hud_text_has(w, 0, 1, "KILLS"),
        "seat0 kills: {:?}",
        royale_widget(w, 0, 1).map(|x| &x.text)
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
    past_pregame(&mut game, &cmds, &mut msgs);
    let w = game.world().unwrap();
    assert!(w.deathmatch);
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
fn players_spawn_on_deathmatch_starts_and_every_gun_has_ammo() {
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
fn supply_drop_announces_lands_and_opens_with_use() {
    let mut game = start(arena(), 2, true);
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 2];
    past_pregame(&mut game, &idle, &mut msgs);
    run(&mut game, 45 * 35 + 5, &idle, &mut msgs);
    assert!(msgs.iter().any(|m| m == "SUPPLY DROP INBOUND"), "{msgs:?}");
    assert!(msgs.iter().any(|m| m == "Supply crate landed"), "{msgs:?}");
    let crates = items_of(game.world().unwrap(), "SupplyCrate");
    assert_eq!(crates.len(), 1, "one supply crate");
    let c = crates[0];
    {
        let w = game.world_mut().unwrap();
        tele_flat(w, 0, c.x(), c.y());
    }
    let mut cmds = [TicCmd::default(); 2];
    cmds[0].buttons = Buttons(Buttons::USE);
    for _ in 0..80 {
        game.tick(&cmds);
        if let Some(w) = game.world_mut() {
            msgs.extend(w.drain_messages());
        }
    }
    assert!(msgs.iter().any(|m| m == "SUPPLY OPENED"), "{msgs:?}");
    let w = game.world().unwrap();
    let mo = body(w, 0);
    let legendary = w.classes.find_name("LegendaryChaingun");
    let picked_legendary = legendary.is_some_and(|c| count_inventory(w, mo, c) > 0);
    assert!(
        picked_legendary
            || !items_of(w, "LegendaryChaingun").is_empty()
            || !items_of(w, "LegendaryGreenArmor").is_empty(),
        "legendary loot from supply (picked={picked_legendary})"
    );
}

#[test]
fn death_drops_victim_weapons_as_loot() {
    let mut game = start(arena(), 2, true);
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 2];
    past_pregame(&mut game, &idle, &mut msgs);
    let death_pos = {
        let w = game.world_mut().unwrap();
        let shotgun = w.classes.find_name("Shotgun").unwrap();
        give_inventory(w, body(w, 0), shotgun, 1);
        w.actors.get(body(w, 0)).unwrap().pos
    };
    {
        let w = game.world_mut().unwrap();
        let (p0, p1) = (body(w, 0), body(w, 1));
        assert!(room_sim::damage_mobj(w, p0, Some(p1), Some(p1), 500) > 0);
    }
    run(&mut game, 5, &idle, &mut msgs);
    let w = game.world().unwrap();
    let drops = items_of(w, "Shotgun");
    assert!(
        drops.iter().any(|&p| flat(p, death_pos) < 128.0),
        "shotgun dropped near death site: {drops:?} vs {death_pos:?}"
    );
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

#[test]
fn death_spectate_camera_follows_killer() {
    let mut game = start(arena_with(true), 2, true);
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 2];
    past_pregame(&mut game, &idle, &mut msgs);
    let (p0, p1) = {
        let w = game.world().unwrap();
        (body(w, 0), body(w, 1))
    };
    {
        let w = game.world_mut().unwrap();
        assert!(room_sim::damage_mobj(w, p1, Some(p0), Some(p0), 500) > 0);
    }
    run(&mut game, 4 * 35 + 15, &idle, &mut msgs);
    let w = game.world().unwrap();
    assert_eq!(
        w.players[1].as_ref().unwrap().camera,
        Some(p0),
        "dead seat spectates killer"
    );
}

#[test]
fn death_recap_and_spectate_cycle_three_seats() {
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
    let mut game = start(map, 3, true);
    let mut msgs = Vec::new();
    let idle = [TicCmd::default(); 3];
    past_pregame(&mut game, &idle, &mut msgs);
    for _ in 0..3 {
        game.tick(&idle);
        let w = game.world_mut().unwrap();
        let (p0, p1, p2) = (body(w, 0), body(w, 1), body(w, 2));
        let _ = room_sim::damage_mobj(w, p2, Some(p0), Some(p0), 20);
        let _ = room_sim::damage_mobj(w, p2, Some(p1), Some(p1), 35);
    }
    {
        game.tick(&idle);
        let w = game.world_mut().unwrap();
        let (p0, p2) = (body(w, 0), body(w, 2));
        let _ = room_sim::damage_mobj(w, p2, Some(p0), Some(p0), 500);
    }
    run(&mut game, 35, &idle, &mut msgs);
    let w = game.world().unwrap();
    assert!(
        hud_text_has(w, 2, 61, "Player 1") && hud_text_has(w, 2, 61, "Player 2"),
        "recap lists top damage sources"
    );
    run(&mut game, 3 * 35 + 5, &idle, &mut msgs);
    let w = game.world().unwrap();
    let first = spectate_seat(w, 2);
    assert!(
        first.is_some_and(|s| s != 2),
        "dead seat spectates an alive player"
    );
    let alive = (0..3)
        .filter(|&s| {
            w.players[s]
                .as_ref()
                .is_some_and(|p| p.player_state == PlayerState::Live)
        })
        .count();
    if alive >= 2 {
        let mut cycle = idle;
        cycle[2].buttons = Buttons(Buttons::ATTACK);
        run(&mut game, 2, &cycle, &mut msgs);
        let w = game.world().unwrap();
        let second = spectate_seat(w, 2);
        assert_ne!(second, first, "ATTACK cycles spectate target");
    }
}

#[test]
fn squads_duos_teammate_no_damage_downed_revive_and_ping() {
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
    {
        let w = game.world().unwrap();
        assert!(!w.team_damage, "duos disables friendly fire");
        assert_eq!(w.players[0].as_ref().unwrap().team, 0);
        assert_eq!(w.players[1].as_ref().unwrap().team, 0);
        assert_eq!(w.players[2].as_ref().unwrap().team, 1);
    }
    let (p0, p1) = {
        let w = game.world().unwrap();
        (body(w, 0), body(w, 1))
    };
    {
        let w = game.world_mut().unwrap();
        assert_eq!(room_sim::damage_mobj(w, p1, Some(p0), Some(p0), 50), 0);
    }

    {
        let w = game.world_mut().unwrap();
        w.team_damage = true;
        w.players[1].as_mut().unwrap().team = 1;
        let _ = room_sim::damage_mobj(w, p1, Some(p0), Some(p0), 500);
        w.players[1].as_mut().unwrap().team = 0;
        assert_eq!(w.actors.get(p1).unwrap().health, 1, "DBNO at 1 HP");
        let pos0 = w.actors.get(p0).unwrap().pos;
        if let Some(a) = w.actors.get_mut(p1) {
            a.pos = pos0;
        }
    }
    run(&mut game, 5, &idle, &mut msgs);

    let mut revive = idle;
    revive[0].buttons = Buttons(Buttons::USE);
    run(&mut game, 4 * 35 + 10, &revive, &mut msgs);
    assert!(
        game.world().unwrap().actors.get(p1).unwrap().health >= 30,
        "revive after 4s USE"
    );
}

#[test]
fn royale_wad_packs_track_b_lumps_and_decorate_class() {
    use room_decorate::parse_decorate;
    use room_sim::{ClassRegistry, StateTable};
    use std::sync::Arc;

    let vfs = VfsBuilder::new()
        .mount_memory("royale.wad", Bytes::from_static(ROYALE_WAD))
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
        classes.find_name("RoyaleSupplyBeacon").is_some(),
        "RoyaleSupplyBeacon from Track B DECORATE"
    );
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
