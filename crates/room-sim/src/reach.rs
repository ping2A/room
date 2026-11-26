//! Which sectors a player can both reach and leave, starting from the
//! level's main area (`docs/28` battle royale spot checks).
//!
//! A directed graph over sectors, from the level as loaded:
//! - **Open lines** (two-sided, not blocking): a player crosses from A to B
//!   when the step up is at most 24 and the opening fits a player (56).
//!   Dropping down is always allowed.
//! - **Doors** (unlocked `Door_*` specials, by tag or the line's back
//!   sector) count as open: their ceiling rises. A door opened from
//!   elsewhere (switch, walk-over line) counts only when that line repeats.
//! - **Lifts** (`Plat_*` / `Generic_Lift`) on a repeatable line connect
//!   both ways with every neighbour.
//! - Walls that lower, bridges that rise and stairs that build once a
//!   switch is pressed do **not** count: in a match nobody may press it,
//!   and the area behind stays cut off.
//! - **Teleporters** (`Teleport`, `Teleport_NoFog`) lead from the line's
//!   sector to the sector of the destination.
//!
//! The main area is the largest group of sectors that all reach each other
//! and hold a player start (deathmatch or co-op): a spot there is never a
//! closet, a cage, a pit, a secret behind a locked door or an outdoor area
//! behind a switch, and every spot reaches every other from the first tic.

use crate::world::World;

/// Doom max step up.
const STEP: f64 = 24.0;
/// Player height.
const HEIGHT: f64 = 56.0;

const DOOR_CLOSE: i16 = 10;
const DOOR_OPEN: i16 = 11;
const DOOR_RAISE: i16 = 12;
const DOOR_LOCKED_RAISE: i16 = 13;
const DOOR_ANIMATED: i16 = 14;
const GENERIC_DOOR: i16 = 202;
const DOOR_CLOSE_WAIT_OPEN: i16 = 249;
const TELEPORT: i16 = 70;
const TELEPORT_NO_FOG: i16 = 71;
const GENERIC_LIFT: i16 = 203;
/// `Plat_*` specials (perpetual, down/up wait, by value, …).
const PLATS: [i16; 9] = [60, 62, 63, 64, 65, 172, 206, 207, 228];

/// `true` per sector in the main area (computed once per level).
#[must_use]
pub fn main_area(world: &World) -> &[bool] {
    world.reach_area.get_or_init(|| compute(world))
}

/// Is `(x, y)` in the main area? Points outside every sector are not.
#[must_use]
pub fn point_in_main_area(world: &World, x: f64, y: f64) -> bool {
    let Some(s) = crate::physics::sector_at_xy(world, x, y) else {
        return false;
    };
    main_area(world).get(s).copied().unwrap_or(false)
}

