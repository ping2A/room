//! `room inspect`: what a game file is and what it holds, for every format
//! Room mounts — IWAD / PWAD, PK3 / IPK3 / PKE / ZIP, PK7 / IPK7 / 7Z,
//! directories, Duke Nukem 3D (Build) GRP archives, Build `.MAP` and `.ART`
//! files, DeHackEd patches and other loose files.

use anyhow::{Context, Result};
use room_vfs::{ContainerKind, Namespace, Vfs, VfsBuilder};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The IWADINFO Room ships (`wadsrc/room/IWADINFO`).
const IWADINFO: &str = include_str!("../../../wadsrc/room/IWADINFO");

/// Definition and data lumps worth naming, with what they are.
const KNOWN_LUMPS: &[(&str, &str)] = &[
    ("GAMEINFO", "game setup (IWAD, load order)"),
    ("MAPINFO", "map and episode info"),
    ("ZMAPINFO", "map and episode info (ZDoom)"),
    ("UMAPINFO", "map info (UMAPINFO)"),
    ("EMAPINFO", "map info (Eternity)"),
    ("DEHACKED", "DeHackEd patch"),
    ("DECORATE", "actor definitions"),
    ("ZSCRIPT", "ZScript code"),
    ("LOADACS", "ACS libraries to load"),
    ("KEYCONF", "key / class setup"),
    ("SNDINFO", "sound names"),
    ("SNDSEQ", "sound sequences"),
    ("MUSINFO", "music changes"),
    ("REVERBS", "reverb environments"),
    ("GLDEFS", "lights, brightmaps, glows"),
    ("DOOMDEFS", "lights (Doom)"),
    ("ANIMDEFS", "animations and switches"),
    ("ANIMATED", "animations (Boom)"),
    ("SWITCHES", "switches (Boom)"),
    ("TEXTURES", "texture definitions (ZDoom)"),
    ("TEXTURE1", "texture definitions"),
    ("TEXTURE2", "texture definitions"),
    ("PNAMES", "patch names"),
    ("DECALDEF", "decals"),
    ("MODELDEF", "3D models"),
    ("VOXELDEF", "voxels"),
    ("LANGUAGE", "text strings"),
    ("MENUDEF", "menus"),
    ("SBARINFO", "status bar"),
    ("CVARINFO", "console variables"),
    ("LOCKDEFS", "locks"),
    ("TERRAIN", "terrain splashes"),
    ("FONTDEFS", "fonts"),
    ("TRNSLATE", "translations"),
    ("SECRETS", "secret hints"),
    ("XHAIRS", "crosshairs"),
    ("SKININFO", "player skins"),
    ("S_SKIN", "player skin"),
    ("PLAYPAL", "palette"),
    ("COLORMAP", "light colormap"),
    ("TINTTAB", "translucency table"),
    ("ENDOOM", "exit screen"),
    ("GENMIDI", "OPL instruments"),
    ("TITLEPIC", "title screen"),
    ("IWADINFO", "IWAD detection table"),
];

/// One map found in a file.
struct MapRow {
    name: String,
    format: &'static str,
    title: Option<String>,
    things: Option<usize>,
    lines: Option<usize>,
    sectors: Option<usize>,
    nodes: &'static str,
    /// Container holding it, when not the file itself (`maps/x.wad`).
    inside: Option<String>,
}

/// Everything `inspect` found out about one path.
#[derive(Default)]
struct Report {
    path: String,
    kind: String,
    size: u64,
    md5: Option<String>,
    entries: usize,
    game: Option<String>,
    maps: Vec<MapRow>,
    /// Category → count.
    content: BTreeMap<String, usize>,
    /// Known lumps present (name, what, count).
    definitions: Vec<(String, String, usize)>,
    /// Archives mounted inside the file.
    embedded: Vec<String>,
    gameinfo: Vec<(String, String)>,
    notes: Vec<String>,
    warnings: Vec<String>,
    /// `--lumps`: (name, namespace, size, container).
    lumps: Vec<(String, String, u32, String)>,
}

/// `room inspect` options.
pub struct Args {
    /// Files or directories.
    pub paths: Vec<PathBuf>,
    /// JSON instead of text.
    pub json: bool,
    /// Every lump / file.
    pub lumps: bool,
    /// Every map (the text output shows the first 40).
    pub all_maps: bool,
}

