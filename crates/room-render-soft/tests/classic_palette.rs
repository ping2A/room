//! SR10 / `docs/29` U4 · B1: the GPU `ClassicPalette` light mode against the
//! software renderer, pixel by pixel, on the same Freedoom views (no things).
//!
//! Skipped without a wgpu adapter or the Freedoom WADs. It does not demand
//! parity (the GPU mode has no COLORMAP path yet, `docs/29` U4); it measures
//! the difference and fails only when it grows past a ceiling.
//!
//! `ROOM_WRITE_REPORT=1` rewrites `docs/qa/classic-palette-vs-software.md`
//! and the side-by-side images in `docs/qa/classic-palette/`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_render::{
    Camera, GpuContext, LightMode, MaterialCatalog, OffscreenTarget, UnlitPass,
    build_scene_with_sky_ex_sized, read_rgba,
};
use room_render_soft::shot::{ShotOptions, default_sky, encode_png, shoot};
use room_vfs::{Vfs, VfsBuilder};
use std::fmt::Write as _;
use std::path::PathBuf;

const W: u32 = 320;
const H: u32 = 200;

/// Views: (WAD, map, facing in degrees or the start's).
const VIEWS: &[(&str, &str, Option<f64>)] = &[
    ("freedoom1.wad", "E1M1", None),
    ("freedoom1.wad", "E1M1", Some(180.0)),
    ("freedoom2.wad", "MAP01", None),
    ("freedoom2.wad", "MAP01", Some(90.0)),
    ("freedoom2.wad", "MAP01", Some(225.0)),
];

/// Ceiling on the mean absolute channel difference (0–255) of any view.
/// The worst view measured 23.5 in 2026-11 (see the report); a GPU change that pushes a view past
/// it is a regression of the classic look.
const MEAN_ABS_CEILING: f64 = 30.0;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vfs(file: &str) -> Option<Vfs> {
    let p = root().join("testdata/freedoom").join(file);
    p.is_file()
        .then(|| VfsBuilder::new().mount_path(&p).unwrap().build().unwrap())
}

struct Stats {
    identical: f64,
    close: f64,
    mean_abs: f64,
    luma_soft: f64,
    luma_gpu: f64,
}

fn luma(p: &[u8]) -> f64 {
    0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2])
}

fn compare(soft: &[u8], gpu: &[u8]) -> (Stats, Vec<u8>) {
    let n = (W * H) as usize;
    let (mut identical, mut close, mut abs, mut ls, mut lg) = (0usize, 0usize, 0u64, 0.0, 0.0);
    let mut diff = Vec::with_capacity(n * 4);
    for (a, b) in soft.chunks_exact(4).zip(gpu.chunks_exact(4)) {
        let d: [u8; 3] = std::array::from_fn(|c| a[c].abs_diff(b[c]));
        let max = d.into_iter().max().unwrap_or(0);
        identical += usize::from(max == 0);
        close += usize::from(max <= 16);
        abs += d.iter().map(|&v| u64::from(v)).sum::<u64>();
        ls += luma(a);
        lg += luma(b);
        // Heat: amplified difference, red where the GPU is brighter, blue
        // where it is darker.
        let v = (u32::from(max) * 4).min(255) as u8;
        if luma(b) >= luma(a) {
            diff.extend_from_slice(&[v, v / 3, v / 3, 255]);
        } else {
            diff.extend_from_slice(&[v / 3, v / 3, v, 255]);
        }
    }
    let nf = n as f64;
    (
        Stats {
            identical: identical as f64 / nf * 100.0,
            close: close as f64 / nf * 100.0,
            mean_abs: abs as f64 / (nf * 3.0),
            luma_soft: ls / nf,
            luma_gpu: lg / nf,
        },
        diff,
    )
}

/// Software | GPU | difference, side by side.
fn strip(parts: [&[u8]; 3]) -> Vec<u8> {
    let mut out = Vec::with_capacity((W * 3 * H * 4) as usize);
    for y in 0..H as usize {
        for p in parts {
            out.extend_from_slice(&p[y * W as usize * 4..(y + 1) * W as usize * 4]);
        }
    }
    out
}

