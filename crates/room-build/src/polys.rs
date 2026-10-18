//! Moving Build sectors as polyobjects (`docs/23-build-engine.md` §Moving sectors).
//!
//! * Swinging doors (sector lotag 23 + `SE 11`) and solid rotating sectors
//!   (`SE 0` around an `SE 1` pivot) are slabs: the slab's walls become a
//!   polyobject turning around the effector / pivot, and the slab's floor is
//!   merged into the sector around it.
//! * Sliding doors (lotag 25 + `SE 15`) and star doors (lotag 9) draw their
//!   door as runs of one-sided walls (panels) inside a thin sector. Each run
//!   becomes a one-sided polyobject sliding into its pocket; the static
//!   sector keeps a one-sided wall where the panel met the pocket.
//!
//! Doors that Build operates together (same effector hitag) share one ACS
//! script so the door lines run `ACS_Execute`; rotators start from an
//! `OPEN` script.

use crate::convert::GameTiles;
use crate::map::{BuildMap, Sprite};
use std::collections::{BTreeMap, BTreeSet};

/// Build point.
pub(crate) type Pt = (i32, i32);
/// One ACS special call: (special, args).
pub(crate) type Call = (i32, [i32; 5]);
/// An ACS script: (number, calls).
pub(crate) type Script = (i32, Vec<Call>);

/// One line of a polyobject.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PolyWall {
    /// Map wall `w` (to its `point2`), sided like the static wall would be.
    Wall(usize),
    /// One-sided segment closing a panel, textured like wall `tex`.
    Closer { from: Pt, to: Pt, tex: usize },
}

/// A polyobject to emit.
#[derive(Debug, Clone)]
pub(crate) struct Poly {
    /// Poly id (1-based).
    pub id: i32,
    /// Sector its sides belong to.
    pub sector: usize,
    /// Lines in loop order; the first is the `Polyobj_StartLine`.
    pub walls: Vec<PolyWall>,
    /// Rotation centre / spawn spot.
    pub spawn: Pt,
    /// Script its lines run when used.
    pub script: Option<i32>,
}

/// Polyobject plan for one map.
#[derive(Debug, Clone, Default)]
pub(crate) struct Polys {
    /// Slab sector → sector its floor merges into.
    pub merged: BTreeMap<usize, usize>,
    /// Walls drawn by a panel poly instead of the static map.
    pub panel_walls: BTreeSet<usize>,
    /// Static one-sided walls where a panel left (from, to, texture wall).
    pub closers: Vec<(Pt, Pt, usize)>,
    /// Door sectors → script their bordering lines run.
    pub door_scripts: BTreeMap<usize, i32>,
    /// The polyobjects.
    pub polys: Vec<Poly>,
    /// ACS scripts (number, specials).
    pub scripts: Vec<Script>,
}

const SE_ROTATE: i16 = 0;
const SE_PIVOT: i16 = 1;
const SE_DOOR_AUTO_CLOSE: i16 = 10;
const SE_SWING_DOOR: i16 = 11;
const SE_SLIDE_DOOR: i16 = 15;
const LT_STAR_DOOR: i16 = 9;
const LT_SWING_DOOR: i16 = 23;
const LT_SLIDE_DOOR: i16 = 25;
const LT_ROTATE_RISE: i16 = 30;
const SE_SUBWAY: i16 = 6;
const SE_CAR: i16 = 14;
const SE_TRAIN: i16 = 30;

const POLYOBJ_ROTATE_LEFT: i32 = 2;
const POLYOBJ_ROTATE_RIGHT: i32 = 3;
const POLYOBJ_DOOR_SWING: i32 = 7;
const POLYOBJ_DOOR_SLIDE: i32 = 8;

