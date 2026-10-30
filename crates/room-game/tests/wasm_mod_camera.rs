//! BE9: `player_set_camera` is sim state and affects checksum.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::apply_nodes;
use room_script::ModuleScope;
use room_sim::TicCmd;
use room_sim::script_mods::ModSource;
use room_testkit::{MapBuilder, rect};

fn camera_mod() -> ModSource {
    let wasm = wat::parse_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../room-script/testdata/player_set_camera.wat"
    ))
    .expect("player_set_camera.wat");
    ModSource {
        mod_id: "test.camera".into(),
        name: "camera".into(),
        scope: ModuleScope::Play,
        bytes: wasm,
    }
}

fn start() -> Headless {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    game.set_script_mods(vec![camera_mod()]).unwrap();
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    game.start_custom_map(map, skill).unwrap();
    game
}

#[test]
fn wasm_mod_player_set_camera_checksum() {
    let mut game = start();
    let c0 = game.world().unwrap().checksum();
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo;
    assert_eq!(w.players[0].as_ref().unwrap().camera, mo);
    assert_ne!(w.checksum(), c0);
}
