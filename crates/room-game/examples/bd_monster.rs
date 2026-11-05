//! Spawn a monster near the player on a Brutal Doom map and trace its missiles.
//! `bd_monster IWAD PK3 MAP CLASS TICS [dx]`
use room_core::{Angle, DVec3};
use room_game::{GameOptions, Headless, Skill};
use room_vfs::VfsBuilder;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let map_name = a.get(3).cloned().unwrap_or_else(|| "MAP01".into());
    let class = a.get(4).cloned().unwrap_or_else(|| "DoomImp".into());
    let tics: u32 = a.get(5).map_or(300, |s| s.parse().unwrap());
    let dx: f64 = a.get(6).map_or(256.0, |s| s.parse().unwrap());
    let watch = std::env::var("WATCH").unwrap_or_default();
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
    game.set_keyconf(&defs.keyconf);
    game.set_acs_library_pool(
        room_game::acs_library_pool(&vfs),
        defs.loadacs.libraries.clone(),
    );
    game.set_mapinfo(Arc::new(defs.mapinfo));
    let zs = room_game::load_zscript_from_vfs(&vfs);
    game.set_zscript(zs.program, handlers);
    let data = room_game::load_embedded_engine_data_with(&vfs).unwrap();
    game.set_defs(data.classes, data.states);
    let mut map = room_map::load_map(&vfs, &map_name).unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    if std::env::var_os("GOD").is_some() {
        let _ = game.cheat_god(0);
    }
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
    let keep = std::env::var_os("KEEP").is_some();
    let (pmo, ppos, pang) = {
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let p = w.actors.get(mo).unwrap();
        (mo, p.pos, p.angle)
    };
    let w = game.world_mut().unwrap();
    // Remove other monsters so nothing else interferes.
    let others: Vec<_> = w
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&i| {
            !keep
                && i != pmo
                && w.actors
                    .get(i)
                    .is_some_and(|a| a.health > 0 && a.flags & (1 << 2) != 0)
        })
        .collect();
    for o in others {
        if let Some(a) = w.actors.get_mut(o) {
            a.pos = DVec3::new(a.pos.x() + 30000.0, a.pos.y(), a.pos.z());
            a.flags = 0;
        }
    }
    let cid = w
        .classes
        .find_name(&class)
        .unwrap_or_else(|| panic!("no class {class}"));
    let cid = w.classes.replacement(cid);
    let r = pang.radians()
        + std::env::var("ANG")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
            .to_radians();
    let pos = DVec3::new(ppos.x() + r.cos() * dx, ppos.y() + r.sin() * dx, ppos.z());
    let id = w
        .spawn_at(cid, pos, Angle::from_degrees(pang.degrees() + 180.0))
        .unwrap();
    if let Some(m) = w.actors.get_mut(id) {
        m.target = Some(pmo);
    }
    let name = w
        .classes
        .get(w.actors.get(id).unwrap().class)
        .unwrap()
        .name
        .clone();
    println!(
        "spawned {name} #{} at {pos:?}; player at {ppos:?}",
        id.index()
    );
    let mut seen = std::collections::HashSet::new();
    let mut last_hp = 100;
    for t in 0..tics {
        game.run_tics(1);
        let w = game.world().unwrap();
        let hp = w.actors.get(pmo).map_or(0, |a| a.health);
        if hp != last_hp {
            println!("t={t} player hp {last_hp} -> {hp}");
            last_hp = hp;
        }
        if let Some(m) = w.actors.get(id) {
            if t % std::env::var("EVERY")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(10)
                == 0
            {
                let st = m
                    .state
                    .and_then(|s| w.states.get(s))
                    .map(|s| format!("{}{}", s.sprite, s.frame))
                    .unwrap_or_default();
                println!(
                    "t={t} monster pos=({:.0},{:.0},{:.0}) state={st} hp={}",
                    m.pos.x(),
                    m.pos.y(),
                    m.pos.z(),
                    m.health
                );
                if std::env::var("PROBE").is_ok() {
                    let tgt = m
                        .target
                        .and_then(|t| w.actors.get(t))
                        .map(|a| w.classes.get(a.class).unwrap().name.clone());
                    println!(
                        "   target={tgt:?} tpos={:?} movedir={} angle={:.0} flags={:#x} speed={} radius={} height={}",
                        m.target.and_then(|t| w.actors.get(t)).map(|t| t.pos),
                        m.move_dir,
                        m.angle.degrees(),
                        m.flags,
                        m.speed,
                        m.radius,
                        m.height
                    );
                }
            }
        }
        if std::env::var("DRAW").is_ok() {
            for &aid in w.thinkers.default.iter() {
                let Some(act) = w.actors.get(aid) else {
                    continue;
                };
                let c = w.classes.get(act.class).unwrap();
                if c.name.eq_ignore_ascii_case("ImpFireBallTrail1") && !act.destroyed {
                    println!(
                        "t={t} DRAW {}#{} sprite={}{} flags={:#x} cflags={:#x} style={:?} alpha={} scale=({},{}) lump={:?}",
                        c.name,
                        aid.index(),
                        act.sprite,
                        act.frame,
                        act.flags,
                        c.flags,
                        act.render_style,
                        act.alpha,
                        act.scale_x,
                        act.scale_y,
                        vfs.find_any_ns(&format!("{}{}0", act.sprite, (b'A' + act.frame) as char))
                    );
                }
            }
        }
        for &aid in w.thinkers.default.iter() {
            let Some(act) = w.actors.get(aid) else {
                continue;
            };
            let n = &w.classes.get(act.class).unwrap().name;
            let first = seen.insert(aid);
            let is_watch = !watch.is_empty() && n.eq_ignore_ascii_case(&watch);
            if (first && act.flags & 0 == 0 && (is_watch || watch.is_empty() && first))
                || (is_watch && !first)
            {
                if watch.is_empty() && !first {
                    continue;
                }
                println!(
                    "t={t} {} {n}#{} pos=({:.0},{:.0},{:.0}) vel=({:.1},{:.1},{:.1}) hp={} state={:?} actor_sprite={}{} destroyed={}",
                    if first { "new" } else { "   " },
                    aid.index(),
                    act.pos.x(),
                    act.pos.y(),
                    act.pos.z(),
                    act.vel.x(),
                    act.vel.y(),
                    act.vel.z(),
                    act.health,
                    act.state
                        .and_then(|s| w.states.get(s))
                        .map(|s| format!("{}{}", s.sprite, s.frame)),
                    act.sprite,
                    act.frame,
                    act.destroyed
                );
                if is_watch && act.hit_kind != 0 {
                    let h = act.hit_actor.and_then(|h| w.actors.get(h)).map(|h| {
                        (
                            w.classes.get(h.class).unwrap().name.clone(),
                            h.pos,
                            h.radius,
                            h.height,
                            h.flags,
                        )
                    });
                    println!("      hit kind={} actor={h:?}", act.hit_kind);
                }
            }
        }
    }
}
