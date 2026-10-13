//! Movement / collision (`P_CheckPosition` / `P_TryMove` subset).

use crate::actor::{Actor, ActorId};
use crate::class::{FLAG_MISSILE, FLAG_NOBLOCKMAP};
use crate::world::World;
use ferrite_core::{DVec2, DVec3, LineId};
use ferrite_map::{Blockmap, Line, MapGeometry};
use std::collections::BTreeSet;

/// Vanilla `ORIG_FRICTION` (~0xE800 / 65536).
const ORIG_FRICTION: f64 = 0.90625;
/// Vanilla `STOPSPEED` (~0x1000 / 65536) — zero XY below this after friction.
const STOPSPEED: f64 = 0.0625;
/// Default `MaxStepHeight` used when probing 3D-floor slabs in `check_position`.
const DEFAULT_STEP: f64 = 24.0;

/// Result of [`check_position`].
#[derive(Clone, Debug, Default)]
pub struct CheckResult {
    /// True if the position is free of blocking geometry.
    pub ok: bool,
    /// Floor Z at the position (from contacted sectors; else actor floor).
    pub floor_z: f64,
    /// Ceiling Z at the position.
    pub ceiling_z: f64,
    /// Doom `tmdropoffz` — lowest floor among contacted openings.
    pub dropoff_z: f64,
    /// Blocking linedef, if any.
    pub blocking_line: Option<LineId>,
    /// Solid actor that blocked the move, if any.
    pub blocking_actor: Option<ActorId>,
}

/// Blockmap lines overlapping the actor's AABB at `pos`.
pub fn lines_near(map: &MapGeometry, pos: DVec2, radius: f64) -> Vec<LineId> {
    let bm = &map.blockmap;
    if bm.columns == 0 || bm.rows == 0 || bm.blocks.is_empty() {
        return map
            .lines
            .iter()
            .enumerate()
            .map(|(i, _)| LineId::from_raw(i as u32))
            .collect();
    }
    let min_x = pos.x() - radius;
    let max_x = pos.x() + radius;
    let min_y = pos.y() - radius;
    let max_y = pos.y() + radius;
    let (bx0, by0) = block_coords(bm, min_x, min_y);
    let (bx1, by1) = block_coords(bm, max_x, max_y);
    let mut out = Vec::new();
    for by in by0..=by1 {
        for bx in bx0..=bx1 {
            let idx = usize::from(by) * usize::from(bm.columns) + usize::from(bx);
            if let Some(cell) = bm.blocks.get(idx) {
                for &lid in cell {
                    if !out.contains(&lid) {
                        out.push(lid);
                    }
                }
            }
        }
    }
    out
}

/// Blockmap cell containing map point `(x, y)` (clamped into the grid).
#[must_use]
pub fn block_coords(bm: &Blockmap, x: f64, y: f64) -> (u16, u16) {
    let bx = ((x as i32 - i32::from(bm.origin_x)) / 128)
        .clamp(0, i32::from(bm.columns.saturating_sub(1))) as u16;
    let by = ((y as i32 - i32::from(bm.origin_y)) / 128)
        .clamp(0, i32::from(bm.rows.saturating_sub(1))) as u16;
    (bx, by)
}

/// Doom `P_PathTraverse` block walk — cells visited from `(x1,y1)` to `(x2,y2)`.
///
/// Things outside these cells are invisible to vanilla hitscan/aim (blockmap corner skip).
#[must_use]
pub fn path_traverse_blocks(bm: &Blockmap, x1: f64, y1: f64, x2: f64, y2: f64) -> Vec<(i32, i32)> {
    if bm.columns == 0 || bm.rows == 0 {
        return Vec::new();
    }
    use ferrite_core::Fixed;
    const FRACUNIT: i32 = 1 << 16;
    const MAPBLOCKSIZE: i32 = 128 << 16;
    const MAPBLOCKSHIFT: i32 = 16 + 7;
    const MAPBTOFRAC: i32 = MAPBLOCKSHIFT - 16;

    let org_x = i32::from(bm.origin_x) * FRACUNIT;
    let org_y = i32::from(bm.origin_y) * FRACUNIT;

    let mut x1f = Fixed::from_f64(x1).0;
    let mut y1f = Fixed::from_f64(y1).0;
    let x2f = Fixed::from_f64(x2).0;
    let y2f = Fixed::from_f64(y2).0;

    // Don't sit exactly on a block boundary.
    if (x1f - org_x) & (MAPBLOCKSIZE - 1) == 0 {
        x1f = x1f.wrapping_add(FRACUNIT);
    }
    if (y1f - org_y) & (MAPBLOCKSIZE - 1) == 0 {
        y1f = y1f.wrapping_add(FRACUNIT);
    }

    let x1b = x1f.wrapping_sub(org_x);
    let y1b = y1f.wrapping_sub(org_y);
    let xt1 = x1b >> MAPBLOCKSHIFT;
    let yt1 = y1b >> MAPBLOCKSHIFT;

    let x2b = x2f.wrapping_sub(org_x);
    let y2b = y2f.wrapping_sub(org_y);
    let xt2 = x2b >> MAPBLOCKSHIFT;
    let yt2 = y2b >> MAPBLOCKSHIFT;

    let fixed_div = |a: i32, b: i32| (Fixed(a) / Fixed(b)).0;
    let fixed_mul = |a: i32, b: i32| Fixed(a).mul(Fixed(b)).0;

    let (mapxstep, ystep, mut yintercept) = if xt2 > xt1 {
        let partial = FRACUNIT - ((x1b >> MAPBTOFRAC) & (FRACUNIT - 1));
        let ystep = fixed_div(
            y2b.wrapping_sub(y1b),
            (x2b.wrapping_sub(x1b)).wrapping_abs(),
        );
        (
            1,
            ystep,
            (y1b >> MAPBTOFRAC).wrapping_add(fixed_mul(partial, ystep)),
        )
    } else if xt2 < xt1 {
        let partial = (x1b >> MAPBTOFRAC) & (FRACUNIT - 1);
        let ystep = fixed_div(
            y2b.wrapping_sub(y1b),
            (x2b.wrapping_sub(x1b)).wrapping_abs(),
        );
        (
            -1,
            ystep,
            (y1b >> MAPBTOFRAC).wrapping_add(fixed_mul(partial, ystep)),
        )
    } else {
        (0, 256 * FRACUNIT, y1b >> MAPBTOFRAC)
    };

    let (mapystep, xstep, mut xintercept) = if yt2 > yt1 {
        let partial = FRACUNIT - ((y1b >> MAPBTOFRAC) & (FRACUNIT - 1));
        let xstep = fixed_div(
            x2b.wrapping_sub(x1b),
            (y2b.wrapping_sub(y1b)).wrapping_abs(),
        );
        (
            1,
            xstep,
            (x1b >> MAPBTOFRAC).wrapping_add(fixed_mul(partial, xstep)),
        )
    } else if yt2 < yt1 {
        let partial = (y1b >> MAPBTOFRAC) & (FRACUNIT - 1);
        let xstep = fixed_div(
            x2b.wrapping_sub(x1b),
            (y2b.wrapping_sub(y1b)).wrapping_abs(),
        );
        (
            -1,
            xstep,
            (x1b >> MAPBTOFRAC).wrapping_add(fixed_mul(partial, xstep)),
        )
    } else {
        (0, 256 * FRACUNIT, x1b >> MAPBTOFRAC)
    };

    let mut mapx = xt1;
    let mut mapy = yt1;
    let mut out = Vec::new();
    for _ in 0..64 {
        out.push((mapx, mapy));
        if mapx == xt2 && mapy == yt2 {
            break;
        }
        // Chocolate Doom: only one of these branches; no corner both-step.
        if (yintercept >> 16) == mapy {
            yintercept = yintercept.wrapping_add(ystep);
            mapx += mapxstep;
        } else if (xintercept >> 16) == mapx {
            xintercept = xintercept.wrapping_add(xstep);
            mapy += mapystep;
        }
    }
    let _ = mapystep;
    out
}

fn line_blocks(
    world: &World,
    lid: LineId,
    line: &Line,
    actor: &Actor,
    self_id: Option<ActorId>,
) -> bool {
    if world
        .map
        .setup
        .portals
        .lines
        .iter()
        .any(|p| p.line == lid && p.is_traversable())
    {
        return false;
    }
    let flags = world.line_flags(lid);
    if line.back_sector.is_none() || (flags & crate::world::ML_BLOCKING) != 0 {
        return true;
    }
    if (flags & crate::world::ML_BLOCKEVERYTHING) != 0 {
        return true;
    }
    let is_player = self_id.is_some_and(|id| {
        world
            .players
            .iter()
            .any(|p| p.as_ref().and_then(|pl| pl.mo) == Some(id))
    });
    if (flags & crate::world::ML_BLOCK_PLAYERS) != 0 && is_player {
        return true;
    }
    if (flags & crate::world::ML_BLOCKMONSTERS) != 0
        && !is_player
        && (actor.flags & crate::class::FLAG_COUNTKILL) != 0
    {
        return true;
    }
    // UDMF `blockfloaters` / ZDoom ML_BLOCK_FLOATERS.
    if line.block_floaters && (actor.flags & crate::class::FLAG_FLOAT) != 0 {
        return true;
    }
    // UDMF `blockprojectiles` / ZDoom ML_BLOCKPROJECTILE.
    if line.block_projectiles && (actor.flags & FLAG_MISSILE) != 0 {
        return true;
    }
    false
}

/// Closest distance from point to line segment.
fn dist_point_to_seg(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = DVec2::new(b.x() - a.x(), b.y() - a.y());
    let ap = DVec2::new(p.x() - a.x(), p.y() - a.y());
    let len2 = ab.x() * ab.x() + ab.y() * ab.y();
    if len2 <= f64::EPSILON {
        let dx = p.x() - a.x();
        let dy = p.y() - a.y();
        return (dx * dx + dy * dy).sqrt();
    }
    let t = ((ap.x() * ab.x() + ap.y() * ab.y()) / len2).clamp(0.0, 1.0);
    let cx = a.x() + ab.x() * t;
    let cy = a.y() + ab.y() * t;
    let dx = p.x() - cx;
    let dy = p.y() - cy;
    (dx * dx + dy * dy).sqrt()
}

