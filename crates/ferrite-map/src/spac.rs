//! Linedef activation masks (`SPAC_*`, `docs/09-specials.md` §2.1).

/// Player / anything walkover.
pub const CROSS: u32 = 1 << 0;
/// Player use (space).
pub const USE: u32 = 1 << 1;
/// Monster walkover.
pub const MCROSS: u32 = 1 << 2;
/// Projectile impact.
pub const IMPACT: u32 = 1 << 3;
/// Player push.
pub const PUSH: u32 = 1 << 4;
/// Projectile cross.
pub const PCROSS: u32 = 1 << 5;
/// Use-through (passthrough).
pub const USETHROUGH: u32 = 1 << 6;
/// Anything can cross (Boom monsters-on-walk).
pub const ANYCROSS: u32 = 1 << 7;
/// Monster use.
pub const MUSE: u32 = 1 << 8;
/// Monster push.
pub const MPUSH: u32 = 1 << 9;
/// Player use from back side.
pub const USEBACK: u32 = 1 << 10;
/// Destructible line took damage (`damagespecial`).
pub const DAMAGE: u32 = 1 << 11;
/// Destructible line health reached 0 (`deathspecial`).
pub const DEATH: u32 = 1 << 12;

/// Hexen `ML_SPAC_*` packed in linedef flags bits 10–12.
const ML_SPAC_SHIFT: i16 = 10;
const ML_SPAC_MASK: i16 = 0x1c00;

/// Decode Hexen-format SPAC nibble from linedef flags.
#[must_use]
pub fn from_hexen_flags(flags: i16) -> u32 {
    match (flags & ML_SPAC_MASK) >> ML_SPAC_SHIFT {
        0 => CROSS,
        1 => USE,
        2 => MCROSS,
        3 => IMPACT,
        4 => PUSH,
        5 => PCROSS,
        6 => USETHROUGH,
        7 => IMPACT | PCROSS,
        _ => CROSS,
    }
}

/// Boom generalized trigger type (low 3 bits of linedef special) → SPAC mask.
///
/// Push triggers map to [`USE`] (GZDoom `P_TranslateLineDef`).
#[must_use]
pub fn from_boom_trigger(bits: u32, monsters: bool) -> u32 {
    let act = match bits & 7 {
        0 | 1 => CROSS,  // WalkOnce / WalkMany
        2 | 3 => USE,    // SwitchOnce / SwitchMany
        4 | 5 => IMPACT, // GunOnce / GunMany
        6 | 7 => USE,    // PushOnce / PushMany → Use
        _ => 0,
    };
    if monsters && act == CROSS {
        ANYCROSS
    } else {
        act
    }
}

/// Whether `mask` allows a player walkover activation.
#[must_use]
pub fn allows_cross(mask: u32) -> bool {
    mask == 0 || (mask & (CROSS | ANYCROSS | MCROSS | PCROSS)) != 0
}

/// Whether `mask` allows a player use activation.
#[must_use]
pub fn allows_use(mask: u32) -> bool {
    mask == 0 || (mask & (USE | USETHROUGH | PUSH | MUSE | MPUSH)) != 0
}

/// Whether `mask` allows impact activation.
#[must_use]
pub fn allows_impact(mask: u32) -> bool {
    mask == 0 || (mask & IMPACT) != 0
}

/// Whether `mask` allows push activation.
#[must_use]
pub fn allows_push(mask: u32) -> bool {
    mask == 0 || (mask & (PUSH | MPUSH | USE)) != 0
}

/// Whether `mask` allows destructible-damage activation.
#[must_use]
pub fn allows_damage(mask: u32) -> bool {
    (mask & DAMAGE) != 0
}

/// Whether `mask` allows destructible-death activation.
#[must_use]
pub fn allows_death(mask: u32) -> bool {
    (mask & DEATH) != 0
}
