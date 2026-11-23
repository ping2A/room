//! Spawn sprites for Doom's map things, for previews before the playsim
//! feeds the renderer (`room soft-view --things`, tests). The game's own
//! `mobjinfo` spawn states decide this once the shells drive the software
//! renderer (`docs/40` SR3).

use crate::render::{FF_FULLBRIGHT, MF_SHADOW};

const FB: u32 = FF_FULLBRIGHT;

/// `(sprite, frame, flags)` of a Doom / Doom II editor number's spawn
/// state, or `None` for starts, teleport spots and unknown numbers.
#[must_use]
pub fn doom_spawn_sprite(type_id: i32) -> Option<(&'static str, u32, u32)> {
    let (sprite, frame, flags) = match type_id {
        // Monsters.
        3004 => ("POSS", 0, 0),
        9 => ("SPOS", 0, 0),
        65 => ("CPOS", 0, 0),
        3001 => ("TROO", 0, 0),
        3002 => ("SARG", 0, 0),
        58 => ("SARG", 0, MF_SHADOW),
        3006 => ("SKUL", FB, 0),
        3005 => ("HEAD", 0, 0),
        3003 => ("BOSS", 0, 0),
        69 => ("BOS2", 0, 0),
        16 => ("CYBR", 0, 0),
        7 => ("SPID", 0, 0),
        64 => ("VILE", 0, 0),
        66 => ("SKEL", 0, 0),
        67 => ("FATT", 0, 0),
        68 => ("BSPI", 0, 0),
        71 => ("PAIN", 0, 0),
        84 => ("SSWV", 0, 0),
        72 => ("KEEN", 0, 0),
        // Weapons.
        2001 => ("SHOT", 0, 0),
        82 => ("SGN2", 0, 0),
        2002 => ("MGUN", 0, 0),
        2003 => ("LAUN", 0, 0),
        2004 => ("PLAS", 0, 0),
        2005 => ("CSAW", 0, 0),
        2006 => ("BFUG", 0, 0),
        // Ammo.
        2007 => ("CLIP", 0, 0),
        2048 => ("AMMO", 0, 0),
        2008 => ("SHEL", 0, 0),
        2049 => ("SBOX", 0, 0),
        2010 => ("ROCK", 0, 0),
        2046 => ("BROK", 0, 0),
        2047 => ("CELL", 0, 0),
        17 => ("CELP", 0, 0),
        8 => ("BPAK", 0, 0),
        // Health, armour, powerups.
        2011 => ("STIM", 0, 0),
        2012 => ("MEDI", 0, 0),
        2014 => ("BON1", 0, 0),
        2015 => ("BON2", 0, 0),
        2018 => ("ARM1", 0, 0),
        2019 => ("ARM2", 0, 0),
        2013 => ("SOUL", FB, 0),
        83 => ("MEGA", FB, 0),
        2022 => ("PINV", FB, 0),
        2023 => ("PSTR", FB, 0),
        2024 => ("PINS", FB, 0),
        2025 => ("SUIT", FB, 0),
        2026 => ("PMAP", FB, 0),
        2045 => ("PVIS", FB, 0),
        // Keys.
        5 => ("BKEY", 0, 0),
        6 => ("YKEY", 0, 0),
        13 => ("RKEY", 0, 0),
        40 => ("BSKU", 0, 0),
        39 => ("YSKU", 0, 0),
        38 => ("RSKU", 0, 0),
        // Decorations.
        2035 => ("BAR1", 0, 0),
        2028 => ("COLU", FB, 0),
        48 => ("ELEC", 0, 0),
        30 => ("COL1", 0, 0),
        31 => ("COL2", 0, 0),
        32 => ("COL3", 0, 0),
        33 => ("COL4", 0, 0),
        36 => ("COL5", 0, 0),
        37 => ("COL6", 0, 0),
        41 => ("CEYE", FB, 0),
        42 => ("FSKU", FB, 0),
        43 => ("TRE1", 0, 0),
        54 => ("TRE2", 0, 0),
        44 => ("TBLU", FB, 0),
        45 => ("TGRN", FB, 0),
        46 => ("TRED", FB, 0),
        55 => ("SMBT", FB, 0),
        56 => ("SMGT", FB, 0),
        57 => ("SMRT", FB, 0),
        47 => ("SMIT", 0, 0),
        34 => ("CAND", FB, 0),
        35 => ("CBRA", FB, 0),
        85 => ("TLMP", FB, 0),
        86 => ("TLP2", FB, 0),
        70 => ("FCAN", FB, 0),
        24 => ("POL5", 0, 0),
        25 => ("POL1", 0, 0),
        26 => ("POL6", 0, 0),
        27 => ("POL4", 0, 0),
        28 => ("POL2", 0, 0),
        29 => ("POL3", FB, 0),
        // Corpses (last death frames).
        10 | 12 => ("PLAY", 22, 0),
        15 => ("PLAY", 13, 0),
        18 => ("POSS", 11, 0),
        19 | 21 => ("SARG", 13, 0),
        20 => ("TROO", 12, 0),
        22 => ("HEAD", 11, 0),
        23 => ("SKUL", 10, 0),
        _ => return None,
    };
    Some((sprite, frame, flags))
}

/// Map things of `map` as [`SoftThing`](crate::SoftThing)s at their spawn
/// sprites, standing on the floor (hanging things too, as a preview). Only
/// things of skill 3 (flag 0x2) that are not multiplayer-only (0x10).
#[must_use]
pub fn map_things(map: &room_map::MapGeometry, level: &crate::SoftLevel) -> Vec<crate::SoftThing> {
    let mut out = Vec::new();
    for t in &map.things {
        if t.flags & 0x2 == 0 || t.flags & 0x10 != 0 {
            continue;
        }
        let Some((sprite, frame, flags)) = doom_spawn_sprite(i32::from(t.type_id)) else {
            continue;
        };
        let x = (f64::from(t.x) * 65536.0) as i32;
        let y = (f64::from(t.y) * 65536.0) as i32;
        let Some(sector) = level.point_in_sector(x, y) else {
            continue;
        };
        out.push(crate::SoftThing {
            x,
            y,
            z: level.sectors[sector].floor,
            // `ANG45 * (angle / 45)`, as P_SpawnMapThing.
            angle: (0x2000_0000u32).wrapping_mul((i32::from(t.angle) / 45) as u32),
            sprite: crate::sprite_name(sprite),
            frame,
            flags,
            sector: sector as u32,
            ..crate::SoftThing::default()
        });
    }
    out
}
