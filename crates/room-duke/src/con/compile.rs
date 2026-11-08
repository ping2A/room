//! CON 1.5 compiler — keyword emit/rewind matching Chocolate `GAMEDEF.C`.
//!
//! GPL game-code translation is allowed. Script pointers are stored as word
//! indices into [`ScriptImage::script`] (relocatable).

use super::keywords::keyword_index;
use super::lex::Lexer;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Atomic sound limit (`NUM_SOUNDS`).
pub const NUM_SOUNDS: usize = 450;
/// Quote limit (`NUMOFFIRSTTIMEACTIVE`).
pub const NUM_QUOTES: usize = 192;
/// Tile limit for `actorscrptr` / `actortype` (Chocolate `MAXTILES`).
pub const MAX_TILES: usize = 9216;

/// One `definesound` entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundDef {
    /// VOC / file name (case preserved).
    pub file: String,
    /// Pitch start.
    pub ps: i32,
    /// Pitch end.
    pub pe: i32,
    /// Priority.
    pub pr: i32,
    /// Type / flags.
    pub m: i32,
    /// Volume offset.
    pub vo: i32,
}

/// One `definelevelname` slot (`vol * 11 + level`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LevelDef {
    /// Map file name (`E1L1.MAP`).
    pub file: String,
    /// Display name (uppercased).
    pub name: String,
    /// Par time in Chocolate tic units.
    pub partime: i32,
    /// 3D Realms designer time in Chocolate tic units.
    pub designertime: i32,
}

/// Detected CON dialect from `gamestartup` arity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConVersion {
    /// 1.3 / 1.3d style (shorter `gamestartup`).
    V13 = 13,
    /// 1.4 / 1.5 Atomic.
    #[default]
    V15 = 15,
}

/// A CON symbol binding (`define` immediate or script address).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label {
    /// Immediate value or word index into [`ScriptImage::script`].
    pub value: i32,
    /// True when `value` is a script word index (action/move/ai/state).
    pub is_ptr: bool,
}

/// Compiled script image.
#[derive(Debug, Clone, Default)]
pub struct ScriptImage {
    /// `script[0]` = end word index (Chocolate stores an end pointer).
    pub script: Vec<i32>,
    /// Parallel to `script`: true when the word holds a relocated script pointer.
    pub script_ptr: Vec<bool>,
    /// `define` values and `action`/`move`/`ai`/`state` entry word indices.
    pub labels: BTreeMap<String, Label>,
    /// Per-tile actor script entry (word index; 0 = none).
    pub actorscrptr: Vec<i32>,
    /// `useractor` type byte per tile (0 if plain `actor`).
    pub actortype: Vec<u8>,
    /// `definesound` table indexed by sound id.
    pub sounds: Vec<Option<SoundDef>>,
    /// `definequote` strings.
    pub quotes: Vec<String>,
    /// Volume display names (`definevolumename`).
    pub volume_names: [String; 4],
    /// Skill display names (`defineskillname`).
    pub skill_names: [String; 5],
    /// Level table (`definelevelname`), length 4×11.
    pub levels: Vec<LevelDef>,
    /// Level MIDI filenames from the four Atomic `music` tables (4×11).
    pub music_tracks: Vec<String>,
    /// Environment/menu MIDI filenames from the `music 0` table.
    pub environment_music: Vec<String>,
    /// Raw `gamestartup` parameters (length depends on [`ConVersion`]).
    pub gamestartup: Vec<i32>,
    /// Dialect guessed while parsing `gamestartup`.
    pub con_version: ConVersion,
    /// `betaname` line (if any).
    pub betaname: String,
    /// Soft issues (duplicate defines, unfinished keywords).
    pub warnings: u32,
    /// Hard compile errors.
    pub errors: u32,
}

impl ScriptImage {
    /// Empty image with `script[0]` reserved.
    #[must_use]
    pub fn new() -> Self {
        let mut script = Vec::with_capacity(1024);
        script.push(0);
        Self {
            script,
            script_ptr: vec![true], // slot 0 is the end pointer
            labels: BTreeMap::new(),
            actorscrptr: vec![0; MAX_TILES],
            actortype: vec![0; MAX_TILES],
            sounds: vec![None; NUM_SOUNDS],
            quotes: vec![String::new(); NUM_QUOTES],
            volume_names: Default::default(),
            skill_names: Default::default(),
            levels: vec![LevelDef::default(); 4 * 11],
            music_tracks: vec![String::new(); 4 * 11],
            environment_music: vec![String::new(); 11],
            gamestartup: Vec::new(),
            con_version: ConVersion::V15,
            betaname: String::new(),
            warnings: 0,
            errors: 0,
        }
    }

