//! DeluxePaint Animator `.ANM` (LPF + ANIM, RunSkipDump).
//!
//! Duke ships `LOGO.ANM` and a few episode cutscenes in this 1989 Large Page
//! format. Frame 0 is a full 320×200 image; later frames are deltas.

use crate::error::BuildError;

/// One decoded `.ANM` frame: size, row-major indices, and a 256-colour RGB palette.
#[derive(Debug, Clone)]
pub struct AnmFrame {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// `width * height` palette indices, row-major.
    pub indices: Vec<u8>,
    /// 256 RGB triples (768 bytes).
    pub rgb: [u8; 768],
}

const HEADER_LEN: usize = 128;
const PAD_AFTER_HEADER: usize = 128;
const PALETTE_LEN: usize = 256 * 4;
const MAX_LPS: usize = 256;
const LP_DESC: usize = 6;
const PAGE_BASE: usize = 0xB00;
const PAGE_SIZE: usize = 0x10000;

/// Every still of an `.ANM` (no ring frame), plus the file's playback rate.
#[derive(Debug, Clone)]
pub struct AnmAnim {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Frames per second from the header (`0` treated as 10).
    pub fps: u16,
    /// 256 RGB triples (768 bytes).
    pub rgb: [u8; 768],
    /// Row-major index buffers, one per still.
    pub frames: Vec<Vec<u8>>,
}

struct Anm<'a> {
    data: &'a [u8],
    width: u16,
    height: u16,
    n_records: u32,
    last_delta_valid: bool,
    fps: u16,
    rgb: [u8; 768],
    pages: Vec<(u16, u16, u16)>,
}

/// Held title still: every frame up to the last one before a ring delta.
pub fn title_frame(data: &[u8]) -> Result<AnmFrame, BuildError> {
    let anim = decode_stills(data)?;
    let indices = anim
        .frames
        .last()
        .cloned()
        .ok_or(BuildError::Invalid("ANM has no frames"))?;
    Ok(AnmFrame {
        width: anim.width,
        height: anim.height,
        indices,
        rgb: anim.rgb,
    })
}

/// Every still up to the last frame before a ring delta.
pub fn decode_stills(data: &[u8]) -> Result<AnmAnim, BuildError> {
    let anim = Anm::load(data)?;
    let last = if anim.last_delta_valid {
        anim.n_records.saturating_sub(2)
    } else {
        anim.n_records.saturating_sub(1)
    };
    let pixels = usize::from(anim.width) * usize::from(anim.height);
    let mut buf = vec![0u8; pixels];
    let mut frames = Vec::with_capacity(last as usize + 1);
    for frame in 0..=last {
        anim.blit(frame as u16, &mut buf)?;
        frames.push(buf.clone());
    }
    Ok(AnmAnim {
        width: anim.width,
        height: anim.height,
        fps: anim.fps,
        rgb: anim.rgb,
        frames,
    })
}

impl<'a> Anm<'a> {
    fn load(data: &'a [u8]) -> Result<Self, BuildError> {
        if data.len() < HEADER_LEN + PAD_AFTER_HEADER + PALETTE_LEN {
            return Err(BuildError::Truncated("ANM header"));
        }
        if &data[0..4] != b"LPF " {
            return Err(BuildError::Invalid("ANM magic"));
        }
        if &data[16..20] != b"ANIM" {
            return Err(BuildError::Invalid("ANM content type"));
        }
        let n_lps = u16::from_le_bytes([data[6], data[7]]) as usize;
        let n_records = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let width = u16::from_le_bytes([data[20], data[21]]);
        let height = u16::from_le_bytes([data[22], data[23]]);
        let last_delta_valid = data[27] != 0;
        let fps = u16::from_le_bytes([data[68], data[69]]).max(1);
        if n_lps == 0 || n_lps > MAX_LPS {
            return Err(BuildError::Invalid("ANM page count"));
        }
        if n_records == 0 || n_records > 4096 {
            return Err(BuildError::Invalid("ANM frame count"));
        }
        if !(1..=2048).contains(&width) || !(1..=2048).contains(&height) {
            return Err(BuildError::Invalid("ANM dimensions"));
        }

        let pal_off = HEADER_LEN + PAD_AFTER_HEADER;
        let mut rgb = [0u8; 768];
        for i in 0..256 {
            let src = &data[pal_off + i * 4..pal_off + i * 4 + 3];
            // File order is B, G, R, pad.
            rgb[i * 3] = src[2];
            rgb[i * 3 + 1] = src[1];
            rgb[i * 3 + 2] = src[0];
        }

        let desc_off = pal_off + PALETTE_LEN;
        let desc_end = desc_off + n_lps * LP_DESC;
        if data.len() < desc_end {
            return Err(BuildError::Truncated("ANM page table"));
        }
        let mut pages = Vec::with_capacity(n_lps);
        for i in 0..n_lps {
            let d = &data[desc_off + i * LP_DESC..];
            pages.push((
                u16::from_le_bytes([d[0], d[1]]),
                u16::from_le_bytes([d[2], d[3]]) & 0x3FFF,
                u16::from_le_bytes([d[4], d[5]]),
            ));
        }
        Ok(Self {
            data,
            width,
            height,
            n_records,
            last_delta_valid,
            fps,
            rgb,
            pages,
        })
    }

