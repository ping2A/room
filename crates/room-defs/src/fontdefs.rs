//! FONTDEFS parser.

use crate::error::DefsError;
use room_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// One font definition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontDef {
    /// Font name.
    pub name: String,
    /// Template like `STCFN%03d`.
    pub template: Option<String>,
    /// First character code.
    pub first: Option<i64>,
    /// Space width.
    pub space_width: Option<i64>,
    /// Explicit character → lump map.
    pub chars: BTreeMap<String, String>,
}

/// Parse FONTDEFS.
pub fn parse_fontdefs(text: &str) -> Result<BTreeMap<String, FontDef>, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            ..Default::default()
        },
    );
    let mut fonts = BTreeMap::new();

    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Ident(name) => {
                let mut def = FontDef {
                    name: name.to_owned(),
                    ..Default::default()
                };
                if !sc.check_punct(Punct::LBrace)? {
                    return Err(DefsError::Message(
                        "FONTDEFS: expected `{` after font name".into(),
                    ));
                }
                loop {
                    match sc.next_token()? {
                        Tok::Punct(Punct::RBrace) | Tok::Eof => break,
                        Tok::Ident(k) => {
                            let key = k.to_ascii_lowercase();
                            match key.as_str() {
                                "template" => {
                                    def.template = Some(read_template_raw(&mut sc)?);
                                }
                                "first" => {
                                    let _ = sc.check_punct(Punct::Eq)?;
                                    def.first = Some(sc.must_get_number()?);
                                    let _ = sc.check_punct(Punct::Semi)?;
                                }
                                "spacewidth" => {
                                    let _ = sc.check_punct(Punct::Eq)?;
                                    def.space_width = Some(sc.must_get_number()?);
                                    let _ = sc.check_punct(Punct::Semi)?;
                                }
                                _ => {
                                    let _ = sc.check_punct(Punct::Eq)?;
                                    let lump = read_value(&mut sc)?;
                                    def.chars.insert(key, lump);
                                    let _ = sc.check_punct(Punct::Semi)?;
                                }
                            }
                        }
                        Tok::String(ch) => {
                            let _ = sc.check_punct(Punct::Eq)?;
                            let lump = read_value(&mut sc)?;
                            def.chars.insert(ch.into_owned(), lump);
                            let _ = sc.check_punct(Punct::Semi)?;
                        }
                        Tok::Punct(Punct::Semi) => {}
                        other => {
                            return Err(DefsError::Message(format!(
                                "FONTDEFS: unexpected {other:?}"
                            )));
                        }
                    }
                }
                fonts.insert(def.name.to_ascii_lowercase(), def);
            }
            _ => {}
        }
    }
    Ok(fonts)
}

fn read_value(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Int(n) => Ok(n.to_string()),
        other => Err(DefsError::Message(format!(
            "FONTDEFS: expected value, got {other:?}"
        ))),
    }
}

/// Read a template token that may contain `%` (not valid as a single C-mode token).
///
/// Important: do not call `check_punct` first — that would tokenize `STCFN` and
/// leave `%03d` as the raw remainder.
fn read_template_raw(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    let rem = sc.remaining();
    let mut trimmed = rem.trim_start();
    let mut skipped = rem.len() - trimmed.len();
    if let Some(rest) = trimmed.strip_prefix('=') {
        trimmed = rest.trim_start();
        skipped = rem.len() - trimmed.len();
    }
    sc.bump_bytes(skipped);
    if trimmed.starts_with('"') {
        return Ok(sc.must_get_string()?.into_owned());
    }
    let end = trimmed
        .find(|c: char| c.is_whitespace() || c == ';' || c == '}')
        .unwrap_or(trimmed.len());
    let value = trimmed[..end].to_owned();
    sc.bump_bytes(end);
    // Optional trailing `;` via raw remainder (avoid tokenizing the next key).
    let rem = sc.remaining();
    let t = rem.trim_start();
    if let Some(rest) = t.strip_prefix(';') {
        let skip = rem.len() - rest.len();
        sc.bump_bytes(skip);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parse_font() {
        let text = r#"
SMALLFONT
{
  Template STCFN%03d
  First 33
  SpaceWidth 4
}
"#;
        let fonts = parse_fontdefs(text).unwrap();
        let f = fonts.get("smallfont").unwrap();
        assert_eq!(f.template.as_deref(), Some("STCFN%03d"));
        assert_eq!(f.first, Some(33));
    }
}
