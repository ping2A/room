//! Mod options for a settings screen: the CVars a mod declares in CVARINFO,
//! labelled and grouped the way its own MENUDEF option menus present them.
//!
//! A mod's `OptionMenu` blocks name every setting it wants players to see:
//! `Option "$BD_DECOR", "bd_disabledecorations", "OffOn"` gives the label
//! (a LANGUAGE string), the CVar and the value names; the `StaticText` lines
//! right after an item describe it. CVars no menu mentions are listed last,
//! under their own names, so nothing the mod declares is out of reach.

use crate::cvarinfo::{CvarDecl, CvarType};
use crate::language::Language;
use crate::menudef::{MenuBlockKind, MenuCmd, MenuDef};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// How a setting is edited.
#[derive(Clone, Debug, PartialEq)]
pub enum ModOptionKind {
    /// Pick one of named values (`Option`).
    Choice(Vec<(String, String)>),
    /// Number in a range (`Slider`, `NumberField`).
    Range {
        /// Lowest value.
        min: f64,
        /// Highest value.
        max: f64,
        /// Step.
        step: f64,
    },
    /// Free text or number (`TextField`, or a CVar no menu describes).
    Field,
    /// RGB colour (`ColorPicker`, `color` CVars).
    Color,
}

/// One setting.
#[derive(Clone, Debug, PartialEq)]
pub struct ModOption {
    /// CVar name as declared.
    pub cvar: String,
    /// Label shown to players.
    pub label: String,
    /// Description lines from the menu (may be empty).
    pub hint: String,
    /// Editor.
    pub kind: ModOptionKind,
    /// Declared type.
    pub ty: CvarType,
    /// Default value as text (`1` / `0` for bools).
    pub default: String,
}

/// Settings under one menu title.
#[derive(Clone, Debug, PartialEq)]
pub struct ModOptionGroup {
    /// Menu title.
    pub title: String,
    /// Settings in menu order.
    pub items: Vec<ModOption>,
    /// ACS scripts the menu runs to apply its settings (`Command "…",
    /// "pukename GetCVars always"`): `(name, always)`. Mods that copy CVars
    /// into script variables at level start need these after a change.
    pub apply_scripts: Vec<(String, bool)>,
}

/// `pukename NAME [always]` from a `Command` item (`;`-separated allowed).
fn puke_commands(cmd: &str) -> Vec<(String, bool)> {
    cmd.split(';')
        .filter_map(|part| {
            let mut words = part.split_whitespace();
            if !words.next()?.eq_ignore_ascii_case("pukename") {
                return None;
            }
            let name = words.next()?.trim_matches('"').to_owned();
            let always = words.next().is_some_and(|w| w.eq_ignore_ascii_case("always"));
            Some((name, always))
        })
        .collect()
}

fn collect_apply(body: &[MenuCmd], out: &mut Vec<(String, bool)>) {
    for cmd in body {
        match cmd {
            MenuCmd::Block { body, .. } => collect_apply(body, out),
            MenuCmd::Cmd { name, args } if name.eq_ignore_ascii_case("command") => {
                for p in args.get(1).map(|c| puke_commands(c)).unwrap_or_default() {
                    if !out.contains(&p) {
                        out.push(p);
                    }
                }
            }
            MenuCmd::Cmd { .. } => {}
        }
    }
}

/// Normalise a CVar default / value: bools become `1` / `0`.
#[must_use]
pub fn normalize_value(ty: CvarType, text: &str) -> String {
    let t = text.trim().trim_matches('"');
    match ty {
        CvarType::Bool => {
            let on = t.eq_ignore_ascii_case("true") || t.parse::<f64>().is_ok_and(|v| v != 0.0);
            if on { "1" } else { "0" }.to_owned()
        }
        _ => t.to_owned(),
    }
}

/// Drop GZDoom colour escapes (`\cX`, `\c[Name]`) and trim.
#[must_use]
pub fn clean_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' && it.peek() == Some(&'c') {
            it.next();
            if it.peek() == Some(&'[') {
                for d in it.by_ref() {
                    if d == ']' {
                        break;
                    }
                }
            } else {
                it.next();
            }
            continue;
        }
        if c == '\x1c' {
            // Raw colour escape byte + code.
            it.next();
            continue;
        }
        out.push(c);
    }
    out.replace("\\n", " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn resolve(lang: &Language, s: &str) -> String {
    if let Some(key) = s.strip_prefix('$') {
        if let Some(v) = lang.get("enu", key) {
            return clean_text(v);
        }
        return clean_text(key);
    }
    clean_text(s)
}

