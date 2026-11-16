//! Sector type effects (`P_PlayerInSpecialSector` / load-time init subset).

use crate::actor::ActorId;
use crate::damage::damage_mobj;
use crate::world::World;

/// Doom sector type: secret.
const ST_SECRET: i16 = 9;
/// Damage 5 / 10% (HELLSLIME).
const ST_DAMAGE_5: i16 = 5;
/// Damage 10 / 20% (NUKAGE).
const ST_DAMAGE_10: i16 = 7;
/// Damage 20 / 5% (super hellslime).
const ST_DAMAGE_20: i16 = 16;
/// Damage 20 and end level when health ≤ 10 (type 11).
const ST_DAMAGE_END: i16 = 11;

/// Doom sector type: light blink random / strobe.
const ST_LIGHT_BLINK: i16 = 1;
const ST_LIGHT_STROBE_FAST: i16 = 2;
const ST_LIGHT_STROBE_SLOW: i16 = 3;
const ST_LIGHT_STROBE_HURT: i16 = 4;
const ST_LIGHT_GLOW: i16 = 8;
/// Door closes after 30 seconds (`P_SpawnDoorCloseIn30`).
const ST_DOOR_CLOSE_IN_30: i16 = 10;
/// Synchronized slow strobe (`P_SpawnStrobeFlash` sync).
const ST_LIGHT_STROBE_SLOW_SYNC: i16 = 12;
/// Synchronized fast strobe.
const ST_LIGHT_STROBE_FAST_SYNC: i16 = 13;
/// Door raises after 5 minutes (`P_SpawnDoorRaiseIn5Mins`).
const ST_DOOR_RAISE_IN_5: i16 = 14;
const ST_LIGHT_FLICKER: i16 = 17;

/// Boom generalized sector bits (`09` §2.3 / Boom `sector_t`).
const BOOM_DAMAGE_MASK: i16 = 0x60;
const BOOM_DAMAGE_5: i16 = 0x20;
const BOOM_DAMAGE_10: i16 = 0x40;
const BOOM_DAMAGE_20: i16 = 0x60;
const BOOM_SECRET: i16 = 0x80;
const BOOM_FRICTION: i16 = 0x100;
/// Boom push/wind enable (`0x200`); wind vectors come from linedef specials.
pub const BOOM_PUSH: i16 = 0x200;
/// MBF21: kill grounded players.
const MBF21_KILL_PLAYERS: i16 = 0x400;
/// MBF21: kill grounded monsters.
const MBF21_KILL_MONSTERS: i16 = 0x800;

/// Tics per second (Doom).
const TICRATE: i32 = 35;

/// Boom sticky floor friction when the friction bit is set (no FRICTION linedef).
const BOOM_FRICTION_DEFAULT: f64 = 0.85;

/// Doom `STROBEBRIGHT` — tics at bright level.
const STROBE_BRIGHT: i32 = 5;
/// Doom `FASTDARK`.
const STROBE_FAST_DARK: i32 = 15;
/// Doom `SLOWDARK`.
const STROBE_SLOW_DARK: i32 = 35;

/// Doom `P_FindMinSurroundingLight`.
fn find_min_surrounding_light(world: &World, sector: usize, max: i16) -> i16 {
    let mut min = max;
    for line in &world.map.lines {
        let neighbor = match (line.front_sector, line.back_sector) {
            (Some(f), Some(b)) if f.raw() as usize == sector => Some(b.raw() as usize),
            (Some(f), Some(b)) if b.raw() as usize == sector => Some(f.raw() as usize),
            _ => None,
        };
        let Some(ni) = neighbor else {
            continue;
        };
        let light = world
            .sectors
            .get(ni)
            .map(|s| s.light)
            .or_else(|| world.map.sectors.get(ni).map(|s| s.light))
            .unwrap_or(max);
        if light < min {
            min = light;
        }
    }
    min
}

