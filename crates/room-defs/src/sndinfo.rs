//! SNDINFO parser (non-C mode).

use crate::error::DefsError;
use room_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// Logical sound → lump or random/alias target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoundDef {
    /// Direct lump name.
    Lump(String),
    /// `$alias` target logical name.
    Alias(String),
    /// `$random` list of logical names.
    Random(Vec<String>),
}

/// Parsed SNDINFO registry (subset of commands).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundRegistry {
    /// Logical name (lowercase) → definition.
    pub sounds: BTreeMap<String, SoundDef>,
    /// `$limit` values.
    pub limits: BTreeMap<String, i64>,
    /// `$volume` values.
    pub volumes: BTreeMap<String, String>,
    /// `$map` mapnum → music.
    pub map_music: BTreeMap<i64, String>,
    /// `$rolloff` name → raw args joined.
    pub rolloffs: BTreeMap<String, String>,
    /// `$pitchshift` name → range.
    pub pitchshifts: BTreeMap<String, i64>,
    /// Warnings for skipped/unknown commands.
    pub warnings: Vec<String>,
}

impl SoundRegistry {
    /// Resolve a logical name to a lump, following `$alias` chains (max 16).
    ///
    /// For `$random`, picks `list[index % len]` (caller supplies a deterministic index).
    #[must_use]
    pub fn resolve_lump(&self, logical: &str, random_index: u32) -> Option<&str> {
        let mut name = logical.to_ascii_lowercase();
        for _ in 0..16 {
            match self.sounds.get(&name) {
                Some(SoundDef::Lump(lump)) => return Some(lump.as_str()),
                Some(SoundDef::Alias(target)) => {
                    name = target.to_ascii_lowercase();
                }
                Some(SoundDef::Random(list)) => {
                    if list.is_empty() {
                        return None;
                    }
                    let pick = &list[random_index as usize % list.len()];
                    // Random entries may be lumps or further logical names.
                    if !self.sounds.contains_key(&pick.to_ascii_lowercase()) {
                        return Some(pick.as_str());
                    }
                    name = pick.to_ascii_lowercase();
                }
                None => return None,
            }
        }
        None
    }

    /// Music lump for `$map` entry, if any.
    #[must_use]
    pub fn music_for_map(&self, mapnum: i64) -> Option<&str> {
        self.map_music.get(&mapnum).map(String::as_str)
    }
}

