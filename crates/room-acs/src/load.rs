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

/// `SCRIPTF_NET` — only runs on the net arbiter (safe to honour as normal offline).
pub const SCRIPTF_NET: u16 = 1;
/// `SCRIPTF_CLIENTSIDE` — client-only (bootstrap: still run; netplay filters later).
pub const SCRIPTF_CLIENTSIDE: u16 = 2;

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
    /// `SFLG` flags (`SCRIPTF_*`); 0 when absent.
    pub flags: u16,
    /// `SVCT` local var count; `0` means Hexen default (20).
    pub local_count: u16,
}

/// One ACS function from the `FUNC` chunk (`ScriptFunctionInFile`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcsFunction {
    /// Argument count (also first locals).
    pub argc: u8,
    /// Extra local variables beyond args.
    pub local_count: u8,
    /// Non-zero when the function returns a value.
    pub has_return: u8,
    /// Import library index (`0` = local).
    pub import_num: u8,
    /// Bytecode entry PC (0 when imported unresolved).
    pub address: u32,
}

/// One map array from `ARAY` (+ optional `AINI` elements).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcsMapArray {
    /// Map-var index that stores this array's runtime slot.
    pub map_var: u32,
    /// Element storage (zero-filled, then `AINI`).
    pub elements: Vec<i32>,
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
    /// `FUNC` function table.
    pub functions: Vec<AcsFunction>,
    /// `FNAM` function names (parallel to `functions` when present).
    pub function_names: Vec<String>,
    /// `MINI` map-var defaults (applied when the module is attached).
    pub map_var_defaults: [i32; 128],
    /// `ARAY`/`AINI` map arrays (runtime copies live on `AcsState`).
    pub map_arrays: Vec<AcsMapArray>,
    /// `MEXP` exported map-variable / array names (for library linking).
    pub map_exports: Vec<String>,
    /// Module name (library lump name, or the map name for `BEHAVIOR`).
    pub name: String,
    /// Offset added to this module's `SNAM` script numbers so named scripts stay
    /// unique across modules (GZDoom numbers them by global name).
    pub named_base: i32,
    /// `MIMP` imported map variables: (local var number, exported name).
    pub map_imports: Vec<(u32, String)>,
    /// `AIMP` imported map arrays: (local var number, expected size, exported name).
    pub array_imports: Vec<(u32, u32, String)>,
    /// `MSTR` map variables initialised with strings.
    pub mstr_vars: Vec<u32>,
    /// `ASTR` map variables whose arrays hold strings.
    pub astr_vars: Vec<u32>,
    /// `ATAG` per-element tags (var number, tags: 0 int, 1 string, 2 function).
    pub atags: Vec<(u32, Vec<u8>)>,
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
            .map(|i| -(self.named_base + i as i32 + 1))
    }

    /// Index of exported map variable `name` (`MEXP`; GZDoom `FindMapVarName`).
    #[must_use]
    pub fn find_map_var_name(&self, name: &str) -> Option<usize> {
        self.map_exports
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
    }

    /// Index of function `name` (`FNAM`; GZDoom `FindFunctionName`).
    #[must_use]
    pub fn find_function_name(&self, name: &str) -> Option<usize> {
        self.function_names
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
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

/// GZDoom `UnescapeStringTable`: run `strbin` on every string in place (strings
/// only shrink). ACC keeps `\\n`, `\\c` (text colour) etc. as written.
fn unescape_strings(data: &mut [u8], offsets: &[u32]) {
    for &off in offsets {
        let start = off as usize;
        if start < data.len() {
            strbin(&mut data[start..]);
        }
    }
}

/// GZDoom `strbin`: decode C-style escapes of the NUL-terminated string at the
/// start of `buf`, writing the result (and its NUL) back over it.
pub(crate) fn strbin(buf: &mut [u8]) {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let mut w = 0;
    let mut r = 0;
    while r < end {
        let c = buf[r];
        r += 1;
        if c != b'\\' {
            buf[w] = c;
            w += 1;
            continue;
        }
        if r >= end {
            break;
        }
        let e = buf[r];
        let out = match e {
            b'a' => Some(7),
            b'b' => Some(8),
            b'c' => Some(0x1c), // TEXTCOLOR_ESCAPE
            b'f' => Some(12),
            b'n' => Some(b'\n'),
            b't' => Some(b'\t'),
            b'r' => Some(b'\r'),
            b'v' => Some(11),
            b'?' => Some(b'?'),
            b'\n' => None,
            b'x' | b'X' => {
                let mut v: u8 = 0;
                for _ in 0..2 {
                    let Some(&d) = buf.get(r + 1).filter(|_| r + 1 < end) else {
                        break;
                    };
                    let dv = match d {
                        b'0'..=b'9' => d - b'0',
                        b'a'..=b'f' => 10 + d - b'a',
                        b'A'..=b'F' => 10 + d - b'A',
                        _ => break,
                    };
                    v = v.wrapping_shl(4).wrapping_add(dv);
                    r += 1;
                }
                Some(v)
            }
            b'0'..=b'7' => {
                let mut v = e - b'0';
                for _ in 0..2 {
                    match buf.get(r + 1).filter(|_| r + 1 < end) {
                        Some(&d @ b'0'..=b'7') => {
                            v = v.wrapping_shl(3).wrapping_add(d - b'0');
                            r += 1;
                        }
                        _ => break,
                    }
                }
                Some(v)
            }
            other => Some(other),
        };
        if let Some(b) = out {
            buf[w] = b;
            w += 1;
        }
        r += 1;
    }
    if w < buf.len() {
        buf[w] = 0;
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

    let mut scripts = parse_sptr(&data, chunks_off, data_end, hybrid_sptr)?;
    apply_sflg_svct(&mut scripts, &data, chunks_off, data_end);
    let string_offsets = parse_strl(&data, chunks_off, data_end)?;
    unescape_strings(&mut data, &string_offsets);
    let libraries = parse_load_chunk(&data, chunks_off, data_end);
    let script_names = parse_snam_chunk(&data, chunks_off, data_end);
    let mut functions = parse_func_chunk(&data, chunks_off, data_end);
    let function_names = parse_fnam_chunk(&data, chunks_off, data_end);
    apply_sary_fary(&mut scripts, &mut functions, &data, chunks_off, data_end);
    let mut map_var_defaults = parse_mini_chunk(&data, chunks_off, data_end);
    let mut map_arrays = parse_aray_aini(&data, chunks_off, data_end, &mut map_var_defaults);
    apply_astr_atag(
        &mut map_arrays,
        &data,
        chunks_off,
        data_end,
        &map_var_defaults,
    );
    let map_exports = parse_mexp_chunk(&data, chunks_off, data_end);
    let map_imports = parse_mimp_chunk(&data, chunks_off, data_end);
    let array_imports = parse_aimp_chunk(&data, chunks_off, data_end);
    let mstr_vars = parse_u32_list_chunk(&data, chunks_off, data_end, b"MSTR");
    let astr_vars = parse_u32_list_chunk(&data, chunks_off, data_end, b"ASTR");
    let atags = parse_atag_chunks(&data, chunks_off, data_end);
    Ok(AcsModule {
        data,
        format,
        scripts,
        string_offsets,
        libraries,
        script_names,
        functions,
        function_names,
        map_var_defaults,
        map_arrays,
        map_exports,
        name: String::new(),
        named_base: 0,
        map_imports,
        array_imports,
        mstr_vars,
        astr_vars,
        atags,
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
                flags: 0,
                local_count: 0,
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
                flags: 0,
                local_count: 0,
            });
        }
    }
    Ok(scripts)
}

