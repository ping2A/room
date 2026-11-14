//! STAT_PLAYER movers — Chocolate `moveplayers` (D5: APLAYER water bubbles).
//!
//! Runs the GAME.CON `ifinwater` block (action swim machine + `ifrnd 4` spawns)
//! plus the currently implemented player weapon-hit path.

use room_build_engine::geom::{
    SectorWalls, SectorZ, WallXY, getzrange_with_sprites, pushmove, updatesector,
};
use room_build_engine::helpers::{getangle, sincos_q14};
use room_build_engine::numeric::klabs;
use room_build_engine::types::{SectorT, SpriteT};
use room_build_engine::world::SpriteLists;

use crate::con::{ActorConTemps, ScriptImage, tick_action_frames};
use crate::player::{DukeInput, Player};
use crate::spawn::WATERBUBBLE;
use crate::stat::{STAT_MISC, STAT_PLAYER};

/// Advance Chocolate's dead-player `processinput` branch without accepting
/// gameplay input. The corpse camera still follows Build water and sector
/// changes, runs `updatesector`/`pushmove`, and derives the floor-relative roll.
pub(crate) fn tick_dead_player_view(
    player: &mut Player,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[room_build_engine::types::WallT],
    tile_meta: &[crate::player::TileZMeta],
) {
    if player.extra > 0 || player.freezer_frozen {
        return;
    }
    let sector_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let sector_z: Vec<SectorZ> = sectors
        .iter()
        .map(|sector| SectorZ {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
            ceilingz: sector.ceilingz,
            floorz: sector.floorz,
            ceilingstat: sector.ceilingstat,
            floorstat: sector.floorstat,
            ceilingheinum: sector.ceilingheinum,
            floorheinum: sector.floorheinum,
        })
        .collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(|wall| wall.xy()).collect();
    let sprite_clips: Vec<_> = sprites
        .iter()
        .map(|sprite| crate::player::sprite_z_clip(sprite, tile_meta))
        .collect();
    let range = getzrange_with_sprites(
        player.posx,
        player.posy,
        player.posz,
        player.cursectnum,
        128,
        crate::player::CLIPMASK0,
        &sector_z,
        &wall_xy,
        &sprite_clips,
    );
    let floor_z = range.florz;
    let ceiling_z = range.ceilz;
    let body_index = usize::try_from(player.sprite_i).ok();
    let body_palette = body_index
        .and_then(|index| sprites.get(index))
        .map_or(0, |body| body.pal);
    if sectors
        .get(usize::try_from(player.cursectnum).unwrap_or(usize::MAX))
        .is_some_and(|sector| sector.lotag == 2)
    {
        if player.on_warping_sector == 0 {
            if klabs(player.posz.wrapping_sub(floor_z)) > (crate::player::PHEIGHT >> 1) {
                player.posz = player.posz.wrapping_add(348);
            }
        } else if let Some(body) = body_index.and_then(|index| sprites.get_mut(index)) {
            body.z = body.z.wrapping_sub(512);
            body.zvel = -348;
        }
    }
    player.oposx = player.posx;
    player.oposy = player.posy;
    player.oposz = player.posz;
    player.horiz = 100;
    player.horizoff = 0;
    let _ = updatesector(
        player.posx,
        player.posy,
        &mut player.cursectnum,
        &sector_walls,
        &wall_xy,
    );
    let _ = pushmove(
        &mut player.posx,
        &mut player.posy,
        player.posz,
        &mut player.cursectnum,
        128,
        4 << 8,
        20 << 8,
        crate::player::CLIPMASK0,
        &sector_walls,
        &sector_z,
        &wall_xy,
    );
    if floor_z > ceiling_z.wrapping_add(16 << 8) && body_palette != 1 {
        player.rotscrnang = (i32::from(player.dead_flag)
            .wrapping_add((floor_z.wrapping_add(player.posz)) >> 7))
        .rem_euclid(2048) as i16;
    }
    player.on_warping_sector = 0;
}

