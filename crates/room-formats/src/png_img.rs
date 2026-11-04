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

    let cleaned = strip_invalid_trns(data);
    let mut decoder = png::Decoder::new(Cursor::new(cleaned.as_ref()));
    // 16-bit channels → 8-bit. Sub-byte samples (1/2/4-bit) stay packed and
    // are unpacked below so indexed sprites keep their raw palette indices.
    decoder.set_transformations(png::Transformations::STRIP_16);
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
    let depth = frame.bit_depth as u8;
    if depth < 8 && matches!(color, png::ColorType::Indexed | png::ColorType::Grayscale) {
        let unpacked = unpack_samples(&buf, frame.line_size, width, height, depth);
        let scale = matches!(color, png::ColorType::Grayscale);
        buf = if scale {
            let max = (1u16 << depth) - 1;
            unpacked
                .into_iter()
                .map(|v| u8::try_from(u16::from(v) * 255 / max).unwrap_or(255))
                .collect()
        } else {
            unpacked
        };
    }
    let samples = match color {
        png::ColorType::Indexed | png::ColorType::Grayscale if depth < 8 => {
            usize::try_from(u64::from(width) * u64::from(height)).unwrap_or(0)
        }
        _ => frame.buffer_size(),
    };
    let buf = &buf[..samples.min(buf.len())];

    match color {
        png::ColorType::Indexed => {
            // Expand with the PNG palette. Mapping the indices through PLAYPAL
            // turns Brutal Doom sprites into the wrong colours.
            let indices = buf;
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
                // Honour tRNS. GZDoom `FPNGTexture`: without tRNS a paletted
                // PNG is fully opaque (index 0 is an ordinary colour).
                let a = trns.get(i).copied().unwrap_or(255);
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
            let gray = buf;
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
            // tRNS on greyscale: one grey level is transparent.
            let key = info.trns.as_deref().and_then(|t| match t.len() {
                1 => Some(u16::from(t[0])),
                n if n >= 2 => Some(u16::from_be_bytes([t[0], t[1]])),
                _ => None,
            });
            let gray = buf;
            let mut pixels = Vec::with_capacity(gray.len() * 4);
            let mut keyed = false;
            for &g in gray {
                let hole = key == Some(u16::from(g));
                keyed |= hole;
                pixels.extend_from_slice(&[g, g, g, if hole { 0 } else { 255 }]);
            }
            Ok(Image {
                left_offset,
                top_offset,
                has_translucency: keyed,
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width,
                    height,
                    pixels,
                }),
            })
        }
        png::ColorType::GrayscaleAlpha => {
            let src = buf;
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
            // GZDoom `FPNGTexture`: an RGB tRNS is a colour key (`NonPaletteTrans`).
            // The png crate keeps 16-bit samples as BE pairs and narrows an
            // 8-bit image's key to one byte per channel.
            let key = info.trns.as_deref().and_then(|t| match t.len() {
                3 => Some([u16::from(t[0]), u16::from(t[1]), u16::from(t[2])]),
                n if n >= 6 => Some([
                    u16::from_be_bytes([t[0], t[1]]),
                    u16::from_be_bytes([t[2], t[3]]),
                    u16::from_be_bytes([t[4], t[5]]),
                ]),
                _ => None,
            });
            let src = buf;
            let mut pixels = Vec::with_capacity(src.len() / 3 * 4);
            let mut keyed = false;
            for chunk in src.chunks_exact(3) {
                let hole = key.is_some_and(|k| {
                    k == [
                        u16::from(chunk[0]),
                        u16::from(chunk[1]),
                        u16::from(chunk[2]),
                    ]
                });
                keyed |= hole;
                let a = if hole { 0 } else { 255 };
                pixels.extend_from_slice(&[chunk[0], chunk[1], chunk[2], a]);
            }
            Ok(Image {
                left_offset,
                top_offset,
                has_translucency: keyed,
                pixels: ImagePixels::Rgba8(RgbaImage {
                    width,
                    height,
                    pixels,
                }),
            })
        }
        png::ColorType::Rgba => {
            let pixels = buf.to_vec();
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

/// Drop `tRNS` chunks from RGBA / grey-alpha PNGs. The PNG spec forbids them
/// there and the decoder rejects the file; ZDoom ignores them. Brutal Doom
/// ships hundreds of such sprites (blood, gibs, particles).
fn strip_invalid_trns(data: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    // IHDR is the first chunk: sig(8) len(4) "IHDR"(4) w(4) h(4) depth(1) color(1).
    let color = data.get(25).copied().unwrap_or(0);
    if data.get(12..16) != Some(b"IHDR".as_slice()) || !(color == 4 || color == 6) {
        return std::borrow::Cow::Borrowed(data);
    }
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&data[..8.min(data.len())]);
    let mut i = 8usize;
    let mut changed = false;
    while i + 12 <= data.len() {
        let len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let end = i.saturating_add(12).saturating_add(len);
        if end > data.len() {
            out.extend_from_slice(&data[i..]);
            i = data.len();
            break;
        }
        if &data[i + 4..i + 8] == b"tRNS" {
            changed = true;
        } else {
            out.extend_from_slice(&data[i..end]);
        }
        i = end;
    }
    out.extend_from_slice(&data[i..]);
    if changed {
        std::borrow::Cow::Owned(out)
    } else {
        std::borrow::Cow::Borrowed(data)
    }
}

