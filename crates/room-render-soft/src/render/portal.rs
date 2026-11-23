//! Portals and skies (SR7): line portals, mirrors, horizons, sky
//! viewpoints, stacked sectors and double skies.
//!
//! A portal is a screen region: the columns and rows a portal wall or a
//! portal plane would have covered, as the walk left them open. After the
//! world planes each region is drawn by a copy of the renderer looking from
//! the portal's other side (moved, turned or mirrored), into a scratch frame
//! whose pixels inside the region are copied back. The copy starts with the
//! region as its open rows, culls what lies on the near side of the
//! destination line, and may open portals of its own, up to
//! [`MAX_PORTAL_DEPTH`] deep (UZDoom's `r_portal_recursions` default is 4).

use super::{
    ANGLETOSKYSHIFT, PL_SKYFLAT, PLANE_UNSET, SoftFrame, SoftRenderer, SoftTarget, Visplane,
    point_on_line, tint_of,
};
use crate::data::SoftData;
use crate::level::{FlatXform, SKY_FLAT, SoftLevel, SoftLinePortal, SoftSector, SoftSectorPortal};
use room_map::portals::{PORG_CEILING, PORG_FLOOR};

/// Portal planes' picnum bit (with the sector portal index).
pub(super) const PL_PORTAL: u32 = 0x4000_0000;

/// Portals inside portals, at most.
pub const MAX_PORTAL_DEPTH: u32 = 3;

/// What a region shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RegionKey {
    /// A line portal (line index) seen from `sector`.
    Line(u32, u32),
    /// A sector portal (index into `SoftLevel::sector_portals`).
    Plane(u32),
}

/// Open rows per column (`top > bottom` = closed).
#[derive(Clone, Debug)]
pub(super) struct Region {
    key: RegionKey,
    top: Vec<i16>,
    bottom: Vec<i16>,
    minx: i32,
    maxx: i32,
}

impl Region {
    fn new(key: RegionKey, width: usize) -> Self {
        Self {
            key,
            top: vec![i16::MAX; width],
            bottom: vec![-1; width],
            minx: i32::MAX,
            maxx: -1,
        }
    }

    /// Open rows `top..=bottom` of column `x`.
    pub(super) fn set(&mut self, x: i32, top: i32, bottom: i32) {
        if top > bottom || x < 0 || x as usize >= self.top.len() {
            return;
        }
        let xu = x as usize;
        self.top[xu] = self.top[xu].min(top as i16);
        self.bottom[xu] = self.bottom[xu].max(bottom as i16);
        self.minx = self.minx.min(x);
        self.maxx = self.maxx.max(x);
    }

    fn mirrored(&self) -> Self {
        let w = self.top.len() as i32;
        let mut r = Self::new(self.key, self.top.len());
        for x in self.minx.max(0)..=self.maxx {
            let xu = x as usize;
            r.set(
                w - 1 - x,
                i32::from(self.top[xu]),
                i32::from(self.bottom[xu]),
            );
        }
        r
    }
}

/// Keep only what is on side `keep` of the line through `(x, y)` along
/// `(dx, dy)` (16.16); `line` itself is skipped.
#[derive(Clone, Copy, Debug)]
pub(super) struct ClipLine {
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    keep: usize,
    line: u32,
}

/// A view drawn inside a portal.
#[derive(Clone, Debug)]
pub(super) struct SubView {
    region: Region,
    clip: Option<ClipLine>,
    depth: u32,
}

/// Per-frame portal state.
#[derive(Clone, Debug, Default)]
pub(super) struct PortalState {
    regions: Vec<Region>,
    /// The wall being stored is a portal: its region.
    pub cur: Option<usize>,
    clip: Option<ClipLine>,
    pub depth: u32,
    /// Hexen double sky: the back sky and both skies' scroll (BAM).
    pub sky2: Option<u16>,
    pub sky_offsets: [u32; 2],
}

const UNIT: f64 = 65536.0;

fn bam_to_rad(a: u32) -> f64 {
    f64::from(a) / 4_294_967_296.0 * std::f64::consts::TAU
}

