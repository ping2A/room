//! Fixed-point `P_CheckPosition` / `P_TryMove` / `P_XYMovement` bootstrap (`docs/19` §4).

use crate::actor::{Actor, ActorId};
use crate::class::{FLAG_CORPSE, FLAG_MISSILE, FLAG_NOBLOCKMAP, FLAG_SHOOTABLE};
use crate::physics::{CheckResult, TryMoveResult, lines_near, path_traverse_blocks};
use crate::world::World;
use room_core::{DVec2, DVec3, FRACBITS, FRACUNIT, Fixed, LineId};

/// Doom `ML_BLOCKING`.
const ML_BLOCKING: i16 = 0x0001;
const FLAG_SOLID: u64 = 1 << 0;
/// Doom `MAXMOVE` (30 map units as fixed).
const MAXMOVE: Fixed = Fixed(30 * FRACUNIT);
/// Doom `ORIG_FRICTION` (`0xe800`).
const ORIG_FRICTION_FIXED: Fixed = Fixed(0xe800);
/// Doom `STOPSPEED` (`0x1000`).
const STOPSPEED_FIXED: Fixed = Fixed(0x1000);

fn line_blocks(line: &room_map::Line) -> bool {
    line.back_sector.is_none() || (line.flags & ML_BLOCKING) != 0
}

/// Doom `P_PointOnLineSide` (0 front / 1 back).
fn point_on_line_side(x: Fixed, y: Fixed, x1: Fixed, y1: Fixed, dx: Fixed, dy: Fixed) -> i32 {
    if dx.0 == 0 {
        if x.0 <= x1.0 {
            return i32::from(dy.0 > 0);
        }
        return i32::from(dy.0 < 0);
    }
    if dy.0 == 0 {
        if y.0 <= y1.0 {
            return i32::from(dx.0 < 0);
        }
        return i32::from(dx.0 > 0);
    }
    let adx = x - x1;
    let ady = y - y1;
    let left = Fixed(dy.0 >> FRACBITS).mul(adx);
    let right = ady.mul(Fixed(dx.0 >> FRACBITS));
    i32::from(right.0 >= left.0)
}

/// Doom `P_BoxOnLineSide`: 0/1, or -1 if the AABB crosses the infinite line.
#[allow(clippy::too_many_arguments)]
fn box_on_line_side(
    top: Fixed,
    bottom: Fixed,
    left: Fixed,
    right: Fixed,
    x1: Fixed,
    y1: Fixed,
    dx: Fixed,
    dy: Fixed,
) -> i32 {
    let (p1, p2) = if dy.0 == 0 {
        // ST_HORIZONTAL
        let mut a = i32::from(top.0 > y1.0);
        let mut b = i32::from(bottom.0 > y1.0);
        if dx.0 < 0 {
            a ^= 1;
            b ^= 1;
        }
        (a, b)
    } else if dx.0 == 0 {
        // ST_VERTICAL
        let mut a = i32::from(right.0 < x1.0);
        let mut b = i32::from(left.0 < x1.0);
        if dy.0 < 0 {
            a ^= 1;
            b ^= 1;
        }
        (a, b)
    } else if (dx.0 > 0) == (dy.0 > 0) {
        // ST_POSITIVE
        (
            point_on_line_side(left, top, x1, y1, dx, dy),
            point_on_line_side(right, bottom, x1, y1, dx, dy),
        )
    } else {
        // ST_NEGATIVE
        (
            point_on_line_side(right, top, x1, y1, dx, dy),
            point_on_line_side(left, bottom, x1, y1, dx, dy),
        )
    };
    if p1 == p2 { p1 } else { -1 }
}

/// Doom `PIT_CheckLine` geometry test: actor AABB overlaps line bbox and crosses the line.
fn aabb_hits_blocking_line(
    px: Fixed,
    py: Fixed,
    radius: Fixed,
    ax: Fixed,
    ay: Fixed,
    bx: Fixed,
    by: Fixed,
) -> bool {
    let top = py + radius;
    let bottom = py - radius;
    let left = px - radius;
    let right = px + radius;
    let ld_left = if ax.0 < bx.0 { ax } else { bx };
    let ld_right = if ax.0 > bx.0 { ax } else { bx };
    let ld_bottom = if ay.0 < by.0 { ay } else { by };
    let ld_top = if ay.0 > by.0 { ay } else { by };
    if right.0 <= ld_left.0 || left.0 >= ld_right.0 || top.0 <= ld_bottom.0 || bottom.0 >= ld_top.0
    {
        return false;
    }
    let dx = bx - ax;
    let dy = by - ay;
    box_on_line_side(top, bottom, left, right, ax, ay, dx, dy) == -1
}

