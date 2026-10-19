//! GRP → Room PWAD.

use crate::anm::{decode_stills, title_frame};
use crate::art::{TRANSPARENT, Tile, TileSet};
use crate::con::{LevelInfo, parse_menu_music, parse_skill_names, parse_user_con};
use crate::condata::ConData;
use crate::convert::{GameTiles, convert_map_with, doom_map_name, switch_name, tile_name};
use crate::duke::Duke;
use crate::error::BuildError;
use crate::grp::Grp;
use crate::lookup::Lookups;
use crate::map::parse_map;
use crate::palette::Palette;
use crate::sprites::Origin;
use room_vfs::WadWriter;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Title menu over `TITLEPIC` (Doom's `M_NGAME` patches are not in the GRP).
const DUKE_MENUDEF: &str = r#"
ListMenu "MainMenu"
{
	Title "DUKE NUKEM 3D"
	Position 48, 80
	TextItem "New Game", "n", "EpisodeMenu"
	TextItem "Load Game", "l", "LoadGameMenu"
	TextItem "Options", "o", "OptionsMenu"
	TextItem "Quit Game", "q", "QuitMenu"
}

OptionMenu "OptionsMenu"
{
	Title "OPTIONS"
	Option "Auto Aim", "autoaim", "OnOff"
	Option "Mouse Aiming", "mouse_look", "OnOff"
	Option "Invert Mouse", "invertmouse", "OnOff"
	Option "Always Run", "cl_run", "OnOff"
	Option "Show FPS", "vid_fps", "OnOff"
	Command "Write Config", "writeconfig"
}
"#;

/// Result of [`convert_grp`].
#[derive(Debug, Clone)]
pub struct GrpWad {
    /// PWAD bytes.
    pub wad: Vec<u8>,
    /// Room map names in play order.
    pub maps: Vec<String>,
    /// Maps or tiles that could not be converted.
    pub warnings: Vec<String>,
}

/// Convert a Duke Nukem 3D GRP into a Room PWAD (see the crate docs).
pub fn grp_to_wad(data: &[u8]) -> Result<Vec<u8>, BuildError> {
    convert_grp(data).map(|g| g.wad)
}

/// Like [`grp_to_wad`] with the map list and conversion warnings.
pub fn convert_grp(data: &[u8]) -> Result<GrpWad, BuildError> {
    convert_grp_with_maps(data, &[])
}

/// Convert a GRP plus user `.MAP` files (`(file name, bytes)`). The user maps
/// use the GRP's tiles, CON actors and sounds, and replace a GRP map of the
/// same name.
pub fn convert_grp_with_maps(
    data: &[u8],
    user_maps: &[(String, Vec<u8>)],
) -> Result<GrpWad, BuildError> {
    let grp = Grp::parse(data)?;
    if let Some(game) = grp.other_build_game() {
        return Err(BuildError::Unsupported(game));
    }
    let palette = Palette::parse(
        grp.get("PALETTE.DAT")
            .ok_or_else(|| BuildError::Missing("PALETTE.DAT".into()))?,
    )?;
    let mut warnings = Vec::new();

    let mut art_names: Vec<&str> = grp
        .names()
        .filter(|n| n.starts_with("TILES") && n.ends_with(".ART"))
        .collect();
    art_names.sort_unstable();
    art_names.dedup();
    let mut tiles = TileSet::new();
    for name in art_names {
        if let Some(body) = grp.get(name) {
            if let Err(e) = tiles.add_art(body) {
                warnings.push(format!("{name}: {e}"));
            }
        }
    }
    if tiles.is_empty() {
        return Err(BuildError::Missing("TILES*.ART".into()));
    }

    let user_con = grp
        .get("USER.CON")
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    let (levels, volumes) = parse_user_con(&user_con);
    let skill_names = parse_skill_names(&user_con);
    let mut con_text = String::new();
    for name in ["DEFS.CON", "USER.CON", "GAME.CON"] {
        if let Some(b) = grp.get(name) {
            con_text.push_str(&String::from_utf8_lossy(b));
            con_text.push('\n');
        }
    }
    let con = ConData::parse(&con_text);
    let game = GameTiles::from_con(&con);
    let lookups = grp
        .get("LOOKUP.DAT")
        .and_then(|b| Lookups::parse(b).ok())
        .unwrap_or_default();
    let mut duke = Duke::new(&con, &tiles, &grp);

    // Maps in level-table order, then any other map files by name.
    let mut files: Vec<String> = levels.iter().map(|l| l.file.clone()).collect();
    let mut rest: Vec<String> = grp
        .names()
        .filter(|n| n.ends_with(".MAP"))
        .map(str::to_owned)
        .collect();
    rest.sort_unstable();
    files.extend(rest);
    let mut seen = BTreeSet::new();
    files.retain(|f| grp.get(f).is_some() && seen.insert(f.clone()));
    let user_name = |f: &str| {
        let base = f
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(f)
            .to_ascii_uppercase();
        if base.ends_with(".MAP") {
            base
        } else {
            format!("{base}.MAP")
        }
    };
    let user: Vec<(String, &[u8])> = user_maps
        .iter()
        .map(|(f, b)| (user_name(f), b.as_slice()))
        .collect();
    let user_body = |f: &str| user.iter().rev().find(|(u, _)| u == f).map(|(_, b)| *b);
    let mut sources: Vec<(String, &[u8])> = files
        .iter()
        .filter_map(|f| user_body(f).or_else(|| grp.get(f)).map(|b| (f.clone(), b)))
        .collect();
    for (u, _) in &user {
        if !sources.iter().any(|(f, _)| f == u) {
            if let Some(b) = user_body(u) {
                sources.push((u.clone(), b));
            }
        }
    }

    let mut converted = Vec::new();
    let mut used = BTreeSet::new();
    for (file, body) in &sources {
        let Some(name) = doom_map_name(file) else {
            warnings.push(format!("{file}: name is not a map lump"));
            continue;
        };
        match parse_map(body)
            .and_then(|m| convert_map_with(&m, &tiles, &game, &mut duke, Some(&palette)))
        {
            Ok(c) => {
                used.extend(c.tiles.iter().copied());
                let info = levels
                    .iter()
                    .find(|l| l.file.eq_ignore_ascii_case(file))
                    .cloned();
                converted.push((name, c, info));
            }
            Err(e) => warnings.push(format!("{file}: {e}")),
        }
    }
    if converted.is_empty() {
        return Err(BuildError::Missing("a convertible .MAP".into()));
    }
    let content = duke.finish();

    let mut w = WadWriter::pwad();
    w.add("PLAYPAL", palette.playpal());
    w.add("COLORMAP", palette.colormap());
    add_anm_pics(&mut w, &mut warnings, &grp, &palette);
    if !content.decorate.is_empty() {
        w.add("DECORATE", content.decorate.into_bytes());
        w.add("KEYCONF", content.keyconf.into_bytes());
    }
    w.add("MENUDEF", DUKE_MENUDEF.as_bytes().to_vec());
    if !content.sndinfo.is_empty() {
        w.add("SNDINFO", content.sndinfo.into_bytes());
    }
    if !content.sndseq.is_empty() {
        w.add("SNDSEQ", content.sndseq.into_bytes());
    }
    if !content.conevt.is_empty() {
        w.add("CONEVT", content.conevt.into_bytes());
    }
    for (lump, file) in &content.sounds {
        if let Some(body) = grp.get(file) {
            w.add(lump, body.to_vec());
        }
    }
    let mut pic = |w: &mut WadWriter, name: &str, tile: i32, pal: u8, origin: Option<Origin>| {
        let Some(t) = tiles.get(tile) else { return };
        match encode_tile(t, &lookups, pal, origin) {
            Ok(lump) => {
                w.add(name, lump);
            }
            Err(e) => warnings.push(format!("{name}: {e}")),
        }
    };
    // Status bar, digits, key icons and the console font (`STCFNnnn`).
    let hud_tile = |name: &str, default: i32| con.value(name).unwrap_or(default);
    if tiles.get(hud_tile("BOTTOMSTATUSBAR", 2462)).is_some() {
        pic(
            &mut w,
            "DUKEBAR",
            hud_tile("BOTTOMSTATUSBAR", 2462),
            0,
            None,
        );
        for d in 0..10 {
            pic(
                &mut w,
                &format!("DKNUM{d}"),
                hud_tile("DIGITALNUM", 2472) + d,
                0,
                None,
            );
            pic(
                &mut w,
                &format!("DKSM{d}"),
                hud_tile("THREEBYFIVE", 3010) + d,
                0,
                None,
            );
        }
        for (suffix, pal) in [("B", 0), ("R", 21), ("Y", 23)] {
            pic(
                &mut w,
                &format!("DKKEY{suffix}"),
                hud_tile("ACCESS_ICON", 2471),
                pal,
                None,
            );
        }
        let font = hud_tile("STARTALPHANUM", 2822);
        for c in 33..=126 {
            pic(&mut w, &format!("STCFN{c:03}"), font + c - 33, 0, None);
        }
    }
    // Menu cursor (Doom's skull) as Duke's spinning nuke icon.
    let nuke = con.value("SPINNINGNUKEICON").unwrap_or(896);
    pic(&mut w, "M_SKULL1", nuke, 0, None);
    pic(&mut w, "M_SKULL2", nuke + 3, 0, None);
    // Wall bullet marks: Doom's CHIP* / PUFF* lumps do not exist here.
    let hole = con.value("BULLETHOLE").unwrap_or(952);
    if tiles.get(hole).is_some() {
        pic(&mut w, "DKBHOLE", hole, 0, None);
        w.add(
            "DECALDEF",
            b"decal BulletChip\n{\n\tpic DKBHOLE\n\trandomflipx\n}\n\
              generator DukePistol BulletChip\n\
              generator DukeShotgun BulletChip\n\
              generator DukeRipper BulletChip\n"
                .to_vec(),
        );
    }
    w.marker("S_START");
    let mut seen = BTreeSet::new();
    for l in content.sprites.lumps() {
        if seen.insert(l.name.clone()) {
            pic(&mut w, &l.name, l.tile, l.pal, Some(l.origin));
        }
    }
    w.marker("S_END");

    let mut mapinfo = String::new();
    // Room skills 1–5 play as Duke skills 1, 1, 2, 3, 4 (see `convert`).
    if skill_names.len() >= 4 {
        mapinfo.push_str("clearskills\n");
        for (id, duke, suffix) in [
            ("baby", 0, " (Easy)"),
            ("easy", 0, ""),
            ("normal", 1, ""),
            ("hard", 2, ""),
            ("nightmare", 3, ""),
        ] {
            let name = format!("{}{suffix}", title_case(&skill_names[duke])).replace('"', "'");
            let _ = writeln!(mapinfo, "skill {id} {{ name = \"{name}\" }}");
        }
    }
    let mut episodes = BTreeSet::new();
    for (name, ..) in &converted {
        let ep = name
            .strip_prefix('E')
            .and_then(|r| r.split_once('M'))
            .and_then(|(e, _)| e.parse::<usize>().ok());
        if let Some(title) = ep
            .filter(|&e| e >= 1 && episodes.insert(e))
            .and_then(|e| volumes.get(e - 1))
            .filter(|t| !t.is_empty())
        {
            if episodes.len() == 1 {
                mapinfo.push_str("clearepisodes\n");
            }
            let _ = writeln!(
                mapinfo,
                "episode {name} {{ name = \"{}\" }}",
                title_case(title).replace('"', "'")
            );
        }
    }
    mapinfo.push('\n');
    let mut music = BTreeSet::new();
    let mut skies = BTreeSet::new();
    for (i, (name, c, info)) in converted.iter().enumerate() {
        let title = info
            .as_ref()
            .map_or_else(|| name.clone(), |l| title_case(&l.name));
        let _ = writeln!(mapinfo, "map {name} \"{}\"", title.replace('"', "'"));
        let next = converted.get(i + 1).filter(|(n, ..)| same_episode(name, n));
        if let Some((next, ..)) = next {
            let _ = writeln!(mapinfo, "next {next}");
        }
        if let Some(sky) = c.sky.filter(|&s| tiles.get(s).is_some()) {
            let _ = writeln!(mapinfo, "sky1 {} 0", sky_name(sky));
            skies.insert(sky);
        }
        if let Some(lump) = info
            .as_ref()
            .and_then(music_lump)
            .filter(|(_, f)| grp.get(f).is_some())
        {
            let _ = writeln!(mapinfo, "music \"{}\"", lump.0);
            music.insert(lump);
        }
        if let Some(par) = info.as_ref().and_then(|l| l.par_seconds) {
            let _ = writeln!(mapinfo, "par {par}");
        }
        mapinfo.push('\n');
    }
    w.add("MAPINFO", mapinfo.into_bytes());
    for (lump, file) in &music {
        if let Some(body) = grp.get(file) {
            w.add(lump, body.to_vec());
        }
    }
    if let Some(file) = parse_menu_music(&user_con) {
        if let Some(lump) = mid_stem(&file) {
            if let Some(body) = grp.get(&file) {
                if music.iter().all(|(l, _)| l != &lump) {
                    w.add(&lump, body.to_vec());
                }
            }
        }
    }

    // Animated wall / plane tiles (`picanm`) cycle through the tiles after them.
    let mut animdefs = String::from("cameratexture VCAMA0 64 64\n");
    for pic in used.clone() {
        let frames = tiles.animation(pic);
        if frames.len() < 2 {
            continue;
        }
        let _ = writeln!(animdefs, "texture {}", tile_name(pic));
        for (f, tics) in frames {
            let _ = writeln!(animdefs, "  pic {} tics {tics}", tile_name(f));
            used.insert(f);
        }
    }
    w.add("ANIMDEFS", animdefs.into_bytes());

    w.marker("TX_START");
    for &pic in &used {
        let Some(t) = tiles.get(pic) else { continue };
        let indices = t.row_major();
        let alpha: Vec<u8> = if indices.contains(&TRANSPARENT) {
            indices
                .iter()
                .map(|&i| if i == TRANSPARENT { 0 } else { 255 })
                .collect()
        } else {
            Vec::new()
        };
        match room_formats::patch::encode_patch(t.width, t.height, 0, 0, &indices, &alpha) {
            Ok(lump) => {
                w.add(&tile_name(pic), lump);
            }
            Err(e) => warnings.push(format!("tile {pic}: {e}")),
        }
    }
    let switches: BTreeSet<i32> = converted
        .iter()
        .flat_map(|(_, c, _)| c.switch_tiles.iter().copied())
        .collect();
    for p in switches {
        for (on, tile) in [(false, p), (true, p + 1)] {
            if let Some(lump) = tiles
                .get(tile)
                .and_then(|t| encode_tile(t, &lookups, 0, None).ok())
            {
                w.add(&switch_name(p, on), lump);
            }
        }
    }
    for sky in skies {
        match compose_sky(&tiles, sky, &sky_offsets(&con, sky)) {
            Ok(lump) => {
                w.add(&sky_name(sky), lump);
            }
            Err(e) => warnings.push(format!("sky {sky}: {e}")),
        }
    }
    w.marker("TX_END");

    let mut maps = Vec::with_capacity(converted.len());
    for (name, c, _) in converted {
        w.marker(&name);
        w.add("TEXTMAP", c.textmap.into_bytes());
        if let Some(behavior) = c.behavior {
            w.add("BEHAVIOR", behavior);
        }
        w.marker("ENDMAP");
        maps.push(name);
    }
    let wad = w.finish().map_err(|e| BuildError::Wad(e.to_string()))?;
    Ok(GrpWad {
        wad,
        maps,
        warnings,
    })
}

fn patch_from_anm(anm: &[u8], palette: &Palette) -> Result<Vec<u8>, BuildError> {
    let frame = title_frame(anm)?;
    encode_anm_frame(
        frame.width,
        frame.height,
        &frame.indices,
        &frame.rgb,
        palette,
    )
}

/// Title, intermission and episode-end stills from the GRP's `.ANM` files.
fn add_anm_pics(w: &mut WadWriter, warnings: &mut Vec<String>, grp: &Grp, palette: &Palette) {
    {
        let mut cache: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
        let mut decode = |file: &'static str| -> Option<Vec<u8>> {
            if let Some(body) = cache.get(file) {
                return Some(body.clone());
            }
            let anm = grp.get(file)?;
            match patch_from_anm(anm, palette) {
                Ok(body) => {
                    cache.insert(file, body.clone());
                    Some(body)
                }
                Err(e) => {
                    warnings.push(format!("{file}: {e}"));
                    None
                }
            }
        };
        let mut add = |lump: &str, files: &[&'static str]| {
            for file in files {
                if let Some(body) = decode(file) {
                    w.add(lump, body);
                    return;
                }
            }
        };
        add("TITLEPIC", &["LOGO.ANM", "RADLOGO.ANM"]);
        add("INTERPIC", &["DUKETEAM.ANM", "LOGO.ANM", "RADLOGO.ANM"]);
        add("CREDIT", &["DUKETEAM.ANM", "LOGO.ANM", "RADLOGO.ANM"]);
        add("WIMAP0", &["CINEOV2.ANM", "DUKETEAM.ANM", "LOGO.ANM"]);
        add("WIMAP1", &["CINEOV3.ANM", "DUKETEAM.ANM", "LOGO.ANM"]);
        add("WIMAP2", &["VOL4E1.ANM", "DUKETEAM.ANM", "LOGO.ANM"]);
    }
    add_anm_sequence(
        w,
        warnings,
        grp,
        palette,
        &["RADLOGO.ANM"],
        "RADL",
        "RADLDEF",
    );
    add_anm_sequence(
        w,
        warnings,
        grp,
        palette,
        &["LOGO.ANM", "RADLOGO.ANM"],
        "TITL",
        "TITLEDEF",
    );
    add_anm_sequence(
        w,
        warnings,
        grp,
        palette,
        &["CINEOV2.ANM"],
        "E1AN",
        "E1ANDEF",
    );
    add_anm_sequence(
        w,
        warnings,
        grp,
        palette,
        &["CINEOV3.ANM"],
        "E2AN",
        "E2ANDEF",
    );
    add_anm_sequence(
        w,
        warnings,
        grp,
        palette,
        &["DUKETEAM.ANM"],
        "E3AN",
        "E3ANDEF",
    );
    add_anm_sequence(
        w,
        warnings,
        grp,
        palette,
        &["VOL4E1.ANM"],
        "E4AN",
        "E4ANDEF",
    );
}

