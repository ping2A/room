//! Doom patch encoder for generated graphics (`docs/31` MA3).

#![cfg(feature = "alloc")]

use alloc::vec::Vec;

const MAX_POST: usize = 128;

/// Encode a Doom patch (column posts). Transparent where `mask[i] == 0`
/// (or everywhere opaque when `mask` is empty). Dimensions capped at 256 for
/// mod-generated art (`lump_limits::MAX_GRAPHIC`).
///
/// # Errors
///
/// Bad dimensions or pixel count.
pub fn encode_patch(
    width: u16,
    height: u16,
    left_offset: i16,
    top_offset: i16,
    indices: &[u8],
    mask: &[u8],
) -> Result<Vec<u8>, &'static str> {
    if !(1..=256).contains(&width) || !(1..=256).contains(&height) {
        return Err("patch dimensions");
    }
    let w = usize::from(width);
    let h = usize::from(height);
    let n = w * h;
    if indices.len() != n || (!mask.is_empty() && mask.len() != n) {
        return Err("patch pixel count");
    }
    let opaque = |i: usize| mask.is_empty() || mask[i] != 0;
    let header = 8 + w * 4;
    let mut out = Vec::with_capacity(header + n + w * 8);
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&left_offset.to_le_bytes());
    out.extend_from_slice(&top_offset.to_le_bytes());
    out.resize(header, 0);
    let mut column = Vec::with_capacity(h);
    for x in 0..w {
        let off = u32::try_from(out.len()).map_err(|_| "patch too large")?;
        out[8 + x * 4..12 + x * 4].copy_from_slice(&off.to_le_bytes());
        let mut prev: Option<usize> = None;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_2x2() {
        let p = encode_patch(2, 2, 0, 0, &[1, 2, 3, 4], &[]).unwrap();
        assert_eq!(&p[0..2], &2u16.to_le_bytes());
        assert_eq!(&p[2..4], &2u16.to_le_bytes());
        assert!(p.len() > 16);
    }
}
