//! Hexen actions (`actors/hexen`).

use super::heretic::{class_speed, player_fire, radial_missiles, spawn_health};
use super::*;

pub(super) const NAMES: &[&str] = &[
    "A_AccelBalls",
    "A_BatMove",
    "A_BatSpawn",
    "A_BatSpawnInit",
    "A_BellReset1",
    "A_BellReset2",
    "A_BishopAttack",
    "A_BishopAttack2",
    "A_BishopChase",
    "A_BishopDecide",
    "A_BishopDoBlur",
    "A_BishopPainBlur",
    "A_BishopSpawnBlur",
    "A_BounceCheck",
    "A_CFlameAttack",
    "A_CFlameMissile",
    "A_CFlamePuff",
    "A_CFlameRotate",
    "A_CHolyAttack",
    "A_CHolyAttack2",
    "A_CHolyCheckScream",
    "A_CHolyPalette",
    "A_CHolySeek",
    "A_CHolyTail",
    "A_CMaceAttack",
    "A_CStaffAttack",
    "A_CStaffCheck",
    "A_CStaffCheckBlink",
    "A_CStaffInitBlink",
    "A_CheckTeleRing",
    "A_CheckThrowBomb",
    "A_CheckThrowBomb2",
    "A_ClericAttack",
    "A_CorpseBloodDrip",
    "A_CorpseExplode",
    "A_DecelBalls",
    "A_DelayGib",
    "A_DragonAttack",
    "A_DragonCheckCrash",
    "A_DragonFX2",
    "A_DragonFlap",
    "A_DragonFlight",
    "A_DragonInitFlight",
    "A_DragonPain",
    "A_FAxeAttack",
    "A_FAxeCheckAtk",
    "A_FAxeCheckReady",
    "A_FAxeCheckReadyG",
    "A_FAxeCheckUp",
    "A_FAxeCheckUpG",
    "A_FHammerAttack",
    "A_FHammerThrow",
    "A_FPunchAttack",
    "A_FSwordAttack",
    "A_FSwordFlames",
    "A_FighterAttack",
    "A_FireConePL1",
    "A_FiredAttack",
    "A_FiredChase",
    "A_FiredRocks",
    "A_FiredSpawnRock",
    "A_FiredSplotch",
    "A_FloatGib",
    "A_FlyBuzz",
    "A_FlySearch",
    "A_FogMove",
    "A_FogSpawn",
    "A_IceGuyAttack",
    "A_IceGuyChase",
    "A_IceGuyLook",
    "A_IceGuyMissileExplode",
    "A_KBolt",
    "A_KBoltRaise",
    "A_KSpiritRoam",
    "A_KoraxBonePop",
    "A_KoraxChase",
    "A_KoraxCommand",
    "A_KoraxDecide",
    "A_KoraxMissile",
    "A_KoraxStep",
    "A_LastZap",
    "A_LeafCheck",
    "A_LeafSpawn",
    "A_LeafThrust",
    "A_LightningClip",
    "A_LightningReady",
    "A_LightningRemove",
    "A_LightningZap",
    "A_MLightningAttack",
    "A_MStaffAttack",
    "A_MStaffPalette",
    "A_MStaffTrack",
    "A_MageAttack",
    "A_PoisonBagCheck",
    "A_PoisonBagDamage",
    "A_PoisonBagInit",
    "A_PoisonShroom",
    "A_PotteryCheck",
    "A_PotteryChooseBit",
    "A_PotteryExplode",
    "A_SerpentCheckForAttack",
    "A_SerpentChooseAttack",
    "A_SerpentHeadCheck",
    "A_SerpentHide",
    "A_SerpentHumpDecide",
    "A_SerpentLowerHump",
    "A_SerpentMeleeAttack",
    "A_SerpentRaiseHump",
    "A_SerpentSpawnGibs",
    "A_SerpentUnHide",
    "A_ShedShard",
    "A_SinkGib",
    "A_SlowBalls",
    "A_SmBounce",
    "A_SnoutAttack",
    "A_SoAExplode",
    "A_SorcBallExplode",
    "A_SorcBallOrbit",
    "A_SorcBallPop",
    "A_SorcBossAttack",
    "A_SorcFX1Seek",
    "A_SorcFX2Orbit",
    "A_SorcFX2Split",
    "A_SorcFX4Check",
    "A_SorcOffense2",
    "A_SorcSpinBalls",
    "A_SorcererBishopEntry",
    "A_SpawnBishop",
    "A_SpawnFizzle",
    "A_SpeedBalls",
    "A_StopBalls",
    "A_Summon",
    "A_TeloSpawnA",
    "A_TeloSpawnB",
    "A_TeloSpawnC",
    "A_TeloSpawnD",
    "A_ThrustImpale",
    "A_ThrustInitDn",
    "A_ThrustInitUp",
    "A_ThrustLower",
    "A_ThrustRaise",
    "A_WraithChase",
    "A_WraithFX2",
    "A_WraithFX3",
    "A_WraithFX4",
    "A_WraithInit",
    "A_WraithMelee",
    "A_WraithRaise",
    "A_WraithRaiseInit",
    "A_ZapMimic",
];

// Heresiarch ball modes (`Heresiarch.ESorc`).
const SORC_DECELERATE: i32 = 0;
const SORC_ACCELERATE: i32 = 1;
const SORC_STOPPING: i32 = 2;
const SORC_FIRESPELL: i32 = 3;
const SORC_STOPPED: i32 = 4;
const SORC_NORMAL: i32 = 5;
const SORC_FIRING_SPELL: i32 = 6;
const SORCBALL_INITIAL_SPEED: i32 = 7;
const SORCBALL_TERMINAL_SPEED: i32 = 25;
const SORCBALL_SPEED_ROTATIONS: i32 = 5;
const SORC_DEFENSE_TIME: i32 = 255;
const SORC_DEFENSE_HEIGHT: f64 = 45.0;
const BOUNCE_TIME_UNIT: i32 = 35 / 2;
const SORCFX4_RAPIDFIRE_TIME: i32 = 6 * 3;
const SORCFX4_SPREAD_ANGLE: f64 = 20.0;
// Frost shard directions (`FrostMissile.SHARDSPAWN_*`).
const SHARDSPAWN_LEFT: i32 = 1;
const SHARDSPAWN_RIGHT: i32 = 2;
const SHARDSPAWN_UP: i32 = 4;
const SHARDSPAWN_DOWN: i32 = 8;
const FIREDEMON_ATTACK_RANGE: f64 = 64.0 * 8.0;
const ZAGSPEED: f64 = 1.0;
const AXERANGE: f64 = 64.0 * 2.25;
const HAMMER_RANGE: f64 = 64.0 * 1.25;
const KORAX_SPIRIT_LIFETIME: i32 = 5 * 35 / 5;
const KORAX_BOLT_HEIGHT: f64 = 48.0;
const KORAX_BOLT_LIFETIME: i32 = 3;
const TELEPORT_LIFE: i32 = 1;

/// `BobSin(index)`: Hexen's float-bob table, `8 × sin(index × 360/64)`.
fn bob_sin(i: i32) -> f64 {
    8.0 * (f64::from(i & 63) * (360.0 / 64.0)).to_radians().sin()
}

fn args_of(w: &World, id: ActorId) -> [i32; 5] {
    w.actors.get(id).map_or([0; 5], |a| a.args)
}

fn set_arg(w: &mut World, id: ActorId, i: usize, v: i32) {
    if let Some(a) = w.actors.get_mut(id) {
        a.args[i] = v;
    }
}

fn s1(w: &World, id: ActorId) -> i32 {
    w.actors.get(id).map_or(0, |a| a.ext.special1)
}

fn s2(w: &World, id: ActorId) -> i32 {
    w.actors.get(id).map_or(0, |a| a.ext.special2)
}

fn set_s1(w: &mut World, id: ActorId, v: i32) {
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.special1 = v;
    }
}

fn set_s2(w: &mut World, id: ActorId, v: i32) {
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.special2 = v;
    }
}

fn add_z(w: &mut World, id: ActorId, dz: f64) {
    if let Some(a) = w.actors.get_mut(id) {
        a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + dz);
    }
}

fn set_z(w: &mut World, id: ActorId, z: f64) {
    if let Some(a) = w.actors.get_mut(id) {
        a.pos = DVec3::new(a.pos.x(), a.pos.y(), z);
    }
}

fn floorclip(w: &World, id: ActorId) -> f64 {
    w.actors.get(id).map_or(0.0, |a| a.ext.floorclip)
}

fn add_floorclip(w: &mut World, id: ActorId, d: f64) {
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.floorclip += d;
    }
}

fn set_tics(w: &mut World, id: ActorId, t: i32) {
    if let Some(a) = w.actors.get_mut(id) {
        a.tics = t;
    }
}

fn set_target(w: &mut World, id: ActorId, t: Option<ActorId>) {
    if let Some(a) = w.actors.get_mut(id) {
        a.target = t;
    }
}

fn set_tracer(w: &mut World, id: ActorId, t: Option<ActorId>) {
    if let Some(a) = w.actors.get_mut(id) {
        a.tracer = t;
    }
}

fn height(w: &World, id: ActorId) -> f64 {
    w.actors.get(id).map_or(0.0, |a| a.height)
}

fn radius(w: &World, id: ActorId) -> f64 {
    w.actors.get(id).map_or(0.0, |a| a.radius)
}

fn speed(w: &World, id: ActorId) -> f64 {
    w.actors.get(id).map_or(0.0, |a| a.speed)
}

/// `Vec3Angle(dist, angle, z)` from the actor.
fn vec3_angle(w: &World, id: ActorId, dist: f64, deg: f64, dz: f64) -> DVec3 {
    let p = pos(w, id);
    let (dx, dy) = angle_vec(deg, dist);
    DVec3::new(p.x() + dx, p.y() + dy, p.z() + dz)
}

/// `DistanceBySpeed(other, speed)`: tics to cover the 2D distance.
fn dist_by_speed(w: &World, a: ActorId, b: ActorId, sp: f64) -> f64 {
    (dist2d(w, a, b) / sp.max(1e-6)).max(1.0)
}

/// `AdjustPlayerAngle`: turn at most 5° toward `t`.
fn adjust_player_angle(w: &mut World, id: ActorId, t: ActorId) {
    let to = angle_to(w, id, t);
    let a = angle(w, id);
    let diff = delta_angle(a, to);
    let new = if diff.abs() > 5.0 {
        a + 5.0 * diff.signum()
    } else {
        to
    };
    set_angle(w, id, new);
}

/// `RoughMonsterSearch(blocks [, onlyseekable, frontonly])`: nearest monster
/// within `blocks × 128` units the actor can see.
fn rough_monster_search(w: &World, id: ActorId, blocks: i32, front_only: bool) -> Option<ActorId> {
    let range = f64::from(blocks) * 128.0;
    let me = angle(w, id);
    let owner = target(w, id);
    w.thinkers
        .default
        .iter()
        .copied()
        .filter(|&o| {
            o != id
                && Some(o) != owner
                && health(w, o) > 0
                && has_flag(w, o, "SHOOTABLE")
                && (has_flag(w, o, "ISMONSTER") || has_flag(w, o, "COUNTKILL"))
                && !has_flag(w, o, "FRIENDLY")
                && dist2d(w, id, o) <= range
                && (!front_only || delta_angle(me, angle_to(w, id, o)).abs() <= 90.0)
                && check_sight(w, id, o)
        })
        .min_by(|&a, &b| dist2d(w, id, a).total_cmp(&dist2d(w, id, b)))
}

/// `AimLineAttack(..., t)` returning the sighted actor.
fn aim_target(w: &World, id: ActorId, deg: f64, range: f64) -> Option<(f64, ActorId)> {
    crate::hitscan::aim_line_attack_target(w, id, Angle::from_degrees(deg), range)
}

/// `Weapon.Ammo1.Amount` for the ready weapon.
fn ammo1(w: &World, id: ActorId) -> i32 {
    let ammo = w
        .psprite_weapon
        .or_else(|| {
            w.players
                .iter()
                .flatten()
                .find(|p| p.mo == Some(id))
                .and_then(|p| p.ready_weapon)
        })
        .and_then(|c| w.classes.get(c))
        .map(|c| c.ammo_type.clone())
        .unwrap_or_default();
    if ammo.is_empty() {
        return 0;
    }
    count_inv(w, id, &ammo)
}

/// The player's `weaponspecial` counter (kept on the pawn).
fn weaponspecial(w: &World, id: ActorId) -> i32 {
    w.actors
        .get(id)
        .and_then(|a| a.ext.scratch.get("weaponspecial").copied())
        .unwrap_or(0.0) as i32
}

fn set_weaponspecial(w: &mut World, id: ActorId, v: i32) {
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.scratch.insert("weaponspecial".into(), f64::from(v));
    }
}

/// Turn toward `aim` by up to `turn_max` (half the gap past `thresh`), keeping speed.
fn seek_turn(w: &mut World, id: ActorId, aim: ActorId, thresh: f64, turn_max: f64) {
    let dir = delta_angle(angle(w, id), angle_to(w, id, aim));
    let mut delta = dir.abs();
    if delta > thresh {
        delta = (delta / 2.0).min(turn_max);
    }
    let a = angle(w, id) + if dir > 0.0 { delta } else { -delta };
    set_angle(w, id, a);
    let sp = speed(w, id);
    let vz = vel(w, id).z();
    let (vx, vy) = angle_vec(a, sp);
    set_vel(w, id, DVec3::new(vx, vy, vz));
}

/// Spirit / holy ghost Z seek (every 16 tics or when out of the target's span).
fn seek_z(w: &mut World, id: ActorId, aim: ActorId, rng: &str) {
    let (ap, ah) = (pos(w, aim), height(w, aim));
    let (sp, sh) = (pos(w, id), height(w, id));
    if w.level.maptime & 15 == 0 || sp.z() > ap.z() + ah || sp.z() + sh < ap.z() {
        let new_z = ap.z() + f64::from(rnd(w, rng)) * ah / 256.0;
        let dz = (new_z - sp.z()).clamp(-15.0, 15.0);
        let s = speed(w, id);
        let d = dist_by_speed(w, aim, id, s);
        set_vel_z(w, id, dz / d);
    }
}

