//! Offscreen RGBA readback for golden-image tests and in-game screenshots / GIFs.

use crate::device::GpuContext;
use crate::offscreen::OffscreenTarget;
use thiserror::Error;

/// Capture errors.
#[derive(Debug, Error)]
pub enum CaptureError {
    /// Buffer map / encode failed.
    #[error("{0}")]
    Message(String),
}

/// Copy the color target into a tightly-packed RGBA8 buffer (row-major).
pub fn read_rgba(gpu: &GpuContext, target: &OffscreenTarget) -> Result<Vec<u8>, CaptureError> {
    read_texture_rgba(gpu, &target.color, target.width, target.height)
}

/// Copy any `COPY_SRC` 2D texture into a tightly-packed RGBA8 buffer (row-major).
///
/// BGRA8 surface formats are swizzled to RGBA.
pub fn read_texture_rgba(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, CaptureError> {
    let unpadded_bytes_per_row = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
    let buffer_size = u64::from(padded_bytes_per_row) * u64::from(height);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("capture-enc"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device
        .poll(wgpu::PollType::Wait)
        .map_err(|e| CaptureError::Message(e.to_string()))?;
    rx.recv()
        .map_err(|e| CaptureError::Message(e.to_string()))?
        .map_err(|e| CaptureError::Message(e.to_string()))?;
    let data = slice.get_mapped_range();
    let mut rgba = Vec::with_capacity((unpadded_bytes_per_row * height) as usize);
    for row in 0..height {
        let start = (row * padded_bytes_per_row) as usize;
        let end = start + unpadded_bytes_per_row as usize;
        rgba.extend_from_slice(&data[start..end]);
    }
    drop(data);
    buffer.unmap();
    if matches!(
        texture.format(),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        swizzle_bgra_to_rgba(&mut rgba);
    }
    Ok(rgba)
}

/// In-place BGRA8 → RGBA8.
pub fn swizzle_bgra_to_rgba(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
}

/// Encode RGBA8 as PNG into `out`.
pub fn encode_png(
    width: u32,
    height: u32,
    rgba: &[u8],
    out: &mut Vec<u8>,
) -> Result<(), CaptureError> {
    let mut encoder = png::Encoder::new(out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| CaptureError::Message(e.to_string()))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| CaptureError::Message(e.to_string()))?;
    Ok(())
}

/// Encode an animated GIF from RGBA frames (`delay_cs` = centiseconds per frame).
pub fn encode_gif(
    width: u16,
    height: u16,
    frames: &mut [Vec<u8>],
    delay_cs: u16,
    out: &mut Vec<u8>,
) -> Result<(), CaptureError> {
    if frames.is_empty() {
        return Err(CaptureError::Message("gif: no frames".into()));
    }
    let expected = usize::from(width) * usize::from(height) * 4;
    let mut encoder = gif::Encoder::new(&mut *out, width, height, &[])
        .map_err(|e| CaptureError::Message(e.to_string()))?;
    encoder
        .set_repeat(gif::Repeat::Infinite)
        .map_err(|e| CaptureError::Message(e.to_string()))?;
    for frame_rgba in frames.iter_mut() {
        if frame_rgba.len() != expected {
            return Err(CaptureError::Message(format!(
                "gif: frame len {} != {expected}",
                frame_rgba.len()
            )));
        }
        let mut frame = gif::Frame::from_rgba_speed(width, height, frame_rgba, 10);
        frame.delay = delay_cs;
        encoder
            .write_frame(&frame)
            .map_err(|e| CaptureError::Message(e.to_string()))?;
    }
    Ok(())
}

/// Nearest-neighbour downscale of RGBA8 (keeps aspect; `max_w`/`max_h` caps).
#[must_use]
pub fn scale_rgba_nn(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    max_w: u32,
    max_h: u32,
) -> (u32, u32, Vec<u8>) {
    if src_w == 0 || src_h == 0 || src.len() < (src_w * src_h * 4) as usize {
        return (0, 0, Vec::new());
    }
    let (dst_w, dst_h) = if src_w <= max_w && src_h <= max_h {
        (src_w, src_h)
    } else {
        let sx = f64::from(max_w) / f64::from(src_w);
        let sy = f64::from(max_h) / f64::from(src_h);
        let s = sx.min(sy);
        (
            (f64::from(src_w) * s).round().max(1.0) as u32,
            (f64::from(src_h) * s).round().max(1.0) as u32,
        )
    };
    if dst_w == src_w && dst_h == src_h {
        return (dst_w, dst_h, src.to_vec());
    }
    let mut out = vec![0u8; (dst_w * dst_h * 4) as usize];
    for y in 0..dst_h {
        let sy = y * src_h / dst_h;
        for x in 0..dst_w {
            let sx = x * src_w / dst_w;
            let si = ((sy * src_w + sx) * 4) as usize;
            let di = ((y * dst_w + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    (dst_w, dst_h, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swizzle_swaps_channels() {
        let mut px = [10u8, 20, 30, 255];
        swizzle_bgra_to_rgba(&mut px);
        assert_eq!(px, [30, 20, 10, 255]);
    }

    #[test]
    fn encode_gif_one_frame() {
        let mut frames = vec![vec![255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255]];
        let mut out = Vec::new();
        encode_gif(2, 2, &mut frames, 10, &mut out).unwrap();
        assert!(out.starts_with(b"GIF"));
        assert!(out.len() > 32);
    }

    #[test]
    fn scale_halves() {
        let src = vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let (w, h, out) = scale_rgba_nn(&src, 2, 2, 1, 1);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out, vec![1, 2, 3, 4]);
    }
}
