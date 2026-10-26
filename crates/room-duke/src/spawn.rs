//! Post-load sprite classification — partial Chocolate `spawn` (D5).
//!
//! [`BuildWorld::load_map_bytes`] already places sprites on lists using the
//! map's `statnum`. This pass reassigns statuses for markers / effectors /
//! CON actors and soft-culls SP-only / over-skill pickups onto [`STAT_MISC`]
//! with `xrepeat == 0` (later removed by [`crate::explosions::moveexplosions`]).

use room_build_engine::geom::{
    SectorWalls, SectorZ, SpriteZClip, WallXY, getzrange_with_sprites, getzsofslope,
};
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::player::{sprite_z_clip, TileZMeta, CLIPMASK0};

use crate::stat::{
    STAT_ACTIVATOR, STAT_ACTOR, STAT_DEFAULT, STAT_DUMMY, STAT_EFFECTOR, STAT_FX, STAT_LOCATOR,
    STAT_MISC, STAT_PLAYER, STAT_STANDABLE, STAT_TRANSPORT,
};

/// Chocolate `FOURSLEIGHT` — sprite rests this many Build Z units above the floor.
pub const FOURSLEIGHT: i32 = 1 << 8;

/// Picnum for the player body in Atomic Duke.
pub const APLAYER: i16 = 1405;

/// Sector effector sprite picnum.
pub const SECTOREFFECTOR: i16 = 1;

/// `names.h` markers / standables used in status classify.
pub const ACTIVATOR: i16 = 2;
pub const TOUCHPLATE: i16 = 3;
pub const ACTIVATORLOCKED: i16 = 4;
pub const MUSICANDSFX: i16 = 5;
pub const LOCATORS: i16 = 6;
pub const MASTERSWITCH: i16 = 8;
pub const RESPAWN: i16 = 9;
pub const BOLT1: i16 = 634;
pub const WATERBUBBLE: i16 = 661;
pub const WATERBUBBLEMAKER: i16 = 662;
pub const FIREEXT: i16 = 916;
pub const SEENINE: i16 = 1247;
pub const OOZFILTER: i16 = 1079;
pub const CRACK1: i16 = 546;
pub const VIEWSCREEN: i16 = 502;
pub const CONE: i16 = 978;
pub const CAMERA1: i16 = 621;
pub const NUKEBUTTON: i16 = 142;
pub const GREENSLIME: i16 = 2370;
pub const RECON: i16 = 1960;
pub const CANWITHSOMETHING: i16 = 1232;
pub const TREE1: i16 = 908;
pub const TREE2: i16 = 910;
pub const TIRE: i16 = 990;
pub const BOX: i16 = 951;

/// Inventory / ammo pics that share Chocolate's SP `pal != 0` / skill soft-cull
/// (`game.c` spawn cases through `SIXPAK`, before the `ACCESSCARD` fall-through).
const SOFT_CULL_PICKUPS: &[i16] = &[
    21, 22, 23, 24, 25, 27, 28, 29, // weapon sprites + TRIPBOMBSPRITE
    37, 40, 41, 42, 44, 45, 46, 47, 48, 49, // ammo
    51, 52, 53, 54, 55, 56, 57, 59, 61, // COLA…JETPACK, HEATSENSOR, BOOTS
    100, 1348, // ATOMICHEALTH, HOLODUKE
];

/// Hardcoded badguys / barrels that land on STAT_DUMMY (Chocolate explicit
/// spawn cases — not the `actorscrptr` default, which would wake them).
const DUMMY_ENEMIES: &[i16] = &[
    GREENSLIME,
    RECON,
    CANWITHSOMETHING,
    1233,
    1234,
    1235, // cans
    1221,
    1222,
    1225,
    1226,
    1227,
    1228, // barrels / horse
    1238, // EXPLODINGBARREL
    675,  // EGG
    1550, // SHARK
    1680,
    1681,
    1682,
    1715,
    1725,
    1741,
    1742,
    1744, // LIZTROOP*
    1820,
    1821, // OCTABRAIN*
    1880, // DRONE
    1920,
    1921, // COMMANDER*
    2000,
    2001,
    2045, // PIGCOP*
    2120,
    2121,
    2150,
    2165, // LIZMAN*
    2420, // ORGANTIC
    2631, // BOSS1STAYPUT
];

