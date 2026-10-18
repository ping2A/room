//! Sector effects the door / rotator pass does not cover.
//!
//! Each effector becomes a Room special the playsim already runs: a floor
//! scroller that carries actors, a waggle, a lift, a light change, a quake or
//! a teleport. Geometry that has to slide along a track (the subway car
//! itself, a stretching bridge, a floor that spins under your feet) is still
//! a scroller or a door, not a moving Build sector. Subway / train cars
//! (`SE 6` / `14` / `30`) follow `LOCATORS` for carry direction; the walls
//! stay where the map built them. `SE 16` reactor floors oscillate along
//! the effector angle. `SE 22` teeth retract when the linked lotag-29
//! door ceiling opens. Unfinished `SE 40`–`45` become a 3D floor.

use crate::acs::Op;
use crate::convert::{GameTiles, Z_SCALE, shade_light};
use crate::map::{BuildMap, Sprite};
use std::collections::{BTreeMap, BTreeSet};

/// A line special to attach when the wall does not already have one.
pub(crate) struct FxLine {
    pub wall: usize,
    pub special: i32,
    pub args: [i32; 5],
    pub cross: bool,
    pub repeat: bool,
    pub impact: bool,
}

/// Effects for one map.
pub(crate) struct Fx {
    /// Specials for the map-start ACS script.
    pub open: Vec<(i32, [i32; 5])>,
    /// Extra sector tags.
    pub tags: Vec<(usize, i32)>,
    /// Doom sector specials (lights).
    pub specials: BTreeMap<usize, i32>,
    pub lines: Vec<FxLine>,
    /// Teleport destinations: tid, Build x, Build y, angle.
    pub dests: Vec<(i32, i32, i32, i16)>,
    /// `SE 13` sectors as Duke spawns them (shut until blown open).
    pub closed: Vec<Closed>,
    /// What a C-9 detonation of each hitag sets off (`SE 8` / `13` / `18` / `21`).
    pub detonate: BTreeMap<i16, Vec<Op>>,
    /// `SE 3` light changes for wall lights shot out, by wall lotag.
    pub shot_lights: BTreeMap<i16, Vec<Op>>,
    /// Operate the effectors that sit in this sector (`MASTERSWITCH`).
    pub sector_ops: BTreeMap<usize, Vec<Op>>,
    /// `SE 128` sectors, by the effector's hitag.
    pub glass_sectors: BTreeMap<i16, Vec<usize>>,
    /// Ceiling-door sectors whose `SE 8` / `SE 9` channel lights a room.
    pub door_lights: BTreeMap<usize, DoorLight>,
    /// Lotag-30 floors that carry actors around an `SE 1` pivot.
    pub spins: Vec<FloorSpin>,
    /// `SE 6` / `14` / `30` cars that follow locator waypoints.
    pub subways: Vec<Subway>,
    /// `SE 20` stretch / shrink sectors (lotag 27).
    pub stretches: Vec<Stretch>,
    /// `SE 22` teeth prongs that retract when a lotag-29 door opens.
    pub teeth: Vec<Teeth>,
    /// `SE 16` reactor skew (vertices oscillate along the effector angle).
    pub skews: Vec<Skew>,
}

/// `SE 22` teeth: vertices slide when the linked lotag-29 door opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Teeth {
    pub sector: usize,
    /// Lotag-29 door sector.
    pub door: usize,
    /// Direction X in 8.8 (256 = +1).
    pub dx: i32,
    /// Direction Y in 8.8 (Room y).
    pub dy: i32,
    /// Travel in Room map units.
    pub dist: i32,
    /// Map units per tic (1/32).
    pub speed: i32,
}

/// `SE 16` reactor skew: vertices oscillate along the effector angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Skew {
    pub sector: usize,
    /// Direction X in 8.8 (256 = +1).
    pub dx: i32,
    /// Direction Y in 8.8 (Room y).
    pub dy: i32,
    /// Travel in Room map units.
    pub dist: i32,
    /// Map units per tic (1/32).
    pub speed: i32,
}

/// Lotag-27 / `SE 20` stretch: vertices slide along the effector angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stretch {
    pub sector: usize,
    /// Direction X in 8.8 (256 = +1).
    pub dx: i32,
    /// Direction Y in 8.8 (Room y).
    pub dy: i32,
    /// Travel in Room map units.
    pub dist: i32,
    /// Map units per tic (1/32).
    pub speed: i32,
}

/// Subway / train car (`SE 6` / `14` / `30`) that follows locators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Subway {
    pub sector: usize,
    /// Room-unit start (the effector).
    pub x: i32,
    pub y: i32,
    /// Carry speed in 1/32 map units per tic.
    pub speed: i32,
    /// Waypoints in Room units, in visit order.
    pub points: Vec<(i32, i32)>,
}

/// Walkable rotator (`SE 0` + `SE 1` in a lotag-30 sector).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FloorSpin {
    pub sector: usize,
    /// Pivot X in Room map units.
    pub x: i32,
    /// Pivot Y in Room map units.
    pub y: i32,
    /// Hexen byte angles per tic (signed).
    pub speed: i32,
}

/// Lights that follow a ceiling door (`SE 8` on open, `SE 9` on close).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DoorLight {
    /// Extra tag shared by the door and the rooms it lights.
    pub tag: i32,
    /// Effector shade as a Room light.
    pub bright: i32,
    /// `true` = `SE 8` (bright while open); `false` = `SE 9` (bright while shut).
    pub on_open: bool,
}

/// A sector `SE 13` shuts at map start (Build z).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Closed {
    pub sector: usize,
    pub floorz: i32,
    pub ceilingz: i32,
    /// The parallaxed ceiling only shows once blown open.
    pub hide_sky: bool,
}

impl Fx {
    /// `map` with the [`Closed`] sectors shut.
    pub(crate) fn shut(&self, map: &BuildMap) -> BuildMap {
        let mut out = map.clone();
        for c in &self.closed {
            if let Some(sec) = out.sectors.get_mut(c.sector) {
                sec.floorz = c.floorz;
                sec.ceilingz = c.ceilingz;
                if c.hide_sky {
                    sec.ceilingstat &= !1;
                    sec.ceilingshade = sec.floorshade;
                }
            }
        }
        out
    }
}

/// `special(tag, speed, |h|, h < 0)` moving a plane to Build height `z`.
pub(crate) fn move_to(special: i32, tag: i32, speed: i32, z: i32) -> Op {
    let h = (-f64::from(z) / Z_SCALE).round() as i32;
    Op::Special(special, [tag, speed, h.abs(), i32::from(h < 0), 0])
}

