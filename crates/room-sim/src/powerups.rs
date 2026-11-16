//! Generic `PowerupGiver` powerups (UZDoom `inventory/powerups.zs`).
//!
//! Room's Doom pickups (BlurSphere, InvulnerabilitySphere, RadSuit, …) keep
//! their dedicated `Player` fields. A `PowerupGiver` whose `Powerup.Type`
//! names any `Power*` class lands here: the power's duration, `Strength`,
//! `Mode`, colour and the type-specific effect are applied, ticked and undone
//! when it runs out.

use crate::actor::ActorId;
use crate::class::RenderStyle;
use crate::player::Player;
use crate::world::World;
use room_core::ClassId;

/// One running powerup.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PowerState {
    /// Base power kind (`invulnerable`, `invisibility`, `speed`, …).
    pub kind: String,
    /// Tics left (`i32::MAX` = until the level ends).
    pub tics: i32,
    /// `Powerup.Strength`.
    pub strength: f64,
    /// `Powerup.Mode` (lowercase).
    pub mode: String,
    /// `PowerReflection.ReflectType`: reflected damage keeps its type.
    pub reflect_type: bool,
    /// `Powerup.Color` as written (screen blend).
    pub color: String,
    /// Weapon swapped in by `PowerWeaponLevel2` (to swap back at the end).
    pub swapped_weapon: Option<ClassId>,
    /// Owner's player number (`PowerTimeFreezer` freeze mask).
    pub seat: u8,
}

/// Map a `Power*` class (or its ancestors) to the base kind it behaves as.
fn base_kind(world: &World, power: ClassId) -> Option<&'static str> {
    const KINDS: &[(&str, &str)] = &[
        ("PowerInvulnerable", "invulnerable"),
        ("PowerGhost", "invisibility"),
        ("PowerShadow", "invisibility"),
        ("PowerInvisibility", "invisibility"),
        ("PowerMask", "ironfeet"),
        ("PowerIronFeet", "ironfeet"),
        ("PowerTorch", "lightamp"),
        ("PowerLightAmp", "lightamp"),
        ("PowerStrength", "strength"),
        ("PowerFlight", "flight"),
        ("PowerSpeed", "speed"),
        ("PowerDamage", "damage"),
        ("PowerProtection", "protection"),
        ("PowerDrain", "drain"),
        ("PowerRegeneration", "regeneration"),
        ("PowerHighJump", "highjump"),
        ("PowerDoubleFiringSpeed", "doublefiringspeed"),
        ("PowerInfiniteAmmo", "infiniteammo"),
        ("PowerBuddha", "buddha"),
        ("PowerFrightener", "frightener"),
        ("PowerReflection", "reflection"),
        ("PowerTimeFreezer", "timefreezer"),
        ("PowerTargeter", "targeter"),
        ("PowerScanner", "scanner"),
        ("PowerWeaponLevel2", "weaponlevel2"),
        ("PowerMinotaur", "minotaur"),
    ];
    KINDS
        .iter()
        .find(|(class, _)| world.classes.inherits_from(power, class))
        .map(|(_, kind)| *kind)
}

/// GZDoom defaults per kind: (duration in tics, strength, mode).
fn kind_defaults(kind: &str) -> (i32, f64, &'static str) {
    match kind {
        "invulnerable" => (30 * 35, 0.0, ""),
        "invisibility" => (60 * 35, 80.0, "fuzzy"),
        "ironfeet" => (60 * 35, 0.0, ""),
        "lightamp" => (120 * 35, 0.0, ""),
        "strength" => (1, 0.0, ""),
        "flight" => (60 * 35, 0.0, ""),
        "speed" => (45 * 35, 0.0, ""),
        "damage" | "protection" => (25 * 35, 0.0, ""),
        "drain" => (60 * 35, 0.5, ""),
        "regeneration" => (120 * 35, 5.0, ""),
        "highjump" => (30 * 35, 2.0, ""),
        "doublefiringspeed" => (40 * 35, 0.0, ""),
        "infiniteammo" => (30 * 35, 0.0, ""),
        "reflection" => (60 * 35, 0.0, ""),
        "timefreezer" => (12 * 35, 0.0, ""),
        "targeter" => (160 * 35, 0.0, ""),
        "scanner" => (80 * 35, 0.0, ""),
        "weaponlevel2" => (40 * 35, 0.0, ""),
        _ => (30 * 35, 0.0, ""),
    }
}