/// Apply `SFLG` / `SVCT` onto an already-parsed script directory.
fn apply_sflg_svct(scripts: &mut [ScriptDef], data: &[u8], chunks_off: usize, data_end: usize) {
    if let Some(off) = find_chunk(data, chunks_off, data_end, b"SFLG") {
        let size = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        let payload = data.get(off + 8..off + 8 + size).unwrap_or(&[]);
        for ent in payload.chunks_exact(4) {
            let number = i16::from_le_bytes([ent[0], ent[1]]) as i32;
            let flags = u16::from_le_bytes([ent[2], ent[3]]);
            if let Some(s) = scripts.iter_mut().find(|s| s.number == number) {
                s.flags = flags;
            }
        }
    }
    if let Some(off) = find_chunk(data, chunks_off, data_end, b"SVCT") {
        let size = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        let payload = data.get(off + 8..off + 8 + size).unwrap_or(&[]);
        for ent in payload.chunks_exact(4) {
            let number = i16::from_le_bytes([ent[0], ent[1]]) as i32;
            let locals = u16::from_le_bytes([ent[2], ent[3]]);
            if let Some(s) = scripts.iter_mut().find(|s| s.number == number) {
                s.local_count = locals;
            }
        }
    }
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
            flags: 0,
            local_count: 0,
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
    let mut data = bytes.to_vec();
    unescape_strings(&mut data, &string_offsets);
    Ok(AcsModule {
        data,
        format: AcsFormat::Acs0,
        scripts,
        string_offsets,
        libraries: Vec::new(),
        script_names: Vec::new(),
        functions: Vec::new(),
        function_names: Vec::new(),
        map_var_defaults: [0; 128],
        map_arrays: Vec::new(),
        map_exports: Vec::new(),
        name: String::new(),
        named_base: 0,
        map_imports: Vec::new(),
        array_imports: Vec::new(),
        mstr_vars: Vec::new(),
        astr_vars: Vec::new(),
        atags: Vec::new(),
    })
}