/// Spawn Doom sector light thinkers (`P_SpawnLightFlash` / strobe / fire / glow).
///
/// Must run after [`World::set_vanilla_demo_mode`] so spawn-time `P_Random` hits the
/// classic table (Chocolate DEMO1 burns these before tic 1).
pub fn spawn_sector_light_thinkers(world: &mut World) {
    world.flickers.clear();
    world.strobes.clear();
    world.glows.clear();
    let specs: Vec<(usize, i16, i16)> = world
        .map
        .sectors
        .iter()
        .enumerate()
        .map(|(i, sec)| (i, sec.special, sec.light))
        .collect();
    for (i, special, map_light) in specs {
        // Vanilla matches on the raw special; Boom low 5 bits carry the type.
        let light_type = if special > 0 && special < 32 {
            special
        } else {
            special & 0x1f
        };
        let max_light = world.sectors.get(i).map(|s| s.light).unwrap_or(map_light);
        let sid = room_core::SectorId::from_raw(i as u32);
        match light_type {
            ST_LIGHT_BLINK => {
                // Doom `P_SpawnLightFlash`.
                let min_light = find_min_surrounding_light(world, i, max_light);
                let max_time = 64;
                let count = i32::from(world.p_random("pr_lights") & (max_time as u8)) + 1;
                world.flickers.push(crate::specials::LightFlicker {
                    sector: sid,
                    max_light,
                    min_light,
                    count,
                    fire: false,
                    bright: true,
                    min_time: 7,
                    max_time,
                });
            }
            ST_LIGHT_FLICKER => {
                // Doom `P_SpawnFireFlicker` — no spawn `P_Random`.
                let min_light = find_min_surrounding_light(world, i, max_light).saturating_add(16);
                world.flickers.push(crate::specials::LightFlicker {
                    sector: sid,
                    max_light,
                    min_light,
                    count: 4,
                    fire: true,
                    bright: true,
                    min_time: 7,
                    max_time: 64,
                });
            }
            ST_LIGHT_STROBE_FAST | ST_LIGHT_STROBE_HURT => {
                spawn_strobe(world, sid, i, max_light, STROBE_FAST_DARK, false);
            }
            ST_LIGHT_STROBE_SLOW => {
                spawn_strobe(world, sid, i, max_light, STROBE_SLOW_DARK, false);
            }
            ST_LIGHT_STROBE_FAST_SYNC => {
                spawn_strobe(world, sid, i, max_light, STROBE_FAST_DARK, true);
            }
            ST_LIGHT_STROBE_SLOW_SYNC => {
                spawn_strobe(world, sid, i, max_light, STROBE_SLOW_DARK, true);
            }
            ST_LIGHT_GLOW => {
                // Doom `P_SpawnGlowingLight`: GLOWSPEED toward min surrounding.
                let lower = find_min_surrounding_light(world, i, max_light);
                let tics = 20;
                let current = f64::from(max_light);
                let dest = f64::from(lower);
                let step = if tics > 0 {
                    (dest - current) / f64::from(tics)
                } else {
                    0.0
                };
                world.glows.push(crate::specials::LightGlow {
                    sector: sid,
                    upper: max_light,
                    lower,
                    tics_max: tics,
                    tics,
                    dir: -1,
                    current,
                    step,
                });
            }
            _ => {}
        }
    }
}

fn spawn_strobe(
    world: &mut World,
    sid: room_core::SectorId,
    sector_idx: usize,
    max_light: i16,
    dark_time: i32,
    in_sync: bool,
) {
    let mut min_light = find_min_surrounding_light(world, sector_idx, max_light);
    if min_light == max_light {
        min_light = 0;
    }
    let count = if in_sync {
        1
    } else {
        i32::from(world.p_random("pr_lights") & 7) + 1
    };
    world.strobes.push(crate::specials::LightStrobe {
        sector: sid,
        max_light,
        min_light,
        count,
        bright_time: STROBE_BRIGHT,
        dark_time,
        bright: true,
    });
}

