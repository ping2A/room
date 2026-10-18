//! Build map → UDMF `TEXTMAP` (namespace `zdoom`).
//!
//! Units follow Raze: map x / y divide by 16 and z by 256 (z grows down in
//! Build, up in Room), and y flips so Build's clockwise sector loops keep
//! their sector on the right-hand (front) side of each line. Two walls that
//! face each other (`nextwall`) become one two-sided line. Wall repeat /
//! panning become per-side UDMF scale / offset, shades become absolute side
//! and sector light, slopes (`heinum`) become exact plane equations and
//! parallaxed planes use `F_SKY1`.
//!
//! Gameplay: player / coop / deathmatch starts (`APLAYER`), sector lotags
//! (doors 20 / 21 / 22 / 26, lifts 16 / 17, elevators 18 / 19, secret 32767,
//! end of level −1) as line specials, switches and access switches wired to
//! the sectors holding a matching `ACTIVATOR`, touchplates, `SE 7`
//! transporters (teleport lines + `TeleportDest`), `SE 4` flickering lights,
//! `SE 10` auto-closing doors, damaging floor tiles and the nuke button exit.
//! Every other sprite goes through a [`SpriteMapper`] (see [`crate::duke`]).

use crate::acs::{AcsScript, Op};
use crate::art::TileSet;
use crate::condata::ConData;
use crate::error::BuildError;
use crate::fx::move_to;
use crate::map::{BuildMap, Sprite};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Thing for a converted map sprite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapThing {
    /// UDMF thing type (editor number).
    pub ednum: i32,
    /// Lowest Duke skill (1–4) the thing appears on; 0 = every skill.
    pub min_skill: u8,
    /// Radius and height of a walking monster. Build clips sprites by their
    /// centre, so the converter moves one whose Doom box crosses a blocking
    /// line to the nearest free spot in its sector.
    pub size: Option<(i32, i32)>,
    /// UDMF `arg0`…`arg4` (0 = omit).
    pub args: [i32; 5],
}

/// How a door, lift or elevator sector sounds when it moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoverSound {
    /// Room's generic mover sounds.
    Default,
    /// No sound (UDMF `silent`).
    Silent,
    /// UDMF `soundsequence`: a SNDSEQ choice list (0 open, 1 close).
    Sequence(String),
}

/// Chooses the thing (if any) for each map sprite.
pub trait SpriteMapper {
    /// Thing for `sprite`; `in_mover` is set when its sector is a door, lift
    /// or elevator.
    fn thing(&mut self, sprite: &Sprite, in_mover: bool) -> Option<MapThing>;

    /// Sound of a mover sector whose `MUSICANDSFX` sprite (if any) has these
    /// `(lotag, hitag)`: Build plays the lotag sound on opening and the hitag
    /// one on closing.
    fn mover_sound(&mut self, _sfx: Option<(i16, i16)>) -> MoverSound {
        MoverSound::Default
    }

    /// What shooting wall picture `pic` leaves; `masked` is set for the
    /// masked picture of a two-sided wall.
    fn wall_break(&mut self, _pic: i32, _masked: bool) -> Option<WallBreak> {
        None
    }
}

/// A wall picture that breaks when shot (Raze `checkhitwall`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallBreak {
    /// Picture left behind; `None` removes a masked picture.
    pub broken: Option<i32>,
    /// Dormant thing whose `Active` state plays the sound and throws debris.
    pub debris: i32,
    /// The wall stops blocking (glass, the fan grate).
    pub unblock: bool,
    /// Shooting it out darkens the `SE 3` lights with the wall's lotag.
    pub light: bool,
    /// Other glass with the same wall lotag breaks with this pane (`SE 128`).
    pub chain: bool,
}

/// Converts no sprites (geometry, starts and specials only).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSprites;

impl SpriteMapper for NoSprites {
    fn thing(&mut self, _: &Sprite, _: bool) -> Option<MapThing> {
        None
    }
}

/// Tiles the converter gives gameplay meaning (Duke Nukem 3D numbering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameTiles {
    /// `SECTOREFFECTOR`.
    pub effector: i32,
    /// `ACTIVATOR`.
    pub activator: i32,
    /// `ACTIVATORLOCKED`.
    pub activator_locked: i32,
    /// `TOUCHPLATE`.
    pub touchplate: i32,
    /// `NUKEBUTTON`.
    pub nukebutton: i32,
    /// Switch tiles (the "off" picture; `+1` is "on").
    pub switches: BTreeSet<i32>,
    /// Switches that need the keycard of their palette.
    pub access_switches: BTreeSet<i32>,
    /// Floor tiles that hurt.
    pub hurt_floors: BTreeMap<i32, i32>,
    /// `GPSPEED` sprite.
    pub gpspeed: i32,
    /// `LOCATORS` (subway / train waypoints).
    pub locator: i32,
    /// `CYCLER` (sector shade pulse).
    pub cycler: i32,
    /// `MIRROR` (masked picture of a one-way mirror wall).
    pub mirror: i32,
    /// `MUSICANDSFX` (ambient / mover sound sprite).
    pub music_and_sfx: i32,
    /// C-9 charges (`SEENINE`, `OOZFILTER`): one blowing sets off the rest of
    /// its hitag and the effectors sharing it.
    pub charges: BTreeSet<i32>,
    /// `MASTERSWITCH`: the switch with its lotag sets off its sector's charges.
    pub master_switch: i32,
    /// `FANSPRITE`, and the `FANSHADOW` floor it turns to `FANSHADOWBROKE`.
    pub fan: [i32; 3],
    /// `BIGFORCE` / `W_FORCEFIELD` / `FORCERIPPLE` wall pictures.
    pub force_walls: BTreeSet<i32>,
}

const SWITCH_NAMES: &[(&str, i32)] = &[
    ("ACCESSSWITCH", 130),
    ("SLOTDOOR", 132),
    ("LIGHTSWITCH", 134),
    ("SPACEDOORSWITCH", 136),
    ("SPACELIGHTSWITCH", 138),
    ("FRANKENSTINESWITCH", 140),
    ("MULTISWITCH", 146),
    ("DIPSWITCH", 162),
    ("DIPSWITCH2", 164),
    ("TECHSWITCH", 166),
    ("DIPSWITCH3", 168),
    ("ACCESSSWITCH2", 170),
    ("LIGHTSWITCH2", 712),
    ("POWERSWITCH1", 860),
    ("LOCKSWITCH1", 862),
    ("POWERSWITCH2", 864),
    ("HANDSWITCH", 1111),
    ("PULLSWITCH", 1122),
    ("ALIENSWITCH", 1142),
    ("HANDPRINTSWITCH", 1155),
];

const HURT_FLOORS: &[(&str, i32, i32)] = &[
    ("FLOORSLIME", 200, 5),
    ("HURTRAIL", 859, 10),
    ("FLOORPLASMA", 1082, 20),
    ("PURPLELAVA", 4240, 10),
];

impl Default for GameTiles {
    fn default() -> Self {
        Self::from_con(&ConData::default())
    }
}

impl GameTiles {
    /// Tile numbers from the CON `define`s, Duke 1.3D / 1.5 numbers otherwise.
    #[must_use]
    pub fn from_con(con: &ConData) -> Self {
        let get = |name: &str, default: i32| con.value(name).unwrap_or(default);
        let set = |list: &[(&str, i32)]| list.iter().map(|&(n, d)| get(n, d)).collect();
        let hurt = HURT_FLOORS
            .iter()
            .map(|&(n, d, dmg)| (get(n, d), dmg))
            .collect();
        Self {
            effector: get("SECTOREFFECTOR", 1),
            activator: get("ACTIVATOR", 2),
            activator_locked: get("ACTIVATORLOCKED", 4),
            touchplate: get("TOUCHPLATE", 3),
            nukebutton: get("NUKEBUTTON", i32::from(NUKEBUTTON)),
            switches: set(SWITCH_NAMES),
            access_switches: set(&SWITCH_NAMES[..1])
                .into_iter()
                .chain([get("ACCESSSWITCH2", 170)])
                .collect(),
            hurt_floors: hurt,
            gpspeed: get("GPSPEED", 106),
            locator: get("LOCATORS", 6),
            cycler: get("CYCLER", 7),
            mirror: get("MIRROR", 560),
            music_and_sfx: get("MUSICANDSFX", 5),
            charges: [get("SEENINE", 1247), get("OOZFILTER", 1079)].into(),
            master_switch: get("MASTERSWITCH", 8),
            fan: [
                get("FANSPRITE", 407),
                get("FANSHADOW", 412),
                get("FANSHADOWBROKE", 416),
            ],
            force_walls: [
                get("BIGFORCE", 230),
                get("W_FORCEFIELD", 430),
                get("FORCERIPPLE", 431),
            ]
            .into_iter()
            .filter(|&t| t != 0)
            .collect(),
        }
    }
}

/// What a sector moves like when operated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Mover {
    Door,
    FloorDoor,
    PlatDown,
    PlatUp,
    ElevDown,
    ElevUp,
}

fn mover(lotag: i16) -> Option<Mover> {
    match lotag {
        LT_CEILING_DOOR | LT_SPLIT_DOOR | LT_SPLIT_STAR_DOOR | LT_TEETH_DOOR => Some(Mover::Door),
        LT_FLOOR_DOOR => Some(Mover::FloorDoor),
        16 => Some(Mover::PlatDown),
        17 => Some(Mover::PlatUp),
        18 => Some(Mover::ElevDown),
        19 => Some(Mover::ElevUp),
        _ => None,
    }
}

