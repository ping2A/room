//! An AssemblyScript mod (`testdata/mods/hello_as.ts`, compiled with
//! `crates/room-web/as/room.ts` the way the editor does) runs in the engine:
//! class registration, greeting, timed spawn, kill callback.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_map::MapGeometry;
use room_script::ModuleScope;
use room_sim::TicCmd;
use room_sim::script_mods::ModSource;
use room_testkit::{MapBuilder, rect};

const HELLO_AS: &[u8] = include_bytes!("../../../testdata/mods/hello_as.wasm");

fn arena() -> MapGeometry {
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .player_start(512.0, 512.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    room_nodebuild::apply_nodes(&mut map);
    map
}

fn hello_source() -> ModSource {
    ModSource {
        mod_id: "test.hello-as".into(),
        name: "hello_as".into(),
        scope: ModuleScope::Play,
        bytes: HELLO_AS.to_vec(),
        options: Default::default(),
    }
}

fn run(tics: u32) -> (Headless, Vec<String>) {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(4);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    let names = game.set_script_mods(vec![hello_source()]).unwrap();
    assert_eq!(names, ["hello_as"]);
    game.start_custom_map(arena(), skill).unwrap();
    let mut msgs = Vec::new();
    for _ in 0..tics {
        game.tick(&[TicCmd::default()]);
        msgs.extend(game.world_mut().unwrap().drain_messages());
        msgs.extend(game.drain_script_log());
    }
    (game, msgs)
}

fn ghost_imps(game: &Headless) -> Vec<room_sim::ActorId> {
    let w = game.world().unwrap();
    let id = w.classes.find_name("GhostImp").unwrap();
    w.thinkers
        .default
        .iter()
        .copied()
        .filter(|&a| {
            w.actors
                .get(a)
                .is_some_and(|a| a.class == id && a.health > 0)
        })
        .collect()
}

#[test]
fn assemblyscript_mod_registers_greets_spawns_and_reacts_to_kills() {
    let (mut game, msgs) = run(3 * 35 + 2);
    assert!(
        msgs.iter()
            .any(|m| m == "Hello from AssemblyScript! skill 4"),
        "{msgs:?}"
    );
    assert!(!msgs.iter().any(|m| m.contains("disabled")), "{msgs:?}");
    let imps = ghost_imps(&game);
    assert_eq!(imps.len(), 1, "GhostImp spawned at 3 s: {msgs:?}");
    let imp = imps[0];
    assert_eq!(game.world().unwrap().actors.get(imp).unwrap().health, 30);

    // Kill it as the player: the mod heals the killer by 10.
    let w = game.world_mut().unwrap();
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    let before = w.actors.get(player).unwrap().health;
    room_sim::damage_mobj(w, imp, Some(player), Some(player), 1000);
    let mut msgs = Vec::new();
    for _ in 0..2 {
        game.tick(&[TicCmd::default()]);
        msgs.extend(game.world_mut().unwrap().drain_messages());
        msgs.extend(game.drain_script_log());
    }
    assert!(
        msgs.iter().any(|m| m == "Kill #1 (GhostImp): +10 health"),
        "{msgs:?}"
    );
    let w = game.world().unwrap();
    assert_eq!(w.actors.get(player).unwrap().health, before + 10);
}

#[test]
fn hot_swap_keeps_the_level_running() {
    let (mut game, _) = run(50);
    let maptime = game.world().unwrap().level.maptime;
    let classes = game.classes().len();
    assert_eq!(
        game.hot_swap_script_mods(vec![hello_source()]).unwrap(),
        ["hello_as"]
    );
    let mut msgs = game.world_mut().unwrap().drain_messages();
    msgs.extend(game.drain_script_log());
    assert!(
        msgs.iter()
            .any(|m| m == "Hello from AssemblyScript! skill 4"),
        "new instance got WorldLoaded: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("restart the level")),
        "{msgs:?}"
    );
    assert_eq!(game.world().unwrap().level.maptime, maptime, "same level");
    assert_eq!(game.classes().len(), classes, "classes not merged twice");
    game.tick(&[TicCmd::default()]);

    // Removing every mod mid-level also works.
    assert!(game.hot_swap_script_mods(Vec::new()).unwrap().is_empty());
    assert!(game.classes().find_name("GhostImp").is_none());
    game.tick(&[TicCmd::default()]);
}
