//! Doom status-bar face (`ST_updateFaceWidget`) and pain / pickup palette
//! (`ST_doPaletteStuff`), ticked after the playsim like `ST_Ticker`.

use crate::actor::ActorId;
use crate::player::{MAX_PLAYERS, Player, TICRATE};
use crate::world::World;
use room_core::{Fixed, math};

const PAIN_FACES: i32 = 5;
const FACE_STRIDE: u8 = 8;
const TURN_OFFSET: u8 = 3;
const OUCH_OFFSET: u8 = 5;
const EVIL_GRIN_OFFSET: u8 = 6;
const RAMPAGE_OFFSET: u8 = 7;
const GOD_FACE: u8 = 40;
const DEAD_FACE: u8 = 41;
const EVIL_GRIN_COUNT: i32 = 2 * TICRATE;
const STRAIGHT_FACE_COUNT: i32 = TICRATE / 2;
const TURN_COUNT: i32 = TICRATE;
const RAMPAGE_DELAY: i32 = 2 * TICRATE;
const MUCH_PAIN: i32 = 20;
const ANG45: u32 = 0x2000_0000;
const ANG180: u32 = 0x8000_0000;

/// Face widget state (`st_faceindex`, `st_facecount`, `priority`, …).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MugShot {
    index: u8,
    count: i32,
    priority: u8,
    last_attack_down: i32,
    old_health: i32,
    old_weapons: u32,
    primed: bool,
}

impl MugShot {
    /// Current face lump (`STFST00`, `STFTL20`, `STFOUCH1`, `STFGOD0`, …).
    #[must_use]
    pub fn lump(&self) -> String {
        match self.index {
            GOD_FACE => "STFGOD0".into(),
            DEAD_FACE => "STFDEAD0".into(),
            i => {
                let pain = i / FACE_STRIDE;
                match i % FACE_STRIDE {
                    o @ 0..=2 => format!("STFST{pain}{o}"),
                    TURN_OFFSET => format!("STFTR{pain}0"),
                    4 => format!("STFTL{pain}0"),
                    OUCH_OFFSET => format!("STFOUCH{pain}"),
                    EVIL_GRIN_OFFSET => format!("STFEVL{pain}"),
                    _ => format!("STFKILL{pain}"),
                }
            }
        }
    }
}

/// `ST_calcPainOffset`: 0, 8, …, 32 for 100..80 → 19..0 health.
fn pain_offset(health: i32) -> u8 {
    let h = health.clamp(0, 100);
    FACE_STRIDE * (((100 - h) * PAIN_FACES) / 101) as u8
}

/// One bit per distinct owned weapon class (slot order), for the evil grin.
fn owned_weapon_bits(world: &World, mo: ActorId) -> u32 {
    let mut bits = 0u32;
    let mut bit = 0u32;
    for slot in 1u8..=9 {
        for name in world.weapon_slots.weapons_for(slot) {
            if bit >= 32 {
                return bits;
            }
            if world
                .classes
                .find_name(&name)
                .is_some_and(|cid| world.inventory.count(mo, cid) > 0)
            {
                bits |= 1 << bit;
            }
            bit += 1;
        }
    }
    bits
}

/// Advance every player's face one tic.
pub(crate) fn tick_players(world: &mut World) {
    for i in 0..MAX_PLAYERS {
        let Some(mut player) = world.players[i].take() else {
            continue;
        };
        update_face(world, &mut player);
        world.players[i] = Some(player);
    }
}

