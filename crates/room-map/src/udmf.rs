//! Fast zero-copy UDMF lexer + block parser (M3 bootstrap).

use crate::error::MapError;
use crate::raw::{RawLinedef, RawSector, RawSidedef, RawThing, RawVertex};
use std::borrow::Cow;
use std::collections::BTreeMap;

/// UDMF token.
#[derive(Clone, Debug, PartialEq)]
pub enum UdmfTok<'a> {
    /// Identifier.
    Ident(&'a str),
    /// Integer literal.
    Int(i64),
    /// Float literal.
    Float(f64),
    /// Quoted string.
    String(Cow<'a, str>),
    /// `true` / `false`.
    Bool(bool),
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `=`
    Eq,
    /// `;`
    Semi,
    /// End of input.
    Eof,
}

/// Zero-copy UDMF lexer.
pub struct UdmfLexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> UdmfLexer<'a> {
    /// Create a lexer over `text`.
    #[must_use]
    pub fn new(text: &'a str) -> Self {
        Self {
            src: text,
            bytes: text.as_bytes(),
            pos: 0,
        }
    }

    /// Next token.
    pub fn next_tok(&mut self) -> Result<UdmfTok<'a>, MapError> {
        self.skip_trivia()?;
        if self.pos >= self.bytes.len() {
            return Ok(UdmfTok::Eof);
        }
        let b = self.bytes[self.pos];
        match b {
            b'{' => {
                self.pos += 1;
                Ok(UdmfTok::LBrace)
            }
            b'}' => {
                self.pos += 1;
                Ok(UdmfTok::RBrace)
            }
            b'=' => {
                self.pos += 1;
                Ok(UdmfTok::Eq)
            }
            b';' => {
                self.pos += 1;
                Ok(UdmfTok::Semi)
            }
            b'"' => self.lex_string(),
            b'-' | b'0'..=b'9' => self.lex_number(),
            c if c == b'_' || c.is_ascii_alphabetic() => self.lex_ident(),
            _ => Err(MapError::Message(format!(
                "UDMF: unexpected byte 0x{b:02x} at {}",
                self.pos
            ))),
        }
    }

    fn skip_trivia(&mut self) -> Result<(), MapError> {
        loop {
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos] == b'/'
                && self.bytes[self.pos + 1] == b'/'
            {
                self.pos += 2;
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            if self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos] == b'/'
                && self.bytes[self.pos + 1] == b'*'
            {
                self.pos += 2;
                while self.pos + 1 < self.bytes.len() {
                    if self.bytes[self.pos] == b'*' && self.bytes[self.pos + 1] == b'/' {
                        self.pos += 2;
                        break;
                    }
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
        Ok(())
    }

    fn lex_ident(&mut self) -> Result<UdmfTok<'a>, MapError> {
        let start = self.pos;
        self.pos += 1;
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            if c == b'_' || c.is_ascii_alphanumeric() {
                self.pos += 1;
            } else {
                break;
            }
        }
        let s = &self.src[start..self.pos];
        if s.eq_ignore_ascii_case("true") {
            return Ok(UdmfTok::Bool(true));
        }
        if s.eq_ignore_ascii_case("false") {
            return Ok(UdmfTok::Bool(false));
        }
        Ok(UdmfTok::Ident(s))
    }

    fn lex_number(&mut self) -> Result<UdmfTok<'a>, MapError> {
        let start = self.pos;
        let neg = self.bytes[self.pos] == b'-';
        if neg {
            self.pos += 1;
        }
        // Hex: `0x` / `0X` (optional leading `-`).
        if self.pos + 1 < self.bytes.len()
            && self.bytes[self.pos] == b'0'
            && matches!(self.bytes[self.pos + 1], b'x' | b'X')
        {
            self.pos += 2;
            let hex_start = self.pos;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_hexdigit() {
                self.pos += 1;
            }
            if self.pos == hex_start {
                return Err(MapError::Message("UDMF: empty hex literal".into()));
            }
            let digits = &self.src[hex_start..self.pos];
            let n = i64::from_str_radix(digits, 16)
                .map_err(|_| MapError::Message(format!("UDMF bad hex `0x{digits}`")))?;
            return Ok(UdmfTok::Int(if neg { -n } else { n }));
        }
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        let mut is_float = false;
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'.' {
            is_float = true;
            self.pos += 1;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        if self.pos < self.bytes.len() && matches!(self.bytes[self.pos], b'e' | b'E') {
            is_float = true;
            self.pos += 1;
            if self.pos < self.bytes.len() && matches!(self.bytes[self.pos], b'+' | b'-') {
                self.pos += 1;
            }
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        let s = &self.src[start..self.pos];
        if is_float {
            let f: f64 = s
                .parse()
                .map_err(|_| MapError::Message(format!("UDMF bad float `{s}`")))?;
            Ok(UdmfTok::Float(f))
        } else {
            let n: i64 = s
                .parse()
                .map_err(|_| MapError::Message(format!("UDMF bad int `{s}`")))?;
            Ok(UdmfTok::Int(n))
        }
    }

    fn lex_string(&mut self) -> Result<UdmfTok<'a>, MapError> {
        self.pos += 1; // "
        let start = self.pos;
        let mut owned: Option<String> = None;
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'"' => {
                    let end = self.pos;
                    self.pos += 1;
                    return if let Some(mut o) = owned {
                        o.push_str(&self.src[start..end]);
                        Ok(UdmfTok::String(Cow::Owned(o)))
                    } else {
                        Ok(UdmfTok::String(Cow::Borrowed(&self.src[start..end])))
                    };
                }
                b'\\' => {
                    let base = owned.get_or_insert_with(|| self.src[start..self.pos].to_owned());
                    self.pos += 1;
                    let esc = *self
                        .bytes
                        .get(self.pos)
                        .ok_or_else(|| MapError::Message("UDMF unterminated string".into()))?;
                    self.pos += 1;
                    match esc {
                        b'n' => base.push('\n'),
                        b't' => base.push('\t'),
                        b'\\' | b'"' => base.push(esc as char),
                        other => base.push(other as char),
                    }
                    // restart borrow slice after escape — simplify: fold rest into owned
                    let rest_start = self.pos;
                    while self.pos < self.bytes.len()
                        && self.bytes[self.pos] != b'"'
                        && self.bytes[self.pos] != b'\\'
                    {
                        self.pos += 1;
                    }
                    base.push_str(&self.src[rest_start..self.pos]);
                    if self.pos < self.bytes.len() && self.bytes[self.pos] == b'\\' {
                        continue;
                    }
                    if self.pos < self.bytes.len() && self.bytes[self.pos] == b'"' {
                        self.pos += 1;
                        return Ok(UdmfTok::String(Cow::Owned(
                            owned.take().unwrap_or_default(),
                        )));
                    }
                }
                _ => self.pos += 1,
            }
        }
        Err(MapError::Message("UDMF unterminated string".into()))
    }
}

/// Per-object custom `user_*` UDMF properties.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UdmfUserBag {
    /// Integer / bool `user_*` values.
    pub ints: BTreeMap<String, i32>,
    /// Float `user_*` values stored as 16.16 fixed.
    pub fixed: BTreeMap<String, i32>,
}

/// Parallel `user_*` bags for UDMF objects (index = object order).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UdmfUserStore {
    /// Linedefs.
    pub lines: Vec<UdmfUserBag>,
    /// Sidedefs.
    pub sides: Vec<UdmfUserBag>,
    /// Sectors.
    pub sectors: Vec<UdmfUserBag>,
    /// Things.
    pub things: Vec<UdmfUserBag>,
    /// Extended linedef flags ([`ext`]`::LINE_*`), parallel to linedefs.
    pub line_ext: Vec<u32>,
    /// Extended sector flags ([`ext`]`::SECTOR_*`), parallel to sectors.
    pub sector_ext: Vec<u32>,
    /// Extended thing flags ([`ext`]`::THING_*`), parallel to things.
    pub thing_ext: Vec<u32>,
}

impl UdmfUserBag {
    /// No `user_*` fields.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ints.is_empty() && self.fixed.is_empty()
    }
}

impl UdmfUserStore {
    /// Extended flags of linedef `i` (0 when not UDMF / unset).
    pub fn line_ext(&self, i: usize) -> u32 {
        self.line_ext.get(i).copied().unwrap_or(0)
    }
    /// Extended flags of sector `i`.
    pub fn sector_ext(&self, i: usize) -> u32 {
        self.sector_ext.get(i).copied().unwrap_or(0)
    }
    /// Extended flags of thing `i`.
    pub fn thing_ext(&self, i: usize) -> u32 {
        self.thing_ext.get(i).copied().unwrap_or(0)
    }
}

