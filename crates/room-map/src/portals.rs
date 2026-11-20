//! Line and sector portals, as UZDoom sets them up at load time.
//!
//! Ported from UZDoom `src/maploader/specials.cpp` (`SpawnLinePortal`,
//! `SetupPortals`, `SpawnPortal`, `CopyPortal`, `SpawnSkybox`) and
//! `src/playsim/portal.cpp` (`UpdatePortal`, `SetPortalRotation`,
//! `CreateLinkedPortals`, `ConnectPortalGroups`), GPL-3.0-or-later.
//!
//! * **Line portals** (`Line_SetPortal` 156, `Line_QuickPortal` 301): an
//!   origin line shows (and, for the passable types, leads to) a destination
//!   line elsewhere in the map. Visual, teleport, interactive and linked
//!   types follow `PORTT_*`; the Eternity two-line form (type 4) too.
//! * **Sector portals**: one table shared by every sector plane, as
//!   UZDoom's `sectorPortals`. Entry 0 is the level's default sky (a
//!   `SkyViewpoint` without a TID takes it), entry 1 a plain sky. Stacked
//!   sectors come from `UpperStackLookOnly` / `LowerStackLookOnly` things or
//!   `Sector_SetPortal` 0, linked plane portals from `Sector_SetPortal` 6, EE
//!   skyboxes from `SkyCamCompat`, EE plane and horizon portals from types 3
//!   and 4, and `SkyPicker` things pick a `SkyViewpoint` per sector.
//! * **Portal groups**: linked portals (line and plane) split the map into
//!   groups with a fixed XY offset between each pair (`Displacements`).
//!   Sectors not reached by any linked portal are group 0.
//!
//! Room keeps its Build water / fall links (`docs/23-build-engine.md`) in
//! [`PortalTables::build_links`]; they are not UZDoom portals.

use crate::geom::{Line, MapGeometry};
use room_core::{LineId, SectorId, dmath};
use std::collections::BTreeMap;

/// `Line_SetPortal` (target id, this id, type, alignment).
pub const LINE_SET_PORTAL: i16 = 156;
/// `Line_QuickPortal` (non-linked): pairs two lines with the same line id.
pub const LINE_QUICK_PORTAL: i16 = 301;
/// `Sector_SetPortal` (tag, type, plane, misc, alpha).
pub const SECTOR_SET_PORTAL: i16 = 57;
/// `Line_Horizon` (wall drawn as the front sector's planes to the horizon).
pub const LINE_HORIZON: i16 = 9;
/// `Line_Mirror`.
pub const LINE_MIRROR: i16 = 182;
/// `Line_SetIdentification` (Hexen line ids).
const LINE_SET_IDENTIFICATION: i16 = 121;
/// Hexen-format specials that carry their own line id (UZDoom `SetLineID`).
const TRANSLUCENT_LINE: i16 = 208;
const TELEPORT_LINE: i16 = 215;
const SCROLL_TEXTURE_MODEL: i16 = 222;
const POLYOBJ_START_LINE: i16 = 1;
const POLYOBJ_EXPLICIT_LINE: i16 = 5;
const PLANE_ALIGN: i16 = 181;
const STATIC_INIT: i16 = 190;
const INIT_SECTOR_LINK: i32 = 4;

/// `PORTT_VISUAL`: drawn only.
pub const PORTT_VISUAL: u8 = 0;
/// `PORTT_TELEPORT`: actors crossing are moved (not seamless).
pub const PORTT_TELEPORT: u8 = 1;
/// `PORTT_INTERACTIVE`: seamless crossing, collision on both sides.
pub const PORTT_INTERACTIVE: u8 = 2;
/// `PORTT_LINKED`: static XY offset between two map areas (portal groups).
pub const PORTT_LINKED: u8 = 3;
/// `PORTT_LINKEDEE`: Eternity two-line linked portal.
pub const PORTT_LINKEDEE: u8 = 4;
/// Room-only line portal kind: `Line_Horizon`.
pub const PORT_KIND_HORIZON: u8 = 9;
/// Room-only line portal kind: `Line_Mirror`.
pub const PORT_KIND_MIRROR: u8 = 182;

/// `PORTF_VISIBLE`.
pub const PORTF_VISIBLE: u8 = 1;
/// `PORTF_PASSABLE`.
pub const PORTF_PASSABLE: u8 = 2;
/// `PORTF_SOUNDTRAVERSE`.
pub const PORTF_SOUNDTRAVERSE: u8 = 4;
/// `PORTF_INTERACTIVE`.
pub const PORTF_INTERACTIVE: u8 = 8;
/// `PORTF_TYPETELEPORT`.
pub const PORTF_TYPETELEPORT: u8 = PORTF_VISIBLE | PORTF_PASSABLE | PORTF_SOUNDTRAVERSE;
/// `PORTF_TYPEINTERACTIVE`.
pub const PORTF_TYPEINTERACTIVE: u8 =
    PORTF_VISIBLE | PORTF_PASSABLE | PORTF_SOUNDTRAVERSE | PORTF_INTERACTIVE;

/// `PORG_ABSOLUTE`: no Z alignment.
pub const PORG_ABSOLUTE: u8 = 0;
/// `PORG_FLOOR`: Z follows the floors of the two front sectors.
pub const PORG_FLOOR: u8 = 1;
/// `PORG_CEILING`: Z follows the ceilings.
pub const PORG_CEILING: u8 = 2;

/// `PORTS_SKYVIEWPOINT`: a sky (with or without a `SkyViewpoint`).
pub const PORTS_SKYVIEWPOINT: u8 = 0;
/// `PORTS_STACKEDSECTORTHING`: stacked sectors set up by stack things.
pub const PORTS_STACKEDSECTORTHING: u8 = 1;
/// `PORTS_PORTAL`: stacked sectors set up by `Sector_SetPortal` 0.
pub const PORTS_PORTAL: u8 = 2;
/// `PORTS_LINKEDPORTAL`: interactive plane portal (`Sector_SetPortal` 6).
pub const PORTS_LINKEDPORTAL: u8 = 3;
/// `PORTS_PLANE`: Eternity plane portal (`Sector_SetPortal` 3).
pub const PORTS_PLANE: u8 = 4;
/// `PORTS_HORIZON`: Eternity horizon portal (`Sector_SetPortal` 4).
pub const PORTS_HORIZON: u8 = 5;

/// `PORTSF_SKYFLATONLY`: the portal only shows on sky-flat planes.
pub const PORTSF_SKYFLATONLY: u8 = 1;

/// `PLANEF_NORENDER` (UDMF `portal_*_norender`).
pub const PLANEF_NORENDER: u16 = 8;
/// `PLANEF_NOPASS` (UDMF `portal_*_nopass`).
pub const PLANEF_NOPASS: u16 = 16;
/// `PLANEF_BLOCKSOUND` (UDMF `portal_*_blocksound`).
pub const PLANEF_BLOCKSOUND: u16 = 32;
/// `PLANEF_DISABLED` (UDMF `portal_*_disabled`).
pub const PLANEF_DISABLED: u16 = 64;
/// `PLANEF_OBSTRUCTED`: the sector's own plane is past the portal plane.
pub const PLANEF_OBSTRUCTED: u16 = 128;
/// `PLANEF_LINKED`: the plane is a working linked portal.
pub const PLANEF_LINKED: u16 = 256;

/// Sector plane index: floor.
pub const PLANE_FLOOR: usize = 0;
/// Sector plane index: ceiling.
pub const PLANE_CEILING: usize = 1;

/// Thing numbers (UZDoom `mapinfo/common.txt`).
pub const THING_UPPER_STACK: i16 = 9077;
/// `LowerStackLookOnly`.
pub const THING_LOWER_STACK: i16 = 9078;
/// `SkyViewpoint`.
pub const THING_SKY_VIEWPOINT: i16 = 9080;
/// `SkyPicker`.
pub const THING_SKY_PICKER: i16 = 9081;
/// `SkyCamCompat`.
pub const THING_SKY_CAM_COMPAT: i16 = 9083;
/// `SkyCamCompat` (Eternity number).
pub const THING_SKY_CAM_COMPAT_EE: i16 = 5006;

/// Room's Build water link (`docs/23-build-engine.md`): only players pass,
/// down through the floor when swimming down or falling fast, up through the
/// ceiling when swimming up; they move by [`BuildLink::offset`].
pub const PORTAL_KIND_WATER: u8 = 200;

