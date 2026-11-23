//! 3D floors (SR6): ZDoom's `Sector_Set3DFloor` slabs and their light, on
//! top of the vanilla renderer.
//!
//! The BSP walk stays vanilla's. Each subsector of a target sector notes the
//! open rows of every column as it is reached (what nearer walls left), and
//! after the world planes its slab planes are drawn from the subsector's
//! outline at the slab heights, clipped to those rows. Slab sides ride on
//! the two-sided segs as masked columns. A depth buffer (scale per pixel)
//! that only slabs, sloped planes and masked things write keeps sprites and
//! sides behind slabs hidden and lets translucent slabs go last.

use super::{
    FRACBITS, FRACUNIT, LIGHTLEVELS, LIGHTSEGSHIFT, MASKED_DONE, PLANE_UNSET, Px, SoftRenderer,
    SoftStyle, SoftTarget, Visplane, blend, pack, seg_at, tint_of, unpack,
};
use crate::data::SoftData;
use crate::fixed::fixed_mul;
use crate::level::{
    FF3D_ADDITIVE, FF3D_DOUBLESHADOW, FF3D_LOWERTEXTURE, FF3D_UPPERTEXTURE, FlatXform, SlabPlanes,
    SoftFFloor, SoftLevel, SoftSector,
};

/// One slab plane of one subsector to draw after the world planes.
#[derive(Clone, Copy, Debug)]
pub(super) struct SlabJob {
    subsector: u32,
    /// Index into [`Extra::clips`].
    clip: u32,
    height: i32,
    pic: u16,
    light: i32,
    xoffs: i32,
    yoffs: i32,
    tint: (u32, u32),
    /// Opacity 0..=256 and style (`Normal` = opaque).
    alpha: u32,
    style: SoftStyle,
    /// Distance of the outline's centre (sorting translucent planes).
    depth: f64,
    /// Sloped slab plane.
    slope: Option<crate::level::SoftSlope>,
}

/// A slab side on one drawseg.
#[derive(Clone, Copy, Debug)]
pub(super) struct SegSlab {
    ff: SoftFFloor,
    planes: SlabPlanes,
}

/// Per-frame state of the SR6 extras.
#[derive(Clone, Debug, Default)]
pub(super) struct Extra {
    /// Scale (16.16, `projection / depth`) of the nearest slab, sloped
    /// plane or masked thing per pixel; 0 = none.
    pub zbuf: Vec<i32>,
    /// The depth buffer is in use this frame.
    pub zon: bool,
    /// Scale of what is being drawn, and whether it tests / writes.
    pub zscale: i32,
    pub ztest: bool,
    pub zwrite: bool,
    /// Each sector's 3D floors with this frame's planes (empty = none).
    pub sector_slabs: Vec<Vec<(SoftFFloor, SlabPlanes)>>,
    pub any: bool,
    /// Open rows per column when a slab subsector was reached.
    clips: Vec<(Vec<i16>, Vec<i16>)>,
    jobs: Vec<SlabJob>,
    pub seg_slabs: Vec<SegSlab>,
    /// Light splits of the wall being drawn: below height `.0` (16.16)
    /// use wall light row `.1`, highest first.
    pub wall_lights: Vec<(i32, usize)>,
    /// A slab plane is being drawn: style and opacity (0..=256).
    pub slab_plane: Option<(SoftStyle, u32)>,
    /// Quads of this frame's wall / flat / rolled sprites and decals.
    pub quads: Vec<super::quads::Quad>,
    /// Portals (SR7).
    pub portal: super::portal::PortalState,
    /// Voxel models and particles of the frame, dynamic lights (SR8).
    pub voxjobs: Vec<super::voxels::VoxJob>,
    pub partjobs: Vec<super::voxels::PartJob>,
    pub dl: super::voxels::DynState,
}

