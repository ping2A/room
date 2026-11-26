//! Post-load sprite classification — partial Chocolate `spawn` (D5).
//!
//! `BuildWorld::load_map_bytes` already places sprites on lists using the
//! map's `statnum`. This pass reassigns statuses for markers / effectors /
//! CON actors and soft-culls SP-only / over-skill pickups onto [`STAT_MISC`]
//! with `xrepeat == 0` (later removed by [`crate::explosions::moveexplosions`]).

use room_build_engine::geom::{
    SectorWalls, SectorZ, SpriteZClip, WallXY, getzrange_with_sprites, getzsofslope,
};
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::con::ActorConTemps;
use crate::player::{CLIPMASK0, TileZMeta, sprite_z_clip};

use crate::stat::{
    STAT_ACTIVATOR, STAT_ACTOR, STAT_DEFAULT, STAT_DUMMY, STAT_EFFECTOR, STAT_FALLER, STAT_FX,
    STAT_LOCATOR, STAT_MISC, STAT_PLAYER, STAT_STANDABLE, STAT_TRANSPORT,
};

/// Chocolate `FOURSLEIGHT` — sprite rests this many Build Z units above the floor.
pub const FOURSLEIGHT: i32 = 1 << 8;

/// Picnum for the player body in Atomic Duke.
pub const APLAYER: i16 = 1405;

/// Sector effector sprite picnum.
pub const SECTOREFFECTOR: i16 = 1;
/// Atomic `REACTOR2` tile referenced by `actors.c::moveeffectors` SE16.
pub const REACTOR2: i16 = 578;
/// Atomic `REACTOR` tile referenced by `actors.c::moveeffectors` SE16.
pub const REACTOR: i16 = 1088;

/// `names.h` markers / standables used in status classify.
pub const ACTIVATOR: i16 = 2;
/// Atomic `TOUCHPLATE` tile.
pub const TOUCHPLATE: i16 = 3;
/// Atomic `ACTIVATORLOCKED` tile.
pub const ACTIVATORLOCKED: i16 = 4;
/// Atomic `MUSICANDSFX` tile.
pub const MUSICANDSFX: i16 = 5;
/// Atomic `LOCATORS` tile.
pub const LOCATORS: i16 = 6;
/// Atomic `MASTERSWITCH` tile.
pub const MASTERSWITCH: i16 = 8;
/// Atomic `RESPAWN` tile.
pub const RESPAWN: i16 = 9;
/// Atomic red/yellow/green item-respawn marker tiles (`names.h`).
pub const RESPAWNMARKERRED: i16 = 3190;
/// Atomic `RESPAWNMARKERYELLOW` tile.
pub const RESPAWNMARKERYELLOW: i16 = 3200;
/// Atomic `RESPAWNMARKERGREEN` tile.
pub const RESPAWNMARKERGREEN: i16 = 3210;
/// Atomic `TRANSPORTERSTAR` tile.
pub const TRANSPORTERSTAR: i16 = 1630;
/// Atomic `BOLT1` tile.
pub const BOLT1: i16 = 634;
/// Atomic `WATERBUBBLE` tile.
pub const WATERBUBBLE: i16 = 661;
/// Atomic `WATERBUBBLEMAKER` tile.
pub const WATERBUBBLEMAKER: i16 = 662;
/// Atomic `FIREEXT` tile.
pub const FIREEXT: i16 = 916;
/// Atomic `SEENINE` tile.
pub const SEENINE: i16 = 1247;
/// Atomic `OOZFILTER` tile.
pub const OOZFILTER: i16 = 1079;
/// Atomic `CRACK1` tile.
pub const CRACK1: i16 = 546;
/// Atomic `VIEWSCREEN` tile.
pub const VIEWSCREEN: i16 = 502;
/// Duke Atomic alternate monitor tile.
pub const VIEWSCREEN2: i16 = 499;
/// Duke Atomic live-camera monitor texture.
pub const STATIC: i16 = 351;
/// Atomic `CONE` tile.
pub const CONE: i16 = 978;
/// Atomic `CAMERA1` tile.
pub const CAMERA1: i16 = 621;
/// Atomic `NUKEBUTTON` tile.
pub const NUKEBUTTON: i16 = 142;
/// Atomic `GREENSLIME` tile.
pub const GREENSLIME: i16 = 2370;
/// Atomic `RECON` tile.
pub const RECON: i16 = 1960;
/// Atomic `CANWITHSOMETHING` tile.
pub const CANWITHSOMETHING: i16 = 1232;
/// Atomic `TREE1` tile.
pub const TREE1: i16 = 908;
/// Atomic `WATERFOUNTAIN` tile.
pub const WATERFOUNTAIN: i16 = 563;
/// Atomic `TOILET` tile.
pub const TOILET: i16 = 569;
/// Atomic `STALL` tile.
pub const STALL: i16 = 571;
/// Atomic `PLUG` tile.
pub const PLUG: i16 = 1069;
/// Atomic `TREE2` tile.
pub const TREE2: i16 = 910;
/// Atomic `TIRE` tile.
pub const TIRE: i16 = 990;
/// Atomic `BOX` tile.
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
    1267, // RAT (C-coded in `moveactors`, no CON body)
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
/// Atomic `FIRE2` tile.
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
    classify_map_sprites_stats(lists, sprites, actorscrptr, actortype, picnum_out, skill);
    hide_map_markers(lists, sprites);
}

