//! Gameplay mod catalog (`--mods-dir`): PWADs rooms can be created with.
//!
//! Every seat runs the full sim, so all clients must mount the same mod bytes.
//! The server lists the catalog at `GET /api/mods`, serves the files at
//! `GET /api/mods/{name}.wad`, names the room's mod (with its fingerprint) in
//! `welcome.room.mod`, and the optional verifier mounts the same file.
//!
//! Mods found in the directory start disallowed: an admin allows each one
//! (admin Settings → Mods), and the choice is kept in `<data-dir>/mods.json`.
//! Only allowed mods are listed and accepted for new rooms; rooms already
//! running a mod keep it (and its file stays downloadable).

use bytes::Bytes;
use room_proto::{RoomModRef, content_fingerprint, is_valid_mod_name};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use tracing::{info, warn};

/// Largest mod file the catalog accepts.
pub const MAX_MOD_BYTES: usize = 32 * 1024 * 1024;

/// One servable mod.
#[derive(Clone, Debug)]
pub struct ModEntry {
    /// Wire reference sent in `welcome.room.mod`.
    pub info: RoomModRef,
    /// File bytes served to clients.
    pub bytes: Bytes,
    /// On-disk path (verifier mounts it); `None` for in-memory entries.
    pub path: Option<PathBuf>,
    /// Offered in the lobby "Play solo" picker (`[mod] solo`, default true).
    pub solo: bool,
    /// Short player-facing blurb (`[mod] description`).
    pub description: String,
}

/// Public listing row for `GET /api/mods`.
#[derive(Clone, Debug, Serialize)]
pub struct ModListing {
    /// Catalog name.
    pub name: String,
    /// Manifest title.
    pub title: String,
    /// Content fingerprint (hex).
    pub fingerprint: String,
    /// Bytes.
    pub size: u64,
    /// Download path.
    pub url: String,
    /// Suitable for a local single-player run (lobby *Play solo*).
    pub solo: bool,
    /// Short blurb for lobby / play pickers.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Admin listing row: every catalog mod with its allow state.
#[derive(Clone, Debug, Serialize)]
pub struct AdminModRow {
    /// Public fields.
    #[serde(flatten)]
    pub listing: ModListing,
    /// Rooms may be created with it.
    pub allowed: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct PolicyFile {
    allowed: Vec<String>,
    /// Every mod the policy has seen. A mod added to the mods dir later (not
    /// listed here) follows the first-run seed instead of staying disallowed.
    /// Missing in older files: the allowed list stands in for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    known: Option<Vec<String>>,
}

/// Mods available on this node, keyed by name.
#[derive(Debug, Default)]
pub struct ModCatalog {
    entries: BTreeMap<String, ModEntry>,
    allowed: RwLock<BTreeSet<String>>,
    /// Mods the saved policy has decided on (see [`PolicyFile::known`]).
    known: BTreeSet<String>,
    policy_path: Option<PathBuf>,
}

impl ModCatalog {
    /// Load every `*.wad` in `dir` (non-recursive), all disallowed until
    /// [`Self::load_policy`] / [`Self::set_allowed`]. Unreadable / oversized /
    /// badly named files are skipped with a warning. Empty `dir` = no mods.
    #[must_use]
    pub fn load_dir(dir: &str) -> Self {
        let mut cat = Self::default();
        if dir.is_empty() {
            return cat;
        }
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(e) => {
                warn!(%dir, %e, "mods dir unreadable");
                return cat;
            }
        };
        let mut paths: Vec<PathBuf> = rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wad")))
            .collect();
        paths.sort();
        for path in paths {
            match load_file(&path) {
                Ok((name, bytes)) => match cat.add(&name, bytes, Some(path.clone())) {
                    Ok(info) => {
                        info!(mod_name = %info.name, title = %info.title, size = info.size, "mod loaded")
                    }
                    Err(e) => warn!(path = %path.display(), %e, "mod skipped"),
                },
                Err(e) => warn!(path = %path.display(), %e, "mod skipped"),
            }
        }
        cat
    }

