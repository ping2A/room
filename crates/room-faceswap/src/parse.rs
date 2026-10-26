//! `HEADANCHORS` text lump parser (`docs/26` §4.2).

use crate::anchor::{HeadAnchor, HeadYaw, Visibility};
use room_sc::{Punct, ScanError, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// One parsed anchor entry (lump name + content hash + placement).
#[derive(Clone, Debug, PartialEq)]
pub struct AnchorEntry {
    /// Lump short name (e.g. `TROOA1`).
    pub lump: String,
    /// Expected pixel hash (`0` = accept any / estimator fill-in).
    pub hash: u64,
    /// Placement.
    pub anchor: HeadAnchor,
}

/// Optional class-level overrides.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassOverride {
    /// Class name (DECORATE / Duke).
    pub name: String,
    /// Lumps to skip for this class.
    pub exclude: Vec<String>,
}

/// Parsed `HEADANCHORS` file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeadAnchorsFile {
    /// Anchor records keyed by uppercase lump name.
    pub anchors: BTreeMap<String, AnchorEntry>,
    /// Class overrides.
    pub classes: Vec<ClassOverride>,
    /// Unknown keywords encountered (warn, do not fail).
    pub warnings: Vec<String>,
}

/// Parse errors (scanner / structure).
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// Scanner failure.
    #[error("HEADANCHORS scan: {0}")]
    Scan(#[from] ScanError),
    /// Missing required field / bad structure.
    #[error("HEADANCHORS: {0}")]
    Msg(String),
}

/// Parse a `HEADANCHORS` text lump. Unknown keywords are collected as warnings.
pub fn parse_headanchors(text: &str) -> Result<HeadAnchorsFile, ParseError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            semicolon_comments: false,
        },
    );
    let mut out = HeadAnchorsFile::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(kw) => {
                let key = kw.to_ascii_lowercase();
                match key.as_str() {
                    "anchor" => {
                        let entry = parse_anchor(&mut sc, &mut out.warnings)?;
                        out.anchors.insert(entry.lump.to_ascii_uppercase(), entry);
                    }
                    "class" => {
                        out.classes.push(parse_class(&mut sc, &mut out.warnings)?);
                    }
                    other => {
                        out.warnings
                            .push(format!("unknown top-level keyword `{other}`"));
                        skip_unknown_construct(&mut sc)?;
                    }
                }
            }
            other => {
                out.warnings
                    .push(format!("ignored token at top level: {other:?}"));
            }
        }
    }
    Ok(out)
}

fn parse_anchor(
    sc: &mut Scanner<'_>,
    warnings: &mut Vec<String>,
) -> Result<AnchorEntry, ParseError> {
    let lump = sc.must_get_string()?.into_owned();
    let mut hash = 0u64;
    // Optional: `hash <value>` before `{`.
    let tok = sc.next_token()?;
    match tok {
        Tok::Ident(kw) if Scanner::compare(kw, "hash") => {
            hash = parse_hash_value(sc)?;
            expect_punct(sc, Punct::LBrace)?;
        }
        Tok::Punct(Punct::LBrace) => {}
        other => {
            sc.unget(other);
            expect_punct(sc, Punct::LBrace)?;
        }
    }
    let mut center = (0.0f32, 0.0f32);
    let mut radius = (8.0f32, 8.0f32);
    let mut roll = 0.0f32;
    let mut yaw = HeadYaw::Front;
    let mut visible = Visibility::Full;
    let mut keep_top = 0.0f32;
    let mut hidden = false;
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) => break,
            Tok::Eof => {
                return Err(ParseError::Msg("unclosed Anchor block".into()));
            }
            Tok::Ident(kw) => {
                let key = kw.to_ascii_lowercase();
                match key.as_str() {
                    "center" => {
                        center.0 = must_float(sc)?;
                        let _ = sc.check_punct(Punct::Comma)?;
                        center.1 = must_float(sc)?;
                    }
                    "radius" => {
                        radius.0 = must_float(sc)?;
                        let _ = sc.check_punct(Punct::Comma)?;
                        radius.1 = must_float(sc)?;
                    }
                    "roll" => roll = must_float(sc)?,
                    "yaw" => yaw = parse_yaw(sc)?,
                    "keeptop" => keep_top = must_float(sc)?,
                    "hidden" => {
                        hidden = true;
                        visible = Visibility::Hidden;
                    }
                    "visible" => {
                        let v = sc.must_get_ident()?;
                        if Scanner::compare(v, "hidden") {
                            hidden = true;
                            visible = Visibility::Hidden;
                        } else if Scanner::compare(v, "full") {
                            visible = Visibility::Full;
                        } else if Scanner::compare(v, "partial") {
                            visible = Visibility::Partial;
                        } else {
                            warnings.push(format!("unknown Visible `{v}`"));
                        }
                    }
                    other => {
                        warnings.push(format!("unknown Anchor field `{other}`"));
                        skip_value(sc)?;
                    }
                }
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                warnings.push(format!("ignored in Anchor: {other:?}"));
            }
        }
    }
    let anchor = if hidden {
        HeadAnchor {
            center,
            radius,
            roll,
            yaw,
            visible: Visibility::Hidden,
            keep_top,
        }
    } else {
        HeadAnchor {
            center,
            radius,
            roll,
            yaw,
            visible,
            keep_top: keep_top.clamp(0.0, 1.0),
        }
    };
    Ok(AnchorEntry { lump, hash, anchor })
}