/// Fixed-point [`crate::physics::check_position`].
pub fn check_position_fixed(
    world: &World,
    actor: &Actor,
    pos: DVec3,
    self_id: Option<ActorId>,
) -> CheckResult {
    let px = Fixed::from_f64(pos.x());
    let py = Fixed::from_f64(pos.y());
    let radius = Fixed::from_f64(actor.radius);
    let xy = DVec2::new(pos.x(), pos.y());
    let mut result = CheckResult {
        ok: true,
        floor_z: actor.pos.z(),
        ceiling_z: actor.pos.z() + actor.height,
        dropoff_z: actor.pos.z(),
        blocking_line: None,
        blocking_actor: None,
    };
    // Seed from the sector under the point, then raise/lower planes with Doom
    // `P_LineOpening` over every contacted line (higher floor / lower ceiling).
    // Without openings, stepping onto a lower sector mid-AABB drops `floor_z`
    // and skips ground friction (DEMO1@61 vs Chocolate).
    let mut saw_sector = false;
    if let Some(sid) = room_nodebuild::point_in_sector(&world.map, pos.x(), pos.y()) {
        if let Some(sec) = world.sectors.get(sid.raw() as usize) {
            result.floor_z = sec.floor_z;
            result.ceiling_z = sec.ceiling_z;
            result.dropoff_z = sec.floor_z;
            saw_sector = true;
        }
    }
    for lid in lines_near(&world.map, xy, actor.radius) {
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        let hit = aabb_hits_blocking_line(
            px,
            py,
            radius,
            Fixed::from_f64(v1.pos.x()),
            Fixed::from_f64(v1.pos.y()),
            Fixed::from_f64(v2.pos.x()),
            Fixed::from_f64(v2.pos.y()),
        );
        if !hit {
            continue;
        }
        // `P_LineOpening` for two-sided (and single-sided front) contacts.
        let front = line
            .front_sector
            .and_then(|sid| world.sectors.get(sid.raw() as usize));
        let back = line
            .back_sector
            .and_then(|sid| world.sectors.get(sid.raw() as usize));
        match (front, back) {
            (Some(f), Some(b)) => {
                let open_bottom = f.floor_z.max(b.floor_z);
                let open_top = f.ceiling_z.min(b.ceiling_z);
                let low_floor = f.floor_z.min(b.floor_z);
                if saw_sector {
                    result.floor_z = result.floor_z.max(open_bottom);
                    result.ceiling_z = result.ceiling_z.min(open_top);
                    result.dropoff_z = result.dropoff_z.min(low_floor);
                } else {
                    result.floor_z = open_bottom;
                    result.ceiling_z = open_top;
                    result.dropoff_z = low_floor;
                    saw_sector = true;
                }
            }
            (Some(f), None) | (None, Some(f)) => {
                if !saw_sector {
                    result.floor_z = f.floor_z;
                    result.ceiling_z = f.ceiling_z;
                    result.dropoff_z = f.floor_z;
                    saw_sector = true;
                }
            }
            (None, None) => {}
        }
        if line_blocks(line) {
            result.ok = false;
            result.blocking_line = Some(lid);
            return result;
        }
    }
    let is_missile = (actor.flags & FLAG_MISSILE) != 0;
    let originator = if is_missile { actor.target } else { None };
    for id in &world.thinkers.default {
        if self_id == Some(*id) {
            continue;
        }
        let Some(other) = world.actors.get(*id) else {
            continue;
        };
        if other.destroyed || (other.flags & FLAG_NOBLOCKMAP) != 0 {
            continue;
        }
        // Doom `PIT_CheckThing`: only `SOLID|SPECIAL|SHOOTABLE` (we omit SPECIAL pickups).
        let solid = (other.flags & FLAG_SOLID) != 0;
        let shootable = (other.flags & FLAG_SHOOTABLE) != 0;
        if !solid && !shootable {
            continue;
        }
        // Doom `PIT_CheckThing`: axis-aligned square (not a circle).
        let dx = (Fixed::from_f64(other.pos.x()) - px).0.abs();
        let dy = (Fixed::from_f64(other.pos.y()) - py).0.abs();
        let blockdist = (Fixed::from_f64(other.radius) + radius).0;
        if dx >= blockdist || dy >= blockdist {
            continue;
        }
        if is_missile {
            // Missiles: Doom over/under test, then originator / same-species rules.
            let top = pos.z() + actor.height;
            let obot = other.pos.z();
            let otop = obot + other.height;
            if top <= obot || pos.z() >= otop {
                continue;
            }
            if originator == Some(*id) {
                continue;
            }
            if let Some(orig) = originator.and_then(|oid| world.actors.get(oid)) {
                let other_is_player = world.players.iter().flatten().any(|p| p.mo == Some(*id));
                if orig.class == other.class && !other_is_player {
                    continue;
                }
            }
            if shootable {
                result.ok = false;
                result.blocking_actor = Some(*id);
                return result;
            }
            if solid {
                result.ok = false;
                return result;
            }
            continue;
        }
        // Doom `PIT_CheckThing` (non-missile): XY square only — no Z skip.
        // Corpses keep SOLID until `A_Fall`, so flat death-height still blocks.
        if solid {
            result.ok = false;
            result.blocking_actor = Some(*id);
            return result;
        }
    }
    if !saw_sector {
        if let Some(sec) = world.sectors.first() {
            result.floor_z = sec.floor_z;
            result.ceiling_z = sec.ceiling_z;
        }
    }
    result
}

/// Fixed-point [`crate::physics::try_move_ex`].
pub fn try_move_fixed(world: &mut World, actor_id: ActorId, new_pos: DVec3) -> TryMoveResult {
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return TryMoveResult {
            ok: false,
            blocking_actor: None,
        };
    };
    let from = DVec2::new(actor.pos.x(), actor.pos.y());
    let to = DVec2::new(new_pos.x(), new_pos.y());
    let mut dest = new_pos;
    let noclip = world
        .players
        .iter()
        .flatten()
        .any(|p| p.noclip && p.mo == Some(actor_id));
    if !noclip {
        let check = check_position_fixed(world, &actor, new_pos, Some(actor_id));
        if !check.ok {
            return TryMoveResult {
                ok: false,
                blocking_actor: check.blocking_actor,
            };
        }
        let max_step = world
            .classes
            .get(actor.class)
            .map(|c| c.max_step_height)
            .unwrap_or(24.0);
        let max_drop = world
            .classes
            .get(actor.class)
            .map(|c| c.max_dropoff_height)
            .unwrap_or(24.0);
        // Doom `P_TryMove`: step-up vs current z; dropoff is `tmfloorz - tmdropoffz`.
        if check.floor_z - actor.pos.z() > max_step {
            return TryMoveResult {
                ok: false,
                blocking_actor: None,
            };
        }
        if check.ceiling_z - actor.pos.z() < actor.height {
            return TryMoveResult {
                ok: false,
                blocking_actor: None,
            };
        }
        let floating = (actor.flags & crate::class::FLAG_FLOAT) != 0
            || (actor.flags & crate::class::FLAG_NOGRAVITY) != 0;
        let can_dropoff = (actor.flags & crate::class::FLAG_DROPOFF) != 0;
        if !floating && !can_dropoff && check.floor_z - check.dropoff_z > max_drop {
            return TryMoveResult {
                ok: false,
                blocking_actor: None,
            };
        }
        if check.ceiling_z - check.floor_z < actor.height {
            return TryMoveResult {
                ok: false,
                blocking_actor: None,
            };
        }
        if check.floor_z > actor.pos.z() {
            dest = DVec3::new(new_pos.x(), new_pos.y(), check.floor_z);
        }
    }
    // Snap XY through Fixed so path is bit-stable under VanillaFixed.
    dest = DVec3::new(
        Fixed::from_f64(dest.x()).to_f64(),
        Fixed::from_f64(dest.y()).to_f64(),
        dest.z(),
    );
    if let Some(a) = world.actors.get_mut(actor_id) {
        a.pos = dest;
    }
    crate::cross_lines::cross_special_lines(world, actor_id, from, to);
    TryMoveResult {
        ok: true,
        blocking_actor: None,
    }
}

