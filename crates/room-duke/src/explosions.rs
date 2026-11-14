//! STAT_MISC movers — partial Chocolate `moveexplosions` (D5).
//!
//! Soft-culled spawn leftovers (`xrepeat == 0` on stat 5) are deleted on the
//! first pass — matching Atomic `if (sect < 0 || s->xrepeat == 0) KILLIT(i)`.
//! Bloodsplats drip (`z += 16+(TRAND&15)`) while `temp_data[0] < 7*26`.
//! WATERBUBBLE: Chocolate `execute` + `move()` geth/getv step, retaining its
//! spawn hit-plane cache across teleports for later `iffloordistl` predicates.
//! WATERSPLASH2: frame counters (no TRAND).
//! SMALLSMOKE: Chocolate `execute` only (no C-side move).
//! BLOOD: Chocolate `execute` (`ifpdistg` / `ifactioncount` killit).
//! JIBS6: Chocolate ballistic step (`xvel` decay + gravity) — moves same tic as spawn.

use room_build_engine::geom::{SectorWalls, SectorZ, WallXY, getzsofslope, updatesector};
use room_build_engine::helpers::sincos_q14;
use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::con::{ActorConTemps, ConStop, ScriptImage, execute_with_sprite_id};
use crate::fta::findplayer_dist;
use crate::player::Player;
use crate::shoot::{BLOOD, JIBS6, SHOTGUNSHELL, SMALLSMOKE};
use crate::spawn::WATERBUBBLE;
use crate::stat::STAT_MISC;

#[cfg(test)]
pub static EXPLOSION_TRANDS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const BLOODSPLAT1: i16 = 2296;
const BLOODSPLAT4: i16 = 2299;
const BLOODPOOL: i16 = 1226;
const PUKE: i16 = 4389;
const TIRE: i16 = 990;
const WATERSPLASH2: i16 = 1380;
const EXPLOSION2: i16 = 1890;
const FRAMEEFFECT1: i16 = 4095;
const SCRAP6: i16 = 2390;
const GLASSPIECES: i16 = 1031;
const FOURSLEIGHT: i32 = 1 << 8;
const FIRELASER_TRAIL: i16 = 1625;
const SCRAP5: i16 = 2416;

/// Atomic `move BUBMOVE -10 -36` / `BUBMOVEFAST -10 -52` (GAME.CON).
const BUBMOVE_H: i32 = -10;
const BUBMOVE_V: i32 = -36;

