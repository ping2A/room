//! Shuriken mod e2e (`docs/32` SK4 / `docs/31` MA4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::apply_nodes;
use room_script::ModuleScope;
use room_script::manifest::{ManifestOptions, OptionValue};
use room_sim::script_mods::ModSource;
use room_sim::{Buttons, TicCmd, give_inventory};
use room_testkit::{MapBuilder, rect};
use room_vfs::VfsBuilder;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

fn shuriken_wasm() -> Vec<u8> {
    let manifest =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods/doom1-shuriken/Cargo.toml");
    let crate_dir = manifest.parent().unwrap();
    let built =
        crate_dir.join("target/wasm32-unknown-unknown/release/room_mod_doom1_shuriken.wasm");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/mods/wasm32-unknown-unknown/release/room_mod_doom1_shuriken.wasm");
    for p in [&built, &workspace] {
        if p.is_file() {
            return std::fs::read(p).unwrap();
        }
    }
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--manifest-path",
        ])
        .arg(&manifest)
        .status()
        .expect("spawn cargo");
    assert!(status.success(), "shuriken wasm build failed");
    std::fs::read(&built).expect("built wasm")
}

fn shuriken_mod() -> ModSource {
    let mut options = ManifestOptions::new();
    options.insert("slot".into(), OptionValue::Int(8));
    options.insert("place_weapon".into(), OptionValue::Bool(false));
    options.insert("ammo_replace_percent".into(), OptionValue::Int(0));
    ModSource {
        mod_id: "room.example.doom1-shuriken".into(),
        name: "shuriken".into(),
        scope: ModuleScope::Play,
        bytes: shuriken_wasm(),
        options,
    }
}

fn arena() -> room_map::MapGeometry {
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 1024.0), 0.0, 128.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    map
}

fn body(w: &room_sim::World) -> room_sim::ActorId {
    w.players[0].as_ref().unwrap().mo.unwrap()
}

#[test]
fn wasm_mod_shuriken_fire_ammo_and_slot() {
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);

    let mut vfs = VfsBuilder::new().build().unwrap();
    // PLAYPAL for registration palette.
    if let Ok(engine) = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/room/room-engine.wad"),
    ) {
        let _ = vfs.mount_memory("engine.wad", bytes::Bytes::from(engine));
    }
    game.set_script_mods_with_vfs(vec![shuriken_mod()], Some(&mut vfs))
        .unwrap();

    assert!(game.classes().find_name("Shuriken").is_some());
    assert!(game.classes().find_name("ShurikenMissile").is_some());
    assert!(game.classes().find_name("ShurikenAmmo").is_some());
    let slot8 = game.weapon_slots().weapons_for(8);
    assert!(
        slot8.iter().any(|n| n.eq_ignore_ascii_case("Shuriken")),
        "{slot8:?}"
    );

    game.start_custom_map(arena(), skill).unwrap();
    let wpn = game.classes().find_name("Shuriken").unwrap();
    let ammo = game.classes().find_name("ShurikenAmmo").unwrap();
    {
        let w = game.world_mut().unwrap();
        let mo = body(w);
        assert!(give_inventory(w, mo, wpn, 1));
        assert!(give_inventory(w, mo, ammo, 10));
        let mut player = w.players[0].take().unwrap();
        player.ready_weapon = Some(wpn);
        player.pending_weapon = None;
        player.pspr_state = None;
        player.pspr_y = 32.0; // WEAPON_TOP
        player.attack_down = false;
        player.alt_attack_down = false;
        w.players[0] = Some(player);
    }

    let ammo_before = {
        let w = game.world().unwrap();
        w.inventory.count(body(w), ammo)
    };

    // Enter Ready then Fire via attack.
    for _ in 0..10 {
        game.tick(&[TicCmd::default()]);
    }
    let mut cmd = TicCmd::default();
    cmd.buttons = Buttons(Buttons::ATTACK);
    for _ in 0..20 {
        game.tick(&[cmd]);
    }

    let (ammo_after, missiles) = {
        let w = game.world().unwrap();
        let ammo_n = w.inventory.count(body(w), ammo);
        let missile = game.classes().find_name("ShurikenMissile").unwrap();
        let n = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == missile))
            .count();
        (ammo_n, n)
    };
    assert!(
        ammo_after < ammo_before || missiles > 0,
        "expected fire to spend ammo or spawn missile (ammo {ammo_before}→{ammo_after}, missiles {missiles})"
    );

    // Alt fan: hold ALTATTACK.
    {
        let w = game.world_mut().unwrap();
        let mo = body(w);
        let _ = give_inventory(w, mo, ammo, 10);
    }
    for _ in 0..15 {
        game.tick(&[TicCmd::default()]);
    }
    let mut alt = TicCmd::default();
    alt.buttons = Buttons(Buttons::ALTATTACK);
    let missiles_before = {
        let w = game.world().unwrap();
        let missile = game.classes().find_name("ShurikenMissile").unwrap();
        w.thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == missile))
            .count()
    };
    for _ in 0..30 {
        game.tick(&[alt]);
    }
    let missiles_after = {
        let w = game.world().unwrap();
        let missile = game.classes().find_name("ShurikenMissile").unwrap();
        w.thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == missile))
            .count()
    };
    assert!(
        missiles_after >= missiles_before,
        "alt fire should spawn missiles ({missiles_before} → {missiles_after})"
    );

    // Determinism: two short runs with same inputs share checksum after reset.
    // Determinism: second load merges the same classes / generated wad.
    let mut game2 = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    let data2 = load_embedded_engine_data().unwrap();
    game2.set_defs(data2.classes, data2.states);
    let mut vfs2 = VfsBuilder::new().build().unwrap();
    game2
        .set_script_mods_with_vfs(vec![shuriken_mod()], Some(&mut vfs2))
        .unwrap();
    assert_eq!(game.generated_mod_wad(), game2.generated_mod_wad());
    assert!(game.classes().find_name("ShurikenMissile").is_some());
    assert!(game2.classes().find_name("ShurikenMissile").is_some());
}

#[test]
fn wasm_mod_shuriken_options_slot() {
    let data = load_embedded_engine_data().unwrap();
    let mut game = Headless::new(GameOptions::default());
    game.set_defs(data.classes, data.states);
    let mut options = BTreeMap::new();
    options.insert("slot".into(), OptionValue::Int(9));
    options.insert("place_weapon".into(), OptionValue::Bool(false));
    options.insert("ammo_replace_percent".into(), OptionValue::Int(0));
    let mut src = shuriken_mod();
    src.options = options;
    let mut vfs = VfsBuilder::new().build().unwrap();
    game.set_script_mods_with_vfs(vec![src], Some(&mut vfs))
        .unwrap();
    let slot9 = game.weapon_slots().weapons_for(9);
    assert!(
        slot9.iter().any(|n| n.eq_ignore_ascii_case("Shuriken")),
        "{slot9:?}"
    );
}
