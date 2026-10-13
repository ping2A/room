//! Boom discrete linedef specials 142–259 → Hexen-style specials.
//!
//! Behavioural table from GZDoom `xlat/base.txt`. Activation flags (WALK/USE/…)
//! are not encoded here yet — only special number + args.

use crate::xlat::TranslatedSpecial;

const F_SLOW: i32 = 8;
const C_SLOW: i32 = 8;
const C_NORMAL: i32 = 16;
const P_SLOW: i32 = 8;
const PLATWAIT: i32 = 105;
const D_SLOW: i32 = 16;
const ST_SLOW: i32 = 2;
const ST_TURBO: i32 = 32;
const DORATE: i32 = 4;
const ELEVATORSPEED: i32 = 32;

const FLOOR_RAISE_BY_VALUE: i16 = 23;
const FLOOR_LOWER_TO_LOWEST: i16 = 21;
const FLOOR_RAISE_BY_TEXTURE: i16 = 240;
const FLOOR_RAISE_BY_VALUE_TIMES8: i16 = 35;
const FLOOR_DONUT: i16 = 250;
const FLOOR_LOWER_TO_NEAREST: i16 = 242;
const CEILING_CRUSH_AND_RAISE_DIST: i16 = 42;
const CEILING_LOWER_AND_CRUSH: i16 = 44;
const CEILING_LOWER_TO_FLOOR: i16 = 254;
const CEILING_CRUSH_STOP: i16 = 46;
const CEILING_CRUSH_AND_RAISE_SILENT_DIST: i16 = 104;
const CEILING_LOWER_TO_LOWEST: i16 = 253;
const CEILING_LOWER_TO_HIGHEST_FLOOR: i16 = 192;
const FLOOR_AND_CEILING_LOWER_RAISE: i16 = 251;
const PLAT_UP_BY_VALUE_STAY_TX: i16 = 230;
const PLAT_PERPETUAL_RAISE: i16 = 60;
const PLAT_STOP: i16 = 61;
const PLAT_TOGGLE_CEILING: i16 = 231;
const DOOR_CLOSE_WAIT_OPEN: i16 = 249;
const EXIT_NORMAL: i16 = 243;
const EXIT_SECRET: i16 = 244;
const TELEPORT: i16 = 70;
const TELEPORT_NO_FOG: i16 = 71;
const TELEPORT_LINE: i16 = 215;
const LIGHT_STROBE_DOOM: i16 = 232;
const LIGHT_CHANGE_TO_VALUE: i16 = 112;
const LIGHT_MIN_NEIGHBOR: i16 = 233;
const LIGHT_MAX_NEIGHBOR: i16 = 234;
const FLOOR_TRANSFER_TRIGGER: i16 = 235;
const FLOOR_TRANSFER_NUMERIC: i16 = 236;
const TRANSFER_FLOOR_LIGHT: i16 = 210;
const TRANSFER_HEIGHTS: i16 = 209;
const SCROLL_FLOOR: i16 = 223;
const SCROLL_CEILING: i16 = 224;
const SCROLL_TEXTURE_MODEL: i16 = 222;
const SCROLL_TEXTURE_OFFSETS: i16 = 225;
const SECTOR_SET_WIND: i16 = 218;
const SECTOR_SET_FRICTION: i16 = 219;
const SECTOR_SET_CURRENT: i16 = 220;
const POINT_PUSH_SET_FORCE: i16 = 227;
const ELEVATOR_RAISE_TO_NEAREST: i16 = 245;
const ELEVATOR_MOVE_TO_FLOOR: i16 = 246;
const ELEVATOR_LOWER_TO_NEAREST: i16 = 247;
const STAIRS_BUILD_UP_DOOM: i16 = 217;
const STAIRS_BUILD_UP_DOOM_CRUSH: i16 = 217;