/// Parse an SNDINFO lump.
pub fn parse_sndinfo(text: &str) -> Result<SoundRegistry, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: false,
            semicolon_comments: true,
            ..Default::default()
        },
    );
    let mut reg = SoundRegistry::default();
    let mut skip_until_endif = false;

    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Ident(tok) if skip_until_endif => {
                if Scanner::compare(tok, "$endif") {
                    skip_until_endif = false;
                }
            }
            Tok::Ident(tok) if tok.starts_with('$') => {
                let cmd = tok.to_ascii_lowercase();
                match cmd.as_str() {
                    "$alias" => {
                        let logical = read_word(&mut sc)?;
                        let target = read_word(&mut sc)?;
                        reg.sounds
                            .insert(logical.to_ascii_lowercase(), SoundDef::Alias(target));
                    }
                    "$random" => {
                        let logical = read_word(&mut sc)?;
                        if !sc.check_punct(Punct::LBrace)? {
                            return Err(DefsError::Message("SNDINFO $random: expected `{`".into()));
                        }
                        let mut list = Vec::new();
                        loop {
                            match sc.next_token()? {
                                Tok::Punct(Punct::RBrace) | Tok::Eof => break,
                                Tok::Ident(s) => list.push(s.to_owned()),
                                Tok::String(s) => list.push(s.into_owned()),
                                _ => {}
                            }
                        }
                        reg.sounds
                            .insert(logical.to_ascii_lowercase(), SoundDef::Random(list));
                    }
                    "$limit" => {
                        let name = read_word(&mut sc)?;
                        let n = sc.must_get_number()?;
                        // optional distance — ignore if present
                        let peek = sc.next_token()?;
                        match peek {
                            Tok::Int(_) | Tok::Float(_) => {}
                            other => sc.unget(other),
                        }
                        reg.limits.insert(name.to_ascii_lowercase(), n);
                    }
                    "$volume" => {
                        let name = read_word(&mut sc)?;
                        let vol = read_word(&mut sc)?;
                        reg.volumes.insert(name.to_ascii_lowercase(), vol);
                    }
                    "$map" => {
                        let mapnum = sc.must_get_number()?;
                        let music = read_word(&mut sc)?;
                        reg.map_music.insert(mapnum, music);
                    }
                    "$rolloff" => {
                        let name = read_word(&mut sc)?;
                        let mut args = Vec::new();
                        loop {
                            match sc.next_token()? {
                                Tok::Ident(s) if s.starts_with('$') => {
                                    sc.unget(Tok::Ident(s));
                                    break;
                                }
                                Tok::Ident(s) => args.push(s.to_owned()),
                                Tok::String(s) => args.push(s.into_owned()),
                                Tok::Int(n) => args.push(n.to_string()),
                                Tok::Float(f) => args.push(f.to_string()),
                                Tok::Eof => break,
                                other => {
                                    sc.unget(other);
                                    break;
                                }
                            }
                            // Stop at end of "line" heuristically: if next is a new logical assign
                            // we can't know without newline tracking; take one type + optional numbers.
                            if args.len() >= 3 {
                                break;
                            }
                        }
                        reg.rolloffs
                            .insert(name.to_ascii_lowercase(), args.join(" "));
                    }
                    "$pitchshift" => {
                        let name = read_word(&mut sc)?;
                        let n = sc.must_get_number()?;
                        reg.pitchshifts.insert(name.to_ascii_lowercase(), n);
                    }
                    "$ifdoom" | "$ifheretic" | "$ifhexen" | "$ifstrife" => {
                        // Stub: always take the doom branch for now; skip others.
                        if !Scanner::compare(tok, "$ifdoom") {
                            skip_until_endif = true;
                        }
                    }
                    "$endif" => {}
                    "$registered" | "$edfoverride" => {}
                    other => {
                        reg.warnings
                            .push(format!("SNDINFO: unsupported command {other}"));
                        // skip rest of “line”: consume tokens until something that looks like a new command/assign
                        // Best effort: consume one more word if present.
                        let next = sc.next_token()?;
                        match next {
                            Tok::Ident(s) if s.starts_with('$') => sc.unget(Tok::Ident(s)),
                            Tok::Eof => break,
                            _ => {}
                        }
                    }
                }
            }
            Tok::Ident(logical) => {
                // `logicalname lumpname`
                let lump = match sc.next_token()? {
                    Tok::Ident(s) => s.to_owned(),
                    Tok::String(s) => s.into_owned(),
                    other => {
                        sc.unget(other);
                        reg.warnings
                            .push(format!("SNDINFO: expected lump after {logical}"));
                        continue;
                    }
                };
                reg.sounds
                    .insert(logical.to_ascii_lowercase(), SoundDef::Lump(lump));
            }
            Tok::Punct(_) => {}
            other => {
                reg.warnings
                    .push(format!("SNDINFO: unexpected token {other:?}"));
            }
        }
    }
    Ok(reg)
}

fn read_word(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Int(n) => Ok(n.to_string()),
        Tok::Float(f) => Ok(f.to_string()),
        other => Err(DefsError::Message(format!(
            "SNDINFO: expected word, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn basic_sndinfo() {
        let text = r#"
weapons/pistol	DSPISTOL
$alias weapons/pistol2 weapons/pistol
$random world/drip { drip1 drip2 drip3 }
$limit weapons/pistol 4
$map 1 D_RUNNIN
"#;
        let r = parse_sndinfo(text).unwrap();
        assert!(matches!(
            r.sounds.get("weapons/pistol"),
            Some(SoundDef::Lump(s)) if s == "DSPISTOL"
        ));
        assert!(matches!(
            r.sounds.get("weapons/pistol2"),
            Some(SoundDef::Alias(s)) if s == "weapons/pistol"
        ));
        assert!(matches!(
            r.sounds.get("world/drip"),
            Some(SoundDef::Random(v)) if v.len() == 3
        ));
        assert_eq!(r.limits.get("weapons/pistol"), Some(&4));
        assert_eq!(r.map_music.get(&1).map(String::as_str), Some("D_RUNNIN"));
        assert_eq!(r.resolve_lump("weapons/pistol2", 0), Some("DSPISTOL"));
        assert_eq!(r.resolve_lump("world/drip", 1), Some("drip2"));
        assert_eq!(r.music_for_map(1), Some("D_RUNNIN"));
    }

    #[test]
    fn pitchshift_and_rolloff() {
        let text = r#"
$pitchshift weapons/pistol 4
$rolloff weapons/pistol custom 200 1000
weapons/pistol DSPISTOL
"#;
        let r = parse_sndinfo(text).unwrap();
        assert_eq!(r.pitchshifts.get("weapons/pistol"), Some(&4));
        assert!(
            r.rolloffs
                .get("weapons/pistol")
                .is_some_and(|s| s.contains("custom"))
        );
    }
}
