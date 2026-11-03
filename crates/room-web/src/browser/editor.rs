//! WAD editor bridge (`editor.html`): resource browser, map document ↔ live
//! 3D / play session, lump and actor overrides, PWAD export.
//!
//! The JS UI owns the map document ([`RawMap`] as JSON). Every edit is pushed
//! back with [`room_editor_update_map`]; property-only changes patch the
//! running world in place, structural ones restart the level.

use super::{APP, Mode, enter_play_mode, set_status};
use crate::editor_pick::{Pick, WallPart, pick};
use crate::play::{EditCam, PlaySession, PlayStartOpts};
use bytes::Bytes;
use glam::{DVec3, Vec3};
use room_game::{
    EngineData, load_embedded_engine_data, load_layered_engine_data, mount_embedded_engine,
};
use room_map::{MapGeometry, RawMap, load_map};
use room_nodebuild::ensure_nodes;
use room_render::{MaterialCatalog, lump_preview_rgba};
use room_sim::{
    ClassDef, FLAG_AMBUSH, FLAG_BOSS, FLAG_COUNTITEM, FLAG_COUNTKILL, FLAG_DONTTHRUST,
    FLAG_DROPOFF, FLAG_FLOAT, FLAG_FLOATBOB, FLAG_FLOORCLIP, FLAG_FRIENDLY, FLAG_INVULNERABLE,
    FLAG_MISSILE, FLAG_NOBLOCKMAP, FLAG_NOBLOOD, FLAG_NODAMAGE, FLAG_NOGRAVITY, FLAG_NORADIUSDMG,
    FLAG_NOTARGET, FLAG_RIPPER, FLAG_SHOOTABLE, FLAG_SOLID, FLAG_SPAWNCEILING,
};
use room_vfs::{ContainerKind, Namespace, Vfs, VfsBuilder, WadWriter, detect_iwad};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

mod scripts;

thread_local! {
    static EDITOR: RefCell<Option<Editor>> = const { RefCell::new(None) };
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
}

/// `true` once the editor page opened a WAD (changes focus / pointer-lock rules).
pub(super) fn active() -> bool {
    ACTIVE.with(Cell::get)
}

/// `true` while the editor shows the frozen fly camera (not play-testing).
pub(super) fn flying() -> bool {
    active()
        && APP.with(|app| {
            app.borrow().as_ref().is_some_and(|s| {
                s.try_borrow()
                    .ok()
                    .is_some_and(|s| matches!(&s.mode, Mode::Play(p) if p.edit_cam.is_some()))
            })
        })
}

/// Numeric / string actor properties exposed to the editor (key, DECORATE name).
const ACTOR_PROPS: &[(&str, &str)] = &[
    ("tag", "Tag"),
    ("health", "Health"),
    ("gib_health", "GibHealth"),
    ("radius", "Radius"),
    ("height", "Height"),
    ("speed", "Speed"),
    ("fast_speed", "FastSpeed"),
    ("float_speed", "FloatSpeed"),
    ("mass", "Mass"),
    ("reaction_time", "ReactionTime"),
    ("pain_chance", "PainChance"),
    ("pain_threshold", "PainThreshold"),
    ("melee_range", "MeleeRange"),
    ("max_target_range", "MaxTargetRange"),
    ("min_missile_chance", "MinMissileChance"),
    ("damage", "Damage"),
    ("damage_factor", "DamageFactor"),
    ("gravity", "Gravity"),
    ("friction", "Friction"),
    ("max_step_height", "MaxStepHeight"),
    ("max_dropoff_height", "MaxDropoffHeight"),
    ("scale", "Scale"),
    ("alpha", "Alpha"),
    ("inventory_amount", "Inventory.Amount"),
    ("inventory_max_amount", "Inventory.MaxAmount"),
    ("ammo_type", "Weapon.AmmoType"),
    ("ammo_use", "Weapon.AmmoUse"),
    ("ammo_give", "Weapon.AmmoGive"),
    ("obituary", "Obituary"),
    ("pickup_message", "Inventory.PickupMessage"),
    ("see_sound", "SeeSound"),
    ("attack_sound", "AttackSound"),
    ("pain_sound", "PainSound"),
    ("death_sound", "DeathSound"),
    ("active_sound", "ActiveSound"),
];

/// Actor flags exposed to the editor.
const ACTOR_FLAGS: &[(&str, u64)] = &[
    ("SOLID", FLAG_SOLID),
    ("SHOOTABLE", FLAG_SHOOTABLE),
    ("COUNTKILL", FLAG_COUNTKILL),
    ("COUNTITEM", FLAG_COUNTITEM),
    ("MISSILE", FLAG_MISSILE),
    ("NOGRAVITY", FLAG_NOGRAVITY),
    ("FLOAT", FLAG_FLOAT),
    ("FLOATBOB", FLAG_FLOATBOB),
    ("SPAWNCEILING", FLAG_SPAWNCEILING),
    ("AMBUSH", FLAG_AMBUSH),
    ("DROPOFF", FLAG_DROPOFF),
    ("FLOORCLIP", FLAG_FLOORCLIP),
    ("NOBLOCKMAP", FLAG_NOBLOCKMAP),
    ("NOBLOOD", FLAG_NOBLOOD),
    ("NODAMAGE", FLAG_NODAMAGE),
    ("INVULNERABLE", FLAG_INVULNERABLE),
    ("NORADIUSDMG", FLAG_NORADIUSDMG),
    ("DONTTHRUST", FLAG_DONTTHRUST),
    ("NOTARGET", FLAG_NOTARGET),
    ("FRIENDLY", FLAG_FRIENDLY),
    ("BOSS", FLAG_BOSS),
    ("RIPPER", FLAG_RIPPER),
];

/// Text lumps the editor opens in the text editor.
const TEXT_LUMPS: &[&str] = &[
    "DECORATE", "MAPINFO", "ZMAPINFO", "UMAPINFO", "SNDINFO", "SNDSEQ", "ANIMDEFS", "GLDEFS",
    "LANGUAGE", "DEHACKED", "TEXTURES", "KEYCONF", "LOCKDEFS", "TERRAIN", "DECALDEF", "MENUDEF",
    "LOADACS", "CVARINFO", "GAMEINFO", "MUSINFO", "REVERBS", "FONTDEFS", "SBARINFO", "IWADINFO",
    "ZSCRIPT", "MODELDEF", "VOXELDEF", "ALTHUDCF", "TEXTCOLO", "SWITCHES", "ANIMATED", "EMAPINFO",
    "MAPINFO", "XLAT", "TEAMINFO", "PALVERS", "DEFBINDS", "DEFCVARS", "SECRETS",
];

/// Lumps belonging to a map (after its marker).
const MAP_LUMPS: &[&str] = &[
    "THINGS", "LINEDEFS", "SIDEDEFS", "VERTEXES", "SEGS", "SSECTORS", "NODES", "SECTORS", "REJECT",
    "BLOCKMAP", "BEHAVIOR", "SCRIPTS", "TEXTMAP", "ZNODES", "DIALOGUE", "ENDMAP", "GL_VERT",
    "GL_SEGS", "GL_SSECT", "GL_NODES", "GL_PVS",
];

struct Editor {
    /// Base WAD as mounted: the IWAD, or the PWAD converted from a Duke GRP.
    iwad: (String, Bytes),
    /// Duke Nukem 3D: the GRP and the user `.MAP` files converted with it.
    duke: Option<DukeSource>,
    pwads: Vec<(String, Bytes)>,
    /// Edited lumps: namespace (`global` / `flats` / `sprites` / `patches`) → name → bytes.
    overrides: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    vfs: Vfs,
    materials: MaterialCatalog,
    /// Layered DECORATE (engine + WADs + overrides), before actor value edits.
    data: EngineData,
    data_error: Option<String>,
    /// Class name → property key → value.
    actor_edits: BTreeMap<String, BTreeMap<String, Value>>,
    doc: Option<RawMap>,
    geom: Option<MapGeometry>,
    /// Per map: `BEHAVIOR` + untouched lumps (not part of the JSON document).
    blobs: BTreeMap<String, MapBlobs>,
    skill: u8,
    /// WASM mod written as `ROOMMOD` + module lumps into the overrides.
    script_mod: scripts::EditorMod,
}

type MapBlobs = (Option<Vec<u8>>, Vec<(String, Vec<u8>)>);
/// GRP name, GRP bytes, user `.MAP` files.
type DukeSource = (String, Bytes, Vec<(String, Vec<u8>)>);

impl Editor {
    fn attach_blobs(&self, doc: &mut RawMap) {
        if let Some((behavior, extra)) = self.blobs.get(&doc.name) {
            if doc.behavior.is_none() {
                doc.behavior.clone_from(behavior);
            }
            if doc.extra_lumps.is_empty() {
                doc.extra_lumps.clone_from(extra);
            }
        }
    }
}

