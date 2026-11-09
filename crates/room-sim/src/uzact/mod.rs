//! Native ports of UZDoom action functions that Room did not have (`docs/36`).
//!
//! UZDoom writes most of these in ZScript (`wadsrc/static/zscript`). Room's
//! VM natives cannot reach the world synchronously, so the bodies are ported
//! to Rust here, one module per family. The helpers below mirror the ZScript
//! engine calls the bodies use (`Spawn`, `SpawnMissile`, `LineAttack`, …) so
//! each port stays close to the original line by line.
//!
//! A port that needs a class (`Feather`, `KnightAxe`, …) does nothing when the
//! class is not defined, as UZDoom does when `Spawn` gets a null class.

#![allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use crate::action::{ActionResult, call_action_args};
use crate::actor::ActorId;
use crate::world::World;
use room_core::{Angle, DVec3, StateId, dmath};

mod generic;
mod heretic;
mod hexen;
mod strife;

/// `MISSILERANGE` (32 × 64).
pub(crate) const MISSILE_RANGE: f64 = 2048.0;
/// `PLAYERMISSILERANGE`.
pub(crate) const PLAYER_MISSILE_RANGE: f64 = 8192.0;
/// `MELEERANGE`.
pub(crate) const MELEE_RANGE: f64 = 64.0;

/// Run `name` if it is one of the ported actions.
pub(crate) fn dispatch(
    world: &mut World,
    id: ActorId,
    name: &str,
    args: &[String],
) -> Option<ActionResult> {
    generic::dispatch(world, id, name, args)
        .or_else(|| heretic::dispatch(world, id, name, args))
        .or_else(|| hexen::dispatch(world, id, name, args))
        .or_else(|| strife::dispatch(world, id, name, args))
}

/// Every action name this module implements (for `known_action_names`).
#[must_use]
pub fn ported_action_names() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    v.extend_from_slice(generic::NAMES);
    v.extend_from_slice(heretic::NAMES);
    v.extend_from_slice(hexen::NAMES);
    v.extend_from_slice(strife::NAMES);
    v
}

/// Canonical spelling of a ported action (case-insensitive).
#[must_use]
pub fn canonical(name: &str) -> Option<&'static str> {
    [generic::NAMES, heretic::NAMES, hexen::NAMES, strife::NAMES]
        .iter()
        .flat_map(|n| n.iter())
        .copied()
        .find(|n| n.eq_ignore_ascii_case(name))
}

// ---------------------------------------------------------------------------
// ZScript-like helpers
// ---------------------------------------------------------------------------

/// `random[name]()`: 0..=255.
pub(crate) fn rnd(w: &mut World, name: &str) -> i32 {
    i32::from(w.p_random(name))
}

/// `random[name](lo, hi)`.
pub(crate) fn rnd_range(w: &mut World, name: &str, lo: i32, hi: i32) -> i32 {
    if hi <= lo {
        return lo;
    }
    lo + rnd(w, name) % (hi - lo + 1)
}

/// `random2[name]()`: difference of two rolls (−255..=255).
pub(crate) fn rnd2(w: &mut World, name: &str) -> i32 {
    let a = rnd(w, name);
    let b = rnd(w, name);
    a - b
}

/// `random2[name](mask)`: `(r1 & mask) - (r2 & mask)`.
pub(crate) fn rnd2_mask(w: &mut World, name: &str, mask: i32) -> i32 {
    let a = rnd(w, name) & mask;
    let b = rnd(w, name) & mask;
    a - b
}

pub(crate) fn pos(w: &World, id: ActorId) -> DVec3 {
    w.actors.get(id).map_or(DVec3::ZERO, |a| a.pos)
}

pub(crate) fn vel(w: &World, id: ActorId) -> DVec3 {
    w.actors.get(id).map_or(DVec3::ZERO, |a| a.vel)
}

pub(crate) fn set_vel(w: &mut World, id: ActorId, v: DVec3) {
    if let Some(a) = w.actors.get_mut(id) {
        a.vel = v;
    }
}

