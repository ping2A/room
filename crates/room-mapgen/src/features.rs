//! Room dressing: symmetric set pieces that give each room an identity and
//! cover (daises, sunken pits and pools, pillar rings, colonnades, crates,
//! central blocks), ceiling overlays (skylights, light panels), outdoor
//! terrain (hills, boulders, ponds), teleporter pads and the exit switch.

use crate::Options;
use crate::grid::{Area, CELL, Grid, Mat, Owner, SecProps};
use crate::layout::{Layout, Room};
use crate::rng::Rng;
use crate::theme::Palette;

/// Teleporter pad (2×2 cells from `(x, y)`).
#[derive(Clone, Copy, Debug)]
pub struct Pad {
    /// Room holding the pad.
    pub room: usize,
    /// Lower-left cell.
    pub x: i32,
    /// Lower-left cell.
    pub y: i32,
    /// Destination thing angle (faces the room centre).
    pub angle: i16,
}

/// Room interior: whole floor cells of `ri` at least two cells from its
/// walls and four from its doors.
fn interior(grid: &Grid, room: &Room, ri: usize, x: i32, y: i32) -> bool {
    for dy in -2..=2 {
        for dx in -2..=2 {
            let c = grid.get(x + dx, y + dy);
            if c.owner != Owner::Room(ri as u16) || c.full_floor().is_none() {
                return false;
            }
        }
    }
    room.doors
        .iter()
        .all(|&(dx, dy)| (dx - x).abs().max((dy - y).abs()) >= 4)
}

