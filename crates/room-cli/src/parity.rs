//! `room parity` — UZDoom vs Room action / flag / property / ACSF diff (`docs/36` UZ0).

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const CLASS_RS: &str = include_str!("../../room-sim/src/class.rs");
const PCD_RS: &str = include_str!("../../room-acs/src/pcd.rs");

/// One comparison table.
#[derive(Clone, Debug)]
pub struct NameDiff {
    pub title: String,
    pub room: usize,
    pub uzdoom: usize,
    pub missing_in_room: Vec<String>,
}

/// Report printed as Markdown and JSON.
#[derive(Clone, Debug)]
pub struct ParityReport {
    pub uzdoom: Option<String>,
    pub commit: Option<String>,
    pub actions: NameDiff,
    pub flags: NameDiff,
    pub properties: NameDiff,
    pub acsf: NameDiff,
}

/// Room's canonical action names.
#[must_use]
pub fn room_actions() -> BTreeSet<String> {
    room_sim::known_action_names()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// Flag spellings accepted by `is_known_flag`.
#[must_use]
pub fn room_flags() -> BTreeSet<String> {
    let mut names = quoted_names_in_fn(CLASS_RS, "is_known_flag");
    names.extend(room_sim::UZDOOM_FLAGS.iter().map(|name| (*name).to_owned()));
    names
}

/// Property spellings Room gives meaning to: the explicit names in
/// `is_known_property` plus the names the class builder (`add_actor`,
/// `apply_ext_property`, `canonical_property_name`) matches. Blanket prefixes
/// (`weapon.` …) only silence `mod check` and are not counted.
#[must_use]
pub fn room_properties() -> BTreeSet<String> {
    let mut out: BTreeSet<String> = quoted_names_in_fn(CLASS_RS, "is_known_property")
        .into_iter()
        .filter(|n| !n.ends_with('.'))
        .collect();
    for f in ["add_actor", "apply_ext_property", "canonical_property_name"] {
        if let Some(body) = method_body(CLASS_RS, f) {
            out.extend(quoted_property_names(body));
        }
    }
    // Kept-as-written properties count once something reads them.
    out.extend(room_sim::STORED_PROPERTIES.iter().map(|n| (*n).to_owned()));
    for n in room_sim::PROPERTIES_PARSED_ONLY {
        out.remove(*n);
    }
    out
}

/// Brace-matched body of `fn name` (works for methods inside `impl` blocks).
fn method_body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("fn {name}(");
    let start = src.find(&marker)?;
    let open = start + src[start..].find('{')?;
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    let mut in_str = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
        } else if b == b'"' {
            in_str = true;
        } else if b == b'\'' && i + 2 < bytes.len() && bytes[i + 2] == b'\'' {
            i += 3;
            continue;
        } else if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(&src[open..=i]);
            }
        }
        i += 1;
    }
    None
}

fn quoted_property_names(body: &str) -> BTreeSet<String> {
    // Drop `'"'` char literals first so they do not pair up with real quotes.
    let body = body.replace("'\"'", "");
    let mut out = BTreeSet::new();
    for (i, part) in body.split('"').enumerate() {
        if i % 2 == 1
            && !part.is_empty()
            && !part.ends_with('.')
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
        {
            out.insert(part.to_owned());
        }
    }
    out
}

/// `ACSF_*` names documented in `room-acs`.
#[must_use]
pub fn room_acsf() -> BTreeSet<String> {
    scan_idents(PCD_RS, "ACSF_")
}

/// Build the report. `uzdoom` is a UZDoom checkout (or `None` for Room-only counts).
pub fn build_report(uzdoom: Option<&Path>) -> Result<ParityReport> {
    let room_actions = room_actions();
    let room_flags = room_flags();
    let room_properties = room_properties();
    let room_acsf = room_acsf();
    let (commit, uz_actions, uz_flags, uz_properties, uz_acsf) = match uzdoom {
        Some(root) => {
            let commit = git_head(root).ok();
            let actions = uz_actions(root)?;
            let flags = uz_flags(root)?;
            let properties = uz_properties(root)?;
            let acsf = uz_acsf(root)?;
            (commit, actions, flags, properties, acsf)
        }
        None => (
            None,
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        ),
    };
    Ok(ParityReport {
        uzdoom: uzdoom.map(|p| p.display().to_string()),
        commit,
        actions: diff("Actions", &room_actions, &uz_actions),
        flags: diff("Flags", &room_flags, &uz_flags),
        properties: diff("Properties", &room_properties, &uz_properties),
        acsf: diff("ACSF", &room_acsf, &uz_acsf),
    })
}