pub(crate) fn set_vel_z(w: &mut World, id: ActorId, z: f64) {
    if let Some(a) = w.actors.get_mut(id) {
        a.vel = DVec3::new(a.vel.x(), a.vel.y(), z);
    }
}

pub(crate) fn angle(w: &World, id: ActorId) -> f64 {
    w.actors.get(id).map_or(0.0, |a| a.angle.degrees())
}

pub(crate) fn set_angle(w: &mut World, id: ActorId, deg: f64) {
    if let Some(a) = w.actors.get_mut(id) {
        a.angle = Angle::from_degrees(deg);
    }
}

pub(crate) fn target(w: &World, id: ActorId) -> Option<ActorId> {
    w.actors
        .get(id)
        .and_then(|a| a.target)
        .filter(|t| w.actors.get(*t).is_some_and(|a| !a.destroyed))
}

pub(crate) fn tracer(w: &World, id: ActorId) -> Option<ActorId> {
    w.actors
        .get(id)
        .and_then(|a| a.tracer)
        .filter(|t| w.actors.get(*t).is_some_and(|a| !a.destroyed))
}

pub(crate) fn master(w: &World, id: ActorId) -> Option<ActorId> {
    w.actors
        .get(id)
        .and_then(|a| a.master)
        .filter(|t| w.actors.get(*t).is_some_and(|a| !a.destroyed))
}

pub(crate) fn health(w: &World, id: ActorId) -> i32 {
    w.actors.get(id).map_or(0, |a| a.health)
}

pub(crate) fn is_player(w: &World, id: ActorId) -> bool {
    w.players.iter().flatten().any(|p| p.mo == Some(id))
}

pub(crate) fn class_name(w: &World, id: ActorId) -> String {
    w.actors
        .get(id)
        .and_then(|a| w.classes.get(a.class))
        .map(|c| c.name.clone())
        .unwrap_or_default()
}

/// `Spawn(class, pos, ALLOW_REPLACE)`.
pub(crate) fn spawn(w: &mut World, class: &str, at: DVec3) -> Option<ActorId> {
    let cid = w.classes.find_name(class)?;
    w.spawn_at(cid, at, Angle::ZERO).ok()
}

/// `Spawn` facing `deg`.
pub(crate) fn spawn_facing(w: &mut World, class: &str, at: DVec3, deg: f64) -> Option<ActorId> {
    let cid = w.classes.find_name(class)?;
    w.spawn_at(cid, at, Angle::from_degrees(deg)).ok()
}

/// `A_StartSound(name)` at the actor.
pub(crate) fn sound(w: &mut World, id: ActorId, name: &str) {
    if name.is_empty() {
        return;
    }
    let p = pos(w, id);
    w.play_sound(name, p.x(), p.y(), p.z(), 1.0);
}

/// The class `SeeSound` / `AttackSound` / `PainSound` / `DeathSound` / `ActiveSound`.
pub(crate) fn class_sound(w: &World, id: ActorId, which: &str) -> String {
    let Some(c) = w.actors.get(id).and_then(|a| w.classes.get(a.class)) else {
        return String::new();
    };
    match which {
        "see" => c.see_sound.clone(),
        "attack" => c.attack_sound.clone(),
        "pain" => c.pain_sound.clone(),
        "death" => c.death_sound.clone(),
        "active" => c.active_sound.clone(),
        _ => String::new(),
    }
}

pub(crate) fn face_target(w: &mut World, id: ActorId) {
    let _ = crate::action::a_face_target(w, id);
}

/// `CheckMeleeRange()` against the target.
pub(crate) fn in_melee_range(w: &World, id: ActorId) -> bool {
    target(w, id).is_some_and(|t| crate::action::check_melee_range(w, id, t))
}

/// `DamageMobj`.
pub(crate) fn damage(
    w: &mut World,
    victim: ActorId,
    inflictor: Option<ActorId>,
    source: Option<ActorId>,
    amount: i32,
) -> i32 {
    if amount <= 0 {
        return 0;
    }
    crate::damage::damage_mobj(w, victim, inflictor, source, amount)
}

