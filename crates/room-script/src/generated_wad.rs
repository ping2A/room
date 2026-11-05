//! Pack registration-generated lumps into an in-memory PWAD (`docs/31` MA3).

use crate::registration::{GeneratedLump, GeneratedSoundAlias};
use room_script_abi::lump_ns;

/// Serialise generated lumps (+ optional SNDINFO) to PWAD bytes.
///
/// # Errors
///
/// Too many lumps / oversized directory offsets (same limits as a WAD).
pub fn pack_generated_pwad(
    lumps: &[GeneratedLump],
    aliases: &[GeneratedSoundAlias],
) -> Result<Vec<u8>, String> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut push = |name: &str, data: Vec<u8>| {
        entries.push((short_upper(name), data));
    };

    let sprites: Vec<_> = lumps
        .iter()
        .filter(|l| l.namespace == lump_ns::SPRITES)
        .collect();
    if !sprites.is_empty() {
        push("S_START", Vec::new());
        for l in sprites {
            push(&l.name, l.data.clone());
        }
        push("S_END", Vec::new());
    }

    let flats: Vec<_> = lumps
        .iter()
        .filter(|l| l.namespace == lump_ns::FLATS)
        .collect();
    if !flats.is_empty() {
        push("F_START", Vec::new());
        for l in flats {
            push(&l.name, l.data.clone());
        }
        push("F_END", Vec::new());
    }

    for l in lumps.iter().filter(|l| {
        matches!(
            l.namespace,
            lump_ns::GLOBAL | lump_ns::GRAPHICS | lump_ns::SOUNDS
        )
    }) {
        push(&l.name, l.data.clone());
    }

    if !aliases.is_empty() {
        let mut sndinfo = String::new();
        for a in aliases {
            if a.limit >= 0 {
                sndinfo.push_str(&format!("$limit {} {}\n", a.logical, a.limit));
            }
            sndinfo.push_str(&format!("{} {}\n", a.logical, a.lump));
        }
        push("SNDINFO", sndinfo.into_bytes());
    }

    write_pwad(&entries)
}

fn short_upper(name: &str) -> String {
    let mut s = name.to_ascii_uppercase();
    s.truncate(8);
    while s.len() < 8 {
        s.push('\0');
    }
    s
}

fn write_pwad(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let numlumps = i32::try_from(entries.len()).map_err(|_| "too many lumps".to_owned())?;
    let mut data_section = Vec::new();
    let mut directory = Vec::with_capacity(entries.len() * 16);
    for (name, payload) in entries {
        let filepos =
            i32::try_from(12 + data_section.len()).map_err(|_| "wad too large".to_owned())?;
        let size = i32::try_from(payload.len()).map_err(|_| "lump too large".to_owned())?;
        data_section.extend_from_slice(payload);
        directory.extend_from_slice(&filepos.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        let mut n = [0u8; 8];
        let bytes = name.as_bytes();
        let nlen = bytes.len().min(8);
        n[..nlen].copy_from_slice(&bytes[..nlen]);
        directory.extend_from_slice(&n);
    }
    let infotableofs =
        i32::try_from(12 + data_section.len()).map_err(|_| "wad too large".to_owned())?;
    let mut out = Vec::with_capacity(12 + data_section.len() + directory.len());
    out.extend_from_slice(b"PWAD");
    out.extend_from_slice(&numlumps.to_le_bytes());
    out.extend_from_slice(&infotableofs.to_le_bytes());
    out.extend_from_slice(&data_section);
    out.extend_from_slice(&directory);
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn packs_sprite_markers() {
        let lumps = [GeneratedLump {
            module_index: 0,
            name: "SHKMA0".into(),
            namespace: lump_ns::SPRITES,
            data: vec![1, 2, 3],
        }];
        let wad = pack_generated_pwad(&lumps, &[]).unwrap();
        assert!(wad.starts_with(b"PWAD"));
        assert!(wad.windows(8).any(|w| w.starts_with(b"S_START")));
        assert!(wad.windows(8).any(|w| w.starts_with(b"SHKMA0")));
    }
}
