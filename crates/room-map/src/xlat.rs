//! Doom/Heretic/Strife linedef special translation (XLAT).
//!
//! Tables are behavioural facts from GZDoom `wadsrc/static/xlat/base.txt`
//! (Door_Raise = 12, speeds D_SLOW = 16, VDOORWAIT = 150, …). A full XLAT
//! language interpreter lands later; this covers Doom 0–141, Boom discrete
//! 142–259, and Boom generalized ranges via [`crate::boom_gen`].

use crate::raw::RawLinedef;
use crate::spac;

/// Speeds / waits from `xlat/defines.i`.
const D_SLOW: i32 = 16;
const D_FAST: i32 = 64;
const VDOORWAIT: i32 = 150;
const F_SLOW: i32 = 8;
const F_FAST: i32 = 32;
const C_SLOW: i32 = 8;
const C_NORMAL: i32 = 16;
const P_SLOW: i32 = 8;
const P_FAST: i32 = 32;
const P_TURBO: i32 = 64;
const PLATWAIT: i32 = 105;
const ST_SLOW: i32 = 2;
const ST_TURBO: i32 = 32;
const DORATE: i32 = 4;
const SCROLL_UNIT: i32 = 64;

/// Hexen-style special numbers (GZDoom `actionspecials.h`).
const DOOR_CLOSE: i16 = 10;
const DOOR_OPEN: i16 = 11;
const DOOR_RAISE: i16 = 12;
const DOOR_LOCKED_RAISE: i16 = 13;
const DOOR_CLOSE_WAIT_OPEN: i16 = 249;
const FLOOR_RAISE_TO_LOWEST_CEILING: i16 = 238;
const FLOOR_RAISE_TO_NEAREST: i16 = 25;
const FLOOR_LOWER_TO_HIGHEST: i16 = 22;
const FLOOR_LOWER_TO_LOWEST: i16 = 21;
const FLOOR_RAISE_BY_TEXTURE: i16 = 240;
const FLOOR_RAISE_BY_VALUE: i16 = 23;
const FLOOR_DONUT: i16 = 250;
const FLOOR_RAISE_AND_CRUSH_DOOM: i16 = 99;
const CEILING_CRUSH_AND_RAISE_DIST: i16 = 42;
const CEILING_LOWER_TO_FLOOR: i16 = 254;
const CEILING_LOWER_AND_CRUSH: i16 = 43;
const CEILING_CRUSH_STOP: i16 = 44;
const STAIRS_BUILD_UP_DOOM: i16 = 217;
const PLAT_DOWN_WAIT_UP_STAY_LIP: i16 = 62;
const PLAT_UP_BY_VALUE_STAY_TX: i16 = 230;
const PLAT_RAISE_AND_STAY_TX0: i16 = 228;
const PLAT_PERPETUAL_RAISE: i16 = 60;
const PLAT_STOP: i16 = 61;
const EXIT_NORMAL: i16 = 243;
const EXIT_SECRET: i16 = 244;
const TELEPORT: i16 = 70;
const LIGHT_CHANGE_TO_VALUE: i16 = 112;
const LIGHT_MIN_NEIGHBOR: i16 = 233;
const LIGHT_MAX_NEIGHBOR: i16 = 234;
const LIGHT_STROBE_DOOM: i16 = 232;
const SCROLL_TEXTURE_LEFT: i16 = 100;
const SCROLL_TEXTURE_RIGHT: i16 = 101;
const GENERIC_FLOOR: i16 = 200;
const GENERIC_CEILING: i16 = 201;
const FLOOR_RAISE_BY_VALUE_TIMES8: i16 = 35;
const STAIRS_BUILD_UP_DOOM_CRUSH: i16 = 217;
const CEILING_CRUSH_AND_RAISE_SILENT_DIST: i16 = 104;

/// Key bitmasks for locked doors (`CardIsSkull = 128`).
const B_CARD: i32 = 2 | 128;
const Y_CARD: i32 = 3 | 128;
const R_CARD: i32 = 1 | 128;

