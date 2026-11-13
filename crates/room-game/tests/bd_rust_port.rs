//! Brutal Doom with its ZScript vs Brutal Doom with the Rust port
//! (`mods/brutal-zscript-rs`, `docs/39`): the same scripted run on
//! Freedoom E1M1 must play out the same.
//!
//! The two sessions do not have the same class table (the port registers
//! `Damage_Control` through the mod ABI instead of the ZScript compiler), so
//! they are compared on what can be seen in the game: every live actor's
//! class name, health, position and TID, and every inventory, per tic.
//! Both runs save and reload halfway. Gated on `ROOM_BRUTALDOOM_PK3` (a
//! Brutal Doom pk3 or its unpacked directory); `ROOM_BD_PORT_TICS` sets the
//! length (default 1500).
//!
//! Room plays skills 1–5, so Brutal Doom's skill-6 `Damage_Control` branch
//! never runs in a game; `room-sim`'s `brutal_zscript_port_end_to_end`
//! test drives it (and the `ScriptCall` string paths) directly.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill};
use room_sim::World;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn mount(paths: &[&Path]) -> room_vfs::Vfs {
    let mut b = room_game::mount_embedded_engine(room_vfs::VfsBuilder::new()).unwrap();
    for p in paths {
        b = b.mount_path(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    }
    b.build().unwrap()
}

/// Everything a player could notice, in thinker order.
fn digest(w: &World) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let name = |c| w.classes.get(c).map_or("?", |d| d.name.as_str());
    for &id in &w.thinkers.default {
        let Some(a) = w.actors.get(id).filter(|a| !a.destroyed) else {
            continue;
        };
        let _ = write!(
            s,
            "{} {} {:.2} {:.2} {:.2} {} [",
            name(a.class),
            a.health,
            a.pos.x(),
            a.pos.y(),
            a.pos.z(),
            a.tid
        );
        let mut inv: Vec<String> = w
            .inventory
            .iter(id)
            .map(|i| format!("{}={}", name(i.class), i.amount))
            .collect();
        inv.sort();
        // The voice actually used: the actor's, else its class's, else
        // "player" (`actor_sound_identity`).
        let voice = [a.sound_class.trim(), w.classes.get(a.class).map_or("", |d| d.sound_class.trim())]
            .into_iter()
            .find(|v| !v.is_empty())
            .unwrap_or("player");
        let _ = writeln!(s, "{}] {voice}", inv.join(","));
    }
    s
}

struct Run {
    digests: Vec<String>,
    log: Vec<String>,
    replaced: Vec<String>,
    mods: Vec<String>,
}

fn session(paths: &[&Path], tics: u32, skill: u8) -> Run {
    let mut vfs = mount(paths);
    let mut game = Headless::new(GameOptions {
        skill: Skill(skill),
        ..GameOptions::default()
    });
    let defs = room_defs::DefinitionsBuilder::new(&vfs).build().unwrap();
    let handlers = defs.zscript_event_handlers();
    game.set_complevel(defs.complevel);
    game.set_terrain(&defs.terrain);
    game.set_keyconf(&defs.keyconf);
    game.set_acs_library_pool(room_game::acs_library_pool(&vfs), defs.loadacs.libraries.clone());
    game.set_mapinfo(Arc::new(defs.mapinfo));
    let zs = room_game::load_zscript_from_vfs(&vfs);
    assert!(zs.ok, "ZScript: {:?}", zs.notes);
    let replaced: Vec<String> = zs
        .skipped
        .iter()
        .filter(|n| n.contains("replaces_zscript"))
        .cloned()
        .collect();
    game.set_zscript(zs.program, handlers);
    let data = room_game::load_embedded_engine_data_with(&vfs).unwrap();
    game.set_defs(data.classes, data.states);
    let (mods, errors) = room_game::discover_mods(&vfs);
    assert!(errors.is_empty(), "{errors:?}");
    let mods = if mods.is_empty() {
        Vec::new()
    } else {
        game.set_script_mods_with_vfs(mods, Some(&mut vfs)).unwrap()
    };
    let mut level = room_map::load_map(&vfs, "E1M1").unwrap();
    room_nodebuild::ensure_nodes(&mut level);
    game.start_custom_map(level, Skill(skill)).unwrap();
    let mut digests = Vec::with_capacity(tics as usize);
    for t in 0..tics {
        if t == tics / 2 {
            // Mid-run save and load: the port's state lives in its module memory.
            let bytes = game.save_game().unwrap();
            game.load_game(&bytes).unwrap();
        }
        game.tick(&[room_game::mod_convert::scripted_cmd(t)]);
        digests.push(digest(game.world().unwrap()));
    }
    Run {
        digests,
        log: game.drain_script_log(),
        replaced,
        mods,
    }
}

fn compare(skill: u8) -> Option<(Run, Run)> {
    let Ok(bd) = std::env::var("ROOM_BRUTALDOOM_PK3") else {
        eprintln!("skip: set ROOM_BRUTALDOOM_PK3");
        return None;
    };
    let iwad = root().join("testdata/freedoom/freedoom1.wad");
    let port = root().join("mods/brutal-zscript-rs/brutal-zscript.wad");
    if !iwad.is_file() || !port.is_file() {
        eprintln!("skip: needs freedoom1.wad and mods/brutal-zscript-rs/brutal-zscript.wad");
        return None;
    }
    let tics = std::env::var("ROOM_BD_PORT_TICS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1500);
    let bd = PathBuf::from(bd);
    let original = session(&[&iwad, &bd], tics, skill);
    let ported = session(&[&iwad, &bd, &port], tics, skill);
    assert!(original.mods.is_empty());
    assert_eq!(ported.mods, ["brutal-zscript"]);
    assert_eq!(ported.replaced.len(), 2, "{:?}", ported.replaced);
    assert!(
        !ported.log.iter().any(|l| l.contains("disabled")),
        "{:?}",
        ported.log
    );
    Some((original, ported))
}

fn first_divergence(a: &Run, b: &Run) -> Option<usize> {
    a.digests.iter().zip(&b.digests).position(|(x, y)| x != y)
}

fn show(a: &Run, b: &Run, t: usize) -> String {
    let (x, y) = (&a.digests[t], &b.digests[t]);
    let diff: Vec<String> = x
        .lines()
        .zip(y.lines())
        .filter(|(l, r)| l != r)
        .take(6)
        .map(|(l, r)| format!("  zscript: {l}\n  rust:    {r}"))
        .collect();
    format!("tic {t}:\n{}", diff.join("\n"))
}

/// Normal skills: the port runs the `ScriptCall`s Brutal Doom's ACS makes
/// (gender / face tokens, player TIDs, lump probes) exactly like the ZScript.
#[test]
fn rust_port_plays_like_the_zscript() {
    let Some((a, b)) = compare(4) else {
        return;
    };
    if let Some(t) = first_divergence(&a, &b) {
        panic!("diverged at {}", show(&a, &b, t));
    }
    let last = a.digests.last().unwrap();
    assert!(last.contains("GenderMale=1"), "no gender token:\n{last}");
    eprintln!("{} tics identical", a.digests.len());
}
