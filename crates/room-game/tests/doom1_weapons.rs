//! Doom 1 IWAD: weapons without sprites (super shotgun) are never selected.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bytes::Bytes;
use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data, mount_embedded_engine};
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_sim::{Buttons, TicCmd};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

#[test]
fn doom1_give_all_then_every_slot_shows_and_fires() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    let Ok(iwad) = std::fs::read(&p) else {
        return;
    };
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("doom1.wad", Bytes::from(iwad))
        .unwrap()
        .build()
        .unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let data = load_embedded_engine_data().unwrap();
    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        ..GameOptions::default()
    });
    game.set_defs(data.classes, data.states);
    game.drop_weapons_without_sprites(&vfs);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    game.cheat_give(0, "all");

    let expected = [
        (3u8, "Shotgun", "SHTG"),
        (4, "Chaingun", "CHGG"),
        (5, "RocketLauncher", "MISG"),
        (6, "PlasmaRifle", "PLSG"),
        (7, "BFG9000", "BFGG"),
    ];
    for (slot, class, sprite) in expected {
        game.tick(&[TicCmd {
            weapon_select: slot,
            ..TicCmd::default()
        }]);
        for _ in 0..30 {
            game.tick(&[TicCmd::default()]);
        }
        let ammo_before = {
            let w = game.world().unwrap();
            let pl = w.players[0].as_ref().unwrap();
            let ready = pl.ready_weapon.and_then(|c| w.classes.get(c)).unwrap();
            assert_eq!(ready.name, class, "slot {slot}");
            let hud = w.player_hud(0).unwrap();
            assert!(hud.weapon_patch.starts_with(sprite), "{}", hud.weapon_patch);
            assert!(
                vfs.find_any_ns(&hud.weapon_patch).is_some(),
                "{} exists",
                hud.weapon_patch
            );
            hud.ammo
        };
        for _ in 0..40 {
            game.tick(&[TicCmd {
                buttons: Buttons(Buttons::ATTACK),
                ..TicCmd::default()
            }]);
        }
        let hud = game.world().unwrap().player_hud(0).unwrap();
        assert!(
            hud.ammo < ammo_before,
            "{class} fired ({ammo_before} → {})",
            hud.ammo
        );
    }
}
