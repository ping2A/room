//! Voxels, particles and dynamic lights (SR8).
//!
//! Voxels (VOXELDEF / KVX): each voxel of the model is placed in the map by
//! the same matrix as the hardware renderer's (angle offset, voxel offset,
//! scale, spin, roll, pitch, yaw), and splatted as a square the size of a
//! voxel at its depth; the depth buffer sorts the voxels among themselves
//! and against slabs and other voxels. The model is sorted with sprites and
//! clipped against walls by its centre, like a sprite.
//!
//! Particles: squares of colour, sized by their map size, sorted and
//! clipped like sprites, blended (translucent or additive).
//!
//! Dynamic lights (true colour only, like UZDoom's software `r_dynlights`):
//! a light adds `texel × colour × (1 − distance / radius)` (and the cosine
//! towards the light on walls and planes) to each pixel of walls, planes,
//! sprites, voxels and particles in its reach.

use super::{
    FRACUNIT, LIGHTLEVELS, LIGHTSEGSHIFT, MINZ, Px, SoftRenderer, SoftStyle, SoftTarget, VisSprite,
    ffloor::put_blend, pack, unpack,
};
use crate::data::SoftData;
use crate::fixed::fixed_div;
use crate::level::SoftVertex;

/// How a thing's voxel model is placed (VOXELDEF and the actor).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoftVoxelPose {
    /// Voxel model ([`SoftData::voxel`]).
    pub id: u32,
    /// VOXELDEF `AngleOffset` (BAM; GZDoom's default 90°).
    pub angle_offset: u32,
    /// `PlacedSpin` / `DroppedSpin` turn this tic (BAM).
    pub spin: u32,
    /// Pitch and roll (BAM, as the hardware renderer sums them).
    pub pitch: u32,
    /// See `pitch`.
    pub roll: u32,
    /// Scale per axis (x, y, z; 16.16): VOXELDEF `Scale` × actor scale.
    pub scale: [i32; 3],
    /// `XOffset` / `YOffset` / `ZOffset` over the VOXELDEF scale (16.16).
    pub offset: [i32; 3],
}

/// A particle (`A_SpawnParticle`, puffs, blood).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoftParticle {
    /// Centre (map units).
    pub x: f64,
    /// Centre (map units).
    pub y: f64,
    /// Centre (map units).
    pub z: f64,
    /// Size (map units).
    pub size: f64,
    /// Colour 0xRRGGBB.
    pub color: u32,
    /// Opacity 0..=255.
    pub alpha: u8,
    /// Additive.
    pub additive: bool,
    /// Light level 0..=255 (255 for fullbright particles).
    pub light: i32,
}

/// A dynamic light (true colour only).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SoftLight {
    /// Centre (map units).
    pub x: f64,
    /// Centre (map units).
    pub y: f64,
    /// Centre (map units).
    pub z: f64,
    /// Radius (map units).
    pub radius: f64,
    /// Colour, 0..=1 per channel (more brightens further).
    pub color: [f32; 3],
    /// Darkens instead.
    pub subtractive: bool,
}

/// A voxel model to draw.
#[derive(Clone, Copy, Debug)]
pub(super) struct VoxJob {
    id: u32,
    /// Render-space (y up) matrix and translation, applied to `(x, z, y)`.
    m: [[f64; 3]; 3],
    t: [f64; 3],
    origin: [f64; 3],
    /// Splat size (map units).
    size: f64,
    lightnum: usize,
    fixed: bool,
    dyn_light: Option<[f32; 3]>,
}

/// A particle to draw, with its screen square.
#[derive(Clone, Copy, Debug)]
pub(super) struct PartJob {
    p: SoftParticle,
    sy: f64,
    half: f64,
    dyn_light: Option<[f32; 3]>,
}

