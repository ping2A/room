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

use crate::actor::{Actor, ActorId};
use crate::player::{Buttons, MAX_PLAYERS, PlayerState, TicCmd};
use crate::sight::check_sight;
use crate::world::World;
use room_core::{DVec3, dmath};

const TURN_PER_TIC: f64 = 8.0;
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

    let enemy = nearest_enemy(world, seat, mo);
    let enemy_seen = enemy.filter(|&e| check_sight(world, mo, e));
    let goal = bot_goal_xy(world, seat, mo, &a, armed, enemy, enemy_seen);

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
            cmd.yaw = yaw_toward_deg(angle_deg, heading);
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
            if r & 15 == 0 && enemy_seen.is_some() {
                cmd.side_move = if r & 16 != 0 { MOVE / 2 } else { -MOVE / 2 };
            }
        }
    }

    if let Some(target) = enemy_seen {
        let ta = world.actors.get(target).unwrap();
        let (tx, ty) = (ta.pos.x() - px, ta.pos.y() - py);
        let dist = dmath::sqrt(tx * tx + ty * ty);
        let close_enough = !melee_only || dist < MELEE + ta.radius;
        if close_enough {
            cmd.yaw = yaw_toward_deg(angle_deg, dmath::atan2(ty, tx).to_degrees());
            let fire = world.p_random(&format!("bot_fire_{seat}"));
            let aimed = wrap(dmath::atan2(ty, tx).to_degrees() - angle_deg).abs() < 15.0;
            if aimed && (melee_only || fire % 3 != 0) {
                cmd.buttons = Buttons(Buttons::ATTACK);
            }
        }
    }

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
        && (ex - px).powi(2) + (ey - py).powi(2) < CHASE_BLIND * CHASE_BLIND
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

fn yaw_toward_deg(from_deg: f64, want_deg: f64) -> i16 {
    let turn = wrap(want_deg - from_deg).clamp(-TURN_PER_TIC, TURN_PER_TIC);
    (turn * YAW_SCALE) as i16
}
