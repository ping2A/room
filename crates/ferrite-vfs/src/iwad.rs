//! IWADINFO parsing and IWAD candidate matching.

use crate::error::VfsError;
use crate::namespace::Namespace;
use crate::vfs::Vfs;
use std::collections::BTreeSet;

/// One `IWad { ... }` record from IWADINFO.
#[derive(Clone, Debug, Default)]
pub struct IwadRecord {
    /// Display name.
    pub name: String,
    /// Dotted autoname (`doom.id.doom2.commercial`).
    pub autoname: String,
    /// Game family (`Doom`, `Heretic`, …).
    pub game: String,
    /// Config section name.
    pub config: String,
    /// Required lump short names (all must be present).
    pub must_contain: Vec<String>,
    /// Candidate file names (from `Names { }`).
    pub names: Vec<String>,
}

/// Parsed IWADINFO lump.
#[derive(Clone, Debug, Default)]
pub struct IwadInfo {
    /// Records in file order (most specific first recommended).
    pub records: Vec<IwadRecord>,
    /// Preference order for the picker.
    pub order: Vec<String>,
}

/// Parse a GZDoom-style IWADINFO text lump (subset sufficient for detection).
pub fn parse_iwadinfo(text: &str) -> Result<IwadInfo, VfsError> {
    let mut info = IwadInfo::default();
    let mut chars = text.chars().peekable();
    let mut tokens = Vec::new();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '/' && chars.clone().nth(1) == Some('/') {
            for ch in chars.by_ref() {
                if ch == '\n' {
                    break;
                }
            }
            continue;
        }
        if c == '"' {
            chars.next();
            let mut s = String::new();
            for ch in chars.by_ref() {
                if ch == '"' {
                    break;
                }
                s.push(ch);
            }
            tokens.push(Tok::String(s));
            continue;
        }
        if c == '{' {
            chars.next();
            tokens.push(Tok::LBrace);
            continue;
        }
        if c == '}' {
            chars.next();
            tokens.push(Tok::RBrace);
            continue;
        }
        if c == '=' {
            chars.next();
            tokens.push(Tok::Eq);
            continue;
        }
        if c == ',' {
            chars.next();
            tokens.push(Tok::Comma);
            continue;
        }
        let mut ident = String::new();
        while let Some(&ch) = chars.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
                ident.push(ch);
                chars.next();
            } else {
                break;
            }
        }
        if ident.is_empty() {
            chars.next();
            continue;
        }
        tokens.push(Tok::Ident(ident));
    }

    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Tok::Ident(name) if name.eq_ignore_ascii_case("IWad") => {
                i += 1;
                expect(&tokens, &mut i, &Tok::LBrace)?;
                let mut rec = IwadRecord::default();
                while i < tokens.len() && tokens[i] != Tok::RBrace {
                    let key = ident(&tokens, &mut i)?;
                    if tokens.get(i) == Some(&Tok::Eq) {
                        i += 1;
                    }
                    if key.eq_ignore_ascii_case("MustContain") {
                        rec.must_contain = parse_string_list(&tokens, &mut i)?;
                    } else {
                        let val = string_or_ident(&tokens, &mut i)?;
                        match key.to_ascii_lowercase().as_str() {
                            "name" => rec.name = val,
                            "autoname" => rec.autoname = val,
                            "game" => rec.game = val,
                            "config" => rec.config = val,
                            _ => {}
                        }
                        if tokens.get(i) == Some(&Tok::Comma) {
                            i += 1;
                        }
                    }
                }
                expect(&tokens, &mut i, &Tok::RBrace)?;
                info.records.push(rec);
            }
            Tok::Ident(name) if name.eq_ignore_ascii_case("Names") => {
                i += 1;
                expect(&tokens, &mut i, &Tok::LBrace)?;
                let names = parse_string_list(&tokens, &mut i)?;
                expect(&tokens, &mut i, &Tok::RBrace)?;
                if let Some(last) = info.records.last_mut() {
                    last.names = names;
                }
            }
            Tok::Ident(name) if name.eq_ignore_ascii_case("Order") => {
                i += 1;
                expect(&tokens, &mut i, &Tok::LBrace)?;
                info.order = parse_string_list(&tokens, &mut i)?;
                expect(&tokens, &mut i, &Tok::RBrace)?;
            }
            _ => i += 1,
        }
    }
    Ok(info)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Ident(String),
    String(String),
    LBrace,
    RBrace,
    Eq,
    Comma,
}