/// Face flames: Chocolate default `actor` → STAT_ACTOR; CON AI is hardcoded
/// in `moveactors`, so Room skips script `killit` until that lands.
pub const FIRE: i16 = 2271;
pub const FIRE2: i16 = 2311;

/// Reclassify sprite statuses after map load.
pub fn classify_map_sprites(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    actorscrptr: &[i32],
    actortype: &[u8],
    picnum_out: &mut Vec<i16>,
    skill: u8,
) {
    picnum_out.clear();
    picnum_out.resize(sprites.len().max(lists.sprite_stat.len()), 0);
    for i in 0..sprites.len() {
        if sprites[i].sectnum < 0 {
            continue;
        }
        if i >= lists.sprite_stat.len() {
            break;
        }
        if lists.sprite_stat[i] == room_build_engine::world::MAXSTATUS as i16 {
            continue;
        }

        // SP: all map APLAYERs → zero-size STAT_MISC (resetpsprite EGS adds the live body).
        if sprites[i].picnum == APLAYER {
            soft_cull_to_misc(lists, sprites, picnum_out, i);
            continue;
        }

        // BOLT1…BOLT1+3 → standable (Chocolate: T1/T2 = repeats, no pic++).
        if (BOLT1..=BOLT1 + 3).contains(&sprites[i].picnum) {
            picnum_out[i] = sprites[i].picnum;
            let _ = lists.changespritestat(i as i16, STAT_STANDABLE);
            continue;
        }

        // MUSICANDSFX / RESPAWN: SP pal==1 soft-cull, else STAT_FX.
        if sprites[i].picnum == MUSICANDSFX || sprites[i].picnum == RESPAWN {
            if sprites[i].pal == 1 {
                soft_cull_to_misc(lists, sprites, picnum_out, i);
                continue;
            }
            picnum_out[i] = sprites[i].picnum;
            let _ = lists.changespritestat(i as i16, STAT_FX);
            continue;
        }

        // CRACK / FIREEXT: SP pal soft-cull, else standable.
        // Chocolate: `cstat |= 17`, `xvel = 8`, one `ssp` (movesprite).
        if (CRACK1..=CRACK1 + 3).contains(&sprites[i].picnum) || sprites[i].picnum == FIREEXT {
            if sprites[i].pal != 0 {
                soft_cull_to_misc(lists, sprites, picnum_out, i);
                continue;
            }
            picnum_out[i] = sprites[i].picnum;
            if sprites[i].picnum == FIREEXT {
                sprites[i].cstat = 257;
            } else {
                sprites[i].cstat |= 17;
                sprites[i].xvel = 8;
                // `ssp` needs walls — deferred to [`ssp_cracks_after_classify`].
            }
            let _ = lists.changespritestat(i as i16, STAT_STANDABLE);
            continue;
        }

        // BLOODSPLAT1…4 (2296…2299) → STAT_MISC; Chocolate `z -= (16<<8)`.
        if (2296..=2299).contains(&sprites[i].picnum) {
            picnum_out[i] = sprites[i].picnum;
            sprites[i].cstat |= 16;
            sprites[i].z -= 16 << 8;
            let _ = lists.changespritestat(i as i16, STAT_MISC);
            continue;
        }

        // CAMERA1 → STAT_ACTOR (awake cameras).
        if (CAMERA1..=CAMERA1 + 4).contains(&sprites[i].picnum) {
            if sprites[i].pal != 0 {
                soft_cull_to_misc(lists, sprites, picnum_out, i);
                continue;
            }
            sprites[i].picnum = CAMERA1;
            picnum_out[i] = CAMERA1;
            let _ = lists.changespritestat(i as i16, STAT_ACTOR);
            continue;
        }

        // Inventory SP pal / over-skill → STAT_MISC zero size.
        if soft_cull_pickup(&sprites[i], skill) {
            soft_cull_to_misc(lists, sprites, picnum_out, i);
            continue;
        }

        // RECON over-skill soft-cull.
        if sprites[i].picnum == RECON && sprites[i].lotag as u8 > skill {
            soft_cull_to_misc(lists, sprites, picnum_out, i);
            continue;
        }

        picnum_out[i] = sprites[i].picnum;
        let want = classify_stat(&sprites[i], actorscrptr, actortype);
        if lists.sprite_stat[i] != want {
            let _ = lists.changespritestat(i as i16, want);
        }
    }
}

