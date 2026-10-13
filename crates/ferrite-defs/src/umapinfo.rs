//! UMAPINFO parser (brace syntax, additive over MAPINFO).

use crate::error::DefsError;
use crate::mapinfo::{EpisodeDef, MapDef, MapInfo};
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};

/// Parse UMAPINFO into a [`MapInfo`] (maps only; episodes via `episode` keys).
pub fn parse_umapinfo(text: &str) -> Result<MapInfo, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            ..Default::default()
        },
    );
    let mut info = MapInfo::default();
    let mut clear_episodes = false;
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Ident(kw) if Scanner::compare(kw, "map") => {
                let name = match sc.next_token()? {
                    Tok::Ident(s) => s.to_ascii_uppercase(),
                    Tok::String(s) => s.into_owned().to_ascii_uppercase(),
                    other => {
                        return Err(DefsError::Message(format!(
                            "UMAPINFO: expected map name, got {other:?}"
                        )));
                    }
                };
                if !sc.check_punct(Punct::LBrace)? {
                    return Err(DefsError::Message("UMAPINFO: expected `{`".into()));
                }
                let mut def = MapDef {
                    name: name.clone(),
                    ..Default::default()
                };
                loop {
                    match sc.next_token()? {
                        Tok::Punct(Punct::RBrace) | Tok::Eof => break,
                        Tok::Ident(key) => {
                            let _ = sc.check_punct(Punct::Eq)?;
                            let key_l = key.to_ascii_lowercase();
                            match key_l.as_str() {
                                "levelname" | "label" => {
                                    def.title = Some(read_value(&mut sc)?);
                                }
                                "next" => {
                                    def.next = Some(read_value(&mut sc)?.to_ascii_uppercase());
                                }
                                "nextsecret" | "secretnext" => {
                                    def.secret_next =
                                        Some(read_value(&mut sc)?.to_ascii_uppercase());
                                }
                                "skytexture" | "sky1" => {
                                    def.sky1 = Some(read_value(&mut sc)?);
                                }
                                "music" => {
                                    def.music = Some(read_value(&mut sc)?);
                                }
                                "partime" | "par" => {
                                    let v = read_value(&mut sc)?;
                                    if let Ok(n) = v.parse::<u32>() {
                                        def.par_seconds = Some(n);
                                    } else {
                                        def.extras.insert(key_l, v);
                                    }
                                }
                                "bossaction" => {
                                    let joined = read_csv_values(&mut sc)?;
                                    // Multiple bossaction lines append with `|`.
                                    def.extras
                                        .entry("bossaction".into())
                                        .and_modify(|e| {
                                            e.push('|');
                                            e.push_str(&joined);
                                        })
                                        .or_insert(joined);
                                }
                                "endpic" | "endbunny" | "endgame" | "endcast" | "intertext"
                                | "intertextsecret" | "interbackdrop" | "intermusic"
                                | "exitpic" | "enterpic" | "nointermission" | "author" => {
                                    let v = read_csv_values(&mut sc)?;
                                    def.extras.insert(key_l, v);
                                }
                                "episode" => {
                                    // `episode = clear` or `episode = "map", "name", "key"`
                                    let parts = read_csv_values(&mut sc)?;
                                    if parts.eq_ignore_ascii_case("clear") {
                                        clear_episodes = true;
                                        info.episodes.clear();
                                    } else {
                                        let mut bits = parts.split(',').map(str::trim);
                                        if let Some(start) = bits.next() {
                                            let ep = EpisodeDef {
                                                start_map: start.to_ascii_uppercase(),
                                                name: bits.next().map(|s| s.to_owned()),
                                            };
                                            info.episodes.push(ep);
                                        }
                                    }
                                }
                                other => {
                                    let v = read_csv_values(&mut sc)?;
                                    def.extras.insert(other.to_owned(), v);
                                }
                            }
                            let _ = sc.check_punct(Punct::Semi)?;
                        }
                        Tok::Punct(Punct::Semi | Punct::Comma) => {}
                        other => {
                            return Err(DefsError::Message(format!(
                                "UMAPINFO: unexpected {other:?}"
                            )));
                        }
                    }
                }
                info.maps.insert(name, def);
            }
            Tok::Ident(kw) if Scanner::compare(kw, "episode") => {
                let _ = sc.check_punct(Punct::Eq)?;
                let parts = read_csv_values(&mut sc)?;
                let _ = sc.check_punct(Punct::Semi)?;
                if parts.eq_ignore_ascii_case("clear") {
                    clear_episodes = true;
                    info.episodes.clear();
                } else {
                    let mut bits = parts.split(',').map(str::trim);
                    if let Some(start) = bits.next() {
                        info.episodes.push(EpisodeDef {
                            start_map: start.to_ascii_uppercase(),
                            name: bits.next().map(|s| s.to_owned()),
                        });
                    }
                }
            }
            Tok::Ident(_) => skip_optional_block(&mut sc)?,
            _ => {}
        }
    }
    let _ = clear_episodes;
    Ok(info)
}

