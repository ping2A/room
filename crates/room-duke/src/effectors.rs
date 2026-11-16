//! Sector effector simulation (`actors.c::moveeffectors`, D7).

use room_build_engine::geom::{
    SectorWalls, SectorZ, SpriteHit, WallXY, clipinsidebox, clipmove_xy, getzsofslope,
    hitscan_full, updatesector,
};
use room_build_engine::helpers::{getangle, length2d, rotatepoint, sincos_q14};
use room_build_engine::numeric::{krand_step, mulscale, nsqrtasm, scale};
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::{BuildWorld, SpriteLists};

use crate::con::ActorConTemps;
use crate::player::{Player, TileZMeta};
use crate::shoot::{CHAINGUN, COOLEXPLOSION1, FIRELASER, SHOTGUN, SHOTSPARK1, SPIT, egs, ldist};
use crate::stat::{STAT_EFFECTOR, STAT_PROJECTILE};

const BLOODPOOL: i16 = 1226;
const PUKE: i16 = 4389;
const FOOTPRINTS: i16 = 550;
const FOOTPRINTS2: i16 = 672;
const FOOTPRINTS3: i16 = 673;
const FOOTPRINTS4: i16 = 674;
const BULLETHOLE: i16 = 952;
const LASERLINE: i16 = 2567;
const TRIPBOMB: i16 = 2566;
const SIDEBOLT1: i16 = 4525;
const SIDEBOLT_LAST: i16 = SIDEBOLT1 + 3;
const CRANE: i16 = 1222;
const EXPLOSION2: i16 = 1890;
const SCRAP6: i16 = 2390;
const RPG: i16 = 2605;
const FREEZEBLAST: i16 = 1641;
const APLAYER: i16 = 1405;
const NATURALLIGHTNING: i16 = 4890;
const SE28_THUNDER: i16 = 352;
const SE28_LIGHTNING_SLAP: i16 = 351;
const DUKE_LONGTERM_PAIN: i16 = 211;
const SHORT_CIRCUIT: i16 = 21;

/// Advance Atomic SE36 shooters and port the Octabrain, RPG/freezer and SPIT
/// projectile attacks plus the three vanilla enemy hitscan shot tiles.
/// Chocolate calls `shoot(marker, sector.extra)` once every 130 tics while
/// the marker is active; other `shoot()` weapon families remain separate work.
pub(crate) fn move_shooter_effectors(
    world: &mut BuildWorld,
    players: &[Player],
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
) {
    let mut markers = Vec::new();
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        if world
            .sprites
            .get(index)
            .is_some_and(|sprite| sprite.lotag == 36)
        {
            markers.push(index);
        }
    }

    for index in markers {
        if index >= world.sprites.len() || temps.get(index).is_none_or(|temp| temp.g_t[0] == 0) {
            continue;
        }
        let fire = temps[index].g_t[0] == 1;
        if temps[index].g_t[0] == 26 * 5 {
            temps[index].g_t[0] = 0;
        }
        temps[index].g_t[0] = temps[index].g_t[0].wrapping_add(1);
        if !fire {
            continue;
        }
        let effect = world.sprites[index];
        let Some(sector) = usize::try_from(effect.sectnum)
            .ok()
            .and_then(|i| world.sectors.get(i))
        else {
            continue;
        };
        let attack = sector.extra;
        if effect.extra >= 0 {
            // `shoot()` marks an armed source as fully bright on each shot.
            world.sprites[index].shade = -96;
        }
        if attack == COOLEXPLOSION1 {
            // The native helper follows Chocolate's non-player projectile
            // path: source height, jitter, target opos Z and 348 velocity.
            let (target, _) = crate::fta::findplayer_dist(&effect, players);
            if players.is_empty() || target >= players.len() {
                continue;
            }
            let mut ordered_players = Vec::with_capacity(players.len());
            ordered_players.push(players[target].clone());
            ordered_players.extend(
                players
                    .iter()
                    .enumerate()
                    .filter_map(|(i, player)| (i != target).then_some(player.clone())),
            );
            let actortype = script.map_or(&[][..], |image| image.actortype.as_slice());
            crate::shoot::shoot_coolexplosion_enemy(
                &mut world.lists,
                &mut world.sprites[..],
                picnum,
                temps,
                tile_meta,
                script,
                actortype,
                &ordered_players,
                randomseed,
                index as i16,
            );
        } else if matches!(attack, SHOTSPARK1 | SHOTGUN | CHAINGUN) {
            shoot_effector_hitscan(
                world, players, temps, picnum, tile_meta, script, randomseed, index, attack,
            );
        } else if attack == SPIT {
            shoot_effector_spit(
                world, players, temps, picnum, tile_meta, script, randomseed, index,
            );
        } else if attack == FIRELASER {
            shoot_effector_firelaser(
                world, players, temps, picnum, tile_meta, script, randomseed, index,
            );
        } else if matches!(attack, RPG | FREEZEBLAST) {
            shoot_effector_rpg_or_freezer(
                world, players, temps, picnum, tile_meta, script, randomseed, index, attack, None,
            );
        }
    }
}

/// Advance Chocolate's SE28 storm cycle and linked NATURALLIGHTNING flashes.
/// The classic renderer has no global Build visibility multiplier yet, so
/// player-visible flashes use the native palette flash state instead.
pub(crate) fn move_lightning_effectors(
    world: &mut BuildWorld,
    players: &mut [Player],
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
) {
    let mut markers = Vec::new();
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        if index >= world.sprites.len() {
            break;
        }
        marker = world.lists.nextspritestat[index];
        if world.sprites[index].lotag == 28 {
            markers.push(index);
        }
    }

    for index in markers {
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if temps[index].g_t[5] > 0 {
            temps[index].g_t[5] -= 1;
            continue;
        }
        if temps[index].g_t[1] == 0 {
            let (target, distance) = crate::fta::findplayer_dist(&effect, players);
            if players.is_empty() || target >= players.len() || distance > 15_500 {
                continue;
            }
            temps[index].g_t[1] = 1;
            temps[index].g_t[2] = 64 + (krand_step(randomseed) & 511);
            temps[index].g_t[3] = 0;
            continue;
        }

        temps[index].g_t[3] = temps[index].g_t[3].saturating_add(1);
        let phase = temps[index].g_t[3];
        let duration = temps[index].g_t[2].max(1);
        if phase > duration {
            temps[index].g_t[1] = 0;
            continue;
        }
        if phase == duration / 2 {
            temps[index].pending_actor_sounds.push(SE28_THUNDER);
            continue;
        }
        if phase == duration / 8 {
            temps[index].pending_actor_sounds.push(SE28_LIGHTNING_SLAP);
            continue;
        }
        let mut natural = Vec::new();
        let mut actor = world.lists.headspritestat[0];
        while actor >= 0 {
            let actor_index = actor as usize;
            actor = world.lists.nextspritestat[actor_index];
            if world.sprites.get(actor_index).is_some_and(|sprite| {
                sprite.picnum == NATURALLIGHTNING && sprite.hitag == effect.hitag
            }) {
                natural.push(actor_index);
            }
        }
        if phase == duration / 4 {
            for natural_index in natural {
                world.sprites[natural_index].cstat |= 32768;
            }
            continue;
        }
        if phase <= duration / 8 || phase >= duration / 4 {
            continue;
        }

        let visible = players.first().is_some_and(|player| {
            let sectors = world
                .sectors
                .iter()
                .take(world.numsectors.max(0) as usize)
                .map(SectorT::z)
                .collect::<Vec<_>>();
            room_build_engine::geom::cansee(
                effect.x,
                effect.y,
                effect.z,
                effect.sectnum,
                player.posx,
                player.posy,
                player.posz,
                player.cursectnum,
                &sectors,
                &world.wall_xy(),
            )
        });
        if (krand_step(randomseed) & 255) < 192 && phase & 1 != 0 {
            if visible && let Some(player) = players.first_mut() {
                player.palette_flash = [64; 3];
                player.palette_flash_tics = 2;
            }
        }

        for natural_index in natural {
            if (krand_step(randomseed) & 31) != 0 || phase & 1 == 0 {
                world.sprites[natural_index].cstat |= 32768;
                continue;
            }
            world.sprites[natural_index].cstat &= 32767;
            let strike = world.sprites[natural_index];
            crate::shoot::egs(
                &mut world.lists,
                world.sprites.as_mut(),
                picnum,
                temps,
                script,
                strike.sectnum,
                strike.x,
                strike.y,
                strike.z,
                crate::shoot::SMALLSMOKE,
                strike.shade,
                16,
                16,
                strike.ang,
                0,
                0,
                natural_index as i16,
                crate::stat::STAT_MISC,
            );
            let target = players
                .iter()
                .enumerate()
                .filter_map(|(target, player)| {
                    usize::try_from(player.sprite_i)
                        .ok()
                        .and_then(|sprite| world.sprites.get(sprite))
                        .map(|body| (target, ldist(body, &strike)))
                })
                .min_by_key(|&(_, distance)| distance);
            if let Some((target, distance)) = target
                && distance < 768
                && let Some(player) = players.get_mut(target)
            {
                let Ok(player_sprite) = usize::try_from(player.sprite_i) else {
                    break;
                };
                if temps.len() <= player_sprite {
                    temps.resize_with(player_sprite + 1, ActorConTemps::default);
                }
                temps[player_sprite]
                    .pending_actor_sounds
                    .push(DUKE_LONGTERM_PAIN);
                temps[player_sprite]
                    .pending_actor_sounds
                    .push(SHORT_CIRCUIT);
                player.extra = player
                    .extra
                    .saturating_sub(8 + (krand_step(randomseed) & 7) as i16)
                    .max(0);
                player.last_extra = player.extra;
                player.palette_flash = [16, 0, 0];
                player.palette_flash_tics = 32;
            }
            break;
        }
    }
}

/// Advance Chocolate's SE0 rotating-sector geometry and occupants. The map
/// spawn pass links each marker to its SE1 pivot and caches the original floor
/// plane for lotag-30 rotators. Geometry follows Chocolate's direct `ms()`/`dragpoint` update.
pub(crate) fn move_rotating_sector_effectors(
    world: &mut BuildWorld,
    players: &mut [Player],
    temps: &mut Vec<ActorConTemps>,
    geometry: &[EffectorWallOffsets],
) {
    let mut markers = Vec::new();
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        if index >= world.sprites.len() || index >= world.lists.nextspritestat.len() {
            break;
        }
        marker = world.lists.nextspritestat[index];
        if world.sprites[index].lotag == 0 {
            markers.push(index);
        }
    }

    for index in markers {
        let effect = world.sprites[index];
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        let Ok(pivot_index) = usize::try_from(effect.owner) else {
            continue;
        };
        let Some(pivot) = world.sprites.get(pivot_index).copied() else {
            continue;
        };
        let timed_sector = sector.lotag & 0x3fff == 30;
        if timed_sector && !matches!(effect.extra, 1 | 3) {
            continue;
        }
        let mut rotate_this_tic = true;
        let mut transition_sound = false;
        if timed_sector {
            let Some(temp) = temps.get_mut(index) else {
                continue;
            };
            let previous_angle = temp.tempang;
            if effect.extra == 1 {
                if temp.tempang < 256 {
                    temp.tempang = temp.tempang.saturating_add(4).min(256);
                    transition_sound = previous_angle < 256 && temp.tempang == 256;
                } else {
                    rotate_this_tic = false;
                }
            } else if temp.tempang > 0 {
                temp.tempang = temp.tempang.saturating_sub(4).max(0);
                transition_sound = previous_angle > 0 && temp.tempang == 0;
            } else {
                rotate_this_tic = false;
            }
        }
        let direction = if timed_sector {
            if effect.extra == 1 {
                if effect.clipdist != 0 { 1_i32 } else { -1_i32 }
            } else if effect.clipdist != 0 {
                -1_i32
            } else {
                1_i32
            }
        } else {
            let Some(pivot_temp) = temps.get(pivot_index) else {
                continue;
            };
            if pivot_temp.g_t[0] == 2 {
                let _ = world.lists.deletesprite(index as i16);
                world.sprites[index].statnum = room_build_engine::world::MAXSTATUS as i16;
                continue;
            }
            if pivot_temp.g_t[0] == 0 {
                continue;
            }
            if pivot.ang > 1024 { -1_i32 } else { 1_i32 }
        };
        let mut turn = i32::from(sector.extra >> 3);
        if timed_sector {
            turn >>= 2;
        }
        turn = if rotate_this_tic {
            turn.saturating_mul(direction)
        } else {
            0
        };
        if let Some(temp) = temps.get_mut(index) {
            temp.g_t[2] = temp.g_t[2].wrapping_add(turn);
        }
        world.sprites[index].ang = effect.ang.wrapping_add(turn as i16) & 2047;
        if !timed_sector {
            let Some(temp) = temps.get_mut(index) else {
                continue;
            };
            if temp.g_t[3] == 0 {
                temp.g_t[3] = ldist(&effect, &pivot);
            }
            let radius = temp.g_t[3];
            world.sprites[index].xvel = radius as i16;
            let clipped_radius = i32::from(world.sprites[index].xvel);
            let (sin, cos) = sincos_q14(i32::from(world.sprites[index].ang));
            world.sprites[index].x = pivot.x.wrapping_add(clipped_radius.wrapping_mul(cos) >> 14);
            world.sprites[index].y = pivot.y.wrapping_add(clipped_radius.wrapping_mul(sin) >> 14);
        }

        let z_change = if timed_sector {
            let target = if effect.extra == 1 {
                effect.z
            } else {
                temps.get(index).map_or(sector.floorz, |temp| temp.g_t[4])
            };
            target
                .saturating_sub(world.sectors[sector_index].floorz)
                .clamp(-512, 512)
        } else {
            0
        };
        if z_change != 0 {
            world.sectors[sector_index].floorz =
                world.sectors[sector_index].floorz.saturating_add(z_change);
        }

        let start = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
        let count = usize::try_from(sector.wallnum).unwrap_or(0);
        let end = start.saturating_add(count).min(world.walls.len());
        if start >= end {
            continue;
        }
        let Some(base) = geometry.iter().find(|state| state.marker == index) else {
            continue;
        };
        let mut walls_xy: Vec<WallXY> = world.walls.iter().map(WallT::xy).collect();
        let total_angle = temps.get(index).map_or(0, |temp| temp.g_t[2] & 2047) as i16;
        for (wall_index, &(offset_x, offset_y)) in (start..end).zip(base.offsets.iter()) {
            let (rotated_x, rotated_y) = rotatepoint(0, 0, offset_x, offset_y, total_angle);
            let (x, y) = (
                world.sprites[index].x.wrapping_add(rotated_x),
                world.sprites[index].y.wrapping_add(rotated_y),
            );
            room_build_engine::helpers::dragpoint(&mut walls_xy, wall_index as i16, x, y);
        }
        for (wall, xy) in world.walls.iter_mut().zip(walls_xy) {
            wall.x = xy.x;
            wall.y = xy.y;
        }

        if sector.floorstat & 64 != 0 && rotate_this_tic {
            for player in players.iter_mut() {
                if player.cursectnum != effect.sectnum || !player.on_ground {
                    continue;
                }
                let (x, y) = rotatepoint(pivot.x, pivot.y, player.posx, player.posy, turn as i16);
                player.ang = player.ang.wrapping_add(turn as i16) & 2047;
                player.posz = player.posz.wrapping_add(z_change);
                player.bobposx = player.bobposx.wrapping_add(x.wrapping_sub(player.posx));
                player.bobposy = player.bobposy.wrapping_add(y.wrapping_sub(player.posy));
                player.posx = x;
                player.posy = y;
            }
            let mut sprite = world.lists.headspritesect[sector_index];
            while sprite >= 0 {
                let sprite_index = sprite as usize;
                if sprite_index >= world.sprites.len()
                    || sprite_index >= world.lists.nextspritesect.len()
                {
                    break;
                }
                sprite = world.lists.nextspritesect[sprite_index];
                if matches!(
                    world.lists.sprite_stat[sprite_index],
                    STAT_EFFECTOR | STAT_PROJECTILE
                ) || (world.sprites[sprite_index].picnum == APLAYER
                    && world.sprites[sprite_index].owner >= 0)
                    || world.sprites[sprite_index].picnum == LASERLINE
                {
                    continue;
                }
                let current = world.sprites[sprite_index];
                let (x, y) = rotatepoint(pivot.x, pivot.y, current.x, current.y, turn as i16);
                world.sprites[sprite_index].x = x;
                world.sprites[sprite_index].y = y;
                world.sprites[sprite_index].z = current.z.wrapping_add(z_change);
                world.sprites[sprite_index].ang = current.ang.wrapping_add(turn as i16) & 2047;
            }
        }
        if transition_sound {
            crate::interactions::call_sector_sound(world, effect.sectnum, temps);
        }
    }
}

/// Chocolate `shoot(i, RPG/FREEZEBLAST)` for SE36 and train non-player sources.
/// The effector is not a badguy, so the muzzle remains at its origin and the
/// native smart-enemy aim jitter and boss-only projectile mutations do not run.
#[allow(clippy::too_many_arguments)]
fn shoot_effector_rpg_or_freezer(
    world: &mut BuildWorld,
    players: &[Player],
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    source_index: usize,
    attack: i16,
    aim_angle: Option<i16>,
) {
    let Some(source) = world.sprites.get(source_index).copied() else {
        return;
    };
    if players.is_empty() || source.sectnum < 0 {
        return;
    }
    let (target_index, _) = crate::fta::findplayer_dist(&source, players);
    let Some(target) = players.get(target_index) else {
        return;
    };
    let Some(body_index) = usize::try_from(target.sprite_i)
        .ok()
        .filter(|&index| index < world.sprites.len())
    else {
        return;
    };
    let height = tile_meta
        .get(source.picnum.max(0) as usize)
        .map_or(0, |meta| i32::from(meta.height));
    // Chocolate `shoot`'s common actor muzzle followed by the RPG/FREEZER
    // case's extra +3<<8 adjustment for the freezer.
    let mut z = source
        .z
        .wrapping_sub((i32::from(source.yrepeat) * height) << 1)
        .wrapping_sub(3 << 8);
    if attack == FREEZEBLAST {
        z = z.wrapping_add(3 << 8);
    }
    let angle = aim_angle.unwrap_or_else(|| {
        getangle(
            target.oposx.wrapping_sub(source.x),
            target.oposy.wrapping_sub(source.y),
        ) as i16
    });
    let velocity = 644_i32;
    let distance = crate::shoot::ldist(&world.sprites[body_index], &source).max(1);
    let mut zvel = target.oposz.wrapping_sub(z).wrapping_mul(velocity) / distance;
    if attack == FREEZEBLAST {
        zvel -= 2 << 4;
    }
    let (sin, cos) = sincos_q14(i32::from(angle).wrapping_add(348));
    let x = source.x.wrapping_add(cos / 448);
    let y = source.y.wrapping_add(sin / 448);
    let projectile = egs(
        &mut world.lists,
        &mut world.sprites[..],
        picnum,
        temps,
        script,
        source.sectnum,
        x,
        y,
        z.wrapping_sub(1 << 8),
        attack,
        -127,
        14,
        14,
        angle,
        velocity as i16,
        zvel.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        source_index as i16,
        STAT_PROJECTILE,
    );
    if projectile < 0 {
        return;
    }
    let pi = projectile as usize;
    world.sprites[pi].extra = world.sprites[pi]
        .extra
        .wrapping_add((krand_step(randomseed) & 7) as i16);
    if attack == RPG {
        world.sprites[pi].yvel = -1;
        world.sprites[pi].clipdist = 4;
    } else {
        let bounce_count = script
            .map(|image| {
                let index = if image.con_version == crate::con::ConVersion::V15 {
                    26
                } else {
                    24
                };
                image.gamestartup.get(index).copied().unwrap_or(3)
            })
            .unwrap_or(3)
            .clamp(0, i32::from(i16::MAX)) as i16;
        world.sprites[pi].yvel = bounce_count;
        world.sprites[pi].xrepeat >>= 1;
        world.sprites[pi].yrepeat >>= 1;
        world.sprites[pi].clipdist = 40;
    }
    world.sprites[pi].cstat = 128;
}