pub(super) fn dispatch(
    w: &mut World,
    id: ActorId,
    name: &str,
    args: &[String],
) -> Option<ActionResult> {
    let r = match name {
        "A_AccelBalls" => {
            if let Some(sorc) = target(w, id) {
                let a = args_of(w, sorc);
                if a[4] < a[2] {
                    set_arg(w, sorc, 4, a[4] + 1);
                } else {
                    set_arg(w, sorc, 3, SORC_NORMAL);
                    if a[4] >= SORCBALL_TERMINAL_SPEED {
                        stop_balls(w, sorc);
                    }
                }
            }
            ActionResult::Continue
        }
        "A_DecelBalls" => {
            if let Some(sorc) = target(w, id) {
                let a = args_of(w, sorc);
                if a[4] > a[2] {
                    set_arg(w, sorc, 4, a[4] - 1);
                } else {
                    set_arg(w, sorc, 3, SORC_NORMAL);
                }
            }
            ActionResult::Continue
        }
        "A_SlowBalls" => {
            set_arg(w, id, 3, SORC_DECELERATE);
            set_arg(w, id, 2, SORCBALL_INITIAL_SPEED);
            ActionResult::Continue
        }
        "A_SpeedBalls" => {
            set_arg(w, id, 3, SORC_ACCELERATE);
            set_arg(w, id, 2, SORCBALL_TERMINAL_SPEED);
            ActionResult::Continue
        }
        "A_SorcBossAttack" => {
            set_arg(w, id, 3, SORC_ACCELERATE);
            set_arg(w, id, 2, SORCBALL_INITIAL_SPEED);
            ActionResult::Continue
        }
        "A_StopBalls" => {
            stop_balls(w, id);
            ActionResult::Continue
        }
        "A_SorcSpinBalls" => {
            set_arg(w, id, 3, SORC_DECELERATE);
            set_arg(w, id, 2, SORCBALL_INITIAL_SPEED);
            set_arg(w, id, 0, 0);
            set_arg(w, id, 3, SORC_NORMAL);
            set_arg(w, id, 4, SORCBALL_INITIAL_SPEED);
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.specialf.0 = 1.0; // BallAngle
            }
            let p = pos(w, id);
            for (i, c) in ["SorcBall1", "SorcBall2", "SorcBall3"].iter().enumerate() {
                if let Some(mo) = spawn(w, c, p) {
                    set_target(w, mo, Some(id));
                    if let Some(a) = w.actors.get_mut(mo) {
                        a.ext.specialf.1 = 120.0 * i as f64; // AngleOffset
                    }
                    if i == 0 {
                        set_s2(w, mo, SORCFX4_RAPIDFIRE_TIME);
                    }
                }
            }
            ActionResult::Continue
        }
        "A_SorcBallOrbit" => sorc_ball_orbit(w, id),
        "A_SorcBallPop" => {
            sound(w, id, "SorcererBallPop");
            set_flag(w, id, "NOGRAVITY", false);
            let vx = f64::from(rnd_range(w, "pr_heresiarch", -5, 4));
            let vy = f64::from(rnd_range(w, "pr_heresiarch", -5, 4));
            let vz = f64::from(rnd_range(w, "pr_heresiarch", 2, 4));
            if let Some(a) = w.actors.get_mut(id) {
                a.gravity = 1.0 / 8.0;
                a.vel = DVec3::new(vx, vy, vz);
                a.args[4] = BOUNCE_TIME_UNIT;
                a.args[3] = 5;
            }
            ActionResult::Continue
        }
        "A_BounceCheck" | "A_SorcFX1Seek" => {
            let a = args_of(w, id);
            set_arg(w, id, 4, a[4] - 1);
            if a[4] <= 0 {
                set_arg(w, id, 3, a[3] - 1);
                if a[3] <= 0 {
                    let _ = set_label(w, id, "Death");
                    sound(
                        w,
                        id,
                        if name == "A_BounceCheck" {
                            "SorcererBigBallExplode"
                        } else {
                            "SorcererHeadScream"
                        },
                    );
                } else {
                    set_arg(w, id, 4, BOUNCE_TIME_UNIT);
                }
            }
            if name == "A_SorcFX1Seek" {
                return Some(call(w, id, "A_SeekerMissile", &["2", "6"]));
            }
            ActionResult::Continue
        }
        "A_SorcBallExplode" => {
            set_flag(w, id, "NOBOUNCESOUND", true);
            explode(w, id, 255, 255.0);
            ActionResult::Continue
        }
        "A_SorcFX2Orbit" => sorc_fx2_orbit(w, id),
        "A_SorcFX2Split" => {
            let cls = class_name(w, id);
            let p = pos(w, id);
            let (t, a) = (target(w, id), angle(w, id));
            for ccw in [0, 1] {
                if let Some(mo) = spawn(w, &cls, p) {
                    set_target(w, mo, t);
                    set_arg(w, mo, 0, ccw);
                    if let Some(x) = w.actors.get_mut(mo) {
                        x.ext.specialf.0 = a;
                    }
                    let _ = set_label(w, mo, "Orbit");
                }
            }
            destroy(w, id);
            ActionResult::Continue
        }
        "A_SorcFX4Check" => {
            let v = s2(w, id);
            set_s2(w, id, v - 1);
            if v <= 0 {
                let _ = set_label(w, id, "Death");
            }
            ActionResult::Continue
        }
        "A_SorcOffense2" => {
            sorc_offense2(w, id);
            ActionResult::Continue
        }
        "A_SorcererBishopEntry" => {
            let p = pos(w, id);
            let _ = spawn(w, "SorcFX3Explosion", p);
            let s = class_sound(w, id, "see");
            sound(w, id, &s);
            ActionResult::Continue
        }
        "A_SpawnBishop" => {
            let p = pos(w, id);
            if let Some(mo) = spawn(w, "Bishop", p) {
                let fits = w
                    .actors
                    .get(mo)
                    .is_some_and(|a| crate::physics::check_position(w, a, a.pos, Some(mo)).ok);
                if !fits {
                    if let Some(a) = w.actors.get_mut(mo) {
                        a.flags &= !crate::class::FLAG_COUNTKILL;
                    }
                    destroy(w, mo);
                } else if let Some(t) = target(w, id) {
                    let friendly = has_flag(w, t, "FRIENDLY");
                    set_flag(w, mo, "FRIENDLY", friendly);
                    if let Some(a) = w.actors.get_mut(mo) {
                        a.master = Some(t);
                    }
                }
            }
            destroy(w, id);
            ActionResult::Continue
        }
        "A_SpawnFizzle" => {
            let a = angle(w, id);
            let at = vec3_angle(w, id, 5.0, a, -floorclip(w, id) + height(w, id) / 2.0);
            let sp = speed(w, id) as i32;
            for _ in 0..5 {
                if let Some(mo) = spawn(w, "SorcSpark1", at) {
                    let r = a + f64::from(rnd_range(w, "pr_heresiarch", 0, 4)) * (4096.0 / 360.0);
                    let m = f64::from(rnd_range(w, "pr_heresiarch", 0, (sp - 1).max(0)));
                    let (vx, vy) = angle_vec(r, m);
                    set_vel(w, mo, DVec3::new(vx, vy, 2.0));
                }
            }
            ActionResult::Continue
        }
        "A_BatMove" => {
            if s2(w, id) < 0 {
                let _ = set_label(w, id, "Death");
            }
            let v = s2(w, id) - 2;
            set_s2(w, id, v);
            let turn = f64::from(args_of(w, id)[4]);
            let a = angle(w, id);
            let new = if rnd(w, "pr_batmove") < 128 {
                a + turn
            } else {
                a - turn
            };
            set_angle(w, id, new);
            let sp = speed(w, id);
            let (vx, vy) = angle_vec(new, sp);
            let vz = vel(w, id).z();
            set_vel(w, id, DVec3::new(vx, vy, vz));
            if rnd(w, "pr_batmove") < 15 {
                sound(w, id, "BatScream");
            }
            if let Some(t) = target(w, id) {
                let z = pos(w, t).z() + 2.0 * bob_sin(args_of(w, id)[0]);
                set_z(w, id, z);
            }
            let a0 = args_of(w, id)[0];
            set_arg(w, id, 0, (a0 + 3) & 63);
            ActionResult::Continue
        }
        "A_BatSpawn" => {
            let c = s1(w, id);
            set_s1(w, id, c - 1);
            if c > 0 {
                return Some(ActionResult::Continue);
            }
            let a = args_of(w, id);
            set_s1(w, id, a[0]);
            let delta = if a[1] == 0 { 1 } else { a[1] };
            let ang = angle(w, id)
                + f64::from(rnd_range(w, "pr_batspawn", 0, delta - 1) - (delta >> 1))
                    * (360.0 / 256.0);
            if let Some(mo) = spawn_missile_angle(w, id, "Bat", ang, 0.0) {
                let bob = rnd_range(w, "pr_batspawn", 0, 63);
                set_arg(w, mo, 0, bob);
                set_arg(w, mo, 4, a[4]);
                set_s2(w, mo, a[3] << 3);
                set_target(w, mo, Some(id));
            }
            ActionResult::Continue
        }
        "A_BatSpawnInit" => {
            set_s1(w, id, 0);
            ActionResult::Continue
        }
        "A_BellReset1" => {
            set_flag(w, id, "NOGRAVITY", true);
            let h = w
                .actors
                .get(id)
                .and_then(|a| w.classes.get(a.class))
                .map_or(0.0, |c| c.height);
            if let Some(a) = w.actors.get_mut(id) {
                a.height = h;
            }
            ActionResult::Continue
        }
        "A_BellReset2" => {
            set_flag(w, id, "SHOOTABLE", true);
            set_flag(w, id, "CORPSE", false);
            if let Some(a) = w.actors.get_mut(id) {
                a.health = 5;
            }
            ActionResult::Continue
        }
        "A_BishopAttack" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            let s = class_sound(w, id, "attack");
            sound(w, id, &s);
            if in_melee_range(w, id) {
                let d = rnd_range(w, "pr_bishopattack", 1, 8) * 4;
                let _ = damage(w, t, Some(id), Some(id), d);
                return Some(ActionResult::Continue);
            }
            let n = rnd_range(w, "pr_bishopattack", 5, 8);
            set_missilecount(w, id, n);
            ActionResult::Continue
        }
        "A_BishopAttack2" => {
            let n = missilecount(w, id);
            let Some(t) = target(w, id).filter(|_| n != 0) else {
                set_missilecount(w, id, 0);
                let _ = set_label(w, id, "See");
                return Some(ActionResult::Continue);
            };
            if let Some(mo) = spawn_missile(w, id, t, "BishopFX") {
                set_tracer(w, mo, Some(t));
            }
            set_missilecount(w, id, n - 1);
            ActionResult::Continue
        }
        "A_BishopChase" => {
            let b = bobstate(w, id);
            let mut z = pos(w, id).z() - bob_sin(b) / 2.0;
            let nb = (b + 4) & 63;
            z += bob_sin(nb) / 2.0;
            set_bobstate(w, id, nb);
            set_z(w, id, z);
            ActionResult::Continue
        }
        "A_BishopDecide" => {
            if rnd(w, "pr_bishopdecide") >= 220 {
                let _ = set_label(w, id, "Blur");
            }
            ActionResult::Continue
        }
        "A_BishopDoBlur" => {
            let n = rnd_range(w, "pr_bishopdoblur", 3, 6);
            set_missilecount(w, id, n);
            let a = angle(w, id);
            if rnd(w, "pr_bishopdoblur") < 120 {
                thrust(w, id, a + 90.0, 11.0);
            } else if rnd(w, "pr_bishopdoblur") > 125 {
                thrust(w, id, a - 90.0, 11.0);
            } else {
                thrust(w, id, a, 11.0);
            }
            sound(w, id, "BishopBlur");
            ActionResult::Continue
        }
        "A_BishopPainBlur" => {
            if rnd(w, "pr_bishoppainblur") < 64 {
                let _ = set_label(w, id, "Blur");
                return Some(ActionResult::Continue);
            }
            let xo = f64::from(rnd2(w, "pr_bishoppainblur")) / 16.0;
            let yo = f64::from(rnd2(w, "pr_bishoppainblur")) / 16.0;
            let zo = f64::from(rnd2(w, "pr_bishoppainblur")) / 32.0;
            let p = pos(w, id);
            let a = angle(w, id);
            let _ = spawn_facing(
                w,
                "BishopPainBlur",
                DVec3::new(p.x() + xo, p.y() + yo, p.z() + zo),
                a,
            );
            ActionResult::Continue
        }
        "A_BishopSpawnBlur" => {
            let n = missilecount(w, id) - 1;
            set_missilecount(w, id, n);
            if n == 0 {
                let v = vel(w, id);
                set_vel(w, id, DVec3::new(0.0, 0.0, v.z()));
                if rnd(w, "pr_bishopspawnblur") > 96 {
                    let _ = set_label(w, id, "See");
                } else {
                    let _ = set_label(w, id, "Missile");
                }
            }
            let (p, a) = (pos(w, id), angle(w, id));
            let _ = spawn_facing(w, "BishopBlur", p, a);
            ActionResult::Continue
        }
        "A_CFlameAttack" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let _ = spawn_player_missile(w, id, "CFlameMissile", 0.0);
            sound(w, id, "ClericFlameFire");
            ActionResult::Continue
        }
        "A_CFlameMissile" => {
            set_flag(w, id, "INVISIBLE", false);
            sound(w, id, "ClericFlameExplode");
            let hit = w
                .actors
                .get(id)
                .and_then(|a| a.ext.scratch.get("blocking").copied())
                .and_then(|_| None::<ActorId>)
                .or_else(|| last_hit_actor(w, id))
                .filter(|&b| has_flag(w, b, "SHOOTABLE"));
            if let Some(b) = hit {
                let dist = radius(w, b) + 18.0;
                let t = target(w, id);
                for i in 0..4 {
                    for (an, sign) in [
                        (f64::from(i) * 45.0, 1.0),
                        (f64::from(i) * 45.0 + 180.0, -1.0),
                    ] {
                        let at = vec3_angle(w, b, dist, an, 5.0);
                        if let Some(mo) = spawn_facing(w, "CircleFlame", at, an) {
                            set_target(w, mo, t);
                            let (vx, vy) = angle_vec(an, 10.0 * sign);
                            set_vel(w, mo, DVec3::new(vx, vy, 0.0));
                            let dt = rnd_range(w, "pr_flamemissile", 0, 3);
                            if let Some(x) = w.actors.get_mut(mo) {
                                x.ext.specialf = (vx, vy);
                                x.tics -= dt;
                            }
                        }
                    }
                }
                let _ = set_label(w, id, "Spawn");
            }
            ActionResult::Continue
        }
        "A_CFlamePuff" => {
            set_flag(w, id, "INVISIBLE", false);
            set_flag(w, id, "MISSILE", false);
            set_vel(w, id, DVec3::ZERO);
            sound(w, id, "ClericFlameExplode");
            ActionResult::Continue
        }
        "A_CFlameRotate" => {
            let a = angle(w, id);
            let (vx, vy) = angle_vec(a + 90.0, 5.0);
            let (fx, fy) = w.actors.get(id).map_or((0.0, 0.0), |x| x.ext.specialf);
            let vz = vel(w, id).z();
            set_vel(w, id, DVec3::new(vx + fx, vy + fy, vz));
            set_angle(w, id, a + 6.0);
            ActionResult::Continue
        }
        "A_CHolyAttack" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let a = angle(w, id);
            let t = aim_target(w, id, a, 1024.0).map(|(_, t)| t);
            if let Some(m) = spawn_player_missile(w, id, "HolyMissile", 0.0) {
                set_tracer(w, m, t);
            }
            sound(w, id, "HolySymbolFire");
            ActionResult::Continue
        }
        "A_CHolyAttack2" => {
            let p = pos(w, id);
            let (a, t, tr) = (angle(w, id), target(w, id), tracer(w, id));
            for j in 0..4 {
                let Some(mo) = spawn(w, "HolySpirit", p) else {
                    continue;
                };
                let (xy, z) = match j {
                    0 => (-1, rnd_range(w, "pr_holyatk2", 0, 7)),
                    1 => (-1, rnd_range(w, "pr_holyatk2", 32, 39)),
                    2 => (rnd_range(w, "pr_holyatk2", 32, 39), -1),
                    _ => (
                        rnd_range(w, "pr_holyatk2", 32, 39),
                        rnd_range(w, "pr_holyatk2", 32, 39),
                    ),
                };
                let an = a + 67.5 - 45.0 * f64::from(j);
                set_angle(w, mo, an);
                let sp = speed(w, mo);
                thrust(w, mo, an, sp);
                let dm = w.deathmatch;
                if let Some(x) = w.actors.get_mut(mo) {
                    if xy >= 0 {
                        x.float_bob_phase = (xy & 63) as u8;
                    }
                    if z >= 0 {
                        x.ext.weave_z = z;
                    }
                    x.pos = DVec3::new(x.pos.x(), x.pos.y(), p.z());
                    x.target = t;
                    x.args[0] = 10;
                    x.args[1] = 0;
                    if dm {
                        x.health = 85;
                    }
                }
                if let Some(trc) = tr {
                    set_tracer(w, mo, Some(trc));
                    set_flag(w, mo, "NOCLIP", true);
                    set_flag(w, mo, "SKULLFLY", true);
                    set_flag(w, mo, "MISSILE", false);
                }
                spawn_spirit_tail(w, mo);
            }
            ActionResult::Continue
        }
        "A_CHolyCheckScream" => {
            let _ = holy_seek(w, id);
            if rnd(w, "pr_holyscream") < 20 {
                sound(w, id, "SpiritActive");
            }
            if tracer(w, id).is_none() {
                holy_find_target(w, id);
            }
            ActionResult::Continue
        }
        "A_CHolyPalette" | "A_MStaffPalette" => ActionResult::Continue,
        "A_CHolySeek" => holy_seek(w, id),
        "A_CHolyTail" => {
            let parent = target(w, id).filter(|&p| health(w, p) > 0);
            let Some(parent) = parent else {
                let mut mo = Some(id);
                while let Some(m) = mo {
                    let next = w.actors.get(m).and_then(|a| a.tracer);
                    destroy(w, m);
                    mo = next;
                }
                return Some(ActionResult::Continue);
            };
            let pa = angle(w, parent);
            let at = vec3_angle(w, parent, -14.0, pa, 0.0);
            if crate::physics::try_move(w, id, DVec3::new(at.x(), at.y(), pos(w, id).z())) {
                let z = pos(w, parent).z() - 5.0;
                set_z(w, id, z);
            }
            holy_tail_follow(w, id, 10.0);
            ActionResult::Continue
        }
        "A_CMaceAttack" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            let dmg = rnd_range(w, "pr_maceatk", 25, 40);
            melee_sweep(
                w,
                id,
                16,
                45.0 / 16.0,
                2.0 * MELEE_RANGE,
                dmg,
                "HammerPuff",
                |w, id, t| {
                    adjust_player_angle(w, id, t);
                },
            );
            ActionResult::Continue
        }
        "A_CStaffAttack" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            for (off, idx) in [(-3.0, 32), (3.0, 0)] {
                if let Some(mo) = spawn_player_missile(w, id, "CStaffMissile", off) {
                    if let Some(x) = w.actors.get_mut(mo) {
                        x.float_bob_phase = idx;
                    }
                }
            }
            sound(w, id, "ClericCStaffFire");
            ActionResult::Continue
        }
        "A_CStaffCheck" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            let dmg = rnd_range(w, "pr_staffcheck", 20, 35);
            let base = angle(w, id);
            for i in 0..3 {
                for j in [1.0, -1.0] {
                    let ang = base + j * f64::from(i) * (45.0 / 16.0);
                    let Some((slope, _)) = aim_target(w, id, ang, 1.5 * MELEE_RANGE) else {
                        continue;
                    };
                    if let Some(t) =
                        line_attack(w, id, ang, 1.5 * MELEE_RANGE, slope, dmg, "CStaffPuff")
                    {
                        let to = angle_to(w, id, t);
                        set_angle(w, id, to);
                        if (is_player(w, t) || has_flag(w, t, "ISMONSTER"))
                            && !has_flag(w, t, "DORMANT")
                            && !has_flag(w, t, "INVULNERABLE")
                        {
                            if let Some(a) = w.actors.get_mut(id) {
                                a.health = (a.health + (dmg >> 3)).min(100.max(a.health));
                            }
                            let r = jump(w, id, "Drain");
                            let _ = deplete_ammo(w, id);
                            return Some(r);
                        }
                        let _ = deplete_ammo(w, id);
                    }
                    return Some(ActionResult::Continue);
                }
            }
            ActionResult::Continue
        }
        "A_CStaffCheckBlink" => {
            let v = weaponspecial(w, id) - 1;
            set_weaponspecial(w, id, v);
            if v == 0 {
                let n = (rnd(w, "pr_cstaffblink") + 50) >> 2;
                set_weaponspecial(w, id, n);
                return Some(jump(w, id, "Blink"));
            }
            call(w, id, "A_WeaponReady", &[])
        }
        "A_CStaffInitBlink" => {
            let n = (rnd(w, "pr_cstaffblink") >> 1) + 20;
            set_weaponspecial(w, id, n);
            ActionResult::Continue
        }
        "A_CheckTeleRing" => {
            let v = s1(w, id);
            set_s1(w, id, v - 1);
            if v <= 0 {
                let _ = set_label(w, id, "Death");
            }
            ActionResult::Continue
        }
        "A_CheckThrowBomb" => {
            check_throw_bomb(w, id);
            ActionResult::Continue
        }
        "A_CheckThrowBomb2" => {
            let v = vel(w, id);
            if v.z() < 2.0 && (v.x() * v.x() + v.y() * v.y() + v.z() * v.z()).sqrt() < 1.5 {
                let _ = set_label(w, id, "Tail");
                let (floor, _) = floor_ceil(w, id);
                set_z(w, id, floor);
                set_vel_z(w, id, 0.0);
                for f in [
                    "BOUNCEONFLOORS",
                    "BOUNCEONCEILINGS",
                    "BOUNCEONWALLS",
                    "MISSILE",
                ] {
                    set_flag(w, id, f, false);
                }
            }
            check_throw_bomb(w, id);
            ActionResult::Continue
        }
        "A_ClericAttack" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            let z = pos(w, id).z() + 40.0;
            if let Some(m) = spawn_missile_z(w, id, z, t, "HolyMissile") {
                set_tracer(w, m, None);
            }
            sound(w, id, "HolySymbolFire");
            ActionResult::Continue
        }
        "A_CorpseBloodDrip" => {
            if rnd(w, "pr_corpsedrip") <= 128 {
                let p = pos(w, id);
                let h = height(w, id);
                let _ = spawn(
                    w,
                    "CorpseBloodDrip",
                    DVec3::new(p.x(), p.y(), p.z() + h / 2.0),
                );
            }
            ActionResult::Continue
        }
        "A_CorpseExplode" => {
            let p = pos(w, id);
            let n = rnd_range(w, "pr_corpseexplode", 3, 6);
            for k in 0..=n {
                let Some(mo) = spawn(w, "CorpseBit", p) else {
                    continue;
                };
                let frame = if k == n {
                    3
                } else {
                    rnd_range(w, "pr_corpseexplode", 0, 2)
                };
                set_spawn_state_plus(w, mo, frame as u32);
                let vx = f64::from(rnd2(w, "pr_corpseexplode")) / 64.0;
                let vy = f64::from(rnd2(w, "pr_corpseexplode")) / 64.0;
                let vz = f64::from(rnd_range(w, "pr_corpseexplode", 5, 12)) * 0.75;
                set_vel(w, mo, DVec3::new(vx, vy, vz));
            }
            let s = class_sound(w, id, "death");
            sound(w, id, &s);
            destroy(w, id);
            ActionResult::Continue
        }
        "A_DelayGib" => {
            let d = rnd(w, "pr_delaygib") >> 2;
            if let Some(a) = w.actors.get_mut(id) {
                a.tics -= d;
            }
            ActionResult::Continue
        }
        "A_DragonAttack" => {
            if let Some(t) = target(w, id) {
                let _ = spawn_missile(w, id, t, "DragonFireball");
            }
            ActionResult::Continue
        }
        "A_DragonCheckCrash" => {
            let (floor, _) = floor_ceil(w, id);
            if pos(w, id).z() <= floor {
                let _ = set_label(w, id, "Crash");
            }
            ActionResult::Continue
        }
        "A_DragonFX2" => {
            let delay = 16 + (rnd(w, "pr_dragonfx2") >> 3);
            let n = rnd_range(w, "pr_dragonfx2", 1, 4);
            let p = pos(w, id);
            let t = target(w, id);
            for i in (1..=n).rev() {
                let xo = f64::from(rnd(w, "pr_dragonfx2") - 128) / 4.0;
                let yo = f64::from(rnd(w, "pr_dragonfx2") - 128) / 4.0;
                let zo = f64::from(rnd(w, "pr_dragonfx2") - 128) / 16.0;
                if let Some(mo) = spawn(
                    w,
                    "DragonExplosion",
                    DVec3::new(p.x() + xo, p.y() + yo, p.z() + zo),
                ) {
                    let t2 = delay + rnd_range(w, "pr_dragonfx2", 0, 3) * i * 2;
                    set_tics(w, mo, t2);
                    set_target(w, mo, t);
                }
            }
            ActionResult::Continue
        }
        "A_DragonFlap" => {
            let r = dragon_flight(w, id);
            if rnd(w, "pr_dragonflight") < 240 {
                sound(w, id, "DragonWingflap");
            } else {
                let s = class_sound(w, id, "active");
                sound(w, id, &s);
            }
            r
        }
        "A_DragonFlight" => dragon_flight(w, id),
        "A_DragonInitFlight" => {
            let tid = w.actors.get(id).map_or(0, |a| a.tid);
            let other = (tid != 0)
                .then(|| {
                    w.thinkers.default.iter().copied().find(|&o| {
                        o != id
                            && w.actors
                                .get(o)
                                .is_some_and(|a| a.tid == tid && !a.destroyed)
                    })
                })
                .flatten();
            match other {
                Some(o) => {
                    set_tracer(w, id, Some(o));
                    if let Some(a) = w.actors.get_mut(id) {
                        a.tid = 0;
                    }
                }
                None => {
                    let _ = set_label(w, id, "Spawn");
                }
            }
            ActionResult::Continue
        }
        "A_DragonPain" => {
            let r = call(w, id, "A_Pain", &[]);
            if tracer(w, id).is_none() {
                let _ = set_label(w, id, "See");
            }
            r
        }
        "A_FAxeAttack" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            let mut dmg = rnd_range(w, "pr_axeatk", 40, 55) + rnd_range(w, "pr_axeatk", 0, 7);
            let glow = ammo1(w, id) > 0;
            let (power, puff) = if glow {
                dmg <<= 1;
                (6.0, "AxePuffGlow")
            } else {
                (0.0, "AxePuff")
            };
            let base = angle(w, id);
            for i in 0..16 {
                for j in [1.0, -1.0] {
                    let ang = base + j * f64::from(i) * (45.0 / 16.0);
                    let Some((slope, _)) = aim_target(w, id, ang, AXERANGE) else {
                        continue;
                    };
                    if let Some(t) = line_attack(w, id, ang, AXERANGE, slope, dmg, puff) {
                        if has_flag(w, t, "ISMONSTER") || is_player(w, t) {
                            let at = angle_to(w, id, t);
                            thrust(w, t, at, power);
                        }
                        adjust_player_angle(w, id, t);
                        let _ = deplete_ammo(w, id);
                        if ammo1(w, id) == 0 && glow {
                            return Some(jump(w, id, "EndAttack"));
                        }
                        return Some(ActionResult::Continue);
                    }
                }
            }
            set_weaponspecial(w, id, 0);
            let slope = aim(w, id, base, MELEE_RANGE);
            let _ = line_attack(w, id, base, MELEE_RANGE, slope, dmg, puff);
            ActionResult::Continue
        }
        "A_FAxeCheckAtk" => {
            if ammo1(w, id) > 0 {
                return Some(jump(w, id, "FireGlow"));
            }
            ActionResult::Continue
        }
        "A_FAxeCheckReady" => {
            if ammo1(w, id) > 0 {
                return Some(jump(w, id, "ReadyGlow"));
            }
            call(w, id, "A_WeaponReady", &[])
        }
        "A_FAxeCheckReadyG" => {
            if ammo1(w, id) <= 0 {
                return Some(jump(w, id, "Ready"));
            }
            call(w, id, "A_WeaponReady", &[])
        }
        "A_FAxeCheckUp" => {
            if ammo1(w, id) > 0 {
                return Some(jump(w, id, "SelectGlow"));
            }
            call(w, id, "A_Raise", &[])
        }
        "A_FAxeCheckUpG" => {
            if ammo1(w, id) <= 0 {
                return Some(jump(w, id, "Select"));
            }
            call(w, id, "A_Raise", &[])
        }
        "A_FHammerAttack" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            let dmg = rnd_range(w, "pr_hammeratk", 60, 123);
            let base = angle(w, id);
            for i in 0..16 {
                for j in [1.0, -1.0] {
                    let ang = base + j * f64::from(i) * (45.0 / 32.0);
                    let Some((slope, _)) = aim_target(w, id, ang, HAMMER_RANGE) else {
                        continue;
                    };
                    if let Some(t) = line_attack(w, id, ang, HAMMER_RANGE, slope, dmg, "HammerPuff")
                    {
                        adjust_player_angle(w, id, t);
                        if has_flag(w, t, "ISMONSTER") || is_player(w, t) {
                            let at = angle_to(w, id, t);
                            thrust(w, t, at, 10.0);
                        }
                        set_weaponspecial(w, id, 0);
                        return Some(ActionResult::Continue);
                    }
                }
            }
            let slope = aim(w, id, base, HAMMER_RANGE);
            let hit = line_attack(w, id, base, HAMMER_RANGE, slope, dmg, "HammerPuff");
            let can_throw = hit.is_none() && ammo1(w, id) > 0;
            set_weaponspecial(w, id, i32::from(can_throw));
            ActionResult::Continue
        }
        "A_FHammerThrow" => {
            if !is_player(w, id) || weaponspecial(w, id) == 0 || !deplete_ammo(w, id) {
                return Some(ActionResult::Continue);
            }
            if let Some(mo) = spawn_player_missile(w, id, "HammerMissile", 0.0) {
                set_s1(w, mo, 0);
            }
            ActionResult::Continue
        }
        "A_FPunchAttack" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            let dmg = rnd_range(w, "pr_fighteratk", 40, 55);
            let base = angle(w, id);
            for i in 0..16 {
                let off = f64::from(i) * (45.0 / 16.0);
                if try_punch(w, id, base + off, dmg, 2.0) || try_punch(w, id, base - off, dmg, 2.0)
                {
                    if weaponspecial(w, id) >= 3 {
                        set_weaponspecial(w, id, 0);
                        sound(w, id, "*fistgrunt");
                        return Some(jump(w, id, "Fire2"));
                    }
                    return Some(ActionResult::Continue);
                }
            }
            set_weaponspecial(w, id, 0);
            let slope = aim(w, id, base, MELEE_RANGE);
            let _ = line_attack(w, id, base, MELEE_RANGE, slope, dmg, "PunchPuff");
            ActionResult::Continue
        }
        "A_FSwordAttack" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            for (off, h) in [
                (45.0 / 4.0, -10.0),
                (45.0 / 8.0, -5.0),
                (0.0, 0.0),
                (-45.0 / 8.0, 5.0),
                (-45.0 / 4.0, 10.0),
            ] {
                if let Some(cid) = w.classes.find_name("FSwordMissile") {
                    let sp = class_speed(w, "FSwordMissile");
                    let opts = crate::action::PlayerMissileOpts {
                        angle: off,
                        height: h,
                        ..Default::default()
                    };
                    let _ = crate::action::spawn_player_missile_ex(w, id, cid, sp, opts);
                }
            }
            sound(w, id, "FighterSwordFire");
            ActionResult::Continue
        }
        "A_FSwordFlames" => {
            let p = pos(w, id);
            let n = rnd_range(w, "pr_fswordflame", 1, 4);
            for _ in 0..n {
                let xo = f64::from(rnd(w, "pr_fswordflame") - 128) / 16.0;
                let yo = f64::from(rnd(w, "pr_fswordflame") - 128) / 16.0;
                let zo = f64::from(rnd(w, "pr_fswordflame") - 128) / 8.0;
                let _ = spawn(
                    w,
                    "FSwordFlame",
                    DVec3::new(p.x() + xo, p.y() + yo, p.z() + zo),
                );
            }
            ActionResult::Continue
        }
        "A_FighterAttack" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            let a = angle(w, id);
            for off in [45.0 / 4.0, 45.0 / 8.0, 0.0, -45.0 / 8.0, -45.0 / 4.0] {
                let _ = spawn_missile_angle(w, id, "FSwordMissile", a + off, 0.0);
            }
            sound(w, id, "FighterSwordFire");
            ActionResult::Continue
        }
        "A_FireConePL1" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            sound(w, id, "MageShardsFire");
            let dmg = rnd_range(w, "pr_magecone", 90, 105);
            let base = angle(w, id);
            for i in 0..16 {
                let ang = base + f64::from(i) * (45.0 / 16.0);
                if let Some((_, t)) = aim_target(w, id, ang, MELEE_RANGE) {
                    let _ = damage(w, t, Some(id), Some(id), dmg);
                    return Some(ActionResult::Continue);
                }
            }
            if let Some(mo) = spawn_player_missile(w, id, "FrostMissile", 0.0) {
                set_s1(
                    w,
                    mo,
                    SHARDSPAWN_LEFT | SHARDSPAWN_DOWN | SHARDSPAWN_UP | SHARDSPAWN_RIGHT,
                );
                set_s2(w, mo, 3);
                set_target(w, mo, Some(id));
                set_arg(w, mo, 0, 3);
            }
            ActionResult::Continue
        }
        "A_FiredAttack" => {
            if let Some(t) = target(w, id) {
                if spawn_missile(w, id, t, "FireDemonMissile").is_some() {
                    sound(w, id, "FireDemonAttack");
                }
            }
            ActionResult::Continue
        }
        "A_FiredChase" => fired_chase(w, id),
        "A_FiredRocks" => {
            for _ in 0..5 {
                fired_spawn_rock(w, id);
            }
            ActionResult::Continue
        }
        "A_FiredSpawnRock" => {
            fired_spawn_rock(w, id);
            ActionResult::Continue
        }
        "A_FiredSplotch" => {
            let p = pos(w, id);
            for c in ["FireDemonSplotch1", "FireDemonSplotch2"] {
                if let Some(mo) = spawn(w, c, p) {
                    let vx = f64::from(rnd(w, "pr_firedemonsplotch") - 128) / 32.0;
                    let vy = f64::from(rnd(w, "pr_firedemonsplotch") - 128) / 32.0;
                    let vz = f64::from(rnd(w, "pr_firedemonsplotch")) / 64.0 + 3.0;
                    set_vel(w, mo, DVec3::new(vx, vy, vz));
                }
            }
            ActionResult::Continue
        }
        "A_FloatGib" => {
            add_floorclip(w, id, -1.0);
            ActionResult::Continue
        }
        "A_SinkGib" => {
            add_floorclip(w, id, 1.0);
            ActionResult::Continue
        }
        "A_FlyBuzz" => fly_buzz(w, id),
        "A_FlySearch" => {
            let p = pos(w, id);
            let corpse = w.thinkers.default.iter().copied().find(|&o| {
                o != id
                    && w.actors.get(o).is_some_and(|a| {
                        !a.destroyed
                            && a.health <= 0
                            && (a.flags & crate::class::FLAG_COUNTKILL) != 0
                            && (a.pos.x() - p.x()).hypot(a.pos.y() - p.y()) < 5.0 * 128.0
                    })
                    && check_sight(w, id, o)
            });
            if let Some(c) = corpse {
                set_target(w, id, Some(c));
                let _ = set_label(w, id, "Buzz");
            }
            ActionResult::Continue
        }
        "A_FogMove" => {
            let a = args_of(w, id);
            if a[4] == 0 {
                return Some(ActionResult::Continue);
            }
            set_arg(w, id, 3, a[3] - 1);
            if a[3] <= 0 {
                if let Some(c) = w.actors.get(id).map(|x| x.class) {
                    if let Some(s) = w.states.find_label(c, "Death") {
                        w.set_state(id, Some(s), false);
                    }
                }
                return Some(ActionResult::Continue);
            }
            if (a[3] - 1) % 4 == 0 {
                let wz = w.actors.get(id).map_or(0, |x| x.ext.weave_z);
                add_z(w, id, bob_sin(wz) / 2.0);
                if let Some(x) = w.actors.get_mut(id) {
                    x.ext.weave_z = (wz + 1) & 63;
                }
            }
            vel_from_angle(w, id, f64::from(a[0]));
            ActionResult::Continue
        }
        "A_FogSpawn" => {
            let c = s1(w, id);
            set_s1(w, id, c - 1);
            if c > 0 {
                return Some(ActionResult::Continue);
            }
            let a = args_of(w, id);
            set_s1(w, id, a[2]);
            let fog = match rnd_range(w, "pr_fogspawn", 0, 2) {
                1 => "FogPatchMedium",
                2 => "FogPatchLarge",
                _ => "FogPatchSmall",
            };
            let p = pos(w, id);
            if let Some(mo) = spawn(w, fog, p) {
                let delta = if a[1] == 0 { 1 } else { a[1] };
                let an = angle(w, id)
                    + f64::from(rnd_range(w, "pr_fogspawn", 0, delta - 1) - (delta >> 1))
                        * (360.0 / 256.0);
                set_angle(w, mo, an);
                set_target(w, mo, Some(id));
                let max = a[0].max(1);
                set_arg(w, id, 0, max);
                let sp = rnd_range(w, "pr_fogspawn", 1, max);
                let wz = rnd_range(w, "pr_fogspawn", 0, 63);
                if let Some(x) = w.actors.get_mut(mo) {
                    x.args[0] = sp;
                    x.args[3] = a[3];
                    x.args[4] = 1;
                    x.ext.weave_z = wz;
                }
            }
            ActionResult::Continue
        }
        "A_IceGuyAttack" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            let (r, a) = (radius(w, id), angle(w, id));
            for off in [90.0, -90.0] {
                let at = vec3_angle(w, id, r / 2.0, a + off, 40.0);
                let _ = spawn_missile_xyz(w, id, at, t, "IceGuyFX");
            }
            let s = class_sound(w, id, "attack");
            sound(w, id, &s);
            ActionResult::Continue
        }
        "A_IceGuyChase" => {
            let r = crate::action::a_chase(w, id);
            if rnd(w, "pr_iceguylook") < 128 {
                spawn_wisp(w, id);
            }
            r
        }
        "A_IceGuyLook" => {
            let r = crate::action::a_look(w, id);
            if rnd(w, "pr_iceguylook") < 64 {
                spawn_wisp(w, id);
            }
            r
        }
        "A_IceGuyMissileExplode" => {
            let z = pos(w, id).z() + 3.0;
            let t = target(w, id);
            for i in 0..8 {
                if let Some(mo) = spawn_missile_angle_z_speed(
                    w,
                    id,
                    z,
                    "IceGuyFX2",
                    f64::from(i) * 45.0,
                    -0.3,
                    None,
                ) {
                    set_target(w, mo, t);
                }
            }
            ActionResult::Continue
        }
        "A_KBolt" => {
            let v = s1(w, id);
            set_s1(w, id, v - 1);
            if v <= 0 {
                destroy(w, id);
            }
            ActionResult::Continue
        }
        "A_KBoltRaise" => {
            let z = pos(w, id).z() + KORAX_BOLT_HEIGHT;
            let (_, ceil) = floor_ceil(w, id);
            if z + KORAX_BOLT_HEIGHT < ceil {
                let p = pos(w, id);
                if let Some(mo) = spawn(w, "KoraxBolt", DVec3::new(p.x(), p.y(), z)) {
                    set_s1(w, mo, KORAX_BOLT_LIFETIME);
                }
            }
            ActionResult::Continue
        }
        "A_KSpiritRoam" => {
            let hp = health(w, id);
            if let Some(a) = w.actors.get_mut(id) {
                a.health -= 1;
            }
            if hp <= 0 {
                sound(w, id, "SpiritDie");
                let _ = set_label(w, id, "Death");
                return Some(ActionResult::Continue);
            }
            if let Some(t) = tracer(w, id) {
                let a0 = f64::from(args_of(w, id)[0]);
                seek_turn(w, id, t, a0, a0 * 2.0);
                seek_z(w, id, t, "pr_koraxroam");
            }
            let xy = rnd_range(w, "pr_koraxroam", 0, 4).to_string();
            let z = rnd_range(w, "pr_koraxroam", 0, 4).to_string();
            let _ = call(w, id, "A_Weave", &[&xy, &z, "4.0", "2.0"]);
            if rnd(w, "pr_koraxroam") < 50 {
                sound(w, id, "SpiritActive");
            }
            ActionResult::Continue
        }
        "A_KoraxBonePop" => {
            for i in 0..6 {
                if let Some(mo) =
                    spawn_missile_angle(w, id, "KoraxSpirit", 60.0 * f64::from(i), 5.0)
                {
                    let wz = rnd_range(w, "pr_kspiritinit", 32, 39);
                    if let Some(x) = w.actors.get_mut(mo) {
                        x.health = KORAX_SPIRIT_LIFETIME;
                        x.tracer = Some(id);
                        x.ext.weave_z = wz;
                        x.args[0] = 10;
                        x.args[1] = 0;
                    }
                    spawn_spirit_tail(w, mo);
                }
            }
            let _ = call(w, id, "ACS_Execute", &["255", "0"]);
            ActionResult::Continue
        }
        "A_KoraxChase" => korax_chase(w, id),
        "A_KoraxCommand" => {
            sound(w, id, "KoraxCommand");
            let a = angle(w, id) - 90.0;
            let at = vec3_angle(w, id, 27.0, a, 120.0);
            let _ = spawn(w, "KoraxBolt", at);
            let n = if health(w, id) <= spawn_health(w, id) >> 1 {
                4
            } else {
                3
            };
            let script = (250 + rnd_range(w, "pr_koraxcommand", 0, n)).to_string();
            let _ = call(w, id, "ACS_Execute", &[&script, "0"]);
            ActionResult::Continue
        }
        "A_KoraxDecide" => {
            let label = if rnd(w, "pr_koraxdecide") < 220 {
                "Attack"
            } else {
                "Command"
            };
            let _ = set_label(w, id, label);
            ActionResult::Continue
        }
        "A_KoraxMissile" => korax_missile(w, id),
        "A_KoraxStep" => {
            sound(w, id, "KoraxStep");
            crate::action::a_chase(w, id)
        }
        "A_LastZap" => {
            let cls = missile_name_or(w, id, "LightningZap");
            let p = pos(w, id);
            if let Some(mo) = spawn(w, &cls, p) {
                let _ = set_label(w, mo, "Death");
                set_vel_z(w, mo, 40.0);
                if let Some(x) = w.actors.get_mut(mo) {
                    x.damage = 0;
                }
            }
            ActionResult::Continue
        }
        "A_LeafCheck" => {
            let c = s1(w, id) + 1;
            set_s1(w, id, c);
            if c >= 20 {
                destroy(w, id);
                return Some(ActionResult::Continue);
            }
            let ang = target(w, id).map_or_else(|| angle(w, id), |t| angle(w, t));
            if rnd(w, "pr_leafcheck") > 64 {
                let v = vel(w, id);
                if v.x() == 0.0 && v.y() == 0.0 {
                    let s = f64::from(rnd(w, "pr_leafcheck")) / 128.0 + 1.0;
                    thrust(w, id, ang, s);
                }
                return Some(ActionResult::Continue);
            }
            let _ = set_label(w, id, "Looping");
            let vz = f64::from(rnd(w, "pr_leafcheck")) / 128.0 + 1.0;
            set_vel_z(w, id, vz);
            let s = f64::from(rnd(w, "pr_leafcheck")) / 128.0 + 2.0;
            thrust(w, id, ang, s);
            set_flag(w, id, "MISSILE", true);
            ActionResult::Continue
        }
        "A_LeafSpawn" => {
            let p = pos(w, id);
            let a = angle(w, id);
            let n = rnd_range(w, "pr_leafspawn", 1, 4);
            for _ in 0..n {
                let xo = f64::from(rnd2(w, "pr_leafspawn")) / 4.0;
                let yo = f64::from(rnd2(w, "pr_leafspawn")) / 4.0;
                let zo = f64::from(rnd(w, "pr_leafspawn")) / 4.0;
                let cls = if rnd_range(w, "pr_leafspawn", 0, 1) == 0 {
                    "Leaf1"
                } else {
                    "Leaf2"
                };
                if let Some(mo) = spawn(w, cls, DVec3::new(p.x() + xo, p.y() + yo, p.z() + zo)) {
                    let s = f64::from(rnd(w, "pr_leafspawn")) / 128.0 + 3.0;
                    thrust(w, mo, a, s);
                    set_target(w, mo, Some(id));
                    set_s1(w, mo, 0);
                }
            }
            ActionResult::Continue
        }
        "A_LeafThrust" => {
            if rnd(w, "pr_leafthrust") <= 96 {
                let vz = vel(w, id).z() + f64::from(rnd(w, "pr_leafthrust")) / 128.0 + 1.0;
                set_vel_z(w, id, vz);
            }
            ActionResult::Continue
        }
        "A_LightningClip" => {
            lightning_clip(w, id);
            ActionResult::Continue
        }
        "A_LightningReady" => {
            let r = call(w, id, "A_WeaponReady", &[]);
            if rnd(w, "pr_lightningready") < 160 {
                sound(w, id, "MageLightningReady");
            }
            r
        }
        "A_LightningRemove" => {
            if let Some(mo) = w.actors.get(id).and_then(|a| a.ext.last_enemy) {
                set_flag(w, id, "NOTARGET", true);
                if let Some(x) = w.actors.get_mut(mo) {
                    x.ext.last_enemy = None;
                }
                crate::physics::explode_missile(w, mo);
            }
            ActionResult::Continue
        }
        "A_LightningZap" => {
            lightning_zap(w, id);
            ActionResult::Continue
        }
        "A_MLightningAttack" => {
            let fl = sarg(args, 0, "LightningFloor");
            let ce = sarg(args, 1, "LightningCeiling");
            let fmo = spawn_player_missile(w, id, &fl, 0.0);
            let cmo = spawn_player_missile(w, id, &ce, 0.0);
            if let Some(f) = fmo {
                set_s1(w, f, 0);
                if let Some(x) = w.actors.get_mut(f) {
                    x.ext.last_enemy = cmo;
                }
                set_flag(w, f, "FLOORHUGGER", true);
                lightning_zap(w, f);
            }
            if let Some(c) = cmo {
                set_tracer(w, c, None);
                if let Some(x) = w.actors.get_mut(c) {
                    x.ext.last_enemy = fmo;
                }
                set_flag(w, c, "CEILINGHUGGER", true);
                lightning_zap(w, c);
            }
            sound(w, id, "MageLightningFire");
            let _ = deplete_ammo(w, id);
            ActionResult::Continue
        }
        "A_MStaffAttack" => {
            if !player_fire(w, id) {
                return Some(ActionResult::Continue);
            }
            let a = angle(w, id);
            let alt = aim_target(w, id, a, PLAYER_MISSILE_RANGE)
                .map(|(_, t)| t)
                .or_else(|| rough_monster_search(w, id, 10, true));
            for off in [0.0, -5.0, 5.0] {
                if let Some(mo) = spawn_player_missile(w, id, "MageStaffFX2", off) {
                    set_target(w, mo, Some(id));
                    set_tracer(w, mo, alt);
                }
            }
            sound(w, id, "MageStaffFire");
            ActionResult::Continue
        }
        "A_MStaffTrack" => {
            if tracer(w, id).is_none() && rnd(w, "pr_mstafftrack") < 50 {
                let t = rough_monster_search(w, id, 10, false);
                set_tracer(w, id, t);
            }
            call(w, id, "A_SeekerMissile", &["2", "10"])
        }
        "A_MageAttack" => {
            if target(w, id).is_none() {
                return Some(ActionResult::Continue);
            }
            let (a, z) = (angle(w, id), pos(w, id).z() + 40.0);
            for off in [0.0, -5.0, 5.0] {
                if let Some(mo) =
                    spawn_missile_angle_z_speed(w, id, z, "MageStaffFX2", a + off, 0.0, None)
                {
                    set_target(w, mo, Some(id));
                    let t = rough_monster_search(w, id, 10, true);
                    set_tracer(w, mo, t);
                }
            }
            sound(w, id, "MageStaffFire");
            ActionResult::Continue
        }
        "A_PoisonBagCheck" => {
            let v = s1(w, id) - 1;
            set_s1(w, id, v);
            if v <= 0 {
                let _ = set_label(w, id, "Death");
            }
            ActionResult::Continue
        }
        "A_PoisonBagDamage" => {
            explode(w, id, 4, 40.0);
            let v = s2(w, id);
            add_z(w, id, bob_sin(v) / 16.0);
            set_s2(w, id, (v + 1) & 63);
            ActionResult::Continue
        }
        "A_PoisonBagInit" => {
            let p = pos(w, id);
            if let Some(mo) = spawn(w, "PoisonCloud", DVec3::new(p.x(), p.y(), p.z() + 28.0)) {
                let t = target(w, id);
                set_target(w, mo, t);
            }
            ActionResult::Continue
        }
        "A_PoisonShroom" => {
            let t = 128 + (rnd(w, "pr_poisonshroom") << 1);
            set_tics(w, id, t);
            ActionResult::Continue
        }
        "A_PotteryCheck" => {
            let mos: Vec<ActorId> = w.players.iter().flatten().filter_map(|p| p.mo).collect();
            for mo in mos {
                if check_sight(w, mo, id)
                    && delta_angle(angle(w, mo), angle_to(w, mo, id)).abs() <= 45.0
                {
                    let label = w
                        .actors
                        .get(id)
                        .and_then(|a| a.ext.scratch.get("loopstate").copied());
                    if let Some(s) = label {
                        w.set_state(id, Some(StateId::from_raw(s as u32)), true);
                    }
                    return Some(ActionResult::Continue);
                }
            }
            ActionResult::Continue
        }
        "A_PotteryChooseBit" => {
            let bit = ["Pottery1", "Pottery2", "Pottery3", "Pottery4", "Pottery5"]
                [rnd_range(w, "pr_potterybit", 0, 4) as usize];
            if let Some(s) = w
                .actors
                .get(id)
                .and_then(|a| w.states.find_label(a.class, bit))
            {
                if let Some(a) = w.actors.get_mut(id) {
                    a.ext.scratch.insert("loopstate".into(), f64::from(s.raw()));
                }
                w.set_state(id, Some(s), true);
            }
            let t = 256 + (rnd(w, "pr_potterybit") << 1);
            set_tics(w, id, t);
            ActionResult::Continue
        }
        "A_PotteryExplode" => {
            let p = pos(w, id);
            let n = rnd_range(w, "pr_pottery", 3, 6);
            let mut last = None;
            for _ in 0..n {
                if let Some(mo) = spawn(w, "PotteryBit", p) {
                    let f = rnd_range(w, "pr_pottery", 0, 4);
                    set_spawn_state_plus(w, mo, f as u32);
                    let vx = f64::from(rnd2(w, "pr_pottery")) / 64.0;
                    let vy = f64::from(rnd2(w, "pr_pottery")) / 64.0;
                    let vz = f64::from(rnd_range(w, "pr_pottery", 5, 12)) * 0.75;
                    set_vel(w, mo, DVec3::new(vx, vy, vz));
                    last = Some(mo);
                }
            }
            if let Some(l) = last {
                sound(w, l, "PotteryExplode");
            }
            spawn_by_spawn_id(w, id, args_of(w, id)[0]);
            ActionResult::Continue
        }
        "A_SoAExplode" => {
            let p = pos(w, id);
            let h = height(w, id);
            for i in 0..10 {
                let xo = f64::from(rnd(w, "pr_soaexplode") - 128) / 16.0;
                let yo = f64::from(rnd(w, "pr_soaexplode") - 128) / 16.0;
                let zo = f64::from(rnd(w, "pr_soaexplode")) * h / 256.0;
                if let Some(mo) = spawn(
                    w,
                    "ZArmorChunk",
                    DVec3::new(p.x() + xo, p.y() + yo, p.z() + zo),
                ) {
                    set_spawn_state_plus(w, mo, i);
                    let vx = f64::from(rnd2(w, "pr_soaexplode")) / 64.0;
                    let vy = f64::from(rnd2(w, "pr_soaexplode")) / 64.0;
                    let vz = f64::from(rnd_range(w, "pr_soaexplode", 5, 12));
                    set_vel(w, mo, DVec3::new(vx, vy, vz));
                }
            }
            spawn_by_spawn_id(w, id, args_of(w, id)[0]);
            let s = class_sound(w, id, "death");
            sound(w, id, &s);
            destroy(w, id);
            ActionResult::Continue
        }
        "A_SerpentCheckForAttack" => {
            serpent_check_for_attack(w, id);
            ActionResult::Continue
        }
        "A_SerpentChooseAttack" => {
            if target(w, id).is_none() || in_melee_range(w, id) {
                return Some(ActionResult::Continue);
            }
            let _ = set_label(w, id, "Missile");
            ActionResult::Continue
        }
        "A_SerpentHeadCheck" => {
            let (floor, _) = floor_ceil(w, id);
            if pos(w, id).z() <= floor {
                let _ = set_label(w, id, "Death");
            }
            ActionResult::Continue
        }
        "A_SerpentHide" => {
            set_flag(w, id, "INVISIBLE", true);
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.floorclip = 0.0;
            }
            ActionResult::Continue
        }
        "A_SerpentUnHide" => {
            set_flag(w, id, "INVISIBLE", false);
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.floorclip = 24.0;
            }
            ActionResult::Continue
        }
        "A_SerpentHumpDecide" => {
            let has_missile = w
                .actors
                .get(id)
                .is_some_and(|a| w.states.find_label(a.class, "Missile").is_some());
            if has_missile {
                if rnd(w, "pr_serpenthump") > 30 {
                    return Some(ActionResult::Continue);
                } else if rnd(w, "pr_serpenthump") < 40 {
                    let _ = set_label(w, id, "Melee");
                    return Some(ActionResult::Continue);
                }
            } else if rnd(w, "pr_serpenthump") > 3 {
                return Some(ActionResult::Continue);
            }
            if !in_melee_range(w, id) {
                if has_missile && rnd(w, "pr_serpenthump") < 128 {
                    let _ = set_label(w, id, "Melee");
                } else {
                    let _ = set_label(w, id, "Hump");
                    sound(w, id, "SerpentActive");
                }
            }
            ActionResult::Continue
        }
        "A_SerpentLowerHump" => {
            add_floorclip(w, id, 4.0);
            ActionResult::Continue
        }
        "A_SerpentRaiseHump" => {
            add_floorclip(w, id, -4.0);
            ActionResult::Continue
        }
        "A_SerpentMeleeAttack" => {
            let Some(t) = target(w, id) else {
                return Some(ActionResult::Continue);
            };
            if in_melee_range(w, id) {
                let d = rnd_range(w, "pr_serpentattack", 1, 8) * 5;
                let _ = damage(w, t, Some(id), Some(id), d);
                sound(w, id, "SerpentMeleeHit");
            }
            if rnd(w, "pr_serpentattack") < 96 {
                serpent_check_for_attack(w, id);
            }
            ActionResult::Continue
        }
        "A_SerpentSpawnGibs" => {
            let p = pos(w, id);
            let (floor, _) = floor_ceil(w, id);
            for c in ["SerpentGib1", "SerpentGib2", "SerpentGib3"] {
                let x = f64::from(rnd(w, "pr_serpentgibs") - 128) / 16.0;
                let y = f64::from(rnd(w, "pr_serpentgibs") - 128) / 16.0;
                if let Some(mo) = spawn(w, c, DVec3::new(p.x() + x, p.y() + y, floor + 1.0)) {
                    let vx = f64::from(rnd(w, "pr_serpentgibs") - 128) / 1024.0;
                    let vy = f64::from(rnd(w, "pr_serpentgibs") - 128) / 1024.0;
                    set_vel(w, mo, DVec3::new(vx, vy, 0.0));
                    if let Some(a) = w.actors.get_mut(mo) {
                        a.ext.floorclip = 6.0;
                    }
                }
            }
            ActionResult::Continue
        }
        "A_ShedShard" => {
            shed_shard(w, id);
            ActionResult::Continue
        }
        "A_SmBounce" => {
            let (floor, _) = floor_ceil(w, id);
            set_z(w, id, floor + 1.0);
            let vz = 2.0 + f64::from(rnd(w, "pr_smbounce")) / 64.0;
            let vx = f64::from(rnd_range(w, "pr_smbounce", 0, 2));
            let vy = f64::from(rnd_range(w, "pr_smbounce", 0, 2));
            set_vel(w, id, DVec3::new(vx, vy, vz));
            ActionResult::Continue
        }
        "A_SnoutAttack" => {
            if !is_player(w, id) {
                return Some(ActionResult::Continue);
            }
            let dmg = rnd_range(w, "pr_snoutattack", 3, 6);
            let a = angle(w, id);
            let slope = aim(w, id, a, MELEE_RANGE);
            let hit = line_attack(w, id, a, MELEE_RANGE, slope, dmg, "SnoutPuff");
            sound(w, id, "PigActive");
            if let Some(t) = hit {
                adjust_player_angle(w, id, t);
                sound(w, id, "PigAttack");
            }
            ActionResult::Continue
        }
        "A_Summon" => summon(w, id),
        "A_TeloSpawnA" | "A_TeloSpawnB" | "A_TeloSpawnC" | "A_TeloSpawnD" => {
            let cls = match name {
                "A_TeloSpawnA" => "TelOtherFX2",
                "A_TeloSpawnB" => "TelOtherFX3",
                "A_TeloSpawnC" => "TelOtherFX4",
                _ => "TelOtherFX5",
            };
            let (p, a, t, v) = (pos(w, id), angle(w, id), target(w, id), vel(w, id));
            if let Some(fx) = spawn_facing(w, cls, p, a) {
                set_s1(w, fx, TELEPORT_LIFE);
                set_target(w, fx, t);
                set_vel(w, fx, DVec3::new(v.x() / 2.0, v.y() / 2.0, v.z() / 2.0));
            }
            ActionResult::Continue
        }
        "A_ThrustImpale" => {
            let (p, r, h) = (pos(w, id), radius(w, id), height(w, id));
            let ids: Vec<ActorId> = w.thinkers.default.clone();
            for t in ids {
                if t == id || !has_flag(w, t, "SHOOTABLE") {
                    continue;
                }
                let (tp, tr, th) = (pos(w, t), radius(w, t), height(w, t));
                let bd = r + tr;
                if (tp.x() - p.x()).abs() >= bd || (tp.y() - p.y()).abs() >= bd {
                    continue;
                }
                if tp.z() + th < p.z() || tp.z() > p.z() + h {
                    continue;
                }
                let _ = damage(w, t, Some(id), Some(id), 10_001);
                set_arg(w, id, 1, 1);
            }
            ActionResult::Continue
        }
        "A_ThrustInitDn" => {
            set_s2(w, id, 5);
            set_arg(w, id, 0, 0);
            let h = w
                .actors
                .get(id)
                .and_then(|a| w.classes.get(a.class))
                .map_or(0.0, |c| c.height);
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.floorclip = h;
            }
            set_flag(w, id, "SOLID", false);
            set_flag(w, id, "NOTELEPORT", true);
            set_flag(w, id, "FLOORCLIP", true);
            set_flag(w, id, "INVISIBLE", true);
            let p = pos(w, id);
            let clump = spawn(w, "DirtClump", p);
            if let Some(a) = w.actors.get_mut(id) {
                a.master = clump;
            }
            ActionResult::Continue
        }
        "A_ThrustInitUp" => {
            set_s2(w, id, 5);
            set_arg(w, id, 0, 1);
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.floorclip = 0.0;
            }
            set_flag(w, id, "SOLID", true);
            set_flag(w, id, "NOTELEPORT", true);
            set_flag(w, id, "FLOORCLIP", true);
            set_s1(w, id, 0);
            ActionResult::Continue
        }
        "A_ThrustLower" => {
            if sink(w, id, 6.0) {
                set_arg(w, id, 0, 0);
                let label = if args_of(w, id)[1] != 0 {
                    "BloodThrustInit1"
                } else {
                    "ThrustInit1"
                };
                if let Some(s) = w
                    .actors
                    .get(id)
                    .and_then(|a| w.states.find_label(a.class, label))
                {
                    w.set_state(id, Some(s), false);
                }
            }
            ActionResult::Continue
        }
        "A_ThrustRaise" => {
            let sp = f64::from(s2(w, id));
            if raise(w, id, sp) {
                set_arg(w, id, 0, 1);
                let label = if args_of(w, id)[1] != 0 {
                    "BloodThrustInit2"
                } else {
                    "ThrustInit2"
                };
                if let Some(s) = w
                    .actors
                    .get(id)
                    .and_then(|a| w.states.find_label(a.class, label))
                {
                    w.set_state(id, Some(s), false);
                }
            }
            if floorclip(w, id) < height(w, id) {
                if let Some(m) = master(w, id) {
                    destroy(w, m);
                    if let Some(a) = w.actors.get_mut(id) {
                        a.master = None;
                    }
                }
            }
            if rnd(w, "pr_thrustraise") < 40 {
                let r = radius(w, id);
                spawn_dirt(w, id, r);
            }
            set_s2(w, id, s2(w, id) + 1);
            ActionResult::Continue
        }
        "A_WraithChase" => {
            let wz = w.actors.get(id).map_or(0, |a| a.ext.weave_z);
            add_z(w, id, bob_sin(wz));
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.weave_z = (wz + 2) & 63;
            }
            let r = crate::action::a_chase(w, id);
            wraith_fx4(w, id);
            r
        }
        "A_WraithFX2" => {
            let (p, a) = (pos(w, id), angle(w, id));
            for _ in 0..2 {
                if let Some(mo) = spawn(w, "WraithFX2", p) {
                    let mut na = f64::from(rnd(w, "pr_wraithfx2")) * (360.0 / 1024.0);
                    if rnd(w, "pr_wraithfx2") >= 128 {
                        na = -na;
                    }
                    na += a;
                    let m = f64::from(rnd(w, "pr_wraithfx2")) / 512.0 + 1.0;
                    let (vx, vy) = angle_vec(na, m);
                    set_vel(w, mo, DVec3::new(vx, vy, 0.0));
                    set_target(w, mo, Some(id));
                    if let Some(x) = w.actors.get_mut(mo) {
                        x.ext.floorclip = 10.0;
                    }
                }
            }
            ActionResult::Continue
        }
        "A_WraithFX3" => {
            let n = rnd_range(w, "pr_wraithfx3", 0, 14);
            let p = pos(w, id);
            for _ in 0..n {
                let xo = f64::from(rnd(w, "pr_wraithfx3") - 128) / 32.0;
                let yo = f64::from(rnd(w, "pr_wraithfx3") - 128) / 32.0;
                let zo = f64::from(rnd(w, "pr_wraithfx3")) / 64.0;
                if let Some(mo) = spawn(
                    w,
                    "WraithFX3",
                    DVec3::new(p.x() + xo, p.y() + yo, p.z() + zo),
                ) {
                    set_target(w, mo, Some(id));
                }
            }
            ActionResult::Continue
        }
        "A_WraithFX4" => {
            wraith_fx4(w, id);
            ActionResult::Continue
        }
        "A_WraithInit" => {
            add_z(w, id, 48.0);
            let (_, ceil) = floor_ceil(w, id);
            let h = height(w, id);
            if pos(w, id).z() + h > ceil {
                set_z(w, id, ceil - h);
            }
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.weave_z = 0;
            }
            ActionResult::Continue
        }
        "A_WraithMelee" => {
            if in_melee_range(w, id) && rnd(w, "pr_stealhealth") < 220 {
                let amount = rnd_range(w, "pr_stealhealth", 1, 8) * 2;
                if let Some(t) = target(w, id) {
                    let _ = damage(w, t, Some(id), Some(id), amount);
                }
                if let Some(a) = w.actors.get_mut(id) {
                    a.health += amount;
                }
            }
            ActionResult::Continue
        }
        "A_WraithRaise" => {
            if raise(w, id, 2.0) {
                set_flag(w, id, "DONTMORPH", false);
                set_flag(w, id, "SPECIALFLOORCLIP", false);
                let _ = set_label(w, id, "Chase");
            }
            let r = radius(w, id);
            spawn_dirt(w, id, r);
            ActionResult::Continue
        }
        "A_WraithRaiseInit" => {
            set_flag(w, id, "INVISIBLE", false);
            set_flag(w, id, "NONSHOOTABLE", false);
            set_flag(w, id, "DONTBLAST", false);
            set_flag(w, id, "SHOOTABLE", true);
            set_flag(w, id, "SOLID", true);
            let h = height(w, id);
            if let Some(a) = w.actors.get_mut(id) {
                a.ext.floorclip = h;
            }
            ActionResult::Continue
        }
        "A_ZapMimic" => {
            if let Some(mo) = w.actors.get(id).and_then(|a| a.ext.last_enemy) {
                if has_flag(w, mo, "NOTARGET") {
                    crate::physics::explode_missile(w, id);
                } else {
                    let v = vel(w, mo);
                    let vz = vel(w, id).z();
                    set_vel(w, id, DVec3::new(v.x(), v.y(), vz));
                }
            }
            ActionResult::Continue
        }
        _ => return None,
    };
    Some(r)
}

