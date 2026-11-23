//! Actor movers — Chocolate `moveactors` / CON `execute` (D5).

use room_build_engine::geom::{
    SectorWalls, SectorZ, SpriteHit, SpriteZClip, WallXY, cansee, clipmove_xy_at_z_cliptype,
    clipmove_xy_at_z_sprites_with_lists, getzsofslope,
};
use room_build_engine::helpers::{getangle, sincos_q14};
use room_build_engine::numeric::krand_step;
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::{MAXSPRITES, MAXSTATUS, SpriteLists};

use crate::badguy::badguy_pic;
use crate::con::vm::{CanseeInput, CanseeLazy, execute_actor_with};
use crate::con::{ActorConTemps, ConProjectile, ConSpriteEnv, ConStop, ScriptImage};
use crate::fta::findplayer_dist;
use crate::player::{DUKE_GC, Player, TileZMeta, sprite_z_clip};
use crate::shoot::{
    BLOOD, BLOODSPLAT1, BLOODSPLAT2, BLOODSPLAT3, BLOODSPLAT4, COOLEXPLOSION1, FREEZEBLAST,
    HEAVYHBOMB, con_spawn_blood, shoot_bloodsplat_enemy, shoot_coolexplosion_enemy,
};
use crate::spawn::{FIRE, FIRE2, GREENSLIME, con_fall};
use crate::stat::{STAT_ACTOR, STAT_DEFAULT};

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
const STRIPEBALL: i16 = 901;
const QUEBALL: i16 = 902;
const POOLBALLHIT: i16 = 8;
/// Atomic `DUKECAR` — the E1L1 intro shuttle.
pub(crate) const DUKECAR: i16 = 2491;
/// Atomic `HELECOPT`.
pub(crate) const HELECOPT: i16 = 1346;
const SCRAP6: i16 = 2390;
const WAR_AMBIENCE2: i16 = 232;
const RPG_EXPLODE: i16 = 9;

/// Chocolate `moveactors` STAT_DEFAULT cases for QUEBALL/STRIPEBALL.
pub(crate) fn tick_poolballs(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    tile_meta: &[TileZMeta],
    picnum: &mut Vec<i16>,
    players: &mut [Player],
    input_bits: &[u32],
    randomseed: &mut i32,
    temps: &mut Vec<ActorConTemps>,
    script: &ScriptImage,
) {
    let pocket = script
        .labels
        .get("POCKET")
        .filter(|label| !label.is_ptr)
        .map(|label| label.value as i16)
        .unwrap_or(903);
    let sec_walls: Vec<SectorWalls> = sectors.iter().map(SectorT::walls).collect();
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let mut order = Vec::new();
    let mut ball_i = lists.headspritestat[STAT_DEFAULT as usize];
    while ball_i >= 0 {
        order.push(ball_i);
        ball_i = lists.nextspritestat[ball_i as usize];
    }

    for ball_i in order {
        let ball_u = ball_i as usize;
        if ball_u >= sprites.len()
            || lists.sprite_stat.get(ball_u).copied() != Some(STAT_DEFAULT)
            || !matches!(sprites[ball_u].picnum, STRIPEBALL | QUEBALL)
        {
            continue;
        }
        if sprites[ball_u].xvel != 0 {
            let mut pocket_i = lists.headspritestat[STAT_DEFAULT as usize];
            let mut pocketed = false;
            while pocket_i >= 0 {
                let pi = pocket_i as usize;
                pocket_i = lists.nextspritestat[pi];
                if pi != ball_u
                    && sprites
                        .get(pi)
                        .is_some_and(|sprite| sprite.picnum == pocket)
                    && crate::shoot::ldist(&sprites[ball_u], &sprites[pi]) < 52
                {
                    let _ = lists.deletesprite(ball_i);
                    pocketed = true;
                    break;
                }
            }
            if pocketed {
                continue;
            }

            let ball = sprites[ball_u];
            let (sin, cos) = sincos_q14(i32::from(ball.ang) & 2047);
            let dx = (((i32::from(ball.xvel) * cos) >> 14) * TICSPERFRAME) << 11;
            let dy = (((i32::from(ball.xvel) * sin) >> 14) * TICSPERFRAME) << 11;
            let mut clip_sprites: Vec<SpriteZClip> = sprites
                .iter()
                .enumerate()
                .map(|(index, sprite)| {
                    let mut clip = sprite_z_clip(sprite, tile_meta);
                    if index == ball_u
                        || lists.sprite_stat.get(index).copied() == Some(MAXSTATUS as i16)
                    {
                        clip.cstat = 0;
                    }
                    clip
                })
                .collect();
            let old_sect = sprites[ball_u].sectnum;
            let hit = clipmove_xy_at_z_cliptype(
                &mut sprites[ball_u].x,
                &mut sprites[ball_u].y,
                ball.z,
                &mut sprites[ball_u].sectnum,
                dx,
                dy,
                24,
                4 << 8,
                4 << 8,
                &sec_walls,
                &wall_xy,
                Some(&sec_z),
                &clip_sprites,
                CLIPMASK1,
                Some(lists),
            );
            if sprites[ball_u].sectnum != old_sect {
                let _ = lists.changespritesect(ball_i, sprites[ball_u].sectnum);
            }
            clip_sprites.clear();
            if hit & 0xc000 == 0x8000 {
                let wall_i = (hit & 0x3fff) as usize;
                if let Some(wall) = walls.get(wall_i)
                    && let Some(end) = walls.get(wall.point2.max(0) as usize)
                {
                    let wall_ang = getangle(end.x - wall.x, end.y - wall.y);
                    sprites[ball_u].ang = ((wall_ang * 2 - i32::from(ball.ang)) & 2047) as i16;
                }
            } else if hit & 0xc000 == 0xc000 {
                let other_i = hit as u16 as usize & 0x3fff;
                if let Some(other) = sprites.get(other_i).copied() {
                    if matches!(other.picnum, STRIPEBALL | QUEBALL) {
                        let impulse = (sprites[ball_u].xvel >> 1) + (sprites[ball_u].xvel >> 2);
                        sprites[other_i].xvel = impulse;
                        sprites[other_i].ang = other
                            .ang
                            .wrapping_sub((ball.ang.wrapping_shl(1)).wrapping_add(1024));
                        sprites[ball_u].ang =
                            getangle(ball.x - other.x, ball.y - other.y).wrapping_sub(512) as i16;
                        if temps.len() <= ball_u {
                            temps.resize_with(ball_u + 1, ActorConTemps::default);
                        }
                        temps[ball_u].pending_actor_sounds.push(POOLBALLHIT);
                    } else if krand_step(randomseed) & 3 != 0 {
                        sprites[ball_u].xvel = 164;
                        sprites[ball_u].ang = other.ang;
                    } else {
                        crate::weapons_move::spawn_sprite_glass(
                            lists,
                            sprites,
                            temps,
                            picnum,
                            Some(script),
                            randomseed,
                            ball,
                            ball_i,
                            3,
                        );
                        let _ = lists.deletesprite(ball_i);
                        continue;
                    }
                }
            }
            sprites[ball_u].xvel = sprites[ball_u].xvel.saturating_sub(1).max(0);
            if sprites[ball_u].picnum == STRIPEBALL {
                sprites[ball_u].cstat =
                    257 | (4 & sprites[ball_u].xvel as u16) | (8 & sprites[ball_u].xvel as u16);
            }
        } else {
            let (player_i, dist) = findplayer_dist(&sprites[ball_u], players);
            let input = input_bits.get(player_i).copied().unwrap_or_default();
            if let Some(player) = players.get_mut(player_i) {
                let ball_yaw = getangle(
                    sprites[ball_u].x - player.posx,
                    sprites[ball_u].y - player.posy,
                );
                let yaw_delta = getincangle(i32::from(player.ang), ball_yaw);
                if dist < 1596
                    && input & crate::player::INPUT_OPEN != 0
                    && player.toggle_key_flag == 1
                    && (-64..64).contains(&yaw_delta)
                {
                    let mut blocked_by_ball = false;
                    let mut other_i = lists.headspritestat[STAT_DEFAULT as usize];
                    while other_i >= 0 {
                        let other_u = other_i as usize;
                        other_i = lists.nextspritestat[other_u];
                        if other_u == ball_u
                            || !matches!(sprites[other_u].picnum, STRIPEBALL | QUEBALL)
                        {
                            continue;
                        }
                        let other = sprites[other_u];
                        let other_dist = (player.posx - other.x).saturating_abs()
                            + (player.posy - other.y).saturating_abs()
                            + ((player.posz - other.z + (28 << 8)).saturating_abs() >> 4);
                        let other_yaw = getangle(other.x - player.posx, other.y - player.posy);
                        let other_delta = getincangle(i32::from(player.ang), other_yaw);
                        if other_dist < dist && (-64..64).contains(&other_delta) {
                            blocked_by_ball = true;
                            break;
                        }
                    }
                    if !blocked_by_ball {
                        sprites[ball_u].xvel = if sprites[ball_u].pal == 12 { 164 } else { 140 };
                        sprites[ball_u].ang = player.ang;
                        player.toggle_key_flag = 2;
                    }
                }
                if dist < 512 && sprites[ball_u].sectnum == player.cursectnum {
                    sprites[ball_u].ang = ball_yaw as i16;
                    sprites[ball_u].xvel = 48;
                }
            }
        }
    }
}
const LIZTROOP: i16 = 1680;
const LIZMAN: i16 = 2120;
const BOSS1: i16 = 2630;
const BOSS2: i16 = 2710;
const COMMANDER: i16 = 1920;
/// Atomic tiles with their own `move()` rules.
const DRONE: i16 = 1880;
const SHARK: i16 = 1550;
const ROTATEGUN: i16 = 2360;
const APLAYER: i16 = 1405;
pub(crate) const BLIMP_WEAPON_SPRITES: [i16; 14] =
    [23, 22, 42, 44, 44, 57, 54, 53, 55, 44, 44, 23, 44, 24];
const PIPEBOMB_BLAST_RADIUS_FALLBACK: i32 = 2500;
const EXPLOSION2: i16 = 1890;
const WATERSPLASH2: i16 = 1380;
const RADIUSEXPLOSION_DAMAGE: i16 = 1670;

fn getincangle(a: i32, goal: i32) -> i32 {
    (((goal - a) + 1024) & 2047) - 1024
}

