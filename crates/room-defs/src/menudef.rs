//! MENUDEF parser (M7 bootstrap — structure + nested `IfGame`/`IfOption`).

use crate::error::DefsError;
use room_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// Kind of top-level MENUDEF block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuBlockKind {
    /// `ListMenu` / `DefaultListMenu` / `AddListMenu`.
    ListMenu,
    /// `OptionMenu` / `DefaultOptionMenu` / `AddOptionMenu`.
    OptionMenu,
    /// `OptionValue`.
    OptionValue,
    /// `OptionString`.
    OptionString,
    /// `OptionMenuSettings`.
    OptionMenuSettings,
    /// `ImageScroller`.
    ImageScroller,
    /// Unrecognised top-level keyword.
    Other(String),
}

/// One top-level menu / value block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuBlock {
    /// Block kind.
    pub kind: MenuBlockKind,
    /// Optional name (`"MainMenu"`, …). Defaults omit the name.
    pub name: Option<String>,
    /// Extra header flags (`protected`, …).
    pub flags: Vec<String>,
    /// Body commands.
    pub body: Vec<MenuCmd>,
}

impl Default for MenuBlockKind {
    fn default() -> Self {
        Self::Other(String::new())
    }
}

/// One MENUDEF body command (or nested conditional).
#[derive(Clone, Debug, PartialEq)]
pub enum MenuCmd {
    /// Nested `IfGame(…)` / `IfOption(…)` / bare `{…}` group.
    Block {
        /// Keyword (`IfGame`, `IfOption`, …); empty for anonymous.
        kind: String,
        /// Header args before `{`.
        args: Vec<String>,
        /// Nested body.
        body: Vec<MenuCmd>,
    },
    /// Flat command: name + argument tokens until `;` or next keyword/`{`/`}`.
    Cmd {
        /// Command name (`StaticPatch`, `Option`, `Title`, …).
        name: String,
        /// Arguments.
        args: Vec<String>,
    },
}

/// Compiled MENUDEF registry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuDef {
    /// All top-level blocks in source order.
    pub blocks: Vec<MenuBlock>,
    /// Blocks indexed by name (last wins).
    pub by_name: BTreeMap<String, usize>,
}

/// Parse a MENUDEF lump (C-mode scanner).
pub fn parse_menudef(text: &str) -> Result<MenuDef, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            semicolon_comments: false,
        },
    );
    let mut info = MenuDef::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(kw) => {
                let key = kw.to_ascii_lowercase();
                let kind = match key.as_str() {
                    "listmenu" | "defaultlistmenu" | "addlistmenu" => MenuBlockKind::ListMenu,
                    "optionmenu" | "defaultoptionmenu" | "addoptionmenu" => {
                        MenuBlockKind::OptionMenu
                    }
                    "optionvalue" => MenuBlockKind::OptionValue,
                    "optionstring" => MenuBlockKind::OptionString,
                    "optionmenusettings" => MenuBlockKind::OptionMenuSettings,
                    "imagescroller" => MenuBlockKind::ImageScroller,
                    _ => MenuBlockKind::Other(kw.to_owned()),
                };
                let mut block = MenuBlock {
                    kind,
                    ..Default::default()
                };
                // Optional name + flags before `{`.
                loop {
                    match sc.next_token()? {
                        Tok::Punct(Punct::LBrace) => break,
                        Tok::Eof => {
                            return Err(DefsError::Message(
                                "MENUDEF: unexpected EOF before `{`".into(),
                            ));
                        }
                        Tok::String(s) => {
                            if block.name.is_none() {
                                block.name = Some(s.into_owned());
                            } else {
                                block.flags.push(s.into_owned());
                            }
                        }
                        Tok::Ident(s) => {
                            if block.name.is_none()
                                && !s.eq_ignore_ascii_case("protected")
                                && matches!(
                                    block.kind,
                                    MenuBlockKind::OptionValue
                                        | MenuBlockKind::OptionString
                                        | MenuBlockKind::Other(_)
                                )
                            {
                                // `OptionValue AutoOffOn` — unquoted name.
                                block.name = Some(s.to_owned());
                            } else {
                                block.flags.push(s.to_owned());
                            }
                        }
                        Tok::Int(n) => block.flags.push(n.to_string()),
                        Tok::Float(f) => block.flags.push(f.to_string()),
                        Tok::Punct(Punct::Comma) => {}
                        other => {
                            return Err(DefsError::Message(format!(
                                "MENUDEF: unexpected {other:?} in block header"
                            )));
                        }
                    }
                }
                block.body = parse_body(&mut sc)?;
                if let Some(ref name) = block.name {
                    info.by_name.insert(name.clone(), info.blocks.len());
                }
                info.blocks.push(block);
            }
            other => {
                return Err(DefsError::Message(format!("MENUDEF: unexpected {other:?}")));
            }
        }
    }
    Ok(info)
}