/// Room's Build `SE 7` transporter floating above its floor. A player whose
/// eye comes within 24 units of the source effector height moves by
/// [`BuildLink::offset`] to 24 units below (above, when rising) the
/// destination effector. `arg2` packs the heights: source in the low 16
/// bits, destination in the high 16 (both signed).
pub const PORTAL_KIND_FALL: u8 = 201;

/// One line portal (`FLinePortal`).
#[derive(Clone, Copy, Debug)]
pub struct LinePortal {
    /// Origin line.
    pub line: LineId,
    /// Destination line (`None` for horizons, mirrors and unconnected ones).
    pub target: Option<LineId>,
    /// `PORTT_*`, or [`PORT_KIND_HORIZON`] / [`PORT_KIND_MIRROR`].
    pub kind: u8,
    /// Active `PORTF_*` bits.
    pub flags: u8,
    /// Default `PORTF_*` bits for the type.
    pub def_flags: u8,
    /// `PORG_*`.
    pub align: u8,
    /// Linked portals: `destination.v2 − origin.v1`.
    pub displacement: (f64, f64),
    /// Rotation from origin to destination, degrees.
    pub angle_diff: f64,
    /// `sin(angle_diff)`.
    pub sin_rot: f64,
    /// `cos(angle_diff)`.
    pub cos_rot: f64,
}

impl LinePortal {
    fn new(line: LineId, target: Option<LineId>, kind: u8) -> Self {
        Self {
            line,
            target,
            kind,
            flags: 0,
            def_flags: 0,
            align: PORG_ABSOLUTE,
            displacement: (0.0, 0.0),
            angle_diff: 0.0,
            sin_rot: 0.0,
            cos_rot: 1.0,
        }
    }

    /// Actors may cross (`PORTF_PASSABLE`).
    #[must_use]
    pub fn is_traversable(self) -> bool {
        self.flags & PORTF_PASSABLE != 0
    }

    /// Seamless crossing with collision on both sides (`PORTF_INTERACTIVE`).
    #[must_use]
    pub fn is_interactive(self) -> bool {
        self.flags & PORTF_INTERACTIVE != 0
    }

    /// Linked (portal groups, pure offset).
    #[must_use]
    pub fn is_linked(self) -> bool {
        self.kind == PORTT_LINKED && self.flags != 0
    }

    /// Sound passes (`PORTF_SOUNDTRAVERSE`).
    #[must_use]
    pub fn passes_sound(self) -> bool {
        self.flags & PORTF_SOUNDTRAVERSE != 0
    }

    /// Drawn as a view of the destination (`isVisualPortal`).
    #[must_use]
    pub fn is_visual(self) -> bool {
        self.flags & PORTF_VISIBLE != 0 && self.target.is_some()
    }

    /// `Line_Mirror`.
    #[must_use]
    pub fn is_mirror(self) -> bool {
        self.kind == PORT_KIND_MIRROR
    }

    /// `Line_Horizon`.
    #[must_use]
    pub fn is_horizon(self) -> bool {
        self.kind == PORT_KIND_HORIZON
    }

    /// The wall is replaced by the portal view (visual portals and horizons).
    /// Mirrors keep their wall, drawn as a reflection.
    #[must_use]
    pub fn skip_wall(self) -> bool {
        self.is_visual() || self.is_horizon()
    }

    /// `P_TranslatePortalXY`: a point on the origin side, in destination
    /// coordinates.
    #[must_use]
    pub fn translate_xy(&self, map: &MapGeometry, x: f64, y: f64) -> (f64, f64) {
        let Some(dst) = self.target.and_then(|t| map.lines.get(t.raw() as usize)) else {
            return (x, y);
        };
        let Some(src) = map.lines.get(self.line.raw() as usize) else {
            return (x, y);
        };
        let s1 = map.vertices[src.v1.raw() as usize].pos;
        let d2 = map.vertices[dst.v2.raw() as usize].pos;
        let nx = x - s1.x();
        let ny = y - s1.y();
        let tx = nx * self.cos_rot - ny * self.sin_rot;
        let ty = ny * self.cos_rot + nx * self.sin_rot;
        (tx + d2.x(), ty + d2.y())
    }

    /// `P_TranslatePortalVXVY`.
    #[must_use]
    pub fn translate_vel(&self, vx: f64, vy: f64) -> (f64, f64) {
        (
            vx * self.cos_rot - vy * self.sin_rot,
            vy * self.cos_rot + vx * self.sin_rot,
        )
    }

    /// `P_TranslatePortalAngle` (degrees in, degrees out, 0–360).
    #[must_use]
    pub fn translate_angle_deg(&self, angle: f64) -> f64 {
        (angle + self.angle_diff).rem_euclid(360.0)
    }

    /// `P_TranslatePortalZ`: `floor` / `ceiling` give the current plane
    /// heights of (origin front, destination front) at their line ends.
    #[must_use]
    pub fn translate_z(&self, z: f64, floors: (f64, f64), ceilings: (f64, f64)) -> f64 {
        match self.align {
            PORG_FLOOR => z - floors.0 + floors.1,
            PORG_CEILING => z - ceilings.0 + ceilings.1,
            _ => z,
        }
    }
}

/// One sector portal (`FSectorPortal`).
#[derive(Clone, Copy, Debug)]
pub struct SectorPortal {
    /// `PORTS_*`.
    pub kind: u8,
    /// `PORTSF_*`.
    pub flags: u8,
    /// Stacked-thing portals: the matching portal on the other side.
    pub partner: Option<u32>,
    /// Plane the portal was made for (0 floor, 1 ceiling, 2 both).
    pub plane: u8,
    /// Sector the portal is seen from.
    pub origin: Option<SectorId>,
    /// Sector the portal leads to (the viewpoint's sector for skies).
    pub destination: Option<SectorId>,
    /// XY offset from the origin's coordinates to the destination's.
    pub displacement: (f64, f64),
    /// Linked portals: plane height at load (`GetPlaneTexZ`), else `f64::MAX`.
    pub plane_z: f64,
    /// Sky viewpoint thing (index into `map.things`).
    pub skybox: Option<usize>,
}

impl SectorPortal {
    fn sky() -> Self {
        Self {
            kind: PORTS_SKYVIEWPOINT,
            flags: PORTSF_SKYFLATONLY,
            partner: None,
            plane: 0,
            origin: None,
            destination: None,
            displacement: (0.0, 0.0),
            plane_z: f64::MAX,
            skybox: None,
        }
    }

    /// A sky portal without a viewpoint is just the regular sky.
    #[must_use]
    pub fn is_plain_sky(&self) -> bool {
        self.kind == PORTS_SKYVIEWPOINT && self.skybox.is_none()
    }
}

/// One Room Build link (water or fall transporter).
#[derive(Clone, Copy, Debug)]
pub struct BuildLink {
    /// Source sector.
    pub sector: SectorId,
    /// Destination sector.
    pub target: SectorId,
    /// [`PORTAL_KIND_WATER`] or [`PORTAL_KIND_FALL`].
    pub kind: u8,
    /// `0` = floor plane, `1` = ceiling plane.
    pub plane: u8,
    /// XY offset (`args[3]`, `args[4]` map units).
    pub offset: [i32; 2],
    /// [`PORTAL_KIND_FALL`] source and destination effector heights.
    pub heights: [i32; 2],
}

/// A sky viewpoint or stack point thing, as the renderer needs it.
#[derive(Clone, Copy, Debug)]
pub struct PortalThing {
    /// Index into `map.things`.
    pub thing: usize,
    /// Map XY.
    pub x: f64,
    /// Map XY.
    pub y: f64,
    /// Height above the floor (`height` / Hexen z).
    pub z_offset: f64,
    /// Facing, degrees.
    pub angle: f64,
    /// Sector the thing stands in.
    pub sector: Option<SectorId>,
}

