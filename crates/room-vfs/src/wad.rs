//! WAD container parser.

use crate::error::VfsError;
use crate::lump::{ContainerId, LumpFlags, LumpInfo};
use crate::namespace::Namespace;
use crate::short_name::ShortName;
use crate::source::ContainerSource;

#[derive(Clone, Debug)]
pub(crate) struct WadEntry {
    pub info: LumpInfo,
    pub filepos: u32,
    /// When set, size was clamped to the container end.
    pub truncated: bool,
}

struct MarkerRange {
    start_names: &'static [&'static str],
    end_names: &'static [&'static str],
    ns: Namespace,
}

const MARKER_RANGES: &[MarkerRange] = &[
    MarkerRange {
        start_names: &["S_START", "SS_START"],
        end_names: &["S_END", "SS_END"],
        ns: Namespace::Sprites,
    },
    MarkerRange {
        start_names: &["F_START", "FF_START"],
        end_names: &["F_END", "FF_END"],
        ns: Namespace::Flats,
    },
    MarkerRange {
        start_names: &["C_START"],
        end_names: &["C_END"],
        ns: Namespace::Colormaps,
    },
    MarkerRange {
        start_names: &["A_START"],
        end_names: &["A_END"],
        ns: Namespace::Acs,
    },
    MarkerRange {
        start_names: &["TX_START"],
        end_names: &["TX_END"],
        ns: Namespace::NewTextures,
    },
    MarkerRange {
        start_names: &["V_START"],
        end_names: &["V_END"],
        ns: Namespace::Voxels,
    },
    MarkerRange {
        start_names: &["HI_START"],
        end_names: &["HI_END"],
        ns: Namespace::HiRes,
    },
    MarkerRange {
        start_names: &["P_START", "PP_START"],
        end_names: &["P_END", "PP_END"],
        ns: Namespace::Patches,
    },
];

/// Parse a WAD from `source` into lump entries for `container`.
pub(crate) fn parse_wad(
    source: &dyn ContainerSource,
    container: ContainerId,
) -> Result<Vec<WadEntry>, VfsError> {
    if source.len() < 12 {
        return Err(VfsError::parse("WAD too small"));
    }
    let header = source.read_range(0, 12)?;
    let magic = &header[0..4];
    if magic != b"IWAD" && magic != b"PWAD" {
        return Err(VfsError::parse("not a WAD (bad magic)"));
    }
    let numlumps = i32::from_le_bytes(header[4..8].try_into().unwrap_or([0; 4]));
    let infotableofs = i32::from_le_bytes(header[8..12].try_into().unwrap_or([0; 4]));
    if numlumps < 0 {
        return Err(VfsError::parse("negative lump count"));
    }
    if infotableofs < 0 {
        return Err(VfsError::parse("negative directory offset"));
    }
    let numlumps = numlumps as usize;
    let dir_off = infotableofs as u64;
    let dir_len = numlumps
        .checked_mul(16)
        .ok_or_else(|| VfsError::parse("directory size overflow"))?;
    if dir_off.saturating_add(dir_len as u64) > source.len() {
        return Err(VfsError::parse("directory past end of file"));
    }
    let dir = source.read_range(dir_off, dir_len)?;

    let mut raw: Vec<(ShortName, u32, u32)> = Vec::with_capacity(numlumps);
    for i in 0..numlumps {
        let e = &dir[i * 16..(i + 1) * 16];
        let filepos = i32::from_le_bytes(e[0..4].try_into().unwrap_or([0; 4]));
        let size = i32::from_le_bytes(e[4..8].try_into().unwrap_or([0; 4]));
        let mut name_bytes = [0u8; 8];
        name_bytes.copy_from_slice(&e[8..16]);
        let name = ShortName::from_wad_bytes(&name_bytes);
        let filepos = if filepos < 0 { 0u32 } else { filepos as u32 };
        let size = if size < 0 { 0u32 } else { size as u32 };
        raw.push((name, filepos, size));
    }

    let namespaces = assign_namespaces(&raw);
    let file_len = source.len();
    let mut entries = Vec::with_capacity(raw.len());
    for (i, (name, filepos, size)) in raw.into_iter().enumerate() {
        let end = u64::from(filepos).saturating_add(u64::from(size));
        let (size, truncated) = if end > file_len {
            let clamped = file_len.saturating_sub(u64::from(filepos));
            (clamped as u32, true)
        } else {
            (size, false)
        };
        let mut flags = LumpFlags::empty();
        if size == 0 {
            flags.insert(LumpFlags::MARKER);
        }
        if namespaces[i].1 {
            flags.insert(LumpFlags::MAYBE_FLAT);
        }
        let short = name.as_str().to_owned();
        entries.push(WadEntry {
            info: LumpInfo {
                container,
                short_name: name,
                full_path: short,
                namespace: namespaces[i].0,
                size,
                flags,
            },
            filepos,
            truncated,
        });
    }
    Ok(entries)
}

