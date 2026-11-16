//! Biomes: what a room *is*, beyond its textures.
//!
//! - **Cave**: organic outline (cellular automata), rolling rock floor,
//!   uneven ceiling, dim varied light, liquid streams.
//! - **Canyon**: open rock valley under the sky, terraced slopes, rock
//!   islands left standing by the automaton.
//! - **Swamp**: liquid everywhere, dry islands, wooden boardwalks from every
//!   doorway to a central platform.
//! - **Courtyard**: open-air fort around a central keep (an indoor
//!   building with doorways), moat with bridges, corner towers.
//! - **Plaza**: urban square with building blocks, some hollow with a
//!   doorway, streets between.
//!
//! Every height field is clamped so neighbouring cells differ by at most
//! 16 units (walkable both ways) and stays at the room's base height at its
//! doorways, so corridors always meet it flush.

use crate::Options;
use crate::grid::{Area, Diag, Grid, Mat, Owner, SecProps};
use crate::layout::Room;
use crate::noise::Noise;
use crate::rng::Rng;
use crate::theme::Palette;
use serde::{Deserialize, Serialize};

/// Kind of room.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Biome {
    /// Built interior (set pieces, architecture).
    #[default]
    Indoor,
    /// Rectangular open-air yard (hills, boulders, ponds).
    Yard,
    /// Organic cavern.
    Cave,
    /// Organic valley under the sky.
    Canyon,
    /// Marsh with islands and boardwalks.
    Swamp,
    /// Fort courtyard around a keep.
    Courtyard,
    /// Urban square with buildings.
    Plaza,
}

impl Biome {
    /// Open to the sky.
    #[must_use]
    pub fn outdoor(self) -> bool {
        !matches!(self, Biome::Indoor | Biome::Cave)
    }

    /// Outline grown by cellular automata.
    #[must_use]
    pub fn organic(self) -> bool {
        matches!(self, Biome::Cave | Biome::Canyon | Biome::Swamp)
    }
}

/// Biome mix for the whole map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Biomes {
    /// A bit of everything, by district.
    #[default]
    Mixed,
    /// Built interiors (and open yards per the outdoor setting).
    Indoor,
    /// Mostly caverns, canyons outside.
    Caves,
    /// Canyons, swamps and caves.
    Wilderness,
    /// Plazas and courtyards between buildings.
    Urban,
}

/// Pick a room's biome from the map mix and the district roll `d` (0..1).
pub fn choose(biomes: Biomes, outdoor: u8, w: i32, h: i32, d: f64, rng: &mut Rng) -> Biome {
    let small = w.min(h);
    let open = match outdoor {
        0 => 0.0,
        1 => 0.25,
        _ => 0.5,
    };
    let yard_or = |b: Biome, min: i32| if small >= min { b } else { Biome::Yard };
    let pick = match biomes {
        Biomes::Indoor => {
            if rng.chance(open) {
                Biome::Yard
            } else {
                Biome::Indoor
            }
        }
        Biomes::Caves => {
            if rng.chance(open * 0.8) {
                Biome::Canyon
            } else if d < 0.75 {
                Biome::Cave
            } else {
                Biome::Indoor
            }
        }
        Biomes::Wilderness => match d {
            d if d < 0.4 => Biome::Canyon,
            d if d < 0.7 => Biome::Swamp,
            _ => Biome::Cave,
        },
        Biomes::Urban => {
            if small >= 14 {
                if rng.chance(0.5) {
                    Biome::Plaza
                } else {
                    yard_or(Biome::Courtyard, 16)
                }
            } else if rng.chance(open) {
                Biome::Yard
            } else {
                Biome::Indoor
            }
        }
        Biomes::Mixed => {
            // District decides the flavour; the outdoor knob how much sky.
            let outside = rng.chance(open + 0.1);
            match d {
                d if d < 0.3 => {
                    if outside {
                        Biome::Yard
                    } else {
                        Biome::Indoor
                    }
                }
                d if d < 0.5 => {
                    if outside {
                        Biome::Canyon
                    } else {
                        Biome::Cave
                    }
                }
                d if d < 0.65 => {
                    if outside {
                        Biome::Swamp
                    } else {
                        Biome::Cave
                    }
                }
                d if d < 0.85 => {
                    if outside && small >= 14 {
                        if rng.chance(0.5) {
                            Biome::Plaza
                        } else {
                            yard_or(Biome::Courtyard, 16)
                        }
                    } else {
                        Biome::Indoor
                    }
                }
                _ => {
                    if outside {
                        Biome::Canyon
                    } else {
                        Biome::Indoor
                    }
                }
            }
        }
    };
    // Without sky (outdoor 0) open biomes fall back to their roofed kin,
    // unless the mix is all about them.
    if outdoor == 0 && pick.outdoor() && !matches!(biomes, Biomes::Wilderness | Biomes::Urban) {
        return if pick.organic() {
            Biome::Cave
        } else {
            Biome::Indoor
        };
    }
    if pick.organic() && small < 10 {
        return if pick.outdoor() {
            Biome::Yard
        } else {
            Biome::Indoor
        };
    }
    if pick == Biome::Plaza && small < 14 {
        return Biome::Yard;
    }
    pick
}

