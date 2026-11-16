//! Chocolate `displayweapon`: the first-person weapon overlay as a list of
//! `myospal` draws in the 320×200 `rotatesprite` plane.
//!
//! Each draw is centred on (x, y) with the tile's picanm offsets, exactly as
//! `rotatesprite` without flag 16; `flip` is `myospal`'s orientation bit 4
//! (angle 1024 + y-flip = horizontal mirror). The renderer scales the plane
//! to the frame. This is view-only: nothing here touches the playsim.

use crate::player::Player;
use room_build_engine::helpers::sincos_q14;

const KNEE: i16 = 2521;
const FIRSTGUN: i16 = 2524;
const SHOTGUN: i16 = 2613;
const CHAINGUN: i16 = 2536;
const RPGGUN: i16 = 2544;
const HANDTHROW: i16 = 2573;
const HANDREMOTE: i16 = 2570;
const SHRINKER: i16 = 2556;
const DEVISTATOR: i16 = 2510;
const FREEZE: i16 = 2548;
const HANDHOLDINGLASER: i16 = 2563;
const FIST: i16 = 1640;

/// One `myospal` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponDraw {
    pub x: i32,
    pub y: i32,
    pub tile: i16,
    pub shade: i8,
    pub pal: u8,
    pub flip: bool,
}

/// What `displayweapon` needs from the world besides the player.
#[derive(Clone, Copy, Debug, Default)]
pub struct WeaponViewEnv {
    /// Body sprite shade (clamped to 24 like Chocolate's `gs`).
    pub body_shade: i8,
    pub body_pal: u8,
    pub body_xrepeat: u8,
    pub body_xvel: i16,
    pub body_extra: i16,
    /// `sector[cursectnum].floorpal`.
    pub floorpal: u8,
    /// Render-only jitter source (Chocolate uses C `rand()`); pass 0 for a
    /// steady picture.
    pub rand: u32,
}

fn sin(a: i32) -> i32 {
    sincos_q14(a & 2047).0
}

