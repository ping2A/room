//! ACS object formats — ACS0 / ACSE / hybrid loader.

use crate::error::AcsError;

/// Object format (GZDoom `ACSFormat`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcsFormat {
    /// Hexen `"ACS\0"` directory.
    Acs0,
    /// ZDoom `"ACSE"` — 4-byte p-codes + chunks.
    Acse,
    /// ZDoom `"ACSe"` — compressed p-codes + chunks (directory loadable; VM expand later).
    AcseCompressed,
}

/// Script activation type (GZDoom `EACSScriptType` subset).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptType {
    /// Closed (activated by special / `ACS_Execute`).
    Closed,
    /// Open (map start).
    Open,
    /// Player respawn.
    Respawn,
    /// Player death.
    Death,
    /// Player enters the map (after Open).
    Enter,
    /// Player returns to a hub map.
    Return,
    /// Map unload / exit.
    Unloading,
    /// Other Hexen/ZDoom types — raw type byte.
    Other(u8),
}

impl ScriptType {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Closed,
            1 => Self::Open,
            2 => Self::Respawn,
            3 => Self::Death,
            4 => Self::Enter,
            15 => Self::Return,
            13 => Self::Unloading,
            n => Self::Other(n),
        }
    }
}

/// One script entry from the module directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptDef {
    /// Script number (OPEN scripts store the number after the ≥1000 remap).
    pub number: i32,
    /// Bytecode entry PC (byte offset into module data).
    pub address: u32,
    /// Declared argument count (0..=3 for Hexen ACS0; ZDoom may allow more).
    pub argc: u8,
    /// Script type.
    pub script_type: ScriptType,
}

/// Loaded ACS module (immutable bytecode + directory).
#[derive(Clone, Debug)]
pub struct AcsModule {
    /// Raw lump bytes (header + code + directory / chunks).
    pub data: Vec<u8>,
    /// Detected format.
    pub format: AcsFormat,
    /// Script directory.
    pub scripts: Vec<ScriptDef>,
    /// String table (absolute byte offsets into `data`).
    pub string_offsets: Vec<u32>,
    /// `LOAD` chunk library names (ACS libraries this module imports).
    pub libraries: Vec<String>,
    /// `SNAM` names for negative script numbers (`names[0]` → script −1).
    pub script_names: Vec<String>,
}

impl AcsModule {
    /// Look up a script by number.
    #[must_use]
    pub fn find_script(&self, number: i32) -> Option<&ScriptDef> {
        self.scripts.iter().find(|s| s.number == number)
    }

    /// Resolve a named script to its negative number (`−1` …), if listed in `SNAM`.
    #[must_use]
    pub fn script_number_by_name(&self, name: &str) -> Option<i32> {
        self.script_names
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .map(|i| -(i as i32 + 1))
    }

