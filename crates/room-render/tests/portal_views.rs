//! Portal views on the GPU: line portals, linked / stacked planes, sky
//! viewpoints and mirrors show the right part of the map, in screen space.
//! Rooms get distinct light levels; the test reads the light the portal
//! surface shows. Skipped when no wgpu adapter is present.
//!
//! `ROOM_PORTAL_PNG=dir` writes the frames as PNG for a look.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_nodebuild::apply_nodes;
use room_render::{
    Camera, GpuContext, MaterialCatalog, OffscreenTarget, PortalRenderer, SceneLists, UnlitPass,
    build_scene, read_rgba,
};
use room_testkit::PolyMap;
use room_vfs::VfsBuilder;

const W: u32 = 160;
const H: u32 = 100;
const NO_SPRITES: &[room_render::SpriteInstance] = &[];

fn load(m: &PolyMap) -> room_map::MapGeometry {
    let mut map = room_map::load_udmf_text("MAP01", &m.textmap()).unwrap();
    apply_nodes(&mut map);
    map
}

/// Draw `map` from `camera` with portal views; returns RGBA.
fn render(
    gpu: &GpuContext,
    map: &room_map::MapGeometry,
    camera: &Camera,
    name: &str,
) -> (Vec<u8>, usize) {
    let vfs = VfsBuilder::new().build().unwrap();
    render_with(gpu, map, camera, name, &vfs, (W, H))
}

fn render_with(
    gpu: &GpuContext,
    map: &room_map::MapGeometry,
    camera: &Camera,
    name: &str,
    vfs: &room_vfs::Vfs,
    (w, h): (u32, u32),
) -> (Vec<u8>, usize) {
    let mut materials = MaterialCatalog::new();
    materials.load_playpal(vfs);
    materials.load_texturex(vfs);
    let target = OffscreenTarget::new(gpu, w, h);
    let mut pass = UnlitPass::new(gpu, target.format);
    let mut scene: SceneLists = build_scene(map, camera, Some(NO_SPRITES));
    let mut portals = PortalRenderer::new();
    let mut build = |c: &Camera, _d: u32, _s: bool| build_scene(map, c, Some(NO_SPRITES));
    portals.render(
        gpu,
        &mut pass,
        &mut materials,
        vfs,
        map,
        camera,
        &mut scene,
        (w, h),
        &mut build,
    );
    pass.prepare(gpu, camera, &scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.draw_materials(gpu, &mut rp, &scene, &mut materials, vfs);
    }
    gpu.queue.submit(Some(encoder.finish()));
    let rgba = read_rgba(gpu, &target).unwrap();
    if let Some(dir) = std::env::var_os("ROOM_PORTAL_PNG") {
        let path = std::path::Path::new(&dir).join(format!("{name}.png"));
        image::save_buffer(&path, &rgba, w, h, image::ColorType::Rgba8).unwrap();
    }
    (rgba, portals.last_views)
}

/// Mean luma of a screen box.
fn luma(rgba: &[u8], x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * W + x) * 4) as usize;
            sum += 0.299 * f64::from(rgba[i])
                + 0.587 * f64::from(rgba[i + 1])
                + 0.114 * f64::from(rgba[i + 2]);
            n += 1.0;
        }
    }
    sum / n
}

fn gpu() -> Option<GpuContext> {
    match pollster::block_on(GpuContext::headless()) {
        Ok(g) => Some(g),
        Err(_) => {
            eprintln!("skipping portal views: no wgpu adapter");
            None
        }
    }
}

fn cam(x: f32, y: f32, z: f32, fwd: Vec3) -> Camera {
    Camera {
        eye: Vec3::new(x, y, z),
        forward: fwd.normalize(),
        aspect: f64::from(W) / f64::from(H),
        ..Camera::default()
    }
}

/// Two rooms of one light level each, joined by a line portal of `kind`.
fn lit_rooms(kind: i32, rotated: bool) -> room_map::MapGeometry {
    let mut m = if rotated {
        room_testkit::rotated_line_portal_rooms(kind)
    } else {
        room_testkit::linked_line_portal_rooms(kind)
    };
    m.sector_fields(0, "lightlevel = 64;");
    m.sector_fields(1, "lightlevel = 64;");
    m.sector_fields(2, "lightlevel = 255;");
    m.sector_fields(3, "lightlevel = 255;");
    load(&m)
}

#[test]
fn a_line_portal_shows_the_room_behind_the_destination() {
    let Some(gpu) = gpu() else {
        return;
    };
    let map = lit_rooms(3, false);
    let (rgba, views) = render(&gpu, &map, &cam(64.0, 128.0, 41.0, Vec3::X), "line_portal");
    assert_eq!(views, 1);
    // The middle of the screen is the portal: it shows bright room B.
    let mid = luma(&rgba, 70, 40, 90, 60);
    // Without the portal the same spot is the dark strip behind A's wall.
    let mut plain = map.clone();
    plain.setup.portals.lines.clear();
    plain
        .setup
        .portals
        .line_portal
        .iter_mut()
        .for_each(|p| *p = None);
    let (rgba2, views2) = render(
        &gpu,
        &plain,
        &cam(64.0, 128.0, 41.0, Vec3::X),
        "line_portal_off",
    );
    assert_eq!(views2, 0);
    let mid2 = luma(&rgba2, 70, 40, 90, 60);
    assert!(mid > mid2 + 20.0, "portal {mid} vs wall {mid2}");
}