/// `{prefix}00`… plus `{def}` so a screen can play an `.ANM` once.
fn add_anm_sequence(
    w: &mut WadWriter,
    warnings: &mut Vec<String>,
    grp: &Grp,
    palette: &Palette,
    files: &[&str],
    prefix: &str,
    def: &str,
) {
    let Some(anm) = files.iter().find_map(|f| grp.get(f)) else {
        return;
    };
    let anim = match decode_stills(anm) {
        Ok(anim) => anim,
        Err(e) => {
            warnings.push(format!("{def}: {e}"));
            return;
        }
    };
    if anim.frames.is_empty() {
        return;
    }
    let n = anim.frames.len().min(100);
    for (i, indices) in anim.frames.iter().take(n).enumerate() {
        let name = format!("{prefix}{i:02}");
        match encode_anm_frame(anim.width, anim.height, indices, &anim.rgb, palette) {
            Ok(patch) => {
                w.add(&name, patch);
            }
            Err(e) => warnings.push(format!("{name}: {e}")),
        }
    }
    w.add(
        def,
        format!("frames {n}\nfps {}\n", anim.fps.max(1)).into_bytes(),
    );
}

fn encode_anm_frame(
    width: u16,
    height: u16,
    indices: &[u8],
    rgb: &[u8; 768],
    palette: &Palette,
) -> Result<Vec<u8>, BuildError> {
    let mut remap = [0u8; 256];
    for (i, slot) in remap.iter_mut().enumerate() {
        let o = i * 3;
        *slot = palette.nearest([
            u32::from(rgb[o]),
            u32::from(rgb[o + 1]),
            u32::from(rgb[o + 2]),
        ]);
    }
    let mapped: Vec<u8> = indices.iter().map(|&p| remap[usize::from(p)]).collect();
    room_formats::patch::encode_patch(width, height, 0, 0, &mapped, &[])
        .map_err(|e| BuildError::Wad(e.to_string()))
}