fn missilecount(w: &World, id: ActorId) -> i32 {
    w.actors
        .get(id)
        .and_then(|a| a.ext.scratch.get("missilecount").copied())
        .unwrap_or(0.0) as i32
}

fn set_missilecount(w: &mut World, id: ActorId, v: i32) {
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.scratch.insert("missilecount".into(), f64::from(v));
    }
}

fn bobstate(w: &World, id: ActorId) -> i32 {
    w.actors
        .get(id)
        .and_then(|a| a.ext.scratch.get("bobstate").copied())
        .unwrap_or(0.0) as i32
}

fn set_bobstate(w: &mut World, id: ActorId, v: i32) {
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.scratch.insert("bobstate".into(), f64::from(v));
    }
}

/// The actor this missile last hit (`BlockingMobj`), as recorded by the playsim.
fn last_hit_actor(w: &World, id: ActorId) -> Option<ActorId> {
    w.actors.get(id).and_then(|a| a.hit_actor)
}

fn missile_name_or(w: &World, id: ActorId, def: &str) -> String {
    w.actors
        .get(id)
        .and_then(|a| w.classes.get(a.class))
        .map(|c| c.missile_type.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| def.to_owned())
}

fn check_throw_bomb(w: &mut World, id: ActorId) {
    let hp = health(w, id) - 1;
    if let Some(a) = w.actors.get_mut(id) {
        a.health = hp;
    }
    if hp <= 0 {
        let _ = set_label(w, id, "Death");
    }
}

