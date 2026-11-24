//! STAT_TRANSPORT movers — Chocolate `movetransports` (D5: SE7 water / floor links).

use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::con::ActorConTemps;
use crate::player::{DukeInput, PHEIGHT, Player, sync_player_sprite};
use crate::spawn::WATERBUBBLE;
use crate::stat::{
    STAT_ACTOR, STAT_DUMMYPLAYER, STAT_FALLER, STAT_MISC, STAT_PLAYER, STAT_PROJECTILE,
    STAT_TRANSPORT,
};

// Atomic tiles named in Chocolate `movetransports`.
const TRANSPORTERSTAR: i16 = 1630;
const TRANSPORTERBEAM: i16 = 1261;
const TRIPBOMB: i16 = 2566;
const BULLETHOLE: i16 = 952;
const BURNING: i16 = 2270;
const BURNING2: i16 = 2310;
const FIRE: i16 = 2271;
const FIRE2: i16 = 2311;
const TOILETWATER: i16 = 921;
const LASERLINE: i16 = 2567;
const PLAYERONWATER: i16 = 1420;
const SHARK: i16 = 1550;
const COMMANDER: i16 = 1920;
const OCTABRAIN: i16 = 1820;

/// Atomic `WATERSPLASH2`.
const WATERSPLASH2: i16 = 1380;

fn klabs(x: i32) -> i32 {
    if x == i32::MIN { i32::MIN } else { x.abs() }
}

/// Minimal Chocolate `spawn(j, WATERBUBBLE)` + `z += TRAND&16383`.
fn spawn_water_bubble(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<crate::con::ActorConTemps>,
    randomseed: &mut i32,
    from: i16,
    script: Option<&crate::con::ScriptImage>,
) -> i16 {
    let fu = from as usize;
    if fu >= sprites.len() {
        return -1;
    }
    let sect = sprites[fu].sectnum;
    let ang = sprites[fu].ang;
    let (x, y, z) = (sprites[fu].x, sprites[fu].y, sprites[fu].z - (16 << 8));
    let i = lists.insertsprite(sect, STAT_MISC);
    if i < 0 {
        return -1;
    }
    let iu = i as usize;
    if iu >= sprites.len() {
        let _ = lists.deletesprite(i);
        return -1;
    }
    sprites[iu] = SpriteT {
        x,
        y,
        z,
        ang,
        sectnum: sect,
        picnum: WATERBUBBLE,
        xrepeat: 4,
        yrepeat: 4,
        owner: from,
        ..Default::default()
    };
    sprites[iu].z += krand_step(randomseed) & 16383;
    if picnum.len() <= iu {
        picnum.resize(iu + 1, 0);
    }
    picnum[iu] = WATERBUBBLE;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    // Chocolate EGS: zero temps then fill action/move/hitag from `actorscrptr`.
    if let Some(img) = script {
        let (extra, temps) = crate::con::init_actor_temps_from_script(img, WATERBUBBLE);
        sprites[iu].extra = extra;
        sprites[iu].hitag = crate::con::actor_header_hitag(img, WATERBUBBLE) as i16;
        actor_temps[iu] = temps;
    } else {
        actor_temps[iu] = ActorConTemps::default();
    }
    crate::con::vm::inherit_hit_planes(actor_temps, iu, from);
    i
}

/// Atomic `FLOORSLIME` (a slime floor or ceiling tints the splash, pal 7).
const FLOORSLIME: i16 = 200;

