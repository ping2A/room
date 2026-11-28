//! Chocolate `moveweapons` (STAT_PROJECTILE) — SHOTSPARK1 CON + COOLEXPLOSION1 fly (D6).

use room_build_engine::geom::{
    SectorWalls, SectorZ, SpriteHit, WallXY, cansee, getzsofslope, hitscan_full,
};
use room_build_engine::helpers::{getangle, sincos_q14};
use room_build_engine::numeric::{divscale, dmulscale, klabs, krand_step, ksqrt, mulscale};
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

use crate::con::{ActorConTemps, ConSpriteEnv, ConStop, ScriptImage, execute_actor};
use crate::fta::findplayer_dist;
use crate::player::{Player, TileZMeta};
use crate::shoot::{
    COOLEXPLOSION1, FIRELASER, FREEZEBLAST, GROWSPARK, RPG_PROJECTILE, SHOTSPARK1, SHRINKSPARK,
    SPIT, egs,
};
use crate::spawn::WATERBUBBLE;
use crate::standables::spawn_maker_bubble;
use crate::stat::{STAT_ACTOR, STAT_DEFAULT, STAT_DUMMY, STAT_FALLER, STAT_MISC, STAT_PROJECTILE};

fn script_tile(script: Option<&ScriptImage>, name: &str) -> Option<i16> {
    script
        .and_then(|image| image.labels.get(name))
        .filter(|label| !label.is_ptr)
        .map(|label| label.value as i16)
}

fn is_flammable_radius_target(script: Option<&ScriptImage>, picnum: i16) -> bool {
    [
        ("BOX", 951),
        ("TREE1", 908),
        ("TREE2", 910),
        ("TIRE", 990),
        ("CONE", 978),
    ]
    .iter()
    .any(|(name, fallback)| script_tile(script, name).unwrap_or(*fallback) == picnum)
}

/// Atomic `hitradius(i, r, x>>2, x>>1, x-(x>>2), x)` as EDuke32's
/// `ud.returnvar` payload `{radius, dmg1..dmg4}`.
fn rpg_blast_payload(radius: i32, strength: i32) -> [i32; 5] {
    [
        radius,
        strength >> 2,
        strength >> 1,
        strength - (strength >> 2),
        strength,
    ]
}

fn queue_radius_sprite_hit(
    sprites: &mut [SpriteT],
    actor_temps: &mut Vec<ActorConTemps>,
    target_i: usize,
    target: SpriteT,
    amount: i16,
    hit_picnum: i16,
    owner: i16,
    hit_ang: i16,
    knockback: i32,
    source_i: i16,
    payload: [i32; 5],
) {
    if actor_temps.len() <= target_i {
        actor_temps.resize_with(target_i + 1, ActorConTemps::default);
    }
    // Chocolate `hitradius` / EDuke32 `A_RadiusDamage` replace the queued
    // damage; a source projectile with PROJECTILE_HITRADIUS_ADDITIVE adds.
    let additive = usize::try_from(source_i)
        .ok()
        .and_then(|index| actor_temps.get(index))
        .is_some_and(|source| {
            source.projectile_workslike & crate::shoot::PROJECTILE_FLAG_HITRADIUS_ADDITIVE != 0
        });
    let temp = &mut actor_temps[target_i];
    temp.hit_extra = if temp.hit_extra < 0 || !additive {
        amount
    } else {
        temp.hit_extra.saturating_add(amount)
    };
    temp.hit_picnum = hit_picnum;
    temp.hit_owner = owner;
    temp.hit_ang = hit_ang;
    temp.hit_source = source_i;
    temp.hit_radius = payload;
    if !matches!(
        target.picnum,
        1975 | 2360 | 1960 | 2630 | 2710 | 2760 | 4740
    ) {
        sprites[target_i].xvel = sprites[target_i].xvel.max(0).wrapping_add(knockback as i16);
    }
}

/// Choose the hit tile applied by a radius blast, including EDuke32's
/// custom-projectile attribution override.
fn rpg_blast_hit_picnum(
    target_extra: i16,
    damage_picnum: i16,
    script: Option<&ScriptImage>,
    projectile_hit_picnum: Option<i16>,
) -> i16 {
    if let Some(projectile_hit_picnum) = projectile_hit_picnum {
        projectile_hit_picnum
    } else if damage_picnum == script_tile(script, "RPG").unwrap_or(RPG_PROJECTILE)
        && target_extra > 0
    {
        damage_picnum
    } else {
        RADIUSEXPLOSION
    }
}

#[allow(clippy::too_many_arguments)]
fn check_radius_target_object(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    randomseed: &mut i32,
    script: Option<&ScriptImage>,
    tile_meta: &[TileZMeta],
    players: &[Player],
    target: usize,
    source_i: i16,
) -> bool {
    checkhitsprite_object(
        lists,
        sprites,
        sectors,
        walls,
        actor_temps,
        picnum,
        randomseed,
        script,
        tile_meta,
        players,
        target,
        source_i,
    )
}

/// Atomic `hitradius` only follows its actor/standable damage bookkeeping with
/// `checkhitsprite` for this fixed set of civilian, statue, and pool-ball tiles.
fn radius_actor_checkhit_target(script: Option<&ScriptImage>, picnum: i16) -> bool {
    [
        "PODFEM1",
        "FEM1",
        "FEM2",
        "FEM3",
        "FEM4",
        "FEM5",
        "FEM6",
        "FEM7",
        "FEM8",
        "FEM9",
        "FEM10",
        "STATUE",
        "STATUEFLASH",
        "SPACEMARINE",
        "QUEBALL",
        "STRIPEBALL",
    ]
    .iter()
    .filter_map(|name| script_tile(script, name))
    .any(|tile| tile == picnum)
}

/// Chocolate `lotsofglass(i, -1, n)` for a sprite-local break.
pub(crate) fn spawn_sprite_glass(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
    parent: SpriteT,
    parent_i: i16,
    count: i16,
) {
    let glass = script.and_then(|image| {
        image
            .labels
            .get("GLASSPIECES")
            .filter(|label| !label.is_ptr)
            .map(|label| label.value as i16)
    });
    for shard in (0..count.max(0)).rev() {
        // Chocolate `lotsofglass(i,-1,n)`: `a = …(TRAND&511)…;` then EGS arguments
        // right to left: zvel before xvel.
        let angle = parent
            .ang
            .wrapping_sub(256)
            .wrapping_add((krand_step(randomseed) & 511) as i16)
            .wrapping_add(1024);
        let zvel = (1024 - (krand_step(randomseed) & 1023)) as i16;
        let xvel = (32 + (krand_step(randomseed) & 63)) as i16;
        if let Some(glass) = glass {
            crate::shoot::egs(
                lists,
                sprites,
                picnum,
                actor_temps,
                script,
                parent.sectnum,
                parent.x,
                parent.y,
                parent.z,
                glass + (shard % 3),
                -32,
                36,
                36,
                angle,
                xvel,
                zvel,
                parent_i,
                STAT_MISC,
            );
        }
    }
}

/// Apply the simple, native `sector.c::checkhitsprite` object transitions
/// shared by direct projectile impacts. `true` means the source's switch arm
/// handled the hit and the generic badguy damage path must not run.
pub(crate) fn checkhitsprite_object(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    randomseed: &mut i32,
    script: Option<&ScriptImage>,
    tile_meta: &[TileZMeta],
    players: &[Player],
    target: usize,
    source_i: i16,
) -> bool {
    let Some(target_sprite) = sprites.get(target).copied() else {
        return false;
    };
    let mut glass_parent = target_sprite;
    let tile = |name: &str| {
        script.and_then(|image| {
            image
                .labels
                .get(name)
                .filter(|label| !label.is_ptr)
                .map(|label| label.value as i16)
        })
    };
    let is = |name: &str| tile(name) == Some(target_sprite.picnum);
    let transition = |name: &str| tile(name);
    let mut sounds: &[i16] = &[];
    let mut global_sounds: &[i16] = &[];
    let mut spawn_tile = None;
    let mut delete_target = false;
    let mut glass_count = 0_i16;

    let source_pic = sprites
        .get(source_i.max(0) as usize)
        .map(|sprite| sprite.picnum);
    let ignites_wood = [
        "RADIUSEXPLOSION",
        "RPG",
        "FIRELASER",
        "HYDRENT",
        "HEAVYHBOMB",
    ]
    .iter()
    .filter_map(|name| tile(name))
    .any(|pic| Some(pic) == source_pic);
    let flammable = ["TREE1", "TREE2", "TIRE", "CONE", "BOX"]
        .iter()
        .filter_map(|name| tile(name))
        .any(|pic| pic == target_sprite.picnum);
    let fragile_tech = ["SATELITE", "FUELPOD", "SOLARPANNEL", "ANTENNA"]
        .iter()
        .filter_map(|name| tile(name))
        .any(|pic| pic == target_sprite.picnum);
    let shot_damage = script.map_or(0, |image| {
        crate::con::init_actor_temps_from_script(image, SHOTSPARK1).0
    });
    let heavy_projectile_hit = sprites
        .get(source_i.max(0) as usize)
        .is_some_and(|source| source.extra != shot_damage);

    if is("QUEBALL") || is("STRIPEBALL") {
        let source_ang = sprites
            .get(source_i.max(0) as usize)
            .map_or(target_sprite.ang, |source| source.ang);
        if krand_step(randomseed) & 3 != 0 {
            sprites[target].xvel = 164;
            sprites[target].ang = source_ang;
        } else {
            spawn_sprite_glass(
                lists,
                sprites,
                actor_temps,
                picnum,
                script,
                randomseed,
                target_sprite,
                target as i16,
                3,
            );
            delete_target = true;
        }
    } else if fragile_tech && heavy_projectile_hit {
        if let Some(sector) = sectors.get(target_sprite.sectnum.max(0) as usize) {
            for piece in 0..15_i32 {
                // Chocolate `EGS(…, SCRAPn+(TRAND&…), -8, …, TRAND&2047, (TRAND&…)+64, -(TRAND&…)…)`:
                // 32-bit gcc evaluates the arguments right to left, so zvel draws first.
                let zvel = -((krand_step(randomseed) & 511) as i16) - 256;
                let xvel = (krand_step(randomseed) & 127) as i16 + 64;
                let angle = (krand_step(randomseed) & 2047) as i16;
                let scrap_tile =
                    tile("SCRAP1").unwrap_or(2400) + (krand_step(randomseed) & 15) as i16;
                crate::shoot::egs(
                    lists,
                    sprites,
                    picnum,
                    actor_temps,
                    script,
                    target_sprite.sectnum,
                    target_sprite.x,
                    target_sprite.y,
                    sector.floorz - (12 << 8) - (piece << 9),
                    scrap_tile,
                    -8,
                    64,
                    64,
                    angle,
                    xvel,
                    zvel,
                    target as i16,
                    STAT_MISC,
                );
            }
        }
        spawn_tile = tile("EXPLOSION2");
        delete_target = true;
    } else if is("CACTUS") && ignites_wood {
        for _ in 0..64 {
            // Chocolate `EGS(…, SCRAPn+(TRAND&…), -8, …, TRAND&2047, (TRAND&…)+64, -(TRAND&…)…)`:
            // 32-bit gcc evaluates the arguments right to left, so zvel draws first.
            let zvel =
                (-(krand_step(randomseed) & 4095) - (i32::from(target_sprite.zvel) >> 2)) as i16;
            let xvel = (krand_step(randomseed) & 63) as i16 + 64;
            let angle = (krand_step(randomseed) & 2047) as i16;
            let scrap = tile("SCRAP3").unwrap_or(2402) + (krand_step(randomseed) & 3) as i16;
            let z = target_sprite
                .z
                .wrapping_sub(krand_step(randomseed) % (48 << 8));
            let shard = crate::shoot::egs(
                lists,
                sprites,
                picnum,
                actor_temps,
                script,
                target_sprite.sectnum,
                target_sprite.x,
                target_sprite.y,
                z,
                scrap,
                -8,
                48,
                48,
                angle,
                xvel,
                zvel,
                target as i16,
                STAT_MISC,
            );
            if shard >= 0 {
                sprites[shard as usize].pal = 8;
            }
        }
        if let Some(broken) = transition("CACTUSBROKE") {
            sprites[target].picnum = broken;
            if let Some(out) = picnum.get_mut(target) {
                *out = broken;
            }
        }
        sprites[target].cstat &= !257;
    } else if is("HANGLIGHT") || is("GENERICPOLE2") {
        for _ in 0..6 {
            // Chocolate `EGS(…, SCRAPn+(TRAND&…), -8, …, TRAND&2047, (TRAND&…)+64, -(TRAND&…)…)`:
            // 32-bit gcc evaluates the arguments right to left, so zvel draws first.
            let zvel =
                (-(krand_step(randomseed) & 4095) - (i32::from(target_sprite.zvel) >> 2)) as i16;
            let xvel = (krand_step(randomseed) & 63) as i16 + 64;
            let angle = (krand_step(randomseed) & 2047) as i16;
            let scrap = tile("SCRAP1").unwrap_or(2400) + (krand_step(randomseed) & 15) as i16;
            crate::shoot::egs(
                lists,
                sprites,
                picnum,
                actor_temps,
                script,
                target_sprite.sectnum,
                target_sprite.x,
                target_sprite.y,
                target_sprite.z - (8 << 8),
                scrap,
                -8,
                48,
                48,
                angle,
                xvel,
                zvel,
                target as i16,
                STAT_MISC,
            );
        }
        sounds = &[20]; // GLASS_HEAVYBREAK
        delete_target = true;
    } else if flammable
        && ignites_wood
        && actor_temps.get(target).is_none_or(|temp| temp.g_t[0] == 0)
    {
        sprites[target].cstat &= !257;
        if actor_temps.len() <= target {
            actor_temps.resize_with(target + 1, ActorConTemps::default);
        }
        actor_temps[target].g_t[0] = 1;
        spawn_tile = tile("BURNING");
    } else if (0..=4).any(|n| is(&format!("OCEANSPRITE{}", n + 1))) {
        spawn_tile = tile("SMALLSMOKE");
        delete_target = true;
    } else if (0..=3)
        .any(|n| tile("WATERFOUNTAIN").is_some_and(|base| target_sprite.picnum == base + n))
    {
        if let Some(broken) = transition("WATERFOUNTAINBROKE") {
            sprites[target].picnum = broken;
            if let Some(out) = picnum.get_mut(target) {
                *out = broken;
            }
        }
        spawn_tile = tile("TOILETWATER");
    } else if is("BOTTLE10") {
        // Chocolate `lotsofmoney`: count roll, then Z/angle/cstat per coin.
        let count = 4 + (krand_step(randomseed) & 3) as i16;
        for _ in 0..count {
            // Chocolate `lotsofmoney`/`lotsofmail`/`lotsofpaper`: `EGS(…, z-(TRAND%(47<<8)), …,
            // TRAND&2047, …)` — right-to-left arguments: the angle draws before z.
            let angle = (krand_step(randomseed) & 2047) as i16;
            let z = target_sprite
                .z
                .wrapping_sub(krand_step(randomseed) % (47 << 8));
            let coin = crate::shoot::egs(
                lists,
                sprites,
                picnum,
                actor_temps,
                script,
                target_sprite.sectnum,
                target_sprite.x,
                target_sprite.y,
                z,
                tile("MONEY").unwrap_or(1233),
                -32,
                8,
                8,
                angle,
                0,
                0,
                0,
                STAT_MISC,
            );
            let cstat = (krand_step(randomseed) & 12) as u16;
            if coin >= 0 {
                sprites[coin as usize].cstat = cstat;
            }
        }
        sounds = &[19]; // GLASS_BREAKING
        glass_parent.ang = (krand_step(randomseed) & 2047) as i16;
        glass_count = 8;
        delete_target = true;
    } else if is("VASE") || is("STATUE") || is("STATUEFLASH") {
        let colored = is("STATUE") || is("STATUEFLASH");
        for shard in (0..40_i16).rev() {
            let angle;
            let z;
            let xvel;
            let zvel;
            let pal;
            // `lotsofcolourglass(i,-1,n)` / `lotsofglass(i,-1,n)`: the angle is
            // its own statement, then the EGS arguments draw right to left
            // (32-bit gcc): zvel, xvel, z; the colour roll comes after.
            if colored {
                angle = (krand_step(randomseed) & 2047) as i16;
                zvel = (1024 - (krand_step(randomseed) & 2047)) as i16;
                xvel = (32 + (krand_step(randomseed) & 63)) as i16;
                z = target_sprite
                    .z
                    .wrapping_sub(krand_step(randomseed) & (63 << 8));
                pal = (krand_step(randomseed) & 15) as u8;
            } else {
                angle = target_sprite
                    .ang
                    .wrapping_sub(256)
                    .wrapping_add((krand_step(randomseed) & 511) as i16)
                    .wrapping_add(1024);
                z = target_sprite.z;
                zvel = (1024 - (krand_step(randomseed) & 1023)) as i16;
                xvel = (32 + (krand_step(randomseed) & 63)) as i16;
                pal = 0;
            }
            if let Some(glass) = tile("GLASSPIECES") {
                let shard_sprite = crate::shoot::egs(
                    lists,
                    sprites,
                    picnum,
                    actor_temps,
                    script,
                    target_sprite.sectnum,
                    target_sprite.x,
                    target_sprite.y,
                    z,
                    glass + shard % 3,
                    -32,
                    36,
                    36,
                    angle,
                    xvel,
                    zvel,
                    target as i16,
                    STAT_MISC,
                );
                if shard_sprite >= 0 && colored {
                    sprites[shard_sprite as usize].pal = pal;
                }
            }
        }
        sounds = if colored { &[20, 19] } else { &[19] };
        glass_parent.ang = (krand_step(randomseed) & 2047) as i16;
        glass_count = 8;
        delete_target = true;
    } else if is("FETUSBROKE") {
        // Chocolate falls through this case into BOTTLE7 after the eight
        // BLOODSPLAT shots, sounds, and the common ten-piece glass burst.
        for _ in 0..48 {
            sprites[target].ang = (krand_step(randomseed) & 2047) as i16;
            crate::shoot::shoot_bloodsplat_enemy(
                lists,
                sprites,
                sectors,
                walls,
                picnum,
                actor_temps,
                tile_meta,
                script,
                randomseed,
                target as i16,
                crate::shoot::BLOODSPLAT1,
            );
            sprites[target].ang = sprites[target].ang.wrapping_add(333);
        }
        sounds = &[20, 69, 19]; // GLASS_HEAVYBREAK, SQUISHED, GLASS_BREAKING
        glass_parent.ang = sprites[target].ang;
        glass_count = 10;
        delete_target = true;
    } else if ["MONK", "LUKE", "INDY", "JURYGUY", "SPACEMARINE"]
        .iter()
        .any(|name| is(name))
    {
        let civilian = ["MONK", "LUKE", "INDY", "JURYGUY"]
            .iter()
            .any(|name| is(name));
        let source = sprites
            .get(source_i.max(0) as usize)
            .copied()
            .unwrap_or_default();
        if civilian {
            if target_sprite.lotag > 0 {
                if actor_temps.len() <= target {
                    actor_temps.resize_with(target + 1, ActorConTemps::default);
                }
                actor_temps[target]
                    .pending_actor_sounds
                    .push(target_sprite.lotag);
            }
            if target_sprite.hitag != 0 {
                let _ = crate::shoot::egs(
                    lists,
                    sprites,
                    picnum,
                    actor_temps,
                    script,
                    target_sprite.sectnum,
                    target_sprite.x,
                    target_sprite.y,
                    target_sprite.z,
                    target_sprite.hitag,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    target as i16,
                    STAT_DEFAULT,
                );
            }
        }
        let health = target_sprite.extra.wrapping_sub(source.extra);
        sprites[target].extra = health;
        if health > 0 {
            return true;
        }
        let blood_tiles = [
            crate::shoot::BLOODSPLAT1,
            crate::shoot::BLOODSPLAT2,
            crate::shoot::BLOODSPLAT3,
            crate::shoot::BLOODSPLAT4,
            crate::shoot::BLOODSPLAT1,
            crate::shoot::BLOODSPLAT2,
            crate::shoot::BLOODSPLAT3,
            crate::shoot::BLOODSPLAT4,
        ];
        for blood_tile in blood_tiles {
            sprites[target].ang = (krand_step(randomseed) & 2047) as i16;
            crate::shoot::shoot_bloodsplat_enemy(
                lists,
                sprites,
                sectors,
                walls,
                picnum,
                actor_temps,
                tile_meta,
                script,
                randomseed,
                target as i16,
                blood_tile,
            );
        }
        spawn_civilian_guts(
            lists,
            sprites,
            sectors,
            walls,
            actor_temps,
            picnum,
            script,
            randomseed,
            sprites[target],
            players.first().map_or(-1, |player| player.sprite_i),
        );
        global_sounds = &[69]; // SQUISHED (Chocolate `sound`, not spritesound)
        delete_target = true;
    } else if is("CHAIR1") || is("CHAIR2") {
        if let Some(broken) = transition("BROKENCHAIR") {
            sprites[target].picnum = broken;
            if let Some(out) = picnum.get_mut(target) {
                *out = broken;
            }
        }
        sprites[target].cstat = 0;
    } else if [
        "CHAIR3",
        "MOVIECAMERA",
        "SCALE",
        "VACUUM",
        "CAMERALIGHT",
        "IVUNIT",
        "POT1",
        "POT2",
        "POT3",
        "TRIPODCAMERA",
    ]
    .iter()
    .any(|name| is(name))
    {
        for _ in 0..16 {
            // Chocolate `RANDOMSCRAP`: 32-bit gcc evaluates the EGS arguments right
            // to left, so zvel draws first and x last.
            let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
            let xvel = (64 + (krand_step(randomseed) & 63)) as i16;
            let angle = (krand_step(randomseed) & 2047) as i16;
            let scrap = tile("SCRAP6").unwrap_or(2390) + (krand_step(randomseed) & 15) as i16;
            let z = target_sprite.z - (8 << 8) - (krand_step(randomseed) & 8191);
            let y = target_sprite.y + (krand_step(randomseed) & 255) - 128;
            let x = target_sprite.x + (krand_step(randomseed) & 255) - 128;
            let _ = crate::shoot::egs(
                lists,
                sprites,
                picnum,
                actor_temps,
                script,
                target_sprite.sectnum,
                x,
                y,
                z,
                scrap,
                -8,
                48,
                48,
                angle,
                xvel,
                zvel,
                target as i16,
                STAT_MISC,
            );
        }
        sounds = &[20]; // GLASS_HEAVYBREAK
        delete_target = true;
    } else if is("FANSPRITE") {
        if let Some(broken) = transition("FANSPRITEBROKE") {
            sprites[target].picnum = broken;
            if let Some(out) = picnum.get_mut(target) {
                *out = broken;
            }
            if sectors
                .get(target_sprite.sectnum.max(0) as usize)
                .is_some_and(|sector| Some(sector.floorpicnum) == tile("FANSHADOW"))
            {
                if let (Some(sector), Some(broken_shadow)) = (
                    sectors.get_mut(target_sprite.sectnum.max(0) as usize),
                    tile("FANSHADOWBROKE"),
                ) {
                    sector.floorpicnum = broken_shadow;
                }
            }
        }
        sprites[target].cstat &= !257;
        sounds = &[20]; // GLASS_HEAVYBREAK
    } else if is("TOILET") || is("STALL") {
        let broken = if is("TOILET") {
            "TOILETBROKE"
        } else {
            "STALLBROKE"
        };
        if let Some(pn) = transition(broken) {
            sprites[target].picnum = pn;
            if let Some(out) = picnum.get_mut(target) {
                *out = pn;
            }
        }
        sprites[target].cstat &= !257;
        spawn_tile = tile("TOILETWATER");
        sounds = if is("TOILET") { &[19] } else { &[20] };
    } else if is("HYDRENT") {
        if let Some(pn) = transition("BROKEFIREHYDRENT") {
            sprites[target].picnum = pn;
            if let Some(out) = picnum.get_mut(target) {
                *out = pn;
            }
        }
        spawn_tile = tile("TOILETWATER");
        sounds = &[20];
    } else if is("GRATE1") || is("CIRCLEPANNEL") || is("PANNEL1") || is("PANNEL2") || is("PANNEL3")
    {
        let broken = if is("GRATE1") {
            "BGRATE1"
        } else if is("CIRCLEPANNEL") {
            "CIRCLEPANNELBROKE"
        } else if is("PANNEL3") {
            "BPANNEL3"
        } else {
            "BPANNEL1"
        };
        if let Some(pn) = transition(broken) {
            sprites[target].picnum = pn;
            if let Some(out) = picnum.get_mut(target) {
                *out = pn;
            }
        }
        sprites[target].cstat &= !257;
        sounds = &[18]; // VENT_BUST
    } else if (1..=6).any(|n| is(&format!("PIPE{}", n))) {
        let broken = format!(
            "PIPE{}B",
            (1..=6).find(|n| is(&format!("PIPE{}", n))).unwrap_or(1)
        );
        if let Some(pn) = transition(&broken) {
            sprites[target].picnum = pn;
            if let Some(out) = picnum.get_mut(target) {
                *out = pn;
            }
        }
        spawn_tile = tile("STEAM");
    } else if is("HYDROPLANT") {
        if let Some(pn) = transition("BROKEHYDROPLANT") {
            sprites[target].picnum = pn;
            if let Some(out) = picnum.get_mut(target) {
                *out = pn;
            }
        }
        sounds = &[19];
    } else if is("BROKEHYDROPLANT") && target_sprite.cstat & 1 != 0 {
        sprites[target].z = sprites[target].z.wrapping_add(16 << 8);
        sprites[target].cstat = 0;
        glass_count = 5;
        sounds = &[19];
    } else if is("BOTTLE7") {
        glass_count = 10;
        delete_target = true;
        sounds = &[19];
    } else if is("FETUS") {
        if let Some(pn) = transition("FETUSBROKE") {
            sprites[target].picnum = pn;
            if let Some(out) = picnum.get_mut(target) {
                *out = pn;
            }
        }
        glass_count = 10;
        sounds = &[19];
    } else if [
        "BOTTLE1",
        "BOTTLE2",
        "BOTTLE3",
        "BOTTLE4",
        "BOTTLE5",
        "BOTTLE6",
        "BOTTLE8",
        "BOTTLE11",
        "BOTTLE12",
        "BOTTLE13",
        "BOTTLE14",
        "BOTTLE15",
        "BOTTLE16",
        "BOTTLE17",
        "BOTTLE18",
        "BOTTLE19",
        "WATERFOUNTAINBROKE",
        "DOMELITE",
        "SUSHIPLATE1",
        "SUSHIPLATE2",
        "SUSHIPLATE3",
        "SUSHIPLATE4",
        "SUSHIPLATE5",
        "WAITTOBESEATED",
    ]
    .iter()
    .any(|name| is(name))
    {
        glass_count = 8;
        delete_target = true;
        sounds = &[19];
        glass_parent.ang = (krand_step(randomseed) & 2047) as i16;
    } else if is("FORCESPHERE") {
        sprites[target].xrepeat = 0;
        let owner = sprites
            .get(source_i.max(0) as usize)
            .map_or(-1, |source| source.owner);
        if owner >= 0 {
            let owner = owner as usize;
            if actor_temps.len() <= owner {
                actor_temps.resize_with(owner + 1, ActorConTemps::default);
            }
            actor_temps[owner].g_t[0] = 32;
            actor_temps[owner].g_t[1] = i32::from(actor_temps[owner].g_t[1] == 0);
            actor_temps[owner].g_t[2] = actor_temps[owner].g_t[2].wrapping_add(1);
        }
        spawn_tile = tile("EXPLOSION2");
    } else {
        return false;
    }

    if glass_count > 0 {
        spawn_sprite_glass(
            lists,
            sprites,
            actor_temps,
            picnum,
            script,
            randomseed,
            glass_parent,
            target as i16,
            glass_count,
        );
    }

    if let Some(pn) = spawn_tile
        && pn == tile("TOILETWATER").unwrap_or(-1)
        && let Some(image) = script
    {
        // Chocolate `spawn(i, TOILETWATER)`: a CON actor (animated
        // TOILETWATERFRAMES, `sizeto` growth, gurgle sound), shade -16.
        let water = crate::spawn_rt::spawn_from(
            lists,
            sprites,
            &*sectors,
            walls,
            picnum,
            actor_temps,
            tile_meta,
            image,
            randomseed,
            target as i16,
            pn,
            None,
        );
        if water >= 0 {
            sprites[water as usize].shade = -16;
        }
    } else if let Some(pn) = spawn_tile {
        let effect = crate::shoot::egs(
            lists,
            sprites,
            picnum,
            actor_temps,
            script,
            target_sprite.sectnum,
            target_sprite.x,
            target_sprite.y,
            target_sprite.z,
            pn,
            0,
            32,
            32,
            target_sprite.ang,
            0,
            0,
            source_i,
            STAT_MISC,
        );
        if pn == tile("STEAM").unwrap_or(-1) && effect >= 0 {
            if let Some(sector) = sectors.get(target_sprite.sectnum.max(0) as usize) {
                sprites[effect as usize].z = sector.floorz - (32 << 8);
            }
        }
    }
    if !sounds.is_empty() {
        if actor_temps.len() <= target {
            actor_temps.resize_with(target + 1, ActorConTemps::default);
        }
        actor_temps[target]
            .pending_actor_sounds
            .extend_from_slice(sounds);
    }
    if !global_sounds.is_empty() {
        if actor_temps.len() <= target {
            actor_temps.resize_with(target + 1, ActorConTemps::default);
        }
        actor_temps[target]
            .pending_global_sounds
            .extend_from_slice(global_sounds);
    }
    if delete_target {
        let _ = lists.deletesprite(target as i16);
    }
    true
}

