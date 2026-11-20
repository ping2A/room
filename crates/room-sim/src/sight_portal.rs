//! Line of sight on maps with portals: UZDoom's `SightCheck`
//! (`src/playsim/p_sight.cpp`, GPL-3.0-or-later) without 3D floors.
//!
//! The trace collects the lines it crosses, sorted by fraction, and narrows
//! the top / bottom slopes at each opening. A line portal stops the trace
//! and queues a *task*: the same trace re-run in the destination's portal
//! group (both ends `PosRelative` to that group) from the portal's
//! fraction on. Lines next to a linked plane portal open to the sky / pit
//! on that side, and crossing into such a sector queues a task in the group
//! above or below. The check succeeds when one trace reaches the target in
//! the target's own group.

use crate::actor::Actor;
use crate::world::World;
use room_core::{DVec2, LineId};
use room_map::portals::{PLANE_CEILING, PLANE_FLOOR};

#[derive(Clone, Copy, Debug)]
struct Task {
    frac: f64,
    top: f64,
    bottom: f64,
    /// -1 none, 0 floor, 1 ceiling: the direction a plane portal took.
    dir: i8,
    group: u32,
}

const OPEN_MAX: f64 = f64::MAX;
const OPEN_MIN: f64 = f64::MIN;

/// Portal flags of an opening (`SO_*`).
const TOP_FRONT: u8 = 1;
const TOP_BACK: u8 = 2;
const BOTTOM_FRONT: u8 = 4;
const BOTTOM_BACK: u8 = 8;

struct Trace<'a> {
    world: &'a World,
    start: (f64, f64, f64),
    d: (f64, f64),
    top: f64,
    bottom: f64,
    dir: i8,
    group: u32,
    startfrac: f64,
}

fn swap_sides(f: u8) -> u8 {
    ((f & (TOP_FRONT | BOTTOM_FRONT)) << 1) | ((f & (TOP_BACK | BOTTOM_BACK)) >> 1)
}

/// The line borders a sector with a linked plane portal (`ML_PORTALCONNECT`).
fn portal_connect(world: &World, line: &room_map::Line) -> bool {
    let t = &world.map.setup.portals;
    [line.front_sector, line.back_sector]
        .into_iter()
        .flatten()
        .any(|s| {
            let si = s.raw() as usize;
            t.plane_is_linked(si, PLANE_FLOOR) || t.plane_is_linked(si, PLANE_CEILING)
        })
}

fn z_at(world: &World, si: usize, x: f64, y: f64, floor: bool) -> f64 {
    crate::physics::sector_z_at_pub(world, si, x, y, floor)
}

struct Opening {
    top: f64,
    bottom: f64,
    range: bool,
    flags: u8,
}

/// `SightCheck::P_SightOpening`.
fn opening(world: &World, line: &room_map::Line, x: f64, y: f64) -> Opening {
    let mut o = Opening {
        top: 0.0,
        bottom: 0.0,
        range: false,
        flags: 0,
    };
    let Some(front) = line.front_sector.map(|s| s.raw() as usize) else {
        return o;
    };
    let connect = portal_connect(world, line);
    let open_up = |s: usize| !crate::portal::plane_blocks_sight(world, s, PLANE_CEILING);
    let open_down = |s: usize| !crate::portal::plane_blocks_sight(world, s, PLANE_FLOOR);
    let Some(back) = line.back_sector.map(|s| s.raw() as usize) else {
        if connect {
            let mut top = None;
            let mut bottom = None;
            if open_up(front) {
                top = Some(OPEN_MAX);
                o.flags |= TOP_FRONT;
            }
            if open_down(front) {
                bottom = Some(OPEN_MIN);
                o.flags |= BOTTOM_FRONT;
            }
            o.top = top.unwrap_or_else(|| z_at(world, front, x, y, true));
            o.bottom = bottom.unwrap_or_else(|| z_at(world, front, x, y, false));
        }
        return o;
    };
    let (mut fc, mut bc, mut ff, mut bf) = (None, None, None, None);
    if connect {
        if open_up(front) {
            fc = Some(OPEN_MAX);
            o.flags |= TOP_FRONT;
        }
        if open_up(back) {
            bc = Some(OPEN_MAX);
            o.flags |= TOP_BACK;
        }
        if open_down(front) {
            ff = Some(OPEN_MIN);
            o.flags |= BOTTOM_FRONT;
        }
        if open_down(back) {
            bf = Some(OPEN_MIN);
            o.flags |= BOTTOM_BACK;
        }
    }
    let fc = fc.unwrap_or_else(|| z_at(world, front, x, y, false));
    let bc = bc.unwrap_or_else(|| z_at(world, back, x, y, false));
    let ff = ff.unwrap_or_else(|| z_at(world, front, x, y, true));
    let bf = bf.unwrap_or_else(|| z_at(world, back, x, y, true));
    o.bottom = ff.max(bf);
    o.top = fc.min(bc);
    o.range = o.bottom < o.top;
    o
}