fn parse_body(sc: &mut Scanner<'_>) -> Result<Vec<MenuCmd>, DefsError> {
    let mut body = Vec::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) | Tok::Punct(Punct::Comma) => {}
            Tok::Ident(kw) => {
                let lower = kw.to_ascii_lowercase();
                if lower == "ifgame" || lower == "ifoption" {
                    let args = parse_paren_args(sc)?;
                    expect_lbrace(sc)?;
                    let nested = parse_body(sc)?;
                    body.push(MenuCmd::Block {
                        kind: kw.to_owned(),
                        args,
                        body: nested,
                    });
                } else if sc.check_punct(Punct::LBrace)? {
                    let nested = parse_body(sc)?;
                    body.push(MenuCmd::Block {
                        kind: kw.to_owned(),
                        args: Vec::new(),
                        body: nested,
                    });
                } else {
                    let args = parse_cmd_args(sc)?;
                    body.push(MenuCmd::Cmd {
                        name: kw.to_owned(),
                        args,
                    });
                }
            }
            // OptionValue rows: `0, "No"` / `"cursor", "Cursor"` / `-1, "Off"`.
            Tok::Int(n) => {
                let mut args = vec![n.to_string()];
                args.extend(parse_cmd_args(sc)?);
                body.push(MenuCmd::Cmd {
                    name: String::new(),
                    args,
                });
            }
            Tok::Float(f) => {
                let mut args = vec![f.to_string()];
                args.extend(parse_cmd_args(sc)?);
                body.push(MenuCmd::Cmd {
                    name: String::new(),
                    args,
                });
            }
            Tok::String(s) => {
                let mut args = vec![s.into_owned()];
                args.extend(parse_cmd_args(sc)?);
                body.push(MenuCmd::Cmd {
                    name: String::new(),
                    args,
                });
            }
            Tok::Punct(Punct::Minus) => {
                let mut args = vec![read_signed(sc, true)?];
                args.extend(parse_cmd_args(sc)?);
                body.push(MenuCmd::Cmd {
                    name: String::new(),
                    args,
                });
            }
            Tok::Punct(Punct::Plus) => {
                let mut args = vec![read_signed(sc, false)?];
                args.extend(parse_cmd_args(sc)?);
                body.push(MenuCmd::Cmd {
                    name: String::new(),
                    args,
                });
            }
            other => {
                return Err(DefsError::Message(format!(
                    "MENUDEF body: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(body)
}

fn parse_paren_args(sc: &mut Scanner<'_>) -> Result<Vec<String>, DefsError> {
    match sc.next_token()? {
        Tok::Punct(Punct::LParen) => {}
        other => {
            // `IfGame` without parens is invalid; allow bare form.
            sc.unget(other);
            return Ok(Vec::new());
        }
    }
    let mut args = Vec::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RParen) | Tok::Eof => break,
            Tok::Punct(Punct::Comma) => {}
            Tok::Ident(s) => args.push(s.to_owned()),
            Tok::String(s) => args.push(s.into_owned()),
            Tok::Int(n) => args.push(n.to_string()),
            Tok::Float(f) => args.push(f.to_string()),
            other => {
                return Err(DefsError::Message(format!(
                    "MENUDEF If*: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(args)
}

fn parse_cmd_args(sc: &mut Scanner<'_>) -> Result<Vec<String>, DefsError> {
    let mut args = Vec::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::Semi) => break,
            Tok::Punct(Punct::RBrace) => {
                sc.unget(Tok::Punct(Punct::RBrace));
                break;
            }
            Tok::Punct(Punct::LBrace) => {
                sc.unget(Tok::Punct(Punct::LBrace));
                break;
            }
            Tok::Eof => break,
            Tok::Punct(Punct::Comma) => {}
            Tok::Ident(s) => {
                // Next command keyword at start of a "line" — heuristic: if we
                // already have args and this looks like a new statement keyword,
                // unget. Simpler: never unget idents; MENUDEF commands are
                // one-per-line and end at newline… but scanner ignores newlines.
                // So commands run until `;` or `}` or `{`.
                // Without `;`, the next Ident is the next command.
                if !args.is_empty() {
                    sc.unget(Tok::Ident(s));
                    break;
                }
                args.push(s.to_owned());
            }
            Tok::String(s) => args.push(s.into_owned()),
            Tok::Int(n) => args.push(n.to_string()),
            Tok::Float(f) => args.push(f.to_string()),
            Tok::Punct(Punct::Minus) => args.push(read_signed(sc, true)?),
            Tok::Punct(Punct::Plus) => args.push(read_signed(sc, false)?),
            Tok::Punct(p) => args.push(format!("{p:?}")),
        }
    }
    Ok(args)
}

fn read_signed(sc: &mut Scanner<'_>, negative: bool) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Int(n) => Ok(if negative {
            (-n).to_string()
        } else {
            n.to_string()
        }),
        Tok::Float(f) => Ok(if negative {
            (-f).to_string()
        } else {
            f.to_string()
        }),
        other => Err(DefsError::Message(format!(
            "MENUDEF: expected number after sign, got {other:?}"
        ))),
    }
}