/// Resolve the spawned Atomic `KNEE` impact: emit its brief smoke sprite,
/// run `checkhitsprite` for a struck sprite, or `checkhitwall` for a struck
/// wall. Player damage was queued by the knee hitscan itself.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_knee_hit_effects(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    script: Option<&ScriptImage>,
    tile_meta: &[TileZMeta],
    players: &[Player],
    hit: crate::shoot::KneeHit,
    player_sprite: i16,
    player_angle: i16,
) -> (Vec<i16>, Option<i16>) {
    let [x, y, z] = hit.position;
    let knee = crate::shoot::egs(
        lists,
        sprites,
        picnum,
        actor_temps,
        script,
        hit.sector,
        x,
        y,
        z,
        crate::shoot::KNEE,
        -15,
        0,
        0,
        player_angle.wrapping_add(15),
        32,
        0,
        player_sprite,
        STAT_PROJECTILE,
    );
    let Ok(knee_index) = usize::try_from(knee) else {
        return (Vec::new(), None);
    };
    let Some(knee_source) = sprites.get_mut(knee_index) else {
        return (Vec::new(), None);
    };
    knee_source.extra = hit.impact_strength;
    let knee_source = *knee_source;

    let smoke = crate::shoot::spawn_smallsmoke(
        lists,
        sprites,
        sectors,
        walls,
        picnum,
        actor_temps,
        script,
        randomseed,
        knee,
    );
    if let Ok(smoke_index) = usize::try_from(smoke)
        && let Some(smoke) = sprites.get_mut(smoke_index)
    {
        smoke.z = smoke.z.wrapping_sub(8 << 8);
    }

    if let Some(target) = hit.sprite.and_then(|index| usize::try_from(index).ok())
        && let Some(target_sprite) = sprites.get(target)
    {
        let access_switch = [
            script_tile(script, "ACCESSSWITCH").unwrap_or(130),
            script_tile(script, "ACCESSSWITCH2").unwrap_or(170),
        ];
        if !access_switch.contains(&target_sprite.picnum) {
            let _ = checkhitsprite_object(
                lists,
                sprites,
                sectors,
                walls,
                actor_temps,
                picnum,
                randomseed,
                script,
                tile_meta,
                players,
                target,
                knee,
            );
        }
    }

    let Some(mut wall_index) = hit.wall.and_then(|index| usize::try_from(index).ok()) else {
        return (Vec::new(), None);
    };
    if walls
        .get(wall_index)
        .is_some_and(|wall| wall.cstat & 2 != 0)
        && let Some(wall) = walls.get(wall_index)
        && let Some(next_sector) = usize::try_from(wall.nextsector)
            .ok()
            .and_then(|index| sectors.get(index))
        && z >= next_sector.floorz
        && let Some(peer) = usize::try_from(wall.nextwall)
            .ok()
            .filter(|&index| index < walls.len())
    {
        wall_index = peer;
    }
    let access_wall = [
        script_tile(script, "ACCESSSWITCH").unwrap_or(130),
        script_tile(script, "ACCESSSWITCH2").unwrap_or(170),
    ];
    let wall_id = i16::try_from(wall_index).ok();
    let sounds = if walls
        .get(wall_index)
        .is_some_and(|wall| !access_wall.contains(&wall.picnum))
    {
        crate::weapons_move::apply_checkhitwall(
            lists,
            sprites,
            sectors,
            walls,
            picnum,
            actor_temps,
            randomseed,
            knee_source,
            knee,
            crate::shoot::KNEE,
            wall_index,
            x,
            y,
            z,
            hit.sector,
            script,
        )
    } else {
        Vec::new()
    };
    (sounds, wall_id)
}

/// Atomic `TICSPERFRAME` at 30 Hz.
const TICSPERFRAME: i32 = 4;
const EXPLOSION2: i16 = 1890;
const TRANSPORTERSTAR: i16 = 1630;
const APLAYER: i16 = 1405;
const RPG_BLAST_RADIUS: i32 = 1780;
const RADIUSEXPLOSION: i16 = 1670;
const PISTOL_BODYHIT: u16 = 2;
const DUKE_LONGTERM_PAIN: u16 = 211;

/// Chocolate `checkhitsprite` wakes a damaged sleeping badguy into STAT_ACTOR.
fn wake_hit_badguy(lists: &mut SpriteLists, actor_temps: &mut [ActorConTemps], target: usize) {
    if lists.sprite_stat.get(target).copied() == Some(STAT_DUMMY) {
        let _ = lists.changespritestat(target as i16, STAT_ACTOR);
        if let Some(temps) = actor_temps.get_mut(target) {
            temps.timetosleep = crate::fta::SLEEPTIME;
        }
    }
}

fn spawn_civilian_guts(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
    body: SpriteT,
    player_owner: i16,
) {
    let sector_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let floor_z = usize::try_from(body.sectnum)
        .ok()
        .filter(|&index| index < sectors.len())
        .map(|_| getzsofslope(body.sectnum, body.x, body.y, &sector_z, &wall_xy).1)
        .unwrap_or(body.z);
    let gut_z = (body.z - (8 << 8)).min(floor_z - (8 << 8));
    let is_badguy = crate::badguy::badguy_pic(
        body.picnum,
        script.map_or(&[], |image| image.actortype.as_slice()),
    );
    let repeats = if is_badguy && body.xrepeat < 16 {
        8
    } else {
        32
    };
    let palette = if is_badguy && body.pal == 6 { 6 } else { 0 };
    for (name, count) in [
        ("JIBS1", 1),
        ("JIBS2", 2),
        ("JIBS3", 3),
        ("JIBS4", 4),
        ("JIBS5", 1),
        ("JIBS3", 6),
    ] {
        let tile = script
            .and_then(|image| image.labels.get(name))
            .filter(|label| !label.is_ptr)
            .map_or_else(
                || match name {
                    "JIBS1" => 2245,
                    "JIBS2" => 2250,
                    "JIBS3" => 2255,
                    "JIBS4" => 2260,
                    _ => 2265,
                },
                |label| label.value as i16,
            );
        for _ in 0..count {
            // Chocolate `guts`: `a = TRAND&2047;` first, then the EGS arguments
            // right to left (32-bit gcc): zvel, xvel, z, y, x.
            let angle = (krand_step(randomseed) & 2047) as i16;
            let zvel = (-512 - (krand_step(randomseed) & 2047)) as i16;
            let xvel = (48 + (krand_step(randomseed) & 31)) as i16;
            let z = gut_z - (krand_step(randomseed) & 8191);
            let y = body.y + (krand_step(randomseed) & 255) - 128;
            let x = body.x + (krand_step(randomseed) & 255) - 128;
            let repeat = if name == "JIBS2" {
                repeats >> 2
            } else {
                repeats
            };
            let shard = crate::shoot::egs(
                lists,
                sprites,
                picnum,
                actor_temps,
                script,
                body.sectnum,
                x,
                y,
                z,
                tile,
                -32,
                repeat,
                repeat,
                angle,
                xvel,
                zvel,
                player_owner,
                STAT_MISC,
            );
            if shard >= 0 && palette == 6 {
                sprites[shard as usize].pal = palette;
            }
        }
    }
}

