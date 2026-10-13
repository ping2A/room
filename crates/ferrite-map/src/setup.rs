//! Load-time geometry setup: slopes, 3D floors, portals, polyobjects.
//!
//! Applies UDMF planes, `Plane_Align`, slope things (9500/9501), and collects
//! polyobject / portal / 3D-floor declarations. Portal *groups* / displacements
//! and full GZDoom spawn ordering land with M7.

use ferrite_core::{LineId, SectorId, VertexId};
use std::collections::{BTreeMap, BTreeSet};

/// Aggregated spawn-time geometry extras.
#[derive(Clone, Debug, Default)]
pub struct SetupData {
    /// Sector floor/ceiling planes (slopes applied).
    pub planes: Vec<SectorPlanes>,
    /// Static polyobject definitions.
    pub polyobjects: Vec<PolyobjStatic>,
    /// Line / sector portal tables.
    pub portals: PortalTables,
    /// Linked-portal groups and 2D displacements.
    pub portal_groups: PortalGroups,
    /// 3D floor definitions attached to sectors.
    pub ffloors: Vec<FFloorStatic>,
    /// `Sector_Attach3dMidtex` links (line indices attached to a sector plane).
    pub midtex_attach: Vec<MidtexAttach>,
}

/// One `Sector_Attach3dMidtex` binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidtexAttach {
    /// Sector whose floor/ceiling movers drag the midtex.
    pub sector: SectorId,
    /// Attached linedef indices.
    pub lines: Vec<u32>,
    /// `true` = ceiling plane; `false` = floor.
    pub ceiling: bool,
}

/// Initial floor + ceiling planes for one sector.
#[derive(Clone, Copy, Debug)]
pub struct SectorPlanes {
    /// Floor.
    pub floor: SecPlane,
    /// Ceiling.
    pub ceiling: SecPlane,
    /// Load-time sector `floor_height` (immutable reference for mover deltas).
    pub base_floor: f64,
    /// Load-time sector `ceiling_height`.
    pub base_ceiling: f64,
}

impl SectorPlanes {
    /// Horizontal planes at the given heights (normal = +Z for floor).
    #[must_use]
    pub fn flat(floor_z: f64, ceil_z: f64) -> Self {
        Self {
            floor: SecPlane::horizontal(floor_z, true),
            ceiling: SecPlane::horizontal(ceil_z, false),
            base_floor: floor_z,
            base_ceiling: ceil_z,
        }
    }
}

/// Sector plane (`secplane_t`-compatible).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecPlane {
    /// Plane normal (unit-ish; GZDoom stores raw).
    pub normal_x: f64,
    /// Y.
    pub normal_y: f64,
    /// Z (positive up for floors).
    pub normal_z: f64,
    /// Plane constant `d` in `nx*x + ny*y + nz*z = d`.
    pub d: f64,
    /// Cached `-1/nz` for height queries (0 if vertical).
    pub neg_inv_c: f64,
}

impl SecPlane {
    /// Flat horizontal plane at height `z`.
    #[must_use]
    pub fn horizontal(z: f64, floor: bool) -> Self {
        let nz = if floor { 1.0 } else { -1.0 };
        Self {
            normal_x: 0.0,
            normal_y: 0.0,
            normal_z: nz,
            d: nz * z,
            neg_inv_c: -1.0 / nz,
        }
    }

    /// Plane from UDMF / GZDoom `a*x + b*y + c*z = d` coefficients.
    #[must_use]
    pub fn from_abcd(a: f64, b: f64, c: f64, d: f64) -> Self {
        let neg_inv_c = if c.abs() > 1e-12 { -1.0 / c } else { 0.0 };
        Self {
            normal_x: a,
            normal_y: b,
            normal_z: c,
            d,
            neg_inv_c,
        }
    }

    /// Plane through three non-colinear points.
    #[must_use]
    pub fn from_points(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]) -> Option<Self> {
        let ux = p1[0] - p0[0];
        let uy = p1[1] - p0[1];
        let uz = p1[2] - p0[2];
        let vx = p2[0] - p0[0];
        let vy = p2[1] - p0[1];
        let vz = p2[2] - p0[2];
        let nx = uy * vz - uz * vy;
        let ny = uz * vx - ux * vz;
        let nz = ux * vy - uy * vx;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len < 1e-12 {
            return None;
        }
        let (nx, ny, nz) = (nx / len, ny / len, nz / len);
        let d = nx * p0[0] + ny * p0[1] + nz * p0[2];
        Some(Self::from_abcd(nx, ny, nz, d))
    }

    /// Height of this plane at `(x, y)`.
    #[must_use]
    pub fn z_at(self, x: f64, y: f64) -> f64 {
        if self.normal_z == 0.0 {
            return 0.0;
        }
        (self.d - self.normal_x * x - self.normal_y * y) / self.normal_z
    }

    /// True when the plane is not horizontal.
    #[must_use]
    pub fn is_slope(self) -> bool {
        self.normal_x != 0.0 || self.normal_y != 0.0
    }
}

/// Static polyobject (vertices + lines; movers live in sim).
#[derive(Clone, Debug)]
pub struct PolyobjStatic {
    /// Tag / poly id.
    pub id: i32,
    /// Original vertex indices (copied on spawn into a movable set).
    pub vertices: Vec<VertexId>,
    /// Lines belonging to the polyobject.
    pub lines: Vec<LineId>,
    /// Mirror poly id (`0` = none).
    pub mirror: i32,
    /// Crushing.
    pub crush: bool,
}

