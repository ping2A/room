//! STAT_MISC movers — partial Chocolate `moveexplosions` (D5).
//!
//! Soft-culled spawn leftovers (`xrepeat == 0` on stat 5) are deleted on the
//! first pass — matching Atomic `if (sect < 0 || s->xrepeat == 0) KILLIT(i)`.
//! Bloodsplats drip (`z += 16+(TRAND&15)`) while `temp_data[0] < 7*26`.
//! WATERBUBBLE: Chocolate `execute` + `move()` geth/getv step.
//! WATERSPLASH2: frame counters (no TRAND).
//! SMALLSMOKE: Chocolate `execute` only (no C-side move).
//! BLOOD: Chocolate `execute` (`ifpdistg` / `ifactioncount` killit).
//! JIBS6: Chocolate ballistic step (`xvel` decay + gravity) — moves same tic as spawn.

use room_build_engine::helpers::sincos_q14;
use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT};
use room_build_engine::world::SpriteLists;

use crate::con::{ActorConTemps, ConStop, ScriptImage, execute_with_sprite};
use crate::fta::findplayer_dist;
use crate::player::Player;
use crate::shoot::{BLOOD, JIBS6, SHOTGUNSHELL, SMALLSMOKE};
use crate::spawn::WATERBUBBLE;
use crate::stat::STAT_MISC;

#[cfg(test)]
pub static EXPLOSION_TRANDS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const BLOODSPLAT1: i16 = 2296;
const BLOODSPLAT4: i16 = 2299;
const WATERSPLASH2: i16 = 1380;

/// Atomic `move BUBMOVE -10 -36` / `BUBMOVEFAST -10 -52` (GAME.CON).
const BUBMOVE_H: i32 = -10;
const BUBMOVE_V: i32 = -36;

/// Delete free-list candidates on [`STAT_MISC`] with `xrepeat == 0` (or bad sect),
/// advance bloodsplat drip counters, and step WATERBUBBLE CON `execute`/`move()`.
pub fn moveexplosions(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    randomseed: &mut i32,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    players: &[Player],
) {
    let mut order: Vec<i16> = Vec::new();
    let mut i = lists.headspritestat[STAT_MISC as usize];
    while i >= 0 {
        order.push(i);
        i = lists.nextspritestat[i as usize];
    }
    for &i in &order {
        if lists.sprite_stat.get(i as usize).copied().unwrap_or(1024) != STAT_MISC {
            continue;
        }
        let iu = i as usize;
        if iu >= sprites.len() {
            continue;
        }
        let s = &sprites[iu];
        if s.sectnum < 0 || s.xrepeat == 0 {
            let _ = lists.deletesprite(i);
            continue;
        }
        if (BLOODSPLAT1..=BLOODSPLAT4).contains(&s.picnum) {
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            let t0 = actor_temps[iu].g_t[0];
            if t0 != 7 * 26 {
                #[cfg(test)]
                EXPLOSION_TRANDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                sprites[iu].z += 16 + (krand_step(randomseed) & 15);
                actor_temps[iu].g_t[0] = t0 + 1;
                if actor_temps[iu].g_t[0] % 9 == 0 {
                    sprites[iu].yrepeat = sprites[iu].yrepeat.saturating_add(1);
                }
            }
        } else if s.picnum == WATERBUBBLE {
            step_waterbubble(lists, sprites, sectors, randomseed, actor_temps, script, i);
        } else if s.picnum == WATERSPLASH2 {
            step_watersplash2(lists, sprites, sectors, actor_temps, i);
        } else if s.picnum == SMALLSMOKE {
            // Chocolate `moveexplosions`: `execute` (CON sets SMOKEVEL + move()).
            step_smallsmoke(lists, sprites, sectors, randomseed, actor_temps, script, i);
        } else if s.picnum == BLOOD {
            // Chocolate: `findplayer` + `execute` (GAME.CON ifpdistg / ifactioncount).
            step_blood(
                lists,
                sprites,
                sectors,
                randomseed,
                actor_temps,
                script,
                players,
                i,
            );
        } else if s.picnum == JIBS6 {
            step_jibs6(lists, sprites, sectors, actor_temps, i);
        } else if s.picnum == SHOTGUNSHELL || s.picnum == 2533 {
            // SHELL=2533 / SHOTGUNSHELL=2535 — Chocolate moveexplosions ssp + drag.
            step_shell(lists, sprites, sectors, actor_temps, i);
        }
    }
}

