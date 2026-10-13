//! Doom picture ("patch") format.

use crate::error::FormatError;

/// One post in a column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchPost {
    /// Top delta (absolute after tall-patch fixup).
    pub top: u16,
    /// Pixel indices.
    pub pixels: Vec<u8>,
}

/// One column of posts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchColumn {
    /// Posts bottom-to-end terminator already consumed.
    pub posts: Vec<PatchPost>,
}

/// Parsed Doom patch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    /// Width.
    pub width: u16,
    /// Height.
    pub height: u16,
    /// Left offset.
    pub left_offset: i16,
    /// Top offset.
    pub top_offset: i16,
    /// Columns.
    pub columns: Vec<PatchColumn>,
}

/// Validate and parse a Doom picture lump.
pub fn parse_patch(data: &[u8]) -> Result<Patch, FormatError> {
    if data.len() < 8 {
        return Err(FormatError::Truncated("patch header"));
    }
    let width = u16::from_le_bytes([data[0], data[1]]);
    let height = u16::from_le_bytes([data[2], data[3]]);
    let left_offset = i16::from_le_bytes([data[4], data[5]]);
    let top_offset = i16::from_le_bytes([data[6], data[7]]);
    if !(1..=2048).contains(&width) || !(1..=2048).contains(&height) {
        return Err(FormatError::Invalid("patch dimensions"));
    }
    let header_cols = 8 + usize::from(width) * 4;
    if data.len() < header_cols {
        return Err(FormatError::Truncated("column offsets"));
    }
    let mut columns = Vec::with_capacity(width as usize);
    for col in 0..width as usize {
        let off_i = 8 + col * 4;
        let off = u32::from_le_bytes(data[off_i..off_i + 4].try_into().unwrap_or([0; 4])) as usize;
        if off >= data.len() {
            return Err(FormatError::Invalid("column offset"));
        }
        columns.push(parse_column(data, off, height)?);
    }
    Ok(Patch {
        width,
        height,
        left_offset,
        top_offset,
        columns,
    })
}

impl Patch {
    /// Rasterize to row-major palette indices (`0` = transparent / unwritten).
    ///
    /// Prefer [`Self::to_indices_masked`] for HUD/weapons: posts may contain
    /// palette index 0 as a real colour.
    #[must_use]
    pub fn to_indices(&self) -> Vec<u8> {
        self.to_indices_masked().0
    }

    /// Rasterize to `(indices, alpha)` — unwritten gaps get `alpha=0`; every
    /// post pixel (including palette index 0) gets `alpha=255`.
    #[must_use]
    pub fn to_indices_masked(&self) -> (Vec<u8>, Vec<u8>) {
        let w = usize::from(self.width);
        let h = usize::from(self.height);
        let n = w.saturating_mul(h);
        let mut indices = vec![0u8; n];
        let mut alpha = vec![0u8; n];
        for (x, col) in self.columns.iter().enumerate() {
            if x >= w {
                break;
            }
            for post in &col.posts {
                let top = usize::from(post.top);
                for (i, &px) in post.pixels.iter().enumerate() {
                    let y = top + i;
                    if y < h {
                        let di = y * w + x;
                        indices[di] = px;
                        alpha[di] = 255;
                    }
                }
            }
        }
        (indices, alpha)
    }
}

fn parse_column(data: &[u8], mut pos: usize, height: u16) -> Result<PatchColumn, FormatError> {
    let mut posts = Vec::new();
    let mut prev_top: i32 = -1;
    loop {
        if pos >= data.len() {
            return Err(FormatError::Truncated("column posts"));
        }
        let topdelta = data[pos];
        pos += 1;
        if topdelta == 0xFF {
            break;
        }
        if pos >= data.len() {
            return Err(FormatError::Truncated("post length"));
        }
        let length = data[pos] as usize;
        pos += 1;
        // pad byte
        if pos >= data.len() {
            return Err(FormatError::Truncated("post pad"));
        }
        pos += 1;
        if pos + length >= data.len() {
            // need length pixels + trailing pad
            return Err(FormatError::Truncated("post pixels"));
        }
        let pixels = data[pos..pos + length].to_vec();
        pos += length;
        // trailing pad
        pos += 1;

        let mut top = i32::from(topdelta);
        if prev_top >= 0 && top <= prev_top {
            // DeePsea tall patch
            top += prev_top;
        }
        if top < 0 || top as u16 >= height && length > 0 {
            // allow top == height only for empty; still accept and clamp warn via Invalid soft?
            // Keep permissive for fuzz: reject clearly broken.
            if top > i32::from(height) + 256 {
                return Err(FormatError::Invalid("post top"));
            }
        }
        prev_top = top;
        posts.push(PatchPost {
            top: top.clamp(0, 65535) as u16,
            pixels,
        });
    }
    Ok(PatchColumn { posts })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn empty_1x1_patch() {
        // width=1 height=1 offsets=0, one column ending immediately
        let mut data = vec![1, 0, 1, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0xFF];
        let p = parse_patch(&data).unwrap();
        assert_eq!(p.width, 1);
        assert_eq!(p.columns[0].posts.len(), 0);
        data[0] = 0;
        assert!(parse_patch(&data).is_err());
    }
}