/// Chocolate `FindDistance2D` (Build approx, not `ksqrt`).
pub(crate) fn find_distance_2d(mut ix: i32, mut iy: i32) -> i32 {
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
pub(crate) fn hits_xy(
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
    lists: &SpriteLists,
    sp: &mut SpriteT,
    // Chocolate `hits()` / CLIPMASK0 face-sprite clips, built on first use.
    clip_source: &mut ClipSource<'_>,
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
    // Chocolate `g_x`: `findplayer` distance for this actor's tic.
    g_x: i32,
    // `ps[g_p]`: pushed back by large close enemies; `actorsqu` never moves.
    mut player: Option<&mut Player>,
    single_player: bool,
    self_i: i16,
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

    // Chocolate `move()`: everything below runs only while the actor moves.
    if xvel == 0 && zvel == 0 {
        return;
    }
    let is_bad = badguy_pic(sp.picnum, actortype);
    let plane_z = |sp: &SpriteT| -> (i32, i32) {
        if sp.sectnum >= 0 && (sp.sectnum as usize) < sectors.len() {
            let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
            let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
            getzsofslope(sp.sectnum, sp.x, sp.y, &sec_z, &wxy)
        } else {
            (temps.hit_ceilingz, temps.hit_floorz)
        }
    };
    // Floor/ceiling clamps run *before* the skill odd-frame return — so a
    // freshly woken underwater OCTA with getv<0 snaps to `ceilz+(66<<8)` even
    // when movesprite is skipped (DEMO1 tic138).
    if is_bad && sp.picnum != ROTATEGUN {
        if (sp.picnum == DRONE || sp.picnum == COMMANDER) && sp.extra > 0 {
            if sp.picnum == COMMANDER {
                let (ceilz, florz) = plane_z(sp);
                temps.hit_floorz = florz;
                if sp.z > florz - (8 << 8) {
                    sp.z = florz - (8 << 8);
                    sp.zvel = 0;
                }
                temps.hit_ceilingz = ceilz;
                if sp.z - ceilz < (80 << 8) {
                    sp.z = ceilz + (80 << 8);
                    sp.zvel = 0;
                }
            } else if sp.zvel > 0 {
                let (_, florz) = plane_z(sp);
                temps.hit_floorz = florz;
                if sp.z > florz - (30 << 8) {
                    sp.z = florz - (30 << 8);
                }
            } else {
                let (ceilz, _) = plane_z(sp);
                temps.hit_ceilingz = ceilz;
                if sp.z - ceilz < (50 << 8) {
                    sp.z = ceilz + (50 << 8);
                    sp.zvel = 0;
                }
            }
            zvel = i32::from(sp.zvel);
        } else if sp.picnum != ORGANTIC {
            if zvel > 0 && temps.hit_floorz < sp.z {
                sp.z = temps.hit_floorz;
            }
            if zvel < 0 {
                let (ceilz, _) = plane_z(sp);
                if sp.z - ceilz < (66 << 8) {
                    sp.z = ceilz + (66 << 8);
                    sp.zvel >>= 1;
                    zvel = i32::from(sp.zvel);
                }
            }
        }
    } else if sp.picnum == APLAYER && sp.z - temps.hit_ceilingz < (32 << 8) {
        sp.z = temps.hit_ceilingz + (32 << 8);
    }

    let mut daxvel = xvel;
    let mut angdif = i32::from(sp.ang) & 2047;
    if is_bad && sp.picnum != ROTATEGUN {
        let squished = player
            .as_ref()
            .is_some_and(|p| p.actorsqu >= 0 && p.actorsqu == self_i);
        if g_x < 960 && sp.xrepeat > 16 {
            // Too close to a big enemy: it pushes toward the player and the
            // player's momentum is damped (`dukefriction-0x2000`).
            daxvel = -(1024 - g_x);
            angdif = getangle(player_x - sp.x, player_y - sp.y) & 2047;
            if let Some(p) = player.as_mut() {
                if g_x < 512 {
                    p.posxv = 0;
                    p.posyv = 0;
                } else {
                    let k = i64::from(crate::player::DUKE_FRICTION - 0x2000);
                    p.posxv = ((i64::from(p.posxv) * k) >> 16) as i32;
                    p.posyv = ((i64::from(p.posyv) * k) >> 16) as i32;
                }
            }
        } else if sp.picnum != DRONE && sp.picnum != SHARK && sp.picnum != COMMANDER {
            // Skill < 2 (single player) or a vertical change since the last
            // tic: move every other tic at double speed, else every fourth
            // tic at quadruple speed.
            let z_moved = temps.bposz != sp.z;
            if z_moved || (single_player && player_skill < 2) {
                if temps.g_t[0] & 1 != 0 || squished {
                    return;
                }
                daxvel <<= 1;
            } else {
                if temps.g_t[0] & 3 != 0 || squished {
                    return;
                }
                daxvel <<= 2;
            }
        }
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
    let cd = if sp.xrepeat > 60 {
        // Chocolate `movesprite`: large badguys use the full 1024-unit
        // collision radius, independently of their `actortype` bits.
        1024
    } else if sp.picnum == LIZMAN {
        292
    } else if actortype.get(sp.picnum as usize).copied().unwrap_or(0) & 3 != 0 {
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
    let retval = clipmove_xy_at_z_sprites_with_lists(
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
        clip_source.clips(lists, tile_meta),
        CLIPMASK0_SPR,
        Some(lists),
    );
    temps.movflag = retval as i16;
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
    // Chocolate accepts the candidate Z only inside the cached bounds set by
    // `getzrange` for this actor. If XY was clear but the cached vertical
    // interval blocks the step, movesprite reports the sector-coded hit.
    let next_z = sp.z.wrapping_add(dz);
    if next_z > temps.hit_ceilingz && next_z <= temps.hit_floorz {
        sp.z = next_z;
    } else if retval == 0 {
        temps.movflag = 16_384_i16.wrapping_add(sp.sectnum);
    }
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
    sectors: &mut [SectorT],
    walls: &mut [WallT],
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
        [false; 3],
        false,
        &mut [],
        0,
        0,
    );
}

/// Like [`moveactors`] with explicit skill and respawn-rule settings.
pub fn moveactors_with_skill(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    tile_meta: &[TileZMeta],
    picnum: &mut Vec<i16>,
    actorscrptr: &[i32],
    script: &ScriptImage,
    players: &mut [Player],
    sync_bits: u32,
    randomseed: &mut i32,
    temps: &mut Vec<ActorConTemps>,
    player_skill: u8,
    respawn_settings: [bool; 3],
    monsters_off: bool,
    active_sound_owners: &mut [Option<i16>],
    actor_toggle: u8,
    global_random: i32,
) {
    let mut decal_queue = crate::decal_queue::DukeDecalQueue::default();
    moveactors_with_skill_with_decal_queue(
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
        player_skill,
        respawn_settings,
        monsters_off,
        active_sound_owners,
        actor_toggle,
        global_random,
        &mut decal_queue,
    );
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn moveactors_with_skill_with_decal_queue(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    tile_meta: &[TileZMeta],
    picnum: &mut Vec<i16>,
    actorscrptr: &[i32],
    script: &ScriptImage,
    players: &mut [Player],
    sync_bits: u32,
    randomseed: &mut i32,
    temps: &mut Vec<ActorConTemps>,
    player_skill: u8,
    respawn_settings: [bool; 3],
    monsters_off: bool,
    active_sound_owners: &mut [Option<i16>],
    actor_toggle: u8,
    global_random: i32,
    decal_queue: &mut crate::decal_queue::DukeDecalQueue,
) {
    let heavybomb_tile = crate::shoot::script_tile_or(Some(script), "HEAVYHBOMB", HEAVYHBOMB);
    let morter_tile = crate::shoot::script_tile_or(Some(script), "MORTER", 1650);
    let bouncemine_tile = crate::shoot::script_tile_or(Some(script), "BOUNCEMINE", 940);
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    move_pipebombs(
        lists,
        sprites,
        sectors,
        walls,
        tile_meta,
        picnum,
        players,
        randomseed,
        temps,
        Some(script),
        respawn_settings[2],
    );
    let players_len = players.len();
    let mut cansee_view = ActorPassView::new(ViewKind::Cansee, sprites, lists, tile_meta);
    let mut clip_view = ActorPassView::new(ViewKind::Clip, sprites, lists, tile_meta);
    let mut i = lists.headspritestat[STAT_ACTOR as usize];
    while i >= 0 {
        let next = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu >= MAXSPRITES || iu >= picnum.len() || iu >= sprites.len() {
            i = next;
            continue;
        }
        let pic_i = picnum[iu];
        // Chocolate handles HEAVYHBOMB in the C actor switch; it does not run
        // an actor CON body for this tile.
        if pic_i == heavybomb_tile || pic_i == morter_tile || pic_i == bouncemine_tile {
            i = next;
            continue;
        }
        // FIRE / FIRE2 run their CON (`useractor notenemy FIRE … state
        // firestate`): Chocolate `moveactors` has no C case for them. The old
        // skip here dropped their `ifrnd` rolls (7 krands on DEMO1 tic 0).
        // Legacy DEMO1 fix-ups were tuned with the skip, so keep it there.
        if crate::fixups::on() && (pic_i == FIRE || pic_i == FIRE2) {
            i = next;
            continue;
        }
        let is_badguy = badguy_pic(pic_i, script.actortype.as_slice());
        let actor_cheat_active = players.len() < 2 && is_badguy && actor_toggle != 0;
        if actor_cheat_active && actor_toggle == 1 {
            // Chocolate `GAME.C::dnmonsters`: the actor switch sets DONTDRAW
            // and jumps to BOLT before the actor's movement/CON execute.
            sprites[iu].cstat = 32768;
            i = next;
            continue;
        }
        if actor_cheat_active && actor_toggle == 2 {
            sprites[iu].cstat = 257;
        }
        if temps.len() <= iu {
            temps.resize_with(iu + 1, ActorConTemps::default);
        }
        temps[iu].pending_actor_kills = 0;
        temps[iu].pending_wackplayer_angle = None;
        temps[iu].pending_pstomp = false;
        temps[iu].pending_operate = false;
        // Chocolate: snapshot bpos* before execute/fall (skill gate uses bposz).
        temps[iu].bposz = sprites[iu].z;
        if pic_i == DUKECAR || pic_i == HELECOPT {
            move_dukecar(
                lists, sprites, sectors, walls, picnum, temps, script, randomseed, iu,
            );
            i = next;
            continue;
        }
        if tick_frozen_slime(
            lists, sprites, sectors, &sec_z, &wxy, picnum, temps, script, players, randomseed, iu,
        ) {
            i = next;
            continue;
        }
        // Chocolate C-coded actors without a CON body (RECON, RAT, live
        // GREENSLIME, OOZ, CAMERA1): run their `moveactors` case.
        if crate::c_actors::is_c_actor(sprites[iu].picnum)
            && lists.sprite_stat.get(iu).copied() == Some(STAT_ACTOR)
        {
            let mut ctx = crate::c_actors::CActorCtx {
                lists,
                sprites,
                sectors,
                walls,
                picnum,
                temps,
                tile_meta,
                script,
                players,
                sync_bits,
                randomseed,
                global_random,
                single_player: players_len < 2,
                actor_toggle,
                decal_queue,
            };
            if crate::c_actors::move_c_actor(&mut ctx, iu) {
                i = next;
                continue;
            }
        }
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
        temps[iu].pending_access_cards.clear();
        temps[iu].pending_respawn_hitags.clear();
        temps[iu].pending_weapon_tosses.clear();
        temps[iu].pending_player_inventory.clear();
        temps[iu].pending_player_inventory_icon = None;
        temps[iu].pending_spawns.clear();
        temps[iu].pending_shoots.clear();

        let (pidx, dist) = findplayer_dist(&sprites[iu], players);
        let mut env_player = ConSpriteEnv {
            player_dist: dist,
            actor_owner: sprites[iu].owner,
            actor_yvel: sprites[iu].yvel,
            actor_pal: sprites[iu].pal,
            actor_is_badguy: badguy_pic(pic_i, script.actortype.as_slice()),
            actor_is_inventory: matches!(pic_i, 53 | 55 | 56 | 57 | 59 | 61 | 1348),
            respawn_monsters: respawn_settings[0],
            respawn_inventory: respawn_settings[1],
            respawn_items: respawn_settings[2],
            sync_bits,
            player_skill,
            multimode: players.len().min(usize::from(u8::MAX)) as u8,
            player_index: pidx.min(usize::from(u8::MAX)) as u8,
            ..ConSpriteEnv::default()
        };
        env_player.actor_tile_height = tile_meta
            .get(pic_i.max(0) as usize)
            .map_or(0, |meta| meta.height);
        if let Some(p) = players.get(pidx) {
            env_player.player_sprite_i = p.sprite_i;
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
            env_player.player_kickback_pic = p.kickback_pic;
            env_player.player_quick_kick = p.quick_kick;
            env_player.player_knee_incs = p.knee_incs;
            env_player.player_time_before_exit = p.time_before_exit;
            env_player.max_player_health = script
                .gamestartup
                .get(2)
                .copied()
                .unwrap_or(100)
                .clamp(1, i32::from(i16::MAX)) as i16;
            env_player.player_god_mode = p.god_mode;
            env_player.coop_mode = p.coop_mode;
            env_player.player_ammo = p.ammo;
            env_player.player_gotweapon = p.gotweapon;
            env_player.player_access_cards = p.got_access;
            env_player.player_shield_amount = p.shield_amount;
            env_player.player_steroids_amount = p.steroids_amount;
            env_player.player_scuba_amount = p.scuba_amount;
            env_player.player_holoduke_amount = p.holoduke_amount;
            env_player.player_jetpack_amount = p.jetpack_amount;
            env_player.player_heat_amount = p.heat_amount;
            env_player.player_firstaid_amount = p.firstaid_amount;
            env_player.player_boot_amount = p.boot_amount;
            env_player.player_inventory_icon = p.inventory_icon;
            env_player.player_weapon_reservations = p.weapon_reservations;
            env_player.player_weapon_reservation_count = p.weapon_reservation_count;
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
        // `iffloordistl`/`ifceilingdistl` read the cached hittype values. The
        // reference VM deliberately does not call `getglobalz` in those
        // predicates; only CON `fall` and specific C actor paths refresh them.
        // Keep the per-actor cache intact here instead of recomputing a range
        // every tic, which changes both branch decisions and CON RNG traffic.
        let hit_floorz = temps[iu].hit_floorz;
        let hit_ceilingz = temps[iu].hit_ceilingz;
        let mut projectile_i = lists.headspritestat[crate::stat::STAT_PROJECTILE as usize];
        let mut projectiles = Vec::new();
        while projectile_i >= 0 {
            let projectile_index = projectile_i as usize;
            let Some(projectile) = sprites.get(projectile_index) else {
                break;
            };
            projectiles.push(ConProjectile {
                x: projectile.x,
                y: projectile.y,
                sectnum: projectile.sectnum,
                ang: projectile.ang,
                owner: projectile.owner,
            });
            projectile_i = lists.nextspritestat[projectile_index];
        }
        // Run the actor on local copies so `shoot` can act on the live world
        // mid-parse (Chocolate `shoot()` is inline), then write them back.
        let mut actor_t = std::mem::take(&mut temps[iu]);
        let mut actor_s = sprites[iu];
        let stop = {
            let sectors_ro: &[SectorT] = sectors;
            let walls_ro: &[WallT] = walls;
            let players_ro: &[Player] = players;
            // `shoot` and `fall` both act on the live sprite array mid-parse.
            let sprites_cell = std::cell::RefCell::new(&mut *sprites);
            let lists_cell = std::cell::RefCell::new(&mut *lists);
            // Chocolate hitscan reads the live sector sprite lists per actor:
            // the sight view reflects earlier actors' moves, spawns and tile
            // changes, as of the start of this actor's code. It is built at
            // the first sight ray, or before `shoot`/`fall` change the world.
            let sight = LazySight {
                view: std::cell::RefCell::new(&mut cansee_view),
                built: std::cell::Cell::new(false),
                sprites: &sprites_cell,
                lists: &lists_cell,
                tile_meta,
            };
            let mut fall_now = |t: &mut ActorConTemps, z: &mut i32, zvel: &mut i16| {
                sight.build();
                let mut guard = sprites_cell.borrow_mut();
                let sprites: &mut [SpriteT] = &mut guard;
                // `me` is a copy, so the live array serves as the (unchanged)
                // peer snapshot without cloning all MAXSPRITES slots.
                let mut me = sprites[iu];
                me.z = *z;
                me.zvel = *zvel;
                con_fall(
                    &mut me, sectors_ro, walls_ro, sprites, tile_meta, DUKE_GC, t,
                );
                sprites[iu].z = me.z;
                sprites[iu].zvel = me.zvel;
                *z = me.z;
                *zvel = me.zvel;
            };
            let mut shoot_now = |pn: i16, ang: i16, seed: &mut i32| -> bool {
                sight.build();
                let mut guard = sprites_cell.borrow_mut();
                let sprites: &mut [SpriteT] = &mut guard;
                let mut lists_guard = lists_cell.borrow_mut();
                let lists: &mut SpriteLists = &mut lists_guard;
                let bloodsplat = pn == BLOODSPLAT1
                    || pn == BLOODSPLAT2
                    || pn == BLOODSPLAT3
                    || pn == BLOODSPLAT4;
                if !bloodsplat && pn != COOLEXPLOSION1 {
                    return false;
                }
                sprites[iu].ang = ang;
                if bloodsplat {
                    shoot_bloodsplat_enemy(
                        lists,
                        sprites,
                        sectors_ro,
                        walls_ro,
                        picnum,
                        temps,
                        tile_meta,
                        Some(script),
                        seed,
                        i,
                        pn,
                    );
                } else {
                    shoot_coolexplosion_enemy(
                        lists,
                        sprites,
                        picnum,
                        temps,
                        tile_meta,
                        Some(script),
                        script.actortype.as_slice(),
                        players_ro,
                        seed,
                        i,
                    );
                }
                true
            };
            execute_actor_with(
                script,
                body,
                randomseed,
                &mut actor_t,
                &mut actor_s,
                i as i16,
                &sec,
                hit_floorz,
                hit_ceilingz,
                env_player,
                &sec_z,
                &wxy,
                CanseeInput::Lazy(&sight),
                &projectiles,
                active_sound_owners,
                Some(&mut shoot_now),
                Some(&mut fall_now),
            )
        };
        temps[iu] = actor_t;
        sprites[iu] = actor_s;
        for low in temps[iu].pending_respawn_hitags.drain(..) {
            let mut marker_i = lists.headspritestat[crate::stat::STAT_FX as usize];
            while marker_i >= 0 {
                let marker_index = marker_i as usize;
                if marker_index >= sprites.len() || marker_index >= lists.nextspritestat.len() {
                    break;
                }
                marker_i = lists.nextspritestat[marker_index];
                let marker = sprites[marker_index];
                if marker.picnum == crate::spawn::RESPAWN
                    && marker.lotag == low
                    && !(monsters_off && badguy_pic(marker.hitag, script.actortype.as_slice()))
                {
                    // Chocolate `operaterespawns` arms the marker with 54;
                    // `tick_respawns` performs its transporter-star sequence.
                    sprites[marker_index].extra = 54;
                }
            }
        }
        let weapon_tosses: Vec<_> = temps[iu].pending_weapon_tosses.drain(..).collect();
        for (owner_i, tile, item_cstat) in weapon_tosses {
            let Some(parent_index) = usize::try_from(owner_i)
                .ok()
                .filter(|&index| index < sprites.len())
            else {
                continue;
            };
            let parent = sprites[parent_index];
            let spawned = crate::shoot::egs(
                lists,
                sprites,
                picnum,
                temps,
                Some(script),
                parent.sectnum,
                parent.x,
                parent.y,
                parent.z,
                tile,
                0,
                0,
                0,
                parent.ang,
                0,
                0,
                owner_i,
                crate::stat::STAT_DEFAULT,
            );
            let Ok(spawned_index) = usize::try_from(spawned) else {
                continue;
            };
            if let Some(cstat) = item_cstat {
                // Chocolate `spawn()` item branch for a player-thrown weapon.
                let item = &mut sprites[spawned_index];
                item.lotag = 0;
                item.z = item.z.wrapping_sub(32 << 8);
                item.zvel = -1024;
                item.cstat = cstat;
                item.xrepeat = 32;
                item.yrepeat = 32;
                item.shade = -17;
                item.pal = 0;
                let _ = lists.changespritestat(spawned, STAT_ACTOR);
            } else if script
                .actorscrptr
                .get(tile.max(0) as usize)
                .is_some_and(|entry| *entry > 0)
            {
                let _ = lists.changespritestat(spawned, STAT_ACTOR);
            }
        }
        let hit_radii: Vec<_> = temps[iu].pending_hit_radii.drain(..).collect();
        for request in hit_radii {
            crate::weapons_move::apply_con_hitradius(
                lists,
                sprites,
                sectors,
                walls,
                picnum,
                temps,
                players,
                randomseed,
                i as i16,
                request.radius,
                request.damage,
                script.actortype.as_slice(),
                Some(script),
                tile_meta,
            );
        }
        // Chocolate CON `cactor` changes the live tile while retaining the
        // current sprite status list; keep the sidecar tile cache in sync.
        picnum[iu] = sprites[iu].picnum;
        if let Some(ang) = temps[iu].pending_ang.take() {
            sprites[iu].ang = ang;
        }
        if let Some((xr, yr)) = temps[iu].pending_size.take() {
            sprites[iu].xrepeat = xr;
            sprites[iu].yrepeat = yr;
        }
        let quote_requests: Vec<i16> = temps[iu].pending_quotes.drain(..).collect();
        // CON addweapon/addammo (killit_flag=2 already aborted parse without KillIt).
        if let Some(p) = players.get_mut(pidx) {
            p.actors_killed = p
                .actors_killed
                .wrapping_add(temps[iu].pending_actor_kills as u8);
            temps[iu].pending_actor_kills = 0;
            if temps[iu].pending_quick_kick {
                if p.quick_kick == 0 {
                    p.quick_kick = 14;
                }
                temps[iu].pending_quick_kick = false;
            }
            if let Some(angle) = temps[iu].pending_wackplayer_angle.take() {
                p.horiz = p.horiz.wrapping_add(64);
                p.return_to_center = 9;
                p.look_ang = angle;
                p.rotscrnang = angle;
            }
            if temps[iu].pending_pstomp
                && p.knee_incs == 0
                && p.sprite_i >= 0
                && sprites
                    .get(p.sprite_i as usize)
                    .is_some_and(|body| body.xrepeat >= 40)
            {
                p.knee_incs = 1;
                if p.weapon_pos == 0 {
                    p.weapon_pos = -1;
                }
                p.actorsqu = i as i16;
            }
            temps[iu].pending_pstomp = false;
            if let Some(timer) = temps[iu].pending_end_of_game.take() {
                p.time_before_exit = timer;
                p.custom_exit_sound = -1;
                p.end_of_game = true;
            }
            for picnum in temps[iu].pending_weapon_reservations.drain(..) {
                let count = usize::from(p.weapon_reservation_count);
                if count < p.weapon_reservations.len() {
                    p.weapon_reservations[count] = picnum;
                    p.weapon_reservation_count += 1;
                }
            }
            let mut low_health_sounds = 0_u8;
            for (amount, is_atomic_health) in temps[iu].pending_player_health.drain(..) {
                if p.god_mode {
                    continue;
                }
                let max_health = i32::from(env_player.max_player_health);
                let mut health = i32::from(p.extra);
                let amount = i32::from(amount);
                if !is_atomic_health && health > max_health && amount > 0 {
                    continue;
                }
                if health > 0 {
                    health = health.saturating_add(amount);
                }
                let cap = if is_atomic_health {
                    max_health.saturating_mul(2)
                } else {
                    max_health
                };
                if health > cap && amount > 0 {
                    health = cap;
                }
                let low_health_threshold = max_health >> 2;
                if amount > 0
                    && health - amount < low_health_threshold
                    && health >= low_health_threshold
                {
                    // Chocolate `spritesound(DUKE_GOTHEALTHATLOW, ps[g_p].i)`.
                    low_health_sounds = low_health_sounds.saturating_add(1);
                }
                p.extra = health.max(0).min(i32::from(i16::MAX)) as i16;
                if amount > 0 {
                    p.last_extra = p.extra;
                }
                if p.sprite_i >= 0 {
                    if let Some(body) = sprites.get_mut(p.sprite_i as usize) {
                        body.extra = p.extra;
                    }
                }
            }
            temps[iu]
                .pending_actor_sounds
                .extend(std::iter::repeat_n(229, usize::from(low_health_sounds)));
            // Chocolate `FTA(q, p, 0)`: honor the FTA toggle and preserve
            // persistent high-priority frag quotes 115/116.
            for quote in quote_requests {
                if !p.fta_enabled {
                    continue;
                }
                if p.quote_timer > 0
                    && !matches!(quote, 115 | 116)
                    && matches!(p.quote_id, 115 | 116)
                {
                    continue;
                }
                p.quote_timer = 100;
                if p.quote_id != quote || quote == 26 {
                    p.quote_id = quote;
                }
            }
            if temps[iu].pending_tip {
                p.tip_timer = 26;
                temps[iu].pending_tip = false;
            }
            for (is_add, wpn, ammo) in temps[iu].pending_weapon_ops.drain(..) {
                let _ = crate::weapons::con_weapon_op(p, is_add, wpn, ammo);
            }
            for access_bit in temps[iu].pending_access_cards.drain(..) {
                p.got_access |= access_bit;
            }
            for (inventory, value) in temps[iu].pending_player_inventory.drain(..) {
                match inventory {
                    0 => p.steroids_amount = value,
                    1 => p.shield_amount = value,
                    2 => p.scuba_amount = value,
                    3 => p.holoduke_amount = value,
                    4 => p.jetpack_amount = value,
                    7 => p.heat_amount = value,
                    9 => p.firstaid_amount = value,
                    10 => p.boot_amount = value,
                    _ => {}
                }
            }
            if let Some(icon) = temps[iu].pending_player_inventory_icon.take() {
                p.inventory_icon = icon;
            }
            for (palette, tics) in temps[iu].pending_player_palette.drain(..) {
                p.palette_flash = palette;
                p.palette_flash_tics = tics;
            }
        } else {
            temps[iu].pending_weapon_ops.clear();
            temps[iu].pending_access_cards.clear();
            temps[iu].pending_player_inventory.clear();
            temps[iu].pending_player_inventory_icon = None;
            temps[iu].pending_actor_kills = 0;
            temps[iu].pending_quick_kick = false;
            temps[iu].pending_end_of_game = None;
            temps[iu].pending_weapon_reservations.clear();
            temps[iu].pending_player_health.clear();
            temps[iu].pending_player_palette.clear();
            temps[iu].pending_quotes.clear();
            temps[iu].pending_tip = false;
        }
        // Chocolate: CON `fall` runs mid-parse (OCTABRAIN first opcode) before
        // `spawn BLOOD` in checkoctahitstate — apply before deferred spawns so
        // blood Z uses post-fall parent Z (DEMO1 tic231 #196: 62720 not 62758).
        if temps[iu].fall_requested {
            temps[iu].fall_requested = false;
            // Fall a copy against the untouched array (no full-array clone).
            let mut me = sprites[iu];
            con_fall(
                &mut me,
                sectors,
                walls,
                sprites,
                tile_meta,
                DUKE_GC,
                &mut temps[iu],
            );
            sprites[iu] = me;
        }
        // CON `spawn` / `shoot` — after fall + damage so BLOOD freelist/Z match.
        let spawns: Vec<i16> = temps[iu].pending_spawns.drain(..).collect();
        let shoots: Vec<i16> = temps[iu].pending_shoots.drain(..).collect();
        let item_sprays: Vec<(i16, i16)> = temps[iu].pending_item_sprays.drain(..).collect();
        let glass_bursts: Vec<i16> = temps[iu].pending_glass.drain(..).collect();
        let debris_bursts: Vec<_> = temps[iu].pending_debris.drain(..).collect();
        let guts_bursts: Vec<_> = temps[iu].pending_guts.drain(..).collect();
        for count in glass_bursts {
            for shard in (1..=count.max(0)).rev() {
                let parent = sprites[iu];
                // Chocolate `spriteglass`: `a = TRAND&2047; z = SZ-((TRAND&16)<<8);` then the EGS
                // arguments right to left: zvel, xvel, shade.
                let angle = (krand_step(randomseed) & 2047) as i16;
                let z = parent.z.wrapping_sub((krand_step(randomseed) & 16) << 8);
                let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
                let xvel = (32 + (krand_step(randomseed) & 63)) as i16;
                let shade = (krand_step(randomseed) & 15) as i8;
                let glass = crate::shoot::egs(
                    lists,
                    sprites,
                    picnum,
                    temps,
                    Some(script),
                    parent.sectnum,
                    parent.x,
                    parent.y,
                    z,
                    1031 + shard % 3, // GLASSPIECES + (j % 3)
                    shade,
                    36,
                    36,
                    angle,
                    xvel,
                    zvel,
                    i as i16,
                    crate::stat::STAT_MISC,
                );
                if glass >= 0 {
                    sprites[glass as usize].pal = parent.pal;
                }
            }
        }
        for debris in debris_bursts {
            let index = crate::shoot::egs(
                lists,
                sprites,
                picnum,
                temps,
                Some(script),
                debris.sector,
                debris.position[0],
                debris.position[1],
                debris.position[2],
                debris.tile,
                debris.shade,
                debris.repeats[0],
                debris.repeats[1],
                debris.angle,
                debris.xvel,
                debris.zvel,
                debris.owner,
                crate::stat::STAT_MISC,
            );
            if index >= 0 {
                let shard = &mut sprites[index as usize];
                shard.yvel = if debris.blimp_pickup_index >= 0 {
                    BLIMP_WEAPON_SPRITES
                        [debris.blimp_pickup_index as usize % BLIMP_WEAPON_SPRITES.len()]
                } else {
                    -1
                };
                shard.pal = debris.palette;
            }
        }
        for guts in guts_bursts {
            let index = crate::shoot::egs(
                lists,
                sprites,
                picnum,
                temps,
                Some(script),
                guts.sector,
                guts.position[0],
                guts.position[1],
                guts.position[2],
                guts.tile,
                -32,
                guts.repeats[0],
                guts.repeats[1],
                guts.angle,
                guts.xvel,
                guts.zvel,
                guts.owner,
                crate::stat::STAT_MISC,
            );
            if index >= 0 && guts.palette == 6 {
                sprites[index as usize].pal = 6;
            }
        }
        for (tile, count) in item_sprays {
            for _ in 0..count.max(0) {
                let parent = sprites[iu];
                // Chocolate `lotsofmoney`/`lotsofmail`/`lotsofpaper`: `EGS(…, z-(TRAND%(47<<8)), …,
                // TRAND&2047, …)` — right-to-left arguments: the angle draws before z.
                let angle = (krand_step(randomseed) & 2047) as i16;
                let z_offset = krand_step(randomseed) % (47 << 8);
                let j = crate::shoot::egs(
                    lists,
                    sprites,
                    picnum,
                    temps,
                    Some(script),
                    parent.sectnum,
                    parent.x,
                    parent.y,
                    parent.z.wrapping_sub(z_offset),
                    tile,
                    -32,
                    8,
                    8,
                    angle,
                    0,
                    0,
                    0,
                    crate::stat::STAT_MISC,
                );
                if j >= 0 {
                    sprites[j as usize].cstat = (krand_step(randomseed) & 12) as u16;
                } else {
                    // Chocolate evaluates all three TRANDs for each money
                    // sprite request even if its fixed sprite pool is full.
                    let _ = krand_step(randomseed);
                }
            }
        }
        for pn in spawns {
            if pn == BLOOD {
                let _ = con_spawn_blood(lists, sprites, picnum, temps, Some(script), i);
            } else if pn == EXPLOSION2 {
                spawn_explosion2_from(
                    lists, sprites, sectors, walls, picnum, temps, script, randomseed, iu,
                );
            } else {
                let _ = crate::spawn_rt::spawn_from(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum,
                    temps,
                    tile_meta,
                    script,
                    randomseed,
                    i,
                    pn,
                    Some(decal_queue),
                );
            }
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
            } else {
                let _ = crate::shoot::shoot_enemy(
                    lists, sprites, sectors, walls, picnum, temps, tile_meta, script, players,
                    randomseed, i, pn,
                );
            }
        }
        if stop == ConStop::KillIt {
            let _ = lists.deletesprite(i);
        } else {
            let (px, py) = players
                .get(pidx)
                .map(|p| (p.posx, p.posy))
                .unwrap_or((temps[iu].lastvx, temps[iu].lastvy));
            let old_sect = sprites[iu].sectnum;
            let single_player = players.len() < 2;
            // The mover works on a copy so the clip snapshot (taken on first
            // use, as before the move) can borrow the untouched array.
            let mut me = sprites[iu];
            let mut clip_source = ClipSource {
                view: &mut clip_view,
                sprites,
                mover: iu,
            };
            actor_con_move(
                lists,
                &mut me,
                &mut clip_source,
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
                dist,
                players.get_mut(pidx),
                single_player,
                i,
            );
            sprites[iu] = me;
            // `clipmove` updates the actor's destination sector in the
            // temporary `dasectnum`; Chocolate only links it into that
            // sector after stayput/boss/water rejection has completed.
            // Room stores the destination directly on SpriteT, so synchronize
            // the Build sector lists here after the movement routine returns.
            if sprites[iu].sectnum != old_sect {
                let _ = lists.changespritesect(i, sprites[iu].sectnum);
            }
        }
        i = next;
    }
    move_musicandsfx(
        lists,
        sprites,
        sectors,
        players,
        script,
        global_random,
        temps,
    );
}

/// One sprite slot of the CON sight/aim view (`ifcansee`, `ifcanshoottarget`).
fn cansee_hit(
    sprite_index: usize,
    sprite: &SpriteT,
    lists: &SpriteLists,
    tile_meta: &[TileZMeta],
) -> SpriteHit {
    let active = lists.sprite_stat[sprite_index] != MAXSTATUS as i16;
    let cstat = if active { sprite.cstat } else { 0 };
    let meta = tile_meta.get(sprite.picnum.max(0) as usize);
    let face = active && sprite.cstat & 48 == 0;
    let face_span = if face {
        meta.map_or(0, |meta| i32::from(meta.width) * i32::from(sprite.xrepeat))
    } else {
        0
    };
    let face_height = if face {
        meta.map_or(0, |meta| {
            (i32::from(meta.height) * i32::from(sprite.yrepeat)) << 2
        })
    } else {
        0
    };
    SpriteHit {
        x: sprite.x,
        y: sprite.y,
        z: sprite.z,
        sectnum: sprite.sectnum,
        clipdist: i32::from(sprite.clipdist) << 2,
        cstat,
        face_span,
        face_height,
    }
}

/// Which per-slot views an [`ActorPassView`] keeps.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewKind {
    /// CON sight/aim rays: [`cansee_hit`] plus each slot's picnum.
    Cansee,
    /// Actor movement clipping: Z clips with raw `cstat`.
    Clip,
}

/// Index-aligned views of every sprite slot used during the actor pass.
///
/// The pass used to rebuild these for all `MAXSPRITES` slots before every
/// actor (two `O(MAXSPRITES)` copies per actor per tic). Each refresh now
/// recomputes, with the same per-slot formulas, every slot that is linked in
/// a status list now or was at the previous refresh; a slot unlinked at
/// both refreshes is never written by game code, so the views equal a full
/// rebuild. `ROOM_DUKE_CHECK_VIEWS=1` compares against a full rebuild on
/// every refresh (debug aid).
struct ActorPassView {
    kind: ViewKind,
    hits: Vec<SpriteHit>,
    picnums: Vec<i16>,
    clips: Vec<SpriteZClip>,
    /// Sprite and linked state each slot's entry was computed from.
    source: Vec<(SpriteT, bool)>,
    /// Slots linked at the previous refresh.
    live: Vec<usize>,
    /// Slot whose clip `cstat` was cleared by [`Self::exclude`].
    excluded: Option<usize>,
    check: bool,
}

impl ActorPassView {
    fn new(
        kind: ViewKind,
        sprites: &[SpriteT],
        lists: &SpriteLists,
        tile_meta: &[TileZMeta],
    ) -> Self {
        let mut view = Self::empty(kind);
        view.check = std::env::var_os("ROOM_DUKE_CHECK_VIEWS").is_some();
        view.rebuild(sprites, lists, tile_meta);
        view
    }

    fn empty(kind: ViewKind) -> Self {
        Self {
            kind,
            hits: Vec::new(),
            picnums: Vec::new(),
            clips: Vec::new(),
            source: Vec::new(),
            live: Vec::new(),
            excluded: None,
            check: false,
        }
    }

    /// The moving actor does not clip against itself: clear its clip `cstat`
    /// until the next refresh.
    fn exclude(&mut self, k: usize) {
        if let Some(clip) = self.clips.get_mut(k) {
            clip.cstat = 0;
            self.excluded = Some(k);
        }
    }

    fn slot(
        &mut self,
        k: usize,
        sprites: &[SpriteT],
        lists: &SpriteLists,
        tile_meta: &[TileZMeta],
    ) {
        let s = &sprites[k];
        match self.kind {
            ViewKind::Cansee => {
                self.hits[k] = cansee_hit(k, s, lists, tile_meta);
                self.picnums[k] = s.picnum;
            }
            ViewKind::Clip => self.clips[k] = sprite_z_clip(s, tile_meta),
        }
    }

    fn rebuild(&mut self, sprites: &[SpriteT], lists: &SpriteLists, tile_meta: &[TileZMeta]) {
        match self.kind {
            ViewKind::Cansee => {
                self.hits = sprites
                    .iter()
                    .enumerate()
                    .map(|(k, s)| cansee_hit(k, s, lists, tile_meta))
                    .collect();
                self.picnums = sprites.iter().map(|s| s.picnum).collect();
            }
            ViewKind::Clip => {
                self.clips = sprites
                    .iter()
                    .map(|s| sprite_z_clip(s, tile_meta))
                    .collect();
            }
        }
        self.source = sprites
            .iter()
            .enumerate()
            .map(|(k, s)| (*s, slot_linked(lists, k)))
            .collect();
        self.live = linked_slots(lists, sprites.len());
        self.excluded = None;
    }

    /// Bring the view up to date with the current sprites.
    fn refresh(&mut self, sprites: &[SpriteT], lists: &SpriteLists, tile_meta: &[TileZMeta]) {
        if self.source.len() != sprites.len() || lists.sprite_stat.len() < sprites.len() {
            self.rebuild(sprites, lists, tile_meta);
            return;
        }
        if let Some(k) = self.excluded.take() {
            self.clips[k] = sprite_z_clip(&self.source[k].0, tile_meta);
        }
        let now = linked_slots(lists, sprites.len());
        let previous = std::mem::take(&mut self.live);
        for &k in previous.iter().chain(now.iter()) {
            let key = (sprites[k], slot_linked(lists, k));
            if self.source[k] != key {
                self.slot(k, sprites, lists, tile_meta);
                self.source[k] = key;
            }
        }
        self.live = now;
        if self.check {
            let mut full = Self::empty(self.kind);
            full.rebuild(sprites, lists, tile_meta);
            assert!(
                full.hits == self.hits,
                "actor-pass hit view diverged from a full rebuild"
            );
            if self.kind == ViewKind::Cansee {
                // Unlinked slots' picnums are never read (only hit sprites).
                for &k in &self.live {
                    assert_eq!(full.picnums[k], self.picnums[k], "picnum view diverged");
                }
            } else {
                assert!(
                    full.clips == self.clips,
                    "actor-pass clip view diverged from a full rebuild"
                );
            }
        }
    }
}

fn slot_linked(lists: &SpriteLists, k: usize) -> bool {
    lists
        .sprite_stat
        .get(k)
        .is_some_and(|&stat| stat != MAXSTATUS as i16)
}

/// The actor pass sight view for one actor's CON run, built on first use.
struct LazySight<'r, 's, 'l> {
    view: std::cell::RefCell<&'r mut ActorPassView>,
    built: std::cell::Cell<bool>,
    sprites: &'r std::cell::RefCell<&'s mut [SpriteT]>,
    lists: &'r std::cell::RefCell<&'l mut SpriteLists>,
    tile_meta: &'r [TileZMeta],
}

impl LazySight<'_, '_, '_> {
    /// Snapshot the sprites now (idempotent per actor run).
    fn build(&self) {
        if self.built.get() {
            return;
        }
        let sprites = self.sprites.borrow();
        let lists = self.lists.borrow();
        self.view
            .borrow_mut()
            .refresh(&sprites, &lists, self.tile_meta);
        self.built.set(true);
    }
}