/// All portal tables.
#[derive(Clone, Debug, Default)]
pub struct PortalTables {
    /// Line portals (`linePortals`).
    pub lines: Vec<LinePortal>,
    /// Per line: index into [`Self::lines`].
    pub line_portal: Vec<Option<u32>>,
    /// Sector portals (`sectorPortals`; 0 = default sky, 1 = plain sky).
    pub sectors: Vec<SectorPortal>,
    /// Per sector: portal index for `[floor, ceiling]`.
    pub sector_planes: Vec<[u32; 2]>,
    /// Per sector: `PLANEF_*` bits for `[floor, ceiling]`.
    pub plane_flags: Vec<[u16; 2]>,
    /// Per sector: portal plane alpha (`Sector_SetPortal` arg 4, stack thing arg 0).
    pub plane_alpha: Vec<[f32; 2]>,
    /// Per line: sector portal drawn on the line (`Sector_SetPortal` 5).
    pub line_copy: Vec<Option<u32>>,
    /// Sky viewpoint and stack point things.
    pub things: BTreeMap<usize, PortalThing>,
    /// Room Build links.
    pub build_links: Vec<BuildLink>,
    /// Some line portal can be crossed (cached for the movement code).
    pub has_passable_lines: bool,
    /// Some sector plane is a working linked portal.
    pub has_linked_planes: bool,
    /// Bumped when a portal changes at runtime (`Line_SetPortalTarget`), so
    /// renderers holding their own map copy can pick the change up.
    pub revision: u32,
}

impl PortalTables {
    /// Line portal of a line.
    #[must_use]
    pub fn line(&self, line: LineId) -> Option<&LinePortal> {
        let i = (*self.line_portal.get(line.raw() as usize)?)?;
        self.lines.get(i as usize)
    }

    /// Portal index of a sector plane (0 = default sky).
    #[must_use]
    pub fn plane_portal_index(&self, sector: usize, plane: usize) -> u32 {
        self.sector_planes.get(sector).map_or(0, |p| p[plane & 1])
    }

    /// Portal of a sector plane.
    #[must_use]
    pub fn plane_portal(&self, sector: usize, plane: usize) -> Option<&SectorPortal> {
        self.sectors
            .get(self.plane_portal_index(sector, plane) as usize)
    }

    /// `PLANEF_*` of a sector plane.
    #[must_use]
    pub fn flags(&self, sector: usize, plane: usize) -> u16 {
        self.plane_flags.get(sector).map_or(0, |f| f[plane & 1])
    }

    /// `sector_t::PortalIsLinked`.
    #[must_use]
    pub fn plane_is_linked(&self, sector: usize, plane: usize) -> bool {
        self.plane_portal(sector, plane)
            .is_some_and(|p| p.kind == PORTS_LINKEDPORTAL)
    }

    /// `sector_t::PortalBlocksMovement` (static flags; obstruction is
    /// re-checked by the playsim against live plane heights).
    #[must_use]
    pub fn plane_blocks_movement(&self, sector: usize, plane: usize) -> bool {
        PLANEF_LINKED
            != self.flags(sector, plane)
                & (PLANEF_NOPASS | PLANEF_DISABLED | PLANEF_OBSTRUCTED | PLANEF_LINKED)
    }

    /// `sector_t::PortalBlocksSight`.
    #[must_use]
    pub fn plane_blocks_sight(&self, sector: usize, plane: usize) -> bool {
        PLANEF_LINKED
            != self.flags(sector, plane)
                & (PLANEF_NORENDER
                    | PLANEF_NOPASS
                    | PLANEF_DISABLED
                    | PLANEF_OBSTRUCTED
                    | PLANEF_LINKED)
    }

    /// `sector_t::PortalBlocksSound`.
    #[must_use]
    pub fn plane_blocks_sound(&self, sector: usize, plane: usize) -> bool {
        PLANEF_LINKED
            != self.flags(sector, plane)
                & (PLANEF_BLOCKSOUND | PLANEF_DISABLED | PLANEF_OBSTRUCTED | PLANEF_LINKED)
    }

    /// `sector_t::PortalBlocksView` (only linked portals can be switched off).
    #[must_use]
    pub fn plane_blocks_view(&self, sector: usize, plane: usize) -> bool {
        if !self.plane_is_linked(sector, plane) {
            return false;
        }
        self.flags(sector, plane) & (PLANEF_NORENDER | PLANEF_DISABLED | PLANEF_OBSTRUCTED) != 0
    }

    /// `sector_t::GetPortalPlaneZ` (`f64::MAX` unless linked).
    #[must_use]
    pub fn plane_z(&self, sector: usize, plane: usize) -> f64 {
        self.plane_portal(sector, plane)
            .map_or(f64::MAX, |p| p.plane_z)
    }

    /// `sector_t::GetPortalDisplacement`.
    #[must_use]
    pub fn plane_displacement(&self, sector: usize, plane: usize) -> (f64, f64) {
        self.plane_portal(sector, plane)
            .map_or((0.0, 0.0), |p| p.displacement)
    }

    /// `sector_t::ValidatePortal`: the portal to draw on a plane, if any.
    /// `is_sky` says whether the plane's texture is the sky flat.
    #[must_use]
    pub fn visible_plane_portal(&self, sector: usize, plane: usize, is_sky: bool) -> Option<u32> {
        let i = self.plane_portal_index(sector, plane);
        let p = self.sectors.get(i as usize)?;
        if p.kind == PORTS_SKYVIEWPOINT && p.skybox.is_none() {
            return None;
        }
        if self.plane_blocks_view(sector, plane) {
            return None;
        }
        if p.flags & PORTSF_SKYFLATONLY != 0 && !is_sky {
            return None;
        }
        Some(i)
    }

    /// True when the map has any line or sector portal (beyond plain skies).
    #[must_use]
    pub fn any(&self) -> bool {
        !self.lines.is_empty() || self.sectors.iter().any(|p| !p.is_plain_sky())
    }
}

/// Linked portal groups and the displacement table (`FDisplacementTable`).
#[derive(Clone, Debug, Default)]
pub struct PortalGroups {
    /// Group per sector (0 = not reached by a linked portal).
    pub sector_group: Vec<u32>,
    /// Number of groups (≥ 1).
    pub size: usize,
    /// `Displacements(x, y)` at `x + size * y`: add to a point in group `x`
    /// to express it in group `y`.
    pub disp: Vec<Option<(f64, f64)>>,
}

impl PortalGroups {
    /// Group of a sector.
    #[must_use]
    pub fn group_of(&self, sector: usize) -> Option<u32> {
        self.sector_group.get(sector).copied()
    }

    /// True when the map has linked portals (more than one group).
    #[must_use]
    pub fn has_links(&self) -> bool {
        self.size > 1
    }

    /// `FDisplacementTable::getOffset(from, to)`.
    #[must_use]
    pub fn offset(&self, from: u32, to: u32) -> (f64, f64) {
        if from == to {
            return (0.0, 0.0);
        }
        let (f, t) = (from as usize, to as usize);
        if f >= self.size || t >= self.size {
            return (0.0, 0.0);
        }
        self.disp[f + self.size * t].unwrap_or((0.0, 0.0))
    }

    /// `FDisplacementTable::MoveGroup`: group `grp` moved by `delta`.
    pub fn move_group(&mut self, grp: u32, delta: (f64, f64)) {
        let g = grp as usize;
        if g >= self.size {
            return;
        }
        for i in 1..self.size {
            if let Some(d) = self.disp[g + self.size * i].as_mut() {
                d.0 -= delta.0;
                d.1 -= delta.1;
            }
            if let Some(d) = self.disp[i + g * self.size].as_mut() {
                d.0 += delta.0;
                d.1 += delta.1;
            }
        }
    }

    /// Whether two groups are connected.
    #[must_use]
    pub fn connected(&self, from: u32, to: u32) -> bool {
        from == to || {
            let (f, t) = (from as usize, to as usize);
            f < self.size && t < self.size && self.disp[f + self.size * t].is_some()
        }
    }
}

/// UZDoom `GetFirstLineID` / `SetLineID`: the ids of a line. UDMF and Doom
/// maps use the line's id (`tag`, `moreids`); Hexen maps take it from the
/// specials that carry one.
#[must_use]
pub fn line_ids(map: &MapGeometry, line: &Line) -> Vec<i32> {
    if map.format != crate::geom::MapFormat::Hexen {
        let mut v = Vec::new();
        if line.tag != 0 {
            v.push(i32::from(line.tag));
        }
        v.extend(line.extra_ids.iter().copied());
        return v;
    }
    let a = line.args;
    let id = match line.special {
        LINE_SET_IDENTIFICATION => Some(a[0] + 256 * a[4]),
        TRANSLUCENT_LINE | TELEPORT_LINE | SCROLL_TEXTURE_MODEL => Some(a[0]),
        POLYOBJ_START_LINE => Some(a[3]),
        POLYOBJ_EXPLICIT_LINE => Some(a[4]),
        PLANE_ALIGN => Some(a[2]),
        STATIC_INIT if a[1] == INIT_SECTOR_LINK => Some(a[0]),
        LINE_SET_PORTAL => Some(a[1]),
        _ => None,
    };
    id.filter(|&i| i != 0).into_iter().collect()
}