/// Chocolate `moveweapons` subset: run CON on `SHOTSPARK1`.
pub fn moveweapons(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    players: &mut [Player],
    randomseed: &mut i32,
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&ScriptImage>,
    tile_meta: &[TileZMeta],
) {
    let rpg_tile = crate::shoot::script_tile_or(script, "RPG", RPG_PROJECTILE);
    let mut order: Vec<i16> = Vec::new();
    let mut i = lists.headspritestat[STAT_PROJECTILE as usize];
    while i >= 0 {
        order.push(i);
        i = lists.nextspritestat[i as usize];
    }
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let sec_w: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();

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
        if sprites[iu].picnum == SPIT {
            move_vanilla_firelaser(
                lists,
                sprites,
                sectors,
                walls,
                players,
                randomseed,
                actor_temps,
                picnum,
                script,
                tile_meta,
                i,
                &sec_w,
                &wxy,
                &sec_z,
            );
            continue;
        }
        if sprites[iu].picnum == FIRELASER {
            move_vanilla_firelaser(
                lists,
                sprites,
                sectors,
                walls,
                players,
                randomseed,
                actor_temps,
                picnum,
                script,
                tile_meta,
                i,
                &sec_w,
                &wxy,
                &sec_z,
            );
            continue;
        }
        // Atomic's Octabrain projectile flies until it hits a wall, plane, or
        // player. It sticks to ordinary actors without damaging them, and
        // fades after forty actor tics.
        if sprites[iu].picnum == COOLEXPLOSION1 {
            let source = sprites[iu];
            if let Some(sound) = script
                .and_then(|image| image.labels.get("WIERDSHOT_FLY"))
                .filter(|label| !label.is_ptr)
                .map(|label| label.value as i16)
            {
                if actor_temps.len() <= iu {
                    actor_temps.resize_with(iu + 1, ActorConTemps::default);
                }
                // Chocolate starts this sound only while no voice of that ID
                // is active; the audio bridge's once queue applies that gate.
                actor_temps[iu].pending_actor_sounds_once.push(sound);
            }
            let xv = i32::from(source.xvel);
            let zv = i32::from(source.zvel);
            let (sin, cos) = sincos_q14(i32::from(sprites[iu].ang) & 2047);
            let dx = (((xv * cos) >> 14) * TICSPERFRAME) >> 3;
            let dy = (((xv * sin) >> 14) * TICSPERFRAME) >> 3;
            let dz = (zv * TICSPERFRAME) >> 3;
            // Chocolate `movesprite` (non-badguy, STAT_PROJECTILE): clipmove
            // with walldist 8 at `z - height`, then the z step if it stays
            // between the cached planes; otherwise a sector hit.
            let height = tile_meta
                .get(source.picnum.max(0) as usize)
                .map_or(0, |m| i32::from(m.height));
            let clip_z = source.z - ((height * i32::from(source.yrepeat)) << 1);
            let mut sprite_clips: Vec<_> =
                crate::player::live_sprite_z_clips(sprites, &lists.sprite_stat, tile_meta);
            if let Some(me) = sprite_clips.get_mut(iu) {
                me.cstat = 0;
            }
            let (mut nx, mut ny, mut nsect) = (source.x, source.y, source.sectnum);
            // `qq = CLIPMASK1`: walls stop it only on `cstat & 64`.
            let retval = room_build_engine::geom::clipmove_xy_at_z_cliptype(
                &mut nx,
                &mut ny,
                clip_z,
                &mut nsect,
                dx,
                dy,
                8,
                4 << 8,
                4 << 8,
                &sec_w,
                &wxy,
                Some(&sec_z),
                &sprite_clips,
                (256 << 16) | 64,
                Some(&*lists),
            );
            let sect_for_planes = if nsect >= 0 { nsect } else { source.sectnum };
            // Chocolate `moveweapons` runs `getglobalz(i)` at the old position
            // before `movesprite`, which then tests the Z step against those
            // cached `hittype` planes (not the planes at the new position).
            let (ceilz, florz) =
                projectile_globalz(&source, sectors, &sec_z, &wxy, &sprite_clips, sprites);
            let daz = source.z + dz;
            let z_ok = daz > ceilz && daz <= florz;
            let nz = if z_ok { daz } else { source.z };
            let hit = room_build_engine::geom::HitScan {
                hitsect: sect_for_planes,
                hitwall: if retval & 49152 == 32768 {
                    (retval & 32767) as i16
                } else {
                    -1
                },
                hitsprite: if retval & 49152 == 49152 {
                    (retval & 4095) as i16
                } else {
                    -1
                },
                hitx: nx,
                hity: ny,
                hitz: nz,
            };
            let within_step = retval != 0 || !z_ok;
            if hit.hitsect >= 0 && within_step {
                sprites[iu].x = hit.hitx;
                sprites[iu].y = hit.hity;
                sprites[iu].z = hit.hitz;
                if hit.hitsect != source.sectnum {
                    let _ = lists.changespritesect(i, hit.hitsect);
                }
                sprites[iu].sectnum = hit.hitsect;

                let mut stop = hit.hitwall >= 0 || (hit.hitwall < 0 && hit.hitsprite < 0);
                // Chocolate `moveweapons`: a wall hit (`32768`, not a mirror) or a
                // floor/ceiling hit (`16384`) calls `setsprite(i,dax,day,daz)`,
                // putting the projectile back where it was before `movesprite`
                // slid it. Sprite hits keep the moved position.
                if hit.hitsprite < 0 {
                    sprites[iu].x = source.x;
                    sprites[iu].y = source.y;
                    sprites[iu].z = source.z;
                    if sprites[iu].sectnum != source.sectnum {
                        let _ = lists.changespritesect(i, source.sectnum);
                        sprites[iu].sectnum = source.sectnum;
                    }
                }
                if hit.hitsprite >= 0 {
                    let target = hit.hitsprite as usize;
                    if target < sprites.len() && sprites[target].picnum == APLAYER {
                        stop = true;
                        if actor_temps.len() <= target {
                            actor_temps.resize_with(target + 1, ActorConTemps::default);
                        }
                        let temp = &mut actor_temps[target];
                        temp.hit_picnum = COOLEXPLOSION1;
                        // Chocolate `checkhitsprite`: `hittype.extra += extra` onto the
                        // idle -1, so a hit deals one less than the shot's `extra`.
                        temp.hit_extra = temp.hit_extra.saturating_add(source.extra);
                        temp.hit_owner = source.owner;
                        temp.hit_source = i;
                        temp.hit_radius = crate::con::direct_hit_payload();
                        temp.hit_ang = source.ang;
                        temp.pending_actor_sounds
                            .push(i16::try_from(PISTOL_BODYHIT).unwrap_or_default());
                    }
                    // `moveweapons` skips non-player sprite hits for this tile.
                }
                // A non-player sprite hit is `goto BOLT`: no velocity reset, no
                // `checkhitsprite`, and no `shade++` fade this tic.
                if hit.hitsprite >= 0 && !stop {
                    continue;
                }

                if hit.hitwall >= 0 {
                    let wall_sounds = apply_checkhitwall(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        picnum,
                        actor_temps,
                        randomseed,
                        source,
                        i,
                        source.picnum,
                        hit.hitwall as usize,
                        // `checkhitwall(i,j,s->x,s->y,s->z,…)` after the restore.
                        source.x,
                        source.y,
                        source.z,
                        source.sectnum,
                        script,
                    );
                    if actor_temps.len() <= iu {
                        actor_temps.resize_with(iu + 1, ActorConTemps::default);
                    }
                    actor_temps[iu].pending_actor_sounds.extend(wall_sounds);
                } else if hit.hitsprite < 0 && source.zvel < 0 {
                    // `checkhitceiling(s->sectnum)` after `setsprite` restored it.
                    let is_sky = sectors.get(source.sectnum as usize).is_some_and(|sector| {
                        sector.ceilingstat & 1 != 0 && sector.ceilingpal == 0
                    });
                    if is_sky {
                        let _ = lists.deletesprite(i);
                        continue;
                    }
                    let mut ceiling_sounds = Vec::new();
                    crate::shoot::check_hit_ceiling(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        picnum,
                        actor_temps,
                        script,
                        randomseed,
                        source.sectnum,
                        i,
                        source.owner,
                        &mut ceiling_sounds,
                    );
                    if actor_temps.len() <= iu {
                        actor_temps.resize_with(iu + 1, ActorConTemps::default);
                    }
                    actor_temps[iu].pending_actor_sounds.extend(
                        ceiling_sounds
                            .into_iter()
                            .map(|sound| i16::try_from(sound).unwrap_or_default()),
                    );
                } else if hit.hitsprite < 0
                    && hit.hitwall < 0
                    && source.zvel > 0
                    && !sectors
                        .get(source.sectnum as usize)
                        .is_some_and(|sector| sector.floorstat & 1 != 0 && sector.floorpal == 0)
                {
                    // EDuke32 `Proj_MaybeDamageCF`: a downward plane hit
                    // damages the floor unless it is unshaded parallax.
                    queue_floor_damage(script, sectors, i, source.sectnum);
                }

                if stop {
                    sprites[iu].xvel = 0;
                    sprites[iu].zvel = 0;
                }
            } else {
                sprites[iu].x = source.x.wrapping_add(dx);
                sprites[iu].y = source.y.wrapping_add(dy);
                sprites[iu].z = source.z.wrapping_add(dz);
                let mut next_sector = source.sectnum;
                room_build_engine::geom::updatesector(
                    sprites[iu].x,
                    sprites[iu].y,
                    &mut next_sector,
                    &sec_w,
                    &wxy,
                );
                if next_sector >= 0 && next_sector != source.sectnum {
                    let _ = lists.changespritesect(i, next_sector);
                    sprites[iu].sectnum = next_sector;
                }
            }

            sprites[iu].shade = sprites[iu].shade.saturating_add(1);
            if sprites[iu].shade >= 40 {
                let _ = lists.deletesprite(i);
            }
            continue;
        }
        let projectile_workslike = actor_temps
            .get(iu)
            .map_or(0, |temps| temps.projectile_workslike);
        if projectile_workslike
            & (crate::shoot::PROJECTILE_FLAG_HITSCAN | crate::shoot::PROJECTILE_FLAG_KNEE)
            != 0
        {
            let _ = lists.deletesprite(i);
            continue;
        }
        let custom_rpg = projectile_workslike & crate::shoot::PROJECTILE_FLAG_RPG != 0;
        if sprites[iu].picnum == rpg_tile && !custom_rpg {
            move_vanilla_rpg(
                lists,
                sprites,
                sectors,
                walls,
                players,
                randomseed,
                actor_temps,
                picnum,
                script,
                tile_meta,
                i,
                &sec_w,
                &wxy,
                &sec_z,
            );
            continue;
        }
        if sprites[iu].picnum == rpg_tile || custom_rpg {
            let mut timer_expired = false;
            if custom_rpg
                && projectile_workslike & crate::shoot::PROJECTILE_FLAG_TIMED != 0
                && actor_temps
                    .get(iu)
                    .is_some_and(|temps| temps.projectile_range > 0)
            {
                let temps = &mut actor_temps[iu];
                if temps.projectile_age >= temps.projectile_range {
                    if projectile_workslike & crate::shoot::PROJECTILE_FLAG_EXPLODEONTIMER != 0 {
                        timer_expired = true;
                    } else {
                        let _ = lists.deletesprite(i);
                        continue;
                    }
                } else {
                    temps.projectile_age = temps.projectile_age.saturating_add(1);
                }
            }
            if custom_rpg {
                let drop = actor_temps.get(iu).map_or(0, |temps| temps.projectile_drop);
                sprites[iu].zvel = i32::from(sprites[iu].zvel)
                    .wrapping_sub(drop)
                    .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                    as i16;
            }
            let source = sprites[iu];
            // EDuke32's PROJ_VEL_MULT repeats this movement step. Keep the
            // individual step's collision and sector update below, including
            // any remaining steps after a bounce changes projectile angle.
            let velocity_multiplier = if custom_rpg {
                actor_temps
                    .get(iu)
                    .map_or(0, |temps| temps.projectile_velmult)
                    .max(1)
            } else {
                1
            };
            if custom_rpg && !timer_expired {
                let temps = actor_temps.get(iu);
                let trail_tile = temps.map_or(0, |temps| temps.projectile_trail);
                let trail_count = temps.map_or(0, |temps| temps.projectile_tnum).clamp(0, 32);
                if let Ok(trail_tile) = i16::try_from(trail_tile)
                    && trail_tile > 0
                {
                    let repeat_x = temps
                        .map_or(0, |temps| temps.projectile_txrepeat)
                        .clamp(0, i32::from(u8::MAX));
                    let repeat_y = temps
                        .map_or(0, |temps| temps.projectile_tyrepeat)
                        .clamp(0, i32::from(u8::MAX));
                    let repeat_x = if repeat_x == 0 { 32 } else { repeat_x as u8 };
                    let repeat_y = if repeat_y == 0 { 32 } else { repeat_y as u8 };
                    let offset = temps.map_or(0, |temps| temps.projectile_toffset);
                    for _ in 0..trail_count {
                        let _ = crate::shoot::egs(
                            lists,
                            sprites,
                            picnum,
                            actor_temps,
                            script,
                            source.sectnum,
                            source.x,
                            source.y,
                            source.z.wrapping_add(offset.wrapping_shl(8)),
                            trail_tile,
                            source.shade,
                            repeat_x,
                            repeat_y,
                            source.ang,
                            0,
                            0,
                            i,
                            crate::stat::STAT_MISC,
                        );
                    }
                }
            }
            let mut spr_hits: Vec<SpriteHit> = sprites
                .iter()
                .enumerate()
                .map(|(index, sp)| {
                    let mut hit = SpriteHit::cylinder(
                        sp.x,
                        sp.y,
                        sp.z,
                        sp.sectnum,
                        i32::from(sp.clipdist) << 2,
                        sp.cstat,
                    );
                    if custom_rpg
                        && (index == iu || source.owner >= 0 && index == source.owner as usize)
                    {
                        hit.cstat = 0;
                    }
                    hit
                })
                .collect();
            crate::shoot::drop_deleted_hits(&mut spr_hits, &lists.sprite_stat);
            let hit = if timer_expired {
                room_build_engine::geom::HitScan {
                    hitsect: source.sectnum,
                    hitwall: -1,
                    hitsprite: -1,
                    hitx: source.x,
                    hity: source.y,
                    hitz: source.z,
                }
            } else {
                advance_projectile_with_sweeps(
                    lists,
                    sprites,
                    actor_temps,
                    i,
                    velocity_multiplier,
                    projectile_workslike,
                    script_tile(script, "MIRROR").unwrap_or(560),
                    &sec_w,
                    &wxy,
                    &sec_z,
                    &spr_hits,
                    walls,
                )
            };
            if custom_rpg
                && !timer_expired
                && projectile_workslike & crate::shoot::PROJECTILE_FLAG_WATERBUBBLES != 0
            {
                let (bubble_sector, bubble_x, bubble_y, bubble_z) = if hit.hitsect >= 0 {
                    (hit.hitsect, hit.hitx, hit.hity, hit.hitz)
                } else {
                    let projectile = sprites[iu];
                    (projectile.sectnum, projectile.x, projectile.y, projectile.z)
                };
                spawn_custom_projectile_water_bubble(
                    lists,
                    sprites,
                    sectors,
                    picnum,
                    actor_temps,
                    script,
                    randomseed,
                    bubble_sector,
                    bubble_x,
                    bubble_y,
                    bubble_z,
                    sprites[iu].owner,
                );
            }
            let plane_hit = hit.hitsect >= 0 && hit.hitwall < 0 && hit.hitsprite < 0;
            if hit.hitwall >= 0 || hit.hitsprite >= 0 || plane_hit {
                sprites[iu].x = hit.hitx;
                sprites[iu].y = hit.hity;
                sprites[iu].z = hit.hitz;
                if hit.hitsect >= 0 {
                    if lists.sprite_sect[iu] != hit.hitsect {
                        let _ = lists.changespritesect(i, hit.hitsect);
                    }
                    sprites[iu].sectnum = hit.hitsect;
                }
                if plane_hit && !timer_expired && sprites[iu].zvel < 0 {
                    if let Some(sector) = usize::try_from(hit.hitsect)
                        .ok()
                        .and_then(|index| sectors.get(index))
                        && sector.ceilingstat & 1 != 0
                        && sector.ceilingpal == 0
                    {
                        let _ = lists.deletesprite(i);
                        continue;
                    }
                    let mut ceiling_sounds = Vec::new();
                    crate::shoot::check_hit_ceiling(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        picnum,
                        actor_temps,
                        script,
                        randomseed,
                        hit.hitsect,
                        i,
                        sprites[iu].owner,
                        &mut ceiling_sounds,
                    );
                    if actor_temps.len() <= iu {
                        actor_temps.resize_with(iu + 1, ActorConTemps::default);
                    }
                    actor_temps[iu].pending_actor_sounds.extend(
                        ceiling_sounds
                            .into_iter()
                            .map(|sound| i16::try_from(sound).unwrap_or_default()),
                    );
                } else if plane_hit
                    && !timer_expired
                    && sprites[iu].zvel > 0
                    && !usize::try_from(hit.hitsect)
                        .ok()
                        .and_then(|index| sectors.get(index))
                        .is_some_and(|sector| sector.floorstat & 1 != 0 && sector.floorpal == 0)
                {
                    queue_floor_damage(script, sectors, i, hit.hitsect);
                }
                if plane_hit
                    && custom_rpg
                    && !timer_expired
                    && projectile_workslike & crate::shoot::PROJECTILE_FLAG_BOUNCESOFFWALLS != 0
                    && sprites[iu].yvel > 0
                {
                    bounce_freezeblast(&mut sprites[iu], sectors, walls, &sec_z, &wxy);
                    sprites[iu].yvel = sprites[iu].yvel.wrapping_sub(1);
                    queue_custom_projectile_bounce_sound(actor_temps, iu);
                    if projectile_workslike & crate::shoot::PROJECTILE_FLAG_LOSESVELOCITY != 0 {
                        decay_custom_projectile_velocity(&mut sprites[iu]);
                    }
                    continue;
                }
                if hit.hitsprite >= 0 {
                    let target = hit.hitsprite as usize;
                    let projectile = sprites[iu];
                    if target < sprites.len()
                        && projectile.owner != hit.hitsprite
                        && lists.sprite_stat.get(target).copied().unwrap_or(0) != STAT_PROJECTILE
                    {
                        let object_handled = checkhitsprite_object(
                            lists,
                            sprites,
                            sectors,
                            walls,
                            actor_temps,
                            picnum,
                            randomseed,
                            script,
                            tile_meta,
                            players,
                            target,
                            i,
                        );
                        if actor_temps.len() <= target {
                            actor_temps.resize_with(target + 1, ActorConTemps::default);
                        }
                        let is_badguy = crate::badguy::badguy_pic(
                            sprites[target].picnum,
                            script.map_or(&[], |image| image.actortype.as_slice()),
                        );
                        let direct_hit_enabled = !custom_rpg
                            || projectile_workslike & crate::shoot::PROJECTILE_FLAG_RPG_IMPACT != 0;
                        let direct_damage = if !direct_hit_enabled {
                            0
                        } else if custom_rpg
                            && projectile_workslike
                                & crate::shoot::PROJECTILE_FLAG_RPG_IMPACT_DAMAGE
                                != 0
                        {
                            // EDuke32 adds the projectile's `extra` again.
                            projectile.extra.saturating_mul(2)
                        } else if custom_rpg {
                            projectile.extra
                        } else if is_badguy {
                            projectile.extra.saturating_mul(2)
                        } else {
                            projectile.extra
                        };
                        if !object_handled && direct_hit_enabled {
                            actor_temps[target].hit_picnum = if custom_rpg {
                                projectile.picnum
                            } else {
                                rpg_tile
                            };
                            actor_temps[target].hit_extra = if actor_temps[target].hit_extra < 0 {
                                direct_damage
                            } else {
                                actor_temps[target].hit_extra.saturating_add(direct_damage)
                            };
                            actor_temps[target].hit_owner = projectile.owner;
                            actor_temps[target].hit_source = i;
                            actor_temps[target].hit_radius = crate::con::direct_hit_payload();
                            actor_temps[target].hit_ang = projectile.ang;
                            if is_badguy {
                                wake_hit_badguy(lists, actor_temps, target);
                            }
                        }
                        if !object_handled && sprites[target].picnum == APLAYER {
                            if projectile_workslike & crate::shoot::PROJECTILE_FLAG_SPIT != 0 {
                                if let Some(player) = players
                                    .iter_mut()
                                    .find(|player| player.sprite_i == hit.hitsprite)
                                {
                                    apply_spit_player_effects(
                                        player,
                                        actor_temps,
                                        hit.hitsprite,
                                        &mut *randomseed,
                                    );
                                }
                            } else {
                                actor_temps[target]
                                    .pending_actor_sounds
                                    .push(i16::try_from(PISTOL_BODYHIT).unwrap_or_default());
                            }
                        }
                    }
                }
                let blast_strength = i32::from(sprites[iu].extra.max(1));
                let blast_radius = if custom_rpg {
                    actor_temps
                        .get(iu)
                        .map_or(0, |temps| temps.projectile_hitradius)
                } else {
                    RPG_BLAST_RADIUS
                };
                let blast_picnum = if custom_rpg
                    && projectile_workslike & crate::shoot::PROJECTILE_FLAG_RPG_IMPACT == 0
                {
                    RADIUSEXPLOSION
                } else {
                    sprites[iu].picnum
                };
                let radius_picnum = (custom_rpg
                    && projectile_workslike
                        & (crate::shoot::PROJECTILE_FLAG_RADIUS_PICNUM
                            | crate::shoot::PROJECTILE_FLAG_RADIUS_PICNUM_EX)
                        != 0)
                    .then_some(sprites[iu].picnum);
                if custom_rpg
                    && sprites[iu].extra <= 0
                    && blast_radius <= 0
                    && projectile_workslike
                        & (crate::shoot::PROJECTILE_FLAG_RPG_IMPACT
                            | crate::shoot::PROJECTILE_FLAG_FORCEIMPACT)
                        == 0
                {
                    let _ = lists.deletesprite(i);
                    continue;
                }
                apply_rpg_blast(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    actor_temps,
                    players,
                    randomseed,
                    blast_strength,
                    blast_radius,
                    blast_picnum,
                    radius_picnum,
                    sprites[iu],
                    i,
                    picnum,
                    script,
                    tile_meta,
                    script
                        .map(|image| image.actortype.as_slice())
                        .unwrap_or(&[]),
                    sprites[iu].x,
                    sprites[iu].y,
                    sprites[iu].z,
                    sprites[iu].sectnum,
                    sprites[iu].owner,
                );
                let impact_state = actor_temps.get(iu).cloned().unwrap_or_default();
                let timed_effects_enabled = !timer_expired
                    || projectile_workslike & crate::shoot::PROJECTILE_FLAG_EXPLODEONTIMER != 0;
                if timed_effects_enabled {
                    if impact_state.projectile_isound > 0
                        && let Ok(sound) = i16::try_from(impact_state.projectile_isound)
                    {
                        actor_temps[iu].pending_actor_sounds.push(sound);
                    }
                    if let Ok(spawn_tile) = i16::try_from(impact_state.projectile_spawns)
                        && spawn_tile > 0
                    {
                        let impact = sprites[iu];
                        let repeat_x = if impact_state.projectile_sxrepeat > 0 {
                            impact_state
                                .projectile_sxrepeat
                                .clamp(0, i32::from(u8::MAX)) as u8
                        } else if impact.xrepeat > 0 {
                            impact.xrepeat
                        } else {
                            32
                        };
                        let repeat_y = if impact_state.projectile_syrepeat > 0 {
                            impact_state
                                .projectile_syrepeat
                                .clamp(0, i32::from(u8::MAX)) as u8
                        } else if impact.yrepeat > 0 {
                            impact.yrepeat
                        } else {
                            32
                        };
                        let _ = crate::shoot::egs(
                            lists,
                            sprites,
                            picnum,
                            actor_temps,
                            script,
                            impact.sectnum,
                            impact.x,
                            impact.y,
                            impact.z,
                            spawn_tile,
                            impact.shade,
                            repeat_x,
                            repeat_y,
                            impact.ang,
                            0,
                            0,
                            i,
                            crate::stat::STAT_ACTOR,
                        );
                    }
                }
                let wall_sounds = if iu < actor_temps.len() {
                    std::mem::take(&mut actor_temps[iu].pending_actor_sounds)
                } else {
                    Vec::new()
                };
                sprites[iu].picnum = EXPLOSION2;
                sprites[iu].xrepeat = 32;
                sprites[iu].yrepeat = 32;
                sprites[iu].cstat = 0;
                let _ = lists.changespritestat(i, crate::stat::STAT_MISC);
                if iu < picnum.len() {
                    picnum[iu] = EXPLOSION2;
                }
                if actor_temps.len() <= iu {
                    actor_temps.resize_with(iu + 1, ActorConTemps::default);
                }
                actor_temps[iu] = ActorConTemps::default();
                actor_temps[iu].pending_actor_sounds = wall_sounds;
                actor_temps[iu].g_t[5] = 0;
            }
            continue;
        }
        if matches!(sprites[iu].picnum, FREEZEBLAST | SHRINKSPARK) {
            let source = sprites[iu];
            if source.picnum == FREEZEBLAST
                && (source.yvel < 1 || source.extra < 2 || (source.xvel | source.zvel) == 0)
            {
                let star = crate::shoot::egs(
                    lists,
                    sprites,
                    picnum,
                    actor_temps,
                    script,
                    source.sectnum,
                    source.x,
                    source.y,
                    source.z,
                    TRANSPORTERSTAR,
                    0,
                    32,
                    32,
                    source.ang,
                    0,
                    0,
                    i,
                    crate::stat::STAT_MISC,
                );
                if star >= 0 {
                    sprites[star as usize].pal = 1;
                }
                let _ = lists.deletesprite(i);
                continue;
            }
            let xv = i32::from(source.xvel);
            let (sin, cos) = sincos_q14(i32::from(source.ang) & 2047);
            let dx = (((xv * cos) >> 14) * TICSPERFRAME) >> 3;
            let dy = (((xv * sin) >> 14) * TICSPERFRAME) >> 3;
            let dz = (i32::from(source.zvel) * TICSPERFRAME) >> 3;
            let freezer_hurts_owner = source.picnum == FREEZEBLAST
                && script
                    .and_then(|image| {
                        let index = if image.con_version == crate::con::ConVersion::V15 {
                            27
                        } else {
                            25
                        };
                        image.gamestartup.get(index).copied()
                    })
                    .unwrap_or(0)
                    != 0;
            let mut sprite_hits: Vec<SpriteHit> = sprites
                .iter()
                .enumerate()
                .map(|(index, sp)| {
                    let mut hit = SpriteHit::cylinder(
                        sp.x,
                        sp.y,
                        sp.z,
                        sp.sectnum,
                        i32::from(sp.clipdist) << 2,
                        sp.cstat,
                    );
                    let is_owner = source.owner >= 0 && index == source.owner as usize;
                    if index == iu
                        || (is_owner && (source.picnum == SHRINKSPARK || !freezer_hurts_owner))
                        || source.picnum == SHRINKSPARK && sp.xrepeat < 24 && sp.picnum != 1550
                    {
                        hit.cstat = 0;
                    }
                    hit
                })
                .collect();
            crate::shoot::drop_deleted_hits(&mut sprite_hits, &lists.sprite_stat);
            let hit = hitscan_full(
                source.x,
                source.y,
                source.z,
                source.sectnum,
                dx,
                dy,
                dz,
                (256 << 16) + 64,
                &sec_w,
                &wxy,
                &sprite_hits,
                Some(&sec_z),
            );
            if hit_beyond_step(&hit, source.x, source.y, source.z, dx, dy, dz) {
                // Chocolate `movesprite`: no collision within this tic's step.
                step_projectile(lists, sprites, i, dx, dy, dz, &sec_w, &wxy);
                continue;
            }
            if hit.hitsprite >= 0 {
                let target = hit.hitsprite as usize;
                let freeze_immune = source.picnum == FREEZEBLAST
                    && target < sprites.len()
                    && sprites[target].pal == 1
                    && (sprites[target].picnum == APLAYER
                        || crate::badguy::badguy_pic(
                            sprites[target].picnum,
                            script.map(|img| img.actortype.as_slice()).unwrap_or(&[]),
                        ));
                if freeze_immune {
                    let star = crate::shoot::egs(
                        lists,
                        sprites,
                        picnum,
                        actor_temps,
                        script,
                        hit.hitsect,
                        hit.hitx,
                        hit.hity,
                        hit.hitz,
                        TRANSPORTERSTAR,
                        0,
                        32,
                        32,
                        source.ang,
                        0,
                        0,
                        i,
                        crate::stat::STAT_MISC,
                    );
                    if star >= 0 {
                        sprites[star as usize].pal = 1;
                    }
                    let _ = lists.deletesprite(i);
                    continue;
                }
                // Chocolate `moveweapons` plays the shared body-impact cue
                // after `checkhitsprite` for every accepted projectile hit on
                // APLAYER. SPIT and FIRELASER use their earlier specialized
                // paths, so this covers RPG, FREEZEBLAST and SHRINKSPARK here.
                let object_handled = target < sprites.len()
                    && checkhitsprite_object(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        actor_temps,
                        picnum,
                        randomseed,
                        script,
                        tile_meta,
                        players,
                        target,
                        i,
                    );
                if !object_handled && target < sprites.len() && sprites[target].picnum == APLAYER {
                    if actor_temps.len() <= target {
                        actor_temps.resize_with(target + 1, ActorConTemps::default);
                    }
                    actor_temps[target]
                        .pending_actor_sounds
                        .push(i16::try_from(PISTOL_BODYHIT).unwrap_or_default());
                }
                if source.picnum == SHRINKSPARK {
                    let impact = SpriteT {
                        x: hit.hitx,
                        y: hit.hity,
                        z: hit.hitz,
                        sectnum: hit.hitsect,
                        ..source
                    };
                    apply_shrinker_blast(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        actor_temps,
                        randomseed,
                        script,
                        impact,
                        source.owner,
                    );
                } else if !object_handled
                    && target < sprites.len()
                    && (target != source.owner as usize
                        || (source.picnum == FREEZEBLAST && freezer_hurts_owner))
                {
                    if actor_temps.len() <= target {
                        actor_temps.resize_with(target + 1, ActorConTemps::default);
                    }
                    let is_badguy = crate::badguy::badguy_pic(
                        sprites[target].picnum,
                        script.map_or(&[], |image| image.actortype.as_slice()),
                    );
                    // Chocolate `checkhitsprite` accumulates projectile extra
                    // for every accepted target and doubles RPG damage against
                    // badguy actors before writing hittype[].extra.
                    let hit_damage = if source.picnum == rpg_tile && is_badguy {
                        source.extra.saturating_mul(2)
                    } else {
                        source.extra
                    };
                    actor_temps[target].hit_picnum = source.picnum;
                    actor_temps[target].hit_extra = if actor_temps[target].hit_extra < 0 {
                        hit_damage
                    } else {
                        actor_temps[target].hit_extra.saturating_add(hit_damage)
                    };
                    actor_temps[target].hit_owner = source.owner;
                    actor_temps[target].hit_source = i;
                    actor_temps[target].hit_radius = crate::con::direct_hit_payload();
                    actor_temps[target].hit_ang = source.ang;
                    if is_badguy {
                        wake_hit_badguy(lists, actor_temps, target);
                    }
                }
                let _ = lists.deletesprite(i);
            } else if hit.hitwall >= 0 {
                if source.picnum == FREEZEBLAST {
                    let wi = hit.hitwall as usize;
                    let wall_sounds = apply_checkhitwall(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        picnum,
                        actor_temps,
                        randomseed,
                        source,
                        i,
                        source.picnum,
                        wi,
                        hit.hitx,
                        hit.hity,
                        hit.hitz,
                        hit.hitsect,
                        script,
                    );
                    if actor_temps.len() <= iu {
                        actor_temps.resize_with(iu + 1, ActorConTemps::default);
                    }
                    actor_temps[iu].pending_actor_sounds.extend(wall_sounds);
                    let mirror = walls
                        .get(wi)
                        .is_some_and(|w| w.picnum == 560 || w.overpicnum == 560);
                    if let Some(wall) = walls.get(wi).copied() {
                        if let Some(end) = walls.get(wall.point2.max(0) as usize) {
                            let wall_angle = getangle(end.x - wall.x, end.y - wall.y);
                            sprites[iu].ang =
                                ((wall_angle * 2 - i32::from(source.ang)) & 2047) as i16;
                        }
                    }
                    sprites[iu].x = hit.hitx;
                    sprites[iu].y = hit.hity;
                    sprites[iu].z = hit.hitz;
                    sprites[iu].sectnum = hit.hitsect;
                    if !mirror {
                        sprites[iu].extra >>= 1;
                        sprites[iu].yvel = sprites[iu].yvel.saturating_sub(1);
                    }
                } else {
                    let impact = SpriteT {
                        x: hit.hitx,
                        y: hit.hity,
                        z: hit.hitz,
                        sectnum: hit.hitsect,
                        ..source
                    };
                    apply_shrinker_blast(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        actor_temps,
                        randomseed,
                        script,
                        impact,
                        source.owner,
                    );
                    let _ = lists.deletesprite(i);
                }
            } else if hit.hitsect < 0 {
                let _ = lists.deletesprite(i);
            } else if hit.hitsprite < 0 && source.picnum == SHRINKSPARK {
                let impact = SpriteT {
                    x: hit.hitx,
                    y: hit.hity,
                    z: hit.hitz,
                    sectnum: hit.hitsect,
                    ..source
                };
                apply_shrinker_blast(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    actor_temps,
                    randomseed,
                    script,
                    impact,
                    source.owner,
                );
                let _ = lists.deletesprite(i);
            } else if hit.hitsprite < 0 && source.picnum == FREEZEBLAST {
                sprites[iu].x = hit.hitx;
                sprites[iu].y = hit.hity;
                sprites[iu].z = hit.hitz;
                sprites[iu].sectnum = hit.hitsect;
                bounce_freezeblast(&mut sprites[iu], sectors, walls, &sec_z, &wxy);
                sprites[iu].extra >>= 1;
                sprites[iu].yvel = sprites[iu].yvel.saturating_sub(1);
                sprites[iu].xrepeat = sprites[iu].xrepeat.saturating_sub(2).max(8);
                sprites[iu].yrepeat = sprites[iu].yrepeat.saturating_sub(2).max(8);
            } else {
                sprites[iu].x = hit.hitx;
                sprites[iu].y = hit.hity;
                sprites[iu].z = hit.hitz;
                sprites[iu].sectnum = hit.hitsect;
            }
            continue;
        }
        if (GROWSPARK..=GROWSPARK + 3).contains(&sprites[iu].picnum) {
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            actor_temps[iu].g_t[0] = (actor_temps[iu].g_t[0] + 1) & 7;
            let frame = i16::try_from(actor_temps[iu].g_t[0] / 2).unwrap_or(0);
            sprites[iu].picnum = GROWSPARK + frame;
            if picnum.len() <= iu {
                picnum.resize(iu + 1, 0);
            }
            picnum[iu] = sprites[iu].picnum;
            continue;
        }
        if sprites[iu].picnum != SHOTSPARK1 {
            continue;
        }
        let Some(script) = script else {
            continue;
        };
        let pn = SHOTSPARK1.max(0) as usize;
        if pn >= script.actorscrptr.len() || script.actorscrptr[pn] <= 0 {
            continue;
        }
        let entry = (script.actorscrptr[pn] as usize).saturating_add(4);
        if actor_temps.len() <= iu {
            actor_temps.resize_with(iu + 1, ActorConTemps::default);
        }
        actor_temps[iu].pending_spawns.clear();
        actor_temps[iu].pending_spawn_guts_before.clear();
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
            &[],
            &[],
            &mut [],
            None,
            None,
        );
        // CON `spawn WATERBUBBLE` (`ifactioncount 3` + `ifinwater`) and
        // `spawn WATERSPLASH2` (`ifcount 2 nullop else ifonwater`).
        let spawns: Vec<i16> = actor_temps[iu].pending_spawns.drain(..).collect();
        actor_temps[iu].pending_spawn_guts_before.clear();
        for pn in spawns {
            if pn == WATERBUBBLE {
                let _ = spawn_maker_bubble(lists, sprites, picnum, actor_temps, i, Some(script));
            } else if pn == crate::shoot::script_tile_or(Some(script), "WATERSPLASH2", 1380) {
                let _ = crate::transports::spawn_water_splash2(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum,
                    actor_temps,
                    randomseed,
                    i,
                    Some(script),
                );
            }
        }
        if stop == ConStop::KillIt {
            let _ = lists.deletesprite(i);
        } else if lists.sprite_stat.get(iu).copied() == Some(STAT_PROJECTILE) {
            // Chocolate `execute` then `move()`, whose first step is
            // `g_t[0]++`: the spark's `ifcount 2` splash gate counts tics.
            actor_temps[iu].g_t[0] = actor_temps[iu].g_t[0].wrapping_add(1);
        }
    }
}

/// Reflect a FREEZEBLAST from a Build ceiling/floor plane using Chocolate's
/// `actors.c::bounce`. The plane is selected from the projectile's position
/// relative to the sector's vertical midpoint, which also handles slopes.
fn bounce_freezeblast(
    sprite: &mut SpriteT,
    sectors: &[SectorT],
    walls: &[WallT],
    sector_z: &[SectorZ],
    wall_xy: &[WallXY],
) {
    let Some(sector) = usize::try_from(sprite.sectnum)
        .ok()
        .and_then(|index| sectors.get(index))
    else {
        sprite.zvel = sprite.zvel.saturating_neg();
        return;
    };
    let Some(wall) = usize::try_from(sector.wallptr)
        .ok()
        .and_then(|index| walls.get(index))
    else {
        sprite.zvel = sprite.zvel.saturating_neg();
        return;
    };
    let Some(edge) = usize::try_from(wall.point2)
        .ok()
        .and_then(|index| walls.get(index))
    else {
        sprite.zvel = sprite.zvel.saturating_neg();
        return;
    };

    let wall_angle = getangle(edge.x.wrapping_sub(wall.x), edge.y.wrapping_sub(wall.y));
    let (ceiling_z, floor_z) = getzsofslope(sprite.sectnum, sprite.x, sprite.y, sector_z, wall_xy);
    let heinum = if sprite.z < ceiling_z.wrapping_add(floor_z) >> 1 {
        i32::from(sector.ceilingheinum)
    } else {
        i32::from(sector.floorheinum)
    };
    let (sin_x, _) = room_build_engine::helpers::sincos_q14(wall_angle);
    let (sin_y, _) = room_build_engine::helpers::sincos_q14(wall_angle + 1536);
    let dax = mulscale(heinum, sin_x, 14);
    let day = mulscale(heinum, sin_y, 14);
    let daz = 4096;

    let (sin_xv, _) = room_build_engine::helpers::sincos_q14(i32::from(sprite.ang) + 512);
    let (sin_yv, _) = room_build_engine::helpers::sincos_q14(i32::from(sprite.ang));
    let mut xvect = mulscale(i32::from(sprite.xvel), sin_xv, 10);
    let mut yvect = mulscale(i32::from(sprite.xvel), sin_yv, 10);
    let mut zvect = i32::from(sprite.zvel);
    let projection = xvect
        .wrapping_mul(dax)
        .wrapping_add(yvect.wrapping_mul(day))
        .wrapping_add(zvect.wrapping_mul(daz));
    let norm2 = dax
        .wrapping_mul(dax)
        .wrapping_add(day.wrapping_mul(day))
        .wrapping_add(daz.wrapping_mul(daz));
    if (projection.unsigned_abs() >> 14) < norm2 as u32 {
        let ratio = divscale(projection, norm2, 17);
        xvect = xvect.wrapping_sub(mulscale(dax, ratio, 16));
        yvect = yvect.wrapping_sub(mulscale(day, ratio, 16));
        zvect = zvect.wrapping_sub(mulscale(daz, ratio, 16));
    }

    sprite.zvel = zvect.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    let speed = ksqrt(dmulscale(xvect, xvect, yvect, yvect, 8));
    sprite.xvel = speed.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    sprite.ang = getangle(xvect, yvect) as i16;
}

