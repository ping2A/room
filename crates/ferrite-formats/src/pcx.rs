//! PCX image decoder (8-bit paletted and 24-bit truecolor, RLE).

use crate::error::FormatError;
use crate::image::{Image, ImagePixels, PalettedImage, RgbaImage};

/// Decode PCX RLE into exactly `out.len()` bytes.
fn decode_pcx_rle(src: &[u8], mut pos: usize, out: &mut [u8]) -> Result<usize, FormatError> {
    let mut i = 0;
    while i < out.len() {
        let b = *src.get(pos).ok_or(FormatError::Truncated("PCX RLE"))?;
        pos += 1;
        if b & 0xC0 == 0xC0 {
            let count = (b & 0x3F) as usize;
            let val = *src.get(pos).ok_or(FormatError::Truncated("PCX RLE run"))?;
            pos += 1;
            for _ in 0..count {
                if i >= out.len() {
                    break;
                }
                out[i] = val;
                i += 1;
            }
        } else {
            out[i] = b;
            i += 1;
        }
    }
    Ok(pos)
}

/// Parse a PCX lump (8-bit indexed or 24-bit RGB).
pub fn parse_pcx(data: &[u8]) -> Result<Image, FormatError> {
    if data.len() < 128 || data[0] != 0x0A {
        return Err(FormatError::Invalid("not a PCX"));
    }
    let encoding = data[2];
    let bits = data[3];
    let xmin = u16::from_le_bytes([data[4], data[5]]) as u32;
    let ymin = u16::from_le_bytes([data[6], data[7]]) as u32;
    let xmax = u16::from_le_bytes([data[8], data[9]]) as u32;
    let ymax = u16::from_le_bytes([data[10], data[11]]) as u32;
    let planes = data[65];
    let bytes_per_line = u16::from_le_bytes([data[66], data[67]]) as usize;
    if encoding != 1 {
        return Err(FormatError::Invalid("PCX encoding"));
    }
    if xmax < xmin || ymax < ymin {
        return Err(FormatError::Invalid("PCX dimensions"));
    }
    let width = xmax - xmin + 1;
    let height = ymax - ymin + 1;
    let bitcount = bits as u32 * u32::from(planes);

    match bitcount {
        8 => {
            let line_bytes = bytes_per_line;
            let mut raw = vec![0u8; line_bytes * height as usize];
            decode_pcx_rle(&data[128..], 0, &mut raw)?;
            let mut indices = vec![0u8; (width * height) as usize];
            for y in 0..height as usize {
                let src = &raw[y * line_bytes..y * line_bytes + width as usize];
                indices[y * width as usize..(y + 1) * width as usize].copy_from_slice(src);
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
        24 => {
            let line_bytes = bytes_per_line * 3;
            let mut raw = vec![0u8; line_bytes * height as usize];
            decode_pcx_rle(&data[128..], 0, &mut raw)?;
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            for y in 0..height as usize {
                // Planar: R plane, G plane, B plane per scanline.
                let row = &raw[y * line_bytes..];
                let r_plane = &row[0..bytes_per_line];
                let g_plane = &row[bytes_per_line..bytes_per_line * 2];
                let b_plane = &row[bytes_per_line * 2..bytes_per_line * 3];
                for x in 0..width as usize {
                    let di = (y * width as usize + x) * 4;
                    pixels[di] = r_plane[x];
                    pixels[di + 1] = g_plane[x];
                    pixels[di + 2] = b_plane[x];
                    pixels[di + 3] = 255;
                }
            }
            Ok(Image {
                left_offset: 0,
                top_offset: 0,
                has_translucency: false,
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width,
                    height,
                    pixels,
                }),
            })
        }
        _ => Err(FormatError::Invalid("PCX bit depth")),
    }
}

/// True if `data` looks like a PCX header.
#[must_use]
pub fn is_pcx(data: &[u8]) -> bool {
    data.len() >= 128 && data[0] == 0x0A && data[2] == 1
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn pcx_8bit_1x1() {
        let mut h = vec![0u8; 128];
        h[0] = 0x0A;
        h[2] = 1; // RLE
        h[3] = 8; // bits
        // xmin/ymin = 0, xmax/ymax = 0 → 1x1
        h[65] = 1; // planes
        h[66] = 2;
        h[67] = 0; // bytes per line = 2 (even)
        let mut data = h;
        // One pixel value 42, then pad byte for line width 2
        data.push(42);
        data.push(0);
        let img = parse_pcx(&data).unwrap();
        assert_eq!(img.width(), 1);
        if let ImagePixels::Paletted(p) = &img.pixels {
            assert_eq!(p.indices[0], 42);
        } else {
            panic!("paletted");
        }
    }
}
