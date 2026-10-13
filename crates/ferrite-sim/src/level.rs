//! Per-level counters and flags (GZDoom `level_info_t` / `FLevelLocals` subset).

/// Mutable level-wide state advanced each tic.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelState {
    /// Tics since level start (`level.time`).
    pub time: u32,
    /// Map time (pauses while `frozen`).
    pub maptime: u32,
    /// Total time across hubs (`level.totaltime`).
    pub totaltime: u32,
    /// Gravity (map units / tic²); Doom default 1.0.
    pub gravity: f64,
    /// Air control while airborne (ACS `SetAirControl`; Doom default ≈ 0.00390625).
    pub air_control: f64,
    /// Frozen (cheat / ACS); thinkers still run but specials pause later.
    pub frozen: bool,
    /// Exit_Normal / Exit_Secret / Teleport_NewMap requested this level.
    pub exiting: bool,
    /// True if the pending exit is secret.
    pub secret_exit: bool,
    /// Explicit next map from `Teleport_NewMap` (e.g. `"MAP05"`); overrides MAPINFO/default.
    pub next_map_override: Option<String>,
    /// Secrets present on this map.
    pub total_secrets: u32,
    /// Secrets found so far.
    pub found_secrets: u32,
    /// Monsters present at start (countkill).
    pub total_kills: u32,
    /// Monsters killed.
    pub found_kills: u32,
    /// Countable items present at start.
    pub total_items: u32,
    /// Items collected.
    pub found_items: u32,
    /// MAPINFO `par` time in seconds (`GetLevelInfo` `LEVELINFO_PAR_TIME`).
    pub par_time: u32,
    /// MAPINFO suck time in seconds (`LEVELINFO_SUCK_TIME`); 0 if unset.
    pub suck_time: u32,
    /// MAPINFO cluster id (`LEVELINFO_CLUSTERNUM`); 0 if unset.
    pub cluster: i32,
    /// MAPINFO `levelnum` (`LEVELINFO_LEVELNUM`); 0 if unset.
    pub levelnum: i32,
    /// Hexen-style falling damage (`MAPINFO` fallingdamage / forcefallingdamage).
    pub falling_damage: bool,
}

impl Default for LevelState {
    fn default() -> Self {
        Self {
            time: 0,
            maptime: 0,
            totaltime: 0,
            gravity: 1.0,
            air_control: 0.003_906_25,
            frozen: false,
            exiting: false,
            secret_exit: false,
            next_map_override: None,
            total_secrets: 0,
            found_secrets: 0,
            total_kills: 0,
            found_kills: 0,
            total_items: 0,
            found_items: 0,
            par_time: 0,
            suck_time: 0,
            cluster: 0,
            levelnum: 0,
            falling_damage: false,
        }
    }
}

impl LevelState {
    /// Advance clocks by one tic (`P_Ticker` tail).
    /// `maptime` freezes while `frozen` (menu / ACS freeze); wall `time` still advances.
    pub fn tick(&mut self) {
        self.time = self.time.saturating_add(1);
        self.totaltime = self.totaltime.saturating_add(1);
        if !self.frozen {
            self.maptime = self.maptime.saturating_add(1);
        }
    }

    /// Percent helper (100 if total is 0).
    #[must_use]
    pub fn percent(found: u32, total: u32) -> u8 {
        match total {
            0 => 100,
            t => found
                .saturating_mul(100)
                .checked_div(t)
                .unwrap_or(100)
                .min(100) as u8,
        }
    }

    /// Kill completion percent.
    #[must_use]
    pub fn kills_percent(&self) -> u8 {
        Self::percent(self.found_kills, self.total_kills)
    }

    /// Item completion percent.
    #[must_use]
    pub fn items_percent(&self) -> u8 {
        Self::percent(self.found_items, self.total_items)
    }

    /// Secret completion percent.
    #[must_use]
    pub fn secrets_percent(&self) -> u8 {
        Self::percent(self.found_secrets, self.total_secrets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_holds_maptime_but_advances_time() {
        let mut lvl = LevelState::default();
        lvl.tick();
        assert_eq!(lvl.maptime, 1);
        assert_eq!(lvl.time, 1);
        lvl.frozen = true;
        lvl.tick();
        assert_eq!(lvl.maptime, 1);
        assert_eq!(lvl.time, 2);
    }
}