const SE_ROTATE: i16 = 0;
const SE_PIVOT: i16 = 1;
const SE_QUAKE: i16 = 2;
const SE_SHOT_LIGHTS: i16 = 3;
const SE_DOOR_LIGHTS: i16 = 8;
const SE_DOOR_LIGHTS_CLOSE: i16 = 9;
const SE_SUBWAY: i16 = 6;
const SE_CAR: i16 = 14;
const SE_LIGHT_SWITCH: i16 = 12;
const SE_EXPLODE: i16 = 13;
const SE_TRANSPORT: i16 = 17;
const SE_INCREMENT: i16 = 18;
const SE_CEILING_FALL: i16 = 19;
const SE_SKEW: i16 = 16;
const SE_STRETCH: i16 = 20;
const SE_TEETH: i16 = 22;
const SE_RISE: i16 = 21;
const SE_CONVEYOR: i16 = 24;
const SE_PISTON: i16 = 25;
const SE_ESCALATOR: i16 = 26;
const SE_LIGHTNING: i16 = 28;
const SE_FLOAT: i16 = 29;
const SE_TRAIN: i16 = 30;
const SE_FLOOR: i16 = 31;
const SE_CEILING: i16 = 32;
const SE_CONVEYOR_SLOW: i16 = 34;
const SE_DRILL: i16 = 35;
const SE_FOF_FIRST: i16 = 40;
const SE_FOF_LAST: i16 = 45;
const SE_GLASS: i16 = 128;

const SCROLL_FLOOR: i32 = 223;
/// Room: toggle an `SE 20` stretch (`docs/23-build-engine.md`).
const SECTOR_STRETCH: i32 = 284;
/// GZDoom `Sector_Set3DFloor` — Duke's unfinished `SE 40`–`45` FoF.
const SECTOR_SET_3DFLOOR: i32 = 160;
const FLOOR_WAGGLE: i32 = 138;
const PLAT_PERPETUAL: i32 = 60;
const RADIUS_QUAKE: i32 = 120;
const LIGHT_CHANGE: i32 = 112;
const LIGHT_GLOW: i32 = 114;
const LIGHT_FLICKER: i32 = 115;
const FLOOR_RAISE_BY_VALUE: i32 = 23;
const FLOOR_LOWER_BY_VALUE: i32 = 20;
const CEILING_RAISE_BY_VALUE: i32 = 41;
const CEILING_LOWER_BY_VALUE: i32 = 40;
const FLOOR_MOVE_TO_VALUE: i32 = 37;
const CEILING_MOVE_TO_VALUE: i32 = 47;
/// Speed of a blown-open crack (instant in Duke) and of a sinking `SE 13` plane.
const BLAST_SPEED: i32 = 255;
const SINK_SPEED: i32 = 64;
const TELEPORT: i32 = 70;
/// Random blink (`ST_LIGHT_BLINK`).
const ST_BLINK: i32 = 1;
/// First tid for transport-elevator destinations.
const DEST_TID: i32 = 8000;

/// Scroll / carry velocity for a Build angle. `units` is Room map units per tic;
/// the special stores that speed in 1/32 units.
pub(crate) fn angle_scroll(ang: i16, units: f64) -> (i32, i32) {
    let a = f64::from(i32::from(ang) & 2047) * std::f64::consts::TAU / 2048.0;
    let dx = (units * a.cos() * 32.0).round() as i32;
    let dy = (-units * a.sin() * 32.0).round() as i32;
    if dx == 0 && dy == 0 {
        (32, 0)
    } else {
        (dx, dy)
    }
}

/// Unit direction of an `SE 20` stretch, 8.8 (256 = 1).
/// Sectors that receive an unfinished Duke FoF (`SE 40`–`45`) from `control`.
fn fof_targets(map: &BuildMap, control: usize, hitag: i16) -> Vec<usize> {
    if hitag == 0 {
        return Vec::new();
    }
    let mut out: Vec<usize> = map
        .sectors
        .iter()
        .enumerate()
        .filter(|&(i, s)| i != control && s.hitag == hitag)
        .map(|(i, _)| i)
        .collect();
    if out.is_empty() {
        out = map
            .sectors
            .iter()
            .enumerate()
            .filter(|&(i, s)| i != control && s.lotag == hitag)
            .map(|(i, _)| i)
            .collect();
    }
    out
}

fn fof_type(lotag: i16) -> i32 {
    match lotag {
        42 => 2,
        43..=45 => 3,
        _ => 1,
    }
}

/// Lotag-29 door that `SE 22` `hitag` should follow.
fn teeth_door(map: &BuildMap, hitag: i16) -> Option<usize> {
    let mut fallback = None;
    for (i, s) in map.sectors.iter().enumerate() {
        if s.lotag != 29 {
            continue;
        }
        if hitag != 0 && s.hitag == hitag {
            return Some(i);
        }
        fallback = Some(i);
    }
    fallback
}

fn stretch_dir(ang: i16) -> (i32, i32) {
    let a = f64::from(i32::from(ang) & 2047) * std::f64::consts::TAU / 2048.0;
    let dx = (a.cos() * 256.0).round() as i32;
    let dy = (-a.sin() * 256.0).round() as i32;
    if dx == 0 && dy == 0 {
        (256, 0)
    } else {
        (dx, dy)
    }
}

fn tag_of(sector: usize) -> i32 {
    300_000 + i32::try_from(sector).unwrap_or(0)
}

fn channel_tag(lotag: i16, kind: i32) -> i32 {
    410_000 + (i32::from(lotag) & 0xffff) * 8 + kind
}

fn sector_of(sp: &Sprite, n: usize) -> Option<usize> {
    usize::try_from(sp.sectnum).ok().filter(|&s| s < n)
}

fn map_units(build_dz: i32) -> i32 {
    (build_dz.abs() / 256).clamp(8, 1024)
}

fn is_ceiling_door(lotag: i16) -> bool {
    matches!(lotag, 20 | 22 | 26 | 29)
}

/// `GPSPEED` lotag in `sector`, or 256 when the sprite is absent.
fn gpspeed(map: &BuildMap, game: &GameTiles, sector: usize) -> i32 {
    map.sprites
        .iter()
        .find(|s| {
            usize::try_from(s.sectnum).ok() == Some(sector) && i32::from(s.picnum) == game.gpspeed
        })
        .map(|s| i32::from(s.lotag).clamp(1, 4096))
        .unwrap_or(256)
}

