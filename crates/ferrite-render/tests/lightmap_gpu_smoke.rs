//! Lightmap atlas GPU upload + debug draw (skipped without adapter).

#![allow(clippy::unwrap_used)]

use ferrite_render::{
    GpuContext, LIGHTMAP_MAGIC, LightmapAtlas, LightmapPass, OffscreenTarget, read_rgba,
};

#[test]
fn lightmap_gpu_debug_draw() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping lightmap_gpu_debug_draw: no wgpu adapter");
            return;
        }
    };
    let mut lump = Vec::new();
    lump.extend_from_slice(LIGHTMAP_MAGIC);
    lump.extend_from_slice(&1u32.to_le_bytes());
    lump.extend_from_slice(&8u32.to_le_bytes());
    lump.extend_from_slice(&8u32.to_le_bytes());
    lump.extend_from_slice(&1u32.to_le_bytes());
    lump.extend_from_slice(&0.0f32.to_le_bytes());
    lump.extend_from_slice(&0.0f32.to_le_bytes());
    lump.extend_from_slice(&1.0f32.to_le_bytes());
    lump.extend_from_slice(&1.0f32.to_le_bytes());
    let mut rgba = vec![0u8; 8 * 8 * 4];
    for p in rgba.chunks_exact_mut(4) {
        p[0] = 200;
        p[1] = 100;
        p[2] = 50;
        p[3] = 255;
    }
    let atlas = LightmapAtlas::from_lump_and_rgba(&lump, &rgba).unwrap();
    let target = OffscreenTarget::new(&gpu, 32, 32);
    let mut pass = LightmapPass::new(&gpu, target.format);
    pass.upload(&gpu, &atlas);
    pass.draw_debug(&gpu, &target.color_view, 1.0);
    let out = read_rgba(&gpu, &target).expect("readback");
    let mean_r =
        out.chunks_exact(4).map(|c| u64::from(c[0])).sum::<u64>() as f32 / (out.len() / 4) as f32;
    assert!(mean_r > 50.0, "expected warm lightmap mean_r={mean_r}");
}
