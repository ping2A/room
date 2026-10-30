//! BE1: WAT mod sets/clears zone rings; checksum unchanged.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::apply_nodes;
use room_script::ModuleScope;
use room_sim::script_mods::ModSource;
use room_testkit::{MapBuilder, rect};

fn zone_mod() -> ModSource {
    let wasm = wat::parse_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../room-script/testdata/zone_set.wat"
    ))
    .expect("zone_set.wat");
    ModSource {
        mod_id: "test.zone".into(),
        name: "zone".into(),
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
    game.set_script_mods(vec![zone_mod()]).unwrap();
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
fn wasm_mod_zone_set_clear_and_checksum() {
    let game = start();
    let c0 = game.world().unwrap().checksum();
    {
        let w = game.world().unwrap();
        assert_eq!(w.mod_zones.len(), 3, "id 4 rejected; id 1 cleared");
        let z0 = w.mod_zones.get(0).expect("zone 0");
        assert_eq!(z0.cx, 256);
        assert_eq!(z0.r, 512);
        assert_eq!(z0.next_cx, 300);
        assert!(w.mod_zones.get(1).is_none());
        assert!(w.mod_zones.get(2).is_some());
        assert!(w.mod_zones.get(3).is_some());
    }
    assert_eq!(game.world().unwrap().checksum(), c0);
    assert_ne!(game.world().unwrap().mod_zones_hash(), 0);
}
