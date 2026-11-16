//! Things: deathmatch starts spread by farthest-point sampling, co-op
//! starts in the hub, weapons tiered by room prestige and spread apart,
//! ammo next to its weapon, health / armour / power-ups balanced by player
//! count, bonus trails along corridors, theme decorations in corners.

use crate::Options;
use crate::features::Pad;
use crate::grid::{CELL, Grid, Owner};
use crate::layout::Layout;
use crate::rng::Rng;
use crate::theme::Palette;
use room_map::raw::RawThing;
use std::collections::HashSet;

/// Things placed and what they are for (checks and report).
#[derive(Clone, Debug, Default)]
pub struct Placed {
    /// All things (grid-world coordinates, origin not yet applied).
    pub things: Vec<RawThing>,
    /// Cells everybody must reach (starts, weapons, items).
    pub must_reach: Vec<(i32, i32)>,
    /// Deathmatch starts.
    pub dm_starts: usize,
    /// Weapons.
    pub weapons: usize,
    /// Health, armour and power-ups.
    pub items: usize,
    /// Monsters.
    pub monsters: usize,
}

/// A monster kind: DoomEdNum, sprite prefix, danger tier (1–5), clearance
/// in cells around its spot.
struct Mon {
    ed: i16,
    sprite: &'static str,
    tier: u8,
    clear: i32,
}

const MONSTERS: &[Mon] = &[
    Mon {
        ed: 3004,
        sprite: "POSS",
        tier: 1,
        clear: 1,
    },
    Mon {
        ed: 9,
        sprite: "SPOS",
        tier: 1,
        clear: 1,
    },
    Mon {
        ed: 3001,
        sprite: "TROO",
        tier: 1,
        clear: 1,
    },
    Mon {
        ed: 3002,
        sprite: "SARG",
        tier: 2,
        clear: 1,
    },
    Mon {
        ed: 58,
        sprite: "SARG",
        tier: 2,
        clear: 1,
    },
    Mon {
        ed: 65,
        sprite: "CPOS",
        tier: 2,
        clear: 1,
    },
    Mon {
        ed: 3006,
        sprite: "SKUL",
        tier: 2,
        clear: 1,
    },
    Mon {
        ed: 3005,
        sprite: "HEAD",
        tier: 3,
        clear: 1,
    },
    Mon {
        ed: 69,
        sprite: "BOS2",
        tier: 3,
        clear: 1,
    },
    Mon {
        ed: 66,
        sprite: "SKEL",
        tier: 3,
        clear: 1,
    },
    Mon {
        ed: 71,
        sprite: "PAIN",
        tier: 4,
        clear: 1,
    },
    Mon {
        ed: 67,
        sprite: "FATT",
        tier: 4,
        clear: 2,
    },
    Mon {
        ed: 68,
        sprite: "BSPI",
        tier: 4,
        clear: 2,
    },
    Mon {
        ed: 3003,
        sprite: "BOSS",
        tier: 4,
        clear: 1,
    },
    Mon {
        ed: 64,
        sprite: "VILE",
        tier: 5,
        clear: 1,
    },
    Mon {
        ed: 16,
        sprite: "CYBR",
        tier: 5,
        clear: 2,
    },
];

/// Monster spot: safe floor with `clear` cells around it at most 16 up or
/// down (monsters stand on rough ground too) and room for their height.
fn roomy(grid: &Grid, p: (i32, i32), clear: i32) -> bool {
    let Some(here) = grid.props(p.0, p.1) else {
        return false;
    };
    if here.special != 0 && here.special != 8 || here.teleport_to != 0 {
        return false;
    }
    let f = here.floor;
    (-clear..=clear).all(|dy| {
        (-clear..=clear).all(|dx| {
            grid.props(p.0 + dx, p.1 + dy).is_some_and(|q| {
                (q.floor - f).abs() <= 16 && q.ceil - q.floor >= 72 && q.teleport_to == 0
            }) && !grid.get(p.0 + dx, p.1 + dy).busy
        })
    })
}