/// XLAT activation bits (`base.txt` `WALK|USE|SHOOT|REP|MONST|MONWALK`).
const A_WALK: u8 = 1;
const A_USE: u8 = 2;
const A_SHOOT: u8 = 4;
const A_REP: u8 = 8;
const A_MONST: u8 = 16;
const A_MONWALK: u8 = 32;

/// ZDoom `ML_REPEAT_SPECIAL`.
const ML_REPEAT_SPECIAL: i16 = 0x0200;
/// ZDoom `ML_MONSTERSCANACTIVATE`.
const ML_MONSTERS: i16 = 0x2000;

/// Per Doom linedef type 0..=141 activation flags from GZDoom `xlat/base.txt`.
const DOOM_XLAT_ACT: [u8; 142] = [
    0x00, 0x1a, 0x01, 0x01, 0x11, 0x01, 0x01, 0x02, // 0–7
    0x01, 0x02, 0x11, 0x02, 0x01, 0x01, 0x02, 0x02, // 8–15
    0x01, 0x01, 0x02, 0x01, 0x02, 0x02, 0x01, 0x02, // 16–23
    0x04, 0x01, 0x0a, 0x0a, 0x0a, 0x02, 0x01, 0x02, // 24–31
    0x12, 0x12, 0x12, 0x01, 0x01, 0x01, 0x01, 0x11, // 32–39
    0x01, 0x02, 0x0a, 0x0a, 0x01, 0x0a, 0x1c, 0x04, // 40–47
    0x00, 0x02, 0x02, 0x02, 0x01, 0x01, 0x01, 0x02, // 48–55
    0x01, 0x01, 0x01, 0x01, 0x0a, 0x0a, 0x0a, 0x0a, // 56–63
    0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x02, // 64–71
    0x09, 0x09, 0x09, 0x09, 0x09, 0x09, 0x0a, 0x09, // 72–79
    0x09, 0x09, 0x09, 0x09, 0x09, 0x00, 0x09, 0x09, // 80–87
    0x19, 0x09, 0x09, 0x09, 0x09, 0x09, 0x09, 0x09, // 88–95
    0x09, 0x19, 0x09, 0x0a, 0x01, 0x02, 0x02, 0x02, // 96–103
    0x01, 0x09, 0x09, 0x09, 0x01, 0x01, 0x01, 0x02, // 104–111
    0x02, 0x02, 0x0a, 0x0a, 0x0a, 0x0a, 0x02, 0x01, // 112–119
    0x09, 0x01, 0x02, 0x0a, 0x01, 0x20, 0x28, 0x02, // 120–127
    0x09, 0x09, 0x01, 0x02, 0x0a, 0x02, 0x0a, 0x02, // 128–135
    0x0a, 0x02, 0x0a, 0x0a, 0x02, 0x01, // 136–141
];

/// Merge Doom geometry flags with XLAT `USE|WALK|…|REP` bits → ZDoom flags + SPAC.
fn apply_doom_xlat_act(line_flags: i16, act: u8) -> (i16, u32) {
    let mut flags = line_flags;
    if (act & A_REP) != 0 {
        flags |= ML_REPEAT_SPECIAL;
    }
    if (act & A_MONST) != 0 {
        flags |= ML_MONSTERS;
    }
    let mut activation = 0u32;
    if (act & A_MONWALK) != 0 {
        activation |= spac::MCROSS;
    } else if (act & A_WALK) != 0 {
        activation |= spac::CROSS;
        if (act & A_MONST) != 0 {
            activation |= spac::ANYCROSS;
        }
    }
    if (act & A_USE) != 0 {
        activation |= spac::USE;
    }
    if (act & A_SHOOT) != 0 {
        activation |= spac::IMPACT;
    }
    (flags, activation)
}