/// Chocolate `spawn(player, WATERBUBBLE)` — APLAYER owner lifts Z by `16<<8`.
fn spawn_aplayer_water_bubble(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    from: i16,
    script: Option<&ScriptImage>,
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
    if sect >= 0 && (sect as usize) < sectors.len() {
        actor_temps[iu].warp_floorz = Some(sectors[sect as usize].floorz);
        actor_temps[iu].warp_ceilingz = Some(sectors[sect as usize].ceilingz);
    }
    // Chocolate EGS copies the APLAYER owner's getzrange planes.
    crate::con::vm::inherit_hit_planes(actor_temps, iu, from);
    i
}

/// Chocolate `rnd(X)`.
fn rnd(randomseed: &mut i32, x: i32, context: &str) -> bool {
    let recording = room_build_engine::numeric::krand_log::is_recording();
    if recording {
        room_build_engine::numeric::krand_log::set_context(Some(context.to_string()));
    }
    let t = room_build_engine::numeric::krand_step(randomseed);
    if recording {
        room_build_engine::numeric::krand_log::set_context(None);
    }
    (t >> 8) >= (255 - x)
}

fn label(script: &ScriptImage, name: &str) -> i32 {
    script.labels.get(name).map(|l| l.value).unwrap_or(0)
}

fn set_action(temps: &mut ActorConTemps, a: i32) {
    temps.g_t[2] = 0;
    temps.g_t[3] = 0;
    temps.g_t[4] = if a == 2048 { 0 } else { a };
}

/// Chocolate `ifp` bits used by the APLAYER water block.
fn ifp_flags(xvel: i16, bits: u32) -> (bool, bool, bool, bool, bool) {
    let s = i32::from(xvel);
    let run = (bits & (1 << 5)) != 0;
    let pstanding = (0..8).contains(&s);
    let pwalking = s >= 8 && !run;
    let prunning = s >= 8 && run;
    let pwalkingback = s <= -8 && !run;
    let prunningback = s <= -8 && run;
    (pstanding, pwalking, prunning, pwalkingback, prunningback)
}