#[test]
fn a_rotated_portal_turns_the_view() {
    let Some(gpu) = gpu() else {
        return;
    };
    let map = lit_rooms(2, true);
    let (rgba, views) = render(
        &gpu,
        &map,
        &cam(64.0, 128.0, 41.0, Vec3::X),
        "rotated_portal",
    );
    assert_eq!(views, 1);
    // Room B is bright; through the portal we see it, not the dark strip.
    let mid = luma(&rgba, 70, 40, 90, 60);
    let side = luma(&rgba, 0, 40, 10, 60);
    assert!(mid > side + 20.0, "portal {mid} vs A's wall {side}");
}

#[test]
fn a_linked_ceiling_portal_shows_the_room_above() {
    let Some(gpu) = gpu() else {
        return;
    };
    let mut m = room_testkit::linked_plane_portal_rooms();
    m.sector_fields(0, "lightlevel = 64;");
    m.sector_fields(1, "lightlevel = 255;");
    let map = load(&m);
    // Look straight-ish up from A.
    let (rgba, views) = render(
        &gpu,
        &map,
        &cam(128.0, 128.0, 41.0, Vec3::new(1.0, 0.0, 2.0)),
        "plane_portal",
    );
    assert!(views >= 1);
    let top = luma(&rgba, 60, 5, 100, 30);
    assert!(top > 80.0, "ceiling portal shows lit room B: {top}");
}

#[test]
fn a_sky_viewpoint_replaces_the_sky() {
    let Some(gpu) = gpu() else {
        return;
    };
    let mut m = PolyMap::new();
    let a = m.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 0);
    m.textures(a, "FLOOR0_1", "F_SKY1");
    m.sector_fields(a, "lightlevel = 64;");
    let b = m.rect(2048.0, 2048.0, 2560.0, 2560.0, 0.0, 512.0, 0);
    m.sector_fields(b, "lightlevel = 255;");
    m.thing(9080, 2304.0, 2304.0, 64.0, 0, 0, [0; 5]);
    m.player(128.0, 128.0, 0);
    let map = load(&m);
    let (rgba, views) = render(
        &gpu,
        &map,
        &cam(128.0, 128.0, 41.0, Vec3::new(1.0, 0.0, 1.5)),
        "skybox",
    );
    assert!(views >= 1);
    // The sky area is the lit sky box room, not the clear colour.
    let top = luma(&rgba, 60, 2, 100, 20);
    assert!(top > 80.0, "sky box room shows through the sky: {top}");
}

#[test]
fn a_mirror_reflects_the_room() {
    let Some(gpu) = gpu() else {
        return;
    };
    let mut m = PolyMap::new();
    let a = m.rect(0.0, 0.0, 512.0, 256.0, 0.0, 128.0, 0);
    let b = m.rect(-256.0, 0.0, 0.0, 256.0, 0.0, 128.0, 0);
    m.sector_fields(a, "lightlevel = 255;");
    m.sector_fields(b, "lightlevel = 255;");
    // East wall mirrors; the west part of the room (behind the camera) is
    // what the mirror shows.
    m.line((512.0, 0.0), (512.0, 256.0), 182, [0; 5], 0, Some(a));
    m.player(256.0, 128.0, 0);
    let map = load(&m);
    let (rgba, views) = render(&gpu, &map, &cam(400.0, 128.0, 41.0, Vec3::X), "mirror");
    assert_eq!(views, 1);
    let mid = luma(&rgba, 70, 40, 90, 60);
    assert!(mid > 20.0, "mirror shows the lit room: {mid}");
}

/// Textured look (Freedoom 2 textures): `ROOM_PORTAL_PNG=dir cargo test -p
/// room-render --test portal_views textured -- --ignored`.
#[test]
#[ignore = "visual check; needs testdata/freedoom/freedoom2.wad"]
fn textured() {
    let Some(gpu) = gpu() else {
        return;
    };
    let wad = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/freedoom/freedoom2.wad");
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let wide = |e: Vec3, f: Vec3| Camera {
        aspect: 1.6,
        ..cam(e.x, e.y, e.z, f)
    };
    for (name, map, c) in [
        (
            "tex_linked",
            lit_rooms(3, false),
            wide(Vec3::new(40.0, 60.0, 41.0), Vec3::new(1.0, 0.3, 0.0)),
        ),
        (
            "tex_rotated",
            lit_rooms(2, true),
            wide(Vec3::new(40.0, 60.0, 41.0), Vec3::new(1.0, 0.3, 0.0)),
        ),
    ] {
        let _ = render_with(&gpu, &map, &c, name, &vfs, (640, 400));
    }
}