fn sink(w: &mut World, id: ActorId, speed: f64) -> bool {
    let (fc, h) = (floorclip(w, id), height(w, id));
    if fc < h {
        add_floorclip(w, id, speed);
        false
    } else {
        true
    }
}

fn raise(w: &mut World, id: ActorId, speed: f64) -> bool {
    let fc = floorclip(w, id);
    if fc > 0.0 {
        let n = fc - speed;
        if let Some(a) = w.actors.get_mut(id) {
            a.ext.floorclip = n.max(0.0);
        }
        n <= 0.0
    } else {
        true
    }
}

fn spawn_dirt(w: &mut World, id: ActorId, r: f64) {
    const CHUNKS: [&str; 6] = ["Dirt1", "Dirt2", "Dirt3", "Dirt4", "Dirt5", "Dirt6"];
    let zo = f64::from(rnd(w, "pr_dirt")) / 128.0 + 1.0;
    let a = f64::from(rnd(w, "pr_dirt")) * (360.0 / 256.0);
    let at = vec3_angle(w, id, r, a, zo);
    let cls = CHUNKS[rnd_range(w, "pr_dirt", 0, 5) as usize];
    if let Some(mo) = spawn(w, cls, at) {
        let vz = f64::from(rnd(w, "pr_dirt")) / 64.0;
        set_vel_z(w, mo, vz);
    }
}