fn update_face(world: &mut World, pl: &mut Player) {
    let random = world.vanilla_rng.m_random();
    let Some(mo) = pl.mo else {
        return;
    };
    let Some(actor) = world.actors.get(mo) else {
        return;
    };
    let health = actor.health.max(0);
    let (mx, my) = (actor.pos.x(), actor.pos.y());
    let mangle = actor.angle.to_bam().0;
    let weapons = owned_weapon_bits(world, mo);
    let attacker = pl
        .attacker
        .filter(|&a| a != mo)
        .and_then(|a| world.actors.get(a))
        .map(|a| (a.pos.x(), a.pos.y()));
    let ms = &mut pl.mugshot;
    if !ms.primed {
        *ms = MugShot {
            primed: true,
            old_weapons: weapons,
            old_health: health,
            last_attack_down: -1,
            ..MugShot::default()
        };
    }
    // `Player.MugshotMaxHealth` (else `Player.MaxHealth`, else 100) is the
    // health that counts as the healthiest face.
    let max_health = world
        .actors
        .get(mo)
        .and_then(|a| world.classes.get(a.class))
        .map_or(100, |c| {
            c.ext
                .stored_i32("player.mugshotmaxhealth")
                .filter(|&m| m > 0)
                .unwrap_or(if c.ext.player_max_health > 0 {
                    c.ext.player_max_health
                } else {
                    100
                })
        });
    let pain = if max_health == 100 {
        pain_offset(health)
    } else {
        pain_offset(health.saturating_mul(100) / max_health.max(1))
    };
    // GZDoom `st_oldouch false`: the ouch face is for a big *loss* (vanilla
    // compared the wrong way round and only showed it while healing).
    let ouch = ms.old_health - health > MUCH_PAIN;

    if ms.priority < 10 && health == 0 {
        ms.priority = 9;
        ms.index = DEAD_FACE;
        ms.count = 1;
    }
    if ms.priority < 9 && pl.bonus_count > 0 && weapons != ms.old_weapons {
        ms.old_weapons = weapons;
        ms.priority = 8;
        ms.count = EVIL_GRIN_COUNT;
        ms.index = pain + EVIL_GRIN_OFFSET;
    }
    if ms.priority < 8 && pl.damage_count > 0 {
        if let Some((ax, ay)) = attacker {
            ms.priority = 7;
            ms.count = TURN_COUNT;
            ms.index = if ouch {
                pain + OUCH_OFFSET
            } else {
                let bad = math::point_to_angle2(
                    Fixed::from_f64(mx).0,
                    Fixed::from_f64(my).0,
                    Fixed::from_f64(ax).0,
                    Fixed::from_f64(ay).0,
                );
                let (diff, right) = if bad > mangle {
                    let d = bad.wrapping_sub(mangle);
                    (d, d > ANG180)
                } else {
                    let d = mangle.wrapping_sub(bad);
                    (d, d <= ANG180)
                };
                pain + if diff < ANG45 {
                    RAMPAGE_OFFSET
                } else if right {
                    TURN_OFFSET
                } else {
                    TURN_OFFSET + 1
                }
            };
        }
    }
    if ms.priority < 7 && pl.damage_count > 0 {
        ms.count = TURN_COUNT;
        if ouch {
            ms.priority = 7;
            ms.index = pain + OUCH_OFFSET;
        } else {
            ms.priority = 6;
            ms.index = pain + RAMPAGE_OFFSET;
        }
    }
    if ms.priority < 6 {
        if pl.attack_down {
            if ms.last_attack_down == -1 {
                ms.last_attack_down = RAMPAGE_DELAY;
            } else {
                ms.last_attack_down -= 1;
                if ms.last_attack_down == 0 {
                    ms.priority = 5;
                    ms.index = pain + RAMPAGE_OFFSET;
                    ms.count = 1;
                    ms.last_attack_down = 1;
                }
            }
        } else {
            ms.last_attack_down = -1;
        }
    }
    if ms.priority < 5 && (pl.god || pl.invulnerable) {
        ms.priority = 4;
        ms.index = GOD_FACE;
        ms.count = 1;
    }
    if ms.count == 0 {
        ms.index = pain + random % 3;
        ms.count = STRAIGHT_FACE_COUNT;
        ms.priority = 0;
    }
    ms.count -= 1;
    ms.old_health = health;
}