/// Fixed-point `P_XYMovement` (stepped try-move + ground friction).
pub fn xy_movement_fixed(world: &mut World, actor_id: ActorId) {
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return;
    };
    let mut momx = Fixed::from_f64(actor.vel.x());
    let mut momy = Fixed::from_f64(actor.vel.y());
    if momx.0 == 0 && momy.0 == 0 {
        return;
    }
    // Doom clamps each axis to ±MAXMOVE before stepping.
    if momx.0 > MAXMOVE.0 {
        momx = MAXMOVE;
    } else if momx.0 < -MAXMOVE.0 {
        momx = Fixed(-MAXMOVE.0);
    }
    if momy.0 > MAXMOVE.0 {
        momy = MAXMOVE;
    } else if momy.0 < -MAXMOVE.0 {
        momy = Fixed(-MAXMOVE.0);
    }
    // Doom `P_XYMovement` do/while: if either axis > MAXMOVE/2, take half and
    // keep the rest for another iteration; else take the remainder and finish.
    let mut xmove = momx;
    let mut ymove = momy;
    let is_missile = (actor.flags & FLAG_MISSILE) != 0;
    let z = actor.pos.z();
    let half = Fixed(MAXMOVE.0 / 2);
    loop {
        let cur = world
            .actors
            .get(actor_id)
            .map(|a| a.pos)
            .unwrap_or(actor.pos);
        // Doom compares signed `> MAXMOVE/2` (not abs) — large negative moves do not split.
        let (ptry_x, ptry_y, done) = if xmove.0 > half.0 || ymove.0 > half.0 {
            // Doom: `ptry = mo + xmove/2` (toward-zero), then `xmove >>= 1`
            // (arithmetic). For negative mom those differ by 1 — do not reuse `/2`
            // as the remainder (DEMO1 @914 Y ulp).
            let step_x = Fixed(xmove.0 / 2);
            let step_y = Fixed(ymove.0 / 2);
            xmove = Fixed(xmove.0 >> 1);
            ymove = Fixed(ymove.0 >> 1);
            (
                (Fixed::from_f64(cur.x()) + step_x).to_f64(),
                (Fixed::from_f64(cur.y()) + step_y).to_f64(),
                false,
            )
        } else {
            let px = (Fixed::from_f64(cur.x()) + xmove).to_f64();
            let py = (Fixed::from_f64(cur.y()) + ymove).to_f64();
            xmove = Fixed::ZERO;
            ymove = Fixed::ZERO;
            (px, py, true)
        };
        let dest = DVec3::new(ptry_x, ptry_y, z);
        let mv = try_move_fixed(world, actor_id, dest);
        if !mv.ok {
            if is_missile {
                let mut ripped = false;
                if let Some(victim) = mv.blocking_actor {
                    crate::physics::apply_missile_damage(world, actor_id, victim);
                    ripped = match (world.actors.get(actor_id), world.actors.get(victim)) {
                        (Some(m), Some(v)) => {
                            crate::action::is_ripper_missile(m)
                                && crate::action::can_rip_through(m, v)
                        }
                        _ => false,
                    };
                    if ripped {
                        if let Some(a) = world.actors.get_mut(actor_id) {
                            a.pos = dest;
                        }
                    }
                }
                if ripped {
                    if done || (xmove.0 == 0 && ymove.0 == 0) {
                        break;
                    }
                    continue;
                }
                crate::physics::explode_missile(world, actor_id);
            } else if world
                .players
                .iter()
                .any(|p| p.as_ref().and_then(|pl| pl.mo) == Some(actor_id))
            {
                // Doom `P_SlideMove` uses full `mo->momx/momy`, not the step.
                let (sx, sy) = slide_move_fixed(world, actor_id, dest, momx, momy);
                momx = sx;
                momy = sy;
            } else {
                momx = Fixed::ZERO;
                momy = Fixed::ZERO;
            }
            break;
        }
        if done || (xmove.0 == 0 && ymove.0 == 0) {
            break;
        }
    }
    // Doom `P_XYMovement`: `if (mo->z > mo->floorz) return` — no friction airborne.
    // Use exact `z <= floorz` (not +1 slack): DEMO1@1689 is z=-4 over floor=-5.
    let (on_ground, skip_corpse_fric) = world.actors.get(actor_id).map_or((false, false), |a| {
        let chk = check_position_fixed(world, a, a.pos, Some(actor_id));
        let on_ground = a.pos.z() <= chk.floor_z;
        // Doom: `MF_CORPSE` + |mom| > FRACUNIT/4 + floorz != sector floorheight
        // → keep sliding off a step (DEMO1@1918 dead Demon coast).
        let sec_floor = room_nodebuild::point_in_sector(&world.map, a.pos.x(), a.pos.y())
            .and_then(|sid| world.sectors.get(sid.raw() as usize))
            .map(|s| s.floor_z)
            .unwrap_or(chk.floor_z);
        let quarter = Fixed(FRACUNIT / 4);
        let skip = (a.flags & FLAG_CORPSE) != 0
            && (momx.abs().0 > quarter.0 || momy.abs().0 > quarter.0)
            && Fixed::from_f64(chk.floor_z).0 != Fixed::from_f64(sec_floor).0;
        (on_ground, skip)
    });
    if !is_missile && on_ground && !skip_corpse_fric {
        // Doom `P_XYMovement`: STOPSPEED stop is an *alternative* to friction
        // (check pre-friction mom). Applying friction first then zeroing stops
        // one tic early (DEMO1 @1369).
        let no_cmd = world
            .players
            .iter()
            .flatten()
            .find(|p| p.mo == Some(actor_id))
            .is_none_or(|p| p.cmd.forward_move == 0 && p.cmd.side_move == 0);
        if no_cmd && momx.abs().0 < STOPSPEED_FIXED.0 && momy.abs().0 < STOPSPEED_FIXED.0 {
            momx = Fixed::ZERO;
            momy = Fixed::ZERO;
        } else {
            let class_fric = if actor.friction > 0.0 {
                actor.friction
            } else {
                world
                    .classes
                    .get(actor.class)
                    .map(|c| c.friction)
                    .unwrap_or(1.0)
            };
            let pos = world
                .actors
                .get(actor_id)
                .map(|a| a.pos)
                .unwrap_or(actor.pos);
            let sector_fric = room_nodebuild::point_in_sector(&world.map, pos.x(), pos.y())
                .and_then(|sid| world.sectors.get(sid.raw() as usize))
                .map(|s| s.friction)
                .unwrap_or(ORIG_FRICTION_FIXED.to_f64());
            let factor = if (class_fric - 1.0).abs() < 1e-12
                && (sector_fric - ORIG_FRICTION_FIXED.to_f64()).abs() < 1e-12
            {
                ORIG_FRICTION_FIXED
            } else {
                Fixed::from_f64((sector_fric * class_fric).clamp(0.0, 1.0))
            };
            momx = momx * factor;
            momy = momy * factor;
        }
    }
    if let Some(a) = world.actors.get_mut(actor_id) {
        a.vel = DVec3::new(momx.to_f64(), momy.to_f64(), a.vel.z());
    }
}

