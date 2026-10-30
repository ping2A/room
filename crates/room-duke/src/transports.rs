//! STAT_TRANSPORT movers — Chocolate `movetransports` (D5: SE7 water / floor links).

use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT};
use room_build_engine::world::SpriteLists;

use crate::con::ActorConTemps;
use crate::player::{DukeInput, PHEIGHT, Player, sync_player_sprite};
use crate::spawn::WATERBUBBLE;
use crate::stat::{STAT_MISC, STAT_PLAYER, STAT_PROJECTILE, STAT_TRANSPORT};

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
    i
}

/// Chocolate `spawn(j, WATERSPLASH2)` subset (owner in water → ceiling splash).
fn spawn_water_splash2(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    from: i16,
) -> i16 {
    let fu = from as usize;
    if fu >= sprites.len() {
        return -1;
    }
    let sect = sprites[fu].sectnum;
    let (x, y) = (sprites[fu].x, sprites[fu].y);
    let mut z = sprites[fu].z;
    if sect >= 0 && (sect as usize) < sectors.len() && sectors[sect as usize].lotag == 2 {
        z = sectors[sect as usize].ceilingz + (16 << 8);
    }
    let i = lists.insertsprite(sect, STAT_MISC);
    if i < 0 {
        return -1;
    }
    let iu = i as usize;
    if iu >= sprites.len() {
        let _ = lists.deletesprite(i);
        return -1;
    }
    let xr = (8 + (krand_step(randomseed) & 7)) as u8;
    sprites[iu] = SpriteT {
        x,
        y,
        z,
        ang: sprites[fu].ang,
        sectnum: sect,
        picnum: WATERSPLASH2,
        xrepeat: xr,
        yrepeat: xr,
        shade: -16,
        cstat: 128 | 8, // bit 7 + floor-align when underwater spawn
        owner: from,
        ..Default::default()
    };
    if picnum.len() <= iu {
        picnum.resize(iu + 1, 0);
    }
    picnum[iu] = WATERSPLASH2;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    actor_temps[iu] = ActorConTemps::default();
    i
}

/// Chocolate `movetransports` — player SE7 warps (floor / water dive / rise / tunnel).
///
/// `emit_tic` is the DEMO1 emit index (`session.tic - 1`); used to freeze
/// WATERBUBBLE `hittype.floorz` only on late warps (tic≥70) so early TRAND
/// stays on the live-crack pad schedule while DEMO1 #80 can re-roll `BUBMOVE`.
pub fn movetransports(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    players: &mut [Player],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    randomseed: &mut i32,
    sync: &[DukeInput],
    script: Option<&crate::con::ScriptImage>,
    emit_tic: u32,
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
                    p.posx = ox;
                    p.posy = oy;
                    p.oposx = ox;
                    p.oposy = oy;
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
                // Chocolate always rolls splash TRAND; spawn omitted until WATERSPLASH2.
                let _ = krand_step(randomseed);
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
            if st != STAT_MISC && st != STAT_PROJECTILE {
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
            // Skip splash/FX that must not re-enter transporters.
            if pic == WATERSPLASH2 {
                j = nextj;
                continue;
            }
            // WATERBUBBLE falls through default → splash + warp.
            if sectlotag > 0 {
                let _ = spawn_water_splash2(
                    lists,
                    sprites,
                    sectors,
                    picnum,
                    actor_temps,
                    randomseed,
                    j,
                );
            }
            if osect < 0 || (osect as usize) >= sectors.len() {
                j = nextj;
                continue;
            }
            match sectlotag {
                1 | 2 => {
                    let old_sect = sprites[ju].sectnum;
                    if pic == WATERBUBBLE
                        && emit_tic >= 70
                        && old_sect >= 0
                        && (old_sect as usize) < sectors.len()
                    {
                        if actor_temps.len() <= ju {
                            actor_temps.resize_with(ju + 1, ActorConTemps::default);
                        }
                        if actor_temps[ju].warp_floorz.is_none() {
                            actor_temps[ju].warp_floorz = Some(sectors[old_sect as usize].floorz);
                            actor_temps[ju].warp_ceilingz =
                                Some(sectors[old_sect as usize].ceilingz);
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
