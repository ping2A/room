//! Duke Nukem 3D `.DMO` demo headers and tic streams.
//!
//! Retail bodies use Ken Silverman's Build LZW (`kdfread` / `uncompress`):
//! a `u16` length, then a blob whose first two words are uncompressed size
//! and string-table count. Records after the first in a stream are byte
//! deltas from the previous tic. A raw 10-byte 1.3D stream is also accepted.

use crate::error::BuildError;

/// Maximum size accepted for a newly encoded Atomic demo.
const MAX_ATOMIC_DMO_BYTES: usize = 64 * 1024 * 1024;
/// Atomic Edition 1.5 demo revision (`BYTEVERSION_117`).
const ATOMIC_15_DEMO_VERSION: u8 = 117;
/// Original player input size from `duke3d.h` (`#pragma pack(1)`).
const DUKE_INPUT_BYTES: usize = 10;
/// Atomic 1.5 stores one aim-mode byte and ten weapon-choice bytes per player
/// between the fixed header and compressed tic array.
const ATOMIC_15_PLAYER_HEADER_BYTES: usize = 11;

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
    /// Atomic `ud.coop` game mode (0 = DukeMatch, 1 = co-op).
    pub coop: u8,
    /// Atomic friendly-fire option (`ud.ffire`).
    pub friendly_fire: u8,
    /// Players in the recording (1..=8).
    pub players: u8,
    /// Player-0 inputs, retained as a compatibility view.
    pub tics: Vec<DmoInput>,
    /// Inputs for each player, ordered by seat. `tics` is player zero's
    /// compatibility view.
    pub player_tics: Vec<Vec<DmoInput>>,
    /// Atomic per-player aim mode from the v117 header, when present.
    pub aim_modes: Vec<u8>,
    /// Atomic per-player ten-slot weapon preference table, when present.
    pub weapon_choices: Vec<[u8; 10]>,
}

impl DmoDemo {
    /// Room map lump (`E1L1` …).
    #[must_use]
    pub fn map_name(&self) -> String {
        format!(
            "E{}L{}",
            self.volume.saturating_add(1),
            self.level.saturating_add(1)
        )
    }

    /// Parse a `.DMO` header and, when the body is raw 1.3D tics, the stream.
    pub fn parse(bytes: &[u8]) -> Result<Self, BuildError> {
        if bytes.len() < 9 {
            return Err(BuildError::Truncated("DMO"));
        }
        let (volume, level, skill, coop, friendly_fire, players, aim_modes, weapon_choices, body) =
            if bytes.len() >= 0x2A2 && matches!(bytes[4], 116 | 117) {
                let player_count = u16::from_le_bytes([bytes[0x0A], bytes[0x0B]]);
                if !(1..=8).contains(&player_count) {
                    return Err(BuildError::Invalid("DMO player count"));
                }
                let players = player_count as u8;
                // v116 stores one aim-mode byte per player; v117 adds the
                // ten-slot weapon-choice table (`ud.wchoice`).
                let per_player = if bytes[4] == ATOMIC_15_DEMO_VERSION {
                    ATOMIC_15_PLAYER_HEADER_BYTES
                } else {
                    1
                };
                let volume = bytes[5];
                let level = bytes[6];
                let skill = bytes[7];
                let player_header = usize::from(players)
                    .checked_mul(per_player)
                    .ok_or(BuildError::Invalid("DMO player header length"))?;
                let start = 0x2A2usize
                    .checked_add(player_header)
                    .ok_or(BuildError::Invalid("DMO player header length"))?;
                if bytes.len() < start {
                    return Err(BuildError::Truncated("DMO player header"));
                }
                let body = bytes.get(start..).unwrap_or(&[]);
                let mut aim_modes = Vec::with_capacity(usize::from(players));
                let mut weapon_choices = Vec::with_capacity(usize::from(players));
                for seat in 0..usize::from(players) {
                    let offset = 0x2A2 + seat * per_player;
                    aim_modes.push(bytes[offset]);
                    if per_player == ATOMIC_15_PLAYER_HEADER_BYTES {
                        let mut choices = [0u8; 10];
                        choices.copy_from_slice(&bytes[offset + 1..offset + 11]);
                        weapon_choices.push(choices);
                    }
                }
                (
                    volume,
                    level,
                    skill,
                    bytes[8],
                    bytes[9],
                    players.max(1),
                    aim_modes,
                    weapon_choices,
                    body,
                )
            } else if bytes.len() >= 24 && bytes[4] <= 3 {
                let players = u16::from_le_bytes([bytes[8], bytes[9]]).clamp(1, 8) as u8;
                (
                    bytes[4],
                    bytes[5],
                    bytes[6],
                    0,
                    0,
                    players.max(1),
                    Vec::new(),
                    Vec::new(),
                    &bytes[24..],
                )
            } else {
                let players = u16::from_le_bytes([bytes[7], bytes[8]]).clamp(1, 8) as u8;
                (
                    bytes[4],
                    bytes[5],
                    bytes[6],
                    0,
                    0,
                    players.max(1),
                    Vec::new(),
                    Vec::new(),
                    &bytes[9..],
                )
            };
        if volume > 3 || level > 15 {
            return Err(BuildError::Invalid("DMO map"));
        }
        let player_tics =
            parse_lzw_tics(body, players).unwrap_or_else(|| parse_raw_tics(body, players));
        let tics = player_tics.first().cloned().unwrap_or_default();
        Ok(Self {
            volume,
            level,
            skill,
            coop,
            friendly_fire,
            players,
            tics,
            player_tics,
            aim_modes,
            weapon_choices,
        })
    }

