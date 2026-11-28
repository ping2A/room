//! ZScript map objects: `level.Sectors`, `level.Lines`, `level.Sides` and
//! `level.Vertexes` as real `Sector` / `Line` / `Side` / `Vertex` objects,
//! `Sector.floorplane` / `ceilingplane`, `CurSector`, and the natives that
//! act on them (`Index`, light level, plane height, flats, line portals).
//!
//! Ported from UZDoom (GPL-3.0-or-later): `sector_t`, `line_t`, `side_t`,
//! `vertex_t` and `secplane_t` script exports (`vmthunks.cpp`).
//!
//! The playsim builds the objects once per map in the VM heap and refreshes
//! what moves (plane heights, light, flats) once per tic. Writes that the
//! playsim must see (`SetLightLevel`, `SetTexture`, `SetPlaneLight`) are
//! queued as [`crate::ZsActorOp`]s.

use std::collections::BTreeMap;

use crate::heap::ObjHandle;
use crate::natives::{NativeRegs, NativeTable, ZsActorOp, ZvmCtx};
use crate::trap::Trap;
use crate::world_natives::{arg_a, arg_f, arg_i, ret_f};

/// Kind of a map object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZsMapKind {
    /// `Sector`.
    Sector,
    /// `Line`.
    Line,
    /// `Side`.
    Side,
    /// `Vertex`.
    Vertex,
    /// `SecPlane` of a sector (0 floor, 1 ceiling).
    Plane(u8),
}

/// A sector plane as `z = z0 + gx·x + gy·y`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ZsPlane {
    /// Height at the map origin.
    pub z0: f64,
    /// Slope along x.
    pub gx: f64,
    /// Slope along y.
    pub gy: f64,
}

impl ZsPlane {
    /// `secplane_t::ZatPoint`.
    #[must_use]
    pub fn z_at(&self, x: f64, y: f64) -> f64 {
        self.z0 + self.gx * x + self.gy * y
    }
}

/// The map objects of the running level (handles into this VM's heap).
#[derive(Clone, Debug, Default)]
pub struct ZsMapObjects {
    /// The `LevelLocals` object `level` names.
    pub level: ObjHandle,
    /// `level.Sectors`.
    pub sectors: Vec<ObjHandle>,
    /// `level.Lines`.
    pub lines: Vec<ObjHandle>,
    /// `level.Sides`.
    pub sides: Vec<ObjHandle>,
    /// `level.Vertexes`.
    pub vertices: Vec<ObjHandle>,
    /// What each handle is, and its index.
    pub index: BTreeMap<ObjHandle, (ZsMapKind, u32)>,
    /// `[floor, ceiling]` plane of each sector (refreshed per tic).
    pub planes: Vec<[ZsPlane; 2]>,
    /// Light level of each sector (refreshed per tic).
    pub light: Vec<i32>,
    /// `[floor, ceiling]` flat of each sector (refreshed per tic).
    pub flats: Vec<[String; 2]>,
}

impl ZsMapObjects {
    /// What `h` is, when it is a map object.
    #[must_use]
    pub fn kind_of(&self, h: ObjHandle) -> Option<(ZsMapKind, u32)> {
        if h.is_null() {
            return None;
        }
        self.index.get(&h).copied()
    }

    /// Sector index of a `Sector` object.
    #[must_use]
    pub fn sector_of(&self, h: ObjHandle) -> Option<usize> {
        match self.kind_of(h)? {
            (ZsMapKind::Sector, i) => Some(i as usize),
            _ => None,
        }
    }
}

fn set_ret_a(regs: &mut NativeRegs<'_>, h: ObjHandle) {
    if let Some(slot) = regs.a.first_mut() {
        *slot = h;
    }
}

fn set_ret_i(regs: &mut NativeRegs<'_>, v: i64) {
    if let Some(slot) = regs.i.first_mut() {
        *slot = v;
    }
    if let Some(slot) = regs.f.first_mut() {
        *slot = v as f64;
    }
}

/// `Sector.Index()`, `Line.Index()`, `Side.Index()`, `Vertex.Index()`:
/// −1 for anything that is not a map object.
fn map_index(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let v = ctx
        .play
        .map_objects
        .kind_of(arg_a(regs, 0))
        .map_or(-1, |(_, i)| i64::from(i));
    set_ret_i(regs, v);
    Ok(())
}

