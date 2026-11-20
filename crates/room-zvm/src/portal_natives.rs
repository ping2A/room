//! ZScript access to portals: `Sector` portal queries, the portal-aware
//! `LevelLocals` vector helpers, and the portal-aware actor vector natives
//! (`Vec2To`, `Distance2D`, `AngleTo`, `Vec3Offset`, …).
//!
//! Ported from UZDoom (GPL-3.0-or-later): `sector_t::GetPortal*` /
//! `PortalBlocks*` / `PortalIsLinked`, `FLevelLocals::Vec2Diff` /
//! `Vec3Diff` / `Vec2Offset` / `Vec3Offset` / `GetPortalOffsetPosition`,
//! `AActor::PosRelative` / `Vec2To` / `Vec3Offset`.
//!
//! The VM never borrows the world: the playsim publishes a [`ZsPortalInfo`]
//! snapshot per level (and again when a portal changes). On maps without
//! portals every native answers as it did before (no displacement).
//!
//! Room's ZScript has no line objects (no `level.Lines`, `BlockingLine` or
//! `LineTrace`), so the `Line` portal queries have nothing to act on; line
//! portals still count for `Vec2Offset` / `Vec3Offset`.

use std::fmt;
use std::sync::Arc;

use crate::heap::{ObjHandle, ObjKind};
use crate::natives::{NativeRegs, NativeTable, ZvmCtx};
use crate::trap::Trap;
use crate::world_natives::{arg_a, arg_f, arg_i, pos_of, rad_to_deg, ret_bool, ret_f, ret_vec};

/// Point → sector index (the playsim's node lookup).
#[derive(Clone)]
pub struct SectorLookup(pub Arc<dyn Fn(f64, f64) -> Option<usize> + Send + Sync>);

impl fmt::Debug for SectorLookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SectorLookup")
    }
}

/// One sector plane's portal, as scripts see it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ZsPlanePortal {
    /// `PORTS_*` of the plane's portal (0 = sky viewpoint, the default).
    pub kind: i32,
    /// `GetPortalPlaneZ` (`f64::MAX` when not linked, as UZDoom's
    /// `FLT_MAX`).
    pub plane_z: f64,
    /// `GetPortalDisplacement`.
    pub displacement: (f64, f64),
    /// `GetOppositePortalGroup`.
    pub opposite_group: i32,
    /// `PortalIsLinked`.
    pub linked: bool,
    /// `PortalBlocksView` / `Sight` / `Movement` / `Sound`.
    pub blocks: [bool; 4],
}

/// One line portal: what `Vec2Offset` needs to carry a vector through it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ZsLinePortal {
    /// Origin line `v1`.
    pub v1: (f64, f64),
    /// Origin line `v2`.
    pub v2: (f64, f64),
    /// Destination line `v2` (rotating portals translate around it).
    pub dest_v2: (f64, f64),
    /// Linked portals: `dest.v2 − origin.v1`.
    pub displacement: (f64, f64),
    /// Rotation (`mSinRot`, `mCosRot`).
    pub sin_rot: f64,
    /// Rotation cosine.
    pub cos_rot: f64,
    /// `PORTT_LINKED`.
    pub linked: bool,
    /// `PORTF_INTERACTIVE`: vectors go through it.
    pub interactive: bool,
}

/// A line's portal as `Line.isLinePortal` & co. see it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZsLineInfo {
    /// Destination line.
    pub destination: Option<u32>,
    /// `PORTF_*` flags.
    pub flags: u8,
    /// `PORG_*` alignment.
    pub align: u8,
    /// `PORTF_VISIBLE` (`isVisualPortal`).
    pub visible: bool,
}

