//! Map loot: a fixed set of weapons, ammo, health and armour placed once
//! per match. Nothing is dropped later (no supply crates, no death drops).
//!
//! 1. On load, every pickup the map had is removed (keys stay) and its spot
//!    is kept: mappers put items where players can walk, so those spots are
//!    reachable. Monster spots and player starts are the fallback.
//! 2. At deploy, [`Royale::plan_loot`] picks the spots (random,
//!    deterministic) and [`Royale::spawn_loot`] spawns them; then the
//!    players are placed on free spots.
//! 3. Guns still on the ground show on every minimap.

use room_script_sdk::game::Actor;
use room_script_sdk::ClassRef;

use crate::host::{Host, CHECK_POSITION};
use crate::match_flow::Royale;
use crate::tables::{
    gun_counts, supply_counts, Item, GUN_SPACING, MAX_LOOT, MAX_SPOTS, PICKUP_EDNUMS,
};
use crate::zone::{dist2, xy};

/// Probe rings around each reachable anchor: distances (map units).
const PROBE_DIST: [f64; 2] = [192.0, 384.0];
/// Highest floor step between an anchor and a probe (Doom max step).
const PROBE_STEP: f64 = 24.0;
/// Room a player needs above the floor.
const PROBE_HEADROOM: f64 = 56.0;
/// Below this many item spots, probes add more.
const WANT_SPOTS: usize = 48;

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

    /// Maps with few things (deathmatch arenas) give too few loot spots:
    /// add points around reachable anchors (player starts, item and monster
    /// spots). A point counts when an item fits there, its floor is within a
    /// step of the anchor's, a player fits under its ceiling, and the anchor
    /// can see it (no wall in between). Deterministic: fixed order, no RNG.
    pub(crate) fn probe_extra_spots<H: Host>(&mut self, host: &mut H) {
        if self.nitem_spots >= WANT_SPOTS {
            return;
        }
        let probe = self.items[Item::Clip as usize];
        if probe.is_null() {
            return;
        }
        let mut anchors = [[0.0; 3]; MAX_SPOTS];
        let mut na = 0usize;
        for s in self.dm[..self.ndm]
            .iter()
            .chain(&self.spots[..self.nspots])
            .chain(&self.item_spots[..self.nitem_spots])
        {
            if na < anchors.len() {
                anchors[na] = *s;
                na += 1;
            }
        }
        let dirs: [(f64, f64); 8] = [
            (1.0, 0.0),
            (0.7071, 0.7071),
            (0.0, 1.0),
            (-0.7071, 0.7071),
            (-1.0, 0.0),
            (-0.7071, -0.7071),
            (0.0, -1.0),
            (0.7071, -0.7071),
        ];
        for a in &anchors[..na] {
            if self.nitem_spots >= MAX_SPOTS {
                break;
            }
            let az = host.floor_z(a[0], a[1]);
            let Some(origin) = host.spawn(probe, [a[0], a[1], az], 0.0, 0) else {
                continue;
            };
            for d in PROBE_DIST {
                for (dx, dy) in dirs {
                    let (x, y) = (a[0] + dx * d, a[1] + dy * d);
                    let z = host.floor_z(x, y);
                    if (z - az).abs() > PROBE_STEP || host.ceiling_z(x, y) - z < PROBE_HEADROOM {
                        continue;
                    }
                    let p = [x, y];
                    let taken = self.item_spots[..self.nitem_spots]
                        .iter()
                        .chain(&self.spots[..self.nspots])
                        .chain(&self.dm[..self.ndm])
                        .any(|s| dist2(xy(*s), p) < 96.0 * 96.0);
                    if taken {
                        continue;
                    }
                    let Some(test) = host.spawn(probe, [x, y, z], 0.0, CHECK_POSITION) else {
                        continue;
                    };
                    let seen = host.actor_can_see(origin, test);
                    host.actor_destroy(test);
                    if seen && self.nitem_spots < MAX_SPOTS {
                        self.item_spots[self.nitem_spots] = [x, y, z];
                        self.nitem_spots += 1;
                    }
                }
            }
            host.actor_destroy(origin);
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
                self.loot_guns[self.nloot_guns] = (gun, item);
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

impl Royale {
    /// Minimap markers for the guns still lying on the map (shared by
    /// every seat; picked-up guns lose theirs).
    pub(crate) fn update_gun_markers<H: Host>(&self, host: &H) {
        use crate::ui::{GUN_MARKER0, GUN_MARKERS};
        use room_script_sdk::hud::{flags, layer, Hud};
        let weapon = host.class_find("Weapon");
        let mut lying = [Actor::NULL; 256];
        let n = if weapon.is_null() {
            0
        } else {
            host.actors_of_class(weapon, true, &mut lying).min(lying.len())
        };
        for k in 0..GUN_MARKERS {
            let id = GUN_MARKER0 + k as u16;
            let gun = (k < self.nloot_guns).then(|| self.loot_guns[k]);
            match gun {
                Some((a, item)) if lying[..n].contains(&a) => {
                    let p = host.actor_pos(a);
                    let _ = Hud::marker(id)
                        .seat(-1)
                        .layer(layer::OVER_HUD)
                        .flags(flags::MAP_ONLY)
                        .world_at(p[0] as i32, p[1] as i32, p[2] as i32)
                        .rgba(gun_color(item))
                        .size(5, 5)
                        .show();
                }
                _ => {
                    let _ = Hud::remove(-1, id);
                }
            }
        }
    }
}

/// Minimap colour per gun.
pub(crate) fn gun_color(item: Item) -> u32 {
    match item {
        Item::Shotgun => 0xffa0_40ff,
        Item::Chaingun => 0x70c0_ffff,
        Item::RocketLauncher => 0xff50_50ff,
        _ => 0xd080_ffff,
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
        let guns: Vec<_> = r.loot[..r.nloot].iter().filter(|l| l.0.is_gun()).collect();
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
        assert!(r.loot[..r.nloot].iter().all(|l| l.0.is_gun()), "guns come first");
    }
}
