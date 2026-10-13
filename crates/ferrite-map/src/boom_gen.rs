//! Boom generalized linedef bitfield → Hexen `Generic_*` specials.
//!
//! Ranges and selectors from GZDoom `wadsrc/static/xlat/base.txt`.

use crate::xlat::TranslatedSpecial;

/// Hexen-style Generic_* numbers (GZDoom `actionspecials.h`).
const GENERIC_FLOOR: i16 = 200;
const GENERIC_CEILING: i16 = 201;
const GENERIC_DOOR: i16 = 202;
const GENERIC_LIFT: i16 = 203;
const GENERIC_STAIRS: i16 = 204;
const GENERIC_CRUSHER: i16 = 205;

const D_SLOW: i32 = 16;
const D_NORMAL: i32 = 32;
const D_FAST: i32 = 64;
const D_TURBO: i32 = 128;
const F_SLOW: i32 = 8;
const F_NORMAL: i32 = 16;
const F_FAST: i32 = 32;
const F_TURBO: i32 = 64;
const P_SLOW: i32 = 8;
const P_NORMAL: i32 = 16;
const P_FAST: i32 = 32;
const P_TURBO: i32 = 64;
const C_SLOW: i32 = 8;
const C_NORMAL: i32 = 16;
const C_FAST: i32 = 32;
const C_TURBO: i32 = 64;
const ST_SLOW: i32 = 2;
const ST_NORMAL: i32 = 4;
const ST_FAST: i32 = 16;
const ST_TURBO: i32 = 32;

/// ZDoom `ML_REPEAT_SPECIAL` (repeatable activation).
const ML_REPEAT_SPECIAL: i16 = 0x0200;
/// ZDoom `ML_MONSTERSCANACTIVATE`.
const ML_MONSTERS: i16 = 0x2000;

/// Translate a Boom generalized linedef type, or `None` if out of range.
#[must_use]
pub fn translate_generalized(special: u16, tag: i32) -> Option<TranslatedSpecial> {
    let s = u32::from(special);
    let (mut t, monst_mask, monst_sel) = if (0x2f80..=0x2fff).contains(&s) {
        (gen_crusher(s, tag), 0x0020, 0x0020)
    } else if (0x3000..=0x33ff).contains(&s) {
        (gen_stairs(s, tag), 0x0020, 0x0020)
    } else if (0x3400..=0x37ff).contains(&s) {
        (gen_lift(s, tag), 0x0020, 0x0020)
    } else if (0x3800..=0x3bff).contains(&s) {
        (gen_locked_door(s, tag), 0, 0)
    } else if (0x3c00..=0x3fff).contains(&s) {
        (gen_door(s, tag), 0x0080, 0x0080)
    } else if (0x4000..=0x5fff).contains(&s) {
        (gen_ceiling(s, tag), 0x0c20, 0x0020)
    } else if (0x6000..=0x7fff).contains(&s) {
        (gen_floor(s, tag), 0x0c20, 0x0020)
    } else {
        return None;
    };
    t.flags = boom_trigger_flags(s, monst_mask, monst_sel);
    let monsters = (t.flags & ML_MONSTERS) != 0;
    t.activation = crate::spac::from_boom_trigger(s, monsters);
    Some(t)
}

/// Decode Boom trigger bits (low 3) + optional MONST bit into ZDoom line flags.
fn boom_trigger_flags(bits: u32, monst_mask: u32, monst_sel: u32) -> i16 {
    let mut f = 0i16;
    // WalkMany / SwitchMany / GunMany / PushMany → repeatable.
    match bits & 7 {
        1 | 3 | 5 | 7 => f |= ML_REPEAT_SPECIAL,
        _ => {}
    }
    if monst_mask != 0 && (bits & monst_mask) == monst_sel {
        f |= ML_MONSTERS;
    }
    f
}

fn pick(bits: u32, mask: u32, table: &[(u32, i32)]) -> i32 {
    let v = bits & mask;
    for &(sel, val) in table {
        if sel == v {
            return val;
        }
    }
    table.first().map(|t| t.1).unwrap_or(0)
}

fn gen_crusher(bits: u32, tag: i32) -> TranslatedSpecial {
    let speed = pick(
        bits,
        0x0018,
        &[
            (0x0000, C_SLOW),
            (0x0008, C_NORMAL),
            (0x0010, C_FAST),
            (0x0018, C_TURBO),
        ],
    );
    let silent = i32::from((bits & 0x0040) != 0);
    TranslatedSpecial {
        special: GENERIC_CRUSHER,
        args: [tag, speed, speed, silent, 10],
        flags: 0, // caller merges
        activation: 0,
    }
}