/// Portal data published by the playsim for the running level.
#[derive(Clone, Debug, Default)]
pub struct ZsPortalInfo {
    /// Portal groups (≥ 1 when set).
    pub group_count: usize,
    /// `Displacements(x, y)` at `x + group_count * y`.
    pub disp: Vec<Option<(f64, f64)>>,
    /// Group of each sector.
    pub sector_group: Vec<u32>,
    /// `[floor, ceiling]` portal of each sector.
    pub planes: Vec<[ZsPlanePortal; 2]>,
    /// Every traversable line portal with a destination.
    pub lines: Vec<ZsLinePortal>,
    /// The portal on each line (`None`: no portal).
    pub line_info: Vec<Option<ZsLineInfo>>,
    /// Point → sector.
    pub lookup: Option<SectorLookup>,
}

impl ZsPortalInfo {
    /// The map has linked portals (more than one group).
    #[must_use]
    pub fn has_links(&self) -> bool {
        self.group_count > 1
    }

    /// Sector at a point.
    #[must_use]
    pub fn sector_at(&self, x: f64, y: f64) -> Option<usize> {
        self.lookup.as_ref().and_then(|l| (l.0)(x, y))
    }

    /// Group at a point (0 off the map or without links).
    #[must_use]
    pub fn group_at(&self, x: f64, y: f64) -> u32 {
        if !self.has_links() {
            return 0;
        }
        self.sector_at(x, y)
            .and_then(|s| self.sector_group.get(s).copied())
            .unwrap_or(0)
    }

    /// `Displacements.getOffset(from, to)`.
    #[must_use]
    pub fn offset(&self, from: u32, to: u32) -> (f64, f64) {
        if from == to {
            return (0.0, 0.0);
        }
        let n = self.group_count;
        let (f, t) = (from as usize, to as usize);
        if f >= n || t >= n {
            return (0.0, 0.0);
        }
        self.disp
            .get(f + n * t)
            .copied()
            .flatten()
            .unwrap_or((0.0, 0.0))
    }

    /// `p` seen from the group at `from` (`PosRelative`).
    #[must_use]
    pub fn relative(&self, p: [f64; 3], from: [f64; 3]) -> [f64; 3] {
        if !self.has_links() {
            return p;
        }
        let (dx, dy) = self.offset(self.group_at(p[0], p[1]), self.group_at(from[0], from[1]));
        [p[0] + dx, p[1] + dy, p[2]]
    }

    /// `FLevelLocals::GetPortalOffsetPosition`: `(x, y) + (dx, dy)`, carried
    /// through the interactive line portals the vector crosses.
    #[must_use]
    pub fn offset_position(&self, x: f64, y: f64, dx: f64, dy: f64) -> (f64, f64) {
        let mut dest = (x + dx, y + dy);
        if self.lines.is_empty() {
            return dest;
        }
        let (mut ax, mut ay, mut ddx, mut ddy) = (x, y, dx, dy);
        for _ in 0..8 {
            let Some((p, frac)) = self.first_crossing(ax, ay, ddx, ddy) else {
                break;
            };
            if !p.interactive {
                break;
            }
            let mut hit = (ax + ddx * frac, ay + ddy * frac);
            if p.linked {
                hit = (hit.0 + p.displacement.0, hit.1 + p.displacement.1);
                dest = (dest.0 + p.displacement.0, dest.1 + p.displacement.1);
            } else {
                hit = translate_xy(p, hit);
                dest = translate_xy(p, dest);
            }
            ddx = dest.0 - hit.0;
            ddy = dest.1 - hit.1;
            ax = hit.0;
            ay = hit.1;
        }
        dest
    }

    /// The nearest portal line the vector crosses from its front to its back.
    fn first_crossing(&self, x: f64, y: f64, dx: f64, dy: f64) -> Option<(&ZsLinePortal, f64)> {
        let mut best: Option<(&ZsLinePortal, f64)> = None;
        for p in &self.lines {
            let side = |px: f64, py: f64| {
                (py - p.v1.1) * (p.v2.0 - p.v1.0) - (px - p.v1.0) * (p.v2.1 - p.v1.1) > 0.0
            };
            if side(x, y) || !side(x + dx, y + dy) {
                continue;
            }
            let (sx, sy) = (p.v2.0 - p.v1.0, p.v2.1 - p.v1.1);
            let den = dx * sy - dy * sx;
            if den.abs() < 1e-12 {
                continue;
            }
            let (qx, qy) = (p.v1.0 - x, p.v1.1 - y);
            let t = (qx * sy - qy * sx) / den;
            let u = (qx * dy - qy * dx) / den;
            if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
                continue;
            }
            if best.is_none_or(|(_, b)| t < b) {
                best = Some((p, t));
            }
        }
        best
    }
}