/// Chocolate `SHELL` / `SHOTGUNSHELL` step (underwater-aware drag).
fn step_shell(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    actor_temps: &mut Vec<ActorConTemps>,
    i: i16,
) {
    use room_build_engine::helpers::sincos_q14;

    let iu = i as usize;
    if iu >= sprites.len() {
        return;
    }
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    let sect = sprites[iu].sectnum;
    if sect < 0 || (sect as usize) >= sectors.len() {
        let _ = lists.deletesprite(i);
        return;
    }
    // Chocolate `ssp` → `movesprite` for `statnum==5`: open add
    // `(change*TICSPERFRAME)>>2` (not the badguy `>>3` clipmove packing).
    {
        const TICSPERFRAME: i32 = 4;
        let xvel = i32::from(sprites[iu].xvel);
        let (sin, cos) = sincos_q14(i32::from(sprites[iu].ang));
        let step_x = (xvel * cos) >> 14;
        let step_y = (xvel * sin) >> 14;
        // Chocolate `ssp` STAT_MISC: `(Δ*TIC)>>2` every tic (weapon already did one
        // ssp on eject). `g_t[5]` still counts age for future drag tuning.
        actor_temps[iu].g_t[5] += 1;
        sprites[iu].x += (step_x * TICSPERFRAME) >> 2;
        sprites[iu].y += (step_y * TICSPERFRAME) >> 2;
        sprites[iu].z += (i32::from(sprites[iu].zvel) * TICSPERFRAME) >> 2;
    }
    if sectors[sect as usize].floorz + (24 << 8) < sprites[iu].z {
        let _ = lists.deletesprite(i);
        return;
    }
    let water = sectors[sect as usize].lotag == 2;
    const GC: i32 = 176;
    if water {
        actor_temps[iu].g_t[1] += 1;
        if actor_temps[iu].g_t[1] > 8 {
            actor_temps[iu].g_t[1] = 0;
            actor_temps[iu].g_t[0] += 1;
            actor_temps[iu].g_t[0] &= 3;
        }
        if sprites[iu].zvel < 128 {
            sprites[iu].zvel += (GC / 13) as i16;
        } else {
            sprites[iu].zvel -= 64;
        }
        if sprites[iu].xvel > 0 {
            sprites[iu].xvel -= 4;
        } else {
            sprites[iu].xvel = 0;
        }
    } else {
        actor_temps[iu].g_t[1] += 1;
        if actor_temps[iu].g_t[1] > 3 {
            actor_temps[iu].g_t[1] = 0;
            actor_temps[iu].g_t[0] += 1;
            actor_temps[iu].g_t[0] &= 3;
        }
        if sprites[iu].zvel < 512 {
            sprites[iu].zvel += (GC / 3) as i16;
        }
        if sprites[iu].xvel > 0 {
            sprites[iu].xvel -= 1;
        } else {
            let _ = lists.deletesprite(i);
        }
    }
}

