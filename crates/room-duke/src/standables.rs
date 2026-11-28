//! STAT_STANDABLE movers — Chocolate `movestandables` (D5 subset).
//!
//! `BOLT1`…`BOLT1+3` spark animation; `WATERBUBBLEMAKER` CON subset.

use room_build_engine::geom::{
    SectorWalls, SectorZ, SpriteHit, SpriteZClip, WallXY, hitscan_full, movesprite_xy_at_z,
};
use room_build_engine::numeric::krand_step;
use room_build_engine::types::SpriteT;
use room_build_engine::world::{BuildWorld, SpriteLists};

use crate::con::ActorConTemps;
use crate::player::{Player, TileZMeta};
use crate::spawn::{BOLT1, CRACK1, FIREEXT, WATERBUBBLE, WATERBUBBLEMAKER};
use crate::stat::{STAT_MISC, STAT_STANDABLE};

const SEENINE: i16 = 1247;
#[allow(dead_code, reason = "Duke tile number, kept next to SEENINE")]
const SEENINEDEAD: i16 = 1248;
#[allow(dead_code, reason = "Duke tile number, kept next to SEENINE")]
const SEENINEDEAD2: i16 = 1249;
const OOZFILTER: i16 = 1079;
const PIPEBOMB_EXPLODE: i16 = 14;
const TRIPBOMB: i16 = 2566;
const LASERLINE: i16 = 2567;
const SCRAP6: i16 = 2390;
const SCRAP3: i16 = 2402;
const RADIUSEXPLOSION: i16 = 1670;
const RPG: i16 = 2605;
const GLASS_HEAVYBREAK: i16 = 20;
const PIPEBOMB_BLAST_RADIUS_FALLBACK: i32 = 2500;

fn klabs(x: i32) -> i32 {
    if x == i32::MIN { i32::MIN } else { x.abs() }
}

/// Chocolate `findplayer` distance (single-player): manhattan on `opos` + Z/16.
fn bolt_player_dist(s: &SpriteT, p: &Player) -> i32 {
    klabs(p.oposx - s.x) + klabs(p.oposy - s.y) + (klabs(p.oposz - s.z + (28 << 8)) >> 4)
}

/// Chocolate `rnd(X)`.
fn rnd(randomseed: &mut i32, x: i32) -> bool {
    let t = krand_step(randomseed);
    (t >> 8) >= (255 - x)
}

fn actor_tile(script: Option<&crate::con::ScriptImage>, name: &str, fallback: i16) -> i16 {
    script
        .and_then(|image| image.labels.get(name))
        .filter(|label| !label.is_ptr)
        .map_or(fallback, |label| label.value as i16)
}

/// Activate the native hitag chain shared by C-9 explosions and tagged props.
fn arm_explosive_hitag(world: &mut BuildWorld, actor_temps: &mut Vec<ActorConTemps>, hitag: i16) {
    let mut effector = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
    while effector >= 0 {
        let index = effector as usize;
        effector = world.lists.nextspritestat[index];
        if world.sprites[index].hitag != hitag {
            continue;
        }
        if actor_temps.len() <= index {
            actor_temps.resize_with(index + 1, ActorConTemps::default);
        }
        match world.sprites[index].lotag {
            13 if actor_temps[index].g_t[2] == 0 => actor_temps[index].g_t[2] = 1,
            8 => actor_temps[index].g_t[4] = 1,
            18 if actor_temps[index].g_t[0] == 0 => actor_temps[index].g_t[0] = 1,
            21 => actor_temps[index].g_t[0] = 1,
            _ => {}
        }
    }
    let mut linked = world.lists.headspritestat[STAT_STANDABLE as usize];
    while linked >= 0 {
        let index = linked as usize;
        linked = world.lists.nextspritestat[index];
        if world.sprites.get(index).is_some_and(|sprite| {
            sprite.hitag == hitag && matches!(sprite.picnum, SEENINE | OOZFILTER)
        }) && world.sprites[index].shade != -32
        {
            world.sprites[index].shade = -32;
        }
    }
}

fn spawn_random_scraps(
    world: &mut BuildWorld,
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    source: SpriteT,
    source_i: i16,
    count: usize,
) {
    for _ in 0..count {
        // Chocolate `RANDOMSCRAP`: 32-bit gcc evaluates the EGS arguments right
        // to left, so zvel draws first and x last.
        let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
        let xvel = (64 + (krand_step(randomseed) & 63)) as i16;
        let angle = (krand_step(randomseed) & 2047) as i16;
        let tile = actor_tile(script, "SCRAP6", SCRAP6) + (krand_step(randomseed) & 15) as i16;
        let z = source
            .z
            .wrapping_sub(8 << 8)
            .wrapping_sub(krand_step(randomseed) & 8191);
        let y = source.y.wrapping_add((krand_step(randomseed) & 255) - 128);
        let x = source.x.wrapping_add((krand_step(randomseed) & 255) - 128);
        let _ = crate::shoot::egs(
            &mut world.lists,
            &mut world.sprites[..],
            picnum,
            actor_temps,
            script,
            source.sectnum,
            x,
            y,
            z,
            tile,
            -8,
            48,
            48,
            angle,
            xvel,
            zvel,
            source_i,
            STAT_MISC,
        );
    }
}

fn mark_sprite_deleted(world: &mut BuildWorld, picnum: &mut Vec<i16>, sprite_index: usize) {
    let _ = world.lists.deletesprite(sprite_index as i16);
    if picnum.len() <= sprite_index {
        picnum.resize(sprite_index + 1, 0);
    }
    picnum[sprite_index] = -1;
}

