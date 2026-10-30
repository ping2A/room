//! BE4: MOD1 on `TicCmd` reaches raw `player_cmd`; movement unchanged.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::apply_nodes;
use room_script::ModuleScope;
use room_sim::script_mods::ModSource;
use room_sim::{Buttons, TicCmd};
use room_testkit::{MapBuilder, rect};

fn mod1_probe_mod() -> ModSource {
    let wasm = wat::parse_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../room-script/testdata/player_mod1.wat"
    ))
    .expect("player_mod1.wat");
    ModSource {
        mod_id: "test.mod1".into(),
        name: "mod1".into(),
        scope: ModuleScope::Play,
        bytes: wasm,
    }
}

#[test]
fn mod1_bit_in_player_cmd_and_movement_ok() {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(
        game.set_script_mods(vec![mod1_probe_mod()]).unwrap(),
        ["mod1"]
    );
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    game.start_custom_map(map, skill).unwrap();
    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    let x0 = game.world().unwrap().actors.get(mo).unwrap().pos.x();
    let mut cmd = TicCmd::default();
    cmd.forward_move = 25 * 256;
    cmd.buttons = Buttons(Buttons::MOD1);
    game.tick(&[cmd]);
    let w = game.world().unwrap();
    let x1 = w.actors.get(mo).unwrap().pos.x();
    assert!(x1 > x0, "forward move with MOD1");
    assert_eq!(
        w.players[0].as_ref().unwrap().raw_cmd.buttons.0 & Buttons::MOD1,
        Buttons::MOD1
    );
    assert!(
        game.drain_script_log().iter().any(|m| m.contains("mod1=1")),
        "guest should read MOD1 from player_cmd"
    );
}