/// `FUNC` — 8-byte entries: argc, localcount, hasreturn, importnum, address.
fn parse_func_chunk(data: &[u8], chunks_off: usize, data_end: usize) -> Vec<AcsFunction> {
    let Some(off) = find_chunk(data, chunks_off, data_end, b"FUNC") else {
        return Vec::new();
    };
    let size =
        u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]) as usize;
    let payload = data.get(off + 8..off + 8 + size).unwrap_or(&[]);
    let mut out = Vec::with_capacity(payload.len() / 8);
    for ent in payload.chunks_exact(8) {
        out.push(AcsFunction {
            argc: ent[0],
            local_count: ent[1],
            has_return: ent[2],
            import_num: ent[3],
            address: u32::from_le_bytes([ent[4], ent[5], ent[6], ent[7]]),
        });
    }
    out
}

/// `FNAM` — same layout as `SNAM` (count + offsets, or packed C strings).
fn parse_fnam_chunk(data: &[u8], chunks_off: usize, data_end: usize) -> Vec<String> {
    let Some(off) = find_chunk(data, chunks_off, data_end, b"FNAM") else {
        return Vec::new();
    };
    let size =
        u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]) as usize;
    let start = off + 8;
    let end = start.saturating_add(size).min(data_end);
    let payload = &data[start..end];
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
                        names.push(s.to_owned());
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

/// `MINI` — map-var initialisers (`first_var` + `i32` values); may repeat.
fn parse_mini_chunk(data: &[u8], chunks_off: usize, data_end: usize) -> [i32; 128] {
    let mut vars = [0i32; 128];
    let mut off = chunks_off;
    while off + 8 <= data_end {
        let id = &data[off..off + 4];
        let size = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        let payload_end = off.saturating_add(8).saturating_add(size);
        if payload_end > data_end {
            break;
        }
        if id == b"MINI" && size >= 4 {
            let payload = &data[off + 8..payload_end];
            let first =
                u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
            let count = (size / 4).saturating_sub(1);
            for i in 0..count {
                let base = 4 + i * 4;
                if base + 4 > payload.len() {
                    break;
                }
                let v = i32::from_le_bytes([
                    payload[base],
                    payload[base + 1],
                    payload[base + 2],
                    payload[base + 3],
                ]);
                if let Some(slot) = vars.get_mut(first + i) {
                    *slot = v;
                }
            }
        }
        off = payload_end;
    }
    vars
}