fn skip_optional_block(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    if !sc.check_punct(Punct::LBrace)? {
        return Ok(());
    }
    let mut d = 1i32;
    while d > 0 {
        match sc.next_token()? {
            Tok::Punct(Punct::LBrace) => d += 1,
            Tok::Punct(Punct::RBrace) => d -= 1,
            Tok::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

fn read_value(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Int(n) => Ok(n.to_string()),
        Tok::Float(f) => Ok(f.to_string()),
        other => Err(DefsError::Message(format!(
            "UMAPINFO: expected value, got {other:?}"
        ))),
    }
}

/// One or more comma-separated values (UMAPINFO `bossaction`, `episode`, …).
fn read_csv_values(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    let mut parts = vec![read_value(sc)?];
    while sc.check_punct(Punct::Comma)? {
        parts.push(read_value(sc)?);
    }
    Ok(parts.join(","))
}

/// Merge UMAPINFO maps over a base MAPINFO (field-wise additive; later wins).
pub fn merge_umapinfo(base: &mut MapInfo, extra: MapInfo) {
    for (k, v) in extra.maps {
        match base.maps.get_mut(&k) {
            Some(dst) => merge_map_def(dst, v),
            None => {
                base.maps.insert(k, v);
            }
        }
    }
    if !extra.episodes.is_empty() {
        // Non-empty episode list from UMAPINFO replaces when `episode = clear` was used,
        // otherwise appends.
        if base.episodes.is_empty() {
            base.episodes = extra.episodes;
        } else {
            base.episodes.extend(extra.episodes);
        }
    }
}

fn merge_map_def(dst: &mut MapDef, src: MapDef) {
    if src.title.is_some() {
        dst.title = src.title;
    }
    if src.next.is_some() {
        dst.next = src.next;
    }
    if src.secret_next.is_some() {
        dst.secret_next = src.secret_next;
    }
    if src.sky1.is_some() {
        dst.sky1 = src.sky1;
    }
    if src.music.is_some() {
        dst.music = src.music;
    }
    if src.par_seconds.is_some() {
        dst.par_seconds = src.par_seconds;
    }
    for (k, v) in src.extras {
        dst.extras.insert(k, v);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parse_umap() {
        let text = r#"
MAP MAP01
{
  levelname = "Entryway"
  next = "MAP02"
  music = "D_RUNNIN"
}
"#;
        let info = parse_umapinfo(text).unwrap();
        let m = info.maps.get("MAP01").unwrap();
        assert_eq!(m.title.as_deref(), Some("Entryway"));
        assert_eq!(m.next.as_deref(), Some("MAP02"));
    }

    #[test]
    fn parse_bossaction_and_endpic() {
        let text = r#"
MAP MAP07
{
  levelname = "Dead Simple"
  bossaction = Fatso, 23, 666
  bossaction = Arachnotron, 23, 667
  endpic = "CREDIT"
  partime = 120
}
"#;
        let info = parse_umapinfo(text).unwrap();
        let m = info.maps.get("MAP07").unwrap();
        assert_eq!(m.par_seconds, Some(120));
        assert_eq!(
            m.extras.get("bossaction").map(String::as_str),
            Some("Fatso,23,666|Arachnotron,23,667")
        );
        assert_eq!(m.extras.get("endpic").map(String::as_str), Some("CREDIT"));
    }

    #[test]
    fn merge_is_additive() {
        let mut base = MapInfo::default();
        base.maps.insert(
            "MAP01".into(),
            MapDef {
                name: "MAP01".into(),
                title: Some("Old".into()),
                next: Some("MAP02".into()),
                music: Some("D_RUNNIN".into()),
                ..Default::default()
            },
        );
        let extra = parse_umapinfo(
            r#"
MAP MAP01
{
  levelname = "New Title"
  endpic = "VICTORY2"
}
"#,
        )
        .unwrap();
        merge_umapinfo(&mut base, extra);
        let m = base.maps.get("MAP01").unwrap();
        assert_eq!(m.title.as_deref(), Some("New Title"));
        assert_eq!(m.next.as_deref(), Some("MAP02"));
        assert_eq!(m.music.as_deref(), Some("D_RUNNIN"));
        assert_eq!(m.extras.get("endpic").map(String::as_str), Some("VICTORY2"));
    }

    #[test]
    fn episode_clear_and_add() {
        let info = parse_umapinfo(
            r#"
MAP MAP01 { levelname = "A" }
episode = clear
episode = "MAP01", "Knee-Deep", "k"
"#,
        )
        .unwrap();
        assert_eq!(info.episodes.len(), 1);
        assert_eq!(info.episodes[0].start_map, "MAP01");
        assert_eq!(info.episodes[0].name.as_deref(), Some("Knee-Deep"));
    }
}