/// `GetSpawnableType(spawnid)` then spawn it (Hexen pottery / armor drops).
fn spawn_by_spawn_id(w: &mut World, id: ActorId, spawn_id: i32) {
    if spawn_id <= 0 {
        return;
    }
    let Some(cid) = w
        .classes
        .iter()
        .find(|c| c.spawn_id == spawn_id)
        .map(|c| c.id)
    else {
        return;
    };
    // Room has no `-nomonsters` world switch, so monsters always spawn.
    let p = pos(w, id);
    let _ = w.spawn_at(cid, p, Angle::ZERO);
}

fn spawn_wisp(w: &mut World, id: ActorId) {
    let r = radius(w, id);
    let dist = f64::from(rnd(w, "pr_iceguylook") - 128) * r / 128.0;
    let a = angle(w, id) + 90.0;
    let at = vec3_angle(w, id, dist, a, 60.0);
    let cls = if rnd_range(w, "pr_iceguylook", 0, 1) == 0 {
        "IceGuyWisp1"
    } else {
        "IceGuyWisp2"
    };
    if let Some(mo) = spawn(w, cls, at) {
        let v = vel(w, id);
        set_vel(w, mo, v);
        set_target(w, mo, Some(id));
    }
}

fn wraith_fx4(w: &mut World, id: ActorId) {
    let chance = rnd(w, "pr_wraithfx4");
    let (s4, s5) = match chance {
        c if c < 10 => (true, false),
        c if c < 20 => (false, true),
        c if c < 25 => (true, true),
        _ => (false, false),
    };
    let p = pos(w, id);
    for (on, cls, div) in [(s4, "WraithFX4", 16.0), (s5, "WraithFX5", 32.0)] {
        if !on {
            continue;
        }
        let xo = f64::from(rnd(w, "pr_wraithfx4") - 128) / div;
        let yo = f64::from(rnd(w, "pr_wraithfx4") - 128) / div;
        let zo = f64::from(rnd(w, "pr_wraithfx4")) / 64.0;
        if let Some(mo) = spawn(w, cls, DVec3::new(p.x() + xo, p.y() + yo, p.z() + zo)) {
            set_target(w, mo, Some(id));
        }
    }
}

