//! Mod manifest (`room.toml` in PK3s, `ROOMMOD` lump in WADs) — `docs/12` §2.
//!
//! Accepts the TOML subset the manifest uses: `[mod]`, `[options]`, `[[module]]`,
//! and `key = "string" | integer | bool` lines. Unknown tables / keys are ignored.

use std::collections::BTreeMap;

use crate::ModuleScope;

/// One `[options]` value (`docs/31` MA4b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionValue {
    /// Integer option.
    Int(i64),
    /// String option.
    Str(String),
    /// Boolean option.
    Bool(bool),
}

impl OptionValue {
    /// Integer coercion (`true`→1, `false`→0, parseable strings).
    #[must_use]
    pub fn as_i32(&self, default: i32) -> i32 {
        match self {
            Self::Int(n) => i32::try_from(*n).unwrap_or(default),
            Self::Bool(b) => i32::from(*b),
            Self::Str(s) => s.parse().unwrap_or(default),
        }
    }

    /// String form (ints/bools rendered).
    #[must_use]
    pub fn as_str_owned(&self) -> String {
        match self {
            Self::Int(n) => n.to_string(),
            Self::Bool(b) => b.to_string(),
            Self::Str(s) => s.clone(),
        }
    }
}

/// Sorted `[options]` table (deterministic fingerprint).
pub type ManifestOptions = BTreeMap<String, OptionValue>;

/// Guest / host language for a `[[module]]` entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModuleLanguage {
    /// `wasm32` guest (default; netplay-safe).
    #[default]
    Wasm,
    /// Host CPython (desktop `python-mods` feature; not netplay).
    Python,
}

impl ModuleLanguage {
    fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("python") || s.eq_ignore_ascii_case("py") {
            Self::Python
        } else {
            Self::Wasm
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::Python => "python",
        }
    }
}

/// One `[[module]]` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestModule {
    /// PK3 path (`modules/play.wasm`) or WAD lump name (`SURVIVAL`).
    pub file: String,
    /// `play` (default) or `ui`.
    pub scope: ModuleScope,
    /// Namespace / save identity (defaults to the file stem).
    pub name: String,
    /// Optional WAT source lump / path (editor round-trip).
    pub source: Option<String>,
    /// `wasm` (default) or `python` (host-side desktop mods).
    pub language: ModuleLanguage,
}

/// Parsed manifest.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    /// Reverse-DNS mod id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Mod version string.
    pub version: String,
    /// Required ABI major version.
    pub abi: i32,
    /// Suggested max seats for multiplayer rooms (`docs/28` BE12).
    pub max_players: Option<u8>,
    /// Shared `[options]` (identical on every client; `docs/31` MA4b).
    pub options: ManifestOptions,
    /// `[mod] replaces_zscript = ["zscript/a.zs", …]`: ZScript files of the
    /// mods mounted before this one that it reimplements (a WASM port).
    /// Room leaves them, and the `#include`s naming them, out of the
    /// ZScript it compiles.
    pub replaces_zscript: Vec<String>,
    /// Modules in load order.
    pub modules: Vec<ManifestModule>,
}

/// Manifest parse failure.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    /// Malformed line.
    #[error("room.toml line {line}: {msg}")]
    Syntax {
        /// 1-based line.
        line: usize,
        /// Reason.
        msg: String,
    },
    /// `[[module]]` without `file`.
    #[error("room.toml: module #{0} has no file")]
    MissingFile(usize),
    /// Unsupported ABI.
    #[error("room.toml: abi {0} unsupported (host ABI {abi})", abi = crate::ABI_VERSION)]
    Abi(i32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Root,
    Mod,
    Options,
    Module,
    Other,
}

enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
}

impl Value {
    fn text(&self) -> String {
        match self {
            Self::Str(s) => s.clone(),
            Self::Int(n) => n.to_string(),
            Self::Bool(b) => b.to_string(),
        }
    }
}

fn parse_value(raw: &str) -> Option<Value> {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        loop {
            match chars.next()? {
                '"' => break,
                '\\' => match chars.next()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    c => out.push(c),
                },
                c => out.push(c),
            }
        }
        let tail = chars.as_str().trim();
        return (tail.is_empty() || tail.starts_with('#')).then_some(Value::Str(out));
    }
    if raw.starts_with('[') || raw.starts_with('{') {
        return Some(Value::Str(raw.to_owned()));
    }
    let raw = raw.split('#').next().unwrap_or("").trim();
    match raw {
        "true" => Some(Value::Bool(true)),
        "false" => Some(Value::Bool(false)),
        _ => raw.parse::<i64>().ok().map(Value::Int),
    }
}

