//! A map in the form Doom's renderer walks: 16.16 vertices, segs with their
//! angle / offset / sectors, subsectors, nodes, and side textures resolved
//! to [`SoftData`] ids.

use crate::data::SoftData;
use crate::fixed::to_fixed;
use room_map::{MapFormat, MapGeometry, NF_SUBSECTOR};
use room_vfs::Vfs;

/// `ML_TWOSIDED`.
pub const ML_TWOSIDED: i16 = 4;
/// `ML_DONTPEGTOP`.
pub const ML_DONTPEGTOP: i16 = 8;
/// `ML_DONTPEGBOTTOM`.
pub const ML_DONTPEGBOTTOM: i16 = 16;

/// Specials (ZDoom numbering after `room-map`'s translation).
const TRANSLUCENT_LINE: i16 = 208;
const TRANSFER_HEIGHTS: i16 = 209;
const TRANSFER_FLOOR_LIGHT: i16 = 210;
const TRANSFER_CEILING_LIGHT: i16 = 211;
const STATIC_INIT: i16 = 190;
const INIT_TRANSFER_SKY: i32 = 255;
/// Boom specials a Doom-format map keeps as their own numbers.
const BOOM_TRANSLUCENT: i16 = 260;
const BOOM_SKY: i16 = 271;
const BOOM_SKY_FLIPPED: i16 = 272;
/// `ExtraFloor_LightOnly` and `Sector_Set3DFloor`.
const EXTRAFLOOR_LIGHT_ONLY: i16 = 50;
const SECTOR_SET_3DFLOOR: i16 = 160;

/// `Sector_Set3DFloor` flags (arg 2) the renderer reads.
pub const FF3D_NOSHADE: u32 = 1;
/// Light stops at the slab's bottom (`FF_DOUBLESHADOW`).
pub const FF3D_DOUBLESHADOW: u32 = 2;
/// A fog volume (`FF_FOG`): not drawn (the fog itself is not done).
pub const FF3D_FOG: u32 = 4;
/// No bottom: the slab is its top plane (`FF_THINFLOOR`).
pub const FF3D_THIN: u32 = 8;
/// Sides use the target line's upper / lower texture.
pub const FF3D_UPPERTEXTURE: u32 = 16;
/// See [`FF3D_UPPERTEXTURE`].
pub const FF3D_LOWERTEXTURE: u32 = 32;
/// Additive translucency.
pub const FF3D_ADDITIVE: u32 = 64;

/// A 3D floor of a sector (`Sector_Set3DFloor` / `ExtraFloor_LightOnly`):
/// a slab between the control sector's planes, drawn inside the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoftFFloor {
    /// Control sector (its planes bound the slab, its light is inside).
    pub control: u32,
    /// Type (arg 1): `& 3` = 0 Vavoom (planes swapped), 1 solid,
    /// 2 swimmable, 3 non-solid; `4` = drawn from inside too.
    pub typ: i32,
    /// `FF3D_*` flags (arg 2).
    pub flags: u32,
    /// Opacity 0..=255 (0 = planes and sides hidden; light only).
    pub alpha: u8,
    /// Front side of the control line (its middle texture is the sides').
    pub side: Option<u32>,
}

impl SoftFFloor {
    /// Planes and sides are drawn.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.alpha > 0 && self.flags & FF3D_FOG == 0
    }

    /// Planes and inner sides are drawn from inside the slab
    /// (swimmable, or type bit 4).
    #[must_use]
    pub fn both_sides(&self) -> bool {
        self.typ & 3 == 2 || self.typ & 4 != 0
    }

    /// The slab's top and bottom (16.16) and their flats, from the control
    /// sector as `sectors` has it.
    #[must_use]
    pub fn planes(&self, sectors: &[SoftSector]) -> Option<SlabPlanes> {
        let c = sectors.get(self.control as usize)?;
        let mut p = if self.typ & 3 == 0 {
            SlabPlanes {
                top: c.floor,
                top_pic: c.floor_pic,
                top_slope: c.floor_slope,
                bottom: c.ceiling,
                bottom_pic: c.ceiling_pic,
                bottom_slope: c.ceiling_slope,
                light: c.light,
            }
        } else {
            SlabPlanes {
                top: c.ceiling,
                top_pic: c.ceiling_pic,
                top_slope: c.ceiling_slope,
                bottom: c.floor,
                bottom_pic: c.floor_pic,
                bottom_slope: c.floor_slope,
                light: c.light,
            }
        };
        if self.flags & FF3D_THIN != 0 {
            p.bottom = p.top;
            p.bottom_slope = p.top_slope;
        }
        Some(p)
    }

    /// The slab shades the space below it with the control sector's light.
    #[must_use]
    pub fn shades(&self) -> bool {
        self.flags & FF3D_NOSHADE == 0
    }
}

/// A line portal as the renderer draws it (SR7), seen from the line's
/// front side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoftLinePortal {
    /// `Line_SetPortal` with a target: the view continues out of `target`.
    /// A point `p` maps to `d2 + rot(p − s1)` (map units).
    Visual {
        /// Destination line.
        target: u32,
        /// Origin line's first vertex.
        s1: (f64, f64),
        /// Destination line's second vertex.
        d2: (f64, f64),
        /// Rotation (cos, sin) and as BAM.
        cos: f64,
        /// See `cos`.
        sin: f64,
        /// See `cos`.
        angle: u32,
        /// `PORG_*` height alignment.
        align: u8,
        /// Front sectors of origin and destination (height alignment).
        sectors: (u32, u32),
    },
    /// `Line_Mirror`.
    Mirror,
    /// `Line_Horizon`: the front sector's floor and ceiling run to the
    /// horizon.
    Horizon,
}

