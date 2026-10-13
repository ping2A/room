//! BMP decoder (uncompressed BI_RGB 8/24/32-bit).

use crate::error::FormatError;
use crate::image::{Image, ImagePixels, PalettedImage, RgbaImage};

/// Parse a Windows BMP lump.
pub fn parse_bmp(data: &[u8]) -> Result<Image, FormatError> {
    if data.len() < 54 || &data[0..2] != b"BM" {
        return Err(FormatError::Invalid("not a BMP"));
    }
    let pixel_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let dib_size = u32::from_le_bytes([data[14], data[15], data[16], data[17]]) as usize;
    if dib_size < 40 || data.len() < 14 + dib_size {
        return Err(FormatError::Truncated("BMP DIB"));
    }
    let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let height_raw = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let planes = u16::from_le_bytes([data[26], data[27]]);
    let bpp = u16::from_le_bytes([data[28], data[29]]);
    let compression = u32::from_le_bytes([data[30], data[31], data[32], data[33]]);
    if planes != 1 || compression != 0 {
        return Err(FormatError::Invalid("BMP compression/planes"));
    }
    if width <= 0 {
        return Err(FormatError::Invalid("BMP width"));
    }
    let top_down = height_raw < 0;
    let height = height_raw.unsigned_abs();
    let w = width as usize;
    let h = height as usize;

    match bpp {
        24 | 32 => {
            let stride_src = (w * (bpp as usize) / 8).div_ceil(4) * 4;
            let need = pixel_offset
                .checked_add(
                    stride_src
                        .checked_mul(h)
                        .ok_or(FormatError::Invalid("BMP size"))?,
                )
                .ok_or(FormatError::Invalid("BMP size"))?;
            if data.len() < need {
                return Err(FormatError::Truncated("BMP pixels"));
            }
            let mut pixels = vec![0u8; w * h * 4];
            for y in 0..h {
                let src_y = if top_down { y } else { h - 1 - y };
                let row = &data[pixel_offset + src_y * stride_src..];
                for x in 0..w {
                    let si = x * (bpp as usize / 8);
                    let di = (y * w + x) * 4;
                    pixels[di] = row[si + 2];
                    pixels[di + 1] = row[si + 1];
                    pixels[di + 2] = row[si];
                    pixels[di + 3] = if bpp == 32 { row[si + 3] } else { 255 };
                }
            }
            Ok(Image {
                left_offset: 0,
                top_offset: 0,
                has_translucency: bpp == 32 && pixels.chunks_exact(4).any(|p| p[3] != 255),
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width: width as u32,
                    height,
                    pixels,
                }),
            })
        }
        8 => {
            let palette_off = 14 + dib_size;
            let colors_used = u32::from_le_bytes([data[46], data[47], data[48], data[49]]) as usize;
            let ncolors = if colors_used == 0 { 256 } else { colors_used };
            if data.len() < palette_off + ncolors * 4 {
                return Err(FormatError::Truncated("BMP palette"));
            }
            // We keep indices; palette is available to callers via PLAYPAL later.
            let _ = &data[palette_off..palette_off + ncolors * 4];
            let stride_src = w.div_ceil(4) * 4;
            let need = pixel_offset
                .checked_add(
                    stride_src
                        .checked_mul(h)
                        .ok_or(FormatError::Invalid("BMP size"))?,
                )
                .ok_or(FormatError::Invalid("BMP size"))?;
            if data.len() < need {
                return Err(FormatError::Truncated("BMP indices"));
            }
            let mut indices = vec![0u8; w * h];
            for y in 0..h {
                let src_y = if top_down { y } else { h - 1 - y };
                let row =
                    &data[pixel_offset + src_y * stride_src..pixel_offset + src_y * stride_src + w];
                indices[y * w..(y + 1) * w].copy_from_slice(row);
            }
            Ok(Image {
                left_offset: 0,
                top_offset: 0,
                has_translucency: indices.contains(&0),
                pixels: ImagePixels::Paletted(PalettedImage {
                    width: width as u32,
                    height,
                    indices,
                    alpha: Vec::new(),
                }),
            })
        }
        _ => Err(FormatError::Invalid("BMP bpp")),
    }
}

/// True if `data` starts with a BMP signature.
#[must_use]
pub fn is_bmp(data: &[u8]) -> bool {
    data.len() >= 2 && &data[0..2] == b"BM"
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn bmp_24_1x1() {
        let mut b = Vec::new();
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&70u32.to_le_bytes()); // file size placeholder
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&54u32.to_le_bytes()); // pixel offset
        b.extend_from_slice(&40u32.to_le_bytes()); // DIB size
        b.extend_from_slice(&1i32.to_le_bytes()); // w
        b.extend_from_slice(&1i32.to_le_bytes()); // h
        b.extend_from_slice(&1u16.to_le_bytes()); // planes
        b.extend_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes()); // compression
        b.extend_from_slice(&0u32.to_le_bytes()); // image size
        b.extend_from_slice(&0i32.to_le_bytes());
        b.extend_from_slice(&0i32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        // BGR + pad to 4
        b.extend_from_slice(&[0x11, 0x22, 0x33, 0x00]);
        let img = parse_bmp(&b).unwrap();
        assert_eq!(img.width(), 1);
        if let ImagePixels::Rgba8(r) = &img.pixels {
            assert_eq!(&r.pixels[..3], &[0x33, 0x22, 0x11]);
        } else {
            panic!("rgba");
        }
    }
}