/// Advance Atomic tagged CRACK and FIREEXT standables after `ifhitbyweapon`.
/// Returns true when the source consumes a hit or detonates and the caller
/// should skip the ordinary SEENINE path.
fn tick_breakable_standable(
    world: &mut BuildWorld,
    sprite_index: usize,
    players: &mut [Player],
    tile_meta: &[TileZMeta],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    earthquake_time: &mut u8,
) -> bool {
    let Some(mut thing) = world.sprites.get(sprite_index).copied() else {
        return false;
    };
    let fire_ext_tile = actor_tile(script, "FIREEXT", FIREEXT);
    let crack1_tile = actor_tile(script, "CRACK1", CRACK1);
    let is_fire_ext = thing.picnum == fire_ext_tile;
    let is_crack = (crack1_tile..=crack1_tile.saturating_add(3)).contains(&thing.picnum);
    if !is_fire_ext && !is_crack {
        return false;
    }
    if actor_temps.len() <= sprite_index {
        actor_temps.resize_with(sprite_index + 1, ActorConTemps::default);
    }
    // Chocolate: a crack only reacts with `hitag > 0`; then every tic it
    // runs `ifhitbyweapon` (which needs `extra >= 0`) and, unless a blast
    // detonates it, restores `extra = 0`.
    if is_crack && thing.hitag <= 0 {
        return true;
    }
    let temp = &mut actor_temps[sprite_index];
    if temp.hit_extra < 0 || temp.hit_picnum < 0 || thing.extra < 0 {
        if is_crack {
            thing.extra = 0;
            world.sprites[sprite_index] = thing;
        }
        return true;
    }
    let hit_picnum = temp.hit_picnum;
    temp.hit_extra = -1;
    let valid_crack_source = [
        FIREEXT,
        actor_tile(script, "RPG", RPG),
        actor_tile(script, "RADIUSEXPLOSION", RADIUSEXPLOSION),
        actor_tile(script, "SEENINE", SEENINE),
        actor_tile(script, "OOZFILTER", OOZFILTER),
    ]
    .contains(&hit_picnum);
    if is_crack && (!valid_crack_source || thing.hitag <= 0) {
        thing.extra = 0;
        world.sprites[sprite_index] = thing;
        return true;
    }

    let pipebomb_sound = i16::try_from(PIPEBOMB_EXPLODE).unwrap_or_default();
    let glass_sound = i16::try_from(GLASS_HEAVYBREAK).unwrap_or_default();
    if is_fire_ext {
        for _ in 0..16 {
            // Chocolate `EGS(…, SCRAPn+(TRAND&…), -8, …, TRAND&2047, (TRAND&…)+64, -(TRAND&…)…)`:
            // 32-bit gcc evaluates the arguments right to left, so zvel draws first.
            let zvel = (-(krand_step(randomseed) & 4095) - (i32::from(thing.zvel) >> 2)) as i16;
            let xvel = (64 + (krand_step(randomseed) & 63)) as i16;
            let angle = (krand_step(randomseed) & 2047) as i16;
            let tile = SCRAP3 + (krand_step(randomseed) & 3) as i16;
            let z = thing.z.wrapping_sub(krand_step(randomseed) % (48 << 8));
            let scrap = crate::shoot::egs(
                &mut world.lists,
                &mut world.sprites[..],
                picnum,
                actor_temps,
                script,
                thing.sectnum,
                thing.x,
                thing.y,
                z,
                tile,
                -8,
                48,
                48,
                angle,
                xvel,
                zvel,
                sprite_index as i16,
                STAT_MISC,
            );
            if scrap >= 0 {
                world.sprites[scrap as usize].pal = 2;
            }
        }
        // Chocolate `spawn(i, EXPLOSION2)` (its `cstat` draws `TRAND&4`).
        world.sprites[sprite_index] = thing;
        let _ = crate::actors::spawn_explosion2_from(
            &mut world.lists,
            &mut world.sprites[..],
            &world.sectors[..world.numsectors as usize],
            &world.walls[..world.numwalls as usize],
            picnum,
            actor_temps,
            script,
            randomseed,
            sprite_index,
        );
        actor_temps[sprite_index]
            .pending_actor_sounds
            .extend([pipebomb_sound, glass_sound]);
        if thing.hitag > 0 {
            arm_explosive_hitag(world, actor_temps, thing.hitag);
            // Chocolate `spawn(i, EXPLOSION2)` (its `cstat` draws `TRAND&4`).
            world.sprites[sprite_index] = thing;
            let _ = crate::actors::spawn_explosion2_from(
                &mut world.lists,
                &mut world.sprites[..],
                &world.sectors[..world.numsectors as usize],
                &world.walls[..world.numwalls as usize],
                picnum,
                actor_temps,
                script,
                randomseed,
                sprite_index,
            );
            let radius = script
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
            let power = i32::from(thing.extra.max(0));
            crate::weapons_move::apply_con_hitradius(
                &mut world.lists,
                &mut world.sprites[..],
                &mut world.sectors[..world.numsectors as usize],
                &mut world.walls[..world.numwalls as usize],
                picnum,
                actor_temps,
                players,
                randomseed,
                sprite_index as i16,
                radius,
                [
                    power >> 2,
                    power - (power >> 1),
                    power - (power >> 2),
                    power,
                ],
                script.map_or(&[], |image| image.actortype.as_slice()),
                script,
                tile_meta,
            );
            actor_temps[sprite_index]
                .pending_actor_sounds
                .push(pipebomb_sound);
            *earthquake_time = 16;
            thing.z = thing.z.wrapping_sub(32 << 8);
            world.sprites[sprite_index] = thing;
            spawn_random_scraps(
                world,
                picnum,
                actor_temps,
                script,
                randomseed,
                thing,
                sprite_index as i16,
                8,
            );
        } else {
            let radius = script
                .and_then(|image| {
                    let index = if image.con_version == crate::con::ConVersion::V15 {
                        14
                    } else {
                        13
                    };
                    image.gamestartup.get(index).copied()
                })
                .unwrap_or(2500)
                .max(1);
            crate::weapons_move::apply_con_hitradius(
                &mut world.lists,
                &mut world.sprites[..],
                &mut world.sectors[..world.numsectors as usize],
                &mut world.walls[..world.numwalls as usize],
                picnum,
                actor_temps,
                players,
                randomseed,
                sprite_index as i16,
                radius,
                [10, 15, 20, 25],
                script.map_or(&[], |image| image.actortype.as_slice()),
                script,
                tile_meta,
            );
        }
        world.sprites[sprite_index] = thing;
        mark_sprite_deleted(world, picnum, sprite_index);
        return true;
    }

    arm_explosive_hitag(world, actor_temps, thing.hitag);
    *earthquake_time = 16;
    thing.z = thing.z.wrapping_sub(32 << 8);
    world.sprites[sprite_index] = thing;
    spawn_random_scraps(
        world,
        picnum,
        actor_temps,
        script,
        randomseed,
        thing,
        sprite_index as i16,
        8,
    );
    mark_sprite_deleted(world, picnum, sprite_index);
    true
}