/// Solid thing flag (`MF_SOLID`).
const FLAG_SOLID: u64 = 1 << 0;

/// `P_CheckPosition` subset: solid lines + solid things.
pub fn check_position(
    world: &World,
    actor: &Actor,
    pos: DVec3,
    self_id: Option<ActorId>,
) -> CheckResult {
    let xy = DVec2::new(pos.x(), pos.y());
    let mut result = CheckResult {
        ok: true,
        floor_z: actor.pos.z(),
        ceiling_z: actor.pos.z() + actor.height,
        dropoff_z: actor.pos.z(),
        blocking_line: None,
        blocking_actor: None,
    };
    // Seed from the sector under the actor centre (Doom `P_CheckPosition`), then
    // raise/lower with `P_LineOpening`-style contacts. Seeding only from linedef
    // *fronts* ignored backs — near a ledge whose front faces the low sector the
    // player would inherit that floor and fall through the high floor they stand on.
    let mut saw_sector = false;
    let mut contacted: BTreeSet<usize> = BTreeSet::new();
    if let Some(sid) = ferrite_nodebuild::point_in_sector(&world.map, pos.x(), pos.y())
        .or_else(|| ferrite_nodebuild::point_in_sector_vanilla(&world.map, pos.x(), pos.y()))
    {
        let si = sid.raw() as usize;
        if world.sectors.get(si).is_some() {
            contacted.insert(si);
            result.floor_z = sector_z_at(world, si, pos.x(), pos.y(), true);
            result.ceiling_z = sector_z_at(world, si, pos.x(), pos.y(), false);
            result.dropoff_z = result.floor_z;
            saw_sector = true;
        }
    }
    for lid in lines_near(&world.map, xy, actor.radius) {
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        let front = line
            .front_sector
            .and_then(|sid| world.sectors.get(sid.raw() as usize).map(|_| sid));
        let back = line
            .back_sector
            .and_then(|sid| world.sectors.get(sid.raw() as usize).map(|_| sid));
        if let Some(sid) = front {
            contacted.insert(sid.raw() as usize);
        }
        if let Some(sid) = back {
            contacted.insert(sid.raw() as usize);
        }
        // Opening: only when the actor AABB actually reaches the seg.
        let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        let touches = dist_point_to_seg(xy, v1.pos, v2.pos) < actor.radius;
        if touches {
            match (front, back) {
                (Some(f), Some(b)) => {
                    let fi = f.raw() as usize;
                    let bi = b.raw() as usize;
                    let f_floor = sector_z_at(world, fi, pos.x(), pos.y(), true);
                    let b_floor = sector_z_at(world, bi, pos.x(), pos.y(), true);
                    let f_ceil = sector_z_at(world, fi, pos.x(), pos.y(), false);
                    let b_ceil = sector_z_at(world, bi, pos.x(), pos.y(), false);
                    let open_bottom = f_floor.max(b_floor);
                    let open_top = f_ceil.min(b_ceil);
                    let low_floor = f_floor.min(b_floor);
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
                    let fi = f.raw() as usize;
                    let floor_z = sector_z_at(world, fi, pos.x(), pos.y(), true);
                    let ceiling_z = sector_z_at(world, fi, pos.x(), pos.y(), false);
                    if !saw_sector {
                        result.floor_z = floor_z;
                        result.ceiling_z = ceiling_z;
                        result.dropoff_z = floor_z;
                        saw_sector = true;
                    }
                }
                (None, None) => {}
            }
        }
        if !line_blocks(world, lid, line, actor, self_id) {
            continue;
        }
        if touches {
            result.ok = false;
            result.blocking_line = Some(lid);
            return result;
        }
    }
    // Thing-thing: block on solid actors whose XY disks overlap.
    for id in &world.thinkers.default {
        if self_id == Some(*id) {
            continue;
        }
        let Some(other) = world.actors.get(*id) else {
            continue;
        };
        if other.destroyed
            || (other.flags & FLAG_SOLID) == 0
            || (other.flags & FLAG_NOBLOCKMAP) != 0
        {
            continue;
        }
        let dx = other.pos.x() - pos.x();
        let dy = other.pos.y() - pos.y();
        let r = other.radius + actor.radius;
        if dx * dx + dy * dy < r * r {
            // Vertical overlap check (no step-up yet).
            let top = pos.z() + actor.height;
            let obot = other.pos.z();
            let otop = obot + other.height;
            if top > obot && pos.z() < otop {
                result.ok = false;
                result.blocking_actor = Some(*id);
                return result;
            }
        }
    }
    if !saw_sector {
        // Fall back to sector 0 if present.
        if !world.sectors.is_empty() {
            contacted.insert(0);
            result.floor_z = sector_z_at(world, 0, pos.x(), pos.y(), true);
            result.ceiling_z = sector_z_at(world, 0, pos.x(), pos.y(), false);
        }
    }
    apply_solid_ffloors(world, actor, pos, &contacted, &mut result);
    apply_midtex3d(world, actor, pos, xy, &mut result);
    result
}

/// Bootstrap 3D midtexture: two-sided `midtex3d` lines act as a 64mu-tall walkable slab
/// (floor-sitting by default; `Sector_Attach3dMidtex` ceiling arg hangs from the plane).
fn apply_midtex3d(world: &World, actor: &Actor, pos: DVec3, xy: DVec2, result: &mut CheckResult) {
    const MID_H: f64 = 64.0;
    let feet = pos.z();
    let head = feet + actor.height;
    for lid in lines_near(&world.map, xy, actor.radius) {
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        if !line.midtex3d {
            continue;
        }
        let (Some(fs), Some(bs)) = (line.front_sector, line.back_sector) else {
            continue;
        };
        let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        if dist_point_to_seg(xy, v1.pos, v2.pos) > actor.radius + 1.0 {
            continue;
        }
        let ff = sector_z_at(world, fs.raw() as usize, pos.x(), pos.y(), true);
        let bf = sector_z_at(world, bs.raw() as usize, pos.x(), pos.y(), true);
        let fc = sector_z_at(world, fs.raw() as usize, pos.x(), pos.y(), false);
        let bc = sector_z_at(world, bs.raw() as usize, pos.x(), pos.y(), false);
        let open_bot = ff.max(bf);
        let open_top = fc.min(bc);
        if open_top <= open_bot + 1e-6 {
            continue;
        }
        let (mid_bot, mid_top) = midtex_slab_span(world, lid.raw(), open_bot, open_top, MID_H);
        if mid_top <= mid_bot + 1e-6 {
            continue;
        }
        if feet >= mid_top - 1e-6 {
            result.floor_z = result.floor_z.max(mid_top);
        } else if head <= mid_bot + 1e-6 {
            result.ceiling_z = result.ceiling_z.min(mid_bot);
        } else if mid_top - feet <= DEFAULT_STEP {
            result.floor_z = result.floor_z.max(mid_top);
        } else if head > mid_bot && feet < mid_top {
            // `midtex3dimpassible`: block like a short wall; missiles pass through.
            let missile = (actor.flags & FLAG_MISSILE) != 0;
            if line.midtex3d_impassible {
                if !missile {
                    result.ok = false;
                    result.blocking_line = Some(lid);
                }
            } else if !missile {
                result.ok = false;
                result.blocking_line = Some(lid);
            }
        }
    }
    if result.ceiling_z - result.floor_z < actor.height {
        result.ok = false;
    }
}

/// Midtex vertical span: default sits on the opening floor; `Sector_Attach3dMidtex`
/// with ceiling≠0 hangs from the attached sector's ceiling plane.
fn midtex_slab_span(
    world: &World,
    line_idx: u32,
    open_bot: f64,
    open_top: f64,
    mid_h: f64,
) -> (f64, f64) {
    for attach in &world.map.setup.midtex_attach {
        if !attach.lines.iter().any(|&i| i == line_idx) {
            continue;
        }
        let si = attach.sector.raw() as usize;
        if attach.ceiling {
            let cz = world
                .sectors
                .get(si)
                .map(|s| s.ceiling_z)
                .unwrap_or(open_top);
            let mid_top = cz.min(open_top);
            let mid_bot = (mid_top - mid_h).max(open_bot);
            return (mid_bot, mid_top);
        }
        let fz = world.sectors.get(si).map(|s| s.floor_z).unwrap_or(open_bot);
        let mid_bot = fz.clamp(open_bot, open_top);
        let mid_top = (mid_bot + mid_h).min(open_top);
        return (mid_bot, mid_top);
    }
    (open_bot, (open_bot + mid_h).min(open_top))
}

/// Raise floor / lower ceiling for solid 3D floors in contacted sectors.
fn apply_solid_ffloors(
    world: &World,
    actor: &Actor,
    pos: DVec3,
    contacted: &BTreeSet<usize>,
    result: &mut CheckResult,
) {
    if world.map.setup.ffloors.is_empty() {
        return;
    }
    let feet = pos.z();
    let head = feet + actor.height;
    for ff in &world.map.setup.ffloors {
        if !ff.is_solid() {
            continue;
        }
        let ti = ff.target.raw() as usize;
        if !contacted.is_empty() && !contacted.contains(&ti) {
            continue;
        }
        let ci = ff.control.raw() as usize;
        let a = sector_z_at(world, ci, pos.x(), pos.y(), true);
        let b = sector_z_at(world, ci, pos.x(), pos.y(), false);
        let bot = a.min(b);
        let top = a.max(b);
        if top <= bot {
            continue;
        }
        if feet >= top - 1e-6 {
            result.floor_z = result.floor_z.max(top);
        } else if head <= bot + 1e-6 {
            result.ceiling_z = result.ceiling_z.min(bot);
        } else if top - feet <= DEFAULT_STEP {
            result.floor_z = result.floor_z.max(top);
        } else {
            result.ceiling_z = result.ceiling_z.min(bot);
            if head > bot && feet < top {
                result.ok = false;
            }
        }
    }
    if result.ceiling_z - result.floor_z < actor.height {
        result.ok = false;
    }
}

