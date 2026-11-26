//! Front-to-back portal renderer for the textured first-person view.
//!
//! Classic Build draws a sector, then the sectors visible through its red
//! (portal) walls, clipping every column to the still-open window
//! (`umost`/`dmost`). This module follows that model:
//!
//! - each sector draws its own ceiling and floor (slopes and parallax sky
//!   included) between the open window and its nearest wall in that column;
//! - walls are clipped against the near plane instead of being dropped, and
//!   textured with perspective-correct `u`;
//! - portal walls draw their upper/lower bands, narrow the window, then the
//!   neighbour sector is drawn in those columns only;
//! - masked portal walls (cstat 16) are drawn last, nearest last, with the
//!   per-pixel depth test that sprites already use.
//!
//! It returns that per-pixel depth buffer (camera-forward distance in Build
//! XY units, `i32::MAX` = empty) for the sprite and mirror passes.

/// Atomic `MIRROR` tile.
const MIRROR_TILE: i16 = 560;

use super::{SoftFrame, SoftTiles, TranslucentPixel};
use crate::geom::{SectorZ, WallXY, getzsofslope};
use crate::soft::STAT_PARALLAX;
use crate::view::BuildView;
use crate::world::BuildWorld;

/// Near clip plane, in Build XY units in front of the eye.
const NEAR: i64 = 8;
/// Recursion guard for pathological portal loops.
const MAX_DEPTH: u32 = 256;

/// One projected, near-clipped wall of the sector being drawn.
#[derive(Clone, Copy)]
struct Seg {
    wall: usize,
    /// Screen columns covered: `[x0, x1)`.
    x0: i32,
    x1: i32,
    /// Screen x (Q16) and camera depth of the clipped endpoints.
    sxa: i64,
    sxb: i64,
    /// 1/z (Q30) at both clipped endpoints.
    iza: i64,
    izb: i64,
    /// Fraction along the original wall (Q16) at both clipped endpoints.
    ta: i64,
    tb: i64,
}

struct MaskedCol {
    wall: usize,
    sect: i16,
    /// Build `drawmaskwall` reference: lower ceiling, or higher floor (cstat 4).
    zref: i32,
    col: i32,
    y0: i32,
    y1: i32,
    depth: i32,
    t_q16: i64,
}

/// Prevent malformed portal maps from accumulating an unbounded fragment list.
const MAX_TRANSLUCENT_WALL_PIXELS: usize = 2_000_000;

struct Ctx<'a> {
    world: &'a BuildWorld,
    view: &'a BuildView,
    tiles: &'a SoftTiles,
    frame: &'a mut SoftFrame,
    zbuf: Vec<i32>,
    umost: Vec<i32>,
    dmost: Vec<i32>,
    /// Depth at which the ray entered the sector now drawn in each column;
    /// walls nearer than this belong to an earlier pass of a concave sector.
    entry: Vec<i64>,
    sin: i64,
    cos: i64,
    w: i32,
    h: i32,
    cx: i32,
    focal_x: i64,
    focal_y: i64,
    horizon: i32,
    clock: u32,
    sector_z: Vec<SectorZ>,
    wall_xy: Vec<WallXY>,
    masked: Vec<MaskedCol>,
    translucent: Vec<TranslucentPixel>,
    visits: u32,
    /// Sectors drawn at least once, first-visit order.
    drawn: Vec<i16>,
    drawn_mark: Vec<bool>,
}