fn first_line_id(map: &MapGeometry, line: &Line) -> i32 {
    line_ids(map, line).first().copied().unwrap_or(0)
}

fn line_has_id(map: &MapGeometry, line: &Line, id: i32) -> bool {
    line_ids(map, line).contains(&id)
}

/// `FindPortalDestination`.
fn find_destination(
    map: &MapGeometry,
    src: usize,
    tag: i32,
    special: Option<i16>,
) -> Option<LineId> {
    if tag == 0 {
        return None;
    }
    map.lines
        .iter()
        .enumerate()
        .find(|(i, l)| {
            *i != src && special.is_none_or(|s| l.special == s) && line_has_id(map, l, tag)
        })
        .map(|(i, _)| LineId::from_raw(i as u32))
}

fn vertex(map: &MapGeometry, line: &Line, end: usize) -> (f64, f64) {
    let v = if end == 0 { line.v1 } else { line.v2 };
    let p = map.vertices[v.raw() as usize].pos;
    (p.x(), p.y())
}

fn line_angle_deg(line: &Line) -> f64 {
    dmath::atan2(line.delta.y(), line.delta.x()) * (180.0 / std::f64::consts::PI)
}

fn line_mid(map: &MapGeometry, line: &Line) -> (f64, f64) {
    let (x1, y1) = vertex(map, line, 0);
    (x1 + line.delta.x() / 2.0, y1 + line.delta.y() / 2.0)
}

/// Sector containing a point (load time; no nodes needed). Parity test over
/// each sector's own lines, smallest-area winner for nested sectors.
#[must_use]
pub fn sector_at(map: &MapGeometry, x: f64, y: f64) -> Option<SectorId> {
    let mut best: Option<(usize, f64)> = None;
    for (si, sec) in map.sectors.iter().enumerate() {
        let mut inside = false;
        let mut area = 0.0;
        for &lid in &sec.lines {
            let Some(l) = map.lines.get(lid.raw() as usize) else {
                continue;
            };
            if l.front_sector == l.back_sector {
                continue;
            }
            let (x1, y1) = vertex(map, l, 0);
            let (x2, y2) = vertex(map, l, 1);
            area += x1 * y2 - x2 * y1;
            if (y1 > y) != (y2 > y) {
                let t = (y - y1) / (y2 - y1);
                if x < x1 + t * (x2 - x1) {
                    inside = !inside;
                }
            }
        }
        let area = area.abs();
        if inside && best.is_none_or(|(_, a)| area < a) {
            best = Some((si, area));
        }
    }
    best.map(|(i, _)| SectorId::from_raw(i as u32))
}

fn sectors_tagged(map: &MapGeometry, tag: i32) -> Vec<usize> {
    map.sectors
        .iter()
        .enumerate()
        .filter(|(_, s)| s.tags.contains(&tag))
        .map(|(i, _)| i)
        .collect()
}

fn unpack_fall_heights(arg: i32) -> [i32; 2] {
    [i32::from(arg as u16 as i16), arg >> 16]
}

#[cfg(test)]
#[test]
fn fall_portal_heights_are_signed_halves() {
    let pack = |src: i16, dst: i16| (i32::from(dst) << 16) | i32::from(src as u16);
    assert_eq!(unpack_fall_heights(pack(140, 276)), [140, 276]);
    assert_eq!(unpack_fall_heights(pack(-132, -24)), [-132, -24]);
    assert_eq!(unpack_fall_heights(pack(580, -3000)), [580, -3000]);
}

fn portal_thing(map: &MapGeometry, ti: usize) -> PortalThing {
    let t = &map.things[ti];
    let (x, y) = (f64::from(t.x), f64::from(t.y));
    PortalThing {
        thing: ti,
        x,
        y,
        z_offset: f64::from(t.z),
        angle: f64::from(t.angle),
        sector: sector_at(map, x, y),
    }
}

struct Builder<'a> {
    map: &'a MapGeometry,
    t: PortalTables,
    /// Lines of polyobjects (`WALLF_POLYOBJ`): one-sided, yet passable.
    poly_lines: std::collections::BTreeSet<u32>,
}