/// Line special + args that operate `tag` like `m`.
fn mover_special(m: Mover, tag: i32, auto_close: bool) -> (i32, [i32; 5]) {
    match m {
        Mover::Door if auto_close => (DOOR_RAISE, [tag, DOOR_SPEED, DOOR_WAIT, 0, 0]),
        // Build doors stay open until used again; a re-used raising door
        // reverses, so a raise that waits ~30 minutes is a toggle.
        Mover::Door => (DOOR_RAISE, [tag, DOOR_SPEED, DOOR_STAY_OPEN, 0, 0]),
        Mover::FloorDoor => (FLOOR_LOWER_TO_LOWEST, [tag, DOOR_SPEED, 0, 0, 0]),
        Mover::PlatDown => (PLAT_DOWN_WAIT_UP_STAY, [tag, DOOR_SPEED, DOOR_WAIT, 0, 0]),
        Mover::PlatUp => (PLAT_UP_WAIT_DOWN_STAY, [tag, DOOR_SPEED, DOOR_WAIT, 0, 0]),
        Mover::ElevDown => (ELEVATOR_LOWER_TO_NEAREST, [tag, DOOR_SPEED / 2, 0, 0, 0]),
        Mover::ElevUp => (ELEVATOR_RAISE_TO_NEAREST, [tag, DOOR_SPEED / 2, 0, 0, 0]),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    Use,
    Cross,
    /// Use, or walking into the line. Swing doors open when Duke runs into them.
    UseOrCross,
}

#[derive(Debug, Clone, Copy)]
struct LineAction {
    special: i32,
    args: [i32; 5],
    trigger: Trigger,
    repeat: bool,
    lock: i32,
    /// The line runs when shot.
    impact: bool,
}

impl LineAction {
    fn write(&self, t: &mut String) {
        let _ = write!(t, " special = {};", self.special);
        for (k, a) in self.args.iter().enumerate() {
            if *a != 0 || k == 0 {
                let _ = write!(t, " arg{k} = {a};");
            }
        }
        // Room never raises impact activations: a shot line runs its
        // special when its 1 health is gone.
        t.push_str(match (self.impact, self.trigger) {
            (true, _) => " health = 1; deathspecial = true;",
            (false, Trigger::Use) => " playeruse = true; playeruseback = true;",
            (false, Trigger::UseOrCross) => {
                " playeruse = true; playeruseback = true; playercross = true;"
            }
            (false, Trigger::Cross) => " playercross = true;",
        });
        if self.special == TELEPORT {
            t.push_str(" monstercross = true;");
        }
        if self.repeat {
            t.push_str(" repeatspecial = true;");
        }
        if self.lock != 0 {
            let _ = write!(t, " locknumber = {};", self.lock);
        }
    }
}

/// Append a converter script; returns its number.
fn push_script(scripts: &mut Vec<AcsScript>, ops: Vec<Op>) -> i32 {
    let n = FIRST_SCRIPT + i32::try_from(scripts.len()).unwrap_or(0);
    scripts.push((n, ops));
    n
}

/// Sector tag for activator group `lotag` of mover kind `m`.
fn group_tag(lotag: i16, m: Mover) -> i32 {
    100_000 + (i32::from(lotag) & 0xffff) * 8 + m as i32
}

/// Build xy units per Room map unit.
pub const XY_SCALE: f64 = 16.0;
/// Build z units per Room map unit.
pub const Z_SCALE: f64 = 256.0;

/// Duke player start / multiplayer spawn sprite.
pub const APLAYER: i16 = 1405;
/// Duke end-of-level button sprite.
pub const NUKEBUTTON: i16 = 142;

/// Sector lotags converted to doors.
const LT_BRIDGE: i16 = 27;
const LT_TEETH_DOOR: i16 = 29;
const LT_CEILING_DOOR: i16 = 20;
const LT_FLOOR_DOOR: i16 = 21;
const LT_SPLIT_DOOR: i16 = 22;
const LT_SPLIT_STAR_DOOR: i16 = 26;

/// Hexen / ZDoom line specials.
const DOOR_RAISE: i32 = 12;
const FLOOR_LOWER_TO_LOWEST: i32 = 21;
const PLAT_DOWN_WAIT_UP_STAY: i32 = 62;
const PLAT_UP_WAIT_DOWN_STAY: i32 = 64;
const TELEPORT: i32 = 70;
const LINE_MIRROR: i32 = 182;
const POLYOBJ_START_LINE: i32 = 1;
const SECTOR_SET_PORTAL: i32 = 57;
/// Room's Build water-link portal kind (`room_map::PORTAL_KIND_WATER`).
pub const WATER_PORTAL: i32 = 200;
/// Room's Build fall-link portal kind (`room_map::PORTAL_KIND_FALL`).
pub const FALL_PORTAL: i32 = 201;

/// `arg2` of a [`FALL_PORTAL`]: source effector height in the low 16 bits,
/// destination in the high 16 (both signed map units).
fn pack_fall_heights(src: i16, dst: i16) -> i32 {
    (i32::from(dst) << 16) | i32::from(src as u16)
}
const ACS_EXECUTE: i32 = 80;
/// Polyobject spawn spot / anchor things.
const POLY_SPAWN: i32 = 9301;
const POLY_ANCHOR: i32 = 9300;
const EXIT_NORMAL: i32 = 243;
const ELEVATOR_RAISE_TO_NEAREST: i32 = 245;
const ELEVATOR_LOWER_TO_NEAREST: i32 = 247;

/// Door speed in map units per tic.
const DOOR_SPEED: i32 = 16;
/// Door / lift wait in tics.
const DOOR_WAIT: i32 = 150;
/// Wait of a door that only closes when used again.
const DOOR_STAY_OPEN: i32 = 35 * 60 * 30;

/// Doom sector specials.
const ST_FLICKER: i32 = 17;
const ST_SECRET: i32 = 9;
/// Duke sector lotags.
const LT_SECRET: i16 = 32767;
const LT_END_LEVEL: i16 = -1;
/// Sector lotag 65534: end the level (the quote is not shown).
const LT_END_MESSAGE: i16 = -2;
const LT_ABOVE_WATER: i16 = 1;
const LT_UNDER_WATER: i16 = 2;
/// Effector lotags.
const SE_RANDOM_LIGHTS: i16 = 4;
const SE_TRANSPORTER: i16 = 7;
/// One-way teleporter exit (`SE 23`). Sources stay `SE 7`.
const SE_ONE_WAY: i16 = 23;
const SE_DOOR_AUTO_CLOSE: i16 = 10;
/// Doom `TeleportDest`.
const THING_TELEPORT_DEST: i32 = 14;
/// First number of the converter's own ACS scripts (after the polyobject
/// ones, before the `OPEN` range).
const FIRST_SCRIPT: i32 = 500;
/// ACS map variables (run-once flags of the charge groups).
const MAP_VARS: i32 = 128;
/// Thing ids of C-9 charge groups and of wall-break debris spots.
const CHARGE_TID: i32 = 20_000;
const BREAK_TID: i32 = 21_000;
/// Line ids of breakable walls.
const BREAK_LINE: i32 = 430_000;
/// UDMF `healthgroup` of glass panes that share a wall lotag.
const GLASS_GROUP: i32 = 5_000;
const FLOOR_MOVE_TO_VALUE: i32 = 37;
const LINE_SET_BLOCKING: i32 = 55;
/// `Line_SetBlocking` flags: creatures and hitscans.
const BLOCK_CREATURES_HITSCAN: i32 = 1 | 512;
/// First thing id for teleport destinations.
const TELEPORT_TID: i32 = 1000;

/// Picture name for tile `picnum` in the converted WAD.
#[must_use]
pub fn tile_name(picnum: i32) -> String {
    format!("BT{picnum:04}")
}

/// Room map name for a Build map file: `E1L1.MAP` → `E1M1`, other names keep
/// their stem when it fits a lump name.
#[must_use]
pub fn doom_map_name(file: &str) -> Option<String> {
    let stem = file
        .strip_suffix(".MAP")
        .or_else(|| file.strip_suffix(".map"))?;
    let up = stem.to_ascii_uppercase();
    let b = up.as_bytes();
    if b.len() >= 4 && b[0] == b'E' && b[1].is_ascii_digit() && b[2] == b'L' {
        let level = &up[3..];
        if !level.is_empty() && level.bytes().all(|c| c.is_ascii_digit()) {
            return Some(format!("E{}M{level}", char::from(b[1])));
        }
    }
    (!up.is_empty() && up.len() <= 8 && up.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
        .then_some(up)
}

/// Converted map plus what the WAD needs to ship with it.
#[derive(Debug, Clone, Default)]
pub struct ConvertedMap {
    /// UDMF text.
    pub textmap: String,
    /// Tiles referenced by walls / planes (and the sky).
    pub tiles: BTreeSet<i32>,
    /// Most common parallax ceiling tile.
    pub sky: Option<i32>,
    /// Switch tiles used on walls: tile `p` is written as `SW1Dpppp` and
    /// `p + 1` as `SW2Dpppp` so Room's switch flip finds the pair.
    pub switch_tiles: BTreeSet<i32>,
    /// ACS0 `BEHAVIOR` driving door and rotating-sector polyobjects.
    pub behavior: Option<Vec<u8>>,
    /// Polyobjects made from moving sectors.
    pub polyobjects: usize,
}

/// Wall texture name for switch picture `p` (off) / `p + 1` (on).
#[must_use]
pub fn switch_name(p: i32, on: bool) -> String {
    format!("SW{}D{p:04}", if on { 2 } else { 1 })
}

/// Build shade (0 = full bright, ~31 = dark, negative = glowing) → Room light.
#[must_use]
pub fn shade_light(shade: i8) -> i32 {
    (255 - 6 * i32::from(shade)).clamp(48, 255)
}

/// Room flat mapping for one Build plane: `(rotation°, xscale, yscale, xpan, ypan)`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FlatAlign {
    rotation: f64,
    scale_u: f64,
    scale_v: f64,
    pan_u: f64,
    pan_v: f64,
}

/// Build/Raze plane UVs (`UVCalculator`) expressed in Room's
/// `u = (rot(x, y).x + pan_u) / 64 / scale_u` model. Flats tile at the
/// power-of-two tile size, one texel per world unit (half with stat 8);
/// stat 64 aligns to the first wall, 4 swaps, 16/32 flip, panning is in
/// 1/256 of the tile. `w0`/`w1` are the first wall's ends in Room space.
fn flat_align(
    stat: u16,
    pan: (u8, u8),
    heinum: i16,
    tile: (f64, f64),
    w0: (f64, f64),
    w1: (f64, f64),
) -> FlatAlign {
    let pow2 = |s: f64| {
        let mut p = 1.0;
        while p < s && p < 65536.0 {
            p *= 2.0;
        }
        p
    };
    let half = if stat & 8 != 0 { 0.5 } else { 1.0 };
    let (xs, ys) = (pow2(tile.0) * half, pow2(tile.1) * half);
    // (tu, tv) = m · (x, y) + t
    let (mut m, mut t) = ([[1.0, 0.0], [0.0, 1.0]], [0.0, 0.0]);
    let (dx, dy) = (w1.0 - w0.0, w1.1 - w0.1);
    let len = dx.hypot(dy);
    if stat & 64 != 0 && len > 1e-9 {
        let (c, s) = (dx / len, dy / len);
        m = [[c, s], [s, -c]];
        t = [-(c * w0.0 + s * w0.1), -(s * w0.0 - c * w0.1)];
        if stat & 2 != 0 {
            let f = (1.0 + (f64::from(heinum) / 4096.0).powi(2)).sqrt();
            m[1] = [m[1][0] * f, m[1][1] * f];
            t[1] *= f;
        }
    }
    if stat & 4 != 0 {
        m.swap(0, 1);
        t.swap(0, 1);
    }
    if stat & 16 != 0 {
        m[0] = [-m[0][0], -m[0][1]];
        t[0] = -t[0];
    }
    if stat & 32 != 0 {
        m[1] = [-m[1][0], -m[1][1]];
        t[1] = -t[1];
    }
    // Row 0 = l0 · (cos, -sin), row 1 = l1 · (sin, cos).
    let l0 = m[0][0].hypot(m[0][1]).max(1e-9);
    let (c, s) = (m[0][0] / l0, -m[0][1] / l0);
    let mut l1 = m[1][0] * s + m[1][1] * c;
    if l1.abs() < 1e-9 {
        l1 = 1.0;
    }
    let pu = f64::from(pan.0) / 256.0;
    let pv = f64::from(pan.1) / 256.0;
    FlatAlign {
        rotation: s.atan2(c).to_degrees(),
        scale_u: xs / (64.0 * l0),
        scale_v: ys / (64.0 * l1),
        pan_u: (t[0] + pu * xs) / l0,
        pan_v: (t[1] + pv * ys) / l1,
    }
}

fn fmt_f(v: f64) -> String {
    let v = v + 0.0;
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

struct Plane {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
}

/// Convert one map's geometry, starts and specials (no other sprites).
/// `tiles` supplies tile sizes for panning.
pub fn convert_map(map: &BuildMap, tiles: &TileSet<'_>) -> Result<ConvertedMap, BuildError> {
    convert_map_with(map, tiles, &GameTiles::default(), &mut NoSprites)
}

/// Convert one map; `game` names the gameplay tiles and `sprites` turns map
/// sprites into things.
pub fn convert_map_with(
    map: &BuildMap,
    tiles: &TileSet<'_>,
    game: &GameTiles,
    sprites: &mut dyn SpriteMapper,
) -> Result<ConvertedMap, BuildError> {
    let n = map.walls.len();
    if map.sectors.is_empty() || n == 0 {
        return Err(BuildError::Invalid("map has no sectors"));
    }
    let fx = crate::fx::plan(map, game);
    let shut;
    let map: &BuildMap = if fx.closed.is_empty() {
        map
    } else {
        shut = fx.shut(map);
        &shut
    };
    let wsec = map.wall_sectors();
    let pos = |x: i32, y: i32| (f64::from(x) / XY_SCALE, -f64::from(y) / XY_SCALE);
    let tile_size = |pic: i16| {
        tiles
            .get(i32::from(pic))
            .map_or((64.0, 64.0), |t| (f64::from(t.width), f64::from(t.height)))
    };
    let mut out = ConvertedMap::default();
    let tex = |pic: i16, out: &mut ConvertedMap| -> String {
        let p = i32::from(pic);
        if tiles.get(p).is_none() {
            return "-".into();
        }
        let (base, on) = if game.switches.contains(&p) {
            (p, false)
        } else if game.switches.contains(&(p - 1)) {
            (p - 1, true)
        } else {
            out.tiles.insert(p);
            return tile_name(p);
        };
        if tiles.get(base).is_some() && tiles.get(base + 1).is_some() {
            out.switch_tiles.insert(base);
            switch_name(base, on)
        } else {
            out.tiles.insert(p);
            tile_name(p)
        }
    };
    let sector_tag = |s: usize| -> i32 { i32::try_from(s).unwrap_or(i32::MAX - 1) + 1 };
    let sec_of = |sp: &Sprite| {
        usize::try_from(sp.sectnum)
            .ok()
            .filter(|&s| s < map.sectors.len())
    };
    let sector_walls =
        |s: usize| -> Vec<usize> { (0..n).filter(|&i| wsec[i] == Some(s)).collect() };

    // Effectors, activator groups and transporters.
    let mut groups: BTreeMap<i16, BTreeSet<usize>> = BTreeMap::new();
    let mut locked = BTreeSet::new();
    let mut auto_close = BTreeSet::new();
    let mut flicker = BTreeSet::new();
    let mut transporters: BTreeMap<i16, Vec<&Sprite>> = BTreeMap::new();
    let mut water: BTreeMap<i16, Vec<(&Sprite, usize)>> = BTreeMap::new();
    for sp in &map.sprites {
        let Some(s) = sec_of(sp) else { continue };
        let pic = i32::from(sp.picnum);
        if pic == game.activator || pic == game.activator_locked {
            groups.entry(sp.lotag).or_default().insert(s);
            if pic == game.activator_locked {
                locked.insert(s);
            }
        } else if pic == game.effector {
            match sp.lotag {
                SE_RANDOM_LIGHTS => {
                    flicker.insert(s);
                }
                SE_DOOR_AUTO_CLOSE => {
                    auto_close.insert(s);
                }
                SE_TRANSPORTER | SE_ONE_WAY
                    if !matches!(map.sectors[s].lotag, LT_ABOVE_WATER | LT_UNDER_WATER) =>
                {
                    transporters.entry(sp.hitag).or_default().push(sp);
                }
                SE_TRANSPORTER => water.entry(sp.hitag).or_default().push((sp, s)),
                _ => {}
            }
        }
    }
    // Extra sector tags: (sector → group tags).
    let mut more_ids: BTreeMap<usize, BTreeSet<i32>> = BTreeMap::new();
    // Water links: diving through an above-water floor surfaces under the
    // linked underwater ceiling and back, moved by the effectors' offset.
    let mut water_links: BTreeMap<usize, [i32; 5]> = BTreeMap::new();
    for pads in water.values() {
        for &(a, sa) in pads {
            let (plane, want) = match map.sectors[sa].lotag {
                LT_ABOVE_WATER => (0, LT_UNDER_WATER),
                _ => (1, LT_ABOVE_WATER),
            };
            let Some(&(b, sb)) = pads.iter().find(|(_, sb)| map.sectors[*sb].lotag == want) else {
                continue;
            };
            let tag = sector_tag(sb);
            more_ids.entry(sb).or_default().insert(tag);
            let dx = (f64::from(b.x - a.x) / XY_SCALE).round() as i32;
            let dy = (-f64::from(b.y - a.y) / XY_SCALE).round() as i32;
            water_links
                .entry(sa)
                .or_insert([tag, WATER_PORTAL, plane, dx, dy]);
        }
    }
    let group_kind = |lotag: i16| -> Option<Mover> {
        let mut votes: BTreeMap<Mover, usize> = BTreeMap::new();
        for &s in groups.get(&lotag)? {
            if let Some(m) = mover(map.sectors[s].lotag) {
                *votes.entry(m).or_default() += 1;
            }
        }
        votes
            .iter()
            .max_by_key(|&(m, c)| (*c, std::cmp::Reverse(*m)))
            .map(|(m, _)| *m)
    };

    let mut scripts: Vec<AcsScript> = Vec::new();
    // Floor doors (lotag 21) toggle: a shut floor drops to the next floor
    // down around it, an open one rises back to the ceiling.
    let floor_door = |s: usize| -> Option<Op> {
        let sec = &map.sectors[s];
        let open = sector_walls(s)
            .into_iter()
            .filter_map(|i| usize::try_from(map.walls[i].nextsector).ok())
            .filter_map(|o| map.sectors.get(o))
            .map(|o| o.floorz)
            .filter(|&z| z > sec.ceilingz)
            .min()
            .or((sec.floorz != sec.ceilingz).then_some(sec.floorz))?;
        let tag = sector_tag(s);
        Some(Op::IfShut {
            tag,
            open: vec![move_to(FLOOR_MOVE_TO_VALUE, tag, DOOR_SPEED, open)],
            close: vec![move_to(FLOOR_MOVE_TO_VALUE, tag, DOOR_SPEED, sec.ceilingz)],
        })
    };
    let mut floor_doors: BTreeMap<i32, i32> = BTreeMap::new();
    for s in 0..map.sectors.len() {
        if mover(map.sectors[s].lotag) == Some(Mover::FloorDoor) {
            if let Some(op) = floor_door(s) {
                floor_doors.insert(sector_tag(s), push_script(&mut scripts, vec![op]));
            }
        }
    }
    for (&lotag, secs) in &groups {
        if group_kind(lotag) == Some(Mover::FloorDoor) {
            let ops: Vec<Op> = secs
                .iter()
                .filter(|&&s| mover(map.sectors[s].lotag) == Some(Mover::FloorDoor))
                .filter_map(|&s| floor_door(s))
                .collect();
            if !ops.is_empty() {
                let tag = group_tag(lotag, Mover::FloorDoor);
                floor_doors.insert(tag, push_script(&mut scripts, ops));
            }
        }
    }
    let operate = |m: Mover, tag: i32, auto: bool| -> (i32, [i32; 5]) {
        match floor_doors.get(&tag) {
            Some(&script) if m == Mover::FloorDoor => (ACS_EXECUTE, [script, 0, 0, 0, 0]),
            _ => mover_special(m, tag, auto),
        }
    };

    let group_action = |lotag: i16,
                        trigger: Trigger,
                        repeat: bool,
                        lock: i32,
                        more_ids: &mut BTreeMap<usize, BTreeSet<i32>>|
     -> Option<LineAction> {
        let secs = groups.get(&lotag)?;
        let kind = group_kind(lotag)?;
        let tag = group_tag(lotag, kind);
        let mut auto = false;
        for &s in secs {
            if mover(map.sectors[s].lotag) == Some(kind) {
                more_ids.entry(s).or_default().insert(tag);
                auto |= auto_close.contains(&s);
            }
        }
        let (special, args) = operate(kind, tag, auto);
        Some(LineAction {
            special,
            args,
            trigger,
            repeat,
            lock,
            impact: false,
        })
    };

    // C-9 charges: one blowing (its death special) runs its hitag's group
    // once, which moves the effectors with that hitag and lights the fuses of
    // the other charges (Raze `detonate` / `SEENINE` chain).
    let mut charge_groups: BTreeMap<i16, Vec<usize>> = BTreeMap::new();
    for (k, sp) in map.sprites.iter().enumerate() {
        if game.charges.contains(&i32::from(sp.picnum)) && sec_of(sp).is_some() {
            charge_groups.entry(sp.hitag).or_default().push(k);
        }
    }
    let mut group_ops: BTreeMap<i16, Vec<Op>> = BTreeMap::new();
    let mut thing_specials: BTreeMap<usize, (i32, i32, [i32; 5])> = BTreeMap::new();
    for (g, (&hitag, members)) in charge_groups.iter().enumerate() {
        let g = i32::try_from(g).unwrap_or(i32::MAX);
        let tid = CHARGE_TID + g.min(BREAK_TID - CHARGE_TID - 1);
        let mut ops = fx.detonate.get(&hitag).cloned().unwrap_or_default();
        ops.push(Op::ActorState {
            tid,
            state: "Fuse".into(),
        });
        let ops = if g < MAP_VARS {
            vec![Op::Once { var: g, ops }]
        } else {
            ops
        };
        let script = push_script(&mut scripts, ops.clone());
        group_ops.insert(hitag, ops);
        for &k in members {
            // Duke counts lotag down by 3 a tic: to 0 for a visible charge
            // (then its blinking death), to -99 for a hidden one.
            let sp = &map.sprites[k];
            let hidden = sp.xrepeat <= crate::duke::HIDDEN_CHARGE_XREPEAT;
            let count = i32::from(sp.lotag).max(0) + if hidden { 99 } else { 0 };
            let fuse = (count / 3 * 35 / 30).max(1);
            thing_specials.insert(k, (tid, ACS_EXECUTE, [script, 0, fuse, 0, 0]));
        }
    }
    // `MASTERSWITCH`: the switch sharing its lotag sets off, after its hitag
    // delay, the charges and the other effectors in its sector (Raze
    // `operatesectors` / `activatebysector`).
    let mut masters: BTreeMap<i16, Vec<(i32, usize, Vec<i16>)>> = BTreeMap::new();
    for sp in &map.sprites {
        if i32::from(sp.picnum) != game.master_switch || sp.lotag == 0 {
            continue;
        }
        let Some(s) = sec_of(sp) else { continue };
        let delay = i32::from(sp.hitag).max(0) * 35 / 30;
        let mut hitags: Vec<i16> = map
            .sprites
            .iter()
            .filter(|c| game.charges.contains(&i32::from(c.picnum)) && sec_of(c) == Some(s))
            .map(|c| c.hitag)
            .collect();
        hitags.sort_unstable();
        hitags.dedup();
        masters
            .entry(sp.lotag)
            .or_default()
            .push((delay, s, hitags));
    }
    let inline = |special: i32, args: [i32; 5], scripts: &[AcsScript]| -> Vec<Op> {
        match scripts
            .iter()
            .find(|s| special == ACS_EXECUTE && s.0 == args[0])
        {
            Some(s) => s.1.clone(),
            None => vec![Op::Special(special, args)],
        }
    };
    let mut master_scripts: BTreeMap<i16, i32> = BTreeMap::new();
    for (&lotag, list) in &mut masters {
        list.sort_by_key(|(delay, s, _)| (*delay, *s));
        let mut ops = group_action(lotag, Trigger::Use, true, 0, &mut more_ids)
            .map(|a| inline(a.special, a.args, &scripts))
            .unwrap_or_default();
        let mut waited = 0;
        for (delay, s, hitags) in list.iter() {
            if *delay > waited {
                ops.push(Op::Delay(*delay - waited));
                waited = *delay;
            }
            if let Some(m) = mover(map.sectors[*s].lotag) {
                let tag = sector_tag(*s);
                more_ids.entry(*s).or_default().insert(tag);
                let (special, args) = operate(m, tag, auto_close.contains(s));
                ops.extend(inline(special, args, &scripts));
            }
            ops.extend(fx.sector_ops.get(s).into_iter().flatten().cloned());
            for hitag in hitags {
                ops.extend(group_ops.get(hitag).into_iter().flatten().cloned());
            }
        }
        master_scripts.insert(lotag, push_script(&mut scripts, ops));
    }
    let switch_action = |lotag: i16,
                         lock: i32,
                         trigger: Trigger,
                         repeat: bool,
                         more_ids: &mut BTreeMap<usize, BTreeSet<i32>>| {
        match master_scripts.get(&lotag) {
            Some(&script) => Some(LineAction {
                special: ACS_EXECUTE,
                args: [script, 0, 0, 0, 0],
                trigger,
                repeat,
                lock,
                impact: false,
            }),
            None => group_action(lotag, trigger, repeat, lock, more_ids),
        }
    };
    // A fan blown off its `FANSHADOW` floor leaves `FANSHADOWBROKE`.
    let [fan, fan_shadow, fan_broken] = game.fan;
    for (k, sp) in map.sprites.iter().enumerate() {
        let Some(s) = sec_of(sp) else { continue };
        if i32::from(sp.picnum) != fan
            || i32::from(map.sectors[s].floorpicnum) != fan_shadow
            || tiles.get(fan_broken).is_none()
            || thing_specials.contains_key(&k)
        {
            continue;
        }
        let tag = sector_tag(s);
        more_ids.entry(s).or_default().insert(tag);
        out.tiles.insert(fan_broken);
        let script = push_script(
            &mut scripts,
            vec![Op::Flat {
                tag,
                texture: tile_name(fan_broken),
                ceiling: false,
            }],
        );
        thing_specials.insert(k, (0, ACS_EXECUTE, [script, 0, 0, 0, 0]));
    }

    let mut actions: BTreeMap<usize, LineAction> = BTreeMap::new();
    let lock_of = |pic: i32, pal: u8| {
        if game.access_switches.contains(&pic) {
            crate::duke::lock_for_pal(pal)
        } else {
            0
        }
    };
    // Switches on walls (picture or masked picture).
    for (i, w) in map.walls.iter().enumerate() {
        let pic = [w.picnum, w.overpicnum]
            .into_iter()
            .map(i32::from)
            .find(|p| game.switches.contains(p) || game.switches.contains(&(p - 1)));
        if let Some(pic) = pic.filter(|_| w.lotag != 0) {
            let base = if game.switches.contains(&pic) {
                pic
            } else {
                pic - 1
            };
            let lock = lock_of(base, w.pal);
            if let Some(a) = switch_action(w.lotag, lock, Trigger::Use, true, &mut more_ids) {
                actions.insert(i, a);
            }
        }
    }
    // Switch sprites, the nuke button and touchplates.
    let nearest_wall = |sp: &Sprite| -> Option<usize> {
        let mut best: Option<(f64, usize)> = None;
        for (i, w) in map.walls.iter().enumerate() {
            if sec_of(sp) != wsec[i] {
                continue;
            }
            let Some(w2) = usize::try_from(w.point2)
                .ok()
                .and_then(|p| map.walls.get(p))
            else {
                continue;
            };
            let d = seg_dist2([sp.x, sp.y], [w.x, w.y], [w2.x, w2.y]);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, i));
            }
        }
        best.map(|(_, i)| i)
    };
    for sp in &map.sprites {
        let pic = i32::from(sp.picnum);
        if pic == game.nukebutton {
            if let Some(i) = nearest_wall(sp) {
                actions.insert(
                    i,
                    LineAction {
                        special: EXIT_NORMAL,
                        args: [0; 5],
                        trigger: Trigger::Use,
                        repeat: false,
                        lock: 0,
                        impact: false,
                    },
                );
            }
        } else if sp.lotag != 0
            && (game.switches.contains(&pic) || game.switches.contains(&(pic - 1)))
        {
            let base = if game.switches.contains(&pic) {
                pic
            } else {
                pic - 1
            };
            let lock = lock_of(base, sp.pal);
            if let (Some(i), Some(a)) = (
                nearest_wall(sp),
                switch_action(sp.lotag, lock, Trigger::Use, true, &mut more_ids),
            ) {
                actions.entry(i).or_insert(a);
            }
        } else if pic == game.touchplate && sp.lotag != 0 {
            let Some(s) = sec_of(sp) else { continue };
            if let Some(a) = switch_action(sp.lotag, 0, Trigger::Cross, false, &mut more_ids) {
                for i in sector_walls(s) {
                    if map.walls[i].nextwall >= 0 {
                        actions.entry(i).or_insert(a);
                    }
                }
            }
        }
    }
    // Transporters: each pad sends to the next pad with the same hitag.
    let mut teleport_dests: Vec<(i32, &Sprite)> = Vec::new();
    let mut teleport_sectors = BTreeSet::new();
    for pads in transporters.values().filter(|p| p.len() >= 2) {
        for (k, from) in pads
            .iter()
            .enumerate()
            .filter(|(_, p)| p.lotag == SE_TRANSPORTER)
        {
            let to = pads[(k + 1) % pads.len()];
            let (Some(s), Some(st)) = (sec_of(from), sec_of(to)) else {
                continue;
            };
            // Floating above its floor: a fall-through link (Duke `onfloorz`
            // is 0), not a pad you walk onto.
            if from.z != map.sectors[s].floorz {
                let tag = sector_tag(st);
                more_ids.entry(st).or_default().insert(tag);
                let h = |sp: &Sprite| {
                    (-f64::from(sp.z) / Z_SCALE)
                        .round()
                        .clamp(-32768.0, 32767.0) as i16
                };
                let dx = (f64::from(to.x - from.x) / XY_SCALE).round() as i32;
                let dy = (-f64::from(to.y - from.y) / XY_SCALE).round() as i32;
                let heights = pack_fall_heights(h(from), h(to));
                water_links
                    .entry(s)
                    .or_insert([tag, FALL_PORTAL, heights, dx, dy]);
                continue;
            }
            let tid = TELEPORT_TID + i32::try_from(teleport_dests.len()).unwrap_or(0);
            teleport_dests.push((tid, to));
            teleport_sectors.insert(s);
            for i in sector_walls(s) {
                if map.walls[i].nextwall >= 0 {
                    actions.insert(
                        i,
                        LineAction {
                            special: TELEPORT,
                            args: [tid, 0, 0, 0, 0],
                            trigger: Trigger::Cross,
                            repeat: true,
                            lock: 0,
                            impact: false,
                        },
                    );
                }
            }
        }
    }
    // End-of-level sectors exit when entered.
    for (s, sec) in map.sectors.iter().enumerate() {
        if sec.lotag == LT_END_LEVEL || sec.lotag == LT_END_MESSAGE {
            for i in sector_walls(s) {
                if map.walls[i].nextwall >= 0 {
                    actions.entry(i).or_insert(LineAction {
                        special: EXIT_NORMAL,
                        args: [0; 5],
                        trigger: Trigger::Cross,
                        repeat: false,
                        lock: 0,
                        impact: false,
                    });
                }
            }
        }
    }
    // Teleport lines are crossed from their front: put the pad behind them.
    // Sector portals are written on a line the portal sector is in front of.
    let portal_secs: BTreeSet<usize> = water_links.keys().copied().collect();
    let wall_first = |a: usize, b: usize| -> bool {
        let tele = |w: usize| wsec[w].is_some_and(|s| teleport_sectors.contains(&s));
        let portal = |w: usize| wsec[w].is_some_and(|s| portal_secs.contains(&s));
        match (tele(a), tele(b), portal(a), portal(b)) {
            (false, true, ..) => true,
            (true, false, ..) => false,
            (.., true, false) => true,
            (.., false, true) => false,
            _ => a < b,
        }
    };

    for &(s, tag) in &fx.tags {
        more_ids.entry(s).or_default().insert(tag);
    }
    for line in &fx.lines {
        actions.entry(line.wall).or_insert(LineAction {
            special: line.special,
            args: line.args,
            trigger: if line.cross {
                Trigger::Cross
            } else {
                Trigger::Use
            },
            repeat: line.repeat,
            lock: 0,
            impact: line.impact,
        });
    }

    let mut t = String::with_capacity(n * 400);
    t.push_str("namespace = \"zdoom\";\n");

    // Vertices, merged by exact position.
    let mut vids: BTreeMap<(i32, i32), usize> = BTreeMap::new();
    let mut wall_v = vec![usize::MAX; n];
    for (i, w) in map.walls.iter().enumerate() {
        let next = vids.len();
        let id = *vids.entry((w.x, w.y)).or_insert(next);
        if id == next {
            let (x, y) = pos(w.x, w.y);
            let _ = writeln!(t, "vertex {{ x = {}; y = {}; }}", fmt_f(x), fmt_f(y));
        }
        wall_v[i] = id;
    }

    let partner_of = |i: usize| {
        usize::try_from(map.walls[i].nextwall).ok().filter(|&j| {
            j < n && wsec[j].is_some() && i16::try_from(i).ok() == Some(map.walls[j].nextwall)
        })
    };
    let (boundary, cuts) = boundary_overlaps(map, &wsec, &partner_of, &wall_first);
    let mut plan = crate::polys::plan(map, &wsec, game, &partner_of);
    if !fx.open.is_empty() {
        if let Some(script) = plan.scripts.iter_mut().find(|s| s.0 == 1001) {
            script.1.extend(fx.open.iter().copied());
        } else {
            plan.scripts.push((1001, fx.open.clone()));
        }
    }
    // Static sector of a wall: slabs that became polyobjects merge into their host.
    let ssec = |w: usize| wsec[w].map(|s| plan.merged.get(&s).copied().unwrap_or(s));
    let door_use = |s: usize| {
        plan.door_scripts.get(&s).map(|&script| LineAction {
            special: ACS_EXECUTE,
            args: [script, 0, 0, 0, 0],
            trigger: Trigger::UseOrCross,
            repeat: true,
            lock: 0,
            impact: false,
        })
    };

    let mut break_spots: Vec<(i32, i32, (f64, f64), f64)> = Vec::new();
    let mut sides = String::with_capacity(n * 300);
    let mut side_count = 0usize;
    let mut side = |wi: usize, s: usize, two: Option<usize>, len: f64, used: &mut ConvertedMap| {
        let wl = &map.walls[wi];
        let xr = f64::from(wl.xrepeat.max(1)) * 8.0;
        let sx = len / xr;
        let sy = 8.0 / f64::from(wl.yrepeat.max(1));
        let (_, th) = tile_size(wl.picnum);
        let ox = (f64::from(wl.xpanning) * sx).round();
        let oy = (f64::from(wl.ypanning) * th / 256.0 * sy).round();
        let (top, mid, bot) = match two {
            None => ("-".to_owned(), tex(wl.picnum, used), "-".to_owned()),
            Some(j) => {
                let mid = if wl.cstat & (16 | 32) != 0 {
                    tex(wl.overpicnum, used)
                } else {
                    "-".to_owned()
                };
                let low = if wl.cstat & 2 != 0 {
                    map.walls[j].picnum
                } else {
                    wl.picnum
                };
                (tex(wl.picnum, used), mid, tex(low, used))
            }
        };
        let _ = writeln!(
            sides,
            "sidedef {{ sector = {s}; texturetop = \"{top}\"; texturemiddle = \"{mid}\"; \
             texturebottom = \"{bot}\"; offsetx = {ox}; offsety = {oy}; \
             scalex_top = {sx}; scaley_top = {sy}; scalex_mid = {sx}; scaley_mid = {sy}; \
             scalex_bottom = {sx}; scaley_bottom = {sy}; light = {}; lightabsolute = true; }}",
            shade_light(wl.shade),
            sx = fmt_f(sx),
            sy = fmt_f(sy),
        );
        side_count += 1;
        side_count - 1
    };
    for i in 0..n {
        let Some(sec) = ssec(i) else { continue };
        let partner = partner_of(i);
        if partner.is_some_and(|j| wall_first(j, i)) || plan.panel_walls.contains(&i) {
            continue;
        }
        // Walls between a merged slab and its host vanish.
        let merged = |w: usize| wsec[w].is_some_and(|s| plan.merged.contains_key(&s));
        if partner.is_some_and(|j| ssec(j) == Some(sec) && (merged(i) || merged(j))) {
            continue;
        }
        // An inner loop lying on its sector's own outer wall: one-sided,
        // facing the inner sector.
        // A mirror wall becomes a one-sided `Line_Mirror` facing its room
        // (the sector behind only served Build's mirror trick).
        let is_mirror = |k: usize| {
            let w = &map.walls[k];
            w.cstat & 32 != 0 && i32::from(w.overpicnum) == game.mirror
        };
        let (i, sec, partner, mirror) = match (boundary.get(&i), partner) {
            // A portal whose floor or ceiling differs from the wall it lies on
            // is a door or window. Keep it two-sided. A coplanar strip (same
            // heights) stays one-sided so the node builder has no overlap.
            (Some(&h), _) => {
                let host = ssec(h).unwrap_or(sec);
                let open = partner_of(h).filter(|&j| {
                    let other = ssec(j).unwrap_or(host);
                    other != host && sector_opens(map, host, other)
                });
                (h, host, open, false)
            }
            (None, Some(j)) if is_mirror(i) || is_mirror(j) => {
                let m = if is_mirror(i) { i } else { j };
                (m, ssec(m).unwrap_or(sec), None, true)
            }
            (None, _) => (i, sec, partner, false),
        };
        let w = &map.walls[i];
        let Some(p2) = usize::try_from(w.point2).ok().filter(|&p| p < n) else {
            continue;
        };
        if wall_v[i] == wall_v[p2] {
            continue;
        }
        let pieces: Vec<(usize, usize)> = match cuts.get(&i) {
            Some(spans) => uncut_pieces(map, i, p2, spans)
                .into_iter()
                .filter_map(|(a, b)| Some((*vids.get(&a)?, *vids.get(&b)?)))
                .filter(|(a, b)| a != b)
                .collect(),
            None => vec![(wall_v[i], wall_v[p2])],
        };
        let (ax, ay) = pos(w.x, w.y);
        let (bx, by) = pos(map.walls[p2].x, map.walls[p2].y);
        let len = (bx - ax).hypot(by - ay);

        for (v1, v2) in pieces {
            let front = side(i, sec, partner, len, &mut out);
            let back = partner.map(|j| side(j, ssec(j).unwrap_or(sec), Some(i), len, &mut out));

            let _ = write!(t, "linedef {{ v1 = {v1}; v2 = {v2}; sidefront = {front};");
            if let Some(b) = back {
                let _ = write!(t, " sideback = {b}; twosided = true;");
            }
            if back.is_none() || w.cstat & 1 != 0 {
                t.push_str(" blocking = true;");
            }
            if w.cstat & 4 != 0 {
                t.push_str(" dontpegbottom = true;");
                if back.is_some() {
                    t.push_str(" dontpegtop = true;");
                }
            }
            if back.is_some() && w.cstat & 16 != 0 {
                t.push_str(" wrapmidtex = true;");
                if w.cstat & 128 != 0 {
                    let alpha = if w.cstat & 512 != 0 { "0.33" } else { "0.66" };
                    let _ = write!(t, " alpha = {alpha}; renderstyle = \"translucent\";");
                }
            }
            // Wired actions first, then direct use of a door / lift sector line.
            let usable = |s: usize| {
                mover(map.sectors[s].lotag)
                    .filter(|_| !locked.contains(&s))
                    .map(|m| (m, s))
            };
            let action = actions
                .get(&i)
                .or_else(|| partner.and_then(|j| actions.get(&j)))
                .copied()
                .or_else(|| partner.and_then(|j| wsec[j]).and_then(door_use))
                .or_else(|| wsec[i].filter(|_| back.is_some()).and_then(door_use))
                .or_else(|| {
                    let (m, s) = partner
                        .and_then(|j| wsec[j])
                        .and_then(usable)
                        .or_else(|| usable(sec).filter(|_| back.is_some()))?;
                    let (special, args) = operate(m, sector_tag(s), auto_close.contains(&s));
                    Some(LineAction {
                        special,
                        args,
                        trigger: Trigger::Use,
                        repeat: true,
                        lock: 0,
                        impact: false,
                    })
                });
            let broken = if mirror {
                None
            } else {
                let line = BREAK_LINE + i32::try_from(break_spots.len()).unwrap_or(0);
                wall_break(map, &mut *sprites, i, partner, line, &mut out)
                    .filter(|(_, b, _)| action.is_none() || b.chain)
            };
            let shattered = broken.is_some();
            if let Some((ops, b, from_back)) = broken {
                let line = BREAK_LINE + i32::try_from(break_spots.len()).unwrap_or(0);
                let tid = BREAK_TID + i32::try_from(break_spots.len()).unwrap_or(0);
                let mut ops = ops;
                if b.unblock {
                    ops.push(Op::Special(
                        LINE_SET_BLOCKING,
                        [line, 0, BLOCK_CREATURES_HITSCAN, 0, 0],
                    ));
                }
                ops.push(Op::ActorState {
                    tid,
                    state: "Active".into(),
                });
                let lotag = {
                    let face = if from_back { partner.unwrap_or(i) } else { i };
                    let a = map.walls[face].lotag;
                    let b = partner.map(|j| map.walls[j].lotag).unwrap_or(0);
                    if a != 0 { a } else { b }
                };
                if b.light && lotag != 0 {
                    ops.extend(fx.shot_lights.get(&lotag).into_iter().flatten().cloned());
                }
                let script = push_script(&mut scripts, ops);
                let _ = write!(
                    t,
                    " moreids = \"{line}\"; health = 1; deathspecial = true; special = {ACS_EXECUTE}; \
                     arg0 = {script};"
                );
                if back.is_some() && b.unblock {
                    t.push_str(" blockhitscan = true;");
                }
                if b.chain {
                    let face = if from_back {
                        partner.and_then(|j| wsec[j]).unwrap_or(sec)
                    } else {
                        wsec[i].unwrap_or(sec)
                    };
                    let tag = if lotag != 0 {
                        Some(lotag)
                    } else {
                        fx.glass_sectors
                            .iter()
                            .find(|(_, secs)| secs.contains(&face))
                            .map(|(&h, _)| h)
                    };
                    if let Some(tag) = tag {
                        let _ = write!(t, " healthgroup = {};", GLASS_GROUP + i32::from(tag));
                    }
                }
                // The debris spot stands just off the broken face, halfway up.
                let (dx, dy) = ((bx - ax) / len.max(1e-9), (by - ay) / len.max(1e-9));
                let side_sign = if from_back { -1.0 } else { 1.0 };
                let (mx, my) = ((ax + bx) / 2.0, (ay + by) / 2.0);
                let at = (mx + dy * 4.0 * side_sign, my - dx * 4.0 * side_sign);
                let face = if from_back {
                    partner.and_then(&ssec).unwrap_or(sec)
                } else {
                    sec
                };
                let fs = &map.sectors[face];
                let half = f64::from(fs.floorz - fs.ceilingz) / Z_SCALE / 2.0;
                break_spots.push((tid, b.debris, at, half.max(0.0)));
            }
            if mirror {
                let _ = write!(t, " special = {LINE_MIRROR};");
            } else if let Some(a) = action.filter(|_| !shattered) {
                a.write(&mut t);
            } else if let Some(args) = water_links.remove(&sec) {
                let _ = write!(t, " special = {SECTOR_SET_PORTAL};");
                for (k, a) in args.iter().enumerate() {
                    let _ = write!(t, " arg{k} = {a};");
                }
            }
            let force_pics = [
                i32::from(w.picnum),
                i32::from(w.overpicnum),
                partner.map(|j| i32::from(map.walls[j].picnum)).unwrap_or(0),
                partner
                    .map(|j| i32::from(map.walls[j].overpicnum))
                    .unwrap_or(0),
            ];
            if force_pics.iter().any(|p| game.force_walls.contains(p)) {
                t.push_str(
                    " user_contactdamage = 8; blockhitscan = true; blockprojectiles = true;",
                );
            }
            t.push_str(" }\n");
        }
    }
    // Where a sliding panel left the static map, a one-sided wall closes it.
    for &(from, to, wi) in &plan.closers {
        let (Some(&v1), Some(&v2), Some(s)) = (vids.get(&from), vids.get(&to), wsec[wi]) else {
            continue;
        };
        let len = f64::from(to.0 - from.0).hypot(f64::from(to.1 - from.1)) / XY_SCALE;
        let front = side(wi, s, None, len, &mut out);
        let _ = writeln!(
            t,
            "linedef {{ v1 = {v1}; v2 = {v2}; sidefront = {front}; blocking = true; }}"
        );
    }
    // Polyobjects, drawn in a box east of the map and moved into place by
    // their anchor → spawn spot.
    let mut next_vertex = vids.len();
    let mut cursor = map.walls.iter().map(|w| w.x).max().unwrap_or(0) + 4096;
    let mut anchors: Vec<(i32, Pt, Pt)> = Vec::new();
    for p in &plan.polys {
        let ends = |pw: &crate::polys::PolyWall| -> Option<(Pt, Pt)> {
            match *pw {
                crate::polys::PolyWall::Wall(w) => {
                    let p2 = usize::try_from(map.walls[w].point2)
                        .ok()
                        .filter(|&q| q < n)?;
                    Some((
                        (map.walls[w].x, map.walls[w].y),
                        (map.walls[p2].x, map.walls[p2].y),
                    ))
                }
                crate::polys::PolyWall::Closer { from, to, .. } => Some((from, to)),
            }
        };
        let segs: Vec<(Pt, Pt)> = p.walls.iter().filter_map(ends).collect();
        if segs.len() != p.walls.len() {
            continue;
        }
        let min_x = segs.iter().map(|s| s.0.0).min().unwrap_or(0);
        let max_x = segs.iter().map(|s| s.0.0).max().unwrap_or(0);
        let shift = cursor - min_x;
        cursor += max_x - min_x + 2048;
        let mut pverts: BTreeMap<Pt, usize> = BTreeMap::new();
        for &(a, _) in &segs {
            pverts.entry(a).or_insert_with(|| {
                let (x, y) = pos(a.0 + shift, a.1);
                let _ = writeln!(t, "vertex {{ x = {}; y = {}; }}", fmt_f(x), fmt_f(y));
                next_vertex += 1;
                next_vertex - 1
            });
        }
        for (k, (pw, &(a, b))) in p.walls.iter().zip(&segs).enumerate() {
            let (Some(&v1), Some(&v2)) = (pverts.get(&a), pverts.get(&b)) else {
                continue;
            };
            let len = f64::from(b.0 - a.0).hypot(f64::from(b.1 - a.1)) / XY_SCALE;
            let (front, back) = match *pw {
                crate::polys::PolyWall::Wall(w) => {
                    let partner = partner_of(w);
                    let front = side(w, p.sector, partner, len, &mut out);
                    let back = partner
                        .map(|j| side(j, ssec(j).unwrap_or(p.sector), Some(w), len, &mut out));
                    (front, back)
                }
                crate::polys::PolyWall::Closer { tex, .. } => {
                    (side(tex, p.sector, None, len, &mut out), None)
                }
            };
            let _ = write!(t, "linedef {{ v1 = {v1}; v2 = {v2}; sidefront = {front};");
            match back {
                Some(b) => {
                    let _ = write!(t, " sideback = {b}; twosided = true;");
                }
                None => t.push_str(" blocking = true;"),
            }
            if k == 0 {
                let _ = write!(t, " special = {POLYOBJ_START_LINE}; arg0 = {};", p.id);
            } else if let Some(script) = p.script {
                LineAction {
                    special: ACS_EXECUTE,
                    args: [script, 0, 0, 0, 0],
                    trigger: Trigger::UseOrCross,
                    repeat: true,
                    lock: 0,
                    impact: false,
                }
                .write(&mut t);
            }
            t.push_str(" }\n");
        }
        anchors.push((p.id, p.spawn, (p.spawn.0 + shift, p.spawn.1)));
        out.polyobjects += 1;
    }
    t.push_str(&sides);

    let mut sky_votes: BTreeMap<i32, usize> = BTreeMap::new();
    for (si, s) in map.sectors.iter().enumerate() {
        let plane = |z: i32, heinum: i16, ceiling: bool| -> Option<Plane> {
            let w0 = usize::try_from(s.wallptr)
                .ok()
                .and_then(|i| map.walls.get(i))?;
            let w1 = usize::try_from(w0.point2)
                .ok()
                .and_then(|i| map.walls.get(i))?;
            let (wx, wy) = pos(w0.x, w0.y);
            let (x1, y1) = pos(w1.x, w1.y);
            let (dx, dy) = (x1 - wx, y1 - wy);
            let l = dx.hypot(dy);
            if l < 1e-9 {
                return None;
            }
            let k = f64::from(heinum) / (4096.0 * l);
            let f = -f64::from(z) / Z_SCALE;
            // z = f + k * (dx * (y - wy) - dy * (x - wx))
            let (mut a, mut b, mut c, mut d) = (k * dy, -k * dx, 1.0, k * (dx * wy - dy * wx) - f);
            let norm = (a * a + b * b + c * c).sqrt();
            a /= norm;
            b /= norm;
            c /= norm;
            d /= norm;
            if ceiling {
                (a, b, c, d) = (-a, -b, -c, -d);
            }
            Some(Plane { a, b, c, d })
        };
        let h = |z: i32| (-f64::from(z) / Z_SCALE).round().clamp(-32768.0, 32767.0);
        let floor_tex = if s.floorstat & 1 != 0 {
            "F_SKY1".to_owned()
        } else {
            tex(s.floorpicnum, &mut out)
        };
        let ceil_tex = if s.ceilingstat & 1 != 0 {
            *sky_votes.entry(i32::from(s.ceilingpicnum)).or_default() += 1;
            "F_SKY1".to_owned()
        } else {
            tex(s.ceilingpicnum, &mut out)
        };
        let first_wall = usize::try_from(s.wallptr)
            .ok()
            .and_then(|i| map.walls.get(i))
            .map(|w0| {
                let p1 = usize::try_from(w0.point2)
                    .ok()
                    .and_then(|i| map.walls.get(i))
                    .map_or((w0.x, w0.y), |w1| (w1.x, w1.y));
                (pos(w0.x, w0.y), pos(p1.0, p1.1))
            })
            .unwrap_or(((0.0, 0.0), (0.0, 0.0)));
        let floor_h = h(s.floorz);
        let mut ceil_h = h(s.ceilingz);
        // Lotag-27 stretch bridges often start paper-thin. Keep them walkable
        // (always extended); `user_stretch` slides the walls when activated.
        if s.lotag == LT_BRIDGE && ceil_h <= floor_h + 1.0 {
            ceil_h = floor_h + 64.0;
        }
        let _ = write!(
            t,
            "sector {{ heightfloor = {}; heightceiling = {}; texturefloor = \"{floor_tex}\"; \
             textureceiling = \"{ceil_tex}\"; lightlevel = {};",
            fmt_f(floor_h),
            fmt_f(ceil_h),
            {
                let mut light = shade_light(s.floorshade);
                // Build sector visibility is a fog multiplier. 0 keeps the shade.
                if s.visibility >= 8 {
                    light = (light * 128 / (128 + i32::from(s.visibility))).clamp(32, 255);
                }
                light
            },
        );
        if s.visibility > 0 {
            let fog = (i32::from(s.visibility) * 4).clamp(1, 255);
            let _ = write!(t, " fogdensity = {fog};");
        }
        for (part, stat, pan, heinum, pic) in [
            (
                "floor",
                s.floorstat,
                (s.floorxpanning, s.floorypanning),
                s.floorheinum,
                s.floorpicnum,
            ),
            (
                "ceiling",
                s.ceilingstat,
                (s.ceilingxpanning, s.ceilingypanning),
                s.ceilingheinum,
                s.ceilingpicnum,
            ),
        ] {
            if stat & 1 != 0 {
                continue;
            }
            let a = flat_align(
                stat,
                pan,
                heinum,
                tile_size(pic),
                first_wall.0,
                first_wall.1,
            );
            for (key, v, def) in [
                ("xpanning", a.pan_u, 0.0),
                ("ypanning", a.pan_v, 0.0),
                ("rotation", a.rotation, 0.0),
                ("xscale", a.scale_u, 1.0),
                ("yscale", a.scale_v, 1.0),
            ] {
                if (v - def).abs() > 1e-9 {
                    let _ = write!(t, " {key}{part} = {};", fmt_f(v));
                }
            }
        }
        if mover(s.lotag).is_some() {
            let _ = write!(t, " id = {};", sector_tag(si));
            let sfx = map
                .sprites
                .iter()
                .find(|sp| {
                    i32::from(sp.picnum) == game.music_and_sfx
                        && usize::try_from(sp.sectnum).ok() == Some(si)
                        && (1..1000).contains(&sp.lotag)
                })
                .map(|sp| (sp.lotag, sp.hitag));
            match sprites.mover_sound(sfx) {
                MoverSound::Default => {}
                MoverSound::Silent => t.push_str(" silent = true;"),
                MoverSound::Sequence(name) => {
                    let _ = write!(t, " soundsequence = \"{name}\";");
                }
            }
        }
        if let Some(ids) = more_ids.get(&si) {
            let ids: Vec<String> = ids.iter().map(i32::to_string).collect();
            let _ = write!(t, " moreids = \"{}\";", ids.join(" "));
        }
        if s.lotag == LT_BRIDGE {
            t.push_str(" user_bridge = 1;");
        }
        if let Some(spin) = fx.spins.iter().find(|p| p.sector == si) {
            let _ = write!(
                t,
                " user_spin = {}; user_spinx = {}; user_spiny = {};",
                spin.speed, spin.x, spin.y
            );
        }
        if let Some(sub) = fx.subways.iter().find(|p| p.sector == si) {
            let _ = write!(
                t,
                " user_subway = 1; user_subspeed = {}; user_subx = {}; user_suby = {}; user_subn = {};",
                sub.speed,
                sub.x,
                sub.y,
                sub.points.len()
            );
            for (i, &(x, y)) in sub.points.iter().enumerate() {
                let _ = write!(t, " user_sx{i} = {x}; user_sy{i} = {y};");
            }
            if let Some(p) = plan.polys.iter().find(|p| p.sector == si) {
                let _ = write!(t, " user_subpo = {};", p.id);
            }
        }
        if let Some(st) = fx.stretches.iter().find(|p| p.sector == si) {
            let _ = write!(
                t,
                " user_stretch = 1; user_stdx = {}; user_stdy = {}; user_stdist = {}; user_stspd = {};",
                st.dx, st.dy, st.dist, st.speed
            );
        }
        if let Some(sk) = fx.skews.iter().find(|p| p.sector == si) {
            let _ = write!(
                t,
                " user_skew = 1; user_skdx = {}; user_skdy = {}; user_skdist = {}; user_skspd = {};",
                sk.dx, sk.dy, sk.dist, sk.speed
            );
        }
        if let Some(th) = fx.teeth.iter().find(|p| p.sector == si) {
            let door = plan.merged.get(&th.door).copied().unwrap_or(th.door);
            let _ = write!(
                t,
                " user_teeth = 1; user_thdx = {}; user_thdy = {}; user_thdist = {}; user_thspd = {}; user_thdoor = {};",
                th.dx, th.dy, th.dist, th.speed, door
            );
        }
        if let Some(dl) = fx.door_lights.get(&si) {
            let _ = write!(
                t,
                " user_doorlight = {}; user_doorbright = {}; user_doorlightopen = {};",
                dl.tag,
                dl.bright,
                i32::from(dl.on_open)
            );
        }
        if s.lotag == LT_SECRET {
            let _ = write!(t, " special = {ST_SECRET};");
        } else if let Some(&sp) = fx.specials.get(&si) {
            let _ = write!(t, " special = {sp};");
        } else if flicker.contains(&si) {
            let _ = write!(t, " special = {ST_FLICKER};");
        }
        if s.lotag == LT_UNDER_WATER {
            t.push_str(" waterzone = true;");
        }
        if s.floorstat & 1 == 0 {
            if let Some(&dmg) = game.hurt_floors.get(&i32::from(s.floorpicnum)) {
                let _ = write!(t, " damageamount = {dmg}; damageinterval = 32;");
            }
        }
        for (prefix, stat, z, heinum, ceiling) in [
            ("floorplane", s.floorstat, s.floorz, s.floorheinum, false),
            (
                "ceilingplane",
                s.ceilingstat,
                s.ceilingz,
                s.ceilingheinum,
                true,
            ),
        ] {
            if stat & 2 == 0 || heinum == 0 {
                continue;
            }
            if let Some(p) = plane(z, heinum, ceiling) {
                let _ = write!(
                    t,
                    " {prefix}_a = {}; {prefix}_b = {}; {prefix}_c = {}; {prefix}_d = {};",
                    fmt_f(p.a),
                    fmt_f(p.b),
                    fmt_f(p.c),
                    fmt_f(p.d)
                );
            }
        }
        t.push_str(" }\n");
    }
    out.sky = sky_votes
        .iter()
        .max_by_key(|&(pic, count)| (*count, -*pic))
        .map(|(&pic, _)| pic);
    if let Some(sky) = out.sky {
        if tiles.get(sky).is_some() {
            out.tiles.insert(sky);
        }
    }

    // Player 1 at the map start, then APLAYER sprites as coop / deathmatch starts.
    let angle = |ang: i16| (-(i32::from(ang) & 2047) * 360 / 2048).rem_euclid(360);
    let thing = |t: &mut String, ty: i32, x: i32, y: i32, ang: i16, z: Option<i32>| {
        let (x, y) = pos(x, y);
        let _ = writeln!(
            t,
            "thing {{ x = {}; y = {}; angle = {}; type = {ty};{} coop = true; dm = true; single = true; }}",
            fmt_f(x),
            fmt_f(y),
            angle(ang),
            z.map(|z| format!(" height = {};", fmt_f(-f64::from(z) / Z_SCALE - 36.0)))
                .unwrap_or_default()
        );
    };
    // Map header Z is the eye. Feet sit one view-height (36) below it.
    thing(
        &mut t,
        1,
        map.start[0],
        map.start[1],
        map.start_ang,
        Some(map.start[2]),
    );
    let mut coop = 2;
    for sp in map.sprites.iter().filter(|s| s.picnum == APLAYER) {
        thing(&mut t, 11, sp.x, sp.y, sp.ang, Some(sp.z));
        let at_start = sp.x == map.start[0] && sp.y == map.start[1];
        if !at_start && coop <= 4 {
            thing(&mut t, coop, sp.x, sp.y, sp.ang, Some(sp.z));
            coop += 1;
        }
    }
    for &(id, spawn, anchor) in &anchors {
        for (ty, (x, y)) in [(POLY_SPAWN, spawn), (POLY_ANCHOR, anchor)] {
            let (x, y) = pos(x, y);
            let _ = writeln!(
                t,
                "thing {{ x = {}; y = {}; angle = {id}; type = {ty}; coop = true; dm = true; single = true; }}",
                fmt_f(x),
                fmt_f(y),
            );
        }
    }
    let mut all_scripts: Vec<AcsScript> = plan
        .scripts
        .iter()
        .map(|(n, calls)| {
            let ops = calls
                .iter()
                .map(|&(special, args)| Op::Special(special, args))
                .collect();
            (*n, ops)
        })
        .collect();
    all_scripts.append(&mut scripts);
    if !all_scripts.is_empty() {
        out.behavior = Some(crate::acs::assemble(&all_scripts));
    }
    for &(tid, ednum, (x, y), height) in &break_spots {
        let _ = writeln!(
            t,
            "thing {{ id = {tid}; x = {}; y = {}; height = {}; type = {ednum}; \
             coop = true; dm = true; single = true; }}",
            fmt_f(x),
            fmt_f(y),
            fmt_f(height.round()),
        );
    }
    for (tid, sp) in &teleport_dests {
        let (x, y) = pos(sp.x, sp.y);
        let _ = writeln!(
            t,
            "thing {{ id = {tid}; x = {}; y = {}; angle = {}; type = {THING_TELEPORT_DEST}; \
             coop = true; dm = true; single = true; }}",
            fmt_f(x),
            fmt_f(y),
            angle(sp.ang)
        );
    }
    for &(tid, x, y, ang) in &fx.dests {
        let (x, y) = pos(x, y);
        let _ = writeln!(
            t,
            "thing {{ id = {tid}; x = {}; y = {}; angle = {}; type = {THING_TELEPORT_DEST}; \
             coop = true; dm = true; single = true; }}",
            fmt_f(x),
            fmt_f(y),
            angle(ang)
        );
    }

    // Other sprites, at their Build height above the floor and scale.
    let bottom_of = |sp: &Sprite| {
        let mut bottom = -f64::from(sp.z) / Z_SCALE;
        if sp.cstat & 128 != 0 {
            bottom -= tile_size(sp.picnum).1 * f64::from(sp.yrepeat) / 64.0 / 2.0;
        }
        bottom
    };
    let placed: Vec<(usize, &Sprite, usize, MapThing)> = map
        .sprites
        .iter()
        .enumerate()
        .filter(|(_, sp)| {
            if sp.picnum == APLAYER {
                return false;
            }
            // Hidden SE 33 / 36 (and other zero-repeat effectors we spawn)
            // have xrepeat 0.
            let se = i32::from(sp.picnum) == game.effector && matches!(sp.lotag, 5 | 27 | 33 | 36);
            se || (sp.xrepeat != 0 && sp.yrepeat != 0)
        })
        .filter_map(|(k, sp)| {
            let s = sec_of(sp)?;
            let in_mover = mover(map.sectors[s].lotag).is_some();
            Some((k, sp, s, sprites.thing(sp, in_mover)?))
        })
        .collect();
    // Solid decorations, as the boxes the Duke classes give them.
    let solids: Vec<Solid> = placed
        .iter()
        .filter(|(_, sp, _, th)| th.size.is_none() && sp.cstat & 1 != 0 && sp.cstat & 48 == 0)
        .map(|(_, sp, _, _)| {
            let (w, h) = tile_size(sp.picnum);
            Solid {
                at: pos(sp.x, sp.y),
                radius: (w / 4.0).floor().clamp(4.0, 48.0),
                bottom: bottom_of(sp),
                height: (h / 2.0).floor().clamp(8.0, 128.0),
            }
        })
        .collect();
    for &(k, sp, s, th) in &placed {
        let (sx, sy) = (f64::from(sp.xrepeat) / 64.0, f64::from(sp.yrepeat) / 64.0);
        let bottom = bottom_of(sp);
        let (mut x, mut y) = into_sector(map, &wsec, s, pos(sp.x, sp.y));
        let floor = floor_z_at(map, s, x, y);
        let height = (bottom - floor).max(0.0).round().min(32767.0);
        if let Some((radius, tall)) = th.size {
            let feet = floor + height;
            let fit = Fit {
                map,
                wsec: &wsec,
                solids: &solids,
                sector: s,
                feet,
                radius: f64::from(radius),
                height: f64::from(tall),
            };
            (x, y) = fit.nearest(x, y);
        }
        let _ = write!(
            t,
            "thing {{ x = {}; y = {}; height = {}; angle = {}; type = {}; scalex = {}; \
             scaley = {}; coop = true; dm = true; single = true;",
            fmt_f(x),
            fmt_f(y),
            fmt_f(height),
            angle(sp.ang),
            th.ednum,
            fmt_f(sx),
            fmt_f(sy)
        );
        if let Some(&(tid, special, args)) = thing_specials.get(&k) {
            if tid != 0 {
                let _ = write!(t, " id = {tid};");
            }
            let _ = write!(t, " special = {special};");
            for (a, v) in args.iter().enumerate().filter(|(_, v)| **v != 0) {
                let _ = write!(t, " arg{a} = {v};");
            }
        }
        // Room skills 1–5 play as Duke skills 1, 1, 2, 3, 4.
        for (skill, duke) in [(1, 1), (2, 1), (3, 2), (4, 3), (5, 4)] {
            if duke < th.min_skill {
                let _ = write!(t, " skill{skill} = false;");
            }
        }
        for (i, &v) in th.args.iter().enumerate() {
            if v != 0 {
                let _ = write!(t, " arg{i} = {v};");
            }
        }
        t.push_str(" }\n");
    }
    out.textmap = t;
    Ok(out)
}