/// Chocolate `spawn(j, WATERSPLASH2)`: `EGS(sprite[j]…, pn, 0,0,0,0,0,0, j, 0)`,
/// then `xrepeat = yrepeat = 8+(TRAND&7)`, shade -16, cstat 128, and the
/// splash sits on the water ceiling (`lotag 2`, floor-aligned) or floor
/// (`lotag 1`) of the spawner's sector; slime tints it; STAT_MISC.
#[allow(clippy::too_many_arguments)]
fn spawn_water_splash2(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    from: i16,
    script: Option<&crate::con::ScriptImage>,
) -> i16 {
    let fu = from as usize;
    if fu >= sprites.len() {
        return -1;
    }
    let src = sprites[fu];
    let i = crate::shoot::egs(
        lists,
        sprites,
        picnum,
        actor_temps,
        script,
        src.sectnum,
        src.x,
        src.y,
        src.z,
        WATERSPLASH2,
        0,
        0,
        0,
        0,
        0,
        0,
        from,
        STAT_MISC,
    );
    if i < 0 {
        return -1;
    }
    let iu = i as usize;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    // `hittype[i].picnum = sprite[j].picnum`.
    actor_temps[iu].hit_picnum = src.picnum;
    // `setsprite(i, sprite[j].x, sprite[j].y, sprite[j].z)`: the splash
    // re-finds its sector from the spawner's (possibly stale) one, so a
    // splash over a non-water sector is killed by its first tic.
    {
        let sec_walls: Vec<room_build_engine::geom::SectorWalls> = sectors
            .iter()
            .map(|s| room_build_engine::geom::SectorWalls {
                wallptr: s.wallptr,
                wallnum: s.wallnum,
            })
            .collect();
        let wxy: Vec<room_build_engine::geom::WallXY> = walls.iter().map(WallT::xy).collect();
        let mut sect = sprites[iu].sectnum;
        room_build_engine::geom::updatesector(src.x, src.y, &mut sect, &sec_walls, &wxy);
        if sect >= 0 && sect != sprites[iu].sectnum {
            let _ = lists.changespritesect(i, sect);
            sprites[iu].sectnum = sect;
        }
    }
    let size = (8 + (krand_step(randomseed) & 7)) as u8;
    let sprite = &mut sprites[iu];
    sprite.xrepeat = size;
    sprite.yrepeat = size;
    sprite.shade = -16;
    sprite.cstat |= 128;
    if let Some(sector) = usize::try_from(src.sectnum)
        .ok()
        .and_then(|s| sectors.get(s))
    {
        let sec_z: Vec<room_build_engine::geom::SectorZ> = sectors.iter().map(SectorT::z).collect();
        let wxy: Vec<room_build_engine::geom::WallXY> = walls.iter().map(WallT::xy).collect();
        let (ceilz, florz) =
            room_build_engine::geom::getzsofslope(sprite.sectnum, sprite.x, sprite.y, &sec_z, &wxy);
        if sector.lotag == 2 {
            sprite.z = ceilz + (16 << 8);
            sprite.cstat |= 8;
        } else if sector.lotag == 1 {
            sprite.z = florz;
        }
    }
    // Chocolate's slime check reads `sect`, the sector captured before
    // `setsprite` (the spawner's).
    if let Some(own) = usize::try_from(src.sectnum)
        .ok()
        .and_then(|s| sectors.get(s))
    {
        if own.floorpicnum == FLOORSLIME || own.ceilingpicnum == FLOORSLIME {
            sprite.pal = 7;
        }
    }
    i
}