/// `P_TranslatePortalXY` for a rotating portal.
fn translate_xy(p: &ZsLinePortal, (x, y): (f64, f64)) -> (f64, f64) {
    let (nx, ny) = (x - p.v1.0, y - p.v1.1);
    (
        nx * p.cos_rot - ny * p.sin_rot + p.dest_v2.0,
        ny * p.cos_rot + nx * p.sin_rot + p.dest_v2.1,
    )
}

/// Heap class id of `level.PointInSector` references (see `natives.rs`).
const SECTOR_POINT_CLASS: u32 = u32::MAX - 7;

/// The sector a `Sector` receiver stands for: a `PointInSector` reference
/// names its point; any other one (`CurSector`) is the calling actor's.
pub(crate) fn receiver_sector(ctx: &ZvmCtx<'_>, h: ObjHandle) -> Option<usize> {
    if let Some(s) = ctx.play.map_objects.sector_of(h) {
        return Some(s);
    }
    let info = &ctx.play.portals;
    if let Some(ObjKind::Object {
        class_id: SECTOR_POINT_CLASS,
        fields_f,
        ..
    }) = ctx.heap.get(h)
    {
        let (x, y) = (fields_f.first().copied()?, fields_f.get(1).copied()?);
        return info.sector_at(x, y);
    }
    let p = ctx.play.caller_pos?;
    info.sector_at(p[0], p[1])
}

fn plane_of(ctx: &ZvmCtx<'_>, regs: &NativeRegs<'_>) -> Option<ZsPlanePortal> {
    let sector = receiver_sector(ctx, arg_a(regs, 0))?;
    let plane = usize::from(arg_i(regs, 0, 0) != 0);
    ctx.play.portals.planes.get(sector).map(|p| p[plane])
}

fn sector_portal_type(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let k = plane_of(ctx, regs).map_or(0, |p| p.kind);
    if let Some(slot) = regs.i.first_mut() {
        *slot = i64::from(k);
    }
    Ok(())
}

fn sector_portal_plane_z(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let z = plane_of(ctx, regs).map_or(f64::from(f32::MAX), |p| {
        if p.plane_z == f64::MAX {
            f64::from(f32::MAX)
        } else {
            p.plane_z
        }
    });
    ret_f(regs, z);
    Ok(())
}

fn sector_portal_displacement(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let d = plane_of(ctx, regs).map_or((0.0, 0.0), |p| p.displacement);
    ret_vec(regs, &[d.0, d.1]);
    Ok(())
}

fn sector_opposite_group(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let g = plane_of(ctx, regs).map_or(0, |p| p.opposite_group);
    if let Some(slot) = regs.i.first_mut() {
        *slot = i64::from(g);
    }
    Ok(())
}

fn blocks(ctx: &ZvmCtx<'_>, regs: &mut NativeRegs<'_>, which: usize) {
    // Unknown sector: as a plain plane (blocks all but the view).
    let b = plane_of(ctx, regs).map_or(which != 0, |p| p.blocks[which]);
    ret_bool(regs, b);
}

fn sector_blocks_view(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    blocks(ctx, regs, 0);
    Ok(())
}

fn sector_blocks_sight(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    blocks(ctx, regs, 1);
    Ok(())
}

fn sector_blocks_movement(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    blocks(ctx, regs, 2);
    Ok(())
}

fn sector_blocks_sound(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    blocks(ctx, regs, 3);
    Ok(())
}

fn sector_is_linked(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let l = plane_of(ctx, regs).is_some_and(|p| p.linked);
    ret_bool(regs, l);
    Ok(())
}