impl CanseeLazy for LazySight<'_, '_, '_> {
    fn with(&self, f: &mut dyn FnMut(&[SpriteHit], &[i16])) {
        self.build();
        let view = self.view.borrow();
        f(&view.hits, &view.picnums);
    }
}

/// Lazily refreshed CLIPMASK0 sprite clips for one actor's move: most actors
/// return before `movesprite`, so the snapshot is only built when clipping.
struct ClipSource<'a> {
    view: &'a mut ActorPassView,
    sprites: &'a [SpriteT],
    mover: usize,
}

impl ClipSource<'_> {
    fn clips(&mut self, lists: &SpriteLists, tile_meta: &[TileZMeta]) -> &[SpriteZClip] {
        self.view.refresh(self.sprites, lists, tile_meta);
        // The moving actor does not clip against itself.
        self.view.exclude(self.mover);
        &self.view.clips
    }
}

/// Every sprite slot linked into a status list (`0..MAXSTATUS`), bounded.
fn linked_slots(lists: &SpriteLists, len: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let statuses = lists.headspritestat.len().min(MAXSTATUS);
    for stat in 0..statuses {
        let mut i = lists.headspritestat[stat];
        let mut guard = 0usize;
        while i >= 0 && guard <= len {
            let k = i as usize;
            if k >= len || k >= lists.nextspritestat.len() {
                break;
            }
            out.push(k);
            i = lists.nextspritestat[k];
            guard += 1;
        }
    }
    out
}