/// Run `room inspect`.
///
/// # Errors
/// When a path is missing or unreadable; a file that is not a known format
/// is still described (as a loose file).
pub fn run(args: &Args) -> Result<()> {
    let mut reports = Vec::new();
    for p in &args.paths {
        reports.push(inspect(p, args.lumps).with_context(|| format!("inspect {}", p.display()))?);
    }
    if args.json {
        let v: Vec<Value> = reports.iter().map(to_json).collect();
        let out = if v.len() == 1 {
            v[0].clone()
        } else {
            Value::Array(v)
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        for (i, r) in reports.iter().enumerate() {
            if i > 0 {
                println!();
            }
            print!("{}", to_text(r, args.all_maps));
        }
    }
    Ok(())
}

fn inspect(path: &Path, lumps: bool) -> Result<Report> {
    if !path.exists() {
        anyhow::bail!("path does not exist");
    }
    let mut r = Report {
        path: path.display().to_string(),
        ..Report::default()
    };
    if path.is_dir() {
        r.kind = "directory (mounted like a PK3)".into();
        let vfs = VfsBuilder::new().mount_path(path)?.build()?;
        r.size = dir_size(path);
        describe_vfs(&mut r, &vfs, lumps);
        return Ok(r);
    }
    let data = std::fs::read(path)?;
    r.size = data.len() as u64;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if room_build::is_grp(&data) {
        r.md5 = Some(md5_hex(&data));
        describe_grp(&mut r, &data, lumps)?;
        return Ok(r);
    }
    if data.starts_with(b"IWAD") || data.starts_with(b"PWAD") {
        r.kind = if data.starts_with(b"IWAD") {
            "IWAD"
        } else {
            "PWAD"
        }
        .into();
    } else if data.starts_with(b"PK\x03\x04")
        || matches!(ext.as_str(), "pk3" | "ipk3" | "pke" | "zip")
    {
        r.kind = format!("{} (zip archive)", zip_label(&ext));
    } else if data.starts_with(b"7z\xBC\xAF'\x1C") || matches!(ext.as_str(), "pk7" | "ipk7" | "7z")
    {
        r.kind = format!("{} (7z archive)", zip_label(&ext));
    } else {
        r.md5 = Some(md5_hex(&data));
        describe_loose(&mut r, path, &data, &ext);
        return Ok(r);
    }
    let vfs = VfsBuilder::new().mount_path(path)?.build()?;
    describe_vfs(&mut r, &vfs, lumps);
    if r.kind == "IWAD" || ext.starts_with('i') {
        identify_iwad(&mut r, &vfs);
    }
    Ok(r)
}

fn zip_label(ext: &str) -> String {
    match ext {
        "" => "archive".into(),
        "ipk3" | "ipk7" => format!("{} (IWAD package)", ext.to_ascii_uppercase()),
        e => e.to_ascii_uppercase(),
    }
}

fn md5_hex(data: &[u8]) -> String {
    // The VFS hashes containers; loose files and GRPs are hashed here the
    // same way (MD5 of the whole file).
    let vfs = VfsBuilder::new()
        .mount_memory("x.bin", data.to_vec())
        .and_then(VfsBuilder::build);
    vfs.ok()
        .and_then(|v| v.container(room_vfs::ContainerId(0)).map(|c| hex(&c.md5)))
        .unwrap_or_default()
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn dir_size(path: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(path) else {
        return 0;
    };
    rd.flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                dir_size(&p)
            } else {
                e.metadata().map_or(0, |m| m.len())
            }
        })
        .sum()
}

fn ns_name(ns: Namespace) -> &'static str {
    match ns {
        Namespace::Global => "global",
        Namespace::Sprites => "sprites",
        Namespace::Flats => "flats",
        Namespace::Colormaps => "colormaps",
        Namespace::Acs => "acs",
        Namespace::NewTextures => "textures",
        Namespace::BloodRaw | Namespace::BloodSfx | Namespace::BloodMisc => "blood",
        Namespace::Voices => "voices",
        Namespace::HiRes => "hires",
        Namespace::Voxels => "voxels",
        Namespace::Patches => "patches",
        Namespace::Graphics => "graphics",
        Namespace::Sounds => "sounds",
        Namespace::Music => "music",
        Namespace::Fonts => "fonts",
        Namespace::Models => "models",
        Namespace::Shaders => "shaders",
        Namespace::Brightmaps => "brightmaps",
        Namespace::Materials => "materials",
        Namespace::Filter => "filter",
        Namespace::Maps => "maps",
        Namespace::Skins => "skins",
    }
}

/// Lumps that belong to a map after its marker.
fn is_map_lump(name: &str) -> bool {
    matches!(
        name,
        "THINGS"
            | "LINEDEFS"
            | "SIDEDEFS"
            | "VERTEXES"
            | "SEGS"
            | "SSECTORS"
            | "NODES"
            | "SECTORS"
            | "REJECT"
            | "BLOCKMAP"
            | "BEHAVIOR"
            | "SCRIPTS"
            | "TEXTMAP"
            | "ZNODES"
            | "DIALOGUE"
            | "ENDMAP"
            | "LEAFS"
            | "LIGHTS"
            | "MACROS"
    ) || name.starts_with("GL_")
}