/// Apply the player-facing portion of Chocolate's `checkhitsprite` for a
/// `SPIT` hit, and its common body-hit cue for other hostile projectiles.
pub(crate) fn apply_spit_player_effects(
    player: &mut Player,
    actor_temps: &mut Vec<ActorConTemps>,
    target_sprite: i16,
    randomseed: &mut i32,
) {
    let Ok(target_index) = usize::try_from(target_sprite) else {
        return;
    };
    if actor_temps.len() <= target_index {
        actor_temps.resize_with(target_index + 1, ActorConTemps::default);
    }
    actor_temps[target_index]
        .pending_actor_sounds
        .push(i16::try_from(PISTOL_BODYHIT).unwrap_or_default());
    player.horiz = player.horiz.wrapping_add(32);
    player.return_to_center = 8;
    player.palette_flash = [0, 32, 0];
    player.palette_flash_tics = 32;
    if player.loogie_tics != 0 {
        return;
    }
    actor_temps[target_index]
        .pending_actor_sounds
        .push(i16::try_from(DUKE_LONGTERM_PAIN).unwrap_or_default());
    let count = 3 + (krand_step(randomseed) & 3) as u8;
    player.num_loogies = count;
    player.loogie_tics = 24 * 4;
    for index in 0..usize::from(count) {
        player.loogie_x[index] = (krand_step(randomseed) % 320) as i16;
        player.loogie_y[index] = (krand_step(randomseed) % 200) as i16;
    }
}

/// Apply the SHRINKSPARK part of Chocolate `hitradius`: no health damage, but
/// visible eligible targets receive the shrink weapon marker for their CON.
fn apply_shrinker_blast(
    lists: &SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    script: Option<&ScriptImage>,
    source: SpriteT,
    owner: i16,
) {
    let radius_index = script.map(|image| {
        if image.con_version == crate::con::ConVersion::V15 {
            10
        } else {
            9
        }
    });
    let radius = radius_index
        .and_then(|index| script.and_then(|image| image.gamestartup.get(index).copied()))
        .unwrap_or(0);
    if radius <= 0 || source.sectnum < 0 {
        return;
    }
    let q = -(16 << 8) + (krand_step(randomseed) & ((32 << 8) - 1));
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let actortype = script
        .map(|image| image.actortype.as_slice())
        .unwrap_or(&[]);
    let tripbomb_tile = script
        .and_then(|image| image.labels.get("TRIPBOMB"))
        .filter(|label| !label.is_ptr)
        .map_or(2566, |label| label.value as i16);
    // Chocolate `hitradius` scans its fixed STAT list in this order. SHRINKSPARK
    // treats the first and last groups differently: simple blocking/flamable
    // sprites go through checkhitsprite, while actors/players/standables use
    // the shootable-target LOS and owner/size filters.
    let stat_order = [
        crate::stat::STAT_DEFAULT,
        crate::stat::STAT_ACTOR,
        crate::stat::STAT_STANDABLE,
        crate::stat::STAT_PLAYER,
        crate::stat::STAT_FALLER,
        crate::stat::STAT_DUMMY,
        crate::stat::STAT_MISC,
    ];
    for (group, stat) in stat_order.into_iter().enumerate() {
        let mut target = lists.headspritestat[stat as usize];
        while target >= 0 {
            let next = lists.nextspritestat[target as usize];
            let ti = target as usize;
            if target != owner && ti < sprites.len() {
                let target_sprite = sprites[ti];
                let badguy = crate::badguy::badguy_pic(target_sprite.picnum, actortype);
                let flammable = matches!(
                    target_sprite.picnum,
                    crate::spawn::BOX
                        | crate::spawn::TREE1
                        | crate::spawn::TREE2
                        | crate::spawn::TIRE
                        | crate::spawn::CONE
                );
                let mut qualifies = false;
                if group == 0 || group >= 5 || flammable {
                    // Chocolate's `x == 0 || x >= 5 || AFLAMABLE` branch only
                    // offers SHRINKSPARK to blocking sprites. Badguys also
                    // need line of sight; ordinary breakables do not.
                    if target_sprite.cstat & 257 != 0 {
                        let distance = room_build_engine::numeric::find_distance_3d(
                            source.x.wrapping_sub(target_sprite.x),
                            source.y.wrapping_sub(target_sprite.y),
                            source.z.wrapping_sub(target_sprite.z) >> 4,
                        );
                        qualifies = distance < radius
                            && (!badguy
                                || cansee(
                                    target_sprite.x,
                                    target_sprite.y,
                                    target_sprite.z + q,
                                    target_sprite.sectnum,
                                    source.x,
                                    source.y,
                                    source.z + q,
                                    source.sectnum,
                                    &sec_z,
                                    &wall_xy,
                                ));
                    }
                } else if target_sprite.extra >= 0
                    && (target_sprite.picnum == tripbomb_tile
                        || badguy
                        || matches!(target_sprite.picnum, 901 | 902)
                        || target_sprite.cstat & 257 != 0
                        || target_sprite.picnum == 1518)
                    && (target_sprite.picnum == 1550
                        || (target != owner && target_sprite.xrepeat >= 24))
                {
                    // Chocolate's `dist()` measures APLAYER from its feet.
                    let target_z = if target_sprite.picnum == crate::spawn::APLAYER {
                        target_sprite.z.wrapping_sub(crate::player::PHEIGHT)
                    } else {
                        target_sprite.z
                    };
                    let distance = room_build_engine::numeric::find_distance_3d(
                        source.x.wrapping_sub(target_sprite.x),
                        source.y.wrapping_sub(target_sprite.y),
                        source.z.wrapping_sub(target_z) >> 4,
                    );
                    qualifies = distance < radius
                        && cansee(
                            target_sprite.x,
                            target_sprite.y,
                            target_sprite.z - (8 << 8),
                            target_sprite.sectnum,
                            source.x,
                            source.y,
                            source.z - (12 << 8),
                            source.sectnum,
                            &sec_z,
                            &wall_xy,
                        );
                }
                if qualifies {
                    if actor_temps.len() <= ti {
                        actor_temps.resize_with(ti + 1, ActorConTemps::default);
                    }
                    actor_temps[ti].hit_picnum = SHRINKSPARK;
                    actor_temps[ti].hit_extra = 0;
                    actor_temps[ti].hit_owner = owner;
                    actor_temps[ti].hit_ang = getangle(
                        target_sprite.x.wrapping_sub(source.x),
                        target_sprite.y.wrapping_sub(source.y),
                    ) as i16;
                }
            }
            target = next;
        }
    }
}

/// Arm hitag-linked SEENINE/OOZFILTER groups when a radius blast reaches a
/// tagged Atomic crack or fire extinguisher standable.
fn arm_tagged_detonables_from_blast(
    lists: &SpriteLists,
    sprites: &mut [SpriteT],
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    x: i32,
    y: i32,
    z: i32,
    radius: i32,
) {
    let crack1_tile = script_tile(script, "CRACK1").unwrap_or(546);
    let fire_ext_tile = script_tile(script, "FIREEXT").unwrap_or(916);
    let seenine_tile = script_tile(script, "SEENINE").unwrap_or(1247);
    let oozfilter_tile = script_tile(script, "OOZFILTER").unwrap_or(1079);
    let mut target = lists.headspritestat[crate::stat::STAT_STANDABLE as usize];
    while target >= 0 {
        let next = lists.nextspritestat[target as usize];
        let index = target as usize;
        let Some(sprite) = sprites.get(index).copied() else {
            target = next;
            continue;
        };
        let tagged_breakable = (crack1_tile..=crack1_tile.saturating_add(3))
            .contains(&sprite.picnum)
            || sprite.picnum == fire_ext_tile;
        let dist = room_build_engine::numeric::find_distance_3d(
            x.wrapping_sub(sprite.x),
            y.wrapping_sub(sprite.y),
            z.wrapping_sub(sprite.z) >> 4,
        );
        if tagged_breakable && sprite.extra >= 0 && (sprite.cstat & 257) != 0 && dist < radius {
            if actor_temps.len() <= index {
                actor_temps.resize_with(index + 1, ActorConTemps::default);
            }
            let temp = actor_temps.get_mut(index);
            if sprite.picnum == fire_ext_tile || sprite.hitag > 0 {
                if let Some(temp) = temp {
                    if temp.hit_extra < 0 {
                        temp.hit_extra = 1;
                        temp.hit_picnum = if sprite.picnum == fire_ext_tile {
                            fire_ext_tile
                        } else {
                            RADIUSEXPLOSION
                        };
                    }
                }
            }
            let hitag = sprite.hitag;
            if hitag > 0 {
                let mut linked = lists.headspritestat[crate::stat::STAT_STANDABLE as usize];
                while linked >= 0 {
                    let next_linked = lists.nextspritestat[linked as usize];
                    let linked_index = linked as usize;
                    if sprites.get(linked_index).is_some_and(|candidate| {
                        candidate.hitag == hitag
                            && (candidate.picnum == seenine_tile
                                || candidate.picnum == oozfilter_tile)
                            && candidate.shade != -32
                    }) {
                        sprites[linked_index].shade = -32;
                    }
                    linked = next_linked;
                }
            }
        }
        target = next;
    }
}

/// Queue RPG blast hits for eligible sprites. Atomic's RPG CON strength and
/// RPGBLASTRADIUS are content data; damage is selected from the three vanilla
/// distance bands.
/// The target CON consumes `hit_extra` through its normal `ifhitweapon` path.
/// Chocolate `movesprite` for a STAT_PROJECTILE sprite (non-badguy, statnum
/// 4): `clipmove` with wall distance 8 at `z - height`, by the per-tic step
/// of `(xchange, ychange)`, then the Z step when it stays between the planes
/// `getglobalz` cached before the move. Returns Build's code: 0, `16384 |
/// sector` (plane), `32768 | wall`, `49152 | sprite`.
#[allow(clippy::too_many_arguments)]
fn projectile_movesprite(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    i: i16,
    xchange: i32,
    ychange: i32,
    zchange: i32,
    cliptype: u32,
    (ceilz, florz): (i32, i32),
    sec_w: &[SectorWalls],
    wxy: &[WallXY],
    sec_z: &[SectorZ],
    tile_meta: &[TileZMeta],
) -> i32 {
    let iu = i as usize;
    let me = sprites[iu];
    let height = tile_meta
        .get(me.picnum.max(0) as usize)
        .map_or(0, |m| i32::from(m.height));
    let clip_z = me.z - ((height * i32::from(me.yrepeat)) << 1);
    let mut sprite_clips: Vec<_> =
        crate::player::live_sprite_z_clips(sprites, &lists.sprite_stat, tile_meta);
    if let Some(own) = sprite_clips.get_mut(iu) {
        own.cstat = 0;
    }
    let (mut nx, mut ny, mut nsect) = (me.x, me.y, me.sectnum);
    let retval = room_build_engine::geom::clipmove_xy_at_z_cliptype(
        &mut nx,
        &mut ny,
        clip_z,
        &mut nsect,
        (xchange * TICSPERFRAME) >> 3,
        (ychange * TICSPERFRAME) >> 3,
        8,
        4 << 8,
        4 << 8,
        sec_w,
        wxy,
        Some(sec_z),
        &sprite_clips,
        cliptype,
        Some(&*lists),
    );
    sprites[iu].x = nx;
    sprites[iu].y = ny;
    if nsect >= 0 && nsect != me.sectnum {
        let _ = lists.changespritesect(i, nsect);
        sprites[iu].sectnum = nsect;
    }
    let daz = sprites[iu].z + ((zchange * TICSPERFRAME) >> 3);
    if daz > ceilz && daz <= florz {
        sprites[iu].z = daz;
    } else if retval == 0 {
        return 16384 + i32::from(nsect);
    }
    retval
}

/// Chocolate `moveweapons` for `FIRELASER` (RECON / trooper lasers) and
/// `SPIT` (lizmen): `movesprite` (CLIPMASK1, walldist 8) against the cached
/// `getglobalz` planes, then the laser's five-sprite trail at its *new*
/// place or the spit's gravity, then on a hit `checkhitsprite` (spit also
/// blinds the player) / laser mirror bounce / `checkhitwall` /
/// `checkhitceiling`. Neither makes an explosion.
#[allow(clippy::too_many_arguments)]
fn move_vanilla_firelaser(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    players: &mut [Player],
    randomseed: &mut i32,
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&ScriptImage>,
    tile_meta: &[TileZMeta],
    i: i16,
    sec_w: &[SectorWalls],
    wxy: &[WallXY],
    sec_z: &[SectorZ],
) {
    let iu = i as usize;
    let s = sprites[iu];
    let lotag = |sectors: &[SectorT], sect: i16| {
        usize::try_from(sect)
            .ok()
            .and_then(|k| sectors.get(k))
            .map_or(0, |sector| sector.lotag)
    };
    let (dax, day, daz) = (s.x, s.y, s.z);
    let sprite_clips: Vec<_> =
        crate::player::live_sprite_z_clips(sprites, &lists.sprite_stat, tile_meta);
    let planes = projectile_globalz(&s, sectors, sec_z, wxy, &sprite_clips, sprites);
    let k = i32::from(s.xvel);
    let (sin, cos) = sincos_q14(i32::from(s.ang) & 2047);
    let mut j = projectile_movesprite(
        lists,
        sprites,
        i,
        (k * cos) >> 14,
        (k * sin) >> 14,
        i32::from(s.zvel),
        (256 << 16) | 64,
        planes,
        sec_w,
        wxy,
        sec_z,
        tile_meta,
    );
    let s = sprites[iu];
    if s.sectnum < 0 {
        let _ = lists.deletesprite(i);
        return;
    }
    if j & 49152 != 49152 {
        let (ceilz, florz) = planes;
        let on_water = lotag(sectors, s.sectnum) == 1;
        if s.z < ceilz {
            j = 16384 | i32::from(s.sectnum);
            sprites[iu].zvel = -1;
        } else if (s.z > florz && !on_water) || (s.z > florz + (16 << 8) && on_water) {
            j = 16384 | i32::from(s.sectnum);
            if !on_water {
                sprites[iu].zvel = 1;
            }
        }
    }
    // The trail sits around the beam's new position, after the plane check
    // may have set `zvel` to -1 / 1. Spit falls instead.
    let beam = sprites[iu];
    let is_spit = beam.picnum == SPIT;
    if is_spit && sprites[iu].zvel < 6144 {
        sprites[iu].zvel = sprites[iu].zvel.saturating_add(176 - 112); // `gc - 112`
    }
    let (sin, cos) = sincos_q14(i32::from(beam.ang) & 2047);
    let zv = i32::from(beam.zvel);
    for k in (-3_i32..2).filter(|_| !is_spit) {
        let trail = egs(
            lists,
            sprites,
            picnum,
            actor_temps,
            script,
            beam.sectnum,
            beam.x + ((k * cos) >> 9),
            beam.y + ((k * sin) >> 9),
            beam.z + ((k * zv.signum()) * (zv / 24).abs()),
            FIRELASER,
            (-40 + (k << 2)) as i8,
            beam.xrepeat,
            beam.yrepeat,
            0,
            0,
            0,
            beam.owner,
            crate::stat::STAT_MISC,
        );
        if trail >= 0 {
            let trail = trail as usize;
            sprites[trail].cstat = 128;
            sprites[trail].pal = beam.pal;
        }
    }
    if j == 0 {
        return;
    }
    if j & 49152 == 49152 {
        let target = (j & 4095) as usize;
        // `checkhitsprite(j, i)`: object cases, or the default branch (a
        // badguy bleeds JIBS6, is knocked back and woken; the hit is queued).
        if target < sprites.len() {
            let actortype: Vec<u8> = script.map_or_else(Vec::new, |image| image.actortype.clone());
            crate::shoot::checkhitsprite_shotgun(
                lists,
                sprites,
                sectors,
                walls,
                players,
                picnum,
                actor_temps,
                tile_meta,
                script,
                &actortype,
                randomseed,
                target as i16,
                i,
                false,
            );
            if sprites[target].picnum == APLAYER {
                if actor_temps.len() <= target {
                    actor_temps.resize_with(target + 1, ActorConTemps::default);
                }
                actor_temps[target]
                    .pending_actor_sounds
                    .push(i16::try_from(PISTOL_BODYHIT).unwrap_or_default());
                if is_spit
                    && let Some(player) = players
                        .iter_mut()
                        .find(|player| player.sprite_i == target as i16)
                {
                    apply_spit_player_effects(player, actor_temps, target as i16, randomseed);
                }
            }
        }
    } else if j & 49152 == 32768 {
        let wall = (j & 32767) as usize;
        let mirror = script_tile(script, "MIRROR").unwrap_or(560);
        if !is_spit
            && walls
                .get(wall)
                .is_some_and(|w| w.overpicnum == mirror || w.picnum == mirror)
        {
            // Mirrors reflect the beam, which now owns itself.
            let w = walls[wall];
            let p2 = walls[w.point2.max(0) as usize];
            let wall_ang = getangle(p2.x - w.x, p2.y - w.y);
            sprites[iu].ang = (((wall_ang << 1) - i32::from(sprites[iu].ang)) & 2047) as i16;
            sprites[iu].owner = i;
            if let Some(image) = script {
                let _ = crate::spawn_rt::spawn_from(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum,
                    actor_temps,
                    tile_meta,
                    image,
                    randomseed,
                    i,
                    TRANSPORTERSTAR,
                    None,
                );
            }
            return;
        }
        setsprite_xyz(lists, sprites, i, (dax, day, daz), sec_w, wxy);
        let here = sprites[iu];
        let sounds = apply_checkhitwall(
            lists,
            sprites,
            sectors,
            walls,
            picnum,
            actor_temps,
            randomseed,
            here,
            i,
            here.picnum,
            wall,
            here.x,
            here.y,
            here.z,
            here.sectnum,
            script,
        );
        if !sounds.is_empty() {
            if actor_temps.len() <= iu {
                actor_temps.resize_with(iu + 1, ActorConTemps::default);
            }
            actor_temps[iu].pending_actor_sounds.extend(sounds);
        }
    } else {
        setsprite_xyz(lists, sprites, i, (dax, day, daz), sec_w, wxy);
        if sprites[iu].zvel < 0 {
            let sect = sprites[iu].sectnum;
            if usize::try_from(sect)
                .ok()
                .and_then(|k| sectors.get(k))
                .is_some_and(|sector| sector.ceilingstat & 1 != 0 && sector.ceilingpal == 0)
            {
                let _ = lists.deletesprite(i);
                return;
            }
            let mut ceiling_sounds = Vec::new();
            crate::shoot::check_hit_ceiling(
                lists,
                sprites,
                sectors,
                walls,
                picnum,
                actor_temps,
                script,
                randomseed,
                sect,
                i,
                sprites[iu].owner,
                &mut ceiling_sounds,
            );
        }
    }
    let _ = lists.deletesprite(i);
}

/// Build `setsprite(i, x, y, z)`: move the sprite and re-find its sector.
fn setsprite_xyz(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    i: i16,
    (x, y, z): (i32, i32, i32),
    sec_w: &[SectorWalls],
    wxy: &[WallXY],
) {
    let iu = i as usize;
    sprites[iu].x = x;
    sprites[iu].y = y;
    sprites[iu].z = z;
    let mut sect = sprites[iu].sectnum;
    room_build_engine::geom::updatesector(x, y, &mut sect, sec_w, wxy);
    if sect >= 0 && sect != sprites[iu].sectnum {
        let _ = lists.changespritesect(i, sect);
        sprites[iu].sectnum = sect;
    }
}

/// Chocolate `moveweapons` for the vanilla `RPG` rocket: smoke trail,
/// `movesprite` (CLIPMASK1), the homing proximity hit, plane checks against
/// the cached `getglobalz` planes, then on impact `checkhitsprite` /
/// `checkhitwall` / `checkhitceiling`, a spawned `EXPLOSION2` (plus
/// `EXPLOSION2BOT`), `RPG_EXPLODE`, `hitradius`, and the rocket is deleted.
#[allow(clippy::too_many_arguments)]
fn move_vanilla_rpg(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    players: &mut [Player],
    randomseed: &mut i32,
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&ScriptImage>,
    tile_meta: &[TileZMeta],
    i: i16,
    sec_w: &[SectorWalls],
    wxy: &[WallXY],
    sec_z: &[SectorZ],
) {
    let iu = i as usize;
    let tile = |name: &str, fallback: i16| script_tile(script, name).unwrap_or(fallback);
    let s = sprites[iu];
    let lotag = |sect: i16| {
        usize::try_from(sect)
            .ok()
            .and_then(|k| sectors.get(k))
            .map_or(0, |sector| sector.lotag)
    };
    let underwater = lotag(s.sectnum) == 2;
    let (k, ll) = if underwater {
        (i32::from(s.xvel) >> 1, i32::from(s.zvel) >> 1)
    } else {
        (i32::from(s.xvel), i32::from(s.zvel))
    };
    let (dax, day, daz) = (s.x, s.y, s.z);
    // `getglobalz(i)` at the old position, before the smoke spawns.
    let sprite_clips: Vec<_> =
        crate::player::live_sprite_z_clips(sprites, &lists.sprite_stat, tile_meta);
    let planes = projectile_globalz(&s, sectors, sec_z, wxy, &sprite_clips, sprites);
    if actor_temps.len() <= iu {
        actor_temps.resize_with(iu + 1, ActorConTemps::default);
    }
    if actor_temps[iu].hit_picnum != tile("BOSS2", 2710) && s.xrepeat >= 10 && !underwater {
        if let Some(image) = script {
            let smoke = crate::spawn_rt::spawn_from(
                lists,
                sprites,
                sectors,
                walls,
                picnum,
                actor_temps,
                tile_meta,
                image,
                randomseed,
                i,
                tile("SMALLSMOKE", 1060),
                None,
            );
            if let Some(sp) = usize::try_from(smoke).ok().and_then(|k| sprites.get_mut(k)) {
                sp.z += 1 << 8;
            }
        }
    }
    let (sin, cos) = sincos_q14(i32::from(s.ang) & 2047);
    let mut j = projectile_movesprite(
        lists,
        sprites,
        i,
        (k * cos) >> 14,
        (k * sin) >> 14,
        ll,
        (256 << 16) | 64,
        planes,
        sec_w,
        wxy,
        sec_z,
        tile_meta,
    );
    let s = sprites[iu];
    if s.yvel >= 0
        && let Some(target) = sprites.get(s.yvel as usize)
        && crate::actors::find_distance_2d(s.x - target.x, s.y - target.y) < 256
    {
        j = 49152 | i32::from(s.yvel);
    }
    if s.sectnum < 0 {
        let _ = lists.deletesprite(i);
        return;
    }
    if j & 49152 != 49152 {
        let (ceilz, florz) = planes;
        let on_water = lotag(s.sectnum) == 1;
        if s.z < ceilz {
            j = 16384 | i32::from(s.sectnum);
            sprites[iu].zvel = -1;
        } else if (s.z > florz && !on_water) || (s.z > florz + (16 << 8) && on_water) {
            j = 16384 | i32::from(s.sectnum);
            if !on_water {
                sprites[iu].zvel = 1;
            }
        }
    }
    if j == 0 {
        // No hit: a full-size rocket under water leaves bubbles (`rnd(140)`).
        if lotag(sprites[iu].sectnum) == 2
            && sprites[iu].xrepeat >= 10
            && (krand_step(randomseed) >> 8) >= 255 - 140
            && let Some(image) = script
        {
            let _ = crate::spawn_rt::spawn_from(
                lists,
                sprites,
                sectors,
                walls,
                picnum,
                actor_temps,
                tile_meta,
                image,
                randomseed,
                i,
                WATERBUBBLE,
                None,
            );
        }
        return;
    }
    let mut sounds: Vec<i16> = Vec::new();
    if j & 49152 == 49152 {
        let target = (j & 4095) as usize;
        // `checkhitsprite(j, i)`: object cases, or the default branch (a
        // badguy doubles the rocket's extra, gets knocked back and woken).
        if target < sprites.len() {
            let actortype: Vec<u8> = script.map_or_else(Vec::new, |image| image.actortype.clone());
            crate::shoot::checkhitsprite_shotgun(
                lists,
                sprites,
                sectors,
                walls,
                players,
                picnum,
                actor_temps,
                tile_meta,
                script,
                &actortype,
                randomseed,
                target as i16,
                i,
                false,
            );
            if sprites[target].picnum == APLAYER {
                if actor_temps.len() <= target {
                    actor_temps.resize_with(target + 1, ActorConTemps::default);
                }
                actor_temps[target]
                    .pending_actor_sounds
                    .push(i16::try_from(PISTOL_BODYHIT).unwrap_or_default());
            }
        }
    } else if j & 49152 == 32768 {
        // The rocket never mirror-bounces: back to where it was, then `checkhitwall`.
        let wall = (j & 32767) as usize;
        setsprite_xyz(lists, sprites, i, (dax, day, daz), sec_w, wxy);
        let here = sprites[iu];
        sounds.extend(apply_checkhitwall(
            lists,
            sprites,
            sectors,
            walls,
            picnum,
            actor_temps,
            randomseed,
            here,
            i,
            here.picnum,
            wall,
            here.x,
            here.y,
            here.z,
            here.sectnum,
            script,
        ));
    } else {
        setsprite_xyz(lists, sprites, i, (dax, day, daz), sec_w, wxy);
        if sprites[iu].zvel < 0 {
            let sect = sprites[iu].sectnum;
            if usize::try_from(sect)
                .ok()
                .and_then(|k| sectors.get(k))
                .is_some_and(|sector| sector.ceilingstat & 1 != 0 && sector.ceilingpal == 0)
            {
                let _ = lists.deletesprite(i);
                return;
            }
            let mut ceiling_sounds = Vec::new();
            crate::shoot::check_hit_ceiling(
                lists,
                sprites,
                sectors,
                walls,
                picnum,
                actor_temps,
                script,
                randomseed,
                sect,
                i,
                sprites[iu].owner,
                &mut ceiling_sounds,
            );
            sounds.extend(
                ceiling_sounds
                    .into_iter()
                    .map(|id| i16::try_from(id).unwrap_or_default()),
            );
        }
    }
    let s = sprites[iu];
    let plane_hit = j & 49152 == 16384;
    let mut boom = -1;
    if let Some(image) = script {
        boom = crate::spawn_rt::spawn_from(
            lists,
            sprites,
            sectors,
            walls,
            picnum,
            actor_temps,
            tile_meta,
            image,
            randomseed,
            i,
            tile("EXPLOSION2", 1890),
            None,
        );
        if let Some(e) = usize::try_from(boom).ok().filter(|&k| k < sprites.len()) {
            sprites[e].x = dax;
            sprites[e].y = day;
            sprites[e].z = daz;
            if s.xrepeat < 10 {
                sprites[e].xrepeat = 6;
                sprites[e].yrepeat = 6;
            } else if plane_hit {
                if s.zvel > 0 {
                    let _ = crate::spawn_rt::spawn_from(
                        lists,
                        sprites,
                        sectors,
                        walls,
                        picnum,
                        actor_temps,
                        tile_meta,
                        image,
                        randomseed,
                        i,
                        tile("EXPLOSION2BOT", 2219),
                        None,
                    );
                } else {
                    sprites[e].cstat |= 8;
                    sprites[e].z += 48 << 8;
                }
            }
        }
    }
    // `spritesound(RPG_EXPLODE, i)`: the rocket goes away, so the sound rides
    // on its explosion (same place), with any wall / ceiling hit sounds.
    sounds.push(9);
    if let Some(e) = usize::try_from(boom).ok() {
        if actor_temps.len() <= e {
            actor_temps.resize_with(e + 1, ActorConTemps::default);
        }
        actor_temps[e].pending_actor_sounds.extend(sounds);
    }
    let (strength, radius) = if s.xrepeat >= 10 {
        (i32::from(s.extra), RPG_BLAST_RADIUS)
    } else {
        // Chocolate adds `global_random & 3`; Room's projectile pass has no
        // `global_random`, and the value only nudges a shrunk rocket's damage.
        (i32::from(s.extra), RPG_BLAST_RADIUS >> 1)
    };
    apply_rpg_blast(
        lists,
        sprites,
        sectors,
        walls,
        actor_temps,
        players,
        randomseed,
        strength,
        radius,
        s.picnum,
        None,
        s,
        i,
        picnum,
        script,
        tile_meta,
        script.map_or(&[], |image| image.actortype.as_slice()),
        s.x,
        s.y,
        s.z,
        s.sectnum,
        s.owner,
    );
    let _ = lists.deletesprite(i);
}

