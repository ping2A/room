//! BD: live monsters (health > 0) that are not drawable (TNT1, DONTDRAW,
//! RenderStyle None, alpha 0, no lump) — with the state chain that hid them.

// Dev tool: failing loudly on bad input is the right behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use room_game::{GameOptions, Headless, Skill};
use room_sim::TicCmd;
use room_ui::PatchCache;
use room_vfs::VfsBuilder;
use std::collections::BTreeMap;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let tics: u32 = a.get(3).map_or(2000, |s| s.parse().unwrap());
    let map_name = a.get(4).cloned().unwrap_or_else(|| "MAP01".into());
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
    game.set_mapinfo(Arc::new(defs.mapinfo));
    let zs = room_game::load_zscript_from_vfs(&vfs);
    game.set_zscript(zs.program, handlers);
    let data = room_game::load_embedded_engine_data_with(&vfs).unwrap();
    game.set_defs(data.classes, data.states);
    let mut map = room_map::load_map(&vfs, &map_name).unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    game.cheat_god(0);
    game.cheat_give(0, "all");
    let mut patches = PatchCache::from_vfs(&vfs);
    let mut why: BTreeMap<String, u32> = BTreeMap::new();
    let mut prev: BTreeMap<u32, String> = BTreeMap::new();
    let mut corpses: BTreeMap<u32, (String, u32, Option<u32>, String)> = BTreeMap::new();
    for t in 0..tics {
        let cmd = TicCmd {
            forward_move: if (t / 105) % 2 == 0 { 25 } else { -25 },
            yaw: if (t / 300) % 2 == 0 { 120 } else { -120 },
            buttons: room_sim::Buttons(if t % 4 == 0 {
                room_sim::Buttons::ATTACK
            } else {
                0
            }),
            weapon_select: if t == 3 { 2 } else { 0 },
            ..TicCmd::default()
        };
        game.tick(&[cmd]);
        let Some(w) = game.world() else { continue };
        for &id in &w.thinkers.default {
            let Some(act) = w.actors.get(id) else {
                continue;
            };
            let cname = w
                .classes
                .get(act.class)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            if cname.starts_with("DeadZombie") || cname.starts_with("DeadShotgun") {
                let e =
                    corpses
                        .entry(id.index())
                        .or_insert((cname.clone(), t, None, String::new()));
                if act.destroyed && e.2.is_none() {
                    e.2 = Some(t);
                }
                e.3 = format!(
                    "{}{} z={:.0} floor={:?} pos=({:.0},{:.0}) style={:?} a={:.2} flags_dd={}",
                    act.sprite,
                    act.frame,
                    act.pos.z(),
                    room_sim::floor_at_xy(w, act.pos.x(), act.pos.y()),
                    act.pos.x(),
                    act.pos.y(),
                    act.render_style,
                    act.alpha,
                    (act.flags | w.classes.get(act.class).map(|c| c.flags).unwrap_or(0))
                        & room_sim::FLAG_DONTDRAW
                        != 0
                );
            }
            let cf = w.classes.get(act.class).map(|c| c.flags).unwrap_or(0);
            let flags = act.flags | cf;
            if act.destroyed || flags & room_sim::FLAG_COUNTKILL == 0 {
                continue;
            }
            let dead = act.health <= 0;
            let cls = w
                .classes
                .get(act.class)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let st = act.state.and_then(|s| w.states.get(s));
            let here = format!(
                "{}{} {} act={:?}{:?}",
                act.sprite,
                act.frame,
                st.map(|x| x.tics).unwrap_or(0),
                st.and_then(|x| x.action.clone()),
                st.map(|x| x.action_args.clone()).unwrap_or_default()
            );
            let s = act.sprite.to_ascii_uppercase();
            let reason = if flags & room_sim::FLAG_DONTDRAW != 0 {
                Some("DONTDRAW")
            } else if matches!(act.render_style, room_sim::RenderStyle::None) {
                Some("STYLE_NONE")
            } else if act.alpha <= 0.01 {
                Some("ALPHA0")
            } else if s == "TNT1" {
                Some("TNT1")
            } else {
                let ok = (1..=8u8).all(|r| {
                    patches
                        .sprite_frame_lump(&vfs, &s, act.frame, r)
                        .is_some_and(|(n, _)| patches.ensure(&vfs, &n).is_some())
                });
                (!ok).then_some("NOLUMP")
            };
            if let Some(r) = reason {
                let k = format!(
                    "{cls}{} {r} now[{here}] prev[{}]",
                    prev.get(&id.index()).cloned().unwrap_or_default(),
                    if dead { "(dead)" } else { "" }
                );
                *why.entry(k).or_default() += 1;
            }
            prev.insert(id.index(), here);
        }
        if let Some(w) = game.world_mut() {
            for m in w.drain_debug() {
                if m.contains("JUMP miss") {
                    *why.entry(m.split(" tic=").next().unwrap_or("").to_string())
                        .or_default() += 1;
                }
            }
        }
    }
    for (i, c) in corpses.iter().take(20) {
        println!("corpse #{i}: {c:?}");
    }
    let mut v: Vec<_> = why.into_iter().collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1));
    for (k, n) in v.iter().take(40) {
        println!("{n:6} {k}");
    }
}
