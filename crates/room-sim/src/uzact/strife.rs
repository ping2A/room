//! Strife actions (`actors/strife`).

use super::heretic::{class_speed, pitch_slope, player_fire, refire};
use super::*;

pub(super) const NAMES: &[&str] = &[
    "A_AcolyteBits",
    "A_AcolyteDie",
    "A_AlienSpectreDeath",
    "A_Bang4Cloud",
    "A_BeShadowyFoe",
    "A_Beacon",
    "A_Burnarea",
    "A_Burnination",
    "A_CrispyPlayer",
    "A_CrusaderChoose",
    "A_CrusaderDeath",
    "A_CrusaderRefire",
    "A_CrusaderSweepLeft",
    "A_CrusaderSweepRight",
    "A_DropFire",
    "A_EntityAttack",
    "A_EntityDeath",
    "A_Explode512",
    "A_FireArrow",
    "A_FireAssaultGun",
    "A_FireFlamer",
    "A_FireGrenade",
    "A_FireMauler1",
    "A_FireMauler2",
    "A_FireMauler2Pre",
    "A_FireMiniMissile",
    "A_FireSigil1",
    "A_FireSigil2",
    "A_FireSigil3",
    "A_FireSigil4",
    "A_FireSigil5",
    "A_FlameDie",
    "A_GetHurt",
    "A_GiveQuestItem",
    "A_HandLower",
    "A_HideDecepticon",
    "A_InquisitorAttack",
    "A_InquisitorCheckLand",
    "A_InquisitorDecide",
    "A_InquisitorJump",
    "A_InquisitorWalk",
    "A_ItBurnsItBurns",
    "A_JabDagger",
    "A_KlaxonBlare",
    "A_LightGoesOut",
    "A_LoremasterChain",
    "A_MaulerTorpedoWave",
    "A_OreSpawner",
    "A_ProgrammerDeath",
    "A_ProgrammerMelee",
    "A_ReaverRanged",
    "A_RemoveForceField",
    "A_RocketInFlight",
    "A_SelectPiece",
    "A_SelectSigilAttack",
    "A_SelectSigilDown",
    "A_SelectSigilView",
    "A_SentinelAttack",
    "A_SentinelBob",
    "A_ShootGun",
    "A_ShowElectricFlash",
    "A_SigilCharge",
    "A_SpawnEntity",
    "A_SpawnProgrammerBase",
    "A_SpectralBigBallLightning",
    "A_SpectralLightning",
    "A_SpectralLightningTail",
    "A_SpectralMissile",
    "A_Spectre3Attack",
    "A_SpectreChunkLarge",
    "A_SpectreChunkSmall",
    "A_SpotLightning",
    "A_StalkerAttack",
    "A_StalkerChaseDecide",
    "A_StalkerDrop",
    "A_StalkerLookInit",
    "A_StalkerWalk",
    "A_SubEntityDeath",
    "A_TemplarAttack",
    "A_TossArm",
    "A_TossGib",
    "A_TurretLook",
    "A_WakeOracleSpectre",
];

/// Strife `MTF_SHADOW` / `MTF_ALTSHADOW` map-thing flags.
const MTF_SHADOW: u32 = 0x0100;
const MTF_ALTSHADOW: u32 = 0x0200;