    /// Encode an Atomic Edition 1.5 `.DMO` stream for one or more seats.
    ///
    /// The header follows Chocolate `opendemowrite`'s v117 layout. Input
    /// records use Build's 10-byte packed `input` shape; their bytes are delta
    /// encoded as `dfwrite` expects, then wrapped in raw Ken LZW chunks
    /// (`strtot == 0`, accepted by the reference `uncompress`). The Atomic
    /// CRC-bearing v119 header is deliberately not emitted.
    ///
    /// For multiple players, `player_tics` must contain one equally sized
    /// stream per player. Missing aim-mode and weapon-choice entries are
    /// written as zero defaults.
    pub fn encode_atomic_15(&self, monsters_off: bool) -> Result<Vec<u8>, BuildError> {
        if self.volume > 3
            || self.level > 15
            || self.skill > 4
            || self.coop > 2
            || self.friendly_fire > 1
            || self.players == 0
            || self.players > 8
        {
            return Err(BuildError::Invalid("Atomic 1.5 DMO metadata"));
        }
        let streams = if self.player_tics.len() == usize::from(self.players) {
            self.player_tics.as_slice()
        } else if self.players == 1 && self.player_tics.is_empty() {
            std::slice::from_ref(&self.tics)
        } else {
            return Err(BuildError::Invalid("Atomic 1.5 DMO player streams"));
        };
        let tic_count = streams.first().map_or(0, Vec::len);
        if streams.iter().any(|stream| stream.len() != tic_count) {
            return Err(BuildError::Invalid("Atomic 1.5 DMO player tic count"));
        }
        let record_size = DUKE_INPUT_BYTES
            .checked_mul(usize::from(self.players))
            .ok_or(BuildError::Invalid("Atomic 1.5 DMO record size"))?;
        let record_bytes = tic_count
            .checked_mul(record_size)
            .ok_or(BuildError::Invalid("Atomic 1.5 DMO length"))?;
        let max_tics = i32::MAX as usize;
        if tic_count > max_tics {
            return Err(BuildError::Invalid("Atomic 1.5 DMO tic count"));
        }

        let player_header = usize::from(self.players)
            .checked_mul(ATOMIC_15_PLAYER_HEADER_BYTES)
            .ok_or(BuildError::Invalid("Atomic 1.5 DMO player header"))?;
        let mut out = vec![0u8; 0x2A2 + player_header];
        out[0..4].copy_from_slice(&(tic_count as i32).to_le_bytes());
        out[4] = ATOMIC_15_DEMO_VERSION;
        out[5] = self.volume;
        out[6] = self.level;
        out[7] = self.skill;
        out[8] = self.coop; // `ud.m_coop`
        out[9] = self.friendly_fire; // `ud.m_ffire`
        out[10..12].copy_from_slice(&u16::from(self.players).to_le_bytes()); // `ud.multimode`
        out[12..14].copy_from_slice(&u16::from(monsters_off).to_le_bytes());
        if (!self.aim_modes.is_empty() && self.aim_modes.len() != usize::from(self.players))
            || (!self.weapon_choices.is_empty()
                && self.weapon_choices.len() != usize::from(self.players))
        {
            return Err(BuildError::Invalid("Atomic 1.5 DMO player metadata"));
        }
        for seat in 0..usize::from(self.players) {
            let offset = 0x2A2 + seat * ATOMIC_15_PLAYER_HEADER_BYTES;
            out[offset] = self.aim_modes.get(seat).copied().unwrap_or_default();
            if let Some(choices) = self.weapon_choices.get(seat) {
                out[offset + 1..offset + 11].copy_from_slice(choices);
            }
        }

        if record_bytes == 0 {
            return Ok(out);
        }
        let mut raw = Vec::with_capacity(record_bytes);
        for tic in 0..tic_count {
            for stream in streams {
                let input = stream[tic];
                raw.extend_from_slice(&[
                    input.turn as u8,
                    input.horizon as u8,
                    input.forward as u8,
                    (input.forward >> 8) as u8,
                    input.strafe as u8,
                    (input.strafe >> 8) as u8,
                    input.bits as u8,
                    (input.bits >> 8) as u8,
                    (input.bits >> 16) as u8,
                    (input.bits >> 24) as u8,
                ]);
            }
        }
        let blocks = kdfwrite_array_raw(&raw, record_size, tic_count)
            .ok_or(BuildError::Invalid("Atomic 1.5 DMO chunk encoding"))?;
        out.extend_from_slice(&blocks);
        if out.len() > MAX_ATOMIC_DMO_BYTES {
            return Err(BuildError::Invalid("Atomic 1.5 DMO exceeds size limit"));
        }
        Ok(out)
    }
}