fn err(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn mount(
    iwad: &(String, Bytes),
    pwads: &[(String, Bytes)],
    overrides: &BTreeMap<String, BTreeMap<String, Vec<u8>>>,
) -> Result<Vfs, String> {
    let mut b = mount_embedded_engine(VfsBuilder::new())
        .map_err(|e| e.to_string())?
        .mount_memory(iwad.0.clone(), iwad.1.clone())
        .map_err(|e| e.to_string())?;
    for (name, data) in pwads {
        b = b
            .mount_memory(name.clone(), data.clone())
            .map_err(|e| e.to_string())?;
    }
    if overrides.values().any(|m| !m.is_empty()) {
        let wad = overrides_wad(overrides, None)?;
        b = b
            .mount_memory("editor-overrides.wad", Bytes::from(wad))
            .map_err(|e| e.to_string())?;
    }
    b.build().map_err(|e| e.to_string())
}

/// Override lumps (plus optional extra global lumps) as a PWAD.
fn overrides_wad(
    overrides: &BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    extra: Option<&[(String, Vec<u8>)]>,
) -> Result<Vec<u8>, String> {
    let mut w = WadWriter::pwad();
    if let Some(extra) = extra {
        for (name, data) in extra {
            w.add(name, data.clone());
        }
    }
    for (ns, lumps) in overrides {
        if lumps.is_empty() {
            continue;
        }
        let markers = match ns.as_str() {
            "flats" => Some(("FF_START", "FF_END")),
            "sprites" => Some(("SS_START", "SS_END")),
            "patches" => Some(("PP_START", "PP_END")),
            "textures" => Some(("TX_START", "TX_END")),
            _ => None,
        };
        if let Some((start, _)) = markers {
            w.marker(start);
        }
        for (name, data) in lumps {
            w.add(name, data.clone());
        }
        if let Some((_, end)) = markers {
            w.marker(end);
        }
    }
    w.finish().map_err(|e| e.to_string())
}

fn catalog(vfs: &Vfs) -> MaterialCatalog {
    let mut m = MaterialCatalog::new();
    m.load_playpal(vfs);
    m.load_texturex(vfs);
    m
}

/// Duke GRP + user maps → the converted base PWAD.
fn duke_base(
    name: &str,
    grp: &[u8],
    maps: &[(String, Vec<u8>)],
) -> Result<(String, Bytes), String> {
    let out = room_build::convert_grp_with_maps(grp, maps).map_err(|e| format!("{name}: {e}"))?;
    for w in &out.warnings {
        web_sys::console::warn_1(&format!("{name}: {w}").into());
    }
    Ok((format!("{name}.wad"), Bytes::from(out.wad)))
}

impl Editor {
    fn new(iwad: (String, Bytes)) -> Result<Self, String> {
        let (iwad, duke) = if room_build::is_grp(&iwad.1) {
            (
                duke_base(&iwad.0, &iwad.1, &[])?,
                Some((iwad.0, iwad.1, Vec::new())),
            )
        } else {
            (iwad, None)
        };
        let overrides = BTreeMap::new();
        let vfs = mount(&iwad, &[], &overrides)?;
        let materials = catalog(&vfs);
        let data = load_layered_engine_data(&vfs).map_err(|e| e.to_string())?;
        Ok(Self {
            iwad,
            duke,
            pwads: Vec::new(),
            overrides,
            vfs,
            materials,
            data,
            data_error: None,
            actor_edits: BTreeMap::new(),
            doc: None,
            geom: None,
            blobs: BTreeMap::new(),
            skill: 3,
            script_mod: scripts::EditorMod::default(),
        })
    }

    fn rebuild(&mut self) -> Result<(), String> {
        self.vfs = mount(&self.iwad, &self.pwads, &self.overrides)?;
        self.materials = catalog(&self.vfs);
        match load_layered_engine_data(&self.vfs) {
            Ok(d) => {
                self.data = d;
                self.data_error = None;
            }
            Err(e) => {
                self.data_error = Some(e.to_string());
                self.data = load_embedded_engine_data().map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    /// Engine data with actor value edits applied.
    fn engine_data(&self) -> EngineData {
        let mut data = self.data.clone();
        if !self.actor_edits.is_empty() {
            let classes = Arc::make_mut(&mut data.classes);
            for (name, props) in &self.actor_edits {
                let Some(id) = classes.find_name(name) else {
                    continue;
                };
                if let Some(c) = classes.get_mut(id) {
                    for (k, v) in props {
                        set_actor_prop(c, k, v);
                    }
                }
            }
        }
        data
    }

    fn sprite_names(&self) -> BTreeSet<String> {
        self.vfs
            .iter()
            .filter(|(_, i)| i.namespace == Namespace::Sprites)
            .map(|(_, i)| i.short_name.as_str().to_owned())
            .collect()
    }

    fn spawn_sprite(&self, c: &ClassDef, sprites: &BTreeSet<String>) -> Option<String> {
        let sid = self.data.states.find_label(c.id, "Spawn")?;
        let st = self.data.states.get(sid)?;
        if st.sprite.len() != 4 || st.sprite == "TNT1" {
            return None;
        }
        let frame = char::from(b'A' + st.frame.min(25));
        let prefix = format!("{}{frame}", st.sprite.to_ascii_uppercase());
        for rot in ['0', '1'] {
            let name = format!("{prefix}{rot}");
            if sprites.contains(&name) {
                return Some(name);
            }
        }
        sprites
            .range(prefix.clone()..)
            .next()
            .filter(|n| n.starts_with(&prefix))
            .cloned()
    }

    /// Animation sequences per state label (own labels override inherited),
    /// in Doom order: each walks `next` until Stop, a loop or another label.
    fn class_sequences(&self, class: &ClassDef) -> Vec<(String, Vec<&room_sim::State>)> {
        let states = &self.data.states;
        let mut labels = BTreeMap::new();
        let mut cur = Some(class);
        for _ in 0..32 {
            let Some(c) = cur else {
                break;
            };
            for (l, id) in states.labels_of(c.id) {
                labels.entry(l.to_owned()).or_insert(id);
            }
            cur = c.parent.and_then(|p| self.data.classes.get(p));
        }
        let starts: BTreeSet<u32> = labels.values().map(|s: &_| s.raw()).collect();
        const ORDER: [&str; 9] = [
            "spawn", "see", "melee", "missile", "pain", "death", "xdeath", "raise", "ready",
        ];
        let rank = |l: &str| ORDER.iter().position(|o| *o == l).unwrap_or(ORDER.len());
        let mut keys: Vec<String> = labels.keys().cloned().collect();
        keys.sort_by_key(|l| (rank(l), l.clone()));
        let mut out = Vec::new();
        for label in keys {
            let start = labels[&label];
            let mut seq = Vec::new();
            let mut seen = BTreeSet::new();
            let mut at = Some(start);
            while let Some(id) = at {
                if seq.len() >= 128
                    || !seen.insert(id.raw())
                    || (id != start && starts.contains(&id.raw()))
                {
                    break;
                }
                let Some(st) = states.get(id) else {
                    break;
                };
                seq.push(st);
                at = st.next;
            }
            if seq
                .iter()
                .any(|s| s.sprite.len() == 4 && !s.sprite.eq_ignore_ascii_case("TNT1"))
            {
                let mut name = match label.as_str() {
                    "xdeath" => "XDeath".to_owned(),
                    _ => label,
                };
                if let Some(f) = name.get_mut(0..1) {
                    f.make_ascii_uppercase();
                }
                out.push((name, seq));
            }
        }
        out
    }

    fn summary(&self) -> Value {
        let infos: Vec<_> = self.vfs.iter().collect();
        let mut lumps = Vec::with_capacity(infos.len());
        let mut maps = Vec::new();
        let mut flats = BTreeSet::new();
        let mut patches = BTreeSet::new();
        for (k, (id, info)) in infos.iter().enumerate() {
            let name = info.short_name.as_str();
            let next_is_map = infos.get(k + 1).is_some_and(|(_, n)| {
                n.container == info.container
                    && matches!(n.short_name.as_str(), "THINGS" | "TEXTMAP")
            });
            let src = self
                .vfs
                .container(info.container)
                .map_or("", |c| c.name.as_str());
            let kind = if next_is_map {
                maps.push(name.to_owned());
                "map"
            } else {
                classify(name, &info.full_path, info.namespace)
            };
            match info.namespace {
                Namespace::Flats => {
                    flats.insert(name.to_owned());
                }
                Namespace::Patches => {
                    patches.insert(name.to_owned());
                }
                _ => {}
            }
            lumps.push(json!({
                "id": id.raw(),
                "name": name,
                "ns": info.namespace.as_str(),
                "size": info.size,
                "src": src,
                "path": info.full_path,
                "kind": kind,
            }));
        }
        let mut seen = BTreeSet::new();
        maps.retain(|m| seen.insert(m.clone()));
        let sprites = self.sprite_names();
        let mut things = Vec::new();
        let mut nums = BTreeSet::new();
        for c in self.data.classes.iter() {
            let Some(num) = c.doomednum else {
                continue;
            };
            if self.data.classes.find_doomednum(num) != Some(c.id) {
                continue;
            }
            nums.insert(num);
            things.push(json!({
                "num": num,
                "name": c.name,
                "title": c.display_name(),
                "radius": c.radius,
                "height": c.height,
                "sprite": self.spawn_sprite(c, &sprites),
                "cat": thing_category(num, c),
            }));
        }
        for (num, name) in [
            (1, "Player 1 start"),
            (2, "Player 2 start"),
            (3, "Player 3 start"),
            (4, "Player 4 start"),
            (11, "Deathmatch start"),
        ] {
            if !nums.contains(&num) {
                things.push(json!({
                    "num": num, "name": name, "title": name, "radius": 16.0,
                    "height": 56.0, "sprite": "PLAYA1", "cat": "player",
                }));
            }
        }
        let overrides: Vec<Value> = self
            .overrides
            .iter()
            .flat_map(|(ns, m)| m.keys().map(move |n| json!({"ns": ns, "name": n})))
            .collect();
        let duke = self.duke.as_ref().map(|(grp, _, user)| {
            let user: Vec<String> = user
                .iter()
                .filter_map(|(f, _)| room_build::doom_map_name(f))
                .collect();
            json!({ "grp": grp, "userMaps": user })
        });
        let mut kind_counts = BTreeMap::<String, u32>::new();
        let mut ns_counts = BTreeMap::<String, u32>::new();
        let mut src_bytes = BTreeMap::<String, u64>::new();
        let mut src_lumps = BTreeMap::<String, u32>::new();
        for l in &lumps {
            let kind = l["kind"].as_str().unwrap_or("data");
            *kind_counts.entry(kind.to_owned()).or_default() += 1;
            let ns = l["ns"].as_str().unwrap_or("global");
            *ns_counts.entry(ns.to_owned()).or_default() += 1;
            let src = l["src"].as_str().unwrap_or("");
            let size = l["size"].as_u64().unwrap_or(0);
            *src_bytes.entry(src.to_owned()).or_default() += size;
            *src_lumps.entry(src.to_owned()).or_default() += 1;
        }
        let mut containers = Vec::new();
        for i in 0..self.vfs.container_count() as u32 {
            let id = room_vfs::ContainerId(i);
            let Some(c) = self.vfs.container(id) else {
                continue;
            };
            let kind = match c.kind {
                ContainerKind::Wad => "wad",
                ContainerKind::Zip => "pk3",
                ContainerKind::SevenZ => "pk7",
                ContainerKind::Directory => "dir",
                ContainerKind::LooseFile => "file",
            };
            containers.push(json!({
                "name": c.name,
                "kind": kind,
                "md5": c.md5.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                "lumps": src_lumps.get(&c.name).copied().unwrap_or(0),
                "bytes": src_bytes.get(&c.name).copied().unwrap_or(0),
            }));
        }
        let detected = self.data.iwadinfo.as_ref().and_then(|info| {
            detect_iwad(&self.vfs, info).map(|m| {
                json!({
                    "name": m.record.name,
                    "game": m.record.game,
                    "autoname": m.record.autoname,
                    "config": m.record.config,
                    "mustContain": m.record.must_contain,
                    "names": m.record.names,
                    "filters": m.filters,
                })
            })
        });
        let magic = if self.iwad.1.len() >= 4 {
            String::from_utf8_lossy(&self.iwad.1[..4]).into_owned()
        } else {
            String::new()
        };
        json!({
            "iwad": self.iwad.0,
            "iwadBytes": self.iwad.1.len() as u64,
            "iwadMagic": magic,
            "iwadDetected": detected,
            "containers": containers,
            "kindCounts": kind_counts,
            "nsCounts": ns_counts,
            "duke": duke,
            "pwads": self.pwads.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            "lumps": lumps,
            "maps": maps,
            "textures": self.materials.texture_names().collect::<Vec<_>>(),
            "flats": flats,
            "patches": patches,
            "things": things,
            "overrides": overrides,
            "decorateError": self.data_error,
        })
    }
}

fn classify(name: &str, path: &str, ns: Namespace) -> &'static str {
    match ns {
        Namespace::Flats => return "flat",
        Namespace::Sprites => return "sprite",
        Namespace::Patches => return "patch",
        Namespace::NewTextures => return "texture",
        Namespace::Sounds => return "sound",
        Namespace::Music => return "music",
        Namespace::Graphics => return "graphic",
        Namespace::Acs => return "acs",
        _ => {}
    }
    let lower = path.to_ascii_lowercase();
    let stem = name.split('.').next().unwrap_or(name);
    if TEXT_LUMPS.contains(&stem)
        || [
            ".txt", ".dec", ".acs", ".zs", ".zc", ".zsc", ".cfg", ".ini", ".json", ".ron", ".deh",
            ".bex",
        ]
        .iter()
        .any(|e| lower.ends_with(e))
    {
        "text"
    } else if MAP_LUMPS.contains(&name) || name.starts_with("GL_") {
        "maplump"
    } else if matches!(name, "PLAYPAL" | "COLORMAP") {
        "palette"
    } else if matches!(name, "TEXTURE1" | "TEXTURE2" | "PNAMES") {
        "texdefs"
    } else if name.starts_with("DS") || name.starts_with("DP") || lower.ends_with(".wav") {
        "sound"
    } else if name.starts_with("D_")
        || name.starts_with("MUS_")
        || [
            ".mid", ".mus", ".ogg", ".mp3", ".mod", ".xm", ".it", ".s3m", ".flac",
        ]
        .iter()
        .any(|e| lower.ends_with(e))
    {
        "music"
    } else if [".png", ".jpg", ".jpeg", ".tga", ".bmp", ".lmp"]
        .iter()
        .any(|e| lower.ends_with(e))
    {
        "graphic"
    } else {
        "data"
    }
}

fn thing_category(num: i32, c: &ClassDef) -> &'static str {
    if matches!(num, 1..=4 | 11) {
        "player"
    } else if c.flags & FLAG_COUNTKILL != 0 {
        "monster"
    } else if c.is_weapon {
        "weapon"
    } else if c.flags & FLAG_COUNTITEM != 0 || !c.pickup_message.is_empty() {
        "pickup"
    } else if c.name.contains("Teleport") || c.radius < 1.0 && c.flags & FLAG_SOLID == 0 {
        "special"
    } else if c.flags & FLAG_SOLID != 0 {
        "obstacle"
    } else {
        "decoration"
    }
}

fn actor_prop(c: &ClassDef, key: &str) -> Value {
    match key {
        "tag" => json!(c.tag),
        "health" => json!(c.health),
        "gib_health" => json!(c.gib_health),
        "radius" => json!(c.radius),
        "height" => json!(c.height),
        "speed" => json!(c.speed),
        "fast_speed" => json!(c.fast_speed),
        "float_speed" => json!(c.float_speed),
        "mass" => json!(c.mass),
        "reaction_time" => json!(c.reaction_time),
        "pain_chance" => json!(c.pain_chance),
        "pain_threshold" => json!(c.pain_threshold),
        "melee_range" => json!(c.melee_range),
        "max_target_range" => json!(c.max_target_range),
        "min_missile_chance" => json!(c.min_missile_chance),
        "damage" => json!(c.damage),
        "damage_factor" => json!(c.damage_factor),
        "gravity" => json!(c.gravity),
        "friction" => json!(c.friction),
        "max_step_height" => json!(c.max_step_height),
        "max_dropoff_height" => json!(c.max_dropoff_height),
        "scale" => json!(c.scale_x),
        "alpha" => json!(c.alpha),
        "inventory_amount" => json!(c.inventory_amount),
        "inventory_max_amount" => json!(c.inventory_max_amount),
        "ammo_type" => json!(c.ammo_type),
        "ammo_use" => json!(c.ammo_use),
        "ammo_give" => json!(c.ammo_give),
        "obituary" => json!(c.obituary),
        "pickup_message" => json!(c.pickup_message),
        "see_sound" => json!(c.see_sound),
        "attack_sound" => json!(c.attack_sound),
        "pain_sound" => json!(c.pain_sound),
        "death_sound" => json!(c.death_sound),
        "active_sound" => json!(c.active_sound),
        _ => ACTOR_FLAGS
            .iter()
            .find(|(n, _)| key.strip_prefix('+') == Some(*n))
            .map_or(Value::Null, |(_, bit)| json!(c.flags & bit != 0)),
    }
}

fn set_actor_prop(c: &mut ClassDef, key: &str, v: &Value) {
    let f = v.as_f64().unwrap_or(0.0);
    let i = f.round() as i32;
    let s = || v.as_str().unwrap_or_default().to_owned();
    match key {
        "tag" => c.tag = s(),
        "health" => c.health = i,
        "gib_health" => c.gib_health = i,
        "radius" => c.radius = f,
        "height" => c.height = f,
        "speed" => c.speed = f,
        "fast_speed" => c.fast_speed = f,
        "float_speed" => c.float_speed = f,
        "mass" => c.mass = i,
        "reaction_time" => c.reaction_time = i,
        "pain_chance" => c.pain_chance = i,
        "pain_threshold" => c.pain_threshold = i,
        "melee_range" => c.melee_range = f,
        "max_target_range" => c.max_target_range = f,
        "min_missile_chance" => c.min_missile_chance = i,
        "damage" => c.damage = i,
        "damage_factor" => c.damage_factor = f,
        "gravity" => c.gravity = f,
        "friction" => c.friction = f,
        "max_step_height" => c.max_step_height = f,
        "max_dropoff_height" => c.max_dropoff_height = f,
        "scale" => {
            c.scale_x = f;
            c.scale_y = f;
        }
        "alpha" => c.alpha = f,
        "inventory_amount" => c.inventory_amount = i,
        "inventory_max_amount" => c.inventory_max_amount = i,
        "ammo_type" => c.ammo_type = s(),
        "ammo_use" => c.ammo_use = i,
        "ammo_give" => c.ammo_give = i,
        "obituary" => c.obituary = s(),
        "pickup_message" => c.pickup_message = s(),
        "see_sound" => c.see_sound = s(),
        "attack_sound" => c.attack_sound = s(),
        "pain_sound" => c.pain_sound = s(),
        "death_sound" => c.death_sound = s(),
        "active_sound" => c.active_sound = s(),
        _ => {
            if let Some((_, bit)) = ACTOR_FLAGS
                .iter()
                .find(|(n, _)| key.strip_prefix('+') == Some(*n))
            {
                if v.as_bool().unwrap_or(false) {
                    c.flags |= bit;
                } else {
                    c.flags &= !bit;
                }
            }
        }
    }
}

/// `actor X_Edited : X replaces X { … }` for every edited class.
fn actor_edits_decorate(edits: &BTreeMap<String, BTreeMap<String, Value>>) -> String {
    let mut out = String::from("// Generated by the Room web editor.\n");
    for (class, props) in edits {
        if props.is_empty() {
            continue;
        }
        let _ = writeln!(out, "\nactor {class}_Edited : {class} replaces {class}\n{{");
        for (key, v) in props {
            if let Some(flag) = key.strip_prefix('+') {
                let sign = if v.as_bool().unwrap_or(false) {
                    '+'
                } else {
                    '-'
                };
                let _ = writeln!(out, "  {sign}{flag}");
            } else if let Some((_, deco)) = ACTOR_PROPS.iter().find(|(k, _)| k == key) {
                match v {
                    Value::String(s) => {
                        let _ = writeln!(out, "  {deco} \"{}\"", s.replace('"', "'"));
                    }
                    other => {
                        let _ = writeln!(out, "  {deco} {other}");
                    }
                }
            }
        }
        out.push_str("}\n");
    }
    out
}

/// Same vertices, line ends / side refs and side sectors (nodes stay valid).
fn same_topology(a: &RawMap, b: &RawMap) -> bool {
    a.format == b.format
        && a.vertices == b.vertices
        && a.linedefs.len() == b.linedefs.len()
        && a.sidedefs.len() == b.sidedefs.len()
        && a.sectors.len() == b.sectors.len()
        && a.linedefs.iter().zip(&b.linedefs).all(|(x, y)| {
            x.v1 == y.v1
                && x.v2 == y.v2
                && x.side_front == y.side_front
                && x.side_back == y.side_back
        })
        && a.sidedefs
            .iter()
            .zip(&b.sidedefs)
            .all(|(x, y)| x.sector == y.sector)
}

/// Topology plus everything the level only reads at spawn (things, specials, slopes).
fn live_patchable(a: &RawMap, b: &RawMap) -> bool {
    same_topology(a, b)
        && a.things == b.things
        && a.linedefs
            .iter()
            .zip(&b.linedefs)
            .all(|(x, y)| x.special == y.special && x.args == y.args && x.tag == y.tag)
        && a.sectors.iter().zip(&b.sectors).all(|(x, y)| {
            x.special == y.special
                && x.tag == y.tag
                && x.floor_plane == y.floor_plane
                && x.ceiling_plane == y.ceiling_plane
        })
}

fn with_editor<T>(f: impl FnOnce(&mut Editor) -> Result<T, String>) -> Result<T, JsValue> {
    EDITOR.with(|e| {
        let mut slot = e.borrow_mut();
        let ed = slot
            .as_mut()
            .ok_or_else(|| JsValue::from_str("editor: open a WAD first"))?;
        f(ed).map_err(|e| JsValue::from_str(&e))
    })
}

fn with_play<T>(f: impl FnOnce(&mut PlaySession) -> T) -> Option<T> {
    let state = APP.with(|a| a.borrow().clone())?;
    let mut s = state.try_borrow_mut().ok()?;
    match &mut s.mode {
        Mode::Play(play) => Some(f(play)),
        _ => None,
    }
}

/// Start a fresh session on `geom` with the editor's VFS + actor edits.
fn start_session(ed: &Editor, geom: MapGeometry, cam: Option<EditCam>) -> Result<(), String> {
    let state = APP
        .with(|a| a.borrow().clone())
        .ok_or_else(|| "renderer not ready".to_owned())?;
    let vfs = mount(&ed.iwad, &ed.pwads, &ed.overrides)?;
    let mut play = PlaySession::from_parts(
        vfs,
        geom,
        PlayStartOpts {
            skill: ed.skill,
            ..PlayStartOpts::default()
        },
        ed.engine_data(),
    )?;
    let cam = cam.or_else(|| {
        play.current_eye().map(|mut c| {
            let ceiling =
                room_nodebuild::point_in_sector(&play.map, f64::from(c.pos.x), f64::from(c.pos.y))
                    .and_then(|s| play.map.sectors.get(s.raw() as usize))
                    .map_or(f64::INFINITY, |s| s.ceiling_height);
            c.pos.z = (c.pos.z + 24.0).min((ceiling - 8.0) as f32).max(c.pos.z);
            c
        })
    });
    play.set_edit_cam(cam);
    enter_play_mode(&state, play).map_err(|e| e.as_string().unwrap_or_default())
}

fn rgba_js(w: u32, h: u32, rgba: &[u8]) -> JsValue {
    let o = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&o, &"w".into(), &w.into());
    let _ = js_sys::Reflect::set(&o, &"h".into(), &h.into());
    let _ = js_sys::Reflect::set(
        &o,
        &"rgba".into(),
        &js_sys::Uint8ClampedArray::from(rgba).into(),
    );
    o.into()
}

/// Open an IWAD; returns the resource summary JSON.
#[wasm_bindgen(js_name = roomEditorOpen)]
pub fn room_editor_open(iwad: &[u8], name: &str) -> Result<String, JsValue> {
    let ed = Editor::new((name.to_owned(), Bytes::copy_from_slice(iwad))).map_err(err)?;
    let summary = ed.summary().to_string();
    EDITOR.with(|e| *e.borrow_mut() = Some(ed));
    ACTIVE.with(|a| a.set(true));
    set_status("Editor ready");
    Ok(summary)
}

/// Mount a PWAD / PK3 on top (load order = call order); returns the summary.
/// With a Duke GRP open, a Build `.MAP` is converted with the GRP's content.
#[wasm_bindgen(js_name = roomEditorAddPwad)]
pub fn room_editor_add_pwad(data: &[u8], name: &str) -> Result<String, JsValue> {
    with_editor(|ed| {
        if room_build::is_build_map(data) {
            let Some((grp_name, grp, maps)) = ed.duke.as_mut() else {
                return Err("a Build .MAP needs DUKE3D.GRP opened first".to_owned());
            };
            let file = name.to_ascii_uppercase();
            maps.retain(|(n, _)| !n.eq_ignore_ascii_case(&file));
            maps.push((file, data.to_vec()));
            let base = duke_base(grp_name, grp, maps);
            match base {
                Ok(b) => ed.iwad = b,
                Err(e) => {
                    maps.pop();
                    return Err(e);
                }
            }
            ed.rebuild()?;
            return Ok(ed.summary().to_string());
        }
        ed.pwads
            .push((name.to_owned(), Bytes::copy_from_slice(data)));
        if let Err(e) = ed.rebuild() {
            ed.pwads.pop();
            ed.rebuild()?;
            return Err(e);
        }
        Ok(ed.summary().to_string())
    })
}

/// Unmount a previously added PWAD / PK3 by name (case-insensitive).
/// Returns the new summary JSON.
#[wasm_bindgen(js_name = roomEditorRemovePwad)]
pub fn room_editor_remove_pwad(name: &str) -> Result<String, JsValue> {
    with_editor(|ed| {
        let before = ed.pwads.len();
        ed.pwads.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        if ed.pwads.len() == before {
            // Also allow removing a Duke user .MAP that was folded into the GRP convert.
            if let Some((grp_name, grp, maps)) = ed.duke.as_mut() {
                let file = name.to_ascii_uppercase();
                let had = maps.len();
                maps.retain(|(n, _)| !n.eq_ignore_ascii_case(&file));
                if maps.len() == had {
                    return Err(format!("not mounted: {name}"));
                }
                ed.iwad = duke_base(grp_name, grp, maps)?;
            } else {
                return Err(format!("not mounted: {name}"));
            }
        }
        ed.rebuild()?;
        Ok(ed.summary().to_string())
    })
}

/// Resource summary JSON (lumps, maps, textures, flats, thing types, overrides).
#[wasm_bindgen(js_name = roomEditorSummary)]
pub fn room_editor_summary() -> Result<String, JsValue> {
    with_editor(|ed| Ok(ed.summary().to_string()))
}

/// Raw lump bytes by VFS id.
#[wasm_bindgen(js_name = roomEditorReadLump)]
pub fn room_editor_read_lump(id: u32) -> Result<Vec<u8>, JsValue> {
    with_editor(|ed| {
        ed.vfs
            .read(room_vfs::LumpId(id))
            .map(|b| b.to_vec())
            .map_err(|e| e.to_string())
    })
}

/// Decode a lump as a picture (patch / flat / PNG …): `{w, h, rgba}` or `null`.
#[wasm_bindgen(js_name = roomEditorPreviewLump)]
pub fn room_editor_preview_lump(id: u32) -> Result<JsValue, JsValue> {
    with_editor(|ed| {
        let data = ed
            .vfs
            .read(room_vfs::LumpId(id))
            .map_err(|e| e.to_string())?;
        Ok(lump_preview_rgba(&data, ed.materials.playpal())
            .map_or(JsValue::NULL, |(w, h, px)| rgba_js(w, h, &px)))
    })
}

/// Texture / flat / sprite / patch picture by name: `{w, h, rgba}` or `null`.
#[wasm_bindgen(js_name = roomEditorPreview)]
pub fn room_editor_preview(name: &str) -> Result<JsValue, JsValue> {
    with_editor(|ed| {
        if let Some((w, h, px)) = ed.materials.preview_rgba(&ed.vfs, name) {
            return Ok(rgba_js(w, h, &px));
        }
        let found = ed
            .vfs
            .find(name, Namespace::Flats)
            .or_else(|| ed.vfs.find(name, Namespace::Sprites))
            .or_else(|| ed.vfs.find_any_ns(name));
        Ok(found
            .and_then(|id| ed.vfs.read(id).ok())
            .and_then(|d| lump_preview_rgba(&d, ed.materials.playpal()))
            .map_or(JsValue::NULL, |(w, h, px)| rgba_js(w, h, &px)))
    })
}

/// RGBA preview of a patch by name (patch namespace first, never a texture),
/// for composing TEXTURE1/TEXTURE2 entries.
#[wasm_bindgen(js_name = roomEditorPreviewPatch)]
pub fn room_editor_preview_patch(name: &str) -> Result<JsValue, JsValue> {
    with_editor(|ed| {
        let found = ed
            .vfs
            .find(name, Namespace::Patches)
            .or_else(|| ed.vfs.find(name, Namespace::Global))
            .or_else(|| ed.vfs.find_any_ns(name));
        Ok(found
            .and_then(|id| ed.vfs.read(id).ok())
            .and_then(|d| lump_preview_rgba(&d, ed.materials.playpal()))
            .map_or(JsValue::NULL, |(w, h, px)| rgba_js(w, h, &px)))
    })
}

/// Decode a sound lump: `{rate, channels, samples: Float32Array}`.
#[wasm_bindgen(js_name = roomEditorDecodeSound)]
pub fn room_editor_decode_sound(id: u32) -> Result<JsValue, JsValue> {
    with_editor(|ed| {
        let data = ed
            .vfs
            .read(room_vfs::LumpId(id))
            .map_err(|e| e.to_string())?;
        let snd = room_formats::decode_sound(&data).map_err(|e| e.to_string())?;
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&o, &"rate".into(), &snd.rate.into());
        let _ = js_sys::Reflect::set(&o, &"channels".into(), &snd.channels.into());
        let _ = js_sys::Reflect::set(
            &o,
            &"samples".into(),
            &js_sys::Float32Array::from(snd.samples.as_slice()).into(),
        );
        Ok(o.into())
    })
}

