//! Heretic and shared Raven actions (`actors/heretic`, `actors/raven`,
//! `actors/shared/bridge.zs`).

use super::*;

pub(super) const NAMES: &[&str] = &[
    "A_AccTeleGlitter",
    "A_AddPlayerRain",
    "A_BeakAttackPL1",
    "A_BeakAttackPL2",
    "A_BeakRaise",
    "A_BlueSpark",
    "A_DeathBallImpact",
    "A_DripBlood",
    "A_FireBlasterPL1",
    "A_FireCrossbowPL1",
    "A_FireCrossbowPL2",
    "A_FireGoldWandPL1",
    "A_FireGoldWandPL2",
    "A_FireMacePL1",
    "A_FireMacePL2",
    "A_FirePhoenixPL1",
    "A_FirePhoenixPL2",
    "A_FireSkullRodPL1",
    "A_FireSkullRodPL2",
    "A_FlameEnd",
    "A_FloatPuff",
    "A_GauntletAttack",
    "A_GenWizard",
    "A_GhostOff",
    "A_HideInCeiling",
    "A_ImpDeath",
    "A_ImpExplode",
    "A_ImpMsAttack",
    "A_ImpXDeath1",
    "A_InitPhoenixPL2",
    "A_KnightAttack",
    "A_LichAttack",
    "A_LichFireGrow",
    "A_LichIceImpact",
    "A_MaceBallImpact",
    "A_MaceBallImpact2",
    "A_MacePL1Check",
    "A_MakePod",
    "A_PhoenixPuff",
    "A_PodPain",
    "A_RainImpact",
    "A_RemovePod",
    "A_ShutdownPhoenixPL2",
    "A_SkullRodStorm",
    "A_Sor1Chase",
    "A_Sor1Pain",
    "A_Sor2DthInit",
    "A_Sor2DthLoop",
    "A_SorcererRise",
    "A_SpawnRippers",
    "A_Srcr1Attack",
    "A_Srcr2Attack",
    "A_Srcr2Decide",
    "A_StaffAttack",
    "A_TimeBomb",
    "A_VolcBallImpact",
    "A_VolcanoBlast",
    "A_VolcanoSet",
    "A_WhirlwindSeek",
    "A_WizAtk1",
    "A_WizAtk2",
    "A_WizAtk3",
    // Raven (shared by Heretic and Hexen)
    "A_MinotaurAtk1",
    "A_MinotaurAtk2",
    "A_MinotaurAtk3",
    "A_MinotaurCharge",
    "A_MinotaurChase",
    "A_MinotaurDeath",
    "A_MinotaurDecide",
    "A_MinotaurLook",
    "A_MinotaurRoam",
    "A_MntrFloorFire",
    "A_BridgeInit",
    "A_BridgeOrbit",
];

/// Raven `MAGIC_JUNK` marker for a mace ball that already bounced.
const MAGIC_JUNK: i32 = 1234;
/// `MNTR_CHARGE_SPEED`.
const MNTR_CHARGE_SPEED: f64 = 13.0;
/// `MAULATORTICS` (25 s).
const MAULATORTICS: i32 = 25 * 35;
/// `MINOTAUR_LOOK_DIST`.
const MINOTAUR_LOOK_DIST: f64 = 16.0 * 54.0;
/// `CustomBridge.ORBIT_RADIUS`.
const ORBIT_RADIUS: f64 = 15.0;

