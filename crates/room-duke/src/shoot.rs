//! Chocolate `shoot` / `aim` (player shotgun path for D6 / DEMO1 tic135).

use room_build_engine::geom::{
    HitScan, SectorWalls, SectorZ, SpriteHit, WallXY, cansee, clipmove_xy, getzsofslope,
    hitscan_full, pushmove, updatesector,
};
use room_build_engine::helpers::{getangle, sincos_q14};
use room_build_engine::numeric::{klabs, krand_step, mulscale};
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::badguy::badguy_pic;
use crate::con::{ActorConTemps, ScriptImage, actor_header_hitag, init_actor_temps_from_script};
use crate::player::{CLIPMASK0, Player, TileZMeta};
use crate::spawn::WATERBUBBLE;
use crate::stat::{STAT_DEFAULT, STAT_EFFECTOR, STAT_MISC, STAT_PROJECTILE};
use crate::weapons::{SHOTGUN_WEAPON, checkavailweapon};

/// Atomic tile numbers used by shotgun FX.
pub const SHOTSPARK1: i16 = 2595;
/// Atomic `FREEZEBLAST` projectile tile.
pub const FREEZEBLAST: i16 = 1641;
/// Atomic `SHRINKSPARK` projectile tile.
pub const SHRINKSPARK: i16 = 1646;
/// Atomic `GROWSPARK` impact sprite tile.
pub const GROWSPARK: i16 = 2448;
const DRONE: i16 = 1880;
const COMMANDER: i16 = 1920;
const GREENSLIME: i16 = 2370;
const ORGANTIC: i16 = 2420;
const ROTATEGUN: i16 = 2360;
pub(crate) const RECON: i16 = 1960;
/// Atomic `CHAINGUN` weapon damage profile.
pub const CHAINGUN: i16 = 2536;
/// Atomic `COOLEXPLOSION1` — OCTABRAIN projectile (`shoot` / STAT_PROJECTILE).
pub const COOLEXPLOSION1: i16 = 1360;
/// Atomic `SPIT` projectile tile used by Lizman shooters.
pub const SPIT: i16 = 1636;
/// Atomic `FIRELASER` projectile tile used by sector shooter effectors.
pub const FIRELASER: i16 = 1625;
pub const SHOTGUN: i16 = 2613;
/// Atomic `KNEE` hit sprite / weapon view tile.
pub const KNEE: i16 = 2521;
pub const SMALLSMOKE: i16 = 2329;
pub const JIBS6: i16 = 2286;
pub const BLOOD: i16 = 1620;
pub const BLOODSPLAT1: i16 = 2296;
pub const BLOODSPLAT2: i16 = 2297;
pub const BLOODSPLAT3: i16 = 2298;
pub const BLOODSPLAT4: i16 = 2299;
pub const APLAYER: i16 = 1405;
/// Atomic `SHOTGUNSHELL` (ejected casing).
pub const SHOTGUNSHELL: i16 = 2535;
/// Atomic `SHELL` (pistol casing).
pub const SHELL: i16 = 2533;
/// Atomic bullet impact decal.
const BULLETHOLE: i16 = 952;
/// Atomic RPG projectile tile (defs.con `RPG`).
pub const RPG_PROJECTILE: i16 = 2605;
/// Atomic `HEAVYHBOMB` (pipebomb projectile and weapon view).
pub const HEAVYHBOMB: i16 = 26;

/// Resolve a CON tile definition while retaining the Atomic tile as the
/// fallback for older or partial script images.
#[must_use]
pub(crate) fn script_tile_or(script: Option<&ScriptImage>, name: &str, fallback: i16) -> i16 {
    script
        .and_then(|image| image.labels.get(name))
        .filter(|label| !label.is_ptr)
        .and_then(|label| i16::try_from(label.value).ok())
        .unwrap_or(fallback)
}

/// Chocolate `CLIPMASK1`.
const CLIPMASK1: u32 = (256 << 16) + 64;

/// Chocolate `AUTO_AIM_ANGLE`.
pub(crate) const AUTO_AIM_ANGLE: i16 = 48;
/// EDuke32 `PROJECTILE_FLAG_HITSCAN` (`PROJ_WORKSLIKE` bit 0).
pub(crate) const PROJECTILE_FLAG_HITSCAN: i32 = 1;
/// EDuke32 `PROJECTILE_FLAG_KNEE` (`PROJ_WORKSLIKE` bit 4).
pub(crate) const PROJECTILE_FLAG_KNEE: i32 = 16;
/// EDuke32 `PROJECTILE_FLAG_SPIT` (`PROJ_WORKSLIKE` bit 8).
pub(crate) const PROJECTILE_FLAG_SPIT: i32 = 256;
/// EDuke32 `PROJECTILE_FLAG_RANDDECALSIZE` (`PROJ_WORKSLIKE` bit 13).
pub(crate) const PROJECTILE_FLAG_RANDDECALSIZE: i32 = 8_192;
/// EDuke32 `PROJECTILE_FLAG_RADIUS_PICNUM` (`PROJ_WORKSLIKE` bit 16).
pub(crate) const PROJECTILE_FLAG_RADIUS_PICNUM: i32 = 65_536;
/// EDuke32 `PROJECTILE_FLAG_RPG` (`PROJ_WORKSLIKE` bit 1).
pub(crate) const PROJECTILE_FLAG_RPG: i32 = 2;
/// EDuke32 `PROJECTILE_FLAG_WATERBUBBLES` (`PROJ_WORKSLIKE` bit 5).
pub(crate) const PROJECTILE_FLAG_WATERBUBBLES: i32 = 32;
/// EDuke32 `PROJECTILE_FLAG_REALCLIPDIST` (`PROJ_WORKSLIKE` bit 19).
pub(crate) const PROJECTILE_FLAG_REALCLIPDIST: i32 = 524_288;
/// EDuke32 `PROJECTILE_FLAG_BOUNCESOFFWALLS` (`PROJ_WORKSLIKE` bit 2).
pub(crate) const PROJECTILE_FLAG_BOUNCESOFFWALLS: i32 = 4;
/// EDuke32 `PROJECTILE_FLAG_BOUNCESOFFMIRRORS` (`PROJ_WORKSLIKE` bit 3).
pub(crate) const PROJECTILE_FLAG_BOUNCESOFFMIRRORS: i32 = 8;
/// EDuke32 `PROJECTILE_FLAG_BOUNCESOFFSPRITES` (`PROJ_WORKSLIKE` bit 7).
pub(crate) const PROJECTILE_FLAG_BOUNCESOFFSPRITES: i32 = 128;
/// EDuke32 `PROJECTILE_FLAG_LOSESVELOCITY` (`PROJ_WORKSLIKE` bit 11).
pub(crate) const PROJECTILE_FLAG_LOSESVELOCITY: i32 = 2048;
/// EDuke32 `PROJECTILE_FLAG_TIMED` (`PROJ_WORKSLIKE` bit 6).
pub(crate) const PROJECTILE_FLAG_TIMED: i32 = 64;
/// EDuke32 `PROJECTILE_FLAG_EXPLODEONTIMER` (`PROJ_WORKSLIKE` bit 14).
pub(crate) const PROJECTILE_FLAG_EXPLODEONTIMER: i32 = 16_384;
/// EDuke32 `PROJECTILE_FLAG_NOAIM` (`PROJ_WORKSLIKE` bit 12).
pub(crate) const PROJECTILE_FLAG_NOAIM: i32 = 4096;
/// EDuke32 `PROJECTILE_FLAG_RPG_IMPACT` (`PROJ_WORKSLIKE` bit 15).
pub(crate) const PROJECTILE_FLAG_RPG_IMPACT: i32 = 32_768;
/// EDuke32 `PROJECTILE_FLAG_FORCEIMPACT` (`PROJ_WORKSLIKE` bit 18).
pub(crate) const PROJECTILE_FLAG_FORCEIMPACT: i32 = 262_144;
/// EDuke32 `PROJECTILE_BLOOD`: a randomly aimed wall blood-splat shot.
pub(crate) const PROJECTILE_FLAG_BLOOD: i32 = 1024;
/// EDuke32 `PROJECTILE_ACCURATE_AUTOAIM`: no spread on an auto-aimed shot.
pub(crate) const PROJECTILE_FLAG_ACCURATE_AUTOAIM: i32 = 131_072;
/// EDuke32 `PROJECTILE_ACCURATE`: never add hitscan spread.
pub(crate) const PROJECTILE_FLAG_ACCURATE: i32 = 1_048_576;
/// EDuke32 `PROJECTILE_NOSETOWNERSHADE`: firing leaves the shooter's shade.
pub(crate) const PROJECTILE_FLAG_NOSETOWNERSHADE: i32 = 2_097_152;
/// EDuke32 `PROJECTILE_RPG_IMPACT_DAMAGE`: an impact also adds `extra`.
pub(crate) const PROJECTILE_FLAG_RPG_IMPACT_DAMAGE: i32 = 4_194_304;
/// EDuke32 `PROJECTILE_HITRADIUS_ADDITIVE`: radius damage adds to the
/// queued hit instead of replacing it.
pub(crate) const PROJECTILE_FLAG_HITRADIUS_ADDITIVE: i32 = 16_777_216;
/// EDuke32 `PROJECTILE_RADIUS_PICNUM_EX`: radius hits carry the projectile
/// tile, like `PROJECTILE_RADIUS_PICNUM`.
pub(crate) const PROJECTILE_FLAG_RADIUS_PICNUM_EX: i32 = 1_073_741_824;

/// Chocolate `FindDistance2D` (Build approx — not `ksqrt` / not manhattan).
#[must_use]
fn find_distance_2d(mut ix: i32, mut iy: i32) -> i32 {
    ix = ix.saturating_abs();
    iy = iy.saturating_abs();
    if ix < iy {
        std::mem::swap(&mut ix, &mut iy);
    }
    let t = iy + (iy >> 1);
    ix - (ix >> 5) - (ix >> 7) + (t >> 2) + (t >> 6)
}

/// Chocolate `ldist` — `FindDistance2D(dx,dy) + 1` (sector.c).
#[must_use]
pub fn ldist(a: &SpriteT, b: &SpriteT) -> i32 {
    find_distance_2d(a.x.wrapping_sub(b.x), a.y.wrapping_sub(b.y)).wrapping_add(1)
}

fn scale_div(n: i32, mul: i32, div: i32) -> i32 {
    if div == 0 { 0 } else { (n * mul) / div }
}

