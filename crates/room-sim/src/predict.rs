//! Client prediction (UZ12): where the local player will be once the inputs
//! it has sent but the lockstep has not released yet are played.
//!
//! The playsim stays lockstep-authoritative and is never touched: this reads
//! the confirmed world and walks a copy of the player's pawn through the
//! pending commands with the same thrust rules as `move_player`, checking
//! walls and solid things with the read-only [`check_position`]. Only the
//! view (eye position, yaw, pitch) is predicted. Each frame starts again from
//! the confirmed pawn, so a guess that went wrong (a door, a push, a monster
//! in the way) is corrected as soon as the real tic arrives.

use crate::physics::check_position;
use crate::player::{PlayerState, TicCmd};
use crate::vanilla::SimMathMode;
use crate::world::World;
use room_core::{Angle, DVec3};

/// Turn units of `TicCmd::yaw` / `pitch` in degrees.
const YAW_TO_DEG: f64 = 360.0 / 65536.0;
/// `TicCmd` move units per map unit of thrust (`think_player::MOVE_SCALE`).
const MOVE_SCALE: f64 = 1.0 / 2048.0;
/// Doom `ORIG_FRICTION`.
const FRICTION: f64 = 0.90625;
/// Highest step a walking pawn climbs.
const MAX_STEP: f64 = 24.0;

/// The predicted view of one player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictedView {
    /// Pawn position (feet).
    pub pos: DVec3,
    /// Yaw.
    pub angle: Angle,
    /// Pitch.
    pub pitch: Angle,
    /// Eye height above the map origin (the player's `view_z` moved with the
    /// pawn).
    pub view_z: f64,
}

impl World {
    /// Predict seat `seat` after `pending` (oldest first). `None` when there
    /// is nothing to predict or the player is in a state prediction does not
    /// model (dead, frozen, viewing through a camera, flying, no-clipping, a
    /// Duke pawn); the caller then draws the confirmed view.
    #[must_use]
    pub fn predict_player_view(&self, seat: usize, pending: &[TicCmd]) -> Option<PredictedView> {
        let player = self.players.get(seat)?.as_ref()?;
        let mo = player.mo?;
        let actor = self.actors.get(mo)?;
        if pending.is_empty()
            || player.player_state != PlayerState::Live
            || player.camera.is_some()
            || player.frozen
            || player.noclip
            || player.fly_tics > 0
            || self.level.frozen_power
            || self.paused
        {
            return None;
        }
        let class = self.classes.get(actor.class)?;
        if class.name == "DukePlayer" {
            return None;
        }
        let fwd_scale = if class.forward_move > 0.0 {
            class.forward_move
        } else {
            1.0
        };
        let side_scale = if (class.ext.side_move - 1.0).abs() > 1e-9 {
            class.ext.side_move
        } else {
            fwd_scale
        };
        let vanilla = self.math_mode == SimMathMode::VanillaFixed;
        let mut probe = actor.clone();
        let eye = player.view_z - actor.pos.z();
        let mut vel = actor.vel;
        let mut floor_z = crate::physics::floor_at_xy(self, actor.pos.x(), actor.pos.y())
            .unwrap_or(actor.pos.z());
        for cmd in pending {
            // Turn first, as `player_think` does.
            if cmd.yaw != 0 {
                probe.angle =
                    Angle::from_degrees(probe.angle.degrees() + f64::from(cmd.yaw) * YAW_TO_DEG);
            }
            if cmd.pitch != 0 {
                let deg = (probe.pitch.degrees() + f64::from(cmd.pitch) * YAW_TO_DEG)
                    .clamp(-89.0, 89.0);
                probe.pitch = Angle::from_degrees(deg);
            }
            let mut forward = f64::from(cmd.forward_move) * fwd_scale;
            let mut side = f64::from(cmd.side_move) * side_scale;
            if player.crouching {
                forward /= 2.0;
                side /= 2.0;
            }
            let rad = probe.angle.radians();
            let (s, c) = (room_core::dmath::sin(rad), room_core::dmath::cos(rad));
            let on_ground = probe.pos.z() <= floor_z + 0.1;
            if vanilla {
                // Doom `P_Thrust` adds to momentum on the ground; then
                // `P_XYMovement` moves by it and applies friction.
                if on_ground {
                    let f = f64::from(cmd.forward_move >> 8) * 2048.0 / 65536.0 * fwd_scale;
                    let sd = f64::from(cmd.side_move >> 8) * 2048.0 / 65536.0 * side_scale;
                    vel = DVec3::new(
                        vel.x() + f * c + sd * s,
                        vel.y() + f * s - sd * c,
                        vel.z(),
                    );
                }
                self.predict_step(&mut probe, mo, &mut floor_z, vel.x(), vel.y());
                if on_ground {
                    vel = DVec3::new(vel.x() * FRICTION, vel.y() * FRICTION, vel.z());
                }
            } else {
                let (fm, sm) = (forward * MOVE_SCALE, side * MOVE_SCALE);
                if fm == 0.0 && sm == 0.0 {
                    if on_ground {
                        vel = DVec3::new(0.0, 0.0, vel.z());
                    }
                    if !on_ground {
                        self.predict_step(&mut probe, mo, &mut floor_z, vel.x(), vel.y());
                    }
                    continue;
                }
                // `move_player` sets the speed and moves once; the world's
                // `xy_movement` then moves by the same speed again.
                let (tx, ty) = (fm * c + sm * s, fm * s - sm * c);
                vel = DVec3::new(tx, ty, vel.z());
                self.predict_step(&mut probe, mo, &mut floor_z, tx, ty);
                self.predict_step(&mut probe, mo, &mut floor_z, tx, ty);
            }
        }
        Some(PredictedView {
            pos: probe.pos,
            angle: probe.angle,
            pitch: probe.pitch,
            view_z: probe.pos.z() + eye,
        })
    }

