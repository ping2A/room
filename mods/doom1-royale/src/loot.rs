//! Loot placement: guns with ammo, tiered tables, spacing caps.

use room_script_sdk::game::Actor;

use crate::host::{Host, CHECK_POSITION};
use crate::match_flow::Royale;
use crate::rarity::{Rarity, RarityTable};
use crate::tables::{
    chaingun_cap, pick, rocket_cap, tier, total, Item, GUN_SPACING, ITEM_CLASSES, LOOT_CLEARANCE,
    MAX_SPOTS,
};
use crate::zone::{dist2, xy};

/// Spawn helpers used by supply / death loot (`extras`, `death`).
pub trait LootSpawner {
    fn item_class(&self, item: Item) -> room_script_sdk::ClassRef;
    fn spawn_gun<H: Host>(&self, host: &mut H, gun: Item, at: [f64; 3]);
    fn spawn_item<H: Host>(&self, host: &mut H, item: Item, rarity: Rarity, at: [f64; 3]);
    fn spawn_fog<H: Host>(&self, host: &mut H, at: [f64; 3]);
}

/// Copyable spawn tables (avoids aliasing `Royale` with `LootExtras`).
#[derive(Clone, Copy)]
pub struct LootCtx {
    pub items: [room_script_sdk::ClassRef; ITEM_CLASSES.len()],
    pub fog: room_script_sdk::ClassRef,
    pub rarity: RarityTable,
}

impl LootSpawner for LootCtx {
    fn item_class(&self, item: Item) -> room_script_sdk::ClassRef {
        self.items[item as usize]
    }

    fn spawn_gun<H: Host>(&self, host: &mut H, gun: Item, at: [f64; 3]) {
        Self::spawn_gun_inner(self, host, gun, at);
    }

    fn spawn_item<H: Host>(&self, host: &mut H, item: Item, rarity: Rarity, at: [f64; 3]) {
        Self::spawn_item_rarity(self, host, item, rarity, at);
    }

    fn spawn_fog<H: Host>(&self, host: &mut H, at: [f64; 3]) {
        if let Some(fog) = host.spawn(self.fog, at, 0.0, 0) {
            host.actor_sound(fog, "misc/teleport", 1.0);
        }
    }
}

impl LootCtx {
    pub fn from_royale(r: &Royale) -> Self {
        Self {
            items: r.items,
            fog: r.fog,
            rarity: r.rarity,
        }
    }

    fn spawn_item_rarity<H: Host>(
        &self,
        host: &mut H,
        item: Item,
        rarity: Rarity,
        at: [f64; 3],
    ) {
        let mut class = self.rarity.class_for(item, rarity);
        if class.is_null() {
            class = self.items[item as usize];
        }
        if !class.is_null() {
            let _ = host.spawn(class, at, 0.0, 0);
        }
    }

    fn spawn_gun_inner<H: Host>(&self, host: &mut H, gun: Item, at: [f64; 3]) {
        self.spawn_item_rarity(host, gun, Rarity::Common, at);
        if let Some(ammo) = gun.ammo() {
            let class = self.items[ammo as usize];
            let side = [at[0] + 24.0, at[1], at[2]];
            let fits = !class.is_null()
                && host
                    .spawn(class, side, 0.0, CHECK_POSITION)
                    .is_some();
            if !fits {
                self.spawn_item_rarity(host, ammo, Rarity::Common, at);
            }
        }
    }
}

impl LootSpawner for Royale {
    fn item_class(&self, item: Item) -> room_script_sdk::ClassRef {
        self.items[item as usize]
    }

    fn spawn_gun<H: Host>(&self, host: &mut H, gun: Item, at: [f64; 3]) {
        Royale::spawn_gun(self, host, gun, at);
    }

    fn spawn_item<H: Host>(&self, host: &mut H, item: Item, rarity: Rarity, at: [f64; 3]) {
        Royale::spawn_item_rarity(self, host, item, rarity, at);
    }

    fn spawn_fog<H: Host>(&self, host: &mut H, at: [f64; 3]) {
        if let Some(fog) = host.spawn(self.fog, at, 0.0, 0) {
            host.actor_sound(fog, "misc/teleport", 1.0);
        }
    }
}

impl Royale {
    pub(crate) fn spawn_item_rarity<H: Host>(
        &self,
        host: &mut H,
        item: Item,
        rarity: Rarity,
        at: [f64; 3],
    ) {
        let mut class = self.rarity.class_for(item, rarity);
        if class.is_null() {
            class = self.items[item as usize];
        }
        if !class.is_null() {
            let _ = host.spawn(class, at, 0.0, 0);
        }
    }