/// Chocolate `shoot(i, SPIT)` enemy launch, excluding only target-selection
/// presentation side effects; the projectile itself is native and mutable.
#[allow(clippy::too_many_arguments)]
fn shoot_effector_spit(
    world: &mut BuildWorld,
    players: &[Player],
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    source_index: usize,
) {
    let Some(source) = world.sprites.get(source_index).copied() else {
        return;
    };
    if players.is_empty() || source.sectnum < 0 {
        return;
    }
    let (target_index, _) = crate::fta::findplayer_dist(&source, players);
    let Some(target) = players.get(target_index) else {
        return;
    };
    let Some(body_index) = usize::try_from(target.sprite_i)
        .ok()
        .filter(|&index| index < world.sprites.len())
    else {
        return;
    };
    let height = tile_meta
        .get(source.picnum.max(0) as usize)
        .map_or(0, |meta| i32::from(meta.height));
    let z = source.z - ((i32::from(source.yrepeat) * height) << 1) - (13 << 8);
    let mut angle = source.ang;
    angle = angle.wrapping_add(16 - (krand_step(randomseed) & 31) as i16);
    let velocity = 292_i32;
    let zvel = (target.oposz.wrapping_sub(z).wrapping_add(3 << 8) * velocity)
        / ldist(&world.sprites[body_index], &source).max(1);
    let projectile = egs(
        &mut world.lists,
        &mut world.sprites[..],
        picnum,
        temps,
        script,
        source.sectnum,
        source.x,
        source.y,
        z,
        SPIT,
        -127,
        18,
        18,
        angle,
        velocity as i16,
        zvel.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        source_index as i16,
        STAT_PROJECTILE,
    );
    if projectile < 0 {
        return;
    }
    let pi = projectile as usize;
    let strength = script
        .and_then(|image| image.actorscrptr.get(SPIT as usize))
        .and_then(|entry| usize::try_from(*entry).ok())
        .and_then(|entry| script.and_then(|image| image.script.get(entry)).copied())
        .unwrap_or(0);
    world.sprites[pi].extra = (strength as i16).wrapping_add((krand_step(randomseed) & 7) as i16);
    world.sprites[pi].cstat = 128;
    world.sprites[pi].clipdist = 4;
}

/// Chocolate `shoot(i, FIRELASER)` for a non-player sector effector.
#[allow(clippy::too_many_arguments)]
fn shoot_effector_firelaser(
    world: &mut BuildWorld,
    players: &[Player],
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    source_index: usize,
) {
    let Some(source) = world.sprites.get(source_index).copied() else {
        return;
    };
    if players.is_empty() || source.sectnum < 0 {
        return;
    }
    let (target_index, _) = crate::fta::findplayer_dist(&source, players);
    let Some(target) = players.get(target_index) else {
        return;
    };
    let Some(body_index) = usize::try_from(target.sprite_i)
        .ok()
        .filter(|&index| index < world.sprites.len())
    else {
        return;
    };
    let height = tile_meta
        .get(source.picnum.max(0) as usize)
        .map_or(0, |meta| i32::from(meta.height));
    // `shoot`: initial actor muzzle is +4<<8 then -7<<8; FIRELASER subtracts
    // another 4<<7, giving source.z - actor-height - 5<<8.
    let z = source.z - ((i32::from(source.yrepeat) * height) << 1) - (5 << 8);
    let (sin, cos) =
        room_build_engine::helpers::sincos_q14(i32::from(source.ang.wrapping_add(96)) & 2047);
    let (x, y) = if source.picnum != 1920 {
        (source.x - (sin >> 7), source.y + (cos >> 7))
    } else {
        (source.x, source.y)
    };
    let angle = source
        .ang
        .wrapping_add(16 - (krand_step(randomseed) & 31) as i16);
    let velocity = 840_i32;
    let zvel = ((target.oposz.wrapping_sub(z).wrapping_add(3 << 8)) * velocity)
        / ldist(&world.sprites[body_index], &source).max(1);
    let projectile = egs(
        &mut world.lists,
        &mut world.sprites[..],
        picnum,
        temps,
        script,
        source.sectnum,
        x,
        y,
        z,
        FIRELASER,
        -127,
        18,
        18,
        angle,
        velocity as i16,
        zvel.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        source_index as i16,
        STAT_PROJECTILE,
    );
    if projectile < 0 {
        return;
    }
    let pi = projectile as usize;
    let strength = script
        .and_then(|image| image.actorscrptr.get(FIRELASER as usize))
        .and_then(|entry| usize::try_from(*entry).ok())
        .and_then(|entry| script.and_then(|image| image.script.get(entry)).copied())
        .unwrap_or(0);
    world.sprites[pi].extra = (strength as i16).wrapping_add((krand_step(randomseed) & 7) as i16);
    world.sprites[pi].cstat = 128;
    world.sprites[pi].clipdist = 4;
}

/// Chocolate `shoot(i, SHOTSPARK1|SHOTGUN|CHAINGUN)` for a non-player source.
#[allow(clippy::too_many_arguments)]
fn shoot_effector_hitscan(
    world: &mut BuildWorld,
    players: &[Player],
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    source_index: usize,
    attack: i16,
) {
    let Some(source) = world.sprites.get(source_index).copied() else {
        return;
    };
    if players.is_empty() || source.sectnum < 0 {
        return;
    }
    let (target_index, _) = crate::fta::findplayer_dist(&source, players);
    let Some(target) = players.get(target_index) else {
        return;
    };
    let Some(body_index) = usize::try_from(target.sprite_i)
        .ok()
        .filter(|&index| index < world.sprites.len())
    else {
        return;
    };
    let height = tile_meta
        .get(source.picnum.max(0) as usize)
        .map_or(0, |meta| i32::from(meta.height));
    let sx = source.x;
    let sy = source.y;
    let mut sz = source.z - ((i32::from(source.yrepeat) * height) << 1) + (4 << 8);
    sz -= 7 << 8;
    sz -= 4 << 8;
    let body = world.sprites[body_index];
    let distance = crate::shoot::ldist(&body, &source).max(1);
    let mut angle = source.ang;
    let mut zvel = ((target.posz - sz) << 8) / distance;
    zvel = zvel.wrapping_add(128 - (krand_step(randomseed) & 255));
    angle = angle.wrapping_add(32 - (krand_step(randomseed) & 63) as i16);

    let sectors: Vec<SectorWalls> = world.sectors.iter().map(|sector| sector.walls()).collect();
    let sector_z: Vec<SectorZ> = world.sectors.iter().map(SectorT::z).collect();
    let walls: Vec<WallXY> = world.walls.iter().map(WallT::xy).collect();
    let sprite_hits: Vec<SpriteHit> = world
        .sprites
        .iter()
        .enumerate()
        .map(|(index, sprite)| {
            let mut hit = SpriteHit::cylinder(
                sprite.x,
                sprite.y,
                sprite.z,
                sprite.sectnum,
                i32::from(sprite.clipdist) << 2,
                sprite.cstat,
            );
            if index == source_index {
                hit.cstat &= !257;
            }
            hit
        })
        .collect();
    let (sin, cos) = sincos_q14(i32::from(angle) & 2047);
    let hit = hitscan_full(
        sx,
        sy,
        sz,
        source.sectnum,
        cos,
        sin,
        zvel.wrapping_shl(6),
        (256 << 16) + 64,
        &sectors,
        &walls,
        &sprite_hits,
        Some(&sector_z),
    );

    // Chocolate creates the spark on all valid sector hits and takes damage
    // from the attack tile's CON actor-header strength.
    let strength = script
        .and_then(|image| image.actorscrptr.get(attack.max(0) as usize))
        .and_then(|entry| usize::try_from(*entry).ok())
        .and_then(|entry| script.and_then(|image| image.script.get(entry)).copied())
        .unwrap_or(0);
    if hit.hitsect >= 0 {
        let spark = egs(
            &mut world.lists,
            &mut world.sprites[..],
            picnum,
            temps,
            script,
            hit.hitsect,
            hit.hitx,
            hit.hity,
            hit.hitz,
            SHOTSPARK1,
            -15,
            24,
            24,
            angle,
            0,
            0,
            source_index as i16,
            STAT_PROJECTILE,
        );
        if spark >= 0 {
            world.sprites[spark as usize].extra = strength as i16;
        }
    }
    if hit.hitsprite >= 0 {
        let target_index = hit.hitsprite as usize;
        if target_index < world.sprites.len() {
            let damage = strength.clamp(0, i32::from(i16::MAX)) as i16;
            if temps.len() <= target_index {
                temps.resize_with(target_index + 1, ActorConTemps::default);
            }
            let temp = &mut temps[target_index];
            temp.hit_picnum = SHOTSPARK1;
            temp.hit_extra = if temp.hit_extra < 0 {
                damage
            } else {
                temp.hit_extra.saturating_add(damage)
            };
            temp.hit_owner = source_index as i16;
            temp.hit_ang = angle;
        }
    }
    // `shoot()` checks the ricochet roll after impact regardless of hit kind.
    let _ = krand_step(randomseed);
}

/// Chocolate `SE29` continuously displaces the sector floor with the Build
/// sine table. The following slope pass is performed after all effectors.
pub(crate) fn move_sine_floor_effectors(world: &mut BuildWorld) {
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if effect.lotag != 29 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get_mut(sector_index) else {
            continue;
        };
        world.sprites[index].hitag = world.sprites[index].hitag.wrapping_add(64);
        let (sin, _) = sincos_q14(i32::from(world.sprites[index].hitag));
        let displacement = mulscale(i32::from(effect.yvel), sin, 12);
        sector.floorz = effect.z.wrapping_add(displacement);
    }
}

/// Chocolate's post-effector `Sloped sin-wave floors!` pass uses wall 2 to
/// align each four-wall SE29 sector's floor to its neighboring sector.
pub(crate) fn align_sine_floor_slopes(world: &mut BuildWorld) {
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if effect.lotag != 29 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        if sector.wallnum != 4 {
            continue;
        }
        let wall_index = sector.wallptr.saturating_add(2) as usize;
        let Some(wall) = world.walls.get(wall_index).copied() else {
            continue;
        };
        let Ok(neighbor_index) = usize::try_from(wall.nextsector) else {
            continue;
        };
        let Some(neighbor) = world.sectors.get(neighbor_index) else {
            continue;
        };
        let Some(first_wall) = world.walls.get(sector.wallptr.max(0) as usize) else {
            continue;
        };
        let Some(point2) = usize::try_from(first_wall.point2)
            .ok()
            .and_then(|i| world.walls.get(i))
        else {
            continue;
        };
        let dx = point2.x.wrapping_sub(first_wall.x);
        let dy = point2.y.wrapping_sub(first_wall.y);
        let cross = (effect.y.wrapping_sub(first_wall.y))
            .wrapping_mul(dx)
            .wrapping_sub((effect.x.wrapping_sub(first_wall.x)).wrapping_mul(dy));
        if cross == 0 {
            continue;
        }
        let length = nsqrtasm(dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)) as u32);
        let rise = neighbor.floorz.wrapping_sub(sector.floorz).wrapping_shl(8);
        let heinum = scale(rise, length, cross) as i16;
        let target = &mut world.sectors[sector_index];
        target.floorheinum = heinum;
        if heinum == 0 {
            target.floorstat &= !2;
        } else {
            target.floorstat |= 2;
        }
    }
}

/// Map-load relative wall coordinates used by Chocolate's `ms()` for moving
/// effector sectors.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct EffectorWallOffsets {
    marker: usize,
    offsets: Vec<(i32, i32)>,
}

/// Capture moving effector wall offsets from each marker's map-load position.
pub(crate) fn init_sliding_door_offsets(world: &BuildWorld) -> Vec<EffectorWallOffsets> {
    let mut offsets = Vec::new();
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if !matches!(effect.lotag, 0 | 5 | 6 | 11 | 14 | 15 | 26 | 30) {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index) else {
            continue;
        };
        let Ok(start) = usize::try_from(sector.wallptr) else {
            continue;
        };
        let Ok(count) = usize::try_from(sector.wallnum) else {
            continue;
        };
        let end = start.saturating_add(count).min(world.walls.len());
        offsets.push(EffectorWallOffsets {
            marker: index,
            offsets: world.walls[start..end]
                .iter()
                .map(|wall| (wall.x.wrapping_sub(effect.x), wall.y.wrapping_sub(effect.y)))
                .collect(),
        });
    }
    offsets
}

/// Initialize moving-sector effectors and cache their adjacent locator/linked
/// sector (`Chocolate game.c::spawn`, SE5/6/14/30).
pub(crate) fn init_nonrotating_subways(world: &mut BuildWorld, temps: &mut Vec<ActorConTemps>) {
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        if world
            .sprites
            .get(index)
            .is_none_or(|sprite| !matches!(sprite.lotag, 5 | 6 | 14 | 30))
        {
            continue;
        }
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        world.sprites[index].owner = -1;
        temps[index].g_t[0] = i32::from(sector_index as i16);
        if effect.lotag == 5 {
            // SE5 keeps zero-initialized T3/T4/T5; T1 below stores the
            // adjacent sector selected by Chocolate's common effector setup.
        } else if effect.lotag == 30 {
            world.sprites[index].xvel = 0;
            world.sprites[index].extra = if sector.hitag == -1 { 0 } else { 1 };
            temps[index].g_t[3] = 0; // T4 route selector.
            temps[index].g_t[4] = 0; // T5 motion state.
        } else {
            temps[index].g_t[2] = 0; // T3 accumulated turn angle.
            temps[index].g_t[3] = 0; // T4 locator route.
            temps[index].g_t[4] = i32::from(effect.hitag); // T5 speed ramp timer.
            temps[index].g_t[5] = 0; // T6 sound/spacing state.
            temps[index].lastvx = 75; // SUBWAY fallback from sounds.h.
            temps[index].lastvy = 0; // Train loop sound is initially stopped.
        }
        temps[index].tempang = 0;
        world.sectors[sector_index].hitag = index as i16;

        let start = sector.wallptr.max(0) as usize;
        let end = start.saturating_add(sector.wallnum.max(0) as usize);
        let mut locator_sector = None;
        for wall in world
            .walls
            .get(start..end.min(world.walls.len()))
            .unwrap_or_default()
        {
            let Ok(neighbor) = usize::try_from(wall.nextsector) else {
                continue;
            };
            if world
                .sectors
                .get(neighbor)
                .is_some_and(|candidate| candidate.hitag == 0 && candidate.lotag < 3)
            {
                locator_sector = Some(neighbor);
                break;
            }
        }
        if let Some(locator_sector) = locator_sector {
            temps[index].g_t[0] = locator_sector as i32;
        }
    }
}

/// Capture the native SE5 boss controller's initial owner sentinel.
pub(crate) fn init_boss_effectors(world: &mut BuildWorld) {
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        if world
            .sprites
            .get(index)
            .is_some_and(|sprite| sprite.lotag == 5)
        {
            world.sprites[index].owner = -1;
        }
    }
}

/// Apply Chocolate's SE18 map-load plane setup and convert its hitag timer
/// from map units to the per-tic countdown (`game.c::spawn`).
pub(crate) fn init_kroz_effectors(world: &mut BuildWorld, temps: &mut Vec<ActorConTemps>) {
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if effect.lotag != 18 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        if effect.ang == 512 {
            temps[index].g_t[1] = sector.ceilingz;
            if effect.pal != 0 {
                world.sectors[sector_index].ceilingz = effect.z;
            }
        } else {
            temps[index].g_t[1] = sector.floorz;
            if effect.pal != 0 {
                world.sectors[sector_index].floorz = effect.z;
            }
        }
        world.sprites[index].hitag = effect.hitag.wrapping_mul(4);
    }
}

fn apply_effector_wall_rotation(
    world: &mut BuildWorld,
    state: &EffectorWallOffsets,
    pivot_x: i32,
    pivot_y: i32,
    rotation: i32,
) {
    let Some(marker) = world.sprites.get(state.marker) else {
        return;
    };
    let Ok(sector_index) = usize::try_from(marker.sectnum) else {
        return;
    };
    let Some(sector) = world.sectors.get(sector_index) else {
        return;
    };
    let Ok(start) = usize::try_from(sector.wallptr) else {
        return;
    };
    let (sin, cos) = sincos_q14(rotation.rem_euclid(2048));
    for (offset_index, (ox, oy)) in state.offsets.iter().copied().enumerate() {
        let wall = start.saturating_add(offset_index);
        if wall >= world.walls.len() {
            break;
        }
        let rx = (ox * cos - oy * sin) >> 14;
        let ry = (ox * sin + oy * cos) >> 14;
        crate::interactions::dragpoint_compat(
            &mut world.walls,
            wall,
            pivot_x.wrapping_add(rx),
            pivot_y.wrapping_add(ry),
        );
    }
}

/// Chocolate `ms(i); setsprite(i, ...)` for an SE11 swing door. `ms` first
/// advances the controller by its xvel/angle, then rotates the cached map
/// wall offsets; `setsprite` refreshes the marker's Build sector membership.
fn move_swing_door_geometry(world: &mut BuildWorld, state: &EffectorWallOffsets, rotation: i32) {
    let Some(effect) = world.sprites.get(state.marker).copied() else {
        return;
    };
    let angle = i32::from(effect.ang).rem_euclid(2048);
    let (sin_x, _) = sincos_q14(angle + 512);
    let (sin_y, _) = sincos_q14(angle);
    let x = effect
        .x
        .wrapping_add((i32::from(effect.xvel) * sin_x) >> 14);
    let y = effect
        .y
        .wrapping_add((i32::from(effect.xvel) * sin_y) >> 14);
    world.sprites[state.marker].x = x;
    world.sprites[state.marker].y = y;
    apply_effector_wall_rotation(world, state, x, y, rotation);

    let sector_walls: Vec<_> = world
        .sectors
        .iter()
        .take(world.numsectors.max(0) as usize)
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<_> = world
        .walls
        .iter()
        .take(world.numwalls.max(0) as usize)
        .map(WallT::xy)
        .collect();
    let mut sector = effect.sectnum;
    if updatesector(x, y, &mut sector, &sector_walls, &wall_xy) >= 0 && sector != effect.sectnum {
        world.sprites[state.marker].sectnum = sector;
        let _ = world.lists.changespritesect(state.marker as i16, sector);
    }
}

