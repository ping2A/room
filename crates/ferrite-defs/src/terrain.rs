//! TERRAIN parser (splash / terrain / floor mappings).

use crate::error::DefsError;
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// Which game's `if*` blocks to accept when parsing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerrainGame {
    /// Unconditional + `ifdoom` (default).
    #[default]
    Doom,
    /// Unconditional + `ifheretic`.
    Heretic,
    /// Unconditional + `ifhexen`.
    Hexen,
    /// Unconditional + `ifstrife`.
    Strife,
    /// Accept every conditional block (tests / tooling).
    Any,
}

/// Splash definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SplashDef {
    /// Name.
    pub name: String,
    /// Small splash actor class.
    pub small_class: Option<String>,
    /// Small clip.
    pub small_clip: Option<i32>,
    /// Small sound.
    pub small_sound: Option<String>,
    /// Base splash class.
    pub base_class: Option<String>,
    /// Chunk class.
    pub chunk_class: Option<String>,
    /// Chunk X vel shift.
    pub chunk_x_vel_shift: Option<i32>,
    /// Chunk Y vel shift.
    pub chunk_y_vel_shift: Option<i32>,
    /// Chunk Z vel shift.
    pub chunk_z_vel_shift: Option<i32>,
    /// Chunk base Z vel.
    pub chunk_base_z_vel: Option<i32>,
    /// Splash sound.
    pub sound: Option<String>,
    /// `noalert`.
    pub no_alert: bool,
    /// Extra properties.
    pub extras: BTreeMap<String, String>,
}

/// Terrain definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainDef {
    /// Name.
    pub name: String,
    /// Splash name.
    pub splash: Option<String>,
    /// Foot clip (map units).
    pub foot_clip: Option<i32>,
    /// Liquid flag.
    pub liquid: bool,
    /// Friction multiplier.
    pub friction: Option<f64>,
    /// Damage per tic mask window.
    pub damage_amount: Option<i32>,
    /// Damage type name.
    pub damage_type: Option<String>,
    /// Damage time mask.
    pub damage_time_mask: Option<i32>,
    /// Damage when landing.
    pub damage_on_land: bool,
    /// Allow protection.
    pub allow_protection: bool,
    /// Extra properties.
    pub extras: BTreeMap<String, String>,
}

/// Compiled TERRAIN registry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainInfo {
    /// Splashes by name.
    pub splashes: BTreeMap<String, SplashDef>,
    /// Terrains by name.
    pub terrains: BTreeMap<String, TerrainDef>,
    /// Flat lump → terrain name.
    pub floors: BTreeMap<String, String>,
    /// `defaultterrain` name.
    pub default_terrain: Option<String>,
}

/// Parse TERRAIN for [`TerrainGame::Doom`].
pub fn parse_terrain(text: &str) -> Result<TerrainInfo, DefsError> {
    parse_terrain_for(text, TerrainGame::Doom)
}

