//! BE3: `player_name` reads session names; checksum includes them.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::apply_nodes;
use room_script::ModuleScope;
use room_sim::TicCmd;
use room_sim::script_mods::ModSource;
use room_testkit::{MapBuilder, rect};

fn name_mod() -> ModSource {
    let wasm = wat::parse_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../room-script/testdata/player_name.wat"
    ))
    .expect("player_name.wat");
    ModSource {
        mod_id: "test.player_name".into(),
        name: "player_name".into(),
        scope: ModuleScope::Play,
        bytes: wasm,
    }
}

fn start_with_name(name: Option<&str>) -> Headless {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    if let Some(n) = name {
        game.set_player_display_name(0, n);
    }
    game.set_defs(data.classes, data.states);
    game.set_script_mods(vec![name_mod()]).unwrap();
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
fn wasm_mod_player_name_checksum_and_fallback() {
    let game = start_with_name(Some("Alice"));
    let c_named = game.world().unwrap().checksum();

    let game_fb = start_with_name(None);
    let c_fallback = game_fb.world().unwrap().checksum();
    assert_ne!(c_named, c_fallback);

    let mut buf = [0u8; 32];
    let n = game_fb.world().unwrap().player_names.write_display(0, &mut buf);
    assert_eq!(n, 8);
    assert_eq!(&buf[..8], b"Player 1");

    let mut game = start_with_name(Some("Alice"));
    game.tick(&[TicCmd::default()]);
    assert_eq!(game.world().unwrap().player_names.stored(0), b"Alice");
}