/// `ARAY` sizes + `AINI` element values. Writes array indices into `map_vars`.
fn parse_aray_aini(
    data: &[u8],
    chunks_off: usize,
    data_end: usize,
    map_vars: &mut [i32; 128],
) -> Vec<AcsMapArray> {
    let Some(off) = find_chunk(data, chunks_off, data_end, b"ARAY") else {
        return Vec::new();
    };
    let size =
        u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]) as usize;
    let payload = data.get(off + 8..off + 8 + size).unwrap_or(&[]);
    let mut arrays = Vec::new();
    for ent in payload.chunks_exact(8) {
        let map_var = u32::from_le_bytes([ent[0], ent[1], ent[2], ent[3]]);
        let arr_size = u32::from_le_bytes([ent[4], ent[5], ent[6], ent[7]]) as usize;
        let slot = arrays.len() as i32;
        if let Some(mv) = map_vars.get_mut(map_var as usize) {
            *mv = slot;
        }
        arrays.push(AcsMapArray {
            map_var,
            elements: vec![0; arr_size.min(1_048_576)],
        });
    }
    // AINI chunks: first dword = mapvar index holding the array slot.
    let mut coff = chunks_off;
    while coff + 8 <= data_end {
        let id = &data[coff..coff + 4];
        let csize = u32::from_le_bytes([
            data[coff + 4],
            data[coff + 5],
            data[coff + 6],
            data[coff + 7],
        ]) as usize;
        let payload_end = coff.saturating_add(8).saturating_add(csize);
        if payload_end > data_end {
            break;
        }
        if id == b"AINI" && csize >= 4 {
            let p = &data[coff + 8..payload_end];
            let map_var = u32::from_le_bytes([p[0], p[1], p[2], p[3]]) as usize;
            let arraynum = map_vars.get(map_var).copied().unwrap_or(-1);
            if arraynum >= 0 {
                if let Some(arr) = arrays.get_mut(arraynum as usize) {
                    let inits = (csize / 4).saturating_sub(1);
                    for j in 0..inits.min(arr.elements.len()) {
                        let base = 4 + j * 4;
                        if base + 4 > p.len() {
                            break;
                        }
                        arr.elements[j] =
                            i32::from_le_bytes([p[base], p[base + 1], p[base + 2], p[base + 3]]);
                    }
                }
            }
        }
        coff = payload_end;
    }
    arrays
}

/// `SARY` / `FARY` — local array size lists; bump script/function local counts.
fn apply_sary_fary(
    scripts: &mut [ScriptDef],
    functions: &mut [AcsFunction],
    data: &[u8],
    chunks_off: usize,
    data_end: usize,
) {
    let mut off = chunks_off;
    while off + 8 <= data_end {
        let id = [data[off], data[off + 1], data[off + 2], data[off + 3]];
        let size = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        let payload_end = off.saturating_add(8).saturating_add(size);
        if payload_end > data_end {
            break;
        }
        if size >= 6 {
            if &id == b"SARY" {
                if let Some((num, total)) = parse_local_array_chunk(data, off, size) {
                    if let Some(s) = scripts.iter_mut().find(|s| s.number == i32::from(num)) {
                        let base = if s.local_count == 0 {
                            20u32
                        } else {
                            u32::from(s.local_count)
                        };
                        s.local_count = base.saturating_add(total).min(u32::from(u16::MAX)) as u16;
                    }
                }
            } else if &id == b"FARY" {
                if let Some((num, total)) = parse_local_array_chunk(data, off, size) {
                    if let Some(f) = functions.get_mut(usize::from(num)) {
                        let base = u32::from(f.local_count) + u32::from(f.argc);
                        let new_total = base.saturating_add(total);
                        f.local_count = new_total.saturating_sub(u32::from(f.argc)).min(255) as u8;
                    }
                }
            }
        }
        off = payload_end;
    }
}