/// Chocolate `moveplayers` subset: APLAYER `ifinwater` swim bubbles.
pub fn moveplayers(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[room_build_engine::types::WallT],
    players: &mut [Player],
    sync: &[DukeInput],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    script: Option<&ScriptImage>,
    emit_tic: u32,
) {
    apply_player_weapon_hits(
        lists,
        sprites,
        players,
        picnum,
        actor_temps,
        randomseed,
        script,
    );
    // Chocolate's second APLAYER in STAT_PLAYER is the Holoduke decoy
    // (`owner=-1`), while the live player body has a nonnegative owner.
    if players.is_empty() {
        return;
    }
    let mut hologram = lists.headspritestat[STAT_PLAYER as usize];
    while hologram >= 0 {
        let next = lists.nextspritestat[hologram as usize];
        let iu = hologram as usize;
        if iu < sprites.len() && sprites[iu].owner < 0 {
            let owner = players.iter().find(|player| player.holoduke_on == hologram);
            let Some(player) = owner else {
                let _ = lists.deletesprite(hologram);
                if iu < picnum.len() {
                    picnum[iu] = 0;
                }
                hologram = next;
                continue;
            };
            let sprite = &mut sprites[iu];
            sprite.cstat = 0;
            if sprite.xrepeat < 42 {
                sprite.xrepeat = sprite.xrepeat.saturating_add(4).min(42);
                sprite.cstat |= 2;
            }
            if sprite.yrepeat < 36 {
                sprite.yrepeat = sprite.yrepeat.saturating_add(4).min(36);
            }
            if sprite.extra < 8 {
                sprite.xvel = 128;
                sprite.ang = player.ang;
                sprite.extra += 1;
            } else {
                sprite.ang = 2047 - player.ang;
            }
            if sprite.sectnum >= 0
                && let Some(sector) = sectors.get(sprite.sectnum as usize)
            {
                if sector.ceilingshade < sprite.shade {
                    sprite.shade += (sector.ceilingshade - sprite.shade) >> 1;
                }
                if sector.floorshade < sprite.shade {
                    sprite.shade += (sector.floorshade - sprite.shade) >> 1;
                }
            }
        }
        hologram = next;
    }
    let Some(img) = script else {
        return;
    };
    let ptred = label(img, "PTREDWATER");
    let pswim = label(img, "PSWIMMING");
    let pswim_go = label(img, "PSWIMMINGGO");
    let pswim_wait = label(img, "PSWIMMINGWAIT");
    let sector_z: Vec<SectorZ> = sectors
        .iter()
        .map(|sector| SectorZ {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
            ceilingz: sector.ceilingz,
            floorz: sector.floorz,
            ceilingstat: sector.ceilingstat,
            floorstat: sector.floorstat,
            ceilingheinum: sector.ceilingheinum,
            floorheinum: sector.floorheinum,
        })
        .collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(|wall| wall.xy()).collect();
    let mut i = lists.headspritestat[STAT_PLAYER as usize];
    while i >= 0 {
        let next = lists.nextspritestat[i as usize];
        let iu = i as usize;
        if iu >= sprites.len() {
            i = next;
            continue;
        }
        if sprites[iu].owner < 0 {
            i = next;
            continue;
        }
        let Some(seat) = players.iter().position(|player| player.sprite_i == i) else {
            i = next;
            continue;
        };
        let p = &mut players[seat];
        let bits = sync.get(seat).map_or(0, |input| input.bits);
        if actor_temps.len() <= iu {
            actor_temps.resize_with(iu + 1, ActorConTemps::default);
        }
        let sect = sprites[iu].sectnum;
        if sect < 0 || sect as usize >= sectors.len() {
            i = next;
            continue;
        }
        // Chocolate `execute` always calls `move()` after a non-killit parse,
        // and `move()` advances APLAYER's temp_data[0] even outside water.
        actor_temps[iu].g_t[0] = actor_temps[iu].g_t[0].wrapping_add(1);
        let spawn_shrink_afterimage = tick_player_shrink_state(
            p,
            &mut sprites[iu],
            &mut actor_temps[iu],
            &sector_z,
            &wall_xy,
            img,
        );
        if spawn_shrink_afterimage {
            let body = sprites[iu];
            let _ = crate::shoot::egs(
                lists,
                sprites,
                picnum,
                actor_temps,
                Some(img),
                body.sectnum,
                body.x,
                body.y,
                body.z,
                4095, // FRAMEEFFECT1
                body.shade,
                body.xrepeat,
                body.yrepeat,
                body.ang,
                0,
                0,
                i,
                crate::stat::STAT_MISC,
            );
        }
        // Chocolate: `s->ang = p->ang` runs *after* `execute` (and its spawns).

        if ptred == 0 && pswim == 0 {
            sprites[iu].ang = p.ang;
            i = next;
            continue;
        }
        let in_water = sectors[sect as usize].lotag == 2;
        if !in_water {
            sprites[iu].ang = p.ang;
            i = next;
            continue;
        }

        // Chocolate `execute` ticks action frames before parse.
        tick_action_frames(&img.script, &mut actor_temps[iu], &mut sprites[iu].lotag);

        let (pstanding, pwalking, prunning, pwalkingback, prunningback) =
            ifp_flags(sprites[iu].xvel, bits);
        let action = actor_temps[iu].g_t[4];

        // Keep this action emulation synchronized with the CON blocks below.
        if action == ptred {
            if pwalking || prunning {
                set_action(&mut actor_temps[iu], pswim_go);
            }
        } else if pstanding || pwalkingback || prunningback {
            set_action(&mut actor_temps[iu], ptred);
        } else if action == pswim {
            if rnd(randomseed, 4, "APLAYER PSWIMMING ifrnd") {
                let _ = spawn_aplayer_water_bubble(
                    lists,
                    sprites,
                    sectors,
                    picnum,
                    actor_temps,
                    i,
                    Some(img),
                );
            }
            if actor_temps[iu].g_t[2] >= 4 {
                set_action(&mut actor_temps[iu], pswim_wait);
            }
        } else if action == pswim_wait {
            if actor_temps[iu].g_t[2] >= 2 {
                set_action(&mut actor_temps[iu], pswim_go);
            }
        } else if action == pswim_go {
            if actor_temps[iu].g_t[2] >= 2 {
                set_action(&mut actor_temps[iu], pswim);
            }
        } else {
            set_action(&mut actor_temps[iu], ptred);
        }

        // GAME.CON: `ifrnd 4 spawn WATERBUBBLE` closes the `ifinwater` block
        // and runs every tic in water (`action` does not stop the parse).
        if rnd(randomseed, 4, "APLAYER trailing WATERBUBBLE ifrnd") {
            let _ = spawn_aplayer_water_bubble(
                lists,
                sprites,
                sectors,
                picnum,
                actor_temps,
                i,
                Some(img),
            );
        }
        // Chocolate `moveplayers`: after execute, `s->ang = p->ang`.
        sprites[iu].ang = p.ang;
        i = next;
    }
}

