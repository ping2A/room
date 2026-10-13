//! MAPINFO / ZMAPINFO — new brace syntax and old Hexen line syntax.

use crate::error::DefsError;
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// One map definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapDef {
    /// Lump / map name (`MAP01`).
    pub name: String,
    /// Display title.
    pub title: Option<String>,
    /// Next map.
    pub next: Option<String>,
    /// Secret next.
    pub secret_next: Option<String>,
    /// Sky texture 1.
    pub sky1: Option<String>,
    /// GLDEFS skybox name (`skybox` property).
    pub skybox: Option<String>,
    /// Music lump.
    pub music: Option<String>,
    /// Par time in seconds (`par` / MAPINFO).
    pub par_seconds: Option<u32>,
    /// Cluster id (`cluster` / `clusterdef`).
    pub cluster: Option<i32>,
    /// Hexen-style falling damage (`fallingdamage` / `forcefallingdamage` / …).
    /// `None` = unset (engine default off for Doom).
    pub falling_damage: Option<bool>,
    /// MAPINFO `clipmidtextures` — default clip midtextures to openings.
    pub clip_midtextures: Option<bool>,
    /// MAPINFO `wrapmidtextures` — default wrap midtextures vertically.
    pub wrap_midtextures: Option<bool>,
    /// Extra key/value pairs not yet specialised.
    pub extras: BTreeMap<String, String>,
}

/// Episode definition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EpisodeDef {
    /// Map to start.
    pub start_map: String,
    /// Episode name.
    pub name: Option<String>,
}

/// Cluster / hub definition (`cluster` / `clusterdef`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClusterDef {
    /// Cluster number.
    pub id: i32,
    /// Hub: map state preserved when travelling within the cluster.
    pub hub: bool,
    /// Enter text (intermission / hub).
    pub enter_text: Option<String>,
    /// Exit text.
    pub exit_text: Option<String>,
    /// Cluster music.
    pub music: Option<String>,
    /// Background flat.
    pub flat: Option<String>,
    /// Extra properties.
    pub extras: BTreeMap<String, String>,
}

/// Compiled MAPINFO.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapInfo {
    /// Maps by lump name (uppercase).
    pub maps: BTreeMap<String, MapDef>,
    /// Episodes in order.
    pub episodes: Vec<EpisodeDef>,
    /// Clusters by id.
    pub clusters: BTreeMap<i32, ClusterDef>,
}

impl MapInfo {
    /// Cluster id for a map lump name, if set.
    #[must_use]
    pub fn cluster_of(&self, map: &str) -> Option<i32> {
        self.maps
            .get(&map.to_ascii_uppercase())
            .and_then(|m| m.cluster)
    }

    /// True if `id` is a hub cluster.
    #[must_use]
    pub fn is_hub_cluster(&self, id: i32) -> bool {
        self.clusters.get(&id).is_some_and(|c| c.hub)
    }

    /// True when both maps share a hub cluster.
    #[must_use]
    pub fn same_hub(&self, a: &str, b: &str) -> bool {
        match (self.cluster_of(a), self.cluster_of(b)) {
            (Some(ca), Some(cb)) if ca == cb => self.is_hub_cluster(ca),
            _ => false,
        }
    }
}

/// Keywords that end an old-format map body (start another top-level block).
fn is_old_toplevel(kw: &str) -> bool {
    matches!(
        kw.to_ascii_lowercase().as_str(),
        "map"
            | "episode"
            | "cluster"
            | "clusterdef"
            | "defaultmap"
            | "adddefaultmap"
            | "gamedefaults"
            | "gameinfo"
            | "clearepisodes"
            | "skill"
            | "clearskills"
            | "doomEdNums"
            | "doomednums"
            | "spawnnums"
            | "damagetype"
            | "include"
    )
}