/// Uncompressed 1.3D records: 10 bytes per player per tic.
fn parse_raw_tics(body: &[u8], players: u8) -> Vec<Vec<DmoInput>> {
    let stride = 10 * usize::from(players.max(1));
    if stride == 0 || body.len() < 10 || body.len() % stride != 0 {
        return vec![Vec::new(); usize::from(players.max(1))];
    }
    let mut streams = vec![Vec::new(); usize::from(players.max(1))];
    for record in body.chunks_exact(stride) {
        for (seat, bytes) in record.chunks_exact(DUKE_INPUT_BYTES).enumerate() {
            streams[seat].push(decode_dmo_input(bytes));
        }
    }
    streams
}

/// Ken `kdfread`: `u16` blob length, LZW payload, then wrapping byte deltas.
fn parse_lzw_tics(body: &[u8], players: u8) -> Option<Vec<Vec<DmoInput>>> {
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
    let mut streams = vec![Vec::with_capacity(n); usize::from(players.max(1))];
    for record in raw.chunks_exact(stride) {
        for (seat, bytes) in record.chunks_exact(DUKE_INPUT_BYTES).enumerate() {
            streams[seat].push(decode_dmo_input(bytes));
        }
    }
    Some(streams)
}

fn decode_dmo_input(bytes: &[u8]) -> DmoInput {
    DmoInput {
        turn: bytes[0] as i8,
        horizon: bytes[1] as i8,
        forward: i16::from_le_bytes([bytes[2], bytes[3]]),
        strafe: i16::from_le_bytes([bytes[4], bytes[5]]),
        bits: u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]),
    }
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

/// Read one complete `kdfread` array, consuming every compressed block and
/// reconstructing Build's element-wise byte deltas.
pub(crate) fn kdfread_array(
    src: &[u8],
    pos: &mut usize,
    element_size: usize,
    count: usize,
) -> Option<Vec<u8>> {
    if element_size == 0 {
        return None;
    }
    let total = element_size.checked_mul(count)?;
    if total == 0 {
        return Some(Vec::new());
    }
    let mut out = Vec::with_capacity(total);
    while out.len() < total {
        if pos.checked_add(2)? > src.len() {
            return None;
        }
        let compressed_len = usize::from(u16::from_le_bytes([src[*pos], src[*pos + 1]]));
        *pos += 2;
        let end = pos.checked_add(compressed_len)?;
        let compressed = src.get(*pos..end)?;
        let deltas = ken_uncompress(compressed)?;
        if deltas.is_empty() || deltas.len() % element_size != 0 {
            return None;
        }
        let needed = total - out.len();
        if deltas.len() > needed {
            return None;
        }
        for delta in deltas {
            let value = if out.len() < element_size {
                delta
            } else {
                delta.wrapping_add(out[out.len() - element_size])
            };
            out.push(value);
        }
        *pos = end;
    }
    Some(out)
}

