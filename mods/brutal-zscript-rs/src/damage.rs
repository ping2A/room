//! `Script_DamageMult` and `Damage_Control` (`zscript/DamageHandler.zc`):
//! Brutal Doom's top skill multiplies the damage players and monsters deal
//! to each other.
//!
//! The handler gives every player and every monster one `Damage_Control`
//! token; the token's `ModifyDamage` scales hits its carrier *takes*.

use crate::host::Host;
use room_script_sdk::game::Actor;

/// Name of the token class this mod registers.
pub const DAMAGE_CONTROL: &str = "Damage_Control";

/// `Skill < 6` turns the whole feature off.
pub const MIN_SKILL: i32 = 6;

/// `Script_DamageMult.PlayerEntered`.
pub fn player_entered(host: &mut dyn Host, player: i32) {
    // `GameState != GS_LEVEL`: Room only runs play events inside a level.
    if host.skill() < MIN_SKILL {
        return;
    }
    if let Some(plr) = host.player(player) {
        host.give(plr, DAMAGE_CONTROL, 1);
    }
}

/// `Script_DamageMult.WorldThingSpawned`.
pub fn thing_spawned(host: &mut dyn Host, thing: Actor) {
    if host.skill() < MIN_SKILL {
        return;
    }
    if host.is_monster(thing) {
        host.give(thing, DAMAGE_CONTROL, 1);
    }
}

/// One `ModifyDamage` call (the arguments GZDoom passes).
#[derive(Clone, Copy, Debug)]
pub struct Hit<'a> {
    /// Incoming damage.
    pub damage: i32,
    /// `damageType`.
    pub damage_type: &'a str,
    /// True when the carrier is the one being hurt.
    pub passive: bool,
    /// Projectile / puff.
    pub inflictor: Option<Actor>,
    /// Attacker.
    pub source: Option<Actor>,
}

/// `Damage_Control.ModifyDamage`: the new damage (`newDamage` starts as
/// `damage`).
pub fn modify_damage(host: &dyn Host, hit: &Hit<'_>) -> i32 {
    let damage = hit.damage;
    // Hard early-outs.
    if !hit.passive || hit.inflictor.is_none() {
        return damage;
    }
    let Some(source) = hit.source else {
        return damage;
    };
    // Only scale player / monster damage.
    if !(host.is_monster(source) || host.is_player_pawn(source)) {
        return damage;
    }
    // `Name` comparisons are case-insensitive; ZScript `int` wraps.
    let mut new_damage = if hit.damage_type.eq_ignore_ascii_case("Burn") {
        damage.wrapping_mul(3)
    } else {
        damage.wrapping_mul(7)
    };
    if host.class_name_is(source, "TehLostSoul") {
        new_damage = damage.wrapping_mul(2);
    }
    new_damage
}