pub(super) fn dispatch(
    w: &mut World,
    id: ActorId,
    name: &str,
    args: &[String],
) -> Option<ActionResult> {
    let r = match name {
        "A_AcolyteBits" => {
            let sf = w.actors.get(id).map_or(0, |a| a.ext.spawn_flags);
            if sf & MTF_SHADOW != 0 {
                be_shadowy_foe(w, id);
            }
            if sf & MTF_ALTSHADOW != 0 && !has_flag(w, id, "SHADOW") {
                if let Some(a) = w.actors.get_mut(id) {
                    a.render_style = crate::class::RenderStyle::None;
                }
            }
            ActionResult::Continue
        }
        "A_AcolyteDie" => {
            if let Some(a) = w.actors.get_mut(id) {
                a.render_style = crate::class::RenderStyle::Normal;
                a.alpha = 1.0;
            }
            if class_name(w, id).eq_ignore_ascii_case("AcolyteBlue") {
                if let Some(p) = live_player(w) {
                    if check_boss_death(w, id) && count_inv(w, p, "QuestItem7") == 0 {
                        let _ = give(w, p, "QuestItem7", 1);
                        sound(w, id, "svox/voc14");
                    }
                }
            }
            ActionResult::Continue
        }
        "A_AlienSpectreDeath" => alien_spectre_death(w, id),
        "A_Bang4Cloud" => {
            let xo = f64::from(rnd_range(w, "pr_bang4cloud", 0, 3)) * (10.0 / 64.0);
            let yo = f64::from(rnd_range(w, "pr_bang4cloud", 0, 3)) * (10.0 / 64.0);
            let p = pos(w, id);
            let _ = spawn(w, "Bang4Cloud", DVec3::new(p.x() + xo, p.y() + yo, p.z()));
            ActionResult::Continue
        }
        "A_BeShadowyFoe" => {
            be_shadowy_foe(w, id);
            ActionResult::Continue
        }
        "A_Beacon" => beacon(w, id),
        "A_Burnarea" => call(w, id, "A_Explode", &["128", "128"]),
        "A_Burnination" => burnination(w, id),
        "A_CrispyPlayer" | "A_ItBurnsItBurns" | "A_HandLower" => {
            if name == "A_ItBurnsItBurns" {
                sound(w, id, "human/imonfire");
            }
            for p in w.players.iter_mut().flatten() {
                if p.mo == Some(id) {
                    match name {
                        "A_ItBurnsItBurns" => p.extralight = 3,
                        "A_HandLower" if p.extralight > 0 => p.extralight -= 1,
                        _ => {}
                    }
                }
            }
            ActionResult::Continue
        }
        "A_CrusaderChoose" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            let z = pos(w, id).z();
            let close = w.actors.get(id).is_some_and(|a| a.reaction_time == 0)
                && check_sight(w, id, t)
                && dist2d(w, id, t) < 264.0;
            if close {
                face_target(w, id);
                turn(w, id, -180.0 / 16.0);
                let _ = spawn_missile_z_aimed(w, id, z + 40.0, t, "FastFlameMissile");
            } else {
                if crate::action::check_missile_range(w, id, t) {
                    face_target(w, id);
                    let _ = spawn_missile_z_aimed(w, id, z + 56.0, t, "CrusaderMissile");
                    turn(w, id, -45.0 / 32.0);
                    let _ = spawn_missile_z_aimed(w, id, z + 40.0, t, "CrusaderMissile");
                    turn(w, id, 45.0 / 16.0);
                    let _ = spawn_missile_z_aimed(w, id, z + 40.0, t, "CrusaderMissile");
                    turn(w, id, -45.0 / 16.0);
                    if let Some(a) = w.actors.get_mut(id) {
                        a.reaction_time += 15;
                    }
                }
                let _ = set_label(w, id, "See");
            }
            ActionResult::Continue
        }
        "A_CrusaderDeath" => {
            if check_boss_death(w, id) {
                let _ = call(w, id, "Floor_LowerToLowest", &["667", "8"]);
            }
            ActionResult::Continue
        }
        "A_CrusaderRefire" => {
            let ok = target(w, id).is_some_and(|t| health(w, t) > 0 && check_sight(w, id, t));
            if !ok {
                let _ = set_label(w, id, "See");
            }
            ActionResult::Continue
        }
        "A_CrusaderSweepLeft" | "A_CrusaderSweepRight" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            turn(
                w,
                id,
                if name == "A_CrusaderSweepLeft" {
                    90.0 / 16.0
                } else {
                    -90.0 / 16.0
                },
            );
            let z = pos(w, id).z() + 48.0;
            if let Some(m) = spawn_missile_z_aimed(w, id, z, t, "FastFlameMissile") {
                let vz = vel(w, m).z() + 1.0;
                set_vel_z(w, m, vz);
            }
            ActionResult::Continue
        }
        "A_DropFire" => {
            let p = pos(w, id);
            if let Some(d) = spawn(w, "FireDroplet", DVec3::new(p.x(), p.y(), p.z() + 24.0)) {
                set_vel_z(w, d, -1.0);
            }
            explode(w, id, 64, 64.0);
            ActionResult::Continue
        }
        "A_EntityAttack" => {
            match rnd_range(w, "pr_entity", 0, 4) {
                0 => {
                    let _ = spot_lightning(w, id);
                }
                2 => spectral_missile(w, id, "SpectralLightningH3"),
                3 => {
                    let _ = spectre3_attack(w, id);
                }
                4 => spectral_missile(w, id, "SpectralLightningBigV2"),
                _ => spectral_missile(w, id, "SpectralLightningBigBall2"),
            }
            ActionResult::Continue
        }
        "A_EntityDeath" => {
            let r2 = w
                .classes
                .find_name("EntitySecond")
                .and_then(|c| w.classes.get(c))
                .map_or(40.0, |c| c.radius * 2.0);
            let spot = tracer(w, id).unwrap_or(id);
            let up = if tracer(w, id).is_some() { 70.0 } else { 0.0 };
            let sp = pos(w, spot);
            let a = angle(w, id);
            let t = target(w, id);
            for (i, turn) in [0.0, 90.0, -90.0].iter().enumerate() {
                let an = a + turn;
                let (dx, dy) = angle_vec(an, r2);
                if let Some(mo) = spawn_facing(
                    w,
                    "EntitySecond",
                    DVec3::new(sp.x() + dx, sp.y() + dy, sp.z() + up),
                    an,
                ) {
                    let friendly = has_flag(w, id, "FRIENDLY");
                    set_flag(w, mo, "FRIENDLY", friendly);
                    if let Some(x) = w.actors.get_mut(mo) {
                        x.target = t;
                    }
                    face_target(w, mo);
                    let (vx, vy) =
                        angle_vec(an, if i == 0 { 4.882_812_5 } else { r2 * 4.0 / 35.0 });
                    set_vel(w, mo, DVec3::new(vx, vy, 0.0));
                }
            }
            ActionResult::Continue
        }
        "A_Explode512" => {
            explode(w, id, 512, 512.0);
            if let Some(t) = target(w, id) {
                for p in w.players.iter_mut().flatten() {
                    if p.mo == Some(t) {
                        p.extralight = 5;
                    }
                }
            }
            if let Some(a) = w.actors.get_mut(id) {
                a.render_style = crate::class::RenderStyle::Add;
                a.alpha = 1.0;
            }
            ActionResult::Continue
        }
        "A_FireArrow" => {
            let proj = sarg(args, 0, "");
            if !player_fire(w, id) || proj.is_empty() {
                return Some(ActionResult::Continue);
            }
            let off = f64::from(rnd2(w, "pr_electric")) * (5.625 / 256.0) * accuracy_factor(w, id);
            let _ = spawn_player_missile(w, id, &proj, off);
            sound(w, id, "weapons/xbowshoot");
            ActionResult::Continue
        }
        "A_FireAssaultGun" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            sound(w, id, "weapons/assaultgun");
            if !deplete_ammo(w, id) {
                return Some(ActionResult::Continue);
            }
            let dmg = 4 * rnd_range(w, "pr_strifegun", 1, 3);
            let mut ang = angle(w, id);
            if refire(w, id) {
                ang += f64::from(rnd2(w, "pr_strifegun")) * (22.5 / 256.0) * accuracy_factor(w, id);
            }
            let slope = bullet_slope(w, id);
            let _ = line_attack(w, id, ang, PLAYER_MISSILE_RANGE, slope, dmg, "StrifePuff");
            ActionResult::Continue
        }
        "A_FireFlamer" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let d = f64::from(rnd2(w, "pr_flamethrower")) * (5.625 / 256.0);
            turn(w, id, d);
            if let Some(mo) = spawn_player_missile(w, id, "FlameMissile", 0.0) {
                let vz = vel(w, mo).z() + 5.0;
                set_vel_z(w, mo, vz);
            }
            ActionResult::Continue
        }
        "A_FireGrenade" => {
            let ty = sarg(args, 0, "");
            let ofs = farg(args, 1, 0.0);
            if !player_fire(w, id) || ty.is_empty() {
                return Some(ActionResult::Continue);
            }
            let p = pos(w, id);
            if let Some(a) = w.actors.get_mut(id) {
                a.pos = DVec3::new(p.x(), p.y(), p.z() + 32.0);
            }
            let g = spawn_sub_missile(w, id, &ty, Some(id));
            if let Some(a) = w.actors.get_mut(id) {
                a.pos = p;
            }
            let Some(g) = g else {
                return Some(ActionResult::Continue);
            };
            let see = class_sound(w, g, "see");
            sound(w, g, &see);
            let speed = w.actors.get(g).map_or(0.0, |a| a.speed);
            set_vel_z(w, g, pitch_slope(w, id).clamp(-5.0, 5.0) * speed + 8.0);
            let a = angle(w, id);
            let (r, gr) = (
                w.actors.get(id).map_or(0.0, |x| x.radius),
                w.actors.get(g).map_or(0.0, |x| x.radius),
            );
            let (ox, oy) = angle_vec(a, r + gr);
            let (ox2, oy2) = angle_vec(a + ofs, 15.0);
            if let Some(x) = w.actors.get_mut(g) {
                x.pos = DVec3::new(x.pos.x() + ox + ox2, x.pos.y() + oy + oy2, x.pos.z());
            }
            ActionResult::Continue
        }
        "A_FireMauler1" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            sound(w, id, "weapons/mauler1");
            if !deplete_ammo(w, id) {
                return Some(ActionResult::Continue);
            }
            let slope = bullet_slope(w, id);
            let a = angle(w, id);
            for _ in 0..20 {
                let dmg = 5 * rnd_range(w, "pr_mauler1", 1, 3);
                let ang = a + f64::from(rnd2(w, "pr_mauler1")) * (11.25 / 256.0);
                let s = slope
                    + (f64::from(rnd2(w, "pr_mauler1")) * (7.097 / 256.0))
                        .to_radians()
                        .tan();
                let _ = line_attack(w, id, ang, PLAYER_MISSILE_RANGE, s, dmg, "MaulerPuff");
            }
            ActionResult::Continue
        }
        "A_FireMauler2" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let _ = spawn_player_missile(w, id, "MaulerTorpedo", 0.0);
            let _ = damage(w, id, Some(id), None, 20);
            let a = angle(w, id);
            thrust(w, id, a + 180.0, 7.8125);
            ActionResult::Continue
        }
        "A_FireMauler2Pre" => {
            sound(w, id, "weapons/mauler2charge");
            let _ = rnd2(w, "pr_mauler2");
            let _ = rnd2(w, "pr_mauler2");
            ActionResult::Continue
        }
        "A_FireMiniMissile" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let off =
                f64::from(rnd2(w, "pr_minimissile")) * (11.25 / 256.0) * accuracy_factor(w, id);
            let _ = spawn_player_missile(w, id, "MiniMissile", off);
            ActionResult::Continue
        }
        "A_FireSigil1" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            sigil_cost(w, id, 1);
            let a = angle(w, id);
            let t = crate::hitscan::aim_line_attack_target(w, id, Angle::from_degrees(a), 1024.0)
                .map(|(_, t)| t);
            let spot = if let Some(t) = t {
                let (tp, (tf, _)) = (pos(w, t), floor_ceil(w, t));
                let s = spawn(w, "SpectralLightningSpot", DVec3::new(tp.x(), tp.y(), tf));
                if let Some(x) = s.and_then(|s| w.actors.get_mut(s)) {
                    x.tracer = Some(t);
                }
                s
            } else {
                let p = pos(w, id);
                let s = spawn_facing(w, "SpectralLightningSpot", p, a);
                if let Some(s) = s {
                    let (vx, vy) = angle_vec(a, 28.0);
                    set_vel(w, s, DVec3::new(vx, vy, 0.0));
                }
                s
            };
            if let Some(x) = spot.and_then(|s| w.actors.get_mut(s)) {
                x.target = Some(id);
                x.flags |= crate::class::FLAG_FRIENDLY;
            }
            ActionResult::Continue
        }
        "A_FireSigil2" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            sigil_cost(w, id, 2);
            let _ = spawn_player_missile(w, id, "SpectralLightningH1", 0.0);
            ActionResult::Continue
        }
        "A_FireSigil3" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            sigil_cost(w, id, 3);
            let z0 = pos(w, id).z();
            turn(w, id, -90.0);
            for _ in 0..20 {
                turn(w, id, 9.0);
                if let Some(s) = spawn_sub_missile(w, id, "SpectralLightningBall1", Some(id)) {
                    if let Some(x) = w.actors.get_mut(s) {
                        x.pos = DVec3::new(x.pos.x(), x.pos.y(), z0 + 32.0);
                    }
                }
            }
            turn(w, id, -90.0);
            ActionResult::Continue
        }
        "A_FireSigil4" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            sigil_cost(w, id, 4);
            let a = angle(w, id);
            let t = crate::hitscan::aim_line_attack_target(w, id, Angle::from_degrees(a), 1024.0)
                .map(|(_, t)| t);
            if let Some(s) = spawn_player_missile(w, id, "SpectralLightningBigV1", 0.0) {
                if let Some(t) = t {
                    if let Some(x) = w.actors.get_mut(s) {
                        x.tracer = Some(t);
                    }
                } else {
                    let speed = w.actors.get(s).map_or(0.0, |x| x.speed);
                    let (vx, vy) = angle_vec(a, speed);
                    let vz = vel(w, s).z();
                    set_vel(w, s, DVec3::new(vx, vy, vz));
                }
            }
            ActionResult::Continue
        }
        "A_FireSigil5" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            sigil_cost(w, id, 5);
            let _ = spawn_player_missile(w, id, "SpectralLightningBigBall1", 0.0);
            ActionResult::Continue
        }
        "A_FlameDie" => {
            set_flag(w, id, "NOGRAVITY", true);
            let vz = f64::from(rnd_range(w, "pr_flamedie", 0, 3));
            set_vel_z(w, id, vz);
            ActionResult::Continue
        }
        "A_GetHurt" => {
            set_flag(w, id, "INCOMBAT", true);
            if rnd_range(w, "pr_hurtme", 0, 4) == 0 {
                let s = class_sound(w, id, "pain");
                sound(w, id, &s);
                if let Some(a) = w.actors.get_mut(id) {
                    a.health -= 1;
                }
            }
            if health(w, id) <= 0 {
                let t = target(w, id);
                let _ = damage(w, id, t, t, 1_000_000);
            }
            ActionResult::Continue
        }
        "A_GiveQuestItem" => {
            let q = iarg(args, 0, -1);
            if q >= 0 {
                give_all_players(w, &format!("QuestItem{q}"));
            }
            mid_print(w, &format!("$TXT_QUEST_{q}"));
            ActionResult::Continue
        }
        "A_HideDecepticon" => {
            let _ = call(w, id, "Door_Close", &["999", "64"]);
            if target(w, id).is_some_and(|t| is_player(w, t)) {
                let _ = call(w, id, "A_AlertMonsters", &[]);
            }
            ActionResult::Continue
        }
        "A_InquisitorAttack" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            face_target(w, id);
            let z = pos(w, id).z() + 32.0;
            turn(w, id, -45.0 / 32.0);
            if let Some(p) = spawn_missile_z_aimed(w, id, z, t, "InquisitorShot") {
                let vz = vel(w, p).z() + 9.0;
                set_vel_z(w, p, vz);
            }
            turn(w, id, 45.0 / 16.0);
            if let Some(p) = spawn_missile_z_aimed(w, id, z, t, "InquisitorShot") {
                let vz = vel(w, p).z() + 16.0;
                set_vel_z(w, p, vz);
            }
            ActionResult::Continue
        }
        "A_InquisitorCheckLand" => {
            let rt = w.actors.get_mut(id).map_or(0, |a| {
                a.reaction_time -= 1;
                a.reaction_time
            });
            let v = vel(w, id);
            let (floor, _) = floor_ceil(w, id);
            if rt < 0 || v.x() == 0.0 || v.y() == 0.0 || pos(w, id).z() <= floor {
                let _ = set_label(w, id, "See");
                if let Some(a) = w.actors.get_mut(id) {
                    a.reaction_time = 0;
                }
                set_flag(w, id, "NOGRAVITY", false);
                return Some(ActionResult::Continue);
            }
            sound(w, id, "inquisitor/jump");
            ActionResult::Continue
        }
        "A_InquisitorDecide" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            face_target(w, id);
            let near = w.actors.get(id).is_some_and(|a| a.reaction_time == 0)
                && check_sight(w, id, t)
                && dist2d(w, id, t) < 264.0;
            if !near {
                let _ = set_label(w, id, "Grenade");
            }
            if (pos(w, t).z() - pos(w, id).z()).abs() > 1e-6 {
                let (_, ceil) = floor_ceil(w, id);
                let h = w.actors.get(id).map_or(0.0, |a| a.height);
                if pos(w, id).z() + h + 54.0 < ceil {
                    let _ = set_label(w, id, "Jump");
                }
            }
            ActionResult::Continue
        }
        "A_InquisitorJump" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            sound(w, id, "inquisitor/jump");
            if let Some(a) = w.actors.get_mut(id) {
                a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + 64.0);
            }
            face_target(w, id);
            let speed = w.actors.get(id).map_or(0.0, |a| a.speed) * (2.0 / 3.0);
            vel_from_angle(w, id, speed);
            let dist = (dist2d(w, id, t) / speed.max(1e-6)).max(1.0);
            let vz = (pos(w, t).z() - pos(w, id).z()) / dist;
            set_vel_z(w, id, vz);
            if let Some(a) = w.actors.get_mut(id) {
                a.reaction_time = 60;
            }
            set_flag(w, id, "NOGRAVITY", true);
            ActionResult::Continue
        }
        "A_InquisitorWalk" | "A_StalkerWalk" => {
            sound(
                w,
                id,
                if name == "A_InquisitorWalk" {
                    "inquisitor/walk"
                } else {
                    "stalker/walk"
                },
            );
            crate::action::a_chase(w, id)
        }
        "A_JabDagger" => {
            let dmg = if count_inv(w, id, "SVETalismanPowerup") > 0 {
                1000
            } else {
                let stamina = w.actors.get(id).map_or(0, |a| a.stamina);
                let power = (stamina / 10).min(10);
                let mut d = rnd_range(w, "pr_jabdagger", 0, power + 7) * (power + 2);
                if count_inv(w, id, "PowerStrength") > 0 {
                    d *= 10;
                }
                d
            };
            let ang = angle(w, id) + f64::from(rnd2(w, "pr_jabdagger")) * (5.625 / 256.0);
            let slope = aim(w, id, ang, 80.0);
            if let Some(t) = line_attack(w, id, ang, 80.0, slope, dmg, "StrifeSpark") {
                let snd = if has_flag(w, t, "NOBLOOD") {
                    "misc/metalhit"
                } else {
                    "misc/meathit"
                };
                sound(w, id, snd);
                let a = angle_to(w, id, t);
                set_angle(w, id, a);
                if let Some(x) = w.actors.get_mut(id) {
                    x.flags |= crate::class::FLAG_JUSTATTACKED;
                }
            } else {
                sound(w, id, "misc/swish");
            }
            ActionResult::Continue
        }
        "A_KlaxonBlare" => {
            let rt = w.actors.get_mut(id).map_or(0, |a| {
                a.reaction_time -= 1;
                a.reaction_time
            });
            if rt < 0 {
                let def = w
                    .actors
                    .get(id)
                    .and_then(|a| w.classes.get(a.class))
                    .map_or(8, |c| c.reaction_time);
                if let Some(a) = w.actors.get_mut(id) {
                    a.target = None;
                    a.reaction_time = def;
                }
                let _ = turret_look(w, id);
                if target(w, id).is_none() {
                    let _ = set_label(w, id, "Spawn");
                } else if let Some(a) = w.actors.get_mut(id) {
                    a.reaction_time = 50;
                }
            }
            let rt = w.actors.get(id).map_or(0, |a| a.reaction_time);
            if rt == 2 {
                let _ = call(w, id, "A_AlertMonsters", &[]);
            } else if rt > 50 {
                sound(w, id, "misc/alarm");
            }
            ActionResult::Continue
        }
        "A_LightGoesOut" => {
            let p = pos(w, id);
            if let Some(sid) = crate::physics::sector_at_xy(w, p.x(), p.y()) {
                if let Some(s) = w.sectors.get_mut(sid) {
                    s.light = 0;
                }
                let tag = w
                    .map
                    .sectors
                    .get(sid)
                    .and_then(|s| s.tags.first().copied())
                    .unwrap_or(0);
                if tag != 0 {
                    let _ = call(w, id, "Floor_LowerToLowest", &[&tag.to_string(), "65536"]);
                }
            }
            for _ in 0..8 {
                if let Some(mo) = spawn(w, "Rubble1", p) {
                    let t = rnd_range(w, "pr_lightout", 0, 7);
                    let vx = f64::from(t - rnd_range(w, "pr_lightout", 0, 15));
                    let t = rnd_range(w, "pr_lightout", 0, 7);
                    let vy = f64::from(t - rnd_range(w, "pr_lightout", 0, 15));
                    let vz = f64::from(rnd_range(w, "pr_lightout", 7, 10));
                    set_vel(w, mo, DVec3::new(vx, vy, vz));
                }
            }
            ActionResult::Continue
        }
        "A_LoremasterChain" => {
            sound(w, id, "loremaster/active");
            let (p, v) = (pos(w, id), vel(w, id));
            for k in [0.0, 0.5, 1.0] {
                let _ = spawn(
                    w,
                    "LoreShot2",
                    DVec3::new(p.x() - v.x() * k, p.y() - v.y() * k, p.z() - v.z() * k),
                );
            }
            ActionResult::Continue
        }
        "A_MaulerTorpedoWave" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            let z0 = pos(w, id).z();
            let wh = w
                .classes
                .find_name("MaulerTorpedoWave")
                .and_then(|c| w.classes.get(c))
                .map_or(0.0, |c| c.height);
            let (_, ceil) = floor_ceil(w, id);
            if ceil < z0 + wh {
                if let Some(a) = w.actors.get_mut(id) {
                    a.pos = DVec3::new(a.pos.x(), a.pos.y(), ceil - wh);
                }
            }
            turn(w, id, 180.0);
            for _ in 0..80 {
                turn(w, id, 4.5);
                let _ = spawn_sub_missile(w, id, "MaulerTorpedoWave", Some(t));
            }
            if let Some(a) = w.actors.get_mut(id) {
                a.pos = DVec3::new(a.pos.x(), a.pos.y(), z0);
            }
            ActionResult::Continue
        }
        "A_OreSpawner" => {
            if w.deathmatch {
                return Some(ActionResult::Continue);
            }
            let mos: Vec<ActorId> = w.players.iter().flatten().filter_map(|p| p.mo).collect();
            if !mos.iter().any(|&m| dist2d(w, id, m) < 2048.0) {
                return Some(ActionResult::Continue);
            }
            if actors_of(w, "DegninOre").len() >= 2 {
                return Some(ActionResult::Continue);
            }
            let p = pos(w, id);
            let _ = spawn(w, "DegninOre", p);
            ActionResult::Continue
        }
        "A_ProgrammerDeath" => {
            if !check_boss_death(w, id) {
                return Some(ActionResult::Continue);
            }
            if let Some(p) = live_player(w) {
                let _ = give(w, p, "ProgLevelEnder", 1);
            }
            crate::action::a_boss_death(w, id)
        }
        "A_ProgrammerMelee" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            face_target(w, id);
            if !in_melee_range(w, id) {
                return Some(ActionResult::Continue);
            }
            sound(w, id, "programmer/clank");
            let dmg = rnd_range(w, "pr_programmer", 1, 10) * 6;
            let _ = damage(w, t, Some(id), Some(id), dmg);
            ActionResult::Continue
        }
        "A_ReaverRanged" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            face_target(w, id);
            sound(w, id, "reaver/attack");
            let a = angle(w, id);
            let slope = aim(w, id, a, MISSILE_RANGE);
            for _ in 0..3 {
                let ang = a + f64::from(rnd2(w, "pr_reaverattack")) * (22.5 / 256.0);
                let dmg = rnd_range(w, "pr_reaverattack", 1, 8) * 3;
                let _ = line_attack(w, id, ang, MISSILE_RANGE, slope, dmg, "StrifePuff");
            }
            ActionResult::Continue
        }
        "A_RemoveForceField" => {
            set_flag(w, id, "SPECIAL", false);
            remove_force_field(w, id);
            ActionResult::Continue
        }
        "A_RocketInFlight" => {
            sound(w, id, "misc/missileinflight");
            let (p, v) = (pos(w, id), vel(w, id));
            let _ = spawn(w, "MiniMissilePuff", p);
            if let Some(t) = spawn(
                w,
                "RocketTrail",
                DVec3::new(p.x() - v.x(), p.y() - v.y(), p.z()),
            ) {
                set_vel_z(w, t, 1.0);
            }
            ActionResult::Continue
        }
        "A_SelectPiece" => {
            let pieces = health(w, id).min(5);
            if pieces > 1 {
                set_spawn_state_plus(w, id, pieces as u32);
            }
            ActionResult::Continue
        }
        "A_SelectSigilAttack" | "A_SelectSigilDown" | "A_SelectSigilView" => {
            // The weapon state offsets come from the Sigil's piece count.
            let pieces = sigil_pieces(w, id);
            let add = match name {
                "A_SelectSigilAttack" => 4 * pieces - 3,
                _ => pieces,
            };
            jump(w, id, &add.max(0).to_string())
        }
        "A_SentinelAttack" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            let z = pos(w, id).z() + 32.0;
            if let Some(m) = spawn_missile_z_aimed(w, id, z, t, "SentinelFX2") {
                let mv = vel(w, m);
                if mv.x() != 0.0 || mv.y() != 0.0 {
                    let (mr, ma, mp) = (
                        w.actors.get(m).map_or(0.0, |a| a.radius),
                        angle(w, m),
                        pos(w, id),
                    );
                    for i in (2..=8).rev() {
                        let (dx, dy) = angle_vec(ma, mr * f64::from(i));
                        let at = DVec3::new(
                            mp.x() + dx,
                            mp.y() + dy,
                            mp.z() + 32.0 + mv.z() / 4.0 * f64::from(i),
                        );
                        if let Some(tr) = spawn(w, "SentinelFX1", at) {
                            if let Some(x) = w.actors.get_mut(tr) {
                                x.target = Some(id);
                                x.vel = mv;
                            }
                        }
                    }
                    if let Some(x) = w.actors.get_mut(m) {
                        x.pos = DVec3::new(x.pos.x(), x.pos.y(), x.pos.z() + mv.z() / 4.0);
                    }
                }
            }
            ActionResult::Continue
        }
        "A_SentinelBob" => {
            if has_flag(w, id, "INFLOAT") {
                set_vel_z(w, id, 0.0);
                return Some(ActionResult::Continue);
            }
            if w.actors.get(id).is_some_and(|a| a.threshold != 0) {
                return Some(ActionResult::Continue);
            }
            let (floor, ceil) = floor_ceil(w, id);
            let h = w.actors.get(id).map_or(0.0, |a| a.height);
            let maxz = ceil - h - 16.0;
            let minz = (floor + 96.0).min(maxz);
            let z = pos(w, id).z();
            let vz = vel(w, id).z() + if minz < z { -1.0 } else { 1.0 };
            set_vel_z(w, id, vz);
            if let Some(a) = w.actors.get_mut(id) {
                a.reaction_time = if minz >= z { 4 } else { 0 };
            }
            ActionResult::Continue
        }
        "A_ShootGun" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            sound(w, id, "monsters/rifle");
            face_target(w, id);
            let a = angle(w, id);
            let slope = aim(w, id, a, MISSILE_RANGE);
            let ang = a + f64::from(rnd2(w, "pr_shootgun")) * (11.25 / 256.0);
            let dmg = 3 * rnd_range(w, "pr_shootgun", 1, 5);
            let _ = line_attack(w, id, ang, MISSILE_RANGE, slope, dmg, "StrifePuff");
            ActionResult::Continue
        }
        "A_ShowElectricFlash" => call(w, id, "A_GunFlash", &["Flash"]),
        "A_SigilCharge" => {
            sound(w, id, "weapons/sigilcharge");
            for p in w.players.iter_mut().flatten() {
                if p.mo == Some(id) {
                    p.extralight = 2;
                }
            }
            ActionResult::Continue
        }
        "A_SpawnEntity" => {
            let p = pos(w, id);
            let a = angle(w, id);
            if let Some(e) =
                spawn_facing(w, "EntityBoss", DVec3::new(p.x(), p.y(), p.z() + 70.0), a)
            {
                let friendly = has_flag(w, id, "FRIENDLY");
                set_flag(w, e, "FRIENDLY", friendly);
                set_vel_z(w, e, 5.0);
                if let Some(x) = w.actors.get_mut(e) {
                    x.tracer = Some(id);
                }
            }
            ActionResult::Continue
        }
        "A_SpawnProgrammerBase" => {
            let p = pos(w, id);
            let a = angle(w, id) + 180.0 + f64::from(rnd2(w, "pr_programmer")) * (360.0 / 1024.0);
            if let Some(b) = spawn_facing(
                w,
                "ProgrammerBase",
                DVec3::new(p.x(), p.y(), p.z() + 24.0),
                a,
            ) {
                let speed = w.actors.get(b).map_or(0.0, |x| x.speed);
                let (vx, vy) = angle_vec(a, speed);
                let vz = f64::from(rnd(w, "pr_programmer")) / 128.0;
                set_vel(w, b, DVec3::new(vx, vy, vz));
            }
            ActionResult::Continue
        }
        "A_SpectralBigBallLightning" => {
            let t = target(w, id);
            turn(w, id, 90.0);
            let _ = spawn_sub_missile(w, id, "SpectralLightningH3", t);
            turn(w, id, 180.0);
            let _ = spawn_sub_missile(w, id, "SpectralLightningH3", t);
            turn(w, id, -270.0);
            let _ = spawn_sub_missile(w, id, "SpectralLightningH3", t);
            ActionResult::Continue
        }
        "A_SpectralLightning" => {
            if let Some(a) = w.actors.get_mut(id) {
                if a.threshold != 0 {
                    a.threshold -= 1;
                }
            }
            let dx = f64::from(rnd2_mask(w, "pr_zap5", 3));
            let dy = f64::from(rnd2_mask(w, "pr_zap5", 3));
            if let Some(a) = w.actors.get_mut(id) {
                a.vel = DVec3::new(a.vel.x() + dx, a.vel.y() + dy, a.vel.z());
            }
            let xo = f64::from(rnd2_mask(w, "pr_zap5", 3)) * 50.0;
            let yo = f64::from(rnd2_mask(w, "pr_zap5", 3)) * 50.0;
            let cls = if w.actors.get(id).is_some_and(|a| a.threshold > 25) {
                "SpectralLightningV2"
            } else {
                "SpectralLightningV1"
            };
            let p = pos(w, id);
            let t = target(w, id);
            for (c, ox, oy) in [(cls, xo, yo), ("SpectralLightningV2", 0.0, 0.0)] {
                let at = DVec3::new(p.x() + ox, p.y() + oy, p.z());
                if let Some(f) = spawn(w, c, at) {
                    let (_, ceil) = floor_ceil(w, f);
                    if let Some(x) = w.actors.get_mut(f) {
                        x.pos = DVec3::new(x.pos.x(), x.pos.y(), ceil - x.height);
                        x.target = t;
                        x.vel = DVec3::new(0.0, 0.0, -18.0);
                    }
                }
            }
            ActionResult::Continue
        }
        "A_SpectralLightningTail" => {
            let (p, v, a) = (pos(w, id), vel(w, id), angle(w, id));
            let _ = spawn_facing(
                w,
                "SpectralLightningHTail",
                DVec3::new(p.x() - v.x(), p.y() - v.y(), p.z()),
                a,
            );
            ActionResult::Continue
        }
        "A_SpectralMissile" => {
            let cls = sarg(args, 0, "");
            if !cls.is_empty() {
                spectral_missile(w, id, &cls);
            }
            ActionResult::Continue
        }
        "A_Spectre3Attack" => spectre3_attack(w, id),
        "A_SpectreChunkLarge" | "A_SpectreChunkSmall" => {
            let (cls, zmax) = if name == "A_SpectreChunkLarge" {
                ("AlienChunkLarge", 7)
            } else {
                ("AlienChunkSmall", 15)
            };
            let p = pos(w, id);
            if let Some(mo) = spawn(w, cls, DVec3::new(p.x(), p.y(), p.z() + 10.0)) {
                let t = rnd_range(w, "pr_spectrechunk", 0, 7);
                let vx = f64::from(t - rnd_range(w, "pr_spectrechunk", 0, 15));
                let t = rnd_range(w, "pr_spectrechunk", 0, 7);
                let vy = f64::from(t - rnd_range(w, "pr_spectrechunk", 0, 15));
                let vz = f64::from(rnd_range(w, "pr_spectrechunk", 0, zmax));
                set_vel(w, mo, DVec3::new(vx, vy, vz));
            }
            ActionResult::Continue
        }
        "A_SpotLightning" => spot_lightning(w, id),
        "A_StalkerAttack" => {
            if has_flag(w, id, "NOGRAVITY") {
                let _ = set_label(w, id, "Drop");
            } else if let Some(t) = target(w, id) {
                face_target(w, id);
                if in_melee_range(w, id) {
                    let dmg = rnd_range(w, "pr_stalker", 1, 8) * 2;
                    let _ = damage(w, t, Some(id), Some(id), dmg);
                }
            }
            ActionResult::Continue
        }
        "A_StalkerChaseDecide" => {
            if !has_flag(w, id, "NOGRAVITY") {
                let _ = set_label(w, id, "SeeFloor");
            } else {
                let (_, ceil) = floor_ceil(w, id);
                let h = w.actors.get(id).map_or(0.0, |a| a.height);
                if ceil > pos(w, id).z() + h {
                    let _ = set_label(w, id, "Drop");
                }
            }
            ActionResult::Continue
        }
        "A_StalkerDrop" => {
            set_flag(w, id, "NOVERTICALMELEERANGE", false);
            set_flag(w, id, "NOGRAVITY", false);
            ActionResult::Continue
        }
        "A_StalkerLookInit" => {
            let label = if has_flag(w, id, "NOGRAVITY") {
                "LookCeiling"
            } else {
                "LookFloor"
            };
            let (class, next) = w
                .actors
                .get(id)
                .map(|a| {
                    (
                        a.class,
                        a.state.and_then(|s| w.states.get(s)).and_then(|s| s.next),
                    )
                })
                .unzip();
            if let Some(st) = class.and_then(|c| w.states.find_label(c, label)) {
                if next.flatten() != Some(st) {
                    w.set_state(id, Some(st), true);
                }
            }
            ActionResult::Continue
        }
        "A_SubEntityDeath" => {
            if check_boss_death(w, id) {
                w.level.exiting = true;
            }
            ActionResult::Continue
        }
        "A_TemplarAttack" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            sound(w, id, "templar/shoot");
            face_target(w, id);
            let a = angle(w, id);
            let slope = aim(w, id, a, MISSILE_RANGE);
            for _ in 0..10 {
                let dmg = rnd_range(w, "pr_templar", 1, 4) * 2;
                let ang = a + f64::from(rnd2(w, "pr_templar")) * (11.25 / 256.0);
                let s = slope
                    + (f64::from(rnd2(w, "pr_templar")) * (7.097 / 256.0))
                        .to_radians()
                        .tan();
                let _ = line_attack(w, id, ang, MISSILE_RANGE + 64.0, s, dmg, "MaulerPuff");
            }
            ActionResult::Continue
        }
        "A_TossArm" => {
            let p = pos(w, id);
            let a = angle(w, id) - 90.0 + f64::from(rnd2(w, "pr_inquisitor")) * (360.0 / 1024.0);
            if let Some(arm) = spawn_facing(
                w,
                "InquisitorArm",
                DVec3::new(p.x(), p.y(), p.z() + 24.0),
                a,
            ) {
                let speed = class_speed(w, "InquisitorArm") / 8.0;
                let (vx, vy) = angle_vec(a, speed);
                let vz = f64::from(rnd(w, "pr_inquisitor")) / 64.0;
                set_vel(w, arm, DVec3::new(vx, vy, vz));
            }
            ActionResult::Continue
        }
        "A_TossGib" => {
            let ty = sarg(args, 0, "");
            let zofs = farg(args, 1, 24.0);
            let ty = if ty.is_empty() {
                if has_flag(w, id, "NOBLOOD") {
                    "Junk".to_owned()
                } else {
                    "Meat".to_owned()
                }
            } else {
                ty
            };
            let p = pos(w, id);
            if let Some(g) = spawn(w, &ty, DVec3::new(p.x(), p.y(), p.z() + zofs)) {
                let a = f64::from(rnd(w, "pr_gibtosser")) * (360.0 / 256.0);
                set_angle(w, g, a);
                let sp = f64::from(rnd_range(w, "pr_gibtosser", 0, 15));
                let (vx, vy) = angle_vec(a, sp);
                let vz = f64::from(rnd_range(w, "pr_gibtosser", 0, 15));
                set_vel(w, g, DVec3::new(vx, vy, vz));
            }
            ActionResult::Continue
        }
        "A_TurretLook" => turret_look(w, id),
        "A_WakeOracleSpectre" => {
            if let Some(s) = actors_of(w, "AlienSpectre3").first().copied() {
                let t = target(w, id);
                if health(w, s) > 0 && t != Some(s) {
                    let heard = w.actors.get(id).and_then(|a| a.last_heard);
                    if let Some(x) = w.actors.get_mut(s) {
                        x.last_heard = heard;
                        x.target = t;
                    }
                    let _ = set_label(w, s, "See");
                }
            }
            ActionResult::Continue
        }
        _ => return None,
    };
    Some(r)
}

