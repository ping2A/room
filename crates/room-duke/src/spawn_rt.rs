//! Chocolate `spawn(j, pn)` for sprites created at run time by another
//! sprite (CON `spawn`, RESPAWN markers): the `j >= 0` header path plus the
//! per-tile cases CON actors actually use (enemies, blood pools, effects,
//! item drops, generic CON actors).

use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::badguy::badguy_pic;
use crate::con::{ActorConTemps, ScriptImage};
use crate::player::{DUKE_GC, TileZMeta};
use crate::shoot::{egs, script_tile_or};
use crate::stat::{STAT_ACTOR, STAT_DEFAULT, STAT_DUMMY, STAT_MISC, STAT_STANDABLE};

const BOSS_NAMES: [&str; 6] = [
    "BOSS1",
    "BOSS2",
    "BOSS3",
    "BOSS4",
    "BOSS1STAYPUT",
    "BOSS4STAYPUT",
];
const TROOP_NAMES: [&str; 8] = [
    "LIZTROOP",
    "LIZTROOPONTOILET",
    "LIZTROOPSHOOT",
    "LIZTROOPJETPACK",
    "LIZTROOPDUCKING",
    "LIZTROOPRUNNING",
    "LIZTROOPSTAYPUT",
    "LIZTROOPJUSTSIT",
];
const ITEM_NAMES: [&str; 29] = [
    "ATOMICHEALTH",
    "STEROIDS",
    "HEATSENSOR",
    "SHIELD",
    "AIRTANK",
    "TRIPBOMBSPRITE",
    "JETPACK",
    "HOLODUKE",
    "FIRSTGUNSPRITE",
    "CHAINGUNSPRITE",
    "SHOTGUNSPRITE",
    "RPGSPRITE",
    "SHRINKERSPRITE",
    "FREEZESPRITE",
    "DEVISTATORSPRITE",
    "SHOTGUNAMMO",
    "FREEZEAMMO",
    "HBOMBAMMO",
    "CRYSTALAMMO",
    "GROWAMMO",
    "BATTERYAMMO",
    "DEVISTATORAMMO",
    "RPGAMMO",
    "BOOTS",
    "AMMO",
    "AMMOLOTS",
    "COLA",
    "FIRSTAID",
    "SIXPAK",
];

fn named(script: &ScriptImage, name: &str) -> i16 {
    script_tile_or(Some(script), name, -1)
}

fn krand(seed: &mut i32) -> i32 {
    room_build_engine::numeric::krand_step(seed)
}

/// Initialise a run-time spawned enemy the way Chocolate `spawn()` does for
/// its badguy cases (size, clip distance, hitscan cstat, stat, fall).
#[allow(clippy::too_many_arguments)]
pub(crate) fn init_spawned_badguy(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    temps: &mut [ActorConTemps],
    tile_meta: &[TileZMeta],
    script: &ScriptImage,
    child: i16,
    parent: i16,
) {
    let cu = child as usize;
    let pn = sprites[cu].picnum;
    let parent_sprite = usize::try_from(parent)
        .ok()
        .and_then(|p| sprites.get(p))
        .copied();
    let from_respawn = parent_sprite.is_some_and(|p| p.picnum == crate::spawn::RESPAWN);
    if from_respawn {
        sprites[cu].pal = parent_sprite.map_or(0, |p| p.pal);
        temps[cu].tempang = i16::from(sprites[cu].pal);
    }
    if sprites[cu].pal == 0 && TROOP_NAMES.iter().any(|n| named(script, n) == pn) {
        sprites[cu].pal = 22;
    }
    let is_boss = BOSS_NAMES.iter().any(|n| named(script, n) == pn);
    if is_boss {
        if sprites[cu].pal != 0 {
            sprites[cu].clipdist = 80;
            sprites[cu].xrepeat = 40;
            sprites[cu].yrepeat = 40;
        } else {
            sprites[cu].xrepeat = 80;
            sprites[cu].yrepeat = 80;
            sprites[cu].clipdist = 164;
        }
    } else if pn == named(script, "SHARK") {
        sprites[cu].xrepeat = 60;
        sprites[cu].yrepeat = 60;
        sprites[cu].clipdist = 40;
    } else if pn == named(script, "RAT") {
        sprites[cu].xrepeat = 48;
        sprites[cu].yrepeat = 48;
        sprites[cu].clipdist = 80;
    } else {
        sprites[cu].xrepeat = 40;
        sprites[cu].yrepeat = 40;
        sprites[cu].clipdist = 80;
    }
    sprites[cu].lotag = 0;
    if pn == named(script, "RAT") {
        sprites[cu].cstat = 0;
    } else {
        sprites[cu].cstat |= 257;
    }
    if pn == named(script, "ORGANTIC") {
        sprites[cu].cstat |= 128;
    }
    if let Some(p) = parent_sprite {
        if !from_respawn {
            sprites[cu].ang = p.ang;
        }
    }
    let actortype = script
        .actortype
        .get(pn.max(0) as usize)
        .copied()
        .unwrap_or(0);
    if actortype & 2 != 0 {
        temps[cu].actorstayput = sprites[cu].sectnum;
    }
    let peers = sprites.to_vec();
    crate::spawn::makeitfall(
        &mut sprites[cu],
        sectors,
        walls,
        &peers,
        tile_meta,
        DUKE_GC,
        false,
    );
    let _ = lists.changespritestat(child, STAT_ACTOR);
}

