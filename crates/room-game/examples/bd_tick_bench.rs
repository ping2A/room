//! Simulation cost per tic under Brutal Doom, loaded like the web's "Play
//! solo" (engine + IWAD + pk3 + Room's ZScript port + script mods).
//!
//! `cargo run --release -p room-game --example bd_tick_bench -- <iwad> <pk3> [tics] [map]`
//!
//! The player walks, turns and fires (god mode, all weapons), so monsters
//! wake up, gibs and casings fly: the busy case. Prints ms per tic (mean,
//! median, 95th percentile, worst) and the actor count at the end.

// Dev tool: failing loudly on bad input is the right behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use room_game::{GameOptions, Headless, Skill};
use room_sim::{Buttons, TicCmd};
use room_vfs::{Namespace, VfsBuilder};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (iwad, pk3) = (&a[1], &a[2]);
    let tics: usize = a.get(3).map_or(1400, |s| s.parse().unwrap());
    let map_name = a.get(4).cloned().unwrap_or_else(|| "MAP01".into());
    let mut b = room_game::mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_path(iwad)
        .unwrap()
        .mount_path(pk3)
        .unwrap();
    let port = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../mods/brutal-zscript-rs/brutal-zscript.wad"
    );
    if std::path::Path::new(port).is_file() {
        b = b.mount_path(port).unwrap();
    }
    let mut vfs = b.build().unwrap();
    let mut map = room_map::load_map(&vfs, &map_name).unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let data = room_game::load_layered_engine_data(&vfs).unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        rng_seed: 1,
        ..GameOptions::default()
    });
    let defs = room_defs::DefinitionsBuilder::new(&vfs).build().unwrap();
    let opts = room_defs::mod_options::mod_options(&defs.cvars, &defs.menudef, &defs.language);
    for item in opts.iter().flat_map(|g| &g.items) {
        game.set_mod_cvar(&item.cvar, &item.default);
    }
    game.set_complevel(defs.complevel);
    let handlers = defs.zscript_event_handlers();
    game.set_mapinfo(Arc::new(defs.mapinfo.clone()));
    game.set_terrain(&defs.terrain);
    game.set_decaldef(&defs.decaldef);
    game.set_keyconf(&defs.keyconf);
    let mut libs = Vec::new();
    for name in &defs.loadacs.libraries {
        if let Some(id) = vfs
            .find(name, Namespace::Acs)
            .or_else(|| vfs.find_any_ns(name))
        {
            libs.push(vfs.read(id).unwrap().to_vec());
        }
    }
    if !libs.is_empty() {
        game.set_acs_libraries(libs);
    }
    game.set_acs_library_pool(
        room_game::acs_library_pool(&vfs),
        defs.loadacs.libraries.clone(),
    );
    let zs = room_game::load_zscript_from_vfs(&vfs);
    game.set_zscript(zs.program, handlers);
    game.set_defs(data.classes, data.states);
    game.drop_weapons_without_sprites(&vfs);
    let (mods, _errors) = room_game::discover_mods(&vfs);
    if !mods.is_empty() {
        game.set_script_mods_with_vfs(mods, Some(&mut vfs)).unwrap();
    }
    if let Some(db) = data.compat_db {
        game.set_compat_db(db);
    }
    if let Some(f) = data.level_fixes {
        game.set_level_fixes(f);
    }
    game.start_custom_map(map, skill).unwrap();
    game.cheat_god(0);
    game.cheat_give(0, "all");
    let mut times = Vec::with_capacity(tics);
    for t in 0..tics {
        let mut cmd = TicCmd::default();
        // Walk back and forth, sweep the view, fire most of the time.
        cmd.forward_move = if (t / 70) % 2 == 0 { 6400 } else { -6400 };
        cmd.yaw = if (t / 35) % 4 < 2 { 300 } else { -300 };
        if t % 50 < 40 {
            cmd.buttons = Buttons(Buttons::ATTACK);
        }
        if t % 210 == 0 {
            // Next weapon now and then.
            cmd.buttons = Buttons(cmd.buttons.0 | Buttons::WEAPONNEXT);
        }
        let s = Instant::now();
        game.tick(&[cmd]);
        times.push(s.elapsed().as_secs_f64() * 1000.0);
        // `BENCH_CHECKSUMS=1`: the world checksum every 100 tics, to show a
        // speed-up left the simulation unchanged.
        if t % 100 == 99 && std::env::var_os("BENCH_CHECKSUMS").is_some() {
            println!(
                "tic {} checksum {:016x}",
                t + 1,
                game.world().map_or(0, |w| w.checksum())
            );
        }
    }
    let actors = game.world().map_or(0, |w| w.actors.len());
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let pct = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize];
    // Split: first 10 s vs the rest (debris piles up).
    let early = times[..times.len().min(350)].iter().sum::<f64>() / times.len().min(350) as f64;
    if std::env::var_os("SPRITE_BENCH").is_some() {
        sprite_bench(&game, &vfs);
    }
    println!(
        "{map_name}: {tics} tics | ms/tic mean {mean:.3} median {:.3} p95 {:.3} max {:.3} | first 10 s {early:.3} | actors at end {actors}",
        pct(0.5),
        pct(0.95),
        pct(1.0)
    );
}