use crate::polys::Pt;

/// Texture changes of a line whose pictures break when shot, the first
/// break found, and whether it is on the back side.
fn wall_break(
    map: &BuildMap,
    sprites: &mut dyn SpriteMapper,
    i: usize,
    partner: Option<usize>,
    line: i32,
    out: &mut ConvertedMap,
) -> Option<(Vec<Op>, WallBreak, bool)> {
    let mut ops = Vec::new();
    let mut found = None;
    let mut chain = false;
    for (side, w, other) in [(0, Some(i), partner), (1, partner, Some(i))] {
        let Some(w) = w else { continue };
        let wl = &map.walls[w];
        // (UDMF texture position, picture, masked)
        let parts: Vec<(i32, i16, bool)> = match other.filter(|_| partner.is_some()) {
            Some(o) => {
                let low = if wl.cstat & 2 != 0 {
                    map.walls[o].picnum
                } else {
                    wl.picnum
                };
                let mut v = vec![(0, wl.picnum, false), (2, low, false)];
                if wl.cstat & 16 != 0 || wl.overpicnum != 0 {
                    v.push((1, wl.overpicnum, true));
                }
                v
            }
            None => vec![(1, wl.picnum, false)],
        };
        for (position, pic, masked) in parts {
            let Some(b) = sprites.wall_break(i32::from(pic), masked) else {
                continue;
            };
            let texture = b.broken.map_or_else(
                || "-".to_owned(),
                |p| {
                    out.tiles.insert(p);
                    tile_name(p)
                },
            );
            ops.push(Op::LineTexture {
                line,
                side,
                position,
                texture,
            });
            chain |= b.chain;
            found.get_or_insert((b, side == 1));
        }
    }
    let (mut b, back) = found?;
    b.chain = chain;
    Some((ops, b, back))
}

