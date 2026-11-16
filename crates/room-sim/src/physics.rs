//! Movement / collision (`P_CheckPosition` / `P_TryMove` subset).

use crate::actor::{Actor, ActorId};
use crate::class::{FLAG_MISSILE, FLAG_NOBLOCKMAP, FLAG_NOINTERACTION};
use crate::world::World;
use room_core::{DVec2, DVec3, LineId};
use room_map::{Blockmap, Line, MapGeometry};
use std::collections::BTreeSet;

/// Vanilla `ORIG_FRICTION` (~0xE800 / 65536).
const ORIG_FRICTION: f64 = 0.90625;
/// Vanilla `STOPSPEED` (~0x1000 / 65536) — zero XY below this after friction.
const STOPSPEED: f64 = 0.0625;
/// Default `MaxStepHeight` used when the actor has none set.
const DEFAULT_STEP: f64 = 24.0;

fn step_limit(actor: &Actor) -> f64 {
    if actor.max_step_height > 0.0 {
        actor.max_step_height
    } else {
        DEFAULT_STEP
    }
}

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
    use room_core::Fixed;
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

/// Doom `P_PointOnLineSide` == 1 (back). Front is the right side of `a → b`.
fn on_back_side(p: DVec2, a: DVec2, b: DVec2) -> bool {
    let dx = b.x() - a.x();
    let dy = b.y() - a.y();
    let adx = p.x() - a.x();
    let ady = p.y() - a.y();
    if dx.abs() < 1e-9 {
        return if p.x() <= a.x() { dy > 0.0 } else { dy < 0.0 };
    }
    if dy.abs() < 1e-9 {
        return if p.y() <= a.y() { dx < 0.0 } else { dx > 0.0 };
    }
    dy * adx <= ady * dx
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
    if line.back_sector.is_none() {
        return true;
    }
    // A railing's `ML_BLOCKING` is the fence itself (handled below).
    if (flags & crate::world::ML_BLOCKING) != 0 && (flags & crate::world::ML_RAILING) == 0 {
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
    let ext = world.map.udmf_user.line_ext(lid.raw() as usize);
    // UDMF `blocklandmonsters` (ML2_BLOCKLANDMONSTERS): non-flying monsters.
    if ext & room_map::udmf::ext::LINE_BLOCKLANDMONSTERS != 0
        && !is_player
        && (actor.flags & crate::class::FLAG_COUNTKILL) != 0
        && (actor.flags & crate::class::FLAG_FLOAT) == 0
    {
        return true;
    }
    // Strife railing (`ML_RAILING` / UDMF `jumpover`): a 32-unit-high fence
    // over the higher of the two floors; jumping or flying clears it.
    if (flags & crate::world::ML_RAILING) != 0 || ext & room_map::udmf::ext::LINE_JUMPOVER != 0 {
        let floor = |s: Option<room_core::SectorId>| {
            s.and_then(|s| world.map.sectors.get(s.raw() as usize))
                .map(|s| f64::from(s.floor_height))
                .unwrap_or(f64::MIN)
        };
        let top = floor(line.front_sector).max(floor(line.back_sector)) + 32.0;
        if actor.pos.z() < top {
            return true;
        }
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
    if let Some(sid) = room_nodebuild::point_in_sector(&world.map, pos.x(), pos.y())
        .or_else(|| room_nodebuild::point_in_sector_vanilla(&world.map, pos.x(), pos.y()))
    {
        let si = sid.raw() as usize;
        if world.sectors.get(si).is_some() {
            let floor_z = sector_z_at(world, si, pos.x(), pos.y(), true);
            let ceiling_z = sector_z_at(world, si, pos.x(), pos.y(), false);
            // An overlapping Build sector (a balcony over a doorway) can contain
            // the same point. Ignore a floor the actor cannot step up to.
            // Doom/GZDoom always seed from the centre sector (an out-of-reach
            // floor then blocks the move in `try_move`).
            if !world.build_overlaps || floor_z - pos.z() <= step_limit(actor) {
                contacted.insert(si);
                result.floor_z = floor_z;
                result.ceiling_z = ceiling_z;
                result.dropoff_z = result.floor_z;
                saw_sector = true;
            }
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
        // Opening: only when the actor AABB actually reaches the seg.
        let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        let touches = dist_point_to_seg(xy, v1.pos, v2.pos) < actor.radius;
        if touches {
            if let Some(sid) = front {
                contacted.insert(sid.raw() as usize);
            }
            if let Some(sid) = back {
                contacted.insert(sid.raw() as usize);
            }
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
                    let feet = pos.z();
                    let head = feet + actor.height;
                    // A step the actor cannot climb, on a line of some other
                    // sector, is a balcony overlapping the floor they are on.
                    let standing =
                        room_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
                            .or_else(|| {
                                room_nodebuild::point_in_sector_vanilla(
                                    &world.map,
                                    actor.pos.x(),
                                    actor.pos.y(),
                                )
                            })
                            .map(|sid| sid.raw() as usize);
                    let on_this = standing.is_some_and(|s| s == fi || s == bi);
                    if !on_this && open_bottom - feet > step_limit(actor) {
                        continue;
                    }
                    // A floor more than a body-height above the actor's head is
                    // a balcony overlapping this room, not a door panel.
                    if open_bottom - head > actor.height {
                        continue;
                    }
                    // A floor above the actor's head is a wall (Build clipmove).
                    // Merging it with another line's ceiling crushes a doorway
                    // the body only overlaps.
                    if head <= open_bottom + 1e-6 && open_bottom - feet > step_limit(actor) {
                        result.ok = false;
                        result.blocking_line = Some(lid);
                        return result;
                    }
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
                    // A balcony sector overlapping the floor the actor is on.
                    if floor_z - pos.z() <= step_limit(actor) && !saw_sector {
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
            // A one-sided wall only stops movement on its sector side. Build
            // maps put the empty side over another sector (a ledge beside a
            // doorway); the disc may overlap that wall until the centre crosses.
            // A balcony whose floor is above the actor is the same kind of
            // overlap: its wall is not a barrier on the floor they are walking.
            // Only skip when the *actor* is already on the back — a front-side
            // origin whose probe centre crossed into void must still block
            // (Doom one-siders after MapBuilder uses right-hand fronts).
            if line.back_sector.is_none() && on_back_side(xy, v1.pos, v2.pos) {
                let origin = DVec2::new(actor.pos.x(), actor.pos.y());
                if on_back_side(origin, v1.pos, v2.pos) {
                    continue;
                }
            }
            if line.back_sector.is_none()
                && line.front_sector.is_some_and(|sid| {
                    sector_z_at(world, sid.raw() as usize, pos.x(), pos.y(), true) - pos.z()
                        > step_limit(actor)
                })
            {
                continue;
            }
            result.ok = false;
            result.blocking_line = Some(lid);
            return result;
        }
    }
    // `+ALLOWTHRUBITS`: actors sharing a `ThruBits` bit pass through each other.
    let self_thru = actor
        .extra_flags
        .get_name("ALLOWTHRUBITS")
        .then(|| {
            world
                .classes
                .get(actor.class)
                .map_or(0, |c| c.ext.thru_bits)
        })
        .filter(|b| *b != 0);
    // Thing-thing: block on solid actors whose XY disks overlap.
    for id in world.collision_candidates() {
        if self_id == Some(*id) {
            continue;
        }
        let Some(other) = world.actors.get(*id) else {
            continue;
        };
        if let Some(bits) = self_thru {
            if other.extra_flags.get_name("ALLOWTHRUBITS")
                && world
                    .classes
                    .get(other.class)
                    .is_some_and(|c| c.ext.thru_bits & bits != 0)
            {
                continue;
            }
        }
        if other.destroyed
            || (other.flags & FLAG_SOLID) == 0
            || (other.flags & FLAG_NOBLOCKMAP) != 0
            || (other.flags & FLAG_NOINTERACTION) != 0
            || (actor.flags & FLAG_NOINTERACTION) != 0
        {
            continue;
        }
        // GZDoom `PIT_CheckThing`: a missile never hits its own shooter
        // (`target`) unless `+HITOWNER`. The projectile spawns inside the
        // shooter, so without this a fine-stepped fireball hit its imp.
        if (actor.flags & FLAG_MISSILE) != 0
            && actor.target == Some(*id)
            && !actor.extra_flags.get_name("HITOWNER")
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
        } else if mid_top - feet <= step_limit(actor) {
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
        } else if top - feet <= step_limit(actor) {
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

/// Sector index containing `(x, y)` (nodes, then the vanilla fallback).
#[must_use]
pub fn sector_at_xy(world: &World, x: f64, y: f64) -> Option<usize> {
    room_nodebuild::point_in_sector(&world.map, x, y)
        .or_else(|| room_nodebuild::point_in_sector_vanilla(&world.map, x, y))
        .map(|id| id.raw() as usize)
}

/// Floor Z under `(x, y)` for the sector containing the point (slopes + movers).
#[must_use]
pub fn floor_at_xy(world: &World, x: f64, y: f64) -> Option<f64> {
    let si = sector_at_xy(world, x, y)?;
    Some(sector_z_at(world, si, x, y, true))
}

/// Ceiling Z above `(x, y)` for the sector containing the point.
#[must_use]
pub fn ceiling_at_xy(world: &World, x: f64, y: f64) -> Option<f64> {
    let si = sector_at_xy(world, x, y)?;
    Some(sector_z_at(world, si, x, y, false))
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

/// Another line of the same polyobject that actually has a cross special.
/// `Polyobj_StartLine` itself does not open the door.
fn poly_cross_line(world: &World, lid: LineId) -> Option<LineId> {
    let po = world.polyobjs.iter().find(|p| p.lines.contains(&lid))?;
    po.lines.iter().copied().find(|&other| {
        other != lid
            && world
                .map
                .lines
                .get(other.raw() as usize)
                .is_some_and(|ln| ln.special == crate::specials::num::ACS_EXECUTE as i16)
            && world.line_activation(other) & room_map::spac::CROSS != 0
    })
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
            // A door the player is walking into never gets a cross event,
            // because the move stops on the panel. Run it anyway.
            if let Some(lid) = check.blocking_line {
                let player = world
                    .players
                    .iter()
                    .flatten()
                    .any(|p| p.mo == Some(actor_id));
                if player {
                    // The poly's first line is only `Polyobj_StartLine`, so it
                    // has no cross action. The other edges run the swing
                    // script. Walking into the tagged edge should open it too.
                    let cross = world.line_activation(lid) & room_map::spac::CROSS != 0;
                    let door = if cross {
                        Some(lid)
                    } else {
                        poly_cross_line(world, lid)
                    };
                    if let Some(lid) = door {
                        let _ = crate::specials::activate_line(
                            world,
                            lid,
                            Some(actor_id),
                            crate::specials::ActivationType::Cross,
                        );
                    }
                }
            }
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
            water_climb(world, actor_id, &actor, delta, max_step);
            return TryMoveResult {
                ok: false,
                blocking_actor: None,
            };
        }
        if too_steep(world, &actor, new_pos) {
            return TryMoveResult {
                ok: false,
                blocking_actor: None,
            };
        }
        let floating = (actor.flags & crate::class::FLAG_FLOAT) != 0
            || (actor.flags & crate::class::FLAG_NOGRAVITY) != 0;
        let can_dropoff = (actor.flags & crate::class::FLAG_DROPOFF) != 0;
        // Doom / GZDoom `P_TryMove`: "don't stand over a dropoff" compares the
        // highest and lowest floors under the new box (`tmfloorz - tmdropoffz`),
        // not the floor with the actor's own z. An actor in the air (a gib
        // thrown at chest height, a missile checked by `P_CheckMissileSpawn`)
        // over flat ground is not at a ledge.
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

/// `Player.WaterClimbSpeed` (default 3.5): a player wading (`waterlevel` 1 or
/// 2) into a ledge up to 48 units above the step height is pushed up the wall.
fn water_climb(world: &mut World, actor_id: ActorId, actor: &Actor, delta: f64, max_step: f64) {
    if delta > max_step + 48.0
        || !world
            .players
            .iter()
            .flatten()
            .any(|p| p.mo == Some(actor_id))
    {
        return;
    }
    let level = actor_waterlevel(world, actor);
    if !(1..=2).contains(&level) {
        return;
    }
    let speed = world
        .classes
        .get(actor.class)
        .and_then(|c| c.ext.stored_f64("player.waterclimbspeed"))
        .unwrap_or(3.5);
    if let Some(a) = world.actors.get_mut(actor_id) {
        if a.vel.z() < speed {
            a.vel = DVec3::new(a.vel.x(), a.vel.y(), speed);
        }
    }
}

/// `MaxSlopeSteepness`: an actor that sets it cannot walk onto a floor slope
/// whose normal's Z is below that value (UZDoom default 46342/65536 ≈ 45°,
/// applied only to classes that declare it, so vanilla movement is unchanged).
fn too_steep(world: &World, actor: &Actor, at: DVec3) -> bool {
    let Some(limit) = world
        .classes
        .get(actor.class)
        .and_then(|c| c.ext.stored_f64("maxslopesteepness"))
    else {
        return false;
    };
    if actor.flags & (crate::class::FLAG_NOGRAVITY | crate::class::FLAG_FLOAT) != 0 {
        return false;
    }
    let Some(sid) = room_nodebuild::point_in_sector(&world.map, at.x(), at.y()) else {
        return false;
    };
    let Some([a, b, c, _]) = world
        .map
        .sectors
        .get(sid.raw() as usize)
        .and_then(|s| s.floor_plane)
    else {
        return false;
    };
    let len = (a * a + b * b + c * c).sqrt();
    if len < 1e-9 {
        return false;
    }
    (c / len).abs() < limit
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
fn slide_move_double(
    world: &mut World,
    actor_id: ActorId,
    mut dx: f64,
    mut dy: f64,
) -> (bool, bool) {
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

/// Below this speed (units per tic) a bounce settles instead of rebounding.
const BOUNCE_REST_SPEED: f64 = 1.0;

/// Remaining bounces (GZDoom `AActor::bouncecount`), kept per actor and
/// seeded from the class `BounceCount` on the first bounce.
const BOUNCES_LEFT: &str = "__room_bouncecount";

/// GZDoom `if (bouncecount > 0 && --bouncecount == 0)`: true when this bounce
/// uses up the last one (the bouncer then explodes instead of rebounding).
fn spend_bounce(world: &mut World, actor_id: ActorId) -> bool {
    let Some(a) = world.actors.get(actor_id) else {
        return false;
    };
    let left = a.user_vars.get(BOUNCES_LEFT).copied().unwrap_or_else(|| {
        world
            .classes
            .get(a.class)
            .map_or(-1, |c| c.bounce_count_max)
    });
    if left <= 0 {
        return false;
    }
    let left = left - 1;
    if let Some(a) = world.actors.get_mut(actor_id) {
        a.user_vars.insert(BOUNCES_LEFT.to_owned(), left);
    }
    left == 0
}

/// The last bounce is spent: missiles explode, other bouncers stop.
fn end_bounce(world: &mut World, actor_id: ActorId) {
    let missile = world
        .actors
        .get(actor_id)
        .is_some_and(|a| a.flags & FLAG_MISSILE != 0);
    if missile {
        explode_missile(world, actor_id);
    } else if let Some(a) = world.actors.get_mut(actor_id) {
        a.vel = DVec3::ZERO;
    }
}

/// `+MBFBOUNCER` uses the same floor and wall reflect as the bounce flags (UZ1).
fn mbf_bouncer(actor: &Actor) -> bool {
    actor.extra_flags.get_name("MBFBOUNCER")
}

fn bounce_factor(world: &World, actor: &Actor) -> f64 {
    world
        .classes
        .get(actor.class)
        .map_or(0.7, |c| c.bounce_factor)
}

fn play_bounce_sound(world: &mut World, actor: &Actor, wall: bool) {
    let Some(class) = world.classes.get(actor.class) else {
        return;
    };
    let sound = if wall && !class.wall_bounce_sound.is_empty() {
        class.wall_bounce_sound.clone()
    } else {
        class.bounce_sound.clone()
    };
    if !sound.is_empty() {
        world.play_sound(&sound, actor.pos.x(), actor.pos.y(), actor.pos.z(), 1.0);
    }
}

/// `+BOUNCEONWALLS`: reflect the horizontal velocity off the line blocking a
/// move to `dest`, scaled by `BounceFactor`. False when no line blocks it.
fn bounce_off_wall(world: &mut World, actor_id: ActorId, dest: DVec3) -> bool {
    let Some(actor) = world.actors.get(actor_id).cloned() else {
        return false;
    };
    let check = check_position(world, &actor, dest, Some(actor_id));
    let lid = check.blocking_line.or_else(|| {
        let mid = DVec2::new(
            (actor.pos.x() + dest.x()) * 0.5,
            (actor.pos.y() + dest.y()) * 0.5,
        );
        lines_near(&world.map, mid, actor.radius + 8.0)
            .into_iter()
            .find(|&lid| {
                world
                    .map
                    .lines
                    .get(lid.raw() as usize)
                    .is_some_and(|l| l.back_sector.is_none())
            })
    });
    if lid.is_none_or(|lid| world.map.lines.get(lid.raw() as usize).is_none()) {
        return false;
    }
    // GZDoom `P_BounceWall`: the bounce count runs out → the caller explodes it.
    if spend_bounce(world, actor_id) {
        return false;
    }
    let Some(line) = lid.and_then(|lid| world.map.lines.get(lid.raw() as usize)) else {
        return false;
    };
    let (Some(v1), Some(v2)) = (
        world.map.vertices.get(line.v1.raw() as usize),
        world.map.vertices.get(line.v2.raw() as usize),
    ) else {
        return false;
    };
    let (lx, ly) = (v2.pos.x() - v1.pos.x(), v2.pos.y() - v1.pos.y());
    let len2 = lx * lx + ly * ly;
    if len2 < 1e-12 {
        return false;
    }
    let (vx, vy) = (actor.vel.x(), actor.vel.y());
    let along = (vx * lx + vy * ly) / len2;
    let (px, py) = (along * lx, along * ly);
    let f = bounce_factor(world, &actor);
    let (nx, ny) = ((2.0 * px - vx) * f, (2.0 * py - vy) * f);
    if let Some(a) = world.actors.get_mut(actor_id) {
        a.vel = DVec3::new(nx, ny, a.vel.z());
        a.angle = room_core::math::Angle::from_degrees(
            room_core::dmath::atan2(ny, nx) * (180.0 / core::f64::consts::PI),
        );
    }
    play_bounce_sound(world, &actor, true);
    true
}

/// Light an actor is drawn with: the sector light (3D-floor aware), or the
/// class `LightLevel` when set (`+ADDLIGHTLEVEL` adds it to the sector light).
#[must_use]
pub fn actor_light_level(world: &World, a: &Actor) -> i32 {
    let own = world
        .classes
        .get(a.class)
        .and_then(|c| c.ext.stored_i32("lightlevel"))
        .filter(|&l| l >= 0);
    match own {
        Some(l) if a.extra_flags.get_name("ADDLIGHTLEVEL") => {
            (sector_light_for_actor(world, a) + l).clamp(0, 255)
        }
        Some(l) => l.clamp(0, 255),
        None => sector_light_for_actor(world, a),
    }
}

/// Sector light for an actor, honouring 3D-floor control lights below/inside slabs.
#[must_use]
pub fn sector_light_for_actor(world: &World, a: &Actor) -> i32 {
    let Some(sid) = room_nodebuild::point_in_sector(&world.map, a.pos.x(), a.pos.y())
        .or_else(|| room_nodebuild::point_in_sector_vanilla(&world.map, a.pos.x(), a.pos.y()))
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
    let Some(sid) = room_nodebuild::point_in_sector(&world.map, a.pos.x(), a.pos.y())
        .or_else(|| room_nodebuild::point_in_sector_vanilla(&world.map, a.pos.x(), a.pos.y()))
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
    if impact_vel_z >= -23.0 {
        return;
    }
    let Some(actor) = world.actors.get(actor_id) else {
        return;
    };
    // Level switch, or the actor's own `+FALLDAMAGE` (UZ1).
    let flagged = actor.extra_flags.get_name("FALLDAMAGE");
    if !world.level.falling_damage && !flagged {
        return;
    }
    // Players only, unless the actor flag opts a monster in.
    let is_player = world
        .players
        .iter()
        .flatten()
        .any(|p| p.mo == Some(actor_id));
    if !is_player && !flagged {
        return;
    }
    let sid =
        room_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y()).or_else(|| {
            room_nodebuild::point_in_sector_vanilla(&world.map, actor.pos.x(), actor.pos.y())
        });
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
    let no_gravity =
        (actor.flags & crate::class::FLAG_NOGRAVITY) != 0 || actor.extra_flags.get_name("SKULLFLY");
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
        let sector_g = room_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
            .or_else(|| {
                room_nodebuild::point_in_sector_vanilla(&world.map, actor.pos.x(), actor.pos.y())
            })
            .and_then(|sid| world.sectors.get(sid.raw() as usize))
            .map(|s| s.gravity)
            .unwrap_or(1.0);
        vel_z -= GRAVITY * world.level.gravity * sector_g * actor_g * water_gravity_scale(water);
    }
    let player_slot = world
        .players
        .iter()
        .flatten()
        .find(|p| p.mo == Some(actor_id));
    // Swim thrust from player `up_move` (or jump, GZDoom `CheckJump`) when
    // waist-deep or deeper.
    if water >= 2 {
        if let Some(slot) = player_slot {
            let up = if slot.cmd.up_move == 0 && slot.cmd.buttons.any(crate::player::Buttons::JUMP)
            {
                1
            } else {
                slot.cmd.up_move
            };
            if up != 0 {
                let speed = if actor.float_speed > 0.0 {
                    actor.float_speed
                } else {
                    4.0
                };
                let dir = if up > 0 { 1.0 } else { -1.0 };
                vel_z += dir * speed;
            }
        }
        // Mild vertical drag in water.
        vel_z *= 0.9;
    } else if no_gravity && actor.pos.z() > floor_z && player_slot.is_some_and(|p| p.fly_tics > 0) {
        vel_z *= crate::player::FRICTION_FLY;
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
    if (actor.flags & crate::class::FLAG_BOUNCEONFLOORS) != 0 || mbf_bouncer(&actor) {
        let hit_floor = z <= floor_z && vel_z < 0.0;
        let hit_ceiling = z + actor.height > ceiling_z && vel_z > 0.0;
        if hit_floor || hit_ceiling {
            let z = if hit_floor {
                floor_z
            } else {
                ceiling_z - actor.height
            };
            // GZDoom `AActor::FloorBounceMissile`.
            if spend_bounce(world, actor_id) {
                if let Some(a) = world.actors.get_mut(actor_id) {
                    a.pos = DVec3::new(a.pos.x(), a.pos.y(), z);
                }
                end_bounce(world, actor_id);
                return;
            }
            let f = bounce_factor(world, &actor);
            let rebound = -vel_z * f;
            let loud = rebound.abs() >= BOUNCE_REST_SPEED;
            let vel_z = if loud { rebound } else { 0.0 };
            let doom_bounce = actor.extra_flags.get_name("BOUNCEAUTOOFF")
                || actor.extra_flags.get_name("BOUNCEAUTOOFFFLOORONLY");
            // GZDoom reflects the whole velocity and keeps `BounceFactor` of
            // it. Room applies that to the Doom bounce type (`+DOOMBOUNCE`);
            // other bouncers (Duke glass, MBF) keep their horizontal speed.
            let xy = if doom_bounce && !mbf_bouncer(&actor) {
                f
            } else {
                1.0
            };
            // `BOUNCE_AutoOff` (`+DOOMBOUNCE`): a slow floor rebound ends the
            // bouncing, so the next floor contact explodes the missile.
            let auto_off = (actor.extra_flags.get_name("BOUNCEAUTOOFF")
                || (hit_floor && actor.extra_flags.get_name("BOUNCEAUTOOFFFLOORONLY")))
                && (actor.flags & crate::class::FLAG_NOGRAVITY) == 0
                && vel_z < 3.0;
            if let Some(a) = world.actors.get_mut(actor_id) {
                a.pos = DVec3::new(a.pos.x(), a.pos.y(), z);
                a.vel = DVec3::new(a.vel.x() * xy, a.vel.y() * xy, vel_z);
                if auto_off {
                    a.flags &=
                        !(crate::class::FLAG_BOUNCEONFLOORS | crate::class::FLAG_BOUNCEONWALLS);
                    a.extra_flags.set_name("BOUNCEONFLOORS", false);
                    a.extra_flags.set_name("BOUNCEONWALLS", false);
                    a.extra_flags.set_name("BOUNCEONCEILINGS", false);
                    a.extra_flags.set_name("MBFBOUNCER", false);
                }
            }
            if loud {
                play_bounce_sound(world, &actor, false);
            }
            return;
        }
    }
    if z <= floor_z {
        // Doom `P_ZMovement`: hard land (`*oof*`) when momz < -8.
        // `LandingSpeed` (default −8) sets the threshold.
        let landing = world
            .classes
            .get(actor.class)
            .map_or(-8.0, |c| c.ext.landing_speed);
        if actor.pos.z() > floor_z + 0.1 && vel_z < landing {
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
        // Vanilla `P_ZMovement`: a charging skull bounces off the floor.
        vel_z = if is_skull_flying(&actor) { -vel_z } else { 0.0 };
    }
    let head = z + actor.height;
    if head > ceiling_z {
        // A squeezing ceiling never pushes a thing through its floor (GZDoom
        // `P_ThingHeightClip` keeps it on the floor and reports "no fit").
        z = if is_missile {
            ceiling_z - actor.height
        } else {
            (ceiling_z - actor.height).max(floor_z.min(z))
        };
        vel_z = if is_skull_flying(&actor) && vel_z > 0.0 {
            -vel_z
        } else {
            0.0
        };
        hit_ceiling = true;
    }
    if let Some(a) = world.actors.get_mut(actor_id) {
        a.pos = DVec3::new(a.pos.x(), a.pos.y(), z);
        a.vel = DVec3::new(a.vel.x(), a.vel.y(), vel_z);
    }
    if is_missile && (hit_floor || hit_ceiling) {
        let pos = world
            .actors
            .get(actor_id)
            .map(|a| a.pos)
            .unwrap_or(actor.pos);
        let (kind, normal) = if hit_ceiling {
            (
                room_script_abi::hit_kind::CEILING,
                DVec3::new(0.0, 0.0, -1.0),
            )
        } else {
            (room_script_abi::hit_kind::FLOOR, DVec3::new(0.0, 0.0, 1.0))
        };
        set_missile_hit(world, actor_id, kind, None, pos, normal);
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

/// Falling faster than this (map units per tic) dives through a water link
/// without swimming down (Duke: `vel.z > 8`).
const WATER_DIVE_SPEED: f64 = 8.0;

/// A Build fall link moves a player whose eye is this close to the source
/// effector height, and leaves it this far past the destination one.
const FALL_LINK_BAND: f64 = 24.0;

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
    let Some(sid) = room_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
    else {
        return false;
    };
    let portals: Vec<_> = world.map.setup.portals.sectors.clone();
    for p in portals {
        if p.sector != sid || !p.is_traversable() || p.target == p.sector {
            continue;
        }
        if p.kind == room_map::PORTAL_KIND_FALL {
            let Some(view) = world
                .players
                .iter()
                .flatten()
                .find(|s| s.mo == Some(actor_id))
                .map(|s| s.view_height)
            else {
                continue;
            };
            let [src, dst] = p.heights.map(f64::from);
            let (before, after) = (actor.pos.z() + view - src, z + view - src);
            // Fast falls may step over the band in one tic.
            if after.abs() >= FALL_LINK_BAND && before.signum() == after.signum() {
                continue;
            }
            let eye = if vel_z > 0.0 {
                dst + FALL_LINK_BAND
            } else {
                dst - FALL_LINK_BAND
            };
            if let Some(a) = world.actors.get_mut(actor_id) {
                a.pos = DVec3::new(
                    actor.pos.x() + f64::from(p.offset[0]),
                    actor.pos.y() + f64::from(p.offset[1]),
                    eye - view,
                );
                a.vel = DVec3::new(a.vel.x(), a.vel.y(), vel_z);
            }
            return true;
        }
        let water = p.kind == room_map::PORTAL_KIND_WATER;
        let (through_floor, through_ceil) = if water {
            let Some(up) = world
                .players
                .iter()
                .flatten()
                .find(|s| s.mo == Some(actor_id))
                .map(|s| s.cmd.up_move)
            else {
                continue;
            };
            (
                p.plane == 0 && z <= floor_z && (up < 0 || vel_z < -WATER_DIVE_SPEED),
                p.plane == 1 && z + actor.height >= ceiling_z && (up > 0 || vel_z > 0.0),
            )
        } else {
            (
                p.plane == 0 && vel_z < 0.0 && z <= floor_z,
                p.plane == 1 && vel_z > 0.0 && z + actor.height >= ceiling_z,
            )
        };
        if !through_floor && !through_ceil {
            continue;
        }
        let (dx, dy) = if water {
            (f64::from(p.offset[0]), f64::from(p.offset[1]))
        } else {
            let Some(src_c) = sector_xy_center(&world.map, p.sector.raw() as usize) else {
                continue;
            };
            let Some(dst_c) = sector_xy_center(&world.map, p.target.raw() as usize) else {
                continue;
            };
            (dst_c.x() - src_c.x(), dst_c.y() - src_c.y())
        };
        let nx = actor.pos.x() + dx;
        let ny = actor.pos.y() + dy;
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
        if is_skull_flying(&actor) {
            // Vanilla `P_XYMovement`: "the skull slammed into something".
            skull_slam(world, actor_id, None);
        }
        return;
    }
    // GZDoom `P_XYMovement`: step in increments no larger than `radius - 1`
    // so a small, fast projectile cannot tunnel through a thin door or wall
    // (a radius-2 tracer moving 50 units/tic would otherwise skip a door
    // sector entirely). Actors with radius ≤ 1 use `MAXMOVE`.
    let maxmove = if actor.radius - 1.0 > 0.0 {
        (actor.radius - 1.0).min(MAXMOVE)
    } else {
        MAXMOVE
    };
    let fastest = dx.abs().max(dy.abs());
    let steps = if fastest > maxmove {
        (1 + (fastest / maxmove) as i32).min(256)
    } else {
        1
    };
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
            if ((actor.flags & crate::class::FLAG_BOUNCEONWALLS) != 0 || mbf_bouncer(&actor))
                && mv.blocking_actor.is_none()
                && bounce_off_wall(world, actor_id, DVec3::new(nx, ny, z))
            {
                break;
            }
            if !is_missile && is_skull_flying(&actor) {
                if mv.blocking_actor.is_some() {
                    // `PIT_CheckThing` → `AActor::Slam`: damage and stop.
                    skull_slam(world, actor_id, mv.blocking_actor);
                } else if let Some(a) = world.actors.get_mut(actor_id) {
                    // Wall: XY momentum stops; the next tic's zero-velocity
                    // check ends the charge (`P_XYMovement`).
                    a.vel = DVec3::new(0.0, 0.0, a.vel.z());
                }
                break;
            }
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
                        let rip_sound = world
                            .actors
                            .get(actor_id)
                            .and_then(|a| world.classes.get(a.class))
                            .map(|c| c.ext.rip_sound.clone())
                            .unwrap_or_default();
                        if !rip_sound.is_empty() {
                            world.play_sound(&rip_sound, nx, ny, z, 1.0);
                        }
                        if let Some(a) = world.actors.get_mut(actor_id) {
                            a.pos = DVec3::new(nx, ny, z);
                        }
                        x = nx;
                        y = ny;
                        continue;
                    }
                    if let Some(passes_through) = reflect_missile_off_actor(world, actor_id, victim)
                    {
                        if passes_through {
                            if let Some(missile) = world.actors.get_mut(actor_id) {
                                missile.pos = DVec3::new(nx, ny, z);
                            }
                            x = nx;
                            y = ny;
                            continue;
                        }
                        break;
                    }
                    let pos = world
                        .actors
                        .get(actor_id)
                        .map(|a| a.pos)
                        .unwrap_or(DVec3::new(nx, ny, z));
                    set_missile_hit(
                        world,
                        actor_id,
                        room_script_abi::hit_kind::ACTOR,
                        Some(victim),
                        pos,
                        DVec3::ZERO,
                    );
                } else {
                    let pos = world
                        .actors
                        .get(actor_id)
                        .map(|a| a.pos)
                        .unwrap_or(DVec3::new(nx, ny, z));
                    set_missile_hit(
                        world,
                        actor_id,
                        room_script_abi::hit_kind::WALL,
                        None,
                        pos,
                        DVec3::ZERO,
                    );
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
    // Vanilla `P_XYMovement` returns before friction for `MF_SKULLFLY` too.
    if !is_missile && !is_skull_flying(&actor) {
        let class_fric = if actor.friction > 0.0 {
            actor.friction
        } else {
            world
                .classes
                .get(actor.class)
                .map(|c| c.friction)
                .unwrap_or(1.0)
        };
        let sector_fric = room_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
            .and_then(|sid| world.sectors.get(sid.raw() as usize))
            .map(|s| s.friction)
            .unwrap_or(ORIG_FRICTION);
        let mut factor = (sector_fric * class_fric).clamp(0.0, 1.0);
        // `+NOFRICTION`: keep XY velocity (UZ1). Missiles already skip this block.
        if actor.extra_flags.get_name("NOFRICTION") {
            factor = 1.0;
        }
        if let Some(a) = world.actors.get_mut(actor_id) {
            let (mut vx, mut vy) = if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
                use room_core::math::Fixed;
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

/// GZDoom `P_ReflectOffActor`: reflective actors redirect missiles unless
/// `THRUREFLECT` asks them to pass through and retarget. Returns `Some(true)`
/// for pass-through, `Some(false)` for a rebound, and `None` when unmarked.
fn reflect_missile_off_actor(
    world: &mut World,
    missile_id: ActorId,
    blocker_id: ActorId,
) -> Option<bool> {
    let blocker = world.actors.get(blocker_id)?;
    if !blocker.extra_flags.get_name("REFLECTIVE") {
        return None;
    }
    let thru = blocker.extra_flags.get_name("THRUREFLECT");
    let blocker_pos = blocker.pos;
    let blocker_target = blocker.target;
    let aim_reflect = blocker.extra_flags.get_name("AIMREFLECT");
    let mirror_reflect = blocker.extra_flags.get_name("MIRRORREFLECT");
    let Some(missile) = world.actors.get(missile_id) else {
        return None;
    };
    let old_target = missile.target;
    let velocity = missile.vel;
    let missile_pos = missile.pos;
    let missile_speed = missile.speed;
    let seeker = missile.extra_flags.get_name("SEEKERMISSILE");
    let aim_target = old_target.or(blocker_target);
    let aim_pos = if aim_reflect && !thru {
        aim_target
            .and_then(|target| world.actors.get(target))
            .map(|target| {
                DVec3::new(
                    target.pos.x(),
                    target.pos.y(),
                    target.pos.z() + target.height * 0.5,
                )
            })
    } else {
        None
    };
    if let Some(missile) = world.actors.get_mut(missile_id) {
        if seeker {
            missile.tracer = old_target;
        }
        missile.target = Some(blocker_id);
        if !thru {
            if let Some(target) = aim_pos {
                let dx = target.x() - missile_pos.x();
                let dy = target.y() - missile_pos.y();
                let dz = target.z() - missile_pos.z();
                let len = (dx * dx + dy * dy + dz * dz).sqrt();
                if len > 0.0 {
                    missile.vel = DVec3::new(
                        dx / len * missile_speed,
                        dy / len * missile_speed,
                        dz / len * missile_speed,
                    );
                    missile.angle = room_core::Angle::from_radians(room_core::dmath::atan2(dy, dx));
                }
            } else if mirror_reflect {
                missile.angle = room_core::Angle::from_degrees(missile.angle.degrees() + 180.0);
                missile.vel = DVec3::new(
                    -velocity.x() * 0.5,
                    -velocity.y() * 0.5,
                    -velocity.z() * 0.5,
                );
            } else {
                let dx = missile.pos.x() - blocker_pos.x();
                let dy = missile.pos.y() - blocker_pos.y();
                let len = (dx * dx + dy * dy).sqrt();
                if len > 0.0 {
                    let speed = (velocity.x() * velocity.x() + velocity.y() * velocity.y()).sqrt();
                    let reflected_speed = if missile.speed > 0.0 {
                        missile.speed
                    } else {
                        speed
                    };
                    missile.vel = DVec3::new(
                        dx / len * reflected_speed * 0.5,
                        dy / len * reflected_speed * 0.5,
                        -velocity.z() * 0.5,
                    );
                    missile.angle = room_core::Angle::from_radians(room_core::dmath::atan2(dy, dx));
                }
            }
        }
    }
    Some(thru)
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
    let dmg = missile_damage_roll(world, missile_id, &missile, "pr_missile");
    if dmg <= 0 {
        return;
    }
    // Doom `PIT_CheckThing`: no puff/blood on missile hit — only damage + explode.
    // Source is the shooter (`tmthing->target`), inflictor is the missile.
    let source = missile.target;
    let _ = crate::damage::damage_mobj(world, victim, Some(missile_id), source, dmg);
}

/// GZDoom `AActor::GetMissileDamage(7, 1)`: a parenthesised `Damage (expr)` is
/// the exact damage; a plain number is multiplied by 1d8.
fn missile_damage_roll(world: &mut World, id: ActorId, actor: &Actor, rng: &str) -> i32 {
    let mut exact = false;
    let base = if actor.damage > 0 {
        actor.damage
    } else {
        let expr = world
            .classes
            .get(actor.class)
            .map(|c| c.damage_expr.clone())
            .unwrap_or_default();
        if expr.is_empty() {
            world
                .classes
                .get(actor.class)
                .map(|c| c.damage)
                .unwrap_or(0)
        } else {
            exact = true;
            crate::deco_eval::eval_action_arg(world, id, &expr)
                .parse::<i32>()
                .unwrap_or(0)
                .max(0)
        }
    };
    if base <= 0 {
        return 0;
    }
    if exact {
        base
    } else {
        base * (i32::from(world.p_random(rng)) % 8 + 1)
    }
}

/// `MF_SKULLFLY` charge state (`A_SkullAttack`).
pub(crate) fn is_skull_flying(a: &Actor) -> bool {
    a.extra_flags.get_name("SKULLFLY")
}

/// GZDoom `AActor::Slam` / vanilla `PIT_CheckThing` + `P_XYMovement` skull
/// stop: clear `SKULLFLY`, zero velocity, damage the victim (melee damage
/// type, inflictor and source = the charger) and return to the See state.
pub(crate) fn skull_slam(world: &mut World, id: ActorId, victim: Option<ActorId>) {
    let Some(actor) = world.actors.get(id).cloned() else {
        return;
    };
    if let Some(a) = world.actors.get_mut(id) {
        a.extra_flags.set_name("SKULLFLY", false);
        a.vel = DVec3::ZERO;
    }
    if actor.health <= 0 {
        return;
    }
    if let Some(v) = victim {
        let dmg = missile_damage_roll(world, id, &actor, "pr_skullfly");
        if dmg > 0 {
            let _ = crate::damage::damage_mobj(world, v, Some(id), Some(id), dmg);
        }
    }
    if world.actors.get(id).is_some_and(|a| a.health > 0) {
        // Vanilla `PIT_CheckThing` returns to the spawn state; GZDoom `Slam`
        // to the See state when there is one.
        let vanilla = world.math_mode == crate::vanilla::SimMathMode::VanillaFixed;
        let st = if vanilla {
            world.states.find_label(actor.class, "Spawn")
        } else {
            world
                .states
                .find_label(actor.class, "See")
                .or_else(|| world.states.find_label(actor.class, "Spawn"))
        };
        if let Some(st) = st {
            world.set_state(id, Some(st), true);
        }
    }
}

/// Record missile impact metadata before [`explode_missile`] (`docs/31` MA4).
pub(crate) fn set_missile_hit(
    world: &mut World,
    actor_id: ActorId,
    kind: i32,
    hit_actor: Option<ActorId>,
    pos: DVec3,
    normal: DVec3,
) {
    if let Some(a) = world.actors.get_mut(actor_id) {
        a.hit_kind = kind;
        a.hit_actor = hit_actor;
        a.hit_pos = pos;
        a.hit_normal = normal;
    }
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
        // Default wall hit when callers forgot to set metadata.
        if a.hit_kind == 0 {
            a.hit_kind = room_script_abi::hit_kind::WALL;
            a.hit_pos = a.pos;
            a.hit_normal = DVec3::ZERO;
        }
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
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    #[test]
    fn missile_with_empty_death_label_vanishes() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Spark
{
  Radius 1
  Height 1
  Speed 6
  Projectile
  States
  {
  Spawn:
    SPKO B 1 Bright
    SPKO B 3 Bright
    stop
  Death:
    Stop
  }
}
actor Other
{
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(200.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let spark = classes.find_name("Spark").unwrap();
        let id = w
            .spawn_at(
                spark,
                DVec3::new(100.0, 128.0, 40.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        explode_missile(&mut w, id);
        let a = w.actors.get(id).unwrap();
        assert!(
            a.destroyed,
            "`Death: Stop` must remove the actor, state={:?}",
            a.state
        );
    }

    #[test]
    fn fast_small_missile_stops_at_closed_door() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Bullet
{
  Radius 2
  Height 2
  Speed 50
  Projectile
  States { Spawn: TRAC A -1 Stop Death: TNT1 A 1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("door", rect(256.0, 0.0, 264.0, 256.0), 0.0, 0.0)
            .room("b", rect(264.0, 0.0, 520.0, 256.0), 0.0, 128.0)
            .player_start(400.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let bullet = classes.find_name("Bullet").unwrap();
        for (speed, dy) in [(50.0, 0.0), (140.0, 0.0), (50.0, 17.0), (140.0, 31.0)] {
            let id = w
                .spawn_at(
                    bullet,
                    DVec3::new(200.0, 128.0, 40.0),
                    room_core::Angle::ZERO,
                )
                .unwrap();
            if let Some(a) = w.actors.get_mut(id) {
                a.vel = DVec3::new(speed, dy, 0.0);
            }
            for _ in 0..10 {
                xy_movement(&mut w, id);
            }
            let a = w.actors.get(id).unwrap();
            assert!(
                a.pos.x() < 256.0,
                "speed {speed}: bullet went through the door to {:?}",
                a.pos
            );
        }
    }

    #[test]
    fn hitscan_stops_at_closed_door() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Shooter : Actor 9941
{
  Health 100
  Radius 20
  Height 56
  +SOLID
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("door", rect(256.0, 0.0, 264.0, 256.0), 0.0, 0.0)
            .room("b", rect(264.0, 0.0, 520.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let shooter = classes.find_name("Shooter").unwrap();
        let victim = w
            .spawn_at(
                shooter,
                DVec3::new(400.0, 128.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        let src = w.players[0].as_ref().unwrap().mo.unwrap();
        for deg in [0.0, 3.0, -3.0] {
            let hit = crate::hitscan::line_attack(
                &mut w,
                src,
                room_core::Angle::from_degrees(deg),
                2048.0,
                10,
            );
            assert_ne!(
                hit.actor,
                Some(victim),
                "angle {deg}: shot went through the door"
            );
        }
        assert_eq!(w.actors.get(victim).unwrap().health, 100);
    }

    #[test]
    fn ripper_missile_damages_without_exploding() {
        let ir = room_decorate::parse_decorate(
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
            .spawn_at(dummy, DVec3::new(160.0, 64.0, 0.0), room_core::Angle::ZERO)
            .unwrap();
        let v2 = w
            .spawn_at(dummy, DVec3::new(220.0, 64.0, 0.0), room_core::Angle::ZERO)
            .unwrap();
        let missile = w
            .spawn_at(shot, DVec3::new(100.0, 64.0, 32.0), room_core::Angle::ZERO)
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
            .spawn_at(dummy, DVec3::new(160.0, 160.0, 0.0), room_core::Angle::ZERO)
            .unwrap();
        let v4 = w
            .spawn_at(dummy, DVec3::new(220.0, 160.0, 0.0), room_core::Angle::ZERO)
            .unwrap();
        let flag_shot = classes.find_name("RipFlagShot").unwrap();
        let missile2 = w
            .spawn_at(
                flag_shot,
                DVec3::new(100.0, 160.0, 32.0),
                room_core::Angle::ZERO,
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
        let ir = room_decorate::parse_decorate(
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
            .spawn_at(mate, DVec3::new(200.0, 64.0, 0.0), room_core::Angle::ZERO)
            .unwrap();
        let hp0 = w.actors.get(victim).unwrap().health;
        let mid = w
            .spawn_at(shot, DVec3::new(160.0, 64.0, 32.0), room_core::Angle::ZERO)
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
        use room_decorate::parse_decorate;
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
            .spawn_at(zombie, DVec3::new(200.0, 64.0, 0.0), room_core::Angle::ZERO)
            .unwrap();
        let hp_before = w.actors.get(victim).unwrap().health;
        let missile = w
            .spawn_at(ball, DVec3::new(160.0, 64.0, 32.0), room_core::Angle::ZERO)
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
    }

    #[test]
    fn missile_damage_expression_random_hurts() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let mut src = include_str!("../../../testdata/actors/doom_basic.txt").to_owned();
        src.push_str(
            r#"
actor ExprBall : Actor
{
  Health 1000
  Radius 6
  Height 8
  Speed 10
  Damage (random(4, 4))
  Projectile
  States
  {
  Spawn:
    BAL1 A 4
    Loop
  Death:
    BAL1 B 6
    Stop
  }
}
"#,
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let ball = classes.find_name("ExprBall").unwrap();
        assert!(
            classes
                .get(ball)
                .unwrap()
                .damage_expr
                .to_ascii_lowercase()
                .contains("random"),
            "expr stored"
        );
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(400.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let zombie = w.classes.find_name("ZombieMan").unwrap();
        let victim = w
            .spawn_at(zombie, DVec3::new(200.0, 64.0, 0.0), room_core::Angle::ZERO)
            .unwrap();
        let hp_before = w.actors.get(victim).unwrap().health;
        let missile = w
            .spawn_at(ball, DVec3::new(160.0, 64.0, 32.0), room_core::Angle::ZERO)
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
        // GZDoom: `Damage (expr)` is exact (no 1d8 multiplier).
        assert_eq!(hp_after, hp_before - 4, "expression damage");
        let death = w.states.find_label(ball, "Death").unwrap();
        assert_eq!(w.actors.get(missile).unwrap().state, Some(death));
        // Doom `PIT_CheckThing`: missiles damage + explode; blood is hitscan-only.
    }

    #[test]
    fn missile_damage_uses_dice_roll() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
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
            .spawn_at(zombie, DVec3::new(200.0, 64.0, 0.0), room_core::Angle::ZERO)
            .unwrap();
        // Keep the zombie alive through one hit so we can measure damage.
        if let Some(a) = w.actors.get_mut(victim) {
            a.health = 10_000;
        }
        let hp_before = w.actors.get(victim).unwrap().health;
        let missile = w
            .spawn_at(ball, DVec3::new(160.0, 64.0, 32.0), room_core::Angle::ZERO)
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
        use room_decorate::parse_decorate;
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
            .spawn_at(ball, DVec3::new(15.0, 128.0, 32.0), room_core::Angle::ZERO)
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
        use room_decorate::parse_decorate;
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
            .spawn_at(ball, DVec3::new(128.0, 128.0, 8.0), room_core::Angle::ZERO)
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
    fn doom_bounce_limb_settles_into_its_death_state() {
        // Brutal Doom limbs (`XDeathArm1`) are `+DOOMBOUNCE` gravity missiles
        // that tumble in a looping Spawn state until they die. GZDoom ends the
        // bounce (`BOUNCE_AutoOff` below 3 units/tic up, or `BounceCount`), and
        // every bounce keeps only `BounceFactor` of the whole velocity.
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let file = parse_decorate(
            r#"
actor Limb 9994
{
  Radius 8
  Height 8
  Speed 8
  Gravity 0.5
  BounceFactor 0.2
  WallBounceFactor 0.2
  BounceCount 3
  +DOOMBOUNCE
  +MISSILE
  +NOBLOCKMAP
  States
  {
  Spawn:
    XARM ABCD 5
    Loop
  Death:
    XDT5 I -1
    Stop
  }
}
actor Grenade : Limb 9995
{
  BounceFactor 0.9
  BounceCount 2
  -DOOMBOUNCE
  +HEXENBOUNCE
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 512.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w =
            World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
        let limb = classes.find_name("Limb").unwrap();
        let death = w.states.find_label(limb, "Death").unwrap();
        let id = w
            .spawn_at(
                limb,
                DVec3::new(1024.0, 1024.0, 40.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.vel = DVec3::new(6.0, 2.0, 4.0);
        }
        let mut died = None;
        for t in 0..200 {
            w.tick();
            if w.actors.get(id).is_some_and(|a| a.state == Some(death)) {
                died = Some(t);
                break;
            }
        }
        assert!(
            died.is_some(),
            "a Doom-bounce limb must stop tumbling and die"
        );

        // `BounceCount 2` on a lively Hexen bouncer (no auto-off): the second
        // floor contact explodes it.
        let grenade = classes.find_name("Grenade").unwrap();
        let gdeath = w.states.find_label(grenade, "Death").unwrap();
        let g = w
            .spawn_at(
                grenade,
                DVec3::new(512.0, 512.0, 64.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(g) {
            a.vel = DVec3::new(0.0, 0.0, -10.0);
        }
        let mut floor_hits = 0;
        let mut was_up = true;
        for _ in 0..300 {
            w.tick();
            let a = w.actors.get(g).unwrap();
            if a.state == Some(gdeath) {
                break;
            }
            let down = a.pos.z() <= 0.0;
            if down && was_up {
                floor_hits += 1;
            }
            was_up = !down;
        }
        assert_eq!(
            w.actors.get(g).unwrap().state,
            Some(gdeath),
            "BounceCount 2 must end it"
        );
        assert!(
            floor_hits <= 1,
            "exploded on the second floor contact, saw {floor_hits} rebounds"
        );
    }

    #[test]
    fn bouncers_reflect_off_floors_and_walls() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let file = parse_decorate(
            "actor Bomb 9990\n{\n  Radius 4\n  Height 4\n  BounceFactor 0.5\n  \
             +BOUNCEONFLOORS\n  +BOUNCEONWALLS\n  States\n  {\n  Spawn:\n    BOMB A -1\n    Stop\n  }\n}\n",
        )
        .unwrap();
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
        let bomb = classes.find_name("Bomb").unwrap();
        let id = w
            .spawn_at(bomb, DVec3::new(128.0, 128.0, 4.0), room_core::Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.vel = DVec3::new(0.0, 0.0, -9.0);
        }
        z_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        assert_eq!(a.pos.z(), 0.0);
        assert_eq!(a.vel.z(), 5.0, "(9 + 1 gravity) * 0.5 back up");

        let id = w
            .spawn_at(bomb, DVec3::new(128.0, 200.0, 4.0), room_core::Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.flags &= !crate::class::FLAG_BOUNCEONFLOORS;
            a.extra_flags.set_name("MBFBOUNCER", true);
            a.vel = DVec3::new(0.0, 0.0, -9.0);
        }
        z_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        assert_eq!(a.vel.z(), 5.0, "MBFBOUNCER rebounds off the floor");

        let id = w
            .spawn_at(bomb, DVec3::new(10.0, 128.0, 0.0), room_core::Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.vel = DVec3::new(-12.0, 4.0, 0.0);
        }
        xy_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        assert!(a.vel.x() > 0.0, "reflected off the west wall: {:?}", a.vel);
        assert!(a.vel.y() > 0.0);
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
                room_core::ClassId::from_raw(0),
                DVec3::new(100.0, 64.0, 0.0),
                room_core::Angle::ZERO,
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
        let ir = room_decorate::parse_decorate(
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
        let ir = room_decorate::parse_decorate(
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
        let ir = room_decorate::parse_decorate(
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
        let ir = room_decorate::parse_decorate(
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
                room_core::Angle::ZERO,
            )
            .unwrap();
        // Doom `P_TryMove` "don't stand over a dropoff": the box at x=248
        // (radius 16) spans the 40-unit ledge at x=256, so floorz − dropoffz
        // is 40 > MaxDropOffHeight 24 → blocked without DROPOFF.
        assert!(!try_move(&mut w, cautious, DVec3::new(248.0, 128.0, 40.0)));
        let daredevil = w
            .spawn_at(
                reckless,
                DVec3::new(128.0, 128.0, 40.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        assert!(try_move(&mut w, daredevil, DVec3::new(248.0, 128.0, 40.0)));
        assert!((w.actors.get(daredevil).unwrap().pos.x() - 248.0).abs() < 1e-6);
    }

    #[test]
    fn try_move_rejects_step_above_max_step_height() {
        let ir = room_decorate::parse_decorate(
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
            .spawn_at(short, DVec3::new(200.0, 128.0, 0.0), room_core::Angle::ZERO)
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
    fn falldamage_flag_hurts_without_the_level_switch() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert!(!w.level.falling_damage);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.flags |= crate::class::FLAG_SHOOTABLE;
            a.health = 100;
            a.mass = 100;
            a.extra_flags.set_name("FALLDAMAGE", true);
        }
        try_falling_damage(&mut w, mo, -40.0);
        assert!(w.actors.get(mo).unwrap().health < 100);
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
        let file = room_decorate::parse_decorate(src).unwrap();
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
            .spawn_at(ball, DVec3::new(64.0, 64.0, 40.0), room_core::Angle::ZERO)
            .unwrap();
        z_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        assert!((a.pos.z() - 40.0).abs() < 1e-9, "z={}", a.pos.z());
        assert_eq!(a.vel.z(), 0.0);
    }

    #[test]
    fn z_movement_scales_by_class_gravity() {
        let ir = room_decorate::parse_decorate(
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
            .spawn_at(cls, DVec3::new(64.0, 64.0, 40.0), room_core::Angle::ZERO)
            .unwrap();
        z_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        // One tic: vel_z = -0.5, z = 39.5
        assert!((a.vel.z() + 0.5).abs() < 1e-9, "vel_z={}", a.vel.z());
        assert!((a.pos.z() - 39.5).abs() < 1e-9, "z={}", a.pos.z());
    }

    #[test]
    fn xy_movement_scales_by_class_friction() {
        let ir = room_decorate::parse_decorate(
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
            .spawn_at(cls, DVec3::new(128.0, 128.0, 0.0), room_core::Angle::ZERO)
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
        let ir = room_decorate::parse_decorate(
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
            .spawn_at(cls, DVec3::new(128.0, 128.0, 0.0), room_core::Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.vel = DVec3::new(16.0, 0.0, 0.0);
        }
        xy_movement(&mut w, id);
        let a = w.actors.get(id).unwrap();
        let factor = ORIG_FRICTION * 0.5;
        let expected = (room_core::math::Fixed::from_f64(16.0)
            * room_core::math::Fixed::from_f64(factor))
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
        use room_decorate::parse_decorate;
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
                room_core::Angle::ZERO,
            )
            .unwrap();
        let walker = w
            .spawn_at(
                classes.find_name("Walker").unwrap(),
                DVec3::new(160.0, 64.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        assert!(try_move(&mut w, walker, DVec3::new(200.0, 64.0, 0.0),));
        assert!((w.actors.get(walker).unwrap().pos.x() - 200.0).abs() < 1e-6);
    }

    #[test]
    fn nointeraction_solid_does_not_block_move() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor GoreTrail : Actor 9312
{
  Radius 20
  Height 56
  +SOLID
  +NOINTERACTION
  States { Spawn: BLUD C -1 Stop }
}
actor Walker2 : Actor 9313
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
                .get(classes.find_name("GoreTrail").unwrap())
                .unwrap()
                .flags
                & FLAG_NOINTERACTION,
            FLAG_NOINTERACTION
        );
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let _gore = w
            .spawn_at(
                classes.find_name("GoreTrail").unwrap(),
                DVec3::new(200.0, 64.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        let walker = w
            .spawn_at(
                classes.find_name("Walker2").unwrap(),
                DVec3::new(160.0, 64.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        assert!(try_move(&mut w, walker, DVec3::new(200.0, 64.0, 0.0),));
        assert!((w.actors.get(walker).unwrap().pos.x() - 200.0).abs() < 1e-6);
    }

    #[test]
    fn bd8_nointeraction_gore_cloud_ticks_deterministically() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor GoreBit : Actor 9320
{
  Radius 4
  Height 4
  +NOINTERACTION
  +NOBLOCKMAP
  +NOGRAVITY
  States { Spawn: BLUD C -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let gore = classes.find_name("GoreBit").unwrap();
        let mk = |classes: &Arc<ClassRegistry>, states: &Arc<StateTable>| {
            let mut map = MapBuilder::new()
                .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
                .player_start(32.0, 32.0, 0)
                .build_udmf("MAP01")
                .unwrap();
            apply_nodes(&mut map);
            let mut w =
                World::from_map_with_classes(map, Arc::clone(classes), Arc::clone(states)).unwrap();
            for i in 0..512 {
                let x = 64.0 + f64::from(i % 32) * 8.0;
                let y = 64.0 + f64::from(i / 32) * 8.0;
                let _ = w.spawn_at(gore, DVec3::new(x, y, 16.0), room_core::Angle::ZERO);
            }
            for _ in 0..8 {
                w.tick();
            }
            w.checksum()
        };
        let a = mk(&classes, &states);
        let b = mk(&classes, &states);
        assert_eq!(
            a, b,
            "512 NOINTERACTION gore actors × 8 tics must be deterministic"
        );
    }

    #[test]
    fn airborne_actor_over_flat_floor_is_not_at_a_dropoff() {
        // Brutal Doom gibs are gravity missiles spawned at chest height by
        // `A_CustomMissile`. Doom's dropoff test compares the floors under the
        // box (`tmfloorz - tmdropoffz`), so `P_CheckMissileSpawn` lets them
        // fly; comparing the floor with the actor's z exploded them in mid-air
        // and left their wall smears floating in the room.
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor FlyingGib : Actor 9322
{
  Radius 1
  Height 1
  +MISSILE
  +NOBLOCKMAP
  +THRUACTORS
  States { Spawn: BLUD C -1 Stop }
}
actor Walker : Actor 9323
{
  Radius 16
  Height 56
  States { Spawn: TROO A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let gib = classes.find_name("FlyingGib").unwrap();
        let walker = classes.find_name("Walker").unwrap();
        let mut map = MapBuilder::new()
            .room("high", rect(0.0, 0.0, 256.0, 256.0), 64.0, 256.0)
            .room("low", rect(256.0, 0.0, 512.0, 256.0), 0.0, 256.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let g = w
            .spawn_at(gib, DVec3::new(100.0, 128.0, 106.0), room_core::Angle::ZERO)
            .unwrap();
        assert!(
            try_move(&mut w, g, DVec3::new(104.0, 128.0, 106.0)),
            "42 units up over flat floor"
        );
        // A walker still stops at a 64-unit ledge (box over both floors).
        let m = w
            .spawn_at(
                walker,
                DVec3::new(236.0, 128.0, 64.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        assert!(
            !try_move(&mut w, m, DVec3::new(248.0, 128.0, 64.0)),
            "ledge must still block"
        );
    }

    #[test]
    fn bd8_three_k_gore_soft_budget() {
        // docs/33 BD8: 3,000 gore ≤ 4 ms/tic desktop — CI uses a loose smoke ceiling.
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;
        use std::time::Instant;

        let ir = parse_decorate(
            r#"
actor GoreBit3k : Actor 9321
{
  Radius 4
  Height 4
  +NOINTERACTION
  +NOBLOCKMAP
  +NOGRAVITY
  States { Spawn: BLUD C -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let gore = classes.find_name("GoreBit3k").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        for i in 0..3000 {
            let x = 64.0 + f64::from(i % 64) * 8.0;
            let y = 64.0 + f64::from(i / 64) * 8.0;
            let _ = w.spawn_at(gore, DVec3::new(x, y, 16.0), room_core::Angle::ZERO);
        }
        let start = Instant::now();
        const TICS: u32 = 8;
        for _ in 0..TICS {
            w.tick();
        }
        let ms = start.elapsed().as_millis();
        let per = ms / u128::from(TICS);
        // Smoke: 500 ms/tic ceiling (CI); criterion / local machines target ≤ 4 ms.
        assert!(
            per < 500,
            "3000 NOINTERACTION gore avg {per} ms/tic (soft smoke 500; BD8 desktop ≤4)"
        );
    }

    #[test]
    fn bd8_corpse_queue_evicts_oldest() {
        use crate::class::FLAG_CORPSE;
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor CorpseBit : Actor 9322
{
  Radius 16
  Height 16
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let cid = classes.find_name("CorpseBit").unwrap();
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        w.corpse_queue_size = 2;
        let mut ids = Vec::new();
        for i in 0..3 {
            let id = w
                .spawn_at(
                    cid,
                    DVec3::new(64.0 + f64::from(i) * 40.0, 64.0, 0.0),
                    room_core::Angle::ZERO,
                )
                .unwrap();
            if let Some(a) = w.actors.get_mut(id) {
                a.flags |= FLAG_CORPSE;
            }
            w.queue_corpse(id);
            ids.push(id);
        }
        assert_eq!(w.corpse_queue.len(), 2);
        assert!(
            w.actors.get(ids[0]).is_some_and(|a| a.destroyed),
            "oldest corpse must be destroyed"
        );
        assert!(w.actors.get(ids[1]).is_some_and(|a| !a.destroyed));
        assert!(w.actors.get(ids[2]).is_some_and(|a| !a.destroyed));
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
        map.setup.midtex_attach.push(room_map::MidtexAttach {
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
    fn water_link_dives_players_only_when_swimming_down() {
        let mut map = MapBuilder::new()
            .room("above", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("under", rect(256.0, 0.0, 512.0, 256.0), -256.0, -64.0)
            .sector_tag("under", 5)
            .sector_portal_between("above", "under", 5, 200, 0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        for p in &mut map.setup.portals.sectors {
            p.offset = [300, 10];
        }
        let mut w = World::from_map(map).unwrap();
        let id = w.players[0].as_ref().unwrap().mo.unwrap();
        z_movement(&mut w, id);
        assert!(w.actors.get(id).unwrap().pos.x() < 256.0, "standing stays");
        w.players[0].as_mut().unwrap().cmd.up_move = -10;
        z_movement(&mut w, id);
        let pos = w.actors.get(id).unwrap().pos;
        assert!(
            (pos.x() - 364.0).abs() < 1e-6 && (pos.y() - 74.0).abs() < 1e-6,
            "{pos:?}"
        );
        assert!(
            pos.z() < -64.0 && pos.z() > -128.0,
            "below the ceiling: {pos:?}"
        );
    }

    #[test]
    fn swimmable_water_reduces_gravity_and_applies_upmove() {
        use room_core::SectorId;
        use room_map::setup::FFloorStatic;

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
        let si = room_nodebuild::point_in_sector(&w.map, 128.0, 128.0)
            .or_else(|| room_nodebuild::point_in_sector_vanilla(&w.map, 128.0, 128.0))
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
