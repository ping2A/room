//! Loot counts and timings.
//!
//! Balance (`docs/28` §6.2–6.3): loot grace matches the §5 loot phase; zone rows
//! follow the published hold/shrink/damage table; gun spacing and caps scale for
//! 8-player / large maps (e.g. **ROYALE01**).

pub const TICRATE: i32 = 35;
pub const MAX_PLAYERS: usize = 8;
pub const MAX_SPOTS: usize = 256;
/// Seconds after deploy before the zone starts closing (loot grace; §5 loot phase).
pub const GRACE_SECS: i32 = 60;
/// Pre-game countdown once all contenders are ready.
pub const PREGAME_COUNTDOWN_SECS: i32 = 10;
/// Deploy / landing window.
pub const DEPLOY_SECS: i32 = 3;
/// Legacy delay before shells; resolution uses victory + standings MODALs (BR7).
pub const END_SECS: i32 = 7;
/// Guns are placed at least this far apart when the map has room.
pub const GUN_SPACING: f64 = 448.0;
/// Weapons placed once per match (scaled to the number of contenders, at
/// least 2). Nothing is dropped or added afterwards.
pub fn gun_counts(players: usize) -> [(Item, usize); 4] {
    let p = players.max(2);
    [
        (Item::RocketLauncher, p.div_ceil(4)),
        (Item::Chaingun, p.div_ceil(2)),
        (Item::Shotgun, p),
        (Item::Chainsaw, 1),
    ]
}

/// Ammo, health and armour placed alongside the guns.
pub fn supply_counts(players: usize) -> [(Item, usize); 8] {
    let p = players.max(2);
    [
        (Item::Shell, p),
        (Item::Stimpack, p),
        (Item::Clip, p),
        (Item::ArmorBonus, p),
        (Item::Medikit, p.div_ceil(2)),
        (Item::GreenArmor, p.div_ceil(2)),
        (Item::RocketAmmo, p.div_ceil(4)),
        (Item::Berserk, 1),
    ]
}

/// Most loot pieces a match places.
pub const MAX_LOOT: usize = 96;

/// Health for a frag.
pub const FRAG_HEAL: i32 = 25;

/// One row of the zone schedule (§6.3); `radius_frac` is target radius as a
/// share of the initial radius `r0` (final row → 0).
#[derive(Clone, Copy, Debug)]
pub struct ZoneRow {
    pub hold_secs: i32,
    pub shrink_secs: i32,
    pub radius_frac: f64,
    pub damage_per_sec: i32,
}

pub const ZONE_ROWS: [ZoneRow; 6] = [
    ZoneRow {
        hold_secs: 60,
        shrink_secs: 40,
        radius_frac: 0.6,
        damage_per_sec: 2,
    },
    ZoneRow {
        hold_secs: 45,
        shrink_secs: 30,
        radius_frac: 0.55,
        damage_per_sec: 4,
    },
    ZoneRow {
        hold_secs: 35,
        shrink_secs: 25,
        radius_frac: 0.5,
        damage_per_sec: 7,
    },
    ZoneRow {
        hold_secs: 25,
        shrink_secs: 20,
        radius_frac: 0.45,
        damage_per_sec: 11,
    },
    ZoneRow {
        hold_secs: 20,
        shrink_secs: 20,
        radius_frac: 0.4,
        damage_per_sec: 16,
    },
    ZoneRow {
        hold_secs: 10,
        shrink_secs: 60,
        radius_frac: 0.0,
        damage_per_sec: 25,
    },
];

/// Whole zone timeline from loot start: grace, then every hold and shrink
/// (seconds).
pub fn zone_schedule_secs() -> i32 {
    GRACE_SECS
        + ZONE_ROWS
            .iter()
            .map(|r| r.hold_secs + r.shrink_secs)
            .sum::<i32>()
}

/// Fastest the zone may run (× normal speed).
pub const MAX_ZONE_PACE_MILLI: i32 = 30_000;

/// Zone speed for a match with `time_left_tics` to go at loot start
/// (`1000` = normal). A time limit shorter than the zone timeline speeds
/// the zone up so it is fully closed a little before time runs out (the
/// last fighters are decided by the zone, not the clock). `0` = no limit.
pub fn zone_pace_milli(time_left_tics: i32) -> i32 {
    if time_left_tics <= 0 {
        return 1000;
    }
    // Keep a sixth of the time (5..20 s) after the zone has closed.
    let margin = (time_left_tics / 6).clamp(5 * TICRATE, 20 * TICRATE);
    let budget = (time_left_tics - margin).max(10 * TICRATE);
    let full = i64::from(zone_schedule_secs() * TICRATE);
    let pace = (full * 1000 + i64::from(budget) - 1) / i64::from(budget);
    (pace as i32).clamp(1000, MAX_ZONE_PACE_MILLI)
}