    /// Read a little-endian `u32` at `pc`, or `None` if OOB.
    #[must_use]
    pub fn read_u32(&self, pc: u32) -> Option<u32> {
        let i = pc as usize;
        let bytes = self.data.get(i..i + 4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a little-endian `i32` at `pc`.
    #[must_use]
    pub fn read_i32(&self, pc: u32) -> Option<i32> {
        self.read_u32(pc).map(|u| u as i32)
    }

    /// Fetch next p-code opcode, advancing `pc` (GZDoom `RunScript` opcode decode).
    pub fn fetch_op(&self, pc: &mut u32) -> Option<u32> {
        match self.format {
            AcsFormat::AcseCompressed => {
                let b = *self.data.get(*pc as usize)?;
                *pc = pc.wrapping_add(1);
                if b >= 240 {
                    let b2 = *self.data.get(*pc as usize)?;
                    *pc = pc.wrapping_add(1);
                    Some(u32::from(240 + (u16::from(b - 240) << 8) + u16::from(b2)))
                } else {
                    Some(u32::from(b))
                }
            }
            AcsFormat::Acs0 | AcsFormat::Acse => {
                let op = self.read_u32(*pc)?;
                *pc = pc.wrapping_add(4);
                Some(op)
            }
        }
    }

    /// `NEXTBYTE` — 1 byte under ACSe, else 4-byte word.
    pub fn fetch_byte_or_word(&self, pc: &mut u32) -> Option<i32> {
        match self.format {
            AcsFormat::AcseCompressed => {
                let b = *self.data.get(*pc as usize)?;
                *pc = pc.wrapping_add(1);
                Some(i32::from(b))
            }
            AcsFormat::Acs0 | AcsFormat::Acse => {
                let v = self.read_i32(*pc)?;
                *pc = pc.wrapping_add(4);
                Some(v)
            }
        }
    }

    /// Always a little-endian `i32` immediate (e.g. `PUSHNUMBER`, `DELAYDIRECT`).
    pub fn fetch_i32(&self, pc: &mut u32) -> Option<i32> {
        let v = self.read_i32(*pc)?;
        *pc = pc.wrapping_add(4);
        Some(v)
    }

    /// Always a little-endian `u32` immediate (e.g. `GOTO` target).
    pub fn fetch_u32(&self, pc: &mut u32) -> Option<u32> {
        let v = self.read_u32(*pc)?;
        *pc = pc.wrapping_add(4);
        Some(v)
    }

    /// Always one unsigned byte (packed ACSe immediates).
    pub fn fetch_u8(&self, pc: &mut u32) -> Option<u8> {
        let b = *self.data.get(*pc as usize)?;
        *pc = pc.wrapping_add(1);
        Some(b)
    }

    /// `NEXTSHORT` — 2 bytes under ACSe, else 4-byte word.
    pub fn fetch_short_or_word(&self, pc: &mut u32) -> Option<i32> {
        match self.format {
            AcsFormat::AcseCompressed => {
                let i = *pc as usize;
                let lo = *self.data.get(i)?;
                let hi = *self.data.get(i + 1)?;
                *pc = pc.wrapping_add(2);
                Some(i32::from(i16::from_le_bytes([lo, hi])))
            }
            AcsFormat::Acs0 | AcsFormat::Acse => {
                let v = self.read_i32(*pc)?;
                *pc = pc.wrapping_add(4);
                Some(v)
            }
        }
    }

    /// String by index, or empty when missing.
    #[must_use]
    pub fn string(&self, index: usize) -> &str {
        let Some(&off) = self.string_offsets.get(index) else {
            return "";
        };
        let start = off as usize;
        if start >= self.data.len() {
            return "";
        }
        let end = self.data[start..]
            .iter()
            .position(|&b| b == 0)
            .map_or(self.data.len(), |n| start + n);
        std::str::from_utf8(&self.data[start..end]).unwrap_or("")
    }
}

/// Parse a `BEHAVIOR` lump (ACS0, ACSE, ACSe, or hybrid ACS0+chunks).
pub fn load_behavior(bytes: &[u8]) -> Result<AcsModule, AcsError> {
    if bytes.len() < 32 {
        return Err(AcsError::Message("lump too short".into()));
    }
    if &bytes[0..3] != b"ACS" {
        return Err(AcsError::Message(format!(
            "bad magic {:02x?}",
            &bytes[0..4.min(bytes.len())]
        )));
    }
    match bytes[3] {
        0 => load_acs0_or_hybrid(bytes),
        b'E' => load_acse(bytes, AcsFormat::Acse),
        b'e' => load_acse(bytes, AcsFormat::AcseCompressed),
        _ => Err(AcsError::Message(format!(
            "bad magic {:02x?}",
            &bytes[0..4]
        ))),
    }
}

fn load_acs0_or_hybrid(bytes: &[u8]) -> Result<AcsModule, AcsError> {
    let dir_off = read_u32(bytes, 4)? as usize;
    if dir_off >= 6 * 4 && dir_off <= bytes.len() {
        let pretag = &bytes[dir_off - 4..dir_off];
        if pretag == b"ACSE" || pretag == b"ACSe" {
            let chunks_off = read_u32(bytes, dir_off - 8)? as usize;
            let data_size = dir_off.saturating_sub(8);
            let format = if pretag == b"ACSe" {
                AcsFormat::AcseCompressed
            } else {
                AcsFormat::Acse
            };
            return parse_chunks(bytes, chunks_off, data_size, format, true);
        }
    }
    parse_acs0(bytes, dir_off)
}

fn load_acse(bytes: &[u8], format: AcsFormat) -> Result<AcsModule, AcsError> {
    let chunks_off = read_u32(bytes, 4)? as usize;
    parse_chunks(bytes, chunks_off, bytes.len(), format, false)
}

/// `hybrid_sptr` = ACS0-wrapped chunks → 8-byte SPTR entries; pure ACSE → 12-byte.
fn parse_chunks(
    bytes: &[u8],
    chunks_off: usize,
    data_end: usize,
    format: AcsFormat,
    hybrid_sptr: bool,
) -> Result<AcsModule, AcsError> {
    if chunks_off > data_end || data_end > bytes.len() {
        return Err(AcsError::Message("chunk region OOB".into()));
    }
    let mut data = bytes.to_vec();
    unencrypt_stre(&mut data, chunks_off, data_end);

    let scripts = parse_sptr(&data, chunks_off, data_end, hybrid_sptr)?;
    let string_offsets = parse_strl(&data, chunks_off, data_end)?;
    let libraries = parse_load_chunk(&data, chunks_off, data_end);
    let script_names = parse_snam_chunk(&data, chunks_off, data_end);
    Ok(AcsModule {
        data,
        format,
        scripts,
        string_offsets,
        libraries,
        script_names,
    })
}

fn find_chunk(data: &[u8], chunks_off: usize, data_end: usize, id: &[u8; 4]) -> Option<usize> {
    let mut off = chunks_off;
    while off + 8 <= data_end {
        let chunk_id = &data[off..off + 4];
        let size = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        let payload_end = off.saturating_add(8).saturating_add(size);
        if payload_end > data_end {
            return None;
        }
        if chunk_id == id {
            return Some(off);
        }
        off = payload_end;
    }
    None
}

fn parse_sptr(
    data: &[u8],
    chunks_off: usize,
    data_end: usize,
    hybrid_sptr: bool,
) -> Result<Vec<ScriptDef>, AcsError> {
    let Some(off) = find_chunk(data, chunks_off, data_end, b"SPTR") else {
        return Ok(Vec::new());
    };
    let size =
        u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]) as usize;
    let payload = data
        .get(off + 8..off + 8 + size)
        .ok_or_else(|| AcsError::Message("SPTR truncated".into()))?;
    let mut scripts = Vec::new();
    if hybrid_sptr {
        if size % 8 != 0 {
            return Err(AcsError::Message("SPTR size not multiple of 8".into()));
        }
        for ent in payload.chunks_exact(8) {
            let number = i16::from_le_bytes([ent[0], ent[1]]) as i32;
            let ty = ent[2];
            let argc = ent[3];
            let address = u32::from_le_bytes([ent[4], ent[5], ent[6], ent[7]]);
            scripts.push(ScriptDef {
                number,
                address,
                argc,
                script_type: ScriptType::from_u8(ty),
            });
        }
    } else {
        if size % 12 != 0 {
            return Err(AcsError::Message("SPTR size not multiple of 12".into()));
        }
        for ent in payload.chunks_exact(12) {
            let number = i16::from_le_bytes([ent[0], ent[1]]) as i32;
            let ty = u16::from_le_bytes([ent[2], ent[3]]) as u8;
            let address = u32::from_le_bytes([ent[4], ent[5], ent[6], ent[7]]);
            let argc = u32::from_le_bytes([ent[8], ent[9], ent[10], ent[11]]);
            if argc > 255 {
                return Err(AcsError::Message(format!("script argc {argc} too large")));
            }
            scripts.push(ScriptDef {
                number,
                address,
                argc: argc as u8,
                script_type: ScriptType::from_u8(ty),
            });
        }
    }
    Ok(scripts)
}