/// Chocolate `displayweapon` (Atomic) for `player`.
#[must_use]
pub fn display_weapon(p: &Player, env: &WeaponViewEnv) -> Vec<WeaponDraw> {
    let mut out = Vec::new();
    if env.body_pal != 1 && env.body_extra <= 0 {
        return out;
    }
    let mut rng = env.rand;
    let mut rand = move || {
        rng = rng.wrapping_mul(1_103_515_245).wrapping_add(12345);
        ((rng >> 16) & 0x7fff) as i32
    };
    let kb = i32::from(p.kickback_pic);
    let look = i32::from(p.look_ang) >> 1;
    let mut looking_arc = i32::from(p.look_ang).abs() / 9;
    let gs = env.body_shade.min(24);
    let wp = i32::from(p.weapon_pos);
    let mut gun_pos = 80 - wp * wp;
    let mut weapon_xoffset = 160 - 90;
    weapon_xoffset -= sin((p.weapon_sway >> 1) + 512) / (1024 + 512);
    weapon_xoffset -= 58 + i32::from(p.weapon_ang);
    if env.body_xrepeat < 32 {
        gun_pos -= (sin(p.weapon_sway << 2) >> 9).abs();
    } else {
        gun_pos -= (sin(p.weapon_sway >> 1) >> 10).abs();
    }
    gun_pos -= i32::from(p.hard_landing) << 3;
    let pal = if env.body_pal == 1 { 1 } else { env.floorpal };
    let mut d = |x: i32, y: i32, tile: i16, shade: i8, flip: bool, pal: u8| {
        out.push(WeaponDraw {
            x,
            y,
            tile,
            shade,
            pal,
            flip,
        });
    };

    // Quick kick (`quick_kick` counts down from 14).
    let j = 14 - i32::from(p.quick_kick);
    if j != 14 {
        if !(5..=9).contains(&j) {
            d(
                weapon_xoffset + 80 - look,
                looking_arc + 250 - gun_pos,
                KNEE,
                gs,
                true,
                pal,
            );
        } else {
            d(
                weapon_xoffset + 160 - 16 - look,
                looking_arc + 214 - gun_pos,
                KNEE + 1,
                gs,
                true,
                pal,
            );
        }
    }

    // Shrunk player: two fists.
    if env.body_xrepeat < 40 {
        let mut fistsign = 0;
        if p.jetpack_on == 0 {
            let i = i32::from(env.body_xvel);
            looking_arc += 32 - (i >> 1);
            fistsign += i >> 1;
        }
        let base = weapon_xoffset;
        let fx = base + (sin(fistsign) >> 10);
        d(
            fx + 250 - look,
            looking_arc + 258 - (sin(fistsign) >> 8).abs(),
            FIST,
            gs,
            false,
            env.floorpal,
        );
        let fx = base - (sin(fistsign) >> 10);
        d(
            fx + 40 - look,
            looking_arc + 200 + (sin(fistsign) >> 8).abs(),
            FIST,
            gs,
            true,
            env.floorpal,
        );
        return out;
    }

    let cw = if p.weapon_pos < 0 && (0..=11).contains(&p.last_weapon) {
        p.last_weapon
    } else {
        p.curr_weapon
    };
    match cw {
        0 => {
            if kb > 0 {
                if !(5..=9).contains(&kb) {
                    d(
                        weapon_xoffset + 220 - look,
                        looking_arc + 250 - gun_pos,
                        KNEE,
                        gs,
                        false,
                        pal,
                    );
                } else {
                    d(
                        weapon_xoffset + 160 - look,
                        looking_arc + 214 - gun_pos,
                        KNEE + 1,
                        gs,
                        false,
                        pal,
                    );
                }
            }
        }
        8 => {
            weapon_xoffset += 8;
            gun_pos -= 10;
            if kb > 6 {
                looking_arc += kb << 3;
            } else if kb < 4 {
                d(
                    weapon_xoffset + 142 - look,
                    looking_arc + 234 - gun_pos,
                    HANDHOLDINGLASER + 3,
                    gs,
                    false,
                    pal,
                );
            }
            let t = HANDHOLDINGLASER + (kb >> 2) as i16;
            d(
                weapon_xoffset + 130 - look,
                looking_arc + 249 - gun_pos,
                t,
                gs,
                false,
                pal,
            );
            d(
                weapon_xoffset + 152 - look,
                looking_arc + 249 - gun_pos,
                t,
                gs,
                true,
                pal,
            );
        }
        4 => {
            weapon_xoffset -= sin(768 + (kb << 7)) >> 11;
            gun_pos += sin(768 + ((kb << 7) & 2047)) >> 11;
            if kb > 0 && kb < 8 {
                d(
                    weapon_xoffset + 164,
                    (looking_arc << 1) + 176 - gun_pos,
                    RPGGUN + (kb >> 1) as i16,
                    gs,
                    false,
                    pal,
                );
            }
            d(
                weapon_xoffset + 164,
                (looking_arc << 1) + 176 - gun_pos,
                RPGGUN,
                gs,
                false,
                pal,
            );
        }
        2 => {
            weapon_xoffset -= 8;
            match kb {
                0..=2 | 6..=8 => {
                    if kb == 1 || kb == 2 {
                        d(
                            weapon_xoffset + 168 - look,
                            looking_arc + 201 - gun_pos,
                            SHOTGUN + 2,
                            -128,
                            false,
                            pal,
                        );
                    }
                    d(
                        weapon_xoffset + 146 - look,
                        looking_arc + 202 - gun_pos,
                        SHOTGUN,
                        gs,
                        false,
                        pal,
                    );
                }
                3..=5 | 9..=12 => {
                    if kb > 1 && kb < 5 {
                        gun_pos -= 40;
                        weapon_xoffset += 20;
                        let t = SHOTGUN + 1 + ((kb - 1) >> 1) as i16;
                        d(
                            weapon_xoffset + 178 - look,
                            looking_arc + 194 - gun_pos,
                            t,
                            -128,
                            false,
                            pal,
                        );
                    }
                    d(
                        weapon_xoffset + 158 - look,
                        looking_arc + 220 - gun_pos,
                        SHOTGUN + 3,
                        gs,
                        false,
                        pal,
                    );
                }
                13..=15 => d(
                    32 + weapon_xoffset + 166 - look,
                    looking_arc + 210 - gun_pos,
                    SHOTGUN + 4,
                    gs,
                    false,
                    pal,
                ),
                16..=19 => d(
                    64 + weapon_xoffset + 170 - look,
                    looking_arc + 196 - gun_pos,
                    SHOTGUN + 5,
                    gs,
                    false,
                    pal,
                ),
                20..=23 => d(
                    64 + weapon_xoffset + 176 - look,
                    looking_arc + 196 - gun_pos,
                    SHOTGUN + 6,
                    gs,
                    false,
                    pal,
                ),
                24..=27 => d(
                    64 + weapon_xoffset + 170 - look,
                    looking_arc + 196 - gun_pos,
                    SHOTGUN + 5,
                    gs,
                    false,
                    pal,
                ),
                28..=30 => d(
                    32 + weapon_xoffset + 156 - look,
                    looking_arc + 206 - gun_pos,
                    SHOTGUN + 4,
                    gs,
                    false,
                    pal,
                ),
                _ => {}
            }
        }
        3 => {
            if kb > 0 {
                gun_pos -= sin(kb << 7) >> 12;
                if env.body_pal != 1 {
                    weapon_xoffset += 1 - (rand() & 3);
                }
            }
            d(
                weapon_xoffset + 168 - look,
                looking_arc + 260 - gun_pos,
                CHAINGUN,
                gs,
                false,
                pal,
            );
            if kb == 0 {
                d(
                    weapon_xoffset + 178 - look,
                    looking_arc + 233 - gun_pos,
                    CHAINGUN + 1,
                    gs,
                    false,
                    pal,
                );
            } else {
                if kb > 4 && kb < 12 {
                    let t = CHAINGUN + 5 + ((kb - 4) / 5) as i16;
                    let i = if env.body_pal != 1 { rand() & 7 } else { 0 };
                    d(
                        i + weapon_xoffset - 4 + 140 - look,
                        i + looking_arc - (kb >> 1) + 208 - gun_pos,
                        t,
                        gs,
                        false,
                        pal,
                    );
                    let i = if env.body_pal != 1 { rand() & 7 } else { 0 };
                    d(
                        i + weapon_xoffset - 4 + 184 - look,
                        i + looking_arc - (kb >> 1) + 208 - gun_pos,
                        t,
                        gs,
                        false,
                        pal,
                    );
                }
                if kb < 8 {
                    let i = rand() & 7;
                    let t = CHAINGUN + 5 + ((kb - 2) / 5) as i16;
                    d(
                        i + weapon_xoffset - 4 + 162 - look,
                        i + looking_arc - (kb >> 1) + 208 - gun_pos,
                        t,
                        gs,
                        false,
                        pal,
                    );
                    d(
                        weapon_xoffset + 178 - look,
                        looking_arc + 233 - gun_pos,
                        CHAINGUN + 1 + (kb >> 1) as i16,
                        gs,
                        false,
                        pal,
                    );
                } else {
                    d(
                        weapon_xoffset + 178 - look,
                        looking_arc + 233 - gun_pos,
                        CHAINGUN + 1,
                        gs,
                        false,
                        pal,
                    );
                }
            }
        }
        1 => {
            if kb < 5 {
                const KB_FRAMES: [i16; 5] = [0, 1, 2, 0, 0];
                let mut l = 195 - 12 + weapon_xoffset;
                if kb == 2 {
                    l -= 3;
                }
                d(
                    l - look,
                    looking_arc + 244 - gun_pos,
                    FIRSTGUN + KB_FRAMES[kb.max(0) as usize],
                    gs,
                    false,
                    pal,
                );
            } else if kb < 10 {
                d(
                    194 - look,
                    looking_arc + 230 - gun_pos,
                    FIRSTGUN + 4,
                    gs,
                    false,
                    pal,
                );
            } else if kb < 15 {
                d(
                    244 - (kb << 3) - look,
                    looking_arc + 130 - gun_pos + (kb << 4),
                    FIRSTGUN + 6,
                    gs,
                    false,
                    pal,
                );
                d(
                    224 - look,
                    looking_arc + 220 - gun_pos,
                    FIRSTGUN + 5,
                    gs,
                    false,
                    pal,
                );
            } else if kb < 20 {
                d(
                    124 + (kb << 1) - look,
                    looking_arc + 430 - gun_pos - (kb << 3),
                    FIRSTGUN + 6,
                    gs,
                    false,
                    pal,
                );
                d(
                    224 - look,
                    looking_arc + 220 - gun_pos,
                    FIRSTGUN + 5,
                    gs,
                    false,
                    pal,
                );
            } else if kb < 23 {
                d(
                    184 - look,
                    looking_arc + 235 - gun_pos,
                    FIRSTGUN + 8,
                    gs,
                    false,
                    pal,
                );
                d(
                    224 - look,
                    looking_arc + 210 - gun_pos,
                    FIRSTGUN + 5,
                    gs,
                    false,
                    pal,
                );
            } else if kb < 25 {
                d(
                    164 - look,
                    looking_arc + 245 - gun_pos,
                    FIRSTGUN + 8,
                    gs,
                    false,
                    pal,
                );
                d(
                    224 - look,
                    looking_arc + 220 - gun_pos,
                    FIRSTGUN + 5,
                    gs,
                    false,
                    pal,
                );
            } else if kb < 27 {
                d(
                    194 - look,
                    looking_arc + 235 - gun_pos,
                    FIRSTGUN + 5,
                    gs,
                    false,
                    pal,
                );
            }
        }
        5 => {
            if kb > 0 {
                const THROW: [i16; 21] = [
                    0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2,
                ];
                if kb < 7 {
                    gun_pos -= 10 * kb;
                } else if kb < 12 {
                    gun_pos += 20 * (kb - 10);
                } else if kb < 20 {
                    gun_pos -= 9 * (kb - 14);
                }
                let f = THROW.get(kb as usize).copied().unwrap_or(0);
                d(
                    weapon_xoffset + 190 - look,
                    looking_arc + 250 - gun_pos,
                    HANDTHROW + f,
                    gs,
                    false,
                    pal,
                );
            } else {
                d(
                    weapon_xoffset + 190 - look,
                    looking_arc + 260 - gun_pos,
                    HANDTHROW,
                    gs,
                    false,
                    pal,
                );
            }
        }
        10 => {
            const REMOTE: [i16; 11] = [0, 1, 1, 2, 1, 1, 0, 0, 0, 0, 0];
            let x = -48 + 150 - look;
            let f = REMOTE.get(kb.max(0) as usize).copied().unwrap_or(0);
            d(
                x,
                looking_arc + 258 - gun_pos,
                HANDREMOTE + f,
                gs,
                false,
                pal,
            );
        }
        7 => {
            if kb > 0 {
                const CYCLOIDY: [i32; 7] = [0, 4, 12, 24, 12, 4, 0];
                let c = CYCLOIDY.get(kb as usize).copied().unwrap_or(0);
                let i = (kb >> 2).signum() as i16;
                if p.hbomb_hold_delay != 0 {
                    d(
                        (c >> 1) + weapon_xoffset + 268 - look,
                        c + looking_arc + 238 - gun_pos,
                        DEVISTATOR + i,
                        -32,
                        false,
                        pal,
                    );
                    d(
                        weapon_xoffset + 30 - look,
                        looking_arc + 240 - gun_pos,
                        DEVISTATOR,
                        gs,
                        true,
                        pal,
                    );
                } else {
                    d(
                        -(c >> 1) + weapon_xoffset + 30 - look,
                        c + looking_arc + 240 - gun_pos,
                        DEVISTATOR + i,
                        -32,
                        true,
                        pal,
                    );
                    d(
                        weapon_xoffset + 268 - look,
                        looking_arc + 238 - gun_pos,
                        DEVISTATOR,
                        gs,
                        false,
                        pal,
                    );
                }
            } else {
                d(
                    weapon_xoffset + 268 - look,
                    looking_arc + 238 - gun_pos,
                    DEVISTATOR,
                    gs,
                    false,
                    pal,
                );
                d(
                    weapon_xoffset + 30 - look,
                    looking_arc + 240 - gun_pos,
                    DEVISTATOR,
                    gs,
                    true,
                    pal,
                );
            }
        }
        9 => {
            if kb > 0 {
                const CAT: [i16; 6] = [0, 0, 1, 1, 2, 2];
                if env.body_pal != 1 {
                    weapon_xoffset += rand() & 3;
                    looking_arc += rand() & 3;
                }
                gun_pos -= 16;
                d(
                    weapon_xoffset + 210 - look,
                    looking_arc + 261 - gun_pos,
                    FREEZE + 2,
                    -32,
                    false,
                    pal,
                );
                d(
                    weapon_xoffset + 210 - look,
                    looking_arc + 235 - gun_pos,
                    FREEZE + 3 + CAT[(kb % 6) as usize],
                    -32,
                    false,
                    pal,
                );
            } else {
                d(
                    weapon_xoffset + 210 - look,
                    looking_arc + 261 - gun_pos,
                    FREEZE,
                    gs,
                    false,
                    pal,
                );
            }
        }
        6 | 11 => {
            weapon_xoffset += 28;
            looking_arc += 18;
            let grow = cw == 11;
            let glow_pal = if grow { 2 } else { 0 };
            if kb == 0 {
                d(
                    weapon_xoffset + 184 - look,
                    looking_arc + 240 - gun_pos,
                    SHRINKER + 2,
                    16,
                    false,
                    glow_pal,
                );
                d(
                    weapon_xoffset + 188 - look,
                    looking_arc + 240 - gun_pos,
                    if grow { SHRINKER - 2 } else { SHRINKER },
                    gs,
                    false,
                    pal,
                );
            } else {
                if env.body_pal != 1 {
                    weapon_xoffset += rand() & 3;
                    gun_pos += rand() & 3;
                }
                d(
                    weapon_xoffset + 184 - look,
                    looking_arc + 240 - gun_pos,
                    SHRINKER + 3 + (kb & 3) as i16,
                    -32,
                    false,
                    glow_pal,
                );
                d(
                    weapon_xoffset + 188 - look,
                    looking_arc + 240 - gun_pos,
                    if grow { SHRINKER - 1 } else { SHRINKER + 1 },
                    gs,
                    false,
                    pal,
                );
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> WeaponViewEnv {
        WeaponViewEnv {
            body_xrepeat: 42,
            body_extra: 100,
            ..WeaponViewEnv::default()
        }
    }

    #[test]
    fn pistol_idle_is_one_firstgun_draw() {
        let p = Player {
            curr_weapon: 1,
            last_weapon: -1,
            weapon_sway: 1024,
            ..Player::default()
        };
        let d = display_weapon(&p, &env());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].tile, FIRSTGUN);
    }

    #[test]
    fn pistol_reload_shows_clip_and_hand() {
        let mut p = Player {
            curr_weapon: 1,
            last_weapon: -1,
            weapon_sway: 1024,
            ..Player::default()
        };
        let mut tiles = Vec::new();
        for kb in 0..27 {
            p.kickback_pic = kb;
            tiles.extend(display_weapon(&p, &env()).into_iter().map(|d| d.tile));
        }
        for t in [FIRSTGUN + 4, FIRSTGUN + 5, FIRSTGUN + 6, FIRSTGUN + 8] {
            assert!(tiles.contains(&t), "reload frame {t} missing");
        }
    }
}