pub(crate) fn apply_rpg_blast(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    actor_temps: &mut Vec<ActorConTemps>,
    players: &mut [Player],
    randomseed: &mut i32,
    strength: i32,
    radius: i32,
    damage_picnum: i16,
    projectile_hit_picnum: Option<i16>,
    wall_source: SpriteT,
    wall_source_i: i16,
    picnum_out: &mut Vec<i16>,
    script: Option<&ScriptImage>,
    tile_meta: &[TileZMeta],
    actortype: &[u8],
    x: i32,
    y: i32,
    z: i32,
    sect: i16,
    owner: i16,
) {
    if radius <= 0 {
        return;
    }
    let wall_sounds = apply_radius_wall_hits(
        lists,
        sprites,
        sectors,
        walls,
        picnum_out,
        actor_temps,
        randomseed,
        wall_source,
        wall_source_i,
        radius,
        players.first().map_or(-1, |player| player.sprite_i),
        script,
    );
    if !wall_sounds.is_empty() {
        let source_index = wall_source_i.max(0) as usize;
        if actor_temps.len() <= source_index {
            actor_temps.resize_with(source_index + 1, ActorConTemps::default);
        }
        actor_temps[source_index]
            .pending_actor_sounds
            .extend(wall_sounds);
    }
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    // `hitradius` consumes one random Z offset before scanning its stat lists.
    let q = -(16 << 8) + (krand_step(randomseed) & ((32 << 8) - 1));
    let standable_damage_tiles = [
        script_tile(script, "SEENINE").unwrap_or(1247),
        script_tile(script, "OOZFILTER").unwrap_or(1079),
    ];
    let mut target = lists.headspritestat[STAT_DEFAULT as usize];
    while target >= 0 {
        let next = lists.nextspritestat[target as usize];
        let ti = target as usize;
        if target != wall_source_i && ti < sprites.len() {
            let t = sprites[ti];
            let badguy = crate::badguy::badguy_pic(t.picnum, actortype);
            let dist = room_build_engine::numeric::find_distance_3d(
                x.wrapping_sub(t.x),
                y.wrapping_sub(t.y),
                z.wrapping_sub(t.z) >> 4,
            );
            if t.extra >= 0
                && (t.cstat & 257) != 0
                && dist < radius
                && (!badguy
                    || cansee(
                        t.x,
                        t.y,
                        t.z + q,
                        t.sectnum,
                        x,
                        y,
                        z + q,
                        sect,
                        &sec_z,
                        &wxy,
                    ))
                && !check_radius_target_object(
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
                    ti,
                    wall_source_i,
                )
            {
                let amount = radius_damage(dist, radius, randomseed, strength);
                queue_radius_sprite_hit(
                    sprites,
                    actor_temps,
                    ti,
                    t,
                    amount,
                    rpg_blast_hit_picnum(t.extra, damage_picnum, script, projectile_hit_picnum),
                    owner,
                    getangle(t.x.wrapping_sub(x), t.y.wrapping_sub(y)) as i16,
                    strength << 2,
                    wall_source_i,
                    rpg_blast_payload(radius, strength),
                );
            }
        }
        target = next;
    }
    let mut target = lists.headspritestat[crate::stat::STAT_ACTOR as usize];
    while target >= 0 {
        let next = lists.nextspritestat[target as usize];
        let ti = target as usize;
        if target != wall_source_i && ti < sprites.len() {
            let t = sprites[ti];
            let badguy = crate::badguy::badguy_pic(t.picnum, actortype);
            let special_target = ["TRIPBOMB", "QUEBALL", "STRIPEBALL", "DUKELYINGDEAD"]
                .iter()
                .filter_map(|name| script_tile(script, name))
                .any(|tile| tile == t.picnum);
            let flammable = is_flammable_radius_target(script, t.picnum);
            let shootable_target =
                t.extra >= 0 && (special_target || badguy || (t.cstat & 257) != 0 || flammable);
            let dist = room_build_engine::numeric::find_distance_3d(
                x.wrapping_sub(t.x),
                y.wrapping_sub(t.y),
                z.wrapping_sub(t.z) >> 4,
            );
            let visible = if flammable {
                !badguy
                    || cansee(
                        t.x,
                        t.y,
                        t.z + q,
                        t.sectnum,
                        x,
                        y,
                        z + q,
                        sect,
                        &sec_z,
                        &wxy,
                    )
            } else {
                cansee(
                    t.x,
                    t.y,
                    t.z - (8 << 8),
                    t.sectnum,
                    x,
                    y,
                    z - (12 << 8),
                    sect,
                    &sec_z,
                    &wxy,
                )
            };
            if shootable_target && dist < radius && visible {
                let damage = radius_damage(dist, radius, randomseed, strength);
                queue_radius_sprite_hit(
                    sprites,
                    actor_temps,
                    ti,
                    t,
                    damage,
                    rpg_blast_hit_picnum(t.extra, damage_picnum, script, projectile_hit_picnum),
                    owner,
                    getangle(t.x.wrapping_sub(x), t.y.wrapping_sub(y)) as i16,
                    strength << 2,
                    wall_source_i,
                    rpg_blast_payload(radius, strength),
                );
                if radius_actor_checkhit_target(script, t.picnum) {
                    let _ = check_radius_target_object(
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
                        ti,
                        wall_source_i,
                    );
                }
            }
        }
        target = next;
    }
    let mut target = lists.headspritestat[crate::stat::STAT_STANDABLE as usize];
    while target >= 0 {
        let next = lists.nextspritestat[target as usize];
        let ti = target as usize;
        if target != wall_source_i && ti < sprites.len() {
            let t = sprites[ti];
            let destructible =
                standable_damage_tiles.contains(&t.picnum) && t.extra >= 0 && (t.cstat & 257) != 0;
            let dist = room_build_engine::numeric::find_distance_3d(
                x.wrapping_sub(t.x),
                y.wrapping_sub(t.y),
                z.wrapping_sub(t.z) >> 4,
            );
            if destructible
                && dist < radius
                && cansee(
                    t.x,
                    t.y,
                    t.z - (8 << 8),
                    t.sectnum,
                    x,
                    y,
                    z - (12 << 8),
                    sect,
                    &sec_z,
                    &wxy,
                )
            {
                let amount = radius_damage(dist, radius, randomseed, strength);
                if actor_temps.len() <= ti {
                    actor_temps.resize_with(ti + 1, ActorConTemps::default);
                }
                let hit_picnum =
                    rpg_blast_hit_picnum(t.extra, damage_picnum, script, projectile_hit_picnum);
                let temp = &mut actor_temps[ti];
                temp.hit_extra = if temp.hit_extra < 0 {
                    amount
                } else {
                    temp.hit_extra.saturating_add(amount)
                };
                temp.hit_picnum = hit_picnum;
                temp.hit_owner = owner;
                temp.hit_ang = getangle(t.x.wrapping_sub(x), t.y.wrapping_sub(y)) as i16;
                sprites[ti].xvel = sprites[ti].xvel.max(0).wrapping_add((strength << 2) as i16);
                if radius_actor_checkhit_target(script, t.picnum) {
                    let _ = check_radius_target_object(
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
                        ti,
                        wall_source_i,
                    );
                }
            }
        }
        target = next;
    }
    arm_tagged_detonables_from_blast(lists, sprites, actor_temps, script, x, y, z, radius);
    let mut target = lists.headspritestat[crate::stat::STAT_PLAYER as usize];
    while target >= 0 {
        let next = lists.nextspritestat[target as usize];
        let ti = target as usize;
        if target != wall_source_i && ti < sprites.len() {
            let body = sprites[ti];
            let feet_z = body.z.wrapping_sub(crate::player::PHEIGHT);
            let dist = room_build_engine::numeric::find_distance_3d(
                x.wrapping_sub(body.x),
                y.wrapping_sub(body.y),
                z.wrapping_sub(feet_z) >> 4,
            );
            if body.extra >= 0
                && dist < radius
                && cansee(
                    body.x,
                    body.y,
                    body.z - (8 << 8),
                    body.sectnum,
                    x,
                    y,
                    z - (12 << 8),
                    sect,
                    &sec_z,
                    &wxy,
                )
            {
                let amount = radius_damage(dist, radius, randomseed, strength);
                if actor_temps.len() <= ti {
                    actor_temps.resize_with(ti + 1, ActorConTemps::default);
                }
                let hit_picnum =
                    rpg_blast_hit_picnum(body.extra, damage_picnum, script, projectile_hit_picnum);
                let temp = &mut actor_temps[ti];
                temp.hit_extra = if temp.hit_extra < 0 {
                    amount
                } else {
                    temp.hit_extra.saturating_add(amount)
                };
                temp.hit_picnum = hit_picnum;
                temp.hit_owner = owner;
                temp.hit_ang = getangle(body.x.wrapping_sub(x), body.y.wrapping_sub(y)) as i16;
            }
        }
        target = next;
    }
    for stat in [STAT_FALLER, STAT_DUMMY, STAT_MISC] {
        let mut target = lists.headspritestat[stat as usize];
        while target >= 0 {
            let next = lists.nextspritestat[target as usize];
            let ti = target as usize;
            if target != wall_source_i && ti < sprites.len() {
                let t = sprites[ti];
                let badguy = crate::badguy::badguy_pic(t.picnum, actortype);
                let flammable = is_flammable_radius_target(script, t.picnum);
                let dist = room_build_engine::numeric::find_distance_3d(
                    x.wrapping_sub(t.x),
                    y.wrapping_sub(t.y),
                    z.wrapping_sub(t.z) >> 4,
                );
                let visible = if stat == STAT_FALLER && !flammable {
                    cansee(
                        t.x,
                        t.y,
                        t.z - (8 << 8),
                        t.sectnum,
                        x,
                        y,
                        z - (12 << 8),
                        sect,
                        &sec_z,
                        &wxy,
                    )
                } else {
                    !badguy
                        || cansee(
                            t.x,
                            t.y,
                            t.z + q,
                            t.sectnum,
                            x,
                            y,
                            z + q,
                            sect,
                            &sec_z,
                            &wxy,
                        )
                };
                if t.extra >= 0 && ((t.cstat & 257) != 0 || flammable) && dist < radius && visible {
                    let actor_branch = stat == STAT_FALLER && !flammable;
                    let amount_before_object =
                        actor_branch.then(|| radius_damage(dist, radius, randomseed, strength));
                    if check_radius_target_object(
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
                        ti,
                        wall_source_i,
                    ) {
                        target = next;
                        continue;
                    }
                    let amount = amount_before_object
                        .unwrap_or_else(|| radius_damage(dist, radius, randomseed, strength));
                    queue_radius_sprite_hit(
                        sprites,
                        actor_temps,
                        ti,
                        t,
                        amount,
                        rpg_blast_hit_picnum(t.extra, damage_picnum, script, projectile_hit_picnum),
                        owner,
                        getangle(t.x.wrapping_sub(x), t.y.wrapping_sub(y)) as i16,
                        strength << 2,
                        wall_source_i,
                        rpg_blast_payload(radius, strength),
                    );
                }
            }
            target = next;
        }
    }
}

/// EDuke32 surface damage held back until the session can run
/// EVENT_DAMAGEWALL / EVENT_DAMAGEFLOOR / EVENT_DAMAGECEILING /
/// EVENT_DAMAGEHPLANE, whose negative `RETURN` cancels the effect. Only hits
/// on surfaces that need an event are deferred (a `damageeventtile` surface
/// tile, or any plane while EVENT_DAMAGEHPLANE is hooked), so Atomic and
/// unflagged maps keep their immediate effects.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SurfaceDamage {
    Wall {
        source: SpriteT,
        source_i: i16,
        atwith: i16,
        wall: usize,
        hit: [i32; 3],
        hit_sector: i16,
    },
    Floor {
        source_i: i16,
        sector: i16,
    },
    Ceiling {
        source_i: i16,
        sector: i16,
        owner: i16,
    },
}

thread_local! {
    static PENDING_SURFACE_DAMAGE: std::cell::RefCell<Vec<SurfaceDamage>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Drain the surface hits deferred since the last call (one session tic).
pub(crate) fn take_pending_surface_damage() -> Vec<SurfaceDamage> {
    PENDING_SURFACE_DAMAGE.with(|queue| std::mem::take(&mut *queue.borrow_mut()))
}

fn defer_surface_damage(damage: SurfaceDamage) {
    PENDING_SURFACE_DAMAGE.with(|queue| queue.borrow_mut().push(damage));
}

/// Whether `event` is hooked by an EDuke32 script.
pub(crate) fn eduke_event_hooked(script: Option<&ScriptImage>, event: &str) -> bool {
    script.is_some_and(|image| {
        image.dialect == crate::con::ConDialect::EDuke32 && image.has_eduke_event(event)
    })
}

/// Whether `tile` was opted into damage events with `damageeventtile`
/// (EDuke32 `SFLAG_DAMAGEEVENT`).
pub(crate) fn eduke_damage_event_tile(script: Option<&ScriptImage>, tile: i16) -> bool {
    script.is_some_and(|image| {
        image.eduke_damage_event_tiles.iter().any(|name| {
            image
                .labels
                .get(name.as_str())
                .filter(|label| !label.is_ptr)
                .map(|label| label.value)
                .or_else(|| name.parse().ok())
                == Some(i32::from(tile))
        })
    })
}

/// EDuke32 `Sect_DamageFloor`: Duke has no hardcoded floor damage, so a
/// floor hit only raises EVENT_DAMAGEFLOOR (for a `damageeventtile` floor)
/// and EVENT_DAMAGEHPLANE. Queued for the session when either is hooked.
pub(crate) fn queue_floor_damage(
    script: Option<&ScriptImage>,
    sectors: &[SectorT],
    source_i: i16,
    sector: i16,
) {
    let Some(floor) = usize::try_from(sector)
        .ok()
        .and_then(|index| sectors.get(index))
        .map(|sector| sector.floorpicnum)
    else {
        return;
    };
    if eduke_event_hooked(script, "EVENT_DAMAGEHPLANE")
        || (eduke_event_hooked(script, "EVENT_DAMAGEFLOOR")
            && eduke_damage_event_tile(script, floor))
    {
        defer_surface_damage(SurfaceDamage::Floor { source_i, sector });
    }
}

/// Defer a ceiling hit for its EDuke32 events; `true` when deferred.
pub(crate) fn defer_ceiling_damage(
    script: Option<&ScriptImage>,
    sectors: &[SectorT],
    source_i: i16,
    sector: i16,
    owner: i16,
) -> bool {
    let Some(ceiling) = usize::try_from(sector)
        .ok()
        .and_then(|index| sectors.get(index))
        .map(|sector| sector.ceilingpicnum)
    else {
        return false;
    };
    let deferred = eduke_event_hooked(script, "EVENT_DAMAGEHPLANE")
        || (eduke_event_hooked(script, "EVENT_DAMAGECEILING")
            && eduke_damage_event_tile(script, ceiling));
    if deferred {
        defer_surface_damage(SurfaceDamage::Ceiling {
            source_i,
            sector,
            owner,
        });
    }
    deferred
}

/// Chocolate `checkhitwall`. With an EDuke32 script that hooks
/// EVENT_DAMAGEWALL, a hit on a `damageeventtile` wall (picnum or overpicnum)
/// is deferred to the session, which runs the event and then applies
/// [`apply_checkhitwall_unchecked`] unless `RETURN` is negative.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_checkhitwall(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &mut [WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    source: SpriteT,
    source_i: i16,
    atwith: i16,
    wall_index: usize,
    hit_x: i32,
    hit_y: i32,
    hit_z: i32,
    hit_sector: i16,
    script: Option<&ScriptImage>,
) -> Vec<i16> {
    if eduke_event_hooked(script, "EVENT_DAMAGEWALL")
        && walls.get(wall_index).is_some_and(|wall| {
            eduke_damage_event_tile(script, wall.picnum)
                || eduke_damage_event_tile(script, wall.overpicnum)
        })
    {
        defer_surface_damage(SurfaceDamage::Wall {
            source,
            source_i,
            atwith,
            wall: wall_index,
            hit: [hit_x, hit_y, hit_z],
            hit_sector,
        });
        return Vec::new();
    }
    apply_checkhitwall_unchecked(
        lists,
        sprites,
        sectors,
        walls,
        picnum_out,
        actor_temps,
        randomseed,
        source,
        source_i,
        atwith,
        wall_index,
        hit_x,
        hit_y,
        hit_z,
        hit_sector,
        script,
    )
}

/// The wall side effects of Chocolate `checkhitwall` (breakable glass,
/// screens, fans, cracks…), without the EDuke32 event check.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_checkhitwall_unchecked(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &mut [WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    source: SpriteT,
    source_i: i16,
    atwith: i16,
    wall_index: usize,
    hit_x: i32,
    hit_y: i32,
    hit_z: i32,
    hit_sector: i16,
    script: Option<&ScriptImage>,
) -> Vec<i16> {
    let tile = |name: &str| {
        script.and_then(|image| {
            image
                .labels
                .get(name)
                .filter(|label| !label.is_ptr)
                .map(|label| label.value as i16)
        })
    };
    let Some(wall) = walls.get(wall_index).copied() else {
        return Vec::new();
    };
    let mut sounds = Vec::new();
    let overlay = wall.overpicnum;
    let peer = wall.nextwall;
    // Chocolate `checkhitwall` handles mirrors before forcefields and other
    // overlay materials, and only explosive/projectile sources listed here
    // can shatter them. This is a direct-impact path; radius explosions have
    // their parallel mirror handling in `apply_radius_wall_hits`.
    let mirror_breaker = [
        "HEAVYHBOMB",
        "RADIUSEXPLOSION",
        "RPG",
        "HYDRENT",
        "SEENINE",
        "OOZFILTER",
        "EXPLODINGBARREL",
    ]
    .iter()
    .any(|name| tile(name) == Some(atwith));
    if tile("MIRROR") == Some(overlay) && mirror_breaker {
        walls[wall_index].cstat &= !16;
        if let Some(broken) = tile("MIRRORBROKE") {
            walls[wall_index].overpicnum = broken;
        }
        spawn_radius_wall_glass(
            lists,
            sprites,
            sectors,
            walls,
            picnum_out,
            actor_temps,
            randomseed,
            SpriteT {
                x: hit_x,
                y: hit_y,
                z: hit_z,
                sectnum: hit_sector,
                ..source
            },
            source_i,
            wall_index,
            70,
            false,
            script,
        );
        sounds.push(20); // GLASS_HEAVYBREAK.
        return sounds;
    }
    let mut restore_glass = false;
    let mut overlay_handled = false;
    let forcefield = tile("W_FORCEFIELD").is_some_and(|base| (base..=base + 2).contains(&overlay));
    let big_force = tile("BIGFORCE") == Some(overlay);
    let force_wall_gate = (wall.cstat & 16 != 0 || big_force)
        && usize::try_from(wall.nextsector)
            .ok()
            .and_then(|sector| sectors.get(sector))
            .is_some_and(|next| next.floorz > hit_z && next.floorz != next.ceilingz);

    if force_wall_gate && (forcefield || big_force) {
        if forcefield {
            walls[wall_index].extra = 1;
        }
        let chaingun_tile = tile("CHAINGUN").unwrap_or(crate::shoot::CHAINGUN);
        let ripple_repeat = if atwith == -1 {
            (8, 8)
        } else if atwith == chaingun_tile {
            (
                source.xrepeat.saturating_add(16),
                source.yrepeat.saturating_add(16),
            )
        } else {
            (32, 32)
        };
        let ripple = crate::shoot::egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            script,
            hit_sector,
            hit_x,
            hit_y,
            hit_z,
            tile("FORCERIPPLE").unwrap_or(1671),
            -127,
            ripple_repeat.0,
            ripple_repeat.1,
            0,
            0,
            0,
            source_i,
            crate::stat::STAT_MISC,
        );
        if let Ok(index) = usize::try_from(ripple)
            && let Some(ripple) = sprites.get_mut(index)
        {
            ripple.cstat |= 18 + 128;
            if let Some(end) = usize::try_from(wall.point2)
                .ok()
                .and_then(|point| walls.get(point))
            {
                ripple.ang = getangle(wall.x.wrapping_sub(end.x), wall.y.wrapping_sub(end.y))
                    .wrapping_sub(512) as i16;
            }
        }
        sounds.push(35); // SOMETHINGHITFORCE.
        return sounds;
    }

    let mut glass_count = 0;
    let mut colored_glass = false;
    let mut screen_break = false;
    let mut wall_light = false;
    if tile("GLASS") == Some(overlay) {
        if let Some(broken) = tile("GLASS2") {
            walls[wall_index].overpicnum = broken;
        }
        walls[wall_index].cstat = 0;
        if let Ok(index) = usize::try_from(peer)
            && let Some(peer) = walls.get_mut(index)
        {
            peer.cstat = 0;
        }
        sounds.push(19); // GLASS_BREAKING.
        glass_count = 10;
        restore_glass = true;
        overlay_handled = true;
    } else if force_wall_gate && tile("FANSPRITE") == Some(overlay) {
        if let Some(broken) = tile("FANSPRITEBROKE") {
            walls[wall_index].overpicnum = broken;
        }
        walls[wall_index].cstat &= !65;
        if let Ok(index) = usize::try_from(peer)
            && let Some(peer) = walls.get_mut(index)
        {
            if let Some(broken) = tile("FANSPRITEBROKE") {
                peer.overpicnum = broken;
            }
            peer.cstat &= !65;
        }
        sounds.extend([18, 19]); // VENT_BUST, GLASS_BREAKING.
        overlay_handled = true;
    } else if force_wall_gate && tile("STAINGLASS1") == Some(overlay) {
        walls[wall_index].cstat = 0;
        if let Ok(index) = usize::try_from(peer)
            && let Some(peer) = walls.get_mut(index)
        {
            peer.cstat = 0;
        }
        sounds.extend([18, 19]); // VENT_BUST, GLASS_BREAKING.
        glass_count = 80;
        colored_glass = true;
        overlay_handled = true;
    }

    let wall_pic = wall.picnum;
    let screen_names = [
        "OJ",
        "FEMPIC2",
        "FEMPIC3",
        "SCREENBREAK1",
        "SCREENBREAK2",
        "SCREENBREAK3",
        "SCREENBREAK4",
        "SCREENBREAK5",
        "SCREENBREAK6",
        "SCREENBREAK7",
        "SCREENBREAK8",
        "SCREENBREAK9",
        "SCREENBREAK10",
        "SCREENBREAK11",
        "SCREENBREAK12",
        "SCREENBREAK13",
        "SCREENBREAK14",
        "SCREENBREAK15",
        "SCREENBREAK16",
        "SCREENBREAK17",
        "SCREENBREAK18",
        "SCREENBREAK19",
        "BORNTOBEWILDSCREEN",
    ];
    if !overlay_handled && screen_names.iter().any(|name| tile(name) == Some(wall_pic)) {
        sounds.push(20); // GLASS_HEAVYBREAK.
        glass_count = 30;
        screen_break = true;
    } else if !overlay_handled
        && let Some((_, replacement)) = [
            ("COLAMACHINE", None),
            ("VENDMACHINE", None),
            ("W_TECHWALL5", Some("W_TECHWALL6")),
            ("W_TECHWALL6", Some("W_TECHWALL7")),
            ("W_TECHWALL7", Some("W_TECHWALL8")),
            ("W_TECHWALL8", Some("W_TECHWALL9")),
            ("W_TECHWALL9", Some("W_TECHWALL10")),
            ("W_MILKSHELF", Some("W_MILKSHELFBROKE")),
            ("W_TECHWALL10", Some("W_HITTECHWALL10")),
            ("W_TECHWALL1", Some("W_HITTECHWALL1")),
            ("W_TECHWALL11", Some("W_HITTECHWALL1")),
            ("W_TECHWALL12", Some("W_HITTECHWALL1")),
            ("W_TECHWALL13", Some("W_HITTECHWALL1")),
            ("W_TECHWALL14", Some("W_HITTECHWALL1")),
            ("W_TECHWALL15", Some("W_HITTECHWALL15")),
            ("W_TECHWALL16", Some("W_HITTECHWALL16")),
            ("W_TECHWALL2", Some("W_HITTECHWALL2")),
            ("W_TECHWALL3", Some("W_HITTECHWALL3")),
            ("W_TECHWALL4", Some("W_HITTECHWALL4")),
        ]
        .into_iter()
        .find(|(name, _)| tile(name) == Some(wall_pic))
    {
        if let Some(replacement) = replacement {
            if let Some(broken) = tile(replacement) {
                walls[wall_index].picnum = broken;
            }
        } else {
            walls[wall_index].picnum = wall_pic.saturating_add(2);
        }
        sounds.extend([18, 20]); // VENT_BUST, GLASS_HEAVYBREAK.
        glass_count = 10;
    } else if !overlay_handled && tile("ATM") == Some(wall_pic) {
        if let Some(broken) = tile("ATMBROKE") {
            walls[wall_index].picnum = broken;
        }
        spawn_radius_wall_money(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            randomseed,
            SpriteT {
                x: hit_x,
                y: hit_y,
                z: hit_z,
                sectnum: hit_sector,
                ..source
            },
            script,
            tile("MONEY").unwrap_or(1233),
        );
        sounds.push(20); // GLASS_HEAVYBREAK.
    } else if !overlay_handled
        && let Some((_, busted)) = [
            ("WALLLIGHT1", "WALLLIGHTBUST1"),
            ("WALLLIGHT2", "WALLLIGHTBUST2"),
            ("WALLLIGHT3", "WALLLIGHTBUST3"),
            ("WALLLIGHT4", "WALLLIGHTBUST4"),
            ("TECHLIGHT2", "TECHLIGHTBUST2"),
            ("TECHLIGHT4", "TECHLIGHTBUST4"),
        ]
        .into_iter()
        .find(|(lit, _)| tile(lit) == Some(wall_pic))
    {
        if let Some(busted) = tile(busted) {
            walls[wall_index].picnum = busted;
        }
        sounds.push(if krand_step(randomseed) & 127 != 0 {
            20
        } else {
            19
        });
        glass_count = 30;
        wall_light = true;
    }

    // Chocolate chooses the wall-light break cue before `lotsofglass`; the
    // shard spawner consumes RNG, so keep this draw ahead of the glass burst.
    let wall_light_sound = wall_light.then(|| {
        if krand_step(randomseed) & 127 != 0 {
            20 // GLASS_HEAVYBREAK.
        } else {
            19 // GLASS_BREAKING.
        }
    });
    if glass_count > 0 {
        spawn_radius_wall_glass(
            lists,
            sprites,
            sectors,
            walls,
            picnum_out,
            actor_temps,
            randomseed,
            SpriteT {
                x: hit_x,
                y: hit_y,
                z: hit_z,
                ..source
            },
            source_i,
            wall_index,
            glass_count,
            colored_glass,
            script,
        );
    }
    if screen_break && let Some(base) = tile("W_SCREENBREAK") {
        // The screen shards consume their random draws before the cracked
        // variant is selected in Chocolate's checkhitwall.
        walls[wall_index].picnum = base + (krand_step(randomseed) % 3) as i16;
    }
    if let Some(sound) = wall_light_sound {
        sounds.push(sound);
    }
    if wall_light
        && wall.lotag != 0
        && let Ok(next_sector) = usize::try_from(wall.nextsector)
        && let Some(next_sector) = sectors.get(next_sector)
    {
        let start = usize::try_from(next_sector.wallptr).unwrap_or(usize::MAX);
        let end = start
            .saturating_add(usize::try_from(next_sector.wallnum).unwrap_or_default())
            .min(walls.len());
        let darkest = walls[start..end]
            .iter()
            .map(|wall| wall.shade)
            .fold(0_i8, i8::max);
        let phase = krand_step(randomseed) & 1;
        let stat = crate::stat::STAT_EFFECTOR as usize;
        let mut marker = lists.headspritestat.get(stat).copied().unwrap_or(-1);
        while marker >= 0 {
            let index = marker as usize;
            let next = lists.nextspritestat.get(index).copied().unwrap_or(-1);
            if let Some(effect) = sprites.get(index)
                && effect.lotag == 3
                && effect.hitag == wall.lotag
            {
                if actor_temps.len() <= index {
                    actor_temps.resize_with(index + 1, ActorConTemps::default);
                }
                actor_temps[index].g_t[2] = phase;
                actor_temps[index].g_t[3] = i32::from(darkest);
                actor_temps[index].g_t[4] = 1;
            }
            marker = next;
        }
    }
    if restore_glass {
        let effector = crate::shoot::egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            script,
            hit_sector,
            wall.x,
            wall.y,
            hit_z,
            tile("SECTOREFFECTOR").unwrap_or(1),
            0,
            0,
            0,
            source.ang,
            0,
            0,
            source_i,
            crate::stat::STAT_EFFECTOR,
        );
        if let Ok(index) = usize::try_from(effector)
            && let Some(effector) = sprites.get_mut(index)
        {
            effector.lotag = 128;
            if actor_temps.len() <= index {
                actor_temps.resize_with(index + 1, ActorConTemps::default);
            }
            actor_temps[index].g_t[1] = 5;
            actor_temps[index].g_t[2] = wall_index as i32;
        }
    }
    sounds
}