/// UDMF boolean keys that have no slot in the raw Doom/Hexen records.
pub mod ext {
    /// Linedef `blockuse`: use traces stop here.
    pub const LINE_BLOCKUSE: u32 = 1;
    /// Linedef `passuse`: use passes through to lines behind.
    pub const LINE_PASSUSE: u32 = 2;
    /// Linedef `blocklandmonsters`: blocks non-flying monsters.
    pub const LINE_BLOCKLANDMONSTERS: u32 = 4;
    /// Linedef `monsteractivate`: monsters may trigger the special.
    pub const LINE_MONSTERACTIVATE: u32 = 8;
    /// Linedef `jumpover` (Strife railing): blocks unless the mover jumped.
    pub const LINE_JUMPOVER: u32 = 16;
    /// Linedef `drawfullheight`: automap shows as one-sided.
    pub const LINE_DRAWFULLHEIGHT: u32 = 32;
    /// Linedef `noskywalls`.
    pub const LINE_NOSKYWALLS: u32 = 64;
    /// Linedef `zoneboundary` (reverb zone boundary).
    pub const LINE_ZONEBOUNDARY: u32 = 128;
    /// Sector `noattack`: monsters inside do not attack.
    pub const SECTOR_NOATTACK: u32 = 1;
    /// Sector `norespawn`: players do not respawn here (DM spots skipped).
    pub const SECTOR_NORESPAWN: u32 = 2;
    /// Sector `hurtmonsters`: sector damage also hits monsters.
    pub const SECTOR_HURTMONSTERS: u32 = 4;
    /// Sector `harminair`: sector damage applies even when airborne.
    pub const SECTOR_HARMINAIR: u32 = 8;
    /// Sector `damagehazard`: Strife-style hazard (accumulating) damage.
    pub const SECTOR_DAMAGEHAZARD: u32 = 16;
    /// Sector `hidden`: never drawn on the automap.
    pub const SECTOR_HIDDEN: u32 = 32;
    /// Sector `noskywalls`.
    pub const SECTOR_NOSKYWALLS: u32 = 64;
    /// Sector `portal_floor_blocksound`.
    pub const SECTOR_PORTAL_FLOOR_BLOCKSOUND: u32 = 128;
    /// Sector `portal_floor_disabled`.
    pub const SECTOR_PORTAL_FLOOR_DISABLED: u32 = 256;
    /// Sector `portal_floor_nopass`.
    pub const SECTOR_PORTAL_FLOOR_NOPASS: u32 = 512;
    /// Sector `portal_floor_norender`.
    pub const SECTOR_PORTAL_FLOOR_NORENDER: u32 = 1024;
    /// Sector `portal_ceil_blocksound` (the floor bit shifted left by 4).
    pub const SECTOR_PORTAL_CEIL_BLOCKSOUND: u32 = 2048;
    /// Sector `portal_ceil_disabled`.
    pub const SECTOR_PORTAL_CEIL_DISABLED: u32 = 4096;
    /// Sector `portal_ceil_nopass`.
    pub const SECTOR_PORTAL_CEIL_NOPASS: u32 = 8192;
    /// Sector `portal_ceil_norender`.
    pub const SECTOR_PORTAL_CEIL_NORENDER: u32 = 16384;
    /// Thing `standing` (Strife): stays put.
    pub const THING_STANDING: u32 = 1;
    /// Thing `strifeally`.
    pub const THING_STRIFEALLY: u32 = 2;
    /// Thing `invisible` (Strife): spawned invisible.
    pub const THING_INVISIBLE: u32 = 4;
    /// Thing `translucent` (Strife): 25 % alpha style.
    pub const THING_TRANSLUCENT: u32 = 8;
    /// Thing `coop` false: hidden in cooperative games.
    pub const THING_NOTCOOP: u32 = 16;
    /// Thing `dm` false: hidden in deathmatch.
    pub const THING_NOTDM: u32 = 32;
}

fn ext_bits(fields: &BTreeMap<String, UdmfTok<'_>>, keys: &[(&str, u32)]) -> u32 {
    keys.iter()
        .filter(|(k, _)| bool_field(fields, k, false))
        .fold(0, |a, (_, b)| a | b)
}

/// UDMF keys of the extended linedef flags, in [`ext`] bit order.
pub const LINE_EXT_KEYS: &[(&str, u32)] = &[
    ("blockuse", ext::LINE_BLOCKUSE),
    ("passuse", ext::LINE_PASSUSE),
    ("blocklandmonsters", ext::LINE_BLOCKLANDMONSTERS),
    ("monsteractivate", ext::LINE_MONSTERACTIVATE),
    ("jumpover", ext::LINE_JUMPOVER),
    ("drawfullheight", ext::LINE_DRAWFULLHEIGHT),
    ("noskywalls", ext::LINE_NOSKYWALLS),
    ("zoneboundary", ext::LINE_ZONEBOUNDARY),
];

/// UDMF keys of the extended sector flags (portal plane flags included).
pub const SECTOR_EXT_KEYS: &[(&str, u32)] = &[
    ("noattack", ext::SECTOR_NOATTACK),
    ("norespawn", ext::SECTOR_NORESPAWN),
    ("hurtmonsters", ext::SECTOR_HURTMONSTERS),
    ("harminair", ext::SECTOR_HARMINAIR),
    ("damagehazard", ext::SECTOR_DAMAGEHAZARD),
    ("hidden", ext::SECTOR_HIDDEN),
    ("noskywalls", ext::SECTOR_NOSKYWALLS),
    (
        "portal_floor_blocksound",
        ext::SECTOR_PORTAL_FLOOR_BLOCKSOUND,
    ),
    ("portal_floor_disabled", ext::SECTOR_PORTAL_FLOOR_DISABLED),
    ("portal_floor_nopass", ext::SECTOR_PORTAL_FLOOR_NOPASS),
    ("portal_floor_norender", ext::SECTOR_PORTAL_FLOOR_NORENDER),
    ("portal_ceil_blocksound", ext::SECTOR_PORTAL_CEIL_BLOCKSOUND),
    ("portal_ceil_disabled", ext::SECTOR_PORTAL_CEIL_DISABLED),
    ("portal_ceil_nopass", ext::SECTOR_PORTAL_CEIL_NOPASS),
    ("portal_ceil_norender", ext::SECTOR_PORTAL_CEIL_NORENDER),
];

/// UDMF keys of the extended thing flags that are `true` when set
/// (`coop` / `dm` are inverted into `THING_NOTCOOP` / `THING_NOTDM`).
pub const THING_EXT_KEYS: &[(&str, u32)] = &[
    ("standing", ext::THING_STANDING),
    ("strifeally", ext::THING_STRIFEALLY),
    ("invisible", ext::THING_INVISIBLE),
    ("translucent", ext::THING_TRANSLUCENT),
];

fn line_ext_from_fields(fields: &BTreeMap<String, UdmfTok<'_>>) -> u32 {
    ext_bits(fields, LINE_EXT_KEYS)
}

fn sector_ext_from_fields(fields: &BTreeMap<String, UdmfTok<'_>>) -> u32 {
    ext_bits(fields, SECTOR_EXT_KEYS)
}

fn thing_ext_from_fields(fields: &BTreeMap<String, UdmfTok<'_>>) -> u32 {
    use ext::*;
    let mut v = ext_bits(fields, THING_EXT_KEYS);
    if !bool_field(fields, "coop", true) {
        v |= THING_NOTCOOP;
    }
    if !bool_field(fields, "dm", true) {
        v |= THING_NOTDM;
    }
    v
}

/// Parsed UDMF translation unit (raw fields).
#[derive(Clone, Debug, Default)]
pub struct UdmfMap {
    /// Namespace name (`doom`, `zdoom`, …).
    pub namespace: String,
    /// Vertices.
    pub vertices: Vec<RawVertex>,
    /// Exact (fractional) vertex positions, parallel to `vertices`.
    pub vertex_pos: Vec<room_core::DVec2>,
    /// Linedefs.
    pub linedefs: Vec<RawLinedef>,
    /// Sidedefs.
    pub sidedefs: Vec<RawSidedef>,
    /// Sectors.
    pub sectors: Vec<RawSector>,
    /// Things.
    pub things: Vec<RawThing>,
    /// Custom `user_*` properties parallel to object vectors.
    pub user: UdmfUserStore,
}

/// Parse a `TEXTMAP` lump into raw records (subset of keys).
pub fn parse_udmf(text: &str) -> Result<UdmfMap, MapError> {
    let mut lx = UdmfLexer::new(text);
    let mut map = UdmfMap::default();
    // `zfloor` / `zceiling` per vertex (UDMF vertex heights).
    let mut vertex_z: Vec<(Option<f64>, Option<f64>)> = Vec::new();
    loop {
        match lx.next_tok()? {
            UdmfTok::Eof => break,
            UdmfTok::Ident(name) if name.eq_ignore_ascii_case("namespace") => {
                expect_eq(&mut lx)?;
                map.namespace = expect_string_or_ident(&mut lx)?;
                expect_semi(&mut lx)?;
            }
            UdmfTok::Ident(kind) => {
                expect_lbrace(&mut lx)?;
                let fields = parse_block_fields(&mut lx)?;
                let user = collect_user_fields(&fields);
                match kind.to_ascii_lowercase().as_str() {
                    "thing" => {
                        let ext = thing_ext_from_fields(&fields);
                        map.things.push(RawThing {
                            ext,
                            user: user.clone(),
                            ..thing_from_fields(&fields)
                        });
                        map.user.things.push(user);
                        map.user.thing_ext.push(ext);
                    }
                    "linedef" => {
                        let ext = line_ext_from_fields(&fields);
                        map.linedefs.push(RawLinedef {
                            ext,
                            user: user.clone(),
                            ..linedef_from_fields(&fields)
                        });
                        map.user.lines.push(user);
                        map.user.line_ext.push(ext);
                    }
                    "sidedef" => {
                        map.sidedefs.push(RawSidedef {
                            user: user.clone(),
                            ..sidedef_from_fields(&fields)
                        });
                        map.user.sides.push(user);
                    }
                    "sector" => {
                        let ext = sector_ext_from_fields(&fields);
                        map.sectors.push(RawSector {
                            ext,
                            user: user.clone(),
                            ..sector_from_fields(&fields)
                        });
                        map.user.sectors.push(user);
                        map.user.sector_ext.push(ext);
                    }
                    "vertex" => {
                        let z = |k: &str| match fields.get(k) {
                            Some(UdmfTok::Int(v)) => Some(*v as f64),
                            Some(UdmfTok::Float(v)) => Some(*v),
                            _ => None,
                        };
                        vertex_z.push((z("zfloor"), z("zceiling")));
                        map.vertices.push(vertex_from_fields(&fields));
                        map.vertex_pos.push(room_core::DVec2::new(
                            float_field(&fields, "x", 0.0),
                            float_field(&fields, "y", 0.0),
                        ));
                    }
                    _ => {} // ignore unknown blocks
                }
            }
            other => {
                return Err(MapError::Message(format!(
                    "UDMF: unexpected top-level {other:?}"
                )));
            }
        }
    }
    apply_vertex_heights(&mut map, &vertex_z);
    Ok(map)
}