impl SoftRenderer {
    /// Frame start: this frame's slab planes per sector.
    pub(super) fn begin_extras(&mut self, level: &SoftLevel, sectors: &[SoftSector]) {
        let x = &mut self.x;
        x.zon = false;
        x.ztest = false;
        x.zwrite = false;
        x.clips.clear();
        x.jobs.clear();
        x.seg_slabs.clear();
        x.wall_lights.clear();
        x.slab_plane = None;
        x.quads.clear();
        x.voxjobs.clear();
        x.partjobs.clear();
        x.any = level.ffloors.iter().any(|f| !f.is_empty());
        x.sector_slabs.iter_mut().for_each(Vec::clear);
        if !x.any {
            return;
        }
        x.sector_slabs.resize_with(level.ffloors.len(), Vec::new);
        for (si, list) in level.ffloors.iter().enumerate() {
            for ff in list {
                if let Some(p) = ff.planes(sectors) {
                    x.sector_slabs[si].push((*ff, p));
                }
            }
            // Highest top first.
            x.sector_slabs[si].sort_by_key(|(_, p)| std::cmp::Reverse(p.top));
        }
    }

    /// Sector `si` has 3D floors this frame.
    #[inline]
    pub(super) fn has_slabs(&self, si: usize) -> bool {
        self.x.any && self.x.sector_slabs.get(si).is_some_and(|l| !l.is_empty())
    }

    /// The light level at height `z` (16.16) in sector `si`: a shading
    /// slab lights everything below its top with its control sector's
    /// light, down to the next slab, or to its own bottom with
    /// `FF_DOUBLESHADOW` (where the light above it returns).
    pub(super) fn light_at(&self, si: usize, z: i32, own: i32) -> i32 {
        let Some(list) = self.x.sector_slabs.get(si) else {
            return own;
        };
        let mut z = z;
        // Lowest top above `z` (a thing standing on a slab is lit from
        // above it).
        for _ in 0..list.len() + 1 {
            let Some((ff, p)) = list.iter().rev().find(|(ff, p)| ff.shades() && p.top > z) else {
                return own;
            };
            if ff.flags & FF3D_DOUBLESHADOW != 0 && z < p.bottom {
                z = p.top.saturating_add(1);
                continue;
            }
            return p.light;
        }
        own
    }

    /// Turn the depth buffer on (cleared) for this frame.
    pub(super) fn zbuf_on(&mut self) {
        if self.x.zon {
            return;
        }
        let n = (self.width * self.height) as usize;
        self.x.zbuf.clear();
        self.x.zbuf.resize(n, 0);
        self.x.zon = true;
    }

    /// Pixel `i` is hidden by something nearer than `scale` (with a small
    /// margin so a thing standing on a plane keeps its feet).
    #[inline]
    pub(super) fn z_hidden(&self, i: usize, scale: i32) -> bool {
        self.x.zon && self.x.zbuf[i] > scale.saturating_add(scale >> 5)
    }

    /// Light splits for the wall of a seg whose front sector is `si`.
    pub(super) fn wall_light_splits(
        &mut self,
        si: usize,
        own: i32,
        v1: crate::level::SoftVertex,
        v2: crate::level::SoftVertex,
    ) {
        self.x.wall_lights.clear();
        if !self.has_slabs(si) || self.fixed.is_some() {
            return;
        }
        let mut bounds: Vec<i32> = Vec::new();
        for (ff, p) in &self.x.sector_slabs[si] {
            if ff.shades() {
                bounds.push(p.top);
                if ff.flags & FF3D_DOUBLESHADOW != 0 {
                    bounds.push(p.bottom);
                }
            }
        }
        bounds.sort_unstable_by(|a, b| b.cmp(a));
        bounds.dedup();
        let splits: Vec<(i32, usize)> = bounds
            .into_iter()
            .map(|h| {
                let l = self.light_at(si, h.saturating_sub(1), own);
                (h, self.light_row(l, v1, v2))
            })
            .collect();
        self.x.wall_lights = splits;
    }