/// A sector plane portal (SR7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoftSectorPortal {
    /// Sky viewpoint: the view from a thing (`x`, `y` in units, `z` above
    /// its sector's floor), turned by `angle`.
    Skybox {
        /// Viewpoint.
        x: f64,
        /// Viewpoint.
        y: f64,
        /// Height above the floor of `sector`.
        z_offset: f64,
        /// Sector the viewpoint stands in.
        sector: Option<u32>,
        /// Turn (BAM).
        angle: u32,
    },
    /// Stacked sectors / plane portals: the view moved by `(dx, dy)`.
    Stacked {
        /// Displacement (units).
        dx: f64,
        /// Displacement (units).
        dy: f64,
    },
}

/// A plane's portal: index into [`SoftLevel::sector_portals`], and whether
/// it only shows on sky flats.
pub type PlanePortal = Option<(u32, bool)>;

/// Top and bottom of a 3D floor this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlabPlanes {
    /// Top height (16.16; the nominal height of a sloped top).
    pub top: i32,
    /// Top flat.
    pub top_pic: u16,
    /// Sloped top (the control sector's plane).
    pub top_slope: Option<SoftSlope>,
    /// Bottom height (16.16).
    pub bottom: i32,
    /// Bottom flat.
    pub bottom_pic: u16,
    /// Sloped bottom.
    pub bottom_slope: Option<SoftSlope>,
    /// Control sector light.
    pub light: i32,
}

/// Boom sky transfer (271 / 272) of a sector: the sky is the upper texture
/// of `side`, panned by its offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkyTransfer {
    /// Front side of the transfer line.
    pub side: u32,
    /// 271 draws the sky mirrored (`flip = ~0`); 272 as it is.
    pub mirrored: bool,
}

/// UDMF flat scale / rotation as a [`FlatXform`] (0 or non-finite scales
/// mean 1).
fn flat_xform(su: f64, sv: f64, angle_deg: f64) -> FlatXform {
    let s = |v: f64| {
        if v.is_finite() && v != 0.0 {
            (v * 65536.0).round() as i32
        } else {
            1 << 16
        }
    };
    FlatXform {
        xscale: s(su),
        yscale: s(sv),
        angle: if angle_deg.is_finite() {
            ((angle_deg.rem_euclid(360.0) / 360.0) * 4_294_967_296.0) as u64 as u32
        } else {
            0
        },
    }
}

/// The slope of `plane` when it is not horizontal.
#[must_use]
pub fn slope_of(plane: &room_map::setup::SecPlane) -> Option<SoftSlope> {
    if !plane.is_slope() || plane.normal_z == 0.0 {
        return None;
    }
    Some(SoftSlope {
        z0: plane.d / plane.normal_z,
        dzdx: -plane.normal_x / plane.normal_z,
        dzdy: -plane.normal_y / plane.normal_z,
    })
}

/// A UDMF colour (`-1` = unset) as stored in [`SoftSector`]: white light
/// and black fade become `0`.
#[must_use]
pub fn udmf_color(v: i32, default: u32) -> u32 {
    let c = if v < 0 { default } else { v as u32 & 0xff_ffff };
    if c == 0xff_ffff && default == 0xff_ffff {
        0
    } else {
        c
    }
}

/// Picture id of the sky flat (`F_SKY1`).
pub const SKY_FLAT: u16 = u16::MAX;

/// A vertex in 16.16.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoftVertex {
    /// X.
    pub x: i32,
    /// Y.
    pub y: i32,
}

/// `seg_t`.
#[derive(Clone, Copy, Debug)]
pub struct SoftSeg {
    /// Start vertex index.
    pub v1: u32,
    /// End vertex index.
    pub v2: u32,
    /// BAM angle (high 16 bits significant, as `P_LoadSegs`).
    pub angle: u32,
    /// Distance along the line (16.16).
    pub offset: i32,
    /// Sidedef.
    pub side: u32,
    /// Linedef.
    pub line: u32,
    /// Front sector.
    pub front: u32,
    /// Back sector (`None` for one-sided lines).
    pub back: Option<u32>,
}

/// UDMF per-part texture placement of a side (`offsetx_top`, `scalex_mid`,
/// …): added offsets and scales (16.16; 1.0 = vanilla).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SidePart {
    /// Extra horizontal offset (16.16).
    pub xoff: i32,
    /// Extra vertical offset (16.16).
    pub yoff: i32,
    /// Horizontal scale (16.16, texels per map unit).
    pub xscale: i32,
    /// Vertical scale.
    pub yscale: i32,
}

impl Default for SidePart {
    fn default() -> Self {
        Self {
            xoff: 0,
            yoff: 0,
            xscale: 1 << 16,
            yscale: 1 << 16,
        }
    }
}

/// Parts of [`SoftSide::parts`].
pub const PART_TOP: usize = 0;
/// Lower part.
pub const PART_BOTTOM: usize = 1;
/// Middle part.
pub const PART_MID: usize = 2;

