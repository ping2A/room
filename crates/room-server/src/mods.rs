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
}

/// Mods available on this node, keyed by name.
#[derive(Debug, Default)]
pub struct ModCatalog {
    entries: BTreeMap<String, ModEntry>,
    allowed: RwLock<BTreeSet<String>>,
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
    /// `"*"` for every mod. Empty `data_dir` = in memory only.
    pub fn load_policy(&mut self, data_dir: &str, seed: &[String]) {
        let path = (!data_dir.is_empty()).then(|| Path::new(data_dir).join("mods.json"));
        let saved = path.as_deref().and_then(|p| {
            let bytes = std::fs::read(p).ok()?;
            match serde_json::from_slice::<PolicyFile>(&bytes) {
                Ok(f) => Some(f.allowed),
                Err(e) => {
                    warn!(path = %p.display(), %e, "mods.json unreadable; all mods disallowed");
                    Some(Vec::new())
                }
            }
        });
        let names: BTreeSet<String> = match saved {
            Some(list) => list.into_iter().collect(),
            None if seed.iter().any(|s| s.trim() == "*") => self.entries.keys().cloned().collect(),
            None => seed.iter().map(|s| s.trim().to_ascii_lowercase()).collect(),
        };
        let allowed: BTreeSet<String> = names
            .into_iter()
            .filter(|n| self.entries.contains_key(n))
            .collect();
        info!(allowed = ?allowed, "mod allow list");
        *self.allowed_mut() = allowed;
        self.policy_path = path;
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
            let body = serde_json::to_vec_pretty(&PolicyFile { allowed: snapshot })
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
        let title = manifest_title(manifest).unwrap_or_else(|| name.to_owned());
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

/// `name = "…"` from the manifest's `[mod]` table.
fn manifest_title(toml: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(toml).ok()?;
    let mut in_mod = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_mod = line == "[mod]";
            continue;
        }
        let Some(rest) = line.strip_prefix("name") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        if in_mod {
            let v = value.trim().trim_matches('"').trim();
            if !v.is_empty() {
                return Some(v.chars().take(64).collect());
            }
        }
    }
    None
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
}