    /// A wall column, split where 3D floors change the light.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn wall_column(
        &self,
        data: &SoftData,
        tex: u16,
        col: i32,
        x: i32,
        yl: i32,
        yh: i32,
        mid: i32,
        iscale: i32,
        colormap: usize,
        scale: i32,
        target: &mut SoftTarget,
    ) {
        if self.x.wall_lights.is_empty() {
            self.draw_column(data, tex, col, x, yl, yh, mid, iscale, colormap, target);
            return;
        }
        let mut y = yl;
        let mut cmap = colormap;
        for &(h, row) in &self.x.wall_lights {
            // First row below height `h`.
            let yb = (self
                .centeryfrac
                .wrapping_sub(fixed_mul(h.wrapping_sub(self.viewz), scale))
                + FRACUNIT
                - 1)
                >> FRACBITS;
            if yb > y {
                self.draw_column(
                    data,
                    tex,
                    col,
                    x,
                    y,
                    yh.min(yb - 1),
                    mid,
                    iscale,
                    cmap,
                    target,
                );
                y = yb;
            }
            if y > yh {
                return;
            }
            cmap = self.scale_light(row, scale);
        }
        self.draw_column(data, tex, col, x, y, yh, mid, iscale, cmap, target);
    }

    /// `R_Subsector` of a slab sector: note its open rows and the slab
    /// planes the viewer sees.
    pub(super) fn note_slab_subsector(&mut self, level: &SoftLevel, num: usize, si: usize) {
        let Some(shape) = level.subsector_shapes.get(num).filter(|s| s.len() >= 3) else {
            return;
        };
        let (vx, vy) = (
            f64::from(self.viewx) / 65536.0,
            f64::from(self.viewy) / 65536.0,
        );
        let (cx, cy) = shape.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
        let n = shape.len() as f64;
        let depth = ((cx / n - vx).powi(2) + (cy / n - vy).powi(2)).sqrt();
        let own = self.frontsector.light;
        let clip = self.x.clips.len() as u32;
        let mut any = false;
        let mut jobs = Vec::new();
        let list = self.x.sector_slabs[si].clone();
        for (ff, p) in list {
            if !ff.visible() {
                continue;
            }
            let inside =
                self.viewz >= bottom_here_of(&p, vx, vy) && self.viewz <= top_here_of(&p, vx, vy);
            let both = ff.both_sides();
            let (style, alpha) = if ff.alpha == 255 {
                (SoftStyle::Normal, 256)
            } else if ff.flags & FF3D_ADDITIVE != 0 {
                (SoftStyle::Add, u32::from(ff.alpha) * 256 / 255)
            } else {
                (SoftStyle::Translucent, u32::from(ff.alpha) * 256 / 255)
            };
            let c = level
                .sectors
                .get(ff.control as usize)
                .copied()
                .unwrap_or_default();
            let ctrl_tint = tint_of(&c);
            let top_light = self.light_at(si, p.top.saturating_add(1), own);
            let bottom_light = self.light_at(si, p.bottom.saturating_sub(1), own);
            let (vxu, vyu) = (vx, vy);
            let at = |h: i32, s: Option<crate::level::SoftSlope>| {
                s.map_or(h, |s| (s.z_at(vxu, vyu) * 65536.0) as i32)
            };
            let (top_here, bottom_here) = (at(p.top, p.top_slope), at(p.bottom, p.bottom_slope));
            // Planes outside the target's own floor..ceiling are not drawn.
            let (lo, hi) = (self.frontsector.floor, self.frontsector.ceiling);
            let in_range =
                |h: i32, s: Option<crate::level::SoftSlope>| s.is_some() || (lo..=hi).contains(&h);
            let mut push = |height: i32,
                            pic: u16,
                            light: i32,
                            xo: i32,
                            yo: i32,
                            slope: Option<crate::level::SoftSlope>| {
                if !in_range(height, slope) {
                    return;
                }
                jobs.push(SlabJob {
                    subsector: num as u32,
                    clip,
                    height,
                    pic,
                    light,
                    xoffs: xo,
                    yoffs: yo,
                    tint: ctrl_tint,
                    alpha,
                    style,
                    depth,
                    slope,
                });
                any = true;
            };
            let vav = ff.typ & 3 == 0;
            let (top_xo, top_yo, bot_xo, bot_yo) = if vav {
                (
                    c.floor_xoffs,
                    c.floor_yoffs,
                    c.ceiling_xoffs,
                    c.ceiling_yoffs,
                )
            } else {
                (
                    c.ceiling_xoffs,
                    c.ceiling_yoffs,
                    c.floor_xoffs,
                    c.floor_yoffs,
                )
            };
            let viewz = self.viewz;
            if viewz > top_here || (both && inside) {
                push(p.top, p.top_pic, top_light, top_xo, top_yo, p.top_slope);
            }
            if (p.bottom < p.top || p.bottom_slope.is_some())
                && (viewz < bottom_here || (both && inside))
            {
                push(
                    p.bottom,
                    p.bottom_pic,
                    bottom_light,
                    bot_xo,
                    bot_yo,
                    p.bottom_slope,
                );
            }
        }
        self.x.jobs.extend(jobs);
        if any {
            self.x
                .clips
                .push((self.ceilingclip.clone(), self.floorclip.clone()));
        }
    }

    /// The slab sides a two-sided seg shows: the back sector's slabs the
    /// front lacks (outside faces), and the front's own the back lacks when
    /// they are drawn from inside. Returns (first, count) in `seg_slabs`.
    pub(super) fn seg_slab_sides(&mut self, front: usize, back: Option<usize>) -> (u32, u32) {
        let Some(back) = back else {
            return (0, 0);
        };
        if !self.has_slabs(front) && !self.has_slabs(back) {
            return (0, 0);
        }
        let first = self.x.seg_slabs.len() as u32;
        let empty = Vec::new();
        let fl = self.x.sector_slabs.get(front).unwrap_or(&empty);
        let bl = self.x.sector_slabs.get(back).unwrap_or(&empty);
        let has = |l: &Vec<(SoftFFloor, SlabPlanes)>, c: u32| l.iter().any(|(f, _)| f.control == c);
        let mut out = Vec::new();
        for (ff, p) in bl {
            if ff.visible() && !has(fl, ff.control) && p.top > p.bottom {
                out.push(SegSlab {
                    ff: *ff,
                    planes: *p,
                });
            }
        }
        for (ff, p) in fl {
            if ff.visible() && ff.both_sides() && !has(bl, ff.control) && p.top > p.bottom {
                out.push(SegSlab {
                    ff: *ff,
                    planes: *p,
                });
            }
        }
        let count = out.len() as u32;
        self.x.seg_slabs.extend(out);
        (first, count)
    }

    /// Slab sides of drawseg `ds` in columns `x1..=x2` (masked phase).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_slab_sides(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        ds: &super::DrawSeg,
        x1: i32,
        x2: i32,
        target: &mut SoftTarget,
    ) {
        let super::Clip::Openings(colbase) = ds.slabcol else {
            return;
        };
        let seg = seg_at(level, ds.seg);
        let own_side = level.sides[seg.side as usize];
        let v1 = level.vertices[seg.v1 as usize];
        let v2 = level.vertices[seg.v2 as usize];
        let w = self.width as usize;
        let slabs: Vec<SegSlab> = self.x.seg_slabs
            [ds.slab_first as usize..(ds.slab_first + ds.slab_count) as usize]
            .to_vec();
        for x in x1..=x2 {
            let slot = (colbase + x) as usize;
            let col = self.openings.get(slot).copied().unwrap_or(MASKED_DONE);
            if col == MASKED_DONE {
                continue;
            }
            self.openings[slot] = MASKED_DONE;
            let scale = ds
                .scale1
                .wrapping_add((x - ds.x1).wrapping_mul(ds.scalestep));
            if scale <= 0 {
                continue;
            }
            let fclip = self
                .clip_at(ds.sprbottomclip, x)
                .map_or(self.height, i32::from);
            let cclip = self.clip_at(ds.sprtopclip, x).map_or(-1, i32::from);
            let iscale = (0xffff_ffffu32 / (scale as u32).max(1)) as i32;
            for s in &slabs {
                let cside =
                    s.ff.side
                        .and_then(|i| level.sides.get(i as usize))
                        .copied()
                        .unwrap_or_default();
                let texid = if s.ff.flags & FF3D_UPPERTEXTURE != 0 {
                    own_side.top
                } else if s.ff.flags & FF3D_LOWERTEXTURE != 0 {
                    own_side.bottom
                } else {
                    cside.mid
                };
                if texid == 0 {
                    continue;
                }
                let texid = data.anim_texture(texid, self.leveltime);
                let Some(tex) = data.textures.get(texid as usize) else {
                    continue;
                };
                let p = s.planes;
                let (top, bottom) = if p.top_slope.is_some() || p.bottom_slope.is_some() {
                    let (mx, my) = self.seg_point_at_column(v1, v2, x);
                    (top_here_of(&p, mx, my), bottom_here_of(&p, mx, my))
                } else {
                    (p.top, p.bottom)
                };
                if top <= bottom {
                    continue;
                }
                let row = |h: i32| {
                    self.centeryfrac
                        .wrapping_sub(fixed_mul(h.wrapping_sub(self.viewz), scale))
                };
                let yl = ((row(top) + FRACUNIT - 1) >> FRACBITS)
                    .max(cclip + 1)
                    .max(0);
                let yh = ((row(bottom) - 1) >> FRACBITS)
                    .min(fclip - 1)
                    .min(self.height - 1);
                if yl > yh {
                    continue;
                }
                let tcol = i32::from(col).wrapping_add(cside.texture_offset >> FRACBITS);
                let pixels = tex.column(tcol);
                let alpha_col = tex.column_alpha(tcol);
                let rgb = if self.truecolor {
                    tex.rgb_column(tcol)
                } else {
                    None
                };
                let th = (tex.height as i32).max(1);
                let texturemid = top.wrapping_sub(self.viewz).wrapping_add(cside.row_offset);
                let colormap = self.fixed.unwrap_or_else(|| {
                    let l = self.light_row(p.light, v1, v2);
                    self.scale_light(l, scale)
                });
                self.tint = tint_of(
                    &level
                        .sectors
                        .get(s.ff.control as usize)
                        .copied()
                        .unwrap_or_default(),
                );
                let px = self.px(data, colormap);
                let (style, a) = if s.ff.alpha == 255 {
                    (SoftStyle::Normal, 256)
                } else if s.ff.flags & FF3D_ADDITIVE != 0 {
                    (SoftStyle::Add, u32::from(s.ff.alpha) * 256 / 255)
                } else {
                    (SoftStyle::Translucent, u32::from(s.ff.alpha) * 256 / 255)
                };
                let mut frac = texturemid.wrapping_add((yl - self.centery).wrapping_mul(iscale));
                for y in yl..=yh {
                    let t = (frac >> FRACBITS).rem_euclid(th) as usize;
                    frac = frac.wrapping_add(iscale);
                    if alpha_col.get(t).copied().unwrap_or(255) == 0 {
                        continue;
                    }
                    let i = y as usize * w + x as usize;
                    if self.z_hidden(i, scale) {
                        continue;
                    }
                    let texel = pixels.get(t).copied().unwrap_or(0);
                    let c = rgb.and_then(|r| r.get(t).copied());
                    if style == SoftStyle::Normal {
                        px.put_rgb(target, i, texel, c);
                        if self.x.zon {
                            self.x.zbuf[i] = self.x.zbuf[i].max(scale);
                        }
                    } else {
                        put_blend(px, data, target, i, texel, c, style, a, self.truecolor);
                    }
                }
            }
        }
    }

    /// Draw the noted slab planes: opaque ones (after the world planes) or
    /// translucent ones, far to near (after the masked things).
    pub(super) fn draw_slab_planes(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        opaque: bool,
        target: &mut SoftTarget,
    ) {
        if self.x.jobs.is_empty() {
            return;
        }
        self.zbuf_on();
        let mut jobs: Vec<SlabJob> = self
            .x
            .jobs
            .iter()
            .filter(|j| (j.style == SoftStyle::Normal) == opaque)
            .copied()
            .collect();
        if !opaque {
            jobs.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        }
        for job in jobs {
            let Some(shape) = level.subsector_shapes.get(job.subsector as usize) else {
                continue;
            };
            let Some(pl) = self.rasterize_shape(shape, job) else {
                continue;
            };
            self.x.slab_plane = Some((job.style, job.alpha));
            self.draw_flat_plane(data, &pl, target);
            self.x.slab_plane = None;
        }
    }

    /// Columns `top..=bottom` of the outline `shape` seen at `job.height`,
    /// inside the rows open when its subsector was reached.
    fn rasterize_shape(&self, shape: &[(f64, f64)], job: SlabJob) -> Option<Visplane> {
        let (clip_top, clip_bot) = self.x.clips.get(job.clip as usize)?;
        let proj = f64::from(self.projection) / 65536.0;
        let (vx, vy, vz) = (
            f64::from(self.viewx) / 65536.0,
            f64::from(self.viewy) / 65536.0,
            f64::from(self.viewz) / 65536.0,
        );
        let va = f64::from(self.viewangle) / 4_294_967_296.0 * std::f64::consts::TAU;
        let (sa, ca) = va.sin_cos();
        // View space: (right, forward).
        let view: Vec<(f64, f64)> = shape
            .iter()
            .map(|&(x, y)| {
                let (dx, dy) = (x - vx, y - vy);
                (dx * sa - dy * ca, dx * ca + dy * sa)
            })
            .collect();
        // Near clip.
        const NEAR: f64 = 0.25;
        let mut clipped = Vec::with_capacity(view.len() + 2);
        for i in 0..view.len() {
            let a = view[i];
            let b = view[(i + 1) % view.len()];
            if a.1 >= NEAR {
                clipped.push(a);
            }
            if (a.1 >= NEAR) != (b.1 >= NEAR) {
                let t = (NEAR - a.1) / (b.1 - a.1);
                clipped.push((a.0 + (b.0 - a.0) * t, NEAR));
            }
        }
        if clipped.len() < 3 {
            return None;
        }
        let (cx, cy) = (
            f64::from(self.centerxfrac) / 65536.0,
            f64::from(self.centery),
        );
        // Back to map space for a sloped plane's height at each corner.
        let height_at = |s: f64, z: f64| match job.slope {
            Some(sl) => {
                let (x, y) = (vx + s * sa + z * ca, vy - s * ca + z * sa);
                sl.z_at(x, y) - vz
            }
            None => f64::from(job.height) / 65536.0 - vz,
        };
        let pts: Vec<(f64, f64)> = clipped
            .iter()
            .map(|&(s, z)| (cx + s * proj / z, cy - height_at(s, z) * proj / z))
            .collect();
        let xmin = pts.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        let xmax = pts.iter().map(|p| p.0).fold(f64::MIN, f64::max);
        let x0 = ((xmin - 0.5).ceil() as i32).max(0);
        let x1 = ((xmax - 0.5).floor() as i32).min(self.width - 1);
        if x0 > x1 {
            return None;
        }
        let w = self.width as usize;
        let mut pl = Visplane {
            height: job.height,
            picnum: u32::from(job.pic),
            light: job.light,
            xoffs: job.xoffs,
            yoffs: job.yoffs,
            tint: job.tint,
            slope: job.slope,
            xform: FlatXform::default(),
            minx: self.width,
            maxx: -1,
            top: vec![PLANE_UNSET; w + 2],
            bottom: vec![0; w + 2],
        };
        for x in x0..=x1 {
            let xc = f64::from(x) + 0.5;
            let (mut ylo, mut yhi) = (f64::MAX, f64::MIN);
            for i in 0..pts.len() {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                if (a.0 <= xc && b.0 >= xc) || (b.0 <= xc && a.0 >= xc) {
                    let y = if (b.0 - a.0).abs() < 1e-9 {
                        ylo = ylo.min(a.1.min(b.1));
                        a.1.max(b.1)
                    } else {
                        a.1 + (b.1 - a.1) * (xc - a.0) / (b.0 - a.0)
                    };
                    ylo = ylo.min(y);
                    yhi = yhi.max(y);
                }
            }
            if ylo > yhi {
                continue;
            }
            let xu = x as usize;
            let top = ((ylo - 0.5).ceil().max(-1.0) as i32).max(i32::from(clip_top[xu]) + 1);
            let bottom = ((yhi - 0.5).floor().min(f64::from(self.height)) as i32)
                .min(i32::from(clip_bot[xu]) - 1)
                .min(self.height - 1);
            if top > bottom || top < 0 {
                continue;
            }
            pl.top[xu + 1] = top as u16;
            pl.bottom[xu + 1] = bottom as u16;
            pl.minx = pl.minx.min(x);
            pl.maxx = pl.maxx.max(x);
        }
        (pl.minx <= pl.maxx).then_some(pl)
    }

    /// One pixel of a slab plane or sloped plane at depth `t` (units):
    /// depth-tested (slabs), written to the depth buffer (opaque), blended
    /// (translucent).
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn plane_pixel(
        &mut self,
        px: Px<'_>,
        data: &SoftData,
        target: &mut SoftTarget,
        i: usize,
        texel: u8,
        rgb: Option<u32>,
        t: f64,
    ) {
        let scale = (f64::from(self.projection) / t.max(1e-3)).min(f64::from(i32::MAX)) as i32;
        match self.x.slab_plane {
            None => {
                px.put_rgb(target, i, texel, rgb);
                if self.plane_slope.is_some() {
                    self.zbuf_on();
                    self.x.zbuf[i] = self.x.zbuf[i].max(scale);
                }
            }
            Some((style, a)) => {
                if self.x.zbuf[i] > scale {
                    return;
                }
                if style == SoftStyle::Normal {
                    px.put_rgb(target, i, texel, rgb);
                    self.x.zbuf[i] = scale;
                } else {
                    put_blend(px, data, target, i, texel, rgb, style, a, self.truecolor);
                }
            }
        }
    }

    /// The light level index of a slab plane (as `R_DrawPlanes`).
    pub(super) fn plane_light_index(&self, light: i32) -> usize {
        ((light >> LIGHTSEGSHIFT) + self.extralight).clamp(0, LIGHTLEVELS as i32 - 1) as usize
    }
}