/// Count secret sectors and spawn light / timed-door thinkers at map start.
pub fn init_sector_specials(world: &mut World) {
    let mut secrets = 0u32;
    for (i, sec) in world.map.sectors.iter().enumerate() {
        if sec.special == ST_SECRET || (sec.special & BOOM_SECRET) != 0 {
            secrets += 1;
        }
        // Boom friction bit: enable sticky floor (linedef FRICTION may override later).
        if (sec.special & BOOM_FRICTION) != 0 {
            if let Some(dyn_sec) = world.sectors.get_mut(i) {
                dyn_sec.friction = BOOM_FRICTION_DEFAULT;
            }
        }
        if (sec.special & BOOM_PUSH) != 0 {
            if let Some(dyn_sec) = world.sectors.get_mut(i) {
                dyn_sec.boom_push = true;
            }
        }
        let light_type = if sec.special > 0 && sec.special < 32 {
            sec.special
        } else {
            sec.special & 0x1f
        };
        let sid = room_core::SectorId::from_raw(i as u32);
        match light_type {
            ST_DOOR_CLOSE_IN_30 => {
                let Some(sec) = world.sectors.get(i) else {
                    continue;
                };
                world.doors.push(crate::specials::DoorMover {
                    sector: sid,
                    direction: 0,
                    speed: 16.0,
                    top_z: sec.ceiling_z,
                    bottom_z: sec.floor_z,
                    wait: 30 * TICRATE,
                    wait_max: 0,
                    close_wait_open: false,
                    wait_then_raise: false,
                });
            }
            ST_DOOR_RAISE_IN_5 => {
                let Some(sec) = world.sectors.get(i) else {
                    continue;
                };
                let top = sec.floor_z + 120.0;
                world.doors.push(crate::specials::DoorMover {
                    sector: sid,
                    direction: 0,
                    speed: 16.0,
                    top_z: top.max(sec.ceiling_z),
                    bottom_z: sec.floor_z,
                    wait: 5 * 60 * TICRATE,
                    wait_max: -1,          // stay open
                    close_wait_open: true, // after wait → raise
                    wait_then_raise: false,
                });
            }
            _ => {}
        }
    }
    world.level.total_secrets = secrets;
}

