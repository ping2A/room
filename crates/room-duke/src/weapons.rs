//! Weapon inventory helpers — Chocolate `addweapon` / `addammo` (D6).

use crate::player::Player;

/// Atomic `SHOTGUN_WEAPON`.
pub const SHOTGUN_WEAPON: i16 = 2;
/// Atomic `PISTOL_WEAPON`.
pub const PISTOL_WEAPON: i16 = 1;
/// Atomic `KNEE_WEAPON`.
pub const KNEE_WEAPON: i16 = 0;

/// Chocolate `max_ammo_amount[]` defaults (Atomic) when `gamestartup` is absent.
pub fn max_ammo_amount(weapon: i16) -> i16 {
    match weapon {
        0 => 0,   // knee
        1 => 200, // pistol
        2 => 50,  // shotgun
        3 => 200, // chaingun
        4 => 50,  // RPG
        5 => 50,  // pipebomb
        6 => 50,  // shrinker
        7 => 99,  // devastator
        8 => 10,  // tripbomb
        9 => 99,  // freeze
        10 => 0,  // hand remote
        11 => 50, // grow
        _ => 50,
    }
}

/// Chocolate `addammo`.
pub fn addammo(p: &mut Player, weapon: i16, amount: i16) {
    let w = weapon as usize;
    if w >= p.ammo.len() {
        return;
    }
    let max = max_ammo_amount(weapon);
    p.ammo[w] = p.ammo[w].saturating_add(amount).min(max);
}

/// Chocolate `addweapon` (demo path always autoswitches).
pub fn addweapon(p: &mut Player, weapon: i16) {
    let w = weapon as usize;
    if w >= p.gotweapon.len() {
        return;
    }
    if !p.gotweapon[w] {
        p.gotweapon[w] = true;
        if weapon == 6 {
            // SHRINKER also grants GROW
            if p.gotweapon.len() > 11 {
                p.gotweapon[11] = true;
            }
        }
    }
    p.kickback_pic = 0;
    p.last_weapon = p.curr_weapon;
    p.weapon_pos = -1;
    p.curr_weapon = weapon;
}

/// Apply CON `addweapon` (true) or `addammo` (false). Returns `false` when
/// Chocolate would set `killit_flag = 2` (ammo already full — abort script, keep sprite).
pub fn con_weapon_op(p: &mut Player, is_addweapon: bool, weapon: i16, ammo: i16) -> bool {
    let w = weapon as usize;
    if w >= p.ammo.len() {
        return true;
    }
    if is_addweapon {
        let had = p.gotweapon[w];
        if !had {
            addweapon(p, weapon);
        } else if p.ammo[w] >= max_ammo_amount(weapon) {
            return false;
        }
        addammo(p, weapon, ammo);
        if p.curr_weapon == KNEE_WEAPON && p.gotweapon[w] {
            addweapon(p, weapon);
        }
    } else {
        if p.ammo[w] >= max_ammo_amount(weapon) {
            return false;
        }
        addammo(p, weapon, ammo);
        if p.curr_weapon == KNEE_WEAPON && p.gotweapon[w] {
            addweapon(p, weapon);
        }
    }
    true
}