/// The sector a `Sector` receiver stands for: a map object, else the
/// calling actor's (`CurSector` before the map objects existed).
pub(crate) fn sector_index(ctx: &ZvmCtx<'_>, h: ObjHandle) -> Option<usize> {
    if let Some(s) = ctx.play.map_objects.sector_of(h) {
        return Some(s);
    }
    crate::portal_natives::receiver_sector(ctx, h)
}

fn sector_get_light(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let v = sector_index(ctx, arg_a(regs, 0))
        .and_then(|s| ctx.play.map_objects.light.get(s).copied())
        .unwrap_or(0);
    set_ret_i(regs, i64::from(v));
    Ok(())
}

fn queue_light(ctx: &mut ZvmCtx<'_>, sector: usize, level: i32) {
    if let Some(l) = ctx.play.map_objects.light.get_mut(sector) {
        *l = level;
    }
    ctx.play.actor_ops.push(ZsActorOp::SectorLight {
        sector: sector as u32,
        level,
    });
}

/// `Sector.SetLightLevel(level)`.
fn sector_set_light(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    if let Some(s) = sector_index(ctx, arg_a(regs, 0)) {
        let level = i32::try_from(arg_i(regs, 0, 0)).unwrap_or(0).clamp(0, 255);
        queue_light(ctx, s, level);
    }
    Ok(())
}

/// `Sector.ChangeLightLevel(delta)`.
fn sector_change_light(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    if let Some(s) = sector_index(ctx, arg_a(regs, 0)) {
        let cur = ctx.play.map_objects.light.get(s).copied().unwrap_or(0);
        let delta = i32::try_from(arg_i(regs, 0, 0)).unwrap_or(0);
        queue_light(ctx, s, (cur + delta).clamp(0, 255));
    }
    Ok(())
}

/// `Sector.SetPlaneLight(plane, level)`: Room has no per-plane light, so the
/// sector's light carries it.
fn sector_set_plane_light(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    if let Some(s) = sector_index(ctx, arg_a(regs, 0)) {
        let level = i32::try_from(arg_i(regs, 1, 0)).unwrap_or(0).clamp(0, 255);
        queue_light(ctx, s, level);
    }
    Ok(())
}

/// `Sector.GetTexture(plane)`.
fn sector_get_texture(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let plane = usize::from(arg_i(regs, 0, 0) != 0);
    let name = sector_index(ctx, arg_a(regs, 0))
        .and_then(|s| ctx.play.map_objects.flats.get(s))
        .map(|f| f[plane].clone());
    let id = name.map_or(-1, |n| ctx.play.intern_texture(&n));
    set_ret_i(regs, id);
    Ok(())
}

/// `Sector.SetTexture(plane, tex, floorclip)`.
fn sector_set_texture(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let Some(s) = sector_index(ctx, arg_a(regs, 0)) else {
        return Ok(());
    };
    let plane = usize::from(arg_i(regs, 0, 0) != 0);
    let Some(name) = ctx.play.texture_name(arg_i(regs, 1, 0)).map(str::to_owned) else {
        return Ok(());
    };
    if let Some(f) = ctx.play.map_objects.flats.get_mut(s) {
        f[plane] = name.clone();
    }
    ctx.play.actor_ops.push(ZsActorOp::SectorTexture {
        sector: s as u32,
        ceiling: plane == 1,
        name,
    });
    Ok(())
}

/// `SecPlane.ZatPoint(pos)`.
fn secplane_z_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (x, y) = (arg_f(regs, 0, 0.0), arg_f(regs, 1, 0.0));
    let z = match ctx.play.map_objects.kind_of(arg_a(regs, 0)) {
        Some((ZsMapKind::Plane(p), s)) => ctx
            .play
            .map_objects
            .planes
            .get(s as usize)
            .map_or(0.0, |pl| pl[usize::from(p & 1)].z_at(x, y)),
        _ => 0.0,
    };
    ret_f(regs, z);
    Ok(())
}

/// `Sector.LowestFloorAt` / `HighestCeilingAt`-style height queries on a
/// sector (its own plane; portals aside).
fn sector_floor_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    sector_plane_at(ctx, regs, 0);
    Ok(())
}