/// Per-tic player-in-sector effects.
pub fn player_in_special_sector(world: &mut World, mo: ActorId) {
    let Some(actor) = world.actors.get(mo).cloned() else {
        return;
    };
    // Bootstrap: use sector 0 when undetermined — prefer front sector of nearest line.
    let sid = sector_at(world, actor.pos.x(), actor.pos.y()).unwrap_or(0);
    let Some(sec) = world.map.sectors.get(sid) else {
        return;
    };
    let special = world
        .sector_specials
        .get(&(sid as u32))
        .copied()
        .unwrap_or(sec.special);
    if special == ST_SECRET || (special & BOOM_SECRET) != 0 {
        // Clear secret bit by marking discovered via level counter only once per sector.
        if !world.secret_found.get(sid).copied().unwrap_or(false) {
            if sid >= world.secret_found.len() {
                world.secret_found.resize(sid + 1, false);
            }
            world.secret_found[sid] = true;
            world.level.found_secrets = world.level.found_secrets.saturating_add(1);
            world.credit_player_stat(Some(mo), 2);
            world.push_message_kind(crate::world::MessageKind::Secret, "A secret is revealed!");
            world.push_view_debug("secret");
            if let Some(a) = world.actors.get(mo) {
                world.play_sound("misc/secret", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
            }
            // Boom: clear secret bit from the runtime special (keep damage/friction bits).
            if (special & BOOM_SECRET) != 0 {
                let cleared = special & !BOOM_SECRET;
                world.sector_specials.insert(sid as u32, cleared);
            } else if special == ST_SECRET {
                world.sector_specials.insert(sid as u32, 0);
            }
        }
    }
    // MBF21 kill-grounded-players: lethal damage each tic while in the sector.
    // COMPAT(compat_nombf21)
    if (special & MBF21_KILL_PLAYERS) != 0 && world.compat.mbf21_enabled() {
        let is_player = world.players.iter().flatten().any(|p| p.mo == Some(mo));
        let protected = world
            .players
            .iter()
            .flatten()
            .any(|p| p.mo == Some(mo) && (p.invulnerable || p.god));
        if is_player && !protected {
            damage_mobj(world, mo, None, None, 10_000);
        }
    }
    let boom_dmg = match special & BOOM_DAMAGE_MASK {
        BOOM_DAMAGE_5 => 5,
        BOOM_DAMAGE_10 => 10,
        BOOM_DAMAGE_20 => 20,
        _ => 0,
    };
    let classic_dmg = match special & 0x1f {
        ST_DAMAGE_5 => 5,
        ST_DAMAGE_10 => 10,
        ST_DAMAGE_20 | ST_DAMAGE_END | ST_LIGHT_STROBE_HURT => 20,
        _ => 0,
    };
    // Prefer Boom damage bits when set; else Doom type numbers.
    let dmg = if boom_dmg > 0 { boom_dmg } else { classic_dmg };
    let invul = world
        .players
        .iter()
        .flatten()
        .any(|p| p.mo == Some(mo) && (p.invulnerable || p.god));
    let radsuit = world
        .players
        .iter()
        .flatten()
        .any(|p| p.mo == Some(mo) && p.radsuit);
    if dmg > 0 && sector_damage_applies(world, mo, invul) && world.level.time % 32 == 0 {
        if sector_damage_leaks(world, sid, radsuit) {
            damage_mobj(world, mo, None, None, dmg);
            if (special & 0x1f) == ST_DAMAGE_END {
                if let Some(a) = world.actors.get(mo) {
                    if a.health <= 10 {
                        world.level.exiting = true;
                        if !cfg!(test) {
                            eprintln!("room: level exit: exit sector (tic {})", world.level.time);
                        }
                    }
                }
            }
        }
    }
    // UDMF `damageamount` / ACS `SetSectorDamage` — interval from `damageinterval`.
    let ext = world.map.udmf_user.sector_ext(sid);
    if let Some(dyn_s) = world.sectors.get(sid) {
        let amt = dyn_s.damage_amount;
        let iv = dyn_s.damage_interval.max(1) as u32;
        // ZDoom: no floor damage while airborne unless `harminair`.
        let airborne = actor.pos.z() > dyn_s.floor_z + 0.5;
        let grounded = !airborne || ext & room_map::udmf::ext::SECTOR_HARMINAIR != 0;
        if amt > 0 && grounded && ext & room_map::udmf::ext::SECTOR_DAMAGEHAZARD != 0 {
            // Strife hazard: standing here fills a meter; damage comes from
            // `tick_hazard` while it drains (a radsuit only slows the fill).
            if sector_damage_applies(world, mo, invul)
                && let Some(p) = world
                    .players
                    .iter_mut()
                    .flatten()
                    .find(|p| p.mo == Some(mo))
            {
                let fill = if radsuit { 1 } else { amt.max(1) };
                p.hazard_count = (p.hazard_count + fill).min(60 * 35);
            }
        } else if amt > 0
            && grounded
            && sector_damage_applies(world, mo, invul)
            && world.level.time % iv == 0
        {
            if sector_damage_leaks(world, sid, radsuit) {
                damage_mobj(world, mo, None, None, amt);
            }
        }
    }
    tick_hazard(world, mo);
}

/// Strife `hazardcount`: once the meter passes 16 s, hurt every 32 tics.
fn tick_hazard(world: &mut World, mo: ActorId) {
    let time = world.level.time;
    let Some(p) = world
        .players
        .iter_mut()
        .flatten()
        .find(|p| p.mo == Some(mo))
    else {
        return;
    };
    if p.hazard_count <= 0 {
        return;
    }
    p.hazard_count -= 1;
    // ZDoom `P_PlayerThink`: a flat 5 damage per hit once past 16 s.
    if p.hazard_count > 16 * 35 && time % 32 == 0 {
        damage_mobj(world, mo, None, None, 5);
    }
}

/// `+NOSECTORDAMAGE` skips floor damage. `+FORCESECTORDAMAGE` applies it even
/// when the actor's player is invulnerable (UZ1).
fn sector_damage_applies(world: &World, mo: ActorId, invul: bool) -> bool {
    let Some(actor) = world.actors.get(mo) else {
        return false;
    };
    let force = actor.extra_flags.get_name("FORCESECTORDAMAGE");
    if actor.extra_flags.get_name("NOSECTORDAMAGE") && !force {
        return false;
    }
    force || !invul
}

/// Radsuit blocks sector damage unless `leakiness`/256 succeeds (`P_Random`).
fn sector_damage_leaks(world: &mut World, sid: usize, radsuit: bool) -> bool {
    if !radsuit {
        return true;
    }
    let leak = world
        .sectors
        .get(sid)
        .map(|s| s.leakiness.clamp(0, 256))
        .unwrap_or(0);
    if leak <= 0 {
        return false;
    }
    i32::from(world.p_random("pr_leakiness")) < leak
}

/// Per-tic MBF21 kill-monsters sector bit (grounded `COUNTKILL` actors).
pub fn monsters_in_special_sectors(world: &mut World) {
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        let Some(a) = world.actors.get(id).cloned() else {
            continue;
        };
        if a.destroyed || a.health <= 0 {
            continue;
        }
        if (a.flags & crate::class::FLAG_COUNTKILL) == 0 {
            continue;
        }
        if world.players.iter().flatten().any(|p| p.mo == Some(id)) {
            continue;
        }
        let Some(sid) = sector_at(world, a.pos.x(), a.pos.y()) else {
            continue;
        };
        let special = world
            .sector_specials
            .get(&(sid as u32))
            .copied()
            .or_else(|| world.map.sectors.get(sid).map(|s| s.special))
            .unwrap_or(0);
        // COMPAT(compat_nombf21)
        if (special & MBF21_KILL_MONSTERS) != 0 && world.compat.mbf21_enabled() {
            damage_mobj(world, id, None, None, 10_000);
        }
        // UDMF `hurtmonsters`: the sector's damage also hits monsters.
        let ext = world.map.udmf_user.sector_ext(sid);
        if ext & room_map::udmf::ext::SECTOR_HURTMONSTERS != 0
            && let Some(dyn_s) = world.sectors.get(sid)
        {
            let amt = dyn_s.damage_amount;
            let iv = dyn_s.damage_interval.max(1) as u32;
            let airborne = a.pos.z() > dyn_s.floor_z + 0.5;
            if amt > 0
                && (!airborne || ext & room_map::udmf::ext::SECTOR_HARMINAIR != 0)
                && world.level.time % iv == 0
                && sector_damage_applies(world, id, false)
            {
                damage_mobj(world, id, None, None, amt);
            }
        }
    }
}