pub(super) fn dispatch(
    w: &mut World,
    id: ActorId,
    name: &str,
    args: &[String],
) -> Option<ActionResult> {
    let r = match name {
        "A_AccTeleGlitter" => {
            if let Some(a) = w.actors.get_mut(id) {
                a.health += 1;
                if a.health > 35 {
                    a.vel = DVec3::new(a.vel.x(), a.vel.y(), a.vel.z() * 1.5);
                }
            }
            ActionResult::Continue
        }
        "A_AddPlayerRain" => add_player_rain(w, id),
        "A_BeakAttackPL1" | "A_BeakAttackPL2" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            let dmg = if name == "A_BeakAttackPL1" {
                rnd_range(w, "pr_beakatk", 1, 3)
            } else {
                rnd_range(w, "pr_beakatk", 1, 8) * 4
            };
            let ang = angle(w, id);
            let slope = aim(w, id, ang, MELEE_RANGE);
            if let Some(t) = line_attack(w, id, ang, MELEE_RANGE, slope, dmg, "BeakPuff") {
                let a = angle_to(w, id, t);
                set_angle(w, id, a);
            }
            sound(w, id, "chicken/peck");
            ActionResult::Continue
        }
        "A_BeakRaise" => call(w, id, "A_Raise", &[]),
        "A_BlueSpark" => {
            let p = pos(w, id);
            for _ in 0..2 {
                if let Some(mo) = spawn(w, "Sorcerer2FXSpark", p) {
                    let vx = f64::from(rnd2(w, "pr_bluespark")) / 128.0;
                    let vy = f64::from(rnd2(w, "pr_bluespark")) / 128.0;
                    let vz = 1.0 + f64::from(rnd(w, "pr_bluespark")) / 256.0;
                    set_vel(w, mo, DVec3::new(vx, vy, vz));
                }
            }
            ActionResult::Continue
        }
        "A_DeathBallImpact" => death_ball_impact(w, id),
        "A_DripBlood" => {
            let p = pos(w, id);
            let xo = f64::from(rnd2(w, "pr_dripblood")) / 32.0;
            let yo = f64::from(rnd2(w, "pr_dripblood")) / 32.0;
            if let Some(mo) = spawn(w, "Blood", DVec3::new(p.x() + xo, p.y() + yo, p.z())) {
                let vx = f64::from(rnd2(w, "pr_dripblood")) / 64.0;
                let vy = f64::from(rnd2(w, "pr_dripblood")) / 64.0;
                set_vel(w, mo, DVec3::new(vx, vy, 0.0));
                if let Some(a) = w.actors.get_mut(mo) {
                    a.gravity = 1.0 / 8.0;
                }
            }
            ActionResult::Continue
        }
        "A_FireBlasterPL1" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let slope = bullet_slope(w, id);
            let dmg = rnd_range(w, "pr_fireblaster", 1, 8) * 4;
            let mut ang = angle(w, id);
            if refire(w, id) {
                ang += f64::from(rnd2(w, "pr_fireblaster")) * (5.625 / 256.0);
            }
            let _ = line_attack(w, id, ang, PLAYER_MISSILE_RANGE, slope, dmg, "BlasterPuff");
            sound(w, id, "weapons/blastershoot");
            ActionResult::Continue
        }
        "A_FireCrossbowPL1" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let _ = spawn_player_missile(w, id, "CrossbowFX1", 0.0);
            let _ = spawn_player_missile(w, id, "CrossbowFX3", -4.5);
            let _ = spawn_player_missile(w, id, "CrossbowFX3", 4.5);
            ActionResult::Continue
        }
        "A_FireCrossbowPL2" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            for (c, a) in [
                ("CrossbowFX2", 0.0),
                ("CrossbowFX2", -4.5),
                ("CrossbowFX2", 4.5),
                ("CrossbowFX3", -9.0),
                ("CrossbowFX3", 9.0),
            ] {
                let _ = spawn_player_missile(w, id, c, a);
            }
            ActionResult::Continue
        }
        "A_FireGoldWandPL1" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let slope = bullet_slope(w, id);
            let dmg = rnd_range(w, "pr_firegoldwand", 7, 14);
            let mut ang = angle(w, id);
            if refire(w, id) {
                ang += f64::from(rnd2(w, "pr_firegoldwand")) * (5.625 / 256.0);
            }
            let _ = line_attack(
                w,
                id,
                ang,
                PLAYER_MISSILE_RANGE,
                slope,
                dmg,
                "GoldWandPuff1",
            );
            sound(w, id, "weapons/wandhit");
            ActionResult::Continue
        }
        "A_FireGoldWandPL2" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let slope = bullet_slope(w, id);
            let speed = class_speed(w, "GoldWandFX2");
            let vz = speed * slope.clamp(-5.0, 5.0);
            let a = angle(w, id);
            let _ = spawn_missile_angle(w, id, "GoldWandFX2", a - 45.0 / 8.0, vz);
            let _ = spawn_missile_angle(w, id, "GoldWandFX2", a + 45.0 / 8.0, vz);
            let mut ang = a - 45.0 / 8.0;
            for _ in 0..5 {
                let dmg = rnd_range(w, "pr_firegoldwand", 1, 8);
                let _ = line_attack(
                    w,
                    id,
                    ang,
                    PLAYER_MISSILE_RANGE,
                    slope,
                    dmg,
                    "GoldWandPuff2",
                );
                ang += (45.0 / 8.0) * 2.0 / 4.0;
            }
            sound(w, id, "weapons/wandhit");
            ActionResult::Continue
        }
        "A_FireMacePL1" => fire_mace_pl1(w, id),
        "A_FireMacePL2" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let slope = pitch_slope(w, id);
            let v = vel(w, id);
            if let Some(mo) = spawn_player_missile(w, id, "MaceFX4", 0.0) {
                let mv = vel(w, mo);
                set_vel(
                    w,
                    mo,
                    DVec3::new(mv.x() + v.x(), mv.y() + v.y(), 2.0 + slope.clamp(-5.0, 5.0)),
                );
                let ang = angle(w, id);
                if let Some(t) =
                    crate::hitscan::aim_line_attack_target(w, id, Angle::from_degrees(ang), 1024.0)
                        .map(|(_, t)| t)
                {
                    if let Some(a) = w.actors.get_mut(mo) {
                        a.tracer = Some(t);
                    }
                }
            }
            sound(w, id, "weapons/maceshoot");
            ActionResult::Continue
        }
        "A_FirePhoenixPL1" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let _ = spawn_player_missile(w, id, "PhoenixFX1", 0.0);
            let a = angle(w, id);
            thrust(w, id, a + 180.0, 4.0);
            ActionResult::Continue
        }
        "A_FirePhoenixPL2" => fire_phoenix_pl2(w, id),
        "A_InitPhoenixPL2" => {
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.scratch.insert("flamecount".into(), 10.0 * 35.0);
            }
            ActionResult::Continue
        }
        "A_ShutdownPhoenixPL2" => {
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.scratch.insert("flamecount".into(), 0.0);
            }
            let _ = deplete_ammo(w, id);
            ActionResult::Continue
        }
        "A_FireSkullRodPL1" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            if let Some(mo) = spawn_player_missile(w, id, "HornRodFX1", 0.0) {
                if rnd(w, "pr_fireskullrod") > 128 {
                    let next = w
                        .actors
                        .get(mo)
                        .and_then(|a| a.state)
                        .and_then(|s| w.states.get(s))
                        .and_then(|s| s.next);
                    if let Some(n) = next {
                        w.set_state(mo, Some(n), true);
                    }
                }
            }
            ActionResult::Continue
        }
        "A_FireSkullRodPL2" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let ang = angle(w, id);
            let t = crate::hitscan::aim_line_attack_target(w, id, Angle::from_degrees(ang), 1024.0)
                .map(|(_, t)| t);
            if let Some(mo) = spawn_player_missile(w, id, "HornRodFX2", 0.0) {
                if let Some(a) = w.actors.get_mut(mo) {
                    a.tracer = t;
                }
                sound(w, mo, "weapons/hornrodpowshoot");
            }
            ActionResult::Continue
        }
        "A_FlameEnd" => add_vz(w, id, 1.5),
        "A_FloatPuff" => add_vz(w, id, 1.8),
        "A_GauntletAttack" => gauntlet_attack(w, id, iarg(args, 0, 0) != 0),
        "A_GenWizard" => gen_wizard(w, id),
        "A_GhostOff" => {
            ghost(w, id, false);
            ActionResult::Continue
        }
        "A_HideInCeiling" => {
            set_flag(w, id, "INVISIBLE", true);
            set_flag(w, id, "SOLID", false);
            set_flag(w, id, "MISSILE", false);
            set_vel(w, id, DVec3::ZERO);
            ActionResult::Continue
        }
        "A_ImpDeath" => {
            set_flag(w, id, "SOLID", false);
            set_flag(w, id, "FLOORCLIP", true);
            ActionResult::Continue
        }
        "A_ImpXDeath1" => {
            set_flag(w, id, "SOLID", false);
            set_flag(w, id, "FLOORCLIP", true);
            set_flag(w, id, "NOGRAVITY", true);
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.scratch.insert("extremecrash".into(), 1.0);
            }
            ActionResult::Continue
        }
        "A_ImpExplode" => {
            set_flag(w, id, "NOGRAVITY", false);
            let p = pos(w, id);
            for c in ["HereticImpChunk1", "HereticImpChunk2"] {
                if let Some(mo) = spawn(w, c, p) {
                    let vx = f64::from(rnd2(w, "pr_impexplode")) / 64.0;
                    let vy = f64::from(rnd2(w, "pr_impexplode")) / 64.0;
                    set_vel(w, mo, DVec3::new(vx, vy, 9.0));
                }
            }
            let xcrash = w
                .actors
                .get(id)
                .is_some_and(|a| a.ext.scratch.contains_key("extremecrash"));
            if xcrash {
                let _ = set_label(w, id, "XCrash");
            }
            ActionResult::Continue
        }
        "A_ImpMsAttack" => {
            if target(w, id).is_none() || rnd(w, "pr_impmsatk") > 64 {
                let _ = set_label(w, id, "See");
                return Some(ActionResult::Continue);
            }
            call(w, id, "A_SkullAttack", &["12"])
        }
        "A_KnightAttack" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            if in_melee_range(w, id) {
                let dmg = rnd_range(w, "pr_knightattack", 1, 8) * 3;
                let _ = damage(w, t, Some(id), Some(id), dmg);
                sound(w, id, "hknight/melee");
                return Some(ActionResult::Continue);
            }
            let snd = class_sound(w, id, "attack");
            sound(w, id, &snd);
            let z = pos(w, id).z() + 36.0;
            let axe = if has_flag(w, id, "SHADOW") || rnd(w, "pr_knightattack") < 40 {
                "RedAxe"
            } else {
                "KnightAxe"
            };
            let _ = spawn_missile_z(w, id, z, t, axe);
            ActionResult::Continue
        }
        "A_LichAttack" => lich_attack(w, id),
        "A_LichFireGrow" => {
            if let Some(a) = w.actors.get_mut(id) {
                a.health -= 1;
                a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + 9.0);
            }
            if health(w, id) == 0 {
                if let Some(a) = w.actors.get_mut(id) {
                    a.damage = -1;
                }
                let _ = set_label(w, id, "NoGrow");
            }
            ActionResult::Continue
        }
        "A_LichIceImpact" => {
            radial_missiles(w, id, "HeadFX2", 8, 45.0, None, -0.6);
            ActionResult::Continue
        }
        "A_MaceBallImpact" => {
            if health(w, id) != MAGIC_JUNK && in_float(w, id) {
                if let Some(a) = w.actors.get_mut(id) {
                    a.health = MAGIC_JUNK;
                    a.vel = DVec3::new(a.vel.x(), a.vel.y(), a.vel.z() * 0.75);
                }
                set_flag(w, id, "BOUNCEONFLOORS", false);
                set_flag(w, id, "BOUNCEONCEILINGS", false);
                let _ = set_label(w, id, "Spawn");
                sound(w, id, "weapons/macebounce");
            } else {
                stop_missile(w, id);
                sound(w, id, "weapons/macehit");
            }
            ActionResult::Continue
        }
        "A_MaceBallImpact2" => mace_ball_impact2(w, id),
        "A_MacePL1Check" => {
            let s1 = w.actors.get(id).map_or(0, |a| a.ext.special1);
            if s1 == 0 {
                return Some(ActionResult::Continue);
            }
            let left = s1 - 4;
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.special1 = left.max(0);
            }
            if left > 0 {
                return Some(ActionResult::Continue);
            }
            set_flag(w, id, "NOGRAVITY", false);
            let v = vel(w, id);
            let len = v.x().hypot(v.y()).max(1e-6);
            let k = 7.0 / len;
            if let Some(a) = w.actors.get_mut(id) {
                a.gravity = 1.0 / 8.0;
                a.vel = DVec3::new(v.x() * k, v.y() * k, v.z() * 0.5);
            }
            ActionResult::Continue
        }
        "A_MakePod" => {
            let ty = sarg(args, 0, "Pod");
            let count = w.actors.get(id).map_or(0, |a| a.ext.special1);
            if count >= 16 {
                return Some(ActionResult::Continue);
            }
            let p = pos(w, id);
            let (floor, _) = floor_ceil(w, id);
            let Some(mo) = spawn(w, &ty, DVec3::new(p.x(), p.y(), floor)) else {
                return Some(ActionResult::Continue);
            };
            let fits = w
                .actors
                .get(mo)
                .is_some_and(|a| crate::physics::check_position(w, a, a.pos, Some(mo)).ok);
            if !fits {
                destroy(w, mo);
                return Some(ActionResult::Continue);
            }
            let _ = set_label(w, mo, "Grow");
            let ang = f64::from(rnd(w, "pr_makepod")) * (360.0 / 256.0);
            thrust(w, mo, ang, 4.5);
            let snd = class_sound(w, id, "attack");
            sound(w, id, &snd);
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.special1 += 1;
            }
            if let Some(a) = w.actors.get_mut(mo) {
                a.master = Some(id);
            }
            ActionResult::Continue
        }
        "A_PhoenixPuff" => {
            let p = pos(w, id);
            let a = angle(w, id);
            for off in [90.0, -90.0] {
                if let Some(mo) = spawn(w, "PhoenixPuff", p) {
                    let (vx, vy) = angle_vec(a + off, 1.3);
                    set_vel(w, mo, DVec3::new(vx, vy, 0.0));
                }
            }
            ActionResult::Continue
        }
        "A_PodPain" => {
            let goo = sarg(args, 0, "PodGoo");
            let chance = rnd(w, "pr_podpain");
            if chance < 128 {
                return Some(ActionResult::Continue);
            }
            let p = pos(w, id);
            for _ in 0..if chance > 240 { 2 } else { 1 } {
                if let Some(mo) = spawn(w, &goo, DVec3::new(p.x(), p.y(), p.z() + 48.0)) {
                    if let Some(a) = w.actors.get_mut(mo) {
                        a.target = Some(id);
                    }
                    let vx = f64::from(rnd2(w, "pr_podpain")) / 128.0;
                    let vy = f64::from(rnd2(w, "pr_podpain")) / 128.0;
                    let vz = 0.5 + f64::from(rnd(w, "pr_podpain")) / 128.0;
                    set_vel(w, mo, DVec3::new(vx, vy, vz));
                }
            }
            ActionResult::Continue
        }
        "A_RainImpact" => {
            let (floor, _) = floor_ceil(w, id);
            if pos(w, id).z() > floor {
                let _ = set_label(w, id, "NotFloor");
            } else {
                let _ = rnd(w, "pr_rainimpact");
            }
            ActionResult::Continue
        }
        "A_RemovePod" => {
            if let Some(m) = master(w, id) {
                if let Some(a) = w.actors.get_mut(m) {
                    if a.ext.special1 > 0 {
                        a.ext.special1 -= 1;
                    }
                }
            }
            ActionResult::Continue
        }
        "A_SkullRodStorm" => skull_rod_storm(w, id),
        "A_Sor1Chase" => {
            if let Some(a) = w.actors.get_mut(id) {
                if a.ext.special1 != 0 {
                    a.ext.special1 -= 1;
                    a.tics -= 3;
                }
            }
            crate::action::a_chase(w, id)
        }
        "A_Sor1Pain" => {
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.special1 = 20;
            }
            call(w, id, "A_Pain", &[])
        }
        "A_Sor2DthInit" => {
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.special1 = 7;
            }
            kill_all_monsters(w, id);
            ActionResult::Continue
        }
        "A_Sor2DthLoop" => {
            let left = w.actors.get_mut(id).map_or(0, |a| {
                a.ext.special1 -= 1;
                a.ext.special1
            });
            if left != 0 {
                let _ = set_label(w, id, "DeathLoop");
            }
            ActionResult::Continue
        }
        "A_SorcererRise" => {
            set_flag(w, id, "SOLID", false);
            let p = pos(w, id);
            let a = angle(w, id);
            if let Some(mo) = spawn_facing(w, "Sorcerer2", p, a) {
                let tr = w
                    .actors
                    .get(id)
                    .map(|x| x.translation.clone())
                    .unwrap_or_default();
                let friendly = has_flag(w, id, "FRIENDLY");
                if let Some(m) = w.actors.get_mut(mo) {
                    m.translation = tr;
                }
                set_flag(w, mo, "FRIENDLY", friendly);
                let _ = set_label(w, mo, "Rise");
            }
            ActionResult::Continue
        }
        "A_SpawnRippers" => {
            let t = target(w, id);
            radial_missiles(w, id, "Ripper", 8, 45.0, t, 0.0);
            ActionResult::Continue
        }
        "A_Srcr1Attack" => srcr1_attack(w, id),
        "A_Srcr2Attack" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            let snd = class_sound(w, id, "attack");
            sound(w, id, &snd);
            if in_melee_range(w, id) {
                let dmg = rnd_range(w, "pr_srcr2atk", 1, 8) * 20;
                let _ = damage(w, t, Some(id), Some(id), dmg);
                return Some(ActionResult::Continue);
            }
            let chance = if health(w, id) < spawn_health(w, id) / 2 {
                96
            } else {
                48
            };
            if rnd(w, "pr_srcr2atk") < chance {
                let a = angle(w, id);
                let _ = spawn_missile_angle(w, id, "Sorcerer2FX2", a - 45.0, 0.5);
                let _ = spawn_missile_angle(w, id, "Sorcerer2FX2", a + 45.0, 0.5);
            } else {
                let _ = spawn_missile(w, id, t, "Sorcerer2FX1");
            }
            ActionResult::Continue
        }
        "A_Srcr2Decide" => {
            const CHANCE: [i32; 9] = [192, 120, 120, 120, 64, 64, 32, 16, 0];
            let h8 = (spawn_health(w, id) / 8).max(1);
            let idx = (health(w, id) / h8).clamp(0, 8) as usize;
            if rnd(w, "pr_srcr2decide") < CHANCE[idx] {
                dsparil_teleport(w, id);
            }
            ActionResult::Continue
        }
        "A_StaffAttack" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let dmg = iarg(args, 0, 0);
            let puff = sarg(args, 1, "StaffPuff");
            let ang = angle(w, id) + f64::from(rnd2(w, "pr_staffatk")) * (5.625 / 256.0);
            let slope = aim(w, id, ang, MELEE_RANGE);
            if let Some(t) = line_attack(w, id, ang, MELEE_RANGE, slope, dmg, &puff) {
                let a = angle_to(w, id, t);
                set_angle(w, id, a);
            }
            ActionResult::Continue
        }
        "A_TimeBomb" => {
            if let Some(a) = w.actors.get_mut(id) {
                a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + 32.0);
                a.render_style = crate::class::RenderStyle::Add;
                a.alpha = 1.0;
            }
            call(w, id, "A_Explode", &[])
        }
        "A_VolcBallImpact" => {
            let (floor, _) = floor_ceil(w, id);
            if pos(w, id).z() <= floor {
                set_flag(w, id, "NOGRAVITY", true);
                if let Some(a) = w.actors.get_mut(id) {
                    a.gravity = 1.0;
                    a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + 28.0);
                }
            }
            let _ = call(w, id, "A_Explode", &["25", "25", "1"]);
            let p = pos(w, id);
            for i in 0..4 {
                if let Some(mo) = spawn_facing(w, "VolcanoTBlast", p, 90.0 * f64::from(i)) {
                    if let Some(a) = w.actors.get_mut(mo) {
                        a.target = Some(id);
                    }
                    let (vx, vy) = angle_vec(90.0 * f64::from(i), 0.7);
                    let vz = 1.0 + f64::from(rnd(w, "pr_volcballimpact")) / 128.0;
                    set_vel(w, mo, DVec3::new(vx, vy, vz));
                }
            }
            ActionResult::Continue
        }
        "A_VolcanoBlast" => {
            let n = rnd_range(w, "pr_volcanoblast", 1, 3);
            let p = pos(w, id);
            for _ in 0..n {
                let a = f64::from(rnd(w, "pr_volcanoblast")) * (360.0 / 256.0);
                if let Some(mo) =
                    spawn_facing(w, "VolcanoBlast", DVec3::new(p.x(), p.y(), p.z() + 44.0), a)
                {
                    if let Some(x) = w.actors.get_mut(mo) {
                        x.target = Some(id);
                    }
                    let (vx, vy) = angle_vec(a, 1.0);
                    let vz = 2.5 + f64::from(rnd(w, "pr_volcanoblast")) / 64.0;
                    set_vel(w, mo, DVec3::new(vx, vy, vz));
                    sound(w, mo, "world/volcano/shoot");
                }
            }
            ActionResult::Continue
        }
        "A_VolcanoSet" => {
            let t = rnd_range(w, "pr_volcanoset", 105, 232);
            if let Some(a) = w.actors.get_mut(id) {
                a.tics = t;
            }
            ActionResult::Continue
        }
        "A_WhirlwindSeek" => {
            let hp = health(w, id) - 3;
            if let Some(a) = w.actors.get_mut(id) {
                a.health = hp;
            }
            if hp < 0 {
                set_vel(w, id, DVec3::ZERO);
                let _ = set_label(w, id, "Death");
                set_flag(w, id, "MISSILE", false);
                return Some(ActionResult::Continue);
            }
            let th = w.actors.get_mut(id).map_or(0, |a| {
                a.threshold -= 3;
                a.threshold
            });
            if th < 0 {
                let n = rnd_range(w, "pr_whirlwindseek", 58, 89);
                if let Some(a) = w.actors.get_mut(id) {
                    a.threshold = n;
                }
                sound(w, id, "ironlich/attack3");
            }
            if tracer(w, id).is_some_and(|t| has_flag(w, t, "SHADOW")) {
                return Some(ActionResult::Continue);
            }
            call(w, id, "A_SeekerMissile", &["10", "30"])
        }
        "A_WizAtk1" => {
            face_target(w, id);
            ghost(w, id, false);
            ActionResult::Continue
        }
        "A_WizAtk2" => {
            face_target(w, id);
            ghost(w, id, true);
            ActionResult::Continue
        }
        "A_WizAtk3" => {
            ghost(w, id, false);
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            let snd = class_sound(w, id, "attack");
            sound(w, id, &snd);
            if in_melee_range(w, id) {
                let dmg = rnd_range(w, "pr_wizatk3", 1, 8) * 4;
                let _ = damage(w, t, Some(id), Some(id), dmg);
                return Some(ActionResult::Continue);
            }
            if let Some(mo) = spawn_missile(w, id, t, "WizardFX1") {
                let (a, vz) = (angle(w, mo), vel(w, mo).z());
                let _ = spawn_missile_angle(w, id, "WizardFX1", a - 45.0 / 8.0, vz);
                let _ = spawn_missile_angle(w, id, "WizardFX1", a + 45.0 / 8.0, vz);
            }
            ActionResult::Continue
        }
        "A_MinotaurAtk1" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            sound(w, id, "minotaur/melee");
            if in_melee_range(w, id) {
                let dmg = rnd_range(w, "pr_minotauratk1", 1, 8) * 4;
                let _ = damage(w, t, Some(id), Some(id), dmg);
                squish_player(w, t);
            }
            ActionResult::Continue
        }
        "A_MinotaurAtk2" => {
            let friendly = has_flag(w, id, "SUMMONEDMONSTER");
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            sound(w, id, "minotaur/attack2");
            if in_melee_range(w, id) {
                let dmg = rnd_range(w, "pr_minotauratk2", 1, 8) * if friendly { 3 } else { 5 };
                let _ = damage(w, t, Some(id), Some(id), dmg);
                return Some(ActionResult::Continue);
            }
            let z = pos(w, id).z() + 40.0;
            if let Some(mo) = spawn_missile_z(w, id, z, t, "MinotaurFX1") {
                let (a, vz) = (angle(w, mo), vel(w, mo).z());
                for off in [-45.0 / 8.0, 45.0 / 8.0, -45.0 / 16.0, 45.0 / 16.0] {
                    let _ = spawn_missile_angle_z_speed(w, id, z, "MinotaurFX1", a + off, vz, None);
                }
            }
            ActionResult::Continue
        }
        "A_MinotaurAtk3" => {
            let friendly = has_flag(w, id, "SUMMONEDMONSTER");
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            sound(w, id, "minotaur/attack3");
            if in_melee_range(w, id) {
                let dmg = rnd_range(w, "pr_minotauratk3", 1, 8) * if friendly { 3 } else { 5 };
                let _ = damage(w, t, Some(id), Some(id), dmg);
                squish_player(w, t);
            } else if let Some(mo) = spawn_missile(w, id, t, "MinotaurFX2") {
                sound(w, mo, "minotaur/attack1");
            }
            let s2 = w.actors.get(id).map_or(0, |a| a.ext.special2);
            if rnd(w, "pr_minotauratk3") < 192 && s2 == 0 {
                let _ = set_label(w, id, "HammerLoop");
                if let Some(a) = w.actors.get_mut(id) {
                    a.ext.special2 = 1;
                }
            }
            ActionResult::Continue
        }
        "A_MinotaurCharge" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            let s1 = w.actors.get(id).map_or(0, |a| a.ext.special1);
            if s1 > 0 {
                let puff = if w.classes.find_name("PunchPuff").is_some() {
                    "PunchPuff"
                } else {
                    "PhoenixPuff"
                };
                let p = pos(w, id);
                if let Some(mo) = spawn(w, puff, p) {
                    set_vel_z(w, mo, 2.0);
                }
                if let Some(a) = w.actors.get_mut(id) {
                    a.ext.special1 -= 1;
                }
            } else {
                set_flag(w, id, "SKULLFLY", false);
                set_flag(w, id, "INVULNERABLE", false);
                let _ = set_label(w, id, "See");
            }
            ActionResult::Continue
        }
        "A_MinotaurChase" => minotaur_chase(w, id),
        "A_MinotaurDeath" => {
            // GZDoom fades the Maulator out when the `MNTRF` death sprites are
            // missing (Hexen without Heretic graphics). Room loads both games'
            // sprites from the same IWAD, so the death frames are always used.
            ActionResult::Continue
        }
        "A_MinotaurDecide" => minotaur_decide(w, id),
        "A_MinotaurLook" => minotaur_look(w, id),
        "A_MinotaurRoam" => minotaur_roam(w, id),
        "A_MntrFloorFire" => {
            let (floor, _) = floor_ceil(w, id);
            if let Some(a) = w.actors.get_mut(id) {
                a.pos = DVec3::new(a.pos.x(), a.pos.y(), floor);
            }
            let x = f64::from(rnd2(w, "pr_mntrfloorfire")) / 64.0;
            let y = f64::from(rnd2(w, "pr_mntrfloorfire")) / 64.0;
            let p = pos(w, id);
            let t = target(w, id);
            if let Some(mo) = spawn(w, "MinotaurFX3", DVec3::new(p.x() + x, p.y() + y, floor)) {
                if let Some(a) = w.actors.get_mut(mo) {
                    a.target = t;
                    a.vel = DVec3::new(1.0 / 65536.0, 0.0, 0.0);
                }
            }
            ActionResult::Continue
        }
        "A_BridgeInit" => {
            let ball = sarg(args, 0, "BridgeBall");
            let start = f64::from(rnd(w, "pr_orbit")) * (360.0 / 256.0);
            let (count, r4, radius) = w.actors.get(id).map_or((3, 0, 0.0), |a| {
                (
                    if a.args[2] == 0 { 3 } else { a.args[2] },
                    a.args[4],
                    a.radius,
                )
            });
            let p = pos(w, id);
            for i in 0..count {
                let a = start + (45.0 / 32.0) * f64::from(256 / count.max(1)) * f64::from(i);
                let r = if r4 != 0 {
                    f64::from(r4) * radius / 100.0
                } else {
                    ORBIT_RADIUS
                };
                let (dx, dy) = angle_vec(a, r);
                if let Some(mo) =
                    spawn_facing(w, &ball, DVec3::new(p.x() + dx, p.y() + dy, p.z()), a)
                {
                    if let Some(m) = w.actors.get_mut(mo) {
                        m.target = Some(id);
                    }
                }
            }
            ActionResult::Continue
        }
        "A_BridgeOrbit" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            let (a3, a4, tr) = w
                .actors
                .get(t)
                .map_or((0, 0, 0.0), |a| (a.args[3], a.args[4], a.radius));
            let speed = if a3 > 128 {
                45.0 / 32.0 * f64::from(a3 - 256) / 35.0
            } else if a3 > 0 {
                45.0 / 32.0 * f64::from(a3) / 35.0
            } else {
                45.0 / 32.0 * 3.0 / 5.0
            };
            let r = if a4 != 0 {
                f64::from(a4) * tr / 100.0
            } else {
                ORBIT_RADIUS
            };
            let ang = angle(w, id) + speed;
            set_angle(w, id, ang);
            let tp = pos(w, t);
            let (dx, dy) = angle_vec(ang, r);
            if let Some(a) = w.actors.get_mut(id) {
                a.pos = DVec3::new(tp.x() + dx, tp.y() + dy, tp.z());
            }
            ActionResult::Continue
        }
        _ => return None,
    };
    Some(r)
}

