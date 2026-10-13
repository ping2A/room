//! Built-in combat arena: spawn monsters, give arsenal, fire, register kills.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_decorate::parse_decorate;
use room_game::{GameOptions, GameState, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::apply_nodes;
use room_sim::{Buttons, ClassRegistry, FLAG_COUNTKILL, StateTable, TicCmd};
use room_testkit::combat_arena;
use std::sync::Arc;

fn load_defs() -> (Arc<ClassRegistry>, Arc<StateTable>) {
    if let Ok(data) = load_embedded_engine_data() {
        return (data.classes, data.states);
    }
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    (classes, states)
}

#[test]
fn combat_arena_spawns_monsters_and_weapons() {
    let mut map = combat_arena().build_udmf("MAP01").unwrap();
    apply_nodes(&mut map);
    let monsters = map
        .things
        .iter()
        .filter(|t| matches!(t.type_id, 3004 | 9 | 65 | 3001))
        .count();
    let weapons = map
        .things
        .iter()
        .filter(|t| matches!(t.type_id, 2001 | 2002 | 2003 | 2004 | 82))
        .count();
    assert!(
        monsters >= 10,
        "expected a pack of monsters, got {monsters}"
    );
    assert!(weapons >= 5, "expected weapon pickups, got {weapons}");
}

#[test]
fn combat_arena_give_all_and_fire_kills() {
    let (classes, states) = load_defs();
    let mut map = combat_arena().build_udmf("MAP01").unwrap();
    apply_nodes(&mut map);

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 2026,
        record_demo: false,
        ..Default::default()
    });
    game.set_defs(classes, states);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    assert_eq!(game.state, GameState::Level);

    let world = game.world().unwrap();
    let kills_before = world.level.found_kills;
    let live_monsters = world
        .thinkers
        .default
        .iter()
        .filter(|&&id| {
            world
                .actors
                .get(id)
                .is_some_and(|a| !a.destroyed && a.health > 0 && a.flags & FLAG_COUNTKILL != 0)
        })
        .count();
    assert!(
        live_monsters >= 8,
        "expected live monsters in arena, got {live_monsters}"
    );

    let given = game.cheat_give(0, "all");
    assert!(given > 0, "give all should grant weapons/ammo");

    // Face north (toward the pack) and hold fire with chaingun if available.
    if let Some(world) = game.world_mut() {
        if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
            if let Some(a) = world.actors.get_mut(mo) {
                a.angle = room_core::Angle::from_degrees(90.0);
            }
        }
        if let Some(cg) = world.classes.find_name("Chaingun") {
            if let Some(p) = world.players[0].as_mut() {
                p.ready_weapon = Some(cg);
                p.pspr_state = None;
            }
        }
    }

    let fire = TicCmd {
        buttons: Buttons(Buttons::ATTACK),
        forward_move: 25 * 256,
        ..TicCmd::default()
    };
    for _ in 0..350 {
        game.tick(&[fire]);
    }

    let world = game.world().unwrap();
    assert!(
        world.level.found_kills > kills_before,
        "expected kills after firing (before={}, after={})",
        kills_before,
        world.level.found_kills
    );
}
