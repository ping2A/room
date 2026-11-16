//! Playability check: from the first start, every start, weapon and item
//! must be reachable *and* every one must lead back (steps up ≤ 24, drops
//! any height, 56 units of headroom, teleporters one-way). A map that
//! fails is regenerated.

use crate::features::Pad;
use crate::grid::Grid;
use std::collections::VecDeque;

const STEP: i16 = 24;
const HEADROOM: i16 = 56;

fn walk(grid: &Grid, x: i32, y: i32) -> Option<(i16, i16)> {
    let p = grid.props(x, y)?;
    (p.ceil - p.floor >= HEADROOM).then_some((p.floor, p.ceil))
}

fn flood(grid: &Grid, start: (i32, i32), pads: &[(Pad, Pad)], reverse: bool) -> Vec<bool> {
    let idx = |x: i32, y: i32| (y * grid.w + x) as usize;
    let mut seen = vec![false; (grid.w * grid.h) as usize];
    let mut q = VecDeque::new();
    if walk(grid, start.0, start.1).is_some() {
        seen[idx(start.0, start.1)] = true;
        q.push_back(start);
    }
    // Pad cell → partner pad's cells.
    let teleport = |x: i32, y: i32| -> Option<(i32, i32)> {
        for (a, b) in pads {
            for (from, to) in [(a, b), (b, a)] {
                let (src, dst) = if reverse { (to, from) } else { (from, to) };
                if (src.x..src.x + 2).contains(&x) && (src.y..src.y + 2).contains(&y) {
                    return Some((dst.x, dst.y));
                }
            }
        }
        None
    };
    while let Some((x, y)) = q.pop_front() {
        let Some((f, c)) = walk(grid, x, y) else {
            continue;
        };
        let mut next: Vec<(i32, i32)> = Vec::new();
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            let Some((nf, nc)) = walk(grid, nx, ny) else {
                continue;
            };
            let (lo_f, hi_c) = (f.max(nf), c.min(nc));
            if hi_c - lo_f < HEADROOM {
                continue;
            }
            // Forward: may step up 24 and drop anything. Reverse: the opposite.
            let rise = if reverse { f - nf } else { nf - f };
            if rise <= STEP {
                next.push((nx, ny));
            }
        }
        if let Some(t) = teleport(x, y) {
            next.push(t);
        }
        for (nx, ny) in next {
            if grid.inside(nx, ny) && !seen[idx(nx, ny)] {
                seen[idx(nx, ny)] = true;
                q.push_back((nx, ny));
            }
        }
    }
    seen
}

/// Every cell of `must` reachable from `must[0]` and back.
#[must_use]
pub fn playable(grid: &Grid, must: &[(i32, i32)], pads: &[(Pad, Pad)]) -> bool {
    let Some(&start) = must.first() else {
        return false;
    };
    let fwd = flood(grid, start, pads, false);
    let back = flood(grid, start, pads, true);
    let idx = |(x, y): (i32, i32)| (y * grid.w + x) as usize;
    must.iter()
        .all(|&p| grid.inside(p.0, p.1) && fwd[idx(p)] && back[idx(p)])
}