/// Advance past one Ken `kdfread` blob without keeping the payload.
#[allow(
    dead_code,
    reason = "kept for demo formats that skip compressed blocks"
)]
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

/// Encode the raw-block form accepted by Chocolate's `uncompress` (`strtot=0`).
/// The caller supplies bytes that are already delta-coded for `dfread`.
fn kdfwrite_raw_blob(data: &[u8], out: &mut Vec<u8>) -> Option<()> {
    let encoded_len = data.len().checked_add(4)?;
    let encoded_len = u16::try_from(encoded_len).ok()?;
    let block_len = usize::from(encoded_len);
    out.extend_from_slice(&encoded_len.to_le_bytes());
    out.extend_from_slice(&(data.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(data);
    debug_assert_eq!(block_len, data.len() + 4);
    Some(())
}

/// Encode Build's `dfwrite` byte-delta array using valid uncompressed Ken LZW
/// blocks. This is larger than Chocolate's compressed output, but has identical
/// `dfread` semantics and avoids introducing a second codec implementation.
///
/// `element_size` is the record stride passed to `dfwrite`; zero-sized records
/// and payloads that cannot fit the format's 16-bit block lengths are rejected.
pub(crate) fn kdfwrite_array_raw(
    data: &[u8],
    element_size: usize,
    count: usize,
) -> Option<Vec<u8>> {
    const LZW_SIZE: usize = 16_384;
    if element_size == 0 {
        return None;
    }
    let total = element_size.checked_mul(count)?;
    if data.len() != total {
        return None;
    }
    if total == 0 {
        return Some(Vec::new());
    }
    let stride = if element_size > LZW_SIZE {
        1
    } else {
        element_size
    };
    let records = total / stride;
    let mut deltas = Vec::with_capacity(total);
    deltas.extend_from_slice(&data[..stride]);
    for i in 1..records {
        let current = &data[i * stride..(i + 1) * stride];
        let previous = &data[(i - 1) * stride..i * stride];
        deltas.extend(
            current
                .iter()
                .zip(previous)
                .map(|(a, b)| a.wrapping_sub(*b)),
        );
    }

    let records_per_block = (LZW_SIZE / stride).max(1);
    let mut out = Vec::new();
    for block in deltas.chunks(records_per_block * stride) {
        kdfwrite_raw_blob(block, &mut out)?;
    }
    Some(out)
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
        assert_eq!(d.map_name(), "E1L1");
        assert_eq!(d.skill, 2);
        assert!(d.tics.is_empty());
    }

    #[test]
    fn v15_header_names_e2m3() {
        let mut b = vec![0u8; 0x2A2 + 11];
        b[4] = 117;
        b[5] = 1;
        b[6] = 2;
        b[7] = 3;
        b[0x0A] = 1;
        let d = DmoDemo::parse(&b).unwrap();
        assert_eq!(d.map_name(), "E2L3");
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
    fn atomic_v116_demos_decode_every_record() {
        // v116 (the Atomic retail demos) has a one-byte player header; the
        // body must decode to exactly `reccnt` records.
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke/demos");
        for (name, map) in [("DEMO1", "E3L11"), ("DEMO2", "E4L11"), ("DEMO3", "E4L2")] {
            let Ok(b) = std::fs::read(dir.join(format!("{name}.DMO"))) else {
                return;
            };
            let reccnt = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
            let d = DmoDemo::parse(&b).unwrap();
            assert_eq!(d.map_name(), map, "{name}");
            assert_eq!(d.tics.len(), reccnt, "{name}");
            assert!(
                d.weapon_choices.is_empty(),
                "{name}: v116 has no weapon table"
            );
        }
    }

    #[test]
    fn lzw_looking_body_stays_header_only() {
        let mut b = v13_header(0, 1, 0, 1);
        b.extend_from_slice(&[8, 0, 20, 0, 9, 0, 1]);
        let d = DmoDemo::parse(&b).unwrap();
        assert_eq!(d.map_name(), "E1L2");
        assert!(d.tics.is_empty());
    }
}