    /// Add an allowed mod from bytes (tests / embedding).
    ///
    /// # Errors
    ///
    /// Invalid name, oversized file or not a PWAD with a `ROOMMOD` manifest.
    pub fn insert(
        &mut self,
        name: &str,
        bytes: Vec<u8>,
        path: Option<PathBuf>,
    ) -> Result<RoomModRef, String> {
        let info = self.add(name, bytes, path)?;
        self.allowed_mut().insert(info.name.clone());
        Ok(info)
    }

    /// Read the allow list from `<data_dir>/mods.json` and save later changes
    /// there. Without that file (first run), `seed` decides: mod names, or
    /// `"*"` for every mod. Mods added to the mods dir after the file was
    /// written follow `seed` too (and are then remembered), so a new mod
    /// shows up without an admin having to find it. Empty `data_dir` = in
    /// memory only.
    pub fn load_policy(&mut self, data_dir: &str, seed: &[String]) {
        let path = (!data_dir.is_empty()).then(|| Path::new(data_dir).join("mods.json"));
        let saved = path.as_deref().and_then(|p| {
            let bytes = std::fs::read(p).ok()?;
            match serde_json::from_slice::<PolicyFile>(&bytes) {
                Ok(f) => Some(f),
                Err(e) => {
                    warn!(path = %p.display(), %e, "mods.json unreadable; all mods disallowed");
                    Some(PolicyFile {
                        allowed: Vec::new(),
                        known: Some(self.entries.keys().cloned().collect()),
                    })
                }
            }
        });
        let seed_all = seed.iter().any(|s| s.trim() == "*");
        let seeded =
            |name: &str| seed_all || seed.iter().any(|s| s.trim().eq_ignore_ascii_case(name));
        let had_file = saved.is_some();
        let (mut names, known): (BTreeSet<String>, BTreeSet<String>) = match saved {
            Some(f) => {
                let allowed: BTreeSet<String> = f.allowed.into_iter().collect();
                let known = f
                    .known
                    .map_or_else(|| allowed.clone(), |k| k.into_iter().collect());
                (allowed, known)
            }
            None => (BTreeSet::new(), BTreeSet::new()),
        };
        let mut added = Vec::new();
        for name in self.entries.keys() {
            if !known.contains(name) && seeded(name) && names.insert(name.clone()) {
                added.push(name.clone());
            }
        }
        let allowed: BTreeSet<String> = names
            .into_iter()
            .filter(|n| self.entries.contains_key(n))
            .collect();
        info!(allowed = ?allowed, new = ?added, "mod allow list");
        *self.allowed_mut() = allowed;
        self.known = known;
        self.policy_path = path;
        if had_file && self.entries.keys().any(|n| !self.known.contains(n)) {
            // Remember the new mods so a later admin "disallow" sticks.
            let _ = self.save_policy();
        }
    }