/// Floor Z under `(x, y)` for the sector containing the point (slopes + movers).
#[must_use]
pub fn floor_at_xy(world: &World, x: f64, y: f64) -> Option<f64> {
    let sid = ferrite_nodebuild::point_in_sector(&world.map, x, y)
        .or_else(|| ferrite_nodebuild::point_in_sector_vanilla(&world.map, x, y))?;
    Some(sector_z_at(world, sid.raw() as usize, x, y, true))
}

/// Ceiling Z above `(x, y)` for the sector containing the point.
#[must_use]
pub fn ceiling_at_xy(world: &World, x: f64, y: f64) -> Option<f64> {
    let sid = ferrite_nodebuild::point_in_sector(&world.map, x, y)
        .or_else(|| ferrite_nodebuild::point_in_sector_vanilla(&world.map, x, y))?;
    Some(sector_z_at(world, sid.raw() as usize, x, y, false))
}

/// Floor or ceiling Z at XY: sloped setup plane plus mover delta.
fn sector_z_at(world: &World, si: usize, x: f64, y: f64, floor: bool) -> f64 {
    let dyn_z = world
        .sectors
        .get(si)
        .map(|s| if floor { s.floor_z } else { s.ceiling_z })
        .unwrap_or(0.0);
    let static_h = world
        .map
        .sectors
        .get(si)
        .map(|s| {
            if floor {
                s.floor_height
            } else {
                s.ceiling_height
            }
        })
        .unwrap_or(0.0);
    let plane_z = world
        .map
        .setup
        .planes
        .get(si)
        .map(|p| {
            if floor {
                p.floor.z_at(x, y)
            } else {
                p.ceiling.z_at(x, y)
            }
        })
        .unwrap_or(static_h);
    plane_z + (dyn_z - static_h)
}

/// True if `actor_id` is a possessed player with `noclip` on.
fn actor_noclip(world: &World, actor_id: ActorId) -> bool {
    world
        .players
        .iter()
        .flatten()
        .any(|p| p.noclip && p.mo == Some(actor_id))
}

/// Outcome of [`try_move`].
#[derive(Clone, Copy, Debug)]
pub struct TryMoveResult {
    /// Relocated successfully.
    pub ok: bool,
    /// Solid actor that blocked, if any.
    pub blocking_actor: Option<ActorId>,
}

/// `P_TryMove` subset: relocate if [`check_position`] succeeds.
pub fn try_move(world: &mut World, actor_id: ActorId, new_pos: DVec3) -> bool {
    try_move_ex(world, actor_id, new_pos).ok
}

/// Like [`try_move`], but reports which actor blocked the move.
pub fn try_move_ex(world: &mut World, actor_id: ActorId, new_pos: DVec3) -> TryMoveResult {
    if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
        return crate::vanilla::try_move_fixed(world, actor_id, new_pos);
    }
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return TryMoveResult {
            ok: false,
            blocking_actor: None,
        };
    };
    let from = DVec2::new(actor.pos.x(), actor.pos.y());
    let to = DVec2::new(new_pos.x(), new_pos.y());
    let mut dest = new_pos;
    if !actor_noclip(world, actor_id) {
        let check = check_position(world, &actor, new_pos, Some(actor_id));
        if !check.ok {
            return TryMoveResult {
                ok: false,
                blocking_actor: check.blocking_actor,
            };
        }
        let max_step = if actor.max_step_height > 0.0 {
            actor.max_step_height
        } else {
            world
                .classes
                .get(actor.class)
                .map(|c| c.max_step_height)
                .unwrap_or(24.0)
        };
        let max_drop = if actor.max_dropoff_height > 0.0 {
            actor.max_dropoff_height
        } else {
            world
                .classes
                .get(actor.class)
                .map(|c| c.max_dropoff_height)
                .unwrap_or(24.0)
        };
        let delta = check.floor_z - actor.pos.z();
        if delta > max_step {
            return TryMoveResult {
                ok: false,
                blocking_actor: None,
            };
        }
        let floating = (actor.flags & crate::class::FLAG_FLOAT) != 0
            || (actor.flags & crate::class::FLAG_NOGRAVITY) != 0;
        let can_dropoff = (actor.flags & crate::class::FLAG_DROPOFF) != 0;
        if !floating && !can_dropoff && delta < -max_drop {
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
        // Step up onto the new floor when climbing.
        if delta > 0.0 {
            dest = DVec3::new(new_pos.x(), new_pos.y(), check.floor_z);
        }
    }
    if let Some(a) = world.actors.get_mut(actor_id) {
        a.pos = dest;
    }
    crate::cross_lines::cross_special_lines(world, actor_id, from, to);
    TryMoveResult {
        ok: true,
        blocking_actor: None,
    }
}

/// Max XY step before subdivision (`MAXMOVE` = 30 in Doom).
const MAXMOVE: f64 = 30.0;

/// Double-math stairstep: try pure-Y then pure-X (Doom `P_SlideMove` last resort).
///
/// Returns `(keep_momx, keep_momy)` for whichever axis still moved.
fn slide_stairstep_double(world: &mut World, actor_id: ActorId, dx: f64, dy: f64) -> (bool, bool) {
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return (false, false);
    };
    let mut keep_y = false;
    let dest_y = DVec3::new(actor.pos.x(), actor.pos.y() + dy, actor.pos.z());
    if try_move_ex(world, actor_id, dest_y).ok {
        keep_y = true;
    }
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return (false, keep_y);
    };
    let mut keep_x = false;
    let dest_x = DVec3::new(actor.pos.x() + dx, actor.pos.y(), actor.pos.z());
    if try_move_ex(world, actor_id, dest_x).ok {
        keep_x = true;
    }
    (keep_x, keep_y)
}

/// Doom `P_HitSlideLine` in double math — project mom onto the linedef direction.
fn hit_slide_line_double(
    world: &World,
    lid: LineId,
    dx: f64,
    dy: f64,
    actor_x: f64,
    actor_y: f64,
) -> (f64, f64) {
    let Some(line) = world.map.lines.get(lid.raw() as usize) else {
        return (dx, dy);
    };
    let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
        return (dx, dy);
    };
    let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
        return (dx, dy);
    };
    let ldx = v2.pos.x() - v1.pos.x();
    let ldy = v2.pos.y() - v1.pos.y();
    if ldy.abs() < 1e-9 {
        return (dx, 0.0);
    }
    if ldx.abs() < 1e-9 {
        return (0.0, dy);
    }
    // Flip line direction when the actor is on the back side (Doom `P_HitSlideLine`).
    let side = {
        let cross = (actor_x - v1.pos.x()) * ldy - (actor_y - v1.pos.y()) * ldx;
        i32::from(cross > 0.0)
    };
    let (ldx, ldy) = if side == 1 { (-ldx, -ldy) } else { (ldx, ldy) };
    let len2 = ldx * ldx + ldy * ldy;
    if len2 < 1e-12 {
        return (0.0, 0.0);
    }
    let t = (dx * ldx + dy * ldy) / len2;
    (t * ldx, t * ldy)
}

/// Double-math `P_SlideMove`: clip against walls, then stairstep with full mom.
fn slide_move_double(world: &mut World, actor_id: ActorId, mut dx: f64, mut dy: f64) -> (bool, bool) {
    for _ in 0..3 {
        let Some(actor) = world.actors.get(actor_id).cloned() else {
            return (false, false);
        };
        let dest = DVec3::new(actor.pos.x() + dx, actor.pos.y() + dy, actor.pos.z());
        if try_move_ex(world, actor_id, dest).ok {
            return (dx.abs() > 1e-9, dy.abs() > 1e-9);
        }
        let check = check_position(world, &actor, dest, Some(actor_id));
        let lid = check.blocking_line.or_else(|| {
            // Step-height / opening blocks often leave `blocking_line` unset — pick
            // the nearest solid line overlapping the move AABB.
            let mid = DVec2::new(actor.pos.x() + dx * 0.5, actor.pos.y() + dy * 0.5);
            let near = lines_near(&world.map, mid, actor.radius + dx.abs().max(dy.abs()) + 8.0);
            near.into_iter().find(|&lid| {
                let Some(line) = world.map.lines.get(lid.raw() as usize) else {
                    return false;
                };
                if line.back_sector.is_none() {
                    return true;
                }
                let flags = world.line_flags(lid);
                (flags & crate::world::ML_BLOCKING) != 0
            })
        });
        let Some(lid) = lid else {
            return slide_stairstep_double(world, actor_id, dx, dy);
        };
        // Nudge toward the wall (Doom `bestslidefrac - 0x800` ≈ 3% shorten).
        let approach = DVec3::new(
            actor.pos.x() + dx * 0.97,
            actor.pos.y() + dy * 0.97,
            actor.pos.z(),
        );
        let _ = try_move_ex(world, actor_id, approach);
        let Some(actor) = world.actors.get(actor_id).cloned() else {
            return (false, false);
        };
        let (sx, sy) = hit_slide_line_double(world, lid, dx, dy, actor.pos.x(), actor.pos.y());
        if sx.abs() < 1e-6 && sy.abs() < 1e-6 {
            return slide_stairstep_double(world, actor_id, dx, dy);
        }
        dx = sx;
        dy = sy;
        let slide_dest = DVec3::new(actor.pos.x() + sx, actor.pos.y() + sy, actor.pos.z());
        if try_move_ex(world, actor_id, slide_dest).ok {
            return (sx.abs() > 1e-9, sy.abs() > 1e-9);
        }
    }
    slide_stairstep_double(world, actor_id, dx, dy)
}

/// Doom gravity units per tic² (before level gravity scale).
const GRAVITY: f64 = 1.0;