/// Chocolate `movetransports` — player SE7 warps (floor / water dive / rise / tunnel).
///
/// `emit_tic` is the DEMO1 emit index (`session.tic - 1`); retained for the
/// assisted trace path's late-warp state capture.
#[allow(clippy::too_many_arguments)]
pub fn movetransports(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    players: &mut [Player],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    randomseed: &mut i32,
    sync: &[DukeInput],
    script: Option<&crate::con::ScriptImage>,
    _emit_tic: u32,
) {
    let mut i = lists.headspritestat[STAT_TRANSPORT as usize];
    while i >= 0 {
        let nexti = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu >= sprites.len() {
            i = nexti;
            continue;
        }
        let sect = sprites[iu].sectnum;
        if sect < 0 || (sect as usize) >= sectors.len() {
            i = nexti;
            continue;
        }
        let sectlotag = sectors[sect as usize].lotag;
        let owner = sprites[iu].owner;
        if owner == i || owner < 0 || (owner as usize) >= sprites.len() {
            i = nexti;
            continue;
        }
        if iu >= actor_temps.len() {
            i = nexti;
            continue;
        }
        if actor_temps[iu].g_t[0] > 0 {
            actor_temps[iu].g_t[0] -= 1;
        }
        let onfloorz = actor_temps[iu].g_t[5] != 0;
        let sx = sprites[iu].x;
        let sy = sprites[iu].y;
        let sz = sprites[iu].z;
        let ow = owner as usize;
        let ox = sprites[ow].x;
        let oy = sprites[ow].y;
        let oz = sprites[ow].z;
        let osect = sprites[ow].sectnum;
        let ow_owner = sprites[ow].owner;
        let ow_ang = sprites[ow].ang;

        let mut j = lists.headspritesect[sect as usize];
        while j >= 0 {
            let nextj = lists.nextspritesect[j as usize];
            let ju = j as usize;
            if ju >= sprites.len() {
                j = nextj;
                continue;
            }
            if lists.sprite_stat[ju] != STAT_PLAYER {
                j = nextj;
                continue;
            }
            if sprites[ju].owner < 0 {
                j = nextj;
                continue;
            }
            let pidx = sprites[ju].yvel as usize;
            if pidx >= players.len() {
                j = nextj;
                continue;
            }
            let bits = sync.get(pidx).map(|s| s.bits).unwrap_or(0);
            let p = &mut players[pidx];
            p.on_warping_sector = 1;

            if p.transporter_hold == 0 && p.jumping_counter == 0 {
                // Floor teleporter (sectlotag 0 + onfloorz).
                if p.on_ground && sectlotag == 0 && onfloorz && p.jetpack_on == 0 {
                    p.ang = ow_ang;
                    if ow_owner != owner {
                        actor_temps[iu].g_t[0] = 13;
                        if ow < actor_temps.len() {
                            actor_temps[ow].g_t[0] = 13;
                        }
                        p.transporter_hold = 13;
                    }
                    // `ps[p].bobposx = ps[p].oposx = ps[p].posx = sprite[OW].x`:
                    // the bob origin moves too, so the next tic's body `xvel`
                    // (and `bobcounter`) do not see the teleport distance.
                    p.posx = ox;
                    p.posy = oy;
                    p.oposx = ox;
                    p.oposy = oy;
                    p.bobposx = ox;
                    p.bobposy = oy;
                    p.posz = oz - PHEIGHT;
                    p.oposz = p.posz;
                    if osect >= 0 {
                        let _ = lists.changespritesect(j, osect);
                        p.cursectnum = osect;
                        sprites[ju].sectnum = osect;
                    }
                    sync_player_sprite(p, sprites);
                    break;
                }
            } else if !(sectlotag == 1 && p.on_ground) {
                break;
            }

            // Off-floor water tunnel.
            if !onfloorz && klabs(sz - p.posz) < 6144 {
                let jet_ok = p.jetpack_on == 0 || (bits & 1) != 0 || (bits & 2) != 0;
                if jet_ok {
                    p.posx += ox - sx;
                    p.posy += oy - sy;
                    p.oposx = p.posx;
                    p.oposy = p.posy;
                    if p.jetpack_on != 0 && ((bits & 1) != 0 || p.jetpack_on < 11) {
                        p.posz = oz - 6144;
                    } else {
                        p.posz = oz + 6144;
                    }
                    p.oposz = p.posz;
                    if osect >= 0 {
                        let _ = lists.changespritesect(j, osect);
                        p.cursectnum = osect;
                        sprites[ju].sectnum = osect;
                    }
                    sync_player_sprite(p, sprites);
                    break;
                }
            }

            let mut k = 0_i32;

            // Dive: surface water (lotag 1) → underwater.
            if onfloorz
                && sectlotag == 1
                && p.on_ground
                && p.posz > sectors[sect as usize].floorz - (16 << 8)
                && ((bits & 2) != 0 || p.poszv > 2048)
            {
                k = 1;
                if osect >= 0 && (osect as usize) < sectors.len() {
                    p.posz = sectors[osect as usize].ceilingz + (7 << 8);
                }
                p.oposz = p.posz;
                p.posxv = 4096 - (krand_step(randomseed) & 8192);
                p.posyv = 4096 - (krand_step(randomseed) & 8192);
            }

            // Rise: underwater (lotag 2) → surface.
            if onfloorz && sectlotag == 2 && p.posz < sectors[sect as usize].ceilingz + (6 << 8) {
                k = 1;
                if osect >= 0 && (osect as usize) < sectors.len() {
                    p.posz = sectors[osect as usize].floorz - (7 << 8);
                }
                p.oposz = p.posz;
                p.jumping_toggle = 1;
                p.jumping_counter = 0;
            }

            if k == 1 {
                p.posx += ox - sx;
                p.posy += oy - sy;
                p.oposx = p.posx;
                p.oposy = p.posy;
                if ow_owner != owner {
                    p.transporter_hold = -2;
                }
                if osect >= 0 {
                    let _ = lists.changespritesect(j, osect);
                    p.cursectnum = osect;
                    sprites[ju].sectnum = osect;
                }
                sync_player_sprite(p, sprites);
                // EDuke32 `P_FinishWaterChange` → `P_UpdateScreenPal`: an
                // explicit CON game palette yields to the new water state.
                p.game_palette = 0;
                // Chocolate `if( (TRAND&255) < 32 ) spawn(j,WATERSPLASH2);`
                if krand_step(randomseed) & 255 < 32 {
                    let _ = spawn_water_splash2(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        picnum,
                        actor_temps,
                        randomseed,
                        j,
                        script,
                    );
                }
                if sectlotag == 1 {
                    let body = p.sprite_i;
                    for _ in 0..9 {
                        let _ = spawn_water_bubble(
                            lists,
                            sprites,
                            picnum,
                            actor_temps,
                            randomseed,
                            body,
                            script,
                        );
                    }
                }
                break;
            }

            j = nextj;
        }

        // Chocolate: also warp projectiles / MISC (WATERBUBBLE rise → splash).
        let mut j = lists.headspritesect[sect as usize];
        while j >= 0 {
            let nextj = lists.nextspritesect[j as usize];
            let ju = j as usize;
            if ju >= sprites.len() {
                j = nextj;
                continue;
            }
            let st = lists.sprite_stat[ju];
            // Chocolate handles statnum 1 (actors; live water monsters stay),
            // 4, 5, 12 and 13 here.
            if !matches!(
                st,
                STAT_ACTOR | STAT_PROJECTILE | STAT_MISC | STAT_FALLER | STAT_DUMMYPLAYER
            ) {
                j = nextj;
                continue;
            }
            if st == STAT_ACTOR
                && matches!(
                    sprites[ju].picnum,
                    SHARK | COMMANDER | OCTABRAIN | 2370..=2377
                )
                && sprites[ju].extra > 0
            {
                j = nextj;
                continue;
            }
            let ll = klabs(i32::from(sprites[ju].zvel));
            let mut warpspriteto = false;
            if ll != 0 && sectlotag == 2 && sprites[ju].z < sectors[sect as usize].ceilingz + ll {
                warpspriteto = true;
            }
            if ll != 0 && sectlotag == 1 && sprites[ju].z > sectors[sect as usize].floorz - ll {
                warpspriteto = true;
            }
            if !warpspriteto {
                j = nextj;
                continue;
            }
            let pic = sprites[ju].picnum;
            // Effects that never cross a transporter (`goto JBOLT`).
            if matches!(
                pic,
                TRANSPORTERSTAR
                    | TRANSPORTERBEAM
                    | TRIPBOMB
                    | BULLETHOLE
                    | WATERSPLASH2
                    | BURNING
                    | BURNING2
                    | FIRE
                    | FIRE2
                    | TOILETWATER
                    | LASERLINE
            ) {
                j = nextj;
                continue;
            }
            // `case PLAYERONWATER: if(sectlotag == 2) { cstat &= 32767; break; }`
            if pic == PLAYERONWATER && sectlotag == 2 {
                sprites[ju].cstat &= 32767;
                j = nextj;
                continue;
            }
            // `default: if(statnum == 5 && !(sectlotag == 1 || sectlotag == 2)) break;`
            // (WATERBUBBLE skips this test.)
            if pic != WATERBUBBLE && st == STAT_MISC && !(sectlotag == 1 || sectlotag == 2) {
                j = nextj;
                continue;
            }
            if sectlotag > 0 {
                let _ = spawn_water_splash2(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum,
                    actor_temps,
                    randomseed,
                    j,
                    script,
                );
            }
            if osect < 0 || (osect as usize) >= sectors.len() {
                j = nextj;
                continue;
            }
            match sectlotag {
                1 | 2 => {
                    let old_sect = sprites[ju].sectnum;
                    if pic == WATERBUBBLE && old_sect >= 0 && (old_sect as usize) < sectors.len() {
                        if actor_temps.len() <= ju {
                            actor_temps.resize_with(ju + 1, ActorConTemps::default);
                        }
                        if actor_temps[ju].warp_floorz.is_none() {
                            // Preserve the source hit-plane cache across a warp.
                            // Water bubbles initialize this cache at spawn, matching
                            // their stale `hittype.floorz`/`ceilingz` in Chocolate.
                            // Chocolate never refreshes a STAT_MISC sprite's
                            // `hittype.floorz`; the pre-warp plane is the
                            // closest stand-in when no spawn value was cached.
                            let cache_sect = osect as usize;
                            actor_temps[ju].warp_floorz = Some(sectors[cache_sect].floorz);
                            actor_temps[ju].warp_ceilingz = Some(sectors[cache_sect].ceilingz);
                        }
                    }
                    sprites[ju].x += ox - sx;
                    sprites[ju].y += oy - sy;
                    if sectlotag == 1 {
                        sprites[ju].z = sectors[osect as usize].ceilingz + ll;
                    } else {
                        sprites[ju].z = sectors[osect as usize].floorz - ll;
                    }
                    let _ = lists.changespritesect(j, osect);
                    sprites[ju].sectnum = osect;
                }
                _ => {}
            }
            j = nextj;
        }
        i = nexti;
    }
}
