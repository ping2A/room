//! Behaviour tests for the DECORATE properties added for `docs/36` parity.
#![allow(clippy::unwrap_used, clippy::float_cmp)]

use crate::actor::ActorId;
use crate::{ClassRegistry, StateTable, World};
use room_core::{Angle, DVec3};
use room_decorate::parse_decorate;
use room_nodebuild::apply_nodes;
use room_testkit::{MapBuilder, rect};
use std::sync::Arc;

const BASES: &str = r#"
actor Inventory : Actor {}
actor Powerup : Inventory {}
actor PowerSpeed : Powerup {}
actor PowerProtection : Powerup {}
actor PowerReflection : Powerup {}
actor PowerInvisibility : Powerup {}
actor PowerRegeneration : Powerup {}
actor PowerInfiniteAmmo : Powerup {}
actor PowerTimeFreezer : Powerup {}
actor PowerTargeter : Powerup {}
actor PowerScanner : Powerup {}
actor PowerDoubleFiringSpeed : Powerup {}
actor PowerupGiver : Inventory {}
actor FreezeGiver : PowerupGiver { +INVENTORY.AUTOACTIVATE Powerup.Type "TimeFreezer" Powerup.Duration -6 States { Spawn: TIME A -1 Stop } }
actor TargetGiver : PowerupGiver { +INVENTORY.AUTOACTIVATE Powerup.Type "Targeter" Powerup.Duration -10 States { Spawn: TRGT A -1 Stop } }
actor ScanGiver : PowerupGiver { +INVENTORY.AUTOACTIVATE Powerup.Type "Scanner" States { Spawn: SCAN A -1 Stop } }
actor HasteGiver : PowerupGiver { +INVENTORY.AUTOACTIVATE Powerup.Type "DoubleFiringSpeed" States { Spawn: HAST A -1 Stop } }
actor Drifter : Actor { Radius 8 Height 16 +NOGRAVITY States { Spawn: DRFT A -1 Stop } }
actor Clock : Actor { Radius 8 Height 16 +NOGRAVITY +NOTIMEFREEZE States { Spawn: DRFT A -1 Stop } }
actor PuzzleItem : Inventory {}
actor WeaponPiece : Inventory {}
actor HealthPickup : Inventory {}
actor SpeedGiver : PowerupGiver
{
  +INVENTORY.AUTOACTIVATE
  Powerup.Type "Speed"
  Powerup.Duration -10
  Powerup.Color "00 00 ff", 0.25
  States { Spawn: SPED A -1 Stop }
}
actor ShieldGiver : PowerupGiver
{
  +INVENTORY.AUTOACTIVATE
  Powerup.Type "PowerProtection"
  States { Spawn: SHLD A -1 Stop }
}
actor GhostGiver : PowerupGiver
{
  +INVENTORY.AUTOACTIVATE
  Powerup.Type "Invisibility"
  Powerup.Strength 60
  Powerup.Mode "Translucent"
  States { Spawn: GHST A -1 Stop }
}
actor Gem : PuzzleItem
{
  PuzzleItem.Number 3
  PuzzleItem.FailMessage "Nothing happens"
  PuzzleItem.FailSound "gem/fail"
}
actor Rod : Weapon
{
  Health 2
  Weapon.AmmoType "Clip"
  Weapon.AmmoGive 5
}
actor RodPiece1 : WeaponPiece { WeaponPiece.Number 1  WeaponPiece.Weapon "Rod"  States { Spawn: PCE1 A -1 Stop } }
actor RodPiece2 : WeaponPiece { WeaponPiece.Number 2  WeaponPiece.Weapon "Rod"  States { Spawn: PCE2 A -1 Stop } }
actor Flask : HealthPickup { Health 25  HealthPickup.AutoUse 2 }
actor Quest : Inventory { Inventory.GiveQuest 4  States { Spawn: QUST A -1 Stop } }
actor QuestItem4 : Inventory {}
actor PoisonDart : Actor
{
  Radius 4
  Height 4
  Damage 1
  PoisonDamage 3, 10, 2
  PoisonDamageType "Venom"
  Projectile
  States { Spawn: DART A -1 Stop }
}
actor Lurker : Actor
{
  Health 100
  Radius 20
  Height 56
  Monster
  +STEALTH
  StealthAlpha 0.25
  RenderRequired 4
  States { Spawn: LURK A -1 Stop  See: LURK B 4 Loop  Missile: LURK C 8 Goto See }
}
actor Peeker : Actor
{
  Radius 20
  Height 56
  VisibleAngles -45, 45
  SpriteRotation 30
  LightLevel 200
  States { Spawn: PEEK A -1 Stop }
}
actor RespawnBox : Inventory { Inventory.RespawnTics 10  States { Spawn: RBOX A -1 Stop } }
"#;

