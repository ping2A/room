//! Macro layout: rooms of varied shapes scattered on the canvas, a spanning
//! tree of corridors plus extra loops (deathmatch flow), routed with A*
//! around everything else, then floor heights per room with stairs in the
//! corridors that climb between them.

use crate::Options;
use crate::biomes::{self, Biome};
use crate::grid::{Area, Diag, Grid, Owner, SecProps};
use crate::rng::Rng;
use crate::theme::Palette;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

/// Room outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Rectangle.
    Rect,
    /// Rectangle with 45° corners cut `k` cells deep.
    Octagon(i32),
    /// Plus sign: corners of `kx` × `ky` cells removed.
    Cross(i32, i32),
    /// Grown by cellular automata (see [`Room::mask`]).
    Organic,
}

/// What a cell of a room's bounding box is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Not in the room.
    Out,
    /// Room floor.
    Full,
    /// Cut by a diagonal; `floor_is_a` tells which half is floor.
    Split(Diag, bool),
}

/// One room.
#[derive(Clone, Debug)]
pub struct Room {
    /// Left cell.
    pub x0: i32,
    /// Bottom cell.
    pub y0: i32,
    /// Width in cells.
    pub w: i32,
    /// Height in cells.
    pub h: i32,
    /// Outline.
    pub shape: Shape,
    /// Open to the sky.
    pub outdoor: bool,
    /// What kind of place this is.
    pub biome: Biome,
    /// Floor cells of an organic outline (`w × h`, row-major).
    pub mask: Option<Vec<bool>>,
    /// Palette index.
    pub pal: usize,
    /// Floor height.
    pub floor: i16,
    /// Ceiling height above the floor.
    pub ceil_h: i16,
    /// Light level.
    pub light: i16,
    /// Door cells (just outside the floor) with their corridor.
    pub doors: Vec<(i32, i32)>,
    /// Feature names (report).
    pub features: Vec<&'static str>,
    /// Prestige spots (dais tops…), best first.
    pub power: Vec<(i32, i32)>,
}

impl Room {
    /// Centre in cell units.
    #[must_use]
    pub fn center(&self) -> (f64, f64) {
        (
            f64::from(self.x0) + f64::from(self.w) / 2.0,
            f64::from(self.y0) + f64::from(self.h) / 2.0,
        )
    }

    /// Cell `(x, y)` (grid coordinates) of this room's box.
    #[must_use]
    pub fn part(&self, x: i32, y: i32) -> Part {
        let (i, j) = (x - self.x0, y - self.y0);
        if i < 0 || j < 0 || i >= self.w || j >= self.h {
            return Part::Out;
        }
        match self.shape {
            Shape::Rect => Part::Full,
            Shape::Organic => match &self.mask {
                Some(m) if m.get((j * self.w + i) as usize).copied().unwrap_or(false) => Part::Full,
                Some(_) => Part::Out,
                None => Part::Full,
            },
            Shape::Cross(kx, ky) => {
                let edge_x = i < kx || i >= self.w - kx;
                let edge_y = j < ky || j >= self.h - ky;
                if edge_x && edge_y {
                    Part::Out
                } else {
                    Part::Full
                }
            }
            Shape::Octagon(k) => {
                let (ii, jj) = (self.w - 1 - i, self.h - 1 - j);
                let corners = [
                    (i + j, Diag::Backslash, false),
                    (ii + j, Diag::Slash, false),
                    (i + jj, Diag::Slash, true),
                    (ii + jj, Diag::Backslash, true),
                ];
                for (d, diag, floor_a) in corners {
                    if d < k - 1 {
                        return Part::Out;
                    }
                    if d == k - 1 {
                        return Part::Split(diag, floor_a);
                    }
                }
                Part::Full
            }
        }
    }

    /// Is `(x, y)` room floor (whole cell)?
    #[must_use]
    pub fn full(&self, x: i32, y: i32) -> bool {
        self.part(x, y) == Part::Full
    }

