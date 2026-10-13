//! SBARINFO parser (M7 bootstrap — structure + common draw commands).

use crate::error::DefsError;
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// Compiled SBARINFO (last lump wins per GZDoom).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SbarInfo {
    /// `base Doom|Heretic|…` — inherit built-in bar.
    pub base: Option<String>,
    /// Status bar height in pixels.
    pub height: Option<i32>,
    /// Interpolate health display.
    pub interpolate_health: bool,
    /// Interpolate armor display.
    pub interpolate_armor: bool,
    /// `completeborder`.
    pub complete_border: bool,
    /// `monospacefonts`.
    pub monospace_fonts: bool,
    /// `lowerhealthcap`.
    pub lower_health_cap: bool,
    /// `resolution w, h`.
    pub resolution: Option<(i32, i32)>,
    /// Named status bar blocks (`normal`, `fullscreen`, …).
    pub statusbars: BTreeMap<String, StatusBarDef>,
    /// Mugshot definitions by state name.
    pub mugshots: BTreeMap<String, MugshotDef>,
    /// Extra top-level properties.
    pub extras: BTreeMap<String, String>,
}

/// One `statusbar` block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatusBarDef {
    /// Kind / name (`normal`, `fullscreen`, …).
    pub kind: String,
    /// Extra flags on the statusbar header (`fullscreenoffsets`, …).
    pub flags: Vec<String>,
    /// Commands in source order.
    pub commands: Vec<SbarCmd>,
}

/// Mugshot animation definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MugshotDef {
    /// State name (`Normal`, `Pain`, …).
    pub name: String,
    /// Flags (`health`, `health2`, `directional`, …).
    pub flags: Vec<String>,
    /// Raw body lines (frames / durations) for later interpretation.
    pub body: Vec<String>,
}

/// One SBARINFO command (or nested block).
#[derive(Clone, Debug, PartialEq)]
pub enum SbarCmd {
    /// `drawimage "STBAR", x, y [, flags…]`.
    DrawImage {
        /// Image / lump / keyword.
        image: String,
        /// X.
        x: i32,
        /// Y.
        y: i32,
        /// Trailing flags / alignment tokens.
        flags: Vec<String>,
    },
    /// `drawnumber digits, font, color, value, …, x, y`.
    DrawNumber {
        /// Max digits / width token.
        digits: String,
        /// Font name.
        font: String,
        /// Colour / translation token.
        color: String,
        /// Value expression (`health`, `ammo1`, …).
        value: String,
        /// Remaining tokens (flags + coords).
        rest: Vec<String>,
    },
    /// `drawmugshot [flags…,] x, y`.
    DrawMugShot {
        /// Flags and coordinates as tokens.
        args: Vec<String>,
    },
    /// `drawstring …`.
    DrawString {
        /// Raw argument tokens.
        args: Vec<String>,
    },
    /// `drawbar …`.
    DrawBar {
        /// Raw argument tokens.
        args: Vec<String>,
    },
    /// Nested conditional / grouping block (`gamemode`, `usesammo`, `else`, …).
    Block {
        /// Keyword (`gamemode`, `usesammo`, `else`, …).
        kind: String,
        /// Header args before `{` (`deathmatch`, …).
        args: Vec<String>,
        /// Nested commands.
        body: Vec<SbarCmd>,
    },
    /// Unrecognised command kept as name + arg tokens.
    Other {
        /// Command name.
        name: String,
        /// Arguments.
        args: Vec<String>,
    },
}

