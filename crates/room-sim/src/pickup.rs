//! Touch / pickup (`CallTryPickup` subset).

use crate::actor::ActorId;
use crate::inventory::give_inventory;
use crate::world::World;
use room_core::ClassId;

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
    if world.classes.inherits_from(class, "WeaponPiece") {
        return pickup_weapon_piece(world, toucher, item, class);
    }
    let amount = world
        .classes
        .get(class)
        .map(|c| c.inventory_amount.max(1))
        .unwrap_or(1);
    // Vanilla `MF_DROPPED`: ammo (and weapon ammo give) at half amount (min 1).
    // `Ammo.DropAmount` replaces the dropped amount; `DropAmmoFactorMultiplier`
    // scales Doom's half.
    let amount = if dropped {
        match world
            .classes
            .get(class)
            .map(|c| (c.ext.drop_amount, c.ext.drop_ammo_factor_mult))
        {
            Some((n, _)) if n > 0 => n,
            Some((_, m)) if (m - 1.0).abs() > 1e-12 => {
                ((f64::from(amount) * 0.5 * m) as i32).max(1)
            }
            _ => (amount / 2).max(1),
        }
    } else {
        amount
    };
    let class_name = pickup_kind(world, class);
    // Armor pickups set `Player.armor` instead of stacking as inventory.
    let armor_kind = armor_kind(&class_name);
    // Health pickups heal the toucher's actor (amount / max from class props).
    let health_heal = match class_name.as_str() {
        "stimpack" | "medikit" | "healthbonus" | "soulsphere" | "megasphere" => {
            let max = world
                .classes
                .get(class)
                .map(|c| c.inventory_max_amount)
                .unwrap_or(100)
                .max(1);
            // Stimpacks and medikits heal up to the player's `Player.MaxHealth`
            // (GZDoom `Health` with no `MaxAmount` uses `GetMaxHealth`).
            let player_max = world
                .actors
                .get(toucher)
                .and_then(|a| world.classes.get(a.class))
                .map_or(0, |c| c.ext.player_max_health);
            let max = if matches!(class_name.as_str(), "stimpack" | "medikit")
                && max == 100
                && player_max > 0
            {
                player_max
            } else {
                max
            };
            Some((amount, max))
        }
        _ => None,
    };
    let is_berserk = class_name == "berserk";
    let is_invuln = class_name == "invulnerabilitysphere";
    let is_blur = class_name == "blursphere";
    let is_radsuit = class_name == "radsuit";
    let is_flight = class_name == "artifly";
    let is_allmap = class_name == "allmap";
    let is_infrared = class_name == "infrared";
    let is_backpack = class_name == "backpack";
    // Medikit uses a different message when health was critically low.
    let mut medikit_needed = false;
    // `Health.LowMessage`: replaces the pickup message when health was low.
    let mut low_message: Option<String> = None;
    if world.classes.inherits_from(class, "HexenArmor") {
        // `HexenArmor.HandlePickup`: `Health` is the slot, `Amount` the AC
        // to add (0 = the class's full piece).
        let (slot, raw_amount) = world
            .classes
            .get(class)
            .map_or((0, 0), |c| (c.health, c.inventory_amount));
        if !add_hexen_armor(world, toucher, slot, raw_amount) {
            return false;
        }
    } else if armor_kind.is_some() {
        if !use_armor(world, toucher, class, 1) {
            return false;
        }
    } else if let Some((heal, max_hp)) = health_heal {
        let Some(a) = world.actors.get_mut(toucher) else {
            return false;
        };
        if class_name == "medikit" {
            medikit_needed = a.health < 25;
        }
        let low = world
            .classes
            .get(class)
            .filter(|c| !c.ext.low_message.is_empty())
            .map(|c| (c.ext.low_message_health, c.ext.low_message.clone()));
        if let Some((n, msg)) = low {
            if a.health < n {
                low_message = Some(msg);
            }
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
                    p.clear_armor_kind();
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
            p.berserk_tics = 1;
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
        world.push_message_kind(crate::world::MessageKind::Power, "Berserk!");
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
    } else if is_flight {
        // Flight still starts on pickup (`ArtiFly`); medkit / steroids wait for INVUSE.
        let mut applied = false;
        for p in world.players.iter_mut().flatten() {
            if p.mo != Some(toucher) {
                continue;
            }
            p.fly_tics = crate::player::FLIGHT_TICS;
            applied = true;
        }
        if !applied {
            return false;
        }
        if let Some(a) = world.actors.get_mut(toucher) {
            a.flags |= crate::class::FLAG_NOGRAVITY;
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
        backpack_fill(world, toucher);
        world.push_message_kind(crate::world::MessageKind::Pickup, "Picked up a backpack full of ammo!");
    } else if world.classes.get(class).is_some_and(|c| c.is_weapon) {
        // docs/30 W17: weapon stay — each player may take a map weapon once;
        // the actor stays for others. Dropped weapons always vanish.
        let stay = world.weapon_stay && !dropped;
        let already = world.inventory.count(toucher, class) > 0;
        if stay && already {
            return false;
        }
        if !already && !give_inventory(world, toucher, class, amount) {
            return false;
        }
        let (ammo_name, ammo_give, ammo_name2, ammo_give2) = world
            .classes
            .get(class)
            .map(|c| {
                (
                    c.ammo_type.clone(),
                    c.ammo_give,
                    c.ammo_type2.clone(),
                    c.ammo_give2,
                )
            })
            .unwrap_or_default();
        let mut gave_ammo = false;
        if ammo_give > 0 && !ammo_name.is_empty() {
            if let Some(ammo) = world.classes.find_name(&ammo_name) {
                let give = skill_ammo(
                    world,
                    if dropped {
                        (ammo_give / 2).max(1)
                    } else {
                        ammo_give
                    },
                );
                let before = world.inventory.count(toucher, ammo);
                gave_ammo = give_inventory(world, toucher, ammo, give);
                if std::env::var_os("ROOM_TRACE_AMMO").is_some() {
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
        if ammo_give2 > 0 && !ammo_name2.is_empty() {
            if let Some(ammo) = world.classes.find_name(&ammo_name2) {
                let give = skill_ammo(
                    world,
                    if dropped {
                        (ammo_give2 / 2).max(1)
                    } else {
                        ammo_give2
                    },
                );
                if give_inventory(world, toucher, ammo, give) {
                    gave_ammo = true;
                }
            }
        }
        if already && !gave_ammo {
            return false;
        }
        if !already {
            switch_player_weapon(world, toucher, class);
        }
        if stay {
            // Vanilla `P_GiveWeapon` returns false so TouchSpecialThing leaves
            // the mobj (no remove, no item count). Still show pickup FX.
            let (pickup_msg, pickup_snd) = world
                .classes
                .get(class)
                .map(|c| (c.pickup_message.clone(), c.pickup_sound.clone()))
                .unwrap_or_default();
            if !pickup_msg.is_empty() {
                world.push_message_kind(crate::world::MessageKind::Pickup, pickup_msg);
            }
            if !pickup_snd.is_empty() {
                if let Some(a) = world.actors.get(toucher) {
                    world.play_sound(&pickup_snd, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
                }
            }
            // Return false so callers treat this like "not consumed".
            return false;
        }
    } else if is_custom_inventory(world, class)
        && world.states.find_label(class, "Pickup").is_some()
    {
        // Brutal Doom map guns are `CustomInventory` spawners. The Pickup
        // state gives the real weapon; stocking the spawner itself does not.
        if !run_custom_pickup(world, toucher, item) {
            return false;
        }
    } else if let Some(ammo_name) = ammo_box_grants(&class_name) {
        // Boxes are separate DoomEdNums but feed the base ammo class.
        let Some(ammo) = world.classes.find_name(ammo_name) else {
            return false;
        };
        if std::env::var_os("ROOM_TRACE_AMMO").is_some() {
            eprintln!(
                "AMMOBOX tic={} item={} -> {} amount={}",
                world.level.time, class_name, ammo_name, amount
            );
        }
        if !give_inventory(world, toucher, ammo, skill_ammo(world, amount)) {
            return false;
        }
    } else if !give_inventory(
        world,
        toucher,
        flechette_for(world, toucher, class)
            .unwrap_or_else(|| base_ammo(world, class).unwrap_or(class)),
        if class_is_ammo(world, class) {
            skill_ammo(world, amount)
        } else {
            amount
        },
    ) {
        return false;
    } else if std::env::var_os("ROOM_TRACE_AMMO").is_some()
        && (class_name.contains("shell") || class_name.contains("clip"))
    {
        eprintln!(
            "AMMOPICK tic={} item={} amount={}",
            world.level.time, class_name, amount
        );
    }
    if crate::action::apply_powerup_giver(world, toucher, class)
        && crate::powerups::is_generic_giver(world, class)
    {
        // An auto-activating giver is used up on pickup, not carried.
        let _ = world.inventory.take(toucher, class, 1);
    }
    let (countitem, countsecret) = world.actors.get(item).map_or((false, false), |a| {
        (
            (a.flags & crate::class::FLAG_COUNTITEM) != 0,
            a.count_secret,
        )
    });
    // `Inventory.PickupFlash`: a flash actor where the item stood.
    let flash = world
        .classes
        .get(class)
        .map(|c| c.ext.pickup_flash.clone())
        .filter(|f| !f.is_empty() && !f.eq_ignore_ascii_case("none"));
    if let Some(fcid) = flash.and_then(|f| world.classes.find_name(&f)) {
        let _ = world.spawn_at(fcid, it.pos, room_core::Angle::ZERO);
    }
    crate::world::queue_item_respawn(world, item);
    // `Inventory.GiveQuest n`: Strife quest token `QuestItemN`.
    if let Some(q) = world
        .classes
        .get(class)
        .and_then(|c| c.ext.stored_i32("inventory.givequest"))
        .filter(|&q| q > 0)
    {
        if let Some(qc) = world.classes.find_name(&format!("QuestItem{q}")) {
            let _ = give_inventory(world, toucher, qc, 1);
        }
    }
    // Consume the world item.
    if let Some(a) = world.actors.get_mut(item) {
        a.destroyed = true;
        a.state = None;
        a.flags = 0;
        a.count_secret = false;
    }
    if countitem {
        world.level.found_items = world.level.found_items.saturating_add(1);
        world.credit_player_stat(Some(toucher), 1);
    }
    if countsecret {
        world.level.found_secrets = world.level.found_secrets.saturating_add(1);
        world.credit_player_stat(Some(toucher), 2);
    }
    let (pickup_msg, pickup_snd) = world
        .classes
        .get(class)
        .map(|c| (c.pickup_message.clone(), c.pickup_sound.clone()))
        .unwrap_or_default();
    if let Some(msg) = low_message {
        world.push_message_kind(crate::world::MessageKind::Pickup, msg);
    } else if !pickup_msg.is_empty() {
        world.push_message_kind(crate::world::MessageKind::Pickup, pickup_msg);
    } else if class_name == "medikit" {
        // Vanilla: `GOTMEDINEED` when health was below 25 before the heal.
        world.push_message_kind(crate::world::MessageKind::Pickup, if medikit_needed {
            "Picked up a medikit that you REALLY need!"
        } else {
            "Picked up a medikit."
        });
    } else if let Some(msg) = default_pickup_message(&class_name) {
        world.push_message_kind(crate::world::MessageKind::Pickup, msg);
    }
    if !pickup_snd.is_empty() {
        world.play_sound(&pickup_snd, it.pos.x(), it.pos.y(), it.pos.z(), 1.0);
    }
    if let Some(p) = world
        .players
        .iter_mut()
        .flatten()
        .find(|p| p.mo == Some(toucher))
    {
        p.bonus_count += crate::player::BONUS_ADD;
    }
    world.push_script_event(crate::script_mods::ScriptEvent::ItemPickup {
        toucher,
        item_class: class,
    });
    true
}

/// Begin a raise/lower weapon switch for the player owning `toucher`.
fn switch_player_weapon(world: &mut World, toucher: ActorId, weapon: room_core::ClassId) {
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
/// Vanilla `P_GiveAmmo`: I'm Too Young To Die and Nightmare give double ammo.
pub(crate) fn skill_ammo(world: &World, amount: i32) -> i32 {
    if matches!(world.skill, 1 | 5) {
        amount.saturating_mul(2)
    } else {
        amount
    }
}

/// Armor family of a pickup kind: `Some(false)` for `BasicArmorPickup`
/// (green / blue), `Some(true)` for `BasicArmorBonus`.
fn armor_kind(kind: &str) -> Option<bool> {
    match kind {
        "greenarmor" | "bluearmor" | "basicarmorpickup" => Some(false),
        "armorbonus" | "basicarmorbonus" => Some(true),
        _ => None,
    }
}

/// True when `class` is armor (`BasicArmorPickup` / `BasicArmorBonus` or a
/// Doom armor subclass).
#[must_use]
pub fn is_armor_class(world: &World, class: ClassId) -> bool {
    armor_kind(&pickup_kind(world, class)).is_some()
}

/// `BasicArmorPickup.Use` / `BasicArmorBonus.Use` for `owner`'s player.
///
/// `count` multiplies `Armor.SaveAmount` (`SetGiveAmount`, as
/// `A_GiveInventory("GreenArmor", 2)` does). Returns whether the item was used
/// up: a full armor pickup is left on the floor; an armor bonus is always
/// taken (`+INVENTORY.ALWAYSPICKUP`, and vanilla Doom).
pub fn use_armor(world: &mut World, owner: ActorId, class: ClassId, count: i32) -> bool {
    let kind = pickup_kind(world, class);
    let Some(bonus) = armor_kind(&kind) else {
        return false;
    };
    let Some(def) = world.classes.get(class) else {
        return false;
    };
    let ext = &def.ext;
    // Room's Doom armors predate the `Armor.*` properties: fall back to the
    // GZDoom defaults (`GreenArmor` 100 @ 33.335 %, `BlueArmor` 200 @ 50 %,
    // `ArmorBonus` 1 @ 33.335 % up to 200).
    let vanilla = match kind.as_str() {
        "greenarmor" => (100, 33.335, 0),
        "bluearmor" => (200, 50.0, 0),
        "armorbonus" => (1, 33.335, 200),
        _ => (0, 0.0, 0),
    };
    let save_amount = if ext.armor_save_amount > 0 {
        ext.armor_save_amount
    } else {
        vanilla.0
    };
    let save_amount = save_amount.saturating_mul(count.max(1));
    // `GetSaveAmount`: skill `ArmorFactor` unless `+INVENTORY.IGNORESKILL`.
    let save_amount = if (world.armor_factor - 1.0).abs() > 1e-12
        && !ext.has_qualified_flag("INVENTORY.IGNORESKILL")
    {
        (f64::from(save_amount) * world.armor_factor) as i32
    } else {
        save_amount
    };
    let percent = if ext.armor_save_percent > 0.0 {
        ext.armor_save_percent
    } else {
        vanilla.1
    };
    let max_save = if ext.armor_max_save_amount > 0 {
        ext.armor_max_save_amount
    } else {
        vanilla.2
    };
    let (bonus_count, bonus_max) = (ext.armor_max_bonus, ext.armor_max_bonus_max);
    let max_absorb = ext.armor_max_absorb;
    let max_full = ext.armor_max_full_absorb;
    let fraction = percent.clamp(0.0, 100.0) / 100.0;
    let armor_class = if fraction >= 0.5 { 2 } else { 1 };
    let Some(p) = world
        .players
        .iter_mut()
        .flatten()
        .find(|p| p.mo == Some(owner))
    else {
        return false;
    };
    let wear = |p: &mut crate::player::Player| {
        p.armor_save_percent = fraction;
        p.armor_max_absorb = max_absorb;
        p.armor_max_full_absorb = max_full;
        p.armor_type = Some(class);
        p.armor_class = armor_class;
    };
    if bonus {
        // `Armor.MaxBonus` raises the cap by `BonusCount` up to `MaxBonusMax`.
        if bonus_count > 0 && p.armor_bonus_count < bonus_max {
            p.armor_bonus_count = (p.armor_bonus_count + bonus_count).min(bonus_max);
        }
        let save = save_amount.min(max_save);
        let cap = max_save + p.armor_bonus_count;
        if save <= 0 || p.armor >= cap {
            return true;
        }
        if p.armor <= 0 {
            p.armor = 0;
            wear(p);
        }
        p.armor = (p.armor + save).min(cap);
        return true;
    }
    if p.armor >= save_amount + p.armor_bonus_count {
        return false;
    }
    p.armor = save_amount + p.armor_bonus_count;
    wear(p);
    true
}

/// `Player.HexenArmor` of `owner`'s class: natural AC (slot 4) and the four
/// slot increments, all × 5 as in UZDoom (`HexenArmor.Slots`).
#[must_use]
pub fn hexen_armor_profile(world: &World, owner: ActorId) -> Option<(f64, [f64; 4])> {
    let class = world.actors.get(owner)?.class;
    let mut cur = Some(class);
    for _ in 0..32 {
        let def = cur.and_then(|c| world.classes.get(c))?;
        if let Some(raw) = def.ext.stored("player.hexenarmor") {
            let v: Vec<f64> = raw
                .split(',')
                .filter_map(|t| t.trim().trim_matches('"').parse().ok())
                .collect();
            if v.len() >= 5 {
                return Some((v[0], [v[1], v[2], v[3], v[4]]));
            }
        }
        cur = def.parent;
    }
    None
}

/// `HexenArmor.AddArmorToSlot(slot, amount)` for `owner`'s player. `amount`
/// 0 = a full piece (the slot's increment); otherwise `amount × 5` AC points
/// up to the class maximum (Dragonskin Bracers).
pub fn add_hexen_armor(world: &mut World, owner: ActorId, slot: i32, amount: i32) -> bool {
    let Ok(slot) = usize::try_from(slot) else {
        return false;
    };
    if slot > 3 {
        return false;
    }
    let (base, inc) = hexen_armor_profile(world, owner).unwrap_or((0.0, [0.0; 4]));
    let Some(p) = world
        .players
        .iter_mut()
        .flatten()
        .find(|p| p.mo == Some(owner))
    else {
        return false;
    };
    if amount <= 0 {
        if p.hexen_armor[slot] < inc[slot] {
            p.hexen_armor[slot] = inc[slot];
            return true;
        }
        return false;
    }
    let total: f64 = p.hexen_armor.iter().sum::<f64>() + base;
    let max = inc.iter().sum::<f64>() + base + 4.0 * 5.0;
    if total < max {
        p.hexen_armor[slot] += f64::from(amount) * 5.0;
        return true;
    }
    false
}

/// `HexenArmor.AbsorbDamage`: returns the damage left. Each worn piece wears
/// down by `damage × increment / 300` (gone below 2); 10000+ damage strips it.
pub(crate) fn absorb_hexen_armor(world: &mut World, target: ActorId, damage: i32) -> i32 {
    let Some((base, inc)) = hexen_armor_profile(world, target) else {
        return damage;
    };
    let Some(p) = world
        .players
        .iter_mut()
        .flatten()
        .find(|p| p.mo == Some(target))
    else {
        return damage;
    };
    let mut saved_percent: f64 = p.hexen_armor.iter().sum::<f64>() + base;
    if saved_percent <= 0.0 {
        return damage;
    }
    saved_percent = saved_percent.min(100.0);
    for (i, slot) in p.hexen_armor.iter_mut().enumerate() {
        if *slot != 0.0 {
            if damage < 10_000 {
                *slot -= f64::from(damage) * inc[i] / 300.0;
                if *slot < 2.0 {
                    *slot = 0.0;
                }
            } else {
                *slot = 0.0;
            }
        }
    }
    let mut saved = (f64::from(damage) * saved_percent / 100.0) as i32;
    if f64::from(saved) > saved_percent * 2.0 {
        saved = (saved_percent * 2.0) as i32;
    }
    damage - saved
}

/// Hexen armor class shown on the status bar (`(Σ slots) / 5`).
#[must_use]
pub fn hexen_armor_class(world: &World, owner: ActorId) -> i32 {
    let Some((base, _)) = hexen_armor_profile(world, owner) else {
        return 0;
    };
    let worn: f64 = world
        .players
        .iter()
        .flatten()
        .find(|p| p.mo == Some(owner))
        .map_or(0.0, |p| p.hexen_armor.iter().sum());
    ((worn + base) / 5.0) as i32
}

/// Built-in pickups whose effect is keyed by class name.
const PICKUP_KINDS: [&str; 22] = [
    "greenarmor",
    "bluearmor",
    "armorbonus",
    "basicarmorpickup",
    "basicarmorbonus",
    "stimpack",
    "medikit",
    "healthbonus",
    "soulsphere",
    "megasphere",
    "berserk",
    "invulnerabilitysphere",
    "blursphere",
    "radsuit",
    "artifly",
    "allmap",
    "infrared",
    "backpack",
    "clipbox",
    "shellbox",
    "rocketbox",
    "cellpack",
];

/// Lowercase name of the nearest built-in pickup `class` derives from, so a
/// DECORATE `actor Cola : Stimpack` heals like a stimpack; otherwise the
/// class's own name.
pub(crate) fn pickup_kind(world: &World, class: ClassId) -> String {
    let own = world
        .classes
        .get(class)
        .map(|c| c.name.to_ascii_lowercase())
        .unwrap_or_default();
    let mut cur = Some(class);
    for _ in 0..64 {
        let Some(def) = cur.and_then(|c| world.classes.get(c)) else {
            break;
        };
        let name = def.name.to_ascii_lowercase();
        if PICKUP_KINDS.contains(&name.as_str()) {
            return name;
        }
        cur = def.parent;
    }
    own
}

/// The weapon ammo type `class` is (or derives from): picking up a subclass of
/// `Clip` fills the `Clip` stack, as in GZDoom.
fn base_ammo(world: &World, class: ClassId) -> Option<ClassId> {
    let mut cur = Some(class);
    for _ in 0..64 {
        let def = cur.and_then(|c| world.classes.get(c))?;
        if world
            .classes
            .iter()
            .any(|c| c.is_weapon && c.ammo_type.eq_ignore_ascii_case(&def.name))
        {
            return Some(def.id);
        }
        cur = def.parent;
    }
    None
}

/// Base ammo types a backpack fills: classes right under `Ammo`, plus Room's
/// Doom ammo (which derive from `Actor`), never their subclasses.
fn backpack_ammo_types(world: &World) -> Vec<ClassId> {
    let mut out = Vec::new();
    for def in world.classes.iter() {
        if def.name.eq_ignore_ascii_case("Ammo") || !class_is_ammo(world, def.id) {
            continue;
        }
        let parent_is_ammo = def.parent.is_some_and(|pid| {
            world
                .classes
                .get(pid)
                .is_some_and(|p| !p.name.eq_ignore_ascii_case("Ammo"))
                && class_is_ammo(world, pid)
        });
        if !parent_is_ammo {
            out.push(def.id);
        }
    }
    out
}

/// `BackpackItem.CreateCopy`: every base ammo type gets `Ammo.BackpackAmount`
/// (doubled on skills 1 and 5) and its cap raised to `Ammo.BackpackMaxAmount`.
///
/// Room's Doom ammo leaves those unset; it falls back to vanilla `P_GiveAmmo
/// (…, 1)` (one clip: the class `Inventory.Amount`) and a doubled cap.
fn backpack_fill(world: &mut World, toucher: ActorId) {
    for cid in backpack_ammo_types(world) {
        let Some(def) = world.classes.get(cid) else {
            continue;
        };
        let vanilla = matches!(
            def.name.to_ascii_lowercase().as_str(),
            "clip" | "shell" | "rocketammo" | "cell"
        );
        let (give, max) =
            if vanilla && def.ammo_backpack_amount <= 0 && def.ammo_backpack_max_amount <= 0 {
                (
                    def.inventory_amount.max(1),
                    def.inventory_max_amount.saturating_mul(2),
                )
            } else {
                (
                    def.ammo_backpack_amount.max(0),
                    def.ammo_backpack_max_amount,
                )
            };
        let give = skill_ammo(world, give);
        let class_max = def.inventory_max_amount.max(1);
        let max = max.max(class_max);
        if let Some(item) = world.inventory.find_mut(toucher, cid) {
            item.max_amount = item.max_amount.max(max);
        }
        if give > 0 {
            let _ = world.inventory.give(toucher, cid, give, max);
        }
    }
}

fn class_is_ammo(world: &World, class: ClassId) -> bool {
    let Some(name) = world.classes.get(class).map(|c| c.name.as_str()) else {
        return false;
    };
    if world
        .classes
        .iter()
        .any(|c| c.is_weapon && c.ammo_type.eq_ignore_ascii_case(name))
    {
        return true;
    }
    let mut cur = Some(class);
    for _ in 0..64 {
        let Some(def) = cur.and_then(|c| world.classes.get(c)) else {
            return false;
        };
        if def.name.eq_ignore_ascii_case("Ammo") {
            return true;
        }
        cur = def.parent;
    }
    false
}

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
        "artifly" => "Wings of Wrath",
        "allmap" => "Computer Area Map",
        "infrared" => "Light Amplification Visor",
        _ => return None,
    })
}

fn is_custom_inventory(world: &World, class: room_core::ClassId) -> bool {
    let mut cur = Some(class);
    for _ in 0..32 {
        let Some(def) = cur.and_then(|c| world.classes.get(c)) else {
            return false;
        };
        if def.name.eq_ignore_ascii_case("CustomInventory") {
            return true;
        }
        cur = def.parent;
    }
    false
}

/// Run the `Pickup` state. Inventory actions apply to `toucher`, not the item.
fn run_custom_pickup(world: &mut World, toucher: ActorId, item: ActorId) -> bool {
    let Some((class, pos, angle)) = world.actors.get(item).map(|a| (a.class, a.pos, a.angle))
    else {
        return false;
    };
    run_custom_pickup_at(world, toucher, class, pos, angle)
}

/// Execute a `CustomInventory` Pickup state for an item granted directly by
/// DECORATE (`A_GiveToTarget` / `A_GiveInventory`). GZDoom runs the item's
/// pickup actions with its owner as target/activator even though there is no
/// map pickup actor.
pub(crate) fn run_given_custom_pickup(
    world: &mut World,
    owner: ActorId,
    class: room_core::ClassId,
) {
    let Some((pos, angle)) = world.actors.get(owner).map(|a| (a.pos, a.angle)) else {
        return;
    };
    let _ = run_custom_pickup_at(world, owner, class, pos, angle);
}

fn run_custom_pickup_at(
    world: &mut World,
    owner: ActorId,
    class: room_core::ClassId,
    pos: room_core::DVec3,
    angle: room_core::Angle,
) -> bool {
    run_custom_state_at(world, owner, class, pos, angle, "Pickup")
}

/// `UseInventory(class)`: run a `CustomInventory`'s `Use` state for `owner`
/// (or wear armor), play `Inventory.UseSound`, and take one. Returns false when
/// `owner` has none or the item has no use.
pub fn use_inventory_item(world: &mut World, owner: ActorId, class: room_core::ClassId) -> bool {
    if world.inventory.count(owner, class) <= 0 {
        return false;
    }
    let used = if is_armor_class(world, class) {
        use_armor(world, owner, class, 1)
    } else if world.classes.inherits_from(class, "ArtiBoostArmor") {
        // Dragonskin Bracers: one AC point (×5) in each of the four slots.
        let mut any = false;
        for slot in 0..4 {
            any |= add_hexen_armor(world, owner, slot, 1);
        }
        any
    } else if world.classes.inherits_from(class, "PuzzleItem") {
        use_puzzle_item(world, owner, class)
    } else if world.classes.inherits_from(class, "ArtiHealingRadius") {
        use_healing_radius(world, owner)
    } else if world.classes.inherits_from(class, "HealthPickup") {
        use_health_pickup(world, owner, class)
    } else if world.classes.inherits_from(class, "PowerupGiver") {
        crate::powerups::give_power(world, owner, class)
    } else if let Some((pos, angle)) = world.actors.get(owner).map(|a| (a.pos, a.angle)) {
        run_custom_state_at(world, owner, class, pos, angle, "Use")
    } else {
        false
    };
    if !used {
        return false;
    }
    let sound = world
        .classes
        .get(class)
        .map(|c| c.ext.use_sound.clone())
        .unwrap_or_default();
    if !sound.is_empty() {
        if let Some(a) = world.actors.get(owner) {
            world.play_sound(&sound, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
    }
    let _ = world.inventory.take(owner, class, 1);
    true
}

/// `WeaponPiece.TryPickup` (Hexen fourth weapons): give the whole weapon's
/// ammo, mark piece `WeaponPiece.Number` in the toucher's holder for
/// `WeaponPiece.Weapon`, and when the mask holds all `Health` pieces of that
/// weapon class, give the weapon (with no extra ammo). A piece already held is
/// still taken if it gave ammo.
fn pickup_weapon_piece(world: &mut World, toucher: ActorId, item: ActorId, class: ClassId) -> bool {
    let (weapon, number) = world.classes.get(class).map_or((String::new(), 0), |c| {
        (
            c.ext.stored("weaponpiece.weapon").unwrap_or("").to_owned(),
            c.ext.stored_i32("weaponpiece.number").unwrap_or(0),
        )
    });
    let Some(wcid) = world
        .classes
        .find_name(&weapon)
        .map(|c| world.classes.replacement(c))
    else {
        return false;
    };
    let Some(wdef) = world.classes.get(wcid).cloned() else {
        return false;
    };
    let mut gave = false;
    for (ammo, n) in [
        (&wdef.ammo_type, wdef.ammo_give),
        (&wdef.ammo_type2, wdef.ammo_give2),
    ] {
        if n <= 0 || ammo.is_empty() {
            continue;
        }
        if let Some(acid) = world.classes.find_name(ammo) {
            let before = world.inventory.count(toucher, acid);
            let _ = give_inventory(world, toucher, acid, n);
            gave |= world.inventory.count(toucher, acid) > before;
        }
    }
    let key = format!("piecemask:{}", wdef.name.to_ascii_lowercase());
    let piece = if (1..=31).contains(&number) {
        1i64 << (number - 1)
    } else {
        0
    };
    let mask = world
        .actors
        .get(toucher)
        .and_then(|a| a.ext.scratch.get(&key).copied())
        .unwrap_or(0.0) as i64;
    if mask & piece != 0 {
        if !gave {
            return false;
        }
    } else {
        let mask = mask | piece;
        if let Some(a) = world.actors.get_mut(toucher) {
            a.ext.scratch.insert(key, mask as f64);
        }
        let pieces = wdef.health.clamp(1, 31);
        let full = (1i64 << pieces) - 1;
        if mask == full && world.inventory.count(toucher, wcid) <= 0 {
            let _ = world
                .inventory
                .give(toucher, wcid, 1, wdef.inventory_max_amount.max(1));
            if let Some(p) = world
                .players
                .iter_mut()
                .flatten()
                .find(|p| p.mo == Some(toucher))
            {
                p.pending_weapon = Some(wcid);
            }
            if !wdef.pickup_message.is_empty() {
                world.push_message_kind(crate::world::MessageKind::Pickup, wdef.pickup_message.clone());
            }
        }
    }
    let (msg, snd, pos) = world
        .classes
        .get(class)
        .map(|c| {
            (
                c.pickup_message.clone(),
                c.pickup_sound.clone(),
                world.actors.get(item).map(|a| a.pos),
            )
        })
        .unwrap_or_default();
    if !msg.is_empty() {
        world.push_message_kind(crate::world::MessageKind::Pickup, msg);
    }
    if let (false, Some(p)) = (snd.is_empty(), pos) {
        world.play_sound(&snd, p.x(), p.y(), p.z(), 1.0);
    }
    if let Some(a) = world.actors.get_mut(item) {
        a.destroyed = true;
        a.state = None;
        a.flags = 0;
    }
    true
}

/// Hexen `ArtiPoisonBag.CreateCopy`: the plain flechette becomes the
/// toucher class's `Player.FlechetteType` (default `ArtiPoisonBag3`).
fn flechette_for(world: &World, toucher: ActorId, class: ClassId) -> Option<ClassId> {
    if !world
        .classes
        .get(class)?
        .name
        .eq_ignore_ascii_case("ArtiPoisonBag")
    {
        return None;
    }
    let name = world
        .actors
        .get(toucher)
        .and_then(|a| world.classes.get(a.class))
        .and_then(|c| c.ext.stored("player.flechettetype"))
        .unwrap_or("ArtiPoisonBag3")
        .to_owned();
    world.classes.find_name(&name)
}

/// `ArtiHealingRadius.Use`: every live player within 255 units gets the
/// owner class's `Player.HealRadiusType` — `Health` (50..99 health, the
/// default), `Mana` (50..99 of `Mana1` or `Mana2`) or `Armor` (Room has no
/// Hexen armor pieces: 5 armor points per piece, four pieces).
fn use_healing_radius(world: &mut World, owner: ActorId) -> bool {
    let mode = world
        .actors
        .get(owner)
        .and_then(|a| world.classes.get(a.class))
        .and_then(|c| c.ext.stored("player.healradiustype"))
        .unwrap_or("Health")
        .trim()
        .to_ascii_lowercase();
    let Some(origin) = world.actors.get(owner).map(|a| a.pos) else {
        return false;
    };
    let mos: Vec<ActorId> = world
        .players
        .iter()
        .flatten()
        .filter_map(|p| p.mo)
        .collect();
    let mut effective = false;
    for mo in mos {
        let Some(a) = world.actors.get(mo) else {
            continue;
        };
        let (dx, dy) = (a.pos.x() - origin.x(), a.pos.y() - origin.y());
        if a.health <= 0 || (dx * dx + dy * dy).sqrt() > 255.0 {
            continue;
        }
        let amount = 50 + i32::from(world.p_random("pr_healradius")) % 50;
        let got = match mode.as_str() {
            "armor" => world
                .players
                .iter_mut()
                .flatten()
                .find(|p| p.mo == Some(mo))
                .is_some_and(|p| {
                    if p.armor >= 100 {
                        return false;
                    }
                    p.armor = (p.armor + 20).min(100);
                    if p.armor_class == 0 {
                        p.armor_class = 1;
                    }
                    true
                }),
            "mana" => ["Mana1", "Mana2"].iter().fold(false, |got, m| {
                world
                    .classes
                    .find_name(m)
                    .is_some_and(|c| give_inventory(world, mo, c, amount))
                    || got
            }),
            _ => world.actors.get_mut(mo).is_some_and(|a| {
                if a.health >= 100 {
                    return false;
                }
                a.health = (a.health + amount).min(100);
                true
            }),
        };
        if got {
            if let Some(a) = world.actors.get(mo) {
                world.play_sound("MysticIncant", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
            }
            effective = true;
        }
    }
    effective
}

/// `PuzzleItem.Use`: try the `UsePuzzleItem` special in front of the owner;
/// on failure play `PuzzleItem.FailSound` (default `*puzzfail`) and print
/// `PuzzleItem.FailMessage` (default `$TXT_USEPUZZLEFAILED`).
fn use_puzzle_item(world: &mut World, owner: ActorId, class: ClassId) -> bool {
    let (number, fail_sound, fail_msg) =
        world
            .classes
            .get(class)
            .map_or((0, String::new(), String::new()), |c| {
                (
                    c.ext.stored_i32("puzzleitem.number").unwrap_or(0),
                    c.ext
                        .stored("puzzleitem.failsound")
                        .unwrap_or("*puzzfail")
                        .to_owned(),
                    c.ext
                        .stored("puzzleitem.failmessage")
                        .unwrap_or("$TXT_USEPUZZLEFAILED")
                        .to_owned(),
                )
            });
    if crate::use_lines::use_puzzle_item(world, owner, number) {
        return true;
    }
    if let Some(a) = world.actors.get(owner) {
        world.play_sound(&fail_sound, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
    }
    if world.players.iter().flatten().any(|p| p.mo == Some(owner)) {
        world.push_message_kind(crate::world::MessageKind::Lock, fail_msg);
    }
    false
}

/// `HealthPickup.Use`: heal the class `Health` up to the owner's max health;
/// refused at full health.
fn use_health_pickup(world: &mut World, owner: ActorId, class: ClassId) -> bool {
    let heal = world.classes.get(class).map_or(0, |c| c.health.max(1));
    let max = world
        .actors
        .get(owner)
        .and_then(|a| world.classes.get(a.class))
        .map_or(100, |c| {
            if c.ext.player_max_health > 0 {
                c.ext.player_max_health
            } else {
                100
            }
        });
    let Some(a) = world.actors.get_mut(owner) else {
        return false;
    };
    if a.health >= max {
        return false;
    }
    a.health = (a.health + heal).min(max);
    true
}

fn run_custom_state_at(
    world: &mut World,
    owner: ActorId,
    class: room_core::ClassId,
    pos: room_core::DVec3,
    angle: room_core::Angle,
    label: &str,
) -> bool {
    if !is_custom_inventory(world, class) {
        return false;
    }
    let Some(start) = world.states.find_label(class, label) else {
        return false;
    };
    let Ok(item) = world.spawn_at(class, pos, angle) else {
        return false;
    };
    if let Some(actor) = world.actors.get_mut(item) {
        actor.target = Some(owner);
        actor.flags = 0;
        actor
            .user_vars
            .insert("room_internal_temp_custom_inventory".into(), 1);
    }
    world.set_state(item, Some(start), true);
    true
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
        let kind = pickup_kind(world, other.class);
        let is_armor = armor_kind(&kind).is_some();
        let is_health = matches!(
            kind.as_str(),
            "stimpack"
                | "medikit"
                | "healthbonus"
                | "soulsphere"
                | "megasphere"
                | "berserk"
                | "invulnerabilitysphere"
                | "blursphere"
                | "radsuit"
                | "artifly"
                | "allmap"
                | "infrared"
                | "backpack"
        );
        let is_key = matches!(
            kind.as_str(),
            "bluecard" | "yellowcard" | "redcard" | "blueskull" | "yellowskull" | "redskull"
        );
        let custom_pickup = is_custom_inventory(world, other.class)
            && world.states.find_label(other.class, "Pickup").is_some();
        if !custom_pickup
            && !def.is_weapon
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
            if std::env::var_os("ROOM_TRACE_AMMO").is_some()
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
    use room_core::{Angle, DVec3};
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};
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
    fn artifly_grants_timed_flight_driven_by_up_move() {
        let src = format!(
            "{}\nactor ArtiFly : Actor {{ Inventory.Amount 1 States {{ Spawn: TNT1 A -1 Stop }} }}\n",
            include_str!("../../../testdata/actors/doom_basic.txt")
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 256.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let fly = classes.find_name("ArtiFly").unwrap();
        let player = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(fly, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, player, item));
        assert_eq!(
            w.players[0].as_ref().unwrap().fly_tics,
            crate::player::FLIGHT_TICS
        );
        w.players[0].as_mut().unwrap().cmd.up_move = 768;
        for _ in 0..10 {
            w.tick();
        }
        let z = w.actors.get(player).unwrap().pos.z();
        assert!(z > 40.0, "up_move should fly the player up, z={z}");
        w.players[0].as_mut().unwrap().cmd.up_move = 0;
        for _ in 0..35 {
            w.tick();
        }
        let hover = w.actors.get(player).unwrap().pos.z();
        assert!(hover > 40.0, "a flying player hovers, z={hover}");
        w.players[0].as_mut().unwrap().fly_tics = 1;
        for _ in 0..70 {
            w.tick();
        }
        assert_eq!(
            w.actors.get(player).unwrap().flags & crate::class::FLAG_NOGRAVITY,
            0
        );
        assert!(
            w.actors.get(player).unwrap().pos.z() < 1.0,
            "flight ended: fall"
        );
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

    fn doom_world_with(extra: &str) -> (World, Arc<ClassRegistry>, ActorId) {
        let src = format!(
            "{}\n{extra}",
            include_str!("../../../testdata/actors/doom_basic.txt")
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        (w, classes, mo)
    }

    const ARMOR_BASES: &str = "actor Inventory : Actor {}\n\
        actor BasicArmorPickup : Inventory {}\n\
        actor BasicArmorBonus : Inventory {}\n";

    #[test]
    fn hexen_armor_slots_fill_absorb_and_wear_down() {
        let (mut w, classes, mo) = doom_world_with(&format!(
            "{ARMOR_BASES}actor HexenArmor : Inventory {{}}\n\
             actor TestMesh : HexenArmor {{ Health 0 Inventory.Amount 0 }}\n\
             actor TestAmulet : HexenArmor {{ Health 3 Inventory.Amount 0 }}\n\
             actor ArtiBoostArmor : Inventory {{}}\n\
             actor TestFighter : DoomPlayer {{ Player.HexenArmor 15, 25, 20, 15, 5 }}\n"
        ));
        let fighter = classes.find_name("TestFighter").unwrap();
        w.actors.get_mut(mo).unwrap().class = fighter;
        assert_eq!(
            hexen_armor_profile(&w, mo),
            Some((15.0, [25.0, 20.0, 15.0, 5.0]))
        );
        assert_eq!(hexen_armor_class(&w, mo), 3, "natural AC 15 / 5");
        let mesh = classes.find_name("TestMesh").unwrap();
        let item = w
            .spawn_at(mesh, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, mo, item));
        assert_eq!(w.players[0].as_ref().unwrap().hexen_armor[0], 25.0);
        let again = w
            .spawn_at(mesh, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(
            !try_pickup(&mut w, mo, again),
            "a full piece is not taken twice"
        );
        let amulet = classes.find_name("TestAmulet").unwrap();
        let item = w
            .spawn_at(amulet, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, mo, item));
        assert_eq!(hexen_armor_class(&w, mo), 9, "(15 + 25 + 5) / 5");
        // 45 % saved; mesh wears by 30 × 25 / 300 = 2.5.
        let left = absorb_hexen_armor(&mut w, mo, 30);
        assert_eq!(left, 30 - 13);
        let slots = w.players[0].as_ref().unwrap().hexen_armor;
        assert!((slots[0] - 22.5).abs() < 1e-9);
        assert!((slots[3] - 4.5).abs() < 1e-9, "amulet wears 30 × 5 / 300");
        // Dragonskin Bracers add 5 to every slot.
        let bracers = classes.find_name("ArtiBoostArmor").unwrap();
        let _ = crate::inventory::give_inventory(&mut w, mo, bracers, 1);
        assert!(use_inventory_item(&mut w, mo, bracers));
        let slots = w.players[0].as_ref().unwrap().hexen_armor;
        assert!((slots[1] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn skill_armor_factor_scales_save_amount_unless_ignoreskill() {
        let (mut w, classes, mo) = doom_world_with(&format!(
            "{ARMOR_BASES}actor HalfArmor : BasicArmorPickup {{ Armor.SaveAmount 100 }}\n\
             actor FixedArmor : BasicArmorPickup {{ Armor.SaveAmount 100 +INVENTORY.IGNORESKILL }}\n"
        ));
        w.armor_factor = 0.5;
        let half = classes.find_name("HalfArmor").unwrap();
        assert!(use_armor(&mut w, mo, half, 1));
        assert_eq!(w.players[0].as_ref().unwrap().armor, 50);
        w.players[0].as_mut().unwrap().armor = 0;
        let fixed = classes.find_name("FixedArmor").unwrap();
        assert!(use_armor(&mut w, mo, fixed, 1));
        assert_eq!(w.players[0].as_ref().unwrap().armor, 100);
    }

    #[test]
    fn decorate_armor_pickup_uses_save_amount_and_percent() {
        let (mut w, classes, mo) = doom_world_with(&format!(
            "{ARMOR_BASES}actor SuperArmor : BasicArmorPickup {{\n  Armor.SaveAmount 150\n  Armor.SavePercent 80\n  Armor.MaxAbsorb 10\n}}\n"
        ));
        let cid = classes.find_name("SuperArmor").unwrap();
        let item = w
            .spawn_at(cid, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, mo, item));
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.armor, 150);
        assert!((p.armor_fraction() - 0.8).abs() < 1e-9);
        assert_eq!(p.armor_type, Some(cid));
        assert_eq!(p.armor_class, 2, "80 % shows the blue icon");
        // 30 damage: 80 % is 24, but MaxAbsorb caps the tic at 10.
        crate::damage::damage_mobj(&mut w, mo, None, None, 30);
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.armor, 140);
        assert_eq!(w.actors.get(mo).unwrap().health, 80);
        // The same tic's budget is spent: the next hit is all health.
        crate::damage::damage_mobj(&mut w, mo, None, None, 10);
        assert_eq!(w.players[0].as_ref().unwrap().armor, 140);
        assert_eq!(w.actors.get(mo).unwrap().health, 70);
    }

    #[test]
    fn decorate_armor_bonus_caps_at_max_save_amount_and_is_always_taken() {
        let (mut w, classes, mo) = doom_world_with(&format!(
            "{ARMOR_BASES}actor BigBonus : BasicArmorBonus {{\n  Armor.SaveAmount 5\n  Armor.MaxSaveAmount 12\n}}\n"
        ));
        let cid = classes.find_name("BigBonus").unwrap();
        for want in [5, 10, 12, 12] {
            let item = w
                .spawn_at(cid, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
                .unwrap();
            assert!(try_pickup(&mut w, mo, item), "bonuses are always picked up");
            assert_eq!(w.players[0].as_ref().unwrap().armor, want);
        }
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.armor_type, Some(cid));
        assert_eq!(p.armor_class, 1);
    }

    #[test]
    fn armor_max_bonus_raises_the_cap_up_to_max_bonus_max() {
        let (mut w, classes, mo) = doom_world_with(&format!(
            "{ARMOR_BASES}actor CapBonus : BasicArmorBonus {{\n  Armor.SaveAmount 5\n  Armor.MaxSaveAmount 10\n  Armor.MaxBonus 3\n  Armor.MaxBonusMax 4\n}}\n"
        ));
        let cid = classes.find_name("CapBonus").unwrap();
        // Cap 10 + bonus count (3, then capped at 4).
        for want in [5, 10, 14, 14] {
            let item = w
                .spawn_at(cid, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
                .unwrap();
            assert!(try_pickup(&mut w, mo, item));
            assert_eq!(w.players[0].as_ref().unwrap().armor, want);
        }
        assert_eq!(w.players[0].as_ref().unwrap().armor_bonus_count, 4);
    }

    #[test]
    fn vanilla_armor_absorbs_rounding_down() {
        let (mut w, _classes, mo) = doom_world_with("");
        {
            let p = w.players[0].as_mut().unwrap();
            p.armor = 100;
            p.armor_class = 1;
        }
        // Doom `P_DamageMobj`: saved = damage / 3 → 3 of 10.
        crate::damage::damage_mobj(&mut w, mo, None, None, 10);
        assert_eq!(w.players[0].as_ref().unwrap().armor, 97);
        assert_eq!(w.actors.get(mo).unwrap().health, 93);
        {
            let p = w.players[0].as_mut().unwrap();
            p.armor = 2;
            p.armor_class = 2;
        }
        // Blue: 9 / 2 = 4, but only 2 points are left; the armor type clears.
        crate::damage::damage_mobj(&mut w, mo, None, None, 9);
        let p = w.players[0].as_ref().unwrap();
        assert_eq!((p.armor, p.armor_class), (0, 0));
        assert_eq!(w.actors.get(mo).unwrap().health, 86);
    }

    #[test]
    fn give_inventory_wears_armor() {
        let (mut w, classes, mo) = doom_world_with("");
        let green = classes.find_name("GreenArmor").unwrap();
        let _ = crate::action::call_action_args(
            &mut w,
            mo,
            "A_GiveInventory",
            &["GreenArmor".into(), "2".into()],
        );
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.armor, 200, "count multiplies SaveAmount");
        assert_eq!(p.armor_type, Some(green));
        assert_eq!(
            w.inventory.count(mo, green),
            0,
            "armor is worn, not carried"
        );
    }

    #[test]
    fn backpack_gives_one_clip_of_each_vanilla_ammo() {
        let (mut w, classes, mo) = doom_world_with("");
        let pack = classes.find_name("Backpack").unwrap();
        let ammo = [
            ("Clip", 10, 400),
            ("Shell", 4, 100),
            ("RocketAmmo", 1, 100),
            ("Cell", 20, 600),
        ];
        let had: Vec<i32> = ammo
            .iter()
            .map(|(n, _, _)| w.inventory.count(mo, classes.find_name(n).unwrap()))
            .collect();
        let item = w
            .spawn_at(pack, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, mo, item));
        for ((name, give, max), had) in ammo.into_iter().zip(had) {
            let cid = classes.find_name(name).unwrap();
            let it = w.inventory.find(mo, cid).unwrap();
            assert_eq!((it.amount, it.max_amount), (had + give, max), "{name}");
        }
    }

    #[test]
    fn backpack_uses_decorate_backpack_amounts() {
        let (mut w, classes, mo) = doom_world_with(
            "actor Inventory : Actor {}\nactor Ammo : Inventory {}\n\
             actor Nails : Ammo {\n  Inventory.MaxAmount 80\n  Ammo.BackpackAmount 7\n  Ammo.BackpackMaxAmount 160\n}\n\
             actor BigNails : Nails { Inventory.Amount 20 }\n",
        );
        let pack = classes.find_name("Backpack").unwrap();
        let item = w
            .spawn_at(pack, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, mo, item));
        let nails = classes.find_name("Nails").unwrap();
        let it = w.inventory.find(mo, nails).unwrap();
        assert_eq!((it.amount, it.max_amount), (7, 160));
        let big = classes.find_name("BigNails").unwrap();
        assert!(
            w.inventory.find(mo, big).is_none(),
            "subclasses are not filled"
        );
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
        let patch = room_dehacked::parse_dehacked(
            r#"
[STRINGS]
GOTCLIP = BEX remapped clip pickup.
GOTARMOR = BEX remapped armor pickup.
"#,
        )
        .unwrap();
        room_dehacked::apply_patch(&mut ir, &patch).unwrap();
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
    fn weapon_stay_leaves_map_gun_for_second_player() {
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
        w.weapon_stay = true;
        let shotgun = classes.find_name("Shotgun").unwrap();
        let p0 = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(shotgun, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        // Stay path returns false (not consumed) but still grants the weapon.
        assert!(!try_pickup(&mut w, p0, item));
        assert!(!w.actors.get(item).unwrap().destroyed);
        assert!(w.inventory.count(p0, shotgun) > 0);
        // Already owned → ignore.
        assert!(!try_pickup(&mut w, p0, item));
        // Dropped guns always vanish.
        let dropped = w
            .spawn_at(shotgun, DVec3::new(48.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        w.actors.get_mut(dropped).unwrap().flags |= crate::class::FLAG_DROPPED;
        // Fresh pawn without the gun — use inventory clear via second class give path.
        let _ = w.inventory.take(p0, shotgun, 99);
        assert!(try_pickup(&mut w, p0, dropped));
        assert!(w.actors.get(dropped).unwrap().destroyed);
    }

    #[test]
    fn custom_inventory_pickup_grants_the_weapon() {
        let mut src = include_str!("../../../testdata/actors/doom_basic.txt").to_owned();
        src.push_str(
            r#"
actor CustomInventory : Actor
{
}
actor CrateGun : Weapon
{
  States
  {
  Ready:
    PISG A 1 A_WeaponReady
    Loop
  Select:
    PISG A 1
    Goto Ready
  }
}
actor Crate : CustomInventory
{
  Radius 20
  Height 16
  States
  {
  Spawn:
    BON1 A -1
    Stop
  Pickup:
    TNT1 A 0 A_GiveInventory("CrateGun", 1)
    Stop
  }
}
"#,
        );
        let file = parse_decorate(&src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let gun = w.classes.find_name("CrateGun").unwrap();
        let crate_id = w.classes.find_name("Crate").unwrap();
        let p0 = w.players[0].as_ref().unwrap().mo.unwrap();
        let item = w
            .spawn_at(crate_id, DVec3::new(40.0, 32.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(try_pickup(&mut w, p0, item));
        assert!(w.actors.get(item).unwrap().destroyed);
        assert!(w.inventory.count(p0, gun) > 0);
        let player = w.players[0].as_ref().unwrap();
        assert!(
            player.ready_weapon == Some(gun) || player.pending_weapon == Some(gun),
            "ready {:?} pending {:?}",
            player.ready_weapon,
            player.pending_weapon
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
