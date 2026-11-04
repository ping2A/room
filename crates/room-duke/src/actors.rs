//! Actor movers — Chocolate `moveactors` / CON `execute` (D5).

use room_build_engine::geom::{
    SectorWalls, SectorZ, SpriteHit, SpriteZClip, WallXY, clipmove_xy_at_z_sprites, getzsofslope,
};
use room_build_engine::helpers::{getangle, sincos_q14};
use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::{MAXSPRITES, SpriteLists};

use crate::badguy::badguy_pic;
use crate::con::{ActorConTemps, ConSpriteEnv, ConStop, ScriptImage, execute_actor};
use crate::fta::findplayer_dist;
use crate::player::{DUKE_GC, Player, TileZMeta, sprite_z_clip};
use crate::shoot::{
    BLOOD, BLOODSPLAT1, BLOODSPLAT2, BLOODSPLAT3, BLOODSPLAT4, COOLEXPLOSION1, con_spawn_blood,
    shoot_bloodsplat_enemy, shoot_coolexplosion_enemy,
};
use crate::spawn::{FIRE, FIRE2, con_fall};
use crate::stat::STAT_ACTOR;

/// Atomic `TICSPERFRAME` at 30 Hz.
const TICSPERFRAME: i32 = 4;
/// Atomic `define faceplayer 1`.
const MOVE_FACEPLAYER: i16 = 1;
/// Atomic `define seekplayer 512`.
const MOVE_SEEKPLAYER: i16 = 512;
const MOVE_GETH: i16 = 2;
const MOVE_GETV: i16 = 4;
/// Atomic `define faceplayerslow 16`.
const MOVE_FACEPLAYERSLOW: i16 = 16;
/// Atomic `ORGANTIC` — exempt from move() floor/ceiling z clamps.
const ORGANTIC: i16 = 2420;
const LIZTROOP: i16 = 1680;
const LIZMAN: i16 = 2120;
const BOSS1: i16 = 2630;
const BOSS2: i16 = 2710;
const COMMANDER: i16 = 1920;

fn getincangle(a: i32, goal: i32) -> i32 {
    (((goal - a) + 1024) & 2047) - 1024
}

/// Chocolate `FindDistance2D` (Build approx, not `ksqrt`).
fn find_distance_2d(mut ix: i32, mut iy: i32) -> i32 {
    ix = ix.saturating_abs();
    iy = iy.saturating_abs();
    if ix < iy {
        std::mem::swap(&mut ix, &mut iy);
    }
    let t = iy + (iy >> 1);
    ix - (ix >> 5) - (ix >> 7) + (t >> 2) + (t >> 6)
}

/// Chocolate `CLIPMASK1` for `hits()` / `hitasprite`.
const CLIPMASK1: u32 = (256 << 16) + 64;

/// Chocolate `hits(i)` — XY range of a horizontal hitscan along `ang`.
fn hits_xy(
    sp: &SpriteT,
    spr_hits: &[SpriteHit],
    sectors: &[SectorWalls],
    walls: &[WallXY],
    sector_z: &[SectorZ],
) -> i32 {
    use room_build_engine::geom::hitscan_full;
    let (sin, cos) = sincos_q14(i32::from(sp.ang) & 2047);
    let h = hitscan_full(
        sp.x,
        sp.y,
        sp.z,
        sp.sectnum,
        cos,
        sin,
        0,
        CLIPMASK1,
        sectors,
        walls,
        spr_hits,
        Some(sector_z),
    );
    find_distance_2d(h.hitx - sp.x, h.hity - sp.y)
}

