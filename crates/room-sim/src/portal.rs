//! Playsim side of line and sector portals (`docs/06-maps-geometry.md` §5).
//!
//! Ported from UZDoom (GPL-3.0-or-later): `AActor::PosRelative` /
//! `Vec2To`, `HighestCeilingAt` / `LowestFloorAt`, `NextHighestCeilingAt` /
//! `NextLowestFloorAt` (portal part), `AActor::CheckPortalTransition`, and
//! the line-portal crossing of `P_TryMove`.
//!
//! Every helper is a no-op on maps without portals, so vanilla maps, demos
//! and checksums are unchanged.

use crate::actor::{Actor, ActorId};
use crate::world::World;
use room_core::{DVec2, DVec3, LineId};
use room_map::portals::{LinePortal, PLANE_CEILING, PLANE_FLOOR};

/// The map has portals of any kind the playsim cares about.
#[must_use]
pub fn has_playsim_portals(world: &World) -> bool {
    world.map.setup.portal_groups.has_links() || world.map.setup.portals.has_passable_lines
}

/// Sector at a point (nodes, then the vanilla fallback).
#[must_use]
pub fn sector_at(world: &World, x: f64, y: f64) -> Option<usize> {
    crate::physics::sector_at_xy(world, x, y)
}

/// Portal group of a sector (0 when the map has no linked portals).
#[must_use]
pub fn sector_group(world: &World, sector: usize) -> u32 {
    world.map.setup.portal_groups.group_of(sector).unwrap_or(0)
}

/// Portal group at a point.
#[must_use]
pub fn group_at(world: &World, x: f64, y: f64) -> u32 {
    if !world.map.setup.portal_groups.has_links() {
        return 0;
    }
    sector_at(world, x, y).map_or(0, |s| sector_group(world, s))
}

/// Portal group an actor is in.
#[must_use]
pub fn actor_group(world: &World, a: &Actor) -> u32 {
    group_at(world, a.pos.x(), a.pos.y())
}

/// `Displacements.getOffset(from, to)`.
#[must_use]
pub fn displacement(world: &World, from: u32, to: u32) -> (f64, f64) {
    world.map.setup.portal_groups.offset(from, to)
}

/// `AActor::PosRelative(group)`: an actor's position expressed in another
/// group's coordinates.
#[must_use]
pub fn pos_relative(world: &World, a: &Actor, group: u32) -> DVec3 {
    let (dx, dy) = displacement(world, actor_group(world, a), group);
    DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z())
}

/// `AActor::Vec2To(other)`: `other − self` across linked portals.
#[must_use]
pub fn vec2_to(world: &World, from: &Actor, to: &Actor) -> DVec2 {
    if !world.map.setup.portal_groups.has_links() {
        return DVec2::new(to.pos.x() - from.pos.x(), to.pos.y() - from.pos.y());
    }
    let p = pos_relative(world, to, actor_group(world, from));
    DVec2::new(p.x() - from.pos.x(), p.y() - from.pos.y())
}

/// `AActor::Vec3To(other)`.
#[must_use]
pub fn vec3_to(world: &World, from: &Actor, to: &Actor) -> DVec3 {
    let v = vec2_to(world, from, to);
    DVec3::new(v.x(), v.y(), to.pos.z() - from.pos.z())
}

/// Position of `to` as seen from `from`'s group (`PosRelative`).
#[must_use]
pub fn pos_seen_from(world: &World, from: &Actor, to: &Actor) -> DVec3 {
    if !world.map.setup.portal_groups.has_links() {
        return to.pos;
    }
    pos_relative(world, to, actor_group(world, from))
}

/// Live plane height of a sector (no slope).
fn live_plane(world: &World, sector: usize, plane: usize) -> f64 {
    world.sectors.get(sector).map_or(0.0, |s| {
        if plane == PLANE_FLOOR {
            s.floor_z
        } else {
            s.ceiling_z
        }
    })
}