/// `LevelLocals.Vec2Diff(v1, v2)` / `Vec3Diff`: `v2 − v1` across portals.
fn level_vec_diff(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>, three: bool) {
    let n = if three { 3 } else { 2 };
    let a = [
        arg_f(regs, 0, 0.0),
        arg_f(regs, 1, 0.0),
        if three { arg_f(regs, 2, 0.0) } else { 0.0 },
    ];
    let b = [
        arg_f(regs, n, 0.0),
        arg_f(regs, n + 1, 0.0),
        if three { arg_f(regs, n + 2, 0.0) } else { 0.0 },
    ];
    let b = ctx.play.portals.relative(b, a);
    ret_vec(regs, &[b[0] - a[0], b[1] - a[1], b[2] - a[2]][..n]);
}

fn level_vec2_diff(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    level_vec_diff(ctx, regs, false);
    Ok(())
}

fn level_vec3_diff(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    level_vec_diff(ctx, regs, true);
    Ok(())
}

/// `LevelLocals.Vec2Offset(pos, dir, absolute)`.
fn level_vec2_offset(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (x, y, dx, dy) = (
        arg_f(regs, 0, 0.0),
        arg_f(regs, 1, 0.0),
        arg_f(regs, 2, 0.0),
        arg_f(regs, 3, 0.0),
    );
    let absolute = arg_i(regs, 0, 0) != 0;
    let (ox, oy) = if absolute {
        (x + dx, y + dy)
    } else {
        ctx.play.portals.offset_position(x, y, dx, dy)
    };
    ret_vec(regs, &[ox, oy]);
    Ok(())
}

/// `LevelLocals.Vec3Offset(pos, dir, absolute)`.
fn level_vec3_offset(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let p = [
        arg_f(regs, 0, 0.0),
        arg_f(regs, 1, 0.0),
        arg_f(regs, 2, 0.0),
    ];
    let d = [
        arg_f(regs, 3, 0.0),
        arg_f(regs, 4, 0.0),
        arg_f(regs, 5, 0.0),
    ];
    let absolute = arg_i(regs, 0, 0) != 0;
    let (ox, oy) = if absolute {
        (p[0] + d[0], p[1] + d[1])
    } else {
        ctx.play.portals.offset_position(p[0], p[1], d[0], d[1])
    };
    ret_vec(regs, &[ox, oy, p[2] + d[2]]);
    Ok(())
}

/// `LevelLocals.Vec2OffsetZ(pos, dir, atz, absolute)`.
fn level_vec2_offset_z(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (x, y, dx, dy, z) = (
        arg_f(regs, 0, 0.0),
        arg_f(regs, 1, 0.0),
        arg_f(regs, 2, 0.0),
        arg_f(regs, 3, 0.0),
        arg_f(regs, 4, 0.0),
    );
    let absolute = arg_i(regs, 0, 0) != 0;
    let (ox, oy) = if absolute {
        (x + dx, y + dy)
    } else {
        ctx.play.portals.offset_position(x, y, dx, dy)
    };
    ret_vec(regs, &[ox, oy, z]);
    Ok(())
}

/// `LevelLocals.GetDisplacement(pg1, pg2)`.
fn level_get_displacement(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let from = u32::try_from(arg_i(regs, 0, 0)).unwrap_or(0);
    let to = u32::try_from(arg_i(regs, 1, 0)).unwrap_or(0);
    let d = ctx.play.portals.offset(from, to);
    ret_vec(regs, &[d.0, d.1]);
    Ok(())
}

/// Self and other position, the other seen from self's portal group.
fn pair(ctx: &ZvmCtx<'_>, regs: &NativeRegs<'_>) -> Option<([f64; 3], [f64; 3])> {
    let me = pos_of(ctx, arg_a(regs, 0))?;
    let other = arg_a(regs, 1);
    if other.is_null() {
        return None;
    }
    let o = pos_of(ctx, other)?;
    Some((me, ctx.play.portals.relative(o, me)))
}