/// GAME.CON `spawn WATERBUBBLE` from a non-APLAYER owner (no Z lift).
pub(crate) fn spawn_maker_bubble(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    from: i16,
    script: Option<&crate::con::ScriptImage>,
) -> i16 {
    let fu = from as usize;
    if fu >= sprites.len() {
        return -1;
    }
    let sect = sprites[fu].sectnum;
    let ang = sprites[fu].ang;
    let (x, y, z) = (sprites[fu].x, sprites[fu].y, sprites[fu].z);
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
    if picnum.len() <= iu {
        picnum.resize(iu + 1, 0);
    }
    picnum[iu] = WATERBUBBLE;
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    if let Some(img) = script {
        let (extra, temps) = crate::con::init_actor_temps_from_script(img, WATERBUBBLE);
        sprites[iu].extra = extra;
        sprites[iu].hitag = crate::con::actor_header_hitag(img, WATERBUBBLE) as i16;
        actor_temps[iu] = temps;
    } else {
        actor_temps[iu] = ActorConTemps::default();
    }
    actor_temps[iu].hit_picnum = sprites[fu].picnum;
    crate::con::vm::inherit_hit_planes(actor_temps, iu, from);
    i
}

/// Chocolate `movestandables` — BOLT1 sparks + WATERBUBBLEMAKER.
pub fn movestandables(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum: &mut Vec<i16>,
    players: &[Player],
    randomseed: &mut i32,
    global_random: i32,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
    only_sprite: Option<i16>,
) {
    let Some(p) = players.first() else {
        return;
    };
    let mut i = only_sprite.unwrap_or(lists.headspritestat[STAT_STANDABLE as usize]);
    while i >= 0 {
        // One-sprite calls (Chocolate's per-sprite `movestandables` dispatch)
        // must not fall through to the rest of the stat list.
        let next = if only_sprite.is_some() {
            -1
        } else {
            lists.nextspritestat[i as usize]
        };
        let iu = i as usize;
        if iu >= sprites.len() {
            i = next;
            continue;
        }
        if sprites[iu].sectnum < 0 {
            let _ = lists.deletesprite(i);
            i = next;
            continue;
        }
        let pic = sprites[iu].picnum;
        if pic == WATERBUBBLEMAKER {
            // GAME.CON: ifpdistl 3084 / ifrnd 24 / spawn WATERBUBBLE; then move count++.
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            if bolt_player_dist(&sprites[iu], p) < 3084 && rnd(randomseed, 24) {
                let _ = spawn_maker_bubble(lists, sprites, picnum, actor_temps, i, script);
            }
            actor_temps[iu].g_t[0] += 1;
            i = next;
            continue;
        }
        if (crate::spawn::WATERFOUNTAIN..=crate::spawn::WATERFOUNTAIN + 3).contains(&pic) {
            // Chocolate: a used fountain sprays (frames +1/+2) for 20 tics,
            // then resets once the player is farther than 512.
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            let t0 = actor_temps[iu].g_t[0];
            if t0 > 0 {
                if t0 < 20 {
                    actor_temps[iu].g_t[0] = t0 + 1;
                    let s = &mut sprites[iu];
                    s.picnum += 1;
                    if s.picnum == crate::spawn::WATERFOUNTAIN + 3 {
                        s.picnum = crate::spawn::WATERFOUNTAIN + 1;
                    }
                } else if bolt_player_dist(&sprites[iu], p) > 512 {
                    actor_temps[iu].g_t[0] = 0;
                    sprites[iu].picnum = crate::spawn::WATERFOUNTAIN;
                } else {
                    actor_temps[iu].g_t[0] = 1;
                }
            }
            i = next;
            continue;
        }
        if !(BOLT1..=BOLT1 + 3).contains(&pic) {
            i = next;
            continue;
        }
        let player_dist = bolt_player_dist(&sprites[iu], p);
        if actor_temps.len() <= iu {
            actor_temps.resize_with(iu + 1, ActorConTemps::default);
        }
        if player_dist > 20480 {
            i = next;
            continue;
        }
        // Chocolate `CLEAR_THE_BOLT` loop body (one pass; re-entry via hide path).
        loop {
            let t2 = actor_temps[iu].g_t[2];
            if t2 != 0 {
                actor_temps[iu].g_t[2] = t2 - 1;
                break;
            }
            let s = &mut sprites[iu];
            if (s.xrepeat | s.yrepeat) == 0 {
                s.xrepeat = actor_temps[iu].g_t[0] as u8;
                s.yrepeat = actor_temps[iu].g_t[1] as u8;
            } else if {
                let log_active = room_build_engine::numeric::krand_log::is_recording();
                if log_active {
                    room_build_engine::numeric::krand_log::set_context(Some(format!(
                        "BOLT sprite={i} pic={pic} dist={player_dist} t2={t2}"
                    )));
                }
                let flicker = (krand_step(randomseed) & 8) == 0;
                if log_active {
                    room_build_engine::numeric::krand_log::set_context(None);
                }
                flicker
            } {
                actor_temps[iu].g_t[0] = i32::from(s.xrepeat);
                actor_temps[iu].g_t[1] = i32::from(s.yrepeat);
                actor_temps[iu].g_t[2] = global_random & 4;
                s.xrepeat = 0;
                s.yrepeat = 0;
                continue; // goto CLEAR_THE_BOLT
            }
            s.picnum = s.picnum.wrapping_add(1);
            let l = global_random & 7;
            s.xrepeat = (l + 8) as u8;
            if (l & 1) != 0 {
                s.cstat ^= 2;
            }
            // Chocolate: short-circuit still consumes TRAND when pic == BOLT1+1.
            if s.picnum == BOLT1 + 1 {
                let log_active = room_build_engine::numeric::krand_log::is_recording();
                if log_active {
                    room_build_engine::numeric::krand_log::set_context(Some(format!(
                        "BOLT sound sprite={i} pic={} dist={player_dist}",
                        s.picnum
                    )));
                }
                let _ = krand_step(randomseed); // TRAND&7 sound gate (HURTRAIL)
                if log_active {
                    room_build_engine::numeric::krand_log::set_context(None);
                }
            }
            if s.picnum == BOLT1 + 4 {
                s.picnum = BOLT1;
            }
            break;
        }
        i = next;
    }
}

