//! Map spot collection, zone centre pick, and spawn selection.

use room_script_sdk::game::thing_type;
use room_script_sdk::ClassRef;

use crate::host::Host;
use crate::match_flow::Royale;
use crate::tables::MAX_SPOTS;
use crate::zone::{dist2, xy};

impl Royale {
    pub(crate) fn push_spot<H: Host>(&mut self, host: &H, x: f64, y: f64) {
        if self.nspots < MAX_SPOTS {
            self.spots[self.nspots] = [x, y, host.floor_z(x, y)];
            self.nspots += 1;
        }
    }

    /// Remove every monster; its spot becomes a loot drop / fallback spawn.
    pub(crate) fn clear_monsters<H: Host>(&mut self, host: &mut H) {
        let mut all = [room_script_sdk::game::Actor::NULL; 4096];
        let n = host
            .actors_of_class(ClassRef::NULL, false, &mut all)
            .min(all.len());
        for &a in &all[..n] {
            if !host.actor_is_monster(a) {
                continue;
            }
            let p = host.actor_pos(a);
            self.push_spot(host, p[0], p[1]);
            host.actor_destroy(a);
        }
    }

    /// Deathmatch starts for spawning; co-op starts become loot spots.
    pub(crate) fn read_starts<H: Host>(&mut self, host: &H) {
        let n = host.level_things(thing_type::DEATHMATCH_START, &mut self.dm);
        self.ndm = n.min(self.dm.len());
        for s in &mut self.dm[..self.ndm] {
            s[2] = host.floor_z(s[0], s[1]);
        }
        for k in 0..4 {
            let mut one = [[0.0; 3]; 1];
            if host.level_things(thing_type::PLAYER1_START + k, &mut one) > 0 {
                self.push_spot(host, one[0][0], one[0][1]);
            }
        }
    }

    /// Zone centre: a random one of the spots nearest the middle of the map.
    pub(crate) fn pick_zone<H: Host>(&mut self, host: &mut H) {
        let Some(first) = host.player(0) else {
            return;
        };
        if self.nspots == 0 {
            let p = host.actor_pos(first);
            self.center = [p[0], p[1]];
            self.r0 = 2048.0;
            return;
        }
        let n = self.nspots as f64;
        let mid = self.spots[..self.nspots]
            .iter()
            .fold([0.0, 0.0], |m, s| [m[0] + s[0] / n, m[1] + s[1] / n]);
        let mut near = [usize::MAX; 4];
        for i in 0..self.nspots {
            let d = dist2(xy(self.spots[i]), mid);
            for k in 0..near.len() {
                let worse = near[k] == usize::MAX || d < dist2(xy(self.spots[near[k]]), mid);
                if worse {
                    near[k..].rotate_right(1);
                    near[k] = i;
                    break;
                }
            }
        }
        let count = near.iter().filter(|&&i| i != usize::MAX).count() as i32;
        let pick = near[host.random(0, count - 1) as usize];
        self.center = xy(self.spots[pick]);
        let far = self.spots[..self.nspots]
            .iter()
            .chain(&self.dm[..self.ndm])
            .map(|s| dist2(xy(*s), self.center))
            .fold(0.0, f64::max);
        self.r0 = host.sqrt(far) + 256.0;
    }

    /// Farthest free spot from the players placed so far (random first pick).
    pub(crate) fn farthest<H: Host>(
        &self,
        host: &mut H,
        pool: &[[f64; 3]],
        taken: &[bool],
    ) -> Option<usize> {
        if pool.is_empty() {
            return None;
        }
        let start = host.random(0, pool.len() as i32 - 1) as usize;
        let mut best = None;
        let mut best_d = -1.0;
        for k in 0..pool.len() {
            let i = (start + k) % pool.len();
            if taken[i] {
                continue;
            }
            if self.nplaced == 0 {
                return Some(i);
            }
            let d = self.placed[..self.nplaced]
                .iter()
                .map(|&p| dist2(p, xy(pool[i])))
                .fold(f64::MAX, f64::min);
            if d > best_d {
                best_d = d;
                best = Some(i);
            }
        }
        best
    }
}
