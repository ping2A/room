//! Touch / pickup (`CallTryPickup` subset).

use crate::actor::ActorId;
use crate::inventory::give_inventory;
use crate::world::World;
use ferrite_core::ClassId;

/// Try to pick up `item` into `toucher`'s inventory. Returns true if consumed.
pub fn try_pickup(world: &mut World, toucher: ActorId, item: ActorId) -> bool {
    let Some(it) = world.actors.get(item).cloned() else {
        return false;
    };
    if it.destroyed {
        return false;
    }
    let class = it.class;
    let dropped = (it.flags & crate::class::FLAG_DROPPED) != 0;
    let amount = world
        .classes
        .get(class)
        .map(|c| c.inventory_amount.max(1))
        .unwrap_or(1);
    // Vanilla `MF_DROPPED`: ammo (and weapon ammo give) at half amount (min 1).
    let amount = if dropped { (amount / 2).max(1) } else { amount };
    let class_name = world
        .classes
        .get(class)
        .map(|c| c.name.to_ascii_lowercase())
        .unwrap_or_default();
    // Armor pickups set `Player.armor` instead of stacking as inventory.
    let armor_points = match class_name.as_str() {
        "greenarmor" => Some(100),
        "bluearmor" => Some(200),
        "armorbonus" => Some(-1), // +1, capped at 200
        _ => None,
    };
    // Health pickups heal the toucher's actor (amount / max from class props).
    let health_heal = match class_name.as_str() {
        "stimpack" | "medikit" | "healthbonus" | "soulsphere" | "megasphere" => {
            let max = world
                .classes
                .get(class)
                .map(|c| c.inventory_max_amount)
                .unwrap_or(100)
                .max(1);
            Some((amount, max))
        }
        _ => None,
    };
    let is_berserk = class_name == "berserk";
    let is_invuln = class_name == "invulnerabilitysphere";
    let is_blur = class_name == "blursphere";
    let is_radsuit = class_name == "radsuit";
    let is_allmap = class_name == "allmap";
    let is_infrared = class_name == "infrared";
    let is_backpack = class_name == "backpack";
    // Medikit uses a different message when health was critically low.
    let mut medikit_needed = false;
    if let Some(pts) = armor_points {
        let mut applied = false;
        for p in world.players.iter_mut().flatten() {
            if p.mo != Some(toucher) {
                continue;
            }
            if pts < 0 {
                // Armor bonus: +1 up to 200; first bonus grants green armor type.
                if p.armor < 200 {
                    p.armor += 1;
                    if p.armor_class == 0 {
                        p.armor_class = 1;
                    }
                    applied = true;
                }
            } else if p.armor < pts {
                p.armor = pts;
                p.armor_class = if class_name == "bluearmor" { 2 } else { 1 };
                applied = true;
            }
        }
        if !applied {
            return false;
        }
    } else if let Some((heal, max_hp)) = health_heal {
        let Some(a) = world.actors.get_mut(toucher) else {
            return false;
        };
        if class_name == "medikit" {
            medikit_needed = a.health < 25;
        }
        if class_name == "megasphere" {
            a.health = max_hp;
        } else if a.health >= max_hp {
            return false;
        } else {
            a.health = (a.health + heal).min(max_hp);
        }
        if class_name == "megasphere" {
            for p in world.players.iter_mut().flatten() {
                if p.mo == Some(toucher) {
                    p.armor = 200;
                    p.armor_class = 2;
                }
            }
        }
    } else if is_berserk {
        let mut applied = false;
        for p in world.players.iter_mut().flatten() {
            if p.mo != Some(toucher) {
                continue;
            }
            p.berserk = true;
            applied = true;
        }
        if !applied {
            return false;
        }
        if let Some(a) = world.actors.get_mut(toucher) {
            if a.health < 100 {
                a.health = 100;
            }
        }
        // Doom: force ready weapon to Fist when available (raise/lower).
        if let Some(fist) = world.classes.find_name("Fist") {
            if world.inventory.count(toucher, fist) > 0 {
                switch_player_weapon(world, toucher, fist);
            }
        }
        world.push_message("Berserk!");
    } else if is_invuln {
        let mut applied = false;
        for p in world.players.iter_mut().flatten() {
            if p.mo != Some(toucher) {
                continue;
            }
            p.invulnerable = true;
            p.invul_tics = crate::player::INVUL_TICS;
            applied = true;
        }
        if !applied {
            return false;
        }
    } else if is_blur {
        let mut applied = false;
        for p in world.players.iter_mut().flatten() {
            if p.mo != Some(toucher) {
                continue;
            }
            p.blur = true;
            p.blur_tics = crate::player::BLUR_TICS;
            applied = true;
        }
        if !applied {
            return false;
        }
        // Doom blur = spectre fuzz look on the player mobj.
        if let Some(a) = world.actors.get_mut(toucher) {
            a.render_style = crate::class::RenderStyle::Fuzzy;
            a.alpha = 0.4;
        }
    } else if is_radsuit {
        let mut applied = false;
        for p in world.players.iter_mut().flatten() {
            if p.mo != Some(toucher) {
                continue;
            }
            p.radsuit = true;
            p.radsuit_tics = crate::player::RADSUIT_TICS;
            applied = true;
        }
        if !applied {
            return false;
        }
    } else if is_allmap {
        let mut applied = false;
        for p in world.players.iter_mut().flatten() {
            if p.mo != Some(toucher) {
                continue;
            }
            p.allmap = true;
            applied = true;
        }
        if !applied {
            return false;
        }
    } else if is_infrared {
        let mut applied = false;
        for p in world.players.iter_mut().flatten() {
            if p.mo != Some(toucher) {
                continue;
            }
            p.infrared = true;
            p.infrared_tics = crate::player::INFRARED_TICS;
            applied = true;
        }
        if !applied {
            return false;
        }
    } else if is_backpack {
        let mut applied = false;
        for p in world.players.iter_mut().flatten() {
            if p.mo != Some(toucher) {
                continue;
            }
            if p.backpack {
                // Already have backpack — still grant small ammo refill.
            } else {
                p.backpack = true;
            }
            applied = true;
        }
        if !applied {
            return false;
        }
        // Grant a small ammo dump and double existing ammo stack caps.
        for ammo_name in ["Clip", "Shell", "RocketAmmo", "Cell"] {
            let Some(cid) = world.classes.find_name(ammo_name) else {
                continue;
            };
            let max = world
                .classes
                .get(cid)
                .map(|c| c.inventory_max_amount)
                .unwrap_or(50);
            let doubled = max.saturating_mul(2);
            if let Some(item) = world.inventory.find_mut(toucher, cid) {
                item.max_amount = doubled;
            }
            let _ = give_inventory(world, toucher, cid, amount.max(1));
            if let Some(item) = world.inventory.find_mut(toucher, cid) {
                item.max_amount = doubled;
            }
        }
        world.push_message("Picked up a backpack full of ammo!");
    } else if world.classes.get(class).is_some_and(|c| c.is_weapon) {
        let already = world.inventory.count(toucher, class) > 0;
        if !already && !give_inventory(world, toucher, class, amount) {
            return false;
        }
        let (ammo_name, ammo_give) = world
            .classes
            .get(class)
            .map(|c| (c.ammo_type.clone(), c.ammo_give))
            .unwrap_or_default();
        let mut gave_ammo = false;
        if ammo_give > 0 && !ammo_name.is_empty() {
            if let Some(ammo) = world.classes.find_name(&ammo_name) {
                let give = if dropped {
                    (ammo_give / 2).max(1)
                } else {
                    ammo_give
                };
                let before = world.inventory.count(toucher, ammo);
                gave_ammo = give_inventory(world, toucher, ammo, give);
                if std::env::var_os("FERRITE_TRACE_AMMO").is_some() {
                    eprintln!(
                        "AMMOGIVE tic={} item={} dropped={} give={} before={} after={} ok={}",
                        world.level.time,
                        class_name,
                        dropped,
                        give,
                        before,
                        world.inventory.count(toucher, ammo),
                        gave_ammo
                    );
                }
            }
        }
        if already && !gave_ammo {
            return false;
        }
        if !already {
            switch_player_weapon(world, toucher, class);
        }
    } else if let Some(ammo_name) = ammo_box_grants(&class_name) {
        // Boxes are separate DoomEdNums but feed the base ammo class.
        let Some(ammo) = world.classes.find_name(ammo_name) else {
            return false;
        };
        if std::env::var_os("FERRITE_TRACE_AMMO").is_some() {
            eprintln!(
                "AMMOBOX tic={} item={} -> {} amount={}",
                world.level.time, class_name, ammo_name, amount
            );
        }
        if !give_inventory(world, toucher, ammo, amount) {
            return false;
        }
    } else if !give_inventory(world, toucher, class, amount) {
        return false;
    } else if std::env::var_os("FERRITE_TRACE_AMMO").is_some()
        && (class_name.contains("shell") || class_name.contains("clip"))
    {
        eprintln!(
            "AMMOPICK tic={} item={} amount={}",
            world.level.time, class_name, amount
        );
    }
    let (countitem, countsecret) = world.actors.get(item).map_or((false, false), |a| {
        (
            (a.flags & crate::class::FLAG_COUNTITEM) != 0,
            a.count_secret,
        )
    });
    // Consume the world item.
    if let Some(a) = world.actors.get_mut(item) {
        a.destroyed = true;
        a.state = None;
        a.flags = 0;
        a.count_secret = false;
    }
    if countitem {
        world.level.found_items = world.level.found_items.saturating_add(1);
    }
    if countsecret {
        world.level.found_secrets = world.level.found_secrets.saturating_add(1);
    }
    let (pickup_msg, pickup_snd) = world
        .classes
        .get(class)
        .map(|c| (c.pickup_message.clone(), c.pickup_sound.clone()))
        .unwrap_or_default();
    if !pickup_msg.is_empty() {
        world.push_message(pickup_msg);
    } else if class_name == "medikit" {
        // Vanilla: `GOTMEDINEED` when health was below 25 before the heal.
        world.push_message(if medikit_needed {
            "Picked up a medikit that you REALLY need!"
        } else {
            "Picked up a medikit."
        });
    } else if let Some(msg) = default_pickup_message(&class_name) {
        world.push_message(msg);
    }
    if !pickup_snd.is_empty() {
        world.play_sound(&pickup_snd, it.pos.x(), it.pos.y(), it.pos.z(), 1.0);
    }
    let _ = ClassId::from_raw(0);
    true
}

