//! Sprites that are not upright billboards (SR6): `+WALLSPRITE`,
//! `+FLATSPRITE`, `+ROLLSPRITE` things and wall / floor decals.
//!
//! Each is a textured quad in the map. It is sorted and clipped against
//! walls like a sprite (by its centre), then drawn pixel by pixel: the
//! pixel's view ray meets the quad's plane, and the hit picks the texel and
//! its depth (light, depth buffer).

use super::{
    FRACUNIT, LIGHTLEVELS, LIGHTSEGSHIFT, MINZ, Px, SoftRenderer, SoftStyle, SoftTarget, VisSprite,
    ffloor::put_blend, tint_of,
};
use crate::data::SoftData;
use crate::fixed::fixed_div;
use crate::level::{SoftLevel, SoftSector};

/// How a thing's picture is laid out in the map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SoftSpriteKind {
    /// Faces the viewer (Doom's sprites).
    #[default]
    Billboard,
    /// Upright along the plane whose normal is the thing's angle
    /// (`+WALLSPRITE`).
    Wall,
    /// Flat at the thing's height, its top towards the thing's angle
    /// (`+FLATSPRITE`).
    Flat,
}

/// A decal: a picture on a wall (clipped to the wall part it sits on) or
/// on a floor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoftDecal {
    /// Picture lump (loaded by `SoftView::prepare`).
    pub picture: String,
    /// Centre (map units).
    pub x: f64,
    /// Centre (map units).
    pub y: f64,
    /// Centre height (map units); the floor height for a floor decal.
    pub z: f64,
    /// Size in map units.
    pub width: f64,
    /// Size in map units.
    pub height: f64,
    /// The line it is on (`None` with `flat`).
    pub line: Option<u32>,
    /// Lies on the floor.
    pub flat: bool,
    /// Light level 0..=255 (`None` = its sector's).
    pub light: Option<i32>,
    /// Opacity 0..=255.
    pub alpha: u8,
    /// Render style.
    pub style: SoftStyle,
    /// Fill colour (stencil styles).
    pub fill: u32,
}

