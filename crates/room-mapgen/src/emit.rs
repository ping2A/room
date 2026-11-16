//! Grid → Doom map: one line per boundary between different areas, merged
//! along straight runs, front sides facing into sectors (teleporter pads
//! from outside), textures from each side's own sector, sidedef offsets
//! continuous along walls.

use crate::grid::{Area, CELL, Diag, Grid, Side};
use room_map::raw::{RawLinedef, RawSector, RawSidedef, RawThing, RawVertex};
use room_map::{MapFormat, RawMap};
use std::collections::HashMap;

/// A unit boundary piece in cell-corner coordinates.
#[derive(Clone, Copy, Debug)]
struct Seg {
    p1: (i32, i32),
    p2: (i32, i32),
    front: Area,
    back: Area,
}

impl Seg {
    fn dir(&self) -> (i32, i32) {
        (self.p2.0 - self.p1.0, self.p2.1 - self.p1.1)
    }
}

/// Order two sectors: the front side is the one with the lower rank
/// (pads always on the back, so walking in from the room crosses
/// front-to-back and triggers the teleport).
fn rank(grid: &Grid, a: Area) -> (u8, u32) {
    match a {
        Area::Solid(_) => (2, 0),
        Area::Floor(s) => {
            let back = grid
                .sectors
                .get(s as usize)
                .is_some_and(|p| p.teleport_to != 0 || p.door);
            (u8::from(back), s)
        }
    }
}

/// Orient a boundary: `right` is the area on the right of `p1 → p2`.
fn push(grid: &Grid, out: &mut Vec<Seg>, p1: (i32, i32), p2: (i32, i32), right: Area, left: Area) {
    if right == left {
        return;
    }
    if matches!(right, Area::Solid(_)) && matches!(left, Area::Solid(_)) {
        return;
    }
    if rank(grid, right) <= rank(grid, left) {
        out.push(Seg {
            p1,
            p2,
            front: right,
            back: left,
        });
    } else {
        out.push(Seg {
            p1: p2,
            p2: p1,
            front: left,
            back: right,
        });
    }
}

fn segments(grid: &Grid) -> Vec<Seg> {
    let mut out = Vec::new();
    // Vertical edges: x = gx, from (gx, y) north to (gx, y+1). Right = east.
    for y in 0..grid.h {
        for gx in 0..=grid.w {
            let west = grid.get(gx - 1, y).edge(Side::E);
            let east = grid.get(gx, y).edge(Side::W);
            push(grid, &mut out, (gx, y), (gx, y + 1), east, west);
        }
    }
    // Horizontal edges: y = gy, from (x, gy) east to (x+1, gy). Right = south.
    for gy in 0..=grid.h {
        for x in 0..grid.w {
            let south = grid.get(x, gy - 1).edge(Side::N);
            let north = grid.get(x, gy).edge(Side::S);
            push(grid, &mut out, (x, gy), (x + 1, gy), south, north);
        }
    }
    // Diagonals inside split cells: right side is half `a`.
    for y in 0..grid.h {
        for x in 0..grid.w {
            let c = grid.get(x, y);
            if let Some((d, b)) = c.split {
                match d {
                    Diag::Slash => push(grid, &mut out, (x, y), (x + 1, y + 1), c.a, b),
                    Diag::Backslash => push(grid, &mut out, (x, y + 1), (x + 1, y), c.a, b),
                }
            }
        }
    }
    out
}

/// Merge collinear runs through corners where nothing else meets.
fn merge(segs: &[Seg]) -> Vec<Seg> {
    let mut degree: HashMap<(i32, i32), u32> = HashMap::new();
    let mut by_start: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    let mut by_end: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        *degree.entry(s.p1).or_default() += 1;
        *degree.entry(s.p2).or_default() += 1;
        by_start.entry(s.p1).or_default().push(i);
        by_end.entry(s.p2).or_default().push(i);
    }
    let same = |a: &Seg, b: &Seg| a.dir() == b.dir() && a.front == b.front && a.back == b.back;
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for i in 0..segs.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut run = segs[i];
        // Backwards.
        loop {
            if degree.get(&run.p1).copied() != Some(2) {
                break;
            }
            let Some(&j) = by_end
                .get(&run.p1)
                .and_then(|v| v.iter().find(|&&j| !used[j] && same(&segs[j], &run)))
            else {
                break;
            };
            used[j] = true;
            run.p1 = segs[j].p1;
        }
        // Forwards.
        loop {
            if degree.get(&run.p2).copied() != Some(2) {
                break;
            }
            let Some(&j) = by_start
                .get(&run.p2)
                .and_then(|v| v.iter().find(|&&j| !used[j] && same(&segs[j], &run)))
            else {
                break;
            };
            used[j] = true;
            run.p2 = segs[j].p2;
        }
        out.push(run);
    }
    out
}

