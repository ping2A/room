//! GAMEINFO lump (PWAD game overrides).

use crate::error::DefsError;
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};

/// Parsed GAMEINFO keys (subset).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameInfo {
    /// `startmap` / `mapinfo` style start.
    pub start_map: Option<String>,
    /// Title / IWAD title override.
    pub title: Option<String>,
    /// `iwad` preference name.
    pub iwad: Option<String>,
    /// Extra `load` entries.
    pub load: Vec<String>,
}

/// Parse a GAMEINFO text lump.
pub fn parse_gameinfo(text: &str) -> Result<GameInfo, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            ..Default::default()
        },
    );
    let mut info = GameInfo::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Ident(key) => {
                let _ = sc.check_punct(Punct::Eq)?;
                let key_l = key.to_ascii_lowercase();
                match key_l.as_str() {
                    "startmap" | "mapinfo" => {
                        info.start_map = Some(read_value(&mut sc)?);
                    }
                    "titlepage" | "title" => {
                        info.title = Some(read_value(&mut sc)?);
                    }
                    "iwad" => {
                        info.iwad = Some(read_value(&mut sc)?);
                    }
                    "load" => {
                        info.load.push(read_value(&mut sc)?);
                    }
                    _ => {
                        // Skip unknown key value for forward compatibility.
                        let _ = read_value(&mut sc);
                    }
                }
                let _ = sc.check_punct(Punct::Semi)?;
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!(
                    "GAMEINFO: unexpected token {other:?}"
                )));
            }
        }
    }
    Ok(info)
}

fn read_value(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::Int(n) => Ok(n.to_string()),
        other => Err(DefsError::Message(format!(
            "GAMEINFO: expected value, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_basic_gameinfo() {
        let g = parse_gameinfo(
            r#"
            title = "My Mod";
            startmap = MAP01;
            load = "extra.wad";
        "#,
        )
        .unwrap();
        assert_eq!(g.title.as_deref(), Some("My Mod"));
        assert_eq!(g.start_map.as_deref(), Some("MAP01"));
        assert_eq!(g.load, vec!["extra.wad"]);
    }
}
