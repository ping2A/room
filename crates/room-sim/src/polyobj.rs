//! Polyobject movers (`docs/06` §6, GZDoom `po_man.cpp`).
//!
//! Bootstrap: spawn from load-time [`room_map::setup::PolyobjStatic`], then
//! `Polyobj_Move` / `Rotate*` / `DoorSwing` / `DoorSlide` / `Stop`, with mirror
//! sync and solid collide. Poly lines are relinked into the blockmap whenever
//! the poly moves so collision, use and hitscan see their current position.

use crate::class::{FLAG_SHOOTABLE, FLAG_SOLID};
use crate::world::World;
use room_core::{DVec2, LineId, VertexId, dmath};
use room_map::setup::PolyobjStatic;
use std::sync::Arc;

/// Hexen/Doom crush amount when a crushing poly pins an actor.
const POLY_CRUSH_DAMAGE: i32 = 10;

/// Radians per Hexen byte angle.
const RAD_PER_BYTE: f64 = std::f64::consts::TAU / 256.0;

/// Active polyobject instance.
#[derive(Clone, Debug)]
pub struct PolyobjDynamic {
    /// Tag / poly id.
    pub id: i32,
    /// Vertices owned by this poly (indices into `World::map.vertices`).
    pub vertices: Vec<VertexId>,
    /// Vertex offsets from `center` at angle 0 (GZDoom `OriginalPts`).
    pub offsets: Vec<DVec2>,
    /// Linedefs belonging to the poly.
    pub lines: Vec<LineId>,
    /// Mirror poly id (`0` = none).
    pub mirror: i32,
    /// Crushing when blocked.
    pub crush: bool,
    /// Rotation center (start spot after relocate).
    pub center: DVec2,
    /// Start-spot XY (rotation origin / GetPolyobjX|Y).
    pub start_spot: DVec2,
    /// Current angle in radians (for rotate movers).
    pub angle: f64,
    /// Active translation / rotation / wait, if any.
    pub motion: Option<PolyMotion>,
    /// Door state while a `DoorSwing` / `DoorSlide` runs.
    pub door: Option<PolyDoor>,
    /// A door that opened for good (negative delay) never moves again.
    pub blocked: bool,
}

/// `Polyobj_DoorSwing` / `DoorSlide` bookkeeping (GZDoom `DPolyDoor`).
#[derive(Clone, Copy, Debug)]
pub struct PolyDoor {
    /// Full opening leg; closing runs it reversed.
    pub open: PolyMotionResume,
    /// Tics to stay open; negative stays open for good.
    pub delay: i32,
    /// Running (or waiting for) the closing leg.
    pub closing: bool,
}

/// One poly move / rotate / door-wait job.
#[derive(Clone, Copy, Debug)]
pub enum PolyMotion {
    /// Translate by `dir * speed` each tic until `remaining` hits 0.
    Move {
        /// Unit direction.
        dir: DVec2,
        /// Map units per tic.
        speed: f64,
        /// Distance left.
        remaining: f64,
    },
    /// Rotate by `omega` rad/tic until `|remaining|` hits 0 (infinite = perpetual).
    Rotate {
        /// Signed radians per tic.
        omega: f64,
        /// Signed radians left.
        remaining: f64,
    },
    /// Door delay before the closing leg (`DoorSwing` / `DoorSlide`).
    Wait {
        /// Tics left.
        tics: i32,
        /// Motion to start when the wait ends.
        then: PolyMotionResume,
    },
}

/// Closing (or follow-up) leg after a [`PolyMotion::Wait`].
#[derive(Clone, Copy, Debug)]
pub enum PolyMotionResume {
    /// Resume a translation.
    Move {
        /// Unit direction.
        dir: DVec2,
        /// Map units per tic.
        speed: f64,
        /// Distance left.
        remaining: f64,
    },
    /// Resume a rotation.
    Rotate {
        /// Signed radians per tic.
        omega: f64,
        /// Signed radians left.
        remaining: f64,
    },
}

impl PolyMotionResume {
    fn into_motion(self) -> PolyMotion {
        match self {
            Self::Move {
                dir,
                speed,
                remaining,
            } => PolyMotion::Move {
                dir,
                speed,
                remaining,
            },
            Self::Rotate { omega, remaining } => PolyMotion::Rotate { omega, remaining },
        }
    }

    /// Same leg run backwards.
    fn reversed(self) -> Self {
        match self {
            Self::Move {
                dir,
                speed,
                remaining,
            } => Self::Move {
                dir: DVec2::new(-dir.x(), -dir.y()),
                speed,
                remaining,
            },
            Self::Rotate { omega, remaining } => Self::Rotate {
                omega: -omega,
                remaining: -remaining,
            },
        }
    }
}

/// DoomEdNums for poly anchor / spawn (ZDoom; Hexen 3000–3002 also accepted).
const PO_ANCHOR: i16 = 9300;
const PO_SPAWN: i16 = 9301;
const PO_SPAWN_CRUSH: i16 = 9302;
const PO_SPAWN_HURT: i16 = 9303;
const PO_ANCHOR_HEXEN: i16 = 3000;
const PO_SPAWN_HEXEN: i16 = 3001;
const PO_SPAWN_CRUSH_HEXEN: i16 = 3002;

