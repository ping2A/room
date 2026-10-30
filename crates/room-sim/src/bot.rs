//! Deterministic bot brains (`docs/28` §9 BE8 MVP).
//!
//! Bot seats get a fresh [`crate::player::TicCmd`] each tic inside the sim
//! (before `player_think`), so lockstep peers agree without sending cmds.

use crate::player::{Buttons, MAX_PLAYERS, PlayerState, TicCmd};
use crate::sight::check_sight;
use crate::world::World;
use room_core::dmath;

const TURN_PER_TIC: f64 = 6.0;
const YAW_SCALE: f64 = 65536.0 / 360.0;
const MOVE: i16 = 40 * 256;

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
    let Some(a) = world.actors.get(mo) else {
        return cmd;
    };
    let (px, py) = (a.pos.x(), a.pos.y());
    let angle_deg = a.angle.degrees();

    let (gx, gy) = bot_goal_xy(world, seat, px, py);
    let dx = gx - px;
    let dy = gy - py;
    if dx * dx + dy * dy > 64.0 {
        cmd.yaw = yaw_toward(angle_deg, dx, dy);
        cmd.forward_move = MOVE;
        let r = world.p_random(&format!("bot_strafe_{seat}"));
        if r & 7 == 0 {
            cmd.side_move = if r & 8 != 0 { MOVE / 2 } else { -MOVE / 2 };
        }
    }

    if let Some(target) = nearest_enemy(world, seat, mo) {
        if check_sight(world, mo, target) {
            let ta = world.actors.get(target).unwrap();
            cmd.yaw = yaw_toward(angle_deg, ta.pos.x() - px, ta.pos.y() - py);
            let fire = world.p_random(&format!("bot_fire_{seat}"));
            if fire % 28 == 0 {
                cmd.buttons = Buttons(Buttons::ATTACK);
            }
        }
    }

    cmd
}

fn bot_goal_xy(world: &World, seat: usize, px: f64, py: f64) -> (f64, f64) {
    if let Some(zone) = world.mod_zones.get(0) {
        let (cx, cy, r) = (
            f64::from(zone.cx),
            f64::from(zone.cy),
            f64::from(zone.r).max(32.0),
        );
        let dx = px - cx;
        let dy = py - cy;
        let dist = dmath::sqrt(dx * dx + dy * dy);
        if dist > r * 0.82 {
            return (cx, cy);
        }
    }
    if let Some((ex, ey)) = nearest_enemy_xy(world, seat) {
        return (ex, ey);
    }
    let t = f64::from(world.level.maptime % 350) * 0.02 + f64::from(seat as u32);
    (
        px + dmath::cos(t) * 256.0,
        py + dmath::sin(t * 1.31) * 256.0,
    )
}

fn nearest_enemy(
    world: &World,
    seat: usize,
    self_mo: crate::actor::ActorId,
) -> Option<crate::actor::ActorId> {
    let mut best: Option<(f64, crate::actor::ActorId)> = None;
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

fn nearest_enemy_xy(world: &World, seat: usize) -> Option<(f64, f64)> {
    let self_mo = world.players[seat].as_ref()?.mo?;
    let id = nearest_enemy(world, seat, self_mo)?;
    let a = world.actors.get(id)?;
    Some((a.pos.x(), a.pos.y()))
}

fn dist2(world: &World, a: crate::actor::ActorId, b: crate::actor::ActorId) -> f64 {
    let pa = world.actors.get(a).unwrap().pos;
    let pb = world.actors.get(b).unwrap().pos;
    let dx = pa.x() - pb.x();
    let dy = pa.y() - pb.y();
    dx * dx + dy * dy
}

fn yaw_toward(from_deg: f64, dx: f64, dy: f64) -> i16 {
    if dx * dx + dy * dy < 1.0 {
        return 0;
    }
    let want = dmath::atan2(dy, dx) * (180.0 / core::f64::consts::PI);
    let mut diff = want - from_deg;
    while diff > 180.0 {
        diff -= 360.0;
    }
    while diff < -180.0 {
        diff += 360.0;
    }
    let turn = diff.clamp(-TURN_PER_TIC, TURN_PER_TIC);
    (turn * YAW_SCALE) as i16
}
