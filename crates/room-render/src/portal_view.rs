//! Portal views on the GPU (`docs/13-renderer.md` §6).
//!
//! UZDoom draws a portal's contents into the stencil-masked portal area
//! (`hw_portal.cpp`). Room draws each visible portal view into its own
//! full-resolution target and the portal surface samples it in screen
//! space (`unlit.wgsl`, vertex style [`crate::scene::PORTAL_STYLE`]). For a
//! portal whose view is a rigid move of the camera (line portals, stacked
//! sectors, sky viewpoints) the two give the same picture; mirrors are drawn
//! from the reflected camera and sampled flipped left-right.
//!
//! * Line portals: the eye goes through `P_TranslatePortalXY` / `Z` and the
//!   view turns by the portal's angle; geometry behind the destination line
//!   (between the moved eye and the portal) is clipped away.
//! * Sector stacks and linked plane portals: the eye moves by the
//!   displacement; the view is clipped to the far side of the plane.
//! * Sky viewpoints: the eye sits at the viewpoint (kept 4 units off its
//!   floor and ceiling) and turns by its angle; skies inside show the plain
//!   sky.
//! * Horizons: the sector's planes drawn out to 32768 units.
//!
//! Views nest up to [`PortalRenderer::max_depth`] deep; deeper portals are
//! left out (the clear colour shows).

use crate::camera::Camera;
use crate::device::GpuContext;
use crate::materials::MaterialCatalog;
use crate::offscreen::OffscreenTarget;
use crate::pass::UnlitPass;
use crate::scene::{MaterialRange, PORTAL_VIEW_PREFIX, PortalView, PortalViewKind, SceneLists};
use glam::Vec3;
use room_map::MapGeometry;
use room_vfs::Vfs;
use std::collections::HashMap;

/// Camera and clip plane for one portal view.
#[derive(Clone, Copy, Debug)]
pub struct PortalCamera {
    /// Eye for the view.
    pub camera: Camera,
    /// World clip plane (`n·p + d > 0` discarded); zero = none.
    pub clip: [f32; 4],
    /// Draw a horizon scene of this sector instead of the map.
    pub horizon: Option<(u32, bool)>,
    /// The view is a sky box: skies inside it are plain skies.
    pub skybox: bool,
}

fn rotate_z(v: Vec3, deg: f64) -> Vec3 {
    if deg == 0.0 {
        return v;
    }
    let (s, c) = deg.to_radians().sin_cos();
    #[allow(clippy::cast_possible_truncation)]
    let (s, c) = (s as f32, c as f32);
    Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
}

fn line_clip_back(map: &MapGeometry, line: room_core::LineId) -> Option<[f32; 4]> {
    let l = map.lines.get(line.raw() as usize)?;
    let v1 = map.vertices.get(l.v1.raw() as usize)?.pos;
    let (dx, dy) = (l.delta.x(), l.delta.y());
    // Back of the line: (p − v1) × d > 0  ⇔  −dy·x + dx·y + (dy·x1 − dx·y1) > 0.
    #[allow(clippy::cast_possible_truncation)]
    Some([
        (-dy) as f32,
        dx as f32,
        0.0,
        (dy * v1.x() - dx * v1.y()) as f32,
    ])
}

fn front_heights(map: &MapGeometry, line: room_core::LineId) -> (f64, f64) {
    map.lines
        .get(line.raw() as usize)
        .and_then(|l| l.front_sector)
        .and_then(|s| map.sectors.get(s.raw() as usize))
        .map_or((0.0, 0.0), |s| (s.floor_height, s.ceiling_height))
}

