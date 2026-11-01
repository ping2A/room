//! MH3: input capture blocks weapon switch; mod reads raw `player_cmd`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::apply_nodes;
use room_script::ModuleScope;
use room_sim::script_mods::ModSource;
use room_sim::{Buttons, TicCmd};
use room_testkit::{MapBuilder, rect};

fn capture_mod() -> ModSource {
    let wasm = wat::parse_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../room-script/testdata/player_capture.wat"
    ))
    .expect("player_capture.wat");
    ModSource {
        mod_id: "test.capture".into(),
        name: "capture".into(),
        scope: ModuleScope::Play,
        bytes: wasm,
        options: Default::default(),
    }
}

#[test]
fn captured_weapon_select_does_not_switch_and_mod_reads_it() {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(
        game.set_script_mods(vec![capture_mod()]).unwrap(),
        ["capture"]
    );

    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    game.start_custom_map(map, skill).unwrap();

    let ready_before = {
        let w = game.world().unwrap();
        let p = w.players[0].as_ref().unwrap();
        assert_ne!(p.input_capture, 0, "mod should capture on WorldLoaded");
        p.ready_weapon
    };

    let mut cmd = TicCmd::default();
    cmd.weapon_select = 2; // shotgun slot
    game.tick(&[cmd]);

    let w = game.world().unwrap();
    let p = w.players[0].as_ref().unwrap();
    assert_eq!(
        p.ready_weapon, ready_before,
        "capture must block weapon_select"
    );
    assert_eq!(p.raw_cmd.weapon_select, 2);
    assert_eq!(p.cmd.weapon_select, 0, "filtered cmd clears select");
}

#[test]
fn capture_released_on_death() {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(
        game.set_script_mods(vec![capture_mod()]).unwrap(),
        ["capture"]
    );
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    game.start_custom_map(map, skill).unwrap();
    assert_ne!(
        game.world().unwrap().players[0]
            .as_ref()
            .unwrap()
            .input_capture,
        0
    );

    {
        let w = game.world_mut().unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.health = 0;
        }
        if let Some(p) = w.players[0].as_mut() {
            p.player_state = room_sim::PlayerState::Dead;
        }
    }
    game.tick(&[TicCmd {
        buttons: Buttons(Buttons::USE),
        ..Default::default()
    }]);
    assert_eq!(
        game.world().unwrap().players[0]
            .as_ref()
            .unwrap()
            .input_capture,
        0,
        "death must release capture"
    );
}

#[test]
fn two_peer_capture_checksums_match() {
    let run = || {
        let data = load_embedded_engine_data().unwrap();
        let skill = Skill::new(3);
        let mut game = Headless::new(GameOptions {
            skill,
            ..Default::default()
        });
        game.set_defs(data.classes, data.states);
        assert_eq!(
            game.set_script_mods(vec![capture_mod()]).unwrap(),
            ["capture"]
        );
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        game.start_custom_map(map, skill).unwrap();
        let mut cmd = TicCmd::default();
        cmd.weapon_select = 3;
        cmd.buttons = Buttons(Buttons::ATTACK);
        for _ in 0..30 {
            game.tick(&[cmd]);
            cmd.weapon_select = if cmd.weapon_select == 3 { 1 } else { 3 };
        }
        game.world().unwrap().checksum()
    };
    assert_eq!(run(), run());
}