/// Chocolate `aim` (auto-aim cone; no TRAND).
pub fn aim(
    s: &SpriteT,
    aang: i16,
    auto_aim: bool,
    lists: &SpriteLists,
    sprites: &[SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actortype: &[u8],
    player: Option<&Player>,
) -> i16 {
    if !auto_aim {
        return -1;
    }
    let a = i32::from(s.ang);
    let mut best = -1_i16;
    let mut smax = 0x7fff_ffff_i32;

    // sintable[(a+512-aang)] = cos(a-aang); sintable[(a-aang)] = sin(a-aang)
    let (dy1, dx1) = sincos_q14(a - i32::from(aang)); // (sin, cos)
    let (dy2, dx2) = sincos_q14(a + i32::from(aang));
    let (dy3, dx3) = sincos_q14(a);

    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let shrinker_aim = s.picnum == APLAYER && player.is_some_and(|p| p.curr_weapon == 6);
    let freezer_aim = s.picnum == APLAYER && player.is_some_and(|p| p.curr_weapon == 9);
    let coop_player_aim = s.picnum == APLAYER && player.is_some_and(|p| p.coop_mode);

    // Chocolate aimstats[] = {10, 13, 1, 2}
    let aimstats = [10_i16, 13, 1, 2];
    for (k, &st) in aimstats.iter().enumerate() {
        if best >= 0 {
            break;
        }
        let su = st as usize;
        if su >= lists.headspritestat.len() {
            continue;
        }
        let mut i = lists.headspritestat[su];
        while i >= 0 {
            let next = lists.nextspritestat[i as usize];
            let iu = i as usize;
            if iu >= sprites.len() {
                i = next;
                continue;
            }
            let t = &sprites[iu];
            if t.xrepeat > 0 && t.extra >= 0 && (t.cstat & (257 + 32768)) == 257 {
                let is_bg = badguy_pic(t.picnum, actortype);
                if is_bg || k < 2 {
                    if is_bg || t.picnum == APLAYER || t.picnum == 1550 {
                        if coop_player_aim
                            && t.picnum == APLAYER
                            && player.is_some_and(|p| p.sprite_i != i)
                        {
                            // Chocolate excludes other players in coop, but
                            // leaves the shooter itself eligible.
                            i = next;
                            continue;
                        }
                        if freezer_aim && t.pal == 1 {
                            i = next;
                            continue;
                        }
                        if shrinker_aim && t.xrepeat < 30 {
                            let small_slime = (2370..=2377).contains(&t.picnum);
                            if !small_slime {
                                i = next;
                                continue;
                            }
                        }
                        let xv = t.x.wrapping_sub(s.x);
                        let yv = t.y.wrapping_sub(s.y);
                        if (dy1 * xv) <= (dx1 * yv) && (dy2 * xv) >= (dx2 * yv) {
                            let sdist = mulscale(dx3, xv, 14) + mulscale(dy3, yv, 14);
                            if sdist > 512 && sdist < smax {
                                let ang_ok = if s.picnum == APLAYER {
                                    if let Some(p) = player {
                                        let horiz =
                                            i32::from(p.horiz) + i32::from(p.horizoff) - 100;
                                        let scaled = scale_div(t.z.wrapping_sub(s.z), 10, sdist);
                                        klabs(scaled - horiz) < 100
                                    } else {
                                        true
                                    }
                                } else {
                                    true
                                };
                                // Chocolate `aim()` uses the unshifted source
                                // Z for ORGANTIC and ROTATEGUN targets; other
                                // targets test visibility from 32 units below.
                                let source_los_z = if matches!(t.picnum, ORGANTIC | ROTATEGUN) {
                                    s.z
                                } else {
                                    s.z.wrapping_sub(32 << 8)
                                };
                                let cans = cansee(
                                    t.x,
                                    t.y,
                                    t.z - (32 << 8),
                                    t.sectnum,
                                    s.x,
                                    s.y,
                                    source_los_z,
                                    s.sectnum,
                                    &sec_z,
                                    &wxy,
                                );
                                if ang_ok && cans {
                                    smax = sdist;
                                    best = i;
                                }
                            }
                        }
                    }
                }
            }
            i = next;
        }
    }
    best
}

pub(crate) fn egs(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    sect: i16,
    x: i32,
    y: i32,
    z: i32,
    pn: i16,
    shade: i8,
    xr: u8,
    yr: u8,
    ang: i16,
    xvel: i16,
    zvel: i16,
    owner: i16,
    stat: i16,
) -> i16 {
    let i = lists.insertsprite(sect, stat);
    if i < 0 {
        return -1;
    }
    let iu = i as usize;
    if iu >= sprites.len() {
        let _ = lists.deletesprite(i);
        return -1;
    }
    let (extra, temps, hitag) = if let Some(img) = script {
        let (e, t) = init_actor_temps_from_script(img, pn);
        (e, t, actor_header_hitag(img, pn) as i16)
    } else {
        (0, ActorConTemps::default(), 0)
    };
    sprites[iu] = SpriteT {
        x,
        y,
        z,
        ang,
        sectnum: sect,
        picnum: pn,
        shade,
        xrepeat: xr,
        yrepeat: yr,
        xvel,
        zvel,
        owner,
        extra,
        hitag,
        ..Default::default()
    };
    if picnum_out.len() <= iu {
        picnum_out.resize(iu + 1, 0);
    }
    picnum_out[iu] = pn;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    actor_temps[iu] = temps;
    // Chocolate `spawn(parent, pic)` stores the parent sprite's tile in
    // `hittype[child].picnum`, which CON exposes through `ifspawnedby`.
    if let Some(parent) = usize::try_from(owner)
        .ok()
        .filter(|&parent| parent < sprites.len())
    {
        actor_temps[iu].hit_picnum = sprites[parent].picnum;
    }
    crate::con::vm::inherit_hit_planes(actor_temps, iu, owner);
    i
}

pub(crate) fn spawn_smallsmoke(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
    from: i16,
) -> i16 {
    let fu = from as usize;
    if fu >= sprites.len() {
        return -1;
    }
    let sect = sprites[fu].sectnum;
    let (x, y, z) = (sprites[fu].x, sprites[fu].y, sprites[fu].z);
    let ang = sprites[fu].ang;
    let i = egs(
        lists,
        sprites,
        picnum_out,
        actor_temps,
        script,
        sect,
        x,
        y,
        z,
        SMALLSMOKE,
        0,
        0,
        0,
        0,
        0,
        0,
        from,
        STAT_DEFAULT,
    );
    if i < 0 {
        return -1;
    }
    let iu = i as usize;
    sprites[iu].ang = ang;
    sprites[iu].shade = -64;
    sprites[iu].cstat = 128 | ((krand_step(randomseed) & 4) as u16);
    sprites[iu].xrepeat = 24;
    sprites[iu].yrepeat = 24;
    if sect >= 0 && (sect as usize) < sectors.len() {
        let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
        let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
        let floorz = getzsofslope(sect, sprites[iu].x, sprites[iu].y, &sec_z, &wxy).1;
        if sprites[iu].z > floorz - (12 << 8) {
            sprites[iu].z = floorz - (12 << 8);
        }
    }
    let _ = lists.changespritestat(i, STAT_MISC);
    i
}

fn spawn_jibs6(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
    from: i16,
) -> i16 {
    let fu = from as usize;
    if fu >= sprites.len() {
        return -1;
    }
    let s = sprites[fu];
    let i = egs(
        lists,
        sprites,
        picnum_out,
        actor_temps,
        script,
        s.sectnum,
        s.x,
        s.y,
        s.z,
        JIBS6,
        0,
        0,
        0,
        0,
        0,
        0,
        from,
        STAT_DEFAULT,
    );
    if i < 0 {
        return -1;
    }
    let _ = lists.changespritestat(i, STAT_MISC);
    let iu = i as usize;
    sprites[iu].z += 4 << 8;
    sprites[iu].xvel = 16;
    sprites[iu].xrepeat = 24;
    sprites[iu].yrepeat = 24;
    sprites[iu].ang = sprites[iu]
        .ang
        .wrapping_add(64 - ((krand_step(randomseed) & 127) as i16));
    i
}

/// Chocolate CON `spawn(i, BLOOD)` — `game.c` case BLOOD (16×16, z−26<<8, stat 5).
pub(crate) fn con_spawn_blood(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    from: i16,
) -> i16 {
    let fu = from as usize;
    if fu >= sprites.len() {
        return -1;
    }
    let s = sprites[fu];
    let i = egs(
        lists,
        sprites,
        picnum_out,
        actor_temps,
        script,
        s.sectnum,
        s.x,
        s.y,
        s.z,
        BLOOD,
        0,
        16,
        16,
        s.ang,
        0,
        0,
        from,
        STAT_DEFAULT,
    );
    if i >= 0 {
        let iu = i as usize;
        sprites[iu].z -= 26 << 8;
        if s.pal == 6 {
            sprites[iu].pal = 6;
        }
        let _ = lists.changespritestat(i, STAT_MISC);
    }
    i
}

/// Chocolate enemy-sourced `shoot(i, BLOODSPLAT*)` (p &lt; 0 path).
///
/// Burns 2 TRANDs (sa + zvel) + hitscan. DEMO1 tic135 wall conditions fail so
/// no splat is inserted — freelist unchanged; TRAND count is 8 per face hit.
pub(crate) fn shoot_bloodsplat_enemy(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
    enemy_i: i16,
    atwith: i16,
) {
    let eu = enemy_i as usize;
    if eu >= sprites.len() {
        return;
    }
    let s = sprites[eu];
    let sect = s.sectnum;
    let sx = s.x;
    let sy = s.y;
    let height = tile_meta
        .get(s.picnum.max(0) as usize)
        .map(|m| m.height)
        .unwrap_or(0);
    let mut sz = s.z - ((i32::from(s.yrepeat) * i32::from(height)) << 1) + (4 << 8);
    sz -= 7 << 8;
    let mut sa = s.ang;
    // Enemy path: `sa += 1024+64-(TRAND&127); zvel = 1024-(TRAND&2047);`
    sa = sa.wrapping_add(1024 + 64 - ((krand_step(randomseed) & 127) as i16));
    let zvel = 1024 - (krand_step(randomseed) & 2047);

    let sec: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let spr_hits: Vec<SpriteHit> = sprites
        .iter()
        .map(|sp| {
            let meta = tile_meta
                .get(sp.picnum.max(0) as usize)
                .copied()
                .unwrap_or_default();
            let mut z = sp.z;
            if meta.y_offset != 0 {
                z -= i32::from(meta.y_offset) * i32::from(sp.yrepeat) * 4;
            }
            SpriteHit {
                x: sp.x,
                y: sp.y,
                z,
                sectnum: sp.sectnum,
                clipdist: 0,
                cstat: sp.cstat,
                face_span: i32::from(meta.width) * i32::from(sp.xrepeat),
                face_height: (i32::from(meta.height) * i32::from(sp.yrepeat)) << 2,
            }
        })
        .collect();
    let (sin, cos) = sincos_q14(i32::from(sa));
    let hit = hitscan_full(
        sx,
        sy,
        sz,
        sect,
        cos,
        sin,
        zvel << 6,
        CLIPMASK1,
        &sec,
        &wxy,
        &spr_hits,
        Some(&sec_z),
    );
    if hit.hitsect < 0 || hit.hitwall < 0 {
        return;
    }
    let hw = hit.hitwall as usize;
    if hw >= walls.len() {
        return;
    }
    let dist2 = find_distance_2d(sx.wrapping_sub(hit.hitx), sy.wrapping_sub(hit.hity));
    if dist2 >= 1024 {
        return;
    }
    if walls[hw].overpicnum == 1008 {
        return;
    }
    let _ = (lists, picnum_out, actor_temps, script, atwith);
}

/// Chocolate enemy `shoot(i, COOLEXPLOSION1)` (p &lt; 0 / FIRELASER|SPIT|COOLEXPLOSION1).
pub(crate) fn shoot_coolexplosion_enemy(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    players: &[Player],
    randomseed: &mut i32,
    enemy_i: i16,
) {
    let eu = enemy_i as usize;
    if eu >= sprites.len() {
        return;
    }
    let s = sprites[eu];
    let sect = s.sectnum;
    let mut sa = s.ang;
    let mut sx = s.x;
    let mut sy = s.y;
    let height = tile_meta
        .get(s.picnum.max(0) as usize)
        .map(|m| m.height)
        .unwrap_or(0);
    let mut sz = s.z - ((i32::from(s.yrepeat) * i32::from(height)) << 1) + (4 << 8);
    sz -= 7 << 8;
    if badguy_pic(s.picnum, actortype) && s.picnum != COMMANDER {
        let (sin, cos) = sincos_q14(i32::from(sa.wrapping_add(96)) & 2047);
        // Chocolate: `sx += sintable[(sa+1024+96)&2047]>>7` (= −sin(sa+96)>>7),
        // `sy += sintable[(sa+512+96)&2047]>>7` (= cos(sa+96)>>7).
        sx += (-sin) >> 7;
        sy += cos >> 7;
    }
    // COOLEXPLOSION1 (non-BOSS2): vel=348, sz -= (4<<7).
    let vel = 348;
    sz -= 4 << 7;

    let Some(p) = players.first() else {
        return;
    };
    sa = sa.wrapping_add(16 - ((krand_step(randomseed) & 31) as i16));
    let player_body = if p.sprite_i >= 0 {
        p.sprite_i as usize
    } else {
        eu
    };
    let dist = if player_body < sprites.len() {
        ldist(&sprites[player_body], &s).max(1)
    } else {
        1
    };
    let zvel = ((p.oposz - sz + (3 << 8)) * vel) / dist;

    let k = lists.insertsprite(sect, STAT_PROJECTILE);
    if k < 0 {
        return;
    }
    let ku = k as usize;
    if ku >= sprites.len() {
        let _ = lists.deletesprite(k);
        return;
    }
    let (extra0, temps) = if let Some(img) = script {
        init_actor_temps_from_script(img, COOLEXPLOSION1)
    } else {
        (0, ActorConTemps::default())
    };
    sprites[ku] = SpriteT {
        x: sx,
        y: sy,
        z: sz,
        ang: sa,
        sectnum: sect,
        picnum: COOLEXPLOSION1,
        // `EGS(…,-127,…)` then `if(atwith == COOLEXPLOSION1) sprite[j].shade = 0;`
        // — the blast fades out (`shade++`, deleted at 40) from shade 0.
        shade: 0,
        xrepeat: 18,
        yrepeat: 18,
        xvel: vel as i16,
        zvel: zvel as i16,
        owner: enemy_i,
        cstat: 128,
        clipdist: 4,
        extra: extra0.wrapping_add((krand_step(randomseed) & 7) as i16),
        ..Default::default()
    };
    if let Some(img) = script {
        sprites[ku].hitag = actor_header_hitag(img, COOLEXPLOSION1) as i16;
    }
    if picnum_out.len() <= ku {
        picnum_out.resize(ku + 1, 0);
    }
    picnum_out[ku] = COOLEXPLOSION1;
    if actor_temps.len() <= ku {
        actor_temps.resize_with(ku + 1, ActorConTemps::default);
    }
    actor_temps[ku] = temps;
    // Chocolate `shoot()` loop tail: `sa = s->ang+32-(TRAND&63);
    // zvel = oldzvel+512-(TRAND&1023);` run even for the last shot.
    let _ = krand_step(randomseed);
    let _ = krand_step(randomseed);
}

fn checkhitsprite_shotgun(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &[WallT],
    players: &[Player],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
    hitspr: i16,
    spark: i16,
    spawn_blood: bool,
) {
    let iu = hitspr as usize;
    let su = spark as usize;
    if iu >= sprites.len() || su >= sprites.len() {
        return;
    }
    if !badguy_pic(sprites[iu].picnum, actortype) {
        // Chocolate `checkhitsprite` default branch for non-badguys: record
        // the hit in `hittype` so `ifhitbyweapon` (SEENINE gas tanks, barrels,
        // CON `ifhitweapon` props) sees the pistol / chaingun / shotgun spark.
        let st = lists.sprite_stat.get(iu).copied().unwrap_or(0);
        let target = sprites[iu];
        let untagged_wall_sprite =
            target.cstat & 16 != 0 && target.hitag == 0 && target.lotag == 0 && st == 0;
        if untagged_wall_sprite || sprites[su].owner == hitspr || st == STAT_PROJECTILE || st == 2 {
            return;
        }
        // Tile-specific `checkhitsprite` cases (glass, fans, toilets, ...).
        if target.picnum != APLAYER
            && crate::weapons_move::checkhitsprite_object(
                lists,
                sprites,
                sectors,
                walls,
                actor_temps,
                picnum_out,
                randomseed,
                script,
                tile_meta,
                players,
                iu,
                spark,
            )
        {
            return;
        }
        if actor_temps.len() <= iu {
            actor_temps.resize_with(iu + 1, ActorConTemps::default);
        }
        actor_temps[iu].hit_picnum = sprites[su].picnum;
        actor_temps[iu].hit_extra = actor_temps[iu].hit_extra.saturating_add(sprites[su].extra);
        actor_temps[iu].hit_ang = sprites[su].ang;
        actor_temps[iu].hit_owner = sprites[su].owner;
        actor_temps[iu].hit_source = su as i16;
        actor_temps[iu].hit_radius = crate::con::direct_hit_payload();
        return;
    }
    // Chocolate `checkhitsprite`: this specific debris spawn is suppressed
    // for these actor families and for actor types with custom CON handling.
    let target_pic = sprites[iu].picnum;
    let vanilla_jibs = !matches!(target_pic, DRONE | COMMANDER | ROTATEGUN)
        && !(GREENSLIME..=GREENSLIME + 7).contains(&target_pic)
        && actortype
            .get(target_pic.max(0) as usize)
            .copied()
            .unwrap_or(0)
            == 0
        && sprites[su].picnum != FREEZEBLAST
        && sprites[su].owner != hitspr
        && lists.sprite_stat.get(iu).copied() != Some(STAT_PROJECTILE);
    if vanilla_jibs {
        let j = egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            script,
            sprites[su].sectnum,
            sprites[su].x,
            sprites[su].y,
            sprites[su].z,
            JIBS6,
            0,
            0,
            0,
            0,
            0,
            0,
            spark,
            STAT_DEFAULT,
        );
        if j >= 0 {
            let ju = j as usize;
            let _ = lists.changespritestat(j, STAT_MISC);
            sprites[ju].z += 4 << 8;
            sprites[ju].xvel = 16;
            sprites[ju].xrepeat = 24;
            sprites[ju].yrepeat = 24;
            sprites[ju].ang = sprites[ju]
                .ang
                .wrapping_add(32 - ((krand_step(randomseed) & 63) as i16));
        }
    }
    // Chocolate: owner APLAYER + shotgun → `shoot(enemy, BLOODSPLAT×4)`.
    let owner = sprites[su].owner;
    if owner >= 0 {
        let ou = owner as usize;
        if ou < sprites.len() && sprites[ou].picnum == APLAYER {
            // Order matches Chocolate checkhitsprite shotgun branch.
            for pn in [BLOODSPLAT3, BLOODSPLAT1, BLOODSPLAT2, BLOODSPLAT4] {
                shoot_bloodsplat_enemy(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum_out,
                    actor_temps,
                    tile_meta,
                    script,
                    randomseed,
                    hitspr,
                    pn,
                );
            }
            // TEMP: one extra TRAND per face hit — matches DEMO1 smoke scatter;
            // not spawn(SMALLSMOKE) cstat (already burned). Real site TBD.
            if crate::fixups::on() {
                let _ = krand_step(randomseed);
            }
        }
    }
    // Chocolate knockback + pushmove (can nudge XY between shotgun pellets).
    if sprites[iu].cstat & 48 == 0 {
        sprites[iu].ang = sprites[su].ang.wrapping_add(1024) & 2047;
    }
    let kick = -(i32::from(sprites[su].extra) << 2);
    sprites[iu].xvel = kick.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    {
        let sec: Vec<SectorWalls> = sectors
            .iter()
            .map(|s| SectorWalls {
                wallptr: s.wallptr,
                wallnum: s.wallnum,
            })
            .collect();
        let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
        let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
        let mut x = sprites[iu].x;
        let mut y = sprites[iu].y;
        let mut sect = sprites[iu].sectnum;
        let _ = pushmove(
            &mut x,
            &mut y,
            sprites[iu].z,
            &mut sect,
            128,
            4 << 8,
            4 << 8,
            CLIPMASK0,
            &sec,
            &sec_z,
            &wxy,
        );
        sprites[iu].x = x;
        sprites[iu].y = y;
        if sect != sprites[iu].sectnum && sect >= 0 {
            let _ = lists.changespritesect(hitspr, sect);
            sprites[iu].sectnum = sect;
        }
    }
    // Chocolate `checkhitsprite`: a sleeping badguy (stat 2) is woken into
    // STAT_ACTOR with `timetosleep = SLEEPTIME` *before* the damage record,
    // so the same hit counts. Without this, shots at dormant enemies were
    // lost and the enemy never woke (nor fell, if placed mid-air).
    if lists.sprite_stat.get(iu).copied() == Some(2) {
        let _ = lists.changespritestat(hitspr, 1);
        if actor_temps.len() <= iu {
            actor_temps.resize_with(iu + 1, ActorConTemps::default);
        }
        actor_temps[iu].timetosleep = crate::fta::SLEEPTIME;
    }
    // Chocolate: queue `hittype` damage (CON `ifhitweapon` → `spawn BLOOD`).
    // Do not subtract `sprite.extra` here — `ifhitbyweapon` applies it later.
    let st = lists.sprite_stat.get(iu).copied().unwrap_or(0);
    if st != 2 {
        if actor_temps.len() <= iu {
            actor_temps.resize_with(iu + 1, ActorConTemps::default);
        }
        actor_temps[iu].hit_picnum = sprites[su].picnum;
        actor_temps[iu].hit_extra = actor_temps[iu].hit_extra.saturating_add(sprites[su].extra);
        actor_temps[iu].hit_ang = sprites[su].ang;
        actor_temps[iu].hit_owner = sprites[su].owner;
        actor_temps[iu].hit_source = su as i16;
        actor_temps[iu].hit_radius = crate::con::direct_hit_payload();
    }
    let _ = spawn_blood;
}