/// Duke runs at 30 Hz, Room at 35: one Build unit (Room map unit) per Duke
/// tic is 7/8 of a map unit per Room tic.
const SLIDE_SPEED: i32 = 7;
/// Sector `extra` 256 → 32 Duke tics of sliding (`SE 15`).
const SLIDE_DIST: i32 = 32;
/// `SE 11` turns 64 Build angle units per Duke tic: 55/8 bytes per Room tic.
const SWING_SPEED: i32 = 55;
/// A quarter turn in Hexen byte angles.
const SWING_ANGLE: i32 = 64;
/// `SE 0` turns 32 Build angle units per Duke tic: 27/8 bytes per Room tic.
const ROTATE_SPEED: i32 = 27;
/// Auto-closing doors (`SE 10`) wait this long; others stay open.
const DOOR_WAIT: i32 = 150;
/// Player height in Room map units; lower slabs are solid.
const PLAYER_HEIGHT: i32 = 56;
/// Max step-up in Room map units.
const STEP: i32 = 24;

/// Walls of `s` from `wallptr` when they form one closed loop.
fn single_loop(map: &BuildMap, s: usize) -> Option<Vec<usize>> {
    let sec = map.sectors.get(s)?;
    let start = usize::try_from(sec.wallptr).ok()?;
    let count = usize::try_from(sec.wallnum).ok()?;
    let mut walls = Vec::with_capacity(count);
    let mut w = start;
    loop {
        walls.push(w);
        w = usize::try_from(map.walls.get(w)?.point2).ok()?;
        if w == start || walls.len() > count {
            break;
        }
    }
    (walls.len() == count && count >= 3).then_some(walls)
}

/// First closed loop of `s` (Build `wallptr`), even when the sector has holes.
fn first_loop(map: &BuildMap, s: usize) -> Option<Vec<usize>> {
    let sec = map.sectors.get(s)?;
    let start = usize::try_from(sec.wallptr).ok()?;
    let count = usize::try_from(sec.wallnum).ok()?;
    let mut walls = Vec::new();
    let mut w = start;
    loop {
        walls.push(w);
        w = usize::try_from(map.walls.get(w)?.point2).ok()?;
        if w == start || walls.len() >= count {
            break;
        }
    }
    (w == start && walls.len() >= 3).then_some(walls)
}

fn point(map: &BuildMap, w: usize) -> Pt {
    (map.walls[w].x, map.walls[w].y)
}

/// Build angle (0–2047, clockwise from east with y down) → Hexen byte angle.
fn byte_angle(build: i32) -> i32 {
    ((2048 - (build & 2047)) % 2048 + 4) / 8 % 256
}

/// Build direction vector → Hexen byte angle.
fn vector_byte_angle(dx: f64, dy: f64) -> i32 {
    let a = (-dy).atan2(dx);
    ((a * 128.0 / std::f64::consts::PI).round() as i32).rem_euclid(256)
}

