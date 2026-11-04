//! Run a map idle and report when the level ends or the player dies.
use room_game::{GameOptions, GameState, Headless, Skill};
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
    if let Some(w) = game.world() {
        let pm = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        println!("t=-1 angle {:?}", w.actors.get(pm).unwrap().angle);
    }
    let walk = a.get(5).map_or(false, |s| s == "walk");
    let mut seen = std::collections::HashSet::new();
    for t in 0..tics {
        if walk {
            let mut c = room_sim::TicCmd::default();
            if std::env::var("FIREONCE").is_ok() {
                if t == 60 {
                    c.buttons.0 |= room_sim::Buttons::ATTACK;
                }
            } else if std::env::var("CYCLE").is_ok() {
                if t % 20 == 0 {
                    c.buttons.0 |= if (t / 200) % 2 == 0 {
                        room_sim::Buttons::WEAPONNEXT
                    } else {
                        room_sim::Buttons::WEAPONPREV
                    };
                }
                if t % 20 == 10 {
                    c.weapon_select = ((t / 20) % 7 + 1) as u8;
                }
            } else if std::env::var("LOOK").is_ok() {
                let k: i16 = std::env::var("LOOK").unwrap().parse().unwrap_or(600);
                c.yaw = if (t / 70) % 2 == 0 { k } else { -k };
                c.pitch = if (t / 35) % 2 == 0 { k } else { -k };
            } else {
                c.forward_move = if (t / 140) % 2 == 0 { 50 } else { -50 };
                if t % 140 < 20 {
                    c.yaw = 2000;
                }
                if t % 35 == 0 {
                    c.buttons.0 |= room_sim::Buttons::USE;
                }
                if t % 10 == 0 {
                    c.buttons.0 |= room_sim::Buttons::ATTACK;
                }
            }
            game.tick(&[c]);
        } else {
            game.run_tics(1);
        }
        let st = game.state;
        let (hp, ex) = game.world().map_or((-1, false), |w| {
            let hp = w.players[0]
                .as_ref()
                .and_then(|p| p.mo)
                .and_then(|m| w.actors.get(m))
                .map_or(-1, |a| a.health);
            (hp, w.level.exiting)
        });
        if let Some(w) = game.world_mut() {
            if t == 1 || t == 100 {
                let names: Vec<_> = w
                    .acs
                    .modules
                    .iter()
                    .map(|m| (m.name.clone(), m.scripts.len(), m.functions.len()))
                    .collect();
                println!(
                    "t={t} acs modules {names:?} running={} {:?}",
                    w.acs.scripts.len(),
                    w.acs
                        .scripts
                        .iter()
                        .map(|s| (s.number, s.module, format!("{:?}", s.state)))
                        .collect::<Vec<_>>()
                );
            }
            for m in &w.hud_messages {
                if m.tics <= 0 {
                    println!(
                        "t={t} hud id={} kind={} x={} y={} hud={}x{} font={:?} hold={} text={:?}",
                        m.id, m.kind, m.x, m.y, m.hud_w, m.hud_h, m.font, m.hold_tics, m.text
                    );
                }
            }
            for m in w.drain_messages() {
                println!("t={t} msg {m}");
            }
            for m in w.drain_debug() {
                if m.contains("xit") || m.contains("ACS") || m.contains("Script") {
                    println!("t={t} dbg {m}");
                }
            }
        }
        if t == 5 && std::env::var("KILLALL").is_ok() {
            if let Some(w) = game.world_mut() {
                let ids: Vec<_> = w
                    .thinkers
                    .default
                    .iter()
                    .copied()
                    .filter(|&id| {
                        w.actors.get(id).is_some_and(|a| {
                            w.classes.get(a.class).is_some_and(|c| {
                                c.name.to_ascii_lowercase().contains("zomb")
                                    || c.name.to_ascii_lowercase().contains("gun")
                                    || c.name.to_ascii_lowercase().contains("poss")
                            }) && a.health > 0
                        })
                    })
                    .collect();
                eprintln!("killing {}", ids.len());
                for id in ids {
                    room_sim::damage_mobj(w, id, None, None, 10000);
                }
            }
        }
        if std::env::var("SPAWNS").is_ok() {
            if let Some(w) = game.world() {
                for &id in w.thinkers.default.iter() {
                    if seen.insert(id) && t >= 55 {
                        if let Some(a) = w.actors.get(id) {
                            let n = w
                                .classes
                                .get(a.class)
                                .map_or("?".into(), |c| c.name.clone());
                            println!(
                                "t={t} new {n} pos=({:.0},{:.0},{:.0}) vel=({:.1},{:.1},{:.1})",
                                a.pos.x(),
                                a.pos.y(),
                                a.pos.z(),
                                a.vel.x(),
                                a.vel.y(),
                                a.vel.z()
                            );
                        }
                    }
                }
            }
        }
        if std::env::var("NEAR").is_ok() && (t == 0 || t == 1 || t == 100) {
            if let Some(w) = game.world() {
                let pm = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
                let pp = w.actors.get(pm).unwrap().pos;
                println!(
                    "player pos {pp:?} angle {:?}",
                    w.actors.get(pm).unwrap().angle
                );
                for &id in w.thinkers.default.iter() {
                    if let Some(a) = w.actors.get(id) {
                        let d = (a.pos - pp).length();
                        if d < 400.0 {
                            let n = w
                                .classes
                                .get(a.class)
                                .map_or("?".into(), |c| c.name.clone());
                            println!(
                                "  near {n} d={d:.0} pos={:?} rs={:?} alpha={} r={} h={}",
                                a.pos, a.render_style, a.alpha, a.radius, a.height
                            );
                        }
                    }
                }
            }
        }
        if st != GameState::Level || ex || hp <= 0 || t % 50 == 0 {
            let pos = game.world().and_then(|w| {
                w.players[0]
                    .as_ref()
                    .and_then(|p| p.mo)
                    .and_then(|m| w.actors.get(m))
                    .map(|a| a.pos)
            });
            println!("t={t} state={st:?} exiting={ex} hp={hp} pos={pos:?}");
            if st != GameState::Level {
                break;
            }
        }
    }
}