/// A quad's texel `(col, row)` is at `o + col·u + row·v` (map units).
#[derive(Clone, Copy, Debug)]
pub(super) struct Quad {
    o: [f64; 3],
    u: [f64; 3],
    v: [f64; 3],
    w: i32,
    h: i32,
    /// Columns drawn (decals stop at the line's ends).
    cmin: f64,
    cmax: f64,
    /// Heights drawn (decals stay on their wall part).
    zmin: f64,
    zmax: f64,
    lightnum: usize,
    /// Colormap fixed for the whole quad (fullbright, fixed colormaps).
    fixed: bool,
    /// Screen rows it can cover.
    ylo: i32,
    yhi: i32,
}

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
#[inline]
fn mul(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

const UNIT: f64 = 65536.0;

impl SoftRenderer {
    fn eye(&self) -> [f64; 3] {
        [
            f64::from(self.viewx) / UNIT,
            f64::from(self.viewy) / UNIT,
            f64::from(self.viewz) / UNIT,
        ]
    }

    /// The viewer's right (map units, unit length).
    fn right(&self) -> [f64; 3] {
        let a = f64::from(self.viewangle) / 4_294_967_296.0 * std::f64::consts::TAU;
        [a.sin(), -a.cos(), 0.0]
    }

    /// A thing drawn as a quad (wall, flat or rolled sprite):
    /// `R_ProjectSprite` without the billboard's screen maths.
    pub(super) fn project_quad_thing(
        &mut self,
        data: &SoftData,
        thing: &super::SoftThing,
        lightnum: usize,
    ) {
        let Some(frames) = data.sprite_frames(super::sprite_str(&thing.sprite)) else {
            return;
        };
        let Some(sprframe) = frames.get((thing.frame & super::FF_FRAMEMASK) as usize) else {
            return;
        };
        let rot = if sprframe.rotate {
            let ang = self.point_to_angle(thing.x, thing.y);
            (ang.wrapping_sub(thing.angle)
                .wrapping_add((super::ANG45 / 2).wrapping_mul(9))
                >> 29) as usize
        } else {
            0
        };
        let (Some(lump), flip) = (sprframe.lump[rot], sprframe.flip[rot]) else {
            return;
        };
        let flip = flip ^ thing.xflip;
        let fullbright = thing.frame & super::FF_FULLBRIGHT != 0;
        let mut vis = VisSprite {
            x1: 0,
            x2: 0,
            gx: thing.x,
            gy: thing.y,
            gz: thing.z,
            gzt: thing.z,
            startfrac: 0,
            scale: FRACUNIT,
            xiscale: FRACUNIT,
            texturemid: 0,
            patch: lump,
            colormap: if thing.flags & super::MF_SHADOW != 0 {
                None
            } else if let Some(f) = self.fixed {
                Some(f)
            } else if fullbright {
                Some(0)
            } else {
                Some(self.scale_light(lightnum, FRACUNIT))
            },
            translation: ((thing.flags & super::MF_TRANSLATION) >> super::MF_TRANSSHIFT) as u8,
            tranmap: (thing.flags & super::MF_TRANSLUCENT != 0 && !data.tranmaps.is_empty())
                .then_some(0),
            heightsec: None,
            tint: if fullbright { (0, 0) } else { self.tint },
            yscale: FRACUNIT,
            yiscale: FRACUNIT,
            style: thing.style,
            alpha: u32::from(thing.alpha) * 256 / 255,
            fill: thing.fill,
            special: None,
            voxel: None,
            particle: None,
        };
        let fixed = vis.colormap.is_none() || self.fixed.is_some() || fullbright;
        let Some(patch) = data.patches.get(vis.patch as usize) else {
            return;
        };
        let s = |v: i32| {
            if v == 0 { 1.0 } else { f64::from(v) / UNIT }
        };
        let (sx, sy) = (s(thing.scale_x), s(thing.scale_y));
        let pos = [
            f64::from(thing.x) / UNIT,
            f64::from(thing.y) / UNIT,
            f64::from(thing.z) / UNIT,
        ];
        let yaw = f64::from(thing.angle) / 4_294_967_296.0 * std::f64::consts::TAU;
        let (w, h) = (f64::from(patch.width), f64::from(patch.height));
        let (left, top) = (f64::from(patch.left), f64::from(patch.top));
        let up = [0.0, 0.0, 1.0];
        // Width and height directions.
        let (across, down) = match thing.kind {
            SoftSpriteKind::Wall => ([-yaw.sin(), yaw.cos(), 0.0], mul(up, -1.0)),
            SoftSpriteKind::Flat => ([-yaw.sin(), yaw.cos(), 0.0], [-yaw.cos(), -yaw.sin(), 0.0]),
            SoftSpriteKind::Billboard => (self.right(), mul(up, -1.0)),
        };
        let (mut o, mut u, mut v) = (
            add(add(pos, mul(across, -left * sx)), mul(down, -top * sy)),
            mul(across, sx),
            mul(down, sy),
        );
        if flip {
            o = add(o, mul(u, w));
            u = mul(u, -1.0);
        }
        if thing.yflip {
            o = add(o, mul(v, h));
            v = mul(v, -1.0);
        }
        if thing.roll != 0 && thing.kind == SoftSpriteKind::Billboard {
            // Spin in the picture's plane about its origin (or centre).
            let th = f64::from(thing.roll) / 4_294_967_296.0 * std::f64::consts::TAU;
            let (sn, cs) = th.sin_cos();
            let r = self.right();
            let rot = |p: [f64; 3]| {
                let (a, b) = (dot(p, r), p[2]);
                add(mul(r, a * cs - b * sn), mul(up, a * sn + b * cs))
            };
            let pivot = if thing.roll_center {
                add(o, add(mul(u, w / 2.0), mul(v, h / 2.0)))
            } else {
                pos
            };
            o = add(pivot, rot(sub(o, pivot)));
            u = rot(u);
            v = rot(v);
        }
        let quad = Quad {
            o,
            u,
            v,
            w: patch.width,
            h: patch.height,
            cmin: 0.0,
            cmax: w,
            zmin: f64::MIN,
            zmax: f64::MAX,
            lightnum,
            fixed,
            ylo: 0,
            yhi: 0,
        };
        if self.place_quad(&mut vis, quad) {
            self.vissprites.push(vis);
        }
    }

    /// Screen extent, centre and scale of `quad` into `vis`; false when
    /// off screen or behind the viewer.
    fn place_quad(&mut self, vis: &mut VisSprite, mut quad: Quad) -> bool {
        let e = self.eye();
        let r = self.right();
        let fwd = [-r[1], r[0], 0.0];
        let proj = f64::from(self.projection) / UNIT;
        let cx = f64::from(self.centerxfrac) / UNIT;
        let (c0, c1) = (quad.cmin, quad.cmax);
        let corners = [
            add(quad.o, mul(quad.u, c0)),
            add(quad.o, mul(quad.u, c1)),
            add(add(quad.o, mul(quad.u, c0)), mul(quad.v, f64::from(quad.h))),
            add(add(quad.o, mul(quad.u, c1)), mul(quad.v, f64::from(quad.h))),
        ];
        let near = 1.0;
        let cy = f64::from(self.centery);
        let (mut x1, mut x2) = (f64::MAX, f64::MIN);
        let (mut y1, mut y2) = (f64::MAX, f64::MIN);
        let mut behind = 0;
        for c in corners {
            let d = sub(c, e);
            let z = dot(d, fwd);
            if z < near {
                behind += 1;
                continue;
            }
            let sx = cx + dot(d, r) * proj / z;
            let sy = cy - d[2] * proj / z;
            x1 = x1.min(sx);
            x2 = x2.max(sx);
            y1 = y1.min(sy);
            y2 = y2.max(sy);
        }
        if behind == 4 {
            return false;
        }
        if behind > 0 {
            // Reaches behind the viewer: test every pixel.
            x1 = 0.0;
            x2 = f64::from(self.width);
            y1 = 0.0;
            y2 = f64::from(self.height);
        }
        quad.ylo = (y1.floor() as i32 - 1).max(0);
        quad.yhi = (y2.ceil() as i32 + 1).min(self.height - 1);
        let x1 = (x1.floor() as i32).max(0);
        let x2 = (x2.ceil() as i32).min(self.width - 1);
        if x1 > x2 {
            return false;
        }
        let centre = add(
            quad.o,
            add(
                mul(quad.u, (c0 + c1) / 2.0),
                mul(quad.v, f64::from(quad.h) / 2.0),
            ),
        );
        let z = dot(sub(centre, e), fwd).max(f64::from(MINZ) / UNIT);
        vis.x1 = x1;
        vis.x2 = x2;
        vis.scale = fixed_div(self.projection, (z * UNIT) as i32);
        vis.gx = (centre[0] * UNIT) as i32;
        vis.gy = (centre[1] * UNIT) as i32;
        let zs = corners.map(|c| c[2]);
        let (lo, hi) = (
            zs.iter().copied().fold(f64::MAX, f64::min),
            zs.iter().copied().fold(f64::MIN, f64::max),
        );
        vis.gz = (lo.max(quad.zmin) * UNIT) as i32;
        vis.gzt = (hi.min(quad.zmax) * UNIT) as i32;
        vis.special = Some(self.x.quads.len() as u32);
        self.x.quads.push(quad);
        true
    }

    /// Project the frame's decals (after the BSP walk).
    pub(super) fn add_decals(
        &mut self,
        level: &SoftLevel,
        data: &SoftData,
        sectors: &[SoftSector],
        decals: &[SoftDecal],
    ) {
        let e = self.eye();
        if decals.is_empty() {
            return;
        }
        // Segs of the lines decals sit on (one pass over the segs).
        let mut by_line: std::collections::HashMap<u32, Vec<&crate::level::SoftSeg>> = decals
            .iter()
            .filter_map(|d| d.line)
            .map(|l| (l, Vec::new()))
            .collect();
        for seg in &level.segs {
            if let Some(list) = by_line.get_mut(&seg.line) {
                list.push(seg);
            }
        }
        for d in decals {
            let Some(patch_id) = data.picture_id(&d.picture) else {
                continue;
            };
            let Some(patch) = data.patches.get(patch_id as usize) else {
                continue;
            };
            if d.width <= 0.0 || d.height <= 0.0 || patch.width <= 0 || patch.height <= 0 {
                continue;
            }
            let (pw, ph) = (f64::from(patch.width), f64::from(patch.height));
            let sector = level
                .point_in_sector((d.x * UNIT) as i32, (d.y * UNIT) as i32)
                .unwrap_or(0);
            let sec = sectors.get(sector).copied().unwrap_or_default();
            let mut quad;
            if d.flat {
                // Picture top towards +y, as the map is drawn.
                let o = [d.x - d.width / 2.0, d.y + d.height / 2.0, d.z + 0.05];
                quad = Quad {
                    o,
                    u: [d.width / pw, 0.0, 0.0],
                    v: [0.0, -d.height / ph, 0.0],
                    w: patch.width,
                    h: patch.height,
                    cmin: 0.0,
                    cmax: pw,
                    zmin: f64::MIN,
                    zmax: f64::MAX,
                    lightnum: 0,
                    fixed: false,
                    ylo: 0,
                    yhi: 0,
                };
                if e[2] <= d.z {
                    continue;
                }
            } else {
                let Some(li) = d.line else { continue };
                let Some((a, b, front, back)) =
                    by_line.get(&li).and_then(|segs| line_ends(level, segs))
                else {
                    continue;
                };
                let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                if len < 1e-6 {
                    continue;
                }
                let dir = [(b[0] - a[0]) / len, (b[1] - a[1]) / len, 0.0];
                // Which face: the decal's side of the line, else the viewer's.
                let side_of =
                    |p: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
                let viewer_side = side_of([e[0], e[1]]);
                let ds = side_of([d.x, d.y]);
                let side = if ds.abs() > 0.05 * len {
                    ds
                } else {
                    viewer_side
                };
                if side * viewer_side <= 0.0 {
                    continue;
                }
                // Left side (positive cross) is the back of the line.
                let (own, other) = if side > 0.0 {
                    (back, front)
                } else {
                    (front, back)
                };
                let Some(own) = own.and_then(|s| sectors.get(s as usize)) else {
                    continue;
                };
                let (zmin, zmax) = match other.and_then(|s| sectors.get(s as usize)) {
                    None => (own.floor, own.ceiling),
                    Some(o) => {
                        let cz = (d.z * UNIT) as i32;
                        if cz > o.ceiling {
                            (o.ceiling.max(own.floor), own.ceiling)
                        } else if cz < o.floor {
                            (own.floor, o.floor.min(own.ceiling))
                        } else {
                            (own.floor.max(o.floor), own.ceiling.min(o.ceiling))
                        }
                    }
                };
                // Texture left to right as the viewer sees it.
                let r = self.right();
                let across = if dot(dir, r) < 0.0 {
                    mul(dir, -1.0)
                } else {
                    dir
                };
                let n = [-dir[1], dir[0], 0.0];
                let n = if dot(n, sub(e, [d.x, d.y, d.z])) < 0.0 {
                    mul(n, -1.0)
                } else {
                    n
                };
                let su = d.width / pw;
                let o = add(
                    add(
                        [d.x, d.y, d.z + d.height / 2.0],
                        mul(across, -d.width / 2.0),
                    ),
                    mul(n, 0.25),
                );
                // Columns inside the line's ends.
                let along =
                    |p: [f64; 2]| ((p[0] - o[0]) * across[0] + (p[1] - o[1]) * across[1]) / su;
                let (ca, cb) = (along(a), along(b));
                quad = Quad {
                    o,
                    u: mul(across, su),
                    v: [0.0, 0.0, -d.height / ph],
                    w: patch.width,
                    h: patch.height,
                    cmin: ca.min(cb).max(0.0),
                    cmax: ca.max(cb).min(pw),
                    zmin: f64::from(zmin) / UNIT,
                    zmax: f64::from(zmax) / UNIT,
                    lightnum: 0,
                    fixed: false,
                    ylo: 0,
                    yhi: 0,
                };
                if quad.cmin >= quad.cmax || quad.zmin >= quad.zmax {
                    continue;
                }
            }
            let light = d.light.unwrap_or_else(|| {
                if self.has_slabs(sector) {
                    self.light_at(sector, (d.z * UNIT) as i32, sec.light)
                } else {
                    sec.light
                }
            });
            quad.lightnum = ((light >> LIGHTSEGSHIFT) + self.extralight)
                .clamp(0, LIGHTLEVELS as i32 - 1) as usize;
            quad.fixed = self.fixed.is_some();
            let mut vis = VisSprite {
                x1: 0,
                x2: 0,
                gx: 0,
                gy: 0,
                gz: 0,
                gzt: 0,
                startfrac: 0,
                scale: FRACUNIT,
                xiscale: FRACUNIT,
                texturemid: 0,
                patch: patch_id,
                colormap: Some(self.fixed.unwrap_or(0)),
                translation: 0,
                tranmap: None,
                heightsec: None,
                tint: tint_of(&sec),
                yscale: FRACUNIT,
                yiscale: FRACUNIT,
                style: d.style,
                alpha: u32::from(d.alpha) * 256 / 255,
                fill: d.fill,
                special: None,
                voxel: None,
                particle: None,
            };
            if d.style == SoftStyle::Normal && d.alpha < 255 {
                vis.style = SoftStyle::Translucent;
            }
            if self.place_quad(&mut vis, quad) {
                self.vissprites.push(vis);
            }
        }
    }

    /// Draw a quad sprite inside the clip rows (`cliptop` / `clipbot`).
    pub(super) fn draw_quad(
        &mut self,
        data: &SoftData,
        vis: &VisSprite,
        quad: Quad,
        clipped: bool,
        target: &mut SoftTarget,
    ) {
        let Some(patch) = data.patches.get(vis.patch as usize) else {
            return;
        };
        self.tint = vis.tint;
        let e = self.eye();
        let proj = f64::from(self.projection) / UNIT;
        let n = cross(quad.u, quad.v);
        let w = self.width as usize;
        let opaque =
            vis.colormap.is_some() && vis.tranmap.is_none() && vis.style == SoftStyle::Normal;
        let r = sub(e, quad.o);
        let translation = vis.translation;
        for x in vis.x1..=vis.x2 {
            let xu = x as usize;
            let (top, bot) = if clipped {
                (
                    i32::from(self.cliptop[xu]) + 1,
                    i32::from(self.clipbot[xu]) - 1,
                )
            } else {
                (0, self.height - 1)
            };
            let (rx, ry) = self.col_dirs[xu];
            for y in top.max(quad.ylo)..=bot.min(quad.yhi) {
                let dz = (f64::from(self.centery) - f64::from(y) - 0.5) / proj;
                let dir = [rx, ry, dz];
                let den = -dot(n, dir);
                if den.abs() < 1e-12 {
                    continue;
                }
                // Cramer: o + a·u + b·v = e + t·dir.
                let md = mul(dir, -1.0);
                let a = dot(r, cross(quad.v, md)) / den;
                let b = dot(quad.u, cross(r, md)) / den;
                let t = dot(quad.u, cross(quad.v, r)) / den;
                if t <= 0.0 || a < quad.cmin || a >= quad.cmax || b < 0.0 || b >= f64::from(quad.h)
                {
                    continue;
                }
                let hz = e[2] + t * dz;
                if hz < quad.zmin || hz > quad.zmax {
                    continue;
                }
                let (col, row) = ((a as i32).min(quad.w - 1), b as i32);
                let Some(posts) = patch.columns.get(col as usize) else {
                    continue;
                };
                let Some(post) = posts
                    .iter()
                    .find(|p| row >= p.top && row < p.top + p.pixels.len() as i32)
                else {
                    continue;
                };
                let k = (row - post.top) as usize;
                let i = y as usize * w + xu;
                let scale = (f64::from(self.projection) / t).min(f64::from(i32::MAX)) as i32;
                if self.z_hidden(i, scale) {
                    continue;
                }
                let mut texel = post.pixels[k];
                if translation > 0 {
                    texel =
                        self.translations[usize::from(translation - 1).min(2)][usize::from(texel)];
                }
                let rgb = if self.truecolor {
                    post.rgb.as_ref().and_then(|c| c.get(k).copied())
                } else {
                    None
                };
                let Some(cm) = vis.colormap else {
                    // Fuzz: darken what is there.
                    if self.truecolor {
                        let c = target.rgb[i];
                        let f = |s: u32| ((c >> s) & 255) * 26 / 32;
                        target.rgb[i] = (f(16) << 16) | (f(8) << 8) | f(0);
                    } else {
                        target.pixels[i] = self.cmap(data, 6)[usize::from(target.pixels[i])];
                    }
                    continue;
                };
                let colormap = if quad.fixed {
                    cm
                } else {
                    self.scale_light(quad.lightnum, scale)
                };
                let px: Px<'_> = self.px(data, colormap);
                if opaque {
                    px.put_rgb(target, i, texel, rgb);
                    if self.x.zon {
                        self.x.zbuf[i] = self.x.zbuf[i].max(scale);
                    }
                } else {
                    let (style, alpha) = if vis.tranmap.is_some() {
                        (SoftStyle::Translucent, 169)
                    } else {
                        (vis.style, vis.alpha)
                    };
                    let (style, texel, rgb) = match style {
                        SoftStyle::Stencil
                        | SoftStyle::AddStencil
                        | SoftStyle::Shaded
                        | SoftStyle::AddShaded => {
                            let s = if matches!(style, SoftStyle::AddStencil | SoftStyle::AddShaded)
                            {
                                SoftStyle::Add
                            } else {
                                SoftStyle::Translucent
                            };
                            (s, data.nearest_rgb(super::unpack(vis.fill)), Some(vis.fill))
                        }
                        s => (s, texel, rgb),
                    };
                    put_blend(
                        px,
                        data,
                        target,
                        i,
                        texel,
                        rgb,
                        style,
                        alpha,
                        self.truecolor,
                    );
                }
            }
        }
    }
}

/// A line's two ends (map units), front and back sector.
type LineEnds = ([f64; 2], [f64; 2], Option<u32>, Option<u32>);

/// A line's ends (map units) and its front / back sectors.
fn line_ends(level: &SoftLevel, segs: &[&crate::level::SoftSeg]) -> Option<LineEnds> {
    // The line's first seg on each side gives its sectors; its ends are the
    // extreme seg vertices along it.
    let first = segs.first()?;
    let f = |v: u32| {
        let p = level.vertices[v as usize];
        [f64::from(p.x) / UNIT, f64::from(p.y) / UNIT]
    };
    let (p0, p1) = (f(first.v1), f(first.v2));
    let dir = [p1[0] - p0[0], p1[1] - p0[1]];
    // Orient along the front side's segs.
    let front_seg = segs
        .iter()
        .find(|s| {
            let (a, b) = (f(s.v1), f(s.v2));
            (b[0] - a[0]) * dir[0] + (b[1] - a[1]) * dir[1] > 0.0
        })
        .copied()
        .unwrap_or(first);
    let fdir = {
        let (a, b) = (f(front_seg.v1), f(front_seg.v2));
        [b[0] - a[0], b[1] - a[1]]
    };
    let mut pts: Vec<[f64; 2]> = segs.iter().flat_map(|s| [f(s.v1), f(s.v2)]).collect();
    pts.sort_by(|a, b| {
        (a[0] * fdir[0] + a[1] * fdir[1]).total_cmp(&(b[0] * fdir[0] + b[1] * fdir[1]))
    });
    let (a, b) = (*pts.first()?, *pts.last()?);
    // Sides: a seg running along `fdir` has the line's front on its right.
    let along = |s: &&crate::level::SoftSeg| {
        let (x, y) = (f(s.v1), f(s.v2));
        (y[0] - x[0]) * fdir[0] + (y[1] - x[1]) * fdir[1] > 0.0
    };
    let front = segs.iter().find(|s| along(s)).map(|s| s.front);
    let back = segs
        .iter()
        .find(|s| !along(s))
        .map(|s| s.front)
        .or_else(|| front_seg.back.filter(|_| front.is_some()));
    Some((a, b, front, back))
}