/// `side_t` (what the renderer reads).
#[derive(Clone, Copy, Debug, Default)]
pub struct SoftSide {
    /// `textureoffset` (16.16).
    pub texture_offset: i32,
    /// `rowoffset` (16.16).
    pub row_offset: i32,
    /// Upper texture id (0 = none).
    pub top: u16,
    /// Lower texture id.
    pub bottom: u16,
    /// Middle texture id.
    pub mid: u16,
    /// UDMF placement per part ([`PART_TOP`], [`PART_BOTTOM`], [`PART_MID`]).
    pub parts: [SidePart; 3],
}

/// A sloped plane (ZDoom `secplane_t`): height `z0 + dzdx·x + dzdy·y` in
/// map units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoftSlope {
    /// Height at the origin.
    pub z0: f64,
    /// Rise per unit of x.
    pub dzdx: f64,
    /// Rise per unit of y.
    pub dzdy: f64,
}

impl SoftSlope {
    /// Height at `(x, y)` (map units).
    #[inline]
    #[must_use]
    pub fn z_at(&self, x: f64, y: f64) -> f64 {
        self.z0 + self.dzdx * x + self.dzdy * y
    }
}

/// UDMF flat placement (`xscalefloor`, `rotationfloor`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatXform {
    /// Horizontal scale (16.16).
    pub xscale: i32,
    /// Vertical scale (16.16).
    pub yscale: i32,
    /// Rotation (BAM).
    pub angle: u32,
}

impl Default for FlatXform {
    fn default() -> Self {
        Self {
            xscale: 1 << 16,
            yscale: 1 << 16,
            angle: 0,
        }
    }
}

impl FlatXform {
    /// Vanilla placement (no scale, no rotation).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }
}

/// `node_t`.
#[derive(Clone, Copy, Debug)]
pub struct SoftNode {
    /// Partition X (16.16).
    pub x: i32,
    /// Partition Y.
    pub y: i32,
    /// Partition delta X.
    pub dx: i32,
    /// Partition delta Y.
    pub dy: i32,
    /// Child boxes `[right, left]`, each `[top, bottom, left, right]` (16.16).
    pub bbox: [[i32; 4]; 2],
    /// Children (`NF_SUBSECTOR` = leaf).
    pub children: [u32; 2],
}

/// Per-frame sector state (heights from the playsim; defaults from the map).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoftSector {
    /// Floor height (16.16).
    pub floor: i32,
    /// Ceiling height (16.16).
    pub ceiling: i32,
    /// Floor flat id ([`SKY_FLAT`] = sky).
    pub floor_pic: u16,
    /// Ceiling flat id.
    pub ceiling_pic: u16,
    /// Light level 0–255.
    pub light: i32,
    /// Floor flat panning, Boom `floor_xoffs` (16.16).
    pub floor_xoffs: i32,
    /// `floor_yoffs`.
    pub floor_yoffs: i32,
    /// `ceiling_xoffs`.
    pub ceiling_xoffs: i32,
    /// `ceiling_yoffs`.
    pub ceiling_yoffs: i32,
    /// Light colour 0xRRGGBB (true-colour mode; `0` = white, no tint).
    pub color: u32,
    /// Fade (fog) colour 0xRRGGBB (true-colour mode; `0` = black, Doom's).
    pub fade: u32,
    /// Desaturation 0–255 (true-colour mode; UDMF `desaturation`).
    pub desaturation: u8,
    /// Sloped floor (SR6); `floor` stays the texture height.
    pub floor_slope: Option<SoftSlope>,
    /// Sloped ceiling.
    pub ceiling_slope: Option<SoftSlope>,
    /// UDMF floor flat scale / rotation.
    pub floor_xform: FlatXform,
    /// UDMF ceiling flat scale / rotation.
    pub ceiling_xform: FlatXform,
}

impl SoftSector {
    /// Floor height at `(x, y)` (16.16 in, 16.16 out).
    #[must_use]
    pub fn floor_at(&self, x: i32, y: i32) -> i32 {
        match self.floor_slope {
            Some(p) => to_fixed(p.z_at(f64::from(x) / 65536.0, f64::from(y) / 65536.0)),
            None => self.floor,
        }
    }

    /// Ceiling height at `(x, y)`.
    #[must_use]
    pub fn ceiling_at(&self, x: i32, y: i32) -> i32 {
        match self.ceiling_slope {
            Some(p) => to_fixed(p.z_at(f64::from(x) / 65536.0, f64::from(y) / 65536.0)),
            None => self.ceiling,
        }
    }
}

