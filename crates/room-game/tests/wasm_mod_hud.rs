//! MH2: a WAT mod creates, patches, removes, and clears Mod HUD widgets.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::apply_nodes;
use room_script::ModuleScope;
use room_sim::TicCmd;
use room_sim::mod_hud::{WidgetKey, kind};
use room_sim::script_mods::ModSource;
use room_testkit::{MapBuilder, rect};

fn map_hud_mod() -> ModSource {
    let wasm = wat::parse_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../room-script/testdata/hud_map.wat"
    ))
    .expect("hud_map.wat");
    ModSource {
        mod_id: "test.map_hud".into(),
        name: "map_hud".into(),
        scope: ModuleScope::Play,
        bytes: wasm,
        options: Default::default(),
    }
}

fn hud_mod() -> ModSource {
    let wasm = wat::parse_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../room-script/testdata/hud_widget.wat"
    ))
    .expect("hud_widget.wat");
    ModSource {
        mod_id: "test.hud".into(),
        name: "hud".into(),
        scope: ModuleScope::Play,
        bytes: wasm,
        options: Default::default(),
    }
}

fn noop_mod() -> ModSource {
    let wasm = wat::parse_str(
        r#"(module
  (memory (export "memory") 1)
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 64))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32) (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param i32 i32 i32) (result i64) (i64.const 0)))"#,
    )
    .unwrap();
    ModSource {
        mod_id: "test.noop".into(),
        name: "noop".into(),
        scope: ModuleScope::Play,
        bytes: wasm,
        options: Default::default(),
    }
}

fn start(mods: Vec<ModSource>) -> Headless {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    game.set_script_mods(mods).unwrap();
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    game.start_custom_map(map, skill).unwrap();
    game
}

fn widget(game: &Headless, module: u16, id: u16) -> Option<&room_sim::mod_hud::HudWidget> {
    let key = WidgetKey {
        module,
        seat1: 1,
        id,
    };
    game.world()
        .unwrap()
        .mod_hud
        .iter()
        .find(|(k, _)| **k == key)
        .map(|(_, w)| w)
}

#[test]
fn wasm_mod_creates_patches_removes_and_clears_widgets() {
    let mut game = start(vec![hud_mod()]);
    {
        let text = widget(&game, 0, 7).expect("text widget after WorldLoaded");
        assert_eq!(text.kind, kind::TEXT);
        assert_eq!(text.text, "HELLO");
        assert_eq!(text.y, 24);
        assert_eq!(text.font, 1);
        assert_eq!(text.text_color, 5);
        assert_eq!(text.alpha, 1000);
        assert_eq!(text.value, 1);
        let fill = widget(&game, 0, 8).expect("short prefix widget");
        assert_eq!(fill.kind, kind::FILL);
        assert_eq!(fill.y, 24, "omitted fields keep defaults");
        assert_eq!(fill.alpha, 1000);
        assert_eq!(fill.color, 0xffff_ffff);
        assert!(fill.text.is_empty());
    }

    game.tick(&[TicCmd::default()]);
    assert_eq!(widget(&game, 0, 7).unwrap().value, 99);

    game.tick(&[TicCmd::default()]);
    assert!(widget(&game, 0, 7).is_none(), "removed on tick 2");
    assert!(widget(&game, 0, 8).is_some(), "other id stays");

    game.tick(&[TicCmd::default()]);
    assert!(
        widget(&game, 0, 8).is_none(),
        "hud_clear(seat1) drops the seat"
    );
    let log = game.drain_script_log();
    assert!(!log.iter().any(|m| m.contains("disabled")), "{log:?}");
}

#[test]
fn widget_owner_is_the_calling_module() {
    let game = start(vec![noop_mod(), hud_mod()]);
    let text = widget(&game, 1, 7).expect("owned by the second module");
    assert_eq!(text.text, "HELLO");
    assert!(widget(&game, 0, 7).is_none());
}

#[test]
fn map_widget_kind_is_stored() {
    let game = start(vec![map_hud_mod()]);
    let w = widget(&game, 0, 50).expect("MAP widget");
    assert_eq!(w.kind, kind::MAP);
    assert_eq!(w.w, 64);
    assert_eq!(w.h, 48);
    assert_eq!(w.max, 3);
}
