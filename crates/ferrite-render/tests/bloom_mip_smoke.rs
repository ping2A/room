//! Bloom mip chain soft check (skipped without GPU).

#![allow(clippy::unwrap_used)]

use ferrite_render::{BLOOM_LEVELS, BloomPass, GpuContext, OffscreenTarget};
use wgpu::util::DeviceExt;

#[test]
fn bloom_mip_chain_builds_levels() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping bloom_mip_chain_builds_levels: no wgpu adapter");
            return;
        }
    };
    let target = OffscreenTarget::new(&gpu, 128, 128);
    let mut bloom = BloomPass::new(&gpu, target.format);
    bloom.ensure_size(&gpu, 128, 128);
    assert_eq!(bloom.level_count(), BLOOM_LEVELS);
    assert_eq!(bloom.level_width(0), Some(64));
    assert_eq!(bloom.level_width(1), Some(32));

    // Bright centre pixel so extract has signal.
    let bright = gpu.device.create_texture_with_data(
        &gpu.queue,
        &wgpu::TextureDescriptor {
            label: Some("bloom-src"),
            size: wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: target.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &{
            let mut px = vec![0u8; 128 * 128 * 4];
            for y in 56..72 {
                for x in 56..72 {
                    let i = (y * 128 + x) * 4;
                    px[i] = 255;
                    px[i + 1] = 255;
                    px[i + 2] = 255;
                    px[i + 3] = 255;
                }
            }
            px
        },
    );
    let src_view = bright.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bloom-smoke"),
        });
    let out = bloom.apply(&gpu, &mut encoder, &src_view, 128, 128, 0.3);
    gpu.queue.submit(Some(encoder.finish()));
    assert!(out.is_some(), "bloom composite view missing");
}
