//! Map loot: a fixed set of weapons, ammo, health and armour placed once
//! per match. Nothing is dropped later (no supply crates, no death drops).
//!
//! 1. On load, every pickup the map had is removed (keys stay) and its spot
//!    is kept: mappers put items where players can walk, so those spots are
//!    reachable. Monster spots and player starts are the fallback.
//! 2. When drop selection starts, [`Royale::plan_loot`] picks the spots
//!    (random, deterministic) so the drop map can show the guns.
//! 3. At deploy, [`Royale::spawn_loot`] spawns the plan.

use room_script_sdk::game::Actor;
use room_script_sdk::ClassRef;

use crate::host::{Host, CHECK_POSITION};
use crate::match_flow::Royale;
use crate::tables::{
    gun_counts, supply_counts, Item, GUN_SPACING, MAX_LOOT, MAX_SPOTS, PICKUP_EDNUMS,
};
use crate::zone::{dist2, xy};

/// Two spots closer than this count as the same spot.
const SAME_SPOT: f64 = 32.0;

impl Royale {
    /// Remember where the map's pickups were, then remove them (keys stay).
    pub(crate) fn clear_map_pickups<H: Host>(&mut self, host: &mut H) {
        let mut buf = [[0.0; 3]; 64];
        for ednum in PICKUP_EDNUMS {
            let n = host.level_things(ednum, &mut buf).min(buf.len());
            for p in &buf[..n] {
                if self.nitem_spots < MAX_SPOTS {
                    self.item_spots[self.nitem_spots] = [p[0], p[1], host.floor_z(p[0], p[1])];
                    self.nitem_spots += 1;
                }
            }
        }
        // Remove whatever stands on those spots: the map item or the class a
        // gameplay mod replaced it with.
        let mut all = [Actor::NULL; 4096];
        let n = host
            .actors_of_class(ClassRef::NULL, false, &mut all)
            .min(all.len());
        for &a in &all[..n] {
            if host.actor_player(a).is_some() || host.actor_is_monster(a) {
                continue;
            }
            let p = xy(host.actor_pos(a));
            if self.item_spots[..self.nitem_spots]
                .iter()
                .any(|s| dist2(xy(*s), p) < 1.0)
            {
                host.actor_destroy(a);
            }
        }
    }

    /// Candidate loot spots: item spots (shuffled), then the other spots
    /// (shuffled), without duplicates.
    fn loot_candidates<H: Host>(&self, host: &mut H, out: &mut [[f64; 3]; MAX_SPOTS * 2]) -> usize {
        let mut n = 0usize;
        let push = |out: &mut [[f64; 3]; MAX_SPOTS * 2], n: &mut usize, s: [f64; 3]| {
            let dup = out[..*n]
                .iter()
                .any(|o| dist2(xy(*o), xy(s)) < SAME_SPOT * SAME_SPOT);
            if !dup && *n < out.len() {
                out[*n] = s;
                *n += 1;
            }
        };
        for s in &self.item_spots[..self.nitem_spots] {
            push(out, &mut n, *s);
        }
        let first_group = n;
        // Deathmatch starts stay free for landings.
        for s in &self.spots[..self.nspots] {
            push(out, &mut n, *s);
        }
        shuffle(host, &mut out[..first_group]);
        shuffle(host, &mut out[first_group..n]);
        n
    }