/// The map as the software renderer walks it.
#[derive(Clone, Debug)]
pub struct SoftLevel {
    /// Vertices (map vertices, then node-builder extra vertices).
    pub vertices: Vec<SoftVertex>,
    /// Segs.
    pub segs: Vec<SoftSeg>,
    /// Subsectors: (sector, first seg, seg count).
    pub subsectors: Vec<(u32, u32, u32)>,
    /// Nodes (root last).
    pub nodes: Vec<SoftNode>,
    /// Line flags.
    pub line_flags: Vec<i16>,
    /// Sides.
    pub sides: Vec<SoftSide>,
    /// Sector state at load.
    pub sectors: Vec<SoftSector>,
    /// Boom `heightsec` (242): the control sector whose planes are drawn.
    pub heightsec: Vec<Option<u32>>,
    /// Boom `floorlightsec` (213).
    pub floorlightsec: Vec<Option<u32>>,
    /// Boom `ceilinglightsec` (261).
    pub ceilinglightsec: Vec<Option<u32>>,
    /// Colormap sets of a control sector: (below the fake floor, between,
    /// above the fake ceiling) — `bottommap`, `midmap`, `topmap`.
    pub colormaps: Vec<[u16; 3]>,
    /// Boom sky transfer per sector.
    pub sky: Vec<Option<SkyTransfer>>,
    /// Translucency table per line (Boom 260 / `alpha`); `None` = opaque.
    pub line_tranmap: Vec<Option<u16>>,
    /// Lines of polyobjects: their BSP segs (at the anchor) are skipped.
    pub line_is_poly: Vec<bool>,
    /// Polyobject segs at their current place ([`Self::place_polyobjects`]).
    pub poly_segs: Vec<SoftSeg>,
    /// Polyobject seg indices drawn with each subsector (Hexen
    /// `subsector->poly`).
    pub subsector_polys: Vec<Vec<u32>>,
    /// 3D floors per target sector (SR6), lowest top first is not
    /// guaranteed: the order is the map's.
    pub ffloors: Vec<Vec<SoftFFloor>>,
    /// Convex outline (map units) of each subsector of a 3D floor's target
    /// sector, from its BSP leaf clipped by its segs; empty elsewhere.
    pub subsector_shapes: Vec<Vec<(f64, f64)>>,
    /// Line portals per line, with the line's front side (SR7).
    pub line_portals: Vec<Option<(SoftLinePortal, u32)>>,
    /// Sector portals drawn on planes.
    pub sector_portals: Vec<SoftSectorPortal>,
    /// Per sector: the floor and ceiling portal.
    pub plane_portals: Vec<[PlanePortal; 2]>,
    /// `PortalTables::revision` the portal tables were built from.
    pub portal_revision: u32,
    /// Line ends (16.16) per line, for portal clip lines.
    pub line_ends: Vec<(SoftVertex, SoftVertex)>,
}

