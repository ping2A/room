//! What each file of a preset brings, and how its load order combines them
//! (`POST /admin/api/v1/presets/plan`).
//!
//! Files mount in order: the engine's own definitions, the base game, the
//! preset add-ons (top to bottom), then the gameplay mod. For every file the
//! plan lists what it contains (sprites, sounds, maps, definition lumps,
//! actor classes, Room mod manifest), what it takes over from files loaded
//! before it, and what files after it take over from it. It also flags
//! orders that cannot work, such as a DECORATE actor inheriting from an
//! actor that only a later file defines.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek};
use std::path::Path;

use serde::Serialize;

/// Resource groups where a later file's entry of the same name wins.
const GROUPS: &[(&str, &str)] = &[
    ("maps", "Maps"),
    ("sprites", "Sprites (monster, weapon and item frames)"),
    ("sounds", "Sounds"),
    ("music", "Music"),
    ("graphics", "Graphics (HUD, menus, title screens, fonts)"),
    ("textures", "Textures"),
    ("patches", "Wall patches"),
    ("flats", "Flats (floor and ceiling)"),
    ("tables", "Doom texture tables (TEXTURE1/2, PNAMES)"),
    ("palette", "Palette and colormaps"),
    ("hires", "Hi-res replacements"),
    ("brightmaps", "Brightmaps"),
    ("models", "3D model files"),
    ("voxels", "Voxels"),
    ("acs", "Compiled ACS script libraries"),
    ("other", "Other files"),
];