/// Portal lookup tables (line + sector).
#[derive(Clone, Debug, Default)]
pub struct PortalTables {
    /// Line portals.
    pub lines: Vec<LinePortal>,
    /// Sector (plane/sky) portals.
    pub sectors: Vec<SectorPortal>,
}

/// One line portal.
#[derive(Clone, Copy, Debug)]
pub struct LinePortal {
    /// Source line.
    pub line: LineId,
    /// Destination line (or None for horizon/mirror).
    pub target: Option<LineId>,
    /// Portal type (0 visual … 3 linked; 182 mirror; 9 horizon).
    pub kind: u8,
}

impl LinePortal {
    /// Actors may walk through (teleporter / interactive / linked).
    #[must_use]
    pub fn is_traversable(self) -> bool {
        matches!(self.kind, 1..=3)
    }

    /// `Line_Mirror` (special 182) — reflective surface (render recursion later).
    #[must_use]
    pub fn is_mirror(self) -> bool {
        self.kind == 182
    }

    /// Do not emit a blocking wall (visual hole or walk-through).
    ///
    /// Mirrors keep their wall (drawn as reflective); only portal holes skip.
    #[must_use]
    pub fn skip_wall(self) -> bool {
        self.kind <= 3
    }
}

/// Sector portal-group ids plus displacements between groups.
#[derive(Clone, Debug, Default)]
pub struct PortalGroups {
    /// Group id per sector index.
    pub sector_group: Vec<u32>,
    /// Add this XY to a point in `from` to express it in `to` (`(from, to)`).
    pub delta: BTreeMap<(u32, u32), (f64, f64)>,
}

impl PortalGroups {
    /// Group for a sector index.
    #[must_use]
    pub fn group_of(&self, sector: usize) -> Option<u32> {
        self.sector_group.get(sector).copied()
    }
}

/// One sector portal / skybox link.
#[derive(Clone, Copy, Debug)]
pub struct SectorPortal {
    /// Source sector.
    pub sector: SectorId,
    /// Destination sector.
    pub target: SectorId,
    /// Kind (0 visual, 1 teleporter, 3 linked, …).
    pub kind: u8,
    /// `0` = floor plane, `1` = ceiling plane.
    pub plane: u8,
}

impl SectorPortal {
    /// Actors may pass through this plane (teleporter / linked).
    #[must_use]
    pub fn is_traversable(self) -> bool {
        matches!(self.kind, 1 | 3)
    }
}

/// Static 3D floor attached to a control sector.
#[derive(Clone, Copy, Debug)]
pub struct FFloorStatic {
    /// Target sector that receives the 3D floor.
    pub target: SectorId,
    /// Control sector defining the slab.
    pub control: SectorId,
    /// `Sector_Set3DFloor` type (0 Vavoom, 1 solid, 2 swimmable, 3 non-solid).
    pub typ: i32,
    /// Extra flags (`FF_*`) — raw.
    pub flags: u32,
    /// Opacity 0..=255 from special arg3 (`0` = invisible / no planes in GZDoom).
    pub alpha: u8,
}

impl FFloorStatic {
    /// Solid collision slab (Vavoom / type-1). Swimmable and non-solid skip blocking.
    #[must_use]
    pub fn is_solid(self) -> bool {
        matches!(self.typ, 0 | 1)
    }

    /// Swimmable water slab (`Sector_Set3DFloor` type 2).
    #[must_use]
    pub fn is_swimmable(self) -> bool {
        self.typ == 2
    }

    /// Non-solid / light-only / see-through (`Sector_Set3DFloor` type 3, `ExtraFloor_LightOnly`).
    #[must_use]
    pub fn is_light_transfer(self) -> bool {
        self.typ == 3
    }

    /// Plane/side opacity 0..=1 (`alpha` 0 → hidden).
    #[must_use]
    pub fn opacity(self) -> f32 {
        f32::from(self.alpha) / 255.0
    }
}

// Hexen-style special numbers (GZDoom actionspecials.h).
const POLYOBJ_START_LINE: i16 = 1;
const POLYOBJ_EXPLICIT_LINE: i16 = 5;
const LINE_HORIZON: i16 = 9;
const SECTOR_SET_PORTAL: i16 = 57;
const LINE_SET_PORTAL: i16 = 156;
const EXTRA_FLOOR_LIGHT_ONLY: i16 = 50;
const SECTOR_ATTACH_3D_MIDTEX: i16 = 51;
const SECTOR_SET_3DFLOOR: i16 = 160;
const LINE_MIRROR: i16 = 182;
const PLANE_ALIGN: i16 = 181;

/// Thing editor numbers for polyobject anchors / spawn spots.
const PO_ANCHOR: i16 = 9300;
const PO_SPAWN: i16 = 9301;
const PO_SPAWN_CRUSH: i16 = 9302;
const PO_SPAWN_HURT: i16 = 9303;
const PO_ANCHOR_HEXEN: i16 = 3000;
const PO_SPAWN_HEXEN: i16 = 3001;
const PO_SPAWN_CRUSH_HEXEN: i16 = 3002;

/// Thing editor numbers: slope floor/ceiling to a line id (`args[0]`).
const SLOPE_FLOOR_POINT_LINE: i16 = 9500;
const SLOPE_CEILING_POINT_LINE: i16 = 9501;