/// Chocolate `resetpspritevars` EGS: insert one `APLAYER` at the map start pose.
pub fn spawn_player_body(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum_out: &mut Vec<i16>,
    start: [i32; 3],
    start_ang: i16,
    start_sector: i16,
) -> i16 {
    let i = lists.insertsprite(start_sector, STAT_PLAYER);
    if i < 0 {
        return -1;
    }
    let iu = i as usize;
    if iu >= sprites.len() {
        return -1;
    }
    sprites[iu] = SpriteT {
        x: start[0],
        y: start[1],
        z: start[2],
        ang: start_ang,
        sectnum: start_sector,
        picnum: APLAYER,
        xrepeat: 42,
        yrepeat: 36,
        cstat: 1 + 256,
        clipdist: 64,
        extra: 100,
        // Chocolate EGS: `owner = i`, `yvel = player index` (SP → 0).
        owner: i,
        yvel: 0,
        ..Default::default()
    };
    if picnum_out.len() <= iu {
        picnum_out.resize(iu + 1, 0);
    }
    picnum_out[iu] = APLAYER;
    i
}

/// Partial Chocolate `makeitfall` after map spawn: snap resting sprites to
/// `floorz - FOURSLEIGHT` (one-shot; ignores gravity / `getzrange` for now).
pub fn spawn_floor_snap(
    sprites: &mut [SpriteT],
    lists: &SpriteLists,
    sectors: &[SectorT],
    walls: &[WallT],
) {
    let sec_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sec_z: Vec<_> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();

    for i in 0..sprites.len() {
        let st = lists.sprite_stat.get(i).copied().unwrap_or(1024);
        // Map spawn `makeitfall` for sleeping/active actors — not player (EGS
        // keeps loadboard posz; first-tic gravity settles later) or standables.
        if st != STAT_ACTOR && st != STAT_DUMMY {
            continue;
        }
        let s = &mut sprites[i];
        let sect = s.sectnum;
        if sect < 0 || sect as usize >= sec_z.len() {
            continue;
        }
        let (_ceil, flor) = getzsofslope(sect, s.x, s.y, &sec_z, &wxy);
        let _ = &sec_walls;
        // Settled makeitfall: snap to floor rest (Build Z grows downward).
        s.z = flor - FOURSLEIGHT;
    }
}

/// Chocolate `spawn` SECTORFFECTOR init (subset): lotag 0 snaps to lotag-1
/// pivot XY when `ang == 512`; lotag 13/18/25/35 rewrite sector Z before
/// pickup `makeitfall`.
pub fn spawn_sector_effectors(sprites: &mut [SpriteT], sectors: &mut [SectorT]) {
    // Collect lotag-1 pivots by hitag (first match wins, sprite index order).
    let mut pivots: Vec<(i16, i32, i32)> = Vec::new();
    for s in sprites.iter() {
        if s.picnum == SECTOREFFECTOR && s.lotag == 1 {
            pivots.push((s.hitag, s.x, s.y));
        }
    }

    for i in 0..sprites.len() {
        if sprites[i].picnum != SECTOREFFECTOR {
            continue;
        }
        let lotag = sprites[i].lotag;
        let ang = sprites[i].ang;
        let z = sprites[i].z;
        let pal = sprites[i].pal;
        let hitag = sprites[i].hitag;
        let sect = sprites[i].sectnum;
        if sect < 0 || sect as usize >= sectors.len() {
            continue;
        }

        match lotag {
            // SE7/23 transporters: pair by hitag, T5 = on-floor, STAT_TRANSPORT.
            7 | 23 => {
                sprites[i].cstat = 0;
                sprites[i].xrepeat = 0;
                sprites[i].yrepeat = 0;
                if lotag != 23 {
                    let mut ow = i as i16;
                    for (j, other) in sprites.iter().enumerate() {
                        if j == i {
                            continue;
                        }
                        if other.picnum == SECTOREFFECTOR
                            && (other.lotag == 7 || other.lotag == 23)
                            && other.hitag == hitag
                        {
                            ow = j as i16;
                            break;
                        }
                    }
                    sprites[i].owner = ow;
                } else {
                    sprites[i].owner = i as i16;
                }
                // Stash onfloorz in actor temp later; mark via owner high unused —
                // `g_t[5]` filled in `init_transport_temps`.
            }
            _ => {
                // Invisible zero-size marker (Chocolate `cstat |= 32768`).
                sprites[i].cstat |= 32768;
                sprites[i].xrepeat = 0;
                sprites[i].yrepeat = 0;
            }
        }

        match lotag {
            0 => {
                if let Some((_, px, py)) = pivots.iter().find(|(h, _, _)| *h == hitag) {
                    if ang == 512 {
                        sprites[i].x = *px;
                        sprites[i].y = *py;
                    }
                }
            }
            13 => {
                let sec = &mut sectors[sect as usize];
                let d_ceil = (sec.ceilingz - z).abs();
                let d_flor = (sec.floorz - z).abs();
                let owner_ceil = d_ceil < d_flor;
                sprites[i].owner = if owner_ceil { 1 } else { 0 };
                if ang == 512 {
                    if owner_ceil {
                        sec.ceilingz = z;
                    } else {
                        sec.floorz = z;
                    }
                } else {
                    sec.ceilingz = z;
                    sec.floorz = z;
                }
                if sec.ceilingstat & 1 != 0 {
                    sec.ceilingstat ^= 1;
                    if !owner_ceil && ang == 512 {
                        sec.ceilingstat ^= 1;
                    }
                    sec.ceilingshade = sec.floorshade;
                }
            }
            18 => {
                let sec = &mut sectors[sect as usize];
                if ang == 512 {
                    if pal != 0 {
                        sec.ceilingz = z;
                    }
                } else if pal != 0 {
                    sec.floorz = z;
                }
                // Chocolate `hitag <<= 2` (runtime only; keep map hitag for now).
            }
            25 | 35 => {
                sectors[sect as usize].ceilingz = z;
            }
            _ => {}
        }
    }
}