/// Begin a raise/lower weapon switch for the player owning `toucher`.
fn switch_player_weapon(world: &mut World, toucher: ActorId, weapon: ferrite_core::ClassId) {
    for i in 0..crate::player::MAX_PLAYERS {
        let Some(mut player) = world.players[i].take() else {
            continue;
        };
        if player.mo != Some(toucher) {
            world.players[i] = Some(player);
            continue;
        }
        crate::think_player::request_weapon_change(world, &mut player, toucher, weapon);
        world.players[i] = Some(player);
        break;
    }
}

/// Doom ammo boxes grant their base ammo type (not a separate inventory stack).
fn ammo_box_grants(class_name: &str) -> Option<&'static str> {
    match class_name {
        "clipbox" => Some("Clip"),
        "shellbox" => Some("Shell"),
        "rocketbox" => Some("RocketAmmo"),
        "cellpack" => Some("Cell"),
        _ => None,
    }
}

fn default_pickup_message(class_name: &str) -> Option<&'static str> {
    Some(match class_name {
        "bluecard" => "Picked up a blue keycard.",
        "yellowcard" => "Picked up a yellow keycard.",
        "redcard" => "Picked up a red keycard.",
        "blueskull" => "Picked up a blue skull key.",
        "yellowskull" => "Picked up a yellow skull key.",
        "redskull" => "Picked up a red skull key.",
        "stimpack" => "Picked up a stimpack.",
        "medikit" => "Picked up a medikit.",
        "greenarmor" => "Picked up the armor.",
        "bluearmor" => "Picked up the megaarmor!",
        "clip" => "Picked up a clip.",
        "clipbox" => "Picked up a box of bullets.",
        "shell" => "Picked up 4 shotgun shells.",
        "shellbox" => "Picked up a box of shotgun shells.",
        "rocketammo" => "Picked up a rocket.",
        "rocketbox" => "Picked up a box of rockets.",
        "cell" => "Picked up an energy cell.",
        "cellpack" => "Picked up an energy cell pack.",
        "shotgun" => "You got the shotgun!",
        "supershotgun" => "You got the super shotgun!",
        "chaingun" => "You got the chaingun!",
        "rocketlauncher" => "You got the rocket launcher!",
        "plasmarifle" => "You got the plasma rifle!",
        "bfg9000" => "You got the BFG9000! Oh, yes.",
        "chainsaw" => "A chainsaw! Find some meat!",
        "soulsphere" => "Supercharge!",
        "megasphere" => "MegaSphere!",
        "healthbonus" => "Picked up a health bonus.",
        "armorbonus" => "Picked up an armor bonus.",
        "invulnerabilitysphere" => "Invulnerability!",
        "blursphere" => "Partial Invisibility",
        "radsuit" => "Radiation Shielding Suit",
        "allmap" => "Computer Area Map",
        "infrared" => "Light Amplification Visor",
        _ => return None,
    })
}

