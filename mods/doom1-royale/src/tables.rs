//! Loot tables, timings, and weighted picks.
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
/// Drop selection on the modal map (`docs/28` §5).
pub const DROP_SELECT_SECS: i32 = 15;
/// Max distance from the map pick to the resolved spawn spot.
pub const DROP_RESOLVE_RADIUS: f64 = 512.0;
/// Min distance between two players' resolved picks.
pub const DROP_PICK_SEP: f64 = 256.0;
/// Deploy / landing window.
pub const DEPLOY_SECS: i32 = 3;
/// Legacy delay before shells; resolution uses victory + standings MODALs (BR7).
pub const END_SECS: i32 = 7;
/// Loot never drops this close to where a player was placed.
pub const LOOT_CLEARANCE: f64 = 96.0;
/// Two guns never drop closer than this; the later one becomes its ammo.
pub const GUN_SPACING: f64 = 448.0;
/// Rocket launchers on the map (centre forced drop counts toward this).
pub fn rocket_cap(players: usize) -> usize {
    players.div_ceil(2)
}
/// Chainguns from weighted loot (shotguns-per-player are forced separately).
pub fn chaingun_cap(players: usize) -> usize {
    players.saturating_sub(1).max(2)
}
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

/// Every item the mod drops (all present in shareware Doom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Item {
    Shotgun,
    Chaingun,
    RocketLauncher,
    Chainsaw,
    Shell,
    ShellBox,
    Clip,
    ClipBox,
    RocketAmmo,
    Stimpack,
    Medikit,
    GreenArmor,
    ArmorBonus,
    HealthBonus,
    Berserk,
}

pub const ITEM_CLASSES: [&str; 15] = [
    "Shotgun",
    "Chaingun",
    "RocketLauncher",
    "Chainsaw",
    "Shell",
    "ShellBox",
    "Clip",
    "ClipBox",
    "RocketAmmo",
    "Stimpack",
    "Medikit",
    "GreenArmor",
    "ArmorBonus",
    "HealthBonus",
    "Berserk",
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

/// Loot tables by distance to the zone centre: (item, weight).
pub static INNER: [(Item, i32); 9] = [
    (Item::RocketLauncher, 2),
    (Item::Chaingun, 5),
    (Item::Shotgun, 4),
    (Item::ShellBox, 5),
    (Item::ClipBox, 4),
    (Item::RocketAmmo, 4),
    (Item::Medikit, 6),
    (Item::GreenArmor, 5),
    (Item::Berserk, 2),
];
pub static MIDDLE: [(Item, i32); 10] = [
    (Item::Shotgun, 8),
    (Item::Chaingun, 5),
    (Item::Shell, 8),
    (Item::Clip, 7),
    (Item::ClipBox, 4),
    (Item::RocketAmmo, 2),
    (Item::Stimpack, 7),
    (Item::Medikit, 4),
    (Item::ArmorBonus, 5),
    (Item::GreenArmor, 2),
];
pub static OUTER: [(Item, i32); 7] = [
    (Item::Shotgun, 7),
    (Item::Chainsaw, 3),
    (Item::Shell, 10),
    (Item::Clip, 10),
    (Item::Stimpack, 9),
    (Item::ArmorBonus, 7),
    (Item::HealthBonus, 7),
];

/// Weighted pick from `table` with `roll` in `0..sum(weights)`.
pub fn pick(table: &[(Item, i32)], mut roll: i32) -> Item {
    for &(item, w) in table {
        if roll < w {
            return item;
        }
        roll -= w;
    }
    table[0].0
}

pub fn total(table: &[(Item, i32)]) -> i32 {
    table.iter().map(|&(_, w)| w).sum()
}

/// Loot table for the spot ranked `rank` of `n` by distance to the centre.
pub fn tier(rank: usize, n: usize) -> &'static [(Item, i32)] {
    if rank * 4 < n {
        &INNER
    } else if rank * 3 < n * 2 {
        &MIDDLE
    } else {
        &OUTER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_guns_live_in_the_inner_ring() {
        let has = |t: &[(Item, i32)], i: Item| t.iter().any(|&(x, _)| x == i);
        assert!(core::ptr::eq(tier(0, 100), INNER.as_slice()));
        assert!(core::ptr::eq(tier(50, 100), MIDDLE.as_slice()));
        assert!(core::ptr::eq(tier(99, 100), OUTER.as_slice()));
        assert!(has(&INNER, Item::RocketLauncher));
        assert!(!has(&MIDDLE, Item::RocketLauncher) && !has(&OUTER, Item::RocketLauncher));
        assert!(!has(&OUTER, Item::Chaingun));
        for t in [&INNER[..], &MIDDLE[..], &OUTER[..]] {
            let sum = total(t);
            assert_eq!(pick(t, 0), t[0].0);
            assert_eq!(pick(t, sum - 1), t[t.len() - 1].0);
        }
    }

    #[test]
    fn guns_come_with_their_ammo() {
        assert!(Item::Shotgun.ammo() == Some(Item::Shell));
        assert!(Item::RocketLauncher.ammo() == Some(Item::RocketAmmo));
        assert!(Item::Chainsaw.ammo().is_none() && Item::Chainsaw.is_gun());
        assert_eq!(ITEM_CLASSES[Item::Berserk as usize], "Berserk");
        assert_eq!(ITEM_CLASSES[Item::HealthBonus as usize], "HealthBonus");
    }

    #[test]
    fn zone_damage_follows_schedule() {
        assert_eq!(zone_damage_for_phase(1), 2);
        assert_eq!(zone_damage_for_phase(6), 25);
        assert_eq!(zone_damage_for_phase(0), 0);
    }
}
