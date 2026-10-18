//! Resolve and start map music (MAPINFO / Doom II defaults).

use room_defs::MapInfo;
use room_music::{MusicError, MusicPlayer};
use room_vfs::{Namespace, Vfs};

/// Doom II / FreeDoom Phase 2 default music lump for `MAPxx` (truncated to 8 chars).
#[must_use]
pub fn doom2_music_lump(map: &str) -> Option<&'static str> {
    let name = map.trim().to_ascii_uppercase();
    let n = name.strip_prefix("MAP")?.parse::<u32>().ok()?;
    Some(match n {
        1 => "D_RUNNIN",
        2 => "D_STALKS",
        3 => "D_COUNTD",
        4 => "D_BETWEE",
        5 => "D_DOOM",
        6 => "D_THE_DA",
        7 => "D_SHAWN",
        8 => "D_DDTBLU",
        9 => "D_IN_CIT",
        10 => "D_DEAD",
        11 => "D_STLKS2",
        12 => "D_THEDA2",
        13 => "D_DOOM2",
        14 => "D_DDTBL2",
        15 => "D_RUNNI2",
        16 => "D_DEAD2",
        17 => "D_STLKS3",
        18 => "D_ROMERO",
        19 => "D_SHAWN2",
        20 => "D_MESSAG",
        21 => "D_COUNT2",
        22 => "D_DDTBL3",
        23 => "D_AMPIE",
        24 => "D_THEDA3",
        25 => "D_ADRIA",
        26 => "D_MESSG2",
        27 => "D_ROMER2",
        28 => "D_TENSE",
        29 => "D_SHAWN3",
        30 => "D_OPENIN",
        31 => "D_EVIL",
        32 => "D_ULTIMA",
        _ => return None,
    })
}

/// Doom / Ultimate Doom / FreeDoom Phase 1 music for `ExMy` (`D_E1M1`, …).
#[must_use]
pub fn doom1_music_lump(map: &str) -> Option<&'static str> {
    let name = map.trim().to_ascii_uppercase();
    // E1M1 … E4M9 (and E5Mx for Sigil-style add-ons).
    if name.len() < 4 || !name.as_bytes()[0].eq_ignore_ascii_case(&b'E') {
        return None;
    }
    let rest = &name[1..];
    let (ep, mapn) = rest.split_once('M')?;
    let ep: u32 = ep.parse().ok()?;
    let mapn: u32 = mapn.parse().ok()?;
    if !(1..=5).contains(&ep) || !(1..=9).contains(&mapn) {
        return None;
    }
    // Lump names match the map id: D_E1M1, D_E2M1, …
    // Leak into a static table via match for the common Doom 1 set; format for the rest.
    Some(match (ep, mapn) {
        (1, 1) => "D_E1M1",
        (1, 2) => "D_E1M2",
        (1, 3) => "D_E1M3",
        (1, 4) => "D_E1M4",
        (1, 5) => "D_E1M5",
        (1, 6) => "D_E1M6",
        (1, 7) => "D_E1M7",
        (1, 8) => "D_E1M8",
        (1, 9) => "D_E1M9",
        (2, 1) => "D_E2M1",
        (2, 2) => "D_E2M2",
        (2, 3) => "D_E2M3",
        (2, 4) => "D_E2M4",
        (2, 5) => "D_E2M5",
        (2, 6) => "D_E2M6",
        (2, 7) => "D_E2M7",
        (2, 8) => "D_E2M8",
        (2, 9) => "D_E2M9",
        (3, 1) => "D_E3M1",
        (3, 2) => "D_E3M2",
        (3, 3) => "D_E3M3",
        (3, 4) => "D_E3M4",
        (3, 5) => "D_E3M5",
        (3, 6) => "D_E3M6",
        (3, 7) => "D_E3M7",
        (3, 8) => "D_E3M8",
        (3, 9) => "D_E3M9",
        (4, 1) => "D_E3M4", // Ultimate Doom reuses E3 tracks for E4
        (4, 2) => "D_E3M2",
        (4, 3) => "D_E3M3",
        (4, 4) => "D_E1M5",
        (4, 5) => "D_E2M7",
        (4, 6) => "D_E2M4",
        (4, 7) => "D_E2M6",
        (4, 8) => "D_E2M5",
        (4, 9) => "D_E1M9",
        _ => return None,
    })
}