/// Plan sector effects.
pub(crate) fn plan(map: &BuildMap, game: &GameTiles) -> Fx {
    let mut fx = Fx {
        open: Vec::new(),
        tags: Vec::new(),
        specials: BTreeMap::new(),
        lines: Vec::new(),
        dests: Vec::new(),
        closed: Vec::new(),
        detonate: BTreeMap::new(),
        shot_lights: BTreeMap::new(),
        sector_ops: BTreeMap::new(),
        glass_sectors: BTreeMap::new(),
        door_lights: BTreeMap::new(),
        spins: Vec::new(),
        subways: Vec::new(),
        stretches: Vec::new(),
        teeth: Vec::new(),
        skews: Vec::new(),
    };
    let mut door_light_ch: BTreeMap<(i16, bool), Vec<(usize, i32)>> = BTreeMap::new();
    let nsec = map.sectors.len();
    let nwall = map.walls.len();
    // Walls whose right side is `s` (Build's own sector).
    let own_walls = |s: usize| -> Vec<usize> {
        (0..nwall)
            .filter(|&i| wall_sector(map, i) == Some(s))
            .collect()
    };
    let effectors: Vec<&Sprite> = map
        .sprites
        .iter()
        .filter(|s| i32::from(s.picnum) == game.effector)
        .collect();
    let mut tagged: BTreeSet<usize> = BTreeSet::new();
    let mut use_tag = |fx: &mut Fx, s: usize| {
        if tagged.insert(s) {
            fx.tags.push((s, tag_of(s)));
        }
        tag_of(s)
    };

    for sp in &effectors {
        let Some(s) = sector_of(sp, nsec) else {
            continue;
        };
        let speed = gpspeed(map, game, s);
        let rate = f64::from(speed) / 256.0;
        match sp.lotag {
            SE_ROTATE if map.sectors[s].lotag == 30 => {
                if let Some(pivot) = effectors
                    .iter()
                    .find(|p| p.lotag == SE_PIVOT && p.hitag == sp.hitag)
                {
                    let mut spd = (27 * speed / 256).max(1);
                    if i32::from(pivot.ang) & 2047 > 1024 {
                        spd = -spd;
                    }
                    fx.spins.push(FloorSpin {
                        sector: s,
                        x: pivot.x / 16,
                        y: -pivot.y / 16,
                        speed: spd,
                    });
                }
            }
            SE_QUAKE => {
                let args = [5, 140, 0, 1024, 0];
                for i in own_walls(s) {
                    if map.walls[i].nextwall >= 0 {
                        fx.lines.push(FxLine {
                            wall: i,
                            special: RADIUS_QUAKE,
                            args,
                            cross: true,
                            repeat: false,
                            impact: false,
                        });
                    }
                }
                fx.sector_ops
                    .entry(s)
                    .or_default()
                    .push(Op::Special(RADIUS_QUAKE, args));
            }
            SE_DOOR_LIGHTS | SE_DOOR_LIGHTS_CLOSE => {
                let bright = shade_light(sp.shade);
                if sp.lotag == SE_DOOR_LIGHTS {
                    // Raze `detonate`: SE 8 with the charge's hitag lights its
                    // sector to the effector's shade (the hole in the E1L2 roof).
                    let tag = use_tag(&mut fx, s);
                    fx.detonate
                        .entry(sp.hitag)
                        .or_default()
                        .push(Op::Special(LIGHT_CHANGE, [tag, bright, 0, 0, 0]));
                }
                // Same-hitag rooms follow a ceiling door in the channel
                // (Raze `SE_8_UP_OPEN_DOOR_LIGHTS` / `SE_9_DOWN_OPEN_DOOR_LIGHTS`).
                door_light_ch
                    .entry((sp.hitag, sp.lotag == SE_DOOR_LIGHTS))
                    .or_default()
                    .push((s, bright));
            }
            SE_GLASS => {
                if sp.hitag != 0 {
                    fx.glass_sectors.entry(sp.hitag).or_default().push(s);
                }
            }
            SE_SHOT_LIGHTS => {
                let tag = use_tag(&mut fx, s);
                let bright = shade_light(sp.shade);
                let dim = shade_light(map.sectors[s].floorshade);
                fx.shot_lights
                    .entry(sp.hitag)
                    .or_default()
                    .push(Op::Special(
                        LIGHT_FLICKER,
                        [tag, bright, dim.min(bright), 0, 0],
                    ));
            }
            SE_SUBWAY | SE_CAR | SE_TRAIN | SE_CONVEYOR | SE_CONVEYOR_SLOW | SE_ESCALATOR => {
                let tag = use_tag(&mut fx, s);
                let units = if matches!(sp.lotag, SE_CONVEYOR_SLOW) {
                    rate * 0.5
                } else if matches!(sp.lotag, SE_SUBWAY | SE_CAR | SE_TRAIN) {
                    rate * 2.0
                } else {
                    rate
                };
                let path = if matches!(sp.lotag, SE_SUBWAY | SE_CAR | SE_TRAIN) {
                    locator_path(map, game, sp.hitag)
                } else {
                    Vec::new()
                };
                if path.len() >= 2 {
                    let speed = (units.max(0.25) * 32.0).round() as i32;
                    fx.subways.push(Subway {
                        sector: s,
                        x: sp.x / 16,
                        y: -sp.y / 16,
                        speed: speed.max(8),
                        points: path,
                    });
                } else {
                    let (dx, dy) = angle_scroll(sp.ang, units.max(0.25));
                    fx.open.push((SCROLL_FLOOR, [tag, 0, dx, dy, 0]));
                }
            }
            SE_SKEW => {
                let (dx, dy) = stretch_dir(sp.ang);
                fx.skews.push(Skew {
                    sector: s,
                    dx,
                    dy,
                    dist: (speed / 16).clamp(8, 256),
                    speed: 32,
                });
            }
            SE_TEETH => {
                if let Some(door) = teeth_door(map, sp.hitag) {
                    let (dx, dy) = stretch_dir(sp.ang);
                    fx.teeth.push(Teeth {
                        sector: s,
                        door,
                        dx,
                        dy,
                        dist: (speed / 16).clamp(8, 256),
                        speed: 32,
                    });
                }
            }
            SE_STRETCH => {
                let tag = use_tag(&mut fx, s);
                let (dx, dy) = stretch_dir(sp.ang);
                fx.stretches.push(Stretch {
                    sector: s,
                    dx,
                    dy,
                    dist: (speed / 16).clamp(8, 256),
                    speed: 32,
                });
                activator_lines(
                    map,
                    game,
                    s,
                    &mut fx.lines,
                    SECTOR_STRETCH,
                    [tag, 0, 0, 0, 0],
                );
            }
            SE_LIGHT_SWITCH => {
                let tag = channel_tag(sp.hitag, 1);
                fx.tags.push((s, tag));
                let bright = shade_light(sp.shade).clamp(48, 255);
                switch_lines(
                    map,
                    game,
                    sp.hitag,
                    &mut fx.lines,
                    LIGHT_CHANGE,
                    [tag, bright, 0, 0, 0],
                    true,
                );
            }
            SE_EXPLODE => {
                // Raze `spawneffector` / `handle_se13`: the plane nearer the
                // effector (both, unless its angle is 512) starts at the
                // effector's height and goes back when a charge blows.
                let tag = use_tag(&mut fx, s);
                let sec = &map.sectors[s];
                let (top, bottom) = (sec.ceilingz, sec.floorz);
                let ceiling = (top - sp.z).abs() < (bottom - sp.z).abs();
                let one_plane = sp.ang == 512;
                let mut closed = Closed {
                    sector: s,
                    floorz: bottom,
                    ceilingz: top,
                    hide_sky: false,
                };
                let ops = fx.detonate.entry(sp.hitag).or_default();
                if one_plane && ceiling {
                    closed.ceilingz = sp.z;
                    ops.push(move_to(CEILING_MOVE_TO_VALUE, tag, SINK_SPEED, top));
                } else if one_plane {
                    closed.floorz = sp.z;
                    ops.push(move_to(FLOOR_MOVE_TO_VALUE, tag, SINK_SPEED, bottom));
                } else {
                    closed.floorz = sp.z;
                    closed.ceilingz = sp.z;
                    ops.push(move_to(FLOOR_MOVE_TO_VALUE, tag, BLAST_SPEED, bottom));
                    ops.push(move_to(CEILING_MOVE_TO_VALUE, tag, BLAST_SPEED, top));
                }
                if sec.ceilingstat & 1 != 0 && !(one_plane && !ceiling) {
                    closed.hide_sky = true;
                    ops.push(Op::Flat {
                        tag,
                        texture: "F_SKY1".into(),
                        ceiling: true,
                    });
                }
                fx.closed.push(closed);
            }
            SE_INCREMENT => {
                let tag = use_tag(&mut fx, s);
                let dist = (i32::from(sp.hitag).max(1) * 4).clamp(4, 256);
                let floor = (i32::from(sp.ang) & 2047) > 1024;
                let special = if floor {
                    FLOOR_RAISE_BY_VALUE
                } else {
                    CEILING_LOWER_BY_VALUE
                };
                activator_lines(map, game, s, &mut fx.lines, special, [tag, 16, dist, 0, 0]);
                let op = Op::Special(special, [tag, 16, dist, 0, 0]);
                fx.detonate.entry(sp.hitag).or_default().push(op.clone());
                fx.sector_ops.entry(s).or_default().push(op);
            }
            SE_CEILING_FALL => {
                let tag = use_tag(&mut fx, s);
                for i in own_walls(s) {
                    fx.lines.push(FxLine {
                        wall: i,
                        special: CEILING_LOWER_BY_VALUE,
                        args: [tag, 16, 128, 0, 0],
                        cross: false,
                        repeat: false,
                        impact: true,
                    });
                }
            }
            SE_RISE => {
                let tag = channel_tag(sp.hitag, 3);
                fx.tags.push((s, tag));
                let sec = &map.sectors[s];
                let floor = (i32::from(sp.ang) & 2047) <= 1024;
                let (special, dist) = if floor {
                    (FLOOR_RAISE_BY_VALUE, map_units(sp.z - sec.floorz))
                } else {
                    (CEILING_LOWER_BY_VALUE, map_units(sp.z - sec.ceilingz))
                };
                let raise = if floor {
                    sp.z < sec.floorz
                } else {
                    sp.z > sec.ceilingz
                };
                let special = if raise {
                    special
                } else if floor {
                    FLOOR_LOWER_BY_VALUE
                } else {
                    CEILING_RAISE_BY_VALUE
                };
                switch_lines(
                    map,
                    game,
                    sp.hitag,
                    &mut fx.lines,
                    special,
                    [tag, 16, dist, 0, 0],
                    false,
                );
                let op = Op::Special(special, [tag, 16, dist, 0, 0]);
                fx.detonate.entry(sp.hitag).or_default().push(op.clone());
                fx.sector_ops.entry(s).or_default().push(op);
            }
            SE_PISTON | SE_DRILL => {
                let tag = use_tag(&mut fx, s);
                fx.open.push((PLAT_PERPETUAL, [tag, 16, 35, 0, 0]));
            }
            SE_LIGHTNING => {
                fx.specials.entry(s).or_insert(ST_BLINK);
            }
            SE_FLOAT => {
                let tag = use_tag(&mut fx, s);
                // GPSpeed 256 is one Build Z step (4 map units). Waggle amp is ×8.
                let amp = (speed / 8).clamp(8, 64);
                let delay = i32::from(sp.hitag & 2047) * 35 / 2048;
                fx.open.push((FLOOR_WAGGLE, [tag, amp, 16, delay, 0]));
            }
            SE_FLOOR | SE_CEILING => {
                let tag = channel_tag(sp.hitag, if sp.lotag == SE_FLOOR { 4 } else { 5 });
                fx.tags.push((s, tag));
                let sec = &map.sectors[s];
                let (special, dist) = if sp.lotag == SE_FLOOR {
                    let up = sp.z < sec.floorz;
                    let special = if up {
                        FLOOR_RAISE_BY_VALUE
                    } else {
                        FLOOR_LOWER_BY_VALUE
                    };
                    (special, map_units(sp.z - sec.floorz))
                } else {
                    let down = sp.z > sec.ceilingz;
                    let special = if down {
                        CEILING_LOWER_BY_VALUE
                    } else {
                        CEILING_RAISE_BY_VALUE
                    };
                    (special, map_units(sp.z - sec.ceilingz))
                };
                let spd = (speed / 16).clamp(4, 64);
                activator_lines(map, game, s, &mut fx.lines, special, [tag, spd, dist, 0, 0]);
                fx.sector_ops
                    .entry(s)
                    .or_default()
                    .push(Op::Special(special, [tag, spd, dist, 0, 0]));
            }
            SE_FOF_FIRST..=SE_FOF_LAST => {
                let targets = fof_targets(map, s, sp.hitag);
                if targets.is_empty() {
                    continue;
                }
                let tag = channel_tag(sp.hitag, 6);
                let typ = fof_type(sp.lotag);
                for t in targets {
                    fx.tags.push((t, tag));
                }
                if let Some(wall) = own_walls(s).into_iter().next() {
                    fx.lines.push(FxLine {
                        wall,
                        special: SECTOR_SET_3DFLOOR,
                        args: [tag, typ, 0, 255, 0],
                        cross: false,
                        repeat: false,
                        impact: false,
                    });
                }
            }
            _ => {}
        }
    }

    for sp in &map.sprites {
        if i32::from(sp.picnum) != game.cycler {
            continue;
        }
        let Some(s) = sector_of(sp, nsec) else {
            continue;
        };
        let tag = use_tag(&mut fx, s);
        let a = shade_light(map.sectors[s].floorshade);
        let b = shade_light(i8::try_from(sp.lotag.clamp(-32, 48)).unwrap_or(0));
        let (upper, mut lower) = if a >= b { (a, b) } else { (b, a) };
        if upper == lower {
            lower = (upper - 16).max(32);
        }
        let tics = if sp.hitag > 0 {
            i32::from(sp.hitag).clamp(2, 70)
        } else {
            12
        };
        fx.open.push((LIGHT_GLOW, [tag, upper, lower, tics, 0]));
    }

    for ((hitag, on_open), members) in door_light_ch {
        let doors: Vec<usize> = members
            .iter()
            .map(|&(s, _)| s)
            .filter(|&s| is_ceiling_door(map.sectors[s].lotag))
            .collect();
        if doors.is_empty() {
            continue;
        }
        let kind = if on_open { 2 } else { 3 };
        let tag = channel_tag(hitag, kind);
        let bright = doors
            .iter()
            .find_map(|&d| members.iter().find(|&&(s, _)| s == d).map(|&(_, b)| b))
            .unwrap_or(255);
        for &(s, _) in &members {
            fx.tags.push((s, tag));
        }
        for d in doors {
            fx.door_lights.insert(
                d,
                DoorLight {
                    tag,
                    bright,
                    on_open,
                },
            );
        }
    }

    // Transport elevators: step into one shaft and come out in the other.
    let mut lifts: BTreeMap<i16, Vec<&Sprite>> = BTreeMap::new();
    for sp in &effectors {
        if sp.lotag == SE_TRANSPORT {
            lifts.entry(sp.hitag).or_default().push(sp);
        }
    }
    for pads in lifts.values().filter(|p| p.len() >= 2) {
        for (k, from) in pads.iter().enumerate() {
            let to = pads[(k + 1) % pads.len()];
            let Some(s) = sector_of(from, nsec) else {
                continue;
            };
            let tid = DEST_TID + i32::try_from(fx.dests.len()).unwrap_or(0);
            fx.dests.push((tid, to.x, to.y, to.ang));
            for i in own_walls(s) {
                if map.walls[i].nextwall >= 0 {
                    fx.lines.push(FxLine {
                        wall: i,
                        special: TELEPORT,
                        args: [tid, 0, 0, 0, 0],
                        cross: true,
                        repeat: true,
                        impact: false,
                    });
                }
            }
        }
    }
    plan_stacked_rooms(map, &mut fx);
    fx
}