/// Parse an SBARINFO lump.
pub fn parse_sbarinfo(text: &str) -> Result<SbarInfo, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            // `;` terminates statements in SBARINFO (not Hexen-style comments).
            semicolon_comments: false,
        },
    );
    let mut info = SbarInfo::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(kw) => {
                let key = kw.to_ascii_lowercase();
                match key.as_str() {
                    "base" => {
                        info.base = Some(read_value(&mut sc)?);
                        let _ = sc.check_punct(Punct::Semi)?;
                    }
                    "height" => {
                        info.height = Some(read_i32(&mut sc)?);
                        let _ = sc.check_punct(Punct::Semi)?;
                    }
                    "interpolatehealth" => {
                        info.interpolate_health = read_bool_flag(&mut sc)?;
                    }
                    "interpolatearmor" => {
                        info.interpolate_armor = read_bool_flag(&mut sc)?;
                    }
                    "completeborder" => {
                        info.complete_border = read_bool_flag(&mut sc)?;
                    }
                    "monospacefonts" => {
                        info.monospace_fonts = true;
                        // optional `, "0"` style args — consume until `;`
                        consume_until_semi(&mut sc)?;
                    }
                    "lowerhealthcap" => {
                        info.lower_health_cap = read_bool_flag(&mut sc)?;
                    }
                    "resolution" => {
                        let w = read_i32(&mut sc)?;
                        let _ = sc.check_punct(Punct::Comma)?;
                        let h = read_i32(&mut sc)?;
                        info.resolution = Some((w, h));
                        let _ = sc.check_punct(Punct::Semi)?;
                    }
                    "statusbar" => {
                        let def = parse_statusbar(&mut sc)?;
                        info.statusbars.insert(def.kind.clone(), def);
                    }
                    "mugshot" => {
                        let def = parse_mugshot(&mut sc)?;
                        info.mugshots.insert(def.name.clone(), def);
                    }
                    "include" => {
                        // Includes resolved by VFS layer later; skip path.
                        let _ = read_value(&mut sc)?;
                        let _ = sc.check_punct(Punct::Semi)?;
                    }
                    _ => {
                        // Unknown top-level: store extras or skip block/semi.
                        if sc.check_punct(Punct::LBrace)? {
                            skip_block(&mut sc)?;
                        } else {
                            let mut v = String::new();
                            loop {
                                match sc.next_token()? {
                                    Tok::Punct(Punct::Semi) | Tok::Eof => break,
                                    Tok::Punct(Punct::LBrace) => {
                                        skip_block(&mut sc)?;
                                        break;
                                    }
                                    Tok::Ident(s) => {
                                        if !v.is_empty() {
                                            v.push(' ');
                                        }
                                        v.push_str(s);
                                    }
                                    Tok::String(s) => {
                                        if !v.is_empty() {
                                            v.push(' ');
                                        }
                                        v.push('"');
                                        v.push_str(&s);
                                        v.push('"');
                                    }
                                    Tok::Int(n) => {
                                        if !v.is_empty() {
                                            v.push(' ');
                                        }
                                        v.push_str(&n.to_string());
                                    }
                                    Tok::Float(f) => {
                                        if !v.is_empty() {
                                            v.push(' ');
                                        }
                                        v.push_str(&f.to_string());
                                    }
                                    Tok::Punct(Punct::Comma) => v.push(','),
                                    _ => {}
                                }
                            }
                            info.extras.insert(key, v);
                        }
                    }
                }
            }
            other => {
                return Err(DefsError::Message(format!(
                    "SBARINFO: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(info)
}

fn parse_statusbar(sc: &mut Scanner<'_>) -> Result<StatusBarDef, DefsError> {
    let kind = read_value(sc)?;
    let mut flags = Vec::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::LBrace) => break,
            Tok::Punct(Punct::Comma) => {}
            Tok::Ident(f) => flags.push(f.to_owned()),
            Tok::String(f) => flags.push(f.into_owned()),
            other => {
                return Err(DefsError::Message(format!(
                    "SBARINFO statusbar: unexpected {other:?}"
                )));
            }
        }
    }
    let commands = parse_cmd_block(sc)?;
    Ok(StatusBarDef {
        kind: kind.to_ascii_lowercase(),
        flags,
        commands,
    })
}

fn parse_mugshot(sc: &mut Scanner<'_>) -> Result<MugshotDef, DefsError> {
    let name = read_value(sc)?;
    let mut flags = Vec::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::LBrace) => break,
            Tok::Punct(Punct::Comma) => {}
            Tok::Ident(f) => flags.push(f.to_owned()),
            Tok::String(f) => flags.push(f.into_owned()),
            other => {
                return Err(DefsError::Message(format!(
                    "SBARINFO mugshot: unexpected {other:?}"
                )));
            }
        }
    }
    let mut body = Vec::new();
    let mut line = String::new();
    let mut depth = 1i32;
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::LBrace) => {
                depth += 1;
                line.push('{');
            }
            Tok::Punct(Punct::RBrace) => {
                depth -= 1;
                if depth == 0 {
                    if !line.trim().is_empty() {
                        body.push(line.trim().to_owned());
                    }
                    break;
                }
                line.push('}');
            }
            Tok::Punct(Punct::Semi) => {
                if !line.trim().is_empty() {
                    body.push(line.trim().to_owned());
                }
                line.clear();
            }
            Tok::Ident(s) => {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(s);
            }
            Tok::String(s) => {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push('"');
                line.push_str(&s);
                line.push('"');
            }
            Tok::Int(n) => {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(&n.to_string());
            }
            Tok::Float(f) => {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(&f.to_string());
            }
            Tok::Punct(Punct::Comma) => line.push(','),
            Tok::Eof => break,
            _ => {}
        }
    }
    Ok(MugshotDef { name, flags, body })
}

