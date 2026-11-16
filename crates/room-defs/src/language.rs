//! LANGUAGE lump parser (text form).

use crate::error::DefsError;
use room_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// String tables keyed by locale code, then by KEY.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Language {
    /// `locale → (KEY → string)`.
    pub tables: BTreeMap<String, BTreeMap<String, String>>,
}

impl Language {
    /// Text for a message that may be a LANGUAGE reference: `$KEY`, or a
    /// bare `KEY` the tables define (pickup messages arrive both ways).
    /// Keys no loaded LANGUAGE defines fall back to Doom's standard English
    /// strings ([`engine_string`]). Anything else comes back unchanged.
    #[must_use]
    pub fn resolve<'a>(&'a self, locale: &str, text: &'a str) -> &'a str {
        let t = text.trim();
        if let Some(key) = t.strip_prefix('$') {
            return self
                .get(locale, key)
                .or_else(|| engine_string(key))
                .unwrap_or(key);
        }
        let bare = !t.is_empty()
            && t.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
        if bare {
            if let Some(v) = self.get(locale, t).or_else(|| engine_string(t)) {
                return v;
            }
        }
        text
    }

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

/// Doom's standard English message strings (pickups, keys, locked doors)
/// that GZDoom's own LANGUAGE provides and mods refer to by key
/// (`Inventory.PickupMessage "$GOTSHOTGUN2"`).
#[must_use]
pub fn engine_string(key: &str) -> Option<&'static str> {
    Some(match key.to_ascii_uppercase().as_str() {
        "GOTARMOR" => "Picked up the armor.",
        "GOTMEGA" => "Picked up the MegaArmor!",
        "GOTHTHBONUS" => "Picked up a health bonus.",
        "GOTARMBONUS" => "Picked up an armor bonus.",
        "GOTSTIM" => "Picked up a stimpack.",
        "GOTMEDINEED" => "Picked up a medikit that you REALLY need!",
        "GOTMEDIKIT" => "Picked up a medikit.",
        "GOTSUPER" => "Supercharge!",
        "GOTBLUECARD" => "Picked up a blue keycard.",
        "GOTYELWCARD" => "Picked up a yellow keycard.",
        "GOTREDCARD" => "Picked up a red keycard.",
        "GOTBLUESKUL" => "Picked up a blue skull key.",
        "GOTYELWSKUL" => "Picked up a yellow skull key.",
        "GOTREDSKUL" => "Picked up a red skull key.",
        "GOTINVUL" => "Invulnerability!",
        "GOTBERSERK" => "Berserk!",
        "GOTINVIS" => "Partial Invisibility",
        "GOTSUIT" => "Radiation Shielding Suit",
        "GOTMAP" => "Computer Area Map",
        "GOTVISOR" => "Light Amplification Visor",
        "GOTMSPHERE" => "MegaSphere!",
        "GOTCLIP" => "Picked up a clip.",
        "GOTCLIPBOX" => "Picked up a box of bullets.",
        "GOTROCKET" => "Picked up a rocket.",
        "GOTROCKBOX" => "Picked up a box of rockets.",
        "GOTCELL" => "Picked up an energy cell.",
        "GOTCELLBOX" => "Picked up an energy cell pack.",
        "GOTSHELLS" => "Picked up 4 shotgun shells.",
        "GOTSHELLBOX" => "Picked up a box of shotgun shells.",
        "GOTBACKPACK" => "Picked up a backpack full of ammo!",
        "GOTBFG9000" => "You got the BFG9000! Oh, yes.",
        "GOTCHAINGUN" => "You got the chaingun!",
        "GOTCHAINSAW" => "A chainsaw! Find some meat!",
        "GOTLAUNCHER" => "You got the rocket launcher!",
        "GOTPLASMA" => "You got the plasma gun!",
        "GOTSHOTGUN" => "You got the shotgun!",
        "GOTSHOTGUN2" => "You got the super shotgun!",
        "PD_BLUEO" => "You need a blue key to activate this object",
        "PD_REDO" => "You need a red key to activate this object",
        "PD_YELLOWO" => "You need a yellow key to activate this object",
        "PD_BLUEK" => "You need a blue key to open this door",
        "PD_REDK" => "You need a red key to open this door",
        "PD_YELLOWK" => "You need a yellow key to open this door",
        "PD_BLUEC" => "You need a blue card to open this door",
        "PD_REDC" => "You need a red card to open this door",
        "PD_YELLOWC" => "You need a yellow card to open this door",
        "PD_BLUES" => "You need a blue skull to open this door",
        "PD_REDS" => "You need a red skull to open this door",
        "PD_YELLOWS" => "You need a yellow skull to open this door",
        "PD_ANY" => "Any key will open this door",
        "PD_ANYOBJ" => "Any key will activate this object",
        "PD_ALL3" => "You need all three keys to open this door",
        "PD_ALL6" => "You need all six keys to open this door",
        "SECRETMESSAGE" => "A secret is revealed!",
        _ => return None,
    })
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
        // Top-level `$ifgame(…)` prefix (before KEY = …), after ws/comments.
        skip_language_trivia(&mut sc);
        {
            let rem = sc.remaining();
            if rem
                .as_bytes()
                .get(..7)
                .is_some_and(|b| b.eq_ignore_ascii_case(b"$ifgame"))
            {
                sc.bump_bytes(7);
                let rem = sc.remaining();
                if let Some(rest) = rem.strip_prefix('(') {
                    if let Some(end) = rest.find(')') {
                        sc.bump_bytes(1 + end + 1);
                    }
                }
                continue;
            }
        }
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
                // Skip `$ifgame(...)` prefix when present (raw, so UTF-8 values stay intact).
                {
                    let rem = sc.remaining().trim_start();
                    if rem
                        .as_bytes()
                        .get(..7)
                        .is_some_and(|b| b.eq_ignore_ascii_case(b"$ifgame"))
                    {
                        let skipped = 7;
                        sc.bump_bytes(sc.remaining().len() - rem.len() + skipped);
                        let rem = sc.remaining();
                        if let Some(rest) = rem.strip_prefix('(') {
                            if let Some(end) = rest.find(')') {
                                sc.bump_bytes(1 + end + 1);
                            }
                        }
                    }
                }
                let value = read_language_value(&mut sc)?;
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

fn skip_language_trivia(sc: &mut Scanner<'_>) {
    loop {
        let rem = sc.remaining();
        let trimmed = rem.trim_start_matches([' ', '\t', '\r', '\n']);
        let n = rem.len() - trimmed.len();
        if n > 0 {
            sc.bump_bytes(n);
        }
        let rem = sc.remaining();
        if rem.starts_with("//") {
            let end = rem.find('\n').unwrap_or(rem.len());
            sc.bump_bytes(end);
            continue;
        }
        if let Some(rest) = rem.strip_prefix("/*") {
            if let Some(end) = rest.find("*/") {
                sc.bump_bytes(2 + end + 2);
                continue;
            }
            sc.bump_bytes(rem.len());
            break;
        }
        break;
    }
}

/// Read a LANGUAGE value: quoted string(s) with BD-tolerant nested `"`, or bare text to `;`.
fn read_language_value(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    // Drop leading whitespace without tokenizing (avoids UTF-8 surprises later).
    {
        let rem = sc.remaining();
        let trimmed =
            rem.trim_start_matches(|c: char| c == ' ' || c == '\t' || c == '\r' || c == '\n');
        let n = rem.len() - trimmed.len();
        if n > 0 {
            sc.bump_bytes(n);
        }
    }
    let rem = sc.remaining();
    if rem.starts_with('"') {
        let mut value = String::new();
        let mut i = 0usize;
        let bytes = rem.as_bytes();
        while i < bytes.len() && bytes[i] == b'"' {
            i += 1; // opening quote
            while i < bytes.len() {
                let b = bytes[i];
                if b == b'\\' && i + 1 < bytes.len() {
                    let n = bytes[i + 1];
                    match n {
                        b'n' => value.push('\n'),
                        b't' => value.push('\t'),
                        b'r' => value.push('\r'),
                        b'"' => value.push('"'),
                        b'\\' => value.push('\\'),
                        other => {
                            value.push('\\');
                            value.push(char::from(other));
                        }
                    }
                    i += 2;
                    continue;
                }
                if b == b'"' {
                    // Closing quote only when followed by end / `;` / another `"` (concat)
                    // or whitespace then those. Otherwise keep as literal (BD nested quotes).
                    let after = &rem[i + 1..];
                    let rest = after.trim_start_matches([' ', '\t', '\r', '\n']);
                    if rest.is_empty()
                        || rest.starts_with(';')
                        || rest.starts_with('"')
                        || rest.starts_with('[')
                        || is_language_key_start(rest)
                    {
                        i += 1;
                        break;
                    }
                    value.push('"');
                    i += 1;
                    continue;
                }
                // UTF-8 safe: advance by char.
                let ch = rem[i..].chars().next().unwrap_or('\0');
                value.push(ch);
                i += ch.len_utf8();
            }
            // Adjacent string concat: skip ws to next `"`.
            let after = &rem[i..];
            let trimmed = after.trim_start_matches([' ', '\t', '\r', '\n']);
            let skip = after.len() - trimmed.len();
            i += skip;
            if !trimmed.starts_with('"') {
                break;
            }
        }
        // After `""…` or broken concat, bare text may remain until `;`
        // (e.g. French `KEY = ""Des armes d'…";`).
        let after = &rem[i..];
        let trimmed = after.trim_start_matches([' ', '\t', '\r', '\n']);
        let mut consume = rem.len() - after.len() + (after.len() - trimmed.len());
        if !trimmed.is_empty() && !trimmed.starts_with(';') && !trimmed.starts_with('[') {
            if let Some(semi) = trimmed.find(';') {
                let mut extra = trimmed[..semi].trim_end();
                if let Some(stripped) = extra.strip_suffix('"') {
                    extra = stripped.trim_end();
                }
                if !extra.is_empty() {
                    value.push_str(extra);
                }
                consume += semi + 1;
                sc.bump_bytes(consume);
                return Ok(value);
            }
        }
        if trimmed.starts_with(';') {
            consume += 1;
        }
        sc.bump_bytes(consume);
        return Ok(value);
    }
    // Bare value until `;`.
    let end = rem.find(';').unwrap_or(rem.len());
    let value = rem[..end].trim().to_owned();
    sc.bump_bytes(if end < rem.len() { end + 1 } else { end });
    Ok(value)
}

fn is_language_key_start(s: &str) -> bool {
    let b = s.as_bytes().first().copied().unwrap_or(0);
    (b == b'_' || b.is_ascii_alphabetic())
        && s.chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .any(|_| true)
        && s.contains('=')
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn resolves_dollar_bare_and_engine_keys() {
        let l =
            parse_language("[enu default]\nGOTRIFLE = \"You got the Assault Rifle!\";\n").unwrap();
        assert_eq!(l.resolve("enu", "$GOTRIFLE"), "You got the Assault Rifle!");
        assert_eq!(l.resolve("enu", "GOTRIFLE"), "You got the Assault Rifle!");
        assert_eq!(l.resolve("enu", "Picked up a clip."), "Picked up a clip.");
        assert_eq!(l.resolve("enu", "$NOPE"), "NOPE");
        assert_eq!(
            l.resolve("enu", "$GOTSHOTGUN2"),
            "You got the super shotgun!"
        );
        assert_eq!(
            l.resolve("enu", "PD_BLUEK"),
            "You need a blue key to open this door"
        );
    }

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

    #[test]
    fn language_nested_quotes_utf8() {
        let text = "[rus]\nKEY = \"foo \"/bar/\" baz\";\n";
        let lang = parse_language(text).unwrap();
        assert_eq!(lang.get("rus", "KEY"), Some("foo \"/bar/\" baz"));
    }
}
