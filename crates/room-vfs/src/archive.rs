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
    /// A stored or deflated zip member, inflated when read (docs/41 M6:
    /// a 160 MB PK3 no longer sits in memory unpacked). `archive` shares the
    /// zip's bytes with every other member.
    Zipped {
        /// The whole zip file.
        archive: bytes::Bytes,
        /// Offset of the member's data.
        start: usize,
        /// Bytes of compressed data.
        compressed: usize,
        /// Bytes once inflated.
        size: usize,
        /// Deflate (`true`) or stored (`false`).
        deflated: bool,
        /// CRC-32 of the inflated bytes, checked on read as the zip reader did.
        crc32: u32,
    },
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
    // Only a folder is a wrapper: a lone root *file* (a PK3 holding just
    // `zscript.zs`) must stay, or the archive mounts empty.
    if !paths
        .iter()
        .all(|p| p.trim_start_matches('/').starts_with(&prefix))
    {
        return;
    }
    for p in paths.iter_mut() {
        if let Some(rest) = p.trim_start_matches('/').strip_prefix(&prefix) {
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
    data: &bytes::Bytes,
    container: ContainerId,
    active_filters: &[String],
) -> Result<Vec<ArchiveEntry>, VfsError> {
    let cursor = Cursor::new(data.as_ref());
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
        let payload = match lazy_member(&mut archive, idx, data) {
            Some(lazy) => lazy,
            None => {
                // Other compression methods (bzip2, LZMA, …) and encrypted
                // members are unpacked now, as before.
                let mut file = archive
                    .by_index(idx)
                    .map_err(|e| VfsError::parse(e.to_string()))?;
                let mut buf = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut buf)
                    .map_err(|e| VfsError::parse(e.to_string()))?;
                StoredPayload::Inflated(bytes::Bytes::from(buf))
            }
        };
        let size = u32::try_from(payload_len(&payload))
            .map_err(|_| VfsError::parse("zip entry too large"))?;
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
            payload,
        });
    }
    Ok(entries)
}

/// Where a stored / deflated member's data lies, without unpacking it.
fn lazy_member(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    idx: usize,
    data: &bytes::Bytes,
) -> Option<StoredPayload> {
    let file = archive.by_index_raw(idx).ok()?;
    if file.encrypted() {
        return None;
    }
    let deflated = match file.compression() {
        zip::CompressionMethod::Stored => false,
        zip::CompressionMethod::Deflated => true,
        _ => return None,
    };
    let start = usize::try_from(file.data_start()?).ok()?;
    let compressed = usize::try_from(file.compressed_size()).ok()?;
    let size = usize::try_from(file.size()).ok()?;
    if start.checked_add(compressed)? > data.len() || (!deflated && compressed != size) {
        return None;
    }
    Some(StoredPayload::Zipped {
        archive: data.clone(),
        start,
        compressed,
        size,
        deflated,
        crc32: file.crc32(),
    })
}