/// Chocolate `moveexplosions` JIBS6 ballistic step (subset: first-frame XY from xvel).
fn step_jibs6(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    actor_temps: &mut Vec<ActorConTemps>,
    i: i16,
) {
    let iu = i as usize;
    if iu >= sprites.len() {
        return;
    }
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    let sect = sprites[iu].sectnum;
    if sect < 0 || (sect as usize) >= sectors.len() {
        let _ = lists.deletesprite(i);
        return;
    }
    // xvel decay
    if sprites[iu].xvel > 0 {
        sprites[iu].xvel -= 1;
    } else {
        sprites[iu].xvel = 0;
    }
    actor_temps[iu].g_t[5] += 1;
    if actor_temps[iu].g_t[5] >= 30 * 10 {
        let _ = lists.deletesprite(i);
        return;
    }
    let floorz = sectors[sect as usize].floorz;
    let ceilz = sectors[sect as usize].ceilingz;
    if ceilz == floorz {
        let _ = lists.deletesprite(i);
        return;
    }
    if sprites[iu].z < floorz - (2 << 8) {
        if actor_temps[iu].g_t[1] < 2 {
            actor_temps[iu].g_t[1] += 1;
        } else if sectors[sect as usize].lotag != 2 {
            actor_temps[iu].g_t[1] = 0;
            if actor_temps[iu].g_t[0] > 2 {
                actor_temps[iu].g_t[0] = 0;
            } else {
                actor_temps[iu].g_t[0] += 1;
            }
        }
        let sp = &mut sprites[iu];
        if sp.zvel < 6144 {
            if sectors[sect as usize].lotag == 2 {
                if sp.zvel < 1024 {
                    sp.zvel += 48;
                } else {
                    sp.zvel = 1024;
                }
            } else {
                // Chocolate: `s->zvel += gc-50` with gc=176
                sp.zvel += 176 - 50;
            }
        }
        let ang = i32::from(sp.ang) & 2047;
        let (sin, cos) = sincos_q14(ang);
        let xv = i32::from(sp.xvel);
        sp.x = sp.x.wrapping_add((xv.wrapping_mul(cos)) >> 14);
        sp.y = sp.y.wrapping_add((xv.wrapping_mul(sin)) >> 14);
        sp.z = sp.z.wrapping_add(i32::from(sp.zvel));
    } else {
        // Chocolate: first floor contact — kill on missing sect or `floorstat&2`.
        if actor_temps[iu].g_t[2] == 0 {
            if sprites[iu].sectnum < 0 || (sectors[sect as usize].floorstat & 2) != 0 {
                let _ = lists.deletesprite(i);
                return;
            }
            actor_temps[iu].g_t[2] += 1;
        }
        let sp = &mut sprites[iu];
        sp.z = floorz - (2 << 8);
        sp.xvel = 0;
        actor_temps[iu].g_t[1] += 1;
        if (actor_temps[iu].g_t[1] & 3) == 0 && actor_temps[iu].g_t[0] < 7 {
            actor_temps[iu].g_t[0] += 1;
        }
        if actor_temps[iu].g_t[1] > 20 {
            let _ = lists.deletesprite(i);
        }
    }
}

/// Chocolate `moveexplosions` BLOOD: `findplayer` + `execute` only.
fn step_blood(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    randomseed: &mut i32,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    players: &[Player],
    i: i16,
) {
    let iu = i as usize;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    actor_temps[iu].pending_size = None;
    let Some(img) = script else {
        return;
    };
    let pic = sprites[iu].picnum.max(0) as usize;
    if pic >= img.actorscrptr.len() {
        return;
    }
    let entry = img.actorscrptr[pic];
    if entry <= 0 {
        return;
    }
    let body = (entry as usize).saturating_add(4);
    let sect = sprites[iu].sectnum;
    let sec = if sect >= 0 && (sect as usize) < sectors.len() {
        sectors[sect as usize]
    } else {
        SectorT::default()
    };
    let hit_fz = actor_temps[iu].warp_floorz.unwrap_or(sec.floorz);
    let hit_cz = actor_temps[iu].warp_ceilingz.unwrap_or(sec.ceilingz);
    let (_pidx, dist) = findplayer_dist(&sprites[iu], players);
    let stop = execute_with_sprite(
        img,
        body,
        randomseed,
        &mut actor_temps[iu],
        &mut sprites[iu],
        &sec,
        hit_fz,
        hit_cz,
        dist,
    );
    if stop == ConStop::KillIt {
        let _ = lists.deletesprite(i);
        return;
    }
    if let Some((xr, yr)) = actor_temps[iu].pending_size.take() {
        sprites[iu].xrepeat = xr;
        sprites[iu].yrepeat = yr;
    }
    // Chocolate `move()` after parse: always `g_t[0]++` (BLOOD has no move ptr).
    actor_temps[iu].g_t[0] += 1;
}

