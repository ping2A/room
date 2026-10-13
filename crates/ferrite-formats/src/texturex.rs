//! Doom/Strife binary `TEXTURE1` / `TEXTURE2` lumps.

use crate::error::FormatError;

/// One patch placement inside a TEXTUREx definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TexturexPatch {
    /// Origin X.
    pub origin_x: i16,
    /// Origin Y.
    pub origin_y: i16,
    /// Index into PNAMES.
    pub pnames_index: u16,
}

/// One composed texture from TEXTURE1/2.
#[derive(Clone, Debug, PartialEq)]
pub struct TexturexDef {
    /// Texture name (uppercase, truncated at NUL).
    pub name: String,
    /// Width in pixels.
    pub width: i16,
    /// Height in pixels.
    pub height: i16,
    /// ZDoom flags half of the masked dword (`0x8000` = world panning).
    pub flags: u16,
    /// Scale X in 1/8 units (`0` = 1.0).
    pub scale_x: u8,
    /// Scale Y in 1/8 units (`0` = 1.0).
    pub scale_y: u8,
    /// Patches composing the texture.
    pub patches: Vec<TexturexPatch>,
}

fn le_i32(data: &[u8], o: usize) -> Result<i32, FormatError> {
    let b = data
        .get(o..o + 4)
        .ok_or(FormatError::Truncated("TEXTUREX i32"))?;
    Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn le_u32(data: &[u8], o: usize) -> Result<u32, FormatError> {
    let b = data
        .get(o..o + 4)
        .ok_or(FormatError::Truncated("TEXTUREX u32"))?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn le_i16(data: &[u8], o: usize) -> Result<i16, FormatError> {
    let b = data
        .get(o..o + 2)
        .ok_or(FormatError::Truncated("TEXTUREX i16"))?;
    Ok(i16::from_le_bytes([b[0], b[1]]))
}

fn le_u16(data: &[u8], o: usize) -> Result<u16, FormatError> {
    let b = data
        .get(o..o + 2)
        .ok_or(FormatError::Truncated("TEXTUREX u16"))?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

/// Parse a Doom or Strife `TEXTURE1`/`TEXTURE2` lump.
///
/// Layout detection follows GZDoom: try the Doom 22-byte header (+10-byte
/// patches); if offsets/sizes are inconsistent, fall back to Strife 18-byte
/// header (+6-byte patches).
pub fn parse_texturex(data: &[u8]) -> Result<Vec<TexturexDef>, FormatError> {
    if data.len() < 4 {
        return Err(FormatError::Truncated("TEXTUREX header"));
    }
    let count = le_i32(data, 0)? as usize;
    if count == 0 {
        return Ok(Vec::new());
    }
    let offset_table_end = 4usize
        .checked_add(
            count
                .checked_mul(4)
                .ok_or(FormatError::Invalid("TEXTUREX count"))?,
        )
        .ok_or(FormatError::Invalid("TEXTUREX count"))?;
    if data.len() < offset_table_end {
        return Err(FormatError::Truncated("TEXTUREX offset table"));
    }
    let mut offsets = Vec::with_capacity(count);
    for i in 0..count {
        let off = le_i32(data, 4 + i * 4)?;
        if off < 0 {
            return Err(FormatError::Invalid("TEXTUREX negative offset"));
        }
        offsets.push(off as usize);
    }

    if let Ok(defs) = try_parse(data, &offsets, Layout::Doom) {
        return Ok(defs);
    }
    try_parse(data, &offsets, Layout::Strife)
}

#[derive(Clone, Copy)]
enum Layout {
    Doom,
    Strife,
}

impl Layout {
    fn header_size(self) -> usize {
        match self {
            Self::Doom => 22,
            Self::Strife => 18,
        }
    }

    fn patch_size(self) -> usize {
        match self {
            Self::Doom => 10,
            Self::Strife => 6,
        }
    }
}

fn try_parse(
    data: &[u8],
    offsets: &[usize],
    layout: Layout,
) -> Result<Vec<TexturexDef>, FormatError> {
    let mut defs = Vec::with_capacity(offsets.len());
    for &off in offsets {
        if off.saturating_add(layout.header_size()) > data.len() {
            return Err(FormatError::Truncated("TEXTUREX texture header"));
        }
        let name = read_name8(&data[off..off + 8])?;
        let masked = le_u32(data, off + 8)?;
        let flags = (masked & 0xffff) as u16;
        let scale_x = ((masked >> 16) & 0xff) as u8;
        let scale_y = ((masked >> 24) & 0xff) as u8;
        let width = le_i16(data, off + 12)?;
        let height = le_i16(data, off + 14)?;
        let (patch_count_off, patch_start) = match layout {
            Layout::Doom => (off + 20, off + 22),
            Layout::Strife => (off + 16, off + 18),
        };
        let patch_count = le_u16(data, patch_count_off)? as usize;
        let patches_bytes = patch_count
            .checked_mul(layout.patch_size())
            .ok_or(FormatError::Invalid("TEXTUREX patch count"))?;
        if patch_start.saturating_add(patches_bytes) > data.len() {
            return Err(FormatError::Truncated("TEXTUREX patches"));
        }
        let mut patches = Vec::with_capacity(patch_count);
        for i in 0..patch_count {
            let p = patch_start + i * layout.patch_size();
            patches.push(TexturexPatch {
                origin_x: le_i16(data, p)?,
                origin_y: le_i16(data, p + 2)?,
                pnames_index: le_u16(data, p + 4)?,
            });
        }
        defs.push(TexturexDef {
            name,
            width,
            height,
            flags,
            scale_x,
            scale_y,
            patches,
        });
    }
    Ok(defs)
}

fn read_name8(raw: &[u8]) -> Result<String, FormatError> {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(8).min(8);
    let s = std::str::from_utf8(&raw[..end]).map_err(|_| FormatError::Invalid("TEXTUREX name"))?;
    Ok(s.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn doom_lump() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&8i32.to_le_bytes());
        b.extend_from_slice(b"DOOR\0\0\0\0");
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&64i16.to_le_bytes());
        b.extend_from_slice(&128i16.to_le_bytes());
        b.extend_from_slice(&0i32.to_le_bytes());
        b.extend_from_slice(&1i16.to_le_bytes());
        b.extend_from_slice(&0i16.to_le_bytes());
        b.extend_from_slice(&0i16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&1i16.to_le_bytes());
        b.extend_from_slice(&0i16.to_le_bytes());
        b
    }

    #[test]
    fn parse_doom_texturex() {
        let defs = parse_texturex(&doom_lump()).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "DOOR");
        assert_eq!(defs[0].width, 64);
        assert_eq!(defs[0].height, 128);
        assert_eq!(defs[0].patches[0].pnames_index, 0);
    }
}