/// Sector light for an actor, honouring 3D-floor control lights below/inside slabs.
#[must_use]
pub fn actor_light_level(world: &World, a: &Actor) -> i32 {
    let Some(sid) = ferrite_nodebuild::point_in_sector(&world.map, a.pos.x(), a.pos.y())
        .or_else(|| ferrite_nodebuild::point_in_sector_vanilla(&world.map, a.pos.x(), a.pos.y()))
    else {
        return 0;
    };
    let si = sid.raw() as usize;
    let base = world
        .sectors
        .get(si)
        .map(|s| i32::from(s.light))
        .unwrap_or(0);
    if world.map.setup.ffloors.is_empty() {
        return base;
    }
    let feet = a.pos.z();
    let head = feet + a.height;
    let mid = feet + a.height * 0.5;
    let mut light = base;
    for ff in &world.map.setup.ffloors {
        if ff.target.raw() as usize != si {
            continue;
        }
        let ci = ff.control.raw() as usize;
        let Some(ctrl) = world.sectors.get(ci) else {
            continue;
        };
        let bot = ctrl.floor_z.min(ctrl.ceiling_z);
        let top = ctrl.floor_z.max(ctrl.ceiling_z);
        if top <= bot + 1e-6 {
            continue;
        }
        let ctrl_light = i32::from(ctrl.light);
        if ff.is_swimmable() {
            // Immersed in the water volume → control light.
            if head > bot + 1e-6 && feet < top - 1e-6 {
                light = ctrl_light;
            }
        } else if ff.is_solid() {
            // Standing in the airspace under the slab → control light (double-shadow-ish).
            if mid < bot {
                light = ctrl_light;
            }
        } else if ff.is_light_transfer() {
            // ExtraFloor_LightOnly / type-3: control light below the top plane.
            if mid < top {
                light = ctrl_light;
            }
        }
    }
    light
}

/// Water immersion 0..=3 (Transfer_Heights fake floor + swimmable 3D floors).
#[must_use]
pub fn actor_waterlevel(world: &World, a: &Actor) -> i32 {
    let Some(sid) = ferrite_nodebuild::point_in_sector(&world.map, a.pos.x(), a.pos.y())
        .or_else(|| ferrite_nodebuild::point_in_sector_vanilla(&world.map, a.pos.x(), a.pos.y()))
    else {
        return 0;
    };
    let si = sid.raw() as usize;
    let feet = a.pos.z();
    let mid = feet + a.height * 0.5;
    let head = feet + a.height;
    let mut best = 0i32;

    if let Some(sec) = world.sectors.get(si) {
        if let Some(surface) = sec.fake_floor_z {
            if surface > sec.floor_z + 1e-6 {
                best = best.max(waterlevel_from_surface(feet, mid, head, surface));
            }
        }
        // UDMF `waterzone` — treat dynamic ceiling as water surface.
        if world.map.sectors.get(si).is_some_and(|s| s.waterzone) {
            best = best.max(waterlevel_from_surface(feet, mid, head, sec.ceiling_z));
        }
    }

    for ff in &world.map.setup.ffloors {
        if !ff.is_swimmable() || ff.target.raw() as usize != si {
            continue;
        }
        let ci = ff.control.raw() as usize;
        let bot = world
            .sectors
            .get(ci)
            .map(|s| s.floor_z.min(s.ceiling_z))
            .unwrap_or(0.0);
        let top = world
            .sectors
            .get(ci)
            .map(|s| s.floor_z.max(s.ceiling_z))
            .unwrap_or(0.0);
        if top <= bot + 1e-6 {
            continue;
        }
        if head <= bot + 1e-6 || feet >= top - 1e-6 {
            continue;
        }
        best = best.max(waterlevel_from_surface(feet, mid, head, top));
    }
    best
}

fn waterlevel_from_surface(feet: f64, mid: f64, head: f64, surface: f64) -> i32 {
    if surface <= feet {
        0
    } else if surface <= mid {
        1
    } else if surface < head {
        2
    } else {
        3
    }
}

/// Gravity scale while immersed (1.0 = dry).
fn water_gravity_scale(level: i32) -> f64 {
    match level {
        0 => 1.0,
        1 => 0.75,
        2 => 0.5,
        _ => 0.25,
    }
}

/// Hexen `P_FallingDamage` amount from impact `momz` (map units / tic).
#[must_use]
pub fn hexen_falling_damage(momz: f64, health: i32, mass: i32) -> i32 {
    let mom = momz.abs();
    if mom < 23.0 {
        return 0;
    }
    let mut damage = if mom >= 63.0 {
        1_000_000
    } else {
        let dist = mom * 16.0 / 23.0;
        let d = (dist * dist) / 10.0 - 24.0;
        d.max(0.0).round() as i32
    };
    // Soft-death threshold: leave 1 HP when impact is milder than −35.
    if momz > -35.0 && damage > health && health != 1 {
        damage = health - 1;
    }
    let mass = mass.max(1);
    (i64::from(damage) * i64::from(mass) / 100).clamp(0, i64::from(i32::MAX)) as i32
}

/// Apply Hexen falling damage if the level enables it and the floor allows it.
pub(crate) fn try_falling_damage(world: &mut World, actor_id: ActorId, impact_vel_z: f64) {
    if !world.level.falling_damage || impact_vel_z >= -23.0 {
        return;
    }
    let Some(actor) = world.actors.get(actor_id) else {
        return;
    };
    // Players only for now (Hexen also damages monsters; M8 can widen).
    let is_player = world
        .players
        .iter()
        .flatten()
        .any(|p| p.mo == Some(actor_id));
    if !is_player {
        return;
    }
    let sid = ferrite_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y()).or_else(
        || ferrite_nodebuild::point_in_sector_vanilla(&world.map, actor.pos.x(), actor.pos.y()),
    );
    if let Some(sid) = sid {
        if world
            .map
            .sectors
            .get(sid.raw() as usize)
            .is_some_and(|s| s.no_falling_damage)
        {
            return;
        }
    }
    let health = actor.health;
    let mass = if actor.mass > 0 {
        actor.mass
    } else {
        world
            .classes
            .get(actor.class)
            .map(|c| c.mass)
            .unwrap_or(100)
    };
    let dmg = hexen_falling_damage(impact_vel_z, health, mass);
    if dmg > 0 {
        crate::damage::damage_mobj(world, actor_id, None, None, dmg);
    }
}

/// `P_ZMovement` subset: gravity + floor/ceiling clamp (no splash / bounce).
pub fn z_movement(world: &mut World, actor_id: ActorId) {
    if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
        crate::vanilla::z_movement_fixed(world, actor_id);
        return;
    }
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return;
    };
    let planes = check_position(world, &actor, actor.pos, Some(actor_id));
    let floor_z = planes.floor_z;
    let ceiling_z = planes.ceiling_z;
    let mut vel_z = actor.vel.z();
    let water = actor_waterlevel(world, &actor);
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
        let sector_g = ferrite_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
            .or_else(|| {
                ferrite_nodebuild::point_in_sector_vanilla(&world.map, actor.pos.x(), actor.pos.y())
            })
            .and_then(|sid| world.sectors.get(sid.raw() as usize))
            .map(|s| s.gravity)
            .unwrap_or(1.0);
        vel_z -= GRAVITY * world.level.gravity * sector_g * actor_g * water_gravity_scale(water);
    }
    // Swim thrust from player `up_move` when waist-deep or deeper.
    if water >= 2 {
        if let Some(slot) = world
            .players
            .iter()
            .flatten()
            .find(|p| p.mo == Some(actor_id))
        {
            if slot.cmd.up_move != 0 {
                let speed = if actor.float_speed > 0.0 {
                    actor.float_speed
                } else {
                    4.0
                };
                let dir = if slot.cmd.up_move > 0 { 1.0 } else { -1.0 };
                vel_z += dir * speed;
            }
        }
        // Mild vertical drag in water.
        vel_z *= 0.9;
    }
    let mut z = actor.pos.z() + vel_z;
    if try_sector_plane_portal(world, actor_id, &actor, z, vel_z, floor_z, ceiling_z) {
        return;
    }
    let mut hard_land = false;
    let mut hit_floor = false;
    let mut hit_ceiling = false;
    let impact_vel_z = vel_z;
    let is_missile = (actor.flags & FLAG_MISSILE) != 0;
    if z <= floor_z {
        // Doom `P_ZMovement`: hard land (`*oof*`) when momz < -8.
        if actor.pos.z() > floor_z + 0.1 && vel_z < -8.0 {
            hard_land = world
                .players
                .iter()
                .flatten()
                .any(|p| p.mo == Some(actor_id));
        }
        if actor.pos.z() > floor_z + 0.1 && vel_z < 0.0 {
            hit_floor = true;
        }
        // Missiles already on/in the floor still explode (Doom `P_ZMovement`).
        if is_missile && z <= floor_z {
            hit_floor = true;
        }
        z = floor_z;
        vel_z = 0.0;
    }
    let head = z + actor.height;
    if head > ceiling_z {
        z = ceiling_z - actor.height;
        vel_z = 0.0;
        hit_ceiling = true;
    }
    if let Some(a) = world.actors.get_mut(actor_id) {
        a.pos = DVec3::new(a.pos.x(), a.pos.y(), z);
        a.vel = DVec3::new(a.vel.x(), a.vel.y(), vel_z);
    }
    if is_missile && (hit_floor || hit_ceiling) {
        explode_missile(world, actor_id);
        return;
    }
    if hard_land {
        world.play_sound(
            "player/male/land",
            actor.pos.x(),
            actor.pos.y(),
            floor_z,
            1.0,
        );
    }
    if hit_floor {
        try_falling_damage(world, actor_id, impact_vel_z);
        // Capture pre-clamp vel for splash size selection.
        let splash_vel = actor.vel.z();
        crate::terrain::hit_floor_splash(world, actor_id, splash_vel);
    }
}