/// Resolve a logical SNDINFO name to a lump: `{lump, id}` (`id` null if missing).
#[wasm_bindgen(js_name = roomEditorResolveSound)]
pub fn room_editor_resolve_sound(logical: &str) -> Result<JsValue, JsValue> {
    with_editor(|ed| {
        let mut combined = String::new();
        for id in ed.vfs.find_all("SNDINFO", Namespace::Global) {
            if let Ok(bytes) = ed.vfs.read(id) {
                combined.push_str(&String::from_utf8_lossy(&bytes));
                combined.push('\n');
            }
        }
        if combined.is_empty() {
            if let Some(id) = ed.vfs.find_any_ns("SNDINFO") {
                if let Ok(bytes) = ed.vfs.read(id) {
                    combined.push_str(&String::from_utf8_lossy(&bytes));
                }
            }
        }
        let reg = if combined.is_empty() {
            room_defs::SoundRegistry::default()
        } else {
            room_defs::parse_sndinfo(&combined).unwrap_or_default()
        };
        let mut lump = reg.resolve_lump(logical, 0).map(|s| s.to_ascii_uppercase());
        if lump.is_none() {
            let cand = logical.to_ascii_uppercase();
            if ed.vfs.find(&cand, Namespace::Sounds).is_some()
                || ed.vfs.find(&cand, Namespace::Global).is_some()
                || ed.vfs.find_any_ns(&cand).is_some()
            {
                lump = Some(cand);
            }
        }
        let id = lump.as_ref().and_then(|n| {
            ed.vfs
                .find(n, Namespace::Sounds)
                .or_else(|| ed.vfs.find(n, Namespace::Global))
                .or_else(|| ed.vfs.find_any_ns(n))
                .map(|lid| lid.0)
        });
        let o = js_sys::Object::new();
        match &lump {
            Some(n) => {
                let _ = js_sys::Reflect::set(&o, &"lump".into(), &n.into());
            }
            None => {
                let _ = js_sys::Reflect::set(&o, &"lump".into(), &JsValue::NULL);
            }
        }
        match id {
            Some(i) => {
                let _ = js_sys::Reflect::set(&o, &"id".into(), &JsValue::from_f64(f64::from(i)));
            }
            None => {
                let _ = js_sys::Reflect::set(&o, &"id".into(), &JsValue::NULL);
            }
        }
        Ok(o.into())
    })
}