fn turn(w: &mut World, id: ActorId, deg: f64) {
    let a = angle(w, id) + deg;
    set_angle(w, id, a);
}

/// `AccuracyFactor()`: `1 / (accuracy / 25 + 1)`… GZDoom: `1. / (Accuracy * 2 / 100. + 1)`.
fn accuracy_factor(w: &World, id: ActorId) -> f64 {
    let acc = w.actors.get(id).map_or(0, |a| a.accuracy);
    1.0 / (f64::from(acc) * 2.0 / 100.0 + 1.0)
}

fn be_shadowy_foe(w: &mut World, id: ActorId) {
    if let Some(a) = w.actors.get_mut(id) {
        a.render_style = crate::class::RenderStyle::Translucent;
        a.alpha = 0.4;
        a.flags &= !crate::class::FLAG_FRIENDLY;
    }
}

/// The Sigil hurts its wielder `4 × n` (no armor).
fn sigil_cost(w: &mut World, id: ActorId, n: i32) {
    if let Some(a) = w.actors.get_mut(id) {
        a.health -= 4 * n;
    }
    sound(w, id, "weapons/sigilcharge");
}

fn sigil_pieces(w: &World, id: ActorId) -> i32 {
    count_inv(w, id, "Sigil").clamp(1, 5)
}