impl SoftLevel {
    /// Build from linked geometry with game nodes (`room_nodebuild::ensure_nodes`).
    /// `None` when the map has no BSP.
    pub fn new(map: &MapGeometry, data: &mut SoftData, vfs: &Vfs) -> Option<Self> {
        let gn = map.game_nodes.as_ref()?;
        let mut vertices: Vec<SoftVertex> = map
            .vertices
            .iter()
            .map(|v| SoftVertex {
                x: to_fixed(v.pos.x()),
                y: to_fixed(v.pos.y()),
            })
            .collect();
        vertices.extend(gn.extra_vertices.iter().map(|v| SoftVertex {
            x: to_fixed(v.x()),
            y: to_fixed(v.y()),
        }));
        // Doom-format nodes carry each seg's angle and offset; node builders
        // that leave them zero get them from the vertices (`P_LoadSegs` values).
        let stored = gn.segs.iter().any(|s| s.angle != 0 || s.offset != 0);
        let mut segs = Vec::with_capacity(gn.segs.len());
        for s in &gn.segs {
            let Some(li) = s.line else {
                continue;
            };
            let line = &map.lines[li.raw() as usize];
            let side_idx = usize::from(s.side != 0);
            let Some(side) = line.sides[side_idx] else {
                continue;
            };
            let front = map.sides[side.raw() as usize].sector.raw();
            let back = (line.flags & ML_TWOSIDED != 0)
                .then(|| line.sides[side_idx ^ 1])
                .flatten()
                .map(|sd| map.sides[sd.raw() as usize].sector.raw());
            let (v1, v2) = (s.v1.raw(), s.v2.raw());
            let (angle, offset) = if stored {
                ((s.angle as u16 as u32) << 16, i32::from(s.offset) << 16)
            } else {
                let a = vertices[v1 as usize];
                let b = vertices[v2 as usize];
                let ang = room_core::math::point_to_angle2(a.x, a.y, b.x, b.y) & 0xffff_0000;
                let start = if side_idx == 0 { line.v1 } else { line.v2 };
                let o = map.vertices[start.raw() as usize].pos;
                let dx = f64::from(a.x) / 65536.0 - o.x();
                let dy = f64::from(a.y) / 65536.0 - o.y();
                (ang, ((dx * dx + dy * dy).sqrt() as i32) << 16)
            };
            segs.push(SoftSeg {
                v1,
                v2,
                angle,
                offset,
                side: side.raw(),
                line: li.raw(),
                front,
                back,
            });
        }
        // Subsector seg ranges refer to the original seg list; skipped
        // minisegs are rare in game nodes, but map indices to be safe.
        let mut remap = Vec::with_capacity(gn.segs.len() + 1);
        let mut n = 0u32;
        for s in &gn.segs {
            remap.push(n);
            if s.line.is_some() {
                n += 1;
            }
        }
        remap.push(n);
        let subsectors = gn
            .subsectors
            .iter()
            .map(|ss| {
                let a = remap[(ss.first_seg as usize).min(remap.len() - 1)];
                let b = remap[((ss.first_seg + ss.num_segs) as usize).min(remap.len() - 1)];
                let sector = segs.get(a as usize).map_or(0, |s| s.front);
                (sector, a, b - a)
            })
            .collect();
        let nodes = gn
            .nodes
            .iter()
            .map(|nd| SoftNode {
                x: i32::from(nd.x) << 16,
                y: i32::from(nd.y) << 16,
                dx: i32::from(nd.dx) << 16,
                dy: i32::from(nd.dy) << 16,
                bbox: nd.bbox.map(|b| b.map(|v| i32::from(v) << 16)),
                children: nd.children,
            })
            .collect();
        let sides = map
            .sides
            .iter()
            .map(|s| {
                let scale = |v: f32| {
                    if v.is_finite() && v != 0.0 {
                        (f64::from(v) * 65536.0).round() as i32
                    } else {
                        1 << 16
                    }
                };
                let part = |xoff: i16, yoff: i16, xs: f32, ys: f32| SidePart {
                    xoff: i32::from(xoff) << 16,
                    yoff: i32::from(yoff) << 16,
                    xscale: scale(xs),
                    yscale: scale(ys),
                };
                SoftSide {
                    texture_offset: i32::from(s.x_offset) << 16,
                    row_offset: i32::from(s.y_offset) << 16,
                    top: data.texture(vfs, &s.upper),
                    bottom: data.texture(vfs, &s.lower),
                    mid: data.texture(vfs, &s.middle),
                    parts: [
                        part(s.x_offset_top, s.y_offset_top, s.scale_x_top, s.scale_y_top),
                        part(
                            s.x_offset_bottom,
                            s.y_offset_bottom,
                            s.scale_x_bottom,
                            s.scale_y_bottom,
                        ),
                        part(0, 0, s.scale_x, s.scale_y),
                    ],
                }
            })
            .collect();
        let sectors = map
            .sectors
            .iter()
            .map(|s| SoftSector {
                floor: to_fixed(s.floor_height),
                ceiling: to_fixed(s.ceiling_height),
                floor_pic: flat_id(data, vfs, &s.floor_pic),
                ceiling_pic: flat_id(data, vfs, &s.ceiling_pic),
                light: i32::from(s.light),
                floor_xoffs: to_fixed(s.floor_u_off),
                floor_yoffs: to_fixed(s.floor_v_off),
                ceiling_xoffs: to_fixed(s.ceiling_u_off),
                ceiling_yoffs: to_fixed(s.ceiling_v_off),
                color: udmf_color(s.light_color, 0xff_ffff),
                fade: udmf_color(s.fade_color, 0),
                desaturation: s.desaturation.clamp(0, 255) as u8,
                floor_slope: None,
                ceiling_slope: None,
                floor_xform: flat_xform(s.floor_scale_u, s.floor_scale_v, s.floor_angle),
                ceiling_xform: flat_xform(s.ceiling_scale_u, s.ceiling_scale_v, s.ceiling_angle),
            })
            .collect();
        let n = map.sectors.len();
        let mut level = Self {
            vertices,
            segs,
            subsectors,
            nodes,
            line_flags: map.lines.iter().map(|l| l.flags).collect(),
            sides,
            sectors,
            heightsec: vec![None; n],
            floorlightsec: vec![None; n],
            ceilinglightsec: vec![None; n],
            colormaps: vec![[0; 3]; n],
            sky: vec![None; n],
            line_tranmap: vec![None; map.lines.len()],
            line_is_poly: vec![false; map.lines.len()],
            poly_segs: Vec::new(),
            subsector_polys: Vec::new(),
            ffloors: vec![Vec::new(); n],
            subsector_shapes: Vec::new(),
            line_portals: Vec::new(),
            sector_portals: Vec::new(),
            plane_portals: Vec::new(),
            portal_revision: 0,
            line_ends: map
                .lines
                .iter()
                .map(|l| {
                    let v = |id: room_core::VertexId| {
                        let p = map.vertices[id.raw() as usize].pos;
                        SoftVertex {
                            x: to_fixed(p.x()),
                            y: to_fixed(p.y()),
                        }
                    };
                    (v(l.v1), v(l.v2))
                })
                .collect(),
        };
        for (i, sp) in map.setup.planes.iter().enumerate().take(n) {
            level.sectors[i].floor_slope = slope_of(&sp.floor);
            level.sectors[i].ceiling_slope = slope_of(&sp.ceiling);
        }
        for po in &map.setup.polyobjects {
            for l in &po.lines {
                if let Some(f) = level.line_is_poly.get_mut(l.raw() as usize) {
                    *f = true;
                }
            }
        }
        level.boom_specials(map, data, vfs);
        level.collect_ffloors(map, data, vfs);
        level.load_portals(map);
        level.place_polyobjects(map);
        Some(level)
    }

    /// `R_PointInSubsector`.
    #[must_use]
    pub fn point_in_subsector(&self, x: i32, y: i32) -> Option<usize> {
        if self.nodes.is_empty() {
            return (!self.subsectors.is_empty()).then_some(0);
        }
        let mut num = (self.nodes.len() - 1) as u32;
        loop {
            if let Some(leaf) = Self::leaf(num) {
                return (leaf < self.subsectors.len()).then_some(leaf);
            }
            let n = self.nodes.get(num as usize)?;
            num = n.children[crate::render::point_on_line(x, y, n.x, n.y, n.dx, n.dy)];
        }
    }

