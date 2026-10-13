//! IWAD / mod path discovery (M13 launcher bootstrap).

use std::path::PathBuf;

/// A playable IWAD candidate.
#[derive(Clone, Debug)]
pub struct IwadCandidate {
    /// Display label.
    pub label: String,
    /// Absolute or relative path.
    pub path: PathBuf,
}

/// Search common locations for Doom-family IWADs.
#[must_use]
pub fn discover_iwads() -> Vec<IwadCandidate> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for dir in search_dirs() {
        if !dir.is_dir() {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let lower = name.to_ascii_lowercase();
            if !lower.ends_with(".wad") && !lower.ends_with(".pk3") {
                continue;
            }
            if !is_likely_iwad(&lower) {
                continue;
            }
            let key = path.to_string_lossy().to_ascii_lowercase();
            if !seen.insert(key) {
                continue;
            }
            out.push(IwadCandidate {
                label: format!("{name} ({})", dir.display()),
                path,
            });
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// True when the filename looks like a known IWAD.
fn is_likely_iwad(lower: &str) -> bool {
    const NAMES: &[&str] = &[
        "doom.wad",
        "doom1.wad",
        "doom2.wad",
        "doom2f.wad",
        "plutonia.wad",
        "tnt.wad",
        "freedoom1.wad",
        "freedoom2.wad",
        "freedm.wad",
        "heretic.wad",
        "hexen.wad",
        "hexdd.wad",
        "strife1.wad",
        "chex.wad",
    ];
    NAMES.iter().any(|n| lower.ends_with(n))
}

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("testdata/doom"),
        PathBuf::from("testdata/freedoom"),
        PathBuf::from("wad"),
        PathBuf::from("wads"),
        PathBuf::from("."),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join("Games").join("Doom"));
        dirs.push(home.join("games").join("doom"));
        dirs.push(home.join(".local").join("share").join("games").join("doom"));
        dirs.push(
            home.join("Library")
                .join("Application Support")
                .join("gzdoom"),
        );
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(xdg).join("games").join("doom"));
    }
    if let Ok(extra) = std::env::var("DOOMWADDIR") {
        dirs.push(PathBuf::from(extra));
    }
    if let Ok(extra) = std::env::var("FERRITE_IWAD_DIR") {
        dirs.push(PathBuf::from(extra));
    }
    dirs
}

/// Pick the first Freedoom Phase 2 / Doom II style IWAD if present.
#[must_use]
pub fn prefer_doom2(cands: &[IwadCandidate]) -> Option<&IwadCandidate> {
    cands.iter().find(|c| {
        let n = c
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        n.contains("freedoom2") || n == "doom2.wad"
    })
}

/// Resolve gzdoom.ini / zdoom.ini import candidates.
#[must_use]
pub fn discover_gzdoom_ini() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        out.push(
            home.join("Library")
                .join("Application Support")
                .join("gzdoom")
                .join("gzdoom.ini"),
        );
        out.push(home.join(".config").join("gzdoom").join("gzdoom.ini"));
        out.push(home.join(".config").join("zdoom").join("zdoom.ini"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        out.push(PathBuf::from(appdata).join("gzdoom").join("gzdoom.ini"));
    }
    out.retain(|p| p.is_file());
    out
}

/// Print a short launcher summary to stderr.
pub fn print_launcher_help(cands: &[IwadCandidate]) {
    eprintln!("ferrite: launcher — discovered {} IWAD(s)", cands.len());
    for (i, c) in cands.iter().enumerate() {
        eprintln!("  [{}] {}", i + 1, c.path.display());
    }
    if cands.is_empty() {
        eprintln!("  (none — pass --iwad PATH or run scripts/fetch-freedoom.sh)");
    } else {
        eprintln!("  tip: ferrite --iwad testdata/doom/doom1.wad --play --map E1M1");
        eprintln!("       ferrite --iwad <path> --play --map MAP01");
        eprintln!("       ferrite --combat-demo");
        eprintln!("       ./scripts/play-doom1.sh");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn likely_iwad_names() {
        assert!(is_likely_iwad("freedoom2.wad"));
        assert!(is_likely_iwad("doom2.wad"));
        assert!(is_likely_iwad("doom1.wad"));
        assert!(!is_likely_iwad("my_pwads.wad"));
        // Callers lowercase before checking.
        assert!(is_likely_iwad(&"DOOM2.WAD".to_ascii_lowercase()));
    }
}
