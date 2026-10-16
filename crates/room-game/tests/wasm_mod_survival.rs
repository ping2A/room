//! The example WASM mod (`mods/doom1-survival/survival.wad`) end to end:
//! discovered from a PWAD, classes merged, waves spawned on schedule per skill.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bytes::Bytes;
use room_game::{GameOptions, Headless, Skill, discover_mods, load_embedded_engine_data};
use room_map::{MapGeometry, load_map};
use room_nodebuild::ensure_nodes;
use room_sim::TicCmd;
use room_sim::script_mods::ModSource;
use room_testkit::{MapBuilder, rect};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

const SURVIVAL_WAD: &[u8] = include_bytes!("../../../mods/doom1-survival/survival.wad");

fn survival_mods() -> Vec<ModSource> {
    let vfs = VfsBuilder::new()
        .mount_memory("survival.wad", Bytes::from_static(SURVIVAL_WAD))
        .unwrap()
        .build()
        .unwrap();
    let (mods, errors) = discover_mods(&vfs);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(mods.len(), 1);
    mods
}

fn arena() -> MapGeometry {
    let mut b = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .player_start(128.0, 128.0, 0);
    for (x, y) in [
        (1800.0, 1800.0),
        (1800.0, 300.0),
        (300.0, 1800.0),
        (1000.0, 1000.0),
    ] {
        b = b.thing(2007, x, y, 0);
    }
    let mut map = b.build_udmf("MAP01").unwrap();
    room_nodebuild::apply_nodes(&mut map);
    map
}

fn run(map: MapGeometry, skill: u8, tics: u32) -> (Headless, Vec<String>) {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(skill);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    assert_eq!(game.set_script_mods(survival_mods()).unwrap(), ["survival"]);
    game.start_custom_map(map, skill).unwrap();
    let mut msgs = Vec::new();
    for _ in 0..tics {
        game.tick(&[TicCmd::default()]);
        msgs.extend(game.world_mut().unwrap().drain_messages());
        msgs.extend(game.drain_script_log());
    }
    (game, msgs)
}

fn count(game: &Headless, class: &str) -> usize {
    let w = game.world().unwrap();
    let Some(id) = w.classes.find_name(class) else {
        return 0;
    };
    w.thinkers
        .default
        .iter()
        .filter(|&&a| {
            w.actors
                .get(a)
                .is_some_and(|a| a.class == id && a.health > 0)
        })
        .count()
}

#[test]
fn survival_mod_registers_classes_and_spawns_first_wave_on_skill_schedule() {
    // UV: 32 s between waves, first wave after half of that (560 tics).
    let (game, msgs) = run(arena(), 4, 540);
    assert!(game.classes().find_name("PhantomImp").is_some());
    assert!(game.classes().find_name("Juggernaut").is_some());
    assert!(
        msgs.iter()
            .any(|m| m == "Survival Director: skill 4 - first wave in 16s"),
        "{msgs:?}"
    );
    assert_eq!(count(&game, "ZombieMan"), 0, "no wave yet");

    let (game, msgs) = run(arena(), 4, 600);
    assert!(msgs.iter().any(|m| m == "Wave 1 - 6 incoming"), "{msgs:?}");
    let zombies = count(&game, "ZombieMan");
    assert!(zombies > 0, "wave 1 spawned zombies");
    assert_eq!(
        game.world().unwrap().level.total_kills as usize,
        zombies,
        "rejected spawns are not counted"
    );
    assert!(!msgs.iter().any(|m| m.contains("disabled")), "{msgs:?}");

    // Nightmare: 25 s gap, first wave at 437 tics.
    let (_, msgs) = run(arena(), 5, 450);
    assert!(msgs.iter().any(|m| m.starts_with("Wave 1 - ")), "{msgs:?}");
}

#[test]
fn survival_mod_is_deterministic() {
    let (a, _) = run(arena(), 3, 900);
    let (b, _) = run(arena(), 3, 900);
    assert_eq!(a.world().unwrap().checksum(), b.world().unwrap().checksum());
}

#[test]
fn survival_mod_on_doom1_e1m1() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    if !p.is_file() {
        eprintln!("skip: testdata/doom/doom1.wad missing");
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&p).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let before = {
        let (game, _) = run(map.clone(), 3, 1);
        count(&game, "ZombieMan") + count(&game, "ShotgunGuy")
    };
    // HMP: first wave after 20 s.
    let (game, msgs) = run(map, 3, 720);
    assert!(msgs.iter().any(|m| m == "Wave 1 - 5 incoming"), "{msgs:?}");
    let after = count(&game, "ZombieMan") + count(&game, "ShotgunGuy");
    assert!(after > before, "wave spawned on E1M1 ({before} → {after})");
}