/// A blocking decoration's box (Room units).
struct Solid {
    at: (f64, f64),
    radius: f64,
    bottom: f64,
    height: f64,
}

/// How far [`into_sector`] keeps sprites from their sector's walls.
const EDGE_CLEARANCE: f64 = 1.5;

fn wall_pt(map: &BuildMap, i: usize) -> Option<(f64, f64)> {
    let w = map.walls.get(i)?;
    Some((f64::from(w.x) / XY_SCALE, -f64::from(w.y) / XY_SCALE))
}

/// Floor height of Build sector `s` at `(x, y)` (Room space), slopes included.
fn floor_z_at(map: &BuildMap, s: usize, x: f64, y: f64) -> f64 {
    let Some(sec) = map.sectors.get(s) else {
        return 0.0;
    };
    let flat = -f64::from(sec.floorz) / Z_SCALE;
    if sec.floorstat & 2 == 0 || sec.floorheinum == 0 {
        return flat;
    }
    let first = usize::try_from(sec.wallptr).unwrap_or(usize::MAX);
    let Some(next) = map
        .walls
        .get(first)
        .and_then(|w| usize::try_from(w.point2).ok())
    else {
        return flat;
    };
    let (Some((wx, wy)), Some((x1, y1))) = (wall_pt(map, first), wall_pt(map, next)) else {
        return flat;
    };
    let (dx, dy) = (x1 - wx, y1 - wy);
    let l = dx.hypot(dy);
    if l < 1e-9 {
        return flat;
    }
    let k = f64::from(sec.floorheinum) / (4096.0 * l);
    flat + k * (dx * (y - wy) - dy * (x - wx))
}