    /// Move `probe` by `(dx, dy)` if it fits, else slide along one axis, in
    /// steps no longer than its radius. Floors under the new spot are
    /// followed down and climbed up to a step.
    fn predict_step(
        &self,
        probe: &mut crate::actor::Actor,
        mo: crate::actor::ActorId,
        floor_z: &mut f64,
        dx: f64,
        dy: f64,
    ) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let max = (probe.radius - 1.0).max(1.0);
        let steps = (dx.abs().max(dy.abs()) / max).ceil().clamp(1.0, 64.0);
        let (sx, sy) = (dx / steps, dy / steps);
        for _ in 0..steps as usize {
            let tries = [(sx, sy), (sx, 0.0), (0.0, sy)];
            let mut moved = false;
            for (mx, my) in tries {
                if mx == 0.0 && my == 0.0 {
                    continue;
                }
                let to = DVec3::new(probe.pos.x() + mx, probe.pos.y() + my, probe.pos.z());
                // The confirmed pawn is still in the world: do not collide with it.
                let r = check_position(self, probe, to, Some(mo));
                if r.ok
                    && r.floor_z - probe.pos.z() <= MAX_STEP
                    && r.ceiling_z - r.floor_z >= probe.height
                    && r.ceiling_z - probe.pos.z().max(r.floor_z) >= probe.height
                {
                    let z = if r.floor_z > probe.pos.z() || probe.pos.z() - r.floor_z <= MAX_STEP {
                        r.floor_z
                    } else {
                        probe.pos.z()
                    };
                    probe.pos = DVec3::new(to.x(), to.y(), z);
                    *floor_z = r.floor_z;
                    moved = true;
                    break;
                }
            }
            if !moved {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{Buttons, ClassRegistry, StateTable};
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    fn world() -> World {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 256.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        w.set_ticcmds(&[TicCmd::default()]);
        w.tick();
        w
    }

    fn walk(yaw: i16) -> TicCmd {
        TicCmd {
            forward_move: 0x32 * 256,
            yaw,
            buttons: Buttons(0),
            ..TicCmd::default()
        }
    }

    #[test]
    fn prediction_lands_where_the_lockstep_later_puts_the_player() {
        let mut w = world();
        let pending = [walk(0), walk(512), walk(512), walk(0)];
        let guess = w.predict_player_view(0, &pending).unwrap();
        let before = w.checksum();
        // Predicting changed nothing in the world.
        assert_eq!(w.checksum(), before);
        for cmd in pending {
            w.set_ticcmds(&[cmd]);
            w.tick();
        }
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let real = w.actors.get(mo).unwrap();
        let d = (guess.pos.x() - real.pos.x()).hypot(guess.pos.y() - real.pos.y());
        assert!(d < 0.01, "guess {:?} real {:?}", guess.pos, real.pos);
        assert!((guess.angle.degrees() - real.angle.degrees()).abs() < 1e-6);
        let view = w.players[0].as_ref().unwrap().view_z;
        assert!((guess.view_z - view).abs() < 8.0, "eye {} vs {view}", guess.view_z);
    }

    #[test]
    fn prediction_stops_at_walls_and_refuses_states_it_does_not_model() {
        let mut w = world();
        // Thirty tics east hits the wall at x = 512 (radius 16).
        let pending = vec![walk(0); 30];
        let guess = w.predict_player_view(0, &pending).unwrap();
        assert!(guess.pos.x() <= 512.0 - 16.0 + 0.01 && guess.pos.x() > 400.0, "{:?}", guess.pos);
        assert!(w.predict_player_view(0, &[]).is_none());
        w.players[0].as_mut().unwrap().player_state = PlayerState::Dead;
        assert!(w.predict_player_view(0, &pending).is_none());
    }
}