/// Translate a Doom-format linedef into Hexen-style special + args.
///
/// Hexen-format linedefs (`line.hexen`) pass through unchanged.
#[must_use]
pub fn translate_doom_linedef(line: &RawLinedef) -> TranslatedSpecial {
    if line.hexen {
        return TranslatedSpecial {
            special: line.special,
            args: line.args,
            flags: line.flags,
            activation: line.activation,
        };
    }
    let tag = i32::from(line.tag);
    let raw = u16::try_from(line.special).unwrap_or(0);
    if let Some(mut t) = crate::boom_gen::translate_generalized(raw, tag) {
        // Keep Doom ML_* geometry flags; OR Boom trigger/MONST bits from gen.
        t.flags |= line.flags;
        return t;
    }
    if let Some(mut t) = crate::boom_discrete::translate_boom_discrete(raw, tag) {
        t.flags = line.flags;
        return t;
    }
    let (special, args) = match raw {
        0 => (0, [0; 5]),
        // DR door (manual, monsters, repeatable)
        1 => (DOOR_RAISE, [0, D_SLOW, VDOORWAIT, tag, 0]),
        2 => (DOOR_OPEN, [tag, D_SLOW, 0, 0, 0]),
        3 => (DOOR_CLOSE, [tag, D_SLOW, 0, 0, 0]),
        4 => (DOOR_RAISE, [tag, D_SLOW, VDOORWAIT, 0, 0]),
        5 => (FLOOR_RAISE_TO_LOWEST_CEILING, [tag, F_SLOW, 0, 0, 0]),
        6 => (CEILING_CRUSH_AND_RAISE_DIST, [tag, 8, C_NORMAL, 10, 0]),
        7 => (STAIRS_BUILD_UP_DOOM, [tag, ST_SLOW, 8, 0, 0]),
        8 => (STAIRS_BUILD_UP_DOOM, [tag, ST_SLOW, 8, 0, 0]),
        9 => (FLOOR_DONUT, [tag, DORATE, DORATE, 0, 0]),
        10 => (PLAT_DOWN_WAIT_UP_STAY_LIP, [tag, P_FAST, PLATWAIT, 0, 0]),
        11 => (EXIT_NORMAL, [0, 0, 0, 0, 0]),
        12 => (LIGHT_MAX_NEIGHBOR, [tag, 0, 0, 0, 0]),
        13 => (LIGHT_CHANGE_TO_VALUE, [tag, 255, 0, 0, 0]),
        14 => (PLAT_UP_BY_VALUE_STAY_TX, [tag, P_SLOW / 2, 4, 0, 0]),
        15 => (PLAT_UP_BY_VALUE_STAY_TX, [tag, P_SLOW / 2, 3, 0, 0]),
        16 => (DOOR_CLOSE_WAIT_OPEN, [tag, D_SLOW, 240, 0, 0]),
        17 => (LIGHT_STROBE_DOOM, [tag, 5, 35, 0, 0]),
        18 => (FLOOR_RAISE_TO_NEAREST, [tag, F_SLOW, 0, 0, 0]),
        19 => (FLOOR_LOWER_TO_HIGHEST, [tag, F_SLOW, 128, 0, 0]),
        20 => (PLAT_RAISE_AND_STAY_TX0, [tag, P_SLOW / 2, 0, 0, 0]),
        21 => (PLAT_DOWN_WAIT_UP_STAY_LIP, [tag, P_FAST, PLATWAIT, 0, 0]),
        22 => (PLAT_RAISE_AND_STAY_TX0, [tag, P_SLOW / 2, 0, 0, 0]),
        23 => (FLOOR_LOWER_TO_LOWEST, [tag, F_SLOW, 0, 0, 0]),
        24 => (FLOOR_RAISE_TO_LOWEST_CEILING, [tag, F_SLOW, 0, 0, 0]),
        25 => (CEILING_CRUSH_AND_RAISE_DIST, [tag, 8, C_SLOW, 10, 0]),
        26 => (DOOR_LOCKED_RAISE, [0, D_SLOW, VDOORWAIT, B_CARD, tag]),
        27 => (DOOR_LOCKED_RAISE, [0, D_SLOW, VDOORWAIT, Y_CARD, tag]),
        28 => (DOOR_LOCKED_RAISE, [0, D_SLOW, VDOORWAIT, R_CARD, tag]),
        29 => (DOOR_RAISE, [tag, D_SLOW, VDOORWAIT, 0, 0]),
        30 => (FLOOR_RAISE_BY_TEXTURE, [tag, F_SLOW, 0, 0, 0]),
        31 => (DOOR_OPEN, [0, D_SLOW, tag, 0, 0]),
        32 => (DOOR_LOCKED_RAISE, [0, D_SLOW, 0, B_CARD, tag]),
        33 => (DOOR_LOCKED_RAISE, [0, D_SLOW, 0, R_CARD, tag]),
        34 => (DOOR_LOCKED_RAISE, [0, D_SLOW, 0, Y_CARD, tag]),
        35 => (LIGHT_CHANGE_TO_VALUE, [tag, 35, 0, 0, 0]),
        36 => (FLOOR_LOWER_TO_HIGHEST, [tag, F_FAST, 136, 0, 0]),
        // 37 Floor_LowerToLowestTxTy → Generic_Floor to lowest + change tex/special.
        37 => (GENERIC_FLOOR, [tag, F_SLOW, 0, 2, 3]),
        38 => (FLOOR_LOWER_TO_LOWEST, [tag, F_SLOW, 0, 0, 0]),
        39 => (TELEPORT, [0, tag, 0, 0, 0]),
        40 => (GENERIC_CEILING, [tag, C_SLOW, 0, 1, 8]),
        41 => (CEILING_LOWER_TO_FLOOR, [tag, C_SLOW, 0, 0, 0]),
        42 => (DOOR_CLOSE, [tag, D_SLOW, 0, 0, 0]),
        43 => (CEILING_LOWER_TO_FLOOR, [tag, C_SLOW, 0, 0, 0]),
        44 => (CEILING_LOWER_AND_CRUSH, [tag, C_SLOW, 0, 2, 0]),
        45 => (FLOOR_LOWER_TO_HIGHEST, [tag, F_SLOW, 128, 0, 0]),
        46 => (DOOR_OPEN, [tag, D_SLOW, 0, 0, 0]),
        47 => (PLAT_RAISE_AND_STAY_TX0, [tag, P_SLOW / 2, 0, 0, 0]),
        48 => (SCROLL_TEXTURE_LEFT, [SCROLL_UNIT, 0, 0, 0, 0]),
        49 => (CEILING_CRUSH_AND_RAISE_DIST, [tag, 8, C_SLOW, 10, 0]),
        50 => (DOOR_CLOSE, [tag, D_SLOW, 0, 0, 0]),
        51 => (EXIT_SECRET, [0, 0, 0, 0, 0]),
        52 => (EXIT_NORMAL, [0, 0, 0, 0, 0]),
        53 => (PLAT_PERPETUAL_RAISE, [tag, P_SLOW, PLATWAIT, 0, 0]),
        54 => (PLAT_STOP, [tag, 0, 0, 0, 0]),
        55 => (FLOOR_RAISE_AND_CRUSH_DOOM, [tag, F_SLOW, 10, 2, 0]),
        56 => (FLOOR_RAISE_AND_CRUSH_DOOM, [tag, F_SLOW, 10, 2, 0]),
        57 => (CEILING_CRUSH_STOP, [tag, 0, 0, 0, 0]),
        58 => (FLOOR_RAISE_BY_VALUE, [tag, F_SLOW, 24, 0, 0]),
        59 => (FLOOR_RAISE_BY_VALUE, [tag, F_SLOW, 24, 0, 0]),
        60 => (FLOOR_LOWER_TO_LOWEST, [tag, F_SLOW, 0, 0, 0]),
        61 => (DOOR_OPEN, [tag, D_SLOW, 0, 0, 0]),
        62 => (PLAT_DOWN_WAIT_UP_STAY_LIP, [tag, P_FAST, PLATWAIT, 0, 0]),
        63 => (DOOR_RAISE, [tag, D_SLOW, VDOORWAIT, 0, 0]),
        64 => (FLOOR_RAISE_TO_LOWEST_CEILING, [tag, F_SLOW, 0, 0, 0]),
        65 => (FLOOR_RAISE_AND_CRUSH_DOOM, [tag, F_SLOW, 10, 2, 0]),
        66 => (PLAT_UP_BY_VALUE_STAY_TX, [tag, P_SLOW / 2, 3, 0, 0]),
        67 => (PLAT_UP_BY_VALUE_STAY_TX, [tag, P_SLOW / 2, 4, 0, 0]),
        68 => (PLAT_RAISE_AND_STAY_TX0, [tag, P_SLOW / 2, 0, 0, 0]),
        69 => (FLOOR_RAISE_TO_NEAREST, [tag, F_SLOW, 0, 0, 0]),
        70 => (FLOOR_LOWER_TO_HIGHEST, [tag, F_FAST, 136, 0, 0]),
        71 => (FLOOR_LOWER_TO_HIGHEST, [tag, F_FAST, 136, 0, 0]),
        72 => (CEILING_LOWER_AND_CRUSH, [tag, C_SLOW, 0, 2, 0]),
        73 => (CEILING_CRUSH_AND_RAISE_DIST, [tag, 8, C_SLOW, 10, 0]),
        74 => (CEILING_CRUSH_STOP, [tag, 0, 0, 0, 0]),
        75 => (DOOR_CLOSE, [tag, D_SLOW, 0, 0, 0]),
        76 => (DOOR_CLOSE_WAIT_OPEN, [tag, D_SLOW, 240, 0, 0]),
        77 => (CEILING_CRUSH_AND_RAISE_DIST, [tag, 8, C_NORMAL, 10, 0]),
        78 => (PLAT_RAISE_AND_STAY_TX0, [tag, P_SLOW / 2, 0, 0, 0]),
        79 => (LIGHT_CHANGE_TO_VALUE, [tag, 35, 0, 0, 0]),
        80 => (LIGHT_MAX_NEIGHBOR, [tag, 0, 0, 0, 0]),
        81 => (LIGHT_CHANGE_TO_VALUE, [tag, 255, 0, 0, 0]),
        82 => (FLOOR_LOWER_TO_LOWEST, [tag, F_SLOW, 0, 0, 0]),
        83 => (FLOOR_LOWER_TO_HIGHEST, [tag, F_SLOW, 128, 0, 0]),
        84 => (FLOOR_LOWER_TO_LOWEST, [tag, F_SLOW, 0, 0, 0]),
        85 => (SCROLL_TEXTURE_RIGHT, [SCROLL_UNIT, 0, 0, 0, 0]),
        86 => (DOOR_OPEN, [tag, D_SLOW, 0, 0, 0]),
        87 => (PLAT_PERPETUAL_RAISE, [tag, P_SLOW, PLATWAIT, 0, 0]),
        88 => (PLAT_DOWN_WAIT_UP_STAY_LIP, [tag, P_FAST, PLATWAIT, 0, 0]),
        89 => (PLAT_STOP, [tag, 0, 0, 0, 0]),
        90 => (DOOR_RAISE, [tag, D_SLOW, VDOORWAIT, 0, 0]),
        91 => (FLOOR_RAISE_TO_LOWEST_CEILING, [tag, F_SLOW, 0, 0, 0]),
        92 => (FLOOR_RAISE_BY_VALUE, [tag, F_SLOW, 24, 0, 0]),
        93 => (FLOOR_RAISE_BY_VALUE, [tag, F_SLOW, 24, 0, 0]),
        94 => (FLOOR_RAISE_AND_CRUSH_DOOM, [tag, F_SLOW, 10, 2, 0]),
        95 => (PLAT_RAISE_AND_STAY_TX0, [tag, P_SLOW / 2, 0, 0, 0]),
        96 => (FLOOR_RAISE_BY_TEXTURE, [tag, F_SLOW, 0, 0, 0]),
        97 => (TELEPORT, [0, tag, 0, 0, 0]),
        98 => (FLOOR_LOWER_TO_HIGHEST, [tag, F_FAST, 136, 0, 0]),
        99 => (DOOR_LOCKED_RAISE, [tag, D_FAST, 0, B_CARD, 0]),
        100 => (STAIRS_BUILD_UP_DOOM_CRUSH, [tag, ST_TURBO, 16, 0, 0]),
        101 => (FLOOR_RAISE_TO_LOWEST_CEILING, [tag, F_SLOW, 0, 0, 0]),
        102 => (FLOOR_LOWER_TO_HIGHEST, [tag, F_SLOW, 128, 0, 0]),
        103 => (DOOR_OPEN, [tag, D_SLOW, 0, 0, 0]),
        104 => (LIGHT_MIN_NEIGHBOR, [tag, 0, 0, 0, 0]),
        105 => (DOOR_RAISE, [tag, D_FAST, VDOORWAIT, 0, 0]),
        106 => (DOOR_OPEN, [tag, D_FAST, 0, 0, 0]),
        107 => (DOOR_CLOSE, [tag, D_FAST, 0, 0, 0]),
        108 => (DOOR_RAISE, [tag, D_FAST, VDOORWAIT, 0, 0]),
        109 => (DOOR_OPEN, [tag, D_FAST, 0, 0, 0]),
        110 => (DOOR_CLOSE, [tag, D_FAST, 0, 0, 0]),
        111 => (DOOR_RAISE, [tag, D_FAST, VDOORWAIT, 0, 0]),
        112 => (DOOR_OPEN, [tag, D_FAST, 0, 0, 0]),
        113 => (DOOR_CLOSE, [tag, D_FAST, 0, 0, 0]),
        114 => (DOOR_RAISE, [tag, D_FAST, VDOORWAIT, 0, 0]),
        115 => (DOOR_OPEN, [tag, D_FAST, 0, 0, 0]),
        116 => (DOOR_CLOSE, [tag, D_FAST, 0, 0, 0]),
        117 => (DOOR_RAISE, [0, D_FAST, VDOORWAIT, tag, 0]),
        118 => (DOOR_OPEN, [0, D_FAST, tag, 0, 0]),
        119 => (FLOOR_RAISE_TO_NEAREST, [tag, F_SLOW, 0, 0, 0]),
        120 => (PLAT_DOWN_WAIT_UP_STAY_LIP, [tag, P_TURBO, PLATWAIT, 0, 0]),
        121 => (PLAT_DOWN_WAIT_UP_STAY_LIP, [tag, P_TURBO, PLATWAIT, 0, 0]),
        122 => (PLAT_DOWN_WAIT_UP_STAY_LIP, [tag, P_TURBO, PLATWAIT, 0, 0]),
        123 => (PLAT_DOWN_WAIT_UP_STAY_LIP, [tag, P_TURBO, PLATWAIT, 0, 0]),
        124 => (EXIT_SECRET, [0, 0, 0, 0, 0]),
        125 => (TELEPORT, [0, tag, 0, 0, 0]),
        126 => (TELEPORT, [0, tag, 0, 0, 0]),
        127 => (STAIRS_BUILD_UP_DOOM_CRUSH, [tag, ST_TURBO, 16, 0, 0]),
        128 => (FLOOR_RAISE_TO_NEAREST, [tag, F_SLOW, 0, 0, 0]),
        129 => (FLOOR_RAISE_TO_NEAREST, [tag, F_FAST, 0, 0, 0]),
        130 => (FLOOR_RAISE_TO_NEAREST, [tag, F_FAST, 0, 0, 0]),
        131 => (FLOOR_RAISE_TO_NEAREST, [tag, F_FAST, 0, 0, 0]),
        132 => (FLOOR_RAISE_TO_NEAREST, [tag, F_FAST, 0, 0, 0]),
        133 => (DOOR_LOCKED_RAISE, [tag, D_FAST, 0, B_CARD, 0]),
        134 => (DOOR_LOCKED_RAISE, [tag, D_FAST, 0, R_CARD, 0]),
        135 => (DOOR_LOCKED_RAISE, [tag, D_FAST, 0, R_CARD, 0]),
        136 => (DOOR_LOCKED_RAISE, [tag, D_FAST, 0, Y_CARD, 0]),
        137 => (DOOR_LOCKED_RAISE, [tag, D_FAST, 0, Y_CARD, 0]),
        138 => (LIGHT_CHANGE_TO_VALUE, [tag, 255, 0, 0, 0]),
        139 => (LIGHT_CHANGE_TO_VALUE, [tag, 35, 0, 0, 0]),
        140 => (FLOOR_RAISE_BY_VALUE_TIMES8, [tag, F_SLOW, 64, 0, 0]),
        141 => (CEILING_CRUSH_AND_RAISE_SILENT_DIST, [tag, 8, C_SLOW, 10, 0]),
        // Remaining Boom generalized / Hexen+: identity with tag in arg0.
        other => (other as i16, [tag, 0, 0, 0, 0]),
    };
    let act = DOOM_XLAT_ACT.get(usize::from(raw)).copied().unwrap_or(0);
    let (flags, activation) = apply_doom_xlat_act(line.flags, act);
    TranslatedSpecial {
        special,
        args,
        flags,
        activation,
    }
}