fn gpu_view(
    gpu: &GpuContext,
    vfs: &Vfs,
    map: &room_map::MapGeometry,
    sky: &str,
    eye: Vec3,
    angle: f64,
) -> Vec<u8> {
    let a = angle.to_radians();
    let camera = Camera {
        eye,
        forward: Vec3::new(a.cos() as f32, a.sin() as f32, 0.0),
        // The software projection: 320 columns span 90°, square pixels in
        // the 320 × 200 buffer.
        fov_y_deg: 2.0 * (f64::from(H) / f64::from(W)).atan().to_degrees(),
        aspect: f64::from(W) / f64::from(H),
        ..Camera::default()
    };
    let mut mats = MaterialCatalog::new();
    mats.load_playpal(vfs);
    mats.load_texturex(vfs);
    mats.ensure_sampler(&gpu.device);
    let sizes = mats.wall_texel_sizes();
    let scene =
        build_scene_with_sky_ex_sized(map, &camera, Some(&[]), Some(sky), None, Some(&sizes));
    let target = OffscreenTarget::new(gpu, W, H);
    let mut pass = UnlitPass::new(gpu, target.format);
    pass.light_mode = LightMode::ClassicPalette;
    pass.prepare(gpu, &camera, &scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("classic"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("classic-rp"),
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
        pass.draw_materials(gpu, &mut rp, &scene, &mut mats, vfs);
    }
    gpu.queue.submit(Some(encoder.finish()));
    read_rgba(gpu, &target).expect("readback")
}

#[test]
fn classic_palette_against_software() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skip: no wgpu adapter");
            return;
        }
    };
    let write = std::env::var_os("ROOM_WRITE_REPORT").is_some();
    let img_dir = root().join("docs/qa/classic-palette");
    let mut rows = String::new();
    let mut worst = 0.0f64;
    let mut ran = 0;
    for (i, &(file, map_name, angle)) in VIEWS.iter().enumerate() {
        let Some(vfs) = vfs(file) else {
            eprintln!("skip: testdata/freedoom/{file} missing (scripts/fetch-freedoom.sh)");
            continue;
        };
        let mut map = room_map::load_map(&vfs, map_name).unwrap();
        room_nodebuild::ensure_nodes(&mut map);
        let opts = ShotOptions {
            width: W,
            height: H,
            angle,
            things: false,
            ..ShotOptions::default()
        };
        let shot = shoot(&vfs, map_name, &map, &opts).expect("software shot");
        let (_, _, soft) = shot.rgba(None);
        let start = map.things.iter().find(|t| t.type_id == 1).unwrap();
        let facing = angle.unwrap_or_else(|| f64::from(start.angle));
        let (x, y) = (f32::from(start.x), f32::from(start.y));
        let floor = shot
            .view
            .level
            .point_in_sector((x * 65536.0) as i32, (y * 65536.0) as i32)
            .and_then(|s| shot.view.level.sectors.get(s))
            .map_or(0, |s| s.floor);
        let eye = Vec3::new(x, y, floor as f32 / 65536.0 + 41.0);
        let gpu_rgba = gpu_view(&gpu, &vfs, &map, &default_sky(map_name), eye, facing);
        let (s, diff) = compare(&soft, &gpu_rgba);
        eprintln!(
            "{map_name} {facing}°: identical {:.2} %, within 16 {:.2} %, mean |Δ| {:.2}, luma soft {:.1} gpu {:.1}",
            s.identical, s.close, s.mean_abs, s.luma_soft, s.luma_gpu
        );
        assert!(
            gpu_rgba.chunks_exact(4).any(|p| p[..3] != [0, 0, 0]),
            "GPU view is blank"
        );
        worst = worst.max(s.mean_abs);
        ran += 1;
        let name = format!("{}_{i}.png", map_name.to_ascii_lowercase());
        let _ = writeln!(
            rows,
            "| {file} {map_name} | {facing:.0}° | {:.2} % | {:.2} % | {:.2} | {:.1} | {:.1} | [image](classic-palette/{name}) |",
            s.identical, s.close, s.mean_abs, s.luma_soft, s.luma_gpu
        );
        if write {
            std::fs::create_dir_all(&img_dir).unwrap();
            let png = encode_png(W * 3, H, &strip([&soft, &gpu_rgba, &diff])).unwrap();
            std::fs::write(img_dir.join(&name), png).unwrap();
        }
    }
    if ran == 0 {
        return;
    }
    if write {
        let report = format!(
            "# GPU `ClassicPalette` against the software renderer\n\n\
             Generated by `crates/room-render-soft/tests/classic_palette.rs` with\n\
             `ROOM_WRITE_REPORT=1 cargo test -p room-render-soft --test classic_palette`\n\
             (`docs/40` SR10, `docs/29` U4 / B1). Each view is drawn at 320 × 200 with\n\
             the same eye, facing and 90° horizontal projection by the GPU\n\
             (`LightMode::ClassicPalette`, the map's sky, no things) and by `room-render-soft`\n\
             (vanilla 8-bit, COLORMAP light). The images show software | GPU |\n\
             difference (×4; red where the GPU is brighter, blue where darker).\n\n\
             | View | Facing | Identical | Within 16 | Mean \\|Δ\\| (0–255) | Luma software | Luma GPU | Images |\n\
             |---|---|---|---|---|---|---|---|\n\
             {rows}\n\
             ## Reading it\n\n\
             The GPU mode has no COLORMAP path (`docs/29` U4): it shades true-colour\n\
             texels with a light curve, so few pixels are identical even where the\n\
             geometry matches. Geometry mismatches (missing walls, shifted edges)\n\
             would show as solid shapes or doubled outlines in the difference image.\n\n\
             What the November 2026 run shows:\n\n\
             - **Geometry matches.** Wall edges, door frames, steps and flat borders\n\
               fall on the same pixels in every view; the differences outline texels,\n\
               not surfaces.\n\
             - **The GPU is brighter.** Mean luma is higher in every view, by up to\n\
               ~22 levels (MAP01 start). The gap grows with distance: the GPU curve\n\
               fades less than COLORMAP's diminishing light, so far walls and dark\n\
               rooms (the corridor ahead of the MAP01 start and the room north of it) come out lit\n\
               on the GPU and near black in software.\n\
             - **Near, bright surfaces agree.** Close walls at full light are within\n\
               16 levels or identical (E1M1 facing 180°, MAP01 facing 225°: 74–80 %\n\
               within 16).\n\
             - **Texel noise.** Single-pixel speckle on floors and walls comes from\n\
               the GPU sampling true-colour texels with a smooth light factor where\n\
               software picks one of 32 palette rows per column or span.\n\n\
             Closing the gap is U4: paletted index textures, the COLORMAP row from\n\
             light and distance as the software renderer computes it, then PLAYPAL.\n\n\
             The test fails if any view's mean |Δ| rises past {MEAN_ABS_CEILING}, so a\n\
             GPU change that moves the classic look further from the software\n\
             renderer is caught. When U4 lands, the target is 0 differing pixels.\n"
        );
        std::fs::write(
            root().join("docs/qa/classic-palette-vs-software.md"),
            report,
        )
        .unwrap();
    }
    assert!(
        worst <= MEAN_ABS_CEILING,
        "ClassicPalette drifted from the software renderer: mean |Δ| {worst:.2} > {MEAN_ABS_CEILING}"
    );
}