/// Plan the map's polyobjects.
pub(crate) fn plan(
    map: &BuildMap,
    wsec: &[Option<usize>],
    game: &GameTiles,
    partner_of: &dyn Fn(usize) -> Option<usize>,
) -> Polys {
    let mut out = Polys::default();
    let effectors: Vec<&Sprite> = map
        .sprites
        .iter()
        .filter(|sp| i32::from(sp.picnum) == game.effector)
        .collect();
    let in_sector = |sp: &Sprite, s: usize| usize::try_from(sp.sectnum).ok() == Some(s);
    let effector_in = |s: usize, lotag: i16| {
        effectors
            .iter()
            .copied()
            .find(|sp| in_sector(sp, s) && sp.lotag == lotag)
    };
    let auto_close = |s: usize| effector_in(s, SE_DOOR_AUTO_CLOSE).is_some();
    // Door groups → specials of their script.
    let mut groups: BTreeMap<(i16, i32), Vec<Call>> = BTreeMap::new();
    let mut door_group: BTreeMap<usize, (i16, i32)> = BTreeMap::new();
    let mut open: Vec<(i32, [i32; 5])> = Vec::new();
    let mut next_id = 1;

    for s in 0..map.sectors.len() {
        let sec = &map.sectors[s];
        let Some(walls) = single_loop(map, s) else {
            continue;
        };
        match sec.lotag {
            LT_SWING_DOOR => {
                let Some(se) = effector_in(s, SE_SWING_DOOR) else {
                    continue;
                };
                let Some(host) = host_sector(&walls, wsec, partner_of) else {
                    continue;
                };
                let id = next_id;
                next_id += 1;
                let delay = if auto_close(s) { DOOR_WAIT } else { -1 };
                // Build turns clockwise first when the effector faces < 180°.
                let angle = if i32::from(se.ang) & 2047 > 1024 {
                    SWING_ANGLE
                } else {
                    -SWING_ANGLE
                };
                let key = (SE_SWING_DOOR, i32::from(se.hitag));
                groups
                    .entry(key)
                    .or_default()
                    .push((POLYOBJ_DOOR_SWING, [id, SWING_SPEED, angle, delay, 0]));
                door_group.insert(s, key);
                out.merged.insert(s, host);
                out.polys.push(Poly {
                    id,
                    sector: s,
                    walls: slab_walls(map, &walls),
                    spawn: (se.x, se.y),
                    script: None,
                });
            }
            LT_SLIDE_DOOR | LT_STAR_DOOR => {
                let se = effector_in(s, SE_SLIDE_DOOR);
                if sec.lotag == LT_SLIDE_DOOR && se.is_none() {
                    continue;
                }
                let runs = one_sided_runs(map, &walls);
                if runs.is_empty() {
                    continue;
                }
                let key = match se {
                    Some(se) if sec.lotag == LT_SLIDE_DOOR => (SE_SLIDE_DOOR, i32::from(se.hitag)),
                    _ => (LT_STAR_DOOR, -1 - i32::try_from(s).unwrap_or(0)),
                };
                let delay = if auto_close(s) { DOOR_WAIT } else { -1 };
                for run in runs {
                    let mut pts: Vec<Pt> = run.iter().map(|&w| point(map, w)).collect();
                    let last = *run.last().unwrap_or(&run[0]);
                    let end = usize::try_from(map.walls[last].point2)
                        .ok()
                        .filter(|&p| p < map.walls.len())
                        .map_or(pts[0], |p| point(map, p));
                    pts.push(end);
                    let (from, to) = (pts[0], end);
                    if from == to {
                        continue;
                    }
                    let (angle, dist) = match se {
                        Some(se) if sec.lotag == LT_SLIDE_DOOR => {
                            (byte_angle(i32::from(se.ang) + 1024), SLIDE_DIST)
                        }
                        _ => star_slide(&pts),
                    };
                    if dist <= 0 {
                        continue;
                    }
                    let id = next_id;
                    next_id += 1;
                    groups
                        .entry(key)
                        .or_default()
                        .push((POLYOBJ_DOOR_SLIDE, [id, SLIDE_SPEED, angle, dist, delay]));
                    out.panel_walls.extend(run.iter().copied());
                    out.closers.push((from, to, run[0]));
                    let mut pw = vec![PolyWall::Closer {
                        from: to,
                        to: from,
                        tex: run[0],
                    }];
                    pw.extend(run.iter().map(|&w| PolyWall::Wall(w)));
                    out.polys.push(Poly {
                        id,
                        sector: s,
                        walls: pw,
                        spawn: from,
                        script: None,
                    });
                }
                door_group.insert(s, key);
            }
            _ => {
                // Solid rotating sectors.
                let Some(se) = effector_in(s, SE_ROTATE) else {
                    continue;
                };
                if sec.lotag == LT_ROTATE_RISE {
                    continue;
                }
                let Some(pivot) = effectors
                    .iter()
                    .find(|p| p.lotag == SE_PIVOT && p.hitag == se.hitag)
                else {
                    continue;
                };
                let Some(host) = host_sector(&walls, wsec, partner_of) else {
                    continue;
                };
                let all_host = walls
                    .iter()
                    .all(|&w| partner_of(w).and_then(|j| wsec[j]) == Some(host));
                let h = &map.sectors[host];
                let raised = (h.floorz - sec.floorz) / 256;
                let gap = (sec.floorz - sec.ceilingz) / 256;
                if !all_host || (raised <= STEP && gap >= PLAYER_HEIGHT) {
                    continue;
                }
                let id = next_id;
                next_id += 1;
                let special = if i32::from(pivot.ang) & 2047 > 1024 {
                    POLYOBJ_ROTATE_LEFT
                } else {
                    POLYOBJ_ROTATE_RIGHT
                };
                open.push((special, [id, ROTATE_SPEED, 255, 0, 0]));
                out.merged.insert(s, host);
                out.polys.push(Poly {
                    id,
                    sector: s,
                    walls: slab_walls(map, &walls),
                    spawn: (pivot.x, pivot.y),
                    script: None,
                });
            }
        }
    }

    // Subway / train cars (`SE 6` / `14` / `30`). Lotag 31 is the usual
    // car, but E2L1 keeps the effector in a lotag-0 sector.
    for s in 0..map.sectors.len() {
        if out.polys.iter().any(|p| p.sector == s) {
            continue;
        }
        let Some(walls) = first_loop(map, s) else {
            continue;
        };
        let Some(se) = effectors
            .iter()
            .copied()
            .find(|sp| in_sector(sp, s) && matches!(sp.lotag, SE_SUBWAY | SE_CAR | SE_TRAIN))
        else {
            continue;
        };
        let Some(host) = host_sector(&walls, wsec, partner_of) else {
            continue;
        };
        let id = next_id;
        next_id += 1;
        out.merged.insert(s, host);
        out.polys.push(Poly {
            id,
            sector: s,
            walls: slab_walls(map, &walls),
            spawn: (se.x, se.y),
            script: None,
        });
    }

    let mut numbers: BTreeMap<(i16, i32), i32> = BTreeMap::new();
    for (k, (key, specials)) in groups.into_iter().enumerate() {
        let number = i32::try_from(k).unwrap_or(0) + 1;
        numbers.insert(key, number);
        out.scripts.push((number, specials));
    }
    for (s, key) in door_group {
        if let Some(&n) = numbers.get(&key) {
            out.door_scripts.insert(s, n);
        }
    }
    for p in &mut out.polys {
        p.script = out.door_scripts.get(&p.sector).copied();
    }
    if !open.is_empty() {
        out.scripts.push((1000 + 1, open));
    }
    out
}