/// Corridor hops from the hub (room 0) to every room.
fn hops(layout: &Layout) -> Vec<u32> {
    let n = layout.rooms.len();
    let mut d = vec![u32::MAX; n];
    d[0] = 0;
    let mut q = std::collections::VecDeque::from([0usize]);
    while let Some(u) = q.pop_front() {
        for c in &layout.corridors {
            let v = if c.a == u {
                c.b
            } else if c.b == u {
                c.a
            } else {
                continue;
            };
            if d[v] == u32::MAX {
                d[v] = d[u] + 1;
                q.push_back(v);
            }
        }
    }
    d
}

fn thing(x: i32, y: i32, type_id: i16, angle: i16) -> RawThing {
    RawThing {
        x: (x * CELL + CELL / 2) as i16,
        y: (y * CELL + CELL / 2) as i16,
        angle,
        type_id,
        flags: 7,
        ..RawThing::default()
    }
}

/// Good standing spot: whole floor, safe, roomy, same height all around.
fn spot(grid: &Grid, x: i32, y: i32) -> bool {
    let Some(p) = grid.props(x, y) else {
        return false;
    };
    if (p.special != 0 && p.special != 8)
        || p.teleport_to != 0
        || p.ceil - p.floor < 72
        || grid.get(x, y).busy
    {
        return false;
    }
    for dy in -1..=1 {
        for dx in -1..=1 {
            let Some(q) = grid.props(x + dx, y + dy) else {
                return false;
            };
            if q.floor != p.floor || q.teleport_to != 0 || grid.get(x + dx, y + dy).busy {
                return false;
            }
        }
    }
    true
}

fn angle_to(from: (i32, i32), to: (f64, f64)) -> i16 {
    let a = (to.1 - f64::from(from.1))
        .atan2(to.0 - f64::from(from.0))
        .to_degrees();
    ((a / 45.0).round() * 45.0).rem_euclid(360.0) as i16
}

fn d2(a: (i32, i32), b: (i32, i32)) -> i64 {
    let (dx, dy) = (i64::from(a.0 - b.0), i64::from(a.1 - b.1));
    dx * dx + dy * dy
}

