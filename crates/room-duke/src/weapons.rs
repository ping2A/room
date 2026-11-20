//! Weapon inventory helpers — Chocolate `addweapon` / `addammo` (D6).

use crate::player::Player;

/// Packed `sync[].bits` weapon selector field from Chocolate `getinput()`.
pub const INPUT_WEAPON_SHIFT: u32 = 8;
/// Packed selector field mask (`0`: none, `1..=10`: slots 0..9, `11/12`: cycle).
pub const INPUT_WEAPON_MASK: u32 = 0x0f << INPUT_WEAPON_SHIFT;

/// Atomic `SHOTGUN_WEAPON`.
pub const SHOTGUN_WEAPON: i16 = 2;
/// Atomic `PISTOL_WEAPON`.
pub const PISTOL_WEAPON: i16 = 1;
/// Atomic `KNEE_WEAPON`.
pub const KNEE_WEAPON: i16 = 0;

/// Map Duke's physical weapon hotkeys (`1`..`9`, `0`) to player slots.
/// Slot zero is the knee attack and has no direct number key.
#[must_use]
pub fn weapon_slot_from_hotkey(key: u8) -> Option<i16> {
    match key {
        1..=9 => Some(i16::from(key)),
        10 => Some(10),
        _ => None,
    }
}

/// Select an owned weapon with usable ammunition (or the ammo-free knee/remote).
/// Returns false when the slot is invalid, unavailable, or already selected.
pub fn select_weapon(p: &mut Player, weapon: i16) -> bool {
    let index = usize::try_from(weapon).ok();
    let Some(index) = index.filter(|&index| index < p.gotweapon.len() && index < p.ammo.len())
    else {
        return false;
    };
    let ammo_free = matches!(weapon, KNEE_WEAPON | 10);
    if weapon == p.curr_weapon || !p.gotweapon[index] || (p.ammo[index] <= 0 && !ammo_free) {
        return false;
    }
    p.last_weapon = p.curr_weapon;
    p.curr_weapon = weapon;
    p.kickback_pic = 0;
    p.weapon_pos = -1;
    true
}

/// Apply Chocolate's direct weapon request when the lower/raise animation is
/// ready. Direct hotkeys can select an owned empty gun; cycle selection skips
/// weapons without ammunition. A firing weapon defers a direct choice until
/// its kickback completes (`wantweaponfire`).
pub fn apply_weapon_input(p: &mut Player, input_bits: u32) -> bool {
    let choice = ((input_bits & INPUT_WEAPON_MASK) >> INPUT_WEAPON_SHIFT) as i16;
    if choice == 0 {
        return false;
    }
    if p.kickback_pic > 0 {
        if (2..=15).contains(&choice) {
            p.want_weapon_fire = choice - 1;
        }
        return false;
    }
    if p.weapon_pos != 0 && p.weapon_pos != -9 {
        return false;
    }
    match choice {
        1..=10 => {
            let weapon = choice - 1;
            let Some(index) = usize::try_from(weapon)
                .ok()
                .filter(|&index| index < p.gotweapon.len())
            else {
                return false;
            };
            if !p.gotweapon[index] || p.curr_weapon == weapon {
                return false;
            }
            // Hand bombs and trip bombs need inventory to be usable; other
            // direct hotkeys may intentionally raise an empty weapon.
            if matches!(weapon, 5 | 8) && p.ammo[index] <= 0 {
                return false;
            }
            switch_weapon(p, weapon);
            true
        }
        11 => cycle_weapon(p, -1),
        12 => cycle_weapon(p, 1),
        _ => false,
    }
}

/// Apply a queued `wantweaponfire` after the current weapon reaches idle.
pub fn apply_pending_weapon(p: &mut Player) -> bool {
    let requested = std::mem::replace(&mut p.want_weapon_fire, -1);
    if requested < 0 || requested == p.curr_weapon {
        return false;
    }
    let index = requested as usize;
    if p.gotweapon.get(index).copied().unwrap_or(false)
        && p.ammo.get(index).copied().unwrap_or(0) > 0
    {
        switch_weapon(p, requested);
        true
    } else {
        false
    }
}

/// Switch to an owned weapon slot (as after EDuke32 `EVENT_SELECTWEAPON`
/// chose it); `false` when the slot is outside Room's weapon table, not
/// owned, or already current.
pub fn switch_to_owned_weapon(p: &mut Player, weapon: i16) -> bool {
    if !p.has_weapon(weapon) || weapon == p.curr_weapon {
        return false;
    }
    switch_weapon(p, weapon);
    true
}

