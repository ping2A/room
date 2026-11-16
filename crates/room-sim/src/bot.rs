//! Deterministic bot brains (`docs/28` §9 BE8).
//!
//! Bot seats get a fresh [`crate::player::TicCmd`] each tic inside the sim
//! (before `player_think`), so lockstep peers agree without sending cmds.
//!
//! Goals, in order: get back inside the battle royale zone; with only fists,
//! fetch the nearest weapon lying on the map; chase an enemy in sight (or
//! close); else head for the zone centre. Movement probes the way ahead
//! ([`crate::physics::check_position`]) and takes the open heading closest
//! to the goal; a bot that barely moves for a moment takes a detour, so it
//! never runs on the spot against a wall. Fists only swing at melee range.
//! Bots aim up and down at their target too, so guns without vertical
//! autoaim (Brutal Doom's) still hit players on other floor heights.
//! [`skill`] levels tune turn speed, aim error, reaction and hunting range;
//! bot seats get callsigns from [`BOT_NAMES`] picked by the match seed.

use crate::actor::{Actor, ActorId};
use crate::player::{Buttons, MAX_PLAYERS, PlayerState, TicCmd};
use crate::sight::check_sight;
use crate::world::World;
use room_core::{DVec3, dmath};

/// Bot difficulty levels ([`crate::World::bot_skill`]).
pub mod skill {
    /// Slow to turn and react, shoots less and misses more.
    pub const EASY: u8 = 1;
    /// The classic bot (also `0`, unset).
    pub const NORMAL: u8 = 2;
    /// Quick aim, fires every chance, hunts further.
    pub const HARD: u8 = 3;
    /// Near-instant aim and constant pressure.
    pub const NIGHTMARE: u8 = 4;
}

/// Per-level tuning. `NORMAL` keeps the original constants exactly.
#[derive(Clone, Copy)]
struct Tuning {
    /// Turn per tic (degrees).
    turn: f64,
    /// Up / down aim per tic (degrees).
    pitch: f64,
    /// Fire only when facing within this many degrees of the target.
    aim_tolerance: f64,
    /// Random yaw error added to the aim (± degrees).
    aim_error: f64,
    /// Tics the target must stay in sight before the first shot.
    reaction: u16,
    /// Strafe when `random & mask == 0` (smaller = more often).
    strafe_mask: u8,
    /// Enemies this close are chased even out of sight.
    chase_blind: f64,
    level: u8,
}

fn tuning(level: u8) -> Tuning {
    match level {
        skill::EASY => Tuning {
            turn: 4.5,
            pitch: 3.0,
            aim_tolerance: 22.0,
            aim_error: 7.0,
            reaction: 20,
            strafe_mask: 31,
            chase_blind: 512.0,
            level,
        },
        skill::HARD => Tuning {
            turn: 12.0,
            pitch: 10.0,
            aim_tolerance: 10.0,
            aim_error: 0.0,
            reaction: 4,
            strafe_mask: 7,
            chase_blind: 1024.0,
            level,
        },
        skill::NIGHTMARE => Tuning {
            turn: 20.0,
            pitch: 16.0,
            aim_tolerance: 6.0,
            aim_error: 0.0,
            reaction: 0,
            strafe_mask: 3,
            chase_blind: 1600.0,
            level,
        },
        _ => Tuning {
            turn: 8.0,
            pitch: 6.0,
            aim_tolerance: 15.0,
            aim_error: 0.0,
            reaction: 0,
            strafe_mask: 15,
            chase_blind: CHASE_BLIND,
            level: skill::NORMAL,
        },
    }
}

/// Callsigns for bot seats (plain ASCII, valid display names).
pub const BOT_NAMES: [&str; 40] = [
    "Viper",
    "Rook",
    "Havoc",
    "Banshee",
    "Reaper",
    "Gunner",
    "Specter",
    "Blaze",
    "Cinder",
    "Mortar",
    "Wraith",
    "Jackal",
    "Nova",
    "Raptor",
    "Onyx",
    "Vulture",
    "Hex",
    "Scorch",
    "Talon",
    "Bishop",
    "Ghoul",
    "Rampage",
    "Fang",
    "Brimstone",
    "Cobra",
    "Echo",
    "Ironside",
    "Sable",
    "Torque",
    "Vandal",
    "Warden",
    "Zephyr",
    "Diesel",
    "Grit",
    "Maul",
    "Nitro",
    "Rivet",
    "Slag",
    "Thorn",
    "Kestrel",
];