/// Chocolate `actors.c::moveactors` GREENSLIME frozen-state branch. The slime's
/// non-frozen movement/attack state remains in its CON path; this owns the C
/// branch entered by a Freezer hit and the subsequent frozen lifecycle.
#[allow(clippy::too_many_arguments)]
fn tick_frozen_slime(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    sector_z: &[SectorZ],
    wall_xy: &[WallXY],
    picnum: &mut Vec<i16>,
    temps: &mut Vec<ActorConTemps>,
    script: &ScriptImage,
    players: &mut [Player],
    randomseed: &mut i32,
    index: usize,
) -> bool {
    const GLASS_BREAKING: i16 = 19;
    const SLIM_DYING: i16 = 149;
    const SOMETHINGFROZE: i16 = 303;
    let Some(sprite) = sprites.get(index).copied() else {
        return false;
    };
    if !(GREENSLIME..=GREENSLIME + 7).contains(&sprite.picnum) {
        return false;
    }
    temps[index].g_t[1] = temps[index].g_t[1].wrapping_add(128);
    let Some(sector) = usize::try_from(sprite.sectnum)
        .ok()
        .and_then(|sector| sectors.get(sector))
    else {
        let _ = lists.deletesprite(index as i16);
        sprites[index].statnum = MAXSTATUS as i16;
        return true;
    };
    if sector.floorstat & 1 != 0 {
        let _ = lists.deletesprite(index as i16);
        sprites[index].statnum = MAXSTATUS as i16;
        return true;
    }
    if temps[index].g_t[0] != -5 {
        if temps[index].hit_extra < 0 || temps[index].hit_picnum != FREEZEBLAST {
            return false;
        }
        sprites[index].extra = sprites[index].extra.saturating_sub(temps[index].hit_extra);
        let (player_index, _) = findplayer_dist(&sprite, players);
        if let Some(player) = players.get_mut(player_index) {
            player.actors_killed = player.actors_killed.wrapping_add(1);
        }
        temps[index]
            .pending_actor_sounds
            .extend([SLIM_DYING, SOMETHINGFROZE]);
        temps[index].hit_extra = -1;
        temps[index].g_t[0] = -5;
        temps[index].g_t[3] = 0;
        return true;
    }

    temps[index].g_t[3] = temps[index].g_t[3].saturating_add(1);
    if temps[index].g_t[3] > 280 {
        sprites[index].pal = 0;
        temps[index].g_t[0] = 0;
        return true;
    }

    let (_, floor_z) = getzsofslope(sprite.sectnum, sprite.x, sprite.y, sector_z, wall_xy);
    let gravity = if sector.floorstat & 1 != 0 {
        0
    } else if sector.lotag == 2 {
        DUKE_GC / 6
    } else {
        DUKE_GC
    };
    if sprites[index].z < floor_z - (4 << 8) {
        if sector.lotag == 2 && sprites[index].zvel > 3122 {
            sprites[index].zvel = 3144;
        }
        sprites[index].zvel = i32::from(sprites[index].zvel)
            .saturating_add(gravity)
            .min(6144) as i16;
        sprites[index].z = sprites[index]
            .z
            .wrapping_add(i32::from(sprites[index].zvel));
    }
    if sprites[index].z >= floor_z - (4 << 8) {
        sprites[index].z = floor_z - (4 << 8);
        sprites[index].zvel = 0;
    }
    sprites[index].cstat = 257;
    sprites[index].picnum = GREENSLIME + 2;
    sprites[index].extra = 1;
    sprites[index].pal = 1;
    if index >= picnum.len() {
        picnum.resize(index + 1, 0);
    }
    picnum[index] = GREENSLIME + 2;

    if temps[index].hit_extra >= 0 {
        sprites[index].extra = sprites[index].extra.saturating_sub(temps[index].hit_extra);
        if temps[index].hit_picnum == FREEZEBLAST {
            temps[index].hit_extra = -1;
            return true;
        }
        let frozen = sprites[index];
        let glass_pieces = script
            .labels
            .get("GLASSPIECES")
            .filter(|label| !label.is_ptr)
            .map_or(1031, |label| label.value as i16);
        for shard_index in (0..=16).rev() {
            let tile = glass_pieces + (shard_index % 3);
            // Chocolate `EGS(…, TRAND&2047, 32+(TRAND&63), 1024-(TRAND&1023), …)`:
            // right-to-left arguments, zvel draws first.
            let zvel = (1024 - (krand_step(randomseed) & 1023)) as i16;
            let xvel = (32 + (krand_step(randomseed) & 63)) as i16;
            let angle = (krand_step(randomseed) & 2047) as i16;
            let shard = crate::shoot::egs(
                lists,
                sprites,
                picnum,
                temps,
                Some(script),
                frozen.sectnum,
                frozen.x,
                frozen.y,
                frozen.z,
                tile,
                -32,
                36,
                36,
                angle,
                xvel,
                zvel,
                index as i16,
                crate::stat::STAT_MISC,
            );
            if shard >= 0 {
                sprites[shard as usize].pal = 1;
            }
        }
        temps[index].pending_actor_sounds.push(GLASS_BREAKING);
        let _ = lists.deletesprite(index as i16);
        sprites[index].statnum = MAXSTATUS as i16;
        return true;
    }

    let (player_index, distance) = findplayer_dist(&sprites[index], players);
    if distance < 1024 {
        if let Some(player) = players.get_mut(player_index)
            && player.quick_kick == 0
        {
            let target_angle = getangle(
                sprites[index].x.wrapping_sub(player.posx),
                sprites[index].y.wrapping_sub(player.posy),
            );
            let delta = (target_angle - i32::from(player.ang) + 1024).rem_euclid(2048) - 1024;
            if (-128..128).contains(&delta) {
                player.quick_kick = 14;
            }
        }
    }
    true
}

