//! The render-scale upscaler stretches a small image over a big target
//! (docs/41 M6). Skipped when no GPU adapter is available.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render::{GpuContext, Upscaler, read_texture_rgba};

fn target(gpu: &GpuContext, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("t"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

#[test]
fn a_two_by_two_image_fills_a_sixty_four_square_target() {
    let Ok(gpu) = pollster::block_on(GpuContext::headless()) else {
        eprintln!("skipping: no wgpu adapter");
        return;
    };
    // Red, green / blue, white.
    let small = target(&gpu, 2, 2);
    let px: [u8; 16] = [
        255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 255, 255,
    ];
    gpu.queue.write_texture(
        small.0.as_image_copy(),
        &px,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    let big = target(&gpu, 64, 64);
    let up = Upscaler::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm);
    let mut enc = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    up.draw(&gpu.device, &mut enc, &small.1, &big.1);
    gpu.queue.submit(Some(enc.finish()));
    let out = read_texture_rgba(&gpu, &big.0, 64, 64).unwrap();
    let at = |x: usize, y: usize| {
        let i = (y * 64 + x) * 4;
        [out[i], out[i + 1], out[i + 2]]
    };
    // Corners keep the source colours (clamped edges), the right way up.
    assert_eq!(at(0, 0), [255, 0, 0]);
    assert_eq!(at(63, 0), [0, 255, 0]);
    assert_eq!(at(0, 63), [0, 0, 255]);
    assert_eq!(at(63, 63), [255, 255, 255]);
    // The middle is a blend of all four (bilinear, not nearest).
    let mid = at(32, 32);
    assert!(mid.iter().all(|&c| c > 60 && c < 200), "{mid:?}");
}