/// Deterministic callsign for `seat` in a match seeded with `seed`: the
/// match picks a permutation of [`BOT_NAMES`], seats walk it, and names
/// `taken` elsewhere are skipped.
#[must_use]
pub fn pick_bot_name(seed: u32, seat: usize, taken: impl Fn(&str) -> bool) -> &'static str {
    const STEPS: [usize; 16] = [1, 3, 7, 9, 11, 13, 17, 19, 21, 23, 27, 29, 31, 33, 37, 39];
    let n = BOT_NAMES.len();
    let mut h = u64::from(seed) ^ 0x9E37_79B9_7F4A_7C15;
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    let start = (h % n as u64) as usize;
    let step = STEPS[((h >> 32) % STEPS.len() as u64) as usize];
    let first = (start + seat * step) % n;
    (0..n)
        .map(|k| BOT_NAMES[(first + k) % n])
        .find(|name| !taken(name))
        .unwrap_or(BOT_NAMES[first])
}
const YAW_SCALE: f64 = 65536.0 / 360.0;
const MOVE: i16 = 40 * 256;
/// Doom max step up.
const STEP: f64 = 24.0;
/// Probe distances ahead (map units).
const PROBES: [f64; 2] = [40.0, 88.0];
/// Headings tried around the goal direction (degrees).
const OFFSETS: [f64; 9] = [0.0, 30.0, -30.0, 60.0, -60.0, 90.0, -90.0, 135.0, -135.0];
/// Moved less than this per tic while walking = blocked.
const STUCK_SPEED: f64 = 1.5;
/// Blocked tics before a detour.
const STUCK_TICS: u16 = 7;
/// Detour length.
const DETOUR_TICS: u16 = 30;
/// Fists and chainsaw reach.
const MELEE: f64 = 80.0;
/// Weapons farther than this are ignored.
const WEAPON_RANGE: f64 = 2400.0;
/// Enemies this close are chased even out of sight.
const CHASE_BLIND: f64 = 768.0;

/// Fill [`World::players`][`crate::player::Player::cmd`] for every bot seat.
pub fn run_bot_brains(world: &mut World) {
    for seat in 0..MAX_PLAYERS {
        let bot = world.players[seat].as_ref().is_some_and(|p| p.is_bot);
        if !bot {
            continue;
        }
        let cmd = build_cmd(world, seat);
        if let Some(p) = world.players[seat].as_mut() {
            p.cmd = cmd;
        }
    }
}