/// Atomic APLAYER's `ifmove PSHRINKING` state is part of its CON actor, which
/// advances in the player pass instead of the ordinary STAT_ACTOR pass.
fn tick_player_shrink_state(
    player: &mut Player,
    body: &mut SpriteT,
    temps: &mut ActorConTemps,
    sectors: &[SectorZ],
    walls: &[WallXY],
    script: &ScriptImage,
) -> bool {
    if player.extra <= 0 {
        return false;
    }
    let shrinking_move = script
        .labels
        .get("PSHRINKING")
        .map_or(0, |label| label.value);
    if shrinking_move == 0 || temps.g_t[1] != shrinking_move {
        return false;
    }
    let shrink_count = script
        .labels
        .get("SHRUNKCOUNT")
        .map_or(270, |label| label.value)
        .max(0);
    let shrink_done_count = script
        .labels
        .get("SHRUNKDONECOUNT")
        .map_or(304, |label| label.value)
        .max(shrink_count);

    if temps.g_t[0] < 32 {
        if player.steroids_amount > 0 {
            temps.g_t[0] = shrink_count;
        } else {
            body.xrepeat = approach_repeat(body.xrepeat, 8);
            body.yrepeat = approach_repeat(body.yrepeat, 9);
            return true;
        }
        return false;
    }
    if temps.g_t[0] >= shrink_done_count {
        temps.g_t[1] = 0;
        body.cstat = 257;
        return false;
    }
    if temps.g_t[0] < shrink_count {
        if player.steroids_amount > 0 {
            temps.g_t[0] = shrink_count;
        }
        return false;
    }

    body.xrepeat = approach_repeat(body.xrepeat, 42);
    body.yrepeat = approach_repeat(body.yrepeat, 36);
    let (ceiling_z, floor_z) =
        room_build_engine::geom::getzsofslope(body.sectnum, body.x, body.y, sectors, walls);
    let gap = floor_z.wrapping_sub(ceiling_z) >> 8;
    if gap < 24 {
        player.extra = 0;
        player.last_extra = 0;
        body.extra = 0;
        player.palette_flash = [64, 0, 0];
        player.palette_flash_tics = 48;
        if let Some(sound) = script
            .labels
            .get("SQUISHED")
            .and_then(|label| i16::try_from(label.value).ok())
        {
            temps.pending_actor_sounds.push(sound);
        }
    }
    false
}

/// Chocolate `sizeto` approaches each repeat by one unit per actor tic.
fn approach_repeat(current: u8, target: u8) -> u8 {
    match current.cmp(&target) {
        std::cmp::Ordering::Less => current + 1,
        std::cmp::Ordering::Greater => current - 1,
        std::cmp::Ordering::Equal => current,
    }
}

