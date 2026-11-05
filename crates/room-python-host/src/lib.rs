//! Host-side CPython play mods for desktop (`room-app --features python-mods`).
//!
//! Default build is a stub (no Python). Enable the `embed` feature to link pyo3.

#![cfg_attr(not(feature = "embed"), allow(unused_imports))]
#![allow(missing_docs)]

use room_game::{NativePlayMod, PythonModSource};

#[cfg(feature = "embed")]
mod embed;

/// Build boxed native mods from discovered Python sources.
pub fn load_native_mods(
    sources: &[PythonModSource],
) -> Result<(Vec<Box<dyn NativePlayMod>>, Vec<String>), String> {
    if sources.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    #[cfg(feature = "embed")]
    {
        embed::load_native_mods(sources)
    }
    #[cfg(not(feature = "embed"))]
    {
        let _ = sources;
        Err("Python mods require `room-app --features python-mods` (room-python-host/embed)".into())
    }
}

#[cfg(all(test, feature = "embed"))]
mod smoke {
    use super::*;
    use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn hello_mod_spawns_imp_after_three_seconds() {
        let src = include_str!("../../../mods/doom1-python-hello/hello_mod.py");
        let sources = [PythonModSource {
            mod_id: "test".into(),
            name: "hello".into(),
            source: src.into(),
        }];
        let (native, log) = load_native_mods(&sources).expect("load");
        assert!(!log.is_empty(), "{log:?}");

        let data = load_embedded_engine_data().expect("engine");
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
            .player_start(512.0, 512.0, 0)
            .build_udmf("MAP01")
            .expect("map");
        room_nodebuild::apply_nodes(&mut map);

        let mut game = Headless::new(GameOptions {
            skill: Skill::new(4),
            ..Default::default()
        });
        game.set_defs(data.classes, data.states);
        game.set_native_mods(native);
        game.start_custom_map(map, Skill::new(4)).expect("start");

        for _ in 0..(35 * 4) {
            game.tick(&[]);
        }

        let world = game.world().expect("world");
        let imps = world
            .thinkers
            .default
            .iter()
            .filter_map(|&id| world.actors.get(id))
            .filter(|a| !a.destroyed && a.health > 0)
            .filter(|a| {
                world
                    .classes
                    .get(a.class)
                    .is_some_and(|c| c.name.eq_ignore_ascii_case("DoomImp"))
            })
            .count();
        assert!(imps >= 1, "expected a spawned DoomImp, got {imps}");
    }
}