/// Fill `g_t[5]` = Chocolate `T5` (`sector.floorz == sprite.z`) for SE7/23.
pub fn init_transport_temps(
    sprites: &[SpriteT],
    sectors: &[SectorT],
    lists: &SpriteLists,
    actor_temps: &mut Vec<crate::con::ActorConTemps>,
) {
    let mut i = lists.headspritestat[STAT_TRANSPORT as usize];
    while i >= 0 {
        let next = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu < sprites.len() {
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, crate::con::ActorConTemps::default);
            }
            let sect = sprites[iu].sectnum;
            let on_floor = sect >= 0
                && (sect as usize) < sectors.len()
                && sectors[sect as usize].floorz == sprites[iu].z;
            actor_temps[iu].g_t[5] = i32::from(on_floor);
        }
        i = next;
    }
}

/// Duke Atomic default `TICSPERFRAME` (`120/26`).
const TICSPERFRAME: i32 = 4;

/// Chocolate crack/FIREEXT spawn `ssp` → `movesprite` XY.
///
/// `ssp` computes Q14 steps `(xvel*sintable)>>14`, then `movesprite` feeds
/// `((step*TICSPERFRAME)<<11)` into `clipmove` (world Δ = `(step*TICSPERFRAME)>>3`).
pub fn ssp_cracks_after_classify(sprites: &mut [SpriteT], sectors: &[SectorT], walls: &[WallT]) {
    use room_build_engine::geom::{clipmove_xy, SectorWalls};
    use room_build_engine::helpers::sincos_q14;

    let sec: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    for s in sprites.iter_mut() {
        let is_crack = (CRACK1..=CRACK1 + 3).contains(&s.picnum) || s.picnum == FIREEXT;
        if !is_crack {
            continue;
        }
        // Chocolate spawn always sets `xvel = 8` then `ssp`.
        s.xvel = 8;
        let xvel = i32::from(s.xvel);
        let (sin, cos) = sincos_q14(i32::from(s.ang));
        let step_x = (xvel * cos) >> 14;
        let step_y = (xvel * sin) >> 14;
        // World units after Chocolate `movesprite` Q14 packing.
        let dx = (step_x * TICSPERFRAME) >> 3;
        let dy = (step_y * TICSPERFRAME) >> 3;
        let walldist = i32::from(s.clipdist) << 2;
        let _ = clipmove_xy(
            &mut s.x,
            &mut s.y,
            &mut s.sectnum,
            dx,
            dy,
            walldist,
            &sec,
            &wxy,
        );
        s.xvel = 0;
    }
}

