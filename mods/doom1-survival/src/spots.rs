//! Spawn-spot graph: collection, selection, learning, stuck (`docs/27` §6.1).

use crate::host::{Host, IS_MISSILE};
use crate::tables::{
    MAX_SPAWN_DIST, MAX_SPOTS, MIN_SPAWN_DIST, SPOT_DISABLE_FAILS, SPOT_DISABLE_SCORE, SPOT_EDNUMS,
    SPOT_GRID, SPOT_MIN_HEIGHT, STUCK_MOVE_EPS, STUCK_STILL_TICS,
};
use crate::team::Team;
use room_script_sdk::game::Actor;
use room_script_sdk::ClassRef;

/// One designer / prop spawn candidate.
#[derive(Clone, Copy, Debug)]
pub struct Spot {
    /// Map X.
    pub x: f64,
    /// Map Y.
    pub y: f64,
    /// Floor Z.
    pub z: f64,
    /// Learned quality (higher = better).
    pub score: i16,
    /// Successful spawn uses.
    pub uses: u16,
    /// Stuck / failed spawn count.
    pub fails: u16,
    /// Maptime of last use (`-1` = never).
    pub last_used: i32,
}

impl Default for Spot {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            score: 0,
            uses: 0,
            fails: 0,
            last_used: -1,
        }
    }
}

impl Spot {
    /// World position.
    #[must_use]
    pub fn pos(&self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }

    /// Disabled for the rest of the map.
    #[must_use]
    pub fn disabled(&self) -> bool {
        self.fails >= SPOT_DISABLE_FAILS && self.score < SPOT_DISABLE_SCORE
    }
}

/// Director-tracked monster for stuck / learning.
#[derive(Clone, Copy, Debug)]
pub struct Tracked {
    /// Live actor.
    pub actor: Actor,
    /// Spot index that spawned them (`u16::MAX` = unknown).
    pub spot_idx: u16,
    /// Spawn maptime (straggler / diagnostics).
    #[allow(dead_code)]
    pub spawn_tic: i32,
    /// Best (smallest) distance to any alive player since the last progress.
    pub best_dist: f64,
    /// Maptime when [`Self::best_dist`] last improved by ≥ [`STUCK_MOVE_EPS`].
    pub progress_tic: i32,
    /// Threat cost refunded on stuck recycle.
    pub threat_cost: i32,
}

impl Default for Tracked {
    fn default() -> Self {
        Self {
            actor: Actor::NULL,
            spot_idx: u16::MAX,
            spawn_tic: 0,
            best_dist: f64::MAX,
            progress_tic: 0,
            threat_cost: 0,
        }
    }
}

/// Collect spots from `level_things` (preferred) then loose props.
pub fn collect_spots(host: &impl Host, spots: &mut [Spot; MAX_SPOTS]) -> usize {
    let mut n = 0usize;
    let mut tmp = [[0.0f64; 3]; 64];
    for &ed in SPOT_EDNUMS {
        let got = host.level_things(ed, &mut tmp);
        for i in 0..got {
            if n == MAX_SPOTS {
                return n;
            }
            let p = tmp[i];
            if !accept_point(host, spots, n, p) {
                continue;
            }
            spots[n] = Spot {
                x: p[0],
                y: p[1],
                z: host.floor_z(p[0], p[1]),
                ..Spot::default()
            };
            n += 1;
        }
    }
    // Lower-priority: non-player non-missile actors (legacy SV0 behaviour).
    let mut all = [Actor::NULL; 512];
    let na = host
        .actors_of_class(ClassRef::NULL, false, &mut all)
        .min(all.len());
    for &a in &all[..na] {
        if n == MAX_SPOTS {
            break;
        }
        if host.actor_player(a).is_some() || host.actor_get_i32(a, IS_MISSILE) != 0 {
            continue;
        }
        let p = host.actor_pos(a);
        let pz = [p[0], p[1], host.floor_z(p[0], p[1])];
        if !accept_point(host, spots, n, pz) {
            continue;
        }
        spots[n] = Spot {
            x: pz[0],
            y: pz[1],
            z: pz[2],
            ..Spot::default()
        };
        n += 1;
    }
    n
}

fn accept_point(host: &impl Host, spots: &[Spot], n: usize, p: [f64; 3]) -> bool {
    let floor = host.floor_z(p[0], p[1]);
    let ceil = host.ceiling_z(p[0], p[1]);
    if ceil - floor < SPOT_MIN_HEIGHT {
        return false;
    }
    let gx = (p[0] / SPOT_GRID) as i32;
    let gy = (p[1] / SPOT_GRID) as i32;
    for s in spots.iter().take(n) {
        let sx = (s.x / SPOT_GRID) as i32;
        let sy = (s.y / SPOT_GRID) as i32;
        if sx == gx && sy == gy {
            return false;
        }
    }
    true
}