fn build_cmd(world: &mut World, seat: usize) -> TicCmd {
    let mut cmd = TicCmd::default();
    let Some(player) = world.players[seat].as_ref() else {
        return cmd;
    };
    if player.player_state != PlayerState::Live {
        return cmd;
    }
    let Some(mo) = player.mo else {
        return cmd;
    };
    let Some(a) = world.actors.get(mo).cloned() else {
        return cmd;
    };
    let armed = player
        .ready_weapon
        .and_then(|w| world.classes.get(w))
        .is_some_and(|c| !c.name.eq_ignore_ascii_case("Fist"));
    let melee_only = player
        .ready_weapon
        .and_then(|w| world.classes.get(w))
        .is_none_or(|c| {
            c.name.eq_ignore_ascii_case("Fist") || c.name.eq_ignore_ascii_case("Chainsaw")
        });
    let (px, py) = (a.pos.x(), a.pos.y());
    let angle_deg = a.angle.degrees();

    // Stuck tracking: distance moved since last tic while trying to walk.
    let (moved, was_walking) = {
        let p = world.players[seat].as_ref().unwrap();
        let dx = px - p.bot_last_xy.0;
        let dy = py - p.bot_last_xy.1;
        (dmath::sqrt(dx * dx + dy * dy), p.bot_walking)
    };

    let tune = tuning(world.bot_skill);
    let enemy = nearest_enemy(world, seat, mo);
    let enemy_seen = enemy.filter(|&e| check_sight(world, mo, e));
    let goal = bot_goal_xy(
        world,
        seat,
        mo,
        &a,
        armed,
        enemy,
        enemy_seen,
        tune.chase_blind,
    );
    // Reaction delay: how long the current target has been in sight.
    let seen_for = {
        let p = world.players[seat].as_mut().unwrap();
        p.bot_seen = if enemy_seen.is_some() {
            p.bot_seen.saturating_add(1)
        } else {
            0
        };
        p.bot_seen
    };

    let mut walking = false;
    if let Some((gx, gy)) = goal {
        let (dx, dy) = (gx - px, gy - py);
        // Items are picked up by moving over them: keep walking onto a gun
        // even when it lies right underfoot.
        let on_target = dx * dx + dy * dy <= 24.0 * 24.0;
        if on_target && !armed {
            cmd.forward_move = MOVE / 2;
            walking = true;
        }
        if !on_target {
            let want = dmath::atan2(dy, dx).to_degrees();
            let p = world.players[seat].as_mut().unwrap();
            if was_walking && moved < STUCK_SPEED {
                p.bot_stuck = p.bot_stuck.saturating_add(1);
            } else {
                p.bot_stuck = 0;
            }
            let stuck = p.bot_stuck >= STUCK_TICS;
            if stuck {
                p.bot_stuck = 0;
            }
            let detouring = p.bot_detour > 0;
            let detour_dir = p.bot_detour_dir;
            let heading = if stuck {
                // Turn away: sideways or back, alternating by seat and time.
                let r = world.p_random(&format!("bot_detour_{seat}"));
                let side = if r & 1 == 0 { 1.0 } else { -1.0 };
                let turn = 90.0 + f64::from(r % 4) * 22.5;
                let dir = want + side * turn;
                let p = world.players[seat].as_mut().unwrap();
                p.bot_detour = DETOUR_TICS;
                p.bot_detour_dir = dir;
                dir
            } else if detouring {
                let p = world.players[seat].as_mut().unwrap();
                p.bot_detour -= 1;
                detour_dir
            } else {
                open_heading(world, &a, mo, want).unwrap_or(want)
            };
            cmd.yaw = yaw_toward_deg(angle_deg, heading, tune.turn);
            // Walk once roughly facing the heading (no wide arcs into walls);
            // near the goal turn on the spot first, or the bot orbits it.
            let off = wrap(heading - angle_deg).abs();
            let near = dx * dx + dy * dy < 160.0 * 160.0;
            if near && off > 20.0 {
                // turning only
            } else if off < 60.0 {
                cmd.forward_move = MOVE;
                walking = true;
            } else if off < 100.0 {
                cmd.forward_move = MOVE / 2;
                walking = true;
            }
            let r = world.p_random(&format!("bot_strafe_{seat}"));
            if r & tune.strafe_mask == 0 && enemy_seen.is_some() {
                cmd.side_move = if r & 16 != 0 { MOVE / 2 } else { -MOVE / 2 };
            }
        }
    }

    // Aim up / down too: weapons without vertical autoaim (Brutal Doom's,
    // or any game with autoaim off) shoot along the pitch, so a level gun
    // misses anyone on another floor height. Level out with no target.
    let pitch_deg = a.pitch.degrees();
    let mut want_pitch = 0.0;
    if let Some(target) = enemy_seen {
        let ta = world.actors.get(target).unwrap();
        let (tx, ty) = (ta.pos.x() - px, ta.pos.y() - py);
        let dist = dmath::sqrt(tx * tx + ty * ty);
        let eye = world.players[seat]
            .as_ref()
            .map_or(a.pos.z() + 41.0, |p| p.view_z);
        let chest = ta.pos.z() + ta.height * 0.55;
        want_pitch = dmath::atan2(chest - eye, dist.max(1.0)).to_degrees();
        let close_enough = !melee_only || dist < MELEE + ta.radius;
        if close_enough {
            let mut want = dmath::atan2(ty, tx).to_degrees();
            if tune.aim_error > 0.0 {
                let r = world.p_random(&format!("bot_aim_{seat}"));
                want += (f64::from(r % 201) / 100.0 - 1.0) * tune.aim_error;
            }
            cmd.yaw = yaw_toward_deg(angle_deg, want, tune.turn);
            let fire = world.p_random(&format!("bot_fire_{seat}"));
            let aimed = wrap(want - angle_deg).abs() < tune.aim_tolerance;
            let trigger = match tune.level {
                skill::EASY => fire % 3 == 0,
                skill::NORMAL => fire % 3 != 0,
                _ => true,
            };
            if aimed && seen_for > tune.reaction && (melee_only || trigger) {
                cmd.buttons = Buttons(Buttons::ATTACK);
            }
        }
    }

    cmd.pitch = pitch_toward_deg(pitch_deg, want_pitch, tune.pitch);

    if let Some(p) = world.players[seat].as_mut() {
        p.bot_last_xy = (px, py);
        p.bot_walking = walking;
    }
    cmd
}

