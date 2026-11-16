//! Player interaction (`gamefunc_Open`) with Build wall switches.

use room_build_engine::types::WallT;
use room_build_engine::world::SpriteLists;
use room_build_engine::{
    BuildWorld, SectorWalls, SectorZ, WallXY, approx_sincos_q14, cansee, length2d, neartag_wall,
};

use crate::con::ActorConTemps;
use crate::player::Player;
use crate::prelevel::ShadeCycler;
use crate::wall_animations::AnimWall;

/// Chocolate `neartag` range used by `checksectors` for the open action.
const USE_RANGE: i32 = 1280;

/// Handle Chocolate `checksectors`' entry-independent special-sector prefix.
/// Returns true when vanilla returns before processing the player's Use key.
pub(crate) fn handle_sector_special(
    world: &mut BuildWorld,
    player: &mut Player,
    level_exit_ready: &mut bool,
) -> bool {
    let Ok(sector_index) = usize::try_from(player.cursectnum) else {
        return false;
    };
    let Some(sector) = world.sectors.get_mut(sector_index) else {
        return false;
    };
    match sector.lotag {
        32767 => {
            sector.lotag = 0;
            if player.fta_enabled {
                player.quote_id = 9;
                player.quote_timer = 100;
            }
            player.secret_rooms = player.secret_rooms.wrapping_add(1);
            true
        }
        -1 => {
            sector.lotag = 0;
            *level_exit_ready = true;
            true
        }
        -2 => {
            sector.lotag = 0;
            player.time_before_exit = 26 * 8;
            player.custom_exit_sound = sector.hitag;
            true
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CameraUseResult {
    NotUsed,
    Entered,
    Exited,
}

/// Chocolate `checksectors` VIEWSCREEN path: a monitor's lotag selects its
/// inactive CAMERA1 actor, then links the monitor owner and enables the camera.
pub(crate) fn use_camera_screen(
    world: &mut BuildWorld,
    player: &mut Player,
    tile_meta: &[crate::player::TileZMeta],
) -> CameraUseResult {
    // Chocolate handles an active `newowner` before looking for a new
    // VIEWSCREEN target: the use edge exits the current feed even if another
    // monitor is still in the player's neartag ray. Otherwise a second screen
    // could steal the edge and leave the player stuck in remote view.
    if let Some((camera_x, camera_y, camera_z, camera_ang, camera_sector)) =
        player.remote_camera.take()
    {
        for (index, sprite) in world.sprites.iter_mut().enumerate() {
            if sprite.picnum == crate::spawn::CAMERA1
                && sprite.x == camera_x
                && sprite.y == camera_y
                && sprite.z == camera_z
                && sprite.ang == camera_ang
                && sprite.sectnum == camera_sector
                && world
                    .lists
                    .sprite_stat
                    .get(index)
                    .is_some_and(|status| *status == crate::stat::STAT_ACTOR)
            {
                sprite.yvel = 0;
            }
        }
        return CameraUseResult::Exited;
    }

    let (sin, cos) = approx_sincos_q14(i32::from(player.view_ang()));
    let sectors: Vec<_> = world
        .sectors
        .iter()
        .map(|sector| SectorWalls {
            wallptr: sector.wallptr,
            wallnum: sector.wallnum,
        })
        .collect();
    let wall_xy: Vec<_> = world
        .walls
        .iter()
        .map(|wall| WallXY {
            x: wall.x,
            y: wall.y,
            point2: wall.point2,
            nextsector: wall.nextsector,
            cstat: wall.cstat,
        })
        .collect();
    let sector_z: Vec<_> = world
        .sectors
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
    let wall_lotag: Vec<_> = world.walls.iter().map(|wall| wall.lotag).collect();
    let wall_target = neartag_wall(
        player.posx,
        player.posy,
        player.posz,
        player.cursectnum,
        player.view_ang(),
        USE_RANGE,
        &sectors,
        &wall_xy,
        &wall_lotag,
    );
    let (ray_sin, ray_cos) = (i64::from(sin), i64::from(cos));
    let ray_x = ray_cos * i64::from(USE_RANGE) >> 14;
    let ray_y = ray_sin * i64::from(USE_RANGE) >> 14;
    let ray_len_sq = i128::from(ray_x) * i128::from(ray_x) + i128::from(ray_y) * i128::from(ray_y);
    let mut nearest: Option<(i32, usize)> = None;
    for (index, screen) in world.sprites.iter().enumerate() {
        if !matches!(
            screen.picnum,
            crate::spawn::VIEWSCREEN | crate::spawn::VIEWSCREEN2
        ) || screen.sectnum < 0
            || world
                .lists
                .sprite_stat
                .get(index)
                .is_none_or(|status| *status == room_build_engine::world::MAXSTATUS as i16)
        {
            continue;
        }
        let dx = i64::from(screen.x) - i64::from(player.posx);
        let dy = i64::from(screen.y) - i64::from(player.posy);
        let dot = ray_x * dx + ray_y * dy;
        if dot <= 0 || i128::from(dot) > ray_len_sq {
            continue;
        }
        let meta = tile_meta
            .get(screen.picnum.max(0) as usize)
            .copied()
            .unwrap_or_default();
        let height = i64::from(meta.height.max(0)) * i64::from(screen.yrepeat);
        let mut top_z = i64::from(screen.z);
        if screen.cstat & 128 != 0 {
            top_z += height << 1;
        }
        top_z -= ((i64::from(meta.y_offset) + i64::from(screen.yoffset))
            * i64::from(screen.yrepeat))
            << 2;
        let bottom_z = top_z - (height << 2);
        let cross = i128::from(ray_x) * i128::from(dy) - i128::from(ray_y) * i128::from(dx);
        let width = i128::from(meta.width.max(0)) * i128::from(screen.xrepeat);
        if cross * cross * 128 > ray_len_sq * width * width {
            continue;
        }
        let hit_x =
            (i128::from(player.posx) + i128::from(ray_x) * i128::from(dot) / ray_len_sq) as i64;
        let hit_y =
            (i128::from(player.posy) + i128::from(ray_y) * i128::from(dot) / ray_len_sq) as i64;
        let distance = length2d(
            (hit_x - i64::from(player.posx)) as i32,
            (hit_y - i64::from(player.posy)) as i32,
        );
        if wall_target.is_some_and(|(_, wall_distance)| wall_distance < distance) {
            continue;
        }
        let target_z =
            (top_z - (height << 1)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let visible_from_z = [0_i32, 8 << 8, 16 << 8].into_iter().any(|z_offset| {
            let z = player.posz.saturating_add(z_offset);
            i64::from(z) <= top_z
                && i64::from(z) >= bottom_z
                && cansee(
                    player.posx,
                    player.posy,
                    z,
                    player.cursectnum,
                    screen.x,
                    screen.y,
                    target_z,
                    screen.sectnum,
                    &sector_z,
                    &wall_xy,
                )
        });
        if !visible_from_z {
            continue;
        }
        if nearest.is_none_or(|(best, _)| distance < best) {
            nearest = Some((distance, index));
        }
    }
    if let Some((_, screen_index)) = nearest {
        let screen_tag = world.sprites[screen_index].lotag;
        let camera_index = (0..world.sprites.len()).find(|&index| {
            let camera = world.sprites[index];
            camera.picnum == crate::spawn::CAMERA1
                && camera.pal == 0
                && camera.hitag == screen_tag
                && camera.sectnum >= 0
                && camera.yvel == 0
                && world
                    .lists
                    .sprite_stat
                    .get(index)
                    .is_some_and(|status| *status == crate::stat::STAT_ACTOR)
        });
        if let Some(camera_index) = camera_index {
            let camera = world.sprites[camera_index];
            player.remote_camera = Some((camera.x, camera.y, camera.z, camera.ang, camera.sectnum));
            world.sprites[camera_index].yvel = 1;
            world.sprites[screen_index].owner = camera_index as i16;
            return CameraUseResult::Entered;
        }
    }
    CameraUseResult::NotUsed
}

/// Switch tile ids from Atomic `names.h`, recorded as data (not translated code).
const SWITCH_BASES: &[i16] = &[
    130,  // ACCESSSWITCH
    132,  // SLOTDOOR
    134,  // LIGHTSWITCH
    136,  // SPACEDOORSWITCH
    138,  // SPACELIGHTSWITCH
    140,  // FRANKENSTINESWITCH
    146,  // MULTISWITCH (four frames)
    162,  // DIPSWITCH
    164,  // DIPSWITCH2
    166,  // TECHSWITCH
    168,  // DIPSWITCH3
    170,  // ACCESSSWITCH2
    712,  // LIGHTSWITCH2
    860,  // POWERSWITCH1
    862,  // LOCKSWITCH1
    864,  // POWERSWITCH2
    1111, // HANDSWITCH
    1122, // PULLSWITCH
    1142, // ALIENSWITCH
];

/// Door textures accepted by Chocolate `isadoorwall` as tagged operators.
const DOOR_OPERATOR_WALLS: &[i16] = &[
    150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 395, 447, 448, 449, 717, 781, 1102, 1144,
    1169, 1178, 1179, 4391,
];

/// Active floor / ceiling interpolation used by `doanimations`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SectorMotion {
    sector: usize,
    floor_goal: Option<i32>,
    ceiling_goal: Option<i32>,
    velocity: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum WallAxis {
    X,
    Y,
}

/// Active wall vertex interpolation used by sliding doors (lotag 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct WallMotion {
    sector: usize,
    wall: usize,
    axis: WallAxis,
    goal: i32,
    velocity: i32,
}

/// Chocolate `getanimationgoal(&sector[sector].ceilingz) >= 0`.
pub(crate) fn ceiling_animating(motions: &[SectorMotion], sector: usize) -> bool {
    motions
        .iter()
        .any(|m| m.sector == sector && m.ceiling_goal.is_some())
}

/// Advance active plane animations at Atomic's 120 Hz interpolation scale.
pub(crate) fn tick_sector_motions(
    motions: &mut Vec<SectorMotion>,
    world: &mut BuildWorld,
    lists: &SpriteLists,
    actor_temps: &mut Vec<ActorConTemps>,
    player_sector: i16,
    player_z: &mut i32,
    player_zv: &mut i32,
) {
    // Chocolate SE22 toothed doors carry a tagged ceiling while its controlling
    // sector ceiling animation is active (`actors.c::moveeffectors`).
    let mut effector = lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
    while effector >= 0 {
        let index = effector as usize;
        effector = lists.nextspritestat[index];
        let Some(marker) = world.sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 22 || actor_temps.get(index).is_none_or(|temp| temp.g_t[1] == 0) {
            continue;
        }
        let controlling_sector = actor_temps[index].g_t[0];
        let moving = usize::try_from(controlling_sector)
            .ok()
            .is_some_and(|sector| {
                motions
                    .iter()
                    .any(|motion| motion.sector == sector && motion.ceiling_goal.is_some())
            });
        if moving {
            if let Ok(sector) = usize::try_from(marker.sectnum)
                && let Some(sector) = world.sectors.get_mut(sector)
            {
                sector.ceilingz = sector
                    .ceilingz
                    .saturating_add(i32::from(marker.extra).saturating_mul(9));
            }
        } else if let Some(temp) = actor_temps.get_mut(index) {
            temp.g_t[1] = 0;
        }
    }

    let mut i = motions.len();
    while i > 0 {
        i -= 1;
        let motion = motions[i];
        let Some(sector) = world.sectors.get_mut(motion.sector) else {
            motions.swap_remove(i);
            continue;
        };
        let mut floor_delta = 0;
        let complete_floor = motion.floor_goal.is_none_or(|goal| {
            let step = motion.velocity.saturating_mul(4) * (goal - sector.floorz).signum();
            let next = sector.floorz.saturating_add(step);
            let next = if step > 0 {
                next.min(goal)
            } else {
                next.max(goal)
            };
            floor_delta = next - sector.floorz;
            sector.floorz = next;
            next == goal
        });
        let complete_ceiling = motion.ceiling_goal.is_none_or(|goal| {
            let step = motion.velocity.saturating_mul(4) * (goal - sector.ceilingz).signum();
            let next = sector.ceilingz.saturating_add(step);
            let next = if step > 0 {
                next.min(goal)
            } else {
                next.max(goal)
            };
            sector.ceilingz = next;
            next == goal
        });
        if floor_delta != 0 {
            if player_sector == motion.sector as i16
                && sector.floorz.saturating_sub(*player_z) < (64 << 8)
            {
                *player_z = player_z.saturating_add(floor_delta);
                *player_zv = 0;
            }
            let mut sprite = lists.headspritesect[motion.sector];
            while sprite >= 0 {
                let idx = sprite as usize;
                let next = lists.nextspritesect[idx];
                if lists.sprite_stat[idx] != crate::stat::STAT_EFFECTOR {
                    let previous_z = world.sprites.get(idx).map(|actor| actor.z);
                    if let Some(actor) = world.sprites.get_mut(idx) {
                        actor.z = actor.z.saturating_add(floor_delta);
                    }
                    if actor_temps.len() <= idx {
                        actor_temps.resize_with(idx + 1, ActorConTemps::default);
                    }
                    if let Some(previous_z) = previous_z {
                        actor_temps[idx].bposz = previous_z;
                    }
                    actor_temps[idx].hit_floorz = sector.floorz;
                    actor_temps[idx].hit_ceilingz = sector.ceilingz;
                }
                sprite = next;
            }
        }
        if complete_floor && complete_ceiling {
            call_sector_sound(world, motion.sector as i16, actor_temps);
            motions.swap_remove(i);
        }
    }
}

/// Cache the two bridge endpoints used by Chocolate's SE 20 `moveeffectors`.
pub(crate) fn init_bridge_effectors(world: &BuildWorld, temps: &mut Vec<ActorConTemps>) {
    let mut marker = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        if world.sprites[index].lotag != 20 {
            continue;
        }
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        // Chocolate spawn case 20: two passes over the SE's *own sector*
        // walls, picking the closest vertex and then the closest other one.
        let Some(sector) = usize::try_from(world.sprites[index].sectnum)
            .ok()
            .and_then(|sector| world.sectors.get(sector))
        else {
            continue;
        };
        let start = sector.wallptr.max(0) as usize;
        let end = start
            .saturating_add(sector.wallnum.max(0) as usize)
            .min(world.walls.len());
        let (sx, sy) = (world.sprites[index].x, world.sprites[index].y);
        let distance_to = |wall: &room_build_engine::types::WallT| {
            crate::actors::find_distance_2d(sx.wrapping_sub(wall.x), sy.wrapping_sub(wall.y))
        };
        let mut first = -1_i32;
        let mut best = i32::MAX;
        for wall_index in start..end {
            let distance = distance_to(&world.walls[wall_index]);
            if distance < best {
                best = distance;
                first = wall_index as i32;
            }
        }
        let mut second = first;
        best = i32::MAX;
        for wall_index in start..end {
            let distance = distance_to(&world.walls[wall_index]);
            if distance < best && wall_index as i32 != first {
                best = distance;
                second = wall_index as i32;
            }
        }
        temps[index].g_t[1] = first;
        temps[index].g_t[2] = second;
        temps[index].g_t[3] = 0;
    }
}

/// Tick Atomic extend-o-bridge sectors (SE 20) and carry grounded occupants.
pub(crate) fn tick_bridge_effectors(
    world: &mut BuildWorld,
    players: &mut [Player],
    temps: &mut Vec<ActorConTemps>,
) {
    let mut marker = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        if world.sprites[index].lotag != 20 || temps.get(index).is_none_or(|t| t.g_t[0] == 0) {
            continue;
        }
        let direction = if temps[index].g_t[0] == 1 { 1 } else { -1 };
        let distance = direction * 8;
        let angle = i32::from(world.sprites[index].ang) & 2047;
        let (sin, cos) = room_build_engine::helpers::sincos_q14(angle);
        let dx = (distance * cos) >> 14;
        let dy = (distance * sin) >> 14;
        // Chocolate: `s->xvel = ±8; t[3] += xvel;` and the travel limit is
        // `SP` (sprite yvel = the sector's GPSPEED), not xvel. T4 keeps the
        // overshoot on the stopping tic so open/close travel stays symmetric.
        world.sprites[index].xvel = distance as i16;
        let progress = temps[index].g_t[3].wrapping_add(distance);
        temps[index].g_t[3] = progress;
        let limit = i32::from(world.sprites[index].yvel);
        if progress <= 0 || (progress >> 6) >= (limit >> 6) {
            temps[index].g_t[0] = 0;
            call_sector_sound(world, world.sprites[index].sectnum, temps);
            continue;
        }
        world.sprites[index].x = world.sprites[index].x.wrapping_add(dx);
        world.sprites[index].y = world.sprites[index].y.wrapping_add(dy);
        for endpoint in [temps[index].g_t[1], temps[index].g_t[2]] {
            if endpoint >= 0 {
                let wall = endpoint as usize;
                if wall < world.walls.len() {
                    let x = world.walls[wall].x.wrapping_add(dx);
                    let y = world.walls[wall].y.wrapping_add(dy);
                    dragpoint_compat(&mut world.walls, wall, x, y);
                }
            }
        }
        let sector_index = usize::try_from(world.sprites[index].sectnum).ok();
        if let Some(sector) = sector_index.and_then(|s| world.sectors.get_mut(s)) {
            sector.floorxpanning = sector.floorxpanning.wrapping_sub((dx >> 3) as u8);
            sector.floorypanning = sector.floorypanning.wrapping_sub((dy >> 3) as u8);
            sector.ceilingxpanning = sector.ceilingxpanning.wrapping_sub((dx >> 3) as u8);
            sector.ceilingypanning = sector.ceilingypanning.wrapping_sub((dy >> 3) as u8);
        }
        let sector = world.sprites[index].sectnum;
        let Ok(sector_index) = usize::try_from(sector) else {
            continue;
        };
        let Some(&head) = world.lists.headspritesect.get(sector_index) else {
            continue;
        };
        let sector_walls = world.sector_walls();
        let wall_xy = world.wall_xy();
        let mut occupant = head;
        while occupant >= 0 {
            let sprite_index = occupant as usize;
            occupant = world.lists.nextspritesect[sprite_index];
            if sprite_index != index
                && world.lists.sprite_stat[sprite_index] != crate::stat::STAT_EFFECTOR
                && world.lists.sprite_stat[sprite_index] != crate::stat::STAT_PLAYER
                && world.sprites[sprite_index].zvel == 0
            {
                world.sprites[sprite_index].x = world.sprites[sprite_index].x.wrapping_add(dx);
                world.sprites[sprite_index].y = world.sprites[sprite_index].y.wrapping_add(dy);
                let old_sector = world.sprites[sprite_index].sectnum;
                let mut new_sector = old_sector;
                room_build_engine::geom::updatesector(
                    world.sprites[sprite_index].x,
                    world.sprites[sprite_index].y,
                    &mut new_sector,
                    &sector_walls,
                    &wall_xy,
                );
                if new_sector >= 0 && new_sector != old_sector {
                    world.sprites[sprite_index].sectnum = new_sector;
                    let _ = world
                        .lists
                        .changespritesect(sprite_index as i16, new_sector);
                }
            }
        }
        for player in players.iter_mut() {
            if player.cursectnum == sector && player.on_ground {
                player.posx = player.posx.wrapping_add(dx);
                player.posy = player.posy.wrapping_add(dy);
                player.oposx = player.oposx.wrapping_add(dx);
                player.oposy = player.oposy.wrapping_add(dy);
                if let Ok(body) = usize::try_from(player.sprite_i)
                    && let Some(sprite) = world.sprites.get_mut(body)
                {
                    sprite.x = player.posx;
                    sprite.y = player.posy;
                    let mut new_sector = sprite.sectnum;
                    room_build_engine::geom::updatesector(
                        player.posx,
                        player.posy,
                        &mut new_sector,
                        &sector_walls,
                        &wall_xy,
                    );
                    if new_sector >= 0 && new_sector != sprite.sectnum {
                        sprite.sectnum = new_sector;
                        let _ = world.lists.changespritesect(body as i16, new_sector);
                    }
                    if new_sector >= 0 {
                        player.cursectnum = new_sector;
                    }
                }
            }
        }
    }
}

/// Advance active wall vertex animations at Atomic's 120 Hz interpolation scale.
pub(crate) fn tick_wall_motions(motions: &mut Vec<WallMotion>, world: &mut BuildWorld) {
    let mut i = motions.len();
    while i > 0 {
        i -= 1;
        let motion = motions[i];
        let Some(wall) = world.walls.get(motion.wall).copied() else {
            motions.swap_remove(i);
            continue;
        };
        let value = match motion.axis {
            WallAxis::X => wall.x,
            WallAxis::Y => wall.y,
        };
        let delta = motion.goal.saturating_sub(value);
        let step = motion.velocity.saturating_mul(4) * delta.signum();
        let next = value.saturating_add(step);
        let moved = if step > 0 {
            next.min(motion.goal)
        } else {
            next.max(motion.goal)
        };
        let (x, y) = match motion.axis {
            WallAxis::X => (moved, wall.y),
            WallAxis::Y => (wall.x, moved),
        };
        dragpoint_compat(&mut world.walls, motion.wall, x, y);
        if moved == motion.goal {
            motions.swap_remove(i);
        }
    }
}

/// Chocolate Build `dragpoint`: move a highlighted vertex and every matching
/// vertex across a connected two-sided wall chain.
pub(crate) fn dragpoint_compat(walls: &mut [WallT], point: usize, x: i32, y: i32) {
    if point >= walls.len() {
        return;
    }
    walls[point].x = x;
    walls[point].y = y;
    let mut current = point;
    for _ in 0..walls.len() {
        let Ok(nextwall) = usize::try_from(walls[current].nextwall) else {
            // Search clockwise through the predecessor edge of the original
            // loop, as Build's lastwall() fallback does.
            current = point;
            for _ in 0..walls.len() {
                let previous = previous_wall(walls, current);
                let Ok(peer) = usize::try_from(walls[previous].nextwall) else {
                    break;
                };
                if peer >= walls.len() {
                    break;
                }
                current = peer;
                walls[current].x = x;
                walls[current].y = y;
                if current == point {
                    break;
                }
            }
            break;
        };
        if nextwall >= walls.len() {
            break;
        }
        let Ok(peer_point) = usize::try_from(walls[nextwall].point2) else {
            break;
        };
        if peer_point >= walls.len() {
            break;
        }
        current = peer_point;
        walls[current].x = x;
        walls[current].y = y;
        if current == point {
            break;
        }
    }
}

fn previous_wall(walls: &[WallT], point: usize) -> usize {
    if point > 0
        && walls
            .get(point - 1)
            .is_some_and(|wall| wall.point2 == point as i16)
    {
        return point - 1;
    }
    let mut current = point;
    for _ in 0..walls.len() {
        let Ok(next) = usize::try_from(walls[current].point2) else {
            return point;
        };
        if next >= walls.len() {
            return point;
        }
        if next == point {
            return current;
        }
        current = next;
    }
    point
}

pub(crate) fn operate_activators(
    world: &mut BuildWorld,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    cyclers: &mut [ShadeCycler],
    lotag: i16,
    player: Option<&mut Player>,
    actor_temps: &mut Vec<ActorConTemps>,
    monsters_off: bool,
    actor_types: &[u8],
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
) {
    let mut player = player;
    for cycler in cyclers
        .iter_mut()
        .rev()
        .filter(|cycler| cycler.hitag == lotag)
    {
        cycler.active = !cycler.active;
        if let Some(sector_index) = usize::try_from(cycler.sector).ok()
            && let Some(sector) = world.sectors.get_mut(sector_index)
        {
            sector.floorshade = cycler.max_shade;
            sector.ceilingshade = cycler.max_shade;
            let first_wall = sector.wallptr.max(0) as usize;
            let wall_end = first_wall
                .saturating_add(sector.wallnum.max(0) as usize)
                .min(world.walls.len());
            if let Some(walls) = world.walls.get_mut(first_wall..wall_end) {
                for wall in walls {
                    wall.shade = cycler.max_shade;
                }
            }
        }
    }
    let mut i = world.lists.headspritestat[crate::stat::STAT_ACTIVATOR as usize];
    let mut lotag_22_sound_handled = false;
    while i >= 0 {
        let idx = i as usize;
        i = world.lists.nextspritestat[idx];
        let activator = world.sprites[idx];
        if activator.lotag != lotag {
            continue;
        }
        let sector_idx = activator.sectnum;
        if activator.picnum == crate::spawn::ACTIVATORLOCKED {
            if let Ok(sector_index) = usize::try_from(sector_idx)
                && let Some(sector) = world.sectors.get_mut(sector_index)
            {
                sector.lotag ^= 16384;
                if let Some(player) = player.as_deref_mut() {
                    if player.fta_enabled {
                        player.quote_id = if sector.lotag & 16384 != 0 { 4 } else { 8 };
                        player.quote_timer = 100;
                    }
                }
            }
            continue;
        }
        if activator.picnum != crate::spawn::ACTIVATOR {
            continue;
        }
        let Some(sector) = usize::try_from(sector_idx)
            .ok()
            .and_then(|idx| world.sectors.get(idx).copied())
        else {
            continue;
        };
        match activator.hitag {
            1 if sector.floorz != sector.ceilingz => continue,
            2 if sector.floorz == sector.ceilingz => continue,
            _ => {}
        }
        // Chocolate `operateactivators` first toggles these effectors in
        // low-lotag sectors, then operates the sector itself. Keep the local
        // sprite-list order because each matching SE also runs `callsound`.
        if sector.lotag < 3 {
            let Ok(sector_index) = usize::try_from(sector_idx) else {
                continue;
            };
            let mut marker = world.lists.headspritesect[sector_index];
            while marker >= 0 {
                let marker_index = marker as usize;
                marker = world.lists.nextspritesect[marker_index];
                if world.lists.sprite_stat[marker_index] != crate::stat::STAT_EFFECTOR
                    || !matches!(world.sprites[marker_index].lotag, 18 | 31 | 32 | 36)
                {
                    continue;
                }
                if actor_temps.len() <= marker_index {
                    actor_temps.resize_with(marker_index + 1, ActorConTemps::default);
                }
                actor_temps[marker_index].g_t[0] =
                    1_i32.wrapping_sub(actor_temps[marker_index].g_t[0]);
                call_sector_sound(world, sector_idx, actor_temps);
            }
        }
        if sector.lotag & 0xff == 22 && !lotag_22_sound_handled {
            lotag_22_sound_handled = call_sector_sound(world, sector_idx, actor_temps);
        }
        operate_sector_motion(
            world,
            motions,
            wall_motions,
            sector_idx,
            actor_temps,
            player.as_deref(),
            Some(activator.hitag),
        );
    }
    // Chocolate's `operateactivators` always finishes with
    // `operaterespawns(low)`, including calls from trains and touchplates.
    arm_respawn_markers(
        world,
        lotag,
        monsters_off,
        actor_types,
        actor_temps,
        picnum,
        script,
    );
}

/// Advance native `SE 10` automatic sector activators.
///
/// Chocolate `moveeffectors` waits `hitag + 1` tics, then delegates to
/// `activatebysector`: tagged activators in the sector win, otherwise the
/// sector's own lotag is operated. Player occupancy suppresses most automatic
/// triggers, except lotag 27.
pub(crate) fn tick_auto_activators(
    world: &mut BuildWorld,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    cyclers: &mut [ShadeCycler],
    players: &[Player],
    temps: &mut Vec<ActorConTemps>,
    monsters_off: bool,
    actor_types: &[u8],
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
) {
    let mut effector = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
    while effector >= 0 {
        let index = effector as usize;
        effector = world.lists.nextspritestat[index];
        let Some(marker) = world.sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 10 {
            continue;
        }
        let Ok(sector_index) = usize::try_from(marker.sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        let low_tag = sector.lotag & 0xff;
        let trigger_sector = low_tag == 27
            || (sector.floorz > sector.ceilingz && low_tag != 23)
            || sector.lotag == -32745; // `(short)32791` in Chocolate.
        if !trigger_sector {
            temps[index].g_t[0] = 0;
            continue;
        }
        let player_inside = low_tag != 27
            && sector.lotag != 30
            && sector.lotag != 31
            && sector.lotag != 0
            && players.iter().any(|player| {
                usize::try_from(player.sprite_i)
                    .ok()
                    .and_then(|sprite_index| world.sprites.get(sprite_index))
                    .is_some_and(|body| body.sectnum == marker.sectnum)
            });
        if player_inside {
            continue;
        }

        if temps[index].g_t[0] <= i32::from(marker.hitag) {
            temps[index].g_t[0] = temps[index].g_t[0].saturating_add(1);
            continue;
        }

        let waits_for_ceiling = matches!(low_tag, 20 | 21 | 22 | 26)
            && motions
                .iter()
                .any(|motion| motion.sector == sector_index && motion.ceiling_goal.is_some());
        if waits_for_ceiling {
            continue;
        }

        let mut activator = world.lists.headspritesect[sector_index];
        let mut activated_tagged_switch = false;
        while activator >= 0 {
            let actor_index = activator as usize;
            activator = world.lists.nextspritesect[actor_index];
            let Some(actor) = world.sprites.get(actor_index).copied() else {
                continue;
            };
            if actor.picnum == crate::spawn::ACTIVATOR {
                operate_activators(
                    world,
                    motions,
                    wall_motions,
                    cyclers,
                    actor.lotag,
                    None,
                    temps,
                    monsters_off,
                    actor_types,
                    picnum,
                    script,
                );
                activated_tagged_switch = true;
            }
        }
        if !activated_tagged_switch {
            operate_sector_motion(
                world,
                motions,
                wall_motions,
                marker.sectnum,
                temps,
                None,
                None,
            );
        }
        temps[index].g_t[0] = 0;
    }
}

fn arm_respawn_markers(
    world: &mut BuildWorld,
    lotag: i16,
    monsters_off: bool,
    actor_types: &[u8],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
) {
    let mut i = world.lists.headspritestat[crate::stat::STAT_FX as usize];
    while i >= 0 {
        let index = i as usize;
        i = world.lists.nextspritestat[index];
        let marker = world.sprites[index];
        if marker.picnum == crate::spawn::RESPAWN
            && marker.lotag == lotag
            && !(monsters_off && crate::badguy::badguy_pic(marker.hitag, actor_types))
        {
            // Chocolate `operaterespawns` creates the transporter star now;
            // `movefx` advances the marker's 54-tic seed later in this tic.
            if actor_temps.len() <= index {
                actor_temps.resize_with(index + 1, ActorConTemps::default);
            }
            world.sprites[index].extra = 54;
            let star = crate::shoot::egs(
                &mut world.lists,
                world.sprites.as_mut(),
                picnum,
                actor_temps,
                script,
                marker.sectnum,
                marker.x,
                marker.y,
                marker.z.saturating_sub(32 << 8),
                crate::spawn::TRANSPORTERSTAR,
                -127,
                48,
                64,
                marker.ang,
                128,
                0,
                index as i16,
                crate::stat::STAT_ACTOR,
            );
            if star >= 0 {
                let star_index = star as usize;
                world.sprites[star_index].cstat = 130;
                if let Some(image) = script
                    && image
                        .actorscrptr
                        .get(crate::spawn::TRANSPORTERSTAR as usize)
                        .copied()
                        .unwrap_or(0)
                        <= 0
                {
                    world.sprites[star_index].xrepeat = 0;
                    world.sprites[star_index].yrepeat = 0;
                }
            }
            actor_temps[index].respawn_star_spawned = true;
        }
    }
}

/// Chocolate `dnunlock`: clear the locked bit from eligible sectors in reverse
/// order, run each sector's normal operation, then toggle every force field.
pub(crate) fn cheat_unlock_all(
    world: &mut BuildWorld,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    anim_walls: &mut [crate::wall_animations::AnimWall],
    actor_temps: &mut Vec<ActorConTemps>,
    player: &Player,
) {
    for sector_index in (0..world.sectors.len()).rev() {
        let original_tag = world.sectors[sector_index].lotag;
        if original_tag == -1 || original_tag == i16::MAX {
            continue;
        }
        if (original_tag as u16 & 0x7fff) <= 2 {
            continue;
        }
        if original_tag as u16 & 0xbfff != 0 {
            world.sectors[sector_index].lotag &= !16384;
        }
        if let Ok(sector_num) = i16::try_from(sector_index) {
            operate_sector_motion(
                world,
                motions,
                wall_motions,
                sector_num,
                actor_temps,
                Some(player),
                None,
            );
        }
    }
    crate::wall_animations::operate_forcefields(&mut world.walls, anim_walls, -1);
}

/// Resolve one deferred CON opcode 66 and dispatch Chocolate operatesectors.
pub(crate) fn operate_con_near_sector(
    world: &mut BuildWorld,
    actor_index: usize,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    cyclers: &mut [ShadeCycler],
    actor_temps: &mut Vec<ActorConTemps>,
    players: &[Player],
    tile_meta: &[crate::player::TileZMeta],
) {
    let Some(actor) = world.sprites.get(actor_index).copied() else {
        return;
    };
    let Some(sector) = world.sectors.get(actor.sectnum.max(0) as usize) else {
        return;
    };
    if actor.sectnum < 0 || sector.lotag != 0 {
        return;
    }
    let Some(target) = near_tagged_sector(
        world,
        actor.x,
        actor.y,
        actor.z - (32 << 8),
        actor.sectnum,
        actor.ang,
        tile_meta,
    ) else {
        return;
    };
    let Some(target_sector) = world.sectors.get(target as usize).copied() else {
        return;
    };
    let lotag = target_sector.lotag;
    let near_operator = matches!(lotag & 0xff, 9 | 15 | 16..=23 | 25 | 26 | 29);
    if !near_operator
        || !((lotag & 0xff) == 23 || target_sector.floorz == target_sector.ceilingz)
        || lotag & 16384 != 0
        || (lotag as u16) & 32768 != 0
    {
        return;
    }
    // Chocolate's CON opcode avoids sectors with an ACTIVATOR marker.
    let mut sprite = world.lists.headspritesect[target as usize];
    while sprite >= 0 {
        let index = sprite as usize;
        if index >= world.sprites.len() || index >= world.lists.nextspritesect.len() {
            break;
        }
        if world.sprites[index].picnum == crate::spawn::ACTIVATOR {
            return;
        }
        sprite = world.lists.nextspritesect[index];
    }
    let (player_index, _) = crate::fta::findplayer_dist(&actor, players);
    operate_sector_motion(
        world,
        motions,
        wall_motions,
        target,
        actor_temps,
        players.get(player_index),
        None,
    );
    let _ = cyclers;
}

/// Chocolate neartag sector-tag ray for a sector-local CON operate.
fn near_tagged_sector(
    world: &BuildWorld,
    x: i32,
    y: i32,
    z: i32,
    start_sector: i16,
    angle: i16,
    tile_meta: &[crate::player::TileZMeta],
) -> Option<i16> {
    near_tagged_sector_range(world, x, y, z, start_sector, angle, tile_meta, 768)
}

/// Chocolate `checksectors` tail for the player's open key: operate the
/// tagged sector in front (`isanearoperator`), or the sector the player
/// stands in (`isanunderoperator`). Sectors holding an ACTIVATOR or
/// MASTERSWITCH are switch-driven and ignore the key. Returns true when a
/// sector was operated or the key was consumed by such a sector.
pub(crate) fn use_near_sector(
    world: &mut BuildWorld,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    actor_temps: &mut Vec<ActorConTemps>,
    player: &Player,
    tile_meta: &[crate::player::TileZMeta],
) -> bool {
    let is_near = |lotag: i16| matches!(lotag & 0xff, 9 | 15..=23 | 25 | 26 | 29);
    let is_under = |lotag: i16| matches!(lotag & 0xff, 15..=19 | 22 | 26);
    let switch_driven = |world: &BuildWorld, sector: i16| {
        let mut i = world.lists.headspritesect[sector as usize];
        while i >= 0 {
            let index = i as usize;
            if index >= world.sprites.len() {
                break;
            }
            let pic = world.sprites[index].picnum;
            if pic == crate::spawn::ACTIVATOR || pic == crate::spawn::MASTERSWITCH {
                return true;
            }
            i = world.lists.nextspritesect[index];
        }
        false
    };
    let start = player.cursectnum;
    if start < 0 || start as usize >= world.sectors.len() {
        return false;
    }
    let mut target = None;
    for dz in [0, 8 << 8, 16 << 8] {
        target = near_tagged_sector_range(
            world,
            player.posx,
            player.posy,
            player.posz + dz,
            start,
            player.ang,
            tile_meta,
            1280,
        );
        if target.is_some() {
            break;
        }
    }
    if std::env::var_os("ROOM_USE_DEBUG").is_some() {
        eprintln!(
            "use: pos ({}, {}, {}) sect {start} ang {} -> near sector {target:?}",
            player.posx, player.posy, player.posz, player.ang
        );
    }
    if let Some(t) = target
        && let Some(sector) = world.sectors.get(t as usize).copied()
        && sector.lotag & 16384 == 0
        && is_near(sector.lotag)
    {
        if switch_driven(world, t) {
            return true;
        }
        operate_sector_motion(
            world,
            motions,
            wall_motions,
            t,
            actor_temps,
            Some(player),
            None,
        );
        return true;
    }
    let own = world.sectors[start as usize];
    if own.lotag & 16384 == 0 && is_under(own.lotag) {
        if switch_driven(world, start) {
            return true;
        }
        operate_sector_motion(
            world,
            motions,
            wall_motions,
            start,
            actor_temps,
            Some(player),
            None,
        );
        return true;
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn near_tagged_sector_range(
    world: &BuildWorld,
    x: i32,
    y: i32,
    z: i32,
    start_sector: i16,
    angle: i16,
    tile_meta: &[crate::player::TileZMeta],
    range: i32,
) -> Option<i16> {
    let start = usize::try_from(start_sector).ok()?;
    if start >= world.sectors.len() {
        return None;
    }
    let (sin, cos) = approx_sincos_q14(i32::from(angle));
    let vx = ((i64::from(cos) * i64::from(range)) >> 14) as i32;
    let vy = ((i64::from(sin) * i64::from(range)) >> 14) as i32;
    let mut queue = vec![start_sector];
    let mut head = 0;
    let mut nearest: Option<(i64, Option<i16>)> = None;
    while head < queue.len() {
        let sector_index = queue[head] as usize;
        head += 1;
        let Some(sector) = world.sectors.get(sector_index) else {
            continue;
        };
        let begin = sector.wallptr.max(0) as usize;
        let end = begin
            .saturating_add(sector.wallnum.max(0) as usize)
            .min(world.walls.len());
        for wall_index in begin..end {
            let wall = world.walls[wall_index];
            let Ok(point2) = usize::try_from(wall.point2) else {
                continue;
            };
            let Some(other) = world.walls.get(point2) else {
                continue;
            };
            let side_left = i64::from(wall.x - x) * i64::from(other.y - y);
            let side_right = i64::from(other.x - x) * i64::from(wall.y - y);
            if side_left < side_right {
                continue;
            }
            let Some((distance, _, _)) = room_build_engine::geom::ray_seg_intersect(
                x, y, vx, vy, wall.x, wall.y, other.x, other.y,
            ) else {
                continue;
            };
            if distance > 65536 {
                continue;
            }
            let next_sector = wall.nextsector;
            if next_sector >= 0 {
                if let Some(adjacent) = world.sectors.get(next_sector as usize)
                    && adjacent.lotag != 0
                    && nearest.is_none_or(|(best, _)| distance < best)
                {
                    nearest = Some((distance, Some(next_sector)));
                }
                if !queue.contains(&next_sector) {
                    queue.push(next_sector);
                }
            }
            if wall.lotag != 0 && nearest.is_none_or(|(best, _)| distance < best) {
                nearest = Some((distance, None));
            }
        }

        // Build neartag also considers tagged sprites after the sector walls.
        // Use ART dimensions and picanm offsets rather than a guessed radius.
        // The candidate is tested against the currently clipped ray endpoint,
        // matching the nearest-tag behavior of `engine.c::neartag`.
        let mut sprite = world.lists.headspritesect[sector_index];
        while sprite >= 0 {
            let index = sprite as usize;
            if index >= world.sprites.len() || index >= world.lists.nextspritesect.len() {
                break;
            }
            sprite = world.lists.nextspritesect[index];
            let tagged = world.sprites[index];
            if tagged.lotag == 0 {
                continue;
            }
            let Some(meta) = tile_meta.get(tagged.picnum.max(0) as usize) else {
                continue;
            };
            let bot = i64::from(vx) * i64::from(vx) + i64::from(vy) * i64::from(vy);
            if bot == 0 {
                continue;
            }
            let dx = i64::from(tagged.x) - i64::from(x);
            let dy = i64::from(tagged.y) - i64::from(y);
            let top = i64::from(vx) * dx + i64::from(vy) * dy;
            if top <= 0 {
                continue;
            }
            let height = i64::from(meta.height) * i64::from(tagged.yrepeat);
            let mut top_z = i64::from(tagged.z);
            if tagged.cstat & 128 != 0 {
                top_z += height << 1;
            }
            top_z -= (i64::from(meta.y_offset) * i64::from(tagged.yrepeat)) << 2;
            let ray_z = i64::from(z);
            if ray_z > top_z || ray_z < top_z - (height << 2) {
                continue;
            }
            let cross = i64::from(vx) * dy - i64::from(vy) * dx;
            let off_x = i64::from(vx) * cross / bot;
            let off_y = i64::from(vy) * cross / bot;
            let width = i64::from(meta.width) * i64::from(tagged.xrepeat);
            if off_x * off_x + off_y * off_y > (width * width >> 7) {
                continue;
            }
            let hit_x = i64::from(x) + i64::from(vx) * top / bot;
            let hit_y = i64::from(y) + i64::from(vy) * top / bot;
            let distance = (hit_x - i64::from(x)).abs() + (hit_y - i64::from(y)).abs();
            let best_distance = nearest.map_or_else(
                || i64::from(vx).abs() + i64::from(vy).abs(),
                |(best, _)| (i64::from(vx).abs() + i64::from(vy).abs()) * best / 65536,
            );
            if distance < best_distance {
                let ray_fraction = (top * 65536 / bot).clamp(0, 65536);
                nearest = Some((ray_fraction, None));
            }
        }
    }
    let _ = z;
    nearest.and_then(|(_, sector)| sector)
}

fn operate_sector_motion(
    world: &mut BuildWorld,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    sector_idx: i16,
    actor_temps: &mut Vec<ActorConTemps>,
    player: Option<&Player>,
    activator_hitag: Option<i16>,
) {
    let Ok(sector_index) = usize::try_from(sector_idx) else {
        return;
    };
    let Some(sector) = world.sectors.get(sector_index).copied() else {
        return;
    };
    // Chocolate keeps sector locking (bit 14) and toggle direction (bit 15)
    // out of the base lotag used by `operatesectors`.
    let kind = sector.lotag & 0x3fff;
    if kind == 15 {
        activate_warp_elevator(world, sector_idx, actor_temps, player);
        return;
    }
    if kind == 30 {
        let Ok(marker_index) = usize::try_from(sector.hitag) else {
            return;
        };
        if !world
            .sprites
            .get(marker_index)
            .is_some_and(|marker| marker.lotag == 30 && marker.sectnum == sector_idx)
        {
            return;
        }
        if actor_temps.len() <= marker_index {
            actor_temps.resize_with(marker_index + 1, ActorConTemps::default);
        }
        world.sprites[marker_index].extra = if world.sprites[marker_index].extra == 1 {
            3
        } else {
            1
        };
        actor_temps[marker_index].g_t[4] = 1;
        // Chocolate only plays the SE30 switch cue at the two settled
        // endpoints of the subway animation. Intermediate activations still
        // toggle the controller's travel mode, but stay silent.
        if matches!(actor_temps[marker_index].tempang, 0 | 256) {
            call_sector_sound(world, sector_idx, actor_temps);
        }
        return;
    }
    if kind == 9 {
        let old_len = wall_motions.len();
        operate_sliding_sector(
            world,
            sector_idx,
            i32::from(sector.extra >> 4),
            wall_motions,
        );
        if wall_motions.len() > old_len {
            call_sector_sound(world, sector_idx, actor_temps);
        }
        return;
    }
    if kind == 28 {
        let mut local = world.lists.headspritesect[sector_index];
        let mut cascade_tag = None;
        while local >= 0 {
            let index = local as usize;
            local = world.lists.nextspritesect[index];
            if world.lists.sprite_stat[index] == crate::stat::STAT_EFFECTOR
                && world
                    .sprites
                    .get(index)
                    .is_some_and(|marker| marker.lotag == 21)
            {
                cascade_tag = world.sprites.get(index).map(|marker| marker.hitag);
                break;
            }
        }
        let Some(cascade_tag) = cascade_tag else {
            return;
        };
        let mut marker = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
        while marker >= 0 {
            let index = marker as usize;
            marker = world.lists.nextspritestat[index];
            if !world
                .sprites
                .get(index)
                .is_some_and(|sprite| sprite.lotag == 21 && sprite.hitag == cascade_tag)
            {
                continue;
            }
            if actor_temps.len() <= index {
                actor_temps.resize_with(index + 1, ActorConTemps::default);
            }
            if actor_temps[index].g_t[0] == 0 {
                actor_temps[index].g_t[0] = 1;
            }
        }
        call_sector_sound(world, sector_idx, actor_temps);
        return;
    }
    if kind == 23 {
        let mut local = world.lists.headspritesect[sector_index];
        let mut group_tag = None;
        while local >= 0 {
            let index = local as usize;
            local = world.lists.nextspritesect[index];
            if world.lists.sprite_stat[index] != crate::stat::STAT_EFFECTOR
                || world.sprites[index].lotag != 11
                || actor_temps
                    .get(index)
                    .is_some_and(|temps| temps.g_t[4] != 0)
            {
                continue;
            }
            group_tag = Some(world.sprites[index].hitag);
            break;
        }
        let Some(group_tag) = group_tag else {
            return;
        };
        // Chocolate's operatesectors case 23 filters the swinging-door group
        // through `sprite[j].hitag == SHT`, where SHT is the linked
        // ACTIVATOR's hitag. A sector-only/CON activation has no activator
        // tag to compare and retains the direct-sector path.
        if activator_hitag.is_some_and(|hitag| hitag != group_tag) {
            return;
        }
        let open_bit = world.sectors[sector_index].lotag & i16::MIN;
        let mut called_sound = false;
        let mut marker = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
        while marker >= 0 {
            let index = marker as usize;
            marker = world.lists.nextspritestat[index];
            if world.sprites[index].lotag != 11 || world.sprites[index].hitag != group_tag {
                continue;
            }
            let Ok(marker_sector) = usize::try_from(world.sprites[index].sectnum) else {
                continue;
            };
            if marker_sector >= world.sectors.len()
                || world.sectors[marker_sector].lotag & i16::MIN != open_bit
                || actor_temps
                    .get(index)
                    .is_some_and(|temps| temps.g_t[4] != 0)
            {
                continue;
            }
            world.sectors[marker_sector].lotag ^= i16::MIN;
            if actor_temps.len() <= index {
                actor_temps.resize_with(index + 1, ActorConTemps::default);
            }
            actor_temps[index].g_t[3] = actor_temps[index].g_t[3].wrapping_neg();
            actor_temps[index].g_t[4] = 1;
            if !called_sound {
                call_sector_sound(world, world.sprites[index].sectnum, actor_temps);
                called_sound = true;
            }
        }
        return;
    }
    if kind == 25 {
        // Chocolate `operatesectors` case 25: the first SE15 (stat 3 order)
        // in this sector names the group; every SE15 sharing its hitag
        // toggles. Player use, activators and CON all take this path.
        let mut scan = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
        let mut group_tag = None;
        while scan >= 0 {
            let index = scan as usize;
            scan = world.lists.nextspritestat[index];
            if world.sprites[index].lotag == 15
                && usize::try_from(world.sprites[index].sectnum).ok() == Some(sector_index)
            {
                group_tag = Some(world.sprites[index].hitag);
                break;
            }
        }
        if let Some(group_tag) = group_tag {
            let mut marker = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
            while marker >= 0 {
                let index = marker as usize;
                marker = world.lists.nextspritestat[index];
                if world.sprites[index].lotag != 15 || world.sprites[index].hitag != group_tag {
                    continue;
                }
                let Ok(marker_sector) = usize::try_from(world.sprites[index].sectnum) else {
                    continue;
                };
                if marker_sector >= world.sectors.len() {
                    continue;
                }
                world.sectors[marker_sector].lotag ^= i16::MIN;
                world.sprites[index].ang = world.sprites[index].ang.wrapping_add(1024);
                if actor_temps.len() <= index {
                    actor_temps.resize_with(index + 1, ActorConTemps::default);
                }
                // Chocolate's SE15 slider uses T4 (temp_data[3]) for the
                // angular travel and T5 (temp_data[4]) for its active mode.
                // Do not use the angle slot to decide whether a prior move
                // was active: that makes the first activation sound behave
                // differently and overwrites the movement counter below.
                if actor_temps[index].g_t[4] != 0 {
                    call_sector_sound(world, world.sprites[index].sectnum, actor_temps);
                }
                call_sector_sound(world, world.sprites[index].sectnum, actor_temps);
                actor_temps[index].g_t[4] = if world.sectors[marker_sector].lotag & i16::MIN != 0 {
                    1
                } else {
                    2
                };
            }
            return;
        }
    }
    if kind == 27 {
        let mut local = world.lists.headspritesect[sector_index];
        while local >= 0 {
            let index = local as usize;
            local = world.lists.nextspritesect[index];
            if world.lists.sprite_stat[index] != crate::stat::STAT_EFFECTOR
                || world.sprites[index].lotag != 20
            {
                continue;
            }
            if actor_temps.len() <= index {
                actor_temps.resize_with(index + 1, ActorConTemps::default);
            }
            world.sectors[sector_index].lotag ^= i16::MIN;
            actor_temps[index].g_t[0] = if world.sectors[sector_index].lotag & i16::MIN != 0 {
                1
            } else {
                2
            };
            call_sector_sound(world, sector_idx, actor_temps);
            return;
        }
        return;
    }
    if kind == 31 {
        let mut local = world.lists.headspritesect[sector_index];
        while local >= 0 {
            let index = local as usize;
            local = world.lists.nextspritesect[index];
            if world.lists.sprite_stat[index] != crate::stat::STAT_EFFECTOR
                || world.sprites[index].lotag != 31
            {
                continue;
            }
            if actor_temps.len() <= index {
                actor_temps.resize_with(index + 1, ActorConTemps::default);
            }
            if actor_temps[index].g_t[4] == 0 {
                actor_temps[index].g_t[4] = 1;
            }
            call_sector_sound(world, sector_idx, actor_temps);
            return;
        }
        return;
    }
    let speed = i32::from(sector.extra).abs();
    if speed == 0 {
        return;
    }
    // Chocolate's split door (lotag 26) first checks the vertical door's
    // animation; repeated activation while it is moving is ignored.
    if kind == 26 && motions.iter().any(|motion| motion.sector == sector_index) {
        return;
    }
    if kind == 29 {
        let target = if sector.lotag & i16::MIN != 0 {
            neighbor_z(world, sector_idx, sector.ceilingz, true, true)
        } else {
            neighbor_z(world, sector_idx, sector.ceilingz, false, false)
        };
        let Some(target) = target.map(|neighbor| {
            if sector.lotag & i16::MIN != 0 {
                world.sectors[neighbor].floorz
            } else {
                world.sectors[neighbor].ceilingz
            }
        }) else {
            return;
        };
        let mut linked = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
        while linked >= 0 {
            let index = linked as usize;
            linked = world.lists.nextspritestat[index];
            if !world
                .sprites
                .get(index)
                .is_some_and(|marker| marker.lotag == 22 && marker.hitag == sector.hitag)
            {
                continue;
            }
            world.sprites[index].extra = world.sprites[index].extra.wrapping_neg();
            if actor_temps.len() <= index {
                actor_temps.resize_with(index + 1, ActorConTemps::default);
            }
            actor_temps[index].g_t[0] = i32::from(sector_idx);
            actor_temps[index].g_t[1] = 1;
        }
        if let Some(motion) = motions
            .iter_mut()
            .find(|motion| motion.sector == sector_index)
        {
            motion.floor_goal = None;
            motion.ceiling_goal = Some(target);
            motion.velocity = speed;
            world.sectors[sector_index].lotag ^= i16::MIN;
            call_sector_sound(world, sector_idx, actor_temps);
            return;
        }
        world.sectors[sector_index].lotag ^= i16::MIN;
        motions.push(SectorMotion {
            sector: sector_index,
            floor_goal: None,
            ceiling_goal: Some(target),
            velocity: speed,
        });
        call_sector_sound(world, sector_idx, actor_temps);
        return;
    }
    // Chocolate's sector-lotag 21 toggles an in-flight floor door by
    // replacing its current goal. Starting a second animation is not valid;
    // reverse the existing one from its current plane instead.
    if kind == 21
        && let Some(motion) = motions
            .iter_mut()
            .find(|motion| motion.sector == sector_index)
    {
        let closing = motion.floor_goal == Some(sector.ceilingz);
        let goal = if closing {
            let Some(neighbor) = neighbor_z(world, sector_idx, sector.floorz, true, true) else {
                return;
            };
            world.sectors[neighbor].floorz
        } else {
            sector.ceilingz
        };
        motion.floor_goal = Some(goal);
        motion.velocity = speed;
        return;
    }
    let (floor_goal, ceiling_goal, toggle_door) = match kind {
        16 | 17 => {
            let Some(neighbor) = neighbor_z(world, sector_idx, sector.floorz, true, true)
                .or_else(|| neighbor_z(world, sector_idx, sector.floorz, true, false))
            else {
                return;
            };
            (Some(world.sectors[neighbor].floorz), None, false)
        }
        18 | 19 => {
            let Some(neighbor) = neighbor_z(world, sector_idx, sector.floorz, true, false)
                .or_else(|| neighbor_z(world, sector_idx, sector.floorz, true, true))
            else {
                return;
            };
            let target = world.sectors[neighbor].floorz;
            let gap = sector.ceilingz.saturating_sub(sector.floorz);
            (Some(target), Some(target.saturating_add(gap)), false)
        }
        20 => {
            let target = if sector.lotag & i16::MIN != 0 {
                sector.floorz
            } else if let Some(neighbor) =
                neighbor_z(world, sector_idx, sector.ceilingz, false, false)
            {
                world.sectors[neighbor].ceilingz
            } else {
                sector.floorz
            };
            (None, Some(target), true)
        }
        21 => {
            let target = if sector.ceilingz == sector.floorz {
                let Some(neighbor) = neighbor_z(world, sector_idx, sector.floorz, true, true)
                else {
                    return;
                };
                world.sectors[neighbor].floorz
            } else {
                sector.ceilingz
            };
            (Some(target), None, true)
        }
        22 | 26 => {
            if sector.lotag & i16::MIN != 0 {
                let midpoint = sector.ceilingz.saturating_add(sector.floorz) / 2;
                (Some(midpoint), Some(midpoint), true)
            } else {
                let Some(floor_neighbor) = neighbor_z(world, sector_idx, sector.floorz, true, true)
                else {
                    return;
                };
                let Some(ceiling_neighbor) =
                    neighbor_z(world, sector_idx, sector.ceilingz, false, false)
                else {
                    return;
                };
                (
                    Some(world.sectors[floor_neighbor].floorz),
                    Some(world.sectors[ceiling_neighbor].ceilingz),
                    true,
                )
            }
        }
        _ => return,
    };
    if motions.iter().any(|motion| motion.sector == sector_index) {
        return;
    }
    if toggle_door {
        world.sectors[sector_index].lotag ^= i16::MIN;
    }
    motions.push(SectorMotion {
        sector: sector_index,
        floor_goal,
        ceiling_goal,
        velocity: speed,
    });
    if kind == 26 {
        operate_sliding_sector(
            world,
            sector_idx,
            i32::from(sector.extra >> 4),
            wall_motions,
        );
    }
    call_sector_sound(world, sector_idx, actor_temps);
}

/// Queue Chocolate `callsound`'s MUSICANDSFX on/off cue for a sector event.
pub(crate) fn call_sector_sound(
    world: &BuildWorld,
    sector_idx: i16,
    actor_temps: &mut Vec<ActorConTemps>,
) -> bool {
    let Ok(sector_index) = usize::try_from(sector_idx) else {
        return false;
    };
    let Some(sector) = world.sectors.get(sector_index) else {
        return false;
    };
    let mut sprite = world.lists.headspritesect[sector_index];
    let mut handled = false;
    while sprite >= 0 {
        let index = sprite as usize;
        sprite = world.lists.nextspritesect[index];
        let Some(marker) = world.sprites.get(index) else {
            continue;
        };
        if marker.picnum != crate::spawn::MUSICANDSFX || marker.lotag >= 1000 {
            continue;
        }
        if actor_temps.len() <= index {
            actor_temps.resize_with(index + 1, ActorConTemps::default);
        }
        let on_sound = marker.lotag;
        let off_sound = marker.hitag;
        handled = on_sound != -1;
        let sound_flags = actor_temps[index].g_t[3];
        if actor_temps[index].g_t[0] == 0 {
            if sound_flags & 16 == 0 {
                if on_sound > 0 {
                    actor_temps[index].pending_actor_sounds.push(on_sound);
                }
                if off_sound > 0 && off_sound != on_sound {
                    actor_temps[index].pending_sound_stops.push(off_sound);
                }
            }
            if sector.lotag & 0xff != 22 {
                actor_temps[index].g_t[0] = 1;
            }
        } else {
            if off_sound > 0 {
                actor_temps[index].pending_actor_sounds.push(off_sound);
            }
            if sound_flags & 1 != 0 || (off_sound > 0 && off_sound != on_sound) {
                actor_temps[index].pending_sound_stops.push(on_sound);
            }
            actor_temps[index].g_t[0] = 0;
        }
        break;
    }
    handled
}

/// Activate Chocolate's sector-lotag 15 / SE17 elevator group.
fn activate_warp_elevator(
    world: &BuildWorld,
    sector_idx: i16,
    actor_temps: &mut Vec<ActorConTemps>,
    player: Option<&Player>,
) {
    let Some(player) = player else {
        return;
    };
    let Ok(sector_index) = usize::try_from(sector_idx) else {
        return;
    };
    let Some(sector) = world.sectors.get(sector_index) else {
        return;
    };
    let mut marker = world.lists.headspritesect[sector_index];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritesect[index];
        let Some(effector) = world.sprites.get(index).copied() else {
            continue;
        };
        if world.lists.sprite_stat[index] != crate::stat::STAT_EFFECTOR || effector.lotag != 17 {
            continue;
        }
        let player_in_sector = player.cursectnum == sector_idx;
        let preferred = if player_in_sector {
            -1
        } else if sector.floorz > player.posz {
            -1
        } else {
            1
        };
        // Chocolate `activatewarpelevators` succeeds either when a linked
        // destination sector exists in the requested direction or when any
        // elevator in this hitag group has already left its saved stop floor.
        // The latter lets a switch reverse an in-flight lift without waiting
        // for another destination lookup to succeed.
        let can_activate = |direction: i32| {
            let mut candidate = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
            while candidate >= 0 {
                let ci = candidate as usize;
                candidate = world.lists.nextspritestat[ci];
                let Some(other) = world.sprites.get(ci) else {
                    continue;
                };
                if other.lotag != 17 || other.hitag != effector.hitag {
                    continue;
                }
                let Ok(other_sector_index) = usize::try_from(other.sectnum) else {
                    continue;
                };
                let Some(other_sector) = world.sectors.get(other_sector_index) else {
                    continue;
                };
                let moving = actor_temps.get(ci).is_some_and(|temp| {
                    other_sector.floorz.abs_diff(temp.g_t[2]) > i32::from(other.yvel).unsigned_abs()
                });
                if moving
                    || i32::from(other_sector.hitag)
                        == i32::from(sector.hitag).saturating_sub(direction)
                {
                    return true;
                }
            }
            false
        };
        // `operatesectors` tries both directions for a player standing in the
        // elevator sector, but only the height-selected direction otherwise.
        let direction = if player_in_sector {
            if can_activate(preferred) {
                preferred
            } else if can_activate(-preferred) {
                -preferred
            } else {
                return;
            }
        } else if can_activate(preferred) {
            preferred
        } else {
            return;
        };
        if actor_temps.len() <= index {
            actor_temps.resize_with(index + 1, ActorConTemps::default);
        }
        // Chocolate's activatewarpelevators(..., direction) emits the
        // movement cue at the selected SE17 marker.
        actor_temps[index].pending_actor_sounds.push(71);
        let mut paired = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
        while paired >= 0 {
            let index = paired as usize;
            paired = world.lists.nextspritestat[index];
            if world
                .sprites
                .get(index)
                .is_some_and(|other| other.lotag == 17 && other.hitag == effector.hitag)
            {
                if actor_temps.len() <= index {
                    actor_temps.resize_with(index + 1, ActorConTemps::default);
                }
                actor_temps[index].g_t[0] = direction;
                actor_temps[index].g_t[1] = direction;
            }
        }
        return;
    }
}

/// Chocolate `moveactors` master-switch countdown and delayed sector operation.
pub(crate) fn tick_master_switches(
    world: &mut BuildWorld,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    actor_temps: &mut Vec<crate::con::ActorConTemps>,
    picnum: &mut Vec<i16>,
) {
    let mut i = world.lists.headspritestat[crate::stat::STAT_STANDABLE as usize];
    while i >= 0 {
        let index = i as usize;
        i = world.lists.nextspritestat[index];
        if world.sprites[index].picnum != crate::spawn::MASTERSWITCH
            || world.sprites[index].yvel != 1
        {
            continue;
        }
        world.sprites[index].hitag = world.sprites[index].hitag.wrapping_sub(1);
        if world.sprites[index].hitag > 0 {
            continue;
        }
        let sector_idx = world.sprites[index].sectnum;
        operate_sector_motion(
            world,
            motions,
            wall_motions,
            sector_idx,
            actor_temps,
            None,
            None,
        );

        if let Ok(sector_index) = usize::try_from(sector_idx)
            && world.sectors.get(sector_index).is_some()
            && let Some(mut j) = world.lists.headspritesect.get(sector_index).copied()
        {
            while j >= 0 {
                let object = j as usize;
                j = world.lists.nextspritesect[object];
                if world.lists.sprite_stat[object] == crate::stat::STAT_EFFECTOR {
                    match world.sprites[object].lotag {
                        2 | 18 | 21 | 31 | 32 | 36 => {
                            if actor_temps.len() <= object {
                                actor_temps.resize_with(object + 1, Default::default);
                            }
                            if world.sprites[object].lotag == 36 {
                                // Chocolate `operatesectors`: SE36 is an
                                // on/off shooter, unlike the one-shot plane
                                // effectors that are simply armed here.
                                actor_temps[object].g_t[0] =
                                    1_i32.wrapping_sub(actor_temps[object].g_t[0]);
                            } else {
                                actor_temps[object].g_t[0] = 1;
                            }
                        }
                        3 => {
                            if actor_temps.len() <= object {
                                actor_temps.resize_with(object + 1, Default::default);
                            }
                            actor_temps[object].g_t[4] = 1;
                        }
                        _ => {}
                    }
                } else if world.lists.sprite_stat[object] == crate::stat::STAT_STANDABLE
                    && matches!(
                        world.sprites[object].picnum,
                        crate::spawn::SEENINE | crate::spawn::OOZFILTER
                    )
                {
                    world.sprites[object].shade = -31;
                }
            }
        }
        let _ = world.lists.deletesprite(index as i16);
        if picnum.len() <= index {
            picnum.resize(index + 1, 0);
        }
        picnum[index] = -1;
    }
}

/// Chocolate `movefx` countdown and target restoration for activated RESPAWN markers.
pub(crate) fn tick_respawns(
    world: &mut BuildWorld,
    picnum: &mut Vec<i16>,
    actor_temps: &mut Vec<crate::con::ActorConTemps>,
    script: Option<&crate::con::ScriptImage>,
    tile_meta: &[crate::player::TileZMeta],
) {
    let mut i = world.lists.headspritestat[crate::stat::STAT_FX as usize];
    while i >= 0 {
        let index = i as usize;
        i = world.lists.nextspritestat[index];
        if world.sprites[index].picnum != crate::spawn::RESPAWN {
            continue;
        }
        if world.sprites[index].extra == 54
            && !actor_temps
                .get(index)
                .is_some_and(|temps| temps.respawn_star_spawned)
        {
            let marker = world.sprites[index];
            let star = crate::shoot::egs(
                &mut world.lists,
                world.sprites.as_mut(),
                picnum,
                actor_temps,
                script,
                marker.sectnum,
                marker.x,
                marker.y,
                marker.z.saturating_sub(32 << 8),
                crate::spawn::TRANSPORTERSTAR,
                -127,
                48,
                64,
                marker.ang,
                128,
                0,
                index as i16,
                crate::stat::STAT_ACTOR,
            );
            if star >= 0 {
                let sprite = &mut world.sprites[star as usize];
                sprite.cstat = 130;
                if let Some(image) = script
                    && image
                        .actorscrptr
                        .get(crate::spawn::TRANSPORTERSTAR as usize)
                        .copied()
                        .unwrap_or(0)
                        <= 0
                {
                    sprite.xrepeat = 0;
                    sprite.yrepeat = 0;
                }
            }
            if actor_temps.len() <= index {
                actor_temps.resize_with(index + 1, ActorConTemps::default);
            }
            actor_temps[index].respawn_star_spawned = true;
        }
        if world.sprites[index].extra == 66 {
            let marker = world.sprites[index];
            if marker.hitag > 0 {
                let spawned = crate::shoot::egs(
                    &mut world.lists,
                    world.sprites.as_mut(),
                    picnum,
                    actor_temps,
                    script,
                    marker.sectnum,
                    marker.x,
                    marker.y,
                    marker.z,
                    marker.hitag,
                    marker.shade,
                    0,
                    0,
                    marker.ang,
                    0,
                    0,
                    index as i16,
                    crate::stat::STAT_DEFAULT,
                );
                if spawned >= 0
                    && let Some(image) = script
                {
                    let child = spawned as usize;
                    let tile = marker.hitag as usize;
                    let actor_entry = image.actorscrptr.get(tile).copied().unwrap_or(0);
                    let actor_type = image.actortype.get(tile).copied().unwrap_or(0);
                    if crate::badguy::badguy_pic(marker.hitag, image.actortype.as_slice()) {
                        // Chocolate `spawn()` badguy case: full size, hitscan
                        // cstat, clipdist, pal from the RESPAWN marker.
                        crate::spawn_rt::init_spawned_badguy(
                            &mut world.lists,
                            world.sprites.as_mut(),
                            &world.sectors,
                            &world.walls,
                            actor_temps,
                            tile_meta,
                            image,
                            spawned,
                            index as i16,
                        );
                    } else if actor_entry > 0 {
                        let badguy = actor_type & 3 != 0;
                        let sprite = &mut world.sprites[child];
                        sprite.xrepeat = sprite.xrepeat.max(1);
                        sprite.yrepeat = sprite.yrepeat.max(1);
                        sprite.pal = if badguy { marker.pal } else { 0 };
                        sprite.clipdist = if badguy { 80 } else { 40 };
                        if badguy {
                            actor_temps[child].tempang = marker.pal as i16;
                        }
                        if !badguy {
                            sprite.owner = spawned;
                        }
                        let _ = world
                            .lists
                            .changespritestat(spawned, crate::stat::STAT_ACTOR);
                        if badguy {
                            if actor_type & 2 != 0 {
                                actor_temps[child].actorstayput = marker.sectnum;
                            }
                            let peer_snapshot = world.sprites.to_vec();
                            crate::spawn::makeitfall(
                                &mut world.sprites[child],
                                &world.sectors,
                                &world.walls,
                                &peer_snapshot,
                                tile_meta,
                                crate::player::DUKE_GC,
                                false,
                            );
                        }
                    } else {
                        let target_stat = crate::spawn::classify_stat(
                            &world.sprites[child],
                            &image.actorscrptr,
                            &image.actortype,
                        );
                        if target_stat != crate::stat::STAT_DEFAULT {
                            let _ = world.lists.changespritestat(spawned, target_stat);
                        }
                    }
                }
            }
            let _ = world.lists.deletesprite(index as i16);
            if let Some(temps) = actor_temps.get_mut(index) {
                temps.respawn_star_spawned = false;
            }
            if picnum.len() <= index {
                picnum.resize(index + 1, 0);
            }
            picnum[index] = -1;
        } else if world.sprites[index].extra > 53 {
            world.sprites[index].extra += 1;
        }
    }
}

fn operate_sliding_sector(
    world: &BuildWorld,
    sector_idx: i16,
    speed: i32,
    motions: &mut Vec<WallMotion>,
) {
    if speed <= 0 {
        return;
    }
    let Some(sector) = world.sectors.get(sector_idx.max(0) as usize) else {
        return;
    };
    if sector.wallnum < 3 {
        return;
    }
    let start = sector.wallptr.max(0) as usize;
    let end = start
        .saturating_add(sector.wallnum as usize)
        .min(world.walls.len());
    if end <= start {
        return;
    }
    let mut x_sum = 0_i32;
    let mut y_sum = 0_i32;
    for wall in &world.walls[start..end] {
        x_sum = x_sum.wrapping_add(wall.x);
        y_sum = y_sum.wrapping_add(wall.y);
    }
    let count = (end - start) as i32;
    let center_x = x_sum / count;
    let center_y = y_sum / count;
    let center_walls: Vec<usize> = (start..end)
        .filter(|&i| world.walls[i].x == center_x || world.walls[i].y == center_y)
        .take(2)
        .collect();
    if center_walls.len() < 2 {
        return;
    }

    for wall_idx in center_walls {
        let wall = world.walls[wall_idx];
        let point2 = wall.point2;
        if point2 < 0 || point2 as usize >= world.walls.len() {
            continue;
        }
        let point2 = point2 as usize;
        let point2_next = world.walls[point2].point2;
        if point2_next < 0 || point2_next as usize >= world.walls.len() {
            continue;
        }
        let point2_next = point2_next as usize;
        let previous = if wall_idx == start {
            end - 1
        } else {
            wall_idx - 1
        };
        let previous_wall = world.walls[previous];

        if wall.x == center_x && wall.y == center_y {
            let dx =
                ((previous_wall.x.wrapping_add(world.walls[point2].x)) >> 1).wrapping_sub(wall.x);
            let dy =
                ((previous_wall.y.wrapping_add(world.walls[point2].y)) >> 1).wrapping_sub(wall.y);
            if dx != 0 {
                let delta = world.walls[point2_next]
                    .x
                    .wrapping_sub(world.walls[point2].x);
                queue_wall_goal(
                    motions,
                    sector_idx,
                    wall_idx,
                    WallAxis::X,
                    wall.x.wrapping_add(delta),
                    speed,
                );
                queue_wall_goal(
                    motions,
                    sector_idx,
                    previous,
                    WallAxis::X,
                    previous_wall.x.wrapping_add(delta),
                    speed,
                );
                queue_wall_goal(
                    motions,
                    sector_idx,
                    point2,
                    WallAxis::X,
                    world.walls[point2].x.wrapping_add(delta),
                    speed,
                );
            } else if dy != 0 {
                let delta = world.walls[point2_next]
                    .y
                    .wrapping_sub(world.walls[point2].y);
                queue_wall_goal(
                    motions,
                    sector_idx,
                    wall_idx,
                    WallAxis::Y,
                    wall.y.wrapping_add(delta),
                    speed,
                );
                queue_wall_goal(
                    motions,
                    sector_idx,
                    previous,
                    WallAxis::Y,
                    previous_wall.y.wrapping_add(delta),
                    speed,
                );
                queue_wall_goal(
                    motions,
                    sector_idx,
                    point2,
                    WallAxis::Y,
                    world.walls[point2].y.wrapping_add(delta),
                    speed,
                );
            }
        } else {
            let dx =
                ((previous_wall.x.wrapping_add(world.walls[point2].x)) >> 1).wrapping_sub(wall.x);
            let dy =
                ((previous_wall.y.wrapping_add(world.walls[point2].y)) >> 1).wrapping_sub(wall.y);
            if dx != 0 {
                queue_wall_goal(motions, sector_idx, wall_idx, WallAxis::X, center_x, speed);
                queue_wall_goal(
                    motions,
                    sector_idx,
                    previous,
                    WallAxis::X,
                    center_x.wrapping_add(dx),
                    speed,
                );
                queue_wall_goal(
                    motions,
                    sector_idx,
                    point2,
                    WallAxis::X,
                    center_x.wrapping_add(dx),
                    speed,
                );
            } else if dy != 0 {
                queue_wall_goal(motions, sector_idx, wall_idx, WallAxis::Y, center_y, speed);
                queue_wall_goal(
                    motions,
                    sector_idx,
                    previous,
                    WallAxis::Y,
                    center_y.wrapping_add(dy),
                    speed,
                );
                queue_wall_goal(
                    motions,
                    sector_idx,
                    point2,
                    WallAxis::Y,
                    center_y.wrapping_add(dy),
                    speed,
                );
            }
        }
    }
}

fn queue_wall_goal(
    motions: &mut Vec<WallMotion>,
    sector: i16,
    wall: usize,
    axis: WallAxis,
    goal: i32,
    velocity: i32,
) {
    if let Some(existing) = motions
        .iter_mut()
        .find(|m| m.wall == wall && m.axis == axis)
    {
        existing.goal = goal;
        existing.velocity = velocity;
        existing.sector = sector as usize;
    } else {
        motions.push(WallMotion {
            sector: sector as usize,
            wall,
            axis,
            goal,
            velocity,
        });
    }
}

fn neighbor_z(
    world: &BuildWorld,
    sector: i16,
    z: i32,
    floor: bool,
    direction_down: bool,
) -> Option<usize> {
    let source = world.sectors.get(sector.max(0) as usize)?;
    let start = source.wallptr.max(0) as usize;
    let end = start
        .saturating_add(source.wallnum.max(0) as usize)
        .min(world.walls.len());
    let mut best: Option<(i32, usize)> = None;
    for wall in &world.walls[start..end] {
        let neighbor = wall.nextsector;
        if neighbor < 0 || neighbor == sector {
            continue;
        }
        let Some(candidate) = world.sectors.get(neighbor as usize) else {
            continue;
        };
        let value = if floor {
            candidate.floorz
        } else {
            candidate.ceilingz
        };
        let eligible = if direction_down { value > z } else { value < z };
        if eligible
            && best.is_none_or(|(old, _)| {
                if direction_down {
                    value < old
                } else {
                    value > old
                }
            })
        {
            best = Some((value, neighbor as usize));
        }
    }
    best.map(|(_, sector)| sector)
}

/// Find a tagged wall switch in the player's view, toggle linked switch art,
/// and return its wall index. Sector/activator execution is handled by D7.
pub(crate) fn use_wall_switch(
    world: &mut BuildWorld,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    cyclers: &mut [ShadeCycler],
    x: i32,
    y: i32,
    z: i32,
    angle: i16,
    sector: i16,
    level_exit_ready: &mut bool,
    player: &mut Player,
    tile_meta: &[crate::player::TileZMeta],
    anim_walls: &mut [AnimWall],
    monsters_off: bool,
    actor_types: &[u8],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_state: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
) -> Option<i16> {
    let sectors: Vec<_> = world
        .sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sector_z: Vec<_> = world
        .sectors
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
    let walls: Vec<_> = world
        .walls
        .iter()
        .map(|w| WallXY {
            x: w.x,
            y: w.y,
            point2: w.point2,
            nextsector: w.nextsector,
            cstat: w.cstat,
        })
        .collect();
    let tags: Vec<_> = world.walls.iter().map(|w| w.lotag).collect();
    // Chocolate `checksectors` retries `neartag` at the eye (`posz`), then
    // `posz + 8<<8`, then `posz + 16<<8` until something tagged is found.
    // Callers pass the last of these heights.
    let mut wall_target = None;
    let mut sprite_target = None;
    for dz in [-(16 << 8), -(8 << 8), 0] {
        let probe_z = z + dz;
        wall_target = neartag_wall(
            x, y, probe_z, sector, angle, USE_RANGE, &sectors, &walls, &tags,
        );
        sprite_target = nearest_tagged_sprite_switch(
            world,
            x,
            y,
            probe_z,
            sector,
            angle,
            USE_RANGE,
            tile_meta,
            &sector_z,
            &walls,
            NearSpriteFilter::Switch,
        );
        if wall_target.is_some() || sprite_target.is_some() {
            break;
        }
    }
    let use_sprite = sprite_target.is_some_and(|(_, distance)| {
        wall_target.is_none_or(|(_, wall_distance)| distance < wall_distance)
    });
    if wall_target.is_none() && sprite_target.is_none() {
        if world
            .sectors
            .get(sector.max(0) as usize)
            .is_some_and(|sector| sector.lotag & 0x3fff == 15)
        {
            activate_warp_elevator(world, sector, actor_temps, Some(player));
        }
        return None;
    }
    let (target, lotag, picnum, palette) = if use_sprite {
        let (index, _) = sprite_target?;
        let sprite = world.sprites.get(index as usize)?;
        (index, sprite.lotag, sprite.picnum, sprite.pal)
    } else {
        let (index, _) = wall_target?;
        let wall = world.walls.get(index as usize)?;
        (index, wall.lotag, wall.picnum, wall.pal)
    };
    if lotag == -1 {
        *level_exit_ready = true;
        return None;
    }
    if lotag <= 0 || (!is_switch(picnum) && !is_door_operator_wall(picnum)) {
        return None;
    }
    if player.access_incs != 0 {
        return None;
    }
    if is_switch(picnum) && is_access_switch(picnum) {
        let bit = access_bit_for_palette(palette);
        if bit == 0 || player.got_access & bit == 0 {
            if player.fta_enabled {
                player.quote_id = match palette {
                    0 => 70,
                    21 => 71,
                    23 => 72,
                    _ => 0,
                };
                player.quote_timer = 100;
            }
            return None;
        }
        player.access_incs = 1;
        player.access_wallnum = if use_sprite { -1 } else { target };
        player.access_spritenum = if use_sprite { target } else { -1 };
        return None;
    }

    if switch_waits_for_activator(picnum)
        && tagged_activator_is_moving(world, lotag, motions, wall_motions, actor_temps)
    {
        return None;
    }

    activate_switch(
        world,
        motions,
        wall_motions,
        cyclers,
        player,
        lotag,
        use_sprite.then_some(target),
        (!use_sprite).then_some(target),
        anim_walls,
        monsters_off,
        actor_types,
        actor_temps,
        picnum_state,
        script,
    )
}

/// Chocolate `checkhitswitch(player, wall, 0)` entry for a bullet impact.
/// Uses the exact wall hit by hitscan rather than running a new use-ray search.
pub(crate) fn hit_switch_target(
    world: &mut BuildWorld,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    cyclers: &mut [ShadeCycler],
    wall_index: Option<i16>,
    sprite_index: Option<i16>,
    level_exit_ready: &mut bool,
    player: &mut Player,
    anim_walls: &mut [AnimWall],
    monsters_off: bool,
    actor_types: &[u8],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum_state: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
) -> Option<i16> {
    let (lotag, switch_picnum, palette) = if let Some(wall_index) = wall_index {
        let wall = world.walls.get(wall_index as usize)?;
        (wall.lotag, wall.picnum, wall.pal)
    } else {
        let sprite = world.sprites.get(sprite_index? as usize)?;
        (sprite.lotag, sprite.picnum, sprite.pal)
    };
    if lotag == -1 {
        *level_exit_ready = true;
        return None;
    }
    if lotag <= 0
        || (!is_switch(switch_picnum) && !is_door_operator_wall(switch_picnum))
        || player.access_incs != 0
    {
        return None;
    }
    if is_switch(switch_picnum) && is_access_switch(switch_picnum) {
        let bit = access_bit_for_palette(palette);
        if bit == 0 || player.got_access & bit == 0 {
            if player.fta_enabled {
                player.quote_id = match palette {
                    0 => 70,
                    21 => 71,
                    23 => 72,
                    _ => 0,
                };
                player.quote_timer = 100;
            }
            return None;
        }
        player.access_incs = 1;
        player.access_wallnum = wall_index.unwrap_or(-1);
        player.access_spritenum = sprite_index.unwrap_or(-1);
        return None;
    }
    if switch_waits_for_activator(switch_picnum)
        && tagged_activator_is_moving(world, lotag, motions, wall_motions, actor_temps)
    {
        return None;
    }
    activate_switch(
        world,
        motions,
        wall_motions,
        cyclers,
        player,
        lotag,
        sprite_index,
        wall_index,
        anim_walls,
        monsters_off,
        actor_types,
        actor_temps,
        picnum_state,
        script,
    )
}

/// Chocolate `check_activator_motion`: reject switch tags whose ACTIVATOR
/// sector still has a native plane/wall animation or active motion effector.
fn tagged_activator_is_moving(
    world: &BuildWorld,
    lotag: i16,
    motions: &[SectorMotion],
    wall_motions: &[WallMotion],
    actor_temps: &[ActorConTemps],
) -> bool {
    let mut activator = world.lists.headspritestat[crate::stat::STAT_ACTIVATOR as usize];
    while activator >= 0 {
        let activator_index = activator as usize;
        activator = world.lists.nextspritestat[activator_index];
        let Some(actor) = world.sprites.get(activator_index) else {
            continue;
        };
        if actor.lotag != lotag {
            continue;
        }
        let Ok(sector) = usize::try_from(actor.sectnum) else {
            continue;
        };
        if motions.iter().any(|motion| motion.sector == sector)
            || wall_motions.iter().any(|motion| motion.sector == sector)
        {
            return true;
        }
        let mut effector = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
        while effector >= 0 {
            let effector_index = effector as usize;
            effector = world.lists.nextspritestat[effector_index];
            let Some(marker) = world.sprites.get(effector_index) else {
                continue;
            };
            if marker.sectnum != actor.sectnum {
                continue;
            }
            let Some(temp) = actor_temps.get(effector_index) else {
                continue;
            };
            if (matches!(marker.lotag, 11 | 30) && temp.g_t[4] != 0)
                || (matches!(marker.lotag, 18 | 20 | 31 | 32) && temp.g_t[0] != 0)
            {
                return true;
            }
        }
    }
    false
}

/// Switch families gated by `check_activator_motion`; dip, tech, alien, and
/// access-card switches take their dedicated paths without this check.
fn switch_waits_for_activator(picnum: i16) -> bool {
    matches!(
        picnum,
        132..=149 | 164..=165 | 168..=169 | 712..=713 | 860..=865 | 1111..=1112 | 1122..=1123
    )
}

/// Complete the switch part of Chocolate `checkhitswitch` after any access-card animation.
fn activate_switch(
    world: &mut BuildWorld,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    cyclers: &mut [ShadeCycler],
    player: &mut Player,
    lotag: i16,
    target_sprite: Option<i16>,
    target_wall: Option<i16>,
    anim_walls: &mut [AnimWall],
    monsters_off: bool,
    actor_types: &[u8],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
) -> Option<i16> {
    let switch_picnum = target_wall
        .and_then(|index| world.walls.get(index as usize).map(|wall| wall.picnum))
        .or_else(|| {
            target_sprite.and_then(|index| {
                world
                    .sprites
                    .get(index as usize)
                    .map(|sprite| sprite.picnum)
            })
        })
        .unwrap_or(0);
    let directional_dip = matches!(switch_picnum, 162 | 163 | 166 | 167 | 1142 | 1143);
    for (index, wall) in world.walls.iter_mut().enumerate() {
        if wall.lotag == lotag {
            if !directional_dip || target_wall == i16::try_from(index).ok() {
                wall.picnum = next_switch_frame(wall.picnum).unwrap_or(wall.picnum);
            }
        }
    }
    for (index, sprite) in world.sprites.iter_mut().enumerate() {
        if sprite.lotag == lotag && is_switch(sprite.picnum) {
            if !directional_dip || target_sprite == i16::try_from(index).ok() {
                sprite.picnum = next_switch_frame(sprite.picnum).unwrap_or(sprite.picnum);
            }
        }
    }
    // Chocolate toggles matching SE24/SE34 T5 before operating activators.
    toggle_tagged_conveyors(world, actor_temps, player, lotag);
    operate_activators(
        world,
        motions,
        wall_motions,
        cyclers,
        lotag,
        Some(player),
        actor_temps,
        monsters_off,
        actor_types,
        picnum,
        script,
    );
    crate::wall_animations::operate_forcefields(&mut world.walls, anim_walls, lotag);
    // Chocolate `checkhitswitch` calls `operatemasterswitches(lotag)` after
    // activating the tagged sectors. A master switch is a one-shot marker:
    // `yvel == 0` means armed, and the first matching switch sets it to 1.
    arm_master_switches(world, lotag);
    // Chocolate `checkhitswitch` advances every matching SE 12's light state.
    toggle_tagged_se12(world, actor_temps, lotag);
    if is_door_operator_wall(switch_picnum) && !is_switch(switch_picnum) {
        None
    } else {
        target_sprite.or_else(|| {
            world
                .walls
                .iter()
                .position(|wall| wall.lotag == lotag)
                .and_then(|index| i16::try_from(index).ok())
        })
    }
}

/// Chocolate `checkhitswitch` transition for tagged sector-light SE 12s.
fn toggle_tagged_se12(world: &mut BuildWorld, temps: &mut Vec<ActorConTemps>, hitag: i16) {
    // Walk the live STAT_EFFECTOR list: `SpriteT::statnum` keeps the map-file
    // value (0 for every effector) and is never updated by `SpriteLists`.
    let mut next = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
    while next >= 0 {
        let index = next as usize;
        next = world.lists.nextspritestat[index];
        let Some(marker) = world.sprites.get(index).copied() else {
            continue;
        };
        if marker.lotag != 12 || marker.hitag != hitag {
            continue;
        }
        if let Ok(sector_index) = usize::try_from(marker.sectnum)
            && let Some(sector) = world.sectors.get_mut(sector_index)
        {
            sector.floorpal = 0;
        }
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        temps[index].g_t[0] = temps[index].g_t[0].wrapping_add(1);
        if temps[index].g_t[0] == 2 {
            temps[index].g_t[0] = temps[index].g_t[0].wrapping_add(1);
        }
    }
}

/// Chocolate `checkhitswitch` toggles matching SE24/SE34 conveyor T5. A
/// nonzero T5 pauses the conveyor in `moveeffectors`.
fn toggle_tagged_conveyors(
    world: &BuildWorld,
    temps: &mut Vec<ActorConTemps>,
    player: &mut Player,
    hitag: i16,
) {
    let mut marker = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
    while marker >= 0 {
        let index = marker as usize;
        marker = world.lists.nextspritestat[index];
        let Some(sprite) = world.sprites.get(index) else {
            continue;
        };
        if !matches!(sprite.lotag, 24 | 34) || sprite.hitag != hitag {
            continue;
        }
        if temps.len() <= index {
            temps.resize_with(index + 1, ActorConTemps::default);
        }
        temps[index].g_t[4] = if temps[index].g_t[4] == 0 { 1 } else { 0 };
        player.quote_id = if temps[index].g_t[4] == 0 { 15 } else { 2 };
        player.quote_timer = 100;
    }
}

fn is_access_switch(picnum: i16) -> bool {
    matches!(picnum, 130 | 131 | 170 | 171)
}

/// Chocolate `neartag` sprite test for tagged switches in sectors visible through
/// connected portals, constrained by the forward use ray and wall occlusion.
fn nearest_tagged_sprite_switch(
    world: &BuildWorld,
    x: i32,
    y: i32,
    z: i32,
    sector: i16,
    angle: i16,
    range: i32,
    tile_meta: &[crate::player::TileZMeta],
    sectors: &[SectorZ],
    walls: &[WallXY],
    filter: NearSpriteFilter,
) -> Option<(i16, i32)> {
    let (sin, cos) = approx_sincos_q14(i32::from(angle));
    let vx = i64::from(cos) * i64::from(range) >> 14;
    let vy = i64::from(sin) * i64::from(range) >> 14;
    let bot = i128::from(vx) * i128::from(vx) + i128::from(vy) * i128::from(vy);
    if bot == 0 {
        return None;
    }
    let mut best: Option<(i16, i32)> = None;
    for (index, sprite) in world.sprites.iter().enumerate() {
        if world
            .lists
            .sprite_stat
            .get(index)
            .is_none_or(|stat| *stat == room_build_engine::world::MAXSTATUS as i16)
            || match filter {
                NearSpriteFilter::NukeButton => sprite.picnum != crate::spawn::NUKEBUTTON,
                NearSpriteFilter::Switch => sprite.lotag <= 0 || !is_switch(sprite.picnum),
                // `neartag(…, 1)`: any lotag-tagged utility prop.
                NearSpriteFilter::Utility => {
                    sprite.lotag == 0
                        || !matches!(
                            sprite.picnum,
                            crate::spawn::TOILET | crate::spawn::STALL | crate::spawn::WATERFOUNTAIN
                                ..=566 | crate::spawn::PLUG
                        )
                }
            }
        {
            continue;
        }
        let meta = tile_meta
            .get(sprite.picnum.max(0) as usize)
            .copied()
            .unwrap_or_default();
        let rel_x = i64::from(sprite.x) - i64::from(x);
        let rel_y = i64::from(sprite.y) - i64::from(y);
        let top = i128::from(vx) * i128::from(rel_x) + i128::from(vy) * i128::from(rel_y);
        if top <= 0 || top > bot {
            continue;
        }
        let height = i64::from(meta.height.max(0)) * i64::from(sprite.yrepeat);
        let mut top_z = i64::from(sprite.z);
        if sprite.cstat & 128 != 0 {
            top_z += height << 1;
        }
        let animated_y_offset = i64::from(meta.y_offset) + i64::from(sprite.yoffset);
        top_z -= (animated_y_offset * i64::from(sprite.yrepeat)) << 2;
        if i64::from(z) > top_z || i64::from(z) < top_z - (height << 2) {
            continue;
        }
        let target_z =
            (top_z - (height << 1)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        if !cansee(
            x,
            y,
            z,
            sector,
            sprite.x,
            sprite.y,
            target_z,
            sprite.sectnum,
            sectors,
            walls,
        ) {
            continue;
        }
        let cross = i128::from(vx) * i128::from(rel_y) - i128::from(vy) * i128::from(rel_x);
        let width = i128::from(meta.width.max(0)) * i128::from(sprite.xrepeat);
        if cross * cross * 128 > bot * width * width {
            continue;
        }
        let hit_x = i64::from(x) + (i128::from(vx) * top / bot) as i64;
        let hit_y = i64::from(y) + (i128::from(vy) * top / bot) as i64;
        let distance = length2d((hit_x - i64::from(x)) as i32, (hit_y - i64::from(y)) as i32);
        if best.is_none_or(|(_, best_distance)| distance < best_distance) {
            best = i16::try_from(index)
                .ok()
                .map(|sprite_index| (sprite_index, distance));
        }
    }
    best
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NearSpriteFilter {
    Switch,
    NukeButton,
    Utility,
}

/// What a `use` on a toilet, urinal stall, water fountain or wall plug did,
/// so the session can queue the matching Chocolate sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UtilityUse {
    /// `DUKE_URINATE`: holster, +10% health, start `last_pissed_time`.
    Urinate,
    /// Used again during the cooldown: `FLUSH_TOILET`.
    Flush,
    /// One health point per press below max: `DUKE_DRINKING`.
    Drink,
    /// Drinking at full health: nothing audible, but the use is consumed.
    FountainFull,
    /// `SHORT_CIRCUIT` shock: 2..5 damage and a blue flash.
    Shock,
}

/// Chocolate `checksectors` sprite cases TOILET/STALL, WATERFOUNTAIN, PLUG.
#[allow(clippy::too_many_arguments)]
pub(crate) fn use_utility_sprite(
    world: &mut BuildWorld,
    x: i32,
    y: i32,
    z: i32,
    angle: i16,
    sector: i16,
    player: &mut Player,
    tile_meta: &[crate::player::TileZMeta],
    actor_temps: &mut Vec<ActorConTemps>,
    max_player_health: i16,
    randomseed: &mut i32,
) -> Option<UtilityUse> {
    let sectors: Vec<_> = world
        .sectors
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
    let walls: Vec<_> = world
        .walls
        .iter()
        .map(|w| WallXY {
            x: w.x,
            y: w.y,
            point2: w.point2,
            nextsector: w.nextsector,
            cstat: w.cstat,
        })
        .collect();
    // Chocolate probes at the eye, +8 and +16 units (`z` is eye + 16).
    let target = [-(16 << 8), -(8 << 8), 0].into_iter().find_map(|dz| {
        nearest_tagged_sprite_switch(
            world,
            x,
            y,
            z + dz,
            sector,
            angle,
            USE_RANGE,
            tile_meta,
            &sectors,
            &walls,
            NearSpriteFilter::Utility,
        )
    });
    let index = usize::try_from(target?.0).ok()?;
    let picnum = world.sprites.get(index)?.picnum;
    let body = usize::try_from(player.sprite_i).ok()?;
    match picnum {
        crate::spawn::TOILET | crate::spawn::STALL => {
            if player.last_pissed_time != 0 {
                return Some(UtilityUse::Flush);
            }
            player.last_pissed_time = 26 * 220;
            player.transporter_hold = 29 * 2;
            if player.holster_weapon == 0 {
                player.holster_weapon = 1;
                player.weapon_pos = -1;
            }
            // `Player::extra` mirrors `sprite[p->i].extra` (synced each tic).
            if player.extra <= max_player_health - max_player_health / 10 {
                player.extra += max_player_health / 10;
                player.last_extra = player.extra;
            } else if player.extra < max_player_health {
                player.extra = max_player_health;
            }
            world.sprites[body].extra = player.extra;
            Some(UtilityUse::Urinate)
        }
        crate::spawn::WATERFOUNTAIN..=566 => {
            if actor_temps.len() <= index {
                actor_temps.resize_with(index + 1, ActorConTemps::default);
            }
            if actor_temps[index].g_t[0] == 1 {
                return Some(UtilityUse::FountainFull);
            }
            actor_temps[index].g_t[0] = 1;
            world.sprites[index].owner = player.sprite_i;
            if player.extra < max_player_health {
                player.extra += 1;
                world.sprites[body].extra = player.extra;
                Some(UtilityUse::Drink)
            } else {
                Some(UtilityUse::FountainFull)
            }
        }
        crate::spawn::PLUG => {
            let damage = 2 + (room_build_engine::numeric::krand_step(randomseed) & 3) as i16;
            player.extra -= damage;
            world.sprites[body].extra = player.extra;
            player.palette_flash = [48, 48, 64];
            player.palette_flash_tics = 32;
            Some(UtilityUse::Shock)
        }
        _ => None,
    }
}

/// Activate an Atomic nuke button with the `gamefunc_Open` ray, as
/// `sector.c::checksectors` does. Returns true only on first activation.
pub(crate) fn use_nuke_button(
    world: &mut BuildWorld,
    x: i32,
    y: i32,
    z: i32,
    angle: i16,
    sector: i16,
    player: &mut Player,
    tile_meta: &[crate::player::TileZMeta],
    actor_temps: &mut Vec<ActorConTemps>,
) -> bool {
    let sectors: Vec<_> = world
        .sectors
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
    let walls: Vec<_> = world
        .walls
        .iter()
        .map(|w| WallXY {
            x: w.x,
            y: w.y,
            point2: w.point2,
            nextsector: w.nextsector,
            cstat: w.cstat,
        })
        .collect();
    let Some((index, _)) = nearest_tagged_sprite_switch(
        world,
        x,
        y,
        z,
        sector,
        angle,
        USE_RANGE,
        tile_meta,
        &sectors,
        &walls,
        NearSpriteFilter::NukeButton,
    ) else {
        return false;
    };
    let Ok(index) = usize::try_from(index) else {
        return false;
    };
    if actor_temps.len() <= index {
        actor_temps.resize_with(index + 1, ActorConTemps::default);
    }
    if actor_temps[index].g_t[0] != 0 {
        return false;
    }
    let Some(sprite) = world.sprites.get_mut(index) else {
        return false;
    };
    actor_temps[index].g_t[0] = 1;
    sprite.owner = player.sprite_i;
    player.button_palette = sprite.pal;
    player.secret_level = if sprite.pal != 0 { sprite.lotag } else { 0 };
    true
}

/// Advance the Atomic nuke-button countdown and player fist exit animation.
/// Mirrors `actors.c`'s NUKEBUTTON case and `player.c`'s `fist_incs` tail.
pub(crate) fn tick_nuke_buttons(
    world: &mut BuildWorld,
    picnum: &mut [i16],
    actor_temps: &mut Vec<ActorConTemps>,
    players: &mut [Player],
    level_exit_ready: &mut bool,
    script: Option<&crate::con::ScriptImage>,
    sound_ids: &mut Vec<u16>,
) {
    for player in players.iter_mut() {
        if player.fist_incs == 0 {
            continue;
        }
        player.fist_incs = player.fist_incs.saturating_add(1);
        if player.fist_incs == 28 {
            if let Some(sound) = script
                .and_then(|script| script.labels.get("PIPEBOMB_EXPLODE"))
                .and_then(|sound| u16::try_from(sound.value).ok())
            {
                sound_ids.push(sound);
            }
            player.palette_flash = [64; 3];
            player.palette_flash_tics = 48;
        }
        if player.fist_incs > 42 {
            *level_exit_ready = true;
            player.fist_incs = 0;
        }
    }
    for index in 0..world.sprites.len() {
        if !(crate::spawn::NUKEBUTTON..=crate::spawn::NUKEBUTTON + 3)
            .contains(&world.sprites[index].picnum)
            || actor_temps.get(index).is_none_or(|temp| temp.g_t[0] <= 0)
        {
            continue;
        }
        let temp = &mut actor_temps[index].g_t[0];
        *temp = temp.saturating_add(1);
        if *temp == 8 {
            world.sprites[index].picnum = 143;
        } else if *temp == 16 {
            world.sprites[index].picnum = 144;
            let owner = world.sprites[index].owner;
            if let Some(player) = players.iter_mut().find(|p| p.sprite_i == owner) {
                player.fist_incs = 1;
            }
        }
        let owner = world.sprites[index].owner;
        if owner >= 0
            && players
                .iter()
                .find(|p| p.sprite_i == owner)
                .is_some_and(|p| p.fist_incs == 26)
        {
            world.sprites[index].picnum = 145;
        }
        if let Some(saved_picnum) = picnum.get_mut(index) {
            *saved_picnum = world.sprites[index].picnum;
        }
    }
}

fn access_bit_for_palette(pal: u8) -> u8 {
    match pal {
        0 => 1,
        21 => 2,
        23 => 4,
        _ => 0,
    }
}

/// Advance Chocolate's access-key insertion, activate the stored wall at frame 12,
/// and consume the matching key.
pub(crate) fn tick_access_switch(
    world: &mut BuildWorld,
    player: &mut Player,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    cyclers: &mut [ShadeCycler],
    anim_walls: &mut [AnimWall],
    monsters_off: bool,
    actor_types: &[u8],
    actor_temps: &mut Vec<ActorConTemps>,
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
) -> bool {
    if player.access_incs == 0 {
        return false;
    }
    if player.sprite_i >= 0
        && world
            .sprites
            .get(player.sprite_i as usize)
            .is_some_and(|sprite| sprite.pal == 1)
    {
        return false;
    }
    let body_dead = player.sprite_i >= 0
        && world
            .sprites
            .get(player.sprite_i as usize)
            .is_some_and(|sprite| sprite.extra <= 0);
    player.access_incs = if body_dead {
        12
    } else {
        player.access_incs.saturating_add(1)
    };
    if player.access_incs == 12 {
        let wall_index = player.access_wallnum;
        let sprite_index = player.access_spritenum;
        let mut activated = false;
        let target = if sprite_index >= 0 {
            world
                .sprites
                .get(sprite_index as usize)
                .map(|sprite| (sprite.lotag, access_bit_for_palette(sprite.pal)))
        } else {
            world
                .walls
                .get(wall_index.max(0) as usize)
                .map(|wall| (wall.lotag, access_bit_for_palette(wall.pal)))
        };
        if let Some((lotag, access_bit)) = target
            && lotag > 0
            && access_bit != 0
            && activate_switch(
                world,
                motions,
                wall_motions,
                cyclers,
                player,
                lotag,
                (sprite_index >= 0).then_some(sprite_index),
                (sprite_index < 0).then_some(wall_index),
                anim_walls,
                monsters_off,
                actor_types,
                actor_temps,
                picnum,
                script,
            )
            .is_some()
        {
            player.got_access &= !access_bit;
            activated = true;
        }
        if activated {
            return true;
        }
    }
    if player.access_incs > 20 {
        player.access_incs = 0;
        player.access_wallnum = -1;
        player.access_spritenum = -1;
        player.weapon_pos = 10;
        player.kickback_pic = 0;
    }
    false
}

fn arm_master_switches(world: &mut BuildWorld, lotag: i16) {
    let mut master = world.lists.headspritestat[crate::stat::STAT_STANDABLE as usize];
    while master >= 0 {
        let index = master as usize;
        master = world.lists.nextspritestat[index];
        let sprite = &mut world.sprites[index];
        if sprite.picnum == crate::spawn::MASTERSWITCH && sprite.lotag == lotag && sprite.yvel == 0
        {
            sprite.yvel = 1;
        }
    }
}

/// Chocolate `TOUCHPLATE` pressure handling from `moveactors`.
pub(crate) fn tick_touchplates(
    world: &mut BuildWorld,
    player: &mut Player,
    actor_temps: &mut Vec<crate::con::ActorConTemps>,
    motions: &mut Vec<SectorMotion>,
    wall_motions: &mut Vec<WallMotion>,
    cyclers: &mut [ShadeCycler],
    monsters_off: bool,
    actor_types: &[u8],
    picnum: &mut Vec<i16>,
    script: Option<&crate::con::ScriptImage>,
) {
    let mut i = world.lists.headspritestat[crate::stat::STAT_STANDABLE as usize];
    while i >= 0 {
        let index = i as usize;
        i = world.lists.nextspritestat[index];
        if world.sprites[index].picnum != crate::spawn::TOUCHPLATE {
            continue;
        }
        if actor_temps.len() <= index {
            actor_temps.resize_with(index + 1, Default::default);
        }
        let (sectnum, lotag, hitag, angle, sprite_z) = {
            let sprite = &world.sprites[index];
            (
                sprite.sectnum,
                sprite.lotag,
                sprite.hitag,
                sprite.ang,
                sprite.z,
            )
        };
        let Ok(sector_index) = usize::try_from(sectnum) else {
            continue;
        };
        let Some(sector) = world.sectors.get(sector_index).copied() else {
            continue;
        };
        let mut temp = actor_temps[index].g_t;
        let extra = i32::from(sector.extra);
        if temp[1] == 1 && hitag >= 0 {
            if temp[3] == 1 {
                if sector.floorz >= temp[2] {
                    world.sectors[sector_index].floorz = sector.floorz;
                    temp[1] = 0;
                } else {
                    world.sectors[sector_index].floorz = sector.floorz.saturating_add(extra);
                    carry_touchplate_floor(world, sector_index, extra, player);
                }
            } else if sector.floorz <= sprite_z {
                world.sectors[sector_index].floorz = sprite_z;
                temp[1] = 0;
            } else {
                world.sectors[sector_index].floorz = sector.floorz.saturating_sub(extra);
                carry_touchplate_floor(world, sector_index, -extra, player);
            }
            actor_temps[index].g_t = temp;
            continue;
        }
        if temp[5] == 1 {
            continue;
        }
        let pressed = player.cursectnum == sectnum && (player.on_ground || angle == 512);
        if pressed {
            if temp[0] == 0
                && !touchplate_lotag_in_motion(world, motions, wall_motions, actor_temps, lotag)
            {
                temp[0] = 1;
                temp[1] = 1;
                temp[3] = i32::from(temp[3] == 0);
                arm_master_switches(world, lotag);
                operate_activators(
                    world,
                    motions,
                    wall_motions,
                    cyclers,
                    lotag,
                    Some(&mut *player),
                    actor_temps,
                    monsters_off,
                    actor_types,
                    picnum,
                    script,
                );
                if world.sprites[index].hitag > 0 {
                    world.sprites[index].hitag -= 1;
                    if world.sprites[index].hitag == 0 {
                        temp[5] = 1;
                    }
                }
            }
        } else {
            temp[0] = 0;
        }
        actor_temps[index].g_t = temp;
        if temp[1] == 1 {
            let direction = temp[3];
            let mut other = world.lists.headspritestat[crate::stat::STAT_STANDABLE as usize];
            while other >= 0 {
                let other_index = other as usize;
                other = world.lists.nextspritestat[other_index];
                if other_index == index
                    || world.sprites[other_index].picnum != crate::spawn::TOUCHPLATE
                    || world.sprites[other_index].lotag != lotag
                {
                    continue;
                }
                if actor_temps.len() <= other_index {
                    actor_temps.resize_with(other_index + 1, Default::default);
                }
                actor_temps[other_index].g_t[1] = 1;
                actor_temps[other_index].g_t[3] = direction;
            }
        }
    }
}

fn touchplate_lotag_in_motion(
    world: &BuildWorld,
    motions: &[SectorMotion],
    wall_motions: &[WallMotion],
    actor_temps: &[crate::con::ActorConTemps],
    lotag: i16,
) -> bool {
    let mut activator = world.lists.headspritestat[crate::stat::STAT_ACTIVATOR as usize];
    while activator >= 0 {
        let index = activator as usize;
        activator = world.lists.nextspritestat[index];
        if world.sprites[index].lotag != lotag {
            continue;
        }
        let sector = world.sprites[index].sectnum;
        let Ok(sector_index) = usize::try_from(sector) else {
            continue;
        };
        if motions.iter().any(|motion| motion.sector == sector_index)
            || wall_motions
                .iter()
                .any(|motion| motion.sector == sector_index)
        {
            return true;
        }
        let mut effector = world.lists.headspritestat[crate::stat::STAT_EFFECTOR as usize];
        while effector >= 0 {
            let effector_index = effector as usize;
            effector = world.lists.nextspritestat[effector_index];
            if world.sprites[effector_index].sectnum != sector {
                continue;
            }
            let temps = actor_temps
                .get(effector_index)
                .map(|temps| temps.g_t[4])
                .unwrap_or(0);
            let active = match world.sprites[effector_index].lotag {
                11 | 30 => temps != 0,
                18 | 20 | 31 | 32 => actor_temps
                    .get(effector_index)
                    .map(|temps| temps.g_t[0] != 0)
                    .unwrap_or(false),
                _ => false,
            };
            if active {
                return true;
            }
        }
    }
    false
}

fn carry_touchplate_floor(
    world: &mut BuildWorld,
    sector_index: usize,
    delta: i32,
    player: &mut Player,
) {
    if player.cursectnum == sector_index as i16
        && world.sectors[sector_index]
            .floorz
            .saturating_sub(player.posz)
            < (64 << 8)
    {
        player.posz = player.posz.saturating_add(delta);
        player.poszv = 0;
    }
    let mut sprite = world.lists.headspritesect[sector_index];
    while sprite >= 0 {
        let index = sprite as usize;
        sprite = world.lists.nextspritesect[index];
        if world.lists.sprite_stat[index] != crate::stat::STAT_EFFECTOR {
            world.sprites[index].z = world.sprites[index].z.saturating_add(delta);
        }
    }
}

pub(crate) fn is_switch(picnum: i16) -> bool {
    SWITCH_BASES.iter().any(|base| {
        (picnum >= *base && picnum <= base + 1) || (*base == 146 && (146..=149).contains(&picnum))
    })
}

/// Atomic player bullets call `checkhitswitch` only for these DIP/HAND tiles
/// in `shoot`; the other switch classes are operated through `operate`.
pub(crate) fn is_shoot_activated_switch(picnum: i16) -> bool {
    matches!(picnum, 162..=165 | 168..=169 | 1111..=1112)
}

pub(crate) fn is_door_operator_wall(picnum: i16) -> bool {
    DOOR_OPERATOR_WALLS.contains(&picnum)
}

/// Quick-kick may activate a tagged switch only when its hitscan actually
/// reached a switch tile. Keep ordinary actors and untagged geometry out of
/// `hit_switch_target` (where special lotag values have separate meanings).
pub(crate) fn knee_hit_is_switch(
    world: &BuildWorld,
    wall_index: Option<i16>,
    sprite_index: Option<i16>,
) -> bool {
    if let Some(index) = wall_index {
        return world.walls.get(index as usize).is_some_and(|wall| {
            wall.lotag > 0 && (is_switch(wall.picnum) || is_door_operator_wall(wall.picnum))
        });
    }
    sprite_index
        .and_then(|index| world.sprites.get(index as usize))
        .is_some_and(|sprite| sprite.lotag > 0 && is_switch(sprite.picnum))
}

fn next_switch_frame(picnum: i16) -> Option<i16> {
    if (146..=149).contains(&picnum) {
        return Some(if picnum == 149 { 146 } else { picnum + 1 });
    }
    SWITCH_BASES
        .iter()
        .find(|base| **base != 146 && (picnum == **base || picnum == **base + 1))
        .map(|base| if picnum == *base { *base + 1 } else { *base })
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_build_engine::{SectorT, SpriteT, WallT};

    fn tagged_switch_map() -> BuildWorld {
        let mut world = BuildWorld::new();
        world.sectors.push(SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -(64 << 8),
            floorz: 0,
            lotag: 20,
            extra: 256,
            ..Default::default()
        });
        world.sectors.push(SectorT {
            wallptr: 4,
            wallnum: 0,
            ceilingz: -(128 << 8),
            floorz: 0,
            ..Default::default()
        });
        world.walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: 1,
                picnum: 0,
                ..Default::default()
            },
            WallT {
                x: 1024,
                y: 0,
                point2: 2,
                nextsector: -1,
                picnum: 130,
                lotag: 7,
                ..Default::default()
            },
            WallT {
                x: 1024,
                y: 1024,
                point2: 3,
                nextsector: -1,
                picnum: 0,
                ..Default::default()
            },
            WallT {
                x: 0,
                y: 1024,
                point2: 0,
                nextsector: -1,
                picnum: 131,
                lotag: 7,
                ..Default::default()
            },
        ];
        world.numsectors = 2;
        world.numwalls = 4;
        world.sprites[0] = SpriteT {
            picnum: crate::spawn::ACTIVATOR,
            lotag: 7,
            sectnum: 0,
            ..Default::default()
        };
        assert!(
            world
                .lists
                .place_sprite_at(0, 0, crate::stat::STAT_ACTIVATOR)
        );
        world
    }

    #[test]
    fn switch_pairs_flip_and_multiswitch_cycles() {
        assert_eq!(next_switch_frame(130), Some(131));
        assert_eq!(next_switch_frame(131), Some(130));
        assert_eq!(next_switch_frame(149), Some(146));
        assert_eq!(next_switch_frame(150), None);
    }

    #[test]
    fn switch_classifier_excludes_regular_wall_tiles() {
        assert!(is_switch(130));
        assert!(is_switch(149));
        assert!(is_switch(1143));
        assert!(!is_switch(129));
        assert!(!is_switch(1000));
    }

    #[test]
    fn use_flips_linked_wall_switches_with_one_neartag() {
        let mut world = tagged_switch_map();
        let mut motions = Vec::new();
        let mut wall_motions = Vec::new();
        let mut player = Player::default();
        let mut level_exit_ready = false;
        assert_eq!(
            use_wall_switch(
                &mut world,
                &mut motions,
                &mut wall_motions,
                &mut [],
                512,
                512,
                -(32 << 8),
                0,
                0,
                &mut level_exit_ready,
                &mut player,
                &[],
                &mut [],
                false,
                &[],
                &mut Vec::new(),
                &mut Vec::new(),
                None,
            ),
            Some(1)
        );
        assert_eq!(world.walls[1].picnum, 131);
        assert_eq!(world.walls[3].picnum, 130);
        assert_eq!(world.walls[0].picnum, 0);
        assert_eq!(motions.len(), 1);
        assert_eq!(motions[0].ceiling_goal, Some(-(128 << 8)));
    }

    #[test]
    fn activator_opens_and_closes_sector20_ceiling_door() {
        let mut world = BuildWorld::new();
        world.sectors = vec![
            SectorT {
                wallptr: 0,
                wallnum: 1,
                ceilingz: -(64 << 8),
                floorz: 0,
                lotag: 20,
                extra: 256,
                ..Default::default()
            },
            SectorT {
                wallptr: 1,
                wallnum: 0,
                ceilingz: -(128 << 8),
                floorz: 0,
                ..Default::default()
            },
        ];
        world.walls.push(WallT {
            point2: 0,
            nextsector: 1,
            ..Default::default()
        });
        world.sprites[0] = SpriteT {
            picnum: crate::spawn::ACTIVATOR,
            lotag: 7,
            sectnum: 0,
            ..Default::default()
        };
        assert!(
            world
                .lists
                .place_sprite_at(0, 0, crate::stat::STAT_ACTIVATOR)
        );

        let mut motions = Vec::new();
        let mut wall_motions = Vec::new();
        operate_activators(
            &mut world,
            &mut motions,
            &mut wall_motions,
            &mut [],
            7,
            None,
            &mut Vec::new(),
            false,
            &[],
            &mut Vec::new(),
            None,
        );
        assert_eq!(motions.len(), 1);
        assert_eq!(world.sectors[0].lotag & i16::MIN, i16::MIN);
        let mut player_z = -(40 << 8);
        let mut player_zv = 0;
        let lists = world.lists.clone();
        for _ in 0..16 {
            tick_sector_motions(
                &mut motions,
                &mut world,
                &lists,
                &mut Vec::new(),
                -1,
                &mut player_z,
                &mut player_zv,
            );
        }
        assert!(motions.is_empty());
        assert_eq!(world.sectors[0].ceilingz, -(128 << 8));

        operate_activators(
            &mut world,
            &mut motions,
            &mut wall_motions,
            &mut [],
            7,
            None,
            &mut Vec::new(),
            false,
            &[],
            &mut Vec::new(),
            None,
        );
        assert_eq!(motions.len(), 1);
        for _ in 0..32 {
            tick_sector_motions(
                &mut motions,
                &mut world,
                &lists,
                &mut Vec::new(),
                -1,
                &mut player_z,
                &mut player_zv,
            );
        }
        assert!(motions.is_empty());
        assert_eq!(world.sectors[0].ceilingz, 0);
    }

    #[test]
    fn sector21_platform_targets_neighbor_floor() {
        let mut world = BuildWorld::new();
        world.sectors = vec![
            SectorT {
                wallptr: 0,
                wallnum: 1,
                ceilingz: 0,
                floorz: 0,
                lotag: 21,
                extra: 64,
                ..Default::default()
            },
            SectorT {
                wallptr: 1,
                wallnum: 0,
                ceilingz: -(64 << 8),
                floorz: 8192,
                ..Default::default()
            },
        ];
        world.walls.push(WallT {
            nextsector: 1,
            ..Default::default()
        });
        world.sprites[0] = SpriteT {
            picnum: crate::spawn::ACTIVATOR,
            lotag: 9,
            sectnum: 0,
            ..Default::default()
        };
        assert!(
            world
                .lists
                .place_sprite_at(0, 0, crate::stat::STAT_ACTIVATOR)
        );
        let mut motions = Vec::new();
        let mut wall_motions = Vec::new();
        operate_activators(
            &mut world,
            &mut motions,
            &mut wall_motions,
            &mut [],
            9,
            None,
            &mut Vec::new(),
            false,
            &[],
            &mut Vec::new(),
            None,
        );
        assert_eq!(motions[0].floor_goal, Some(8192));
        assert_eq!(world.sectors[0].lotag & i16::MIN, i16::MIN);
    }

    #[test]
    fn sector22_split_door_opens_and_closes_both_planes() {
        let mut world = BuildWorld::new();
        world.sectors = vec![
            SectorT {
                wallptr: 0,
                wallnum: 2,
                ceilingz: -(64 << 8),
                floorz: 0,
                lotag: 22,
                extra: 256,
                ..Default::default()
            },
            SectorT {
                wallptr: 2,
                wallnum: 0,
                ceilingz: -(64 << 8),
                floorz: 16384,
                ..Default::default()
            },
            SectorT {
                wallptr: 2,
                wallnum: 0,
                ceilingz: -(128 << 8),
                floorz: 0,
                ..Default::default()
            },
        ];
        world.walls = vec![
            WallT {
                nextsector: 1,
                ..Default::default()
            },
            WallT {
                nextsector: 2,
                ..Default::default()
            },
        ];
        world.sprites[0] = SpriteT {
            picnum: crate::spawn::ACTIVATOR,
            lotag: 11,
            sectnum: 0,
            ..Default::default()
        };
        assert!(
            world
                .lists
                .place_sprite_at(0, 0, crate::stat::STAT_ACTIVATOR)
        );
        let mut motions = Vec::new();
        let mut wall_motions = Vec::new();
        operate_activators(
            &mut world,
            &mut motions,
            &mut wall_motions,
            &mut [],
            11,
            None,
            &mut Vec::new(),
            false,
            &[],
            &mut Vec::new(),
            None,
        );
        assert_eq!(motions[0].floor_goal, Some(16384));
        assert_eq!(motions[0].ceiling_goal, Some(-(128 << 8)));
        assert_eq!(world.sectors[0].lotag & i16::MIN, i16::MIN);

        let mut player_z = 0;
        let mut player_zv = 0;
        let lists = world.lists.clone();
        for _ in 0..16 {
            tick_sector_motions(
                &mut motions,
                &mut world,
                &lists,
                &mut Vec::new(),
                -1,
                &mut player_z,
                &mut player_zv,
            );
        }
        assert!(motions.is_empty());
        operate_activators(
            &mut world,
            &mut motions,
            &mut wall_motions,
            &mut [],
            11,
            None,
            &mut Vec::new(),
            false,
            &[],
            &mut Vec::new(),
            None,
        );
        assert_eq!(motions[0].floor_goal, Some(-8192));
        assert_eq!(motions[0].ceiling_goal, Some(-8192));
        assert_eq!(world.sectors[0].lotag & i16::MIN, 0);
        for _ in 0..24 {
            tick_sector_motions(
                &mut motions,
                &mut world,
                &lists,
                &mut Vec::new(),
                -1,
                &mut player_z,
                &mut player_zv,
            );
        }
        assert!(motions.is_empty());
        assert_eq!(world.sectors[0].floorz, -8192);
        assert_eq!(world.sectors[0].ceilingz, -8192);
    }

    #[test]
    fn sector9_sliding_door_moves_six_wall_vertices() {
        let mut world = BuildWorld::new();
        world.sectors.push(SectorT {
            wallptr: 0,
            wallnum: 6,
            ceilingz: -(64 << 8),
            floorz: 0,
            lotag: 9,
            extra: 16,
            ..Default::default()
        });
        world.walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 50,
                y: 0,
                point2: 2,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 100,
                y: 20,
                point2: 3,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 100,
                y: 100,
                point2: 4,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 50,
                y: 100,
                point2: 5,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 0,
                y: 80,
                point2: 0,
                nextsector: -1,
                ..Default::default()
            },
        ];
        world.sprites[0] = SpriteT {
            picnum: crate::spawn::ACTIVATOR,
            lotag: 13,
            sectnum: 0,
            ..Default::default()
        };
        assert!(
            world
                .lists
                .place_sprite_at(0, 0, crate::stat::STAT_ACTIVATOR)
        );
        let mut sector_motions = Vec::new();
        let mut wall_motions = Vec::new();
        operate_activators(
            &mut world,
            &mut sector_motions,
            &mut wall_motions,
            &mut [],
            13,
            None,
            &mut Vec::new(),
            false,
            &[],
            &mut Vec::new(),
            None,
        );
        assert_eq!(wall_motions.len(), 6);
        for _ in 0..20 {
            tick_wall_motions(&mut wall_motions, &mut world);
        }
        assert!(wall_motions.is_empty());
        assert_eq!(world.walls[0].y, 60);
        assert_eq!(world.walls[1].y, 50);
        assert_eq!(world.walls[2].y, 60);
        assert_eq!(world.walls[3].y, 40);
        assert_eq!(world.walls[4].y, 50);
        assert_eq!(world.walls[5].y, 40);
    }
}
