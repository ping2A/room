//! Load built-in engine data (`room.pk3` / `wadsrc/room`) for M5 bootstrap.

use crate::GameError;
use bytes::Bytes;
use room_decorate::{DecorateError, parse_decorate_with_includes};
use room_sim::{ClassRegistry, StateTable};
use room_vfs::{IwadInfo, Namespace, Vfs, VfsBuilder, parse_iwadinfo};
use std::sync::Arc;

/// Embedded `room.pk3` bytes produced by `build.rs` from `wadsrc/room`.
pub fn embedded_room_pk3() -> &'static [u8] {
    include_bytes!(concat!(env!("OUT_DIR"), "/room.pk3"))
}

/// Mount the embedded engine archive onto a [`VfsBuilder`] (under IWAD / PWADs).
pub fn mount_embedded_engine(builder: VfsBuilder) -> Result<VfsBuilder, GameError> {
    let bytes = embedded_room_pk3();
    if bytes.is_empty() {
        return Err(GameError::Message(
            "embedded room.pk3 is empty (wadsrc/room missing at build)".into(),
        ));
    }
    builder
        .mount_memory("room.pk3", Bytes::from_static(bytes))
        .map_err(|e| GameError::Message(e.to_string()))
}

/// GZDoom-compatible optional engine extras (after `room.pk3`, before IWAD).
pub const OPTIONAL_ENGINE_EXTRAS: &[&str] = &[
    "lights.pk3",
    "brightmaps.pk3",
    "game_support.pk3",
    "game_widescreen_gfx.pk3",
];

/// Mount optional extras from `search_roots` when the files exist (silent skip).
///
/// Looks for each name in [`OPTIONAL_ENGINE_EXTRAS`] under every root, first
/// hit wins per name. Also accepts a bare directory named without `.pk3`
/// (e.g. `lights/` for development).
pub fn mount_optional_engine_extras(
    mut builder: VfsBuilder,
    search_roots: &[std::path::PathBuf],
) -> VfsBuilder {
    let mut to_mount = Vec::new();
    for name in OPTIONAL_ENGINE_EXTRAS {
        for root in search_roots {
            let file = root.join(name);
            if file.is_file() {
                to_mount.push(file);
                break;
            }
            // Dev convenience: `lights/` beside `lights.pk3`.
            if let Some(stem) = name.strip_suffix(".pk3") {
                let dir = root.join(stem);
                if dir.is_dir() {
                    to_mount.push(dir);
                    break;
                }
            }
        }
    }
    for path in to_mount {
        // Path was verified to exist; `mount_path` consumes `self` on error so we
        // treat failure as exceptional (corrupt archive / IO race).
        builder = builder
            .mount_path(&path)
            .unwrap_or_else(|e| panic!("optional engine extra {}: {e}", path.display()));
    }
    builder
}

/// Mount the embedded engine archive into a [`Vfs`].
pub fn embedded_engine_vfs() -> Result<Vfs, GameError> {
    mount_embedded_engine(VfsBuilder::new())?
        .build()
        .map_err(|e| GameError::Message(e.to_string()))
}

/// Load engine data from the embedded `room.pk3`.
pub fn load_embedded_engine_data() -> Result<EngineData, GameError> {
    let vfs = embedded_engine_vfs()?;
    load_engine_data(&vfs)
}

