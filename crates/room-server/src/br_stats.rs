//! Battle Royale match history and ratings (`docs/28` S-BR1 / S-BR2).
//!
//! Aggregates per callsign from agreed `result` standings (BE11 fields when
//! present). Persisted at `<data-dir>/br_stats.json` (async Postgres in full
//! `docs/22` is out of scope for this MVP).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::warn;

const MAX_ROWS: usize = 5_000;

/// Per-mode bucket (solo / duo / squad deferred — MVP uses `"br"` only).
pub const MODE_BR: &str = "br";

/// One seat row ingested from a finished match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrMatchRow {
    /// Display name (callsign).
    pub name: String,
    /// 1-based place.
    pub place: u8,
    /// Won the match.
    pub won: bool,
    /// Kills (frags in protocol).
    pub kills: i32,
    /// Damage dealt (BE11).
    pub damage: i32,
}

/// Cumulative stats for the lobby leaderboard.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrPlayerStats {
    /// Latest spelling of the callsign.
    pub name: String,
    /// Finished BR matches.
    pub matches: u32,
    /// First-place finishes.
    pub wins: u32,
    /// Finishes in places 1..=5.
    pub top5s: u32,
    /// Sum of kills.
    pub kills: i64,
    /// Sum of damage dealt.
    pub damage: i64,
    /// Sum of places (avg = place_sum / matches).
    pub place_sum: i64,
    /// Simple placement rating (higher is better).
    pub rating: i32,
    /// Unix seconds of the latest match.
    pub last_played: u64,
}

impl BrPlayerStats {
    /// Average finish (1 = best).
    #[must_use]
    pub fn avg_place(&self) -> f64 {
        if self.matches == 0 {
            return 0.0;
        }
        self.place_sum as f64 / f64::from(self.matches)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct ModeFile {
    players: Vec<BrPlayerStats>,
}

#[derive(Default, Serialize, Deserialize)]
struct BrFile {
    #[serde(default)]
    modes: BTreeMap<String, ModeFile>,
}

/// Shared BR stats store.
#[derive(Default)]
pub struct BrStats {
    path: Option<PathBuf>,
    modes: Mutex<BTreeMap<String, BTreeMap<String, BrPlayerStats>>>,
}

fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// MVP rating delta from placement (OpenSkill deferred).
fn rating_delta(place: u8, seats: usize, won: bool) -> i32 {
    let n = seats.clamp(2, 8) as i32;
    let expected = (n + 1) / 2;
    let actual = i32::from(place);
    let mut d = (expected - actual) * 12;
    if won {
        d += 20;
    }
    d
}

impl BrStats {
    /// Load `<data_dir>/br_stats.json` (empty dir = memory only).
    #[must_use]
    pub fn open(data_dir: &str) -> Self {
        let path = (!data_dir.is_empty()).then(|| Path::new(data_dir).join("br_stats.json"));
        let mut modes = BTreeMap::new();
        if let Some(p) = &path {
            if let Ok(bytes) = std::fs::read(p) {
                match serde_json::from_slice::<BrFile>(&bytes) {
                    Ok(f) => {
                        for (mode, file) in f.modes {
                            let mut map = BTreeMap::new();
                            for s in file.players {
                                map.insert(key(&s.name), s);
                            }
                            modes.insert(mode, map);
                        }
                    }
                    Err(e) => warn!(path = %p.display(), %e, "br_stats.json unreadable"),
                }
            }
        }
        Self {
            path,
            modes: Mutex::new(modes),
        }
    }

    /// Record one BR match (`mode` is typically [`MODE_BR`]).
    pub fn record_match(&self, mode: &str, rows: &[BrMatchRow], now: u64) {
        let named: Vec<&BrMatchRow> = rows.iter().filter(|r| !key(&r.name).is_empty()).collect();
        if named.len() < 2 {
            return;
        }
        let seats = named.len();
        let snapshot = {
            let mut g = self.modes.lock().unwrap_or_else(|p| p.into_inner());
            let bucket = g.entry(mode.to_owned()).or_default();
            for r in named {
                let s = bucket.entry(key(&r.name)).or_default();
                s.name = r.name.trim().to_owned();
                s.matches = s.matches.saturating_add(1);
                s.wins = s.wins.saturating_add(u32::from(r.won));
                if r.place <= 5 {
                    s.top5s = s.top5s.saturating_add(1);
                }
                s.kills = s.kills.saturating_add(i64::from(r.kills.max(0)));
                s.damage = s.damage.saturating_add(i64::from(r.damage.max(0)));
                s.place_sum = s.place_sum.saturating_add(i64::from(r.place));
                s.rating = s.rating.saturating_add(rating_delta(r.place, seats, r.won));
                s.last_played = now;
            }
            if bucket.len() > MAX_ROWS {
                let mut by_age: Vec<(u64, String)> = bucket
                    .iter()
                    .map(|(k, s)| (s.last_played, k.clone()))
                    .collect();
                by_age.sort();
                for (_, k) in by_age.into_iter().take(bucket.len() - MAX_ROWS) {
                    bucket.remove(&k);
                }
            }
            BrFile {
                modes: g
                    .iter()
                    .map(|(mode, map)| {
                        (
                            mode.clone(),
                            ModeFile {
                                players: map.values().cloned().collect(),
                            },
                        )
                    })
                    .collect(),
            }
        };
        if let Some(p) = &self.path {
            let tmp = p.with_extension("json.tmp");
            let ok = serde_json::to_vec(&snapshot)
                .ok()
                .and_then(|b| std::fs::write(&tmp, b).ok())
                .and_then(|()| std::fs::rename(&tmp, p).ok());
            if ok.is_none() {
                warn!(path = %p.display(), "br_stats.json write failed");
            }
        }
    }

    /// Top `n` by rating, then wins, then kills.
    #[must_use]
    pub fn top(&self, mode: &str, n: usize) -> Vec<BrPlayerStats> {
        let g = self.modes.lock().unwrap_or_else(|p| p.into_inner());
        let Some(bucket) = g.get(mode) else {
            return Vec::new();
        };
        let mut all: Vec<BrPlayerStats> = bucket.values().cloned().collect();
        all.sort_by(|a, b| {
            b.rating
                .cmp(&a.rating)
                .then(b.wins.cmp(&a.wins))
                .then(b.kills.cmp(&a.kills))
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

    fn row(name: &str, place: u8, won: bool, kills: i32, damage: i32) -> BrMatchRow {
        BrMatchRow {
            name: name.into(),
            place,
            won,
            kills,
            damage,
        }
    }

    #[test]
    fn aggregates_and_persists_br_stats() {
        let dir = std::env::temp_dir().join(format!("room-br-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_str().unwrap();
        let st = BrStats::open(d);
        st.record_match(
            MODE_BR,
            &[row("Ann", 1, true, 3, 120), row("bob", 2, false, 1, 40)],
            10,
        );
        st.record_match(
            MODE_BR,
            &[row("ann", 3, false, 0, 10), row("Cid", 1, true, 5, 200)],
            11,
        );
        let top = st.top(MODE_BR, 10);
        let ann = top
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case("ann"))
            .unwrap();
        assert_eq!((ann.matches, ann.wins), (2, 1));
        assert!(ann.kills >= 3);
        let cid = top.iter().find(|s| s.name == "Cid").unwrap();
        assert_eq!(cid.wins, 1);
        assert!(top.len() >= 2);
        let again = BrStats::open(d);
        assert_eq!(again.top(MODE_BR, 2), top[..2].to_vec());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