fn add_vz(w: &mut World, id: ActorId, dz: f64) -> ActionResult {
    if let Some(a) = w.actors.get_mut(id) {
        a.vel = DVec3::new(a.vel.x(), a.vel.y(), a.vel.z() + dz);
    }
    ActionResult::Continue
}

/// `player != null` and the ready weapon's ammo could be spent.
pub(crate) fn player_fire(w: &mut World, id: ActorId) -> bool {
    is_player(w, id) && deplete_ammo(w, id)
}

pub(crate) fn refire(w: &World, id: ActorId) -> bool {
    w.players
        .iter()
        .flatten()
        .any(|p| p.mo == Some(id) && p.refire != 0)
        || w.thinking_refire != 0
}

/// `-tan(pitch)` in GZDoom's sign: Room's pitch is positive up.
pub(crate) fn pitch_slope(w: &World, id: ActorId) -> f64 {
    w.actors.get(id).map_or(0.0, |a| a.pitch.radians().tan())
}

pub(crate) fn class_speed(w: &World, class: &str) -> f64 {
    w.classes
        .find_name(class)
        .and_then(|c| w.classes.get(c))
        .map_or(0.0, |c| c.speed)
}

pub(crate) fn spawn_health(w: &World, id: ActorId) -> i32 {
    w.actors
        .get(id)
        .and_then(|a| w.classes.get(a.class))
        .map_or(1, |c| c.health.max(1))
}

