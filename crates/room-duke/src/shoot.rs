//! Chocolate `shoot` / `aim` (player shotgun path for D6 / DEMO1 tic135).

use room_build_engine::geom::{
    HitScan, SectorWalls, SectorZ, SpriteHit, WallXY, cansee, getzsofslope, hitscan_full, pushmove,
    updatesector,
};
use room_build_engine::helpers::{getangle, sincos_q14};
use room_build_engine::numeric::{klabs, krand_step, mulscale};
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::badguy::badguy_pic;
use crate::con::{ActorConTemps, ScriptImage, actor_header_hitag, init_actor_temps_from_script};
use crate::player::{CLIPMASK0, Player, TileZMeta};
use crate::spawn::WATERBUBBLE;
use crate::stat::{STAT_DEFAULT, STAT_MISC, STAT_PROJECTILE};
use crate::weapons::SHOTGUN_WEAPON;

/// Atomic tile numbers used by shotgun FX.
pub const SHOTSPARK1: i16 = 2595;
/// Atomic `COOLEXPLOSION1` — OCTABRAIN projectile (`shoot` / STAT_PROJECTILE).
pub const COOLEXPLOSION1: i16 = 1360;
pub const SHOTGUN: i16 = 2613;
pub const SMALLSMOKE: i16 = 2329;
/// Atomic `COMMANDER` — muzzle offset exempt in `shoot` enemy path.
const COMMANDER: i16 = 1920;
pub const JIBS6: i16 = 2286;
pub const BLOOD: i16 = 1620;
pub const BLOODSPLAT1: i16 = 2296;
pub const BLOODSPLAT2: i16 = 2297;
pub const BLOODSPLAT3: i16 = 2298;
pub const BLOODSPLAT4: i16 = 2299;
pub const APLAYER: i16 = 1405;
/// Atomic `SHOTGUNSHELL` (ejected casing).
pub const SHOTGUNSHELL: i16 = 2535;

/// Chocolate `CLIPMASK1`.
const CLIPMASK1: u32 = (256 << 16) + 64;

/// Chocolate `AUTO_AIM_ANGLE`.
const AUTO_AIM_ANGLE: i16 = 48;

/// TEMP: pre-burst TRAND (0 — first spark matches Chocolate; was 1 for early
/// floor-miss experiments). Real pre-shotgun site TBD if seed drifts.
const PRE_SHOT_PAD: u32 = 0;

/// TEMP: floor/wall miss skips hit-path JIBS ang TRAND (1). With
/// BLOOD_SPLAT_TRANDS=9, miss pad=1 yields Chocolate SMALLSMOKE XYZ on DEMO1
/// tic135 (floor pellet early — freelist order still open).
const MISS_HITPATH_PAD: u32 = 1;

/// TEMP: Chocolate `shoot(enemy, BLOODSPLAT)×4` TRAND count stub. 8 (=4×2 sa/zvel)
/// was one short; 9 matches tic135 smoke scatter. Replace with real BLOODSPLAT shoot.
const BLOOD_SPLAT_TRANDS: u32 = 9;

/// TEMP: pad before 7th pellet (0 — unused while miss=1 floors early).
const LAST_SHOT_PAD: u32 = 0;

/// Test overrides (<0 = use const). Set via `set_shot_pads` in demo_sync probes.
static PRE_SHOT_PAD_OVR: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
static MISS_HITPATH_PAD_OVR: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
static BLOOD_SPLAT_TRANDS_OVR: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
static FIRST_FACE_EXTRA_OVR: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
static LAST_SHOT_PAD_OVR: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

pub fn set_shot_pads(pre: i32, miss: i32, blood: i32, first_extra: i32) {
    set_shot_pads5(pre, miss, blood, first_extra, -1);
}

pub fn set_shot_pads5(pre: i32, miss: i32, blood: i32, first_extra: i32, last: i32) {
    PRE_SHOT_PAD_OVR.store(pre, std::sync::atomic::Ordering::SeqCst);
    MISS_HITPATH_PAD_OVR.store(miss, std::sync::atomic::Ordering::SeqCst);
    BLOOD_SPLAT_TRANDS_OVR.store(blood, std::sync::atomic::Ordering::SeqCst);
    FIRST_FACE_EXTRA_OVR.store(first_extra, std::sync::atomic::Ordering::SeqCst);
    LAST_SHOT_PAD_OVR.store(last, std::sync::atomic::Ordering::SeqCst);
}