fn parse_strl(data: &[u8], chunks_off: usize, data_end: usize) -> Result<Vec<u32>, AcsError> {
    let Some(off) = find_chunk(data, chunks_off, data_end, b"STRL") else {
        return Ok(Vec::new());
    };
    let size =
        u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]) as usize;
    let table_abs = off + 8;
    let table = data
        .get(table_abs..table_abs + size)
        .ok_or_else(|| AcsError::Message("STRL truncated".into()))?;
    if table.len() < 12 {
        return Ok(Vec::new());
    }
    let count = u32::from_le_bytes([table[4], table[5], table[6], table[7]]) as usize;
    let mut offsets = Vec::with_capacity(count);
    for i in 0..count {
        let oidx = 12 + i * 4;
        if oidx + 4 > table.len() {
            return Err(AcsError::Message("STRL offset OOB".into()));
        }
        let rel = u32::from_le_bytes([
            table[oidx],
            table[oidx + 1],
            table[oidx + 2],
            table[oidx + 3],
        ]);
        offsets.push(table_abs as u32 + rel);
    }
    Ok(offsets)
}

/// Descramble STRE chunks in-place and rename them to STRL (GZDoom `UnencryptStrings`).
fn unencrypt_stre(data: &mut [u8], chunks_off: usize, data_end: usize) {
    let mut off = chunks_off;
    while off + 8 <= data_end {
        let id = [data[off], data[off + 1], data[off + 2], data[off + 3]];
        let size = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        let payload_end = off + 8 + size;
        if payload_end > data_end {
            break;
        }
        if &id == b"STRE" && size >= 12 {
            let count = u32::from_le_bytes([
                data[off + 12],
                data[off + 13],
                data[off + 14],
                data[off + 15],
            ]) as usize;
            for strnum in 0..count {
                let oidx = off + 8 + 12 + strnum * 4;
                if oidx + 4 > payload_end {
                    break;
                }
                let ofs = i32::from_le_bytes([
                    data[oidx],
                    data[oidx + 1],
                    data[oidx + 2],
                    data[oidx + 3],
                ]);
                // GZDoom: `uint8_t *data = (uint8_t *)chunk + ofs + 8`
                // with `chunk` pointing at the chunk id; so base = off, data at off+ofs+8.
                let mut i = 0i32;
                let p = ofs.wrapping_mul(157135);
                loop {
                    let di = (off as i32) + ofs + 8 + i;
                    if di < 0 || di as usize >= payload_end {
                        break;
                    }
                    let di = di as usize;
                    let last = data[di] ^ ((p + (i >> 1)) as u8);
                    data[di] = last;
                    i += 1;
                    if last == 0 {
                        break;
                    }
                }
            }
            data[off] = b'S';
            data[off + 1] = b'T';
            data[off + 2] = b'R';
            data[off + 3] = b'L';
        }
        off = payload_end;
    }
}