/// Chocolate `spawn()` hides the editor-only marker tiles it keeps alive:
/// `cstat |= 32768` for MUSICANDSFX/RESPAWN, LOCATORS, MASTERSWITCH,
/// TOUCHPLATE and WATERBUBBLEMAKER; `cstat = 32768` for ACTIVATOR(LOCKED).
fn hide_map_markers(lists: &SpriteLists, sprites: &mut [SpriteT]) {
    for (i, sprite) in sprites.iter_mut().enumerate() {
        if lists.sprite_stat.get(i).copied() == Some(room_build_engine::world::MAXSTATUS as i16)
            || sprite.xrepeat == 0
        {
            continue;
        }
        match sprite.picnum {
            MUSICANDSFX | RESPAWN | LOCATORS | MASTERSWITCH | TOUCHPLATE | WATERBUBBLEMAKER => {
                sprite.cstat |= 32768;
            }
            ACTIVATOR | ACTIVATORLOCKED => sprite.cstat = 32768,
            _ => {}
        }
    }
}

fn classify_map_sprites_stats(
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

        // Chocolate `spawn()`'s early tagged-hit-sprite dispatch. Its
        // exceptions precede the normal tile-specific STAT setup below.
        let pic = sprites[i].picnum;
        let excluded_faller_tile = matches!(pic, 4397 | 4502 | 4361 | 4359 | 2566 | 499 | 502)
            || (CRACK1..=CRACK1 + 3).contains(&pic);
        let switch_tile = is_wall_switch_tile(pic);
        if !excluded_faller_tile
            && sprites[i].cstat & 48 != 0
            && !(switch_tile && sprites[i].cstat & 16 != 0)
            && sprites[i].hitag != 0
        {
            picnum_out[i] = pic;
            sprites[i].cstat |= 257;
            let _ = lists.changespritestat(i as i16, STAT_FALLER);
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

        // DUKECAR / HELECOPT (E1L1 intro shuttle): Chocolate `spawn()`
        // falls through to the BLIMP case → cstat 257, clipdist 128, STAT_ACTOR.
        if sprites[i].picnum == crate::actors::DUKECAR
            || sprites[i].picnum == crate::actors::HELECOPT
        {
            picnum_out[i] = sprites[i].picnum;
            sprites[i].cstat = 257;
            sprites[i].extra = 1;
            sprites[i].xvel = 292;
            sprites[i].zvel = 360;
            sprites[i].clipdist = 128;
            let _ = lists.changespritestat(i as i16, STAT_ACTOR);
            continue;
        }

        // Chocolate `spawn()` TOILET/STALL: `lotag = 1` makes them
        // `neartag`-usable; solid with a tight clipdist.
        if matches!(sprites[i].picnum, TOILET | STALL) {
            picnum_out[i] = sprites[i].picnum;
            sprites[i].lotag = 1;
            sprites[i].cstat |= 257;
            sprites[i].clipdist = 8;
            sprites[i].owner = i as i16;
            continue;
        }
        // WATERFOUNTAIN (`SLT = 1`, falls into TREE1…BOX) and PLUG
        // (`lotag = 9999`) both live on STAT_STANDABLE.
        if matches!(sprites[i].picnum, WATERFOUNTAIN | PLUG) {
            picnum_out[i] = sprites[i].picnum;
            if sprites[i].picnum == WATERFOUNTAIN {
                sprites[i].lotag = 1;
                sprites[i].cstat = 257;
                sprites[i].extra = 1;
            } else {
                sprites[i].lotag = 9999;
            }
            if lists.sprite_stat[i] != STAT_STANDABLE {
                let _ = lists.changespritestat(i as i16, STAT_STANDABLE);
            }
            continue;
        }

        // Chocolate `spawn()`: breakable props (HYDRENT, PANNEL*, BOTTLE*,
        // CHAIR*, VASE, PIPE*, ...) become solid and hitscan-hittable with
        // clipdist 32 on STAT_DEFAULT; FEMMAG loses both bits.
        if is_solid_prop_tile(sprites[i].picnum) {
            picnum_out[i] = sprites[i].picnum;
            sprites[i].clipdist = 32;
            sprites[i].cstat |= 257;
            if lists.sprite_stat[i] != 0 {
                let _ = lists.changespritestat(i as i16, 0);
            }
            continue;
        }
        if matches!(sprites[i].picnum, 568 | 577) {
            picnum_out[i] = sprites[i].picnum;
            sprites[i].cstat &= !257;
            if lists.sprite_stat[i] != 0 {
                let _ = lists.changespritestat(i as i16, 0);
            }
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
        // Chocolate `spawn()` RECON: 130 health, hittable, single-player
        // drops palette variants, then STAT_ZOMBIEACTOR until seen.
        if sprites[i].picnum == RECON {
            sprites[i].extra = 130;
            sprites[i].cstat |= 257;
            if sprites[i].pal != 0 {
                soft_cull_to_misc(lists, sprites, picnum_out, i);
                continue;
            }
            sprites[i].shade = -17;
        }

        picnum_out[i] = sprites[i].picnum;
        let want = classify_stat(&sprites[i], actorscrptr, actortype);
        if lists.sprite_stat[i] != want {
            let _ = lists.changespritestat(i as i16, want);
        }
    }
}

/// Chocolate `spawn()` TOUCHPLATE: `T3` keeps the sector's floor height at
/// spawn (the plate's lowered position) and, outside water sectors, the floor
/// starts at the plate sprite's z. Without `T3` the plate sank its floor
/// toward z 0 the first time the player stepped on it.
pub(crate) fn init_touchplates(
    lists: &SpriteLists,
    sprites: &[SpriteT],
    sectors: &mut [SectorT],
    temps: &mut Vec<ActorConTemps>,
) {
    for (index, sprite) in sprites.iter().enumerate() {
        if sprite.picnum != TOUCHPLATE
            || lists
                .sprite_stat
                .get(index)
                .copied()
                .unwrap_or(room_build_engine::world::MAXSTATUS as i16)
                == room_build_engine::world::MAXSTATUS as i16
        {
            continue;
        }
        let Some(sector) = usize::try_from(sprite.sectnum)
            .ok()
            .and_then(|s| sectors.get_mut(s))
        else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        temps[index].g_t[2] = sector.floorz;
        if sector.lotag != 1 && sector.lotag != 2 {
            sector.floorz = sprite.z;
        }
    }
}

/// Initialize Chocolate `spawn()` health for tagged sprites on STAT_FALLER.
pub(crate) fn init_faller_sprites(
    lists: &SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    temps: &mut Vec<ActorConTemps>,
    impact_damage: i32,
) {
    for (index, sprite) in sprites.iter_mut().enumerate() {
        if lists.sprite_stat.get(index).copied() == Some(STAT_FALLER) {
            sprite.extra = impact_damage as i16;
            if temps.len() <= index {
                temps.resize_with(index + 1, ActorConTemps::default);
            }
            if let Some(sector) = usize::try_from(sprite.sectnum)
                .ok()
                .and_then(|sector| sectors.get(sector))
            {
                // Chocolate map `spawn()` seeds these un-slope-adjusted bounds.
                temps[index].hit_floorz = sector.floorz;
                temps[index].hit_ceilingz = sector.ceilingz;
            }
        }
    }
}

/// Chocolate `spawn()` HYDRENT…PIPE6 case: `clipdist = 32; cstat |= 257`.
pub(crate) fn is_solid_prop_tile(pic: i16) -> bool {
    matches!(
        pic,
        981 | 342
            | 343
            | 516
            | 517
            | 602
            | 607
            | 595
            | 556
            | 557
            | 680
            | 954
            | 955
            | 956
            | 957
            | 1012
            | 1013
            | 1025
            | 1014
            | 1157
            ..=1166
                | 971
                | 972
                | 973
                | 975
                | 1352
                | 1355
                | 1354
                | 4943
                | 678
                | 669
                | 407
                | 911
                | 939
                | 979
                | 1358
                | 1359
                | 685
                | 686
                | 689
                | 694
                | 695
                | 697
                | 4444
                | 768
                | 769
                | 774
                | 779
                | 792
                | 716
                | 765
                | 619
                | 616
                | 618
                | 996
                | 994
                | 995
    )
}

/// Chocolate `wallswitchcheck()` tile set, used by the early `spawn()` path.
fn is_wall_switch_tile(pic: i16) -> bool {
    matches!(
        pic,
        1155 | 1156 // HANDPRINTSWITCH
            | 1142 | 1143 // ALIENSWITCH
            | 146
            ..=149 // MULTISWITCH
            | 130 | 170 // ACCESS SWITCHES
            | 1122 | 1123 // PULLSWITCH
            | 1111 | 1112 // HANDSWITCH
            | 132 | 133 // SLOTDOOR
            | 134 | 135 // LIGHTSWITCH
            | 138 | 139 // SPACELIGHTSWITCH
            | 136 | 137 // SPACEDOORSWITCH
            | 140 | 141 // FRANKENSTINESWITCH
            | 712 | 713 // LIGHTSWITCH2
            | 860 | 861 // POWERSWITCH1
            | 862 | 863 // LOCKSWITCH1
            | 864 | 865 // POWERSWITCH2
            | 162 | 163 // DIPSWITCH
            | 164 | 165 // DIPSWITCH2
            | 166 | 167 // TECHSWITCH
            | 168 | 169 // DIPSWITCH3
    )
}

/// Chocolate `spawn()` initialization for tagged destructible explosive
/// sprites (`SEENINE`, `OOZFILTER`) and the cracks / fire extinguishers.
pub(crate) fn init_destructible_explosives(
    lists: &SpriteLists,
    sprites: &mut [SpriteT],
    impact_damage: i32,
) {
    for (index, sprite) in sprites.iter_mut().enumerate() {
        if lists.sprite_stat.get(index).copied() != Some(STAT_STANDABLE) {
            continue;
        }
        // `case CRACK1..4 / FIREEXT`: FIREEXT `extra = impact_damage<<2`,
        // a crack `extra = 1` (blast hits need `extra >= 0`).
        if sprite.picnum == FIREEXT {
            sprite.extra = impact_damage.wrapping_shl(2) as i16;
            continue;
        }
        if (CRACK1..=CRACK1 + 3).contains(&sprite.picnum) {
            sprite.extra = 1;
            continue;
        }
        if !matches!(sprite.picnum, SEENINE | OOZFILTER) {
            continue;
        }
        sprite.shade = -16;
        if sprite.xrepeat <= 8 {
            sprite.cstat = 32768;
            sprite.xrepeat = 0;
            sprite.yrepeat = 0;
        } else {
            sprite.cstat = 1 + 256;
        }
        sprite.extra = impact_damage.wrapping_shl(2) as i16;
        sprite.owner = index as i16;
    }
}

/// Apply Chocolate `spawn()`'s skill and `ud.monsters_off` gates after map
/// actors have been classified and their CON actor type is available.
pub fn apply_monster_spawn_rules(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    actortype: &[u8],
    skill: u8,
    monsters_off: bool,
) {
    for (index, sprite) in sprites.iter_mut().enumerate() {
        if lists
            .sprite_stat
            .get(index)
            .copied()
            .unwrap_or(room_build_engine::world::MAXSTATUS as i16)
            == room_build_engine::world::MAXSTATUS as i16
            || !crate::badguy::badguy_pic(sprite.picnum, actortype)
        {
            continue;
        }
        let is_egg = sprite.picnum == 675;
        let above_skill = !is_egg && sprite.lotag > i16::from(skill);
        if !monsters_off && !above_skill {
            continue;
        }
        sprite.xrepeat = 0;
        sprite.yrepeat = 0;
        let _ = lists.changespritestat(index as i16, crate::stat::STAT_MISC);
    }
}

/// Count map enemies Chocolate adds to `ps[myconnectindex].max_actors_killed`
/// while processing `spawn()` during prelevel.
#[must_use]
pub(crate) fn count_prelevel_kills(
    lists: &SpriteLists,
    sprites: &[SpriteT],
    actorscrptr: &[i32],
    actortype: &[u8],
    skill: u8,
    monsters_off: bool,
) -> u8 {
    let mut count = 0_u8;
    for (index, sprite) in sprites.iter().enumerate() {
        if lists
            .sprite_stat
            .get(index)
            .copied()
            .unwrap_or(room_build_engine::world::MAXSTATUS as i16)
            == room_build_engine::world::MAXSTATUS as i16
            || sprite.sectnum < 0
            || sprite.lotag > i16::from(skill)
        {
            continue;
        }
        let pic = sprite.picnum;
        let vanilla_enemy = matches!(
            pic,
            RECON
                | 1880 // DRONE
                | 1680..=1682 // LIZTROOP / RUNNING / STAYPUT
                | 1715 | 1725 | 1741 | 1742 | 1744
                | 1820..=1821 // OCTABRAIN
                | 1920..=1921 // COMMANDER
                | 2000..=2001 | 2045 // PIGCOP variants
                | 2120 | 2121 | 2150 | 2160 | 2165 // LIZMAN variants
                | 2420 // ORGANTIC
                | 2630..=2631 // BOSS1
                | 2710 | 2760 | 4740 | 4741 // BOSS2-4
                | 2360 // ROTATEGUN
                | 2370..=2377 // GREENSLIME
        );
        let scripted_sleeping_enemy = actorscrptr
            .get(pic.max(0) as usize)
            .is_some_and(|&entry| entry > 0)
            && actortype
                .get(pic.max(0) as usize)
                .is_some_and(|actor_type| actor_type & 3 != 0);
        let enabled = pic == RECON || !monsters_off;
        if enabled && (vanilla_enemy || scripted_sleeping_enemy) {
            count = count.wrapping_add(1);
        }
    }
    count
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
        // Chocolate `premap()` assigns the first zero-palette APLAYER the
        // first player lookup palette (9) before rendering the world.
        pal: 9,
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
pub fn spawn_sector_effectors(
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    temps: &mut Vec<ActorConTemps>,
) {
    // Collect lotag-1 pivots by hitag (first match wins, sprite index order).
    let mut pivots: Vec<(i16, usize, i32, i32)> = Vec::new();
    for (index, s) in sprites.iter().enumerate() {
        if s.picnum == SECTOREFFECTOR && s.lotag == 1 {
            pivots.push((s.hitag, index, s.x, s.y));
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
        let shade = sprites[i].shade;
        let hitag = sprites[i].hitag;
        let sect = sprites[i].sectnum;
        if sect < 0 || sect as usize >= sectors.len() {
            continue;
        }
        // Chocolate `spawn` assigns `SP = sector[sect].extra` for every SE.
        sprites[i].yvel = sectors[sect as usize].extra;

        match lotag {
            // Chocolate game.c `spawn()` initializes the swing-door turn
            // direction before the common invisible-effector setup.
            11 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                temps[i].g_t[3] = if ang > 1024 { 2 } else { -2 };
            }
            // Chocolate game.c `spawn()` stores the escalator's map-load
            // origin in T4/T5 and derives its vertical travel direction from
            // the sector floor shade before the runtime shade cycle begins.
            26 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                temps[i].g_t[3] = sprites[i].x;
                temps[i].g_t[4] = sprites[i].y;
                sprites[i].zvel = if shade == sectors[sect as usize].floorshade {
                    -256
                } else {
                    256
                };
                sprites[i].shade = 0;
            }
            // Chocolate gives SE28 its initial 65-tic lightning delay.
            28 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                temps[i].g_t[5] = 65;
            }
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
                sprites[i].owner = -1;
                if let Some((_, pivot, px, py)) = pivots.iter().find(|(h, _, _, _)| *h == hitag) {
                    sprites[i].owner = *pivot as i16;
                    if ang == 512 {
                        sprites[i].x = *px;
                        sprites[i].y = *py;
                    }
                }
                if sectors[sect as usize].lotag & 0x3fff == 30 {
                    sectors[sect as usize].hitag = i as i16;
                    sprites[i].clipdist = u8::from(pal != 0);
                    if temps.len() <= i {
                        temps.resize_with(i + 1, ActorConTemps::default);
                    }
                    temps[i].g_t[4] = sectors[sect as usize].floorz;
                }
            }
            13 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                let sec = &mut sectors[sect as usize];
                temps[i].g_t[0] = sec.ceilingz;
                temps[i].g_t[1] = sec.floorz;
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
                    temps[i].g_t[3] = 1;
                    if !owner_ceil && ang == 512 {
                        sec.ceilingstat ^= 1;
                        temps[i].g_t[3] = 0;
                    }
                    sec.ceilingshade = sec.floorshade;
                }
            }
            18 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                let sector = &mut sectors[sect as usize];
                temps[i].g_t[1] = if ang == 512 {
                    sector.ceilingz
                } else {
                    sector.floorz
                };
                if pal != 0 {
                    if ang == 512 {
                        sector.ceilingz = z;
                    } else {
                        sector.floorz = z;
                    }
                }
                // Chocolate scales the runtime countdown interval in spawn.
                sprites[i].hitag = hitag.wrapping_mul(4);
            }
            1 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                sprites[i].owner = -1;
                // Chocolate initializes SE1 T1 to zero; a linked SE19 later
                // arms the pivot group when its shield is activated.
                temps[i].g_t[0] = 0;
            }
            2 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                temps[i].g_t[5] = i32::from(sectors[sect as usize].floorheinum);
            }
            11 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                // Chocolate map `spawn`: SE11 T4 sets its swing direction.
                temps[i].g_t[3] = if ang > 1024 { 2 } else { -2 };
            }
            19 => {
                sprites[i].owner = -1;
            }
            26 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                temps[i].g_t[3] = sprites[i].x;
                temps[i].g_t[4] = sprites[i].y;
                sprites[i].zvel = if sprites[i].shade == sectors[sect as usize].floorshade {
                    -256
                } else {
                    256
                };
                sprites[i].shade = 0;
            }
            25 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                temps[i].g_t[3] = sectors[sect as usize].ceilingz;
                temps[i].g_t[4] = 1;
                sectors[sect as usize].ceilingz = z;
            }
            35 => {
                if temps.len() <= i {
                    temps.resize_with(i + 1, ActorConTemps::default);
                }
                // Chocolate only moves the ceiling to the SE marker here;
                // it leaves T5 (temp_data[4]) at its zero-initialized value.
                // moveeffectors uses that value as the return clamp.
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
    use room_build_engine::geom::{SectorWalls, clipmove_xy};
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

pub(crate) fn classify_stat(s: &SpriteT, actorscrptr: &[i32], actortype: &[u8]) -> i16 {
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
    // Chocolate `floorspace` / `ceilingspace` use the two outer-space sky
    // textures only when the ceiling palette is the base palette.
    const MOONSKY1: i16 = 80;
    const BIGORBIT1: i16 = 84;
    let outer_sky = |picnum| picnum == MOONSKY1 || picnum == BIGORBIT1;
    let floor_space = sec.floorstat & 1 != 0 && sec.ceilingpal == 0 && outer_sky(sec.floorpicnum);
    let ceiling_space =
        sec.ceilingstat & 1 != 0 && sec.ceilingpal == 0 && outer_sky(sec.ceilingpicnum);
    let c = if floor_space {
        0
    } else if ceiling_space || sec.lotag == 2 {
        gc / 6
    } else {
        gc
    };

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
        if sec.lotag == 2 && zvel > 3122 {
            zvel = 3144;
        }
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
        // Water lotag==1 actors use a 24-unit default floor offset. EDuke32's
        // `htwaterzoffset` replaces that default, so setting 24 removes the
        // extra offset while leaving the Atomic default unchanged.
        if sec.lotag == 1 {
            const OCTABRAIN: i16 = 1820;
            const COMMANDER: i16 = 1920;
            const DRONE: i16 = 1880;
            if !matches!(s.picnum, OCTABRAIN | COMMANDER | DRONE) {
                s.z =
                    s.z.wrapping_add((24_i32 - i32::from(temps.water_z_offset)).wrapping_shl(8));
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
/// `DUMMY_ENEMIES` use flat `sector.floorz` (Chocolate `makeitfall` before
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

/// Chocolate `spawn(-1, i)` random rolls at map load (`game.c`), in sprite
/// index order like `prelevel`'s spawn loop.
///
/// Room's [`classify_map_sprites`] mirrors the status / culling side of
/// `spawn`; this pass adds the `TRAND` side so the level starts with the same
/// `randomseed` as vanilla, and with the same random cstat / size / angle on
/// the sprites that roll. DEMO1 (E3L11) rolls 13 times here: 6 cans, 1
/// exploding barrel, 4 eggs (`cstat = 257 | (TRAND & 4)`) and one blood splat
/// (two `7 + (TRAND & 7)` repeats).
///
/// `alive` says whether a sprite index survived `prelevel`'s marker deletes
/// (`statnum < MAXSTATUS`). `monsters_off` is `ud.monsters_off`.
pub fn spawn_load_rolls(
    sprites: &mut [SpriteT],
    alive: &dyn Fn(usize) -> bool,
    actor_temps: &mut Vec<crate::con::ActorConTemps>,
    randomseed: &mut i32,
    skill: u8,
    monsters_off: bool,
) {
    const WATERDRIP: i16 = 660;
    const EGG: i16 = 675;
    const WOODENHORSE: i16 = 904;
    const BULLETHOLE: i16 = 952;
    const HORSEONSIDE: i16 = 1026;
    const RUBBERCAN: i16 = 1062;
    const NUKEBARREL: i16 = 1227;
    const NUKEBARRELDENTED: i16 = 1228;
    const NUKEBARRELLEAKED: i16 = 1229;
    const MONEY: i16 = 1233;
    const EXPLODINGBARREL: i16 = 1238;
    const FIREBARREL: i16 = 1240;
    const RAT: i16 = 1267;
    const TRASH: i16 = 1272;
    const WATERSPLASH2: i16 = 1380;
    const FIREVASE: i16 = 1390;
    const MAIL: i16 = 4410;
    const PAPER: i16 = 4460;
    const CANWITHSOMETHING2: i16 = 4580;
    const CANWITHSOMETHING4: i16 = 4582;
    let roll = |seed: &mut i32| room_build_engine::numeric::krand_step(seed);
    for i in 0..sprites.len() {
        if !alive(i) {
            continue;
        }
        let sp = &mut sprites[i];
        // `spawn(-1, i)` header: `OW = hittype[i].owner = i` for every map
        // sprite (`ifspawnedby`, `ifactor` owner checks rely on it).
        sp.owner = i as i16;
        match sp.picnum {
            WATERSPLASH2 => {
                let r = 16 + (roll(randomseed) & 15);
                sp.xrepeat = r as u8;
                sp.yrepeat = r as u8;
            }
            2296..=2299 => {
                // BLOODSPLAT1…4 (cstat / z handled by classify).
                sp.xrepeat = (7 + (roll(randomseed) & 7)) as u8;
                sp.yrepeat = (7 + (roll(randomseed) & 7)) as u8;
            }
            BULLETHOLE => {
                sp.xrepeat = 3;
                sp.yrepeat = 3;
                sp.cstat = (16 + (roll(randomseed) & 12)) as u16;
            }
            MONEY | MAIL | PAPER => {
                let t0 = roll(randomseed) & 2047;
                if actor_temps.len() <= i {
                    actor_temps.resize_with(i + 1, Default::default);
                }
                actor_temps[i].g_t[0] = t0;
                sp.cstat = (roll(randomseed) & 12) as u16;
                sp.xrepeat = 8;
                sp.yrepeat = 8;
                sp.ang = (roll(randomseed) & 2047) as i16;
            }
            WATERDRIP => {
                // j == -1: `z += 4<<8; T1 = z; T2 = TRAND & 127`.
                sp.z += 4 << 8;
                if actor_temps.len() <= i {
                    actor_temps.resize_with(i + 1, Default::default);
                }
                actor_temps[i].g_t[0] = sp.z;
                actor_temps[i].g_t[1] = roll(randomseed) & 127;
            }
            TRASH => sp.ang = (roll(randomseed) & 2047) as i16,
            RAT => {
                if i32::from(sp.lotag) <= i32::from(skill) && !monsters_off {
                    sp.ang = (roll(randomseed) & 2047) as i16;
                    sp.xrepeat = 48;
                    sp.yrepeat = 48;
                    sp.cstat = 0;
                }
            }
            CANWITHSOMETHING
            | CANWITHSOMETHING2..=CANWITHSOMETHING4
            | RUBBERCAN
            | EXPLODINGBARREL
            | HORSEONSIDE
            | FIREBARREL
            | NUKEBARREL
            | FIREVASE
            | NUKEBARRELDENTED
            | NUKEBARRELLEAKED
            | WOODENHORSE
            | EGG => {
                if sp.picnum == EGG && monsters_off {
                    continue;
                }
                sp.clipdist = if sp.picnum == EGG { 24 } else { 72 };
                sp.cstat = (257 | (roll(randomseed) & 4)) as u16;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod load_roll_tests {
    use super::*;

    #[test]
    fn barrels_eggs_and_splats_roll_like_chocolate_spawn() {
        // Index order, one roll per can/barrel/egg, two per blood splat.
        let mut sprites = vec![
            SpriteT {
                picnum: CANWITHSOMETHING,
                ..Default::default()
            },
            SpriteT {
                picnum: 2297,
                ..Default::default()
            }, // BLOODSPLAT3
            SpriteT {
                picnum: 675,
                ..Default::default()
            }, // EGG
            SpriteT {
                picnum: 100,
                ..Default::default()
            }, // no roll
        ];
        let mut temps = Vec::new();
        let mut seed = 17_i32;
        spawn_load_rolls(&mut sprites, &|_| true, &mut temps, &mut seed, 2, false);
        let mut expect = 17_i32;
        for _ in 0..4 {
            let _ = room_build_engine::numeric::krand_step(&mut expect);
        }
        assert_eq!(seed, expect);
        assert_eq!(sprites[0].cstat & 257, 257);
        assert_eq!(sprites[0].clipdist, 72);
        assert_eq!(sprites[2].clipdist, 24);
        assert!((7..=14).contains(&sprites[1].xrepeat));
        // `spawn(-1, i)` header: every map sprite owns itself.
        assert!(sprites.iter().enumerate().all(|(i, s)| s.owner == i as i16));
    }

    #[test]
    fn con_fall_applies_eduke_water_floor_offset() {
        let floorz = 100 << 8;
        let rest = floorz - FOURSLEIGHT;
        let placement = |water_z_offset: i8, picnum: i16| {
            let mut sprite = SpriteT {
                picnum,
                sectnum: 0,
                z: rest,
                ..SpriteT::default()
            };
            let sectors = [SectorT {
                lotag: 1,
                floorz,
                ..SectorT::default()
            }];
            let mut temps = ActorConTemps {
                cgg: 1,
                hit_floorz: floorz,
                water_z_offset,
                ..ActorConTemps::default()
            };
            let peers = [sprite];
            con_fall(&mut sprite, &sectors, &[], &peers, &[], 0, &mut temps);
            sprite.z
        };

        assert_eq!(placement(0, 100), rest + (24 << 8));
        assert_eq!(placement(24, 100), rest);
        assert_eq!(placement(0, 1820), rest);
    }
}
