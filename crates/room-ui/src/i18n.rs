//! Engine string localization (M13 bootstrap).

use std::collections::BTreeMap;

/// Lookup table for `$KEY` / engine UI strings.
#[derive(Clone, Debug, Default)]
pub struct StringTable {
    entries: BTreeMap<String, String>,
}

impl StringTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Built-in English engine strings.
    #[must_use]
    pub fn english() -> Self {
        let mut t = Self::new();
        for (k, v) in [
            ("NEWGAME", "New Game"),
            ("OPTIONS", "Options"),
            ("LOADGAME", "Load Game"),
            ("SAVEGAME", "Save Game"),
            ("QUITGAME", "Quit Game"),
            ("PAUSE", "Paused"),
            ("RESUME", "Resume"),
            ("ENDGAME", "End Game"),
            ("QUIT", "Quit"),
            ("YES", "Yes"),
            ("NO", "No"),
            ("SKILL_ITYTD", "I'm too young to die."),
            ("SKILL_HNTR", "Hey, not too rough."),
            ("SKILL_HMP", "Hurt me plenty."),
            ("SKILL_UV", "Ultra-Violence."),
            ("SKILL_NM", "Nightmare!"),
            ("PRESSKEY", "Press a key."),
            ("PRESSYN", "Press Y or N."),
            ("QSPROMPT", "Are you sure you want to quit?"),
            ("DOSQUIT", "End current game?"),
        ] {
            t.insert(k, v);
        }
        t
    }

    /// Insert / overwrite a key (stored uppercase).
    pub fn insert(&mut self, key: &str, value: impl Into<String>) {
        self.entries.insert(
            key.trim_start_matches('$').to_ascii_uppercase(),
            value.into(),
        );
    }

    /// Resolve `$KEY` or bare key; returns the key itself when missing.
    #[must_use]
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        let k = key.trim_start_matches('$').to_ascii_uppercase();
        self.entries.get(&k).map(String::as_str).unwrap_or(key)
    }

    /// Parse a LANGUAGE-style fragment: `KEY = "value";` or `KEY "value"`.
    pub fn load_language_text(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
                continue;
            }
            if let Some((k, rest)) = line.split_once('=') {
                let key = k.trim();
                let val = rest.trim().trim_end_matches(';').trim().trim_matches('"');
                if !key.is_empty() {
                    self.insert(key, val);
                }
            } else {
                let mut parts = line.splitn(2, char::is_whitespace);
                if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                    self.insert(k, v.trim().trim_matches('"'));
                }
            }
        }
    }
}

/// Strip a leading `$` and look up in the table (fallback: stripped key).
#[must_use]
pub fn localize<'a>(table: &'a StringTable, key: &'a str) -> &'a str {
    table.get(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_lookup() {
        let t = StringTable::english();
        assert_eq!(t.get("$NEWGAME"), "New Game");
        assert_eq!(t.get("QUITGAME"), "Quit Game");
    }

    #[test]
    fn language_text_loads() {
        let mut t = StringTable::new();
        t.load_language_text("NEWGAME = \"Partie\";\n// c\nOPTIONS \"Opts\"");
        assert_eq!(t.get("NEWGAME"), "Partie");
        assert_eq!(t.get("OPTIONS"), "Opts");
    }
}
