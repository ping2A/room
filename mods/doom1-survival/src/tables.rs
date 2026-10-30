//! Monster pool, tuning, and music tables (`docs/27`).

/// Tics per second.
pub const TICRATE: i32 = 35;
/// Spawn-spot capacity (`docs/27` §6.1).
pub const MAX_SPOTS: usize = 256;
/// Live wave-monster tracking capacity.
pub const MAX_WAVE: usize = 24;
/// Kills closer together than this chain into a streak.
pub const STREAK_TICS: i32 = 3 * TICRATE;
/// Spawn spots closer than this to **every** alive player are skipped.
pub const MIN_SPAWN_DIST: f64 = 384.0;
/// Prefer spots within this of the nearest alive player (widened after fails).
pub const MAX_SPAWN_DIST: f64 = 2048.0;
/// Stillness window before stuck recycle (`docs/27` §6.1).
pub const STUCK_STILL_TICS: i32 = 12 * TICRATE;
/// How often stuck / learning runs.
pub const STUCK_CHECK_PERIOD: i32 = TICRATE;
/// Movement threshold (map units) over the stillness window.
pub const STUCK_MOVE_EPS: f64 = 16.0;
/// After this many fails with a bad score, disable the spot for the map.
pub const SPOT_DISABLE_FAILS: u16 = 3;
/// Score floor for disable (with fails).
pub const SPOT_DISABLE_SCORE: i16 = -16;
/// Clear-phase straggler timeout (wave age).
pub const STRAGGLER_TICS: i32 = 30 * TICRATE;
/// Grid size for spot dedupe.
pub const SPOT_GRID: f64 = 64.0;
/// Minimum open height at a spot.
pub const SPOT_MIN_HEIGHT: f64 = 64.0;

/// Per-skill tuning (index = skill − 1).
#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    /// Seconds between waves (Warmup / Intermission baseline).
    pub wave_gap: i32,
    /// Extra monsters per wave (legacy size helper).
    pub extra: i32,
    /// Health restored per kill.
    pub heal: i32,
    /// Threat / budget skill multiplier (%).
    pub skill_mul: i32,
    /// Elite chance per group (%).
    pub elite_chance: i32,
    /// Last combat round before extraction (`docs/27` §13).
    pub final_round: i32,
    /// Objective chance (%).
    pub objective_chance: i32,
}

/// ITYTD → Nightmare.
///
/// ITYTD `final_round = 2` keeps SV5 extraction e2e short; raise for production balance.
pub const TUNING: [Tuning; 5] = [
    Tuning {
        wave_gap: 60,
        extra: 0,
        heal: 5,
        skill_mul: 60,
        elite_chance: 5,
        final_round: 2,
        objective_chance: 40,
    },
    Tuning {
        wave_gap: 50,
        extra: 1,
        heal: 4,
        skill_mul: 80,
        elite_chance: 10,
        final_round: 10,
        objective_chance: 50,
    },
    Tuning {
        wave_gap: 40,
        extra: 2,
        heal: 3,
        skill_mul: 100,
        elite_chance: 15,
        final_round: 15,
        objective_chance: 50,
    },
    Tuning {
        wave_gap: 32,
        extra: 3,
        heal: 2,
        skill_mul: 130,
        elite_chance: 22,
        final_round: 15,
        objective_chance: 55,
    },
    Tuning {
        wave_gap: 25,
        extra: 5,
        heal: 1,
        skill_mul: 170,
        elite_chance: 30,
        final_round: 20,
        objective_chance: 60,
    },
];

/// Monster pool, unlocked one entry per wave (shareware-safe names).
pub const POOL: [&str; 6] = [
    "ZombieMan",
    "ShotgunGuy",
    "PhantomImp",
    "Demon",
    "Spectre",
    "Juggernaut",
];

/// Threat cost per pool index (and Baron).
pub const POOL_THREAT: [i32; 6] = [2, 3, 4, 4, 5, 10];
/// Baron threat cost.
pub const BARON_THREAT: i32 = 18;
/// Intensity peak threshold (×100 fixed).
pub const PEAK_INTENSITY: i32 = 7000;
/// Build-up intensity soft cap.
pub const BUILDUP_INTENSITY: i32 = 3000;
/// Peak-fade hold tics after intensity spike.
pub const PEAK_FADE_TICS: i32 = 8 * TICRATE;
/// Build-up duration.
pub const BUILDUP_TICS: i32 = 8 * TICRATE;
/// Intermission duration baseline (scaled by skill gap).
pub const INTERMISSION_TICS: i32 = 18 * TICRATE;