/// GZDoom `ParseLocalArrayChunk` — returns (script_or_func index, total elements).
fn parse_local_array_chunk(data: &[u8], chunk_off: usize, size: usize) -> Option<(u16, u32)> {
    if size < 6 || chunk_off + 10 > data.len() {
        return None;
    }
    let num = u16::from_le_bytes([data[chunk_off + 8], data[chunk_off + 9]]);
    let count = size.saturating_sub(2) / 4;
    let mut total = 0u32;
    for i in 0..count {
        let base = chunk_off + 10 + i * 4;
        if base + 4 > data.len() {
            break;
        }
        total = total.saturating_add(u32::from_le_bytes([
            data[base],
            data[base + 1],
            data[base + 2],
            data[base + 3],
        ]));
    }
    Some((num, total.min(1_048_576)))
}

/// `ASTR` / `ATAG` — mark map-array cells as strings (lookup into string table → leave
/// indices as-is for Room; presence is accepted so BD libraries load).
fn apply_astr_atag(
    _map_arrays: &mut [AcsMapArray],
    data: &[u8],
    chunks_off: usize,
    data_end: usize,
    _map_vars: &[i32; 128],
) {
    // Soft-accept: walk chunks so malformed sizes don't panic; no runtime rewrite yet.
    let mut off = chunks_off;
    while off + 8 <= data_end {
        let id = &data[off..off + 4];
        let size = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        let payload_end = off.saturating_add(8).saturating_add(size);
        if payload_end > data_end {
            break;
        }
        let _seen = id == b"ASTR" || id == b"ATAG";
        let _ = _seen;
        off = payload_end;
    }
}

/// `MEXP` — exported map variable names, indexed by map variable number
/// (count + offsets like `FNAM`; GZDoom `FindStringInChunk`).
fn parse_mexp_chunk(data: &[u8], chunks_off: usize, data_end: usize) -> Vec<String> {
    let Some(off) = find_chunk(data, chunks_off, data_end, b"MEXP") else {
        return Vec::new();
    };
    let size =
        u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]) as usize;
    let start = off + 8;
    let end = start.saturating_add(size).min(data_end);
    parse_count_offset_names(&data[start..end]).unwrap_or_default()
}

/// `count` + `count` offsets (from payload start) to NUL-terminated names.
/// Empty names keep their slot: the index is the variable / function number.
fn parse_count_offset_names(payload: &[u8]) -> Option<Vec<String>> {
    let count = u32::from_le_bytes(payload.get(0..4)?.try_into().ok()?) as usize;
    if count > 65_536 || payload.len() < 4 + count * 4 {
        return None;
    }
    let mut names = Vec::with_capacity(count);
    for i in 0..count {
        let base = 4 + i * 4;
        let ofs = u32::from_le_bytes(payload[base..base + 4].try_into().ok()?) as usize;
        let rest = payload.get(ofs..).unwrap_or(&[]);
        let len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        names.push(String::from_utf8_lossy(&rest[..len]).into_owned());
    }
    Some(names)
}

/// Payload of chunk `id`, if present.
fn chunk_payload<'a>(
    data: &'a [u8],
    chunks_off: usize,
    data_end: usize,
    id: &[u8; 4],
) -> Option<&'a [u8]> {
    let off = find_chunk(data, chunks_off, data_end, id)?;
    let size =
        u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]) as usize;
    let start = off + 8;
    data.get(start..start.saturating_add(size).min(data_end))
}

/// NUL-terminated string at the start of `bytes`, and the bytes consumed (incl. NUL).
fn take_cstr(bytes: &[u8]) -> (String, usize) {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    (
        String::from_utf8_lossy(&bytes[..len]).into_owned(),
        (len + 1).min(bytes.len()),
    )
}