/// Play a music lump through the live session's synth (`""` stops).
#[wasm_bindgen(js_name = roomEditorPlayMusic)]
pub fn room_editor_play_music(name: &str) {
    with_play(|play| {
        play.ensure_audio();
        play.play_music_named(name);
    });
}

/// Load `name` as the editable document (JSON) and show it in the 3D view.
#[wasm_bindgen(js_name = roomEditorLoadMap)]
pub fn room_editor_load_map(name: &str) -> Result<String, JsValue> {
    with_editor(|ed| {
        let doc = RawMap::load(&ed.vfs, name).map_err(|e| e.to_string())?;
        ed.blobs.insert(
            doc.name.clone(),
            (doc.behavior.clone(), doc.extra_lumps.clone()),
        );
        let mut geom = load_map(&ed.vfs, name).map_err(|e| e.to_string())?;
        ensure_nodes(&mut geom);
        start_session(ed, geom.clone(), None)?;
        ed.doc = Some(doc.clone());
        ed.geom = Some(geom);
        serde_json::to_string(&doc).map_err(|e| e.to_string())
    })
}

/// Brand-new empty map document (`format`: `doom` / `hexen` / `udmf`).
#[wasm_bindgen(js_name = roomEditorNewMap)]
pub fn room_editor_new_map(name: &str, format: &str) -> Result<String, JsValue> {
    let format = match format {
        "hexen" => room_map::MapFormat::Hexen,
        "udmf" => room_map::MapFormat::Udmf,
        _ => room_map::MapFormat::Doom,
    };
    let doc = room_map::RawMap::starter_room(name, format);
    with_editor(|ed| {
        ed.blobs
            .insert(doc.name.clone(), (doc.behavior.clone(), Vec::new()));
        Ok(())
    })?;
    let json = serde_json::to_string(&doc).map_err(err)?;
    room_editor_update_map(&json)?;
    Ok(json)
}

