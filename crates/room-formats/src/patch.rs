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

/// Longest post [`encode_patch`] writes (runs are split; vanilla-safe length).
const MAX_POST: usize = 128;

/// Encode row-major palette `indices` as a Doom picture lump.
///
/// `alpha` marks holes (`0` = transparent); pass an empty slice for a fully
/// opaque picture. Pictures taller than 254 pixels use DeePsea relative post
/// offsets, which [`parse_patch`] reads back.
pub fn encode_patch(
    width: u16,
    height: u16,
    left_offset: i16,
    top_offset: i16,
    indices: &[u8],
    alpha: &[u8],
) -> Result<Vec<u8>, FormatError> {
    if !(1..=2048).contains(&width) || !(1..=2048).contains(&height) {
        return Err(FormatError::Invalid("patch dimensions"));
    }
    let w = usize::from(width);
    let h = usize::from(height);
    let n = w * h;
    if indices.len() != n || (!alpha.is_empty() && alpha.len() != n) {
        return Err(FormatError::Invalid("patch pixel count"));
    }
    let opaque = |i: usize| alpha.is_empty() || alpha[i] != 0;
    let header = 8 + w * 4;
    let mut out = Vec::with_capacity(header + n + w * 8);
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&left_offset.to_le_bytes());
    out.extend_from_slice(&top_offset.to_le_bytes());
    out.resize(header, 0);
    let mut column = Vec::with_capacity(h);
    for x in 0..w {
        let off = u32::try_from(out.len()).map_err(|_| FormatError::Invalid("patch too large"))?;
        out[8 + x * 4..12 + x * 4].copy_from_slice(&off.to_le_bytes());
        let mut prev = None;
        let mut y = 0;
        while y < h {
            if !opaque(y * w + x) {
                y += 1;
                continue;
            }
            let start = y;
            column.clear();
            while y < h && opaque(y * w + x) && y - start < MAX_POST {
                column.push(indices[y * w + x]);
                y += 1;
            }
            let delta = post_delta(&mut out, &mut prev, start);
            let first = column[0];
            let last = column[column.len() - 1];
            #[allow(clippy::cast_possible_truncation)]
            out.extend_from_slice(&[delta, column.len() as u8, first]);
            out.extend_from_slice(&column);
            out.push(last);
        }
        out.push(0xFF);
    }
    Ok(out)
}

/// Topdelta byte placing the next post at absolute row `top`, emitting empty
/// filler posts when `top` is not reachable in one absolute / relative step.
fn post_delta(out: &mut Vec<u8>, prev: &mut Option<usize>, top: usize) -> u8 {
    loop {
        if top < 255 && prev.is_none_or(|p| top > p) {
            *prev = Some(top);
            #[allow(clippy::cast_possible_truncation)]
            return top as u8;
        }
        if let Some(p) = *prev {
            let d = top - p;
            if d > 0 && d <= p && d < 255 {
                *prev = Some(top);
                #[allow(clippy::cast_possible_truncation)]
                return d as u8;
            }
        }
        let (next, byte) = match *prev {
            Some(p) if p >= 254 => (p + 254, 254),
            _ => (254, 254),
        };
        out.extend_from_slice(&[byte, 0, 0, 0]);
        *prev = Some(next);
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

    /// Deterministic pseudo-random image with holes (`alpha` 0) in runs.
    fn sample(w: usize, h: usize, seed: u32) -> (Vec<u8>, Vec<u8>) {
        let mut s = seed;
        let mut next = || {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (s >> 16) as u8
        };
        let indices: Vec<u8> = (0..w * h).map(|_| next()).collect();
        let mut alpha = vec![255u8; w * h];
        for x in 0..w {
            let mut y = 0;
            while y < h {
                let run = usize::from(next() % 90) + 1;
                if next() % 3 == 0 {
                    for yy in y..(y + run).min(h) {
                        alpha[yy * w + x] = 0;
                    }
                }
                y += run;
            }
        }
        (indices, alpha)
    }

    fn assert_roundtrip(w: u16, h: u16, indices: &[u8], alpha: &[u8]) {
        let bytes = encode_patch(w, h, -3, 7, indices, alpha).unwrap();
        let p = parse_patch(&bytes).unwrap();
        assert_eq!(
            (p.width, p.height, p.left_offset, p.top_offset),
            (w, h, -3, 7)
        );
        let (got_i, got_a) = p.to_indices_masked();
        for i in 0..indices.len() {
            let want_a = if alpha.is_empty() || alpha[i] != 0 {
                255
            } else {
                0
            };
            assert_eq!(got_a[i], want_a, "alpha at {i}");
            if want_a == 255 {
                assert_eq!(got_i[i], indices[i], "index at {i}");
            }
        }
    }

    #[test]
    fn encode_roundtrips_opaque_and_holes() {
        let (i, a) = sample(64, 128, 1);
        assert_roundtrip(64, 128, &i, &a);
        assert_roundtrip(64, 128, &i, &[]);
        // Palette index 0 stays a real colour when opaque.
        assert_roundtrip(2, 2, &[0, 0, 0, 0], &[]);
    }

    #[test]
    fn encode_roundtrips_tall_patches() {
        for (h, seed) in [(255u16, 2), (300, 3), (600, 4), (1024, 5)] {
            let (i, a) = sample(8, usize::from(h), seed);
            assert_roundtrip(8, h, &i, &a);
            assert_roundtrip(8, h, &i, &[]);
        }
        // Lone pixel far below a lone pixel near the top (filler posts needed).
        let h = 900usize;
        let mut a = vec![0u8; h];
        a[0] = 255;
        a[899] = 255;
        assert_roundtrip(1, 900, &vec![9u8; h], &a);
        // Only the bottom pixel.
        let mut a = vec![0u8; h];
        a[899] = 255;
        assert_roundtrip(1, 900, &vec![9u8; h], &a);
    }

    #[test]
    fn encode_rejects_bad_input() {
        assert!(encode_patch(0, 1, 0, 0, &[], &[]).is_err());
        assert!(encode_patch(2, 2, 0, 0, &[1, 2, 3], &[]).is_err());
        assert!(encode_patch(2, 2, 0, 0, &[1, 2, 3, 4], &[1]).is_err());
    }
}
