//! Battle Royale + Brutal Doom, built the way the web client builds a game
//! (`room-web` `PlaySession::from_parts`): bots must be able to hurt a human
//! seat. `ROOM_BD_IWAD=…/freedoom2.wad ROOM_BRUTALDOOM_PK3=…/brutal.pk3
//! cargo test -p room-game --test br_brutal_bots -- --ignored --nocapture`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bytes::Bytes;
use room_defs::DefinitionsBuilder;
use room_game::{GameOptions, Headless, Skill, discover_mods, mount_embedded_engine};
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_sim::TicCmd;
use room_vfs::VfsBuilder;
use std::sync::Arc;

const ROYALE_WAD: &[u8] = include_bytes!("../../../mods/doom1-royale/royale.wad");

fn game(seats: u8) -> Option<Headless> {
    let iwad = std::fs::read(std::env::var("ROOM_BD_IWAD").ok()?).ok()?;
    let pk3 = std::fs::read(std::env::var("ROOM_BRUTALDOOM_PK3").ok()?).ok()?;
    let mut vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("doom2.wad", Bytes::from(iwad))
        .unwrap()
        .mount_memory("brutal.pk3", Bytes::from(pk3))
        .unwrap()
        .mount_memory("royale.wad", Bytes::from_static(ROYALE_WAD))
        .unwrap()
        .build()
        .unwrap();
    let mut map = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map);
    let data = room_game::load_layered_engine_data(&vfs).unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        coop_players: seats,
        fill_bots: true,
        bot_first_seat: 1,
        ..GameOptions::default()
    });
    let mut handlers = Vec::new();
    if let Ok(defs) = DefinitionsBuilder::new(&vfs).build() {
        handlers = defs.zscript_event_handlers();
        game.set_complevel(defs.complevel);
        game.set_mapinfo(Arc::new(defs.mapinfo));
        game.set_lockdefs(&defs.lockdefs, Some("Doom"));
        game.set_decaldef(&defs.decaldef);
        game.set_keyconf(&defs.keyconf);
        game.set_acs_library_pool(
            room_game::acs_library_pool(&vfs),
            defs.loadacs.libraries.clone(),
        );
    }
    let zs = room_game::load_zscript_from_vfs(&vfs);
    game.set_zscript(zs.program, handlers);
    game.set_defs(data.classes, data.states);
    game.drop_weapons_without_sprites(&vfs);
    let (mods, errors) = discover_mods(&vfs);
    assert!(errors.is_empty(), "{errors:?}");
    game.set_script_mods_with_vfs(mods, Some(&mut vfs)).unwrap();
    game.start_custom_map(map, skill).unwrap();
    Some(game)
}

#[test]
#[ignore = "needs ROOM_BD_IWAD + ROOM_BRUTALDOOM_PK3"]
fn bots_hurt_a_human_with_brutal_doom() {
    let Some(mut game) = game(4) else {
        eprintln!("skip: set ROOM_BD_IWAD and ROOM_BRUTALDOOM_PK3");
        return;
    };
    let idle = [TicCmd::default(); 4];
    for sec in 0..40 {
        // Once PvP is on, park the human 160 units in front of bot 1.
        // Brutal Doom's guns have no vertical autoaim: the bot has to aim
        // its pitch, or every shot flies level over or under the target.
        if sec == 25 {
            let w = game.world_mut().unwrap();
            let bot = w.players[1].as_ref().unwrap().mo.unwrap();
            let (bp, ang) = {
                let a = w.actors.get(bot).unwrap();
                (a.pos, a.angle.radians())
            };
            let me = w.players[0].as_ref().unwrap().mo.unwrap();
            w.actors.get_mut(me).unwrap().pos = room_core::DVec3::new(
                bp.x() + ang.cos() * 160.0,
                bp.y() + ang.sin() * 160.0,
                bp.z(),
            );
        }
        for _ in 0..35 {
            game.tick(&idle);
        }
    }
    let w = game.world().unwrap();
    assert!(w.deathmatch, "PvP on after the loot phase starts");
    assert!(
        w.player_hits[0][1] > 0,
        "bots never hurt the human seat: {:?}",
        &w.player_hits[..4]
    );
}