/// A slab's top / bottom height (16.16) above map point `(x, y)` (units).
fn top_here_of(p: &SlabPlanes, x: f64, y: f64) -> i32 {
    p.top_slope
        .map_or(p.top, |s| (s.z_at(x, y) * 65536.0) as i32)
}

fn bottom_here_of(p: &SlabPlanes, x: f64, y: f64) -> i32 {
    p.bottom_slope
        .map_or(p.bottom, |s| (s.z_at(x, y) * 65536.0) as i32)
}

/// `texel` blended over pixel `i` with `style` at opacity `a` (0..=256).
#[allow(clippy::too_many_arguments)]
pub(super) fn put_blend(
    px: Px<'_>,
    data: &SoftData,
    target: &mut SoftTarget,
    i: usize,
    texel: u8,
    rgb: Option<u32>,
    style: SoftStyle,
    a: u32,
    truecolor: bool,
) {
    let pal = |c: u8| {
        let [r, g, b] = data.palette[usize::from(c)];
        [u32::from(r), u32::from(g), u32::from(b)]
    };
    let src = match px {
        Px::True(sh) => unpack(match rgb {
            Some(c) if sh.cmap.is_none() => sh.shade(unpack(c)),
            _ => sh.rgb(texel),
        }),
        Px::Index(cm) => pal(cm[usize::from(texel)]),
    };
    let dst = if truecolor {
        unpack(target.rgb[i])
    } else {
        pal(target.pixels[i])
    };
    let out = blend(style, src, dst, a.min(256));
    if truecolor {
        target.rgb[i] = pack(out[0], out[1], out[2]);
    } else {
        target.pixels[i] = data.nearest_rgb(out);
    }
}
