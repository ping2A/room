//! BD: list actors near the player that the renderer would draw
//! (not DONTDRAW, sprite has a patch) — finds "phantom gun" billboards.
use room_game::{GameOptions, Headless, Skill};
use room_sim::TicCmd;
use room_ui::PatchCache;
use room_vfs::VfsBuilder;
use std::collections::BTreeMap;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (iwad, pk3) = (&a[1], &a[2]);
    let tics: u32 = a.get(3).map_or(1500, |s| s.parse().unwrap());
    let map_name = a.get(4).cloned().unwrap_or_else(|| "MAP01".into());
    let vfs = room_game::mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_path(iwad)
        .unwrap()
        .mount_path(pk3)
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
    let mut seen: BTreeMap<String, (u32, String)> = BTreeMap::new();
    let mut prev: std::collections::HashMap<u32, String> = Default::default();
    let mut reported = 0;
    for t in 0..tics {
        let cmd = TicCmd {
            forward_move: if (t / 105) % 2 == 0 { 20 } else { -20 },
            yaw: 300,
            buttons: room_sim::Buttons(if t % 3 != 0 {
                room_sim::Buttons::ATTACK
            } else {
                0
            }),
            weapon_select: if t % 150 == 5 {
                ((t / 150) % 9 + 1) as u8
            } else {
                0
            },
            ..TicCmd::default()
        };
        game.tick(&[cmd]);
        let Some(w) = game.world() else { continue };
        for &id in &w.thinkers.default {
            let Some(act) = w.actors.get(id) else {
                continue;
            };
            if act.destroyed {
                continue;
            }
            let cls = w
                .classes
                .get(act.class)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let st = act.state.and_then(|sid| w.states.get(sid));
            let owner = st
                .and_then(|x| w.classes.get(x.owner))
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let d = format!(
                "{:?} {}{} owner={owner} act={:?}{:?}",
                act.state,
                st.map(|x| x.sprite.clone()).unwrap_or_default(),
                st.map(|x| x.frame).unwrap_or(0),
                st.and_then(|x| x.action.clone()),
                st.map(|x| x.action_args.clone())
            );
            if st.is_some_and(|x| {
                w.states.find_label(x.owner, "Ready").is_some()
                    && w.states.find_label(act.class, "Ready").is_none()
            }) && reported < 6
            {
                if let Some(p) = prev.get(&id.index()) {
                    if !p.contains(&owner) || true {
                        println!("t={t} {cls} #{} prev[{p}] now[{d}]", id.index());
                        reported += 1;
                    }
                } else {
                    println!("t={t} {cls} #{} NEW now[{d}]", id.index());
                    reported += 1;
                }
            }
            prev.insert(id.index(), d);
        }
        let Some(pmo) = w
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            continue;
        };
        let Some(pa) = w.actors.get(pmo) else {
            continue;
        };
        let (px, py) = (pa.pos.x(), pa.pos.y());
        for &id in &w.thinkers.default {
            if id == pmo {
                continue;
            }
            let Some(act) = w.actors.get(id) else {
                continue;
            };
            if act.destroyed {
                continue;
            }
            let cf = w.classes.get(act.class).map(|c| c.flags).unwrap_or(0);
            if (act.flags | cf) & room_sim::FLAG_DONTDRAW != 0 {
                continue;
            }
            if (act.flags | cf) & room_sim::FLAG_COUNTKILL != 0 {
                continue;
            }
            let d = ((act.pos.x() - px).powi(2) + (act.pos.y() - py).powi(2)).sqrt();
            if d > 96.0 {
                continue;
            }
            let s = act.sprite.to_ascii_uppercase();
            if s.is_empty() || s == "TNT1" {
                continue;
            }
            let l = char::from(b'A' + act.frame.min(25));
            let vis = ['0', '1', '2', '3', '4', '5', '6', '7', '8']
                .iter()
                .any(|r| patches.ensure(&vfs, &format!("{s}{l}{r}")).is_some())
                || (1..=8).any(|r| {
                    patches
                        .ensure(&vfs, &format!("{s}{l}{r}{l}{}", 10 - r))
                        .is_some()
                });
            if !vis {
                continue;
            }
            let cls = w
                .classes
                .get(act.class)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let e = seen
                .entry(format!("{cls} {s}"))
                .or_insert((0, String::new()));
            e.0 += 1;
            let st = act.state.and_then(|sid| w.states.get(sid));
            let owner = st
                .and_then(|x| w.classes.get(x.owner))
                .map(|c| c.name.clone())
                .unwrap_or_default();
            e.1 = format!(
                "d={d:.0} style={:?} alpha={:.2} state={:?} st.sprite={:?} st.owner={owner} tics={} act={:?} {:?}",
                act.render_style,
                act.alpha,
                act.state,
                st.map(|x| x.sprite.clone()),
                act.tics,
                st.and_then(|x| x.action.clone()),
                st.map(|x| x.action_args.clone())
            );
        }
    }
    for (k, v) in &seen {
        println!("{k}: {} tics  {}", v.0, v.1);
    }
}