/// Chocolate `spawn(j, pn)` with `j >= 0`. Returns the new sprite or -1.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn spawn_from(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: &ScriptImage,
    randomseed: &mut i32,
    j: i16,
    pn: i16,
    mut decal_queue: Option<&mut crate::decal_queue::DukeDecalQueue>,
) -> i16 {
    let ju = j as usize;
    if ju >= sprites.len() || pn < 0 {
        return -1;
    }
    let parent = sprites[ju];
    let i = egs(
        lists,
        sprites,
        picnum_out,
        temps,
        Some(script),
        parent.sectnum,
        parent.x,
        parent.y,
        parent.z,
        pn,
        0,
        0,
        0,
        0,
        0,
        0,
        j,
        STAT_DEFAULT,
    );
    if i < 0 {
        return -1;
    }
    let iu = i as usize;
    let parent_stat = lists.sprite_stat.get(ju).copied().unwrap_or(0);
    let has_con = script.actorscrptr.get(pn as usize).copied().unwrap_or(0) > 0;
    let is = |name: &str| named(script, name) == pn;

    // Enemies (explicit badguy cases and CON `actortype & 3`).
    if badguy_pic(pn, script.actortype.as_slice()) && !is("EGG") {
        init_spawned_badguy(
            lists, sprites, sectors, walls, temps, tile_meta, script, i, j,
        );
        return i;
    }
    if is("BLOOD") {
        sprites[iu].xrepeat = 16;
        sprites[iu].yrepeat = 16;
        sprites[iu].z -= 26 << 8;
        if parent.pal == 6 {
            sprites[iu].pal = 6;
        }
        let _ = lists.changespritestat(i, STAT_MISC);
        return i;
    }
    if is("BLOODPOOL") || is("PUKE") {
        let sect = sprites[iu].sectnum;
        let floorz = sectors.get(sect.max(0) as usize).map_or(0, |s| s.floorz);
        let flat_ok = [(108, 108), (-108, -108), (108, -108), (-108, 108)]
            .iter()
            .all(|&(dx, dy)| {
                let mut s1 = sect;
                room_build_engine::geom::updatesector(
                    sprites[iu].x + dx,
                    sprites[iu].y + dy,
                    &mut s1,
                    &sectors.iter().map(SectorT::walls).collect::<Vec<_>>(),
                    &walls.iter().map(WallT::xy).collect::<Vec<_>>(),
                );
                s1 >= 0 && sectors[s1 as usize].floorz == floorz
            });
        if !flat_ok
            || sectors
                .get(sect.max(0) as usize)
                .is_some_and(|s| s.lotag == 1)
        {
            sprites[iu].xrepeat = 0;
            sprites[iu].yrepeat = 0;
            let _ = lists.changespritestat(i, STAT_MISC);
            if is("BLOODPOOL")
                && let Some(queue) = decal_queue.as_deref_mut()
            {
                queue.insert(lists, sprites, i);
            }
            return i;
        }
        if !is("PUKE") {
            sprites[iu].pal = if parent.pal == 1 {
                1
            } else if parent.pal != 6
                && parent.picnum != named(script, "NUKEBARREL")
                && parent.picnum != named(script, "TIRE")
            {
                if parent.picnum == named(script, "FECES") {
                    7
                } else {
                    2
                }
            } else {
                0
            };
            if parent.picnum == named(script, "TIRE") {
                sprites[iu].shade = 127;
            }
        }
        // Room advances pools in `moveexplosions` (STAT_MISC), growing
        // from Chocolate's 1×1 spawn size.
        sprites[iu].cstat |= 32;
        sprites[iu].xrepeat = 1;
        sprites[iu].yrepeat = 1;
        let _ = lists.changespritestat(i, STAT_MISC);
        if is("BLOODPOOL")
            && let Some(queue) = decal_queue.as_deref_mut()
        {
            queue.insert(lists, sprites, i);
        }
        return i;
    }
    if is("FECES") {
        sprites[iu].cstat |= 257;
        sprites[iu].clipdist = 128;
        let _ = lists.changespritestat(i, STAT_ACTOR);
        return i;
    }
    if is("RESPAWNMARKERRED") {
        sprites[iu].xrepeat = 24;
        sprites[iu].yrepeat = 24;
        sprites[iu].z = temps.get(ju).map_or(parent.z, |t| t.hit_floorz);
        let _ = lists.changespritestat(i, STAT_ACTOR);
        return i;
    }
    if is("FRAMEEFFECT1") {
        sprites[iu].xrepeat = parent.xrepeat;
        sprites[iu].yrepeat = parent.yrepeat;
        temps[iu].g_t[1] = i32::from(parent.picnum);
        let _ = lists.changespritestat(i, STAT_MISC);
        return i;
    }
    if is("TRANSPORTERSTAR") {
        if parent_stat == crate::stat::STAT_PROJECTILE {
            sprites[iu].xrepeat = 8;
            sprites[iu].yrepeat = 8;
        } else {
            sprites[iu].xrepeat = 48;
            sprites[iu].yrepeat = 64;
            if parent_stat == crate::stat::STAT_PLAYER
                || badguy_pic(parent.picnum, script.actortype.as_slice())
            {
                sprites[iu].z -= 32 << 8;
            }
        }
        sprites[iu].shade = -127;
        sprites[iu].cstat = 128 | 2;
        sprites[iu].ang = parent.ang;
        sprites[iu].xvel = 128;
        let _ = lists.changespritestat(i, STAT_MISC);
        return i;
    }
    if is("WATERBUBBLE") {
        if parent.picnum == 1405 {
            sprites[iu].z -= 16 << 8;
        }
        sprites[iu].ang = parent.ang;
        sprites[iu].xrepeat = 4;
        sprites[iu].yrepeat = 4;
        let _ = lists.changespritestat(i, STAT_MISC);
        return i;
    }
    if is("WATERDRIP") {
        if parent_stat == crate::stat::STAT_PLAYER || parent_stat == STAT_ACTOR {
            sprites[iu].shade = 32;
            if parent.pal != 1 {
                sprites[iu].pal = 2;
                sprites[iu].z -= 18 << 8;
            } else {
                sprites[iu].z -= 13 << 8;
            }
            sprites[iu].xvel = (48 - (krand(randomseed) & 31)) as i16;
        }
        sprites[iu].xrepeat = 24;
        sprites[iu].yrepeat = 24;
        let _ = lists.changespritestat(i, STAT_STANDABLE);
        return i;
    }
    let explosion_like = [
        "EXPLOSION2",
        "EXPLOSION2BOT",
        "BURNING",
        "BURNING2",
        "SMALLSMOKE",
        "SHRINKEREXPLOSION",
        "COOLEXPLOSION1",
    ];
    if explosion_like.iter().any(|n| is(n)) {
        sprites[iu].ang = parent.ang;
        sprites[iu].shade = -64;
        sprites[iu].cstat = 128 | (krand(randomseed) & 4) as u16;
        if is("EXPLOSION2") || is("EXPLOSION2BOT") {
            sprites[iu].xrepeat = 48;
            sprites[iu].yrepeat = 48;
            sprites[iu].shade = -127;
            sprites[iu].cstat |= 128;
        } else if is("SHRINKEREXPLOSION") {
            sprites[iu].xrepeat = 32;
            sprites[iu].yrepeat = 32;
        } else if is("SMALLSMOKE") {
            sprites[iu].xrepeat = 24;
            sprites[iu].yrepeat = 24;
        } else if is("BURNING") || is("BURNING2") {
            sprites[iu].xrepeat = 4;
            sprites[iu].yrepeat = 4;
        }
        let sec_z: Vec<_> = sectors.iter().map(SectorT::z).collect();
        let wxy: Vec<_> = walls.iter().map(WallT::xy).collect();
        let floorz = room_build_engine::geom::getzsofslope(
            sprites[iu].sectnum,
            sprites[iu].x,
            sprites[iu].y,
            &sec_z,
            &wxy,
        )
        .1;
        if sprites[iu].z > floorz - (12 << 8) {
            sprites[iu].z = floorz - (12 << 8);
        }
        let _ = lists.changespritestat(i, STAT_MISC);
        return i;
    }
    if ITEM_NAMES.iter().any(|n| is(n)) {
        sprites[iu].lotag = 0;
        sprites[iu].z -= 32 << 8;
        sprites[iu].zvel = -1024;
        sprites[iu].cstat = (krand(randomseed) & 4) as u16;
        sprites[iu].pal = 0;
        if is("ATOMICHEALTH") {
            sprites[iu].cstat |= 128;
        }
        let size = if is("AMMO") { 16 } else { 32 };
        sprites[iu].xrepeat = size;
        sprites[iu].yrepeat = size;
        sprites[iu].shade = -17;
        let _ = lists.changespritestat(i, STAT_ACTOR);
        return i;
    }
    // Generic CON actor (`default:` with `actorscrptr`).
    if has_con {
        if sprites[iu].xrepeat == 0 || sprites[iu].yrepeat == 0 {
            sprites[iu].xrepeat = 1;
            sprites[iu].yrepeat = 1;
        }
        sprites[iu].clipdist = 40;
        sprites[iu].owner = i;
        sprites[iu].ang = parent.ang;
        let _ = lists.changespritestat(i, STAT_ACTOR);
        return i;
    }
    // Anything else: keep the map classification of the tile.
    let stat = crate::spawn::classify_stat(&sprites[iu], &script.actorscrptr, &script.actortype);
    if stat == STAT_DUMMY {
        let _ = lists.changespritestat(i, STAT_ACTOR);
    } else if stat != STAT_DEFAULT {
        let _ = lists.changespritestat(i, stat);
    }
    i
}
