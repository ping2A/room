//! Random-walk the player through a map (god mode) and report any tic where
//! the player ends up below the floor or outside every sector.
use room_game::{GameOptions, Headless, Skill};
use room_vfs::VfsBuilder;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let map_name = a.get(3).cloned().unwrap_or_else(|| "MAP01".into());
    let tics: u32 = a.get(4).map_or(300, |s| s.parse().unwrap());
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
    game.set_terrain(&defs.terrain);
    game.set_sndseq(&defs.sndseq);
    game.set_lockdefs(&defs.lockdefs, Some("Doom"));
    game.set_decaldef(&defs.decaldef);
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
    eprintln!("LOADACS libs: {}", libs.len());
    if !libs.is_empty() && std::env::var("NOACS").is_err() {
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
    eprintln!(
        "map {map_name}: things={} lines={} behavior={}",
        map.things.len(),
        map.lines.len(),
        map.behavior.as_ref().map_or(0, |b| b.len())
    );
    game.start_custom_map(map, Skill::new(3)).unwrap();
    let _ = game.cheat_god(0);
    let mut rng: u32 = std::env::var("SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12345);
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng
    };
    let mut last = room_core::DVec3::ZERO;
    let mut stuck = 0;
    let mut turning = 0i32;
    let mut reports = 0;
    let watch = std::env::var("WATCH")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut tracked: std::collections::HashMap<u32, (u32, room_core::DVec3, bool)> =
        Default::default();
    for t in 0..tics {
        let mut c = room_sim::TicCmd::default();
        if turning > 0 {
            c.yaw = 1500;
            turning -= 1;
        } else {
            c.forward_move = 50 * 256;
            if next() % 7 == 0 {
                c.side_move = if next() % 2 == 0 { 40 * 256 } else { -40 * 256 };
            }
        }
        if t % 20 == 0 {
            c.buttons.0 |= room_sim::Buttons::USE;
        }
        game.tick(&[c]);
        if std::env::var("SOUNDS").is_ok() {
            if let Some(w) = game.world_mut() {
                for e in w.drain_sound_events() {
                    println!("t={t} SND {:?}", e);
                }
            }
        }
        if let Some(w) = game.world_mut() {
            for m in w.drain_messages() {
                println!("t={t} MSG {m}");
            }
        }
        let Some(w) = game.world() else { break };
        if !watch.is_empty() {
            let mut alive = std::collections::HashSet::new();
            for &id in w.thinkers.default.iter() {
                let Some(a) = w.actors.get(id) else { continue };
                let n = w
                    .classes
                    .get(a.class)
                    .map(|c| c.name.to_ascii_lowercase())
                    .unwrap_or_default();
                if !watch.split(',').any(|x| x == n) {
                    continue;
                }
                alive.insert(id.index());
                let moving = a.vel.length() > 0.01;
                let e = tracked.entry(id.index()).or_insert((t, a.pos, true));
                if e.2 && !moving {
                    e.2 = false;
                    let tgt = a
                        .hit_actor
                        .and_then(|h| w.actors.get(h))
                        .and_then(|h| w.classes.get(h.class))
                        .map(|c| c.name.clone());
                    let shooter = a
                        .target
                        .and_then(|h| w.actors.get(h))
                        .and_then(|h| w.classes.get(h.class))
                        .map(|c| c.name.clone());
                    println!(
                        "t={t} FB {n}#{} stopped after {} tics, dist {:.0}, hit kind {} {tgt:?}#{:?} shooter {shooter:?}#{:?} at {:?}",
                        id.index(),
                        t - e.0,
                        (a.pos - e.1).length(),
                        a.hit_kind,
                        a.hit_actor.map(|h| h.index()),
                        a.target.map(|h| h.index()),
                        a.pos
                    );
                }
            }
        }
        let Some(pm) = w.players[0].as_ref().and_then(|p| p.mo) else {
            break;
        };
        let a = w.actors.get(pm).unwrap();
        if (a.pos - last).length() < 0.5 {
            stuck += 1;
            if stuck > 6 {
                turning = 5 + (next() % 20) as i32;
                stuck = 0;
            }
        } else {
            stuck = 0;
        }
        last = a.pos;
        let sec = room_nodebuild::point_in_sector(&w.map, a.pos.x(), a.pos.y());
        let floor = sec
            .and_then(|s| w.sectors.get(s.raw() as usize))
            .map(|s| s.floor_z);
        let bad_floor = floor.is_some_and(|f| a.pos.z() < f - 2.0);
        if (sec.is_none() || bad_floor) && reports < 20 {
            reports += 1;
            println!(
                "t={t} BAD pos={:?} vel={:?} sector={:?} floor={floor:?}",
                a.pos,
                a.vel,
                sec.map(|s| s.raw())
            );
        }
        if std::env::var("TRACE")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .is_some_and(|v| t + 6 >= v && t <= v + 3)
        {
            println!(
                "t={t} TRACE pos={:?} vel={:?} sec={:?} floors={:?} plats={} crouch={:?} h={}",
                a.pos,
                a.vel,
                sec.map(|s| s.raw()),
                w.floors
                    .iter()
                    .map(|f| (f.sector.raw(), w.sectors[f.sector.raw() as usize].floor_z))
                    .collect::<Vec<_>>(),
                w.plats.len(),
                w.players[0].as_ref().map(|p| p.crouching),
                a.height
            );
            let ck = room_sim::check_position(w, a, a.pos, Some(pm));
            println!(
                "   check ok={} floor={} ceil={} line={:?}",
                ck.ok, ck.floor_z, ck.ceiling_z, ck.blocking_line
            );
            if let Some(sid) = sec {
                let ms = &w.map.sectors[sid.raw() as usize];
                let ls = &w.sectors[sid.raw() as usize];
                println!(
                    "   sector special={} tags={:?} floor={} ceil={} floorpic={:?}",
                    ms.special, ms.tags, ls.floor_z, ls.ceiling_z, ms.floor_pic
                );
            }
        }
        if t % 500 == 0 {
            println!("t={t} pos={:?} sector={:?}", a.pos, sec.map(|s| s.raw()));
        }
    }
}