fn compute(world: &World) -> Vec<bool> {
    let map = &world.map;
    let n = map.sectors.len();
    if n == 0 {
        return Vec::new();
    }
    let tagged = |tag: i32, out: &mut Vec<bool>| {
        if tag == 0 {
            return;
        }
        for (i, s) in map.sectors.iter().enumerate() {
            if s.tags.contains(&tag) {
                out[i] = true;
            }
        }
    };
    // Doors that open (no key) and lifts.
    let mut door = vec![false; n];
    let mut lift = vec![false; n];
    for l in &map.lines {
        let back = l.back_sector.map(|s| s.raw() as usize);
        let repeat = (l.flags & crate::world::ML_REPEAT_SPECIAL as i16) != 0;
        // A door opened from elsewhere only once (a switch or a walk-over
        // line) may never open in a match: only doors you push, or that
        // reopen, count.
        let door_ok = |tag: i32| tag == 0 || repeat;
        match l.special {
            DOOR_CLOSE | DOOR_OPEN | DOOR_RAISE | DOOR_ANIMATED | DOOR_CLOSE_WAIT_OPEN => {
                if l.lock_number != 0 || !door_ok(l.args[0]) {
                    continue;
                }
                if l.args[0] == 0 {
                    if let Some(b) = back {
                        door[b] = true;
                    }
                } else {
                    tagged(l.args[0], &mut door);
                }
            }
            DOOR_LOCKED_RAISE | GENERIC_DOOR => {
                let lock = if l.special == GENERIC_DOOR {
                    l.args[4]
                } else {
                    l.args[3]
                };
                if lock != 0 || l.lock_number != 0 || !door_ok(l.args[0]) {
                    continue;
                }
                if l.args[0] == 0 {
                    if let Some(b) = back {
                        door[b] = true;
                    }
                } else {
                    tagged(l.args[0], &mut door);
                }
            }
            s if s == GENERIC_LIFT || PLATS.contains(&s) => {
                if !repeat {
                    continue;
                }
                if l.args[0] == 0 {
                    if let Some(b) = back {
                        lift[b] = true;
                    }
                } else {
                    tagged(l.args[0], &mut lift);
                }
            }
            _ => {}
        }
    }
    let floor = |i: usize| map.sectors[i].floor_height;
    let ceil = |i: usize| map.sectors[i].ceiling_height;
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut link = |a: usize, b: usize| {
        if a != b && !edges[a].contains(&b) {
            edges[a].push(b);
        }
    };
    for l in &map.lines {
        let (Some(fa), Some(fb)) = (l.front_sector, l.back_sector) else {
            continue;
        };
        let (a, b) = (fa.raw() as usize, fb.raw() as usize);
        if a >= n || b >= n || (l.flags & crate::world::ML_BLOCKING as i16) != 0 {
            continue;
        }
        if lift[a] || lift[b] {
            link(a, b);
            link(b, a);
            continue;
        }
        let opening_ok = door[a] || door[b] || {
            let top = ceil(a).min(ceil(b));
            let bottom = floor(a).max(floor(b));
            top - bottom >= HEIGHT
        };
        if !opening_ok {
            continue;
        }
        if floor(b) - floor(a) <= STEP {
            link(a, b);
        }
        if floor(a) - floor(b) <= STEP {
            link(b, a);
        }
    }
    // Teleporters: line sector → destination sector.
    let dest_sectors: Vec<(i32, usize)> = map
        .things
        .iter()
        .filter(|t| t.type_id == 14)
        .filter_map(|t| {
            let s = crate::physics::sector_at_xy(world, f64::from(t.x), f64::from(t.y))?;
            Some((i32::from(t.tid), s))
        })
        .collect();
    for l in &map.lines {
        if l.special != TELEPORT && l.special != TELEPORT_NO_FOG {
            continue;
        }
        let Some(from) = l.front_sector.map(|s| s.raw() as usize) else {
            continue;
        };
        let (tid, tag) = (l.args[0], l.args[1]);
        for &(thing_tid, s) in &dest_sectors {
            let by_tag = tag != 0 && map.sectors[s].tags.contains(&tag);
            let by_tid = tag == 0 && tid != 0 && thing_tid == tid;
            if (by_tag || by_tid) && from < n {
                link(from, s);
            }
        }
    }
    // The main area: the largest group of sectors that all reach each other
    // and hold a player start (deathmatch or co-op), else the largest group.
    let comp = strongly_connected(&edges);
    let mut size = vec![0usize; n];
    for s in 0..n {
        size[comp[s]] += 1;
    }
    let mut has_start = vec![false; n];
    for t in &map.things {
        if matches!(t.type_id, 1..=4 | 11)
            && let Some(s) = crate::physics::sector_at_xy(world, f64::from(t.x), f64::from(t.y))
        {
            has_start[comp[s]] = true;
        }
    }
    let any_start = has_start.iter().any(|b| *b);
    let best = (0..n)
        .filter(|&c| !any_start || has_start[c])
        .max_by_key(|&c| (size[c], usize::MAX - c))
        .unwrap_or(0);
    (0..n).map(|s| comp[s] == best).collect()
}

/// Kosaraju: component id per node (iterative, O(V + E)).
fn strongly_connected(edges: &[Vec<usize>]) -> Vec<usize> {
    let n = edges.len();
    let mut order = Vec::with_capacity(n);
    let mut seen = vec![false; n];
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some((v, i)) = stack.pop() {
            if let Some(&w) = edges[v].get(i) {
                stack.push((v, i + 1));
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                order.push(v);
            }
        }
    }
    let mut reverse: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (a, outs) in edges.iter().enumerate() {
        for &b in outs {
            reverse[b].push(a);
        }
    }
    let mut comp = vec![usize::MAX; n];
    for &root in order.iter().rev() {
        if comp[root] != usize::MAX {
            continue;
        }
        comp[root] = root;
        let mut stack = vec![root];
        while let Some(v) = stack.pop() {
            for &w in &reverse[v] {
                if comp[w] == usize::MAX {
                    comp[w] = root;
                    stack.push(w);
                }
            }
        }
    }
    comp
}
