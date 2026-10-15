//! Display-name (callsign / pseudo) sanitization for lobby + hello.

/// Max UTF-8 chars kept for a player pseudo.
pub const MAX_NAME_CHARS: usize = 24;

/// Sanitize a player-chosen display name.
///
/// Strips controls, bidi overrides, and zero-width joiners; trims; caps length.
/// Returns `None` when nothing usable remains.
#[must_use]
pub fn sanitize_display_name(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len().min(MAX_NAME_CHARS * 4));
    for c in raw.chars() {
        if out.chars().count() >= MAX_NAME_CHARS {
            break;
        }
        if c.is_control() || is_dangerous_char(c) {
            continue;
        }
        // Collapse runs of whitespace to a single space.
        if c.is_whitespace() {
            if out.ends_with(' ') || out.is_empty() {
                continue;
            }
            out.push(' ');
            continue;
        }
        out.push(c);
    }
    let out = out.trim().to_owned();
    if out.is_empty() { None } else { Some(out) }
}

fn is_dangerous_char(c: char) -> bool {
    matches!(
        c,
        '\u{200B}' // zero-width space
        | '\u{200C}' // ZWNJ
        | '\u{200D}' // ZWJ
        | '\u{2060}' // word joiner
        | '\u{FEFF}' // BOM / ZWNBSP
        | '\u{202A}'..='\u{202E}' // bidi embeds/overrides
        | '\u{2066}'..='\u{2069}' // bidi isolates
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_controls_and_bidi() {
        assert_eq!(
            sanitize_display_name("do\u{202E}omguy"),
            Some("doomguy".into())
        );
        assert_eq!(sanitize_display_name("\u{0001}"), None);
    }

    #[test]
    fn trims_and_caps() {
        assert_eq!(sanitize_display_name("  alice  "), Some("alice".into()));
        let long = "a".repeat(40);
        assert_eq!(
            sanitize_display_name(&long).map(|s| s.chars().count()),
            Some(MAX_NAME_CHARS)
        );
    }
}