/// `sector_t::PortalBlocksMovement`, with the obstruction re-checked
/// against the live plane (UZDoom calls `CheckPortalPlane` when planes move).
#[must_use]
pub fn plane_blocks_movement(world: &World, sector: usize, plane: usize) -> bool {
    let t = &world.map.setup.portals;
    if !t.plane_is_linked(sector, plane) {
        return true;
    }
    let flags = t.flags(sector, plane) & !room_map::portals::PLANEF_OBSTRUCTED;
    if flags & (room_map::portals::PLANEF_NOPASS | room_map::portals::PLANEF_DISABLED) != 0 {
        return true;
    }
    plane_obstructed(world, sector, plane)
}

/// `sector_t::PortalBlocksSight` (live obstruction).
#[must_use]
pub fn plane_blocks_sight(world: &World, sector: usize, plane: usize) -> bool {
    let t = &world.map.setup.portals;
    if !t.plane_is_linked(sector, plane) {
        return true;
    }
    let flags = t.flags(sector, plane);
    if flags
        & (room_map::portals::PLANEF_NORENDER
            | room_map::portals::PLANEF_NOPASS
            | room_map::portals::PLANEF_DISABLED)
        != 0
    {
        return true;
    }
    plane_obstructed(world, sector, plane)
}

/// `sector_t::PortalBlocksSound` (live obstruction).
#[must_use]
pub fn plane_blocks_sound(world: &World, sector: usize, plane: usize) -> bool {
    let t = &world.map.setup.portals;
    if !t.plane_is_linked(sector, plane) {
        return true;
    }
    let flags = t.flags(sector, plane);
    if flags & (room_map::portals::PLANEF_BLOCKSOUND | room_map::portals::PLANEF_DISABLED) != 0 {
        return true;
    }
    plane_obstructed(world, sector, plane)
}

/// `CheckPortalPlane`: the sector's own plane is past the portal plane.
#[must_use]
pub fn plane_obstructed(world: &World, sector: usize, plane: usize) -> bool {
    let portal_z = world.map.setup.portals.plane_z(sector, plane);
    let h = live_plane(world, sector, plane);
    if plane == PLANE_FLOOR {
        h > portal_z + 1e-9
    } else {
        h < portal_z - 1e-9
    }
}

fn plane_z(world: &World, sector: usize, plane: usize) -> f64 {
    world.map.setup.portals.plane_z(sector, plane)
}

fn plane_disp(world: &World, sector: usize, plane: usize) -> (f64, f64) {
    world.map.setup.portals.plane_displacement(sector, plane)
}

/// `NextHighestCeilingAt` (portal walk only; 3D floors stay with the
/// callers). Returns the ceiling height and the sector it belongs to.
#[must_use]
pub fn ceiling_through_portals(world: &World, sector: usize, x: f64, y: f64) -> (f64, usize) {
    let (mut sec, mut px, mut py) = (sector, x, y);
    let mut planeheight = f64::MIN;
    for _ in 0..16 {
        if plane_blocks_movement(world, sec, PLANE_CEILING)
            || planeheight >= plane_z(world, sec, PLANE_CEILING)
        {
            break;
        }
        let (dx, dy) = plane_disp(world, sec, PLANE_CEILING);
        px += dx;
        py += dy;
        planeheight = plane_z(world, sec, PLANE_CEILING);
        let Some(next) = sector_at(world, px, py) else {
            break;
        };
        sec = next;
    }
    (sector_plane_z_at(world, sec, px, py, false), sec)
}

/// `NextLowestFloorAt` (portal walk only).
#[must_use]
pub fn floor_through_portals(world: &World, sector: usize, x: f64, y: f64) -> (f64, usize) {
    let (mut sec, mut px, mut py) = (sector, x, y);
    let mut planeheight = f64::MAX;
    for _ in 0..16 {
        if plane_blocks_movement(world, sec, PLANE_FLOOR)
            || planeheight <= plane_z(world, sec, PLANE_FLOOR)
        {
            break;
        }
        let (dx, dy) = plane_disp(world, sec, PLANE_FLOOR);
        px += dx;
        py += dy;
        planeheight = plane_z(world, sec, PLANE_FLOOR);
        let Some(next) = sector_at(world, px, py) else {
            break;
        };
        sec = next;
    }
    (sector_plane_z_at(world, sec, px, py, true), sec)
}