fn expect(tokens: &[Tok], i: &mut usize, want: &Tok) -> Result<(), VfsError> {
    if tokens.get(*i) == Some(want) {
        *i += 1;
        Ok(())
    } else {
        Err(VfsError::parse(format!("IWADINFO: expected {want:?}")))
    }
}

fn ident(tokens: &[Tok], i: &mut usize) -> Result<String, VfsError> {
    match tokens.get(*i) {
        Some(Tok::Ident(s)) => {
            *i += 1;
            Ok(s.clone())
        }
        _ => Err(VfsError::parse("IWADINFO: expected identifier")),
    }
}

fn string_or_ident(tokens: &[Tok], i: &mut usize) -> Result<String, VfsError> {
    match tokens.get(*i) {
        Some(Tok::String(s) | Tok::Ident(s)) => {
            *i += 1;
            Ok(s.clone())
        }
        _ => Err(VfsError::parse("IWADINFO: expected string")),
    }
}

fn parse_string_list(tokens: &[Tok], i: &mut usize) -> Result<Vec<String>, VfsError> {
    let mut out = Vec::new();
    while *i < tokens.len() {
        match &tokens[*i] {
            Tok::String(s) | Tok::Ident(s) => {
                out.push(s.clone());
                *i += 1;
                if tokens.get(*i) == Some(&Tok::Comma) {
                    *i += 1;
                }
            }
            Tok::RBrace => break,
            _ => break,
        }
    }
    Ok(out)
}

/// Result of matching a candidate archive against IWADINFO.
#[derive(Clone, Debug)]
pub struct IwadMatch {
    /// Matching record.
    pub record: IwadRecord,
    /// Active filter directory names derived from Autoname / Game.
    pub filters: Vec<String>,
}

impl IwadMatch {
    /// Build filter list from a record.
    #[must_use]
    pub fn filters_for(record: &IwadRecord) -> Vec<String> {
        let mut filters = Vec::new();
        if !record.game.is_empty() {
            filters.push(format!("game-{}", record.game.to_ascii_lowercase()));
        }
        if !record.autoname.is_empty() {
            let parts: Vec<&str> = record.autoname.split('.').collect();
            for n in 1..=parts.len() {
                filters.push(parts[..n].join("."));
            }
        }
        filters
    }
}

/// Detect which IWADINFO record a mounted VFS satisfies.
///
/// Picks the first record (file order) whose entire `MustContain` list is present
/// as short names in [`Namespace::Global`] (or any namespace via `find_any_ns`).
pub fn detect_iwad(vfs: &Vfs, info: &IwadInfo) -> Option<IwadMatch> {
    for record in &info.records {
        if record.must_contain.is_empty() {
            continue;
        }
        let ok = record
            .must_contain
            .iter()
            .all(|name| vfs.find_any_ns(name).is_some());
        if ok {
            return Some(IwadMatch {
                filters: IwadMatch::filters_for(record),
                record: record.clone(),
            });
        }
    }
    None
}

/// Collect short names present for synthetic MustContain tests.
#[must_use]
pub fn short_names_in(vfs: &Vfs) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    for (_id, info) in vfs.iter() {
        if !info.short_name.is_empty() && info.namespace == Namespace::Global {
            set.insert(info.short_name.as_str().to_owned());
        }
        if !info.short_name.is_empty() {
            set.insert(info.short_name.as_str().to_owned());
        }
    }
    set
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::vfs::VfsBuilder;
    use crate::write::WadWriter;

    const SAMPLE: &str = r#"
