//! Track C C2–C5: flags, player flash props, quake, radius attack.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::apply_nodes;
use room_script::ModuleScope;
use room_sim::TicCmd;
use room_sim::script_mods::ModSource;
use room_testkit::{MapBuilder, rect};

fn track_c_mod() -> ModSource {
    let wasm = wat::parse_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../room-script/testdata/track_c.wat"
    ))
    .expect("track_c.wat");
    ModSource {
        mod_id: "test.track_c".into(),
        name: "track_c".into(),
        scope: ModuleScope::Play,
        bytes: wasm,
    }
}

#[test]
fn track_c_flag_flash_quake_and_radius() {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    game.set_script_mods(vec![track_c_mod()]).unwrap();
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    game.start_custom_map(map, skill).unwrap();
    // WORLD_LOADED already ran during start; inspect before a full tic decays flashes.
    {
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let a = w.actors.get(mo).unwrap();
        assert!(
            a.flags & room_sim::FLAG_INVULNERABLE != 0,
            "C2 INVULNERABLE set"
        );
        assert_eq!(
            w.players[0].as_ref().unwrap().bonus_count,
            12,
            "C3 BONUSCOUNT"
        );
        assert!(!w.quakes.is_empty(), "C4 quake queued");
    }
    game.tick(&[TicCmd::default()]);
}