fn parse_cmd_block(sc: &mut Scanner<'_>) -> Result<Vec<SbarCmd>, DefsError> {
    let mut cmds = Vec::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => return Ok(cmds),
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(name) => {
                let lower = name.to_ascii_lowercase();
                // `else` / block openers that take `{` with optional args.
                if is_block_keyword(&lower) {
                    let mut args = Vec::new();
                    loop {
                        match sc.next_token()? {
                            Tok::Punct(Punct::LBrace) => break,
                            Tok::Punct(Punct::Comma) => {}
                            Tok::Ident(a) => args.push(a.to_owned()),
                            Tok::String(a) => args.push(a.into_owned()),
                            Tok::Int(n) => args.push(n.to_string()),
                            other => {
                                return Err(DefsError::Message(format!(
                                    "SBARINFO block {lower}: unexpected {other:?}"
                                )));
                            }
                        }
                    }
                    let body = parse_cmd_block(sc)?;
                    cmds.push(SbarCmd::Block {
                        kind: lower,
                        args,
                        body,
                    });
                    continue;
                }
                cmds.push(parse_draw_cmd(sc, &lower)?);
            }
            other => {
                return Err(DefsError::Message(format!(
                    "SBARINFO command: unexpected {other:?}"
                )));
            }
        }
    }
}

fn is_block_keyword(name: &str) -> bool {
    matches!(
        name,
        "gamemode"
            | "playerclass"
            | "aspectratio"
            | "isselected"
            | "usesammo"
            | "usessecondaryammo"
            | "hasweaponpiece"
            | "inventorybarnotvisible"
            | "weaponammo"
            | "ininventory"
            | "else"
            | "armortype"
            | "health"
            | "armor"
            | "ifhealth"
            | "ifarmor"
    )
}

fn parse_draw_cmd(sc: &mut Scanner<'_>, name: &str) -> Result<SbarCmd, DefsError> {
    // Some keywords are both values and blocks — if next is `{`, treat as block.
    match sc.next_token()? {
        Tok::Punct(Punct::LBrace) => {
            let body = parse_cmd_block(sc)?;
            return Ok(SbarCmd::Block {
                kind: name.to_owned(),
                args: Vec::new(),
                body,
            });
        }
        other => sc.unget(other),
    }

    let args = read_arg_list(sc)?;
    // `drawselectedinventory … { empty body }` (no `;` before `{`).
    if sc.check_punct(Punct::LBrace)? {
        let body = parse_cmd_block(sc)?;
        return Ok(SbarCmd::Block {
            kind: name.to_owned(),
            args,
            body,
        });
    }
    Ok(match name {
        "drawimage" => {
            let image = args.first().cloned().unwrap_or_default();
            let x = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let y = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            let flags = args.iter().skip(3).cloned().collect();
            SbarCmd::DrawImage { image, x, y, flags }
        }
        "drawnumber" => SbarCmd::DrawNumber {
            digits: args.first().cloned().unwrap_or_default(),
            font: args.get(1).cloned().unwrap_or_default(),
            color: args.get(2).cloned().unwrap_or_default(),
            value: args.get(3).cloned().unwrap_or_default(),
            rest: args.iter().skip(4).cloned().collect(),
        },
        "drawmugshot" => SbarCmd::DrawMugShot { args },
        "drawstring" => SbarCmd::DrawString { args },
        "drawbar" => SbarCmd::DrawBar { args },
        _ => SbarCmd::Other {
            name: name.to_owned(),
            args,
        },
    })
}

fn read_arg_list(sc: &mut Scanner<'_>) -> Result<Vec<String>, DefsError> {
    let mut args = Vec::new();
    let mut cur = String::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::Semi) => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
                return Ok(args);
            }
            Tok::Punct(Punct::Comma) => {
                args.push(std::mem::take(&mut cur));
            }
            Tok::Eof => {
                if !cur.is_empty() {
                    args.push(cur);
                }
                return Ok(args);
            }
            Tok::Ident(s) => {
                if !cur.is_empty() {
                    cur.push(' ');
                }
                cur.push_str(s);
                // Function-call style: ammo(Clip)
                if sc.check_punct(Punct::LParen)? {
                    cur.push('(');
                    loop {
                        match sc.next_token()? {
                            Tok::Punct(Punct::RParen) => {
                                cur.push(')');
                                break;
                            }
                            Tok::Ident(a) => cur.push_str(a),
                            Tok::String(a) => {
                                cur.push('"');
                                cur.push_str(&a);
                                cur.push('"');
                            }
                            Tok::Int(n) => cur.push_str(&n.to_string()),
                            Tok::Punct(Punct::Comma) => cur.push(','),
                            Tok::Eof => break,
                            _ => {}
                        }
                    }
                }
            }
            Tok::String(s) => {
                if !cur.is_empty() {
                    cur.push(' ');
                }
                cur.push_str(&s);
            }
            Tok::Int(n) => {
                if !cur.is_empty()
                    && cur
                        .chars()
                        .last()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                {
                    cur.push(' ');
                }
                cur.push_str(&n.to_string());
            }
            Tok::Float(f) => {
                if !cur.is_empty() {
                    cur.push(' ');
                }
                cur.push_str(&f.to_string());
            }
            Tok::Punct(Punct::Minus) => {
                cur.push('-');
            }
            Tok::Punct(Punct::Plus) => {
                cur.push('+');
            }
            Tok::Punct(Punct::LBrace) => {
                sc.unget(Tok::Punct(Punct::LBrace));
                if !cur.is_empty() {
                    args.push(cur);
                }
                return Ok(args);
            }
            _ => {}
        }
    }
}