/// `p`, or `p` moved [`EDGE_CLEARANCE`] inside sector `s` when it sits on,
/// near or just past one of its walls. Room rounds thing positions and BSP
/// partitions to whole units, so a point closer than that to a Build wall
/// can land on the wrong side and take the other sector's floor.
fn into_sector(map: &BuildMap, wsec: &[Option<usize>], s: usize, p: (f64, f64)) -> (f64, f64) {
    let segs: Vec<((f64, f64), (f64, f64))> = (0..map.walls.len())
        .filter(|&i| wsec.get(i).copied().flatten() == Some(s))
        .filter_map(|i| {
            let p2 = usize::try_from(map.walls[i].point2).ok()?;
            Some((wall_pt(map, i)?, wall_pt(map, p2)?))
        })
        .collect();
    let inside = |(x, y): (f64, f64)| {
        let mut odd = false;
        for &(a, b) in &segs {
            if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                odd = !odd;
            }
        }
        odd
    };
    // Signed distance to the nearest wall (positive on the sector's right side).
    let nearest = segs
        .iter()
        .filter_map(|&(a, b)| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len2 = dx * dx + dy * dy;
            if len2 < 1e-12 {
                return None;
            }
            let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0);
            let dist = (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy);
            let len = len2.sqrt();
            let n = (dy / len, -dx / len);
            let side = (p.0 - a.0) * n.0 + (p.1 - a.1) * n.1;
            Some((dist, n, side))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    let Some((dist, n, side)) = nearest else {
        return p;
    };
    if inside(p) && dist >= EDGE_CLEARANCE {
        return p;
    }
    [1.0, -1.0]
        .into_iter()
        .map(|dir: f64| {
            let shift = dir * EDGE_CLEARANCE - side;
            (p.0 + n.0 * shift, p.1 + n.1 * shift)
        })
        .find(|&q| inside(q))
        .unwrap_or(p)
}