/// Embedded `room.pk3` actors followed by every `DECORATE` lump in `vfs`
/// (IWAD → PWADs), for shells whose game VFS does not mount the engine.
pub fn load_embedded_engine_data_with(vfs: &Vfs) -> Result<EngineData, GameError> {
    let engine = embedded_engine_vfs()?;
    let mut base = load_engine_data(&engine)?;
    let lumps: Vec<_> = vfs.find_all("DECORATE", Namespace::Global).collect();
    if lumps.is_empty() {
        return Ok(base);
    }
    let engine_decorate = read_text_lump(&engine, "DECORATE").unwrap_or_default();
    let mut decorate = engine_decorate.clone();
    for id in lumps {
        let bytes = vfs
            .read(id)
            .map_err(|e| GameError::Message(e.to_string()))?;
        let text = String::from_utf8_lossy(&bytes);
        // `vfs` may already mount room.pk3 itself.
        if text == engine_decorate {
            continue;
        }
        decorate.push('\n');
        decorate.push_str(&text);
    }
    let mut ir = parse_decorate_with_includes(&decorate, |path| {
        resolve_include(&engine, path).or_else(|_| resolve_include(vfs, path))
    })
    .map_err(|e| GameError::Message(format!("DECORATE: {e}")))?;
    // GZDoom: a later lump/include redefining the same class replaces the earlier
    // body (BD redefines engine actors such as `RealGibs2`). Keep first-seen
    // order so parents still precede children.
    let replaced = dedupe_actors_last_wins(&mut ir);
    if replaced > 0 {
        eprintln!("room: DECORATE: {replaced} duplicate class name(s) — later definition wins");
    }
    stub_zscript_parents(&mut ir, || zscript_class_tree(vfs));
    let classes =
        Arc::new(ClassRegistry::from_decorate(&ir).map_err(|e| GameError::Message(e.to_string()))?);
    base.states = Arc::new(StateTable::from_decorate(&ir, &classes));
    base.classes = classes;
    Ok(base)
}

/// The mod's ZScript classes, then UZDoom's (`PowerLightAmp`, …) for the
/// ancestors Room's own DECORATE does not define.
fn zscript_class_tree(vfs: &Vfs) -> room_zscript::TranslationUnit {
    let mut sources = crate::zscript_sources_from_vfs(vfs).0;
    sources.extend(crate::uzdoom_stdlib_sources());
    room_zscript::parse_sources(&sources).unit
}

/// GZDoom lets DECORATE actors inherit ZScript classes (`actor PB_Fists :
/// PB_Weapon`). The DECORATE registry is built before ZScript loads, so add an
/// empty actor for each such ancestor (parents first, ahead of the first
/// child); the ZScript program later fills in its defaults and states.
fn stub_zscript_parents(
    ir: &mut room_decorate::ClassDefIr,
    zscript: impl FnOnce() -> room_zscript::TranslationUnit,
) {
    use std::collections::{BTreeMap, BTreeSet};
    let mut known: BTreeSet<String> = ir
        .actors
        .iter()
        .map(|a| a.name.to_ascii_lowercase())
        .collect();
    known.insert("actor".into());
    let missing = |a: &room_decorate::ActorDef, known: &BTreeSet<String>| {
        a.parent
            .as_deref()
            .is_some_and(|p| !known.contains(&p.to_ascii_lowercase()))
    };
    let Some(first) = ir.actors.iter().position(|a| missing(a, &known)) else {
        return;
    };
    let unit = zscript();
    let mut zs: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
    // `Default { … }` lines per class (and `extend class` additions).
    let mut defaults: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for d in &unit.decls {
        if let room_zscript::Decl::Class(c) | room_zscript::Decl::ExtendClass(c) = d {
            if matches!(d, room_zscript::Decl::Class(_)) {
                zs.insert(
                    c.name.to_ascii_lowercase(),
                    (c.name.clone(), c.parent.clone()),
                );
            }
            for m in &c.members {
                if let room_zscript::Member::Default(block) = m {
                    defaults
                        .entry(c.name.to_ascii_lowercase())
                        .or_default()
                        .extend(block.lines.iter().cloned());
                }
            }
        }
    }
    let mut stubs: Vec<room_decorate::ActorDef> = Vec::new();
    let wanted: Vec<String> = ir
        .actors
        .iter()
        .filter(|a| missing(a, &known))
        .filter_map(|a| a.parent.clone())
        .collect();
    for parent in wanted {
        // Walk up to a class DECORATE knows, then add the chain top-down.
        let mut chain = Vec::new();
        let mut cur = parent.to_ascii_lowercase();
        while !known.contains(&cur) && chain.len() < 64 {
            let Some((name, up)) = zs.get(&cur) else {
                break;
            };
            chain.push((name.clone(), up.clone()));
            match up {
                Some(u) => cur = u.to_ascii_lowercase(),
                None => break,
            }
        }
        for (name, up) in chain.into_iter().rev() {
            if !known.insert(name.to_ascii_lowercase()) {
                continue;
            }
            // Its `Default` block, read as a DECORATE actor body, so DECORATE
            // children (`PB_Pistol : PB_Weapon`) inherit its properties.
            let (flags, properties) = defaults
                .get(&name.to_ascii_lowercase())
                .map(|lines| default_block_as_decorate(&name, lines))
                .unwrap_or_default();
            stubs.push(room_decorate::ActorDef {
                name,
                parent: up,
                replaces: None,
                doomednum: None,
                flags,
                properties,
                states: None,
            });
        }
    }
    if !stubs.is_empty() {
        eprintln!(
            "room: DECORATE: {} ZScript parent class(es) stubbed for DECORATE actors",
            stubs.len()
        );
        ir.actors.splice(first..first, stubs);
    }
}