/// Build runtime polys from map setup; relocate by anchor→start-spot delta.
pub fn init_polyobjs(world: &mut World) {
    let statics: Vec<PolyobjStatic> = world.map.setup.polyobjects.clone();
    // angle field holds poly id for these things.
    let mut anchors: Vec<(i32, DVec2)> = Vec::new();
    let mut spawns: Vec<(i32, DVec2)> = Vec::new();
    for th in &world.map.things {
        let pos = DVec2::new(f64::from(th.x), f64::from(th.y));
        let id = i32::from(th.angle);
        match th.type_id {
            PO_ANCHOR | PO_ANCHOR_HEXEN => anchors.push((id, pos)),
            PO_SPAWN | PO_SPAWN_CRUSH | PO_SPAWN_HURT | PO_SPAWN_HEXEN | PO_SPAWN_CRUSH_HEXEN => {
                spawns.push((id, pos));
            }
            _ => {}
        }
    }
    world.polyobjs.clear();
    for s in statics {
        let spawn = spawns.iter().find(|(id, _)| *id == s.id).map(|(_, p)| *p);
        let anchor = anchors.iter().find(|(id, _)| *id == s.id).map(|(_, p)| *p);
        {
            let map = Arc::make_mut(&mut world.map);
            unlink_lines(map, &s.lines);
            // GZDoom `TranslateToStartSpot`: verts -= (anchor - spawn) ≡ verts += (spawn - anchor).
            if let (Some(spawn), Some(anchor)) = (spawn, anchor) {
                let dx = spawn.x() - anchor.x();
                let dy = spawn.y() - anchor.y();
                for &vid in &s.vertices {
                    if let Some(v) = map.vertices.get_mut(vid.raw() as usize) {
                        v.pos = DVec2::new(v.pos.x() + dx, v.pos.y() + dy);
                    }
                }
            }
            refresh_line_geom(map, &s.lines);
            link_lines(map, &s.lines);
        }
        let start_spot = spawn.unwrap_or_else(|| poly_center(world, &s.vertices));
        let center = start_spot;
        let offsets = s
            .vertices
            .iter()
            .map(|vid| {
                world
                    .map
                    .vertices
                    .get(vid.raw() as usize)
                    .map_or(DVec2::ZERO, |v| {
                        DVec2::new(v.pos.x() - center.x(), v.pos.y() - center.y())
                    })
            })
            .collect();
        world.polyobjs.push(PolyobjDynamic {
            id: s.id,
            vertices: s.vertices,
            offsets,
            lines: s.lines,
            mirror: s.mirror,
            crush: s.crush,
            center,
            start_spot,
            angle: 0.0,
            motion: None,
            door: None,
            blocked: false,
        });
    }
}

fn poly_center(world: &World, verts: &[VertexId]) -> DVec2 {
    if verts.is_empty() {
        return DVec2::ZERO;
    }
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut n = 0.0;
    for &vid in verts {
        if let Some(v) = world.map.vertices.get(vid.raw() as usize) {
            sx += v.pos.x();
            sy += v.pos.y();
            n += 1.0;
        }
    }
    if n <= 0.0 {
        DVec2::ZERO
    } else {
        DVec2::new(sx / n, sy / n)
    }
}

fn find_poly_mut(world: &mut World, id: i32) -> Option<&mut PolyobjDynamic> {
    world.polyobjs.iter_mut().find(|p| p.id == id)
}

fn mirror_of(world: &World, id: i32) -> i32 {
    world
        .polyobjs
        .iter()
        .find(|p| p.id == id)
        .map_or(0, |p| p.mirror)
}

/// Hexen byte angle (0–255) → unit vector (east = 0).
fn byte_angle_dir(byte: i32) -> DVec2 {
    let a = f64::from(byte.rem_euclid(256)) * RAD_PER_BYTE;
    DVec2::new(dmath::cos(a), dmath::sin(a))
}

/// Start `motion` on `id` and the mirrored opposite on `mirror` (if any).
/// When `override_busy` is false (plain Move/Rotate), refuse if already moving.
fn start_motion(world: &mut World, id: i32, motion: PolyMotion, override_busy: bool) -> i32 {
    let mirror = mirror_of(world, id);
    let Some(po) = find_poly_mut(world, id) else {
        return 0;
    };
    if !override_busy && (po.motion.is_some() || po.blocked) {
        return 0;
    }
    po.motion = Some(motion);
    po.door = None;
    po.blocked = false;
    if mirror != 0 && mirror != id {
        if let Some(m) = find_poly_mut(world, mirror) {
            if override_busy || (m.motion.is_none() && !m.blocked) {
                m.motion = Some(mirror_motion(motion));
                m.door = None;
                m.blocked = false;
            }
        }
    }
    1
}

fn mirror_motion(m: PolyMotion) -> PolyMotion {
    match m {
        PolyMotion::Move {
            dir,
            speed,
            remaining,
        } => PolyMotion::Move {
            // Mirror slides opposite (angle + 180°).
            dir: DVec2::new(-dir.x(), -dir.y()),
            speed,
            remaining,
        },
        PolyMotion::Rotate { omega, remaining } => PolyMotion::Rotate {
            omega: -omega,
            remaining: -remaining,
        },
        PolyMotion::Wait { tics, then } => PolyMotion::Wait {
            tics,
            then: then.reversed(),
        },
    }
}

/// `Polyobj_Move` (4): args = po, speed, angle, dist.
pub fn polyobj_move(world: &mut World, args: &[i32; 5]) -> i32 {
    let id = args[0];
    let speed = hexen_speed(args[1]);
    let dist = args[3].max(0) as f64;
    if speed <= 0.0 || dist <= 0.0 {
        return 0;
    }
    let dir = byte_angle_dir(args[2]);
    start_motion(
        world,
        id,
        PolyMotion::Move {
            dir,
            speed,
            remaining: dist,
        },
        false,
    )
}

/// `Polyobj_OR_Move` (92): same as Move but overrides an in-progress move.
pub fn polyobj_or_move(world: &mut World, args: &[i32; 5]) -> i32 {
    let id = args[0];
    let speed = hexen_speed(args[1]);
    let dist = args[3].max(0) as f64;
    if speed <= 0.0 || dist <= 0.0 {
        return 0;
    }
    let dir = byte_angle_dir(args[2]);
    start_motion(
        world,
        id,
        PolyMotion::Move {
            dir,
            speed,
            remaining: dist,
        },
        true,
    )
}

/// `Polyobj_MoveTimes8` (6): distance × 8.
pub fn polyobj_move_times8(world: &mut World, args: &[i32; 5]) -> i32 {
    let mut a = *args;
    a[3] = a[3].saturating_mul(8);
    polyobj_move(world, &a)
}

/// `Polyobj_OR_MoveTimes8` (93).
pub fn polyobj_or_move_times8(world: &mut World, args: &[i32; 5]) -> i32 {
    let mut a = *args;
    a[3] = a[3].saturating_mul(8);
    polyobj_or_move(world, &a)
}

