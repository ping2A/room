//! TGA image decoder (uncompressed TrueColor / paletted / greyscale).

use crate::error::FormatError;
use crate::image::{Image, ImagePixels, PalettedImage, RgbaImage};

/// Parse an uncompressed TGA lump.
pub fn parse_tga(data: &[u8]) -> Result<Image, FormatError> {
    if data.len() < 18 {
        return Err(FormatError::Truncated("TGA header"));
    }
    let id_len = data[0] as usize;
    let color_map_type = data[1];
    let image_type = data[2];
    let width = u16::from_le_bytes([data[12], data[13]]) as u32;
    let height = u16::from_le_bytes([data[14], data[15]]) as u32;
    let bpp = data[16];
    let descriptor = data[17];
    let top_origin = descriptor & 0x20 != 0;

    if width == 0 || height == 0 {
        return Err(FormatError::Invalid("TGA empty"));
    }
    let pixels_start = 18 + id_len;
    // Skip color map if present.
    let cm_len = u16::from_le_bytes([data[5], data[6]]) as usize;
    let cm_entry = data[7] as usize;
    let cm_bytes = if color_map_type == 1 {
        cm_len * cm_entry.div_ceil(8)
    } else {
        0
    };
    let data_start = pixels_start + cm_bytes;
    if data_start > data.len() {
        return Err(FormatError::Truncated("TGA colormap"));
    }

    let pixel_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or(FormatError::Invalid("TGA size"))?;

    match (image_type, bpp) {
        // Uncompressed truecolor 24/32
        (2, 24 | 32) => {
            let stride = (bpp / 8) as usize;
            let need = data_start + pixel_count * stride;
            if data.len() < need {
                return Err(FormatError::Truncated("TGA pixels"));
            }
            let src = &data[data_start..need];
            let mut pixels = vec![0u8; pixel_count * 4];
            for y in 0..height as usize {
                let src_y = if top_origin {
                    y
                } else {
                    height as usize - 1 - y
                };
                for x in 0..width as usize {
                    let si = (src_y * width as usize + x) * stride;
                    let di = (y * width as usize + x) * 4;
                    pixels[di] = src[si + 2]; // BGR → RGB
                    pixels[di + 1] = src[si + 1];
                    pixels[di + 2] = src[si];
                    pixels[di + 3] = if stride == 4 { src[si + 3] } else { 255 };
                }
            }
            let has_translucency = stride == 4 && pixels.chunks_exact(4).any(|p| p[3] != 255);
            Ok(Image {
                left_offset: 0,
                top_offset: 0,
                has_translucency,
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width,
                    height,
                    pixels,
                }),
            })
        }
        // Uncompressed greyscale 8
        (3, 8) => {
            let need = data_start + pixel_count;
            if data.len() < need {
                return Err(FormatError::Truncated("TGA grey"));
            }
            let src = &data[data_start..need];
            let mut indices = vec![0u8; pixel_count];
            for y in 0..height as usize {
                let src_y = if top_origin {
                    y
                } else {
                    height as usize - 1 - y
                };
                let row = &src[src_y * width as usize..(src_y + 1) * width as usize];
                indices[y * width as usize..(y + 1) * width as usize].copy_from_slice(row);
            }
            Ok(Image {
                left_offset: 0,
                top_offset: 0,
                has_translucency: indices.contains(&0),
                pixels: ImagePixels::Paletted(PalettedImage {
                    width,
                    height,
                    indices,
                    alpha: Vec::new(),
                }),
            })
        }
        _ => Err(FormatError::Invalid("TGA type/bpp unsupported")),
    }
}

/// True if the buffer looks like a TGA we might accept (heuristic: valid bpp + type).
#[must_use]
pub fn is_tga(data: &[u8]) -> bool {
    if data.len() < 18 {
        return false;
    }
    matches!(data[2], 2 | 3) && matches!(data[16], 8 | 24 | 32)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn tga_24_bottom_up() {
        let mut b = vec![0u8; 18];
        b[2] = 2; // truecolor
        b[12] = 1;
        b[13] = 0; // w=1
        b[14] = 1;
        b[15] = 0; // h=1
        b[16] = 24;
        // BGR pixel
        b.extend_from_slice(&[0x00, 0x80, 0xFF]);
        let img = parse_tga(&b).unwrap();
        assert_eq!(img.width(), 1);
        if let ImagePixels::Rgba8(r) = &img.pixels {
            assert_eq!(&r.pixels[..3], &[0xFF, 0x80, 0x00]);
        } else {
            panic!("expected rgba");
        }
    }
}