fn rad_to_bam(r: f64) -> u32 {
    ((r / std::f64::consts::TAU).rem_euclid(1.0) * 4_294_967_296.0) as u64 as u32
}

impl SoftRenderer {
    /// Frame start: the sub-view's region (open rows, closed columns as
    /// solid segs), clip line and depth; the frame's skies.
    pub(super) fn begin_portals(&mut self, frame: &SoftFrame, sub: Option<&SubView>) {
        let p = &mut self.x.portal;
        p.regions.clear();
        p.cur = None;
        p.clip = sub.and_then(|s| s.clip);
        p.depth = sub.map_or(0, |s| s.depth);
        p.sky2 = frame.sky2;
        p.sky_offsets = frame.sky_offsets;
        let Some(sub) = sub else {
            return;
        };
        let w = self.width;
        let r = &sub.region;
        let mut runs: Vec<(i32, i32)> = Vec::new();
        for x in 0..w {
            let xu = x as usize;
            let (t, b) = (i32::from(r.top[xu]), i32::from(r.bottom[xu]));
            if t > b {
                self.ceilingclip[xu] = self.height as i16;
                self.floorclip[xu] = -1;
                match runs.last_mut() {
                    Some(last) if last.1 == x - 1 => last.1 = x,
                    _ => runs.push((x, x)),
                }
            } else {
                self.ceilingclip[xu] = (t - 1) as i16;
                self.floorclip[xu] = (b + 1) as i16;
            }
        }
        // Sentinels, merged with closed runs at the edges.
        let mut segs: Vec<(i32, i32)> = vec![(-0x7fff_ffff, -1)];
        for (a, b) in runs {
            match segs.last_mut() {
                Some(last) if a <= last.1 + 1 => last.1 = b,
                _ => segs.push((a, b)),
            }
        }
        match segs.last_mut() {
            Some(last) if last.1 >= w - 1 => last.1 = 0x7fff_ffff,
            _ => segs.push((w, 0x7fff_ffff)),
        }
        self.solidsegs = segs;
    }

    /// The point is on the culled side of the sub-view's clip line.
    #[inline]
    pub(super) fn portal_culled(&self, x: i32, y: i32) -> bool {
        self.x
            .portal
            .clip
            .is_some_and(|c| point_on_line(x, y, c.x, c.y, c.dx, c.dy) != c.keep)
    }

    /// The seg is the clip line itself, or wholly on its culled side.
    pub(super) fn portal_culls_seg(&self, level: &SoftLevel, seg: &crate::level::SoftSeg) -> bool {
        let Some(c) = self.x.portal.clip else {
            return false;
        };
        if seg.line == c.line {
            return true;
        }
        let (a, b) = (
            level.vertices[seg.v1 as usize],
            level.vertices[seg.v2 as usize],
        );
        self.portal_culled(a.x, a.y) && self.portal_culled(b.x, b.y)
    }

    /// The portal a seg shows instead of its wall (front side only, not
    /// past the depth limit for nested views).
    pub(super) fn seg_portal(&self, level: &SoftLevel, seg: &crate::level::SoftSeg) -> bool {
        level
            .line_portals
            .get(seg.line as usize)
            .copied()
            .flatten()
            .is_some_and(|(_, front)| front == seg.side)
    }

    /// The region for `key` (made on first use).
    pub(super) fn portal_region(&mut self, key: RegionKey) -> usize {
        if let Some(i) = self.x.portal.regions.iter().position(|r| r.key == key) {
            return i;
        }
        self.x
            .portal
            .regions
            .push(Region::new(key, self.width as usize));
        self.x.portal.regions.len() - 1
    }

    pub(super) fn portal_region_set(&mut self, i: usize, x: i32, top: i32, bottom: i32) {
        if let Some(r) = self.x.portal.regions.get_mut(i) {
            r.set(x, top, bottom);
        }
    }

