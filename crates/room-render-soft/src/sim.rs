//! The shells' bridge from the playsim to the software renderer (`docs/40`
//! SR3, feature `sim`): sector heights, light and pictures, and every actor
//! the GPU path would draw as a sprite, read from a [`room_sim::World`]
//! into a [`SoftFrame`]. The playsim is only read.
//!
//! The weapon is left to the shells' HUD (it draws over either renderer),
//! so [`SoftFrame::psprites`] stays empty here.

use crate::level::{SoftSector, flat_id};
use crate::render::{
    FF_FULLBRIGHT, MF_SHADOW, MF_TRANSSHIFT, SoftDecal, SoftFrame, SoftLight, SoftParticle,
    SoftSpriteKind, SoftStyle, SoftThing, SoftVoxelPose, sprite_name,
};
use crate::{SoftTarget, SoftView};
use room_map::MapGeometry;
use room_sim::{ActorId, RenderStyle, World};
use room_vfs::Vfs;

/// `FLAG_DONTDRAW` in `room-sim`'s `MF_*` subset.
const FLAG_DONTDRAW: u64 = 1 << 29;
use room_sim::{FLAG_FLATSPRITE, FLAG_WALLSPRITE};

/// Where the software view is drawn from.
#[derive(Clone, Debug, Default)]
pub struct SoftCamera {
    /// Eye position (map units).
    pub x: f64,
    /// Eye position.
    pub y: f64,
    /// Eye height.
    pub z: f64,
    /// Facing, degrees (0 = east, counter-clockwise).
    pub angle_deg: f64,
    /// `extralight` (gun flashes).
    pub extralight: i32,
    /// `player->fixedcolormap` (0 = none; 32 invulnerability, 1 light amp).
    pub fixed_colormap: u8,
    /// The viewer's own body, not drawn.
    pub hide: Option<ActorId>,
    /// Sky texture name (`SKY1`).
    pub sky: String,
    /// Draw sector planes between the previous and current tic (`0..=1`);
    /// `None` = the current tic, as vanilla.
    pub plane_lerp: Option<f64>,
    /// Looking up (+) / down (−), degrees: free look by shearing (SR5).
    pub pitch_deg: f64,
}

/// A [`SoftView`] kept for one map and view size, fed from the playsim.
#[derive(Debug)]
pub struct SoftWorldView {
    /// The renderer and its data.
    pub view: SoftView,
    /// Picture names last resolved per sector (floor, ceiling).
    sector_pics: Vec<(String, String)>,
    /// Texture names last resolved per side (upper, lower, middle).
    side_names: Vec<[String; 3]>,
    sky: (String, u16),
    /// Hexen sky2 name and texture (looked up when it changes).
    sky2: (String, u16),
    /// The frame drawn into, reused (SR9).
    target: SoftTarget,
    /// Per class: `RenderRequired`, `RenderHidden` (looked up once).
    class_render: Vec<Option<(i32, i32)>>,
    /// Flag indices: ABSVIEWANGLES, XFLIP, YFLIP, ROLLSPRITE, ROLLCENTER.
    flag_index: [Option<u16>; 5],
    frame: SoftFrame,
    /// Field of view of the current renderer (`None` = vanilla tables).
    fov: Option<f64>,
    /// Decals to draw (the shell keeps them; SR6).
    pub decals: Vec<SoftDecal>,
    /// Camera texture canvases (SR7): name, texture id and a renderer of
    /// the canvas size.
    canvases: Vec<(String, u16, crate::SoftRenderer)>,
    /// VOXELDEF: sprite frames drawn as voxel models (SR8).
    voxeldefs: room_defs::VoxelDefs,
    /// Particles and dynamic lights to draw (the shell keeps them; SR8).
    pub particles: Vec<SoftParticle>,
    /// See [`Self::particles`]; lights only show in true colour.
    pub lights: Vec<SoftLight>,
}