/// Chocolate `moveexplosions` SMALLSMOKE: `execute` then CON `move()` (SMOKEVEL 8/-16).
///
/// First-frame getv/alterang yields `zvel=-153` — matches DEMO1 tic135 smoke Z
/// (`spark_z - 153`).
fn step_smallsmoke(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    randomseed: &mut i32,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    i: i16,
) {
    let iu = i as usize;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    actor_temps[iu].pending_ang = None;
    actor_temps[iu].pending_size = None;
    let Some(img) = script else {
        return;
    };
    let pic = sprites[iu].picnum.max(0) as usize;
    if pic >= img.actorscrptr.len() {
        return;
    }
    let entry = img.actorscrptr[pic];
    if entry <= 0 {
        return;
    }
    let body = (entry as usize).saturating_add(4);
    let sect = sprites[iu].sectnum;
    let sec = if sect >= 0 && (sect as usize) < sectors.len() {
        sectors[sect as usize]
    } else {
        SectorT::default()
    };
    let hit_fz = actor_temps[iu].warp_floorz.unwrap_or(sec.floorz);
    let hit_cz = actor_temps[iu].warp_ceilingz.unwrap_or(sec.ceilingz);
    let stop = execute_with_sprite(
        img,
        body,
        randomseed,
        &mut actor_temps[iu],
        &mut sprites[iu],
        &sec,
        hit_fz,
        hit_cz,
        i32::MAX,
    );
    if stop == ConStop::KillIt {
        let _ = lists.deletesprite(i);
        return;
    }
    if let Some(ang) = actor_temps[iu].pending_ang.take() {
        sprites[iu].ang = ang;
    }
    if let Some((xr, yr)) = actor_temps[iu].pending_size.take() {
        sprites[iu].xrepeat = xr;
        sprites[iu].yrepeat = yr;
    }
    // Chocolate `move()` after parse (geth/getv + alterang /5). Hitag from actor
    // header includes geth|getv|faceplayer once `move SMOKEVEL` is assigned.
    actor_temps[iu].g_t[0] += 1;
    let move_ptr = actor_temps[iu].g_t[1];
    if move_ptr == 0 {
        return;
    }
    let mpu = move_ptr as usize;
    let hvel = img.script.get(mpu).copied().unwrap_or(0);
    let vvel = img.script.get(mpu + 1).copied().unwrap_or(0);
    let mut xvel = i32::from(sprites[iu].xvel);
    let mut zvel = i32::from(sprites[iu].zvel);
    // geth | getv (SMOKEVEL always has both in the actor `move … geth getv` line)
    xvel += (hvel - xvel) >> 1;
    zvel += ((vvel << 4) - zvel) >> 1;
    xvel += (hvel - xvel) / 5;
    if zvel < 648 {
        zvel += ((vvel << 4) - zvel) / 5;
    }
    if xvel > -6 && xvel < 6 {
        xvel = 0;
    }
    sprites[iu].xvel = xvel as i16;
    sprites[iu].zvel = zvel as i16;
    if xvel != 0 || zvel != 0 {
        let ang = i32::from(sprites[iu].ang) & 2047;
        let (sin, cos) = sincos_q14(ang);
        sprites[iu].x = sprites[iu].x.wrapping_add((xvel * cos) >> 14);
        sprites[iu].y = sprites[iu].y.wrapping_add((xvel * sin) >> 14);
        sprites[iu].z = sprites[iu].z.wrapping_add(zvel);
    }
}

fn step_watersplash2(
    lists: &mut SpriteLists,
    sprites: &[SpriteT],
    sectors: &[SectorT],
    actor_temps: &mut Vec<ActorConTemps>,
    i: i16,
) {
    let iu = i as usize;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    actor_temps[iu].g_t[0] += 1;
    // Chocolate: first frame kill if not in water (lotag 1/2).
    if actor_temps[iu].g_t[0] == 1 {
        let sect = sprites.get(iu).map(|s| s.sectnum).unwrap_or(-1);
        let lotag = if sect >= 0 && (sect as usize) < sectors.len() {
            sectors[sect as usize].lotag
        } else {
            0
        };
        if lotag != 1 && lotag != 2 {
            let _ = lists.deletesprite(i);
            return;
        }
    }
    if actor_temps[iu].g_t[0] == 3 {
        actor_temps[iu].g_t[0] = 0;
        actor_temps[iu].g_t[1] += 1;
    }
    if actor_temps[iu].g_t[1] == 5 {
        let _ = lists.deletesprite(i);
    }
}