/// Zone damage per second for gameplay phase `phase` (1..=6).
pub fn zone_damage_for_phase(phase: i32) -> i32 {
    if phase <= 0 {
        return 0;
    }
    let i = (phase as usize).saturating_sub(1).min(ZONE_ROWS.len() - 1);
    ZONE_ROWS[i].damage_per_sec
}

/// Seconds from loot start until the first shrink begins (grace + phase 1 hold).
#[cfg(test)]
pub fn secs_until_first_shrink() -> i32 {
    GRACE_SECS + ZONE_ROWS[0].hold_secs
}

/// Every item the mod places (all present in shareware Doom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Item {
    Shotgun,
    Chaingun,
    RocketLauncher,
    Chainsaw,
    Shell,
    Clip,
    RocketAmmo,
    Stimpack,
    Medikit,
    GreenArmor,
    ArmorBonus,
    Berserk,
}

pub const ITEM_CLASSES: [&str; 12] = [
    "Shotgun",
    "Chaingun",
    "RocketLauncher",
    "Chainsaw",
    "Shell",
    "Clip",
    "RocketAmmo",
    "Stimpack",
    "Medikit",
    "GreenArmor",
    "ArmorBonus",
    "Berserk",
];

/// Doom editor numbers of every pickup a map can hold (weapons, ammo,
/// health, armour, powerups; keys excluded). Their spots are where the
/// mapper put items, so players can reach them.
pub const PICKUP_EDNUMS: [i32; 30] = [
    2001, 82, 2002, 2003, 2004, 2005, 2006, // weapons
    2007, 2048, 2008, 2049, 2010, 2046, 2047, 17, 8, // ammo, backpack
    2011, 2012, 2014, 2013, 83, // health
    2015, 2018, 2019, // armour
    2022, 2023, 2024, 2025, 2026, 2045, // powerups
];

impl Item {
    /// Ammo dropped next to a gun (and dropped instead of a gun that is too
    /// close to another one or over its cap).
    pub fn ammo(self) -> Option<Item> {
        match self {
            Item::Shotgun => Some(Item::Shell),
            Item::Chaingun => Some(Item::Clip),
            Item::RocketLauncher => Some(Item::RocketAmmo),
            _ => None,
        }
    }

    pub fn is_gun(self) -> bool {
        matches!(
            self,
            Item::Shotgun | Item::Chaingun | Item::RocketLauncher | Item::Chainsaw
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_time_limits_keep_the_normal_zone() {
        assert_eq!(zone_schedule_secs(), 450);
        assert_eq!(zone_pace_milli(0), 1000, "no limit");
        assert_eq!(zone_pace_milli(20 * 60 * TICRATE), 1000);
        assert_eq!(zone_pace_milli(10 * 60 * TICRATE), 1000);
    }

    #[test]
    fn short_time_limits_close_the_zone_before_time_runs_out() {
        for secs in [30, 60, 90, 180, 300, 420] {
            let left = secs * TICRATE;
            let pace = zone_pace_milli(left);
            assert!(pace > 1000, "{secs}s speeds the zone up");
            let zone_real = zone_schedule_secs() * TICRATE * 1000 / pace;
            assert!(
                zone_real <= left - 5 * TICRATE,
                "{secs}s: zone closes after {} s",
                zone_real / TICRATE
            );
            // Not absurdly early either: the zone uses most of the match.
            assert!(
                zone_real * 10 >= left * 6,
                "{secs}s: zone {}",
                zone_real / TICRATE
            );
        }
    }

    #[test]
    fn loot_scales_with_players() {
        let guns = |p| gun_counts(p).iter().map(|&(_, n)| n).sum::<usize>();
        assert_eq!(guns(2), 1 + 1 + 2 + 1);
        assert_eq!(guns(8), 2 + 4 + 8 + 1);
        let total = |p| guns(p) * 2 + supply_counts(p).iter().map(|&(_, n)| n).sum::<usize>();
        assert!(total(8) <= MAX_LOOT, "{}", total(8));
    }

    #[test]
    fn guns_come_with_their_ammo() {
        assert!(Item::Shotgun.ammo() == Some(Item::Shell));
        assert!(Item::RocketLauncher.ammo() == Some(Item::RocketAmmo));
        assert!(Item::Chainsaw.ammo().is_none() && Item::Chainsaw.is_gun());
        assert_eq!(ITEM_CLASSES[Item::Berserk as usize], "Berserk");
        assert_eq!(ITEM_CLASSES[Item::Medikit as usize], "Medikit");
    }

    #[test]
    fn zone_damage_follows_schedule() {
        assert_eq!(zone_damage_for_phase(1), 2);
        assert_eq!(zone_damage_for_phase(6), 25);
        assert_eq!(zone_damage_for_phase(0), 0);
    }
}