/// `Polyobj_MoveTo` (88) / `OR_MoveTo` (94): args = po, speed, destx, desty (map units).
pub fn polyobj_move_to(world: &mut World, args: &[i32; 5], override_busy: bool) -> i32 {
    let id = args[0];
    let speed = hexen_speed(args[1]);
    if speed <= 0.0 {
        return 0;
    }
    let dest = DVec2::new(f64::from(args[2]), f64::from(args[3]));
    let Some(po) = world.polyobjs.iter().find(|p| p.id == id) else {
        return 0;
    };
    let dx = dest.x() - po.start_spot.x();
    let dy = dest.y() - po.start_spot.y();
    let dist = dmath::sqrt(dx * dx + dy * dy);
    if dist <= 1e-9 {
        return 0;
    }
    let dir = DVec2::new(dx / dist, dy / dist);
    start_motion(
        world,
        id,
        PolyMotion::Move {
            dir,
            speed,
            remaining: dist,
        },
        override_busy,
    )
}

/// `Polyobj_MoveToSpot` (86) / `OR_MoveToSpot` (59): args = po, speed, tid.
pub fn polyobj_move_to_spot(world: &mut World, args: &[i32; 5], override_busy: bool) -> i32 {
    let tid = args[2];
    let Some(aid) = world.find_by_tid(tid) else {
        return 0;
    };
    let Some(a) = world.actors.get(aid) else {
        return 0;
    };
    let mapped = [
        args[0],
        args[1],
        a.pos.x().round() as i32,
        a.pos.y().round() as i32,
        0,
    ];
    polyobj_move_to(world, &mapped, override_busy)
}

/// `Polyobj_RotateLeft` (2) / `RotateRight` (3): args = po, speed, angle(bytes).
pub fn polyobj_rotate(world: &mut World, args: &[i32; 5], left: bool) -> i32 {
    polyobj_rotate_ex(world, args, left, false)
}

/// `Polyobj_OR_RotateLeft` (90) / `OR_RotateRight` (91).
pub fn polyobj_or_rotate(world: &mut World, args: &[i32; 5], left: bool) -> i32 {
    polyobj_rotate_ex(world, args, left, true)
}

/// GZDoom `EV_RotatePoly`: speed is eighths of a byte angle per tic; angle `0`
/// is a full turn and `255` rotates forever.
fn polyobj_rotate_ex(world: &mut World, args: &[i32; 5], left: bool, override_busy: bool) -> i32 {
    let id = args[0];
    let speed_bytes = f64::from(args[1].abs().max(1)) / 8.0;
    let turn_bytes = match args[2].abs() {
        0 => 256.0,
        255 => f64::INFINITY,
        n => f64::from(n),
    };
    let sign = if left { 1.0 } else { -1.0 };
    let omega = speed_bytes * RAD_PER_BYTE * sign;
    let remaining = turn_bytes * RAD_PER_BYTE * sign;
    start_motion(
        world,
        id,
        PolyMotion::Rotate { omega, remaining },
        override_busy,
    )
}

/// `Polyobj_DoorSwing` (7): args = po, speed (1/8 byte per tic), angle(bytes),
/// delay. Room extension: a negative angle swings clockwise.
pub fn polyobj_door_swing(world: &mut World, args: &[i32; 5]) -> i32 {
    let speed_bytes = f64::from(args[1].abs().max(1)) / 8.0;
    let turn = f64::from(args[2]);
    if turn == 0.0 {
        return 0;
    }
    let sign = turn.signum();
    let open = PolyMotionResume::Rotate {
        omega: speed_bytes * RAD_PER_BYTE * sign,
        remaining: turn * RAD_PER_BYTE,
    };
    start_door(world, args[0], open, args[3])
}

/// `Polyobj_DoorSlide` (8): args = po, speed, angle, distance, delay.
pub fn polyobj_door_slide(world: &mut World, args: &[i32; 5]) -> i32 {
    let speed = hexen_speed(args[1]);
    let dist = args[3].max(0) as f64;
    if speed <= 0.0 || dist <= 0.0 {
        return 0;
    }
    let open = PolyMotionResume::Move {
        dir: byte_angle_dir(args[2]),
        speed,
        remaining: dist,
    };
    start_door(world, args[0], open, args[4])
}

/// GZDoom `EV_OpenPolyDoor`: a moving (or opened-for-good) door ignores the
/// trigger; the mirror runs the opposite leg.
fn start_door(world: &mut World, id: i32, open: PolyMotionResume, delay: i32) -> i32 {
    let mirror = mirror_of(world, id);
    let mut started = 0;
    for (pid, leg) in [(id, open), (mirror, open.reversed())] {
        if pid == 0 || (started == 1 && pid == id) {
            continue;
        }
        let Some(po) = find_poly_mut(world, pid) else {
            continue;
        };
        if po.motion.is_some() || po.blocked {
            break;
        }
        po.motion = Some(leg.into_motion());
        po.door = Some(PolyDoor {
            open: leg,
            delay,
            closing: false,
        });
        started = 1;
    }
    started
}

/// `Polyobj_Stop` (87).
pub fn polyobj_stop(world: &mut World, args: &[i32; 5]) -> i32 {
    let id = args[0];
    let mirror = mirror_of(world, id);
    let mut n = 0;
    for po in &mut world.polyobjs {
        if po.id == id || (mirror != 0 && po.id == mirror) {
            po.motion = None;
            po.door = None;
            n = 1;
        }
    }
    n
}

fn hexen_speed(arg: i32) -> f64 {
    // Same as sector movers: arg is eighths of a map-unit per tic.
    (arg as f64) / 8.0
}