/// Place everything.
pub fn place(
    grid: &Grid,
    layout: &Layout,
    pals: &[Palette],
    pads: &[(Pad, Pad)],
    opts: &Options,
    rng: &mut Rng,
    has_sprite: &dyn Fn(&str) -> bool,
) -> Placed {
    let mut out = Placed::default();
    let mut used: HashSet<(i32, i32)> = HashSet::new();
    let players = i32::from(opts.players.max(1));
    let room_of = |x: i32, y: i32| match grid.get(x, y).owner {
        Owner::Room(r) => Some(r as usize),
        _ => None,
    };
    // Spots per room.
    let mut spots: Vec<Vec<(i32, i32)>> = vec![Vec::new(); layout.rooms.len()];
    for y in 0..grid.h {
        for x in 0..grid.w {
            if let Some(r) = room_of(x, y)
                && spot(grid, x, y)
                && layout.rooms[r]
                    .doors
                    .iter()
                    .all(|&(dx, dy)| (dx - x).abs() + (dy - y).abs() > 2)
            {
                spots[r].push((x, y));
            }
        }
    }
    let all: Vec<(i32, i32)> = spots.iter().flatten().copied().collect();
    if all.is_empty() {
        return out;
    }
    let free = |used: &HashSet<(i32, i32)>, p: (i32, i32)| -> bool {
        (-1..=1).all(|dy| (-1..=1).all(|dx| !used.contains(&(p.0 + dx, p.1 + dy))))
    };

    // Co-op / single-player starts in the hub, around its centre.
    let hub = &layout.rooms[0];
    let hc = hub.center();
    let mut hub_spots = spots[0].clone();
    hub_spots.sort_by_key(|&p| d2(p, (hc.0 as i32, hc.1 as i32)));
    let mut coop = 0;
    for &p in &hub_spots {
        if coop >= 4 {
            break;
        }
        if free(&used, p) {
            used.insert(p);
            out.things
                .push(thing(p.0, p.1, (1 + coop) as i16, angle_to(p, hc)));
            out.must_reach.push(p);
            coop += 1;
        }
    }

    // Deathmatch starts: farthest-point sampling, never two on a cell.
    let want = (players * 2).clamp(8, 48) as usize;
    let mut starts: Vec<(i32, i32)> = Vec::new();
    let first = *rng.pick(&all).unwrap_or(&all[0]);
    starts.push(first);
    while starts.len() < want {
        let next = all
            .iter()
            .filter(|&&p| free(&used, p) && !starts.contains(&p))
            .max_by_key(|&&p| starts.iter().map(|&s| d2(p, s)).min().unwrap_or(0));
        match next {
            Some(&p) => starts.push(p),
            None => break,
        }
    }
    for &p in &starts {
        used.insert(p);
        let r = room_of(p.0, p.1).unwrap_or(0);
        out.things
            .push(thing(p.0, p.1, 11, angle_to(p, layout.rooms[r].center())));
        out.must_reach.push(p);
    }
    out.dm_starts = starts.len();

    // Room prestige: set pieces first, then size.
    let mut by_prestige: Vec<usize> = (0..layout.rooms.len())
        .filter(|&r| !spots[r].is_empty())
        .collect();
    by_prestige.sort_by_key(|&r| {
        (
            std::cmp::Reverse(layout.rooms[r].power.len()),
            std::cmp::Reverse(layout.rooms[r].area()),
        )
    });

    let place_near = |out: &mut Placed,
                      used: &mut HashSet<(i32, i32)>,
                      room: usize,
                      at: (i32, i32),
                      type_id: i16,
                      min: i64,
                      max: i64|
     -> Option<(i32, i32)> {
        let mut cands: Vec<(i32, i32)> = spots[room]
            .iter()
            .copied()
            .filter(|&p| !used.contains(&p) && (min..=max).contains(&d2(p, at)))
            .collect();
        cands.sort_by_key(|&p| d2(p, at));
        let p = *cands.first()?;
        used.insert(p);
        out.things.push(thing(p.0, p.1, type_id, 0));
        out.must_reach.push(p);
        Some(p)
    };
    let centre_spot = |room: usize, used: &HashSet<(i32, i32)>| -> Option<(i32, i32)> {
        let r = &layout.rooms[room];
        if let Some(&p) = r.power.first()
            && grid.props(p.0, p.1).is_some()
            && !used.contains(&p)
        {
            return Some(p);
        }
        let c = r.center();
        spots[room]
            .iter()
            .copied()
            .filter(|p| !used.contains(p))
            .min_by_key(|&p| d2(p, (c.0 as i32, c.1 as i32)))
    };

    // Weapons: heavy ones in prestige rooms far apart; light ones elsewhere.
    let heavy_types: [i16; 3] = [2003, 2004, 82];
    let light_types: [i16; 4] = [2001, 2002, 82, 2005];
    let heavy_n = (1 + players / 4).min(6) as usize;
    let mut weapon_rooms: Vec<usize> = Vec::new();
    for &r in &by_prestige {
        if weapon_rooms.len() >= heavy_n {
            break;
        }
        let c = layout.rooms[r].center();
        let far = weapon_rooms.iter().all(|&o| {
            let oc = layout.rooms[o].center();
            (c.0 - oc.0).hypot(c.1 - oc.1) > 18.0
        });
        if far {
            weapon_rooms.push(r);
        }
    }
    let ammo_of = |w: i16| -> [i16; 2] {
        match w {
            2001 | 82 => [2008, 2049],
            2002 => [2007, 2048],
            2003 => [2010, 2046],
            2004 | 2006 => [2047, 17],
            _ => [0, 0],
        }
    };
    let mut placed_rooms: Vec<usize> = Vec::new();
    for (i, &r) in weapon_rooms.iter().enumerate() {
        let w = heavy_types[i % heavy_types.len()];
        if let Some(p) = centre_spot(r, &used) {
            used.insert(p);
            out.things.push(thing(p.0, p.1, w, 0));
            out.must_reach.push(p);
            out.weapons += 1;
            for a in ammo_of(w) {
                if a != 0 {
                    place_near(&mut out, &mut used, r, p, a, 4, 40);
                }
            }
            placed_rooms.push(r);
        }
    }
    let mut rest: Vec<usize> = (0..layout.rooms.len())
        .filter(|r| !placed_rooms.contains(r) && !spots[*r].is_empty())
        .collect();
    rng.shuffle(&mut rest);
    let light_n = (layout.rooms.len() * 2 / 3).max(2);
    for (light_i, &r) in rest.iter().take(light_n).enumerate() {
        let w = light_types[light_i % light_types.len()];
        if let Some(p) = centre_spot(r, &used) {
            used.insert(p);
            out.things.push(thing(p.0, p.1, w, 0));
            out.must_reach.push(p);
            out.weapons += 1;
            if let Some(&a) = ammo_of(w).first().filter(|&&a| a != 0) {
                place_near(&mut out, &mut used, r, p, a, 4, 30);
            }
            placed_rooms.push(r);
        }
    }

    // Power items: one prize in the remotest room of the hub, BFG on big maps.
    let remote = (0..layout.rooms.len())
        .filter(|&r| !spots[r].is_empty())
        .max_by(|&a, &b| {
            let (ac, bc) = (layout.rooms[a].center(), layout.rooms[b].center());
            (ac.0 - hc.0)
                .hypot(ac.1 - hc.1)
                .total_cmp(&(bc.0 - hc.0).hypot(bc.1 - hc.1))
        });
    let item =
        |out: &mut Placed, used: &mut HashSet<(i32, i32)>, rng: &mut Rng, room: usize, t: i16| {
            let mut cands: Vec<(i32, i32)> = spots[room]
                .iter()
                .copied()
                .filter(|p| free(used, *p))
                .collect();
            if cands.is_empty() {
                cands = spots[room]
                    .iter()
                    .copied()
                    .filter(|p| !used.contains(p))
                    .collect();
            }
            let pick = if cands.len() > 2 {
                cands[rng.below(cands.len() as u32) as usize]
            } else {
                *cands.first()?
            };
            used.insert(pick);
            out.things.push(thing(pick.0, pick.1, t, 0));
            out.must_reach.push(pick);
            out.items += 1;
            Some(pick)
        };
    if let Some(r) = remote {
        let prize = if opts.power_items {
            if players >= 8 { 83 } else { 2013 }
        } else {
            2019
        };
        item(&mut out, &mut used, rng, r, prize);
        if opts.power_items
            && players >= 8
            && let Some(p) = item(&mut out, &mut used, rng, r, 2006)
        {
            out.weapons += 1;
            out.items -= 1;
            place_near(&mut out, &mut used, r, p, 2047, 4, 40);
        }
    }
    // Armour.
    let mut rooms_cycle: Vec<usize> = (0..layout.rooms.len())
        .filter(|&r| !spots[r].is_empty())
        .collect();
    rng.shuffle(&mut rooms_cycle);
    let mut cyc = rooms_cycle.iter().copied().cycle();
    let green = (1 + players / 6) as usize;
    for _ in 0..green {
        if let Some(r) = cyc.next() {
            item(&mut out, &mut used, rng, r, 2018);
        }
    }
    if players >= 4
        && let Some(r) = cyc.next()
    {
        item(&mut out, &mut used, rng, r, 2019);
    }
    // Health: medikits and stimpacks across rooms.
    let medis = (layout.rooms.len() / 2).max(2);
    for _ in 0..medis {
        if let Some(r) = cyc.next() {
            item(&mut out, &mut used, rng, r, 2012);
        }
    }
    for _ in 0..layout.rooms.len() / 3 {
        if let Some(r) = cyc.next() {
            item(&mut out, &mut used, rng, r, 2011);
        }
    }
    if let Some(r) = cyc.next() {
        item(&mut out, &mut used, rng, r, 8);
    }

    // Bonus trails down the middle of some corridors.
    for c in &layout.corridors {
        if c.path.len() < 9 || !rng.chance(0.55) {
            continue;
        }
        let t: i16 = if rng.chance(0.5) { 2014 } else { 2015 };
        let mid = c.path.len() / 2;
        for k in [mid - 3, mid - 1, mid + 1, mid + 3] {
            let p = c.path[k];
            let ok = grid.props(p.0, p.1).is_some_and(|q| q.special == 0) && !used.contains(&p);
            if ok {
                used.insert(p);
                out.things.push(thing(p.0, p.1, t, 0));
            }
        }
    }

    // Monsters (solo / co-op): none in the hub; danger grows with distance
    // from it and with the episode; a boss pack guards the exit room.
    if opts.monsters > 0 {
        let pool: Vec<&Mon> = MONSTERS.iter().filter(|m| has_sprite(m.sprite)).collect();
        let dist = hops(layout);
        let far = dist
            .iter()
            .copied()
            .filter(|&d| d != u32::MAX)
            .max()
            .unwrap_or(1)
            .max(1);
        let exit_room = (1..layout.rooms.len())
            .max_by(|&a, &b| {
                let (ac, bc) = (layout.rooms[a].center(), layout.rooms[b].center());
                (ac.0 - hc.0)
                    .hypot(ac.1 - hc.1)
                    .total_cmp(&(bc.0 - hc.0).hypot(bc.1 - hc.1))
            })
            .unwrap_or(0);
        let density = [0.0, 0.55, 1.0, 1.7][opts.monsters.min(3) as usize];
        let boost = i32::from(opts.progress.min(4)) + i32::from(opts.monsters >= 3);
        let place_mon = |out: &mut Placed,
                         used: &mut HashSet<(i32, i32)>,
                         rng: &mut Rng,
                         room: usize,
                         m: &Mon,
                         flags: i16| {
            let c = layout.rooms[room].center();
            let r = &layout.rooms[room];
            let mut cands: Vec<(i32, i32)> = (r.y0..r.y0 + r.h)
                .flat_map(|y| (r.x0..r.x0 + r.w).map(move |x| (x, y)))
                .filter(|&(x, y)| grid.get(x, y).owner == Owner::Room(room as u16))
                .filter(|&p| {
                    r.doors
                        .iter()
                        .all(|&(dx, dy)| (dx - p.0).abs() + (dy - p.1).abs() > 3)
                })
                .filter(|&p| free(used, p) && roomy(grid, p, m.clear))
                .collect();
            rng.shuffle(&mut cands);
            let p = *cands.first()?;
            for dy in -m.clear..=m.clear {
                for dx in -m.clear..=m.clear {
                    used.insert((p.0 + dx, p.1 + dy));
                }
            }
            let mut t = thing(p.0, p.1, m.ed, angle_to(p, c));
            t.flags = flags;
            out.things.push(t);
            out.monsters += 1;
            Some(())
        };
        for room in 1..layout.rooms.len() {
            if dist[room] == u32::MAX {
                continue;
            }
            // Tier ceiling: 1 near the start, up to 4 at the far end (+ boosts).
            let depth = f64::from(dist[room]) / f64::from(far);
            let top = ((1.0 + depth * 3.0).round() as i32 + boost).clamp(1, 5) as u8;
            let choices: Vec<&&Mon> = pool.iter().filter(|m| m.tier <= top.min(4)).collect();
            if choices.is_empty() {
                continue;
            }
            let count = ((f64::from(layout.rooms[room].area()) / 45.0) * density)
                .round()
                .max(1.0) as usize;
            for k in 0..count {
                // Weight toward the room's top tier, but keep fodder around.
                let pick = if rng.chance(0.35) {
                    choices
                        .iter()
                        .filter(|m| m.tier + 1 >= top.min(4))
                        .copied()
                        .collect::<Vec<_>>()
                } else {
                    choices
                        .iter()
                        .filter(|m| m.tier <= 2)
                        .copied()
                        .collect::<Vec<_>>()
                };
                let Some(m) = rng.pick(&pick).or_else(|| rng.pick(&choices)).copied() else {
                    break;
                };
                // Most on every skill, more on medium, more again on hard.
                let frac = k as f64 / count as f64;
                let mut flags: i16 = if frac < 0.6 {
                    7
                } else if frac < 0.85 {
                    6
                } else {
                    4
                };
                if rng.chance(0.3) {
                    flags |= 8; // ambush: waits until it sees you
                }
                if place_mon(&mut out, &mut used, rng, room, m, flags).is_none() {
                    break;
                }
            }
            // Supplies to match.
            let ammo = *rng
                .pick(&[2008i16, 2007, 2049, 2010, 2048, 2047])
                .unwrap_or(&2008);
            let c = layout.rooms[room].center();
            place_near(
                &mut out,
                &mut used,
                room,
                (c.0 as i32, c.1 as i32),
                ammo,
                0,
                i64::MAX,
            );
            if rng.chance(0.6) {
                place_near(
                    &mut out,
                    &mut used,
                    room,
                    (c.0 as i32, c.1 as i32),
                    2011,
                    0,
                    i64::MAX,
                );
            }
        }
        // Boss pack in the exit room.
        let bosses: Vec<&&Mon> = pool
            .iter()
            .filter(|m| m.tier >= 4 && (m.tier < 5 || boost >= 2 || opts.monsters >= 3))
            .collect();
        let n_boss = 1 + usize::from(opts.monsters >= 2) + usize::from(boost >= 3);
        for _ in 0..n_boss {
            if let Some(m) = rng.pick(&bosses).copied() {
                place_mon(&mut out, &mut used, rng, exit_room, m, 7);
            }
        }
        if exit_room != 0 {
            let c = layout.rooms[exit_room].center();
            place_near(
                &mut out,
                &mut used,
                exit_room,
                (c.0 as i32, c.1 as i32),
                2012,
                0,
                i64::MAX,
            );
            place_near(
                &mut out,
                &mut used,
                exit_room,
                (c.0 as i32, c.1 as i32),
                2046,
                0,
                i64::MAX,
            );
        }
    }

    // Teleport destinations at pad centres (corner between the 4 cells).
    for (a, b) in pads {
        for pad in [a, b] {
            out.things.push(RawThing {
                x: ((pad.x + 1) * CELL) as i16,
                y: ((pad.y + 1) * CELL) as i16,
                angle: pad.angle,
                type_id: 14,
                flags: 7,
                ..RawThing::default()
            });
        }
    }

    // Decorations: corner lamps / torches indoors, trees outdoors.
    for (ri, room) in layout.rooms.iter().enumerate() {
        let pal = &pals[room.pal];
        let mut corners: Vec<(i32, i32)> = Vec::new();
        for y in room.y0..room.y0 + room.h {
            for x in room.x0..room.x0 + room.w {
                let c = grid.get(x, y);
                if c.owner != Owner::Room(ri as u16)
                    || c.full_floor().is_none()
                    || c.busy
                    || used.contains(&(x, y))
                {
                    continue;
                }
                let solid = |dx: i32, dy: i32| {
                    grid.get(x + dx, y + dy).a.sector().is_none()
                        || grid.get(x + dx, y + dy).split.is_some()
                };
                let corner = (solid(-1, 0) || solid(1, 0)) && (solid(0, -1) || solid(0, 1));
                let far = room
                    .doors
                    .iter()
                    .all(|&(dx, dy)| (dx - x).abs() + (dy - y).abs() > 3);
                if corner && far && grid.props(x, y).is_some_and(|p| p.special == 0) {
                    corners.push((x, y));
                }
            }
        }
        use crate::biomes::Biome;
        let n = match room.biome {
            Biome::Cave | Biome::Canyon | Biome::Swamp => (room.area() / 50).clamp(2, 7) as usize,
            _ => 4,
        };
        rng.shuffle(&mut corners);
        for &p in corners.iter().take(n) {
            let t = match room.biome {
                Biome::Cave => *rng.pick(&[47i16, 47, pal.lamp]).unwrap_or(&47),
                Biome::Canyon | Biome::Yard => *rng.pick(&[54i16, 43, 54, 47]).unwrap_or(&54),
                Biome::Swamp => *rng.pick(&[43i16, 43, 54]).unwrap_or(&43),
                Biome::Plaza => *rng.pick(&[2028i16, pal.lamp, 2035]).unwrap_or(&2028),
                Biome::Courtyard | Biome::Indoor => pal.lamp,
            };
            used.insert(p);
            out.things.push(thing(p.0, p.1, t, 0));
        }
        if !room.outdoor && rng.chance(0.4) {
            for &p in corners.iter().skip(n).take(2) {
                used.insert(p);
                out.things.push(thing(p.0, p.1, pal.small_decor, 0));
            }
        }
    }
    out
}

/// One prize per secret closet.
pub fn secret_prizes(
    out: &mut Placed,
    closets: &[(i32, i32)],
    opts: &Options,
    rng: &mut Rng,
    has_sprite: &dyn Fn(&str) -> bool,
) {
    for &(x, y) in closets {
        let mut prizes: Vec<i16> = vec![2013, 8, 2019, 2023];
        if has_sprite("MEGA") {
            prizes.push(83);
        }
        if opts.monsters >= 2 {
            prizes.push(2022);
        }
        let t = *rng.pick(&prizes).unwrap_or(&2013);
        out.things.push(thing(x, y, t, 0));
        out.items += 1;
    }
}