/// Chocolate `move()` after CON parse (seekplayer / geth / skill odd-frame skip).
fn actor_con_move(
    sp: &mut SpriteT,
    _spr_hits: &[SpriteHit],
    spr_clips: &[SpriteZClip],
    temps: &mut ActorConTemps,
    script: &ScriptImage,
    sectors: &[SectorT],
    walls: &[WallT],
    actortype: &[u8],
    tile_meta: &[TileZMeta],
    player_skill: u8,
    // Chocolate `ps[g_p].posx/posy` for `faceplayer` (not `lastvx`).
    player_x: i32,
    player_y: i32,
    randomseed: &mut i32,
) {
    let mut a = sp.hitag;
    if a == -1 {
        a = 0;
    }
    temps.g_t[0] += 1;
    // Chocolate `faceplayer` / `faceplayerslow` run even when move ptr is 0.
    if a & MOVE_FACEPLAYER != 0 {
        let goal = getangle(player_x - sp.x, player_y - sp.y);
        let mut angdif = getincangle(i32::from(sp.ang), goal) >> 2;
        if angdif > -8 && angdif < 0 {
            angdif = 0;
        }
        sp.ang = ((i32::from(sp.ang) + angdif) & 2047) as i16;
    }
    if a & MOVE_FACEPLAYERSLOW != 0 {
        let goal = getangle(player_x - sp.x, player_y - sp.y);
        let mut angdif = getincangle(i32::from(sp.ang), goal).signum() << 5;
        if angdif > -32 && angdif < 0 {
            angdif = 0;
            sp.ang = (goal & 2047) as i16;
        }
        sp.ang = ((i32::from(sp.ang) + angdif) & 2047) as i16;
    }
    let move_ptr = temps.g_t[1];
    if move_ptr == 0 || a == 0 {
        return;
    }
    let mpu = move_ptr as usize;
    let hvel = script.script.get(mpu).copied().unwrap_or(0);
    let vvel = script.script.get(mpu + 1).copied().unwrap_or(0);
    let mut xvel = i32::from(sp.xvel);
    let mut zvel = i32::from(sp.zvel);
    if a & MOVE_GETH != 0 {
        xvel += (hvel - xvel) >> 1;
    }
    if a & MOVE_GETV != 0 {
        zvel += ((vvel << 4) - zvel) >> 1;
    }
    // alterang
    xvel += (hvel - xvel) / 5;
    if zvel < 648 {
        zvel += ((vvel << 4) - zvel) / 5;
    }
    if a & MOVE_SEEKPLAYER != 0 {
        let goal = getangle(temps.lastvx - sp.x, temps.lastvy - sp.y);
        // Chocolate: only retarget ang when xvel==0, or in the <2 / 18..26 windows.
        if xvel != 0 {
            let ticselapsed = temps.g_t[0] & 31;
            let angdif = getincangle(i32::from(sp.ang), goal);
            if ticselapsed < 2 {
                if angdif.abs() < 256 {
                    // Chocolate: `j = 128-(TRAND&256)`; undo if `hits() < 844`.
                    let j = 128 - (krand_step(randomseed) & 256);
                    sp.ang = ((i32::from(sp.ang) + j) & 2047) as i16;
                    let sec: Vec<SectorWalls> = sectors
                        .iter()
                        .map(|s| SectorWalls {
                            wallptr: s.wallptr,
                            wallnum: s.wallnum,
                        })
                        .collect();
                    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
                    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
                    // Wall-only hits(): Room's sprite cylinder hitscan is hotter
                    // than Chocolate's and would false-hit nearby troopers once
                    // `cstat|=257` is set at spawn (DEMO1 seekplayer sync).
                    if hits_xy(sp, &[], &sec, &wxy, &sec_z) < 844 {
                        sp.ang = ((i32::from(sp.ang) - j) & 2047) as i16;
                    }
                }
            } else if (19..26).contains(&ticselapsed) {
                // Chocolate: `ticselapsed > 18 && ticselapsed < 26`
                if (angdif >> 2).abs() < 128 {
                    sp.ang = (goal & 2047) as i16;
                } else {
                    sp.ang = ((i32::from(sp.ang) + (angdif >> 2)) & 2047) as i16;
                }
            }
        } else {
            sp.ang = (goal & 2047) as i16;
        }
    }
    if xvel > -6 && xvel < 6 {
        xvel = 0;
    }
    sp.xvel = xvel as i16;
    sp.zvel = zvel as i16;

    let is_bad = badguy_pic(sp.picnum, actortype);
    // Chocolate `move()` floor/ceiling clamps run *before* the skill odd-frame
    // return — so a freshly woken underwater OCTA with getv&lt;0 snaps to
    // `ceilz+(66<<8)` even when movesprite is skipped (DEMO1 tic138).
    if is_bad && sp.picnum != ORGANTIC {
        if zvel > 0 && temps.hit_floorz < sp.z {
            sp.z = temps.hit_floorz;
        }
        if zvel < 0 {
            let sect = sp.sectnum;
            let ceilz = if sect >= 0 && (sect as usize) < sectors.len() {
                let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
                let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
                getzsofslope(sect, sp.x, sp.y, &sec_z, &wxy).0
            } else {
                temps.hit_ceilingz
            };
            if sp.z - ceilz < (66 << 8) {
                sp.z = ceilz + (66 << 8);
                sp.zvel >>= 1;
                zvel = i32::from(sp.zvel);
            }
        }
    }

    let mut daxvel = xvel;
    let angdif = i32::from(sp.ang) & 2047;
    if is_bad {
        // DEMO skill < 2: odd `g_t[0]` skips movesprite (after alterang).
        // Chocolate also uses the looser `&1` gate when `bposz != z` (fall moved us).
        let z_moved = temps.bposz != sp.z;
        if player_skill < 2 || z_moved {
            if temps.g_t[0] & 1 != 0 {
                return;
            }
            daxvel <<= 1;
        } else if temps.g_t[0] & 3 != 0 {
            return;
        } else {
            daxvel <<= 2;
        }
    }
    if daxvel == 0 && zvel == 0 {
        return;
    }
    let (sin, cos) = sincos_q14(angdif);
    // Chocolate movesprite args = (daxvel*sintable)>>14, then clipmove
    // `((arg*TICSPERFRAME)<<11)` → world Δ `(arg*TICSPERFRAME)>>3`.
    let step_x = (daxvel * cos) >> 14;
    let step_y = (daxvel * sin) >> 14;
    let dx = (step_x * TICSPERFRAME) >> 3;
    let dy = (step_y * TICSPERFRAME) >> 3;
    // z: Chocolate `z + ((zvel*TICSPERFRAME)>>3)`
    let dz = (zvel * TICSPERFRAME) >> 3;
    let sec: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    // Badguy default clipdist 192 when actortype&3 == 0 (Chocolate movesprite).
    let cd = if actortype.get(sp.picnum as usize).copied().unwrap_or(0) & 3 != 0 {
        i32::from(sp.clipdist) << 2
    } else {
        192
    };
    // Chocolate: daz = z - ((tilesizy*yrepeat)<<1); clip with (4<<8) ceil/flor.
    let th = tile_meta
        .get(sp.picnum as usize)
        .map(|t| i32::from(t.height))
        .unwrap_or(64);
    let h = (th * i32::from(sp.yrepeat)) << 1;
    let daz = sp.z - h;
    let oldx = sp.x;
    let oldy = sp.y;
    let oldsect = sp.sectnum;
    // Chocolate CON `move()` → `movesprite(..., CLIPMASK0)`.
    const CLIPMASK0_SPR: u16 = 1;
    let retval = clipmove_xy_at_z_sprites(
        &mut sp.x,
        &mut sp.y,
        daz,
        &mut sp.sectnum,
        dx,
        dy,
        cd,
        4 << 8,
        4 << 8,
        &sec,
        &wxy,
        Some(&sec_z),
        spr_clips,
        CLIPMASK0_SPR,
    );
    // Chocolate `movesprite`: early-return (no Z) when destination sector is
    // invalid for this badguy (stayput / lotag gates).
    let dasect = sp.sectnum;
    let revert = dasect < 0
        || (dasect as usize) >= sectors.len()
        || (temps.actorstayput >= 0 && temps.actorstayput != dasect)
        || {
            let lotag = sectors[dasect as usize].lotag;
            (sp.picnum == BOSS2 && sp.pal == 0 && lotag != 3)
                || ((sp.picnum == BOSS1 || sp.picnum == BOSS2) && lotag == 1)
                || (lotag == 1 && (sp.picnum == LIZMAN || (sp.picnum == LIZTROOP && sp.zvel == 0)))
        };
    if revert {
        sp.x = oldx;
        sp.y = oldy;
        sp.sectnum = oldsect;
        // Chocolate: LIZMAN water / (g_t[0]&3)==1 → random ang (not COMMANDER).
        let lotag = if dasect >= 0 && (dasect as usize) < sectors.len() {
            sectors[dasect as usize].lotag
        } else {
            0
        };
        if lotag == 1 && sp.picnum == LIZMAN {
            sp.ang = (krand_step(randomseed) & 2047) as i16;
        } else if (temps.g_t[0] & 3) == 1 && sp.picnum != COMMANDER {
            sp.ang = (krand_step(randomseed) & 2047) as i16;
        }
        return;
    }
    // Chocolate: wall hit → ang += 768 when cgg==0.
    if (retval & 49152) >= 32768 && temps.cgg == 0 {
        sp.ang = ((i32::from(sp.ang) + 768) & 2047) as i16;
    }
    // Chocolate clamps Z to hit ceiling/floor; Room applies ΔZ then relies on
    // the pre-movesprite badguy ceil/floor snaps above (hit_* may be stale).
    sp.z += dz;
}