/// Chocolate `movestandables` SEENINE/OOZFILTER state machine, including
/// hitag-linked effector activation and Atomic hitradius damage.
pub(crate) fn tick_detonables(
    world: &mut BuildWorld,
    players: &mut [Player],
    tile_meta: &[TileZMeta],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    earthquake_time: &mut u8,
    only_sprite: Option<i16>,
) {
    let mut index = only_sprite.unwrap_or(world.lists.headspritestat[STAT_STANDABLE as usize]);
    while index >= 0 {
        let sprite_index = index as usize;
        index = if only_sprite.is_some() {
            -1
        } else {
            world.lists.nextspritestat[sprite_index]
        };
        let Some(mut thing) = world.sprites.get(sprite_index).copied() else {
            continue;
        };
        let crack1_tile = actor_tile(script, "CRACK1", CRACK1);
        let fire_ext_tile = actor_tile(script, "FIREEXT", FIREEXT);
        if (crack1_tile..=crack1_tile.saturating_add(3)).contains(&thing.picnum)
            || thing.picnum == fire_ext_tile
        {
            let _ = tick_breakable_standable(
                world,
                sprite_index,
                players,
                tile_meta,
                picnum,
                actor_temps,
                script,
                randomseed,
                earthquake_time,
            );
            continue;
        }
        let seenine_tile = actor_tile(script, "SEENINE", SEENINE);
        let oozfilter_tile = actor_tile(script, "OOZFILTER", OOZFILTER);
        let seenine_dead_tile = actor_tile(script, "SEENINEDEAD", seenine_tile + 1);
        let seenine_dead2_tile = actor_tile(script, "SEENINEDEAD2", seenine_tile + 2);
        if thing.picnum != seenine_tile
            && thing.picnum != oozfilter_tile
            && thing.picnum != seenine_dead_tile
            && thing.picnum != seenine_dead2_tile
        {
            continue;
        }
        if actor_temps.len() <= sprite_index {
            actor_temps.resize_with(sprite_index + 1, ActorConTemps::default);
        }
        if thing.sectnum < 0 {
            let _ = world.lists.deletesprite(sprite_index as i16);
            continue;
        }

        if thing.shade != -32 && thing.shade != -33 {
            let hit_extra = actor_temps[sprite_index].hit_extra;
            let hit = if thing.xrepeat != 0
                && hit_extra >= 0
                && thing.extra >= 0
                && actor_temps[sprite_index].hit_picnum >= 0
            {
                thing.extra = thing.extra.wrapping_sub(hit_extra);
                actor_temps[sprite_index].hit_extra = -1;
                true
            } else {
                false
            };
            if hit && crate::shoot::hit_debug_enabled() {
                crate::shoot::push_hit_log(format!(
                    "hit: SEENINE/OOZFILTER #{sprite_index} took {hit_extra} damage -> detonating (hitag {})",
                    thing.hitag
                ));
            }
            if hit || thing.shade == -31 {
                if hit {
                    thing.lotag = 0;
                }
                actor_temps[sprite_index].g_t[3] = 1;
                let mut linked = world.lists.headspritestat[STAT_STANDABLE as usize];
                while linked >= 0 {
                    let linked_index = linked as usize;
                    linked = world.lists.nextspritestat[linked_index];
                    if linked_index != sprite_index
                        && world.sprites.get(linked_index).is_some_and(|other| {
                            other.hitag == thing.hitag
                                && (other.picnum == seenine_tile || other.picnum == oozfilter_tile)
                        })
                    {
                        world.sprites[linked_index].shade = -32;
                    }
                }
                thing.shade = -32;
            }
        } else if thing.shade == -32 {
            if thing.lotag > 0 {
                thing.lotag = thing.lotag.saturating_sub(3);
                if thing.lotag <= 0 {
                    thing.lotag = -99;
                }
            } else {
                thing.shade = -33;
            }
        } else {
            let mut detonate_now = thing.xrepeat == 0;
            if thing.xrepeat > 0 {
                actor_temps[sprite_index].g_t[2] = actor_temps[sprite_index].g_t[2].wrapping_add(1);
                if actor_temps[sprite_index].g_t[2] == 3 {
                    if thing.picnum == oozfilter_tile {
                        actor_temps[sprite_index].g_t[2] = 0;
                        detonate_now = true;
                    } else if thing.picnum != seenine_dead2_tile {
                        actor_temps[sprite_index].g_t[2] = 0;
                        thing.picnum = if thing.picnum == seenine_tile {
                            seenine_dead_tile
                        } else {
                            seenine_dead2_tile
                        };
                        if picnum.len() <= sprite_index {
                            picnum.resize(sprite_index + 1, 0);
                        }
                        picnum[sprite_index] = thing.picnum;
                        world.sprites[sprite_index] = thing;
                        continue;
                    } else {
                        detonate_now = true;
                    }
                }
                if !detonate_now {
                    world.sprites[sprite_index] = thing;
                    continue;
                }
            }

            *earthquake_time = 16;
            let linked_started = actor_temps[sprite_index].g_t[3] == 1;
            let mut effector = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
            while effector >= 0 {
                let effector_index = effector as usize;
                effector = world.lists.nextspritestat[effector_index];
                if world.sprites[effector_index].hitag != thing.hitag {
                    continue;
                }
                if actor_temps.len() <= effector_index {
                    actor_temps.resize_with(effector_index + 1, ActorConTemps::default);
                }
                match world.sprites[effector_index].lotag {
                    13 if actor_temps[effector_index].g_t[2] == 0 => {
                        actor_temps[effector_index].g_t[2] = 1;
                    }
                    8 => actor_temps[effector_index].g_t[4] = 1,
                    // Chocolate only arms a Kroz effector that has not already
                    // started; SE21 is deliberately re-armed on every blast.
                    18 if actor_temps[effector_index].g_t[0] == 0 => {
                        actor_temps[effector_index].g_t[0] = 1;
                    }
                    21 => actor_temps[effector_index].g_t[0] = 1,
                    _ => {}
                }
            }

            thing.z = thing.z.wrapping_sub(32 << 8);
            let explosive = (linked_started && thing.xrepeat != 0) || thing.lotag == -99;
            world.sprites[sprite_index] = thing;
            if explosive {
                let radius = script
                    .and_then(|image| {
                        let startup_index = if image.con_version == crate::con::ConVersion::V15 {
                            14
                        } else {
                            13
                        };
                        image.gamestartup.get(startup_index).copied()
                    })
                    .unwrap_or(2500)
                    .max(1);
                let power = i32::from(thing.extra);
                // Chocolate `spawn(i, EXPLOSION2)` (its `cstat` draws `TRAND&4`).
                world.sprites[sprite_index] = thing;
                let _ = crate::actors::spawn_explosion2_from(
                    &mut world.lists,
                    &mut world.sprites[..],
                    &world.sectors[..world.numsectors as usize],
                    &world.walls[..world.numwalls as usize],
                    picnum,
                    actor_temps,
                    script,
                    randomseed,
                    sprite_index,
                );
                crate::weapons_move::apply_con_hitradius(
                    &mut world.lists,
                    &mut world.sprites[..],
                    &mut world.sectors[..world.numsectors as usize],
                    &mut world.walls[..world.numwalls as usize],
                    picnum,
                    actor_temps,
                    players,
                    randomseed,
                    sprite_index as i16,
                    radius,
                    [
                        power >> 2,
                        power - (power >> 1),
                        power - (power >> 2),
                        power,
                    ],
                    script.map_or(&[], |image| image.actortype.as_slice()),
                    script,
                    tile_meta,
                );
                actor_temps[sprite_index]
                    .pending_actor_sounds
                    .push(PIPEBOMB_EXPLODE);
            }
            if thing.xrepeat != 0 {
                for _ in 0..8 {
                    // Chocolate `RANDOMSCRAP`: 32-bit gcc evaluates the EGS arguments right
                    // to left, so zvel draws first and x last.
                    let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
                    let xvel = (64 + (krand_step(randomseed) & 63)) as i16;
                    let angle = (krand_step(randomseed) & 2047) as i16;
                    let tile = 2390 + (krand_step(randomseed) & 15) as i16;
                    let z = thing
                        .z
                        .wrapping_sub(8 << 8)
                        .wrapping_sub(krand_step(randomseed) & 8191);
                    let y = thing.y.wrapping_add((krand_step(randomseed) & 255) - 128);
                    let x = thing.x.wrapping_add((krand_step(randomseed) & 255) - 128);
                    let _ = crate::shoot::egs(
                        &mut world.lists,
                        &mut world.sprites[..],
                        picnum,
                        actor_temps,
                        script,
                        thing.sectnum,
                        x,
                        y,
                        z,
                        tile,
                        -8,
                        48,
                        48,
                        angle,
                        xvel,
                        zvel,
                        sprite_index as i16,
                        crate::stat::STAT_MISC,
                    );
                }
            }
            let _ = world.lists.deletesprite(sprite_index as i16);
            if picnum.len() <= sprite_index {
                picnum.resize(sprite_index + 1, 0);
            }
            picnum[sprite_index] = -1;
            continue;
        }
        world.sprites[sprite_index] = thing;
    }
}

