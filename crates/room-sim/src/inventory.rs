//! Inventory linked list (GZDoom `AInventory` subset).

use crate::actor::ActorId;
use crate::world::World;
use room_core::ClassId;
use std::collections::BTreeMap;

/// One inventory item instance owned by an actor.
#[derive(Clone, Debug)]
pub struct InvItem {
    /// Item class.
    pub class: ClassId,
    /// Stack amount.
    pub amount: i32,
    /// Max stack.
    pub max_amount: i32,
}

/// Per-actor inventory bags (newest-first order preserved as `Vec` push front).
#[derive(Clone, Debug, Default)]
pub struct InventoryBags {
    /// Owner → items (index 0 = newest).
    bags: BTreeMap<u64, Vec<InvItem>>,
}

fn key(id: ActorId) -> u64 {
    (u64::from(id.index()) << 32) | u64::from(id.generation())
}

impl InventoryBags {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Find first item of `class` on `owner`.
    pub fn find(&self, owner: ActorId, class: ClassId) -> Option<&InvItem> {
        self.bags
            .get(&key(owner))?
            .iter()
            .find(|i| i.class == class)
    }

    /// Mutable find.
    pub fn find_mut(&mut self, owner: ActorId, class: ClassId) -> Option<&mut InvItem> {
        self.bags
            .get_mut(&key(owner))?
            .iter_mut()
            .find(|i| i.class == class)
    }

    /// Give `amount` of `class` (merge stacks). Returns true if anything given.
    pub fn give(&mut self, owner: ActorId, class: ClassId, amount: i32, max_amount: i32) -> bool {
        if amount <= 0 {
            return false;
        }
        let bag = self.bags.entry(key(owner)).or_default();
        if let Some(existing) = bag.iter_mut().find(|i| i.class == class) {
            let room = (existing.max_amount - existing.amount).max(0);
            let add = amount.min(room);
            existing.amount += add;
            return add > 0;
        }
        bag.insert(
            0,
            InvItem {
                class,
                amount: amount.min(max_amount),
                max_amount,
            },
        );
        true
    }

    /// Take up to `amount`. Returns amount actually taken.
    pub fn take(&mut self, owner: ActorId, class: ClassId, amount: i32) -> i32 {
        let Some(bag) = self.bags.get_mut(&key(owner)) else {
            return 0;
        };
        let Some(pos) = bag.iter().position(|i| i.class == class) else {
            return 0;
        };
        let item = &mut bag[pos];
        let taken = amount.min(item.amount).max(0);
        item.amount -= taken;
        if item.amount <= 0 {
            bag.remove(pos);
        }
        taken
    }

    /// Set absolute amount (clamped to `max_amount`; 0 removes the stack).
    pub fn set(&mut self, owner: ActorId, class: ClassId, amount: i32, max_amount: i32) {
        let max_amount = max_amount.max(1);
        if amount <= 0 {
            let _ = self.take(owner, class, i32::MAX);
            return;
        }
        let amount = amount.min(max_amount);
        let bag = self.bags.entry(key(owner)).or_default();
        if let Some(existing) = bag.iter_mut().find(|i| i.class == class) {
            existing.amount = amount;
            existing.max_amount = max_amount;
        } else {
            bag.insert(
                0,
                InvItem {
                    class,
                    amount,
                    max_amount,
                },
            );
        }
    }

    /// ACS `SetAmmoCapacity` — set max (and clamp amount); creates a 0-stack if absent.
    pub fn set_max_amount(&mut self, owner: ActorId, class: ClassId, max_amount: i32) {
        let max_amount = max_amount.max(1);
        let bag = self.bags.entry(key(owner)).or_default();
        if let Some(existing) = bag.iter_mut().find(|i| i.class == class) {
            existing.max_amount = max_amount;
            existing.amount = existing.amount.min(max_amount);
        } else {
            bag.insert(
                0,
                InvItem {
                    class,
                    amount: 0,
                    max_amount,
                },
            );
        }
    }

    /// Total amount of `class` on owner.
    #[must_use]
    pub fn count(&self, owner: ActorId, class: ClassId) -> i32 {
        self.find(owner, class).map_or(0, |i| i.amount)
    }

    /// Iterate items newest-first.
    pub fn iter(&self, owner: ActorId) -> impl Iterator<Item = &InvItem> {
        self.bags
            .get(&key(owner))
            .map(|v| v.iter())
            .into_iter()
            .flatten()
    }