/// Advance all active poly movers one tic.
pub fn tick_polyobjs(world: &mut World) {
    let n = world.polyobjs.len();
    for i in 0..n {
        let motion = world.polyobjs[i].motion;
        let Some(m) = motion else {
            continue;
        };
        match m {
            PolyMotion::Wait { tics, then } => {
                if tics <= 1 {
                    world.polyobjs[i].motion = Some(then.into_motion());
                } else {
                    world.polyobjs[i].motion = Some(PolyMotion::Wait {
                        tics: tics - 1,
                        then,
                    });
                }
            }
            PolyMotion::Move {
                dir,
                speed,
                remaining,
            } => {
                let step = speed.min(remaining);
                if step <= 0.0 {
                    finish_leg(world, i);
                    continue;
                }
                let dx = dir.x() * step;
                let dy = dir.y() * step;
                translate_poly(world, i, dx, dy);
                if !resolve_poly_block(world, i) {
                    // GZDoom `CheckMobjBlocking`: undo; retry next tic.
                    translate_poly(world, i, -dx, -dy);
                    blocked_leg(world, i);
                    continue;
                }
                let left = remaining - step;
                if left <= 1e-9 {
                    finish_leg(world, i);
                } else {
                    world.polyobjs[i].motion = Some(PolyMotion::Move {
                        dir,
                        speed,
                        remaining: left,
                    });
                }
            }
            PolyMotion::Rotate { omega, remaining } => {
                let step = if remaining.abs() < omega.abs() {
                    remaining
                } else {
                    omega
                };
                if step.abs() <= 1e-12 {
                    finish_leg(world, i);
                    continue;
                }
                rotate_poly(world, i, step);
                if !resolve_poly_block(world, i) {
                    rotate_poly(world, i, -step);
                    blocked_leg(world, i);
                    continue;
                }
                let left = remaining - step;
                if left.abs() <= 1e-9 {
                    finish_leg(world, i);
                } else {
                    world.polyobjs[i].motion = Some(PolyMotion::Rotate {
                        omega,
                        remaining: left,
                    });
                }
            }
        }
    }
}

/// A closing door that hits something opens back up (GZDoom `DPolyDoor::Tick`);
/// anything else keeps trying.
fn blocked_leg(world: &mut World, index: usize) {
    let po = &mut world.polyobjs[index];
    let Some(door) = po.door.as_mut() else {
        return;
    };
    if !door.closing || po.crush {
        return;
    }
    door.closing = false;
    po.motion = match (po.motion, door.open) {
        (
            Some(PolyMotion::Move { remaining, .. }),
            PolyMotionResume::Move {
                dir,
                speed,
                remaining: total,
            },
        ) => Some(PolyMotion::Move {
            dir,
            speed,
            remaining: (total - remaining).max(0.0),
        }),
        (
            Some(PolyMotion::Rotate { remaining, .. }),
            PolyMotionResume::Rotate {
                omega,
                remaining: total,
            },
        ) => Some(PolyMotion::Rotate {
            omega,
            remaining: total.signum() * (total.abs() - remaining.abs()).max(0.0),
        }),
        (m, _) => m,
    };
}

/// Leg done → door wait + close (or stay open for good), else idle.
fn finish_leg(world: &mut World, index: usize) {
    let po = &mut world.polyobjs[index];
    po.motion = None;
    match po.door {
        Some(ref mut door) if !door.closing => {
            if door.delay >= 0 {
                door.closing = true;
                po.motion = Some(PolyMotion::Wait {
                    tics: door.delay,
                    then: door.open.reversed(),
                });
            } else {
                po.door = None;
                po.blocked = true;
            }
        }
        _ => po.door = None,
    }
}

/// Returns `true` if the move may stand (clear, or crushing through victims).
fn resolve_poly_block(world: &mut World, index: usize) -> bool {
    let crush = world.polyobjs[index].crush;
    let victims = actors_hit_by_poly(world, index);
    if victims.is_empty() {
        return true;
    }
    if !crush
        && shove_off_poly(world, index, &victims)
        && actors_hit_by_poly(world, index).is_empty()
    {
        return true;
    }
    if crush {
        for id in victims {
            let shootable = world
                .actors
                .get(id)
                .is_some_and(|a| (a.flags & FLAG_SHOOTABLE) != 0 && a.health > 0);
            if shootable {
                crate::damage::damage_mobj(world, id, None, None, POLY_CRUSH_DAMAGE);
            }
        }
        return true;
    }
    false
}

/// Slide actors out of a swinging door so the panel can move.
fn shove_off_poly(world: &mut World, index: usize, victims: &[crate::actor::ActorId]) -> bool {
    let center = world.polyobjs[index].center;
    for &id in victims {
        let Some(a) = world.actors.get(id) else {
            return false;
        };
        let (px, py, pz, r) = (a.pos.x(), a.pos.y(), a.pos.z(), a.radius.max(8.0));
        let vx = a.vel.x();
        let vy = a.vel.y();
        let vlen = (vx * vx + vy * vy).sqrt();
        let (fx, fy) = if vlen > 0.2 {
            (vx / vlen, vy / vlen)
        } else {
            let ang = a.angle.radians();
            (room_core::dmath::cos(ang), room_core::dmath::sin(ang))
        };
        let dx = px - center.x();
        let dy = py - center.y();
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let dirs = [(fx, fy), (-fy, fx), (fy, -fx), (dx / len, dy / len)];
        let mut clear = false;
        let step = r + 16.0;
        for (ux, uy) in dirs {
            let mut x = px;
            let mut y = py;
            for _ in 0..2 {
                if !actors_hit_by_poly(world, index).contains(&id) {
                    clear = true;
                    break;
                }
                let dest = room_core::DVec3::new(x + ux * step, y + uy * step, pz);
                if !crate::physics::try_move(world, id, dest) {
                    break;
                }
                x = dest.x();
                y = dest.y();
            }
            if clear || !actors_hit_by_poly(world, index).contains(&id) {
                clear = true;
                break;
            }
        }
        if !clear {
            return false;
        }
    }
    true
}