/// Chocolate `actors.c::moveactors` ambient/music sprite path. Ambient sound
/// defs with `m&2` loop while the local player is within the marker's hitag
/// radius; `m&16` defs are played globally at randomized sector intervals.
fn move_musicandsfx(
    lists: &SpriteLists,
    sprites: &[SpriteT],
    sectors: &[SectorT],
    players: &mut [Player],
    script: &ScriptImage,
    global_random: i32,
    temps: &mut Vec<ActorConTemps>,
) {
    const MUSICANDSFX: i16 = 5;
    let Some(player) = players.first_mut() else {
        return;
    };
    let player_body = sprites.get(player.sprite_i.max(0) as usize).copied();
    let mut i = lists.headspritestat[crate::stat::STAT_FX as usize];
    while i >= 0 {
        let iu = i as usize;
        i = lists.nextspritestat[iu];
        let Some(marker) = sprites.get(iu).copied() else {
            continue;
        };
        if marker.picnum != MUSICANDSFX || marker.lotag < 0 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(marker.sectnum) else {
            continue;
        };
        let Some(sector) = sectors.get(sector_index) else {
            continue;
        };
        if temps.len() <= iu {
            temps.resize_with(iu + 1, ActorConTemps::default);
        }
        let temp = &mut temps[iu];
        // Chocolate T2 watches SoundToggle; Room has no runtime sound toggle,
        // so model the enabled state as stable and perform the first reset.
        if temp.g_t[1] != 1 {
            temp.g_t[1] = 1;
            temp.g_t[0] = 0;
        }
        let sound_id = marker.lotag as usize;
        let sound_flags = script
            .sounds
            .get(sound_id)
            .and_then(Option::as_ref)
            .map_or(0, |s| s.m);
        // MUSICANDSFX T4 is otherwise unused in Atomic; `callsound` reads
        // the same soundm flags while operating sector animations.
        temp.g_t[3] = sound_flags;
        if marker.lotag < 999 && sector.lotag < 9 && sector.floorz != sector.ceilingz {
            let distance = player_body
                .map(|body| {
                    find_distance_3d(
                        body.x.wrapping_sub(marker.x),
                        body.y.wrapping_sub(marker.y),
                        body.z.wrapping_sub(marker.z) >> 4,
                    )
                })
                .unwrap_or(i32::MAX);
            let radius = i32::from(marker.hitag.max(0));
            if sound_flags & 2 != 0 {
                if distance < radius && temp.g_t[0] == 0 {
                    temp.pending_actor_sounds_loop.push(marker.lotag);
                    temp.g_t[0] = 1;
                } else if distance >= radius && temp.g_t[0] == 1 {
                    temp.g_t[0] = 0;
                    temp.pending_stop_source_sound = true;
                }
            }
            if sound_flags & 16 != 0 {
                if temp.g_t[4] > 0 {
                    temp.g_t[4] -= 1;
                } else if player.cursectnum == marker.sectnum {
                    let range = i32::from(marker.hitag.max(0)).saturating_add(1) as u32;
                    temp.pending_global_sounds.push(
                        marker
                            .lotag
                            .saturating_add(((global_random as u32) % range) as i16),
                    );
                    temp.g_t[4] = 26 * 40 + global_random % (26 * 40);
                }
            }
        }
    }
}