/// Parse MAPINFO / ZMAPINFO. Detects new (`{`) vs old Hexen line format per map.
pub fn parse_mapinfo(text: &str) -> Result<MapInfo, DefsError> {
    // First pass uses C-mode so `{` / `=` / `;` tokenize for the new format.
    // Old-format property lines still work because non-splitter idents are whole words.
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            semicolon_comments: true,
        },
    );
    let mut info = MapInfo::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Ident(kw) if Scanner::compare(kw, "map") => {
                let map_name = read_name(&mut sc)?;
                let mut def = MapDef {
                    name: normalize_map_name(&map_name),
                    ..Default::default()
                };
                // Optional quoted title.
                match sc.next_token()? {
                    Tok::String(t) => {
                        def.title = Some(t.into_owned());
                    }
                    other => {
                        sc.unget(other);
                    }
                }
                if sc.check_punct(Punct::LBrace)? {
                    parse_map_block_new(&mut sc, &mut def)?;
                } else {
                    parse_map_body_old(&mut sc, &mut def)?;
                }
                info.maps.insert(def.name.clone(), def);
            }
            Tok::Ident(kw) if Scanner::compare(kw, "episode") => {
                let start = read_name(&mut sc)?;
                let mut ep = EpisodeDef {
                    start_map: normalize_map_name(&start),
                    name: None,
                };
                if sc.check_punct(Punct::LBrace)? {
                    loop {
                        match sc.next_token()? {
                            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
                            Tok::Ident(k) => {
                                let _ = sc.check_punct(Punct::Eq)?;
                                let v = read_name(&mut sc)?;
                                if Scanner::compare(k, "name") {
                                    ep.name = Some(v);
                                }
                                let _ = sc.check_punct(Punct::Semi)?;
                            }
                            _ => {}
                        }
                    }
                } else {
                    // Old: episode e1m1
                    //      name "…"
                    loop {
                        match sc.next_token()? {
                            Tok::Eof => break,
                            Tok::Ident(k) if is_old_toplevel(k) => {
                                sc.unget(Tok::Ident(k));
                                break;
                            }
                            Tok::Ident(k) if Scanner::compare(k, "name") => {
                                ep.name = Some(read_name(&mut sc)?);
                            }
                            Tok::Ident(_) => {
                                let _ = sc.next_token()?;
                            }
                            _ => {}
                        }
                    }
                }
                info.episodes.push(ep);
            }
            Tok::Ident(kw)
                if Scanner::compare(kw, "cluster") || Scanner::compare(kw, "clusterdef") =>
            {
                let id = read_cluster_id(&mut sc)?;
                let mut def = ClusterDef {
                    id,
                    ..Default::default()
                };
                if sc.check_punct(Punct::LBrace)? {
                    parse_cluster_block(&mut sc, &mut def)?;
                } else {
                    // Old: skip until next toplevel.
                    loop {
                        match sc.next_token()? {
                            Tok::Eof => break,
                            Tok::Ident(k) if is_old_toplevel(k) => {
                                sc.unget(Tok::Ident(k));
                                break;
                            }
                            Tok::Ident(k) if Scanner::compare(k, "hub") => {
                                def.hub = true;
                            }
                            Tok::Ident(k) => {
                                let key = k.to_ascii_lowercase();
                                if let Ok(v) = read_name(&mut sc) {
                                    apply_cluster_prop(&mut def, &key, &v);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                info.clusters.insert(id, def);
            }
            Tok::Ident(kw)
                if Scanner::compare(kw, "defaultmap") || Scanner::compare(kw, "adddefaultmap") =>
            {
                if sc.check_punct(Punct::LBrace)? {
                    skip_block(&mut sc)?;
                } else {
                    // Old defaultmap: skip properties until next toplevel.
                    loop {
                        match sc.next_token()? {
                            Tok::Eof => break,
                            Tok::Ident(k) if is_old_toplevel(k) => {
                                sc.unget(Tok::Ident(k));
                                break;
                            }
                            Tok::Ident(_) => {
                                let _ = sc.next_token()?;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Tok::Ident(_) => {
                if sc.check_punct(Punct::LBrace)? {
                    skip_block(&mut sc)?;
                } else {
                    loop {
                        match sc.next_token()? {
                            Tok::Punct(Punct::Semi) | Tok::Eof => break,
                            Tok::Punct(Punct::LBrace) => {
                                skip_block(&mut sc)?;
                                break;
                            }
                            Tok::Ident(k) if is_old_toplevel(k) => {
                                sc.unget(Tok::Ident(k));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!("MAPINFO: unexpected {other:?}")));
            }
        }
    }
    Ok(info)
}

fn normalize_map_name(name: &str) -> String {
    // Hexen uses bare numbers (`map 1`); Doom uses `MAP01` / `E1M1`.
    if let Ok(n) = name.parse::<u32>() {
        if (1..=99).contains(&n) {
            return format!("MAP{n:02}");
        }
    }
    name.to_ascii_uppercase()
}

fn is_map_flag_keyword(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "fallingdamage"
            | "oldfallingdamage"
            | "strifefallingdamage"
            | "forcefallingdamage"
            | "monsterfallingdamage"
            | "nofallingdamage"
            | "clipmidtextures"
            | "wrapmidtextures"
    )
}

fn parse_map_block_new(sc: &mut Scanner<'_>, def: &mut MapDef) -> Result<(), DefsError> {
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => return Ok(()),
            Tok::Ident(key) => {
                if is_map_flag_keyword(key) {
                    // Flag-only; optional `= true` / `= false`.
                    let val = if sc.check_punct(Punct::Eq)? {
                        read_name(sc)?
                    } else {
                        String::new()
                    };
                    apply_map_prop(def, key, &val);
                    let _ = sc.check_punct(Punct::Semi)?;
                    continue;
                }
                let _ = sc.check_punct(Punct::Eq)?;
                let val = read_name(sc)?;
                apply_map_prop(def, key, &val);
                let _ = sc.check_punct(Punct::Semi)?;
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!(
                    "MAPINFO map block: unexpected {other:?}"
                )));
            }
        }
    }
}

fn parse_map_body_old(sc: &mut Scanner<'_>, def: &mut MapDef) -> Result<(), DefsError> {
    loop {
        match sc.next_token()? {
            Tok::Eof => return Ok(()),
            Tok::Ident(key) if is_old_toplevel(key) => {
                sc.unget(Tok::Ident(key));
                return Ok(());
            }
            Tok::Ident(key) => {
                let key_l = key.to_ascii_lowercase();
                match key_l.as_str() {
                    "next" => {
                        def.next = Some(normalize_map_name(&read_name(sc)?));
                    }
                    "secretnext" => {
                        def.secret_next = Some(normalize_map_name(&read_name(sc)?));
                    }
                    "skybox" => {
                        def.skybox = Some(read_name(sc)?);
                    }
                    "sky1" | "sky" => {
                        def.sky1 = Some(read_name(sc)?);
                        // Optional scroll speed.
                        consume_optional_number(sc)?;
                    }
                    "music" => {
                        def.music = Some(read_name(sc)?);
                    }
                    "par" => match sc.next_token()? {
                        Tok::Int(n) if n >= 0 => {
                            def.par_seconds = Some(n as u32);
                        }
                        other => {
                            sc.unget(other);
                        }
                    },
                    "levelname" | "title" | "lookup" => {
                        def.title = Some(read_name(sc)?);
                    }
                    "cluster" => match sc.next_token()? {
                        Tok::Int(n) => {
                            def.cluster = Some(n as i32);
                        }
                        other => {
                            sc.unget(other);
                            if let Ok(v) = read_name(sc) {
                                if let Ok(n) = v.parse::<i32>() {
                                    def.cluster = Some(n);
                                } else {
                                    def.extras.insert("cluster".into(), v);
                                }
                            }
                        }
                    },
                    // Flag-only keywords.
                    "lightning"
                    | "evenlighting"
                    | "nointermission"
                    | "nosoundclipping"
                    | "baronspecial"
                    | "cyberdemonspecial"
                    | "spidermastermindspecial"
                    | "doubleSky"
                    | "doublesky"
                    | "forcenoskystretch"
                    | "allowmonstertelefrags"
                    | "noautosequences"
                    | "strictmonsteractivation"
                    | "laxmonsteractivation"
                    | "missileshootersactivateimpactlines"
                    | "impactactivateobjective"
                    | "allowfreelook"
                    | "nofreelook"
                    | "nojump"
                    | "jump"
                    | "nocrouch"
                    | "crouch"
                    | "noinfighting"
                    | "normalinfighting"
                    | "totalinfighting"
                    | "filterstarts"
                    | "allowrespawn"
                    | "teamplayon"
                    | "teamplayoff"
                    | "checkswitchrange"
                    | "nocheckswitchrange"
                    | "resethealth"
                    | "resetinventory"
                    | "resetitems" => {
                        def.extras.insert(key_l, "true".into());
                    }
                    "clipmidtextures" => {
                        def.clip_midtextures = Some(true);
                        def.extras.insert(key_l, "true".into());
                    }
                    "wrapmidtextures" => {
                        def.wrap_midtextures = Some(true);
                        def.extras.insert(key_l, "true".into());
                    }
                    "fallingdamage"
                    | "oldfallingdamage"
                    | "strifefallingdamage"
                    | "forcefallingdamage"
                    | "monsterfallingdamage" => {
                        def.falling_damage = Some(true);
                        def.extras.insert(key_l, "true".into());
                    }
                    "nofallingdamage" => {
                        def.falling_damage = Some(false);
                        def.extras.insert(key_l, "true".into());
                    }
                    _ => {
                        // Consume one value token if present.
                        match sc.next_token()? {
                            Tok::Eof => {}
                            Tok::Ident(k) if is_old_toplevel(k) => {
                                sc.unget(Tok::Ident(k));
                            }
                            Tok::Ident(v) => {
                                def.extras.insert(key_l, v.to_owned());
                            }
                            Tok::String(v) => {
                                def.extras.insert(key_l, v.into_owned());
                            }
                            Tok::Int(n) => {
                                def.extras.insert(key_l, n.to_string());
                            }
                            Tok::Float(f) => {
                                def.extras.insert(key_l, f.to_string());
                            }
                            other => {
                                sc.unget(other);
                            }
                        }
                    }
                }
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!(
                    "MAPINFO old map body: unexpected {other:?}"
                )));
            }
        }
    }
}

fn consume_optional_number(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    let tok = sc.next_token()?;
    match tok {
        Tok::Int(_) | Tok::Float(_) => Ok(()),
        other => {
            sc.unget(other);
            Ok(())
        }
    }
}

fn apply_map_prop(def: &mut MapDef, key: &str, val: &str) {
    let key_l = key.to_ascii_lowercase();
    match key_l.as_str() {
        "next" => def.next = Some(normalize_map_name(val)),
        "secretnext" => def.secret_next = Some(normalize_map_name(val)),
        "sky1" | "sky" => def.sky1 = Some(val.to_owned()),
        "skybox" => def.skybox = Some(val.to_owned()),
        "music" => def.music = Some(val.to_owned()),
        "title" | "levelname" => def.title = Some(val.to_owned()),
        "cluster" => {
            if let Ok(n) = val.parse::<i32>() {
                def.cluster = Some(n);
            } else {
                def.extras.insert(key_l, val.to_owned());
            }
        }
        "par" => {
            if let Ok(n) = val.parse::<u32>() {
                def.par_seconds = Some(n);
            } else {
                def.extras.insert(key_l, val.to_owned());
            }
        }
        "fallingdamage"
        | "oldfallingdamage"
        | "strifefallingdamage"
        | "forcefallingdamage"
        | "monsterfallingdamage" => {
            def.falling_damage = Some(parse_mapinfo_bool(val, true));
            def.extras.insert(key_l, val.to_owned());
        }
        "nofallingdamage" => {
            // Flag present → disable, unless explicitly `false`.
            def.falling_damage = Some(!parse_mapinfo_bool(val, true));
            def.extras.insert(key_l, val.to_owned());
        }
        "clipmidtextures" => {
            def.clip_midtextures = Some(parse_mapinfo_bool(val, true));
            def.extras.insert(key_l, val.to_owned());
        }
        "wrapmidtextures" => {
            def.wrap_midtextures = Some(parse_mapinfo_bool(val, true));
            def.extras.insert(key_l, val.to_owned());
        }
        _ => {
            def.extras.insert(key_l, val.to_owned());
        }
    }
}

fn parse_mapinfo_bool(val: &str, default: bool) -> bool {
    match val.trim().to_ascii_lowercase().as_str() {
        "" | "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => default,
    }
}

fn read_cluster_id(sc: &mut Scanner<'_>) -> Result<i32, DefsError> {
    match sc.next_token()? {
        Tok::Int(n) => Ok(n as i32),
        Tok::Ident(s) => s
            .parse::<i32>()
            .map_err(|_| DefsError::Message(format!("MAPINFO cluster: bad id {s}"))),
        Tok::String(s) => s
            .parse::<i32>()
            .map_err(|_| DefsError::Message(format!("MAPINFO cluster: bad id {s}"))),
        other => Err(DefsError::Message(format!(
            "MAPINFO cluster: expected id, got {other:?}"
        ))),
    }
}

fn parse_cluster_block(sc: &mut Scanner<'_>, def: &mut ClusterDef) -> Result<(), DefsError> {
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => return Ok(()),
            Tok::Ident(key) => {
                let key_l = key.to_ascii_lowercase();
                if key_l == "hub" {
                    // Flag-only; optional `= true`.
                    if sc.check_punct(Punct::Eq)? {
                        let v = read_name(sc)?;
                        def.hub = !matches!(v.to_ascii_lowercase().as_str(), "0" | "false" | "no");
                    } else {
                        def.hub = true;
                    }
                    let _ = sc.check_punct(Punct::Semi)?;
                    continue;
                }
                let _ = sc.check_punct(Punct::Eq)?;
                let val = read_name(sc)?;
                apply_cluster_prop(def, &key_l, &val);
                let _ = sc.check_punct(Punct::Semi)?;
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!(
                    "MAPINFO cluster block: unexpected {other:?}"
                )));
            }
        }
    }
}