/// True if the actor AABB at `pos` overlaps the linedef segment bbox.
fn aabb_overlaps_line_seg(world: &World, actor: &Actor, pos: DVec3, lid: LineId) -> bool {
    let Some(line) = world.map.lines.get(lid.raw() as usize) else {
        return false;
    };
    let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
        return false;
    };
    let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
        return false;
    };
    let px = Fixed::from_f64(pos.x());
    let py = Fixed::from_f64(pos.y());
    let radius = Fixed::from_f64(actor.radius);
    let top = py + radius;
    let bottom = py - radius;
    let left = px - radius;
    let right = px + radius;
    let ld_left = Fixed::from_f64(v1.pos.x().min(v2.pos.x()));
    let ld_right = Fixed::from_f64(v1.pos.x().max(v2.pos.x()));
    let ld_bottom = Fixed::from_f64(v1.pos.y().min(v2.pos.y()));
    let ld_top = Fixed::from_f64(v1.pos.y().max(v2.pos.y()));
    !(right.0 <= ld_left.0 || left.0 >= ld_right.0 || top.0 <= ld_bottom.0 || bottom.0 >= ld_top.0)
}

/// True if a two-sided line blocks slide like Doom `PTR_SlideTraverse` (`openrange`
/// / step / headroom), not only `ML_BLOCKING`.
fn line_blocks_slide(world: &World, lid: LineId, line: &room_map::Line, actor: &Actor) -> bool {
    if line.back_sector.is_none() {
        return true;
    }
    if (world.line_flags(lid) & i32::from(ML_BLOCKING)) != 0 {
        return true;
    }
    let front = line
        .front_sector
        .and_then(|sid| world.sectors.get(sid.raw() as usize));
    let back = line
        .back_sector
        .and_then(|sid| world.sectors.get(sid.raw() as usize));
    let (Some(f), Some(b)) = (front, back) else {
        return true;
    };
    let open_bottom = f.floor_z.max(b.floor_z);
    let open_top = f.ceiling_z.min(b.ceiling_z);
    let open_range = open_top - open_bottom;
    // Doom `PTR_SlideTraverse` isblocking tests.
    if open_range < actor.height {
        return true;
    }
    if open_top - actor.pos.z() < actor.height {
        return true;
    }
    if open_bottom - actor.pos.z() > 24.0 {
        return true;
    }
    false
}

/// Lines in blockmap cells along the three Doom slide corner traces.
fn slide_path_lines(world: &World, actor: &Actor, momx: Fixed, momy: Fixed) -> Vec<LineId> {
    let px = Fixed::from_f64(actor.pos.x());
    let py = Fixed::from_f64(actor.pos.y());
    let radius = Fixed::from_f64(actor.radius);
    let (leadx, trailx) = if momx.0 > 0 {
        (px + radius, px - radius)
    } else {
        (px - radius, px + radius)
    };
    let (leady, traily) = if momy.0 > 0 {
        (py + radius, py - radius)
    } else {
        (py - radius, py + radius)
    };
    let traces = [(leadx, leady), (trailx, leady), (leadx, traily)];
    let bm = &world.map.blockmap;
    let mut lids = Vec::new();
    for (sx, sy) in traces {
        let x1 = sx.to_f64();
        let y1 = sy.to_f64();
        let x2 = (sx + momx).to_f64();
        let y2 = (sy + momy).to_f64();
        for (bx, by) in path_traverse_blocks(bm, x1, y1, x2, y2) {
            if bx < 0 || by < 0 {
                continue;
            }
            let (bx, by) = (bx as u16, by as u16);
            if bx >= bm.columns || by >= bm.rows {
                continue;
            }
            let idx = usize::from(by) * usize::from(bm.columns) + usize::from(bx);
            let Some(cell) = bm.blocks.get(idx) else {
                continue;
            };
            for &lid in cell {
                if lids.contains(&lid) {
                    continue;
                }
                let Some(line) = world.map.lines.get(lid.raw() as usize) else {
                    continue;
                };
                if line_blocks_slide(world, lid, line, actor) {
                    lids.push(lid);
                }
            }
        }
    }
    lids
}