/// Floor / ceiling Z at a point of a sector (slopes + movers), as the
/// physics code computes it.
fn sector_plane_z_at(world: &World, si: usize, x: f64, y: f64, floor: bool) -> f64 {
    crate::physics::sector_z_at_pub(world, si, x, y, floor)
}

/// A linked plane portal anywhere in the map.
#[must_use]
pub fn has_linked_planes(world: &World) -> bool {
    world.map.setup.portals.has_linked_planes
}

/// Shift an actor by an XY offset, recording it for render interpolation.
fn shift_actor(world: &mut World, id: ActorId, dx: f64, dy: f64, dz: f64, dangle: f64) {
    if let Some(a) = world.actors.get_mut(id) {
        a.pos = DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z() + dz);
    }
    world.portal_moves.push(PortalMove {
        actor: id,
        offset: [dx, dy, dz],
        angle: dangle,
    });
}

/// `AActor::CheckPortalTransition`: move an actor into the group above or
/// below when it passed a linked plane portal. Returns true if it moved.
pub fn check_portal_transition(world: &mut World, id: ActorId) -> bool {
    if !has_linked_planes(world) {
        return false;
    }
    let mut moved = false;
    for _ in 0..8 {
        let Some(a) = world.actors.get(id) else {
            return moved;
        };
        let Some(sec) = sector_at(world, a.pos.x(), a.pos.y()) else {
            return moved;
        };
        if plane_blocks_movement(world, sec, PLANE_CEILING)
            || a.pos.z() < plane_z(world, sec, PLANE_CEILING)
        {
            break;
        }
        let (dx, dy) = plane_disp(world, sec, PLANE_CEILING);
        shift_actor(world, id, dx, dy, 0.0, 0.0);
        moved = true;
    }
    if moved {
        return true;
    }
    for _ in 0..8 {
        let Some(a) = world.actors.get(id) else {
            return moved;
        };
        let Some(sec) = sector_at(world, a.pos.x(), a.pos.y()) else {
            return moved;
        };
        if plane_blocks_movement(world, sec, PLANE_FLOOR) {
            break;
        }
        let portal_z = plane_z(world, sec, PLANE_FLOOR);
        // UZDoom also wants `floorz < portalz`: an actor standing on a floor
        // above the portal plane (a 3D floor) stays.
        let floor_here = sector_plane_z_at(world, sec, a.pos.x(), a.pos.y(), true);
        if !(a.pos.z() < portal_z && floor_here.min(portal_z) < portal_z + 1e-9) {
            break;
        }
        let (dx, dy) = plane_disp(world, sec, PLANE_FLOOR);
        shift_actor(world, id, dx, dy, 0.0, 0.0);
        moved = true;
    }
    moved
}

/// UZDoom `CalcPosVel`: sound positions in the listener's portal group,
/// so a sound across a linked portal comes from where it is seen.
pub fn relocate_sounds_for_listener(
    world: &World,
    events: &mut [crate::world::SimSoundEvent],
    listener: Option<ActorId>,
) {
    if !world.map.setup.portal_groups.has_links() {
        return;
    }
    let Some(l) = listener.and_then(|id| world.actors.get(id)) else {
        return;
    };
    let lg = actor_group(world, l);
    for e in events {
        let sg = group_at(world, e.x, e.y);
        let (dx, dy) = displacement(world, lg, sg);
        e.x -= dx;
        e.y -= dy;
    }
}

/// One portal move of this tic, for the shells' render interpolation: the
/// previous position should move by the same offset (UZDoom `Prev +=`).
#[derive(Clone, Copy, Debug)]
pub struct PortalMove {
    /// Moved actor.
    pub actor: ActorId,
    /// XYZ offset applied.
    pub offset: [f64; 3],
    /// Yaw change, degrees.
    pub angle: f64,
}