/// Solid actors whose AABB crosses any linedef of the poly (Doom square clip).
fn actors_hit_by_poly(world: &World, index: usize) -> Vec<crate::actor::ActorId> {
    let lines = world.polyobjs[index].lines.clone();
    let mut hit = Vec::new();
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || (a.flags & FLAG_SOLID) == 0 {
            continue;
        }
        let r = a.radius;
        let px = a.pos.x();
        let py = a.pos.y();
        for &lid in &lines {
            let Some(line) = world.map.lines.get(lid.raw() as usize) else {
                continue;
            };
            let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
                continue;
            };
            let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
                continue;
            };
            if aabb_hits_seg(px, py, r, v1.pos.x(), v1.pos.y(), v2.pos.x(), v2.pos.y()) {
                hit.push(id);
                break;
            }
        }
    }
    hit
}

/// Actor square vs infinite-line crossing test (same geometry as `PIT_CheckLine`).
fn aabb_hits_seg(px: f64, py: f64, radius: f64, ax: f64, ay: f64, bx: f64, by: f64) -> bool {
    let top = py + radius;
    let bottom = py - radius;
    let left = px - radius;
    let right = px + radius;
    let ld_left = ax.min(bx);
    let ld_right = ax.max(bx);
    let ld_bottom = ay.min(by);
    let ld_top = ay.max(by);
    if right <= ld_left || left >= ld_right || top <= ld_bottom || bottom >= ld_top {
        return false;
    }
    // Box straddles the infinite line if corners disagree on side.
    let dx = bx - ax;
    let dy = by - ay;
    let side = |x: f64, y: f64| -> i32 {
        let left_c = dy * (x - ax);
        let right_c = (y - ay) * dx;
        i32::from(right_c >= left_c)
    };
    let s1 = side(left, top);
    if side(right, top) != s1 || side(left, bottom) != s1 || side(right, bottom) != s1 {
        return true;
    }
    false
}

/// Slide map vertices and refresh the linedefs that use them (Duke `SE 20`).
pub(crate) fn translate_vertices(
    world: &mut World,
    verts: &[VertexId],
    lines: &[LineId],
    dx: f64,
    dy: f64,
) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    let map = Arc::make_mut(&mut world.map);
    unlink_lines(map, lines);
    for &vid in verts {
        if let Some(v) = map.vertices.get_mut(vid.raw() as usize) {
            v.pos = DVec2::new(v.pos.x() + dx, v.pos.y() + dy);
        }
    }
    refresh_line_geom(map, lines);
    link_lines(map, lines);
}

fn translate_poly(world: &mut World, index: usize, dx: f64, dy: f64) {
    let po = &mut world.polyobjs[index];
    po.center = DVec2::new(po.center.x() + dx, po.center.y() + dy);
    po.start_spot = DVec2::new(po.start_spot.x() + dx, po.start_spot.y() + dy);
    place_poly(world, index);
}

fn rotate_poly(world: &mut World, index: usize, delta: f64) {
    let po = &mut world.polyobjs[index];
    po.angle = (po.angle + delta).rem_euclid(std::f64::consts::TAU);
    place_poly(world, index);
}

/// Put the poly's vertices at `center + rotate(offset, angle)` and relink its
/// lines into the blockmap.
fn place_poly(world: &mut World, index: usize) {
    let po = &world.polyobjs[index];
    let (center, angle) = (po.center, po.angle);
    let (s, c) = (dmath::sin(angle), dmath::cos(angle));
    let placed: Vec<(VertexId, DVec2)> = po
        .vertices
        .iter()
        .zip(&po.offsets)
        .map(|(&vid, o)| {
            let p = DVec2::new(
                center.x() + o.x() * c - o.y() * s,
                center.y() + o.x() * s + o.y() * c,
            );
            (vid, p)
        })
        .collect();
    let lines = po.lines.clone();
    let map = Arc::make_mut(&mut world.map);
    unlink_lines(map, &lines);
    for (vid, p) in placed {
        if let Some(v) = map.vertices.get_mut(vid.raw() as usize) {
            v.pos = p;
        }
    }
    refresh_line_geom(map, &lines);
    link_lines(map, &lines);
}

/// Blockmap cells covered by the bounding box of `line`.
fn line_cells(map: &room_map::MapGeometry, lid: LineId) -> Option<(u16, u16, u16, u16)> {
    let line = map.lines.get(lid.raw() as usize)?;
    let a = map.vertices.get(line.v1.raw() as usize)?.pos;
    let b = map.vertices.get(line.v2.raw() as usize)?.pos;
    let bm = &map.blockmap;
    if bm.columns == 0 || bm.rows == 0 {
        return None;
    }
    let (x0, y0) = crate::physics::block_coords(bm, a.x().min(b.x()), a.y().min(b.y()));
    let (x1, y1) = crate::physics::block_coords(bm, a.x().max(b.x()), a.y().max(b.y()));
    Some((x0, y0, x1, y1))
}

fn unlink_lines(map: &mut room_map::MapGeometry, lines: &[LineId]) {
    for &lid in lines {
        let Some((x0, y0, x1, y1)) = line_cells(map, lid) else {
            continue;
        };
        let cols = usize::from(map.blockmap.columns);
        for by in y0..=y1 {
            for bx in x0..=x1 {
                if let Some(cell) = map
                    .blockmap
                    .blocks
                    .get_mut(usize::from(by) * cols + usize::from(bx))
                {
                    cell.retain(|&l| l != lid);
                }
            }
        }
    }
}

fn link_lines(map: &mut room_map::MapGeometry, lines: &[LineId]) {
    for &lid in lines {
        let Some((x0, y0, x1, y1)) = line_cells(map, lid) else {
            continue;
        };
        let cols = usize::from(map.blockmap.columns);
        for by in y0..=y1 {
            for bx in x0..=x1 {
                if let Some(cell) = map
                    .blockmap
                    .blocks
                    .get_mut(usize::from(by) * cols + usize::from(bx))
                {
                    if !cell.contains(&lid) {
                        cell.push(lid);
                    }
                }
            }
        }
    }
}