/// Chocolate `ifhitbyweapon`'s APLAYER path. Damage metadata is queued by
/// projectile/hitscan handling and consumed in `moveplayers` after weapons.
fn apply_player_weapon_hits(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    players: &mut [Player],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    script: Option<&ScriptImage>,
) {
    const APLAYER: i16 = 1405;
    const SHRINKSPARK: i16 = 1646;
    const FREEZEBLAST: i16 = 1641;
    const STRONG_KNOCKBACK: &[i16] = &[1670, 2605, 981, 26, 1247, 1079, 1238];

    let player_count = players.len();
    for player in players {
        let Ok(sprite_index) = usize::try_from(player.sprite_i) else {
            continue;
        };
        let Some(body) = sprites.get(sprite_index).copied() else {
            continue;
        };
        let Some(temps) = actor_temps.get(sprite_index) else {
            continue;
        };
        if body.picnum != APLAYER || temps.hit_extra < 0 || body.extra < 0 {
            continue;
        }
        let hit_picnum = temps.hit_picnum;
        let hit_extra = temps.hit_extra;
        let hit_owner = temps.hit_owner;
        let hit_ang = temps.hit_ang;
        let body_palette = body.pal;
        if player.god_mode && hit_picnum != SHRINKSPARK {
            continue;
        }
        let owner_is_player = usize::try_from(hit_owner)
            .ok()
            .and_then(|index| sprites.get(index))
            .is_some_and(|owner| owner.picnum == APLAYER);
        // Atomic defaults to friendly fire off in coop. Keep the pending hit
        // intact exactly as Chocolate does when this guard rejects damage.
        if player.coop_mode && !player.friendly_fire && owner_is_player {
            continue;
        }
        if hit_picnum == SHRINKSPARK {
            let shrink_move = script
                .and_then(|image| image.labels.get("PSHRINKING"))
                .map_or(0, |label| label.value);
            if shrink_move != 0 {
                actor_temps[sprite_index].g_t[0] = 0;
                actor_temps[sprite_index].g_t[1] = shrink_move;
                sprites[sprite_index].cstat = 0;
                player.palette_flash = [48, 0, 48];
                player.palette_flash_tics = 48;
                if let Some(sound) = script
                    .and_then(|image| image.labels.get("ACTOR_SHRINKING"))
                    .and_then(|label| i16::try_from(label.value).ok())
                {
                    actor_temps[sprite_index].pending_actor_sounds.push(sound);
                }
            }
        } else if hit_picnum == crate::shoot::GROWSPARK {
            player.palette_flash = [52, 0, 0];
            player.palette_flash_tics = 48;
            if let Some(sound) = script
                .and_then(|image| image.labels.get("EXPANDERHIT"))
                .and_then(|label| i16::try_from(label.value).ok())
            {
                actor_temps[sprite_index].pending_actor_sounds.push(sound);
            }
        }
        if player.freezer_frozen && hit_picnum != FREEZEBLAST {
            player.freezer_frozen = false;
            player.freezer_tics = 0;
            sprites[sprite_index].pal = player.freezer_palette;
            let glass_pieces = script
                .and_then(|image| image.labels.get("GLASSPIECES"))
                .filter(|label| !label.is_ptr)
                .map_or(1031, |label| label.value as i16);
            let glass_breaking = script
                .and_then(|image| image.labels.get("GLASS_BREAKING"))
                .filter(|label| !label.is_ptr)
                .and_then(|label| i16::try_from(label.value).ok())
                .unwrap_or(19);
            for shard_index in (0..=16).rev() {
                let shard_tile = glass_pieces + (shard_index % 3);
                // Chocolate `EGS(…, TRAND&2047, 32+(TRAND&63), 1024-(TRAND&1023), …)`:
                // right-to-left arguments, zvel draws first.
                let zvel =
                    (1024 - (room_build_engine::numeric::krand_step(randomseed) & 1023)) as i16;
                let xvel = (32 + (room_build_engine::numeric::krand_step(randomseed) & 63)) as i16;
                let angle = (room_build_engine::numeric::krand_step(randomseed) & 2047) as i16;
                let shard = crate::shoot::egs(
                    lists,
                    sprites,
                    picnum,
                    actor_temps,
                    script,
                    body.sectnum,
                    body.x,
                    body.y,
                    body.z,
                    shard_tile,
                    -32,
                    36,
                    36,
                    angle,
                    xvel,
                    zvel,
                    player.sprite_i,
                    STAT_MISC,
                );
                if shard >= 0 {
                    sprites[shard as usize].pal = 1;
                }
            }
            if let Some(temps) = actor_temps.get_mut(sprite_index) {
                temps.pending_actor_sounds.push(glass_breaking);
            }
        }
        let health = body.extra.wrapping_sub(hit_extra);
        let health = if health <= 0 && hit_owner >= 0 && hit_picnum != FREEZEBLAST {
            0
        } else {
            health
        };
        // Chocolate `processinput` calls `incur_damage` after actor damage:
        // armor absorbs a random 20–49% of lost health, capped by the current
        // shield amount. The one TRAND is consumed only when damage is taken
        // while armor is present.
        let health = if health > 0 {
            let mut damage = health.wrapping_sub(player.last_extra);
            if damage < 0 && player.shield_amount > 0 {
                let shield_percent = 20 + room_build_engine::numeric::krand_step(randomseed) % 30;
                let shield_damage = (i32::from(damage).wrapping_mul(shield_percent) / 100) as i16;
                damage = damage.wrapping_sub(shield_damage);
                player.shield_amount = player.shield_amount.wrapping_add(shield_damage);
                if player.shield_amount < 0 {
                    damage = damage.wrapping_add(player.shield_amount);
                    player.shield_amount = 0;
                }
            }
            player.last_extra.wrapping_add(damage)
        } else {
            player.shield_amount = 0;
            0
        };
        if health == 0 && hit_picnum != FREEZEBLAST {
            if hit_owner >= 0 {
                // Chocolate `ifhitbyweapon`: preserve the lethal actor so
                // `moveactors` can turn the dead player's view toward it.
                player.wacked_by_actor = hit_owner;
            }
            if let Some(owner_seat) = usize::try_from(hit_owner)
                .ok()
                .and_then(|owner_index| sprites.get(owner_index))
                .filter(|owner| owner.picnum == APLAYER)
                .map(|owner| owner.yvel)
                .filter(|&seat| {
                    seat >= 0 && usize::try_from(seat).unwrap_or(usize::MAX) < player_count
                })
            {
                player.frag_ps = owner_seat;
            }
        }
        sprites[sprite_index].extra = health;
        player.extra = health;
        player.last_extra = health;
        actor_temps[sprite_index].hit_extra = -1;

        if health == 0 && hit_picnum == FREEZEBLAST {
            let thaw_tics = script
                .and_then(|image| image.labels.get("THAWTIME"))
                .map_or(138, |label| {
                    label.value.max(1).min(i32::from(u16::MAX)) as u16
                });
            player.freezer_frozen = true;
            player.freezer_tics = thaw_tics.saturating_add(1);
            player.freezer_palette = body_palette;
            sprites[sprite_index].pal = 1;
            player.dead_flag = 0;
            player.death_processed = false;
            player.palette_flash = [48, 0, 0];
            player.palette_flash_tics = 48;
            if let Some(sound) = script
                .and_then(|image| image.labels.get("SOMETHINGFROZE"))
                .and_then(|label| i16::try_from(label.value).ok())
            {
                actor_temps[sprite_index].pending_actor_sounds.push(sound);
            }
        }

        let (sin, cos) = sincos_q14(i32::from(hit_ang) & 2047);
        let shift = if STRONG_KNOCKBACK.contains(&hit_picnum) {
            2
        } else {
            1
        };
        player.posxv = player
            .posxv
            .wrapping_add(i32::from(hit_extra).wrapping_mul(cos).wrapping_shl(shift));
        player.posyv = player
            .posyv
            .wrapping_add(i32::from(hit_extra).wrapping_mul(sin).wrapping_shl(shift));
    }
}