/// A ZScript `Default` block read as DECORATE flags and properties. Each
/// statement is read on its own, so one DECORATE does not know
/// (`PB_Monster.posHeadHB 0.8;`, a `//$Title` comment) no longer drops the
/// whole block: PB's monsters lost `Monster;` that way, so DECORATE monsters
/// below them were neither solid nor shootable.
fn default_block_as_decorate(
    name: &str,
    lines: &[String],
) -> (Vec<room_decorate::FlagOp>, Vec<room_decorate::PropertyStmt>) {
    let body = lines.join("\n");
    let mut flags = Vec::new();
    let mut properties = Vec::new();
    for stmt in body.split([';', '\n']) {
        let stmt = stmt
            .trim()
            .trim_matches(|c: char| c == '{' || c == '}' || c.is_whitespace());
        if stmt.is_empty() || stmt.starts_with("//") {
            continue;
        }
        let text = format!("actor {name}\n{{\n{stmt}\n}}\n");
        if let Some(a) = room_decorate::parse_decorate(&text)
            .ok()
            .and_then(|ir| ir.actors.into_iter().next())
        {
            flags.extend(a.flags);
            properties.extend(a.properties);
        }
    }
    (flags, properties)
}

#[cfg(test)]
mod zscript_parent_tests {
    #![allow(clippy::unwrap_used)]

    #[test]
    fn decorate_actor_may_inherit_a_zscript_class() {
        let mut ir = room_decorate::parse_decorate(
            "actor Weapon {}\nactor PB_Fists : PB_Weapon { States { Spawn: TNT1 A 1 Stop } }\n",
        )
        .unwrap();
        let zs = "class PB_WeaponBase : Weapon { Default { Weapon.SlotNumber 2; +WEAPON.NOAUTOFIRE } }\nclass PB_Weapon : PB_WeaponBase {}\n";
        super::stub_zscript_parents(&mut ir, || room_zscript::parse_file("z.zs", zs).unit);
        let names: Vec<&str> = ir.actors.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Weapon", "PB_WeaponBase", "PB_Weapon", "PB_Fists"]);
        assert_eq!(ir.actors[1].parent.as_deref(), Some("Weapon"));
        assert!(
            ir.actors[1]
                .properties
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case("Weapon.SlotNumber")),
            "{:?}",
            ir.actors[1]
        );
        assert!(!ir.actors[1].flags.is_empty());
        room_sim::ClassRegistry::from_decorate(&ir).unwrap();
    }

    /// PB's monster `Default` blocks mix things DECORATE cannot read (a
    /// custom `PB_Monster.Gibbable 1;`) with `Monster;` and flags; one bad
    /// statement must not drop the rest (PB monsters had no SHOOTABLE).
    #[test]
    fn default_block_keeps_statements_around_unknown_ones() {
        let lines = vec![
            "{".to_owned(),
            "Health 60; Speed 8;".to_owned(),
            "PB_Monster.Gibbable 1, \"x\" ?;".to_owned(),
            "Monster;".to_owned(),
            "+FLOORCLIP -COUNTKILL".to_owned(),
            "}".to_owned(),
        ];
        let (flags, props) = super::default_block_as_decorate("PB_Zombie", &lines);
        let has_prop = |n: &str| props.iter().any(|p| p.name.eq_ignore_ascii_case(n));
        assert!(has_prop("Health") && has_prop("Speed"), "{props:?}");
        let has_flag = |n: &str, set: bool| {
            flags
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(n) && f.set == set)
        };
        assert!(has_flag("FLOORCLIP", true), "{flags:?}");
        assert!(has_flag("COUNTKILL", false), "{flags:?}");
        // `Monster` expands to SHOOTABLE (+ friends) or stays a combo flag.
        assert!(
            has_flag("SHOOTABLE", true) || has_flag("MONSTER", true) || has_prop("Monster"),
            "{flags:?} {props:?}"
        );
    }
}