fn parse_acs0(bytes: &[u8], dir_off: usize) -> Result<AcsModule, AcsError> {
    if dir_off + 4 > bytes.len() {
        return Err(AcsError::Message("directory OOB".into()));
    }
    let numscripts = read_u32(bytes, dir_off)? as usize;
    let mut cursor = dir_off + 4;
    let mut scripts = Vec::with_capacity(numscripts);
    for _ in 0..numscripts {
        if cursor + 12 > bytes.len() {
            return Err(AcsError::Message("script entry OOB".into()));
        }
        let number_raw = read_i32(bytes, cursor)?;
        let address = read_u32(bytes, cursor + 4)?;
        let argc = read_u32(bytes, cursor + 8)?;
        cursor += 12;
        let (number, script_type) = if number_raw >= 1000 {
            (number_raw - 1000, ScriptType::Open)
        } else {
            (number_raw, ScriptType::Closed)
        };
        if argc > 3 {
            return Err(AcsError::Message(format!("script argc {argc} > 3")));
        }
        scripts.push(ScriptDef {
            number,
            address,
            argc: argc as u8,
            script_type,
        });
    }
    if cursor + 4 > bytes.len() {
        return Err(AcsError::Message("string count OOB".into()));
    }
    let numstrings = read_u32(bytes, cursor)? as usize;
    cursor += 4;
    let mut string_offsets = Vec::with_capacity(numstrings);
    for _ in 0..numstrings {
        if cursor + 4 > bytes.len() {
            return Err(AcsError::Message("string offset OOB".into()));
        }
        string_offsets.push(read_u32(bytes, cursor)?);
        cursor += 4;
    }
    Ok(AcsModule {
        data: bytes.to_vec(),
        format: AcsFormat::Acs0,
        scripts,
        string_offsets,
        libraries: Vec::new(),
        script_names: Vec::new(),
    })
}