    /// Grows the box by `m` and tests `(x, y)`.
    #[must_use]
    pub fn near(&self, x: i32, y: i32, m: i32) -> bool {
        x >= self.x0 - m && y >= self.y0 - m && x < self.x0 + self.w + m && y < self.y0 + self.h + m
    }

    /// Floor cells count (approx area).
    #[must_use]
    pub fn area(&self) -> i32 {
        self.w * self.h
    }
}

/// A corridor between two rooms.
#[derive(Clone, Debug)]
pub struct Corridor {
    /// Room at the start.
    pub a: usize,
    /// Room at the end.
    pub b: usize,
    /// Centre line, from `a`'s door to `b`'s door.
    pub path: Vec<(i32, i32)>,
    /// Brush radius (width = 2r+1 cells).
    pub r: i32,
    /// Extra edge closing a loop (not in the spanning tree).
    pub is_loop: bool,
}

impl Corridor {
    /// Path indices where a stair step may go: straight runs, at least a
    /// brush width away from turns and from both ends.
    #[must_use]
    pub fn stair_slots(&self) -> Vec<usize> {
        let n = self.path.len();
        let dir = |k: usize| {
            (
                self.path[k + 1].0 - self.path[k].0,
                self.path[k + 1].1 - self.path[k].1,
            )
        };
        let pad = self.r as usize + 1;
        (2..n.saturating_sub(2))
            .filter(|&i| {
                let lo = i.saturating_sub(pad);
                let hi = (i + pad).min(n - 2);
                (lo..=hi).all(|k| dir(k) == dir(i.min(n - 2)))
            })
            .collect()
    }
}

/// Rooms and corridors.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// Rooms; index 0 is the hub (largest).
    pub rooms: Vec<Room>,
    /// Corridors.
    pub corridors: Vec<Corridor>,
}