fn builtin_values(name: &str) -> Option<Vec<(String, String)>> {
    let pair = |a: &str, b: &str, c: &str, d: &str| {
        Some(vec![(a.to_owned(), b.to_owned()), (c.to_owned(), d.to_owned())])
    };
    match name.to_ascii_lowercase().as_str() {
        "onoff" => pair("0", "Off", "1", "On"),
        "offon" => pair("0", "On", "1", "Off"),
        "yesno" => pair("0", "No", "1", "Yes"),
        "noyes" => pair("0", "Yes", "1", "No"),
        _ => None,
    }
}

fn option_values(menu: &MenuDef, lang: &Language, name: &str) -> Vec<(String, String)> {
    let block = menu.blocks.iter().rev().find(|b| {
        matches!(b.kind, MenuBlockKind::OptionValue | MenuBlockKind::OptionString)
            && b.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name))
    });
    if let Some(block) = block {
        let mut out = Vec::new();
        for cmd in &block.body {
            if let MenuCmd::Cmd { name, args } = cmd {
                if name.is_empty() {
                    // `0, "No"` per line; a one-line body is one long row.
                    for pair in args.chunks_exact(2) {
                        out.push((pair[0].clone(), resolve(lang, &pair[1])));
                    }
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    builtin_values(name).unwrap_or_default()
}

fn num(args: &[String], i: usize, or: f64) -> f64 {
    args.get(i).and_then(|a| a.trim().parse::<f64>().ok()).unwrap_or(or)
}

fn walk(
    body: &[MenuCmd],
    ctx: &mut Walk<'_>,
    items: &mut Vec<ModOption>,
    hint_open: &mut bool,
) {
    for cmd in body {
        match cmd {
            MenuCmd::Block { body, .. } => walk(body, ctx, items, hint_open),
            MenuCmd::Cmd { name, args } => {
                let lname = name.to_ascii_lowercase();
                if lname == "statictext" {
                    let text = args.first().map(|a| resolve(ctx.lang, a)).unwrap_or_default();
                    if text.is_empty() {
                        *hint_open = false;
                    } else if *hint_open {
                        if let Some(last) = items.last_mut() {
                            if last.hint.len() < 400 {
                                if !last.hint.is_empty() {
                                    last.hint.push(' ');
                                }
                                last.hint.push_str(&text);
                            }
                        }
                    }
                    continue;
                }
                *hint_open = false;
                let kind_of = |n: &str| match n {
                    "option" => Some(0),
                    "slider" | "scaleslider" | "numberfield" => Some(1),
                    "textfield" => Some(2),
                    "colorpicker" => Some(3),
                    _ => None,
                };
                let Some(which) = kind_of(lname.as_str()) else {
                    continue;
                };
                let (Some(label), Some(cvar)) = (args.first(), args.get(1)) else {
                    continue;
                };
                let key = cvar.to_ascii_lowercase();
                let Some(decl) = ctx.decls.iter().find(|d| d.name.eq_ignore_ascii_case(cvar)) else {
                    continue;
                };
                if !ctx.seen.insert(key) {
                    continue;
                }
                let kind = match which {
                    0 => {
                        let values = args
                            .get(2)
                            .map(|v| option_values(ctx.menu, ctx.lang, v))
                            .unwrap_or_default();
                        if values.is_empty() {
                            ModOptionKind::Field
                        } else {
                            ModOptionKind::Choice(values)
                        }
                    }
                    1 => ModOptionKind::Range {
                        min: num(args, 2, 0.0),
                        max: num(args, 3, 100.0),
                        step: num(args, 4, 1.0),
                    },
                    3 => ModOptionKind::Color,
                    _ => ModOptionKind::Field,
                };
                items.push(ModOption {
                    cvar: decl.name.clone(),
                    label: resolve(ctx.lang, label),
                    hint: String::new(),
                    kind,
                    ty: decl.ty,
                    default: normalize_value(decl.ty, decl.default.as_deref().unwrap_or("0")),
                });
                *hint_open = true;
            }
        }
    }
}

struct Walk<'a> {
    decls: &'a [CvarDecl],
    menu: &'a MenuDef,
    lang: &'a Language,
    seen: BTreeSet<String>,
}

/// Settings for every CVar in `decls`: grouped by the option menu that
/// shows it (menu order), then the rest under "Other settings".
#[must_use]
pub fn mod_options(decls: &[CvarDecl], menu: &MenuDef, lang: &Language) -> Vec<ModOptionGroup> {
    let mut ctx = Walk {
        decls,
        menu,
        lang,
        seen: BTreeSet::new(),
    };
    let mut groups = Vec::new();
    for block in &menu.blocks {
        if block.kind != MenuBlockKind::OptionMenu {
            continue;
        }
        let title = block
            .body
            .iter()
            .find_map(|c| match c {
                MenuCmd::Cmd { name, args } if name.eq_ignore_ascii_case("title") => {
                    args.first().map(|t| resolve(lang, t))
                }
                _ => None,
            })
            .filter(|t| !t.is_empty())
            .or_else(|| block.name.clone())
            .unwrap_or_else(|| "Options".to_owned());
        let mut items = Vec::new();
        let mut hint_open = false;
        walk(&block.body, &mut ctx, &mut items, &mut hint_open);
        if !items.is_empty() {
            let mut apply_scripts = Vec::new();
            collect_apply(&block.body, &mut apply_scripts);
            groups.push(ModOptionGroup {
                title,
                items,
                apply_scripts,
            });
        }
    }
    let mut rest = Vec::new();
    for d in decls {
        if d.flags.cheat || ctx.seen.contains(&d.name.to_ascii_lowercase()) {
            continue;
        }
        let kind = match d.ty {
            CvarType::Bool => ModOptionKind::Choice(vec![
                ("0".to_owned(), "Off".to_owned()),
                ("1".to_owned(), "On".to_owned()),
            ]),
            CvarType::Color => ModOptionKind::Color,
            _ => ModOptionKind::Field,
        };
        rest.push(ModOption {
            cvar: d.name.clone(),
            label: d.name.clone(),
            hint: String::new(),
            kind,
            ty: d.ty,
            default: normalize_value(d.ty, d.default.as_deref().unwrap_or("0")),
        });
    }
    if !rest.is_empty() {
        groups.push(ModOptionGroup {
            title: "Other settings".to_owned(),
            items: rest,
            apply_scripts: Vec::new(),
        });
    }
    groups
}

/// JSON for a settings UI (`[{title, items: [{cvar, label, hint, type,
/// default, kind, values | min/max/step}]}]`).
#[must_use]
pub fn mod_options_json(groups: &[ModOptionGroup]) -> Value {
    let ty = |t: CvarType| match t {
        CvarType::Int => "int",
        CvarType::Float => "float",
        CvarType::Bool => "bool",
        CvarType::Color => "color",
        CvarType::String => "string",
    };
    Value::Array(
        groups
            .iter()
            .map(|g| {
                json!({
                    "title": g.title,
                    "items": g.items.iter().map(|o| {
                        let mut v = json!({
                            "cvar": o.cvar,
                            "label": o.label,
                            "hint": o.hint,
                            "type": ty(o.ty),
                            "default": o.default,
                        });
                        match &o.kind {
                            ModOptionKind::Choice(values) => {
                                v["kind"] = json!("choice");
                                v["values"] = json!(values.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>());
                            }
                            ModOptionKind::Range { min, max, step } => {
                                v["kind"] = json!("range");
                                v["min"] = json!(min);
                                v["max"] = json!(max);
                                v["step"] = json!(step);
                            }
                            ModOptionKind::Field => v["kind"] = json!("field"),
                            ModOptionKind::Color => v["kind"] = json!("color"),
                        }
                        v
                    }).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cvarinfo::parse_cvarinfo;
    use crate::language::parse_language;
    use crate::menudef::parse_menudef;

    #[test]
    fn menu_labels_values_and_hints() {
        let cvars = parse_cvarinfo(
            "server int bd_disabledecorations = 0;\nuser bool bd_gore = true;\nserver float bd_speed = 1.5;\n",
        )
        .unwrap();
        let menu = parse_menudef(
            r#"
OptionValue "OffOn" { 0, "On" 1, "Off" }
OptionMenu "BDLevel"
{
    Title "$BD_LEVEL"
    Command "Apply", "pukename GetCVars always"
    Option "$BD_DECOR", "bd_disabledecorations", "OffOn"
    StaticText "$BD_DECOR_TEXT", 1
    StaticText " "
    Slider "Speed", "bd_speed", 0.5, 3, 0.25
}
"#,
        )
        .unwrap();
        let lang = parse_language(
            "[enu default]\nBD_LEVEL = \"Level options\";\nBD_DECOR = \"\\cdMap decorations\";\nBD_DECOR_TEXT = \"Flares and glows around items.\";\n",
        )
        .unwrap();
        let groups = mod_options(&cvars, &menu, &lang);
        assert_eq!(groups.len(), 2, "{groups:?}");
        assert_eq!(groups[0].title, "Level options");
        assert_eq!(groups[0].apply_scripts, vec![("GetCVars".to_owned(), true)]);
        let decor = &groups[0].items[0];
        assert_eq!(decor.label, "Map decorations");
        assert_eq!(decor.hint, "Flares and glows around items.");
        assert_eq!(
            decor.kind,
            ModOptionKind::Choice(vec![("0".into(), "On".into()), ("1".into(), "Off".into())])
        );
        assert!(matches!(groups[0].items[1].kind, ModOptionKind::Range { step, .. } if (step - 0.25).abs() < 1e-9));
        // Not in a menu: listed under its own name, bool default normalised.
        assert_eq!(groups[1].items[0].cvar, "bd_gore");
        assert_eq!(groups[1].items[0].default, "1");
        let json = mod_options_json(&groups);
        assert_eq!(json[0]["items"][0]["values"][1][1], "Off");
    }
}
