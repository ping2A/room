//! Chocolate `movedummyplayers` (statnum 13): the `PLAYERONWATER` sprite that
//! stands in for a player wading in shallow water.

use room_build_engine::geom::{SectorWalls, WallXY, updatesector};
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::con::ActorConTemps;
use crate::player::Player;
use crate::stat::STAT_DUMMYPLAYER;

/// Run once per tic right after `global_random = TRAND`, before
/// `processinput` (Chocolate `domovethings`).
///
/// For each dummy: the owning player is `sprite[OW].yvel`. The dummy is
/// deleted when the player is dead or no longer in a `lotag 1` sector;
/// otherwise it shows at the water ceiling while the player stands on a
/// warping sector, or hides on the floor. Either way it follows the player's
/// last displacement (`posx - oposx`).
pub fn movedummyplayers(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    players: &mut [Player],
    actor_temps: &mut Vec<ActorConTemps>,
) {
    let mut i = lists.headspritestat[STAT_DUMMYPLAYER as usize];
    if i < 0 {
        return;
    }
    let sec_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    while i >= 0 {
        let nexti = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu >= sprites.len() {
            i = nexti;
            continue;
        }
        let owner = sprites[iu].owner;
        let p = usize::try_from(owner)
            .ok()
            .and_then(|o| sprites.get(o))
            .map_or(0, |o| o.yvel.max(0) as usize);
        let Some(player) = players.get_mut(p) else {
            i = nexti;
            continue;
        };
        let player_lotag = usize::try_from(player.cursectnum)
            .ok()
            .and_then(|s| sectors.get(s))
            .map_or(0, |s| s.lotag);
        let body_dead = usize::try_from(player.sprite_i)
            .ok()
            .and_then(|b| sprites.get(b))
            .is_none_or(|b| b.extra <= 0);
        if player_lotag != 1 || body_dead {
            // `ps[p].dummyplayersprite = -1; KILLIT(i);`
            player.dummy_player_sprite = -1;
            let _ = lists.deletesprite(i);
            i = nexti;
            continue;
        }
        let sect = sprites[iu].sectnum;
        let sector = usize::try_from(sect).ok().and_then(|s| sectors.get(s));
        if player.on_ground && player.on_warping_sector == 1 && player_lotag == 1 {
            sprites[iu].cstat = 257;
            if let Some(sector) = sector {
                sprites[iu].z = sector.ceilingz + (27 << 8);
            }
            sprites[iu].ang = player.ang;
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            let t0 = &mut actor_temps[iu].g_t[0];
            *t0 = if *t0 == 8 { 0 } else { *t0 + 1 };
        } else {
            if let Some(sector) = sector
                && sector.lotag != 2
            {
                sprites[iu].z = sector.floorz;
            }
            sprites[iu].cstat = 32768;
        }
        sprites[iu].x += player.posx - player.oposx;
        sprites[iu].y += player.posy - player.oposy;
        // `setsprite(i,SX,SY,SZ)`: keep the sector in step with the position.
        let mut new_sect = sprites[iu].sectnum;
        updatesector(
            sprites[iu].x,
            sprites[iu].y,
            &mut new_sect,
            &sec_walls,
            &wxy,
        );
        if new_sect >= 0 && new_sect != sprites[iu].sectnum {
            let _ = lists.changespritesect(i, new_sect);
            sprites[iu].sectnum = new_sect;
        }
        i = nexti;
    }
}
