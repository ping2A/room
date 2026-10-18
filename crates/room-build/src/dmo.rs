//! Duke Nukem 3D `.DMO` demo headers and tic streams.
//!
//! Retail bodies use Ken Silverman's Build LZW (`kdfread` / `uncompress`):
//! a `u16` length, then a blob whose first two words are uncompressed size
//! and string-table count. Records after the first in a stream are byte
//! deltas from the previous tic. A raw 10-byte 1.3D stream is also accepted.

use crate::error::BuildError;

/// One player's input for one Duke tic (1.3D / 1.5 layout).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DmoInput {
    /// Turn speed (positive = right).
    pub turn: i8,
    /// Horizon / mouse-aim pitch.
    pub horizon: i8,
    /// Forward (`fvel`). Walk is about 1280.
    pub forward: i16,
    /// Strafe (`svel`).
    pub strafe: i16,
    /// Duke action bits (jump, fire, open, …).
    pub bits: u32,
}

/// Parsed Duke demo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmoDemo {
    /// Volume, 0-based (`E1` = 0).
    pub volume: u8,
    /// Level, 0-based (`L1` = 0).
    pub level: u8,
    /// Skill 0..=4.
    pub skill: u8,
    /// Players in the recording (1..=8).
    pub players: u8,
    /// Player-0 inputs. Empty when the body is still LZW-compressed.
    pub tics: Vec<DmoInput>,
}

impl DmoDemo {
    /// Room map lump (`E1M1` …).
    #[must_use]
    pub fn map_name(&self) -> String {
        format!(
            "E{}M{}",
            self.volume.saturating_add(1),
            self.level.saturating_add(1)
        )
    }

    /// Parse a `.DMO` header and, when the body is raw 1.3D tics, the stream.
    pub fn parse(bytes: &[u8]) -> Result<Self, BuildError> {
        if bytes.len() < 9 {
            return Err(BuildError::Truncated("DMO"));
        }
        let (volume, level, skill, players, body) =
            if bytes.len() >= 0x2A2 && matches!(bytes[4], 116 | 117) {
                let players = u16::from_le_bytes([bytes[0x0A], bytes[0x0B]]).clamp(1, 16) as u8;
                let volume = bytes[5];
                let level = bytes[6];
                let skill = bytes[7];
                let start = 0x2A2 + usize::from(players);
                let body = bytes.get(start..).unwrap_or(&[]);
                (volume, level, skill, players.max(1), body)
            } else if bytes.len() >= 24 && bytes[4] <= 3 {
                let players = u16::from_le_bytes([bytes[8], bytes[9]]).clamp(1, 8) as u8;
                (bytes[4], bytes[5], bytes[6], players.max(1), &bytes[24..])
            } else {
                let players = u16::from_le_bytes([bytes[7], bytes[8]]).clamp(1, 8) as u8;
                (bytes[4], bytes[5], bytes[6], players.max(1), &bytes[9..])
            };
        if volume > 3 || level > 15 {
            return Err(BuildError::Invalid("DMO map"));
        }
        let tics = parse_lzw_tics(body, players).unwrap_or_else(|| parse_raw_tics(body, players));
        Ok(Self {
            volume,
            level,
            skill,
            players,
            tics,
        })
    }
}

/// Uncompressed 1.3D records: 10 bytes per player per tic.
fn parse_raw_tics(body: &[u8], players: u8) -> Vec<DmoInput> {
    let stride = 10 * usize::from(players.max(1));
    if stride == 0 || body.len() < 10 || body.len() % stride != 0 {
        return Vec::new();
    }
    body.chunks_exact(stride)
        .map(|chunk| DmoInput {
            turn: chunk[0] as i8,
            horizon: chunk[1] as i8,
            forward: i16::from_le_bytes([chunk[2], chunk[3]]),
            strafe: i16::from_le_bytes([chunk[4], chunk[5]]),
            bits: u32::from_le_bytes([chunk[6], chunk[7], chunk[8], chunk[9]]),
        })
        .collect()
}