fn expect_lbrace(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    match sc.next_token()? {
        Tok::Punct(Punct::LBrace) => Ok(()),
        other => Err(DefsError::Message(format!(
            "MENUDEF: expected `{{`, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_listmenu_and_optionvalue() {
        let text = r#"
DefaultListMenu
{
	Font "BigFont", "Untranslated"
	LineSpacing 20
	IfGame(Doom)
	{
		Selector "M_SKULL1", -32, -5
	}
}
ListMenu "MainMenu"
{
	TextItem "$MNU_NEWGAME", "n", "PlayerclassMenu"
}
OptionValue "YesNo"
{
	0, "No"
	1, "Yes"
}
OptionMenu "OptionsMenu" protected
{
	Title "Options"
	Submenu "Input", "InputMenu"
}
"#;
        let info = parse_menudef(text).unwrap();
        assert_eq!(info.blocks.len(), 4);
        assert!(info.by_name.contains_key("MainMenu"));
        assert!(info.by_name.contains_key("YesNo"));
        let main = &info.blocks[*info.by_name.get("MainMenu").unwrap()];
        assert!(matches!(
            &main.body[0],
            MenuCmd::Cmd { name, args } if name == "TextItem" && args.len() == 3
        ));
        let def = &info.blocks[0];
        assert!(matches!(
            &def.body[2],
            MenuCmd::Block { kind, args, body }
                if kind.eq_ignore_ascii_case("IfGame")
                    && args.as_slice() == ["Doom"]
                    && !body.is_empty()
        ));
    }

    #[test]
    fn parses_gzdoom_menudef_fixture() {
        // GZDoom `wadsrc/static/menudef.txt` (GPL-3.0-or-later).
        let text = include_str!("../../../testdata/menudef/gzdoom_menudef.txt");
        let info = parse_menudef(text).unwrap();
        assert!(info.by_name.contains_key("MainMenu"));
        assert!(info.by_name.contains_key("OptionsMenu"));
        assert!(info.blocks.len() > 50);
    }
}
