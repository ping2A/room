//! UZ6 (docs/36): thick fog (`ThickFogDistance` / `ThickFogMultiplier`) and
//! sky fog (`skyfog` / `Level.SetSkyFog`) on the GPU. Golden:
//! `testdata/goldens/uz6_fog_gpu.txt`, mean colours of a far-wall and a sky
//! region quantised to steps of 8 so driver rounding does not matter.
//! Skipped when no wgpu adapter is present.
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-render --test golden_uz6_fog`

#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_nodebuild::apply_nodes;
use room_render::{Camera, GpuContext, OffscreenTarget, UnlitPass, build_scene, read_rgba, render_offscreen};
use room_testkit::{MapBuilder, rect};
use std::fmt::Write as _;

const W: u32 = 160;
const H: u32 = 100;

fn mean(rgba: &[u8], x0: u32, y0: u32, x1: u32, y1: u32) -> [u32; 3] {
    let mut sum = [0u64; 3];
    let mut n = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * W + x) * 4) as usize;
            for c in 0..3 {
                sum[c] += u64::from(rgba[i + c]);
            }
            n += 1;
        }
    }
    sum.map(|s| ((s / n.max(1)) as u32 / 8) * 8)
}

fn dist(a: [u32; 3], b: [u32; 3]) -> u32 {
    a.iter().zip(b).map(|(x, y)| x.abs_diff(y)).sum()
}

#[test]
fn uz6_thick_fog_and_sky_fog_match_golden() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping uz6 fog golden: no wgpu adapter");
            return;
        }
    };
    // A long open-sky hall: the far wall is 3000 units away.
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 512.0, 3200.0), 0.0, 256.0)
        .sector_textures("a", "FLOOR4_8", "F_SKY1")
        .player_start(256.0, 128.0, 90)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let camera = Camera {
        eye: Vec3::new(256.0, 128.0, 64.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    let scene = build_scene(&map, &camera, None);
    assert!(!scene.sky_parts.is_empty(), "F_SKY1 ceiling builds the sky shell");
    let target = OffscreenTarget::new(&gpu, W, H);
    let mut pass = UnlitPass::new(&gpu, target.format);
    pass.set_fog_density(0.5);
    let mut out = String::new();
    let mut runs = Vec::new();
    for (label, thick, sky) in [
        ("plain", (0.0, 0.0), 0.0),
        ("thick", (512.0, 8.0), 0.0),
        ("skyfog", (0.0, 0.0), 255.0),
    ] {
        pass.set_thick_fog(thick.0, thick.1);
        pass.set_sky_fog(sky);
        render_offscreen(&gpu, &mut pass, &target, &camera, &scene);
        let rgba = read_rgba(&gpu, &target).expect("readback");
        // Far wall: the centre of the view. Sky: the top band.
        let far = mean(&rgba, W / 2 - 4, H / 2 - 4, W / 2 + 4, H / 2 + 4);
        let top = mean(&rgba, W / 2 - 20, 0, W / 2 + 20, 6);
        let _ = writeln!(out, "{label:<7} far={far:?} sky={top:?}");
        runs.push((far, top));
    }
    // Fog colour (0.05, 0.05, 0.08) as the target stores it.
    let fog = mean(&{
        pass.set_thick_fog(0.0, 0.0);
        pass.set_sky_fog(255.0);
        pass.set_fog_density(1000.0);
        render_offscreen(&gpu, &mut pass, &target, &camera, &scene);
        read_rgba(&gpu, &target).expect("readback")
    }, W / 2 - 4, H / 2 - 4, W / 2 + 4, H / 2 + 4);
    let _ = writeln!(out, "fog     {fog:?}");
    let (plain, thick, skyfog) = (runs[0], runs[1], runs[2]);
    assert!(
        dist(thick.0, fog) <= dist(plain.0, fog),
        "thick fog pulls the far wall toward the fog colour: {out}"
    );
    assert!(
        dist(skyfog.1, fog) <= dist(plain.1, fog),
        "sky fog 255 pulls the sky toward the fog colour: {out}"
    );
    assert_eq!(plain.0, skyfog.0, "sky fog leaves walls alone: {out}");
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/goldens/uz6_fog_gpu.txt");
    if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &out).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap();
    assert_eq!(out, want, "fog render differs from {}", path.display());
}
