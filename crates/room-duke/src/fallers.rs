//! Tagged destructible map sprites — Chocolate `movefallers` (D5/D7).

use room_build_engine::geom::{SectorWalls, SectorZ, SpriteZClip, WallXY, movesprite_xy_at_z};
use room_build_engine::helpers::sincos_q14;
use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::con::{ActorConTemps, ScriptImage};
use crate::player::{TileZMeta, sprite_z_clip};
use crate::stat::STAT_FALLER;

const DAMAGE_TRIGGERS_FALL: [i16; 5] = [916, 2605, 1670, 1247, 1079];
const TICSPERFRAME: i32 = 4;

/// Advance map sprites registered on STAT_FALLER by Chocolate's spawn path.
/// Their state is held in the same T1/T2 and hit-extra fields as `hittype`.
pub(crate) fn move_fallers(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&ScriptImage>,
    tile_meta: &[TileZMeta],
    gc: i32,
    randomseed: &mut i32,
) {
    let sec: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sector_z: Vec<SectorZ> = sectors
        .iter()
        .map(|sector| SectorZ {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
            ceilingz: sector.ceilingz,
            floorz: sector.floorz,
            ceilingstat: sector.ceilingstat,
            floorstat: sector.floorstat,
            ceilingheinum: sector.ceilingheinum,
            floorheinum: sector.floorheinum,
        })
        .collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let mut order = Vec::new();
    let mut i = lists.headspritestat[STAT_FALLER as usize];
    while i >= 0 {
        order.push(i);
        i = lists.nextspritestat[i as usize];
    }
    for id in order {
        let index = id as usize;
        if index >= sprites.len() || temps.len() <= index {
            continue;
        }
        if lists.sprite_stat.get(index).copied() != Some(STAT_FALLER) {
            continue;
        }
        let mut sprite = sprites[index];
        let Some(sector) = usize::try_from(sprite.sectnum)
            .ok()
            .and_then(|n| sectors.get(n))
        else {
            let _ = lists.deletesprite(id);
            continue;
        };
        if temps[index].g_t[0] == 0 {
            // Chocolate probes 16 units above the sprite to distinguish a
            // damaging impact from an ordinary hit, then restores Z/angle.
            // `IFHIT` is the `ifhitbyweapon` macro: it only consumes hits
            // while actor health is nonnegative, applies damage, updates an
            // owned sprite's owner, and returns the weapon tile.
            let mut preserve_hit = false;
            let hit_tile = if temps[index].hit_extra >= 0 && sprite.extra >= 0 {
                let hit_tile = temps[index].hit_picnum;
                if temps[index].hit_extra == 0
                    && hit_tile == crate::shoot::SHRINKSPARK
                    && sprite.xrepeat < 24
                {
                    // Exact `ifhitbyweapon` early return: this narrow shrink
                    // hit is not consumed by the probe.
                    preserve_hit = true;
                    -1
                } else {
                    sprite.extra = sprite.extra.wrapping_sub(temps[index].hit_extra);
                    let owner = usize::try_from(sprite.owner).ok();
                    if sprite.picnum != crate::spawn::RECON
                        && owner
                            .and_then(|owner| sprites.get(owner))
                            .is_some_and(|owner| {
                                owner.statnum < room_build_engine::world::MAXSTATUS as i16
                            })
                    {
                        sprite.owner = temps[index].hit_owner;
                    }
                    hit_tile
                }
            } else {
                -1
            };
            let queued_hit = hit_tile >= 0;
            if !preserve_hit {
                temps[index].hit_extra = -1;
            }
            if queued_hit && DAMAGE_TRIGGERS_FALL.contains(&hit_tile) && sprite.extra <= 0 {
                temps[index].g_t[0] = 1;
                let mut other = lists.headspritestat[STAT_FALLER as usize];
                while other >= 0 {
                    let oi = other as usize;
                    other = lists.nextspritestat[oi];
                    if sprites
                        .get(oi)
                        .is_some_and(|candidate| candidate.hitag == sprite.hitag)
                    {
                        temps[oi].g_t[0] = 1;
                        sprites[oi].cstat &= !64;
                        if matches!(sprites[oi].picnum, 1255 | 1250) {
                            sprites[oi].cstat |= 32768;
                        }
                    }
                }
            } else if queued_hit && !DAMAGE_TRIGGERS_FALL.contains(&hit_tile) {
                // Chocolate's `movefallers` restores health for hits that do
                // not activate a falling sprite, while leaving a zero-damage
                // `hittype.extra` until the next IFHIT probe. `sprite` is a
                // copy that this branch never writes back, so the stored
                // health is already the health before the probe.
                temps[index].hit_extra = 0;
            }
            continue;
        }

        if sprite.lotag > 0 {
            sprite.lotag = sprite.lotag.saturating_sub(3);
            if sprite.lotag <= 0 {
                sprite.xvel = 32 + (krand_step(randomseed) & 63) as i16;
                sprite.zvel = -(1024 + (krand_step(randomseed) & 1023) as i16);
            }
        } else {
            if sprite.xvel > 0 {
                sprite.xvel = sprite.xvel.saturating_sub(8);
                let (sin, cos) = sincos_q14(i32::from(sprite.ang));
                let dx = ((i32::from(sprite.xvel) * cos >> 14) * TICSPERFRAME) >> 3;
                let dy = ((i32::from(sprite.xvel) * sin >> 14) * TICSPERFRAME) >> 3;
                let meta = tile_meta
                    .get(sprite.picnum.max(0) as usize)
                    .copied()
                    .unwrap_or_default();
                let top_offset = (i32::from(meta.height) * i32::from(sprite.yrepeat)) << 1;
                let mut clips: Vec<SpriteZClip> = sprites
                    .iter()
                    .map(|other| sprite_z_clip(other, tile_meta))
                    .collect();
                clips[index].cstat = 0;
                let dz = (i32::from(sprite.zvel) * TICSPERFRAME) >> 3;
                let old_sect = sprite.sectnum;
                let _move_result = movesprite_xy_at_z(
                    &mut sprite.x,
                    &mut sprite.y,
                    &mut sprite.z,
                    &mut sprite.sectnum,
                    dx,
                    dy,
                    dz,
                    top_offset,
                    i32::from(sprite.clipdist) << 2,
                    temps[index].hit_ceilingz,
                    temps[index].hit_floorz,
                    &sec,
                    &wall_xy,
                    Some(&sector_z),
                    &clips,
                    1,
                    Some(lists),
                );
                if sprite.sectnum >= 0 && sprite.sectnum != old_sect {
                    let _ = lists.changespritesect(id, sprite.sectnum);
                }
            }
            let sky_floor = sector.floorstat & 1 != 0
                && sector.ceilingpal == 0
                && matches!(sector.floorpicnum, 80 | 84);
            let sky_ceiling = sector.ceilingstat & 1 != 0
                && sector.ceilingpal == 0
                && matches!(sector.ceilingpicnum, 80 | 84);
            let gravity = if sky_floor {
                0
            } else if sky_ceiling {
                gc / 6
            } else {
                gc
            };
            if sprite.z < sector.floorz.saturating_sub(4 << 8) {
                sprite.zvel = i32::from(sprite.zvel).saturating_add(gravity).min(6144) as i16;
                sprite.z = sprite.z.saturating_add(i32::from(sprite.zvel));
            }
            if sector.floorz.saturating_sub(sprite.z) < 16 << 8 {
                // Chocolate: the scrap count draws first; `egs` initializes each
                // STAT_MISC actor.
                let scraps = 1 + (krand_step(randomseed) & 7);
                for _ in 0..scraps {
                    // Chocolate `RANDOMSCRAP`: 32-bit gcc evaluates the EGS arguments right
                    // to left, so zvel draws first and x last.
                    let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
                    let xvel = (64 + (krand_step(randomseed) & 63)) as i16;
                    let angle = (krand_step(randomseed) & 2047) as i16;
                    let tile = 2390 + (krand_step(randomseed) & 15) as i16;
                    let z = sprite.z - (8 << 8) - (krand_step(randomseed) & 8191);
                    let y = sprite.y + (krand_step(randomseed) & 255) - 128;
                    let x = sprite.x + (krand_step(randomseed) & 255) - 128;
                    let _ = crate::shoot::egs(
                        lists,
                        sprites,
                        picnum,
                        temps,
                        script,
                        sprite.sectnum,
                        x,
                        y,
                        z,
                        tile,
                        -8,
                        48,
                        48,
                        angle,
                        xvel,
                        zvel,
                        id,
                        crate::stat::STAT_MISC,
                    );
                }
                let _ = lists.deletesprite(id);
                continue;
            }
        }
        sprites[index] = sprite;
    }
}