/// Pick a spawn spot relative to the alive team. Returns `(pos, spot_idx)`.
#[must_use]
pub fn pick_spot(
    host: &mut impl Host,
    spots: &[Spot],
    nspots: usize,
    team: &Team,
    tries: usize,
) -> Option<([f64; 3], usize)> {
    if nspots == 0 || !team.any_alive(host) {
        return None;
    }
    let mut max_dist = MAX_SPAWN_DIST;
    for attempt in 0..tries {
        if attempt == 4 {
            max_dist *= 1.5;
        }
        let idx = host.random(0, nspots as i32 - 1) as usize;
        let s = &spots[idx];
        if s.disabled() {
            continue;
        }
        if !team_distance_ok(host, team, s.pos(), MIN_SPAWN_DIST, max_dist) {
            continue;
        }
        return Some((s.pos(), idx));
    }
    None
}

fn team_distance_ok(
    host: &impl Host,
    team: &Team,
    pos: [f64; 3],
    min_dist: f64,
    max_dist: f64,
) -> bool {
    let mut nearest = f64::MAX;
    let mut any = false;
    for a in team.iter_alive(host) {
        any = true;
        let p = host.actor_pos(a);
        let (dx, dy) = (pos[0] - p[0], pos[1] - p[1]);
        let d = host.sqrt(dx * dx + dy * dy);
        if d < min_dist {
            return false;
        }
        if d < nearest {
            nearest = d;
        }
    }
    any && nearest <= max_dist
}

/// Facing angle (degrees) from spot toward a player.
#[must_use]
pub fn face_player(host: &impl Host, spot: [f64; 3], player_pos: [f64; 3]) -> f64 {
    let (dx, dy) = (spot[0] - player_pos[0], spot[1] - player_pos[1]);
    host.atan2(-dy, -dx).to_degrees()
}

/// Mark a spot as proven (monster reached / damaged a player).
pub fn mark_proven(spots: &mut [Spot], idx: u16) {
    if let Some(s) = spots.get_mut(idx as usize) {
        s.score = s.score.saturating_add(2);
    }
}

/// Record a stuck failure on a spot.
pub fn mark_stuck(spots: &mut [Spot], idx: u16) {
    if let Some(s) = spots.get_mut(idx as usize) {
        s.fails = s.fails.saturating_add(1);
        s.score = s.score.saturating_sub(8);
    }
}

/// Update progress tracking; returns `true` if the monster is stuck.
///
/// Stuck = has a living target, cannot engage (≤768 + LOS), and has not closed
/// distance to the team by ≥ [`STUCK_MOVE_EPS`] for [`STUCK_STILL_TICS`].
/// Roaming inside a sealed closet does not count as progress.
pub fn update_stuck(host: &impl Host, t: &mut Tracked, team: &Team, now: i32) -> bool {
    if !host.actor_exists(t.actor) || host.actor_is_dead(t.actor) {
        return false;
    }
    let Some(target) = host.actor_target(t.actor) else {
        t.progress_tic = now;
        return false;
    };
    if host.actor_is_dead(target) {
        t.progress_tic = now;
        return false;
    }
    let pos = host.actor_pos(t.actor);
    let mut nearest = f64::MAX;
    let mut engageable = false;
    for p in team.iter_alive(host) {
        let pp = host.actor_pos(p);
        let (dx, dy) = (pos[0] - pp[0], pos[1] - pp[1]);
        let d = host.sqrt(dx * dx + dy * dy);
        if d < nearest {
            nearest = d;
        }
        if d <= 768.0 && host.actor_can_see(t.actor, p) {
            engageable = true;
        }
    }
    if engageable {
        t.best_dist = nearest;
        t.progress_tic = now;
        return false;
    }
    // Outside engage range: only meaningful progress is closing toward 768.
    // Pacing inside a sealed closet must not reset the stuck clock.
    if nearest > 768.0 {
        if t.best_dist == f64::MAX {
            t.best_dist = nearest;
            // keep progress_tic
        }
        return now - t.progress_tic >= STUCK_STILL_TICS;
    }
    if nearest <= t.best_dist - STUCK_MOVE_EPS {
        t.best_dist = nearest;
        t.progress_tic = now;
        return false;
    }
    if t.best_dist == f64::MAX {
        t.best_dist = nearest;
        t.progress_tic = now;
        return false;
    }
    now - t.progress_tic >= STUCK_STILL_TICS
}