/// Push the edited document. Returns `"live"`, `"restart"` or `"rebuild"`.
#[wasm_bindgen(js_name = roomEditorUpdateMap)]
pub fn room_editor_update_map(json: &str) -> Result<String, JsValue> {
    let mut doc: RawMap = serde_json::from_str(json).map_err(err)?;
    with_editor(|ed| {
        ed.attach_blobs(&mut doc);
        let mut geom = doc.to_geometry().map_err(|e| e.to_string())?;
        let (live, how) = match (ed.doc.as_ref(), ed.geom.as_ref()) {
            (Some(old), Some(old_geom)) if old.name == doc.name && same_topology(old, &doc) => {
                // Reuse the BSP (and its split vertices).
                geom.vertices.extend_from_slice(
                    &old_geom.vertices[doc.vertices.len().min(old_geom.vertices.len())..],
                );
                geom.game_nodes.clone_from(&old_geom.game_nodes);
                geom.gl_nodes.clone_from(&old_geom.gl_nodes);
                if live_patchable(old, &doc) {
                    (true, "live")
                } else {
                    (false, "restart")
                }
            }
            _ => {
                ensure_nodes(&mut geom);
                (false, "rebuild")
            }
        };
        let same_map = ed.doc.as_ref().is_some_and(|d| d.name == doc.name);
        let applied = if same_map {
            with_play(|play| play.editor_apply(geom.clone(), live))
        } else {
            None
        };
        match applied {
            Some(r) => r?,
            None => start_session(ed, geom.clone(), None)?,
        }
        ed.doc = Some(doc);
        ed.geom = Some(geom);
        Ok(how.to_owned())
    })
}

/// `edit` (fly camera), `start` (play from the player start), `here` (play
/// from the camera), `resume` (continue the frozen world).
#[wasm_bindgen(js_name = roomEditorPlay)]
pub fn room_editor_play(mode: &str) -> Result<(), JsValue> {
    let geom = with_editor(|ed| ed.geom.clone().ok_or_else(|| "no map loaded".to_owned()))?;
    with_play(|play| -> Result<(), String> {
        match mode {
            "edit" => {
                let cam = play.current_eye();
                play.set_edit_cam(cam);
            }
            "resume" => play.set_edit_cam(None),
            "here" => {
                let at = play.current_eye().map(|c| {
                    (
                        c.pos.x.round() as i32,
                        c.pos.y.round() as i32,
                        (c.yaw.to_degrees().round() as i32).rem_euclid(360) as i16,
                    )
                });
                play.set_edit_cam(None);
                play.restart_level(geom, at)?;
            }
            _ => {
                play.set_edit_cam(None);
                play.restart_level(geom, None)?;
            }
        }
        play.ensure_audio();
        Ok(())
    })
    .ok_or_else(|| JsValue::from_str("no session"))?
    .map_err(err)
}

