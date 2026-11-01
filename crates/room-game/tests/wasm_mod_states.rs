//! MA1: module states merge + animate (`docs/31`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::apply_nodes;
use room_script::ModuleScope;
use room_sim::TicCmd;
use room_sim::script_mods::ModSource;
use room_testkit::{MapBuilder, rect};

fn states_mod() -> ModSource {
    let wasm = wat::parse_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../room-script/testdata/mod_states.wat"
    ))
    .expect("mod_states.wat");
    ModSource {
        mod_id: "test.mod_states".into(),
        name: "mod_states".into(),
        scope: ModuleScope::Play,
        bytes: wasm,
        options: Default::default(),
    }
}

#[test]
fn wasm_mod_states_spawn_loop_animates() {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    game.set_script_mods(vec![states_mod()]).unwrap();

    let spin = game
        .classes()
        .find_name("SpinImp")
        .expect("SpinImp class from mod");
    let spawn = game
        .states()
        .find_label(spin, "Spawn")
        .expect("mod Spawn label");
    let a = game.states().get(spawn).unwrap();
    assert_eq!(a.sprite, "TROO");
    assert!(a.action.is_none(), "idle loop has no action");
    assert_eq!(a.tics, 5);
    let b_id = a.next.expect("frame A next");
    let b = game.states().get(b_id).unwrap();
    assert_eq!(b.frame, 1);
    assert_eq!(b.next, Some(spawn), "Loop → Spawn");

    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 1024.0, 512.0), 0.0, 128.0)
        .player_start(64.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    game.start_custom_map(map, skill).unwrap();

    let spin_count = |g: &Headless| {
        g.world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                g.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == spin)
            })
            .count()
    };
    assert_eq!(spin_count(&game), 1, "WorldLoaded spawned SpinImp");

    let actor_id = game
        .world()
        .unwrap()
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            game.world()
                .unwrap()
                .actors
                .get(id)
                .is_some_and(|a| a.class == spin)
        })
        .unwrap();
    let (frame0, tics0, state0) = {
        let a = game.world().unwrap().actors.get(actor_id).unwrap();
        (a.frame, a.tics, a.state)
    };
    assert_eq!(state0, Some(spawn), "spawned with mod Spawn state");
    assert_eq!(tics0, 5, "spawn tics from mod state");
    for _ in 0..6 {
        game.tick(&[TicCmd::default()]);
    }
    let (frame1, tics1, state1) = {
        let a = game.world().unwrap().actors.get(actor_id).unwrap();
        (a.frame, a.tics, a.state)
    };
    assert_eq!(
        state1,
        Some(b_id),
        "after 6 tics should be on frame B (tics0={tics0} frame0={frame0} tics1={tics1} frame1={frame1})"
    );
    assert_eq!(frame1, 1);
}