/// Tick Atomic's standable TRIPBOMB actor: its initial delay, attached laser
/// segments, proximity re-arm timer, weapon-hit fuse and radius detonation.
pub(crate) fn tick_tripbombs(
    world: &mut BuildWorld,
    players: &mut [Player],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
    tile_meta: &[TileZMeta],
    randomseed: &mut i32,
    only_sprite: Option<i16>,
) {
    let sec_walls: Vec<SectorWalls> = world
        .sectors
        .iter()
        .take(world.numsectors as usize)
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<WallXY> = world
        .walls
        .iter()
        .take(world.numwalls as usize)
        .map(|wall| WallXY {
            x: wall.x,
            y: wall.y,
            point2: wall.point2,
            nextsector: wall.nextsector,
            cstat: wall.cstat,
        })
        .collect();
    let sec_z: Vec<SectorZ> = world
        .sectors
        .iter()
        .take(world.numsectors as usize)
        .map(|sector| sector.z())
        .collect();
    let tripbomb_tile = script
        .and_then(|image| image.labels.get("TRIPBOMB"))
        .filter(|label| !label.is_ptr)
        .map_or(TRIPBOMB, |label| label.value as i16);
    let mut index = only_sprite.unwrap_or(world.lists.headspritestat[STAT_STANDABLE as usize]);
    while index >= 0 {
        let sprite_index = index as usize;
        index = if only_sprite.is_some() {
            -1
        } else {
            world.lists.nextspritestat[sprite_index]
        };
        if world
            .sprites
            .get(sprite_index)
            .is_none_or(|sprite| sprite.picnum != tripbomb_tile)
        {
            continue;
        }
        if actor_temps.len() <= sprite_index {
            actor_temps.resize_with(sprite_index + 1, ActorConTemps::default);
        }
        let bomb = world.sprites[sprite_index];
        if bomb.sectnum < 0 {
            let _ = world.lists.deletesprite(sprite_index as i16);
            continue;
        }

        if actor_temps[sprite_index].g_t[2] > 0 {
            actor_temps[sprite_index].g_t[2] -= 1;
            if actor_temps[sprite_index].g_t[2] == 8 {
                actor_temps[sprite_index].pending_actor_sounds.push(17); // LASERTRIP_EXPLODE.
                for _ in 0..5 {
                    spawn_tripbomb_scrap(
                        world,
                        picnum,
                        actor_temps,
                        script,
                        randomseed,
                        bomb,
                        sprite_index as i16,
                    );
                }
                let radius = script
                    .and_then(|image| {
                        let startup_index = if image.con_version == crate::con::ConVersion::V15 {
                            11
                        } else {
                            10
                        };
                        image.gamestartup.get(startup_index).copied()
                    })
                    .unwrap_or(2048)
                    .max(1);
                let power = i32::from(bomb.extra);
                world.sprites[sprite_index] = bomb;
                crate::weapons_move::apply_con_hitradius(
                    &mut world.lists,
                    &mut world.sprites[..],
                    &mut world.sectors[..world.numsectors as usize],
                    &mut world.walls[..world.numwalls as usize],
                    picnum,
                    actor_temps,
                    players,
                    randomseed,
                    sprite_index as i16,
                    radius,
                    [power >> 2, power >> 1, power - (power >> 2), power],
                    script.map_or(&[], |image| image.actortype.as_slice()),
                    script,
                    tile_meta,
                );
                // Chocolate `spawn(i, EXPLOSION2)` (its `cstat` draws `TRAND&4`).
                world.sprites[sprite_index] = bomb;
                let _explosion = crate::actors::spawn_explosion2_from(
                    &mut world.lists,
                    &mut world.sprites[..],
                    &world.sectors[..world.numsectors as usize],
                    &world.walls[..world.numwalls as usize],
                    picnum,
                    actor_temps,
                    script,
                    randomseed,
                    sprite_index,
                );
                if _explosion >= 0 {
                    world.sprites[_explosion as usize].xvel = 348;
                }
                // Chocolate immediately runs `ssp(j, CLIPMASK0)` on the
                // spawned EXPLOSION2 so the visual flash starts at the end of
                // the bomb's 348-unit blast vector this tic.
                ssp_tripbomb_explosion_once(
                    world, _explosion, &sec_walls, &wall_xy, &sec_z, tile_meta,
                );
                let mut line = world.lists.headspritestat[STAT_MISC as usize];
                while line >= 0 {
                    let line_index = line as usize;
                    line = world.lists.nextspritestat[line_index];
                    if world.sprites[line_index].picnum == LASERLINE
                        && world.sprites[line_index].hitag == bomb.hitag
                    {
                        world.sprites[line_index].xrepeat = 0;
                        world.sprites[line_index].yrepeat = 0;
                    }
                }
                let _ = world.lists.deletesprite(sprite_index as i16);
                if picnum.len() <= sprite_index {
                    picnum.resize(sprite_index + 1, 0);
                }
                picnum[sprite_index] = -1;
                continue;
            }
            world.sprites[sprite_index] = bomb;
            continue;
        }

        // Chocolate's IFHIT temporarily sets health to one, then runs the
        // ordinary actor damage path, then restores the tripbomb's original
        // health. The incoming damage is only an arming signal; it must not
        // reduce `extra`, which supplies the later blast strength.
        if actor_temps[sprite_index].hit_extra >= 0 && actor_temps[sprite_index].hit_picnum >= 0 {
            actor_temps[sprite_index].hit_extra = -1;
            actor_temps[sprite_index].g_t[2] = 16;
        }

        if actor_temps[sprite_index].g_t[0] < 32 {
            // Chocolate `findplayer` chooses the sole local player in single
            // player, but the nearest living seat in multiplayer; its range
            // metric uses previous-tic player coordinates and Manhattan XY.
            let multiplayer = players.len() > 1;
            let distance = players
                .iter()
                .filter(|player| !multiplayer || player.extra > 0)
                .filter(|player| player.sprite_i >= 0)
                .map(|player| {
                    klabs(player.oposx.wrapping_sub(bomb.x))
                        .wrapping_add(klabs(player.oposy.wrapping_sub(bomb.y)))
                        .wrapping_add(
                            klabs(player.oposz.wrapping_sub(bomb.z).wrapping_add(28 << 8)) >> 4,
                        )
                })
                .min()
                .unwrap_or(i32::MAX);
            if distance > 768 || actor_temps[sprite_index].g_t[0] > 16 {
                actor_temps[sprite_index].g_t[0] += 1;
            }
        }
        if actor_temps[sprite_index].g_t[0] == 32 {
            let (distance, hit_sprite, start_x, start_y, start_z, hit_x, hit_y, hit_sect) =
                tripbomb_laser_trace(
                    world,
                    &bomb,
                    sprite_index,
                    &sec_walls,
                    &wall_xy,
                    &sec_z,
                    tile_meta,
                );
            actor_temps[sprite_index].lastvx = distance;
            spawn_tripbomb_laser(
                world,
                picnum,
                actor_temps,
                script,
                bomb,
                sprite_index as i16,
                start_x,
                start_y,
                start_z,
                hit_x,
                hit_y,
                hit_sect,
                distance,
                tile_meta,
            );
            actor_temps[sprite_index].g_t[0] += 1;
            if hit_sprite >= 0 {
                actor_temps[sprite_index].g_t[2] = 13;
                actor_temps[sprite_index].pending_actor_sounds.push(16); // LASERTRIP_ARMING.
            }
        } else if actor_temps[sprite_index].g_t[0] == 33 {
            actor_temps[sprite_index].g_t[1] += 1;
            let (distance, _, _, _, _, _, _, _) = tripbomb_laser_trace(
                world,
                &bomb,
                sprite_index,
                &sec_walls,
                &wall_xy,
                &sec_z,
                tile_meta,
            );
            if actor_temps[sprite_index].lastvx != distance {
                actor_temps[sprite_index].g_t[2] = 13;
                actor_temps[sprite_index].pending_actor_sounds.push(16);
            }
        }
        world.sprites[sprite_index] = bomb;
    }
}

