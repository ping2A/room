//! ZIP/PK3 and directory archive mounts.

use crate::error::VfsError;
use crate::lump::{ContainerId, LumpFlags, LumpInfo};
use crate::namespace::Namespace;
use crate::short_name::ShortName;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) enum StoredPayload {
    /// Already-decoded bytes.
    Inflated(bytes::Bytes),
    /// Path relative to a directory mount root.
    File(PathBuf),
}

#[derive(Clone, Debug)]
pub(crate) struct ArchiveEntry {
    pub info: LumpInfo,
    pub payload: StoredPayload,
}

/// Namespace directory names that must not be stripped as a single root folder.
fn is_namespace_dir(name: &str) -> bool {
    Namespace::from_dir(name).is_some()
}

/// Normalise archive path to `/` lowercase for lookup keys.
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

/// Strip a single shared non-namespace root folder from all paths.
pub(crate) fn strip_single_root(paths: &mut [String]) {
    if paths.is_empty() {
        return;
    }
    let first = paths[0].trim_start_matches('/');
    let root = match first.split('/').next() {
        Some(r) if !r.is_empty() => r.to_owned(),
        _ => return,
    };
    if is_namespace_dir(&root) {
        return;
    }
    let prefix = format!("{root}/");
    if !paths.iter().all(|p| {
        let p = p.trim_start_matches('/');
        p == root || p.starts_with(&prefix)
    }) {
        return;
    }
    for p in paths.iter_mut() {
        let trimmed = p.trim_start_matches('/');
        if trimmed == root {
            *p = String::new();
        } else if let Some(rest) = trimmed.strip_prefix(&prefix) {
            *p = rest.to_owned();
        }
    }
}

/// Apply filter remapping: keep `filter/<name>/...` only when `name` is active; strip prefix.
pub(crate) fn apply_filters(path: &str, active_filters: &[String]) -> Option<String> {
    let norm = normalize_path(path);
    let rest = norm.strip_prefix("filter/")?;
    let (filter_name, rem) = rest.split_once('/')?;
    if active_filters
        .iter()
        .any(|f| f.eq_ignore_ascii_case(filter_name))
    {
        Some(rem.to_owned())
    } else {
        None
    }
}

fn path_to_lump_meta(path: &str) -> (Namespace, ShortName, LumpFlags, String) {
    let norm = normalize_path(path);
    let parts: Vec<&str> = norm.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return (
            Namespace::Global,
            ShortName::NONE,
            LumpFlags::FULLPATH,
            String::new(),
        );
    }
    let (ns, short_ok, stem_path) = if let Some(ns) = Namespace::from_dir(parts[0]) {
        let file = parts.last().copied().unwrap_or("");
        let stem = Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(file);
        (ns, true, stem)
    } else if parts.len() == 1 {
        let file = parts[0];
        let stem = Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(file);
        (Namespace::Global, true, stem)
    } else {
        let file = parts.last().copied().unwrap_or("");
        let stem = Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(file);
        (Namespace::Global, false, stem)
    };
    let mut flags = LumpFlags::empty();
    let short = if short_ok {
        ShortName::from_stem(stem_path)
    } else {
        flags.insert(LumpFlags::FULLPATH);
        ShortName::NONE
    };
    (ns, short, flags, norm)
}

fn finalize_path(logical: String, active_filters: &[String]) -> Option<String> {
    if logical.starts_with("filter/") {
        apply_filters(&logical, active_filters)
    } else {
        Some(logical)
    }
}

/// Parse a ZIP/PK3 from memory bytes.
pub(crate) fn parse_zip(
    data: &[u8],
    container: ContainerId,
    active_filters: &[String],
) -> Result<Vec<ArchiveEntry>, VfsError> {
    let cursor = Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| VfsError::parse(e.to_string()))?;

    let mut names: Vec<String> = Vec::new();
    let mut indices: Vec<usize> = Vec::new();
    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| VfsError::parse(e.to_string()))?;
        if file.is_dir() {
            continue;
        }
        names.push(normalize_path(file.name()));
        indices.push(i);
    }
    strip_single_root(&mut names);

    let mut entries = Vec::new();
    for (logical, idx) in names.into_iter().zip(indices) {
        let Some(path) = finalize_path(logical, active_filters) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let mut file = archive
            .by_index(idx)
            .map_err(|e| VfsError::parse(e.to_string()))?;
        let mut buf = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buf)
            .map_err(|e| VfsError::parse(e.to_string()))?;
        let size = u32::try_from(buf.len()).map_err(|_| VfsError::parse("zip entry too large"))?;
        let (ns, short, flags, full_path) = path_to_lump_meta(&path);
        entries.push(ArchiveEntry {
            info: LumpInfo {
                container,
                short_name: short,
                full_path,
                namespace: ns,
                size,
                flags,
            },
            payload: StoredPayload::Inflated(bytes::Bytes::from(buf)),
        });
    }
    Ok(entries)
}