impl ParityReport {
    /// Markdown, stable across runs (names are sorted).
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# UZDoom parity report\n\n");
        out.push_str("Generated by `room parity` (`docs/36` UZ0). Names are case-insensitive.\n\n");
        out.push_str("Actions are `A_Name(` in `wadsrc/static/zscript`. Flags are `DEFINE_FLAG` in `thingdef_data.cpp`. Properties are `property Name` in zscript, spelled as DECORATE writes them (bare on `Actor`, else `Class.Name` or the class `property prefix`); Room's side is the names its class builder matches. ACSF names are `ACSF_*` in `p_acs.cpp` and `room-acs`.\n\n");
        out.push_str("<!-- parity-baseline\n");
        out.push_str(&format!("room-actions: {}\n", self.actions.room));
        out.push_str(&format!("room-flags: {}\n", self.flags.room));
        out.push_str(&format!("room-properties: {}\n", self.properties.room));
        out.push_str(&format!("room-acsf: {}\n", self.acsf.room));
        out.push_str("-->\n\n");
        if self.uzdoom.is_some() {
            out.push_str(
                "UZDoom sources: vendored `wadsrc/uzdoom` plus `src/scripting/thingdef_data.cpp` and `src/playsim/p_acs.cpp` from the same commit.\n\n",
            );
        }
        if let Some(commit) = &self.commit {
            out.push_str(&format!("UZDoom commit: `{commit}`\n\n"));
        }
        for table in [&self.actions, &self.flags, &self.properties, &self.acsf] {
            out.push_str(&format!("## {}\n\n", table.title));
            out.push_str(&format!(
                "Room {} · UZDoom {} · missing in Room {}\n\n",
                table.room,
                table.uzdoom,
                table.missing_in_room.len()
            ));
            if table.missing_in_room.is_empty() {
                out.push_str("No UZDoom names missing from Room (or no checkout was passed).\n\n");
                continue;
            }
            for name in &table.missing_in_room {
                out.push_str(&format!("- `{name}`\n"));
            }
            out.push('\n');
        }
        out
    }

    /// Compact JSON (identifier-safe strings).
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        out.push_str(&format!(
            "  \"commit\": {},\n",
            json_opt_str(self.commit.as_deref())
        ));
        for (key, table) in [
            ("actions", &self.actions),
            ("flags", &self.flags),
            ("properties", &self.properties),
            ("acsf", &self.acsf),
        ] {
            out.push_str(&format!(
                "  \"{key}\": {{\"room\": {}, \"uzdoom\": {}, \"missing\": [",
                table.room, table.uzdoom
            ));
            for (i, name) in table.missing_in_room.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push('"');
                out.push_str(name);
                out.push('"');
            }
            out.push_str("]},\n");
        }
        out.push_str("  \"ok\": true\n}\n");
        out
    }
}

fn diff(title: &str, room: &BTreeSet<String>, uz: &BTreeSet<String>) -> NameDiff {
    let room_ci: BTreeSet<String> = room.iter().map(|s| s.to_ascii_lowercase()).collect();
    let missing = uz
        .iter()
        .filter(|n| !room_ci.contains(&n.to_ascii_lowercase()))
        .cloned()
        .collect();
    NameDiff {
        title: title.to_owned(),
        room: room.len(),
        uzdoom: uz.len(),
        missing_in_room: missing,
    }
}