/// `P_PointOnLineSide` for a portal line: 0 front, 1 back.
fn point_side(world: &World, lid: LineId, p: DVec2) -> u8 {
    let Some(line) = world.map.lines.get(lid.raw() as usize) else {
        return 0;
    };
    let v1 = world.map.vertices[line.v1.raw() as usize].pos;
    let d = line.delta;
    // Back when the point is on the left of v1 → v2 (Doom convention).
    let cross = (p.y() - v1.y()) * d.x() - (p.x() - v1.x()) * d.y();
    u8::from(cross > 0.0)
}

/// The first passable portal line a move from `from` to `to` crosses from
/// its front to its back (`P_TryMove`'s `portalhit` search), with the
/// fraction along the move.
#[must_use]
pub fn crossed_line_portal(world: &World, from: DVec2, to: DVec2) -> Option<(LinePortal, f64)> {
    let t = &world.map.setup.portals;
    let mut best: Option<(LinePortal, f64)> = None;
    for p in &t.lines {
        if !p.is_traversable() || p.target.is_none() {
            continue;
        }
        if point_side(world, p.line, from) != 0 || point_side(world, p.line, to) != 1 {
            continue;
        }
        let Some(line) = world.map.lines.get(p.line.raw() as usize) else {
            continue;
        };
        let v1 = world.map.vertices[line.v1.raw() as usize].pos;
        let v2 = world.map.vertices[line.v2.raw() as usize].pos;
        let Some(frac) = segment_hit(from, to, v1, v2) else {
            continue;
        };
        if best.is_none_or(|(_, f)| frac < f) {
            best = Some((*p, frac));
        }
    }
    best
}