/// Music and sound by header, for lumps outside the `sounds/` / `music/`
/// namespaces (plain WADs).
fn audio_kind(name: &str, head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"MUS\x1a") || head.starts_with(b"MThd") {
        return Some("music");
    }
    if head.starts_with(b"OggS")
        || head.starts_with(b"fLaC")
        || head.starts_with(b"ID3")
        || head.starts_with(b"RIFF")
        || head.starts_with(b"IMPM")
        || head.starts_with(b"Extended Module")
    {
        return Some(if name.starts_with("D_") || name.starts_with("O_") {
            "music"
        } else {
            "sounds"
        });
    }
    // Doom sound format 3 (DMX digital).
    if head.len() >= 2
        && head[0] == 3
        && head[1] == 0
        && (name.starts_with("DS") || name.starts_with("DP"))
    {
        return Some("sounds");
    }
    if name.starts_with("DP") && head.len() >= 2 && head[0] == 0 && head[1] == 0 {
        return Some("sounds (PC speaker)");
    }
    None
}

/// A Doom picture (patch format): plausible size and column offsets.
fn is_doom_picture(b: &[u8]) -> bool {
    if b.len() < 12 {
        return false;
    }
    let w = usize::from(u16::from_le_bytes([b[0], b[1]]));
    let h = u16::from_le_bytes([b[2], b[3]]);
    if w == 0 || h == 0 || w > 4096 || h > 4096 || b.len() < 8 + 4 * w {
        return false;
    }
    (0..w).all(|x| {
        let o =
            u32::from_le_bytes([b[8 + 4 * x], b[9 + 4 * x], b[10 + 4 * x], b[11 + 4 * x]]) as usize;
        o >= 8 + 4 * w && o < b.len()
    })
}

