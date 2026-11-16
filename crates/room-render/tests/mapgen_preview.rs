//! Screenshots of generated maps (manual look at `room-mapgen` output).
//!
//! `MAPGEN_SHOTS=1 cargo test -p room-render --test mapgen_preview -- --ignored --nocapture`
//! (Doom II at `testdata/doom/doom2.wad` or `MAPGEN_IWAD`). PNGs go to /tmp.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_nodebuild::ensure_nodes;
use room_render::{
    Camera, GpuContext, MaterialCatalog, OffscreenTarget, UnlitPass, build_scene_with_sky_ex,
    encode_png, read_rgba,
};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

#[test]
#[ignore = "manual preview: renders generated maps to /tmp"]
fn mapgen_screenshots() {
    let wad = std::env::var("MAPGEN_IWAD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom2.wad")
        });
    let Ok(gpu) = pollster::block_on(GpuContext::headless()) else {
        eprintln!("skip: no wgpu");
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut mats = MaterialCatalog::new();
    mats.load_playpal(&vfs);
    mats.load_texturex(&vfs);
    mats.ensure_sampler(&gpu.device);
    let cases: Vec<(u64, u8, room_mapgen::Theme, room_mapgen::Biomes)> =
        std::env::var("MAPGEN_CASES")
            .ok()
            .map(|s| {
                s.split(',')
                    .filter_map(|c| {
                        let mut it = c.split(':');
                        let seed = it.next()?.parse().ok()?;
                        let players = it.next()?.parse().ok()?;
                        let theme = serde_json::from_str(&format!("\"{}\"", it.next()?)).ok()?;
                        let biomes = it
                            .next()
                            .and_then(|b| serde_json::from_str(&format!("\"{b}\"")).ok())
                            .unwrap_or_default();
                        Some((seed, players, theme, biomes))
                    })
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    (7, 8, room_mapgen::Theme::Tech, room_mapgen::Biomes::Mixed),
                    (3, 16, room_mapgen::Theme::Hell, room_mapgen::Biomes::Mixed),
                ]
            });
    // One view per biome when MAPGEN_BIOME_VIEWS is set, else the first starts.
    let per_biome = std::env::var("MAPGEN_BIOME_VIEWS").is_ok();
    for (seed, players, theme, biomes) in cases {
        let opts = room_mapgen::Options {
            seed,
            players,
            theme,
            biomes,
            verticality: 2,
            ..Default::default()
        };
        let g = room_mapgen::generate(&opts).unwrap();
        let mut map = g.map.to_geometry().unwrap();
        ensure_nodes(&mut map);
        let mut starts: Vec<_> = g
            .map
            .things
            .iter()
            .filter(|t| t.type_id == 11 || t.type_id == 1)
            .collect();
        if per_biome {
            // First start found in each biome's room.
            let mut seen = std::collections::HashSet::new();
            starts.retain(|t| {
                let b = room_mapgen::biome_at(&g, t.x, t.y);
                b.is_some() && seen.insert(b)
            });
        }
        for (k, t) in starts.iter().take(6).enumerate() {
            let (x, y) = (f32::from(t.x), f32::from(t.y));
            let sid = room_nodebuild::point_in_sector(&map, f64::from(x), f64::from(y)).unwrap();
            let floor = map.sectors[sid.raw() as usize].floor_height as f32;
            let a = f32::from(t.angle).to_radians();
            let camera = Camera {
                eye: Vec3::new(x, y, floor + 41.0),
                forward: Vec3::new(a.cos(), a.sin(), -0.08).normalize(),
                ..Camera::default()
            };
            let scene = build_scene_with_sky_ex(&map, &camera, None, Some("SKY1"), None);
            for r in scene.flat_ranges.iter().chain(scene.wall_ranges.iter()) {
                let _ = mats.get_or_upload(&gpu.device, &gpu.queue, &vfs, &r.name);
            }
            let _ = mats.get_or_upload(&gpu.device, &gpu.queue, &vfs, "SKY1");
            let target = OffscreenTarget::new(&gpu, 640, 400);
            let mut pass = UnlitPass::new(&gpu, target.format);
            pass.prepare(&gpu, &camera, &scene);
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("mapgen"),
                });
            {
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mapgen"),
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
            let rgba = read_rgba(&gpu, &target).unwrap();
            let mut png = Vec::new();
            encode_png(640, 400, &rgba, &mut png).unwrap();
            let tag = room_mapgen::biome_at(&g, t.x, t.y)
                .map_or("x".to_owned(), |b| format!("{b:?}").to_lowercase());
            let out = format!("/tmp/mapgen-{seed}-{k}-{tag}.png");
            std::fs::write(&out, &png).unwrap();
            eprintln!("wrote {out}");
        }
    }
}