fn apply_effector_wall_translation(
    world: &mut BuildWorld,
    state: &EffectorWallOffsets,
    marker_x: i32,
    marker_y: i32,
) {
    let Some(marker) = world.sprites.get(state.marker) else {
        return;
    };
    let Ok(sector_index) = usize::try_from(marker.sectnum) else {
        return;
    };
    let Some(sector) = world.sectors.get(sector_index) else {
        return;
    };
    let Ok(start) = usize::try_from(sector.wallptr) else {
        return;
    };
    for (offset_index, (ox, oy)) in state.offsets.iter().copied().enumerate() {
        let wall = start.saturating_add(offset_index);
        if wall >= world.walls.len() {
            break;
        }
        crate::interactions::dragpoint_compat(
            &mut world.walls,
            wall,
            marker_x.wrapping_add(ox),
            marker_y.wrapping_add(oy),
        );
    }
}

/// Advance Atomic's SE26 escalator sector and its grounded riders.
pub(crate) fn move_escalator_effectors(
    world: &mut BuildWorld,
    players: &mut [Player],
    temps: &mut Vec<ActorConTemps>,
    wall_offsets: &[EffectorWallOffsets],
    tile_meta: &[TileZMeta],
    gravity: i32,
) {
    let sectors: Vec<SectorWalls> = world
        .sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let walls: Vec<WallXY> = world.walls.iter().map(WallT::xy).collect();
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if effect.lotag != 26 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        if sector_index >= world.sectors.len() {
            continue;
        }
        world.sprites[index].xvel = 32;
        let (sin_x, _) = sincos_q14(i32::from(effect.ang.wrapping_add(512)));
        let (sin_y, _) = sincos_q14(i32::from(effect.ang));
        let dx = (i32::from(world.sprites[index].xvel) * sin_x) >> 14;
        let dy = (i32::from(world.sprites[index].xvel) * sin_y) >> 14;
        world.sprites[index].shade = world.sprites[index].shade.saturating_add(1);
        if world.sprites[index].shade > 7 {
            let shade = i32::from(world.sprites[index].shade);
            let correction = i32::from(effect.zvel)
                .saturating_mul(shade)
                .saturating_sub(i32::from(effect.zvel));
            world.sectors[sector_index].floorz = world.sectors[sector_index]
                .floorz
                .saturating_sub(correction);
            world.sprites[index].shade = 0;
            if temps.len() <= index {
                temps.resize_with(index + 1, ActorConTemps::default);
            }
            world.sprites[index].x = temps[index].g_t[3];
            world.sprites[index].y = temps[index].g_t[4];
        } else {
            world.sectors[sector_index].floorz = world.sectors[sector_index]
                .floorz
                .saturating_add(i32::from(effect.zvel));
        }

        let mut actor = world.lists.headspritesect[sector_index];
        while actor >= 0 {
            let actor_index = actor as usize;
            actor = world.lists.nextspritesect[actor_index];
            if world
                .lists
                .sprite_stat
                .get(actor_index)
                .is_none_or(|status| {
                    *status == STAT_EFFECTOR || *status == crate::stat::STAT_PLAYER
                })
            {
                continue;
            }
            if temps.len() <= actor_index {
                temps.resize_with(actor_index + 1, ActorConTemps::default);
            }
            temps[actor_index].bposx = world.sprites[actor_index].x;
            temps[actor_index].bposy = world.sprites[actor_index].y;
            world.sprites[actor_index].x = world.sprites[actor_index].x.wrapping_add(dx);
            world.sprites[actor_index].y = world.sprites[actor_index].y.wrapping_add(dy);
            world.sprites[actor_index].z = world.sprites[actor_index]
                .z
                .wrapping_add(i32::from(effect.zvel));
            let old_sector = world.sprites[actor_index].sectnum;
            let mut new_sector = old_sector;
            let _ = updatesector(
                world.sprites[actor_index].x,
                world.sprites[actor_index].y,
                &mut new_sector,
                &sectors,
                &walls,
            );
            if new_sector >= 0 && new_sector != old_sector {
                world.sprites[actor_index].sectnum = new_sector;
                let _ = world.lists.changespritesect(actor_index as i16, new_sector);
            }
            // Chocolate calls makeitfall for moved active actors after
            // setsprite when they land in a sector with a sloped floor.
            let actor_sector = world.sprites[actor_index].sectnum;
            if world
                .sectors
                .get(actor_sector.max(0) as usize)
                .is_some_and(|sector| sector.floorstat & 2 != 0)
                && world.lists.sprite_stat.get(actor_index).copied()
                    == Some(crate::stat::STAT_ACTOR)
            {
                let peer_snapshot = world.sprites.to_vec();
                crate::spawn::makeitfall(
                    &mut world.sprites[actor_index],
                    &world.sectors,
                    &world.walls,
                    &peer_snapshot,
                    tile_meta,
                    gravity,
                    true,
                );
            }
        }

        for player in players.iter_mut() {
            // Chocolate tests the APLAYER sprite's sector (`sprite[ps.i]`),
            // not the cached `cursectnum`, for both conveyor force and
            // escalator rider height. These can diverge briefly during
            // portal/effector updates.
            let body_in_sector = usize::try_from(player.sprite_i)
                .ok()
                .and_then(|body| world.sprites.get(body))
                .is_some_and(|body| body.sectnum == effect.sectnum);
            if body_in_sector && player.on_ground {
                player.posz = player.posz.wrapping_add(i32::from(effect.zvel));
                player.effector_force_x = player.effector_force_x.saturating_add(dx << 5);
                player.effector_force_y = player.effector_force_y.saturating_add(dy << 5);
                crate::player::sync_player_sprite(player, world.sprites.as_mut());
            }
        }

        world.sprites[index].x = world.sprites[index].x.wrapping_add(dx);
        world.sprites[index].y = world.sprites[index].y.wrapping_add(dy);
        let old_sector = world.sprites[index].sectnum;
        let mut new_sector = old_sector;
        let _ = updatesector(
            world.sprites[index].x,
            world.sprites[index].y,
            &mut new_sector,
            &sectors,
            &walls,
        );
        if new_sector >= 0 && new_sector != old_sector {
            world.sprites[index].sectnum = new_sector;
            let _ = world.lists.changespritesect(index as i16, new_sector);
        }
        if let Some(state) = wall_offsets.iter().find(|state| state.marker == index) {
            apply_effector_wall_translation(
                world,
                state,
                world.sprites[index].x,
                world.sprites[index].y,
            );
        }
    }
}

/// Tick Atomic's sector-lotag-25 / SE15 translating sliding doors.
pub(crate) fn move_sliding_door_effectors(
    world: &mut BuildWorld,
    temps: &mut Vec<ActorConTemps>,
    geometry: &[EffectorWallOffsets],
) {
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if effect.lotag != 15 {
            continue;
        }
        let Some(state) = geometry.iter().find(|state| state.marker == index) else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        let mode = temps[index].g_t[4]; // Chocolate T5: 1 opening, 2 closing.
        if mode == 0 {
            continue;
        }
        // Chocolate's moveeffectors sets SE15 xvel on each active tic
        // before `ms()` translates the marker and its sector walls.
        world.sprites[index].xvel = 16;
        let max_angle = i32::from(effect.yvel) >> 3;
        if mode == 1 {
            if temps[index].g_t[3] >= max_angle {
                temps[index].g_t[4] = 0;
                crate::interactions::call_sector_sound(world, effect.sectnum, temps);
                continue;
            }
            temps[index].g_t[3] += 1;
        } else if mode == 2 {
            if temps[index].g_t[3] < 1 {
                temps[index].g_t[4] = 0;
                crate::interactions::call_sector_sound(world, effect.sectnum, temps);
                continue;
            }
            temps[index].g_t[3] -= 1;
        } else {
            continue;
        }

        let marker_angle = i32::from(effect.ang).rem_euclid(2048);
        let (move_sin, move_cos) = sincos_q14(marker_angle);
        let x = world.sprites[index].x.wrapping_add((16 * move_cos) >> 14);
        let y = world.sprites[index].y.wrapping_add((16 * move_sin) >> 14);
        world.sprites[index].x = x;
        world.sprites[index].y = y;
        apply_effector_wall_translation(world, state, x, y);
        let mut new_sector = effect.sectnum;
        let sectors: Vec<SectorWalls> = world
            .sectors
            .iter()
            .map(|sector| SectorWalls {
                wallptr: sector.wallptr,
                wallnum: sector.wallnum,
            })
            .collect();
        let wall_xy: Vec<WallXY> = world.walls.iter().map(WallT::xy).collect();
        let _ = updatesector(x, y, &mut new_sector, &sectors, &wall_xy);
        if new_sector >= 0 && new_sector != effect.sectnum {
            world.sprites[index].sectnum = new_sector;
            let _ = world.lists.changespritesect(index as i16, new_sector);
        }
    }
}

/// Tick Atomic's SE11 rotating swing doors, including Chocolate's actor and
/// player `clipinsidebox` obstruction checks and delayed player recoil.
pub(crate) fn move_swing_door_effectors(
    world: &mut BuildWorld,
    temps: &mut Vec<ActorConTemps>,
    geometry: &[EffectorWallOffsets],
    actor_types: &[u8],
) {
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if effect.lotag != 11 {
            continue;
        }
        let Some(state) = geometry.iter().find(|state| state.marker == index) else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        if temps[index].g_t[5] > 0 {
            temps[index].g_t[5] -= 1;
            continue;
        }
        if temps[index].g_t[4] == 0 {
            continue;
        }

        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index) else {
            continue;
        };
        let Ok(wall_start) = usize::try_from(sector.wallptr) else {
            continue;
        };
        let Ok(wall_count) = usize::try_from(sector.wallnum) else {
            continue;
        };
        let wall_end = wall_start.saturating_add(wall_count).min(world.walls.len());
        let wall_xy: Vec<WallXY> = world.walls.iter().map(WallT::xy).collect();
        let mut badguy_blocked = false;
        'walls: for wall in wall_start..wall_end {
            let mut actor = world.lists.headspritestat[crate::stat::STAT_ACTOR as usize];
            while actor >= 0 {
                let actor_index = actor as usize;
                actor = world.lists.nextspritestat[actor_index];
                let Some(sprite) = world.sprites.get(actor_index) else {
                    continue;
                };
                if sprite.extra > 0
                    && crate::badguy::badguy_pic(sprite.picnum, actor_types)
                    && clipinsidebox(sprite.x, sprite.y, wall as i16, 256, &wall_xy) == 1
                {
                    badguy_blocked = true;
                    break 'walls;
                }
            }
        }
        if badguy_blocked {
            continue;
        }

        let step = (i32::from(effect.yvel) >> 3).saturating_mul(temps[index].g_t[3]);
        let mut player_blocked = false;
        'player_walls: for wall in wall_start..wall_end {
            let mut player = world.lists.headspritestat[crate::stat::STAT_PLAYER as usize];
            while player >= 0 {
                let player_index = player as usize;
                player = world.lists.nextspritestat[player_index];
                let Some(sprite) = world.sprites.get(player_index) else {
                    continue;
                };
                if sprite.owner >= 0
                    && clipinsidebox(sprite.x, sprite.y, wall as i16, 144, &wall_xy) == 1
                {
                    temps[index].g_t[5] = 8;
                    temps[index].g_t[2] = temps[index].g_t[2].wrapping_sub(step);
                    temps[index].g_t[4] = temps[index].g_t[4].wrapping_sub(step);
                    move_swing_door_geometry(world, state, temps[index].g_t[2]);
                    player_blocked = true;
                    break 'player_walls;
                }
            }
        }
        if player_blocked {
            continue;
        }

        temps[index].g_t[2] = temps[index].g_t[2].wrapping_add(step);
        temps[index].g_t[4] = temps[index].g_t[4].wrapping_add(step);
        move_swing_door_geometry(world, state, temps[index].g_t[2]);
        if temps[index].g_t[4] <= -511 || temps[index].g_t[4] >= 512 {
            temps[index].g_t[4] = 0;
            temps[index].g_t[2] &= !255;
            // Chocolate runs `ms` and `setsprite` again after snapping the
            // turn angle to its exact endpoint.
            move_swing_door_geometry(world, state, temps[index].g_t[2]);
        }
    }
}

/// Chocolate `quickkill`: flash the player's palette and emit eight JIBS6
/// unless god mode is active. The caller has already placed the body at the
/// train marker, matching `quickkill(&ps[p])`'s input sprite.
fn train_quickkill_effects(
    world: &mut BuildWorld,
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    player: &mut Player,
    body_index: usize,
    sector_z: &[SectorZ],
    wall_xy: &[WallXY],
) {
    player.palette_flash = [48; 3];
    player.palette_flash_tics = 48;
    if player.god_mode {
        return;
    }
    let body = world.sprites[body_index];
    let floor_z = getzsofslope(body.sectnum, body.x, body.y, sector_z, wall_xy).1;
    let mut guts_z = body.z.saturating_sub(8 << 8);
    if guts_z > floor_z.saturating_sub(8 << 8) {
        guts_z = floor_z.saturating_sub(8 << 8);
    }
    for _ in 0..8 {
        // Chocolate `guts`: `a = TRAND&2047;` first, then the EGS arguments
        // right to left (32-bit gcc): zvel, xvel, z, y, x.
        let angle = (krand_step(randomseed) & 2047) as i16;
        let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
        let xvel = (48 + (krand_step(randomseed) & 31)) as i16;
        let z = guts_z.wrapping_sub(krand_step(randomseed) & 8191);
        let y = body
            .y
            .wrapping_add(krand_step(randomseed) & 255)
            .wrapping_sub(128);
        let x = body
            .x
            .wrapping_add(krand_step(randomseed) & 255)
            .wrapping_sub(128);
        let _ = egs(
            &mut world.lists,
            world.sprites.as_mut(),
            picnum,
            temps,
            script,
            body.sectnum,
            x,
            y,
            z,
            crate::shoot::JIBS6,
            -32,
            32,
            32,
            angle,
            xvel,
            zvel,
            player.sprite_i,
            crate::stat::STAT_MISC,
        );
    }
}

/// Chocolate checks cramped subway collisions both before and after moving
/// the train. `sync_previous` is true on the post-move pass, where the source
/// also snaps `oposx/oposy` to the quickkill position.
#[allow(clippy::too_many_arguments)]
fn quickkill_train_players(
    world: &mut BuildWorld,
    players: &mut [Player],
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    sector_walls: &[SectorWalls],
    wall_xy: &[WallXY],
    sector_z: &[SectorZ],
    marker_x: i32,
    marker_y: i32,
    marker_z: i32,
    train_sector: i16,
    clipping: bool,
    sync_previous: bool,
) {
    if clipping {
        return;
    }
    for player in players {
        let Some(body_index) = usize::try_from(player.sprite_i)
            .ok()
            .filter(|&body| body < world.sprites.len())
        else {
            continue;
        };
        if world.sprites[body_index].extra <= 0 {
            continue;
        }
        let mut actual_sector = player.cursectnum;
        let found_sector = updatesector(
            player.posx,
            player.posy,
            &mut actual_sector,
            sector_walls,
            wall_xy,
        );
        if found_sector >= 0 && (actual_sector != train_sector || player.cursectnum == train_sector)
        {
            continue;
        }
        player.posx = marker_x;
        player.posy = marker_y;
        if sync_previous {
            player.oposx = marker_x;
            player.oposy = marker_y;
        }
        player.cursectnum = train_sector;
        player.extra = 0;
        player.last_extra = 0;
        {
            let body = &mut world.sprites[body_index];
            body.x = marker_x;
            body.y = marker_y;
            body.z = marker_z;
            body.sectnum = train_sector;
            body.extra = 0;
            body.cstat |= 32768;
        }
        let _ = world.lists.changespritesect(player.sprite_i, train_sector);
        train_quickkill_effects(
            world, temps, picnum, script, randomseed, player, body_index, sector_z, wall_xy,
        );
    }
}

