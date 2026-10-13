//! LANGUAGE lump parser (text form).

use crate::error::DefsError;
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// String tables keyed by locale code, then by KEY.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Language {
    /// `locale → (KEY → string)`.
    pub tables: BTreeMap<String, BTreeMap<String, String>>,
}

impl Language {
    /// Lookup with fallback: requested → `enu` → `default`.
    #[must_use]
    pub fn get(&self, locale: &str, key: &str) -> Option<&str> {
        let key_u = key.trim_start_matches('$');
        for loc in [locale, "enu", "default"] {
            if let Some(v) = self.tables.get(loc).and_then(|t| t.get(key_u)) {
                return Some(v.as_str());
            }
        }
        None
    }
}

/// Parse a LANGUAGE text lump.
pub fn parse_language(text: &str) -> Result<Language, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            ..Default::default()
        },
    );
    let mut lang = Language::default();
    let mut active: Vec<String> = vec!["default".into()];

    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Punct(Punct::LBracket) => {
                active.clear();
                loop {
                    match sc.next_token()? {
                        Tok::Punct(Punct::RBracket) | Tok::Eof => break,
                        Tok::Ident(s) | Tok::String(std::borrow::Cow::Borrowed(s)) => {
                            let name = s.trim_start_matches('~').to_ascii_lowercase();
                            if !s.starts_with('~') {
                                active.push(name);
                            }
                        }
                        Tok::String(std::borrow::Cow::Owned(s)) => {
                            let name = s.trim_start_matches('~').to_ascii_lowercase();
                            if !s.starts_with('~') {
                                active.push(name);
                            }
                        }
                        Tok::Punct(Punct::Comma) => {}
                        _ => {}
                    }
                }
                if active.is_empty() {
                    active.push("default".into());
                }
            }
            Tok::Ident(key) => {
                let _ = sc.check_punct(Punct::Eq)?;
                let mut value = String::new();
                loop {
                    match sc.next_token()? {
                        Tok::String(s) => value.push_str(&s),
                        Tok::Punct(Punct::Semi) | Tok::Eof => break,
                        Tok::Ident(s) if Scanner::compare(s, "$ifgame") => {
                            // skip $ifgame(...) prefix form roughly
                            let _ = sc.check_punct(Punct::LParen)?;
                            while !matches!(sc.next_token()?, Tok::Punct(Punct::RParen) | Tok::Eof)
                            {
                            }
                        }
                        other => {
                            sc.unget(other);
                            break;
                        }
                    }
                }
                let key_u = key.to_ascii_uppercase();
                for loc in &active {
                    lang.tables
                        .entry(loc.clone())
                        .or_default()
                        .insert(key_u.clone(), value.clone());
                }
            }
            Tok::Punct(Punct::Semi) => {}
            _ => {}
        }
    }
    Ok(lang)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn language_lookup_fallback() {
        let text = r#"
[enu default]
HELLO = "Hi";
[fr]
HELLO = "Bonjour";
"#;
        let lang = parse_language(text).unwrap();
        assert_eq!(lang.get("fr", "HELLO"), Some("Bonjour"));
        assert_eq!(lang.get("fr", "$HELLO"), Some("Bonjour"));
        assert_eq!(lang.get("de", "HELLO"), Some("Hi")); // falls back to enu/default
    }
}
