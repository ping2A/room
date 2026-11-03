//! PNG decoder with ZDoom `grAb` / `alPh` ancillary chunks.

use crate::error::FormatError;
use crate::image::{Image, ImagePixels, RgbaImage};
use std::io::Cursor;

const PNG_SIG: &[u8] = &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// Private ZDoom PNG metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PngZdoomMeta {
    /// `grAb` offsets (`None` if chunk absent).
    pub grab: Option<(i32, i32)>,
    /// `alPh` present — grayscale image is an alpha map.
    pub alph: bool,
}

/// Scan a PNG byte stream for `grAb` / `alPh` without decoding pixels.
pub fn scan_zdoom_png_chunks(data: &[u8]) -> Result<PngZdoomMeta, FormatError> {
    if data.len() < 8 || &data[..8] != PNG_SIG {
        return Err(FormatError::Invalid("not a PNG"));
    }
    let mut meta = PngZdoomMeta::default();
    let mut pos = 8usize;
    while pos + 12 <= data.len() {
        let len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let kind = &data[pos + 4..pos + 8];
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(len)
            .ok_or(FormatError::Truncated("PNG chunk"))?;
        let next = data_end
            .checked_add(4)
            .ok_or(FormatError::Truncated("PNG chunk CRC"))?;
        if next > data.len() {
            break;
        }
        if kind == b"grAb" {
            if len != 8 {
                return Err(FormatError::Invalid("grAb length"));
            }
            let chunk = &data[data_start..data_end];
            let mut x = i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let mut y = i32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
            // GZDoom clamps out-of-i16-range offsets to 0.
            if !(-32768..=32767).contains(&x) {
                x = 0;
            }
            if !(-32768..=32767).contains(&y) {
                y = 0;
            }
            meta.grab = Some((x, y));
        } else if kind == b"alPh" {
            meta.alph = true;
        } else if kind == b"IEND" {
            break;
        }
        pos = next;
    }
    Ok(meta)
}

/// Decode a PNG lump into [`Image`], honouring `grAb` / `alPh`.
pub fn parse_png(data: &[u8]) -> Result<Image, FormatError> {
    let meta = scan_zdoom_png_chunks(data)?;
    let (left_offset, top_offset) = meta.grab.unwrap_or((0, 0));

    let decoder = png::Decoder::new(Cursor::new(data));
    let mut reader = decoder
        .read_info()
        .map_err(|_| FormatError::Invalid("PNG header"))?;
    let info = reader.info().clone();
    let width = info.width;
    let height = info.height;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|_| FormatError::Invalid("PNG frame"))?;
    let color = frame.color_type;

    match color {
        png::ColorType::Indexed => {
            // Expand with the PNG palette. Mapping the indices through PLAYPAL
            // turns Brutal Doom sprites into the wrong colours.
            let indices = &buf[..frame.buffer_size()];
            let pal = info.palette.as_deref().unwrap_or(&[]);
            let trns = info.trns.as_deref().unwrap_or(&[]);
            let mut pixels = Vec::with_capacity(indices.len() * 4);
            let mut has_translucency = meta.alph;
            for &idx in indices {
                let i = usize::from(idx);
                let (r, g, b) = if i * 3 + 2 < pal.len() {
                    (pal[i * 3], pal[i * 3 + 1], pal[i * 3 + 2])
                } else {
                    (0, 0, 0)
                };
                // Honour tRNS. With no tRNS, index 0 stays a hole (ZDoom sprites).
                let a = if !trns.is_empty() {
                    trns.get(i).copied().unwrap_or(255)
                } else if idx == 0 {
                    0
                } else {
                    255
                };
                if a != 255 {
                    has_translucency = true;
                }
                pixels.extend_from_slice(&[r, g, b, a]);
            }
            Ok(Image {
                left_offset,
                top_offset,
                has_translucency,
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width,
                    height,
                    pixels,
                }),
            })
        }
        png::ColorType::Grayscale if meta.alph => {
            // Alpha map: gray → A, RGB white.
            let gray = &buf[..frame.buffer_size()];
            let mut pixels = Vec::with_capacity(gray.len() * 4);
            let mut has_translucency = false;
            for &g in gray {
                if g != 255 {
                    has_translucency = true;
                }
                pixels.extend_from_slice(&[255, 255, 255, g]);
            }
            Ok(Image {
                left_offset,
                top_offset,
                has_translucency,
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width,
                    height,
                    pixels,
                }),
            })
        }
        png::ColorType::Grayscale => {
            let gray = &buf[..frame.buffer_size()];
            let mut pixels = Vec::with_capacity(gray.len() * 4);
            for &g in gray {
                pixels.extend_from_slice(&[g, g, g, 255]);
            }
            Ok(Image {
                left_offset,
                top_offset,
                has_translucency: false,
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width,
                    height,
                    pixels,
                }),
            })
        }
        png::ColorType::GrayscaleAlpha => {
            let src = &buf[..frame.buffer_size()];
            let mut pixels = Vec::with_capacity(src.len() * 2);
            let mut has_translucency = false;
            for chunk in src.chunks_exact(2) {
                let g = chunk[0];
                let a = chunk[1];
                if a != 255 {
                    has_translucency = true;
                }
                pixels.extend_from_slice(&[g, g, g, a]);
            }
            Ok(Image {
                left_offset,
                top_offset,
                has_translucency,
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width,
                    height,
                    pixels,
                }),
            })
        }
        png::ColorType::Rgb => {
            let src = &buf[..frame.buffer_size()];
            let mut pixels = Vec::with_capacity(src.len() / 3 * 4);
            for chunk in src.chunks_exact(3) {
                pixels.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            Ok(Image {
                left_offset,
                top_offset,
                has_translucency: false,
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width,
                    height,
                    pixels,
                }),
            })
        }
        png::ColorType::Rgba => {
            let pixels = buf[..frame.buffer_size()].to_vec();
            let has_translucency = pixels.chunks_exact(4).any(|p| p[3] != 255);
            Ok(Image {
                left_offset,
                top_offset,
                has_translucency,
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width,
                    height,
                    pixels,
                }),
            })
        }
    }
}