/// Unpack 1/2/4-bit samples (MSB first, rows padded to a byte) to one byte each.
fn unpack_samples(buf: &[u8], line_size: usize, width: u32, height: u32, depth: u8) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let per_byte = 8 / usize::from(depth.max(1));
    let mask = (1u16 << depth) - 1;
    let mut out = Vec::with_capacity(w * h);
    for row in 0..h {
        let line = buf
            .get(row * line_size..(row + 1) * line_size)
            .unwrap_or(&[]);
        for x in 0..w {
            let byte = line.get(x / per_byte).copied().unwrap_or(0);
            let shift = 8 - usize::from(depth) * (x % per_byte + 1);
            out.push(u8::try_from((u16::from(byte) >> shift) & mask).unwrap_or(0));
        }
    }
    out
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

    fn encode(
        w: u32,
        h: u32,
        color: png::ColorType,
        depth: png::BitDepth,
        pal: Option<Vec<u8>>,
        trns: Option<Vec<u8>>,
        data: &[u8],
    ) -> Vec<u8> {
        let mut raw = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut raw, w, h);
            enc.set_color(color);
            enc.set_depth(depth);
            if let Some(p) = pal {
                enc.set_palette(p);
            }
            if let Some(t) = trns {
                enc.set_trns(t);
            }
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(data).unwrap();
        }
        raw
    }

    fn rgba(img: &Image) -> &[u8] {
        match &img.pixels {
            ImagePixels::Rgba8(r) => &r.pixels,
            _ => panic!("expected rgba"),
        }
    }

    #[test]
    fn four_bit_indexed_sprite_decodes_every_pixel() {
        // 3×2, 4-bit: row0 = 1,2,0  row1 = 3,1,2 (rows padded to whole bytes).
        let pal = vec![0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255];
        let png = encode(
            3,
            2,
            png::ColorType::Indexed,
            png::BitDepth::Four,
            Some(pal),
            Some(vec![0]),
            &[0x12, 0x00, 0x31, 0x20],
        );
        let img = parse_png(&png).unwrap();
        let px = rgba(&img);
        assert_eq!(px.len(), 3 * 2 * 4);
        assert_eq!(&px[0..4], &[255, 0, 0, 255]);
        assert_eq!(&px[4..8], &[0, 255, 0, 255]);
        assert_eq!(px[11], 0, "index 0 is transparent via tRNS");
        assert_eq!(&px[12..16], &[0, 0, 255, 255]);
    }

    #[test]
    fn rgb_trns_is_a_colour_key() {
        // Brutal Doom's flares (LENBA0): RGB with tRNS = black; black is a hole.
        let png = encode(
            2,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            None,
            Some(vec![0, 0, 0, 0, 0, 0]),
            &[0, 0, 0, 10, 100, 200],
        );
        let img = parse_png(&png).unwrap();
        let px = rgba(&img);
        assert_eq!(px[3], 0);
        assert_eq!(&px[4..8], &[10, 100, 200, 255]);
        assert!(img.has_translucency);
    }

    #[test]
    fn indexed_png_without_trns_is_opaque() {
        // GZDoom: no tRNS → no transparency, index 0 included (Brutal Doom's
        // all-black BLASCR fade screen is index 0 everywhere).
        let png = encode(
            2,
            1,
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            Some(vec![0, 0, 0, 255, 255, 255]),
            None,
            &[0, 1],
        );
        let img = parse_png(&png).unwrap();
        let px = rgba(&img);
        assert_eq!(&px[0..4], &[0, 0, 0, 255]);
        assert_eq!(&px[4..8], &[255, 255, 255, 255]);
    }

    #[test]
    fn one_and_two_bit_indexed_unpack() {
        let pal = vec![0, 0, 0, 9, 9, 9, 7, 7, 7, 5, 5, 5];
        let one = encode(
            9,
            1,
            png::ColorType::Indexed,
            png::BitDepth::One,
            Some(pal[..6].to_vec()),
            None,
            &[0b1010_0000, 0b1000_0000],
        );
        let px = parse_png(&one).unwrap();
        let px = rgba(&px);
        assert_eq!(px.len(), 9 * 4);
        assert_eq!((px[0], px[4], px[8], px[32]), (9, 0, 9, 9));
        let two = encode(
            5,
            1,
            png::ColorType::Indexed,
            png::BitDepth::Two,
            Some(pal),
            None,
            &[0b0110_1100, 0b0100_0000],
        );
        let px = parse_png(&two).unwrap();
        let px = rgba(&px);
        assert_eq!(px.len(), 5 * 4);
        assert_eq!((px[0], px[4], px[8], px[12], px[16]), (9, 7, 5, 0, 9));
    }

    #[test]
    fn rgba_png_with_stray_trns_still_decodes() {
        let good = encode(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            None,
            None,
            &[10, 20, 30, 40],
        );
        // Splice a 1-byte tRNS chunk right after IHDR (as Brutal Doom's blood does).
        let ihdr_end = 8 + 12 + 13;
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&1u32.to_be_bytes());
        chunk.extend_from_slice(b"tRNS");
        chunk.push(0);
        let crc = crc32(&chunk[4..]);
        chunk.extend_from_slice(&crc.to_be_bytes());
        let mut bad = good[..ihdr_end].to_vec();
        bad.extend_from_slice(&chunk);
        bad.extend_from_slice(&good[ihdr_end..]);
        let img = parse_png(&bad).unwrap();
        assert_eq!(rgba(&img), &[10, 20, 30, 40]);
    }

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
