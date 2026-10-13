//! Config file load/save (`ferrite.ini`) and gzdoom.ini import (M13).

use crate::cvar::{CvarFlags, CvarStore, CvarType, CvarValue};
use crate::error::ConsoleError;
use std::path::Path;

/// Write archived CVars as `name value` lines (Doom-style).
#[must_use]
pub fn write_ini(cvars: &CvarStore) -> String {
    let mut out = String::from("# ferrite.ini — auto-generated\n");
    for name in cvars.archive_names() {
        if let Some(v) = cvars.get(&name) {
            out.push_str(&name);
            out.push(' ');
            out.push_str(&v.display());
            out.push('\n');
        }
    }
    out
}

/// Apply `name value` lines from a ferrite/gzdoom-style ini fragment.
///
/// Recognises:
/// - `name value` / `set name value`
/// - `[section]` headers (ignored)
/// - `bind KEY "cmd"` → returned as bind pairs
/// - `#` / `//` comments
pub fn apply_ini(cvars: &mut CvarStore, text: &str) -> Result<IniApplyResult, ConsoleError> {
    let mut result = IniApplyResult::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("//")
            || line.starts_with('[')
        {
            continue;
        }
        let tokens = tokenize_ini(line);
        if tokens.is_empty() {
            continue;
        }
        let head = tokens[0].to_ascii_lowercase();
        if head == "bind" && tokens.len() >= 3 {
            result
                .binds
                .push((tokens[1].clone(), tokens[2..].join(" ")));
            continue;
        }
        if head == "set" && tokens.len() >= 3 {
            let name = &tokens[1];
            let val = tokens[2..].join(" ");
            ensure_cvar(cvars, name, &val);
            cvars.set(name, &val)?;
            result.set += 1;
            continue;
        }
        if tokens.len() >= 2 {
            let name = &tokens[0];
            let val = tokens[1..].join(" ");
            // Skip unknown gzdoom keys that look like commands.
            if name.eq_ignore_ascii_case("echo") || name.eq_ignore_ascii_case("alias") {
                continue;
            }
            ensure_cvar(cvars, name, &val);
            if cvars.get(name).is_some() {
                cvars.set(name, &val)?;
                result.set += 1;
            }
        }
    }
    Ok(result)
}

/// Load ini text from `path` if it exists.
pub fn load_ini_file(cvars: &mut CvarStore, path: &Path) -> Result<IniApplyResult, ConsoleError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ConsoleError::Message(format!("read {}: {e}", path.display())))?;
    apply_ini(cvars, &text)
}

/// Save archived CVars to `path`.
pub fn save_ini_file(cvars: &CvarStore, path: &Path) -> Result<(), ConsoleError> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, write_ini(cvars))
        .map_err(|e| ConsoleError::Message(format!("write {}: {e}", path.display())))
}

/// Result of applying an ini file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IniApplyResult {
    /// How many CVars were set.
    pub set: u32,
    /// `bind` lines (`key`, `command`).
    pub binds: Vec<(String, String)>,
}

fn ensure_cvar(cvars: &mut CvarStore, name: &str, sample: &str) {
    if cvars.get(name).is_some() {
        return;
    }
    // Import unknown archiveable scalars from gzdoom.ini.
    let (ty, val) = infer_value(sample);
    cvars.register(name, ty, val, CvarFlags::ARCHIVE);
}

fn infer_value(s: &str) -> (CvarType, CvarValue) {
    let lower = s.to_ascii_lowercase();
    match lower.as_str() {
        "true" | "yes" | "on" => (CvarType::Bool, CvarValue::Bool(true)),
        "false" | "no" | "off" => (CvarType::Bool, CvarValue::Bool(false)),
        _ => {
            if let Ok(i) = s.parse::<i32>() {
                if !s.contains('.') {
                    return (CvarType::Int, CvarValue::Int(i));
                }
            }
            if let Ok(f) = s.parse::<f64>() {
                return (CvarType::Float, CvarValue::Float(f));
            }
            (CvarType::String, CvarValue::String(s.to_owned()))
        }
    }
}

fn tokenize_ini(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for ch in line.chars() {
        match ch {
            '"' => in_quote = !in_quote,
            c if c.is_whitespace() && !in_quote => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn roundtrip_archive() {
        let mut store = CvarStore::new();
        store.register_engine_defaults();
        store.set("m_sensitivity", "1.5").unwrap();
        store.set("invertmouse", "true").unwrap();
        let text = write_ini(&store);
        assert!(text.contains("m_sensitivity 1.5"));
        assert!(text.contains("invertmouse true"));
        let mut again = CvarStore::new();
        again.register_engine_defaults();
        let r = apply_ini(&mut again, &text).unwrap();
        assert!(r.set >= 2);
        assert_eq!(again.get("invertmouse"), Some(&CvarValue::Bool(true)));
    }

    #[test]
    fn import_gzdoom_style_binds() {
        let mut store = CvarStore::new();
        store.register_engine_defaults();
        let r = apply_ini(
            &mut store,
            "m_sensitivity 2\nbind e \"+use\"\n[GlobalSettings]\ncl_run true\n",
        )
        .unwrap();
        assert_eq!(r.binds.len(), 1);
        assert_eq!(r.binds[0].0, "e");
        assert_eq!(store.get("cl_run"), Some(&CvarValue::Bool(true)));
    }
}