fn try_sector_plane_portal(
    world: &mut World,
    actor_id: ActorId,
    actor: &Actor,
    z: f64,
    vel_z: f64,
    floor_z: f64,
    ceiling_z: f64,
) -> bool {
    if world.map.setup.portals.sectors.is_empty() {
        return false;
    }
    let Some(sid) = ferrite_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
    else {
        return false;
    };
    let portals: Vec<_> = world.map.setup.portals.sectors.clone();
    for p in portals {
        if p.sector != sid || !p.is_traversable() || p.target == p.sector {
            continue;
        }
        let through_floor = p.plane == 0 && vel_z < 0.0 && z <= floor_z;
        let through_ceil = p.plane == 1 && vel_z > 0.0 && z + actor.height >= ceiling_z;
        if !through_floor && !through_ceil {
            continue;
        }
        let Some(src_c) = sector_xy_center(&world.map, p.sector.raw() as usize) else {
            continue;
        };
        let Some(dst_c) = sector_xy_center(&world.map, p.target.raw() as usize) else {
            continue;
        };
        let nx = actor.pos.x() + (dst_c.x() - src_c.x());
        let ny = actor.pos.y() + (dst_c.y() - src_c.y());
        let dest_floor = sector_z_at(world, p.target.raw() as usize, nx, ny, true);
        let dest_ceil = sector_z_at(world, p.target.raw() as usize, nx, ny, false);
        let nz = if through_floor {
            dest_ceil - actor.height - 1.0
        } else {
            dest_floor + 1.0
        };
        if let Some(a) = world.actors.get_mut(actor_id) {
            a.pos = DVec3::new(nx, ny, nz);
            a.vel = DVec3::new(a.vel.x(), a.vel.y(), vel_z);
        }
        return true;
    }
    false
}

fn sector_xy_center(map: &MapGeometry, si: usize) -> Option<DVec2> {
    let sec = map.sectors.get(si)?;
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut n = 0.0;
    for &lid in &sec.lines {
        let Some(line) = map.lines.get(lid.raw() as usize) else {
            continue;
        };
        for vid in [line.v1, line.v2] {
            let Some(v) = map.vertices.get(vid.raw() as usize) else {
                continue;
            };
            sx += v.pos.x();
            sy += v.pos.y();
            n += 1.0;
        }
    }
    if n < 1.0 {
        return None;
    }
    Some(DVec2::new(sx / n, sy / n))
}

/// `P_XYMovement` subset: apply `vel.xy`, stop on block (no slide yet).
pub fn xy_movement(world: &mut World, actor_id: ActorId) {
    if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
        crate::vanilla::xy_movement_fixed(world, actor_id);
        return;
    }
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return;
    };
    let dx = actor.vel.x();
    let dy = actor.vel.y();
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    let sx = (dx.abs() / MAXMOVE).ceil() as i32;
    let sy = (dy.abs() / MAXMOVE).ceil() as i32;
    let steps = sx.max(sy).clamp(1, 8);
    let step_x = dx / f64::from(steps);
    let step_y = dy / f64::from(steps);
    let mut x = actor.pos.x();
    let mut y = actor.pos.y();
    let z = actor.pos.z();
    let is_missile = (actor.flags & FLAG_MISSILE) != 0;
    for _ in 0..steps {
        let nx = x + step_x;
        let ny = y + step_y;
        let mv = try_move_ex(world, actor_id, DVec3::new(nx, ny, z));
        if mv.ok {
            x = nx;
            y = ny;
        } else {
            if is_missile {
                if let Some(victim) = mv.blocking_actor {
                    apply_missile_damage(world, actor_id, victim);
                    let rip = match (world.actors.get(actor_id), world.actors.get(victim)) {
                        (Some(m), Some(v)) => {
                            crate::action::is_ripper_missile(m)
                                && crate::action::can_rip_through(m, v)
                        }
                        _ => false,
                    };
                    if rip {
                        // Ripper: damage and continue through the victim.
                        if let Some(a) = world.actors.get_mut(actor_id) {
                            a.pos = DVec3::new(nx, ny, z);
                        }
                        x = nx;
                        y = ny;
                        continue;
                    }
                }
                explode_missile(world, actor_id);
            } else {
                // Doom `P_SlideMove` uses full `momx/momy`, not the subdivision step.
                let (keep_x, keep_y) = slide_move_double(world, actor_id, dx, dy);
                if let Some(a) = world.actors.get_mut(actor_id) {
                    a.vel = DVec3::new(
                        if keep_x { a.vel.x() } else { 0.0 },
                        if keep_y { a.vel.y() } else { 0.0 },
                        a.vel.z(),
                    );
                }
            }
            break;
        }
    }
    // Ground friction for non-missiles (actor/class `Friction` × sector friction).
    if !is_missile {
        let class_fric = if actor.friction > 0.0 {
            actor.friction
        } else {
            world
                .classes
                .get(actor.class)
                .map(|c| c.friction)
                .unwrap_or(1.0)
        };
        let sector_fric =
            ferrite_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
                .and_then(|sid| world.sectors.get(sid.raw() as usize))
                .map(|s| s.friction)
                .unwrap_or(ORIG_FRICTION);
        let factor = (sector_fric * class_fric).clamp(0.0, 1.0);
        if let Some(a) = world.actors.get_mut(actor_id) {
            let (mut vx, mut vy) = if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
                use ferrite_core::math::Fixed;
                let f = Fixed::from_f64(factor);
                (
                    (Fixed::from_f64(a.vel.x()) * f).to_f64(),
                    (Fixed::from_f64(a.vel.y()) * f).to_f64(),
                )
            } else {
                (a.vel.x() * factor, a.vel.y() * factor)
            };
            if vx * vx + vy * vy < STOPSPEED * STOPSPEED {
                vx = 0.0;
                vy = 0.0;
            }
            a.vel = DVec3::new(vx, vy, a.vel.z());
        }
    }
}

/// Apply class `Damage` to a victim hit by a missile.
///
/// Vanilla: `(P_Random() % 8 + 1) * mobjinfo.damage` via `pr_missile`.
pub(crate) fn apply_missile_damage(world: &mut World, missile_id: ActorId, victim: ActorId) {
    let Some(missile) = world.actors.get(missile_id).cloned() else {
        return;
    };
    let Some(victim_a) = world.actors.get(victim).cloned() else {
        return;
    };
    let missile_sp = if !missile.species.is_empty() {
        missile.species.clone()
    } else {
        world
            .classes
            .get(missile.class)
            .map(|c| c.species.clone())
            .unwrap_or_default()
    };
    let victim_sp = if !victim_a.species.is_empty() {
        victim_a.species.clone()
    } else {
        world
            .classes
            .get(victim_a.class)
            .map(|c| c.species.clone())
            .unwrap_or_default()
    };
    if !missile_sp.is_empty() && missile_sp.eq_ignore_ascii_case(&victim_sp) {
        return;
    }
    let base = if missile.damage > 0 {
        missile.damage
    } else {
        world
            .classes
            .get(missile.class)
            .map(|c| c.damage)
            .unwrap_or(0)
    };
    if base <= 0 {
        return;
    }
    let roll = i32::from(world.p_random("pr_missile")) % 8 + 1;
    let dmg = base * roll;
    // Doom `PIT_CheckThing`: no puff/blood on missile hit — only damage + explode.
    // Source is the shooter (`tmthing->target`), inflictor is the missile.
    let source = missile.target;
    let _ = crate::damage::damage_mobj(world, victim, Some(missile_id), source, dmg);
}

