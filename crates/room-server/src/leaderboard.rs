//! Top players: match results agreed by every seat of a room, persisted in
//! `<data-dir>/leaderboard.json` (docs/22 §4 "Match end").

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::warn;

/// Names kept at most (the least active rows are dropped beyond this).
const MAX_ROWS: usize = 5_000;

/// One player's totals.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerStats {
    /// Display name (latest spelling).
    pub name: String,
    /// Finished matches.
    pub matches: u32,
    /// Matches won.
    pub wins: u32,
    /// Frags over all matches.
    pub frags: i64,
    /// Deaths over all matches.
    pub deaths: i64,
    /// Unix seconds of the latest match.
    pub last_played: u64,
}

/// One finished seat of a match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchRow {
    /// Seat display name.
    pub name: String,
    /// Won the match.
    pub won: bool,
    /// Frags this match.
    pub frags: i32,
    /// Deaths this match.
    pub deaths: i32,
}

#[derive(Default, Serialize, Deserialize)]
struct LeaderFile {
    players: Vec<PlayerStats>,
}

/// Process-wide leaderboard (cheap to share behind an `Arc`).
#[derive(Default)]
pub struct Leaderboard {
    path: Option<PathBuf>,
    rows: Mutex<BTreeMap<String, PlayerStats>>,
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

impl Leaderboard {
    /// Load `<data_dir>/leaderboard.json` (empty `data_dir` = in memory only).
    #[must_use]
    pub fn open(data_dir: &str) -> Self {
        let path = (!data_dir.is_empty()).then(|| Path::new(data_dir).join("leaderboard.json"));
        let mut rows = BTreeMap::new();
        if let Some(p) = &path {
            if let Ok(bytes) = std::fs::read(p) {
                match serde_json::from_slice::<LeaderFile>(&bytes) {
                    Ok(f) => {
                        for s in f.players {
                            rows.insert(key(&s.name), s);
                        }
                    }
                    Err(e) => warn!(path = %p.display(), %e, "leaderboard.json unreadable"),
                }
            }
        }
        Self {
            path,
            rows: Mutex::new(rows),
        }
    }

    /// Add one match (ignored with fewer than two named seats).
    pub fn record(&self, rows: &[MatchRow], now: u64) {
        let named: Vec<&MatchRow> = rows.iter().filter(|r| !key(&r.name).is_empty()).collect();
        if named.len() < 2 {
            return;
        }
        let snapshot = {
            let mut g = self.rows.lock().unwrap_or_else(|p| p.into_inner());
            for r in named {
                let s = g.entry(key(&r.name)).or_default();
                s.name = r.name.trim().to_owned();
                s.matches = s.matches.saturating_add(1);
                s.wins = s.wins.saturating_add(u32::from(r.won));
                s.frags = s.frags.saturating_add(i64::from(r.frags));
                s.deaths = s.deaths.saturating_add(i64::from(r.deaths.max(0)));
                s.last_played = now;
            }
            if g.len() > MAX_ROWS {
                let mut by_age: Vec<(u64, String)> =
                    g.iter().map(|(k, s)| (s.last_played, k.clone())).collect();
                by_age.sort();
                for (_, k) in by_age.into_iter().take(g.len() - MAX_ROWS) {
                    g.remove(&k);
                }
            }
            LeaderFile {
                players: g.values().cloned().collect(),
            }
        };
        if let Some(p) = &self.path {
            let tmp = p.with_extension("json.tmp");
            let ok = serde_json::to_vec(&snapshot)
                .ok()
                .and_then(|b| std::fs::write(&tmp, b).ok())
                .and_then(|()| std::fs::rename(&tmp, p).ok());
            if ok.is_none() {
                warn!(path = %p.display(), "leaderboard.json write failed");
            }
        }
    }

    /// Best `n` players: wins, then frags, then fewer deaths, then name.
    #[must_use]
    pub fn top(&self, n: usize) -> Vec<PlayerStats> {
        let g = self.rows.lock().unwrap_or_else(|p| p.into_inner());
        let mut all: Vec<PlayerStats> = g.values().cloned().collect();
        all.sort_by(|a, b| {
            b.wins
                .cmp(&a.wins)
                .then(b.frags.cmp(&a.frags))
                .then(a.deaths.cmp(&b.deaths))
                .then(a.name.cmp(&b.name))
        });
        all.truncate(n);
        all
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn row(name: &str, won: bool, frags: i32, deaths: i32) -> MatchRow {
        MatchRow {
            name: name.into(),
            won,
            frags,
            deaths,
        }
    }

    #[test]
    fn ranks_by_wins_then_frags_and_persists() {
        let dir = std::env::temp_dir().join(format!("room-lb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_str().unwrap();
        let lb = Leaderboard::open(d);
        lb.record(&[row("Ann", true, 3, 0), row("bob", false, 1, 1)], 10);
        lb.record(&[row("ann", false, 0, 1), row("Bob", true, 1, 0)], 11);
        lb.record(&[row("Cid", true, 9, 0), row("solo", false, 0, 0)], 12);
        lb.record(&[row("lonely", true, 5, 0)], 13);
        let top = lb.top(10);
        let names: Vec<&str> = top.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Cid", "ann", "Bob", "solo"], "lonely: no rival");
        assert_eq!((top[1].matches, top[1].wins, top[1].frags), (2, 1, 3));
        let again = Leaderboard::open(d);
        assert_eq!(again.top(10), top);
        assert_eq!(again.top(1).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