fn world_with(extra: &str) -> (World, Arc<ClassRegistry>, ActorId) {
    let src = format!(
        "{}\n{BASES}\n{extra}",
        include_str!("../../../testdata/actors/doom_basic.txt")
    );
    let ir = parse_decorate(&src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 256.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    (w, classes, mo)
}

fn pick(w: &mut World, classes: &ClassRegistry, mo: ActorId, name: &str) -> bool {
    let cid = classes.find_name(name).unwrap();
    let it = w
        .spawn_at(cid, DVec3::new(70.0, 64.0, 0.0), Angle::ZERO)
        .unwrap();
    crate::pickup::try_pickup(w, mo, it)
}

#[test]
fn powerup_giver_runs_a_generic_power_and_expires() {
    let (mut w, classes, mo) = world_with("");
    assert!(pick(&mut w, &classes, mo, "SpeedGiver"));
    let p = w.players[0].as_ref().unwrap();
    let st = p.powers.get("powerspeed").expect("speed power running");
    assert_eq!(st.tics, 350, "Powerup.Duration -10 is ten seconds");
    assert_eq!(crate::powerups::speed_factor(&w, mo), 1.5);
    let blend = crate::powerups::screen_blend(&w, 0).unwrap();
    assert!(
        (blend.0[2] - 1.0).abs() < 1e-6 && (blend.0[3] - 0.25).abs() < 1e-6,
        "Powerup.Color"
    );
    assert_eq!(
        w.inventory
            .count(mo, classes.find_name("SpeedGiver").unwrap()),
        0,
        "giver used up"
    );
    for _ in 0..351 {
        w.tick();
    }
    assert!(
        w.players[0].as_ref().unwrap().powers.is_empty(),
        "power ran out"
    );
}

#[test]
fn protection_quarters_damage_and_invisibility_uses_mode_and_strength() {
    let (mut w, classes, mo) = world_with("");
    assert!(pick(&mut w, &classes, mo, "ShieldGiver"));
    crate::damage::damage_mobj(&mut w, mo, None, None, 40);
    assert_eq!(w.actors.get(mo).unwrap().health, 90);
    assert!(pick(&mut w, &classes, mo, "GhostGiver"));
    let a = w.actors.get(mo).unwrap();
    assert_eq!(a.render_style, crate::RenderStyle::Translucent);
    assert!(
        (a.alpha - 0.4).abs() < 1e-9,
        "Strength 60 → alpha 0.4, got {}",
        a.alpha
    );
    assert!(
        a.flags & crate::class::FLAG_SHADOW != 0,
        "invisibility sets +SHADOW"
    );
}

#[test]
fn puzzle_item_fails_with_its_message_and_sound() {
    let (mut w, classes, mo) = world_with("");
    let gem = classes.find_name("Gem").unwrap();
    let _ = w.inventory.give(mo, gem, 1, 1);
    let _ = w.drain_messages();
    assert!(!crate::pickup::use_inventory_item(&mut w, mo, gem));
    assert_eq!(w.drain_messages(), vec!["Nothing happens".to_owned()]);
    assert!(w.drain_sound_events().iter().any(|e| e.name == "gem/fail"));
    assert_eq!(w.inventory.count(mo, gem), 1, "a failed use keeps the item");
}

#[test]
fn weapon_pieces_assemble_the_weapon() {
    let (mut w, classes, mo) = world_with("");
    let rod = classes.find_name("Rod").unwrap();
    assert!(pick(&mut w, &classes, mo, "RodPiece1"));
    assert_eq!(w.inventory.count(mo, rod), 0);
    assert!(!pick(&mut w, &classes, mo, "RodPiece1") || w.inventory.count(mo, rod) == 0);
    assert!(pick(&mut w, &classes, mo, "RodPiece2"));
    assert_eq!(
        w.inventory.count(mo, rod),
        1,
        "Health 2 pieces make the weapon"
    );
}

#[test]
fn health_pickup_auto_use_saves_the_player() {
    let (mut w, classes, mo) = world_with("");
    let flask = classes.find_name("Flask").unwrap();
    let _ = w.inventory.give(mo, flask, 1, 5);
    crate::damage::damage_mobj(&mut w, mo, None, None, 110);
    let hp = w.actors.get(mo).unwrap().health;
    assert!(
        hp > 0,
        "AutoUse 2 used the flask when the hit would kill (hp {hp})"
    );
    assert_eq!(w.inventory.count(mo, flask), 0);
}

#[test]
fn give_quest_hands_out_the_quest_token() {
    let (mut w, classes, mo) = world_with("");
    assert!(pick(&mut w, &classes, mo, "Quest"));
    assert_eq!(
        w.inventory
            .count(mo, classes.find_name("QuestItem4").unwrap()),
        1
    );
}

#[test]
fn items_respawn_after_respawn_tics() {
    let (mut w, classes, mo) = world_with("");
    w.respawn_items = true;
    let cid = classes.find_name("RespawnBox").unwrap();
    assert!(pick(&mut w, &classes, mo, "RespawnBox"));
    let count = |w: &World| {
        w.thinkers
            .default
            .iter()
            .filter_map(|&id| w.actors.get(id))
            .filter(|a| a.class == cid && !a.destroyed)
            .count()
    };
    assert_eq!(count(&w), 0);
    for _ in 0..12 {
        w.tick();
    }
    assert_eq!(count(&w), 1, "Inventory.RespawnTics 10");
}

#[test]
fn poison_damage_hurts_over_time() {
    let (mut w, classes, _mo) = world_with("");
    let lurker = classes.find_name("Lurker").unwrap();
    let dart = classes.find_name("PoisonDart").unwrap();
    let victim = w
        .spawn_at(lurker, DVec3::new(300.0, 300.0, 0.0), Angle::ZERO)
        .unwrap();
    let d = w
        .spawn_at(dart, DVec3::new(250.0, 300.0, 0.0), Angle::ZERO)
        .unwrap();
    crate::damage::damage_mobj(&mut w, victim, Some(d), Some(d), 1);
    let after_hit = w.actors.get(victim).unwrap().health;
    assert!(w.actors.get(victim).unwrap().ext.poison.is_some());
    for _ in 0..12 {
        w.tick();
    }
    let hp = w.actors.get(victim).unwrap().health;
    assert!(hp < after_hit, "poison ticks ({after_hit} → {hp})");
    assert!(
        after_hit - hp <= 15,
        "10 tics every 2 tics × 3 = at most 15"
    );
}

#[test]
fn stealth_fades_to_stealth_alpha_and_back_on_attack() {
    let (mut w, classes, _mo) = world_with("");
    let lurker = classes.find_name("Lurker").unwrap();
    let id = w
        .spawn_at(lurker, DVec3::new(300.0, 300.0, 0.0), Angle::ZERO)
        .unwrap();
    let see = w.states.find_label(lurker, "See").unwrap();
    w.set_state(id, Some(see), false);
    for _ in 0..20 {
        w.tick();
    }
    assert!(
        (w.actors.get(id).unwrap().alpha - 0.25).abs() < 1e-9,
        "StealthAlpha floor"
    );
    let missile = w.states.find_label(lurker, "Missile").unwrap();
    w.set_state(id, Some(missile), false);
    w.tick();
    assert!(w.actors.get(id).unwrap().alpha > 0.25, "attacking fades in");
}

#[test]
fn sprite_view_honours_render_required_visible_angles_and_rotation() {
    let (mut w, classes, _mo) = world_with("");
    let lurker = classes.find_name("Lurker").unwrap();
    let peeker = classes.find_name("Peeker").unwrap();
    let l = w
        .spawn_at(lurker, DVec3::new(300.0, 300.0, 0.0), Angle::ZERO)
        .unwrap();
    let p = w
        .spawn_at(peeker, DVec3::new(200.0, 200.0, 0.0), Angle::ZERO)
        .unwrap();
    let la = w.actors.get(l).unwrap().clone();
    assert!(
        crate::sprite_view(&w, &la, DVec3::new(0.0, 0.0, 41.0), 0).is_none(),
        "RenderRequired 4"
    );
    assert!(crate::sprite_view(&w, &la, DVec3::new(0.0, 0.0, 41.0), 4).is_some());
    let pa = w.actors.get(p).unwrap().clone();
    // Facing east: a viewer to the east is inside ±45°, one to the west is not.
    let front = crate::sprite_view(&w, &pa, DVec3::new(400.0, 200.0, 41.0), 0).unwrap();
    // `SpriteRotation` turns the rotation frame, not the quad (UZDoom
    // `hw_sprites.cpp`); the quad rolls only with `+ROLLSPRITE`.
    assert!(
        (front.rotation_offset_deg - 30.0).abs() < 1e-9,
        "SpriteRotation 30"
    );
    assert!(front.roll_rad.abs() < 1e-12);
    assert_eq!(front.light_level, Some(200));
    assert!(crate::sprite_view(&w, &pa, DVec3::new(0.0, 200.0, 41.0), 0).is_none());
    assert_eq!(
        crate::physics::actor_light_level(&w, &pa),
        200,
        "LightLevel"
    );
}

#[test]
fn shadow_target_spreads_monster_aim() {
    let (mut w, classes, mo) = world_with("");
    let imp = classes.find_name("DoomImp").unwrap();
    let id = w
        .spawn_at(imp, DVec3::new(300.0, 64.0, 0.0), Angle::ZERO)
        .unwrap();
    w.actors.get_mut(id).unwrap().target = Some(mo);
    let mut spread = false;
    w.players[0].as_mut().unwrap().blur = true;
    for _ in 0..8 {
        let _ = crate::action::call_action_args(&mut w, id, "A_FaceTarget", &[]);
        let deg = w.actors.get(id).unwrap().angle.degrees();
        if (deg - 180.0).abs() > 0.01 && (deg + 180.0).abs() > 0.01 {
            spread = true;
        }
    }
    assert!(spread, "a blurred player throws off A_FaceTarget");
}

#[test]
fn falling_scream_plays_once_per_fall() {
    let (mut w, _classes, mo) = world_with("");
    let _ = w.drain_sound_events();
    {
        let a = w.actors.get_mut(mo).unwrap();
        a.pos = DVec3::new(64.0, 64.0, 200.0);
        a.vel = DVec3::new(0.0, 0.0, -36.0);
    }
    let mut p = w.players[0].take().unwrap();
    crate::think_player::player_think(&mut w, &mut p);
    crate::think_player::player_think(&mut w, &mut p);
    w.players[0] = Some(p);
    let screams = w
        .drain_sound_events()
        .iter()
        .filter(|e| e.name == "*falling")
        .count();
    assert_eq!(screams, 1);
}

#[test]
fn time_freezer_stops_others_but_not_its_owner_and_blinks_out() {
    let (mut w, classes, mo) = world_with("");
    let drifter = w
        .spawn_at(
            classes.find_name("Drifter").unwrap(),
            DVec3::new(300.0, 300.0, 32.0),
            Angle::ZERO,
        )
        .unwrap();
    let clock = w
        .spawn_at(
            classes.find_name("Clock").unwrap(),
            DVec3::new(200.0, 300.0, 32.0),
            Angle::ZERO,
        )
        .unwrap();
    for id in [drifter, clock] {
        w.actors.get_mut(id).unwrap().vel = DVec3::new(2.0, 0.0, 0.0);
    }
    assert!(pick(&mut w, &classes, mo, "FreezeGiver"));
    assert!(w.players[0].as_ref().unwrap().timefreezer & 1 != 0);
    // The freeze starts on the next even tic.
    for _ in 0..2 {
        w.tick();
    }
    assert!(w.level.is_frozen());
    let x0 = w.actors.get(drifter).unwrap().pos.x();
    let c0 = w.actors.get(clock).unwrap().pos.x();
    for _ in 0..4 {
        w.tick();
    }
    assert_eq!(w.actors.get(drifter).unwrap().pos.x(), x0, "frozen in time");
    assert!(
        w.actors.get(clock).unwrap().pos.x() > c0,
        "+NOTIMEFREEZE keeps moving"
    );
    assert!(
        !crate::powerups::actor_time_frozen(&w, mo),
        "the owner moves"
    );
    for _ in 0..220 {
        w.tick();
    }
    assert!(!w.level.frozen_power, "ran out");
    assert_eq!(w.players[0].as_ref().unwrap().timefreezer, 0);
    assert!(w.actors.get(drifter).unwrap().pos.x() > x0);
    // Blinking: the last second alternates.
    assert!(crate::powerups::time_freezer_frozen(200));
    assert!(!crate::powerups::time_freezer_frozen(31) || !crate::powerups::time_freezer_frozen(32));
}

#[test]
fn targeter_scanner_and_double_firing_speed() {
    let (mut w, classes, mo) = world_with("");
    assert!(crate::powerups::targeter_hud(&w, mo).is_none());
    assert!(pick(&mut w, &classes, mo, "TargetGiver"));
    w.actors.get_mut(mo).unwrap().accuracy = 40;
    let t = crate::powerups::targeter_hud(&w, mo).unwrap();
    assert_eq!(t.center, (157.0, 97.0));
    assert_eq!(t.left_x, Some(157.0 - 60.0));
    assert_eq!(t.right_x, Some(157.0 + 60.0));
    assert_eq!(w.player_hud(0).unwrap().targeter, Some(t));
    // Last five seconds: bit 5 of the tics left shows only the left piece.
    w.players[0]
        .as_mut()
        .unwrap()
        .powers
        .get_mut("powertargeter")
        .unwrap()
        .tics = 40;
    let t = crate::powerups::targeter_hud(&w, mo).unwrap();
    assert!(t.left_x.is_some() && t.right_x.is_none());
    w.players[0]
        .as_mut()
        .unwrap()
        .powers
        .get_mut("powertargeter")
        .unwrap()
        .tics = 20;
    let t = crate::powerups::targeter_hud(&w, mo).unwrap();
    assert!(t.left_x.is_none() && t.right_x.is_some());
    assert!(!crate::powerups::reveals_map(&w, mo));
    assert!(pick(&mut w, &classes, mo, "ScanGiver"));
    assert!(crate::powerups::reveals_map(&w, mo));
    assert!(pick(&mut w, &classes, mo, "HasteGiver"));
    assert!(crate::powerups::power(&w, mo, "doublefiringspeed").is_some());
}