fn distance(ctx: &ZvmCtx<'_>, regs: &mut NativeRegs<'_>, three_d: bool, squared: bool) {
    let Some((a, b)) = pair(ctx, regs) else {
        ret_f(regs, 0.0);
        return;
    };
    let (dx, dy, dz) = (
        b[0] - a[0],
        b[1] - a[1],
        if three_d { b[2] - a[2] } else { 0.0 },
    );
    let sq = dx * dx + dy * dy + dz * dz;
    ret_f(
        regs,
        if squared {
            sq
        } else {
            room_core::dmath::sqrt(sq)
        },
    );
}

fn distance_2d(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    distance(ctx, regs, false, false);
    Ok(())
}

fn distance_3d(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    distance(ctx, regs, true, false);
    Ok(())
}

fn distance_2d_sq(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    distance(ctx, regs, false, true);
    Ok(())
}

fn distance_3d_sq(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    distance(ctx, regs, true, true);
    Ok(())
}

/// `AngleTo(target, absolute)`: through portals unless `absolute`.
fn angle_to(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let Some((a, mut b)) = pair(ctx, regs) else {
        ret_f(regs, 0.0);
        return Ok(());
    };
    if arg_i(regs, 0, 0) != 0 {
        b = pos_of(ctx, arg_a(regs, 1)).unwrap_or(b);
    }
    ret_f(
        regs,
        rad_to_deg(room_core::dmath::atan2(b[1] - a[1], b[0] - a[0])),
    );
    Ok(())
}