/// True if `data` starts with a PNG signature.
#[must_use]
pub fn is_png(data: &[u8]) -> bool {
    data.len() >= 8 && &data[..8] == PNG_SIG
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn tiny_rgba_png_with_grab() -> Vec<u8> {
        // Build via png encoder then splice grAb before IEND.
        let mut raw = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut raw, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[255, 0, 0, 255]).unwrap();
        }
        // Insert grAb (-16, 48) before IEND.
        let iend = raw.windows(4).rposition(|w| w == b"IEND").unwrap() - 4;
        let mut grab = Vec::new();
        grab.extend_from_slice(&8u32.to_be_bytes());
        grab.extend_from_slice(b"grAb");
        grab.extend_from_slice(&(-16i32).to_be_bytes());
        grab.extend_from_slice(&48i32.to_be_bytes());
        // CRC of type+data (zlib crc32)
        let mut crc_data = Vec::new();
        crc_data.extend_from_slice(b"grAb");
        crc_data.extend_from_slice(&(-16i32).to_be_bytes());
        crc_data.extend_from_slice(&48i32.to_be_bytes());
        let crc = crc32(&crc_data);
        grab.extend_from_slice(&crc.to_be_bytes());
        let mut out = Vec::new();
        out.extend_from_slice(&raw[..iend]);
        out.extend_from_slice(&grab);
        out.extend_from_slice(&raw[iend..]);
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        // ISO-3309 / PNG CRC
        let mut c: u32 = 0xffff_ffff;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                if c & 1 != 0 {
                    c = (c >> 1) ^ 0xEDB8_8320;
                } else {
                    c >>= 1;
                }
            }
        }
        !c
    }

    #[test]
    fn grab_offsets() {
        let data = tiny_rgba_png_with_grab();
        let meta = scan_zdoom_png_chunks(&data).unwrap();
        assert_eq!(meta.grab, Some((-16, 48)));
        let img = parse_png(&data).unwrap();
        assert_eq!(img.left_offset, -16);
        assert_eq!(img.top_offset, 48);
        assert_eq!(img.width(), 1);
        assert_eq!(img.height(), 1);
    }
}