/// Built-in defs loaded from an engine-data archive (directory or PK3).
#[derive(Clone, Debug)]
pub struct EngineData {
    /// Parsed class registry.
    pub classes: Arc<ClassRegistry>,
    /// Compiled state table.
    pub states: Arc<StateTable>,
    /// Optional IWADINFO (absent if lump missing).
    pub iwadinfo: Option<IwadInfo>,
    /// Optional per-map compatibility flags (`compat/flags.txt`).
    pub compat_db: Option<Arc<room_map::CompatDatabase>>,
    /// Optional per-map geometry/special fixes (`compat/levelfixes.ron`).
    pub level_fixes: Option<Arc<room_map::LevelFixesDatabase>>,
}

/// Load `DECORATE` (+ nested `#include`) and optional `IWADINFO` from `vfs`.
pub fn load_engine_data(vfs: &Vfs) -> Result<EngineData, GameError> {
    let decorate = read_text_lump(vfs, "DECORATE")
        .ok_or_else(|| GameError::Message("engine data: missing DECORATE lump".into()))?;
    load_engine_data_from(vfs, &decorate)
}

fn load_engine_data_from(vfs: &Vfs, decorate: &str) -> Result<EngineData, GameError> {
    let mut ir = parse_decorate_with_includes(decorate, |path| resolve_include(vfs, path))
        .map_err(|e| GameError::Message(format!("DECORATE: {e}")))?;
    let replaced = dedupe_actors_last_wins(&mut ir);
    if replaced > 0 {
        eprintln!("room: DECORATE: {replaced} duplicate class name(s) — later definition wins");
    }
    stub_zscript_parents(&mut ir, || zscript_class_tree(vfs));
    let classes =
        Arc::new(ClassRegistry::from_decorate(&ir).map_err(|e| GameError::Message(e.to_string()))?);
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let iwadinfo = read_text_lump(vfs, "IWADINFO")
        .map(|t| parse_iwadinfo(&t).map_err(|e| GameError::Message(e.to_string())))
        .transpose()?;
    let compat_db = read_text_path(vfs, "compat/flags.txt")
        .or_else(|| read_text_lump(vfs, "FLAGS"))
        .map(|t| {
            room_map::CompatDatabase::parse(&t)
                .map(Arc::new)
                .map_err(|e| GameError::Message(e.to_string()))
        })
        .transpose()?;
    let level_fixes = read_text_path(vfs, "compat/levelfixes.ron")
        .or_else(|| read_text_path(vfs, "compat/LEVELFIXES.RON"))
        .map(|t| {
            room_map::LevelFixesDatabase::parse(&t)
                .map(Arc::new)
                .map_err(|e| GameError::Message(e.to_string()))
        })
        .transpose()?;
    Ok(EngineData {
        classes,
        states,
        iwadinfo,
        compat_db,
        level_fixes,
    })
}

/// Like [`load_engine_data`], but parses **every** `DECORATE` lump in load order
/// (engine → IWAD → PWADs), as GZDoom does, so PWAD actors may add, redefine
/// editor numbers, or `replaces` engine classes.
pub fn load_layered_engine_data(vfs: &Vfs) -> Result<EngineData, GameError> {
    let lumps = vfs
        .find_all("DECORATE", Namespace::Global)
        .collect::<Vec<_>>();
    if lumps.len() <= 1 {
        return load_engine_data(vfs);
    }
    let mut decorate = String::new();
    for id in lumps {
        let bytes = vfs
            .read(id)
            .map_err(|e| GameError::Message(e.to_string()))?;
        decorate.push_str(&String::from_utf8_lossy(&bytes));
        decorate.push('\n');
    }
    load_engine_data_from(vfs, &decorate)
}