/// `SNAM` chunk: NUL-terminated script names (index 0 → script −1).
fn parse_snam_chunk(data: &[u8], chunks_off: usize, data_end: usize) -> Vec<String> {
    let Some(off) = find_chunk(data, chunks_off, data_end, b"SNAM") else {
        return Vec::new();
    };
    let size =
        u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]) as usize;
    let start = off + 8;
    let end = start.saturating_add(size).min(data_end);
    let payload = &data[start..end];
    // GZDoom: first dword is number of strings, then offsets — but many ACC
    // builds store packed C strings; accept both.
    if payload.len() >= 4 {
        let count = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
        if count > 0 && count < 4096 && payload.len() >= 4 + count * 4 {
            let mut names = Vec::with_capacity(count);
            for i in 0..count {
                let base = 4 + i * 4;
                let ofs = u32::from_le_bytes([
                    payload[base],
                    payload[base + 1],
                    payload[base + 2],
                    payload[base + 3],
                ]) as usize;
                if ofs < payload.len() {
                    let rest = &payload[ofs..];
                    let len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
                    if let Ok(s) = std::str::from_utf8(&rest[..len]) {
                        if !s.is_empty() {
                            names.push(s.to_owned());
                        }
                    }
                }
            }
            if !names.is_empty() {
                return names;
            }
        }
    }
    parse_nul_strings(payload)
}

/// `LOAD` chunk: concatenated NUL-terminated library names.
fn parse_load_chunk(data: &[u8], chunks_off: usize, data_end: usize) -> Vec<String> {
    let Some(off) = find_chunk(data, chunks_off, data_end, b"LOAD") else {
        return Vec::new();
    };
    let size =
        u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]) as usize;
    let start = off + 8;
    let end = start.saturating_add(size).min(data_end);
    parse_nul_strings(&data[start..end])
}

fn parse_nul_strings(payload: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < payload.len() {
        if payload[i] == 0 {
            i += 1;
            continue;
        }
        let rest = &payload[i..];
        let len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        if let Ok(s) = std::str::from_utf8(&rest[..len]) {
            let name = s.trim();
            if !name.is_empty() {
                out.push(name.to_owned());
            }
        }
        i += len + 1;
    }
    out
}