/// Where the bot walks to, if anywhere.
fn bot_goal_xy(
    world: &World,
    seat: usize,
    me: ActorId,
    a: &Actor,
    armed: bool,
    enemy: Option<ActorId>,
    enemy_seen: Option<ActorId>,
    chase_blind: f64,
) -> Option<(f64, f64)> {
    let (px, py) = (a.pos.x(), a.pos.y());
    let zone = world
        .mod_zones
        .get(0)
        .map(|z| (f64::from(z.cx), f64::from(z.cy), f64::from(z.r).max(32.0)));
    if let Some((cx, cy, r)) = zone {
        let (dx, dy) = (px - cx, py - cy);
        if dx * dx + dy * dy > (r * 0.82) * (r * 0.82) {
            return Some((cx, cy));
        }
    }
    let enemy_xy = |e: ActorId| world.actors.get(e).map(|t| (t.pos.x(), t.pos.y()));
    if !armed {
        // A gun first, unless someone is already in punching range.
        let close = enemy_seen
            .and_then(enemy_xy)
            .is_some_and(|(ex, ey)| (ex - px).powi(2) + (ey - py).powi(2) < (MELEE * 2.0).powi(2));
        if !close && let Some(w) = nearest_weapon(world, me, a) {
            return Some(w);
        }
    }
    if let Some(e) = enemy_seen {
        return enemy_xy(e);
    }
    if let Some(e) = enemy
        && let Some((ex, ey)) = enemy_xy(e)
        && (ex - px).powi(2) + (ey - py).powi(2) < chase_blind * chase_blind
    {
        return Some((ex, ey));
    }
    if let Some((cx, cy, _)) = zone {
        // Roam around the centre rather than all piling onto one point.
        let t = f64::from(world.level.maptime / 105 % 8) * 0.785 + seat as f64;
        let roam = 192.0;
        return Some((cx + dmath::cos(t) * roam, cy + dmath::sin(t) * roam));
    }
    let t = f64::from(world.level.maptime % 350) * 0.02 + seat as f64;
    Some((
        px + dmath::cos(t) * 256.0,
        py + dmath::sin(t * 1.31) * 256.0,
    ))
}

/// Nearest weapon pickup lying on the map (visible ones count double close).
fn nearest_weapon(world: &World, me: ActorId, a: &Actor) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64, f64)> = None;
    for &id in &world.thinkers.default {
        let Some(w) = world.actors.get(id) else {
            continue;
        };
        if w.destroyed || !world.classes.inherits_from(w.class, "Weapon") {
            continue;
        }
        // Owned already: walking over it does nothing useful.
        if world.inventory.count(me, w.class) > 0 {
            continue;
        }
        let (dx, dy) = (w.pos.x() - a.pos.x(), w.pos.y() - a.pos.y());
        let d = dx * dx + dy * dy;
        if d > WEAPON_RANGE * WEAPON_RANGE {
            continue;
        }
        if best.is_none_or(|(bd, _, _)| d < bd) {
            best = Some((d, w.pos.x(), w.pos.y()));
        }
    }
    best.map(|(_, x, y)| (x, y))
}