    /// Hexen-style polyobjects: copy the current vertex positions of every
    /// polyobject line from `map` (moved by the playsim), build their segs
    /// and give each polyobject to the subsector holding its centre. Call
    /// each frame after polyobjects move.
    pub fn place_polyobjects(&mut self, map: &MapGeometry) {
        self.poly_segs.clear();
        for list in &mut self.subsector_polys {
            list.clear();
        }
        self.subsector_polys
            .resize_with(self.subsectors.len(), Vec::new);
        for po in &map.setup.polyobjects {
            let mut cx = 0.0;
            let mut cy = 0.0;
            let mut count = 0.0;
            let first = self.poly_segs.len();
            for l in &po.lines {
                let Some(line) = map.lines.get(l.raw() as usize) else {
                    continue;
                };
                for v in [line.v1, line.v2] {
                    let vi = v.raw() as usize;
                    if let (Some(src), Some(dst)) =
                        (map.vertices.get(vi), self.vertices.get_mut(vi))
                    {
                        dst.x = to_fixed(src.pos.x());
                        dst.y = to_fixed(src.pos.y());
                        cx += src.pos.x();
                        cy += src.pos.y();
                        count += 1.0;
                    }
                }
                let (Some(side), Some(front)) = (line.sides[0], line.front_sector) else {
                    continue;
                };
                let a = &self.vertices[line.v1.raw() as usize];
                let b = &self.vertices[line.v2.raw() as usize];
                let angle = (f64::from(b.y - a.y))
                    .atan2(f64::from(b.x - a.x))
                    .rem_euclid(std::f64::consts::TAU)
                    / std::f64::consts::TAU
                    * 4_294_967_296.0;
                self.poly_segs.push(SoftSeg {
                    v1: line.v1.raw(),
                    v2: line.v2.raw(),
                    angle: angle as u64 as u32,
                    offset: 0,
                    side: side.raw(),
                    line: l.raw(),
                    front: front.raw(),
                    back: line.back_sector.map(|b| b.raw()),
                });
            }
            if count == 0.0 || self.poly_segs.len() == first {
                continue;
            }
            let (x, y) = (to_fixed(cx / count), to_fixed(cy / count));
            if let Some(ss) = self.point_in_subsector(x, y) {
                self.subsector_polys[ss].extend(first as u32..self.poly_segs.len() as u32);
            }
        }
    }

    /// `P_SpawnSpecials` / `P_LoadSideDefs2` for what Boom's renderer reads:
    /// 242 fake planes and colormaps, 213 / 261 light transfers, 271 / 272
    /// sky transfers and 260 translucent lines.
    fn boom_specials(&mut self, map: &MapGeometry, data: &mut SoftData, vfs: &Vfs) {
        let tagged = |tag: i32| -> Vec<usize> {
            map.sectors
                .iter()
                .enumerate()
                .filter(|(_, s)| tag != 0 && s.tags.contains(&tag))
                .map(|(i, _)| i)
                .collect()
        };
        let doom = map.format == MapFormat::Doom;
        for (li, line) in map.lines.iter().enumerate() {
            let control = line.front_sector.map(|s| s.raw());
            let front_side = line.sides[0].map(|s| s.raw() as usize);
            match line.special {
                TRANSFER_HEIGHTS => {
                    let Some(control) = control else { continue };
                    for s in tagged(line.args[0]) {
                        if s as u32 != control {
                            self.heightsec[s] = Some(control);
                        }
                    }
                    // The control line's textures name colormaps.
                    if let Some(si) = front_side {
                        let side = &map.sides[si];
                        let names = [&side.lower, &side.middle, &side.upper];
                        let mut maps = [0u16; 3];
                        for (slot, name) in names.into_iter().enumerate() {
                            if let Some(n) = data.colormap_number(vfs, name) {
                                maps[slot] = n;
                                let sd = &mut self.sides[si];
                                match slot {
                                    0 => sd.bottom = 0,
                                    1 => sd.mid = 0,
                                    _ => sd.top = 0,
                                }
                            }
                        }
                        self.colormaps[control as usize] = maps;
                    }
                }
                TRANSFER_FLOOR_LIGHT | TRANSFER_CEILING_LIGHT => {
                    let Some(control) = control else { continue };
                    for s in tagged(line.args[0]) {
                        if line.special == TRANSFER_FLOOR_LIGHT {
                            self.floorlightsec[s] = Some(control);
                        } else {
                            self.ceilinglightsec[s] = Some(control);
                        }
                    }
                }
                STATIC_INIT if line.args[1] == INIT_TRANSFER_SKY => {
                    let Some(side) = front_side else { continue };
                    for s in tagged(line.args[0]) {
                        self.sky[s] = Some(SkyTransfer {
                            side: side as u32,
                            mirrored: line.args[2] == 0,
                        });
                    }
                }
                BOOM_SKY | BOOM_SKY_FLIPPED if doom => {
                    let Some(side) = front_side else { continue };
                    for s in tagged(i32::from(line.tag)) {
                        self.sky[s] = Some(SkyTransfer {
                            side: side as u32,
                            mirrored: line.special == BOOM_SKY,
                        });
                    }
                }
                BOOM_TRANSLUCENT if doom => {
                    // The front middle texture may name a translucency table.
                    let mut table = 0;
                    if let Some(si) = front_side {
                        if let Some(t) = data.tranmap_lump(vfs, &map.sides[si].middle) {
                            table = t;
                            self.sides[si].mid = 0;
                        }
                    }
                    data.ensure_tranmap(vfs);
                    if line.tag == 0 {
                        self.line_tranmap[li] = Some(table);
                    } else {
                        for (lj, other) in map.lines.iter().enumerate() {
                            if other.tag == line.tag {
                                self.line_tranmap[lj] = Some(table);
                            }
                        }
                    }
                }
                TRANSLUCENT_LINE => {
                    data.ensure_tranmap(vfs);
                    if line.args[0] == 0 {
                        self.line_tranmap[li] = Some(0);
                    } else {
                        for (lj, other) in map.lines.iter().enumerate() {
                            if i32::from(other.tag) == line.args[0]
                                || other.extra_ids.contains(&line.args[0])
                            {
                                self.line_tranmap[lj] = Some(0);
                            }
                        }
                    }
                }
                _ => {}
            }
            if line.mid_alpha < 0.99 && self.line_tranmap[li].is_none() {
                data.ensure_tranmap(vfs);
                self.line_tranmap[li] = Some(0);
            }
        }
    }