/// Enter Death (or destroy) and zero velocity when a missile hits geometry.
///
/// Doom `P_ExplodeMissile`: clear mom → death state → `tics -= P_Random()&3` (min 1).
pub(crate) fn explode_missile(world: &mut World, actor_id: ActorId) {
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return;
    };
    if let Some(a) = world.actors.get_mut(actor_id) {
        a.vel = DVec3::ZERO;
        a.flags &= !FLAG_MISSILE;
    }
    if let Some(death) = world.states.find_label(actor.class, "Death") {
        world.set_state(actor_id, Some(death), true);
    } else if let Some(a) = world.actors.get_mut(actor_id) {
        a.destroyed = true;
    }
    // Doom trims tics *after* `P_SetMobjState` installs the death frame.
    if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
        let trim = i32::from(world.p_random("pr_explode") & 3);
        if let Some(a) = world.actors.get_mut(actor_id) {
            a.tics = (a.tics - trim).max(1);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{ClassRegistry, StateTable, World};
    use ferrite_nodebuild::apply_nodes;
    use ferrite_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    #[test]
    fn ripper_missile_damages_without_exploding() {
        let ir = ferrite_decorate::parse_decorate(
            r#"
actor Dummy : Actor 9940
{
  Health 100
  Radius 20
  Height 56
  +SOLID
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor RipShot : Actor 9941
{
  Health 1000
  Radius 6
  Height 8
  Damage 5
  +NOGRAVITY
  States
  {
  Spawn:
    BAL1 A -1
    Stop
  Death:
    BAL1 B -1
    Stop
  }
}
actor RipFlagShot : Actor 9942
{
  Health 1000
  Radius 6
  Height 8
  Damage 5
  +NOGRAVITY
  +RIPPER
  States
  {
  Spawn:
    BAL1 A -1
    Stop
  Death:
    BAL1 B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let dummy = classes.find_name("Dummy").unwrap();
        let shot = classes.find_name("RipShot").unwrap();
        let v1 = w
            .spawn_at(
                dummy,
                DVec3::new(160.0, 64.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        let v2 = w
            .spawn_at(
                dummy,
                DVec3::new(220.0, 64.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        let missile = w
            .spawn_at(
                shot,
                DVec3::new(100.0, 64.0, 32.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(missile) {
            a.flags |= FLAG_MISSILE;
            a.flags &= !FLAG_SOLID;
            a.vel = DVec3::new(40.0, 0.0, 0.0);
            a.ripper_level = 1;
        }
        let h1 = w.actors.get(v1).unwrap().health;
        let h2 = w.actors.get(v2).unwrap().health;
        for _ in 0..8 {
            xy_movement(&mut w, missile);
        }
        assert!(w.actors.get(v1).unwrap().health < h1);
        assert!(w.actors.get(v2).unwrap().health < h2);
        assert!(
            !w.actors.get(missile).unwrap().destroyed,
            "ripper should survive hits"
        );
        let death = w.states.find_label(shot, "Death").unwrap();
        assert_ne!(
            w.actors.get(missile).unwrap().state,
            Some(death),
            "ripper should not enter Death on rip"
        );

        // +RIPPER alone (no RipperLevel) also rips through.
        let v3 = w
            .spawn_at(
                dummy,
                DVec3::new(160.0, 160.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        let v4 = w
            .spawn_at(
                dummy,
                DVec3::new(220.0, 160.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        let flag_shot = classes.find_name("RipFlagShot").unwrap();
        let missile2 = w
            .spawn_at(
                flag_shot,
                DVec3::new(100.0, 160.0, 32.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(missile2) {
            a.flags |= FLAG_MISSILE;
            a.flags &= !FLAG_SOLID;
            a.vel = DVec3::new(40.0, 0.0, 0.0);
            assert_eq!(a.ripper_level, 0);
            assert_ne!(a.flags & crate::class::FLAG_RIPPER, 0);
        }
        let h3 = w.actors.get(v3).unwrap().health;
        let h4 = w.actors.get(v4).unwrap().health;
        for _ in 0..8 {
            xy_movement(&mut w, missile2);
        }
        assert!(w.actors.get(v3).unwrap().health < h3);
        assert!(w.actors.get(v4).unwrap().health < h4);
        assert!(!w.actors.get(missile2).unwrap().destroyed);
    }

    #[test]
    fn same_species_missile_skips_damage() {
        let ir = ferrite_decorate::parse_decorate(
            r#"
actor PackMate : Actor 9920
{
  Health 100
  Species "Pack"
  +SOLID
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor PackShot : Actor 9921
{
  Health 1000
  Radius 6
  Height 8
  Speed 20
  Damage 20
  Species "Pack"
  +MISSILE
  +NOGRAVITY
  States
  {
  Spawn:
    BAL1 A -1
    Stop
  Death:
    BAL1 B -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(400.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mate = classes.find_name("PackMate").unwrap();
        let shot = classes.find_name("PackShot").unwrap();
        assert_eq!(classes.get(mate).unwrap().species, "Pack");
        let victim = w
            .spawn_at(
                mate,
                DVec3::new(200.0, 64.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        let hp0 = w.actors.get(victim).unwrap().health;
        let mid = w
            .spawn_at(
                shot,
                DVec3::new(160.0, 64.0, 32.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(mid) {
            a.vel = DVec3::new(20.0, 0.0, 0.0);
        }
        xy_movement(&mut w, mid);
        assert_eq!(w.actors.get(victim).unwrap().health, hp0);
    }

    #[test]
    fn missile_damages_solid_on_hit() {
        use crate::{ClassRegistry, StateTable};
        use ferrite_decorate::parse_decorate;
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(400.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let ball = classes.find_name("DoomImpBall").unwrap();
        let victim = w
            .spawn_at(
                zombie,
                DVec3::new(200.0, 64.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        let hp_before = w.actors.get(victim).unwrap().health;
        let missile = w
            .spawn_at(
                ball,
                DVec3::new(160.0, 64.0, 32.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(missile) {
            a.flags |= FLAG_MISSILE;
            a.flags &= !FLAG_SOLID;
            a.vel = DVec3::new(30.0, 0.0, 0.0);
        }
        for _ in 0..4 {
            xy_movement(&mut w, missile);
        }
        let hp_after = w.actors.get(victim).unwrap().health;
        assert!(
            hp_after < hp_before,
            "expected damage: {hp_before} -> {hp_after}"
        );
        let death = w.states.find_label(ball, "Death").unwrap();
        assert_eq!(w.actors.get(missile).unwrap().state, Some(death));
        // Doom `PIT_CheckThing`: missiles damage + explode; blood is hitscan-only.
    }

    #[test]
    fn missile_damage_uses_dice_roll() {
        use crate::{ClassRegistry, StateTable};
        use ferrite_decorate::parse_decorate;
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(400.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        w.rng.set_seed(1);
        let zombie = classes.find_name("ZombieMan").unwrap();
        let ball = classes.find_name("DoomImpBall").unwrap();
        let base = classes.get(ball).unwrap().damage;
        assert!(base > 0);
        let victim = w
            .spawn_at(
                zombie,
                DVec3::new(200.0, 64.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        // Keep the zombie alive through one hit so we can measure damage.
        if let Some(a) = w.actors.get_mut(victim) {
            a.health = 10_000;
        }
        let hp_before = w.actors.get(victim).unwrap().health;
        let missile = w
            .spawn_at(
                ball,
                DVec3::new(160.0, 64.0, 32.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(missile) {
            a.flags |= FLAG_MISSILE;
            a.flags &= !FLAG_SOLID;
            a.vel = DVec3::new(30.0, 0.0, 0.0);
        }
        for _ in 0..4 {
            xy_movement(&mut w, missile);
        }
        let dealt = hp_before - w.actors.get(victim).unwrap().health;
        assert!(dealt > 0);
        assert_eq!(dealt % base, 0, "damage should be N*base, got {dealt}");
        assert!((1..=8).contains(&(dealt / base)));
    }

    #[test]
    fn missile_explodes_on_wall() {
        use crate::{ClassRegistry, StateTable};
        use ferrite_decorate::parse_decorate;
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let ball = classes.find_name("DoomImpBall").unwrap();
        // Radius 6: start just outside the wall-contact band so one step enters it.
        let id = w
            .spawn_at(
                ball,
                DVec3::new(15.0, 128.0, 32.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.flags |= FLAG_MISSILE;
            a.vel = DVec3::new(-20.0, 0.0, 0.0);
        }
        xy_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        assert_eq!(a.vel.x(), 0.0);
        assert_eq!(a.vel.y(), 0.0);
        let death = states.find_label(ball, "Death").unwrap();
        assert_eq!(a.state, Some(death));
    }

    #[test]
    fn missile_explodes_on_floor_impact() {
        use crate::{ClassRegistry, StateTable};
        use ferrite_decorate::parse_decorate;
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let ball = classes.find_name("DoomImpBall").unwrap();
        let id = w
            .spawn_at(
                ball,
                DVec3::new(128.0, 128.0, 8.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.flags |= FLAG_MISSILE | crate::class::FLAG_NOGRAVITY;
            a.height = 8.0;
            a.vel = DVec3::new(0.0, 0.0, -16.0);
        }
        let spawn = w.actors.get(id).unwrap().state;
        z_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        assert_eq!(
            a.flags & FLAG_MISSILE,
            0,
            "floor impact must clear MISSILE (was skimming forever)"
        );
        assert_eq!(a.vel, DVec3::ZERO);
        assert_ne!(
            a.state, spawn,
            "missile must leave Spawn loop after floor impact"
        );
        let _ = states;
    }

    #[test]
    fn try_move_blocked_by_outer_wall() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let id = w.thinkers.default[0];
        // Overlap the west one-sided wall (x=0) with radius 20.
        assert!(!try_move(&mut w, id, DVec3::new(10.0, 128.0, 0.0)));
        // Comfortably inside — must succeed.
        assert!(try_move(&mut w, id, DVec3::new(140.0, 128.0, 0.0)));
        let a = w.actors.get(id).unwrap();
        assert!((a.pos.x() - 140.0).abs() < 1e-9);
    }

    #[test]
    fn noclip_ignores_walls() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let id = w.players[0].as_ref().unwrap().mo.unwrap();
        w.players[0].as_mut().unwrap().noclip = true;
        assert!(try_move(&mut w, id, DVec3::new(-50.0, 128.0, 0.0)));
        assert!((w.actors.get(id).unwrap().pos.x() - (-50.0)).abs() < 1e-6);
    }

    #[test]
    fn try_move_blocked_by_solid_thing() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let blocker = w
            .spawn_at(
                ferrite_core::ClassId::from_raw(0),
                DVec3::new(100.0, 64.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(blocker) {
            a.flags |= 1 << 0; // SOLID
            a.radius = 20.0;
            a.height = 56.0;
        }
        // Overlap the blocker.
        assert!(!try_move(&mut w, player, DVec3::new(100.0, 64.0, 0.0)));
        let _ = player;
    }

    #[test]
    fn try_move_steps_up_within_max_step_height() {
        let ir = ferrite_decorate::parse_decorate(
            r#"
actor Climber : Actor 0
{
  MaxStepHeight 24
  MaxDropOffHeight 24
  Radius 16
  Height 56
  +SOLID
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("high", rect(256.0, 0.0, 512.0, 256.0), 16.0, 144.0)
            .player_start(200.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let id = w.players[0].as_ref().unwrap().mo.unwrap();
        // Step onto the +16 ledge (within MaxStepHeight 24).
        assert!(try_move(&mut w, id, DVec3::new(300.0, 128.0, 0.0)));
        assert!((w.actors.get(id).unwrap().pos.z() - 16.0).abs() < 1e-6);
    }

    #[test]
    fn xy_movement_stairstep_slides_onto_ledge() {
        // Diagonal mom into a +16 step: full move may fail, but stairstep Y or X must climb.
        let ir = ferrite_decorate::parse_decorate(
            r#"
actor Climber : Actor 0
{
  MaxStepHeight 24
  MaxDropOffHeight 24
  Radius 16
  Height 56
  +SOLID
  +DROPOFF
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("high", rect(256.0, 0.0, 512.0, 256.0), 16.0, 144.0)
            .player_start(200.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let id = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.pos = DVec3::new(230.0, 100.0, 0.0);
            a.vel = DVec3::new(20.0, 20.0, 0.0);
        }
        xy_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        assert!(
            a.pos.x() > 230.0 || a.pos.y() > 100.0,
            "stairstep must advance on at least one axis, got ({}, {})",
            a.pos.x(),
            a.pos.y()
        );
    }

    #[test]
    fn xy_movement_wall_slide_glides_along_blocker() {
        // Walk into a north wall with diagonal mom — should keep X motion.
        let ir = ferrite_decorate::parse_decorate(
            r#"
actor Walker : Actor 0
{
  Radius 16
  Height 56
  +SOLID
  +DROPOFF
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("r", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 200.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let id = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.pos = DVec3::new(128.0, 230.0, 0.0);
            a.vel = DVec3::new(16.0, 16.0, 0.0);
        }
        let x0 = w.actors.get(id).unwrap().pos.x();
        xy_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        assert!(
            a.pos.x() > x0 + 1.0,
            "wall slide should keep lateral X, got x={} from {}",
            a.pos.x(),
            x0
        );
    }

    #[test]
    fn try_move_dropoff_flag_allows_tall_ledge() {
        let ir = ferrite_decorate::parse_decorate(
            r#"
actor Careful : Actor 9911
{
  MaxDropOffHeight 24
  Radius 16
  Height 56
  +SOLID
  States { Spawn: POSS A -1 Stop }
}
actor Reckless : Actor 9912
{
  MaxDropOffHeight 24
  Radius 16
  Height 56
  +SOLID
  +DROPOFF
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("high", rect(0.0, 0.0, 256.0, 256.0), 40.0, 168.0)
            .room("low", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let careful = classes.find_name("Careful").unwrap();
        let reckless = classes.find_name("Reckless").unwrap();
        assert_eq!(
            classes.get(reckless).unwrap().flags & crate::class::FLAG_DROPOFF,
            crate::class::FLAG_DROPOFF
        );
        let cautious = w
            .spawn_at(
                careful,
                DVec3::new(128.0, 128.0, 40.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        // Dest deep in the low sector so check_position floor is 0 (not max'd with 40
        // from the shared wall still in the same blockmap cell).
        // 40-unit drop > MaxDropOffHeight 24 → blocked without DROPOFF.
        assert!(!try_move(&mut w, cautious, DVec3::new(400.0, 128.0, 40.0)));
        let daredevil = w
            .spawn_at(
                reckless,
                DVec3::new(128.0, 128.0, 40.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        assert!(try_move(&mut w, daredevil, DVec3::new(400.0, 128.0, 40.0)));
        assert!((w.actors.get(daredevil).unwrap().pos.x() - 400.0).abs() < 1e-6);
    }

    #[test]
    fn try_move_rejects_step_above_max_step_height() {
        let ir = ferrite_decorate::parse_decorate(
            r#"
actor ShortLegs : Actor 9910
{
  MaxStepHeight 8
  Radius 16
  Height 56
  +SOLID
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("high", rect(256.0, 0.0, 512.0, 256.0), 16.0, 144.0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let short = classes.find_name("ShortLegs").unwrap();
        assert!((classes.get(short).unwrap().max_step_height - 8.0).abs() < 1e-9);
        let id = w
            .spawn_at(
                short,
                DVec3::new(200.0, 128.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        assert!(!try_move(&mut w, id, DVec3::new(300.0, 128.0, 0.0)));
        assert!((w.actors.get(id).unwrap().pos.x() - 200.0).abs() < 1e-6);
    }

    #[test]
    fn z_movement_uses_sector_gravity() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 256.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].gravity = 0.5;
        let mut w = World::from_map(map).unwrap();
        assert!((w.sectors[0].gravity - 0.5).abs() < 1e-9);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.pos = DVec3::new(64.0, 64.0, 40.0);
            a.vel = DVec3::ZERO;
            a.flags &= !crate::class::FLAG_NOGRAVITY;
        }
        z_movement(&mut w, mo);
        let vz = w.actors.get(mo).unwrap().vel.z();
        // Default would be -1.0 per tic; half sector gravity → -0.5.
        assert!(
            (vz + 0.5).abs() < 1e-6,
            "sector gravity 0.5 should halve accel, vel_z={vz}"
        );
    }

    #[test]
    fn z_movement_falls_to_floor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let id = w.thinkers.default[0];
        {
            let a = w.actors.get_mut(id).unwrap();
            a.pos = DVec3::new(64.0, 64.0, 40.0);
            a.vel = DVec3::ZERO;
        }
        for _ in 0..80 {
            z_movement(&mut w, id);
        }
        let a = w.actors.get(id).unwrap();
        assert!((a.pos.z()).abs() < 1e-6, "z={}", a.pos.z());
    }

    #[test]
    fn hexen_falling_damage_thresholds() {
        assert_eq!(hexen_falling_damage(-20.0, 100, 100), 0);
        let dmg = hexen_falling_damage(-30.0, 100, 100);
        assert!(dmg > 0, "momz=-30 should hurt, got {dmg}");
        assert!(dmg < 100, "soft threshold leaves HP, got {dmg}");
        assert!(hexen_falling_damage(-63.0, 100, 100) >= 1_000_000);
    }

    #[test]
    fn falling_damage_hurts_unless_nofallingdamage() {
        use crate::class::FLAG_SHOOTABLE;
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.level.falling_damage = true;
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.flags |= FLAG_SHOOTABLE;
            a.health = 100;
            a.mass = 100;
            // Must reach floor this tic with |momz| ≥ 23.
            a.pos = DVec3::new(64.0, 64.0, 8.0);
            a.vel = DVec3::new(0.0, 0.0, -40.0);
        }
        z_movement(&mut w, mo);
        let hp1 = w.actors.get(mo).unwrap().health;
        assert!(hp1 < 100, "falling damage should hurt (hp={hp1})");

        // Sector gate.
        std::sync::Arc::make_mut(&mut w.map).sectors[0].no_falling_damage = true;
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.health = 100;
            a.pos = DVec3::new(64.0, 64.0, 8.0);
            a.vel = DVec3::new(0.0, 0.0, -40.0);
        }
        z_movement(&mut w, mo);
        assert_eq!(
            w.actors.get(mo).unwrap().health,
            100,
            "nofallingdamage should skip damage"
        );
    }

    #[test]
    fn z_movement_hard_land_plays_oof() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.pos = DVec3::new(64.0, 64.0, 16.0);
            a.vel = DVec3::new(0.0, 0.0, -20.0);
        }
        let _ = w.drain_sound_events();
        z_movement(&mut w, mo);
        let sounds: Vec<_> = w.drain_sound_events().into_iter().map(|e| e.name).collect();
        assert!(
            sounds.iter().any(|n| n == "player/male/land"),
            "hard land should play oof, got {sounds:?}"
        );
        assert!((w.actors.get(mo).unwrap().pos.z()).abs() < 1e-6);
    }

    #[test]
    fn z_movement_skips_gravity_with_nogravity() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = ferrite_decorate::parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let ball = classes.find_name("DoomImpBall").unwrap();
        assert_ne!(
            classes.get(ball).unwrap().flags & crate::class::FLAG_NOGRAVITY,
            0
        );
        let id = w
            .spawn_at(
                ball,
                DVec3::new(64.0, 64.0, 40.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        z_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        assert!((a.pos.z() - 40.0).abs() < 1e-9, "z={}", a.pos.z());
        assert_eq!(a.vel.z(), 0.0);
    }

    #[test]
    fn z_movement_scales_by_class_gravity() {
        let ir = ferrite_decorate::parse_decorate(
            r#"
actor LightFall : Actor 9301
{
  Gravity 0.5
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cls = classes.find_name("LightFall").unwrap();
        assert!((classes.get(cls).unwrap().gravity - 0.5).abs() < 1e-9);
        let id = w
            .spawn_at(cls, DVec3::new(64.0, 64.0, 40.0), ferrite_core::Angle::ZERO)
            .unwrap();
        z_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        // One tic: vel_z = -0.5, z = 39.5
        assert!((a.vel.z() + 0.5).abs() < 1e-9, "vel_z={}", a.vel.z());
        assert!((a.pos.z() - 39.5).abs() < 1e-9, "z={}", a.pos.z());
    }

    #[test]
    fn xy_movement_scales_by_class_friction() {
        let ir = ferrite_decorate::parse_decorate(
            r#"
actor Icy : Actor 9302
{
  Friction 0.5
  Radius 16
  Height 56
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let cls = classes.find_name("Icy").unwrap();
        assert!((classes.get(cls).unwrap().friction - 0.5).abs() < 1e-9);
        let id = w
            .spawn_at(
                cls,
                DVec3::new(128.0, 128.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.vel = DVec3::new(16.0, 0.0, 0.0);
        }
        xy_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        // ORIG_FRICTION * 0.5 ≈ 0.453125 → vel_x ≈ 7.25
        let expected = 16.0 * ORIG_FRICTION * 0.5;
        assert!(
            (a.vel.x() - expected).abs() < 1e-9,
            "vel_x={} expected {expected}",
            a.vel.x()
        );
        assert!((a.pos.x() - 144.0).abs() < 1e-6, "x={}", a.pos.x());
    }

    #[test]
    fn xy_movement_vanilla_fixed_friction_uses_fixed_mul() {
        let ir = ferrite_decorate::parse_decorate(
            r#"
actor IcyFixed : Actor 9303
{
  Friction 0.5
  Radius 16
  Height 56
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        w.math_mode = crate::vanilla::SimMathMode::VanillaFixed;
        let cls = classes.find_name("IcyFixed").unwrap();
        let id = w
            .spawn_at(
                cls,
                DVec3::new(128.0, 128.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.vel = DVec3::new(16.0, 0.0, 0.0);
        }
        xy_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        let factor = ORIG_FRICTION * 0.5;
        let expected = (ferrite_core::math::Fixed::from_f64(16.0)
            * ferrite_core::math::Fixed::from_f64(factor))
        .to_f64();
        assert!(
            (a.vel.x() - expected).abs() < 1e-9,
            "vel_x={} expected {expected}",
            a.vel.x()
        );
    }

    #[test]
    fn noblockmap_solid_does_not_block_move() {
        use crate::{ClassRegistry, StateTable};
        use ferrite_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor GhostPillar : Actor 9310
{
  Radius 20
  Height 56
  +SOLID
  +NOBLOCKMAP
  States { Spawn: COLU A -1 Stop }
}
actor Walker : Actor 9311
{
  Radius 16
  Height 56
  +SOLID
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        assert_eq!(
            classes
                .get(classes.find_name("GhostPillar").unwrap())
                .unwrap()
                .flags
                & FLAG_NOBLOCKMAP,
            FLAG_NOBLOCKMAP
        );
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let _pillar = w
            .spawn_at(
                classes.find_name("GhostPillar").unwrap(),
                DVec3::new(200.0, 64.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        let walker = w
            .spawn_at(
                classes.find_name("Walker").unwrap(),
                DVec3::new(160.0, 64.0, 0.0),
                ferrite_core::Angle::ZERO,
            )
            .unwrap();
        assert!(try_move(&mut w, walker, DVec3::new(200.0, 64.0, 0.0),));
        assert!((w.actors.get(walker).unwrap().pos.x() - 200.0).abs() < 1e-6);
    }

    #[test]
    fn sloped_floor_z_varies_with_xy() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .floor_plane("a", [0.0, 0.25, 1.0, 0.0])
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let actor = w.actors.get(mo).unwrap().clone();
        let z0 = check_position(&w, &actor, DVec3::new(32.0, 0.0, 0.0), Some(mo)).floor_z;
        let z1 = check_position(&w, &actor, DVec3::new(32.0, 128.0, 0.0), Some(mo)).floor_z;
        assert!(
            (z1 - z0).abs() > 8.0,
            "slope should change floor z: z0={z0} z1={z1}"
        );
    }

    #[test]
    fn solid_ffloor_raises_floor_and_clips_under() {
        let mut map = MapBuilder::new()
            .room("play", rect(0.0, 0.0, 256.0, 256.0), 0.0, 192.0)
            .sector_tag("play", 3)
            .room("ctrl", rect(256.0, 0.0, 384.0, 256.0), 64.0, 80.0)
            .ffloor_between("ctrl", "play", 3, 1)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        assert!(
            map.setup.ffloors.iter().any(|f| f.is_solid()),
            "expected solid 3D floor"
        );
        let w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let actor = w.actors.get(mo).unwrap().clone();
        let under = check_position(&w, &actor, DVec3::new(32.0, 32.0, 0.0), Some(mo));
        assert!(under.ok);
        assert!(
            (under.ceiling_z - 64.0).abs() < 1.0,
            "walk-under ceiling should be slab bottom, got {}",
            under.ceiling_z
        );
        let on_top = check_position(&w, &actor, DVec3::new(32.0, 32.0, 80.0), Some(mo));
        assert!(on_top.ok);
        assert!(
            (on_top.floor_z - 80.0).abs() < 1.0,
            "stand-on floor should be slab top, got {}",
            on_top.floor_z
        );
        let inside = check_position(&w, &actor, DVec3::new(32.0, 32.0, 70.0), Some(mo));
        assert!(
            (inside.floor_z - 80.0).abs() < 1.0,
            "step-up from inside slab, floor={}",
            inside.floor_z
        );
    }

    #[test]
    fn midtex3d_raises_floor_on_shared_wall() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut stamped = 0u32;
        for line in &mut map.lines {
            if line.front_sector.is_some() && line.back_sector.is_some() {
                line.midtex3d = true;
                stamped += 1;
            }
        }
        assert!(stamped > 0, "expected a two-sided shared wall");
        let w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let actor = w.actors.get(mo).unwrap().clone();
        // Standing on the shared wall (x≈128) should see midtex top as floor.
        let on = check_position(&w, &actor, DVec3::new(128.0, 64.0, 64.0), Some(mo));
        assert!(on.ok);
        assert!(
            (on.floor_z - 64.0).abs() < 1.0,
            "midtex3d top should be floor, got {}",
            on.floor_z
        );
    }

    #[test]
    fn midtex_attach_ceiling_hangs_slab() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut cand = None;
        for (i, line) in map.lines.iter_mut().enumerate() {
            if line.front_sector.is_some() && line.back_sector.is_some() {
                line.midtex3d = true;
                cand = Some(i);
                break;
            }
        }
        let cand = cand.expect("shared wall");
        let sector = map.lines[cand].front_sector.expect("front");
        map.setup.midtex_attach.push(ferrite_map::MidtexAttach {
            sector,
            lines: vec![cand as u32],
            ceiling: true,
        });
        let w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let actor = w.actors.get(mo).unwrap().clone();
        // Hang from ceiling 128 → slab [64,128]; actor below sees mid_bot as ceiling.
        let under = check_position(&w, &actor, DVec3::new(128.0, 64.0, 0.0), Some(mo));
        assert!(
            (under.ceiling_z - 64.0).abs() < 1.0,
            "ceiling-attached midtex should clip ceiling to 64, got {}",
            under.ceiling_z
        );
        // Floor-sit default would raise floor to 64 here — hang must not.
        let at_mid = check_position(&w, &actor, DVec3::new(128.0, 64.0, 64.0), Some(mo));
        assert!(
            at_mid.floor_z < 63.0 || !at_mid.ok,
            "ceiling hang must not floor-sit at 64, floor={}",
            at_mid.floor_z
        );
    }

    #[test]
    fn blockprojectiles_blocks_missile_not_player() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        for line in &mut map.lines {
            if line.front_sector.is_some() && line.back_sector.is_some() {
                line.block_projectiles = true;
                line.flags &= !(crate::world::ML_BLOCKING as i16);
            }
        }
        let w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let mut actor = w.actors.get(mo).unwrap().clone();
        let player = check_position(&w, &actor, DVec3::new(128.0, 64.0, 0.0), Some(mo));
        assert!(player.ok, "player should cross blockprojectiles line");
        actor.flags |= FLAG_MISSILE;
        let blocked = check_position(&w, &actor, DVec3::new(128.0, 64.0, 0.0), Some(mo));
        assert!(!blocked.ok, "missile should be blocked by blockprojectiles");
    }

    #[test]
    fn blockfloaters_blocks_float_not_ground() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        for line in &mut map.lines {
            if line.front_sector.is_some() && line.back_sector.is_some() {
                line.block_floaters = true;
                line.flags &= !(crate::world::ML_BLOCKING as i16);
            }
        }
        let w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let mut actor = w.actors.get(mo).unwrap().clone();
        let ground = check_position(&w, &actor, DVec3::new(128.0, 64.0, 0.0), Some(mo));
        assert!(ground.ok, "non-float should cross blockfloaters line");
        actor.flags |= crate::class::FLAG_FLOAT;
        let blocked = check_position(&w, &actor, DVec3::new(128.0, 64.0, 0.0), Some(mo));
        assert!(!blocked.ok, "float should be blocked by blockfloaters");
    }

    #[test]
    fn midtex3d_impassible_blocks_player_not_missile() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        for line in &mut map.lines {
            if line.front_sector.is_some() && line.back_sector.is_some() {
                line.midtex3d = true;
                line.midtex3d_impassible = true;
            }
        }
        let w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let mut actor = w.actors.get(mo).unwrap().clone();
        actor.height = 56.0;
        let blocked = check_position(&w, &actor, DVec3::new(128.0, 64.0, 16.0), Some(mo));
        assert!(!blocked.ok, "player should be blocked inside midtex");
        actor.flags |= FLAG_MISSILE;
        let pass = check_position(&w, &actor, DVec3::new(128.0, 64.0, 16.0), Some(mo));
        assert!(pass.ok, "missile should pass midtex3dimpassible");
    }

    #[test]
    fn sector_floor_portal_teleports_on_fall() {
        let mut map = MapBuilder::new()
            .room("from", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("to", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
            .sector_tag("to", 5)
            .sector_portal_between("from", "to", 5, 3, 0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        assert!(
            map.setup
                .portals
                .sectors
                .iter()
                .any(|p| p.is_traversable() && p.plane == 0),
            "expected floor Sector_SetPortal"
        );
        let mut w = World::from_map(map).unwrap();
        let id = w.thinkers.default[0];
        {
            let a = w.actors.get_mut(id).unwrap();
            a.pos = DVec3::new(64.0, 64.0, 16.0);
            a.vel = DVec3::new(0.0, 0.0, -24.0);
        }
        z_movement(&mut w, id);
        let pos = w.actors.get(id).unwrap().pos;
        assert!(
            pos.x() > 256.0,
            "floor portal should shift XY into dest sector, x={}",
            pos.x()
        );
        assert!(
            pos.z() > 32.0,
            "should appear near dest ceiling, z={}",
            pos.z()
        );
    }

    #[test]
    fn swimmable_water_reduces_gravity_and_applies_upmove() {
        use ferrite_core::SectorId;
        use ferrite_map::setup::FFloorStatic;

        let mut map = MapBuilder::new()
            .room("pool", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("ctrl", rect(300.0, 0.0, 364.0, 64.0), 0.0, 64.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.height = 56.0;
            // Hover in the water column (not on the floor).
            a.pos = DVec3::new(128.0, 128.0, 16.0);
            a.vel = DVec3::ZERO;
            a.float_speed = 4.0;
        }
        let si = ferrite_nodebuild::point_in_sector(&w.map, 128.0, 128.0)
            .or_else(|| ferrite_nodebuild::point_in_sector_vanilla(&w.map, 128.0, 128.0))
            .unwrap()
            .raw() as usize;
        let ctrl = if si == 0 { 1 } else { 0 };
        std::sync::Arc::make_mut(&mut w.map)
            .setup
            .ffloors
            .push(FFloorStatic {
                target: SectorId::from_raw(si as u32),
                control: SectorId::from_raw(ctrl as u32),
                typ: 2,
                flags: 0,
                alpha: 255,
            });
        // Surface 64, feet 16, mid 44, head 72 → waterlevel 2 (waist).
        assert_eq!(actor_waterlevel(&w, w.actors.get(mo).unwrap()), 2);

        // Dry gravity = -1.0; scale 0.5 → -0.5 then *0.9 drag → -0.45.
        z_movement(&mut w, mo);
        let wet_vz = w.actors.get(mo).unwrap().vel.z();
        assert!(
            (wet_vz - (-0.45)).abs() < 1e-6,
            "expected reduced sink, vel_z={wet_vz}"
        );

        w.actors.get_mut(mo).unwrap().vel = DVec3::ZERO;
        w.actors.get_mut(mo).unwrap().pos = DVec3::new(128.0, 128.0, 16.0);
        w.players[0].as_mut().unwrap().cmd.up_move = 1;
        z_movement(&mut w, mo);
        let swim_vz = w.actors.get(mo).unwrap().vel.z();
        assert!(swim_vz > 0.0, "up_move should swim upward, vel_z={swim_vz}");
    }
}