/// Encode a tile as a Doom picture. `origin`: `None` = top-left, world
/// sprites stand on their bottom centre, weapon overlays are placed so
/// Room's psprite origin lands the tile where Duke's `rotatesprite` does.
fn encode_tile(
    t: &Tile<'_>,
    lookups: &Lookups,
    pal: u8,
    origin: Option<Origin>,
) -> Result<Vec<u8>, room_formats::FormatError> {
    let indices = lookups.apply(pal, &t.row_major());
    let alpha: Vec<u8> = if indices.contains(&TRANSPARENT) {
        indices
            .iter()
            .map(|&i| if i == TRANSPARENT { 0 } else { 255 })
            .collect()
    } else {
        Vec::new()
    };
    let (w, h) = (i32::from(t.width), i32::from(t.height));
    let (xo, yo) = (i32::from(t.x_offset()), i32::from(t.y_offset()));
    let (left, top) = match origin {
        None => (0, 0),
        Some(Origin::World) => (w / 2 + xo, h + yo),
        Some(Origin::Hud(x, y)) => (w / 2 + xo - x, 32 - (y - (h / 2 + yo))),
    };
    let clamp = |v: i32| i16::try_from(v.clamp(-32768, 32767)).unwrap_or(0);
    room_formats::patch::encode_patch(t.width, t.height, clamp(left), clamp(top), &indices, &alpha)
}