/// Chocolate `FindDistance3D` used by `sector.c::dist` for ambient radii.
fn find_distance_3d(mut x: i32, mut y: i32, mut z: i32) -> i32 {
    x = x.saturating_abs();
    y = y.saturating_abs();
    z = z.saturating_abs();
    if x < y {
        std::mem::swap(&mut x, &mut y);
    }
    if x < z {
        std::mem::swap(&mut x, &mut z);
    }
    let others = y.saturating_add(z);
    x.saturating_sub(x >> 4)
        .saturating_add(others >> 2)
        .saturating_add(others >> 3)
}

/// Chocolate `actors.c::moveactors` HEAVYHBOMB path: fall/bounce, remote or
/// impact detonation, radius damage and short-lived explosion sprites.
fn hide_pipebomb_for_respawn(
    index: i16,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum: &mut Vec<i16>,
    temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    delay: i32,
) {
    let Some(source) = usize::try_from(index)
        .ok()
        .and_then(|index| sprites.get(index).copied())
    else {
        return;
    };
    if let Some(script) = script {
        let _ = crate::shoot::egs(
            lists,
            sprites,
            picnum,
            temps,
            Some(script),
            source.sectnum,
            source.x,
            source.y,
            source.z,
            crate::spawn::RESPAWNMARKERRED,
            -32,
            9,
            9,
            source.ang,
            0,
            0,
            index,
            STAT_ACTOR,
        );
    }
    if let Some(sprite) = sprites.get_mut(index as usize) {
        sprite.cstat = 32768;
        sprite.yrepeat = 9;
    }
    if let Some(temp) = temps.get_mut(index as usize) {
        temp.g_t[2] = delay;
        temp.g_t[3] = 1;
        temp.g_t[4] = 21;
    }
}