/// Draw walls, floors and ceilings; return the per-pixel depth buffer and
/// the sectors that were drawn.
pub(super) fn render(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: &SoftTiles,
    clock_120hz: u32,
) -> (Vec<i32>, Vec<i16>, Vec<TranslucentPixel>) {
    let w = frame.width as i32;
    let h = frame.height as i32;
    let (sin, cos) = crate::helpers::sincos_q14_16(view.ang_q16);
    let horiz_off = crate::soft::horiz_pixel_offset_q16(view.horiz_q16, i64::from((h / 2).max(1)));
    let mut ctx = Ctx {
        world,
        view,
        tiles,
        zbuf: vec![i32::MAX; (w.max(0) as usize) * (h.max(0) as usize)],
        umost: vec![0; w.max(0) as usize],
        dmost: vec![h; w.max(0) as usize],
        entry: vec![0; w.max(0) as usize],
        frame,
        sin: i64::from(sin),
        cos: i64::from(cos),
        w,
        h,
        cx: w / 2,
        focal_x: i64::from((w / 2).max(1)),
        focal_y: i64::from((h / 2).max(1)),
        horizon: h / 2 + horiz_off,
        clock: clock_120hz,
        sector_z: world.sector_z(),
        wall_xy: world.wall_xy(),
        masked: Vec::new(),
        translucent: Vec::new(),
        visits: 0,
        drawn: Vec::new(),
        drawn_mark: vec![false; usize::try_from(world.numsectors).unwrap_or(0)],
    };
    if w <= 0 || h <= 0 || view.sect < 0 || view.sect >= world.numsectors {
        return (ctx.zbuf, Vec::new(), Vec::new());
    }
    ctx.draw_sector(view.sect, 0, w, 0);
    if std::env::var_os("ROOM_PORTAL_DEBUG").is_some() {
        eprintln!(
            "portal: start sect {} pos ({}, {}, {}) visits {} drawn {:?}",
            view.sect, view.posx, view.posy, view.posz, ctx.visits, ctx.drawn
        );
    }
    ctx.draw_masked();
    (ctx.zbuf, ctx.drawn, ctx.translucent)
}

/// Add deferred wall layers to the shared pixel compositor. Sprite fragments
/// submitted afterward are sorted with these by the same camera depth.
pub(super) fn enqueue_translucent(frame: &mut SoftFrame, pixels: Vec<TranslucentPixel>) {
    for pixel in pixels {
        let _ = frame.defer_translucent_pixel(TranslucentPixel {
            blend_table: 0,
            ..pixel
        });
    }
}