/// Doom's view of a standing actor (Room units): its box may not touch a wall
/// it can't stand across (`P_CheckPosition`).
struct Fit<'a> {
    map: &'a BuildMap,
    wsec: &'a [Option<usize>],
    solids: &'a [Solid],
    sector: usize,
    feet: f64,
    radius: f64,
    height: f64,
}

impl Fit<'_> {
    /// Farthest a monster is moved to clear the walls around it.
    const REACH: i32 = 48;

    fn pt(&self, i: usize) -> Option<(f64, f64)> {
        let w = self.map.walls.get(i)?;
        Some((f64::from(w.x) / XY_SCALE, -f64::from(w.y) / XY_SCALE))
    }

    fn seg(&self, i: usize) -> Option<((f64, f64), (f64, f64))> {
        let p2 = usize::try_from(self.map.walls.get(i)?.point2).ok()?;
        Some((self.pt(i)?, self.pt(p2)?))
    }

    fn blocks(&self, i: usize) -> bool {
        let w = &self.map.walls[i];
        let (Some(front), Ok(back)) = (
            self.wsec.get(i).copied().flatten(),
            usize::try_from(w.nextsector),
        ) else {
            return true;
        };
        let (Some(f), Some(b)) = (self.map.sectors.get(front), self.map.sectors.get(back)) else {
            return true;
        };
        let z = |v: i32| -f64::from(v) / Z_SCALE;
        let floor = z(f.floorz).max(z(b.floorz));
        let ceil = z(f.ceilingz).min(z(b.ceilingz));
        w.cstat & 1 != 0 || floor > self.feet + 24.0 || ceil < self.feet + self.height
    }

    /// Whether the box at `(x, y)` touches segment `a`–`b` (`P_BoxOnLineSide`).
    fn touches(&self, (x, y): (f64, f64), (a, b): ((f64, f64), (f64, f64))) -> bool {
        let r = self.radius;
        if a.0.max(b.0) <= x - r
            || a.0.min(b.0) >= x + r
            || a.1.max(b.1) <= y - r
            || a.1.min(b.1) >= y + r
        {
            return false;
        }
        let side = |px: f64, py: f64| (b.0 - a.0) * (py - a.1) - (b.1 - a.1) * (px - a.0) > 0.0;
        let s = side(x - r, y - r);
        [(x + r, y - r), (x - r, y + r), (x + r, y + r)]
            .iter()
            .any(|&(px, py)| side(px, py) != s)
    }

    /// Its sector and the neighbours it can step into.
    fn rooms(&self) -> Vec<usize> {
        let Some(sec) = self.map.sectors.get(self.sector) else {
            return Vec::new();
        };
        let first = usize::try_from(sec.wallptr).unwrap_or(0);
        let n = usize::try_from(sec.wallnum).unwrap_or(0);
        let mut out = vec![self.sector];
        for w in self.map.walls.iter().skip(first).take(n) {
            let Ok(s) = usize::try_from(w.nextsector) else {
                continue;
            };
            let Some(next) = self.map.sectors.get(s) else {
                continue;
            };
            let floor = -f64::from(next.floorz) / Z_SCALE;
            let ceil = -f64::from(next.ceilingz) / Z_SCALE;
            if !out.contains(&s) && (floor - self.feet).abs() <= 24.0 && ceil - floor >= self.height
            {
                out.push(s);
            }
        }
        out
    }

    fn inside(&self, sector: usize, (x, y): (f64, f64)) -> bool {
        let Some(sec) = self.map.sectors.get(sector) else {
            return false;
        };
        let first = usize::try_from(sec.wallptr).unwrap_or(0);
        let n = usize::try_from(sec.wallnum).unwrap_or(0);
        let mut odd = false;
        for i in first..first + n {
            let Some((a, b)) = self.seg(i) else { continue };
            if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                odd = !odd;
            }
        }
        odd
    }

    /// `(x, y)` or the closest spot within [`Self::REACH`] where the box is free.
    fn nearest(&self, x: f64, y: f64) -> (f64, f64) {
        let reach = f64::from(Self::REACH) + self.radius;
        let near: Vec<_> = (0..self.map.walls.len())
            .filter_map(|i| self.seg(i).map(|s| (i, s)))
            .filter(|&(_, (a, b))| {
                a.0.max(b.0) > x - reach
                    && a.0.min(b.0) < x + reach
                    && a.1.max(b.1) > y - reach
                    && a.1.min(b.1) < y + reach
            })
            .filter(|&(i, _)| self.blocks(i))
            .map(|(_, s)| s)
            .collect();
        let solids: Vec<&Solid> = self
            .solids
            .iter()
            .filter(|o| {
                o.bottom < self.feet + self.height
                    && o.bottom + o.height > self.feet
                    && (o.at.0 - x).abs() < reach + o.radius
                    && (o.at.1 - y).abs() < reach + o.radius
            })
            .collect();
        let free = |p: (f64, f64)| {
            near.iter().all(|&s| !self.touches(p, s))
                && solids.iter().all(|o| {
                    let r = o.radius + self.radius;
                    (o.at.0 - p.0).abs() >= r || (o.at.1 - p.1).abs() >= r
                })
        };
        if free((x, y)) {
            return (x, y);
        }
        let mut offsets: Vec<(i32, i32)> = (-Self::REACH..=Self::REACH)
            .step_by(2)
            .flat_map(|dx| {
                (-Self::REACH..=Self::REACH)
                    .step_by(2)
                    .map(move |dy| (dx, dy))
            })
            .filter(|&(dx, dy)| dx * dx + dy * dy <= Self::REACH * Self::REACH)
            .collect();
        offsets.sort_by_key(|&(dx, dy)| (dx * dx + dy * dy, dx, dy));
        let rooms = self.rooms();
        offsets
            .into_iter()
            .map(|(dx, dy)| (x + f64::from(dx), y + f64::from(dy)))
            .find(|&p| rooms.iter().any(|&s| self.inside(s, p)) && free(p))
            .unwrap_or((x, y))
    }
}