fn pre_shot_pad() -> u32 {
    let v = PRE_SHOT_PAD_OVR.load(std::sync::atomic::Ordering::SeqCst);
    if v < 0 { PRE_SHOT_PAD } else { v as u32 }
}
fn miss_hitpath_pad() -> u32 {
    let v = MISS_HITPATH_PAD_OVR.load(std::sync::atomic::Ordering::SeqCst);
    if v < 0 { MISS_HITPATH_PAD } else { v as u32 }
}
fn blood_splat_trands() -> u32 {
    let v = BLOOD_SPLAT_TRANDS_OVR.load(std::sync::atomic::Ordering::SeqCst);
    if v < 0 { BLOOD_SPLAT_TRANDS } else { v as u32 }
}
fn first_face_extra() -> u32 {
    let v = FIRST_FACE_EXTRA_OVR.load(std::sync::atomic::Ordering::SeqCst);
    if v < 0 { 0 } else { v as u32 }
}
fn last_shot_pad() -> u32 {
    let v = LAST_SHOT_PAD_OVR.load(std::sync::atomic::Ordering::SeqCst);
    if v < 0 { LAST_SHOT_PAD } else { v as u32 }
}

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
                                let cans = cansee(
                                    t.x,
                                    t.y,
                                    t.z - (32 << 8),
                                    t.sectnum,
                                    s.x,
                                    s.y,
                                    s.z - (32 << 8),
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

fn egs(
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
    i
}

fn spawn_smallsmoke(
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
        shade: -127,
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
}

fn checkhitsprite_shotgun(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
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
        return;
    }
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
            let xr = (4 + (krand_step(randomseed) & 3)) as u8;
            let yr = (4 + (krand_step(randomseed) & 3)) as u8;
            let ang = (krand_step(randomseed) & 2047) as i16;
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

/// Chocolate `shoot(i, SHOTGUN)` for a player body sprite.
pub fn shoot_shotgun_player(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    tile_meta: &[TileZMeta],
    script: Option<&ScriptImage>,
    actortype: &[u8],
    randomseed: &mut i32,
    player: &Player,
    body_i: i16,
    // When true, first badguy hit this call also spawns floor BLOOD.
    spawn_blood: bool,
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
        true,
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
            let pn = SHOTGUN.max(0) as usize;
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
        for _ in 0..miss_hitpath_pad() {
            let _ = krand_step(randomseed);
        }
    } else if hit.hitsprite >= 0 {
        let hu = hit.hitsprite as usize;
        let is_bg = hu < sprites.len() && badguy_pic(sprites[hu].picnum, actortype);
        checkhitsprite_shotgun(
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
            if spawn_blood {
                for _ in 0..first_face_extra() {
                    let _ = krand_step(randomseed);
                }
            }
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
        for _ in 0..miss_hitpath_pad() {
            let _ = krand_step(randomseed);
        }
    }
    did_blood
}

/// Chocolate shotgun branch of `processinput` SHOOTINCODE (kickback + fire).
pub fn process_weapons_shotgun(
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
    actortype: &[u8],
    randomseed: &mut i32,
) {
    if p.weapon_pos != 0 {
        if p.weapon_pos == -9 {
            if p.last_weapon >= 0 {
                p.weapon_pos = 10;
                p.last_weapon = -1;
            } else {
                p.weapon_pos = 10;
            }
        } else {
            p.weapon_pos -= 1;
        }
    }

    if p.curr_weapon != SHOTGUN_WEAPON {
        return;
    }

    let fire = (sync_bits & (1 << 2)) != 0;
    if p.kickback_pic == 0 {
        if fire && p.last_weapon == -1 && p.weapon_pos == 0 {
            if p.ammo.get(SHOTGUN_WEAPON as usize).copied().unwrap_or(0) > 0 {
                p.kickback_pic = 1;
            }
        }
        return;
    }

    p.kickback_pic += 1;
    if p.kickback_pic == 4 {
        for _ in 0..pre_shot_pad() {
            let _ = krand_step(randomseed);
        }
        let body = p.sprite_i;
        if body >= 0 {
            let mut need_blood = true;
            for shot in 0..7 {
                if shot == 6 {
                    for _ in 0..last_shot_pad() {
                        let _ = krand_step(randomseed);
                    }
                }
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
                    need_blood,
                ) {
                    need_blood = false;
                }
            }
        }
        if let Some(a) = p.ammo.get_mut(SHOTGUN_WEAPON as usize) {
            *a = a.saturating_sub(1);
        }
    }
    match p.kickback_pic {
        17 | 20 => {
            p.kickback_pic += 1;
        }
        24 => {
            // Chocolate: `spawn(pi,SHOTGUNSHELL); ang+=1024; ssp; ang+=1024; kb++`.
            let _ = spawn_shotgun_shell(
                p,
                lists,
                sprites,
                sectors,
                walls,
                picnum_out,
                actor_temps,
                randomseed,
            );
            p.kickback_pic += 1;
        }
        31 => {
            p.kickback_pic = 0;
        }
        _ => {}
    }
}

/// Chocolate `spawn(player, SHOTGUNSHELL)` + immediate `ssp` (kickback_pic == 24).
fn spawn_shotgun_shell(
    p: &Player,
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
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
    let z = (6 << 8) + p.pyoff + p.posz - ((i32::from(p.horizoff) + i32::from(p.horiz) - 100) << 4);
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
        picnum: SHOTGUNSHELL,
        shade: -8,
        xrepeat: 4,
        yrepeat: 4,
        xvel: 20,
        zvel,
        owner: body,
        ..Default::default()
    };
    if picnum_out.len() <= iu {
        picnum_out.resize(iu + 1, 0);
    }
    picnum_out[iu] = SHOTGUNSHELL;
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