/// `TryPunch(angle, damage, power)` for the fighter's fist.
fn try_punch(w: &mut World, id: ActorId, ang: f64, mut dmg: i32, mut power: f64) -> bool {
    let Some((slope, _)) = aim_target(w, id, ang, 2.0 * MELEE_RANGE) else {
        return false;
    };
    let ws = weaponspecial(w, id) + 1;
    set_weaponspecial(w, id, ws);
    let puff = if ws >= 3 {
        dmg <<= 1;
        power *= 3.0;
        "HammerPuff"
    } else {
        "PunchPuff"
    };
    let Some(t) = line_attack(w, id, ang, 2.0 * MELEE_RANGE, slope, dmg, puff) else {
        return false;
    };
    let light = w.actors.get(t).is_some_and(|a| a.mass < 10_000_000);
    if (is_player(w, t) || (light && has_flag(w, t, "ISMONSTER"))) && !has_flag(w, t, "DONTTHRUST")
    {
        let at = angle_to(w, id, t);
        thrust(w, t, at, power);
    }
    adjust_player_angle(w, id, t);
    true
}

/// The cleric mace / similar sweep: try `n` angle steps either side.
fn melee_sweep(
    w: &mut World,
    id: ActorId,
    n: i32,
    step: f64,
    range: f64,
    dmg: i32,
    puff: &str,
    on_hit: impl Fn(&mut World, ActorId, ActorId),
) {
    let base = angle(w, id);
    for i in 0..n {
        for j in [1.0, -1.0] {
            let ang = base + j * f64::from(i) * step;
            let Some((slope, _)) = aim_target(w, id, ang, range) else {
                continue;
            };
            if let Some(t) = line_attack(w, id, ang, range, slope, dmg, puff) {
                on_hit(w, id, t);
                return;
            }
        }
    }
    set_weaponspecial(w, id, 0);
    let slope = aim(w, id, base, MELEE_RANGE);
    let _ = line_attack(w, id, base, MELEE_RANGE, slope, dmg, puff);
}

fn spawn_spirit_tail(w: &mut World, spirit: ActorId) {
    let p = pos(w, spirit);
    let Some(tail) = spawn(w, "HolyTail", p) else {
        return;
    };
    set_target(w, tail, Some(spirit));
    let mut cur = tail;
    for _ in 1..3 {
        let next = spawn(w, "HolyTailTrail", p);
        set_tracer(w, cur, next);
        let Some(n) = next else {
            break;
        };
        cur = n;
    }
    set_tracer(w, cur, None);
}

fn holy_tail_follow(w: &mut World, id: ActorId, mut dist: f64) {
    let mut mo = Some(id);
    let mut guard = 0;
    while let Some(m) = mo {
        guard += 1;
        if guard > 16 {
            break;
        }
        let child = w.actors.get(m).and_then(|a| a.tracer);
        if let Some(c) = child {
            let an = angle_to(w, m, c);
            let old = dist2d(w, c, m);
            let mp = pos(w, m);
            let (dx, dy) = angle_vec(an, dist);
            if crate::physics::try_move(w, c, DVec3::new(mp.x() + dx, mp.y() + dy, pos(w, c).z())) {
                let new = dist2d(w, c, m) - 1.0;
                let cz = pos(w, c).z();
                let z = if old < 1.0 {
                    if cz < mp.z() {
                        mp.z() - dist
                    } else {
                        mp.z() + dist
                    }
                } else {
                    mp.z() + new * (cz - mp.z()) / old
                };
                set_z(w, c, z);
            }
        }
        mo = child;
        dist -= 1.0;
    }
}

fn holy_find_target(w: &mut World, id: ActorId) {
    if let Some(t) = rough_monster_search(w, id, 6, false) {
        set_tracer(w, id, Some(t));
        set_flag(w, id, "NOCLIP", true);
        set_flag(w, id, "SKULLFLY", true);
        set_flag(w, id, "MISSILE", false);
    }
}

fn holy_seek(w: &mut World, id: ActorId) -> ActionResult {
    let hp = health(w, id) - 1;
    if let Some(a) = w.actors.get_mut(id) {
        a.health = hp;
    }
    if hp <= 0 {
        let v = vel(w, id);
        set_vel(w, id, DVec3::new(v.x() / 4.0, v.y() / 4.0, 0.0));
        let _ = set_label(w, id, "Death");
        let d = rnd_range(w, "pr_holyseeker", 0, 3);
        if let Some(a) = w.actors.get_mut(id) {
            a.tics -= d;
        }
        return ActionResult::Continue;
    }
    if let Some(t) = tracer(w, id) {
        if !has_flag(w, t, "SHOOTABLE") || (!has_flag(w, t, "ISMONSTER") && !is_player(w, t)) {
            set_tracer(w, id, None);
            set_flag(w, id, "NOCLIP", false);
            set_flag(w, id, "SKULLFLY", false);
            set_flag(w, id, "MISSILE", true);
            holy_find_target(w, id);
        } else {
            let a0 = f64::from(args_of(w, id)[0]);
            seek_turn(w, id, t, a0, a0 * 2.0);
            seek_z(w, id, t, "pr_holyseeker");
            if (w.level.maptime + 7) & 15 == 0 {
                let n = 5 + rnd(w, "pr_holyseeker") / 20;
                set_arg(w, id, 0, n);
            }
        }
    }
    let xy = rnd_range(w, "pr_holyseeker", 0, 4).to_string();
    let z = rnd_range(w, "pr_holyseeker", 0, 4).to_string();
    call(w, id, "A_Weave", &[&xy, &z, "4.0", "2.0"])
}