    /// Line, sector and plane portals from the map's portal tables (call
    /// again when `map.setup.portals.revision` changes).
    pub fn load_portals(&mut self, map: &MapGeometry) {
        use room_map::portals as p;
        let t = &map.setup.portals;
        self.portal_revision = t.revision;
        self.line_portals = vec![None; map.lines.len()];
        for lp in &t.lines {
            let li = lp.line.raw() as usize;
            let Some(line) = map.lines.get(li) else {
                continue;
            };
            let Some(front) = line.sides[0] else {
                continue;
            };
            let kind = if lp.is_mirror() {
                SoftLinePortal::Mirror
            } else if lp.is_horizon() {
                SoftLinePortal::Horizon
            } else if lp.is_visual() {
                let Some(dst) = lp.target.and_then(|d| map.lines.get(d.raw() as usize)) else {
                    continue;
                };
                let s1 = map.vertices[line.v1.raw() as usize].pos;
                let d2 = map.vertices[dst.v2.raw() as usize].pos;
                SoftLinePortal::Visual {
                    target: lp.target.map_or(0, |d| d.raw()),
                    s1: (s1.x(), s1.y()),
                    d2: (d2.x(), d2.y()),
                    cos: lp.cos_rot,
                    sin: lp.sin_rot,
                    angle: ((lp.angle_diff.rem_euclid(360.0) / 360.0) * 4_294_967_296.0) as u64
                        as u32,
                    align: lp.align,
                    sectors: (
                        line.front_sector.map_or(0, |s| s.raw()),
                        dst.front_sector.map_or(0, |s| s.raw()),
                    ),
                }
            } else {
                continue;
            };
            self.line_portals[li] = Some((kind, front.raw()));
        }
        // Sector portals: sky viewpoints and plane portals.
        self.sector_portals.clear();
        let mut index = std::collections::HashMap::new();
        for (i, sp) in t.sectors.iter().enumerate() {
            let soft = match sp.kind {
                p::PORTS_SKYVIEWPOINT => {
                    let Some(th) = sp.skybox.and_then(|k| t.things.get(&k)) else {
                        continue;
                    };
                    SoftSectorPortal::Skybox {
                        x: th.x,
                        y: th.y,
                        z_offset: th.z_offset,
                        sector: th.sector.map(|s| s.raw()),
                        angle: ((th.angle.rem_euclid(360.0) / 360.0) * 4_294_967_296.0) as u64
                            as u32,
                    }
                }
                p::PORTS_STACKEDSECTORTHING | p::PORTS_PORTAL | p::PORTS_LINKEDPORTAL => {
                    SoftSectorPortal::Stacked {
                        dx: sp.displacement.0,
                        dy: sp.displacement.1,
                    }
                }
                _ => continue,
            };
            index.insert(
                i as u32,
                (
                    self.sector_portals.len() as u32,
                    sp.flags & p::PORTSF_SKYFLATONLY != 0,
                ),
            );
            self.sector_portals.push(soft);
        }
        self.plane_portals = (0..map.sectors.len())
            .map(|s| {
                [0, 1].map(|plane| {
                    if t.plane_blocks_view(s, plane) {
                        return None;
                    }
                    index.get(&t.plane_portal_index(s, plane)).copied()
                })
            })
            .collect();
    }

