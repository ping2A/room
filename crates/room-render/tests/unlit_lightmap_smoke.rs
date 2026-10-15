//! UnlitPass lightmap bind brightens the frame (skipped without GPU).

#![allow(clippy::unwrap_used)]

use glam::Vec3;
use room_nodebuild::apply_nodes;
use room_render::{
    Camera, GpuContext, LIGHTMAP_MAGIC, LightmapAtlas, LightmapSurface, OffscreenTarget, UnlitPass,
    build_scene, read_rgba, render_offscreen, world_xy_to_surface_uv,
};
use room_testkit::{MapBuilder, rect};

#[test]
fn unlit_lightmap_brightens() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping unlit_lightmap_brightens: no wgpu adapter");
            return;
        }
    };
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(128.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let camera = Camera {
        eye: Vec3::new(128.0, 64.0, 41.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };
    let mut scene = build_scene(&map, &camera, None);
    scene.dyn_lights.clear();
    let surf = LightmapSurface {
        u0: 0.1,
        v0: 0.1,
        u1: 0.9,
        v1: 0.9,
    };
    scene.lightmap_surfaces.push(surf.clone());
    for v in &mut scene.opaque_walls {
        v.lm_uv = world_xy_to_surface_uv(&surf, v.pos[0], v.pos[1], 256.0);
    }
    for v in &mut scene.flats {
        v.lm_uv = world_xy_to_surface_uv(&surf, v.pos[0], v.pos[1], 256.0);
    }
    assert!(
        scene.opaque_walls.iter().any(|v| v.lm_uv[0] > 0.0),
        "expected non-zero lightmap UVs on walls"
    );

    let mut lump = Vec::new();
    lump.extend_from_slice(LIGHTMAP_MAGIC);
    lump.extend_from_slice(&1u32.to_le_bytes());
    lump.extend_from_slice(&16u32.to_le_bytes());
    lump.extend_from_slice(&16u32.to_le_bytes());
    lump.extend_from_slice(&1u32.to_le_bytes());
    lump.extend_from_slice(&0.0f32.to_le_bytes());
    lump.extend_from_slice(&0.0f32.to_le_bytes());
    lump.extend_from_slice(&1.0f32.to_le_bytes());
    lump.extend_from_slice(&1.0f32.to_le_bytes());
    let rgba = vec![220u8; 16 * 16 * 4];
    let atlas = LightmapAtlas::from_lump_and_rgba(&lump, &rgba).unwrap();

    let target = OffscreenTarget::new(&gpu, 160, 100);
    let mut pass = UnlitPass::new(&gpu, target.format);
    render_offscreen(&gpu, &mut pass, &target, &camera, &scene);
    let dark = read_rgba(&gpu, &target).expect("dark");

    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lm-test"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16 * 4),
            rows_per_image: Some(16),
        },
        wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    pass.set_lightmap(&gpu.device, &view, 0.75);
    render_offscreen(&gpu, &mut pass, &target, &camera, &scene);
    let lit = read_rgba(&gpu, &target).expect("lit");

    let mean = |px: &[u8]| -> f32 {
        let mut s = 0u64;
        let mut n = 0u64;
        for c in px.chunks_exact(4) {
            s += u64::from(c[0]) + u64::from(c[1]) + u64::from(c[2]);
            n += 3;
        }
        s as f32 / n as f32
    };
    let md = mean(&dark);
    let ml = mean(&lit);
    eprintln!("unlit lightmap mean dark={md:.2} lit={ml:.2}");
    assert!(
        ml > md + 1.0,
        "expected lightmap to brighten (dark={md} lit={ml})"
    );
}