fn quoted_names_in_fn(src: &str, fn_name: &str) -> BTreeSet<String> {
    let Some(body) = fn_body(src, fn_name) else {
        return BTreeSet::new();
    };
    let mut out = BTreeSet::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i + 1;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            if i < bytes.len() {
                if let Ok(s) = std::str::from_utf8(&bytes[start..i]) {
                    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.') {
                        out.insert(s.to_owned());
                    }
                }
                i += 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn fn_body<'a>(src: &'a str, fn_name: &str) -> Option<&'a str> {
    let marker = format!("fn {fn_name}");
    let start = src.find(&marker)?;
    let rest = &src[start..];
    let mut end = rest.len();
    for needle in ["\nfn ", "\npub fn "] {
        if let Some(pos) = rest[marker.len()..].find(needle) {
            end = end.min(marker.len() + pos);
        }
    }
    Some(&rest[..end])
}

fn scan_idents(src: &str, prefix: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes = src.as_bytes();
    let p = prefix.as_bytes();
    let mut i = 0;
    while i + p.len() < bytes.len() {
        if bytes[i..].starts_with(p) && (i == 0 || !is_ident(bytes[i - 1])) {
            let start = i;
            i += p.len();
            while i < bytes.len() && is_ident(bytes[i]) {
                i += 1;
            }
            if let Ok(s) = std::str::from_utf8(&bytes[start..i]) {
                out.insert(s.to_owned());
            }
            continue;
        }
        i += 1;
    }
    out
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn uz_actions(root: &Path) -> Result<BTreeSet<String>> {
    let dir = root.join("wadsrc/static/zscript");
    let mut out = BTreeSet::new();
    for path in walk_files(&dir, &["zs", "zc", "zsc"])? {
        let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        // Every `A_Name(` in the stdlib: definitions and the calls that name them.
        // UZDoom's own count of action functions is this set (~700).
        out.extend(a_names_called(&strip_comments(&text)));
    }
    Ok(out)
}

/// Drop `//` and `/* */` comments (`// Flags for A_Kill (Master…)` is not a call).
fn strip_comments(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c as char);
            if c == b'\\' && i + 1 < b.len() {
                out.push(b[i + 1] as char);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
        } else if c == b'"' {
            in_str = true;
            out.push('"');
            i += 1;
        } else if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(c as char);
            i += 1;
        }
    }
    out
}

fn a_names_called(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 2 < bytes.len() {
        let boundary = i == 0 || !is_ident(bytes[i - 1]);
        if boundary && bytes[i] == b'A' && bytes[i + 1] == b'_' {
            let start = i;
            i += 2;
            while i < bytes.len() && is_ident(bytes[i]) {
                i += 1;
            }
            let mut j = i;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'(' {
                if let Ok(name) = std::str::from_utf8(&bytes[start..i]) {
                    if name.len() > 2 {
                        out.push(name.to_owned());
                    }
                }
            }
            continue;
        }
        i += 1;
    }
    out
}

fn uz_flags(root: &Path) -> Result<BTreeSet<String>> {
    let path = root.join("src/scripting/thingdef_data.cpp");
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("DEFINE_FLAG(") else {
            continue;
        };
        let mut parts = rest.split(',');
        let _prefix = parts.next();
        if let Some(name) = parts.next() {
            let name = name.trim();
            if !name.is_empty() && name.bytes().all(is_ident) {
                out.insert(name.to_owned());
            }
        }
    }
    Ok(out)
}

fn uz_properties(root: &Path) -> Result<BTreeSet<String>> {
    let dir = root.join("wadsrc/static/zscript");
    let mut out = BTreeSet::new();
    for path in walk_files(&dir, &["zs", "zc", "zsc"])? {
        let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        out.extend(qualified_properties(&text));
    }
    Ok(out)
}