    /// Drop every item owned by `owner`.
    pub fn clear(&mut self, owner: ActorId) {
        self.bags.remove(&key(owner));
    }
}

/// Give inventory using class defaults for max amount.
pub fn give_inventory(world: &mut World, owner: ActorId, class: ClassId, amount: i32) -> bool {
    let max = world
        .classes
        .get(class)
        .map(|c| c.inventory_max_amount)
        .unwrap_or(1)
        .max(1);
    world.inventory.give(owner, class, amount, max)
}

/// Query amount.
#[must_use]
pub fn count_inventory(world: &World, owner: ActorId, class: ClassId) -> i32 {
    world.inventory.count(owner, class)
}

/// Set absolute inventory amount using class max.
pub fn set_inventory(world: &mut World, owner: ActorId, class: ClassId, amount: i32) {
    let max = world
        .classes
        .get(class)
        .map(|c| c.inventory_max_amount)
        .unwrap_or(1)
        .max(1);
    world.inventory.set(owner, class, amount, max);
}

/// Console `give` cheat: `"all"`, `"weapons"`, `"ammo"`, `"keys"`, `"armor"`,
/// `"idkfa"` / `"idfa"`, or a class name.
/// Returns how many distinct classes were granted.
pub fn cheat_give(world: &mut World, owner: ActorId, what: &str) -> u32 {
    let key = what.trim().to_ascii_lowercase();
    match key.as_str() {
        "all" | "idkfa" => {
            let n = cheat_give(world, owner, "weapons");
            n + cheat_give(world, owner, "ammo")
                + cheat_give(world, owner, "keys")
                + cheat_give(world, owner, "armor")
        }
        "idfa" => {
            let n = cheat_give(world, owner, "weapons");
            n + cheat_give(world, owner, "ammo") + cheat_give(world, owner, "armor")
        }
        "weapons" => {
            // GZDoom only gives weapons that sit in a slot.
            let names: Vec<&str> = [
                "Chainsaw",
                "Shotgun",
                "SuperShotgun",
                "Chaingun",
                "RocketLauncher",
                "PlasmaRifle",
                "BFG9000",
            ]
            .into_iter()
            .filter(|n| world.weapon_slots.contains(n))
            .collect();
            give_named(world, owner, &names, 1)
        }
        "ammo" => give_named(world, owner, &["Clip", "Shell", "RocketAmmo", "Cell"], 200),
        "keys" => give_named(
            world,
            owner,
            &[
                "BlueCard",
                "YellowCard",
                "RedCard",
                "BlueSkull",
                "YellowSkull",
                "RedSkull",
            ],
            1,
        ),
        "armor" => {
            let n = give_named(world, owner, &["BlueArmor"], 1);
            // Vanilla `idkfa` / `give armor`: armorpoints = 200, armortype = 2.
            for p in world.players.iter_mut().flatten() {
                if p.mo == Some(owner) {
                    p.armor = 200;
                    p.armor_class = 2;
                    break;
                }
            }
            n
        }
        "idchoppers" => {
            let n = give_named(world, owner, &["Chainsaw"], 1);
            // Vanilla also sets `pw_invulnerability` (force on, don't toggle).
            for p in world.players.iter_mut().flatten() {
                if p.mo == Some(owner) {
                    p.invulnerable = true;
                    p.invul_tics = crate::player::INVUL_TICS;
                    break;
                }
            }
            n
        }
        s if s.starts_with("idbehold") => {
            let letter = s.strip_prefix("idbehold").unwrap_or("").chars().next();
            u32::from(letter.is_some_and(|c| cheat_behold(world, owner, c)))
        }
        name => world
            .classes
            .find_name(name)
            .map_or(0, |cid| u32::from(give_inventory(world, owner, cid, 1))),
    }
}