/// Organic outline: cellular automata, doorway pads in the middle third of
/// every side, wandering passages from each pad to the centre, then the
/// part connected to the centre.
pub fn organic_mask(rng: &mut Rng, w: i32, h: i32, fill: f64) -> Vec<bool> {
    let idx = |i: i32, j: i32| (j * w + i) as usize;
    let inside = |i: i32, j: i32| i > 0 && j > 0 && i < w - 1 && j < h - 1;
    let mut m: Vec<bool> = (0..w * h)
        .map(|k| inside(k % w, k / w) && !rng.chance(fill))
        .collect();
    for _ in 0..4 {
        let prev = m.clone();
        for j in 1..h - 1 {
            for i in 1..w - 1 {
                let mut walls = 0;
                for dj in -1..=1 {
                    for di in -1..=1 {
                        if (di, dj) != (0, 0) && !prev[idx(i + di, j + dj)] {
                            walls += 1;
                        }
                    }
                }
                m[idx(i, j)] = walls < 5;
            }
        }
    }
    let (cx, cy) = (w / 2, h / 2);
    let carve = |m: &mut Vec<bool>, x: i32, y: i32, r: i32| {
        for j in y - r..=y + r {
            for i in x - r..=x + r {
                if i >= 0 && j >= 0 && i < w && j < h {
                    m[idx(i, j)] = true;
                }
            }
        }
    };
    carve(&mut m, cx, cy, 2);
    // Doorway pads (3 deep, middle third) and a wandering path inwards.
    let (tw, th) = (w / 3, h / 3);
    let pads = [
        (tw..w - tw, 0, true, (cx, 1)),
        (tw..w - tw, h - 3, true, (cx, h - 2)),
        (th..h - th, 0, false, (1, cy)),
        (th..h - th, w - 3, false, (w - 2, cy)),
    ];
    for (range, at, horizontal, start) in pads {
        for t in range {
            for d in 0..3 {
                if horizontal {
                    m[idx(t, at + d)] = true;
                } else {
                    m[idx(at + d, t)] = true;
                }
            }
        }
        let (mut x, mut y) = start;
        for _ in 0..(w + h) * 2 {
            carve(&mut m, x, y, 1);
            if (x - cx).abs() <= 1 && (y - cy).abs() <= 1 {
                break;
            }
            if rng.chance(0.7) {
                if (x - cx).abs() > (y - cy).abs() {
                    x += (cx - x).signum()
                } else {
                    y += (cy - y).signum()
                }
            } else if horizontal {
                x = (x + rng.range(-1, 1)).clamp(2, w - 3);
            } else {
                y = (y + rng.range(-1, 1)).clamp(2, h - 3);
            }
        }
    }
    // Keep what the centre reaches.
    let mut keep = vec![false; m.len()];
    let mut stack = vec![(cx, cy)];
    keep[idx(cx, cy)] = true;
    while let Some((x, y)) = stack.pop() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= 0 && ny >= 0 && nx < w && ny < h && m[idx(nx, ny)] && !keep[idx(nx, ny)] {
                keep[idx(nx, ny)] = true;
                stack.push((nx, ny));
            }
        }
    }
    keep
}

/// Room cells (whole floor) with their distance to the nearest door.
fn cells(grid: &Grid, room: &Room, ri: usize) -> Vec<(i32, i32, i32)> {
    let mut out = Vec::new();
    for y in room.y0..room.y0 + room.h {
        for x in room.x0..room.x0 + room.w {
            let c = grid.get(x, y);
            if c.owner == Owner::Room(ri as u16) && c.full_floor().is_some() {
                let d = room
                    .doors
                    .iter()
                    .map(|&(dx, dy)| (dx - x).abs() + (dy - y).abs())
                    .min()
                    .unwrap_or(99);
                out.push((x, y, d));
            }
        }
    }
    out
}