#[allow(clippy::too_many_arguments)]
fn tripbomb_laser_trace(
    world: &BuildWorld,
    bomb: &SpriteT,
    bomb_index: usize,
    sectors: &[SectorWalls],
    walls: &[WallXY],
    sector_z: &[SectorZ],
    tile_meta: &[TileZMeta],
) -> (i32, i16, i32, i32, i32, i32, i32, i16) {
    let angle = i32::from(bomb.ang);
    let (sin, cos) = room_build_engine::helpers::sincos_q14(angle);
    let x = bomb.x.wrapping_add(cos >> 9);
    let y = bomb.y.wrapping_add(sin >> 9);
    let z = bomb.z.wrapping_sub(3 << 8);
    // Chocolate temporarily moves the tripbomb and calls `setsprite` before
    // `hitasprite`; the one-unit muzzle offset can cross a sector boundary.
    let mut ray_sector = bomb.sectnum;
    let _ = room_build_engine::geom::updatesector(x, y, &mut ray_sector, sectors, walls);
    let sprites: Vec<SpriteHit> = world
        .sprites
        .iter()
        .enumerate()
        .map(|(index, sprite)| {
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
                cstat: if index == bomb_index { 0 } else { sprite.cstat },
                face_span: i32::from(meta.width) * i32::from(sprite.xrepeat),
                face_height: (i32::from(meta.height) * i32::from(sprite.yrepeat)) << 2,
            }
        })
        .collect();
    let trace = hitscan_full(
        x,
        y,
        z,
        ray_sector,
        cos,
        sin,
        0,
        (256 << 16) + 64,
        sectors,
        walls,
        &sprites,
        Some(sector_z),
    );
    let distance =
        crate::actors::find_distance_2d(trace.hitx.wrapping_sub(x), trace.hity.wrapping_sub(y));
    (
        if trace.hitsect < 0 { 0 } else { distance },
        trace.hitsprite,
        x,
        y,
        z,
        trace.hitx,
        trace.hity,
        trace.hitsect,
    )
}