fn remove_force_field(w: &mut World, id: ActorId) {
    let p = pos(w, id);
    let Some(sid) = crate::physics::sector_at_xy(w, p.x(), p.y()) else {
        return;
    };
    // GZDoom `RemoveForceField`: clear the force-field two-sided lines of the sector.
    let lines: Vec<room_core::LineId> = w
        .map
        .sectors
        .get(sid)
        .map(|s| s.lines.clone())
        .unwrap_or_default();
    for lid in lines {
        if w.line_special(lid) == 33 {
            let raw = lid.raw();
            w.line_specials.insert(raw, 0);
            let f =
                w.line_flags(lid) & !(crate::world::ML_BLOCKING | crate::world::ML_BLOCKEVERYTHING);
            w.line_flags.insert(raw, f);
        }
    }
}

fn turret_look(w: &mut World, id: ActorId) -> ActionResult {
    if has_flag(w, id, "INCONVERSATION") {
        return ActionResult::Continue;
    }
    if let Some(a) = w.actors.get_mut(id) {
        a.threshold = 0;
    }
    let heard = w.actors.get(id).and_then(|a| a.last_heard);
    let Some(t) = heard.filter(|&t| health(w, t) > 0 && has_flag(w, t, "SHOOTABLE")) else {
        return ActionResult::Continue;
    };
    if has_flag(w, t, "FRIENDLY") == has_flag(w, id, "FRIENDLY") && !is_player(w, t) {
        return ActionResult::Continue;
    }
    if let Some(a) = w.actors.get_mut(id) {
        a.target = Some(t);
    }
    if has_flag(w, id, "AMBUSH") && !check_sight(w, id, t) {
        return ActionResult::Continue;
    }
    let see = class_sound(w, id, "see");
    sound(w, id, &see);
    if let Some(a) = w.actors.get_mut(id) {
        a.last_heard = None;
        a.threshold = 10;
    }
    let _ = set_label(w, id, "See");
    ActionResult::Continue
}

