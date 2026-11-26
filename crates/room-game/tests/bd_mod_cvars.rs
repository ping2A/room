//! Brutal Doom's own options reach its scripts: with `bd_disabledecorations`
//! = 1 (Settings › Mod options › Advanced Decorations: Off) its ACS
//! `BDCheckDecorations` sends health bonuses to their plain `Vanilla` loop,
//! so no blue flares spawn. `ROOM_BD_IWAD=…/freedoom2.wad
//! ROOM_BRUTALDOOM_PK3=…/brutal.pk3 cargo test -p room-game --test
//! bd_mod_cvars -- --ignored`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_core::{Angle, DVec3};
use room_decorate::{DecorateError, expand_includes_mapped, parse_decorate, split_actors};
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_sim::{ClassRegistry, StateTable, TicCmd, World};
use room_vfs::{Namespace, Vfs, VfsBuilder};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn resolve_include(vfs: &Vfs, path: &str) -> Result<String, DecorateError> {
    let norm = path.replace('\\', "/");
    let candidates = [
        norm.clone(),
        norm.to_ascii_lowercase(),
        norm.trim_start_matches('/').to_owned(),
    ];
    for c in &candidates {
        if let Some(id) = vfs.find_full(c) {
            let bytes = vfs
                .read(id)
                .map_err(|e| DecorateError::Message(format!("read {c}: {e}")))?;
            return String::from_utf8(bytes.to_vec())
                .map_err(|e| DecorateError::Message(format!("utf8 {c}: {e}")));
        }
        let stem = Path::new(c)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(c);
        if let Some(id) = vfs.find(stem, Namespace::Global) {
            let bytes = vfs
                .read(id)
                .map_err(|e| DecorateError::Message(format!("read {stem}: {e}")))?;
            return String::from_utf8(bytes.to_vec())
                .map_err(|e| DecorateError::Message(format!("utf8 {stem}: {e}")));
        }
    }
    Err(DecorateError::Message(format!("include not found: {path}")))
}

fn flares_with(decorations_off: bool) -> usize {
    let pk3 = PathBuf::from(std::env::var("ROOM_BRUTALDOOM_PK3").unwrap());
    let iwad = PathBuf::from(std::env::var("ROOM_BD_IWAD").unwrap());
    let vfs = VfsBuilder::new()
        .mount_path(&iwad)
        .unwrap()
        .mount_path(&pk3)
        .unwrap()
        .build()
        .unwrap();
    let base = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/room/actors/doom.txt"),
    )
    .unwrap_or_default();
    let root_id = vfs.find("DECORATE", Namespace::Global).expect("DECORATE");
    let root = String::from_utf8_lossy(&vfs.read(root_id).unwrap()).into_owned();
    let exp = expand_includes_mapped(&root, |p| resolve_include(&vfs, p)).expect("expand");
    let combined = format!("{base}\n{}", exp.text);
    let mut ir = room_decorate::ClassDefIr::default();
    for chunk in split_actors(&combined) {
        if let Ok(part) = parse_decorate(&chunk) {
            ir.actors.extend(part.actors);
        }
    }
    let mut order: Vec<room_decorate::ActorDef> = Vec::new();
    let mut index = std::collections::BTreeMap::<String, usize>::new();
    for a in ir.actors.drain(..) {
        let key = a.name.to_ascii_lowercase();
        if let Some(&i) = index.get(&key) {
            order[i] = a;
        } else {
            index.insert(key, order.len());
            order.push(a);
        }
    }
    ir.actors = order;
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).expect("registry"));
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = load_map(&vfs, "MAP01").expect("MAP01");
    ensure_nodes(&mut map);
    let mut world = World::from_map_with_classes(map, classes, states).expect("world");
    world.set_mod_cvar(
        "bd_disabledecorations",
        if decorations_off { "1" } else { "0" },
    );
    {
        let lump = "BD_Main";
        let id = vfs
            .find(lump, Namespace::Global)
            .or_else(|| vfs.find_full(&format!("acs/{lump}.o")));
        if let Some(bytes) = id.and_then(|id| vfs.read(id).ok()) {
            let _ = room_sim::push_acs_library(&mut world, &bytes);
        }
    }
    // Level start: Brutal's OPEN `GetCVars` copies the CVars into ACS
    // variables, which `BDCheckDecorations` then tests.
    room_sim::start_library_open_scripts(&mut world, 0);
    let hp = world.classes.find_name("HealthPlus").unwrap();
    let flare = world.classes.find_name("BlueFlareSmallItem").unwrap();
    let mo = world.players[0].as_ref().unwrap().mo.unwrap();
    let p = world.actors.get(mo).unwrap().pos;
    world
        .spawn_at(hp, DVec3::new(p.x() + 96.0, p.y(), 0.0), Angle::ZERO)
        .unwrap();
    let mut seen = 0;
    for _ in 0..60 {
        world.set_ticcmds(&[TicCmd::default(); 4]);
        world.tick();
        seen += world
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                world
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == flare && !a.destroyed)
            })
            .count();
    }
    seen
}

#[test]
#[ignore = "needs ROOM_BD_IWAD and ROOM_BRUTALDOOM_PK3"]
fn disabling_decorations_stops_bonus_flares() {
    let on = flares_with(false);
    let off = flares_with(true);
    eprintln!("flare-tics: decorations on={on} off={off}");
    assert!(on > 20, "decorations on should keep a flare up ({on})");
    assert!(
        off < 3,
        "bd_disabledecorations=1 should stop the flares ({off})"
    );
}
