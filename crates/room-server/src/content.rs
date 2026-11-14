//! Content library + room presets (admin Settings → Content / Presets).
//!
//! The library is a directory of game files the server hands to every seat:
//! PK3 / PK7 / ZIP resource mods (Brutal Doom, …), PWADs and IWADs. Admins
//! upload and delete files from the admin UI. Files are served at
//! `GET /api/content/{file}` (streamed from disk, never held in memory).
//!
//! A **preset** is an admin-defined combination a room can be created with:
//! an optional base game (IWAD) from the library, an ordered list of content
//! files mounted on top, the maps players may pick, and optional defaults
//! (gameplay mod, skill). With `require_preset` on, players can only create
//! rooms from presets. Presets live in `<data-dir>/presets.json`.

use room_proto::{RoomContentRef, is_valid_content_file};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use tracing::{info, warn};

/// Copy the game files a deployment ships (`<seed>/content/*`) into the
/// content library when the library does not have them yet (hard link when
/// possible). Files already there are never replaced: an admin may have
/// uploaded a different version under the same name.
pub fn seed_content(seed: &Path, content_dir: &Path) {
    let Ok(rd) = std::fs::read_dir(seed.join("content")) else {
        return;
    };
    if let Err(e) = std::fs::create_dir_all(content_dir) {
        warn!(dir = %content_dir.display(), %e, "content dir unusable; seed skipped");
        return;
    }
    for e in rd.flatten() {
        let src = e.path();
        let Some(name) = src.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            continue;
        };
        if !is_valid_content_file(&name) || !src.is_file() {
            continue;
        }
        let dst = content_dir.join(&name);
        if dst.exists() {
            continue;
        }
        let done = std::fs::hard_link(&src, &dst).or_else(|_| std::fs::copy(&src, &dst).map(|_| ()));
        match done {
            Ok(()) => info!(file = %name, "shipped content added"),
            Err(e) => warn!(file = %name, %e, "shipped content not copied"),
        }
    }
}

/// Default upload cap (MiB) when `--content-max-mb` is not set.
pub const DEFAULT_MAX_CONTENT_MB: u64 = 2048;

/// What a library file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentKind {
    /// Base game WAD (`IWAD` magic).
    Iwad,
    /// Duke Nukem 3D Build engine base-game GRP.
    Duke,
    /// Patch WAD (`PWAD` magic).
    Pwad,
    /// Zip / 7z archive (PK3, PK7, IPK3, …).
    Archive,
}

/// One library file.
#[derive(Clone, Debug, Serialize)]
pub struct ContentEntry {
    /// File name (also the URL segment).
    pub file: String,
    /// Kind from the file magic.
    pub kind: ContentKind,
    /// Size in bytes.
    pub size: u64,
    /// [`content_fingerprint`] (16 hex digits).
    pub fingerprint: String,
    /// Maps found in the file (WAD map markers, `maps/NAME.wad` in archives).
    pub maps: Vec<String>,
    /// Download path.
    pub url: String,
}

impl ContentEntry {
    /// Wire reference for `welcome.room`.
    #[must_use]
    pub fn reference(&self) -> RoomContentRef {
        RoomContentRef {
            file: self.file.clone(),
            fingerprint: self.fingerprint.clone(),
            size: self.size,
        }
    }
}

/// An admin-defined room combination.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    /// Stable id (`[a-z0-9_-]`, 1..=32).
    pub id: String,
    /// Display title.
    pub title: String,
    /// Short blurb for the lobby.
    #[serde(default)]
    pub description: String,
    /// Game runtime (`doom` or `duke3d`). Older presets default to Doom.
    #[serde(default = "default_preset_game")]
    pub game: String,
    /// Library base game file (Doom IWAD or Duke GRP).
    #[serde(default)]
    pub iwad: Option<String>,
    /// Library files mounted after the IWAD, in this order.
    #[serde(default)]
    pub content: Vec<String>,
    /// Maps players may pick (upper-case lump names). Empty = any map.
    #[serde(default)]
    pub maps: Vec<String>,
    /// Preselected map (defaults to the first of `maps`).
    #[serde(default)]
    pub default_map: String,
    /// Gameplay mod from the mod catalog (must be allowed).
    #[serde(default, rename = "mod")]
    pub game_mod: Option<String>,
    /// Default skill 1..=5 (0 = lobby default).
    #[serde(default)]
    pub skill: u8,
    /// Atomic Duke `ud.coop`: 0 = DukeMatch, 1 = co-op, 2 = no spawn.
    #[serde(default = "default_duke_game_mode")]
    pub duke_game_mode: u8,
    /// Atomic Duke `ud.ffire`.
    #[serde(default)]
    pub duke_friendly_fire: bool,
    /// Doom netgame mode fixed by the preset: `coop` or `deathmatch`.
    /// `None`: the player picks it in *Host game*.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doom_mode: Option<String>,
}