/// `Vec2To(other)` / `Vec3To(other)`.
fn vec_to(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let Some((a, b)) = pair(ctx, regs) else {
        ret_vec(regs, &[0.0, 0.0, 0.0]);
        return Ok(());
    };
    ret_vec(regs, &[b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
    Ok(())
}

/// `Vec3Offset(x, y, z, absolute)` / `Vec2Offset(x, y, absolute)`.
fn vec3_offset(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let p = pos_of(ctx, arg_a(regs, 0)).unwrap_or([0.0; 3]);
    let d = [
        arg_f(regs, 0, 0.0),
        arg_f(regs, 1, 0.0),
        arg_f(regs, 2, 0.0),
    ];
    let (x, y) = if arg_i(regs, 0, 0) != 0 {
        (p[0] + d[0], p[1] + d[1])
    } else {
        ctx.play.portals.offset_position(p[0], p[1], d[0], d[1])
    };
    ret_vec(regs, &[x, y, p[2] + d[2]]);
    Ok(())
}

/// `Vec2OffsetZ(x, y, z, absolute)`.
fn vec2_offset_z(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let p = pos_of(ctx, arg_a(regs, 0)).unwrap_or([0.0; 3]);
    let (dx, dy, z) = (
        arg_f(regs, 0, 0.0),
        arg_f(regs, 1, 0.0),
        arg_f(regs, 2, 0.0),
    );
    let (x, y) = if arg_i(regs, 0, 0) != 0 {
        (p[0] + dx, p[1] + dy)
    } else {
        ctx.play.portals.offset_position(p[0], p[1], dx, dy)
    };
    ret_vec(regs, &[x, y, z]);
    Ok(())
}

/// `Vec3Angle(length, angle, z, absolute)`.
fn vec3_angle(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let p = pos_of(ctx, arg_a(regs, 0)).unwrap_or([0.0; 3]);
    let (len, ang, z) = (
        arg_f(regs, 0, 0.0),
        arg_f(regs, 1, 0.0),
        arg_f(regs, 2, 0.0),
    );
    let r = ang.to_radians();
    let (dx, dy) = (
        room_core::dmath::cos(r) * len,
        room_core::dmath::sin(r) * len,
    );
    let (x, y) = if arg_i(regs, 0, 0) != 0 {
        (p[0] + dx, p[1] + dy)
    } else {
        ctx.play.portals.offset_position(p[0], p[1], dx, dy)
    };
    ret_vec(regs, &[x, y, p[2] + z]);
    Ok(())
}

/// `Vec2Angle(length, angle, absolute)`.
fn vec2_angle(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let p = pos_of(ctx, arg_a(regs, 0)).unwrap_or([0.0; 3]);
    let (len, ang) = (arg_f(regs, 0, 0.0), arg_f(regs, 1, 0.0));
    let r = ang.to_radians();
    let (dx, dy) = (
        room_core::dmath::cos(r) * len,
        room_core::dmath::sin(r) * len,
    );
    let (x, y) = if arg_i(regs, 0, 0) != 0 {
        (p[0] + dx, p[1] + dy)
    } else {
        ctx.play.portals.offset_position(p[0], p[1], dx, dy)
    };
    ret_vec(regs, &[x, y]);
    Ok(())
}

/// `PosRelative(sector)`: the actor's position in that sector's group.
fn pos_relative(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let p = pos_of(ctx, arg_a(regs, 0)).unwrap_or([0.0; 3]);
    let info = &ctx.play.portals;
    let out = match receiver_sector_arg(ctx, arg_a(regs, 1)) {
        Some(s) if info.has_links() => {
            let to = info.sector_group.get(s).copied().unwrap_or(0);
            let (dx, dy) = info.offset(info.group_at(p[0], p[1]), to);
            [p[0] + dx, p[1] + dy, p[2]]
        }
        _ => p,
    };
    ret_vec(regs, &out);
    Ok(())
}

/// A `Sector` argument (not the receiver): only `PointInSector` references
/// name a sector.
fn receiver_sector_arg(ctx: &ZvmCtx<'_>, h: ObjHandle) -> Option<usize> {
    if let Some(s) = ctx.play.map_objects.sector_of(h) {
        return Some(s);
    }
    match ctx.heap.get(h)? {
        ObjKind::Object {
            class_id: SECTOR_POINT_CLASS,
            fields_f,
            ..
        } => ctx
            .play
            .portals
            .sector_at(fields_f.first().copied()?, fields_f.get(1).copied()?),
        _ => None,
    }
}

/// Install the portal natives (after the world and UZ3 sets, which they
/// replace).
pub(crate) fn register(t: &mut NativeTable) {
    t.replace("sector.getportaltype", sector_portal_type);
    t.replace("sector.getportalplanez", sector_portal_plane_z);
    t.replace("sector.getportaldisplacement", sector_portal_displacement);
    t.replace("sector.getoppositeportalgroup", sector_opposite_group);
    t.replace("sector.portalblocksview", sector_blocks_view);
    t.replace("sector.portalblockssight", sector_blocks_sight);
    t.replace("sector.portalblocksmovement", sector_blocks_movement);
    t.replace("sector.portalblockssound", sector_blocks_sound);
    t.replace("sector.portalislinked", sector_is_linked);
    t.replace("levellocals.vec2diff", level_vec2_diff);
    t.replace("levellocals.vec3diff", level_vec3_diff);
    t.replace("levellocals.vec2offset", level_vec2_offset);
    t.replace("levellocals.vec3offset", level_vec3_offset);
    t.replace("levellocals.vec2offsetz", level_vec2_offset_z);
    t.replace("levellocals.getdisplacement", level_get_displacement);
    t.replace("actor.distance2d", distance_2d);
    t.replace("actor.distance3d", distance_3d);
    t.replace("actor.distance2dsquared", distance_2d_sq);
    t.replace("actor.distance3dsquared", distance_3d_sq);
    t.replace("actor.angleto", angle_to);
    t.replace("actor.vec2to", vec_to);
    t.replace("actor.vec3to", vec_to);
    t.replace("actor.vec3offset", vec3_offset);
    t.replace("actor.vec2offset", vec3_offset);
    t.replace("actor.vec2offsetz", vec2_offset_z);
    t.replace("actor.vec3angle", vec3_angle);
    t.replace("actor.vec2angle", vec2_angle);
    t.replace("actor.posrelative", pos_relative);
}