/// Where a portal view looks from.
#[must_use]
pub fn portal_camera(map: &MapGeometry, view: &PortalView, cam: &Camera) -> Option<PortalCamera> {
    let t = &map.setup.portals;
    match view.kind {
        PortalViewKind::Line { line } => {
            let p = *t.line(room_core::LineId::from_raw(line))?;
            let dst = p.target?;
            let (ex, ey, ez) = (
                f64::from(cam.eye.x),
                f64::from(cam.eye.y),
                f64::from(cam.eye.z),
            );
            let (x, y) = if p.is_linked() {
                (ex + p.displacement.0, ey + p.displacement.1)
            } else {
                p.translate_xy(map, ex, ey)
            };
            let (sf, sc) = front_heights(map, p.line);
            let (df, dc) = front_heights(map, dst);
            let z = p.translate_z(ez, (sf, df), (sc, dc));
            let turn = if p.is_linked() { 0.0 } else { p.angle_diff };
            #[allow(clippy::cast_possible_truncation)]
            let camera = Camera {
                eye: Vec3::new(x as f32, y as f32, z as f32),
                forward: rotate_z(cam.forward, turn),
                up: rotate_z(cam.up, turn),
                ..*cam
            };
            Some(PortalCamera {
                camera,
                clip: line_clip_back(map, dst)?,
                horizon: None,
                skybox: false,
            })
        }
        PortalViewKind::Mirror { line } => {
            let l = map.lines.get(line as usize)?;
            let a = map.vertices.get(l.v1.raw() as usize)?.pos;
            let b = map.vertices.get(l.v2.raw() as usize)?.pos;
            let camera = cam.reflect_across_line(a.x(), a.y(), b.x(), b.y());
            let clip = crate::scene::mirror_clip_plane(
                a.x(),
                a.y(),
                b.x(),
                b.y(),
                f64::from(camera.eye.x),
                f64::from(camera.eye.y),
            );
            Some(PortalCamera {
                camera,
                clip,
                horizon: None,
                skybox: false,
            })
        }
        PortalViewKind::Sector {
            portal,
            plane,
            plane_z,
        } => {
            let p = t.sectors.get(portal as usize)?;
            if p.kind == room_map::portals::PORTS_SKYVIEWPOINT {
                let th = t.things.get(&p.skybox?)?;
                let sec = th.sector.and_then(|s| map.sectors.get(s.raw() as usize));
                let (floor, ceil) = sec.map_or((0.0, 0.0), |s| (s.floor_height, s.ceiling_height));
                let z = (floor + th.z_offset).clamp(floor + 4.0, (ceil - 4.0).max(floor + 4.0));
                #[allow(clippy::cast_possible_truncation)]
                let camera = Camera {
                    eye: Vec3::new(th.x as f32, th.y as f32, z as f32),
                    forward: rotate_z(cam.forward, th.angle),
                    up: rotate_z(cam.up, th.angle),
                    ..*cam
                };
                return Some(PortalCamera {
                    camera,
                    clip: [0.0; 4],
                    horizon: None,
                    skybox: true,
                });
            }
            let camera = cam.shifted_by(p.displacement.0, p.displacement.1);
            #[allow(clippy::cast_possible_truncation)]
            let z = plane_z as f32;
            let clip = if plane == 1 {
                [0.0, 0.0, -1.0, z]
            } else {
                [0.0, 0.0, 1.0, -z]
            };
            Some(PortalCamera {
                camera,
                clip,
                horizon: None,
                skybox: false,
            })
        }
        PortalViewKind::Horizon { sector, relative } => Some(PortalCamera {
            camera: *cam,
            clip: [0.0; 4],
            horizon: Some((sector, relative)),
            skybox: false,
        }),
    }
}