/// Sector most of the loop's two-sided walls face.
fn host_sector(
    walls: &[usize],
    wsec: &[Option<usize>],
    partner_of: &dyn Fn(usize) -> Option<usize>,
) -> Option<usize> {
    let mut votes: BTreeMap<usize, usize> = BTreeMap::new();
    for &w in walls {
        if let Some(s) = partner_of(w).and_then(|j| wsec[j]) {
            *votes.entry(s).or_default() += 1;
        }
    }
    votes
        .into_iter()
        .max_by_key(|&(s, c)| (c, std::cmp::Reverse(s)))
        .map(|(s, _)| s)
}

/// Slab loop with the shortest wall (the start line, which cannot be used)
/// first.
fn slab_walls(map: &BuildMap, walls: &[usize]) -> Vec<PolyWall> {
    let len = |w: usize| {
        let p2 = usize::try_from(map.walls[w].point2).unwrap_or(w);
        let (a, b) = (point(map, w), point(map, p2.min(map.walls.len() - 1)));
        (i64::from(b.0 - a.0)).pow(2) + (i64::from(b.1 - a.1)).pow(2)
    };
    let k = (0..walls.len()).min_by_key(|&k| len(walls[k])).unwrap_or(0);
    walls[k..]
        .iter()
        .chain(&walls[..k])
        .map(|&w| PolyWall::Wall(w))
        .collect()
}