/// Height offsets (≥ 0, multiples of `quantum` ≤ 16) from noise of amplitude `amp`,
/// faded to 0 at doors and busy cells, then clamped so neighbours differ
/// by at most 16.
fn heights(
    grid: &Grid,
    room: &Room,
    ri: usize,
    noise: &Noise,
    amp: f64,
    quantum: f64,
) -> Vec<(i32, i32, i16)> {
    let list = cells(grid, room, ri);
    let w = room.w;
    let at = |x: i32, y: i32| ((y - room.y0) * w + (x - room.x0)) as usize;
    let mut h = vec![f64::INFINITY; (room.w * room.h) as usize];
    for &(x, y, d) in &list {
        let fade = (f64::from(d - 3) / 5.0).clamp(0.0, 1.0);
        let busy = grid.get(x, y).busy;
        h[at(x, y)] = if busy {
            0.0
        } else {
            noise.at(x - room.x0, y - room.y0) * amp * fade
        };
    }
    // Lower Lipschitz envelope (16 per cell).
    for _ in 0..(room.w + room.h) * 2 {
        let mut changed = false;
        for &(x, y, _) in &list {
            let mut v = h[at(x, y)];
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < room.x0 || ny < room.y0 || nx >= room.x0 + room.w || ny >= room.y0 + room.h
                {
                    continue;
                }
                let n = h[at(nx, ny)];
                if n.is_finite() && n + 16.0 < v {
                    v = n + 16.0;
                }
            }
            if v < h[at(x, y)] {
                h[at(x, y)] = v;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    list.iter()
        .map(|&(x, y, _)| (x, y, ((h[at(x, y)] / quantum).floor() * quantum) as i16))
        .collect()
}

fn cave(grid: &mut Grid, room: &mut Room, ri: usize, pal: &Palette, opts: &Options, rng: &mut Rng) {
    let amp = [16.0, 32.0, 48.0][opts.verticality.min(2) as usize];
    let floor_n = Noise::new(rng, room.w, room.h, 5.0);
    let ceil_n = Noise::new(rng, room.w, room.h, 4.0);
    let stream_n = Noise::new(rng, room.w, room.h, 7.0);
    let streams = opts.liquids && rng.chance(0.6);
    for (x, y, off) in heights(grid, room, ri, &floor_n, amp, 8.0) {
        let busy = grid.get(x, y).busy;
        let c = ceil_n.at(x - room.x0, y - room.y0);
        let ceil_off = (112.0 + (c * 6.0).floor() * 16.0) as i16;
        let stream =
            streams && !busy && (stream_n.at(x - room.x0, y - room.y0) - 0.5).abs() < 0.035;
        let door_near = room
            .doors
            .iter()
            .any(|&(dx, dy)| (dx - x).abs() + (dy - y).abs() <= 3);
        grid.restyle(x, y, |p| {
            p.floor = room.floor + off;
            p.ceil = p.floor + ceil_off;
            p.light = (104 + (c * 3.0).floor() as i16 * 16).min(160);
            if stream && !door_near {
                p.floor -= 8;
                p.floor_pic.clone_from(&pal.liquid);
                p.special = pal.liquid_damage;
                p.light = (p.light + 32).min(200);
            }
        });
    }
    room.features.push(if streams {
        "cavern with streams"
    } else {
        "cavern"
    });
}

fn canyon(
    grid: &mut Grid,
    room: &mut Room,
    ri: usize,
    pal: &Palette,
    opts: &Options,
    rng: &mut Rng,
) {
    let amp = [24.0, 48.0, 80.0][opts.verticality.min(2) as usize];
    let n = Noise::new(rng, room.w, room.h, 9.0);
    let sky = room.floor + 96 + amp as i16 + 160;
    // Broad 16-unit terraces.
    for (x, y, off) in heights(grid, room, ri, &n, amp, 16.0) {
        let high = f64::from(off) > amp * 0.5;
        grid.restyle(x, y, |p| {
            p.floor = room.floor + off;
            p.ceil = sky;
            if high {
                p.floor_pic.clone_from(&pal.cave_floor);
            }
        });
    }
    room.features.push("canyon");
}

fn swamp(grid: &mut Grid, room: &mut Room, ri: usize, pal: &Palette, rng: &mut Rng) {
    let n = Noise::new(rng, room.w, room.h, 4.0);
    let (cx, cy) = (room.x0 + room.w / 2, room.y0 + room.h / 2);
    // Boardwalks: door → centre, 3 wide; platform in the middle.
    let mut walk = std::collections::HashSet::new();
    for &(dx, dy) in &room.doors {
        let (mut x, mut y) = (dx, dy);
        for _ in 0..room.w + room.h {
            for j in y - 1..=y + 1 {
                for i in x - 1..=x + 1 {
                    walk.insert((i, j));
                }
            }
            if (x, y) == (cx, cy) {
                break;
            }
            if x != cx {
                x += (cx - x).signum();
            } else {
                y += (cy - y).signum();
            }
        }
    }
    for j in cy - 2..=cy + 2 {
        for i in cx - 2..=cx + 2 {
            walk.insert((i, j));
        }
    }
    let damage = if pal.swamp_liquid.starts_with("NUKAGE") {
        7
    } else {
        0
    };
    for (x, y, d) in cells(grid, room, ri) {
        let busy = grid.get(x, y).busy;
        if busy || d <= 1 {
            continue;
        }
        if walk.contains(&(x, y)) {
            grid.restyle(x, y, |p| p.floor_pic.clone_from(&pal.wood));
            continue;
        }
        if n.at(x - room.x0, y - room.y0) > 0.55 {
            continue; // dry island
        }
        grid.restyle(x, y, |p| {
            p.floor = room.floor - 16;
            p.floor_pic.clone_from(&pal.swamp_liquid);
            p.special = damage;
        });
    }
    room.power.push((cx, cy));
    room.features.push("swamp");
}

/// Free interior rectangle (room-local origin `x, y`, size `w × h`, grown
/// by `margin`): whole floor of this room, not busy, away from doors.
fn free_rect(
    grid: &Grid,
    room: &Room,
    ri: usize,
    (x, y, w, h): (i32, i32, i32, i32),
    margin: i32,
) -> bool {
    for j in y - margin..y + h + margin {
        for i in x - margin..x + w + margin {
            let c = grid.get(i, j);
            if c.owner != Owner::Room(ri as u16) || c.full_floor().is_none() || c.busy {
                return false;
            }
            if room
                .doors
                .iter()
                .any(|&(dx, dy)| (dx - i).abs().max((dy - j).abs()) < 4)
            {
                return false;
            }
        }
    }
    true
}

fn indoor_props(base: &SecProps, pal: &Palette, rise: i16, height: i16) -> SecProps {
    SecProps {
        floor: base.floor + rise,
        ceil: base.floor + rise + height,
        floor_pic: pal.floor.clone(),
        ceil_pic: pal.ceil.clone(),
        light: (base.light - 48).max(112),
        special: 0,
        tag: 0,
        wall: pal.building.clone(),
        skirt: pal.step.clone(),
        teleport_to: 0,
        door: false,
    }
}

fn courtyard(grid: &mut Grid, room: &mut Room, ri: usize, pal: &Palette, rng: &mut Rng) -> bool {
    let kw = (room.w / 2) | 1;
    let kh = (room.h / 2) | 1;
    let (kx, ky) = (room.x0 + (room.w - kw) / 2, room.y0 + (room.h - kh) / 2);
    if kw < 7 || kh < 7 || !free_rect(grid, room, ri, (kx, ky, kw, kh), 2) {
        return false;
    }
    let Some(base) = grid.props(kx, ky).cloned() else {
        return false;
    };
    let wall = grid.mat(Mat {
        tex: pal.building.clone(),
        special: 0,
    });
    let tower = grid.mat(Mat {
        tex: pal.pillar.clone(),
        special: 0,
    });
    let inside = grid.sector(indoor_props(&base, pal, 16, 128));
    let bridge = grid.sector(SecProps {
        floor_pic: pal.wood.clone(),
        ..base.clone()
    });
    let moat = grid.sector(SecProps {
        floor: base.floor - 16,
        floor_pic: pal.swamp_liquid.clone(),
        light: base.light,
        ..base.clone()
    });
    // Doorways: middle of 2–4 sides (3 wide).
    let mut sides = [0, 1, 2, 3];
    rng.shuffle(&mut sides);
    let n_gates = rng.range(2, 4) as usize;
    let gates: Vec<i32> = sides[..n_gates].to_vec();
    let gate_at = |x: i32, y: i32| -> Option<i32> {
        let (mx, my) = (kx + kw / 2, ky + kh / 2);
        let s = if y == ky + kh - 1 && (x - mx).abs() <= 1 {
            0
        } else if x == kx + kw - 1 && (y - my).abs() <= 1 {
            1
        } else if y == ky && (x - mx).abs() <= 1 {
            2
        } else if x == kx && (y - my).abs() <= 1 {
            3
        } else {
            return None;
        };
        gates.contains(&s).then_some(s)
    };
    for y in ky - 2..ky + kh + 2 {
        for x in kx - 2..kx + kw + 2 {
            let in_keep = x >= kx && y >= ky && x < kx + kw && y < ky + kh;
            let ring = in_keep && (x == kx || y == ky || x == kx + kw - 1 || y == ky + kh - 1);
            let Some(c) = grid.at(x, y) else { continue };
            if in_keep {
                if ring && gate_at(x, y).is_none() {
                    c.a = Area::Solid(wall);
                } else {
                    c.a = Area::Floor(inside);
                }
                c.split = None;
                c.busy = ring;
                continue;
            }
            // Moat, with bridges in front of the gates and towers at corners.
            let corner = (x < kx || x >= kx + kw) && (y < ky || y >= ky + kh);
            let (mx, my) = (kx + kw / 2, ky + kh / 2);
            let facing_gate = ((x - mx).abs() <= 1
                && (gates.contains(&0) && y >= ky + kh || gates.contains(&2) && y < ky))
                || ((y - my).abs() <= 1
                    && (gates.contains(&1) && x >= kx + kw || gates.contains(&3) && x < kx));
            if corner
                && (x - kx).abs().min((x - (kx + kw - 1)).abs()) <= 2
                && (y - ky).abs().min((y - (ky + kh - 1)).abs()) <= 2
            {
                c.a = Area::Solid(tower);
                c.busy = true;
            } else if facing_gate {
                c.a = Area::Floor(bridge);
            } else {
                c.a = Area::Floor(moat);
            }
            c.split = None;
        }
    }
    room.power.push((kx + kw / 2, ky + kh / 2));
    room.features.push("keep with moat");
    true
}

fn plaza(grid: &mut Grid, room: &mut Room, ri: usize, pal: &Palette, rng: &mut Rng) -> bool {
    let walls = [
        grid.mat(Mat {
            tex: pal.building.clone(),
            special: 0,
        }),
        grid.mat(Mat {
            tex: pal.building_alt.clone(),
            special: 0,
        }),
    ];
    let street = 3;
    let mut placed = 0;
    let mut y = room.y0 + 3;
    while y + 4 <= room.y0 + room.h - 3 {
        let bh = rng.range(4, 5);
        let mut x = room.x0 + 3;
        while x + 4 <= room.x0 + room.w - 3 {
            let bw = rng.range(4, 6);
            if x + bw <= room.x0 + room.w - 3
                && y + bh <= room.y0 + room.h - 3
                && rng.chance(0.8)
                && free_rect(grid, room, ri, (x, y, bw, bh), 1)
            {
                let Some(base) = grid.props(x, y).cloned() else {
                    break;
                };
                let mat = walls[rng.below(2) as usize];
                let hollow = bw >= 5 && bh >= 5 && rng.chance(0.5);
                let inside = grid.sector(indoor_props(&base, pal, 0, 96));
                // Doorway in the middle of the side facing the room centre.
                let (cx, cy) = room.center();
                let (bx, by) = (
                    f64::from(x) + f64::from(bw) / 2.0,
                    f64::from(y) + f64::from(bh) / 2.0,
                );
                let horiz = (cx - bx).abs() > (cy - by).abs();
                for j in y..y + bh {
                    for i in x..x + bw {
                        let edge = i == x || j == y || i == x + bw - 1 || j == y + bh - 1;
                        let door = hollow
                            && edge
                            && if horiz {
                                (i == if cx > bx { x + bw - 1 } else { x })
                                    && (j - (y + bh / 2)).abs() <= 1
                                    && j > y
                                    && j < y + bh - 1
                            } else {
                                (j == if cy > by { y + bh - 1 } else { y })
                                    && (i - (x + bw / 2)).abs() <= 1
                                    && i > x
                                    && i < x + bw - 1
                            };
                        let Some(c) = grid.at(i, j) else { continue };
                        if hollow && (!edge || door) {
                            c.a = Area::Floor(inside);
                        } else {
                            c.a = Area::Solid(mat);
                        }
                        c.split = None;
                        c.busy = true;
                    }
                }
                placed += 1;
            }
            x += bw + street;
        }
        y += bh + street;
    }
    if placed > 0 {
        room.features.push("city blocks");
    }
    placed > 0
}

/// Dress a non-indoor room. Returns `false` when it should fall back to the
/// indoor set pieces.
pub fn dress(
    grid: &mut Grid,
    room: &mut Room,
    ri: usize,
    pal: &Palette,
    opts: &Options,
    rng: &mut Rng,
) -> bool {
    match room.biome {
        Biome::Indoor | Biome::Yard => false,
        Biome::Cave => {
            cave(grid, room, ri, pal, opts, rng);
            true
        }
        Biome::Canyon => {
            canyon(grid, room, ri, pal, opts, rng);
            true
        }
        Biome::Swamp => {
            swamp(grid, room, ri, pal, rng);
            true
        }
        Biome::Courtyard => courtyard(grid, room, ri, pal, rng),
        Biome::Plaza => plaza(grid, room, ri, pal, rng),
    }
}

/// Smooth organic outlines: cut convex corners and fill concave ones along
/// 45° diagonals (away from doorways).
pub fn smooth(grid: &mut Grid, room: &Room, ri: usize) {
    let own = Owner::Room(ri as u16);
    let floor_of = |grid: &Grid, x: i32, y: i32| -> Option<u32> {
        let c = grid.get(x, y);
        (c.owner == own && !c.busy)
            .then(|| c.full_floor())
            .flatten()
    };
    let rock = |grid: &Grid, x: i32, y: i32| {
        let c = grid.get(x, y);
        c.owner == Owner::None && matches!(c.a, Area::Solid(0)) && c.split.is_none()
    };
    let near_door = |x: i32, y: i32| {
        room.doors
            .iter()
            .any(|&(dx, dy)| (dx - x).abs().max((dy - y).abs()) <= 3)
    };
    // (solid sides, diagonal, floor half is `a`)
    let corners = [
        ((0, 1), (-1, 0), Diag::Slash, true), // N + W solid → floor SE
        ((0, 1), (1, 0), Diag::Backslash, true), // N + E solid → floor SW
        ((0, -1), (-1, 0), Diag::Backslash, false), // S + W solid → floor NE
        ((0, -1), (1, 0), Diag::Slash, false), // S + E solid → floor NW
    ];
    // Two passes: cut convex corners, then fill concave ones next to cells
    // that are still whole (a fill beside a cut would float alone).
    for pass in 0..2 {
        let mut todo = Vec::new();
        for y in room.y0 - 1..=room.y0 + room.h {
            for x in room.x0 - 1..=room.x0 + room.w {
                if near_door(x, y) {
                    continue;
                }
                for &((ax, ay), (bx, by), diag, floor_a) in &corners {
                    let (ox, oy) = (-ax - bx, -ay - by);
                    if pass == 0 {
                        let Some(s) = floor_of(grid, x, y) else { break };
                        let solid_sides = rock(grid, x + ax, y + ay) && rock(grid, x + bx, y + by);
                        let open_sides = floor_of(grid, x - ax, y - ay).is_some()
                            && floor_of(grid, x - bx, y - by).is_some();
                        if solid_sides && open_sides {
                            todo.push((x, y, diag, floor_a, s));
                            break;
                        }
                    } else {
                        if !rock(grid, x, y) {
                            break;
                        }
                        let fa = floor_of(grid, x - ax, y - ay);
                        let fb = floor_of(grid, x - bx, y - by);
                        let fd = floor_of(grid, x + ox, y + oy);
                        let other = rock(grid, x + ax, y + ay) && rock(grid, x + bx, y + by);
                        let height = |s: u32| grid.sectors.get(s as usize).map_or(0, |p| p.floor);
                        if let (Some(s), Some(sb), Some(_)) = (fa, fb, fd)
                            && other
                            && (height(s) - height(sb)).abs() <= 16
                        {
                            todo.push((x, y, diag, floor_a, s));
                            break;
                        }
                    }
                }
            }
        }
        for (x, y, diag, floor_a, s) in todo {
            let Some(c) = grid.at(x, y) else { continue };
            let (a, b) = if floor_a {
                (Area::Floor(s), Area::VOID)
            } else {
                (Area::VOID, Area::Floor(s))
            };
            c.a = a;
            c.split = Some((diag, b));
            c.owner = own;
        }
    }
}