fn spectral_missile(w: &mut World, id: ActorId, cls: &str) {
    let Some(t) = target(w, id) else {
        return;
    };
    if let Some(m) = spawn_missile(w, id, t, cls) {
        if let Some(x) = w.actors.get_mut(m) {
            x.tracer = Some(t);
        }
    }
}

fn spot_lightning(w: &mut World, id: ActorId) -> ActionResult {
    let Some(t) = target(w, id) else {
        return ActionResult::Continue;
    };
    let tp = pos(w, t);
    let (tf, _) = floor_ceil(w, t);
    if let Some(s) = spawn(w, "SpectralLightningSpot", DVec3::new(tp.x(), tp.y(), tf)) {
        if let Some(x) = w.actors.get_mut(s) {
            x.threshold = 25;
            x.target = Some(id);
            x.tracer = Some(t);
        }
    }
    ActionResult::Continue
}

fn spectre3_attack(w: &mut World, id: ActorId) -> ActionResult {
    let Some(t) = target(w, id) else {
        return ActionResult::Continue;
    };
    let p = pos(w, id);
    if let Some(f) = spawn(
        w,
        "SpectralLightningV2",
        DVec3::new(p.x(), p.y(), p.z() + 32.0),
    ) {
        if let Some(x) = w.actors.get_mut(f) {
            x.vel = DVec3::new(0.0, 0.0, -12.0);
            x.target = Some(id);
            x.tracer = Some(t);
        }
    }
    turn(w, id, -90.0);
    for _ in 0..20 {
        turn(w, id, 9.0);
        let _ = spawn_sub_missile(w, id, "SpectralLightningBall2", Some(id));
    }
    turn(w, id, -90.0);
    ActionResult::Continue
}