fn soft_cull_to_misc(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum_out: &mut [i16],
    i: usize,
) {
    sprites[i].xrepeat = 0;
    sprites[i].yrepeat = 0;
    picnum_out[i] = sprites[i].picnum;
    let _ = lists.changespritestat(i as i16, STAT_MISC);
}

fn soft_cull_pickup(s: &SpriteT, skill: u8) -> bool {
    if !SOFT_CULL_PICKUPS.contains(&s.picnum) {
        return false;
    }
    (s.pal != 0) || (s.lotag as u8 > skill)
}

fn classify_stat(s: &SpriteT, actorscrptr: &[i32], actortype: &[u8]) -> i16 {
    match s.picnum {
        SECTOREFFECTOR => {
            if s.lotag == 7 || s.lotag == 23 {
                return STAT_TRANSPORT;
            }
            return STAT_EFFECTOR;
        }
        ACTIVATOR | ACTIVATORLOCKED => return STAT_ACTIVATOR,
        LOCATORS => return STAT_LOCATOR,
        TOUCHPLATE | WATERBUBBLEMAKER | MASTERSWITCH | SEENINE | OOZFILTER | VIEWSCREEN | CONE
        | TREE1 | TREE2 | TIRE | BOX => {
            return STAT_STANDABLE;
        }
        NUKEBUTTON => return STAT_MISC,
        _ => {}
    }
    if DUMMY_ENEMIES.contains(&s.picnum) {
        return STAT_DUMMY;
    }
    // Chocolate inventory/ammo map spawn → STAT_DUMMY + makeitfall (explicit
    // cases), not the `actorscrptr` default.
    if SOFT_CULL_PICKUPS.contains(&s.picnum) || s.picnum == 60 {
        // 60 = ACCESSCARD
        return STAT_DUMMY;
    }
    let pic = s.picnum as usize;
    if pic < actorscrptr.len() && actorscrptr[pic] > 0 {
        // Default `actor` / `useractor`: `actortype&3` → sleep, else awake.
        // FIRE/FIRE2 are plain `actor` → STAT_ACTOR.
        let at = actortype.get(pic).copied().unwrap_or(0);
        return if at & 3 != 0 { STAT_DUMMY } else { STAT_ACTOR };
    }
    if s.statnum == STAT_ACTOR || s.statnum == STAT_DUMMY {
        return s.statnum;
    }
    STAT_DEFAULT
}