/// A missile resting on the floor (Raven `MF_INFLOAT` while bouncing).
fn in_float(w: &World, id: ActorId) -> bool {
    let (floor, _) = floor_ceil(w, id);
    pos(w, id).z() <= floor + 1.0
}

fn stop_missile(w: &mut World, id: ActorId) {
    set_vel(w, id, DVec3::ZERO);
    set_flag(w, id, "NOGRAVITY", true);
    if let Some(a) = w.actors.get_mut(id) {
        a.gravity = 1.0;
    }
}

fn ghost(w: &mut World, id: ActorId, on: bool) {
    if let Some(a) = w.actors.get_mut(id) {
        if on {
            a.render_style = crate::class::RenderStyle::Translucent;
            a.alpha = 0.4;
        } else {
            a.render_style = crate::class::RenderStyle::Normal;
            a.alpha = 1.0;
        }
        a.extra_flags.set_name("GHOST", on);
    }
}

/// `player.deltaviewheight = -16` for a hammered player.
fn squish_player(w: &mut World, t: ActorId) {
    for p in w.players.iter_mut().flatten() {
        if p.mo == Some(t) {
            p.view_ofs_z = -16.0;
        }
    }
}

/// `count` missiles of `class` fanned `step` degrees apart from the actor.
pub(crate) fn radial_missiles(
    w: &mut World,
    id: ActorId,
    class: &str,
    count: i32,
    step: f64,
    owner: Option<ActorId>,
    vz: f64,
) {
    let p = pos(w, id);
    let owner = owner.or_else(|| target(w, id));
    let speed = class_speed(w, class);
    for i in 0..count {
        let a = f64::from(i) * step;
        if let Some(mo) = spawn_facing(w, class, p, a) {
            let (vx, vy) = angle_vec(a, speed);
            set_vel(w, mo, DVec3::new(vx, vy, vz));
            if let Some(x) = w.actors.get_mut(mo) {
                x.target = owner;
            }
        }
    }
}