fn switch_weapon(p: &mut Player, weapon: i16) {
    p.last_weapon = p.curr_weapon;
    p.curr_weapon = weapon;
    p.kickback_pic = 0;
    p.want_weapon_fire = -1;
    p.weapon_pos = -1;
}

/// Cycle to the next owned weapon with ammunition in the requested direction.
/// The order is the Atomic weapon-slot order, wrapping across knee and remote.
pub fn cycle_weapon(p: &mut Player, direction: i32) -> bool {
    if direction == 0 {
        return false;
    }
    let step = direction.signum();
    let mut slot = i32::from(p.curr_weapon.clamp(0, 11));
    for _ in 0..p.gotweapon.len() {
        slot = (slot + step).rem_euclid(p.gotweapon.len() as i32);
        if select_weapon(p, slot as i16) {
            return true;
        }
    }
    false
}

/// Switch away from an exhausted weapon using this player's Atomic preference
/// order. Returns whether a switch occurred.
pub fn checkavailweapon(p: &mut Player) -> bool {
    if apply_pending_weapon(p) {
        return true;
    }
    let current = p.curr_weapon;
    if current == KNEE_WEAPON || p.weapon_ammo(current) > 0 {
        return false;
    }

    let next = p.weapon_choices.into_iter().find_map(|choice| {
        let weapon = if choice == 0 {
            9
        } else {
            i16::from(choice) - 1
        };
        (weapon == KNEE_WEAPON
            || (p.gotweapon.get(weapon as usize).copied().unwrap_or(false)
                && p.ammo.get(weapon as usize).copied().unwrap_or(0) > 0))
            .then_some(weapon)
    });
    let Some(next) = next else {
        return false;
    };
    p.last_weapon = current;
    p.curr_weapon = next;
    p.kickback_pic = 0;
    p.weapon_pos = -1;
    true
}

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
    let Some(max) = p.weapon_max_ammo(weapon) else {
        return;
    };
    if let Some(ammo) = p.weapon_ammo_mut(weapon) {
        *ammo = ammo.saturating_add(amount).min(max);
    }
}

/// Chocolate `addweapon` (demo path always autoswitches).
pub fn addweapon(p: &mut Player, weapon: i16) {
    if p.weapon_max_ammo(weapon).is_none() {
        return;
    }
    if !p.has_weapon(weapon) {
        p.set_has_weapon(weapon, true);
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
    let Some(max) = p.weapon_max_ammo(weapon) else {
        return true;
    };
    let full = p.weapon_ammo(weapon) >= max;
    if is_addweapon {
        if !p.has_weapon(weapon) {
            addweapon(p, weapon);
        } else if full {
            return false;
        }
    } else if full {
        return false;
    }
    addammo(p, weapon, ammo);
    if p.curr_weapon == KNEE_WEAPON && p.has_weapon(weapon) {
        addweapon(p, weapon);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{cycle_weapon, select_weapon, weapon_slot_from_hotkey};
    use crate::player::Player;

    #[test]
    fn duke_hotkeys_select_pistol_through_remote_slots() {
        assert_eq!(weapon_slot_from_hotkey(1), Some(1));
        assert_eq!(weapon_slot_from_hotkey(9), Some(9));
        assert_eq!(weapon_slot_from_hotkey(10), Some(10));
        assert_eq!(weapon_slot_from_hotkey(0), None);
        assert_eq!(weapon_slot_from_hotkey(11), None);
    }

    #[test]
    fn manual_selection_requires_ownership_and_ammo() {
        let mut player = Player::default();
        player.gotweapon[2] = true;
        player.ammo[2] = 3;
        assert!(select_weapon(&mut player, 2));
        assert_eq!(player.curr_weapon, 2);
        player.ammo[3] = 1;
        assert!(!select_weapon(&mut player, 3));
        assert_eq!(player.curr_weapon, 2);
    }

    #[test]
    fn cycle_skips_unowned_weapons_and_wraps() {
        let mut player = Player::default();
        player.gotweapon[2] = true;
        player.ammo[2] = 1;
        assert!(cycle_weapon(&mut player, 1));
        assert_eq!(player.curr_weapon, 2);
        assert!(cycle_weapon(&mut player, -1));
        assert_eq!(player.curr_weapon, 1);
    }
}