/// Later `actor` bodies replace earlier ones with the same name. Position of the
/// first definition is kept so parent classes still appear first.
fn dedupe_actors_last_wins(ir: &mut room_decorate::ClassDefIr) -> usize {
    let mut order: Vec<room_decorate::ActorDef> = Vec::new();
    let mut index = std::collections::BTreeMap::<String, usize>::new();
    let mut replaced = 0usize;
    for actor in ir.actors.drain(..) {
        let key = actor.name.to_ascii_lowercase();
        if let Some(&i) = index.get(&key) {
            order[i] = actor;
            replaced += 1;
        } else {
            index.insert(key, order.len());
            order.push(actor);
        }
    }
    ir.actors = order;
    replaced
}

fn resolve_include(vfs: &Vfs, path: &str) -> Result<String, DecorateError> {
    let normalized = path.replace('\\', "/");
    let candidates = [
        normalized.clone(),
        normalized.to_ascii_lowercase(),
        format!("/{normalized}"),
    ];
    for cand in &candidates {
        if let Some(id) = vfs
            .find_full(cand)
            .or_else(|| vfs.find(cand, Namespace::Global))
            .or_else(|| vfs.find_any_ns(cand))
        {
            let bytes = vfs
                .read(id)
                .map_err(|e| DecorateError::Message(e.to_string()))?;
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    // Basename fallback (short-name lumps).
    if let Some(base) = normalized.rsplit('/').next() {
        if let Some(id) = vfs.find_any_ns(base) {
            let bytes = vfs
                .read(id)
                .map_err(|e| DecorateError::Message(e.to_string()))?;
            return Ok(String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    Err(DecorateError::Message(format!("include not found: {path}")))
}

fn read_text_lump(vfs: &Vfs, name: &str) -> Option<String> {
    let id = vfs
        .find(name, Namespace::Global)
        .or_else(|| vfs.find_any_ns(name))?;
    let bytes = vfs.read(id).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_text_path(vfs: &Vfs, path: &str) -> Option<String> {
    let id = vfs
        .find_full(path)
        .or_else(|| vfs.find_full(&path.to_ascii_lowercase()))
        .or_else(|| vfs.find(path, Namespace::Global))?;
    let bytes = vfs.read(id).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::VfsBuilder;
    use std::path::PathBuf;

    #[test]
    fn load_wadsrc_room_engine_data() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/room");
        if !root.join("decorate.txt").is_file() {
            return;
        }
        let vfs = VfsBuilder::new()
            .mount_path(&root)
            .unwrap()
            .build()
            .unwrap();
        let data = load_engine_data(&vfs).unwrap();
        assert!(data.classes.find_name("ZombieMan").is_some());
        assert!(data.classes.find_name("BossBrain").is_some());
        assert!(data.classes.find_name("ClipBox").is_some());
        assert!(data.classes.find_doomednum(48).is_some()); // TechPillar
        assert!(
            data.classes.find_doomednum(888).is_some(),
            "MBFHelperDog DoomEd 888"
        );
        assert!(data.classes.find_name("MBFHelperDog").is_some());
        let info = data.iwadinfo.unwrap();
        assert!(
            info.records
                .iter()
                .any(|r| r.autoname == "doom.freedoom.phase2")
        );
        assert!(
            data.compat_db.is_some(),
            "wadsrc/room/compat/flags.txt should load"
        );
        assert!(
            data.level_fixes.is_some(),
            "wadsrc/room/compat/levelfixes.ron should load"
        );
    }

    #[test]
    fn load_packed_room_pk3_engine_data() {
        let pk3 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/room.pk3");
        if !pk3.is_file() {
            eprintln!("skip: run scripts/pack-room-pk3.sh first");
            return;
        }
        let vfs = VfsBuilder::new().mount_path(&pk3).unwrap().build().unwrap();
        let data = load_engine_data(&vfs).unwrap();
        assert!(data.classes.find_name("ZombieMan").is_some());
        assert!(data.classes.find_name("DoomImp").is_some());
        assert!(data.iwadinfo.is_some());
    }

    #[test]
    fn load_embedded_room_pk3_engine_data() {
        let bytes = embedded_room_pk3();
        assert!(!bytes.is_empty(), "build.rs should pack wadsrc/room");
        let data = load_embedded_engine_data().unwrap();
        assert!(data.classes.find_name("ZombieMan").is_some());
        assert!(data.classes.find_name("DoomImp").is_some());
        assert!(data.iwadinfo.is_some());
    }

    #[test]
    fn mount_embedded_under_empty_builder_has_sndinfo() {
        let vfs = mount_embedded_engine(VfsBuilder::new())
            .unwrap()
            .build()
            .unwrap();
        assert!(
            vfs.find("SNDINFO", Namespace::Global)
                .or_else(|| vfs.find_any_ns("SNDINFO"))
                .is_some()
        );
    }

    #[test]
    fn mount_optional_lights_dir_exposes_gldefs() {
        let lights = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/m10");
        if !lights.join("lights/GLDEFS").is_file() {
            eprintln!("skip: testdata/m10/lights fixture missing");
            return;
        }
        let vfs = mount_optional_engine_extras(VfsBuilder::new(), &[lights])
            .build()
            .unwrap();
        assert!(
            vfs.find("GLDEFS", Namespace::Global)
                .or_else(|| vfs.find_any_ns("GLDEFS"))
                .is_some(),
            "lights/ fixture should contribute GLDEFS"
        );
        let id = vfs
            .find("GLDEFS", Namespace::Global)
            .or_else(|| vfs.find_any_ns("GLDEFS"))
            .unwrap();
        let text = String::from_utf8_lossy(&vfs.read(id).unwrap()).into_owned();
        assert!(
            text.contains("TestExtraLight"),
            "expected fixture light name in GLDEFS"
        );
    }

    #[test]
    fn lights_fixture_parses_into_definitions() {
        use room_defs::{DefinitionsBuilder, DynLightKind};

        let lights = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/m10");
        if !lights.join("lights/GLDEFS").is_file() {
            eprintln!("skip: testdata/m10/lights fixture missing");
            return;
        }
        let vfs = mount_optional_engine_extras(VfsBuilder::new(), &[lights])
            .build()
            .unwrap();
        let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
        if defs.gldefs.lights.is_empty() {
            panic!(
                "no lights parsed; warnings={:?}; keys={:?}",
                defs.warnings,
                defs.gldefs.lights.keys().collect::<Vec<_>>()
            );
        }
        let point =
            defs.gldefs.lights.get("testextralight").unwrap_or_else(|| {
                panic!("keys={:?}", defs.gldefs.lights.keys().collect::<Vec<_>>())
            });
        assert_eq!(point.kind, DynLightKind::Point);
        let pulse = defs.gldefs.lights.get("testpulselight").unwrap();
        assert_eq!(pulse.kind, DynLightKind::Pulse);
        let flick = defs.gldefs.lights.get("testflickerlight").unwrap();
        assert_eq!(flick.kind, DynLightKind::Flicker);
        assert!(
            !defs.gldefs.object_lights.is_empty(),
            "expected object→light binds from lights fixture"
        );
    }

    #[test]
    fn duplicate_decorate_class_keeps_later_body() {
        use room_sim::ClassRegistry;

        let src = "actor Foo : Actor { Health 1 }\nactor Foo : Actor { Health 9 }\n";
        let mut ir = room_decorate::parse_decorate(src).unwrap();
        assert_eq!(dedupe_actors_last_wins(&mut ir), 1);
        let classes = ClassRegistry::from_decorate(&ir).unwrap();
        let id = classes.find_name("Foo").unwrap();
        assert_eq!(classes.get(id).unwrap().health, 9);
    }
}
