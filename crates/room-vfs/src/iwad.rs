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
    /// The record's file name (`IWADName`), when it has one.
    pub names: Vec<String>,
    /// Files to load after the IWAD (`Load`, e.g. `extras.wad`).
    pub load: Vec<String>,
    /// Display name of an IWAD this one needs first (`Required`).
    pub required: String,
}

/// Parsed IWADINFO lump.
#[derive(Clone, Debug, Default)]
pub struct IwadInfo {
    /// Records in file order (most specific first recommended).
    pub records: Vec<IwadRecord>,
    /// Preference order for the picker (display names, as in GZDoom).
    pub order: Vec<String>,
    /// File names worth trying as IWADs (the global `Names { }` list).
    pub names: Vec<String>,
}

/// Parse a GZDoom-style IWADINFO text lump.
///
/// Every GZDoom key is accepted: `key = value[, value…]` or a bare flag
/// (`SkipBexStringsIfLanguage`). Detection uses `Name`, `Autoname`, `Game`,
/// `Config`, `MustContain`, `IWADName`, `Load` and `Required`; the other keys
/// (`Mapinfo`, `Compatibility`, `BannerColors`, …) are skipped.
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
                    // A bare flag has no `=` and no value.
                    let values = if tokens.get(i) == Some(&Tok::Eq) {
                        i += 1;
                        parse_values(&tokens, &mut i)?
                    } else {
                        Vec::new()
                    };
                    let first = values.first().cloned().unwrap_or_default();
                    match key.to_ascii_lowercase().as_str() {
                        "name" => rec.name = first,
                        "autoname" => rec.autoname = first,
                        "game" => rec.game = first,
                        "config" => rec.config = first,
                        "mustcontain" => rec.must_contain = values,
                        // `IWADName = "doom.wad", 1`: the number is a
                        // preference rank, not a file.
                        "iwadname" if !first.is_empty() => rec.names = vec![first],
                        "load" => rec.load = values,
                        "required" => rec.required = first,
                        _ => {}
                    }
                }
                expect(&tokens, &mut i, &Tok::RBrace)?;
                info.records.push(rec);
            }
            Tok::Ident(name) if name.eq_ignore_ascii_case("Names") => {
                i += 1;
                expect(&tokens, &mut i, &Tok::LBrace)?;
                info.names = parse_string_list(&tokens, &mut i)?;
                expect(&tokens, &mut i, &Tok::RBrace)?;
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

/// `value[, value…]` after a key's `=`: stops at the first token that is
/// not preceded by a comma, so the next key is left alone.
fn parse_values(tokens: &[Tok], i: &mut usize) -> Result<Vec<String>, VfsError> {
    let mut out = vec![string_or_ident(tokens, i)?];
    while tokens.get(*i) == Some(&Tok::Comma) {
        *i += 1;
        out.push(string_or_ident(tokens, i)?);
    }
    Ok(out)
}

/// The strings of a `Names { }` / `Order { }` block, commas optional.
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

    /// Detect a real IWAD from `testdata/` with Room's IWADINFO; `None` when
    /// the file is not there.
    fn detect_testdata(rel: &str) -> Option<IwadMatch> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata")
            .join(rel);
        if !path.is_file() {
            return None;
        }
        let info = parse_iwadinfo(include_str!("../../../wadsrc/room/IWADINFO")).unwrap();
        let vfs = VfsBuilder::new()
            .mount_path(&path)
            .unwrap()
            .build()
            .unwrap();
        Some(detect_iwad(&vfs, &info).expect("an IWADINFO record matches"))
    }

    #[test]
    fn detect_freedoom_from_room_iwadinfo() {
        if let Some(m) = detect_testdata("freedoom/freedoom2.wad") {
            assert_eq!(m.record.autoname, "doom.freedoom.phase2");
            assert!(m.filters.iter().any(|f| f == "game-doom"));
        }
        if let Some(m) = detect_testdata("freedoom/freedoom1.wad") {
            assert_eq!(m.record.autoname, "doom.freedoom.phase1");
        }
    }

    /// Doom II holds every lump the Freedoom: Phase 2 record used to require;
    /// it must be detected as Doom II, not Freedoom.
    #[test]
    fn detect_doom2_from_room_iwadinfo() {
        if let Some(m) = detect_testdata("doom/doom2.wad") {
            assert_eq!(m.record.autoname, "doom.id.doom2.commercial");
        }
    }

    /// `testdata/doom/doom1.wad` holds The Ultimate Doom's content (all four
    /// episodes) despite its name and PWAD header; every lump the Ultimate
    /// record requires is there, and no earlier record matches it.
    #[test]
    fn detect_ultimate_doom_from_room_iwadinfo() {
        if let Some(m) = detect_testdata("doom/doom1.wad") {
            assert_eq!(m.record.autoname, "doom.id.doom1.ultimate");
        }
    }

    /// GZDoom's syntax: bare flags, multi-value keys, `IWADName` with a
    /// rank, `MustContain` over several lines, global `Names`, `Order` by
    /// display name.
    #[test]
    fn parses_gzdoom_syntax() {
        let info = parse_iwadinfo(
            r#"
IWad
{
	Name = "Final Doom: TNT - Evilution: KEX Edition"
	Autoname = "doom.id.doom2.tnt.kex"
	Game = "Doom"
	IWADName = "tntkex.wad", 2
	Compatibility = "Shorttex", "Stairs"
	MustContain = "MAP01",
	              "REDTNT2", "GAMECONF"
	BannerColors = "a8 00 00", "a8 a8 a8"
	Load = "extras.wad", "soundtrack"
	SkipBexStringsIfLanguage
	IgnoreTitlePatches = 1
	Required = "DOOM 2: Hell on Earth"
}
Names
{
	"tntkex.wad"
	"doom2.wad"
}
Order	// Order in the IWAD selection box
{
	"Final Doom: TNT - Evilution: KEX Edition"
}
"#,
        )
        .unwrap();
        let r = &info.records[0];
        assert_eq!(r.autoname, "doom.id.doom2.tnt.kex");
        assert_eq!(r.must_contain, ["MAP01", "REDTNT2", "GAMECONF"]);
        assert_eq!(r.names, ["tntkex.wad"]);
        assert_eq!(r.load, ["extras.wad", "soundtrack"]);
        assert_eq!(r.required, "DOOM 2: Hell on Earth");
        assert_eq!(info.names, ["tntkex.wad", "doom2.wad"]);
        assert_eq!(info.order, ["Final Doom: TNT - Evilution: KEX Edition"]);
    }

    /// Room's IWADINFO: every record parses whole, and every Order entry
    /// names a record.
    #[test]
    fn room_iwadinfo_records_are_complete() {
        let info = parse_iwadinfo(include_str!("../../../wadsrc/room/IWADINFO")).unwrap();
        assert!(info.records.len() >= 24, "{} records", info.records.len());
        for r in &info.records {
            assert!(
                !r.name.is_empty() && !r.autoname.is_empty() && !r.must_contain.is_empty(),
                "incomplete record {r:?}"
            );
            assert_eq!(r.game, "Doom", "{}", r.name);
        }
        let ultimate = info
            .records
            .iter()
            .find(|r| r.autoname == "doom.id.doom1.ultimate")
            .unwrap();
        assert_eq!(ultimate.must_contain.len(), 25);
        assert_eq!(ultimate.names, ["doom.wad"]);
        assert_eq!(
            info.records.last().map(|r| r.autoname.as_str()),
            Some("doom.id.doom2.commercial")
        );
        for name in &info.order {
            assert!(
                info.records.iter().any(|r| &r.name == name),
                "Order: {name}"
            );
        }
    }

    /// same WAD with `FREEDOOM` is Freedoom: Phase 2.
    #[test]
    fn freedoom_needs_its_marker_lump() {
        let info = parse_iwadinfo(include_str!("../../../wadsrc/room/IWADINFO")).unwrap();
        let detect = |marker: bool| {
            let mut w = WadWriter::iwad();
            w.add("PLAYPAL", vec![0; 768]);
            for n in ["TITLEPIC", "MAP01", "MAP02"] {
                w.add(n, vec![1]);
            }
            if marker {
                w.add("FREEDOOM", vec![1]);
            }
            let vfs = VfsBuilder::new()
                .mount_memory("x.wad", w.finish().unwrap())
                .unwrap()
                .build()
                .unwrap();
            detect_iwad(&vfs, &info).unwrap().record.autoname
        };
        assert_eq!(detect(false), "doom.id.doom2.commercial");
        assert_eq!(detect(true), "doom.freedoom.phase2");
    }

    #[test]
    fn synthetic_wad_matches_every_iwadinfo_kind() {
        let iwadinfo = include_str!("../../../wadsrc/room/IWADINFO");
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
            // Records go from most to least specific, so a WAD holding just
            // one record's MustContain lumps selects exactly that record.
            assert_eq!(
                m.record.autoname, record.autoname,
                "{}'s own lumps matched {}: an earlier record is less specific",
                record.autoname, m.record.autoname
            );
        }
    }
}