    /// The plane portal of sector `si`'s floor (0) or ceiling (1), when it
    /// shows on `pic`.
    pub(super) fn plane_portal(
        &self,
        level: &SoftLevel,
        si: usize,
        plane: usize,
        pic: u16,
    ) -> Option<u32> {
        let (i, sky_only) = level.plane_portals.get(si)?[plane]?;
        (!sky_only || pic == SKY_FLAT).then_some(PL_PORTAL | i)
    }

    /// Collect a portal visplane into its region (`R_DrawPlanes` skips it).
    pub(super) fn portal_plane(&mut self, pl: &Visplane) {
        let i = self.portal_region(RegionKey::Plane(pl.picnum & !PL_PORTAL));
        for x in pl.minx.max(0)..=pl.maxx.min(self.width - 1) {
            let t = pl.top[(x + 1) as usize];
            if t != PLANE_UNSET {
                let b = pl.bottom[(x + 1) as usize];
                self.portal_region_set(i, x, i32::from(t), i32::from(b));
            }
        }
    }

    /// A copy of the renderer for a sub-view (without this frame's big
    /// working sets).
    pub(super) fn sub_renderer(&mut self) -> SoftRenderer {
        let x = std::mem::take(&mut self.x);
        let planes = std::mem::take(&mut self.visplanes);
        let segs = std::mem::take(&mut self.drawsegs);
        let openings = std::mem::take(&mut self.openings);
        let sprites = std::mem::take(&mut self.vissprites);
        let pool = std::mem::take(&mut self.plane_pool);
        let slices = std::mem::take(&mut self.slices);
        let mut sub = self.clone();
        sub.threads = 1;
        self.plane_pool = pool;
        self.slices = slices;
        self.x = x;
        self.visplanes = planes;
        self.drawsegs = segs;
        self.openings = openings;
        self.vissprites = sprites;
        sub
    }

    /// Draw every portal region noted this frame.
    pub(super) fn draw_portals(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        frame: &SoftFrame,
        sectors: &[SoftSector],
        target: &mut SoftTarget,
    ) {
        let regions = std::mem::take(&mut self.x.portal.regions);
        for r in &regions {
            if r.minx > r.maxx {
                continue;
            }
            match r.key {
                RegionKey::Line(li, sector) => {
                    let Some((portal, _)) = level.line_portals.get(li as usize).copied().flatten()
                    else {
                        continue;
                    };
                    match portal {
                        SoftLinePortal::Horizon => {
                            self.draw_horizon(level, data, frame, sectors, r, sector, target);
                        }
                        SoftLinePortal::Mirror => {
                            self.draw_mirror(level, data, frame, r, li, target);
                        }
                        SoftLinePortal::Visual { .. } => {
                            self.draw_line_portal(level, data, frame, sectors, r, portal, target);
                        }
                    }
                }
                RegionKey::Plane(pi) => {
                    let Some(p) = level.sector_portals.get(pi as usize).copied() else {
                        continue;
                    };
                    let mut f = frame.clone();
                    match p {
                        SoftSectorPortal::Skybox {
                            x,
                            y,
                            z_offset,
                            sector,
                            angle,
                        } => {
                            let floor = sector
                                .and_then(|s| sectors.get(s as usize))
                                .map_or(0.0, |s| f64::from(s.floor) / UNIT);
                            f.x = (x * UNIT) as i32;
                            f.y = (y * UNIT) as i32;
                            f.z = ((floor + z_offset) * UNIT) as i32;
                            f.angle = frame.angle.wrapping_add(angle);
                        }
                        SoftSectorPortal::Stacked { dx, dy } => {
                            f.x = frame.x.wrapping_add((dx * UNIT) as i32);
                            f.y = frame.y.wrapping_add((dy * UNIT) as i32);
                        }
                    }
                    self.draw_sub_view(level, data, f, r, None, false, target);
                }
            }
        }
    }

