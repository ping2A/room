//! Chocolate `moveweapons` (STAT_PROJECTILE) — SHOTSPARK1 CON + COOLEXPLOSION1 fly (D6).

use room_build_engine::geom::{SectorZ, WallXY};
use room_build_engine::helpers::sincos_q14;
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::con::{ActorConTemps, ConSpriteEnv, ConStop, ScriptImage, execute_actor};
use crate::fta::findplayer_dist;
use crate::player::Player;
use crate::shoot::{COOLEXPLOSION1, SHOTSPARK1};
use crate::spawn::WATERBUBBLE;
use crate::standables::spawn_maker_bubble;
use crate::stat::STAT_PROJECTILE;

/// Atomic `TICSPERFRAME` at 30 Hz.
const TICSPERFRAME: i32 = 4;

/// Chocolate `moveweapons` subset: run CON on `SHOTSPARK1`.
pub fn moveweapons(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    players: &[Player],
    randomseed: &mut i32,
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&ScriptImage>,
) {
    let Some(script) = script else {
        return;
    };
    let mut order: Vec<i16> = Vec::new();
    let mut i = lists.headspritestat[STAT_PROJECTILE as usize];
    while i >= 0 {
        order.push(i);
        i = lists.nextspritestat[i as usize];
    }
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();

    for &i in &order {
        if lists.sprite_stat.get(i as usize).copied().unwrap_or(1024) != STAT_PROJECTILE {
            continue;
        }
        let iu = i as usize;
        if iu >= sprites.len() {
            continue;
        }
        if sprites[iu].sectnum < 0 {
            let _ = lists.deletesprite(i);
            continue;
        }
        // Chocolate `movesprite` for COOLEXPLOSION1: world Δ = (step*TIC)>>3
        // with step = (xvel*sintable)>>14 (CLIPMASK1 path; open flight first).
        if sprites[iu].picnum == COOLEXPLOSION1 {
            let xv = i32::from(sprites[iu].xvel);
            let zv = i32::from(sprites[iu].zvel);
            let (sin, cos) = sincos_q14(i32::from(sprites[iu].ang) & 2047);
            let step_x = (xv * cos) >> 14;
            let step_y = (xv * sin) >> 14;
            sprites[iu].x += (step_x * TICSPERFRAME) >> 3;
            sprites[iu].y += (step_y * TICSPERFRAME) >> 3;
            sprites[iu].z += (zv * TICSPERFRAME) >> 3;
            continue;
        }
        if sprites[iu].picnum != SHOTSPARK1 {
            continue;
        }
        let pn = SHOTSPARK1.max(0) as usize;
        if pn >= script.actorscrptr.len() || script.actorscrptr[pn] <= 0 {
            continue;
        }
        let entry = (script.actorscrptr[pn] as usize).saturating_add(4);
        if actor_temps.len() <= iu {
            actor_temps.resize_with(iu + 1, ActorConTemps::default);
        }
        actor_temps[iu].pending_spawns.clear();
        let (pidx, dist) = findplayer_dist(&sprites[iu], players);
        let mut env = ConSpriteEnv {
            player_dist: dist,
            ..ConSpriteEnv::default()
        };
        if let Some(p) = players.get(pidx) {
            env.player_x = p.posx;
            env.player_y = p.posy;
            env.player_z = p.posz;
            env.player_curr_weapon = p.curr_weapon;
            env.player_ammo = p.ammo;
            env.player_gotweapon = p.gotweapon;
        }
        let sect = sprites[iu].sectnum;
        let sec = if sect >= 0 && (sect as usize) < sectors.len() {
            sectors[sect as usize]
        } else {
            SectorT::default()
        };
        let stop = execute_actor(
            script,
            entry,
            randomseed,
            &mut actor_temps[iu],
            &mut sprites[iu],
            i as i16,
            &sec,
            sec.floorz,
            sec.ceilingz,
            env,
            &sec_z,
            &wxy,
            &[],
        );
        // CON `spawn WATERBUBBLE` (ifactioncount 3 + ifinwater on SHOTSPARK1).
        let spawns: Vec<i16> = actor_temps[iu].pending_spawns.drain(..).collect();
        for pn in spawns {
            if pn == WATERBUBBLE {
                let _ = spawn_maker_bubble(lists, sprites, picnum, actor_temps, i, Some(script));
            }
        }
        if stop == ConStop::KillIt {
            let _ = lists.deletesprite(i);
        }
    }
}