/// Sky texture name for parallax tile `pic`.
fn sky_name(pic: i32) -> String {
    format!("BSKY{pic:04}")
}

/// Duke's per-sky tile offsets (`pskyoff`, eight slots around 360°).
fn sky_offsets(con: &ConData, pic: i32) -> [i32; 8] {
    let is = |name: &str| con.value(name) == Some(pic);
    if is("MOONSKY1") {
        [0, 2, 3, 0, 2, 0, 1, 0]
    } else if is("BIGORBIT1") {
        [0, 0, 4, 0, 0, 1, 2, 3]
    } else if is("LA") {
        [1, 2, 1, 3, 4, 0, 2, 3]
    } else {
        [0; 8]
    }
}

/// One texture holding the eight sky slots side by side; Room wraps a sky
/// texture once around the horizon.
fn compose_sky(
    tiles: &TileSet<'_>,
    pic: i32,
    offsets: &[i32; 8],
) -> Result<Vec<u8>, room_formats::FormatError> {
    let base = tiles
        .get(pic)
        .ok_or(room_formats::FormatError::Invalid("sky tile missing"))?;
    let (w, h) = (usize::from(base.width), usize::from(base.height));
    let width =
        u16::try_from(w * 8).map_err(|_| room_formats::FormatError::Invalid("sky width"))?;
    let mut indices = vec![0u8; w * 8 * h];
    for (slot, off) in offsets.iter().enumerate() {
        let t = tiles
            .get(pic + off)
            .filter(|t| t.width == base.width && t.height == base.height)
            .unwrap_or(base);
        let rows = t.row_major();
        for y in 0..h {
            let dst = y * w * 8 + slot * w;
            indices[dst..dst + w].copy_from_slice(&rows[y * w..(y + 1) * w]);
        }
    }
    room_formats::patch::encode_patch(width, base.height, 0, 0, &indices, &[])
}