/// Advance Atomic's SE30 subway car along its two locator routes. This ports
/// the linear acceleration/braking, tagged activators, force-field toggles,
/// moving-sector occupant carry and narrow-gap crushing.
pub(crate) fn move_nonrotating_subways(
    world: &mut BuildWorld,
    players: &mut [Player],
    temps: &mut Vec<ActorConTemps>,
    geometry: &[EffectorWallOffsets],
    motions: &mut Vec<crate::interactions::SectorMotion>,
    wall_motions: &mut Vec<crate::interactions::WallMotion>,
    cyclers: &mut [crate::prelevel::ShadeCycler],
    anim_walls: &mut [crate::wall_animations::AnimWall],
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    clipping: bool,
    monsters_off: bool,
    actor_types: &[u8],
) {
    let multiplayer = players.len() > 1;
    let sector_z: Vec<_> = world
        .sectors
        .iter()
        .take(world.numsectors as usize)
        .map(SectorT::z)
        .collect();
    let sector_walls: Vec<_> = sector_z
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<_> = world
        .walls
        .iter()
        .take(world.numwalls as usize)
        .map(WallT::xy)
        .collect();
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if effect.lotag != 30 {
            continue;
        }
        let Some(state) = geometry.iter().find(|state| state.marker == index) else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        if world.sprites[index].owner < 0 {
            let route = if temps[index].g_t[3] == 0 { 1 } else { 0 };
            temps[index].g_t[3] = route;
            let sector = temps[index].g_t[0] as i16;
            let mut locator = world.lists.headspritestat[crate::stat::STAT_LOCATOR as usize];
            let mut found = -1;
            while locator >= 0 {
                let locator_index = locator as usize;
                locator = world.lists.nextspritestat[locator_index];
                if world
                    .sprites
                    .get(locator_index)
                    .is_some_and(|sprite| sprite.lotag == route as i16 && sprite.sectnum == sector)
                {
                    found = locator_index as i16;
                    break;
                }
            }
            world.sprites[index].owner = found;
            if found < 0 {
                continue;
            }
        }
        let Some(target) = usize::try_from(world.sprites[index].owner)
            .ok()
            .and_then(|target| world.sprites.get(target).copied())
        else {
            world.sprites[index].owner = -1;
            continue;
        };
        if temps[index].g_t[4] == 0 {
            continue;
        }

        let distance = length2d(
            target.x.wrapping_sub(world.sprites[index].x),
            target.y.wrapping_sub(world.sprites[index].y),
        );
        if temps[index].g_t[4] == 1 {
            if distance < 2048 - 128 {
                temps[index].g_t[4] = 2;
            } else {
                if world.sprites[index].xvel == 0 {
                    let activation_tag =
                        effect
                            .hitag
                            .wrapping_add(if temps[index].g_t[3] == 0 { 1 } else { 0 });
                    crate::interactions::operate_activators(
                        world,
                        motions,
                        wall_motions,
                        cyclers,
                        activation_tag,
                        None,
                        temps,
                        monsters_off,
                        actor_types,
                        picnum,
                        script,
                    );
                }
                if world.sprites[index].xvel < 256 {
                    world.sprites[index].xvel = world.sprites[index].xvel.saturating_add(16);
                }
            }
        }
        if temps[index].g_t[4] == 2 {
            if distance <= 128 {
                world.sprites[index].xvel = 0;
            }
            if world.sprites[index].xvel > 0 {
                world.sprites[index].xvel = world.sprites[index].xvel.saturating_sub(16);
            } else {
                world.sprites[index].xvel = 0;
                world.sprites[index].owner = -1;
                world.sprites[index].ang = world.sprites[index].ang.wrapping_add(1024);
                temps[index].g_t[4] = 0;
                temps[index].tempang = 0;
                crate::interactions::operate_activators(
                    world,
                    motions,
                    wall_motions,
                    cyclers,
                    effect.hitag.wrapping_add(temps[index].g_t[3] as i16),
                    None,
                    temps,
                    monsters_off,
                    actor_types,
                    picnum,
                    script,
                );
                crate::wall_animations::operate_forcefields(
                    &mut world.walls,
                    anim_walls,
                    effect.hitag,
                );
                crate::interactions::call_sector_sound(world, effect.sectnum, temps);
                continue;
            }
        }

        let speed = i32::from(world.sprites[index].xvel);
        if speed == 0 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        let (sin, cos) = sincos_q14(i32::from(world.sprites[index].ang));
        let dx = (speed * cos) >> 14;
        let dy = (speed * sin) >> 14;
        if sector.floorz.saturating_sub(sector.ceilingz) < (108 << 8) {
            quickkill_train_players(
                world,
                players,
                temps,
                picnum,
                script,
                randomseed,
                &sector_walls,
                &wall_xy,
                &sector_z,
                world.sprites[index].x,
                world.sprites[index].y,
                world.sprites[index].z,
                effect.sectnum,
                clipping,
                false,
            );
        }
        world.sprites[index].x = world.sprites[index].x.wrapping_add(dx);
        world.sprites[index].y = world.sprites[index].y.wrapping_add(dy);
        let marker_x = world.sprites[index].x;
        let marker_y = world.sprites[index].y;
        let Some(wall_start) = world
            .sectors
            .get(sector_index)
            .and_then(|sector| usize::try_from(sector.wallptr).ok())
        else {
            continue;
        };
        for (offset_index, (ox, oy)) in state.offsets.iter().copied().enumerate() {
            let Some(wall) = wall_start
                .checked_add(offset_index)
                .filter(|wall| *wall < world.walls.len())
            else {
                break;
            };
            crate::interactions::dragpoint_compat(
                &mut world.walls,
                wall,
                marker_x.wrapping_add(ox),
                marker_y.wrapping_add(oy),
            );
        }

        let mut train_sector = effect.sectnum;
        let resolved_train_sector = updatesector(
            marker_x,
            marker_y,
            &mut train_sector,
            &sector_walls,
            &wall_xy,
        );
        if resolved_train_sector >= 0 {
            if train_sector != world.sprites[index].sectnum {
                world.sprites[index].sectnum = train_sector;
                let _ = world.lists.changespritesect(index as i16, train_sector);
            }
        } else {
            train_sector = effect.sectnum;
        }

        let mut occupant = world.lists.headspritesect[sector_index];
        while occupant >= 0 {
            let actor_index = occupant as usize;
            occupant = world.lists.nextspritesect[actor_index];
            if actor_index == index
                || world.lists.sprite_stat[actor_index] == STAT_EFFECTOR
                || world.sprites[actor_index].picnum == crate::spawn::LOCATORS
            {
                continue;
            }
            let old_x = world.sprites[actor_index].x;
            let old_y = world.sprites[actor_index].y;
            world.sprites[actor_index].x = world.sprites[actor_index].x.wrapping_add(dx);
            world.sprites[actor_index].y = world.sprites[actor_index].y.wrapping_add(dy);
            if temps.len() <= actor_index {
                temps.resize_with(actor_index + 1, ActorConTemps::default);
            }
            if !multiplayer {
                temps[actor_index].bposx = old_x;
                temps[actor_index].bposy = old_y;
            } else {
                temps[actor_index].bposx = world.sprites[actor_index].x;
                temps[actor_index].bposy = world.sprites[actor_index].y;
            }
            if world.sprites[actor_index].picnum == crate::spawn::APLAYER
                && world.sprites[actor_index].owner >= 0
            {
                for player in players.iter_mut() {
                    if player.sprite_i == actor_index as i16 {
                        player.posx = player.posx.wrapping_add(dx);
                        player.posy = player.posy.wrapping_add(dy);
                        player.bobposx = player.bobposx.wrapping_add(dx);
                        player.bobposy = player.bobposy.wrapping_add(dy);
                        if multiplayer {
                            player.oposx = player.posx;
                            player.oposy = player.posy;
                        }
                    }
                }
            }
        }

        // Chocolate's SE30 branch applies the 108-unit clearance test while
        // the car is moving. Unlike SE6, player quickkill has no 192-speed
        // threshold here; the actor gore burst is always 24 JIBS6.
        if speed > 0 && sector.floorz.saturating_sub(sector.ceilingz) < (108 << 8) {
            quickkill_train_players(
                world,
                players,
                temps,
                picnum,
                script,
                randomseed,
                &sector_walls,
                &wall_xy,
                &sector_z,
                marker_x,
                marker_y,
                world.sprites[index].z,
                train_sector,
                clipping,
                true,
            );

            let actortype = script.map_or(&[][..], |image| image.actortype.as_slice());
            let squished_sound = script
                .and_then(|image| image.labels.get("SQUISHED"))
                .and_then(|label| i16::try_from(label.value).ok());
            let mut crushed = world.lists.headspritesect[train_sector as usize];
            while crushed >= 0 {
                let victim = crushed as usize;
                crushed = world.lists.nextspritesect[victim];
                if world.lists.sprite_stat[victim] != crate::stat::STAT_ACTOR
                    || world.sprites[victim].extra < 0
                    || world.sprites[victim].picnum == crate::spawn::LOCATORS
                    || !crate::badguy::badguy_pic(world.sprites[victim].picnum, actortype)
                {
                    continue;
                }
                let victim_sprite = world.sprites[victim];
                let mut victim_sector = victim_sprite.sectnum;
                if updatesector(
                    victim_sprite.x,
                    victim_sprite.y,
                    &mut victim_sector,
                    &sector_walls,
                    &wall_xy,
                ) < 0
                    || victim_sector != train_sector
                {
                    continue;
                }
                let repeats = if victim_sprite.xrepeat < 16 { 8 } else { 32 };
                let floor_z = getzsofslope(
                    victim_sprite.sectnum,
                    victim_sprite.x,
                    victim_sprite.y,
                    &sector_z,
                    &wall_xy,
                )
                .1;
                let mut guts_z = victim_sprite.z.saturating_sub(8 << 8);
                if guts_z > floor_z.saturating_sub(8 << 8) {
                    guts_z = floor_z.saturating_sub(8 << 8);
                }
                if victim_sprite.picnum == 1920 {
                    guts_z = guts_z.saturating_sub(24 << 8);
                }
                for _ in 0..24 {
                    // Chocolate `gutsdir`: `a = TRAND&2047;` then EGS arguments right to left
                    // (32-bit gcc): zvel before xvel.
                    let angle = (krand_step(randomseed) & 2047) as i16;
                    let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
                    let xvel = (256 + (krand_step(randomseed) & 127)) as i16;
                    let _ = egs(
                        &mut world.lists,
                        world.sprites.as_mut(),
                        picnum,
                        temps,
                        script,
                        victim_sprite.sectnum,
                        victim_sprite.x,
                        victim_sprite.y,
                        guts_z,
                        crate::shoot::JIBS6,
                        -32,
                        repeats,
                        repeats,
                        angle,
                        xvel,
                        zvel,
                        0,
                        crate::stat::STAT_MISC,
                    );
                }
                let _ = world.lists.deletesprite(victim as i16);
                world.sprites[victim].xrepeat = 0;
                world.sprites[victim].yrepeat = 0;
                if let Some(sound) = squished_sound {
                    temps[index].pending_actor_sounds.push(sound);
                }
            }
        }
    }
}

/// Advance Chocolate SE6 speed controllers and SE14 train cars along locator
/// routes. Cars turn the sector geometry and carry sector sprites and riders;
/// parallax-floor sectors can launch the native tunnel RPG attack, and fast
/// cars crush badguys when the train sector has Chocolate's narrow clearance.
pub(crate) fn move_train_effectors(
    world: &mut BuildWorld,
    players: &mut [Player],
    temps: &mut Vec<ActorConTemps>,
    geometry: &[EffectorWallOffsets],
    picnum: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    monsters_off: bool,
    clipping: bool,
) {
    let multiplayer = players.len() > 1;
    let sector_z: Vec<_> = world
        .sectors
        .iter()
        .take(world.numsectors as usize)
        .map(SectorT::z)
        .collect();
    let sector_walls: Vec<_> = sector_z
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<_> = world
        .walls
        .iter()
        .take(world.numwalls as usize)
        .map(WallT::xy)
        .collect();
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if !matches!(effect.lotag, 6 | 14) {
            continue;
        }
        let Some(wall_state) = geometry.iter().find(|state| state.marker == index) else {
            continue;
        };
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        let max_speed = i32::from(sector.extra);
        if effect.lotag == 6 {
            let mut speed = i32::from(world.sprites[index].xvel);
            let mut timer = temps[index].g_t[4];
            if timer > 0 {
                timer -= 1;
                if timer >= max_speed - (max_speed >> 3) {
                    speed -= max_speed >> 5;
                }
                if timer > (max_speed >> 1) - 1 && timer < max_speed - (max_speed >> 3) {
                    speed = 0;
                }
                if timer < max_speed >> 1 {
                    speed += max_speed >> 5;
                }
                if timer < (max_speed >> 1) - (max_speed >> 3) {
                    timer = 0;
                    speed = max_speed;
                }
                temps[index].g_t[4] = timer;
            } else {
                speed = max_speed;
            }
            world.sprites[index].xvel = speed.clamp(0, i16::MAX as i32) as i16;

            let mut linked = world.lists.headspritestat[STAT_EFFECTOR as usize];
            while linked >= 0 {
                let linked_index = linked as usize;
                linked = world.lists.nextspritestat[linked_index];
                if !world.sprites.get(linked_index).is_some_and(|wagon| {
                    wagon.lotag == 14
                        && wagon.hitag == effect.hitag
                        && temps
                            .get(linked_index)
                            .is_some_and(|temp| temp.g_t[0] == temps[index].g_t[0])
                }) {
                    continue;
                }
                if temps.len() <= linked_index {
                    temps.resize_with(linked_index + 1, ActorConTemps::default);
                }
                let spacing = length2d(
                    world.sprites[linked_index]
                        .x
                        .wrapping_sub(world.sprites[index].x),
                    world.sprites[linked_index]
                        .y
                        .wrapping_sub(world.sprites[index].y),
                );
                if temps[linked_index].g_t[5] == 0 {
                    temps[linked_index].g_t[5] = spacing;
                }
                world.sprites[linked_index].xvel = world.sprites[index].xvel;
                let mut spacing_adjust = (spacing - temps[linked_index].g_t[5]).signum();
                if world.sprites[linked_index].extra != 0 {
                    spacing_adjust = -spacing_adjust;
                }
                world.sprites[index].xvel = world.sprites[index]
                    .xvel
                    .saturating_add(spacing_adjust as i16);
                temps[linked_index].g_t[4] = temps[index].g_t[4];
            }
        }

        if world.sprites[index].owner < 0 {
            let route = temps[index].g_t[3] as i16;
            let route_sector = temps[index].g_t[0] as i16;
            let mut locator = world.lists.headspritestat[crate::stat::STAT_LOCATOR as usize];
            let mut found = -1;
            while locator >= 0 {
                let locator_index = locator as usize;
                locator = world.lists.nextspritestat[locator_index];
                if world
                    .sprites
                    .get(locator_index)
                    .is_some_and(|node| node.lotag == route && node.sectnum == route_sector)
                {
                    found = locator_index as i16;
                    break;
                }
            }
            world.sprites[index].owner = found;
            if found < 0 {
                if temps[index].g_t[3] != 0 {
                    temps[index].g_t[3] = 0;
                }
                continue;
            }
        }
        let Some(mut target) = usize::try_from(world.sprites[index].owner)
            .ok()
            .and_then(|target| world.sprites.get(target).copied())
        else {
            world.sprites[index].owner = -1;
            continue;
        };
        if length2d(
            target.x.wrapping_sub(world.sprites[index].x),
            target.y.wrapping_sub(world.sprites[index].y),
        ) < 1024
        {
            if effect.lotag == 6 && target.hitag & 1 != 0 {
                temps[index].g_t[4] = max_speed;
            }
            temps[index].g_t[3] = temps[index].g_t[3].wrapping_add(1);
            let route_sector = temps[index].g_t[0] as i16;
            let mut next_route = temps[index].g_t[3] as i16;
            let find_locator = |route: i16| {
                let mut locator = world.lists.headspritestat[crate::stat::STAT_LOCATOR as usize];
                while locator >= 0 {
                    let locator_index = locator as usize;
                    locator = world.lists.nextspritestat[locator_index];
                    if world
                        .sprites
                        .get(locator_index)
                        .is_some_and(|node| node.lotag == route && node.sectnum == route_sector)
                    {
                        return Some(locator_index);
                    }
                }
                None
            };
            let next = find_locator(next_route).or_else(|| {
                temps[index].g_t[3] = 0;
                next_route = 0;
                find_locator(0)
            });
            let Some(next) = next else {
                world.sprites[index].owner = -1;
                continue;
            };
            temps[index].g_t[3] = i32::from(next_route);
            world.sprites[index].owner = next as i16;
            target = world.sprites[next];
        }

        let speed = i32::from(world.sprites[index].xvel);
        if speed == 0 {
            continue;
        }
        if !monsters_off
            && speed == max_speed
            && sector.floorstat & 1 != 0
            && sector.floorpal == 0
            && krand_step(randomseed) >> 8 >= 255 - 8
        {
            let (target_index, distance) = crate::fta::findplayer_dist(&effect, players);
            if distance < 20_480 {
                if let Some(target) = players.get(target_index) {
                    let aim_angle = getangle(
                        effect.x.wrapping_sub(target.posx),
                        effect.y.wrapping_sub(target.posy),
                    ) as i16;
                    shoot_effector_rpg_or_freezer(
                        world,
                        players,
                        temps,
                        picnum,
                        tile_meta,
                        script,
                        randomseed,
                        index,
                        RPG,
                        Some(aim_angle),
                    );
                }
            }
        }
        let pivot_x = world.sprites[index].x;
        let pivot_y = world.sprites[index].y;
        let goal_angle = getangle(
            target.x.wrapping_sub(pivot_x),
            target.y.wrapping_sub(pivot_y),
        );
        let delta_angle =
            (((goal_angle - i32::from(world.sprites[index].ang)) + 1024) & 2047) - 1024;
        let turn = delta_angle >> 3;
        world.sprites[index].ang = world.sprites[index].ang.wrapping_add(turn as i16);
        temps[index].g_t[2] = temps[index].g_t[2].wrapping_add(turn);
        let (sin, cos) = sincos_q14(i32::from(world.sprites[index].ang));
        let dx = (speed * cos) >> 14;
        let dy = (speed * sin) >> 14;

        // Chocolate checks whether the train has pushed the player out of
        // the train sector before carrying occupants along with it.
        let player_crush_speed = if effect.lotag == 6 {
            speed >= 192
        } else {
            speed > 0
        };
        if !clipping
            && player_crush_speed
            && sector.floorz.saturating_sub(sector.ceilingz) < (108 << 8)
        {
            quickkill_train_players(
                world,
                players,
                temps,
                picnum,
                script,
                randomseed,
                &sector_walls,
                &wall_xy,
                &sector_z,
                effect.x,
                effect.y,
                effect.z,
                effect.sectnum,
                clipping,
                false,
            );
        }

        let mut player_sprite = world.lists.headspritestat[crate::stat::STAT_PLAYER as usize];
        while player_sprite >= 0 {
            let player_index = player_sprite as usize;
            player_sprite = world.lists.nextspritestat[player_index];
            if world.sprites[player_index].sectnum != effect.sectnum {
                continue;
            }
            for player in players.iter_mut() {
                if player.sprite_i != player_index as i16
                    || world
                        .sectors
                        .get(player.cursectnum.max(0) as usize)
                        .is_some_and(|player_sector| player_sector.lotag == 2)
                {
                    continue;
                }
                let (x, y) = rotatepoint(pivot_x, pivot_y, player.posx, player.posy, turn as i16);
                player.posx = x.wrapping_add(dx);
                player.posy = y.wrapping_add(dy);
                player.bobposx = player.bobposx.wrapping_add(dx);
                player.bobposy = player.bobposy.wrapping_add(dy);
                player.ang = player.ang.wrapping_add(turn as i16) & 2047;
                if multiplayer {
                    player.oposx = player.posx;
                    player.oposy = player.posy;
                }
            }
        }

        let mut occupant = world.lists.headspritesect[sector_index];
        while occupant >= 0 {
            let actor_index = occupant as usize;
            occupant = world.lists.nextspritesect[actor_index];
            if actor_index == index
                || world.lists.sprite_stat[actor_index] == crate::stat::STAT_PLAYER
                || world.lists.sprite_stat[actor_index] == STAT_EFFECTOR
                || world.sprites[actor_index].picnum == crate::spawn::LOCATORS
                || world
                    .sectors
                    .get(world.sprites[actor_index].sectnum.max(0) as usize)
                    .is_some_and(|actor_sector| actor_sector.lotag == 2)
            {
                continue;
            }
            let (x, y) = rotatepoint(
                pivot_x,
                pivot_y,
                world.sprites[actor_index].x,
                world.sprites[actor_index].y,
                turn as i16,
            );
            let old_x = world.sprites[actor_index].x;
            let old_y = world.sprites[actor_index].y;
            world.sprites[actor_index].x = x.wrapping_add(dx);
            world.sprites[actor_index].y = y.wrapping_add(dy);
            world.sprites[actor_index].ang =
                world.sprites[actor_index].ang.wrapping_add(turn as i16) & 2047;
            if temps.len() <= actor_index {
                temps.resize_with(actor_index + 1, ActorConTemps::default);
            }
            if multiplayer {
                temps[actor_index].bposx = world.sprites[actor_index].x;
                temps[actor_index].bposy = world.sprites[actor_index].y;
            } else {
                temps[actor_index].bposx = old_x;
                temps[actor_index].bposy = old_y;
            }
        }

        world.sprites[index].x = world.sprites[index].x.wrapping_add(dx);
        world.sprites[index].y = world.sprites[index].y.wrapping_add(dy);
        let marker_x = world.sprites[index].x;
        let marker_y = world.sprites[index].y;
        apply_effector_wall_rotation(world, wall_state, marker_x, marker_y, temps[index].g_t[2]);
        let mut train_sector = effect.sectnum;
        let resolved_train_sector = updatesector(
            marker_x,
            marker_y,
            &mut train_sector,
            &sector_walls,
            &wall_xy,
        );
        if resolved_train_sector >= 0 {
            if train_sector != world.sprites[index].sectnum {
                world.sprites[index].sectnum = train_sector;
                let _ = world.lists.changespritesect(index as i16, train_sector);
            }
        } else {
            train_sector = effect.sectnum;
        }

        // Chocolate runs the crush checks after moving the car and calling
        // setsprite on the controller, so test occupants against the updated
        // Build sector rather than assuming the map-load sector is unchanged.
        if speed > 0 && sector.floorz.saturating_sub(sector.ceilingz) < (108 << 8) {
            let actortype = script.map_or(&[][..], |image| image.actortype.as_slice());
            let squished_sound = script
                .and_then(|image| image.labels.get("SQUISHED"))
                .and_then(|label| i16::try_from(label.value).ok());
            let gib_count = if effect.lotag == 6 { 72 } else { 24 };
            let mut crushed = world.lists.headspritesect[train_sector as usize];
            while crushed >= 0 {
                let victim = crushed as usize;
                crushed = world.lists.nextspritesect[victim];
                if world.lists.sprite_stat[victim] != crate::stat::STAT_ACTOR
                    || world.sprites[victim].extra < 0
                    || world.sprites[victim].picnum == crate::spawn::LOCATORS
                    || !crate::badguy::badguy_pic(world.sprites[victim].picnum, actortype)
                {
                    continue;
                }
                let victim_sprite = world.sprites[victim];
                let mut victim_sector = victim_sprite.sectnum;
                if updatesector(
                    victim_sprite.x,
                    victim_sprite.y,
                    &mut victim_sector,
                    &sector_walls,
                    &wall_xy,
                ) < 0
                    || victim_sector != train_sector
                {
                    continue;
                }
                let repeats = if victim_sprite.xrepeat < 16 { 8 } else { 32 };
                let floor_z = getzsofslope(
                    victim_sprite.sectnum,
                    victim_sprite.x,
                    victim_sprite.y,
                    &sector_z,
                    &wall_xy,
                )
                .1;
                let mut guts_z = victim_sprite.z.saturating_sub(8 << 8);
                if guts_z > floor_z.saturating_sub(8 << 8) {
                    guts_z = floor_z.saturating_sub(8 << 8);
                }
                if victim_sprite.picnum == 1920
                /* COMMANDER */
                {
                    guts_z = guts_z.saturating_sub(24 << 8);
                }
                for _ in 0..gib_count {
                    // Chocolate `gutsdir`: `a = TRAND&2047;` then EGS arguments right to left
                    // (32-bit gcc): zvel before xvel.
                    let angle = (krand_step(randomseed) & 2047) as i16;
                    let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
                    let xvel = (256 + (krand_step(randomseed) & 127)) as i16;
                    let _ = egs(
                        &mut world.lists,
                        world.sprites.as_mut(),
                        picnum,
                        temps,
                        script,
                        victim_sprite.sectnum,
                        victim_sprite.x,
                        victim_sprite.y,
                        guts_z,
                        crate::shoot::JIBS6,
                        -32,
                        repeats,
                        repeats,
                        angle,
                        xvel,
                        zvel,
                        0,
                        crate::stat::STAT_MISC,
                    );
                }
                let victim_i = victim as i16;
                let _ = world.lists.deletesprite(victim_i);
                world.sprites[victim].xrepeat = 0;
                world.sprites[victim].yrepeat = 0;
                if let Some(sound) = squished_sound {
                    temps[index].pending_actor_sounds.push(sound);
                }
            }
        }

        // Repeat after the controller's final position/sector update: the
        // source performs the same check on both sides of train movement.
        if !clipping
            && player_crush_speed
            && sector.floorz.saturating_sub(sector.ceilingz) < (108 << 8)
        {
            quickkill_train_players(
                world,
                players,
                temps,
                picnum,
                script,
                randomseed,
                &sector_walls,
                &wall_xy,
                &sector_z,
                marker_x,
                marker_y,
                world.sprites[index].z,
                train_sector,
                clipping,
                true,
            );
        }
        if sector.floorstat & 1 == 0 && sector.ceilingstat & 1 == 0 {
            if speed == max_speed && temps[index].lastvy == 0 && temps[index].lastvx > 0 {
                let sound = temps[index].lastvx as i16;
                temps[index].pending_actor_sounds_once.push(sound);
                temps[index].lastvy = 1;
            } else if speed <= 64 && temps[index].lastvy != 0 {
                let sound = temps[index].lastvx as i16;
                temps[index].pending_sound_stops.push(sound);
                temps[index].lastvy = 0;
            }
        }
    }
}

