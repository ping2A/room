//! Autoload path resolution (GZDoom `[*.Autoload]` + `autoload/` directories).
//!
//! Library-first: the app supplies search roots and config `Path=` lists; this
//! module returns an ordered list of files to mount after the IWAD.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// File extensions treated as autoloadable resource containers (lowercase).
const CONTAINER_EXTS: &[&str] = &["wad", "pk3", "pk7", "zip", "ipk3", "ipk7", "pke", "7z"];

/// Ordered config section keys for an IWAD `Autoname`.
///
/// Example `doom.freedoom.phase2` →
/// `global.autoload`, `doom.autoload`, `doom.freedoom.autoload`,
/// `doom.freedoom.phase2.autoload`.
#[must_use]
pub fn autoload_sections(autoname: &str) -> Vec<String> {
    let mut out = vec!["global.autoload".to_owned()];
    let parts: Vec<&str> = autoname
        .split('.')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    for n in 1..=parts.len() {
        out.push(format!("{}.autoload", parts[..n].join(".")));
    }
    out
}

/// Inputs for [`collect_autoload_paths`].
#[derive(Clone, Debug)]
pub struct AutoloadRequest<'a> {
    /// IWADINFO `Autoname` (may be empty → only `global.autoload`).
    pub autoname: &'a str,
    /// Directories that may contain an `autoload/` tree (exe dir, user data, …).
    pub search_roots: &'a [PathBuf],
    /// Config section → `Path=` entries (keys compared case-insensitively).
    pub config_paths: &'a BTreeMap<String, Vec<PathBuf>>,
}

/// Collect autoload files in mount order (general → specific).
///
/// Duplicate paths (after canonicalize when possible) are kept once — first
/// occurrence wins for the list position; later sections that re-list the same
/// file are skipped so a more-specific remount must use a distinct path.
#[must_use]
pub fn collect_autoload_paths(req: &AutoloadRequest<'_>) -> Vec<PathBuf> {
    let sections = autoload_sections(req.autoname);
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();

    for section in &sections {
        // 1) Files under `{root}/autoload/<prefix>/` (prefix = section without `.autoload`).
        let prefix = section
            .strip_suffix(".autoload")
            .unwrap_or(section.as_str());
        for root in req.search_roots {
            let dir = if prefix.eq_ignore_ascii_case("global") {
                root.join("autoload")
            } else {
                root.join("autoload").join(prefix)
            };
            push_dir_containers(&dir, &mut seen, &mut out);
        }

        // 2) Explicit config `Path=` lines for this section.
        if let Some(paths) = lookup_section(req.config_paths, section) {
            for p in paths {
                push_path(p, &mut seen, &mut out);
            }
        }
    }
    out
}

fn lookup_section<'a>(
    map: &'a BTreeMap<String, Vec<PathBuf>>,
    section: &str,
) -> Option<&'a Vec<PathBuf>> {
    if let Some(v) = map.get(section) {
        return Some(v);
    }
    let want = section.to_ascii_lowercase();
    map.iter()
        .find(|(k, _)| k.to_ascii_lowercase() == want)
        .map(|(_, v)| v)
}

fn push_dir_containers(dir: &Path, seen: &mut BTreeSet<String>, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && is_container_path(p))
        .collect();
    // Stable, case-insensitive order for determinism across platforms.
    entries.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .cmp(&b.file_name().unwrap_or_default().to_ascii_lowercase())
    });
    for p in entries {
        push_path(&p, seen, out);
    }
}

fn push_path(path: &Path, seen: &mut BTreeSet<String>, out: &mut Vec<PathBuf>) {
    if !path.is_file() {
        return;
    }
    if !is_container_path(path) {
        return;
    }
    let key = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_ascii_lowercase();
    if seen.insert(key) {
        out.push(path.to_path_buf());
    }
}

fn is_container_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| CONTAINER_EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::vfs::VfsBuilder;
    use crate::write::WadWriter;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn sections_expand_autoname() {
        assert_eq!(
            autoload_sections("doom.freedoom.phase2"),
            [
                "global.autoload",
                "doom.autoload",
                "doom.freedoom.autoload",
                "doom.freedoom.phase2.autoload",
            ]
        );
        assert_eq!(autoload_sections(""), ["global.autoload"]);
    }

    #[test]
    fn collect_dirs_and_config_order() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // global
        fs::create_dir_all(root.join("autoload")).unwrap();
        let g = WadWriter::pwad().add("GLOBL", vec![1]).finish().unwrap();
        fs::write(root.join("autoload/global.wad"), &g).unwrap();
        // doom-specific
        fs::create_dir_all(root.join("autoload/doom")).unwrap();
        let d = WadWriter::pwad().add("DOOMX", vec![2]).finish().unwrap();
        fs::write(root.join("autoload/doom/doom_extra.wad"), &d).unwrap();
        // more specific
        fs::create_dir_all(root.join("autoload/doom.freedoom.phase2")).unwrap();
        let p = WadWriter::pwad().add("PHAS2", vec![3]).finish().unwrap();
        fs::write(root.join("autoload/doom.freedoom.phase2/phase2.wad"), &p).unwrap();

        // Config path for doom.freedoom.autoload
        let cfg_wad = WadWriter::pwad().add("CFG1", vec![4]).finish().unwrap();
        let cfg_path = root.join("from_config.wad");
        fs::write(&cfg_path, &cfg_wad).unwrap();
        let mut config = BTreeMap::new();
        config.insert("doom.freedoom.Autoload".to_owned(), vec![cfg_path.clone()]);

        let paths = collect_autoload_paths(&AutoloadRequest {
            autoname: "doom.freedoom.phase2",
            search_roots: &[root.to_path_buf()],
            config_paths: &config,
        });
        let names: Vec<String> = paths
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            names,
            [
                "global.wad",
                "doom_extra.wad",
                "from_config.wad",
                "phase2.wad",
            ]
        );

        // Mount in that order — last lump wins for overrides; presence check.
        let mut b = VfsBuilder::new();
        for p in &paths {
            b = b.mount_path(p).unwrap();
        }
        let vfs = b.build().unwrap();
        assert!(vfs.find_any_ns("GLOBL").is_some());
        assert!(vfs.find_any_ns("DOOMX").is_some());
        assert!(vfs.find_any_ns("CFG1").is_some());
        assert!(vfs.find_any_ns("PHAS2").is_some());
    }
}
