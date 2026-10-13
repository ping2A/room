//! LOCKDEFS parser (door / remote lock key requirements).

use crate::error::DefsError;
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};

/// One key requirement inside a lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockKeyReq {
    /// Exact inventory class name.
    Class(String),
    /// Any of the listed classes.
    Any(Vec<String>),
}

/// One `Lock` definition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockDef {
    /// Lock number (linedef lock / ACS).
    pub number: i32,
    /// Optional game filter (`Doom`, `Heretic`, …).
    pub game: Option<String>,
    /// Required keys (all of; `Any` groups are alternatives).
    pub keys: Vec<LockKeyReq>,
    /// Local message (KEYCONF / LANGUAGE lookup allowed).
    pub message: Option<String>,
    /// Remote activation message.
    pub remote_message: Option<String>,
    /// Automap lock colour RGB 0..=255.
    pub map_color: Option<(u8, u8, u8)>,
    /// Sound when locked.
    pub locked_sound: Option<String>,
}

/// Compiled LOCKDEFS registry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockDefs {
    /// All lock defs in load order (same number may appear per game).
    pub locks: Vec<LockDef>,
}

impl LockDefs {
    /// First lock matching `number` and optional `game` filter (case-insensitive).
    /// Prefer an exact game match; fall back to game-less defs.
    #[must_use]
    pub fn find(&self, number: i32, game: Option<&str>) -> Option<&LockDef> {
        let mut fallback = None;
        for lock in &self.locks {
            if lock.number != number {
                continue;
            }
            match (&lock.game, game) {
                (None, _) => fallback = Some(lock),
                (Some(g), Some(want)) if g.eq_ignore_ascii_case(want) => return Some(lock),
                (Some(_), _) => {}
            }
        }
        fallback
    }
}

/// Parse a LOCKDEFS lump. `ClearLocks` empties the result so far (lump-local).
pub fn parse_lockdefs(text: &str) -> Result<LockDefs, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            semicolon_comments: false,
        },
    );
    let mut info = LockDefs::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(kw) => {
                let key = kw.to_ascii_lowercase();
                match key.as_str() {
                    "clearlocks" => {
                        info.locks.clear();
                    }
                    "lock" => {
                        info.locks.push(parse_lock(&mut sc)?);
                    }
                    _ => {
                        if sc.check_punct(Punct::LBrace)? {
                            skip_block(&mut sc)?;
                        }
                    }
                }
            }
            other => {
                return Err(DefsError::Message(format!(
                    "LOCKDEFS: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(info)
}

fn parse_lock(sc: &mut Scanner<'_>) -> Result<LockDef, DefsError> {
    let number = match sc.next_token()? {
        Tok::Int(n) => n as i32,
        other => {
            return Err(DefsError::Message(format!(
                "LOCKDEFS Lock: expected number, got {other:?}"
            )));
        }
    };
    let mut def = LockDef {
        number,
        ..Default::default()
    };
    // Optional game name before `{`.
    match sc.next_token()? {
        Tok::Punct(Punct::LBrace) => {}
        Tok::Ident(g) => {
            def.game = Some(g.to_owned());
            expect_lbrace(sc)?;
        }
        Tok::String(g) => {
            def.game = Some(g.into_owned());
            expect_lbrace(sc)?;
        }
        other => {
            return Err(DefsError::Message(format!(
                "LOCKDEFS Lock: expected game or `{{`, got {other:?}"
            )));
        }
    }
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(k) => {
                let key = k.to_ascii_lowercase();
                match key.as_str() {
                    "any" => {
                        expect_lbrace(sc)?;
                        let mut classes = Vec::new();
                        loop {
                            match sc.next_token()? {
                                Tok::Punct(Punct::RBrace) | Tok::Eof => break,
                                Tok::Ident(c) => classes.push(c.to_owned()),
                                Tok::String(c) => classes.push(c.into_owned()),
                                Tok::Punct(Punct::Semi) | Tok::Punct(Punct::Comma) => {}
                                other => {
                                    return Err(DefsError::Message(format!(
                                        "LOCKDEFS Any: unexpected {other:?}"
                                    )));
                                }
                            }
                        }
                        def.keys.push(LockKeyReq::Any(classes));
                    }
                    "message" => def.message = Some(read_stringy(sc)?),
                    "remotemessage" => def.remote_message = Some(read_stringy(sc)?),
                    "mapcolor" => {
                        let r = read_u8(sc)?;
                        let g = read_u8(sc)?;
                        let b = read_u8(sc)?;
                        def.map_color = Some((r, g, b));
                    }
                    "lockedsound" => def.locked_sound = Some(read_stringy(sc)?),
                    // Bare key class name.
                    _ => def.keys.push(LockKeyReq::Class(k.to_owned())),
                }
            }
            Tok::String(s) => {
                def.keys.push(LockKeyReq::Class(s.into_owned()));
            }
            other => {
                return Err(DefsError::Message(format!(
                    "LOCKDEFS Lock body: unexpected {other:?}"
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
            "LOCKDEFS: expected `{{`, got {other:?}"
        ))),
    }
}

fn read_stringy(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Ident(s) => Ok(s.to_owned()),
        other => Err(DefsError::Message(format!(
            "LOCKDEFS: expected string, got {other:?}"
        ))),
    }
}

fn read_u8(sc: &mut Scanner<'_>) -> Result<u8, DefsError> {
    match sc.next_token()? {
        Tok::Int(n) if (0..=255).contains(&n) => Ok(n as u8),
        Tok::Int(n) => Ok(n.clamp(0, 255) as u8),
        other => Err(DefsError::Message(format!(
            "LOCKDEFS: expected colour byte, got {other:?}"
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
    fn parses_lock_any_and_clear() {
        let text = r#"
ClearLocks
Lock 1 Doom
{
	RedCard
	Message "$PD_REDC"
	RemoteMessage "$PD_REDCO"
	Mapcolor 255 0 0
}
Lock 129 Doom
{
	Any { RedCard RedSkull KeyGreen }
	Message "$PD_REDK"
	Mapcolor 255 0 0
	LockedSound "misc/keytry"
}
"#;
        let info = parse_lockdefs(text).unwrap();
        let l1 = info.find(1, Some("Doom")).unwrap();
        assert_eq!(l1.game.as_deref(), Some("Doom"));
        assert_eq!(l1.keys, vec![LockKeyReq::Class("RedCard".into())]);
        assert_eq!(l1.message.as_deref(), Some("$PD_REDC"));
        assert_eq!(l1.map_color, Some((255, 0, 0)));
        let l129 = info.find(129, Some("Doom")).unwrap();
        assert!(matches!(
            &l129.keys[0],
            LockKeyReq::Any(v) if v.len() == 3 && v[0] == "RedCard"
        ));
        assert_eq!(l129.locked_sound.as_deref(), Some("misc/keytry"));
    }

    #[test]
    fn parses_gzdoom_lockdefs_fixture() {
        // GZDoom `wadsrc/static/lockdefs.txt` (GPL-3.0-or-later).
        let text = include_str!("../../../testdata/lockdefs/gzdoom_lockdefs.txt");
        let info = parse_lockdefs(text).unwrap();
        assert!(info.find(1, Some("Doom")).is_some());
        assert!(info.find(1, Some("Heretic")).is_some());
        assert!(info.find(1, Some("Hexen")).is_some());
        assert!(info.find(129, Some("Doom")).is_some());
        assert!(info.locks.len() > 40);
        let any = info.find(100, None).unwrap();
        assert!(any.keys.is_empty());
        assert_eq!(any.message.as_deref(), Some("$PD_ANY"));
    }
}