/// Rooms stacked on the same 2D footprint become a 3D floor, even if they
/// share a wall (a loft whose loop touches the hall).
fn plan_stacked_rooms(map: &BuildMap, fx: &mut Fx) {
    let n = map.sectors.len();
    if n < 2 {
        return;
    }
    let fof_tags: BTreeSet<i32> = fx
        .lines
        .iter()
        .filter(|l| l.special == SECTOR_SET_3DFLOOR)
        .map(|l| l.args[0])
        .collect();
    let mut taken: BTreeSet<usize> = fx
        .tags
        .iter()
        .filter(|(_, t)| fof_tags.contains(t))
        .map(|&(s, _)| s)
        .collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if taken.contains(&i) || taken.contains(&j) {
                continue;
            }
            let Some(ci) = sector_centroid(map, i) else {
                continue;
            };
            let Some(cj) = sector_centroid(map, j) else {
                continue;
            };
            // Nested loft, matching loops, a 2D overlap, or a shared edge / corner.
            let nested = point_in_sector(map, i, cj) || point_in_sector(map, j, ci);
            let (overlap, edge) = match (sector_aabb(map, i), sector_aabb(map, j)) {
                (Some(a), Some(b)) => (aabbs_overlap(a, b), aabbs_share_edge(a, b)),
                _ => (false, false),
            };
            if !nested && !overlap && !edge {
                continue;
            }
            let (lower, upper) = if sector_below(map, i, j) {
                (i, j)
            } else if sector_below(map, j, i) {
                (j, i)
            } else {
                continue;
            };
            let tag = tag_of(upper);
            let Some(wall) = (0..map.walls.len()).find(|&w| wall_sector(map, w) == Some(upper))
            else {
                continue;
            };
            fx.tags.push((lower, tag));
            fx.lines.push(FxLine {
                wall,
                special: SECTOR_SET_3DFLOOR,
                args: [tag, 1, 0, 255, 0],
                cross: false,
                repeat: false,
                impact: false,
            });
            taken.insert(lower);
            taken.insert(upper);
        }
    }
}