/// Result of XLAT for one linedef.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranslatedSpecial {
    /// Hexen-style special number.
    pub special: i16,
    /// Five args.
    pub args: [i32; 5],
    /// Flags after flag remapping (identity for now).
    pub flags: i16,
    /// Activation mask (`spac::*`); 0 = unset.
    pub activation: u32,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::raw::RawLinedef;

    fn doom_line(special: i16, tag: i16) -> RawLinedef {
        RawLinedef {
            v1: 0,
            v2: 1,
            flags: 1,
            special,
            tag,
            side_front: 0,
            side_back: 0xFFFF,
            args: [0; 5],
            hexen: false,
            activation: 0,
            arg0_str: String::new(),
            midtex3d: false,
            midtex3d_impassible: false,
            check_switch_range: false,
            first_side_only: false,
            block_floaters: false,
            lock_number: 0,
            block_projectiles: false,
            block_sight: false,
            block_hitscan: false,
            extra_ids: Default::default(),
            alpha: 1.0,
            mid_additive: false,
            health: 0,
            health_group: 0,
            automap_style: 0,
            clip_midtex: false,
            wrap_midtex: false,
            ext: 0,
            user: Default::default(),
        }
    }

    #[test]
    fn door_raise_manual() {
        let t = translate_doom_linedef(&doom_line(1, 0));
        assert_eq!(t.special, DOOR_RAISE);
        assert_eq!(t.args[1], D_SLOW);
        assert_eq!(t.args[2], VDOORWAIT);
        // DR door: USE|MONST|REP — must stay usable after the first open.
        assert_ne!(t.flags & ML_REPEAT_SPECIAL, 0);
        assert_ne!(t.activation & spac::USE, 0);
    }

    #[test]
    fn doom_dr_locked_doors_are_repeatable() {
        for special in [26i16, 27, 28, 117] {
            let t = translate_doom_linedef(&doom_line(special, 0));
            assert_ne!(
                t.flags & ML_REPEAT_SPECIAL,
                0,
                "type {special} should be REP"
            );
        }
    }

    #[test]
    fn doom_d1_open_stay_is_one_shot() {
        let t = translate_doom_linedef(&doom_line(31, 0));
        assert_eq!(t.special, DOOR_OPEN);
        assert_eq!(t.flags & ML_REPEAT_SPECIAL, 0);
        assert_ne!(t.activation & spac::USE, 0);
    }

    #[test]
    fn exit_normal() {
        let t = translate_doom_linedef(&doom_line(11, 0));
        assert_eq!(t.special, EXIT_NORMAL);
    }

    #[test]
    fn teleport_keeps_tag() {
        let t = translate_doom_linedef(&doom_line(39, 7));
        assert_eq!(t.special, TELEPORT);
        assert_eq!(t.args[1], 7);
    }

    #[test]
    fn fast_door_and_turbo_plat() {
        let d = translate_doom_linedef(&doom_line(105, 3));
        assert_eq!(d.special, DOOR_RAISE);
        assert_eq!(d.args[1], D_FAST);
        let p = translate_doom_linedef(&doom_line(120, 9));
        assert_eq!(p.special, PLAT_DOWN_WAIT_UP_STAY_LIP);
        assert_eq!(p.args[1], P_TURBO);
        assert_eq!(p.args[0], 9);
    }

    #[test]
    fn hexen_passthrough() {
        let mut line = doom_line(12, 0);
        line.hexen = true;
        line.args = [1, 2, 3, 4, 5];
        let t = translate_doom_linedef(&line);
        assert_eq!(t.special, 12);
        assert_eq!(t.args, [1, 2, 3, 4, 5]);
    }
}