/// Doom `P_HitSlideLine`.
fn hit_slide_line(
    world: &World,
    lid: LineId,
    momx: Fixed,
    momy: Fixed,
    actor_x: Fixed,
    actor_y: Fixed,
) -> (Fixed, Fixed) {
    let Some(line) = world.map.lines.get(lid.raw() as usize) else {
        return (momx, momy);
    };
    let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
        return (momx, momy);
    };
    let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
        return (momx, momy);
    };
    let ldx = Fixed::from_f64(v2.pos.x() - v1.pos.x());
    let ldy = Fixed::from_f64(v2.pos.y() - v1.pos.y());
    // `ST_HORIZONTAL` / `ST_VERTICAL` fast paths.
    if ldy.0 == 0 {
        return (momx, Fixed::ZERO);
    }
    if ldx.0 == 0 {
        return (Fixed::ZERO, momy);
    }
    let side = point_on_line_side(
        actor_x,
        actor_y,
        Fixed::from_f64(v1.pos.x()),
        Fixed::from_f64(v1.pos.y()),
        ldx,
        ldy,
    );
    let mut lineangle = room_core::math::point_to_angle2(0, 0, ldx.0, ldy.0);
    if side == 1 {
        lineangle = lineangle.wrapping_add(0x8000_0000);
    }
    let moveangle = room_core::math::point_to_angle2(0, 0, momx.0, momy.0);
    let mut deltaangle = moveangle.wrapping_sub(lineangle);
    if deltaangle > 0x8000_0000 {
        deltaangle = deltaangle.wrapping_add(0x8000_0000);
    }
    let line_fine = room_core::math::bam_to_fine(lineangle);
    let delta_fine = room_core::math::bam_to_fine(deltaangle);
    let movelen = room_core::math::aprox_distance(momx.0, momy.0);
    let newlen = Fixed(movelen).mul(Fixed(room_core::math::finecosine(delta_fine)));
    (
        newlen.mul(Fixed(room_core::math::finecosine(line_fine))),
        newlen.mul(Fixed(room_core::math::finesine(line_fine))),
    )
}

/// Doom `P_InterceptVector` (`>>8` on terms — matches Chocolate `bestslidefrac`).
fn intercept_vector(
    trace_x: Fixed,
    trace_y: Fixed,
    trace_dx: Fixed,
    trace_dy: Fixed,
    line_x: Fixed,
    line_y: Fixed,
    line_dx: Fixed,
    line_dy: Fixed,
) -> Option<Fixed> {
    let den = Fixed(line_dy.0 >> 8).mul(trace_dx) - Fixed(line_dx.0 >> 8).mul(trace_dy);
    if den.0 == 0 {
        return None;
    }
    let num = Fixed((line_x.0 - trace_x.0) >> 8).mul(line_dy)
        + Fixed((trace_y.0 - line_y.0) >> 8).mul(line_dx);
    Some(num / den)
}

/// Slide fraction for a blocking line (`bestslidefrac` via Doom corner traces).
fn slide_line_frac(
    world: &World,
    actor: &Actor,
    lid: LineId,
    momx: Fixed,
    momy: Fixed,
) -> Option<Fixed> {
    let line = world.map.lines.get(lid.raw() as usize)?;
    let v1 = world.map.vertices.get(line.v1.raw() as usize)?;
    let v2 = world.map.vertices.get(line.v2.raw() as usize)?;
    let ldx = Fixed::from_f64(v2.pos.x() - v1.pos.x());
    let ldy = Fixed::from_f64(v2.pos.y() - v1.pos.y());
    let px = Fixed::from_f64(actor.pos.x());
    let py = Fixed::from_f64(actor.pos.y());
    let radius = Fixed::from_f64(actor.radius);
    let wall_x = Fixed::from_f64(v1.pos.x());
    let wall_y = Fixed::from_f64(v1.pos.y());
    // Doom `P_SlideMove`: three leading-corner traces.
    // Note: `momx == 0` still uses `x ± radius` (not the center) — same for `momy`.
    let (leadx, trailx) = if momx.0 > 0 {
        (px + radius, px - radius)
    } else {
        (px - radius, px + radius)
    };
    let (leady, traily) = if momy.0 > 0 {
        (py + radius, py - radius)
    } else {
        (py - radius, py + radius)
    };
    // Doom `PIT_AddLineIntercepts`: large traces use `P_PointOnDivlineSide` on
    // line endpoints; short traces (|dx|,|dy| ≤ 16) use `P_PointOnLineSide` on
    // the trace endpoints (DEMO1@1007 L40).
    let large = momx.0 > 16 * FRACUNIT
        || momy.0 > 16 * FRACUNIT
        || momx.0 < -16 * FRACUNIT
        || momy.0 < -16 * FRACUNIT;
    let mut best: Option<Fixed> = None;
    for (sx, sy) in [(leadx, leady), (trailx, leady), (leadx, traily)] {
        let (s1, s2) = if large {
            (
                point_on_divline_side(wall_x, wall_y, sx, sy, momx, momy),
                point_on_divline_side(
                    Fixed::from_f64(v2.pos.x()),
                    Fixed::from_f64(v2.pos.y()),
                    sx,
                    sy,
                    momx,
                    momy,
                ),
            )
        } else {
            (
                point_on_line_side(sx, sy, wall_x, wall_y, ldx, ldy),
                point_on_line_side(sx + momx, sy + momy, wall_x, wall_y, ldx, ldy),
            )
        };
        if s1 == s2 {
            continue;
        }
        let Some(frac) = intercept_vector(sx, sy, momx, momy, wall_x, wall_y, ldx, ldy) else {
            continue;
        };
        if frac.0 < 0 || frac.0 >= FRACUNIT {
            continue;
        }
        if best.is_none_or(|b| frac.0 < b.0) {
            best = Some(frac);
        }
    }
    best
}