/// Toggle a vanilla `idbehold` power (`v/s/i/r/a/l`). Returns whether applied.
fn cheat_behold(world: &mut World, owner: ActorId, letter: char) -> bool {
    let letter = letter.to_ascii_lowercase();
    let Some(slot) = world
        .players
        .iter()
        .position(|p| p.as_ref().is_some_and(|pl| pl.mo == Some(owner)))
    else {
        return false;
    };
    let Some(p) = world.players[slot].as_mut() else {
        return false;
    };
    match letter {
        'v' => {
            if p.invulnerable {
                p.invulnerable = false;
                p.invul_tics = 0;
            } else {
                p.invulnerable = true;
                p.invul_tics = crate::player::INVUL_TICS;
            }
        }
        's' => {
            p.berserk = !p.berserk;
        }
        'i' => {
            if p.blur {
                p.blur = false;
                p.blur_tics = 0;
            } else {
                p.blur = true;
                p.blur_tics = crate::player::BLUR_TICS;
            }
        }
        'r' => {
            if p.radsuit {
                p.radsuit = false;
                p.radsuit_tics = 0;
            } else {
                p.radsuit = true;
                p.radsuit_tics = crate::player::RADSUIT_TICS;
            }
        }
        'a' => {
            p.allmap = !p.allmap;
        }
        'l' => {
            if p.infrared {
                p.infrared = false;
                p.infrared_tics = 0;
            } else {
                p.infrared = true;
                p.infrared_tics = crate::player::INFRARED_TICS;
            }
        }
        _ => return false,
    }
    if letter == 'i' {
        let blur = world.players[slot].as_ref().is_some_and(|pl| pl.blur);
        if let Some(a) = world.actors.get_mut(owner) {
            if blur {
                a.alpha = 0.4;
                a.render_style = crate::class::RenderStyle::Fuzzy;
            } else {
                a.alpha = 1.0;
                a.render_style = crate::class::RenderStyle::Normal;
            }
        }
    }
    true
}

fn give_named(world: &mut World, owner: ActorId, names: &[&str], amount: i32) -> u32 {
    let mut n = 0;
    for name in names {
        if let Some(cid) = world.classes.find_name(name) {
            if give_inventory(world, owner, cid, amount) {
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{ClassRegistry, StateTable, World};
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    #[test]
    fn cheat_give_weapons_and_class() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert!(cheat_give(&mut w, mo, "weapons") >= 1);
        let shotgun = classes.find_name("Shotgun").unwrap();
        assert!(count_inventory(&w, mo, shotgun) >= 1);
        assert!(cheat_give(&mut w, mo, "keys") >= 1);
        let blue = classes.find_name("BlueCard").unwrap();
        assert!(count_inventory(&w, mo, blue) >= 1);
    }

    #[test]
    fn cheat_give_keys_includes_skulls() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert!(cheat_give(&mut w, mo, "keys") >= 6);
        let skull = classes.find_name("BlueSkull").unwrap();
        assert!(count_inventory(&w, mo, skull) >= 1);
    }

    #[test]
    fn cheat_give_idkfa_includes_armor() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert!(cheat_give(&mut w, mo, "idkfa") >= 3);
        let armor = classes.find_name("BlueArmor").unwrap();
        assert!(count_inventory(&w, mo, armor) >= 1);
        let p = w.players[0].as_ref().unwrap();
        assert_eq!((p.armor, p.armor_class), (200, 2));
        let bfg = classes.find_name("BFG9000").unwrap();
        assert!(count_inventory(&w, mo, bfg) >= 1);
    }

    #[test]
    fn cheat_idchoppers_and_idbeholdv() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(cheat_give(&mut w, mo, "idchoppers"), 1);
        let saw = classes.find_name("Chainsaw").unwrap();
        assert!(count_inventory(&w, mo, saw) >= 1);
        assert!(w.players[0].as_ref().unwrap().invulnerable);
        assert!(w.players[0].as_ref().unwrap().invul_tics > 0);
        // idchoppers already turned invuln on; idbeholdv toggles it off.
        assert_eq!(cheat_give(&mut w, mo, "idbeholdv"), 1);
        assert!(!w.players[0].as_ref().unwrap().invulnerable);
        assert_eq!(cheat_give(&mut w, mo, "idbeholdv"), 1);
        assert!(w.players[0].as_ref().unwrap().invulnerable);
        assert_eq!(cheat_give(&mut w, mo, "idbeholds"), 1);
        assert!(w.players[0].as_ref().unwrap().berserk);
        assert_eq!(cheat_give(&mut w, mo, "idbeholdi"), 1);
        assert!(w.players[0].as_ref().unwrap().blur);
        assert_eq!(
            w.actors.get(mo).unwrap().render_style,
            crate::class::RenderStyle::Fuzzy
        );
    }

    #[test]
    fn give_and_take_clip() {
        let ir = parse_decorate(
            r#"
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::new());
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let clip = classes.find_name("Clip").unwrap();
        assert!(give_inventory(&mut w, mo, clip, 10));
        assert_eq!(count_inventory(&w, mo, clip), 10);
        assert_eq!(w.inventory.take(mo, clip, 3), 3);
        assert_eq!(count_inventory(&w, mo, clip), 7);
    }
}