fn parse_class(
    sc: &mut Scanner<'_>,
    warnings: &mut Vec<String>,
) -> Result<ClassOverride, ParseError> {
    let name = sc.must_get_string()?.into_owned();
    expect_punct(sc, Punct::LBrace)?;
    let mut exclude = Vec::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) => break,
            Tok::Eof => return Err(ParseError::Msg("unclosed Class block".into())),
            Tok::Ident(kw) => {
                if Scanner::compare(kw, "exclude") {
                    exclude.push(sc.must_get_string()?.into_owned());
                } else {
                    warnings.push(format!("unknown Class field `{kw}`"));
                    skip_value(sc)?;
                }
            }
            Tok::Punct(Punct::Semi) => {}
            other => warnings.push(format!("ignored in Class: {other:?}")),
        }
    }
    Ok(ClassOverride { name, exclude })
}

fn parse_yaw(sc: &mut Scanner<'_>) -> Result<HeadYaw, ParseError> {
    let id = sc.must_get_ident()?;
    Ok(match id.to_ascii_lowercase().as_str() {
        "front" => HeadYaw::Front,
        "threequarterleft" | "tqleft" => HeadYaw::ThreeQuarterLeft,
        "threequarterright" | "tqright" => HeadYaw::ThreeQuarterRight,
        "profileleft" => HeadYaw::ProfileLeft,
        "profileright" => HeadYaw::ProfileRight,
        "back" => HeadYaw::Back,
        other => {
            return Err(ParseError::Msg(format!("unknown Yaw `{other}`")));
        }
    })
}

fn parse_hash_value(sc: &mut Scanner<'_>) -> Result<u64, ParseError> {
    // Full u64 hex (e.g. 0x9f3c12aa77e01b42) overflows the scanner's `i64`
    // number lexer — consume hex from `remaining()` instead.
    let rem = sc.remaining();
    let trimmed = rem.trim_start();
    let ws = rem.len() - trimmed.len();
    if let Some(rest) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        let nhex = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
        if nhex > 0 {
            let hex = core::str::from_utf8(&rest.as_bytes()[..nhex])
                .map_err(|_| ParseError::Msg("bad hash utf8".into()))?;
            let v = u64::from_str_radix(hex, 16)
                .map_err(|_| ParseError::Msg(format!("bad hash 0x{hex}")))?;
            sc.bump_bytes(ws + 2 + nhex);
            return Ok(v);
        }
    }
    match sc.next_token()? {
        Tok::Int(n) => Ok(n as u64),
        Tok::Ident(s) => parse_hex_u64(s),
        Tok::String(s) => parse_hex_u64(s.as_ref()),
        _ => Err(ParseError::Msg("expected hash value".into())),
    }
}

fn parse_hex_u64(s: &str) -> Result<u64, ParseError> {
    let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(t, 16).map_err(|_| ParseError::Msg(format!("bad hash `{s}`")))
}

fn must_float(sc: &mut Scanner<'_>) -> Result<f32, ParseError> {
    match sc.next_token()? {
        Tok::Int(n) => Ok(n as f32),
        Tok::Float(f) => Ok(f as f32),
        _ => Err(ParseError::Msg("expected number".into())),
    }
}

fn expect_punct(sc: &mut Scanner<'_>, p: Punct) -> Result<(), ParseError> {
    match sc.next_token()? {
        Tok::Punct(q) if q == p => Ok(()),
        _ => Err(ParseError::Msg(format!("expected {p:?}"))),
    }
}

fn skip_value(sc: &mut Scanner<'_>) -> Result<(), ParseError> {
    match sc.next_token()? {
        Tok::Punct(Punct::LBrace) => {
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
        Tok::Eof => Ok(()),
        _ => {
            // swallow one more if comma-separated pair
            let _ = sc.check_punct(Punct::Comma)?;
            if matches!(
                sc.next_token().ok(),
                Some(Tok::Int(_)) | Some(Tok::Float(_))
            ) {
                // consumed second number
            }
            Ok(())
        }
    }
}

fn skip_unknown_construct(sc: &mut Scanner<'_>) -> Result<(), ParseError> {
    // Skip optional string/ident then a brace block or a single value.
    let _ = sc.next_token()?;
    skip_value(sc)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_doc_example() {
        let text = r#"
// HEADANCHORS
Anchor "TROOA1" hash 0x9f3c12aa77e01b42
{
    Center 21, 9
    Radius 7, 8
    Roll 0
    Yaw Front
    KeepTop 0.25
}
Anchor "TROOI0" hash 0x1
{
    Hidden
}
Class "DoomImp" { Exclude "TROOU0" }
"#;
        let f = parse_headanchors(text).unwrap();
        assert_eq!(f.anchors.len(), 2);
        let a = f.anchors.get("TROOA1").unwrap();
        assert_eq!(a.anchor.center, (21.0, 9.0));
        assert_eq!(a.anchor.keep_top, 0.25);
        assert!(matches!(
            f.anchors.get("TROOI0").unwrap().anchor.visible,
            Visibility::Hidden
        ));
        assert_eq!(f.classes[0].exclude, vec!["TROOU0".to_string()]);
    }

    #[test]
    fn unknown_keyword_warns() {
        let f = parse_headanchors(r#"Foo "x" { Bar 1 } Anchor "A" { Hidden }"#).unwrap();
        assert!(!f.warnings.is_empty());
        assert!(f.anchors.contains_key("A"));
    }
}