fn dragon_flight(w: &mut World, id: ActorId) -> ActionResult {
    dragon_seek(w, id, 4.0, 8.0);
    let Some(t) = target(w, id) else {
        return crate::action::a_look(w, id);
    };
    if !has_flag(w, t, "SHOOTABLE") {
        set_target(w, id, None);
        return ActionResult::Continue;
    }
    let ang = delta_angle(angle(w, id), angle_to(w, id, t)).abs();
    if ang < 22.5 && in_melee_range(w, id) {
        let d = rnd_range(w, "pr_dragonflight", 1, 8) * 8;
        let _ = damage(w, t, Some(id), Some(id), d);
        let s = class_sound(w, id, "attack");
        sound(w, id, &s);
    } else if ang <= 20.0 {
        let _ = set_label(w, id, "Missile");
        let s = class_sound(w, id, "attack");
        sound(w, id, &s);
    }
    ActionResult::Continue
}

fn dragon_seek(w: &mut World, id: ActorId, thresh: f64, turn_max: f64) {
    let Some(targ) = tracer(w, id) else {
        return;
    };
    seek_turn(w, id, targ, thresh, turn_max);
    let sp = speed(w, id);
    let dist = dist_by_speed(w, targ, id, sp);
    let (p, h) = (pos(w, id), height(w, id));
    let (tp, th) = (pos(w, targ), height(w, targ));
    if p.z() + h < tp.z() || tp.z() + th < p.z() {
        set_vel_z(w, id, (tp.z() - p.z()) / dist);
    }
    if has_flag(w, targ, "SHOOTABLE")
        && rnd(w, "pr_dragonseek") < 64
        && delta_angle(angle(w, id), angle_to(w, id, targ)).abs() < 22.5
    {
        let old = target(w, id);
        set_target(w, id, Some(targ));
        if in_melee_range(w, id) {
            let d = rnd_range(w, "pr_dragonseek", 1, 8) * 10;
            let _ = damage(w, targ, Some(id), Some(id), d);
            let s = class_sound(w, id, "attack");
            sound(w, id, &s);
        } else if rnd(w, "pr_dragonseek") < 128 && crate::action::check_missile_range(w, id, targ) {
            let _ = spawn_missile(w, id, targ, "DragonFireball");
            let s = class_sound(w, id, "attack");
            sound(w, id, &s);
        }
        set_target(w, id, old);
    }
    if dist2d(w, id, targ) < 4.0 * sp.max(1.0) {
        // Reached the spot: pick the next one from its args (tids).
        let targs = args_of(w, targ);
        let candidates: Vec<ActorId> = targs
            .iter()
            .filter(|&&tid| tid != 0)
            .filter_map(|&tid| {
                w.thinkers.default.iter().copied().find(|&o| {
                    w.actors
                        .get(o)
                        .is_some_and(|a| a.tid == tid && !a.destroyed)
                })
            })
            .collect();
        if !candidates.is_empty() {
            let pick = candidates[(rnd(w, "pr_dragonseek") as usize) % candidates.len()];
            set_tracer(w, id, Some(pick));
        }
    }
}

fn fired_chase(w: &mut World, id: ActorId) -> ActionResult {
    let wi = s1(w, id);
    if let Some(a) = w.actors.get_mut(id) {
        if a.reaction_time > 0 {
            a.reaction_time -= 1;
        }
        if a.threshold > 0 {
            a.threshold -= 1;
        }
    }
    add_z(w, id, bob_sin(wi));
    set_s1(w, id, (wi + 2) & 63);
    let (floor, _) = floor_ceil(w, id);
    if pos(w, id).z() < floor + 64.0 {
        add_z(w, id, 2.0);
    }
    let Some(t) = target(w, id).filter(|&t| has_flag(w, t, "SHOOTABLE")) else {
        return crate::action::a_look(w, id);
    };
    let strafe = w
        .actors
        .get(id)
        .and_then(|a| a.ext.scratch.get("fdstrafe").copied())
        .unwrap_or(0.0) as i32;
    let mut strafe_now;
    if strafe > 0 {
        strafe_now = strafe - 1;
    } else {
        strafe_now = 0;
        let vz = vel(w, id).z();
        set_vel(w, id, DVec3::new(0.0, 0.0, vz));
        if dist2d(w, id, t) < FIREDEMON_ATTACK_RANGE && rnd(w, "pr_firedchase") < 30 {
            let mut ang = angle_to(w, id, t);
            ang += if rnd(w, "pr_firedchase") < 128 {
                90.0
            } else {
                -90.0
            };
            thrust(w, id, ang, 8.0);
            strafe_now = 3;
        }
    }
    if let Some(a) = w.actors.get_mut(id) {
        a.ext
            .scratch
            .insert("fdstrafe".into(), f64::from(strafe_now));
    }
    let _ = crate::action::a_face_movement_direction(w, id);
    if strafe_now == 0 {
        let mc = w.actors.get_mut(id).map_or(0, |a| {
            a.move_count -= 1;
            a.move_count
        });
        if mc < 0 || !crate::action::chase_move(w, id) {
            crate::action::new_chase_dir(w, id, t);
        }
    }
    let just = w
        .actors
        .get(id)
        .is_some_and(|a| a.flags & crate::class::FLAG_JUSTATTACKED != 0);
    if !just {
        if crate::action::check_missile_range(w, id, t) && rnd(w, "pr_firedchase") < 20 {
            let _ = set_label(w, id, "Missile");
            if let Some(a) = w.actors.get_mut(id) {
                a.flags |= crate::class::FLAG_JUSTATTACKED;
            }
            return ActionResult::Continue;
        }
    } else if let Some(a) = w.actors.get_mut(id) {
        a.flags &= !crate::class::FLAG_JUSTATTACKED;
    }
    if rnd(w, "pr_firedchase") < 3 {
        let s = class_sound(w, id, "active");
        sound(w, id, &s);
    }
    ActionResult::Continue
}

fn fired_spawn_rock(w: &mut World, id: ActorId) {
    let cls = match rnd_range(w, "pr_firedemonrock", 0, 4) {
        0 => "FireDemonRock1",
        1 => "FireDemonRock2",
        2 => "FireDemonRock3",
        3 => "FireDemonRock4",
        _ => "FireDemonRock5",
    };
    let p = pos(w, id);
    let xo = f64::from(rnd(w, "pr_firedemonrock") - 128) / 16.0;
    let yo = f64::from(rnd(w, "pr_firedemonrock") - 128) / 16.0;
    let zo = f64::from(rnd(w, "pr_firedemonrock")) / 32.0;
    if let Some(mo) = spawn(w, cls, DVec3::new(p.x() + xo, p.y() + yo, p.z() + zo)) {
        set_target(w, mo, Some(id));
        let vx = f64::from(rnd(w, "pr_firedemonrock") - 128) / 64.0;
        let vy = f64::from(rnd(w, "pr_firedemonrock") - 128) / 64.0;
        let vz = f64::from(rnd(w, "pr_firedemonrock")) / 64.0;
        set_vel(w, mo, DVec3::new(vx, vy, vz));
        set_s1(w, mo, 2);
    }
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.scratch.insert("fdstrafe".into(), 0.0);
        a.flags &= !crate::class::FLAG_JUSTATTACKED;
    }
}

fn fly_buzz(w: &mut World, id: ActorId) -> ActionResult {
    let t = target(w, id);
    let ok = t.is_some_and(|t| health(w, t) <= 0);
    if !ok || rnd(w, "pr_fly") < 5 {
        let _ = set_label(w, id, "Spawn");
        return ActionResult::Continue;
    }
    let Some(t) = t else {
        return ActionResult::Continue;
    };
    let a = angle_to(w, id, t);
    set_angle(w, id, a);
    set_arg(w, id, 0, args_of(w, id)[0] + 1);
    let p = pos(w, id);
    let (dx, dy) = angle_vec(a, 6.0);
    if !crate::physics::try_move(w, id, DVec3::new(p.x() + dx, p.y() + dy, p.z())) {
        let _ = set_label(w, id, "Spawn");
        return ActionResult::Continue;
    }
    if args_of(w, id)[0] & 2 != 0 {
        let ax = f64::from(rnd(w, "pr_fly") - 128) / 512.0;
        let ay = f64::from(rnd(w, "pr_fly") - 128) / 512.0;
        let v = vel(w, id);
        set_vel(w, id, DVec3::new(v.x() + ax, v.y() + ay, v.z()));
    }
    let mut z = rnd(w, "pr_fly");
    if pos(w, t).z() + 5.0 < pos(w, id).z() && z > 150 {
        z = -z;
    }
    set_vel_z(w, id, f64::from(z) / 512.0);
    if rnd(w, "pr_fly") < 40 {
        let s = class_sound(w, id, "active");
        sound(w, id, &s);
    }
    ActionResult::Continue
}

fn korax_chase(w: &mut World, id: ActorId) -> ActionResult {
    let sh = spawn_health(w, id);
    if s2(w, id) == 0 && health(w, id) <= sh / 2 {
        if let Some(spot) = tid_actor(w, 248, None) {
            korax_teleport(w, id, spot);
        }
        let _ = call(w, id, "ACS_Execute", &["249", "0"]);
        set_s2(w, id, 1);
        return ActionResult::Continue;
    }
    if target(w, id).is_none() {
        return ActionResult::Continue;
    }
    if rnd(w, "pr_koraxchase") < 30 {
        let _ = set_label(w, id, "Missile");
    } else if rnd(w, "pr_koraxchase") < 30 {
        sound(w, id, "KoraxActive");
    }
    if health(w, id) < sh >> 1 && rnd(w, "pr_koraxchase") < 10 {
        let next = tid_actor(w, 249, tracer(w, id));
        set_tracer(w, id, next);
        if let Some(spot) = next {
            korax_teleport(w, id, spot);
        }
    }
    ActionResult::Continue
}

/// Next actor with `tid` after `after` (wrapping to the first).
fn tid_actor(w: &World, tid: i32, after: Option<ActorId>) -> Option<ActorId> {
    let list: Vec<ActorId> = w
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&o| {
            w.actors
                .get(o)
                .is_some_and(|a| a.tid == tid && !a.destroyed)
        })
        .collect();
    match after.and_then(|a| list.iter().position(|&o| o == a)) {
        Some(i) => list.get(i + 1).copied(),
        None => list.first().copied(),
    }
}

fn korax_teleport(w: &mut World, id: ActorId, spot: ActorId) {
    let old = pos(w, id);
    let sp = pos(w, spot);
    let (floor, _) = floor_ceil(w, spot);
    let a = angle(w, spot);
    crate::specials::spawn_teleport_fog_of(w, id, old, false);
    if let Some(x) = w.actors.get_mut(id) {
        x.pos = DVec3::new(sp.x(), sp.y(), floor);
        x.angle = Angle::from_degrees(a);
        x.vel = DVec3::ZERO;
    }
    crate::specials::spawn_teleport_fog_of(w, id, DVec3::new(sp.x(), sp.y(), floor), true);
}

fn korax_missile(w: &mut World, id: ActorId) -> ActionResult {
    const CHOICES: [&str; 6] = [
        "WraithFX1",
        "Demon1FX1",
        "Demon2FX1",
        "FireDemonMissile",
        "CentaurFX",
        "SerpentFX",
    ];
    const SOUNDS: [&str; 6] = [
        "WraithMissileFire",
        "DemonMissileFire",
        "DemonMissileFire",
        "FireDemonAttack",
        "CentaurLeaderAttack",
        "SerpentLeaderAttack",
    ];
    const EXT: [f64; 6] = [40.0, 55.0, 55.0, 40.0, 55.0, 55.0];
    const ARMZ: [f64; 6] = [108.0, 82.0, 54.0, 104.0, 86.0, 53.0];
    let Some(t) = target(w, id) else {
        return ActionResult::Continue;
    };
    let ty = rnd_range(w, "pr_koraxmissile", 0, 5) as usize;
    sound(w, id, "KoraxAttack");
    sound(w, id, SOUNDS[ty]);
    let base = angle(w, id);
    let fc = floorclip(w, id);
    for arm in 0..6 {
        let ang = base + if arm < 3 { -85.0 } else { 85.0 };
        let at = vec3_angle(w, id, EXT[arm], ang, ARMZ[arm] - fc);
        let Some(th) = spawn(w, CHOICES[ty], at) else {
            continue;
        };
        set_target(w, th, Some(id));
        let mut an =
            Angle::from_radians(dmath::atan2(pos(w, t).y() - at.y(), pos(w, t).x() - at.x()))
                .degrees();
        if has_flag(w, t, "SHADOW") {
            an += f64::from(rnd2(w, "pr_koraxmissile")) * (45.0 / 256.0);
        }
        set_angle(w, th, an);
        let sp = speed(w, th);
        let (vx, vy) = angle_vec(an, sp);
        let d = dist_by_speed(w, t, th, sp);
        let vz = (pos(w, t).z() - at.z() + 30.0) / d;
        set_vel(w, th, DVec3::new(vx, vy, vz));
    }
    ActionResult::Continue
}

fn lightning_clip(w: &mut World, id: ActorId) {
    let floor_hug = has_flag(w, id, "FLOORHUGGER");
    let ceil_hug = has_flag(w, id, "CEILINGHUGGER");
    let last = w.actors.get(id).and_then(|a| a.ext.last_enemy);
    let (floor, ceil) = floor_ceil(w, id);
    let mut tgt = None;
    if floor_hug {
        let Some(l) = last else {
            return;
        };
        set_z(w, id, floor);
        tgt = tracer(w, l);
    } else if ceil_hug {
        let h = height(w, id);
        set_z(w, id, ceil - h);
        tgt = tracer(w, id);
    }
    if floor_hug {
        let zig = rnd(w, "pr_lightningclip");
        let s = s1(w, id);
        let a = angle(w, id);
        let dir = if (zig > 128 && s < 2) || s < -2 {
            90.0
        } else {
            -90.0
        };
        thrust(w, id, a + dir, ZAGSPEED);
        if let Some(c) = last {
            thrust(w, c, a + dir, ZAGSPEED);
        }
        set_s1(w, id, if dir > 0.0 { s + 1 } else { s - 1 });
    }
    if let Some(t) = tgt {
        if health(w, t) <= 0 {
            crate::physics::explode_missile(w, id);
        } else {
            let a = angle_to(w, id, t);
            set_angle(w, id, a);
            let sp = speed(w, id) / 2.0;
            let vz = vel(w, id).z();
            let (vx, vy) = angle_vec(a, sp);
            set_vel(w, id, DVec3::new(vx, vy, vz));
        }
    }
}