/// The two sectors do not share both planes, so the wall between them is an opening.
fn sector_opens(map: &BuildMap, a: usize, b: usize) -> bool {
    let (Some(sa), Some(sb)) = (map.sectors.get(a), map.sectors.get(b)) else {
        return false;
    };
    sa.floorz != sb.floorz || sa.ceilingz != sb.ceilingz
}

/// Two-sided lines lying along a one-sided wall of one of their own sectors
/// (Build bridge strips drawn against the outer wall). Doom node building
/// can't take collinear overlapping lines, so the shared span becomes a
/// one-sided line of the other sector (`emitted wall -> its wall`) and the
/// outer wall is cut around it (`wall -> removed spans`).
#[allow(clippy::type_complexity)]
fn boundary_overlaps(
    map: &BuildMap,
    wsec: &[Option<usize>],
    partner_of: &dyn Fn(usize) -> Option<usize>,
    wall_first: &dyn Fn(usize, usize) -> bool,
) -> (BTreeMap<usize, usize>, BTreeMap<usize, Vec<(Pt, Pt)>>) {
    let n = map.walls.len();
    let seg = |i: usize| -> Option<(Pt, Pt)> {
        let w = &map.walls[i];
        let p2 = map.walls.get(usize::try_from(w.point2).ok()?)?;
        Some(((w.x, w.y), (p2.x, p2.y)))
    };
    let mut solid: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, &s) in wsec.iter().enumerate().take(n) {
        if let (Some(s), None) = (s, partner_of(i)) {
            solid.entry(s).or_default().push(i);
        }
    }
    let mut boundary = BTreeMap::new();
    let mut cuts: BTreeMap<usize, Vec<(Pt, Pt)>> = BTreeMap::new();
    for i in 0..n {
        let Some(j) = partner_of(i) else { continue };
        if wall_first(j, i) {
            continue;
        }
        let Some((a, b)) = seg(i) else { continue };
        for (outer, inner) in [(i, j), (j, i)] {
            let Some(s) = wsec[outer] else { continue };
            let hit = solid
                .get(&s)
                .into_iter()
                .flatten()
                .copied()
                .find(|&k| seg(k).is_some_and(|(c, d)| covers(c, d, a, b)));
            if let Some(k) = hit {
                boundary.insert(i, inner);
                cuts.entry(k).or_default().push((a, b));
                break;
            }
        }
    }
    (boundary, cuts)
}

/// Does segment `c`–`d` contain both `a` and `b`?
fn covers(c: Pt, d: Pt, a: Pt, b: Pt) -> bool {
    let rel = |p: Pt| {
        (
            i64::from(p.0) - i64::from(c.0),
            i64::from(p.1) - i64::from(c.1),
        )
    };
    let (dx, dy) = rel(d);
    let len2 = dx * dx + dy * dy;
    a != b
        && len2 > 0
        && [a, b].into_iter().all(|p| {
            let (px, py) = rel(p);
            dx * py - dy * px == 0 && (0..=len2).contains(&(px * dx + py * dy))
        })
}

/// What is left of wall `i` (to `p2`) once `spans` are removed, in order.
fn uncut_pieces(map: &BuildMap, i: usize, p2: usize, spans: &[(Pt, Pt)]) -> Vec<(Pt, Pt)> {
    let (c, d) = (
        (map.walls[i].x, map.walls[i].y),
        (map.walls[p2].x, map.walls[p2].y),
    );
    let (dx, dy) = (
        i64::from(d.0) - i64::from(c.0),
        i64::from(d.1) - i64::from(c.1),
    );
    let t = |p: Pt| (i64::from(p.0) - i64::from(c.0)) * dx + (i64::from(p.1) - i64::from(c.1)) * dy;
    let mut sorted: Vec<(Pt, Pt)> = spans
        .iter()
        .map(|&(a, b)| if t(a) <= t(b) { (a, b) } else { (b, a) })
        .collect();
    sorted.sort_by_key(|&(a, _)| t(a));
    let mut out = Vec::new();
    let mut at = c;
    for (a, b) in sorted {
        if t(a) > t(at) {
            out.push((at, a));
        }
        if t(b) > t(at) {
            at = b;
        }
    }
    if t(d) > t(at) {
        out.push((at, d));
    }
    out
}

