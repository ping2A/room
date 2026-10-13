//! E1M1 courtyard step: STEP6 riser must cover the ledge when looking down.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_render::{
    Camera, GpuContext, MaterialCatalog, OffscreenTarget, UnlitPass, build_scene_with_sky_ex_sized,
    encode_png, read_rgba,
};
use room_vfs::VfsBuilder;
use glam::Vec3;
use std::path::PathBuf;

#[test]
fn e1m1_step6_riser_visible_lookdown() {
    let wad = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    if !wad.is_file() {
        return;
    }
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => return,
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);

    // High hex floor, look down at L51 pool edge (STEP6 riser faces the pit).
    let camera = Camera {
        eye: Vec3::new(1056.0, -3040.0, 48.0),
        forward: Vec3::new(0.0, -0.35, -0.9).normalize(),
        ..Camera::default()
    };
    let mut mats = MaterialCatalog::new();
    mats.load_playpal(&vfs);
    mats.load_texturex(&vfs);
    mats.ensure_sampler(&gpu.device);
    let sizes = mats.wall_texel_sizes();
    let scene = build_scene_with_sky_ex_sized(&map, &camera, None, None, None, Some(&sizes));
    assert!(
        scene
            .wall_ranges
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case("STEP6")),
        "STEP6 must be in the scene"
    );

    let target = OffscreenTarget::new(&gpu, 640, 400);
    let mut pass = UnlitPass::new(&gpu, target.format);
    pass.light_mode = room_render::LightMode::DoomSoftware;
    pass.prepare(&gpu, &camera, &scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("step-lookdown"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("step-lookdown-rp"),
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
    if std::env::var_os("ROOM_DUMP_STEP").is_some() {
        let mut png = Vec::new();
        encode_png(640, 400, &rgba, &mut png).unwrap();
        std::fs::write("/tmp/e1m1_step_lookdown.png", png).unwrap();
    }

    // Magenta clear = see-through hole. Step band should be covered.
    let mut mag = 0u32;
    for y in 120..300 {
        for x in 180..460 {
            let i = (y * 640 + x) * 4;
            if rgba[i] > 200 && rgba[i + 2] > 200 && rgba[i + 1] < 40 {
                mag += 1;
            }
        }
    }
    eprintln!("lookdown magenta holes={mag}");
    assert!(
        mag < 40,
        "step ledge must not show clear-color holes (magenta={mag})"
    );
}