fn same_episode(a: &str, b: &str) -> bool {
    a.len() > 2 && b.len() > 2 && a.starts_with('E') && a[..2] == b[..2]
}

fn music_lump(l: &LevelInfo) -> Option<(String, String)> {
    let file = l.music.clone()?;
    mid_stem(&file).map(|stem| (stem, file))
}

fn mid_stem(file: &str) -> Option<String> {
    let stem = file.strip_suffix(".MID")?;
    (stem.len() <= 8 && !stem.is_empty()).then(|| stem.to_owned())
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            // Initialisms (`L.A.`) stay upper case.
            if w.trim_end_matches('.').contains('.') {
                return w.to_owned();
            }
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_ascii_uppercase().to_string() + &c.as_str().to_ascii_lowercase()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::art::build_art;
    use crate::map::square_map;
    use crate::palette::build_palette;

    fn grp(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut out = b"KenSilverman".to_vec();
        out.extend_from_slice(&(files.len() as u32).to_le_bytes());
        for (name, body) in files {
            let mut n = [0u8; 12];
            n[..name.len()].copy_from_slice(name.as_bytes());
            out.extend_from_slice(&n);
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        }
        for (_, body) in files {
            out.extend_from_slice(body);
        }
        out
    }

    #[test]
    fn grp_becomes_a_mountable_wad_with_maps_and_tiles() {
        let mut solid = vec![7u8; 64 * 64];
        solid[5] = TRANSPARENT;
        let art = build_art(
            0,
            &[
                (64, 64, &solid),
                (64, 64, &[1; 4096]),
                (64, 64, &[2; 4096]),
                (64, 64, &[3; 4096]),
            ],
        );
        let con =
            b"definelevelname 0 0 E1L1.map 01:00 00:30 THE ROOF\nmusic 0 grabbag.mid\nmusic 1 stalker.mid\n".to_vec();
        let bytes = grp(&[
            ("PALETTE.DAT", build_palette()),
            ("TILES000.ART", art),
            ("USER.CON", con),
            ("E1L1.MAP", square_map().to_bytes()),
            ("STALKER.MID", b"MThd".to_vec()),
            ("GRABBAG.MID", b"MThd".to_vec()),
            ("BROKEN.MAP", vec![1, 2, 3]),
            ("LOGO.ANM", crate::anm::tiny_anm()),
        ]);
        let out = convert_grp(&bytes).unwrap();
        assert_eq!(out.maps, ["E1M1"]);
        assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
        let vfs = room_vfs::VfsBuilder::new()
            .mount_memory("duke.wad", out.wad)
            .unwrap()
            .build()
            .unwrap();
        for lump in [
            "PLAYPAL", "COLORMAP", "MAPINFO", "E1M1", "TEXTMAP", "STALKER", "TITLEPIC", "INTERPIC",
            "CREDIT", "WIMAP0", "MENUDEF", "GRABBAG",
        ] {
            assert!(vfs.find_any_ns(lump).is_some(), "{lump}");
        }
        let tex = vfs
            .find("BT0001", room_vfs::Namespace::NewTextures)
            .expect("tile in TX namespace");
        assert!(room_formats::decode_image(&vfs.read(tex).unwrap()).is_ok());
        let info = String::from_utf8(
            vfs.read(vfs.find_any_ns("MAPINFO").unwrap())
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(info.contains("map E1M1 \"The Roof\""));
        assert!(info.contains("music \"STALKER\""));
        assert!(info.contains("par 60"));
    }

    #[test]
    fn user_maps_convert_with_the_grp_content() {
        let art = build_art(0, &[(64, 64, &[1; 4096]), (64, 64, &[2; 4096])]);
        let bytes = grp(&[
            ("PALETTE.DAT", build_palette()),
            ("TILES000.ART", art),
            ("E1L1.MAP", square_map().to_bytes()),
        ]);
        let user = vec![
            ("maps/mymap.map".to_owned(), square_map().to_bytes()),
            ("E1L1.MAP".to_owned(), square_map().to_bytes()),
        ];
        let out = convert_grp_with_maps(&bytes, &user).unwrap();
        assert_eq!(out.maps, ["E1M1", "MYMAP"]);
        assert!(crate::is_build_map(&square_map().to_bytes()));
        assert!(!crate::is_build_map(b"PWAD\0\0\0\0"));
    }

    #[test]
    fn not_a_grp_or_missing_palette_is_an_error() {
        assert!(grp_to_wad(b"PWAD").is_err());
        assert!(matches!(
            grp_to_wad(&grp(&[("E1L1.MAP", square_map().to_bytes())])),
            Err(BuildError::Missing(_))
        ));
    }

    #[test]
    fn other_build_grps_are_rejected() {
        let art = build_art(0, &[(8, 8, &[1; 64])]);
        let pal = build_palette();
        let map = square_map().to_bytes();
        for (marker, game) in [
            ("BLOOD.INI", "Blood"),
            ("SW.CON", "Shadow Warrior"),
            ("REDNECK.CON", "Redneck Rampage"),
            ("NAM.CON", "NAM"),
        ] {
            let bytes = grp(&[
                ("PALETTE.DAT", pal.clone()),
                ("TILES000.ART", art.clone()),
                (marker, b"x".to_vec()),
                ("E1L1.MAP", map.clone()),
            ]);
            assert_eq!(
                convert_grp(&bytes).err(),
                Some(BuildError::Unsupported(game)),
                "{marker}"
            );
        }
    }

    #[test]
    fn corrupted_inputs_never_panic() {
        let map = square_map().to_bytes();
        let art = build_art(0, &[(64, 64, &[1; 4096]), (8, 8, &[2; 64])]);
        let pal = build_palette();
        let con = b"definelevelname 0 0 E1L1.map 01:00 00:30 ROOF\nmusic 1 a.mid b.mid\n".to_vec();
        let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for round in 0..400 {
            let mut files = [map.clone(), art.clone(), pal.clone(), con.clone()];
            let f = &mut files[round % 4];
            for _ in 0..1 + next() % 8 {
                let i = (next() as usize) % f.len();
                f[i] = next() as u8;
            }
            if next() % 3 == 0 {
                let keep = (next() as usize) % f.len();
                f.truncate(keep);
            }
            let [m, a, p, c] = files;
            let _ = parse_map(&m).map(|m| crate::convert_map(&m, &TileSet::default()));
            let mut tiles = TileSet::default();
            let _ = tiles.add_art(&a);
            let _ = Palette::parse(&p);
            let _ = crate::parse_user_con(&String::from_utf8_lossy(&c));
            let mut whole = grp(&[
                ("PALETTE.DAT", p),
                ("TILES000.ART", a),
                ("USER.CON", c),
                ("E1L1.MAP", m),
            ]);
            let i = (next() as usize) % whole.len();
            whole[i] = next() as u8;
            let _ = convert_grp(&whole);
        }
    }
}