    /// Word count excluding the header slot (Chocolate `conSize`).
    #[must_use]
    pub fn con_size(&self) -> usize {
        self.script.len().saturating_sub(1)
    }

    /// Script words with pointers as **byte** offsets from `script[0]` (Chocolate save form).
    #[must_use]
    pub fn relocated_bytes(&self) -> Vec<i32> {
        self.script
            .iter()
            .zip(self.script_ptr.iter())
            .map(|(&w, &is_ptr)| if is_ptr { w.wrapping_mul(4) } else { w })
            .collect()
    }

    /// SHA-256 (hex) of [`Self::relocated_bytes`] as little-endian i32 words.
    #[must_use]
    pub fn script_sha256_hex(&self) -> String {
        let mut h = Sha256::new();
        for w in self.relocated_bytes() {
            h.update(w.to_le_bytes());
        }
        crate::trace::hex_lower(h.finalize())
    }

    fn finish(&mut self) {
        self.script[0] = self.script.len() as i32;
        if !self.script_ptr.is_empty() {
            self.script_ptr[0] = true;
        }
    }

    fn emit(&mut self, v: i32) {
        self.script.push(v);
        self.script_ptr.push(false);
    }

    fn emit_ptr(&mut self, word_index: i32) {
        self.script.push(word_index);
        self.script_ptr.push(true);
    }

    fn patch_ptr(&mut self, slot: usize, word_index: i32) {
        if slot < self.script.len() {
            self.script[slot] = word_index;
            self.script_ptr[slot] = true;
        }
    }

    fn rewind(&mut self) {
        let _ = self.pop_word();
    }

    /// Pop the last script word (and its ptr flag).
    fn pop_word(&mut self) -> Option<(i32, bool)> {
        let v = self.script.pop()?;
        let is_ptr = self.script_ptr.pop().unwrap_or(false);
        Some((v, is_ptr))
    }

    fn here(&self) -> i32 {
        self.script.len() as i32
    }
}

/// Resolve `include` paths.
pub trait IncludeResolver {
    /// Load an included CON file by name (case-insensitive typical).
    fn read_include(&mut self, name: &str) -> Option<Vec<u8>>;
}

/// No includes (tests / single-file compile).
pub struct NullIncludes;

impl IncludeResolver for NullIncludes {
    fn read_include(&mut self, _name: &str) -> Option<Vec<u8>> {
        None
    }
}

struct Cx<'a> {
    img: &'a mut ScriptImage,
    parsing_state: bool,
    /// Word index of the current actor header, if any.
    parsing_actor: Option<usize>,
    /// Open `{` depth (Chocolate `num_squigilly_brackets`).
    brace_depth: i32,
    /// Pending `else` slots after `if*` (Chocolate `checking_ifelse`).
    checking_ifelse: i32,
}

/// Compile one CON buffer. Nested `include` uses `includes`.
pub fn compile(src: &[u8], includes: &mut dyn IncludeResolver) -> ScriptImage {
    let mut img = ScriptImage::new();
    {
        let mut cx = Cx {
            img: &mut img,
            parsing_state: false,
            parsing_actor: None,
            brace_depth: 0,
            checking_ifelse: 0,
        };
        compile_into(&mut cx, src, includes);
    }
    img.finish();
    img
}

fn compile_into(cx: &mut Cx<'_>, src: &[u8], includes: &mut dyn IncludeResolver) {
    let mut lx = Lexer::new(src);
    while !lx.eof() {
        // Chocolate refuses further commands once error>12; we finish the file
        // so smoke tests can see full size / remaining issues.
        if parse_command(cx, &mut lx, includes) {
            break;
        }
    }
}

fn in_body(cx: &Cx<'_>) -> bool {
    cx.parsing_state || cx.parsing_actor.is_some()
}

/// Peek whether the next token is a keyword (Chocolate `keyword()`).
fn peek_keyword(lx: &Lexer<'_>) -> Option<i32> {
    let mut tmp = lx.clone();
    let tok = tmp.next_token()?;
    if tok.is_empty() {
        return None;
    }
    keyword_index(&tok)
}