/// Advance the Atomic boss elevator controller (Chocolate `SECTOREFFECTOR`
/// lotag 5). The controller seeks the closest locator to the nearest player,
/// raises its ceiling while travelling, and fires its native laser projectile
/// when a player is within range.
pub(crate) fn move_boss_effectors(
    world: &mut BuildWorld,
    players: &mut [Player],
    temps: &mut Vec<ActorConTemps>,
    geometry: &[EffectorWallOffsets],
    picnum_out: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
) {
    if players.is_empty() {
        return;
    }
    let sectors: Vec<SectorWalls> = world
        .sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        if world
            .sprites
            .get(index)
            .is_none_or(|sprite| sprite.lotag != 5)
        {
            continue;
        }
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        let Some(wall_state) = geometry.iter().find(|state| state.marker == index) else {
            continue;
        };
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let (player_index, player_dist) = players
            .iter()
            .enumerate()
            .map(|(i, player)| {
                let distance = length2d(
                    player.posx.wrapping_sub(effect.x),
                    player.posy.wrapping_sub(effect.y),
                );
                (i, distance)
            })
            .min_by_key(|(_, distance)| *distance)
            .unwrap_or((0, i32::MAX));
        let player = &players[player_index];
        if player_dist < 8192 {
            // Chocolate temporarily points the SE5 at the selected player,
            // calls `shoot(FIRELASER)`, then restores its route angle. Reuse
            // the same CON-backed projectile path as the other shooter
            // effectors so script speed, strength, muzzle offset and TRAND
            // order come from the loaded Atomic actor definition.
            world.sprites[index].ang = getangle(
                effect.x.wrapping_sub(player.posx),
                effect.y.wrapping_sub(player.posy),
            ) as i16;
            shoot_effector_firelaser(
                world, players, temps, picnum_out, tile_meta, script, randomseed, index,
            );
            world.sprites[index].ang = effect.ang;
        }

        if world.sprites[index].owner < 0 {
            let mut route = 0_i16;
            let mut best = None;
            loop {
                let mut locator = world.lists.headspritestat[crate::stat::STAT_LOCATOR as usize];
                let mut route_found = false;
                while locator >= 0 {
                    let locator_index = locator as usize;
                    locator = world.lists.nextspritestat[locator_index];
                    let Some(candidate) = world.sprites.get(locator_index) else {
                        continue;
                    };
                    if candidate.lotag != route {
                        continue;
                    }
                    route_found = true;
                    let distance = length2d(
                        candidate.x.wrapping_sub(player.posx),
                        candidate.y.wrapping_sub(player.posy),
                    );
                    if best.is_none_or(|(_, best_distance)| distance < best_distance) {
                        best = Some((locator_index, distance));
                    }
                    break;
                }
                if !route_found {
                    break;
                }
                route = route.wrapping_add(1);
            }
            let Some((locator_index, _)) = best else {
                continue;
            };
            world.sprites[index].owner = locator_index as i16;
            let target_z = world.sprites[locator_index].z;
            world.sprites[index].zvel = if target_z > effect.z {
                16
            } else if target_z < effect.z {
                -16
            } else {
                0
            };
        }

        let Some(target) = usize::try_from(world.sprites[index].owner)
            .ok()
            .and_then(|target| world.sprites.get(target).copied())
        else {
            world.sprites[index].owner = -1;
            continue;
        };
        if length2d(target.x - effect.x, target.y - effect.y) < 1024 {
            world.sprites[index].owner = -1;
            continue;
        }
        world.sprites[index].xvel = 256;

        let desired_angle = getangle(target.x - effect.x, target.y - effect.y);
        let delta = ((desired_angle - i32::from(effect.ang) + 1024) & 2047) - 1024;
        let turn = delta >> 3;
        world.sprites[index].ang = effect.ang.wrapping_add(turn as i16);
        if (room_build_engine::numeric::krand_step(randomseed) & 31) != 0 {
            temps[index].g_t[2] = temps[index].g_t[2].wrapping_add(turn);
            world.sectors[sector_index].ceilingshade = 127;
        } else {
            let toward_player = getangle(player.posx - effect.x, player.posy - effect.y);
            let shade_delta = ((toward_player - temps[index].g_t[2] - 512 + 1024) & 2047) - 1024;
            temps[index].g_t[2] = temps[index].g_t[2].wrapping_add(shade_delta >> 2);
            world.sectors[sector_index].ceilingshade = 0;
        }
        if temps[index].hit_extra >= 0 && world.sprites[index].extra >= 0 {
            world.sprites[index].extra = world.sprites[index]
                .extra
                .wrapping_sub(temps[index].hit_extra);
            temps[index].hit_extra = -1;
            temps[index].g_t[3] = temps[index].g_t[3].wrapping_add(1);
            if temps[index].g_t[3] == 5 {
                world.sprites[index].zvel = world.sprites[index].zvel.wrapping_add(1024);
                if players[0].fta_enabled {
                    players[0].quote_id = 7;
                    players[0].quote_timer = 100;
                }
            }
        }
        let dz = i32::from(world.sprites[index].zvel);
        let (sin, cos) = sincos_q14(i32::from(world.sprites[index].ang));
        let dx = (i32::from(world.sprites[index].xvel) * cos) >> 14;
        let dy = (i32::from(world.sprites[index].xvel) * sin) >> 14;
        world.sprites[index].x = world.sprites[index].x.wrapping_add(dx);
        world.sprites[index].y = world.sprites[index].y.wrapping_add(dy);
        world.sprites[index].z = world.sprites[index].z.wrapping_add(dz);
        world.sectors[sector_index].ceilingz =
            world.sectors[sector_index].ceilingz.wrapping_add(dz);
        let linked_sector = temps[index].g_t[0];
        if let Ok(linked_sector) = usize::try_from(linked_sector)
            && linked_sector < world.sectors.len()
            && linked_sector != sector_index
        {
            world.sectors[linked_sector].ceilingz =
                world.sectors[linked_sector].ceilingz.wrapping_add(dz);
        }
        // `ms(i)` rotates each map-load wall offset by T3 around the moving
        // effector. T3 is also advanced by the boss's turn/shade logic above.
        apply_effector_wall_rotation(
            world,
            wall_state,
            world.sprites[index].x,
            world.sprites[index].y,
            temps[index].g_t[2],
        );
        // Chocolate follows `ms(i)` with `setsprite(i, x, y, z)`. Recompute
        // the controller's sector after its wall/marker translation so the
        // next tic reads the same sector state and keeps sprite lists valid.
        let walls: Vec<WallXY> = world.walls.iter().map(WallT::xy).collect();
        let mut new_sector = world.sprites[index].sectnum;
        let _ = updatesector(
            world.sprites[index].x,
            world.sprites[index].y,
            &mut new_sector,
            &sectors,
            &walls,
        );
        if new_sector >= 0 && new_sector != world.sprites[index].sectnum {
            world.sprites[index].sectnum = new_sector;
            let _ = world.lists.changespritesect(index as i16, new_sector);
        }
    }
}

/// Advance Chocolate's `SE 18` Kroz room-over-room plane marker after it is
/// armed by a master switch or a linked explosive. `T2` retains the map-load
/// plane and the marker Z is the alternate position.
pub(crate) fn move_kroz_effectors(
    world: &mut BuildWorld,
    players: &mut [Player],
    temps: &mut Vec<ActorConTemps>,
) {
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effect) = world.sprites.get(index).copied() else {
            continue;
        };
        if effect.lotag != 18 || temps.get(index).is_none_or(|temp| temp.g_t[0] == 0) {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        let is_ceiling = effect.ang == 512;
        let current = if is_ceiling {
            sector.ceilingz
        } else {
            sector.floorz
        };
        let target = if effect.pal != 0 {
            temps[index].g_t[1]
        } else {
            effect.z
        };
        let speed = i32::from(sector.extra).unsigned_abs() as i32;
        if speed == 0 {
            continue;
        }
        let toward_target = if effect.pal != 0 {
            if is_ceiling { -speed } else { speed }
        } else if is_ceiling {
            speed
        } else {
            -speed
        };
        let next = current.saturating_add(toward_target);
        let arrived = if toward_target > 0 {
            next >= target
        } else {
            next <= target
        };
        let delta = if arrived {
            target - current
        } else {
            toward_target
        };
        if is_ceiling {
            world.sectors[sector_index].ceilingz = current.saturating_add(delta);
        } else {
            world.sectors[sector_index].floorz = current.saturating_add(delta);
            let mut occupant = world.lists.headspritesect[sector_index];
            while occupant >= 0 {
                let actor_index = occupant as usize;
                occupant = world.lists.nextspritesect[actor_index];
                let Some(actor) = world.sprites.get_mut(actor_index) else {
                    continue;
                };
                if actor.picnum == crate::spawn::APLAYER && actor.owner >= 0 {
                    for player in players.iter_mut() {
                        if player.sprite_i == actor_index as i16 && player.on_ground {
                            player.posz = player.posz.saturating_add(delta);
                        }
                    }
                }
                if world.lists.sprite_stat[actor_index] != STAT_EFFECTOR
                    && world.lists.sprite_stat[actor_index] != crate::stat::STAT_PROJECTILE
                    && actor.zvel == 0
                {
                    actor.z = actor.z.saturating_add(delta);
                    if temps.len() <= actor_index {
                        temps.resize_with(actor_index + 1, ActorConTemps::default);
                    }
                    temps[actor_index].bposz = actor.z;
                    temps[actor_index].hit_floorz = world.sectors[sector_index].floorz;
                }
            }
        }
        temps[index].g_t[2] = temps[index].g_t[2].wrapping_add(1);
        if arrived {
            let _ = world.lists.deletesprite(index as i16);
            world.sprites[index].statnum = room_build_engine::world::MAXSTATUS as i16;
            temps[index].g_t[0] = 0;
            temps[index].g_t[2] = 0;
        } else if temps[index].g_t[2] >= i32::from(effect.hitag) {
            temps[index].g_t[0] = 0;
            temps[index].g_t[2] = 0;
        }
    }
}

/// Tick Chocolate's drilling ceiling effect (SE35), including its smoke and
/// intermittent explosion actors.
pub(crate) fn move_drill_effectors(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &[WallT],
    picnum: &mut Vec<i16>,
    temps: &mut Vec<ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
) {
    const TICSPERFRAME: i32 = 4;
    let sector_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let mut marker = lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = lists.nextspritestat[index];
        let Some(effector) = sprites.get(index).copied() else {
            continue;
        };
        if effector.lotag != 35 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effector.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get(sector_index).copied() else {
            continue;
        };

        if sector.ceilingz > effector.z {
            for _ in 0..8 {
                let angle = (room_build_engine::numeric::krand_step(randomseed) & 511) as i16;
                sprites[index].ang = sprites[index].ang.wrapping_add(angle);
                let smoke = crate::shoot::spawn_smallsmoke(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum,
                    temps,
                    script,
                    randomseed,
                    index as i16,
                );
                if smoke >= 0 {
                    let smoke_index = smoke as usize;
                    let xvel =
                        (96 + (room_build_engine::numeric::krand_step(randomseed) & 127)) as i16;
                    sprites[smoke_index].xvel = xvel;
                    let smoke_angle = i32::from(sprites[smoke_index].ang);
                    let (sin, cos) = sincos_q14(smoke_angle);
                    let xchange = (i32::from(xvel) * cos) >> 14;
                    let ychange = (i32::from(xvel) * sin) >> 14;
                    let dx = xchange.saturating_mul(TICSPERFRAME) >> 3;
                    let dy = ychange.saturating_mul(TICSPERFRAME) >> 3;
                    let old_sector = sprites[smoke_index].sectnum;
                    let clipdist = i32::from(sprites[smoke_index].clipdist) << 2;
                    let _ = clipmove_xy(
                        &mut sprites[smoke_index].x,
                        &mut sprites[smoke_index].y,
                        &mut sprites[smoke_index].sectnum,
                        dx,
                        dy,
                        clipdist,
                        &sector_walls,
                        &wall_xy,
                    );
                    if sprites[smoke_index].sectnum != old_sector {
                        let _ = lists.changespritesect(smoke, sprites[smoke_index].sectnum);
                    }
                }
                let roll = room_build_engine::numeric::krand_step(randomseed);
                if (roll >> 8) >= (255 - 16) {
                    let explosion = crate::shoot::egs(
                        lists,
                        sprites,
                        picnum,
                        temps,
                        script,
                        effector.sectnum,
                        effector.x,
                        effector.y,
                        effector.z,
                        EXPLOSION2,
                        -127,
                        48,
                        48,
                        sprites[index].ang,
                        0,
                        0,
                        index as i16,
                        crate::stat::STAT_MISC,
                    );
                    if explosion >= 0 {
                        let explosion_index = explosion as usize;
                        sprites[explosion_index].cstat =
                            128 | (room_build_engine::numeric::krand_step(randomseed) & 4) as u16;
                        temps[explosion_index].g_t[5] = 0;
                    }
                }
            }
        }

        let original_ceiling = temps.get(index).map_or(0, |temp| temp.g_t[4]);
        match temps.get(index).map_or(0, |temp| temp.g_t[0]) {
            0 => {
                let step = i32::from(effector.yvel);
                let ceiling = &mut sectors[sector_index].ceilingz;
                *ceiling = ceiling.saturating_add(step).min(sector.floorz);
                if *ceiling > effector.z.saturating_add(32 << 8) {
                    temps[index].g_t[0] = 1;
                }
            }
            _ => {
                let step = i32::from(effector.yvel).saturating_mul(4);
                let ceiling = &mut sectors[sector_index].ceilingz;
                *ceiling = ceiling.saturating_sub(step);
                if *ceiling < original_ceiling {
                    *ceiling = original_ceiling;
                    temps[index].g_t[0] = 0;
                }
            }
        }
    }
}

/// Advance the continuously cycling Atomic piston ceiling (SE25).
pub(crate) fn move_piston_effectors(
    lists: &SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    temps: &mut Vec<ActorConTemps>,
) {
    let mut marker = lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = lists.nextspritestat[index];
        let Some(sprite) = sprites.get(index).copied() else {
            continue;
        };
        if sprite.lotag != 25 || temps.get(index).is_none_or(|temp| temp.g_t[4] == 0) {
            continue;
        }
        let Ok(sector_index) = usize::try_from(sprite.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get_mut(sector_index) else {
            continue;
        };
        if sector.floorz <= sector.ceilingz {
            sprites[index].shade = 0;
        } else if sector.ceilingz <= temps[index].g_t[3] {
            sprites[index].shade = 1;
        }
        let step = i32::from(sprite.yvel).saturating_mul(16);
        if sprites[index].shade != 0 {
            sector.ceilingz = sector.ceilingz.saturating_add(step).min(sector.floorz);
        } else {
            sector.ceilingz = sector
                .ceilingz
                .saturating_sub(step)
                .max(temps[index].g_t[3]);
        }
    }
}

