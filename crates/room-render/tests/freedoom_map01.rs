//! Offscreen Freedoom MAP01 smoke (skipped without WAD / GPU).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_map::load_map;
use room_nodebuild::apply_nodes;
use room_render::{
    Camera, DynLightInstance, GpuContext, MaterialCatalog, OffscreenTarget, UnlitPass, build_scene,
    encode_png, read_rgba,
};
use room_vfs::VfsBuilder;
use std::path::PathBuf;
use std::time::Instant;

fn freedoom2() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    p.exists().then_some(p)
}

#[test]
fn freedoom_map01_offscreen() {
    let Some(wad) = freedoom2() else {
        eprintln!(
            "skipping: testdata/freedoom/freedoom2.wad missing (run scripts/fetch-freedoom.sh)"
        );
        return;
    };
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping: no wgpu adapter");
            return;
        }
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").expect("load MAP01");
    if map.game_nodes.is_none() {
        apply_nodes(&mut map);
    }
    let start = map
        .things
        .iter()
        .find(|t| t.type_id == 1)
        .expect("player start");
    let camera = Camera {
        eye: Vec3::new(f32::from(start.x), f32::from(start.y), 41.0),
        forward: Vec3::new(1.0, 0.0, 0.0),
        ..Camera::default()
    };
    let scene = build_scene(&map, &camera, None);
    assert!(
        !scene.opaque_walls.is_empty() || !scene.flats.is_empty(),
        "expected geometry"
    );

    let mut mats = MaterialCatalog::new();
    mats.load_playpal(&vfs);
    mats.load_texturex(&vfs);
    mats.ensure_sampler(&gpu.device);

    let target = OffscreenTarget::new(&gpu, 640, 400);
    let mut pass = UnlitPass::new(&gpu, target.format);
    pass.prepare(&gpu, &camera, &scene);

    let t0 = Instant::now();
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fd") });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fd-rp"),
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
    let elapsed = t0.elapsed();
    eprintln!(
        "freedoom MAP01 offscreen 640x400 in {:?} (walls={} flats={} wall_ranges={})",
        elapsed,
        scene.opaque_walls.len(),
        scene.flats.len(),
        scene.wall_ranges.len()
    );
    let rgba = read_rgba(&gpu, &target).expect("readback");
    let mut png = Vec::new();
    encode_png(640, 400, &rgba, &mut png).unwrap();
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    // Sanity: not a flat clear colour.
    assert!(rgba.iter().any(|&b| b > 40));
    // Floors/walls must tile (Repeat sampler). ClampToEdge smears one texel and
    // collapses spatial variance in the lower half of the frame.
    let mut sum = 0u64;
    let mut sum_sq = 0u64;
    let mut n = 0u64;
    for y in 220..400 {
        for x in 0..640 {
            let i = (y * 640 + x) * 4;
            let lum = u64::from(rgba[i]) + u64::from(rgba[i + 1]) + u64::from(rgba[i + 2]);
            sum += lum;
            sum_sq += lum * lum;
            n += 1;
        }
    }
    let mean = sum as f64 / n as f64;
    let var = (sum_sq as f64 / n as f64) - mean * mean;
    assert!(
        var > 200.0,
        "expected tiled floor/wall detail (luma variance {var:.1}); sampler may be ClampToEdge"
    );
}

/// Multi-frame offscreen pacing smoke (`13` §14 MAP01 ≥120 mid-laptop; CI floor is lower).
#[test]
fn freedoom_map01_offscreen_fps_floor() {
    let Some(wad) = freedoom2() else {
        eprintln!(
            "skipping: testdata/freedoom/freedoom2.wad missing (run scripts/fetch-freedoom.sh)"
        );
        return;
    };
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping: no wgpu adapter");
            return;
        }
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").expect("load MAP01");
    if map.game_nodes.is_none() {
        apply_nodes(&mut map);
    }
    let start = map
        .things
        .iter()
        .find(|t| t.type_id == 1)
        .expect("player start");
    let mut camera = Camera {
        eye: Vec3::new(f32::from(start.x), f32::from(start.y), 41.0),
        forward: Vec3::new(1.0, 0.0, 0.0),
        ..Camera::default()
    };
    let mut mats = MaterialCatalog::new();
    mats.load_playpal(&vfs);
    mats.load_texturex(&vfs);
    mats.ensure_sampler(&gpu.device);
    let target = OffscreenTarget::new(&gpu, 640, 400);
    let mut pass = UnlitPass::new(&gpu, target.format);

    const FRAMES: u32 = 60;
    // Warm up once so shader compile / first upload is excluded.
    {
        let scene = build_scene(&map, &camera, None);
        pass.prepare(&gpu, &camera, &scene);
        render_one(&gpu, &mut pass, &target, &scene, &mut mats, &vfs);
        let _ = gpu.device.poll(wgpu::PollType::Wait);
    }

    let t0 = Instant::now();
    for i in 0..FRAMES {
        let yaw = (i as f32) * 0.05;
        camera.forward = Vec3::new(yaw.cos(), yaw.sin(), 0.0);
        let scene = build_scene(&map, &camera, None);
        pass.prepare(&gpu, &camera, &scene);
        render_one(&gpu, &mut pass, &target, &scene, &mut mats, &vfs);
    }
    let _ = gpu.device.poll(wgpu::PollType::Wait);
    let secs = t0.elapsed().as_secs_f64().max(1e-9);
    let fps = f64::from(FRAMES) / secs;
    eprintln!("freedoom MAP01 offscreen fps ≈ {fps:.1} ({FRAMES} frames / {secs:.3}s)");
    // Soft CI floor (lavapipe/software). Desktop mid-laptop target is ≥120 (`13` §14).
    assert!(
        fps >= 15.0,
        "MAP01 offscreen fps {fps:.1} below CI floor 15 (got {FRAMES} frames in {secs:.3}s)"
    );
}