fn read_value(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Int(n) => Ok(n.to_string()),
        other => Err(DefsError::Message(format!(
            "SBARINFO: expected value, got {other:?}"
        ))),
    }
}

fn read_i32(sc: &mut Scanner<'_>) -> Result<i32, DefsError> {
    match sc.next_token()? {
        Tok::Int(n) => Ok(n as i32),
        Tok::Punct(Punct::Minus) => match sc.next_token()? {
            Tok::Int(n) => Ok(-(n as i32)),
            other => Err(DefsError::Message(format!(
                "SBARINFO: expected number, got {other:?}"
            ))),
        },
        other => Err(DefsError::Message(format!(
            "SBARINFO: expected number, got {other:?}"
        ))),
    }
}

fn read_bool_flag(sc: &mut Scanner<'_>) -> Result<bool, DefsError> {
    match sc.next_token()? {
        Tok::Punct(Punct::Semi) => Ok(true),
        Tok::Ident(s) => {
            let v = !matches!(s.to_ascii_lowercase().as_str(), "false" | "0" | "no");
            let _ = sc.check_punct(Punct::Semi)?;
            Ok(v)
        }
        Tok::Int(n) => {
            let _ = sc.check_punct(Punct::Semi)?;
            Ok(n != 0)
        }
        other => {
            sc.unget(other);
            let _ = sc.check_punct(Punct::Semi)?;
            Ok(true)
        }
    }
}

fn consume_until_semi(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::Semi) | Tok::Eof => return Ok(()),
            _ => {}
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_doom_like_statusbar() {
        let text = r#"
height 32;
monospacefonts true, "0";
base Doom;

statusbar fullscreen, fullscreenoffsets
{
	drawimage "MEDIA0", 20, -2, centerbottom;
	drawnumber 3, HUDFONT_DOOM, untranslated, health, drawshadow, 82, -20;
	gamemode deathmatch
	{
		drawnumber 3, HUDFONT_DOOM, untranslated, frags, 1, 1;
	}
	else
	{
		drawimage "STKEYS0", 10, 10;
	}
}

statusbar normal
{
	drawimage "STBAR", 0, 168;
	drawnumber 3, HUDFONT_DOOM, untranslated, ammo1, 44, 171;
	drawnumber 3, INDEXFONT_DOOM, untranslated, ammo(Clip), 288, 173;
}

mugshot "Normal", health2
{
	ST00 17;
}
"#;
        let info = parse_sbarinfo(text).unwrap();
        assert_eq!(info.height, Some(32));
        assert!(info.monospace_fonts);
        assert_eq!(info.base.as_deref(), Some("Doom"));
        assert!(info.statusbars.contains_key("fullscreen"));
        assert!(info.statusbars.contains_key("normal"));
        let normal = info.statusbars.get("normal").unwrap();
        assert!(matches!(
            &normal.commands[0],
            SbarCmd::DrawImage { image, x: 0, y: 168, .. } if image == "STBAR"
        ));
        assert!(matches!(
            &normal.commands[2],
            SbarCmd::DrawNumber { value, .. } if value == "ammo(Clip)"
        ));
        let full = info.statusbars.get("fullscreen").unwrap();
        assert!(full.flags.iter().any(|f| f == "fullscreenoffsets"));
        assert!(matches!(
            &full.commands[2],
            SbarCmd::Block { kind, .. } if kind == "gamemode"
        ));
        assert!(info.mugshots.contains_key("Normal"));
    }

    #[test]
    fn parses_gzdoom_doom_statusbar_fixture() {
        // Fixture from GZDoom `wadsrc/static/sbarinfo/doom.txt` (GPL-3.0-or-later).
        let text = include_str!("../../../testdata/sbarinfo/doom.txt");
        let info = parse_sbarinfo(text).unwrap();
        assert_eq!(info.height, Some(32));
        assert!(info.statusbars.contains_key("normal"));
        assert!(info.statusbars.contains_key("fullscreen"));
        assert!(info.statusbars.contains_key("inventory"));
        assert!(info.statusbars.contains_key("inventoryfullscreen"));
    }
}
