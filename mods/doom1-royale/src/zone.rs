//! Moving zone circles, schedule, damage, and direction helpers.

use crate::host::Host;
use crate::tables::{zone_damage_for_phase, GRACE_SECS, TICRATE, ZONE_ROWS};

pub fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    dx * dx + dy * dy
}

pub fn xy(s: [f64; 3]) -> [f64; 2] {
    [s[0], s[1]]
}

/// Compass word for the direction from `from` to `to` (Doom: +x east, +y north).
pub fn compass<H: Host>(host: &H, from: [f64; 2], to: [f64; 2]) -> &'static str {
    const DIRS: [&str; 8] = ["E", "NE", "N", "NW", "W", "SW", "S", "SE"];
    let deg = host
        .atan2(to[1] - from[1], to[0] - from[0])
        .to_degrees();
    let octant = ((deg + 360.0 + 22.5) / 45.0) as usize % 8;
    DIRS[octant]
}

pub fn target_radius(r0: f64, row: &crate::tables::ZoneRow) -> f64 {
    r0 * row.radius_frac
}

/// Pick a centre for the next circle fully inside the current one.
pub fn pick_next_center<H: Host>(
    host: &mut H,
    center: [f64; 2],
    radius: f64,
    next_radius: f64,
    spots: &[[f64; 3]],
    nspots: usize,
) -> [f64; 2] {
    let max_off = (radius - next_radius).max(0.0);
    if max_off <= 0.0 {
        return center;
    }
    let max_off2 = max_off * max_off;
    if nspots == 0 {
        return jitter_center(host, center, max_off);
    }
    let mut candidates = [0usize; 64];
    let mut nc = 0usize;
    for (i, spot) in spots.iter().take(nspots).enumerate() {
        let p = xy(*spot);
        if dist2(p, center) <= max_off2 && nc < candidates.len() {
            candidates[nc] = i;
            nc += 1;
        }
    }
    if nc == 0 {
        return jitter_center(host, center, max_off);
    }
    let pick = host.random(0, nc as i32 - 1) as usize;
    xy(spots[candidates[pick]])
}

fn jitter_center<H: Host>(host: &mut H, center: [f64; 2], max_off: f64) -> [f64; 2] {
    if max_off <= 0.0 {
        return center;
    }
    let dx = host.random(-1000, 1000) as f64 / 1000.0 * max_off;
    let dy = host.random(-1000, 1000) as f64 / 1000.0 * max_off;
    let d2 = dx * dx + dy * dy;
    let max2 = max_off * max_off;
    if d2 > max2 && d2 > 0.0 {
        let s = host.sqrt(max2 / d2);
        [center[0] + dx * s, center[1] + dy * s]
    } else {
        [center[0] + dx, center[1] + dy]
    }
}

/// Where we are in the schedule after loot start (`t_rel` in tics).
pub fn schedule_segment(t_rel: i32) -> Option<(usize, bool, i32, i32)> {
    let grace = GRACE_SECS * TICRATE;
    if t_rel < grace {
        return None;
    }
    let mut t = t_rel - grace;
    for (i, row) in ZONE_ROWS.iter().enumerate() {
        let hold = row.hold_secs * TICRATE;
        let shrink = row.shrink_secs * TICRATE;
        if t < hold {
            return Some((i, false, t, i as i32 + 1));
        }
        t -= hold;
        if t < shrink {
            return Some((i, true, t, i as i32 + 1));
        }
        t -= shrink;
    }
    let last = ZONE_ROWS.len() - 1;
    Some((last, true, t, last as i32 + 1))
}

/// Current zone geometry and damage phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneGeom {
    pub center: [f64; 2],
    pub radius: f64,
    pub phase: i32,
    pub shrinking: bool,
}

impl ZoneGeom {
    pub fn grace(center: [f64; 2], r0: f64) -> Self {
        Self {
            center,
            radius: r0,
            phase: 0,
            shrinking: false,
        }
    }
}

/// Moving zone state (persisted on the match).
pub struct ZoneRuntime {
    pub geom: ZoneGeom,
    hold_center: [f64; 2],
    hold_radius: f64,
    shrink_to_center: [f64; 2],
    shrink_to_radius: f64,
    prepared_row: i8,
}

impl ZoneRuntime {
    pub fn new(center: [f64; 2], r0: f64) -> Self {
        Self {
            geom: ZoneGeom::grace(center, r0),
            hold_center: center,
            hold_radius: r0,
            shrink_to_center: center,
            shrink_to_radius: r0,
            prepared_row: -1,
        }
    }