fn gen_stairs(bits: u32, tag: i32) -> TranslatedSpecial {
    let speed = pick(
        bits,
        0x0018,
        &[
            (0x0000, ST_SLOW),
            (0x0008, ST_NORMAL),
            (0x0010, ST_FAST),
            (0x0018, ST_TURBO),
        ],
    );
    let step = pick(
        bits,
        0x00c0,
        &[(0x0000, 4), (0x0040, 8), (0x0080, 16), (0x00c0, 24)],
    );
    let dir = pick(
        bits,
        0x0300,
        &[(0x0000, 0), (0x0100, 1), (0x0200, 2), (0x0300, 3)],
    );
    TranslatedSpecial {
        special: GENERIC_STAIRS,
        args: [tag, speed, step, dir, 0],
        flags: 0,
        activation: 0,
    }
}

fn gen_lift(bits: u32, tag: i32) -> TranslatedSpecial {
    let speed = pick(
        bits,
        0x0018,
        &[
            (0x0000, P_SLOW * 2),
            (0x0008, P_NORMAL * 2),
            (0x0010, P_FAST * 2),
            (0x0018, P_TURBO * 2),
        ],
    );
    let delay = pick(
        bits,
        0x00c0,
        &[(0x0000, 8), (0x0040, 24), (0x0080, 40), (0x00c0, 80)],
    );
    let target = pick(
        bits,
        0x0300,
        &[(0x0000, 1), (0x0100, 2), (0x0200, 3), (0x0300, 4)],
    );
    TranslatedSpecial {
        special: GENERIC_LIFT,
        args: [tag, speed, delay, target, 0],
        flags: 0,
        activation: 0,
    }
}

fn gen_locked_door(bits: u32, tag: i32) -> TranslatedSpecial {
    let speed = pick(
        bits,
        0x0018,
        &[
            (0x0000, D_SLOW),
            (0x0008, D_NORMAL),
            (0x0010, D_FAST),
            (0x0018, D_TURBO),
        ],
    );
    let kind = i32::from((bits & 0x0020) != 0);
    let delay = if (bits & 0x0020) != 0 { 34 } else { 0 };
    let mut lock = pick(
        bits,
        0x01c0,
        &[
            (0x0000, 100), // AnyKey
            (0x0040, 1),   // RCard
            (0x0080, 2),   // BCard
            (0x00c0, 3),   // YCard
            (0x0100, 4),   // RSkull
            (0x0140, 5),   // BSkull
            (0x0180, 6),   // YSkull
            (0x01c0, 101), // AllKeys
        ],
    );
    if (bits & 0x0200) != 0 {
        lock |= 128; // CardIsSkull
    }
    TranslatedSpecial {
        special: GENERIC_DOOR,
        args: [tag, speed, kind, delay, lock],
        flags: 0,
        activation: 0,
    }
}

fn gen_door(bits: u32, tag: i32) -> TranslatedSpecial {
    let speed = pick(
        bits,
        0x0018,
        &[
            (0x0000, D_SLOW),
            (0x0008, D_NORMAL),
            (0x0010, D_FAST),
            (0x0018, D_TURBO),
        ],
    );
    let kind = pick(
        bits,
        0x0060,
        &[(0x0000, 64), (0x0020, 65), (0x0040, 66), (0x0060, 67)],
    );
    let delay = pick(
        bits,
        0x0300,
        &[(0x0000, 8), (0x0100, 34), (0x0200, 69), (0x0300, 240)],
    );
    TranslatedSpecial {
        special: GENERIC_DOOR,
        args: [tag, speed, kind, delay, 0],
        flags: 0,
        activation: 0,
    }
}

fn gen_ceiling(bits: u32, tag: i32) -> TranslatedSpecial {
    let speed = pick(
        bits,
        0x0018,
        &[
            (0x0000, F_SLOW),
            (0x0008, F_NORMAL),
            (0x0010, F_FAST),
            (0x0018, F_TURBO),
        ],
    );
    // Same 0x0380 field: targets 1–6, or absolute height 24/32 (target 0).
    let (height, target) = match bits & 0x0380 {
        0x0000 => (0, 1),
        0x0080 => (0, 2),
        0x0100 => (0, 3),
        0x0180 => (0, 4),
        0x0200 => (0, 5),
        0x0280 => (0, 6),
        0x0300 => (24, 0),
        0x0380 => (32, 0),
        _ => (0, 0),
    };
    let mut change = pick(
        bits,
        0x0c00,
        &[(0x0000, 0), (0x0400, 1), (0x0800, 2), (0x0c00, 3)],
    );
    change |= pick(
        bits,
        0x0060,
        &[(0x0000, 0), (0x0020, 4), (0x0040, 8), (0x0060, 12)],
    );
    if (bits & 0x1000) != 0 {
        change |= 16;
    }
    TranslatedSpecial {
        special: GENERIC_CEILING,
        args: [tag, speed, height, target, change],
        flags: 0,
        activation: 0,
    }
}