fn kill_all_monsters(w: &mut World, except: ActorId) {
    let ids: Vec<ActorId> = w.thinkers.default.clone();
    for mo in ids {
        if mo == except || is_player(w, mo) || health(w, mo) <= 0 {
            continue;
        }
        if has_flag(w, mo, "ISMONSTER") || has_flag(w, mo, "COUNTKILL") {
            let hp = health(w, mo);
            let _ = damage(w, mo, None, None, hp.max(1) + 10_000);
        }
    }
}

fn add_player_rain(w: &mut World, id: ActorId) -> ActionResult {
    let Some(t) = target(w, id).filter(|&t| health(w, t) > 0) else {
        return ActionResult::Continue;
    };
    let (r1, r2) = w
        .actors
        .get(t)
        .map_or((None, None), |a| a.ext.special_actor);
    let live = |w: &World, r: Option<ActorId>| {
        r.filter(|x| w.actors.get(*x).is_some_and(|a| !a.destroyed))
    };
    let (mut r1, mut r2) = (live(w, r1), live(w, r2));
    if let (Some(a), Some(b)) = (r1, r2) {
        let (ha, hb) = (health(w, a), health(w, b));
        let old = if ha < hb { a } else { b };
        if let Some(x) = w.actors.get_mut(old) {
            if x.health > 16 {
                x.health = 16;
            }
        }
        if ha < hb {
            r1 = None;
        } else {
            r2 = None;
        }
    }
    if r1.is_some() {
        r2 = Some(id);
    } else {
        r1 = Some(id);
    }
    if let Some(a) = w.actors.get_mut(t) {
        a.ext.special_actor = (r1, r2);
    }
    ActionResult::Continue
}

