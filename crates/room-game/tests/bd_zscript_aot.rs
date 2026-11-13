//! `docs/37` ZW3 acceptance on Brutal Doom: the same session with BD's
//! ZScript on the interpreter and on the AOT (WebAssembly) tier must give the
//! same `World::checksum` and ZScript state every tic.
//!
//! Gated: `ROOM_BRUTALDOOM_PK3` (pk3 or extracted directory). The IWAD is
//! `ROOM_DOOM2_WAD` (MAP01) when set, else `testdata/freedoom/freedoom1.wad`
//! (E1M1). `ROOM_BD_AOT_TICS` sets the length (default 700). Run with
//! `cargo test -p room-game --features zvm-aot --test bd_zscript_aot`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_decorate::{DecorateError, expand_includes_mapped, parse_decorate, split_actors};
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_sim::{Buttons, ClassRegistry, StateTable, TicCmd, World};
use room_vfs::{Namespace, Vfs, VfsBuilder};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn resolve_include(vfs: &Vfs, path: &str) -> Result<String, DecorateError> {
    let norm = path.replace('\\', "/");
    for c in [
        norm.clone(),
        norm.to_ascii_lowercase(),
        norm.trim_start_matches('/').to_owned(),
    ] {
        if let Some(id) = vfs.find_full(&c) {
            let bytes = vfs
                .read(id)
                .map_err(|e| DecorateError::Message(format!("{c}: {e}")))?;
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }
        let stem = Path::new(&c)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&c)
            .to_owned();
        if let Some(id) = vfs.find(&stem, Namespace::Global) {
            let bytes = vfs
                .read(id)
                .map_err(|e| DecorateError::Message(format!("{stem}: {e}")))?;
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    Err(DecorateError::Message(format!("include not found: {path}")))
}

struct Content {
    classes: Arc<ClassRegistry>,
    states: Arc<StateTable>,
    program: room_zvm::CompiledProgram,
    vfs: Vfs,
    map: &'static str,
}

fn load_content() -> Option<Content> {
    let pk3 = PathBuf::from(std::env::var("ROOM_BRUTALDOOM_PK3").ok()?);
    if !pk3.exists() {
        return None;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (iwad, map) = match std::env::var("ROOM_DOOM2_WAD") {
        Ok(p) if Path::new(&p).is_file() => (PathBuf::from(p), "MAP01"),
        _ => (root.join("testdata/freedoom/freedoom1.wad"), "E1M1"),
    };
    if !iwad.is_file() {
        eprintln!("skip: no IWAD");
        return None;
    }
    let vfs = VfsBuilder::new()
        .mount_path(&iwad)
        .expect("iwad")
        .mount_path(&pk3)
        .expect("bd")
        .build()
        .expect("vfs");
    let base =
        std::fs::read_to_string(root.join("wadsrc/room/actors/doom.txt")).unwrap_or_default();
    let root_id = vfs.find("DECORATE", Namespace::Global).expect("DECORATE");
    let text = String::from_utf8_lossy(&vfs.read(root_id).unwrap()).into_owned();
    let exp = expand_includes_mapped(&text, |p| resolve_include(&vfs, p)).expect("expand");
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
    let (sources, _) = room_game::zscript_sources_from_vfs(&vfs);
    let zs = room_zscript::check(&sources);
    assert!(zs.ok(), "BD ZScript: {:?}", zs.diagnostics);
    Some(Content {
        classes,
        states,
        program: zs.program.expect("program"),
        vfs,
        map,
    })
}

/// Scripted input: walk, strafe, turn, fire bursts, use.
fn cmd_at(tic: u32) -> TicCmd {
    let mut cmd = TicCmd::default();
    let phase = tic % 140;
    cmd.forward_move = if phase < 70 { 25 } else { -10 };
    cmd.side_move = if (tic / 35) % 2 == 0 { 8 } else { -8 };
    cmd.yaw = if phase % 35 < 10 { 900 } else { 0 };
    let mut b = 0;
    if tic % 20 < 6 {
        b |= Buttons::ATTACK;
    }
    if tic % 50 == 0 {
        b |= Buttons::USE;
    }
    cmd.buttons = Buttons(b);
    cmd
}

fn run(content: &Content, aot: bool, tics: u32) -> (Vec<(u64, [u8; 32])>, u64, &'static str) {
    let mut map = load_map(&content.vfs, content.map).expect("map");
    ensure_nodes(&mut map);
    let mut world = World::from_map_with_classes(
        map,
        Arc::clone(&content.classes),
        Arc::clone(&content.states),
    )
    .expect("world");
    world.skill = 6;
    world.load_zscript_program(content.program.clone());
    if aot {
        assert!(world.zscript.enable_aot(), "AOT tier failed to attach");
        world.zscript.set_aot_all();
    }
    world
        .zscript
        .set_event_handlers(["ZC_TMapHack".into(), "Script_DamageMult".into()]);
    world.zscript.activate_map_handlers().expect("handlers");
    world.arm_zscript_level();
    let mut trace = Vec::with_capacity(tics as usize);
    for tic in 0..tics {
        world.set_ticcmds(&[cmd_at(tic); 4]);
        world.tick();
        trace.push((
            world.checksum(),
            world.zscript.checksum_bytes().unwrap_or([0; 32]),
        ));
    }
    (
        trace,
        world.zscript.aot_calls(),
        world.zscript.aot_engine().unwrap_or("-"),
    )
}

#[test]
fn bd_zscript_interp_and_aot_agree_gated() {
    let Some(content) = load_content() else {
        eprintln!("skip: set ROOM_BRUTALDOOM_PK3");
        return;
    };
    let tics = std::env::var("ROOM_BD_AOT_TICS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(700);
    let t0 = std::time::Instant::now();
    let (a, calls_a, _) = run(&content, false, tics);
    let t1 = std::time::Instant::now();
    let (b, calls_b, engine) = run(&content, true, tics);
    let t2 = std::time::Instant::now();
    assert_eq!(calls_a, 0);
    for (tic, (x, y)) in a.iter().zip(&b).enumerate() {
        assert_eq!(x, y, "tic {tic}: interpreter vs AOT diverged");
    }
    eprintln!(
        "BD {} {tics} tics at skill 6: checksums identical every tic; {calls_b} ZScript calls served by wasm ({engine}); interp {:?}, aot {:?}",
        content.map,
        t1 - t0,
        t2 - t1
    );
    assert!(calls_b > 0, "AOT tier never ran");
}