/// Ken `kdfread`: `u16` blob length, LZW payload, then wrapping byte deltas.
fn parse_lzw_tics(body: &[u8], players: u8) -> Option<Vec<DmoInput>> {
    let stride = 10 * usize::from(players.max(1));
    if body.len() < 6 {
        return None;
    }
    let leng0 = u16::from_le_bytes([body[0], body[1]]) as usize;
    if leng0 < 4 || 2 + leng0 > body.len() {
        return None;
    }
    let mut raw = Vec::new();
    let mut o = 0;
    while o + 2 <= body.len() {
        let leng = u16::from_le_bytes([body[o], body[o + 1]]) as usize;
        o += 2;
        if leng < 4 || o + leng > body.len() {
            break;
        }
        let Some(chunk) = ken_uncompress(&body[o..o + leng]) else {
            break;
        };
        o += leng;
        raw.extend_from_slice(&chunk);
        if raw.len() > 1 << 22 {
            break;
        }
    }
    if raw.len() < stride {
        return None;
    }
    let n = raw.len() / stride;
    raw.truncate(n * stride);
    for i in 1..n {
        let (head, tail) = raw.split_at_mut(i * stride);
        let prev = &head[(i - 1) * stride..];
        for j in 0..stride {
            tail[j] = tail[j].wrapping_add(prev[j]);
        }
    }
    Some(
        raw.chunks_exact(stride)
            .map(|chunk| DmoInput {
                turn: chunk[0] as i8,
                horizon: chunk[1] as i8,
                forward: i16::from_le_bytes([chunk[2], chunk[3]]),
                strafe: i16::from_le_bytes([chunk[4], chunk[5]]),
                bits: u32::from_le_bytes([chunk[6], chunk[7], chunk[8], chunk[9]]),
            })
            .collect(),
    )
}

/// One `kdfread(..., dasizeof, 1)` record (its own LZW blob).
pub(crate) fn kdfread_one(src: &[u8], pos: &mut usize, n: usize) -> Option<Vec<u8>> {
    if *pos + 2 > src.len() {
        return None;
    }
    let leng = u16::from_le_bytes([src[*pos], src[*pos + 1]]) as usize;
    *pos += 2;
    if leng < 4 || *pos + leng > src.len() {
        return None;
    }
    let chunk = ken_uncompress(&src[*pos..*pos + leng])?;
    *pos += leng;
    if chunk.len() < n {
        return None;
    }
    Some(chunk[..n].to_vec())
}

/// One Ken `kdfread` blob (full uncompressed payload).
pub(crate) fn kdfread_blob(src: &[u8], pos: &mut usize) -> Option<Vec<u8>> {
    if *pos + 2 > src.len() {
        return None;
    }
    let leng = u16::from_le_bytes([src[*pos], src[*pos + 1]]) as usize;
    *pos += 2;
    if leng < 4 || *pos + leng > src.len() {
        return None;
    }
    let chunk = ken_uncompress(&src[*pos..*pos + leng])?;
    *pos += leng;
    Some(chunk)
}

/// Advance past one Ken `kdfread` blob without keeping the payload.
pub(crate) fn kdfskip_one(src: &[u8], pos: &mut usize) -> bool {
    if *pos + 2 > src.len() {
        return false;
    }
    let leng = u16::from_le_bytes([src[*pos], src[*pos + 1]]) as usize;
    *pos += 2;
    if leng < 4 || *pos + leng > src.len() {
        return false;
    }
    *pos += leng;
    true
}

/// Stored-uncompressed Ken blob (strtot = 0) for tests and tiny headers.
#[cfg(test)]
pub(crate) fn kdfwrite_raw(data: &[u8]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(4 + data.len());
    blob.extend_from_slice(&(data.len() as u16).to_le_bytes());
    blob.extend_from_slice(&0u16.to_le_bytes());
    blob.extend_from_slice(data);
    let mut out = Vec::with_capacity(2 + blob.len());
    out.extend_from_slice(&(blob.len() as u16).to_le_bytes());
    out.extend_from_slice(&blob);
    out
}

