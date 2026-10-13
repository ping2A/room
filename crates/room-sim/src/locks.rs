//! LOCKDEFS runtime table (key requirements for locked doors).

use crate::actor::ActorId;
use crate::world::World;

/// One key requirement inside a lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockKeyReq {
    /// Exact inventory class name.
    Class(String),
    /// Any of the listed classes.
    Any(Vec<String>),
}

/// One lock definition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockSpec {
    /// Lock number.
    pub number: i32,
    /// Optional game filter.
    pub game: Option<String>,
    /// Required keys.
    pub keys: Vec<LockKeyReq>,
    /// Denied message.
    pub message: Option<String>,
    /// Locked sound override.
    pub locked_sound: Option<String>,
}

/// Compiled lock registry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockTable {
    /// All locks (same number may appear per game).
    pub locks: Vec<LockSpec>,
    /// Active game filter for lookups (`Doom`, …).
    pub game: Option<String>,
}

impl LockTable {
    /// Find a lock by number for the active game.
    #[must_use]
    pub fn find(&self, number: i32) -> Option<&LockSpec> {
        let mut fallback = None;
        for lock in &self.locks {
            if lock.number != number {
                continue;
            }
            match (&lock.game, &self.game) {
                (None, _) => fallback = Some(lock),
                (Some(g), Some(want)) if g.eq_ignore_ascii_case(want) => return Some(lock),
                (Some(_), _) => {}
            }
        }
        fallback
    }
}

/// Whether the actor satisfies the lock's key requirements.
#[must_use]
pub fn actor_has_lock(world: &World, mo: ActorId, lock_num: i32) -> bool {
    if let Some(spec) = world.locks.find(lock_num) {
        if spec.keys.is_empty() {
            // "Any key" style locks — require at least one inventory key-like item
            // is left to full KEYCONF; bootstrap: allow if any listed later.
            // GZDoom Lock 100 has empty keys → any key. Treat as needing any
            // Red/Blue/Yellow card or skull.
            return has_any_doom_key(world, mo);
        }
        for req in &spec.keys {
            match req {
                LockKeyReq::Class(name) => {
                    if !has_class(world, mo, name) {
                        return false;
                    }
                }
                LockKeyReq::Any(names) => {
                    if !names.iter().any(|n| has_class(world, mo, n)) {
                        return false;
                    }
                }
            }
        }
        return true;
    }
    // Fallback Doom XLAT card/skull numbers when no LOCKDEFS loaded.
    let names: &[&str] = match lock_num & 0x7f {
        1 | 4 => &["RedCard", "RedSkull"],
        2 | 5 => &["BlueCard", "BlueSkull"],
        3 | 6 => &["YellowCard", "YellowSkull"],
        _ => return true,
    };
    names.iter().any(|n| has_class(world, mo, n))
}

/// Message / sound for a denied lock.
#[must_use]
pub fn lock_deny_info(world: &World, lock_num: i32) -> (String, String) {
    if let Some(spec) = world.locks.find(lock_num) {
        let msg = spec
            .message
            .clone()
            .unwrap_or_else(|| fallback_message(lock_num).into());
        let sound = spec
            .locked_sound
            .clone()
            .unwrap_or_else(|| "misc/keytry".into());
        return (msg, sound);
    }
    (fallback_message(lock_num).into(), "misc/keytry".into())
}

fn has_class(world: &World, mo: ActorId, name: &str) -> bool {
    world
        .classes
        .find_name(name)
        .is_some_and(|cid| crate::inventory::count_inventory(world, mo, cid) > 0)
}

fn has_any_doom_key(world: &World, mo: ActorId) -> bool {
    for n in [
        "RedCard",
        "RedSkull",
        "BlueCard",
        "BlueSkull",
        "YellowCard",
        "YellowSkull",
    ] {
        if has_class(world, mo, n) {
            return true;
        }
    }
    false
}

fn fallback_message(lock: i32) -> &'static str {
    match lock & 0x7f {
        1 | 4 => "You need a red key to open this door",
        2 | 5 => "You need a blue key to open this door",
        3 | 6 => "You need a yellow key to open this door",
        _ => "You need a key to open this door",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::class::ClassRegistry;
    use crate::inventory::give_inventory;
    use crate::state::StateTable;
    use crate::world::World;
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    #[test]
    fn lockdefs_any_red_accepts_redcard() {
        let ir = parse_decorate(
            r#"
actor Inventory : Actor
{
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor RedCard : Inventory 5001 {}
actor PlayerPawn : Actor 1 { +SHOOTABLE Health 100 Radius 16 Height 56 }
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut world = World::from_map_with_classes(map, classes, states).unwrap();
        world.locks = LockTable {
            game: Some("Doom".into()),
            locks: vec![LockSpec {
                number: 129,
                game: Some("Doom".into()),
                keys: vec![LockKeyReq::Any(vec!["RedCard".into(), "RedSkull".into()])],
                message: Some("$PD_REDK".into()),
                locked_sound: Some("misc/keytry".into()),
            }],
        };
        let mo = world.players[0].as_ref().unwrap().mo.unwrap();
        assert!(!actor_has_lock(&world, mo, 129));
        let red = world.classes.find_name("RedCard").unwrap();
        give_inventory(&mut world, mo, red, 1);
        assert!(actor_has_lock(&world, mo, 129));
        let (msg, _) = lock_deny_info(&world, 129);
        assert_eq!(msg, "$PD_REDK");
    }
}