fn death_ball_impact(w: &mut World, id: ActorId) -> ActionResult {
    if in_float(w, id) && vel(w, id).z() >= 2.0 {
        let mut new_angle = None;
        if let Some(t) = tracer(w, id) {
            if has_flag(w, t, "SHOOTABLE") {
                new_angle = Some(angle_to(w, id, t));
            } else if let Some(a) = w.actors.get_mut(id) {
                a.tracer = None;
            }
        } else {
            let owner = target(w, id);
            let mut ang: f64 = 0.0;
            for _ in 0..16 {
                if let Some((_, t)) =
                    crate::hitscan::aim_line_attack_target(w, id, Angle::from_degrees(ang), 640.0)
                {
                    if Some(t) != owner {
                        if let Some(a) = w.actors.get_mut(id) {
                            a.tracer = Some(t);
                        }
                        new_angle = Some(angle_to(w, id, t));
                        break;
                    }
                }
                ang += 22.5;
            }
        }
        if let Some(a) = new_angle {
            set_angle(w, id, a);
            let speed = w.actors.get(id).map_or(0.0, |x| x.speed);
            vel_from_angle(w, id, speed);
        }
        let _ = set_label(w, id, "Spawn");
        sound(w, id, "weapons/macestop");
        return ActionResult::Continue;
    }
    stop_missile(w, id);
    sound(w, id, "weapons/maceexplode");
    ActionResult::Continue
}

fn mace_ball_impact2(w: &mut World, id: ActorId) -> ActionResult {
    if in_float(w, id) && vel(w, id).z() >= 2.0 {
        let v = vel(w, id);
        let vz = v.z() * 0.75;
        set_vel(w, id, DVec3::new(v.x(), v.y(), vz));
        let _ = set_label(w, id, "Spawn");
        let p = pos(w, id);
        let a = angle(w, id);
        let t = target(w, id);
        for off in [90.0, -90.0] {
            if let Some(mo) = spawn_facing(w, "MaceFX3", p, a + off) {
                let (vx, vy) = angle_vec(a + off, vz - 1.0);
                set_vel(w, mo, DVec3::new(vx + v.x() * 0.5, vy + v.y() * 0.5, vz));
                if let Some(x) = w.actors.get_mut(mo) {
                    x.target = t;
                }
            }
        }
        return ActionResult::Continue;
    }
    stop_missile(w, id);
    set_flag(w, id, "BOUNCEONFLOORS", false);
    set_flag(w, id, "BOUNCEONCEILINGS", false);
    ActionResult::Continue
}

fn fire_mace_pl1(w: &mut World, id: ActorId) -> ActionResult {
    if !player_fire(w, id) {
        return ActionResult::Continue;
    }
    if rnd(w, "pr_maceatk") < 28 {
        let p = pos(w, id);
        let a = angle(w, id);
        let vz = 2.0 + pitch_slope(w, id).clamp(-5.0, 5.0);
        if let Some(ball) =
            spawn_facing(w, "MaceFX2", DVec3::new(p.x(), p.y(), p.z() + 28.0 + vz), a)
        {
            let speed = w.actors.get(ball).map_or(0.0, |x| x.speed);
            let (vx, vy) = angle_vec(a, speed);
            let v = vel(w, id);
            set_vel(w, ball, DVec3::new(vx + v.x() / 2.0, vy + v.y() / 2.0, vz));
            if let Some(x) = w.actors.get_mut(ball) {
                x.target = Some(id);
            }
            sound(w, ball, "weapons/maceshoot");
        }
    } else {
        let off = f64::from(rnd_range(w, "pr_maceatk", -4, 3)) * (360.0 / 256.0);
        if let Some(ball) = spawn_player_missile(w, id, "MaceFX1", off) {
            if let Some(x) = w.actors.get_mut(ball) {
                x.ext.special1 = 16;
            }
        }
    }
    ActionResult::Continue
}

fn fire_phoenix_pl2(w: &mut World, id: ActorId) -> ActionResult {
    if !is_player(w, id) {
        return ActionResult::Continue;
    }
    let left = w.actors.get_mut(id).map_or(0.0, |a| {
        let v = a.ext.scratch.entry("flamecount".into()).or_insert(0.0);
        *v -= 1.0;
        *v
    });
    if left <= 0.0 {
        let r = call(w, id, "A_WeaponReady", &[]);
        for p in w.players.iter_mut().flatten() {
            if p.mo == Some(id) {
                p.refire = 0;
            }
        }
        return r;
    }
    let mut slope = pitch_slope(w, id).clamp(-5.0, 5.0);
    let xo = f64::from(rnd2(w, "pr_firephoenixpl2")) / 128.0;
    let yo = f64::from(rnd2(w, "pr_firephoenixpl2")) / 128.0;
    let p = pos(w, id);
    let at = DVec3::new(p.x() + xo, p.y() + yo, p.z() + 26.0 + slope);
    slope += 0.1;
    let a = angle(w, id);
    if let Some(mo) = spawn_facing(w, "PhoenixFX2", at, a) {
        let speed = w.actors.get(mo).map_or(0.0, |x| x.speed);
        let (vx, vy) = angle_vec(a, speed);
        let v = vel(w, id);
        set_vel(w, mo, DVec3::new(vx + v.x(), vy + v.y(), speed * slope));
        if let Some(x) = w.actors.get_mut(mo) {
            x.target = Some(id);
        }
    }
    if !refire(w, id) {
        sound(w, id, "weapons/phoenixpowshoot");
    }
    ActionResult::Continue
}

fn gauntlet_attack(w: &mut World, id: ActorId, power: bool) -> ActionResult {
    if !player_fire(w, id) {
        return ActionResult::Continue;
    }
    let dmg = rnd_range(w, "pr_gauntletatk", 1, 8) * 2;
    let (dist, spread, puff) = if power {
        (4.0 * MELEE_RANGE, 2.8125, "GauntletPuff2")
    } else {
        (65.0, 5.625, "GauntletPuff1")
    };
    let a0 = angle(w, id);
    let ang = a0 + f64::from(rnd2(w, "pr_gauntletatk")) * (spread / 256.0);
    let slope = aim(w, id, ang, dist);
    let before = |w: &World, t: ActorId| health(w, t);
    let hit = line_attack(w, id, ang, dist, slope, dmg, puff);
    let Some(t) = hit else {
        if rnd(w, "pr_gauntletatk") > 64 {
            set_extralight(w, id, |l| i32::from(l == 0));
        }
        sound(w, id, "weapons/gauntletson");
        return ActionResult::Continue;
    };
    let _ = before(w, t);
    let r = rnd(w, "pr_gauntletatk");
    set_extralight(w, id, |_| {
        if r < 64 {
            0
        } else if r < 160 {
            1
        } else {
            2
        }
    });
    if power {
        if !has_flag(w, t, "DONTDRAIN") {
            if let Some(a) = w.actors.get_mut(id) {
                a.health = (a.health + (dmg >> 1)).min(100.max(a.health));
            }
        }
        sound(w, id, "weapons/gauntletspowhit");
    } else {
        sound(w, id, "weapons/gauntletshit");
    }
    let to = angle_to(w, id, t);
    let diff = delta_angle(a0, to);
    let new = if diff < 0.0 {
        if diff < -4.5 {
            to + 90.0 / 21.0
        } else {
            a0 - 4.5
        }
    } else if diff > 4.5 {
        to - 90.0 / 21.0
    } else {
        a0 + 4.5
    };
    set_angle(w, id, new);
    if let Some(a) = w.actors.get_mut(id) {
        a.flags |= crate::class::FLAG_JUSTATTACKED;
    }
    ActionResult::Continue
}