fn gen_floor(bits: u32, tag: i32) -> TranslatedSpecial {
    let speed = pick(
        bits,
        0x0018,
        &[
            (0x0000, F_SLOW),
            (0x0008, F_NORMAL),
            (0x0010, F_FAST),
            (0x0018, F_TURBO),
        ],
    );
    // GZDoom xlat: arg3 height only for 0x0300/0x0380; arg4 target for 0x0000–0x0280.
    let (height, target) = match bits & 0x0380 {
        0x0000 => (0, 1),
        0x0080 => (0, 2),
        0x0100 => (0, 3),
        0x0180 => (0, 4),
        0x0200 => (0, 5),
        0x0280 => (0, 6),
        0x0300 => (24, 0),
        0x0380 => (32, 0),
        _ => (0, 0),
    };
    let mut change = pick(
        bits,
        0x0c00,
        &[(0x0000, 0), (0x0400, 1), (0x0800, 2), (0x0c00, 3)],
    );
    change |= pick(
        bits,
        0x0060,
        &[(0x0000, 0), (0x0020, 4), (0x0040, 8), (0x0060, 12)],
    );
    if (bits & 0x1000) != 0 {
        change |= 16;
    }
    TranslatedSpecial {
        special: GENERIC_FLOOR,
        args: [tag, speed, height, target, change],
        flags: 0,
        activation: 0,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn slow_door_3c00() {
        // 0x3c00: slow, kind 64, delay 8
        let t = translate_generalized(0x3c00, 5).unwrap();
        assert_eq!(t.special, GENERIC_DOOR);
        assert_eq!(t.args[0], 5);
        assert_eq!(t.args[1], D_SLOW);
        assert_eq!(t.args[2], 64);
        assert_eq!(t.args[3], 8);
    }

    #[test]
    fn turbo_lift() {
        let t = translate_generalized(0x3400 | 0x0018 | 0x00c0 | 0x0300, 2).unwrap();
        assert_eq!(t.special, GENERIC_LIFT);
        assert_eq!(t.args[1], P_TURBO * 2);
        assert_eq!(t.args[2], 80);
        assert_eq!(t.args[3], 4);
    }

    #[test]
    fn floor_range() {
        let t = translate_generalized(0x6000, 1).unwrap();
        assert_eq!(t.special, GENERIC_FLOOR);
        assert_eq!(t.args[0], 1);
        assert_eq!(t.args[1], F_SLOW);
        assert_eq!(t.args[2], 0);
        assert_eq!(t.args[3], 1); // highest neighbor
    }

    #[test]
    fn floor_absolute_height_24_and_32() {
        // 0x0300 → height 24, target 0 (absolute).
        let t24 = translate_generalized(0x6000 | 0x0300, 3).unwrap();
        assert_eq!(t24.args[2], 24);
        assert_eq!(t24.args[3], 0);
        // 0x0380 → height 32, target 0.
        let t32 = translate_generalized(0x6000 | 0x0380, 3).unwrap();
        assert_eq!(t32.args[2], 32);
        assert_eq!(t32.args[3], 0);
    }

    #[test]
    fn walk_many_sets_repeat_flag() {
        let t = translate_generalized(0x6000 | 1, 1).unwrap(); // WalkMany
        assert_ne!(t.flags & ML_REPEAT_SPECIAL, 0);
        let once = translate_generalized(0x6000, 1).unwrap(); // WalkOnce
        assert_eq!(once.flags & ML_REPEAT_SPECIAL, 0);
    }

    #[test]
    fn boom_trigger_sets_spac_activation() {
        let walk = translate_generalized(0x6000, 1).unwrap();
        assert_eq!(walk.activation, crate::spac::CROSS);
        let sw = translate_generalized(0x6000 | 2, 1).unwrap();
        assert_eq!(sw.activation, crate::spac::USE);
        let gun = translate_generalized(0x6000 | 4, 1).unwrap();
        assert_eq!(gun.activation, crate::spac::IMPACT);
    }

    #[test]
    fn out_of_range() {
        assert!(translate_generalized(100, 0).is_none());
    }
}