/// Fly-camera mouse look (pixels).
#[wasm_bindgen(js_name = roomEditorLook)]
pub fn room_editor_look(dx: f64, dy: f64) {
    with_play(|play| play.edit_look(dx, dy));
}

/// Current view: `{x, y, z, yaw, pitch, playing}` (yaw/pitch radians).
#[wasm_bindgen(js_name = roomEditorCamera)]
pub fn room_editor_camera() -> JsValue {
    with_play(|play| {
        play.current_eye().map(|c| {
            json!({
                "x": c.pos.x, "y": c.pos.y, "z": c.pos.z,
                "yaw": c.yaw, "pitch": c.pitch,
                "playing": play.edit_cam.is_none(),
            })
            .to_string()
        })
    })
    .flatten()
    .map_or(JsValue::NULL, |s| JsValue::from_str(&s))
}

/// Move the fly camera (e.g. double-click in the 2D view). `z = NaN` keeps
/// eye height above the floor at `(x, y)`.
#[wasm_bindgen(js_name = roomEditorSetCamera)]
pub fn room_editor_set_camera(x: f64, y: f64, z: f64, yaw: f64) {
    with_play(|play| {
        let z = if z.is_finite() {
            z
        } else {
            room_nodebuild::point_in_sector(&play.map, x, y)
                .and_then(|s| play.map.sectors.get(s.raw() as usize))
                .map_or(41.0, |s| s.floor_height + 41.0)
        };
        let pitch = play.edit_cam.map_or(0.0, |c| c.pitch);
        play.set_edit_cam(Some(EditCam {
            pos: Vec3::new(x as f32, y as f32, z as f32),
            yaw,
            pitch,
        }));
    });
}

/// Pick at normalised viewport coords (`nx`, `ny` in −1..1, `ny` up).
/// Returns JSON `{kind: "thing"|"wall"|"floor"|"ceiling", …}` or `null`.
#[wasm_bindgen(js_name = roomEditorPick)]
pub fn room_editor_pick(nx: f64, ny: f64) -> JsValue {
    let Some(state) = APP.with(|a| a.borrow().clone()) else {
        return JsValue::NULL;
    };
    let Ok(s) = state.try_borrow() else {
        return JsValue::NULL;
    };
    let Mode::Play(play) = &s.mode else {
        return JsValue::NULL;
    };
    let cam = s.camera;
    let f = cam.forward.as_dvec3().normalize_or_zero();
    let right = f.cross(DVec3::Z).normalize_or_zero();
    let up = right.cross(f);
    let ty = (cam.fov_y_deg.to_radians() * 0.5).tan();
    let dir = f + right * (nx * ty * cam.aspect) + up * (ny * ty);
    let hit = pick(&play.map, &play.thing_boxes(), cam.eye.as_dvec3(), dir);
    let v = match hit {
        Some(Pick::Thing(i)) => json!({"kind": "thing", "index": i}),
        Some(Pick::Wall { line, side, part }) => json!({
            "kind": "wall", "index": line, "side": side,
            "part": match part { WallPart::Upper => "upper", WallPart::Middle => "middle", WallPart::Lower => "lower" },
        }),
        Some(Pick::Plane { sector, ceiling }) => json!({
            "kind": if ceiling { "ceiling" } else { "floor" }, "index": sector,
        }),
        None => return JsValue::NULL,
    };
    JsValue::from_str(&v.to_string())
}

/// Screen rectangles of map things in normalised viewport coords (−1..1, `y`
/// up), flat `[index, x0, y0, x1, y1, visible, …]` where `visible` is 1 when
/// no wall / plane hides the thing's centre. Things wholly or partly behind
/// the camera are skipped.
#[wasm_bindgen(js_name = roomEditorThingRects)]
pub fn room_editor_thing_rects(indices: &[u32]) -> Vec<f32> {
    let Some(state) = APP.with(|a| a.borrow().clone()) else {
        return Vec::new();
    };
    let Ok(s) = state.try_borrow() else {
        return Vec::new();
    };
    let Mode::Play(play) = &s.mode else {
        return Vec::new();
    };
    let vp = s.camera.view_proj();
    let eye = s.camera.eye.as_dvec3();
    let mut out = Vec::with_capacity(indices.len() * 6);
    'things: for &i in indices {
        let Some(b) = play.thing_box(i as usize) else {
            continue;
        };
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for k in 0..8 {
            let sx = if k & 1 == 0 { -b.radius } else { b.radius };
            let sy = if k & 2 == 0 { -b.radius } else { b.radius };
            let z = if k & 4 == 0 { 0.0 } else { b.height };
            let p = (b.pos + DVec3::new(sx, sy, z)).as_vec3();
            let c = vp * p.extend(1.0);
            if c.w <= 1.0 {
                continue 'things;
            }
            let (nx, ny) = (c.x / c.w, c.y / c.w);
            x0 = x0.min(nx);
            y0 = y0.min(ny);
            x1 = x1.max(nx);
            y1 = y1.max(ny);
        }
        if x1 < -1.0 || x0 > 1.0 || y1 < -1.0 || y0 > 1.0 {
            continue;
        }
        let centre = b.pos + DVec3::new(0.0, 0.0, b.height * 0.5);
        let visible = matches!(
            pick(&play.map, std::slice::from_ref(&b), eye, centre - eye),
            Some(Pick::Thing(_))
        );
        out.extend_from_slice(&[i as f32, x0, y0, x1, y1, f32::from(u8::from(visible))]);
    }
    out
}

/// Override (or with `null`, revert) a lump. Call [`room_editor_apply_lumps`] after.
#[wasm_bindgen(js_name = roomEditorSetLump)]
pub fn room_editor_set_lump(ns: &str, name: &str, data: Option<Vec<u8>>) -> Result<(), JsValue> {
    with_editor(|ed| {
        let ns = image_ns(ns);
        let name = name.to_ascii_uppercase();
        if name.is_empty() || name.len() > 8 {
            return Err(format!("lump name must be 1–8 characters: {name}"));
        }
        let m = ed.overrides.entry(ns.to_owned()).or_default();
        match data {
            Some(d) => {
                m.insert(name, d);
            }
            None => {
                m.remove(&name);
            }
        }
        Ok(())
    })
}

/// Override namespace (`textures` = `TX_START`, …); anything else is `global`.
fn image_ns(ns: &str) -> &'static str {
    match ns {
        "flats" => "flats",
        "sprites" => "sprites",
        "patches" => "patches",
        "textures" => "textures",
        _ => "global",
    }
}

/// Decode a picture lump; raw flats are recognised by size in the flat namespace.
fn decode_picture(data: &[u8], raw_flat_first: bool) -> Option<room_formats::Image> {
    let raw = || {
        let dims = room_formats::flat_dimensions(data.len())?;
        Some(room_formats::Image {
            left_offset: 0,
            top_offset: 0,
            has_translucency: false,
            pixels: room_formats::ImagePixels::Paletted(room_formats::PalettedImage {
                width: dims.width,
                height: dims.height,
                indices: data.to_vec(),
                alpha: Vec::new(),
            }),
        })
    };
    if raw_flat_first && !room_formats::is_png(data) {
        if let Some(img) = raw() {
            return Some(img);
        }
    }
    room_formats::decode_image(data).ok().or_else(raw)
}

/// First PLAYPAL palette (greyscale ramp when the WAD has none).
fn palette_rgb(ed: &Editor) -> [u8; 768] {
    ed.materials
        .playpal()
        .and_then(|p| p.palettes.first())
        .map_or_else(
            || {
                let mut g = [0u8; 768];
                for (i, c) in g.chunks_exact_mut(3).enumerate() {
                    #[allow(clippy::cast_possible_truncation)]
                    c.fill(i as u8);
                }
                g
            },
            |p| p.rgb,
        )
}

/// Nearest palette index for an RGB colour.
fn nearest_index(pal: &[u8; 768], r: u8, g: u8, b: u8) -> u8 {
    let mut best = (u32::MAX, 0u8);
    for (i, c) in pal.chunks_exact(3).enumerate() {
        let d = |a: u8, b: u8| u32::from(a.abs_diff(b)).pow(2);
        let dist = d(c[0], r) + d(c[1], g) + d(c[2], b);
        if dist < best.0 {
            #[allow(clippy::cast_possible_truncation)]
            {
                best = (dist, i as u8);
            }
        }
    }
    best.1
}

/// `(indices, alpha)` for any decoded picture (true-colour is quantised).
fn to_paletted(img: &room_formats::Image, pal: &[u8; 768]) -> (Vec<u8>, Vec<u8>) {
    match &img.pixels {
        room_formats::ImagePixels::Paletted(p) => {
            let alpha = if p.alpha.len() == p.indices.len() {
                p.alpha
                    .iter()
                    .map(|&a| if a >= 128 { 255 } else { 0 })
                    .collect()
            } else if img.has_translucency {
                p.indices
                    .iter()
                    .map(|&i| if i == 0 { 0 } else { 255 })
                    .collect()
            } else {
                vec![255; p.indices.len()]
            };
            (p.indices.clone(), alpha)
        }
        room_formats::ImagePixels::Rgba8(p) => p
            .pixels
            .chunks_exact(4)
            .map(|c| {
                if c[3] < 128 {
                    (0, 0)
                } else {
                    (nearest_index(pal, c[0], c[1], c[2]), 255)
                }
            })
            .unzip(),
    }
}