/// Scatter rooms (rejection sampling with spacing for corridors).
pub fn place_rooms(rng: &mut Rng, gw: i32, gh: i32, opts: &Options, palettes: usize) -> Vec<Room> {
    let target = (((gw * gh) as f64 / 240.0).round() as i32).clamp(5, 42) as usize;
    let gap = 6;
    let mut rooms: Vec<Room> = Vec::new();
    let big = 1 + usize::from(gw > 90) + usize::from(gw > 130);
    for attempt in 0..4000 {
        if rooms.len() >= target {
            break;
        }
        let arena = rooms.len() < big && attempt < 800;
        let (w, h) = if arena {
            let s = rng.range(18, 24);
            (s + rng.range(-2, 4), s + rng.range(-2, 4))
        } else if rng.chance(0.18) {
            // Long hall.
            let long = rng.range(18, 26);
            let short = rng.range(7, 9);
            if rng.chance(0.5) {
                (long, short)
            } else {
                (short, long)
            }
        } else {
            (rng.range(8, 16), rng.range(8, 16))
        };
        if w + 4 >= gw || h + 4 >= gh {
            continue;
        }
        let x0 = rng.range(2, gw - w - 3);
        let y0 = rng.range(2, gh - h - 3);
        let clash = rooms.iter().any(|r| {
            x0 < r.x0 + r.w + gap
                && r.x0 < x0 + w + gap
                && y0 < r.y0 + r.h + gap
                && r.y0 < y0 + h + gap
        });
        if clash {
            continue;
        }
        let small = w.min(h);
        let shape = match rng.below(10) {
            0..=3 if small >= 9 => Shape::Octagon((small / 4).clamp(2, 5)),
            4..=5 if small >= 11 && w.max(h) < 2 * small => Shape::Cross(w / 3, h / 3),
            _ if arena => Shape::Octagon((small / 4).clamp(3, 6)),
            _ => Shape::Rect,
        };
        let outdoor_p = match opts.outdoor {
            0 => 0.0,
            1 => 0.18,
            _ => 0.4,
        } + if opts.theme == crate::Theme::Ruins {
            0.35
        } else {
            0.0
        };
        rooms.push(Room {
            x0,
            y0,
            w,
            h,
            shape,
            outdoor: rng.chance(outdoor_p),
            biome: Biome::Indoor,
            mask: None,
            pal: 0,
            floor: 0,
            ceil_h: 0,
            light: 160,
            doors: Vec::new(),
            features: Vec::new(),
            power: Vec::new(),
        });
    }
    // Hub first.
    rooms.sort_by_key(|r| Reverse(r.area()));
    // Districts: palette by nearest of a few random seed points.
    let seeds: Vec<(f64, f64, usize)> = (0..palettes.max(1))
        .map(|i| (rng.f64() * f64::from(gw), rng.f64() * f64::from(gh), i))
        .collect();
    // Biome districts: another set of seed points, each with a roll.
    let biome_seeds: Vec<(f64, f64, f64)> = (0..4)
        .map(|_| {
            (
                rng.f64() * f64::from(gw),
                rng.f64() * f64::from(gh),
                rng.f64(),
            )
        })
        .collect();
    for r in &mut rooms {
        let (cx, cy) = r.center();
        let d = biome_seeds
            .iter()
            .min_by(|a, b| {
                let da = (a.0 - cx).powi(2) + (a.1 - cy).powi(2);
                let db = (b.0 - cx).powi(2) + (b.1 - cy).powi(2);
                da.total_cmp(&db)
            })
            .map_or(0.0, |s| s.2);
        // Legacy knob: Indoor mix keeps the old per-room open yards.
        let biome = if opts.biomes == biomes::Biomes::Indoor {
            if r.outdoor {
                Biome::Yard
            } else {
                Biome::Indoor
            }
        } else {
            biomes::choose(opts.biomes, opts.outdoor, r.w, r.h, d, rng)
        };
        r.biome = biome;
        r.outdoor = biome.outdoor();
        if biome.organic() {
            let fill = match biome {
                Biome::Cave => 0.45,
                Biome::Canyon => 0.40,
                _ => 0.36,
            };
            r.shape = Shape::Organic;
            r.mask = Some(biomes::organic_mask(rng, r.w, r.h, fill));
        }
    }
    for r in &mut rooms {
        let (cx, cy) = r.center();
        r.pal = seeds
            .iter()
            .min_by(|a, b| {
                let da = (a.0 - cx).powi(2) + (a.1 - cy).powi(2);
                let db = (b.0 - cx).powi(2) + (b.1 - cy).powi(2);
                da.total_cmp(&db)
            })
            .map_or(0, |s| s.2);
        let area = r.area();
        r.ceil_h = if r.outdoor {
            if area > 300 { 320 } else { 256 }
        } else if area > 300 {
            256
        } else if area > 150 {
            192
        } else {
            160
        };
        if r.biome == Biome::Cave {
            r.ceil_h = 160;
        }
        r.light = if r.outdoor {
            rng.range(184, 208) as i16
        } else if r.biome == Biome::Cave {
            120
        } else {
            (rng.range(9, 11) * 16) as i16
        };
    }
    rooms
}

/// Router state: which cells rooms and corridors own.
struct Router {
    w: i32,
    h: i32,
    /// Room whose box (+1) covers the cell, `-1` = none.
    room_box: Vec<i32>,
    /// Corridor cells grown by one (blocked for later corridors).
    corr_block: Vec<bool>,
}

impl Router {
    fn new(gw: i32, gh: i32, rooms: &[Room]) -> Self {
        let mut room_box = vec![-1; (gw * gh) as usize];
        for (i, r) in rooms.iter().enumerate() {
            for y in r.y0 - 1..r.y0 + r.h + 1 {
                for x in r.x0 - 1..r.x0 + r.w + 1 {
                    if x >= 0 && y >= 0 && x < gw && y < gh {
                        room_box[(y * gw + x) as usize] = i as i32;
                    }
                }
            }
        }
        Self {
            w: gw,
            h: gh,
            room_box,
            corr_block: vec![false; (gw * gh) as usize],
        }
    }

    fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }

    /// Can a brush of radius `r` sit at `(cx, cy)`?
    fn clear(&self, cx: i32, cy: i32, r: i32, ends: [(usize, (i32, i32)); 2]) -> bool {
        for by in cy - r..=cy + r {
            for bx in cx - r..=cx + r {
                if bx < 2 || by < 2 || bx >= self.w - 2 || by >= self.h - 2 {
                    return false;
                }
                let i = self.idx(bx, by);
                let near_end = ends
                    .iter()
                    .any(|&(_, (dx, dy))| (bx - dx).abs().max((by - dy).abs()) <= r);
                let rb = self.room_box[i];
                if rb >= 0 {
                    let mine = ends.iter().any(|&(room, (dx, dy))| {
                        room as i32 == rb && (bx - dx).abs().max((by - dy).abs()) <= r
                    });
                    if !mine {
                        return false;
                    }
                }
                if self.corr_block[i] && !near_end {
                    return false;
                }
            }
        }
        true
    }

    /// A* with a turn penalty (straight corridors read better).
    fn route(
        &self,
        from: (i32, i32),
        to: (i32, i32),
        r: i32,
        ends: [(usize, (i32, i32)); 2],
    ) -> Option<Vec<(i32, i32)>> {
        const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
        let n = (self.w * self.h) as usize;
        let mut best = vec![u32::MAX; n * 4];
        let mut prev: Vec<u32> = vec![u32::MAX; n * 4];
        let mut open = BinaryHeap::new();
        let h = |x: i32, y: i32| ((x - to.0).abs() + (y - to.1).abs()) as u32 * 10;
        if !self.clear(from.0, from.1, r, ends) || !self.clear(to.0, to.1, r, ends) {
            return None;
        }
        for d in 0..4 {
            let s = self.idx(from.0, from.1) * 4 + d;
            best[s] = 0;
            open.push(Reverse((h(from.0, from.1), 0u32, s as u32)));
        }
        let mut expanded = 0;
        while let Some(Reverse((_, g, s))) = open.pop() {
            let s = s as usize;
            if g > best[s] {
                continue;
            }
            expanded += 1;
            if expanded > 400_000 {
                return None;
            }
            let (cell, d) = (s / 4, s % 4);
            let (x, y) = ((cell as i32) % self.w, (cell as i32) / self.w);
            if (x, y) == to {
                let mut path = vec![(x, y)];
                let mut cur = s;
                while prev[cur] != u32::MAX {
                    cur = prev[cur] as usize;
                    let c = (cur / 4) as i32;
                    if path.last() != Some(&(c % self.w, c / self.w)) {
                        path.push((c % self.w, c / self.w));
                    }
                }
                path.reverse();
                return Some(path);
            }
            for (nd, (dx, dy)) in DIRS.iter().enumerate() {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= self.w || ny >= self.h {
                    continue;
                }
                if !self.clear(nx, ny, r, ends) {
                    continue;
                }
                let turn = if nd == d { 0 } else { 25 };
                let ng = g + 10 + turn;
                let ns = self.idx(nx, ny) * 4 + nd;
                if ng < best[ns] {
                    best[ns] = ng;
                    prev[ns] = s as u32;
                    open.push(Reverse((ng + h(nx, ny), ng, ns as u32)));
                }
            }
        }
        None
    }

    fn occupy(&mut self, path: &[(i32, i32)], r: i32) {
        for &(cx, cy) in path {
            for y in cy - r - 1..=cy + r + 1 {
                for x in cx - r - 1..=cx + r + 1 {
                    if x >= 0 && y >= 0 && x < self.w && y < self.h {
                        let i = self.idx(x, y);
                        self.corr_block[i] = true;
                    }
                }
            }
        }
    }
}