fn sector_aabb(map: &BuildMap, s: usize) -> Option<(f64, f64, f64, f64)> {
    let sec = map.sectors.get(s)?;
    let first = usize::try_from(sec.wallptr).ok()?;
    let count = usize::try_from(sec.wallnum).ok()?;
    if count < 3 {
        return None;
    }
    let mut minx = f64::INFINITY;
    let mut miny = f64::INFINITY;
    let mut maxx = f64::NEG_INFINITY;
    let mut maxy = f64::NEG_INFINITY;
    for i in first..first + count {
        let w = map.walls.get(i)?;
        let x = f64::from(w.x);
        let y = f64::from(w.y);
        minx = minx.min(x);
        miny = miny.min(y);
        maxx = maxx.max(x);
        maxy = maxy.max(y);
    }
    (minx < maxx && miny < maxy).then_some((minx, miny, maxx, maxy))
}

fn aabbs_overlap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    let ox = a.2.min(b.2) - a.0.max(b.0);
    let oy = a.3.min(b.3) - a.1.max(b.1);
    // Any positive 2D overlap (a graze).
    ox > 0.0 && oy > 0.0
}

fn aabbs_share_edge(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    let ox = a.2.min(b.2) - a.0.max(b.0);
    let oy = a.3.min(b.3) - a.1.max(b.1);
    // Axis-aligned edge, or only a corner (odd non-box leftover).
    (ox == 0.0 && oy >= 0.0) || (oy == 0.0 && ox >= 0.0)
}

fn sector_centroid(map: &BuildMap, s: usize) -> Option<(f64, f64)> {
    let sec = map.sectors.get(s)?;
    let first = usize::try_from(sec.wallptr).ok()?;
    let count = usize::try_from(sec.wallnum).ok()?;
    if count < 3 {
        return None;
    }
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut n = 0.0;
    for i in first..first + count {
        let w = map.walls.get(i)?;
        sx += f64::from(w.x);
        sy += f64::from(w.y);
        n += 1.0;
    }
    (n > 0.0).then_some((sx / n, sy / n))
}

fn point_in_sector(map: &BuildMap, s: usize, (x, y): (f64, f64)) -> bool {
    let Some(sec) = map.sectors.get(s) else {
        return false;
    };
    let first = usize::try_from(sec.wallptr).unwrap_or(0);
    let count = usize::try_from(sec.wallnum).unwrap_or(0);
    let mut odd = false;
    for i in first..first + count {
        let Some(a) = map.walls.get(i) else {
            continue;
        };
        let Some(b) = map
            .walls
            .get(usize::try_from(a.point2).unwrap_or(usize::MAX))
        else {
            continue;
        };
        let (ax, ay, bx, by) = (
            f64::from(a.x),
            f64::from(a.y),
            f64::from(b.x),
            f64::from(b.y),
        );
        if (ay > y) != (by > y) && x < ax + (y - ay) * (bx - ax) / (by - ay) {
            odd = !odd;
        }
    }
    odd
}

#[cfg(test)]
fn sectors_adjacent(map: &BuildMap, a: usize, b: usize) -> bool {
    let in_loop = |s: usize, other: usize| {
        let Some(sec) = map.sectors.get(s) else {
            return false;
        };
        let first = usize::try_from(sec.wallptr).unwrap_or(0);
        let count = usize::try_from(sec.wallnum).unwrap_or(0);
        map.walls
            .iter()
            .skip(first)
            .take(count)
            .any(|w| usize::try_from(w.nextsector).ok() == Some(other))
    };
    in_loop(a, b) || in_loop(b, a)
}

/// `low` sits entirely under `high` in Build Z (larger z is lower).
fn sector_below(map: &BuildMap, low: usize, high: usize) -> bool {
    let Some(a) = map.sectors.get(low) else {
        return false;
    };
    let Some(b) = map.sectors.get(high) else {
        return false;
    };
    a.floorz > b.floorz && a.ceilingz >= b.floorz
}