fn line_blocks_sight(world: &World, lid: LineId, line: &room_map::Line) -> bool {
    line.block_sight || (world.line_flags(lid) & crate::world::ML_BLOCKEVERYTHING) != 0
}

/// Lines crossing a segment, with their fraction along the full trace.
fn intercepts(
    world: &World,
    tr: &Trace<'_>,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
) -> Vec<(f64, LineId)> {
    let mut out: Vec<(f64, LineId)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let a = DVec2::new(tr.start.0, tr.start.1);
    let b = DVec2::new(tr.start.0 + tr.d.0, tr.start.1 + tr.d.1);
    for lid in lines_along(world, x1, y1, x2, y2) {
        if !seen.insert(lid) {
            continue;
        }
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        let v1 = world.map.vertices[line.v1.raw() as usize].pos;
        let v2 = world.map.vertices[line.v2.raw() as usize].pos;
        let Some(frac) = crate::portal::segment_hit(a, b, v1, v2) else {
            continue;
        };
        if frac < tr.startfrac {
            continue;
        }
        out.push((frac, lid));
    }
    out.sort_by(|p, q| p.0.total_cmp(&q.0));
    out
}

/// Blockmap lines along a segment (every cell its bounding box covers that
/// the segment passes through; out-of-map stretches are skipped).
fn lines_along(world: &World, x1: f64, y1: f64, x2: f64, y2: f64) -> Vec<LineId> {
    let bm = &world.map.blockmap;
    if bm.columns == 0 || bm.rows == 0 || bm.blocks.is_empty() {
        return (0..world.map.lines.len())
            .map(|i| LineId::from_raw(i as u32))
            .collect();
    }
    let (ox, oy) = (f64::from(bm.origin_x), f64::from(bm.origin_y));
    let cell = |v: f64, o: f64| ((v - o) / 128.0).floor() as i64;
    let (cx0, cy0, cx1, cy1) = (cell(x1, ox), cell(y1, oy), cell(x2, ox), cell(y2, oy));
    let (lo_x, hi_x) = (cx0.min(cx1), cx0.max(cx1));
    let (lo_y, hi_y) = (cy0.min(cy1), cy0.max(cy1));
    let mut out = Vec::new();
    let a = DVec2::new(x1, y1);
    let b = DVec2::new(x2, y2);
    for cy in lo_y.max(0)..=hi_y.min(i64::from(bm.rows) - 1) {
        for cx in lo_x.max(0)..=hi_x.min(i64::from(bm.columns) - 1) {
            // Skip cells the segment does not touch (its distance to the
            // cell centre is over the half diagonal).
            let c = DVec2::new(
                ox + (cx as f64 + 0.5) * 128.0,
                oy + (cy as f64 + 0.5) * 128.0,
            );
            if seg_point_dist(a, b, c) > 91.0 {
                continue;
            }
            let idx = cy as usize * usize::from(bm.columns) + cx as usize;
            if let Some(cell) = bm.blocks.get(idx) {
                out.extend(cell.iter().copied());
            }
        }
    }
    out
}

