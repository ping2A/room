//! `LOADACS` lump — ACS library names to load with every map (M7 bootstrap).

use crate::error::DefsError;

/// Parsed `LOADACS` registry (additive across lumps).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadAcs {
    /// Library lump names (order preserved; duplicates kept unique by last-wins insert).
    pub libraries: Vec<String>,
}

/// Parse a `LOADACS` lump: identifiers / quoted names, `;` / newline separated.
pub fn parse_loadacs(text: &str) -> Result<LoadAcs, DefsError> {
    let mut info = LoadAcs::default();
    let mut seen = std::collections::BTreeSet::new();
    for raw in text.split([';', '\n', '\r', ',']) {
        let tok = raw.trim();
        if tok.is_empty() || tok.starts_with("//") || tok.starts_with('#') {
            continue;
        }
        // Strip optional quotes.
        let name = tok
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(tok)
            .trim();
        if name.is_empty() {
            continue;
        }
        // Skip C-style `/* … */` leftovers crudely.
        if name.starts_with("/*") {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if seen.insert(key) {
            info.libraries.push(name.to_owned());
        }
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_names() {
        let text = r#"
// comment
common
"common2"
libA; libB
"#;
        let info = parse_loadacs(text).unwrap();
        assert_eq!(info.libraries, vec!["common", "common2", "libA", "libB"]);
    }
}