fn refresh_line_geom(map: &mut room_map::MapGeometry, lines: &[LineId]) {
    for &lid in lines {
        let Some(line) = map.lines.get(lid.raw() as usize) else {
            continue;
        };
        let (v1, v2) = (line.v1, line.v2);
        let Some(a) = map.vertices.get(v1.raw() as usize) else {
            continue;
        };
        let Some(b) = map.vertices.get(v2.raw() as usize) else {
            continue;
        };
        let delta = DVec2::new(b.pos.x() - a.pos.x(), b.pos.y() - a.pos.y());
        if let Some(line) = map.lines.get_mut(lid.raw() as usize) {
            line.delta = delta;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::specials::{execute_special, num};
    use crate::world::World;
    use room_map::{
        MapFormat, RawLinedef, RawSector, RawSidedef, RawThing, RawVertex, link_geometry,
    };
    use room_nodebuild::apply_nodes;

    fn poly_square_map() -> room_map::MapGeometry {
        let vertices = vec![
            RawVertex { x: 0, y: 0 },
            RawVertex { x: 64, y: 0 },
            RawVertex { x: 64, y: 64 },
            RawVertex { x: 0, y: 64 },
        ];
        let linedefs = (0u16..4)
            .map(|i| RawLinedef {
                v1: i,
                v2: (i + 1) % 4,
                flags: 1,
                special: if i == 0 { 1 } else { 0 }, // Polyobj_StartLine
                tag: 0,
                side_front: i,
                side_back: 0xFFFF,
                args: if i == 0 { [7, 0, 0, 0, 0] } else { [0; 5] },
                hexen: true,
                activation: 0,
                arg0_str: String::new(),
                midtex3d: false,
                midtex3d_impassible: false,
                check_switch_range: false,
                first_side_only: false,
                block_floaters: false,
                lock_number: 0,
                block_projectiles: false,
                block_sight: false,
                block_hitscan: false,
                extra_ids: Default::default(),
                alpha: 1.0,
                mid_additive: false,
                health: 0,
                health_group: 0,
                automap_style: 0,
                clip_midtex: false,
                wrap_midtex: false,
            })
            .collect();
        let sidedefs = (0u16..4)
            .map(|_| RawSidedef {
                x_offset: 0,
                y_offset: 0,
                upper: "-".into(),
                lower: "-".into(),
                middle: "WALL".into(),
                sector: 0,
                light: i16::MIN,
                light_absolute: false,
                light_top: i16::MIN,
                light_mid: i16::MIN,
                light_bottom: i16::MIN,
                light_absolute_top: false,
                light_absolute_mid: false,
                light_absolute_bottom: false,
                scale_x: 1.0,
                scale_y: 1.0,
                scale_x_top: 1.0,
                scale_y_top: 1.0,
                scale_x_bottom: 1.0,
                scale_y_bottom: 1.0,
                x_offset_top: 0,
                y_offset_top: 0,
                x_offset_bottom: 0,
                y_offset_bottom: 0,
                no_fake_contrast: false,
                smooth_lighting: false,
                scroll_x: 0.0,
                scroll_y: 0.0,
                clip_midtex: false,
                wrap_midtex: false,
                no_decals: false,
                alpha: None,
                block_rendering: false,
            })
            .collect();
        let sectors = vec![RawSector {
            floor_height: 0,
            ceiling_height: 128,
            floor_pic: "FLAT".into(),
            ceiling_pic: "FLAT".into(),
            light: 160,
            special: 0,
            tag: 0,
            floor_plane: None,
            ceiling_plane: None,
            extra_ids: Vec::new(),
            floor_x_pan: 0.0,
            floor_y_pan: 0.0,
            ceiling_x_pan: 0.0,
            ceiling_y_pan: 0.0,
            floor_rotation: 0.0,
            ceiling_rotation: 0.0,
            floor_x_scale: 1.0,
            floor_y_scale: 1.0,
            ceiling_x_scale: 1.0,
            ceiling_y_scale: 1.0,
            gravity: -1.0,
            friction: 0,
            damage_amount: 0,
            damage_type: String::new(),
            damage_interval: 0,
            leakiness: 0,
            silent: false,
            sound_sequence: String::new(),
            waterzone: false,
            drop_actors: true,
            no_falling_damage: false,
            light_color: -1,
            fade_color: -1,
            fog_density: 0,
            desaturation: 0,
            color_floor: -1,
            color_ceiling: -1,
            color_walltop: -1,
            color_wallbottom: -1,
            color_sprites: -1,
            health_floor: 0,
            health_ceiling: 0,
            health_3d: 0,
            floor_alpha: 1.0,
            ceiling_alpha: 1.0,
            floor_additive: false,
            ceiling_additive: false,
        }];
        let things = vec![
            RawThing {
                tid: 0,
                x: 32,
                y: 32,
                z: 0,
                angle: 7,
                type_id: 9300, // anchor
                flags: 0,
                special: 0,
                args: [0; 5],
                hexen: false,
                friendly_see_blocks: -1,
                health: 0,
                score: -2147483648,
                gravity: -1.0,
                scale_x: 0.0,
                scale_y: 0.0,
                alpha: -1.0,
                pitch: -32768,
                roll: -32768,
                fill_color: -1,
                render_style: String::new(),
                float_bob_phase: -1,
                count_secret: false,
                no_count: false,
                arg0_str: String::new(),
                conversation: 0,
                accuracy: i32::MIN,
                stamina: i32::MIN,
                species: String::new(),
                melee_range: -1.0,
                view_height: -1.0,
                mass: i32::MIN,
                speed: -1.0,
                friction: -1.0,
                damage_factor: -1.0,
                max_step_height: -1.0,
                max_dropoff_height: -1.0,
                float_speed: -1.0,
                fast_speed: -1.0,
                jump_z: -1.0,
                attack_z_offset: -1.0,
            },
            RawThing {
                tid: 0,
                x: 96,
                y: 32,
                z: 0,
                angle: 7,
                type_id: 9301, // spawn
                flags: 0,
                special: 0,
                args: [0; 5],
                hexen: false,
                friendly_see_blocks: -1,
                health: 0,
                score: -2147483648,
                gravity: -1.0,
                scale_x: 0.0,
                scale_y: 0.0,
                alpha: -1.0,
                pitch: -32768,
                roll: -32768,
                fill_color: -1,
                render_style: String::new(),
                float_bob_phase: -1,
                count_secret: false,
                no_count: false,
                arg0_str: String::new(),
                conversation: 0,
                accuracy: i32::MIN,
                stamina: i32::MIN,
                species: String::new(),
                melee_range: -1.0,
                view_height: -1.0,
                mass: i32::MIN,
                speed: -1.0,
                friction: -1.0,
                damage_factor: -1.0,
                max_step_height: -1.0,
                max_dropoff_height: -1.0,
                float_speed: -1.0,
                fast_speed: -1.0,
                jump_z: -1.0,
                attack_z_offset: -1.0,
            },
            RawThing {
                tid: 0,
                x: 32,
                y: 32,
                z: 0,
                angle: 0,
                type_id: 1, // player start
                flags: 0,
                special: 0,
                args: [0; 5],
                hexen: false,
                friendly_see_blocks: -1,
                health: 0,
                score: -2147483648,
                gravity: -1.0,
                scale_x: 0.0,
                scale_y: 0.0,
                alpha: -1.0,
                pitch: -32768,
                roll: -32768,
                fill_color: -1,
                render_style: String::new(),
                float_bob_phase: -1,
                count_secret: false,
                no_count: false,
                arg0_str: String::new(),
                conversation: 0,
                accuracy: i32::MIN,
                stamina: i32::MIN,
                species: String::new(),
                melee_range: -1.0,
                view_height: -1.0,
                mass: i32::MIN,
                speed: -1.0,
                friction: -1.0,
                damage_factor: -1.0,
                max_step_height: -1.0,
                max_dropoff_height: -1.0,
                float_speed: -1.0,
                fast_speed: -1.0,
                jump_z: -1.0,
                attack_z_offset: -1.0,
            },
        ];
        link_geometry(
            "MAP01",
            MapFormat::Hexen,
            vertices,
            linedefs,
            sidedefs,
            sectors,
            things,
            [0; 16],
        )
    }

    #[test]
    fn polyobj_move_translates_vertices() {
        let mut map = poly_square_map();
        apply_nodes(&mut map);
        assert_eq!(map.setup.polyobjects.len(), 1);
        assert_eq!(map.setup.polyobjects[0].id, 7);
        // Anchor (32,32) → spawn (96,32): verts shift +64 on X at init.
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.polyobjs.len(), 1);
        assert_eq!(w.polyobjs[0].id, 7);
        assert!((w.polyobjs[0].start_spot.x() - 96.0).abs() < 1e-6);
        let v0 = w.polyobjs[0].vertices[0];
        let x_after_spawn = w.map.vertices[v0.raw() as usize].pos.x();
        // Original square verts were at x=0/64; after +64 relocate, expect 64/128.
        assert!(
            (x_after_spawn - 64.0).abs() < 1e-6 || (x_after_spawn - 128.0).abs() < 1e-6,
            "spawn relocate x={x_after_spawn}"
        );
        let x0 = x_after_spawn;
        // Move east: angle 0, speed 8 → 1 mu/tic, dist 16 → 16 tics.
        assert_eq!(
            execute_special(&mut w, num::POLYOBJ_MOVE, &[7, 8, 0, 16, 0], None, None),
            1
        );
        for _ in 0..16 {
            tick_polyobjs(&mut w);
        }
        let x1 = w.map.vertices[v0.raw() as usize].pos.x();
        assert!(
            (x1 - (x0 + 16.0)).abs() < 1e-6,
            "x0={x0} x1={x1} expected +16"
        );
        assert!(w.polyobjs[0].motion.is_none());
        assert!((w.polyobjs[0].start_spot.x() - 112.0).abs() < 1e-6);
    }

    #[test]
    fn polyobj_move_stops_on_solid_blocker() {
        let mut map = poly_square_map();
        apply_nodes(&mut map);
        let ir = room_decorate::parse_decorate(
            r#"
actor Blocker : Actor 9001
{
  Radius 20
  Height 56
  +SOLID
  States { Spawn: TROO A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let blk = w.classes.find_doomednum(9001).unwrap();
        // East of relocated poly (right edge ~128); Move east will collide.
        w.spawn_at(
            blk,
            room_core::DVec3::new(140.0, 32.0, 0.0),
            room_core::Angle::ZERO,
        )
        .unwrap();
        assert!(!w.polyobjs[0].crush);
        let v0 = w.polyobjs[0].vertices[0];
        let x0 = w.map.vertices[v0.raw() as usize].pos.x();
        assert_eq!(
            execute_special(&mut w, num::POLYOBJ_MOVE, &[7, 8, 0, 64, 0], None, None),
            1
        );
        for _ in 0..64 {
            tick_polyobjs(&mut w);
        }
        assert!(w.polyobjs[0].motion.is_some(), "blocked poly keeps trying");
        let x1 = w.map.vertices[v0.raw() as usize].pos.x();
        assert!(
            (x1 - x0).abs() < 32.0,
            "should not travel full 64u into blocker: Δ={}",
            x1 - x0
        );
    }

    #[test]
    fn polyobj_door_swing_opens_waits_closes() {
        let mut map = poly_square_map();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let ang0 = w.polyobjs[0].angle;
        // speed 256/8 = 32 bytes/tic, turn 64 bytes (=90°), delay 2.
        assert_eq!(
            execute_special(
                &mut w,
                num::POLYOBJ_DOOR_SWING,
                &[7, 256, 64, 2, 0],
                None,
                None
            ),
            1
        );
        // Open: 64/32 = 2 tics.
        tick_polyobjs(&mut w);
        tick_polyobjs(&mut w);
        assert!(w.polyobjs[0].angle > ang0 + 0.5);
        // Wait 2 tics.
        match w.polyobjs[0].motion {
            Some(PolyMotion::Wait { tics, .. }) => assert_eq!(tics, 2),
            other => panic!("expected Wait after open, got {other:?}"),
        }
        tick_polyobjs(&mut w);
        tick_polyobjs(&mut w);
        // Close: 2 tics.
        tick_polyobjs(&mut w);
        tick_polyobjs(&mut w);
        assert!(
            (w.polyobjs[0].angle - ang0).abs() < 1e-6,
            "should return to start ang={}",
            w.polyobjs[0].angle
        );
        assert!(w.polyobjs[0].motion.is_none());
    }

    #[test]
    fn polyobj_mirror_rotates_opposite() {
        let mut map = poly_square_map();
        // Second poly id 8 mirroring 7 — reuse same verts is OK for motion sync test.
        map.setup.polyobjects.push(room_map::PolyobjStatic {
            id: 8,
            vertices: map.setup.polyobjects[0].vertices.clone(),
            lines: map.setup.polyobjects[0].lines.clone(),
            mirror: 0,
            crush: false,
        });
        map.setup.polyobjects[0].mirror = 8;
        apply_nodes(&mut map);
        // Re-run setup after mirror edit? apply_nodes may rebuild setup — set after.
        let mut w = World::from_map(map).unwrap();
        if w.polyobjs.len() < 2 {
            // apply_nodes/build_setup may have dropped the synthetic poly — inject.
            let base = w.polyobjs[0].clone();
            w.polyobjs[0].mirror = 8;
            w.polyobjs.push(PolyobjDynamic {
                id: 8,
                mirror: 0,
                motion: None,
                door: None,
                ..base
            });
            w.polyobjs[1].id = 8;
            w.polyobjs[1].mirror = 0;
        }
        assert_eq!(
            execute_special(
                &mut w,
                num::POLYOBJ_ROTATE_LEFT,
                &[7, 16, 16, 0, 0],
                None,
                None
            ),
            1
        );
        let a7 = w.polyobjs.iter().find(|p| p.id == 7).unwrap();
        let a8 = w.polyobjs.iter().find(|p| p.id == 8).unwrap();
        match (a7.motion, a8.motion) {
            (
                Some(PolyMotion::Rotate {
                    omega: o7,
                    remaining: r7,
                }),
                Some(PolyMotion::Rotate {
                    omega: o8,
                    remaining: r8,
                }),
            ) => {
                assert!((o7 + o8).abs() < 1e-12, "mirror omega opposite");
                assert!((r7 + r8).abs() < 1e-12, "mirror remaining opposite");
            }
            other => panic!("expected dual Rotate, got {other:?}"),
        }
    }

    #[test]
    fn polyobj_move_to_reaches_dest() {
        let mut map = poly_square_map();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let spot0 = w.polyobjs[0].start_spot;
        // Dest 16u east of current start spot; speed 8 → 1 mu/tic → 16 tics.
        let dest_x = (spot0.x() + 16.0).round() as i32;
        let dest_y = spot0.y().round() as i32;
        assert_eq!(
            execute_special(
                &mut w,
                num::POLYOBJ_MOVE_TO,
                &[7, 8, dest_x, dest_y, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..20 {
            tick_polyobjs(&mut w);
        }
        assert!(w.polyobjs[0].motion.is_none());
        assert!((w.polyobjs[0].start_spot.x() - f64::from(dest_x)).abs() < 1e-5);
        assert!((w.polyobjs[0].start_spot.y() - f64::from(dest_y)).abs() < 1e-5);
    }

    #[test]
    fn polyobj_door_ignores_retrigger_and_stays_open_on_negative_delay() {
        let mut map = poly_square_map();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let args = [7, 256, -64, -1, 0];
        assert_eq!(
            execute_special(&mut w, num::POLYOBJ_DOOR_SWING, &args, None, None),
            1
        );
        assert_eq!(
            execute_special(&mut w, num::POLYOBJ_DOOR_SWING, &args, None, None),
            0
        );
        for _ in 0..4 {
            tick_polyobjs(&mut w);
        }
        let p = &w.polyobjs[0];
        assert!(p.motion.is_none() && p.blocked);
        let quarter = std::f64::consts::FRAC_PI_2;
        assert!(
            (p.angle - (std::f64::consts::TAU - quarter)).abs() < 1e-6,
            "clockwise: {}",
            p.angle
        );
        assert_eq!(
            execute_special(&mut w, num::POLYOBJ_DOOR_SWING, &args, None, None),
            0
        );
    }

    #[test]
    fn polyobj_rotate_255_is_perpetual_and_eighth_speed() {
        let mut map = poly_square_map();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::POLYOBJ_ROTATE_LEFT,
                &[7, 64, 255, 0, 0],
                None,
                None
            ),
            1
        );
        let v0 = w.polyobjs[0].vertices[0];
        let start = w.map.vertices[v0.raw() as usize].pos;
        // 64/8 = 8 bytes/tic → a full turn in 32 tics.
        for _ in 0..32 * 10 {
            tick_polyobjs(&mut w);
        }
        assert!(w.polyobjs[0].motion.is_some());
        let end = w.map.vertices[v0.raw() as usize].pos;
        assert!((end.x() - start.x()).abs() < 1e-6 && (end.y() - start.y()).abs() < 1e-6);
    }

    #[test]
    fn polyobj_lines_follow_the_poly_in_the_blockmap() {
        let mut map = poly_square_map();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let lines = w.polyobjs[0].lines.clone();
        let line = lines[0];
        {
            let m = Arc::make_mut(&mut w.map);
            m.blockmap = room_map::Blockmap {
                origin_x: 0,
                origin_y: 0,
                columns: 8,
                rows: 1,
                blocks: vec![Vec::new(); 8],
            };
            link_lines(m, &lines);
        }
        let near = |w: &World, x: f64| {
            crate::physics::lines_near(&w.map, DVec2::new(x, 32.0), 4.0).contains(&line)
        };
        // Relocated to x 64..128 at init.
        assert!(near(&w, 96.0));
        assert_eq!(
            execute_special(
                &mut w,
                num::POLYOBJ_MOVE,
                &[7, 64 * 8, 0, 256, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..4 {
            tick_polyobjs(&mut w);
        }
        assert!(near(&w, 96.0 + 256.0), "poly line linked at its new spot");
        assert!(!near(&w, 96.0), "and unlinked from the old one");
    }
}
