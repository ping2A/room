//! EDuke32 `.mhk` render overrides and their hash-keyed DEF map entries.

use crate::{BuildError, def::DefDirective, md4};
use std::collections::BTreeMap;

/// A map-hack entry from one `mapinfo` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapHackInfo {
    /// Optional display name from `maptitle`.
    pub title: Option<String>,
    /// Map filename from `mapfile`.
    pub map_file: String,
    /// Optional MD4 digest used to distinguish map versions.
    pub map_md4: Option<[u8; 16]>,
    /// Referenced `.mhk` file, when specified.
    pub mhk_file: Option<String>,
}

impl MapHackInfo {
    /// Whether this entry targets the named map bytes.
    pub fn matches(&self, map_file: &str, map_bytes: &[u8]) -> Result<bool, BuildError> {
        if !same_map_name(&self.map_file, map_file) {
            return Ok(false);
        }
        let Some(expected) = self.map_md4 else {
            return Ok(true);
        };
        Ok(md4::digest(map_bytes)? == expected)
    }
}

/// One parsed `.mhk` command. Arguments retain their original spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapHackCommand {
    /// Command kind (`sprite` or `light`).
    pub kind: String,
    /// Command arguments after the kind.
    pub args: Vec<String>,
    /// One-based source line.
    pub line: usize,
}

/// Parsed `.mhk` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapHackDocument {
    /// Commands in source order.
    pub commands: Vec<MapHackCommand>,
}

impl MapHackDocument {
    /// Parse sprite/model overrides and Polymer light records.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed `sprite`/`light` records or unterminated
    /// block comments.
    pub fn parse(source: &str) -> Result<Self, BuildError> {
        let mut commands = Vec::new();
        let mut in_block_comment = false;
        for (index, raw) in source.lines().enumerate() {
            let line = strip_comments(raw, &mut in_block_comment);
            let mut words = line.split_whitespace();
            let Some(kind) = words.next() else {
                continue;
            };
            let kind = kind.to_ascii_lowercase();
            if kind != "sprite" && kind != "light" {
                return Err(BuildError::Invalid("MHK command"));
            }
            let args = words.map(str::to_owned).collect::<Vec<_>>();
            if (kind == "sprite" && args.len() < 2) || (kind == "light" && args.len() < 16) {
                return Err(BuildError::Invalid("MHK command arguments"));
            }
            commands.push(MapHackCommand {
                kind,
                args,
                line: index + 1,
            });
        }
        if in_block_comment {
            return Err(BuildError::Invalid("MHK unterminated block comment"));
        }
        Ok(Self { commands })
    }

    /// Return the render-only sprite angle offsets understood by the software
    /// Build renderer. Other model and light values remain available in
    /// `commands` for the ED7 renderer path.
    pub fn sprite_angle_offsets(&self) -> Result<BTreeMap<usize, i16>, BuildError> {
        let mut offsets = BTreeMap::new();
        for command in &self.commands {
            if command.kind != "sprite" {
                continue;
            }
            let Some(field) = command.args.get(1) else {
                return Err(BuildError::Invalid("MHK sprite arguments"));
            };
            if !field.eq_ignore_ascii_case("angoff") {
                continue;
            }
            let sprite = command
                .args
                .first()
                .ok_or(BuildError::Invalid("MHK sprite index"))?
                .parse::<usize>()
                .map_err(|_| BuildError::Invalid("MHK sprite index"))?;
            let angle = command
                .args
                .get(2)
                .ok_or(BuildError::Invalid("MHK angoff value"))?
                .parse::<i32>()
                .map_err(|_| BuildError::Invalid("MHK angoff value"))?
                .rem_euclid(2048) as i16;
            offsets.insert(sprite, angle);
        }
        Ok(offsets)
    }
}

/// Select and parse the MHK referenced by the first hash-matching DEF map
/// entry. Resource paths are case-insensitive and tolerate `\\` separators.
pub fn select_map_hack(
    def_files: &[(&str, &[u8])],
    mhk_files: &[(&str, &[u8])],
    map_file: &str,
    map_bytes: &[u8],
) -> Result<Option<(String, MapHackDocument)>, BuildError> {
    let documents = def_files
        .iter()
        .map(|(_, bytes)| crate::def::DefDocument::parse(&String::from_utf8_lossy(bytes)))
        .collect::<Result<Vec<_>, _>>()?;
    select_map_hack_from_documents(&documents, mhk_files, map_file, map_bytes)
}

/// Select an MHK from already-expanded DEF documents.
///
/// This lets loaders expand DEF includes at their declaration sites before
/// selecting map-specific metadata, preserving the same source order used by
/// tile and palette resource imports.
pub fn select_map_hack_from_documents(
    documents: &[crate::def::DefDocument],
    mhk_files: &[(&str, &[u8])],
    map_file: &str,
    map_bytes: &[u8],
) -> Result<Option<(String, MapHackDocument)>, BuildError> {
    let mut infos = Vec::new();
    for document in documents {
        infos.extend(map_hack_infos(&document.directives)?);
    }
    for info in infos {
        if !info.matches(map_file, map_bytes)? {
            continue;
        }
        let Some(reference) = info.mhk_file.as_deref() else {
            continue;
        };
        let Some((name, bytes)) = find_resource(mhk_files, reference) else {
            continue;
        };
        let source = String::from_utf8_lossy(bytes);
        return MapHackDocument::parse(&source).map(|document| Some((name.to_owned(), document)));
    }
    Ok(None)
}