/// Right-hand sector of wall `i`, walking the sector loop.
fn wall_sector(map: &BuildMap, wall: usize) -> Option<usize> {
    map.sectors.iter().position(|s| {
        let start = usize::try_from(s.wallptr).unwrap_or(usize::MAX);
        let count = usize::try_from(s.wallnum).unwrap_or(0);
        (start..start.saturating_add(count)).contains(&wall)
    })
}

fn is_switch(game: &GameTiles, pic: i32) -> bool {
    game.switches.contains(&pic) || game.switches.contains(&(pic - 1))
}

/// Lines whose lotag is `channel` (a switch wall, or the wall nearest a switch sprite).
fn switch_lines(
    map: &BuildMap,
    game: &GameTiles,
    channel: i16,
    lines: &mut Vec<FxLine>,
    special: i32,
    args: [i32; 5],
    repeat: bool,
) {
    if channel == 0 {
        return;
    }
    for (i, w) in map.walls.iter().enumerate() {
        if w.lotag == channel && is_switch(game, i32::from(w.picnum)) {
            lines.push(FxLine {
                wall: i,
                special,
                args,
                cross: false,
                repeat,
                impact: false,
            });
        }
    }
    for sp in &map.sprites {
        let pic = i32::from(sp.picnum);
        if sp.lotag != channel || !is_switch(game, pic) {
            continue;
        }
        if let Some(i) = nearest_wall(map, sp) {
            lines.push(FxLine {
                wall: i,
                special,
                args,
                cross: false,
                repeat,
                impact: false,
            });
        }
    }
}

/// The activator sitting in `sector` names the switch channel.
fn activator_lines(
    map: &BuildMap,
    game: &GameTiles,
    sector: usize,
    lines: &mut Vec<FxLine>,
    special: i32,
    args: [i32; 5],
) {
    for sp in &map.sprites {
        let pic = i32::from(sp.picnum);
        if usize::try_from(sp.sectnum).ok() != Some(sector) || sp.lotag == 0 {
            continue;
        }
        if pic != game.activator && pic != game.activator_locked && pic != game.touchplate {
            continue;
        }
        let cross = pic == game.touchplate;
        if cross {
            if let Ok(s) = usize::try_from(sp.sectnum) {
                for i in 0..map.walls.len() {
                    if wall_sector(map, i) == Some(s) && map.walls[i].nextwall >= 0 {
                        lines.push(FxLine {
                            wall: i,
                            special,
                            args,
                            cross: true,
                            repeat: true,
                            impact: false,
                        });
                    }
                }
            }
        } else {
            switch_lines(map, game, sp.lotag, lines, special, args, true);
        }
    }
}

/// Duke locators on one track, in visit order (Room units).
///
/// Prefer the hitag-chain from the locator whose lotag is the effector
/// hitag (Raze `FindLocator`). Fall back to same-hitag locators sorted
/// by lotag, then a contiguous lotag walk, then every locator when the
/// track id is 0 (E2L1).
fn locator_path(map: &BuildMap, game: &GameTiles, track: i16) -> Vec<(i32, i32)> {
    let locs: Vec<&Sprite> = map
        .sprites
        .iter()
        .filter(|s| i32::from(s.picnum) == game.locator)
        .collect();
    if locs.is_empty() {
        return Vec::new();
    }
    let xy = |s: &Sprite| (s.x / 16, -s.y / 16);
    let find = |lotag: i16| locs.iter().copied().find(|s| s.lotag == lotag);
    let mut chain = Vec::new();
    let mut seen = BTreeSet::new();
    let mut cur = track;
    while let Some(s) = find(cur) {
        if !seen.insert(cur) {
            break;
        }
        chain.push(xy(s));
        let next = if s.hitag != cur {
            s.hitag
        } else if s.extra >= 0 && s.extra != cur {
            s.extra
        } else {
            break;
        };
        cur = next;
        if chain.len() >= 32 {
            break;
        }
    }
    if chain.len() >= 2 {
        return chain;
    }
    let mut same: Vec<&Sprite> = locs.iter().copied().filter(|s| s.hitag == track).collect();
    if same.len() >= 2 {
        same.sort_by_key(|s| s.lotag);
        return same.into_iter().take(32).map(xy).collect();
    }
    let mut seq = Vec::new();
    if let Some(start) = find(track) {
        seq.push(xy(start));
        for dir in [1_i16, -1] {
            let mut n = track.wrapping_add(dir);
            while let Some(s) = find(n) {
                seq.push(xy(s));
                n = n.wrapping_add(dir);
                if seq.len() >= 32 {
                    break;
                }
            }
            if seq.len() >= 2 {
                break;
            }
        }
    }
    if seq.len() >= 2 {
        return seq;
    }
    if track == 0 && locs.len() >= 2 {
        let mut all = locs;
        all.sort_by_key(|s| s.lotag);
        return all.into_iter().take(32).map(xy).collect();
    }
    Vec::new()
}