    fn blit(&self, frame: u16, dst: &mut [u8]) -> Result<(), BuildError> {
        let page_i = self
            .pages
            .iter()
            .position(|&(base, recs, _)| {
                recs > 0 && frame >= base && frame < base.saturating_add(recs)
            })
            .ok_or(BuildError::Invalid("ANM frame page"))?;
        let (base, n_recs, n_bytes) = self.pages[page_i];
        let page_off = PAGE_BASE.saturating_add(page_i.saturating_mul(PAGE_SIZE));
        let sizes_off = page_off
            .checked_add(LP_DESC + 2)
            .ok_or(BuildError::Invalid("ANM page offset"))?;
        let recs = usize::from(n_recs);
        let sizes_bytes = recs
            .checked_mul(2)
            .ok_or(BuildError::Invalid("ANM record table"))?;
        if self.data.len() < sizes_off.saturating_add(sizes_bytes).saturating_add(4) {
            return Err(BuildError::Truncated("ANM page"));
        }

        let local = frame.saturating_sub(base);
        let mut offset = 0u32;
        for f in 0..usize::from(local) {
            let s = &self.data[sizes_off + f * 2..];
            offset = offset.saturating_add(u32::from(u16::from_le_bytes([s[0], s[1]])));
        }
        if offset > u32::from(n_bytes) {
            return Err(BuildError::Invalid("ANM record offset"));
        }

        let mut src_off = sizes_off
            .checked_add(sizes_bytes)
            .and_then(|o| o.checked_add(4))
            .and_then(|o| o.checked_add(offset as usize))
            .ok_or(BuildError::Invalid("ANM record"))?;
        let prefix = src_off.saturating_sub(4);
        if self.data.get(prefix + 1).copied().unwrap_or(0) != 0 {
            let temp = u16::from_le_bytes([
                self.data.get(prefix + 2).copied().unwrap_or(0),
                self.data.get(prefix + 3).copied().unwrap_or(0),
            ]);
            src_off = src_off
                .checked_add(usize::from(temp + (temp & 1)))
                .ok_or(BuildError::Invalid("ANM extra"))?;
        }
        if src_off >= self.data.len() {
            return Err(BuildError::Truncated("ANM frame"));
        }
        decode_ops(&self.data[src_off..], dst)
    }
}

fn decode_ops(mut src: &[u8], dst: &mut [u8]) -> Result<(), BuildError> {
    let mut i = 0usize;
    loop {
        let count = *src.first().ok_or(BuildError::Truncated("ANM opcode"))?;
        src = &src[1..];
        if count == 0 {
            let (len, rest) = src.split_first().ok_or(BuildError::Truncated("ANM RLE"))?;
            let (color, rest) = rest
                .split_first()
                .ok_or(BuildError::Truncated("ANM RLE colour"))?;
            src = rest;
            fill(dst, &mut i, usize::from(*len), *color)?;
            continue;
        }
        if count & 0x80 == 0 {
            take_copy(&mut src, dst, &mut i, usize::from(count))?;
            continue;
        }
        let skip = usize::from(count & 0x7F);
        if skip > 0 {
            i = i
                .checked_add(skip)
                .filter(|&n| n <= dst.len())
                .ok_or(BuildError::Invalid("ANM skip"))?;
            continue;
        }
        if src.len() < 2 {
            return Err(BuildError::Truncated("ANM long op"));
        }
        let long = u16::from_le_bytes([src[0], src[1]]);
        src = &src[2..];
        if long == 0 {
            return Ok(());
        }
        if long & 0x8000 == 0 {
            i = i
                .checked_add(usize::from(long))
                .filter(|&n| n <= dst.len())
                .ok_or(BuildError::Invalid("ANM long skip"))?;
            continue;
        }
        let long = long & 0x7FFF;
        if long & 0x4000 != 0 {
            let color = *src.first().ok_or(BuildError::Truncated("ANM long RLE"))?;
            src = &src[1..];
            fill(dst, &mut i, usize::from(long & 0x3FFF), color)?;
            continue;
        }
        take_copy(&mut src, dst, &mut i, usize::from(long))?;
    }
}