/// Ken `uncompress` from the Build engine (`cache1d` / chocolate Duke).
pub(crate) fn ken_uncompress(src: &[u8]) -> Option<Vec<u8>> {
    if src.len() < 4 {
        return None;
    }
    let uncompleng = u16::from_le_bytes([src[0], src[1]]) as usize;
    let strtot = i32::from(u16::from_le_bytes([src[2], src[3]]));
    if uncompleng == 0 || uncompleng > 65_535 {
        return None;
    }
    if strtot == 0 {
        let n = uncompleng.min(src.len().saturating_sub(4));
        return Some(src[4..4 + n].to_vec());
    }
    if strtot < 256 {
        return None;
    }
    let cap = (strtot as usize).saturating_add(16).max(512);
    let mut buf1 = vec![0u8; cap];
    let mut buf2 = vec![0i16; cap];
    let mut buf3 = vec![0i16; cap];
    for i in 0..256 {
        buf2[i] = i as i16;
        buf3[i] = i as i16;
    }
    let mut padded = src.to_vec();
    padded.extend_from_slice(&[0, 0, 0, 0]);
    let mut currstr = 256i32;
    let mut bitcnt = 4 << 3;
    let mut out = Vec::with_capacity(uncompleng);
    let mut numbits = 8i32;
    let mut oneup = 1i32 << 8;
    while currstr < strtot && out.len() < uncompleng {
        let i = (bitcnt >> 3) as usize;
        if i + 4 > padded.len() {
            break;
        }
        let word = u32::from_le_bytes([padded[i], padded[i + 1], padded[i + 2], padded[i + 3]]);
        let mut dat = ((word >> (bitcnt & 7)) as i32) & (oneup - 1);
        bitcnt += numbits;
        if (dat & ((oneup >> 1) - 1)) > ((currstr - 1) & ((oneup >> 1) - 1)) {
            dat &= (oneup >> 1) - 1;
            bitcnt -= 1;
        }
        if (currstr as usize) >= buf3.len() {
            break;
        }
        buf3[currstr as usize] = dat as i16;
        let mut leng = 0i32;
        let mut walk = dat;
        while walk >= 256 && (leng as usize) + 1 < buf1.len() {
            if (walk as usize) >= buf2.len() {
                break;
            }
            buf1[leng as usize] = buf2[walk as usize] as u8;
            walk = i32::from(buf3[walk as usize]);
            leng += 1;
        }
        if out.len() >= uncompleng {
            break;
        }
        out.push(walk as u8);
        for i in (0..leng).rev() {
            if out.len() >= uncompleng {
                break;
            }
            out.push(buf1[i as usize]);
        }
        if currstr > 0 {
            buf2[(currstr as usize) - 1] = walk as i16;
        }
        if (currstr as usize) < buf2.len() {
            buf2[currstr as usize] = walk as i16;
        }
        currstr += 1;
        if currstr > oneup {
            numbits += 1;
            oneup <<= 1;
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn v13_header(vol: u8, lev: u8, skill: u8, players: u16) -> Vec<u8> {
        let mut b = vec![0u8; 24];
        b[4] = vol;
        b[5] = lev;
        b[6] = skill;
        b[8] = players as u8;
        b[9] = (players >> 8) as u8;
        b
    }

    #[test]
    fn v13_header_names_e1m1() {
        let d = DmoDemo::parse(&v13_header(0, 0, 2, 1)).unwrap();
        assert_eq!(d.map_name(), "E1M1");
        assert_eq!(d.skill, 2);
        assert!(d.tics.is_empty());
    }

    #[test]
    fn v15_header_names_e2m3() {
        let mut b = vec![0u8; 0x2A2 + 1];
        b[4] = 117;
        b[5] = 1;
        b[6] = 2;
        b[7] = 3;
        b[0x0A] = 1;
        let d = DmoDemo::parse(&b).unwrap();
        assert_eq!(d.map_name(), "E2M3");
        assert_eq!(d.skill, 3);
    }

    #[test]
    fn raw_v13_tic_is_forward_and_fire() {
        let mut b = v13_header(0, 0, 1, 1);
        let mut tic = [0u8; 10];
        tic[0] = 4;
        tic[2] = 0x00;
        tic[3] = 0x05;
        tic[6] = 1 << 2;
        b.extend_from_slice(&tic);
        let d = DmoDemo::parse(&b).unwrap();
        assert_eq!(d.tics.len(), 1);
        assert_eq!(d.tics[0].forward, 1280);
        assert_eq!(d.tics[0].bits & (1 << 2), 1 << 2);
        assert_eq!(d.tics[0].turn, 4);
    }

    #[test]
    fn lzw_looking_body_stays_header_only() {
        let mut b = v13_header(0, 1, 0, 1);
        b.extend_from_slice(&[8, 0, 20, 0, 9, 0, 1]);
        let d = DmoDemo::parse(&b).unwrap();
        assert_eq!(d.map_name(), "E1M2");
        assert!(d.tics.is_empty());
    }
}