/// `Powerup.Duration`: negative values are seconds.
fn duration_tics(v: f64) -> i32 {
    if v < 0.0 {
        (-v * 35.0) as i32
    } else {
        v as i32
    }
}

/// Look a property up on the giver first, then the power class.
fn prop<'a>(world: &'a World, giver: ClassId, power: ClassId, name: &str) -> Option<&'a str> {
    world
        .classes
        .get(giver)
        .and_then(|c| c.ext.stored(name))
        .or_else(|| world.classes.get(power).and_then(|c| c.ext.stored(name)))
}

fn player_slot(world: &World, mo: ActorId) -> Option<usize> {
    world
        .players
        .iter()
        .position(|p| p.as_ref().is_some_and(|p| p.mo == Some(mo)))
}

/// UZDoom `PlayerPawn.GetEffectTicsForItem`: the tics left on the power
/// `giver` hands out and its full duration, `None` when it is not running.
/// SBARINFO `PowerupTime` reads this.
#[must_use]
pub fn effect_tics_for_item(world: &World, mo: ActorId, giver: ClassId) -> Option<(i32, i32)> {
    let ty = world.classes.get(giver).map(|c| c.powerup_type.clone())?;
    if ty.is_empty() {
        return None;
    }
    let power = world
        .classes
        .find_name(&ty)
        .filter(|&c| world.classes.inherits_from(c, "Powerup"))
        .or_else(|| world.classes.find_name(&format!("Power{ty}")))?;
    let kind = base_kind(world, power)?;
    let (dur, _, _) = kind_defaults(kind);
    let max = prop(world, giver, power, "powerup.duration")
        .and_then(|v| v.split(',').next()?.trim().parse::<f64>().ok())
        .map_or(dur, duration_tics);
    let player = world.players.get(player_slot(world, mo)?)?.as_ref()?;
    let key = world
        .classes
        .get(power)
        .map(|c| c.name.to_ascii_lowercase())?;
    if let Some(st) = player.powers.get(&key) {
        return Some((st.tics, max));
    }
    // Doom's own spheres keep their time on the player.
    let legacy = match kind {
        "invulnerable" => player.invul_tics,
        "invisibility" => player.blur_tics,
        "ironfeet" => player.radsuit_tics,
        "lightamp" => player.infrared_tics,
        _ => 0,
    };
    (legacy > 0).then_some((legacy, max))
}

/// Give the powerup a `PowerupGiver` names to `receiver`'s player.
/// Returns false when `giver` names no known power or `receiver` is no player.
pub fn give_power(world: &mut World, receiver: ActorId, giver: ClassId) -> bool {
    let Some(ty) = world.classes.get(giver).map(|c| c.powerup_type.clone()) else {
        return false;
    };
    if ty.is_empty() {
        return false;
    }
    let power = world
        .classes
        .find_name(&ty)
        .filter(|&c| world.classes.inherits_from(c, "Powerup"))
        .or_else(|| world.classes.find_name(&format!("Power{ty}")));
    let Some(power) = power else {
        return false;
    };
    let Some(kind) = base_kind(world, power) else {
        return false;
    };
    let Some(slot) = player_slot(world, receiver) else {
        return false;
    };
    let (dur, strength, mode) = kind_defaults(kind);
    let dur = prop(world, giver, power, "powerup.duration")
        .and_then(|v| v.split(',').next()?.trim().parse::<f64>().ok())
        .map_or(dur, duration_tics);
    let strength = prop(world, giver, power, "powerup.strength")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(strength);
    let mode = prop(world, giver, power, "powerup.mode")
        .map_or_else(|| mode.to_owned(), |m| m.trim().to_ascii_lowercase());
    let reflect_type = prop(world, giver, power, "powerreflection.reflecttype")
        .is_some_and(|v| v.trim() == "1" || v.trim().eq_ignore_ascii_case("true"));
    let color = prop(world, giver, power, "powerup.color")
        .unwrap_or("")
        .to_owned();
    let key = world
        .classes
        .get(power)
        .map(|c| c.name.to_ascii_lowercase())
        .unwrap_or_default();
    let mut state = PowerState {
        kind: kind.to_owned(),
        tics: dur.max(1),
        strength,
        mode,
        reflect_type,
        color,
        swapped_weapon: None,
        seat: u8::try_from(slot).unwrap_or(0),
    };
    let Some(mut player) = world.players[slot].take() else {
        return false;
    };
    // Picking up a power already running only refreshes its time (GZDoom
    // `Powerup.HandlePickup`).
    if let Some(old) = player.powers.get_mut(&key) {
        old.tics = old.tics.max(state.tics);
        world.players[slot] = Some(player);
        return true;
    }
    init_effect(world, &mut player, receiver, &mut state);
    player.powers.insert(key, state);
    world.players[slot] = Some(player);
    true
}

