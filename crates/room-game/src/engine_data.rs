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
    let ir = parse_decorate_with_includes(&decorate, |path| resolve_include(vfs, path))
        .map_err(|e| GameError::Message(format!("DECORATE: {e}")))?;
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
}
