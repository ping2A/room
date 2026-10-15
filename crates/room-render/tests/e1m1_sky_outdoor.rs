//! E1M1 outdoor sky: elevated floor undersides must not cover SKY1.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_render::{
    Camera, GpuContext, MaterialCatalog, OffscreenTarget, UnlitPass, build_scene_with_sky_ex,
    is_sky_flat, read_rgba,
};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

fn wad() -> Option<PathBuf> {
    for rel in [
        "../../testdata/freedoom/freedoom1.wad",
        "../../testdata/doom/doom1.wad",
    ] {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn outdoor_cam(map: &room_map::MapGeometry) -> Camera {
    let mut candidates = Vec::new();
    for (si, sec) in map.sectors.iter().enumerate() {
        if !is_sky_flat(&sec.ceiling_pic) {
            continue;
        }
        for line in &map.lines {
            if line.front_sector.is_some_and(|s| s.raw() as usize == si)
                || line.back_sector.is_some_and(|s| s.raw() as usize == si)
            {
                let a = &map.vertices[line.v1.raw() as usize];
                let b = &map.vertices[line.v2.raw() as usize];
                candidates.push((
                    0.5 * (a.pos.x() + b.pos.x()),
                    0.5 * (a.pos.y() + b.pos.y()),
                    sec.floor_height + 41.0,
                ));
            }
        }
    }
    let (cx, cy, eye_z) = candidates
        .into_iter()
        .find(|&(x, y, _)| {
            room_nodebuild::point_in_sector(map, x, y)
                .or_else(|| room_nodebuild::point_in_sector_vanilla(map, x, y))
                .is_some_and(|sid| is_sky_flat(&map.sectors[sid.raw() as usize].ceiling_pic))
        })
        .expect("outdoor F_SKY1 point");
    eprintln!("outdoor cam ({cx:.0},{cy:.0},{eye_z:.0})");
    Camera {
        eye: Vec3::new(cx as f32, cy as f32, eye_z as f32),
        // Steep look-up: elevated floor *tops* leave the frame; undersides must
        // not remain (back-face cull) so SKY1 fills the view.
        forward: Vec3::new(0.0, 0.25, 0.97).normalize(),
        ..Camera::default()
    }
}

#[test]
fn e1m1_outdoor_no_fsky_flats() {
    let Some(wad) = wad() else {
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let camera = outdoor_cam(&map);
    let scene = build_scene_with_sky_ex(&map, &camera, None, Some("SKY1"), None);
    assert_eq!(scene.sky_texture.as_str(), "SKY1");
    let bad: u32 = scene
        .flat_ranges
        .iter()
        .filter(|r| r.name.eq_ignore_ascii_case("F_SKY1") || r.name.eq_ignore_ascii_case("SKY1"))
        .map(|r| r.count)
        .sum();
    assert_eq!(
        bad, 0,
        "sky flats must not be drawn as planes (verts={bad})"
    );
}

#[test]
fn e1m1_outdoor_sky_visible_over_floors() {
    let Some(wad) = wad() else {
        return;
    };
    let Ok(gpu) = pollster::block_on(GpuContext::headless()) else {
        eprintln!("skip: no wgpu");
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let camera = outdoor_cam(&map);
    let scene = build_scene_with_sky_ex(&map, &camera, None, Some("SKY1"), None);

    let mut mats = MaterialCatalog::new();
    mats.load_playpal(&vfs);
    mats.load_texturex(&vfs);
    mats.ensure_sampler(&gpu.device);
    assert!(
        mats.get_or_upload(&gpu.device, &gpu.queue, &vfs, "SKY1")
            .is_some()
    );
    for r in scene
        .flat_ranges
        .iter()
        .chain(scene.wall_ranges.iter())
        .take(64)
    {
        let _ = mats.get_or_upload(&gpu.device, &gpu.queue, &vfs, &r.name);
    }

    let target = OffscreenTarget::new(&gpu, 320, 200);
    let mut pass = UnlitPass::new(&gpu, target.format);
    pass.prepare(&gpu, &camera, &scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("e1m1-sky"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("e1m1-sky"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 0.0,
                            b: 1.0,
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
        pass.draw_materials(&gpu, &mut rp, &scene, &mut mats, &vfs);
    }
    gpu.queue.submit(Some(encoder.finish()));
    let rgba = read_rgba(&gpu, &target).expect("readback");

    // Top band of the frame should be SKY1 clouds (bright grey), not dark green
    // undersides of elevated courtyard floors.
    let mut bright_grey = 0u32;
    let mut greenish = 0u32;
    for y in 5..50 {
        for x in 100..220 {
            let o = ((y * 320 + x) * 4) as usize;
            let r = rgba[o];
            let g = rgba[o + 1];
            let b = rgba[o + 2];
            if g > r.saturating_add(12) && g > b.saturating_add(12) {
                greenish += 1;
            }
            let luma = u16::from(r) + u16::from(g) + u16::from(b);
            if luma > 300 && (r as i16 - g as i16).unsigned_abs() < 40 {
                bright_grey += 1;
            }
        }
    }
    eprintln!("top sky band bright_grey={bright_grey} greenish={greenish}");
    assert!(
        bright_grey > 800,
        "expected SKY1 clouds in upper view (bright_grey={bright_grey} greenish={greenish})"
    );
    assert!(
        greenish < bright_grey / 2,
        "elevated floor undersides still covering sky (greenish={greenish} bright_grey={bright_grey})"
    );
}
