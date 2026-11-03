//! Gated Freedoom2 + Brutal Doom MAP01 tick smoke (BD3/BD6).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_core::{Angle, DVec3};
use room_decorate::{DecorateError, expand_includes_mapped, parse_decorate, split_actors};
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_sim::{Buttons, ClassRegistry, StateTable, TicCmd, World};
use room_vfs::{Namespace, Vfs, VfsBuilder};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn collect_zscript(vfs: &Vfs) -> Vec<(String, String)> {
    let mut sources = Vec::new();
    for (id, info) in vfs.iter() {
        let path = if !info.full_path.is_empty() {
            info.full_path.clone()
        } else {
            info.short_name.as_str().to_owned()
        };
        let lower = path.to_ascii_lowercase();
        let is_zs = lower.ends_with(".zs")
            || lower.ends_with(".zc")
            || lower.ends_with(".zsc")
            || lower == "zscript"
            || lower.ends_with("/zscript");
        if !is_zs {
            continue;
        }
        let Ok(bytes) = vfs.read(id) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes.to_vec()) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        sources.push((path, text));
    }
    sources.sort_by(|a, b| {
        let score = |n: &str| {
            let base = n.rsplit(['/', '\\']).next().unwrap_or(n);
            if base.eq_ignore_ascii_case("zscript.zc") || base.eq_ignore_ascii_case("zscript") {
                0
            } else {
                1
            }
        };
        score(&a.0).cmp(&score(&b.0)).then_with(|| a.0.cmp(&b.0))
    });
    sources
}

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