fn beacon(w: &mut World, id: ActorId) -> ActionResult {
    let owner = target(w, id);
    let p = pos(w, id);
    let (floor, _) = floor_ceil(w, id);
    let Some(rebel) = spawn(w, "Rebel1", DVec3::new(p.x(), p.y(), floor)) else {
        return ActionResult::Continue;
    };
    let rp = pos(w, rebel);
    if !crate::physics::try_move(w, rebel, rp) {
        destroy(w, rebel);
        return ActionResult::Continue;
    }
    set_flag(w, id, "SPECIAL", false);
    let def_threshold = w
        .actors
        .get(rebel)
        .and_then(|a| w.classes.get(a.class))
        .map_or(100, |c| c.def_threshold);
    let dm = w.deathmatch;
    if let Some(r) = w.actors.get_mut(rebel) {
        r.threshold = def_threshold;
        r.target = None;
        r.last_heard = owner;
        if dm {
            r.health *= 2;
        }
    }
    set_flag(w, rebel, "INCOMBAT", true);
    if let Some(o) = owner {
        set_flag(w, rebel, "FRIENDLY", true);
        let ot = target(w, o).filter(|&t| !has_flag(w, t, "FRIENDLY"));
        if let Some(r) = w.actors.get_mut(rebel) {
            r.target = ot;
        }
    }
    let _ = set_label(w, rebel, "See");
    let a = angle(w, id);
    set_angle(w, rebel, a);
    let (dx, dy) = angle_vec(a, 20.0);
    crate::specials::spawn_teleport_fog(w, DVec3::new(rp.x() + dx, rp.y() + dy, rp.z()));
    let hp = w.actors.get_mut(id).map_or(0, |x| {
        x.health -= 1;
        x.health
    });
    if hp < 0 {
        let _ = set_label(w, id, "Death");
    }
    ActionResult::Continue
}