    fn save_policy(&mut self) -> Result<(), String> {
        let Some(path) = &self.policy_path else {
            return Ok(());
        };
        let allowed = self
            .allowed
            .read()
            .map(|g| g.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        self.known.extend(self.entries.keys().cloned());
        let body = serde_json::to_vec_pretty(&PolicyFile {
            allowed,
            known: Some(self.known.iter().cloned().collect()),
        })
        .map_err(|e| e.to_string())?;
        std::fs::write(path, body).map_err(|e| {
            warn!(path = %path.display(), %e, "mods.json write failed");
            e.to_string()
        })
    }

    /// Allow / disallow a mod for new rooms (persisted when a data dir is set).
    ///
    /// # Errors
    ///
    /// Unknown mod name.
    pub fn set_allowed(&self, name: &str, allowed: bool) -> Result<(), String> {
        if !self.entries.contains_key(name) {
            return Err(format!("unknown mod {name:?}"));
        }
        let snapshot = {
            let mut g = self.allowed.write().unwrap_or_else(|p| p.into_inner());
            if allowed {
                g.insert(name.to_owned());
            } else {
                g.remove(name);
            }
            g.iter().cloned().collect::<Vec<_>>()
        };
        if let Some(path) = &self.policy_path {
            let mut known: BTreeSet<String> = self.known.clone();
            known.extend(self.entries.keys().cloned());
            let body = serde_json::to_vec_pretty(&PolicyFile {
                allowed: snapshot,
                known: Some(known.into_iter().collect()),
            })
            .map_err(|e| e.to_string())?;
            std::fs::write(path, body).map_err(|e| {
                warn!(path = %path.display(), %e, "mods.json write failed");
                e.to_string()
            })?;
        }
        Ok(())
    }

    /// Rooms may be created with `name`.
    #[must_use]
    pub fn is_allowed(&self, name: &str) -> bool {
        self.allowed
            .read()
            .map(|g| g.contains(name))
            .unwrap_or(false)
    }

    fn allowed_mut(&mut self) -> &mut BTreeSet<String> {
        self.allowed.get_mut().unwrap_or_else(|p| p.into_inner())
    }

    fn add(
        &mut self,
        name: &str,
        bytes: Vec<u8>,
        path: Option<PathBuf>,
    ) -> Result<RoomModRef, String> {
        if !is_valid_mod_name(name) {
            return Err(format!("invalid mod name {name:?} (use a-z 0-9 _ -)"));
        }
        if bytes.len() > MAX_MOD_BYTES {
            return Err(format!("{} bytes exceeds {MAX_MOD_BYTES}", bytes.len()));
        }
        let manifest = find_wad_lump(&bytes, "ROOMMOD").ok_or("no ROOMMOD manifest lump")?;
        let meta = manifest_meta(manifest);
        let title = meta.title.unwrap_or_else(|| name.to_owned());
        let info = RoomModRef {
            name: name.to_owned(),
            title,
            fingerprint: format!("{:016x}", content_fingerprint(&bytes)),
            size: bytes.len() as u64,
        };
        self.entries.insert(
            name.to_owned(),
            ModEntry {
                info: info.clone(),
                bytes: Bytes::from(bytes),
                path,
                solo: meta.solo,
                description: meta.description,
            },
        );
        Ok(info)
    }

    /// Look up by name (allowed or not).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ModEntry> {
        self.entries.get(name)
    }

    /// Public listing of allowed mods, sorted by name.
    #[must_use]
    pub fn listing(&self) -> Vec<ModListing> {
        self.admin_listing()
            .into_iter()
            .filter(|r| r.allowed)
            .map(|r| r.listing)
            .collect()
    }

    /// Every mod with its allow state, sorted by name.
    #[must_use]
    pub fn admin_listing(&self) -> Vec<AdminModRow> {
        self.entries
            .values()
            .map(|e| AdminModRow {
                listing: ModListing {
                    name: e.info.name.clone(),
                    title: e.info.title.clone(),
                    fingerprint: e.info.fingerprint.clone(),
                    size: e.info.size,
                    url: format!("/api/mods/{}.wad", e.info.name),
                    solo: e.solo,
                    description: e.description.clone(),
                },
                allowed: self.is_allowed(&e.info.name),
            })
            .collect()
    }