/// Expand palette indices to RGBA (holes stay transparent unless `opaque`).
fn paletted_rgba(pal: &[u8; 768], indices: &[u8], alpha: &[u8], opaque: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(indices.len() * 4);
    for (i, &idx) in indices.iter().enumerate() {
        let o = usize::from(idx) * 3;
        let a = if opaque {
            255
        } else {
            alpha.get(i).copied().unwrap_or(255)
        };
        out.extend_from_slice(&[pal[o], pal[o + 1], pal[o + 2], a]);
    }
    out
}

/// Swap RGBA pictures into the running 3D / play session's materials by name.
fn live_upload(items: &[(String, u32, u32, Vec<u8>)]) {
    let Some(state) = APP.with(|a| a.borrow().clone()) else {
        return;
    };
    let Ok(mut s) = state.try_borrow_mut() else {
        return;
    };
    let s = &mut *s;
    if let Mode::Play(play) = &mut s.mode {
        for (name, w, h, rgba) in items {
            let _ = play
                .materials
                .upsert_rgba(&s.gpu.device, &s.gpu.queue, name, *w, *h, rgba);
        }
    }
}

/// Swap a sprite / HUD / menu picture into the running session: the soft
/// patch cache (sprite size + offsets, HUD and status-bar blits) and, for
/// sprites or pictures already on the GPU, the material.
fn live_picture(name: &str, patch: room_ui::SoftPatch, rgba: &[u8], sprite: bool) -> bool {
    let Some(state) = APP.with(|a| a.borrow().clone()) else {
        return false;
    };
    let Ok(mut s) = state.try_borrow_mut() else {
        return false;
    };
    let s = &mut *s;
    let Mode::Play(play) = &mut s.mode else {
        return false;
    };
    let (w, h) = (u32::from(patch.width), u32::from(patch.height));
    if sprite || play.materials.get(name).is_some() {
        let _ = play
            .materials
            .upsert_rgba(&s.gpu.device, &s.gpu.queue, name, w, h, rgba);
    }
    play.patches.insert(name, patch);
    true
}