    /// Render `f` inside region `r` (mirrored left to right for mirrors)
    /// and copy the region's pixels into `target`.
    #[allow(clippy::too_many_arguments)]
    fn draw_sub_view(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        mut f: SoftFrame,
        r: &Region,
        clip: Option<ClipLine>,
        mirror: bool,
        target: &mut SoftTarget,
    ) {
        if self.x.portal.depth >= MAX_PORTAL_DEPTH {
            return;
        }
        f.psprites.clear();
        let sub = SubView {
            region: if mirror { r.mirrored() } else { r.clone() },
            clip,
            depth: self.x.portal.depth + 1,
        };
        let mut view = self.sub_renderer();
        let mut scratch = SoftTarget::new(target.width, target.height);
        view.render_view(level, data, &f, Some(&sub), &mut scratch);
        let w = self.width as usize;
        let tc = !scratch.rgb.is_empty() && target.rgb.len() == target.pixels.len();
        for x in r.minx.max(0)..=r.maxx.min(self.width - 1) {
            let xu = x as usize;
            let sx = if mirror { w - 1 - xu } else { xu };
            let (t, b) = (
                i32::from(r.top[xu]).max(0),
                i32::from(r.bottom[xu]).min(self.height - 1),
            );
            for y in t..=b {
                let (d, s) = (y as usize * w + xu, y as usize * w + sx);
                target.pixels[d] = scratch.pixels[s];
                if tc {
                    target.rgb[d] = scratch.rgb[s];
                }
            }
        }
    }

    /// `Line_SetPortal`: the view continues out of the destination line.
    #[allow(clippy::too_many_arguments)]
    fn draw_line_portal(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        frame: &SoftFrame,
        sectors: &[SoftSector],
        r: &Region,
        portal: SoftLinePortal,
        target: &mut SoftTarget,
    ) {
        let SoftLinePortal::Visual {
            target: dst,
            s1,
            d2,
            cos,
            sin,
            angle,
            align,
            sectors: (from, to),
        } = portal
        else {
            return;
        };
        let (ex, ey) = (f64::from(frame.x) / UNIT, f64::from(frame.y) / UNIT);
        let (nx, ny) = (ex - s1.0, ey - s1.1);
        let (tx, ty) = (nx * cos - ny * sin + d2.0, ny * cos + nx * sin + d2.1);
        let mut f = frame.clone();
        f.x = (tx * UNIT) as i32;
        f.y = (ty * UNIT) as i32;
        f.angle = frame.angle.wrapping_add(angle);
        let sec = |i: u32| sectors.get(i as usize).copied().unwrap_or_default();
        f.z = match align {
            PORG_FLOOR => frame.z - sec(from).floor + sec(to).floor,
            PORG_CEILING => frame.z - sec(from).ceiling + sec(to).ceiling,
            _ => frame.z,
        };
        let clip = self.clip_line(level, dst, f.x, f.y);
        self.draw_sub_view(level, data, f, r, clip, false, target);
    }

    /// `Line_Mirror`: the view from the eye reflected in the line, drawn
    /// mirrored.
    fn draw_mirror(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        frame: &SoftFrame,
        r: &Region,
        line: u32,
        target: &mut SoftTarget,
    ) {
        let Some(&(a, b)) = level.line_ends.get(line as usize) else {
            return;
        };
        let (ax, ay) = (f64::from(a.x) / UNIT, f64::from(a.y) / UNIT);
        let (dx, dy) = (f64::from(b.x) / UNIT - ax, f64::from(b.y) / UNIT - ay);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            return;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (ex, ey) = (
            f64::from(frame.x) / UNIT - ax,
            f64::from(frame.y) / UNIT - ay,
        );
        let t = ex * ux + ey * uy;
        let (rx, ry) = (2.0 * t * ux - ex + ax, 2.0 * t * uy - ey + ay);
        let mut f = frame.clone();
        f.x = (rx * UNIT) as i32;
        f.y = (ry * UNIT) as i32;
        let line_angle = rad_to_bam(dy.atan2(dx));
        f.angle = line_angle.wrapping_mul(2).wrapping_sub(frame.angle);
        let _ = bam_to_rad;
        let clip = self.clip_line(level, line, f.x, f.y);
        self.draw_sub_view(level, data, f, r, clip, true, target);
    }