fn default_preset_game() -> String {
    "doom".into()
}
fn default_duke_game_mode() -> u8 {
    1
}

/// `presets.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetConfig {
    /// Rooms must be created from a preset.
    #[serde(default)]
    pub require_preset: bool,
    /// Presets in display order.
    #[serde(default)]
    pub presets: Vec<Preset>,
}

/// Public preset row (`GET /api/presets`).
#[derive(Clone, Debug, Serialize)]
pub struct PublicPreset {
    /// Id.
    pub id: String,
    /// Title.
    pub title: String,
    /// Blurb.
    pub description: String,
    /// Game runtime (`doom` or `duke3d`).
    pub game: String,
    /// Base game file (Doom IWAD or Duke GRP), if the preset sets one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iwad: Option<RoomContentRef>,
    /// Content files, in mount order.
    pub content: Vec<RoomContentRef>,
    /// Maps players may pick (empty = any).
    pub maps: Vec<String>,
    /// Preselected map.
    pub default_map: String,
    /// Gameplay mod name.
    #[serde(rename = "mod", skip_serializing_if = "Option::is_none")]
    pub game_mod: Option<String>,
    /// Default skill (0 = none).
    pub skill: u8,
    /// Atomic Duke multiplayer mode.
    pub duke_game_mode: u8,
    /// Atomic Duke friendly-fire setting.
    pub duke_friendly_fire: bool,
    /// Doom mode fixed by the preset (`coop` / `deathmatch`); absent = player's choice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doom_mode: Option<String>,
    /// Total download size (bytes).
    pub download_bytes: u64,
}

/// `GET /api/presets` body.
#[derive(Clone, Debug, Serialize)]
pub struct PublicPresets {
    /// Rooms must use a preset.
    pub require_preset: bool,
    /// Presets.
    pub presets: Vec<PublicPreset>,
}

/// Resolved preset for a new room.
#[derive(Clone, Debug)]
pub struct ResolvedPreset {
    /// Preset title.
    pub title: String,
    /// Game runtime (`doom` or `duke3d`).
    pub game: String,
    /// Base game reference (Doom IWAD or Duke GRP).
    pub iwad: Option<RoomContentRef>,
    /// Content references, in mount order.
    pub content: Vec<RoomContentRef>,
    /// Map to start (validated).
    pub map: String,
    /// Gameplay mod name.
    pub game_mod: Option<String>,
    /// Skill when the request left it at the lobby default.
    pub skill: u8,
    /// Atomic Duke multiplayer mode.
    pub duke_game_mode: u8,
    /// Atomic Duke friendly-fire setting.
    pub duke_friendly_fire: bool,
    /// Doom mode fixed by the preset; `None` = the request's choice.
    pub doom_mode: Option<String>,
}

/// The library and presets of this node.
#[derive(Debug, Default)]
pub struct ContentStore {
    dir: Option<PathBuf>,
    presets_path: Option<PathBuf>,
    max_bytes: u64,
    entries: RwLock<BTreeMap<String, ContentEntry>>,
    presets: RwLock<PresetConfig>,
    /// File → (fingerprint, contents) for load plans.
    profiles:
        std::sync::Mutex<BTreeMap<String, (String, std::sync::Arc<crate::load_plan::Profile>)>>,
}

