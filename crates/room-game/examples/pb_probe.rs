//! Run a gameplay mod (Project Brutality, …) headless and trace the player's
//! weapon: `pb_probe <iwad> <mod> [map] [tics] [from-to,from-to…]` (fire held).
//!
//! Prints the weapon psprite state whenever it changes, every actor spawned
//! after tic 5, and the actions that were skipped (stderr).

// Dev tool: failing loudly on bad input is the right behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use room_game::{GameOptions, Headless, Skill};
use room_vfs::VfsBuilder;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let map_name = a.get(3).cloned().unwrap_or_else(|| "MAP01".into());
    let tics: u32 = a.get(4).map_or(200, |s| s.parse().unwrap());
    let fire: Vec<(u32, u32)> = a.get(5).map_or_else(Vec::new, |s| {
        s.split(',')
            .map(|r| {
                let (x, y) = r.split_once('-').unwrap();
                (x.parse().unwrap(), y.parse().unwrap())
            })
            .collect()
    });
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
    game.set_lockdefs(&defs.lockdefs, Some("Doom"));
    game.set_keyconf(&defs.keyconf);
    game.set_mapinfo(Arc::new(defs.mapinfo));
    let zs = room_game::load_zscript_from_vfs(&vfs);
    game.set_zscript_uzdoom(zs.uzdoom_library);
    game.set_zscript_lumps(room_game::zscript_wad_lumps(&vfs, zs.program.as_ref()));
    game.set_zscript(zs.program, handlers);
    let data = room_game::load_embedded_engine_data_with(&vfs).unwrap();
    game.set_defs(data.classes, data.states);
    game.drop_weapons_without_sprites(&vfs);
    let mut map = room_map::load_map(&vfs, &map_name).unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    if let (Ok(label), Some(w)) = (std::env::var("DUMPLABEL"), game.world()) {
        let wpn = w.players[0].as_ref().and_then(|p| p.ready_weapon).unwrap();
        if let Some(mut sid) = w.states.find_label(wpn, &label) {
            for _ in 0..40 {
                let Some(st) = w.states.get(sid) else { break };
                println!(
                    "{label} {} {}{} {} {:?} {:?} next={:?}",
                    sid.raw(),
                    st.sprite,
                    char::from(b'A' + st.frame),
                    st.tics,
                    st.action,
                    st.action_args,
                    st.next.map(|n| n.raw())
                );
                match st.next {
                    Some(n) if n.raw() == sid.raw() + 1 => sid = n,
                    _ => break,
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut last = String::new();
    for t in 0..tics {
        let mut c = room_sim::TicCmd::default();
        if fire.iter().any(|&(x, y)| (x..=y).contains(&t)) {
            c.buttons.0 |= room_sim::Buttons::ATTACK;
        }
        // `MOVE=a-b:fwd:side:yaw,…` (`JUMP=a-b`): movement input, and the
        // pawn's position and speed each tic it is held.
        let ranged = |var: &str| -> Vec<(u32, u32, Vec<i16>)> {
            std::env::var(var)
                .unwrap_or_default()
                .split(',')
                .filter_map(|r| {
                    let mut parts = r.split(':');
                    let (a, b) = parts.next()?.split_once('-')?;
                    let rest = parts.filter_map(|v| v.parse().ok()).collect();
                    Some((a.parse().ok()?, b.parse().ok()?, rest))
                })
                .collect()
        };
        let moving = ranged("MOVE");
        for (a, b, v) in &moving {
            if (*a..=*b).contains(&t) {
                c.forward_move = v.first().copied().unwrap_or(0);
                c.side_move = v.get(1).copied().unwrap_or(0);
                c.yaw = v.get(2).copied().unwrap_or(0);
            }
        }
        if ranged("JUMP")
            .iter()
            .any(|(a, b, _)| (*a..=*b).contains(&t))
        {
            c.buttons.0 |= room_sim::Buttons::JUMP;
        }
        // `ALT=a-b,…`: alt-fire held (PB's aim-down-sights / zoom).
        if ranged("ALT").iter().any(|(a, b, _)| (*a..=*b).contains(&t)) {
            c.buttons.0 |= room_sim::Buttons::ALTATTACK;
        }
        if (!moving.is_empty() || std::env::var_os("JUMP").is_some())
            && let Some(w) = game.world()
            && let Some(mo) = w.players[0].as_ref().and_then(|p| p.mo)
            && let Some(a) = w.actors.get(mo)
        {
            let p = w.players[0].as_ref().unwrap();
            println!(
                "t={t} pos=({:.2},{:.2},{:.2}) vel=({:.2},{:.2},{:.2}) speed={:.2} angle={:.1} viewz={:.2} ground={}",
                a.pos.x(),
                a.pos.y(),
                a.pos.z(),
                a.vel.x(),
                a.vel.y(),
                a.vel.z(),
                a.vel.x().hypot(a.vel.y()),
                a.angle.degrees(),
                p.view_z,
                p.on_ground
            );
        }
        if t == 5 {
            if let (Ok(name), Some(w)) = (std::env::var("TARGET"), game.world_mut()) {
                let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
                let (pos, ang) = w.actors.get(mo).map(|a| (a.pos, a.angle)).unwrap();
                let dir = room_core::math::DVec3::new(
                    room_core::dmath::cos(ang.radians()) * 160.0,
                    room_core::dmath::sin(ang.radians()) * 160.0,
                    0.0,
                );
                let cid = w.classes.find_name(&name).expect("TARGET class");
                let id = w.spawn_at(cid, pos + dir, ang).expect("spawn");
                let a = w.actors.get(id).unwrap();
                println!(
                    "t={t} target {name} #{} at {:?} flags={:#x} r={} h={}",
                    id.index(),
                    pos + dir,
                    a.flags,
                    a.radius,
                    a.height
                );
            }
        }
        game.tick(&[c]);
        // `SOUNDS=1`: every sound the sim starts or changes this tic.
        if std::env::var_os("SOUNDS").is_some()
            && let Some(w) = game.world_mut()
        {
            for e in w.drain_sound_events() {
                println!(
                    "t={t} sound {:?} ch={} flags={:#x} vol={:.2} att={:.2} {:?} src={:?} limit_check={:?}",
                    e.name,
                    e.channel,
                    e.channel_flags,
                    e.volume,
                    e.attenuation,
                    e.adjust,
                    e.source_id,
                    e.check_name
                );
            }
        }
        let Some(w) = game.world() else { break };
        if let Some(p) = w.players[0].as_ref() {
            let wpn = p
                .ready_weapon
                .and_then(|c| w.classes.get(c))
                .map_or("-".into(), |c| c.name.clone());
            let st = p
                .pspr_state
                .and_then(|s| w.states.get(s))
                .map_or("-".into(), |s| {
                    format!(
                        "{}{} {} {:?}",
                        s.sprite,
                        char::from(b'A' + s.frame),
                        s.tics,
                        s.action
                    )
                });
            let line = format!("{wpn} {st}");
            if line != last {
                println!("t={t} psp {line}");
                last = line;
            }
        }
        if let (Ok(items), Some(p)) = (std::env::var("INV"), w.players[0].as_ref()) {
            if t % 25 == 0 {
                let mo = p.mo.unwrap();
                let counts: Vec<String> = items
                    .split(',')
                    .map(|n| {
                        let c = w
                            .classes
                            .find_name(n)
                            .map_or(-1, |c| w.inventory.count(mo, c));
                        format!("{n}={c}")
                    })
                    .collect();
                println!("t={t} inv {}", counts.join(" "));
            }
        }
        if let Ok(track) = std::env::var("TRACK") {
            for &id in &w.thinkers.default {
                if let Some(act) = w.actors.get(id) {
                    let n = w.classes.get(act.class).map_or("?", |c| c.name.as_str());
                    if n.eq_ignore_ascii_case(&track) {
                        println!(
                            "t={t} track {n}#{} pos=({:.0},{:.0},{:.0}) vel=({:.1},{:.1},{:.1}) hp={} flags={:#x}",
                            id.index(),
                            act.pos.x(),
                            act.pos.y(),
                            act.pos.z(),
                            act.vel.x(),
                            act.vel.y(),
                            act.vel.z(),
                            act.health,
                            act.flags
                        );
                    }
                }
            }
        }
        for &id in &w.thinkers.default {
            if seen.insert(id) && t >= 5 {
                if let Some(act) = w.actors.get(id) {
                    let n = w
                        .classes
                        .get(act.class)
                        .map_or("?".into(), |c| c.name.clone());
                    println!("t={t} new {n}");
                }
            }
        }
    }
}