    pub fn sync<H: Host>(
        &mut self,
        host: &mut H,
        r0: f64,
        initial_center: [f64; 2],
        spots: &[[f64; 3]],
        nspots: usize,
        t_rel: i32,
    ) {
        let grace = GRACE_SECS * TICRATE;
        if t_rel < grace {
            self.geom = ZoneGeom::grace(initial_center, r0);
            self.hold_center = initial_center;
            self.hold_radius = r0;
            self.prepared_row = -1;
            return;
        }
        let Some((row_i, shrinking, into, phase)) = schedule_segment(t_rel) else {
            self.geom = ZoneGeom::grace(initial_center, r0);
            return;
        };
        let row = &ZONE_ROWS[row_i];
        let shrink_tics = row.shrink_secs * TICRATE;
        let to_r = target_radius(r0, row);

        if self.prepared_row != row_i as i8 {
            self.hold_center = if row_i == 0 {
                initial_center
            } else {
                self.shrink_to_center
            };
            self.hold_radius = if row_i == 0 {
                r0
            } else {
                self.shrink_to_radius
            };
            self.shrink_to_center = pick_next_center(
                host,
                self.hold_center,
                self.hold_radius,
                to_r,
                spots,
                nspots,
            );
            self.shrink_to_radius = to_r;
            self.prepared_row = row_i as i8;
        }

        let (center, radius) = if !shrinking {
            (self.hold_center, self.hold_radius)
        } else {
            let f = if shrink_tics <= 0 {
                1.0
            } else {
                f64::from(into) / f64::from(shrink_tics)
            };
            (
                [
                    self.hold_center[0]
                        + (self.shrink_to_center[0] - self.hold_center[0]) * f,
                    self.hold_center[1]
                        + (self.shrink_to_center[1] - self.hold_center[1]) * f,
                ],
                self.hold_radius + (self.shrink_to_radius - self.hold_radius) * f,
            )
        };

        self.geom = ZoneGeom {
            center,
            radius: radius.max(0.0),
            phase,
            shrinking,
        };
    }

    /// Next circle preview (hold target or post-shrink centre).
    #[must_use]
    pub fn next_circle(&self) -> ([f64; 2], f64) {
        (self.shrink_to_center, self.shrink_to_radius.max(0.0))
    }
}

/// Push current + next rings to the engine renderer (BE1).
pub fn publish_render_zone<H: Host>(_host: &mut H, geom: &ZoneGeom, next: ([f64; 2], f64)) {
    use room_script_sdk::game;
    let (nc, nr) = next;
    let rgba = 0xcc3333aa_u32;
    let _ = game::zone_set(
        0,
        geom.center[0] as i32,
        geom.center[1] as i32,
        geom.radius.max(0.0) as i32,
        nc[0] as i32,
        nc[1] as i32,
        nr as i32,
        rgba,
        0,
    );
}

/// Clear the primary zone ring.
pub fn clear_render_zone<H: Host>(_host: &mut H) {
    let _ = room_script_sdk::game::zone_clear(0);
}

/// Zone damage per second in `phase` (1..).
pub fn zone_damage(phase: i32) -> i32 {
    zone_damage_for_phase(phase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;
    use crate::tables::secs_until_first_shrink;

    #[test]
    fn grace_then_phase_one_hold() {
        let grace = GRACE_SECS * TICRATE;
        assert!(schedule_segment(grace - 1).is_none());
        let seg = schedule_segment(grace).unwrap();
        assert_eq!(seg, (0, false, 0, 1));
    }

    #[test]
    fn first_shrink_timing_matches_table() {
        let t = secs_until_first_shrink() * TICRATE;
        let seg = schedule_segment(t).unwrap();
        assert_eq!(seg.0, 0);
        assert!(seg.1);
    }

    #[test]
    fn next_circle_stays_contained_over_seeds() {
        for seed in 0..1000 {
            let mut host = FakeHost::new(seed);
            let r0 = 500.0 + (seed % 500) as f64;
            let center = [1000.0, 1000.0];
            let mut spots = [[0.0; 3]; 256];
            let mut n = 0usize;
            for k in 0..32 {
                spots[n] = [800.0 + (k * 17) as f64, 900.0 + (k * 23) as f64, 0.0];
                n += 1;
            }
            let mut cur_c = center;
            let mut cur_r = r0;
            for row in &ZONE_ROWS {
                let next_r = target_radius(r0, row);
                let next_c = pick_next_center(&mut host, cur_c, cur_r, next_r, &spots, n);
                let off = host.sqrt(dist2(next_c, cur_c));
                assert!(
                    off <= cur_r - next_r + 0.01,
                    "seed {seed}: off {off} max {}",
                    cur_r - next_r
                );
                cur_c = next_c;
                cur_r = next_r;
            }
        }
    }
}