/// First heading near `want` whose way ahead is open (walkable floor within
/// a step, room for the body, no wall).
fn open_heading(world: &World, a: &Actor, mo: ActorId, want: f64) -> Option<f64> {
    OFFSETS
        .iter()
        .map(|o| want + o)
        .find(|&h| PROBES.iter().all(|&d| passable(world, a, mo, h, d)))
}

fn passable(world: &World, a: &Actor, mo: ActorId, heading: f64, dist: f64) -> bool {
    let rad = heading.to_radians();
    let pos = DVec3::new(
        a.pos.x() + dmath::cos(rad) * dist,
        a.pos.y() + dmath::sin(rad) * dist,
        a.pos.z(),
    );
    let r = crate::physics::check_position(world, a, pos, Some(mo));
    r.ok && r.floor_z - a.pos.z() <= STEP && r.ceiling_z - r.floor_z >= a.height
}

fn nearest_enemy(world: &World, seat: usize, self_mo: ActorId) -> Option<ActorId> {
    let mut best: Option<(f64, ActorId)> = None;
    for (other, p) in world.players.iter().enumerate() {
        if other == seat {
            continue;
        }
        let Some(p) = p else { continue };
        if p.player_state != PlayerState::Live {
            continue;
        }
        let Some(mo) = p.mo else { continue };
        if mo == self_mo {
            continue;
        }
        let Some(a) = world.actors.get(mo) else {
            continue;
        };
        if a.health <= 0 || a.destroyed {
            continue;
        }
        let d = dist2(world, self_mo, mo);
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, mo));
        }
    }
    best.map(|(_, id)| id)
}

fn dist2(world: &World, a: ActorId, b: ActorId) -> f64 {
    let pa = world.actors.get(a).unwrap().pos;
    let pb = world.actors.get(b).unwrap().pos;
    let dx = pa.x() - pb.x();
    let dy = pa.y() - pb.y();
    dx * dx + dy * dy
}

fn wrap(mut deg: f64) -> f64 {
    while deg > 180.0 {
        deg -= 360.0;
    }
    while deg < -180.0 {
        deg += 360.0;
    }
    deg
}

/// Pitch step toward `want_deg` (positive looks up), same units as yaw.
fn pitch_toward_deg(from_deg: f64, want_deg: f64, per_tic: f64) -> i16 {
    let turn = wrap(want_deg - from_deg).clamp(-per_tic, per_tic);
    (turn * YAW_SCALE) as i16
}

fn yaw_toward_deg(from_deg: f64, want_deg: f64, per_tic: f64) -> i16 {
    let turn = wrap(want_deg - from_deg).clamp(-per_tic, per_tic);
    (turn * YAW_SCALE) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_names_are_valid_distinct_and_vary_by_match() {
        for name in BOT_NAMES {
            assert_eq!(
                crate::sanitize_display_name(name).as_deref(),
                Some(name),
                "{name} must be a valid display name"
            );
        }
        let pick_all = |seed: u32| {
            let mut got: Vec<&str> = Vec::new();
            for seat in 0..8 {
                let n = pick_bot_name(seed, seat, |c| {
                    got.iter().any(|g| g.eq_ignore_ascii_case(c))
                });
                got.push(n);
            }
            got
        };
        let a = pick_all(1);
        let mut uniq = a.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), 8, "eight different names: {a:?}");
        assert_eq!(a, pick_all(1), "same seed, same names (lockstep)");
        assert_ne!(a, pick_all(2), "another match, other names");
    }

    #[test]
    fn normal_is_the_default_level() {
        let n = tuning(0);
        assert_eq!(n.level, skill::NORMAL);
        assert!((n.turn - 8.0).abs() < f64::EPSILON);
        assert!(tuning(skill::EASY).turn < n.turn);
        assert!(tuning(skill::NIGHTMARE).turn > tuning(skill::HARD).turn);
    }
}