fn describe_vfs(r: &mut Report, vfs: &Vfs, want_lumps: bool) {
    let root = room_vfs::ContainerId(0);
    if let Some(c) = vfs.container(root)
        && c.kind != ContainerKind::Directory
    {
        r.md5 = Some(hex(&c.md5));
    }
    for i in 1..vfs.container_count() {
        if let Some(c) = vfs.container(room_vfs::ContainerId(i as u32)) {
            let kind = match c.kind {
                ContainerKind::Wad => "WAD",
                ContainerKind::Zip => "zip",
                ContainerKind::SevenZ => "7z",
                ContainerKind::Directory => "directory",
                ContainerKind::LooseFile => "file",
            };
            r.embedded
                .push(format!("{} ({kind})", short_container(&c.name)));
        }
    }
    r.warnings.extend(vfs.warnings().iter().cloned());
    r.entries = vfs.len();

    // Map scan in lump order, per container.
    let ids: Vec<_> = vfs.iter_lumps().collect();
    let names: Vec<String> = ids
        .iter()
        .map(|&id| {
            vfs.lump(id)
                .map(|l| l.short_name.to_string().to_ascii_uppercase())
                .unwrap_or_default()
        })
        .collect();
    let mut in_map = vec![false; ids.len()];
    let mut i = 0;
    while i + 1 < ids.len() {
        let (Some(a), Some(b)) = (vfs.lump(ids[i]), vfs.lump(ids[i + 1])) else {
            i += 1;
            continue;
        };
        if a.container != b.container
            || names[i].is_empty()
            || !(names[i + 1] == "THINGS" || names[i + 1] == "TEXTMAP")
        {
            i += 1;
            continue;
        }
        in_map[i] = true;
        let mut sizes: BTreeMap<&str, u32> = BTreeMap::new();
        let mut j = i + 1;
        while j < ids.len()
            && vfs.lump(ids[j]).is_some_and(|l| l.container == a.container)
            && is_map_lump(&names[j])
        {
            in_map[j] = true;
            sizes.insert(names[j].as_str(), vfs.lump(ids[j]).map_or(0, |l| l.size));
            j += 1;
        }
        let udmf = sizes.contains_key("TEXTMAP");
        let hexen = sizes.contains_key("BEHAVIOR");
        let count = |lump: &str, doom: usize, hx: usize| {
            (!udmf)
                .then(|| {
                    sizes
                        .get(lump)
                        .map(|&s| s as usize / if hexen { hx } else { doom })
                })
                .flatten()
        };
        let nodes = if sizes.contains_key("ZNODES") {
            "ZNODES"
        } else if sizes.keys().any(|k| k.starts_with("GL_")) {
            "GL nodes"
        } else if sizes.get("NODES").is_some_and(|&s| s > 0) {
            "vanilla"
        } else {
            "none (built at load)"
        };
        let inside = (a.container != root)
            .then(|| vfs.container(a.container).map(|c| short_container(&c.name)))
            .flatten();
        let name = names[i].clone();
        r.maps.retain(|m| m.name != name);
        r.maps.push(MapRow {
            name,
            format: if udmf {
                "UDMF"
            } else if hexen {
                "Hexen"
            } else {
                "Doom"
            },
            title: None,
            things: count("THINGS", 10, 20),
            lines: count("LINEDEFS", 14, 16),
            sectors: count("SECTORS", 26, 26),
            nodes,
            inside,
        });
        i = j;
    }
    r.maps.sort_by_key(|a| map_key(&a.name));
    map_titles(r, vfs);

    // Content by category.
    let mut sprite_names = std::collections::BTreeSet::new();
    let mut known: BTreeMap<&str, usize> = BTreeMap::new();
    for (k, &id) in ids.iter().enumerate() {
        let Some(l) = vfs.lump(id) else { continue };
        if want_lumps {
            let shown = if l.full_path.is_empty() || l.full_path == names[k].to_ascii_lowercase() {
                names[k].clone()
            } else {
                l.full_path.clone()
            };
            let owner = vfs
                .container(l.container)
                .map(|c| short_container(&c.name))
                .unwrap_or_default();
            r.lumps
                .push((shown, ns_name(l.namespace).into(), l.size, owner));
        }
        if in_map[k] || l.flags.contains(room_vfs::LumpFlags::MARKER) {
            continue;
        }
        let name = names[k].as_str();
        if let Some((n, _)) = KNOWN_LUMPS.iter().find(|(n, _)| *n == name) {
            *known.entry(n).or_default() += 1;
        }
        // Embedded WADs are listed under "contains".
        if l.full_path.ends_with(".wad") {
            continue;
        }
        let cat = match l.namespace {
            Namespace::Global => {
                if name.starts_with("DEMO")
                    && name[4..].bytes().all(|b| b.is_ascii_digit())
                    && name.len() > 4
                {
                    "demos".to_string()
                } else if KNOWN_LUMPS.iter().any(|(n, _)| *n == name) {
                    continue;
                } else {
                    let head = vfs
                        .read(id)
                        .map(|b| b.slice(..b.len().min(16)))
                        .unwrap_or_default();
                    match audio_kind(name, &head) {
                        Some(c) => c.to_string(),
                        None if vfs.read(id).is_ok_and(|b| is_doom_picture(&b)) => {
                            "graphics (Doom pictures)".to_string()
                        }
                        None => "other lumps".to_string(),
                    }
                }
            }
            Namespace::Sprites => {
                if name.len() >= 4 {
                    sprite_names.insert(name[..4].to_string());
                }
                "sprite frames".to_string()
            }
            ns => ns_name(ns).to_string(),
        };
        *r.content.entry(cat).or_default() += 1;
    }
    if !sprite_names.is_empty() {
        r.content.insert("sprite names".into(), sprite_names.len());
    }
    // Textures listed in TEXTURE1/2 and patches in PNAMES.
    for (lump, label) in [
        ("TEXTURE1", "textures (TEXTURE1)"),
        ("TEXTURE2", "textures (TEXTURE2)"),
        ("PNAMES", "patch names (PNAMES)"),
    ] {
        if let Some(id) = vfs.find(lump, Namespace::Global)
            && let Ok(b) = vfs.read(id)
            && b.len() >= 4
        {
            let n = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            if n > 0 {
                r.content.insert(label.into(), n as usize);
            }
        }
    }
    for (name, what) in KNOWN_LUMPS {
        if let Some(&n) = known.get(name) {
            r.definitions.push(((*name).into(), (*what).into(), n));
        }
    }
    if let Some(id) = vfs.find("GAMEINFO", Namespace::Global)
        && let Ok(b) = vfs.read(id)
    {
        r.gameinfo = parse_gameinfo(&String::from_utf8_lossy(&b));
    }
    if !r.maps.is_empty() {
        let e = r.maps.iter().filter(|m| is_exmy(&m.name)).count();
        let m = r.maps.iter().filter(|m| is_mapxx(&m.name)).count();
        if e > 0 && m == 0 {
            r.notes
                .push("map slots ExMy: Doom 1 / Ultimate Doom style".into());
        } else if m > 0 && e == 0 {
            r.notes.push("map slots MAPxx: Doom II style".into());
        }
    }
}

fn short_container(name: &str) -> String {
    Path::new(name)
        .file_name()
        .map_or_else(|| name.to_string(), |n| n.to_string_lossy().into_owned())
}

fn is_exmy(n: &str) -> bool {
    let b = n.as_bytes();
    b.len() == 4 && b[0] == b'E' && b[2] == b'M' && b[1].is_ascii_digit() && b[3].is_ascii_digit()
}

fn is_mapxx(n: &str) -> bool {
    n.len() == 5 && n.starts_with("MAP") && n[3..].bytes().all(|b| b.is_ascii_digit())
}

fn map_key(n: &str) -> (u8, Vec<(String, u64)>) {
    (
        if is_exmy(n) {
            0
        } else if is_mapxx(n) {
            1
        } else {
            2
        },
        natural(n),
    )
}