/// Move sprites and the local player with Chocolate's SE24/SE34 conveyor.
///
/// The Build sine table is used by `sincos_q14`; the effector's `yvel` is its
/// speed. Sprite movement is a quarter of the calculated vector while player
/// movement is injected into the normal fixed-point velocity/friction path.
pub(crate) fn move_conveyor_effectors(
    world: &mut room_build_engine::world::BuildWorld,
    players: &mut [crate::player::Player],
    temps: &mut Vec<ActorConTemps>,
) {
    let sectors: Vec<SectorWalls> = world
        .sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let walls: Vec<WallXY> = world.walls.iter().map(WallT::xy).collect();
    let sector_planes: Vec<_> = world.sectors.iter().map(SectorT::z).collect();
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(effector) = world.sprites.get(index).copied() else {
            continue;
        };
        if !matches!(effector.lotag, 24 | 34)
            || temps.get(index).is_some_and(|temp| temp.g_t[4] != 0)
        {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effector.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        let (sin, cos) = sincos_q14(i32::from(effector.ang));
        // `SP` is Chocolate's shorthand for `sprite[i].yvel`.
        let speed = i32::from(effector.yvel);
        let dx = speed.saturating_mul(cos) >> 18;
        let dy = speed.saturating_mul(sin) >> 18;

        let mut actor = world.lists.headspritesect[sector_index];
        while actor >= 0 {
            let actor_index = actor as usize;
            actor = world.lists.nextspritesect[actor_index];
            if actor_index == index || world.lists.sprite_stat[actor_index] == STAT_EFFECTOR {
                continue;
            }
            let sprite = world.sprites[actor_index];
            // Chocolate ignores airborne sprites; grounded status-0/1 actors
            // and the carryable status-5/6 effects ride the conveyor.
            let status = world.lists.sprite_stat[actor_index];
            if sprite.zvel < 0 {
                continue;
            }
            if status == 5 {
                if matches!(
                    sprite.picnum,
                    BLOODPOOL
                        | PUKE
                        | FOOTPRINTS
                        | FOOTPRINTS2
                        | FOOTPRINTS3
                        | FOOTPRINTS4
                        | BULLETHOLE
                        | 2296..=2299
                ) {
                    world.sprites[actor_index].xrepeat = 0;
                    world.sprites[actor_index].yrepeat = 0;
                    continue;
                }
                if sprite.picnum == LASERLINE {
                    continue;
                }
            }
            if status == 6 && sprite.picnum == TRIPBOMB {
                continue;
            }
            if matches!(sprite.picnum, 634..=637 | SIDEBOLT1..=SIDEBOLT_LAST)
                || crate::interactions::is_switch(sprite.picnum)
                || matches!(sprite.picnum, CRANE..=1225)
            {
                continue;
            }
            let hit_floorz = temps
                .get(actor_index)
                .map_or(sector.floorz, |temp| temp.hit_floorz);
            if !matches!(status, 0 | 1 | 5 | 6) || sprite.z <= hit_floorz.saturating_sub(16 << 8) {
                continue;
            }
            if temps.len() <= actor_index {
                temps.resize_with(actor_index + 1, ActorConTemps::default);
            }
            temps[actor_index].bposx = sprite.x;
            temps[actor_index].bposy = sprite.y;
            // Chocolate adds the conveyor delta directly, then calls
            // `setsprite` to refresh sector membership; this is not a
            // `clipmove` and must not add wall collision that the reference
            // does not perform here.
            let x = sprite.x.wrapping_add(dx >> 2);
            let y = sprite.y.wrapping_add(dy >> 2);
            let mut new_sector = sprite.sectnum;
            world.sprites[actor_index].x = x;
            world.sprites[actor_index].y = y;
            let _ = updatesector(x, y, &mut new_sector, &sectors, &walls);
            if new_sector >= 0 {
                world.sprites[actor_index].sectnum = new_sector;
                if new_sector != sprite.sectnum {
                    let _ = world.lists.changespritesect(actor_index as i16, new_sector);
                }
            }
        }

        if let Some(sector) = world.sectors.get_mut(sector_index) {
            // Chocolate's SE24/SE34 branch scrolls only floor X by SP >> 7.
            sector.floorxpanning = sector.floorxpanning.wrapping_add((speed >> 7) as u8);
        }

        for player in players.iter_mut() {
            let true_floor_z = getzsofslope(
                player.cursectnum,
                player.posx,
                player.posy,
                &sector_planes,
                &walls,
            )
            .1;
            if player.cursectnum == effector.sectnum
                && player.on_ground
                && player.posz.saturating_sub(true_floor_z).unsigned_abs() < (56 << 8) + (9 << 8)
            {
                player.posxv = player.posxv.saturating_add(dx << 3);
                player.posyv = player.posyv.saturating_add(dy << 3);
            }
        }
    }
}