fn nearest_wall(map: &BuildMap, sp: &Sprite) -> Option<usize> {
    let sec = usize::try_from(sp.sectnum).ok()?;
    let mut best: Option<(i64, usize)> = None;
    for (i, w) in map.walls.iter().enumerate() {
        if wall_sector(map, i) != Some(sec) {
            continue;
        }
        let Some(w2) = usize::try_from(w.point2)
            .ok()
            .and_then(|p| map.walls.get(p))
        else {
            continue;
        };
        let d = (i64::from(sp.x - w.x)).pow(2) + (i64::from(sp.y - w.y)).pow(2);
        let d2 = (i64::from(sp.x - w2.x)).pow(2) + (i64::from(sp.y - w2.y)).pow(2);
        let d = d.min(d2);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best.map(|(_, i)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Sector, Wall};

    #[test]
    fn east_scrolls_positive_x() {
        let (dx, dy) = angle_scroll(0, 1.0);
        assert_eq!((dx, dy), (32, 0));
    }

    #[test]
    fn south_in_build_scrolls_negative_y() {
        // Build angle 512 points down the screen (+y). Room's y is flipped.
        let (dx, dy) = angle_scroll(512, 1.0);
        assert_eq!(dx, 0);
        assert!(dy < 0, "{dy}");
    }

    #[test]
    fn stretch_east_is_positive_x() {
        assert_eq!(stretch_dir(0), (256, 0));
    }

    fn locator(lotag: i16, hitag: i16, x: i32, y: i32) -> Sprite {
        Sprite {
            picnum: 6,
            lotag,
            hitag,
            x,
            y,
            extra: -1,
            ..Sprite::default()
        }
    }

    #[test]
    fn locator_chain_follows_hitag() {
        let map = BuildMap {
            sprites: vec![locator(1, 2, 160, 0), locator(2, 1, 320, 0)],
            ..BuildMap::default()
        };
        let game = GameTiles {
            locator: 6,
            ..GameTiles::default()
        };
        assert_eq!(locator_path(&map, &game, 1), vec![(10, 0), (20, 0)]);
    }

    #[test]
    fn locator_same_hitag_sorts_by_lotag() {
        let map = BuildMap {
            sprites: vec![
                locator(4, 0, 0, 0),
                locator(8, 0, 160, 0),
                locator(6, 0, 80, 0),
            ],
            ..BuildMap::default()
        };
        let game = GameTiles {
            locator: 6,
            ..GameTiles::default()
        };
        assert_eq!(locator_path(&map, &game, 0), vec![(0, 0), (5, 0), (10, 0)]);
    }

    #[test]
    fn cycler_emits_a_light_glow() {
        let map = BuildMap {
            sectors: vec![Sector::default()],
            sprites: vec![Sprite {
                picnum: 7,
                lotag: 16,
                sectnum: 0,
                ..Sprite::default()
            }],
            ..BuildMap::default()
        };
        let game = GameTiles {
            cycler: 7,
            ..GameTiles::default()
        };
        let fx = plan(&map, &game);
        assert!(
            fx.open
                .iter()
                .any(|&(sp, args)| sp == LIGHT_GLOW && args[0] != 0),
            "CYCLER became Light_Glow ({:?})",
            fx.open
        );
    }

    #[test]
    fn teeth_follow_the_matching_lotag_29_door() {
        let map = BuildMap {
            sectors: vec![
                Sector {
                    lotag: 29,
                    hitag: 5,
                    ..Sector::default()
                },
                Sector::default(),
            ],
            sprites: vec![Sprite {
                picnum: 1,
                lotag: 22,
                hitag: 5,
                sectnum: 1,
                ang: 0,
                ..Sprite::default()
            }],
            ..BuildMap::default()
        };
        let fx = plan(&map, &GameTiles::default());
        assert_eq!(
            fx.teeth,
            vec![Teeth {
                sector: 1,
                door: 0,
                dx: 256,
                dy: 0,
                dist: 16,
                speed: 32,
            }]
        );
    }

    #[test]
    fn skew_east_oscillates_along_x() {
        let map = BuildMap {
            sectors: vec![Sector::default()],
            sprites: vec![Sprite {
                picnum: 1,
                lotag: 16,
                sectnum: 0,
                ang: 0,
                ..Sprite::default()
            }],
            ..BuildMap::default()
        };
        let fx = plan(&map, &GameTiles::default());
        assert_eq!(
            fx.skews,
            vec![Skew {
                sector: 0,
                dx: 256,
                dy: 0,
                dist: 16,
                speed: 32,
            }]
        );
    }

    #[test]
    fn fof_se40_tags_the_matching_hitag_sector() {
        let map = BuildMap {
            sectors: vec![
                Sector {
                    wallptr: 0,
                    wallnum: 1,
                    ..Sector::default()
                },
                Sector {
                    hitag: 9,
                    ..Sector::default()
                },
            ],
            walls: vec![Wall::default()],
            sprites: vec![Sprite {
                picnum: 1,
                lotag: 40,
                hitag: 9,
                sectnum: 0,
                ..Sprite::default()
            }],
            ..BuildMap::default()
        };
        let fx = plan(&map, &GameTiles::default());
        let tag = 410_000 + 9 * 8 + 6;
        assert!(
            fx.tags.iter().any(|&(s, t)| s == 1 && t == tag),
            "target tagged {tag} ({:?})",
            fx.tags
        );
        assert!(
            fx.lines
                .iter()
                .any(|l| l.special == SECTOR_SET_3DFLOOR && l.args == [tag, 1, 0, 255, 0]),
            "Sector_Set3DFloor ({:?})",
            fx.lines
                .iter()
                .map(|l| (l.special, l.args))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn stacked_rooms_become_a_3d_floor() {
        let wall = |x, y, point2| Wall {
            x,
            y,
            point2,
            nextsector: -1,
            nextwall: -1,
            ..Wall::default()
        };
        let map = BuildMap {
            sectors: vec![
                Sector {
                    wallptr: 0,
                    wallnum: 4,
                    floorz: 16_384,
                    ceilingz: 8_192,
                    ..Sector::default()
                },
                Sector {
                    wallptr: 4,
                    wallnum: 4,
                    floorz: 4_096,
                    ceilingz: 0,
                    ..Sector::default()
                },
            ],
            walls: vec![
                wall(0, 0, 1),
                wall(1024, 0, 2),
                wall(1024, 1024, 3),
                wall(0, 1024, 0),
                wall(0, 0, 5),
                wall(1024, 0, 6),
                wall(1024, 1024, 7),
                wall(0, 1024, 4),
            ],
            ..BuildMap::default()
        };
        let fx = plan(&map, &GameTiles::default());
        let tag = 300_000 + 1;
        assert!(
            fx.tags.iter().any(|&(s, t)| s == 0 && t == tag),
            "lower tagged {tag} ({:?})",
            fx.tags
        );
        assert!(
            fx.lines.iter().any(|l| {
                l.special == SECTOR_SET_3DFLOOR && l.args == [tag, 1, 0, 255, 0] && l.wall >= 4
            }),
            "upper control ({:?})",
            fx.lines
                .iter()
                .map(|l| (l.wall, l.special, l.args))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn nested_loft_becomes_a_3d_floor() {
        let wall = |x, y, point2| Wall {
            x,
            y,
            point2,
            nextsector: -1,
            nextwall: -1,
            ..Wall::default()
        };
        let map = BuildMap {
            sectors: vec![
                Sector {
                    wallptr: 0,
                    wallnum: 4,
                    floorz: 16_384,
                    ceilingz: 8_192,
                    ..Sector::default()
                },
                Sector {
                    wallptr: 4,
                    wallnum: 4,
                    floorz: 4_096,
                    ceilingz: 0,
                    ..Sector::default()
                },
            ],
            walls: vec![
                wall(0, 0, 1),
                wall(2048, 0, 2),
                wall(2048, 2048, 3),
                wall(0, 2048, 0),
                wall(256, 256, 5),
                wall(768, 256, 6),
                wall(768, 768, 7),
                wall(256, 768, 4),
            ],
            ..BuildMap::default()
        };
        let fx = plan(&map, &GameTiles::default());
        let tag = 300_000 + 1;
        assert!(
            fx.tags.iter().any(|&(s, t)| s == 0 && t == tag),
            "hall tagged {tag} ({:?})",
            fx.tags
        );
        assert!(
            fx.lines.iter().any(|l| {
                l.special == SECTOR_SET_3DFLOOR && l.args == [tag, 1, 0, 255, 0] && l.wall >= 4
            }),
            "loft control ({:?})",
            fx.lines
                .iter()
                .map(|l| (l.wall, l.special, l.args))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn overlapping_strip_becomes_a_3d_floor() {
        let wall = |x, y, point2| Wall {
            x,
            y,
            point2,
            nextsector: -1,
            nextwall: -1,
            ..Wall::default()
        };
        let map = BuildMap {
            sectors: vec![
                Sector {
                    wallptr: 0,
                    wallnum: 4,
                    floorz: 16_384,
                    ceilingz: 8_192,
                    ..Sector::default()
                },
                Sector {
                    wallptr: 4,
                    wallnum: 4,
                    floorz: 4_096,
                    ceilingz: 0,
                    ..Sector::default()
                },
            ],
            walls: vec![
                wall(0, 0, 1),
                wall(1024, 0, 2),
                wall(1024, 1024, 3),
                wall(0, 1024, 0),
                wall(768, 0, 5),
                wall(1792, 0, 6),
                wall(1792, 1024, 7),
                wall(768, 1024, 4),
            ],
            ..BuildMap::default()
        };
        let fx = plan(&map, &GameTiles::default());
        let tag = 300_000 + 1;
        assert!(
            fx.tags.iter().any(|&(s, t)| s == 0 && t == tag),
            "overlap tagged {tag} ({:?})",
            fx.tags
        );
        assert!(
            fx.lines.iter().any(|l| {
                l.special == SECTOR_SET_3DFLOOR && l.args == [tag, 1, 0, 255, 0] && l.wall >= 4
            }),
            "overlap control ({:?})",
            fx.lines
                .iter()
                .map(|l| (l.wall, l.special, l.args))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn shared_wall_loft_becomes_a_3d_floor() {
        let wall = |x, y, point2, nextsector| Wall {
            x,
            y,
            point2,
            nextsector,
            nextwall: -1,
            ..Wall::default()
        };
        let map = BuildMap {
            sectors: vec![
                Sector {
                    wallptr: 0,
                    wallnum: 4,
                    floorz: 16_384,
                    ceilingz: 8_192,
                    ..Sector::default()
                },
                Sector {
                    wallptr: 4,
                    wallnum: 4,
                    floorz: 4_096,
                    ceilingz: 0,
                    ..Sector::default()
                },
            ],
            walls: vec![
                wall(0, 0, 1, -1),
                wall(2048, 0, 2, -1),
                wall(2048, 2048, 3, -1),
                wall(0, 2048, 0, -1),
                wall(256, 256, 5, 0),
                wall(768, 256, 6, -1),
                wall(768, 768, 7, -1),
                wall(256, 768, 4, -1),
            ],
            ..BuildMap::default()
        };
        assert!(sectors_adjacent(&map, 0, 1));
        let fx = plan(&map, &GameTiles::default());
        let tag = 300_000 + 1;
        assert!(
            fx.tags.iter().any(|&(s, t)| s == 0 && t == tag),
            "hall tagged {tag} ({:?})",
            fx.tags
        );
        assert!(
            fx.lines.iter().any(|l| {
                l.special == SECTOR_SET_3DFLOOR && l.args == [tag, 1, 0, 255, 0] && l.wall >= 4
            }),
            "shared-wall loft ({:?})",
            fx.lines
                .iter()
                .map(|l| (l.wall, l.special, l.args))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn grazing_overlap_becomes_a_3d_floor() {
        let wall = |x, y, point2| Wall {
            x,
            y,
            point2,
            nextsector: -1,
            nextwall: -1,
            ..Wall::default()
        };
        let map = BuildMap {
            sectors: vec![
                Sector {
                    wallptr: 0,
                    wallnum: 4,
                    floorz: 16_384,
                    ceilingz: 8_192,
                    ..Sector::default()
                },
                Sector {
                    wallptr: 4,
                    wallnum: 4,
                    floorz: 4_096,
                    ceilingz: 0,
                    ..Sector::default()
                },
            ],
            walls: vec![
                wall(0, 0, 1),
                wall(1024, 0, 2),
                wall(1024, 1024, 3),
                wall(0, 1024, 0),
                wall(992, 992, 5),
                wall(2016, 992, 6),
                wall(2016, 2016, 7),
                wall(992, 2016, 4),
            ],
            ..BuildMap::default()
        };
        let fx = plan(&map, &GameTiles::default());
        let tag = 300_000 + 1;
        assert!(
            fx.tags.iter().any(|&(s, t)| s == 0 && t == tag),
            "graze tagged {tag} ({:?})",
            fx.tags
        );
        assert!(
            fx.lines.iter().any(|l| {
                l.special == SECTOR_SET_3DFLOOR && l.args == [tag, 1, 0, 255, 0] && l.wall >= 4
            }),
            "graze control ({:?})",
            fx.lines
                .iter()
                .map(|l| (l.wall, l.special, l.args))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn shared_edge_rooms_become_a_3d_floor() {
        let wall = |x, y, point2| Wall {
            x,
            y,
            point2,
            nextsector: -1,
            nextwall: -1,
            ..Wall::default()
        };
        let map = BuildMap {
            sectors: vec![
                Sector {
                    wallptr: 0,
                    wallnum: 4,
                    floorz: 16_384,
                    ceilingz: 8_192,
                    ..Sector::default()
                },
                Sector {
                    wallptr: 4,
                    wallnum: 4,
                    floorz: 4_096,
                    ceilingz: 0,
                    ..Sector::default()
                },
            ],
            walls: vec![
                wall(0, 0, 1),
                wall(1024, 0, 2),
                wall(1024, 1024, 3),
                wall(0, 1024, 0),
                wall(1024, 0, 5),
                wall(2048, 0, 6),
                wall(2048, 1024, 7),
                wall(1024, 1024, 4),
            ],
            ..BuildMap::default()
        };
        let fx = plan(&map, &GameTiles::default());
        let tag = 300_000 + 1;
        assert!(
            fx.tags.iter().any(|&(s, t)| s == 0 && t == tag),
            "edge tagged {tag} ({:?})",
            fx.tags
        );
        assert!(
            fx.lines.iter().any(|l| {
                l.special == SECTOR_SET_3DFLOOR && l.args == [tag, 1, 0, 255, 0] && l.wall >= 4
            }),
            "shared-edge loft ({:?})",
            fx.lines
                .iter()
                .map(|l| (l.wall, l.special, l.args))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn shared_corner_rooms_become_a_3d_floor() {
        let wall = |x, y, point2| Wall {
            x,
            y,
            point2,
            nextsector: -1,
            nextwall: -1,
            ..Wall::default()
        };
        let map = BuildMap {
            sectors: vec![
                Sector {
                    wallptr: 0,
                    wallnum: 4,
                    floorz: 16_384,
                    ceilingz: 8_192,
                    ..Sector::default()
                },
                Sector {
                    wallptr: 4,
                    wallnum: 4,
                    floorz: 4_096,
                    ceilingz: 0,
                    ..Sector::default()
                },
            ],
            walls: vec![
                wall(0, 0, 1),
                wall(1024, 0, 2),
                wall(1024, 1024, 3),
                wall(0, 1024, 0),
                wall(1024, 1024, 5),
                wall(2048, 1024, 6),
                wall(2048, 2048, 7),
                wall(1024, 2048, 4),
            ],
            ..BuildMap::default()
        };
        let fx = plan(&map, &GameTiles::default());
        let tag = 300_000 + 1;
        assert!(
            fx.tags.iter().any(|&(s, t)| s == 0 && t == tag),
            "corner tagged {tag} ({:?})",
            fx.tags
        );
        assert!(
            fx.lines.iter().any(|l| {
                l.special == SECTOR_SET_3DFLOOR && l.args == [tag, 1, 0, 255, 0] && l.wall >= 4
            }),
            "shared-corner loft ({:?})",
            fx.lines
                .iter()
                .map(|l| (l.wall, l.special, l.args))
                .collect::<Vec<_>>()
        );
    }
}