/// Doom `P_PointOnDivlineSide` (0/1).
fn point_on_divline_side(x: Fixed, y: Fixed, ox: Fixed, oy: Fixed, dx: Fixed, dy: Fixed) -> i32 {
    if dx.0 == 0 {
        if x.0 <= ox.0 {
            return i32::from(dy.0 > 0);
        }
        return i32::from(dy.0 < 0);
    }
    if dy.0 == 0 {
        if y.0 <= oy.0 {
            return i32::from(dx.0 < 0);
        }
        return i32::from(dx.0 > 0);
    }
    let dx2 = x - ox;
    let dy2 = y - oy;
    let left = Fixed(dy.0 >> 8).mul(dx2);
    let right = Fixed(dx.0 >> 8).mul(dy2);
    i32::from(right.0 >= left.0)
}

/// Doom `P_SlideMove`: fudged approach + HitSlideLine remainder (axis walls).
/// Retries up to 3 times when the clipped move still blocks (corner).
fn slide_move_fixed(
    world: &mut World,
    actor_id: ActorId,
    failed_dest: DVec3,
    mut momx: Fixed,
    mut momy: Fixed,
) -> (Fixed, Fixed) {
    let _ = failed_dest;
    for _hit in 0..3 {
        let Some(actor) = world.actors.get(actor_id).cloned() else {
            return (Fixed::ZERO, Fixed::ZERO);
        };
        // Doom `P_SlideMove`: lines from `P_PathTraverse` on the three leading
        // corners (not an AABB query — DEMO1@1359 L534 is far as a segment but
        // its infinite line straddles a corner ray).
        let lids = slide_path_lines(world, &actor, momx, momy);
        // Thing-only block → stairstep (path traverse sees lines only).
        if lids.is_empty() {
            return slide_stairstep(world, actor_id, momx, momy);
        }
        let mut best_frac = Fixed(FRACUNIT + 1);
        let mut best_lid: Option<LineId> = None;
        for lid in &lids {
            let Some(line) = world.map.lines.get(lid.raw() as usize) else {
                continue;
            };
            // Doom `PTR_SlideTraverse`: one-sided lines only block from the front.
            if line.back_sector.is_none() {
                let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
                    continue;
                };
                let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
                    continue;
                };
                let side = point_on_line_side(
                    Fixed::from_f64(actor.pos.x()),
                    Fixed::from_f64(actor.pos.y()),
                    Fixed::from_f64(v1.pos.x()),
                    Fixed::from_f64(v1.pos.y()),
                    Fixed::from_f64(v2.pos.x() - v1.pos.x()),
                    Fixed::from_f64(v2.pos.y() - v1.pos.y()),
                );
                if side != 0 {
                    continue;
                }
            }
            // Failed-dest lids can include segments only reached after moving into
            // a corner (west wall below the south ledge). A frac=0 hit on those
            // must not clip mom — require the current box to touch the segment.
            let cur_touch = aabb_overlaps_line_seg(world, &actor, actor.pos, *lid);
            if let Some(frac) = slide_line_frac(world, &actor, *lid, momx, momy) {
                if frac.0 == 0 && !cur_touch {
                    continue;
                }
                if frac.0 < best_frac.0 {
                    best_frac = frac;
                    best_lid = Some(*lid);
                }
            }
        }
        let Some(lid) = best_lid else {
            return slide_stairstep(world, actor_id, momx, momy);
        };
        // `bestslidefrac -= 0x800` then approach if still positive.
        let fudged = Fixed(best_frac.0 - 0x800);
        if fudged.0 > 0 {
            let approach = DVec3::new(
                (Fixed::from_f64(actor.pos.x()) + momx.mul(fudged)).to_f64(),
                (Fixed::from_f64(actor.pos.y()) + momy.mul(fudged)).to_f64(),
                actor.pos.z(),
            );
            if !try_move_fixed(world, actor_id, approach).ok {
                return slide_stairstep(world, actor_id, momx, momy);
            }
        }
        // `remain = FRACUNIT - original_bestslidefrac`.
        let mut remain = Fixed(FRACUNIT - best_frac.0);
        if remain.0 > FRACUNIT {
            remain = Fixed::ONE;
        }
        if remain.0 <= 0 {
            return slide_stairstep(world, actor_id, momx, momy);
        }
        let actor = world.actors.get(actor_id).cloned().unwrap_or(actor);
        let tmx = momx.mul(remain);
        let tmy = momy.mul(remain);
        let (sx, sy) = hit_slide_line(
            world,
            lid,
            tmx,
            tmy,
            Fixed::from_f64(actor.pos.x()),
            Fixed::from_f64(actor.pos.y()),
        );
        momx = sx;
        momy = sy;
        let slide_dest = DVec3::new(
            (Fixed::from_f64(actor.pos.x()) + sx).to_f64(),
            (Fixed::from_f64(actor.pos.y()) + sy).to_f64(),
            actor.pos.z(),
        );
        if try_move_fixed(world, actor_id, slide_dest).ok {
            return (sx, sy);
        }
        // Clipped move still blocked (corner) — retry with new mom like Doom.
    }
    slide_stairstep(world, actor_id, momx, momy)
}