/// `ST_doPaletteStuff` PLAYPAL index: `0` normal, `1..=8` red (pain /
/// berserk), `9..=12` gold (pickup), `13` green (radiation suit / night
/// vision), `14` blue (Duke underwater, not a Doom PLAYPAL slot).
#[must_use]
pub fn palette_index(pl: &Player) -> u8 {
    palette_for(pl, 0)
}

/// Like [`palette_index`], with `waterlevel` 3+ for a full-submersion tint.
#[must_use]
pub fn palette_for(pl: &Player, waterlevel: i32) -> u8 {
    let mut cnt = pl.damage_count;
    if pl.berserk {
        cnt = cnt.max(12 - (pl.berserk_tics >> 6));
    }
    if cnt > 0 {
        ((cnt + 7) >> 3).min(7) as u8 + 1
    } else if pl.bonus_count > 0 {
        ((pl.bonus_count + 7) >> 3).min(3) as u8 + 9
    } else if pl.radsuit_tics > 4 * 32
        || pl.radsuit_tics & 8 != 0
        || (pl.infrared && infrared_palette_on(pl.infrared_tics))
    {
        13
    } else if waterlevel >= 3 {
        14
    } else {
        0
    }
}

fn infrared_palette_on(tics: i32) -> bool {
    tics <= 0 || tics > 4 * 32 || tics & 8 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(index: u8) -> MugShot {
        MugShot {
            index,
            ..MugShot::default()
        }
    }

    #[test]
    fn lump_names_follow_st_stuff_layout() {
        assert_eq!(shot(0).lump(), "STFST00");
        assert_eq!(shot(2).lump(), "STFST02");
        assert_eq!(shot(8 + 3).lump(), "STFTR10");
        assert_eq!(shot(16 + 4).lump(), "STFTL20");
        assert_eq!(shot(24 + 5).lump(), "STFOUCH3");
        assert_eq!(shot(32 + 6).lump(), "STFEVL4");
        assert_eq!(shot(7).lump(), "STFKILL0");
        assert_eq!(shot(GOD_FACE).lump(), "STFGOD0");
        assert_eq!(shot(DEAD_FACE).lump(), "STFDEAD0");
    }

    #[test]
    fn pain_offset_matches_vanilla_bands() {
        assert_eq!(pain_offset(100), 0);
        assert_eq!(pain_offset(80), 0);
        assert_eq!(pain_offset(79), 8);
        assert_eq!(pain_offset(60), 8);
        assert_eq!(pain_offset(59), 16);
        assert_eq!(pain_offset(19), 32);
        assert_eq!(pain_offset(-5), 32);
        assert_eq!(pain_offset(200), 0);
    }

    #[test]
    fn palette_priority_red_then_gold_then_suit() {
        let mut p = Player::default();
        assert_eq!(palette_index(&p), 0);
        p.bonus_count = 6;
        assert_eq!(palette_index(&p), 10);
        p.damage_count = 100;
        assert_eq!(palette_index(&p), 8);
        p.damage_count = 1;
        assert_eq!(palette_index(&p), 2);
        p.damage_count = 0;
        p.bonus_count = 0;
        p.radsuit_tics = 1000;
        assert_eq!(palette_index(&p), 13);
        p.radsuit_tics = 100; // blinking: bit 3 clear → off
        assert_eq!(palette_index(&p), 0);
        p.infrared = true;
        p.infrared_tics = 1000;
        assert_eq!(palette_index(&p), 13);
        p.infrared_tics = 100; // blinking: bit 3 clear → off
        assert_eq!(palette_index(&p), 0);
        p.infrared = false;
        p.berserk = true;
        p.berserk_tics = 1;
        assert_eq!(palette_index(&p), 3);
        p.berserk_tics = 64 * 12;
        assert_eq!(palette_index(&p), 0);
        p.berserk = false;
        assert_eq!(palette_for(&p, 3), 14);
        p.damage_count = 20;
        assert_eq!(palette_for(&p, 3), 4);
    }
}