fn set_extralight(w: &mut World, id: ActorId, f: impl Fn(i32) -> i32) {
    for p in w.players.iter_mut().flatten() {
        if p.mo == Some(id) {
            p.extralight = f(p.extralight);
        }
    }
}

fn gen_wizard(w: &mut World, id: ActorId) -> ActionResult {
    let p = pos(w, id);
    let Some(mo) = spawn(w, "Wizard", p) else {
        return ActionResult::Continue;
    };
    let h = w.actors.get(mo).map_or(0.0, |a| a.height);
    if let Some(a) = w.actors.get_mut(mo) {
        a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() - h / 2.0);
    }
    let fits = w
        .actors
        .get(mo)
        .is_some_and(|a| crate::physics::check_position(w, a, a.pos, Some(mo)).ok);
    if !fits {
        if let Some(a) = w.actors.get_mut(mo) {
            a.flags &= !crate::class::FLAG_COUNTKILL;
        }
        destroy(w, mo);
        return ActionResult::Continue;
    }
    let owner = target(w, id);
    let friendly = owner.is_some_and(|o| has_flag(w, o, "FRIENDLY"));
    set_flag(w, mo, "FRIENDLY", friendly);
    if let Some(a) = w.actors.get_mut(mo) {
        a.master = owner;
    }
    set_vel(w, id, DVec3::ZERO);
    let _ = set_label(w, id, "Death");
    set_flag(w, id, "MISSILE", false);
    crate::specials::spawn_teleport_fog(w, p);
    ActionResult::Continue
}

fn lich_attack(w: &mut World, id: ActorId) -> ActionResult {
    const R1: [i32; 2] = [50, 150];
    const R2: [i32; 2] = [150, 200];
    let Some(t) = target(w, id) else {
        return ActionResult::Continue;
    };
    face_target(w, id);
    if in_melee_range(w, id) {
        let dmg = rnd_range(w, "pr_lichattack", 1, 8) * 6;
        let _ = damage(w, t, Some(id), Some(id), dmg);
        return ActionResult::Continue;
    }
    let far = usize::from(dist2d(w, id, t) > 8.0 * 64.0);
    let r = rnd(w, "pr_lichattack");
    if r < R1[far] {
        let _ = spawn_missile(w, id, t, "HeadFX1");
        sound(w, id, "ironlich/attack2");
    } else if r < R2[far] {
        if let Some(base) = spawn_missile(w, id, t, "HeadFX3") {
            let _ = set_label(w, base, "NoGrow");
            let (bp, bv, ba, bt) = w
                .actors
                .get(base)
                .map_or((DVec3::ZERO, DVec3::ZERO, 0.0, None), |a| {
                    (a.pos, a.vel, a.angle.degrees(), a.target)
                });
            for i in 0..5 {
                if i == 0 {
                    sound(w, id, "ironlich/attack1");
                }
                if let Some(fire) = spawn_facing(w, "HeadFX3", bp, ba) {
                    if let Some(a) = w.actors.get_mut(fire) {
                        a.target = bt;
                        a.vel = bv;
                        a.damage = 0;
                        a.health = (i + 1) * 2;
                    }
                }
            }
        }
    } else if let Some(mo) = spawn_missile(w, id, t, "Whirlwind") {
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() - 32.0);
            a.tracer = Some(t);
            a.health = 20 * 35;
        }
        sound(w, id, "ironlich/attack3");
    }
    ActionResult::Continue
}

fn skull_rod_storm(w: &mut World, id: ActorId) -> ActionResult {
    let hp = health(w, id);
    if let Some(a) = w.actors.get_mut(id) {
        a.health -= 1;
    }
    if hp == 0 {
        if let Some(t) = target(w, id) {
            if let Some(a) = w.actors.get_mut(t) {
                let (r1, r2) = a.ext.special_actor;
                a.ext.special_actor = (r1.filter(|r| *r != id), r2.filter(|r| *r != id));
            }
        }
        destroy(w, id);
        return ActionResult::Continue;
    }
    if rnd(w, "pr_skullrodstorm") < 25 {
        return ActionResult::Continue;
    }
    let xo = f64::from(rnd_range(w, "pr_skullrodstorm", -64, 63));
    let yo = f64::from(rnd_range(w, "pr_skullrodstorm", -64, 63));
    let p = pos(w, id);
    let Some(mo) = spawn(w, "RainPillar", DVec3::new(p.x() + xo, p.y() + yo, p.z())) else {
        return ActionResult::Continue;
    };
    let (_, ceil) = floor_ceil(w, mo);
    let t = target(w, id);
    let speed = w.actors.get(mo).map_or(0.0, |a| a.speed);
    if let Some(a) = w.actors.get_mut(mo) {
        a.pos = DVec3::new(a.pos.x(), a.pos.y(), ceil - a.height);
        a.target = t;
        a.vel = DVec3::new(1.0 / 65536.0, 0.0, -speed);
    }
    ActionResult::Continue
}

fn srcr1_attack(w: &mut World, id: ActorId) -> ActionResult {
    let Some(t) = target(w, id) else {
        return ActionResult::Continue;
    };
    let snd = class_sound(w, id, "attack");
    sound(w, id, &snd);
    if in_melee_range(w, id) {
        let dmg = rnd_range(w, "pr_srcr1attack", 1, 8) * 8;
        let _ = damage(w, t, Some(id), Some(id), dmg);
        return ActionResult::Continue;
    }
    let z = pos(w, id).z() + 48.0;
    let sh = spawn_health(w, id);
    if health(w, id) > (sh / 3) * 2 {
        let _ = spawn_missile_z(w, id, z, t, "SorcererFX1");
        return ActionResult::Continue;
    }
    if let Some(mo) = spawn_missile_z(w, id, z, t, "SorcererFX1") {
        let (a, vz) = (angle(w, mo), vel(w, mo).z());
        let _ = spawn_missile_angle_z_speed(w, id, z, "SorcererFX1", a - 3.0, vz, None);
        let _ = spawn_missile_angle_z_speed(w, id, z, "SorcererFX1", a + 3.0, vz, None);
    }
    if health(w, id) < sh / 3 {
        let s1 = w.actors.get(id).map_or(0, |a| a.ext.special1);
        if let Some(a) = w.actors.get_mut(id) {
            a.ext.special1 = i32::from(s1 == 0);
        }
        if s1 == 0 {
            let _ = set_label(w, id, "Missile2");
        }
    }
    ActionResult::Continue
}

/// `DSparilTeleport`: jump to a random `BossSpot` at least 128 units away.
fn dsparil_teleport(w: &mut World, id: ActorId) {
    let spots = actors_of(w, "BossSpot");
    if spots.is_empty() {
        return;
    }
    let start = (rnd(w, "pr_dsparilteleport") as usize) % spots.len();
    let p = pos(w, id);
    for k in 0..spots.len() {
        let s = spots[(start + k) % spots.len()];
        let sp = pos(w, s);
        if (sp.x() - p.x()).hypot(sp.y() - p.y()) < 128.0 {
            continue;
        }
        let ok = w
            .actors
            .get(id)
            .is_some_and(|a| crate::physics::check_position(w, a, sp, Some(id)).ok);
        if !ok {
            continue;
        }
        let ang = angle(w, s);
        let _ = spawn(w, "Sorcerer2Telefade", p);
        sound(w, id, "misc/teleport");
        if let Some(a) = w.actors.get_mut(id) {
            a.pos = sp;
            a.angle = Angle::from_degrees(ang);
            a.vel = DVec3::ZERO;
        }
        let _ = set_label(w, id, "Teleport");
        return;
    }
}

