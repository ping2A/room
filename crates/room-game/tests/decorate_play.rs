//! Smoke: load doom_basic DECORATE, spawn map with zombie, tick deterministically.

#![allow(clippy::unwrap_used)]

use room_decorate::parse_decorate;
use room_game::{GameOptions, GameState, Headless, Skill};
use room_nodebuild::apply_nodes;
use room_sim::{Buttons, ClassRegistry, StateTable, TicCmd};
use room_testkit::{MapBuilder, rect};
use std::sync::Arc;

#[test]
fn decorate_map_ticks_with_zombie_and_exit() {
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));

    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    map.things.push(room_map::RawThing {
        tid: 0,
        x: 200,
        y: 64,
        z: 0,
        angle: 180,
        type_id: 3004,
        flags: 0,
        special: 0,
        args: [0; 5],
        hexen: false,
        friendly_see_blocks: -1,
        health: 0,
        score: -2147483648,
        gravity: -1.0,
        scale_x: 0.0,
        scale_y: 0.0,
        alpha: -1.0,
        pitch: -32768,
        roll: -32768,
        fill_color: -1,
        render_style: String::new(),
        float_bob_phase: -1,
        count_secret: false,
        no_count: false,
        arg0_str: String::new(),
        conversation: 0,
        accuracy: i32::MIN,
        stamina: i32::MIN,
        species: String::new(),
        melee_range: -1.0,
        view_height: -1.0,
        mass: i32::MIN,
        speed: -1.0,
        friction: -1.0,
        damage_factor: -1.0,
        max_step_height: -1.0,
        max_dropoff_height: -1.0,
        float_speed: -1.0,
        fast_speed: -1.0,
        jump_z: -1.0,
        attack_z_offset: -1.0,
        ext: 0,
        user: Default::default(),
    });
    apply_nodes(&mut map);

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        record_demo: true,
        ..Default::default()
    });
    game.set_defs(classes, states);
    game.start_custom_map(map, Skill::new(3)).unwrap();

    assert!(game.world().unwrap().level.total_kills >= 1);
    let c0 = game.checksum().unwrap();
    let cmd = TicCmd {
        forward_move: 50 * 256,
        buttons: Buttons(Buttons::ATTACK),
        ..TicCmd::default()
    };
    for _ in 0..35 {
        game.tick(&[cmd]);
    }
    assert_ne!(c0, game.checksum().unwrap());

    game.world_mut().unwrap().level.exiting = true;
    game.tick(&[]);
    assert_eq!(game.state, GameState::Intermission);
    assert_eq!(game.intermission_stats().unwrap().map_name, "MAP01");
}