/// Returns `(namespace, maybe_flat)` per lump.
fn assign_namespaces(raw: &[(ShortName, u32, u32)]) -> Vec<(Namespace, bool)> {
    let mut out = vec![(Namespace::Global, false); raw.len()];
    let mut active: Option<Namespace> = None;
    let mut open_range: Option<usize> = None;

    for (i, (name, _, _)) in raw.iter().enumerate() {
        let n = name.as_str();
        if let Some(idx) = MARKER_RANGES
            .iter()
            .position(|r| r.start_names.contains(&n))
        {
            active = Some(MARKER_RANGES[idx].ns);
            open_range = Some(idx);
            out[i] = (Namespace::Global, false);
            continue;
        }
        if let Some(idx) = MARKER_RANGES.iter().position(|r| r.end_names.contains(&n)) {
            if open_range == Some(idx) {
                active = None;
                open_range = None;
            }
            // Unmatched END: ignore (leave active as-is per GZDoom warn+ignore for wrong end;
            // for unmatched END with no start, ignore).
            if open_range.is_none() && active.is_none() {
                // flat hack handled below
            }
            out[i] = (Namespace::Global, false);
            continue;
        }
        if let Some(ns) = active {
            out[i] = (ns, false);
        }
    }

    // Missing END: leave active through EOF (already applied).
    // Flat hack: F_END/FF_END present but no start → MAYBE_FLAT on 4096-byte lumps before last end.
    let has_f_start = raw
        .iter()
        .any(|(n, _, _)| n.as_str() == "F_START" || n.as_str() == "FF_START");
    if !has_f_start {
        if let Some(end_idx) = raw.iter().rposition(|(n, _, _)| {
            let s = n.as_str();
            s == "F_END" || s == "FF_END"
        }) {
            for (i, (_, _, size)) in raw.iter().enumerate().take(end_idx) {
                if *size == 4096 && out[i].0 == Namespace::Global {
                    out[i].1 = true;
                }
            }
        }
    }

    out
}

/// Read lump payload from a WAD entry.
pub(crate) fn read_wad_lump(
    source: &dyn ContainerSource,
    entry: &WadEntry,
) -> Result<bytes::Bytes, VfsError> {
    if entry.info.size == 0 {
        return Ok(bytes::Bytes::new());
    }
    let data = source.read_range(u64::from(entry.filepos), entry.info.size as usize)?;
    Ok(bytes::Bytes::from(data))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::source::MemorySource;
    use crate::write::WadWriter;

    #[test]
    fn markers_assign_sprite_namespace() {
        let bytes = WadWriter::pwad()
            .marker("S_START")
            .add("TROOA1", vec![1, 2, 3])
            .marker("S_END")
            .add("DEMO1", vec![9])
            .finish()
            .unwrap();
        let src = MemorySource::new(bytes);
        let entries = parse_wad(&src, ContainerId(0)).unwrap();
        assert_eq!(entries[1].info.namespace, Namespace::Sprites);
        assert_eq!(entries[1].info.short_name.as_str(), "TROOA1");
        assert_eq!(entries[3].info.namespace, Namespace::Global);
    }

    #[test]
    fn flat_hack_marks_4096() {
        let mut flat = vec![0u8; 4096];
        flat[0] = 7;
        let bytes = WadWriter::pwad()
            .add("FLOOR0_1", flat)
            .marker("F_END")
            .finish()
            .unwrap();
        let src = MemorySource::new(bytes);
        let entries = parse_wad(&src, ContainerId(0)).unwrap();
        assert!(entries[0].info.flags.contains(LumpFlags::MAYBE_FLAT));
        assert_eq!(entries[0].info.namespace, Namespace::Global);
    }
}
