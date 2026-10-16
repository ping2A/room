//! Vanilla skill effects: baby damage/ammo, Nightmare fast states, fireballs, reaction time.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_core::{Angle, DVec3};
use room_decorate::parse_decorate;
use room_nodebuild::apply_nodes;
use room_sim::{ClassRegistry, StateTable, World, count_inventory, damage_mobj, try_pickup};
use room_testkit::{MapBuilder, rect};
use std::sync::Arc;

fn world_at(skill: u8) -> World {
    let src = include_str!("../../../wadsrc/room/actors/doom.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 1024.0, 512.0), 0.0, 128.0)
        .player_start(64.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    World::from_map_with_skill(map, classes, states, skill).unwrap()
}

fn player(w: &World) -> room_sim::ActorId {
    w.players[0].as_ref().and_then(|p| p.mo).unwrap()
}

fn spawn(w: &mut World, class: &str, x: f64, y: f64) -> room_sim::ActorId {
    let cid = w.classes.find_name(class).unwrap();
    w.spawn_at(cid, DVec3::new(x, y, 0.0), Angle::ZERO).unwrap()
}

#[test]
fn baby_halves_player_damage_only() {
    let mut baby = world_at(1);
    let mut hmp = world_at(3);
    let (pb, ph) = (player(&baby), player(&hmp));
    assert_eq!(damage_mobj(&mut baby, pb, None, None, 20), 10);
    assert_eq!(damage_mobj(&mut hmp, ph, None, None, 20), 20);
    let imp = spawn(&mut baby, "DoomImp", 500.0, 128.0);
    assert_eq!(damage_mobj(&mut baby, imp, None, None, 20), 20);
}

#[test]
fn baby_and_nightmare_double_ammo_pickups() {
    for (skill, want) in [(1, 2), (3, 1), (4, 1), (5, 2)] {
        let mut w = world_at(skill);
        let p = player(&w);
        let clip = w.classes.find_name("Clip").unwrap();
        let before = count_inventory(&w, p, clip);
        let item = spawn(&mut w, "Clip", 64.0, 128.0);
        assert!(try_pickup(&mut w, p, item), "skill {skill}");
        assert_eq!(
            count_inventory(&w, p, clip) - before,
            10 * want,
            "skill {skill}"
        );
    }
}

#[test]
fn nightmare_halves_demon_fast_states() {
    for (skill, want) in [(4, 2), (5, 1)] {
        let mut w = world_at(skill);
        let demon = spawn(&mut w, "Demon", 500.0, 128.0);
        let cid = w.actors.get(demon).unwrap().class;
        let see = w.states.find_label(cid, "See").unwrap();
        w.set_state(demon, Some(see), false);
        assert_eq!(w.actors.get(demon).unwrap().tics, want, "skill {skill}");
        // Non-fast monster states keep their tics.
        let imp = spawn(&mut w, "DoomImp", 700.0, 128.0);
        let icid = w.actors.get(imp).unwrap().class;
        let isee = w.states.find_label(icid, "See").unwrap();
        w.set_state(imp, Some(isee), false);
        assert_eq!(w.actors.get(imp).unwrap().tics, 3, "imp skill {skill}");
    }
}

#[test]
fn nightmare_fireballs_and_reaction_time() {
    let mut uv = world_at(4);
    let mut nm = world_at(5);
    let uv_imp = spawn(&mut uv, "DoomImp", 500.0, 128.0);
    let nm_imp = spawn(&mut nm, "DoomImp", 500.0, 128.0);
    assert_eq!(uv.actors.get(uv_imp).unwrap().reaction_time, 8);
    assert_eq!(nm.actors.get(nm_imp).unwrap().reaction_time, 0);
    let ball = nm.classes.find_name("DoomImpBall").unwrap();
    let def = nm.classes.get(ball).unwrap();
    assert!((def.speed - 10.0).abs() < 1e-9);
    assert!((def.fast_speed - 20.0).abs() < 1e-9);
    assert!(nm.fast_monsters && nm.respawn_monsters);
    assert!(!uv.fast_monsters && !uv.respawn_monsters);
}