/// `["a", "b"]` (or a single `"a"`) → its strings.
fn parse_str_list(raw: &str) -> Vec<String> {
    let inner = raw.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| {
            // TOML `\\` is one backslash; paths use `/`.
            s.trim().trim_matches('"').trim().replace("\\\\", "\\").replace('\\', "/")
        })
        .filter(|s| !s.is_empty())
        .collect()
}

impl Manifest {
    /// Parse manifest text.
    ///
    /// # Errors
    ///
    /// Malformed key/value lines, a module without `file`, or an ABI mismatch.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let mut out = Self {
            abi: crate::ABI_VERSION,
            ..Self::default()
        };
        let mut section = Section::Root;
        for (i, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix("[[").and_then(|l| l.split("]]").next()) {
                section = if name.trim() == "module" {
                    out.modules.push(ManifestModule {
                        file: String::new(),
                        scope: ModuleScope::Play,
                        name: String::new(),
                        source: None,
                        language: ModuleLanguage::Wasm,
                    });
                    Section::Module
                } else {
                    Section::Other
                };
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.split(']').next()) {
                section = match name.trim() {
                    "mod" => Section::Mod,
                    "options" => Section::Options,
                    _ => Section::Other,
                };
                continue;
            }
            let Some((key, raw)) = line.split_once('=') else {
                return Err(ManifestError::Syntax {
                    line: i + 1,
                    msg: "expected key = value".into(),
                });
            };
            let key = key.trim().trim_matches('"');
            let Some(value) = parse_value(raw) else {
                return Err(ManifestError::Syntax {
                    line: i + 1,
                    msg: format!("bad value for {key}"),
                });
            };
            match section {
                Section::Mod => match key {
                    "id" => out.id = value.text(),
                    "name" => out.name = value.text(),
                    "version" => out.version = value.text(),
                    "abi" => {
                        if let Value::Int(n) = value {
                            out.abi = i32::try_from(n).unwrap_or(i32::MAX);
                        }
                    }
                    "replaces_zscript" => out.replaces_zscript = parse_str_list(&value.text()),
                    "max_players" => {
                        if let Value::Int(n) = value {
                            out.max_players = Some(u8::try_from(n.clamp(2, 8)).unwrap_or(8));
                        }
                    }
                    _ => {}
                },
                Section::Options => {
                    let opt = match value {
                        Value::Int(n) => OptionValue::Int(n),
                        Value::Bool(b) => OptionValue::Bool(b),
                        Value::Str(s) => OptionValue::Str(s),
                    };
                    out.options.insert(key.to_owned(), opt);
                }
                Section::Module => {
                    let Some(m) = out.modules.last_mut() else {
                        continue;
                    };
                    match key {
                        "file" => m.file = value.text(),
                        "name" => m.name = value.text(),
                        "source" => m.source = Some(value.text()).filter(|s| !s.is_empty()),
                        "language" | "lang" => m.language = ModuleLanguage::parse(&value.text()),
                        "scope" => {
                            m.scope = if value.text().eq_ignore_ascii_case("ui") {
                                ModuleScope::Ui
                            } else {
                                ModuleScope::Play
                            }
                        }
                        _ => {}
                    }
                }
                Section::Root | Section::Other => {}
            }
        }
        if out.abi != crate::ABI_VERSION {
            return Err(ManifestError::Abi(out.abi));
        }
        for (i, m) in out.modules.iter_mut().enumerate() {
            if m.file.trim().is_empty() {
                return Err(ManifestError::MissingFile(i + 1));
            }
            if m.name.is_empty() {
                let stem = m.file.rsplit('/').next().unwrap_or(&m.file);
                m.name = stem.split('.').next().unwrap_or(stem).to_ascii_lowercase();
            }
        }
        Ok(out)
    }

    /// Serialize back to manifest text.
    #[must_use]
    pub fn to_toml(&self) -> String {
        fn q(s: &str) -> String {
            let mut o = String::from("\"");
            for c in s.chars() {
                match c {
                    '"' => o.push_str("\\\""),
                    '\\' => o.push_str("\\\\"),
                    '\n' => o.push_str("\\n"),
                    c => o.push(c),
                }
            }
            o.push('"');
            o
        }
        let mut s = format!(
            "[mod]\nid = {}\nname = {}\nversion = {}\nabi = {}\n",
            q(&self.id),
            q(&self.name),
            q(&self.version),
            self.abi
        );
        if !self.replaces_zscript.is_empty() {
            let list: Vec<String> = self.replaces_zscript.iter().map(|p| q(p)).collect();
            s.push_str(&format!("replaces_zscript = [{}]\n", list.join(", ")));
        }
        if !self.options.is_empty() {
            s.push_str("\n[options]\n");
            for (k, v) in &self.options {
                match v {
                    OptionValue::Int(n) => s.push_str(&format!("{k} = {n}\n")),
                    OptionValue::Bool(b) => s.push_str(&format!("{k} = {b}\n")),
                    OptionValue::Str(t) => s.push_str(&format!("{k} = {}\n", q(t))),
                }
            }
        }
        for m in &self.modules {
            s.push_str(&format!(
                "\n[[module]]\nfile = {}\nscope = {}\nname = {}\nlanguage = {}\n",
                q(&m.file),
                q(if m.scope == ModuleScope::Ui {
                    "ui"
                } else {
                    "play"
                }),
                q(&m.name),
                q(m.language.as_str()),
            ));
            if let Some(src) = &m.source {
                s.push_str(&format!("source = {}\n", q(src)));
            }
        }
        s
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_doc_example_and_roundtrips() {
        let text = r#"
# room.toml
[mod]
id = "org.example.betterimps"        # reverse-DNS, unique
name = "Better Imps"
version = "1.4.0"
abi = 1
authors = ["..."]

[[module]]
file = "modules/play.wasm"
scope = "play"
name = "betterimps"

[[module]]
file = "HUD"
scope = "ui"

[events]
handlers = ["betterimps::ImpTracker"]
"#;
        let m = Manifest::parse(text).unwrap();
        assert_eq!(m.id, "org.example.betterimps");
        assert_eq!(m.modules.len(), 2);
        assert_eq!(m.modules[0].name, "betterimps");
        assert_eq!(m.modules[1].scope, ModuleScope::Ui);
        assert_eq!(m.modules[1].name, "hud");
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m);
    }

    #[test]
    fn parses_options_table() {
        let text = r#"
[mod]
id = "x"
abi = 1
[options]
slot = 8
style = "4point"
place_weapon = true
[[module]]
file = "M.wasm"
"#;
        let m = Manifest::parse(text).unwrap();
        assert_eq!(m.options.get("slot"), Some(&OptionValue::Int(8)));
        assert_eq!(
            m.options.get("style"),
            Some(&OptionValue::Str("4point".into()))
        );
        assert_eq!(
            m.options.get("place_weapon"),
            Some(&OptionValue::Bool(true))
        );
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m);
    }

    #[test]
    fn rejects_bad_input_without_panicking() {
        assert!(Manifest::parse("[[module]]\nscope = \"play\"\n").is_err());
        assert!(Manifest::parse("[mod]\nabi = 9\n").is_err());
        assert!(Manifest::parse("[mod]\nid = \"unterminated\n").is_err());
        assert!(Manifest::parse("garbage").is_err());
        for s in ["[", "[[", "=", "\"=\"", "[mod]\n=\"\\", "a = \"\\"] {
            let _ = Manifest::parse(s);
        }
    }

    #[test]
    fn parses_replaced_zscript_and_roundtrips() {
        let text = "[mod]\nid = \"x\"\nreplaces_zscript = [\"zscript/BD_WCS.zc\", \"zscript\\\\DamageHandler.zc\"]\n\n[[module]]\nfile = \"BDZSCRPT\"\n";
        let m = Manifest::parse(text).unwrap();
        assert_eq!(m.replaces_zscript, ["zscript/BD_WCS.zc", "zscript/DamageHandler.zc"]);
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m);
        let one = Manifest::parse("[mod]\nreplaces_zscript = \"a.zs\"\n").unwrap();
        assert_eq!(one.replaces_zscript, ["a.zs"]);
    }
}
