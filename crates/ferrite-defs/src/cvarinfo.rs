//! CVARINFO parser.

use crate::error::DefsError;
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};

/// CVar type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CvarType {
    /// Integer.
    Int,
    /// Float.
    Float,
    /// Bool.
    Bool,
    /// Color.
    Color,
    /// String.
    String,
}

/// CVar scope flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CvarFlags {
    /// Server CVar.
    pub server: bool,
    /// User CVar.
    pub user: bool,
    /// No save.
    pub nosave: bool,
    /// No archive.
    pub noarchive: bool,
    /// Cheat.
    pub cheat: bool,
    /// Latch.
    pub latch: bool,
}

/// One CVARINFO declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CvarDecl {
    /// Flags.
    pub flags: CvarFlags,
    /// Type.
    pub ty: CvarType,
    /// Name.
    pub name: String,
    /// Default as text.
    pub default: Option<String>,
}

/// Parse CVARINFO.
pub fn parse_cvarinfo(text: &str) -> Result<Vec<CvarDecl>, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            ..Default::default()
        },
    );
    let mut out = Vec::new();

    loop {
        let mut flags = CvarFlags::default();
        let ty = loop {
            match sc.next_token()? {
                Tok::Eof => return Ok(out),
                Tok::Ident(s) => {
                    let l = s.to_ascii_lowercase();
                    match l.as_str() {
                        "server" => flags.server = true,
                        "user" => flags.user = true,
                        "nosave" => flags.nosave = true,
                        "noarchive" => flags.noarchive = true,
                        "cheat" => flags.cheat = true,
                        "latch" => flags.latch = true,
                        "int" => break CvarType::Int,
                        "float" => break CvarType::Float,
                        "bool" => break CvarType::Bool,
                        "color" => break CvarType::Color,
                        "string" => break CvarType::String,
                        "handlerclass" => {
                            let _ = sc.check_punct(Punct::LParen)?;
                            while !matches!(sc.next_token()?, Tok::Punct(Punct::RParen) | Tok::Eof)
                            {
                            }
                        }
                        other => {
                            return Err(DefsError::Message(format!(
                                "CVARINFO: unexpected {other}"
                            )));
                        }
                    }
                }
                Tok::Punct(Punct::Semi) => {}
                other => {
                    return Err(DefsError::Message(format!(
                        "CVARINFO: unexpected {other:?}"
                    )));
                }
            }
        };
        let name = match sc.next_token()? {
            Tok::Ident(s) => s.to_owned(),
            other => {
                return Err(DefsError::Message(format!(
                    "CVARINFO: expected name, got {other:?}"
                )));
            }
        };
        let mut default = None;
        if sc.check_punct(Punct::Eq)? {
            default = Some(match sc.next_token()? {
                Tok::Ident(s) => s.to_owned(),
                Tok::String(s) => s.into_owned(),
                Tok::Int(n) => n.to_string(),
                Tok::Float(f) => f.to_string(),
                other => {
                    return Err(DefsError::Message(format!(
                        "CVARINFO: bad default {other:?}"
                    )));
                }
            });
        }
        let _ = sc.check_punct(Punct::Semi)?;
        out.push(CvarDecl {
            flags,
            ty,
            name,
            default,
        });
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parse_decls() {
        let text = r#"
server int sv_cheats = 0;
user bool cl_run = true;
noarchive float mousesens = 1.5;
"#;
        let v = parse_cvarinfo(text).unwrap();
        assert_eq!(v.len(), 3);
        assert!(v[0].flags.server);
        assert_eq!(v[0].name, "sv_cheats");
        assert_eq!(v[1].ty, CvarType::Bool);
        assert_eq!(v[2].default.as_deref(), Some("1.5"));
    }
}