/// Drop portal views nobody can see from `cam`: their surfaces are off
/// screen, wholly on the discarded side of `clip` (the parent portal's
/// clip plane), or seen from behind (line portals and mirrors from their
/// back, ceiling portals from above, floor portals from below). Their
/// surfaces go too.
pub fn cull_portal_views(map: &MapGeometry, scene: &mut SceneLists, cam: &Camera, clip: [f32; 4]) {
    if scene.portal_views.is_empty() {
        return;
    }
    let vp = cam.view_proj();
    let has_clip = clip[0] != 0.0 || clip[1] != 0.0 || clip[2] != 0.0;
    let mut dead: Vec<String> = Vec::new();
    for v in &scene.portal_views {
        // Facing.
        let eye = cam.eye;
        let facing = match v.kind {
            PortalViewKind::Line { line } | PortalViewKind::Mirror { line } => {
                map.lines.get(line as usize).is_none_or(|l| {
                    let v1 = map.vertices[l.v1.raw() as usize].pos;
                    let cross = (f64::from(eye.y) - v1.y()) * l.delta.x()
                        - (f64::from(eye.x) - v1.x()) * l.delta.y();
                    cross <= 1e-6
                })
            }
            PortalViewKind::Sector {
                portal,
                plane,
                plane_z,
            } => {
                portal == 0
                    || if plane == 1 {
                        f64::from(eye.z) < plane_z
                    } else {
                        f64::from(eye.z) > plane_z
                    }
            }
            PortalViewKind::Horizon { .. } => true,
        };
        if !facing {
            dead.push(v.name.clone());
            continue;
        }
        // Surfaces on screen and on the kept side of the clip plane.
        let mut any = false;
        let mut out = [true; 6];
        let mut all_clipped = true;
        let mut check = |p: [f32; 3]| {
            any = true;
            let c = vp * glam::Vec4::new(p[0], p[1], p[2], 1.0);
            let tests = [
                c.x > c.w,
                c.x < -c.w,
                c.y > c.w,
                c.y < -c.w,
                c.w <= 0.0,
                c.z > c.w,
            ];
            for (o, t) in out.iter_mut().zip(tests) {
                *o &= t;
            }
            if !has_clip || clip[0] * p[0] + clip[1] * p[1] + clip[2] * p[2] + clip[3] <= 1e-3 {
                all_clipped = false;
            }
        };
        let mut sky = false;
        for r in scene.wall_ranges.iter().filter(|r| r.name == v.name) {
            for w in scene
                .opaque_walls
                .iter()
                .skip(r.start as usize)
                .take(r.count as usize)
            {
                check(w.pos);
            }
        }
        for r in scene.flat_ranges.iter().filter(|r| r.name == v.name) {
            for w in scene
                .flats
                .iter()
                .skip(r.start as usize)
                .take(r.count as usize)
            {
                check(w.pos);
            }
        }
        if scene.sky_ranges.iter().any(|r| r.name == v.name) {
            sky = true;
        }
        if sky {
            continue;
        }
        if !any || out.iter().any(|&o| o) || all_clipped {
            dead.push(v.name.clone());
        }
    }
    if dead.is_empty() {
        return;
    }
    scene.portal_views.retain(|v| !dead.contains(&v.name));
    scene.wall_ranges.retain(|r| !dead.contains(&r.name));
    scene.flat_ranges.retain(|r| !dead.contains(&r.name));
}

/// Builds the scene seen by a portal camera: `(camera, depth, in_skybox)`.
/// Shells pass their own scene builder (live sprites, sky, wall sizes).
pub type SubSceneBuilder<'a> = dyn FnMut(&Camera, u32, bool) -> SceneLists + 'a;

fn rename_portals(scene: &mut SceneLists, suffix: &str) {
    if scene.portal_views.is_empty() {
        return;
    }
    let names: Vec<String> = scene.portal_views.iter().map(|v| v.name.clone()).collect();
    let fix = |r: &mut MaterialRange| {
        if r.name.starts_with(PORTAL_VIEW_PREFIX) && names.contains(&r.name) {
            r.name.push_str(suffix);
        }
    };
    scene.wall_ranges.iter_mut().for_each(fix);
    scene.flat_ranges.iter_mut().for_each(fix);
    scene.sky_ranges.iter_mut().for_each(fix);
    for v in &mut scene.portal_views {
        v.name.push_str(suffix);
    }
}

/// Leave out portal surfaces (views past the recursion limit, or skies
/// inside a sky box that fall back to the plain sky).
fn strip_portals(scene: &mut SceneLists, keep_sky: bool) {
    let is_pv = |r: &MaterialRange| r.name.starts_with(PORTAL_VIEW_PREFIX);
    scene.wall_ranges.retain(|r| !is_pv(r));
    scene.flat_ranges.retain(|r| !is_pv(r));
    if scene.sky_ranges.iter().any(is_pv) {
        scene.sky_ranges = std::mem::take(&mut scene.plain_sky_ranges);
        if keep_sky {
            for v in &mut scene.sky_parts {
                v._pad = [1.0, -1.0];
            }
        } else {
            scene.sky_parts.clear();
        }
    }
    scene.portal_views.clear();
}

