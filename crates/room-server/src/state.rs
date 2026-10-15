//! Shared HTTP / admin app state.

use crate::registry::RoomRegistry;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// Shared app state for public + admin routes.
#[derive(Clone)]
pub struct AppState {
    /// Room registry.
    pub registry: RoomRegistry,
    /// Admin audit / bans.
    pub admin: AdminStore,
}

/// Append-only audit entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unix seconds.
    pub ts: u64,
    /// Actor (admin user id or `"system"`).
    pub actor: String,
    /// Action name.
    pub action: String,
    /// Target (room id / seat / ban id).
    pub target: String,
    /// Free-form detail.
    pub detail: String,
}

/// A ban row.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ban {
    /// Ban id.
    pub id: String,
    /// Scope (`ip`, `name`, `fingerprint`).
    pub scope: String,
    /// Value for the scope.
    pub value: String,
    /// Reason.
    pub reason: String,
    /// Expiry unix seconds (`0` = permanent).
    pub exp: u64,
}

/// Audit + ban store with optional JSON file persistence (`--data-dir`).
#[derive(Clone)]
pub struct AdminStore {
    audit: Arc<Mutex<Vec<AuditEntry>>>,
    bans: Arc<Mutex<Vec<Ban>>>,
    data_dir: Option<PathBuf>,
}

impl Default for AdminStore {
    fn default() -> Self {
        Self {
            audit: Arc::new(Mutex::new(Vec::new())),
            bans: Arc::new(Mutex::new(Vec::new())),
            data_dir: None,
        }
    }
}

impl AdminStore {
    /// Load (or create) a store under `data_dir`.
    ///
    /// # Errors
    ///
    /// I/O failures creating the directory or reading existing files.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        let mut store = Self {
            audit: Arc::new(Mutex::new(Vec::new())),
            bans: Arc::new(Mutex::new(Vec::new())),
            data_dir: Some(dir.clone()),
        };
        store.load_from_disk()?;
        info!(path = %dir.display(), "admin store opened");
        Ok(store)
    }

    fn audit_path(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join("audit.jsonl"))
    }

    fn bans_path(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join("bans.json"))
    }

    fn load_from_disk(&mut self) -> Result<(), std::io::Error> {
        if let Some(path) = self.audit_path() {
            if path.exists() {
                let file = File::open(&path)?;
                let mut entries = Vec::new();
                for line in BufReader::new(file).lines() {
                    let line = line?;
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<AuditEntry>(&line) {
                        Ok(e) => entries.push(e),
                        Err(e) => warn!(?e, "skip bad audit line"),
                    }
                }
                if entries.len() > 10_000 {
                    entries = entries.split_off(entries.len() - 10_000);
                }
                if let Ok(mut g) = self.audit.lock() {
                    *g = entries;
                }
            }
        }
        if let Some(path) = self.bans_path() {
            if path.exists() {
                let bytes = fs::read(&path)?;
                match serde_json::from_slice::<Vec<Ban>>(&bytes) {
                    Ok(bans) => {
                        if let Ok(mut g) = self.bans.lock() {
                            *g = bans;
                        }
                    }
                    Err(e) => warn!(?e, "bans.json parse failed"),
                }
            }
        }
        Ok(())
    }

    fn append_audit_line(&self, entry: &AuditEntry) {
        let Some(path) = self.audit_path() else {
            return;
        };
        let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) else {
            warn!(path = %path.display(), "audit append open failed");
            return;
        };
        if let Ok(line) = serde_json::to_string(entry) {
            let _ = writeln!(f, "{line}");
        }
    }

    fn rewrite_bans(&self, bans: &[Ban]) {
        let Some(path) = self.bans_path() else {
            return;
        };
        match serde_json::to_vec_pretty(bans) {
            Ok(bytes) => {
                if let Err(e) = fs::write(&path, bytes) {
                    warn!(?e, path = %path.display(), "bans write failed");
                }
            }
            Err(e) => warn!(?e, "bans serialize failed"),
        }
    }

    /// Record an audited action.
    pub fn audit(&self, actor: &str, action: &str, target: &str, detail: &str) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = AuditEntry {
            ts,
            actor: actor.into(),
            action: action.into(),
            target: target.into(),
            detail: detail.into(),
        };
        if let Ok(mut g) = self.audit.lock() {
            g.push(entry.clone());
            if g.len() > 10_000 {
                let drain = g.len() - 10_000;
                g.drain(0..drain);
            }
        }
        self.append_audit_line(&entry);
    }

    /// Snapshot of audit log (newest last).
    #[must_use]
    pub fn audit_list(&self) -> Vec<AuditEntry> {
        self.audit.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// List bans.
    #[must_use]
    pub fn bans(&self) -> Vec<Ban> {
        self.bans.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Add a ban.
    pub fn add_ban(&self, ban: Ban) {
        if let Ok(mut g) = self.bans.lock() {
            g.push(ban);
            let snapshot = g.clone();
            drop(g);
            self.rewrite_bans(&snapshot);
        }
    }

    /// Remove a ban by id. Returns `true` when a row was deleted.
    pub fn remove_ban(&self, id: &str) -> bool {
        let Ok(mut g) = self.bans.lock() else {
            return false;
        };
        let before = g.len();
        g.retain(|b| b.id != id);
        let removed = g.len() != before;
        if removed {
            let snapshot = g.clone();
            drop(g);
            self.rewrite_bans(&snapshot);
        }
        removed
    }

    /// First active ban matching `subject`, if any.
    #[must_use]
    pub fn ban_hit(&self, subject: &crate::bans::BanSubject<'_>) -> Option<crate::bans::BanHit> {
        let bans = self.bans();
        crate::bans::find_ban_hit(&bans, subject)
    }
}