impl ContentStore {
    /// Open the library at `dir` (created if missing; `None` = disabled) and
    /// read `<data_dir>/presets.json`.
    #[must_use]
    pub fn open(dir: Option<PathBuf>, data_dir: &str, max_mb: u64) -> Self {
        let dir = dir.and_then(|d| match std::fs::create_dir_all(&d) {
            Ok(()) => Some(d),
            Err(e) => {
                warn!(dir = %d.display(), %e, "content dir unusable; library disabled");
                None
            }
        });
        let presets_path = (!data_dir.is_empty()).then(|| Path::new(data_dir).join("presets.json"));
        let store = Self {
            dir,
            presets_path,
            max_bytes: max_mb.max(1) * 1024 * 1024,
            ..Self::default()
        };
        store.rescan();
        if let Some(p) = &store.presets_path {
            match std::fs::read(p) {
                Ok(b) => match serde_json::from_slice::<PresetConfig>(&b) {
                    Ok(cfg) => *store.presets_w() = cfg,
                    Err(e) => warn!(path = %p.display(), %e, "presets.json unreadable"),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => warn!(path = %p.display(), %e, "presets.json unreadable"),
            }
        }
        store
    }

    /// Library directory, if enabled.
    #[must_use]
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Upload size cap in bytes.
    #[must_use]
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Re-read the directory.
    pub fn rescan(&self) {
        let Some(dir) = &self.dir else {
            return;
        };
        let mut out = BTreeMap::new();
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let path = e.path();
            let Some(file) = path.file_name().and_then(|s| s.to_str()).map(str::to_owned) else {
                continue;
            };
            if !is_valid_content_file(&file) || !path.is_file() {
                continue;
            }
            match scan_file(&path, &file) {
                Ok(entry) => {
                    out.insert(file, entry);
                }
                Err(err) => warn!(file, %err, "content file skipped"),
            }
        }
        info!(files = out.len(), "content library");
        *self.entries.write().unwrap_or_else(|p| p.into_inner()) = out;
    }

    /// Every library file.
    #[must_use]
    pub fn list(&self) -> Vec<ContentEntry> {
        self.entries_r().values().cloned().collect()
    }

    /// One file.
    #[must_use]
    pub fn get(&self, file: &str) -> Option<ContentEntry> {
        self.entries_r().get(file).cloned()
    }

    /// On-disk path of a library file (served by `GET /api/content/{file}`).
    #[must_use]
    pub fn path_of(&self, file: &str) -> Option<PathBuf> {
        let dir = self.dir.as_ref()?;
        self.entries_r().contains_key(file).then(|| dir.join(file))
    }

    /// Temp path for an upload in progress (same directory, so the final
    /// rename is atomic).
    ///
    /// # Errors
    ///
    /// Library disabled or bad file name.
    pub fn upload_temp(&self, file: &str) -> Result<PathBuf, String> {
        let dir = self
            .dir
            .as_ref()
            .ok_or("content library disabled (no --content-dir / --data-dir)")?;
        if !is_valid_content_file(file) {
            return Err(format!(
                "bad file name {file:?} (A-Z a-z 0-9 . _ -, .wad/.grp/.pk3/.pk7/.zip)"
            ));
        }
        Ok(dir.join(format!(".upload-{file}")))
    }

    /// Validate an uploaded temp file and move it into the library.
    ///
    /// # Errors
    ///
    /// Not a WAD / archive, or I/O.
    pub fn commit_upload(&self, temp: &Path, file: &str) -> Result<ContentEntry, String> {
        let entry = match scan_file(temp, file) {
            Ok(e) => e,
            Err(e) => {
                let _ = std::fs::remove_file(temp);
                return Err(e);
            }
        };
        let dir = self.dir.as_ref().ok_or("content library disabled")?;
        std::fs::rename(temp, dir.join(file)).map_err(|e| e.to_string())?;
        self.entries
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .insert(file.to_owned(), entry.clone());
        Ok(entry)
    }

    /// Delete a library file. Refused while a preset uses it.
    ///
    /// # Errors
    ///
    /// Unknown file, used by a preset, or I/O.
    pub fn delete(&self, file: &str) -> Result<(), String> {
        let users: Vec<String> = self
            .presets_r()
            .presets
            .iter()
            .filter(|p| p.iwad.as_deref() == Some(file) || p.content.iter().any(|c| c == file))
            .map(|p| p.title.clone())
            .collect();
        if !users.is_empty() {
            return Err(format!("used by preset(s): {}", users.join(", ")));
        }
        let path = self.path_of(file).ok_or("unknown file")?;
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        self.entries
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .remove(file);
        Ok(())
    }

    /// What a library file contains (cached per fingerprint; reads the file
    /// the first time, so call it off the async runtime).
    #[must_use]
    pub fn profile(&self, file: &str) -> Option<std::sync::Arc<crate::load_plan::Profile>> {
        let entry = self.get(file)?;
        let lock = || self.profiles.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((fp, p)) = lock().get(file)
            && *fp == entry.fingerprint
        {
            return Some(p.clone());
        }
        let path = self.path_of(file)?;
        let p = std::sync::Arc::new(crate::load_plan::profile_path(&path));
        lock().insert(file.to_owned(), (entry.fingerprint, p.clone()));
        Some(p)
    }