/// MAP01 + 32 dyn lights + shadow bake path (soft CI floor; stand-in for desktop matrix).
#[test]
fn freedoom_map01_lights_shadow_fps_floor() {
    let Some(wad) = freedoom2() else {
        eprintln!(
            "skipping: testdata/freedoom/freedoom2.wad missing (run scripts/fetch-freedoom.sh)"
        );
        return;
    };
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping: no wgpu adapter");
            return;
        }
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").expect("load MAP01");
    if map.game_nodes.is_none() {
        apply_nodes(&mut map);
    }
    let start = map
        .things
        .iter()
        .find(|t| t.type_id == 1)
        .expect("player start");
    let mut camera = Camera {
        eye: Vec3::new(f32::from(start.x), f32::from(start.y), 41.0),
        forward: Vec3::new(1.0, 0.0, 0.0),
        ..Camera::default()
    };
    let mut mats = MaterialCatalog::new();
    mats.load_playpal(&vfs);
    mats.load_texturex(&vfs);
    mats.ensure_sampler(&gpu.device);
    let target = OffscreenTarget::new(&gpu, 640, 400);
    let mut pass = UnlitPass::new(&gpu, target.format);

    let lights: Vec<DynLightInstance> = (0..32)
        .map(|i| {
            let a = i as f32 * 0.2;
            DynLightInstance::point(
                Vec3::new(
                    f32::from(start.x) + a.sin() * 200.0,
                    f32::from(start.y) + a.cos() * 200.0,
                    40.0,
                ),
                [1.0, 0.9, 0.7],
                128.0,
            )
        })
        .collect();

    const FRAMES: u32 = 30;
    {
        let mut scene = build_scene(&map, &camera, None);
        scene.dyn_lights = lights.clone();
        pass.prepare(&gpu, &camera, &scene);
        render_one(&gpu, &mut pass, &target, &scene, &mut mats, &vfs);
        let _ = gpu.device.poll(wgpu::PollType::Wait);
    }

    let t0 = Instant::now();
    for i in 0..FRAMES {
        let yaw = (i as f32) * 0.05;
        camera.forward = Vec3::new(yaw.cos(), yaw.sin(), 0.0);
        let mut scene = build_scene(&map, &camera, None);
        scene.dyn_lights = lights.clone();
        pass.prepare(&gpu, &camera, &scene);
        render_one(&gpu, &mut pass, &target, &scene, &mut mats, &vfs);
    }
    let _ = gpu.device.poll(wgpu::PollType::Wait);
    let secs = t0.elapsed().as_secs_f64().max(1e-9);
    let fps = f64::from(FRAMES) / secs;
    eprintln!("freedoom MAP01+32 lights offscreen fps ≈ {fps:.1} ({FRAMES} frames / {secs:.3}s)");
    // Soft CI floor under full cluster+shadow bake. Desktop §14 / nuts.wad remain manual.
    assert!(
        fps >= 8.0,
        "MAP01+lights fps {fps:.1} below CI floor 8 (got {FRAMES} frames in {secs:.3}s)"
    );
}

fn render_one(
    gpu: &GpuContext,
    pass: &mut UnlitPass,
    target: &OffscreenTarget,
    scene: &room_render::SceneLists,
    mats: &mut MaterialCatalog,
    vfs: &room_vfs::Vfs,
) {
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("fd-fps"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fd-fps-rp"),
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
        pass.draw_materials(gpu, &mut rp, scene, mats, vfs);
    }
    gpu.queue.submit(Some(encoder.finish()));
}