/// Touch all overlapping inventory-ish actors near `toucher` (radius sum).
///
/// `player` is the in-hand slot from [`crate::think_player::player_think`] (the
/// corresponding `world.players[i]` entry is empty for that duration). Park it
/// so pickup paths that match on `player.mo` still work.
pub fn touch_specials(world: &mut World, player: &mut crate::player::Player, toucher: ActorId) {
    world.with_parked_player(player, |world| touch_specials_inner(world, toucher));
}

fn touch_specials_inner(world: &mut World, toucher: ActorId) {
    let Some(self_a) = world.actors.get(toucher).cloned() else {
        return;
    };
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        if id == toucher {
            continue;
        }
        let Some(other) = world.actors.get(id) else {
            continue;
        };
        if other.destroyed {
            continue;
        }
        // Only pick up classes with inventory amounts > defaults-as-actor, or doomednums
        // in ammo/weapon ranges — bootstrap: any non-shootable, non-solid? Prefer
        // inventory_max_amount > 1 or inventory_amount set, or is_weapon.
        let Some(def) = world.classes.get(other.class) else {
            continue;
        };
        let is_armor = matches!(
            def.name.to_ascii_lowercase().as_str(),
            "greenarmor" | "bluearmor" | "armorbonus"
        );
        let is_health = matches!(
            def.name.to_ascii_lowercase().as_str(),
            "stimpack"
                | "medikit"
                | "healthbonus"
                | "soulsphere"
                | "megasphere"
                | "berserk"
                | "invulnerabilitysphere"
                | "blursphere"
                | "radsuit"
                | "allmap"
                | "infrared"
                | "backpack"
        );
        let is_key = matches!(
            def.name.to_ascii_lowercase().as_str(),
            "bluecard" | "yellowcard" | "redcard" | "blueskull" | "yellowskull" | "redskull"
        );
        if !def.is_weapon
            && !is_armor
            && !is_health
            && !is_key
            && (def.flags & crate::class::FLAG_COUNTITEM) == 0
            && def.inventory_max_amount <= 1
            && def.inventory_amount <= 1
        {
            // Skip plain actors (monsters / decorations).
            if def.health > 0 && (other.flags & (1 << 1)) != 0 {
                continue;
            }
            if def.doomednum.is_none() {
                continue;
            }
            // Still allow if MaxAmount was explicitly set via decorate > 1 — already handled.
            if def.inventory_amount == 1 && def.inventory_max_amount == 1 && !def.is_weapon {
                continue;
            }
        }
        // Doom `PIT_CheckThing` / `P_TouchSpecialThing`: axis-aligned square overlap
        // (not a circle). Circle tests miss diagonal grazes (DEMO1 Shell before @410).
        let dx = (other.pos.x() - self_a.pos.x()).abs();
        let dy = (other.pos.y() - self_a.pos.y()).abs();
        let block = other.radius + self_a.radius;
        if dx >= block || dy >= block {
            continue;
        }
        // Doom `P_TouchSpecialThing`: `delta > toucher.height || delta < -8`.
        let delta = other.pos.z() - self_a.pos.z();
        if delta > self_a.height || delta < -8.0 {
            if std::env::var_os("FERRITE_TRACE_AMMO").is_some()
                && (def.is_weapon
                    || def.name.eq_ignore_ascii_case("Shell")
                    || def.name.eq_ignore_ascii_case("ShellBox"))
            {
                eprintln!(
                    "AMMOZMISS tic={} item={} delta={:.3} th={:.1} ih={:.1} p=({:.1},{:.1},{:.1}) i=({:.1},{:.1},{:.1})",
                    world.level.time,
                    def.name,
                    delta,
                    self_a.height,
                    other.height,
                    self_a.pos.x(),
                    self_a.pos.y(),
                    self_a.pos.z(),
                    other.pos.x(),
                    other.pos.y(),
                    other.pos.z()
                );
            }
            continue;
        }
        let _ = try_pickup(world, toucher, id);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{ClassRegistry, StateTable, World, count_inventory};
    use ferrite_core::{Angle, DVec3};
    use ferrite_decorate::parse_decorate;
    use ferrite_nodebuild::apply_nodes;
    use ferrite_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    #[test]
    fn picks_up_shell_rocket_cell_boxes() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let shell = classes.find_name("Shell").unwrap();
        let rocket = classes.find_name("RocketAmmo").unwrap();
        let cell = classes.find_name("Cell").unwrap();
        let shell_box = classes.find_name("ShellBox").unwrap();
        let rocket_box = classes.find_name("RocketBox").unwrap();
        let cell_pack = classes.find_name("CellPack").unwrap();
        assert_eq!(classes.get(shell_box).unwrap().inventory_amount, 20);
        assert_eq!(classes.get(rocket_box).unwrap().inventory_amount, 5);
        assert_eq!(classes.get(cell_pack).unwrap().inventory_amount, 100);

        let shells0 = count_inventory(&w, player, shell);
        let item = w
            .spawn_at(shell_box, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(count_inventory(&w, player, shell), shells0 + 20);
        assert!(
            w.drain_messages()
                .iter()
                .any(|m| m.contains("box of shotgun shells"))
        );

        let rockets0 = count_inventory(&w, player, rocket);
        let item = w
            .spawn_at(rocket_box, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(count_inventory(&w, player, rocket), rockets0 + 5);

        let cells0 = count_inventory(&w, player, cell);
        let item = w
            .spawn_at(cell_pack, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(count_inventory(&w, player, cell), cells0 + 100);
    }

    #[test]
    fn picks_up_clip_box_grants_clip_ammo() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let box_c = classes.find_name("ClipBox").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        assert_eq!(classes.get(box_c).unwrap().inventory_amount, 50);
        assert_eq!(classes.get(box_c).unwrap().doomednum, Some(2048));
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let before = count_inventory(&w, player, clip);
        let item = w
            .spawn_at(box_c, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(count_inventory(&w, player, clip), before + 50);
        assert_eq!(count_inventory(&w, player, box_c), 0);
        assert!(
            w.drain_messages()
                .iter()
                .any(|m| m.contains("box of bullets")),
            "expected clip box message"
        );
    }

    #[test]
    fn picks_up_backpack_doubles_ammo_max() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let pack = classes.find_name("Backpack").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let _ = give_inventory(&mut w, player, clip, 10);
        let item = w
            .spawn_at(pack, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(w.players[0].as_ref().unwrap().backpack);
        let max = w.inventory.find(player, clip).unwrap().max_amount;
        assert_eq!(max, 400); // Clip MaxAmount 200 × 2
        assert!(count_inventory(&w, player, clip) > 10);
        assert!(w.drain_messages().iter().any(|m| m.contains("backpack")));
    }

    #[test]
    fn picks_up_shotgun_gives_shells() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let shotgun = classes.find_name("Shotgun").unwrap();
        let shell = classes.find_name("Shell").unwrap();
        assert_eq!(classes.get(shotgun).unwrap().ammo_give, 8);
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let before = count_inventory(&w, player, shell);
        let item = w
            .spawn_at(shotgun, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(count_inventory(&w, player, shotgun) >= 1);
        assert_eq!(count_inventory(&w, player, shell), before + 8);
        assert_eq!(w.players[0].as_ref().unwrap().pending_weapon, Some(shotgun));
    }

    #[test]
    fn picks_up_infrared() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let goggles = classes.find_name("Infrared").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(goggles, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(w.players[0].as_ref().unwrap().infrared);
    }

    #[test]
    fn picks_up_allmap() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let map_item = classes.find_name("Allmap").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(map_item, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(w.players[0].as_ref().unwrap().allmap);
    }

    #[test]
    fn picks_up_radsuit() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let suit = classes.find_name("RadSuit").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(suit, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(w.players[0].as_ref().unwrap().radsuit);
    }

    #[test]
    fn picks_up_blur_sphere() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let blur = classes.find_name("BlurSphere").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(blur, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(w.players[0].as_ref().unwrap().blur);
        let a = w.actors.get(player).unwrap();
        assert_eq!(a.render_style, crate::class::RenderStyle::Fuzzy);
        assert!((a.alpha - 0.4).abs() < 1e-9);
    }

    #[test]
    fn picks_up_invulnerability() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let inv = classes.find_name("InvulnerabilitySphere").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(inv, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(w.players[0].as_ref().unwrap().invulnerable);
        assert_eq!(
            w.players[0].as_ref().unwrap().invul_tics,
            crate::player::INVUL_TICS
        );
        assert_eq!(
            crate::damage::damage_mobj(&mut w, player, None, None, 50),
            0
        );
    }

    #[test]
    fn picks_up_berserk() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let berserk = classes.find_name("Berserk").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(player) {
            a.health = 40;
        }
        let item = w
            .spawn_at(berserk, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(w.players[0].as_ref().unwrap().berserk);
        assert_eq!(w.actors.get(player).unwrap().health, 100);
        let fist = classes.find_name("Fist").unwrap();
        assert_eq!(w.players[0].as_ref().unwrap().pending_weapon, Some(fist));
        assert!(
            w.drain_messages()
                .iter()
                .any(|m| m.eq_ignore_ascii_case("Berserk!"))
        );
    }

    #[test]
    fn picks_up_soulsphere_and_megasphere() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let soul = classes.find_name("Soulsphere").unwrap();
        let mega = classes.find_name("Megasphere").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(player) {
            a.health = 50;
        }
        let item = w
            .spawn_at(soul, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(w.actors.get(player).unwrap().health, 150);
        let item2 = w
            .spawn_at(mega, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item2));
        assert_eq!(w.actors.get(player).unwrap().health, 200);
        assert_eq!(w.players[0].as_ref().unwrap().armor, 200);
    }

    #[test]
    fn picks_up_stimpack_heals() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let stim = classes.find_name("Stimpack").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(player) {
            a.health = 50;
        }
        let item = w
            .spawn_at(stim, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(w.actors.get(player).unwrap().health, 60);
    }

    #[test]
    fn medikit_message_depends_on_health() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let kit = classes.find_name("Medikit").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(player).unwrap().health = 50;
        let item = w
            .spawn_at(kit, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(w.actors.get(player).unwrap().health, 75);
        assert_eq!(w.drain_messages(), vec!["Picked up a medikit.".to_owned()]);

        w.actors.get_mut(player).unwrap().health = 20;
        let item2 = w
            .spawn_at(kit, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item2));
        assert_eq!(w.actors.get(player).unwrap().health, 45);
        assert_eq!(
            w.drain_messages(),
            vec!["Picked up a medikit that you REALLY need!".to_owned()]
        );
    }

    #[test]
    fn picks_up_health_bonus_past_100() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let bonus = classes.find_name("HealthBonus").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(w.actors.get(player).unwrap().health, 100);
        let items_before = w.level.found_items;
        let item = w
            .spawn_at(bonus, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!((w.actors.get(item).unwrap().flags & crate::class::FLAG_COUNTITEM) != 0);
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(w.actors.get(player).unwrap().health, 101);
        assert_eq!(w.level.found_items, items_before + 1);
        assert!(
            w.drain_messages()
                .iter()
                .any(|m| m.contains("health bonus"))
        );
    }

    #[test]
    fn picks_up_armor_bonus_grants_green_type() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let bonus = classes.find_name("ArmorBonus").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(w.players[0].as_ref().unwrap().armor, 0);
        assert_eq!(w.players[0].as_ref().unwrap().armor_class, 0);
        let item = w
            .spawn_at(bonus, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(w.players[0].as_ref().unwrap().armor, 1);
        assert_eq!(w.players[0].as_ref().unwrap().armor_class, 1);
        // Subsequent bonuses keep existing class (e.g. blue stays 2).
        w.players[0].as_mut().unwrap().armor_class = 2;
        w.players[0].as_mut().unwrap().armor = 50;
        let item2 = w
            .spawn_at(bonus, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item2));
        assert_eq!(w.players[0].as_ref().unwrap().armor, 51);
        assert_eq!(w.players[0].as_ref().unwrap().armor_class, 2);
    }

    #[test]
    fn picks_up_green_armor() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let armor = classes.find_name("GreenArmor").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(armor, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(w.players[0].as_ref().unwrap().armor, 100);
        assert!(w.actors.get(item).unwrap().destroyed);
        // Second green armor while already at 100 is refused.
        let item2 = w
            .spawn_at(armor, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(!try_pickup(&mut w, player, item2));
    }

    #[test]
    fn picks_up_clip() {
        let ir = parse_decorate(
            r#"
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
  Radius 20
  Height 16
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::new());
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let item = w
            .spawn_at(clip, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, mo, item));
        assert_eq!(count_inventory(&w, mo, clip), 10);
        assert!(w.actors.get(item).unwrap().destroyed);
    }

    #[test]
    fn invuln_gets_default_pickup_message() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let inv = classes.find_name("InvulnerabilitySphere").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(inv, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(
            w.drain_messages()
                .iter()
                .any(|m| m.contains("Invulnerability"))
        );
    }

    #[test]
    fn shotgun_gets_default_pickup_message() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let shotgun = classes.find_name("Shotgun").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(shotgun, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(w.drain_messages().iter().any(|m| m.contains("shotgun")));
    }

    #[test]
    fn keycard_gets_default_pickup_message() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let key = classes.find_name("BlueCard").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(key, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert!(
            w.drain_messages()
                .iter()
                .any(|m| m.contains("blue keycard"))
        );
    }

    #[test]
    fn pickup_message_queued_on_success() {
        let ir = parse_decorate(
            r#"
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
  Inventory.PickupMessage "Picked up a clip."
  Inventory.PickupSound "misc/i_pkup"
  Radius 20
  Height 16
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::new());
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let clip = classes.find_name("Clip").unwrap();
        assert_eq!(
            classes.get(clip).unwrap().pickup_message,
            "Picked up a clip."
        );
        assert_eq!(classes.get(clip).unwrap().pickup_sound, "misc/i_pkup");
        let item = w
            .spawn_at(clip, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, mo, item));
        assert_eq!(w.drain_messages(), vec!["Picked up a clip.".to_owned()]);
        let sounds = w.drain_sound_events();
        assert_eq!(sounds.len(), 1);
        assert_eq!(sounds[0].name, "misc/i_pkup");
        assert!((sounds[0].x - 40.0).abs() < 1e-9);
    }

    #[test]
    fn bex_strings_override_pickup_message() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let mut ir = parse_decorate(src).unwrap();
        let patch = ferrite_dehacked::parse_dehacked(
            r#"
[STRINGS]
GOTCLIP = BEX remapped clip pickup.
GOTARMOR = BEX remapped armor pickup.
"#,
        )
        .unwrap();
        ferrite_dehacked::apply_patch(&mut ir, &patch).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let clip = classes.find_name("Clip").unwrap();
        assert_eq!(
            classes.get(clip).unwrap().pickup_message,
            "BEX remapped clip pickup."
        );
        let item = w
            .spawn_at(clip, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, mo, item));
        assert_eq!(
            w.drain_messages(),
            vec!["BEX remapped clip pickup.".to_owned()]
        );

        let armor = classes.find_name("GreenArmor").unwrap();
        let armor_item = w
            .spawn_at(armor, DVec3::new(48.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, mo, armor_item));
        assert!(
            w.drain_messages()
                .iter()
                .any(|m| m == "BEX remapped armor pickup.")
        );
    }

    #[test]
    fn dropped_clip_gives_half_ammo() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let amount = classes.get(clip).unwrap().inventory_amount.max(1);
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let before = count_inventory(&w, player, clip);
        let item = w
            .spawn_at(clip, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        w.actors.get_mut(item).unwrap().flags |= crate::class::FLAG_DROPPED;
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(
            count_inventory(&w, player, clip),
            before + (amount / 2).max(1)
        );
    }
}