/// Advance Chocolate's one-time `processinput` transition after player health
/// reaches zero. Death tilt and frag attribution happen on the next tic, after
/// `global_random` and before the dead player returns early from movement.
pub(crate) fn process_player_deaths(
    players: &mut [Player],
    sprites: &mut [SpriteT],
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    frag_matrix: &mut [i16],
    script: Option<&ScriptImage>,
) {
    let mut scored = Vec::new();
    let player_count = players.len();
    for (seat, player) in players.iter_mut().enumerate() {
        if player.freezer_frozen {
            if player.freezer_tics > 1 {
                player.freezer_tics -= 1;
                continue;
            }
            player.freezer_frozen = false;
            player.freezer_tics = 0;
            player.extra = 1;
            player.last_extra = 1;
            player.dead_flag = 0;
            player.death_processed = false;
            if let Ok(body_index) = usize::try_from(player.sprite_i)
                && let Some(body) = sprites.get_mut(body_index)
            {
                body.pal = player.freezer_palette;
                body.extra = 1;
            }
            continue;
        }
        if player.extra > 0 {
            continue;
        }
        if let Some((camera_x, camera_y, camera_z, camera_ang, camera_sector)) =
            player.remote_camera.take()
            && let Some(camera) = sprites.iter_mut().find(|sprite| {
                sprite.picnum == crate::spawn::CAMERA1
                    && sprite.x == camera_x
                    && sprite.y == camera_y
                    && sprite.z == camera_z
                    && sprite.ang == camera_ang
                    && sprite.sectnum == camera_sector
            })
        {
            // Chocolate APLAYER sets `newowner = -1` on death. Release the
            // camera actor's active-monitor latch at the same transition.
            camera.yvel = 0;
        }
        let Some(body_index) = usize::try_from(player.sprite_i)
            .ok()
            .filter(|&index| index < sprites.len())
        else {
            if !player.death_processed {
                player.death_processed = true;
            }
            continue;
        };
        let body = sprites[body_index];
        // Chocolate `actors.c` keeps the dead player camera at the body and
        // turns it halfway toward the last lethal actor on every actor tic.
        player.posx = body.x;
        player.posy = body.y;
        player.posz = body.z.wrapping_sub(20 << 8);
        if let Some(attacker) = usize::try_from(player.wacked_by_actor)
            .ok()
            .and_then(|index| sprites.get(index))
            .filter(|sprite| sprite.statnum < room_build_engine::world::MAXSTATUS as i16)
        {
            let target_ang = getangle(
                attacker.x.wrapping_sub(player.posx),
                attacker.y.wrapping_sub(player.posy),
            );
            // Keep Chocolate `getincangle`'s strict `< 1024` branch: an
            // exactly-opposite target turns in the positive direction.
            let mut from = i32::from(player.ang) & 2047;
            let mut to = target_ang & 2047;
            let delta = if (from - to).abs() < 1024 {
                to - from
            } else {
                if from > 1024 {
                    from -= 2048;
                }
                if to > 1024 {
                    to -= 2048;
                }
                to - from
            };
            player.ang = (i32::from(player.ang) + (delta >> 1)).rem_euclid(2048) as i16;
        }
        if player.death_processed {
            continue;
        }
        let should_score = player_count > 1 && (body.pal != 1 || body.cstat & 32768 != 0);
        if body.pal != 1 {
            player.palette_flash = [63, 0, 0];
            player.palette_flash_tics = 63;
            player.posz = player.posz.wrapping_sub(16 << 8);
            sprites[body_index].z = body.z.wrapping_sub(16 << 8);
            let side = room_build_engine::numeric::krand_step(randomseed) & 1;
            let angle = room_build_engine::numeric::krand_step(randomseed) & 255;
            player.dead_flag = (angle - (side << 10)).rem_euclid(2048) as u16;
            if body.cstat & 32768 == 0 {
                sprites[body_index].cstat = 0;
            }
        }
        player.jetpack_on = 0;
        player.holoduke_on = -1;
        if let Some(sound) = script
            .and_then(|image| image.labels.get("DUKE_JETPACK_IDLE"))
            .and_then(|label| i16::try_from(label.value).ok())
            && let Some(temps) = actor_temps.get_mut(body_index)
        {
            // Chocolate stops the looping jetpack voice on the one-time
            // death edge, even when the inventory timer did not expire.
            temps.pending_sound_stops.push(sound);
        }
        player.death_processed = true;
        if should_score {
            scored.push((seat, player.frag_ps));
        }
    }
    for (victim, killer) in scored {
        let killer_seat = usize::try_from(killer)
            .ok()
            .filter(|&seat| seat < players.len());
        if killer_seat == Some(victim) {
            players[victim].fragged_self = players[victim].fragged_self.saturating_add(1);
        } else if let Some(killer) = killer_seat {
            players[killer].frag_count = players[killer].frag_count.saturating_add(1);
            let matrix_index = killer.saturating_mul(8).saturating_add(victim);
            if let Some(value) = frag_matrix.get_mut(matrix_index) {
                *value = value.saturating_add(1);
            }
        }
        players[victim].frag_ps = i16::try_from(victim).unwrap_or(i16::MAX);
    }
}