impl Ctx<'_> {
    /// Camera-space (right, forward) of a world XY point.
    fn to_cam(&self, x: i32, y: i32) -> (i64, i64) {
        let dx = i64::from(x) - i64::from(self.view.posx);
        let dy = i64::from(y) - i64::from(self.view.posy);
        // Forward is (cos, sin); screen right is (-sin, cos) (Y points south).
        let rz = (dx * self.cos + dy * self.sin) >> 14;
        let rx = (-dx * self.sin + dy * self.cos) >> 14;
        (rx, rz)
    }

    /// Build distance fog: `getpalookup(mulscale(depth, globvis), shade)`.
    /// Duke's default `visibility` is 512; a non-zero sector `visibility`
    /// scales it by `(v + 16) / 16` (u8 wrap, as Build reads it).
    fn fog_shade(&self, shade: i8, depth: i64, sect: i16) -> i8 {
        let v = self
            .world
            .sectors
            .get(sect as usize)
            .map_or(0, |s| s.visibility);
        // Global visibility (`g_visibility`, 512 by default), scaled by a
        // nonzero sector visibility as Build `mulscale4(globvis, vis + 16)`.
        let global = i64::from(self.world.render_visibility.max(0));
        let globvis = if v == 0 {
            global
        } else {
            (global * (i64::from(v.wrapping_add(16)))) >> 4
        };
        // Build `wallscan`: `swall ≈ 32·depth`, so `mulscale16(swall, globvis) >> 8`
        // adds one shade per 1024 units at visibility 512.
        let add = (depth.max(0) * globvis) >> 19;
        (i64::from(shade) + add).clamp(0, 127) as i8
    }

    /// Screen row of world height `z` at camera depth `depth`.
    fn row_of(&self, z: i32, depth: i64) -> i64 {
        let up = (i64::from(self.view.posz) - i64::from(z)) >> 4;
        i64::from(self.horizon) - (self.focal_y * up) / depth.max(1)
    }

    fn collect_segs(&self, sect: i16) -> Vec<Seg> {
        let sec = &self.world.sectors[sect as usize];
        let start = usize::try_from(sec.wallptr).unwrap_or(usize::MAX);
        let end = start
            .saturating_add(usize::try_from(sec.wallnum).unwrap_or(0))
            .min(self.world.walls.len());
        let mut segs = Vec::new();
        for wi in start..end {
            let a = &self.world.walls[wi];
            let Some(b) = usize::try_from(a.point2)
                .ok()
                .and_then(|p| self.world.walls.get(p))
            else {
                continue;
            };
            let (mut ax, mut az) = self.to_cam(a.x, a.y);
            let (mut bx, mut bz) = self.to_cam(b.x, b.y);
            let (mut ta, mut tb) = (0_i64, 1_i64 << 16);
            // A portal line through the eye projects to zero width. When the
            // camera stands on a two-sided wall (inside its span) and looks
            // across it, the portal is the whole screen: Build keeps drawing
            // the next sector there instead of going black.
            if a.nextsector >= 0 {
                let (ex, ez) = (bx - ax, bz - az);
                let len2 = ex * ex + ez * ez;
                let cross_eye = ex * (-az) - ez * (-ax);
                let along = (-ax) * ex + (-az) * ez;
                let forward = ex * (NEAR - az) - ez * (-ax);
                if len2 > 0
                    && cross_eye.abs() * cross_eye.abs() <= 4 * len2
                    && along > 0
                    && along < len2
                    && forward > 0
                {
                    let iz = (1_i64 << 30) / NEAR;
                    segs.push(Seg {
                        wall: wi,
                        x0: 0,
                        x1: self.w,
                        sxa: 0,
                        sxb: i64::from(self.w) << 16,
                        iza: iz,
                        izb: iz,
                        ta: 0,
                        tb: 1 << 16,
                    });
                    continue;
                }
            }
            if az < NEAR && bz < NEAR {
                continue;
            }
            // Clip to the near plane, keeping the wall fraction for texturing.
            if az < NEAR {
                let f = ((NEAR - az) << 16) / (bz - az);
                ax += ((bx - ax) * f) >> 16;
                az = NEAR;
                ta = f;
            } else if bz < NEAR {
                let f = ((NEAR - bz) << 16) / (az - bz);
                bx += ((ax - bx) * f) >> 16;
                bz = NEAR;
                tb = (1 << 16) - f;
            }
            let sxa = (i64::from(self.cx) << 16) + ((self.focal_x * ax) << 16) / az;
            let sxb = (i64::from(self.cx) << 16) + ((self.focal_x * bx) << 16) / bz;
            // Build loops run clockwise: seen from inside the sector a wall
            // goes left to right on screen. Otherwise it is the back face.
            if sxb <= sxa {
                continue;
            }
            let x0 = (((sxa + 0xFFFF) >> 16).max(0)) as i32;
            let x1 = (((sxb + 0xFFFF) >> 16).min(i64::from(self.w))) as i32;
            if x1 <= x0 {
                continue;
            }
            segs.push(Seg {
                wall: wi,
                x0,
                x1,
                sxa,
                sxb,
                iza: (1_i64 << 30) / az,
                izb: (1_i64 << 30) / bz,
                ta,
                tb,
            });
        }
        segs
    }

    /// Depth and wall fraction (Q16) of `seg` at screen column `col`.
    fn seg_at(seg: &Seg, col: i32) -> (i64, i64) {
        let span = (seg.sxb - seg.sxa).max(1);
        let p = (((i64::from(col) << 16) + 0x8000 - seg.sxa) << 16) / span;
        let p = p.clamp(0, 1 << 16);
        let iz = seg.iza + (((seg.izb - seg.iza) * p) >> 16);
        let tz = ((seg.ta * seg.iza) >> 16)
            + (((((seg.tb * seg.izb) >> 16) - ((seg.ta * seg.iza) >> 16)) * p) >> 16);
        let iz = iz.max(1);
        let depth = (1_i64 << 30) / iz;
        let t = ((tz << 16) / iz).clamp(0, 1 << 16);
        (depth, t)
    }

    fn wall_point(&self, wall: usize, t: i64) -> (i32, i32) {
        let a = &self.world.walls[wall];
        let b = &self.world.walls[a.point2 as usize];
        let x = i64::from(a.x) + (((i64::from(b.x) - i64::from(a.x)) * t) >> 16);
        let y = i64::from(a.y) + (((i64::from(b.y) - i64::from(a.y)) * t) >> 16);
        (x as i32, y as i32)
    }

    fn planes_at(&self, sect: i16, x: i32, y: i32) -> (i32, i32) {
        getzsofslope(sect, x, y, &self.sector_z, &self.wall_xy)
    }

    fn draw_sector(&mut self, sect: i16, x0: i32, x1: i32, depth: u32) {
        self.visits += 1;
        if depth > MAX_DEPTH || self.visits > 20_000 {
            return;
        }
        if let Some(mark) = self.drawn_mark.get_mut(sect as usize)
            && !*mark
        {
            *mark = true;
            self.drawn.push(sect);
        }
        let segs = self.collect_segs(sect);
        let sec = self.world.sectors[sect as usize];
        // Portal runs to recurse into after this sector: (next, x0, x1).
        let mut portals: Vec<(i16, i32, i32)> = Vec::new();
        for col in x0..x1 {
            let ci = col as usize;
            let (top, bottom) = (self.umost[ci], self.dmost[ci]);
            if top >= bottom {
                continue;
            }
            // The ray leaves the sector through its nearest covering wall.
            let mut best: Option<(&Seg, i64, i64)> = None;
            for seg in &segs {
                if col < seg.x0 || col >= seg.x1 {
                    continue;
                }
                let (d, t) = Self::seg_at(seg, col);
                // Walls clearly nearer than the entry portal belong to an
                // earlier pass of a concave sector. A front-facing wall lying
                // *on* the portal line is kept: Build maps put closed swing
                // doors and stall doors exactly there (the portal's own back
                // side is already culled as a back face).
                if d + 2 < self.entry[ci] {
                    continue;
                }
                if best.is_none_or(|(_, bd, _)| d < bd) {
                    best = Some((seg, d, t));
                }
            }
            let Some((seg, d, t)) = best else {
                // No wall (camera numerically on an edge): planes only.
                self.fill_planes(sect, &sec, col, top, bottom, top, bottom);
                continue;
            };
            let wall = self.world.walls[seg.wall];
            let (px, py) = self.wall_point(seg.wall, t);
            let (cz, fz) = self.planes_at(sect, px, py);
            let cy = self.row_of(cz, d).clamp(-1, i64::from(self.h) + 1) as i32;
            let fy = self.row_of(fz, d).clamp(-1, i64::from(self.h) + 1) as i32;
            self.fill_planes(sect, &sec, col, top, bottom, cy, fy);
            let depth_i = d.min(i64::from(i32::MAX - 1)) as i32;
            let wall_len = crate::helpers::length2d(
                self.world.walls[wall.point2 as usize]
                    .x
                    .wrapping_sub(wall.x),
                self.world.walls[wall.point2 as usize]
                    .y
                    .wrapping_sub(wall.y),
            );
            let next = wall.nextsector;
            // cstat 32 = one-way wall: drawn solid even with a neighbour.
            if next < 0 || next >= self.world.numsectors || wall.cstat & 32 != 0 {
                let zref = match (
                    next >= 0 && next < self.world.numsectors,
                    wall.cstat & 4 != 0,
                ) {
                    (true, false) => self.world.sectors[next as usize].ceilingz,
                    (true, true) | (false, false) => sec.ceilingz,
                    (false, true) => sec.floorz,
                };
                self.draw_wall_span(
                    col,
                    cy.max(top),
                    fy.min(bottom),
                    d,
                    depth_i,
                    t,
                    wall_len,
                    &wall,
                    wall.picnum,
                    zref,
                    sect,
                );
                self.umost[ci] = bottom;
                continue;
            }
            let nsec = self.world.sectors[next as usize];
            let (ncz, nfz) = self.planes_at(next, px, py);
            let ncy = self.row_of(ncz, d).clamp(-1, i64::from(self.h) + 1) as i32;
            let nfy = self.row_of(nfz, d).clamp(-1, i64::from(self.h) + 1) as i32;
            let both_sky_ceiling =
                sec.ceilingstat & STAT_PARALLAX != 0 && nsec.ceilingstat & STAT_PARALLAX != 0;
            let both_sky_floor =
                sec.floorstat & STAT_PARALLAX != 0 && nsec.floorstat & STAT_PARALLAX != 0;
            // Upper band: this ceiling down to the neighbour's.
            if ncy > cy && !both_sky_ceiling {
                let zref = if wall.cstat & 4 != 0 {
                    sec.ceilingz
                } else {
                    nsec.ceilingz
                };
                self.draw_wall_span(
                    col,
                    cy.max(top),
                    ncy.min(bottom),
                    d,
                    depth_i,
                    t,
                    wall_len,
                    &wall,
                    wall.picnum,
                    zref,
                    sect,
                );
            }
            // Lower band: neighbour's floor down to this floor. cstat 2 takes
            // the bottom texture from the other side's wall.
            if nfy < fy && !both_sky_floor {
                let src = if wall.cstat & 2 != 0 {
                    usize::try_from(wall.nextwall)
                        .ok()
                        .and_then(|nw| self.world.walls.get(nw))
                        .copied()
                        .unwrap_or(wall)
                } else {
                    wall
                };
                let zref = if wall.cstat & 4 != 0 {
                    sec.ceilingz
                } else {
                    nsec.floorz
                };
                self.draw_wall_span(
                    col,
                    nfy.max(top),
                    fy.min(bottom),
                    d,
                    depth_i,
                    t,
                    wall_len,
                    &src,
                    src.picnum,
                    zref,
                    sect,
                );
            }
            // Parallax on both sides: no wall band, and the neighbour's sky
            // continues this one, so the opening keeps the sky rows.
            let open_top = if both_sky_ceiling {
                top
            } else {
                top.max(cy).max(ncy)
            };
            let open_bottom = if both_sky_floor {
                bottom
            } else {
                bottom.min(fy).min(nfy)
            };
            if wall.cstat & 16 != 0 && open_top < open_bottom {
                self.masked.push(MaskedCol {
                    wall: seg.wall,
                    sect,
                    zref: if wall.cstat & 4 == 0 {
                        sec.ceilingz.max(nsec.ceilingz)
                    } else {
                        sec.floorz.min(nsec.floorz)
                    },
                    col,
                    y0: open_top,
                    y1: open_bottom,
                    depth: depth_i,
                    t_q16: t,
                });
            }
            if open_top < open_bottom {
                self.entry[ci] = d;
                self.umost[ci] = open_top;
                self.dmost[ci] = open_bottom;
                match portals.last_mut() {
                    Some((n, _, end)) if *n == next && *end == col => *end = col + 1,
                    _ => portals.push((next, col, col + 1)),
                }
            } else {
                self.umost[ci] = bottom;
            }
        }
        for (next, a, b) in portals {
            self.draw_sector(next, a, b, depth + 1);
        }
    }

    /// Ceiling rows `[top, cy)` and floor rows `[fy, bottom)` of `sect` in `col`.
    #[allow(clippy::too_many_arguments)]
    fn fill_planes(
        &mut self,
        sect: i16,
        sec: &crate::types::SectorT,
        col: i32,
        top: i32,
        bottom: i32,
        cy: i32,
        fy: i32,
    ) {
        self.plane_span(sect, sec, col, top.max(0), cy.min(bottom).min(self.h), true);
        self.plane_span(
            sect,
            sec,
            col,
            fy.max(top).max(0),
            bottom.min(self.h),
            false,
        );
    }

    /// One column of a floor or ceiling: everything that does not depend on
    /// the row (tile, palette tables, ray, slope plane, alignment basis) is
    /// resolved once; the per-pixel work is a depth, a texel and two tables.
    fn plane_span(
        &mut self,
        sect: i16,
        sec: &crate::types::SectorT,
        col: i32,
        y0: i32,
        y1: i32,
        ceiling: bool,
    ) {
        if y1 <= y0 {
            return;
        }
        let (picnum, stat, xpan, ypan, z, pal, shade) = if ceiling {
            (
                sec.ceilingpicnum,
                sec.ceilingstat,
                sec.ceilingxpanning,
                sec.ceilingypanning,
                sec.ceilingz,
                sec.ceilingpal,
                sec.ceilingshade,
            )
        } else {
            (
                sec.floorpicnum,
                sec.floorstat,
                sec.floorxpanning,
                sec.floorypanning,
                sec.floorz,
                sec.floorpal,
                sec.floorshade,
            )
        };
        let tiles = self.tiles;
        let palette_variant = tiles.has_palette_variant(picnum, pal, self.clock);
        let pal_table = (!palette_variant).then(|| tiles.pal_table(pal)).flatten();
        let flat = super::wall_color(picnum, shade);
        let w = self.w as usize;
        if stat & STAT_PARALLAX != 0 {
            // Sky: column → angle → tile/u, row → v (Build parascan). Drawn
            // unshaded: per-sector sky shades show as seams between sectors.
            let column = tiles.sky_column(
                picnum,
                pal,
                self.view.ang_q16,
                col,
                self.w,
                self.h,
                self.horizon,
                self.clock,
            );
            let height = i64::from(self.h.max(1));
            for y in y0..y1 {
                let (p, color) = tiles
                    .sample_skybox_color_at(
                        picnum,
                        pal,
                        self.view.ang_q16,
                        col,
                        self.w,
                        y,
                        self.h,
                        self.horizon,
                    )
                    .or_else(|| {
                        column.map(|(tile, u, sky_horizon)| {
                            let th = tile.h as i64;
                            let v = th / 2
                                + (i64::from(y) - sky_horizon) * 200 / height
                                + i64::from(ypan) * th / 256;
                            let pixel = tile.at(u, v);
                            if pixel == room_build::art::TRANSPARENT {
                                (flat, 0)
                            } else {
                                (pixel, tile.at_rgb(u, v))
                            }
                        })
                    })
                    .unwrap_or((flat, 0));
                self.zbuf[y as usize * w + col as usize] = i32::MAX - 1;
                self.frame.put_rgb(col, y, p, color);
            }
            return;
        }
        let Some(tile) = tiles.resolve_for_palette(picnum, pal, self.clock) else {
            for y in y0..y1 {
                self.zbuf[y as usize * w + col as usize] = i32::MAX - 1;
                self.frame.put(col, y, flat);
            }
            return;
        };
        let (fx, fy) = (self.focal_x, self.focal_y);
        let side_q14 = (i64::from(col - self.cx) << 14) / fx.max(1);
        let ray_x = self.cos - ((self.sin * side_q14) >> 14);
        let ray_y = self.sin + ((self.cos * side_q14) >> 14);
        let (posx, posy, posz) = (
            i64::from(self.view.posx),
            i64::from(self.view.posy),
            i64::from(self.view.posz),
        );
        // Slope plane: z = z0 + g * depth along this column's ray.
        let slope = (stat & 2 != 0).then(|| {
            let z_at = |x: i32, y: i32| {
                let (c, f) = getzsofslope(sect, x, y, &self.sector_z, &self.wall_xy);
                f64::from(if ceiling { c } else { f })
            };
            const STEP: i32 = 4096;
            let z0 = z_at(self.view.posx, self.view.posy);
            let gx =
                (z_at(self.view.posx.saturating_add(STEP), self.view.posy) - z0) / f64::from(STEP);
            let gy =
                (z_at(self.view.posx, self.view.posy.saturating_add(STEP)) - z0) / f64::from(STEP);
            (
                z0,
                gx * (ray_x as f64 / 16384.0) + gy * (ray_y as f64 / 16384.0),
            )
        });
        // Relative alignment (stat 64): axes from the sector's first wall.
        let basis = (stat & 64 != 0)
            .then(|| {
                let first = self.world.walls.get(usize::try_from(sec.wallptr).ok()?)?;
                let next = self.world.walls.get(usize::try_from(first.point2).ok()?)?;
                let dx = i64::from(next.x) - i64::from(first.x);
                let dy = i64::from(next.y) - i64::from(first.y);
                let len_sq = (dx * dx + dy * dy).min(i64::from(u32::MAX)) as u32;
                let len = i64::from(crate::numeric::nsqrtasm(len_sq));
                (len > 0).then_some((i64::from(first.x), i64::from(first.y), dx, dy, len))
            })
            .flatten();
        // Build `ceilscan`/`florscan`: one texel per 16 world units whatever
        // the tile size (the `8 - log2(size)` shift cancels it), doubled
        // density with stat 8; panning (`<<24`, unshifted) is in 1/256ths
        // of the tile.
        let (su, sv) = if stat & 8 != 0 {
            (32_i64, 32_i64)
        } else {
            (16_i64, 16_i64)
        };
        let (pu, pv) = (
            i64::from(xpan) * tile.w as i64,
            i64::from(ypan) * tile.h as i64,
        );
        for y in y0..y1 {
            let idx = y as usize * w + col as usize;
            let dy = i64::from(y - self.horizon);
            let depth = match slope {
                None => {
                    let dz = (posz - i64::from(z)) >> 4;
                    if dy == 0 || dz == 0 || (dy > 0) != (dz < 0) {
                        self.zbuf[idx] = i32::MAX - 1;
                        self.frame.put(col, y, flat);
                        continue;
                    }
                    (fy * dz.abs()) / dy.abs()
                }
                Some((z0, g)) => {
                    let denom = dy as f64 - g * fy as f64 / 16.0;
                    let t = if denom == 0.0 {
                        -1.0
                    } else {
                        (z0 - posz as f64) * fy as f64 / 16.0 / denom
                    };
                    if t <= 0.0 || !t.is_finite() {
                        self.zbuf[idx] = i32::MAX - 1;
                        self.frame.put(col, y, flat);
                        continue;
                    }
                    t.min(1.0e9) as i64
                }
            };
            let wx = posx + ((ray_x * depth) >> 14);
            let wy = posy + ((ray_y * depth) >> 14);
            let (bx, by) = match basis {
                Some((ox, oy, dx, dy, len)) => {
                    let (rx, ry) = (wx - ox, wy - oy);
                    ((rx * dx + ry * dy) / len, (ry * dx - rx * dy) / len)
                }
                None => (wx, wy),
            };
            let (mut tx, mut ty) = if stat & 4 != 0 { (by, bx) } else { (bx, by) };
            if stat & 16 != 0 {
                tx = -tx;
            }
            if stat & 32 != 0 {
                ty = -ty;
            }
            let u = (tx * su + pu) >> 8;
            let v = (ty * sv + pv) >> 8;
            let texel = tile.at(u, v);
            let (p, color) = if texel == room_build::art::TRANSPARENT {
                (flat, 0)
            } else {
                let p = pal_table.map_or(texel, |t| t[usize::from(texel)]);
                let shade = self.fog_shade(shade, depth, sect);
                (
                    tiles.shade_row(shade).map_or(p, |r| r[usize::from(p)]),
                    tiles.shade_true_color(tile.at_rgb(u, v), shade),
                )
            };
            self.zbuf[idx] = depth.min(i64::from(i32::MAX - 1)) as i32;
            self.frame.put_rgb(col, y, p, color);
        }
    }

    /// Solid wall texels for rows `[y0, y1)`; `zref` anchors texture row 0.
    #[allow(clippy::too_many_arguments)]
    fn draw_wall_span(
        &mut self,
        col: i32,
        y0: i32,
        y1: i32,
        d: i64,
        depth_i: i32,
        t: i64,
        wall_len: i32,
        wall: &crate::types::WallT,
        picnum: i16,
        zref: i32,
        sect: i16,
    ) {
        if y1 <= y0 {
            return;
        }
        let shade = self.fog_shade(wall.shade, d, sect);
        let _ = wall_len;
        let tiles = self.tiles;
        let tile = tiles.resolve_for_palette(picnum, wall.pal, self.clock);
        let (tw, th) = tile.map_or((64, 64), |t| (t.w as i64, t.h as i64));
        // Build: the whole wall spans xrepeat*8 texels; xpanning is in texels.
        let mut u = ((t * i64::from(wall.xrepeat) * 8) >> 16) + i64::from(wall.xpanning);
        if wall.cstat & 8 != 0 {
            u = tw - 1 - u.rem_euclid(tw);
        }
        // A one-way MIRROR wall that is not the reflected mirror this frame
        // (Chocolate reflects only the nearest one) shows dark glass, not the
        // debug colour standing in for the tile's transparent texels.
        let flat = if picnum == MIRROR_TILE && wall.cstat & 32 != 0 {
            0
        } else {
            super::wall_color(picnum, wall.shade)
        };
        let palette_variant = tiles.has_palette_variant(picnum, wall.pal, self.clock);
        let pal_table = (!palette_variant)
            .then(|| tiles.pal_table(wall.pal))
            .flatten();
        let shade_row = tiles.shade_row(shade);
        let w = self.w as usize;
        // Build `wallscan`: texel = (z - zref) * yrepeat / 2048, with z linear
        // in the row at this column's depth.
        let yrep = i64::from(wall.yrepeat);
        let base = i64::from(self.view.posz) - i64::from(zref);
        let pan = (i64::from(wall.ypanning) * th) / 256;
        for y in y0.max(0)..y1.min(self.h) {
            let idx = y as usize * w + col as usize;
            let rows = i64::from(y) - i64::from(self.horizon);
            let dz = ((rows * d) << 4) / self.focal_y;
            let mut v = ((base + dz) * yrep) / 2048 + pan;
            if wall.cstat & 256 != 0 {
                v = th - 1 - v.rem_euclid(th);
            }
            let (pix, color) = match tile.map(|t| (t.at(u, v), t)) {
                Some((p, t)) if p != room_build::art::TRANSPARENT => {
                    let p = pal_table.map_or(p, |t| t[usize::from(p)]);
                    (
                        shade_row.map_or(p, |r| r[usize::from(p)]),
                        tiles.shade_true_color(t.at_rgb(u, v), shade),
                    )
                }
                _ => (flat, 0),
            };
            if wall.cstat & 128 != 0 {
                self.defer_translucent(idx, depth_i, pix, wall.cstat & 512 != 0, color);
            } else {
                self.zbuf[idx] = depth_i;
                self.frame.put_rgb(col, y, pix, color);
            }
        }
    }

    fn defer_translucent(
        &mut self,
        index: usize,
        depth: i32,
        foreground: u8,
        reverse: bool,
        true_color: u32,
    ) {
        let frame_pixels = (self.w.max(0) as usize).saturating_mul(self.h.max(0) as usize);
        let limit = frame_pixels
            .saturating_mul(8)
            .min(MAX_TRANSLUCENT_WALL_PIXELS);
        if self.translucent.len() < limit {
            self.translucent.push(TranslucentPixel {
                index,
                depth,
                foreground,
                reverse,
                blend_table: 0,
                true_color,
            });
        }
    }

    /// Masked (cstat 16) portal walls, farthest first, depth tested.
    fn draw_masked(&mut self) {
        let mut masked = std::mem::take(&mut self.masked);
        masked.sort_by_key(|m| std::cmp::Reverse(m.depth));
        for m in masked {
            let wall = self.world.walls[m.wall];
            let pic = wall.overpicnum;
            let (tw, th) = self
                .tiles
                .dimensions_for_palette(pic, wall.pal)
                .unwrap_or((64, 64));
            let tw = i64::from(tw.max(1));
            let th = i64::from(th.max(1));
            let mut u = ((m.t_q16 * i64::from(wall.xrepeat) * 8) >> 16) + i64::from(wall.xpanning);
            if wall.cstat & 8 != 0 {
                u = tw - 1 - u.rem_euclid(tw);
            }
            let u = u.rem_euclid(tw) as u32;
            let d = i64::from(m.depth);
            let ztop = i64::from(m.zref);
            for y in m.y0.max(0)..m.y1.min(self.h) {
                let idx = y as usize * self.w as usize + m.col as usize;
                if m.depth >= self.zbuf[idx] {
                    continue;
                }
                let rows = i64::from(y) - i64::from(self.horizon);
                let z = i64::from(self.view.posz) + ((rows * d) << 4) / self.focal_y;
                let mut v = ((z - ztop) * i64::from(wall.yrepeat)) / 2048
                    + (i64::from(wall.ypanning) * th) / 256;
                if wall.cstat & 256 != 0 {
                    v = th - 1 - v.rem_euclid(th);
                }
                let v = v.rem_euclid(th) as u32;
                let Some(p) = self
                    .tiles
                    .sample_at_for_palette(pic, wall.pal, u, v, self.clock)
                else {
                    continue;
                };
                if p == room_build::art::TRANSPARENT {
                    continue;
                }
                let shade = self.fog_shade(wall.shade, i64::from(m.depth), m.sect);
                let p = self.tiles.shade_palette(
                    shade,
                    if self.tiles.has_palette_variant(pic, wall.pal, self.clock) {
                        p
                    } else {
                        self.tiles.remap_palette(wall.pal, p)
                    },
                );
                let color = self.tiles.shade_true_color(
                    self.tiles
                        .true_color_at_for_palette(pic, wall.pal, u, v, self.clock),
                    shade,
                );
                if wall.cstat & 128 != 0 {
                    self.defer_translucent(idx, m.depth, p, wall.cstat & 512 != 0, color);
                } else {
                    self.zbuf[idx] = m.depth;
                    self.frame.put_rgb(m.col, y, p, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_translucent_walls_blend_back_to_front_and_respect_opaque_depth() {
        let mut tiles = SoftTiles::new();
        let mut blend = vec![0; 65_536];
        blend[usize::from(10_u8) * 256 + usize::from(20_u8)] = 31;
        blend[usize::from(31_u8) * 256 + usize::from(30_u8)] = 41;
        blend[usize::from(10_u8) * 256 + usize::from(40_u8)] = 51;
        tiles.set_translucency_table(Some(blend));
        let mut frame = SoftFrame::new(1, 1);
        frame.pixels[0] = 10;

        frame.begin_deferred_translucency();
        enqueue_translucent(
            &mut frame,
            vec![
                TranslucentPixel {
                    index: 0,
                    depth: 100,
                    foreground: 30,
                    reverse: false,
                    blend_table: 0,
                    true_color: 0,
                },
                TranslucentPixel {
                    index: 0,
                    depth: 150,
                    foreground: 20,
                    reverse: false,
                    blend_table: 0,
                    true_color: 0,
                },
                TranslucentPixel {
                    index: 0,
                    depth: 250,
                    foreground: 40,
                    reverse: false,
                    blend_table: 0,
                    true_color: 0,
                },
            ],
        );
        frame.finish_deferred_translucency(&tiles, &[200]);

        assert_eq!(frame.pixels, [41]);
    }
}