fn burnination(w: &mut World, id: ActorId) -> ActionResult {
    let dx = f64::from(rnd2_mask(w, "pr_phburn", 3));
    let dy = f64::from(rnd2_mask(w, "pr_phburn", 3));
    if let Some(a) = w.actors.get_mut(id) {
        a.vel = DVec3::new(a.vel.x() + dx, a.vel.y() + dy, a.vel.z() - 8.0);
    }
    sound(w, id, "world/largefire");
    if has_flag(w, id, "DROPPED") {
        return ActionResult::Continue;
    }
    let xr = rnd(w, "pr_phburn");
    let yr = rnd(w, "pr_phburn");
    let xo = f64::from(if xr & 128 != 0 { -(xr & 31) } else { xr & 31 });
    let yo = f64::from(if yr & 128 != 0 { -(yr & 31) } else { yr & 31 });
    let p = pos(w, id);
    let mut at = DVec3::new(p.x() + xo, p.y() + yo, p.z() + 4.0);
    let step = w.actors.get(id).map_or(24.0, |a| a.max_step_height);
    if let Some(a) = w.actors.get(id) {
        let probe = crate::physics::check_position(w, a, at, Some(id));
        if probe.floor_z > p.z() + step {
            at = DVec3::new(p.x(), p.y(), p.z() + 4.0);
        }
    }
    let v = vel(w, id);
    if let Some(d) = spawn(w, "PhosphorousFire", at) {
        let vx = v.x() + f64::from(rnd2_mask(w, "pr_phburn", 7));
        let vy = v.y() + f64::from(rnd2_mask(w, "pr_phburn", 7));
        set_vel(w, d, DVec3::new(vx, vy, v.z() - 1.0));
        let rt = rnd_range(w, "pr_phburn", 2, 5);
        if let Some(x) = w.actors.get_mut(d) {
            x.reaction_time = rt;
        }
        set_dropped(w, d);
    }
    ActionResult::Continue
}