/// Build the map document.
pub fn emit(grid: &Grid, name: &str, things: Vec<RawThing>) -> RawMap {
    let segs = merge(&segments(grid));
    let ox = grid.w * CELL / 2;
    let oy = grid.h * CELL / 2;
    let world = |p: (i32, i32)| (p.0 * CELL - ox, p.1 * CELL - oy);

    // Only sectors that are used, renumbered.
    let mut sector_map: HashMap<u32, u16> = HashMap::new();
    let mut sectors: Vec<RawSector> = Vec::new();
    let mut sec_of = |s: u32, sectors: &mut Vec<RawSector>| -> u16 {
        *sector_map.entry(s).or_insert_with(|| {
            let p = &grid.sectors[s as usize];
            sectors.push(RawSector {
                floor_height: p.floor,
                ceiling_height: p.ceil,
                floor_pic: p.floor_pic.clone(),
                ceiling_pic: p.ceil_pic.clone(),
                light: p.light,
                special: p.special,
                tag: p.tag,
                ..RawSector::default()
            });
            (sectors.len() - 1) as u16
        })
    };
    // Sectors in a stable order (by first appearance on the grid).
    for y in 0..grid.h {
        for x in 0..grid.w {
            let c = grid.get(x, y);
            for a in [Some(c.a), c.split.map(|s| s.1)].into_iter().flatten() {
                if let Area::Floor(s) = a {
                    sec_of(s, &mut sectors);
                }
            }
        }
    }

    let mut vertices: Vec<RawVertex> = Vec::new();
    let mut vidx: HashMap<(i32, i32), u16> = HashMap::new();
    let mut vertex = |p: (i32, i32), vertices: &mut Vec<RawVertex>| -> u16 {
        *vidx.entry(p).or_insert_with(|| {
            let (x, y) = world(p);
            vertices.push(RawVertex {
                x: x as i16,
                y: y as i16,
            });
            (vertices.len() - 1) as u16
        })
    };
    let mut sidedefs: Vec<RawSidedef> = Vec::new();
    let mut linedefs: Vec<RawLinedef> = Vec::new();
    for s in &segs {
        let v1 = vertex(s.p1, &mut vertices);
        let v2 = vertex(s.p2, &mut vertices);
        let (wx, wy) = world(s.p1);
        let (dx, dy) = s.dir();
        // Texture offset continuous along the wall's axis.
        let along = if dx != 0 {
            wx * dx.signum()
        } else {
            wy * dy.signum()
        };
        let x_offset = along.rem_euclid(256) as i16;
        let Area::Floor(fs) = s.front else { continue };
        let fp = &grid.sectors[fs as usize];
        let front_sector = sec_of(fs, &mut sectors);
        match s.back {
            Area::Solid(m) => {
                let tex = if m == 0 {
                    fp.wall.clone()
                } else {
                    grid.mats[m as usize].tex.clone()
                };
                let special = if m == 0 {
                    0
                } else {
                    grid.mats[m as usize].special
                };
                sidedefs.push(RawSidedef {
                    x_offset,
                    middle: tex,
                    sector: front_sector,
                    ..RawSidedef::default()
                });
                linedefs.push(RawLinedef {
                    v1,
                    v2,
                    flags: 1,
                    special,
                    side_front: (sidedefs.len() - 1) as u16,
                    ..RawLinedef::default()
                });
            }
            Area::Floor(bs) => {
                let bp = &grid.sectors[bs as usize];
                let back_sector = sec_of(bs, &mut sectors);
                let side = |own: &crate::grid::SecProps,
                            other: &crate::grid::SecProps,
                            sector: u16,
                            x_offset: i16| {
                    let both_sky = own.ceil_pic == "F_SKY1" && other.ceil_pic == "F_SKY1";
                    RawSidedef {
                        x_offset,
                        upper: if other.ceil < own.ceil && !both_sky {
                            own.wall.clone()
                        } else {
                            "-".into()
                        },
                        lower: if other.floor > own.floor {
                            other.skirt.clone()
                        } else {
                            "-".into()
                        },
                        middle: "-".into(),
                        sector,
                        ..RawSidedef::default()
                    }
                };
                sidedefs.push(side(fp, bp, front_sector, x_offset));
                let front = (sidedefs.len() - 1) as u16;
                sidedefs.push(side(bp, fp, back_sector, x_offset));
                let back = (sidedefs.len() - 1) as u16;
                // Walking onto a pad (front = outside, back = pad).
                // Secret door: D1 open-stay, drawn as a wall on the automap.
                let (special, tag) = if bp.teleport_to != 0 && fp.teleport_to == 0 {
                    (97, bp.teleport_to)
                } else if bp.door && !fp.door {
                    (31, 0)
                } else {
                    (0, 0)
                };
                let secret = if special == 31 { 32 } else { 0 };
                linedefs.push(RawLinedef {
                    v1,
                    v2,
                    flags: 4 | 16 | secret,
                    special,
                    tag,
                    side_front: front,
                    side_back: back,
                    ..RawLinedef::default()
                });
            }
        }
    }
    let things = things
        .into_iter()
        .map(|mut t| {
            t.x = (i32::from(t.x) - ox) as i16;
            t.y = (i32::from(t.y) - oy) as i16;
            t
        })
        .collect();
    RawMap {
        name: name.to_ascii_uppercase(),
        format: MapFormat::Doom,
        vertices,
        linedefs,
        sidedefs,
        sectors,
        things,
        behavior: None,
        extra_lumps: Vec::new(),
    }
}