/// Initialize Chocolate's SE31/SE32 moving-plane targets and wall shade filters.
pub(crate) fn init_true_drop_planes(
    sprites: &[SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    lists: &SpriteLists,
    temps: &mut Vec<ActorConTemps>,
) {
    let mut marker = lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = lists.nextspritestat[index];
        let Some(sprite) = sprites.get(index).copied() else {
            continue;
        };
        if sprite.lotag != 31 && sprite.lotag != 32 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(sprite.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get_mut(sector_index) else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        temps[index].g_t[1] = if sprite.lotag == 31 {
            sector.floorz
        } else {
            sector.ceilingz
        };
        temps[index].g_t[2] = 0;
        if sprite.ang != 1536 {
            if sprite.lotag == 31 {
                sector.floorz = sprite.z;
            } else {
                sector.ceilingz = sprite.z;
            }
        }
        let start = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
        let end = start
            .saturating_add(usize::try_from(sector.wallnum).unwrap_or_default())
            .min(walls.len());
        for wall in walls.get_mut(start..end).unwrap_or_default() {
            if wall.hitag == 0 {
                wall.hitag = 9999;
            }
        }
    }
}

/// Advance Chocolate SE31/SE32 true-drop moving planes and floor riders.
///
/// Mirrors `moveeffectors` cases 31/32: the step is `SP` (sprite `yvel`, the
/// sector GPSPEED), only the retract leg snaps to its goal, and only the floor
/// variant waits `hitag` tics (T4) between legs.
pub(crate) fn move_true_drop_planes(
    world: &mut room_build_engine::world::BuildWorld,
    players: &mut [crate::player::Player],
    temps: &mut Vec<ActorConTemps>,
) {
    let mut marker = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(sprite) = world.sprites.get(index).copied() else {
            continue;
        };
        if (sprite.lotag != 31 && sprite.lotag != 32)
            || temps.get(index).is_none_or(|temp| temp.g_t[0] != 1)
        {
            continue;
        }
        let Ok(sector_index) = usize::try_from(sprite.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        let sp = i32::from(sprite.yvel);
        let ang_1536 = sprite.ang & 2047 == 1536;
        let hitag = i32::from(sprite.hitag);
        let t1 = temps[index].g_t[1];
        let retract = temps[index].g_t[2] == 1;

        if sprite.lotag == 32 {
            let cz = sector.ceilingz;
            let near = |goal: i32| cz.wrapping_sub(goal).unsigned_abs() < (sp << 1) as u32;
            if retract {
                let goal = if ang_1536 { t1 } else { sprite.z };
                if near(goal) {
                    world.sectors[sector_index].ceilingz = goal;
                    crate::interactions::call_sector_sound(world, sprite.sectnum, temps);
                    temps[index].g_t[2] = 0;
                    temps[index].g_t[0] = 0;
                } else {
                    world.sectors[sector_index].ceilingz =
                        cz.wrapping_add(goal.wrapping_sub(cz).signum().wrapping_mul(sp));
                }
            } else if ang_1536 {
                if near(sprite.z) {
                    temps[index].g_t[0] = 0;
                    temps[index].g_t[2] = i32::from(temps[index].g_t[2] == 0);
                    crate::interactions::call_sector_sound(world, sprite.sectnum, temps);
                    world.sectors[sector_index].ceilingz = sprite.z;
                } else {
                    world.sectors[sector_index].ceilingz =
                        cz.wrapping_add(sprite.z.wrapping_sub(cz).signum().wrapping_mul(sp));
                }
            } else if near(t1) {
                temps[index].g_t[0] = 0;
                temps[index].g_t[2] = i32::from(temps[index].g_t[2] == 0);
                crate::interactions::call_sector_sound(world, sprite.sectnum, temps);
            } else {
                world.sectors[sector_index].ceilingz =
                    cz.wrapping_sub(sprite.z.wrapping_sub(t1).signum().wrapping_mul(sp));
            }
            continue;
        }

        // SE31 true drop floor.
        if temps[index].g_t[3] > 0 {
            temps[index].g_t[3] -= 1;
            continue;
        }
        let fz = sector.floorz;
        let near = |goal: i32| fz.wrapping_sub(goal).unsigned_abs() < sp as u32;
        let step = if retract {
            let goal = if ang_1536 { t1 } else { sprite.z };
            if near(goal) {
                world.sectors[sector_index].floorz = goal;
                temps[index].g_t[2] = 0;
                temps[index].g_t[0] = 0;
                temps[index].g_t[3] = hitag;
                crate::interactions::call_sector_sound(world, sprite.sectnum, temps);
                continue;
            }
            goal.wrapping_sub(fz).signum().wrapping_mul(sp)
        } else {
            let (arrived, step) = if ang_1536 {
                (
                    sprite.z.wrapping_sub(fz).unsigned_abs() < sp as u32,
                    sprite.z.wrapping_sub(fz).signum().wrapping_mul(sp),
                )
            } else {
                (
                    near(t1),
                    -sprite.z.wrapping_sub(t1).signum().wrapping_mul(sp),
                )
            };
            if arrived {
                crate::interactions::call_sector_sound(world, sprite.sectnum, temps);
                temps[index].g_t[0] = 0;
                temps[index].g_t[2] = 1;
                temps[index].g_t[3] = hitag;
                continue;
            }
            step
        };
        world.sectors[sector_index].floorz = fz.wrapping_add(step);
        let new_floor = world.sectors[sector_index].floorz;
        let mut occupant = world.lists.headspritesect[sector_index];
        while occupant >= 0 {
            let actor_index = occupant as usize;
            occupant = world.lists.nextspritesect[actor_index];
            let actor = &mut world.sprites[actor_index];
            if actor.picnum == crate::spawn::APLAYER && actor.owner >= 0 {
                for player in players.iter_mut() {
                    if player.sprite_i == actor_index as i16 && player.on_ground {
                        player.posz = player.posz.wrapping_add(step);
                    }
                }
            }
            if actor.zvel == 0
                && world.lists.sprite_stat[actor_index] != STAT_EFFECTOR
                && world.lists.sprite_stat[actor_index] != crate::stat::STAT_PROJECTILE
            {
                actor.z = actor.z.wrapping_add(step);
                if temps.len() <= actor_index {
                    temps.resize_with(actor_index + 1, ActorConTemps::default);
                }
                temps[actor_index].bposz = actor.z;
                temps[actor_index].hit_floorz = new_floor;
            }
        }
    }
}

/// Advance the static randomized flashing light effector (SE 4).
///
/// Chocolate initializes the baseline from the sector's floor shade and the
/// maximum wall shade, then updates the sector and wall palettes every tic.
pub(crate) fn move_light_effectors(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    temps: &mut Vec<ActorConTemps>,
    global_random: i32,
) {
    let mut i = lists.headspritestat[STAT_EFFECTOR as usize];
    while i >= 0 {
        let iu = i as usize;
        i = lists.nextspritestat[iu];
        let Some(marker) = sprites.get(iu).copied() else {
            continue;
        };
        if marker.lotag == 128 {
            if temps.len() <= iu {
                temps.resize_with(iu + 1, ActorConTemps::default);
            }
            let wall_index = usize::try_from(temps[iu].g_t[2]).unwrap_or(usize::MAX);
            if let Some(wall) = walls.get(wall_index).copied() {
                let peer_index = usize::try_from(wall.nextwall).ok();
                // Chocolate's `cstat &= (255-32)` truncates the current
                // flags to the low byte before setting translucent bit 16.
                // Its `if (cstat|32)` is a bitwise OR and therefore always
                // enters this progression, including already-unmasked walls.
                walls[wall_index].cstat = (wall.cstat & 223) | 16;
                if let Some(peer) = peer_index.and_then(|peer| walls.get_mut(peer)) {
                    peer.cstat = (peer.cstat & 223) | 16;
                }
                walls[wall_index].overpicnum = wall.overpicnum.wrapping_add(1);
                if let Some(peer) = peer_index.and_then(|peer| walls.get_mut(peer)) {
                    peer.overpicnum = peer.overpicnum.wrapping_add(1);
                }
                if temps[iu].g_t[0] < temps[iu].g_t[1] {
                    temps[iu].g_t[0] += 1;
                } else {
                    walls[wall_index].cstat &= 128 + 32 + 8 + 4 + 2;
                    if let Some(peer) = peer_index.and_then(|peer| walls.get_mut(peer)) {
                        peer.cstat &= 128 + 32 + 8 + 4 + 2;
                    }
                    let _ = lists.deletesprite(i);
                }
            } else {
                let _ = lists.deletesprite(i);
            }
            continue;
        }
        if marker.lotag != 3 && marker.lotag != 4 && marker.lotag != 12 {
            continue;
        }
        let Ok(si) = usize::try_from(marker.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get(si).copied() else {
            continue;
        };
        if temps.len() <= iu {
            temps.resize_with(iu + 1, ActorConTemps::default);
        }
        if marker.lotag == 3 {
            if temps[iu].g_t[4] != 0 {
                move_trigger_light(marker, sectors, walls, &mut temps[iu].g_t, global_random);
            }
            continue;
        }
        if marker.lotag == 12 {
            // SE 12 stores the unlit shades in T2/T3 during `spawn`.
            // Recreate that initialization for map actors whose CON temp data
            // is allocated by Room after world loading.
            if temps[iu].g_t[5] != i32::MIN + 12 {
                temps[iu].g_t[1] = i32::from(sector.floorshade);
                temps[iu].g_t[2] = i32::from(sector.ceilingshade);
                temps[iu].g_t[5] = i32::MIN + 12;
            }
            move_light_switch(iu, marker, sprites, sectors, walls, lists, temps);
            continue;
        }
        if temps[iu].g_t[5] != i32::MIN {
            temps[iu].g_t[2] = i32::from(sector.floorshade);
            temps[iu].g_t[3] = 0;
            sprites[iu].owner =
                ((i32::from(sector.ceilingpal) << 8) | i32::from(sector.floorpal)) as i16;
            let start = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
            let wall_count = usize::try_from(sector.wallnum).unwrap_or_default();
            let end = start.saturating_add(wall_count);
            temps[iu].g_t[3] = walls
                .get(start..end.min(walls.len()))
                .unwrap_or_default()
                .iter()
                .map(|wall| i32::from(wall.shade))
                .max()
                .unwrap_or(0)
                .max(0);
            temps[iu].g_t[5] = i32::MIN;
        }

        let hitag = i32::from(marker.hitag).saturating_add(1).max(1);
        let bright = (global_random / hitag & 31) < 4;
        let (shade, wall_shade, palette) = if bright {
            let shade = i32::from(marker.shade) + (global_random & 15);
            let palette = i32::from(sprites[iu].owner);
            (shade, shade, palette)
        } else {
            (temps[iu].g_t[2], temps[iu].g_t[3], i32::from(marker.pal))
        };
        let shade = shade.clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8;
        let wall_shade = wall_shade.clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8;
        let sector = &mut sectors[si];
        sector.floorshade = shade;
        sector.ceilingshade = shade;
        if bright {
            sector.ceilingpal = (palette >> 8) as u8;
            sector.floorpal = palette as u8;
        } else {
            sector.ceilingpal = marker.pal;
            sector.floorpal = marker.pal;
        }
        let start = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
        let wall_count = usize::try_from(sector.wallnum).unwrap_or_default();
        let end = start.saturating_add(wall_count).min(walls.len());
        for wi in start..end {
            walls[wi].pal = palette as u8;
            if walls[wi].hitag != 1 {
                walls[wi].shade = wall_shade;
                let peer = walls[wi].nextwall;
                if walls[wi].cstat & 2 != 0
                    && let Ok(peer) = usize::try_from(peer)
                    && let Some(peer_wall) = walls.get_mut(peer)
                {
                    peer_wall.shade = wall_shade;
                }
            }
        }
        let mut linked_sprite = lists.headspritesect[si];
        while linked_sprite >= 0 {
            let index = linked_sprite as usize;
            linked_sprite = lists.nextspritesect[index];
            if let Some(sprite) = sprites.get_mut(index)
                && sprite.cstat & 16 != 0
            {
                sprite.shade = shade;
            }
        }
    }
}

/// Chocolate SE 8 / SE 9 (`game.c::spawn` + `actors.c::moveeffectors`):
/// elevator / door lights that fade linked sectors while the effector's
/// sector ceiling is animating.
///
/// `spawn` stores T1/T2 = floor/ceiling shade, T3 = brightest wall shade
/// (at least 0) and T4 = 1 (the per-tic shade step). `moveeffectors` only
/// acts while `getanimationgoal(&ceilingz) >= 0`, or while T5 (set by
/// activators for other effector kinds, never by stock SE 8/9 spawn) counts
/// up to its kill at 9.
pub(crate) fn move_elevator_shade_effectors(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    temps: &mut Vec<ActorConTemps>,
    ceiling_animating: &dyn Fn(usize) -> bool,
) {
    // Spawn-time snapshot, done once per effector before its first move.
    let mut scan = lists.headspritestat[STAT_EFFECTOR as usize];
    while scan >= 0 {
        let index = scan as usize;
        scan = lists.nextspritestat[index];
        let Some(marker) = sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 8 && marker.lotag != 9 {
            continue;
        }
        let Ok(si) = usize::try_from(marker.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get(si).copied() else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        if temps[index].g_t[5] != i32::MIN + i32::from(marker.lotag) {
            temps[index].g_t[0] = i32::from(sector.floorshade);
            temps[index].g_t[1] = i32::from(sector.ceilingshade);
            temps[index].g_t[2] = sector_shades(sector, walls);
            temps[index].g_t[3] = 1;
            temps[index].g_t[5] = i32::MIN + i32::from(marker.lotag);
            if marker.lotag == 9 && sector.lotag != 0 && sector.ceilingz.abs_diff(marker.z) > 1024 {
                sectors[si].lotag |= i16::MIN;
            }
        }
    }

    let mut i = lists.headspritestat[STAT_EFFECTOR as usize];
    while i >= 0 {
        let index = i as usize;
        i = lists.nextspritestat[index];
        let Some(marker) = sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 8 && marker.lotag != 9 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(marker.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get(sector_index).copied() else {
            continue;
        };
        let active = if temps[index].g_t[4] != 0 {
            temps[index].g_t[4] += 1;
            if temps[index].g_t[4] > 8 {
                let _ = lists.deletesprite(index as i16);
                if let Some(sprite) = sprites.get_mut(index) {
                    sprite.statnum = room_build_engine::world::MAXSTATUS as i16;
                }
                continue;
            }
            true
        } else {
            ceiling_animating(sector_index)
        };
        if !active {
            continue;
        }
        let mut x = if (sector.lotag as u16 & 0x8000) != 0 || temps[index].g_t[4] != 0 {
            -temps[index].g_t[3]
        } else {
            temps[index].g_t[3]
        };
        if marker.lotag == 9 {
            x = -x;
        }
        let mut linked = lists.headspritestat[STAT_EFFECTOR as usize];
        while linked >= 0 {
            let linked_index = linked as usize;
            linked = lists.nextspritestat[linked_index];
            let Some(other) = sprites.get(linked_index).copied() else {
                continue;
            };
            if other.lotag != marker.lotag || other.hitag != marker.hitag {
                continue;
            }
            let Ok(si) = usize::try_from(other.sectnum) else {
                continue;
            };
            let m = other.shade;
            let lt = temps.get(linked_index).map_or([0; 6], |t| t.g_t);
            let Some(target) = sectors.get(si).copied() else {
                continue;
            };
            let start = usize::try_from(target.wallptr).unwrap_or(usize::MAX);
            let end = start
                .saturating_add(usize::try_from(target.wallnum).unwrap_or_default())
                .min(walls.len());
            for wi in start..end {
                if walls[wi].hitag == 1 {
                    continue;
                }
                let shade = choc_shade_add(walls[wi].shade, x, m, lt[2]);
                walls[wi].shade = shade;
                if let Ok(peer) = usize::try_from(walls[wi].nextwall)
                    && let Some(peer_wall) = walls.get_mut(peer)
                    && peer_wall.hitag != 1
                {
                    peer_wall.shade = shade;
                }
            }
            let target = &mut sectors[si];
            target.floorshade = choc_shade_add(target.floorshade, x, m, lt[0]);
            target.ceilingshade = choc_shade_add(target.ceilingshade, x, m, lt[1]);
        }
    }
}

/// Chocolate `shade += x; if (shade < m) shade = m; else if (shade > t) shade = t;`
/// on a signed-char shade (the add wraps, `t` truncates on store).
fn choc_shade_add(current: i8, x: i32, m: i8, t: i32) -> i8 {
    let next = (i32::from(current) + x) as i8;
    if next < m {
        m
    } else if i32::from(next) > t {
        t as i8
    } else {
        next
    }
}

/// Advance the Atomic SE16 reactor ceiling mover.
///
/// Chocolate raises the ceiling by 1024 while the reactor is present, then
/// reverses it by 512 after it reaches the sector's lower plane. If the ceiling
/// has passed the stored T4 threshold and neither reactor tile remains in the
/// sector, the effector deletes itself.
pub(crate) fn move_reactor_effectors(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    temps: &mut Vec<ActorConTemps>,
) {
    let mut i = lists.headspritestat[STAT_EFFECTOR as usize];
    while i >= 0 {
        let index = i as usize;
        i = lists.nextspritestat[index];
        let Some(marker) = sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 16 {
            continue;
        }
        let Ok(si) = usize::try_from(marker.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get(si) else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        temps[index].g_t[2] = temps[index].g_t[2].wrapping_add(32);

        if sector.floorz < sector.ceilingz {
            sprites[index].shade = 0;
        } else if sector.ceilingz < temps[index].g_t[3] {
            let mut actor = lists.headspritesect[si];
            let mut reactor_found = false;
            while actor >= 0 {
                let actor_index = actor as usize;
                actor = lists.nextspritesect[actor_index];
                if sprites.get(actor_index).is_some_and(|sprite| {
                    matches!(
                        sprite.picnum,
                        crate::spawn::REACTOR | crate::spawn::REACTOR2
                    )
                }) {
                    reactor_found = true;
                    break;
                }
            }
            if !reactor_found {
                let _ = lists.deletesprite(index as i16);
                if let Some(sprite) = sprites.get_mut(index) {
                    sprite.statnum = room_build_engine::world::MAXSTATUS as i16;
                }
                continue;
            }
            sprites[index].shade = 1;
        }

        if let Some(sector) = sectors.get_mut(si) {
            sector.ceilingz = sector
                .ceilingz
                .saturating_add(if sprites[index].shade != 0 {
                    1024
                } else {
                    -512
                });
        }
    }
}

/// Chocolate `moveeffectors` case 33: while an earthquake runs, each SE33
/// marker has a 1-in-8 chance per tic to throw one `RANDOMSCRAP` shard.
pub(crate) fn move_quake_debris_effectors(
    world: &mut BuildWorld,
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    earthquake_time: i32,
) {
    if earthquake_time <= 0 {
        return;
    }
    let mut current = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while current >= 0 {
        let index = current as usize;
        current = world.lists.nextspritestat[index];
        let Some(marker) = world.sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 33 || krand_step(randomseed) & 7 != 0 {
            continue;
        }
        // 32-bit gcc evaluates the EGS arguments right to left.
        let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
        let xvel = (64 + (krand_step(randomseed) & 63)) as i16;
        let angle = (krand_step(randomseed) & 2047) as i16;
        let tile = SCRAP6 + (krand_step(randomseed) & 15) as i16;
        let z = marker
            .z
            .wrapping_sub(8 << 8)
            .wrapping_sub(krand_step(randomseed) & 8191);
        let y = marker.y.wrapping_add((krand_step(randomseed) & 255) - 128);
        let x = marker.x.wrapping_add((krand_step(randomseed) & 255) - 128);
        let _ = crate::shoot::egs(
            &mut world.lists,
            &mut world.sprites[..],
            picnum,
            temps,
            script,
            marker.sectnum,
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
            index as i16,
            crate::stat::STAT_MISC,
        );
    }
}

/// Advance Chocolate's SE13 explosive-sector recovery (`moveeffectors`).
/// SEENINE detonation arms T3; the effector restores its original planes over
/// 256 tics, changes the sector shade once, and then removes the marker.
pub(crate) fn move_explosive_sector_effectors(
    world: &mut BuildWorld,
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
) {
    let mut current = world.lists.headspritestat[STAT_EFFECTOR as usize];
    while current >= 0 {
        let index = current as usize;
        current = world.lists.nextspritestat[index];
        let Some(marker) = world.sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 13 || index >= temps.len() || temps[index].g_t[2] == 0 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(marker.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        let step = (i32::from(marker.yvel) << 5) | 1;
        let approach = |plane: i32, target: i32| {
            let delta = target.wrapping_sub(plane);
            if delta.unsigned_abs() >= step as u32 {
                plane.wrapping_add(if delta < 0 { -step } else { step })
            } else {
                target
            }
        };
        if marker.ang == 512 {
            if marker.owner != 0 {
                world.sectors[sector_index].ceilingz =
                    approach(sector.ceilingz, temps[index].g_t[0]);
            } else {
                world.sectors[sector_index].floorz = approach(sector.floorz, temps[index].g_t[1]);
            }
        } else {
            world.sectors[sector_index].floorz = approach(sector.floorz, temps[index].g_t[1]);
            // Atomic's code performs this attempted step and then assigns the
            // stored ceiling directly, so retain the resulting snap.
            let _ = approach(sector.ceilingz, temps[index].g_t[0]);
            world.sectors[sector_index].ceilingz = temps[index].g_t[0];
        }
        if temps[index].g_t[3] == 1 {
            temps[index].g_t[3] += 1;
            // Chocolate prelevel caches the first map-order parallax sector
            // once, before SE13 setup or runtime effectors can flip its flag.
            let sky = temps[index].se13_parallax_sky;
            world.sectors[sector_index].ceilingstat ^= 1;
            if marker.ang == 512 {
                world.sectors[sector_index].floorshade = marker.shade;
                // Chocolate prelevel stores the first map-order sector with
                // a parallax ceiling as `one_parallax_sectnum`.
                if let Some((sky_tile, sky_shade)) = sky {
                    world.sectors[sector_index].ceilingpicnum = sky_tile;
                    world.sectors[sector_index].ceilingshade = sky_shade;
                }
                let start = sector.wallptr.max(0) as usize;
                let end = start.saturating_add(sector.wallnum.max(0) as usize);
                let wall_count = world.walls.len();
                for wall in world
                    .walls
                    .get_mut(start..end.min(wall_count))
                    .unwrap_or_default()
                {
                    wall.shade = marker.shade;
                }
            }
        }
        temps[index].g_t[2] = temps[index].g_t[2].wrapping_add(1);
        let debris = temps[index].g_t[2] == 4 && marker.ang != 512;
        let remove = temps[index].g_t[2] > 256;
        if remove {
            let _ = world.lists.deletesprite(index as i16);
            if picnum.len() <= index {
                picnum.resize(index + 1, 0);
            }
            picnum[index] = -1;
        }
        if debris {
            for _ in 0..7 {
                // Chocolate `RANDOMSCRAP`: 32-bit gcc evaluates the EGS arguments right
                // to left, so zvel draws first and x last.
                let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
                let xvel = (64 + (krand_step(randomseed) & 63)) as i16;
                let angle = (krand_step(randomseed) & 2047) as i16;
                let tile = SCRAP6 + (krand_step(randomseed) & 15) as i16;
                let z = marker
                    .z
                    .wrapping_sub(8 << 8)
                    .wrapping_sub(krand_step(randomseed) & 8191);
                let y = marker.y.wrapping_add((krand_step(randomseed) & 255) - 128);
                let x = marker.x.wrapping_add((krand_step(randomseed) & 255) - 128);
                let _ = crate::shoot::egs(
                    &mut world.lists,
                    &mut world.sprites[..],
                    picnum,
                    temps,
                    script,
                    marker.sectnum,
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
                    index as i16,
                    crate::stat::STAT_MISC,
                );
            }
        }
    }
}

/// Run Atomic's SE1 pivot bookkeeping and SE19 hit-triggered shield shutdown.
/// A sector-local EXPLOSION2 activates the matching hitag group; its SE19
/// marker then lowers the sector ceiling and removes linked BIGFORCE walls.
pub(crate) fn move_battlestar_shields(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    temps: &mut Vec<ActorConTemps>,
) {
    const EXPLOSION2: i16 = 1890;
    const BIGFORCE: i16 = 230;
    const WALL_FLAGS: u16 = 128 + 32 + 8 + 4 + 2;

    let mut marker = lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = lists.nextspritestat[index];
        let Some(effect) = sprites.get(index).copied() else {
            continue;
        };
        if !matches!(effect.lotag, 1 | 19) {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        if sector_index >= sectors.len() {
            continue;
        }
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }

        if effect.lotag == 1 {
            if sprites[index].owner == -1 {
                sprites[index].owner = index as i16;
                let mut candidate = lists.headspritestat[STAT_EFFECTOR as usize];
                while candidate >= 0 {
                    let candidate_index = candidate as usize;
                    candidate = lists.nextspritestat[candidate_index];
                    if sprites
                        .get(candidate_index)
                        .is_some_and(|sprite| sprite.lotag == 19 && sprite.hitag == effect.hitag)
                    {
                        if temps.len() <= candidate_index {
                            temps.resize_with(candidate_index + 1, ActorConTemps::default);
                        }
                        temps[candidate_index].g_t[0] = 0;
                        break;
                    }
                }
            }
            continue;
        }

        let active = temps[index].g_t[0];
        if active == 0 {
            // `IFHITSECT` expands to `ifhitsectors(sectnum)`, which searches
            // only STAT_MISC and accepts EXPLOSION2 in that exact sector.
            let mut actor = lists.headspritestat[crate::stat::STAT_MISC as usize];
            let mut hit = false;
            while actor >= 0 {
                let actor_index = actor as usize;
                actor = lists.nextspritestat[actor_index];
                if sprites.get(actor_index).is_some_and(|sprite| {
                    sprite.picnum == EXPLOSION2 && sprite.sectnum == effect.sectnum
                }) {
                    hit = true;
                    break;
                }
            }
            if !hit {
                continue;
            }

            let mut linked = lists.headspritestat[STAT_EFFECTOR as usize];
            while linked >= 0 {
                let linked_index = linked as usize;
                linked = lists.nextspritestat[linked_index];
                if !sprites.get(linked_index).is_some_and(|sprite| {
                    matches!(sprite.lotag & 0x7fff, 0 | 1 | 12 | 19) && sprite.hitag == effect.hitag
                }) {
                    continue;
                }
                if temps.len() <= linked_index {
                    temps.resize_with(linked_index + 1, ActorConTemps::default);
                }
                if temps[linked_index].g_t[0] == 0 {
                    temps[linked_index].g_t[0] = 1;
                    sprites[linked_index].owner = index as i16;
                }
            }
            continue;
        }

        if active == 1 {
            let Ok(wall_start) = usize::try_from(sectors[sector_index].wallptr) else {
                continue;
            };
            let Ok(wall_count) = usize::try_from(sectors[sector_index].wallnum) else {
                continue;
            };
            let wall_end = wall_start.saturating_add(wall_count);
            for wall_index in wall_start..wall_end.min(walls.len()) {
                if walls[wall_index].overpicnum != BIGFORCE {
                    continue;
                }
                let nextwall = walls[wall_index].nextwall;
                walls[wall_index].cstat &= WALL_FLAGS;
                walls[wall_index].overpicnum = 0;
                if let Ok(peer) = usize::try_from(nextwall)
                    && let Some(peer_wall) = walls.get_mut(peer)
                {
                    peer_wall.overpicnum = 0;
                    peer_wall.cstat &= WALL_FLAGS;
                }
            }
            temps[index].g_t[0] = 2;
        }

        let completed = {
            let Some(sector) = sectors.get_mut(sector_index) else {
                continue;
            };
            if sector.ceilingz < sector.floorz {
                sector.ceilingz = sector.ceilingz.saturating_add(i32::from(effect.yvel));
                false
            } else {
                sector.ceilingz = sector.floorz;
                true
            }
        };
        if !completed {
            continue;
        }
        let source_sector = usize::try_from(effect.sectnum).ok();
        if let Some((shade, pal)) = source_sector.and_then(|source| {
            sectors
                .get(source)
                .map(|source| (source.floorshade, source.floorpal))
        }) {
            let mut linked = lists.headspritestat[STAT_EFFECTOR as usize];
            while linked >= 0 {
                let linked_index = linked as usize;
                linked = lists.nextspritestat[linked_index];
                let Some(target) = sprites.get(linked_index).copied() else {
                    continue;
                };
                if target.lotag != 0 || target.owner != index as i16 {
                    continue;
                }
                if let Ok(target_sector) = usize::try_from(target.sectnum)
                    && let Some(target) = sectors.get_mut(target_sector)
                {
                    target.floorpal = pal;
                    target.ceilingpal = pal;
                    target.floorshade = shade;
                    target.ceilingshade = shade;
                }
            }
        }
        let mut pivot = lists.headspritestat[STAT_EFFECTOR as usize];
        while pivot >= 0 {
            let pivot_index = pivot as usize;
            pivot = lists.nextspritestat[pivot_index];
            if sprites
                .get(pivot_index)
                .is_some_and(|sprite| sprite.lotag == 1 && sprite.hitag == effect.hitag)
            {
                if temps.len() <= pivot_index {
                    temps.resize_with(pivot_index + 1, ActorConTemps::default);
                }
                temps[pivot_index].g_t[0] = 2;
            }
        }
        let _ = lists.deletesprite(index as i16);
        sprites[index].statnum = room_build_engine::world::MAXSTATUS as i16;
    }
}

/// Advance Atomic SE2 quakes that have been armed through their CON temp data.
/// This includes the 96-tic marker lifetime, slope-heinum restoration, grounded
/// player/sprite impulse, global view-shake timer and earthquake cue cadence.
pub(crate) fn move_quake_effectors(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &[WallT],
    players: &mut [Player],
    temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    quake_time: &mut u8,
    pending_sound_ids: &mut Vec<u16>,
) {
    let sector_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();

    let mut marker = lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = lists.nextspritestat[index];
        let Some(effect) = sprites.get(index).copied() else {
            continue;
        };
        if effect.lotag == 33 {
            if *quake_time > 0 && (room_build_engine::numeric::krand_step(randomseed) & 7) == 0 {
                // Chocolate `RANDOMSCRAP`: 32-bit gcc evaluates the EGS arguments right
                // to left, so zvel draws first and x last.
                let zvel =
                    (-512 - (room_build_engine::numeric::krand_step(randomseed) & 2047)) as i16;
                let xvel = (64 + (room_build_engine::numeric::krand_step(randomseed) & 63)) as i16;
                let angle = (room_build_engine::numeric::krand_step(randomseed) & 2047) as i16;
                let scrap =
                    SCRAP6 + (room_build_engine::numeric::krand_step(randomseed) & 15) as i16;
                let z = effect
                    .z
                    .wrapping_sub(8 << 8)
                    .wrapping_sub(room_build_engine::numeric::krand_step(randomseed) & 8191);
                let y = effect
                    .y
                    .wrapping_add((room_build_engine::numeric::krand_step(randomseed) & 255) - 128);
                let x = effect
                    .x
                    .wrapping_add((room_build_engine::numeric::krand_step(randomseed) & 255) - 128);
                let _ = crate::shoot::egs(
                    lists,
                    sprites,
                    picnum,
                    temps,
                    script,
                    effect.sectnum,
                    x,
                    y,
                    z,
                    scrap,
                    -8,
                    48,
                    48,
                    angle,
                    xvel,
                    zvel,
                    index as i16,
                    crate::stat::STAT_MISC,
                );
            }
            continue;
        }
        if effect.lotag != 2 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(effect.sectnum) else {
            continue;
        };
        if sector_index >= sectors.len() {
            continue;
        }
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }

        if temps[index].g_t[4] > 0 && temps[index].g_t[0] == 0 {
            if temps[index].g_t[4] < i32::from(effect.hitag) {
                temps[index].g_t[4] += 1;
            } else {
                temps[index].g_t[0] = 1;
            }
        }
        if temps[index].g_t[0] <= 0 {
            continue;
        }

        temps[index].g_t[0] += 1;
        sprites[index].xvel = 3;
        if temps[index].g_t[0] > 96 {
            temps[index].g_t[0] = -1;
            temps[index].g_t[4] = -1;
            let _ = lists.deletesprite(index as i16);
            sprites[index].statnum = room_build_engine::world::MAXSTATUS as i16;
            continue;
        }
        if (temps[index].g_t[0] & 31) == 8 {
            *quake_time = 48;
            pending_sound_ids.push(81); // Atomic EARTHQUAKE.
        }

        let target_heinum = temps[index].g_t[5].clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        let current_heinum = i32::from(sectors[sector_index].floorheinum);
        let diff = target_heinum - current_heinum;
        if diff.abs() < 8 {
            sectors[sector_index].floorheinum = target_heinum as i16;
        } else {
            sectors[sector_index].floorheinum = current_heinum
                .saturating_add(diff.signum() * 16)
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                as i16;
        }

        let angle = i32::from(effect.ang).rem_euclid(2048);
        let (sin, cos) = sincos_q14(angle);
        let dx = (3 * cos) >> 14;
        let dy = (3 * sin) >> 14;
        for player in players.iter_mut() {
            if player.cursectnum == effect.sectnum && player.on_ground {
                player.posx = player.posx.wrapping_add(dx);
                player.posy = player.posy.wrapping_add(dy);
                player.bobposx = player.bobposx.wrapping_add(dx);
                player.bobposy = player.bobposy.wrapping_add(dy);
            }
        }

        let mut actor = lists.headspritesect[sector_index];
        while actor >= 0 {
            let actor_index = actor as usize;
            actor = lists.nextspritesect[actor_index];
            if sprites
                .get(actor_index)
                .is_none_or(|sprite| sprite.picnum == 1)
            {
                continue;
            }
            sprites[actor_index].x = sprites[actor_index].x.wrapping_add(dx);
            sprites[actor_index].y = sprites[actor_index].y.wrapping_add(dy);
            let mut new_sector = sprites[actor_index].sectnum;
            let _ = updatesector(
                sprites[actor_index].x,
                sprites[actor_index].y,
                &mut new_sector,
                &sector_walls,
                &wall_xy,
            );
            if new_sector >= 0 && new_sector != sprites[actor_index].sectnum {
                sprites[actor_index].sectnum = new_sector;
                let _ = lists.changespritesect(actor_index as i16, new_sector);
            }
        }
        sprites[index].x = sprites[index].x.wrapping_add(dx);
        sprites[index].y = sprites[index].y.wrapping_add(dy);
        let old_sector = sprites[index].sectnum;
        let mut new_sector = old_sector;
        let _ = updatesector(
            sprites[index].x,
            sprites[index].y,
            &mut new_sector,
            &sector_walls,
            &wall_xy,
        );
        if new_sector >= 0 && new_sector != old_sector {
            sprites[index].sectnum = new_sector;
            let _ = lists.changespritesect(index as i16, new_sector);
        }
    }
    // Chocolate decrements `earthquaketime` in the game-tic tail, after
    // moveeffectors has had a chance to set it to 48.
    if *quake_time > 0 {
        *quake_time -= 1;
    }
}

/// Chocolate `actors.c::moveeffectors` timed explosion emitters (SE130/SE131).
/// The marker's `T1` limits its lifetime; each live tic has the source's `rnd`
/// gate and spawns one moving EXPLOSION2 with ordered TRAND-backed fields.
pub(crate) fn move_explosion_effectors(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum: &mut Vec<i16>,
    temps: &mut Vec<ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
) {
    let sector_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let mut marker = lists.headspritestat[STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = lists.nextspritestat[index];
        let Some(effector) = sprites.get(index).copied() else {
            continue;
        };
        let (life_limit, roll_limit, repeat_mask) = match effector.lotag {
            130 => (80, 64, 7),
            131 => (40, 32, 3),
            _ => continue,
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        if temps[index].g_t[0] > life_limit {
            let _ = lists.deletesprite(index as i16);
            sprites[index].statnum = room_build_engine::world::MAXSTATUS as i16;
            continue;
        }
        temps[index].g_t[0] += 1;
        let Ok(sector_index) = usize::try_from(effector.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get(sector_index) else {
            continue;
        };
        let gap = sector.floorz.saturating_sub(sector.ceilingz);
        let fires = krand_step(randomseed) >> 8 >= 255 - roll_limit;
        if !fires || gap <= 0 {
            continue;
        }
        let explosion = crate::shoot::egs(
            lists,
            sprites,
            picnum,
            temps,
            script,
            effector.sectnum,
            effector.x,
            effector.y,
            effector.z,
            EXPLOSION2,
            0,
            0,
            0,
            effector.ang,
            0,
            0,
            index as i16,
            crate::stat::STAT_MISC,
        );
        let repeats = 2 + (krand_step(randomseed) & repeat_mask);
        let z = sector
            .floorz
            .wrapping_sub(krand_step(randomseed).rem_euclid(gap));
        let angle = effector
            .ang
            .wrapping_add(256 - krand_step(randomseed).rem_euclid(511) as i16);
        let xvel = (krand_step(randomseed) & 127) as i16;
        if explosion < 0 {
            continue;
        }
        let explosion_index = explosion as usize;
        sprites[explosion_index].xrepeat = repeats as u8;
        sprites[explosion_index].yrepeat = repeats as u8;
        sprites[explosion_index].z = z;
        sprites[explosion_index].ang = angle;
        sprites[explosion_index].xvel = xvel;

        // Chocolate calls `ssp(k, CLIPMASK0)` immediately after setting xvel.
        let (sin, cos) = sincos_q14(i32::from(angle));
        let dx = (i32::from(xvel) * cos) >> 14;
        let dy = (i32::from(xvel) * sin) >> 14;
        let old_sector = sprites[explosion_index].sectnum;
        let clipdist = i32::from(sprites[explosion_index].clipdist) << 2;
        let _ = clipmove_xy(
            &mut sprites[explosion_index].x,
            &mut sprites[explosion_index].y,
            &mut sprites[explosion_index].sectnum,
            dx,
            dy,
            clipdist,
            &sector_walls,
            &wall_xy,
        );
        if sprites[explosion_index].sectnum != old_sector {
            let _ = lists.changespritesect(explosion, sprites[explosion_index].sectnum);
        }
    }
}

/// Advance Chocolate's SE21 cascading plane effectors activated by sector 28.
pub(crate) fn move_cascade_effectors(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    temps: &mut Vec<ActorConTemps>,
) {
    let mut i = lists.headspritestat[STAT_EFFECTOR as usize];
    while i >= 0 {
        let index = i as usize;
        i = lists.nextspritestat[index];
        let Some(marker) = sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 21 || temps.get(index).is_none_or(|temp| temp.g_t[0] == 0) {
            continue;
        }
        let Ok(sector_index) = usize::try_from(marker.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get_mut(sector_index) else {
            continue;
        };
        if temps[index].g_t[0] == 1 {
            let plane = if marker.ang == 1536 {
                sector.ceilingz
            } else {
                sector.floorz
            };
            let direction = (i32::from(marker.z).saturating_sub(plane)).signum();
            sprites[index].zvel =
                direction.saturating_mul(i32::from(marker.hitag).wrapping_shl(4)) as i16;
            temps[index].g_t[0] = 2;
        }
        if sector.extra == 0 {
            let velocity = i32::from(sprites[index].zvel);
            let plane = if marker.ang == 1536 {
                &mut sector.ceilingz
            } else {
                &mut sector.floorz
            };
            *plane = plane.saturating_add(velocity);
            if plane.abs_diff(marker.z) < 1024 {
                *plane = i32::from(marker.z);
                let _ = lists.deletesprite(index as i16);
                sprites[index].statnum = room_build_engine::world::MAXSTATUS as i16;
            }
        } else {
            sector.extra = sector.extra.wrapping_sub(1);
        }
    }
}

/// Initialize Chocolate SE17 stop planes (`spawn`, T3/T4/T5).
pub(crate) fn init_warp_elevator_effectors(
    lists: &SpriteLists,
    sprites: &[SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    temps: &mut Vec<ActorConTemps>,
) {
    let mut i = lists.headspritestat[STAT_EFFECTOR as usize];
    while i >= 0 {
        let index = i as usize;
        i = lists.nextspritestat[index];
        let Some(marker) = sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 17 {
            continue;
        }
        let Ok(si) = usize::try_from(marker.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get(si).copied() else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        if temps[index].g_t[5] == i32::MIN + 17 {
            continue;
        }
        temps[index].g_t[2] = sector.floorz;
        temps[index].g_t[3] =
            adjacent_plane(sector, sectors, walls, sector.floorz, false, false).unwrap_or(i32::MIN);
        temps[index].g_t[4] =
            adjacent_plane(sector, sectors, walls, sector.ceilingz, true, true).unwrap_or(i32::MAX);
        temps[index].g_t[5] = i32::MIN + 17;
    }
}

/// Advance Chocolate's SE17 elevator transports and move linked sector contents.
pub(crate) fn move_warp_elevator_effectors(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    players: &mut [crate::player::Player],
    temps: &mut Vec<ActorConTemps>,
) {
    let multiplayer = players.len() > 1;
    let mut i = lists.headspritestat[STAT_EFFECTOR as usize];
    while i >= 0 {
        let index = i as usize;
        i = lists.nextspritestat[index];
        let Some(marker) = sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 17 {
            continue;
        }
        let Ok(si) = usize::try_from(marker.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get(si).copied() else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        if temps[index].g_t[5] != i32::MIN + 17 {
            // Maps entered before this native path was added still initialize
            // safely from their current local planes.
            temps[index].g_t[2] = sector.floorz;
            temps[index].g_t[3] = i32::MIN;
            temps[index].g_t[4] = i32::MAX;
            temps[index].g_t[5] = i32::MIN + 17;
        }

        let direction = temps[index].g_t[0];
        let speed = i32::from(marker.yvel).saturating_mul(4);
        let delta = direction.saturating_mul(speed);
        sectors[si].floorz = sector.floorz.saturating_add(delta);
        sectors[si].ceilingz = sector.ceilingz.saturating_add(delta);

        let mut actor = lists.headspritesect[si];
        while actor >= 0 {
            let actor_index = actor as usize;
            actor = lists.nextspritesect[actor_index];
            if actor_index >= sprites.len() {
                continue;
            }
            if lists.sprite_stat[actor_index] == crate::stat::STAT_PLAYER
                && sprites[actor_index].owner >= 0
            {
                let player_index = usize::from(sprites[actor_index].yvel as u16);
                if let Some(player) = players.get_mut(player_index) {
                    if !multiplayer {
                        player.oposz = player.posz;
                    }
                    player.posz = player.posz.saturating_add(delta);
                    player.true_floor_z = player.true_floor_z.saturating_add(delta);
                    player.true_ceiling_z = player.true_ceiling_z.saturating_add(delta);
                    if multiplayer {
                        player.oposz = player.posz;
                    }
                }
            }
            if lists.sprite_stat[actor_index] != STAT_EFFECTOR {
                if temps.len() <= actor_index {
                    temps.resize_with(actor_index + 1, ActorConTemps::default);
                }
                temps[actor_index].bposz = sprites[actor_index].z;
                sprites[actor_index].z = sprites[actor_index].z.saturating_add(delta);
            }
            if temps.len() <= actor_index {
                temps.resize_with(actor_index + 1, ActorConTemps::default);
            }
            temps[actor_index].hit_floorz = sectors[si].floorz;
            temps[actor_index].hit_ceilingz = sectors[si].ceilingz;
        }

        if direction == 0 {
            continue;
        }
        let tolerance = i32::from(marker.yvel).abs();
        if sectors[si].floorz.abs_diff(temps[index].g_t[2]) <= tolerance as u32 {
            set_warp_elevator_direction(sprites, lists, temps, marker.hitag, 0);
            // Chocolate's activatewarpelevators(..., 0) plays ELEVATOR_OFF
            // on the effector that reaches its saved stop floor.
            temps[index].pending_actor_sounds.push(73);
            continue;
        }
        if (direction == -1 && sectors[si].floorz > temps[index].g_t[3])
            || (direction != -1 && sectors[si].ceilingz < temps[index].g_t[4])
        {
            continue;
        }
        if temps[index].g_t[1] == 0 {
            continue;
        }
        temps[index].g_t[1] = 0;

        let destination = lists.headspritestat[STAT_EFFECTOR as usize];
        let mut paired = None;
        let mut scan = destination;
        while scan >= 0 {
            let candidate = scan as usize;
            scan = lists.nextspritestat[candidate];
            let Some(other) = sprites.get(candidate).copied() else {
                continue;
            };
            let Ok(other_sector) = usize::try_from(other.sectnum) else {
                continue;
            };
            let Some(other_sector_data) = sectors.get(other_sector) else {
                continue;
            };
            if candidate != index
                && other.lotag == 17
                && i32::from(sectors[si].hitag).saturating_sub(direction)
                    == i32::from(other_sector_data.hitag)
                && other.hitag == marker.hitag
            {
                paired = Some((candidate, other, other_sector));
                break;
            }
        }
        let Some((_pair_index, peer, peer_sector)) = paired else {
            continue;
        };
        let mut carried = lists.headspritesect[si];
        while carried >= 0 {
            let carried_index = carried as usize;
            let next = lists.nextspritesect[carried_index];
            if carried_index < sprites.len() && lists.sprite_stat[carried_index] != STAT_EFFECTOR {
                if lists.sprite_stat[carried_index] == crate::stat::STAT_PLAYER
                    && sprites[carried_index].owner >= 0
                {
                    let p = usize::from(sprites[carried_index].yvel as u16);
                    if let Some(player) = players.get_mut(p) {
                        player.posx = player.posx.saturating_add(peer.x - marker.x);
                        player.posy = player.posy.saturating_add(peer.y - marker.y);
                        player.posz = sectors[peer_sector]
                            .floorz
                            .saturating_sub(sectors[si].floorz.saturating_sub(player.posz));
                        player.bobposx = player.posx;
                        player.oposx = player.posx;
                        player.bobposy = player.posy;
                        player.oposy = player.posy;
                        player.oposz = player.posz;
                        player.cursectnum = peer.sectnum;
                        player.poszv = 0;
                        player.true_floor_z = sectors[peer_sector].floorz;
                        player.true_ceiling_z = sectors[peer_sector].ceilingz;
                        player.bob_counter = 0;
                        sprites[carried_index].x = player.posx;
                        sprites[carried_index].y = player.posy;
                        sprites[carried_index].z = player.posz + crate::player::PHEIGHT;
                    }
                } else {
                    sprites[carried_index].x =
                        sprites[carried_index].x.saturating_add(peer.x - marker.x);
                    sprites[carried_index].y =
                        sprites[carried_index].y.saturating_add(peer.y - marker.y);
                    sprites[carried_index].z = sectors[peer_sector].floorz.saturating_sub(
                        sectors[si].floorz.saturating_sub(sprites[carried_index].z),
                    );
                    if temps.len() <= carried_index {
                        temps.resize_with(carried_index + 1, ActorConTemps::default);
                    }
                    temps[carried_index].bposz = sprites[carried_index].z;
                }
                let _ = lists.changespritesect(carried as i16, peer.sectnum);
                sprites[carried_index].sectnum = peer.sectnum;
                if temps.len() <= carried_index {
                    temps.resize_with(carried_index + 1, ActorConTemps::default);
                }
                temps[carried_index].hit_floorz = sectors[peer_sector].floorz;
                temps[carried_index].hit_ceilingz = sectors[peer_sector].ceilingz;
            }
            carried = next;
        }
    }
}

fn set_warp_elevator_direction(
    sprites: &[SpriteT],
    lists: &SpriteLists,
    temps: &mut Vec<ActorConTemps>,
    hitag: i16,
    direction: i32,
) {
    let mut i = lists.headspritestat[STAT_EFFECTOR as usize];
    while i >= 0 {
        let index = i as usize;
        i = lists.nextspritestat[index];
        if sprites
            .get(index)
            .is_some_and(|marker| marker.lotag == 17 && marker.hitag == hitag)
        {
            if temps.len() <= index {
                temps.resize_with(index + 1, ActorConTemps::default);
            }
            temps[index].g_t[0] = direction;
            temps[index].g_t[1] = direction;
        }
    }
}

fn adjacent_plane(
    sector: SectorT,
    sectors: &[SectorT],
    walls: &[WallT],
    z: i32,
    target_floor: bool,
    greater_than: bool,
) -> Option<i32> {
    let start = usize::try_from(sector.wallptr).ok()?;
    let end = start
        .checked_add(usize::try_from(sector.wallnum).ok()?)?
        .min(walls.len());
    let mut best = None;
    for wall in walls.get(start..end)? {
        let Ok(neighbor) = usize::try_from(wall.nextsector) else {
            continue;
        };
        let Some(candidate) = sectors.get(neighbor) else {
            continue;
        };
        let value = if target_floor {
            candidate.floorz
        } else {
            candidate.ceilingz
        };
        let eligible = if greater_than { value > z } else { value < z };
        if eligible
            && best.is_none_or(|old: i32| {
                if greater_than {
                    value < old
                } else {
                    value > old
                }
            })
        {
            best = Some(value);
        }
    }
    best
}

fn sector_shades(sector: SectorT, walls: &[WallT]) -> i32 {
    let start = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
    let end = start
        .saturating_add(usize::try_from(sector.wallnum).unwrap_or_default())
        .min(walls.len());
    walls
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .map(|wall| i32::from(wall.shade))
        .max()
        .unwrap_or(0)
        .max(0)
}

fn shade_step(current: i8, step: i32, min: i8, max: i32) -> i8 {
    let next = i32::from(current).saturating_add(step);
    if next < i32::from(min) {
        min
    } else if next > max {
        max.clamp(-128, 127) as i8
    } else {
        next.clamp(-128, 127) as i8
    }
}

/// Advance Chocolate's ceiling-hit-triggered flickering sector light (SE 3).
fn move_trigger_light(
    marker: SpriteT,
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    temp: &mut [i32; 6],
    global_random: i32,
) {
    let Ok(si) = usize::try_from(marker.sectnum) else {
        return;
    };
    let Some(sector) = sectors.get_mut(si) else {
        return;
    };
    let denominator = i32::from(marker.hitag).saturating_add(1);
    let bright = denominator != 0 && (global_random / denominator & 31) < 4 && temp[2] == 0;
    let shade = if bright {
        let owner_palettes = i32::from(marker.owner);
        sector.ceilingpal = (owner_palettes >> 8) as u8;
        sector.floorpal = owner_palettes as u8;
        i32::from(marker.shade) + (global_random & 15)
    } else {
        sector.ceilingpal = marker.pal;
        sector.floorpal = marker.pal;
        temp[3]
    }
    .clamp(-128, 127) as i8;
    temp[0] = i32::from(shade);
    sector.ceilingshade = shade;
    sector.floorshade = shade;
    let start = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
    let end = start
        .saturating_add(usize::try_from(sector.wallnum).unwrap_or_default())
        .min(walls.len());
    for wi in start..end {
        if walls[wi].hitag == 1 {
            continue;
        }
        walls[wi].shade = shade;
        if walls[wi].cstat & 2 != 0
            && let Ok(peer) = usize::try_from(walls[wi].nextwall)
            && let Some(peer) = walls.get_mut(peer)
        {
            peer.shade = shade;
        }
    }
}

/// Advance Chocolate's SE 12 switch-controlled light and extinguish path.
fn move_light_switch(
    index: usize,
    marker: SpriteT,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    lists: &mut SpriteLists,
    temps: &mut [ActorConTemps],
) {
    let Ok(si) = usize::try_from(marker.sectnum) else {
        return;
    };
    let Some(sector) = sectors.get_mut(si) else {
        return;
    };
    let temp = &mut temps[index].g_t;
    if temp[0] == 3 || temp[3] == 1 {
        sector.floorpal = 0;
        sector.ceilingpal = 0;
        let start = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
        let end = start
            .saturating_add(usize::try_from(sector.wallnum).unwrap_or_default())
            .min(walls.len());
        for wall in walls.get_mut(start..end).unwrap_or_default() {
            if wall.hitag != 1 {
                wall.shade = temp[1].clamp(-128, 127) as i8;
                wall.pal = 0;
            }
        }
        sector.floorshade = temp[1].clamp(-128, 127) as i8;
        sector.ceilingshade = temp[2].clamp(-128, 127) as i8;
        temp[0] = 0;
        let shade = if sector.ceilingstat & 1 != 0 {
            sector.ceilingshade
        } else {
            sector.floorshade
        };
        let mut sprite_i = lists.headspritesect[si];
        while sprite_i >= 0 {
            let su = sprite_i as usize;
            sprite_i = lists.nextspritesect[su];
            if let Some(sprite) = sprites.get_mut(su)
                && sprite.cstat & 16 != 0
            {
                sprite.shade = shade;
            }
        }
        if temp[3] == 1 {
            let _ = lists.deletesprite(index as i16);
            if let Some(sprite) = sprites.get_mut(index) {
                sprite.statnum = room_build_engine::world::MAXSTATUS as i16;
            }
            return;
        }
    }
    if temp[0] == 1 {
        if i32::from(sector.floorshade) > i32::from(marker.shade) {
            sector.floorpal = marker.pal;
            sector.ceilingpal = marker.pal;
            sector.floorshade = sector.floorshade.saturating_sub(2);
            sector.ceilingshade = sector.ceilingshade.saturating_sub(2);
            let start = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
            let end = start
                .saturating_add(usize::try_from(sector.wallnum).unwrap_or_default())
                .min(walls.len());
            for wall in walls.get_mut(start..end).unwrap_or_default() {
                if wall.hitag != 1 {
                    wall.pal = marker.pal;
                    wall.shade = wall.shade.saturating_sub(2);
                }
            }
        } else {
            temp[0] = 2;
        }
        let shade = if sector.ceilingstat & 1 != 0 {
            sector.ceilingshade
        } else {
            sector.floorshade
        };
        let mut sprite_i = lists.headspritesect[si];
        while sprite_i >= 0 {
            let su = sprite_i as usize;
            sprite_i = lists.nextspritesect[su];
            if let Some(sprite) = sprites.get_mut(su)
                && sprite.cstat & 16 != 0
            {
                sprite.shade = shade;
            }
        }
    }
}