    /// Choose where every piece of loot goes.
    pub(crate) fn plan_loot<H: Host>(&mut self, host: &mut H) {
        self.nloot = 0;
        let mut cand = [[0.0; 3]; MAX_SPOTS * 2];
        let all = self.loot_candidates(host, &mut cand);
        let players = self.contenders();
        // Leave a landing spot per player (and some slack) free of loot.
        let n = all - (players.max(2) * 2).min(all / 2);
        let mut used = [false; MAX_SPOTS * 2];
        let inner2 = (self.r0 * 0.5) * (self.r0 * 0.5);
        let spacing2 = GUN_SPACING * GUN_SPACING;
        let mut guns = [[0.0; 2]; MAX_LOOT];
        let mut nguns = 0usize;
        for (gun, count) in gun_counts(players) {
            for _ in 0..count {
                // Rocket launchers near the zone centre; every gun spaced out
                // when the map has room, else anywhere free.
                let near_center = gun == Item::RocketLauncher;
                let pick = (0..3).find_map(|pass| {
                    (0..n).find(|&i| {
                        if used[i] {
                            return false;
                        }
                        let p = xy(cand[i]);
                        let centre_ok =
                            !near_center || pass > 0 || dist2(p, self.center) <= inner2;
                        let spaced = pass > 1
                            || guns[..nguns].iter().all(|&g| dist2(g, p) >= spacing2);
                        centre_ok && spaced
                    })
                });
                let Some(i) = pick else { break };
                used[i] = true;
                guns[nguns] = xy(cand[i]);
                nguns += 1;
                self.push_loot(gun, cand[i]);
            }
        }
        // Supplies round-robin, so a small map still gets some of each.
        let mut left = supply_counts(players);
        let mut next = 0usize;
        'rounds: while left.iter().any(|&(_, c)| c > 0) {
            for slot in &mut left {
                if slot.1 == 0 {
                    continue;
                }
                while next < n && used[next] {
                    next += 1;
                }
                if next >= n {
                    break 'rounds;
                }
                used[next] = true;
                self.push_loot(slot.0, cand[next]);
                slot.1 -= 1;
            }
        }
    }

    fn push_loot(&mut self, item: Item, at: [f64; 3]) {
        if self.nloot < MAX_LOOT {
            self.loot[self.nloot] = (item, at);
            self.nloot += 1;
        }
    }

    /// Guns in the plan (for the drop map).
    pub(crate) fn planned_guns(&self) -> impl Iterator<Item = (Item, [f64; 3])> + '_ {
        self.loot[..self.nloot]
            .iter()
            .copied()
            .filter(|(item, _)| item.is_gun())
    }

    /// True when `at` holds a planned piece of loot.
    pub(crate) fn is_loot_spot(&self, at: [f64; 2]) -> bool {
        self.loot[..self.nloot]
            .iter()
            .any(|(_, l)| dist2(xy(*l), at) < SAME_SPOT * SAME_SPOT)
    }

    /// Spawn the plan: each gun with its ammo beside it.
    pub(crate) fn spawn_loot<H: Host>(&mut self, host: &mut H) {
        for k in 0..self.nloot {
            let (item, at) = self.loot[k];
            let class = self.items[item as usize];
            if class.is_null() {
                continue;
            }
            let spawned = host.spawn(class, at, 0.0, 0);
            if item.is_gun()
                && let Some(gun) = spawned
                && self.nloot_guns < self.loot_guns.len()
            {
                self.loot_guns[self.nloot_guns] = gun;
                self.nloot_guns += 1;
            }
            if let Some(ammo) = item.ammo() {
                let ammo_class = self.items[ammo as usize];
                let side = [at[0] + 24.0, at[1], at[2]];
                if !ammo_class.is_null()
                    && host.spawn(ammo_class, side, 0.0, CHECK_POSITION).is_none()
                {
                    let _ = host.spawn(ammo_class, at, 0.0, 0);
                }
            }
        }
    }
}

/// Deterministic Fisher–Yates with the sim's random stream.
fn shuffle<H: Host>(host: &mut H, s: &mut [[f64; 3]]) {
    for i in (1..s.len()).rev() {
        let j = host.random(0, i as i32) as usize;
        s.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;

    fn royale_with_spots(n: usize, players: usize) -> Royale {
        let mut r = Royale::default();
        for i in 0..n {
            r.item_spots[i] = [(i % 10) as f64 * 300.0, (i / 10) as f64 * 300.0, 0.0];
        }
        r.nitem_spots = n;
        for s in 0..players {
            r.contender[s] = true;
        }
        r.center = [1350.0, 1350.0];
        r.r0 = 2000.0;
        r
    }

    #[test]
    fn plan_is_limited_spaced_and_on_distinct_spots() {
        let mut host = FakeHost::new(3);
        let mut r = royale_with_spots(100, 4);
        r.plan_loot(&mut host);
        let guns: Vec<_> = r.planned_guns().collect();
        assert_eq!(guns.len(), 1 + 2 + 4 + 1, "rl + chaingun + shotgun + chainsaw");
        for (i, a) in guns.iter().enumerate() {
            for b in &guns[i + 1..] {
                assert!(dist2(xy(a.1), xy(b.1)) >= GUN_SPACING * GUN_SPACING);
            }
        }
        let spots: Vec<_> = r.loot[..r.nloot].iter().map(|l| xy(l.1)).collect();
        for (i, a) in spots.iter().enumerate() {
            assert!(spots[i + 1..].iter().all(|b| dist2(*a, *b) > 1.0), "one piece per spot");
        }
    }

    #[test]
    fn small_maps_still_get_every_gun() {
        let mut host = FakeHost::new(9);
        let mut r = royale_with_spots(6, 8);
        r.plan_loot(&mut host);
        // 6 spots, 8 players: half the spots stay free for landings.
        assert_eq!(r.nloot, 3);
        assert_eq!(r.planned_guns().count(), r.nloot, "guns come first");
    }
}