fn minotaur_friend(w: &World, id: ActorId) -> bool {
    w.classes
        .find_name("MinotaurFriend")
        .zip(w.actors.get(id))
        .is_some_and(|(f, a)| crate::action::class_is_or_derives(w, a.class, f))
}

fn maulator_expired(w: &mut World, id: ActorId) -> bool {
    let start = w
        .actors
        .get(id)
        .and_then(|a| a.ext.scratch.get("starttime").copied());
    match start {
        Some(s) if s >= 0.0 && f64::from(w.level.maptime) - s >= f64::from(MAULATORTICS) => {
            let _ = damage(w, id, None, None, 1_000_000);
            true
        }
        _ => false,
    }
}

fn minotaur_chase(w: &mut World, id: ActorId) -> ActionResult {
    if !minotaur_friend(w, id) {
        return crate::action::a_chase(w, id);
    }
    if let Some(a) = w.actors.get_mut(id) {
        a.render_style = crate::class::RenderStyle::Normal;
        a.alpha = 1.0;
    }
    if maulator_expired(w, id) {
        return ActionResult::Continue;
    }
    if rnd(w, "pr_minotaurchase") < 30 {
        let _ = minotaur_look(w, id);
    }
    let Some(t) = target(w, id).filter(|&t| health(w, t) > 0 && has_flag(w, t, "SHOOTABLE")) else {
        let _ = set_label(w, id, "Spawn");
        return ActionResult::Continue;
    };
    let _ = crate::action::a_face_movement_direction(w, id);
    if let Some(a) = w.actors.get_mut(id) {
        a.reaction_time = 0;
    }
    let class = w.actors.get(id).map(|a| a.class);
    let has = |w: &World, l: &str| class.is_some_and(|c| w.states.find_label(c, l).is_some());
    if has(w, "Melee") && in_melee_range(w, id) {
        let snd = class_sound(w, id, "attack");
        sound(w, id, &snd);
        let _ = set_label(w, id, "Melee");
        return ActionResult::Continue;
    }
    if has(w, "Missile") && crate::action::check_missile_range(w, id, t) {
        let _ = set_label(w, id, "Missile");
        return ActionResult::Continue;
    }
    if !crate::action::chase_move(w, id) {
        crate::action::new_chase_dir(w, id, t);
        let _ = crate::action::a_face_movement_direction(w, id);
    }
    if rnd(w, "pr_minotaurchase") < 6 {
        let snd = class_sound(w, id, "active");
        sound(w, id, &snd);
    }
    ActionResult::Continue
}

fn minotaur_decide(w: &mut World, id: ActorId) -> ActionResult {
    let friendly = has_flag(w, id, "SUMMONEDMONSTER");
    let Some(t) = target(w, id) else {
        return ActionResult::Continue;
    };
    if !friendly {
        sound(w, id, "minotaur/sight");
    }
    let dist = dist2d(w, id, t);
    let (tp, th) = (pos(w, t), w.actors.get(t).map_or(0.0, |a| a.height));
    let (sp, sh) = (pos(w, id), w.actors.get(id).map_or(0.0, |a| a.height));
    let (tfloor, _) = floor_ceil(w, t);
    if tp.z() + th > sp.z()
        && tp.z() + th < sp.z() + sh
        && dist < if friendly { 16.0 * 64.0 } else { 8.0 * 64.0 }
        && dist > 64.0
        && rnd(w, "pr_minotaurdecide") < 150
    {
        if let Some(c) = w.actors.get(id).map(|a| a.class) {
            if let Some(s) = w.states.find_label(c, "Charge") {
                w.set_state(id, Some(s), false);
            }
        }
        set_flag(w, id, "SKULLFLY", true);
        if !friendly {
            set_flag(w, id, "INVULNERABLE", true);
        }
        face_target(w, id);
        vel_from_angle(w, id, MNTR_CHARGE_SPEED);
        if let Some(a) = w.actors.get_mut(id) {
            a.ext.special1 = 35 / 2;
        }
    } else if (tp.z() - tfloor).abs() < 0.01
        && dist < 9.0 * 64.0
        && rnd(w, "pr_minotaurdecide") < if friendly { 100 } else { 220 }
    {
        let _ = set_label(w, id, "Hammer");
        if let Some(a) = w.actors.get_mut(id) {
            a.ext.special2 = 0;
        }
    } else {
        face_target(w, id);
    }
    ActionResult::Continue
}

fn minotaur_look(w: &mut World, id: ActorId) -> ActionResult {
    if !minotaur_friend(w, id) {
        return crate::action::a_look(w, id);
    }
    let master = tracer(w, id);
    if let Some(a) = w.actors.get_mut(id) {
        a.target = None;
    }
    let mut found = None;
    if w.deathmatch {
        let mos: Vec<ActorId> = w.players.iter().flatten().filter_map(|p| p.mo).collect();
        for mo in mos {
            if Some(mo) == master || health(w, mo) <= 0 || dist2d(w, id, mo) > MINOTAUR_LOOK_DIST {
                continue;
            }
            found = Some(mo);
            break;
        }
    }
    if found.is_none() {
        let ids: Vec<ActorId> = w.thinkers.default.clone();
        for mo in ids {
            if mo == id || Some(mo) == master {
                continue;
            }
            if !has_flag(w, mo, "ISMONSTER") || health(w, mo) <= 0 || !has_flag(w, mo, "SHOOTABLE")
            {
                continue;
            }
            if dist2d(w, id, mo) > MINOTAUR_LOOK_DIST {
                continue;
            }
            if has_flag(w, mo, "SUMMONEDMONSTER") && tracer(w, mo) == master {
                continue;
            }
            found = Some(mo);
            break;
        }
    }
    if let Some(t) = found {
        if let Some(a) = w.actors.get_mut(id) {
            a.target = Some(t);
        }
        let _ = set_label(w, id, "See");
    } else {
        let _ = set_label(w, id, "Roam");
    }
    ActionResult::Continue
}

fn minotaur_roam(w: &mut World, id: ActorId) -> ActionResult {
    if let Some(a) = w.actors.get_mut(id) {
        a.render_style = crate::class::RenderStyle::Normal;
        a.alpha = 1.0;
    }
    if minotaur_friend(w, id) && maulator_expired(w, id) {
        return ActionResult::Continue;
    }
    if rnd(w, "pr_minotaurroam") < 30 {
        let _ = minotaur_look(w, id);
    }
    if rnd(w, "pr_minotaurroam") < 6 {
        let d = rnd_range(w, "pr_minotaurroam", 0, 7);
        if let Some(a) = w.actors.get_mut(id) {
            a.move_dir = d as i8;
        }
        let _ = crate::action::a_face_movement_direction(w, id);
    }
    if !crate::action::chase_move(w, id) {
        let turn = rnd_range(w, "pr_minotaurroam", 0, 1);
        if let Some(a) = w.actors.get_mut(id) {
            a.move_dir = if turn != 0 {
                (a.move_dir + 1) % 8
            } else {
                (a.move_dir + 7) % 8
            };
        }
        let _ = crate::action::a_face_movement_direction(w, id);
    }
    ActionResult::Continue
}