/// `Distance2D`.
pub(crate) fn dist2d(w: &World, a: ActorId, b: ActorId) -> f64 {
    let (pa, pb) = (pos(w, a), pos(w, b));
    (pb.x() - pa.x()).hypot(pb.y() - pa.y())
}

/// `AngleTo(other)` in degrees.
pub(crate) fn angle_to(w: &World, from: ActorId, to: ActorId) -> f64 {
    let (pa, pb) = (pos(w, from), pos(w, to));
    Angle::from_radians(dmath::atan2(pb.y() - pa.y(), pb.x() - pa.x())).degrees()
}

/// `deltaangle(a, b)` in −180..180.
pub(crate) fn delta_angle(a: f64, b: f64) -> f64 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

/// `AngleToVector(deg, len)`.
pub(crate) fn angle_vec(deg: f64, len: f64) -> (f64, f64) {
    let r = deg.to_radians();
    (dmath::cos(r) * len, dmath::sin(r) * len)
}

/// `Thrust(angle, speed)`.
pub(crate) fn thrust(w: &mut World, id: ActorId, deg: f64, speed: f64) {
    let (x, y) = angle_vec(deg, speed);
    if let Some(a) = w.actors.get_mut(id) {
        a.vel = DVec3::new(a.vel.x() + x, a.vel.y() + y, a.vel.z());
    }
}

/// `VelFromAngle(speed)` along the actor's yaw.
pub(crate) fn vel_from_angle(w: &mut World, id: ActorId, speed: f64) {
    let (x, y) = angle_vec(angle(w, id), speed);
    if let Some(a) = w.actors.get_mut(id) {
        a.vel = DVec3::new(x, y, a.vel.z());
    }
}

pub(crate) fn check_sight(w: &World, a: ActorId, b: ActorId) -> bool {
    crate::sight::check_sight(w, a, b)
}

/// `SetStateLabel(label)` now; `None` when the class lacks it.
pub(crate) fn set_label(w: &mut World, id: ActorId, label: &str) -> bool {
    let Some(class) = w.actors.get(id).map(|a| a.class) else {
        return false;
    };
    if label.eq_ignore_ascii_case("null") {
        w.set_state(id, None, false);
        return true;
    }
    match w.states.find_label(class, label) {
        Some(s) => {
            w.set_state(id, Some(s), true);
            true
        }
        None => false,
    }
}

/// `SetState(SpawnState + n)` (Hexen picks frame variants this way).
pub(crate) fn set_spawn_state_plus(w: &mut World, id: ActorId, n: u32) {
    let Some(class) = w.actors.get(id).map(|a| a.class) else {
        return;
    };
    if let Some(s) = w.states.find_label(class, "Spawn") {
        let st = StateId::from_raw(s.raw() + n);
        if w.states
            .get(st)
            .is_some_and(|x| x.owner == w.states.get(s).map_or(x.owner, |y| y.owner))
        {
            w.set_state(id, Some(st), true);
        }
    }
}

/// `ResolveState(label)` as an action result.
pub(crate) fn jump(w: &World, id: ActorId, label: &str) -> ActionResult {
    crate::action::jump_to_label(w, id, label)
}

/// Call an existing Room action by name with literal arguments.
pub(crate) fn call(w: &mut World, id: ActorId, name: &str, args: &[&str]) -> ActionResult {
    let owned: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    call_action_args(w, id, name, &owned)
}

/// `LineAttack(angle, range, slope, damage, damagetype, puff)`.
pub(crate) fn line_attack(
    w: &mut World,
    id: ActorId,
    deg: f64,
    range: f64,
    slope: f64,
    dmg: i32,
    puff: &str,
) -> Option<ActorId> {
    let prev = w.puff_override.take();
    if !puff.is_empty() {
        w.puff_override = Some(puff.to_owned());
    }
    let hit =
        crate::hitscan::line_attack_with_slope(w, id, Angle::from_degrees(deg), range, dmg, slope);
    w.puff_override = prev;
    hit.actor
}