impl Builder<'_> {
    fn new_sector_portal(&mut self, p: SectorPortal) -> u32 {
        self.t.sectors.push(p);
        (self.t.sectors.len() - 1) as u32
    }

    fn portal_kind(&self, sector: usize, plane: usize) -> u8 {
        let i = self.t.sector_planes[sector][plane];
        self.t.sectors[i as usize].kind
    }

    /// `MapLoader::SetPortal`.
    fn set_portal(&mut self, sector: usize, plane: i32, pnum: u32, alpha: f32) {
        if sector >= self.t.sector_planes.len() {
            return;
        }
        if plane > 0 && self.portal_kind(sector, PLANE_CEILING) == PORTS_SKYVIEWPOINT {
            self.t.sector_planes[sector][PLANE_CEILING] = pnum;
            if self.t.plane_alpha[sector][PLANE_CEILING] >= 1.0 {
                self.t.plane_alpha[sector][PLANE_CEILING] = alpha;
            }
        }
        if plane == 2 || plane == 0 {
            if self.portal_kind(sector, PLANE_FLOOR) == PORTS_SKYVIEWPOINT {
                self.t.sector_planes[sector][PLANE_FLOOR] = pnum;
            }
            if self.t.plane_alpha[sector][PLANE_FLOOR] >= 1.0 {
                self.t.plane_alpha[sector][PLANE_FLOOR] = alpha;
            }
        }
    }

    /// `MapLoader::CopyPortal`.
    fn copy_portal(&mut self, sectortag: i32, plane: i32, pnum: u32, alpha: f32, tolines: bool) {
        let map = self.map;
        for s in sectors_tagged(map, sectortag) {
            self.set_portal(s, plane, pnum, alpha);
        }
        for (li, line) in map.lines.iter().enumerate() {
            if line.special == SECTOR_SET_PORTAL
                && line.args[1] == 1
                && (line.args[2] == plane || line.args[2] == 3)
                && line.args[3] == sectortag
            {
                if line.args[0] == 0 {
                    if let Some(f) = line.front_sector {
                        self.set_portal(f.raw() as usize, plane, pnum, alpha);
                    }
                } else {
                    for s in sectors_tagged(map, line.args[0]) {
                        self.set_portal(s, plane, pnum, alpha);
                    }
                }
            }
            if tolines
                && line.special == SECTOR_SET_PORTAL
                && line.args[1] == 5
                && line.args[3] == sectortag
            {
                if line.args[0] == 0 {
                    self.t.line_copy[li] = Some(pnum);
                } else {
                    for (lj, other) in map.lines.iter().enumerate() {
                        if line_has_id(map, other, line.args[0]) {
                            self.t.line_copy[lj] = Some(pnum);
                        }
                    }
                }
            }
        }
    }

    fn plane_height(&self, sector: SectorId, plane: usize) -> f64 {
        self.map
            .sectors
            .get(sector.raw() as usize)
            .map_or(0.0, |s| {
                if plane == PLANE_FLOOR {
                    s.floor_height
                } else {
                    s.ceiling_height
                }
            })
    }

    /// `MapLoader::SpawnPortal`.
    fn spawn_portal(
        &mut self,
        line: &Line,
        sectortag: i32,
        plane: i32,
        bytealpha: i32,
        linked: i32,
    ) {
        if !(0..=2).contains(&plane) || (linked != 0 && plane == 2) {
            return;
        }
        let map = self.map;
        for oline in &map.lines {
            if oline.special == SECTOR_SET_PORTAL
                && oline.args[0] == sectortag
                && oline.args[1] == linked
                && oline.args[2] == plane
                && oline.args[3] == 1
            {
                let pos1 = line_mid(map, line);
                let pos2 = line_mid(map, oline);
                let kind = if linked != 0 {
                    PORTS_LINKEDPORTAL
                } else {
                    PORTS_PORTAL
                };
                let plane_z = match (kind, line.front_sector) {
                    (PORTS_LINKEDPORTAL, Some(f)) => self.plane_height(f, plane as usize & 1),
                    _ => f64::MAX,
                };
                let pnum = self.new_sector_portal(SectorPortal {
                    kind,
                    flags: 0,
                    partner: None,
                    plane: plane as u8,
                    origin: line.front_sector,
                    destination: oline.front_sector,
                    displacement: (pos2.0 - pos1.0, pos2.1 - pos1.1),
                    plane_z,
                    skybox: None,
                });
                self.copy_portal(sectortag, plane, pnum, bytealpha as f32 / 255.0, false);
                return;
            }
        }
    }

    /// `MapLoader::SetupPortals` (stack things).
    fn setup_stack_points(&mut self) {
        let map = self.map;
        let mut created: Vec<(u32, usize, usize)> = Vec::new(); // (pnum, point thing, partner thing)
        for (ti, t) in map.things.iter().enumerate() {
            // An `UpperStackLookOnly` sits in the upper area: its sector's
            // floor shows the area of the `LowerStackLookOnly` with the same
            // TID, whose sector's ceiling shows it back.
            let (plane, partner_type) = match t.type_id {
                THING_UPPER_STACK => (PLANE_FLOOR, THING_LOWER_STACK),
                THING_LOWER_STACK => (PLANE_CEILING, THING_UPPER_STACK),
                _ => continue,
            };
            let point = portal_thing(map, ti);
            self.t.things.insert(ti, point);
            let Some(si) = point.sector else {
                continue;
            };
            let Some(vi) = map
                .things
                .iter()
                .position(|o| o.type_id == partner_type && o.tid == t.tid)
            else {
                continue;
            };
            let view = portal_thing(map, vi);
            self.t.things.insert(vi, view);
            let alpha = (t.args[0].clamp(0, 255) as f32) / 255.0;
            if self.t.plane_alpha[si.raw() as usize][plane] >= 1.0 {
                self.t.plane_alpha[si.raw() as usize][plane] = alpha;
            }
            // `GetStackPortal(skyv, plane)`: origin = the point's sector,
            // destination = the partner's sector.
            let pnum = self.new_sector_portal(SectorPortal {
                kind: PORTS_STACKEDSECTORTHING,
                flags: 0,
                partner: None,
                plane: plane as u8,
                origin: Some(si),
                destination: view.sector,
                displacement: (0.0, 0.0),
                plane_z: f64::MAX,
                skybox: Some(vi),
            });
            self.t.sector_planes[si.raw() as usize][plane] = pnum;
            created.push((pnum, ti, vi));
        }
        // Partners and displacement: skybox.pos − skybox.target.pos, where the
        // skybox is the partner and its target the point.
        for &(pnum, point, view) in &created {
            if let Some(&(other, _, _)) = created.iter().find(|&&(_, p2, _)| p2 == view) {
                self.t.sectors[pnum as usize].partner = Some(other);
            }
            let (pv, vv) = (portal_thing(map, point), portal_thing(map, view));
            let p = &mut self.t.sectors[pnum as usize];
            p.displacement = (vv.x - pv.x, vv.y - pv.y);
            p.skybox = None;
        }
    }

    /// Things that set sky portals: the level's default `SkyViewpoint`, EE
    /// `SkyCamCompat` skyboxes (`SpawnSkybox`).
    fn setup_skyboxes(&mut self) {
        let map = self.map;
        for (ti, t) in map.things.iter().enumerate() {
            if t.type_id == THING_SKY_VIEWPOINT {
                let pt = portal_thing(map, ti);
                self.t.things.insert(ti, pt);
                if t.tid == 0 && self.t.sectors[0].skybox.is_none() {
                    self.t.sectors[0].skybox = Some(ti);
                    self.t.sectors[0].destination = pt.sector;
                }
            }
        }
        for (ti, t) in map.things.iter().enumerate() {
            if t.type_id != THING_SKY_CAM_COMPAT && t.type_id != THING_SKY_CAM_COMPAT_EE {
                continue;
            }
            let pt = portal_thing(map, ti);
            self.t.things.insert(ti, pt);
            let Some(si) = pt.sector else {
                continue;
            };
            let refline = map.sectors[si.raw() as usize]
                .lines
                .iter()
                .find_map(|&lid| {
                    let l = map.lines.get(lid.raw() as usize)?;
                    (l.special == SECTOR_SET_PORTAL && l.args[1] == 2).then_some(l)
                });
            if let Some(refline) = refline {
                let pnum = self.skybox_portal(ti, false);
                self.copy_portal(refline.args[0], refline.args[2], pnum, 0.0, true);
            }
        }
    }

    /// `GetSkyboxPortal`.
    fn skybox_portal(&mut self, thing: usize, sky_flat_only: bool) -> u32 {
        if let Some(i) = self.t.sectors.iter().position(|p| p.skybox == Some(thing)) {
            return i as u32;
        }
        let dest = self.t.things.get(&thing).and_then(|p| p.sector);
        self.new_sector_portal(SectorPortal {
            kind: PORTS_SKYVIEWPOINT,
            flags: if sky_flat_only { PORTSF_SKYFLATONLY } else { 0 },
            partner: None,
            plane: 0,
            origin: None,
            destination: dest,
            displacement: (0.0, 0.0),
            plane_z: f64::MAX,
            skybox: Some(thing),
        })
    }

    /// `SkyPicker` (`PostBeginPlay`: runs after the line specials).
    fn setup_sky_pickers(&mut self) {
        let map = self.map;
        for (ti, t) in map.things.iter().enumerate() {
            if t.type_id != THING_SKY_PICKER {
                continue;
            }
            let pt = portal_thing(map, ti);
            let Some(si) = pt.sector.map(|s| s.raw() as usize) else {
                continue;
            };
            let boxindex = if t.args[0] == 0 {
                1
            } else {
                let Some(vi) = map.things.iter().position(|o| {
                    o.type_id == THING_SKY_VIEWPOINT && i32::from(o.tid) == t.args[0]
                }) else {
                    continue;
                };
                self.skybox_portal(vi, true)
            };
            if t.args[1] & 2 == 0 && self.portal_kind(si, PLANE_CEILING) == PORTS_SKYVIEWPOINT {
                self.t.sector_planes[si][PLANE_CEILING] = boxindex;
            }
            if t.args[1] & 1 == 0 && self.portal_kind(si, PLANE_FLOOR) == PORTS_SKYVIEWPOINT {
                self.t.sector_planes[si][PLANE_FLOOR] = boxindex;
            }
        }
    }

    /// `SpawnLinePortal`.
    fn spawn_line_portal(&mut self, li: usize) {
        let map = self.map;
        let line = &map.lines[li];
        let lid = LineId::from_raw(li as u32);
        let quick = line.special == LINE_QUICK_PORTAL;
        if (0..=i32::from(PORTT_LINKED)).contains(&line.args[2]) || quick {
            let kind = if !quick {
                line.args[2] as u8
            } else if line.args[0] == 0 {
                PORTT_LINKED
            } else {
                PORTT_VISUAL
            };
            let tag = if quick {
                first_line_id(map, line)
            } else {
                line.args[0]
            };
            let dst = find_destination(map, li, tag, quick.then_some(LINE_QUICK_PORTAL));
            let mut p = LinePortal::new(lid, dst, kind);
            if kind == PORTT_LINKED {
                p.align = PORG_ABSOLUTE;
            } else {
                let f = if quick { 0 } else { line.args[3] };
                p.align = if (0..=2).contains(&f) {
                    f as u8
                } else {
                    PORG_ABSOLUTE
                };
                if kind == PORTT_INTERACTIVE && p.align != PORG_ABSOLUTE {
                    // z-offsetting is not allowed for interactive portals.
                    p.kind = PORTT_TELEPORT;
                }
            }
            p.def_flags = match p.kind {
                PORTT_VISUAL => PORTF_VISIBLE,
                PORTT_TELEPORT => PORTF_TYPETELEPORT,
                _ => PORTF_TYPEINTERACTIVE,
            };
            self.push_line_portal(li, p);
        } else if line.args[2] == i32::from(PORTT_LINKEDEE) && line.args[0] == 0 {
            let mytag = first_line_id(map, line);
            for (lj, ln) in map.lines.iter().enumerate() {
                if first_line_id(map, ln) == mytag
                    && ln.args[0] == 1
                    && ln.special == LINE_SET_PORTAL
                {
                    let mut a =
                        LinePortal::new(lid, Some(LineId::from_raw(lj as u32)), PORTT_LINKED);
                    a.def_flags = PORTF_TYPEINTERACTIVE;
                    self.push_line_portal(li, a);
                    let mut b =
                        LinePortal::new(LineId::from_raw(lj as u32), Some(lid), PORTT_LINKED);
                    b.def_flags = PORTF_TYPEINTERACTIVE;
                    self.push_line_portal(lj, b);
                }
            }
        }
    }

    fn push_line_portal(&mut self, li: usize, p: LinePortal) {
        self.t.lines.push(p);
        self.t.line_portal[li] = Some((self.t.lines.len() - 1) as u32);
    }

    fn portal_destination_of(&self, line: LineId) -> Option<LineId> {
        self.t.line(line).and_then(|p| p.target)
    }

    /// `UpdatePortal` + `SetPortalRotation`.
    fn update_line_portal(&mut self, i: usize) {
        let map = self.map;
        let mut p = self.t.lines[i];
        if p.is_mirror() || p.is_horizon() {
            return;
        }
        let origin = &map.lines[p.line.raw() as usize];
        if p.kind != PORTT_VISUAL
            && origin.back_sector.is_none()
            && !self.poly_lines.contains(&p.line.raw())
        {
            // Traversable portals need a back sector and empty space behind.
            p.kind = PORTT_VISUAL;
            p.def_flags &= !(PORTF_PASSABLE | PORTF_SOUNDTRAVERSE | PORTF_INTERACTIVE);
        }
        match p.target {
            None => p.flags = 0,
            Some(dst) if self.portal_destination_of(dst) != Some(p.line) => {
                p.flags = p.def_flags & !PORTF_INTERACTIVE;
                if p.kind == PORTT_LINKED {
                    p.kind = PORTT_TELEPORT;
                    p.def_flags &= !PORTF_INTERACTIVE;
                }
            }
            Some(dst) => {
                p.flags = p.def_flags;
                if p.kind == PORTT_LINKED {
                    if self.t.line(dst).is_some_and(|d| d.kind != PORTT_LINKED) {
                        p.kind = PORTT_INTERACTIVE;
                    } else {
                        let d = &map.lines[dst.raw() as usize];
                        let (dx, dy) = vertex(map, d, 1);
                        let (ox, oy) = vertex(map, origin, 0);
                        p.displacement = (dx - ox, dy - oy);
                    }
                }
            }
        }
        set_rotation(map, &mut p);
        self.t.lines[i] = p;
    }

    fn collect(&mut self) {
        let map = self.map;
        // Things first (`SkyViewpoint::BeginPlay`), then `SetupPortals`
        // (stack things), `SkyCamCompat`, the line specials and last the
        // sky pickers, in UZDoom's order.
        self.setup_skyboxes_default_only();
        self.setup_stack_points();
        self.setup_skyboxes();
        for (li, line) in map.lines.iter().enumerate() {
            match line.special {
                SECTOR_SET_PORTAL => self.sector_set_portal(li, line),
                LINE_SET_PORTAL | LINE_QUICK_PORTAL => self.spawn_line_portal(li),
                LINE_HORIZON => {
                    let p = LinePortal::new(LineId::from_raw(li as u32), None, PORT_KIND_HORIZON);
                    self.push_line_portal(li, p);
                }
                LINE_MIRROR => {
                    let p = LinePortal::new(LineId::from_raw(li as u32), None, PORT_KIND_MIRROR);
                    self.push_line_portal(li, p);
                }
                _ => {}
            }
        }
        self.setup_sky_pickers();
        for i in 0..self.t.lines.len() {
            self.update_line_portal(i);
        }
    }

    fn setup_skyboxes_default_only(&mut self) {
        let map = self.map;
        if let Some(ti) = map
            .things
            .iter()
            .position(|t| t.type_id == THING_SKY_VIEWPOINT && t.tid == 0)
        {
            let pt = portal_thing(map, ti);
            self.t.things.insert(ti, pt);
            self.t.sectors[0].skybox = Some(ti);
            self.t.sectors[0].destination = pt.sector;
        }
    }

    fn sector_set_portal(&mut self, _li: usize, line: &Line) {
        let a = line.args;
        // Room's Build links reuse the special with their own types.
        if a[1] == i32::from(PORTAL_KIND_WATER) || a[1] == i32::from(PORTAL_KIND_FALL) {
            let kind = a[1] as u8;
            let src = line.front_sector.unwrap_or(SectorId::from_raw(0));
            let target = sectors_tagged(self.map, a[0])
                .first()
                .map_or(src, |&i| SectorId::from_raw(i as u32));
            self.t.build_links.push(BuildLink {
                sector: src,
                target,
                kind,
                plane: if kind == PORTAL_KIND_FALL {
                    0
                } else {
                    a[2].clamp(0, 1) as u8
                },
                offset: [a[3], a[4]],
                heights: if kind == PORTAL_KIND_FALL {
                    unpack_fall_heights(a[2])
                } else {
                    [0; 2]
                },
            });
            return;
        }
        if (a[1] == 0 || a[1] == 6) && a[3] == 0 {
            self.spawn_portal(line, a[0], a[2], a[4], a[1]);
        } else if a[1] == 3 || a[1] == 4 {
            let pnum = self.new_sector_portal(SectorPortal {
                kind: if a[1] == 3 {
                    PORTS_PLANE
                } else {
                    PORTS_HORIZON
                },
                flags: 0,
                partner: None,
                plane: a[2].clamp(0, 2) as u8,
                origin: line.front_sector,
                destination: None,
                displacement: (0.0, 0.0),
                plane_z: f64::MAX,
                skybox: None,
            });
            self.copy_portal(a[0], a[2], pnum, 0.0, true);
        }
    }
}

