//! ZombieMan must enter Missile E/F frames (visible attack pose) before hitscan.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ferrite_decorate::parse_decorate;
use ferrite_nodebuild::apply_nodes;
use ferrite_sim::{ClassRegistry, StateTable, World};
use ferrite_testkit::{MapBuilder, rect};
use std::sync::Arc;

#[test]
fn zombie_shows_missile_frames_before_shot() {
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 1024.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 128.0, 0)
        .thing(3004, 400.0, 128.0, 180)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let mut w =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    let zid = classes.find_name("ZombieMan").unwrap();
    let zombie = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| w.actors.get(id).is_some_and(|a| a.class == zid))
        .expect("zombie");
    let player = w.players[0].as_ref().and_then(|p| p.mo).expect("p1");
    let see = states.find_label(zid, "See").unwrap();
    w.set_state(zombie, Some(see), false);
    if let Some(a) = w.actors.get_mut(zombie) {
        a.target = Some(player);
        a.move_count = 0;
        a.reaction_time = 0;
        a.threshold = 0;
    }
    let mut saw_e = false;
    let mut saw_f = false;
    let mut log = Vec::new();
    for tic in 0..500 {
        if let Some(a) = w.actors.get_mut(zombie) {
            a.move_count = 0;
            a.target = Some(player);
        }
        w.tick();
        let a = w.actors.get(zombie).unwrap();
        if a.sprite == "POSS" && a.frame == 4 {
            saw_e = true;
        }
        if a.sprite == "POSS" && a.frame == 5 {
            saw_f = true;
        }
        if tic % 20 == 0 || a.frame >= 4 {
            log.push(format!(
                "t{tic}: frame={} tics={} label_hint={}",
                a.frame, a.tics, a.frame
            ));
        }
        if saw_e && saw_f {
            break;
        }
    }
    for line in &log {
        eprintln!("{line}");
    }
    assert!(saw_e, "never showed Missile E pose");
    assert!(saw_f, "never showed Missile F (shot) pose");
}