fn move_pipebombs(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    tile_meta: &[TileZMeta],
    picnum: &mut Vec<i16>,
    players: &mut [Player],
    randomseed: &mut i32,
    temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    respawn_items: bool,
) {
    let heavybomb_tile = crate::shoot::script_tile_or(script, "HEAVYHBOMB", HEAVYHBOMB);
    let morter_tile = crate::shoot::script_tile_or(script, "MORTER", 1650);
    let bouncemine_tile = crate::shoot::script_tile_or(script, "BOUNCEMINE", 940);
    let respawn_item_time = script
        .and_then(|image| image.gamestartup.get(5).copied())
        .unwrap_or(0)
        .max(0);
    let mut order = Vec::new();
    let mut i = lists.headspritestat[STAT_ACTOR as usize];
    while i >= 0 {
        order.push(i);
        i = lists.nextspritestat[i as usize];
    }
    let sec_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let clips: Vec<SpriteZClip> = sprites
        .iter()
        .map(|s| sprite_z_clip(s, tile_meta))
        .collect();
    for i in order {
        let iu = i as usize;
        if lists.sprite_stat.get(iu).copied() != Some(STAT_ACTOR)
            || sprites.get(iu).is_none_or(|s| s.sectnum < 0)
        {
            continue;
        }
        if matches!(
            sprites[iu].picnum,
            crate::spawn::RESPAWNMARKERRED
                | crate::spawn::RESPAWNMARKERYELLOW
                | crate::spawn::RESPAWNMARKERGREEN
        ) {
            if temps.len() <= iu {
                temps.resize_with(iu + 1, ActorConTemps::default);
            }
            temps[iu].g_t[0] = temps[iu].g_t[0].saturating_add(1);
            let age = temps[iu].g_t[0];
            if respawn_item_time <= 0 || age > respawn_item_time {
                let _ = lists.deletesprite(i);
            } else {
                let yellow_at = respawn_item_time >> 1;
                let green_after = yellow_at + (respawn_item_time >> 2);
                let marker = if age >= yellow_at && age < green_after {
                    Some(crate::spawn::RESPAWNMARKERYELLOW)
                } else if age > green_after {
                    Some(crate::spawn::RESPAWNMARKERGREEN)
                } else {
                    None
                };
                if let Some(marker) = marker {
                    sprites[iu].picnum = marker;
                    if let Some(tile) = picnum.get_mut(iu) {
                        *tile = marker;
                    }
                }
                // Chocolate runs makeitfall for all three marker states after
                // the color/timer logic, using the live actor sprite list.
                let mut me = sprites[iu];
                crate::spawn::makeitfall(
                    &mut me, sectors, walls, sprites, tile_meta, DUKE_GC, true,
                );
                sprites[iu] = me;
            }
            continue;
        }
        // Chocolate shares this C case between HEAVYHBOMB and the enemy
        // projectiles MORTER (Battlelord) and BOUNCEMINE.
        let pic_now = sprites[iu].picnum;
        let is_morter = pic_now == morter_tile;
        let is_bouncemine = pic_now == bouncemine_tile;
        if pic_now != heavybomb_tile && !is_morter && !is_bouncemine {
            continue;
        }
        if iu >= temps.len() {
            temps.resize_with(iu + 1, ActorConTemps::default);
        }
        if (is_morter || is_bouncemine)
            && let Some(script) = script
            && let Some(trail) = script
                .labels
                .get("FRAMEEFFECT1")
                .map(|label| label.value as i16)
        {
            let j = crate::spawn_rt::spawn_from(
                lists, sprites, sectors, walls, picnum, temps, tile_meta, script, randomseed, i,
                trail, None,
            );
            if j >= 0 {
                let ju = j as usize;
                if temps.len() <= ju {
                    temps.resize_with(ju + 1, ActorConTemps::default);
                }
                temps[ju].g_t[0] = 3;
            }
        }
        if usize::try_from(sprites[iu].sectnum).unwrap_or(usize::MAX) >= sectors.len() {
            let _ = lists.deletesprite(i);
            continue;
        }
        if sprites[iu].cstat & 32768 != 0 {
            temps[iu].g_t[2] = temps[iu].g_t[2].saturating_sub(1);
            if temps[iu].g_t[2] <= 0 {
                let hidden = sprites[iu];
                if let Some(sound) = script
                    .and_then(|image| image.labels.get("TELEPORTER"))
                    .and_then(|label| u16::try_from(label.value).ok())
                {
                    temps[iu].pending_actor_sounds.push(sound as i16);
                }
                if let Some(script) = script {
                    let _ = crate::shoot::egs(
                        lists,
                        sprites,
                        picnum,
                        temps,
                        Some(script),
                        hidden.sectnum,
                        hidden.x,
                        hidden.y,
                        hidden.z,
                        crate::spawn::TRANSPORTERSTAR,
                        -32,
                        8,
                        8,
                        hidden.ang,
                        0,
                        0,
                        i,
                        crate::stat::STAT_MISC,
                    );
                }
                sprites[iu].cstat = 257;
                temps[iu].g_t[0] = 0;
                temps[iu].g_t[3] = 0;
                temps[iu].g_t[4] = 0;
            }
            continue;
        }
        temps[iu].g_t[0] += 1;
        let mut bounced = false;
        let move_hit;
        {
            let s = &mut sprites[iu];
            let sect = s.sectnum as usize;
            // `makeitfall` plus Chocolate's pipebomb floor bounce rule.
            let (ceilz, floorz) = getzsofslope(s.sectnum, s.x, s.y, &sec_z, &wall_xy);
            if is_bouncemine {
                // Chocolate: BOUNCEMINE skips makeitfall and the floor bounce.
            } else if s.z < floorz - (4 << 8) {
                let gravity = if sectors[sect].lotag == 2 {
                    DUKE_GC / 6
                } else {
                    DUKE_GC
                };
                s.zvel = i32::from(s.zvel).saturating_add(gravity).min(6144) as i16;
            } else if sectors[sect].lotag != 1 && s.yvel < 3 {
                s.z = floorz - (4 << 8);
                s.zvel = (-((4 - i32::from(s.yvel)) << 8)) as i16;
                if sectors[sect].lotag == 2 {
                    s.zvel >>= 2;
                }
                s.yvel += 1;
                bounced = true;
            } else {
                s.z = floorz - (4 << 8);
                s.zvel = 0;
            }
            if !is_bouncemine && s.z < ceilz {
                s.z = ceilz + (3 << 8);
                s.zvel = 0;
            }
            let (sin, cos) = sincos_q14(i32::from(s.ang));
            let dx = (((i32::from(s.xvel) * cos) >> 14) * TICSPERFRAME) >> 3;
            let dy = (((i32::from(s.xvel) * sin) >> 14) * TICSPERFRAME) >> 3;
            let mut newsect = s.sectnum;
            let hit = clipmove_xy_at_z_sprites_with_lists(
                &mut s.x,
                &mut s.y,
                s.z,
                &mut newsect,
                dx,
                dy,
                36,
                4 << 8,
                4 << 8,
                &sec_walls,
                &wall_xy,
                Some(&sec_z),
                &clips,
                1,
                Some(lists),
            );
            if newsect >= 0 && (newsect as usize) < sectors.len() {
                s.sectnum = newsect;
            }
            s.z = s.z.saturating_add((i32::from(s.zvel) * TICSPERFRAME) >> 3);
            move_hit = hit;
            if s.xvel > 0 {
                s.xvel = s.xvel.saturating_sub(5).max(0);
            }
            if (hit & 0xc000) == 0x8000 && !(is_morter || is_bouncemine) {
                let wi = (hit & 0x3fff) as usize;
                if let Some(w) = walls.get(wi) {
                    let p2 = usize::try_from(w.point2).unwrap_or(usize::MAX);
                    if let Some(end) = walls.get(p2) {
                        s.ang = ((getangle(end.x - w.x, end.y - w.y) * 2 - i32::from(s.ang)) & 2047)
                            as i16;
                        s.xvel >>= 1;
                    }
                }
            }
        }
        if bounced {
            temps[iu].pending_actor_sounds.push(13);
        }
        let (nearest_player, nearest_dist) = findplayer_dist(&sprites[iu], players);
        if nearest_dist < 1220 {
            sprites[iu].cstat &= !257;
        } else {
            sprites[iu].cstat |= 257;
        }
        if temps[iu].hit_extra >= 0 && temps[iu].g_t[3] == 0 {
            temps[iu].hit_extra = -1;
            temps[iu].g_t[3] = 1;
            temps[iu].g_t[4] = 0;
            sprites[iu].xvel = 0;
        }
        let current_sector = usize::try_from(sprites[iu].sectnum).unwrap_or(usize::MAX);
        if current_sector < sectors.len()
            && sectors[current_sector].lotag == 1
            && sprites[iu].zvel == 0
        {
            sprites[iu].z = sprites[iu].z.saturating_add(32 << 8);
            if temps[iu].g_t[5] == 0 {
                temps[iu].g_t[5] = 1;
                let splash = sprites[iu];
                if let Some(script) = script {
                    let _ = crate::shoot::egs(
                        lists,
                        sprites,
                        picnum,
                        temps,
                        Some(script),
                        splash.sectnum,
                        splash.x,
                        splash.y,
                        splash.z,
                        WATERSPLASH2,
                        -32,
                        8,
                        8,
                        splash.ang,
                        0,
                        0,
                        i,
                        crate::stat::STAT_MISC,
                    );
                }
            }
        } else {
            temps[iu].g_t[5] = 0;
        }
        // MORTER / BOUNCEMINE detonate on any contact or next to a player.
        if temps[iu].g_t[3] == 0
            && (is_morter || is_bouncemine)
            && (move_hit != 0 || nearest_dist < 844)
        {
            temps[iu].g_t[3] = 1;
            temps[iu].g_t[4] = 0;
            sprites[iu].xvel = 0;
        }
        let owner = sprites[iu].owner;
        if players.iter().any(|p| p.sprite_i == owner && !p.hbomb_on) && temps[iu].g_t[3] == 0 {
            temps[iu].g_t[3] = 1;
            temps[iu].g_t[4] = 0;
            sprites[iu].xvel = 0;
        }
        if temps[iu].g_t[3] == 1 {
            temps[iu].g_t[4] += 1;
            if temps[iu].g_t[4] == 2 {
                let center = sprites[iu];
                let pipebomb_radius = script
                    .and_then(|image| {
                        let index = if image.con_version == crate::con::ConVersion::V15 {
                            9
                        } else {
                            8
                        };
                        image.gamestartup.get(index).copied()
                    })
                    .unwrap_or(PIPEBOMB_BLAST_RADIUS_FALLBACK)
                    .max(1);
                // gamestartup: …pipebomb, shrinker, tripbomb, morter, bouncemine.
                let blast_offset = if is_morter {
                    3
                } else if is_bouncemine {
                    4
                } else {
                    0
                };
                let pipebomb_radius = if blast_offset == 0 {
                    pipebomb_radius
                } else {
                    script
                        .and_then(|image| {
                            let base = if image.con_version == crate::con::ConVersion::V15 {
                                9
                            } else {
                                8
                            };
                            image.gamestartup.get(base + blast_offset).copied()
                        })
                        .unwrap_or(pipebomb_radius)
                        .max(1)
                };
                crate::weapons_move::apply_rpg_blast(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    temps,
                    players,
                    randomseed,
                    i32::from(center.extra.max(1)),
                    pipebomb_radius,
                    RADIUSEXPLOSION_DAMAGE,
                    None,
                    center,
                    i,
                    picnum,
                    script,
                    tile_meta,
                    script
                        .map(|image| image.actortype.as_slice())
                        .unwrap_or(&[]),
                    center.x,
                    center.y,
                    center.z,
                    center.sectnum,
                    center.owner,
                );
                for _ in 0..8 {
                    let _ = krand_step(randomseed);
                }
                temps[iu].pending_actor_sounds.push(14);
                if let Some(script) = script {
                    let _ = crate::shoot::egs(
                        lists,
                        sprites,
                        picnum,
                        temps,
                        Some(script),
                        center.sectnum,
                        center.x,
                        center.y,
                        center.z,
                        EXPLOSION2,
                        -32,
                        32,
                        32,
                        center.ang,
                        0,
                        0,
                        center.owner,
                        crate::stat::STAT_MISC,
                    );
                }
                sprites[iu].yrepeat = 0;
            }
            if temps[iu].g_t[4] > 20 {
                if respawn_items && sprites[iu].owner == i && respawn_item_time > 0 {
                    hide_pipebomb_for_respawn(
                        i,
                        lists,
                        sprites,
                        picnum,
                        temps,
                        script,
                        respawn_item_time,
                    );
                } else {
                    let _ = lists.deletesprite(i);
                }
                continue;
            }
        } else if pic_now == heavybomb_tile
            && nearest_dist < 788
            && temps[iu].g_t[0] > 7
            && sprites[iu].xvel == 0
        {
            let maximum = script
                .and_then(|image| {
                    let index = if image.con_version == crate::con::ConVersion::V15 {
                        19
                    } else {
                        18
                    };
                    image.gamestartup.get(index).copied()
                })
                .unwrap_or_else(|| i32::from(crate::weapons::max_ammo_amount(5)))
                .clamp(0, i32::from(i16::MAX)) as i16;
            let can_pickup = players.get(nearest_player).is_some_and(|player| {
                player.ammo[5] < maximum
                    && cansee(
                        sprites[iu].x,
                        sprites[iu].y,
                        sprites[iu].z - (8 << 8),
                        sprites[iu].sectnum,
                        player.posx,
                        player.posy,
                        player.posz,
                        player.cursectnum,
                        &sec_z,
                        &wall_xy,
                    )
            });
            if can_pickup {
                let Some(player) = players.get_mut(nearest_player) else {
                    continue;
                };
                player.ammo[5] = player.ammo[5].saturating_add(1).min(maximum);
                if !player.gotweapon[5] || sprites[iu].owner == player.sprite_i {
                    crate::weapons::addweapon(player, 5);
                }
                if let Some(temp) = temps.get_mut(iu) {
                    if let Some(id) = script
                        .and_then(|image| image.labels.get("DUKE_GET"))
                        .and_then(|label| u16::try_from(label.value).ok())
                    {
                        temp.pending_actor_sounds.push(id as i16);
                    }
                }
                if respawn_items && sprites[iu].owner == i && respawn_item_time > 0 {
                    hide_pipebomb_for_respawn(
                        i,
                        lists,
                        sprites,
                        picnum,
                        temps,
                        script,
                        respawn_item_time,
                    );
                } else {
                    let _ = lists.deletesprite(i);
                }
            }
        }
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
            &mut [],
            &mut [],
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

thread_local! {
    /// Set when an actor requested Chocolate's global `earthquaketime = 16`.
    static PENDING_EARTHQUAKE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Take (and clear) a pending actor earthquake request.
pub(crate) fn take_pending_earthquake() -> bool {
    PENDING_EARTHQUAKE.with(|q| q.replace(false))
}

/// Chocolate `moveactors` `case HELECOPT: case DUKECAR:` — the E1L1 intro
/// shuttle descends, trails EXPLOSION2 every 4 tics, then bursts into 32
/// RANDOMSCRAP shards after 208 tics.
#[allow(clippy::too_many_arguments)]
fn move_dukecar(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum: &mut Vec<i16>,
    temps: &mut Vec<ActorConTemps>,
    script: &ScriptImage,
    randomseed: &mut i32,
    iu: usize,
) {
    let i = iu as i16;
    if temps[iu].hit_floorz == 0 && temps[iu].hit_ceilingz == 0 {
        if let Some(sector) = usize::try_from(sprites[iu].sectnum)
            .ok()
            .and_then(|s| sectors.get(s))
        {
            temps[iu].hit_floorz = sector.floorz;
            temps[iu].hit_ceilingz = sector.ceilingz;
        }
    }
    sprites[iu].z = sprites[iu].z.wrapping_add(i32::from(sprites[iu].zvel));
    temps[iu].g_t[0] += 1;
    let t0 = temps[iu].g_t[0];
    if t0 == 4 {
        temps[iu].pending_actor_sounds.push(WAR_AMBIENCE2);
    }
    if t0 > 26 * 8 {
        temps[iu].pending_actor_sounds.push(RPG_EXPLODE);
        // Chocolate `earthquaketime = 16`.
        PENDING_EARTHQUAKE.with(|q| q.set(true));
        let src = sprites[iu];
        for _ in 0..32 {
            // Chocolate `RANDOMSCRAP`: 32-bit gcc evaluates the EGS arguments right
            // to left, so zvel draws first and x last.
            let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
            let xvel = (64 + (krand_step(randomseed) & 63)) as i16;
            let ang = (krand_step(randomseed) & 2047) as i16;
            let pn = SCRAP6 + (krand_step(randomseed) & 15) as i16;
            let z = src.z - (8 << 8) - (krand_step(randomseed) & 8191);
            let y = src.y + (krand_step(randomseed) & 255) - 128;
            let x = src.x + (krand_step(randomseed) & 255) - 128;
            let _ = crate::shoot::egs(
                lists,
                sprites,
                picnum,
                temps,
                Some(script),
                src.sectnum,
                x,
                y,
                z,
                pn,
                -8,
                48,
                48,
                ang,
                xvel,
                zvel,
                i,
                crate::stat::STAT_MISC,
            );
        }
        let _ = lists.deletesprite(i);
        return;
    } else if t0 & 3 == 0 {
        spawn_explosion2_from(
            lists, sprites, sectors, walls, picnum, temps, script, randomseed, iu,
        );
    }
    // Chocolate `ssp(i, CLIPMASK0)` → non-badguy `movesprite`.
    let sp = sprites[iu];
    let (sin, cos) = sincos_q14(i32::from(sp.ang));
    let xvel = i32::from(sp.xvel);
    let dx = (((xvel * cos) >> 14) * TICSPERFRAME) >> 3;
    let dy = (((xvel * sin) >> 14) * TICSPERFRAME) >> 3;
    let sec: Vec<SectorWalls> = sectors.iter().map(SectorT::walls).collect();
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let mut x = sp.x;
    let mut y = sp.y;
    let mut sect = sp.sectnum;
    let retval = clipmove_xy_at_z_sprites_with_lists(
        &mut x,
        &mut y,
        sp.z,
        &mut sect,
        dx,
        dy,
        i32::from(sp.clipdist) << 2,
        4 << 8,
        4 << 8,
        &sec,
        &wxy,
        Some(&sec_z),
        &[],
        1,
        Some(lists),
    );
    let _ = retval;
    sprites[iu].x = x;
    sprites[iu].y = y;
    if sect >= 0 && sect != sp.sectnum {
        sprites[iu].sectnum = sect;
        let _ = lists.changespritesect(i, sect);
    }
    let daz = sprites[iu].z + ((i32::from(sp.zvel) * TICSPERFRAME) >> 3);
    if daz > temps[iu].hit_ceilingz && daz <= temps[iu].hit_floorz {
        sprites[iu].z = daz;
    }
}

/// Chocolate `spawn(j, EXPLOSION2)` for a parent sprite.
#[allow(clippy::too_many_arguments)]
fn spawn_explosion2_from(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum: &mut Vec<i16>,
    temps: &mut Vec<ActorConTemps>,
    script: &ScriptImage,
    randomseed: &mut i32,
    from: usize,
) {
    let src = sprites[from];
    let e = crate::shoot::egs(
        lists,
        sprites,
        picnum,
        temps,
        Some(script),
        src.sectnum,
        src.x,
        src.y,
        src.z,
        EXPLOSION2,
        0,
        0,
        0,
        0,
        0,
        0,
        from as i16,
        STAT_DEFAULT,
    );
    if e < 0 {
        return;
    }
    let eu = e as usize;
    sprites[eu].ang = src.ang;
    sprites[eu].cstat = 128 | ((krand_step(randomseed) & 4) as u16);
    sprites[eu].xrepeat = 48;
    sprites[eu].yrepeat = 48;
    sprites[eu].shade = -127;
    sprites[eu].cstat |= 128;
    if src.sectnum >= 0 && (src.sectnum as usize) < sectors.len() {
        let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
        let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
        let floorz = getzsofslope(src.sectnum, sprites[eu].x, sprites[eu].y, &sec_z, &wxy).1;
        if sprites[eu].z > floorz - (12 << 8) {
            sprites[eu].z = floorz - (12 << 8);
        }
    }
    let _ = lists.changespritestat(e, crate::stat::STAT_MISC);
}