/// Every animation of the actor behind a sprite: `{class, sequences: [{label,
/// frames: [{sprite, frame, tics, bright}]}], lumps: [...]}` where `lumps`
/// lists every sprite lump of the sprites those sequences use. `num` picks
/// the class by editor number; otherwise the first placeable class whose
/// states use `sprite` (4-char prefix of a lump name). `null` if unknown.
#[wasm_bindgen(js_name = roomEditorSpriteAnims)]
pub fn room_editor_sprite_anims(sprite: &str, num: Option<i32>) -> Result<JsValue, JsValue> {
    with_editor(|ed| {
        let prefix: String = sprite.trim().to_ascii_uppercase().chars().take(4).collect();
        let uses = |seqs: &[(String, Vec<&room_sim::State>)]| {
            seqs.iter()
                .flat_map(|(_, s)| s.iter())
                .any(|s| s.sprite.eq_ignore_ascii_case(&prefix))
        };
        let by_num = num
            .and_then(|n| ed.data.classes.find_doomednum(n))
            .and_then(|id| ed.data.classes.get(id))
            .filter(|c| prefix.len() < 4 || uses(&ed.class_sequences(c)));
        let class = by_num.or_else(|| {
            ed.data
                .classes
                .iter()
                .filter(|c| c.doomednum.is_some())
                .find(|c| uses(&ed.class_sequences(c)))
        });
        let seqs = class.map(|c| ed.class_sequences(c)).unwrap_or_default();
        let mut used: BTreeSet<String> = seqs
            .iter()
            .flat_map(|(_, s)| s.iter())
            .filter(|s| s.sprite.len() == 4 && !s.sprite.eq_ignore_ascii_case("TNT1"))
            .map(|s| s.sprite.to_ascii_uppercase())
            .collect();
        if prefix.len() == 4 {
            used.insert(prefix.clone());
        }
        let lumps: Vec<String> = ed
            .sprite_names()
            .into_iter()
            .filter(|n| n.len() >= 6 && used.contains(&n[..4]))
            .collect();
        if lumps.is_empty() {
            return Ok(JsValue::NULL);
        }
        let sequences: Vec<Value> = seqs
            .iter()
            .map(|(label, frames)| {
                json!({
                    "label": label,
                    "frames": frames.iter().map(|s| json!({
                        "sprite": s.sprite.to_ascii_uppercase(),
                        "frame": char::from(b'A' + s.frame.min(28)).to_string(),
                        "tics": s.tics,
                        "bright": s.bright,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let name = class.map(|c| c.display_name().to_owned());
        let v = json!({ "class": name, "sequences": sequences, "lumps": lumps });
        Ok(JsValue::from_str(&v.to_string()))
    })
}

/// PLAYPAL palette 0 as 768 RGB bytes (for the pixel editor swatches).
#[wasm_bindgen(js_name = roomEditorPalette)]
pub fn room_editor_palette() -> Result<Vec<u8>, JsValue> {
    with_editor(|ed| Ok(palette_rgb(ed).to_vec()))
}

/// Picture to edit as palette indices: `{w, h, left, top, indices, alpha}`
/// or `null`. `ns` = `textures` (wall name: `TX_START` / TEXTUREX composite /
/// patch), `flats`, `patches`, `sprites` or `global`.
#[wasm_bindgen(js_name = roomEditorImageGet)]
pub fn room_editor_image_get(ns: &str, name: &str) -> Result<JsValue, JsValue> {
    with_editor(|ed| {
        let ns = image_ns(ns);
        let name = name.trim().to_ascii_uppercase();
        let img = if ns == "textures" {
            ed.materials.source_image(&ed.vfs, &name)
        } else {
            let order: &[Namespace] = match ns {
                "flats" => &[Namespace::Flats],
                "patches" => &[Namespace::Patches, Namespace::Global],
                "sprites" => &[Namespace::Sprites],
                _ => &[Namespace::Global, Namespace::Graphics],
            };
            order
                .iter()
                .find_map(|n| ed.vfs.find(&name, *n))
                .or_else(|| ed.vfs.find_any_ns(&name))
                .and_then(|id| ed.vfs.read(id).ok())
                .and_then(|d| decode_picture(&d, ns == "flats"))
        };
        let Some(img) = img else {
            return Ok(JsValue::NULL);
        };
        let (indices, alpha) = to_paletted(&img, &palette_rgb(ed));
        let o = js_sys::Object::new();
        let set = |k: &str, v: JsValue| {
            let _ = js_sys::Reflect::set(&o, &k.into(), &v);
        };
        set("w", img.width().into());
        set("h", img.height().into());
        set("left", img.left_offset.into());
        set("top", img.top_offset.into());
        set(
            "indices",
            js_sys::Uint8Array::from(indices.as_slice()).into(),
        );
        set("alpha", js_sys::Uint8Array::from(alpha.as_slice()).into());
        Ok(o.into())
    })
}

/// Store an edited picture as a lump override and show it in the running
/// session. Flats are written raw (square 64/128/256…), everything else as a
/// Doom patch (`textures` → `TX_START`, replacing the TEXTUREX composite).
///
/// Returns `"live"` when the 3D view already shows the change, or `"apply"`
/// when the level must restart ([`room_editor_apply_lumps`]) — resized wall
/// textures, or no running session. `commit` also refreshes the editor's own
/// VFS (thumbnails, previews).
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = roomEditorImagePut)]
pub fn room_editor_image_put(
    ns: &str,
    name: &str,
    w: u32,
    h: u32,
    left: i32,
    top: i32,
    indices: &[u8],
    alpha: &[u8],
    commit: bool,
) -> Result<String, JsValue> {
    with_editor(|ed| {
        let ns = image_ns(ns);
        let name = name.trim().to_ascii_uppercase();
        if name.is_empty() || name.len() > 8 {
            return Err(format!("lump name must be 1–8 characters: {name}"));
        }
        let n = (w as usize).saturating_mul(h as usize);
        if indices.len() != n || (!alpha.is_empty() && alpha.len() != n) {
            return Err("pixel buffer does not match the picture size".into());
        }
        let bytes = if ns == "flats" {
            match room_formats::flat_dimensions(n) {
                Some(d) if d.width == w && d.height == h => indices.to_vec(),
                _ => return Err("flats must be square (64×64, 128×128, 256×256…)".into()),
            }
        } else {
            let dim = |v: u32| u16::try_from(v).map_err(|_| "picture too large".to_owned());
            let off = |v: i32| i16::try_from(v).map_err(|_| "offset out of range".to_owned());
            room_formats::encode_patch(dim(w)?, dim(h)?, off(left)?, off(top)?, indices, alpha)
                .map_err(|e| e.to_string())?
        };
        let resized = ns == "textures" && {
            let (ow, oh) = ed.materials.wall_texel_size(&name);
            (ow, oh) != (w as f32, h as f32)
        };
        ed.overrides
            .entry(ns.to_owned())
            .or_default()
            .insert(name.clone(), bytes);
        let pal = palette_rgb(ed);
        let mode = match ns {
            "textures" | "flats" if !resized => {
                let rgba = paletted_rgba(&pal, indices, alpha, ns == "flats");
                live_upload(&[(name, w, h, rgba)]);
                if commit {
                    ed.rebuild()?;
                }
                "live"
            }
            "patches" => {
                ed.rebuild()?;
                let uploads: Vec<_> = ed
                    .materials
                    .textures_using_patch(&name)
                    .into_iter()
                    .filter_map(|t| {
                        let (w, h, px) = ed.materials.preview_rgba(&ed.vfs, &t)?;
                        Some((t, w, h, px))
                    })
                    .collect();
                live_upload(&uploads);
                "live"
            }
            "sprites" | "global" => {
                let dim = |v: u32| u16::try_from(v).unwrap_or(u16::MAX);
                let off = |v: i32| i16::try_from(v).unwrap_or(0);
                let opaque = alpha.is_empty();
                let soft = room_ui::SoftPatch {
                    width: dim(w),
                    height: dim(h),
                    left_offset: off(left),
                    top_offset: off(top),
                    indices: indices.to_vec(),
                    alpha: if opaque { vec![255; n] } else { alpha.to_vec() },
                    rgba: None,
                };
                let rgba = paletted_rgba(&pal, indices, alpha, opaque);
                let shown = live_picture(&name, soft, &rgba, ns == "sprites");
                if commit || !shown {
                    ed.rebuild()?;
                }
                if shown { "live" } else { "apply" }
            }
            _ => {
                ed.rebuild()?;
                "apply"
            }
        };
        Ok(mode.to_owned())
    })
}

/// Remount with the lump overrides, reload definitions, restart the level.
/// Returns the new summary JSON (`decorateError` set if DECORATE failed).
#[wasm_bindgen(js_name = roomEditorApplyLumps)]
pub fn room_editor_apply_lumps() -> Result<String, JsValue> {
    with_editor(|ed| {
        ed.rebuild()?;
        if let Some(geom) = ed.geom.clone() {
            let cam = with_play(|p| p.current_eye()).flatten();
            start_session(ed, geom, cam)?;
        }
        Ok(ed.summary().to_string())
    })
}

/// All classes with their editable values: `[{name, parent, num, props, edited}]`.
#[wasm_bindgen(js_name = roomEditorActors)]
pub fn room_editor_actors() -> Result<String, JsValue> {
    with_editor(|ed| {
        let data = ed.engine_data();
        let list: Vec<Value> = data
            .classes
            .iter()
            .skip(1)
            .map(|c| {
                let mut props = serde_json::Map::new();
                for (k, _) in ACTOR_PROPS {
                    props.insert((*k).to_owned(), actor_prop(c, k));
                }
                for (n, _) in ACTOR_FLAGS {
                    let key = format!("+{n}");
                    let v = actor_prop(c, &key);
                    props.insert(key, v);
                }
                json!({
                    "name": c.name,
                    "parent": c.parent.and_then(|p| data.classes.get(p)).map(|p| p.name.clone()),
                    "num": c.doomednum,
                    "props": props,
                    "edited": ed.actor_edits.get(&c.name).map(|m| m.keys().cloned().collect::<Vec<_>>()),
                })
            })
            .collect();
        serde_json::to_string(&list).map_err(|e| e.to_string())
    })
}

/// Edit one actor value (`value` JSON; `null` reverts). Applies to the running
/// level immediately for new spawns; already spawned actors keep their copy
/// until the level restarts.
#[wasm_bindgen(js_name = roomEditorSetActor)]
pub fn room_editor_set_actor(class: &str, key: &str, value: &str) -> Result<(), JsValue> {
    let v: Value = serde_json::from_str(value).map_err(err)?;
    let data = with_editor(|ed| {
        let name = ed
            .data
            .classes
            .find_name(class)
            .and_then(|id| ed.data.classes.get(id))
            .map(|c| c.name.clone())
            .ok_or_else(|| format!("unknown class {class}"))?;
        let entry = ed.actor_edits.entry(name.clone()).or_default();
        if v.is_null() {
            entry.remove(key);
            if entry.is_empty() {
                ed.actor_edits.remove(&name);
            }
        } else {
            entry.insert(key.to_owned(), v);
        }
        Ok(ed.engine_data())
    })?;
    with_play(|play| play.replace_defs(data));
    Ok(())
}

/// Build the PWAD: every edited map (`{"MAP01": doc, …}`), lump overrides,
/// and a `DECORATE` for actor value edits.
#[wasm_bindgen(js_name = roomEditorExport)]
pub fn room_editor_export(maps_json: &str) -> Result<Vec<u8>, JsValue> {
    let maps: BTreeMap<String, RawMap> = serde_json::from_str(maps_json).map_err(err)?;
    with_editor(|ed| {
        let mut extra = Vec::new();
        for doc in maps.values() {
            let mut doc = doc.clone();
            ed.attach_blobs(&mut doc);
            extra.extend(doc.to_lumps().map_err(|e| e.to_string())?);
        }
        let mut overrides = ed.overrides.clone();
        if !ed.actor_edits.is_empty() {
            let global = overrides.entry("global".into()).or_default();
            let mut text = global
                .get("DECORATE")
                .map(|d| String::from_utf8_lossy(d).into_owned())
                .unwrap_or_default();
            text.push('\n');
            text.push_str(&actor_edits_decorate(&ed.actor_edits));
            global.insert("DECORATE".into(), text.into_bytes());
        }
        overrides_wad(&overrides, Some(&extra))
    })
}

/// Live play-test tool on player 1: `god`, `noclip`, `freeze`, `give <what>`,
/// `kill` (all monsters), `summon <class>`, `health <n>`, `armor <n>`,
/// `suicide`. Returns a short human-readable result.
#[wasm_bindgen(js_name = roomEditorCheat)]
pub fn room_editor_cheat(cmd: &str, arg: &str) -> Result<String, JsValue> {
    with_play(|play| -> Result<String, String> {
        let game = &mut play.game;
        if game.world().is_none() {
            return Err("start a play test first".into());
        }
        let on_off = |b: bool| if b { "on" } else { "off" };
        let set_stat = |game: &mut room_game::Headless, armor: bool| -> Result<String, String> {
            let n: i32 = arg
                .trim()
                .parse()
                .map_err(|_| format!("expected a number, got {arg:?}"))?;
            let n = n.clamp(1, 999);
            let world = game.world_mut().ok_or("no level")?;
            let player = world
                .players
                .get_mut(0)
                .and_then(|p| p.as_mut())
                .ok_or("no player")?;
            if armor {
                player.armor = n.min(200);
                if player.armor_class == 0 {
                    player.armor_class = 1;
                }
                return Ok(format!("armor {}", player.armor));
            }
            let mo = player.mo.ok_or("player has no body")?;
            let a = world.actors.get_mut(mo).ok_or("player has no body")?;
            a.health = n;
            Ok(format!("health {n}"))
        };
        match cmd {
            "god" => Ok(format!("god mode {}", on_off(game.cheat_god(0)))),
            "noclip" => Ok(format!("noclip {}", on_off(game.cheat_noclip(0)))),
            "freeze" => Ok(format!("monsters frozen {}", on_off(game.cheat_freeze()))),
            "give" => {
                let what = if arg.trim().is_empty() {
                    "all"
                } else {
                    arg.trim()
                };
                Ok(format!("gave {what} ({} items)", game.cheat_give(0, what)))
            }
            "kill" => Ok(format!("killed {} monsters", game.cheat_kill_monsters())),
            "summon" => {
                if game.cheat_summon(0, arg.trim()) {
                    Ok(format!("summoned {}", arg.trim()))
                } else {
                    Err(format!("cannot summon {:?}", arg.trim()))
                }
            }
            "health" => set_stat(game, false),
            "armor" => set_stat(game, true),
            "suicide" => Ok(if game.cheat_kill_self(0) {
                "player killed"
            } else {
                "no player"
            }
            .into()),
            other => Err(format!("unknown tool {other}")),
        }
    })
    .ok_or_else(|| JsValue::from_str("no session"))?
    .map_err(err)
}

/// Player 1 + level state for the play-test panel, or `null` when not in a level.
#[wasm_bindgen(js_name = roomEditorPlayerState)]
pub fn room_editor_player_state() -> JsValue {
    with_play(|play| {
        let world = play.game.world()?;
        let p = world.players.first()?.as_ref()?;
        let mo = p.mo.and_then(|id| world.actors.get(id));
        let hud = world.player_hud(0);
        Some(
            json!({
                "health": mo.map_or(0, |a| a.health),
                "armor": p.armor,
                "ammo": hud.as_ref().map_or(0, |h| h.ammo),
                "god": p.god,
                "noclip": p.noclip,
                "frozen": world.level.frozen,
                "x": mo.map_or(0.0, |a| a.pos.x()),
                "y": mo.map_or(0.0, |a| a.pos.y()),
                "z": mo.map_or(0.0, |a| a.pos.z()),
                "kills": world.level.found_kills,
                "totalKills": world.level.total_kills,
                "secrets": world.level.found_secrets,
                "totalSecrets": world.level.total_secrets,
                "time": world.level.time / 35,
                "playing": play.edit_cam.is_none(),
            })
            .to_string(),
        )
    })
    .flatten()
    .map_or(JsValue::NULL, |s| JsValue::from_str(&s))
}

/// Skill used for editor play tests (1..=5); restarts on next play.
#[wasm_bindgen(js_name = roomEditorSetSkill)]
pub fn room_editor_set_skill(skill: u8) -> Result<(), JsValue> {
    with_editor(|ed| {
        ed.skill = skill.clamp(1, 5);
        Ok(())
    })
}