/// Invulnerability mode of the player's class (`Player.InvulnerabilityMode`).
fn invul_mode(world: &World, mo: ActorId) -> String {
    world
        .actors
        .get(mo)
        .and_then(|a| world.classes.get(a.class))
        .and_then(|c| c.ext.stored("player.invulnerabilitymode"))
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn init_effect(world: &mut World, p: &mut Player, mo: ActorId, st: &mut PowerState) {
    match st.kind.as_str() {
        "invulnerable" => {
            p.invulnerable = true;
            p.invul_tics = p.invul_tics.max(st.tics);
            // `Player.InvulnerabilityMode`: Hexen's cleric reflects missiles,
            // the mage turns into a ghost.
            match invul_mode(world, mo).as_str() {
                "reflective" => set_flag(world, mo, "REFLECTIVE", true),
                "ghost" => {
                    set_flag(world, mo, "SHADOW", true);
                    set_flag(world, mo, "GHOST", true);
                }
                _ => {}
            }
        }
        "invisibility" => {
            set_flag(world, mo, "SHADOW", true);
            apply_invisibility_style(world, mo, st);
        }
        "ironfeet" => {
            p.radsuit = true;
            p.radsuit_tics = p.radsuit_tics.max(st.tics);
        }
        "lightamp" => {
            p.infrared = true;
            p.infrared_tics = p.infrared_tics.max(st.tics);
        }
        "strength" => p.berserk = true,
        "flight" => {
            p.fly_tics = p.fly_tics.max(st.tics);
            if let Some(a) = world.actors.get_mut(mo) {
                a.flags |= crate::class::FLAG_NOGRAVITY;
            }
        }
        "buddha" => {
            if let Some(a) = world.actors.get_mut(mo) {
                a.flags |= crate::class::FLAG_BUDDHA;
            }
        }
        "frightener" => set_flag(world, mo, "FRIGHTENING", true),
        "timefreezer" => {
            // UZDoom `PowerTimeFreezer.InitEffect`: the owner and teammates may
            // move; the effect starts and ends on an even tic.
            let mask = crate::player::seat_bit(usize::from(st.seat));
            p.timefreezer |= mask;
            let team = p.team;
            let dm = world.deathmatch;
            for other in world.players.iter_mut().flatten() {
                if !dm || (team != 255 && other.team == team) {
                    other.timefreezer |= mask;
                }
            }
            if st.tics & 1 == 0 && st.tics != i32::MAX {
                st.tics += 1;
            }
            if world.level.maptime & 1 == 0 {
                world.level.frozen_power = true;
            } else if st.tics < i32::MAX {
                st.tics += 1;
            }
        }
        "weaponlevel2" => {
            // Swap the gun in hand for its powered sister (Tome of Power).
            if let Some(sister) = p.ready_weapon.and_then(|w| sister_weapon(world, w)) {
                st.swapped_weapon = p.ready_weapon;
                p.ready_weapon = Some(sister);
                p.pspr_state = None;
            }
        }
        _ => {}
    }
}

fn end_effect(world: &mut World, p: &mut Player, mo: ActorId, st: &PowerState) {
    match st.kind.as_str() {
        "invulnerable" => {
            set_flag(world, mo, "REFLECTIVE", false);
            set_flag(world, mo, "GHOST", false);
        }
        "invisibility" => {
            if !p.blur {
                set_flag(world, mo, "SHADOW", false);
                if let Some(a) = world.actors.get_mut(mo) {
                    a.alpha = 1.0;
                    a.render_style = RenderStyle::Normal;
                }
            }
        }
        "buddha" => {
            if let Some(a) = world.actors.get_mut(mo) {
                a.flags &= !crate::class::FLAG_BUDDHA;
            }
        }
        "frightener" => set_flag(world, mo, "FRIGHTENING", false),
        "timefreezer" => {
            let mask = !crate::player::seat_bit(usize::from(st.seat));
            p.timefreezer &= mask;
            for other in world.players.iter_mut().flatten() {
                other.timefreezer &= mask;
            }
            let any =
                p.timefreezer != 0 || world.players.iter().flatten().any(|o| o.timefreezer != 0);
            if !any {
                world.level.frozen_power = false;
            }
        }
        "weaponlevel2" => {
            if let Some(orig) = st.swapped_weapon {
                if p.ready_weapon
                    .is_some_and(|w| sister_weapon(world, orig) == Some(w))
                {
                    p.ready_weapon = Some(orig);
                    p.pspr_state = None;
                }
            }
        }
        _ => {}
    }
}

/// `Weapon.SisterWeapon` of `weapon` when that sister is `+WEAPON.POWERED_UP`.
#[must_use]
pub fn sister_weapon(world: &World, weapon: ClassId) -> Option<ClassId> {
    let name = world.classes.get(weapon)?.ext.sister_weapon.clone();
    let sister = world.classes.find_name(&name)?;
    world
        .classes
        .get(sister)?
        .ext
        .has_qualified_flag("WEAPON.POWERED_UP")
        .then_some(sister)
}

fn set_flag(world: &mut World, mo: ActorId, name: &str, on: bool) {
    if let Some(a) = world.actors.get_mut(mo) {
        a.extra_flags.set_name(name, on);
        if let Some(bit) = crate::class::legacy_flag_bit(name) {
            if on {
                a.flags |= bit;
            } else {
                a.flags &= !bit;
            }
        }
    }
}

/// `PowerInvisibility.DoEffect`: `Strength` (percent) is how see-through the
/// owner gets; `Mode` picks the style (`Fuzzy`, `Opaque`, `Additive`,
/// `Stencil`, `Translucent`, `Cumulative`, `None`).
fn apply_invisibility_style(world: &mut World, mo: ActorId, st: &PowerState) {
    let ts = (st.strength / 100.0).clamp(0.0, 1.0);
    let alpha = (1.0 - ts).clamp(0.0, 1.0);
    let style = match st.mode.as_str() {
        "fuzzy" => RenderStyle::OptFuzzy,
        "opaque" => RenderStyle::Normal,
        "additive" => RenderStyle::Add,
        "stencil" => RenderStyle::Stencil,
        "translucentstencil" => RenderStyle::TranslucentStencil,
        "addstencil" => RenderStyle::AddStencil,
        "none" | "cumulative" | "translucent" | "" => RenderStyle::Translucent,
        _ => RenderStyle::Translucent,
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.render_style = style;
        a.alpha = if matches!(style, RenderStyle::Normal) {
            1.0
        } else {
            alpha
        };
    }
}

/// Count running powers down and apply per-tic effects (`DoEffect`).
pub(crate) fn tick_powers(world: &mut World, p: &mut Player) {
    if p.powers.is_empty() {
        return;
    }
    let Some(mo) = p.mo else {
        return;
    };
    let now = world.level.maptime;
    let keys: Vec<String> = p.powers.keys().cloned().collect();
    for key in keys {
        let Some(mut st) = p.powers.get(&key).cloned() else {
            continue;
        };
        if st.kind == "regeneration" && now & 31 == 0 {
            let max = world
                .actors
                .get(mo)
                .and_then(|a| world.classes.get(a.class))
                .map_or(100, |c| {
                    if c.ext.player_max_health > 0 {
                        c.ext.player_max_health
                    } else {
                        100
                    }
                });
            let healed = world.actors.get_mut(mo).is_some_and(|a| {
                if a.health > 0 && a.health < max {
                    a.health = (a.health + st.strength as i32).min(max);
                    true
                } else {
                    false
                }
            });
            if healed {
                if let Some(a) = world.actors.get(mo) {
                    world.play_sound("*regenerate", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
                }
            }
        }
        if st.kind == "invisibility" {
            apply_invisibility_style(world, mo, &st);
        }
        if st.kind == "timefreezer" && now & 1 == 0 {
            world.level.frozen_power = time_freezer_frozen(st.tics);
        }
        if st.tics != i32::MAX {
            st.tics -= 1;
        }
        if st.tics <= 0 {
            p.powers.remove(&key);
            end_effect(world, p, mo, &st);
        } else {
            p.powers.insert(key, st);
        }
    }
}

/// `PowerTimeFreezer.DoEffect`: frozen, blinking faster in the last 4 s.
#[must_use]
pub fn time_freezer_frozen(t: i32) -> bool {
    t > 4 * 32
        || (t > 3 * 32 && t <= 4 * 32 && (t + 1) & 15 != 0)
        || (t > 2 * 32 && t <= 3 * 32 && (t + 1) & 7 != 0)
        || (t > 32 && t <= 2 * 32 && (t + 1) & 3 != 0)
        || (t > 0 && t <= 32 && (t + 1) & 1 != 0)
}

/// UZDoom `AActor::isFrozen` (level freeze part): under a freeze, actors stop
/// unless `+NOTIMEFREEZE`; players stop only for the time freezer, and only
/// when no freezer bit covers them.
#[must_use]
pub fn actor_time_frozen(world: &World, id: ActorId) -> bool {
    if !world.level.is_frozen() {
        return false;
    }
    let Some(a) = world.actors.get(id) else {
        return false;
    };
    if a.extra_flags.get_name("NOTIMEFREEZE") {
        return false;
    }
    match world.players.iter().flatten().find(|p| p.mo == Some(id)) {
        Some(p) => world.level.frozen_power && p.timefreezer == 0,
        None => true,
    }
}

/// Targeter HUD (`PowerTargeter`): which reticle pieces show and where, in
/// 320×200 psprite coordinates (top-left of each piece before offsets).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TargeterHud {
    /// Centre piece position (`TRGTA0`).
    pub center: (f32, f32),
    /// Left piece x (`TRGTB0`), when shown.
    pub left_x: Option<f32>,
    /// Right piece x (`TRGTC0`), when shown.
    pub right_x: Option<f32>,
}

/// `PowerTargeter.PositionAccuracy` / `DoEffect` for `mo`: the side pieces
/// sit `100 − accuracy` away from the centre and blink in the last 5 s.
#[must_use]
pub fn targeter_hud(world: &World, mo: ActorId) -> Option<TargeterHud> {
    const POS_X: f32 = 160.0 - 3.0;
    const POS_Y: f32 = 100.0 - 3.0;
    let st = power(world, mo, "targeter")?;
    let accuracy = world.actors.get(mo).map_or(0, |a| a.accuracy);
    let spread = (100 - accuracy) as f32;
    let (mut left, mut right) = (true, true);
    // Last 5 s: bit 5 shows only the left piece, bit 4 only the right; other
    // tics keep whatever the previous (higher) tic showed.
    if st.tics < 5 * 35 {
        for u in st.tics..5 * 35 {
            if u & 32 != 0 {
                (left, right) = (true, false);
                break;
            }
            if u & 16 != 0 {
                (left, right) = (false, true);
                break;
            }
        }
    }
    Some(TargeterHud {
        center: (POS_X, POS_Y),
        left_x: left.then_some(POS_X - spread),
        right_x: right.then_some(POS_X + spread),
    })
}

/// Whether `mo`'s automap shows the whole level (`allmap`, or `PowerScanner`
/// while it runs, as Strife's scanner).
#[must_use]
pub fn reveals_map(world: &World, mo: ActorId) -> bool {
    power(world, mo, "scanner").is_some()
}

/// Strength of a running power of `kind` on `mo`'s player.
#[must_use]
pub fn power(world: &World, mo: ActorId, kind: &str) -> Option<PowerState> {
    if let Some((tmo, powers)) = world.thinking_powers.as_ref() {
        if *tmo == mo {
            return powers.iter().find(|s| s.kind == kind).cloned();
        }
    }
    world
        .players
        .iter()
        .flatten()
        .find(|p| p.mo == Some(mo))?
        .powers
        .values()
        .find(|s| s.kind == kind)
        .cloned()
}

/// `ModifyDamage` of the target's and the source's powers.
///
/// `PowerProtection` takes a quarter, `PowerReflection` halves it, and
/// `PowerDamage` on the source deals four times.
#[must_use]
pub(crate) fn modify_damage(
    world: &World,
    target: ActorId,
    source: Option<ActorId>,
    damage: i32,
) -> i32 {
    if damage <= 0 || damage >= 1_000_000 {
        return damage;
    }
    let mut d = damage;
    if let Some(src) = source.filter(|&s| s != target) {
        if power(world, src, "damage").is_some() {
            d = (d * 4).max(1);
        }
    }
    if power(world, target, "protection").is_some() {
        d = (d / 4).max(0);
    }
    if power(world, target, "reflection").is_some() {
        d = (d / 2).max(0);
    }
    d
}

/// After a hit: `PowerReflection` returns the damage to the attacker and
/// `PowerDrain` heals the attacker by `Strength` × the damage dealt.
pub(crate) fn after_damage(
    world: &mut World,
    target: ActorId,
    source: Option<ActorId>,
    dealt: i32,
    dtype: Option<&str>,
) {
    let Some(src) = source.filter(|&s| s != target) else {
        return;
    };
    if dealt <= 0 {
        return;
    }
    if let Some(r) = power(world, target, "reflection").filter(|_| !world.reflecting) {
        // `PowerReflection.ReflectType 1` keeps the incoming damage type.
        if r.reflect_type {
            world.forced_damage_type = dtype.map(str::to_owned);
        }
        world.reflecting = true;
        crate::damage::damage_mobj(world, src, Some(target), Some(target), dealt);
        world.reflecting = false;
        world.forced_damage_type = None;
    }
    if let Some(drain) = power(world, src, "drain") {
        let heal = (f64::from(dealt) * drain.strength) as i32;
        if heal > 0 {
            if let Some(a) = world.actors.get_mut(src) {
                if a.health > 0 {
                    a.health = (a.health + heal).min(100.max(a.health));
                }
            }
        }
    }
}

/// `PowerSpeed` move factor (1.5) and `PowerHighJump` jump factor (`Strength`).
#[must_use]
pub fn speed_factor(world: &World, mo: ActorId) -> f64 {
    if power(world, mo, "speed").is_some() {
        1.5
    } else {
        1.0
    }
}

/// `PowerHighJump` multiplier on `JumpZ`.
#[must_use]
pub fn jump_factor(world: &World, mo: ActorId) -> f64 {
    power(world, mo, "highjump").map_or(1.0, |s| if s.strength > 0.0 { s.strength } else { 2.0 })
}

/// `PowerInfiniteAmmo`: weapons use no ammo.
#[must_use]
pub fn infinite_ammo(world: &World, mo: ActorId) -> bool {
    power(world, mo, "infiniteammo").is_some()
}

/// True when `giver` is a `PowerupGiver` this module handles (not a morph).
#[must_use]
pub fn is_generic_giver(world: &World, giver: ClassId) -> bool {
    let Some(def) = world.classes.get(giver) else {
        return false;
    };
    if def.powerup_type.is_empty() || !world.classes.inherits_from(giver, "PowerupGiver") {
        return false;
    }
    let ty = &def.powerup_type;
    world
        .classes
        .find_name(ty)
        .filter(|&c| world.classes.inherits_from(c, "Powerup"))
        .or_else(|| world.classes.find_name(&format!("Power{ty}")))
        .is_some_and(|p| base_kind(world, p).is_some())
}

/// Extra full-screen blend for player `seat`: a running power's
/// `Powerup.Color` (with its alpha, default 1/3), or the class
/// `Player.DamageScreenColor` while hurt. The bool is true when that colour
/// replaces Doom's red damage palette.
#[must_use]
pub fn screen_blend(world: &World, seat: usize) -> Option<([f32; 4], bool)> {
    let p = world.players.get(seat)?.as_ref()?;
    let mo = p.mo?;
    let class = world
        .actors
        .get(mo)
        .and_then(|a| world.classes.get(a.class));
    if p.damage_count > 0 {
        if let Some(c) = class.filter(|c| !c.ext.damage_screen_color.is_empty()) {
            let mut f = c.ext.damage_screen_color.split(',');
            let rgb = f.next().and_then(crate::action::parse_blend_color)?;
            let intensity = f
                .next()
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(1.0);
            #[allow(clippy::cast_precision_loss)]
            let a = (p.damage_count.min(100) as f32 / 100.0 * 0.8 * intensity).clamp(0.0, 1.0);
            return Some(([rgb.0, rgb.1, rgb.2, a], true));
        }
    }
    for st in p.powers.values() {
        let mut f = st.color.split(',');
        let Some(col) = f
            .next()
            .map(str::trim)
            .filter(|c| !c.is_empty() && !c.eq_ignore_ascii_case("none"))
        else {
            continue;
        };
        let Some(rgb) = crate::action::parse_blend_color(col) else {
            continue;
        };
        let a = f
            .next()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(1.0 / 3.0);
        // Blink out over the last second, like the Doom powerup palettes.
        if st.tics < 4 * 32 && st.tics & 8 == 0 {
            continue;
        }
        return Some(([rgb.0, rgb.1, rgb.2, a.clamp(0.0, 1.0)], false));
    }
    None
}
