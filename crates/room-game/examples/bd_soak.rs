//! BD headless soak: report state-cycle-limit hits (class + chain) and
//! per-actor sprite flicker (visible → hidden ≤2 tics → visible).
use room_game::{GameOptions, Headless, Skill};
use room_sim::TicCmd;
use room_ui::PatchCache;
use room_vfs::VfsBuilder;
use std::collections::BTreeMap;
use std::sync::Arc;

fn letter(f: u8) -> char {
    match f {
        0..=25 => char::from(b'A' + f),
        26 => '[',
        27 => '\\',
        28 => ']',
        _ => 'A',
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (iwad, pk3) = (&a[1], &a[2]);
    let tics: u32 = a.get(3).map_or(2000, |s| s.parse().unwrap());
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
    if let Ok(c) = std::env::var("SUMMON_CLASS") {
        for _ in 0..3 {
            eprintln!("summon {c}: {}", game.cheat_summon(0, &c));
        }
    }
    if std::env::var_os("SUMMON_ALL").is_some() {
        let names: Vec<String> = game
            .world()
            .unwrap()
            .classes
            .iter()
            .filter(|c| c.flags & room_sim::FLAG_COUNTKILL != 0)
            .map(|c| c.name.clone())
            .collect();
        eprintln!("summoning {} monster classes", names.len());
        for n in &names {
            game.cheat_summon(0, n);
        }
    }
    let mut patches = PatchCache::from_vfs(&vfs);
    let mut vis_hist: BTreeMap<u32, (bool, u32, String)> = BTreeMap::new(); // id -> (visible, hidden_run, last_hidden_desc)
    let mut flicker: BTreeMap<String, u32> = BTreeMap::new();
    let mut cycles: BTreeMap<String, u32> = BTreeMap::new();
    let mut slow = Vec::new();
    for t in 0..tics {
        let cmd = TicCmd {
            forward_move: if (t / 105) % 2 == 0 { 20 } else { -20 },
            yaw: 300,
            buttons: room_sim::Buttons(room_sim::Buttons::ATTACK),
            weapon_select: if t % 150 == 5 {
                ((t / 150) % 9 + 1) as u8
            } else {
                0
            },
            ..TicCmd::default()
        };
        let t0 = std::time::Instant::now();
        game.tick(&[cmd]);
        if std::env::var_os("KILL_EVERY").is_some() && t % 70 == 35 {
            game.cheat_kill_monsters();
            if std::env::var_os("SUMMON_ALL").is_some() {
                let names: Vec<String> = game
                    .world()
                    .unwrap()
                    .classes
                    .iter()
                    .filter(|c| c.flags & room_sim::FLAG_COUNTKILL != 0)
                    .map(|c| c.name.clone())
                    .collect();
                let k = (t / 70) as usize;
                for n in names.iter().skip(k * 12).take(12) {
                    game.cheat_summon(0, n);
                }
            }
        }
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        if dt > 30.0 {
            slow.push((t, dt));
        }
        let Some(w) = game.world() else { continue };
        if t % 100 == 99 {
            let live = w
                .thinkers
                .default
                .iter()
                .filter(|&&i| w.actors.get(i).is_some_and(|a| !a.destroyed))
                .count();
            let mut hist: BTreeMap<String, u32> = BTreeMap::new();
            for &i in &w.thinkers.default {
                if let Some(a) = w.actors.get(i) {
                    if !a.destroyed {
                        *hist
                            .entry(
                                w.classes
                                    .get(a.class)
                                    .map(|c| c.name.clone())
                                    .unwrap_or_default(),
                            )
                            .or_default() += 1;
                    }
                }
            }
            let mut hv: Vec<_> = hist.into_iter().collect();
            hv.sort_by(|a, b| b.1.cmp(&a.1));
            println!(
                "t={t} thinkers={} live={live} ms={dt:.1} top={:?}",
                w.thinkers.default.len(),
                &hv[..hv.len().min(8)]
            );
        }
        let ids: Vec<_> = w.thinkers.default.clone();
        for id in ids {
            let Some(act) = w.actors.get(id) else {
                continue;
            };
            if act.destroyed {
                vis_hist.remove(&id.index());
                continue;
            }
            let cf = w.classes.get(act.class).map(|c| c.flags).unwrap_or(0);
            let monster = (act.flags | cf) & room_sim::FLAG_COUNTKILL != 0;
            if !monster && std::env::var_os("ALL_ACTORS").is_none() {
                continue;
            }
            if act.sprite.eq_ignore_ascii_case("TNT1") {
                vis_hist.remove(&id.index());
                continue;
            }
            let s = act.sprite.to_ascii_uppercase();
            let l = letter(act.frame);
            let visible = !s.is_empty()
                && s != "TNT1"
                && ['1', '0', '2', '3', '4', '5', '6', '7', '8']
                    .iter()
                    .any(|r| patches.ensure(&vfs, &format!("{s}{l}{r}")).is_some())
                || (1..=8).any(|r| {
                    patches
                        .ensure(&vfs, &format!("{s}{l}{r}{l}{}", 10 - r))
                        .is_some()
                });
            let cls = w
                .classes
                .get(act.class)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let st = act.state.and_then(|sid| w.states.get(sid));
            let desc = format!(
                "{cls} {s}{l} tics={} act={}",
                act.tics,
                st.and_then(|x| x.action.clone()).unwrap_or_default()
            );
            let e = vis_hist
                .entry(id.index())
                .or_insert((true, 0, String::new()));
            if visible {
                if !e.0 && e.1 <= 2 {
                    *flicker.entry(e.2.clone()).or_default() += 1;
                }
                *e = (true, 0, String::new());
            } else {
                if e.0 {
                    e.2 = desc;
                }
                e.0 = false;
                e.1 += 1;
            }
        }
        if let Some(w) = game.world_mut() {
            for m in w.drain_debug() {
                if m.contains("state loop") {
                    let key = m
                        .split("loop in ")
                        .nth(1)
                        .unwrap_or(&m)
                        .split(" (")
                        .next()
                        .unwrap_or("")
                        .to_owned()
                        + " "
                        + m.split("chain ").nth(1).unwrap_or("");
                    *cycles.entry(key).or_default() += 1;
                }
            }
        }
    }
    println!("state-cycle hits:");
    for (k, v) in &cycles {
        println!("  {v}x {k}");
    }
    let mut f: Vec<_> = flicker.into_iter().collect();
    f.sort_by(|a, b| b.1.cmp(&a.1));
    println!("monster flicker (hidden 1–2 tics between visible tics):");
    for (k, v) in f.iter().take(25) {
        println!("  {v}x {k}");
    }
    println!(
        "slow tics (>30ms): {}  {:?}",
        slow.len(),
        slow.iter().take(10).collect::<Vec<_>>()
    );
}