/// Door cell on the side of `room` facing `toward`, away from its other doors.
fn pick_door(rng: &mut Rng, room: &Room, toward: (f64, f64), r: i32) -> Option<(i32, i32)> {
    let (cx, cy) = room.center();
    let (dx, dy) = (toward.0 - cx, toward.1 - cy);
    let mut sides = if dx.abs() > dy.abs() {
        vec![if dx > 0.0 { 1 } else { 3 }, if dy > 0.0 { 0 } else { 2 }]
    } else {
        vec![if dy > 0.0 { 0 } else { 2 }, if dx > 0.0 { 1 } else { 3 }]
    };
    sides.push((sides[0] + 2) % 4);
    for side in sides {
        // Candidate door cells (outside) with the inside cell on the edge.
        let mut cands: Vec<(i32, i32)> = Vec::new();
        let (len, lo) = if side % 2 == 0 {
            (room.w, room.x0)
        } else {
            (room.h, room.y0)
        };
        let third = (len / 3).max(r + 1);
        for t in lo + third..lo + len - third {
            let (inside, door) = match side {
                0 => ((t, room.y0 + room.h - 1), (t, room.y0 + room.h)),
                1 => ((room.x0 + room.w - 1, t), (room.x0 + room.w, t)),
                2 => ((t, room.y0), (t, room.y0 - 1)),
                _ => ((room.x0, t), (room.x0 - 1, t)),
            };
            // Whole edge under the brush must be straight floor.
            let ok = (-r..=r).all(|o| {
                let (ix, iy) = if side % 2 == 0 {
                    (inside.0 + o, inside.1)
                } else {
                    (inside.0, inside.1 + o)
                };
                room.full(ix, iy)
            });
            let spaced = room
                .doors
                .iter()
                .all(|&(x, y)| (x - door.0).abs() + (y - door.1).abs() > 2 * r + 4);
            if ok && spaced {
                cands.push(door);
            }
        }
        if let Some(&d) = rng.pick(&cands) {
            return Some(d);
        }
    }
    None
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut c = x;
        while self.0[c] != r {
            let n = self.0[c];
            self.0[c] = r;
            c = n;
        }
        r
    }
}