    pub(crate) fn spawn_item<H: Host>(&self, host: &mut H, item: Item, at: [f64; 3]) {
        let roll = host.random(0, 99);
        self.spawn_item_rarity(host, item, Rarity::roll(roll), at);
    }

    /// Drop a gun with its ammo beside it (same spot when there is no room).
    pub(crate) fn spawn_gun<H: Host>(&self, host: &mut H, gun: Item, at: [f64; 3]) {
        self.spawn_item(host, gun, at);
        if let Some(ammo) = gun.ammo() {
            let class = self.items[ammo as usize];
            let side = [at[0] + 24.0, at[1], at[2]];
            let fits = !class.is_null()
                && host
                    .spawn(class, side, 0.0, CHECK_POSITION)
                    .is_some();
            if !fits {
                self.spawn_item(host, ammo, at);
            }
        }
    }

    /// Place the loot. Guns: a shotgun nearest every player's start, a rocket
    /// launcher at the drop nearest the zone centre, then weighted tables by
    /// ring (strong inside, light outside) with spacing and per-gun caps.
    pub(crate) fn drop_loot<H: Host>(&mut self, host: &mut H) {
        let mut order = [0usize; MAX_SPOTS];
        let mut n = 0;
        for i in 0..self.nspots {
            let s = xy(self.spots[i]);
            let near_player = self.placed[..self.nplaced]
                .iter()
                .any(|&p| dist2(p, s) < LOOT_CLEARANCE * LOOT_CLEARANCE);
            if !self.taken[i] && !near_player {
                order[n] = i;
                n += 1;
            }
        }
        if n == 0 {
            return;
        }
        let center = self.center;
        let spots = &self.spots;
        order[..n].sort_unstable_by(|&a, &b| {
            dist2(xy(spots[a]), center)
                .total_cmp(&dist2(xy(spots[b]), center))
                .then(a.cmp(&b))
        });
        // Guns the map already has count toward spacing and caps.
        let mut guns = [[0.0; 2]; MAX_SPOTS + 64];
        let mut nguns = 0;
        let mut rockets = 0;
        let mut chainguns = 0;
        for gun in [
            Item::Shotgun,
            Item::Chaingun,
            Item::RocketLauncher,
            Item::Chainsaw,
        ] {
            let mut found = [Actor::NULL; 64];
            let k = host
                .actors_of_class(self.items[gun as usize], false, &mut found)
                .min(found.len());
            for a in &found[..k] {
                let p = host.actor_pos(*a);
                guns[nguns] = [p[0], p[1]];
                nguns += 1;
            }
            match gun {
                Item::RocketLauncher => rockets += k,
                Item::Chaingun => chainguns += k,
                _ => {}
            }
        }
        // Per ranked drop: assigned item.
        let mut plan: [Option<Item>; MAX_SPOTS] = [None; MAX_SPOTS];
        if rockets == 0 {
            plan[0] = Some(Item::RocketLauncher);
            rockets = 1;
        }
        for p in 0..self.nplaced {
            let mut best = None;
            let mut best_d = f64::MAX;
            for (r, &i) in order[..n].iter().enumerate() {
                let d = dist2(self.placed[p], xy(self.spots[i]));
                if plan[r].is_none() && d < best_d {
                    best_d = d;
                    best = Some(r);
                }
            }
            if let Some(r) = best {
                plan[r] = Some(Item::Shotgun);
            }
        }
        let players = self.contenders().max(1);
        for r in 0..n {
            let at = self.spots[order[r]];
            let (item, forced) = match plan[r] {
                Some(item) => (item, true),
                None => {
                    let table = tier(r, n);
                    (pick(table, host.random(0, total(table) - 1)), false)
                }
            };
            let mut item = item;
            if item.is_gun() && !forced {
                let crowded = guns[..nguns]
                    .iter()
                    .any(|&g| dist2(g, xy(at)) < GUN_SPACING * GUN_SPACING);
                let capped = match item {
                    Item::RocketLauncher => rockets >= rocket_cap(players),
                    Item::Chaingun => chainguns >= chaingun_cap(players),
                    _ => false,
                };
                if crowded || capped {
                    item = item.ammo().unwrap_or(Item::Stimpack);
                }
            }
            match item {
                Item::RocketLauncher if !forced => rockets += 1,
                Item::Chaingun => chainguns += 1,
                _ => {}
            }
            if item.is_gun() && nguns < guns.len() {
                guns[nguns] = xy(at);
                nguns += 1;
                self.spawn_gun(host, item, at);
            } else {
                self.spawn_item(host, item, at);
            }
        }
    }
}