/// Apply Chocolate's nearby-wall portion of `hitradius`, using a breadth-first
/// adjacent-sector scan and LOS from each wall's quarter-midpoint.
fn apply_radius_wall_hits(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    source: SpriteT,
    source_i: i16,
    radius: i32,
    sound_owner: i16,
    script: Option<&ScriptImage>,
) -> Vec<i16> {
    let tile = |name: &str| {
        script.and_then(|script| {
            script
                .labels
                .get(name)
                .filter(|label| !label.is_ptr)
                .map(|label| label.value as i16)
        })
    };
    if radius <= 0 || source.sectnum < 0 {
        return Vec::new();
    }
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let sec_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let mut visited = vec![false; sectors.len()];
    let mut queue = Vec::with_capacity(sectors.len());
    let start = source.sectnum as usize;
    if start >= sectors.len() {
        return Vec::new();
    }
    if (source.picnum == script_tile(script, "RPG").unwrap_or(RPG_PROJECTILE)
        && source.xrepeat < 11)
        || tile("SHRINKSPARK") == Some(source.picnum)
    {
        return Vec::new();
    }
    visited[start] = true;
    queue.push(start);
    let mut sounds = Vec::new();
    let mut cursor = 0;
    while cursor < queue.len() {
        let sector_index = queue[cursor];
        cursor += 1;
        let sector = sectors[sector_index];
        // Chocolate checks the sector ceiling before its wall list. The first
        // wall vertex is the fast path; only when it is outside the blast does
        // Atomic test the vertex three walls along the sector boundary.
        if (sector.ceilingz.wrapping_sub(source.z) >> 8) < radius {
            let first_wall = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
            let first_vertex = walls.get(first_wall).copied();
            let first_dist = first_vertex.map(|wall| {
                klabs(wall.x.wrapping_sub(source.x))
                    .wrapping_add(klabs(wall.y.wrapping_sub(source.y)))
            });
            let ceiling_in_radius = first_dist.is_some_and(|dist| dist < radius)
                || first_dist.is_some_and(|dist| dist >= radius)
                    && walls
                        .get(first_wall)
                        .and_then(|wall| usize::try_from(wall.point2).ok())
                        .and_then(|wall| walls.get(wall))
                        .and_then(|wall| usize::try_from(wall.point2).ok())
                        .and_then(|wall| walls.get(wall))
                        .is_some_and(|wall| {
                            klabs(wall.x.wrapping_sub(source.x))
                                .wrapping_add(klabs(wall.y.wrapping_sub(source.y)))
                                < radius
                        });
            if ceiling_in_radius {
                let mut ceiling_sounds = Vec::new();
                crate::shoot::check_hit_ceiling(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum_out,
                    actor_temps,
                    script,
                    randomseed,
                    sector_index as i16,
                    source_i,
                    sound_owner,
                    &mut ceiling_sounds,
                );
                if sound_owner >= 0 && !ceiling_sounds.is_empty() {
                    let owner_index = sound_owner as usize;
                    if actor_temps.len() <= owner_index {
                        actor_temps.resize_with(owner_index + 1, ActorConTemps::default);
                    }
                    actor_temps[owner_index].pending_actor_sounds.extend(
                        ceiling_sounds
                            .into_iter()
                            .filter_map(|sound| i16::try_from(sound).ok()),
                    );
                }
            }
        }
        let wall_start = sector.wallptr.max(0) as usize;
        let wall_end = wall_start
            .saturating_add(sector.wallnum.max(0) as usize)
            .min(walls.len());
        for wall_index in wall_start..wall_end {
            let wall = walls[wall_index];
            let distance = (wall.x.wrapping_sub(source.x))
                .saturating_abs()
                .saturating_add((wall.y.wrapping_sub(source.y)).saturating_abs());
            if distance >= radius {
                continue;
            }
            if let Ok(neighbor) = usize::try_from(wall.nextsector)
                && neighbor < sectors.len()
                && !visited[neighbor]
            {
                visited[neighbor] = true;
                queue.push(neighbor);
            }
            let Ok(point2) = usize::try_from(wall.point2) else {
                continue;
            };
            let Some(end_wall) = walls.get(point2).copied() else {
                continue;
            };
            let midpoint_x = ((wall.x.wrapping_add(end_wall.x) >> 1).wrapping_add(source.x)) >> 1;
            let midpoint_y = ((wall.y.wrapping_add(end_wall.y) >> 1).wrapping_add(source.y)) >> 1;
            let mut midpoint_sector = sector_index as i16;
            if room_build_engine::geom::updatesector(
                midpoint_x,
                midpoint_y,
                &mut midpoint_sector,
                &sec_walls,
                &wall_xy,
            ) < 0
            {
                continue;
            }
            if !cansee(
                midpoint_x,
                midpoint_y,
                source.z,
                midpoint_sector,
                source.x,
                source.y,
                source.z,
                source.sectnum,
                &sec_z,
                &wall_xy,
            ) {
                continue;
            }
            let next_sector = usize::try_from(wall.nextsector)
                .ok()
                .and_then(|index| sectors.get(index));
            let force_wall_gate = (wall.cstat & 16 != 0
                || Some(wall.overpicnum) == tile("BIGFORCE"))
                && next_sector
                    .is_some_and(|next| next.floorz > source.z && next.floorz != next.ceilingz);
            let overlay = walls[wall_index].overpicnum;
            let peer_wall = walls[wall_index].nextwall;
            let forcefield_base = tile("W_FORCEFIELD");
            let forcefield = forcefield_base
                .is_some_and(|base| (base..=base.saturating_add(2)).contains(&overlay));
            let big_force = Some(overlay) == tile("BIGFORCE");
            let mirror_breaker = [
                "HEAVYHBOMB",
                "RADIUSEXPLOSION",
                "RPG",
                "HYDRENT",
                "SEENINE",
                "OOZFILTER",
                "EXPLODINGBARREL",
            ]
            .iter()
            .any(|name| Some(source.picnum) == tile(name));
            let mut glass_count = 0;
            let mut colored_glass = false;
            let mut trigger_wall_light = false;
            let mut screen_break = false;
            let mut glass_restore = false;
            if force_wall_gate && (forcefield || big_force) {
                if forcefield {
                    walls[wall_index].extra = 1;
                }
                let ripple_xrepeat = if Some(source.picnum) == tile("CHAINGUN") {
                    source.xrepeat.saturating_add(16)
                } else {
                    32
                };
                let ripple_yrepeat = if Some(source.picnum) == tile("CHAINGUN") {
                    source.yrepeat.saturating_add(16)
                } else {
                    32
                };
                let ripple = crate::shoot::egs(
                    lists,
                    sprites,
                    picnum_out,
                    actor_temps,
                    script,
                    sector_index as i16,
                    wall.x,
                    wall.y,
                    source.z,
                    tile("FORCERIPPLE").unwrap_or(1671),
                    -127,
                    ripple_xrepeat,
                    ripple_yrepeat,
                    0,
                    0,
                    0,
                    source_i,
                    crate::stat::STAT_MISC,
                );
                if let Ok(ripple_index) = usize::try_from(ripple)
                    && let Some(ripple) = sprites.get_mut(ripple_index)
                {
                    ripple.cstat |= 18 + 128;
                    if let Some(end) = usize::try_from(wall.point2)
                        .ok()
                        .and_then(|point| walls.get(point))
                    {
                        ripple.ang =
                            getangle(wall.x.wrapping_sub(end.x), wall.y.wrapping_sub(end.y))
                                .wrapping_sub(512) as i16;
                    }
                }
                sounds.push(35); // SOMETHINGHITFORCE.
            } else if mirror_breaker && Some(overlay) == tile("MIRROR") {
                walls[wall_index].cstat &= !16;
                if let Some(broken) = tile("MIRRORBROKE") {
                    walls[wall_index].overpicnum = broken;
                }
                sounds.push(20); // GLASS_HEAVYBREAK
                glass_count = 70;
            } else if Some(overlay) == tile("GLASS") {
                if let Some(broken) = tile("GLASS2") {
                    walls[wall_index].overpicnum = broken;
                }
                walls[wall_index].cstat = 0;
                if let Ok(peer) = usize::try_from(peer_wall)
                    && let Some(peer_wall) = walls.get_mut(peer)
                {
                    peer_wall.cstat = 0;
                }
                sounds.push(19); // GLASS_BREAKING
                glass_count = 10;
                glass_restore = true;
            } else if force_wall_gate && Some(overlay) == tile("FANSPRITE") {
                if let Some(broken) = tile("FANSPRITEBROKE") {
                    walls[wall_index].overpicnum = broken;
                }
                walls[wall_index].cstat &= !65;
                if let Ok(peer) = usize::try_from(peer_wall)
                    && let Some(peer_wall) = walls.get_mut(peer)
                {
                    if let Some(broken) = tile("FANSPRITEBROKE") {
                        peer_wall.overpicnum = broken;
                    }
                    peer_wall.cstat &= !65;
                }
                sounds.extend([18, 19]); // VENT_BUST, GLASS_BREAKING
            } else if force_wall_gate && Some(overlay) == tile("STAINGLASS1") {
                walls[wall_index].cstat = 0;
                if let Ok(peer) = usize::try_from(peer_wall)
                    && let Some(peer_wall) = walls.get_mut(peer)
                {
                    peer_wall.cstat = 0;
                }
                sounds.extend([18, 19]); // VENT_BUST, GLASS_BREAKING
                glass_count = 80;
                colored_glass = true;
            }
            let pic = walls[wall_index].picnum;
            let screen_names = [
                "OJ",
                "FEMPIC2",
                "FEMPIC3",
                "SCREENBREAK1",
                "SCREENBREAK2",
                "SCREENBREAK3",
                "SCREENBREAK4",
                "SCREENBREAK5",
                "SCREENBREAK6",
                "SCREENBREAK7",
                "SCREENBREAK8",
                "SCREENBREAK9",
                "SCREENBREAK10",
                "SCREENBREAK11",
                "SCREENBREAK12",
                "SCREENBREAK13",
                "SCREENBREAK14",
                "SCREENBREAK15",
                "SCREENBREAK16",
                "SCREENBREAK17",
                "SCREENBREAK18",
                "SCREENBREAK19",
                "BORNTOBEWILDSCREEN",
            ];
            if screen_names.iter().any(|name| Some(pic) == tile(name)) {
                sounds.push(20); // GLASS_HEAVYBREAK.
                glass_count = 30;
                screen_break = true;
            }
            let breakwall = [
                ("COLAMACHINE", "+2"),
                ("VENDMACHINE", "+2"),
                ("W_TECHWALL5", "W_TECHWALL6"),
                ("W_TECHWALL6", "W_TECHWALL7"),
                ("W_TECHWALL7", "W_TECHWALL8"),
                ("W_TECHWALL8", "W_TECHWALL9"),
                ("W_TECHWALL9", "W_TECHWALL10"),
                ("W_MILKSHELF", "W_MILKSHELFBROKE"),
                ("W_TECHWALL10", "W_HITTECHWALL10"),
                ("W_TECHWALL1", "W_HITTECHWALL1"),
                ("W_TECHWALL11", "W_HITTECHWALL1"),
                ("W_TECHWALL12", "W_HITTECHWALL1"),
                ("W_TECHWALL13", "W_HITTECHWALL1"),
                ("W_TECHWALL14", "W_HITTECHWALL1"),
                ("W_TECHWALL15", "W_HITTECHWALL15"),
                ("W_TECHWALL16", "W_HITTECHWALL16"),
                ("W_TECHWALL2", "W_HITTECHWALL2"),
                ("W_TECHWALL3", "W_HITTECHWALL3"),
                ("W_TECHWALL4", "W_HITTECHWALL4"),
            ];
            for (from, to) in breakwall {
                if Some(pic) != tile(from) {
                    continue;
                }
                let replacement = if to == "+2" {
                    pic.saturating_add(2)
                } else {
                    tile(to).unwrap_or(pic)
                };
                walls[wall_index].picnum = replacement;
                sounds.extend([18, 20]); // breakwall: vent bust + heavy glass.
                glass_count = 10;
                break;
            }
            if Some(pic) == tile("ATM") {
                if let Some(broken) = tile("ATMBROKE") {
                    walls[wall_index].picnum = broken;
                }
                spawn_radius_wall_money(
                    lists,
                    sprites,
                    picnum_out,
                    actor_temps,
                    randomseed,
                    source,
                    script,
                    tile("MONEY").unwrap_or(1233),
                );
                sounds.push(20);
            }
            for (lit, busted) in [
                ("WALLLIGHT1", "WALLLIGHTBUST1"),
                ("WALLLIGHT2", "WALLLIGHTBUST2"),
                ("WALLLIGHT3", "WALLLIGHTBUST3"),
                ("WALLLIGHT4", "WALLLIGHTBUST4"),
                ("TECHLIGHT2", "TECHLIGHTBUST2"),
                ("TECHLIGHT4", "TECHLIGHTBUST4"),
            ] {
                if Some(pic) == tile(lit) {
                    if let Some(busted) = tile(busted) {
                        walls[wall_index].picnum = busted;
                    }
                    // Chocolate rolls once for the cue, then emits 30 shards.
                    glass_count = 30;
                    trigger_wall_light = true;
                    break;
                }
            }
            if glass_count > 0 {
                spawn_radius_wall_glass(
                    lists,
                    sprites,
                    sectors,
                    walls,
                    picnum_out,
                    actor_temps,
                    randomseed,
                    source,
                    source_i,
                    wall_index,
                    glass_count,
                    colored_glass,
                    script,
                );
            }
            if screen_break && let Some(base) = tile("W_SCREENBREAK") {
                // Chocolate emits screen shards before choosing the cracked
                // screen variant, so keep this TRAND after the shard draws.
                walls[wall_index].picnum = base + (krand_step(randomseed) % 3) as i16;
            }
            if glass_restore {
                let effector = crate::shoot::egs(
                    lists,
                    sprites,
                    picnum_out,
                    actor_temps,
                    script,
                    sector_index as i16,
                    wall.x,
                    wall.y,
                    source.z,
                    tile("SECTOREFFECTOR").unwrap_or(1),
                    0,
                    0,
                    0,
                    source.ang,
                    0,
                    0,
                    source_i,
                    crate::stat::STAT_EFFECTOR,
                );
                if let Ok(effector_index) = usize::try_from(effector)
                    && let Some(effector) = sprites.get_mut(effector_index)
                {
                    effector.lotag = 128;
                    if actor_temps.len() <= effector_index {
                        actor_temps.resize_with(effector_index + 1, ActorConTemps::default);
                    }
                    actor_temps[effector_index].g_t[1] = 5;
                    actor_temps[effector_index].g_t[2] = wall_index as i32;
                }
            }
            if trigger_wall_light && wall.lotag != 0 {
                if let Ok(next_sector) = usize::try_from(wall.nextsector)
                    && let Some(next_sector) = sectors.get(next_sector).copied()
                {
                    let start = usize::try_from(next_sector.wallptr).unwrap_or(usize::MAX);
                    let end = start
                        .saturating_add(usize::try_from(next_sector.wallnum).unwrap_or_default())
                        .min(walls.len());
                    let darkest = walls[start..end]
                        .iter()
                        .map(|wall| wall.shade)
                        .max()
                        .unwrap_or(0)
                        .max(0);
                    let phase = krand_step(randomseed) & 1;
                    let stat = crate::stat::STAT_EFFECTOR as usize;
                    let mut marker = lists.headspritestat.get(stat).copied().unwrap_or(-1);
                    while marker >= 0 {
                        let marker_index = marker as usize;
                        let next = lists
                            .nextspritestat
                            .get(marker_index)
                            .copied()
                            .unwrap_or(-1);
                        if let Some(effect) = sprites.get(marker_index)
                            && effect.lotag == 3
                            && effect.hitag == wall.lotag
                        {
                            if actor_temps.len() <= marker_index {
                                actor_temps.resize_with(marker_index + 1, ActorConTemps::default);
                            }
                            actor_temps[marker_index].g_t[2] = phase;
                            actor_temps[marker_index].g_t[3] = i32::from(darkest);
                            actor_temps[marker_index].g_t[4] = 1;
                        }
                        marker = next;
                    }
                }
            }
        }
    }
    sounds
}

fn spawn_radius_wall_money(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    source: SpriteT,
    script: Option<&ScriptImage>,
    money_tile: i16,
) {
    let count = 1 + (krand_step(randomseed) & 7);
    for _ in 0..count {
        let z_offset = krand_step(randomseed) % (47 << 8);
        let angle = (krand_step(randomseed) & 2047) as i16;
        let money = crate::shoot::egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            script,
            source.sectnum,
            source.x,
            source.y,
            source.z.wrapping_sub(z_offset),
            money_tile,
            -32,
            8,
            8,
            angle,
            0,
            0,
            0,
            crate::stat::STAT_MISC,
        );
        if let Ok(money_index) = usize::try_from(money)
            && let Some(money) = sprites.get_mut(money_index)
        {
            money.cstat = (krand_step(randomseed) & 12) as u16;
        } else {
            let _ = krand_step(randomseed);
        }
    }
}