    /// Add the presets a deployment ships (`<seed>/presets.json`) that this
    /// server does not have yet. Each shipped id is offered once: the ids
    /// already offered are kept in `<data-dir>/presets_seeded.json`, so a
    /// preset an admin deleted or renamed is not brought back, and admin
    /// edits are never overwritten. A shipped preset whose files or mod are
    /// missing is skipped (and offered again on a later start).
    pub fn seed_presets(&self, seed: &Path, known_mod: &dyn Fn(&str) -> bool) {
        let Ok(bytes) = std::fs::read(seed) else {
            return;
        };
        let shipped = match serde_json::from_slice::<PresetConfig>(&bytes) {
            Ok(c) => c,
            Err(e) => {
                warn!(path = %seed.display(), %e, "seed presets unreadable");
                return;
            }
        };
        let seeded_path = self
            .presets_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|d| d.join("presets_seeded.json"));
        let mut offered: Vec<String> = seeded_path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut added = Vec::new();
        for preset in shipped.presets {
            if offered.contains(&preset.id) {
                continue;
            }
            let mut cfg = self.presets();
            if cfg.presets.iter().any(|p| p.id == preset.id) {
                offered.push(preset.id.clone());
                continue;
            }
            let id = preset.id.clone();
            cfg.presets.push(preset);
            match self.set_presets(cfg, known_mod) {
                Ok(()) => {
                    offered.push(id.clone());
                    added.push(id);
                }
                Err(e) => warn!(preset = %id, %e, "shipped preset skipped"),
            }
        }
        if let Some(p) = &seeded_path
            && let Ok(body) = serde_json::to_vec_pretty(&offered)
        {
            let _ = std::fs::write(p, body);
        }
        if !added.is_empty() {
            info!(presets = ?added, "shipped presets added");
        }
    }

    /// Current presets.
    #[must_use]
    pub fn presets(&self) -> PresetConfig {
        self.presets_r().clone()
    }

    /// Replace all presets (validated against the library), persisted.
    ///
    /// # Errors
    ///
    /// Invalid preset or write failure.
    pub fn set_presets(
        &self,
        mut cfg: PresetConfig,
        known_mod: &dyn Fn(&str) -> bool,
    ) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        {
            let lib = self.entries_r();
            for p in &mut cfg.presets {
                p.id = p.id.trim().to_ascii_lowercase();
                p.title = p.title.trim().to_owned();
                p.game = p.game.trim().to_ascii_lowercase();
                if !matches!(p.game.as_str(), "doom" | "duke3d") {
                    return Err(format!("{}: game must be doom or duke3d", p.title));
                }
                if p.duke_game_mode > 2 {
                    return Err(format!(
                        "{}: Duke multiplayer mode must be 0, 1, or 2",
                        p.title
                    ));
                }
                p.doom_mode = p
                    .doom_mode
                    .take()
                    .map(|m| m.trim().to_ascii_lowercase())
                    .filter(|m| !m.is_empty());
                if let Some(m) = &p.doom_mode {
                    if !matches!(m.as_str(), "coop" | "deathmatch") {
                        return Err(format!("{}: Doom mode must be coop or deathmatch", p.title));
                    }
                }
                if !room_proto::is_valid_mod_name(&p.id) {
                    return Err(format!("preset id {:?}: use a-z 0-9 _ - (1–32)", p.id));
                }
                if !seen.insert(p.id.clone()) {
                    return Err(format!("duplicate preset id {:?}", p.id));
                }
                if p.title.is_empty() {
                    p.title = p.id.clone();
                }
                if let Some(iwad) = p.iwad.as_deref().filter(|s| !s.is_empty()) {
                    match lib.get(iwad) {
                        Some(e) if p.game == "doom" && e.kind == ContentKind::Iwad => {}
                        Some(e) if p.game == "duke3d" && e.kind == ContentKind::Duke => {}
                        Some(_) if p.game == "duke3d" => {
                            return Err(format!("{}: {iwad} is not a Duke GRP", p.title));
                        }
                        Some(_) => return Err(format!("{}: {iwad} is not a Doom IWAD", p.title)),
                        None => return Err(format!("{}: no library file {iwad}", p.title)),
                    }
                } else {
                    p.iwad = None;
                }
                if p.game == "duke3d" && p.iwad.is_none() {
                    return Err(format!("{}: Duke presets need a base GRP file", p.title));
                }
                p.content.retain(|c| !c.trim().is_empty());
                if p.game == "duke3d" && !p.content.is_empty() {
                    return Err(format!(
                        "{}: Duke presets currently support a base GRP only",
                        p.title
                    ));
                }
                for c in &p.content {
                    if !lib.contains_key(c) {
                        return Err(format!("{}: no library file {c}", p.title));
                    }
                }
                p.maps = p
                    .maps
                    .iter()
                    .map(|m| m.trim().to_ascii_uppercase())
                    .filter(|m| !m.is_empty())
                    .collect();
                p.maps.dedup();
                p.default_map = p.default_map.trim().to_ascii_uppercase();
                if p.default_map.is_empty() {
                    p.default_map = p.maps.first().cloned().unwrap_or_default();
                }
                if !p.maps.is_empty() && !p.maps.contains(&p.default_map) {
                    return Err(format!(
                        "{}: default map {} is not in its map list",
                        p.title, p.default_map
                    ));
                }
                if let Some(m) = p
                    .game_mod
                    .as_deref()
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                {
                    if !known_mod(m) {
                        return Err(format!("{}: unknown or disallowed mod {m}", p.title));
                    }
                    p.game_mod = Some(m.to_owned());
                } else {
                    p.game_mod = None;
                }
                if p.game == "duke3d" && p.game_mod.is_some() {
                    return Err(format!(
                        "{}: Doom gameplay mods cannot be used in a Duke preset",
                        p.title
                    ));
                }
                p.skill = if p.skill == 0 { 0 } else { p.skill.clamp(1, 5) };
            }
        }
        if let Some(path) = &self.presets_path {
            let body = serde_json::to_vec_pretty(&cfg).map_err(|e| e.to_string())?;
            std::fs::write(path, body).map_err(|e| e.to_string())?;
        }
        *self.presets_w() = cfg;
        Ok(())
    }

    /// Public preset list (only presets whose files are all present).
    #[must_use]
    pub fn public(&self) -> PublicPresets {
        let cfg = self.presets_r().clone();
        let lib = self.entries_r();
        let presets = cfg
            .presets
            .iter()
            .filter_map(|p| {
                let iwad = match &p.iwad {
                    Some(f) => Some(lib.get(f)?.reference()),
                    None => None,
                };
                let content = p
                    .content
                    .iter()
                    .map(|f| lib.get(f).map(ContentEntry::reference))
                    .collect::<Option<Vec<_>>>()?;
                let download_bytes = iwad.iter().chain(&content).map(|c| c.size).sum();
                Some(PublicPreset {
                    id: p.id.clone(),
                    title: p.title.clone(),
                    description: p.description.clone(),
                    game: p.game.clone(),
                    iwad,
                    content,
                    maps: p.maps.clone(),
                    default_map: p.default_map.clone(),
                    game_mod: p.game_mod.clone(),
                    skill: p.skill,
                    duke_game_mode: p.duke_game_mode,
                    duke_friendly_fire: p.duke_friendly_fire,
                    doom_mode: p.doom_mode.clone(),
                    download_bytes,
                })
            })
            .collect();
        PublicPresets {
            require_preset: cfg.require_preset,
            presets,
        }
    }

    /// Rooms must be created from a preset.
    #[must_use]
    pub fn require_preset(&self) -> bool {
        self.presets_r().require_preset && !self.presets_r().presets.is_empty()
    }

    /// Resolve `id` + requested map for a new room.
    ///
    /// # Errors
    ///
    /// Unknown preset, map not allowed, or a file went missing.
    pub fn resolve(&self, id: &str, map: &str) -> Result<ResolvedPreset, String> {
        let pp = self.public();
        let p = pp
            .presets
            .into_iter()
            .find(|p| p.id.eq_ignore_ascii_case(id))
            .ok_or_else(|| format!("unknown preset {id:?}"))?;
        let want = map.trim().to_ascii_uppercase();
        let map = if want.is_empty() {
            if p.default_map.is_empty() {
                return Err("preset has no default map; pick one".into());
            }
            p.default_map.clone()
        } else {
            want
        };
        if !p.maps.is_empty() && !p.maps.contains(&map) {
            return Err(format!("map {map} is not offered by preset {}", p.title));
        }
        Ok(ResolvedPreset {
            title: p.title,
            game: p.game,
            iwad: p.iwad,
            content: p.content,
            map,
            game_mod: p.game_mod,
            skill: p.skill,
            duke_game_mode: p.duke_game_mode,
            duke_friendly_fire: p.duke_friendly_fire,
            doom_mode: p.doom_mode,
        })
    }

    fn entries_r(&self) -> std::sync::RwLockReadGuard<'_, BTreeMap<String, ContentEntry>> {
        self.entries.read().unwrap_or_else(|p| p.into_inner())
    }

    fn presets_r(&self) -> std::sync::RwLockReadGuard<'_, PresetConfig> {
        self.presets.read().unwrap_or_else(|p| p.into_inner())
    }

    fn presets_w(&self) -> std::sync::RwLockWriteGuard<'_, PresetConfig> {
        self.presets.write().unwrap_or_else(|p| p.into_inner())
    }
}

