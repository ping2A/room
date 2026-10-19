//! Offscreen Duke Nukem 3D E1L1 (converted by `room-build`) from the player
//! start; writes `target/duke-shots/e1l1_<deg>[_up<pitch>].png`. Skipped without
//! `testdata/duke/duke3d.grp` or a GPU.

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

#[test]
fn duke3d_e1l1_renders_from_the_start() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let Ok(grp) = std::fs::read(root.join("testdata/duke/duke3d.grp")) else {
        eprintln!("skipping: testdata/duke/duke3d.grp missing");
        return;
    };
    let Ok(gpu) = pollster::block_on(GpuContext::headless()) else {
        eprintln!("skipping: no wgpu adapter");
        return;
    };
    let g = room_build::Grp::parse(&grp).unwrap();
    let build = room_build::parse_map(g.get("E1L1.MAP").unwrap()).unwrap();
    let wad = room_build::grp_to_wad(&grp).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", wad)
        .unwrap()
        .build()
        .unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let info = String::from_utf8_lossy(&vfs.read(vfs.find_any_ns("MAPINFO").unwrap()).unwrap())
        .into_owned();
    let sky = info
        .lines()
        .skip_while(|l| !l.starts_with("map E1M1"))
        .find_map(|l| l.strip_prefix("sky1 "))
        .and_then(|s| s.split_whitespace().next())
        .map(str::to_owned);

    let mut mats = MaterialCatalog::new();
    mats.load_playpal(&vfs);
    mats.load_texturex(&vfs);
    mats.ensure_sampler(&gpu.device);
    let sizes = mats.wall_texel_sizes();

    let start = build.start;
    let eye = Vec3::new(
        start[0] as f32 / 16.0,
        -(start[1] as f32) / 16.0,
        -(start[2] as f32) / 256.0,
    );
    let base = -f32::from(build.start_ang) * std::f32::consts::TAU / 2048.0;
    let out_dir = root.join("target/duke-shots");
    std::fs::create_dir_all(&out_dir).unwrap();
    let (w, h) = (800u32, 450u32);
    let target = OffscreenTarget::new(&gpu, w, h);
    let mut pass = UnlitPass::new(&gpu, target.format);
    for (deg, up) in [
        (0, 0),
        (90, 0),
        (180, 0),
        (270, 0),
        (0, 40),
        (180, 40),
        (0, -35),
        (90, -35),
    ] {
        let a = base + (deg as f32).to_radians();
        let p = (up as f32).to_radians();
        let camera = Camera {
            eye,
            forward: Vec3::new(a.cos() * p.cos(), a.sin() * p.cos(), p.sin()),
            aspect: f64::from(w) / f64::from(h),
            ..Camera::default()
        };
        let scene =
            build_scene_with_sky_ex_sized(&map, &camera, None, sky.as_deref(), None, Some(&sizes));
        pass.prepare(&gpu, &camera, &scene);
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("duke"),
            });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("duke-rp"),
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
            pass.draw_materials(&gpu, &mut rp, &scene, &mut mats, &vfs);
        }
        gpu.queue.submit(Some(encoder.finish()));
        let rgba = read_rgba(&gpu, &target).expect("readback");
        assert!(
            rgba.chunks(4).any(|p| p[0] > 40 || p[1] > 40 || p[2] > 40),
            "frame {deg} is black"
        );
        let mut png = Vec::new();
        encode_png(w, h, &rgba, &mut png).unwrap();
        let name = if up == 0 {
            format!("e1l1_{deg}.png")
        } else {
            format!("e1l1_{deg}_up{up}.png")
        };
        std::fs::write(out_dir.join(name), png).unwrap();
    }

    // Swing doors are parked off-map until the playsim moves them. Place them
    // and look at the cinema from the street.
    place_polyobjs(&mut map);
    let camera = Camera {
        eye: Vec3::new(1045.0, -3965.0, 4.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        aspect: f64::from(w) / f64::from(h),
        ..Camera::default()
    };
    let scene =
        build_scene_with_sky_ex_sized(&map, &camera, None, sky.as_deref(), None, Some(&sizes));
    pass.prepare(&gpu, &camera, &scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("cinema"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cinema-rp"),
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
        pass.draw_materials(&gpu, &mut rp, &scene, &mut mats, &vfs);
    }
    gpu.queue.submit(Some(encoder.finish()));
    let rgba = read_rgba(&gpu, &target).expect("readback");
    let mut png = Vec::new();
    encode_png(w, h, &rgba, &mut png).unwrap();
    std::fs::write(out_dir.join("e1l1_cinema.png"), png).unwrap();
    // The closed doors sit in the middle of this view. A missing face is the
    // clear color; the door tile is dark glass with a brighter frame.
    let x0 = w as usize / 3;
    let x1 = 2 * w as usize / 3;
    let y0 = h as usize / 3;
    let y1 = 2 * h as usize / 3;
    let mut lit = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = &rgba[(y * w as usize + x) * 4..][..3];
            if p.iter().any(|&c| c > 24) {
                lit += 1;
            }
        }
    }
    assert!(
        lit > 40,
        "cinema doors are missing from the street view ({lit} lit pixels)"
    );
}

fn place_polyobjs(map: &mut room_map::MapGeometry) {
    let polys = map.setup.polyobjects.clone();
    for po in polys {
        let mut spawn = None;
        let mut anchor = None;
        for th in &map.things {
            if i32::from(th.angle) != po.id {
                continue;
            }
            let p = glam::DVec2::new(f64::from(th.x), f64::from(th.y));
            if th.type_id == 9301 {
                spawn = Some(p);
            } else if th.type_id == 9300 {
                anchor = Some(p);
            }
        }
        let (Some(spawn), Some(anchor)) = (spawn, anchor) else {
            continue;
        };
        let dx = spawn.x - anchor.x;
        let dy = spawn.y - anchor.y;
        for vid in &po.vertices {
            if let Some(v) = map.vertices.get_mut(vid.raw() as usize) {
                v.pos = room_core::DVec2::new(v.pos.x() + dx, v.pos.y() + dy);
            }
        }
    }
}