fn seg_point_dist(a: DVec2, b: DVec2, p: DVec2) -> f64 {
    let (dx, dy) = (b.x() - a.x(), b.y() - a.y());
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 1e-12 {
        0.0
    } else {
        (((p.x() - a.x()) * dx + (p.y() - a.y()) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.x() + dx * t, a.y() + dy * t);
    ((p.x() - cx).powi(2) + (p.y() - cy).powi(2)).sqrt()
}

/// 0 front, 1 back (`P_PointOnLineSidePrecise`).
fn side_of(world: &World, line: &room_map::Line, x: f64, y: f64) -> u8 {
    let v1 = world.map.vertices[line.v1.raw() as usize].pos;
    let d = line.delta;
    u8::from((y - v1.y()) * d.x() - (x - v1.x()) * d.y() > 1e-9)
}

impl Trace<'_> {
    /// `PTR_SightTraverse` for one line. `false` stops the trace.
    fn line(&mut self, frac: f64, lid: LineId, tasks: &mut Vec<Task>) -> bool {
        let world = self.world;
        let Some(li) = world.map.lines.get(lid.raw() as usize) else {
            return true;
        };
        let x = self.start.0 + self.d.0 * frac;
        let y = self.start.1 + self.d.1 * frac;
        let mut open = opening(world, li, x, y);
        if line_blocks_sight(world, lid, li) {
            open.range = false;
            if open.bottom != OPEN_MIN {
                open.bottom = open.top;
            }
        }
        let lport = world.map.setup.portals.line(lid).copied();
        let linked = lport.is_some_and(|p| p.is_linked());
        if !open.range && open.flags == 0 && !linked {
            return false;
        }
        if frac == 0.0 {
            return true;
        }
        if open.bottom > OPEN_MIN {
            let slope = (open.bottom - self.start.2) / frac;
            if slope > self.bottom {
                self.bottom = slope;
            }
        }
        if open.top < OPEN_MAX {
            let slope = (open.top - self.start.2) / frac;
            if slope < self.top {
                self.top = slope;
            }
        }
        if open.flags != 0 {
            let front_flag = side_of(world, li, self.start.0, self.start.1);
            let (back, flags) = if front_flag == 0 {
                (li.back_sector, open.flags)
            } else {
                if li.back_sector.is_none() {
                    return false;
                }
                (li.front_sector, swap_sides(open.flags))
            };
            if let Some(back) = back.map(|s| s.raw() as usize) {
                let t = &world.map.setup.portals;
                if self.dir != PLANE_FLOOR as i8 && flags & TOP_BACK != 0 && flags & TOP_FRONT == 0
                {
                    tasks.push(Task {
                        frac,
                        top: self.top,
                        bottom: self.bottom,
                        dir: PLANE_CEILING as i8,
                        group: opposite_group(world, t, back, PLANE_CEILING),
                    });
                }
                if self.dir != PLANE_CEILING as i8
                    && flags & BOTTOM_BACK != 0
                    && flags & BOTTOM_FRONT == 0
                {
                    tasks.push(Task {
                        frac,
                        top: self.top,
                        bottom: self.bottom,
                        dir: PLANE_FLOOR as i8,
                        group: opposite_group(world, t, back, PLANE_FLOOR),
                    });
                }
            }
        }
        if let Some(p) = lport
            && let Some(dst) = p.target
        {
            let g = world
                .map
                .lines
                .get(dst.raw() as usize)
                .and_then(|l| l.front_sector)
                .map_or(0, |s| crate::portal::sector_group(world, s.raw() as usize));
            tasks.push(Task {
                frac,
                top: self.top,
                bottom: self.bottom,
                dir: self.dir,
                group: g,
            });
            return false;
        }
        !(self.top <= self.bottom || !open.range)
    }
}

fn opposite_group(world: &World, t: &room_map::PortalTables, sector: usize, plane: usize) -> u32 {
    t.plane_portal(sector, plane)
        .and_then(|p| p.destination)
        .map_or(0, |s| crate::portal::sector_group(world, s.raw() as usize))
}

/// `SightCheck::P_SightPathTraverse` for one task.
fn traverse(world: &World, t1: &Actor, t2: &Actor, task: Task, tasks: &mut Vec<Task>) -> bool {
    let s = crate::portal::pos_relative(world, t1, task.group);
    let e = crate::portal::pos_relative(world, t2, task.group);
    let mut tr = Trace {
        world,
        start: (s.x(), s.y(), s.z() + t1.height * 0.75),
        d: (e.x() - s.x(), e.y() - s.y()),
        top: task.top,
        bottom: task.bottom,
        dir: task.dir,
        group: task.group,
        startfrac: task.frac,
    };
    let x1 = tr.start.0 + task.frac * tr.d.0;
    let y1 = tr.start.1 + task.frac * tr.d.1;
    if let Some(sec) = crate::portal::sector_at(world, x1, y1) {
        if tr.dir != PLANE_FLOOR as i8
            && !crate::portal::plane_blocks_sight(world, sec, PLANE_CEILING)
        {
            let g = opposite_group(world, &world.map.setup.portals, sec, PLANE_CEILING);
            tasks.push(Task {
                frac: 0.0,
                top: tr.top,
                bottom: tr.bottom,
                dir: PLANE_CEILING as i8,
                group: g,
            });
        }
        if tr.dir != PLANE_CEILING as i8
            && !crate::portal::plane_blocks_sight(world, sec, PLANE_FLOOR)
        {
            let g = opposite_group(world, &world.map.setup.portals, sec, PLANE_FLOOR);
            tasks.push(Task {
                frac: 0.0,
                top: tr.top,
                bottom: tr.bottom,
                dir: PLANE_FLOOR as i8,
                group: g,
            });
        }
    }
    let list = intercepts(world, &tr, x1, y1, e.x(), e.y());
    for (frac, lid) in list {
        if !tr.line(frac, lid, tasks) {
            return false;
        }
    }
    crate::portal::actor_group(world, t2) == tr.group
}

/// `P_CheckSight` on a portal map.
#[must_use]
pub fn check_sight_portals(world: &World, t1: &Actor, t2: &Actor) -> bool {
    let look = t1.pos.z() + t1.height * 0.75;
    // `GetPortalTransition`: an eye above a linked ceiling portal looks from
    // the group above.
    let mut group = crate::portal::actor_group(world, t1);
    if let Some(sec) = crate::portal::sector_at(world, t1.pos.x(), t1.pos.y())
        && !crate::portal::plane_blocks_movement(world, sec, PLANE_CEILING)
        && look >= world.map.setup.portals.plane_z(sec, PLANE_CEILING)
    {
        group = opposite_group(world, &world.map.setup.portals, sec, PLANE_CEILING);
    }
    let bottom = t2.pos.z() - look;
    let top = bottom + t2.height;
    let mut tasks: Vec<Task> = Vec::new();
    let first = Task {
        frac: 0.0,
        top,
        bottom,
        dir: -1,
        group,
    };
    if traverse(world, t1, t2, first, &mut tasks) {
        return true;
    }
    let v = crate::portal::vec2_to(world, t1, t2);
    let dist = (v.x() * v.x() + v.y() * v.y()).sqrt().max(1.0);
    let mut i = 0;
    let mut tried: Vec<(u32, i8, u64)> = Vec::new();
    while i < tasks.len() && i < 64 {
        let mut task = tasks[i];
        i += 1;
        task.frac += 1.0 / dist;
        let key = (task.group, task.dir, task.frac.to_bits());
        if tried.contains(&key) {
            continue;
        }
        tried.push(key);
        if traverse(world, t1, t2, task, &mut tasks) {
            return true;
        }
    }
    false
}