/// Round budget `B(r)` (`docs/27` §6.2).
#[must_use]
pub fn round_budget(round: i32, skill: i32, players: i32) -> i32 {
    let r = round.max(1);
    let base = 12 + 6 * r + (r * r) / 4;
    let sm = tuning_for_skill(skill).skill_mul;
    let pm = 100 + 55 * (players.max(1) - 1);
    (base * sm * pm) / 10_000
}

/// Simultaneous alive-threat cap `C(r)`.
#[must_use]
pub fn alive_cap(round: i32, skill: i32, players: i32) -> i32 {
    let r = round.max(1);
    let base = 20 + 3 * r;
    let sm = tuning_for_skill(skill).skill_mul;
    let pm = 100 + 55 * (players.max(1) - 1);
    ((base * sm * pm) / 10_000).min(64 * 4) // soft bound; actor hard cap is MAX_WAVE
}

/// DoomEdNums preferred as spawn spots (monsters + player/DM starts).
pub const SPOT_EDNUMS: &[i32] = &[
    // Player starts 1–4, deathmatch.
    1, 2, 3, 4, 11, // Doom 1 monsters
    3004, 9, 3001, 3002, 58, 3006, 3005, 3003, // Doom 2 extras (harmless if absent)
    65, 66, 67, 68, 69, 71, 64, 16, 7, 9, 84,
];

/// Tuning for Doom skill `skill` (1..=5); out of range clamps.
#[must_use]
pub fn tuning_for_skill(skill: i32) -> &'static Tuning {
    let i = usize::try_from(skill - 1).unwrap_or(0).min(4);
    &TUNING[i]
}

/// Map music lump for restore after boss (`ExMy` / `MAPxx`).
///
/// Returns bytes written into `out` (0 = unknown — do not change music).
#[must_use]
pub fn map_music_name(map: &str, out: &mut [u8; 8]) -> usize {
    let m = map.as_bytes();
    if m.len() == 4 && m[0] == b'E' && m[2] == b'M' {
        out[..2].copy_from_slice(b"D_");
        out[2..6].copy_from_slice(m);
        return 6;
    }
    if m.len() == 5 && m.starts_with(b"MAP") {
        let n = (m[3].wrapping_sub(b'0')) as usize * 10 + (m[4].wrapping_sub(b'0')) as usize;
        if let Some(track) = DOOM2_MUSIC.get(n.wrapping_sub(1)) {
            let b = track.as_bytes();
            let len = b.len().min(out.len());
            out[..len].copy_from_slice(&b[..len]);
            return len;
        }
    }
    0
}

/// Doom II `MAPxx` → music lump (1-based index via `get(n-1)`).
const DOOM2_MUSIC: [&str; 32] = [
    "D_RUNNIN", "D_STALKS", "D_COUNTD", "D_BETWEE", "D_DOOM", "D_THE_DA", "D_SHAWN", "D_DDTBLU",
    "D_IN_CIT", "D_DEAD", "D_STLKS2", "D_THEDA2", "D_DOOM2", "D_DDTBL2", "D_RUNNI2", "D_DEAD2",
    "D_STLKS3", "D_ROMERO", "D_SHAWN2", "D_MESSAG", "D_COUNT2", "D_DDTBL3", "D_AMPIE", "D_THEDA3",
    "D_ADRIAN", "D_MESSG2", "D_ROMER2", "D_TENSE", "D_SHAWN3", "D_OPENIN", "D_EVIL", "D_ULTIMA",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuning_matches_legacy_table() {
        assert_eq!(tuning_for_skill(1).wave_gap, 60);
        assert_eq!(tuning_for_skill(3).extra, 2);
        assert_eq!(tuning_for_skill(4).heal, 2);
        assert_eq!(tuning_for_skill(5).wave_gap, 25);
        assert_eq!(tuning_for_skill(0).wave_gap, 60);
        assert_eq!(tuning_for_skill(99).wave_gap, 25);
    }

    #[test]
    fn first_wave_delay_is_half_gap() {
        assert_eq!(tuning_for_skill(4).wave_gap * TICRATE / 2, 560);
        assert_eq!(tuning_for_skill(5).wave_gap * TICRATE / 2, 437);
        assert_eq!(tuning_for_skill(3).wave_gap * TICRATE / 2, 700);
    }

    #[test]
    fn map_music_exmy_and_mapxx() {
        let mut buf = [0u8; 8];
        assert_eq!(map_music_name("E1M1", &mut buf), 6);
        assert_eq!(&buf[..6], b"D_E1M1");
        assert_eq!(map_music_name("MAP01", &mut buf), 8);
        assert_eq!(&buf[..8], b"D_RUNNIN");
        assert_eq!(map_music_name("MAP07", &mut buf), 7);
        assert_eq!(&buf[..7], b"D_SHAWN");
        assert_eq!(map_music_name("HUB01", &mut buf), 0);
    }
}