fn set_rotation(map: &MapGeometry, p: &mut LinePortal) {
    let Some(dst) = p.target.and_then(|t| map.lines.get(t.raw() as usize)) else {
        return;
    };
    if p.kind == PORTT_LINKED {
        p.angle_diff = 0.0;
        p.sin_rot = 0.0;
        p.cos_rot = 1.0;
        return;
    }
    let line = &map.lines[p.line.raw() as usize];
    let angle = line_angle_deg(dst) - line_angle_deg(line) + 180.0;
    // UZDoom uses the precise `sindeg` / `cosdeg`; `dmath` keeps every
    // platform on the same bits (netplay, demos).
    let rad = angle * (std::f64::consts::PI / 180.0);
    p.angle_diff = angle.rem_euclid(360.0);
    p.sin_rot = dmath::sin(rad);
    p.cos_rot = dmath::cos(rad);
    // Snap exact quarter turns so axis-aligned portals stay exact.
    for v in [&mut p.sin_rot, &mut p.cos_rot] {
        for target in [-1.0, 0.0, 1.0] {
            if (*v - target).abs() < 1e-12 {
                *v = target;
            }
        }
    }
}

/// Sloped planes cannot hold linked portals (`CreateLinkedPortals`).
fn plane_is_sloped(planes: &[crate::setup::SectorPlanes], sector: usize, plane: usize) -> bool {
    planes.get(sector).is_some_and(|p| {
        let sp = if plane == PLANE_FLOOR {
            p.floor
        } else {
            p.ceiling
        };
        sp.normal_x.abs() > 1e-9 || sp.normal_y.abs() > 1e-9
    })
}

/// `CollectSectors`: flood a group across two-sided lines.
fn collect_sectors(map: &MapGeometry, groups: &mut [i64], group: i64, origin: usize) -> bool {
    if groups[origin] != 0 {
        return false;
    }
    groups[origin] = group;
    let mut list = vec![origin];
    let mut i = 0;
    while i < list.len() {
        let sec = list[i];
        i += 1;
        for &lid in &map.sectors[sec].lines {
            let Some(line) = map.lines.get(lid.raw() as usize) else {
                continue;
            };
            let front = line.front_sector.map(|s| s.raw() as usize);
            let back = line.back_sector.map(|s| s.raw() as usize);
            let other = if front == Some(sec) { back } else { front };
            if let Some(o) = other
                && o != sec
                && o < groups.len()
                && groups[o] != group
            {
                groups[o] = group;
                list.push(o);
            }
        }
    }
    true
}