/// Fraction along `a → b` where it crosses the segment `c → d` (both
/// inclusive), if it does.
#[must_use]
pub fn segment_hit(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<f64> {
    let r = DVec2::new(b.x() - a.x(), b.y() - a.y());
    let s = DVec2::new(d.x() - c.x(), d.y() - c.y());
    let den = r.x() * s.y() - r.y() * s.x();
    if den.abs() < 1e-12 {
        return None;
    }
    let qp = DVec2::new(c.x() - a.x(), c.y() - a.y());
    let t = (qp.x() * s.y() - qp.y() * s.x()) / den;
    let u = (qp.x() * r.y() - qp.y() * r.x()) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// Front-sector plane heights at the ends of the portal's lines
/// (`P_TranslatePortalZ` inputs).
fn portal_z_inputs(world: &World, p: &LinePortal) -> ((f64, f64), (f64, f64)) {
    let at = |lid: LineId, end: usize| -> (f64, f64) {
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            return (0.0, 0.0);
        };
        let v = if end == 0 { line.v1 } else { line.v2 };
        let pos = world.map.vertices[v.raw() as usize].pos;
        let Some(si) = line.front_sector.map(|s| s.raw() as usize) else {
            return (0.0, 0.0);
        };
        (
            sector_plane_z_at(world, si, pos.x(), pos.y(), true),
            sector_plane_z_at(world, si, pos.x(), pos.y(), false),
        )
    };
    let (sf, sc) = at(p.line, 0);
    let (df, dc) = p.target.map_or((sf, sc), |t| at(t, 1));
    ((sf, df), (sc, dc))
}

/// A position on the origin side of a line portal, in destination
/// coordinates (XY rotation + Z alignment).
#[must_use]
pub fn translate_through(world: &World, p: &LinePortal, pos: DVec3) -> DVec3 {
    if p.is_linked() {
        return DVec3::new(
            pos.x() + p.displacement.0,
            pos.y() + p.displacement.1,
            pos.z(),
        );
    }
    let (x, y) = p.translate_xy(&world.map, pos.x(), pos.y());
    let (floors, ceilings) = portal_z_inputs(world, p);
    DVec3::new(x, y, p.translate_z(pos.z(), floors, ceilings))
}

/// After a move through a line portal (the position is already on the
/// destination side): turn velocity and facing, and record the move for
/// render interpolation (`P_TryMove`). Players turn their view with the
/// actor.
pub fn rotate_after_crossing(world: &mut World, id: ActorId, p: &LinePortal) {
    let Some(a) = world.actors.get(id).cloned() else {
        return;
    };
    let dangle = if p.is_linked() { 0.0 } else { p.angle_diff };
    if !p.is_linked() {
        let (vx, vy) = p.translate_vel(a.vel.x(), a.vel.y());
        if let Some(m) = world.actors.get_mut(id) {
            m.vel = DVec3::new(vx, vy, a.vel.z());
            m.angle = room_core::Angle::from_degrees(m.angle.degrees() + dangle).normalized();
        }
    }
    // The shells move the previous render position the same way; for a
    // rotating portal there is no straight path, so they snap instead.
    let offset = if p.is_linked() {
        [p.displacement.0, p.displacement.1, 0.0]
    } else {
        [f64::NAN; 3]
    };
    world.portal_moves.push(PortalMove {
        actor: id,
        offset,
        angle: dangle,
    });
}

/// `P_ClipLineToPortal`-style test: a line lies wholly behind a portal line
/// (on its back side, the empty space behind it) and does not count for
/// collision on this side.
#[must_use]
pub fn line_behind_portal(world: &World, line: LineId, portal_line: LineId) -> bool {
    if line == portal_line {
        return false;
    }
    let Some(l) = world.map.lines.get(line.raw() as usize) else {
        return false;
    };
    let a = world.map.vertices[l.v1.raw() as usize].pos;
    let b = world.map.vertices[l.v2.raw() as usize].pos;
    let side = |p: DVec2| -> i8 {
        let Some(pl) = world.map.lines.get(portal_line.raw() as usize) else {
            return 0;
        };
        let v1 = world.map.vertices[pl.v1.raw() as usize].pos;
        let d = pl.delta;
        let c = (p.y() - v1.y()) * d.x() - (p.x() - v1.x()) * d.y();
        if c > 1e-6 {
            1
        } else if c < -1e-6 {
            -1
        } else {
            0
        }
    };
    let (s1, s2) = (side(a), side(b));
    // Back = positive cross; touching the portal line counts as behind only
    // when the other end is behind too.
    (s1 >= 0 && s2 >= 0) && (s1 > 0 || s2 > 0)
}

/// Line portals whose line the actor's box touches at `pos`.
#[must_use]
pub fn touched_line_portals(world: &World, pos: DVec2, radius: f64) -> Vec<LinePortal> {
    world
        .map
        .setup
        .portals
        .lines
        .iter()
        .filter(|p| p.is_traversable() && p.target.is_some())
        .filter(|p| {
            let Some(l) = world.map.lines.get(p.line.raw() as usize) else {
                return false;
            };
            let a = world.map.vertices[l.v1.raw() as usize].pos;
            let b = world.map.vertices[l.v2.raw() as usize].pos;
            crate::physics::box_touches_segment(pos, radius, a, b)
        })
        .copied()
        .collect()
}

/// Group of the far side of a linked plane portal (`GetOppositePortalGroup`).
fn opposite_group(world: &World, sector: usize, plane: usize) -> Option<u32> {
    let sp = world.map.setup.portals.plane_portal(sector, plane)?;
    let dest = sp.destination?;
    Some(sector_group(world, dest.raw() as usize))
}

/// `BoxOnLineSide(box, line) == -1` with `inRange`: the square box of
/// `radius` around `c` straddles the segment `a → b`.
fn box_straddles(c: DVec2, radius: f64, a: DVec2, b: DVec2) -> bool {
    if c.x() + radius <= a.x().min(b.x())
        || c.x() - radius >= a.x().max(b.x())
        || c.y() + radius <= a.y().min(b.y())
        || c.y() - radius >= a.y().max(b.y())
    {
        return false;
    }
    let d = DVec2::new(b.x() - a.x(), b.y() - a.y());
    let side = |x: f64, y: f64| (y - a.y()) * d.x() - (x - a.x()) * d.y() > 0.0;
    let corners = [
        side(c.x() - radius, c.y() - radius),
        side(c.x() + radius, c.y() - radius),
        side(c.x() - radius, c.y() + radius),
        side(c.x() + radius, c.y() + radius),
    ];
    corners.iter().any(|&s| s) && corners.iter().any(|&s| !s)
}

/// `FLevelLocals::CollectConnectedGroups` (the `PGA_Full3d` form): every
/// portal group a box of `radius` at `pos`, from `pos.z` up to `upper_z`,
/// reaches through linked line and plane portals. `start` comes first.
///
/// Room keeps no per-group thing blockmap; callers use this to know which
/// actors of other groups a position query must see, and at which
/// displaced position (`pos_in_groups`).
#[must_use]
pub fn connected_groups(
    world: &World,
    start: u32,
    pos: DVec3,
    upper_z: f64,
    radius: f64,
) -> Vec<u32> {
    let groups = &world.map.setup.portal_groups;
    let mut out = vec![start];
    if !groups.has_links() {
        return out;
    }
    let mut seen = vec![false; groups.size];
    if let Some(s) = seen.get_mut(start as usize) {
        *s = true;
    }
    let line_pos = |lid: LineId| -> Option<(DVec2, DVec2, usize)> {
        let l = world.map.lines.get(lid.raw() as usize)?;
        let a = world.map.vertices.get(l.v1.raw() as usize)?.pos;
        let b = world.map.vertices.get(l.v2.raw() as usize)?.pos;
        Some((a, b, l.front_sector?.raw() as usize))
    };
    // Linked line portals the box touches, chased transitively.
    let mut found: Vec<(u32, u32)> = Vec::new();
    for p in &world.map.setup.portals.lines {
        if !p.is_linked() {
            continue;
        }
        let (Some(target), Some((a, b, fs))) = (p.target, line_pos(p.line)) else {
            continue;
        };
        let og = sector_group(world, fs);
        if !groups.connected(start, og) {
            continue;
        }
        let (dx, dy) = displacement(world, start, og);
        if !box_straddles(DVec2::new(pos.x() + dx, pos.y() + dy), radius, a, b) {
            continue;
        }
        let Some((_, _, ds)) = line_pos(target) else {
            continue;
        };
        found.push((og, sector_group(world, ds)));
    }
    let mut more = true;
    while more {
        more = false;
        found.retain(|&(from, to)| {
            let from_seen = seen.get(from as usize).copied().unwrap_or(false);
            let to_seen = seen.get(to as usize).copied().unwrap_or(true);
            if from_seen && !to_seen {
                seen[to as usize] = true;
                out.push(to);
                more = true;
                return false;
            }
            true
        });
    }
    if !has_linked_planes(world) {
        return out;
    }
    // Straight up and down through the planes over the point.
    let Some(sec) = sector_at(world, pos.x(), pos.y()) else {
        return out;
    };
    for plane in [PLANE_CEILING, PLANE_FLOOR] {
        let mut wsec = sec;
        for _ in 0..16 {
            if plane_blocks_movement(world, wsec, plane) {
                break;
            }
            let pz = plane_z(world, wsec, plane);
            let reaches = if plane == PLANE_CEILING {
                upper_z > pz
            } else {
                pos.z() < pz
            };
            if !reaches {
                break;
            }
            let Some(og) = opposite_group(world, wsec, plane) else {
                break;
            };
            if seen.get(og as usize).copied().unwrap_or(true) {
                break;
            }
            seen[og as usize] = true;
            out.push(og);
            let (dx, dy) = displacement(world, start, og);
            let Some(next) = sector_at(world, pos.x() + dx, pos.y() + dy) else {
                break;
            };
            wsec = next;
        }
    }
    // Planes of the sectors on lines the box straddles (the box hangs over
    // a neighbouring sector whose plane is a portal).
    let mut queue: Vec<(u32, u8)> = vec![(start, 0)];
    let mut qi = 0;
    while qi < queue.len() {
        let (g, dir) = queue[qi];
        qi += 1;
        let (dx, dy) = displacement(world, start, g);
        let c = DVec2::new(pos.x() + dx, pos.y() + dy);
        for lid in crate::physics::lines_near(&world.map, c, radius) {
            let Some(l) = world.map.lines.get(lid.raw() as usize) else {
                continue;
            };
            let (Some(a), Some(b)) = (
                world.map.vertices.get(l.v1.raw() as usize),
                world.map.vertices.get(l.v2.raw() as usize),
            ) else {
                continue;
            };
            if !box_straddles(c, radius, a.pos, b.pos) {
                continue;
            }
            for sid in [l.front_sector, l.back_sector].into_iter().flatten() {
                let si = sid.raw() as usize;
                // 1 = reached going up, 2 = going down: never turn back.
                if dir != 2
                    && !plane_blocks_movement(world, si, PLANE_CEILING)
                    && plane_z(world, si, PLANE_CEILING) < upper_z
                {
                    if let Some(og) = opposite_group(world, si, PLANE_CEILING) {
                        if !seen.get(og as usize).copied().unwrap_or(true) {
                            seen[og as usize] = true;
                            out.push(og);
                            queue.push((og, 1));
                        }
                    }
                }
                if dir != 1
                    && !plane_blocks_movement(world, si, PLANE_FLOOR)
                    && plane_z(world, si, PLANE_FLOOR) > pos.z()
                {
                    if let Some(og) = opposite_group(world, si, PLANE_FLOOR) {
                        if !seen.get(og as usize).copied().unwrap_or(true) {
                            seen[og as usize] = true;
                            out.push(og);
                            queue.push((og, 2));
                        }
                    }
                }
            }
        }
    }
    out
}

/// Position of `a` in `group`'s coordinates when `a` is in one of
/// `groups` (from `connected_groups`); `None` when it is elsewhere and a
/// query at that position cannot reach it.
#[must_use]
pub fn pos_in_groups(world: &World, a: &Actor, group: u32, groups: &[u32]) -> Option<DVec3> {
    let ag = actor_group(world, a);
    if ag == group {
        return Some(a.pos);
    }
    if !groups.contains(&ag) {
        return None;
    }
    let (dx, dy) = displacement(world, ag, group);
    Some(DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z()))
}