fn slide_stairstep(
    world: &mut World,
    actor_id: ActorId,
    momx: Fixed,
    momy: Fixed,
) -> (Fixed, Fixed) {
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return (Fixed::ZERO, Fixed::ZERO);
    };
    let dest_y = DVec3::new(
        actor.pos.x(),
        (Fixed::from_f64(actor.pos.y()) + momy).to_f64(),
        actor.pos.z(),
    );
    if try_move_fixed(world, actor_id, dest_y).ok {
        return (momx, momy);
    }
    let dest_x = DVec3::new(
        (Fixed::from_f64(actor.pos.x()) + momx).to_f64(),
        actor.pos.y(),
        actor.pos.z(),
    );
    let _ = try_move_fixed(world, actor_id, dest_x);
    (momx, momy)
}

/// Doom `GRAVITY` (`FRACUNIT` per tic²).
const GRAVITY_FIXED: Fixed = Fixed(FRACUNIT);

/// Fixed-point `P_ZMovement` (Doom order: `z += momz`, then gravity if still aloft).
pub fn z_movement_fixed(world: &mut World, actor_id: ActorId) {
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return;
    };
    let planes = check_position_fixed(world, &actor, actor.pos, Some(actor_id));
    let floor_z = Fixed::from_f64(planes.floor_z);
    let ceiling_z = Fixed::from_f64(planes.ceiling_z);
    let mut momz = Fixed::from_f64(actor.vel.z());
    // Doom: apply current vertical mom first.
    let mut z = Fixed::from_f64(actor.pos.z()) + momz;
    let mut hard_land = false;
    let mut hit_floor = false;
    let mut hit_ceiling = false;
    let impact_vel_z = momz.to_f64();
    let is_missile = (actor.flags & crate::class::FLAG_MISSILE) != 0;
    if z.0 <= floor_z.0 {
        if Fixed::from_f64(actor.pos.z()).0 > floor_z.0 + FRACUNIT / 10 && momz.0 < -8 * FRACUNIT {
            hard_land = world
                .players
                .iter()
                .flatten()
                .any(|p| p.mo == Some(actor_id));
        }
        if Fixed::from_f64(actor.pos.z()).0 > floor_z.0 + FRACUNIT / 10 && momz.0 < 0 {
            hit_floor = true;
        }
        if is_missile {
            hit_floor = true;
        }
        if momz.0 < 0 {
            momz = Fixed::ZERO;
        }
        z = floor_z;
    } else {
        let no_gravity = (actor.flags & crate::class::FLAG_NOGRAVITY) != 0;
        if !no_gravity {
            let actor_g = if actor.gravity > 0.0 {
                actor.gravity
            } else {
                world
                    .classes
                    .get(actor.class)
                    .map(|c| c.gravity)
                    .unwrap_or(1.0)
            };
            let sector_g =
                room_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
                    .or_else(|| {
                        room_nodebuild::point_in_sector_vanilla(
                            &world.map,
                            actor.pos.x(),
                            actor.pos.y(),
                        )
                    })
                    .and_then(|sid| world.sectors.get(sid.raw() as usize))
                    .map(|s| s.gravity)
                    .unwrap_or(1.0);
            let g = GRAVITY_FIXED
                * Fixed::from_f64(world.level.gravity)
                * Fixed::from_f64(sector_g)
                * Fixed::from_f64(actor_g);
            // Doom: first fall tick from rest uses `-2*GRAVITY`.
            if momz.0 == 0 {
                momz = Fixed(-g.0 * 2);
            } else {
                momz = momz - g;
            }
        }
    }
    let head = z + Fixed::from_f64(actor.height);
    if head.0 > ceiling_z.0 {
        z = ceiling_z - Fixed::from_f64(actor.height);
        if momz.0 > 0 {
            momz = Fixed::ZERO;
        }
        hit_ceiling = true;
    }
    if let Some(a) = world.actors.get_mut(actor_id) {
        a.pos = DVec3::new(a.pos.x(), a.pos.y(), z.to_f64());
        a.vel = DVec3::new(a.vel.x(), a.vel.y(), momz.to_f64());
    }
    if is_missile && (hit_floor || hit_ceiling) {
        crate::physics::explode_missile(world, actor_id);
        return;
    }
    if hard_land {
        world.play_sound(
            "player/male/land",
            actor.pos.x(),
            actor.pos.y(),
            floor_z.to_f64(),
            1.0,
        );
    }
    if hit_floor {
        crate::physics::try_falling_damage(world, actor_id, impact_vel_z);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::vanilla::SimMathMode;
    use crate::world::World;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn check_position_fixed_no_false_far_actor_block() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 4096.0, 4096.0), 0.0, 128.0)
            .player_start(224.0, 1184.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let ir = room_decorate::parse_decorate(
            r#"
actor DoomImp : Actor 3001
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
        w.math_mode = SimMathMode::VanillaFixed;
        let imp = w.classes.find_doomednum(3001).unwrap();
        w.spawn_at(
            imp,
            DVec3::new(416.0, 2592.0, 0.0),
            room_core::Angle::ZERO,
        )
        .unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let a = w.actors.get(mo).unwrap().clone();
        let chk = check_position_fixed(&w, &a, a.pos, Some(mo));
        assert!(chk.ok, "far imp must not block via FixedMul overflow");
        assert!(chk.blocking_actor.is_none());
    }

    #[test]
    fn try_move_fixed_relocates_in_open_room() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.math_mode = SimMathMode::VanillaFixed;
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let dest = DVec3::new(80.0, 64.0, 0.0);
        assert!(try_move_fixed(&mut w, mo, dest).ok);
        let p = w.actors.get(mo).unwrap().pos;
        assert!((p.x() - 80.0).abs() < 1e-6);
        assert!((p.y() - 64.0).abs() < 1e-6);
    }

    #[test]
    fn xy_movement_fixed_negative_split_matches_doom_shift() {
        // `momx > MAXMOVE/2` forces a split; negative `momy` must use `/2` then `>>1`.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 4096.0, 4096.0), 0.0, 128.0)
            .player_start(1024.0, 1024.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.math_mode = SimMathMode::VanillaFixed;
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let momx = Fixed(985_510);
        let momy = Fixed(-85_465);
        if let Some(a) = w.actors.get_mut(mo) {
            a.vel = DVec3::new(momx.to_f64(), momy.to_f64(), 0.0);
        }
        let (x0, y0) = {
            let a = w.actors.get(mo).unwrap();
            (Fixed::from_f64(a.pos.x()).0, Fixed::from_f64(a.pos.y()).0)
        };
        xy_movement_fixed(&mut w, mo);
        let (x1, y1) = {
            let a = w.actors.get(mo).unwrap();
            (Fixed::from_f64(a.pos.x()).0, Fixed::from_f64(a.pos.y()).0)
        };
        // Doom total Δy = (momy/2) + (momy>>1) = -42732 + -42733 = -85465.
        assert_eq!(x1 - x0, momx.0, "Δx");
        assert_eq!(y1 - y0, momy.0, "Δy must keep the odd negative ulp");
    }

    #[test]
    fn xy_movement_fixed_applies_orig_friction() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.math_mode = SimMathMode::VanillaFixed;
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.vel = DVec3::new(1.5625, 0.0, 0.0);
        }
        let x0 = w.actors.get(mo).unwrap().pos.x();
        xy_movement_fixed(&mut w, mo);
        let a = w.actors.get(mo).unwrap();
        assert!((a.pos.x() - (x0 + 1.5625)).abs() < 1e-6, "x={}", a.pos.x());
        let expected = (Fixed::from_f64(1.5625) * ORIG_FRICTION_FIXED).to_f64();
        assert!(
            (a.vel.x() - expected).abs() < 1e-9,
            "vel.x={} expected {expected}",
            a.vel.x()
        );
    }

    #[test]
    fn xy_movement_corpse_skips_friction_on_step() {
        // Two floors: standing over the higher contact while the center sector is
        // lower → Doom `MF_CORPSE` keeps mom (DEMO1@1918 Demon coast).
        let mut map = MapBuilder::new()
            .room("hi", rect(0.0, 0.0, 128.0, 256.0), 0.0, 128.0)
            .room("lo", rect(128.0, 0.0, 256.0, 256.0), -5.0, 128.0)
            .player_start(140.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.math_mode = SimMathMode::VanillaFixed;
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let mom = Fixed(53_135); // > FRACUNIT/4
        if let Some(a) = w.actors.get_mut(mo) {
            a.flags |= FLAG_CORPSE;
            a.health = 0;
            a.vel = DVec3::new(mom.to_f64(), 0.0, 0.0);
            a.pos = DVec3::new(140.0, 128.0, 0.0); // z matches higher contact floor
        }
        let chk = {
            let a = w.actors.get(mo).unwrap().clone();
            check_position_fixed(&w, &a, a.pos, Some(mo))
        };
        let sec_floor = room_nodebuild::point_in_sector(&w.map, 140.0, 128.0)
            .and_then(|sid| w.sectors.get(sid.raw() as usize))
            .map(|s| s.floor_z)
            .unwrap();
        assert!(
            (chk.floor_z - sec_floor).abs() > 1e-6,
            "need step contact floorz={} sec={}",
            chk.floor_z,
            sec_floor
        );
        xy_movement_fixed(&mut w, mo);
        let a = w.actors.get(mo).unwrap();
        assert!(
            (Fixed::from_f64(a.vel.x()).0 - mom.0).abs() <= 1,
            "corpse mom must not rub: vel={} expected {}",
            a.vel.x(),
            mom.to_f64()
        );
    }

    #[test]
    fn z_movement_fixed_applies_gravity_and_floor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.math_mode = SimMathMode::VanillaFixed;
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(64.0, 64.0, 40.0);
            a.vel = DVec3::ZERO;
        }
        z_movement_fixed(&mut w, mo);
        let a = w.actors.get(mo).unwrap();
        // Doom: z unchanged on first aloft tic; momz becomes -2*GRAVITY.
        assert!((a.vel.z() + 2.0).abs() < 1e-9, "vel_z={}", a.vel.z());
        assert!((a.pos.z() - 40.0).abs() < 1e-6, "z={}", a.pos.z());
        z_movement_fixed(&mut w, mo);
        let a = w.actors.get(mo).unwrap();
        assert!((a.pos.z() - 38.0).abs() < 1e-6, "z={}", a.pos.z());
        assert!((a.vel.z() + 3.0).abs() < 1e-9, "vel_z={}", a.vel.z());
        for _ in 0..80 {
            z_movement_fixed(&mut w, mo);
        }
        let a = w.actors.get(mo).unwrap();
        assert!(
            (a.pos.z()).abs() < 1e-6,
            "should rest on floor, z={}",
            a.pos.z()
        );
        assert!(a.vel.z().abs() < 1e-9);
    }

    #[test]
    fn slide_move_fixed_glides_along_wall() {
        // West wall at x=0; start near it, mom west+north → slide north.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(20.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.math_mode = SimMathMode::VanillaFixed;
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(24.0, 64.0, 0.0);
            a.vel = DVec3::new(-20.0, 10.0, 0.0);
        }
        let actor = w.actors.get(mo).unwrap().clone();
        let probe = check_position_fixed(&w, &actor, DVec3::new(4.0, 74.0, 0.0), Some(mo));
        assert!(
            !probe.ok,
            "west-wall probe should block (radius={} blocking={:?})",
            actor.radius, probe.blocking_line
        );
        let y0 = w.actors.get(mo).unwrap().pos.y();
        let x0 = w.actors.get(mo).unwrap().pos.x();
        xy_movement_fixed(&mut w, mo);
        let a = w.actors.get(mo).unwrap();
        // Direct west try-move must fail; slide keeps x from diving through the wall.
        assert!(
            a.pos.x() > 8.0,
            "must not pass through west wall, x={} from {x0}",
            a.pos.x()
        );
        assert!(
            a.pos.y() >= y0,
            "slide should not reverse north mom, y={} y0={y0}",
            a.pos.y()
        );
    }
}