/// Streaming [`content_fingerprint`] (FNV-1a over the bytes, xor length).
#[derive(Clone, Copy, Debug)]
pub struct Fingerprint {
    h: u64,
    len: u64,
}

impl Default for Fingerprint {
    fn default() -> Self {
        Self {
            h: 0xcbf2_9ce4_8422_2325,
            len: 0,
        }
    }
}

impl Fingerprint {
    /// Feed bytes.
    pub fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.h ^= u64::from(b);
            self.h = self.h.wrapping_mul(0x1000_0000_01b3);
        }
        self.len += bytes.len() as u64;
    }

    /// Final hex string (same as `format!("{:016x}", content_fingerprint(..))`).
    #[must_use]
    pub fn hex(&self) -> String {
        format!("{:016x}", self.h ^ self.len)
    }
}

fn scan_file(path: &Path, file: &str) -> Result<ContentEntry, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let mut fp = Fingerprint::default();
    let mut head = Vec::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if head.len() < 16 {
            head.extend_from_slice(&buf[..n.min(16)]);
        }
        fp.update(&buf[..n]);
    }
    let is_duke_grp = file
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("grp"));
    let kind = if is_duke_grp {
        if head.get(..12) != Some(b"KenSilverman") {
            return Err("not a Duke GRP (missing KenSilverman header)".into());
        }
        ContentKind::Duke
    } else {
        match head.get(..4) {
            Some(b"IWAD") => ContentKind::Iwad,
            Some(b"PWAD") => ContentKind::Pwad,
            Some(b"PK\x03\x04") | Some(b"PK\x05\x06") | Some(b"7z\xbc\xaf") => ContentKind::Archive,
            _ => return Err("not a WAD, PK3/ZIP or PK7 file".into()),
        }
    };
    let maps = match kind {
        ContentKind::Iwad | ContentKind::Pwad => std::fs::read(path)
            .map(|b| wad_maps(&b))
            .unwrap_or_default(),
        ContentKind::Duke => {
            let bytes = std::fs::read(path).map_err(|e| format!("read Duke GRP: {e}"))?;
            duke_grp_maps(&bytes).ok_or("invalid or truncated Duke GRP")?
        }
        ContentKind::Archive => archive_maps(path),
    };
    Ok(ContentEntry {
        file: file.to_owned(),
        kind,
        size,
        fingerprint: fp.hex(),
        maps,
        url: format!("/api/content/{file}"),
    })
}