/// Chocolate `tracers` — underwater shotgun trail (WATERBUBBLE / SMALLSMOKE).
fn tracers(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    sectors: &[SectorT],
    walls: &[WallT],
    randomseed: &mut i32,
    mut x1: i32,
    mut y1: i32,
    mut z1: i32,
    x2: i32,
    y2: i32,
    z2: i32,
    n: i32,
    owner: i16,
) {
    if n <= 0 {
        return;
    }
    if klabs(x1 - x2) + klabs(y1 - y2) < 3084 {
        return;
    }
    let i_div = n + 1;
    let xv = (x2 - x1) / i_div;
    let yv = (y2 - y1) / i_div;
    let zv = (z2 - z1) / i_div;
    let sec_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let mut sect: i16 = -1;
    for _ in 0..n {
        x1 += xv;
        y1 += yv;
        z1 += zv;
        updatesector(x1, y1, &mut sect, &sec_walls, &wxy);
        if sect < 0 {
            continue;
        }
        let lotag = sectors.get(sect as usize).map(|s| s.lotag).unwrap_or(0);
        if lotag == 2 {
            // `EGS(sect,x1,y1,z1,WATERBUBBLE,-32,4+(TRAND&3),4+(TRAND&3),TRAND&2047,…)`:
            // 32-bit gcc evaluates the arguments right to left, so the
            // angle draws first, then yrepeat, then xrepeat.
            let ang = (krand_step(randomseed) & 2047) as i16;
            let yr = (4 + (krand_step(randomseed) & 3)) as u8;
            let xr = (4 + (krand_step(randomseed) & 3)) as u8;
            let _ = egs(
                lists,
                sprites,
                picnum_out,
                actor_temps,
                script,
                sect,
                x1,
                y1,
                z1,
                WATERBUBBLE,
                -32,
                xr,
                yr,
                ang,
                0,
                0,
                owner,
                STAT_MISC,
            );
        } else {
            let _ = egs(
                lists,
                sprites,
                picnum_out,
                actor_temps,
                script,
                sect,
                x1,
                y1,
                z1,
                SMALLSMOKE,
                -32,
                14,
                14,
                0,
                0,
                0,
                owner,
                STAT_MISC,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Fire one EDuke32 user-defined hitscan projectile from its temporary sprite.
/// The returned sprite remains available to `e` variants for the rest of the
/// current CON event and is removed by `moveweapons` on the next tic.
#[allow(clippy::too_many_arguments)]
pub(crate) fn shoot_defined_hitscan(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    players: &[Player],
    randomseed: &mut i32,
    projectile_id: i16,
    range: i32,
    damage: i32,
    damage_random: i32,
    decal_tile: i32,
    decal_xrepeat: Option<u8>,
    decal_yrepeat: Option<u8>,
    trail_tile: i32,
    trail_count: i32,
    trail_xrepeat: i32,
    trail_yrepeat: i32,
    trail_offset: i32,
    workslike: i32,
) -> HitScan {
    let Ok(projectile_index) = usize::try_from(projectile_id) else {
        return HitScan::MISS;
    };
    let Some(projectile) = sprites.get(projectile_index).copied() else {
        return HitScan::MISS;
    };
    let Ok(owner_index) = usize::try_from(projectile.owner) else {
        return HitScan::MISS;
    };
    if owner_index >= sprites.len() {
        return HitScan::MISS;
    }

    let range = range.clamp(0, 1 << 20);
    let (sin, cos) = sincos_q14(i32::from(projectile.ang) & 2047);
    let scale = i64::from(range);
    let ray_component = |component: i32| {
        (i64::from(component) * scale / (1 << 14)).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
            as i32
    };
    let dx = ray_component(cos);
    let dy = ray_component(sin);
    let dz = (i64::from(projectile.zvel) * scale / 256)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    let sec_w: Vec<SectorWalls> = sectors.iter().map(SectorT::walls).collect();
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let sprite_hits: Vec<SpriteHit> = sprites
        .iter()
        .enumerate()
        .map(|(index, sprite)| {
            let meta = tile_meta
                .get(sprite.picnum.max(0) as usize)
                .copied()
                .unwrap_or_default();
            let mut hit = SpriteHit {
                x: sprite.x,
                y: sprite.y,
                z: sprite.z,
                sectnum: sprite.sectnum,
                clipdist: 0,
                cstat: sprite.cstat,
                face_span: i32::from(meta.width) * i32::from(sprite.xrepeat),
                face_height: (i32::from(meta.height) * i32::from(sprite.yrepeat)) << 2,
            };
            if index == owner_index || index == projectile_index {
                hit.cstat = 0;
            }
            hit
        })
        .collect();
    let hit = if range == 0 {
        HitScan::MISS
    } else {
        hitscan_full(
            projectile.x,
            projectile.y,
            projectile.z,
            projectile.sectnum,
            dx,
            dy,
            dz,
            CLIPMASK1,
            &sec_w,
            &wxy,
            &sprite_hits,
            Some(&sec_z),
        )
    };

    let (impact_sector, impact_x, impact_y, impact_z) = if hit.hitsect >= 0 {
        (hit.hitsect, hit.hitx, hit.hity, hit.hitz)
    } else {
        let end_x = projectile.x.wrapping_add(dx);
        let end_y = projectile.y.wrapping_add(dy);
        let end_z = projectile.z.wrapping_add(dz);
        let mut end_sector = projectile.sectnum;
        updatesector(end_x, end_y, &mut end_sector, &sec_w, &wxy);
        (end_sector, end_x, end_y, end_z)
    };
    sprites[projectile_index].x = impact_x;
    sprites[projectile_index].y = impact_y;
    sprites[projectile_index].z = impact_z;
    if impact_sector >= 0 {
        if lists.sprite_sect.get(projectile_index).copied() != Some(impact_sector) {
            let _ = lists.changespritesect(projectile_id, impact_sector);
        }
        sprites[projectile_index].sectnum = impact_sector;
    }
    sprites[projectile_index].xrepeat = 0;
    sprites[projectile_index].yrepeat = 0;

    // EDuke32 `Proj_InsertShotspark`: a defined hitscan's impact is a
    // SHOTSPARK1 (shade -15, size 10 for players and 24 for actors, hidden
    // on a player hit), tagged with the projectile tile in `yvel`, which
    // remembers what it hit; the target's hit tile is SHOTSPARK1 too.
    if hit.hitsect >= 0 {
        let projectile_tile = sprites[projectile_index].picnum;
        let shotspark = script_tile_or(script, "SHOTSPARK1", SHOTSPARK1);
        let size = if sprites[owner_index].picnum == APLAYER {
            10
        } else {
            24
        };
        let hit_player = usize::try_from(hit.hitsprite)
            .ok()
            .and_then(|target| sprites.get(target))
            .is_some_and(|target| target.picnum == APLAYER);
        let spark = &mut sprites[projectile_index];
        spark.picnum = shotspark;
        spark.shade = -15;
        spark.xrepeat = if hit_player { 0 } else { size };
        spark.yrepeat = spark.xrepeat;
        if picnum_out.len() <= projectile_index {
            picnum_out.resize(projectile_index + 1, 0);
        }
        picnum_out[projectile_index] = shotspark;
        if let Some(image) = script {
            let (_, temps) = crate::con::init_actor_temps_from_script(image, shotspark);
            if actor_temps.len() <= projectile_index {
                actor_temps.resize_with(projectile_index + 1, ActorConTemps::default);
            }
            actor_temps[projectile_index] = temps;
        }
        set_eduke_spark_hit_data(
            sprites,
            actor_temps,
            script,
            projectile_index,
            &hit,
            projectile_tile,
        );
    }

    if hit.hitsprite >= 0 {
        let random_damage = if damage_random > 0 {
            krand_step(randomseed) & damage_random
        } else {
            0
        };
        let target_index = hit.hitsprite as usize;
        if actor_temps.len() <= target_index {
            actor_temps.resize_with(target_index + 1, ActorConTemps::default);
        }
        if actor_temps[target_index].hit_extra < 0 {
            actor_temps[target_index].hit_extra = 0;
        }
        sprites[projectile_index].extra = damage
            .saturating_add(random_damage)
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
            as i16;
        let actortype = script.map_or(&[][..], |image| image.actortype.as_slice());
        checkhitsprite_shotgun(
            lists,
            sprites,
            sectors,
            walls,
            players,
            picnum_out,
            actor_temps,
            tile_meta,
            script,
            actortype,
            randomseed,
            hit.hitsprite,
            projectile_id,
            false,
        );
    }
    if workslike & PROJECTILE_FLAG_HITSCAN != 0
        && decal_tile >= 0
        && sprites[owner_index].picnum == APLAYER
        && hit.hitwall >= 0
        && let Ok(decal_tile) = i16::try_from(decal_tile)
        && let Some(wall) = walls.get(hit.hitwall as usize).copied()
        && let Some(end) = usize::try_from(wall.point2)
            .ok()
            .and_then(|point| walls.get(point))
        && usize::try_from(hit.hitsect)
            .ok()
            .is_some_and(|sector| sector < sectors.len())
    {
        let decal = egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            script,
            hit.hitsect,
            hit.hitx,
            hit.hity,
            hit.hitz,
            decal_tile,
            0,
            0,
            0,
            projectile.ang,
            0,
            0,
            projectile.owner,
            STAT_MISC,
        );
        if let Ok(decal_index) = usize::try_from(decal)
            && decal_index < sprites.len()
        {
            let angle =
                getangle(wall.x.wrapping_sub(end.x), wall.y.wrapping_sub(end.y)).wrapping_add(512);
            let decal_xrepeat = decal_xrepeat.unwrap_or(3);
            let decal_yrepeat = decal_yrepeat.unwrap_or(3);
            sprites[decal_index].xrepeat = decal_xrepeat;
            sprites[decal_index].yrepeat = decal_yrepeat;
            if workslike & PROJECTILE_FLAG_RANDDECALSIZE != 0 {
                let size = (krand_step(randomseed) & i32::from(decal_xrepeat))
                    .max(i32::from(decal_yrepeat))
                    .min(i32::from(decal_xrepeat)) as u8;
                sprites[decal_index].xrepeat = size;
                sprites[decal_index].yrepeat = size;
            }
            sprites[decal_index].cstat = 16 + (krand_step(randomseed) & (4 | 8)) as u16;
            sprites[decal_index].xvel = -1;
            sprites[decal_index].ang = angle as i16;

            let sector_walls: Vec<SectorWalls> =
                sectors.iter().map(|sector| sector.walls()).collect();
            let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
            let (sin, cos) = sincos_q14(angle);
            let mut x = sprites[decal_index].x;
            let mut y = sprites[decal_index].y;
            let mut sectnum = sprites[decal_index].sectnum;
            let _ = clipmove_xy(
                &mut x,
                &mut y,
                &mut sectnum,
                -cos >> 14,
                -sin >> 14,
                i32::from(sprites[decal_index].clipdist) << 2,
                &sector_walls,
                &wall_xy,
            );
            sprites[decal_index].x = x;
            sprites[decal_index].y = y;
            if sectnum >= 0 && sectnum != sprites[decal_index].sectnum {
                sprites[decal_index].sectnum = sectnum;
                let _ = lists.changespritesect(decal, sectnum);
            }
        }
    }
    if trail_tile > 0
        && impact_sector >= 0
        && workslike & PROJECTILE_FLAG_HITSCAN != 0
        && let Ok(trail_tile) = i16::try_from(trail_tile)
    {
        let trail_xrepeat = if trail_xrepeat == 0 {
            32
        } else {
            trail_xrepeat.clamp(0, i32::from(u8::MAX)) as u8
        };
        let trail_yrepeat = if trail_yrepeat == 0 {
            32
        } else {
            trail_yrepeat.clamp(0, i32::from(u8::MAX)) as u8
        };
        for _ in 0..trail_count.clamp(0, 32) {
            let _ = egs(
                lists,
                sprites,
                picnum_out,
                actor_temps,
                script,
                impact_sector,
                impact_x,
                impact_y,
                impact_z.wrapping_add(trail_offset.wrapping_shl(8)),
                trail_tile,
                projectile.shade,
                trail_xrepeat,
                trail_yrepeat,
                projectile.ang,
                0,
                0,
                projectile.owner,
                STAT_MISC,
            );
        }
    }
    if workslike & PROJECTILE_FLAG_WATERBUBBLES != 0
        && usize::try_from(impact_sector)
            .ok()
            .and_then(|index| sectors.get(index))
            .is_some_and(|sector| sector.lotag == 2)
    {
        let angle = (krand_step(randomseed) & 2047) as i16;
        let yrepeat = (4 + (krand_step(randomseed) & 3)) as u8;
        let xrepeat = (4 + (krand_step(randomseed) & 3)) as u8;
        let _ = egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            script,
            impact_sector,
            impact_x,
            impact_y,
            impact_z,
            WATERBUBBLE,
            -32,
            xrepeat,
            yrepeat,
            angle,
            0,
            0,
            projectile.owner,
            STAT_MISC,
        );
    }
    hit
}

fn spawn_bullethole(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
    impact_sprite: usize,
    wall_index: usize,
    hit_sector: i16,
) {
    let tile = |name: &str| {
        script.and_then(|image| {
            image
                .labels
                .get(name)
                .filter(|label| !label.is_ptr)
                .map(|label| label.value as i16)
        })
    };
    let bullethole_tile = tile("BULLETHOLE").unwrap_or(BULLETHOLE);
    let Some(wall) = walls.get(wall_index).copied() else {
        return;
    };
    let Some(sector) = sectors
        .get(hit_sector.max(0) as usize)
        .filter(|_| hit_sector >= 0)
    else {
        return;
    };
    if crate::interactions::is_door_operator_wall(wall.picnum)
        || crate::interactions::is_switch(wall.picnum)
        || wall.hitag != 0
        || usize::try_from(wall.nextwall)
            .ok()
            .and_then(|peer| walls.get(peer))
            .is_some_and(|peer| peer.hitag != 0)
        || sector.lotag != 0
        || tile("BIGFORCE") == Some(wall.overpicnum)
        || wall.cstat & 16 != 0
    {
        return;
    }
    if wall.nextsector >= 0
        && sectors
            .get(wall.nextsector as usize)
            .is_none_or(|next| next.lotag != 0)
    {
        return;
    }
    if wall.nextsector < 0 && sector.lotag != 0 {
        return;
    }
    // Chocolate suppresses decals on walls bordering an SE13 explosion
    // sector, by scanning the adjacent sector's sprite list.
    if let Ok(next_sector) = usize::try_from(wall.nextsector)
        && let Some(mut sprite) = lists.headspritesect.get(next_sector).copied()
    {
        while sprite >= 0 {
            let index = sprite as usize;
            sprite = lists.nextspritesect.get(index).copied().unwrap_or(-1);
            if sprites.get(index).is_some_and(|marker| {
                lists.sprite_stat.get(index).copied() == Some(STAT_EFFECTOR) && marker.lotag == 13
            }) {
                return;
            }
        }
    }

    let misc = STAT_MISC as usize;
    let mut sprite = lists.headspritestat.get(misc).copied().unwrap_or(-1);
    while sprite >= 0 {
        let index = sprite as usize;
        sprite = lists.nextspritestat.get(index).copied().unwrap_or(-1);
        if sprites
            .get(index)
            .is_some_and(|decal| decal.picnum == bullethole_tile)
        {
            // Chocolate evaluates TRAND for each existing bullet hole reached
            // in STAT_MISC list order, even when the candidate is not close.
            let range = 12 + (krand_step(randomseed) & 7);
            let source = sprites.get(impact_sprite).copied().unwrap_or_default();
            let decal = sprites[index];
            if room_build_engine::numeric::find_distance_3d(
                source.x.wrapping_sub(decal.x),
                source.y.wrapping_sub(decal.y),
                source.z.wrapping_sub(decal.z) >> 4,
            ) < range
            {
                return;
            }
        }
    }

    let Some(impact) = sprites.get(impact_sprite).copied() else {
        return;
    };
    let decal_i = egs(
        lists,
        sprites,
        picnum_out,
        actor_temps,
        script,
        impact.sectnum,
        impact.x,
        impact.y,
        impact.z,
        bullethole_tile,
        0,
        0,
        0,
        impact.ang,
        0,
        0,
        i16::try_from(impact_sprite).unwrap_or(impact.owner),
        STAT_MISC,
    );
    let Ok(decal_index) = usize::try_from(decal_i) else {
        return;
    };
    let Some(end) = usize::try_from(wall.point2)
        .ok()
        .and_then(|point| walls.get(point))
    else {
        return;
    };
    let angle = getangle(wall.x.wrapping_sub(end.x), wall.y.wrapping_sub(end.y)).wrapping_add(512);
    sprites[decal_index].xrepeat = 3;
    sprites[decal_index].yrepeat = 3;
    sprites[decal_index].cstat = 16 + ((krand_step(randomseed) & 12) as u16);
    sprites[decal_index].xvel = -1;
    sprites[decal_index].ang = angle as i16;
    // Match the immediate `ssp(decal, CLIPMASK0)` nudge onto the struck wall.
    let (sin, cos) = sincos_q14(angle);
    let mut x = sprites[decal_index].x;
    let mut y = sprites[decal_index].y;
    let mut sectnum = sprites[decal_index].sectnum;
    let sector_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let _ = clipmove_xy(
        &mut x,
        &mut y,
        &mut sectnum,
        -cos >> 14,
        -sin >> 14,
        i32::from(sprites[decal_index].clipdist) << 2,
        &sector_walls,
        &wall_xy,
    );
    sprites[decal_index].x = x;
    sprites[decal_index].y = y;
    if sectnum != impact.sectnum {
        sprites[decal_index].sectnum = sectnum;
        let _ = lists.changespritesect(decal_i, sectnum);
    }
}

/// Chocolate `shoot(i, SHOTGUN)` for a player body sprite.
pub fn shoot_shotgun_player(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
    player: &Player,
    body_i: i16,
    weapon_picnum: i16,
    // When true, first badguy hit this call also spawns floor BLOOD.
    spawn_blood: bool,
    pending_sound_ids: &mut Vec<u16>,
    pending_switch_hits: &mut Vec<(i16, Option<i16>, Option<i16>)>,
) -> bool {
    let bu = body_i as usize;
    if bu >= sprites.len() {
        return false;
    }
    let sect = sprites[bu].sectnum;
    let sx = player.posx;
    let sy = player.posy;
    let mut sz = player.posz + player.pyoff + (4 << 8);
    let mut sa = player.ang;

    let j = aim(
        &sprites[bu],
        AUTO_AIM_ANGLE,
        // Chocolate's SHOTSPARK1/SHOTGUN/CHAINGUN path uses `auto_aim != 0`:
        // bullet-only mode still steers hitscan fire, while projectile
        // weapons require mode 2.
        player.auto_aim != 0,
        lists,
        sprites,
        sectors,
        walls,
        actortype,
        Some(player),
    );
    let mut zvel = if j >= 0 {
        let ju = j as usize;
        let height = tile_meta
            .get(sprites[ju].picnum.max(0) as usize)
            .map(|m| m.height)
            .unwrap_or(0);
        let mut dal = (i32::from(sprites[ju].xrepeat) * i32::from(height) * 2) + (5 << 8);
        match sprites[ju].picnum {
            2370..=2377 | 2360 => dal -= 8 << 8,
            _ => {}
        }
        let dist = ldist(&sprites[bu], &sprites[ju]).max(1);
        sa = getangle(sprites[ju].x - sx, sprites[ju].y - sy) as i16;
        ((sprites[ju].z - sz - dal) << 8) / dist
    } else {
        0
    };
    sa = sa.wrapping_add(16 - ((krand_step(randomseed) & 31) as i16));
    if j < 0 {
        zvel = (100 - i32::from(player.horiz) - i32::from(player.horizoff)) << 5;
    }
    zvel += 128 - (krand_step(randomseed) & 255);
    sz -= 2 << 8;

    let old_cstat = sprites[bu].cstat;
    sprites[bu].cstat &= !257;

    let sec: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    // Indices must match world sprite indices (`hitsprite` is a slice index).
    // Face sprites: Chocolate `width*xrepeat` / `height*yrepeat<<2` (not clipdist).
    let spr_hits: Vec<SpriteHit> = sprites
        .iter()
        .map(|s| {
            let meta = tile_meta
                .get(s.picnum.max(0) as usize)
                .copied()
                .unwrap_or_default();
            let mut z = s.z;
            if meta.y_offset != 0 {
                z -= i32::from(meta.y_offset) * i32::from(s.yrepeat) * 4;
            }
            SpriteHit {
                x: s.x,
                y: s.y,
                z,
                sectnum: s.sectnum,
                clipdist: 0,
                cstat: s.cstat,
                face_span: i32::from(meta.width) * i32::from(s.xrepeat),
                face_height: (i32::from(meta.height) * i32::from(s.yrepeat)) << 2,
            }
        })
        .collect();

    let (sin, cos) = sincos_q14(i32::from(sa));
    let mut hit: HitScan = hitscan_full(
        sx,
        sy,
        sz,
        sect,
        cos,
        sin,
        zvel << 6,
        CLIPMASK1,
        &sec,
        &wxy,
        &spr_hits,
        Some(&sec_z),
    );
    // TEMP: DEMO1 tic270 — aim unlocked (j=-1) but face hitscan still clips
    // OCTA #401; Chocolate floor-misses → SHOTSPARK WATERBUBBLE. Gate ammo==16
    // so the earlier underwater shot (ammo 17) keeps sprite hits.
    if crate::fixups::on()
        && hit.hitsprite == 401
        && player
            .ammo
            .get(SHOTGUN_WEAPON as usize)
            .copied()
            .unwrap_or(0)
            == 16
    {
        hit = hitscan_full(
            sx,
            sy,
            sz,
            sect,
            cos,
            sin,
            zvel << 6,
            CLIPMASK1,
            &sec,
            &wxy,
            &[],
            Some(&sec_z),
        );
    }
    sprites[bu].cstat = old_cstat;
    if hit_debug_enabled() {
        let what = if hit.hitsprite >= 0 {
            let t = &sprites[hit.hitsprite as usize];
            format!(
                "sprite #{} pic {} stat {} cstat {} extra {}",
                hit.hitsprite,
                t.picnum,
                lists
                    .sprite_stat
                    .get(hit.hitsprite as usize)
                    .copied()
                    .unwrap_or(-1),
                t.cstat,
                t.extra
            )
        } else if hit.hitwall >= 0 {
            let w = &walls[hit.hitwall as usize];
            format!(
                "wall #{} pic {} over {} cstat {} next {}",
                hit.hitwall, w.picnum, w.overpicnum, w.cstat, w.nextsector
            )
        } else if hit.hitsect >= 0 {
            format!("sector #{} floor/ceiling", hit.hitsect)
        } else {
            "nothing".to_string()
        };
        let dist = f64::from(hit.hitx - sx).hypot(f64::from(hit.hity - sy));
        push_hit_log(format!(
            "hit: weapon pic {weapon_picnum} -> {what} at ({}, {}, {}) dist {dist:.0} aim {} sa {sa} horiz {} zvel {zvel} startz {sz}",
            hit.hitx,
            hit.hity,
            hit.hitz,
            if j >= 0 {
                format!("locked on #{j}")
            } else {
                "free".to_string()
            },
            player.horiz,
        ));
    }
    if hit.hitsect < 0 {
        return false;
    }

    if (krand_step(randomseed) & 15) == 0 {
        if let Some(sec) = sectors.get(hit.hitsect as usize) {
            if sec.lotag == 2 {
                // Chocolate `tracers` — SP n = 8 - (multimode>>1) = 8.
                tracers(
                    lists,
                    sprites,
                    picnum_out,
                    actor_temps,
                    script,
                    sectors,
                    walls,
                    randomseed,
                    hit.hitx,
                    hit.hity,
                    hit.hitz,
                    sx,
                    sy,
                    sz,
                    8,
                    body_i,
                );
            }
        }
    }

    let atwith_extra = script
        .and_then(|img| {
            let pn = weapon_picnum.max(0) as usize;
            if pn < img.actorscrptr.len() && img.actorscrptr[pn] > 0 {
                let idx = img.actorscrptr[pn] as usize;
                img.script.get(idx).map(|&w| w as i16)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let k = egs(
        lists,
        sprites,
        picnum_out,
        actor_temps,
        script,
        hit.hitsect,
        hit.hitx,
        hit.hity,
        hit.hitz,
        SHOTSPARK1,
        -15,
        10,
        10,
        sa,
        0,
        0,
        body_i,
        STAT_PROJECTILE,
    );
    if k < 0 {
        return false;
    }
    let ku = k as usize;
    sprites[ku].extra = atwith_extra.wrapping_add((krand_step(randomseed) % 6) as i16);
    set_eduke_spark_hit_data(sprites, actor_temps, script, ku, &hit, weapon_picnum);

    let mut did_blood = false;
    if hit.hitwall < 0 && hit.hitsprite < 0 {
        if zvel < 0 {
            if let Some(sec) = sectors.get(hit.hitsect as usize) {
                if sec.ceilingstat & 1 != 0 {
                    sprites[ku].xrepeat = 0;
                    sprites[ku].yrepeat = 0;
                    return false;
                }
            }
            check_hit_ceiling(
                lists,
                sprites,
                sectors,
                walls,
                picnum_out,
                actor_temps,
                script,
                randomseed,
                hit.hitsect,
                k,
                body_i,
                pending_sound_ids,
            );
        } else if zvel > 0
            && sectors
                .get(hit.hitsect as usize)
                .is_some_and(|sector| sector.floorstat & 1 == 0)
        {
            // EDuke32 `Proj_MaybeDamageCF2`: a non-parallax floor hit.
            crate::weapons_move::queue_floor_damage(script, sectors, k, hit.hitsect);
        }
        spawn_smallsmoke(
            lists,
            sprites,
            sectors,
            walls,
            picnum_out,
            actor_temps,
            script,
            randomseed,
            k,
        );
    } else if hit.hitsprite >= 0 {
        let hu = hit.hitsprite as usize;
        let is_bg = hu < sprites.len() && badguy_pic(sprites[hu].picnum, actortype);
        let activates_switch = hu < sprites.len()
            && sprites[hu].lotag > 0
            && crate::interactions::is_shoot_activated_switch(sprites[hu].picnum);
        if activates_switch {
            pending_switch_hits.push((body_i, None, Some(hit.hitsprite)));
        }
        checkhitsprite_shotgun(
            lists,
            sprites,
            sectors,
            walls,
            std::slice::from_ref(player),
            picnum_out,
            actor_temps,
            tile_meta,
            script,
            actortype,
            randomseed,
            hit.hitsprite,
            k,
            spawn_blood && is_bg,
        );
        if spawn_blood && is_bg {
            did_blood = true;
        }
        if hu < sprites.len() && sprites[hu].picnum == APLAYER {
            let _ = spawn_jibs6(
                lists,
                sprites,
                picnum_out,
                actor_temps,
                script,
                randomseed,
                k,
            );
            sprites[ku].xrepeat = 0;
            sprites[ku].yrepeat = 0;
        } else {
            spawn_smallsmoke(
                lists,
                sprites,
                sectors,
                walls,
                picnum_out,
                actor_temps,
                script,
                randomseed,
                k,
            );
        }
        // Chocolate returns from `shoot` immediately after checkhitswitch;
        // the rare PISTOL_RICOCHET TRAND below is not consumed on this hit.
        if activates_switch {
            return false;
        }
    } else if hit.hitwall >= 0 {
        spawn_smallsmoke(
            lists,
            sprites,
            sectors,
            walls,
            picnum_out,
            actor_temps,
            script,
            randomseed,
            k,
        );
        let original_wall_index = hit.hitwall as usize;
        if original_wall_index < walls.len() {
            let hit_wall = walls[original_wall_index];
            let switch_tile = crate::interactions::is_shoot_activated_switch(hit_wall.picnum);
            if switch_tile {
                if let Ok(wall_i) = i16::try_from(original_wall_index) {
                    pending_switch_hits.push((body_i, Some(wall_i), None));
                }
                // Chocolate's wall switch path also returns before the
                // ricochet roll and before bullet-hole / checkhitwall logic.
                return false;
            } else {
                spawn_bullethole(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum_out,
                    actor_temps,
                    script,
                    randomseed,
                    ku,
                    original_wall_index,
                    hit.hitsect,
                );
                let mut wall_index = original_wall_index;
                if walls.get(wall_index).is_some_and(|wall| {
                    wall.cstat & 2 != 0
                        && wall.nextsector >= 0
                        && sectors
                            .get(wall.nextsector as usize)
                            .is_some_and(|next| hit.hitz >= next.floorz)
                }) && let Some(peer) = walls
                    .get(wall_index)
                    .and_then(|wall| usize::try_from(wall.nextwall).ok())
                {
                    wall_index = peer;
                }
                if wall_index < walls.len() {
                    // Chocolate calls checkhitwall after the impact spark and
                    // small-smoke cue, using SHOTSPARK1 as `atwith`.
                    let wall_sounds = crate::weapons_move::apply_checkhitwall(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        picnum_out,
                        actor_temps,
                        randomseed,
                        sprites[ku],
                        k,
                        SHOTSPARK1,
                        wall_index,
                        hit.hitx,
                        hit.hity,
                        hit.hitz,
                        hit.hitsect,
                        script,
                    );
                    if actor_temps.len() <= ku {
                        actor_temps.resize_with(ku + 1, ActorConTemps::default);
                    }
                    actor_temps[ku].pending_actor_sounds.extend(wall_sounds);
                }
            }
        }
    }
    // Chocolate `shoot`: after resolving the shot, roll the rare ricochet
    // sound on every valid hitscan, including sprite and floor hits.
    if krand_step(randomseed) & 255 < 4
        && script.is_some_and(|img| img.sounds.get(1).is_some_and(Option::is_some))
    {
        // PISTOL_RICOCHET is sound id 1 in Atomic's soundefs.h.
        pending_sound_ids.push(1);
    }
    did_blood
}

/// EDuke32 `A_SetHitData` (and `Proj_InsertShotspark`'s weapon tag): the
/// spark remembers what it hit in `t_data[6..9]` and, under EDuke32, the
/// firing tile in `yvel`.
pub(crate) fn set_eduke_spark_hit_data(
    sprites: &mut [SpriteT],
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    spark: usize,
    hit: &room_build_engine::geom::HitScan,
    atwith: i16,
) {
    if !script.is_some_and(|image| image.dialect == crate::con::ConDialect::EDuke32) {
        return;
    }
    if actor_temps.len() <= spark {
        actor_temps.resize_with(spark + 1, ActorConTemps::default);
    }
    actor_temps[spark].t_data_extra = [
        i32::from(hit.hitwall),
        i32::from(hit.hitsect),
        i32::from(hit.hitsprite),
        0,
    ];
    if let Some(sprite) = sprites.get_mut(spark) {
        sprite.yvel = atwith;
    }
}

/// Chocolate `checkhitceiling` for a hit by sprite `source_i`. With an
/// EDuke32 script that hooks EVENT_DAMAGEHPLANE, or EVENT_DAMAGECEILING for a
/// `damageeventtile` ceiling, the hit is deferred to the session, which runs
/// the events and then [`check_hit_ceiling_unchecked`] unless one cancels.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_hit_ceiling(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
    sector_index: i16,
    source_i: i16,
    owner: i16,
    pending_sound_ids: &mut Vec<u16>,
) {
    if crate::weapons_move::defer_ceiling_damage(script, sectors, source_i, sector_index, owner) {
        return;
    }
    check_hit_ceiling_unchecked(
        lists,
        sprites,
        sectors,
        walls,
        picnum_out,
        actor_temps,
        script,
        randomseed,
        sector_index,
        owner,
        pending_sound_ids,
    );
}

/// Chocolate `checkhitceiling`: break the Atomic ceiling-light tiles, spawn
/// glass with the vanilla RNG order, and signal linked SE 12 / SE 3 markers.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_hit_ceiling_unchecked(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
    sector_index: i16,
    owner: i16,
    pending_sound_ids: &mut Vec<u16>,
) {
    const LIGHTS: &[(&str, &str, i16, i16)] = &[
        ("WALLLIGHT1", "WALLLIGHTBUST1", 703, 704),
        ("WALLLIGHT2", "WALLLIGHTBUST2", 705, 706),
        ("WALLLIGHT3", "WALLLIGHTBUST3", 701, 702),
        ("WALLLIGHT4", "WALLLIGHTBUST4", 124, 125),
        ("TECHLIGHT2", "TECHLIGHTBUST2", 120, 121),
        ("TECHLIGHT4", "TECHLIGHTBUST4", 122, 123),
    ];
    let tile = |name: &str| {
        script.and_then(|image| {
            image
                .labels
                .get(name)
                .filter(|label| !label.is_ptr)
                .map(|label| label.value as i16)
        })
    };
    let Ok(si) = usize::try_from(sector_index) else {
        return;
    };
    let Some(sector) = sectors.get(si).copied() else {
        return;
    };
    let Some((_, broken_name, _, broken_fallback)) = LIGHTS
        .iter()
        .find(|(name, _, original, _)| sector.ceilingpicnum == tile(name).unwrap_or(*original))
    else {
        return;
    };
    let broken_tile = tile(broken_name).unwrap_or(*broken_fallback);
    let glass_pieces = tile("GLASSPIECES").unwrap_or(1031);
    sectors[si].ceilingpicnum = broken_tile;

    // `ceilingglass(player, sector, 10)` walks all but the closing wall and
    // emits ten shards per edge, consuming angle, z, then x-velocity RNG.
    let start = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
    let end = start
        .saturating_add(usize::try_from(sector.wallnum).unwrap_or_default())
        .min(walls.len());
    if end.saturating_sub(start) >= 2 {
        for wall_index in start..end - 1 {
            let wall = walls[wall_index];
            let next = walls.get(wall_index + 1);
            let Some(next) = next else { continue };
            let dx = (next.x - wall.x) / 11;
            let dy = (next.y - wall.y) / 11;
            let (mut x, mut y) = (wall.x, wall.y);
            for shard in (1..=10).rev() {
                x = x.saturating_add(dx);
                y = y.saturating_add(dy);
                let angle = (krand_step(randomseed) & 2047) as i16;
                let z = sector.ceilingz + ((krand_step(randomseed) & 15) << 8);
                let xvel = krand_step(randomseed) & 31;
                let sprite_i = egs(
                    lists,
                    sprites,
                    picnum_out,
                    actor_temps,
                    script,
                    sector_index,
                    x,
                    y,
                    z,
                    glass_pieces + shard % 3,
                    -32,
                    36,
                    36,
                    angle,
                    xvel as i16,
                    0,
                    owner,
                    STAT_MISC,
                );
                let _ = sprite_i;
            }
        }
    }
    pending_sound_ids.push(19); // GLASS_BREAKING

    // A sector with no hitag can discover one local SE12, then extinguish all
    // SE12 markers carrying that marker's hitag, matching `checkhitceiling`.
    if sector.hitag == 0 {
        let mut local = lists.headspritesect[si];
        let mut tagged_hit = None;
        while local >= 0 {
            let li = local as usize;
            local = lists.nextspritesect[li];
            if let Some(marker) = sprites.get(li)
                && lists.sprite_stat[li] == STAT_EFFECTOR
                && marker.lotag == 12
            {
                tagged_hit = Some(marker.hitag);
                break;
            }
        }
        if let Some(hitag) = tagged_hit {
            let mut effector = lists.headspritestat[STAT_EFFECTOR as usize];
            while effector >= 0 {
                let ei = effector as usize;
                effector = lists.nextspritestat[ei];
                if sprites.get(ei).is_some_and(|marker| marker.hitag == hitag) {
                    if actor_temps.len() <= ei {
                        actor_temps.resize_with(ei + 1, ActorConTemps::default);
                    }
                    actor_temps[ei].g_t[3] = 1;
                }
            }
        }
    }

    // Chocolate consumes this TRAND even when the sector has no SE3.
    let flicker = krand_step(randomseed) & 1;
    let mut effector = lists.headspritestat[STAT_EFFECTOR as usize];
    while effector >= 0 {
        let ei = effector as usize;
        effector = lists.nextspritestat[ei];
        if sprites
            .get(ei)
            .is_some_and(|marker| marker.lotag == 3 && marker.hitag == sector.hitag)
        {
            if actor_temps.len() <= ei {
                actor_temps.resize_with(ei + 1, ActorConTemps::default);
            }
            actor_temps[ei].g_t[2] = flicker;
            actor_temps[ei].g_t[4] = 1;
        }
    }
}

/// Chocolate shotgun branch of `processinput` SHOOTINCODE (kickback + fire).
fn tick_weapon_pos(p: &mut Player) {
    if p.weapon_pos != 0 {
        if p.weapon_pos == -9 {
            // Chocolate: a holstered weapon (toilet use, holster key) stays
            // down until `holster_weapon` clears; a pending switch raises.
            if p.last_weapon >= 0 {
                p.weapon_pos = 10;
                p.last_weapon = -1;
            } else if p.holster_weapon == 0 {
                p.weapon_pos = 10;
            }
        } else {
            p.weapon_pos -= 1;
        }
    }
}

/// Advance the common raise/lower animation for weapons whose firing logic is
/// not implemented yet. This prevents selecting an unsupported weapon from
/// incorrectly firing the shotgun.
pub fn process_unimplemented_weapon(p: &mut Player) {
    tick_weapon_pos(p);
}

/// Atomic knee weapon: starts immediately, hits on kickback tic 7, and repeats
/// after tic 14 while fire remains held.
pub fn process_weapons_knee(
    p: &mut Player,
    sync_bits: u32,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    actortype: &[u8],
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
) -> Option<KneeHit> {
    const WEAPON: i16 = 0;
    tick_weapon_pos(p);
    if p.curr_weapon != WEAPON {
        return None;
    }
    let fire = sync_bits & (1 << 2) != 0;
    if p.kickback_pic == 0 {
        if fire && p.last_weapon == -1 && p.weapon_pos == 0 {
            p.kickback_pic = 1;
        }
        return None;
    }
    p.kickback_pic += 1;
    if p.kickback_pic == 7 {
        return shoot_knee_player(
            p,
            sprites,
            sectors,
            walls,
            actor_temps,
            tile_meta,
            actortype,
            script,
            randomseed,
        );
    }
    if p.kickback_pic == 14 {
        p.kickback_pic = if fire {
            1 + (krand_step(randomseed) & 3) as i16
        } else {
            0
        };
    }
    None
}

/// Result of Chocolate `shoot(player, KNEE)` reaching a wall or sprite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KneeHit {
    /// Wall reached by the knee hitscan, if any.
    pub wall: Option<i16>,
    /// Sprite reached by the knee hitscan, if any.
    pub sprite: Option<i16>,
    /// Build sector of the impact.
    pub sector: i16,
    /// Build-space point where the knee hitscan ended.
    pub position: [i32; 3],
    /// Damage carried by the spawned KNEE hit actor, including player bonuses.
    pub damage: i16,
    /// KNEE CON strength plus the random roll, before player's steroid bonus.
    pub impact_strength: i16,
}

/// Atomic `shoot(player, KNEE)` hitscan subset. Returns a hit target when the
/// short reach test connects; this is also the `KICK_HIT` sound condition.
fn shoot_knee_player(
    p: &Player,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    actortype: &[u8],
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
) -> Option<KneeHit> {
    let Some(body) = usize::try_from(p.sprite_i)
        .ok()
        .filter(|&i| i < sprites.len())
    else {
        return None;
    };
    let sect = sprites[body].sectnum;
    if sect < 0 || sect as usize >= sectors.len() {
        return None;
    }
    let (sin, cos) = sincos_q14(i32::from(p.ang.wrapping_add(15)));
    let z = p.posz + p.pyoff + (10 << 8);
    let vz = ((100 - i32::from(p.horiz) - i32::from(p.horizoff)) << 5) << 6;
    let sec_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let sprite_hits: Vec<SpriteHit> = sprites
        .iter()
        .map(|s| {
            let meta = tile_meta
                .get(s.picnum.max(0) as usize)
                .copied()
                .unwrap_or_default();
            SpriteHit {
                x: s.x,
                y: s.y,
                z: s.z,
                sectnum: s.sectnum,
                clipdist: 0,
                cstat: s.cstat,
                face_span: i32::from(meta.width) * i32::from(s.xrepeat),
                face_height: (i32::from(meta.height) * i32::from(s.yrepeat)) << 2,
            }
        })
        .collect();
    let body_cstat = sprites[body].cstat;
    sprites[body].cstat &= !257;
    let hit = hitscan_full(
        p.posx,
        p.posy,
        z,
        sect,
        cos,
        sin,
        vz,
        CLIPMASK1,
        &sec_walls,
        &wall_xy,
        &sprite_hits,
        Some(&sec_z),
    );
    sprites[body].cstat = body_cstat;
    if hit.hitsect < 0
        || hit.hitwall < 0 && hit.hitsprite < 0
        || hit
            .hitx
            .wrapping_sub(p.posx)
            .saturating_abs()
            .saturating_add(hit.hity.wrapping_sub(p.posy).saturating_abs())
            >= 1024
    {
        return None;
    }
    let knee_strength = script.map_or(10, |image| init_actor_temps_from_script(image, KNEE).0);
    let random_bonus = krand_step(randomseed) & 7;
    let impact_strength = i32::from(knee_strength) + random_bonus;
    let steroid_bonus = if p.steroids_amount > 0 && p.steroids_amount < 400 {
        script
            .and_then(|image| image.gamestartup.get(2).copied())
            .unwrap_or(100)
            >> 2
    } else {
        0
    };
    let damage =
        (impact_strength + steroid_bonus).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    let impact_strength = impact_strength.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    if let Ok(target) = usize::try_from(hit.hitsprite)
        && target < sprites.len()
        && badguy_pic(sprites[target].picnum, actortype)
    {
        if actor_temps.len() <= target {
            actor_temps.resize_with(target + 1, ActorConTemps::default);
        }
        let temp = &mut actor_temps[target];
        temp.hit_picnum = KNEE;
        temp.hit_extra = if temp.hit_extra < 0 {
            damage
        } else {
            temp.hit_extra.saturating_add(damage)
        };
        temp.hit_owner = p.sprite_i;
        temp.hit_ang = p.ang.wrapping_add(15);
        temp.hit_source = p.sprite_i;
        temp.hit_radius = crate::con::direct_hit_payload();
    }
    Some(KneeHit {
        wall: (hit.hitwall >= 0).then_some(hit.hitwall),
        sprite: (hit.hitsprite >= 0).then_some(hit.hitsprite),
        sector: hit.hitsect,
        position: [hit.hitx, hit.hity, hit.hitz],
        damage,
        impact_strength,
    })
}

/// Advance and fire Atomic's RPG weapon (weapon slot 4), including the shared
/// Chocolate aim cone, target height solve, projectile launch offset and cadence.
pub fn process_weapons_rpg(
    p: &mut Player,
    sync_bits: u32,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_out: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
) {
    const WEAPON: i16 = 4;
    tick_weapon_pos(p);
    if p.curr_weapon != WEAPON {
        return;
    }
    let fire = sync_bits & (1 << 2) != 0;
    let rpg_tile = script_tile_or(script, "RPG", RPG_PROJECTILE);
    if p.kickback_pic == 0 {
        if fire
            && p.last_weapon == -1
            && p.weapon_pos == 0
            && p.ammo.get(WEAPON as usize).copied().unwrap_or(0) > 0
        {
            p.kickback_pic = 1;
        }
        return;
    }
    p.kickback_pic += 1;
    if p.kickback_pic == 2 {
        let _ = shoot_rpg_player(
            p,
            rpg_tile,
            lists,
            sprites,
            sectors,
            walls,
            actor_temps,
            picnum_out,
            tile_meta,
            script,
            actortype,
            randomseed,
        );
        p.ammo[WEAPON as usize] = p.ammo[WEAPON as usize].saturating_sub(1);
        if p.ammo[WEAPON as usize] == 0 {
            checkavailweapon(p);
        }
    }
    if p.kickback_pic >= 20 {
        p.kickback_pic = 0;
    }
}

/// Chocolate `shoot(RPG)` for a player: the aimed rocket projectile.
/// Returns the new sprite, or -1.
#[allow(clippy::too_many_arguments)]
pub(crate) fn shoot_rpg_player(
    p: &Player,
    rpg_tile: i16,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_out: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
) -> i16 {
    let mut shot = -1;
    if let Some(body) = (p.sprite_i >= 0)
        .then_some(p.sprite_i as usize)
        .filter(|&i| i < sprites.len())
    {
        let source = sprites[body];
        let mut shot_ang = i32::from(p.ang);
        let mut z = p.posz + p.pyoff + (4 << 8);
        let target = aim(
            &source,
            AUTO_AIM_ANGLE,
            p.auto_aim == 2,
            lists,
            sprites,
            sectors,
            walls,
            actortype,
            Some(p),
        );
        let mut zvel = (100 - i32::from(p.horiz) - i32::from(p.horizoff)) * 81;
        if target >= 0 {
            let target_sprite = &sprites[target as usize];
            let height = tile_meta
                .get(target_sprite.picnum.max(0) as usize)
                .map_or(0, |meta| i32::from(meta.height));
            let dal = (i32::from(target_sprite.xrepeat) * height * 2) + (8 << 8);
            zvel = ((target_sprite.z - z - dal) * 644) / ldist(&source, target_sprite).max(1);
            if target_sprite.picnum != RECON {
                shot_ang = getangle(
                    target_sprite.x.wrapping_sub(p.posx),
                    target_sprite.y.wrapping_sub(p.posy),
                );
            }
        }
        z += 3 << 8;
        let sect = source.sectnum;
        if sect >= 0 && (sect as usize) < sectors.len() {
            let k = lists.insertsprite(sect, STAT_PROJECTILE);
            if k >= 0 && (k as usize) < sprites.len() {
                let i = k as usize;
                shot = k;
                let projectile_z = z - (1 << 8);
                let (base_extra, temps) = script
                    .map(|s| crate::con::init_actor_temps_from_script(s, rpg_tile))
                    .unwrap_or_default();
                sprites[i] = SpriteT {
                    x: p.posx + sincos_q14(shot_ang + 348).1 / 448,
                    y: p.posy + sincos_q14(shot_ang + 348).0 / 448,
                    z: projectile_z,
                    ang: shot_ang as i16,
                    sectnum: sect,
                    picnum: rpg_tile,
                    shade: if source.extra >= 0 { -96 } else { 0 },
                    xrepeat: 16,
                    yrepeat: 16,
                    xvel: 644,
                    zvel: zvel as i16,
                    owner: p.sprite_i,
                    yvel: target,
                    cstat: 128,
                    clipdist: 4,
                    extra: base_extra.saturating_add((krand_step(randomseed) & 7) as i16),
                    ..Default::default()
                };
                if picnum_out.len() <= i {
                    picnum_out.resize(i + 1, 0);
                }
                picnum_out[i] = rpg_tile;
                if actor_temps.len() <= i {
                    actor_temps.resize_with(i + 1, ActorConTemps::default);
                }
                actor_temps[i] = temps;
            } else if k >= 0 {
                let _ = lists.deletesprite(k);
            }
        }
    }
    shot
}

/// Chocolate `shoot(FREEZEBLAST)` for a player. Returns the new sprite,
/// or -1.
#[allow(clippy::too_many_arguments)]
pub(crate) fn shoot_freezeblast_player(
    p: &Player,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_out: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
) -> i16 {
    let mut shot = -1;
    if let Some(body) = (p.sprite_i >= 0)
        .then_some(p.sprite_i as usize)
        .filter(|&i| i < sprites.len())
    {
        let source = sprites[body];
        let mut shot_ang = i32::from(p.ang);
        let mut z = p.posz + p.pyoff + (7 << 8);
        let target = aim(
            &source,
            AUTO_AIM_ANGLE,
            p.auto_aim == 2,
            lists,
            sprites,
            sectors,
            walls,
            actortype,
            Some(p),
        );
        let mut zvel = (100 - i32::from(p.horiz) - i32::from(p.horizoff)) * 81;
        if target >= 0 {
            let target = &sprites[target as usize];
            let height = tile_meta
                .get(target.picnum.max(0) as usize)
                .map_or(0, |meta| i32::from(meta.height));
            let dal = (i32::from(target.xrepeat) * height * 2) + (8 << 8);
            let distance = ldist(&source, target).max(1);
            zvel = ((target.z - z - dal) * 644) / distance;
            if target.picnum != RECON {
                shot_ang = getangle(target.x.wrapping_sub(p.posx), target.y.wrapping_sub(p.posy));
            }
        }
        z += 3 << 8;
        let sect = source.sectnum;
        if sect >= 0 && (sect as usize) < sectors.len() {
            let k = lists.insertsprite(sect, STAT_PROJECTILE);
            if k >= 0 && (k as usize) < sprites.len() {
                let i = k as usize;
                shot = k;
                let projectile_z = z - (1 << 8);
                let (extra, temps) = script
                    .map(|s| crate::con::init_actor_temps_from_script(s, FREEZEBLAST))
                    .unwrap_or_default();
                sprites[i] = SpriteT {
                    x: p.posx + sincos_q14(shot_ang + 348).1 / 448,
                    y: p.posy + sincos_q14(shot_ang + 348).0 / 448,
                    z: projectile_z,
                    ang: shot_ang as i16,
                    sectnum: sect,
                    picnum: FREEZEBLAST,
                    shade: -127,
                    xrepeat: 7,
                    yrepeat: 7,
                    xvel: 644,
                    zvel: (zvel - (2 << 4)) as i16,
                    owner: p.sprite_i,
                    cstat: 128,
                    clipdist: 4,
                    yvel: script
                        .and_then(|image| {
                            let index = if image.con_version == crate::con::ConVersion::V15 {
                                26
                            } else {
                                24
                            };
                            image.gamestartup.get(index).copied()
                        })
                        .unwrap_or(3)
                        .clamp(0, i32::from(i16::MAX)) as i16,
                    extra: extra.saturating_add((krand_step(randomseed) & 7) as i16),
                    ..Default::default()
                };
                if picnum_out.len() <= i {
                    picnum_out.resize(i + 1, 0);
                }
                picnum_out[i] = FREEZEBLAST;
                if actor_temps.len() <= i {
                    actor_temps.resize_with(i + 1, ActorConTemps::default);
                }
                actor_temps[i] = temps;
            } else if k >= 0 {
                let _ = lists.deletesprite(k);
            }
        }
    }
    shot
}

/// Chocolate `shoot(SHRINKER)` (a moving SHRINKSPARK) or `shoot(GROWSPARK)`
/// (a hitscan with its one-tic impact cue) for a player. Returns the
/// SHRINKSPARK or impact sprite, or -1.
#[allow(clippy::too_many_arguments)]
pub(crate) fn shoot_shrink_grow_player(
    p: &Player,
    grower: bool,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_out: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
) -> i16 {
    const SHRINKER: i16 = 6;
    const GROWER: i16 = 11;
    let weapon = if grower { GROWER } else { SHRINKER };
    let mut shot = -1;
    let body = (p.sprite_i >= 0)
        .then_some(p.sprite_i as usize)
        .filter(|&i| i < sprites.len());
    if let Some(body) = body {
        let source = sprites[body];
        let mut shot_ang = i32::from(p.ang);
        let (mut sin, mut cos) = sincos_q14(shot_ang);
        let base_z = p.posz + p.pyoff + (4 << 8);
        let target = aim(
            &source,
            AUTO_AIM_ANGLE,
            p.auto_aim == 2,
            lists,
            sprites,
            sectors,
            walls,
            actortype,
            Some(p),
        );
        let mut zvel;
        if target >= 0 {
            let target_sprite = &sprites[target as usize];
            shot_ang = getangle(
                target_sprite.x.wrapping_sub(p.posx),
                target_sprite.y.wrapping_sub(p.posy),
            );
            let height = tile_meta
                .get(target_sprite.picnum.max(0) as usize)
                .map_or(0, |meta| i32::from(meta.height));
            let mut dal = (i32::from(target_sprite.xrepeat) * height * 2)
                + if weapon == GROWER { 5 << 8 } else { 0 };
            if weapon == GROWER && matches!(target_sprite.picnum, 2370..=2377 | 2360) {
                dal -= 8 << 8;
            }
            let distance = ldist(&source, target_sprite).max(1);
            zvel = if weapon == GROWER {
                ((target_sprite.z - base_z - dal) << 8) / distance
            } else {
                ((target_sprite.z - base_z - dal - (4 << 8)) * 768) / distance
            };
            (sin, cos) = sincos_q14(shot_ang);
        } else if weapon == GROWER {
            shot_ang += 16 - (krand_step(randomseed) & 31);
            zvel = (100 - i32::from(p.horiz) - i32::from(p.horizoff)) << 5;
            zvel += 128 - (krand_step(randomseed) & 255);
            (sin, cos) = sincos_q14(shot_ang);
        } else {
            zvel = (100 - i32::from(p.horiz) - i32::from(p.horizoff)) * 98;
        }
        let z = if weapon == GROWER {
            base_z - (2 << 8)
        } else {
            base_z
        };
        if weapon == SHRINKER {
            let sect = source.sectnum;
            if sect >= 0 && (sect as usize) < sectors.len() {
                let k = lists.insertsprite(sect, STAT_PROJECTILE);
                if k >= 0 && (k as usize) < sprites.len() {
                    let i = k as usize;
                    shot = k;
                    let (extra, temps) = script
                        .map(|s| crate::con::init_actor_temps_from_script(s, SHRINKSPARK))
                        .unwrap_or_default();
                    sprites[i] = SpriteT {
                        x: source.x - sincos_q14(shot_ang).0 / 4096,
                        y: source.y + sincos_q14(shot_ang + 348).0 / 4096,
                        z: z + (2 << 8),
                        ang: shot_ang as i16,
                        sectnum: sect,
                        picnum: SHRINKSPARK,
                        shade: -16,
                        xrepeat: 28,
                        yrepeat: 28,
                        xvel: 768,
                        zvel: zvel as i16,
                        owner: p.sprite_i,
                        cstat: 128,
                        clipdist: 32,
                        extra,
                        ..Default::default()
                    };
                    if picnum_out.len() <= i {
                        picnum_out.resize(i + 1, 0);
                    }
                    picnum_out[i] = SHRINKSPARK;
                    if actor_temps.len() <= i {
                        actor_temps.resize_with(i + 1, ActorConTemps::default);
                    }
                    actor_temps[i] = temps;
                } else if k >= 0 {
                    let _ = lists.deletesprite(k);
                }
            }
        } else {
            let sec_walls: Vec<SectorWalls> = sectors
                .iter()
                .map(|s| SectorWalls {
                    wallptr: s.wallptr,
                    wallnum: s.wallnum,
                })
                .collect();
            let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
            let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
            let sprite_hits: Vec<SpriteHit> = sprites
                .iter()
                .enumerate()
                .map(|(index, s)| {
                    let meta = tile_meta
                        .get(s.picnum.max(0) as usize)
                        .copied()
                        .unwrap_or_default();
                    let mut hit = SpriteHit::cylinder(
                        s.x,
                        s.y,
                        s.z,
                        s.sectnum,
                        i32::from(s.clipdist) << 2,
                        s.cstat,
                    );
                    hit.face_span = i32::from(meta.width) * i32::from(s.xrepeat);
                    hit.face_height = (i32::from(meta.height) * i32::from(s.yrepeat)) << 2;
                    if index == body {
                        hit.cstat &= !257;
                    }
                    hit
                })
                .collect();
            let hit = hitscan_full(
                source.x,
                source.y,
                z,
                source.sectnum,
                cos,
                sin,
                zvel << 6,
                CLIPMASK1,
                &sec_walls,
                &wall_xy,
                &sprite_hits,
                Some(&sec_z),
            );
            if hit.hitsect >= 0 {
                let effect = egs(
                    lists,
                    sprites,
                    picnum_out,
                    actor_temps,
                    script,
                    hit.hitsect,
                    hit.hitx,
                    hit.hity,
                    hit.hitz,
                    GROWSPARK,
                    -16,
                    1,
                    1,
                    shot_ang as i16,
                    0,
                    0,
                    p.sprite_i,
                    STAT_PROJECTILE,
                );
                if effect >= 0 {
                    shot = effect;
                    let ei = effect as usize;
                    sprites[ei].pal = 2;
                    sprites[ei].cstat |= 130;
                }
                if hit.hitsprite >= 0 {
                    let target = hit.hitsprite as usize;
                    if target < sprites.len() {
                        if actor_temps.len() <= target {
                            actor_temps.resize_with(target + 1, ActorConTemps::default);
                        }
                        actor_temps[target].hit_picnum = GROWSPARK;
                        actor_temps[target].hit_extra = 0;
                        actor_temps[target].hit_owner = p.sprite_i;
                        actor_temps[target].hit_ang = shot_ang as i16;
                    }
                }
            }
        }
    }
    shot
}

/// Advance Atomic's Freezethrower cadence (weapon slot 9). Chocolate fires
/// one FREEZEBLAST on kickback tic 3, consumes one cell, then permits an
/// automatic repeat while fire remains held after the short recovery.
pub fn process_weapons_freezer(
    p: &mut Player,
    sync_bits: u32,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    tile_meta: &[TileZMeta],
    actortype: &[u8],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_out: &mut Vec<i16>,
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
    pending_sound_ids: &mut Vec<u16>,
) {
    const WEAPON: i16 = 9;
    tick_weapon_pos(p);
    if p.curr_weapon != WEAPON {
        return;
    }
    let fire = sync_bits & (1 << 2) != 0;
    if p.kickback_pic == 0 {
        if fire && p.last_weapon == -1 && p.weapon_pos == 0 && p.ammo[WEAPON as usize] > 0 {
            p.kickback_pic = 1;
            if let Some(sound_id) = script
                .and_then(|image| image.labels.get("CAT_FIRE"))
                .and_then(|label| u16::try_from(label.value).ok())
            {
                pending_sound_ids.push(sound_id);
            }
        }
        // Start and advance are exclusive in one Chocolate `processweapon`.
        return;
    }
    // Chocolate checks the recovery/repeat branch before advancing a new
    // firing cycle. Capture that state so reaching tic 4 doesn't rearm early.
    let rearming = p.kickback_pic >= 4;
    if p.kickback_pic < 4 {
        p.kickback_pic += 1;
    }
    if p.kickback_pic == 3 {
        let _ = shoot_freezeblast_player(
            p,
            lists,
            sprites,
            sectors,
            walls,
            actor_temps,
            picnum_out,
            tile_meta,
            script,
            actortype,
            randomseed,
        );
        p.ammo[WEAPON as usize] = p.ammo[WEAPON as usize].saturating_sub(1);
        if p.ammo[WEAPON as usize] == 0 {
            checkavailweapon(p);
        }
        if p.sprite_i >= 0
            && sprites
                .get(p.sprite_i as usize)
                .is_some_and(|body| body.xrepeat < 32)
        {
            p.kickback_pic = 0;
            return;
        }
    }
    if rearming {
        if fire && p.ammo[WEAPON as usize] > 0 {
            p.kickback_pic = 1;
            if let Some(sound_id) = script
                .and_then(|image| image.labels.get("CAT_FIRE"))
                .and_then(|label| u16::try_from(label.value).ok())
            {
                pending_sound_ids.push(sound_id);
            }
        } else {
            p.kickback_pic = 0;
        }
    }
}

/// Advance the Shrinker and Grower firing states (slots 6 and 11).
/// Shrinker emits a moving SHRINKSPARK every 12 kickback tics; Grower uses a
/// hitscan and leaves its one-tic impact cue at the resolved hit point.
pub fn process_weapons_shrink_grow(
    p: &mut Player,
    sync_bits: u32,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_out: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
    pending_sound_ids: &mut Vec<u16>,
) {
    const SHRINKER: i16 = 6;
    const GROWER: i16 = 11;
    tick_weapon_pos(p);
    let weapon = p.curr_weapon;
    if weapon != SHRINKER && weapon != GROWER {
        return;
    }
    let fire = sync_bits & (1 << 2) != 0;
    if p.kickback_pic == 0 {
        if fire && p.last_weapon == -1 && p.weapon_pos == 0 && p.ammo[weapon as usize] > 0 {
            p.kickback_pic = 1;
            let sound = if weapon == GROWER {
                "EXPANDERSHOOT"
            } else {
                "SHRINKER_FIRE"
            };
            if let Some(sound_id) = script
                .and_then(|image| image.labels.get(sound))
                .and_then(|label| u16::try_from(label.value).ok())
            {
                pending_sound_ids.push(sound_id);
            }
            return;
        } else {
            return;
        }
    }
    p.kickback_pic += 1;
    // The activation tic stores frame 1 but does not enter Chocolate's
    // nonzero-kickback arm; the following tics increment first, so our
    // post-increment edge is one greater than its pre-increment threshold.
    let fire_tic = if weapon == GROWER { 5 } else { 12 };
    if p.kickback_pic == fire_tic {
        let _ = shoot_shrink_grow_player(
            p,
            weapon == GROWER,
            lists,
            sprites,
            sectors,
            walls,
            actor_temps,
            picnum_out,
            tile_meta,
            script,
            actortype,
            randomseed,
        );
        p.ammo[weapon as usize] = p.ammo[weapon as usize].saturating_sub(1);
        if p.ammo[weapon as usize] == 0 {
            checkavailweapon(p);
        }
    }
    let cycle = if weapon == GROWER { 4 } else { 11 };
    if p.kickback_pic >= cycle {
        p.kickback_pic = if fire && p.ammo[weapon as usize] > 0 {
            1
        } else {
            0
        };
    }
}

/// Advance Atomic's hand bomb / remote cycle (weapon slots 5 and 10).
///
/// Throw strength follows Chocolate `player.c::shoot`: the held-fire count
/// biases horizontal velocity, while horizon sets vertical velocity. The
/// actor-side bounce and detonation behavior is handled by `move_pipebombs`.
pub fn process_weapons_pipebomb(
    p: &mut Player,
    sync_bits: u32,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_out: &mut Vec<i16>,
    script: Option<&ScriptImage>,
) {
    const HANDBOMB: i16 = 5;
    const REMOTE: i16 = 10;
    tick_weapon_pos(p);
    if p.curr_weapon != HANDBOMB && p.curr_weapon != REMOTE {
        return;
    }
    let fire = sync_bits & (1 << 2) != 0;
    let heavybomb_tile = script_tile_or(script, "HEAVYHBOMB", HEAVYHBOMB);
    if p.rapid_fire_hold {
        if fire {
            return;
        }
        p.rapid_fire_hold = false;
    }
    if p.kickback_pic == 0 {
        if fire && p.last_weapon == -1 && p.weapon_pos == 0 {
            if p.curr_weapon == REMOTE || p.ammo[HANDBOMB as usize] > 0 {
                p.hbomb_hold_delay = 0;
                p.kickback_pic = 1;
            }
        }
        return;
    }
    if p.curr_weapon == HANDBOMB {
        if p.kickback_pic == 6 && fire {
            p.rapid_fire_hold = true;
            return;
        }
        p.kickback_pic += 1;
        if p.kickback_pic == 12 {
            p.ammo[HANDBOMB as usize] = p.ammo[HANDBOMB as usize].saturating_sub(1);
            if p.sprite_i >= 0 {
                let owner = p.sprite_i;
                let sect = sprites
                    .get(owner as usize)
                    .map_or(p.cursectnum, |s| s.sectnum);
                if sect >= 0 && (sect as usize) < sectors.len() {
                    let low_throw = p.on_ground && (sync_bits & 2 != 0);
                    let strength = if low_throw { 15 } else { 140 };
                    let zvel = if low_throw {
                        (i32::from(p.horiz) + i32::from(p.horizoff) - 100) * 20
                    } else {
                        -512 - (i32::from(p.horiz) + i32::from(p.horizoff) - 100) * 20
                    };
                    let (sin, cos) = sincos_q14(i32::from(p.ang));
                    let bomb = egs(
                        lists,
                        sprites,
                        picnum_out,
                        actor_temps,
                        script,
                        sect,
                        p.posx + (cos >> 6),
                        p.posy + (sin >> 6),
                        p.posz + if low_throw { 8 << 8 } else { 0 },
                        heavybomb_tile,
                        -16,
                        9,
                        9,
                        p.ang,
                        strength + (p.hbomb_hold_delay << 5),
                        zvel as i16,
                        owner,
                        crate::stat::STAT_ACTOR,
                    );
                    if bomb >= 0 {
                        if let Some(sprite) = sprites.get_mut(bomb as usize) {
                            // Chocolate EGS initializes this C actor with one
                            // point of blast strength; it is not a CON health.
                            sprite.extra = 1;
                            if low_throw {
                                sprite.yvel = 3;
                            }
                        }
                        if let Some(source) = sprites.get(owner as usize).copied() {
                            let sector_walls: Vec<SectorWalls> = sectors
                                .iter()
                                .map(|sector| SectorWalls {
                                    wallptr: sector.wallptr,
                                    wallnum: sector.wallnum,
                                })
                                .collect();
                            let sector_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
                            let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
                            // Chocolate `hits(pi)` slows a just-thrown bomb
                            // and reverses it when the player faces a nearby
                            // wall. Use walls only: actor sprite hits here
                            // would include the newly spawned bomb itself.
                            if crate::actors::hits_xy(
                                &source,
                                &[],
                                &sector_walls,
                                &wall_xy,
                                &sector_z,
                            ) < 512
                            {
                                if let Some(sprite) = sprites.get_mut(bomb as usize) {
                                    sprite.ang = (sprite.ang + 1024) & 2047;
                                    sprite.zvel /= 3;
                                    sprite.xvel /= 3;
                                }
                            }
                        }
                    }
                    p.hbomb_on = true;
                }
            }
        } else if p.kickback_pic < 12 && fire {
            p.hbomb_hold_delay = p.hbomb_hold_delay.saturating_add(1);
        } else if p.kickback_pic > 19 {
            p.kickback_pic = 0;
            p.curr_weapon = REMOTE;
            p.last_weapon = -1;
            p.weapon_pos = 10;
        }
    } else {
        p.kickback_pic += 1;
        if p.kickback_pic == 2 {
            p.hbomb_on = false;
        }
        if p.kickback_pic == 10 {
            p.kickback_pic = 0;
            if p.ammo[HANDBOMB as usize] > 0 {
                p.curr_weapon = HANDBOMB;
                p.last_weapon = -1;
                p.weapon_pos = -1;
            } else {
                checkavailweapon(p);
            }
        }
    }
}

/// Chocolate `doincrements`: fire the player-triggered quick kick on the tic
/// its countdown reaches eight. Unlike the normal knee weapon state machine,
/// this does not change `kickback_pic` or require the fire button.
#[allow(clippy::too_many_arguments)]
pub fn process_quick_kick(
    p: &Player,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    actortype: &[u8],
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
) -> Option<KneeHit> {
    shoot_knee_player(
        p,
        sprites,
        sectors,
        walls,
        actor_temps,
        tile_meta,
        actortype,
        script,
        randomseed,
    )
}

/// Advance Atomic's Devastator burst (`player.c`, `DEVISTATOR_WEAPON`). It
/// fires two spread RPG projectiles on kickback tics 3 and 5, alternating the
/// barrel offset and consuming one ammo unit for each projectile.
pub fn process_weapons_devastator(
    p: &mut Player,
    sync_bits: u32,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_out: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
    pending_sound_ids: &mut Vec<u16>,
) {
    const WEAPON: i16 = 7;
    tick_weapon_pos(p);
    if p.curr_weapon != WEAPON {
        return;
    }
    let fire = sync_bits & (1 << 2) != 0;
    if p.kickback_pic == 0 {
        if fire
            && p.last_weapon == -1
            && p.weapon_pos == 0
            && p.ammo.get(WEAPON as usize).copied().unwrap_or(0) > 0
        {
            p.kickback_pic = 1;
            p.hbomb_hold_delay ^= 1;
            if let Some(sound_id) = script
                .and_then(|image| image.labels.get("CAT_FIRE"))
                .and_then(|label| u16::try_from(label.value).ok())
            {
                pending_sound_ids.push(sound_id);
            }
        }
        // Start and advance are exclusive in one Chocolate `processweapon`.
        return;
    }

    p.kickback_pic += 1;
    if p.kickback_pic & 1 != 0 {
        let _ = spawn_devastator_projectile(
            p,
            lists,
            sprites,
            sectors,
            walls,
            actor_temps,
            picnum_out,
            tile_meta,
            script,
            actortype,
            randomseed,
        );
        p.ammo[WEAPON as usize] = p.ammo[WEAPON as usize].saturating_sub(1);
        if p.ammo[WEAPON as usize] == 0 {
            checkavailweapon(p);
        }
    }
    if p.kickback_pic > 5 {
        p.kickback_pic = 0;
    }
}

/// Advance Atomic tripbomb placement (weapon slot 8). The player may fire only
/// when the laser reaches a nearby, non-sprite wall in a safe sector.
pub fn process_weapons_tripbomb(
    p: &mut Player,
    sync_bits: u32,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    pending_sound_ids: &mut Vec<u16>,
) {
    const WEAPON: i16 = 8;
    tick_weapon_pos(p);
    if p.curr_weapon != WEAPON {
        return;
    }
    let fire = sync_bits & (1 << 2) != 0;
    if p.kickback_pic == 0 {
        if fire
            && p.last_weapon == -1
            && p.weapon_pos == 0
            && p.ammo[WEAPON as usize] > 0
            && tripbomb_target(p, lists, sprites, sectors, walls, tile_meta, script).is_some()
        {
            p.kickback_pic = 1;
        }
        return;
    }

    if p.kickback_pic < 4 {
        p.posz = p.oposz;
        p.poszv = 0;
        if p.kickback_pic == 3
            && let Some(target) =
                tripbomb_target(p, lists, sprites, sectors, walls, tile_meta, script)
        {
            if sprites.get(p.sprite_i.max(0) as usize).is_none() {
                p.kickback_pic += 1;
                return;
            }
            let bomb = crate::shoot::egs(
                lists,
                sprites,
                picnum_out,
                actor_temps,
                script,
                target.hitsect,
                target.hitx,
                target.hity,
                target.hitz,
                script
                    .and_then(|script| script.labels.get("TRIPBOMB"))
                    .filter(|label| !label.is_ptr)
                    .map_or(2566, |label| label.value as i16),
                -16,
                4,
                5,
                p.ang,
                0,
                0,
                p.sprite_i,
                crate::stat::STAT_STANDABLE,
            );
            if let Ok(bomb_index) = usize::try_from(bomb)
                && let Some(bomb_sprite) = sprites.get_mut(bomb_index)
            {
                bomb_sprite.hitag = bomb;
                bomb_sprite.xvel = -20;
                bomb_sprite.cstat = 16;
                // Chocolate calls SSP using the player's firing angle before
                // it rotates the planted actor to face along the wall.
                let (sin, cos) = sincos_q14(i32::from(p.ang));
                bomb_sprite.x = bomb_sprite.x.wrapping_sub((20 * cos) >> 14);
                bomb_sprite.y = bomb_sprite.y.wrapping_sub((20 * sin) >> 14);
                let sector_walls: Vec<SectorWalls> = sectors
                    .iter()
                    .map(|sector| SectorWalls {
                        wallptr: sector.wallptr,
                        wallnum: sector.wallnum,
                    })
                    .collect();
                let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
                let mut new_sector = bomb_sprite.sectnum;
                if updatesector(
                    bomb_sprite.x,
                    bomb_sprite.y,
                    &mut new_sector,
                    &sector_walls,
                    &wall_xy,
                ) >= 0
                    && new_sector != bomb_sprite.sectnum
                {
                    let _ = lists.changespritesect(bomb, new_sector);
                    bomb_sprite.sectnum = new_sector;
                }
                let wall = &walls[target.hitwall as usize];
                let end = walls[wall.point2.max(0) as usize];
                let angle = getangle(wall.x.wrapping_sub(end.x), wall.y.wrapping_sub(end.y))
                    .wrapping_sub(512) as i16;
                bomb_sprite.ang = angle;
                if actor_temps.len() <= bomb_index {
                    actor_temps.resize_with(bomb_index + 1, ActorConTemps::default);
                }
                actor_temps[bomb_index].g_t[0] = 17; // Atomic map-spawn baseline.
                actor_temps[bomb_index].g_t[2] = 0;
                actor_temps[bomb_index].g_t[5] = i32::from(angle);
                pending_sound_ids.push(15); // LASERTRIP_ONWALL.
                p.ammo[WEAPON as usize] = p.ammo[WEAPON as usize].saturating_sub(1);
            } else if bomb >= 0 {
                let _ = lists.deletesprite(bomb);
            }
        }
    }
    if p.kickback_pic == 16 {
        p.kickback_pic = 0;
        crate::weapons::checkavailweapon(p);
        p.weapon_pos = -9;
    } else {
        p.kickback_pic += 1;
    }
}

#[derive(Clone, Copy)]
struct TripbombTarget {
    hitsect: i16,
    hitwall: i16,
    hitx: i32,
    hity: i32,
    hitz: i32,
}

fn tripbomb_target(
    p: &Player,
    lists: &SpriteLists,
    sprites: &[SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
) -> Option<TripbombTarget> {
    let body = usize::try_from(p.sprite_i)
        .ok()
        .filter(|&index| index < sprites.len())?;
    let sect = sprites[body].sectnum;
    if sect < 0 || sect as usize >= sectors.len() {
        return None;
    }
    let (sin, cos) = sincos_q14(i32::from(p.ang));
    let zvel = (100 - i32::from(p.horiz) - i32::from(p.horizoff)) << 5;
    let sec_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let mut sprite_hits: Vec<SpriteHit> = sprites
        .iter()
        .map(|sprite| {
            let meta = tile_meta
                .get(sprite.picnum.max(0) as usize)
                .copied()
                .unwrap_or_default();
            SpriteHit {
                x: sprite.x,
                y: sprite.y,
                z: sprite.z,
                sectnum: sprite.sectnum,
                clipdist: 0,
                cstat: sprite.cstat,
                face_span: i32::from(meta.width) * i32::from(sprite.xrepeat),
                face_height: (i32::from(meta.height) * i32::from(sprite.yrepeat)) << 2,
            }
        })
        .collect();
    sprite_hits[body].cstat &= !257;
    let hit = hitscan_full(
        p.posx,
        p.posy,
        p.posz,
        sect,
        cos,
        sin,
        zvel << 6,
        CLIPMASK1,
        &sec_walls,
        &wall_xy,
        &sprite_hits,
        Some(&sec_z),
    );
    let hit_sector = sectors.get(hit.hitsect as usize)?;
    if hit.hitsect < 0 || hit.hitsprite >= 0 || hit_sector.lotag > 2 || hit.hitwall < 0 {
        return None;
    }
    let wall = walls.get(hit.hitwall as usize)?;
    if wall.cstat & 16 != 0
        || script_tile_is_bigforce(wall.overpicnum)
        || (wall.nextsector >= 0
            && sectors
                .get(wall.nextsector as usize)
                .is_none_or(|sector| sector.lotag > 2))
        || (wall.nextsector < 0 && hit_sector.lotag > 2)
    {
        return None;
    }
    let radius_sq = i64::from(290_i32 * 290);
    let dx = i64::from(hit.hitx.wrapping_sub(p.posx));
    let dy = i64::from(hit.hity.wrapping_sub(p.posy));
    if dx * dx + dy * dy >= radius_sq {
        return None;
    }
    let mut sprite = lists.headspritesect[hit.hitsect as usize];
    let tripbomb_tile = script
        .and_then(|script| script.labels.get("TRIPBOMB"))
        .filter(|label| !label.is_ptr)
        .map_or(2566, |label| label.value as i16);
    while sprite >= 0 {
        let index = sprite as usize;
        sprite = lists.nextspritesect[index];
        let Some(candidate) = sprites.get(index) else {
            continue;
        };
        let sx = i64::from(candidate.x.wrapping_sub(hit.hitx));
        let sy = i64::from(candidate.y.wrapping_sub(hit.hity));
        if candidate.picnum == tripbomb_tile
            && candidate.z.abs_diff(hit.hitz) < (12 << 8) as u32
            && sx * sx + sy * sy < radius_sq
        {
            return None;
        }
    }
    Some(TripbombTarget {
        hitsect: hit.hitsect,
        hitwall: hit.hitwall,
        hitx: hit.hitx,
        hity: hit.hity,
        hitz: hit.hitz,
    })
}

fn script_tile_is_bigforce(tile: i16) -> bool {
    tile == 230
}

pub(crate) fn spawn_devastator_projectile(
    p: &Player,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_out: &mut Vec<i16>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
) -> i16 {
    let Some(body_i) = (p.sprite_i >= 0)
        .then_some(p.sprite_i as usize)
        .filter(|&i| i < sprites.len())
    else {
        return -1;
    };
    let source = sprites[body_i];
    let sect = source.sectnum;
    if sect < 0 || sect as usize >= sectors.len() {
        return -1;
    }
    let k = lists.insertsprite(sect, crate::stat::STAT_PROJECTILE);
    if k < 0 || k as usize >= sprites.len() {
        if k >= 0 {
            let _ = lists.deletesprite(k);
        }
        return -1;
    }
    let i = k as usize;
    // Chocolate `shoot(pi, RPG)`: acquire within the 48-unit aim cone, then
    // apply the Devastator-specific angle and vertical spread rolls.
    let target = aim(
        &source,
        AUTO_AIM_ANGLE,
        p.auto_aim == 2,
        lists,
        sprites,
        sectors,
        walls,
        actortype,
        Some(p),
    );
    let mut sa = i32::from(p.ang);
    let mut zvel = if target >= 0 {
        let target_u = target as usize;
        let target_sprite = &sprites[target_u];
        let target_height = tile_meta
            .get(target_sprite.picnum.max(0) as usize)
            .map_or(0, |meta| i32::from(meta.height));
        let target_dz = (i32::from(target_sprite.xrepeat) * target_height * 2) + (8 << 8);
        let target_dist = ldist(&source, target_sprite).max(1);
        if target_sprite.picnum != 1880 {
            sa = getangle(
                target_sprite.x.wrapping_sub(p.posx),
                target_sprite.y.wrapping_sub(p.posy),
            );
        }
        ((target_sprite.z.wrapping_sub(p.posz + p.pyoff + (4 << 8)) - target_dz) * 644)
            / target_dist
    } else {
        (100 - i32::from(p.horiz) - i32::from(p.horizoff)) * 81
    };
    sa += 16 - (krand_step(randomseed) & 31);
    zvel += 256 - (krand_step(randomseed) & 511);
    let muzzle_x = sincos_q14(sa + 860).0 / 448;
    let muzzle_y = sincos_q14(sa + 348).0 / 448;
    let base_x = p.posx + muzzle_x;
    let base_y = p.posy + muzzle_y;
    let base_z = p.posz + p.pyoff + (4 << 8);
    let side_x = sincos_q14(sa).0;
    let side_y = sincos_q14(sa + 1536).0;
    let (x, y) = if p.hbomb_hold_delay != 0 {
        (base_x - (side_x / 644), base_y - (side_y / 644))
    } else {
        (base_x + (side_x >> 8), base_y + (side_y >> 8))
    };
    let rpg_tile = script_tile_or(script, "RPG", RPG_PROJECTILE);
    let (base_extra, temps) = script
        .map(|image| crate::con::init_actor_temps_from_script(image, rpg_tile))
        .unwrap_or_default();
    // Chocolate EGS initializes `sprite[j].extra` from the projectile's
    // CON strength, then `shoot()` adds TRAND&7 and quarters the result for
    // Devastator shots. Keep the same order and preserve scripted strength.
    let extra = base_extra.wrapping_add((krand_step(randomseed) & 7) as i16) >> 2;
    sprites[i] = SpriteT {
        x,
        y,
        z: base_z - (1 << 8),
        ang: (sa & 2047) as i16,
        sectnum: sect,
        picnum: rpg_tile,
        shade: -96,
        xrepeat: 7,
        yrepeat: 7,
        xvel: 644,
        zvel: zvel as i16,
        owner: p.sprite_i,
        cstat: 128,
        clipdist: 4,
        yvel: target,
        extra,
        ..Default::default()
    };
    if picnum_out.len() <= i {
        picnum_out.resize(i + 1, 0);
    }
    picnum_out[i] = rpg_tile;
    if let Some(image) = script {
        sprites[i].hitag = crate::con::actor_header_hitag(image, rpg_tile) as i16;
    }
    if actor_temps.len() <= i {
        actor_temps.resize_with(i + 1, ActorConTemps::default);
    }
    actor_temps[i] = temps;
    k
}

pub fn process_weapons_shotgun(
    p: &mut Player,
    sync_bits: u32,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
    pending_sound_ids: &mut Vec<u16>,
    pending_switch_hits: &mut Vec<(i16, Option<i16>, Option<i16>)>,
) {
    tick_weapon_pos(p);

    if p.curr_weapon != SHOTGUN_WEAPON {
        return;
    }

    let fire = (sync_bits & (1 << 2)) != 0;
    if p.kickback_pic == 0 {
        if fire && p.last_weapon == -1 && p.weapon_pos == 0 {
            if p.ammo.get(SHOTGUN_WEAPON as usize).copied().unwrap_or(0) > 0 {
                // Chocolate `processweapon`: the start branch and the
                // `else if (*kb)` advance are exclusive within one tic.
                p.kickback_pic = 1;
            }
            return;
        } else {
            return;
        }
    }

    p.kickback_pic += 1;
    if p.kickback_pic == 4 {
        let body = p.sprite_i;
        if body >= 0 {
            let mut need_blood = true;
            for _ in 0..7 {
                if shoot_shotgun_player(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum_out,
                    actor_temps,
                    tile_meta,
                    script,
                    actortype,
                    randomseed,
                    p,
                    body,
                    SHOTGUN,
                    need_blood,
                    pending_sound_ids,
                    pending_switch_hits,
                ) {
                    need_blood = false;
                }
            }
        }
        if let Some(a) = p.ammo.get_mut(SHOTGUN_WEAPON as usize) {
            *a = a.saturating_sub(1);
        }
        if p.ammo[SHOTGUN_WEAPON as usize] == 0 {
            checkavailweapon(p);
        }
    }
    match p.kickback_pic {
        17 | 20 => {
            p.kickback_pic += 1;
        }
        24 => {
            // Chocolate: `spawn(pi,SHOTGUNSHELL); ang+=1024; ssp; ang+=1024; kb++`.
            let _ = spawn_player_shell(
                p,
                lists,
                sprites,
                sectors,
                walls,
                picnum_out,
                actor_temps,
                randomseed,
                SHOTGUNSHELL,
                6,
                20,
            );
            p.kickback_pic += 1;
        }
        31 => {
            p.kickback_pic = 0;
        }
        _ => {}
    }
}

/// Native pistol firing loop. The Atomic pistol uses the same player hitscan
/// path as the shotgun, but emits one SHOTSPARK1 and uses that actor's CON
/// strength for its 6–11 damage roll.
pub fn process_weapons_pistol(
    p: &mut Player,
    sync_bits: u32,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
    pending_sound_ids: &mut Vec<u16>,
    pending_switch_hits: &mut Vec<(i16, Option<i16>, Option<i16>)>,
) {
    const PISTOL: i16 = 1;
    tick_weapon_pos(p);
    if p.curr_weapon != PISTOL {
        return;
    }
    let fire = sync_bits & (1 << 2) != 0;
    if p.kickback_pic == 0 {
        if fire && p.last_weapon == -1 && p.weapon_pos == 0 && p.ammo[PISTOL as usize] > 0 {
            // Chocolate consumes the round when input starts the kickback,
            // then executes the frame-1 shot in the same processinput tic.
            p.kickback_pic = 1;
            p.ammo[PISTOL as usize] -= 1;
        } else {
            return;
        }
    }
    if p.kickback_pic == 1 {
        if p.sprite_i >= 0 {
            let _ = shoot_shotgun_player(
                lists,
                sprites,
                sectors,
                walls,
                picnum_out,
                actor_temps,
                tile_meta,
                script,
                actortype,
                randomseed,
                p,
                p.sprite_i,
                SHOTSPARK1,
                true,
                pending_sound_ids,
                pending_switch_hits,
            );
        }
        if let Some(sound_id) = script
            .and_then(|image| image.labels.get("PISTOL_FIRE"))
            .and_then(|label| u16::try_from(label.value).ok())
        {
            pending_sound_ids.push(sound_id);
        }
    } else if p.kickback_pic == 2 {
        let _ = spawn_player_shell(
            p,
            lists,
            sprites,
            sectors,
            walls,
            picnum_out,
            actor_temps,
            randomseed,
            SHELL,
            3,
            20,
        );
    }
    p.kickback_pic += 1;
    if p.kickback_pic >= 5 {
        if p.ammo[PISTOL as usize] == 0 || p.ammo[PISTOL as usize] % 12 != 0 {
            p.kickback_pic = 0;
            checkavailweapon(p);
        } else {
            let sound = match p.kickback_pic {
                5 => Some("EJECT_CLIP"),
                8 => Some("INSERT_CLIP"),
                _ => None,
            };
            if let Some(sound_id) = sound
                .and_then(|name| script.and_then(|image| image.labels.get(name)))
                .and_then(|label| u16::try_from(label.value).ok())
            {
                pending_sound_ids.push(sound_id);
            }
            if p.kickback_pic == 27 {
                p.kickback_pic = 0;
                checkavailweapon(p);
            }
        }
    }
}

/// Atomic chaingun's repeating hitscan cadence (Chocolate `processinput`).
pub fn process_weapons_chaingun(
    p: &mut Player,
    sync_bits: u32,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
    pending_sound_ids: &mut Vec<u16>,
    pending_switch_hits: &mut Vec<(i16, Option<i16>, Option<i16>)>,
) {
    const WEAPON: i16 = 3;
    tick_weapon_pos(p);
    if p.curr_weapon != WEAPON {
        return;
    }

    let fire = sync_bits & (1 << 2) != 0;
    if p.kickback_pic == 0 {
        if fire && p.last_weapon == -1 && p.weapon_pos == 0 && p.ammo[WEAPON as usize] > 0 {
            p.kickback_pic = 1;
        } else {
            return;
        }
    }

    p.kickback_pic += 1;
    if p.kickback_pic <= 12 {
        if p.kickback_pic % 3 == 0 {
            p.ammo[WEAPON as usize] = p.ammo[WEAPON as usize].saturating_sub(1);
            if p.sprite_i >= 0 {
                let _ = spawn_player_shell(
                    p,
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum_out,
                    actor_temps,
                    randomseed,
                    SHELL,
                    6,
                    52,
                );
            }
            if let Some(sound_id) = script
                .and_then(|image| image.labels.get("CHAINGUN_FIRE"))
                .and_then(|label| u16::try_from(label.value).ok())
            {
                pending_sound_ids.push(sound_id);
            }
            if p.sprite_i >= 0 {
                let _ = shoot_shotgun_player(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum_out,
                    actor_temps,
                    tile_meta,
                    script,
                    actortype,
                    randomseed,
                    p,
                    p.sprite_i,
                    CHAINGUN,
                    false,
                    pending_sound_ids,
                    pending_switch_hits,
                );
            }
            if p.ammo[WEAPON as usize] == 0 {
                checkavailweapon(p);
            }
        }
    } else {
        // Chocolate loops back to frame 1 while fire remains held; otherwise
        // it clears the kickback state. The next bullet fires after two tics.
        p.kickback_pic = if fire { 1 } else { 0 };
    }
}

/// Chocolate `spawn(player, SHOTGUNSHELL)` + immediate `ssp` (kickback_pic == 24).
pub(crate) fn spawn_player_shell(
    p: &Player,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    shell_tile: i16,
    shell_height: i32,
    shell_xvel: i16,
) -> i16 {
    use room_build_engine::geom::{SectorWalls, WallXY, clipmove_xy};
    use room_build_engine::helpers::sincos_q14;

    let body = p.sprite_i;
    if body < 0 || (body as usize) >= sprites.len() {
        return -1;
    }
    let bu = body as usize;
    let sect = sprites[bu].sectnum;
    // Chocolate spawn: `a = ang-(TRAND&63)+8`, `T1 = TRAND&1`, `zvel = -(TRAND&255)`.
    let a = i32::from(p.ang) - (krand_step(randomseed) & 63) + 8;
    let t1 = (krand_step(randomseed) & 1) as i32;
    let zvel = -(krand_step(randomseed) & 255) as i16;
    let z = (shell_height << 8) + p.pyoff + p.posz
        - ((i32::from(p.horizoff) + i32::from(p.horiz) - 100) << 4);
    // Chocolate: x += sintable[(a+512)&2047]>>7; y += sintable[a&2047]>>7
    let (sin_a, cos_a) = sincos_q14(a);
    let x = sprites[bu].x + (cos_a >> 7);
    let y = sprites[bu].y + (sin_a >> 7);
    let ang = ((a - 512) & 2047) as i16;

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
        picnum: shell_tile,
        shade: -8,
        xrepeat: 4,
        yrepeat: 4,
        xvel: shell_xvel,
        zvel,
        owner: body,
        ..Default::default()
    };
    if picnum_out.len() <= iu {
        picnum_out.resize(iu + 1, 0);
    }
    picnum_out[iu] = shell_tile;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    actor_temps[iu] = ActorConTemps::default();
    actor_temps[iu].g_t[0] = t1; // T1

    // ang += 1024; ssp; ang += 1024
    sprites[iu].ang = ((i32::from(sprites[iu].ang) + 1024) & 2047) as i16;
    {
        const TICSPERFRAME: i32 = 4;
        let xvel = i32::from(sprites[iu].xvel);
        let (sin, cos) = sincos_q14(i32::from(sprites[iu].ang));
        let step_x = (xvel * cos) >> 14;
        let step_y = (xvel * sin) >> 14;
        // Chocolate `ssp` on STAT_MISC: open add `(Δ*TIC)>>2` (not badguy `>>3`).
        sprites[iu].x += (step_x * TICSPERFRAME) >> 2;
        sprites[iu].y += (step_y * TICSPERFRAME) >> 2;
        sprites[iu].z += (i32::from(sprites[iu].zvel) * TICSPERFRAME) >> 2;
        let _ = (sectors, walls); // clip reserved if shells ever need CLIPMASK0 walls
    }
    sprites[iu].ang = ((i32::from(sprites[iu].ang) + 1024) & 2047) as i16;
    i
}

#[cfg(test)]
mod knee_weapon_tests {
    use super::process_weapons_knee;
    use crate::con::ActorConTemps;
    use crate::player::Player;
    use room_build_engine::types::{SectorT, SpriteT, WallT};

    #[test]
    fn knee_weapon_hits_on_tic_seven_and_repeats_at_fourteen() {
        let mut player = Player {
            curr_weapon: 0,
            last_weapon: -1,
            weapon_pos: 0,
            ..Player::default()
        };
        let mut sprites = Vec::<SpriteT>::new();
        let sectors = Vec::<SectorT>::new();
        let walls = Vec::<WallT>::new();
        let mut temps = Vec::<ActorConTemps>::new();
        let mut randomseed = 17;
        let input = 1 << 2;
        for tic in 1..=7 {
            let hit = process_weapons_knee(
                &mut player,
                input,
                &mut sprites,
                &sectors,
                &walls,
                &mut temps,
                &[],
                &[],
                None,
                &mut randomseed,
            );
            assert!(hit.is_none());
            assert_eq!(player.kickback_pic, tic);
        }
        for tic in 8..=14 {
            process_weapons_knee(
                &mut player,
                input,
                &mut sprites,
                &sectors,
                &walls,
                &mut temps,
                &[],
                &[],
                None,
                &mut randomseed,
            );
            if tic < 14 {
                assert_eq!(player.kickback_pic, tic);
            }
        }
        assert!((1..=4).contains(&player.kickback_pic));
    }
}

/// Chocolate `shoot(i, atwith)` for a non-player shooter (`p == -1`):
/// enemy hitscan (SHOTSPARK1 / SHOTGUN / CHAINGUN / GROWSPARK / KNEE),
/// projectiles (FIRELASER / SPIT / RPG / FREEZEBLAST / SHRINKER) and lobbed
/// MORTER / BOUNCEMINE. Returns `false` for tiles this path does not handle.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn shoot_enemy(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: &ScriptImage,
    players: &[Player],
    randomseed: &mut i32,
    i: i16,
    atwith: i16,
) -> bool {
    let iu = i as usize;
    if iu >= sprites.len() {
        return false;
    }
    let tile = |name: &str, fallback: i16| script_tile_or(Some(script), name, fallback);
    let morter = tile("MORTER", 1650);
    let bouncemine = tile("BOUNCEMINE", 940);
    let shrinker = tile("SHRINKER", 2556);
    let rpg = tile("RPG", RPG_PROJECTILE);
    let boss1 = tile("BOSS1", 2630);
    let boss2 = tile("BOSS2", 2710);
    let boss3 = tile("BOSS3", 2760);
    let hitscan_weapon =
        atwith == SHOTSPARK1 || atwith == SHOTGUN || atwith == CHAINGUN || atwith == GROWSPARK;
    let projectile = atwith == FIRELASER || atwith == SPIT;
    let rocket = atwith == rpg || atwith == FREEZEBLAST;
    if !(hitscan_weapon
        || projectile
        || rocket
        || atwith == KNEE
        || atwith == morter
        || atwith == bouncemine
        || atwith == shrinker)
    {
        return false;
    }
    let Some(player) = players.first() else {
        return false;
    };
    let actortype = script.actortype.as_slice();
    let s = sprites[iu];
    let sect = s.sectnum;
    let mut sa = s.ang;
    let mut sx = s.x;
    let mut sy = s.y;
    let height = tile_meta
        .get(s.picnum.max(0) as usize)
        .map_or(0, |m| i32::from(m.height));
    let mut sz = s.z - ((i32::from(s.yrepeat) * height) << 1) + (4 << 8);
    if s.picnum != ROTATEGUN {
        sz -= 7 << 8;
        if badguy_pic(s.picnum, actortype) && s.picnum != COMMANDER {
            let (sin, cos) = sincos_q14(i32::from(sa.wrapping_add(96)) & 2047);
            sx += (-sin) >> 7;
            sy += cos >> 7;
        }
    }
    let body = usize::try_from(player.sprite_i)
        .ok()
        .filter(|&b| b < sprites.len());
    let player_dist = body.map_or(1, |b| ldist(&sprites[b], &s).max(1));
    let sin_at = |a: i32| sincos_q14(a & 2047).0;
    let shade_lit = |sprites: &mut [SpriteT]| {
        if sprites[iu].extra >= 0 {
            sprites[iu].shade = -96;
        }
    };

    if hitscan_weapon || atwith == KNEE {
        let mut zvel;
        if atwith == KNEE {
            let target = body.map_or(s, |b| sprites[b]);
            let dist = find_distance_2d(target.x - sx, target.y - sy);
            zvel = ((target.z - sz) << 8) / (dist + 1);
            sa = getangle(target.x - sx, target.y - sy) as i16;
        } else {
            shade_lit(sprites);
            sz -= 4 << 8;
            zvel = ((player.posz - sz) << 8) / player_dist;
            zvel += 128 - (krand_step(randomseed) & 255);
            if s.picnum == boss1 {
                sa = (getangle(player.posx - sx, player.posy - sy) + 64
                    - (krand_step(randomseed) & 127)) as i16;
            } else {
                sa = sa.wrapping_add((32 - (krand_step(randomseed) & 63)) as i16);
            }
        }
        let sec: Vec<SectorWalls> = sectors.iter().map(SectorT::walls).collect();
        let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
        let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
        let old_cstat = sprites[iu].cstat;
        if atwith != KNEE {
            sprites[iu].cstat &= !257;
        }
        let spr_hits: Vec<SpriteHit> = sprites
            .iter()
            .map(|sp| {
                let meta = tile_meta
                    .get(sp.picnum.max(0) as usize)
                    .copied()
                    .unwrap_or_default();
                let mut z = sp.z;
                if meta.y_offset != 0 {
                    z -= i32::from(meta.y_offset) * i32::from(sp.yrepeat) * 4;
                }
                SpriteHit {
                    x: sp.x,
                    y: sp.y,
                    z,
                    sectnum: sp.sectnum,
                    clipdist: 0,
                    cstat: sp.cstat,
                    face_span: i32::from(meta.width) * i32::from(sp.xrepeat),
                    face_height: (i32::from(meta.height) * i32::from(sp.yrepeat)) << 2,
                }
            })
            .collect();
        let (sin, cos) = sincos_q14(i32::from(sa) & 2047);
        let hit = hitscan_full(
            sx,
            sy,
            sz,
            sect,
            cos,
            sin,
            zvel << 6,
            CLIPMASK1,
            &sec,
            &wxy,
            &spr_hits,
            Some(&sec_z),
        );
        sprites[iu].cstat = old_cstat;
        if hit.hitsect < 0 {
            return true;
        }
        if atwith == KNEE {
            if klabs(sx - hit.hitx) + klabs(sy - hit.hity) < 1024
                && (hit.hitwall >= 0 || hit.hitsprite >= 0)
            {
                let j = egs(
                    lists,
                    sprites,
                    picnum_out,
                    actor_temps,
                    Some(script),
                    hit.hitsect,
                    hit.hitx,
                    hit.hity,
                    hit.hitz,
                    KNEE,
                    -15,
                    0,
                    0,
                    sa,
                    32,
                    0,
                    i,
                    STAT_PROJECTILE,
                );
                if j >= 0 {
                    let ju = j as usize;
                    sprites[ju].extra = sprites[ju]
                        .extra
                        .wrapping_add((krand_step(randomseed) & 7) as i16);
                    if hit.hitsprite >= 0 {
                        checkhitsprite_shotgun(
                            lists,
                            sprites,
                            sectors,
                            walls,
                            players,
                            picnum_out,
                            actor_temps,
                            tile_meta,
                            Some(script),
                            actortype,
                            randomseed,
                            hit.hitsprite,
                            j,
                            false,
                        );
                    }
                }
            }
            return true;
        }
        if atwith == GROWSPARK {
            let j = egs(
                lists,
                sprites,
                picnum_out,
                actor_temps,
                Some(script),
                sect,
                hit.hitx,
                hit.hity,
                hit.hitz,
                GROWSPARK,
                -16,
                28,
                28,
                sa,
                0,
                0,
                i,
                crate::stat::STAT_ACTOR,
            );
            if j >= 0 {
                let ju = j as usize;
                sprites[ju].pal = 2;
                sprites[ju].cstat |= 130;
                sprites[ju].xrepeat = 1;
                sprites[ju].yrepeat = 1;
                if hit.hitsprite >= 0 {
                    checkhitsprite_shotgun(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        players,
                        picnum_out,
                        actor_temps,
                        tile_meta,
                        Some(script),
                        actortype,
                        randomseed,
                        hit.hitsprite,
                        j,
                        false,
                    );
                }
            }
            return true;
        }
        if (krand_step(randomseed) & 15) == 0
            && sectors
                .get(hit.hitsect as usize)
                .is_some_and(|sc| sc.lotag == 2)
        {
            tracers(
                lists,
                sprites,
                picnum_out,
                actor_temps,
                Some(script),
                sectors,
                walls,
                randomseed,
                hit.hitx,
                hit.hity,
                hit.hitz,
                sx,
                sy,
                sz,
                8,
                i,
            );
        }
        let k = egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            Some(script),
            hit.hitsect,
            hit.hitx,
            hit.hity,
            hit.hitz,
            SHOTSPARK1,
            -15,
            24,
            24,
            sa,
            0,
            0,
            i,
            STAT_PROJECTILE,
        );
        if k < 0 {
            return true;
        }
        let ku = k as usize;
        // `sprite[k].extra = *actorscrptr[atwith]`.
        sprites[ku].extra = init_actor_temps_from_script(script, atwith).0;
        set_eduke_spark_hit_data(sprites, actor_temps, Some(script), ku, &hit, atwith);
        if hit.hitsprite >= 0 {
            checkhitsprite_shotgun(
                lists,
                sprites,
                sectors,
                walls,
                players,
                picnum_out,
                actor_temps,
                tile_meta,
                Some(script),
                actortype,
                randomseed,
                hit.hitsprite,
                k,
                false,
            );
            let hs = hit.hitsprite as usize;
            if hs < sprites.len() && sprites[hs].picnum == APLAYER {
                sprites[ku].xrepeat = 0;
                sprites[ku].yrepeat = 0;
                if actor_temps.len() > hs {
                    actor_temps[hs].pending_actor_sounds.push(2); // PISTOL_BODYHIT
                }
            } else {
                spawn_smallsmoke(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum_out,
                    actor_temps,
                    Some(script),
                    randomseed,
                    k,
                );
            }
        } else if hit.hitwall >= 0 {
            let source = sprites[ku];
            let _ = crate::weapons_move::apply_checkhitwall(
                lists,
                sprites,
                sectors,
                walls,
                picnum_out,
                actor_temps,
                randomseed,
                source,
                k,
                SHOTSPARK1,
                hit.hitwall as usize,
                hit.hitx,
                hit.hity,
                hit.hitz,
                hit.hitsect,
                Some(script),
            );
        }
        let _ = krand_step(randomseed) & 255; // PISTOL_RICOCHET roll
        return true;
    }

    if projectile {
        shade_lit(sprites);
        let vel = if atwith == SPIT {
            292
        } else {
            sz -= 4 << 7;
            840
        };
        sa = sa.wrapping_add((16 - (krand_step(randomseed) & 31)) as i16);
        let zvel = ((player.oposz - sz + (3 << 8)) * vel) / player_dist;
        let (sizx, sizy) = (18_u8, 18_u8);
        if atwith == SPIT {
            sz -= 10 << 8;
        }
        let j = egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            Some(script),
            sect,
            sx,
            sy,
            sz,
            atwith,
            -127,
            sizx,
            sizy,
            sa,
            vel as i16,
            zvel.clamp(-32768, 32767) as i16,
            i,
            STAT_PROJECTILE,
        );
        if j >= 0 {
            let ju = j as usize;
            sprites[ju].extra = sprites[ju]
                .extra
                .wrapping_add((krand_step(randomseed) & 7) as i16);
            sprites[ju].cstat = 128;
            sprites[ju].clipdist = 4;
            // Chocolate re-rolls sa/zvel for the (unused) next shot.
            let _ = krand_step(randomseed);
            let _ = krand_step(randomseed);
        }
        return true;
    }

    if rocket {
        if atwith == FREEZEBLAST {
            sz += 3 << 8;
        }
        shade_lit(sprites);
        let mut vel = 644;
        sa = getangle(player.oposx - sx, player.oposy - sy) as i16;
        if s.picnum == boss3 {
            sz -= 32 << 8;
        } else if s.picnum == boss2 {
            vel += 128;
            sz += 24 << 8;
        }
        let zvel = ((player.oposz - sz) * vel) / player_dist;
        if badguy_pic(s.picnum, actortype) && (s.hitag & 2048) != 0 {
            sa = s
                .ang
                .wrapping_add(((krand_step(randomseed) & 31) - 16) as i16);
        }
        let x = sx + sincos_q14(348 + i32::from(sa)).1 / 448;
        let y = sy + sin_at(i32::from(sa) + 348) / 448;
        let j = egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            Some(script),
            sect,
            x,
            y,
            sz - (1 << 8),
            atwith,
            0,
            14,
            14,
            sa,
            vel as i16,
            zvel.clamp(-32768, 32767) as i16,
            i,
            STAT_PROJECTILE,
        );
        if j < 0 {
            return true;
        }
        let ju = j as usize;
        sprites[ju].extra = sprites[ju]
            .extra
            .wrapping_add((krand_step(randomseed) & 7) as i16);
        if atwith != FREEZEBLAST {
            sprites[ju].yvel = -1;
        } else {
            sprites[ju].yvel = 3;
            sprites[ju].xrepeat >>= 1;
            sprites[ju].yrepeat >>= 1;
            sprites[ju].zvel -= 2 << 4;
        }
        let a = i32::from(sa);
        if s.picnum == boss3 {
            if krand_step(randomseed) & 1 != 0 {
                sprites[ju].x -= sin_at(a) >> 6;
                sprites[ju].y -= sin_at(a + 1024 + 512) >> 6;
                sprites[ju].ang -= 8;
            } else {
                sprites[ju].x += sin_at(a) >> 6;
                sprites[ju].y += sin_at(a + 1024 + 512) >> 6;
                sprites[ju].ang += 4;
            }
            sprites[ju].xrepeat = 42;
            sprites[ju].yrepeat = 42;
        } else if s.picnum == boss2 {
            sprites[ju].x -= sin_at(a) / 56;
            sprites[ju].y -= sin_at(a + 1024 + 512) / 56;
            sprites[ju].ang = sprites[ju]
                .ang
                .wrapping_sub((8 + (krand_step(randomseed) & 255) - 128) as i16);
            sprites[ju].xrepeat = 24;
            sprites[ju].yrepeat = 24;
        } else if atwith != FREEZEBLAST {
            sprites[ju].xrepeat = 30;
            sprites[ju].yrepeat = 30;
            sprites[ju].extra >>= 2;
        }
        sprites[ju].cstat = 128;
        sprites[ju].clipdist = if atwith == rpg { 4 } else { 40 };
        return true;
    }

    if atwith == morter || atwith == bouncemine {
        shade_lit(sprites);
        let x = player_dist;
        let mut zvel = -x >> 1;
        if zvel < -4096 {
            zvel = -2048;
        }
        let vel = x >> 4;
        let a = i32::from(sa);
        let _ = egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            Some(script),
            sect,
            sx + (sin_at(512 + a + 512) >> 8),
            sy + (sin_at(a + 512) >> 8),
            sz + (6 << 8),
            atwith,
            -64,
            32,
            32,
            sa,
            vel.clamp(-32768, 32767) as i16,
            zvel as i16,
            i,
            crate::stat::STAT_ACTOR,
        );
        return true;
    }

    // SHRINKER (NEWBEAST).
    shade_lit(sprites);
    let zvel = if lists.sprite_stat.get(iu).copied() != Some(STAT_EFFECTOR) {
        ((player.oposz - sz) * 512) / player_dist
    } else {
        0
    };
    let a = i32::from(sa);
    let j = egs(
        lists,
        sprites,
        picnum_out,
        actor_temps,
        Some(script),
        sect,
        sx + (sin_at(512 + a + 512) >> 12),
        sy + (sin_at(a + 512) >> 12),
        sz + (2 << 8),
        SHRINKSPARK,
        -16,
        28,
        28,
        sa,
        768,
        zvel.clamp(-32768, 32767) as i16,
        i,
        STAT_PROJECTILE,
    );
    if j >= 0 {
        sprites[j as usize].cstat = 128;
        sprites[j as usize].clipdist = 32;
    }
    true
}

thread_local! {
    static HIT_DEBUG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static HIT_LOG: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Enable/disable the player hitscan debug log (`duke_hitdebug`).
pub fn set_hit_debug(on: bool) {
    HIT_DEBUG.with(|d| d.set(on));
}

pub(crate) fn hit_debug_enabled() -> bool {
    HIT_DEBUG.with(std::cell::Cell::get)
}

pub(crate) fn push_hit_log(line: String) {
    HIT_LOG.with(|l| {
        let mut l = l.borrow_mut();
        if l.len() < 256 {
            l.push(line);
        }
    });
}

/// Drain player hitscan debug lines recorded since the last call.
#[must_use]
pub fn take_hit_log() -> Vec<String> {
    HIT_LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}