/// Draws a frame's portal views into their materials.
pub struct PortalRenderer {
    targets: HashMap<String, OffscreenTarget>,
    /// Nesting limit (`r_portal_recursions`; UZDoom's default is 4).
    pub max_depth: u32,
    /// Views drawn last frame (for stats / tests).
    pub last_views: usize,
}

impl Default for PortalRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl PortalRenderer {
    /// Renderer with UZDoom's recursion limit.
    #[must_use]
    pub fn new() -> Self {
        Self {
            targets: HashMap::new(),
            max_depth: 4,
            last_views: 0,
        }
    }

    /// Draw every portal view `scene` asks for (and the views those ask
    /// for), so `scene` can then be drawn as usual. `size` is the size of the
    /// target `scene` will be drawn into.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        gpu: &GpuContext,
        pass: &mut UnlitPass,
        materials: &mut MaterialCatalog,
        vfs: &Vfs,
        map: &MapGeometry,
        camera: &Camera,
        scene: &mut SceneLists,
        size: (u32, u32),
        build: &mut SubSceneBuilder<'_>,
    ) {
        self.last_views = 0;
        cull_portal_views(map, scene, camera, [0.0; 4]);
        if scene.portal_views.is_empty() {
            return;
        }
        let saved_clip = pass.clip_plane;
        self.render_level(
            gpu, pass, materials, vfs, map, camera, scene, size, build, 0, false,
        );
        pass.clip_plane = saved_clip;
    }

    #[allow(clippy::too_many_arguments)]
    fn render_level(
        &mut self,
        gpu: &GpuContext,
        pass: &mut UnlitPass,
        materials: &mut MaterialCatalog,
        vfs: &Vfs,
        map: &MapGeometry,
        camera: &Camera,
        scene: &mut SceneLists,
        size: (u32, u32),
        build: &mut SubSceneBuilder<'_>,
        depth: u32,
        in_skybox: bool,
    ) {
        if depth >= self.max_depth || (in_skybox && depth >= 3) {
            strip_portals(scene, true);
            return;
        }
        let views = scene.portal_views.clone();
        for view in &views {
            let Some(pc) = portal_camera(map, view, camera) else {
                continue;
            };
            let skybox = in_skybox || pc.skybox;
            let mut sub = if let Some((sector, relative)) = pc.horizon {
                let e = &camera.eye;
                let mut h = crate::scene::horizon_scene(
                    map,
                    sector,
                    relative,
                    (f64::from(e.x), f64::from(e.y), f64::from(e.z)),
                );
                h.sky_parts = scene.sky_parts.clone();
                h.sky_ranges = scene
                    .plain_sky_ranges
                    .iter()
                    .chain(scene.sky_ranges.iter())
                    .find(|r| !r.name.starts_with(PORTAL_VIEW_PREFIX))
                    .cloned()
                    .into_iter()
                    .collect();
                h.sky_texture = scene.sky_texture.clone();
                for v in &mut h.sky_parts {
                    v._pad = [1.0, -1.0];
                }
                h
            } else {
                build(&pc.camera, depth + 1, skybox)
            };
            if skybox {
                // Inside a sky box, skies are the plain sky.
                if sub
                    .sky_ranges
                    .iter()
                    .any(|r| r.name.starts_with(PORTAL_VIEW_PREFIX))
                {
                    sub.sky_ranges = std::mem::take(&mut sub.plain_sky_ranges);
                    for v in &mut sub.sky_parts {
                        v._pad = [1.0, -1.0];
                    }
                    sub.portal_views
                        .retain(|v| !matches!(v.kind, PortalViewKind::Sector { portal: 0, .. }));
                }
            }
            cull_portal_views(map, &mut sub, &pc.camera, pc.clip);
            rename_portals(&mut sub, &format!("@{}", depth + 1));
            self.render_level(
                gpu,
                pass,
                materials,
                vfs,
                map,
                &pc.camera,
                &mut sub,
                size,
                build,
                depth + 1,
                skybox,
            );
            pass.clip_plane = pc.clip;
            self.draw_view(
                gpu, pass, materials, vfs, &view.name, &pc.camera, &sub, size,
            );
            self.last_views += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_view(
        &mut self,
        gpu: &GpuContext,
        pass: &mut UnlitPass,
        materials: &mut MaterialCatalog,
        vfs: &Vfs,
        name: &str,
        camera: &Camera,
        scene: &SceneLists,
        size: (u32, u32),
    ) {
        let (w, h) = (size.0.max(1), size.1.max(1));
        let fresh = self
            .targets
            .get(name)
            .is_none_or(|t| t.width != w || t.height != h || t.format != pass.format());
        if fresh {
            self.targets.insert(
                name.to_owned(),
                OffscreenTarget::with_format(gpu, w, h, pass.format()),
            );
        }
        let Some(target) = self.targets.get(name) else {
            return;
        };
        pass.prepare(gpu, camera, scene);
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("portal-view"),
            });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("portal-view"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &target.color_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &target.stencil_aux_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Discard,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Discard,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.draw_materials(gpu, &mut rp, scene, materials, vfs);
        }
        gpu.queue.submit(Some(encoder.finish()));
        materials.upsert_target(name, &target.color, &target.color_view, w, h);
    }

    /// Drop targets of views not drawn for a while (call on level change).
    pub fn clear(&mut self) {
        self.targets.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn cam_at(x: f32, y: f32, z: f32) -> Camera {
        Camera {
            eye: Vec3::new(x, y, z),
            forward: Vec3::X,
            ..Camera::default()
        }
    }

    #[test]
    fn line_portal_camera_follows_the_portal() {
        let map = room_map::load_udmf_text(
            "MAP01",
            &room_testkit::rotated_line_portal_rooms(2).textmap(),
        )
        .unwrap();
        let line = map
            .setup
            .portals
            .lines
            .iter()
            .find(|p| p.target.is_some())
            .unwrap()
            .line;
        let view = PortalView {
            name: "__PVL".into(),
            kind: PortalViewKind::Line { line: line.raw() },
        };
        let pc = portal_camera(&map, &view, &cam_at(128.0, 128.0, 41.0)).unwrap();
        let e = pc.camera.eye;
        assert!(
            (e.x - 1152.0).abs() < 1e-3 && (e.y - 896.0).abs() < 1e-3,
            "{e:?}"
        );
        // Looking east in A is looking north in B.
        assert!(pc.camera.forward.x.abs() < 1e-6 && (pc.camera.forward.y - 1.0).abs() < 1e-6);
        // The clip plane removes what lies south of B's portal line (behind it).
        let c = pc.clip;
        let s = |x: f32, y: f32| c[0] * x + c[1] * y + c[2] * 0.0 + c[3];
        assert!(s(1152.0, 1000.0) > 0.0);
        assert!(s(1152.0, 1100.0) < 0.0);
    }

    #[test]
    fn stack_camera_moves_and_clips_by_height() {
        let map = room_map::load_udmf_text(
            "MAP01",
            &room_testkit::linked_plane_portal_rooms().textmap(),
        )
        .unwrap();
        let idx = map.setup.portals.plane_portal_index(0, 1);
        let view = PortalView {
            name: "__PVS".into(),
            kind: PortalViewKind::Sector {
                portal: idx,
                plane: 1,
                plane_z: 128.0,
            },
        };
        let pc = portal_camera(&map, &view, &cam_at(128.0, 128.0, 41.0)).unwrap();
        assert_eq!(pc.camera.eye, Vec3::new(1152.0, 128.0, 41.0));
        let c = pc.clip;
        // Below the plane is discarded, above kept.
        assert!(c[2] * 100.0 + c[3] > 0.0);
        assert!(c[2] * 200.0 + c[3] < 0.0);
    }

    #[test]
    fn rename_and_strip() {
        let mut s = SceneLists::default();
        let n = crate::scene::request_portal_view(&mut s, PortalViewKind::Mirror { line: 3 });
        s.wall_ranges.push(MaterialRange {
            name: n.clone(),
            start: 0,
            count: 6,
            flags: 0,
        });
        rename_portals(&mut s, "@1");
        assert_eq!(s.wall_ranges[0].name, format!("{n}@1"));
        assert_eq!(s.portal_views[0].name, format!("{n}@1"));
        strip_portals(&mut s, true);
        assert!(s.wall_ranges.is_empty() && s.portal_views.is_empty());
    }
}
