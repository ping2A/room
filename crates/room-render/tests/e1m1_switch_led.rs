//! E1M1 SW1COMP→SW2COMP must change rendered LED pixels (offscreen).
//!
//! Doom1 COMP LEDs stay red (dim → brighter); they do not turn green.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_render::{
    Camera, GpuContext, MaterialCatalog, OffscreenTarget, UnlitPass, build_scene_with_sky_ex_sized,
    encode_png, read_rgba,
};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

fn doom1() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    p.is_file().then_some(p)
}

fn render_switch(map: &room_map::MapGeometry, vfs: &room_vfs::Vfs, gpu: &GpuContext) -> Vec<u8> {
    let mut mats = MaterialCatalog::new();
    mats.load_playpal(vfs);
    mats.load_texturex(vfs);
    mats.ensure_sampler(&gpu.device);
    let sizes = mats.wall_texel_sizes();
    // Alcove between STARG3 (y=-3008) and switch (y=-3072), facing the front side.
    let camera = Camera {
        eye: Vec3::new(288.0, -3040.0, 41.0),
        forward: Vec3::new(0.0, -1.0, 0.0),
        ..Camera::default()
    };
    let scene = build_scene_with_sky_ex_sized(map, &camera, None, None, None, Some(&sizes));
    assert!(
        scene
            .wall_ranges
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case("SW1COMP")
                || r.name.eq_ignore_ascii_case("SW2COMP")),
        "switch wall must be in the scene"
    );
    let target = OffscreenTarget::new(gpu, 320, 200);
    let mut pass = UnlitPass::new(gpu, target.format);
    pass.light_mode = room_render::LightMode::DoomSoftware;
    pass.prepare(gpu, &camera, &scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("switch-led"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("switch-led-rp"),
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
        pass.draw_materials(gpu, &mut rp, &scene, &mut mats, vfs);
    }
    gpu.queue.submit(Some(encoder.finish()));
    read_rgba(gpu, &target).expect("readback")
}

/// Mean red channel of reddish texels (r > g+20, r > 40).
fn mean_red(rgba: &[u8]) -> f32 {
    let mut sum = 0u64;
    let mut n = 0u64;
    for i in (0..rgba.len()).step_by(4) {
        let r = u64::from(rgba[i]);
        let g = u64::from(rgba[i + 1]);
        let b = u64::from(rgba[i + 2]);
        if r > g + 20 && r > b + 15 && r > 40 {
            sum += r;
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { sum as f32 / n as f32 }
}

#[test]
fn e1m1_switch_led_pixels_change_on_flip() {
    let Some(wad) = doom1() else {
        return;
    };
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => return,
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);

    let side = map.sides.get_mut(184).expect("side 184");
    assert_eq!(side.lower.to_ascii_uppercase(), "SW1COMP");
    assert_eq!(side.y_offset_bottom, 8);

    let before = render_switch(&map, &vfs, &gpu);
    map.sides[184].lower = "SW2COMP".into();
    let after = render_switch(&map, &vfs, &gpu);

    let mut diff = 0u32;
    let mut max_delta = 0u32;
    for i in (0..before.len()).step_by(4) {
        let dr = (i32::from(before[i]) - i32::from(after[i])).unsigned_abs();
        let dg = (i32::from(before[i + 1]) - i32::from(after[i + 1])).unsigned_abs();
        let db = (i32::from(before[i + 2]) - i32::from(after[i + 2])).unsigned_abs();
        let d = dr + dg + db;
        if d > 0 {
            diff += 1;
        }
        max_delta = max_delta.max(d);
    }
    let red0 = mean_red(&before);
    let red1 = mean_red(&after);
    eprintln!("SW1→SW2 differing pixels={diff} max_delta={max_delta} mean_red {red0:.1}→{red1:.1}");

    if std::env::var_os("ROOM_DUMP_SWITCH").is_some() {
        let mut a = Vec::new();
        let mut b = Vec::new();
        encode_png(320, 200, &before, &mut a).unwrap();
        encode_png(320, 200, &after, &mut b).unwrap();
        std::fs::write("/tmp/sw1comp.png", a).unwrap();
        std::fs::write("/tmp/sw2comp.png", b).unwrap();
    }

    assert!(
        diff > 20 && max_delta > 10,
        "LED flip must change the framebuffer (diff={diff} max_delta={max_delta})"
    );
    // Doom1 COMP: SW2S1 is a brighter red LED, not green.
    assert!(
        red1 > red0 + 5.0,
        "SW2COMP LED must read brighter red than SW1COMP ({red0} → {red1})"
    );
}

#[test]
fn e1m1_switch_wall_uv_covers_led() {
    let Some(wad) = doom1() else {
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let mut mats = MaterialCatalog::new();
    mats.load_texturex(&vfs);
    let sizes = mats.wall_texel_sizes();
    let camera = Camera {
        eye: Vec3::new(288.0, -3040.0, 41.0),
        forward: Vec3::new(0.0, -1.0, 0.0),
        ..Camera::default()
    };
    let scene = build_scene_with_sky_ex_sized(&map, &camera, None, None, None, Some(&sizes));
    let range = scene
        .wall_ranges
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case("SW1COMP"))
        .expect("SW1COMP range");
    let mut v_min = f32::MAX;
    let mut v_max = f32::MIN;
    for v in &scene.opaque_walls[range.start as usize..(range.start + range.count) as usize] {
        v_min = v_min.min(v.uv[1]);
        v_max = v_max.max(v.uv[1]);
    }
    eprintln!("SW1COMP V range [{v_min}, {v_max}]");
    // LED patch at texture y≈72..88 → V 0.56..0.69
    assert!(
        v_min < 0.70 && v_max > 0.55,
        "UV must cover LED band ~0.56–0.69 (got [{v_min}, {v_max}])"
    );
}