fn sector_at(world: &World, x: f64, y: f64) -> Option<usize> {
    room_nodebuild::point_in_sector(&world.map, x, y).map(|id| id.raw() as usize)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::World;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn sync_strobe_sectors_share_phase() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 192.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = ST_LIGHT_STROBE_FAST_SYNC;
        map.sectors[1].special = ST_LIGHT_STROBE_FAST_SYNC;
        apply_nodes(&mut map);
        let w = World::from_map(map).unwrap();
        assert_eq!(w.strobes.len(), 2);
        assert_eq!(w.strobes[0].count, w.strobes[1].count);
        assert_eq!(w.strobes[0].count, 1);
        assert_eq!(w.strobes[0].bright_time, STROBE_BRIGHT);
        assert_eq!(w.strobes[0].dark_time, STROBE_FAST_DARK);
    }

    #[test]
    fn nosector_damage_and_force_flags() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert!(sector_damage_applies(&w, mo, false));
        assert!(!sector_damage_applies(&w, mo, true));
        w.actors
            .get_mut(mo)
            .unwrap()
            .extra_flags
            .set_name("NOSECTORDAMAGE", true);
        assert!(!sector_damage_applies(&w, mo, false));
        w.actors
            .get_mut(mo)
            .unwrap()
            .extra_flags
            .set_name("FORCESECTORDAMAGE", true);
        assert!(sector_damage_applies(&w, mo, true));
    }

    #[test]
    fn light_blink_sector_spawns_flicker() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = ST_LIGHT_BLINK;
        apply_nodes(&mut map);
        let w = World::from_map(map).unwrap();
        assert_eq!(w.flickers.len(), 1);
        assert!(!w.flickers[0].fire);
        assert_eq!(w.flickers[0].sector.raw(), 0);
    }

    #[test]
    fn light_glow_sector_spawns_glow() {
        // Two adjacent rooms so min surrounding light < sector light (glow amplitude).
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .room("b", rect(64.0, 0.0, 128.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = ST_LIGHT_GLOW;
        map.sectors[0].light = 200;
        map.sectors[1].light = 40;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.glows.len(), 1);
        let before = w.sectors[0].light;
        for _ in 0..20 {
            crate::specials::tick_glows(&mut w);
        }
        assert_ne!(w.sectors[0].light, before);
    }

    #[test]
    fn door_close_in_30_spawns_waiting_door() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = ST_DOOR_CLOSE_IN_30;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.doors.len(), 1);
        assert_eq!(w.doors[0].direction, 0);
        assert_eq!(w.doors[0].wait, 30 * TICRATE);
        let open_z = w.sectors[0].ceiling_z;
        // Fast-forward the wait, then one tic to start closing.
        w.doors[0].wait = 1;
        crate::specials::tick_doors(&mut w);
        assert_eq!(w.doors[0].direction, -1);
        crate::specials::tick_doors(&mut w);
        assert!(w.sectors[0].ceiling_z < open_z);
    }

    #[test]
    fn door_raise_in_5_opens_after_wait() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 24.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = ST_DOOR_RAISE_IN_5;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.doors.len(), 1);
        assert!(w.doors[0].close_wait_open);
        assert_eq!(w.doors[0].wait_max, -1);
        w.doors[0].wait = 1;
        crate::specials::tick_doors(&mut w);
        assert_eq!(w.doors[0].direction, 1);
    }

    #[test]
    fn strobe_hurt_spawns_strobe_and_damages() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
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
        map.sectors[0].special = ST_LIGHT_STROBE_HURT;
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        assert_eq!(w.strobes.len(), 1);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hp0 = w.actors.get(mo).unwrap().health;
        w.level.time = 32;
        player_in_special_sector(&mut w, mo);
        assert_eq!(w.actors.get(mo).unwrap().health, hp0 - 20);
    }

    #[test]
    fn fire_flicker_changes_light() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .room("b", rect(64.0, 0.0, 128.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = ST_LIGHT_FLICKER;
        map.sectors[0].light = 200;
        map.sectors[1].light = 40;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.flickers.len(), 1);
        assert!(w.flickers[0].fire);
        let start = w.sectors[0].light;
        for _ in 0..16 {
            crate::specials::tick_flickers(&mut w);
        }
        // Deterministic RNG should have dimmed at least once from max.
        assert!(w.sectors[0].light <= start);
    }

    #[test]
    fn secret_sector_counts_on_init_and_find() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = ST_SECRET;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        init_sector_specials(&mut w);
        assert_eq!(w.level.total_secrets, 1);
        assert_eq!(w.level.found_secrets, 0);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let _ = w.drain_sound_events();
        player_in_special_sector(&mut w, mo);
        assert_eq!(w.level.found_secrets, 1);
        assert_eq!(w.drain_messages(), vec!["A secret is revealed!".to_owned()]);
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "misc/secret"),
            "secret find should play misc/secret"
        );
        player_in_special_sector(&mut w, mo);
        assert_eq!(w.level.found_secrets, 1);
        assert!(w.drain_messages().is_empty());
    }

    #[test]
    fn boom_secret_bit_cleared_after_find() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        // Damage 5 + secret bit.
        map.sectors[0].special = BOOM_DAMAGE_5 | BOOM_SECRET;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        init_sector_specials(&mut w);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        player_in_special_sector(&mut w, mo);
        assert_eq!(w.level.found_secrets, 1);
        assert_eq!(
            w.sector_specials.get(&0).copied(),
            Some(BOOM_DAMAGE_5),
            "secret bit cleared; damage bits kept"
        );
    }

    #[test]
    fn udmf_sector_leakiness_bypasses_radsuit() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("hurt", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].damage_amount = 5;
        map.sectors[0].damage_interval = 1;
        map.sectors[0].leakiness = 256;
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.players[0].as_mut().unwrap().radsuit = true;
        let hp0 = w.actors.get(mo).unwrap().health;
        w.level.time = 1;
        player_in_special_sector(&mut w, mo);
        assert_eq!(
            w.actors.get(mo).unwrap().health,
            hp0 - 5,
            "leakiness 256 always penetrates radsuit"
        );
    }

    fn hurt_room(ext: u32) -> (World, ActorId, ActorId) {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Grunt : Actor 9500
{
  Health 50
  +SHOOTABLE +COUNTKILL
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("hurt", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].damage_amount = 3;
        map.sectors[0].damage_interval = 1;
        map.udmf_user.sector_ext = vec![ext; map.sectors.len()];
        apply_nodes(&mut map);
        let grunt = classes.find_name("Grunt").unwrap();
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let g = w
            .spawn_at(
                grunt,
                room_core::DVec3::new(160.0, 160.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        (w, mo, g)
    }

    #[test]
    fn udmf_hurtmonsters_and_harminair() {
        use room_map::udmf::ext::*;
        // Plain damage sector: monsters are spared, airborne players too.
        let (mut w, mo, g) = hurt_room(0);
        w.actors.get_mut(mo).unwrap().pos = room_core::DVec3::new(64.0, 64.0, 40.0);
        player_in_special_sector(&mut w, mo);
        monsters_in_special_sectors(&mut w);
        assert_eq!(w.actors.get(mo).unwrap().health, 100);
        assert_eq!(w.actors.get(g).unwrap().health, 50);
        // `hurtmonsters` + `harminair`.
        let (mut w, mo, g) = hurt_room(SECTOR_HURTMONSTERS | SECTOR_HARMINAIR);
        w.actors.get_mut(mo).unwrap().pos = room_core::DVec3::new(64.0, 64.0, 40.0);
        player_in_special_sector(&mut w, mo);
        monsters_in_special_sectors(&mut w);
        assert_eq!(w.actors.get(mo).unwrap().health, 97);
        assert_eq!(w.actors.get(g).unwrap().health, 47);
    }

    #[test]
    fn udmf_damagehazard_fills_a_meter_before_hurting() {
        use room_map::udmf::ext::*;
        let (mut w, mo, _) = hurt_room(SECTOR_DAMAGEHAZARD);
        for t in 0..150 {
            w.level.time = t;
            player_in_special_sector(&mut w, mo);
        }
        assert_eq!(w.actors.get(mo).unwrap().health, 100, "meter still filling");
        assert!(w.players[0].as_ref().unwrap().hazard_count > 0);
        for t in 150..400 {
            w.level.time = t;
            player_in_special_sector(&mut w, mo);
        }
        assert!(w.actors.get(mo).unwrap().health < 100);
    }

    #[test]
    fn udmf_sector_damage_interval() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("hurt", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].damage_amount = 3;
        map.sectors[0].damage_interval = 8;
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hp0 = w.actors.get(mo).unwrap().health;
        w.level.time = 7;
        player_in_special_sector(&mut w, mo);
        assert_eq!(w.actors.get(mo).unwrap().health, hp0, "not on interval");
        w.level.time = 8;
        player_in_special_sector(&mut w, mo);
        assert_eq!(w.actors.get(mo).unwrap().health, hp0 - 3);
    }

    #[test]
    fn nukage_damages_player_every_32_tics() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("goo", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = ST_DAMAGE_10;
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let pos = w.actors.get(mo).unwrap().pos;
        assert_eq!(
            sector_at(&w, pos.x(), pos.y()),
            Some(0),
            "point_in_sector should resolve the floor under the player"
        );
        let hp0 = w.actors.get(mo).unwrap().health;
        w.level.time = 32;
        player_in_special_sector(&mut w, mo);
        let hp1 = w.actors.get(mo).unwrap().health;
        assert_eq!(hp1, hp0 - 10, "nukage should deal 10 damage");
        // Radsuit blocks further damage.
        w.players[0].as_mut().unwrap().radsuit = true;
        w.level.time = 64;
        player_in_special_sector(&mut w, mo);
        assert_eq!(w.actors.get(mo).unwrap().health, hp1);
    }

    #[test]
    fn damage_end_sector_exits_when_health_low() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("end", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = ST_DAMAGE_END;
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().health = 25;
        w.level.time = 32;
        assert!(!w.level.exiting);
        player_in_special_sector(&mut w, mo);
        assert_eq!(w.actors.get(mo).unwrap().health, 5);
        assert!(w.level.exiting, "type 11 should exit when health ≤ 10");
    }

    #[test]
    fn boom_damage_bits_hurt_player() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
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
        // Light type 0 + Boom 20 damage bits (no classic damage type).
        map.sectors[0].special = BOOM_DAMAGE_20;
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hp0 = w.actors.get(mo).unwrap().health;
        w.level.time = 32;
        player_in_special_sector(&mut w, mo);
        assert_eq!(w.actors.get(mo).unwrap().health, hp0 - 20);
    }

    #[test]
    fn boom_friction_bit_sets_sticky_floor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = BOOM_FRICTION | BOOM_PUSH;
        apply_nodes(&mut map);
        let w = World::from_map(map).unwrap();
        assert!((w.sectors[0].friction - BOOM_FRICTION_DEFAULT).abs() < 1e-9);
        assert!(w.sectors[0].boom_push);
    }

    #[test]
    fn mbf21_kill_monsters_bit_slays_countkill() {
        use crate::{ClassRegistry, StateTable};
        use room_core::{Angle, DVec3};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
actor Grunt : Actor 3004
{
  Health 20
  +SHOOTABLE
  +COUNTKILL
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = MBF21_KILL_MONSTERS;
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let grunt = classes.find_name("Grunt").unwrap();
        let id = w
            .spawn_at(grunt, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        monsters_in_special_sectors(&mut w);
        assert!(w.actors.get(id).unwrap().health <= 0);
    }

    #[test]
    fn mbf21_kill_disabled_when_nombf21() {
        use crate::class::ClassRegistry;
        use crate::compat::{COMPATF2_NOMBF21, CompatFlags};
        use crate::state::StateTable;
        use room_core::{Angle, DVec3};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Grunt : Actor 3004
{
  Health 20
  +SHOOTABLE
  +COUNTKILL
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].special = MBF21_KILL_MONSTERS;
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        w.compat = CompatFlags::from_bits(0, COMPATF2_NOMBF21);
        let grunt = classes.find_name("Grunt").unwrap();
        let id = w
            .spawn_at(grunt, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        monsters_in_special_sectors(&mut w);
        assert_eq!(w.actors.get(id).unwrap().health, 20);
    }
}