/// Map titles from (Z/U)MAPINFO, best effort: a lump that does not parse is
/// skipped.
fn map_titles(r: &mut Report, vfs: &Vfs) {
    if r.maps.is_empty() {
        return;
    }
    let mut titles: BTreeMap<String, String> = BTreeMap::new();
    for (lump, umap) in [("MAPINFO", false), ("ZMAPINFO", false), ("UMAPINFO", true)] {
        let Some(id) = vfs.find(lump, Namespace::Global) else {
            continue;
        };
        let Ok(b) = vfs.read(id) else { continue };
        let text = String::from_utf8_lossy(&b);
        let parsed = if umap {
            room_defs::parse_umapinfo(&text)
        } else {
            room_defs::mapinfo::parse_mapinfo(&text)
        };
        match parsed {
            Ok(info) => {
                for (k, m) in info.maps {
                    if let Some(t) = m.title.filter(|t| !t.is_empty()) {
                        titles.insert(k.to_ascii_uppercase(), t);
                    }
                }
            }
            Err(e) => r
                .warnings
                .push(format!("{lump}: not read for map titles ({e})")),
        }
    }
    for m in &mut r.maps {
        m.title = titles.get(&m.name).cloned();
    }
}

fn parse_gameinfo(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let l = l.split("//").next()?.trim();
            let (k, v) = l.split_once('=')?;
            Some((
                k.trim().to_ascii_uppercase(),
                v.trim().trim_matches('"').to_string(),
            ))
        })
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// Known IWAD releases by MD5 of the whole file. A wrong or missing entry
/// only means no match; the IWADINFO check below still runs.
const KNOWN_IWADS: &[(&str, &str)] = &[
    (
        "f0cefca49926d00903cf57551d901abe",
        "Doom shareware 1.9 (doom1.wad)",
    ),
    (
        "1cd63c5ddff1bf8ce844237f580e9cf3",
        "Doom registered 1.9 (doom.wad)",
    ),
    (
        "c4fe9fd920207691a9f493668e0a2083",
        "The Ultimate Doom 1.9 (doom.wad)",
    ),
    (
        "25e1459ca71d321525f84628f45ca8cd",
        "Doom II 1.9 (doom2.wad)",
    ),
    (
        "75c8cf89566741fa9d22447604053bd7",
        "Final Doom: The Plutonia Experiment (plutonia.wad)",
    ),
    (
        "4e158d9953c79ccf97bd0663244cc6b6",
        "Final Doom: TNT Evilution (tnt.wad)",
    ),
    (
        "66d686b1ed6d35ff103f15dbd30e0341",
        "Heretic 1.3 (heretic.wad)",
    ),
    ("abb033caf81e26f12a2103e1fa25453f", "Hexen 1.1 (hexen.wad)"),
    (
        "2fed2031a5b03892106e0f117f17901f",
        "Strife 1.2 (strife1.wad)",
    ),
    (
        "b93be13d05148dd01614bc205a03648e",
        "Freedoom: Phase 1 0.13.0 (freedoom1.wad)",
    ),
    (
        "cd666466759b5e5f63af93c5f0ffd0a1",
        "Freedoom: Phase 2 0.13.0 (freedoom2.wad)",
    ),
];

fn identify_iwad(r: &mut Report, vfs: &Vfs) {
    if let Some((_, name)) = r
        .md5
        .as_deref()
        .and_then(|m| KNOWN_IWADS.iter().find(|(k, _)| *k == m))
    {
        r.game = Some(format!("{name} (known release, MD5 match)"));
        return;
    }
    let found = room_vfs::parse_iwadinfo(IWADINFO)
        .ok()
        .and_then(|info| room_vfs::detect_iwad(vfs, &info));
    r.game = Some(match found {
        Some(m) => format!("{} ({}, by its lumps)", m.record.name, m.record.game),
        None => "unidentified IWAD (unknown release)".into(),
    });
}