fn alien_spectre_death(w: &mut World, id: ActorId) -> ActionResult {
    let _ = crate::action::a_no_blocking(w, id);
    if !check_boss_death(w, id) {
        return ActionResult::Continue;
    }
    let Some(p) = live_player(w) else {
        return ActionResult::Continue;
    };
    let cls = class_name(w, id).to_ascii_lowercase();
    let mut log = 0;
    match cls.as_str() {
        "alienspectre1" => {
            let _ = call(w, id, "Floor_LowerToLowest", &["999", "8"]);
            log = 95;
        }
        "alienspectre2" => {
            mid_print(w, "$TXT_KILLED_BISHOP");
            log = 74;
            let _ = give(w, p, "QuestItem21", 1);
        }
        "alienspectre3" => {
            mid_print(w, "$TXT_KILLED_ORACLE");
            for o in actors_of(w, "Oracle") {
                if health(w, o) > 0 {
                    let _ = damage(w, o, Some(id), Some(id), 1_000_000);
                }
            }
            let _ = give(w, p, "QuestItem23", 1);
            if count_inv(w, p, "QuestItem21") > 0 {
                let _ = give(w, p, "QuestItem22", 1);
            }
            log = if count_inv(w, p, "QuestItem24") == 0 {
                87
            } else {
                85
            };
            let _ = call(w, id, "Door_Open", &["222", "64"]);
        }
        "alienspectre4" => {
            mid_print(w, "$TXT_KILLED_MACIL");
            let _ = give(w, p, "QuestItem24", 1);
            log = if count_inv(w, p, "QuestItem25") == 0 {
                79
            } else {
                106
            };
        }
        "alienspectre5" => {
            mid_print(w, "$TXT_KILLED_LOREMASTER");
            let _ = give(w, p, "QuestItem26", 1);
            if w.players.iter().flatten().count() <= 1 {
                let _ = give(w, p, "UpgradeStamina", 1);
                let _ = give(w, p, "UpgradeAccuracy", 1);
            }
            log = if count_inv(w, p, "Sigil") == 5 {
                85
            } else {
                83
            };
            let _ = call(w, id, "Floor_LowerToLowest", &["666", "8"]);
        }
        _ => {}
    }
    if log > 0 {
        sound(w, id, &format!("svox/voc{log}"));
    }
    ActionResult::Continue
}