/// Parse TERRAIN with a game filter for `if*` / `endif` blocks.
pub fn parse_terrain_for(text: &str, game: TerrainGame) -> Result<TerrainInfo, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            // Non-C: keeps `world/watersplash` as one ident (like SNDINFO).
            c_mode: false,
            semicolon_comments: false,
        },
    );
    let mut info = TerrainInfo::default();
    // Stack of whether the current conditional region is active.
    let mut active = vec![true];
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Ident(kw) => {
                let key = kw.to_ascii_lowercase();
                match key.as_str() {
                    "ifdoom" | "ifheretic" | "ifhexen" | "ifstrife" => {
                        let ok = matches!(
                            (game, key.as_str()),
                            (TerrainGame::Any, _)
                                | (TerrainGame::Doom, "ifdoom")
                                | (TerrainGame::Heretic, "ifheretic")
                                | (TerrainGame::Hexen, "ifhexen")
                                | (TerrainGame::Strife, "ifstrife")
                        );
                        let parent = *active.last().unwrap_or(&true);
                        active.push(parent && ok);
                    }
                    "endif" => {
                        if active.len() > 1 {
                            active.pop();
                        }
                    }
                    _ if !*active.last().unwrap_or(&true) => {
                        // Skip tokens until we can resync on a known keyword —
                        // consume a following `{…}` block if present.
                        if sc.check_punct(Punct::LBrace)? {
                            skip_block(&mut sc)?;
                        } else {
                            // One-line forms like `floor X Y` — eat two values.
                            if matches!(
                                key.as_str(),
                                "floor" | "defaultterrain" | "splash" | "terrain"
                            ) {
                                let _ = sc.next_token();
                                if key == "floor" {
                                    let _ = sc.next_token();
                                } else if matches!(key.as_str(), "splash" | "terrain") {
                                    // name already consumed as kw; maybe `{`
                                    if sc.check_punct(Punct::LBrace)? {
                                        skip_block(&mut sc)?;
                                    }
                                }
                            }
                        }
                    }
                    "splash" => {
                        let def = parse_splash(&mut sc)?;
                        info.splashes.insert(def.name.clone(), def);
                    }
                    "terrain" => {
                        let def = parse_terrain_block(&mut sc)?;
                        info.terrains.insert(def.name.clone(), def);
                    }
                    "floor" => {
                        let flat = read_name(&mut sc)?;
                        let terrain = read_name(&mut sc)?;
                        info.floors.insert(flat.to_ascii_uppercase(), terrain);
                    }
                    "defaultterrain" => {
                        info.default_terrain = Some(read_name(&mut sc)?);
                    }
                    _ => {
                        // Unknown — skip optional block.
                        if sc.check_punct(Punct::LBrace)? {
                            skip_block(&mut sc)?;
                        }
                    }
                }
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!("TERRAIN: unexpected {other:?}")));
            }
        }
    }
    Ok(info)
}