fn same_map_name(left: &str, right: &str) -> bool {
    let left = normalized_path(left);
    let right = normalized_path(right);
    left.rsplit('/').next() == right.rsplit('/').next()
}

fn find_resource<'a>(
    files: &'a [(&'a str, &'a [u8])],
    reference: &str,
) -> Option<(&'a str, &'a [u8])> {
    let reference = normalized_path(reference);
    files
        .iter()
        .find(|(name, _)| normalized_path(name) == reference)
        .or_else(|| {
            files.iter().find(|(name, _)| {
                let name = normalized_path(name);
                name.ends_with(&format!("/{reference}"))
            })
        })
        .map(|(name, bytes)| (*name, *bytes))
}

fn normalized_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

/// Parse `mapinfo` child blocks from a DEF document.
pub fn map_hack_infos(directives: &[DefDirective]) -> Result<Vec<MapHackInfo>, BuildError> {
    let mut infos = Vec::new();
    for directive in directives {
        if directive.keyword == "mapinfo" {
            let child_value = |name: &str| {
                directive
                    .children
                    .iter()
                    .find(|child| child.keyword == name)
                    .and_then(|child| child.args.first())
                    .cloned()
            };
            let Some(map_file) = child_value("mapfile") else {
                return Err(BuildError::Invalid("DEF mapinfo mapfile"));
            };
            let map_md4 = child_value("mapmd4")
                .map(|hex| parse_md4(&hex))
                .transpose()?;
            infos.push(MapHackInfo {
                title: child_value("maptitle"),
                map_file,
                map_md4,
                mhk_file: child_value("mhkfile"),
            });
        }
        infos.extend(map_hack_infos(&directive.children)?);
    }
    Ok(infos)
}

fn parse_md4(value: &str) -> Result<[u8; 16], BuildError> {
    if value.len() != 32 {
        return Err(BuildError::Invalid("DEF mapmd4"));
    }
    let mut digest = [0u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or(BuildError::Invalid("DEF mapmd4"))?;
        let low = hex_digit(pair[1]).ok_or(BuildError::Invalid("DEF mapmd4"))?;
        digest[index] = high * 16 + low;
    }
    Ok(digest)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn strip_comments(line: &str, in_block_comment: &mut bool) -> String {
    let mut chars = line.chars().peekable();
    let mut clean = String::new();
    while let Some(character) = chars.next() {
        if *in_block_comment {
            if character == '*' && chars.peek() == Some(&'/') {
                chars.next();
                *in_block_comment = false;
            }
        } else if character == '/' && chars.peek() == Some(&'/') {
            break;
        } else if character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            *in_block_comment = true;
        } else {
            clean.push(character);
        }
    }
    clean
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::def::DefDocument;

    #[test]
    fn parses_mapinfo_and_selects_hash_matched_mhk() {
        let source = r#"mapinfo {
            maptitle "Hollywood"
            mapfile "E1L1.map"
            mapmd4 31d6cfe0d16ae931b73c59d7e0c089c0
            mhkfile "maps/e1l1.mhk"
        }"#;
        let def = DefDocument::parse(source).unwrap();
        let infos = map_hack_infos(&def.directives).unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].title.as_deref(), Some("Hollywood"));
        assert_eq!(infos[0].mhk_file.as_deref(), Some("maps/e1l1.mhk"));
        assert!(infos[0].matches("e1l1.MAP", b"").unwrap());
        assert!(!infos[0].matches("E1L2.MAP", b"").unwrap());
    }

    #[test]
    fn parses_sprite_and_light_mhk_records() {
        let mhk = MapHackDocument::parse(
            "// model alignment\n/* block comment */\nsprite 79 angoff 1024\nsprite 80 away1\nlight 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15\n",
        )
        .unwrap();
        assert_eq!(mhk.commands.len(), 3);
        assert_eq!(mhk.commands[0].args, ["79", "angoff", "1024"]);
        assert_eq!(mhk.commands[1].args, ["80", "away1"]);
        assert_eq!(mhk.commands[2].kind, "light");
    }

    #[test]
    fn selects_hash_matching_mhk_and_builds_render_offsets() {
        let map_bytes = b"abc";
        let digest = md4::digest(map_bytes).unwrap();
        let digest = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let defs = [(
            "maps.def",
            format!(
                "mapinfo {{\n mapfile E1L1.map\n mapmd4 {digest}\n mhkfile \"resources/maphacks/e1l1.mhk\"\n}}"
            )
            .into_bytes(),
        )];
        let mhks = [(
            "mods/resources/maphacks/E1L1.MHK",
            b"sprite 2 angoff -256\n".as_slice(),
        )];
        let def_refs = [(defs[0].0, defs[0].1.as_slice())];
        let selected = select_map_hack(&def_refs, &mhks, "maps/E1L1.MAP", map_bytes)
            .unwrap()
            .unwrap();
        assert_eq!(selected.0, "mods/resources/maphacks/E1L1.MHK");
        assert_eq!(
            selected.1.sprite_angle_offsets().unwrap().get(&2),
            Some(&1792)
        );
    }
}