/// Definition lumps: (name, what it holds, how several files combine).
const DEFS: &[(&str, &str, Combine)] = &[
    (
        "ZSCRIPT",
        "Scripted classes: actors, weapons, event handlers, menus",
        Combine::ZScript,
    ),
    (
        "DECORATE",
        "Actor definitions: monsters, weapons, items, effects",
        Combine::Decorate,
    ),
    (
        "DEHACKED",
        "Patches to the built-in things, weapons and text",
        Combine::Merged,
    ),
    (
        "MAPINFO",
        "Map names, episodes, music, sky, next-map rules",
        Combine::Merged,
    ),
    (
        "ZMAPINFO",
        "Map names, episodes, music, sky, next-map rules",
        Combine::Merged,
    ),
    (
        "UMAPINFO",
        "Map names, episodes, music, sky, next-map rules",
        Combine::Merged,
    ),
    (
        "GAMEINFO",
        "Game-wide settings: player class, status bar class, title, cursor",
        Combine::Merged,
    ),
    ("SBARINFO", "Status bar layout", Combine::LastWins),
    (
        "KEYCONF",
        "Weapon slots, player classes and key aliases",
        Combine::Merged,
    ),
    ("MENUDEF", "Menus and option pages", Combine::Merged),
    (
        "CVARINFO",
        "Mod settings (console variables)",
        Combine::Merged,
    ),
    (
        "LANGUAGE",
        "Text strings (messages, obituaries, menus)",
        Combine::Merged,
    ),
    (
        "SNDINFO",
        "Sound names, random sound groups, ambient sounds",
        Combine::Merged,
    ),
    (
        "SNDSEQ",
        "Door and platform sound sequences",
        Combine::Merged,
    ),
    ("MUSINFO", "Music changes inside a map", Combine::Merged),
    ("REVERBS", "Ambient reverb settings", Combine::Merged),
    (
        "LOADACS",
        "ACS libraries loaded on every map",
        Combine::Merged,
    ),
    (
        "TEXTURES",
        "Texture compositions and replacements",
        Combine::Merged,
    ),
    (
        "ANIMDEFS",
        "Animated textures and switches",
        Combine::Merged,
    ),
    (
        "GLDEFS",
        "Dynamic lights, brightmaps, glows",
        Combine::Merged,
    ),
    ("MODELDEF", "3D model assignments", Combine::Merged),
    ("VOXELDEF", "Voxel assignments", Combine::Merged),
    (
        "DECALDEF",
        "Decals (blood splats, bullet holes)",
        Combine::Merged,
    ),
    (
        "TERRAIN",
        "Floor splashes and footstep effects",
        Combine::Merged,
    ),
    ("LOCKDEFS", "Key locks", Combine::Merged),
    ("FONTDEFS", "Fonts", Combine::Merged),
    ("TRNSLATE", "Colour translations", Combine::Merged),
    ("XHAIRS", "Crosshairs", Combine::Merged),
    ("ALTHUDCF", "Alternative HUD settings", Combine::Merged),
    ("ENDOOM", "Text screen shown on quit", Combine::LastWins),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Combine {
    /// Added to the same lump of earlier files; same entries: later wins.
    Merged,
    /// Only the last file's lump is used.
    LastWins,
    /// All files compiled as one program.
    ZScript,
    /// Read in load order.
    Decorate,
}

impl Combine {
    fn short(self) -> &'static str {
        match self {
            Self::Merged => "Merged",
            Self::LastWins => "Last file wins",
            Self::ZScript => "Compiled together",
            Self::Decorate => "Read in order",
        }
    }

    fn rule(self) -> &'static str {
        match self {
            Self::Merged => {
                "Added to the same lump of earlier files; where two files define the same entry, the later file wins."
            }
            Self::LastWins => "Only the last file that has one is used; earlier ones are ignored.",
            Self::ZScript => {
                "All ZScript of all files is compiled together as one program, so order between ZScript files does not matter; a class defined twice is an error, and a ZScript file at the same path in a later file replaces the earlier one."
            }
            Self::Decorate => {
                "Read in load order: an actor can only inherit from actors defined before it (earlier files, ZScript or the engine)."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lang {
    ZScript,
    Decorate,
}

#[derive(Clone, Debug)]
struct ClassDef {
    name: String,
    parent: Option<String>,
    replaces: Option<String>,
    lang: Lang,
}

/// A Room mod manifest (`room.toml` or `ROOMMOD`).
#[derive(Clone, Debug, Default, Serialize)]
pub struct RoomModInfo {
    /// `[mod] id`.
    pub id: String,
    /// `[mod] name`.
    pub name: String,
    /// `[mod] description`.
    pub description: String,
    /// ZScript files the mod reimplements.
    pub replaces_zscript: Vec<String>,
    /// `[[module]]` names.
    pub modules: Vec<String>,
    /// `[[module]] file` values (lumps holding the code).
    #[serde(skip)]
    pub module_files: Vec<String>,
}

/// What one file contains (computed once per file fingerprint).
#[derive(Clone, Debug, Default)]
pub struct Profile {
    /// Why not, when `!inspected`.
    note: Option<String>,
    /// Group → upper-case resource names.
    groups: BTreeMap<&'static str, BTreeSet<String>>,
    /// Definition lump → number of lumps / files (includes counted).
    defs: BTreeMap<&'static str, usize>,
    classes: Vec<ClassDef>,
    /// `extend class X` targets.
    extends: BTreeSet<String>,
    room_mod: Option<RoomModInfo>,
    /// Lower-case paths of ZScript sources (for `replaces_zscript`).
    zscript_paths: BTreeSet<String>,
}

/// Profile a library file from disk.
#[must_use]
pub fn profile_path(path: &Path) -> Profile {
    finish(profile_path_raw(path))
}

/// Drop a Room mod's own code files from the resource groups.
fn finish(mut p: Profile) -> Profile {
    if let Some(m) = &p.room_mod {
        let code: Vec<String> = m
            .module_files
            .iter()
            .map(|f| upper_stem(f.rsplit('/').next().unwrap_or(f)))
            .collect();
        for set in p.groups.values_mut() {
            set.retain(|n| !code.contains(n));
        }
        p.groups.retain(|_, set| !set.is_empty());
    }
    p
}

fn profile_path_raw(path: &Path) -> Profile {
    let Ok(mut f) = std::fs::File::open(path) else {
        return Profile::not_read("could not open the file");
    };
    let mut magic = [0u8; 4];
    if f.read_exact(&mut magic).is_err() {
        return Profile::not_read("file too short");
    }
    match &magic {
        b"IWAD" | b"PWAD" => match std::fs::read(path) {
            Ok(b) => profile_wad(&b),
            Err(e) => Profile::not_read(&e.to_string()),
        },
        b"PK\x03\x04" | b"PK\x05\x06" => {
            let _ = f.rewind();
            profile_zip(std::io::BufReader::new(f))
        }
        b"7z\xbc\xaf" => Profile::not_read("7z archive: contents are not inspected here"),
        _ => Profile::not_read("unknown file type"),
    }
}

/// Profile in-memory bytes (catalog mods).
#[must_use]
pub fn profile_bytes(bytes: &[u8]) -> Profile {
    finish(match bytes.get(..4) {
        Some(b"IWAD" | b"PWAD") => profile_wad(bytes),
        Some(b"PK\x03\x04" | b"PK\x05\x06") => profile_zip(std::io::Cursor::new(bytes)),
        _ => Profile::not_read("unknown file type"),
    })
}

impl Profile {
    fn not_read(why: &str) -> Self {
        Self {
            note: Some(why.to_owned()),
            ..Self::default()
        }
    }

    fn add(&mut self, group: &'static str, name: String) {
        if !name.is_empty() {
            self.groups.entry(group).or_default().insert(name);
        }
    }

    fn add_def(&mut self, def: &'static str) {
        *self.defs.entry(def).or_default() += 1;
    }

    fn parse_sources(&mut self, lang: Lang, text: &str) {
        let (classes, extends) = parse_classes(lang, text);
        self.classes.extend(classes);
        self.extends.extend(extends);
    }
}

fn def_of(stem: &str) -> Option<&'static str> {
    let up = stem.to_ascii_uppercase();
    DEFS.iter().find(|(n, _, _)| *n == up).map(|(n, _, _)| *n)
}

const MAP_LUMPS: &[&str] = &[
    "THINGS", "LINEDEFS", "SIDEDEFS", "VERTEXES", "SEGS", "SSECTORS", "NODES", "SECTORS", "REJECT",
    "BLOCKMAP", "BEHAVIOR", "SCRIPTS", "TEXTMAP", "ZNODES", "DIALOGUE", "ENDMAP", "LEAFS",
    "LIGHTS", "MACROS",
];

fn profile_wad(b: &[u8]) -> Profile {
    let mut p = Profile::default();
    let rd = |at: usize| -> Option<usize> {
        let raw: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
        usize::try_from(i32::from_le_bytes(raw)).ok()
    };
    let (Some(count), Some(dir)) = (rd(4), rd(8)) else {
        return Profile::not_read("bad WAD header");
    };
    let mut lumps = Vec::new();
    for i in 0..count.min(200_000) {
        let e = dir + i * 16;
        let (Some(pos), Some(size), Some(raw)) = (rd(e), rd(e + 4), b.get(e + 8..e + 16)) else {
            break;
        };
        let end = raw.iter().position(|&c| c == 0).unwrap_or(8);
        let name = String::from_utf8_lossy(&raw[..end]).to_ascii_uppercase();
        lumps.push((name, pos, size));
    }
    let text = |pos: usize, size: usize| {
        b.get(pos..pos.saturating_add(size))
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .unwrap_or_default()
    };
    let mut ns: Option<&'static str> = None;
    let mut sources: Vec<(Lang, String)> = Vec::new();
    for (i, (name, pos, size)) in lumps.iter().enumerate() {
        let n = name.as_str();
        match n {
            "S_START" | "SS_START" => ns = Some("sprites"),
            "F_START" | "FF_START" => ns = Some("flats"),
            "P_START" | "PP_START" => ns = Some("patches"),
            "TX_START" => ns = Some("textures"),
            "HI_START" => ns = Some("hires"),
            "C_START" => ns = Some("palette"),
            "A_START" => ns = Some("acs"),
            "VX_START" => ns = Some("voxels"),
            _ if n.ends_with("_END") && n.len() <= 7 => ns = None,
            _ if n.ends_with("_START") => {}
            _ => {
                if let Some(g) = ns {
                    if *size > 0 {
                        p.add(g, name.clone());
                    }
                    continue;
                }
                if MAP_LUMPS.contains(&n) || n.starts_with("GL_") {
                    continue;
                }
                let next = lumps.get(i + 1).map(|l| l.0.as_str());
                if matches!(next, Some("THINGS" | "TEXTMAP")) {
                    p.add("maps", name.clone());
                    continue;
                }
                if n == "ROOMMOD" {
                    p.room_mod = Some(parse_manifest(&text(*pos, *size)));
                    continue;
                }
                if let Some(d) = def_of(n) {
                    p.add_def(d);
                    match d {
                        "ZSCRIPT" => {
                            p.zscript_paths.insert("zscript".into());
                            sources.push((Lang::ZScript, text(*pos, *size)));
                        }
                        "DECORATE" => sources.push((Lang::Decorate, text(*pos, *size))),
                        _ => {}
                    }
                    continue;
                }
                let g = match n {
                    "TEXTURE1" | "TEXTURE2" | "PNAMES" => "tables",
                    "PLAYPAL" | "COLORMAP" => "palette",
                    _ if n.starts_with("DS") || n.starts_with("DP") => "sounds",
                    _ if n.starts_with("D_") || n.starts_with("MUS_") => "music",
                    _ if n.starts_with("DEMO")
                        || n.starts_with("GENMIDI")
                        || n.starts_with("DMX") =>
                    {
                        "other"
                    }
                    _ => "graphics",
                };
                if *size > 0 {
                    p.add(g, name.clone());
                }
            }
        }
    }
    // `#include`s of WAD script lumps name other lumps.
    let mut seen = BTreeSet::new();
    let mut i = 0;
    while i < sources.len() {
        let (lang, src) = sources[i].clone();
        p.parse_sources(lang, &src);
        for inc in include_paths(&src) {
            let short = inc.rsplit('/').next().unwrap_or(&inc);
            let short = short
                .split('.')
                .next()
                .unwrap_or(short)
                .to_ascii_uppercase();
            if !seen.insert(short.clone()) {
                continue;
            }
            if let Some((_, pos, size)) = lumps.iter().rev().find(|l| l.0 == short) {
                sources.push((lang, text(*pos, *size)));
            }
        }
        i += 1;
    }
    p
}

fn upper_stem(file: &str) -> String {
    let stem = file.split('.').next().unwrap_or(file);
    stem.chars()
        .take(8)
        .collect::<String>()
        .to_ascii_uppercase()
}

fn profile_zip<R: Read + Seek>(r: R) -> Profile {
    let Ok(mut z) = zip::ZipArchive::new(r) else {
        return Profile::not_read("not a readable zip archive");
    };
    let names: Vec<String> = (0..z.len())
        .filter_map(|i| z.by_index_raw(i).ok().map(|e| e.name().replace('\\', "/")))
        .filter(|n| !n.ends_with('/'))
        .collect();
    // A single top folder wrapping everything (common in zipped mods).
    let top = names
        .first()
        .and_then(|n| n.split_once('/').map(|(t, _)| format!("{t}/")));
    let strip = top
        .filter(|t| names.iter().all(|n| n.starts_with(t.as_str())))
        .unwrap_or_default();
    let rel = |n: &str| n[strip.len()..].to_owned();
    let mut p = Profile::default();
    // Lower-case relative path → zip name.
    let by_path: BTreeMap<String, String> = names
        .iter()
        .map(|n| (rel(n).to_ascii_lowercase(), n.clone()))
        .collect();
    let mut roots: Vec<(Lang, String)> = Vec::new();
    for n in &names {
        let r = rel(n);
        let lower = r.to_ascii_lowercase();
        let parts: Vec<&str> = lower.split('/').collect();
        let file = *parts.last().unwrap_or(&"");
        if parts.len() == 1 {
            if lower == "room.toml" {
                if let Some(t) = read_zip_text(&mut z, n) {
                    p.room_mod = Some(parse_manifest(&t));
                }
                continue;
            }
            if let Some(d) = def_of(file.split('.').next().unwrap_or(file)) {
                p.add_def(d);
                match d {
                    "ZSCRIPT" => roots.push((Lang::ZScript, n.clone())),
                    "DECORATE" => roots.push((Lang::Decorate, n.clone())),
                    _ => {}
                }
                continue;
            }
            let stem = upper_stem(file);
            let g = match stem.as_str() {
                "TEXTURE1" | "TEXTURE2" | "PNAMES" => "tables",
                "PLAYPAL" | "COLORMAP" => "palette",
                _ if file.ends_with(".bm") || file.ends_with(".acs") => continue,
                _ => "other",
            };
            p.add(g, stem);
            continue;
        }
        if lower.ends_with(".zs") || lower.ends_with(".zc") || lower.ends_with(".zsc") {
            p.zscript_paths.insert(lower.clone());
        }
        let g = match parts[0] {
            "maps" => {
                if let Some(m) = file.strip_suffix(".wad") {
                    p.add("maps", m.to_ascii_uppercase());
                }
                continue;
            }
            "sprites" => "sprites",
            "sounds" => "sounds",
            "music" => "music",
            "graphics" | "fonts" => "graphics",
            "textures" => "textures",
            "patches" => "patches",
            "flats" => "flats",
            "hires" => "hires",
            "brightmaps" => "brightmaps",
            "models" => "models",
            "voxels" => "voxels",
            "acs" => "acs",
            "colormaps" => "palette",
            // Script sources and other folders are not looked up by lump name.
            _ => continue,
        };
        let name = if g == "models" {
            lower.clone()
        } else {
            upper_stem(file)
        };
        p.add(g, name);
    }
    // ZScript / DECORATE sources: roots plus everything they #include.
    let mut done = BTreeSet::new();
    let mut queue = roots;
    while let Some((lang, name)) = queue.pop() {
        if !done.insert(name.clone()) {
            continue;
        }
        let Some(text) = read_zip_text(&mut z, &name) else {
            continue;
        };
        if lang == Lang::ZScript {
            p.zscript_paths.insert(rel(&name).to_ascii_lowercase());
        }
        let is_root = !rel(&name).contains('/');
        if !is_root {
            *p.defs
                .entry(if lang == Lang::ZScript {
                    "ZSCRIPT"
                } else {
                    "DECORATE"
                })
                .or_default() += 1;
        }
        for inc in include_paths(&text) {
            if let Some(zn) = by_path.get(&inc.to_ascii_lowercase()) {
                queue.push((lang, zn.clone()));
            }
        }
        p.parse_sources(lang, &text);
    }
    p
}

fn read_zip_text<R: Read + Seek>(z: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    let mut e = z.by_name(name).ok()?;
    if e.size() > 8 * 1024 * 1024 {
        return None;
    }
    let mut buf = Vec::new();
    e.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn include_paths(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let rest = l.trim_start();
            let rest = rest
                .get(..8)
                .filter(|h| h.eq_ignore_ascii_case("#include"))
                .map(|_| rest[8..].trim())?;
            let rest = rest.strip_prefix('"')?;
            let end = rest.find('"')?;
            let p = rest[..end].replace('\\', "/");
            let p = p.trim_start_matches("./").trim_start_matches('/');
            (!p.is_empty()).then(|| p.to_owned())
        })
        .collect()
}

/// Minimal `room.toml` reader: the `[mod]` keys and module names the plan shows.
fn parse_manifest(text: &str) -> RoomModInfo {
    let mut m = RoomModInfo::default();
    let mut section = String::new();
    let mut lines = text.lines();
    let unquote = |v: &str| {
        let v = v.trim();
        v.strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(v)
            .to_owned()
    };
    while let Some(line) = lines.next() {
        let line = line.split(" #").next().unwrap_or(line).trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_matches(['[', ']']).trim().to_owned();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match (section.as_str(), k) {
            ("mod", "id") => m.id = unquote(v),
            ("mod", "name") => m.name = unquote(v),
            ("mod", "description") => m.description = unquote(v),
            ("mod", "replaces_zscript") => {
                let mut list = v.to_owned();
                while !list.contains(']') {
                    match lines.next() {
                        Some(l) => list.push_str(l),
                        None => break,
                    }
                }
                m.replaces_zscript = list
                    .trim_matches(['[', ']', ' '])
                    .split(',')
                    .map(unquote)
                    .filter(|s| !s.is_empty() && !s.contains(']'))
                    .collect();
            }
            ("module", "name") => m.modules.push(unquote(v)),
            ("module", "file") => m.module_files.push(unquote(v)),
            _ => {}
        }
    }
    m
}

/// Strip `//` and `/* */` comments and string contents.
fn strip(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
                out.push(' ');
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
                out.push_str("\"\"");
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            if matches!(c, '{' | '}' | ':' | '<' | ';' | '#') {
                out.push(c.to_string());
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Top-level class / actor declarations and `extend class` targets.
fn parse_classes(lang: Lang, text: &str) -> (Vec<ClassDef>, Vec<String>) {
    let t = tokens(&strip(text));
    let is = |i: usize, w: &str| t.get(i).is_some_and(|s| s.eq_ignore_ascii_case(w));
    let ident = |i: usize| {
        t.get(i)
            .filter(|s| {
                s.chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            })
            .cloned()
    };
    let keyword = if lang == Lang::ZScript {
        "class"
    } else {
        "actor"
    };
    let mut classes = Vec::new();
    let mut extends = Vec::new();
    let mut depth = 0i32;
    let mut i = 0;
    while i < t.len() {
        match t[i].as_str() {
            "{" => depth += 1,
            "}" => depth = (depth - 1).max(0),
            _ if depth == 0 && is(i, keyword) => {
                let prev_extend = i > 0 && is(i - 1, "extend");
                let prev_mixin = i > 0 && is(i - 1, "mixin");
                if let Some(name) = ident(i + 1) {
                    if prev_extend {
                        extends.push(name);
                    } else if !prev_mixin {
                        let mut j = i + 2;
                        let mut parent = None;
                        let mut replaces = None;
                        if t.get(j).map(String::as_str) == Some(":") {
                            parent = ident(j + 1);
                            j += 2;
                        }
                        if is(j, "replaces") {
                            replaces = ident(j + 1);
                        }
                        classes.push(ClassDef {
                            name,
                            parent,
                            replaces,
                            lang,
                        });
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    (classes, extends)
}

// ------------------------------------------------------------------ plan

/// One file to place in the plan.
pub struct PlanFile {
    /// Name shown.
    pub file: String,
    /// `"base"`, `"addon"` or `"mod"`.
    pub role: &'static str,
    /// Library kind (`iwad`, `pwad`, `archive`, …).
    pub kind: String,
    /// Size in bytes.
    pub size: Option<u64>,
    /// Contents (`None` = file unknown to the server).
    pub profile: Option<std::sync::Arc<Profile>>,
}

/// Plan returned to the admin UI.
#[derive(Debug, Serialize)]
pub struct LoadPlan {
    /// Steps in mount order (step 0 is the engine).
    pub steps: Vec<Step>,
    /// Problems with this order, in plain words.
    pub warnings: Vec<String>,
}

/// One mounted file.
#[derive(Debug, Serialize)]
pub struct Step {
    /// 0 = engine, 1 = base game, …
    pub order: usize,
    /// File name.
    pub file: String,
    /// `engine`, `base`, `addon`, `mod`.
    pub role: &'static str,
    /// Library kind.
    pub kind: String,
    /// Size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// One line: what this file is.
    pub summary: String,
    /// Resource groups with counts.
    pub provides: Vec<Count>,
    /// Definition lumps with what they hold and how they combine.
    pub definitions: Vec<Definition>,
    /// Actor classes it declares.
    pub classes: ClassCounts,
    /// `X replaces Y` (first 16).
    pub replacements: Vec<Replacement>,
    /// Total replacements.
    pub replacements_total: usize,
    /// What it takes over from earlier files.
    pub overrides: Vec<Override>,
    /// What later files take over from it.
    pub overridden_by: Vec<Override>,
    /// Room mod manifest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_mod: Option<RoomModInfo>,
    /// Informational notes.
    pub notes: Vec<String>,
    /// Problems for this file.
    pub warnings: Vec<String>,
}

/// Resource group count.
#[derive(Debug, Serialize)]
pub struct Count {
    /// Group label.
    pub label: &'static str,
    /// Number of entries.
    pub count: usize,
}

/// Definition lump summary.
#[derive(Debug, Serialize)]
pub struct Definition {
    /// Lump name (`MAPINFO`).
    pub lump: &'static str,
    /// Lumps / files of that kind (includes counted).
    pub count: usize,
    /// What it holds.
    pub what: &'static str,
    /// Short form of `rule`.
    pub combine: &'static str,
    /// How it combines with the same lump of other files.
    pub rule: &'static str,
}

/// ZScript / DECORATE class counts.
#[derive(Debug, Default, Serialize)]
pub struct ClassCounts {
    /// ZScript classes.
    pub zscript: usize,
    /// DECORATE actors.
    pub decorate: usize,
    /// `extend class` blocks.
    pub extends: usize,
}

/// `by replaces actor`.
#[derive(Debug, Serialize)]
pub struct Replacement {
    /// Replaced actor.
    pub actor: String,
    /// New actor.
    pub by: String,
}

/// Same-name entries between two files.
#[derive(Debug, Serialize)]
pub struct Override {
    /// Group label.
    pub label: &'static str,
    /// Other file.
    pub file: String,
    /// Number of names.
    pub count: usize,
    /// A few names.
    pub examples: Vec<String>,
}

/// Doom monsters, weapons and items players recognise, in display order.
const KNOWN_ACTORS: &[&str] = &[
    "DoomPlayer",
    "ZombieMan",
    "ShotgunGuy",
    "ChaingunGuy",
    "DoomImp",
    "Demon",
    "Spectre",
    "LostSoul",
    "Cacodemon",
    "HellKnight",
    "BaronOfHell",
    "Arachnotron",
    "PainElemental",
    "Revenant",
    "Fatso",
    "Archvile",
    "Cyberdemon",
    "SpiderMastermind",
    "WolfensteinSS",
    "Fist",
    "Chainsaw",
    "Pistol",
    "Shotgun",
    "SuperShotgun",
    "Chaingun",
    "RocketLauncher",
    "PlasmaRifle",
    "BFG9000",
    "Clip",
    "Shell",
    "RocketAmmo",
    "Cell",
    "Stimpack",
    "Medikit",
    "Soulsphere",
    "Megasphere",
    "GreenArmor",
    "BlueArmor",
    "Berserk",
];

const ENGINE_FILE: &str = "Room engine definitions";

fn group_label(g: &str) -> &'static str {
    GROUPS
        .iter()
        .find(|(k, _)| *k == g)
        .map_or("Other files", |(_, l)| l)
}

fn summary_for(f: &PlanFile, p: &Profile) -> String {
    if let Some(m) = &p.room_mod {
        let what = if m.name.is_empty() {
            m.id.as_str()
        } else {
            m.name.as_str()
        };
        return format!("Room mod: {what}");
    }
    let maps = p.groups.get("maps").map_or(0, BTreeSet::len);
    let code = p.classes.len();
    match f.role {
        "base" => format!(
            "Base game: {maps} map(s) plus the graphics, sounds and music the game is built on"
        ),
        "mod" => "Gameplay mod picked in the preset defaults".to_owned(),
        _ if code > 0 && maps > 0 => {
            format!("Gameplay mod with {code} actor class(es) and {maps} map(s)")
        }
        _ if code > 0 => format!("Gameplay mod: {code} actor class(es)"),
        _ if maps > 0 => format!("Map pack: {maps} map(s)"),
        _ if p.groups.is_empty() && p.defs.is_empty() => "No game resources found".to_owned(),
        _ => "Resource pack (graphics, sounds or definitions)".to_owned(),
    }
}

fn examples(set: &BTreeSet<String>) -> Vec<String> {
    set.iter().take(8).cloned().collect()
}

/// Build the plan for `files` in mount order.
#[must_use]
pub fn plan(files: &[PlanFile]) -> LoadPlan {
    let mut steps = vec![Step {
        order: 0,
        file: ENGINE_FILE.to_owned(),
        role: "engine",
        kind: "engine".into(),
        size: None,
        summary: "Built-in actor classes, weapons, menus and the ZScript library, loaded before every file".into(),
        provides: Vec::new(),
        definitions: Vec::new(),
        classes: ClassCounts::default(),
        replacements: Vec::new(),
        replacements_total: 0,
        overrides: Vec::new(),
        overridden_by: Vec::new(),
        room_mod: None,
        notes: vec!["Classes such as Actor, Inventory, Weapon, DoomImp or Zombieman come from here; files below can inherit from them in any order.".into()],
        warnings: Vec::new(),
    }];
    let mut warnings = Vec::new();
    // Class name (lower) → (step index, lang) of the first definition.
    let mut defined: BTreeMap<String, (usize, Lang)> = BTreeMap::new();
    for (i, f) in files.iter().enumerate() {
        if let Some(p) = &f.profile {
            for c in &p.classes {
                defined
                    .entry(c.name.to_ascii_lowercase())
                    .or_insert((i, c.lang));
            }
        }
    }
    let empty = Profile::default();
    for (i, f) in files.iter().enumerate() {
        let p = f.profile.as_deref().unwrap_or(&empty);
        let mut step = Step {
            order: i + 1,
            file: f.file.clone(),
            role: f.role,
            kind: f.kind.clone(),
            size: f.size,
            summary: if f.profile.is_none() {
                if f.role == "base" {
                    "Base game chosen by the server (not in the content library)".into()
                } else {
                    "Not in the content library".into()
                }
            } else {
                summary_for(f, p)
            },
            provides: Vec::new(),
            definitions: Vec::new(),
            classes: ClassCounts::default(),
            replacements: Vec::new(),
            replacements_total: 0,
            overrides: Vec::new(),
            overridden_by: Vec::new(),
            room_mod: p.room_mod.clone(),
            notes: Vec::new(),
            warnings: Vec::new(),
        };
        if let Some(n) = &p.note {
            step.notes.push(format!("Contents not inspected: {n}."));
        }
        for (g, label) in GROUPS {
            if let Some(set) = p.groups.get(g) {
                step.provides.push(Count {
                    label,
                    count: set.len(),
                });
            }
        }
        for (lump, what, combine) in DEFS {
            if let Some(&count) = p.defs.get(lump) {
                step.definitions.push(Definition {
                    lump,
                    count,
                    what,
                    combine: combine.short(),
                    rule: combine.rule(),
                });
            }
        }
        step.classes = ClassCounts {
            zscript: p.classes.iter().filter(|c| c.lang == Lang::ZScript).count(),
            decorate: p
                .classes
                .iter()
                .filter(|c| c.lang == Lang::Decorate)
                .count(),
            extends: p.extends.len(),
        };
        let mut reps: Vec<&ClassDef> = p.classes.iter().filter(|c| c.replaces.is_some()).collect();
        // Familiar Doom actors first.
        reps.sort_by_key(|c| {
            let r = c.replaces.as_deref().unwrap_or_default();
            KNOWN_ACTORS
                .iter()
                .position(|k| k.eq_ignore_ascii_case(r))
                .unwrap_or(usize::MAX)
        });
        step.replacements_total = reps.len();
        step.replacements = reps
            .iter()
            .take(16)
            .map(|c| Replacement {
                actor: c.replaces.clone().unwrap_or_default(),
                by: c.name.clone(),
            })
            .collect();
        // Same-name resources against every other file.
        for (j, other) in files.iter().enumerate() {
            if j == i {
                continue;
            }
            let Some(op) = &other.profile else { continue };
            for (g, set) in &p.groups {
                let Some(os) = op.groups.get(g) else { continue };
                let common: BTreeSet<String> = set.intersection(os).cloned().collect();
                if common.is_empty() {
                    continue;
                }
                let o = Override {
                    label: group_label(g),
                    file: other.file.clone(),
                    count: common.len(),
                    examples: examples(&common),
                };
                if j < i {
                    step.overrides.push(o);
                } else {
                    step.overridden_by.push(o);
                }
            }
            for lump in p.defs.keys() {
                let last_wins = DEFS
                    .iter()
                    .any(|(n, _, c)| n == lump && *c == Combine::LastWins);
                if last_wins && op.defs.contains_key(lump) {
                    let o = Override {
                        label: "Definition lumps (only the last one is used)",
                        file: other.file.clone(),
                        count: 1,
                        examples: vec![(*lump).to_owned()],
                    };
                    if j < i {
                        step.overrides.push(o);
                    } else {
                        step.overridden_by.push(o);
                    }
                }
            }
        }
        // Class order problems.
        let file_of = |k: usize| files.get(k).map_or("?", |f| f.file.as_str());
        let mut need_later: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
        for c in &p.classes {
            let key = c.name.to_ascii_lowercase();
            if let Some(&(first, lang)) = defined.get(&key)
                && first < i
            {
                let what = if lang == Lang::ZScript && c.lang == Lang::ZScript {
                    "both define it in ZScript, which stops the ZScript compile"
                } else {
                    "the two definitions conflict"
                };
                step.warnings.push(format!(
                    "Defines actor {} that {} already defines; {what}.",
                    c.name,
                    file_of(first)
                ));
            }
            if c.lang == Lang::Decorate
                && let Some(parent) = &c.parent
                && let Some(&(at, lang)) = defined.get(&parent.to_ascii_lowercase())
                && at > i
                && lang == Lang::Decorate
            {
                need_later
                    .entry(at)
                    .or_default()
                    .insert(format!("{} : {parent}", c.name));
            }
        }
        for name in &p.extends {
            if let Some(&(at, _)) = defined.get(&name.to_ascii_lowercase())
                && at > i
            {
                step.notes.push(format!(
                    "Extends class {name} from {} (loaded later): fine for ZScript, which compiles all files together.",
                    file_of(at)
                ));
            }
        }
        for (at, list) in need_later {
            let ex: Vec<String> = list.iter().take(4).cloned().collect();
            let w = format!(
                "{} has DECORATE actors that inherit from actors of {}, which loads after it ({}{}). Move {} above {}.",
                f.file,
                file_of(at),
                ex.join(", "),
                if list.len() > 4 {
                    format!(", +{} more", list.len() - 4)
                } else {
                    String::new()
                },
                file_of(at),
                f.file
            );
            step.warnings.push(w.clone());
            warnings.push(w);
        }
        // Room mod: ZScript it replaces, grouped by the file that has it.
        if let Some(m) = &p.room_mod {
            let mut later: BTreeMap<usize, Vec<String>> = BTreeMap::new();
            for path in &m.replaces_zscript {
                let key = path
                    .trim_start_matches("./")
                    .trim_start_matches('/')
                    .to_ascii_lowercase();
                let owner = files.iter().enumerate().find(|(j, o)| {
                    *j != i
                        && o.profile
                            .as_ref()
                            .is_some_and(|op| op.zscript_paths.contains(&key))
                });
                match owner {
                    Some((j, _)) if j > i => later.entry(j).or_default().push(path.clone()),
                    Some((_, o)) => step.notes.push(format!(
                        "Turns off {path} from {}: this mod's WebAssembly code runs instead.",
                        o.file
                    )),
                    None => step.warnings.push(format!(
                        "Replaces {path}, but no file in this preset has it; the mod has nothing to replace."
                    )),
                }
            }
            for (j, paths) in later {
                let w = format!(
                    "{} replaces {} of {}, but loads before it. Move {} below {} so the mod runs on top of the files it replaces.",
                    f.file,
                    paths.join(" and "),
                    file_of(j),
                    f.file,
                    file_of(j)
                );
                step.warnings.push(w.clone());
                warnings.push(w);
            }
        }
        if f.role == "addon" && f.kind == "iwad" {
            step.warnings.push("This is a base-game IWAD loaded as an add-on: everything in it overrides the base game.".into());
        }
        steps.push(step);
    }
    LoadPlan { steps, warnings }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::io::Write;
    use std::sync::Arc;

    fn wad(lumps: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = b"PWAD".to_vec();
        out.extend_from_slice(&(lumps.len() as i32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        let mut dir = Vec::new();
        for (n, d) in lumps {
            dir.extend_from_slice(&(out.len() as i32).to_le_bytes());
            dir.extend_from_slice(&(d.len() as i32).to_le_bytes());
            let mut name = [0u8; 8];
            name[..n.len()].copy_from_slice(n.as_bytes());
            dir.extend_from_slice(&name);
            out.extend_from_slice(d);
        }
        let at = out.len() as i32;
        out[8..12].copy_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&dir);
        out
    }

    fn zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (n, t) in files {
                z.start_file(*n, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(t.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    fn pf(file: &str, role: &'static str, p: Profile) -> PlanFile {
        PlanFile {
            file: file.into(),
            role,
            kind: "pwad".into(),
            size: None,
            profile: Some(Arc::new(p)),
        }
    }

    #[test]
    fn parses_classes_and_skips_code() {
        let (c, e) = parse_classes(
            Lang::ZScript,
            "class Foo : Actor replaces Imp { void f() { class<Actor> c; } }\n// class Nope\nextend class Bar {}\nmixin class M {}",
        );
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "Foo");
        assert_eq!(c[0].parent.as_deref(), Some("Actor"));
        assert_eq!(c[0].replaces.as_deref(), Some("Imp"));
        assert_eq!(e, vec!["Bar".to_owned()]);
        let (d, _) = parse_classes(
            Lang::Decorate,
            "Actor BZombie : ZombieMan replaces ZombieMan 3004 { States { Spawn: POSS A 1 } }",
        );
        assert_eq!(d[0].replaces.as_deref(), Some("ZombieMan"));
    }

    #[test]
    fn wad_and_zip_profiles_and_order() {
        let base = profile_wad(&wad(&[
            ("MAP01", b""),
            ("THINGS", b"x"),
            ("S_START", b""),
            ("POSSA1", b"x"),
            ("S_END", b""),
            ("DSPISTOL", b"x"),
        ]));
        assert_eq!(base.groups["maps"].len(), 1);
        assert!(base.groups["sprites"].contains("POSSA1"));
        let modz = profile_bytes(&zip(&[
            ("bd/DECORATE.txt", "#include \"actors/z.txt\""),
            ("bd/actors/z.txt", "actor BZ : BBase replaces ZombieMan {}"),
            ("bd/sprites/POSSA1.png", "x"),
            ("bd/zscript.zc", "class H : EventHandler {}"),
            ("bd/zscript/dmg.zc", ""),
        ]));
        assert!(modz.note.is_none());
        assert_eq!(modz.defs["DECORATE"], 2);
        assert!(modz.groups["sprites"].contains("POSSA1"));
        let lib = profile_bytes(&zip(&[("DECORATE", "actor BBase {}")]));
        let room = profile_wad(&wad(&[(
            "ROOMMOD",
            b"[mod]\nid = \"x\"\nname = \"Port\"\nreplaces_zscript = [\"zscript/dmg.zc\"]\n[[module]]\nname = \"m\"",
        )]));
        assert_eq!(
            room.room_mod.as_ref().unwrap().replaces_zscript,
            vec!["zscript/dmg.zc"]
        );
        let plan1 = plan(&[
            pf("doom2.wad", "base", base.clone()),
            pf("room.wad", "addon", room.clone()),
            pf("bd.pk3", "addon", modz.clone()),
            pf("lib.pk3", "addon", lib.clone()),
        ]);
        assert_eq!(plan1.warnings.len(), 2, "{:?}", plan1.warnings);
        assert!(
            plan1
                .warnings
                .iter()
                .any(|w| w.contains("room.wad replaces zscript/dmg.zc of bd.pk3"))
        );
        assert_eq!(plan1.steps[3].overrides[0].count, 1);
        assert_eq!(plan1.steps[3].replacements[0].actor, "ZombieMan");
        let plan2 = plan(&[
            pf("doom2.wad", "base", base),
            pf("lib.pk3", "addon", lib),
            pf("bd.pk3", "addon", modz),
            pf("room.wad", "addon", room),
        ]);
        assert!(plan2.warnings.is_empty(), "{:?}", plan2.warnings);
        assert!(plan2.steps[4].notes.iter().any(|n| n.contains("Turns off")));
    }
}