/// Maximal cyclic runs of at least two consecutive one-sided walls.
fn one_sided_runs(map: &BuildMap, walls: &[usize]) -> Vec<Vec<usize>> {
    let one = |w: usize| map.walls[w].nextwall < 0;
    let n = walls.len();
    let Some(start) = (0..n).find(|&k| !one(walls[k])) else {
        return Vec::new();
    };
    let mut runs = Vec::new();
    let mut cur = Vec::new();
    for k in 1..=n {
        let w = walls[(start + k) % n];
        if one(w) {
            cur.push(w);
        } else if !cur.is_empty() {
            runs.push(std::mem::take(&mut cur));
        }
    }
    runs.retain(|r| r.len() >= 2);
    runs
}

/// Star door panel: slides from its tip back toward its base by the length
/// of the wall after the tip's neighbour (Raze `handle_st09`).
fn star_slide(pts: &[Pt]) -> (i32, i32) {
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (bx, by) = (f64::from(b.0 - a.0), f64::from(b.1 - a.1));
    let base_len = bx.hypot(by).max(1.0);
    let depth =
        |p: Pt| ((f64::from(p.0 - a.0)) * by - (f64::from(p.1 - a.1)) * bx).abs() / base_len;
    let Some(t) = (1..pts.len() - 1).max_by(|&i, &j| depth(pts[i]).total_cmp(&depth(pts[j])))
    else {
        return (0, 0);
    };
    let tip = pts[t];
    let mid = (f64::from(a.0 + b.0) / 2.0, f64::from(a.1 + b.1) / 2.0);
    let (dx, dy) = (mid.0 - f64::from(tip.0), mid.1 - f64::from(tip.1));
    let dl = dx.hypot(dy).max(1.0);
    let travel = match (pts.get(t + 1), pts.get(t + 2)) {
        (Some(&p), Some(&q)) => {
            ((f64::from(q.0 - p.0)) * dx / dl + (f64::from(q.1 - p.1)) * dy / dl).abs()
        }
        _ => depth(tip),
    };
    let dist = (travel / crate::convert::XY_SCALE).round() as i32;
    (vector_byte_angle(dx, dy), dist)
}

/// ACS0 `BEHAVIOR` running each script's specials with `LSPEC5DIRECT`.
#[cfg(test)]
fn behavior(scripts: &[Script]) -> Vec<u8> {
    let scripts: Vec<crate::acs::AcsScript> = scripts
        .iter()
        .map(|(n, calls)| {
            (
                *n,
                calls
                    .iter()
                    .map(|&(sp, args)| crate::acs::Op::Special(sp, args))
                    .collect(),
            )
        })
        .collect();
    crate::acs::assemble(&scripts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_angles_become_byte_angles() {
        assert_eq!(byte_angle(0), 0);
        // Build 512 points down the screen (+y), Room south.
        assert_eq!(byte_angle(512), 192);
        assert_eq!(byte_angle(1024), 128);
        assert_eq!(vector_byte_angle(0.0, 1.0), 192);
        assert_eq!(vector_byte_angle(-1.0, 0.0), 128);
    }

    #[test]
    fn star_panel_slides_back_into_its_pocket() {
        // E2L3 sector 10's lower panel.
        let pts = [
            (7296, -22912),
            (7296, -22464),
            (7360, -22400),
            (7424, -22464),
            (7424, -22912),
        ];
        // Toward Build −y (Room +y = north, byte 64), 448 / 16 = 28 units.
        assert_eq!(star_slide(&pts), (64, 28));
    }

    #[test]
    fn behavior_lists_scripts() {
        let b = behavior(&[
            (1, vec![(8, [1, 7, 0, 32, -1])]),
            (1001, vec![(3, [2, 27, 255, 0, 0])]),
        ]);
        assert_eq!(&b[..4], b"ACS\0");
        let dir = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
        assert_eq!(dir, 8 + 4 * 16);
        assert_eq!(
            u32::from_le_bytes([b[dir], b[dir + 1], b[dir + 2], b[dir + 3]]),
            2
        );
    }
}