/// Run CON for every sprite on [`STAT_ACTOR`].
///
/// `script` word 0 is the end marker; actor entry is `actorscrptr[picnum]`.
/// Picnums without a script (`0`) are skipped. `killit` deletes the sprite
/// from the status list (full `deletesprite` later). CON `fall` runs
/// Chocolate CON-fall (if/else overshoot; not `makeitfall`) after each `execute`.
/// After a non-`killit` parse, Chocolate `move()` always does `g_t[0]++`.
pub fn moveactors(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    tile_meta: &[TileZMeta],
    picnum: &mut Vec<i16>,
    actorscrptr: &[i32],
    script: &ScriptImage,
    players: &mut [Player],
    sync_bits: u32,
    randomseed: &mut i32,
    temps: &mut Vec<ActorConTemps>,
) {
    moveactors_with_skill(
        lists,
        sprites,
        sectors,
        walls,
        tile_meta,
        picnum,
        actorscrptr,
        script,
        players,
        sync_bits,
        randomseed,
        temps,
        0,
    );
}

/// Like [`moveactors`] with an explicit Duke skill (0..=4) for move odd-frame skip.
pub fn moveactors_with_skill(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    tile_meta: &[TileZMeta],
    picnum: &mut Vec<i16>,
    actorscrptr: &[i32],
    script: &ScriptImage,
    players: &mut [Player],
    sync_bits: u32,
    randomseed: &mut i32,
    temps: &mut Vec<ActorConTemps>,
    player_skill: u8,
) {
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    // Snapshot for `ifcansee` / `furthestcanseepoint` hitscan (CLIPMASK1).
    let cansee_sprs: Vec<SpriteHit> = sprites
        .iter()
        .map(|s| {
            SpriteHit::cylinder(
                s.x,
                s.y,
                s.z,
                s.sectnum,
                i32::from(s.clipdist) << 2,
                s.cstat,
            )
        })
        .collect();
    let mut i = lists.headspritestat[STAT_ACTOR as usize];
    while i >= 0 {
        let next = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu >= MAXSPRITES || iu >= picnum.len() || iu >= sprites.len() {
            i = next;
            continue;
        }
        let pic_i = picnum[iu];
        // FIRE / FIRE2 run their CON (`useractor notenemy FIRE … state
        // firestate`): Chocolate `moveactors` has no C case for them. The old
        // skip here dropped their `ifrnd` rolls (7 krands on DEMO1 tic 0).
        // Legacy DEMO1 fix-ups were tuned with the skip, so keep it there.
        if crate::fixups::on() && (pic_i == FIRE || pic_i == FIRE2) {
            i = next;
            continue;
        }
        if temps.len() <= iu {
            temps.resize_with(iu + 1, ActorConTemps::default);
        }
        // Chocolate: snapshot bpos* before execute/fall (skill gate uses bposz).
        temps[iu].bposz = sprites[iu].z;
        let pic = pic_i as usize;
        if pic >= actorscrptr.len() {
            i = next;
            continue;
        }
        let entry = actorscrptr[pic];
        if entry <= 0 {
            i = next;
            continue;
        }
        // Chocolate `execute`: insptr = 4 + actorscrptr[pic] (skip actor header).
        let body = (entry as usize).saturating_add(4);
        if temps.len() <= iu {
            temps.resize(iu + 1, ActorConTemps::default());
        }
        temps[iu].fall_requested = false;
        temps[iu].pending_ang = None;
        temps[iu].pending_size = None;
        temps[iu].pending_weapon_ops.clear();
        temps[iu].pending_spawns.clear();
        temps[iu].pending_shoots.clear();

        let (pidx, dist) = findplayer_dist(&sprites[iu], players);
        let mut env_player = ConSpriteEnv {
            player_dist: dist,
            sync_bits,
            player_skill,
            ..ConSpriteEnv::default()
        };
        if let Some(p) = players.get(pidx) {
            env_player.player_x = p.posx;
            env_player.player_y = p.posy;
            env_player.player_z = p.posz;
            env_player.player_sect = if p.sprite_i >= 0 {
                sprites
                    .get(p.sprite_i as usize)
                    .map(|s| s.sectnum)
                    .unwrap_or(p.cursectnum)
            } else {
                p.cursectnum
            };
            env_player.player_ang = p.ang;
            env_player.player_extra = p.extra;
            env_player.player_on_ground = p.on_ground;
            env_player.player_jumping_counter = p.jumping_counter;
            env_player.player_jetpack_on = p.jetpack_on;
            env_player.player_poszv = p.poszv;
            env_player.player_curr_weapon = p.curr_weapon;
            env_player.player_ammo = p.ammo;
            env_player.player_gotweapon = p.gotweapon;
            if p.sprite_i >= 0 {
                if let Some(ps) = sprites.get(p.sprite_i as usize) {
                    env_player.player_xrepeat = ps.xrepeat;
                    env_player.player_sprite_z = ps.z;
                    env_player.player_x = ps.x;
                    env_player.player_y = ps.y;
                }
            }
            // Prefer `ps[].extra` (synced onto the body sprite in `sync_player_sprite`).
            env_player.player_extra = p.extra;
        }

        let sect = sprites[iu].sectnum;
        let sec = if sect >= 0 && (sect as usize) < sectors.len() {
            sectors[sect as usize]
        } else {
            SectorT::default()
        };
        let stop = execute_actor(
            script,
            body,
            randomseed,
            &mut temps[iu],
            &mut sprites[iu],
            i as i16,
            &sec,
            sec.floorz,
            sec.ceilingz,
            env_player,
            &sec_z,
            &wxy,
            &cansee_sprs,
        );
        if let Some(ang) = temps[iu].pending_ang.take() {
            sprites[iu].ang = ang;
        }
        if let Some((xr, yr)) = temps[iu].pending_size.take() {
            sprites[iu].xrepeat = xr;
            sprites[iu].yrepeat = yr;
        }
        // CON addweapon/addammo (killit_flag=2 already aborted parse without KillIt).
        if let Some(p) = players.get_mut(pidx) {
            for (is_add, wpn, ammo) in temps[iu].pending_weapon_ops.drain(..) {
                let _ = crate::weapons::con_weapon_op(p, is_add, wpn, ammo);
            }
        } else {
            temps[iu].pending_weapon_ops.clear();
        }
        // Chocolate: CON `fall` runs mid-parse (OCTABRAIN first opcode) before
        // `spawn BLOOD` in checkoctahitstate — apply before deferred spawns so
        // blood Z uses post-fall parent Z (DEMO1 tic231 #196: 62720 not 62758).
        if temps[iu].fall_requested {
            temps[iu].fall_requested = false;
            let peers: Vec<SpriteT> = sprites.to_vec();
            con_fall(
                &mut sprites[iu],
                sectors,
                walls,
                &peers,
                tile_meta,
                DUKE_GC,
                &mut temps[iu],
            );
        }
        // CON `spawn` / `shoot` — after fall + damage so BLOOD freelist/Z match.
        let spawns: Vec<i16> = temps[iu].pending_spawns.drain(..).collect();
        let shoots: Vec<i16> = temps[iu].pending_shoots.drain(..).collect();
        for pn in spawns {
            if pn == BLOOD {
                let _ = con_spawn_blood(lists, sprites, picnum, temps, Some(script), i);
            }
            // Other spawn pics: deferred until needed for bitexact.
        }
        for pn in shoots {
            if matches!(
                pn,
                x if x == BLOODSPLAT1 || x == BLOODSPLAT2 || x == BLOODSPLAT3 || x == BLOODSPLAT4
            ) {
                shoot_bloodsplat_enemy(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum,
                    temps,
                    tile_meta,
                    Some(script),
                    randomseed,
                    i,
                    pn,
                );
            } else if pn == COOLEXPLOSION1 {
                shoot_coolexplosion_enemy(
                    lists,
                    sprites,
                    picnum,
                    temps,
                    tile_meta,
                    Some(script),
                    script.actortype.as_slice(),
                    players,
                    randomseed,
                    i,
                );
            }
        }
        if stop == ConStop::KillIt {
            let _ = lists.deletesprite(i);
        } else {
            // Snapshot for Chocolate `hits()` / CLIPMASK0 face-sprite clips.
            let spr_hits: Vec<SpriteHit> = sprites
                .iter()
                .map(|s| {
                    SpriteHit::cylinder(
                        s.x,
                        s.y,
                        s.z,
                        s.sectnum,
                        i32::from(s.clipdist) << 2,
                        s.cstat,
                    )
                })
                .collect();
            let mx = sprites[iu].x;
            let my = sprites[iu].y;
            // Fast movers resting on a peer L-face: Chocolate passes through on
            // the next <<=2 step (DEMO1 #400 xv=76 vs #401); slow peers still clip.
            let fast_rest = sprites[iu].xvel.abs() >= 48;
            let spr_clips: Vec<SpriteZClip> = sprites
                .iter()
                .enumerate()
                .map(|(j, s)| {
                    let mut z = sprite_z_clip(s, tile_meta);
                    if j == iu {
                        z.cstat = 0;
                    } else if fast_rest
                        && badguy_pic(s.picnum, script.actortype.as_slice())
                        && badguy_pic(sprites[iu].picnum, script.actortype.as_slice())
                    {
                        let bsz = (i32::from(s.clipdist) << 2) + 192;
                        let dx = (mx - s.x).abs();
                        let dy = (my - s.y).abs();
                        if dy < bsz && (-2..=2).contains(&(dx - bsz)) {
                            z.cstat = 0;
                        }
                    }
                    z
                })
                .collect();
            let (px, py) = players
                .get(pidx)
                .map(|p| (p.posx, p.posy))
                .unwrap_or((temps[iu].lastvx, temps[iu].lastvy));
            actor_con_move(
                &mut sprites[iu],
                &spr_hits,
                &spr_clips,
                &mut temps[iu],
                script,
                sectors,
                walls,
                script.actortype.as_slice(),
                tile_meta,
                player_skill,
                px,
                py,
                randomseed,
            );
        }
        i = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::con::{NullIncludes, compile};
    use room_build_engine::world::SpriteLists;

    #[test]
    fn moveactors_runs_killit_and_frees() {
        let src = b"\
define TILE 100\n\
actor TILE 10\n\
  killit\n\
enda\n\
";
        let img = compile(src, &mut NullIncludes);
        assert_eq!(img.errors, 0);
        let mut lists = SpriteLists::new();
        let sp = lists.insertsprite(0, STAT_ACTOR);
        assert!(sp >= 0);
        let mut picnum = vec![0_i16; MAXSPRITES];
        picnum[sp as usize] = 100;
        let mut sprites = vec![SpriteT::default(); MAXSPRITES];
        let mut seed = 1;
        let mut temps = Vec::new();
        moveactors(
            &mut lists,
            &mut sprites,
            &[],
            &[],
            &[],
            &mut picnum,
            &img.actorscrptr,
            &img,
            &mut [],
            0,
            &mut seed,
            &mut temps,
        );
        assert_eq!(
            lists.sprite_stat[sp as usize],
            room_build_engine::world::MAXSTATUS as i16
        );
    }
}