/// `AimLineAttack(angle, range)` as a slope (0 when nothing is in sight).
pub(crate) fn aim(w: &World, id: ActorId, deg: f64, range: f64) -> f64 {
    crate::hitscan::aim_line_attack(w, id, Angle::from_degrees(deg), range).unwrap_or(0.0)
}

/// `BulletSlope()` for a player.
pub(crate) fn bullet_slope(w: &World, id: ActorId) -> f64 {
    crate::hitscan::bullet_slope(w, id)
}

/// The missile class's launch speed (`Speed`, `FastSpeed` on fast monsters).
fn missile_speed(w: &World, class: room_core::ClassId) -> f64 {
    w.classes
        .get(class)
        .map_or(10.0, |c| crate::action::class_move_speed(w, c))
        .max(0.0)
}

fn finish_missile(w: &mut World, src: ActorId, mo: ActorId, dest: Option<ActorId>) {
    let seeker = w
        .actors
        .get(mo)
        .is_some_and(|a| a.extra_flags.get_name("SEEKERMISSILE"));
    if let Some(a) = w.actors.get_mut(mo) {
        a.target = Some(src);
        if seeker {
            a.tracer = dest;
        }
    }
    // `P_CheckMissileSpawn`: a missile born inside a wall explodes at once.
    let p = pos(w, mo);
    if !crate::physics::try_move(w, mo, p) {
        crate::physics::explode_missile(w, mo);
    }
}

/// `SpawnMissileXYZ(at, dest, class)`: aimed at `dest` from `at`.
pub(crate) fn spawn_missile_xyz(
    w: &mut World,
    src: ActorId,
    at: DVec3,
    dest: ActorId,
    class: &str,
) -> Option<ActorId> {
    let cid = w.classes.replacement(w.classes.find_name(class)?);
    let d = pos(w, dest);
    let ang = Angle::from_radians(dmath::atan2(d.y() - at.y(), d.x() - at.x())).degrees();
    // Blur / shadow targets throw the aim off (`P_SpawnMissileXYZ`).
    let shadow = w.actors.get(dest).is_some_and(|a| {
        (a.flags & crate::class::FLAG_SHADOW) != 0 || a.extra_flags.get_name("SHADOW")
    });
    let ang = if shadow {
        ang + f64::from(rnd2(w, "pr_spawnmissile")) * (360.0 / 4096.0)
    } else {
        ang
    };
    let mo = w.spawn_at(cid, at, Angle::from_degrees(ang)).ok()?;
    let speed = missile_speed(w, cid);
    let (vx, vy) = angle_vec(ang, speed);
    let dist = (d.x() - at.x()).hypot(d.y() - at.y());
    let steps = (dist / speed.max(1e-6)).max(1.0);
    let dh = w.actors.get(dest).map_or(0.0, |a| a.height);
    let vz = if w
        .actors
        .get(mo)
        .is_some_and(|a| a.gravity > 0.0 && (a.flags & crate::class::FLAG_NOGRAVITY) == 0)
    {
        0.0
    } else {
        (d.z() + dh * 0.5 - at.z()) / steps
    };
    let _ = dh;
    set_vel(w, mo, DVec3::new(vx, vy, vz));
    finish_missile(w, src, mo, Some(dest));
    Some(mo)
}

/// `SpawnMissile(dest, class)` from `z + 32`.
pub(crate) fn spawn_missile(
    w: &mut World,
    src: ActorId,
    dest: ActorId,
    class: &str,
) -> Option<ActorId> {
    let p = pos(w, src);
    spawn_missile_xyz(w, src, DVec3::new(p.x(), p.y(), p.z() + 32.0), dest, class)
}

/// `SpawnMissileZ(z, dest, class)`.
pub(crate) fn spawn_missile_z(
    w: &mut World,
    src: ActorId,
    z: f64,
    dest: ActorId,
    class: &str,
) -> Option<ActorId> {
    let p = pos(w, src);
    spawn_missile_xyz(w, src, DVec3::new(p.x(), p.y(), z), dest, class)
}