fn describe_grp(r: &mut Report, data: &[u8], want_lumps: bool) -> Result<()> {
    let grp = room_build::Grp::parse(data).map_err(|e| anyhow::anyhow!("{e}"))?;
    r.kind = "GRP (Build engine archive)".into();
    let names: Vec<String> = grp.names().map(str::to_owned).collect();
    r.entries = names.len();
    r.game = Some(match grp.other_build_game() {
        Some(g) => format!("{g} (not supported by Room; Duke Nukem 3D only)"),
        None if grp.get("GAME.CON").is_some() || grp.get("DUKE3D.CON").is_some() => {
            "Duke Nukem 3D".into()
        }
        None => "Build content (no GAME.CON: an add-on or a partial GRP)".into(),
    });
    let mut tiles = room_build::TileSet::new();
    for n in &names {
        let ext = n.rsplit_once('.').map_or("", |(_, e)| e);
        let body = grp.get(n).unwrap_or_default();
        if want_lumps {
            r.lumps.push((
                n.clone(),
                ext.to_ascii_lowercase(),
                body.len() as u32,
                String::new(),
            ));
        }
        let cat = match ext {
            "MAP" => {
                let stem = n.trim_end_matches(".MAP").to_string();
                let parsed = room_build::parse_map(body).ok();
                r.maps.push(MapRow {
                    name: stem,
                    format: "Build",
                    title: None,
                    things: parsed.as_ref().map(|m| m.sprites.len()),
                    lines: parsed.as_ref().map(|m| m.walls.len()),
                    sectors: parsed.as_ref().map(|m| m.sectors.len()),
                    nodes: parsed.as_ref().map_or("unreadable", |m| match m.version {
                        7 => "v7",
                        8 => "v8",
                        _ => "v9",
                    }),
                    inside: None,
                });
                continue;
            }
            "ART" => {
                if tiles.add_art(body).is_err() {
                    r.warnings.push(format!("{n}: unreadable ART"));
                }
                "art files"
            }
            "CON" => "CON scripts",
            "VOC" | "WAV" => "sounds",
            "MID" | "OGG" | "FLAC" => "music",
            "DMO" => "demos",
            "ANM" => "animations",
            "RTS" => "RTS sound bank",
            "DAT" => "data (palette, tables)",
            "DEF" => "DEF scripts",
            "KVX" => "voxels",
            _ => "other files",
        };
        *r.content.entry(cat.into()).or_default() += 1;
    }
    if !tiles.is_empty() {
        r.content.insert("tiles".into(), tiles.len());
    }
    // Episodes and level names from USER.CON, as the game shows them;
    // maps in campaign order, then the rest by name.
    let user = grp
        .get("USER.CON")
        .map(String::from_utf8_lossy)
        .unwrap_or_default();
    let (levels, episodes) = room_build::parse_user_con(&user);
    if !levels.is_empty() {
        r.notes.push(format!(
            "{} levels in {} episodes (USER.CON): {}",
            levels.len(),
            episodes.len(),
            episodes.join(", ")
        ));
    }
    let campaign = |name: &str| {
        levels
            .iter()
            .find(|l| l.file.trim_end_matches(".MAP").eq_ignore_ascii_case(name))
    };
    for m in &mut r.maps {
        if let Some(l) = campaign(&m.name) {
            m.title = Some(format!("E{}L{} {}", l.volume + 1, l.level + 1, l.name));
        }
    }
    r.maps.sort_by_key(|m| {
        campaign(&m.name).map_or((1, 0, 0, natural(&m.name)), |l| {
            (0, l.volume, l.level, natural(&m.name))
        })
    });
    for c in ["GAME.CON", "USER.CON", "DEFS.CON"] {
        if grp.get(c).is_some() {
            r.definitions.push((c.into(), "CON script".into(), 1));
        }
    }
    Ok(())
}

/// Sort key with digit runs compared as numbers (`E2L2` before `E2L10`).
fn natural(name: &str) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut num: Option<u64> = None;
    for c in name.chars() {
        if let Some(d) = c.to_digit(10) {
            num = Some(
                num.unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(u64::from(d)),
            );
        } else {
            if let Some(n) = num.take() {
                out.push((std::mem::take(&mut text), n));
            }
            text.push(c);
        }
    }
    out.push((text, num.unwrap_or(0)));
    out
}

fn describe_loose(r: &mut Report, path: &Path, data: &[u8], ext: &str) {
    r.entries = 1;
    let text_head = String::from_utf8_lossy(&data[..data.len().min(64)]).to_ascii_lowercase();
    if text_head.starts_with("patch file for dehacked") || matches!(ext, "deh" | "bex") {
        r.kind = "DeHackEd patch (loose file)".into();
        let text = String::from_utf8_lossy(data);
        let mut blocks: BTreeMap<String, usize> = BTreeMap::new();
        for l in text.lines() {
            let w = l.split_whitespace().next().unwrap_or("");
            let key = match w.to_ascii_lowercase().as_str() {
                "thing" | "frame" | "weapon" | "ammo" | "sound" | "sprite" | "misc" | "cheat"
                | "pointer" | "text" => w.to_ascii_lowercase(),
                s if s.starts_with('[') => s.trim_matches(['[', ']']).to_string(),
                _ => continue,
            };
            *blocks.entry(key).or_default() += 1;
        }
        r.content = blocks;
        if let Some(v) = text
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("doom version"))
        {
            r.notes.push(v.trim().to_string());
        }
        r.notes.push("mounts as the DEHACKED lump".into());
        return;
    }
    if ext == "map"
        && let Ok(m) = room_build::parse_map(data)
    {
        r.kind = format!("Build map (version {})", m.version);
        r.maps.push(MapRow {
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().to_ascii_uppercase())
                .unwrap_or_default(),
            format: "Build",
            title: None,
            things: Some(m.sprites.len()),
            lines: Some(m.walls.len()),
            sectors: Some(m.sectors.len()),
            nodes: "",
            inside: None,
        });
        r.notes
            .push("open it with `room duke soft-view --map-file`".into());
        return;
    }
    if ext == "art" {
        let mut tiles = room_build::TileSet::new();
        if tiles.add_art(data).is_ok() {
            r.kind = "Build ART tile file".into();
            r.content.insert("tiles".into(), tiles.len());
            return;
        }
    }
    let lump = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_uppercase())
        .unwrap_or_default();
    let what = audio_kind(&lump, &data[..data.len().min(16)]).unwrap_or("data");
    r.kind = format!("loose file ({what})");
    r.notes.push(format!(
        "mounts as the single lump {}",
        lump.chars().take(8).collect::<String>()
    ));
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {} ({bytes} bytes)", UNITS[u])
    }
}