/// `property Name: field;` lines, spelled the way DECORATE writes them:
/// bare on `Actor`, else `Prefix.Name` where the prefix is the class name or
/// the class's `property prefix: X;`.
fn qualified_properties(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut class = String::new();
    let mut prefix = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let decl = trimmed
            .strip_prefix("class ")
            .or_else(|| trimmed.strip_prefix("extend class "));
        if let Some(rest) = decl {
            let name = rest
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                class = name.to_owned();
                prefix = name.to_owned();
            }
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("property ") else {
            continue;
        };
        let name = rest
            .split([':', ';', ' ', '\t'])
            .next()
            .unwrap_or("")
            .trim();
        if name.is_empty() || !name.bytes().all(is_ident) {
            continue;
        }
        if name.eq_ignore_ascii_case("prefix") {
            if let Some(p) = rest.split(':').nth(1) {
                let p = p.trim().trim_end_matches(';').trim();
                if !p.is_empty() {
                    prefix = p.to_owned();
                }
            }
            continue;
        }
        if class.eq_ignore_ascii_case("Actor") || prefix.is_empty() {
            out.push(name.to_owned());
        } else {
            out.push(format!("{prefix}.{name}"));
        }
    }
    out
}

fn uz_acsf(root: &Path) -> Result<BTreeSet<String>> {
    let path = root.join("src/playsim/p_acs.cpp");
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(scan_idents(&text, "ACSF_"))
}

fn walk_files(dir: &Path, exts: &[&str]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        anyhow::bail!("not a directory: {}", dir.display());
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        for ent in fs::read_dir(&cur).with_context(|| format!("read {}", cur.display()))? {
            let ent = ent?;
            let path = ent.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if exts.iter().any(|e| *e == ext) {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn git_head(root: &Path) -> Result<String> {
    let head = root.join(".git/HEAD");
    let text = fs::read_to_string(&head).with_context(|| format!("read {}", head.display()))?;
    let text = text.trim();
    if let Some(r) = text.strip_prefix("ref: ") {
        let ref_path = root.join(".git").join(r.trim());
        let hash = fs::read_to_string(&ref_path)
            .with_context(|| format!("read {}", ref_path.display()))?;
        return Ok(hash.trim().to_owned());
    }
    Ok(text.to_owned())
}

fn json_opt_str(v: Option<&str>) -> String {
    match v {
        Some(s) => format!("\"{s}\""),
        None => "null".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_inventories_are_nonempty() {
        assert!(
            room_actions().len() >= 300,
            "actions {}",
            room_actions().len()
        );
        assert!(room_flags().len() >= 64, "flags {}", room_flags().len());
        assert!(
            room_properties().len() >= 20,
            "props {}",
            room_properties().len()
        );
        assert!(room_acsf().len() >= 80, "acsf {}", room_acsf().len());
        assert!(room_actions().contains("A_Chase"));
        assert!(room_flags().iter().any(|f| f.eq_ignore_ascii_case("SOLID")));
        let props = room_properties();
        assert!(
            props.contains("armor.saveamount"),
            "builder names are counted"
        );
        assert!(
            !props.iter().any(|p| p.ends_with('.')),
            "blanket prefixes are not"
        );
    }

    #[test]
    fn zscript_properties_are_qualified_like_decorate() {
        let text = "class Actor : Thinker\n{\n\tproperty Health: health;\n}\n\
                    class BasicArmorBonus : Armor\n{\n\tproperty prefix: Armor;\n\tproperty SaveAmount : SaveAmount;\n}\n\
                    class Weapon : Inventory\n{\n\tproperty AmmoUse: AmmoUse1;\n}\n";
        assert_eq!(
            qualified_properties(text),
            vec!["Health", "Armor.SaveAmount", "Weapon.AmmoUse"]
        );
    }

    #[test]
    fn report_is_stable() {
        let report = build_report(None).unwrap();
        assert_eq!(report.to_markdown(), report.to_markdown());
        assert!(report.actions.missing_in_room.is_empty());
    }

    #[test]
    fn checked_in_baseline_has_not_regressed() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/qa/uzdoom-parity.md");
        let text = fs::read_to_string(&path).unwrap_or_default();
        if text.is_empty() {
            return;
        }
        let live = [
            ("room-actions:", room_actions().len()),
            ("room-flags:", room_flags().len()),
            ("room-properties:", room_properties().len()),
            ("room-acsf:", room_acsf().len()),
        ];
        for (key, n) in live {
            let recorded = text
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .and_then(|r| r.trim().parse::<usize>().ok())
                .unwrap_or(0);
            assert!(
                n >= recorded,
                "{key} live {n} regressed below the checked-in baseline {recorded}"
            );
        }
    }
}