/// What the renderer needs of the playsim's sprite setup.
#[derive(Clone, Copy, Debug)]
struct SpriteSetup {
    flip_x: bool,
    flip_y: bool,
    roll_rad: f64,
    roll_center: bool,
}

/// A sector colour as [`SoftSector`] keeps it (white light / black fade = 0).
fn color_of(c: [u8; 3], light: bool) -> u32 {
    let v = (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
    if light && v == 0xff_ffff { 0 } else { v }
}

fn fixed(v: f64) -> i32 {
    (v * 65536.0).round() as i32
}

fn bam(deg: f64) -> u32 {
    ((deg.rem_euclid(360.0) / 360.0) * 4_294_967_296.0) as u64 as u32
}

impl SoftWorldView {
    /// Prepare `map` (with game nodes) for a `width` × `height` view;
    /// `None` when the map has no BSP.
    pub fn new(vfs: &Vfs, map: &MapGeometry, width: u32, height: u32) -> Option<Self> {
        let mut view = SoftView::new(vfs, map, width, height)?;
        // Translucent actors (Boom / MBF) draw through the table.
        view.data.ensure_tranmap(vfs);
        Some(Self {
            view,
            sector_pics: map
                .sectors
                .iter()
                .map(|s| (s.floor_pic.clone(), s.ceiling_pic.clone()))
                .collect(),
            side_names: map
                .sides
                .iter()
                .map(|s| [s.upper.clone(), s.lower.clone(), s.middle.clone()])
                .collect(),
            sky: (String::new(), 0),
            sky2: (String::new(), 0),
            target: SoftTarget::new(1, 1),
            class_render: Vec::new(),
            flag_index: [
                "ABSVIEWANGLES",
                "XFLIP",
                "YFLIP",
                "ROLLSPRITE",
                "ROLLCENTER",
            ]
            .map(room_sim::ActorFlags::index_of),
            decals: Vec::new(),
            canvases: Vec::new(),
            voxeldefs: room_defs::VoxelDefs::default(),
            particles: Vec::new(),
            lights: Vec::new(),
            frame: SoftFrame::default(),
            fov: None,
        })
    }

    /// Use a modern view (SR5): `width` × `height` with vanilla-shaped
    /// pixels (wider than 1.6 × `height` is widescreen), field of view `fov`
    /// degrees across the 4:3 part, true colour or 8-bit. Rebuilds the
    /// view tables only when something changed.
    pub fn set_view(&mut self, width: u32, height: u32, fov: f64, truecolor: bool) {
        let r = &self.view.renderer;
        if r.width() != width || r.height() != height || self.fov != Some(fov) {
            let (boom, limits, vis) = (r.boom, r.vanilla_limits, r.visibility);
            let mut renderer = crate::SoftRenderer::new_view(width, height, fov);
            renderer.boom = boom;
            renderer.vanilla_limits = limits;
            renderer.visibility = vis;
            self.view.renderer = renderer;
            self.fov = Some(fov);
        }
        self.view.renderer.truecolor = truecolor;
    }

    /// `room_sim::sprite_view` for the parts the software renderer uses
    /// (`RenderRequired` / `RenderHidden`, `VisibleAngles` /
    /// `VisiblePitch`, flips, roll), with flag indices and class values
    /// looked up once (that function allocates per call).
    fn sprite_setup(
        &mut self,
        world: &World,
        a: &room_sim::Actor,
        eye: room_core::DVec3,
    ) -> Option<SpriteSetup> {
        let ci = a.class.raw() as usize;
        if self.class_render.len() <= ci {
            self.class_render.resize(ci + 1, None);
        }
        let (required, hidden) = match self.class_render[ci] {
            Some(v) => v,
            None => {
                let class = world.classes.get(a.class)?;
                let v = (
                    class.ext.stored_i32("renderrequired").unwrap_or(0),
                    class.ext.stored_i32("renderhidden").unwrap_or(0),
                );
                self.class_render[ci] = Some(v);
                v
            }
        };
        // The normal player view has no view flags.
        if required != 0 {
            return None;
        }
        let _ = hidden;
        let f = &self.flag_index;
        let has = |i: Option<u16>| i.is_some_and(|i| a.extra_flags.get(i));
        let wrap = |mut d: f64| {
            while d > 180.0 {
                d -= 360.0;
            }
            while d < -180.0 {
                d += 360.0;
            }
            d
        };
        let in_range = |v: f64, (s, e): (f64, f64)| {
            let (s, e) = (wrap(s), wrap(e));
            if s <= e {
                v >= s && v <= e
            } else {
                v >= s || v <= e
            }
        };
        let abs = has(f[0]);
        let (dx, dy) = (eye.x() - a.pos.x(), eye.y() - a.pos.y());
        if let Some(range) = a.ext.visible_angles {
            let to_viewer = room_core::dmath::atan2(dy, dx).to_degrees();
            let rel = if abs {
                wrap(to_viewer)
            } else {
                wrap(to_viewer - a.angle.degrees())
            };
            if !in_range(rel, range) {
                return None;
            }
        }
        if let Some(range) = a.ext.visible_pitch {
            let dist = (dx * dx + dy * dy).sqrt();
            let mid = a.pos.z() + a.height * 0.5;
            let pitch = -room_core::dmath::atan2(eye.z() - mid, dist).to_degrees();
            let rel = if abs {
                pitch
            } else {
                pitch + a.pitch.degrees()
            };
            if !in_range(rel, range) {
                return None;
            }
        }
        let rollsprite = has(f[3]);
        Some(SpriteSetup {
            flip_x: has(f[1]),
            flip_y: has(f[2]),
            roll_rad: if rollsprite {
                a.roll.degrees().to_radians()
            } else {
                0.0
            },
            roll_center: has(f[4]),
        })
    }

    /// VOXELDEF: sprite frames it names are drawn as voxel models (SR8).
    pub fn set_voxeldefs(&mut self, defs: room_defs::VoxelDefs) {
        self.voxeldefs = defs;
    }

    /// Camera textures (`ANIMDEFS cameratexture name width height`): the
    /// canvases ACS `SetCameraToTexture` paints. Call after [`Self::new`].
    pub fn set_camera_canvases(&mut self, canvases: impl IntoIterator<Item = (String, u32, u32)>) {
        self.canvases.clear();
        for (name, w, h) in canvases {
            let id = self.view.data.canvas(&name, w, h);
            let renderer = crate::SoftRenderer::new_view(w.max(2), h.max(2), 90.0);
            self.canvases
                .push((name.to_ascii_uppercase(), id, renderer));
        }
        // Walls already naming a canvas.
        for (i, names) in self.side_names.iter().enumerate() {
            for (slot, name) in names.iter().enumerate() {
                let up = name.trim().to_ascii_uppercase();
                if let Some((_, id, _)) = self.canvases.iter().find(|c| c.0 == up) {
                    let side = &mut self.view.level.sides[i];
                    match slot {
                        0 => side.top = *id,
                        1 => side.bottom = *id,
                        _ => side.mid = *id,
                    }
                }
            }
        }
    }

    /// Paint the canvases bound to cameras (`SetCameraToTexture`) with
    /// their camera's view of this frame.
    fn paint_canvases(&mut self, world: &World) {
        if self.canvases.is_empty() || world.camera_textures.is_empty() {
            return;
        }
        let main = &self.view.renderer;
        let (truecolor, boom, vis) = (main.truecolor, main.boom, main.visibility);
        for (name, bind) in &world.camera_textures {
            let up = name.to_ascii_uppercase();
            let Some(c) = self.canvases.iter_mut().find(|c| c.0 == up) else {
                continue;
            };
            let Some(cam) = world.actors.get(bind.camera) else {
                continue;
            };
            let (w, h) = (c.2.width(), c.2.height());
            let fov = bind.fov.clamp(30.0, 120.0);
            if (c.2.projection_rows() - crate::SoftRenderer::new_view(w, h, fov).projection_rows())
                .abs()
                > 1e-6
            {
                c.2 = crate::SoftRenderer::new_view(w, h, fov);
            }
            c.2.truecolor = truecolor;
            c.2.boom = boom;
            c.2.visibility = vis;
            let pitch = cam.pitch.degrees().clamp(-89.0, 89.0);
            let frame = SoftFrame {
                x: fixed(cam.pos.x()),
                y: fixed(cam.pos.y()),
                z: fixed(cam.pos.z() + cam.height / 2.0),
                angle: bam(cam.angle.degrees()),
                psprites: Vec::new(),
                // Doom pitch is positive looking down; the shear is up.
                yshear: (-pitch.to_radians().tan() * c.2.projection_rows()).round() as i32,
                ..self.frame.clone()
            };
            let mut target = SoftTarget::new(w, h);
            c.2.render(&self.view.level, &self.view.data, &frame, &mut target);
            let id = c.1;
            self.view.data.set_canvas(id, &target);
        }
    }

    /// View size.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.view.renderer.width(), self.view.renderer.height())
    }

    /// Bring sector and side pictures in line with `map` (switches, texture
    /// changes); only names that changed are looked up again.
    fn sync_pictures(&mut self, vfs: &Vfs, map: &MapGeometry) {
        for (i, s) in map.sectors.iter().enumerate() {
            let Some(cached) = self.sector_pics.get_mut(i) else {
                break;
            };
            if cached.0 != s.floor_pic {
                cached.0.clone_from(&s.floor_pic);
                let id = flat_id(&mut self.view.data, vfs, &s.floor_pic);
                self.view.level.sectors[i].floor_pic = id;
            }
            if cached.1 != s.ceiling_pic {
                cached.1.clone_from(&s.ceiling_pic);
                let id = flat_id(&mut self.view.data, vfs, &s.ceiling_pic);
                self.view.level.sectors[i].ceiling_pic = id;
            }
        }
        for (i, s) in map.sides.iter().enumerate() {
            // Scrolling walls: offsets every frame.
            if let Some(side) = self.view.level.sides.get_mut(i) {
                side.texture_offset = i32::from(s.x_offset) << 16;
                side.row_offset = i32::from(s.y_offset) << 16;
            }
            let Some(cached) = self.side_names.get_mut(i) else {
                break;
            };
            let names = [&s.upper, &s.lower, &s.middle];
            for (slot, name) in names.into_iter().enumerate() {
                if cached[slot] != *name {
                    cached[slot].clone_from(name);
                    let id = self.view.data.texture(vfs, name);
                    let side = &mut self.view.level.sides[i];
                    match slot {
                        0 => side.top = id,
                        1 => side.bottom = id,
                        _ => side.mid = id,
                    }
                }
            }
        }
    }

    /// Build the frame for `world` as seen from `cam`. `map` is the shell's
    /// render copy of the map (switch textures applied).
    pub fn frame(
        &mut self,
        vfs: &Vfs,
        world: &World,
        map: &MapGeometry,
        cam: &SoftCamera,
    ) -> &SoftFrame {
        self.sync_pictures(vfs, map);
        // Portals changed at run time (`Line_SetPortalTarget`, SR7).
        if map.setup.portals.revision != self.view.level.portal_revision {
            self.view.level.load_portals(map);
        }
        // Polyobjects where the playsim moved them.
        if !map.setup.polyobjects.is_empty() {
            self.view.level.place_polyobjects(map);
        }
        if self.sky.0 != cam.sky {
            self.sky = (cam.sky.clone(), self.view.data.texture(vfs, &cam.sky));
        }
        let level = &self.view.level;
        let lerp = |prev: f64, cur: f64| match cam.plane_lerp {
            Some(t) => prev + (cur - prev) * t.clamp(0.0, 1.0),
            None => cur,
        };
        let mut sectors = std::mem::take(&mut self.frame.sectors).unwrap_or_default();
        sectors.clear();
        sectors.extend(level.sectors.iter().enumerate().map(|(i, base)| {
            // Flat panning (Boom scrollers) as the render map has it.
            let mut base = *base;
            if let Some(m) = map.sectors.get(i) {
                base.floor_xoffs = fixed(m.floor_u_off);
                base.floor_yoffs = fixed(m.floor_v_off);
                base.ceiling_xoffs = fixed(m.ceiling_u_off);
                base.ceiling_yoffs = fixed(m.ceiling_v_off);
            }
            // Slopes keep their shape and move with the sector's height.
            let plane_base = map.setup.planes.get(i);
            world.sectors.get(i).map_or(base, |d| SoftSector {
                floor_slope: base.floor_slope.map(|mut p| {
                    let b = plane_base.map_or(0.0, |pb| pb.base_floor);
                    p.z0 += lerp(d.prev_floor_z, d.floor_z) - b;
                    p
                }),
                ceiling_slope: base.ceiling_slope.map(|mut p| {
                    let b = plane_base.map_or(0.0, |pb| pb.base_ceiling);
                    p.z0 += lerp(d.prev_ceiling_z, d.ceiling_z) - b;
                    p
                }),
                floor: fixed(lerp(d.prev_floor_z, d.floor_z)),
                ceiling: fixed(lerp(d.prev_ceiling_z, d.ceiling_z)),
                light: i32::from(d.light).clamp(0, 255),
                color: color_of(d.color, true),
                fade: color_of(d.fade, false),
                desaturation: d.color_desaturate,
                ..base
            })
        }));
        // Hexen double sky (ACS `ChangeSky`'s sky2).
        let sky2 = match world.sky2_override.as_deref() {
            Some(name) if !name.is_empty() && !name.eq_ignore_ascii_case("-") => {
                if self.sky2.0 != name {
                    self.sky2 = (name.to_owned(), self.view.data.texture(vfs, name));
                }
                Some(self.sky2.1).filter(|&t| t != 0)
            }
            _ => None,
        };
        let mut things = std::mem::take(&mut self.frame.things);
        things.clear();
        let eye = room_core::DVec3::new(cam.x, cam.y, cam.z);
        for &id in &world.thinkers.default {
            if Some(id) == cam.hide {
                continue;
            }
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            let class_flags = world.classes.get(a.class).map_or(0, |c| c.flags);
            if a.destroyed
                || a.sprite.is_empty()
                || (a.flags | class_flags) & FLAG_DONTDRAW != 0
                || matches!(a.render_style, RenderStyle::None)
            {
                continue;
            }
            // The playsim's sprite setup (visibility, flips, roll), without
            // allocating (SR9).
            let Some(sv) = self.sprite_setup(world, a, eye) else {
                continue;
            };
            let sv = Some(sv);
            let Some(sector) = room_sim::sector_at_xy(world, a.pos.x(), a.pos.y()) else {
                continue;
            };
            let bright = a
                .state
                .and_then(|sid| world.states.get(sid))
                .is_some_and(|st| st.bright);
            let mut flags = 0;
            // Render styles (SR6): fuzz stays vanilla's; the others blend.
            let alpha = (a.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
            let style = match a.render_style {
                RenderStyle::Fuzzy | RenderStyle::OptFuzzy => {
                    flags |= MF_SHADOW;
                    SoftStyle::Normal
                }
                RenderStyle::Add => SoftStyle::Add,
                RenderStyle::Subtract => SoftStyle::Subtract,
                RenderStyle::Stencil | RenderStyle::TranslucentStencil | RenderStyle::Shadow => {
                    SoftStyle::Stencil
                }
                RenderStyle::AddStencil => SoftStyle::AddStencil,
                RenderStyle::Shaded => SoftStyle::Shaded,
                RenderStyle::AddShaded => SoftStyle::AddShaded,
                RenderStyle::Translucent
                | RenderStyle::SoulTrans
                | RenderStyle::ColorBlend
                | RenderStyle::ColorAdd
                | RenderStyle::Multiply => SoftStyle::Translucent,
                _ if alpha < 252 => SoftStyle::Translucent,
                _ => SoftStyle::Normal,
            };
            let fill = match (a.render_style, a.fill_color) {
                (RenderStyle::Shadow, _) => 0,
                (_, Some([r, g, b])) => (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
                _ => 0,
            };
            let xflip = sv.is_some_and(|v| v.flip_x);
            // Wall / flat sprites and roll (SR6).
            let kind = if (a.flags | class_flags) & FLAG_WALLSPRITE != 0 {
                SoftSpriteKind::Wall
            } else if (a.flags | class_flags) & FLAG_FLATSPRITE != 0 {
                SoftSpriteKind::Flat
            } else {
                SoftSpriteKind::Billboard
            };
            let roll = sv.map_or(0, |v| bam(v.roll_rad.to_degrees()));
            let roll_center = sv.is_some_and(|v| v.roll_center);
            let yflip = sv.is_some_and(|v| v.flip_y);
            // VOXELDEF (SR8): the hardware renderer's pose.
            let voxel = (!self.voxeldefs.by_name.is_empty())
                .then(|| self.voxeldefs.for_sprite(&a.sprite, a.frame))
                .flatten()
                .cloned()
                .and_then(|def| {
                    let id = self.view.data.voxel(vfs, &def.file, def.override_palette)?;
                    let flags = a.flags | class_flags;
                    let spin_n = if flags & room_sim::FLAG_DROPPED != 0 {
                        def.dropped_spin
                    } else {
                        def.placed_spin
                    };
                    let yaw = a.angle.degrees() as f32;
                    let (pitch, roll) = room_render::voxel_view_angles(
                        yaw,
                        a.pitch.degrees() as f32,
                        a.roll.degrees() as f32,
                        [a.vel.x() as f32, a.vel.y() as f32, a.vel.z() as f32],
                        def.pitch_from_momentum,
                        def.use_actor_pitch,
                        def.use_actor_roll,
                    );
                    let spin = room_render::voxel_spin_degrees(spin_n, world.level.maptime as f32);
                    let vox = if def.scale.abs() < 1e-6 {
                        1.0
                    } else {
                        f64::from(def.scale)
                    };
                    let (ax, ay) = (a.scale_x, a.scale_y);
                    Some(SoftVoxelPose {
                        id,
                        angle_offset: bam(f64::from(def.angle_offset)),
                        spin: bam(f64::from(spin)),
                        pitch: bam(f64::from(pitch)),
                        roll: bam(f64::from(roll)),
                        scale: [fixed(ax * vox), fixed(ax * vox), fixed(ay * vox)],
                        offset: def.offset.map(|o| fixed(f64::from(o) / vox)),
                    })
                });
            let scale = |v: f64| {
                if (v - 1.0).abs() < 1e-9 || !v.is_finite() || v <= 0.0 {
                    0
                } else {
                    fixed(v)
                }
            };
            // Player pawns wear their seat's vanilla colour (seats 1–3).
            if let Some(seat) = world
                .players
                .iter()
                .position(|p| p.as_ref().and_then(|p| p.mo) == Some(id))
                && (1..=3).contains(&seat)
            {
                flags |= (seat as u32) << MF_TRANSSHIFT;
            }
            things.push(SoftThing {
                x: fixed(a.pos.x()),
                y: fixed(a.pos.y()),
                z: fixed(a.pos.z()),
                angle: bam(a.angle.degrees()),
                sprite: sprite_name(&a.sprite),
                frame: u32::from(a.frame) | if bright { FF_FULLBRIGHT } else { 0 },
                flags,
                sector: sector as u32,
                style,
                alpha,
                fill,
                scale_x: scale(a.scale_x),
                scale_y: scale(a.scale_y),
                xflip,
                kind,
                roll,
                roll_center,
                yflip,
                voxel,
            });
        }
        // Reuse last frame's lists (SR9: no allocation per frame).
        let mut decals = std::mem::take(&mut self.frame.decals);
        decals.clone_from(&self.decals);
        let mut particles = std::mem::take(&mut self.frame.particles);
        particles.clear();
        particles.extend_from_slice(&self.particles);
        let mut lights = std::mem::take(&mut self.frame.lights);
        lights.clear();
        lights.extend_from_slice(&self.lights);
        let mut psprites = std::mem::take(&mut self.frame.psprites);
        psprites.clear();
        self.frame = SoftFrame {
            x: fixed(cam.x),
            y: fixed(cam.y),
            z: fixed(cam.z),
            angle: bam(cam.angle_deg),
            extralight: cam.extralight.clamp(0, 15),
            fixed_colormap: (cam.fixed_colormap != 0).then_some(usize::from(cam.fixed_colormap)),
            sectors: Some(sectors),
            sky: self.sky.1,
            things,
            psprites,
            psprite_fuzz: false,
            leveltime: world.level.maptime,
            decals,
            particles,
            lights,
            sky2,
            sky_offsets: [world.sky_speed1, world.sky_speed2].map(|speed| {
                // Columns scrolled (1024 per turn).
                let cols = speed * world.level.maptime as f64;
                ((cols.rem_euclid(1024.0) / 1024.0) * 4_294_967_296.0) as u64 as u32
            }),
            yshear: (cam.pitch_deg.clamp(-89.0, 89.0).to_radians().tan()
                * self.view.renderer.projection_rows())
            .round() as i32,
        };
        &self.frame
    }

    /// [`Self::frame`], then draw it (sprites loaded on first use).
    pub fn draw(
        &mut self,
        vfs: &Vfs,
        world: &World,
        map: &MapGeometry,
        cam: &SoftCamera,
    ) -> SoftTarget {
        self.render_frame(vfs, world, map, cam);
        self.target.clone()
    }

    /// Build and draw the frame into the reused target.
    fn render_frame(&mut self, vfs: &Vfs, world: &World, map: &MapGeometry, cam: &SoftCamera) {
        self.frame(vfs, world, map, cam);
        let frame = std::mem::take(&mut self.frame);
        self.view.prepare(vfs, &frame);
        self.frame = frame;
        self.paint_canvases(world);
        let (w, h) = (self.view.renderer.width(), self.view.renderer.height());
        if self.target.width != w || self.target.height != h {
            self.target = SoftTarget::new(w, h);
        }
        let frame = std::mem::take(&mut self.frame);
        self.view
            .renderer
            .render(&self.view.level, &self.view.data, &frame, &mut self.target);
        self.frame = frame;
    }

    /// [`Self::draw`] scaled into `out_w` × `out_h` RGBA8 (vanilla pixel
    /// shape, black bars) in `out`, reusing every buffer (SR9: what the
    /// shells call each frame).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_fit(
        &mut self,
        vfs: &Vfs,
        world: &World,
        map: &MapGeometry,
        cam: &SoftCamera,
        out_w: u32,
        out_h: u32,
        out: &mut Vec<u8>,
    ) {
        self.render_frame(vfs, world, map, cam);
        self.target
            .to_rgba_fit_into(&self.view.data.palette, out_w, out_h, out);
    }

    /// [`Self::draw`] as RGBA8 through the palette.
    pub fn draw_rgba(
        &mut self,
        vfs: &Vfs,
        world: &World,
        map: &MapGeometry,
        cam: &SoftCamera,
    ) -> Vec<u8> {
        let target = self.draw(vfs, world, map, cam);
        target.to_rgba(&self.view.data.palette)
    }
}