/// Build setup tables: planes (UDMF / Plane_Align / slope things) + poly/portal/3D-floor.
#[must_use]
pub fn build_setup(map: &crate::geom::MapGeometry) -> SetupData {
    let mut planes: Vec<SectorPlanes> = map
        .sectors
        .iter()
        .map(|s| {
            let mut sp = SectorPlanes::flat(s.floor_height, s.ceiling_height);
            if let Some([a, b, c, d]) = s.floor_plane {
                sp.floor = SecPlane::from_abcd(a, b, c, d);
            }
            if let Some([a, b, c, d]) = s.ceiling_plane {
                sp.ceiling = SecPlane::from_abcd(a, b, c, d);
            }
            sp
        })
        .collect();
    apply_plane_align(map, &mut planes);
    apply_slope_point_line_things(map, &mut planes);
    let polyobjects = collect_polyobjects(map);
    let portals = collect_portals(map);
    let portal_groups = build_portal_groups(map, &portals);
    let ffloors = collect_ffloors(map);
    let midtex_attach = collect_midtex_attach(map);
    SetupData {
        planes,
        polyobjects,
        portals,
        portal_groups,
        ffloors,
        midtex_attach,
    }
}

/// Apply `Sector_Attach3dMidtex` (51): mark matching two-sided lines `midtex3d`.
///
/// Call on a mutable map **before** [`build_setup`] so collision sees the flag.
pub fn apply_attach_3d_midtex(map: &mut crate::geom::MapGeometry) {
    let attaches: Vec<(i32, i32)> = map
        .lines
        .iter()
        .filter(|l| l.special == SECTOR_ATTACH_3D_MIDTEX)
        .map(|l| (l.args[0], l.args[1]))
        .collect();
    if attaches.is_empty() {
        return;
    }
    let sector_tags: Vec<Vec<i32>> = map
        .sectors
        .iter()
        .map(|s| s.tags.iter().copied().collect())
        .collect();
    for (line_id, tag) in attaches {
        for line in &mut map.lines {
            if line.front_sector.is_none() || line.back_sector.is_none() {
                continue;
            }
            let id_ok = line_id == 0 || line.has_id(line_id);
            if !id_ok {
                continue;
            }
            if tag != 0 {
                let touches = [line.front_sector, line.back_sector]
                    .into_iter()
                    .flatten()
                    .any(|s| {
                        sector_tags
                            .get(s.raw() as usize)
                            .is_some_and(|tags| tags.contains(&tag))
                    });
                if !touches {
                    continue;
                }
            } else if line_id == 0 {
                continue;
            }
            line.midtex3d = true;
        }
    }
}

fn collect_midtex_attach(map: &crate::geom::MapGeometry) -> Vec<MidtexAttach> {
    let mut out = Vec::new();
    for line in &map.lines {
        if line.special != SECTOR_ATTACH_3D_MIDTEX {
            continue;
        }
        let Some(sector) = line.front_sector else {
            continue;
        };
        let line_id = line.args[0];
        let tag = line.args[1];
        let ceiling = line.args[2] != 0;
        let mut lines = Vec::new();
        for (i, cand) in map.lines.iter().enumerate() {
            if !cand.midtex3d || cand.front_sector.is_none() || cand.back_sector.is_none() {
                continue;
            }
            let id_ok = line_id == 0 || cand.has_id(line_id);
            if !id_ok {
                continue;
            }
            if tag != 0 {
                let touches = [cand.front_sector, cand.back_sector]
                    .into_iter()
                    .flatten()
                    .any(|s| {
                        map.sectors
                            .get(s.raw() as usize)
                            .is_some_and(|sec| sec.tags.contains(&tag))
                    });
                if !touches {
                    continue;
                }
            } else if line_id == 0 {
                continue;
            }
            lines.push(i as u32);
        }
        if !lines.is_empty() {
            out.push(MidtexAttach {
                sector,
                lines,
                ceiling,
            });
        }
    }
    out
}

/// `9500` / `9501`: slope the sector containing the thing through a tagged linedef.
fn apply_slope_point_line_things(map: &crate::geom::MapGeometry, planes: &mut [SectorPlanes]) {
    for th in &map.things {
        let floor = match th.type_id {
            SLOPE_FLOOR_POINT_LINE => true,
            SLOPE_CEILING_POINT_LINE => false,
            _ => continue,
        };
        let line_id = th.args[0];
        if line_id == 0 {
            continue;
        }
        let Some(line) = map.lines.iter().find(|l| l.has_id(line_id)) else {
            continue;
        };
        let Some(si) = sector_containing(map, f64::from(th.x), f64::from(th.y)) else {
            continue;
        };
        if si >= planes.len() {
            continue;
        }
        let (Some(v1), Some(v2)) = (
            map.vertices.get(line.v1.raw() as usize),
            map.vertices.get(line.v2.raw() as usize),
        ) else {
            continue;
        };
        let hinge_z = if floor {
            map.sectors[si].floor_height
        } else {
            map.sectors[si].ceiling_height
        };
        let tip_z = hinge_z + f64::from(th.z);
        let Some(plane) = SecPlane::from_points(
            [v1.pos.x(), v1.pos.y(), hinge_z],
            [v2.pos.x(), v2.pos.y(), hinge_z],
            [f64::from(th.x), f64::from(th.y), tip_z],
        ) else {
            continue;
        };
        if floor {
            planes[si].floor = plane;
        } else {
            planes[si].ceiling = plane;
        }
    }
}