/// `SpawnMissileAngleZSpeed(z, class, angle, vz, speed)`.
pub(crate) fn spawn_missile_angle_z_speed(
    w: &mut World,
    src: ActorId,
    z: f64,
    class: &str,
    deg: f64,
    vz: f64,
    speed: Option<f64>,
) -> Option<ActorId> {
    let cid = w.classes.replacement(w.classes.find_name(class)?);
    let p = pos(w, src);
    let mo = w
        .spawn_at(cid, DVec3::new(p.x(), p.y(), z), Angle::from_degrees(deg))
        .ok()?;
    let speed = speed.unwrap_or_else(|| missile_speed(w, cid));
    let (vx, vy) = angle_vec(deg, speed);
    set_vel(w, mo, DVec3::new(vx, vy, vz));
    let dest = target(w, src);
    finish_missile(w, src, mo, dest);
    Some(mo)
}

/// `SpawnMissileAngle(class, angle, vz)` from `z + 32`.
pub(crate) fn spawn_missile_angle(
    w: &mut World,
    src: ActorId,
    class: &str,
    deg: f64,
    vz: f64,
) -> Option<ActorId> {
    let z = pos(w, src).z() + 32.0;
    spawn_missile_angle_z_speed(w, src, z, class, deg, vz, None)
}

/// `SpawnPlayerMissile(class [, angle])`.
pub(crate) fn spawn_player_missile(
    w: &mut World,
    src: ActorId,
    class: &str,
    add_angle: f64,
) -> Option<ActorId> {
    let cid = w.classes.find_name(class)?;
    let speed = missile_speed(w, cid);
    let opts = crate::action::PlayerMissileOpts {
        angle: add_angle,
        ..Default::default()
    };
    crate::action::spawn_player_missile_ex(w, src, cid, speed, opts)
}

/// `A_Explode(damage, radius)` from the actor.
pub(crate) fn explode(w: &mut World, id: ActorId, dmg: i32, radius: f64) {
    let p = pos(w, id);
    crate::action::radius_attack(w, id, p, dmg, radius);
}

/// `DepleteAmmo` for the player's ready weapon (true when it could fire).
pub(crate) fn deplete_ammo(w: &mut World, id: ActorId) -> bool {
    if w.psprite_weapon.is_none() {
        return true;
    }
    crate::action::deplete_ready_ammo(w, id)
}

/// `GiveInventory(class, amount)`.
pub(crate) fn give(w: &mut World, id: ActorId, class: &str, amount: i32) -> bool {
    match w.classes.find_name(class) {
        Some(cid) => crate::inventory::give_inventory(w, id, cid, amount),
        None => false,
    }
}

/// `CountInv(class)`.
pub(crate) fn count_inv(w: &World, id: ActorId, class: &str) -> i32 {
    w.classes
        .find_name(class)
        .map_or(0, |cid| w.inventory.count(id, cid))
}

/// Floor / ceiling heights at the actor.
pub(crate) fn floor_ceil(w: &World, id: ActorId) -> (f64, f64) {
    let Some(a) = w.actors.get(id) else {
        return (0.0, 0.0);
    };
    let c = crate::physics::check_position(w, a, a.pos, Some(id));
    (c.floor_z, c.ceiling_z)
}

/// `Destroy()`.
pub(crate) fn destroy(w: &mut World, id: ActorId) {
    let _ = crate::action::a_remove(w, id);
}

/// Mark the actor `+DROPPED`.
pub(crate) fn set_dropped(w: &mut World, id: ActorId) {
    if let Some(a) = w.actors.get_mut(id) {
        a.flags |= crate::class::FLAG_DROPPED;
    }
}

pub(crate) fn has_flag(w: &World, id: ActorId, name: &str) -> bool {
    w.actors.get(id).is_some_and(|a| {
        a.extra_flags.get_name(name)
            || crate::class::legacy_flag_bit(name).is_some_and(|b| a.flags & b != 0)
    })
}