    /// `Sector_Set3DFloor` (160) and `ExtraFloor_LightOnly` (50) lines.
    fn collect_ffloors(&mut self, map: &MapGeometry, data: &mut SoftData, vfs: &Vfs) {
        for line in &map.lines {
            let (typ, flags, alpha) = match line.special {
                SECTOR_SET_3DFLOOR => (
                    line.args[1],
                    line.args[2].max(0) as u32,
                    line.args[3].clamp(0, 255) as u8,
                ),
                // Light only: non-solid, invisible; mode 1 stops at the bottom.
                EXTRAFLOOR_LIGHT_ONLY => (
                    3,
                    if line.args[1] == 1 {
                        FF3D_DOUBLESHADOW
                    } else {
                        0
                    },
                    0,
                ),
                _ => continue,
            };
            let (tag, Some(control)) = (line.args[0], line.front_sector) else {
                continue;
            };
            if tag == 0 {
                continue;
            }
            let side = line.sides[0].map(|s| s.raw());
            if line.special == SECTOR_SET_3DFLOOR && alpha < 255 {
                data.ensure_tranmap(vfs);
            }
            for (i, sec) in map.sectors.iter().enumerate() {
                if sec.tags.contains(&tag) && i as u32 != control.raw() {
                    self.ffloors[i].push(SoftFFloor {
                        control: control.raw(),
                        typ,
                        flags,
                        alpha,
                        side,
                    });
                }
            }
        }
        if self.ffloors.iter().any(|f| !f.is_empty()) {
            self.build_subsector_shapes();
        }
    }

    /// Outline every subsector whose sector has 3D floors: walk the BSP
    /// from a box around the map, keeping each child's side of the
    /// partition, then cut the leaf by its own segs.
    fn build_subsector_shapes(&mut self) {
        self.subsector_shapes = vec![Vec::new(); self.subsectors.len()];
        let f = |v: i32| f64::from(v) / 65536.0;
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for v in &self.vertices {
            x0 = x0.min(f(v.x));
            y0 = y0.min(f(v.y));
            x1 = x1.max(f(v.x));
            y1 = y1.max(f(v.y));
        }
        if x0 > x1 {
            return;
        }
        let pad = 64.0;
        let start = vec![
            (x0 - pad, y0 - pad),
            (x1 + pad, y0 - pad),
            (x1 + pad, y1 + pad),
            (x0 - pad, y1 + pad),
        ];
        if self.nodes.is_empty() {
            self.finish_shape(0, start);
            return;
        }
        let mut stack = vec![((self.nodes.len() - 1) as u32, start)];
        while let Some((num, poly)) = stack.pop() {
            if let Some(leaf) = Self::leaf(num) {
                self.finish_shape(leaf, poly);
                continue;
            }
            let Some(n) = self.nodes.get(num as usize).copied() else {
                continue;
            };
            let (px, py, dx, dy) = (f(n.x), f(n.y), f(n.dx), f(n.dy));
            // Front (child 0) is the right of the partition.
            let front = clip_poly(&poly, px, py, dx, dy);
            let back = clip_poly(&poly, px, py, -dx, -dy);
            stack.push((n.children[0], front));
            stack.push((n.children[1], back));
        }
    }

    fn finish_shape(&mut self, leaf: usize, mut poly: Vec<(f64, f64)>) {
        let Some(&(sector, first, count)) = self.subsectors.get(leaf) else {
            return;
        };
        if self.ffloors.get(sector as usize).is_none_or(Vec::is_empty) {
            return;
        }
        let f = |v: i32| f64::from(v) / 65536.0;
        for i in first..first + count {
            let Some(seg) = self.segs.get(i as usize) else {
                continue;
            };
            let (a, b) = (
                self.vertices[seg.v1 as usize],
                self.vertices[seg.v2 as usize],
            );
            poly = clip_poly(&poly, f(a.x), f(a.y), f(b.x - a.x), f(b.y - a.y));
        }
        if poly.len() >= 3 {
            self.subsector_shapes[leaf] = poly;
        }
    }

    /// Leaf index of a child pointer.
    #[must_use]
    pub fn leaf(child: u32) -> Option<usize> {
        (child & NF_SUBSECTOR != 0).then_some((child & !NF_SUBSECTOR) as usize)
    }

    /// `R_PointInSubsector(x, y)->sector` (16.16 coordinates).
    #[must_use]
    pub fn point_in_sector(&self, x: i32, y: i32) -> Option<usize> {
        if self.nodes.is_empty() {
            return self.subsectors.first().map(|s| s.0 as usize);
        }
        let mut num = (self.nodes.len() - 1) as u32;
        loop {
            if let Some(leaf) = Self::leaf(num) {
                return self.subsectors.get(leaf).map(|s| s.0 as usize);
            }
            let n = self.nodes.get(num as usize)?;
            num = n.children[crate::render::point_on_line(x, y, n.x, n.y, n.dx, n.dy)];
        }
    }
}

/// The part of convex `poly` right of (or on) the line through `(px, py)`
/// along `(dx, dy)` (Sutherland–Hodgman).
fn clip_poly(poly: &[(f64, f64)], px: f64, py: f64, dx: f64, dy: f64) -> Vec<(f64, f64)> {
    if dx == 0.0 && dy == 0.0 {
        return poly.to_vec();
    }
    // Positive cross = left of the line (back side).
    let side = |(x, y): (f64, f64)| dx * (y - py) - dy * (x - px);
    let mut out = Vec::with_capacity(poly.len() + 1);
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 {
            out.push(a);
        }
        if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
            let t = sa / (sa - sb);
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    out
}

/// Flat id, with the sky flat mapped to [`SKY_FLAT`].
pub fn flat_id(data: &mut SoftData, vfs: &Vfs, name: &str) -> u16 {
    if name.trim().eq_ignore_ascii_case("F_SKY1") {
        SKY_FLAT
    } else {
        data.flat(vfs, name)
    }
}