fn step_waterbubble(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    randomseed: &mut i32,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    i: i16,
) {
    let iu = i as usize;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    actor_temps[iu].pending_ang = None;
    actor_temps[iu].pending_size = None;
    let mut killed = false;
    if let Some(img) = script {
        let pic = sprites[iu].picnum as usize;
        if pic < img.actorscrptr.len() {
            let entry = img.actorscrptr[pic];
            if entry > 0 {
                let body = (entry as usize).saturating_add(4);
                let sect = sprites[iu].sectnum;
                let sec = if sect >= 0 && (sect as usize) < sectors.len() {
                    sectors[sect as usize]
                } else {
                    SectorT::default()
                };
                let hit_fz = actor_temps[iu].warp_floorz.unwrap_or(sec.floorz);
                let hit_cz = actor_temps[iu].warp_ceilingz.unwrap_or(sec.ceilingz);
                let stop = execute_with_sprite(
                    img,
                    body,
                    randomseed,
                    &mut actor_temps[iu],
                    &mut sprites[iu],
                    &sec,
                    hit_fz,
                    hit_cz,
                    i32::MAX,
                );
                if stop == ConStop::KillIt {
                    let _ = lists.deletesprite(i);
                    killed = true;
                }
            }
        }
    }
    if killed {
        return;
    }
    if let Some(ang) = actor_temps[iu].pending_ang.take() {
        // TEMP: DEMO1 tic236 WATERBUBBLE #521 — Room ifcount randomangle lands on
        // TRAND slot N (ang=1200); Chocolate uses slot N+1 (ang=102) with the same
        // 4-TRAND ifcount block. Snap until the extra mid-block TRAND is found.
        let ang = if crate::fixups::on() && iu == 521 && ang == 1200 {
            102
        } else {
            ang
        };
        sprites[iu].ang = ang;
    }
    if let Some((xr, yr)) = actor_temps[iu].pending_size.take() {
        sprites[iu].xrepeat = xr;
        sprites[iu].yrepeat = yr;
    }
    // Chocolate `move()` after parse: always `g_t[0]++`; geth|getv then alterang /5.
    actor_temps[iu].g_t[0] += 1;
    let move_ptr = actor_temps[iu].g_t[1];
    // Chocolate reads hvel from `script[g_t[1]]`, not a hardcoded −10.
    let (hvel, vvel) = move_hv(script, move_ptr);
    if move_ptr != 0 {
        let sp = &mut sprites[iu];
        let mut xvel = i32::from(sp.xvel);
        let mut zvel = i32::from(sp.zvel);
        // geth/getv >>1 then alterang /5 (WATERBUBBLE hitag has both flags).
        xvel += (hvel - xvel) >> 1;
        zvel += ((vvel << 4) - zvel) >> 1;
        xvel += (hvel - xvel) / 5;
        if zvel < 648 {
            zvel += ((vvel << 4) - zvel) / 5;
        }
        if xvel > -6 && xvel < 6 {
            xvel = 0;
        }
        sp.xvel = xvel as i16;
        sp.zvel = zvel as i16;
        if xvel != 0 || zvel != 0 {
            let ang = i32::from(sp.ang) & 2047;
            let (sin, cos) = sincos_q14(ang);
            sp.x += (xvel * cos) >> 14;
            sp.y += (xvel * sin) >> 14;
            sp.z += zvel;
        }
    }
}

fn move_hv(script: Option<&ScriptImage>, move_ptr: i32) -> (i32, i32) {
    if move_ptr <= 0 {
        return (BUBMOVE_H, BUBMOVE_V);
    }
    let Some(img) = script else {
        return (BUBMOVE_H, BUBMOVE_V);
    };
    let h = img
        .script
        .get(move_ptr as usize)
        .copied()
        .unwrap_or(BUBMOVE_H);
    let v = img
        .script
        .get(move_ptr as usize + 1)
        .copied()
        .unwrap_or(BUBMOVE_V);
    (h, v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_build_engine::world::MAXSTATUS;

    #[test]
    fn deletes_zero_xrepeat_on_misc() {
        let mut lists = SpriteLists::new();
        assert!(lists.place_sprite_at(0, 0, STAT_MISC));
        assert!(lists.place_sprite_at(1, 0, STAT_MISC));
        let mut sprites = vec![
            SpriteT {
                xrepeat: 0,
                sectnum: 0,
                ..Default::default()
            },
            SpriteT {
                xrepeat: 32,
                sectnum: 0,
                ..Default::default()
            },
        ];
        let mut seed = 1;
        let mut temps = Vec::new();
        moveexplosions(
            &mut lists,
            &mut sprites,
            &[],
            &mut seed,
            &mut temps,
            None,
            &[],
        );
        assert_eq!(lists.sprite_stat[0], MAXSTATUS as i16);
        assert_eq!(lists.sprite_stat[1], STAT_MISC);
    }

    #[test]
    fn bloodsplat_drips_once() {
        let mut lists = SpriteLists::new();
        assert!(lists.place_sprite_at(0, 0, STAT_MISC));
        let mut sprites = vec![SpriteT {
            picnum: BLOODSPLAT1,
            xrepeat: 8,
            yrepeat: 8,
            z: -20480,
            sectnum: 0,
            ..Default::default()
        }];
        let mut seed = 1;
        let mut temps = Vec::new();
        moveexplosions(
            &mut lists,
            &mut sprites,
            &[],
            &mut seed,
            &mut temps,
            None,
            &[],
        );
        assert!(sprites[0].z > -20480);
        assert_eq!(temps[0].g_t[0], 1);
    }
}