/// Best proven spot near the team for straggler teleport (`score > 0`).
#[must_use]
pub fn proven_near_team(
    host: &impl Host,
    spots: &[Spot],
    nspots: usize,
    team: &Team,
) -> Option<[f64; 3]> {
    let mut best: Option<(usize, f64)> = None;
    for (i, s) in spots.iter().take(nspots).enumerate() {
        if s.disabled() || s.score <= 0 {
            continue;
        }
        if !team_distance_ok(host, team, s.pos(), 64.0, MAX_SPAWN_DIST) {
            continue;
        }
        // Prefer closer to the first alive player.
        let Some(p) = team.first_alive(host) else {
            return None;
        };
        let pp = host.actor_pos(p);
        let (dx, dy) = (s.x - pp[0], s.y - pp[1]);
        let d = host.sqrt(dx * dx + dy * dy);
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| spots[i].pos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;

    #[test]
    fn pick_spot_rejects_near_all_players() {
        let mut host = FakeHost::new(3, 1);
        host.add_player(0, [0.0, 0.0, 0.0]);
        host.add_player(1, [2000.0, 0.0, 0.0]);
        let mut spots = [Spot::default(); MAX_SPOTS];
        spots[0] = Spot {
            x: 100.0,
            y: 0.0,
            ..Spot::default()
        }; // near p0
        spots[1] = Spot {
            x: 1000.0,
            y: 0.0,
            ..Spot::default()
        }; // mid — ok for both if min 384
        let mut team = Team::default();
        team.refresh(&host);
        let mut found_mid = false;
        for seed in 1..80 {
            host = FakeHost::new(3, seed);
            host.add_player(0, [0.0, 0.0, 0.0]);
            host.add_player(1, [2000.0, 0.0, 0.0]);
            team.refresh(&host);
            if let Some((p, _)) = pick_spot(&mut host, &spots, 2, &team, 8) {
                assert!(p[0] > 500.0, "must not pick the near-p0 spot");
                found_mid = true;
                break;
            }
        }
        assert!(found_mid);
    }

    #[test]
    fn collect_skips_players_and_keeps_props() {
        let mut host = FakeHost::new(3, 1);
        host.add_player(0, [0.0, 0.0, 0.0]);
        host.add_prop([500.0, 500.0, 0.0]);
        host.add_prop([800.0, 100.0, 0.0]);
        let mut spots = [Spot::default(); MAX_SPOTS];
        let n = collect_spots(&host, &mut spots);
        assert_eq!(n, 2);
        assert_eq!(spots[0].pos(), [500.0, 500.0, 0.0]);
        assert_eq!(spots[1].pos(), [800.0, 100.0, 0.0]);
    }

    #[test]
    fn stuck_after_stillness_without_los() {
        let mut host = FakeHost::new(3, 1);
        host.add_player(0, [0.0, 0.0, 0.0]);
        let m = host.add_monster([1000.0, 1000.0, 0.0]);
        host.actor_set_ref(m, crate::host::TARGET_PTR, host.player(0));
        host.set_can_see(false);
        let mut team = Team::default();
        team.refresh(&host);
        let mut t = Tracked {
            actor: m,
            spot_idx: 0,
            spawn_tic: 0,
            best_dist: 1414.0,
            progress_tic: 0,
            threat_cost: 2,
        };
        assert!(!update_stuck(&host, &mut t, &team, STUCK_STILL_TICS - 1));
        assert!(update_stuck(&host, &mut t, &team, STUCK_STILL_TICS));
    }

    #[test]
    fn stuck_when_far_even_if_los_leaks() {
        let mut host = FakeHost::new(3, 1);
        host.add_player(0, [0.0, 0.0, 0.0]);
        let m = host.add_monster([2000.0, 0.0, 0.0]);
        host.actor_set_ref(m, crate::host::TARGET_PTR, host.player(0));
        host.set_can_see(true); // void LOS leak
        let mut team = Team::default();
        team.refresh(&host);
        let mut t = Tracked {
            actor: m,
            spot_idx: 0,
            spawn_tic: 0,
            best_dist: 2000.0,
            progress_tic: 0,
            threat_cost: 2,
        };
        assert!(update_stuck(&host, &mut t, &team, STUCK_STILL_TICS));
    }

    #[test]
    fn mark_stuck_disables_bad_spots() {
        let mut spots = [Spot::default(); 1];
        for _ in 0..SPOT_DISABLE_FAILS {
            mark_stuck(&mut spots, 0);
        }
        // score -24, fails 3
        assert!(spots[0].disabled());
    }
}