fn fill(dst: &mut [u8], i: &mut usize, n: usize, color: u8) -> Result<(), BuildError> {
    let end = i
        .checked_add(n)
        .filter(|&e| e <= dst.len())
        .ok_or(BuildError::Invalid("ANM write"))?;
    dst[*i..end].fill(color);
    *i = end;
    Ok(())
}

fn take_copy(src: &mut &[u8], dst: &mut [u8], i: &mut usize, n: usize) -> Result<(), BuildError> {
    if src.len() < n {
        return Err(BuildError::Truncated("ANM copy"));
    }
    let end = i
        .checked_add(n)
        .filter(|&e| e <= dst.len())
        .ok_or(BuildError::Invalid("ANM write"))?;
    dst[*i..end].copy_from_slice(&src[..n]);
    *src = &src[n..];
    *i = end;
    Ok(())
}

/// Minimal 8×1 ANM: three copied pixels, then stop.
#[cfg(test)]
pub(crate) fn tiny_anm() -> Vec<u8> {
    // Short copy of 3 pixels, then a long-op stop (0x80, 0, 0).
    let decode = [3u8, 10, 20, 30, 0x80, 0, 0];
    let page_body = {
        let mut p = vec![0u8; LP_DESC + 2 + 2 + 4];
        p.extend_from_slice(&decode);
        p[LP_DESC + 2] = decode.len() as u8;
        p
    };
    let mut data = vec![0u8; PAGE_BASE + page_body.len()];
    data[0..4].copy_from_slice(b"LPF ");
    data[6..8].copy_from_slice(&1u16.to_le_bytes());
    data[8..12].copy_from_slice(&1u32.to_le_bytes());
    data[14..16].copy_from_slice(&1280u16.to_le_bytes());
    data[16..20].copy_from_slice(b"ANIM");
    data[20..22].copy_from_slice(&8u16.to_le_bytes());
    data[22..24].copy_from_slice(&1u16.to_le_bytes());
    data[64..68].copy_from_slice(&1u32.to_le_bytes());
    let pal = HEADER_LEN + PAD_AFTER_HEADER;
    data[pal + 10 * 4] = 200;
    data[pal + 10 * 4 + 1] = 0;
    data[pal + 10 * 4 + 2] = 0;
    data[pal + 20 * 4] = 0;
    data[pal + 20 * 4 + 1] = 200;
    data[pal + 20 * 4 + 2] = 0;
    data[pal + 30 * 4] = 0;
    data[pal + 30 * 4 + 1] = 0;
    data[pal + 30 * 4 + 2] = 200;
    let desc = pal + PALETTE_LEN;
    data[desc..desc + 2].copy_from_slice(&0u16.to_le_bytes());
    data[desc + 2..desc + 4].copy_from_slice(&1u16.to_le_bytes());
    data[desc + 4..desc + 6].copy_from_slice(&(page_body.len() as u16).to_le_bytes());
    data[PAGE_BASE..PAGE_BASE + page_body.len()].copy_from_slice(&page_body);
    data
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn title_frame_is_a_full_image() {
        let frame = title_frame(&tiny_anm()).unwrap();
        assert_eq!(frame.width, 8);
        assert_eq!(frame.height, 1);
        assert_eq!(&frame.indices[..3], &[10, 20, 30]);
        assert_eq!(&frame.rgb[10 * 3..10 * 3 + 3], &[0, 0, 200]);
        assert_eq!(&frame.rgb[20 * 3..20 * 3 + 3], &[0, 200, 0]);
        assert_eq!(&frame.rgb[30 * 3..30 * 3 + 3], &[200, 0, 0]);
        let stills = decode_stills(&tiny_anm()).unwrap();
        assert_eq!(stills.frames.len(), 1);
        assert_eq!(stills.frames[0], frame.indices);
    }

    #[test]
    fn rejects_junk() {
        assert!(title_frame(b"nope").is_err());
        assert!(title_frame(&[0; 200]).is_err());
    }
}