/// Even-odd point-in-sector using the sector's bounding linedefs (setup-time, no nodes).
fn sector_containing(map: &crate::geom::MapGeometry, x: f64, y: f64) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (si, sec) in map.sectors.iter().enumerate() {
        let mut inside = false;
        let mut area = 0.0;
        for &lid in &sec.lines {
            let Some(line) = map.lines.get(lid.raw() as usize) else {
                continue;
            };
            let (Some(a), Some(b)) = (
                map.vertices.get(line.v1.raw() as usize),
                map.vertices.get(line.v2.raw() as usize),
            ) else {
                continue;
            };
            let (x1, y1, x2, y2) = (a.pos.x(), a.pos.y(), b.pos.x(), b.pos.y());
            area += x1 * y2 - x2 * y1;
            if (y1 > y) != (y2 > y) {
                let x_cross = (x2 - x1) * (y - y1) / (y2 - y1) + x1;
                if x < x_cross {
                    inside = !inside;
                }
            }
        }
        if inside {
            let a = area.abs();
            if best.is_none_or(|(_, ba)| a < ba) {
                best = Some((si, a));
            }
        }
    }
    best.map(|(si, _)| si)
}

/// `Plane_Align` (181): slope a sector plane to the neighbour along a linedef hinge.
fn apply_plane_align(map: &crate::geom::MapGeometry, planes: &mut [SectorPlanes]) {
    for line in &map.lines {
        if line.special != PLANE_ALIGN {
            continue;
        }
        let (Some(front), Some(back)) = (line.front_sector, line.back_sector) else {
            continue;
        };
        let (fi, bi) = (front.raw() as usize, back.raw() as usize);
        if fi >= planes.len() || bi >= planes.len() {
            continue;
        }
        let (Some(v1), Some(v2)) = (
            map.vertices.get(line.v1.raw() as usize),
            map.vertices.get(line.v2.raw() as usize),
        ) else {
            continue;
        };
        // Perpendicular offset into the aligning sector (map units).
        let px = -(v2.pos.y() - v1.pos.y());
        let py = v2.pos.x() - v1.pos.x();
        let plen = (px * px + py * py).sqrt().max(1e-9);
        let (px, py) = (px / plen * 32.0, py / plen * 32.0);
        let mid_x = (v1.pos.x() + v2.pos.x()) * 0.5;
        let mid_y = (v1.pos.y() + v2.pos.y()) * 0.5;

        align_one(
            planes,
            line.args[0],
            fi,
            bi,
            true,
            v1.pos.x(),
            v1.pos.y(),
            v2.pos.x(),
            v2.pos.y(),
            mid_x,
            mid_y,
            px,
            py,
            map.sectors[fi].floor_height,
            map.sectors[bi].floor_height,
        );
        align_one(
            planes,
            line.args[1],
            fi,
            bi,
            false,
            v1.pos.x(),
            v1.pos.y(),
            v2.pos.x(),
            v2.pos.y(),
            mid_x,
            mid_y,
            px,
            py,
            map.sectors[fi].ceiling_height,
            map.sectors[bi].ceiling_height,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn align_one(
    planes: &mut [SectorPlanes],
    mode: i32,
    fi: usize,
    bi: usize,
    floor: bool,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    mid_x: f64,
    mid_y: f64,
    px: f64,
    py: f64,
    front_z: f64,
    back_z: f64,
) {
    let (target, hinge_z, own_z, into) = match mode {
        1 => (fi, back_z, front_z, (px, py)),
        2 => (bi, front_z, back_z, (-px, -py)),
        _ => return,
    };
    let Some(plane) = SecPlane::from_points(
        [x1, y1, hinge_z],
        [x2, y2, hinge_z],
        [mid_x + into.0, mid_y + into.1, own_z],
    ) else {
        return;
    };
    if floor {
        planes[target].floor = plane;
    } else {
        planes[target].ceiling = plane;
    }
}

fn collect_polyobjects(map: &crate::geom::MapGeometry) -> Vec<PolyobjStatic> {
    // Crushing flags from spawn-spot things, keyed by poly id (angle field).
    let mut crush_ids = BTreeSet::new();
    for th in &map.things {
        match th.type_id {
            PO_SPAWN_CRUSH | PO_SPAWN_HURT | PO_SPAWN_CRUSH_HEXEN => {
                crush_ids.insert(i32::from(th.angle));
            }
            PO_ANCHOR | PO_SPAWN | PO_ANCHOR_HEXEN | PO_SPAWN_HEXEN => {}
            _ => {}
        }
    }

    let mut by_id: BTreeMap<i32, PolyobjStatic> = BTreeMap::new();

    for (li, line) in map.lines.iter().enumerate() {
        let lid = LineId::from_raw(li as u32);
        match line.special {
            POLYOBJ_START_LINE => {
                let id = line.args[0];
                if id == 0 {
                    continue;
                }
                let mirror = line.args[1];
                let lines = trace_poly_lines(map, lid);
                let vertices = verts_of_lines(map, &lines);
                by_id.entry(id).or_insert_with(|| PolyobjStatic {
                    id,
                    vertices,
                    lines,
                    mirror,
                    crush: crush_ids.contains(&id),
                });
            }
            POLYOBJ_EXPLICIT_LINE => {
                let id = line.args[0];
                if id == 0 {
                    continue;
                }
                let mirror = line.args[1];
                let entry = by_id.entry(id).or_insert_with(|| PolyobjStatic {
                    id,
                    vertices: Vec::new(),
                    lines: Vec::new(),
                    mirror,
                    crush: crush_ids.contains(&id),
                });
                if !entry.lines.contains(&lid) {
                    entry.lines.push(lid);
                }
            }
            _ => {}
        }
    }

    // Refresh vertex lists for explicit polys.
    for po in by_id.values_mut() {
        if po.vertices.is_empty() {
            po.vertices = verts_of_lines(map, &po.lines);
        }
    }

    by_id.into_values().collect()
}

/// Walk connected linedefs from a Polyobj_StartLine until the loop closes.
fn trace_poly_lines(map: &crate::geom::MapGeometry, start: LineId) -> Vec<LineId> {
    let mut out = vec![start];
    let Some(first) = map.lines.get(start.raw() as usize) else {
        return out;
    };
    let mut cur_v = first.v2;
    let start_v = first.v1;
    let mut guard = 0u32;
    while cur_v != start_v && guard < 10_000 {
        guard += 1;
        let mut next = None;
        for (li, line) in map.lines.iter().enumerate() {
            let lid = LineId::from_raw(li as u32);
            if out.contains(&lid) {
                continue;
            }
            if line.v1 == cur_v {
                next = Some((lid, line.v2));
                break;
            }
            if line.v2 == cur_v {
                next = Some((lid, line.v1));
                break;
            }
        }
        let Some((lid, nv)) = next else {
            break;
        };
        out.push(lid);
        cur_v = nv;
    }
    out
}

fn verts_of_lines(map: &crate::geom::MapGeometry, lines: &[LineId]) -> Vec<VertexId> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for &lid in lines {
        let Some(line) = map.lines.get(lid.raw() as usize) else {
            continue;
        };
        for v in [line.v1, line.v2] {
            if seen.insert(v.raw()) {
                out.push(v);
            }
        }
    }
    out
}

fn collect_portals(map: &crate::geom::MapGeometry) -> PortalTables {
    let mut lines = Vec::new();
    let mut sectors = Vec::new();

    // Index lines by tag for portal targets.
    let mut by_tag: BTreeMap<i32, Vec<LineId>> = BTreeMap::new();
    for (li, line) in map.lines.iter().enumerate() {
        let tag = i32::from(line.tag);
        if tag != 0 {
            by_tag
                .entry(tag)
                .or_default()
                .push(LineId::from_raw(li as u32));
        }
        // UDMF / Hexen may put id in args[0] for some specials — also index arg0
        // when it looks like a tag for portal pairing.
        if line.special == LINE_SET_PORTAL && line.args[0] != 0 {
            by_tag
                .entry(line.args[0])
                .or_default()
                .push(LineId::from_raw(li as u32));
        }
    }

    for (li, line) in map.lines.iter().enumerate() {
        let lid = LineId::from_raw(li as u32);
        match line.special {
            LINE_HORIZON => {
                lines.push(LinePortal {
                    line: lid,
                    target: None,
                    kind: 9,
                });
            }
            LINE_MIRROR => {
                lines.push(LinePortal {
                    line: lid,
                    target: None,
                    kind: 182,
                });
            }
            LINE_SET_PORTAL => {
                let tag = if line.args[0] != 0 {
                    line.args[0]
                } else {
                    i32::from(line.tag)
                };
                let kind = line.args[2].clamp(0, 255) as u8;
                let target = by_tag
                    .get(&tag)
                    .and_then(|v| v.iter().copied().find(|&t| t != lid));
                lines.push(LinePortal {
                    line: lid,
                    target,
                    kind,
                });
            }
            SECTOR_SET_PORTAL => {
                // args: tag, type, … — bind first sector with matching tag.
                let tag = line.args[0];
                let kind = line.args[1].clamp(0, 255) as u8;
                let plane = line.args[2].clamp(0, 1) as u8;
                let src = line.front_sector.unwrap_or(SectorId::from_raw(0));
                let target = map
                    .sectors
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.tags.contains(&tag))
                    .map(|(i, _)| SectorId::from_raw(i as u32))
                    .unwrap_or(src);
                sectors.push(SectorPortal {
                    sector: src,
                    target,
                    kind,
                    plane,
                });
            }
            _ => {}
        }
    }

    PortalTables { lines, sectors }
}

fn build_portal_groups(map: &crate::geom::MapGeometry, portals: &PortalTables) -> PortalGroups {
    let n = map.sectors.len();
    let mut groups = PortalGroups {
        sector_group: vec![0; n],
        delta: BTreeMap::new(),
    };
    if n == 0 {
        return groups;
    }
    let portal_lines: BTreeSet<u32> = portals
        .lines
        .iter()
        .filter(|p| p.is_traversable())
        .map(|p| p.line.raw())
        .collect();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (li, line) in map.lines.iter().enumerate() {
        if portal_lines.contains(&(li as u32)) {
            continue;
        }
        let Some(a) = line.front_sector else {
            continue;
        };
        let Some(b) = line.back_sector else {
            continue;
        };
        let ai = a.raw() as usize;
        let bi = b.raw() as usize;
        if ai < n && bi < n && ai != bi {
            adj[ai].push(bi);
            adj[bi].push(ai);
        }
    }
    let mut sector_group = vec![u32::MAX; n];
    let mut gid = 0u32;
    for s in 0..n {
        if sector_group[s] != u32::MAX {
            continue;
        }
        let mut stack = vec![s];
        sector_group[s] = gid;
        while let Some(u) = stack.pop() {
            for &v in &adj[u] {
                if sector_group[v] == u32::MAX {
                    sector_group[v] = gid;
                    stack.push(v);
                }
            }
        }
        gid = gid.saturating_add(1);
    }
    for p in &portals.lines {
        if !p.is_traversable() {
            continue;
        }
        let Some(dest_id) = p.target else {
            continue;
        };
        let Some(src) = map.lines.get(p.line.raw() as usize) else {
            continue;
        };
        let Some(dst) = map.lines.get(dest_id.raw() as usize) else {
            continue;
        };
        let Some(g0) = src
            .front_sector
            .and_then(|s| sector_group.get(s.raw() as usize).copied())
        else {
            continue;
        };
        let Some(g1) = dst
            .front_sector
            .and_then(|s| sector_group.get(s.raw() as usize).copied())
        else {
            continue;
        };
        if g0 == g1 {
            continue;
        }
        let Some(sv1) = map.vertices.get(src.v1.raw() as usize) else {
            continue;
        };
        let Some(sv2) = map.vertices.get(src.v2.raw() as usize) else {
            continue;
        };
        let Some(dv1) = map.vertices.get(dst.v1.raw() as usize) else {
            continue;
        };
        let Some(dv2) = map.vertices.get(dst.v2.raw() as usize) else {
            continue;
        };
        let dx = (dv1.pos.x() + dv2.pos.x()) * 0.5 - (sv1.pos.x() + sv2.pos.x()) * 0.5;
        let dy = (dv1.pos.y() + dv2.pos.y()) * 0.5 - (sv1.pos.y() + sv2.pos.y()) * 0.5;
        groups.delta.insert((g0, g1), (dx, dy));
        groups.delta.insert((g1, g0), (-dx, -dy));
    }
    groups.sector_group = sector_group;
    groups
}

fn collect_ffloors(map: &crate::geom::MapGeometry) -> Vec<FFloorStatic> {
    let mut out = Vec::new();
    for line in &map.lines {
        let (tag, typ, flags, alpha) = match line.special {
            SECTOR_SET_3DFLOOR => {
                // Sector_Set3DFloor(tag, type, flags, alpha, …)
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let alpha = line.args[3].clamp(0, 255) as u8;
                #[allow(clippy::cast_sign_loss)]
                let flags = line.args[2].max(0) as u32;
                (line.args[0], line.args[1], flags, alpha)
            }
            EXTRA_FLOOR_LIGHT_ONLY => {
                // ExtraFloor_LightOnly(tag, mode) → type-3 light transfer, no planes (alpha 0).
                // GZDoom flagvals: 0 → FF_FADEWALLS|FF_NODAMAGE, 1 → +FF_DOUBLESHADOW, 2 → +FF_RESET.
                if line.args[0] == 0 {
                    continue;
                }
                let mode = line.args[1].clamp(0, 2);
                let flags: u32 = match mode {
                    1 => 2 + 512 + 2048,    // DOUBLESHADOW | FADEWALLS | NODAMAGE
                    2 => 512 + 1024 + 2048, // FADEWALLS | RESET | NODAMAGE
                    _ => 512 + 2048,        // FADEWALLS | NODAMAGE
                };
                (line.args[0], 3, flags, 0)
            }
            _ => continue,
        };
        let control = line.front_sector.unwrap_or(SectorId::from_raw(0));
        for (i, sec) in map.sectors.iter().enumerate() {
            if sec.tags.contains(&tag) {
                out.push(FFloorStatic {
                    target: SectorId::from_raw(i as u32),
                    control,
                    typ,
                    flags,
                    alpha,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{MapFormat, RawLinedef, RawSector, RawSidedef, RawThing, RawVertex, link_geometry};

    fn square_map_with_line(
        special: i16,
        args: [i32; 5],
        things: Vec<RawThing>,
    ) -> crate::geom::MapGeometry {
        let vertices = vec![
            RawVertex { x: 0, y: 0 },
            RawVertex { x: 64, y: 0 },
            RawVertex { x: 64, y: 64 },
            RawVertex { x: 0, y: 64 },
        ];
        let linedefs = (0u16..4)
            .map(|i| RawLinedef {
                v1: i,
                v2: (i + 1) % 4,
                flags: 1,
                special: if i == 0 { special } else { 0 },
                tag: 0,
                side_front: i,
                side_back: 0xFFFF,
                args: if i == 0 { args } else { [0; 5] },
                hexen: true,
                activation: 0,
                arg0_str: String::new(),
                midtex3d: false,
                midtex3d_impassible: false,
                check_switch_range: false,
                first_side_only: false,
                block_floaters: false,
                lock_number: 0,
                block_projectiles: false,
                block_sight: false,
                block_hitscan: false,
                extra_ids: Default::default(),
                alpha: 1.0,
                mid_additive: false,
                health: 0,
                health_group: 0,
                automap_style: 0,
                clip_midtex: false,
                wrap_midtex: false,
            })
            .collect();
        let sidedefs = (0u16..4)
            .map(|_| RawSidedef {
                x_offset: 0,
                y_offset: 0,
                upper: "-".into(),
                lower: "-".into(),
                middle: "WALL".into(),
                sector: 0,
                light: i16::MIN,
                light_absolute: false,
                light_top: i16::MIN,
                light_mid: i16::MIN,
                light_bottom: i16::MIN,
                light_absolute_top: false,
                light_absolute_mid: false,
                light_absolute_bottom: false,
                scale_x: 1.0,
                scale_y: 1.0,
                scale_x_top: 1.0,
                scale_y_top: 1.0,
                scale_x_bottom: 1.0,
                scale_y_bottom: 1.0,
                x_offset_top: 0,
                y_offset_top: 0,
                x_offset_bottom: 0,
                y_offset_bottom: 0,
                no_fake_contrast: false,
                smooth_lighting: false,
                scroll_x: 0.0,
                scroll_y: 0.0,
                clip_midtex: false,
                wrap_midtex: false,
                no_decals: false,
            })
            .collect();
        let sectors = vec![RawSector {
            floor_height: 0,
            ceiling_height: 128,
            floor_pic: "FLAT".into(),
            ceiling_pic: "FLAT".into(),
            light: 160,
            special: 0,
            tag: 0,
            floor_plane: None,
            ceiling_plane: None,
            extra_ids: Vec::new(),
            floor_x_pan: 0.0,
            floor_y_pan: 0.0,
            ceiling_x_pan: 0.0,
            ceiling_y_pan: 0.0,
            floor_rotation: 0.0,
            ceiling_rotation: 0.0,
            floor_x_scale: 1.0,
            floor_y_scale: 1.0,
            ceiling_x_scale: 1.0,
            ceiling_y_scale: 1.0,
            gravity: -1.0,
            friction: 0,
            damage_amount: 0,
            damage_type: String::new(),
            damage_interval: 0,
            leakiness: 0,
            silent: false,
            waterzone: false,
            drop_actors: true,
            no_falling_damage: false,
            light_color: -1,
            fade_color: -1,
            fog_density: 0,
            desaturation: 0,
            color_floor: -1,
            color_ceiling: -1,
            color_walltop: -1,
            color_wallbottom: -1,
            color_sprites: -1,
            health_floor: 0,
            health_ceiling: 0,
            health_3d: 0,
            floor_alpha: 1.0,
            ceiling_alpha: 1.0,
            floor_additive: false,
            ceiling_additive: false,
        }];
        link_geometry(
            "MAP01",
            MapFormat::Hexen,
            vertices,
            linedefs,
            sidedefs,
            sectors,
            things,
            [0; 16],
        )
    }

    #[test]
    fn flat_plane_z_at() {
        let p = SecPlane::horizontal(64.0, true);
        assert!((p.z_at(0.0, 0.0) - 64.0).abs() < 1e-9);
        assert!(!p.is_slope());
    }

    #[test]
    fn from_points_makes_slope() {
        let p =
            SecPlane::from_points([0.0, 0.0, 0.0], [64.0, 0.0, 0.0], [0.0, 64.0, 32.0]).unwrap();
        assert!(p.is_slope());
        assert!((p.z_at(0.0, 0.0) - 0.0).abs() < 1e-6);
        assert!((p.z_at(0.0, 64.0) - 32.0).abs() < 1e-6);
    }

    #[test]
    fn polyobj_start_line_traces_loop() {
        let map = square_map_with_line(POLYOBJ_START_LINE, [7, 0, 0, 0, 0], Vec::new());
        let setup = build_setup(&map);
        assert_eq!(setup.polyobjects.len(), 1);
        assert_eq!(setup.polyobjects[0].id, 7);
        assert_eq!(setup.polyobjects[0].lines.len(), 4);
        assert_eq!(setup.polyobjects[0].vertices.len(), 4);
    }

    #[test]
    fn line_mirror_recorded() {
        let map = square_map_with_line(LINE_MIRROR, [0; 5], Vec::new());
        let setup = build_setup(&map);
        assert_eq!(setup.portals.lines.len(), 1);
        assert_eq!(setup.portals.lines[0].kind, 182);
        assert!(setup.portals.lines[0].is_mirror());
        assert!(!setup.portals.lines[0].skip_wall());
        assert!(setup.portals.lines[0].target.is_none());
    }

    #[test]
    fn crush_spot_sets_flag() {
        let things = vec![RawThing {
            tid: 0,
            x: 0,
            y: 0,
            z: 0,
            angle: 3, // poly id
            type_id: PO_SPAWN_CRUSH,
            flags: 0,
            special: 0,
            args: [0; 5],
            hexen: false,
            friendly_see_blocks: -1,
            health: 0,
            score: -2147483648,
            gravity: -1.0,
            scale_x: 0.0,
            scale_y: 0.0,
            alpha: -1.0,
            pitch: -32768,
            roll: -32768,
            fill_color: -1,
            render_style: String::new(),
            float_bob_phase: -1,
            count_secret: false,
            no_count: false,
            arg0_str: String::new(),
            conversation: 0,
            accuracy: i32::MIN,
            stamina: i32::MIN,
            species: String::new(),
            melee_range: -1.0,
            view_height: -1.0,
            mass: i32::MIN,
            speed: -1.0,
            friction: -1.0,
            damage_factor: -1.0,
            max_step_height: -1.0,
            max_dropoff_height: -1.0,
            float_speed: -1.0,
            fast_speed: -1.0,
            jump_z: -1.0,
            attack_z_offset: -1.0,
        }];
        let map = square_map_with_line(POLYOBJ_START_LINE, [3, 0, 0, 0, 0], things);
        let setup = build_setup(&map);
        assert!(setup.polyobjects[0].crush);
    }

    #[test]
    fn slope_floor_point_line_thing() {
        let mut map = square_map_with_line(
            0,
            [0; 5],
            vec![RawThing {
                tid: 0,
                x: 32,
                y: 32,
                z: 48,
                angle: 0,
                type_id: SLOPE_FLOOR_POINT_LINE,
                flags: 0,
                special: 0,
                args: [9, 0, 0, 0, 0],
                hexen: true,
                friendly_see_blocks: -1,
                health: 0,
                score: -2147483648,
                gravity: -1.0,
                scale_x: 0.0,
                scale_y: 0.0,
                alpha: -1.0,
                pitch: -32768,
                roll: -32768,
                fill_color: -1,
                render_style: String::new(),
                float_bob_phase: -1,
                count_secret: false,
                no_count: false,
                arg0_str: String::new(),
                conversation: 0,
                accuracy: i32::MIN,
                stamina: i32::MIN,
                species: String::new(),
                melee_range: -1.0,
                view_height: -1.0,
                mass: i32::MIN,
                speed: -1.0,
                friction: -1.0,
                damage_factor: -1.0,
                max_step_height: -1.0,
                max_dropoff_height: -1.0,
                float_speed: -1.0,
                fast_speed: -1.0,
                jump_z: -1.0,
                attack_z_offset: -1.0,
            }],
        );
        map.lines[0].tag = 9;
        let setup = build_setup(&map);
        assert!(setup.planes[0].floor.is_slope());
        assert!((setup.planes[0].floor.z_at(32.0, 32.0) - 48.0).abs() < 1e-3);
    }

    #[test]
    fn sector_set_3dfloor_records_type() {
        let mut map = square_map_with_line(SECTOR_SET_3DFLOOR, [7, 1, 0, 255, 0], Vec::new());
        map.sectors[0].tags.clear();
        map.sectors[0].tags.push(7);
        let setup = build_setup(&map);
        assert_eq!(setup.ffloors.len(), 1);
        assert_eq!(setup.ffloors[0].typ, 1);
        assert_eq!(setup.ffloors[0].alpha, 255);
        assert!(setup.ffloors[0].is_solid());

        let mut map2 = square_map_with_line(SECTOR_SET_3DFLOOR, [7, 3, 0, 128, 0], Vec::new());
        map2.sectors[0].tags.clear();
        map2.sectors[0].tags.push(7);
        let setup2 = build_setup(&map2);
        assert_eq!(setup2.ffloors[0].alpha, 128);
        assert!((setup2.ffloors[0].opacity() - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn sector_attach_3d_midtex_matches_moreids() {
        let mut map = square_map_with_line(SECTOR_ATTACH_3D_MIDTEX, [42, 0, 0, 0, 0], Vec::new());
        map.lines.push(crate::geom::Line {
            v1: VertexId::from_raw(0),
            v2: VertexId::from_raw(1),
            delta: ferrite_core::DVec2::new(64.0, 0.0),
            sides: [
                Some(ferrite_core::SideId::from_raw(0)),
                Some(ferrite_core::SideId::from_raw(0)),
            ],
            front_sector: Some(SectorId::from_raw(0)),
            back_sector: Some(SectorId::from_raw(0)),
            flags: 0,
            special: 0,
            tag: 1,
            args: [0; 5],
            activation: 0,
            mid_alpha: 1.0,
            mid_additive: false,
            arg0_str: String::new(),
            midtex3d: false,
            midtex3d_impassible: false,
            check_switch_range: false,
            first_side_only: false,
            block_floaters: false,
            lock_number: 0,
            block_projectiles: false,
            block_sight: false,
            block_hitscan: false,
            extra_ids: smallvec::smallvec![42],
            health: 0,
            health_group: 0,
            automap_style: 0,
            clip_midtex: false,
            wrap_midtex: false,
        });
        let cand = map.lines.len() - 1;
        apply_attach_3d_midtex(&mut map);
        assert!(
            map.lines[cand].midtex3d,
            "moreids secondary id should match Attach arg0"
        );
    }

    #[test]
    fn sector_attach_3d_midtex_marks_lines() {
        let mut map = square_map_with_line(SECTOR_ATTACH_3D_MIDTEX, [9, 0, 0, 0, 0], Vec::new());
        // Shared two-sided candidate with matching line id.
        map.lines.push(crate::geom::Line {
            v1: VertexId::from_raw(0),
            v2: VertexId::from_raw(1),
            delta: ferrite_core::DVec2::new(64.0, 0.0),
            sides: [
                Some(ferrite_core::SideId::from_raw(0)),
                Some(ferrite_core::SideId::from_raw(0)),
            ],
            front_sector: Some(SectorId::from_raw(0)),
            back_sector: Some(SectorId::from_raw(0)),
            flags: 0,
            special: 0,
            tag: 9,
            args: [0; 5],
            activation: 0,
            mid_alpha: 1.0,
            mid_additive: false,
            arg0_str: String::new(),
            midtex3d: false,
            midtex3d_impassible: false,
            check_switch_range: false,
            first_side_only: false,
            block_floaters: false,
            lock_number: 0,
            block_projectiles: false,
            block_sight: false,
            block_hitscan: false,
            extra_ids: Default::default(),
            health: 0,
            health_group: 0,
            automap_style: 0,
            clip_midtex: false,
            wrap_midtex: false,
        });
        let cand = map.lines.len() - 1;
        apply_attach_3d_midtex(&mut map);
        assert!(map.lines[cand].midtex3d);
        let setup = build_setup(&map);
        assert_eq!(setup.midtex_attach.len(), 1);
        assert_eq!(setup.midtex_attach[0].lines, vec![cand as u32]);
    }

    #[test]
    fn extra_floor_light_only_records_type3() {
        let mut map = square_map_with_line(EXTRA_FLOOR_LIGHT_ONLY, [7, 0, 0, 0, 0], Vec::new());
        map.sectors[0].tags.clear();
        map.sectors[0].tags.push(7);
        let setup = build_setup(&map);
        assert_eq!(setup.ffloors.len(), 1);
        assert_eq!(setup.ffloors[0].typ, 3);
        assert_eq!(setup.ffloors[0].alpha, 0);
        assert!(setup.ffloors[0].is_light_transfer());
        assert!(!setup.ffloors[0].is_solid());
    }
}