/// Connect rooms (spanning tree + loops), set heights. `None` = retry.
pub fn connect(
    rng: &mut Rng,
    gw: i32,
    gh: i32,
    rooms: Vec<Room>,
    opts: &Options,
) -> Option<Layout> {
    let mut rooms = rooms;
    let n = rooms.len();
    if n < 2 {
        return None;
    }
    // Candidate edges by gap between boxes.
    let mut edges: Vec<(i32, usize, usize)> = Vec::new();
    for a in 0..n {
        for b in a + 1..n {
            let (ra, rb) = (&rooms[a], &rooms[b]);
            let gx = (rb.x0 - (ra.x0 + ra.w)).max(ra.x0 - (rb.x0 + rb.w)).max(0);
            let gy = (rb.y0 - (ra.y0 + ra.h)).max(ra.y0 - (rb.y0 + rb.h)).max(0);
            let d = gx + gy;
            if d < 48 {
                edges.push((d + rng.range(0, 3), a, b));
            }
        }
    }
    edges.sort_unstable();
    let mut router = Router::new(gw, gh, &rooms);
    let mut dsu = Dsu((0..n).collect());
    let mut corridors: Vec<Corridor> = Vec::new();
    let mut used = vec![false; edges.len()];
    let wide = gw > 100;
    let try_route = |rng: &mut Rng,
                     rooms: &mut Vec<Room>,
                     router: &mut Router,
                     a: usize,
                     b: usize,
                     is_loop: bool|
     -> Option<Corridor> {
        let r = if wide && rng.chance(0.3) { 2 } else { 1 };
        let da = pick_door(rng, &rooms[a], rooms[b].center(), r)?;
        let db = pick_door(rng, &rooms[b], rooms[a].center(), r)?;
        let path = router.route(da, db, r, [(a, da), (b, db)])?;
        if path.len() < 3 || path.len() > 90 {
            return None;
        }
        // A corridor folding back beside itself would put far-apart stair
        // steps next to each other: reject.
        let reach = 2 * r + 1;
        for i in 0..path.len() {
            for j in i + 2 * reach as usize + 1..path.len() {
                let (a, b) = (path[i], path[j]);
                if (a.0 - b.0).abs().max((a.1 - b.1).abs()) <= reach {
                    return None;
                }
            }
        }
        router.occupy(&path, r);
        rooms[a].doors.push(da);
        rooms[b].doors.push(db);
        Some(Corridor {
            a,
            b,
            path,
            r,
            is_loop,
        })
    };
    for (ei, &(_, a, b)) in edges.iter().enumerate() {
        if dsu.find(a) == dsu.find(b) {
            continue;
        }
        if let Some(c) = try_route(rng, &mut rooms, &mut router, a, b, false) {
            let (ra, rb) = (dsu.find(a), dsu.find(b));
            dsu.0[ra] = rb;
            corridors.push(c);
            used[ei] = true;
        }
    }
    let root = dsu.find(0);
    if (0..n).any(|i| dsu.find(i) != root) {
        return None;
    }
    // Heights along the tree, from the hub.
    let options: &[i16] = match opts.verticality {
        0 => &[0],
        1 => &[0, 0, 16, -16, 32, -32],
        _ => &[0, 16, -16, 32, -32, 48, -48, 64, -64],
    };
    let mut set = vec![false; n];
    set[0] = true;
    rooms[0].floor = 0;
    let mut queue = VecDeque::from([0usize]);
    while let Some(u) = queue.pop_front() {
        for c in &corridors {
            let (v, slots) = if c.a == u {
                (c.b, c.stair_slots().len())
            } else if c.b == u {
                (c.a, c.stair_slots().len())
            } else {
                continue;
            };
            if set[v] {
                continue;
            }
            let max = 16 * slots as i32;
            let fits: Vec<i16> = options
                .iter()
                .copied()
                .filter(|d| i32::from(d.abs()) <= max)
                .filter(|d| (-96..=192).contains(&(rooms[u].floor + d)))
                .collect();
            rooms[v].floor = rooms[u].floor + rng.pick(&fits).copied().unwrap_or(0);
            set[v] = true;
            queue.push_back(v);
        }
    }
    // Loops: rooms far apart in the tree but close on the map.
    let hops = |corridors: &[Corridor], a: usize, b: usize| -> usize {
        let mut dist = vec![usize::MAX; n];
        dist[a] = 0;
        let mut q = VecDeque::from([a]);
        while let Some(u) = q.pop_front() {
            for c in corridors {
                let v = if c.a == u {
                    c.b
                } else if c.b == u {
                    c.a
                } else {
                    continue;
                };
                if dist[v] == usize::MAX {
                    dist[v] = dist[u] + 1;
                    q.push_back(v);
                }
            }
        }
        dist[b]
    };
    let loops_wanted = (n as f64 * 0.5).round() as usize;
    let mut loops = 0;
    for (ei, &(_, a, b)) in edges.iter().enumerate() {
        if loops >= loops_wanted {
            break;
        }
        if used[ei] || hops(&corridors, a, b) < 3 {
            continue;
        }
        // Route on a copy: only commit when the climb fits.
        let mut trial_router = Router {
            w: router.w,
            h: router.h,
            room_box: router.room_box.clone(),
            corr_block: router.corr_block.clone(),
        };
        let mut trial_rooms = rooms.clone();
        if let Some(c) = try_route(rng, &mut trial_rooms, &mut trial_router, a, b, true) {
            let dh = i32::from((rooms[a].floor - rooms[b].floor).abs());
            if dh <= 16 * c.stair_slots().len() as i32 {
                router = trial_router;
                rooms = trial_rooms;
                corridors.push(c);
                loops += 1;
            }
        }
    }
    Some(Layout { rooms, corridors })
}