fn spawn_radius_wall_glass(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    walls: &[WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    randomseed: &mut i32,
    source: SpriteT,
    source_i: i16,
    wall_index: usize,
    count: i32,
    colored: bool,
    script: Option<&ScriptImage>,
) {
    let tile = |name: &str| {
        script.and_then(|script| {
            script
                .labels
                .get(name)
                .filter(|label| !label.is_ptr)
                .map(|label| label.value as i16)
        })
    };
    let Some(glass_tile) = tile("GLASSPIECES") else {
        return;
    };
    let Some(wall) = walls.get(wall_index).copied() else {
        return;
    };
    let Some(end) = usize::try_from(wall.point2)
        .ok()
        .and_then(|index| walls.get(index))
        .copied()
    else {
        return;
    };
    let sec_walls: Vec<SectorWalls> = sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let divisor = count.saturating_add(1).max(1);
    let mut x = wall.x;
    let mut y = wall.y;
    if !colored {
        x = x.wrapping_sub((end.y.wrapping_sub(wall.y)).signum());
        y = y.wrapping_add((end.x.wrapping_sub(wall.x)).signum());
    }
    let dx = end.x.wrapping_sub(wall.x) / divisor;
    let dy = end.y.wrapping_sub(wall.y) / divisor;
    let mut sector_hint = -1;
    for shard in (1..=count).rev() {
        x = x.wrapping_add(dx);
        y = y.wrapping_add(dy);
        let sector_index =
            room_build_engine::geom::updatesector(x, y, &mut sector_hint, &sec_walls, &wall_xy);
        let Ok(sector_index) = usize::try_from(sector_index) else {
            continue;
        };
        let Some(sector) = sectors.get(sector_index).copied() else {
            continue;
        };
        // Both `lotsofglass` and `lotsofcolourglass` use `a = SA-1024` along a
        // wall; neither draws a random angle there.
        let angle = source.ang.wrapping_sub(1024);
        let height = sector.floorz.wrapping_sub(sector.ceilingz).saturating_abs();
        let mut z = sector.floorz.wrapping_sub(krand_step(randomseed) & height);
        if !(-32 << 8..=32 << 8).contains(&z) {
            z = source
                .z
                .wrapping_sub(32 << 8)
                .wrapping_add(krand_step(randomseed) & (64 << 8) - 1);
        }
        // EGS arguments draw right to left (32-bit gcc): zvel before xvel.
        let zvel = (-(krand_step(randomseed) & if colored { 2047 } else { 1023 })) as i16;
        let xvel = (32 + (krand_step(randomseed) & 63)) as i16;
        let shard = crate::shoot::egs(
            lists,
            sprites,
            picnum_out,
            actor_temps,
            script,
            sector_index as i16,
            x,
            y,
            z,
            glass_tile + (shard % 3) as i16,
            -32,
            36,
            36,
            angle,
            xvel,
            zvel,
            source_i,
            crate::stat::STAT_MISC,
        );
        if colored && shard >= 0 {
            sprites[shard as usize].pal = (krand_step(randomseed) & 7) as u8;
        }
    }
}

/// Apply a CON `hitradius` request to live actors and players. The command's
/// damage arguments are `[hp1, hp2, hp3, hp4]`, with progressively larger
/// values toward the center, matching Chocolate's three distance bands.
pub(crate) fn apply_con_hitradius(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &mut [SectorT],
    walls: &mut [WallT],
    picnum_out: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    players: &mut [Player],
    randomseed: &mut i32,
    source_i: i16,
    radius: i32,
    damage: [i32; 4],
    actortype: &[u8],
    script: Option<&ScriptImage>,
    tile_meta: &[TileZMeta],
) {
    let Some(source) = sprites.get(source_i.max(0) as usize).copied() else {
        return;
    };
    if radius <= 0 {
        return;
    }
    // EDuke32 `A_RadiusDamage` replaces each target's queued damage unless
    // the blasting projectile is PROJECTILE_HITRADIUS_ADDITIVE.
    let additive = actor_temps
        .get(source_i.max(0) as usize)
        .is_some_and(|temps| {
            temps.projectile_workslike & crate::shoot::PROJECTILE_FLAG_HITRADIUS_ADDITIVE != 0
        });
    let wall_sounds = apply_radius_wall_hits(
        lists,
        sprites,
        sectors,
        walls,
        picnum_out,
        actor_temps,
        randomseed,
        source,
        source_i,
        radius,
        players.first().map_or(-1, |player| player.sprite_i),
        script,
    );
    if !wall_sounds.is_empty() {
        let source_index = source_i as usize;
        if actor_temps.len() <= source_index {
            actor_temps.resize_with(source_index + 1, ActorConTemps::default);
        }
        actor_temps[source_index]
            .pending_actor_sounds
            .extend(wall_sounds);
    }
    let sec_z: Vec<SectorZ> = sectors.iter().map(SectorT::z).collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let q = -(16 << 8) + (krand_step(randomseed) & ((32 << 8) - 1));
    let standable_damage_tiles = [
        script_tile(script, "SEENINE").unwrap_or(1247),
        script_tile(script, "OOZFILTER").unwrap_or(1079),
    ];
    // Chocolate scans STAT_DEFAULT first. This group only hits blocking
    // sprites; blocking badguys additionally require line of sight.
    let mut target = lists.headspritestat[STAT_DEFAULT as usize];
    while target >= 0 {
        let next = lists.nextspritestat[target as usize];
        let ti = target as usize;
        if target != source_i && ti < sprites.len() {
            let t = sprites[ti];
            let badguy = crate::badguy::badguy_pic(t.picnum, actortype);
            let dist = room_build_engine::numeric::find_distance_3d(
                source.x.wrapping_sub(t.x),
                source.y.wrapping_sub(t.y),
                source.z.wrapping_sub(t.z) >> 4,
            );
            let visible = !badguy
                || cansee(
                    t.x,
                    t.y,
                    t.z + q,
                    t.sectnum,
                    source.x,
                    source.y,
                    source.z + q,
                    source.sectnum,
                    &sec_z,
                    &wxy,
                );
            if t.extra >= 0
                && (t.cstat & 257) != 0
                && dist < radius
                && visible
                && !check_radius_target_object(
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
                    ti,
                    source_i,
                )
            {
                let amount = con_radius_damage(dist, radius, damage, randomseed);
                if actor_temps.len() <= ti {
                    actor_temps.resize_with(ti + 1, ActorConTemps::default);
                }
                let temp = &mut actor_temps[ti];
                temp.hit_extra = if temp.hit_extra < 0 || !additive {
                    amount
                } else {
                    temp.hit_extra.saturating_add(amount)
                };
                temp.hit_picnum = if source.picnum
                    == script_tile(script, "RPG").unwrap_or(RPG_PROJECTILE)
                    && t.extra > 0
                {
                    source.picnum
                } else {
                    RADIUSEXPLOSION
                };
                temp.hit_owner = source.owner;
                temp.hit_source = source_i;
                temp.hit_radius = [radius, damage[0], damage[1], damage[2], damage[3]];
                temp.hit_ang =
                    getangle(t.x.wrapping_sub(source.x), t.y.wrapping_sub(source.y)) as i16;
                if !matches!(t.picnum, 1975 | 2360 | 1960 | 2630 | 2710 | 2760 | 4740) {
                    sprites[ti].xvel = sprites[ti]
                        .xvel
                        .max(0)
                        .wrapping_add(source.extra.wrapping_shl(2) as i16);
                }
            }
        }
        target = next;
    }
    let mut target = lists.headspritestat[crate::stat::STAT_ACTOR as usize];
    while target >= 0 {
        let next = lists.nextspritestat[target as usize];
        let ti = target as usize;
        let t = sprites[ti];
        let badguy = crate::badguy::badguy_pic(t.picnum, actortype);
        let special_target = ["TRIPBOMB", "QUEBALL", "STRIPEBALL", "DUKELYINGDEAD"]
            .iter()
            .filter_map(|name| script_tile(script, name))
            .any(|tile| tile == t.picnum);
        let flammable = is_flammable_radius_target(script, t.picnum);
        let eligible =
            t.extra >= 0 && (special_target || badguy || (t.cstat & 257) != 0 || flammable);
        let dist = room_build_engine::numeric::find_distance_3d(
            source.x.wrapping_sub(t.x),
            source.y.wrapping_sub(t.y),
            source.z.wrapping_sub(t.z) >> 4,
        );
        let visible = if flammable {
            !badguy
                || cansee(
                    t.x,
                    t.y,
                    t.z + q,
                    t.sectnum,
                    source.x,
                    source.y,
                    source.z + q,
                    source.sectnum,
                    &sec_z,
                    &wxy,
                )
        } else {
            cansee(
                t.x,
                t.y,
                t.z - (8 << 8),
                t.sectnum,
                source.x,
                source.y,
                source.z - (12 << 8),
                source.sectnum,
                &sec_z,
                &wxy,
            )
        };
        if target != source_i && eligible && dist < radius && visible {
            let amount = con_radius_damage(dist, radius, damage, randomseed);
            if actor_temps.len() <= ti {
                actor_temps.resize_with(ti + 1, ActorConTemps::default);
            }
            let temp = &mut actor_temps[ti];
            temp.hit_extra = if temp.hit_extra < 0 || !additive {
                amount
            } else {
                temp.hit_extra.saturating_add(amount)
            };
            temp.hit_picnum = if source.picnum
                == script_tile(script, "RPG").unwrap_or(RPG_PROJECTILE)
                && t.extra > 0
            {
                source.picnum
            } else {
                RADIUSEXPLOSION
            };
            temp.hit_owner = source.owner;
            temp.hit_source = source_i;
            temp.hit_radius = [radius, damage[0], damage[1], damage[2], damage[3]];
            temp.hit_ang = getangle(t.x.wrapping_sub(source.x), t.y.wrapping_sub(source.y)) as i16;
            if !matches!(t.picnum, 1975 | 2360 | 1960 | 2630 | 2710 | 2760 | 4740) {
                sprites[ti].xvel = sprites[ti]
                    .xvel
                    .max(0)
                    .wrapping_add(source.extra.wrapping_shl(2) as i16);
            }
            if radius_actor_checkhit_target(script, t.picnum) {
                let _ = check_radius_target_object(
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
                    ti,
                    source_i,
                );
            }
        }
        target = next;
    }
    // Chocolate's hitradius stat order includes STAT_STANDABLE as its third
    // list. SEENINE/OOZFILTER carry impact health in `extra`, so blast damage
    // must be queued for their next standable tic just like a direct hit.
    let mut target = lists.headspritestat[crate::stat::STAT_STANDABLE as usize];
    while target >= 0 {
        let next = lists.nextspritestat[target as usize];
        let ti = target as usize;
        if target != source_i && ti < sprites.len() {
            let t = sprites[ti];
            let destructible =
                standable_damage_tiles.contains(&t.picnum) && t.extra >= 0 && (t.cstat & 257) != 0;
            let dist = room_build_engine::numeric::find_distance_3d(
                source.x.wrapping_sub(t.x),
                source.y.wrapping_sub(t.y),
                source.z.wrapping_sub(t.z) >> 4,
            );
            if destructible
                && dist < radius
                && cansee(
                    t.x,
                    t.y,
                    t.z - (8 << 8),
                    t.sectnum,
                    source.x,
                    source.y,
                    source.z - (12 << 8),
                    source.sectnum,
                    &sec_z,
                    &wxy,
                )
            {
                let amount = con_radius_damage(dist, radius, damage, randomseed);
                if actor_temps.len() <= ti {
                    actor_temps.resize_with(ti + 1, ActorConTemps::default);
                }
                let temp = &mut actor_temps[ti];
                temp.hit_extra = if temp.hit_extra < 0 || !additive {
                    amount
                } else {
                    temp.hit_extra.saturating_add(amount)
                };
                temp.hit_picnum = RADIUSEXPLOSION;
                temp.hit_owner = source.owner;
                temp.hit_source = source_i;
                temp.hit_radius = [radius, damage[0], damage[1], damage[2], damage[3]];
                temp.hit_ang =
                    getangle(t.x.wrapping_sub(source.x), t.y.wrapping_sub(source.y)) as i16;
                sprites[ti].xvel = sprites[ti]
                    .xvel
                    .max(0)
                    .wrapping_add(source.extra.wrapping_shl(2) as i16);
                if radius_actor_checkhit_target(script, t.picnum) {
                    let _ = check_radius_target_object(
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
                        ti,
                        source_i,
                    );
                }
            }
        }
        target = next;
    }
    let triggers_tagged_explosives = [
        script_tile(script, "RPG").unwrap_or(RPG_PROJECTILE),
        script_tile(script, "FIREEXT").unwrap_or(916),
        script_tile(script, "RADIUSEXPLOSION").unwrap_or(RADIUSEXPLOSION),
        script_tile(script, "SEENINE").unwrap_or(1247),
        script_tile(script, "OOZFILTER").unwrap_or(1079),
    ]
    .contains(&source.picnum);
    if triggers_tagged_explosives {
        arm_tagged_detonables_from_blast(
            lists,
            sprites,
            actor_temps,
            script,
            source.x,
            source.y,
            source.z,
            radius,
        );
    }
    let mut target = lists.headspritestat[crate::stat::STAT_PLAYER as usize];
    while target >= 0 {
        let next = lists.nextspritestat[target as usize];
        let ti = target as usize;
        if target != source_i
            && let Some(body) = sprites.get(ti).copied()
        {
            // Chocolate temporarily moves APLAYER to its feet for `dist()`,
            // then restores it before the player-specific visibility query.
            let feet_z = body.z.wrapping_sub(crate::player::PHEIGHT);
            let dist = room_build_engine::numeric::find_distance_3d(
                source.x.wrapping_sub(body.x),
                source.y.wrapping_sub(body.y),
                source.z.wrapping_sub(feet_z) >> 4,
            );
            if body.extra >= 0
                && dist < radius
                && cansee(
                    body.x,
                    body.y,
                    body.z - (8 << 8),
                    body.sectnum,
                    source.x,
                    source.y,
                    source.z - (12 << 8),
                    source.sectnum,
                    &sec_z,
                    &wxy,
                )
            {
                let amount = con_radius_damage(dist, radius, damage, randomseed);
                if actor_temps.len() <= ti {
                    actor_temps.resize_with(ti + 1, ActorConTemps::default);
                }
                let temp = &mut actor_temps[ti];
                temp.hit_extra = if temp.hit_extra < 0 || !additive {
                    amount
                } else {
                    temp.hit_extra.saturating_add(amount)
                };
                temp.hit_picnum = if source.picnum
                    == script_tile(script, "RPG").unwrap_or(RPG_PROJECTILE)
                    && body.extra > 0
                {
                    source.picnum
                } else {
                    RADIUSEXPLOSION
                };
                temp.hit_owner = source.owner;
                temp.hit_source = source_i;
                temp.hit_radius = [radius, damage[0], damage[1], damage[2], damage[3]];
                temp.hit_ang =
                    getangle(body.x.wrapping_sub(source.x), body.y.wrapping_sub(source.y)) as i16;
                // Chocolate `hitradius` exits an active player camera when a
                // live owned source damages APLAYER, then releases every
                // active CAMERA1 actor's monitor latch.
                let owner_is_active = usize::try_from(source.owner)
                    .ok()
                    .and_then(|owner| lists.sprite_stat.get(owner))
                    .is_some_and(|status| *status < room_build_engine::world::MAXSTATUS as i16);
                if owner_is_active {
                    if let Some(player) =
                        players.iter_mut().find(|player| player.sprite_i == target)
                    {
                        player.remote_camera = None;
                    }
                    let camera_tile =
                        script_tile(script, "CAMERA1").unwrap_or(crate::spawn::CAMERA1);
                    for (camera_index, camera) in sprites.iter_mut().enumerate() {
                        if camera.picnum == camera_tile
                            && lists.sprite_stat.get(camera_index).copied() == Some(STAT_ACTOR)
                        {
                            camera.yvel = 0;
                        }
                    }
                }
            }
        }
        target = next;
    }
    // Atomic stat traversal continues through STAT_FALLER, STAT_DUMMY, then
    // STAT_MISC after STAT_PLAYER. Preserve that order for TRAND consumption.
    for stat in [crate::stat::STAT_FALLER, STAT_DUMMY, STAT_MISC] {
        let mut target = lists.headspritestat[stat as usize];
        while target >= 0 {
            let next = lists.nextspritestat[target as usize];
            let ti = target as usize;
            if target != source_i && ti < sprites.len() {
                let t = sprites[ti];
                let badguy = crate::badguy::badguy_pic(t.picnum, actortype);
                let dist = room_build_engine::numeric::find_distance_3d(
                    source.x.wrapping_sub(t.x),
                    source.y.wrapping_sub(t.y),
                    source.z.wrapping_sub(t.z) >> 4,
                );
                let flammable = is_flammable_radius_target(script, t.picnum);
                let visible = if stat == crate::stat::STAT_FALLER && !flammable {
                    cansee(
                        t.x,
                        t.y,
                        t.z - (8 << 8),
                        t.sectnum,
                        source.x,
                        source.y,
                        source.z - (12 << 8),
                        source.sectnum,
                        &sec_z,
                        &wxy,
                    )
                } else {
                    !badguy
                        || cansee(
                            t.x,
                            t.y,
                            t.z + q,
                            t.sectnum,
                            source.x,
                            source.y,
                            source.z + q,
                            source.sectnum,
                            &sec_z,
                            &wxy,
                        )
                };
                if t.extra >= 0 && ((t.cstat & 257) != 0 || flammable) && dist < radius && visible {
                    let actor_branch = stat == crate::stat::STAT_FALLER && !flammable;
                    let amount_before_object =
                        actor_branch.then(|| con_radius_damage(dist, radius, damage, randomseed));
                    let object_handled = check_radius_target_object(
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
                        ti,
                        source_i,
                    );
                    if object_handled {
                        target = next;
                        continue;
                    }
                    let amount = amount_before_object
                        .unwrap_or_else(|| con_radius_damage(dist, radius, damage, randomseed));
                    if actor_temps.len() <= ti {
                        actor_temps.resize_with(ti + 1, ActorConTemps::default);
                    }
                    let temp = &mut actor_temps[ti];
                    temp.hit_extra = if temp.hit_extra < 0 || !additive {
                        amount
                    } else {
                        temp.hit_extra.saturating_add(amount)
                    };
                    temp.hit_picnum = if source.picnum
                        == script_tile(script, "RPG").unwrap_or(RPG_PROJECTILE)
                        && t.extra > 0
                    {
                        source.picnum
                    } else {
                        RADIUSEXPLOSION
                    };
                    temp.hit_owner = source.owner;
                    temp.hit_source = source_i;
                    temp.hit_radius = [radius, damage[0], damage[1], damage[2], damage[3]];
                    temp.hit_ang =
                        getangle(t.x.wrapping_sub(source.x), t.y.wrapping_sub(source.y)) as i16;
                    if !matches!(t.picnum, 1975 | 2360 | 1960 | 2630 | 2710 | 2760 | 4740) {
                        sprites[ti].xvel = sprites[ti]
                            .xvel
                            .max(0)
                            .wrapping_add(source.extra.wrapping_shl(2) as i16);
                    }
                }
            }
            target = next;
        }
    }
}

fn con_radius_damage(dist: i32, radius: i32, mut hp: [i32; 4], randomseed: &mut i32) -> i16 {
    let (low, high) = if dist < radius / 3 {
        if hp[3] == hp[2] {
            hp[3] = hp[3].saturating_add(1);
        }
        (hp[2], hp[3])
    } else if dist < radius.saturating_mul(2) / 3 {
        if hp[2] == hp[1] {
            hp[2] = hp[2].saturating_add(1);
        }
        (hp[1], hp[2])
    } else {
        if hp[1] == hp[0] {
            hp[1] = hp[1].saturating_add(1);
        }
        (hp[0], hp[1])
    };
    let width = high.saturating_sub(low).max(1);
    low.saturating_add(krand_step(randomseed) % width)
        .clamp(0, i32::from(i16::MAX)) as i16
}

/// Atomic RPG hitradius bands: hp1=x/4, hp2=x/2, hp3=3x/4, hp4=x;
/// one random roll selects a value in the band for each visible victim.
#[cfg(test)]
fn rpg_radius_damage(dist: i32, randomseed: &mut i32, strength: i32) -> i16 {
    radius_damage(dist, RPG_BLAST_RADIUS, randomseed, strength)
}

fn radius_damage(dist: i32, radius: i32, randomseed: &mut i32, strength: i32) -> i16 {
    let (low, high) = if dist < radius / 3 {
        (strength - (strength >> 2), strength)
    } else if dist < radius * 2 / 3 {
        (strength >> 1, strength - (strength >> 2))
    } else {
        (strength >> 2, strength >> 1)
    };
    let width = (high - low).max(1);
    (low + krand_step(randomseed) % width) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_hits_replace_queued_damage_unless_the_projectile_is_additive() {
        let mut sprites = vec![SpriteT::default(); 4];
        let target = sprites[2];
        let mut temps = vec![ActorConTemps::default(); 4];
        for additive in [false, true] {
            temps[1].projectile_workslike = if additive {
                crate::shoot::PROJECTILE_FLAG_RPG | crate::shoot::PROJECTILE_FLAG_HITRADIUS_ADDITIVE
            } else {
                crate::shoot::PROJECTILE_FLAG_RPG
            };
            temps[2].hit_extra = 10;
            queue_radius_sprite_hit(
                &mut sprites,
                &mut temps,
                2,
                target,
                7,
                RADIUSEXPLOSION,
                3,
                0,
                0,
                1,
                [512, 1, 2, 3, 4],
            );
            assert_eq!(temps[2].hit_extra, if additive { 17 } else { 7 });
            assert_eq!(temps[2].hit_source, 1);
            assert_eq!(temps[2].hit_radius, [512, 1, 2, 3, 4]);
        }
    }

    #[test]
    fn eduke_projectile_velocity_multiplier_updates_each_portal_step() {
        let mut lists = SpriteLists::new();
        let mut sprites = vec![SpriteT::default(); room_build_engine::world::MAXSPRITES];
        let projectile_id = lists.insertsprite(0, STAT_PROJECTILE);
        let projectile_index = projectile_id as usize;
        sprites[projectile_index] = SpriteT {
            x: 900,
            y: 500,
            z: 0,
            sectnum: 0,
            xvel: 100,
            ang: 0,
            ..SpriteT::default()
        };
        let sectors = [
            SectorWalls {
                wallptr: 0,
                wallnum: 4,
            },
            SectorWalls {
                wallptr: 4,
                wallnum: 4,
            },
        ];
        let walls = [
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                ..WallT::default()
            },
            WallT {
                x: 1_000,
                y: 0,
                point2: 2,
                nextsector: 1,
                ..WallT::default()
            },
            WallT {
                x: 1_000,
                y: 1_000,
                point2: 3,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 1_000,
                point2: 0,
                ..WallT::default()
            },
            WallT {
                x: 1_000,
                y: 0,
                point2: 5,
                ..WallT::default()
            },
            WallT {
                x: 2_000,
                y: 0,
                point2: 6,
                ..WallT::default()
            },
            WallT {
                x: 2_000,
                y: 1_000,
                point2: 7,
                ..WallT::default()
            },
            WallT {
                x: 1_000,
                y: 1_000,
                point2: 4,
                nextsector: 0,
                ..WallT::default()
            },
        ];
        let wall_xy = walls
            .iter()
            .map(|wall| WallXY {
                x: wall.x,
                y: wall.y,
                point2: wall.point2,
                nextsector: wall.nextsector,
                cstat: wall.cstat,
            })
            .collect::<Vec<_>>();
        let sector_z = [
            SectorZ {
                wallptr: 0,
                wallnum: 4,
                ceilingz: -10_000,
                floorz: 10_000,
                ceilingstat: 0,
                floorstat: 0,
                ceilingheinum: 0,
                floorheinum: 0,
            },
            SectorZ {
                wallptr: 4,
                wallnum: 4,
                ceilingz: -10_000,
                floorz: 10_000,
                ceilingstat: 0,
                floorstat: 0,
                ceilingheinum: 0,
                floorheinum: 0,
            },
        ];
        let mut actor_temps = vec![ActorConTemps::default(); projectile_index + 1];

        let hit = advance_projectile_with_sweeps(
            &mut lists,
            &mut sprites,
            &mut actor_temps,
            projectile_id,
            3,
            0,
            560,
            &sectors,
            &wall_xy,
            &sector_z,
            &[],
            &walls,
        );

        assert_eq!(hit, room_build_engine::geom::HitScan::MISS);
        assert_eq!(sprites[projectile_index].x, 1_050);
        assert_eq!(sprites[projectile_index].sectnum, 1);
        assert_eq!(lists.sprite_sect[projectile_index], 1);
    }

    #[test]
    fn eduke_custom_rpg_projectile_applies_drop_before_swept_motion() {
        let mut lists = SpriteLists::new();
        let mut sprites = vec![SpriteT::default(); room_build_engine::world::MAXSPRITES];
        let mut sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -100_000,
            floorz: 100_000,
            lotag: 2,
            ..SectorT::default()
        }];
        let mut walls = vec![
            WallT {
                x: -10_000,
                y: -10_000,
                point2: 1,
                ..WallT::default()
            },
            WallT {
                x: 10_000,
                y: -10_000,
                point2: 2,
                ..WallT::default()
            },
            WallT {
                x: 10_000,
                y: 10_000,
                point2: 3,
                ..WallT::default()
            },
            WallT {
                x: -10_000,
                y: 10_000,
                point2: 0,
                ..WallT::default()
            },
        ];
        let projectile_id = lists.insertsprite(0, STAT_PROJECTILE);
        let projectile_index = projectile_id as usize;
        sprites[projectile_index] = SpriteT {
            x: 1_000,
            y: 1_000,
            z: 30_000,
            sectnum: 0,
            picnum: 9_000,
            xvel: 100,
            ang: 0,
            ..SpriteT::default()
        };
        let mut actor_temps = vec![ActorConTemps::default(); projectile_index + 1];
        actor_temps[projectile_index].projectile_workslike =
            crate::shoot::PROJECTILE_FLAG_RPG | crate::shoot::PROJECTILE_FLAG_WATERBUBBLES;
        actor_temps[projectile_index].projectile_drop = -64;
        actor_temps[projectile_index].projectile_velmult = 2;
        actor_temps[projectile_index].projectile_hitradius = 512;
        actor_temps[projectile_index].projectile_trail = 9_001;
        actor_temps[projectile_index].projectile_tnum = 2;
        actor_temps[projectile_index].projectile_txrepeat = 9;
        actor_temps[projectile_index].projectile_tyrepeat = 10;
        actor_temps[projectile_index].projectile_toffset = 1;
        let mut players = Vec::new();
        let mut randomseed = 17;
        let mut picnum = vec![0; projectile_index + 1];

        moveweapons(
            &mut lists,
            &mut sprites,
            &mut sectors,
            &mut walls,
            &mut players,
            &mut randomseed,
            &mut actor_temps,
            &mut picnum,
            None,
            &[],
        );

        assert_eq!(lists.sprite_stat[projectile_index], STAT_PROJECTILE);
        assert_eq!(sprites[projectile_index].x, 1_100);
        assert_eq!(sprites[projectile_index].z, 30_064);
        assert_eq!(sprites[projectile_index].zvel, 64);
        let trails = sprites
            .iter()
            .enumerate()
            .filter(|(index, sprite)| {
                lists.sprite_stat[*index] == crate::stat::STAT_MISC && sprite.picnum == 9_001
            })
            .map(|(_, sprite)| *sprite)
            .collect::<Vec<_>>();
        assert_eq!(trails.len(), 2);
        assert!(trails.iter().all(|trail| {
            trail.x == 1_000
                && trail.y == 1_000
                && trail.z == 30_256
                && trail.xrepeat == 9
                && trail.yrepeat == 10
        }));
        let bubbles = sprites
            .iter()
            .enumerate()
            .filter(|(index, sprite)| {
                lists.sprite_stat[*index] == STAT_MISC && sprite.picnum == WATERBUBBLE
            })
            .map(|(_, sprite)| *sprite)
            .collect::<Vec<_>>();
        assert_eq!(bubbles.len(), 1);
        assert_eq!(
            (bubbles[0].x, bubbles[0].y, bubbles[0].z),
            (1_100, 1_000, 30_064)
        );
        assert!((4..=7).contains(&bubbles[0].xrepeat));
        assert!((4..=7).contains(&bubbles[0].yrepeat));

        actor_temps[projectile_index].projectile_workslike |= crate::shoot::PROJECTILE_FLAG_TIMED;
        actor_temps[projectile_index].projectile_range = 1;
        actor_temps[projectile_index].projectile_age = 1;
        moveweapons(
            &mut lists,
            &mut sprites,
            &mut sectors,
            &mut walls,
            &mut players,
            &mut randomseed,
            &mut actor_temps,
            &mut picnum,
            None,
            &[],
        );
        assert_ne!(lists.sprite_stat[projectile_index], STAT_PROJECTILE);
    }

    #[test]
    fn eduke_custom_rpg_projectile_bounces_from_wall_and_counts_down() {
        let mut lists = SpriteLists::new();
        let mut sprites = vec![SpriteT::default(); room_build_engine::world::MAXSPRITES];
        let mut sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -100_000,
            floorz: 100_000,
            ..SectorT::default()
        }];
        let mut walls = vec![
            WallT {
                x: -10_000,
                y: -10_000,
                point2: 1,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 10_000,
                y: -10_000,
                point2: 2,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 10_000,
                y: 10_000,
                point2: 3,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: -10_000,
                y: 10_000,
                point2: 0,
                nextsector: -1,
                ..WallT::default()
            },
        ];
        let projectile_id = lists.insertsprite(0, STAT_PROJECTILE);
        let projectile_index = projectile_id as usize;
        sprites[projectile_index] = SpriteT {
            x: 9_800,
            y: 1_000,
            z: 30_000,
            sectnum: 0,
            picnum: 9_000,
            xvel: 1_000,
            zvel: 40,
            yvel: 1,
            ang: 0,
            ..SpriteT::default()
        };
        let mut actor_temps = vec![ActorConTemps::default(); projectile_index + 1];
        actor_temps[projectile_index].projectile_workslike = crate::shoot::PROJECTILE_FLAG_RPG
            | crate::shoot::PROJECTILE_FLAG_BOUNCESOFFWALLS
            | crate::shoot::PROJECTILE_FLAG_LOSESVELOCITY;
        actor_temps[projectile_index].projectile_velmult = 2;
        actor_temps[projectile_index].projectile_bsound = 77;
        let mut players = Vec::new();
        let mut randomseed = 17;
        let mut picnum = vec![0; projectile_index + 1];

        moveweapons(
            &mut lists,
            &mut sprites,
            &mut sectors,
            &mut walls,
            &mut players,
            &mut randomseed,
            &mut actor_temps,
            &mut picnum,
            None,
            &[],
        );

        assert_eq!(lists.sprite_stat[projectile_index], STAT_PROJECTILE);
        assert_eq!(
            sprites[projectile_index].ang, 1024,
            "projectile={:?}, temp={:?}",
            sprites[projectile_index], actor_temps[projectile_index]
        );
        assert_eq!(sprites[projectile_index].yvel, 0);
        assert_eq!(sprites[projectile_index].xvel, 500);
        assert_eq!(sprites[projectile_index].zvel, 20);
        assert!(sprites[projectile_index].x > 9_800);
        assert_eq!(actor_temps[projectile_index].pending_actor_sounds, [77]);
    }

    #[test]
    fn eduke_custom_rpg_projectile_bounces_from_sprites_without_wall_budget() {
        let mut lists = SpriteLists::new();
        let mut sprites = vec![SpriteT::default(); room_build_engine::world::MAXSPRITES];
        let mut sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -100_000,
            floorz: 100_000,
            ..SectorT::default()
        }];
        let mut walls = vec![
            WallT {
                x: -10_000,
                y: -10_000,
                point2: 1,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 10_000,
                y: -10_000,
                point2: 2,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 10_000,
                y: 10_000,
                point2: 3,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: -10_000,
                y: 10_000,
                point2: 0,
                nextsector: -1,
                ..WallT::default()
            },
        ];
        let projectile_id = lists.insertsprite(0, STAT_PROJECTILE);
        let projectile_index = projectile_id as usize;
        sprites[projectile_index] = SpriteT {
            x: 1_000,
            y: 1_000,
            z: 30_000,
            sectnum: 0,
            picnum: 9_000,
            xvel: 400,
            zvel: 40,
            ang: 0,
            ..SpriteT::default()
        };
        let target_id = lists.insertsprite(0, STAT_ACTOR);
        let target_index = target_id as usize;
        sprites[target_index] = SpriteT {
            x: 1_200,
            y: 1_000,
            z: 30_000,
            sectnum: 0,
            picnum: 9_001,
            cstat: 257,
            clipdist: 20,
            xrepeat: 40,
            yrepeat: 40,
            extra: 100,
            ..SpriteT::default()
        };
        let mut actor_temps = vec![ActorConTemps::default(); target_index + 1];
        actor_temps[projectile_index].projectile_workslike = crate::shoot::PROJECTILE_FLAG_RPG
            | crate::shoot::PROJECTILE_FLAG_BOUNCESOFFSPRITES
            | crate::shoot::PROJECTILE_FLAG_LOSESVELOCITY;
        actor_temps[projectile_index].projectile_velmult = 2;
        actor_temps[projectile_index].projectile_bsound = 77;
        let mut players = Vec::new();
        let mut randomseed = 17;
        let mut picnum = vec![0; target_index + 1];

        moveweapons(
            &mut lists,
            &mut sprites,
            &mut sectors,
            &mut walls,
            &mut players,
            &mut randomseed,
            &mut actor_temps,
            &mut picnum,
            None,
            &[],
        );

        assert_eq!(lists.sprite_stat[projectile_index], STAT_PROJECTILE);
        assert_eq!(sprites[projectile_index].ang, 1024);
        assert!(sprites[projectile_index].x > 1_000);
        assert_eq!(sprites[projectile_index].yvel, 0);
        assert_eq!(sprites[projectile_index].xvel, 200);
        assert_eq!(sprites[projectile_index].zvel, 20);
        assert_eq!(actor_temps[target_index].hit_extra, -1);
        assert_eq!(actor_temps[projectile_index].pending_actor_sounds, [77]);
    }

    #[test]
    fn eduke_custom_rpg_loses_velocity_when_bouncing_from_a_floor() {
        let mut lists = SpriteLists::new();
        let mut sprites = vec![SpriteT::default(); room_build_engine::world::MAXSPRITES];
        let mut sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -100_000,
            floorz: 100_000,
            ..SectorT::default()
        }];
        let mut walls = vec![
            WallT {
                x: -10_000,
                y: -10_000,
                point2: 1,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 10_000,
                y: -10_000,
                point2: 2,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 10_000,
                y: 10_000,
                point2: 3,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: -10_000,
                y: 10_000,
                point2: 0,
                nextsector: -1,
                ..WallT::default()
            },
        ];
        let projectile_id = lists.insertsprite(0, STAT_PROJECTILE);
        let projectile_index = projectile_id as usize;
        sprites[projectile_index] = SpriteT {
            x: 1_000,
            y: 1_000,
            z: 99_900,
            sectnum: 0,
            picnum: 9_000,
            xvel: 200,
            zvel: 1_000,
            yvel: 2,
            ..SpriteT::default()
        };
        let mut actor_temps = vec![ActorConTemps::default(); projectile_index + 1];
        actor_temps[projectile_index].projectile_workslike = crate::shoot::PROJECTILE_FLAG_RPG
            | crate::shoot::PROJECTILE_FLAG_BOUNCESOFFWALLS
            | crate::shoot::PROJECTILE_FLAG_LOSESVELOCITY;
        actor_temps[projectile_index].projectile_velmult = 2;
        actor_temps[projectile_index].projectile_bsound = 77;
        let mut players = Vec::new();
        let mut randomseed = 17;
        let mut picnum = vec![0; projectile_index + 1];

        moveweapons(
            &mut lists,
            &mut sprites,
            &mut sectors,
            &mut walls,
            &mut players,
            &mut randomseed,
            &mut actor_temps,
            &mut picnum,
            None,
            &[],
        );

        assert_eq!(lists.sprite_stat[projectile_index], STAT_PROJECTILE);
        assert_eq!(sprites[projectile_index].yvel, 1);
        assert_eq!(sprites[projectile_index].xvel, 100);
        assert_eq!(sprites[projectile_index].zvel, -500);
        assert_eq!(actor_temps[projectile_index].pending_actor_sounds, [77]);
    }

    #[test]
    fn eduke_custom_rpg_impact_flag_applies_direct_sprite_damage() {
        let mut lists = SpriteLists::new();
        let mut sprites = vec![SpriteT::default(); room_build_engine::world::MAXSPRITES];
        let mut sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -100_000,
            floorz: 100_000,
            ..SectorT::default()
        }];
        let mut walls = vec![
            WallT {
                x: -10_000,
                y: -10_000,
                point2: 1,
                ..WallT::default()
            },
            WallT {
                x: 10_000,
                y: -10_000,
                point2: 2,
                ..WallT::default()
            },
            WallT {
                x: 10_000,
                y: 10_000,
                point2: 3,
                ..WallT::default()
            },
            WallT {
                x: -10_000,
                y: 10_000,
                point2: 0,
                ..WallT::default()
            },
        ];
        let projectile_id = lists.insertsprite(0, STAT_PROJECTILE);
        let projectile_index = projectile_id as usize;
        sprites[projectile_index] = SpriteT {
            x: 1_000,
            y: 1_000,
            z: 30_000,
            sectnum: 0,
            picnum: 9_000,
            xvel: 400,
            extra: 5,
            owner: -1,
            ang: 0,
            ..SpriteT::default()
        };
        let target_id = lists.insertsprite(0, STAT_ACTOR);
        let target_index = target_id as usize;
        sprites[target_index] = SpriteT {
            x: 1_200,
            y: 1_000,
            z: 30_000,
            sectnum: 0,
            picnum: crate::shoot::APLAYER,
            cstat: 257,
            clipdist: 20,
            xrepeat: 40,
            yrepeat: 40,
            extra: 100,
            ..SpriteT::default()
        };
        let projectile2_id = lists.insertsprite(0, STAT_PROJECTILE);
        let projectile2_index = projectile2_id as usize;
        sprites[projectile2_index] = SpriteT {
            x: 1_000,
            y: 3_000,
            z: 30_000,
            sectnum: 0,
            picnum: 9_002,
            xvel: 400,
            extra: 5,
            owner: -1,
            ang: 0,
            ..SpriteT::default()
        };
        let radius_target_id = lists.insertsprite(0, STAT_DEFAULT);
        let radius_target_index = radius_target_id as usize;
        sprites[radius_target_index] = SpriteT {
            x: 1_200,
            y: 3_000,
            z: 30_000,
            sectnum: 0,
            picnum: 9_003,
            cstat: 257,
            clipdist: 20,
            xrepeat: 40,
            yrepeat: 40,
            extra: 100,
            ..SpriteT::default()
        };
        let mut actor_temps = vec![ActorConTemps::default(); radius_target_index + 1];
        actor_temps[projectile_index].projectile_workslike =
            2 | 32_768 | crate::shoot::PROJECTILE_FLAG_SPIT;
        actor_temps[projectile_index].projectile_spawns = 9_004;
        actor_temps[projectile_index].projectile_isound = 75;
        actor_temps[projectile_index].projectile_sxrepeat = 13;
        actor_temps[projectile_index].projectile_syrepeat = 14;
        actor_temps[projectile2_index].projectile_workslike = 2
            | crate::shoot::PROJECTILE_FLAG_TIMED
            | crate::shoot::PROJECTILE_FLAG_EXPLODEONTIMER
            | crate::shoot::PROJECTILE_FLAG_RADIUS_PICNUM;
        actor_temps[projectile2_index].projectile_hitradius = 512;
        actor_temps[projectile2_index].projectile_range = 1;
        actor_temps[projectile2_index].projectile_age = 1;
        actor_temps[radius_target_index].hit_picnum = 9_003;
        let mut players = vec![Player {
            sprite_i: target_id,
            ..Player::default()
        }];
        let mut randomseed = 17;
        let mut picnum = vec![0; radius_target_index + 1];

        moveweapons(
            &mut lists,
            &mut sprites,
            &mut sectors,
            &mut walls,
            &mut players,
            &mut randomseed,
            &mut actor_temps,
            &mut picnum,
            None,
            &[],
        );

        assert_eq!(actor_temps[target_index].hit_picnum, 9_000);
        assert_eq!(actor_temps[target_index].hit_extra, 5);
        assert_eq!(actor_temps[target_index].hit_owner, -1);
        assert_eq!(players[0].horiz, Player::default().horiz.wrapping_add(32));
        assert_eq!(players[0].return_to_center, 8);
        assert_eq!(players[0].palette_flash, [0, 32, 0]);
        assert_eq!(players[0].palette_flash_tics, 32);
        assert!((3..=6).contains(&players[0].num_loogies));
        assert_eq!(players[0].loogie_tics, 24 * 4);
        assert_eq!(lists.sprite_stat[projectile_index], STAT_MISC);
        assert_eq!(actor_temps[projectile_index].pending_actor_sounds, [75]);
        let impact_spawns = sprites
            .iter()
            .enumerate()
            .filter(|(index, sprite)| {
                lists.sprite_stat[*index] == STAT_ACTOR && sprite.picnum == 9_004
            })
            .map(|(_, sprite)| *sprite)
            .collect::<Vec<_>>();
        assert_eq!(impact_spawns.len(), 1);
        assert_eq!(impact_spawns[0].xrepeat, 13);
        assert_eq!(impact_spawns[0].yrepeat, 14);
        assert_eq!(actor_temps[radius_target_index].hit_picnum, 9_002);
        assert!(actor_temps[radius_target_index].hit_extra > 0);
        assert_eq!(lists.sprite_stat[projectile2_index], STAT_MISC);
    }

    #[test]
    fn rpg_damage_uses_atomic_three_radius_bands() {
        let mut seed = 17_i32;
        let inner = rpg_radius_damage(0, &mut seed, 140);
        assert!((105..140).contains(&inner));
        let middle = rpg_radius_damage(RPG_BLAST_RADIUS / 3, &mut seed, 140);
        assert!((70..105).contains(&middle));
        let outer = rpg_radius_damage(RPG_BLAST_RADIUS * 2 / 3, &mut seed, 140);
        assert!((35..70).contains(&outer));
    }
}