fn apply_cluster_prop(def: &mut ClusterDef, key: &str, val: &str) {
    match key {
        "entertext" => def.enter_text = Some(val.to_owned()),
        "exittext" => def.exit_text = Some(val.to_owned()),
        "music" => def.music = Some(val.to_owned()),
        "flat" | "pic" => def.flat = Some(val.to_owned()),
        "hub" => {
            def.hub = !matches!(val.to_ascii_lowercase().as_str(), "0" | "false" | "no");
        }
        _ => {
            def.extras.insert(key.to_owned(), val.to_owned());
        }
    }
}

fn skip_block(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    let mut depth = 1i32;
    while depth > 0 {
        match sc.next_token()? {
            Tok::Punct(Punct::LBrace) => depth += 1,
            Tok::Punct(Punct::RBrace) => depth -= 1,
            Tok::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

fn read_name(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Int(n) => Ok(n.to_string()),
        other => Err(DefsError::Message(format!(
            "MAPINFO: expected name, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parse_new_mapinfo() {
        let text = r#"
map MAP01 "Entryway"
{
    next = MAP02
    sky1 = SKY1
    music = D_RUNNIN
    par = 30
}
episode MAP01
{
    name = "Knee-Deep"
}
"#;
        let info = parse_mapinfo(text).unwrap();
        let m = info.maps.get("MAP01").unwrap();
        assert_eq!(m.title.as_deref(), Some("Entryway"));
        assert_eq!(m.next.as_deref(), Some("MAP02"));
        assert_eq!(m.sky1.as_deref(), Some("SKY1"));
        assert_eq!(m.par_seconds, Some(30));
        assert_eq!(info.episodes.len(), 1);
    }

    #[test]
    fn parse_hub_cluster() {
        let text = r#"
map MAP01 "Hub A"
{
    next = MAP02
    cluster = 1
}
map MAP02 "Hub B"
{
    next = MAP01
    cluster = 1
}
cluster 1
{
    hub
    entertext = "Welcome"
    exittext = "Goodbye"
    music = D_VICTOR
    flat = FLOOR4_8
}
"#;
        let info = parse_mapinfo(text).unwrap();
        assert_eq!(info.maps.get("MAP01").unwrap().cluster, Some(1));
        assert_eq!(info.maps.get("MAP02").unwrap().cluster, Some(1));
        let c = info.clusters.get(&1).unwrap();
        assert!(c.hub);
        assert_eq!(c.enter_text.as_deref(), Some("Welcome"));
        assert_eq!(c.exit_text.as_deref(), Some("Goodbye"));
        assert_eq!(c.music.as_deref(), Some("D_VICTOR"));
        assert_eq!(c.flat.as_deref(), Some("FLOOR4_8"));
        assert!(info.same_hub("MAP01", "MAP02"));
        assert!(!info.same_hub("MAP01", "MAP99"));
    }

    #[test]
    fn parse_old_hexen_mapinfo() {
        let text = r#"
map 1 "Winnowing Hall"
next 2
secretnext 9
sky1 SKY2 0
music chess
lightning
map 2 "Seven Portals"
next 3
"#;
        let info = parse_mapinfo(text).unwrap();
        let m1 = info.maps.get("MAP01").unwrap();
        assert_eq!(m1.title.as_deref(), Some("Winnowing Hall"));
        assert_eq!(m1.next.as_deref(), Some("MAP02"));
        assert_eq!(m1.secret_next.as_deref(), Some("MAP09"));
        assert_eq!(m1.sky1.as_deref(), Some("SKY2"));
        assert_eq!(m1.music.as_deref(), Some("chess"));
        assert_eq!(m1.extras.get("lightning").map(String::as_str), Some("true"));
        assert!(info.maps.contains_key("MAP02"));
    }

    #[test]
    fn parse_falling_damage_flags() {
        let on = parse_mapinfo(
            r#"
map MAP01 "Fall"
{
    fallingdamage
    next = MAP02
}
"#,
        )
        .unwrap();
        assert_eq!(on.maps.get("MAP01").unwrap().falling_damage, Some(true));

        let off = parse_mapinfo(
            r#"
map MAP01 "Safe"
{
    nofallingdamage
}
"#,
        )
        .unwrap();
        assert_eq!(off.maps.get("MAP01").unwrap().falling_damage, Some(false));

        let old = parse_mapinfo(
            r#"
map 1 "Hexen"
fallingdamage
next 2
"#,
        )
        .unwrap();
        assert_eq!(old.maps.get("MAP01").unwrap().falling_damage, Some(true));
    }

    #[test]
    fn parse_clip_wrap_midtextures_flags() {
        let info = parse_mapinfo(
            r#"
map MAP01 "Clip"
{
    clipmidtextures
    wrapmidtextures
}
"#,
        )
        .unwrap();
        let m = info.maps.get("MAP01").unwrap();
        assert_eq!(m.clip_midtextures, Some(true));
        assert_eq!(m.wrap_midtextures, Some(true));
    }
}