    /// Number of mods.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// No mods configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn load_file(path: &Path) -> Result<(String, Vec<u8>), String> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("non-UTF-8 file name")?
        .to_ascii_lowercase();
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_MOD_BYTES as u64 {
        return Err(format!("{} bytes exceeds {MAX_MOD_BYTES}", meta.len()));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok((name, bytes))
}

fn read_i32(b: &[u8], at: usize) -> Option<usize> {
    let raw: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
    usize::try_from(i32::from_le_bytes(raw)).ok()
}

/// Body of the first lump called `name` in a PWAD/IWAD.
fn find_wad_lump<'a>(wad: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let magic = wad.get(0..4)?;
    if magic != b"PWAD" && magic != b"IWAD" {
        return None;
    }
    let count = read_i32(wad, 4)?;
    let dir = read_i32(wad, 8)?;
    for i in 0..count.min(65_536) {
        let entry = dir.checked_add(i.checked_mul(16)?)?;
        let lump_name = wad.get(entry + 8..entry + 16)?;
        let end = lump_name.iter().position(|&c| c == 0).unwrap_or(8);
        if lump_name.get(..end)?.eq_ignore_ascii_case(name.as_bytes()) {
            let pos = read_i32(wad, entry)?;
            let size = read_i32(wad, entry + 4)?;
            return wad.get(pos..pos.checked_add(size)?);
        }
    }
    None
}

struct ManifestMeta {
    title: Option<String>,
    /// Default true when the key is omitted.
    solo: bool,
    description: String,
}

/// `[mod] name` / `solo` / `description` from the ROOMMOD manifest.
fn manifest_meta(toml: &[u8]) -> ManifestMeta {
    let mut meta = ManifestMeta {
        title: None,
        solo: true,
        description: String::new(),
    };
    let Ok(text) = std::str::from_utf8(toml) else {
        return meta;
    };
    let mut in_mod = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_mod = line == "[mod]";
            continue;
        }
        if !in_mod {
            continue;
        }
        if let Some(rest) = line.strip_prefix("description") {
            if let Some(value) = rest.trim_start().strip_prefix('=') {
                let v = value.trim().trim_matches('"').trim();
                if !v.is_empty() {
                    meta.description = v.chars().take(280).collect();
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("name") {
            if let Some(value) = rest.trim_start().strip_prefix('=') {
                let v = value.trim().trim_matches('"').trim();
                if !v.is_empty() {
                    meta.title = Some(v.chars().take(64).collect());
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("solo") {
            if let Some(value) = rest.trim_start().strip_prefix('=') {
                let v = value.trim().trim_matches('"').to_ascii_lowercase();
                meta.solo = v != "false" && v != "0" && v != "no";
            }
        }
    }
    meta
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use super::*;

    /// Minimal PWAD with the given lumps.
    pub(crate) fn pwad(lumps: &[(&str, &[u8])]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut dir = Vec::new();
        let mut off = 12u32;
        for (name, body) in lumps {
            let mut n = [0u8; 8];
            n[..name.len()].copy_from_slice(name.as_bytes());
            dir.extend_from_slice(&off.to_le_bytes());
            dir.extend_from_slice(&(body.len() as u32).to_le_bytes());
            dir.extend_from_slice(&n);
            data.extend_from_slice(body);
            off += body.len() as u32;
        }
        let mut out = b"PWAD".to_vec();
        out.extend_from_slice(&(lumps.len() as u32).to_le_bytes());
        out.extend_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&data);
        out.extend_from_slice(&dir);
        out
    }

    #[test]
    fn catalog_reads_title_and_fingerprint() {
        let manifest =
            b"[mod]\nid = \"x\"\nname = \"Bounty Hunter\"\n[[module]]\nname = \"bounty\"\n";
        let wad = pwad(&[("ROOMMOD", manifest), ("BOUNTY", b"\0asm")]);
        let mut cat = ModCatalog::default();
        let info = cat.insert("bounty", wad.clone(), None).unwrap();
        assert_eq!(info.title, "Bounty Hunter");
        assert_eq!(
            info.fingerprint,
            format!("{:016x}", content_fingerprint(&wad))
        );
        assert_eq!(cat.listing()[0].url, "/api/mods/bounty.wad");
        assert!(cat.listing()[0].solo);
    }

    #[test]
    fn catalog_reads_solo_false() {
        let manifest =
            b"[mod]\nname = \"Battle Royale\"\nsolo = false\n[[module]]\nname = \"royale\"\n";
        let wad = pwad(&[("ROOMMOD", manifest), ("ROYALE", b"\0asm")]);
        let mut cat = ModCatalog::default();
        cat.insert("royale", wad, None).unwrap();
        assert!(!cat.listing()[0].solo);
        assert_eq!(cat.listing()[0].title, "Battle Royale");
    }

    #[test]
    fn catalog_reads_description() {
        let manifest = b"[mod]\nname = \"Survival Director\"\ndescription = \"Waves and bosses.\"\n[[module]]\nname = \"survival\"\n";
        let wad = pwad(&[("ROOMMOD", manifest), ("SURVIVAL", b"\0asm")]);
        let mut cat = ModCatalog::default();
        cat.insert("survival", wad, None).unwrap();
        assert_eq!(cat.listing()[0].description, "Waves and bosses.");
    }

    #[test]
    fn catalog_rejects_bad_input() {
        let mut cat = ModCatalog::default();
        assert!(cat.insert("ok", b"not a wad".to_vec(), None).is_err());
        assert!(cat.insert("ok", pwad(&[("MAP01", b"")]), None).is_err());
        assert!(
            cat.insert("../evil", pwad(&[("ROOMMOD", b"")]), None)
                .is_err()
        );
        let mut truncated = pwad(&[("ROOMMOD", b"[mod]")]);
        truncated.truncate(20);
        assert!(cat.insert("ok", truncated, None).is_err());
        assert!(cat.is_empty());
    }

    #[test]
    fn mods_from_a_dir_need_allowing_and_the_choice_persists() {
        let root = std::env::temp_dir().join(format!("room-mods-policy-{}", std::process::id()));
        let (mods_dir, data_dir) = (root.join("mods"), root.join("data"));
        std::fs::create_dir_all(&mods_dir).unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();
        for name in ["alpha", "beta"] {
            let wad = pwad(&[("ROOMMOD", b"[mod]\nname = \"M\"\n")]);
            std::fs::write(mods_dir.join(format!("{name}.wad")), wad).unwrap();
        }
        let (mods, data) = (mods_dir.to_str().unwrap(), data_dir.to_str().unwrap());

        let mut cat = ModCatalog::load_dir(mods);
        assert_eq!(cat.len(), 2);
        assert!(cat.listing().is_empty());
        cat.load_policy(data, &["beta".into(), "ghost".into()]);
        assert_eq!(cat.listing()[0].name, "beta");
        assert!(cat.set_allowed("ghost", true).is_err());
        cat.set_allowed("alpha", true).unwrap();
        cat.set_allowed("beta", false).unwrap();

        let mut reloaded = ModCatalog::load_dir(mods);
        reloaded.load_policy(data, &["*".into()]);
        let rows = reloaded.admin_listing();
        assert_eq!(
            rows.iter()
                .map(|r| (r.listing.name.as_str(), r.allowed))
                .collect::<Vec<_>>(),
            [("alpha", true), ("beta", false)]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mods_added_after_the_policy_follow_the_seed() {
        let root = std::env::temp_dir().join(format!("room-mods-new-{}", std::process::id()));
        let (mods_dir, data_dir) = (root.join("mods"), root.join("data"));
        std::fs::create_dir_all(&mods_dir).unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();
        let write = |name: &str| {
            let wad = pwad(&[("ROOMMOD", b"[mod]\nname = \"M\"\n")]);
            std::fs::write(mods_dir.join(format!("{name}.wad")), wad).unwrap();
        };
        write("alpha");
        write("beta");
        // An older mods.json (no `known`): alpha allowed, beta disabled.
        std::fs::write(data_dir.join("mods.json"), br#"{"allowed":["alpha"]}"#).unwrap();
        let (mods, data) = (mods_dir.to_str().unwrap(), data_dir.to_str().unwrap());
        let allowed = |cat: &ModCatalog| {
            cat.admin_listing()
                .iter()
                .filter(|r| r.allowed)
                .map(|r| r.listing.name.clone())
                .collect::<Vec<_>>()
        };

        // A content mod dropped in later (Color World) shows up with `*`.
        write("colorworld");
        let mut cat = ModCatalog::load_dir(mods);
        cat.load_policy(data, &["*".into()]);
        assert!(allowed(&cat).contains(&"colorworld".to_owned()));
        assert!(allowed(&cat).contains(&"alpha".to_owned()));

        // Once seen, an admin's "disallow" sticks across restarts.
        cat.set_allowed("colorworld", false).unwrap();
        let mut again = ModCatalog::load_dir(mods);
        again.load_policy(data, &["*".into()]);
        assert!(!allowed(&again).contains(&"colorworld".to_owned()));

        // Without a seed a new mod stays off.
        write("rednoir");
        let mut strict = ModCatalog::load_dir(mods);
        strict.load_policy(data, &[]);
        assert!(!allowed(&strict).contains(&"rednoir".to_owned()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
