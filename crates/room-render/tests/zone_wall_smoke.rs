//! BE1 mod zone wall offscreen smoke (skipped when no GPU).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_nodebuild::apply_nodes;
use room_render::{
    Camera, GpuContext, ModZonePass, OffscreenTarget, UnlitPass, ZoneDraw, build_scene,
    build_zone_meshes, read_rgba,
};
use room_testkit::{MapBuilder, rect};

#[test]
fn zone_wall_offscreen_smoke() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping zone_wall_offscreen_smoke: no wgpu adapter");
            return;
        }
    };
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
        .player_start(256.0, 256.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let camera = Camera {
        eye: Vec3::new(256.0, 128.0, 41.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    let scene = build_scene(&map, &camera, None);
    let zones = [ZoneDraw {
        cx: 256.0,
        cy: 256.0,
        r: 100.0,
        rgba: 0x00ccffff,
        flags: 0,
    }];
    let zone_meshes = build_zone_meshes(&map, &zones);
    assert!(!zone_meshes.wall_depth.is_empty());

    let target = OffscreenTarget::new(&gpu, 320, 200);
    let mut pass = UnlitPass::new(&gpu, target.format);
    let mut zone_pass = ModZonePass::new(&gpu, target.format);
    pass.prepare(&gpu, &camera, &scene);
    zone_pass.prepare(&gpu, &camera, 1.0);
    zone_pass.upload(&gpu, &zone_meshes);

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zone-smoke"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("zone-smoke-rp"),
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
        pass.draw(&mut rp, &scene);
        zone_pass.draw(&mut rp, &zone_meshes);
    }
    gpu.queue.submit(Some(encoder.finish()));

    let rgba = read_rgba(&gpu, &target).expect("readback");
    assert_eq!(rgba.len(), 320 * 200 * 4);
    let clear = [13u8, 13, 20, 255];
    let differs = rgba.chunks_exact(4).any(|p| {
        (i16::from(p[0]) - i16::from(clear[0])).abs() > 8
            || (i16::from(p[1]) - i16::from(clear[1])).abs() > 8
            || (i16::from(p[2]) - i16::from(clear[2])).abs() > 8
    });
    assert!(differs, "expected zone wall or scene pixels");
}