/// Size of a payload once read (directory files: on disk now).
fn payload_len(payload: &StoredPayload) -> usize {
    match payload {
        StoredPayload::Inflated(b) => b.len(),
        StoredPayload::File(path) => std::fs::metadata(path).map_or(0, |m| m.len() as usize),
        StoredPayload::Zipped { size, .. } => *size,
    }
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
        StoredPayload::Zipped {
            archive,
            start,
            compressed,
            size,
            deflated,
            crc32,
        } => {
            let raw = archive.slice(*start..*start + *compressed);
            let out = if *deflated {
                let mut out = Vec::with_capacity(*size);
                flate2::read::DeflateDecoder::new(raw.as_ref())
                    .read_to_end(&mut out)
                    .map_err(|e| VfsError::parse(format!("inflate: {e}")))?;
                if out.len() != *size {
                    return Err(VfsError::parse(format!(
                        "inflate: got {} bytes, expected {size}",
                        out.len()
                    )));
                }
                bytes::Bytes::from(out)
            } else {
                raw
            };
            let mut crc = flate2::Crc::new();
            crc.update(&out);
            if crc.sum() != *crc32 {
                return Err(VfsError::parse("zip member CRC mismatch"));
            }
            Ok(out)
        }
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

#[cfg(test)]
mod tests {
    use super::strip_single_root;

    fn strip(paths: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
        strip_single_root(&mut v);
        v
    }

    #[test]
    fn strips_one_wrapper_folder() {
        assert_eq!(
            strip(&["mymod/zscript.txt", "mymod/maps/e1m1.wad"]),
            ["zscript.txt", "maps/e1m1.wad"]
        );
    }

    #[test]
    fn keeps_a_lone_root_file() {
        assert_eq!(strip(&["zscript.zs"]), ["zscript.zs"]);
        assert_eq!(
            strip(&["mapinfo", "mapinfo/extra.txt"]),
            ["mapinfo", "mapinfo/extra.txt"]
        );
    }

    #[test]
    fn keeps_namespace_folders_and_mixed_roots() {
        assert_eq!(
            strip(&["sprites/a.png", "sprites/b.png"]),
            ["sprites/a.png", "sprites/b.png"]
        );
        assert_eq!(strip(&["a/x.txt", "b/y.txt"]), ["a/x.txt", "b/y.txt"]);
    }
}

#[cfg(test)]
mod lazy_zip_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn zip_with(members: &[(&str, zip::CompressionMethod, &[u8])]) -> bytes::Bytes {
        let mut out = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut out);
            for (name, method, data) in members {
                w.start_file(
                    *name,
                    SimpleFileOptions::default().compression_method(*method),
                )
                .unwrap();
                w.write_all(data).unwrap();
            }
            w.finish().unwrap();
        }
        bytes::Bytes::from(out.into_inner())
    }

    #[test]
    fn stored_and_deflated_members_unpack_on_read_others_at_mount() {
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let data = zip_with(&[
            (
                "decorate.txt",
                zip::CompressionMethod::Stored,
                b"actor Foo {}",
            ),
            ("graphics/big.png", zip::CompressionMethod::Deflated, &big),
            (
                "sounds/x.lmp",
                zip::CompressionMethod::Bzip2,
                b"bzip2 member",
            ),
        ]);
        let entries = parse_zip(&data, ContainerId(0), &[]).unwrap();
        let by_path = |p: &str| entries.iter().find(|e| e.info.full_path == p).unwrap();
        let kinds: Vec<&str> = ["decorate.txt", "graphics/big.png", "sounds/x.lmp"]
            .iter()
            .map(|p| match by_path(p).payload {
                StoredPayload::Zipped {
                    deflated: false, ..
                } => "stored",
                StoredPayload::Zipped { deflated: true, .. } => "deflated",
                StoredPayload::Inflated(_) => "inflated",
                StoredPayload::File(_) => "file",
            })
            .collect();
        assert_eq!(kinds, ["stored", "deflated", "inflated"]);
        let read = |p: &str| read_archive_payload(&by_path(p).payload).unwrap();
        assert_eq!(read("decorate.txt").as_ref(), b"actor Foo {}");
        assert_eq!(read("graphics/big.png").as_ref(), big.as_slice());
        assert_eq!(read("sounds/x.lmp").as_ref(), b"bzip2 member");
        // Sizes are known without unpacking.
        assert_eq!(by_path("graphics/big.png").info.size, 200_000);
    }

    #[test]
    fn a_damaged_deflate_stream_is_an_error_not_garbage() {
        let big: Vec<u8> = (0..50_000u32).map(|i| (i * 7 % 256) as u8).collect();
        let data = zip_with(&[("a.lmp", zip::CompressionMethod::Deflated, &big)]);
        let entries = parse_zip(&data, ContainerId(0), &[]).unwrap();
        let StoredPayload::Zipped {
            start, compressed, ..
        } = entries[0].payload
        else {
            panic!("lazy");
        };
        let mut broken = data.to_vec();
        for b in &mut broken[start..start + compressed / 2] {
            *b = 0xFF;
        }
        let broken = bytes::Bytes::from(broken);
        let entries = parse_zip(&broken, ContainerId(0), &[]).unwrap();
        assert!(read_archive_payload(&entries[0].payload).is_err());
    }

    #[test]
    fn a_changed_stored_member_fails_its_crc() {
        let data = zip_with(&[("a.txt", zip::CompressionMethod::Stored, b"hello world")]);
        let entries = parse_zip(&data, ContainerId(0), &[]).unwrap();
        let StoredPayload::Zipped { start, .. } = entries[0].payload else {
            panic!("lazy");
        };
        let mut changed = data.to_vec();
        changed[start] = b'J';
        let changed = bytes::Bytes::from(changed);
        let entries = parse_zip(&changed, ContainerId(0), &[]).unwrap();
        assert!(read_archive_payload(&entries[0].payload).is_err());
    }
}