fn opt(n: Option<usize>) -> String {
    n.map_or_else(|| "-".into(), |n| n.to_string())
}

fn to_text(r: &Report, all_maps: bool) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{}", r.path);
    let _ = writeln!(s, "  type         {}", r.kind);
    let _ = writeln!(s, "  size         {}", human(r.size));
    if let Some(m) = &r.md5 {
        let _ = writeln!(s, "  md5          {m}");
    }
    let _ = writeln!(s, "  entries      {}", r.entries);
    if let Some(g) = &r.game {
        let _ = writeln!(s, "  game         {g}");
    }
    for (k, v) in &r.gameinfo {
        let _ = writeln!(s, "  gameinfo     {k} = {v}");
    }
    for n in &r.notes {
        let _ = writeln!(s, "  note         {n}");
    }
    if !r.embedded.is_empty() {
        let _ = writeln!(s, "  contains     {}", r.embedded.join(", "));
    }
    if !r.content.is_empty() {
        let _ = writeln!(s, "  content");
        for (k, v) in &r.content {
            let _ = writeln!(s, "    {k:<26} {v}");
        }
    }
    if !r.definitions.is_empty() {
        let _ = writeln!(s, "  definitions");
        for (n, what, c) in &r.definitions {
            let times = if *c > 1 {
                format!(" ×{c}")
            } else {
                String::new()
            };
            let _ = writeln!(s, "    {:<26} {what}", format!("{n}{times}"));
        }
    }
    if !r.maps.is_empty() {
        let build = r.maps.iter().any(|m| m.format == "Build");
        let _ = writeln!(s, "  maps ({})", r.maps.len());
        let (t, l) = if build {
            ("sprites", "walls")
        } else {
            ("things", "lines")
        };
        let _ = writeln!(
            s,
            "    {:<9} {:<6} {:>7} {:>7} {:>7}  {:<20} title",
            "name",
            "format",
            t,
            l,
            "sectors",
            if build { "version" } else { "nodes" }
        );
        let limit = if all_maps { usize::MAX } else { 40 };
        for m in r.maps.iter().take(limit) {
            let mut title = m.title.clone().unwrap_or_default();
            if let Some(c) = &m.inside {
                if !title.is_empty() {
                    title.push(' ');
                }
                let _ = write!(title, "[in {c}]");
            }
            let row = format!(
                "    {:<9} {:<6} {:>7} {:>7} {:>7}  {:<20} {title}",
                m.name,
                m.format,
                opt(m.things),
                opt(m.lines),
                opt(m.sectors),
                m.nodes
            );
            let _ = writeln!(s, "{}", row.trim_end());
        }
        if r.maps.len() > limit {
            let _ = writeln!(s, "    … {} more (--all-maps)", r.maps.len() - limit);
        }
    }
    if !r.lumps.is_empty() {
        let _ = writeln!(s, "  lumps");
        for (n, ns, size, owner) in &r.lumps {
            let _ = writeln!(s, "    {n:<32} {ns:<10} {size:>10}  {owner}");
        }
    }
    for w in &r.warnings {
        let _ = writeln!(s, "  warning      {w}");
    }
    s
}