/// Pick music lump: MAPINFO `music` if present, else Doom II / Doom I tables.
#[must_use]
pub fn resolve_map_music(map: &str, mapinfo: Option<&MapInfo>) -> Option<String> {
    let key = map.trim().to_ascii_uppercase();
    if let Some(info) = mapinfo {
        if let Some(def) = info.maps.get(&key) {
            if let Some(m) = &def.music {
                let m = m.trim();
                if !m.is_empty() {
                    return Some(m.to_ascii_uppercase());
                }
            }
        }
    }
    doom2_music_lump(&key)
        .or_else(|| doom1_music_lump(&key))
        .map(str::to_owned)
}

/// Doom II title theme, then Doom 1 intro lumps.
#[must_use]
pub fn title_music_candidates() -> &'static [&'static str] {
    &["GRABBAG", "D_DM2TTL", "D_INTRO", "D_INTROA"]
}

/// Intermission / end-of-level music candidates.
#[must_use]
pub fn intermission_music_candidates() -> &'static [&'static str] {
    &["D_DM2INT", "D_INTER", "GRABBAG"]
}

/// Load a named music lump from `vfs` into `player` (MUS preferred; raw MIDI if `MThd`).
pub fn play_music_lump(
    player: &mut MusicPlayer,
    vfs: &Vfs,
    lump: &str,
) -> Result<String, MusicError> {
    let lump = lump.trim().to_ascii_uppercase();
    let id = vfs
        .find(&lump, Namespace::Global)
        .or_else(|| vfs.find_any_ns(&lump))
        .ok_or_else(|| MusicError::Message(format!("missing music lump {lump}")))?;
    let bytes = vfs
        .read(id)
        .map_err(|e| MusicError::Message(e.to_string()))?;
    if bytes.len() >= 4 && &bytes[..4] == b"MThd" {
        player.play_midi(lump.clone(), bytes.to_vec(), true);
    } else {
        player.play_mus(lump.clone(), &bytes, true)?;
    }
    Ok(lump)
}

/// Try each candidate lump until one loads.
pub fn play_first_music_lump(
    player: &mut MusicPlayer,
    vfs: &Vfs,
    candidates: &[&str],
) -> Result<String, MusicError> {
    let mut last = MusicError::Message("no music candidates".into());
    for lump in candidates {
        match play_music_lump(player, vfs, lump) {
            Ok(name) => return Ok(name),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Load a music lump from `vfs` into `player` (MUS preferred; raw MIDI if `MThd`).
pub fn play_map_music(
    player: &mut MusicPlayer,
    vfs: &Vfs,
    map: &str,
    mapinfo: Option<&MapInfo>,
) -> Result<String, MusicError> {
    let lump = resolve_map_music(map, mapinfo)
        .ok_or_else(|| MusicError::Message(format!("no music for map {map}")))?;
    play_music_lump(player, vfs, &lump)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::VfsBuilder;
    use std::path::PathBuf;

    #[test]
    fn doom2_table_and_freedoom_map01() {
        assert_eq!(doom2_music_lump("map01"), Some("D_RUNNIN"));
        assert_eq!(doom1_music_lump("e1m1"), Some("D_E1M1"));
        assert_eq!(
            resolve_map_music("MAP01", None).as_deref(),
            Some("D_RUNNIN")
        );
        assert_eq!(resolve_map_music("E1M1", None).as_deref(), Some("D_E1M1"));
        let wad =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
        if !wad.is_file() {
            return;
        }
        let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
        let mut player = MusicPlayer::new();
        let name = play_map_music(&mut player, &vfs, "MAP01", None).unwrap();
        assert_eq!(name, "D_RUNNIN");
        assert!(player.is_playing());
        assert_eq!(player.current.as_ref().unwrap().name, "D_RUNNIN");
        let title = play_first_music_lump(&mut player, &vfs, title_music_candidates()).unwrap();
        assert!(
            title == "GRABBAG" || title == "D_DM2TTL" || title == "D_INTRO" || title == "D_INTROA",
            "title={title}"
        );
        let inter =
            play_first_music_lump(&mut player, &vfs, intermission_music_candidates()).unwrap();
        assert!(
            inter == "D_DM2INT" || inter == "D_INTER" || inter == "GRABBAG",
            "inter={inter}"
        );
    }
}