/// Translate Boom discrete specials 142–259. `None` if outside this band.
#[must_use]
pub fn translate_boom_discrete(special: u16, tag: i32) -> Option<TranslatedSpecial> {
    let (spec, args) = match special {
        142 | 147 | 178 => (FLOOR_RAISE_BY_VALUE_TIMES8, [tag, F_SLOW, 64, 0, 0]),
        143 | 148 => (PLAT_UP_BY_VALUE_STAY_TX, [tag, P_SLOW / 2, 3, 0, 0]),
        144 | 149 => (PLAT_UP_BY_VALUE_STAY_TX, [tag, P_SLOW / 2, 4, 0, 0]),
        145 | 152 => (CEILING_LOWER_TO_FLOOR, [tag, C_SLOW, 0, 0, 0]),
        146 | 155 | 191 => (FLOOR_DONUT, [tag, DORATE, DORATE, 0, 0]),
        150 | 165 | 185 => (CEILING_CRUSH_AND_RAISE_SILENT_DIST, [tag, 8, C_SLOW, 10, 0]),
        151 => (FLOOR_AND_CEILING_LOWER_RAISE, [tag, F_SLOW, C_SLOW, 0, 0]),
        153 | 154 | 189 | 190 => (FLOOR_TRANSFER_TRIGGER, [tag, 0, 0, 0, 0]),
        156 | 172 | 193 => (LIGHT_STROBE_DOOM, [tag, 5, 35, 0, 0]),
        157 | 173 | 194 => (LIGHT_MIN_NEIGHBOR, [tag, 0, 0, 0, 0]),
        158 | 176 => (FLOOR_RAISE_BY_TEXTURE, [tag, F_SLOW, 0, 0, 0]),
        159 | 177 => (FLOOR_LOWER_TO_LOWEST, [tag, F_SLOW, 0, 0, 0]),
        160 | 179 => (FLOOR_RAISE_BY_VALUE, [tag, F_SLOW, 24, 0, 0]),
        161 | 180 => (FLOOR_RAISE_BY_VALUE, [tag, F_SLOW, 24, 0, 0]),
        162 | 181 => (PLAT_PERPETUAL_RAISE, [tag, P_SLOW, PLATWAIT, 0, 0]),
        163 | 182 => (PLAT_STOP, [tag, 0, 0, 0, 0]),
        164 | 183 => (CEILING_CRUSH_AND_RAISE_DIST, [tag, 8, C_NORMAL, 10, 0]),
        166 | 186 => (
            FLOOR_AND_CEILING_LOWER_RAISE,
            [tag, F_SLOW, C_SLOW, 1998, 0],
        ),
        167 | 187 => (CEILING_LOWER_AND_CRUSH, [tag, C_SLOW, 0, 2, 0]),
        168 | 188 => (CEILING_CRUSH_STOP, [tag, 0, 0, 0, 0]),
        169 | 192 => (LIGHT_MAX_NEIGHBOR, [tag, 0, 0, 0, 0]),
        170 => (LIGHT_CHANGE_TO_VALUE, [tag, 35, 0, 0, 0]),
        171 => (LIGHT_CHANGE_TO_VALUE, [tag, 255, 0, 0, 0]),
        174 | 195 => (TELEPORT, [0, tag, 0, 0, 0]),
        175 | 196 => (DOOR_CLOSE_WAIT_OPEN, [tag, D_SLOW, 240, 0, 0]),
        184 => (CEILING_CRUSH_AND_RAISE_DIST, [tag, 8, C_SLOW, 10, 0]),
        197 => (EXIT_NORMAL, [0, 0, 0, 0, 0]),
        198 => (EXIT_SECRET, [0, 0, 0, 0, 0]),
        199 | 201 | 203 | 205 => (CEILING_LOWER_TO_LOWEST, [tag, C_SLOW, 0, 0, 0]),
        200 | 202 | 204 | 206 => (CEILING_LOWER_TO_HIGHEST_FLOOR, [tag, C_SLOW, 0, 0, 0]),
        207..=210 => (TELEPORT_NO_FOG, [0, 2, tag, 1, 0]),
        211 | 212 => (PLAT_TOGGLE_CEILING, [tag, 0, 0, 0, 0]),
        213 => (TRANSFER_FLOOR_LIGHT, [tag, 0, 0, 0, 0]),
        214 => (SCROLL_CEILING, [tag, 6, 0, 0, 0]),
        215 => (SCROLL_FLOOR, [tag, 6, 0, 0, 0]),
        216 => (SCROLL_FLOOR, [tag, 6, 1, 0, 0]),
        217 => (SCROLL_FLOOR, [tag, 6, 2, 0, 0]),
        218 => (SCROLL_TEXTURE_MODEL, [tag, 2, 0, 0, 0]), // lineid≈tag
        219..=222 => (FLOOR_LOWER_TO_NEAREST, [tag, F_SLOW, 0, 0, 0]),
        223 => (SECTOR_SET_FRICTION, [tag, 0, 0, 0, 0]),
        224 => (SECTOR_SET_WIND, [tag, 0, 0, 1, 0]),
        225 => (SECTOR_SET_CURRENT, [tag, 0, 0, 1, 0]),
        226 => (POINT_PUSH_SET_FORCE, [tag, 0, 0, 1, 0]),
        227..=230 => (ELEVATOR_RAISE_TO_NEAREST, [tag, ELEVATORSPEED, 0, 0, 0]),
        231..=234 => (ELEVATOR_LOWER_TO_NEAREST, [tag, ELEVATORSPEED, 0, 0, 0]),
        235..=238 => (ELEVATOR_MOVE_TO_FLOOR, [tag, ELEVATORSPEED, 0, 0, 0]),
        239..=241 => (FLOOR_TRANSFER_NUMERIC, [tag, 0, 0, 0, 0]),
        242 => (TRANSFER_HEIGHTS, [tag, 0, 0, 0, 0]),
        243 | 244 => (TELEPORT_LINE, [tag, tag, 0, 0, 0]),
        245 => (SCROLL_CEILING, [tag, 5, 0, 0, 0]),
        246 => (SCROLL_FLOOR, [tag, 5, 0, 0, 0]),
        247 => (SCROLL_FLOOR, [tag, 5, 1, 0, 0]),
        248 => (SCROLL_FLOOR, [tag, 5, 2, 0, 0]),
        249 => (SCROLL_TEXTURE_MODEL, [tag, 1, 0, 0, 0]),
        250 => (SCROLL_CEILING, [tag, 4, 0, 0, 0]),
        251 => (SCROLL_FLOOR, [tag, 4, 0, 0, 0]),
        252 => (SCROLL_FLOOR, [tag, 4, 1, 0, 0]),
        253 => (SCROLL_FLOOR, [tag, 4, 2, 0, 0]),
        254 => (SCROLL_TEXTURE_MODEL, [tag, 0, 0, 0, 0]),
        255 => (SCROLL_TEXTURE_OFFSETS, [0, 0, 0, 0, 0]),
        256 | 258 => (STAIRS_BUILD_UP_DOOM, [tag, ST_SLOW, 8, 0, 0]),
        257 | 259 => (STAIRS_BUILD_UP_DOOM_CRUSH, [tag, ST_TURBO, 16, 0, 0]),
        _ => return None,
    };
    Some(TranslatedSpecial {
        special: spec,
        args,
        flags: 0,
        activation: 0,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn transfer_heights() {
        let t = translate_boom_discrete(242, 9).unwrap();
        assert_eq!(t.special, TRANSFER_HEIGHTS);
        assert_eq!(t.args[0], 9);
    }

    #[test]
    fn elevator_raise() {
        let t = translate_boom_discrete(227, 1).unwrap();
        assert_eq!(t.special, ELEVATOR_RAISE_TO_NEAREST);
        assert_eq!(t.args[1], ELEVATORSPEED);
    }

    #[test]
    fn outside_band() {
        assert!(translate_boom_discrete(100, 0).is_none());
        assert!(translate_boom_discrete(300, 0).is_none());
    }
}
