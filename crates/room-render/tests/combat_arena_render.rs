//! Combat arena must emit textured floors (not walls-only / clear-color void).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_nodebuild::apply_nodes;
use room_render::{
    Camera, GpuContext, MaterialCatalog, OffscreenTarget, UnlitPass, build_scene, encode_png,
    read_rgba, walk_bsp,
};
use room_testkit::combat_arena;
use room_vfs::VfsBuilder;
use std::collections::HashSet;
use std::path::PathBuf;

#[test]
fn combat_arena_textured_offscreen() {
    let wad =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    if !wad.is_file() {
        return;
    }
    let Ok(gpu) = pollster::block_on(GpuContext::headless()) else {
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = combat_arena().build_udmf("MAP01").unwrap();
    apply_nodes(&mut map);
    // Look down at the spawn floor so texels are not grazing-angle crushed.
    let camera = Camera {
        eye: Vec3::new(512.0, 96.0, 41.0),
        forward: Vec3::new(0.0, 0.6, -0.8).normalize(),
        ..Camera::default()
    };
    let vis = walk_bsp(&map, 512.0, 96.0);
    assert!(
        !vis.subsectors.is_empty(),
        "expected visible subsectors in arena"
    );
    let scene = build_scene(&map, &camera, None);
    eprintln!(
        "combat arena: walls={} flats={} flat_ranges={} visible_ss={}",
        scene.opaque_walls.len(),
        scene.flats.len(),
        scene.flat_ranges.len(),
        scene.visible_subsectors.len()
    );
    assert!(
        scene.flats.len() >= 48,
        "expected full sector floor/ceil tris, got {}",
        scene.flats.len()
    );
    assert!(
        scene
            .flat_ranges
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case("FLOOR5_1")
                || r.name.eq_ignore_ascii_case("FLOOR4_8")),
        "expected arena floor flats in ranges, got {:?}",
        scene
            .flat_ranges
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>()
    );

    let mut mats = MaterialCatalog::new();
    mats.load_playpal(&vfs);
    mats.load_texturex(&vfs);
    mats.ensure_sampler(&gpu.device);
    let mut fail = Vec::new();
    for r in scene.flat_ranges.iter().chain(scene.wall_ranges.iter()) {
        if mats
            .get_or_upload(&gpu.device, &gpu.queue, &vfs, &r.name)
            .is_none()
        {
            fail.push(r.name.clone());
        }
    }
    assert!(fail.is_empty(), "missing materials: {fail:?}");
    assert!(
        mats.get_or_upload(&gpu.device, &gpu.queue, &vfs, "FLOOR5_1")
            .is_some(),
        "FLOOR5_1 must upload"
    );

    let target = OffscreenTarget::new(&gpu, 640, 400);
    let mut pass = UnlitPass::new(&gpu, target.format);
    pass.prepare(&gpu, &camera, &scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("arena"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("arena"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
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
    let rgba = read_rgba(&gpu, &target).unwrap();
    let mut png = Vec::new();
    encode_png(640, 400, &rgba, &mut png).unwrap();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/arena_diag.png");
    let _ = std::fs::write(&out, &png);

    // Centre of frame while looking down = floor. Must not be clear/sky purple-grey.
    let clear = (13u8, 13u8, 20u8); // ≈ 0.05,0.05,0.08
    let skyish = (63u8, 63u8, 80u8);
    let mut uniq: HashSet<(u8, u8, u8)> = HashSet::new();
    let mut clear_n = 0u32;
    let mut sky_n = 0u32;
    let mut total = 0u32;
    for y in 160..320 {
        for x in 240..400 {
            let i = (y * 640 + x) * 4;
            let c = (rgba[i], rgba[i + 1], rgba[i + 2]);
            uniq.insert(c);
            total += 1;
            if c.0.abs_diff(clear.0) < 8 && c.1.abs_diff(clear.1) < 8 && c.2.abs_diff(clear.2) < 12
            {
                clear_n += 1;
            }
            if c.0.abs_diff(skyish.0) < 10
                && c.1.abs_diff(skyish.1) < 10
                && c.2.abs_diff(skyish.2) < 14
            {
                sky_n += 1;
            }
        }
    }
    eprintln!(
        "wrote {} lookdown uniq={} clear_frac={:.2} sky_frac={:.2}",
        out.display(),
        uniq.len(),
        f64::from(clear_n) / f64::from(total),
        f64::from(sky_n) / f64::from(total),
    );
    assert!(
        uniq.len() >= 8,
        "look-down floor too uniform: {} colors",
        uniq.len()
    );
    assert!(
        f64::from(clear_n) / f64::from(total) < 0.25,
        "look-down still mostly clear color ({clear_n}/{total})"
    );
    assert!(
        f64::from(sky_n) / f64::from(total) < 0.35,
        "look-down still mostly sky void ({sky_n}/{total})"
    );

    // Eye-level: bottom band must show floor (not clear / sky void).
    let camera_eye = Camera {
        eye: Vec3::new(512.0, 96.0, 41.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    let scene_eye = build_scene(&map, &camera_eye, None);
    pass.prepare(&gpu, &camera_eye, &scene_eye);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("arena-eye"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("arena-eye"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
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
        pass.draw_materials(&gpu, &mut rp, &scene_eye, &mut mats, &vfs);
    }
    gpu.queue.submit(Some(encoder.finish()));
    let rgba_eye = read_rgba(&gpu, &target).unwrap();
    let mut png_eye = Vec::new();
    encode_png(640, 400, &rgba_eye, &mut png_eye).unwrap();
    let out_eye = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/arena_eye.png");
    let _ = std::fs::write(&out_eye, &png_eye);

    let mut floor_uniq: HashSet<(u8, u8, u8)> = HashSet::new();
    let mut void_n = 0u32;
    let mut band = 0u32;
    for y in 280..400 {
        for x in 160..480 {
            let i = (y * 640 + x) * 4;
            let c = (rgba_eye[i], rgba_eye[i + 1], rgba_eye[i + 2]);
            floor_uniq.insert(c);
            band += 1;
            let near_clear = c.0.abs_diff(clear.0) < 8
                && c.1.abs_diff(clear.1) < 8
                && c.2.abs_diff(clear.2) < 12;
            let near_sky = c.0.abs_diff(skyish.0) < 10
                && c.1.abs_diff(skyish.1) < 10
                && c.2.abs_diff(skyish.2) < 14;
            if near_clear || near_sky {
                void_n += 1;
            }
        }
    }
    eprintln!(
        "wrote {} eye-level floor-band uniq={} void_frac={:.2}",
        out_eye.display(),
        floor_uniq.len(),
        f64::from(void_n) / f64::from(band),
    );
    assert!(
        floor_uniq.len() >= 6,
        "eye-level floor band too uniform: {} colors",
        floor_uniq.len()
    );
    assert!(
        f64::from(void_n) / f64::from(band) < 0.40,
        "eye-level floor band still mostly void ({void_n}/{band})"
    );
}