/// Read Build map names from a GRP directory without loading map bodies.
fn duke_grp_maps(bytes: &[u8]) -> Option<Vec<String>> {
    if bytes.len() < 16 || bytes.get(..12) != Some(b"KenSilverman") {
        return None;
    }
    let count = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    let Some(dir_end) = count.checked_mul(16).and_then(|n| n.checked_add(16)) else {
        return None;
    };
    if dir_end > bytes.len() || count > (bytes.len() - 16) / 16 {
        return None;
    }
    let mut body_at = dir_end;
    let mut maps = Vec::new();
    for i in 0..count {
        let at = 16 + i * 16;
        let entry = &bytes[at..at + 16];
        let name_end = entry[..12].iter().position(|&b| b == 0).unwrap_or(12);
        let name = String::from_utf8_lossy(&entry[..name_end]).to_ascii_uppercase();
        let size = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        let Some(next) = body_at.checked_add(size) else {
            return None;
        };
        if next > bytes.len() {
            return None;
        }
        body_at = next;
        if let Some(stem) = name.strip_suffix(".MAP")
            && (1..=8).contains(&stem.len())
            && stem.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !maps.iter().any(|map| map == stem)
        {
            maps.push(stem.to_owned());
        }
    }
    Some(maps)
}

fn rd_i32(b: &[u8], at: usize) -> Option<usize> {
    let raw: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
    usize::try_from(i32::from_le_bytes(raw)).ok()
}