/// Squared distance from `p` to segment `a`–`b`.
fn seg_dist2(p: [i32; 2], a: [i32; 2], b: [i32; 2]) -> f64 {
    let f = |v: [i32; 2]| (f64::from(v[0]), f64::from(v[1]));
    let ((px, py), (ax, ay), (bx, by)) = (f(p), f(a), f(b));
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (ex, ey) = (px - (ax + dx * t), py - (ay + dy * t));
    ex * ex + ey * ey
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::art::build_art;
    use crate::map::{Sector, Sprite, Wall, square_map};

    /// Raze `UVCalculator::GetUV` on Room-space points (y up), slope ignored.
    fn raze_uv(
        stat: u16,
        pan: (u8, u8),
        tile: (f64, f64),
        w0: (f64, f64),
        w1: (f64, f64),
        p: (f64, f64),
    ) -> (f64, f64) {
        let pow2 = |s: f64| 2f64.powf(s.log2().ceil());
        let half = if stat & 8 != 0 { 0.5 } else { 1.0 };
        let (xs, ys) = (pow2(tile.0) * half, pow2(tile.1) * half);
        let (bx, by) = (p.0, -p.1);
        let (ix1, iy1, ix2, iy2) = (w0.0, -w0.1, w1.0, -w1.1);
        let (mut tu, mut tv) = if stat & 64 != 0 {
            let vang = (-(iy2 - iy1)).atan2(ix2 - ix1) - std::f64::consts::FRAC_PI_2;
            let (ca, sa) = (vang.cos(), vang.sin());
            let (dx, dy) = (bx - ix1, by - iy1);
            (-(dx * sa + dy * ca), dx * ca - dy * sa)
        } else {
            (bx, -by)
        };
        if stat & 4 != 0 {
            std::mem::swap(&mut tu, &mut tv);
        }
        if stat & 16 != 0 {
            tu = -tu;
        }
        if stat & 32 != 0 {
            tv = -tv;
        }
        (
            tu / xs + f64::from(pan.0) / 256.0,
            tv / ys + f64::from(pan.1) / 256.0,
        )
    }

    #[test]
    fn flat_align_matches_raze_plane_uvs() {
        let (w0, w1) = ((100.0, -40.0), (37.0, 211.0));
        let room_uv = |a: FlatAlign, (x, y): (f64, f64)| {
            let (c, s) = (a.rotation.to_radians().cos(), a.rotation.to_radians().sin());
            (
                (x * c - y * s + a.pan_u) / 64.0 / a.scale_u,
                (x * s + y * c + a.pan_v) / 64.0 / a.scale_v,
            )
        };
        for stat in 0..128u16 {
            if stat & 3 != 0 {
                continue;
            }
            for tile in [(64.0, 64.0), (128.0, 32.0), (100.0, 200.0)] {
                let a = flat_align(stat, (40, 200), 0, tile, w0, w1);
                for p in [(0.0, 0.0), (300.0, 17.0), (-52.0, -999.0), (w0.0, w0.1)] {
                    let (ru, rv) = room_uv(a, p);
                    let (zu, zv) = raze_uv(stat, (40, 200), tile, w0, w1, p);
                    assert!(
                        (ru - zu).abs() < 1e-6 && (rv - zv).abs() < 1e-6,
                        "stat {stat:#x} tile {tile:?} at {p:?}: room ({ru}, {rv}) raze ({zu}, {zv})"
                    );
                }
            }
        }
        let plain = flat_align(0, (0, 0), 0, (64.0, 64.0), w0, w1);
        assert_eq!(
            plain,
            FlatAlign {
                rotation: 0.0,
                scale_u: 1.0,
                scale_v: 1.0,
                pan_u: 0.0,
                pan_v: 0.0
            }
        );
    }

    fn tiles(art: &[u8]) -> TileSet<'_> {
        let mut t = TileSet::new();
        t.add_art(art).unwrap();
        t
    }

    #[test]
    fn map_names() {
        assert_eq!(doom_map_name("E1L1.MAP").as_deref(), Some("E1M1"));
        assert_eq!(doom_map_name("E4L11.MAP").as_deref(), Some("E4M11"));
        assert_eq!(doom_map_name("ARENA.MAP").as_deref(), Some("ARENA"));
        assert_eq!(doom_map_name("TOO_LONG_NAME.MAP"), None);
        assert_eq!(doom_map_name("E1L1.CON"), None);
    }

    #[test]
    fn square_room_converts_with_scale_flip_and_start() {
        let art = build_art(
            0,
            &[
                (64, 64, &[1; 4096]),
                (64, 64, &[2; 4096]),
                (64, 64, &[3; 4096]),
                (64, 64, &[4; 4096]),
            ],
        );
        let set = tiles(&art);
        let c = convert_map(&square_map(), &set).unwrap();
        let t = &c.textmap;
        assert!(t.contains("vertex { x = 64.0; y = 0.0; }"));
        assert!(t.contains("x = 64.0; y = -64.0;"));
        assert_eq!(t.matches("linedef").count(), 4);
        assert!(t.contains("heightfloor = 0.0; heightceiling = 64.0;"));
        assert!(t.contains("texturemiddle = \"BT0001\""));
        // 1024 build units = 64 map units, xrepeat 8 → 64 texels → 1 unit per texel.
        assert!(t.contains("scalex_mid = 1.0; scaley_mid = 1.0;"));
        // Start faces 512 (south on screen) → 270°.
        assert!(t.contains("x = 32.0; y = -32.0; angle = 270; type = 1;"));
        assert!(t.contains("type = 11;"));
        assert_eq!(c.tiles, BTreeSet::from([1, 2, 3]));
    }

    #[test]
    fn visibility_writes_fogdensity() {
        let art = build_art(0, &[(64, 64, &[1; 4096])]);
        let set = tiles(&art);
        let mut map = square_map();
        map.sectors[0].visibility = 32;
        let t = convert_map(&map, &set).unwrap().textmap;
        assert!(t.contains("fogdensity = 128;"), "{t}");
    }

    #[test]
    fn monsters_move_off_walls_and_solid_decorations() {
        let m = square_map();
        let wsec = m.wall_sectors();
        let solids = [Solid {
            at: (52.0, -52.0),
            radius: 8.0,
            bottom: 0.0,
            height: 32.0,
        }];
        let fit = Fit {
            map: &m,
            wsec: &wsec,
            solids: &solids,
            sector: 0,
            feet: 0.0,
            radius: 10.0,
            height: 40.0,
        };
        assert_eq!(fit.nearest(32.0, -32.0), (32.0, -32.0));
        assert_eq!(fit.nearest(4.0, -32.0), (10.0, -32.0));
        let (x, y) = fit.nearest(44.0, -44.0);
        assert!(
            (x - 52.0).abs() >= 18.0 || (y + 52.0).abs() >= 18.0,
            "({x}, {y})"
        );
        assert!(
            x <= 54.0 && y >= -54.0,
            "still clear of the walls: ({x}, {y})"
        );
    }

    #[test]
    fn red_walls_make_one_two_sided_line_and_doors() {
        // Two squares sharing the wall x = 1024; the right one is a ceiling door.
        let mut m = square_map();
        let right = [(1024, 0), (2048, 0), (2048, 1024), (1024, 1024)];
        for (i, &(x, y)) in right.iter().enumerate() {
            m.walls.push(Wall {
                x,
                y,
                point2: (4 + (i + 1) % 4) as i16,
                nextwall: -1,
                nextsector: -1,
                picnum: 1,
                xrepeat: 8,
                yrepeat: 8,
                ..Wall::default()
            });
        }
        m.sectors.push(Sector {
            wallptr: 4,
            wallnum: 4,
            ceilingz: 0,
            floorz: 0,
            lotag: LT_CEILING_DOOR,
            ..m.sectors[0]
        });
        // Left wall 1 (1024,0)->(1024,1024) faces right wall 7 (1024,1024)->(1024,0).
        m.walls[1].nextwall = 7;
        m.walls[1].nextsector = 1;
        m.walls[7].nextwall = 1;
        m.walls[7].nextsector = 0;
        let c = convert_map(&m, &TileSet::new()).unwrap();
        let t = &c.textmap;
        assert_eq!(t.matches("linedef").count(), 7);
        assert_eq!(t.matches("twosided = true").count(), 1);
        assert!(t.contains("special = 12; arg0 = 2; arg1 = 16; arg2 = 63000;"));
        assert!(t.contains("id = 2;"));
        assert_eq!(t.matches("vertex").count(), 6);
    }

    #[test]
    fn bigforce_walls_write_contact_damage() {
        let mut m = square_map();
        m.walls[0].overpicnum = 230;
        m.walls[0].cstat |= 16;
        let t = &convert_map(&m, &TileSet::new()).unwrap().textmap;
        assert!(t.contains("user_contactdamage = 8;"));
        assert!(t.contains("blockhitscan = true;"));
    }

    #[test]
    fn se8_in_a_ceiling_door_writes_door_light_fields() {
        let mut m = square_map();
        let right = [(1024, 0), (2048, 0), (2048, 1024), (1024, 1024)];
        for (i, &(x, y)) in right.iter().enumerate() {
            m.walls.push(Wall {
                x,
                y,
                point2: (4 + (i + 1) % 4) as i16,
                nextwall: -1,
                nextsector: -1,
                picnum: 1,
                xrepeat: 8,
                yrepeat: 8,
                ..Wall::default()
            });
        }
        m.sectors.push(Sector {
            wallptr: 4,
            wallnum: 4,
            ceilingz: 0,
            floorz: 0,
            lotag: LT_CEILING_DOOR,
            floorshade: 20,
            ..m.sectors[0]
        });
        m.walls[1].nextwall = 7;
        m.walls[1].nextsector = 1;
        m.walls[7].nextwall = 1;
        m.walls[7].nextsector = 0;
        m.sprites.push(Sprite {
            x: 1536,
            y: 512,
            picnum: 1,
            lotag: 8,
            hitag: 7,
            shade: 0,
            sectnum: 1,
            ..Sprite::default()
        });
        m.sprites.push(Sprite {
            x: 512,
            y: 512,
            picnum: 1,
            lotag: 8,
            hitag: 7,
            shade: 0,
            sectnum: 0,
            ..Sprite::default()
        });
        let t = &convert_map(&m, &TileSet::new()).unwrap().textmap;
        assert!(t.contains("user_doorlight = 410058;"));
        assert!(t.contains("user_doorbright = 255;"));
        assert!(t.contains("user_doorlightopen = 1;"));
        assert!(t.contains("410058"));
    }

    #[test]
    fn mirror_walls_become_one_sided_line_mirrors() {
        let mut m = square_map();
        let right = [(1024, 0), (2048, 0), (2048, 1024), (1024, 1024)];
        for (i, &(x, y)) in right.iter().enumerate() {
            m.walls.push(Wall {
                x,
                y,
                point2: (4 + (i + 1) % 4) as i16,
                nextwall: -1,
                nextsector: -1,
                picnum: 1,
                xrepeat: 8,
                yrepeat: 8,
                ..Wall::default()
            });
        }
        m.sectors.push(Sector {
            wallptr: 4,
            wallnum: 4,
            ..m.sectors[0]
        });
        (m.walls[1].nextwall, m.walls[1].nextsector) = (7, 1);
        (m.walls[7].nextwall, m.walls[7].nextsector) = (1, 0);
        m.walls[7].overpicnum = 560;
        m.walls[7].cstat = 32 | 16;
        let t = convert_map(&m, &TileSet::new()).unwrap().textmap;
        assert_eq!(t.matches("linedef").count(), 7);
        assert!(!t.contains("twosided"), "{t}");
        let line = t.lines().find(|l| l.contains("special = 182;")).unwrap();
        let front: usize = line
            .split("sidefront = ")
            .nth(1)
            .and_then(|s| s.split(';').next())
            .unwrap()
            .parse()
            .unwrap();
        let side = t
            .lines()
            .filter(|l| l.starts_with("sidedef"))
            .nth(front)
            .unwrap();
        assert!(
            side.contains("sector = 1;"),
            "faces the mirror's room: {side}"
        );
    }

    #[test]
    fn inner_loop_on_the_outer_wall_cuts_it_and_faces_inward() {
        // A strip sector (1) drawn as a hole of sector 0, its east edge on
        // sector 0's own east wall x = 1024 (Build bridge strips).
        let mut m = square_map();
        let wall = |x, y, point2, nextwall: i16, nextsector| Wall {
            x,
            y,
            point2,
            nextwall,
            nextsector,
            picnum: 1,
            xrepeat: 8,
            yrepeat: 8,
            ..Wall::default()
        };
        m.walls.extend([
            wall(512, 256, 5, 11, 1),
            wall(512, 320, 6, 10, 1),
            wall(1024, 320, 7, 9, 1),
            wall(1024, 256, 4, 8, 1),
            wall(512, 256, 9, 7, 0),
            wall(1024, 256, 10, 6, 0),
            wall(1024, 320, 11, 5, 0),
            wall(512, 320, 8, 4, 0),
        ]);
        m.sectors[0].wallnum = 8;
        m.sectors.push(Sector {
            wallptr: 8,
            wallnum: 4,
            ..m.sectors[0]
        });
        let c = convert_map(&m, &TileSet::new()).unwrap();
        let t = &c.textmap;
        // Outer east wall in two pieces + 3 outer walls + 3 red walls + the
        // shared edge as a one-sided line of the strip.
        assert_eq!(t.matches("linedef").count(), 9, "{t}");
        assert_eq!(t.matches("twosided = true").count(), 3);
        let v = |x: f64, y: f64| {
            t.lines()
                .filter(|l| l.starts_with("vertex"))
                .position(|l| l == format!("vertex {{ x = {x:.1}; y = {y:.1}; }}"))
                .unwrap()
        };
        let line = |a: usize, b: usize| {
            t.lines()
                .find(|l| l.starts_with(&format!("linedef {{ v1 = {a}; v2 = {b};")))
                .unwrap_or_else(|| panic!("no line {a}->{b}: {t}"))
                .to_owned()
        };
        // Strip wall (1024,256)->(1024,320) keeps the strip on its front.
        let edge = line(v(64.0, -16.0), v(64.0, -20.0));
        assert!(!edge.contains("sideback"), "{edge}");
        line(v(64.0, 0.0), v(64.0, -16.0));
        line(v(64.0, -20.0), v(64.0, -64.0));
        let sides: Vec<&str> = t.lines().filter(|l| l.starts_with("sidedef")).collect();
        let front: usize = edge
            .split("sidefront = ")
            .nth(1)
            .and_then(|s| s.split(';').next())
            .unwrap()
            .parse()
            .unwrap();
        assert!(sides[front].contains("sector = 1;"), "{}", sides[front]);
    }

    #[test]
    fn a_lower_opening_on_the_outer_wall_stays_a_door() {
        let mut m = square_map();
        let wall = |x, y, point2, nextwall: i16, nextsector| Wall {
            x,
            y,
            point2,
            nextwall,
            nextsector,
            picnum: 1,
            xrepeat: 8,
            yrepeat: 8,
            ..Wall::default()
        };
        m.walls.extend([
            wall(512, 256, 5, 11, 1),
            wall(512, 320, 6, 10, 1),
            wall(1024, 320, 7, 9, 1),
            wall(1024, 256, 4, 8, 1),
            wall(512, 256, 9, 7, 0),
            wall(1024, 256, 10, 6, 0),
            wall(1024, 320, 11, 5, 0),
            wall(512, 320, 8, 4, 0),
        ]);
        m.sectors[0].wallnum = 8;
        m.sectors.push(Sector {
            wallptr: 8,
            wallnum: 4,
            ceilingz: -8192,
            ..m.sectors[0]
        });
        let t = &convert_map(&m, &TileSet::new()).unwrap().textmap;
        let v = |x: f64, y: f64| {
            t.lines()
                .filter(|l| l.starts_with("vertex"))
                .position(|l| l == format!("vertex {{ x = {x:.1}; y = {y:.1}; }}"))
                .unwrap()
        };
        let edge = t
            .lines()
            .find(|l| {
                l.starts_with(&format!(
                    "linedef {{ v1 = {}; v2 = {};",
                    v(64.0, -16.0),
                    v(64.0, -20.0)
                ))
            })
            .unwrap();
        assert!(edge.contains("twosided = true"), "{edge}");
        assert!(!edge.contains("blocking"), "{edge}");
    }

    #[test]
    fn slopes_become_planes_through_the_first_wall() {
        let mut m = square_map();
        m.sectors[0].floorstat = 2;
        m.sectors[0].floorheinum = 4096;
        let c = convert_map(&m, &TileSet::new()).unwrap();
        let line = c.textmap.lines().find(|l| l.starts_with("sector")).unwrap();
        let get = |k: &str| -> f64 {
            let i = line.find(&format!("{k} = ")).unwrap() + k.len() + 3;
            line[i..line[i..].find(';').unwrap() + i].parse().unwrap()
        };
        let (a, b, cc, d) = (
            get("floorplane_a"),
            get("floorplane_b"),
            get("floorplane_c"),
            get("floorplane_d"),
        );
        let z = |x: f64, y: f64| -(a * x + b * y + d) / cc;
        // First wall runs along y = 0; 45° slope descends into the room.
        assert!(z(10.0, 0.0).abs() < 1e-9);
        assert!((z(10.0, -32.0) + 32.0).abs() < 1e-9);
    }

    #[test]
    fn nuke_button_exits() {
        let mut m = square_map();
        m.sprites.push(crate::map::Sprite {
            x: 1000,
            y: 500,
            picnum: NUKEBUTTON,
            sectnum: 0,
            ..Default::default()
        });
        let c = convert_map(&m, &TileSet::new()).unwrap();
        assert_eq!(c.textmap.matches("special = 243;").count(), 1);
    }
}
