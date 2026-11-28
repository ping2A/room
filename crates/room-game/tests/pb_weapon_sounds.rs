//! Project Brutality's DMR (`actors/Weapons/Slot4/PBRIFLE.dec`) fires from
//! DECORATE action blocks: `R0F8 A 1 BRIGHT { if (CountInv("DMRUpgraded") == 1)
//! { … A_StartSound("weapons/hdmr/fire", CHAN_WEAPON) … } else { …
//! A_StartSound("weapons/rifle", CHAN_WEAPON) … } … }`. The whole block runs
//! when the state is entered, as UZDoom calls the anonymous function. Room
//! used to run everything after the first action a tic late, where the
//! player's other actions used up the `if`'s skip count and the shot's sound
//! was lost (heard only when zoomed, whose block starts differently).
//!
//! Needs `testdata/Project_Brutality-master` and Freedoom; skipped otherwise.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill};
use room_sim::{Buttons, TicCmd};
use room_vfs::VfsBuilder;
use std::path::PathBuf;
use std::sync::Arc;

fn data(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(rel)
}

fn pb_game() -> Option<Headless> {
    let pb = data("Project_Brutality-master");
    let iwad = data("freedoom/freedoom2.wad");
    if !pb.join("zscript").is_dir() || !iwad.is_file() {
        eprintln!(
            "skip: needs testdata/Project_Brutality-master and testdata/freedoom/freedoom2.wad"
        );
        return None;
    }
    let vfs = room_game::mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_path(&iwad)
        .unwrap()
        .mount_path(&pb)
        .unwrap()
        .build()
        .unwrap();
    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        ..GameOptions::default()
    });
    let defs = room_defs::DefinitionsBuilder::new(&vfs).build().unwrap();
    let handlers = defs.zscript_event_handlers();
    game.set_complevel(defs.complevel);
    game.set_terrain(&defs.terrain);
    game.set_lockdefs(&defs.lockdefs, Some("Doom"));
    game.set_keyconf(&defs.keyconf);
    game.set_mapinfo(Arc::new(defs.mapinfo));
    let zs = room_game::load_zscript_from_vfs(&vfs);
    game.set_zscript_uzdoom(zs.uzdoom_library);
    game.set_zscript_lumps(room_game::zscript_wad_lumps(&vfs, zs.program.as_ref()));
    game.set_zscript(zs.program, handlers);
    let loaded = room_game::load_embedded_engine_data_with(&vfs).unwrap();
    game.set_defs(loaded.classes, loaded.states);
    game.drop_weapons_without_sprites(&vfs);
    let mut map = room_map::load_map(&vfs, "MAP01").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    Some(game)
}

/// Tics (from the first held tic) at which the player's gun started
/// `sound` on the weapon channel, firing for `fire` tics after `before`.
fn shot_sounds(game: &mut Headless, before: u32, zoom: bool, fire: u32, sound: &str) -> Vec<u32> {
    let mut hits = Vec::new();
    for t in 0..before + fire {
        let mut c = TicCmd::default();
        if zoom && (before - 20..before - 18).contains(&t) {
            c.buttons.0 |= Buttons::ALTATTACK;
        }
        if t >= before {
            c.buttons.0 |= Buttons::ATTACK;
        }
        game.tick(&[c]);
        let w = game.world_mut().unwrap();
        for e in w.drain_sound_events() {
            if e.name.eq_ignore_ascii_case(sound)
                && e.channel == 1
                && e.adjust == room_sim::SimSoundAdjust::Start
                && t >= before
            {
                hits.push(t - before);
            }
        }
    }
    hits
}

#[test]
fn pb_dmr_shots_are_heard_hip_fired_and_zoomed() {
    let Some(mut game) = pb_game() else { return };
    let hip = shot_sounds(&mut game, 60, false, 16, "weapons/rifle");
    // The first shot sounds on the tic the trigger is pulled; holding it
    // keeps firing (5-tic cycle) and every shot is heard.
    assert_eq!(hip.first(), Some(&0), "hip fire: {hip:?}");
    assert!(hip.len() >= 3, "hip fire: {hip:?}");
    assert!(
        hip.windows(2).all(|w| w[1] - w[0] == 5),
        "hip fire: {hip:?}"
    );

    let Some(mut game) = pb_game() else { return };
    let zoomed = shot_sounds(&mut game, 60, true, 16, "weapons/rifle");
    assert_eq!(zoomed.first(), Some(&0), "zoomed: {zoomed:?}");
    assert!(zoomed.len() >= 3, "zoomed: {zoomed:?}");
    // The upgraded gun's sound never plays without the upgrade.
    let Some(mut game) = pb_game() else { return };
    assert!(shot_sounds(&mut game, 60, false, 16, "weapons/hdmr/fire").is_empty());
}
