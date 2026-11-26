//! Fire a weapon slot and print the player's view pitch / angle and quake
//! intensity every tic (recoil and camera-shake check).

// Dev tool: failing loudly on bad input is the right behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use room_game::{GameOptions, Headless, Skill};
use room_vfs::VfsBuilder;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let map_name = a.get(3).cloned().unwrap_or_else(|| "MAP01".into());
    let slot: u8 = a.get(4).map_or(3, |s| s.parse().unwrap());
    let tics: u32 = a.get(5).map_or(120, |s| s.parse().unwrap());
    let vfs = room_game::mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_path(&a[1])
        .unwrap()
        .mount_path(&a[2])
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
    game.set_keyconf(&defs.keyconf);
    game.set_menudef(defs.menudef.clone());
    let mut libs = Vec::new();
    for name in &defs.loadacs.libraries {
        if let Some(id) = vfs
            .find(name, room_vfs::Namespace::Acs)
            .or_else(|| vfs.find_any_ns(name))
        {
            if let Ok(b) = vfs.read(id) {
                libs.push(b.to_vec());
            }
        }
    }
    if !libs.is_empty() {
        game.set_acs_libraries(libs);
        game.set_acs_library_pool(
            room_game::acs_library_pool(&vfs),
            defs.loadacs.libraries.clone(),
        );
    }
    game.set_mapinfo(Arc::new(defs.mapinfo));
    let zs = room_game::load_zscript_from_vfs(&vfs);
    game.set_zscript(zs.program, handlers);
    let data = room_game::load_embedded_engine_data_with(&vfs).unwrap();
    game.set_defs(data.classes, data.states);
    let mut map = room_map::load_map(&vfs, &map_name).unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    let _ = game.cheat_god(0);
    if let Ok(pp) = std::env::var("PPOS") {
        let v: Vec<f64> = pp.split(',').map(|x| x.parse().unwrap()).collect();
        let w = game.world_mut().unwrap();
        w.set_player_pose(
            v[0],
            v[1],
            v.get(2).copied().unwrap_or(0.0),
            v.get(3).copied().unwrap_or(0.0),
            0.0,
        );
    }
    let use_at = std::env::var("USEAT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(u32::MAX);
    let _ = game.cheat_give(0, "all");
    let mut seen = std::collections::HashSet::new();
    let mut seen_after = std::collections::HashSet::new();
    let mut spawned: std::collections::BTreeMap<String, u32> = Default::default();
    for t in 0..tics {
        let mut c = room_sim::TicCmd::default();
        if t == 2 {
            c.weapon_select = slot;
        }
        let stop = std::env::var("STOP")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(u32::MAX);
        if (t >= 40 && std::env::var("HOLD").is_ok() && t < stop) || t == 40 {
            c.buttons.0 |= room_sim::Buttons::ATTACK;
        }
        if t == use_at || t == use_at + 1 {
            c.buttons.0 |= room_sim::Buttons::USE;
        }
        game.tick(&[c]);
        let w = game.world().unwrap();
        for &id in &w.thinkers.default {
            if seen.insert(id) && t >= 40 {
                seen_after.insert(id);
                if let Some(a) = w.actors.get(id) {
                    let n = w
                        .classes
                        .get(a.class)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    *spawned.entry(n).or_default() += 1;
                }
            }
        }
        let p = w.players[0].as_ref().unwrap();
        let mo = p.mo.unwrap();
        let act = w.actors.get(mo).unwrap();
        let wpn = p
            .ready_weapon
            .and_then(|c| w.classes.get(c))
            .map(|c| c.name.clone());
        println!(
            "t={t} wpn={wpn:?} pitch={:.2} cam_pitch={:.2} angle={:.2} quake={:.1} zoom={:.3} view_z={:.1}",
            act.pitch.degrees(),
            act.camera_pitch().degrees(),
            act.angle.degrees(),
            w.quake_intensity_at(mo),
            p.weapon_zoom,
            p.view_z,
        );
    }
    let mut v: Vec<_> = spawned.into_iter().collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1));
    println!("SPAWNED {:?}", &v[..v.len().min(12)]);
    let w = game.world().unwrap();
    let mut alive: std::collections::BTreeMap<String, (u32, String)> = Default::default();
    for &id in &w.thinkers.default {
        if let Some(a) = w.actors.get(id) {
            if a.destroyed || a.sprite == "TNT1" || !seen_after.contains(&id) {
                continue;
            }
            let n = w
                .classes
                .get(a.class)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let e = alive.entry(n).or_insert((0, String::new()));
            e.0 += 1;
            e.1 = format!("{}{} tics={} pos={:?}", a.sprite, a.frame, a.tics, a.pos);
        }
    }
    println!("LINGERING {alive:#?}");
    if let Ok(b) = std::env::var("BOX") {
        let v: Vec<f64> = b.split(',').map(|x| x.parse().unwrap()).collect();
        for &id in &w.thinkers.default {
            let Some(a) = w.actors.get(id) else { continue };
            if a.destroyed || a.sprite == "TNT1" {
                continue;
            }
            if a.pos.x() >= v[0] && a.pos.x() <= v[2] && a.pos.y() >= v[1] && a.pos.y() <= v[3] {
                let n = w
                    .classes
                    .get(a.class)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                println!(
                    "INBOX {n} {}{} tics={} pos={:?} style={:?} alpha={} flags={:#x}",
                    a.sprite, a.frame, a.tics, a.pos, a.render_style, a.alpha, a.flags
                );
            }
        }
    }
}