fn lightning_zap(w: &mut World, id: ActorId) {
    let cls = missile_name_or(w, id, "LightningZap");
    lightning_clip(w, id);
    let hp = health(w, id) - 8;
    if let Some(a) = w.actors.get_mut(id) {
        a.health = hp;
    }
    if hp <= 0 {
        let _ = set_label(w, id, "Death");
        return;
    }
    let r = radius(w, id);
    let dx = f64::from(rnd(w, "pr_lightningzap") - 128) * r / 256.0;
    let dy = f64::from(rnd(w, "pr_lightningzap") - 128) * r / 256.0;
    let fh = has_flag(w, id, "FLOORHUGGER");
    let dz = if fh { 10.0 } else { -10.0 };
    let p = pos(w, id);
    let v = vel(w, id);
    let t = target(w, id);
    if let Some(mo) = spawn(w, &cls, DVec3::new(p.x() + dx, p.y() + dy, p.z() + dz)) {
        if let Some(x) = w.actors.get_mut(mo) {
            x.ext.last_enemy = Some(id);
            x.vel = DVec3::new(v.x(), v.y(), if fh { 20.0 } else { -20.0 });
            x.target = t;
        }
    }
    if fh && rnd(w, "pr_lightningzap") < 160 {
        let s = class_sound(w, id, "active");
        sound(w, id, &s);
    }
}

fn serpent_check_for_attack(w: &mut World, id: ActorId) {
    if target(w, id).is_none() {
        return;
    }
    let has_missile = w
        .actors
        .get(id)
        .is_some_and(|a| w.states.find_label(a.class, "Missile").is_some());
    if has_missile && !in_melee_range(w, id) {
        let _ = set_label(w, id, "Attack");
        return;
    }
    // `CheckMeleeRange2`: in range but not quite in reach.
    let near2 = target(w, id).is_some_and(|t| {
        let d = dist2d(w, id, t);
        d >= MELEE_RANGE && d < 2.0 * MELEE_RANGE && check_sight(w, id, t)
    });
    if near2 {
        let _ = set_label(w, id, "Walk");
    } else if in_melee_range(w, id) {
        let label = if rnd(w, "pr_serpentattack") < 32 {
            "Walk"
        } else {
            "Attack"
        };
        let _ = set_label(w, id, label);
    }
}

fn shed_shard(w: &mut World, id: ActorId) {
    let dir = s1(w, id);
    let count = s2(w, id);
    if count <= 0 {
        return;
    }
    set_s2(w, id, 0);
    let count = count - 1;
    let (z, a, vz, t) = (pos(w, id).z(), angle(w, id), vel(w, id).z(), target(w, id));
    let arg0 = if count == 3 { 2 } else { 0 };
    let make = |w: &mut World, dz: f64, da: f64, sp: f64, s1v: i32| {
        if let Some(mo) =
            spawn_missile_angle_z_speed(w, id, z + dz, "FrostMissile", a + da, 0.0, Some(sp))
        {
            set_s1(w, mo, s1v);
            set_s2(w, mo, count);
            set_vel_z(w, mo, vz);
            set_arg(w, mo, 0, arg0);
            set_target(w, mo, t);
        }
    };
    if dir & SHARDSPAWN_LEFT != 0 {
        make(w, 0.0, 5.0, 20.0 + 2.0 * f64::from(count), SHARDSPAWN_LEFT);
    }
    if dir & SHARDSPAWN_RIGHT != 0 {
        make(
            w,
            0.0,
            -5.0,
            20.0 + 2.0 * f64::from(count),
            SHARDSPAWN_RIGHT,
        );
    }
    let lr = SHARDSPAWN_LEFT | SHARDSPAWN_RIGHT;
    if dir & SHARDSPAWN_UP != 0 {
        let v = if count & 1 != 0 {
            SHARDSPAWN_UP | lr
        } else {
            SHARDSPAWN_UP
        };
        make(w, 8.0, 0.0, 15.0 + 2.0 * f64::from(count), v);
    }
    if dir & SHARDSPAWN_DOWN != 0 {
        let v = if count & 1 != 0 {
            SHARDSPAWN_DOWN | lr
        } else {
            SHARDSPAWN_DOWN
        };
        make(w, -4.0, 0.0, 15.0 + 2.0 * f64::from(count), v);
    }
}

fn summon(w: &mut World, id: ActorId) -> ActionResult {
    let p = pos(w, id);
    let Some(mo) = spawn(w, "MinotaurFriend", p) else {
        return ActionResult::Continue;
    };
    let fits = w
        .actors
        .get(mo)
        .is_some_and(|a| crate::physics::check_position(w, a, a.pos, Some(mo)).ok);
    let master = tracer(w, id);
    if !fits || master.is_none() {
        destroy(w, mo);
        if let Some(arti) = spawn(w, "ArtiDarkServant", p) {
            set_dropped(w, arti);
        }
        return ActionResult::Continue;
    }
    let now = f64::from(w.level.maptime);
    if let Some(a) = w.actors.get_mut(mo) {
        a.ext.scratch.insert("starttime".into(), now);
    }
    let master = master.filter(|&m| health(w, m) > 0);
    set_tracer(w, mo, master);
    if let Some(m) = master {
        let _ = give(w, m, "PowerMinotaur", 1);
        set_flag(w, mo, "FRIENDLY", true);
    }
    let _ = spawn(w, "MinotaurSmoke", p);
    let s = class_sound(w, mo, "active");
    sound(w, mo, &s);
    ActionResult::Continue
}

fn stop_balls(w: &mut World, id: ActorId) {
    let chance = rnd(w, "pr_heresiarch");
    set_arg(w, id, 3, SORC_STOPPING);
    set_arg(w, id, 1, 0);
    let a0 = args_of(w, id)[0];
    let ball = if a0 <= 0 && chance < 200 {
        "SorcBall2"
    } else if health(w, id) < spawn_health(w, id) >> 1 && chance < 200 {
        "SorcBall3"
    } else {
        "SorcBall1"
    };
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.scratch.insert(
            "stopball".into(),
            f64::from(
                ["SorcBall1", "SorcBall2", "SorcBall3"]
                    .iter()
                    .position(|b| *b == ball)
                    .unwrap_or(0) as i32,
            ),
        );
    }
}

fn ball_index(w: &World, id: ActorId) -> i32 {
    match class_name(w, id).to_ascii_lowercase().as_str() {
        "sorcball2" => 1,
        "sorcball3" => 2,
        _ => 0,
    }
}

fn sorc_ball_orbit(w: &mut World, id: ActorId) -> ActionResult {
    let Some(parent) = target(w, id).filter(|&p| health(w, p) > 0) else {
        let _ = set_label(w, id, "Pain");
        return ActionResult::Continue;
    };
    let mode = args_of(w, parent)[3];
    let dist = radius(w, parent) - radius(w, id) * 2.0;
    let prev = w.actors.get(id).map_or(0.0, |a| a.ext.specialf.0);
    let offset = w.actors.get(id).map_or(0.0, |a| a.ext.specialf.1);
    let base = w.actors.get(parent).map_or(0.0, |a| a.ext.specialf.0);
    let cur = base + offset;
    set_angle(w, id, cur);
    let me = ball_index(w, id);
    let stop = w
        .actors
        .get(parent)
        .and_then(|a| a.ext.scratch.get("stopball").copied())
        .unwrap_or(0.0) as i32;
    let update = |w: &mut World| {
        if me == 0 {
            let speed = f64::from(args_of(w, parent)[4]);
            if let Some(a) = w.actors.get_mut(parent) {
                a.ext.specialf.0 += speed;
            }
        }
    };
    match mode {
        SORC_NORMAL => update(w),
        SORC_DECELERATE => {
            let _ = dispatch(w, id, "A_DecelBalls", &[]);
            update(w);
        }
        SORC_ACCELERATE => {
            let _ = dispatch(w, id, "A_AccelBalls", &[]);
            update(w);
        }
        SORC_STOPPING => {
            let pa = angle(w, parent);
            if stop == me
                && args_of(w, parent)[1] > SORCBALL_SPEED_ROTATIONS
                && delta_angle(cur, pa).abs() < 42.1875
            {
                set_arg(w, parent, 3, SORC_FIRESPELL);
                set_arg(w, parent, 4, 0);
                if let Some(a) = w.actors.get_mut(parent) {
                    a.ext.specialf.0 = pa - offset;
                }
            } else {
                update(w);
            }
        }
        SORC_FIRESPELL => {
            if stop == me {
                if health(w, parent) > 0 {
                    let _ = set_label(w, parent, "Attack1");
                }
                do_fire_spell(w, id, parent, me);
            }
        }
        SORC_FIRING_SPELL => {
            if stop == me {
                let v = s2(w, id);
                set_s2(w, id, v - 1);
                if v <= 0 {
                    set_arg(w, parent, 3, SORC_STOPPED);
                    if health(w, parent) > 0 {
                        let _ = set_label(w, parent, "Attack2");
                    }
                } else {
                    sorc_offense2(w, id);
                }
            }
        }
        _ => {}
    }
    let wrap = |a: f64| a.rem_euclid(360.0);
    if wrap(cur) < wrap(prev) && args_of(w, parent)[4] == SORCBALL_TERMINAL_SPEED {
        set_arg(w, parent, 1, args_of(w, parent)[1] + 1);
        sound(w, id, "SorcererBallWoosh");
    }
    if let Some(a) = w.actors.get_mut(id) {
        a.ext.specialf.0 = cur;
    }
    let (ph, pfc) = (height(w, parent), floorclip(w, parent));
    let at = vec3_angle(w, parent, dist, cur, -pfc + ph);
    if let Some(a) = w.actors.get_mut(id) {
        a.pos = at;
    }
    ActionResult::Continue
}

fn cast_spell_base(w: &mut World, parent: ActorId) {
    sound(w, parent, "SorcererSpellCast");
    if health(w, parent) > 0 {
        let _ = set_label(w, parent, "Attack2");
    }
}

fn do_fire_spell(w: &mut World, id: ActorId, parent: ActorId, me: i32) {
    if me == 0 && rnd(w, "pr_heresiarch") < 200 {
        sound(w, parent, "SorcererSpellCast");
        set_s2(w, id, SORCFX4_RAPIDFIRE_TIME);
        set_arg(w, id, 4, 128);
        set_arg(w, parent, 3, SORC_FIRING_SPELL);
        return;
    }
    cast_spell_base(w, parent);
    let a = angle(w, id);
    match me {
        0 => {
            let pt = target(w, parent);
            for ang in [a + 70.0, a - 70.0] {
                if let Some(mo) = spawn_missile_angle(w, parent, "SorcFX1", ang, 0.0) {
                    set_target(w, mo, Some(parent));
                    set_tracer(w, mo, pt);
                    set_arg(w, mo, 4, BOUNCE_TIME_UNIT);
                    set_arg(w, mo, 3, 15);
                }
            }
        }
        1 => {
            let p = pos(w, id);
            let fc = floorclip(w, parent);
            let mo = spawn(
                w,
                "SorcFX2",
                DVec3::new(p.x(), p.y(), p.z() + fc + SORC_DEFENSE_HEIGHT),
            );
            set_flag(w, parent, "REFLECTIVE", true);
            set_flag(w, parent, "INVULNERABLE", true);
            set_arg(w, parent, 0, SORC_DEFENSE_TIME);
            if let Some(m) = mo {
                set_target(w, m, Some(parent));
            }
        }
        _ => {
            let (mut a1, a2) = (a - 45.0, a + 45.0);
            if health(w, id) < spawn_health(w, id) / 3 {
                for ang in [a1, a2] {
                    if let Some(mo) = spawn_missile_angle(w, parent, "SorcFX3", ang, 4.0) {
                        set_target(w, mo, Some(parent));
                    }
                }
            } else {
                if rnd(w, "pr_heresiarch") < 128 {
                    a1 = a2;
                }
                if let Some(mo) = spawn_missile_angle(w, parent, "SorcFX3", a1, 4.0) {
                    set_target(w, mo, Some(parent));
                }
            }
        }
    }
    set_arg(w, parent, 3, SORC_STOPPED);
}

fn sorc_offense2(w: &mut World, id: ActorId) {
    let Some(parent) = target(w, id) else {
        return;
    };
    let Some(dest) = target(w, parent) else {
        return;
    };
    let idx = args_of(w, id)[4];
    set_arg(w, id, 4, (idx + 15) & 255);
    let delta = (f64::from(idx) * (360.0 / 256.0)).to_radians().sin() * SORCFX4_SPREAD_ANGLE;
    let a = angle(w, id) + delta;
    if let Some(mo) = spawn_missile_angle(w, parent, "SorcFX4", a, 0.0) {
        set_s2(w, mo, 35 * 5 / 2);
        let sp = speed(w, mo);
        let d = dist_by_speed(w, dest, mo, sp);
        let vz = (pos(w, dest).z() - pos(w, mo).z()) / d;
        set_vel_z(w, mo, vz);
    }
}

fn sorc_fx2_orbit(w: &mut World, id: ActorId) -> ActionResult {
    let Some(parent) = target(w, id) else {
        destroy(w, id);
        return ActionResult::Continue;
    };
    let dist = radius(w, parent);
    if health(w, parent) <= 0 || args_of(w, parent)[0] == 0 {
        let _ = set_label(w, id, "Death");
        set_arg(w, parent, 0, 0);
        set_flag(w, parent, "REFLECTIVE", false);
        set_flag(w, parent, "INVULNERABLE", false);
    }
    let ccw = args_of(w, id)[0] != 0;
    if ccw {
        let pa0 = args_of(w, parent)[0];
        set_arg(w, parent, 0, pa0 - 1);
        if pa0 <= 0 {
            let _ = set_label(w, id, "Death");
            set_arg(w, parent, 0, 0);
            set_flag(w, parent, "REFLECTIVE", false);
        }
    }
    let ang = w.actors.get_mut(id).map_or(0.0, |a| {
        a.ext.specialf.0 += if ccw { 10.0 } else { -10.0 };
        a.ext.specialf.0
    });
    set_angle(w, id, ang);
    let fc = floorclip(w, parent);
    let mut at = vec3_angle(w, parent, dist, ang, fc + SORC_DEFENSE_HEIGHT);
    let bob = if ccw {
        15.0 * ang.to_radians().cos()
    } else {
        20.0 * ang.to_radians().sin()
    };
    at = DVec3::new(at.x(), at.y(), at.z() + bob);
    let p = pos(w, id);
    let _ = spawn(w, "SorcFX2T1", p);
    if let Some(a) = w.actors.get_mut(id) {
        a.pos = at;
    }
    ActionResult::Continue
}

#[allow(dead_code)]
fn unused(w: &mut World, id: ActorId) {
    radial_missiles(w, id, "", 0, 0.0, None, 0.0);
}