fn parse_splash(sc: &mut Scanner<'_>) -> Result<SplashDef, DefsError> {
    let name = read_name(sc)?;
    let mut def = SplashDef {
        name,
        ..Default::default()
    };
    expect_lbrace(sc)?;
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Ident(k) => {
                let key = k.to_ascii_lowercase();
                match key.as_str() {
                    "noalert" => def.no_alert = true,
                    "smallclass" => def.small_class = Some(read_name(sc)?),
                    "smallclip" => def.small_clip = Some(read_i32(sc)?),
                    "smallsound" => def.small_sound = Some(read_name(sc)?),
                    "baseclass" => def.base_class = Some(read_name(sc)?),
                    "chunkclass" => def.chunk_class = Some(read_name(sc)?),
                    "chunkxvelshift" => def.chunk_x_vel_shift = Some(read_i32(sc)?),
                    "chunkyvelshift" => def.chunk_y_vel_shift = Some(read_i32(sc)?),
                    "chunkzvelshift" => def.chunk_z_vel_shift = Some(read_i32(sc)?),
                    "chunkbasezvel" => def.chunk_base_z_vel = Some(read_i32(sc)?),
                    "sound" => def.sound = Some(read_name(sc)?),
                    _ => {
                        def.extras.insert(key, read_name(sc).unwrap_or_default());
                    }
                }
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!(
                    "TERRAIN splash: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(def)
}

fn parse_terrain_block(sc: &mut Scanner<'_>) -> Result<TerrainDef, DefsError> {
    let name = read_name(sc)?;
    let mut def = TerrainDef {
        name,
        ..Default::default()
    };
    expect_lbrace(sc)?;
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Ident(k) => {
                let key = k.to_ascii_lowercase();
                match key.as_str() {
                    "liquid" => def.liquid = true,
                    "damageonland" => def.damage_on_land = true,
                    "allowprotection" => def.allow_protection = true,
                    "splash" => def.splash = Some(read_name(sc)?),
                    "footclip" => def.foot_clip = Some(read_i32(sc)?),
                    "friction" => def.friction = Some(read_f64(sc)?),
                    "damageamount" => def.damage_amount = Some(read_i32(sc)?),
                    "damagetype" => def.damage_type = Some(read_name(sc)?),
                    "damagetimemask" => def.damage_time_mask = Some(read_i32(sc)?),
                    _ => {
                        def.extras.insert(key, read_name(sc).unwrap_or_default());
                    }
                }
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!(
                    "TERRAIN terrain: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(def)
}

fn expect_lbrace(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    match sc.next_token()? {
        Tok::Punct(Punct::LBrace) => Ok(()),
        other => Err(DefsError::Message(format!(
            "TERRAIN: expected `{{`, got {other:?}"
        ))),
    }
}

fn read_name(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Int(n) => Ok(n.to_string()),
        other => Err(DefsError::Message(format!(
            "TERRAIN: expected name, got {other:?}"
        ))),
    }
}

fn read_i32(sc: &mut Scanner<'_>) -> Result<i32, DefsError> {
    match sc.next_token()? {
        Tok::Int(n) => Ok(n as i32),
        Tok::Punct(Punct::Minus) => match sc.next_token()? {
            Tok::Int(n) => Ok(-(n as i32)),
            other => Err(DefsError::Message(format!(
                "TERRAIN: expected int, got {other:?}"
            ))),
        },
        other => Err(DefsError::Message(format!(
            "TERRAIN: expected int, got {other:?}"
        ))),
    }
}

fn read_f64(sc: &mut Scanner<'_>) -> Result<f64, DefsError> {
    match sc.next_token()? {
        Tok::Float(f) => Ok(f),
        Tok::Int(n) => Ok(n as f64),
        Tok::Punct(Punct::Minus) => match sc.next_token()? {
            Tok::Float(f) => Ok(-f),
            Tok::Int(n) => Ok(-(n as f64)),
            other => Err(DefsError::Message(format!(
                "TERRAIN: expected float, got {other:?}"
            ))),
        },
        other => Err(DefsError::Message(format!(
            "TERRAIN: expected float, got {other:?}"
        ))),
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_splash_terrain_floor() {
        let text = r#"
splash Water
{
	smallclass		WaterSplashBase
	smallclip		12
	sound			world/watersplash
	noalert
}

terrain Water
{
	splash			Water
	footclip		10
	liquid
}

terrain Lava
{
	splash			Lava
	footclip		10
	liquid
	damageamount	5
	damagetype		lava
	damagetimemask	31
	damageonland
}

floor FWATER1 Water
floor LAVA1 Lava
defaultterrain Water
"#;
        let info = parse_terrain(text).unwrap();
        let w = info.splashes.get("Water").unwrap();
        assert_eq!(w.small_class.as_deref(), Some("WaterSplashBase"));
        assert_eq!(w.small_clip, Some(12));
        assert!(w.no_alert);
        let tw = info.terrains.get("Water").unwrap();
        assert!(tw.liquid);
        assert_eq!(tw.foot_clip, Some(10));
        let lava = info.terrains.get("Lava").unwrap();
        assert_eq!(lava.damage_amount, Some(5));
        assert!(lava.damage_on_land);
        assert_eq!(
            info.floors.get("FWATER1").map(String::as_str),
            Some("Water")
        );
        assert_eq!(info.default_terrain.as_deref(), Some("Water"));
    }

    #[test]
    fn if_blocks_filter_by_game() {
        let text = r#"
terrain Water { liquid }
ifheretic
terrain Lava { liquid footclip 10 }
floor FLTWAWA1 Water
endif
ifdoom
floor FWATER1 Water
endif
"#;
        let doom = parse_terrain_for(text, TerrainGame::Doom).unwrap();
        assert!(doom.terrains.contains_key("Water"));
        assert!(!doom.terrains.contains_key("Lava"));
        assert!(doom.floors.contains_key("FWATER1"));
        assert!(!doom.floors.contains_key("FLTWAWA1"));

        let heretic = parse_terrain_for(text, TerrainGame::Heretic).unwrap();
        assert!(heretic.terrains.contains_key("Lava"));
        assert!(heretic.floors.contains_key("FLTWAWA1"));
        assert!(!heretic.floors.contains_key("FWATER1"));
    }

    #[test]
    fn parses_gzdoom_terrain_fixture() {
        // GZDoom `wadsrc/static/terrain.txt` (GPL-3.0-or-later).
        let text = include_str!("../../../testdata/terrain/gzdoom_terrain.txt");
        let doom = parse_terrain(text).unwrap();
        assert!(doom.splashes.contains_key("Water"));
        assert!(doom.terrains.contains_key("Water"));
        let heretic = parse_terrain_for(text, TerrainGame::Heretic).unwrap();
        assert!(heretic.floors.contains_key("FLTWAWA1"));
        let hexen = parse_terrain_for(text, TerrainGame::Hexen).unwrap();
        assert!(hexen.terrains.contains_key("Ice"));
        assert!((hexen.terrains.get("Ice").unwrap().friction.unwrap() - 1.708).abs() < 0.01);
    }
}