/// Where a position query (`query_groups`) sees `a`: its own position on
/// maps without linked portals, its displaced one when its group is
/// reached, `None` when it is not.
#[must_use]
pub fn query_pos(world: &World, q: &Option<(u32, Vec<u32>)>, a: &Actor) -> Option<DVec3> {
    match q {
        None => Some(a.pos),
        Some((g, gs)) => pos_in_groups(world, a, *g, gs),
    }
}

/// Groups and the query group for a position query of `radius` around
/// `pos` (`None` on maps without linked portals: use absolute positions).
#[must_use]
pub fn query_groups(
    world: &World,
    pos: DVec3,
    upper_z: f64,
    radius: f64,
) -> Option<(u32, Vec<u32>)> {
    if !world.map.setup.portal_groups.has_links() {
        return None;
    }
    let g = group_at(world, pos.x(), pos.y());
    Some((g, connected_groups(world, g, pos, upper_z, radius)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_hit_finds_the_crossing() {
        let f = segment_hit(
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(5.0, -1.0),
            DVec2::new(5.0, 1.0),
        );
        assert!((f.unwrap() - 0.5).abs() < 1e-9);
        assert!(
            segment_hit(
                DVec2::new(0.0, 0.0),
                DVec2::new(4.0, 0.0),
                DVec2::new(5.0, -1.0),
                DVec2::new(5.0, 1.0)
            )
            .is_none()
        );
    }
}