/// UDMF vertex heights (GZDoom `P_SetSlopes` vertex pass): a sector bounded by
/// exactly three vertices, any of which sets `zfloor` / `zceiling`, gets a
/// floor / ceiling plane through those three points. Vertices without the key
/// use the sector's flat height. An explicit `floorplane_*` / `ceilingplane_*`
/// on the sector wins.
fn apply_vertex_heights(map: &mut UdmfMap, vertex_z: &[(Option<f64>, Option<f64>)]) {
    if !vertex_z.iter().any(|(f, c)| f.is_some() || c.is_some()) {
        return;
    }
    let mut verts: Vec<Vec<u16>> = vec![Vec::new(); map.sectors.len()];
    for l in &map.linedefs {
        for side in [l.side_front, l.side_back] {
            let Some(sd) = map
                .sidedefs
                .get(usize::from(side))
                .filter(|_| side != u16::MAX)
            else {
                continue;
            };
            let Some(list) = verts.get_mut(usize::from(sd.sector)) else {
                continue;
            };
            for v in [l.v1, l.v2] {
                if !list.contains(&v) {
                    list.push(v);
                }
            }
        }
    }
    for (si, list) in verts.iter().enumerate() {
        if list.len() != 3 {
            continue;
        }
        let pos = |v: u16| map.vertex_pos.get(usize::from(v)).copied();
        let (Some(p0), Some(p1), Some(p2)) = (pos(list[0]), pos(list[1]), pos(list[2])) else {
            continue;
        };
        let zs = |v: u16| {
            vertex_z
                .get(usize::from(v))
                .copied()
                .unwrap_or((None, None))
        };
        let sector = &map.sectors[si];
        let (floor_h, ceil_h) = (
            f64::from(sector.floor_height),
            f64::from(sector.ceiling_height),
        );
        for ceiling in [false, true] {
            let pick = |v: u16| if ceiling { zs(v).1 } else { zs(v).0 };
            if list.iter().all(|&v| pick(v).is_none()) {
                continue;
            }
            let flat = if ceiling { ceil_h } else { floor_h };
            let z = |v: u16| pick(v).unwrap_or(flat);
            let a = [p0.x(), p0.y(), z(list[0])];
            let b = [p1.x(), p1.y(), z(list[1])];
            let c = [p2.x(), p2.y(), z(list[2])];
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let w = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let mut n = [
                u[1] * w[2] - u[2] * w[1],
                u[2] * w[0] - u[0] * w[2],
                u[0] * w[1] - u[1] * w[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len < 1e-9 || n[2].abs() < 1e-9 {
                continue;
            }
            for k in &mut n {
                *k /= len;
            }
            // Floors face up, ceilings face down (`secplane_t` convention).
            if (n[2] < 0.0) != ceiling {
                for k in &mut n {
                    *k = -*k;
                }
            }
            let d = -(n[0] * a[0] + n[1] * a[1] + n[2] * a[2]);
            let plane = Some([n[0], n[1], n[2], d]);
            let s = &mut map.sectors[si];
            if ceiling {
                if s.ceiling_plane.is_none() {
                    s.ceiling_plane = plane;
                }
            } else if s.floor_plane.is_none() {
                s.floor_plane = plane;
            }
        }
    }
}

fn expect_eq(lx: &mut UdmfLexer<'_>) -> Result<(), MapError> {
    match lx.next_tok()? {
        UdmfTok::Eq => Ok(()),
        other => Err(MapError::Message(format!(
            "UDMF: expected `=`, got {other:?}"
        ))),
    }
}

fn expect_semi(lx: &mut UdmfLexer<'_>) -> Result<(), MapError> {
    match lx.next_tok()? {
        UdmfTok::Semi => Ok(()),
        other => Err(MapError::Message(format!(
            "UDMF: expected `;`, got {other:?}"
        ))),
    }
}

fn expect_lbrace(lx: &mut UdmfLexer<'_>) -> Result<(), MapError> {
    match lx.next_tok()? {
        UdmfTok::LBrace => Ok(()),
        other => Err(MapError::Message(format!(
            "UDMF: expected `{{`, got {other:?}"
        ))),
    }
}

fn expect_string_or_ident(lx: &mut UdmfLexer<'_>) -> Result<String, MapError> {
    match lx.next_tok()? {
        UdmfTok::String(s) => Ok(s.into_owned()),
        UdmfTok::Ident(s) => Ok(s.to_owned()),
        other => Err(MapError::Message(format!(
            "UDMF: expected string, got {other:?}"
        ))),
    }
}

fn parse_block_fields<'a>(
    lx: &mut UdmfLexer<'a>,
) -> Result<BTreeMap<String, UdmfTok<'a>>, MapError> {
    let mut fields = BTreeMap::new();
    loop {
        match lx.next_tok()? {
            UdmfTok::RBrace | UdmfTok::Eof => break,
            UdmfTok::Ident(key) => {
                expect_eq(lx)?;
                let val = lx.next_tok()?;
                expect_semi(lx)?;
                fields.insert(key.to_ascii_lowercase(), val);
            }
            other => {
                return Err(MapError::Message(format!(
                    "UDMF block: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(fields)
}

fn int_field(fields: &BTreeMap<String, UdmfTok<'_>>, key: &str, default: i64) -> i64 {
    match fields.get(key) {
        Some(UdmfTok::Int(n)) => *n,
        Some(UdmfTok::Float(f)) => *f as i64,
        _ => default,
    }
}

fn bool_field(fields: &BTreeMap<String, UdmfTok<'_>>, key: &str, default: bool) -> bool {
    match fields.get(key) {
        Some(UdmfTok::Bool(b)) => *b,
        Some(UdmfTok::Ident(s)) => {
            matches!(s.to_ascii_lowercase().as_str(), "true" | "1" | "yes")
        }
        Some(UdmfTok::String(s)) => {
            matches!(s.to_ascii_lowercase().as_str(), "true" | "1" | "yes")
        }
        Some(UdmfTok::Int(n)) => *n != 0,
        Some(UdmfTok::Float(f)) => *f != 0.0,
        _ => default,
    }
}

fn float_field(fields: &BTreeMap<String, UdmfTok<'_>>, key: &str, default: f64) -> f64 {
    match fields.get(key) {
        Some(UdmfTok::Float(f)) => *f,
        Some(UdmfTok::Int(n)) => *n as f64,
        _ => default,
    }
}

fn str_field(fields: &BTreeMap<String, UdmfTok<'_>>, key: &str) -> String {
    match fields.get(key) {
        Some(UdmfTok::String(s)) => s.to_ascii_uppercase(),
        Some(UdmfTok::Ident(s)) => s.to_ascii_uppercase(),
        _ => "-".into(),
    }
}

fn vertex_from_fields(fields: &BTreeMap<String, UdmfTok<'_>>) -> RawVertex {
    RawVertex {
        x: float_field(fields, "x", 0.0) as i16,
        y: float_field(fields, "y", 0.0) as i16,
    }
}

fn thing_from_fields(fields: &BTreeMap<String, UdmfTok<'_>>) -> RawThing {
    // UDMF skill1..5 default true → Doom MTF skill bits.
    let mut flags: i16 = 0;
    if bool_field(fields, "skill1", true) || bool_field(fields, "skill2", true) {
        flags |= 1;
    }
    if bool_field(fields, "skill3", true) {
        flags |= 2;
    }
    if bool_field(fields, "skill4", true) || bool_field(fields, "skill5", true) {
        flags |= 4;
    }
    if bool_field(fields, "ambush", false) {
        flags |= 8;
    }
    // `single` default true; clear appearance in SP via MTF_NOTSINGLE when false.
    if !bool_field(fields, "single", true) {
        flags |= 16;
    }
    if bool_field(fields, "dormant", false) {
        flags |= 32;
    }
    // MBF / UDMF friend → same bit as Doom `MTF_FRIEND` (128).
    if bool_field(fields, "friend", false) {
        flags |= 128;
    }
    RawThing {
        tid: int_field(fields, "id", 0) as i16,
        x: float_field(fields, "x", 0.0) as i16,
        y: float_field(fields, "y", 0.0) as i16,
        z: float_field(fields, "height", 0.0) as i16,
        angle: int_field(fields, "angle", 0) as i16,
        type_id: int_field(fields, "type", 0) as i16,
        flags,
        special: int_field(fields, "special", 0) as u8,
        args: [
            int_field(fields, "arg0", 0) as i32,
            int_field(fields, "arg1", 0) as i32,
            int_field(fields, "arg2", 0) as i32,
            int_field(fields, "arg3", 0) as i32,
            int_field(fields, "arg4", 0) as i32,
        ],
        hexen: true,
        friendly_see_blocks: if fields.contains_key("friendlyseeblocks") {
            int_field(fields, "friendlyseeblocks", 10) as i16
        } else {
            -1
        },
        health: if fields.contains_key("health") {
            int_field(fields, "health", 0) as i32
        } else {
            0
        },
        score: if fields.contains_key("score") {
            int_field(fields, "score", 0) as i32
        } else {
            i32::MIN
        },
        gravity: if fields.contains_key("gravity") {
            float_field(fields, "gravity", 1.0)
        } else {
            -1.0
        },
        scale_x: {
            let scale = if fields.contains_key("scale") {
                float_field(fields, "scale", 1.0)
            } else {
                0.0
            };
            if fields.contains_key("scalex") {
                float_field(fields, "scalex", 1.0)
            } else {
                scale
            }
        },
        scale_y: {
            let scale = if fields.contains_key("scale") {
                float_field(fields, "scale", 1.0)
            } else {
                0.0
            };
            if fields.contains_key("scaley") {
                float_field(fields, "scaley", 1.0)
            } else {
                scale
            }
        },
        alpha: if fields.contains_key("alpha") {
            float_field(fields, "alpha", 1.0)
        } else {
            -1.0
        },
        pitch: if fields.contains_key("pitch") {
            int_field(fields, "pitch", 0) as i16
        } else {
            i16::MIN
        },
        roll: if fields.contains_key("roll") {
            int_field(fields, "roll", 0) as i16
        } else {
            i16::MIN
        },
        fill_color: if fields.contains_key("fillcolor") {
            int_field(fields, "fillcolor", 0) as i32
        } else {
            -1
        },
        render_style: match fields.get("renderstyle") {
            Some(UdmfTok::String(s)) => s.to_string(),
            Some(UdmfTok::Ident(s)) => (*s).to_owned(),
            _ => String::new(),
        },
        float_bob_phase: if fields.contains_key("floatbobphase") {
            int_field(fields, "floatbobphase", 0) as i16
        } else {
            -1
        },
        count_secret: bool_field(fields, "countsecret", false),
        no_count: bool_field(fields, "nocount", false),
        arg0_str: match fields.get("arg0str") {
            Some(UdmfTok::String(s)) => s.to_string(),
            Some(UdmfTok::Ident(s)) => (*s).to_owned(),
            _ => String::new(),
        },
        conversation: int_field(fields, "conversation", 0) as i32,
        accuracy: if fields.contains_key("accuracy") {
            int_field(fields, "accuracy", 0) as i32
        } else {
            i32::MIN
        },
        stamina: if fields.contains_key("stamina") {
            int_field(fields, "stamina", 0) as i32
        } else {
            i32::MIN
        },
        species: match fields.get("species") {
            Some(UdmfTok::String(s)) => s.to_string(),
            Some(UdmfTok::Ident(s)) => (*s).to_owned(),
            _ => String::new(),
        },
        melee_range: if fields.contains_key("meleerange") {
            float_field(fields, "meleerange", 0.0)
        } else {
            -1.0
        },
        view_height: if fields.contains_key("viewheight") {
            float_field(fields, "viewheight", 0.0)
        } else {
            -1.0
        },
        mass: if fields.contains_key("mass") {
            int_field(fields, "mass", 0) as i32
        } else {
            i32::MIN
        },
        speed: if fields.contains_key("speed") {
            float_field(fields, "speed", 0.0)
        } else {
            -1.0
        },
        friction: if fields.contains_key("friction") {
            float_field(fields, "friction", 0.0)
        } else {
            -1.0
        },
        damage_factor: if fields.contains_key("damagefactor") {
            float_field(fields, "damagefactor", 0.0)
        } else {
            -1.0
        },
        max_step_height: if fields.contains_key("maxstepheight") {
            float_field(fields, "maxstepheight", 0.0)
        } else {
            -1.0
        },
        max_dropoff_height: if fields.contains_key("maxdropoffheight") {
            float_field(fields, "maxdropoffheight", 0.0)
        } else {
            -1.0
        },
        float_speed: if fields.contains_key("floatspeed") {
            float_field(fields, "floatspeed", 0.0)
        } else {
            -1.0
        },
        fast_speed: if fields.contains_key("fastspeed") {
            float_field(fields, "fastspeed", 0.0)
        } else {
            -1.0
        },
        jump_z: if fields.contains_key("jumpz") {
            float_field(fields, "jumpz", 0.0)
        } else {
            -1.0
        },
        attack_z_offset: if fields.contains_key("attackzoffset") {
            float_field(fields, "attackzoffset", 0.0)
        } else {
            -1.0
        },
        ext: 0,
        user: Default::default(),
    }
}

fn collect_user_fields(fields: &BTreeMap<String, UdmfTok<'_>>) -> UdmfUserBag {
    let mut out = UdmfUserBag::default();
    for (key, tok) in fields {
        if !key.to_ascii_lowercase().starts_with("user_") {
            continue;
        }
        let k = key.to_ascii_lowercase();
        match tok {
            UdmfTok::Int(n) => {
                out.ints.insert(k, i32::try_from(*n).unwrap_or(0));
            }
            UdmfTok::Bool(b) => {
                out.ints.insert(k, i32::from(*b));
            }
            UdmfTok::Float(f) => {
                // 16.16 fixed (same as ACS fixed).
                let fixed = ((*f * 65536.0).round()).clamp(i32::MIN as f64, i32::MAX as f64) as i32;
                out.fixed.insert(k, fixed);
            }
            UdmfTok::String(_) | UdmfTok::Ident(_) => {
                // String user props still open — ignore for int/fixed getters.
            }
            _ => {}
        }
    }
    out
}

fn linedef_from_fields(fields: &BTreeMap<String, UdmfTok<'_>>) -> RawLinedef {
    let v1 = int_field(fields, "v1", 0) as u16;
    let v2 = int_field(fields, "v2", 0) as u16;
    let sidefront = int_field(fields, "sidefront", -1);
    let sideback = int_field(fields, "sideback", -1);
    // Doom ML_* + ZDoom extensions (bit values match GZDoom `doomdata.h`).
    let mut flags: i16 = 0;
    if bool_field(fields, "blocking", false) {
        flags |= 0x0001;
    }
    if bool_field(fields, "blockmonsters", false) {
        flags |= 0x0002;
    }
    if bool_field(fields, "twosided", false) {
        flags |= 0x0004;
    }
    if bool_field(fields, "dontpegtop", false) {
        flags |= 0x0008;
    }
    if bool_field(fields, "dontpegbottom", false) {
        flags |= 0x0010;
    }
    if bool_field(fields, "secret", false) {
        flags |= 0x0020;
    }
    if bool_field(fields, "blocksound", false) {
        flags |= 0x0040;
    }
    if bool_field(fields, "dontdraw", false) {
        flags |= 0x0080;
    }
    // `revealed` (ZDoom) = shown on the automap from the start, like `mapped`.
    if bool_field(fields, "mapped", false) || bool_field(fields, "revealed", false) {
        flags |= 0x0100;
    }
    if bool_field(fields, "repeatspecial", false) {
        flags |= 0x0200;
    }
    if bool_field(fields, "monsteractivate", false) {
        flags |= 0x2000; // ML_MONSTERSCANACTIVATE
    }
    if bool_field(fields, "blockplayers", false) {
        flags |= 0x4000; // ML_BLOCK_PLAYERS
    }
    if bool_field(fields, "blockeverything", false) {
        flags |= i16::MIN; // 0x8000 ML_BLOCKEVERYTHING
    }
    RawLinedef {
        v1,
        v2,
        flags,
        special: int_field(fields, "special", 0) as i16,
        tag: int_field(fields, "id", 0) as i16,
        side_front: if sidefront < 0 {
            0xFFFF
        } else {
            sidefront as u16
        },
        side_back: if sideback < 0 {
            0xFFFF
        } else {
            sideback as u16
        },
        args: [
            int_field(fields, "arg0", 0) as i32,
            int_field(fields, "arg1", 0) as i32,
            int_field(fields, "arg2", 0) as i32,
            int_field(fields, "arg3", 0) as i32,
            int_field(fields, "arg4", 0) as i32,
        ],
        hexen: true,
        activation: udmf_activation(fields),
        arg0_str: match fields.get("arg0str") {
            Some(UdmfTok::String(s)) => s.to_string(),
            Some(UdmfTok::Ident(s)) => (*s).to_owned(),
            _ => String::new(),
        },
        midtex3d: bool_field(fields, "midtex3d", false),
        midtex3d_impassible: bool_field(fields, "midtex3dimpassible", false),
        check_switch_range: bool_field(fields, "checkswitchrange", false),
        first_side_only: bool_field(fields, "firstsideonly", false),
        block_floaters: bool_field(fields, "blockfloaters", false),
        lock_number: int_field(fields, "locknumber", 0) as i32,
        block_projectiles: bool_field(fields, "blockprojectiles", false),
        block_sight: bool_field(fields, "blocksight", false),
        block_hitscan: bool_field(fields, "blockhitscan", false),
        extra_ids: moreids_field(fields),
        // Strife `translucent` (75 %) / `transparent` (25 %) when no explicit alpha.
        alpha: if fields.contains_key("alpha") {
            float_field(fields, "alpha", 1.0) as f32
        } else if bool_field(fields, "transparent", false) {
            0.25
        } else if bool_field(fields, "translucent", false) {
            0.75
        } else {
            1.0
        },
        health: int_field(fields, "health", 0) as i32,
        health_group: int_field(fields, "healthgroup", 0) as i32,
        automap_style: int_field(fields, "automapstyle", 0) as i32,
        clip_midtex: bool_field(fields, "clipmidtex", false),
        wrap_midtex: bool_field(fields, "wrapmidtex", false),
        mid_additive: match fields.get("renderstyle") {
            Some(UdmfTok::String(s)) => {
                matches!(s.to_ascii_lowercase().as_str(), "add" | "additive")
            }
            Some(UdmfTok::Ident(s)) => {
                matches!(s.to_ascii_lowercase().as_str(), "add" | "additive")
            }
            _ => false,
        },
        ext: 0,
        user: Default::default(),
    }
}

fn moreids_field(fields: &BTreeMap<String, UdmfTok<'_>>) -> Vec<i32> {
    match fields.get("moreids") {
        Some(UdmfTok::String(s)) => parse_id_list(s.as_ref()),
        Some(UdmfTok::Ident(s)) => parse_id_list(s),
        Some(UdmfTok::Int(n)) => {
            let n = *n as i32;
            if n != 0 { vec![n] } else { Vec::new() }
        }
        _ => Vec::new(),
    }
}

fn parse_id_list(s: &str) -> Vec<i32> {
    s.split(|c: char| c.is_whitespace() || c == ',')
        .filter_map(|p| {
            let t = p.trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<i32>().ok().filter(|&n| n != 0)
            }
        })
        .collect()
}

fn udmf_activation(fields: &BTreeMap<String, UdmfTok<'_>>) -> u32 {
    use crate::spac;
    let mut a = 0u32;
    if bool_field(fields, "playercross", false) {
        a |= spac::CROSS;
    }
    if bool_field(fields, "playeruse", false) {
        a |= spac::USE;
    }
    if bool_field(fields, "monstercross", false) {
        a |= spac::MCROSS;
    }
    if bool_field(fields, "impact", false) {
        a |= spac::IMPACT;
    }
    if bool_field(fields, "playerpush", false) {
        a |= spac::PUSH;
    }
    if bool_field(fields, "missilecross", false) {
        a |= spac::PCROSS;
    }
    if bool_field(fields, "monsteruse", false) {
        a |= spac::MUSE;
    }
    if bool_field(fields, "monsterpush", false) {
        a |= spac::MPUSH;
    }
    if bool_field(fields, "anycross", false) {
        a |= spac::ANYCROSS;
    }
    if bool_field(fields, "playeruseback", false) {
        a |= spac::USE | spac::USEBACK;
    }
    if bool_field(fields, "damagespecial", false) {
        a |= spac::DAMAGE;
    }
    if bool_field(fields, "deathspecial", false) {
        a |= spac::DEATH;
    }
    a
}

fn sidedef_from_fields(fields: &BTreeMap<String, UdmfTok<'_>>) -> RawSidedef {
    let scale_x = if fields.contains_key("scalex_mid") {
        float_field(fields, "scalex_mid", 1.0) as f32
    } else if fields.contains_key("scalex") {
        float_field(fields, "scalex", 1.0) as f32
    } else {
        1.0
    };
    let scale_y = if fields.contains_key("scaley_mid") {
        float_field(fields, "scaley_mid", 1.0) as f32
    } else if fields.contains_key("scaley") {
        float_field(fields, "scaley", 1.0) as f32
    } else {
        1.0
    };
    let scale_x_top = if fields.contains_key("scalex_top") {
        float_field(fields, "scalex_top", 1.0) as f32
    } else {
        scale_x
    };
    let scale_y_top = if fields.contains_key("scaley_top") {
        float_field(fields, "scaley_top", 1.0) as f32
    } else {
        scale_y
    };
    let scale_x_bottom = if fields.contains_key("scalex_bottom") {
        float_field(fields, "scalex_bottom", 1.0) as f32
    } else {
        scale_x
    };
    let scale_y_bottom = if fields.contains_key("scaley_bottom") {
        float_field(fields, "scaley_bottom", 1.0) as f32
    } else {
        scale_y
    };
    let x_base = int_field(fields, "offsetx", 0) as i16;
    let y_base = int_field(fields, "offsety", 0) as i16;
    let x_mid = if fields.contains_key("offsetx_mid") {
        int_field(fields, "offsetx_mid", 0) as i16
    } else {
        x_base
    };
    let y_mid = if fields.contains_key("offsety_mid") {
        int_field(fields, "offsety_mid", 0) as i16
    } else {
        y_base
    };
    let x_top = if fields.contains_key("offsetx_top") {
        int_field(fields, "offsetx_top", 0) as i16
    } else {
        x_base
    };
    let y_top = if fields.contains_key("offsety_top") {
        int_field(fields, "offsety_top", 0) as i16
    } else {
        y_base
    };
    let x_bot = if fields.contains_key("offsetx_bottom") {
        int_field(fields, "offsetx_bottom", 0) as i16
    } else {
        x_base
    };
    let y_bot = if fields.contains_key("offsety_bottom") {
        int_field(fields, "offsety_bottom", 0) as i16
    } else {
        y_base
    };
    RawSidedef {
        x_offset: x_mid,
        y_offset: y_mid,
        upper: str_field(fields, "texturetop"),
        lower: str_field(fields, "texturebottom"),
        middle: str_field(fields, "texturemiddle"),
        sector: int_field(fields, "sector", 0) as u16,
        light: if fields.contains_key("light") {
            int_field(fields, "light", 0) as i16
        } else {
            i16::MIN
        },
        light_absolute: bool_field(fields, "lightabsolute", false),
        light_top: if fields.contains_key("light_top") {
            int_field(fields, "light_top", 0) as i16
        } else {
            i16::MIN
        },
        light_mid: if fields.contains_key("light_mid") {
            int_field(fields, "light_mid", 0) as i16
        } else {
            i16::MIN
        },
        light_bottom: if fields.contains_key("light_bottom") {
            int_field(fields, "light_bottom", 0) as i16
        } else {
            i16::MIN
        },
        light_absolute_top: bool_field(fields, "lightabsolute_top", false),
        light_absolute_mid: bool_field(fields, "lightabsolute_mid", false),
        light_absolute_bottom: bool_field(fields, "lightabsolute_bottom", false),
        scale_x,
        scale_y,
        scale_x_top,
        scale_y_top,
        scale_x_bottom,
        scale_y_bottom,
        x_offset_top: x_top,
        y_offset_top: y_top,
        x_offset_bottom: x_bot,
        y_offset_bottom: y_bot,
        no_fake_contrast: bool_field(fields, "nofakecontrast", false),
        smooth_lighting: bool_field(fields, "smoothlighting", false),
        scroll_x: float_field(fields, "xscroll", 0.0) as f32,
        scroll_y: float_field(fields, "yscroll", 0.0) as f32,
        clip_midtex: bool_field(fields, "clipmidtex", false),
        wrap_midtex: bool_field(fields, "wrapmidtex", false),
        no_decals: bool_field(fields, "nodecals", false),
        alpha: fields
            .contains_key("alpha")
            .then(|| float_field(fields, "alpha", 1.0) as f32),
        block_rendering: bool_field(fields, "blockrendering", false),
        user: Default::default(),
    }
}

fn sector_from_fields(fields: &BTreeMap<String, UdmfTok<'_>>) -> RawSector {
    let floor_plane = plane_from_fields(fields, "floorplane");
    let ceiling_plane = plane_from_fields(fields, "ceilingplane");
    RawSector {
        floor_height: int_field(fields, "heightfloor", 0) as i16,
        ceiling_height: int_field(fields, "heightceiling", 0) as i16,
        floor_pic: str_field(fields, "texturefloor"),
        ceiling_pic: str_field(fields, "textureceiling"),
        light: int_field(fields, "lightlevel", 160) as i16,
        special: int_field(fields, "special", 0) as i16,
        tag: int_field(fields, "id", 0) as i16,
        floor_plane,
        ceiling_plane,
        extra_ids: moreids_field(fields),
        floor_x_pan: float_field(fields, "xpanningfloor", 0.0),
        floor_y_pan: float_field(fields, "ypanningfloor", 0.0),
        ceiling_x_pan: float_field(fields, "xpanningceiling", 0.0),
        ceiling_y_pan: float_field(fields, "ypanningceiling", 0.0),
        floor_rotation: float_field(fields, "rotationfloor", 0.0),
        ceiling_rotation: float_field(fields, "rotationceiling", 0.0),
        floor_x_scale: float_field(fields, "xscalefloor", 1.0),
        floor_y_scale: float_field(fields, "yscalefloor", 1.0),
        ceiling_x_scale: float_field(fields, "xscaleceiling", 1.0),
        ceiling_y_scale: float_field(fields, "yscaleceiling", 1.0),
        gravity: if fields.contains_key("gravity") {
            float_field(fields, "gravity", 1.0)
        } else {
            -1.0
        },
        friction: int_field(fields, "friction", 0) as i32,
        damage_amount: int_field(fields, "damageamount", 0) as i32,
        damage_type: match fields.get("damagetype") {
            Some(UdmfTok::String(s)) => s.to_string(),
            Some(UdmfTok::Ident(s)) => (*s).to_owned(),
            _ => String::new(),
        },
        damage_interval: int_field(fields, "damageinterval", 0) as i32,
        leakiness: int_field(fields, "leakiness", 0) as i32,
        silent: bool_field(fields, "silent", false),
        sound_sequence: match fields.get("soundsequence") {
            Some(UdmfTok::String(s)) => s.to_string(),
            Some(UdmfTok::Ident(s)) => (*s).to_owned(),
            _ => String::new(),
        },
        waterzone: bool_field(fields, "waterzone", false),
        drop_actors: bool_field(fields, "dropactors", true),
        no_falling_damage: bool_field(fields, "nofallingdamage", false),
        light_color: if fields.contains_key("lightcolor") {
            int_field(fields, "lightcolor", 0xFF_FF_FF) as i32
        } else {
            -1
        },
        fade_color: if fields.contains_key("fadecolor") {
            int_field(fields, "fadecolor", 0) as i32
        } else {
            -1
        },
        fog_density: int_field(fields, "fogdensity", 0) as i32,
        desaturation: int_field(fields, "desaturation", 0) as i32,
        color_floor: color_field(fields, "color_floor"),
        color_ceiling: color_field(fields, "color_ceiling"),
        color_walltop: color_field(fields, "color_walltop"),
        color_wallbottom: color_field(fields, "color_wallbottom"),
        color_sprites: color_field(fields, "color_sprites"),
        health_floor: int_field(fields, "healthfloor", 0) as i32,
        health_ceiling: int_field(fields, "healthceiling", 0) as i32,
        health_3d: int_field(fields, "health3d", 0) as i32,
        floor_alpha: {
            let base = if fields.contains_key("alpha") {
                float_field(fields, "alpha", 1.0) as f32
            } else {
                1.0
            };
            if fields.contains_key("alphafloor") {
                float_field(fields, "alphafloor", 1.0) as f32
            } else {
                base
            }
        },
        ceiling_alpha: {
            let base = if fields.contains_key("alpha") {
                float_field(fields, "alpha", 1.0) as f32
            } else {
                1.0
            };
            if fields.contains_key("alphaceiling") {
                float_field(fields, "alphaceiling", 1.0) as f32
            } else {
                base
            }
        },
        floor_additive: sector_renderstyle_additive(fields),
        ceiling_additive: sector_renderstyle_additive(fields),
        ext: 0,
        user: Default::default(),
    }
}

fn sector_renderstyle_additive(fields: &BTreeMap<String, UdmfTok<'_>>) -> bool {
    match fields.get("renderstyle") {
        Some(UdmfTok::String(s)) => {
            matches!(s.to_ascii_lowercase().as_str(), "add" | "additive")
        }
        Some(UdmfTok::Ident(s)) => {
            matches!(s.to_ascii_lowercase().as_str(), "add" | "additive")
        }
        _ => false,
    }
}

/// UDMF RGB colour field (`-1` when unset).
fn color_field(fields: &BTreeMap<String, UdmfTok<'_>>, key: &str) -> i32 {
    if fields.contains_key(key) {
        int_field(fields, key, 0) as i32
    } else {
        -1
    }
}

/// Read `prefix_a..d` into a plane when any component is present.
fn plane_from_fields(fields: &BTreeMap<String, UdmfTok<'_>>, prefix: &str) -> Option<[f64; 4]> {
    let keys = [
        format!("{prefix}_a"),
        format!("{prefix}_b"),
        format!("{prefix}_c"),
        format!("{prefix}_d"),
    ];
    if !keys.iter().any(|k| fields.contains_key(k.as_str())) {
        return None;
    }
    Some([
        float_field(fields, &keys[0], 0.0),
        float_field(fields, &keys[1], 0.0),
        float_field(fields, &keys[2], 1.0),
        float_field(fields, &keys[3], 0.0),
    ])
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parse_linedef_ml_flags() {
        let text = r#"
namespace = "zdoom";
linedef {
  v1 = 0; v2 = 1; sidefront = 0; sideback = 1;
  twosided = true;
  blocking = true;
  blockmonsters = true;
  blockplayers = true;
  secret = true;
  dontpegtop = true;
}
"#;
        let m = parse_udmf(text).unwrap();
        let f = m.linedefs[0].flags;
        assert_ne!(f & 0x0001, 0, "blocking");
        assert_ne!(f & 0x0002, 0, "blockmonsters");
        assert_ne!(f & 0x0004, 0, "twosided");
        assert_ne!(f & 0x0008, 0, "dontpegtop");
        assert_ne!(f & 0x0020, 0, "secret");
        assert_ne!(f & 0x4000, 0, "blockplayers");
    }

    #[test]
    fn parse_clipmidtex_wrapmidtex_line_and_side() {
        let text = r#"
namespace = "zdoom";
sidedef {
  sector = 0;
  texturemiddle = "MID";
  clipmidtex = true;
  wrapmidtex = true;
  nodecals = true;
}
linedef {
  v1 = 0; v2 = 1; sidefront = 0; id = 4;
  clipmidtex = true;
  wrapmidtex = true;
}
"#;
        let m = parse_udmf(text).unwrap();
        assert!(m.linedefs[0].clip_midtex);
        assert!(m.linedefs[0].wrap_midtex);
        assert!(m.sidedefs[0].clip_midtex);
        assert!(m.sidedefs[0].wrap_midtex);
        assert!(m.sidedefs[0].no_decals);
        assert_eq!(m.sidedefs[0].alpha, None);
        assert!(!m.sidedefs[0].block_rendering);
    }

    #[test]
    fn parse_linedef_health_and_automapstyle() {
        let text = r#"
namespace = "zdoom";
linedef {
  v1 = 0; v2 = 1; sidefront = 0; id = 7;
  health = 50;
  healthgroup = 3;
  automapstyle = 8;
  damagespecial = true;
  deathspecial = true;
}
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.linedefs[0].health, 50);
        assert_eq!(m.linedefs[0].health_group, 3);
        assert_eq!(m.linedefs[0].automap_style, 8);
        assert_ne!(m.linedefs[0].activation & crate::spac::DAMAGE, 0);
        assert_ne!(m.linedefs[0].activation & crate::spac::DEATH, 0);
    }

    #[test]
    fn parse_linedef_alpha_and_side_nofakecontrast() {
        let text = r#"
namespace = "zdoom";
sidedef {
  sector = 0;
  texturemiddle = "A";
  nofakecontrast = true;
  smoothlighting = true;
}
linedef {
  v1 = 0; v2 = 1; sidefront = 0; id = 3;
  alpha = 0.25;
  renderstyle = "Add";
}
"#;
        let m = parse_udmf(text).unwrap();
        assert!((m.linedefs[0].alpha - 0.25).abs() < 1e-6);
        assert!(m.linedefs[0].mid_additive);
        assert!(m.sidedefs[0].no_fake_contrast);
        assert!(m.sidedefs[0].smooth_lighting);
    }

    #[test]
    fn parse_sidedef_xscroll_yscroll() {
        let text = r#"
namespace = "zdoom";
sidedef {
  sector = 0;
  texturemiddle = "A";
  xscroll = 1.5;
  yscroll = -0.25;
}
"#;
        let m = parse_udmf(text).unwrap();
        assert!((m.sidedefs[0].scroll_x - 1.5).abs() < 1e-6);
        assert!((m.sidedefs[0].scroll_y - (-0.25)).abs() < 1e-6);
    }

    #[test]
    fn parse_sidedef_light_and_scale() {
        let text = r#"
namespace = "zdoom";
sidedef {
  sector = 0;
  texturemiddle = "STARTAN2";
  light = 32;
  lightabsolute = true;
  light_top = 200;
  light_mid = 100;
  light_bottom = 16;
  lightabsolute_top = true;
  lightabsolute_bottom = true;
  scalex_mid = 2.0;
  scaley_mid = 0.5;
  scalex_top = 3.0;
  scaley_bottom = 0.25;
  offsetx = 8;
  offsety_top = 16;
  offsetx_bottom = 24;
}
"#;
        let m = parse_udmf(text).unwrap();
        let s = &m.sidedefs[0];
        assert_eq!(s.light, 32);
        assert!(s.light_absolute);
        assert_eq!(s.light_top, 200);
        assert_eq!(s.light_mid, 100);
        assert_eq!(s.light_bottom, 16);
        assert!(s.light_absolute_top);
        assert!(!s.light_absolute_mid);
        assert!(s.light_absolute_bottom);
        assert!((s.scale_x - 2.0).abs() < 1e-6);
        assert!((s.scale_y - 0.5).abs() < 1e-6);
        assert!((s.scale_x_top - 3.0).abs() < 1e-6);
        assert!((s.scale_y_bottom - 0.25).abs() < 1e-6);
        assert_eq!(s.x_offset, 8);
        assert_eq!(s.y_offset_top, 16);
        assert_eq!(s.x_offset_bottom, 24);
        assert_eq!(s.x_offset_top, 8); // falls back to offsetx
        let bare = parse_udmf(
            r#"
namespace = "zdoom";
sidedef { sector = 0; texturemiddle = "A"; scalex = 1.5; scaley = 1.25; }
"#,
        )
        .unwrap();
        assert!((bare.sidedefs[0].scale_x - 1.5).abs() < 1e-6);
        assert!((bare.sidedefs[0].scale_y - 1.25).abs() < 1e-6);
        assert_eq!(bare.sidedefs[0].light, i16::MIN);
    }

    #[test]
    fn parse_sector_gravity_friction_damage() {
        let text = r#"
namespace = "zdoom";
sector {
  heightfloor = 0;
  heightceiling = 128;
  texturefloor = "F";
  textureceiling = "C";
  id = 5;
  gravity = 0.5;
  friction = 100;
  damageamount = 10;
  damagetype = "Fire";
  damageinterval = 16;
  leakiness = 64;
  silent = true;
  waterzone = true;
  dropactors = false;
  nofallingdamage = true;
}
"#;
        let m = parse_udmf(text).unwrap();
        let s = &m.sectors[0];
        assert!((s.gravity - 0.5).abs() < 1e-9);
        assert_eq!(s.friction, 100);
        assert_eq!(s.damage_amount, 10);
        assert_eq!(s.damage_type, "Fire");
        assert_eq!(s.damage_interval, 16);
        assert_eq!(s.leakiness, 64);
        assert!(s.silent);
        assert!(s.waterzone);
        assert!(!s.drop_actors);
        assert!(s.no_falling_damage);
        assert_eq!(s.light_color, -1);
        assert_eq!(s.fade_color, -1);
        assert_eq!(s.fog_density, 0);
        assert!((s.floor_alpha - 1.0).abs() < 1e-6);
        assert!((s.ceiling_alpha - 1.0).abs() < 1e-6);
        assert!(!s.floor_additive);
        let alpha = parse_udmf(
            r#"
namespace = "zdoom";
sector {
  heightfloor = 0; heightceiling = 128;
  texturefloor = "F"; textureceiling = "C";
  alphafloor = 0.5; alphaceiling = 0.25; renderstyle = "Add";
}
"#,
        )
        .unwrap();
        assert!((alpha.sectors[0].floor_alpha - 0.5).abs() < 1e-6);
        assert!((alpha.sectors[0].ceiling_alpha - 0.25).abs() < 1e-6);
        assert!(alpha.sectors[0].floor_additive);
        assert!(alpha.sectors[0].ceiling_additive);
        let def = parse_udmf(
            r#"
namespace = "zdoom";
sector {
  heightfloor = 0; heightceiling = 128;
  texturefloor = "F"; textureceiling = "C";
}
"#,
        )
        .unwrap();
        assert!(def.sectors[0].drop_actors, "dropactors defaults true");
    }

    #[test]
    fn parse_sector_light_fade_fog() {
        let text = r#"
namespace = "zdoom";
sector {
  heightfloor = 0; heightceiling = 128;
  texturefloor = "F"; textureceiling = "C";
  lightcolor = 0xFF8040;
  fadecolor = 0x102030;
  fogdensity = 128;
  desaturation = 64;
  healthfloor = 100;
  healthceiling = 50;
  health3d = 25;
  color_floor = 0xFF0000;
  color_ceiling = 0x00FF00;
  color_walltop = 0x0000FF;
  color_wallbottom = 0x808080;
  color_sprites = 0xFFFFFF;
}
"#;
        let m = parse_udmf(text).unwrap();
        let s = &m.sectors[0];
        assert_eq!(s.light_color, 0xFF_80_40);
        assert_eq!(s.fade_color, 0x10_20_30);
        assert_eq!(s.fog_density, 128);
        assert_eq!(s.desaturation, 64);
        assert_eq!(s.health_floor, 100);
        assert_eq!(s.health_ceiling, 50);
        assert_eq!(s.health_3d, 25);
        assert_eq!(s.color_floor, 0xFF_00_00);
        assert_eq!(s.color_ceiling, 0x00_FF_00);
        assert_eq!(s.color_walltop, 0x00_00_FF);
        assert_eq!(s.color_wallbottom, 0x80_80_80);
        assert_eq!(s.color_sprites, 0xFF_FF_FF);
    }

    #[test]
    fn parse_sector_uv_xform() {
        let text = r#"
namespace = "zdoom";
sector {
  heightfloor = 0;
  heightceiling = 128;
  texturefloor = "F";
  textureceiling = "C";
  xpanningfloor = 32;
  ypanningfloor = 16;
  xpanningceiling = 8;
  ypanningceiling = 4;
  rotationfloor = 45;
  rotationceiling = -90;
  xscalefloor = 2;
  yscalefloor = 0.5;
  xscaleceiling = 1.5;
  yscaleceiling = 3;
}
"#;
        let m = parse_udmf(text).unwrap();
        let s = &m.sectors[0];
        assert!((s.floor_x_pan - 32.0).abs() < 1e-9);
        assert!((s.floor_y_pan - 16.0).abs() < 1e-9);
        assert!((s.ceiling_x_pan - 8.0).abs() < 1e-9);
        assert!((s.ceiling_y_pan - 4.0).abs() < 1e-9);
        assert!((s.floor_rotation - 45.0).abs() < 1e-9);
        assert!((s.ceiling_rotation - -90.0).abs() < 1e-9);
        assert!((s.floor_x_scale - 2.0).abs() < 1e-9);
        assert!((s.floor_y_scale - 0.5).abs() < 1e-9);
        assert!((s.ceiling_x_scale - 1.5).abs() < 1e-9);
        assert!((s.ceiling_y_scale - 3.0).abs() < 1e-9);
        let g = crate::geom::link_geometry(
            "MAP01",
            crate::geom::MapFormat::Udmf,
            m.vertices,
            m.linedefs,
            m.sidedefs,
            m.sectors,
            m.things,
            [0; 16],
        );
        let sec = &g.sectors[0];
        assert!((sec.floor_u_off - 32.0).abs() < 1e-9);
        assert!((sec.floor_v_off - 16.0).abs() < 1e-9);
        assert!((sec.ceiling_u_off - 8.0).abs() < 1e-9);
        assert!((sec.ceiling_v_off - 4.0).abs() < 1e-9);
        assert!((sec.floor_angle - 45.0).abs() < 1e-9);
        assert!((sec.ceiling_angle - -90.0).abs() < 1e-9);
        assert!((sec.floor_scale_u - 2.0).abs() < 1e-9);
        assert!((sec.floor_scale_v - 0.5).abs() < 1e-9);
        assert!((sec.ceiling_scale_u - 1.5).abs() < 1e-9);
        assert!((sec.ceiling_scale_v - 3.0).abs() < 1e-9);
    }

    #[test]
    fn parse_sector_moreids() {
        let text = r#"
namespace = "zdoom";
sector {
  heightfloor = 0;
  heightceiling = 128;
  texturefloor = "F";
  textureceiling = "C";
  id = 3;
  moreids = "10 11";
}
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.sectors[0].tag, 3);
        assert_eq!(m.sectors[0].extra_ids, vec![10, 11]);
        let g = crate::geom::link_geometry(
            "MAP01",
            crate::geom::MapFormat::Udmf,
            m.vertices,
            m.linedefs,
            m.sidedefs,
            m.sectors,
            m.things,
            [0; 16],
        );
        assert!(g.sectors[0].tags.contains(&3));
        assert!(g.sectors[0].tags.contains(&10));
        assert!(g.sectors[0].tags.contains(&11));
    }

    #[test]
    fn parse_sector_floorplane() {
        let text = r#"
namespace = "zdoom";
sector {
  heightfloor = 0;
  heightceiling = 128;
  texturefloor = "FLOOR0_1";
  textureceiling = "CEIL1_1";
  floorplane_a = 0.0;
  floorplane_b = 0.1;
  floorplane_c = 1.0;
  floorplane_d = 0.0;
}
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.sectors[0].floor_plane, Some([0.0, 0.1, 1.0, 0.0]));
    }

    #[test]
    fn parse_thing_friend_flag() {
        let text = r#"
namespace = "zdoom";
thing { x = 0.0; y = 0.0; type = 3004; friend = true; }
"#;
        let m = parse_udmf(text).unwrap();
        assert_ne!(m.things[0].flags & 128, 0);
    }

    #[test]
    fn parse_thing_friendlyseeblocks() {
        let text = r#"
namespace = "zdoom";
thing { x = 0.0; y = 0.0; type = 3004; friendlyseeblocks = 4; }
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.things[0].friendly_see_blocks, 4);
    }

    #[test]
    fn parse_thing_health_and_score() {
        let text = r#"
namespace = "zdoom";
thing { x = 0.0; y = 0.0; type = 3004; health = 50; score = 100; }
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.things[0].health, 50);
        assert_eq!(m.things[0].score, 100);
        let bare = parse_udmf(
            r#"
namespace = "zdoom";
thing { x = 0.0; y = 0.0; type = 1; }
"#,
        )
        .unwrap();
        assert_eq!(bare.things[0].health, 0);
        assert_eq!(bare.things[0].score, i32::MIN);
    }

    #[test]
    fn parse_thing_gravity_scale_alpha_pitch_roll() {
        let text = r#"
namespace = "zdoom";
thing {
  x = 0.0; y = 0.0; type = 3004;
  gravity = 0.5; scale = 2.0; alpha = 0.25; pitch = 15; roll = -10;
}
"#;
        let m = parse_udmf(text).unwrap();
        let t = &m.things[0];
        assert!((t.gravity - 0.5).abs() < 1e-9);
        assert!((t.scale_x - 2.0).abs() < 1e-9);
        assert!((t.scale_y - 2.0).abs() < 1e-9);
        assert!((t.alpha - 0.25).abs() < 1e-9);
        assert_eq!(t.pitch, 15);
        assert_eq!(t.roll, -10);
        let xy = parse_udmf(
            r#"
namespace = "zdoom";
thing { x = 0.0; y = 0.0; type = 1; scalex = 1.5; scaley = 0.5; }
"#,
        )
        .unwrap();
        assert!((xy.things[0].scale_x - 1.5).abs() < 1e-9);
        assert!((xy.things[0].scale_y - 0.5).abs() < 1e-9);
    }

    #[test]
    fn parse_thing_countsecret_nocount() {
        let text = r#"
namespace = "zdoom";
thing { x = 0.0; y = 0.0; type = 2014; countsecret = true; nocount = true; }
"#;
        let m = parse_udmf(text).unwrap();
        assert!(m.things[0].count_secret);
        assert!(m.things[0].no_count);
    }

    #[test]
    fn parse_thing_arg0str() {
        let text = r#"
namespace = "zdoom";
thing { x = 0.0; y = 0.0; type = 1; special = 80; arg0str = "Go"; }
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.things[0].arg0_str, "Go");
        assert_eq!(m.things[0].special, 80);
    }

    #[test]
    fn parse_thing_conversation() {
        let text = r#"
namespace = "zdoom";
thing { x = 0.0; y = 0.0; type = 1; conversation = 42; }
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.things[0].conversation, 42);
    }

    #[test]
    fn parse_thing_accuracy_stamina_species() {
        let text = r#"
namespace = "zdoom";
thing {
  x = 0.0; y = 0.0; type = 3004;
  accuracy = 50; stamina = 25; species = "Zombie";
  meleerange = 96.0; viewheight = 48.0;
  mass = 400; speed = 12.0;
  friction = 0.75; damagefactor = 0.5;
  maxstepheight = 32.0; maxdropoffheight = 64.0;
  floatspeed = 4.0; fastspeed = 20.0; jumpz = 9.0; attackzoffset = 8.0;
}
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.things[0].accuracy, 50);
        assert_eq!(m.things[0].stamina, 25);
        assert_eq!(m.things[0].species, "Zombie");
        assert!((m.things[0].melee_range - 96.0).abs() < 1e-9);
        assert!((m.things[0].view_height - 48.0).abs() < 1e-9);
        assert_eq!(m.things[0].mass, 400);
        assert!((m.things[0].speed - 12.0).abs() < 1e-9);
        assert!((m.things[0].friction - 0.75).abs() < 1e-9);
        assert!((m.things[0].damage_factor - 0.5).abs() < 1e-9);
        assert!((m.things[0].max_step_height - 32.0).abs() < 1e-9);
        assert!((m.things[0].max_dropoff_height - 64.0).abs() < 1e-9);
        assert!((m.things[0].float_speed - 4.0).abs() < 1e-9);
        assert!((m.things[0].fast_speed - 20.0).abs() < 1e-9);
        assert!((m.things[0].jump_z - 9.0).abs() < 1e-9);
        assert!((m.things[0].attack_z_offset - 8.0).abs() < 1e-9);
    }

    #[test]
    fn parse_linedef_arg0str() {
        let text = r#"
namespace = "zdoom";
linedef { v1 = 0; v2 = 1; sidefront = 0; special = 80; arg0str = "Elevator"; }
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.linedefs[0].arg0_str, "Elevator");
        assert_eq!(m.linedefs[0].special, 80);
    }

    #[test]
    fn parse_thing_fillcolor_renderstyle_floatbob() {
        let text = r#"
namespace = "zdoom";
thing {
  x = 0.0; y = 0.0; type = 3004;
  fillcolor = 0xff00aa; renderstyle = "Add"; floatbobphase = 12;
}
"#;
        let m = parse_udmf(text).unwrap();
        let t = &m.things[0];
        assert_eq!(t.fill_color, 0xff00aa);
        assert_eq!(t.render_style, "Add");
        assert_eq!(t.float_bob_phase, 12);
        let bare = parse_udmf(
            r#"
namespace = "zdoom";
thing { x = 0.0; y = 0.0; type = 1; }
"#,
        )
        .unwrap();
        assert_eq!(bare.things[0].fill_color, -1);
        assert!(bare.things[0].render_style.is_empty());
        assert_eq!(bare.things[0].float_bob_phase, -1);
    }

    #[test]
    fn parse_minimal_udmf() {
        let text = r#"
namespace = "zdoom";
vertex { x = 0.0; y = 0.0; }
vertex { x = 64.0; y = 0.0; }
sector {
  heightfloor = 0;
  heightceiling = 128;
  texturefloor = "FLOOR0_1";
  textureceiling = "CEIL1_1";
  lightlevel = 160;
}
sidedef { sector = 0; texturemiddle = "STARTAN2"; }
linedef { v1 = 0; v2 = 1; sidefront = 0; }
thing { x = 32.0; y = 32.0; type = 1; angle = 90; }
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.namespace, "zdoom");
        assert_eq!(m.vertices.len(), 2);
        assert_eq!(m.sectors.len(), 1);
        assert_eq!(m.sectors[0].ceiling_height, 128);
        assert_eq!(m.things[0].type_id, 1);
    }

    #[test]
    fn parse_user_props() {
        let text = r#"
namespace = "zdoom";
linedef {
  v1 = 0; v2 = 1; sidefront = 0; id = 7;
  user_score = 42;
  user_scale = 1.5;
}
sector {
  heightfloor = 0; heightceiling = 128;
  texturefloor = "F"; textureceiling = "C";
  user_wet = true;
}
"#;
        let m = parse_udmf(text).unwrap();
        assert_eq!(m.user.lines[0].ints.get("user_score"), Some(&42));
        assert_eq!(
            m.user.lines[0].fixed.get("user_scale"),
            Some(&((1.5_f64 * 65536.0).round() as i32))
        );
        assert_eq!(m.user.sectors[0].ints.get("user_wet"), Some(&1));
    }

    #[test]
    fn parse_midtex3d() {
        let text = r#"
namespace = "zdoom";
linedef { v1 = 0; v2 = 1; sidefront = 0; sideback = 1; midtex3d = true; midtex3dimpassible = true; checkswitchrange = true; firstsideonly = true; blockfloaters = true; locknumber = 129; blockprojectiles = true; blocksight = true; blockhitscan = true; moreids = "7 8"; }
"#;
        let m = parse_udmf(text).unwrap();
        assert!(m.linedefs[0].midtex3d);
        assert!(m.linedefs[0].midtex3d_impassible);
        assert!(m.linedefs[0].check_switch_range);
        assert!(m.linedefs[0].first_side_only);
        assert!(m.linedefs[0].block_floaters);
        assert_eq!(m.linedefs[0].lock_number, 129);
        assert!(m.linedefs[0].block_projectiles);
        assert!(m.linedefs[0].block_sight);
        assert!(m.linedefs[0].block_hitscan);
        assert_eq!(m.linedefs[0].extra_ids, vec![7, 8]);
    }

    #[test]
    fn vertex_heights_slope_a_triangular_sector() {
        let text = r#"namespace = "zdoom";
vertex { x = 0.0; y = 0.0; zfloor = 64.0; }
vertex { x = 128.0; y = 0.0; }
vertex { x = 0.0; y = 128.0; zceiling = 96.0; }
sector { heightfloor = 0; heightceiling = 128; texturefloor = "F"; textureceiling = "C"; }
sidedef { sector = 0; }
linedef { v1 = 0; v2 = 1; sidefront = 0; }
linedef { v1 = 1; v2 = 2; sidefront = 0; }
linedef { v1 = 2; v2 = 0; sidefront = 0; }
"#;
        let map = parse_udmf(text).unwrap();
        let s = &map.sectors[0];
        let [a, b, c, d] = s.floor_plane.expect("floor sloped");
        assert!(c > 0.0, "floor faces up");
        let z = |x: f64, y: f64| -(a * x + b * y + d) / c;
        assert!((z(0.0, 0.0) - 64.0).abs() < 1e-9);
        assert!(z(128.0, 0.0).abs() < 1e-9);
        assert!(z(0.0, 128.0).abs() < 1e-9);
        let [a, b, c, d] = s.ceiling_plane.expect("ceiling sloped");
        assert!(c < 0.0, "ceiling faces down");
        let z = |x: f64, y: f64| -(a * x + b * y + d) / c;
        assert!((z(0.0, 128.0) - 96.0).abs() < 1e-9);
        assert!((z(128.0, 0.0) - 128.0).abs() < 1e-9);
    }

    #[test]
    fn extended_udmf_keys_land_in_ext_flags() {
        let text = r#"namespace = "zdoom";
vertex { x = 0.0; y = 0.0; }
vertex { x = 64.0; y = 0.0; }
sector { heightfloor = 0; heightceiling = 128; texturefloor = "F"; textureceiling = "C"; noattack = true; hidden = true; hurtmonsters = true; }
sidedef { sector = 0; }
linedef { v1 = 0; v2 = 1; sidefront = 0; blockuse = true; jumpover = true; revealed = true; monsteractivate = true; translucent = true; }
thing { x = 0.0; y = 0.0; type = 1; standing = true; coop = false; }
"#;
        let map = parse_udmf(text).unwrap();
        use super::ext::*;
        assert_eq!(
            map.user.line_ext(0),
            LINE_BLOCKUSE | LINE_JUMPOVER | LINE_MONSTERACTIVATE
        );
        assert_eq!(
            map.user.sector_ext(0),
            SECTOR_NOATTACK | SECTOR_HIDDEN | SECTOR_HURTMONSTERS
        );
        assert_eq!(map.user.thing_ext(0), THING_STANDING | THING_NOTCOOP);
        let l = &map.linedefs[0];
        assert_ne!(l.flags & 0x0100, 0, "revealed = mapped");
        assert_ne!(l.flags & 0x2000, 0, "monsteractivate");
        assert!((l.alpha - 0.75).abs() < 1e-6, "strife translucent");
        assert_eq!(map.user.line_ext(9), 0);
    }
}