/// Mirror a local rectangle `(i, j, fw, fh)` 1, 2 or 4 ways.
fn mirrored(
    room: &Room,
    i: i32,
    j: i32,
    fw: i32,
    fh: i32,
    mx: bool,
    my: bool,
) -> Vec<(i32, i32, i32, i32)> {
    let mut out = vec![(i, j, fw, fh)];
    if mx {
        out.push((room.w - i - fw, j, fw, fh));
    }
    if my {
        out.push((i, room.h - j - fh, fw, fh));
    }
    if mx && my {
        out.push((room.w - i - fw, room.h - j - fh, fw, fh));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// All cells of local rects, in grid coordinates; `None` if any is not
/// free interior.
fn cells_if_free(
    grid: &Grid,
    room: &Room,
    ri: usize,
    rects: &[(i32, i32, i32, i32)],
    margin: i32,
) -> Option<Vec<(i32, i32)>> {
    let mut out = Vec::new();
    for &(i, j, fw, fh) in rects {
        if fw <= 0 || fh <= 0 {
            return None;
        }
        for y in room.y0 + j - margin..room.y0 + j + fh + margin {
            for x in room.x0 + i - margin..room.x0 + i + fw + margin {
                if !interior(grid, room, ri, x, y) || grid.get(x, y).busy {
                    return None;
                }
            }
        }
        for y in room.y0 + j..room.y0 + j + fh {
            for x in room.x0 + i..room.x0 + i + fw {
                out.push((x, y));
            }
        }
    }
    Some(out)
}

/// Centred `(i, fw)` along an axis of length `len` with `fw` of the same
/// parity (true symmetry).
fn centred(len: i32, want: i32) -> (i32, i32) {
    let mut fw = want.max(2);
    if (len - fw) % 2 != 0 {
        fw += 1;
    }
    ((len - fw) / 2, fw)
}

fn dais(grid: &mut Grid, room: &mut Room, ri: usize, pal: &Palette, opts: &Options) -> bool {
    let (i, fw) = centred(room.w, room.w / 3);
    let (j, fh) = centred(room.h, room.h / 3);
    let Some(cells) = cells_if_free(grid, room, ri, &[(i, j, fw, fh)], 0) else {
        return false;
    };
    for &(x, y) in &cells {
        grid.restyle(x, y, |p| {
            p.floor += 24;
            p.floor_pic.clone_from(&pal.floor_alt);
            p.skirt.clone_from(&pal.skirt);
            p.light = (p.light + 16).min(255);
        });
    }
    let tier2 = opts.verticality >= 1 && fw >= 6 && fh >= 6;
    if tier2 {
        for y in room.y0 + j + 2..room.y0 + j + fh - 2 {
            for x in room.x0 + i + 2..room.x0 + i + fw - 2 {
                grid.restyle(x, y, |p| p.floor += 24);
            }
        }
    }
    room.power
        .push((room.x0 + i + fw / 2, room.y0 + j + fh / 2));
    room.features
        .push(if tier2 { "two-tier dais" } else { "dais" });
    true
}

fn pit(
    grid: &mut Grid,
    room: &mut Room,
    ri: usize,
    pal: &Palette,
    opts: &Options,
    rng: &mut Rng,
) -> bool {
    let (i, fw) = centred(room.w, room.w / 3 + 1);
    let (j, fh) = centred(room.h, room.h / 3 + 1);
    let Some(cells) = cells_if_free(grid, room, ri, &[(i, j, fw, fh)], 0) else {
        return false;
    };
    let liquid = opts.liquids && rng.chance(0.6);
    for &(x, y) in &cells {
        grid.restyle(x, y, |p| {
            p.floor -= 24;
            if liquid {
                p.floor_pic.clone_from(&pal.liquid);
                p.special = pal.liquid_damage;
                p.light = (p.light + 24).min(255);
            }
        });
    }
    room.features
        .push(if liquid { "pool" } else { "sunken pit" });
    true
}

fn pillars(grid: &mut Grid, room: &mut Room, ri: usize, mat: u16) -> bool {
    let s = if room.w.min(room.h) >= 14 { 2 } else { 1 };
    let rects = mirrored(
        room,
        room.w / 4 - s / 2,
        room.h / 4 - s / 2,
        s,
        s,
        true,
        true,
    );
    let Some(cells) = cells_if_free(grid, room, ri, &rects, 1) else {
        return false;
    };
    for (x, y) in cells {
        grid.wall(x, y, mat);
    }
    room.features.push("pillars");
    true
}

fn colonnade(grid: &mut Grid, room: &mut Room, ri: usize, mat: u16) -> bool {
    let horizontal = room.w >= room.h;
    let (len, short) = if horizontal {
        (room.w, room.h)
    } else {
        (room.h, room.w)
    };
    if len < 2 * short - 2 {
        return false;
    }
    let row = 2;
    let mut rects = Vec::new();
    let mut t = 4;
    while t + 1 < len - 4 {
        let (i, j) = if horizontal { (t, row) } else { (row, t) };
        rects.extend(mirrored(room, i, j, 1, 1, !horizontal, horizontal));
        t += 4;
    }
    let Some(cells) = cells_if_free(grid, room, ri, &rects, 0) else {
        return false;
    };
    for (x, y) in cells {
        grid.wall(x, y, mat);
    }
    room.features.push("colonnade");
    true
}

fn crates(
    grid: &mut Grid,
    room: &mut Room,
    ri: usize,
    pal: &Palette,
    opts: &Options,
    rng: &mut Rng,
) -> bool {
    let want = 1 + opts.cover as usize + usize::from(room.area() > 200);
    let mut placed = 0;
    for _ in 0..30 {
        if placed >= want {
            break;
        }
        let (cw, chh) = if rng.chance(0.5) {
            (2, 2)
        } else if rng.chance(0.5) {
            (2, 1)
        } else {
            (1, 2)
        };
        let i = rng.range(2, (room.w / 2 - cw).max(2));
        let j = rng.range(2, (room.h / 2 - chh).max(2));
        let rects = mirrored(room, i, j, cw, chh, true, rng.chance(0.5));
        let Some(cells) = cells_if_free(grid, room, ri, &rects, 1) else {
            continue;
        };
        let height = if rng.chance(0.5) { 64 } else { 48 };
        for (x, y) in cells {
            grid.restyle(x, y, |p| {
                p.floor += height;
                p.floor_pic.clone_from(&pal.crate_top);
                p.skirt.clone_from(&pal.crate_wall);
            });
            if let Some(c) = grid.at(x, y) {
                c.busy = true;
            }
        }
        placed += 1;
    }
    if placed > 0 {
        room.features.push("crates");
    }
    placed > 0
}

fn ring(grid: &mut Grid, room: &mut Room, ri: usize, mat: u16) -> bool {
    let (i, fw) = centred(room.w, room.w / 4);
    let (j, fh) = centred(room.h, room.h / 4);
    let Some(cells) = cells_if_free(grid, room, ri, &[(i, j, fw, fh)], 1) else {
        return false;
    };
    for (x, y) in cells {
        grid.wall(x, y, mat);
    }
    room.features.push("central block");
    true
}

fn skylight(grid: &mut Grid, room: &mut Room, ri: usize) {
    let (i, fw) = centred(room.w, room.w / 2);
    let (j, fh) = centred(room.h, room.h / 2);
    for y in room.y0 + j..room.y0 + j + fh {
        for x in room.x0 + i..room.x0 + i + fw {
            if grid.get(x, y).owner == Owner::Room(ri as u16) {
                grid.restyle(x, y, |p| {
                    p.ceil += 48;
                    p.ceil_pic = "F_SKY1".into();
                    p.light = (p.light + 32).min(255);
                });
            }
        }
    }
    room.features.push("skylight");
}

fn light_panels(grid: &mut Grid, room: &mut Room, ri: usize, pal: &Palette) {
    let step = if room.area() > 250 { 5 } else { 4 };
    let (oi, oj) = (
        (room.w % step) / 2 + step / 2,
        (room.h % step) / 2 + step / 2,
    );
    let mut any = false;
    for y in room.y0..room.y0 + room.h {
        for x in room.x0..room.x0 + room.w {
            let (i, j) = (x - room.x0, y - room.y0);
            if (i - oi).rem_euclid(step) != 0 || (j - oj).rem_euclid(step) != 0 {
                continue;
            }
            if !interior(grid, room, ri, x, y) {
                continue;
            }
            grid.restyle(x, y, |p| {
                if p.ceil_pic != "F_SKY1" {
                    p.ceil -= 8;
                    p.ceil_pic.clone_from(&pal.light);
                    p.light = (p.light + 48).min(255);
                }
            });
            any = true;
        }
    }
    if any {
        room.features.push("light panels");
    }
}

fn terrain(
    grid: &mut Grid,
    room: &mut Room,
    ri: usize,
    pal: &Palette,
    opts: &Options,
    rng: &mut Rng,
) {
    // Ponds first (centre), then hills and boulders around.
    if opts.liquids && room.area() > 120 && rng.chance(0.5) {
        let (cx, cy) = (room.x0 + room.w / 2, room.y0 + room.h / 2);
        let rad = (room.w.min(room.h) / 5).max(2);
        let mut cells = Vec::new();
        for y in cy - rad..=cy + rad {
            for x in cx - rad..=cx + rad {
                if (x - cx).abs() + (y - cy).abs() <= rad && interior(grid, room, ri, x, y) {
                    cells.push((x, y));
                }
            }
        }
        for (x, y) in cells {
            grid.restyle(x, y, |p| {
                p.floor -= 16;
                p.floor_pic.clone_from(&pal.liquid);
                p.special = pal.liquid_damage;
            });
        }
        room.features.push("pond");
    }
    let blobs = 2 + rng.below(3) as i32;
    for _ in 0..blobs {
        let x = rng.range(room.x0 + 3, room.x0 + room.w - 4);
        let y = rng.range(room.y0 + 3, room.y0 + room.h - 4);
        let boulder = rng.chance(0.4);
        let rad = if boulder {
            rng.range(0, 1)
        } else {
            rng.range(1, 2)
        };
        let mut cells = Vec::new();
        let mut ok = true;
        for yy in y - rad..=y + rad {
            for xx in x - rad..=x + rad {
                if (xx - x).abs() + (yy - y).abs() > rad {
                    continue;
                }
                let c = grid.get(xx, yy);
                let base = grid
                    .props(xx, yy)
                    .is_some_and(|p| p.floor == room.floor && p.special == 0);
                if !interior(grid, room, ri, xx, yy) || c.busy || !base {
                    ok = false;
                }
                cells.push((xx, yy));
            }
        }
        if !ok {
            continue;
        }
        for (xx, yy) in cells {
            grid.restyle(xx, yy, |p| {
                if boulder {
                    p.floor += 64;
                    p.skirt.clone_from(&pal.sky_wall);
                } else {
                    p.floor += 16;
                    p.skirt.clone_from(&pal.sky_wall);
                }
            });
            if boulder && let Some(c) = grid.at(xx, yy) {
                c.busy = true;
            }
        }
    }
    room.features.push("rough ground");
}

/// Architecture: pilasters along long walls and a lowered ceiling ring
/// around the room edge, its inner face in the trim texture.
fn architecture(
    grid: &mut Grid,
    room: &mut Room,
    ri: usize,
    pal: &Palette,
    pillar: u16,
    rng: &mut Rng,
) {
    let own = |grid: &Grid, x: i32, y: i32| {
        let c = grid.get(x, y);
        c.owner == Owner::Room(ri as u16) && c.full_floor().is_some()
    };
    let solid = |grid: &Grid, x: i32, y: i32| {
        let c = grid.get(x, y);
        c.a.sector().is_none() && c.split.is_none()
    };
    let near_door = |x: i32, y: i32| {
        room.doors
            .iter()
            .any(|&(dx, dy)| (dx - x).abs() + (dy - y).abs() <= 3)
    };
    // Pilasters: every 5 cells on straight runs, away from doors and corners.
    if room.area() >= 100 && rng.chance(0.7) {
        let mut pil = Vec::new();
        for y in room.y0..room.y0 + room.h {
            for x in room.x0..room.x0 + room.w {
                if !own(grid, x, y) || grid.get(x, y).busy || near_door(x, y) {
                    continue;
                }
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if !solid(grid, x + dx, y + dy) {
                        continue;
                    }
                    let (tx, ty) = (dy, dx);
                    let along = if tx != 0 { x - room.x0 } else { y - room.y0 };
                    let run = (-2..=2).all(|t| {
                        own(grid, x + tx * t, y + ty * t)
                            && solid(grid, x + dx + tx * t, y + dy + ty * t)
                    });
                    if run && along % 5 == 2 {
                        pil.push((x, y));
                    }
                }
            }
        }
        for (x, y) in pil {
            grid.wall(x, y, pillar);
        }
        room.features.push("pilasters");
    }
    // Ceiling ring.
    let tall = grid
        .props(room.x0 + room.w / 2, room.y0 + room.h / 2)
        .is_some_and(|p| p.ceil - p.floor >= 160);
    if room.area() >= 80 && tall && rng.chance(0.8) {
        let drop: i16 = if room.area() > 250 { 48 } else { 32 };
        let mut ring = Vec::new();
        let mut inner = Vec::new();
        for y in room.y0..room.y0 + room.h {
            for x in room.x0..room.x0 + room.w {
                if !own(grid, x, y) {
                    continue;
                }
                let edge = (-1..=1).any(|dy| (-1..=1).any(|dx| !own(grid, x + dx, y + dy)));
                if edge {
                    ring.push((x, y))
                } else {
                    inner.push((x, y))
                }
            }
        }
        for (x, y) in ring {
            grid.restyle(x, y, |p| {
                p.ceil -= drop;
                p.light = (p.light - 16).max(96);
            });
        }
        for (x, y) in inner {
            grid.restyle(x, y, |p| p.wall.clone_from(&pal.trim));
        }
        room.features.push("ceiling ring");
    }
}

/// Dress every room.
pub fn dress(
    grid: &mut Grid,
    layout: &mut Layout,
    pals: &[Palette],
    opts: &Options,
    rng: &mut Rng,
) {
    let rooms = layout.rooms.len();
    for ri in 0..rooms {
        let room = &mut layout.rooms[ri];
        let pal = &pals[room.pal];
        let pillar = grid.mat(Mat {
            tex: pal.pillar.clone(),
            special: 0,
        });
        if crate::biomes::dress(grid, room, ri, pal, opts, rng) {
            if room.biome.organic() {
                crate::biomes::smooth(grid, room, ri);
            }
            continue;
        }
        if room.outdoor {
            terrain(grid, room, ri, pal, opts, rng);
            continue;
        }
        architecture(grid, room, ri, pal, pillar, rng);
        let small = room.w.min(room.h);
        let long = room.w.max(room.h) >= 2 * small - 2 && small <= 10;
        let mut order: Vec<u8> = if long {
            vec![4, 3]
        } else if small < 10 {
            vec![3, 5]
        } else {
            vec![0, 1, 2, 5, 6]
        };
        rng.shuffle(&mut order);
        if !long && small >= 10 && opts.verticality > 0 && rng.chance(0.5) {
            order.insert(0, 0);
        }
        for f in order {
            let done = match f {
                0 => dais(grid, room, ri, pal, opts),
                1 => pit(grid, room, ri, pal, opts, rng),
                2 => pillars(grid, room, ri, pillar),
                3 => crates(grid, room, ri, pal, opts, rng),
                4 => colonnade(grid, room, ri, pillar),
                5 => opts.cover > 0 && pillars(grid, room, ri, pillar),
                _ => ring(grid, room, ri, pillar),
            };
            if done {
                break;
            }
        }
        // Extra cover around the set piece.
        if opts.cover >= 2 && small >= 10 {
            crates(grid, room, ri, pal, opts, rng);
        }
        if room.area() >= 120 && rng.chance(0.35) {
            skylight(grid, room, ri);
        } else if rng.chance(0.75) {
            light_panels(grid, room, ri, pal);
        }
    }
}

/// Teleporter pads in pairs of far-apart rooms.
pub fn teleporters(
    grid: &mut Grid,
    layout: &mut Layout,
    opts: &Options,
    rng: &mut Rng,
    gate: &str,
) -> Vec<(Pad, Pad)> {
    if !opts.teleporters || layout.rooms.len() < 6 {
        return Vec::new();
    }
    let pairs_wanted = (1 + opts.players as usize / 6).min(4);
    // Rooms that can hold a pad, then the farthest pairs among them.
    let mut spots: Vec<Option<Pad>> = (0..layout.rooms.len())
        .map(|ri| pad_spot(grid, &layout.rooms[ri], ri, rng))
        .collect();
    let mut out = Vec::new();
    let mut tag: i16 = 100;
    for _ in 0..pairs_wanted {
        let mut best: Option<(f64, usize, usize)> = None;
        for a in 0..spots.len() {
            for b in a + 1..spots.len() {
                if spots[a].is_none() || spots[b].is_none() {
                    continue;
                }
                let (ax, ay) = layout.rooms[a].center();
                let (bx, by) = layout.rooms[b].center();
                let d = (ax - bx).hypot(ay - by) * (0.8 + rng.f64() * 0.4);
                if best.is_none_or(|(bd, ..)| d > bd) {
                    best = Some((d, a, b));
                }
            }
        }
        let Some((_, a, b)) = best else { break };
        let (Some(pa), Some(pb)) = (spots[a].take(), spots[b].take()) else {
            break;
        };
        let (ta, tb) = (tag, tag + 1);
        tag += 2;
        for (pad, own, to) in [(pa, ta, tb), (pb, tb, ta)] {
            for y in pad.y..pad.y + 2 {
                for x in pad.x..pad.x + 2 {
                    grid.restyle(x, y, |p| {
                        p.floor += 8;
                        p.floor_pic = gate.to_owned();
                        p.light = 224;
                        p.special = 8;
                        p.tag = own;
                        p.teleport_to = to;
                    });
                    if let Some(c) = grid.at(x, y) {
                        c.busy = true;
                    }
                }
            }
            // Keep a clear ring around the pad.
            for y in pad.y - 1..pad.y + 3 {
                for x in pad.x - 1..pad.x + 3 {
                    if let Some(c) = grid.at(x, y) {
                        c.busy = true;
                    }
                }
            }
            layout.rooms[pad.room].features.push("teleporter");
        }
        out.push((pa, pb));
    }
    out
}

fn pad_spot(grid: &Grid, room: &Room, ri: usize, rng: &mut Rng) -> Option<Pad> {
    let mut cands = Vec::new();
    for y in room.y0..room.y0 + room.h - 1 {
        for x in room.x0..room.x0 + room.w - 1 {
            let ok = (y - 1..y + 3).all(|yy| {
                (x - 1..x + 3).all(|xx| {
                    let c = grid.get(xx, yy);
                    interior(grid, room, ri, xx, yy)
                        && !c.busy
                        && grid
                            .props(xx, yy)
                            .is_some_and(|p| p.special == 0 && p.floor == room.floor)
                })
            });
            if ok {
                cands.push((x, y));
            }
        }
    }
    // Off-centre (the middle is for set pieces): among the farthest quarter.
    let (cx, cy) = room.center();
    cands.sort_by(|a, b| {
        let da = (f64::from(a.0) + 1.0 - cx).hypot(f64::from(a.1) + 1.0 - cy);
        let db = (f64::from(b.0) + 1.0 - cx).hypot(f64::from(b.1) + 1.0 - cy);
        db.total_cmp(&da)
    });
    cands.truncate((cands.len() / 4).max(1));
    let &(x, y) = rng.pick(&cands)?;
    let (px, py) = (f64::from(x) + 1.0, f64::from(y) + 1.0);
    let angle = (cy - py).atan2(cx - px).to_degrees();
    let angle = ((angle / 45.0).round() * 45.0).rem_euclid(360.0) as i16;
    Some(Pad {
        room: ri,
        x,
        y,
        angle,
    })
}

/// Exit switch on a straight wall of the room farthest from the hub.
pub fn exit_switch(grid: &mut Grid, layout: &Layout, pals: &[Palette]) -> bool {
    let hub = layout.rooms[0].center();
    let mut order: Vec<usize> = (1..layout.rooms.len()).collect();
    order.sort_by(|&a, &b| {
        let da = dist(layout.rooms[a].center(), hub);
        let db = dist(layout.rooms[b].center(), hub);
        db.total_cmp(&da)
    });
    for ri in order {
        let room = &layout.rooms[ri];
        let tex = pals[room.pal].switch.clone();
        let mat = grid.mat(Mat { tex, special: 11 });
        for y in room.y0 - 1..=room.y0 + room.h {
            for x in room.x0 - 1..=room.x0 + room.w {
                if !matches!(grid.get(x, y).a, Area::Solid(0)) || grid.get(x, y).split.is_some() {
                    continue;
                }
                // Inside neighbour: plain room floor; the wall runs straight.
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (ix, iy) = (x + dx, y + dy);
                    let (ax, ay) = (dy, dx);
                    let inside_ok = |gx: i32, gy: i32| {
                        let c = grid.get(gx, gy);
                        c.owner == Owner::Room(ri as u16) && c.full_floor().is_some() && !c.busy
                    };
                    let wall_ok = |gx: i32, gy: i32| {
                        matches!(grid.get(gx, gy).a, Area::Solid(0))
                            && grid.get(gx, gy).split.is_none()
                    };
                    let straight = (-1..=1).all(|t| {
                        inside_ok(ix + ax * t, iy + ay * t) && wall_ok(x + ax * t, y + ay * t)
                    });
                    let far_from_doors = room
                        .doors
                        .iter()
                        .all(|&(qx, qy)| (qx - x).abs() + (qy - y).abs() > 4);
                    if straight && far_from_doors {
                        grid.wall(x, y, mat);
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// World coordinate of cell corner `c`.
#[must_use]
pub fn world(c: i32, origin: i32) -> i32 {
    c * CELL - origin
}

/// Sector properties template for doc tests.
#[must_use]
pub fn plain(floor: i16) -> SecProps {
    SecProps {
        floor,
        ceil: floor + 128,
        floor_pic: "FLOOR0_1".into(),
        ceil_pic: "CEIL3_5".into(),
        light: 160,
        special: 0,
        tag: 0,
        wall: "STARTAN3".into(),
        skirt: "STEP1".into(),
        teleport_to: 0,
        door: false,
    }
}

/// Secret closets: a 3×3 room behind a straight wall, opened by using the
/// wall (a door that looks like it), flagged as a Doom secret. Returns the
/// closet centres.
pub fn secrets(
    grid: &mut Grid,
    layout: &Layout,
    pals: &[Palette],
    rng: &mut Rng,
    want: usize,
) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let mut order: Vec<usize> = (1..layout.rooms.len()).collect();
    rng.shuffle(&mut order);
    for ri in order {
        if out.len() >= want {
            break;
        }
        let room = &layout.rooms[ri];
        let pal = &pals[room.pal];
        let rock = |grid: &Grid, x: i32, y: i32| {
            let c = grid.get(x, y);
            grid.inside(x, y)
                && c.owner == Owner::None
                && matches!(c.a, Area::Solid(0))
                && c.split.is_none()
                && !c.busy
        };
        let inside_ok = |grid: &Grid, x: i32, y: i32| {
            let c = grid.get(x, y);
            c.owner == Owner::Room(ri as u16) && c.full_floor().is_some() && !c.busy
        };
        let mut found = None;
        'scan: for y in room.y0 - 1..=room.y0 + room.h {
            for x in room.x0 - 1..=room.x0 + room.w {
                for (nx, ny) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    // (x, y) = wall cell; inside floor is at (x, y) - n.
                    let (fx, fy) = (x - nx, y - ny);
                    let (tx, ty) = (ny, nx);
                    if !inside_ok(grid, fx, fy)
                        || room
                            .doors
                            .iter()
                            .any(|&(dx, dy)| (dx - x).abs() + (dy - y).abs() <= 4)
                    {
                        continue;
                    }
                    let straight = (-1..=1).all(|t| {
                        inside_ok(grid, fx + tx * t, fy + ty * t)
                            && rock(grid, x + tx * t, y + ty * t)
                    });
                    // Closet 3 deep behind the door cell, 3 wide, plus a rock margin.
                    let behind = (1..=4).all(|d| {
                        (-2..=2).all(|t| rock(grid, x + nx * d + tx * t, y + ny * d + ty * t))
                    });
                    if straight && behind && rng.chance(0.25) {
                        found = Some((x, y, nx, ny, fx, fy));
                        break 'scan;
                    }
                }
            }
        }
        let Some((x, y, nx, ny, fx, fy)) = found else {
            continue;
        };
        let Some(base) = grid.props(fx, fy).cloned() else {
            continue;
        };
        let door = grid.sector(SecProps {
            ceil: base.floor,
            door: true,
            ..base.clone()
        });
        let closet = grid.sector(SecProps {
            floor: base.floor,
            ceil: base.floor + 96,
            floor_pic: pal.floor_alt.clone(),
            ceil_pic: pal.light.clone(),
            light: 208,
            special: 9,
            // Own tag: two identical closets must stay two sectors (two secrets).
            tag: 300 + out.len() as i16,
            wall: pal.trim.clone(),
            skirt: pal.skirt.clone(),
            teleport_to: 0,
            door: false,
        });
        let (tx, ty) = (ny, nx);
        if let Some(c) = grid.at(x, y) {
            c.a = Area::Floor(door);
            c.busy = true;
        }
        for d in 1..=3 {
            for t in -1..=1 {
                if let Some(c) = grid.at(x + nx * d + tx * t, y + ny * d + ty * t) {
                    c.a = Area::Floor(closet);
                    c.busy = true;
                }
            }
        }
        out.push((x + nx * 2, y + ny * 2));
    }
    out
}