/// `MIMP` — `[u32 var][name\0]…` imported map variables.
fn parse_mimp_chunk(data: &[u8], chunks_off: usize, data_end: usize) -> Vec<(u32, String)> {
    let Some(p) = chunk_payload(data, chunks_off, data_end, b"MIMP") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= p.len() {
        let var = u32::from_le_bytes([p[i], p[i + 1], p[i + 2], p[i + 3]]);
        i += 4;
        let (name, used) = take_cstr(&p[i..]);
        i += used.max(1);
        out.push((var, name));
    }
    out
}

/// `AIMP` — `u32 count` then `[u32 var][u32 size][name\0]…` imported arrays.
fn parse_aimp_chunk(data: &[u8], chunks_off: usize, data_end: usize) -> Vec<(u32, u32, String)> {
    let Some(p) = chunk_payload(data, chunks_off, data_end, b"AIMP") else {
        return Vec::new();
    };
    let Some(count) = p
        .get(0..4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut i = 4;
    for _ in 0..count.min(4096) {
        if i + 8 > p.len() {
            break;
        }
        let var = u32::from_le_bytes([p[i], p[i + 1], p[i + 2], p[i + 3]]);
        let size = u32::from_le_bytes([p[i + 4], p[i + 5], p[i + 6], p[i + 7]]);
        i += 8;
        let (name, used) = take_cstr(&p[i..]);
        i += used.max(1);
        out.push((var, size, name));
    }
    out
}

/// `MSTR` / `ASTR` — a plain list of `u32` map-variable numbers.
fn parse_u32_list_chunk(data: &[u8], chunks_off: usize, data_end: usize, id: &[u8; 4]) -> Vec<u32> {
    chunk_payload(data, chunks_off, data_end, id)
        .map(|p| {
            p.chunks_exact(4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        })
        .unwrap_or_default()
}

/// Every `ATAG` chunk: version byte (0), `u32` map var, then one tag byte per element.
fn parse_atag_chunks(data: &[u8], chunks_off: usize, data_end: usize) -> Vec<(u32, Vec<u8>)> {
    let mut out = Vec::new();
    let mut off = chunks_off;
    while off + 8 <= data_end {
        let size = u32::from_le_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        let payload_end = off.saturating_add(8).saturating_add(size);
        if payload_end > data_end {
            break;
        }
        if &data[off..off + 4] == b"ATAG" && size >= 5 && data[off + 8] == 0 {
            let p = &data[off + 8..payload_end];
            let var = u32::from_le_bytes([p[1], p[2], p[3], p[4]]);
            out.push((var, p[5..].to_vec()));
        }
        off = payload_end;
    }
    out
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
mod strbin_tests {
    #[test]
    fn strbin_decodes_acc_escapes() {
        let mut b = b"\\n\\n-Friendly-\\cDok\\x41\\101\\\\\0tail".to_vec();
        super::strbin(&mut b);
        let end = b.iter().position(|&c| c == 0).unwrap();
        assert_eq!(&b[..end], b"\n\n-Friendly-\x1cDokAA\\");
    }
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

    #[test]
    fn parse_func_aray_mini_aini() {
        // BD5: FUNC / ARAY / MINI / AINI chunks.
        let mut code = vec![0u8; 16];
        code[0..4].copy_from_slice(&1u32.to_le_bytes()); // TERMINATE
        let chunks_off = 8 + code.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"ACSE");
        out.extend_from_slice(&(chunks_off as u32).to_le_bytes());
        out.extend_from_slice(&code);
        // FUNC: one function, argc=1, locals=2, hasret=1, import=0, addr=12
        let mut func = Vec::new();
        func.push(1u8);
        func.push(2u8);
        func.push(1u8);
        func.push(0u8);
        func.extend_from_slice(&12u32.to_le_bytes());
        out.extend_from_slice(b"FUNC");
        out.extend_from_slice(&(func.len() as u32).to_le_bytes());
        out.extend_from_slice(&func);
        // MINI: first_var=3, value=99
        let mut mini = Vec::new();
        mini.extend_from_slice(&3u32.to_le_bytes());
        mini.extend_from_slice(&99i32.to_le_bytes());
        out.extend_from_slice(b"MINI");
        out.extend_from_slice(&(mini.len() as u32).to_le_bytes());
        out.extend_from_slice(&mini);
        // ARAY: mapvar 5 → size 4
        let mut aray = Vec::new();
        aray.extend_from_slice(&5u32.to_le_bytes());
        aray.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(b"ARAY");
        out.extend_from_slice(&(aray.len() as u32).to_le_bytes());
        out.extend_from_slice(&aray);
        // AINI for mapvar 5: elements 7,8
        let mut aini = Vec::new();
        aini.extend_from_slice(&5u32.to_le_bytes());
        aini.extend_from_slice(&7i32.to_le_bytes());
        aini.extend_from_slice(&8i32.to_le_bytes());
        out.extend_from_slice(b"AINI");
        out.extend_from_slice(&(aini.len() as u32).to_le_bytes());
        out.extend_from_slice(&aini);

        let modu = load_behavior(&out).expect("parse");
        assert_eq!(modu.functions.len(), 1);
        assert_eq!(modu.functions[0].argc, 1);
        assert_eq!(modu.functions[0].local_count, 2);
        assert_eq!(modu.functions[0].address, 12);
        assert_eq!(modu.map_var_defaults[3], 99);
        assert_eq!(modu.map_var_defaults[5], 0); // array slot 0
        assert_eq!(modu.map_arrays.len(), 1);
        assert_eq!(modu.map_arrays[0].elements.len(), 4);
        assert_eq!(modu.map_arrays[0].elements[0], 7);
        assert_eq!(modu.map_arrays[0].elements[1], 8);
    }

    #[test]
    fn parse_sflg_svct() {
        let mut code = vec![0u8; 16];
        code[0..4].copy_from_slice(&1u32.to_le_bytes());
        let chunks_off = 8 + code.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"ACSE");
        out.extend_from_slice(&(chunks_off as u32).to_le_bytes());
        out.extend_from_slice(&code);
        let mut sptr = Vec::new();
        sptr.extend_from_slice(&7i16.to_le_bytes());
        sptr.extend_from_slice(&0u16.to_le_bytes());
        sptr.extend_from_slice(&8u32.to_le_bytes());
        sptr.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(b"SPTR");
        out.extend_from_slice(&(sptr.len() as u32).to_le_bytes());
        out.extend_from_slice(&sptr);
        // SFLG: script 7 → NET|CLIENTSIDE
        let mut sflg = Vec::new();
        sflg.extend_from_slice(&7i16.to_le_bytes());
        sflg.extend_from_slice(&(SCRIPTF_NET | SCRIPTF_CLIENTSIDE).to_le_bytes());
        out.extend_from_slice(b"SFLG");
        out.extend_from_slice(&(sflg.len() as u32).to_le_bytes());
        out.extend_from_slice(&sflg);
        // SVCT: script 7 → 40 locals
        let mut svct = Vec::new();
        svct.extend_from_slice(&7i16.to_le_bytes());
        svct.extend_from_slice(&40u16.to_le_bytes());
        out.extend_from_slice(b"SVCT");
        out.extend_from_slice(&(svct.len() as u32).to_le_bytes());
        out.extend_from_slice(&svct);

        let modu = load_behavior(&out).expect("parse");
        assert_eq!(modu.scripts.len(), 1);
        assert_eq!(modu.scripts[0].number, 7);
        assert_eq!(modu.scripts[0].flags, SCRIPTF_NET | SCRIPTF_CLIENTSIDE);
        assert_eq!(modu.scripts[0].local_count, 40);
    }
}