/// Returns `true` when compilation of this buffer should stop.
fn parse_command(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, includes: &mut dyn IncludeResolver) -> bool {
    lx.skip_ws();
    if lx.eof() {
        return true;
    }
    let Some(tok) = lx.next_token() else {
        return true;
    };
    if tok.is_empty() {
        return false;
    }

    let Some(tw) = keyword_index(&tok) else {
        cx.img.errors += 1;
        return false;
    };

    // Chocolate `transword`: write opcode, then switch may rewind.
    cx.img.emit(tw);

    match tw {
        // `//`
        20 => {
            cx.img.rewind();
            while let Some(c) = lx.peek() {
                lx.bump();
                if c == b'\n' {
                    break;
                }
            }
            false
        }
        // `/*`
        39 => {
            cx.img.rewind();
            loop {
                let Some(c) = lx.bump() else {
                    cx.img.errors += 1;
                    break;
                };
                if c == b'*' && lx.peek() == Some(b'/') {
                    lx.bump();
                    break;
                }
            }
            false
        }
        // `define`
        19 => {
            let name = lx.get_label();
            if name.is_empty() {
                cx.img.errors += 1;
                return false;
            }
            // Chocolate rejects defines whose name `strcmp`s equal to a keyword
            // (case-sensitive — `define MONEY` is fine; `define money` is not).
            if keyword_index(&name).is_some() {
                cx.img.errors += 1;
                return false;
            }
            let dup = cx.img.labels.contains_key(&name);
            if dup {
                cx.img.warnings += 1;
            }
            if !emit_trans_num(cx, lx) {
                cx.img.errors += 1;
                return false;
            }
            let val = *cx.img.script.last().unwrap_or(&0);
            if !dup {
                cx.img.labels.insert(
                    name,
                    Label {
                        value: val,
                        is_ptr: false,
                    },
                );
            }
            cx.img.rewind(); // value
            cx.img.rewind(); // define opcode
            false
        }
        // `include`
        55 => {
            cx.img.rewind();
            let name = lx.get_label();
            if name.is_empty() {
                cx.img.errors += 1;
                return false;
            }
            match includes.read_include(&name) {
                Some(bytes) => compile_into(cx, &bytes, includes),
                None => cx.img.errors += 1,
            }
            false
        }
        // `definesound`
        57 => parse_definesound(cx, lx),
        // `definequote`
        79 => parse_definequote(cx, lx),
        // `gamestartup`
        60 => parse_gamestartup(cx, lx),
        // `definelevelname`
        0 => parse_definelevelname(cx, lx),
        // `definevolumename`
        107 => parse_define_name(cx, lx, true),
        // `defineskillname`
        108 => parse_define_name(cx, lx, false),
        // `action`
        7 => parse_action(cx, lx),
        // `move`
        32 => parse_move(cx, lx),
        // `ai`
        24 => parse_ai(cx, lx),
        // `state` (define or call)
        17 => parse_state(cx, lx),
        // `ends`
        18 => {
            if !cx.parsing_state {
                cx.img.errors += 1;
            }
            cx.parsing_state = false;
            false
        }
        // `enda`
        4 => {
            if cx.parsing_actor.is_none() {
                cx.img.errors += 1;
            }
            cx.parsing_actor = None;
            false
        }
        // `actor`
        1 => parse_actor(cx, lx, false),
        // `useractor`
        98 => parse_actor(cx, lx, true),
        // `palfrom` — up to 4 nums, pad with 0
        14 => {
            let mut j = 0;
            while j < 4 && peek_keyword(lx).is_none() {
                if !emit_trans_num(cx, lx) {
                    break;
                }
                j += 1;
            }
            while j < 4 {
                cx.img.emit(0);
                j += 1;
            }
            false
        }
        // Two-number ops
        2 | 23 | 28 | 37 | 48 | 58 | 99 => {
            for _ in 0..2 {
                if !emit_trans_num(cx, lx) {
                    cx.img.errors += 1;
                }
            }
            false
        }
        // `hitradius` — five numbers
        50 => {
            for _ in 0..5 {
                if !emit_trans_num(cx, lx) {
                    cx.img.errors += 1;
                }
            }
            false
        }
        // Unary ops (not ifs)
        11 | 13 | 15 | 25 | 31 | 40 | 52 | 68 | 69 | 74 | 77 | 80 | 86 | 87 | 88 | 89 | 92 | 93
        | 100 | 101 | 102 | 103 | 105 | 110 => {
            if !emit_trans_num(cx, lx) {
                cx.img.errors += 1;
            }
            false
        }
        // Nullary non-if ops
        12 | 16 | 22 | 36 | 38 | 42 | 47 | 61 | 66 | 83 | 84 | 95 | 96 | 97 | 104 | 106 => false,
        // `ifpinventory` — two nums then if-tail
        75 => {
            if !emit_trans_num(cx, lx) {
                cx.img.errors += 1;
            }
            if !emit_trans_num(cx, lx) {
                cx.img.errors += 1;
            }
            parse_if_tail(cx, lx, includes)
        }
        // One-num ifs
        3 | 8 | 9 | 21 | 33 | 34 | 35 | 41 | 46 | 53 | 56 | 59 | 62 | 72 | 73 | 78 | 85 | 94
        | 111 => {
            if !emit_trans_num(cx, lx) {
                cx.img.errors += 1;
            }
            parse_if_tail(cx, lx, includes)
        }
        // `ifp` — OR flag mask then if-tail
        51 => {
            let mut flags = 0_i32;
            while peek_keyword(lx).is_none() {
                if !emit_trans_num(cx, lx) {
                    break;
                }
                flags |= cx.img.pop_word().map(|(v, _)| v).unwrap_or(0);
            }
            cx.img.emit(flags);
            parse_if_tail(cx, lx, includes)
        }
        // Nullary ifs
        5 | 6 | 26 | 27 | 43 | 44 | 45 | 49 | 63 | 64 | 65 | 67 | 70 | 71 | 81 | 82 | 90 | 91
        | 109 => parse_if_tail(cx, lx, includes),
        // `else`
        10 => {
            if cx.checking_ifelse > 0 {
                cx.checking_ifelse -= 1;
                let slot = cx.img.here() as usize;
                cx.img.emit(0);
                let _ = parse_command(cx, lx, includes);
                cx.img.patch_ptr(slot, cx.img.here());
            } else {
                cx.img.rewind();
                cx.img.errors += 1;
            }
            false
        }
        // `{` … `}`
        29 => {
            cx.brace_depth += 1;
            while !parse_command(cx, lx, includes) {}
            false
        }
        30 => {
            cx.brace_depth -= 1;
            if cx.brace_depth < 0 {
                cx.img.errors += 1;
            }
            true
        }
        // `betaname`
        76 => {
            cx.img.rewind();
            cx.img.betaname = lx.rest_of_line();
            false
        }
        // `music` — Chocolate stores 11 entries per Atomic volume; volume 0
        // names the environment/menu tracks used when no level is active.
        54 => parse_music(cx, lx),
        _ => {
            cx.img.warnings += 1;
            cx.img.rewind();
            let _ = lx.next_token();
            false
        }
    }
}