#[test]
fn bd_freedoom_map01_ticks_gated() {
    let Ok(pk3) = std::env::var("ROOM_BRUTALDOOM_PK3") else {
        return;
    };
    let pk3 = PathBuf::from(pk3);
    if !pk3.is_file() {
        return;
    }
    let iwad =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    if !iwad.is_file() {
        eprintln!("skip: no freedoom2.wad");
        return;
    }

    let vfs = VfsBuilder::new()
        .mount_path(&iwad)
        .expect("iwad")
        .mount_path(&pk3)
        .expect("bd")
        .build()
        .expect("vfs");

    let base_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/room/actors/doom.txt");
    let base = std::fs::read_to_string(&base_path).unwrap_or_default();
    let root_id = vfs.find("DECORATE", Namespace::Global).expect("DECORATE");
    let root = String::from_utf8_lossy(&vfs.read(root_id).unwrap()).into_owned();
    let exp = expand_includes_mapped(&root, |p| resolve_include(&vfs, p)).expect("expand");
    let combined = if base.is_empty() {
        exp.text
    } else {
        format!("{base}\n{}", exp.text)
    };
    let mut ir = room_decorate::ClassDefIr::default();
    let mut fail = 0u32;
    for chunk in split_actors(&combined) {
        match parse_decorate(&chunk) {
            Ok(part) => ir.actors.extend(part.actors),
            Err(_) => fail += 1,
        }
    }
    assert_eq!(fail, 0, "DECORATE parse fails while building registry");
    // Later definitions replace earlier ones (GZDoom redefine), preserving
    // first-seen order so parents still precede children.
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
    assert!(ir.actors.len() > 3000, "actors={}", ir.actors.len());

    let classes = Arc::new(ClassRegistry::from_decorate(&ir).expect("registry"));
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = load_map(&vfs, "MAP01").expect("MAP01");
    ensure_nodes(&mut map);
    let mut world = World::from_map_with_classes(map, classes, states).expect("world");

    // BD6: compile commercial ZScript and register MAPINFO event handlers.
    let zs_sources = collect_zscript(&vfs);
    assert!(!zs_sources.is_empty(), "BD should ship ZScript");
    let zs = room_zscript::check(&zs_sources);
    assert!(zs.ok(), "ZScript: {:?}", zs.diagnostics);
    if let Some(prog) = zs.program {
        world.load_zscript_program(prog);
    }
    world
        .zscript
        .set_event_handlers(["ZC_TMapHack".into(), "Script_DamageMult".into()]);
    world
        .zscript
        .activate_map_handlers()
        .expect("activate handlers");

    // BD5/BD6: LOADACS libraries (OPEN is started for map BEHAVIOR only; libraries
    // still resolve named scripts for puke / MES ACS_NamedExecuteAlways).
    for lump in ["BD_Main", "BD_Hash", "zancmpat"] {
        if let Some(id) = vfs.find(lump, Namespace::Global).or_else(|| {
            vfs.find_full(&format!("acs/{lump}.o"))
                .or_else(|| vfs.find_full(&format!("acs/{lump}")))
        }) {
            if let Ok(bytes) = vfs.read(id) {
                let _ = room_sim::push_acs_library(&mut world, &bytes);
            }
        }
    }

    // Default 10s @ 35 Hz; set ROOM_BD_MAP01_SOAK_TICS=21000 for BD3 10-min soak.
    // Long soaks skip MES decoration seeding (thousands of BDEC thinkers) so the
    // wall-clock stays practical; short soaks still exercise the MES path.
    let tics: u32 = std::env::var("ROOM_BD_MAP01_SOAK_TICS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(350);
    let run_mes = tics <= 1000;

    let mut bdec_seeded = 0u32;
    if run_mes {
        // BD6 MES: Freedoom MAP01 won't hash-match Doom2 Entryway, so drive the
        // decoration spawner the hash script would invoke (D2Map01DecorationSpawn).
        let mes_class = world
            .classes
            .find_name("D2Map01DecorationSpawn")
            .expect("D2Map01DecorationSpawn class");
        world
            .spawn_at(mes_class, DVec3::new(0.0, 0.0, 0.0), Angle::ZERO)
            .expect("spawn MES decoration root");
        bdec_seeded = 1;
    }

    if std::env::var("ROOM_BD_TRACE").is_ok() {
        let pistol = world.classes.find_name("BrutalPistol").unwrap();
        let mag = world.classes.find_name("BDPistolAmmo").unwrap();
        let mo = world.players[0].as_ref().unwrap().mo.unwrap();
        let _ = room_sim::give_inventory(&mut world, mo, pistol, 1);
        let _ = room_sim::give_inventory(&mut world, mo, mag, 16);
        if let Some(p) = world.players[0].as_mut() {
            p.ready_weapon = Some(pistol);
            p.pspr_state = None;
            p.pspr_y = 128.0;
        }
        for i in 0..70 {
            let mut cmd = TicCmd::default();
            if (20..28).contains(&i) {
                cmd.buttons = Buttons(Buttons::ATTACK);
            }
            world.set_ticcmds(&[cmd; 4]);
            world.tick();
            if let Some(hud) = world.player_hud(0) {
                let p = world.players[0].as_ref().unwrap();
                let st = p.pspr_state.and_then(|s| world.states.get(s));
                eprintln!(
                    "trace tic={i} patch={} shown={} frame={} state_spr={} state_fr={} act={:?} y={:.1}",
                    hud.weapon_patch,
                    p.pspr_sprite,
                    p.pspr_frame,
                    st.map(|s| s.sprite.as_str()).unwrap_or(""),
                    st.map(|s| s.frame).unwrap_or(0),
                    st.and_then(|s| s.action.as_deref()),
                    p.pspr_y
                );
            }
        }
        let eye = world.players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .and_then(|id| world.actors.get(id))
            .map(|a| a.pos);
        let player = world.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(eye) = eye {
            let mut near = 0u32;
            for id in world.thinkers.default.iter().copied() {
                let Some(a) = world.actors.get(id) else {
                    continue;
                };
                let Some(def) = world.classes.get(a.class) else {
                    continue;
                };
                if def.flags & room_sim::FLAG_COUNTKILL == 0 || a.health <= 0 {
                    continue;
                }
                let dx = a.pos.x() - eye.x();
                let dy = a.pos.y() - eye.y();
                if dx * dx + dy * dy > 1200.0 * 1200.0 {
                    continue;
                }
                near += 1;
                if near <= 6 {
                    let act = a
                        .state
                        .and_then(|s| world.states.get(s))
                        .and_then(|s| s.action.clone());
                    let see = room_sim::check_sight(&world, id, player);
                    eprintln!(
                        "monster {} sprite={} act={act:?} target={} see={see} dist={:.0}",
                        def.name,
                        a.sprite,
                        a.target.is_some(),
                        (dx * dx + dy * dy).sqrt()
                    );
                }
            }
            eprintln!(
                "monsters near: {near} player_health={}",
                world.actors.get(player).map(|a| a.health).unwrap_or(0)
            );
            let imp = world.classes.find_name("Imp").unwrap();
            let pos = world.actors.get(player).unwrap().pos;
            let beside = room_core::DVec3::new(pos.x() + 96.0, pos.y(), pos.z());
            if let Ok(id) = world.spawn_at(imp, beside, room_core::Angle::ZERO) {
                for _ in 0..8 {
                    world.set_ticcmds(&[TicCmd::default(); 4]);
                    world.tick();
                }
                let a = world.actors.get(id).unwrap();
                let see = room_sim::check_sight(&world, id, player);
                eprintln!("open imp h={:.1} sprite={} see={see}", a.height, a.sprite);
                let before = world.actors.get(player).map(|a| a.health).unwrap_or(0);
                for _ in 0..105 {
                    world.set_ticcmds(&[TicCmd::default(); 4]);
                    world.tick();
                }
                let after = world.actors.get(player).map(|a| a.health).unwrap_or(0);
                let act = world
                    .actors
                    .get(id)
                    .and_then(|a| a.state)
                    .and_then(|s| world.states.get(s))
                    .and_then(|s| s.action.clone());
                eprintln!(
                    "open imp after 3s sprite={} act={act:?} player {before}->{after}",
                    world
                        .actors
                        .get(id)
                        .map(|a| a.sprite.as_str())
                        .unwrap_or("")
                );
            }
        }
    }

    // BD4: commercial BrutalPistol exposes Fire/Reload/Zoom; ATTACK leaves Ready.
    let pistol = world
        .classes
        .find_name("BrutalPistol")
        .expect("BrutalPistol");
    for label in ["Fire", "Reload", "Zoom", "Ready"] {
        assert!(
            world.states.find_label(pistol, label).is_some(),
            "BrutalPistol missing {label}"
        );
    }
    let ready = world.states.find_label(pistol, "Ready").expect("Ready");
    if let Some(p) = world.players[0].as_mut() {
        p.ready_weapon = Some(pistol);
        p.pspr_y = 32.0; // WEAPON_TOP
        p.pspr_state = Some(ready);
        p.pending_weapon = None;
    }
    let mut fire_cmd = TicCmd::default();
    fire_cmd.buttons = Buttons(Buttons::ATTACK);
    for _ in 0..8 {
        world.set_ticcmds(&[fire_cmd; 4]);
        world.tick();
    }
    let after_fire = world.players[0].as_ref().and_then(|p| p.pspr_state);
    assert_ne!(
        after_fire,
        Some(ready),
        "ATTACK should leave BrutalPistol Ready"
    );

    let thinkers0 = world.thinkers.default.len();
    let report_every = (tics / 4).max(1);
    for i in 0..tics {
        world.set_ticcmds(&[Default::default(); 4]);
        world.tick();
        if i == 0 || (i + 1) % report_every == 0 || i + 1 == tics {
            eprintln!(
                "soak progress tic={} thinkers={}",
                i + 1,
                world.thinkers.default.len()
            );
        }
    }

    let mut bdec = 0u32;
    for id in world.thinkers.default.iter().copied() {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        let Some(def) = world.classes.get(a.class) else {
            continue;
        };
        if def.name.starts_with("BDEC") || def.name.starts_with("LightShaft") {
            bdec = bdec.saturating_add(1);
        }
    }
    eprintln!(
        "BD MAP01+ZS+MES smoke: actors_ir={} zs_files={} handlers={:?} acs_mods={} thinkers0={} thinkers_now={} bdec={} level_time={}",
        ir.actors.len(),
        zs_sources.len(),
        world.zscript.event_handlers,
        world.acs.modules.len(),
        thinkers0,
        world.thinkers.default.len(),
        bdec,
        world.level.maptime
    );
    assert!(world.level.maptime >= tics + 8);
    assert!(
        !world.zscript.handler_objs.is_empty(),
        "handlers should allocate"
    );
    if run_mes {
        assert!(
            bdec_seeded == 1 && bdec >= 10,
            "MES D2Map01DecorationSpawn should seed BDEC*/LightShaft actors; got {bdec}"
        );
    }

    // BD5: pukename starts a commercial named NET script from BD_Main.
    assert!(
        world
            .acs
            .modules
            .iter()
            .any(|m| m.script_number_by_name("GetCVars").is_some()),
        "GetCVars should exist in BD_Main after LOADACS"
    );
    let puked = room_sim::puke_named(&mut world, "GetCVars", [0; 3], false);
    assert_ne!(puked, 0, "pukename GetCVars should start");
}