/// `Sector.NextLowestFloorAt(x, y, z, …)` → `(double, Sector, F3DFloor)`:
/// the sector's own floor there and the sector itself (3D floors and
/// portals aside), so `RealFloor.FloorPlane` is not null.
fn sector_next_lowest_floor_at(
    ctx: &mut ZvmCtx<'_>,
    regs: &mut NativeRegs<'_>,
) -> Result<(), Trap> {
    let sector = arg_a(regs, 0);
    sector_plane_at(ctx, regs, 0);
    let z = regs.f.first().copied().unwrap_or(0.0);
    ctx.play.multi_ret = vec![
        crate::interp::Value::F(z),
        crate::interp::Value::A(sector),
        crate::interp::Value::A(ObjHandle::NULL),
    ];
    Ok(())
}

fn sector_ceiling_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    sector_plane_at(ctx, regs, 1);
    Ok(())
}

fn sector_plane_at(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>, plane: usize) {
    let (x, y) = (arg_f(regs, 0, 0.0), arg_f(regs, 1, 0.0));
    let z = sector_index(ctx, arg_a(regs, 0))
        .and_then(|s| ctx.play.map_objects.planes.get(s))
        .map_or(0.0, |pl| pl[plane].z_at(x, y));
    ret_f(regs, z);
}

/// The portal on a line (`linePortals[portalindex]`).
fn line_portal(ctx: &ZvmCtx<'_>, regs: &NativeRegs<'_>) -> Option<crate::ZsLineInfo> {
    let (kind, i) = ctx.play.map_objects.kind_of(arg_a(regs, 0))?;
    if kind != ZsMapKind::Line {
        return None;
    }
    ctx.play
        .portals
        .line_info
        .get(i as usize)
        .copied()
        .flatten()
}

/// `Line.isLinePortal()`.
fn line_is_portal(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let v = line_portal(ctx, regs).is_some();
    set_ret_i(regs, i64::from(v));
    Ok(())
}

/// `Line.isVisualPortal()`.
fn line_is_visual_portal(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let v = line_portal(ctx, regs).is_some_and(|p| p.visible);
    set_ret_i(regs, i64::from(v));
    Ok(())
}

/// `Line.getPortalDestination()`.
fn line_portal_destination(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let h = line_portal(ctx, regs)
        .and_then(|p| p.destination)
        .and_then(|d| ctx.play.map_objects.lines.get(d as usize).copied())
        .unwrap_or(ObjHandle::NULL);
    set_ret_a(regs, h);
    Ok(())
}

/// `Line.getPortalFlags()`.
fn line_portal_flags(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let v = line_portal(ctx, regs).map_or(0, |p| i64::from(p.flags));
    set_ret_i(regs, v);
    Ok(())
}

/// `Line.getPortalAlignment()`.
fn line_portal_alignment(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let v = line_portal(ctx, regs).map_or(0, |p| i64::from(p.align));
    set_ret_i(regs, v);
    Ok(())
}

/// `Team.GetPlayerColor()` / `GetTextColor()` / `GetLogo()`: Room has no
/// TEAMINFO teams, so every team answers as UZDoom's unset defaults.
fn team_none(_ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    set_ret_i(regs, 0);
    Ok(())
}

/// Install the map-object natives (after the other sets, which they
/// replace).
pub(crate) fn register(t: &mut NativeTable) {
    for name in ["sector.index", "line.index", "side.index", "vertex.index"] {
        t.replace(name, map_index);
    }
    t.replace("sector.getlightlevel", sector_get_light);
    t.replace("sector.setlightlevel", sector_set_light);
    t.replace("sector.changelightlevel", sector_change_light);
    t.replace("sector.setplanelight", sector_set_plane_light);
    t.replace("sector.gettexture", sector_get_texture);
    t.replace("sector.settexture", sector_set_texture);
    t.replace("sector.lowestfloorat", sector_floor_at);
    t.replace("sector.nextlowestfloorat", sector_next_lowest_floor_at);
    t.replace("sector.highestceilingat", sector_ceiling_at);
    t.replace("secplane.zatpoint", secplane_z_at);
    t.replace("line.islineportal", line_is_portal);
    t.replace("line.isvisualportal", line_is_visual_portal);
    t.replace("line.getportaldestination", line_portal_destination);
    t.replace("line.getportalflags", line_portal_flags);
    t.replace("line.getportalalignment", line_portal_alignment);
    for name in ["team.getplayercolor", "team.gettextcolor", "team.getlogo"] {
        t.replace(name, team_none);
    }
}