/// Leave a fail-forward word, parse one body command, patch the slot.
fn parse_if_tail(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, includes: &mut dyn IncludeResolver) -> bool {
    let slot = cx.img.here() as usize;
    cx.img.emit(0);
    // Skip leading comments like Chocolate's keyword() loop.
    loop {
        match peek_keyword(lx) {
            Some(20) | Some(39) => {
                let _ = parse_command(cx, lx, includes);
            }
            _ => break,
        }
    }
    let _ = parse_command(cx, lx, includes);
    cx.img.patch_ptr(slot, cx.img.here());
    cx.checking_ifelse += 1;
    false
}

fn take_num(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> Option<i32> {
    if !emit_trans_num(cx, lx) {
        return None;
    }
    cx.img.pop_word().map(|(v, _)| v)
}

fn parse_music(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    cx.img.rewind();
    let Some(volume) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    for slot in 0..11 {
        lx.skip_ws();
        if peek_keyword(lx).is_some() || lx.eof() {
            break;
        }
        let name = lx.raw_token();
        if name.is_empty() {
            break;
        }
        match volume {
            0 => cx.img.environment_music[slot] = name,
            1..=4 => cx.img.music_tracks[((volume - 1) as usize) * 11 + slot] = name,
            _ => cx.img.errors += 1,
        }
    }
    false
}

fn parse_definesound(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    cx.img.rewind();
    let Some(k) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    if k < 0 || k as usize >= NUM_SOUNDS {
        cx.img.errors += 1;
        return false;
    }
    let file = lx.raw_token();
    if file.is_empty() {
        cx.img.errors += 1;
        return false;
    }
    let Some(ps) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    let Some(pe) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    let Some(pr) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    let Some(m) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    let Some(vo) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    cx.img.sounds[k as usize] = Some(SoundDef {
        file,
        ps,
        pe,
        pr,
        m,
        vo,
    });
    false
}

fn parse_definequote(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    cx.img.rewind();
    let Some(k) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    if k < 0 || k as usize >= NUM_QUOTES {
        cx.img.errors += 1;
        return false;
    }
    lx.skip_spaces();
    let mut text = lx.rest_of_line();
    if text.len() > 63 {
        text.truncate(63);
        cx.img.errors += 1;
    }
    cx.img.quotes[k as usize] = text;
    false
}

fn parse_gamestartup(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    cx.img.rewind();
    let mut params = Vec::with_capacity(30);
    let mut version = ConVersion::V13;
    for j in 0..30 {
        let Some(v) = take_num(cx, lx) else {
            break;
        };
        params.push(v);
        if j == 25 {
            // After the 26th word: another number ⇒ 1.5; a keyword ⇒ 1.3.
            if peek_keyword(lx).is_some() {
                break;
            }
            version = ConVersion::V15;
        }
    }
    cx.img.con_version = version;
    cx.img.gamestartup = params;
    false
}

fn parse_define_name(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, volume: bool) -> bool {
    cx.img.rewind();
    let Some(j) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    lx.skip_spaces();
    let raw = lx.rest_of_line();
    let name: String = raw
        .chars()
        .take(32)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if volume {
        if j < 0 || j as usize >= cx.img.volume_names.len() {
            cx.img.errors += 1;
            return false;
        }
        cx.img.volume_names[j as usize] = name;
    } else {
        if j < 0 || j as usize >= cx.img.skill_names.len() {
            cx.img.errors += 1;
            return false;
        }
        cx.img.skill_names[j as usize] = name;
    }
    false
}

fn parse_mmss(bytes: &[u8]) -> Option<i32> {
    // "MM:SS" → Chocolate: ((d0*10+d1)*26*60) + ((d3*10+d4)*26).
    if bytes.len() < 5 {
        return None;
    }
    let d = |i: usize| -> Option<i32> {
        let c = bytes[i];
        if c.is_ascii_digit() {
            Some(i32::from(c - b'0'))
        } else {
            None
        }
    };
    Some(((d(0)? * 10 + d(1)?) * 26 * 60) + ((d(3)? * 10 + d(4)?) * 26))
}

fn parse_definelevelname(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    cx.img.rewind();
    let Some(vol) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    let Some(lev) = take_num(cx, lx) else {
        cx.img.errors += 1;
        return false;
    };
    if vol < 0 || lev < 0 || (vol * 11 + lev) as usize >= cx.img.levels.len() {
        cx.img.errors += 1;
        return false;
    }
    let file = lx.raw_token();
    lx.skip_spaces();
    // partime MM:SS
    let start = lx.pos;
    for _ in 0..5 {
        if lx.peek().is_none() {
            break;
        }
        lx.bump();
    }
    let partime = parse_mmss(&lx.src[start..lx.pos]).unwrap_or(0);
    lx.skip_spaces();
    let start = lx.pos;
    for _ in 0..5 {
        if lx.peek().is_none() {
            break;
        }
        lx.bump();
    }
    let designertime = parse_mmss(&lx.src[start..lx.pos]).unwrap_or(0);
    lx.skip_spaces();
    let name: String = lx
        .rest_of_line()
        .chars()
        .take(32)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let idx = (vol * 11 + lev) as usize;
    cx.img.levels[idx] = LevelDef {
        file,
        name,
        partime,
        designertime,
    };
    false
}

fn parse_action(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    if in_body(cx) {
        if !emit_trans_num(cx, lx) {
            cx.img.errors += 1;
        }
        return false;
    }
    cx.img.rewind();
    let name = lx.get_label();
    if name.is_empty() || keyword_index(&name).is_some() {
        cx.img.errors += 1;
        return false;
    }
    let bind = bind_label(cx, &name);
    let entry = cx.img.here();
    if bind {
        cx.img.labels.insert(
            name,
            Label {
                value: entry,
                is_ptr: true,
            },
        );
    }
    let mut got = 0;
    while got < 5 && peek_keyword(lx).is_none() {
        if !emit_trans_num(cx, lx) {
            break;
        }
        got += 1;
    }
    while got < 5 {
        cx.img.emit(0);
        got += 1;
    }
    false
}

fn parse_move(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    if in_body(cx) {
        // `move <label> [flags…]` — OR flags into one word after the label.
        if !emit_trans_num(cx, lx) {
            cx.img.errors += 1;
            return false;
        }
        let mut flags = 0_i32;
        while peek_keyword(lx).is_none() {
            if !emit_trans_num(cx, lx) {
                break;
            }
            flags |= cx.img.pop_word().map(|(v, _)| v).unwrap_or(0);
        }
        cx.img.emit(flags);
        return false;
    }
    cx.img.rewind();
    let name = lx.get_label();
    if name.is_empty() || keyword_index(&name).is_some() {
        cx.img.errors += 1;
        return false;
    }
    let bind = bind_label(cx, &name);
    let entry = cx.img.here();
    if bind {
        cx.img.labels.insert(
            name,
            Label {
                value: entry,
                is_ptr: true,
            },
        );
    }
    let mut got = 0;
    while got < 2 && peek_keyword(lx).is_none() {
        if !emit_trans_num(cx, lx) {
            break;
        }
        got += 1;
    }
    while got < 2 {
        cx.img.emit(0);
        got += 1;
    }
    false
}

fn parse_ai(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    if in_body(cx) {
        if !emit_trans_num(cx, lx) {
            cx.img.errors += 1;
        }
        return false;
    }
    cx.img.rewind();
    let name = lx.get_label();
    if name.is_empty() || keyword_index(&name).is_some() {
        cx.img.errors += 1;
        return false;
    }
    let bind = bind_label(cx, &name);
    let entry = cx.img.here();
    if bind {
        cx.img.labels.insert(
            name,
            Label {
                value: entry,
                is_ptr: true,
            },
        );
    }
    let mut got = 0;
    while got < 3 && peek_keyword(lx).is_none() {
        if got == 2 {
            let mut flags = 0_i32;
            while peek_keyword(lx).is_none() {
                if !emit_trans_num(cx, lx) {
                    break;
                }
                flags |= cx.img.pop_word().map(|(v, _)| v).unwrap_or(0);
            }
            cx.img.emit(flags);
            got = 3;
            break;
        }
        if !emit_trans_num(cx, lx) {
            break;
        }
        got += 1;
    }
    while got < 3 {
        cx.img.emit(0);
        got += 1;
    }
    false
}

fn parse_state(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    if !in_body(cx) {
        cx.img.rewind();
        let name = lx.get_label();
        if name.is_empty() || keyword_index(&name).is_some() {
            cx.img.errors += 1;
            return false;
        }
        let bind = bind_label(cx, &name);
        let entry = cx.img.here();
        if bind {
            cx.img.labels.insert(
                name,
                Label {
                    value: entry,
                    is_ptr: true,
                },
            );
        }
        cx.parsing_state = true;
        return false;
    }
    let name = lx.get_label();
    match cx.img.labels.get(&name) {
        Some(lab) => {
            if lab.is_ptr {
                cx.img.emit_ptr(lab.value);
            } else {
                cx.img.emit(lab.value);
            }
        }
        None => {
            cx.img.errors += 1;
            cx.img.emit(0);
        }
    }
    false
}

fn parse_actor(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, user: bool) -> bool {
    if cx.parsing_state {
        cx.img.errors += 1;
    }
    if cx.parsing_actor.is_some() {
        cx.img.errors += 1;
    }
    cx.img.rewind();
    let header = cx.img.here() as usize;
    cx.parsing_actor = Some(header);

    let mut actor_kind = 0_i32;
    if user {
        if !emit_trans_num(cx, lx) {
            cx.img.errors += 1;
            return false;
        }
        actor_kind = cx.img.pop_word().map(|(v, _)| v).unwrap_or(0);
    }

    if !emit_trans_num(cx, lx) {
        cx.img.errors += 1;
        return false;
    }
    let tile = cx.img.pop_word().map(|(v, _)| v).unwrap_or(0);
    if tile < 0 || tile as usize >= cx.img.actorscrptr.len() {
        cx.img.errors += 1;
        return false;
    }
    cx.img.actorscrptr[tile as usize] = header as i32;
    cx.img.actortype[tile as usize] = actor_kind as u8;

    // Reserve 4 header words (strength, action, move, flags), fill from CON.
    for _ in 0..4 {
        cx.img.emit(0);
    }
    let mut j = 0;
    while j < 4 {
        if j == 3 {
            let mut flags = 0_i32;
            while peek_keyword(lx).is_none() {
                if !emit_trans_num(cx, lx) {
                    break;
                }
                flags |= cx.img.pop_word().map(|(v, _)| v).unwrap_or(0);
            }
            cx.img.script[header + 3] = flags;
            break;
        }
        if peek_keyword(lx).is_some() {
            // Leave remaining header zeros; body keywords follow.
            break;
        }
        if !emit_trans_num(cx, lx) {
            break;
        }
        let (v, is_ptr) = cx.img.pop_word().unwrap_or((0, false));
        cx.img.script[header + j] = v;
        cx.img.script_ptr[header + j] = is_ptr;
        j += 1;
    }
    false
}

/// Returns whether to bind a new label (false on duplicate — keep first).
fn bind_label(cx: &mut Cx<'_>, name: &str) -> bool {
    if cx.img.labels.contains_key(name) {
        cx.img.warnings += 1;
        false
    } else {
        true
    }
}

/// Chocolate `transnum`: emit a number or defined label value onto the script.
fn emit_trans_num(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    lx.skip_ws();
    let Some(tok) = lx.next_token() else {
        return false;
    };
    if tok.is_empty() {
        return false;
    }
    if let Some(lab) = cx.img.labels.get(&tok) {
        if lab.is_ptr {
            cx.img.emit_ptr(lab.value);
        } else {
            cx.img.emit(lab.value);
        }
        return true;
    }
    if tok
        .as_bytes()
        .first()
        .is_some_and(|c| c.is_ascii_digit() || *c == b'-')
    {
        if let Ok(v) = tok.parse::<i32>() {
            cx.img.emit(v);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn define_chain() {
        let src = b"define FOO 5\ndefine BAR FOO\n// ignore\ndefine BAZ 10\n";
        let img = compile(src, &mut NullIncludes);
        assert_eq!(img.errors, 0);
        assert_eq!(img.labels.get("FOO").map(|l| l.value), Some(5));
        assert_eq!(img.labels.get("BAR").map(|l| l.value), Some(5));
        assert_eq!(img.labels.get("BAZ").map(|l| l.value), Some(10));
        assert_eq!(img.con_size(), 0);
    }

    /// Atomic: `define pjumping 32` then `action PJUMPING` — case-sensitive labels.
    #[test]
    fn label_case_sensitive_pads() {
        let src = b"\
define pjumping 32\n\
action PJUMPING 50 4 5 1 30\n\
state S\n\
  action PJUMPING\n\
ends\n\
";
        let img = compile(src, &mut NullIncludes);
        assert_eq!(img.errors, 0, "errs={}", img.errors);
        assert_eq!(img.labels.get("pjumping").map(|l| l.value), Some(32));
        let pad = img.labels.get("PJUMPING").unwrap();
        assert!(pad.is_ptr);
        let s = img.labels.get("S").unwrap().value as usize;
        assert_eq!(img.script[s], 7);
        assert_eq!(img.script[s + 1], pad.value);
    }

    #[test]
    fn block_comment() {
        let src = b"define A 1\n/* define B 2 */\ndefine C 3\n";
        let img = compile(src, &mut NullIncludes);
        assert_eq!(img.errors, 0);
        assert_eq!(img.labels.get("A").map(|l| l.value), Some(1));
        assert!(img.labels.get("B").is_none());
        assert_eq!(img.labels.get("C").map(|l| l.value), Some(3));
    }

    struct MemIncludes {
        files: BTreeMap<String, Vec<u8>>,
    }

    impl IncludeResolver for MemIncludes {
        fn read_include(&mut self, name: &str) -> Option<Vec<u8>> {
            self.files.get(&name.to_ascii_lowercase()).cloned()
        }
    }

    #[test]
    fn include_defines() {
        let mut inc = MemIncludes {
            files: BTreeMap::from([("defs.con".into(), b"define INNER 7\n".to_vec())]),
        };
        let src = b"include DEFS.CON\ndefine OUTER INNER\n";
        let img = compile(src, &mut inc);
        assert_eq!(img.errors, 0);
        assert_eq!(img.labels.get("INNER").map(|l| l.value), Some(7));
        assert_eq!(img.labels.get("OUTER").map(|l| l.value), Some(7));
    }

    #[test]
    fn atomic_game_con_from_grp_smoke() {
        let grp_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke/duke3d.grp");
        let Ok(bytes) = std::fs::read(&grp_path) else {
            eprintln!("skip: no {grp_path:?}");
            return;
        };
        let Ok(grp) = room_build::Grp::parse(&bytes) else {
            panic!("GRP parse");
        };
        let game = grp.get("GAME.CON").expect("GAME.CON");
        let mut files = BTreeMap::new();
        for name in ["DEFS.CON", "USER.CON", "GAME.CON"] {
            if let Some(b) = grp.get(name) {
                files.insert(name.to_ascii_lowercase(), b.to_vec());
            }
        }
        let mut inc = MemIncludes { files };
        let img = compile(game, &mut inc);
        assert_eq!(img.errors, 0, "warnings={}", img.warnings);
        assert_eq!(img.con_version, ConVersion::V15);
        // Chocolate Atomic: Code Size reports conSize==16208 (1.5).
        assert_eq!(img.con_size(), 16208);
        // Chocolate Atomic: 1794 labels (case-sensitive; e.g. pjumping ≠ PJUMPING).
        assert_eq!(img.labels.len(), 1794, "labels={}", img.labels.len());
        assert!(
            img.sounds.iter().filter(|s| s.is_some()).count() >= 374,
            "sounds"
        );
        // Relocated script SHA-256 vs Chocolate ROOM_SCRIPT_DUMP (i386 Atomic GRP).
        assert_eq!(
            img.script_sha256_hex(),
            "2e247d6d29e225417487783ac0713322be92b359fc18c0f0b99c73db3cf53200"
        );
    }

    #[test]
    fn action_move_ai_pads() {
        let src = b"\
action STAND 0 1 5 1 15\n\
move SEEK 64 0\n\
ai CHASE STAND SEEK\n\
";
        let img = compile(src, &mut NullIncludes);
        assert_eq!(img.errors, 0, "errs={}", img.errors);
        let stand = img.labels.get("STAND").unwrap().value;
        assert_eq!(
            &img.script[stand as usize..stand as usize + 5],
            &[0, 1, 5, 1, 15]
        );
        let seek = img.labels.get("SEEK").unwrap().value;
        assert_eq!(&img.script[seek as usize..seek as usize + 2], &[64, 0]);
        let chase = img.labels.get("CHASE").unwrap().value;
        assert_eq!(
            &img.script[chase as usize..chase as usize + 3],
            &[stand, seek, 0]
        );
    }

    #[test]
    fn if_else_patches_forward() {
        let src = b"\
define X 1\n\
state T\n\
  ifrnd 128\n\
    break\n\
  else\n\
    killit\n\
ends\n\
";
        let img = compile(src, &mut NullIncludes);
        assert_eq!(img.errors, 0, "errs={}", img.errors);
        let t = img.labels.get("T").unwrap().value as usize;
        // ifrnd, 128, fail_ptr, break, else, else_ptr, killit
        assert_eq!(img.script[t], 3); // ifrnd
        assert_eq!(img.script[t + 1], 128);
        let fail = img.script[t + 2] as usize;
        assert_eq!(img.script[t + 3], 12); // break
        assert_eq!(fail, t + 4); // points at else
        assert_eq!(img.script[t + 4], 10); // else
        let else_tgt = img.script[t + 5] as usize;
        assert_eq!(img.script[t + 6], 22); // killit
        assert_eq!(else_tgt, t + 7);
        assert_eq!(img.script[t + 7], 18); // ends
    }

    #[test]
    fn definesound_quote_gamestartup() {
        let src = b"\
define PISTOL_FIRE 0\n\
definesound PISTOL_FIRE pistol.voc 0 0 0 1 0\n\
definequote 1 ANOTHER ONE BITES THE DUST\n\
definevolumename 0 L.A. MELTDOWN\n\
defineskillname 0 PIECE OF CAKE\n\
definelevelname 0 0 E1L1.MAP 03:00 04:00 HOLLYWOOD HOLOCAUST\n\
gamestartup 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30\n\
";
        let img = compile(src, &mut NullIncludes);
        assert_eq!(img.errors, 0, "errs={}", img.errors);
        let snd = img.sounds[0].as_ref().unwrap();
        assert_eq!(snd.file, "pistol.voc");
        assert_eq!(snd.m, 1);
        assert_eq!(img.quotes[1], "ANOTHER ONE BITES THE DUST");
        assert_eq!(img.volume_names[0], "L.A. MELTDOWN");
        assert_eq!(img.skill_names[0], "PIECE OF CAKE");
        assert_eq!(img.levels[0].file, "E1L1.MAP");
        assert_eq!(img.levels[0].name, "HOLLYWOOD HOLOCAUST");
        assert_eq!(img.con_version, ConVersion::V15);
        assert_eq!(img.gamestartup.len(), 30);
        assert_eq!(img.gamestartup[0], 1);
        assert_eq!(img.gamestartup[29], 30);
    }

    #[test]
    fn state_and_actor_body() {
        let src = b"\
action AIDLE 0 1 1 1 1\n\
state DOIDLE\n\
  action AIDLE\n\
ends\n\
actor 1234 10 AIDLE\n\
  state DOIDLE\n\
  break\n\
enda\n\
";
        let img = compile(src, &mut NullIncludes);
        assert_eq!(img.errors, 0, "errs={}", img.errors);
        assert!(img.actorscrptr[1234] != 0);
        let doidle = img.labels.get("DOIDLE").unwrap().value;
        // state body: action opcode + label, then ends opcode
        assert_eq!(img.script[doidle as usize], 7); // action
        assert_eq!(
            img.script[doidle as usize + 1],
            img.labels.get("AIDLE").unwrap().value
        );
        assert_eq!(img.script[doidle as usize + 2], 18); // ends
        let act = img.actorscrptr[1234] as usize;
        assert_eq!(img.script[act], 10); // strength
        assert_eq!(img.script[act + 1], img.labels.get("AIDLE").unwrap().value);
    }

    #[test]
    fn one_line_useractor() {
        let src = b"\
define notenemy 1\n\
define DUKEBURGER 100\n\
define MOP 101\n\
define PIRATEGALSTRENGTH 200\n\
define WEAK 5\n\
state breakobject\n\
  break\n\
ends\n\
action ABURGERROTS 0 1 5\n\
useractor notenemy DUKEBURGER PIRATEGALSTRENGTH ABURGERROTS state breakobject enda\n\
useractor notenemy MOP WEAK state breakobject enda\n\
";
        let img = compile(src, &mut NullIncludes);
        assert_eq!(img.errors, 0, "errors={}", img.errors);
        assert!(img.actorscrptr[100] != 0);
        assert!(img.actorscrptr[101] != 0);
        assert_eq!(img.actortype[100], 1); // notenemy
    }
}