pub(crate) fn set_flag(w: &mut World, id: ActorId, name: &str, on: bool) {
    let bit = crate::class::legacy_flag_bit(name);
    if let Some(a) = w.actors.get_mut(id) {
        if let Some(b) = bit {
            if on {
                a.flags |= b;
            } else {
                a.flags &= !b;
            }
        } else {
            a.extra_flags.set_name(name, on);
        }
    }
}

/// `String arg i` (class / sound / label), quotes stripped; `""` for `none`/`null`.
pub(crate) fn sarg(args: &[String], i: usize, default: &str) -> String {
    match args.get(i).map(|s| s.trim().trim_matches('"')) {
        Some(s) if s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("null") => {
            String::new()
        }
        Some(s) if !s.is_empty() => s.to_owned(),
        _ => default.to_owned(),
    }
}

pub(crate) fn iarg(args: &[String], i: usize, default: i32) -> i32 {
    crate::action::parse_i32_arg(args, i, default)
}

pub(crate) fn farg(args: &[String], i: usize, default: f64) -> f64 {
    crate::action::parse_f64_arg(args, i, default)
}

/// Live actors of class `name` (or a subclass when `sub`).
pub(crate) fn actors_of(w: &World, name: &str) -> Vec<ActorId> {
    let Some(cid) = w.classes.find_name(name) else {
        return Vec::new();
    };
    w.thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| {
            w.actors.get(id).is_some_and(|a| {
                !a.destroyed && crate::action::class_is_or_derives(w, a.class, cid)
            })
        })
        .collect()
}

/// `SpawnMissileZAimed(z, dest, class)`: along the actor's yaw, vertical
/// speed aimed at `dest` (`P_SpawnMissileZAimed`).
pub(crate) fn spawn_missile_z_aimed(
    w: &mut World,
    src: ActorId,
    z: f64,
    dest: ActorId,
    class: &str,
) -> Option<ActorId> {
    let mut deg = angle(w, src);
    if has_flag(w, dest, "SHADOW") {
        deg += f64::from(rnd2(w, "pr_spawnmissile")) * (360.0 / 4096.0);
    }
    let cid = w.classes.find_name(class)?;
    let speed = missile_speed(w, cid);
    let dist = dist2d(w, src, dest) / speed.max(1e-6);
    let vz = if dist != 0.0 {
        (pos(w, dest).z() - pos(w, src).z()) / dist
    } else {
        speed
    };
    spawn_missile_angle_z_speed(w, src, z, class, deg, vz, Some(speed))
}

/// `SpawnSubMissile(class, owner)`: from the actor's position along its yaw,
/// pitched at whatever `AimLineAttack` finds within 1024.
pub(crate) fn spawn_sub_missile(
    w: &mut World,
    src: ActorId,
    class: &str,
    owner: Option<ActorId>,
) -> Option<ActorId> {
    let cid = w.classes.replacement(w.classes.find_name(class)?);
    let deg = angle(w, src);
    let mo = w
        .spawn_at(cid, pos(w, src), Angle::from_degrees(deg))
        .ok()?;
    let speed = missile_speed(w, cid);
    let (vx, vy) = angle_vec(deg, speed);
    let slope = aim(w, src, deg, 1024.0);
    let vz = speed * slope / (1.0 + slope * slope).sqrt();
    set_vel(w, mo, DVec3::new(vx, vy, vz));
    if let Some(a) = w.actors.get_mut(mo) {
        a.target = owner;
    }
    let p = pos(w, mo);
    if !crate::physics::try_move(w, mo, p) {
        crate::physics::explode_missile(w, mo);
        return None;
    }
    Some(mo)
}

/// `CheckBossDeath()`: no other live actor of this class remains.
pub(crate) fn check_boss_death(w: &World, id: ActorId) -> bool {
    let Some(class) = w.actors.get(id).map(|a| a.class) else {
        return false;
    };
    !w.thinkers.default.iter().any(|&o| {
        o != id
            && w.actors
                .get(o)
                .is_some_and(|a| !a.destroyed && a.class == class && a.health > 0)
    })
}