/// Dynamic light state of the frame and of what is being drawn.
#[derive(Clone, Debug, Default)]
pub(super) struct DynState {
    pub active: bool,
    lights: Vec<SoftLight>,
    /// The wall being drawn: ends and normal (towards its front).
    wall: Option<(SoftVertex, SoftVertex, [f64; 3])>,
    /// The wall column being drawn: map point, normal, scale and the
    /// lights that reach it.
    pub col: Option<([f64; 2], [f64; 3], i32)>,
    col_lights: Vec<SoftLight>,
    /// Light on the sprite being drawn.
    pub sprite: Option<[f32; 3]>,
}

type M3 = [[f64; 3]; 3];

fn mm(a: &M3, b: &M3) -> M3 {
    let mut o = [[0.0; 3]; 3];
    for (i, row) in o.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}

fn mv(a: &M3, v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i][0] * v[0] + a[i][1] * v[1] + a[i][2] * v[2])
}

/// glam's right-handed rotations (render space, y up).
fn rot_x(r: f64) -> M3 {
    let (s, c) = r.sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}
fn rot_y(r: f64) -> M3 {
    let (s, c) = r.sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}
fn rot_z(r: f64) -> M3 {
    let (s, c) = r.sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

fn rad(a: u32) -> f64 {
    let d = f64::from(a) / 4_294_967_296.0 * 360.0;
    let d = if d > 180.0 { d - 360.0 } else { d };
    d.to_radians()
}

const UNIT: f64 = 65536.0;

impl SoftRenderer {
    /// Frame start: the lights in use (true colour, no fixed colormap).
    pub(super) fn begin_dyn(&mut self, frame: &super::SoftFrame) {
        let d = &mut self.x.dl;
        d.active = self.truecolor && self.fixed.is_none() && !frame.lights.is_empty();
        d.lights.clear();
        d.wall = None;
        d.col = None;
        d.sprite = None;
        if d.active {
            d.lights
                .extend(frame.lights.iter().filter(|l| l.radius > 0.0));
            d.active = !d.lights.is_empty();
        }
    }

    /// Light reaching `p` (map units), with the surface normal `n` when
    /// the surface faces one way. `None` when nothing reaches it.
    pub(super) fn dyn_at(
        &self,
        lights: &[SoftLight],
        p: [f64; 3],
        n: Option<[f64; 3]>,
    ) -> Option<[f32; 3]> {
        let mut sum = [0.0f32; 3];
        let mut any = false;
        for l in lights {
            let d = [l.x - p[0], l.y - p[1], l.z - p[2]];
            let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            if d2 >= l.radius * l.radius {
                continue;
            }
            let dist = d2.sqrt();
            let mut f = 1.0 - dist / l.radius;
            if let Some(n) = n {
                if dist > 1e-6 {
                    f *= ((n[0] * d[0] + n[1] * d[1] + n[2] * d[2]) / dist).max(0.0);
                }
            }
            if f <= 0.0 {
                continue;
            }
            let s = if l.subtractive { -1.0 } else { 1.0 };
            for (acc, c) in sum.iter_mut().zip(l.color) {
                *acc += s * c * f as f32;
            }
            any = true;
        }
        any.then_some(sum)
    }

    /// Add `base × light` to true-colour pixel `i`.
    #[inline]
    pub(super) fn dyn_add(target: &mut SoftTarget, i: usize, base: [u32; 3], l: [f32; 3]) {
        let c = unpack(target.rgb[i]);
        let ch = |k: usize| (c[k] as f32 + base[k] as f32 * l[k]).clamp(0.0, 255.0) as u32;
        target.rgb[i] = pack(ch(0), ch(1), ch(2));
    }

    /// A texel's unlit colour (its own true colour, or the palette's).
    #[inline]
    pub(super) fn texel_rgb(data: &SoftData, texel: u8, rgb: Option<u32>) -> [u32; 3] {
        match rgb {
            Some(c) => unpack(c),
            None => {
                let [r, g, b] = data.palette[usize::from(texel)];
                [u32::from(r), u32::from(g), u32::from(b)]
            }
        }
    }

    /// Wall start: its ends and the normal towards its front.
    pub(super) fn dyn_wall(&mut self, v1: SoftVertex, v2: SoftVertex) {
        if !self.x.dl.active {
            self.x.dl.wall = None;
            return;
        }
        let (dx, dy) = (f64::from(v2.x - v1.x) / UNIT, f64::from(v2.y - v1.y) / UNIT);
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        self.x.dl.wall = Some((v1, v2, [dy / len, -dx / len, 0.0]));
    }

    /// Wall column start (`R_RenderSegLoop`): the lights reaching it.
    pub(super) fn dyn_column(&mut self, x: i32, scale: i32) {
        let Some((v1, v2, n)) = self.x.dl.wall else {
            self.x.dl.col = None;
            return;
        };
        let (mx, my) = self.seg_point_at_column(v1, v2, x);
        let mut near = std::mem::take(&mut self.x.dl.col_lights);
        near.clear();
        near.extend(self.x.dl.lights.iter().filter(|l| {
            let (dx, dy) = (l.x - mx, l.y - my);
            dx * dx + dy * dy < l.radius * l.radius && dx * n[0] + dy * n[1] > 0.0
        }));
        self.x.dl.col = (!near.is_empty()).then_some(([mx, my], n, scale));
        self.x.dl.col_lights = near;
    }

    pub(super) fn dyn_column_end(&mut self) {
        self.x.dl.col = None;
    }

    /// Light a wall pixel in row `y` of the current column.
    #[inline]
    pub(super) fn dyn_wall_pixel(&self, target: &mut SoftTarget, i: usize, y: i32, base: [u32; 3]) {
        let Some((p, n, scale)) = self.x.dl.col else {
            return;
        };
        let z = f64::from(self.viewz) / UNIT
            + (f64::from(self.centery) - f64::from(y) - 0.5) * UNIT / f64::from(scale.max(1));
        if let Some(l) = self.dyn_at(&self.x.dl.col_lights, [p[0], p[1], z], Some(n)) {
            Self::dyn_add(target, i, base, l);
        }
    }

    /// Light a plane pixel at map point `p` on a plane with normal `n`.
    #[inline]
    pub(super) fn dyn_plane_pixel(
        &self,
        target: &mut SoftTarget,
        i: usize,
        p: [f64; 3],
        n: [f64; 3],
        base: [u32; 3],
    ) {
        if let Some(l) = self.dyn_at(&self.x.dl.lights, p, Some(n)) {
            Self::dyn_add(target, i, base, l);
        }
    }

    /// Light on a thing at `p` (sprites, voxels, particles).
    pub(super) fn dyn_thing(&self, p: [f64; 3]) -> Option<[f32; 3]> {
        if !self.x.dl.active {
            return None;
        }
        self.dyn_at(&self.x.dl.lights, p, None)
    }

    /// `R_ProjectSprite` for a voxel model.
    pub(super) fn project_voxel(
        &mut self,
        data: &SoftData,
        thing: &super::SoftThing,
        pose: SoftVoxelPose,
        lightnum: usize,
    ) {
        let Some(vox) = data.voxels.get(pose.id as usize) else {
            return;
        };
        let s = |v: i32| if v == 0 { 1.0 } else { f64::from(v) / UNIT };
        let (sx, sy, sz) = (s(pose.scale[0]), s(pose.scale[1]), s(pose.scale[2]));
        let yaw = f64::from(thing.angle) / 4_294_967_296.0 * std::f64::consts::TAU;
        // GZDoom's voxel matrix in render space (see `voxel_model_matrix`).
        let scale_m = [[sx, 0.0, 0.0], [0.0, sz, 0.0], [0.0, 0.0, sy]];
        let a = mm(
            &mm(
                &mm(&rot_y(-yaw), &rot_z(rad(pose.pitch))),
                &rot_x(-rad(pose.roll)),
            ),
            &mm(&rot_y(rad(pose.spin)), &scale_m),
        );
        let m = mm(&a, &rot_y(-rad(pose.angle_offset)));
        let off = [
            f64::from(pose.offset[0]) / UNIT,
            f64::from(pose.offset[2]) / UNIT,
            f64::from(pose.offset[1]) / UNIT,
        ];
        let t = mv(&a, off);
        let origin = [
            f64::from(thing.x) / UNIT,
            f64::from(thing.y) / UNIT,
            f64::from(thing.z) / UNIT,
        ];
        let fullbright = thing.frame & super::FF_FULLBRIGHT != 0;
        let mut job = VoxJob {
            id: pose.id,
            m,
            t,
            origin,
            size: sx.abs().max(sy.abs()).max(sz.abs()),
            lightnum,
            fixed: self.fixed.is_some() || fullbright,
            dyn_light: None,
        };
        // The model's box, for the screen extent.
        let [w, d, h] = vox.size.map(f64::from);
        let p = vox.pivot.map(f64::from);
        let mut corners = Vec::with_capacity(8);
        for cx in [-p[0], w - p[0]] {
            for cy in [p[1] - d, p[1]] {
                for cz in [p[2] - h, p[2]] {
                    corners.push(self.voxel_world(&job, [cx, cy, cz]));
                }
            }
        }
        let Some((x1, x2, centre_z)) = self.screen_extent(&corners) else {
            return;
        };
        let zs = corners.iter().map(|c| c[2]);
        let lo = zs.clone().fold(f64::MAX, f64::min);
        let hi = zs.fold(f64::MIN, f64::max);
        job.dyn_light = self.dyn_thing([origin[0], origin[1], (lo + hi) / 2.0]);
        let scale = fixed_div(self.projection, (centre_z * UNIT) as i32);
        let vis = VisSprite {
            x1,
            x2,
            gx: thing.x,
            gy: thing.y,
            gz: (lo * UNIT) as i32,
            gzt: (hi * UNIT) as i32,
            startfrac: 0,
            scale,
            xiscale: FRACUNIT,
            texturemid: 0,
            patch: 0,
            colormap: Some(if let Some(f) = self.fixed {
                f
            } else if fullbright {
                0
            } else {
                self.scale_light(lightnum, scale)
            }),
            translation: 0,
            tranmap: None,
            heightsec: None,
            tint: if fullbright { (0, 0) } else { self.tint },
            yscale: FRACUNIT,
            yiscale: FRACUNIT,
            style: SoftStyle::Normal,
            alpha: 256,
            fill: 0,
            special: None,
            voxel: Some(self.x.voxjobs.len() as u32),
            particle: None,
        };
        self.x.voxjobs.push(job);
        self.vissprites.push(vis);
    }

    /// Map point of model point `c` (room axes: x, y, z up, about the pivot).
    fn voxel_world(&self, job: &VoxJob, c: [f64; 3]) -> [f64; 3] {
        let r = mv(&job.m, [c[0], c[2], c[1]]);
        [
            r[0] + job.t[0] + job.origin[0],
            r[2] + job.t[2] + job.origin[1],
            r[1] + job.t[1] + job.origin[2],
        ]
    }

    /// Screen columns of points (map units) and the depth of their middle;
    /// `None` when all are behind the viewer or off screen.
    fn screen_extent(&self, pts: &[[f64; 3]]) -> Option<(i32, i32, f64)> {
        let (e, r, fwd, proj, cx) = self.view_basis();
        let (mut x1, mut x2, mut zsum, mut n) = (f64::MAX, f64::MIN, 0.0, 0.0);
        let mut behind = false;
        for p in pts {
            let d = [p[0] - e[0], p[1] - e[1], p[2] - e[2]];
            let z = d[0] * fwd[0] + d[1] * fwd[1];
            if z < 1.0 {
                behind = true;
                continue;
            }
            let sx = cx + (d[0] * r[0] + d[1] * r[1]) * proj / z;
            x1 = x1.min(sx);
            x2 = x2.max(sx);
            zsum += z;
            n += 1.0;
        }
        if n == 0.0 {
            return None;
        }
        if behind {
            x1 = 0.0;
            x2 = f64::from(self.width);
        }
        let a = (x1.floor() as i32).max(0);
        let b = (x2.ceil() as i32).min(self.width - 1);
        (a <= b).then_some((a, b, (zsum / n).max(f64::from(MINZ) / UNIT)))
    }

    /// Eye, right and forward (map units), projection and centre column.
    fn view_basis(&self) -> ([f64; 3], [f64; 2], [f64; 2], f64, f64) {
        let a = f64::from(self.viewangle) / 4_294_967_296.0 * std::f64::consts::TAU;
        (
            [
                f64::from(self.viewx) / UNIT,
                f64::from(self.viewy) / UNIT,
                f64::from(self.viewz) / UNIT,
            ],
            [a.sin(), -a.cos()],
            [a.cos(), a.sin()],
            f64::from(self.projection) / UNIT,
            f64::from(self.centerxfrac) / UNIT,
        )
    }

    /// Draw a voxel model: every voxel a depth-tested square.
    pub(super) fn draw_voxel(
        &mut self,
        data: &SoftData,
        vis: &VisSprite,
        job: VoxJob,
        clipped: bool,
        target: &mut SoftTarget,
    ) {
        let Some(vox) = data.voxels.get(job.id as usize) else {
            return;
        };
        self.zbuf_on();
        self.tint = vis.tint;
        let (e, r, fwd, proj, cx) = self.view_basis();
        let cy = f64::from(self.centery);
        let w = self.width as usize;
        let p = vox.pivot.map(f64::from);
        let fixed_cm = vis.colormap.unwrap_or(0);
        for (k, v) in vox.voxels.iter().enumerate() {
            let c = [
                f64::from(v[0]) - p[0] + 0.5,
                p[1] - f64::from(v[1]) + 0.5,
                p[2] - f64::from(v[2]) + 0.5,
            ];
            let wp = self.voxel_world(&job, c);
            let d = [wp[0] - e[0], wp[1] - e[1], wp[2] - e[2]];
            let z = d[0] * fwd[0] + d[1] * fwd[1];
            if z < 1.0 {
                continue;
            }
            let sx = cx + (d[0] * r[0] + d[1] * r[1]) * proj / z;
            let sy = cy - d[2] * proj / z;
            // A voxel cube seen from any side: about 1.2 voxels across.
            let half = (job.size * 0.6 * proj / z).max(0.5);
            let xa = ((sx - half).floor() as i32).max(vis.x1);
            let xb = ((sx + half).ceil() as i32 - 1).min(vis.x2);
            let ya = ((sy - half).floor() as i32).max(0);
            let yb = ((sy + half).ceil() as i32 - 1).min(self.height - 1);
            if xa > xb || ya > yb {
                continue;
            }
            let scale = (f64::from(self.projection) / z).min(f64::from(i32::MAX)) as i32;
            let colormap = if job.fixed {
                fixed_cm
            } else {
                self.scale_light(job.lightnum, scale)
            };
            let px = self.px(data, colormap);
            let rgb = self.truecolor.then(|| vox.rgb[k]);
            for x in xa..=xb {
                let xu = x as usize;
                let (t, b) = if clipped {
                    (
                        i32::from(self.cliptop[xu]) + 1,
                        i32::from(self.clipbot[xu]) - 1,
                    )
                } else {
                    (0, self.height - 1)
                };
                for y in ya.max(t)..=yb.min(b) {
                    let i = y as usize * w + xu;
                    if self.x.zbuf[i] > scale {
                        continue;
                    }
                    self.x.zbuf[i] = scale;
                    px.put_rgb(target, i, v[3], rgb);
                    if let (Some(l), true) = (job.dyn_light, self.truecolor) {
                        Self::dyn_add(target, i, Self::texel_rgb(data, v[3], rgb), l);
                    }
                }
            }
        }
    }

    /// Project the frame's particles (after the BSP walk).
    pub(super) fn add_particles(&mut self, data: &SoftData, particles: &[SoftParticle]) {
        let _ = data;
        if particles.is_empty() {
            return;
        }
        let (e, r, fwd, proj, cx) = self.view_basis();
        let cy = f64::from(self.centery);
        for p in particles {
            let d = [p.x - e[0], p.y - e[1], p.z - e[2]];
            let z = d[0] * fwd[0] + d[1] * fwd[1];
            if z < 1.0 || p.alpha == 0 || p.size <= 0.0 {
                continue;
            }
            if self.portal_culled((p.x * UNIT) as i32, (p.y * UNIT) as i32) {
                continue;
            }
            let sx = cx + (d[0] * r[0] + d[1] * r[1]) * proj / z;
            let sy = cy - d[2] * proj / z;
            let half = (p.size / 2.0 * proj / z).max(0.5);
            let x1 = ((sx - half).floor() as i32).max(0);
            let x2 = ((sx + half).ceil() as i32 - 1).min(self.width - 1);
            if x1 > x2 {
                continue;
            }
            let scale = fixed_div(self.projection, (z * UNIT) as i32);
            let lightnum = ((p.light.clamp(0, 255) >> LIGHTSEGSHIFT) + self.extralight)
                .clamp(0, LIGHTLEVELS as i32 - 1) as usize;
            let job = PartJob {
                p: *p,
                sy,
                half,
                dyn_light: if p.light >= 255 {
                    None
                } else {
                    self.dyn_thing([p.x, p.y, p.z])
                },
            };
            let vis = VisSprite {
                x1,
                x2,
                gx: (p.x * UNIT) as i32,
                gy: (p.y * UNIT) as i32,
                gz: ((p.z - p.size / 2.0) * UNIT) as i32,
                gzt: ((p.z + p.size / 2.0) * UNIT) as i32,
                startfrac: 0,
                scale,
                xiscale: FRACUNIT,
                texturemid: 0,
                patch: 0,
                colormap: Some(self.fixed.unwrap_or_else(|| {
                    if p.light >= 255 {
                        0
                    } else {
                        self.scale_light(lightnum, scale)
                    }
                })),
                translation: 0,
                tranmap: None,
                heightsec: None,
                tint: (0, 0),
                yscale: FRACUNIT,
                yiscale: FRACUNIT,
                style: SoftStyle::Normal,
                alpha: 256,
                fill: 0,
                special: None,
                voxel: None,
                particle: Some(self.x.partjobs.len() as u32),
            };
            self.x.partjobs.push(job);
            self.vissprites.push(vis);
        }
    }

    /// Draw a particle's square.
    pub(super) fn draw_particle(
        &mut self,
        data: &SoftData,
        vis: &VisSprite,
        job: PartJob,
        clipped: bool,
        target: &mut SoftTarget,
    ) {
        self.tint = vis.tint;
        let px: Px<'_> = self.px(data, vis.colormap.unwrap_or(0));
        let w = self.width as usize;
        let index = data.nearest_rgb(unpack(job.p.color));
        let (style, a) = if job.p.additive {
            (SoftStyle::Add, u32::from(job.p.alpha) * 256 / 255)
        } else if job.p.alpha < 255 {
            (SoftStyle::Translucent, u32::from(job.p.alpha) * 256 / 255)
        } else {
            (SoftStyle::Normal, 256)
        };
        let ya = ((job.sy - job.half).floor() as i32).max(0);
        let yb = ((job.sy + job.half).ceil() as i32 - 1).min(self.height - 1);
        for x in vis.x1..=vis.x2 {
            let xu = x as usize;
            let (t, b) = if clipped {
                (
                    i32::from(self.cliptop[xu]) + 1,
                    i32::from(self.clipbot[xu]) - 1,
                )
            } else {
                (0, self.height - 1)
            };
            for y in ya.max(t)..=yb.min(b) {
                let i = y as usize * w + xu;
                if self.z_hidden(i, vis.scale) {
                    continue;
                }
                let rgb = self.truecolor.then_some(job.p.color);
                if style == SoftStyle::Normal {
                    px.put_rgb(target, i, index, rgb);
                } else {
                    put_blend(px, data, target, i, index, rgb, style, a, self.truecolor);
                }
                if let (Some(l), true) = (job.dyn_light, self.truecolor) {
                    Self::dyn_add(target, i, unpack(job.p.color), l);
                }
            }
        }
    }
}