/// Map markers of a WAD: a lump followed by `THINGS` (Doom/Hexen) or `TEXTMAP` (UDMF).
#[must_use]
pub fn wad_maps(wad: &[u8]) -> Vec<String> {
    let (Some(count), Some(dir)) = (rd_i32(wad, 4), rd_i32(wad, 8)) else {
        return Vec::new();
    };
    let name = |i: usize| -> Option<String> {
        let e = dir.checked_add(i.checked_mul(16)?)?;
        let raw = wad.get(e + 8..e + 16)?;
        let end = raw.iter().position(|&c| c == 0).unwrap_or(8);
        Some(String::from_utf8_lossy(&raw[..end]).to_ascii_uppercase())
    };
    let mut out = Vec::new();
    for i in 0..count.min(65_536).saturating_sub(1) {
        let (Some(a), Some(b)) = (name(i), name(i + 1)) else {
            break;
        };
        if (b == "THINGS" || b == "TEXTMAP") && !a.is_empty() && !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

/// Maps of a zip archive: `maps/NAME.wad` entries (named after the file,
/// like GZDoom) — after an optional single top folder.
fn archive_maps(path: &Path) -> Vec<String> {
    let Ok(f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(mut z) = zip::ZipArchive::new(std::io::BufReader::new(f)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..z.len() {
        let Ok(e) = z.by_index_raw(i) else {
            continue;
        };
        let name = e.name().replace('\\', "/").to_ascii_lowercase();
        let parts: Vec<&str> = name.split('/').collect();
        let tail = match parts.as_slice() {
            ["maps", f] | [_, "maps", f] => *f,
            _ => continue,
        };
        if let Some(stem) = tail.strip_suffix(".wad") {
            let m = stem.to_ascii_uppercase();
            if !m.is_empty() && !out.contains(&m) {
                out.push(m);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use room_proto::content_fingerprint;

    fn wad(magic: &[u8; 4], lumps: &[&str]) -> Vec<u8> {
        let mut out = magic.to_vec();
        out.extend_from_slice(&(lumps.len() as i32).to_le_bytes());
        out.extend_from_slice(&12i32.to_le_bytes());
        for l in lumps {
            out.extend_from_slice(&12i32.to_le_bytes());
            out.extend_from_slice(&0i32.to_le_bytes());
            let mut n = [0u8; 8];
            n[..l.len()].copy_from_slice(l.as_bytes());
            out.extend_from_slice(&n);
        }
        out
    }

    #[test]
    fn fingerprint_matches_proto() {
        let b = b"hello brutal doom".repeat(1000);
        let mut f = Fingerprint::default();
        f.update(&b[..7]);
        f.update(&b[7..]);
        assert_eq!(f.hex(), format!("{:016x}", content_fingerprint(&b)));
    }

    #[test]
    fn finds_wad_maps() {
        let w = wad(
            b"IWAD",
            &[
                "PLAYPAL", "MAP01", "THINGS", "LINEDEFS", "MAP02", "TEXTMAP", "ENDMAP",
            ],
        );
        assert_eq!(wad_maps(&w), ["MAP01", "MAP02"]);
    }

    #[test]
    fn library_presets_and_resolution() {
        let root = std::env::temp_dir().join(format!("room-content-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lib = root.join("content");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(
            lib.join("base.wad"),
            wad(b"IWAD", &["MAP01", "THINGS", "MAP07", "THINGS"]),
        )
        .unwrap();
        let mut zip_bytes = Vec::new();
        {
            use std::io::Write;
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("maps/bikerace.wad", o).unwrap();
            z.write_all(&wad(b"PWAD", &["BIKERACE", "THINGS"])).unwrap();
            z.start_file("decorate.txt", o).unwrap();
            z.finish().unwrap();
        }
        std::fs::write(lib.join("bd.pk3"), &zip_bytes).unwrap();
        std::fs::write(lib.join("junk.wad"), b"nope").unwrap();
        let store = ContentStore::open(Some(lib.clone()), root.to_str().unwrap(), 64);
        let files: Vec<_> = store
            .list()
            .into_iter()
            .map(|e| (e.file, e.kind, e.maps))
            .collect();
        assert_eq!(
            files,
            [
                (
                    "base.wad".into(),
                    ContentKind::Iwad,
                    vec!["MAP01".to_string(), "MAP07".into()]
                ),
                (
                    "bd.pk3".into(),
                    ContentKind::Archive,
                    vec!["BIKERACE".to_string()]
                ),
            ]
        );
        let cfg = PresetConfig {
            require_preset: true,
            presets: vec![Preset {
                id: "BD".into(),
                title: "Brutal Doom".into(),
                description: String::new(),
                game: "doom".into(),
                iwad: Some("base.wad".into()),
                content: vec!["bd.pk3".into()],
                maps: vec!["map01".into(), "bikerace".into()],
                default_map: String::new(),
                game_mod: None,
                skill: 4,
                duke_game_mode: 1,
                duke_friendly_fire: false,
                doom_mode: None,
            }],
        };
        store.set_presets(cfg, &|_| false).unwrap();
        assert!(store.require_preset());
        let r = store.resolve("bd", "").unwrap();
        assert_eq!(r.map, "MAP01");
        assert_eq!(r.content[0].file, "bd.pk3");
        assert_eq!(r.iwad.as_ref().unwrap().file, "base.wad");
        assert!(store.resolve("bd", "MAP07").is_err());
        assert_eq!(store.resolve("bd", "bikerace").unwrap().map, "BIKERACE");
        // Persisted and reloaded.
        let again = ContentStore::open(Some(lib.clone()), root.to_str().unwrap(), 64);
        assert_eq!(again.presets().presets[0].id, "bd");
        // In use → no delete; unknown file in a preset → rejected.
        assert!(store.delete("bd.pk3").is_err());
        let bad = PresetConfig {
            require_preset: false,
            presets: vec![Preset {
                content: vec!["nope.pk3".into()],
                ..again.presets().presets[0].clone()
            }],
        };
        assert!(store.set_presets(bad, &|_| false).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn shipped_content_and_presets_are_seeded_once() {
        let root = std::env::temp_dir().join(format!("room-seed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let seed = root.join("seed");
        std::fs::create_dir_all(seed.join("content")).unwrap();
        std::fs::write(seed.join("content/free.wad"), wad(b"IWAD", &["MAP01", "THINGS"])).unwrap();
        std::fs::write(seed.join("content/addon.wad"), wad(b"PWAD", &["MAP02", "THINGS"])).unwrap();
        std::fs::write(seed.join("content/bad name.wad"), b"x").unwrap();
        let shipped = r#"{"presets":[
            {"id":"free","title":"Free","iwad":"free.wad"},
            {"id":"free-addon","title":"Free + addon","iwad":"free.wad","content":["addon.wad"]},
            {"id":"modded","title":"Needs a mod","iwad":"free.wad","mod":"royale"},
            {"id":"missing","title":"Missing file","iwad":"nope.wad"}
        ]}"#;
        std::fs::write(seed.join("presets.json"), shipped).unwrap();
        let lib = root.join("data/content");
        // An admin-uploaded file with a shipped name is kept.
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("addon.wad"), wad(b"PWAD", &["MAP03", "THINGS"])).unwrap();
        seed_content(&seed, &lib);
        assert!(lib.join("free.wad").is_file());
        assert!(!lib.join("bad name.wad").exists());
        assert_eq!(wad_maps(&std::fs::read(lib.join("addon.wad")).unwrap()), ["MAP03"]);
        let data = root.join("data");
        let store = ContentStore::open(Some(lib.clone()), data.to_str().unwrap(), 64);
        store.seed_presets(&seed.join("presets.json"), &|_| false);
        let ids: Vec<_> = store.presets().presets.into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["free", "free-addon"]);
        // The admin deletes one; a restart does not bring it back, but a
        // preset that was skipped (its mod is now allowed) is added.
        let mut cfg = store.presets();
        cfg.presets.retain(|p| p.id != "free");
        store.set_presets(cfg, &|_| false).unwrap();
        let again = ContentStore::open(Some(lib.clone()), data.to_str().unwrap(), 64);
        again.seed_presets(&seed.join("presets.json"), &|m| m == "royale");
        let ids: Vec<_> = again.presets().presets.into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["free-addon", "modded"]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
