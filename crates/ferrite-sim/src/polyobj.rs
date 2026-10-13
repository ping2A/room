//! Polyobject movers (`docs/06` §6, GZDoom `po_man.cpp`).
//!
//! Bootstrap: spawn from load-time [`ferrite_map::setup::PolyobjStatic`], then
//! `Polyobj_Move` / `Rotate*` / `DoorSwing` / `DoorSlide` / `Stop`, with mirror
//! sync and solid collide.

use crate::class::{FLAG_SHOOTABLE, FLAG_SOLID};
use crate::world::World;
use ferrite_core::{DVec2, LineId, VertexId};
use ferrite_map::setup::PolyobjStatic;
use std::sync::Arc;

/// Hexen/Doom crush amount when a crushing poly pins an actor.
const POLY_CRUSH_DAMAGE: i32 = 10;

/// Active polyobject instance.
#[derive(Clone, Debug)]
pub struct PolyobjDynamic {
    /// Tag / poly id.
    pub id: i32,
    /// Vertices owned by this poly (indices into `World::map.vertices`).
    pub vertices: Vec<VertexId>,
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
    /// Door open→close: delay tics after the open leg finishes.
    pub door_delay: i32,
    /// Door open→close: closing motion after `door_delay`.
    pub door_return: Option<PolyMotionResume>,
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
    /// Rotate by `omega` rad/tic until `|remaining|` hits 0.
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
        // GZDoom `TranslateToStartSpot`: verts -= (anchor - spawn) ≡ verts += (spawn - anchor).
        if let (Some(spawn), Some(anchor)) = (spawn, anchor) {
            let dx = spawn.x() - anchor.x();
            let dy = spawn.y() - anchor.y();
            if dx != 0.0 || dy != 0.0 {
                let verts = s.vertices.clone();
                let lines = s.lines.clone();
                {
                    let map = Arc::make_mut(&mut world.map);
                    for &vid in &verts {
                        if let Some(v) = map.vertices.get_mut(vid.raw() as usize) {
                            v.pos = DVec2::new(v.pos.x() + dx, v.pos.y() + dy);
                        }
                    }
                    refresh_line_geom(map, &lines);
                }
            }
        }
        let start_spot = spawn.unwrap_or_else(|| poly_center(world, &s.vertices));
        let center = start_spot;
        world.polyobjs.push(PolyobjDynamic {
            id: s.id,
            vertices: s.vertices,
            lines: s.lines,
            mirror: s.mirror,
            crush: s.crush,
            center,
            start_spot,
            angle: 0.0,
            motion: None,
            door_delay: 0,
            door_return: None,
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

/// Hexen byte angle (0–255) → unit vector (east = 0).
fn byte_angle_dir(byte: i32) -> DVec2 {
    let a = (byte.rem_euclid(256) as f64) * (std::f64::consts::TAU / 256.0);
    DVec2::new(a.cos(), a.sin())
}

/// Start `motion` on `id` and the mirrored opposite on `mirror` (if any).
/// When `override_busy` is false (plain Move/Rotate), refuse if already moving.
fn start_motion(world: &mut World, id: i32, motion: PolyMotion, override_busy: bool) -> i32 {
    let mirror = world
        .polyobjs
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.mirror)
        .unwrap_or(0);
    let Some(po) = find_poly_mut(world, id) else {
        return 0;
    };
    if !override_busy && po.motion.is_some() {
        return 0;
    }
    po.motion = Some(motion);
    if mirror != 0 && mirror != id {
        if let Some(m) = find_poly_mut(world, mirror) {
            if override_busy || m.motion.is_none() {
                m.motion = Some(mirror_motion(motion));
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
            then: match then {
                PolyMotionResume::Move {
                    dir,
                    speed,
                    remaining,
                } => PolyMotionResume::Move {
                    dir: DVec2::new(-dir.x(), -dir.y()),
                    speed,
                    remaining,
                },
                PolyMotionResume::Rotate { omega, remaining } => PolyMotionResume::Rotate {
                    omega: -omega,
                    remaining: -remaining,
                },
            },
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
    let dist = ferrite_core::dmath::sqrt(dx * dx + dy * dy);
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

fn polyobj_rotate_ex(world: &mut World, args: &[i32; 5], left: bool, override_busy: bool) -> i32 {
    let id = args[0];
    // Hexen: speed is bytes/tic; angle arg is total bytes to turn.
    let speed_bytes = args[1].abs().max(1) as f64;
    let turn_bytes = args[2].abs() as f64;
    if turn_bytes <= 0.0 {
        return 0;
    }
    let rad_per_byte = std::f64::consts::TAU / 256.0;
    let sign = if left { 1.0 } else { -1.0 };
    let omega = speed_bytes * rad_per_byte * sign;
    let remaining = turn_bytes * rad_per_byte * sign;
    start_motion(
        world,
        id,
        PolyMotion::Rotate { omega, remaining },
        override_busy,
    )
}

/// `Polyobj_DoorSwing` (7): args = po, speed, angle(bytes), delay.
pub fn polyobj_door_swing(world: &mut World, args: &[i32; 5]) -> i32 {
    let id = args[0];
    let speed_bytes = args[1].abs().max(1) as f64;
    let turn_bytes = args[2].abs() as f64;
    let delay = args[3].max(0);
    if turn_bytes <= 0.0 {
        return 0;
    }
    let rad_per_byte = std::f64::consts::TAU / 256.0;
    let omega = speed_bytes * rad_per_byte;
    let remaining = turn_bytes * rad_per_byte;
    // Open left, wait, close right (opposite omega).
    let open = PolyMotion::Rotate { omega, remaining };
    let close = PolyMotionResume::Rotate {
        omega: -omega,
        remaining: -remaining,
    };
    // Chain: run open; when done, insert Wait then close via tick helper.
    let ok = start_motion(world, id, open, true);
    if ok != 0 {
        attach_door_return(world, id, delay, close);
    }
    ok
}

/// `Polyobj_DoorSlide` (8): args = po, speed, angle, distance, delay.
pub fn polyobj_door_slide(world: &mut World, args: &[i32; 5]) -> i32 {
    let id = args[0];
    let speed = hexen_speed(args[1]);
    let dist = args[3].max(0) as f64;
    let delay = args[4].max(0);
    if speed <= 0.0 || dist <= 0.0 {
        return 0;
    }
    let dir = byte_angle_dir(args[2]);
    let open = PolyMotion::Move {
        dir,
        speed,
        remaining: dist,
    };
    let close = PolyMotionResume::Move {
        dir: DVec2::new(-dir.x(), -dir.y()),
        speed,
        remaining: dist,
    };
    let ok = start_motion(world, id, open, true);
    if ok != 0 {
        attach_door_return(world, id, delay, close);
    }
    ok
}

/// After the current open leg finishes, wait `delay` then run `close` (and mirror).
fn attach_door_return(world: &mut World, id: i32, delay: i32, close: PolyMotionResume) {
    set_door_return(world, id, delay, close);
}

fn set_door_return(world: &mut World, id: i32, delay: i32, close: PolyMotionResume) {
    let mirror = world
        .polyobjs
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.mirror)
        .unwrap_or(0);
    if let Some(po) = world.polyobjs.iter_mut().find(|p| p.id == id) {
        po.door_delay = delay;
        po.door_return = Some(close);
    }
    if mirror != 0 && mirror != id {
        if let Some(m) = world.polyobjs.iter_mut().find(|p| p.id == mirror) {
            m.door_delay = delay;
            m.door_return = Some(match close {
                PolyMotionResume::Move {
                    dir,
                    speed,
                    remaining,
                } => PolyMotionResume::Move {
                    dir: DVec2::new(-dir.x(), -dir.y()),
                    speed,
                    remaining,
                },
                PolyMotionResume::Rotate { omega, remaining } => PolyMotionResume::Rotate {
                    omega: -omega,
                    remaining: -remaining,
                },
            });
        }
    }
}

/// `Polyobj_Stop` (87).
pub fn polyobj_stop(world: &mut World, args: &[i32; 5]) -> i32 {
    let id = args[0];
    let mirror = world
        .polyobjs
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.mirror)
        .unwrap_or(0);
    let mut n = 0;
    for po in &mut world.polyobjs {
        if po.id == id || (mirror != 0 && po.id == mirror) {
            po.motion = None;
            po.door_return = None;
            po.door_delay = 0;
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
                    // Non-crush block: undo and stop (GZDoom `CheckMobjBlocking`).
                    translate_poly(world, i, -dx, -dy);
                    clear_motion(world, i);
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
                    clear_motion(world, i);
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

fn clear_motion(world: &mut World, index: usize) {
    world.polyobjs[index].motion = None;
    world.polyobjs[index].door_return = None;
    world.polyobjs[index].door_delay = 0;
}

/// Open leg done → optional door Wait → close, else idle.
fn finish_leg(world: &mut World, index: usize) {
    let delay = world.polyobjs[index].door_delay;
    let ret = world.polyobjs[index].door_return.take();
    world.polyobjs[index].door_delay = 0;
    match ret {
        Some(close) if delay > 0 => {
            world.polyobjs[index].motion = Some(PolyMotion::Wait {
                tics: delay,
                then: close,
            });
        }
        Some(close) => {
            world.polyobjs[index].motion = Some(close.into_motion());
        }
        None => {
            world.polyobjs[index].motion = None;
        }
    }
}

/// Returns `true` if the move may stand (clear, or crushing through victims).
fn resolve_poly_block(world: &mut World, index: usize) -> bool {
    let crush = world.polyobjs[index].crush;
    let victims = actors_hit_by_poly(world, index);
    if victims.is_empty() {
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

fn translate_poly(world: &mut World, index: usize, dx: f64, dy: f64) {
    let verts = world.polyobjs[index].vertices.clone();
    let lines = world.polyobjs[index].lines.clone();
    {
        let map = Arc::make_mut(&mut world.map);
        for &vid in &verts {
            if let Some(v) = map.vertices.get_mut(vid.raw() as usize) {
                v.pos = DVec2::new(v.pos.x() + dx, v.pos.y() + dy);
            }
        }
        refresh_line_geom(map, &lines);
    }
    world.polyobjs[index].center = DVec2::new(
        world.polyobjs[index].center.x() + dx,
        world.polyobjs[index].center.y() + dy,
    );
    world.polyobjs[index].start_spot = DVec2::new(
        world.polyobjs[index].start_spot.x() + dx,
        world.polyobjs[index].start_spot.y() + dy,
    );
}

fn rotate_poly(world: &mut World, index: usize, delta: f64) {
    let center = world.polyobjs[index].center;
    let verts = world.polyobjs[index].vertices.clone();
    let lines = world.polyobjs[index].lines.clone();
    let (s, c) = (delta.sin(), delta.cos());
    {
        let map = Arc::make_mut(&mut world.map);
        for &vid in &verts {
            if let Some(v) = map.vertices.get_mut(vid.raw() as usize) {
                let x = v.pos.x() - center.x();
                let y = v.pos.y() - center.y();
                v.pos = DVec2::new(center.x() + x * c - y * s, center.y() + x * s + y * c);
            }
        }
        refresh_line_geom(map, &lines);
    }
    world.polyobjs[index].angle += delta;
}

fn refresh_line_geom(map: &mut ferrite_map::MapGeometry, lines: &[LineId]) {
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
    use ferrite_map::{
        MapFormat, RawLinedef, RawSector, RawSidedef, RawThing, RawVertex, link_geometry,
    };
    use ferrite_nodebuild::apply_nodes;

    fn poly_square_map() -> ferrite_map::MapGeometry {
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
        let ir = ferrite_decorate::parse_decorate(
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
            ferrite_core::DVec3::new(140.0, 32.0, 0.0),
            ferrite_core::Angle::ZERO,
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
        assert!(w.polyobjs[0].motion.is_none(), "blocked poly should stop");
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
        // speed 32 bytes/tic, turn 64 bytes (=90°), delay 2.
        assert_eq!(
            execute_special(
                &mut w,
                num::POLYOBJ_DOOR_SWING,
                &[7, 32, 64, 2, 0],
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
        map.setup.polyobjects.push(ferrite_map::PolyobjStatic {
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
                door_delay: 0,
                door_return: None,
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
}