/// First live player's pawn (`players[i].mo` with `health > 0`).
pub(crate) fn live_player(w: &World) -> Option<ActorId> {
    w.players
        .iter()
        .flatten()
        .filter_map(|p| p.mo)
        .find(|&mo| health(w, mo) > 0)
}

/// `Console.MidPrint(null, msg)` (localised `$ID`s are passed through).
pub(crate) fn mid_print(w: &mut World, msg: &str) {
    if !msg.is_empty() {
        w.push_message(msg.to_owned());
    }
}

/// Give `class` to every player in the game.
pub(crate) fn give_all_players(w: &mut World, class: &str) {
    let mos: Vec<ActorId> = w.players.iter().flatten().filter_map(|p| p.mo).collect();
    for mo in mos {
        let _ = give(w, mo, class, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClassRegistry, StateTable};
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};
    use std::collections::BTreeSet;
    use std::sync::Arc;

    /// Every capitalised string literal in the ports: the classes they spawn,
    /// give or look up. Each becomes a small actor so the spawn paths run.
    fn referenced_classes() -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for src in [
            include_str!("generic.rs"),
            include_str!("heretic.rs"),
            include_str!("hexen.rs"),
            include_str!("strife.rs"),
        ] {
            for (i, part) in src.split('"').enumerate() {
                if i % 2 == 1
                    && part.len() > 2
                    && part.as_bytes()[0].is_ascii_uppercase()
                    && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    && !part.starts_with("A_")
                {
                    out.insert(part.to_owned());
                }
            }
        }
        out
    }

    struct Smoke {
        classes: Arc<ClassRegistry>,
        states: Arc<StateTable>,
        map: room_map::MapGeometry,
    }

    fn smoke_defs() -> Smoke {
        let mut src = String::from(
            "actor Mon : Actor\n{\n  Health 100\n  Radius 20\n  Height 56\n  Speed 8\n  Monster\n  MissileType \"Shot\"\n  States\n  {\n",
        );
        for label in [
            "Spawn",
            "See",
            "Melee",
            "Missile",
            "Pain",
            "Death",
            "XDeath",
            "Raise",
            "Heal",
            "Crash",
            "Burn",
            "Ice",
            "Disintegrate",
            "Active",
            "Inactive",
            "Wound",
        ] {
            src.push_str(&format!("  {label}:\n    TNT1 A 3\n    Loop\n"));
        }
        src.push_str("  }\n}\n");
        src.push_str(
            "actor Shot : Actor\n{\n  Radius 6\n  Height 8\n  Speed 10\n  Damage 2\n  Projectile\n  States\n  {\n  Spawn:\n    TNT1 A 2\n    Loop\n  Death:\n    TNT1 A 2\n    Stop\n  }\n}\n",
        );
        let mut seen = BTreeSet::new();
        for name in referenced_classes() {
            const SKIP: &[&str] = &[
                "mon",
                "shot",
                "actor",
                "none",
                "spawn",
                "see",
                "melee",
                "missile",
                "pain",
                "death",
                "xdeath",
                "raise",
                "heal",
                "crash",
                "burn",
                "ice",
                "disintegrate",
                "active",
                "inactive",
                "wound",
                "ready",
                "fire",
                "select",
                "deselect",
                "flash",
                "altfire",
                "althold",
                "hold",
                "reload",
                "zoom",
                "user1",
                "user2",
                "user3",
                "user4",
                "greetings",
                "yes",
                "no",
                "pickup",
                "use",
                "drop",
                "inventory",
                "weapon",
                "ammo",
                "health",
                "armor",
                "powerup",
                "key",
                "states",
                "monster",
                "projectile",
                "loop",
                "stop",
                "wait",
                "fail",
                "goto",
            ];
            let lower = name.to_ascii_lowercase();
            if SKIP.contains(&lower.as_str()) || !seen.insert(lower) {
                continue;
            }
            src.push_str(&format!(
                "actor {name} : Actor\n{{\n  Radius 8\n  Height 16\n  Speed 6\n  Health 20\n  +SHOOTABLE\n  States\n  {{\n  Spawn:\n    TNT1 A 2\n    Loop\n  Death:\n    TNT1 A 2\n    Stop\n  }}\n}}\n"
            ));
        }
        let ir = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(-512.0, -512.0, 512.0, 512.0), 0.0, 256.0)
            .player_start(0.0, -256.0, 90)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        Smoke {
            classes,
            states,
            map,
        }
    }

    fn smoke_world(d: &Smoke) -> (World, ActorId, ActorId, ActorId) {
        let mut w = World::from_map_with_classes(
            d.map.clone(),
            Arc::clone(&d.classes),
            Arc::clone(&d.states),
        )
        .unwrap();
        let mon = d.classes.find_name("Mon").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(p) = w.actors.get_mut(player) {
            p.flags |= crate::class::FLAG_SHOOTABLE;
            p.health = 100;
            p.height = 56.0;
            p.radius = 16.0;
        }
        let a = w
            .spawn_at(mon, DVec3::new(0.0, 0.0, 0.0), Angle::from_degrees(270.0))
            .unwrap();
        let b = w
            .spawn_at(mon, DVec3::new(128.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        for id in [a, b] {
            let m = w.actors.get_mut(id).unwrap();
            m.target = Some(player);
            m.tracer = Some(player);
        }
        w.actors.get_mut(a).unwrap().master = Some(b);
        (w, a, b, player)
    }

    #[test]
    fn every_ported_action_runs_without_panicking() {
        let names = ported_action_names();
        assert!(names.len() > 300, "ports registered: {}", names.len());
        let defs = smoke_defs();
        let fingerprint = |w: &World| -> String {
            let mut f = String::new();
            for &id in &w.thinkers.default {
                if let Some(x) = w.actors.get(id) {
                    f.push_str(&format!(
                        "{:?}{:.1}{:.1}{:.1}{:.1}{:.1}{}{:?}{}{:.2};",
                        x.class,
                        x.pos.x(),
                        x.pos.y(),
                        x.pos.z(),
                        x.vel.x(),
                        x.vel.z(),
                        x.health,
                        x.state,
                        x.destroyed,
                        x.alpha
                    ));
                }
            }
            f
        };
        let mut changed = 0usize;
        let mut idle = Vec::new();
        for name in names {
            let (mut w, a, _b, player) = smoke_world(&defs);
            let before = fingerprint(&w);
            let sounds = w.drain_sound_events().len();
            let _ = call_action_args(&mut w, a, name, &[]);
            let _ = call_action_args(&mut w, player, name, &[]);
            if fingerprint(&w) != before
                || w.drain_sound_events().len() > sounds
                || !w.drain_messages().is_empty()
            {
                changed += 1;
            } else {
                idle.push(name);
            }
            for round in 0..3 {
                // From the monster and from the player (weapon actions); the
                // last round runs against a dead target.
                if round == 2 {
                    if let Some(p) = w.actors.get_mut(player) {
                        p.health = 0;
                    }
                }
                let _ = call_action_args(&mut w, a, name, &[]);
                let _ = call_action_args(&mut w, player, name, &[]);
            }
            for _ in 0..8 {
                w.tick();
            }
        }
        eprintln!(
            "ports changed: {changed}/{}; idle: {}",
            ported_action_names().len(),
            idle.len()
        );
        // Most ports visibly act on this generic setup; the rest need a
        // particular state (a corpse, an owner weapon, a special counter).
        assert!(
            changed * 2 > ported_action_names().len(),
            "only {changed} ports changed anything; idle: {idle:?}"
        );
    }

    #[test]
    fn every_ported_name_is_dispatched() {
        let defs = smoke_defs();
        let (mut w, a, _b, _player) = smoke_world(&defs);
        for name in ported_action_names() {
            assert!(
                dispatch(&mut w, a, name, &[]).is_some(),
                "{name} is listed but not dispatched"
            );
        }
    }
}