/// Delete free-list candidates on [`STAT_MISC`] with `xrepeat == 0` (or bad sect),
/// advance bloodsplat drip counters, and step WATERBUBBLE CON `execute`/`move()`.
pub fn moveexplosions(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    randomseed: &mut i32,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    players: &mut [Player],
    active_sound_owners: &mut [Option<i16>],
    pending_sound_ids: &mut Vec<u16>,
) {
    let sector_walls: Vec<SectorWalls> =
        sectors.iter().map(SectorT::z).map(|z| z.walls()).collect();
    let sector_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
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
        let s = sprites[iu];
        if s.sectnum < 0 || s.xrepeat == 0 {
            let _ = lists.deletesprite(i);
            continue;
        }
        if s.picnum == FIRELASER_TRAIL {
            // Chocolate `moveexplosions` `case FIRELASER`: trail copies live
            // one tic (`s->extra` 999 marker), then KILLIT.
            if sprites[iu].extra != 999 {
                sprites[iu].extra = 999;
            } else {
                let _ = lists.deletesprite(i);
            }
            continue;
        }
        if (GLASSPIECES..=GLASSPIECES + 2).contains(&s.picnum) {
            // Chocolate `moveexplosions` `case GLASSPIECES..+2`: fall with
            // raw sector bounds, bounce three times (halving size), slide
            // with xvel drag, then `ssp` (stat 5: plain add, no sector update).
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            let mut sp = s;
            crate::spawn::makeitfall(&mut sp, sectors, walls, &[], &[], crate::player::DUKE_GC, false);
            if sp.zvel > 4096 {
                sp.zvel = 4096;
            }
            let Some(sec) = sectors.get(sp.sectnum as usize).copied() else {
                let _ = lists.deletesprite(i);
                continue;
            };
            let t0 = actor_temps[iu].g_t[0];
            if sp.z == sec.floorz - FOURSLEIGHT && t0 < 3 {
                let r = krand_step(randomseed) & 511;
                let mut zvel = -((3 - t0) << 8) - r;
                if sec.lotag == 2 {
                    zvel >>= 1;
                }
                sp.zvel = zvel as i16;
                sp.xrepeat >>= 1;
                sp.yrepeat >>= 1;
                // `rnd(96)` → `setsprite` (refresh sector).
                if (krand_step(randomseed) >> 8) >= 255 - 96 {
                    let mut ns = sp.sectnum;
                    updatesector(sp.x, sp.y, &mut ns, &sector_walls, &wall_xy);
                    if ns >= 0 && ns != sp.sectnum {
                        let _ = lists.changespritesect(i, ns);
                        sp.sectnum = ns;
                    }
                }
                actor_temps[iu].g_t[0] = t0 + 1;
            } else if t0 == 3 {
                let _ = lists.deletesprite(i);
                continue;
            }
            if sp.xvel > 0 {
                sp.xvel -= 2;
                sp.cstat = ((sp.xvel & 3) << 2) as u16;
            } else {
                sp.xvel = 0;
            }
            let (sin, cos) = sincos_q14(i32::from(sp.ang));
            sp.x += (((i32::from(sp.xvel) * cos) >> 14) * 4) >> 2;
            sp.y += (((i32::from(sp.xvel) * sin) >> 14) * 4) >> 2;
            sp.z += (i32::from(sp.zvel) * 4) >> 2;
            sprites[iu] = sp;
            continue;
        }
        if (SCRAP6..=SCRAP5 + 3).contains(&s.picnum) {
            // Chocolate `moveexplosions` `IFWITHIN(SCRAP6, SCRAP5+3)`.
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            let mut sp = s;
            if sp.xvel > 0 {
                sp.xvel -= 1;
            } else {
                sp.xvel = 0;
            }
            let mut sect = sp.sectnum;
            if sp.zvel > 1024 && sp.zvel < 1280 {
                // `setsprite`: refresh the sector for the current position.
                let mut ns = sect;
                updatesector(sp.x, sp.y, &mut ns, &sector_walls, &wall_xy);
                if ns >= 0 && ns != sect {
                    let _ = lists.changespritesect(i, ns);
                    sp.sectnum = ns;
                    sect = ns;
                }
            }
            let floorz = sectors.get(sect as usize).map_or(i32::MAX, |sc| sc.floorz);
            if sp.z < floorz - (2 << 8) {
                let t = &mut actor_temps[iu].g_t;
                if t[1] < 1 {
                    t[1] += 1;
                } else {
                    t[1] = 0;
                    let wrap = if sp.picnum < SCRAP6 + 8 { 6 } else { 2 };
                    if t[0] > wrap {
                        t[0] = 0;
                    } else {
                        t[0] += 1;
                    }
                }
                if sp.zvel < 4096 {
                    sp.zvel = sp.zvel.saturating_add((crate::player::DUKE_GC - 50) as i16);
                }
                let (sin, cos) = sincos_q14(i32::from(sp.ang));
                sp.x += (i32::from(sp.xvel) * cos) >> 14;
                sp.y += (i32::from(sp.xvel) * sin) >> 14;
                sp.z += i32::from(sp.zvel);
                sprites[iu] = sp;
            } else {
                sprites[iu] = sp;
                let _ = lists.deletesprite(i);
            }
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
        } else if s.picnum == BLOODPOOL || s.picnum == PUKE {
            // Chocolate `actors.c::moveactors` BLOODPOOL/PUKE growth phase.
            // `spawn()` initializes a valid pool to 1×1. It expands for 32
            // tics using two ordered TRANDs, follows the sector floor, then
            // shrinks by six repeat units.
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            if actor_temps[iu].g_t[0] == 0 {
                actor_temps[iu].g_t[0] = 1;
                if sectors
                    .get(s.sectnum as usize)
                    .is_some_and(|sector| sector.floorstat & 2 != 0)
                {
                    let _ = lists.deletesprite(i);
                    continue;
                }
            }
            let (_, floorz) = getzsofslope(s.sectnum, s.x, s.y, &sector_z, &wall_xy);
            sprites[iu].z = floorz - (4 << 8);
            if actor_temps[iu].g_t[2] < 32 {
                actor_temps[iu].g_t[2] += 1;
                let max_repeat = if actor_temps[iu].hit_picnum == TIRE {
                    64
                } else {
                    32
                };
                if i32::from(s.xrepeat) < max_repeat && i32::from(s.yrepeat) < max_repeat {
                    sprites[iu].xrepeat =
                        s.xrepeat.saturating_add((krand_step(randomseed) & 3) as u8);
                    sprites[iu].yrepeat =
                        s.yrepeat.saturating_add((krand_step(randomseed) & 3) as u8);
                }
            }
            let (player_index, distance) = findplayer_dist(&s, players);
            if distance < 844 && sprites[iu].xrepeat > 6 && sprites[iu].yrepeat > 6 {
                if s.pal == 0 && s.picnum != PUKE && (krand_step(randomseed) & 255) < 16 {
                    if let Some(player) = players.get_mut(player_index) {
                        if player.boot_amount > 0 {
                            player.boot_amount -= 1;
                        } else if let Ok(player_sprite) = usize::try_from(player.sprite_i)
                            && let Some(body) = sprites.get_mut(player_sprite)
                        {
                            body.extra = body.extra.saturating_sub(1);
                            player.extra = player.extra.saturating_sub(1).max(0);
                            player.last_extra = player.extra;
                            player.palette_flash = [16, 0, 0];
                            player.palette_flash_tics = 32;
                            if active_sound_owners.get(211).is_some_and(Option::is_none) {
                                pending_sound_ids.push(211);
                                if let Some(owner) = active_sound_owners.get_mut(211) {
                                    *owner = Some(player.sprite_i);
                                }
                            }
                        }
                    }
                }
                if actor_temps[iu].g_t[1] == 0 {
                    actor_temps[iu].g_t[1] = 1;
                    if let Some(player) = players.get_mut(player_index) {
                        player.footprint_count = if actor_temps[iu].hit_picnum == TIRE {
                            10
                        } else {
                            3
                        };
                        player.footprint_pal = s.pal;
                        player.footprint_shade = s.shade;
                    }
                }
                if actor_temps[iu].g_t[2] == 32 {
                    sprites[iu].xrepeat = sprites[iu].xrepeat.saturating_sub(6);
                    sprites[iu].yrepeat = sprites[iu].yrepeat.saturating_sub(6);
                }
            } else {
                actor_temps[iu].g_t[1] = 0;
            }
        } else if s.picnum == WATERBUBBLE {
            step_waterbubble(
                lists,
                sprites,
                sectors,
                &sector_walls,
                &wall_xy,
                randomseed,
                actor_temps,
                script,
                i,
            );
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
        } else if s.picnum == JIBS6 || is_jib_piece(s.picnum) {
            step_jibs6(
                lists,
                sprites,
                sectors,
                &sector_walls,
                &sector_z,
                &wall_xy,
                actor_temps,
                i,
            );
        } else if s.picnum == FRAMEEFFECT1 {
            // Chocolate `actors.c::moveactors`: FRAMEEFFECT1 is an eight-tic
            // afterimage. The CON spawn has already placed it at its owner;
            // the native actor step advances translucency and copies offsets.
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            if let Ok(owner) = usize::try_from(s.owner)
                && let Some(source) = sprites.get(owner).copied()
            {
                let age = actor_temps[iu].g_t[0].saturating_add(1);
                actor_temps[iu].g_t[0] = age;
                if age > 7 {
                    let _ = lists.deletesprite(i);
                    continue;
                }
                if age > 4 {
                    sprites[iu].cstat |= 512 | 2;
                } else if age > 2 {
                    sprites[iu].cstat |= 2;
                }
                sprites[iu].xoffset = source.xoffset;
                sprites[iu].yoffset = source.yoffset;
            }
        } else if s.picnum == EXPLOSION2 {
            // RPG impact flash: retain the animated ART tile briefly. The
            // radius damage/CON hitradius pass is still a separate D6 item.
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            actor_temps[iu].g_t[5] += 1;
            if actor_temps[iu].g_t[5] >= 20 {
                let _ = lists.deletesprite(i);
            }
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

/// Chocolate `moveexplosions` JIBS1..5 / HEADJIB1 / ARMJIB1 / LEGJIB1 /
/// LIZMAN* / DUKE* body parts: same case as JIBS6 (they become a JIBS6 splat
/// on landing). Without it they hung in the air where they spawned.
fn is_jib_piece(pic: i16) -> bool {
    matches!(
        pic,
        2245 | 2250 | 2255 | 2260 | 2265 // JIBS1..JIBS5
            | 1768 | 1772 | 1776 // HEADJIB1 ARMJIB1 LEGJIB1
            | 2201 | 2205 | 2209 // LIZMANHEAD1 LIZMANARM1 LIZMANLEG1
            | DUKETORSO | DUKEGUN | DUKELEG
    )
}

const DUKETORSO: i16 = 1520;
const DUKEGUN: i16 = 1528;
const DUKELEG: i16 = 1536;

/// Chocolate `moveexplosions` JIBS6 ballistic and floor-contact step.
fn step_jibs6(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    sector_walls: &[SectorWalls],
    sector_z: &[SectorZ],
    wall_xy: &[WallXY],
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
    let mut sect = sprites[iu].sectnum;
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
    // Chocolate calls `setsprite` for a narrow vertical velocity band before
    // querying the floor. This refreshes the sector after the fragment drifts
    // across a boundary; flat floor fields would leave it in the old sector.
    if (1024..1280).contains(&i32::from(sprites[iu].zvel)) {
        let _ = updatesector(
            sprites[iu].x,
            sprites[iu].y,
            &mut sect,
            sector_walls,
            wall_xy,
        );
        if sect != sprites[iu].sectnum {
            if sect < 0 {
                let _ = lists.deletesprite(i);
                return;
            }
            let _ = lists.changespritesect(i, sect);
            sprites[iu].sectnum = sect;
        }
    }
    if sect < 0 || sect as usize >= sectors.len() {
        let _ = lists.deletesprite(i);
        return;
    }
    let (ceilz, floorz) = getzsofslope(sect, sprites[iu].x, sprites[iu].y, sector_z, wall_xy);
    if ceilz == floorz {
        let _ = lists.deletesprite(i);
        return;
    }
    if sprites[iu].z < floorz - (2 << 8) {
        if actor_temps[iu].g_t[1] < 2 {
            actor_temps[iu].g_t[1] += 1;
        } else if sectors[sect as usize].lotag != 2 {
            actor_temps[iu].g_t[1] = 0;
            let wrap = if matches!(sprites[iu].picnum, DUKELEG | DUKETORSO | DUKEGUN) { 6 } else { 2 };
            if actor_temps[iu].g_t[0] > wrap {
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
        if sp.picnum != JIBS6 {
            // A landed body part turns into a JIBS6 splat.
            sp.picnum = JIBS6;
            actor_temps[iu].g_t[0] = 0;
            actor_temps[iu].g_t[1] = 0;
            return;
        }
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
    let stop = execute_with_sprite_id(
        img,
        body,
        randomseed,
        &mut actor_temps[iu],
        &mut sprites[iu],
        &sec,
        hit_fz,
        hit_cz,
        dist,
        i,
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
    let stop = execute_with_sprite_id(
        img,
        body,
        randomseed,
        &mut actor_temps[iu],
        &mut sprites[iu],
        &sec,
        hit_fz,
        hit_cz,
        i32::MAX,
        i,
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
    sector_walls: &[SectorWalls],
    wall_xy: &[WallXY],
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
                let stop = execute_with_sprite_id(
                    img,
                    body,
                    randomseed,
                    &mut actor_temps[iu],
                    &mut sprites[iu],
                    &sec,
                    hit_fz,
                    hit_cz,
                    i32::MAX,
                    i,
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
        let mut xvel = i32::from(sprites[iu].xvel);
        let mut zvel = i32::from(sprites[iu].zvel);
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
        sprites[iu].xvel = xvel as i16;
        sprites[iu].zvel = zvel as i16;
        if xvel != 0 || zvel != 0 {
            let ang = i32::from(sprites[iu].ang) & 2047;
            let (sin, cos) = sincos_q14(ang);
            sprites[iu].x += (xvel * cos) >> 14;
            sprites[iu].y += (xvel * sin) >> 14;
            // Chocolate `movesprite` on STAT_MISC only offsets x/y/z; it
            // never calls updatesector/setsprite, so `sectnum` is kept.
            sprites[iu].z += zvel;
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
        let mut players = [];
        let mut owners = [];
        let mut sounds = Vec::new();
        moveexplosions(
            &mut lists,
            &mut sprites,
            &mut [],
            &mut [],
            &mut seed,
            &mut temps,
            None,
            &mut players,
            &mut owners,
            &mut sounds,
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
        let mut temps: Vec<crate::con::vm::ActorConTemps> = Vec::new();
        let mut players: [crate::player::Player; 0] = [];
        let mut owners: [Option<i16>; 0] = [];
        let mut sounds = Vec::new();
        moveexplosions(
            &mut lists,
            &mut sprites,
            &[],
            &[],
            &mut seed,
            &mut temps,
            None,
            &mut players,
            &mut owners,
            &mut sounds,
        );
        assert!(sprites[0].z > -20480);
        assert_eq!(temps[0].g_t[0], 1);
    }
}