/// Walk a directory tree into archive entries.
pub(crate) fn parse_directory(
    root: &Path,
    container: ContainerId,
    active_filters: &[String],
) -> Result<Vec<ArchiveEntry>, VfsError> {
    let mut paths: Vec<PathBuf> = Vec::new();
    walk_dir(root, &mut paths)?;
    let logicals: Vec<String> = paths
        .iter()
        .filter_map(|p| {
            p.strip_prefix(root)
                .ok()
                .map(|rel| normalize_path(&rel.to_string_lossy()))
        })
        .collect();

    let mut entries = Vec::new();
    for (pathbuf, log) in paths.into_iter().zip(logicals) {
        let Some(path) = finalize_path(log, active_filters) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let meta = std::fs::metadata(&pathbuf)?;
        let size = u32::try_from(meta.len()).map_err(|_| VfsError::parse("file too large"))?;
        let (ns, short, flags, full_path) = path_to_lump_meta(&path);
        entries.push(ArchiveEntry {
            info: LumpInfo {
                container,
                short_name: short,
                full_path,
                namespace: ns,
                size,
                flags,
            },
            payload: StoredPayload::File(pathbuf),
        });
    }
    Ok(entries)
}

fn walk_dir(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), VfsError> {
    for ent in std::fs::read_dir(dir)? {
        let ent = ent?;
        let path = ent.path();
        if path.is_dir() {
            walk_dir(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

pub(crate) fn read_archive_payload(payload: &StoredPayload) -> Result<bytes::Bytes, VfsError> {
    match payload {
        StoredPayload::Inflated(b) => Ok(b.clone()),
        StoredPayload::File(path) => Ok(bytes::Bytes::from(std::fs::read(path)?)),
    }
}

/// True if this archive entry is a nested WAD at the mount root.
#[must_use]
pub(crate) fn is_nested_root_wad(full_path: &str) -> bool {
    let p = full_path.trim_matches('/');
    if p.contains('/') {
        return false;
    }
    p.to_ascii_lowercase().ends_with(".wad")
}

/// Map containers under `maps/` are not nested-mounted.
#[must_use]
pub(crate) fn is_map_wad(full_path: &str) -> bool {
    let lower = full_path.to_ascii_lowercase();
    lower.starts_with("maps/") && lower.ends_with(".wad")
}

/// Parse a 7z/PK7 archive.
#[cfg(feature = "7z")]
pub(crate) fn parse_sevenz(
    data: &[u8],
    container: ContainerId,
    active_filters: &[String],
) -> Result<Vec<ArchiveEntry>, VfsError> {
    use sevenz_rust2::{ArchiveReader, Password};

    let cursor = Cursor::new(data);
    let mut reader = ArchiveReader::new(cursor, Password::empty())
        .map_err(|e| VfsError::parse(e.to_string()))?;

    let mut pairs: Vec<(String, Vec<u8>)> = Vec::new();
    reader
        .for_each_entries(|entry, rdr| {
            if entry.is_directory() {
                return Ok(true);
            }
            let mut buf = Vec::new();
            rdr.read_to_end(&mut buf)?;
            pairs.push((normalize_path(entry.name()), buf));
            Ok(true)
        })
        .map_err(|e| VfsError::parse(e.to_string()))?;

    let mut names: Vec<String> = pairs.iter().map(|(n, _)| n.clone()).collect();
    strip_single_root(&mut names);

    let mut entries = Vec::new();
    for (logical, (_, buf)) in names.into_iter().zip(pairs) {
        let Some(path) = finalize_path(logical, active_filters) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let size = u32::try_from(buf.len()).map_err(|_| VfsError::parse("7z entry too large"))?;
        let (ns, short, flags, full_path) = path_to_lump_meta(&path);
        entries.push(ArchiveEntry {
            info: LumpInfo {
                container,
                short_name: short,
                full_path,
                namespace: ns,
                size,
                flags,
            },
            payload: StoredPayload::Inflated(bytes::Bytes::from(buf)),
        });
    }
    Ok(entries)
}
