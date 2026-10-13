//! ZDoom `FON1` / `FON2` binary font lumps.

use crate::error::FormatError;

/// One decoded glyph (paletted indices, 0 = transparent).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontGlyph {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Row-major palette indices (`width * height`).
    pub indices: Vec<u8>,
}

/// Parsed `FON1` console font (256 fixed-size glyphs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fon1 {
    /// Character cell width.
    pub char_width: u16,
    /// Character cell height.
    pub char_height: u16,
    /// Glyphs for codes 0..=255.
    pub glyphs: Vec<FontGlyph>,
}

/// Parsed `FON2` font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fon2 {
    /// Shared glyph height.
    pub font_height: u16,
    /// First defined character code.
    pub first_char: u8,
    /// Last defined character code.
    pub last_char: u8,
    /// Constant width (all glyphs same width) when set.
    pub constant_width: bool,
    /// Whole-font kerning (0 if absent).
    pub kerning: i16,
    /// Palette RGB entries (index 0 unused/transparent; may include delimiter).
    pub palette: Vec<[u8; 3]>,
    /// Glyphs in order from `first_char`..=`last_char` (empty = missing).
    pub glyphs: Vec<Option<FontGlyph>>,
}

/// Decode PackBits-style RLE used by FON1/FON2 into `dest_len` bytes.
fn decode_rle(
    src: &[u8],
    mut pos: usize,
    dest_len: usize,
) -> Result<(Vec<u8>, usize), FormatError> {
    let mut out = Vec::with_capacity(dest_len);
    while out.len() < dest_len {
        let code = *src.get(pos).ok_or(FormatError::Truncated("font RLE"))? as i8;
        pos += 1;
        if code >= 0 {
            let n = code as usize + 1;
            if pos + n > src.len() {
                return Err(FormatError::Truncated("font RLE literal"));
            }
            out.extend_from_slice(&src[pos..pos + n]);
            pos += n;
        } else if code != -128 {
            let n = (-(code as i16)) as usize + 1;
            let b = *src
                .get(pos)
                .ok_or(FormatError::Truncated("font RLE repeat"))?;
            pos += 1;
            out.extend(std::iter::repeat_n(b, n));
        }
        // code == -128: no-op
    }
    if out.len() != dest_len {
        return Err(FormatError::Invalid("font RLE size"));
    }
    Ok((out, pos))
}

/// Parse a `FON1` lump.
pub fn parse_fon1(data: &[u8]) -> Result<Fon1, FormatError> {
    if data.len() < 8 || &data[0..4] != b"FON1" {
        return Err(FormatError::Invalid("not FON1"));
    }
    let char_width = u16::from_le_bytes([data[4], data[5]]);
    let char_height = u16::from_le_bytes([data[6], data[7]]);
    let glyph_bytes = (char_width as usize)
        .checked_mul(char_height as usize)
        .ok_or(FormatError::Invalid("FON1 size"))?;
    let mut pos = 8usize;
    let mut glyphs = Vec::with_capacity(256);
    for _ in 0..256 {
        let (indices, next) = decode_rle(data, pos, glyph_bytes)?;
        pos = next;
        glyphs.push(FontGlyph {
            width: char_width,
            height: char_height,
            indices,
        });
    }
    Ok(Fon1 {
        char_width,
        char_height,
        glyphs,
    })
}

/// Parse a `FON2` lump.
pub fn parse_fon2(data: &[u8]) -> Result<Fon2, FormatError> {
    if data.len() < 12 || &data[0..4] != b"FON2" {
        return Err(FormatError::Invalid("not FON2"));
    }
    let font_height = u16::from_le_bytes([data[4], data[5]]);
    let first_char = data[6];
    let last_char = data[7];
    if last_char < first_char {
        return Err(FormatError::Invalid("FON2 char range"));
    }
    let constant_width = data[8] != 0;
    let _shading = data[9];
    let palette_size = data[10] as usize; // entries excluding? GZDoom: ActiveColors = data[10]+1
    let flags = data[11];
    let count = (last_char - first_char) as usize + 1;

    let mut pos = 12usize;
    let kerning = if flags & 1 != 0 {
        if pos + 2 > data.len() {
            return Err(FormatError::Truncated("FON2 kerning"));
        }
        let k = i16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        k
    } else {
        0
    };

    // Width table
    let mut widths = Vec::with_capacity(count);
    if constant_width {
        if pos + 2 > data.len() {
            return Err(FormatError::Truncated("FON2 width"));
        }
        let w = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        widths.resize(count, w);
    } else {
        if pos + count * 2 > data.len() {
            return Err(FormatError::Truncated("FON2 widths"));
        }
        for i in 0..count {
            let o = pos + i * 2;
            widths.push(u16::from_le_bytes([data[o], data[o + 1]]));
        }
        pos += count * 2;
    }

    // Palette: (palette_size + 1) RGB triples (last is delimiter).
    let pal_entries = palette_size + 1;
    let pal_bytes = pal_entries
        .checked_mul(3)
        .ok_or(FormatError::Invalid("FON2 palette"))?;
    if pos + pal_bytes > data.len() {
        return Err(FormatError::Truncated("FON2 palette"));
    }
    let mut palette = Vec::with_capacity(pal_entries);
    for i in 0..pal_entries {
        let o = pos + i * 3;
        palette.push([data[o], data[o + 1], data[o + 2]]);
    }
    pos += pal_bytes;

    let mut glyphs = Vec::with_capacity(count);
    for &w in &widths {
        let dest = (w as usize)
            .checked_mul(font_height as usize)
            .ok_or(FormatError::Invalid("FON2 glyph"))?;
        if dest == 0 {
            glyphs.push(None);
            continue;
        }
        let (indices, next) = decode_rle(data, pos, dest)?;
        pos = next;
        glyphs.push(Some(FontGlyph {
            width: w,
            height: font_height,
            indices,
        }));
    }

    Ok(Fon2 {
        font_height,
        first_char,
        last_char,
        constant_width,
        kerning,
        palette,
        glyphs,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn encode_rle(pixels: &[u8]) -> Vec<u8> {
        // Emit as one literal run (max 128).
        assert!(pixels.len() <= 128);
        let mut v = Vec::new();
        v.push((pixels.len() - 1) as u8);
        v.extend_from_slice(pixels);
        v
    }

    #[test]
    fn fon1_roundtrip_one_pixel() {
        let mut data = Vec::new();
        data.extend_from_slice(b"FON1");
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        for i in 0..=255u8 {
            data.extend_from_slice(&encode_rle(&[i]));
        }
        let font = parse_fon1(&data).unwrap();
        assert_eq!(font.char_width, 1);
        assert_eq!(font.glyphs[65].indices, vec![65]);
    }
}