IWad
{
    Name = "DOOM 2: Hell on Earth"
    Autoname = "doom.id.doom2.commercial"
    Game = "Doom"
    Config = "Doom"
    MustContain = "MAP01", "MAP02", "PLAYPAL"
}
Names { "doom2.wad" }
"#;

    #[test]
    fn parse_and_detect() {
        let info = parse_iwadinfo(SAMPLE).unwrap();
        assert_eq!(info.records.len(), 1);
        assert_eq!(info.records[0].must_contain.len(), 3);

        let wad = WadWriter::iwad()
            .add("PLAYPAL", vec![0; 768])
            .add("MAP01", vec![1])
            .add("MAP02", vec![2])
            .finish()
            .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("doom2.wad", wad)
            .unwrap()
            .build()
            .unwrap();
        let m = detect_iwad(&vfs, &info).unwrap();
        assert!(m.filters.iter().any(|f| f == "doom.id.doom2.commercial"));
        assert!(m.filters.iter().any(|f| f == "game-doom"));
    }

    #[test]
    fn detect_freedoom2_from_ferrite_iwadinfo() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/freedoom/freedoom2.wad");
        if !path.is_file() {
            return;
        }
        let iwadinfo = include_str!("../../../wadsrc/ferrite/IWADINFO");
        let info = parse_iwadinfo(iwadinfo).unwrap();
        assert!(
            info.records
                .iter()
                .any(|r| r.autoname == "doom.freedoom.phase2")
        );
        let vfs = VfsBuilder::new()
            .mount_path(&path)
            .unwrap()
            .build()
            .unwrap();
        let m = detect_iwad(&vfs, &info).unwrap();
        assert_eq!(m.record.autoname, "doom.freedoom.phase2");
        assert!(m.filters.iter().any(|f| f == "game-doom"));
    }

    #[test]
    fn synthetic_wad_matches_every_iwadinfo_kind() {
        let iwadinfo = include_str!("../../../wadsrc/ferrite/IWADINFO");
        let info = parse_iwadinfo(iwadinfo).unwrap();
        assert!(
            !info.records.is_empty(),
            "IWADINFO must list at least one IWad"
        );

        for record in &info.records {
            // Isolating MustContain lumps: first match in file order is this record
            // only when earlier records' requirements are not a subset. Prefer an
            // exclusive synthetic set = this record's MustContain alone, and assert
            // detect picks *some* record that is satisfied; then require that every
            // MustContain name was found and that the matched autoname is either
            // this record or an earlier one that is also fully satisfied.
            let mut w = WadWriter::iwad();
            for name in &record.must_contain {
                let payload = if name.eq_ignore_ascii_case("PLAYPAL") {
                    vec![0; 768]
                } else {
                    vec![1]
                };
                w.add(name, payload);
            }
            let wad = w.finish().unwrap();
            let vfs = VfsBuilder::new()
                .mount_memory(format!("{}.wad", record.autoname), wad)
                .unwrap()
                .build()
                .unwrap();
            let m = detect_iwad(&vfs, &info).unwrap();
            assert!(
                record
                    .must_contain
                    .iter()
                    .all(|n| vfs.find_any_ns(n).is_some()),
                "synthetic {} missing lumps",
                record.autoname
            );
            // Matched record must be this one, or an earlier record whose MustContain
            // is a subset of this synthetic WAD (GZDoom picks first in file order).
            let matched_idx = info
                .records
                .iter()
                .position(|r| r.autoname == m.record.autoname)
                .unwrap();
            let self_idx = info
                .records
                .iter()
                .position(|r| r.autoname == record.autoname)
                .unwrap();
            assert!(
                matched_idx <= self_idx,
                "expected match at or before {}, got {}",
                record.autoname,
                m.record.autoname
            );
            assert!(
                m.record.must_contain.iter().all(|n| record
                    .must_contain
                    .iter()
                    .any(|x| x.eq_ignore_ascii_case(n))),
                "matched {} MustContain not subset of synthetic {}",
                m.record.autoname,
                record.autoname
            );
        }
    }
}