/// True when a projectile's hitscan probe hit something past this tic's
/// `movesprite` step `(dx, dy, dz)` — i.e. no collision this tic.
fn hit_beyond_step(
    hit: &room_build_engine::geom::HitScan,
    sx: i32,
    sy: i32,
    sz: i32,
    dx: i32,
    dy: i32,
    dz: i32,
) -> bool {
    if hit.hitsect < 0 {
        return false;
    }
    let step2 = i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy);
    if step2 > 0 {
        let hx = i64::from(hit.hitx - sx);
        let hy = i64::from(hit.hity - sy);
        hx * hx + hy * hy > step2
    } else {
        (hit.hitz - sz).abs() > dz.abs()
    }
}

/// Keep content-controlled repeated projectile movement bounded per tic.
/// Ordinary EDuke32 mods use small values; this prevents one malformed
/// `PROJ_VEL_MULT` value from making a single actor consume unbounded work.
const MAX_PROJECTILE_VEL_MULT_PER_TIC: i32 = 256;

/// Emit the EDuke32 `PROJECTILE_WATERBUBBLES` underwater cue for a moving
/// user projectile. The EGS arguments and random draw order match the existing
/// Build underwater tracer path.
#[allow(clippy::too_many_arguments)]
fn spawn_custom_projectile_water_bubble(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    sectors: &[SectorT],
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<ActorConTemps>,
    script: Option<&ScriptImage>,
    randomseed: &mut i32,
    sector: i16,
    x: i32,
    y: i32,
    z: i32,
    owner: i16,
) {
    let underwater = usize::try_from(sector)
        .ok()
        .and_then(|index| sectors.get(index))
        .is_some_and(|sector| sector.lotag == 2);
    if !underwater {
        return;
    }

    // Match the existing `tracers` EGS call (32-bit argument evaluation is
    // right-to-left): angle, yrepeat, then xrepeat consume the random stream.
    let angle = (krand_step(randomseed) & 2047) as i16;
    let yrepeat = (4 + (krand_step(randomseed) & 3)) as u8;
    let xrepeat = (4 + (krand_step(randomseed) & 3)) as u8;
    let _ = egs(
        lists,
        sprites,
        picnum,
        actor_temps,
        script,
        sector,
        x,
        y,
        z,
        WATERBUBBLE,
        -32,
        xrepeat,
        yrepeat,
        angle,
        0,
        0,
        owner,
        STAT_MISC,
    );
}

/// Advance a projectile through EDuke32's repeated per-tic movement steps.
/// Each step gets its own hitscan, sector update, and collision decision so
/// rounding and portal transitions follow repeated `movesprite` calls.
#[allow(clippy::too_many_arguments)]
fn advance_projectile_with_sweeps(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    actor_temps: &mut Vec<ActorConTemps>,
    sprite_index: i16,
    steps: i32,
    projectile_workslike: i32,
    mirror_tile: i16,
    sec_w: &[SectorWalls],
    wxy: &[WallXY],
    sec_z: &[SectorZ],
    sprite_hits: &[SpriteHit],
    walls: &[WallT],
) -> room_build_engine::geom::HitScan {
    let iu = sprite_index as usize;
    for _ in 0..steps.clamp(0, MAX_PROJECTILE_VEL_MULT_PER_TIC) {
        let current = sprites[iu];
        let (sin, cos) = sincos_q14(i32::from(current.ang) & 2047);
        let xv = i32::from(current.xvel);
        let dx = (((xv * cos) >> 14) * TICSPERFRAME) >> 3;
        let dy = (((xv * sin) >> 14) * TICSPERFRAME) >> 3;
        let dz = (i32::from(current.zvel) * TICSPERFRAME) >> 3;
        let hit = hitscan_full(
            current.x,
            current.y,
            current.z,
            current.sectnum,
            dx,
            dy,
            dz,
            (256 << 16) + 64,
            sec_w,
            wxy,
            sprite_hits,
            Some(sec_z),
        );
        if hit.hitsect >= 0 && !hit_beyond_step(&hit, current.x, current.y, current.z, dx, dy, dz) {
            if hit.hitwall >= 0 {
                let wall_bounces =
                    projectile_workslike & crate::shoot::PROJECTILE_FLAG_BOUNCESOFFWALLS != 0;
                let mirror_bounces =
                    projectile_workslike & crate::shoot::PROJECTILE_FLAG_BOUNCESOFFMIRRORS != 0;
                let wall_index = hit.hitwall as usize;
                let is_mirror = walls.get(wall_index).is_some_and(|wall| {
                    wall.picnum == mirror_tile || wall.overpicnum == mirror_tile
                });
                let mirror_bounce = mirror_bounces && is_mirror;
                if (mirror_bounce || wall_bounces && sprites[iu].yvel > 0)
                    && bounce_custom_projectile_from_wall(
                        &mut sprites[iu],
                        hit.hitwall,
                        hit.hitx,
                        hit.hity,
                        hit.hitz,
                        hit.hitsect,
                        walls,
                    )
                {
                    if mirror_bounce {
                        sprites[iu].owner = sprite_index;
                    } else {
                        if !is_mirror {
                            sprites[iu].yvel = sprites[iu].yvel.wrapping_sub(1);
                        }
                        queue_custom_projectile_bounce_sound(actor_temps, iu);
                        if projectile_workslike & crate::shoot::PROJECTILE_FLAG_LOSESVELOCITY != 0 {
                            decay_custom_projectile_velocity(&mut sprites[iu]);
                        }
                    }
                    if hit.hitsect >= 0 && lists.sprite_sect[iu] != hit.hitsect {
                        let _ = lists.changespritesect(sprite_index, hit.hitsect);
                    }
                    // EDuke32 returns from custom projectile movement as soon
                    // as a bounce occurs, even when PROJ_VEL_MULT requested
                    // more substeps this tic.
                    return room_build_engine::geom::HitScan::MISS;
                }
            } else if hit.hitsprite >= 0
                && projectile_workslike & crate::shoot::PROJECTILE_FLAG_BOUNCESOFFSPRITES != 0
                && let Some(target) = sprites.get(hit.hitsprite as usize).copied()
                && bounce_custom_projectile_from_sprite(
                    &mut sprites[iu],
                    target,
                    hit.hitx,
                    hit.hity,
                    hit.hitz,
                    hit.hitsect,
                )
            {
                if projectile_workslike & crate::shoot::PROJECTILE_FLAG_LOSESVELOCITY != 0 {
                    decay_custom_projectile_velocity(&mut sprites[iu]);
                }
                if hit.hitsect >= 0 && lists.sprite_sect[iu] != hit.hitsect {
                    let _ = lists.changespritesect(sprite_index, hit.hitsect);
                }
                queue_custom_projectile_bounce_sound(actor_temps, iu);
                return room_build_engine::geom::HitScan::MISS;
            }
            return hit;
        }
        step_projectile(lists, sprites, sprite_index, dx, dy, dz, sec_w, wxy);
    }
    room_build_engine::geom::HitScan::MISS
}

fn queue_custom_projectile_bounce_sound(actor_temps: &mut Vec<ActorConTemps>, sprite_index: usize) {
    if actor_temps.len() <= sprite_index {
        actor_temps.resize_with(sprite_index + 1, ActorConTemps::default);
    }
    let sound = actor_temps[sprite_index].projectile_bsound;
    if sound >= 0 {
        actor_temps[sprite_index]
            .pending_actor_sounds
            .push(sound as i16);
    }
}

/// EDuke32's `PROJ_DECAYVELOCITY` halves both velocity components after a
/// qualifying custom-projectile bounce.
fn decay_custom_projectile_velocity(sprite: &mut SpriteT) {
    sprite.xvel >>= 1;
    sprite.zvel >>= 1;
}

/// Reflect an RPG workslike projectile at a wall and move it a few Build
/// units away from the collision plane so the next tic does not immediately
/// report the same zero-distance hit.
fn bounce_custom_projectile_from_wall(
    sprite: &mut SpriteT,
    wall_index: i16,
    x: i32,
    y: i32,
    z: i32,
    sectnum: i16,
    walls: &[WallT],
) -> bool {
    let Some(wall) = usize::try_from(wall_index)
        .ok()
        .and_then(|index| walls.get(index))
    else {
        return false;
    };
    let Some(end) = usize::try_from(wall.point2)
        .ok()
        .and_then(|index| walls.get(index))
    else {
        return false;
    };
    let wall_angle = getangle(end.x.wrapping_sub(wall.x), end.y.wrapping_sub(wall.y));
    sprite.ang = ((wall_angle * 2 - i32::from(sprite.ang)) & 2047) as i16;
    sprite.x = x;
    sprite.y = y;
    sprite.z = z;
    sprite.sectnum = sectnum;
    nudge_custom_projectile_from_impact(sprite);
    true
}

/// Reflect an RPG workslike projectile away from the surface normal of a hit
/// sprite. EDuke32's wall bounce counter is deliberately untouched here.
fn bounce_custom_projectile_from_sprite(
    sprite: &mut SpriteT,
    target: SpriteT,
    x: i32,
    y: i32,
    z: i32,
    sectnum: i16,
) -> bool {
    let normal_angle = getangle(x.wrapping_sub(target.x), y.wrapping_sub(target.y));
    let incoming_angle = i32::from(sprite.ang);
    let outgoing = if x == target.x && y == target.y {
        (incoming_angle + 1024) & 2047
    } else {
        (2 * normal_angle + 1024 - incoming_angle) & 2047
    };
    sprite.ang = outgoing as i16;
    sprite.x = x;
    sprite.y = y;
    sprite.z = z;
    sprite.sectnum = sectnum;
    nudge_custom_projectile_from_impact(sprite);
    true
}

fn nudge_custom_projectile_from_impact(sprite: &mut SpriteT) {
    let (sin, cos) = sincos_q14(i32::from(sprite.ang) & 2047);
    sprite.x = sprite.x.wrapping_add((cos * 8) >> 14);
    sprite.y = sprite.y.wrapping_add((sin * 8) >> 14);
}

/// Advance a projectile by one unobstructed step and refresh its sector.
#[allow(clippy::too_many_arguments)]
fn step_projectile(
    lists: &mut SpriteLists,
    sprites: &mut [SpriteT],
    i: i16,
    dx: i32,
    dy: i32,
    dz: i32,
    sec_w: &[SectorWalls],
    wxy: &[WallXY],
) {
    let iu = i as usize;
    let old_sect = sprites[iu].sectnum;
    sprites[iu].x = sprites[iu].x.wrapping_add(dx);
    sprites[iu].y = sprites[iu].y.wrapping_add(dy);
    sprites[iu].z = sprites[iu].z.wrapping_add(dz);
    let mut next = old_sect;
    room_build_engine::geom::updatesector(sprites[iu].x, sprites[iu].y, &mut next, sec_w, wxy);
    if next >= 0 && next != old_sect {
        let _ = lists.changespritesect(i, next);
        sprites[iu].sectnum = next;
    }
}

/// Chocolate `getglobalz` for a STAT_PROJECTILE sprite: `getzrange` from
/// `z - FOURSLEIGHT` with a 4-unit radius and blocking sprites; when the
/// floor it finds is the projectile owner's own APLAYER body, the sector's
/// planes are used instead. Returns `(ceilingz, floorz)`.
fn projectile_globalz(
    s: &SpriteT,
    sectors: &[SectorT],
    sec_z: &[room_build_engine::geom::SectorZ],
    wxy: &[room_build_engine::geom::WallXY],
    sprites: &[room_build_engine::geom::SpriteZClip],
    all: &[SpriteT],
) -> (i32, i32) {
    const FOURSLEIGHT: i32 = 1 << 8;
    let range = room_build_engine::geom::getzrange_with_sprites(
        s.x,
        s.y,
        s.z - FOURSLEIGHT,
        s.sectnum,
        4,
        (1 << 16) | 1,
        sec_z,
        wxy,
        sprites,
    );
    let sector = usize::try_from(s.sectnum).ok().and_then(|i| sectors.get(i));
    if range.florhit & 49152 == 49152 {
        let lz = range.florhit & 4095;
        let lz_is_player = all
            .get(lz as usize)
            .is_some_and(|body| body.picnum == APLAYER && body.cstat & 48 == 0);
        if lz_is_player
            && i32::from(s.owner) == lz
            && let Some(sector) = sector
        {
            return (sector.ceilingz, sector.floorz);
        }
    }
    (range.ceilz, range.florz)
}