/// Run Chocolate `ssp(i, CLIPMASK0)` once on the tripbomb's spawned blast flash.
fn ssp_tripbomb_explosion_once(
    world: &mut BuildWorld,
    sprite_i: i16,
    sectors: &[SectorWalls],
    walls: &[WallXY],
    sector_z: &[SectorZ],
    tile_meta: &[TileZMeta],
) {
    let Ok(index) = usize::try_from(sprite_i) else {
        return;
    };
    let Some(sprite) = world.sprites.get(index).copied() else {
        return;
    };
    if sprite.sectnum < 0 || sprite.xvel == 0 {
        return;
    }
    let sprite_clips: Vec<SpriteZClip> = world
        .sprites
        .iter()
        .map(|sprite| crate::player::sprite_z_clip(sprite, tile_meta))
        .collect();
    let (ceiling_z, floor_z) =
        room_build_engine::geom::getzsofslope(sprite.sectnum, sprite.x, sprite.y, sector_z, walls);
    let (sin, _) = room_build_engine::helpers::sincos_q14(i32::from(sprite.ang));
    let mut x = sprite.x;
    let mut y = sprite.y;
    let mut z = sprite.z;
    let mut sect = sprite.sectnum;
    let (sin_side, _) =
        room_build_engine::helpers::sincos_q14((i32::from(sprite.ang) + 512) & 2047);
    let _hit = movesprite_xy_at_z(
        &mut x,
        &mut y,
        &mut z,
        &mut sect,
        (i32::from(sprite.xvel) * sin_side) >> 14,
        (i32::from(sprite.xvel) * sin) >> 14,
        i32::from(sprite.zvel),
        0,
        i32::from(sprite.clipdist) << 2,
        ceiling_z,
        floor_z,
        sectors,
        walls,
        Some(sector_z),
        &sprite_clips,
        1,
        Some(&world.lists),
    );
    if sect >= 0 && sect != sprite.sectnum {
        let _ = world.lists.changespritesect(sprite_i, sect);
    }
    if let Some(live) = world.sprites.get_mut(index) {
        live.x = x;
        live.y = y;
        live.z = z;
        live.sectnum = sect;
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_tripbomb_laser(
    world: &mut BuildWorld,
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
    bomb: SpriteT,
    bomb_index: i16,
    start_x: i32,
    start_y: i32,
    start_z: i32,
    _hit_x: i32,
    _hit_y: i32,
    hit_sect: i16,
    distance: i32,
    _tile_meta: &[TileZMeta],
) {
    if distance <= 0 || hit_sect < 0 {
        return;
    }
    let Some(tile) = script
        .and_then(|script| script.labels.get("LASERLINE"))
        .filter(|label| !label.is_ptr)
        .map(|label| label.value as i16)
        .or(Some(LASERLINE))
    else {
        return;
    };
    let angle = i32::from(bomb.ang);
    let (sin, cos) = room_build_engine::helpers::sincos_q14(angle);
    let step_x = cos >> 4;
    let step_y = sin >> 4;
    let sectors: Vec<SectorWalls> = world
        .sectors
        .iter()
        .take(world.numsectors as usize)
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let walls: Vec<WallXY> = world
        .walls
        .iter()
        .take(world.numwalls as usize)
        .map(|wall| WallXY {
            x: wall.x,
            y: wall.y,
            point2: wall.point2,
            nextsector: wall.nextsector,
            cstat: wall.cstat,
        })
        .collect();
    let mut x = start_x;
    let mut y = start_y;
    let mut remaining = distance;
    let mut sector_hint = bomb.sectnum;
    let laser_mode = script
        .filter(|image| image.con_version == crate::con::ConVersion::V15)
        .and_then(|image| image.gamestartup.get(29).copied())
        .unwrap_or(0);
    for _ in 0..512 {
        if remaining <= 0 {
            break;
        }
        let line_sector =
            room_build_engine::geom::updatesector(x, y, &mut sector_hint, &sectors, &walls);
        if line_sector < 0 {
            break;
        }
        let line = crate::shoot::egs(
            &mut world.lists,
            &mut world.sprites[..],
            picnum,
            actor_temps,
            script,
            line_sector,
            x,
            y,
            start_z,
            tile,
            0,
            32,
            6,
            bomb.ang,
            0,
            0,
            bomb_index,
            STAT_MISC,
        );
        if let Ok(line_index) = usize::try_from(line)
            && let Some(line_sprite) = world.sprites.get_mut(line_index)
        {
            line_sprite.hitag = bomb.hitag;
            line_sprite.ang = bomb.ang.wrapping_add(512);
            match laser_mode {
                0 | 2 => line_sprite.cstat = 16,
                1 => line_sprite.cstat = 16 + 2,
                _ => {
                    line_sprite.xrepeat = 0;
                    line_sprite.yrepeat = 0;
                }
            }
            if actor_temps.len() <= line_index {
                actor_temps.resize_with(line_index + 1, ActorConTemps::default);
            }
            actor_temps[line_index].g_t[1] = start_z;
            if remaining < 1024 {
                if laser_mode <= 2 {
                    line_sprite.xrepeat = (remaining >> 5).clamp(0, 255) as u8;
                }
                break;
            }
        } else {
            break;
        }
        x = x.wrapping_add(step_x);
        y = y.wrapping_add(step_y);
        remaining -= 1024;
    }
}

fn spawn_tripbomb_scrap(
    world: &mut BuildWorld,
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
    randomseed: &mut i32,
    bomb: SpriteT,
    bomb_index: i16,
) {
    let rand = |seed: &mut i32| krand_step(seed);
    // Chocolate `RANDOMSCRAP`: 32-bit gcc evaluates the EGS arguments right
    // to left, so zvel draws first and x last.
    let zvel = (-512 - (rand(randomseed) & 2047)) as i16;
    let xvel = (64 + (rand(randomseed) & 63)) as i16;
    let angle = (rand(randomseed) & 2047) as i16;
    let tile = script
        .and_then(|script| script.labels.get("SCRAP6"))
        .filter(|label| !label.is_ptr)
        .map_or(2390, |label| label.value as i16)
        .wrapping_add((rand(randomseed) & 15) as i16);
    let z = bomb
        .z
        .wrapping_sub(8 << 8)
        .wrapping_sub(rand(randomseed) & 8191);
    let y = bomb.y.wrapping_add((rand(randomseed) & 255) - 128);
    let x = bomb.x.wrapping_add((rand(randomseed) & 255) - 128);
    let _ = crate::shoot::egs(
        &mut world.lists,
        &mut world.sprites[..],
        picnum,
        actor_temps,
        script,
        bomb.sectnum,
        x,
        y,
        z,
        tile,
        -8,
        48,
        48,
        angle,
        xvel,
        zvel,
        bomb_index,
        STAT_MISC,
    );
}