    /// Clip along `line`, keeping the side away from the eye `(ex, ey)`.
    fn clip_line(&self, level: &SoftLevel, line: u32, ex: i32, ey: i32) -> Option<ClipLine> {
        let &(a, b) = level.line_ends.get(line as usize)?;
        let (dx, dy) = (b.x.wrapping_sub(a.x), b.y.wrapping_sub(a.y));
        let eye_side = point_on_line(ex, ey, a.x, a.y, dx, dy);
        Some(ClipLine {
            x: a.x,
            y: a.y,
            dx,
            dy,
            keep: eye_side ^ 1,
            line,
        })
    }

    /// `Line_Horizon`: the front sector's floor and ceiling to the horizon.
    #[allow(clippy::too_many_arguments)]
    fn draw_horizon(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        frame: &SoftFrame,
        sectors: &[SoftSector],
        r: &Region,
        sector: u32,
        target: &mut SoftTarget,
    ) {
        let Some(&s) = sectors.get(sector as usize) else {
            return;
        };
        let w = self.width as usize;
        let mk = |height: i32, pic: u16, rows: &dyn Fn(i32, i32) -> (i32, i32)| {
            let sky = pic == SKY_FLAT;
            let mut pl = Visplane {
                height: if sky { 0 } else { height },
                picnum: u32::from(pic),
                light: if sky { 0 } else { s.light },
                xoffs: 0,
                yoffs: 0,
                tint: if sky { (0, 0) } else { tint_of(&s) },
                slope: None,
                xform: FlatXform::default(),
                minx: i32::MAX,
                maxx: -1,
                top: vec![PLANE_UNSET; w + 2],
                bottom: vec![0; w + 2],
            };
            for x in r.minx.max(0)..=r.maxx {
                let xu = x as usize;
                let (t, b) = rows(i32::from(r.top[xu]), i32::from(r.bottom[xu]));
                if t <= b && t >= 0 {
                    pl.top[xu + 1] = t as u16;
                    pl.bottom[xu + 1] = b as u16;
                    pl.minx = pl.minx.min(x);
                    pl.maxx = pl.maxx.max(x);
                }
            }
            pl
        };
        let cy = self.centery;
        let mut planes = Vec::new();
        if s.floor < self.viewz {
            planes.push(mk(s.floor, s.floor_pic, &|t, b| (t.max(cy), b)));
        }
        if s.ceiling > self.viewz || s.ceiling_pic == SKY_FLAT {
            planes.push(mk(s.ceiling, s.ceiling_pic, &|t, b| (t, b.min(cy - 1))));
        }
        let saved = std::mem::replace(&mut self.visplanes, planes);
        self.draw_planes(level, data, frame.sky, target);
        self.visplanes = saved;
        let _ = PL_SKYFLAT;
    }

    /// A Hexen double-sky column: the front sky with colour 0 clear.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sky_front_column(
        &self,
        data: &SoftData,
        tex: u16,
        col: i32,
        x: i32,
        yl: i32,
        yh: i32,
        texturemid: i32,
        iscale: i32,
        colormap: usize,
        target: &mut SoftTarget,
    ) {
        let Some(texture) = data.textures.get(tex as usize) else {
            return;
        };
        let source = texture.column(col);
        let h = (texture.height as i32).max(1);
        let px = self.px(data, colormap);
        let w = self.width as usize;
        let mut frac = texturemid.wrapping_add((yl - self.centery).wrapping_mul(iscale));
        for y in yl.max(0)..=yh.min(self.height - 1) {
            let t = (frac >> super::FRACBITS).rem_euclid(h) as usize;
            frac = frac.wrapping_add(iscale);
            let texel = source.get(t).copied().unwrap_or(0);
            if texel != 0 {
                px.put(target, y as usize * w + x as usize, texel);
            }
        }
    }

    /// Sky column angle for the default sky with its scroll (`which` 0 =
    /// sky1, 1 = sky2).
    pub(super) fn sky_angle(&self, x: i32, which: usize) -> i32 {
        (self
            .viewangle
            .wrapping_add(self.x.portal.sky_offsets[which])
            .wrapping_add(self.xtoviewangle[x as usize])
            >> ANGLETOSKYSHIFT) as i32
    }
}