/// Floor height of corridor path index `i`: one 16-unit step per chosen
/// stair slot (the middle ones of [`Corridor::stair_slots`]).
#[must_use]
pub fn corridor_floor(c: &Corridor, rooms: &[Room], i: usize) -> i16 {
    let (h1, h2) = (rooms[c.a].floor, rooms[c.b].floor);
    let dh = i32::from(h2 - h1);
    let m = (dh.abs() / 16) as usize;
    if m == 0 {
        return h1;
    }
    let slots = c.stair_slots();
    let start = slots.len().saturating_sub(m) / 2;
    let chosen = &slots[start.min(slots.len())..(start + m).min(slots.len())];
    let k = chosen.iter().filter(|&&s| s <= i).count() as i32;
    (i32::from(h1) + dh.signum() * 16 * k) as i16
}

/// Paint rooms and corridors into the grid.
pub fn paint(grid: &mut Grid, layout: &Layout, pals: &[Palette], rng: &mut Rng) {
    for (ri, r) in layout.rooms.iter().enumerate() {
        let p = &pals[r.pal];
        let (floor_pic, ceil_pic, wall, skirt) = match r.biome {
            Biome::Indoor => (
                p.floor.clone(),
                p.ceil.clone(),
                p.wall.clone(),
                p.skirt.clone(),
            ),
            Biome::Cave => (
                p.cave_floor.clone(),
                p.cave_ceil.clone(),
                p.cave_wall.clone(),
                p.cave_wall.clone(),
            ),
            Biome::Yard | Biome::Canyon | Biome::Swamp => (
                p.sky_floor.clone(),
                "F_SKY1".into(),
                p.sky_wall.clone(),
                p.sky_wall.clone(),
            ),
            Biome::Courtyard => (
                p.paving.clone(),
                "F_SKY1".into(),
                p.wall.clone(),
                p.skirt.clone(),
            ),
            Biome::Plaza => (
                p.paving.clone(),
                "F_SKY1".into(),
                p.building.clone(),
                p.skirt.clone(),
            ),
        };
        let props = SecProps {
            floor: r.floor,
            ceil: r.floor + r.ceil_h,
            floor_pic,
            ceil_pic,
            light: r.light,
            special: 0,
            tag: 0,
            wall,
            skirt,
            teleport_to: 0,
            door: false,
        };
        let sec = grid.sector(props);
        for y in r.y0..r.y0 + r.h {
            for x in r.x0..r.x0 + r.w {
                let part = r.part(x, y);
                let Some(c) = grid.at(x, y) else { continue };
                match part {
                    Part::Out => continue,
                    Part::Full => {
                        c.a = Area::Floor(sec);
                        c.split = None;
                    }
                    Part::Split(d, floor_a) => {
                        let (a, b) = if floor_a {
                            (Area::Floor(sec), Area::VOID)
                        } else {
                            (Area::VOID, Area::Floor(sec))
                        };
                        c.a = a;
                        c.split = Some((d, b));
                    }
                }
                c.owner = Owner::Room(ri as u16);
            }
        }
    }
    for (ci, c) in layout.corridors.iter().enumerate() {
        let p = &pals[layout.rooms[c.a].pal];
        let ch: i16 = if c.r >= 2 { 160 } else { 128 };
        let light = (rng.range(7, 9) * 16) as i16;
        let special = if rng.chance(0.12) {
            *rng.pick(&[1i16, 2, 3, 8]).unwrap_or(&0)
        } else {
            0
        };
        for (i, &(cx, cy)) in c.path.iter().enumerate() {
            let floor = corridor_floor(c, &layout.rooms, i);
            let props = SecProps {
                floor,
                ceil: floor + ch,
                floor_pic: p.floor_alt.clone(),
                ceil_pic: p.ceil.clone(),
                light,
                special,
                tag: 0,
                wall: p.trim.clone(),
                skirt: p.step.clone(),
                teleport_to: 0,
                door: false,
            };
            let sec = grid.sector(props);
            for y in cy - c.r..=cy + c.r {
                for x in cx - c.r..=cx + c.r {
                    let Some(cell) = grid.at(x, y) else { continue };
                    if matches!(cell.owner, Owner::Room(_)) {
                        continue;
                    }
                    cell.a = Area::Floor(sec);
                    cell.split = None;
                    cell.owner = Owner::Corridor(ci as u16);
                }
            }
        }
    }
}