/// `CreateLinkedPortals` + `ConnectPortalGroups`: portal groups and the
/// displacement table. Demotes portals UZDoom would disable.
fn create_linked_portals(
    map: &MapGeometry,
    planes: &[crate::setup::SectorPlanes],
    t: &mut PortalTables,
    warnings: &mut Vec<String>,
) -> PortalGroups {
    let n = map.sectors.len();
    let mut groups = vec![0i64; n];
    // Linked plane portals on sloped planes are removed.
    for si in 0..n {
        for plane in [PLANE_FLOOR, PLANE_CEILING] {
            if t.plane_is_linked(si, plane) && plane_is_sloped(planes, si, plane) {
                t.sector_planes[si][plane] = 0;
                warnings.push(format!(
                    "portal on sector {si} plane {plane} is sloped and disabled"
                ));
            }
        }
    }
    let orgs: Vec<usize> = t
        .sectors
        .iter()
        .enumerate()
        .filter(|(_, p)| p.kind == PORTS_LINKEDPORTAL)
        .map(|(i, _)| i)
        .collect();
    let mut id: i64 = 1;
    for &oi in &orgs {
        let (o, d) = (t.sectors[oi].origin, t.sectors[oi].destination);
        if let Some(o) = o
            && collect_sectors(map, &mut groups, id, o.raw() as usize)
        {
            id += 1;
        }
        if let Some(d) = d
            && collect_sectors(map, &mut groups, id, d.raw() as usize)
        {
            id += 1;
        }
    }
    for i in 0..t.lines.len() {
        if t.lines[i].kind != PORTT_LINKED {
            continue;
        }
        let Some(dst) = t.lines[i].target else {
            let li = t.lines[i].line.raw() as usize;
            t.line_portal[li] = None;
            t.lines[i].kind = PORTT_VISUAL;
            t.lines[i].flags = 0;
            continue;
        };
        let of = map.lines[t.lines[i].line.raw() as usize].front_sector;
        let df = map.lines[dst.raw() as usize].front_sector;
        if let Some(o) = of
            && collect_sectors(map, &mut groups, id, o.raw() as usize)
        {
            id += 1;
        }
        if let Some(d) = df
            && collect_sectors(map, &mut groups, id, d.raw() as usize)
        {
            id += 1;
        }
    }
    let size = id as usize;
    let mut disp: Vec<Option<(f64, f64)>> = vec![None; size * size];
    // Leftover sectors that use a linked plane portal join its origin group.
    for si in 0..n {
        for plane in [PLANE_FLOOR, PLANE_CEILING] {
            if t.plane_is_linked(si, plane) && groups[si] == 0 {
                if let Some(o) = t.plane_portal(si, plane).and_then(|p| p.origin) {
                    let g = groups[o.raw() as usize];
                    collect_sectors(map, &mut groups, g, si);
                }
            }
        }
    }
    let group_of = |groups: &[i64], s: Option<SectorId>| s.map_or(0, |s| groups[s.raw() as usize]);
    // `AddDisplacementForPortal` (sector).
    for &oi in &orgs {
        let p = t.sectors[oi];
        let (a, b) = (
            group_of(&groups, p.origin),
            group_of(&groups, p.destination),
        );
        let bad = a == b
            || a <= 0
            || b <= 0
            || a as usize >= size
            || b as usize >= size
            || disp[a as usize + size * b as usize].is_some_and(|d| d != p.displacement);
        if bad {
            warnings.push(format!("sector portal {oi} disabled (group/displacement)"));
            t.sectors[oi].kind = PORTS_PORTAL;
            continue;
        }
        disp[a as usize + size * b as usize] = Some(p.displacement);
    }
    // `AddDisplacementForPortal` (line).
    for i in 0..t.lines.len() {
        if t.lines[i].kind != PORTT_LINKED {
            continue;
        }
        let Some(dst) = t.lines[i].target else {
            continue;
        };
        let of = map.lines[t.lines[i].line.raw() as usize].front_sector;
        let df = map.lines[dst.raw() as usize].front_sector;
        let (a, b) = (group_of(&groups, of), group_of(&groups, df));
        let d = t.lines[i].displacement;
        let bad = a == b
            || a <= 0
            || b <= 0
            || a as usize >= size
            || b as usize >= size
            || disp[a as usize + size * b as usize].is_some_and(|e| e != d);
        if bad {
            warnings.push(format!(
                "linked portal on line {} demoted to teleport (group/displacement)",
                t.lines[i].line.raw()
            ));
            t.lines[i].kind = PORTT_TELEPORT;
            set_rotation(map, &mut t.lines[i]);
            if let Some(j) = t.line_portal[dst.raw() as usize] {
                t.lines[j as usize].kind = PORTT_TELEPORT;
                let mut q = t.lines[j as usize];
                set_rotation(map, &mut q);
                t.lines[j as usize] = q;
            }
            continue;
        }
        disp[a as usize + size * b as usize] = Some(d);
    }
    // `ConnectPortalGroups`: indirect connections.
    loop {
        let mut changed = false;
        for x in 1..size {
            for y in 1..size {
                let Some(xy) = disp[x + size * y] else {
                    continue;
                };
                for z in 1..size {
                    let Some(yz) = disp[y + size * z] else {
                        continue;
                    };
                    if disp[x + size * z].is_none() {
                        disp[x + size * z] = Some((xy.0 + yz.0, xy.1 + yz.1));
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Plane flags: obstructed planes, the linked flag, and a ceiling portal
    // below the floor portal of the same sector.
    for si in 0..n {
        for plane in [PLANE_FLOOR, PLANE_CEILING] {
            if t.plane_is_linked(si, plane) {
                let portalh = t.plane_z(si, plane);
                let s = &map.sectors[si];
                let planeh = if plane == PLANE_FLOOR {
                    s.floor_height
                } else {
                    s.ceiling_height
                };
                let obstructed = if plane == PLANE_FLOOR {
                    planeh > portalh
                } else {
                    planeh < portalh
                };
                if obstructed {
                    t.plane_flags[si][plane] |= PLANEF_OBSTRUCTED;
                }
            }
        }
        if t.plane_is_linked(si, PLANE_CEILING) && t.plane_is_linked(si, PLANE_FLOOR) {
            let cz = t.plane_z(si, PLANE_CEILING);
            let fz = t.plane_z(si, PLANE_FLOOR);
            if cz < fz {
                let s = &map.sectors[si];
                if s.ceiling_height < s.floor_height || fz == s.floor_height {
                    t.sector_planes[si][PLANE_CEILING] = 0;
                } else {
                    t.sector_planes[si][PLANE_FLOOR] = 0;
                }
                warnings.push(format!("sector {si}: ceiling portal below floor portal"));
            }
        }
        for plane in [PLANE_FLOOR, PLANE_CEILING] {
            if t.plane_is_linked(si, plane) {
                t.plane_flags[si][plane] |= PLANEF_LINKED;
            }
        }
    }
    PortalGroups {
        sector_group: groups.iter().map(|&g| g.max(0) as u32).collect(),
        size: size.max(1),
        disp: if size > 1 { disp } else { vec![None] },
    }
}

/// Build all portal tables and groups for a map.
#[must_use]
pub fn build_portals(
    map: &MapGeometry,
    planes: &[crate::setup::SectorPlanes],
) -> (PortalTables, PortalGroups, Vec<String>) {
    let n = map.sectors.len();
    let poly_lines = crate::setup::collect_polyobjects(map)
        .iter()
        .flat_map(|po| po.lines.iter().map(|l| l.raw()))
        .collect();
    let mut b = Builder {
        map,
        poly_lines,
        t: PortalTables {
            lines: Vec::new(),
            line_portal: vec![None; map.lines.len()],
            sectors: vec![SectorPortal::sky(), SectorPortal::sky()],
            sector_planes: vec![[0, 0]; n],
            plane_flags: vec![[0, 0]; n],
            plane_alpha: map
                .sectors
                .iter()
                .map(|s| [s.floor_alpha.min(1.0), s.ceiling_alpha.min(1.0)])
                .collect(),
            line_copy: vec![None; map.lines.len()],
            things: BTreeMap::new(),
            build_links: Vec::new(),
            has_passable_lines: false,
            has_linked_planes: false,
            revision: 0,
        },
    };
    b.collect();
    let mut t = b.t;
    let mut warnings = Vec::new();
    let groups = create_linked_portals(map, planes, &mut t, &mut warnings);
    t.has_passable_lines = t
        .lines
        .iter()
        .any(|p| p.is_traversable() && p.target.is_some());
    t.has_linked_planes = t
        .plane_flags
        .iter()
        .any(|f| f.iter().any(|b| b & PLANEF_LINKED != 0));
    (t, groups, warnings)
}

/// `FLevelLocals::ChangePortalLine`: point a line's portal at the line with
/// id `destid` (0 switches it off). Linked portals cannot change.
pub fn change_portal_line(map: &mut MapGeometry, line: usize, destid: i32) -> bool {
    let Some(pi) = map.setup.portals.line_portal.get(line).copied().flatten() else {
        return false;
    };
    let pi = pi as usize;
    let mut p = map.setup.portals.lines[pi];
    if p.kind == PORTT_LINKED || p.is_mirror() || p.is_horizon() {
        return false;
    }
    p.target = find_destination(map, line, destid, None);
    let mut other: Option<(usize, LinePortal)> = None;
    match p.target {
        None => p.flags = 0,
        Some(dst) if p.kind == PORTT_INTERACTIVE => {
            let back = map.setup.portals.line_portal[dst.raw() as usize]
                .map(|j| (j as usize, map.setup.portals.lines[j as usize]));
            match back {
                Some((j, mut d)) if d.kind == PORTT_INTERACTIVE && d.target == Some(p.line) => {
                    p.flags = p.def_flags | PORTF_INTERACTIVE;
                    d.flags = d.def_flags | PORTF_INTERACTIVE;
                    set_rotation(map, &mut d);
                    other = Some((j, d));
                }
                _ => p.flags = p.def_flags,
            }
        }
        Some(_) => p.flags = p.def_flags,
    }
    set_rotation(map, &mut p);
    let t = &mut map.setup.portals;
    t.lines[pi] = p;
    if let Some((j, d)) = other {
        t.lines[j] = d;
    }
    t.has_passable_lines = t
        .lines
        .iter()
        .any(|q| q.is_traversable() && q.target.is_some());
    t.revision = t.revision.wrapping_add(1);
    true
}

/// `FLevelLocals::ChangePortal`: every line with id `thisid` (or just
/// `line` when `thisid` is 0).
pub fn change_portal(map: &mut MapGeometry, line: Option<usize>, thisid: i32, destid: i32) -> bool {
    if thisid == 0 {
        return line.is_some_and(|l| change_portal_line(map, l, destid));
    }
    let lines: Vec<usize> = map
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| line_has_id(map, l, thisid))
        .map(|(i, _)| i)
        .collect();
    let mut res = false;
    for l in lines {
        res |= change_portal_line(map, l, destid);
    }
    res
}

/// UZDoom `FPolyObj::bHasPortals` for a polyobject's lines: 0 none, 1 a
/// passable portal, 2 a linked one (those polyobjects may not rotate).
#[must_use]
pub fn poly_portal_kind(map: &MapGeometry, lines: &[LineId]) -> u8 {
    let t = &map.setup.portals;
    lines
        .iter()
        .filter_map(|&l| t.line(l))
        .filter(|p| p.def_flags & PORTF_PASSABLE != 0)
        .map(|p| if p.kind == PORTT_LINKED { 2 } else { 1 })
        .max()
        .unwrap_or(0)
}

/// After a polyobject moved its lines: `FPolyObj::UpdateLinks` (linked
/// portals recompute their displacement, the partner takes the opposite,
/// and the destination group moves in the displacement table), plus the
/// rotation of the other portals on or aimed at those lines, so a turning
/// polyobject's view and crossings turn with it (UZDoom keeps the angle
/// from map load).
pub fn update_poly_portals(map: &mut MapGeometry, lines: &[LineId]) {
    let moved: std::collections::BTreeSet<LineId> = lines.iter().copied().collect();
    let mut changed = false;
    let mut moved_groups: Vec<u32> = Vec::new();
    for i in 0..map.setup.portals.lines.len() {
        let mut p = map.setup.portals.lines[i];
        let Some(target) = p.target else {
            continue;
        };
        if !moved.contains(&p.line) && !moved.contains(&target) {
            continue;
        }
        if p.kind == PORTT_LINKED {
            // Only the portal on the polyobject drives (UZDoom walks the
            // polyobject's own lines).
            if !moved.contains(&p.line) {
                continue;
            }
            let (Some(src), Some(dst)) = (
                map.lines.get(p.line.raw() as usize),
                map.lines.get(target.raw() as usize),
            ) else {
                continue;
            };
            let o1 = map.vertices[src.v1.raw() as usize].pos;
            let d2 = map.vertices[dst.v2.raw() as usize].pos;
            let disp = (d2.x() - o1.x(), d2.y() - o1.y());
            let old = p.displacement;
            if disp == old {
                continue;
            }
            let dest_group = dst
                .front_sector
                .and_then(|s| map.setup.portal_groups.group_of(s.raw() as usize))
                .unwrap_or(0);
            p.displacement = disp;
            map.setup.portals.lines[i] = p;
            if let Some(j) = map.setup.portals.line_portal[target.raw() as usize] {
                let q = &mut map.setup.portals.lines[j as usize];
                if q.target == Some(p.line) {
                    q.displacement = (-disp.0, -disp.1);
                }
            }
            if dest_group != 0 && !moved_groups.contains(&dest_group) {
                moved_groups.push(dest_group);
                map.setup
                    .portal_groups
                    .move_group(dest_group, (disp.0 - old.0, disp.1 - old.1));
            }
            changed = true;
        } else {
            let before = (p.sin_rot, p.cos_rot);
            set_rotation(map, &mut p);
            if (p.sin_rot, p.cos_rot) != before {
                map.setup.portals.lines[i] = p;
            }
            // Positions follow the vertices either way; renderers re-read.
            changed = true;
        }
    }
    if changed {
        let t = &mut map.setup.portals;
        t.revision = t.revision.wrapping_add(1);
    }
}

/// Copy what the playsim changes at run time from the live map into a
/// shell's render copy: polyobject vertex positions (and line deltas) and,
/// when they changed (`revision`), the portal tables and the displacement
/// table. Cheap when nothing moved.
pub fn sync_dynamic_geometry(dst: &mut MapGeometry, src: &MapGeometry) {
    if dst.vertices.len() == src.vertices.len() && dst.lines.len() == src.lines.len() {
        for po in &src.setup.polyobjects {
            for &lid in &po.lines {
                let i = lid.raw() as usize;
                let Some(line) = src.lines.get(i) else {
                    continue;
                };
                for v in [line.v1, line.v2] {
                    let vi = v.raw() as usize;
                    if let (Some(d), Some(s)) = (dst.vertices.get_mut(vi), src.vertices.get(vi)) {
                        d.pos = s.pos;
                    }
                }
                if let Some(d) = dst.lines.get_mut(i) {
                    d.delta = line.delta;
                }
            }
        }
    }
    if dst.setup.portals.revision != src.setup.portals.revision
        && dst.setup.portals.lines.len() == src.setup.portals.lines.len()
    {
        dst.setup.portals = src.setup.portals.clone();
        dst.setup.portal_groups = src.setup.portal_groups.clone();
    }
}

/// UDMF `portal_floor_*` / `portal_ceil_*` keys (`blocksound`, `disabled`,
/// `nopass`, `norender`) into [`PortalTables::plane_flags`].
pub fn apply_udmf_plane_flags(t: &mut PortalTables, user: &crate::udmf::UdmfUserStore) {
    use crate::udmf::ext;
    for (si, flags) in t.plane_flags.iter_mut().enumerate() {
        let e = user.sector_ext(si);
        let map_bits = |shift: u32| -> u16 {
            let mut f = 0;
            if e & (ext::SECTOR_PORTAL_FLOOR_BLOCKSOUND << shift) != 0 {
                f |= PLANEF_BLOCKSOUND;
            }
            if e & (ext::SECTOR_PORTAL_FLOOR_DISABLED << shift) != 0 {
                f |= PLANEF_DISABLED;
            }
            if e & (ext::SECTOR_PORTAL_FLOOR_NOPASS << shift) != 0 {
                f |= PLANEF_NOPASS;
            }
            if e & (ext::SECTOR_PORTAL_FLOOR_NORENDER << shift) != 0 {
                f |= PLANEF_NORENDER;
            }
            f
        };
        flags[PLANE_FLOOR] |= map_bits(0);
        flags[PLANE_CEILING] |= map_bits(4);
    }
}

#[cfg(test)]
mod tests;