fn to_json(r: &Report) -> Value {
    json!({
        "path": r.path,
        "type": r.kind,
        "size": r.size,
        "md5": r.md5,
        "entries": r.entries,
        "game": r.game,
        "gameinfo": r.gameinfo.iter().map(|(k, v)| json!({"key": k, "value": v})).collect::<Vec<_>>(),
        "notes": r.notes,
        "contains": r.embedded,
        "content": r.content,
        "definitions": r.definitions.iter().map(|(n, w, c)| json!({"name": n, "what": w, "count": c})).collect::<Vec<_>>(),
        "maps": r.maps.iter().map(|m| json!({
            "name": m.name,
            "format": m.format,
            "title": m.title,
            "things": m.things,
            "lines": m.lines,
            "sectors": m.sectors,
            "nodes": m.nodes,
            "inside": m.inside,
        })).collect::<Vec<_>>(),
        "lumps": r.lumps.iter().map(|(n, ns, size, owner)| json!({"name": n, "namespace": ns, "size": size, "container": owner})).collect::<Vec<_>>(),
        "warnings": r.warnings,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn wad(magic: &[u8; 4], lumps: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = if magic == b"IWAD" {
            room_vfs::WadWriter::iwad()
        } else {
            room_vfs::WadWriter::pwad()
        };
        for (n, d) in lumps {
            w.add(n, d.to_vec());
        }
        w.finish().unwrap()
    }

    #[test]
    fn pwad_maps_and_definitions() {
        let dir = std::env::temp_dir().join(format!("room-inspect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.wad");
        let data = wad(
            b"PWAD",
            &[
                ("MAPINFO", b"map MAP01 \"Hangar Test\"\n"),
                ("MAP01", b""),
                ("THINGS", &[0u8; 30]),
                ("LINEDEFS", &[0u8; 28]),
                ("SIDEDEFS", &[0u8; 30]),
                ("VERTEXES", &[0u8; 8]),
                ("SECTORS", &[0u8; 26]),
                ("MAP02", b""),
                ("TEXTMAP", b"namespace = \"zdoom\";"),
                ("ENDMAP", b""),
                ("D_RUNNIN", b"MUS\x1a...."),
                ("DEHACKED", b"Patch File for DeHackEd v3.0"),
            ],
        );
        std::fs::write(&path, data).unwrap();
        let r = inspect(&path, true).unwrap();
        assert_eq!(r.kind, "PWAD");
        assert_eq!(r.maps.len(), 2);
        assert_eq!(
            (r.maps[0].format, r.maps[0].things, r.maps[0].lines),
            ("Doom", Some(3), Some(2))
        );
        assert_eq!(r.maps[0].title.as_deref(), Some("Hangar Test"));
        assert_eq!(r.maps[1].format, "UDMF");
        assert_eq!(r.content.get("music"), Some(&1));
        assert!(r.definitions.iter().any(|(n, _, _)| n == "DEHACKED"));
        assert!(r.definitions.iter().any(|(n, _, _)| n == "MAPINFO"));
        let text = to_text(&r, false);
        assert!(
            text.contains("MAP01") && text.contains("Hangar Test"),
            "{text}"
        );
        assert!(to_json(&r)["maps"][1]["format"] == "UDMF");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loose_dehacked() {
        let dir = std::env::temp_dir().join(format!("room-inspect-deh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.deh");
        std::fs::write(
            &path,
            "Patch File for DeHackEd v3.0\nDoom version = 21\nThing 1 (Player)\nFrame 5\n",
        )
        .unwrap();
        let r = inspect(&path, false).unwrap();
        assert!(r.kind.starts_with("DeHackEd"), "{}", r.kind);
        assert_eq!(r.content.get("thing"), Some(&1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tmp(name: &str, data: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("room-inspect-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, data).unwrap();
        p
    }

    #[test]
    fn iwad_by_lumps_tells_doom2_from_freedoom() {
        let lumps: Vec<(&str, &[u8])> = vec![
            ("PLAYPAL", &[0u8; 768]),
            ("TITLEPIC", b"x"),
            ("MAP01", b""),
            ("THINGS", b""),
            ("MAP02", b""),
            ("THINGS", b""),
        ];
        let r = inspect(&tmp("a.wad", &wad(b"IWAD", &lumps)), false).unwrap();
        assert!(
            r.game.as_deref().unwrap().starts_with("DOOM 2"),
            "{:?}",
            r.game
        );
        let mut with = lumps.clone();
        with.push(("FREEDOOM", b"1"));
        let r = inspect(&tmp("b.wad", &wad(b"IWAD", &with)), false).unwrap();
        assert!(
            r.game.as_deref().unwrap().starts_with("Freedoom: Phase 2"),
            "{:?}",
            r.game
        );
    }

    #[test]
    fn pk3_gameinfo_and_map_wad() {
        let map = wad(
            b"PWAD",
            &[
                ("MAP07", b""),
                ("TEXTMAP", b"namespace = \"zdoom\";"),
                ("ENDMAP", b""),
            ],
        );
        let zip = room_vfs::zip_from_files(&[
            (
                "gameinfo.txt",
                b"IWAD = \"doom2.wad\"\nLOAD = \"extra.pk3\"\n",
            ),
            ("zscript.txt", b"version \"4.10\""),
            ("sprites/possa1.png", b"x"),
            ("maps/map07.wad", &map),
        ])
        .unwrap();
        let r = inspect(&tmp("m.pk3", &zip), false).unwrap();
        assert_eq!(r.kind, "PK3 (zip archive)");
        assert!(
            r.gameinfo.contains(&("IWAD".into(), "doom2.wad".into())),
            "{:?}",
            r.gameinfo
        );
        assert!(r.definitions.iter().any(|(n, _, _)| n == "ZSCRIPT"));
        assert_eq!(r.content.get("sprite names"), Some(&1));
        let m = r
            .maps
            .iter()
            .find(|m| m.name == "MAP07")
            .expect("map in maps/map07.wad");
        assert_eq!(m.format, "UDMF");
    }
}
