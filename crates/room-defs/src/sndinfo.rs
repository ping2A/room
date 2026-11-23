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
    /// `$playersound` and related mappings: (class, gender, logical sound) → target.
    pub player_sounds: BTreeMap<(String, String, String), SoundDef>,
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

    /// Resolve a `*sound` token through `$playersound*`, with class/gender fallbacks.
    #[must_use]
    pub fn resolve_player_lump(&self, class: &str, gender: &str, logical: &str) -> Option<&str> {
        let mut name = logical.to_ascii_lowercase();
        let class = class.to_ascii_lowercase();
        let gender = gender.to_ascii_lowercase();
        for _ in 0..16 {
            let definition = [
                // `S_LookupPlayerSound`: the class's own gender, then male,
                // then any gender the class defines, then the default class.
                (class.as_str(), gender.as_str()),
                (class.as_str(), "male"),
                (class.as_str(), "female"),
                (class.as_str(), "neutral"),
                (class.as_str(), "other"),
                ("player", gender.as_str()),
                ("player", "male"),
                ("player", "female"),
                ("player", "neutral"),
                ("player", "other"),
            ]
            .into_iter()
            .find_map(|(class, gender)| {
                self.player_sounds
                    .get(&(class.to_owned(), gender.to_owned(), name.clone()))
            })?;
            match definition {
                SoundDef::Lump(lump) => return Some(lump),
                SoundDef::Alias(target) => name = target.to_ascii_lowercase(),
                SoundDef::Random(_) => return None,
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
/// Nothing but blanks or a comment left on the current line (SNDINFO is
/// line-based: a directive never continues on the next line).
fn at_line_end(sc: &Scanner<'_>) -> bool {
    let rest = sc.remaining().trim_start_matches([' ', '\t', '\r']);
    rest.is_empty() || rest.starts_with('\n') || rest.starts_with("//") || rest.starts_with(';')
}

/// Words left on the current line (consumed).
fn rest_of_line(sc: &mut Scanner<'_>) -> Result<Vec<String>, DefsError> {
    let mut out = Vec::new();
    while !at_line_end(sc) {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Ident(s) => out.push(s.to_owned()),
            Tok::String(s) => out.push(s.into_owned()),
            Tok::Int(n) => out.push(n.to_string()),
            Tok::Float(f) => out.push(f.to_string()),
            _ => {}
        }
    }
    Ok(out)
}

/// Parse a SNDINFO lump into the logical-name → lump sound registry.
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
                        // Optional distance (ignored), on the same line only.
                        rest_of_line(&mut sc)?;
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
                        // `$rolloff name [type] min max` (GZDoom: an optional
                        // linear / log / custom word, then two numbers). Read
                        // to the end of the line: taking a fixed count of
                        // tokens swallowed the next definition's name and
                        // shifted every following pair (Brutal Doom).
                        let name = read_word(&mut sc)?;
                        let args = rest_of_line(&mut sc)?;
                        reg.rolloffs
                            .insert(name.to_ascii_lowercase(), args.join(" "));
                    }
                    "$pitchshift" => {
                        let name = read_word(&mut sc)?;
                        let n = sc.must_get_number()?;
                        reg.pitchshifts.insert(name.to_ascii_lowercase(), n);
                    }
                    "$playersound" | "$playersounddup" | "$playeralias" => {
                        let class = read_word(&mut sc)?.to_ascii_lowercase();
                        let gender = read_word(&mut sc)?.to_ascii_lowercase();
                        let logical = read_word(&mut sc)?.to_ascii_lowercase();
                        let target = read_word(&mut sc)?;
                        let definition = if cmd == "$playersound" {
                            SoundDef::Lump(target)
                        } else {
                            SoundDef::Alias(target)
                        };
                        reg.player_sounds
                            .insert((class, gender, logical), definition);
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
                        // `$singular`, `$pitchset`, `$attenuation`, … take a
                        // varying number of arguments: skip the whole line.
                        rest_of_line(&mut sc)?;
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

    /// Brutal Doom's `SNDINFO.BD`: two-number `$rolloff` lines followed by
    /// a definition, `$limit` with a distance, multi-argument directives.
    #[test]
    fn line_based_directives_keep_the_next_definition() {
        let text = "Rifle/Fire\t\t\tDSRIFLE\n$rolloff Rifle/Fire 200 2200\n$limit Rifle/Fire 5\n\n\
                    Rifle/Fire2\t\t\tDSRIFL2\n$limit Rifle/Fire2 5\n$rolloff Rifle/Fire2 500 2200\n\n\
                    Weapon/Empty DRYFIRE\nRifle/Select CLIPIN\n\
                    FARRIF FARRIF\n$limit FARRIF 80 512\n$rolloff  FARRIF 3000 6000\n\
                    $pitchset Rifle/Select 0.9 1.1\n$attenuation Rifle/Select 1.5\n$singular Weapon/Empty\n\
                    $rolloff Plasma/Fire log 1.5 // comment\nPlasma/Fire PLSM9\n";
        let reg = parse_sndinfo(text).unwrap();
        for (logical, lump) in [
            ("rifle/fire", "DSRIFLE"),
            ("rifle/fire2", "DSRIFL2"),
            ("weapon/empty", "DRYFIRE"),
            ("rifle/select", "CLIPIN"),
            ("farrif", "FARRIF"),
            ("plasma/fire", "PLSM9"),
        ] {
            assert_eq!(reg.resolve_lump(logical, 0), Some(lump), "{logical}");
        }
        // No directive word or number became a sound.
        assert!(
            reg.sounds
                .keys()
                .all(|k| !k.starts_with('$') && k.parse::<f64>().is_err())
        );
        assert!(!reg.sounds.contains_key("dsrifl2"));
        assert_eq!(reg.limits.get("farrif"), Some(&80));
        assert_eq!(
            reg.rolloffs.get("farrif").map(String::as_str),
            Some("3000 6000")
        );
        assert_eq!(
            reg.rolloffs.get("plasma/fire").map(String::as_str),
            Some("log 1.5")
        );
    }
}