fn read_u32(bytes: &[u8], off: usize) -> Result<u32, AcsError> {
    let slice = bytes
        .get(off..off + 4)
        .ok_or_else(|| AcsError::Message("u32 OOB".into()))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_i32(bytes: &[u8], off: usize) -> Result<i32, AcsError> {
    Ok(read_u32(bytes, off)? as i32)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn fixture_delay_terminate() -> Vec<u8> {
        let mut code = Vec::new();
        code.extend_from_slice(&56u32.to_le_bytes());
        code.extend_from_slice(&3i32.to_le_bytes());
        code.extend_from_slice(&1u32.to_le_bytes());
        let dir_off = 8 + code.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"ACS\0");
        out.extend_from_slice(&(dir_off as u32).to_le_bytes());
        out.extend_from_slice(&code);
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&1i32.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        // Pad to ≥32 bytes (GZDoom minimum useful size).
        while out.len() < 32 {
            out.push(0);
        }
        out
    }

    #[test]
    fn parse_acs0_delay_script() {
        let modu = load_behavior(&fixture_delay_terminate()).expect("parse");
        assert_eq!(modu.format, AcsFormat::Acs0);
        assert_eq!(modu.scripts.len(), 1);
        assert_eq!(modu.scripts[0].number, 1);
        assert_eq!(modu.scripts[0].address, 8);
        assert_eq!(modu.scripts[0].script_type, ScriptType::Closed);
        assert_eq!(modu.read_u32(8), Some(56));
    }

    #[test]
    fn script_type_bytes() {
        assert_eq!(ScriptType::from_u8(4), ScriptType::Enter);
        assert_eq!(ScriptType::from_u8(3), ScriptType::Death);
        assert_eq!(ScriptType::from_u8(2), ScriptType::Respawn);
        assert_eq!(ScriptType::from_u8(15), ScriptType::Return);
        assert_eq!(ScriptType::from_u8(13), ScriptType::Unloading);
    }

    #[test]
    fn open_script_number_remap() {
        let mut code = Vec::new();
        code.extend_from_slice(&1u32.to_le_bytes());
        let dir_off = 8 + code.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"ACS\0");
        out.extend_from_slice(&(dir_off as u32).to_le_bytes());
        out.extend_from_slice(&code);
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&1001i32.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        while out.len() < 32 {
            out.push(0);
        }
        let modu = load_behavior(&out).expect("parse");
        assert_eq!(modu.scripts[0].number, 1);
        assert_eq!(modu.scripts[0].script_type, ScriptType::Open);
    }

    fn fixture_acse() -> Vec<u8> {
        let mut code = Vec::new();
        code.extend_from_slice(&1u32.to_le_bytes()); // TERMINATE
        while code.len() < 16 {
            code.push(0);
        }
        let chunks_off = 8 + code.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"ACSE");
        out.extend_from_slice(&(chunks_off as u32).to_le_bytes());
        out.extend_from_slice(&code);
        let mut sptr_payload = Vec::new();
        sptr_payload.extend_from_slice(&1i16.to_le_bytes());
        sptr_payload.extend_from_slice(&1u16.to_le_bytes()); // Open
        sptr_payload.extend_from_slice(&8u32.to_le_bytes());
        sptr_payload.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(b"SPTR");
        out.extend_from_slice(&(sptr_payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&sptr_payload);
        let mut strl = Vec::new();
        strl.extend_from_slice(&0u32.to_le_bytes());
        strl.extend_from_slice(&1u32.to_le_bytes());
        strl.extend_from_slice(&0u32.to_le_bytes());
        strl.extend_from_slice(&16u32.to_le_bytes());
        strl.extend_from_slice(b"hi\0\0");
        out.extend_from_slice(b"STRL");
        out.extend_from_slice(&(strl.len() as u32).to_le_bytes());
        out.extend_from_slice(&strl);
        out
    }

    #[test]
    fn parse_acse_sptr_and_strl() {
        let modu = load_behavior(&fixture_acse()).expect("parse");
        assert_eq!(modu.format, AcsFormat::Acse);
        assert_eq!(modu.scripts.len(), 1);
        assert_eq!(modu.scripts[0].number, 1);
        assert_eq!(modu.scripts[0].script_type, ScriptType::Open);
        assert_eq!(modu.scripts[0].address, 8);
        assert_eq!(modu.string(0), "hi");
    }

    #[test]
    fn parse_snam_nul_strings() {
        let mut code = Vec::new();
        code.extend_from_slice(&1u32.to_le_bytes()); // TERMINATE
        while code.len() < 16 {
            code.push(0);
        }
        let chunks_off = 8 + code.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"ACSE");
        out.extend_from_slice(&(chunks_off as u32).to_le_bytes());
        out.extend_from_slice(&code);
        // SPTR: script −1 at address 8
        let mut sptr = Vec::new();
        sptr.extend_from_slice(&(-1i16).to_le_bytes());
        sptr.extend_from_slice(&0u16.to_le_bytes()); // Closed
        sptr.extend_from_slice(&8u32.to_le_bytes());
        sptr.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(b"SPTR");
        out.extend_from_slice(&(sptr.len() as u32).to_le_bytes());
        out.extend_from_slice(&sptr);
        // SNAM: packed C strings
        let snam = b"DoorOpen\0Elevator\0";
        out.extend_from_slice(b"SNAM");
        out.extend_from_slice(&(snam.len() as u32).to_le_bytes());
        out.extend_from_slice(snam);
        let modu = load_behavior(&out).expect("parse");
        assert_eq!(modu.script_names, vec!["DoorOpen", "Elevator"]);
        assert_eq!(modu.script_number_by_name("dooropen"), Some(-1));
        assert_eq!(modu.script_number_by_name("Elevator"), Some(-2));
        assert_eq!(modu.scripts[0].number, -1);
    }

    #[test]
    fn parse_snam_offset_table() {
        // count=2, then two offsets, then string bytes at those offsets.
        // Layout: [count][ofs0][ofs1] = 12 bytes header; strings at 12 and 18.
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(&12u32.to_le_bytes());
        payload.extend_from_slice(&18u32.to_le_bytes());
        payload.extend_from_slice(b"Alpha\0");
        payload.extend_from_slice(b"Beta\0");

        let mut code = vec![0u8; 16];
        code[0..4].copy_from_slice(&1u32.to_le_bytes());
        let chunks_off = 8 + code.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"ACSE");
        out.extend_from_slice(&(chunks_off as u32).to_le_bytes());
        out.extend_from_slice(&code);
        out.extend_from_slice(b"SNAM");
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&payload);
        let modu = load_behavior(&out).expect("parse");
        assert_eq!(modu.script_names, vec!["Alpha", "Beta"]);
        assert_eq!(modu.script_number_by_name("alpha"), Some(-1));
        assert_eq!(modu.script_number_by_name("Beta"), Some(-2));
    }
}