/// The web's per-frame `collect_live_sprites` work (`room-web` `play.rs`)
/// over this world, minus voxels and models: ms per frame, split by stage.
fn sprite_bench(game: &Headless, vfs: &room_vfs::Vfs) {
    let w = game.world().unwrap();
    let mut patches = room_ui::PatchCache::from_vfs(vfs);
    let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
    let eye = w.actors.get(mo).unwrap().pos + room_core::DVec3::new(0.0, 0.0, 41.0);
    let frames = 200;
    let (mut t_view, mut t_lump, mut kept, mut drawn) = (0.0, 0.0, 0usize, 0usize);
    for _ in 0..frames {
        let s = Instant::now();
        let mut list = Vec::new();
        for &id in &w.thinkers.default {
            let Some(a) = w.actors.get(id) else { continue };
            let class_flags = w.classes.get(a.class).map_or(0, |c| c.flags);
            if a.destroyed || id == mo || ((a.flags | class_flags) & room_sim::FLAG_DONTDRAW) != 0 {
                continue;
            }
            let Some(view) = room_sim::sprite_view(w, a, eye, 0) else {
                continue;
            };
            let light = room_sim::actor_light_level(w, a);
            list.push((
                a.sprite.clone(),
                a.frame,
                a.pos,
                a.angle.degrees(),
                view,
                light,
            ));
        }
        t_view += s.elapsed().as_secs_f64();
        kept += list.len();
        let s = Instant::now();
        for (sprite, frame, pos, angle, view, _) in &list {
            if sprite.is_empty() {
                continue;
            }
            let rot_angle = if view.fixed_rotation {
                view.rotation_angle_deg
            } else {
                angle + view.rotation_offset_deg
            };
            let rel = ((eye.y() - pos.y()).atan2(eye.x() - pos.x()).to_degrees() - rot_angle)
                .rem_euclid(360.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let rot = ((((rel + 22.5) / 45.0).floor() as i32).rem_euclid(8) as u8) + 1;
            if sprite.eq_ignore_ascii_case("TNT1") {
                continue;
            }
            let hit = patches.sprite_frame_lump(vfs, sprite, *frame, rot);
            let guesses = std::iter::once(()).flat_map(|()| lump_candidates(sprite, *frame, rot));
            for (name, _) in hit.into_iter().chain(guesses) {
                if patches.ensure(vfs, &name).is_some() {
                    drawn += 1;
                    break;
                }
            }
        }
        t_lump += s.elapsed().as_secs_f64();
    }
    let per = |t: f64| t * 1000.0 / f64::from(frames);
    println!(
        "sprites: {} actors listed, {} drawn per frame | ms/frame: view+light {:.3}, lump lookup {:.3}",
        kept / frames as usize,
        drawn / frames as usize,
        per(t_view),
        per(t_lump)
    );
}

/// `room_render::sprite_lump_candidates_rot` (copied: no room-render here).
fn lump_candidates(sprite: &str, frame: u8, rot: u8) -> Vec<(String, bool)> {
    let s = sprite.trim().to_ascii_uppercase();
    let letter = match frame {
        0..=25 => char::from(b'A' + frame),
        26 => '[',
        27 => '\\',
        28 => ']',
        _ => 'A',
    };
    let mirror = match rot {
        2 => Some(8),
        3 => Some(7),
        4 => Some(6),
        6 => Some(4),
        7 => Some(3),
        8 => Some(2),
        _ => None,
    };
    let mut out = vec![(format!("{s}{letter}{rot}"), false)];
    if let Some(m) = mirror {
        out.push((format!("{s}{letter}{m}"), true));
        out.push((format!("{s}{letter}{rot}{letter}{m}"), false));
        out.push((format!("{s}{letter}{m}{letter}{rot}"), true));
    }
    out.push((format!("{s}{letter}0"), false));
    out.push((format!("{s}{letter}1"), false));
    out
}