/// Chocolate `makeitfall`.
///
/// When `use_getzrange` is true (sprite already on stat 1/2/6/10), floor comes
/// from `getzrange`. Otherwise Chocolate uses raw `sector.floorz` (no slope) —
/// badguy spawn calls `makeitfall` *before* `changespritestat(2)`.
pub fn makeitfall(
    s: &mut SpriteT,
    sectors: &[SectorT],
    walls: &[WallT],
    sprites: &[SpriteT],
    tile_meta: &[TileZMeta],
    gc: i32,
    use_getzrange: bool,
) {
    if s.sectnum < 0 || s.sectnum as usize >= sectors.len() {
        return;
    }
    let sec = &sectors[s.sectnum as usize];
    let c = if sec.lotag == 2 { gc / 6 } else { gc };

    let floorz = if use_getzrange {
        let secs: Vec<SectorZ> = sectors
            .iter()
            .map(|s| SectorZ {
                wallptr: s.wallptr,
                wallnum: s.wallnum,
                ceilingz: s.ceilingz,
                floorz: s.floorz,
                ceilingstat: s.ceilingstat,
                floorstat: s.floorstat,
                ceilingheinum: s.ceilingheinum,
                floorheinum: s.floorheinum,
            })
            .collect();
        let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
        let zclips: Vec<SpriteZClip> = sprites
            .iter()
            .map(|sp| sprite_z_clip(sp, tile_meta))
            .collect();
        getzrange_with_sprites(
            s.x,
            s.y,
            s.z - FOURSLEIGHT,
            s.sectnum,
            127,
            CLIPMASK0,
            &secs,
            &wxy,
            &zclips,
        )
        .florz
    } else {
        sec.floorz
    };
    let rest = floorz - FOURSLEIGHT;
    if s.z < rest {
        let mut zvel = i32::from(s.zvel);
        if zvel < 6144 {
            zvel += c;
        } else {
            zvel = 6144;
        }
        s.z += zvel;
        s.zvel = zvel.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
    // Chocolate `makeitfall`: second `if` clamps even after this-tic overshoot.
    if s.z >= rest {
        s.z = rest;
        s.zvel = 0;
    }
}

/// Chocolate CON `fall` opcode (gamedef.c case 16) — **not** [`makeitfall`].
///
/// Matches Atomic: `getglobalz` when `cgg<=0` or sloped floor, then if/else
/// gravity vs snap to `floorz-FOURSLEIGHT`. `cgg`/`hit_floorz` live on `temps`.
pub fn con_fall(
    s: &mut SpriteT,
    sectors: &[SectorT],
    walls: &[WallT],
    sprites: &[SpriteT],
    tile_meta: &[TileZMeta],
    gc: i32,
    temps: &mut crate::con::ActorConTemps,
) {
    if s.sectnum < 0 || s.sectnum as usize >= sectors.len() {
        return;
    }
    let sec = &sectors[s.sectnum as usize];
    // Chocolate: floorspace → c=0; ceilingspace|lotag2 → gc/6.
    let c = if sec.lotag == 2 { gc / 6 } else { gc };

    if temps.cgg <= 0 || sec.floorstat & 2 != 0 {
        let secs: Vec<SectorZ> = sectors
            .iter()
            .map(|s| SectorZ {
                wallptr: s.wallptr,
                wallnum: s.wallnum,
                ceilingz: s.ceilingz,
                floorz: s.floorz,
                ceilingstat: s.ceilingstat,
                floorstat: s.floorstat,
                ceilingheinum: s.ceilingheinum,
                floorheinum: s.floorheinum,
            })
            .collect();
        let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
        let zclips: Vec<SpriteZClip> = sprites
            .iter()
            .map(|sp| sprite_z_clip(sp, tile_meta))
            .collect();
        let zr = getzrange_with_sprites(
            s.x,
            s.y,
            s.z - FOURSLEIGHT,
            s.sectnum,
            127,
            CLIPMASK0,
            &secs,
            &wxy,
            &zclips,
        );
        temps.hit_ceilingz = zr.ceilz;
        temps.hit_floorz = zr.florz;
        temps.cgg = 6;
    } else {
        temps.cgg = temps.cgg.saturating_sub(1);
    }

    let rest = temps.hit_floorz - FOURSLEIGHT;
    if s.z < rest {
        let mut zvel = i32::from(s.zvel) + c;
        s.z += zvel;
        if zvel > 6144 {
            zvel = 6144;
        }
        s.zvel = zvel.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    } else {
        s.z = rest;
        // Water lotag==1 non-OCTA gets +24<<8; OCTA/COMMANDER/DRONE skip.
        if sec.lotag == 1 {
            const OCTABRAIN: i16 = 1820;
            const COMMANDER: i16 = 1920;
            const DRONE: i16 = 1880;
            if !matches!(s.picnum, OCTABRAIN | COMMANDER | DRONE) {
                s.z += 24 << 8;
            }
        } else {
            s.zvel = 0;
        }
    }
}

/// Run [`makeitfall`] for sleeping actors (STAT_DUMMY). Awake actors get it
/// from AI/`movestandables`; FIRE must not sector-snap every tic.
/// Picnums that Chocolate leaves at map Z (no spawn `makeitfall`).
fn skip_spawn_makeitfall(pic: i16) -> bool {
    matches!(pic, RECON | 675 /* EGG */)
}

/// One-shot Chocolate spawn `makeitfall` for STAT_DUMMY (not per-tic).
///
/// Skips RECON/EGG. Pickups use `getzrange` (Chocolate after `changespritestat(2)`).
/// [`DUMMY_ENEMIES`] use flat `sector.floorz` (Chocolate `makeitfall` before
/// the stat change).
pub fn spawn_makeitfall_dummy(
    lists: &SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    tile_meta: &[TileZMeta],
    gc: i32,
) {
    let mut i = lists.headspritestat[STAT_DUMMY as usize];
    while i >= 0 {
        let next = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu < sprites.len() && !skip_spawn_makeitfall(sprites[iu].picnum) {
            let use_gz = !DUMMY_ENEMIES.contains(&sprites[iu].picnum);
            makeitfall_indexed(sprites, iu, sectors, walls, tile_meta, gc, use_gz);
        }
        i = next;
    }
}

fn makeitfall_indexed(
    sprites: &mut [SpriteT],
    iu: usize,
    sectors: &[SectorT],
    walls: &[WallT],
    tile_meta: &[TileZMeta],
    gc: i32,
    use_getzrange: bool,
) {
    // Collect peers without holding a borrow across mut.
    let peers: Vec<SpriteT> = sprites.to_vec();
    makeitfall(
        &mut sprites[iu],
        sectors,
        walls,
        &peers,
        tile_meta,
        gc,
        use_getzrange,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_build_engine::world::SpriteLists;

    #[test]
    fn map_aplayer_soft_culls_then_egs() {
        let mut lists = SpriteLists::new();
        assert!(lists.place_sprite_at(0, 0, STAT_DEFAULT));
        let mut sprites = vec![SpriteT::default(); 8];
        sprites[0] = SpriteT {
            picnum: APLAYER,
            sectnum: 0,
            statnum: STAT_DEFAULT,
            xrepeat: 42,
            yrepeat: 36,
            ..Default::default()
        };
        // Free slot 1 for EGS.
        assert!(lists.place_sprite_at(1, 0, STAT_DEFAULT));
        lists.deletesprite(1);
        let mut pic = Vec::new();
        classify_map_sprites(&mut lists, &mut sprites, &[], &[], &mut pic, 3);
        assert_eq!(lists.sprite_stat[0], STAT_MISC);
        assert_eq!(sprites[0].xrepeat, 0);
        let i = spawn_player_body(&mut lists, &mut sprites, &mut pic, [10, 20, 30], 100, 0);
        assert!(i >= 0);
        assert_eq!(lists.sprite_stat[i as usize], STAT_PLAYER);
        assert_eq!(sprites[i as usize].picnum, APLAYER);
        assert_eq!(sprites[i as usize].x, 10);
    }

    #[test]
    fn con_actor_promoted() {
        let mut lists = SpriteLists::new();
        assert!(lists.place_sprite_at(3, 0, STAT_DEFAULT));
        let mut sprites = vec![SpriteT::default(); 4];
        sprites[3] = SpriteT {
            picnum: 100,
            sectnum: 0,
            ..Default::default()
        };
        let mut actorscrptr = vec![0_i32; 128];
        actorscrptr[100] = 42;
        let mut pic = Vec::new();
        classify_map_sprites(&mut lists, &mut sprites, &actorscrptr, &[], &mut pic, 3);
        assert_eq!(lists.sprite_stat[3], STAT_DUMMY);
    }

    #[test]
    fn jetpack_pal_soft_culls() {
        let mut lists = SpriteLists::new();
        assert!(lists.place_sprite_at(0, 0, STAT_DEFAULT));
        let mut sprites = vec![SpriteT {
            picnum: 57, // JETPACK
            pal: 1,
            sectnum: 0,
            xrepeat: 32,
            yrepeat: 32,
            ..Default::default()
        }];
        let mut pic = Vec::new();
        classify_map_sprites(&mut lists, &mut sprites, &[], &[], &mut pic, 3);
        assert_eq!(lists.sprite_stat[0], STAT_MISC);
        assert_eq!(sprites[0].xrepeat, 0);
    }

    #[test]
    fn seenine_and_locator_stats() {
        let mut lists = SpriteLists::new();
        assert!(lists.place_sprite_at(0, 0, STAT_DEFAULT));
        assert!(lists.place_sprite_at(1, 0, STAT_DEFAULT));
        assert!(lists.place_sprite_at(2, 0, STAT_DEFAULT));
        let mut sprites = vec![
            SpriteT {
                picnum: SEENINE,
                sectnum: 0,
                ..Default::default()
            },
            SpriteT {
                picnum: LOCATORS,
                sectnum: 0,
                ..Default::default()
            },
            SpriteT {
                picnum: SECTOREFFECTOR,
                lotag: 7,
                sectnum: 0,
                ..Default::default()
            },
        ];
        let mut pic = Vec::new();
        classify_map_sprites(&mut lists, &mut sprites, &[], &[], &mut pic, 3);
        assert_eq!(lists.sprite_stat[0], STAT_STANDABLE);
        assert_eq!(lists.sprite_stat[1], STAT_LOCATOR);
        assert_eq!(lists.sprite_stat[2], STAT_TRANSPORT);
    }
}
