//! CON 1.5 compiler — keyword emit/rewind matching Chocolate `GAMEDEF.C`.
//!
//! GPL game-code translation is allowed. Script pointers are stored as word
//! indices into [`ScriptImage::script`] (relocatable).

use super::eduke_vm::{
    EdActorMember, EdBinaryOperator, EdComparison, EdInstruction, EdItemSprayKind, EdLValue,
    EdOperand, EdSoundMode, EdSpriteListKind, EdStructKind, EdVariableScope, EdukeProgram,
};
use super::keywords::keyword_index;
use super::lex::Lexer;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Atomic sound limit (`NUM_SOUNDS`).
pub const NUM_SOUNDS: usize = 450;
/// EDuke32 supports sound ids through 16383; Atomic remains capped at 450.
pub const EDUKE_MAX_SOUNDS: usize = 16_384;
/// Quote limit (`NUMOFFIRSTTIMEACTIVE`).
pub const NUM_QUOTES: usize = 192;
/// Bounded quote slots accepted by the EDuke32 compiler path.
pub const EDUKE_MAX_QUOTES: usize = 4096;
/// Tile limit for `actorscrptr` / `actortype` (Chocolate `MAXTILES`).
pub const MAX_TILES: usize = 9216;
/// EDuke32's expanded art-tile limit.
pub const EDUKE_MAX_TILES: usize = 30_720;

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
    /// Optional EDuke32 volume override from the eighth `definesound` field.
    pub volume: i32,
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

/// CON language dialect selected for a compiled script tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConDialect {
    /// Atomic / Chocolate Duke CON semantics.
    #[default]
    Atomic,
    /// EDuke32 CON extensions. The compiler currently retains several metadata
    /// declarations, but this image is not executable by the Atomic VM.
    EDuke32,
}

/// Options for compiling a CON script image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConCompileOptions {
    /// Language dialect for this script image.
    pub dialect: ConDialect,
}

/// EDuke32 `var` / `gamevar` declaration retained for runtime initialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdVariableDefinition {
    /// Case-preserved CON identifier.
    pub name: String,
    /// Initial value token (resolved by the EDuke32 VM/compiler stage).
    pub initial_value: String,
    /// Raw EDuke32 storage flags, in source order.
    pub flags: Vec<String>,
}

/// EDuke32 `array` / `gamearray` declaration retained for runtime allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdArrayDefinition {
    /// Case-preserved CON identifier.
    pub name: String,
    /// Raw element-count token, resolved against game constants at load time.
    pub length: String,
    /// Raw EDuke32 storage / save flags, in source order.
    pub flags: Vec<String>,
}

/// EDuke32 state declaration name and whether it extends an existing state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdStateDefinition {
    /// Case-preserved CON identifier.
    pub name: String,
    /// True for `appendstate`, false for `defstate`.
    pub appended: bool,
}

/// EDuke32 event hook declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdEventDefinition {
    /// Event identifier, for example `EVENT_GAME`.
    pub name: String,
    /// True for `appendevent`; false for `onevent`.
    pub appended: bool,
}

/// One EDuke32 `defineprojectile` property assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdProjectileProperty {
    /// Projectile tile or label token.
    pub projectile: String,
    /// `PROJ_*` property token.
    pub property: String,
    /// Value token, resolved by the EDuke32 compiler stage.
    pub value: String,
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
    /// Language dialect selected for this image.
    pub dialect: ConDialect,
    /// Parsed EDuke32 `var` / `gamevar` declarations.
    pub eduke_variables: Vec<EdVariableDefinition>,
    /// Parsed EDuke32 `array` / `gamearray` declarations.
    pub eduke_arrays: Vec<EdArrayDefinition>,
    /// EDuke32 state declaration names.
    pub eduke_states: Vec<EdStateDefinition>,
    /// EDuke32 event hook declarations.
    pub eduke_events: Vec<EdEventDefinition>,
    /// Compiled EDuke32 instruction stream and named state entry points.
    pub eduke_program: EdukeProgram,
    /// EDuke32 event names and program instruction entry positions in source order.
    pub eduke_event_entries: Vec<(String, usize, bool)>,
    /// Uppercased event name → positions in `eduke_event_entries`, in source
    /// order. Kept in step by [`ScriptImage::push_eduke_event_entry`].
    pub eduke_event_index: BTreeMap<String, Vec<usize>>,
    /// EDuke32 actor tiles and their compiled VM body entry positions.
    pub eduke_actor_entries: BTreeMap<i16, usize>,
    /// EDuke32 state names and program instruction entry positions in source order.
    pub eduke_state_entries: Vec<(String, usize, bool)>,
    /// EDuke32 projectile property assignments.
    pub eduke_projectiles: Vec<EdProjectileProperty>,
    /// EDuke32 damage-event tile names.
    pub eduke_damage_event_tiles: Vec<String>,
    /// EDuke32 sprite shading / palette metadata directives and tile operands.
    pub eduke_sprite_directives: Vec<(String, Vec<String>)>,
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
    /// EDuke32 compile errors grouped by the command head active at the error.
    /// The map is capped to avoid attacker-controlled identifier growth.
    pub eduke_error_heads: BTreeMap<String, u32>,
    /// A few bounded source locations per EDuke32 error head for compiler triage.
    pub eduke_error_samples: BTreeMap<String, Vec<String>>,
    /// EDuke32 statements that compile to explicit VM `NoOp`s, grouped by head.
    pub eduke_noop_heads: BTreeMap<String, u32>,
}

impl ScriptImage {
    /// Register one EDuke32 event hook entry (source order).
    pub fn push_eduke_event_entry(&mut self, name: String, entry: usize, appended: bool) {
        self.eduke_event_index
            .entry(name.to_ascii_uppercase())
            .or_default()
            .push(self.eduke_event_entries.len());
        self.eduke_event_entries.push((name, entry, appended));
    }

    /// Positions in `eduke_event_entries` of the hooks for `event`, compared
    /// case-insensitively, in source order.
    #[must_use]
    pub fn eduke_event_hooks(&self, event: &str) -> &[usize] {
        if let Some(hooks) = self.eduke_event_index.get(event) {
            return hooks;
        }
        if event.bytes().any(|byte| byte.is_ascii_lowercase())
            && let Some(hooks) = self.eduke_event_index.get(&event.to_ascii_uppercase())
        {
            return hooks;
        }
        &[]
    }

    /// Whether any hook is registered for `event` (case-insensitive).
    #[must_use]
    pub fn has_eduke_event(&self, event: &str) -> bool {
        !self.eduke_event_hooks(event).is_empty()
    }

    /// Empty image with `script[0]` reserved.
    #[must_use]
    pub fn new() -> Self {
        let mut script = Vec::with_capacity(1024);
        script.push(0);
        Self {
            dialect: ConDialect::Atomic,
            eduke_variables: Vec::new(),
            eduke_arrays: Vec::new(),
            eduke_states: Vec::new(),
            eduke_events: Vec::new(),
            eduke_program: EdukeProgram::default(),
            eduke_event_entries: Vec::new(),
            eduke_event_index: BTreeMap::new(),
            eduke_actor_entries: BTreeMap::new(),
            eduke_state_entries: Vec::new(),
            eduke_projectiles: Vec::new(),
            eduke_damage_event_tiles: Vec::new(),
            eduke_sprite_directives: Vec::new(),
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
            eduke_error_heads: BTreeMap::new(),
            eduke_error_samples: BTreeMap::new(),
            eduke_noop_heads: BTreeMap::new(),
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
    eduke_command_head: String,
    eduke_line: u32,
    eduke_source: String,
    parsing_state: bool,
    eduke_current_state: Option<EdStateFragment>,
    eduke_state_fragments: Vec<EdStateFragment>,
    parsing_event: bool,
    /// Word index of the current actor header, if any.
    parsing_actor: Option<usize>,
    /// Open `{` depth (Chocolate `num_squigilly_brackets`).
    brace_depth: i32,
    /// Pending `else` slots after `if*` (Chocolate `checking_ifelse`).
    checking_ifelse: i32,
    /// EDuke32 conditions whose following branch may be spelled after a
    /// closing brace, paired in source order like Chocolate's `checking_ifelse`.
    eduke_pending_ifelse: Vec<EdPendingIfElse>,
    /// Break jumps awaiting the end of the innermost EDuke32 loop or switch.
    eduke_break_stack: Vec<Vec<usize>>,
    eduke_switch_depth: usize,
    eduke_nested_endswitch_seen: bool,
}

#[derive(Debug, Clone, Copy)]
struct EdPendingIfElse {
    condition_instruction: usize,
    skip_jump: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdStateFragmentKind {
    Define,
    Append,
    Prepend,
}

#[derive(Debug, Clone)]
struct EdStateFragment {
    /// Exact state label key used by CON label resolution.
    key: String,
    /// Exact CON label key used for the exported state pointer.
    label_key: String,
    kind: EdStateFragmentKind,
    entry: usize,
    tail: usize,
}

impl Cx<'_> {
    fn emit_eduke_noop(&mut self) {
        if self.img.dialect == ConDialect::EDuke32 {
            const MAX_NOOP_HEADS: usize = 256;
            let head = if self.eduke_command_head.is_empty() {
                "<unknown>"
            } else {
                &self.eduke_command_head
            };
            // These are empty control-flow syntax or explicit empty commands,
            // rather than unsupported runtime behavior. Keep them out of the
            // coverage report while preserving their VM instruction slots.
            if !matches!(head, "else" | "nullop" | "{" | "}") {
                if let Some(count) = self.img.eduke_noop_heads.get_mut(head) {
                    *count = count.saturating_add(1);
                } else if self.img.eduke_noop_heads.len() < MAX_NOOP_HEADS - 1 {
                    self.img.eduke_noop_heads.insert(head.to_owned(), 1);
                } else {
                    let count = self
                        .img
                        .eduke_noop_heads
                        .entry("<other>".to_owned())
                        .or_default();
                    *count = count.saturating_add(1);
                }
            }
        }
        self.img
            .eduke_program
            .instructions
            .push(EdInstruction::NoOp);
    }

    fn record_error_as(&mut self, head: &str) {
        let previous = std::mem::replace(&mut self.eduke_command_head, head.to_owned());
        self.record_error();
        self.eduke_command_head = previous;
    }

    fn record_error(&mut self) {
        self.img.errors = self.img.errors.saturating_add(1);
        if self.img.dialect != ConDialect::EDuke32 {
            return;
        }
        const MAX_ERROR_HEADS: usize = 256;
        let head = if self.eduke_command_head.is_empty() {
            "<unknown>"
        } else {
            &self.eduke_command_head
        };
        if self.img.eduke_error_heads.contains_key(head) {
            if let Some(count) = self.img.eduke_error_heads.get_mut(head) {
                *count = count.saturating_add(1);
            }
        } else if self.img.eduke_error_heads.len() < MAX_ERROR_HEADS {
            self.img.eduke_error_heads.insert(head.to_owned(), 1);
        } else {
            *self
                .img
                .eduke_error_heads
                .entry("<other>".to_owned())
                .or_default() += 1;
        }
        if let Some(samples) = self.img.eduke_error_samples.get_mut(head) {
            if samples.len() < 5 {
                samples.push(format!(
                    "{}:{} (state={}, event={}, actor={})",
                    self.eduke_source,
                    self.eduke_line,
                    self.parsing_state,
                    self.parsing_event,
                    self.parsing_actor.is_some()
                ));
            }
        } else if self.img.eduke_error_samples.len() < MAX_ERROR_HEADS {
            self.img.eduke_error_samples.insert(
                head.to_owned(),
                vec![format!(
                    "{}:{} (state={}, event={}, actor={})",
                    self.eduke_source,
                    self.eduke_line,
                    self.parsing_state,
                    self.parsing_event,
                    self.parsing_actor.is_some()
                )],
            );
        }
    }
}

/// Compile one CON buffer. Nested `include` uses `includes`.
pub fn compile(src: &[u8], includes: &mut dyn IncludeResolver) -> ScriptImage {
    compile_with_options(src, includes, ConCompileOptions::default())
}

/// Compile one CON buffer with an explicit language dialect.
///
/// The EDuke32 path compiles supported declarations, control flow, and runtime
/// commands into [`EdukeProgram`] while recording unsupported statements as
/// diagnostics. The emitted VM instruction set and its live engine adapter
/// remain a deliberate partial subset of EDuke32.
pub fn compile_with_options(
    src: &[u8],
    includes: &mut dyn IncludeResolver,
    options: ConCompileOptions,
) -> ScriptImage {
    let mut img = ScriptImage::new();
    img.dialect = options.dialect;
    {
        let mut cx = Cx {
            img: &mut img,
            eduke_command_head: String::new(),
            eduke_line: 0,
            eduke_source: "<root>".to_owned(),
            parsing_state: false,
            eduke_current_state: None,
            eduke_state_fragments: Vec::new(),
            parsing_event: false,
            parsing_actor: None,
            brace_depth: 0,
            checking_ifelse: 0,
            eduke_pending_ifelse: Vec::new(),
            eduke_break_stack: Vec::new(),
            eduke_switch_depth: 0,
            eduke_nested_endswitch_seen: false,
        };
        compile_into(&mut cx, src, includes);
        if cx.eduke_current_state.is_some() {
            cx.record_error_as("state-missing-ends");
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Return);
            finish_eduke_state_fragment(&mut cx);
            cx.parsing_state = false;
        }
        finalize_eduke_states(&mut cx);
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
            if cx.img.dialect == ConDialect::EDuke32 && !lx.eof() {
                // An EDuke32 closing brace ends the current block. At file
                // scope it must not truncate the remaining CON source.
                continue;
            }
            break;
        }
    }
}

/// EDuke32 `break` outside a switch or loop leaves the current body. In an
/// event block it jumps to that block's end, and `appendevent` / `onevent`
/// chaining retargets it to the next block (`C_FillEventBreakStackWithJump`),
/// so later handlers of the same event still run; in a state or actor it
/// returns from that body.
fn emit_eduke_body_break(cx: &mut Cx<'_>) {
    let instruction = if cx.parsing_event && !cx.parsing_state && cx.parsing_actor.is_none() {
        EdInstruction::EndEvent
    } else {
        EdInstruction::Return
    };
    cx.img.eduke_program.instructions.push(instruction);
}

fn in_body(cx: &Cx<'_>) -> bool {
    cx.parsing_state || cx.parsing_event || cx.parsing_actor.is_some()
}

/// Peek whether the next token is a keyword (Chocolate `keyword()`).
fn peek_keyword(lx: &Lexer<'_>, dialect: ConDialect) -> Option<i32> {
    let mut tmp = lx.clone();
    let tok = tmp.next_token()?;
    if tok.is_empty() {
        return None;
    }
    if dialect == ConDialect::EDuke32 {
        if tok.starts_with("//") {
            return Some(20);
        }
        if tok.starts_with("/*") {
            return Some(39);
        }
        if eduke_statement_head(&tok) {
            return Some(-1);
        }
        keyword_index(&tok.to_ascii_lowercase())
    } else {
        keyword_index(&tok)
    }
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
    if cx.img.dialect == ConDialect::EDuke32 {
        cx.eduke_command_head = tok.to_ascii_lowercase();
        cx.eduke_line = lx.line;
    }

    // EDuke32 permits comment delimiters followed immediately by comment text
    // (`/*----...` and `//---...`), which the Atomic token table does not.
    if cx.img.dialect == ConDialect::EDuke32 && tok.starts_with("//") {
        while let Some(c) = lx.bump() {
            if c == b'\n' {
                break;
            }
        }
        return false;
    }
    if cx.img.dialect == ConDialect::EDuke32 && tok.starts_with("/*") {
        let mut previous = None;
        if tok.as_bytes().windows(2).any(|pair| pair == b"*/") {
            return false;
        }
        for c in tok.bytes().skip(1) {
            if previous == Some(b'*') && c == b'/' {
                return false;
            }
            previous = Some(c);
        }
        while let Some(c) = lx.bump() {
            if previous == Some(b'*') && c == b'/' {
                return false;
            }
            previous = Some(c);
        }
        cx.record_error();
        return true;
    }

    if cx.img.dialect == ConDialect::EDuke32
        && cx.eduke_switch_depth > 0
        && tok.eq_ignore_ascii_case("endswitch")
    {
        let _ = lx.rest_of_line();
        cx.eduke_nested_endswitch_seen = true;
        return true;
    }

    if cx.img.dialect == ConDialect::EDuke32 && parse_eduke_directive(cx, lx, &tok) {
        return false;
    }

    if cx.img.dialect == ConDialect::EDuke32 && parse_eduke_vm_command(cx, lx, includes, &tok) {
        return false;
    }

    if cx.img.dialect == ConDialect::EDuke32 && keyword_index(&tok.to_ascii_lowercase()).is_none() {
        // EDuke32 statements are line-oriented. Keep an unsupported command
        // and its operands together so diagnostics count source statements and
        // parsing can continue at the next independent command.
        cx.record_error();
        let remainder = lx.rest_of_line();
        account_eduke_braces(cx, &remainder);
        if in_body(cx) {
            cx.emit_eduke_noop();
        }
        return false;
    }

    let Some(tw) = (if cx.img.dialect == ConDialect::EDuke32 {
        keyword_index(&tok.to_ascii_lowercase())
    } else {
        keyword_index(&tok)
    }) else {
        cx.record_error();
        return false;
    };

    // Chocolate `transword`: write opcode, then switch may rewind.
    cx.img.emit(tw);
    if cx.img.dialect == ConDialect::EDuke32 && in_body(cx) {
        match tok.to_ascii_lowercase().as_str() {
            "ends" | "enda" => cx
                .img
                .eduke_program
                .instructions
                .push(EdInstruction::Return),
            "pstomp" => cx
                .img
                .eduke_program
                .instructions
                .push(EdInstruction::PlayerStomp),
            "wackplayer" => cx
                .img
                .eduke_program
                .instructions
                .push(EdInstruction::PlayerWack),
            "{" | "}" => {}
            _ => cx.emit_eduke_noop(),
        }
    }

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
                    cx.record_error();
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
                cx.record_error();
                return false;
            }
            // Atomic rejects keyword collisions. EDuke32 game data defines
            // names such as MONEY, TIP, MAIL and PAPER, which also name actor
            // commands; command-position lookup still gives the opcode priority.
            if cx.img.dialect == ConDialect::Atomic && keyword_index(&name).is_some() {
                cx.record_error();
                return false;
            }
            let key = label_key(cx.img.dialect, &name);
            let dup = cx.img.labels.contains_key(&key);
            if dup && cx.img.dialect == ConDialect::Atomic {
                cx.img.warnings += 1;
            }
            if !emit_trans_num(cx, lx) {
                cx.record_error();
                return false;
            }
            let val = *cx.img.script.last().unwrap_or(&0);
            if !dup || cx.img.dialect == ConDialect::EDuke32 {
                cx.img.labels.insert(
                    key,
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
                cx.record_error();
                return false;
            }
            match includes.read_include(&name) {
                Some(bytes) => {
                    let previous_source = std::mem::replace(&mut cx.eduke_source, name);
                    compile_into(cx, &bytes, includes);
                    cx.eduke_source = previous_source;
                }
                None => cx.record_error(),
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
                cx.record_error_as("ends-without-state");
            } else if cx.img.dialect == ConDialect::EDuke32 {
                finish_eduke_state_fragment(cx);
            }
            cx.parsing_state = false;
            false
        }
        // `enda`
        4 => {
            if cx.parsing_actor.is_none() {
                cx.record_error();
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
            while j < 4 && peek_keyword(lx, cx.img.dialect).is_none() {
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
                    cx.record_error();
                }
            }
            false
        }
        // `hitradius` — five numbers
        50 => {
            for _ in 0..5 {
                if !emit_trans_num(cx, lx) {
                    cx.record_error();
                }
            }
            false
        }
        // Unary ops (not ifs)
        11 | 13 | 15 | 25 | 31 | 40 | 52 | 68 | 69 | 74 | 77 | 80 | 86 | 87 | 88 | 89 | 92 | 93
        | 100 | 101 | 102 | 103 | 105 | 110 => {
            if !emit_trans_num(cx, lx) {
                cx.record_error();
            }
            false
        }
        // Nullary non-if ops
        12 | 16 | 22 | 36 | 38 | 42 | 47 | 61 | 66 | 83 | 84 | 95 | 96 | 97 | 104 | 106 => false,
        // `ifpinventory` — two nums then if-tail
        75 => {
            if !emit_trans_num(cx, lx) {
                cx.record_error();
            }
            if !emit_trans_num(cx, lx) {
                cx.record_error();
            }
            parse_if_tail(cx, lx, includes)
        }
        // One-num ifs
        3 | 8 | 9 | 21 | 33 | 34 | 35 | 41 | 46 | 53 | 56 | 59 | 62 | 72 | 73 | 78 | 85 | 94
        | 111 => {
            if !emit_trans_num(cx, lx) {
                cx.record_error();
            }
            parse_if_tail(cx, lx, includes)
        }
        // `ifp` — OR flag mask then if-tail
        51 => {
            let mut flags = 0_i32;
            while peek_keyword(lx, cx.img.dialect).is_none() {
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
            if cx.img.dialect == ConDialect::EDuke32 {
                // Unlike Atomic CON, EDuke32 accepts unmatched `else`
                // statements in legacy scripts. Their following statement is
                // ignored (the original GAME.CON has this in
                // pigshootenemystate because its `ifcansee` was commented
                // out). Pair a condition that reached the generic parser;
                // otherwise parse and discard the orphan branch.
                cx.img.rewind();
                if let Some(pending) = cx.eduke_pending_ifelse.pop() {
                    let skip_jump = if pending.skip_jump == usize::MAX {
                        let index = cx.img.eduke_program.instructions.len();
                        cx.img
                            .eduke_program
                            .instructions
                            .push(EdInstruction::Jump { target: 0 });
                        index
                    } else {
                        pending.skip_jump
                    };
                    let else_start = cx.img.eduke_program.instructions.len();
                    patch_eduke_conditional(cx, pending.condition_instruction, else_start);
                    compile_eduke_body(cx, lx, includes, &[]);
                    let after_else = cx.img.eduke_program.instructions.len();
                    patch_eduke_jump(cx, skip_jump, after_else);
                } else {
                    discard_eduke_orphan_else_body(cx, lx, includes);
                }
            } else if cx.checking_ifelse > 0 {
                cx.checking_ifelse -= 1;
                let slot = cx.img.here() as usize;
                cx.img.emit(0);
                let _ = parse_command(cx, lx, includes);
                cx.img.patch_ptr(slot, cx.img.here());
            } else {
                cx.img.rewind();
                cx.record_error();
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
                cx.record_error();
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

/// Consume an EDuke32 orphan `else` branch while keeping its statements out
/// of the VM. The source still needs parsing so following statements and
/// closing braces remain aligned.
fn discard_eduke_orphan_else_body(
    cx: &mut Cx<'_>,
    lx: &mut Lexer<'_>,
    includes: &mut dyn IncludeResolver,
) {
    let instruction_start = cx.img.eduke_program.instructions.len();
    let pending_start = cx.eduke_pending_ifelse.len();
    let break_start = cx.eduke_break_stack.len();
    compile_eduke_body(cx, lx, includes, &[]);
    cx.img
        .eduke_program
        .instructions
        .truncate(instruction_start);
    cx.eduke_pending_ifelse.truncate(pending_start);
    cx.eduke_break_stack.truncate(break_start);
}

fn account_eduke_braces(cx: &mut Cx<'_>, source: &str) {
    let mut in_string = false;
    let mut escaped = false;
    let bytes = source.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            at += 1;
            continue;
        }
        if byte == b'"' {
            in_string = true;
            at += 1;
            continue;
        }
        if byte == b'/' && bytes.get(at + 1) == Some(&b'/') {
            break;
        }
        match byte {
            b'{' => cx.brace_depth += 1,
            b'}' => {
                cx.brace_depth -= 1;
                if cx.brace_depth < 0 {
                    cx.record_error();
                }
            }
            _ => {}
        }
        at += 1;
    }
}

fn parse_eduke_vm_command(
    cx: &mut Cx<'_>,
    lx: &mut Lexer<'_>,
    includes: &mut dyn IncludeResolver,
    command: &str,
) -> bool {
    let command = eduke_legacy_alias(&command.to_ascii_lowercase());
    if command == "state" && !in_body(cx) {
        if let Some(name) = lx.next_token() {
            begin_eduke_state_fragment(cx, name, EdStateFragmentKind::Define, false);
        } else {
            cx.record_error();
        }
        return true;
    }
    if !in_body(cx) {
        return false;
    }
    if command == "for" {
        let fields = eduke_line_fields(lx);
        if fields.len() >= 2 && fields[1].eq_ignore_ascii_case("allsprites") {
            let iterator = eduke_lvalue(cx.img, &fields[0]);
            let begin = cx.img.eduke_program.instructions.len();
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::ForAllSpritesBegin {
                    target: iterator.clone(),
                    end: 0,
                });
            cx.eduke_break_stack.push(Vec::new());
            compile_eduke_body(cx, lx, includes, fields.get(2..).unwrap_or_default());
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::ForAllSpritesNext {
                    target: iterator,
                    begin,
                });
            let end = cx.img.eduke_program.instructions.len();
            if let Some(EdInstruction::ForAllSpritesBegin {
                end: end_target, ..
            }) = cx.img.eduke_program.instructions.get_mut(begin)
            {
                *end_target = end;
            }
            if let Some(breaks) = cx.eduke_break_stack.pop() {
                for jump in breaks {
                    patch_eduke_jump(cx, jump, end);
                }
            }
            return true;
        }
        if fields.len() < 3 || !fields[1].eq_ignore_ascii_case("range") {
            cx.record_error();
            cx.emit_eduke_noop();
            return true;
        }

        let iterator = eduke_lvalue(cx.img, &fields[0]);
        let mut suffix = cx.img.eduke_program.instructions.len();
        let mut limit_name = format!("__room_for_bound_{suffix}");
        while cx
            .img
            .eduke_variables
            .iter()
            .any(|variable| variable.name.eq_ignore_ascii_case(&limit_name))
        {
            suffix = suffix.wrapping_add(1);
            limit_name = format!("__room_for_bound_{suffix}");
        }
        let mut counter_name = format!("__room_for_index_{suffix}");
        while cx
            .img
            .eduke_variables
            .iter()
            .any(|variable| variable.name.eq_ignore_ascii_case(&counter_name))
        {
            suffix = suffix.wrapping_add(1);
            counter_name = format!("__room_for_index_{suffix}");
        }
        cx.img.eduke_variables.push(EdVariableDefinition {
            name: limit_name.clone(),
            initial_value: "0".to_owned(),
            flags: Vec::new(),
        });
        cx.img.eduke_variables.push(EdVariableDefinition {
            name: counter_name.clone(),
            initial_value: "0".to_owned(),
            flags: Vec::new(),
        });
        let counter = EdLValue::Variable(counter_name.clone().into());
        let limit = EdLValue::Variable(limit_name.clone().into());
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Assign {
                target: counter.clone(),
                value: EdOperand::Immediate(0),
            });
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Assign {
                target: limit,
                value: eduke_operand(cx.img, &fields[2]),
            });
        let condition = cx.img.eduke_program.instructions.len();
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::JumpIf {
                left: EdOperand::Variable(counter_name.clone().into()),
                comparison: EdComparison::GreaterEqual,
                right: EdOperand::Variable(limit_name.into()),
                target: 0,
            });
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Assign {
                target: iterator.clone(),
                value: EdOperand::Variable(counter_name.into()),
            });
        cx.eduke_break_stack.push(Vec::new());
        compile_eduke_body(cx, lx, includes, fields.get(3..).unwrap_or_default());
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Binary {
                target: counter,
                operator: EdBinaryOperator::Add,
                right: EdOperand::Immediate(1),
            });
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Jump { target: condition });
        let end = cx.img.eduke_program.instructions.len();
        patch_eduke_conditional(cx, condition, end);
        if let Some(breaks) = cx.eduke_break_stack.pop() {
            for jump in breaks {
                patch_eduke_jump(cx, jump, end);
            }
        }
        return true;
    }
    if let Some(comparison) = eduke_while_comparison(&command) {
        let fields = eduke_line_fields(lx);
        if fields.len() < 2 {
            cx.record_error();
            cx.emit_eduke_noop();
            return true;
        }
        let loop_start = cx.img.eduke_program.instructions.len();
        let condition_index = loop_start;
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::JumpIf {
                left: eduke_operand(cx.img, &fields[0]),
                comparison: invert_comparison(comparison),
                right: eduke_operand(cx.img, &fields[1]),
                target: 0,
            });
        cx.eduke_break_stack.push(Vec::new());
        compile_eduke_body(cx, lx, includes, fields.get(2..).unwrap_or_default());
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Jump { target: loop_start });
        let loop_end = cx.img.eduke_program.instructions.len();
        patch_eduke_conditional(cx, condition_index, loop_end);
        patch_eduke_breaks(cx, loop_end);
        return true;
    }

    if command == "ifrnd" {
        let fields = eduke_line_fields(lx);
        compile_eduke_ifrnd_line(cx, lx, includes, &fields);
        return true;
    }

    if eduke_builtin_if_command(&command) {
        let fields = eduke_line_fields(lx);
        let Some((condition_operands, left, comparison, right)) =
            eduke_builtin_condition(cx.img, &command, &fields)
        else {
            eduke_bad_arity(cx);
            return true;
        };
        if fields.len() < condition_operands {
            eduke_bad_arity(cx);
            return true;
        }
        compile_eduke_if_line(
            cx,
            lx,
            includes,
            left,
            comparison,
            right,
            &fields,
            condition_operands,
        );
        return true;
    }

    if let Some(comparison) = eduke_comparison(&command) {
        let fields = eduke_line_fields(lx);
        if fields.len() < 2 {
            cx.record_error();
            cx.emit_eduke_noop();
            return true;
        }
        let (left, right, body_start) = eduke_condition(cx.img, &fields);
        compile_eduke_if_line(
            cx, lx, includes, left, comparison, right, &fields, body_start,
        );
        return true;
    }

    if command == "state" {
        if let Some(name) = lx.next_token() {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::CallState { name: name.into() });
        } else {
            cx.record_error();
            cx.emit_eduke_noop();
        }
        return true;
    }

    if command == "return" {
        let _ = lx.rest_of_line();
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Return);
        return true;
    }

    if command == "palfrom" {
        // Chocolate accepts the four operands across line breaks and stops
        // when the next CON keyword begins (for example, `palfrom 32\n16
        // resetcount`). Keep that continuation behavior for EDuke scripts.
        let mut fields = vec![command];
        while fields.len() <= 4 && peek_keyword(lx, ConDialect::EDuke32).is_none() {
            let Some(field) = lx.next_token() else {
                break;
            };
            fields.push(field);
        }
        compile_eduke_fields(cx, &fields);
        return true;
    }

    if in_body(cx) && command == "move" {
        parse_eduke_actor_script_command(cx, lx, &command);
        return true;
    }

    if in_body(cx) && matches!(command.as_str(), "action" | "ai") {
        let mut fields = vec![command];
        fields.extend(eduke_line_fields(lx));
        compile_eduke_fields(cx, &fields);
        return true;
    }

    if in_body(cx)
        && matches!(
            command.as_str(),
            "strength"
                | "addkills"
                | "addstrength"
                | "cstat"
                | "cstator"
                | "cactor"
                | "spritepal"
                | "clipdist"
                | "count"
                | "sleeptime"
                | "sizeto"
                | "sizeat"
                | "fall"
                | "resetactioncount"
                | "resetcount"
                | "getlastpal"
                | "addphealth"
        )
    {
        let mut fields = vec![command];
        fields.extend(eduke_line_fields(lx));
        compile_eduke_fields(cx, &fields);
        return true;
    }

    if command == "break" {
        let _ = lx.rest_of_line();
        if let Some(breaks) = cx.eduke_break_stack.last_mut() {
            let jump = cx.img.eduke_program.instructions.len();
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Jump { target: 0 });
            breaks.push(jump);
        } else {
            emit_eduke_body_break(cx);
        }
        return true;
    }

    if command == "switch" {
        parse_eduke_switch(cx, lx, includes);
        return true;
    }

    if let Some((kind, is_set)) = eduke_struct_command(&command) {
        let fields = eduke_line_fields(lx);
        compile_eduke_struct_fields(cx, kind, is_set, &fields);
        return true;
    }
    if let Some((base, member_path)) = split_eduke_struct_command(&command) {
        if let Some((kind, is_set)) = eduke_struct_command(base) {
            let mut fields = vec![member_path.to_owned()];
            fields.extend(eduke_line_fields(lx));
            compile_eduke_struct_fields(cx, kind, is_set, &fields);
            return true;
        }
    }
    if let Some((scope, is_set)) = eduke_scoped_var_command(&command) {
        let fields = eduke_line_fields(lx);
        compile_eduke_scoped_var_fields(cx, scope, is_set, &fields);
        return true;
    }
    if let Some((base, member_path)) = split_eduke_scoped_var_command(&command) {
        if let Some((scope, is_set)) = eduke_scoped_var_command(base) {
            let mut fields = vec![member_path.to_owned()];
            fields.extend(eduke_line_fields(lx));
            compile_eduke_scoped_var_fields(cx, scope, is_set, &fields);
            return true;
        }
    }

    if !matches!(
        command.as_str(),
        "set"
            | "add"
            | "sub"
            | "mul"
            | "div"
            | "mod"
            | "and"
            | "or"
            | "xor"
            | "shiftl"
            | "shiftr"
            | "setarray"
            | "getarray"
            | "mulscale"
            | "divscale"
            | "scalevar"
            | "sin"
            | "cos"
            | "getangle"
            | "abs"
            | "clamp"
            | "calchypotenuse"
            | "getflorzofslope"
            | "getceilzofslope"
            | "cansee"
            | "hitscan"
            | "updatesector"
            | "updatesectorz"
            | "ldist"
            | "dist"
            | "headspritestat"
            | "nextspritestat"
            | "headspritesect"
            | "nextspritesect"
            | "changespritestat"
            | "changespritesect"
            | "setsprite"
            | "spawn"
            | "spawnvar"
            | "espawn"
            | "espawnvar"
            | "qspawn"
            | "qspawnvar"
            | "eqspawn"
            | "eqspawnvar"
            | "insertspriteq"
            | "shoot"
            | "shootvar"
            | "eshoot"
            | "eshootvar"
            | "zshoot"
            | "zshootvar"
            | "ezshoot"
            | "ezshootvar"
            | "qsprintf"
            | "rand"
            | "randvar"
            | "displayrand"
            | "displayrandvar"
            | "randvarvar"
            | "displayrandvarvar"
            | "sectorofwall"
            | "inittimer"
            | "getticks"
            | "setgamepalette"
            | "endofgame"
            | "actorsound"
            | "stopactorsound"
            | "findnearactor3d"
            | "globalsoundvar"
            | "guniqhudid"
            | "debris"
            | "guts"
            | "hitradius"
            | "lotsofglass"
            | "money"
            | "mail"
            | "paper"
            | "operate"
            | "respawnhitag"
            | "resetplayer"
            | "lineintersect"
            | "resizearray"
            | "setactorsoundpitch"
            | "spawnceilingglass"
            | "getzrange"
            | "rotatesprite"
            | "minitext"
            | "gametext"
            | "gametextz"
            | "screentext"
            | "digitalnumber"
            | "digitalnumberz"
            | "freezerhurtowner"
            | "echo"
            | "definegamefuncname"
            | "exit"
            | "ifactorsound"
            | "operatemasterswitches"
            | "prependstate"
            | "sqrt"
            | "mikesnd"
            | "soundvar"
            | "ifrnd"
            | "killit"
            | "addammo"
            | "addammovar"
            | "addweapon"
            | "addweaponvar"
            | "addinventory"
            | "addinventoryvar"
            | "palfrom"
            | "quote"
            | "sound"
            | "soundonce"
            | "stopsound"
            | "globalsound"
            | "pkick"
            | "tossweapon"
            | "tip"
    ) {
        return false;
    }

    let fields = eduke_line_fields(lx);
    let mut all_fields = Vec::with_capacity(fields.len() + 1);
    all_fields.push(command);
    all_fields.extend(fields);
    compile_eduke_fields(cx, &all_fields);
    true
}

fn compile_eduke_if_line(
    cx: &mut Cx<'_>,
    lx: &mut Lexer<'_>,
    includes: &mut dyn IncludeResolver,
    left: EdOperand,
    comparison: EdComparison,
    right: EdOperand,
    fields: &[String],
    body_start: usize,
) {
    let instruction_index = cx.img.eduke_program.instructions.len();
    cx.img
        .eduke_program
        .instructions
        .push(EdInstruction::JumpIf {
            left,
            comparison: invert_comparison(comparison),
            right,
            target: instruction_index + 1,
        });
    if let Some(else_index) = fields
        .iter()
        .enumerate()
        .skip(body_start)
        .find_map(|(index, field)| field.eq_ignore_ascii_case("else").then_some(index))
    {
        compile_eduke_field_sequence(cx, &fields[body_start..else_index]);
        let skip_else_index = cx.img.eduke_program.instructions.len();
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Jump { target: 0 });
        let else_start = cx.img.eduke_program.instructions.len();
        patch_eduke_conditional(cx, instruction_index, else_start);
        if else_index + 1 < fields.len() {
            compile_eduke_field_sequence(cx, &fields[else_index + 1..]);
        } else {
            compile_eduke_body(cx, lx, includes, &[]);
        }
        let after_else = cx.img.eduke_program.instructions.len();
        patch_eduke_jump(cx, skip_else_index, after_else);
        return;
    }
    let pending_base = cx.eduke_pending_ifelse.len();
    let pending_index = cx.eduke_pending_ifelse.len();
    cx.eduke_pending_ifelse.push(EdPendingIfElse {
        condition_instruction: instruction_index,
        skip_jump: usize::MAX,
    });
    let mut inline_body = fields.get(body_start..).unwrap_or_default().to_vec();
    let mut nested_condition_compiled = false;
    if inline_body.len() == 1
        && inline_body[0].to_ascii_lowercase().starts_with("if")
        && keyword_index(&inline_body[0]).is_some()
    {
        skip_eduke_comments(cx, lx, includes);
        if !peek_eduke_token(lx)
            .is_some_and(|token| token == "{" || token.eq_ignore_ascii_case("else") || token == "}")
        {
            let nested_command = inline_body[0].to_ascii_lowercase();
            let nested_fields = eduke_line_fields(lx);
            if let Some((nested_operands, nested_left, nested_comparison, nested_right)) =
                eduke_builtin_condition(cx.img, &nested_command, &nested_fields)
                && nested_fields.len() >= nested_operands
            {
                compile_eduke_if_line(
                    cx,
                    lx,
                    includes,
                    nested_left,
                    nested_comparison,
                    nested_right,
                    &nested_fields,
                    nested_operands,
                );
                nested_condition_compiled = true;
            } else {
                inline_body.extend(nested_fields);
            }
        }
    }
    if !nested_condition_compiled {
        compile_eduke_body(cx, lx, includes, &inline_body);
    }
    let after_true = cx.img.eduke_program.instructions.len();
    skip_eduke_comments(cx, lx, includes);
    let next_token = peek_eduke_token(lx);
    if next_token
        .as_deref()
        .is_some_and(|token| token.eq_ignore_ascii_case("else") || token == "}")
    {
        let skip_else_index = cx.img.eduke_program.instructions.len();
        cx.img.eduke_program.instructions.push(EdInstruction::Jump {
            target: skip_else_index + 1,
        });
        cx.eduke_pending_ifelse[pending_index].skip_jump = skip_else_index;
        patch_eduke_conditional(cx, instruction_index, skip_else_index + 1);
        if next_token.is_some_and(|token| token.eq_ignore_ascii_case("else")) {
            compile_eduke_pending_else_chain(cx, lx, includes, pending_base);
        }
    } else {
        cx.eduke_pending_ifelse.truncate(pending_base);
        patch_eduke_conditional(cx, instruction_index, after_true);
    }
}

/// Pair source-level `else` tokens with the most recently opened EDuke32
/// condition, even when a brace closes between the condition body and `else`.
fn compile_eduke_pending_else_chain(
    cx: &mut Cx<'_>,
    lx: &mut Lexer<'_>,
    includes: &mut dyn IncludeResolver,
    pending_base: usize,
) {
    let mut skip_jumps = Vec::new();
    loop {
        skip_eduke_comments(cx, lx, includes);
        if cx.eduke_pending_ifelse.len() <= pending_base
            || !peek_eduke_token(lx).is_some_and(|token| token.eq_ignore_ascii_case("else"))
        {
            break;
        }
        let _ = lx.next_token();
        let Some(pending) = cx.eduke_pending_ifelse.pop() else {
            break;
        };
        skip_jumps.push(pending.skip_jump);
        let else_start = cx.img.eduke_program.instructions.len();
        patch_eduke_conditional(cx, pending.condition_instruction, else_start);
        let else_fields = eduke_line_fields(lx);
        if !compile_eduke_nested_condition(cx, lx, includes, &else_fields) {
            compile_eduke_body(cx, lx, includes, &else_fields);
        }
    }
    let after_else_chain = cx.img.eduke_program.instructions.len();
    for skip_jump in skip_jumps {
        patch_eduke_jump(cx, skip_jump, after_else_chain);
    }
}

/// Compile a nested actor condition whose operands end the current source line.
/// Its body is then read from the following line or brace block.
fn compile_eduke_nested_condition(
    cx: &mut Cx<'_>,
    lx: &mut Lexer<'_>,
    includes: &mut dyn IncludeResolver,
    fields: &[String],
) -> bool {
    let Some(command) = fields
        .first()
        .map(|field| eduke_legacy_alias(&field.to_ascii_lowercase()))
    else {
        return false;
    };
    let operands = &fields[1..];
    let Some((condition_operands, left, comparison, right)) =
        eduke_builtin_condition(cx.img, &command, operands).or_else(|| {
            let comparison = eduke_comparison(&command)?;
            let (left, right, condition_operands) = eduke_condition(cx.img, operands);
            Some((condition_operands, left, comparison, right))
        })
    else {
        return false;
    };
    if operands.len() < condition_operands {
        return false;
    }
    compile_eduke_if_line(
        cx,
        lx,
        includes,
        left,
        comparison,
        right,
        operands,
        condition_operands,
    );
    true
}

fn eduke_builtin_if_command(command: &str) -> bool {
    matches!(
        command,
        "ifcount"
            | "ifaction"
            | "ifactioncount"
            | "ifai"
            | "ifmove"
            | "ifactor"
            | "ifstrength"
            | "ifwasweapon"
            | "ifspawnedby"
            | "ifspritepal"
            | "ifmultiplayer"
            | "ifpdistl"
            | "ifpdistg"
            | "ifangdiffl"
            | "ifawayfromwall"
            | "ifactorsound"
            | "ifonwater"
            | "ifinouterspace"
            | "ifinwater"
            | "ifoutside"
            | "ifphealthl"
            | "ifpinventory"
            | "ifgapzl"
            | "iffloordistl"
            | "ifceilingdistl"
            | "ifp"
            | "ifdead"
            | "ifnotmoving"
            | "ifhitspace"
            | "ifsquished"
            | "ifhitweapon"
            | "ifcansee"
            | "ifcanseetarget"
            | "ifcanshoottarget"
            | "ifnosounds"
            | "ifbulletnear"
            | "ifrespawn"
            | "ifactornotstayput"
            | "ifgotweaponce"
    )
}

fn eduke_actor_member(member: &str, index: Option<i32>) -> EdOperand {
    EdOperand::StructMember {
        kind: EdStructKind::Actor,
        id: Box::new(EdOperand::CurrentActor),
        member: member.to_owned(),
        member_index: index.map(|index| Box::new(EdOperand::Immediate(index))),
    }
}

fn eduke_actor_sector_member(member: &str) -> EdOperand {
    EdOperand::StructMember {
        kind: EdStructKind::Sector,
        id: Box::new(eduke_actor_member("sectnum", None)),
        member: member.to_owned(),
        member_index: None,
    }
}

/// Build bytecode operands for EDuke32's actor-state condition commands.
/// The returned count is the number of condition operands before the body.
fn eduke_builtin_condition(
    img: &ScriptImage,
    command: &str,
    fields: &[String],
) -> Option<(usize, EdOperand, EdComparison, EdOperand)> {
    let argument = fields
        .first()
        .map(|value| {
            if matches!(command, "ifaction" | "ifai" | "ifmove") {
                eduke_pointer_value(img, value)
                    .map(EdOperand::Immediate)
                    .unwrap_or_else(|| eduke_operand(img, value))
            } else {
                eduke_operand(img, value)
            }
        })
        .unwrap_or(EdOperand::Immediate(0));
    let condition = match command {
        "ifcount" => (
            1,
            eduke_actor_member("htg_t", Some(0)),
            EdComparison::GreaterEqual,
            argument.clone(),
        ),
        "ifaction" => (
            1,
            eduke_actor_member("htg_t", Some(4)),
            EdComparison::Equal,
            argument.clone(),
        ),
        "ifactioncount" => (
            1,
            eduke_actor_member("htg_t", Some(2)),
            EdComparison::GreaterEqual,
            argument.clone(),
        ),
        "ifai" => (
            1,
            eduke_actor_member("htg_t", Some(5)),
            EdComparison::Equal,
            argument.clone(),
        ),
        "ifmove" => (
            1,
            eduke_actor_member("htg_t", Some(1)),
            EdComparison::Equal,
            argument.clone(),
        ),
        "ifactor" => (
            1,
            eduke_actor_member("picnum", None),
            EdComparison::Equal,
            argument.clone(),
        ),
        "ifstrength" => (
            1,
            eduke_actor_member("extra", None),
            EdComparison::LessEqual,
            argument.clone(),
        ),
        "ifwasweapon" | "ifspawnedby" => (
            1,
            eduke_actor_member("htpicnum", None),
            EdComparison::Equal,
            argument.clone(),
        ),
        "ifspritepal" => (
            1,
            eduke_actor_member("pal", None),
            EdComparison::Equal,
            argument,
        ),
        "ifmultiplayer" => (
            0,
            EdOperand::Variable("MULTIMODE".into()),
            EdComparison::Greater,
            EdOperand::Immediate(1),
        ),
        "ifpdistl" => (1, EdOperand::PlayerDistance, EdComparison::Less, argument),
        "ifpdistg" => (
            1,
            EdOperand::PlayerDistance,
            EdComparison::Greater,
            argument,
        ),
        "ifangdiffl" => (
            1,
            EdOperand::ActorPlayerAngleDifference,
            EdComparison::LessEqual,
            argument,
        ),
        "ifawayfromwall" => (
            0,
            EdOperand::ActorAwayFromWall,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifactorsound" => {
            if fields.len() < 2 {
                return None;
            }
            (
                2,
                EdOperand::ActorHasSound {
                    actor: Box::new(eduke_operand(img, &fields[0])),
                    sound: Box::new(eduke_operand(img, &fields[1])),
                },
                EdComparison::Equal,
                EdOperand::Immediate(1),
            )
        }
        "ifonwater" => (
            0,
            EdOperand::ActorOnWater,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifinouterspace" => (
            0,
            EdOperand::ActorInOuterSpace,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifinwater" => (
            0,
            eduke_actor_sector_member("lotag"),
            EdComparison::Equal,
            EdOperand::Immediate(2),
        ),
        "ifoutside" => (
            0,
            eduke_actor_sector_member("ceilingstat"),
            EdComparison::BitAndNonZero,
            EdOperand::Immediate(1),
        ),
        "ifphealthl" => (
            1,
            EdOperand::StructMember {
                kind: EdStructKind::Player,
                id: Box::new(EdOperand::CurrentPlayer),
                member: "extra".to_owned(),
                member_index: None,
            },
            EdComparison::Less,
            argument,
        ),
        "ifpinventory" => {
            if fields.len() < 2 {
                return None;
            }
            (
                2,
                EdOperand::PlayerInventoryDiffers {
                    item: Box::new(eduke_operand(img, &fields[0])),
                    amount: Box::new(eduke_operand(img, &fields[1])),
                },
                EdComparison::Equal,
                EdOperand::Immediate(1),
            )
        }
        "ifgapzl" => (1, EdOperand::ActorSectorGap, EdComparison::Less, argument),
        "iffloordistl" => (
            1,
            EdOperand::ActorFloorDistance,
            EdComparison::Less,
            argument.clone(),
        ),
        "ifceilingdistl" => (
            1,
            EdOperand::ActorCeilingDistance,
            EdComparison::Less,
            argument,
        ),
        "ifp" => {
            let (operand_count, flags) = eduke_ifp_flags(img, fields)?;
            (
                operand_count,
                EdOperand::PlayerFlags(flags),
                EdComparison::Equal,
                EdOperand::Immediate(1),
            )
        }
        "ifdead" => (
            0,
            EdOperand::ActorDead,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifnotmoving" => (
            0,
            eduke_actor_member("htmovflag", None),
            EdComparison::Equal,
            EdOperand::Immediate(0),
        ),
        "ifhitspace" => (
            0,
            EdOperand::PlayerInputBits(1 << 29),
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifsquished" => (
            0,
            EdOperand::ActorSquished,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifhitweapon" => (
            0,
            EdOperand::ActorHitByWeapon,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifcansee" => (
            0,
            EdOperand::ActorCanSeePlayer,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifcanseetarget" => (
            0,
            EdOperand::ActorCanSeeTarget,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifcanshoottarget" => (
            0,
            EdOperand::ActorCanShootTarget,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifnosounds" => (
            0,
            EdOperand::ActorHasActiveSound,
            EdComparison::Equal,
            EdOperand::Immediate(0),
        ),
        "ifbulletnear" => (
            0,
            EdOperand::ActorBulletNear,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifrespawn" => (
            0,
            EdOperand::ActorRespawnEnabled,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifactornotstayput" => (
            0,
            EdOperand::ActorNotStayPut,
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        "ifgotweaponce" => (
            1,
            EdOperand::ActorWeaponReservation {
                register: Box::new(eduke_operand(img, fields.first()?)),
            },
            EdComparison::Equal,
            EdOperand::Immediate(1),
        ),
        _ => return None,
    };
    Some(condition)
}

/// Parse the OR-combined flag tokens consumed by EDuke32's `ifp` condition.
/// Flag arguments end at the first inline CON command or brace token.
fn eduke_ifp_flags(img: &ScriptImage, fields: &[String]) -> Option<(usize, i32)> {
    let mut count = 0usize;
    let mut flags = 0i32;
    while let Some(field) = fields.get(count) {
        if field == "{" || field == "}" || eduke_inline_command_token(field) {
            break;
        }
        flags |= eduke_constant(img, field)?;
        count += 1;
    }
    (count > 0).then_some((count, flags))
}

fn compile_eduke_inline_condition(
    cx: &mut Cx<'_>,
    operands: &[String],
    condition_operands: usize,
    left: EdOperand,
    comparison: EdComparison,
    right: EdOperand,
) {
    let instruction = cx.img.eduke_program.instructions.len();
    cx.img
        .eduke_program
        .instructions
        .push(EdInstruction::JumpIf {
            left,
            comparison: invert_comparison(comparison),
            right,
            target: 0,
        });
    if let Some(else_index) = operands
        .iter()
        .enumerate()
        .skip(condition_operands)
        .find_map(|(index, field)| field.eq_ignore_ascii_case("else").then_some(index))
    {
        compile_eduke_field_sequence(cx, &operands[condition_operands..else_index]);
        let skip_else = cx.img.eduke_program.instructions.len();
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Jump { target: 0 });
        let else_start = cx.img.eduke_program.instructions.len();
        patch_eduke_conditional(cx, instruction, else_start);
        compile_eduke_field_sequence(cx, &operands[else_index + 1..]);
        let end = cx.img.eduke_program.instructions.len();
        patch_eduke_jump(cx, skip_else, end);
    } else {
        compile_eduke_field_sequence(cx, &operands[condition_operands..]);
        let end = cx.img.eduke_program.instructions.len();
        patch_eduke_conditional(cx, instruction, end);
    }
}

fn compile_eduke_body(
    cx: &mut Cx<'_>,
    lx: &mut Lexer<'_>,
    includes: &mut dyn IncludeResolver,
    inline_body: &[String],
) {
    if inline_body
        .first()
        .is_some_and(|field| field.eq_ignore_ascii_case("ifrnd"))
    {
        compile_eduke_ifrnd_line(cx, lx, includes, &inline_body[1..]);
    } else if inline_body.first().is_some_and(|field| field == "{") {
        if inline_body.len() == 1 {
            compile_eduke_block(cx, lx, includes);
        } else {
            compile_eduke_field_sequence(cx, inline_body);
        }
    } else if !inline_body.is_empty() {
        // An inline condition (`ife a 0 ife b 0` with the inner body on the
        // next line or in a brace block) needs the lexer for that body:
        // EDuke32 treats the whole inner `if` as the outer one's single
        // statement (Brutal Duke's `ak_finish_reload`).
        if !compile_eduke_nested_condition(cx, lx, includes, inline_body) {
            compile_eduke_field_sequence(cx, inline_body);
        }
    } else {
        skip_eduke_comments(cx, lx, includes);
        if peek_eduke_token(lx).is_some_and(|token| token == "{") {
            let _ = lx.next_token();
            compile_eduke_block(cx, lx, includes);
        } else if !lx.eof()
            && !peek_eduke_token(lx)
                .as_deref()
                .is_some_and(|token| token == "}")
        {
            let _ = parse_command(cx, lx, includes);
        }
    }
}

/// Compile a line-oriented `ifrnd`, including an `else ifrnd` body supplied
/// from the following source line. That case needs the lexer so it can consume
/// the nested branch's brace block instead of leaving the block behind.
fn compile_eduke_ifrnd_line(
    cx: &mut Cx<'_>,
    lx: &mut Lexer<'_>,
    includes: &mut dyn IncludeResolver,
    fields: &[String],
) {
    let Some(threshold) = fields.first() else {
        cx.record_error();
        cx.emit_eduke_noop();
        return;
    };
    let instruction_index = cx.img.eduke_program.instructions.len();
    cx.img
        .eduke_program
        .instructions
        .push(EdInstruction::IfRandom {
            threshold: eduke_operand(cx.img, threshold),
            target: instruction_index + 1,
        });
    if let Some(else_index) = fields
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, field)| field.eq_ignore_ascii_case("else").then_some(index))
    {
        compile_eduke_field_sequence(cx, &fields[1..else_index]);
        let skip_else_index = cx.img.eduke_program.instructions.len();
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Jump { target: 0 });
        let else_start = cx.img.eduke_program.instructions.len();
        if let Some(EdInstruction::IfRandom { target, .. }) =
            cx.img.eduke_program.instructions.get_mut(instruction_index)
        {
            *target = else_start;
        }
        if else_index + 1 < fields.len() {
            compile_eduke_field_sequence(cx, &fields[else_index + 1..]);
        } else {
            compile_eduke_body(cx, lx, includes, &[]);
        }
        let after_else = cx.img.eduke_program.instructions.len();
        patch_eduke_jump(cx, skip_else_index, after_else);
    } else {
        let pending_base = cx.eduke_pending_ifelse.len();
        let pending_index = cx.eduke_pending_ifelse.len();
        cx.eduke_pending_ifelse.push(EdPendingIfElse {
            condition_instruction: instruction_index,
            skip_jump: usize::MAX,
        });
        compile_eduke_body(cx, lx, includes, fields.get(1..).unwrap_or_default());
        let after_true = cx.img.eduke_program.instructions.len();
        skip_eduke_comments(cx, lx, includes);
        let next_token = peek_eduke_token(lx);
        if next_token
            .as_deref()
            .is_some_and(|token| token.eq_ignore_ascii_case("else") || token == "}")
        {
            let skip_else_index = cx.img.eduke_program.instructions.len();
            cx.img.eduke_program.instructions.push(EdInstruction::Jump {
                target: skip_else_index + 1,
            });
            cx.eduke_pending_ifelse[pending_index].skip_jump = skip_else_index;
            patch_eduke_conditional(cx, instruction_index, skip_else_index + 1);
            if next_token.is_some_and(|token| token.eq_ignore_ascii_case("else")) {
                compile_eduke_pending_else_chain(cx, lx, includes, pending_base);
            }
        } else {
            cx.eduke_pending_ifelse.truncate(pending_base);
            patch_eduke_conditional(cx, instruction_index, after_true);
        }
    }
}

fn compile_eduke_field_sequence(cx: &mut Cx<'_>, fields: &[String]) {
    let mut cursor = 0;
    while cursor < fields.len() {
        if fields[cursor] == "}" {
            cx.brace_depth = cx.brace_depth.saturating_sub(1);
            return;
        }
        if fields[cursor] == "{" {
            let close = inline_brace_end(fields, cursor).unwrap_or(fields.len());
            cx.brace_depth += 1;
            compile_eduke_field_sequence(cx, &fields[cursor + 1..close]);
            if close < fields.len() {
                cx.brace_depth = cx.brace_depth.saturating_sub(1);
            }
            cursor = close.saturating_add(1);
            continue;
        }
        let end = inline_command_end(cx.img, fields, cursor).max(cursor + 1);
        compile_eduke_fields(cx, &fields[cursor..end.min(fields.len())]);
        cursor = end.min(fields.len());
    }
}

fn inline_brace_end(fields: &[String], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, field) in fields.iter().enumerate().skip(open) {
        if field == "{" {
            depth = depth.checked_add(1)?;
        } else if field == "}" {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn inline_command_end(img: &ScriptImage, fields: &[String], start: usize) -> usize {
    let Some(token) = fields.get(start) else {
        return start;
    };
    if token == "{" {
        return inline_brace_end(fields, start).map_or(fields.len(), |close| close + 1);
    }
    let command = eduke_legacy_alias(&token.to_ascii_lowercase());
    if command == "killit" {
        return start + 1;
    }
    if command == "ifrnd" {
        let condition_operands = 1;
        let body_start = start + 1 + condition_operands;
        if body_start >= fields.len() || fields[body_start].eq_ignore_ascii_case("else") {
            return body_start;
        }
        let mut end = inline_command_end(img, fields, body_start);
        if fields
            .get(end)
            .is_some_and(|field| field.eq_ignore_ascii_case("else"))
        {
            end = inline_command_end(img, fields, end + 1);
        }
        return end;
    }
    if eduke_comparison(&command).is_some()
        || eduke_while_comparison(&command).is_some()
        || eduke_builtin_if_command(&command)
    {
        let operand_fields = &fields[start + 1..];
        let condition_operands = if eduke_builtin_if_command(&command) {
            eduke_builtin_condition(img, &command, operand_fields)
                .map_or(0, |condition| condition.0)
        } else {
            eduke_condition(img, operand_fields).2
        };
        let body_start = start + 1 + condition_operands;
        if body_start >= fields.len() || fields[body_start] == "else" {
            return body_start;
        }
        let mut end = inline_command_end(img, fields, body_start);
        if fields
            .get(end)
            .is_some_and(|field| field.eq_ignore_ascii_case("else"))
        {
            end = inline_command_end(img, fields, end + 1);
        }
        return end;
    }
    if eduke_noop_command(&command) {
        let mut cursor = start + 1;
        while cursor < fields.len()
            && !eduke_inline_command_token(&fields[cursor])
            && fields[cursor] != "{"
            && fields[cursor] != "}"
        {
            cursor += 1;
        }
        return cursor.min(fields.len());
    }
    let argument_count = if split_eduke_struct_command(&command).is_some() {
        let next = fields.get(start + 2);
        if next.is_some_and(|field| !eduke_inline_command_token(field)) {
            2
        } else {
            1
        }
    } else if split_eduke_scoped_var_command(&command).is_some() {
        1
    } else if eduke_struct_command(&command).is_some() {
        let member = fields
            .get(start + 1)
            .map(String::as_str)
            .unwrap_or_default();
        let kind = eduke_struct_command(&command).map(|(kind, _)| kind);
        let has_indexed_member =
            kind.is_some_and(|kind| eduke_struct_member_is_indexed(kind, member));
        if has_indexed_member { 3 } else { 2 }
    } else if eduke_scoped_var_command(&command).is_some() {
        2
    } else {
        match command.as_str() {
            "endofgame" => usize::from(
                fields
                    .get(start + 1)
                    .is_some_and(|field| !eduke_inline_command_token(field)),
            ),
            "strength" | "addkills" | "addstrength" | "cstat" | "cstator" | "cactor"
            | "spritepal" | "clipdist" | "count" | "sleeptime" => 1,
            "sizeto" | "sizeat" => 2,
            "fall" | "resetactioncount" | "resetcount" | "getlastpal" | "pkick" | "tossweapon"
            | "break" => 0,
            "actorsound" | "stopactorsound" => 2,
            "ifactorsound" => 2,
            "findnearactor3d" => 3,
            "lineintersect" => 14,
            "set" | "add" | "sub" | "mul" | "div" | "mod" | "and" | "or" | "xor" | "shiftl"
            | "shiftr" | "rand" | "randvar" | "randvarvar" | "mulscale" | "divscale"
            | "scalevar" | "ldist" | "dist" | "changespritestat" | "changespritesect"
            | "headspritestat" | "nextspritestat" | "headspritesect" | "nextspritesect"
            | "setactorvar" | "getactorvar" | "setplayervar" | "getplayervar" | "addweapon"
            | "addweaponvar" | "addammo" | "addammovar" | "addinventory" | "addinventoryvar"
            | "sectorofwall" | "debris" | "guts" => 2,
            "sound" | "soundvar" | "soundonce" | "stopsound" | "globalsound" | "globalsoundvar"
            | "quote" | "playsound" | "money" | "mail" | "paper" | "lotsofglass" | "getticks"
            | "setgamepalette" | "inittimer" => 1,
            "palfrom" => 4,
            "getarray" => 3,
            "getangle" => 3,
            "sqrt" => 2,
            "setsprite" => 4,
            "cansee" => 9,
            "getzrange" => 10,
            "hitscan" => 14,
            "rotatesprite" => 12,
            "guniqhudid" => 1,
            "minitext" => 5,
            "gametext" => 11,
            "gametextz" => 12,
            "screentext" => 20,
            "digitalnumber" => 11,
            "digitalnumberz" => 12,
            "hitradius" => 5,
            "ifpinventory" => 2,
            "setarray" => {
                if fields
                    .get(start + 1)
                    .is_some_and(|field| field.contains('['))
                {
                    2
                } else {
                    3
                }
            }
            "state" | "prependstate" | "spawn" | "spawnvar" | "espawn" | "espawnvar" | "qspawn"
            | "qspawnvar" | "eqspawn" | "eqspawnvar" | "insertspriteq" | "return" | "exit"
            | "nullop" | "abs" | "ifrnd" | "addphealth" => 1,
            "" | "}" | "else" => 0,
            _ => {
                if command.starts_with("if") && keyword_index(&command).is_some() {
                    if matches!(
                        command.as_str(),
                        "ifmultiplayer" | "ifinwater" | "ifoutside" | "ifinouterspace"
                    ) {
                        0
                    } else {
                        1
                    }
                } else if keyword_index(&command).is_some() {
                    0
                } else {
                    0
                }
            }
        }
    };
    (start + 1 + argument_count).min(fields.len())
}

fn eduke_noop_command(command: &str) -> bool {
    matches!(command, "action" | "ai" | "move" | "shoot")
}

fn parse_eduke_switch(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, includes: &mut dyn IncludeResolver) {
    let switch_line = cx.eduke_line;
    let fields = eduke_line_fields(lx);
    let Some(value) = fields.first() else {
        cx.record_error();
        cx.emit_eduke_noop();
        return;
    };
    let switch_index = cx.img.eduke_program.instructions.len();
    cx.img
        .eduke_program
        .instructions
        .push(EdInstruction::Switch {
            value: eduke_operand(cx.img, value),
            cases: Vec::new(),
            default_target: 0,
        });
    cx.eduke_switch_depth = cx.eduke_switch_depth.saturating_add(1);
    cx.eduke_break_stack.push(Vec::new());
    let mut cases = Vec::new();
    let mut default_target = None;
    let mut closed = false;
    let mut last_token = String::new();
    loop {
        skip_eduke_comments(cx, lx, includes);
        let Some(token) = peek_eduke_token(lx) else {
            break;
        };
        last_token.clone_from(&token);
        if token.eq_ignore_ascii_case("endswitch") {
            let _ = lx.next_token();
            let _ = lx.rest_of_line();
            closed = true;
            cx.eduke_switch_depth = cx.eduke_switch_depth.saturating_sub(1);
            break;
        }
        if token.eq_ignore_ascii_case("case") {
            cx.eduke_command_head = "case".to_owned();
            cx.eduke_line = lx.line;
            let _ = lx.next_token();
            let case_fields = eduke_line_fields(lx);
            let target = cx.img.eduke_program.instructions.len();
            let mut field_index = 0;
            while field_index < case_fields.len() {
                if case_fields[field_index].eq_ignore_ascii_case("case") {
                    field_index += 1;
                }
                let Some(case_token) = case_fields.get(field_index) else {
                    break;
                };
                if let Some(case_value) = eduke_constant(cx.img, case_token.trim_end_matches(':')) {
                    cases.push((case_value, target));
                    field_index += 1;
                } else {
                    cx.record_error_as("case:unresolved-value");
                    break;
                }
                if field_index >= case_fields.len()
                    || !case_fields[field_index].eq_ignore_ascii_case("case")
                {
                    break;
                }
            }
            if field_index == 0 {
                cx.record_error_as("case:missing-value");
            }
            if field_index < case_fields.len() {
                compile_eduke_field_sequence(cx, &case_fields[field_index..]);
            }
            continue;
        }
        if token.eq_ignore_ascii_case("default") {
            cx.eduke_command_head = "default".to_owned();
            cx.eduke_line = lx.line;
            let _ = lx.next_token();
            let default_fields = eduke_line_fields(lx);
            default_target = Some(cx.img.eduke_program.instructions.len());
            if !default_fields.is_empty() {
                compile_eduke_field_sequence(cx, &default_fields);
            }
            continue;
        }
        if parse_command(cx, lx, includes) {
            if cx.eduke_nested_endswitch_seen {
                cx.eduke_nested_endswitch_seen = false;
                closed = true;
                cx.eduke_switch_depth = cx.eduke_switch_depth.saturating_sub(1);
                break;
            }
            if lx.eof() {
                break;
            }
        }
    }
    if !closed {
        cx.record_error_as(&format!(
            "switch:unterminated:{last_token}:start{switch_line}"
        ));
        cx.eduke_switch_depth = cx.eduke_switch_depth.saturating_sub(1);
    }
    let end = cx.img.eduke_program.instructions.len();
    let default_target = default_target.unwrap_or(end);
    if let Some(EdInstruction::Switch {
        cases: compiled_cases,
        default_target: compiled_default,
        ..
    }) = cx.img.eduke_program.instructions.get_mut(switch_index)
    {
        *compiled_cases = cases;
        *compiled_default = default_target;
    }
    patch_eduke_breaks(cx, end);
}

fn patch_eduke_breaks(cx: &mut Cx<'_>, target: usize) {
    if let Some(breaks) = cx.eduke_break_stack.pop() {
        for instruction in breaks {
            patch_eduke_jump(cx, instruction, target);
        }
    }
}

fn compile_eduke_fields(cx: &mut Cx<'_>, fields: &[String]) {
    let Some(command) = fields
        .first()
        .map(|field| eduke_legacy_alias(&field.to_ascii_lowercase()))
    else {
        return;
    };
    cx.eduke_command_head.clone_from(&command);
    let operands = &fields[1..];
    if let Some((base, member_path)) = split_eduke_struct_command(&command) {
        if let Some((kind, is_set)) = eduke_struct_command(base) {
            let mut inline_fields = vec![member_path.to_owned()];
            inline_fields.extend(operands.iter().cloned());
            compile_eduke_struct_fields(cx, kind, is_set, &inline_fields);
            return;
        }
    }
    if let Some((base, member_path)) = split_eduke_scoped_var_command(&command) {
        if let Some((scope, is_set)) = eduke_scoped_var_command(base) {
            let mut inline_fields = vec![member_path.to_owned()];
            inline_fields.extend(operands.iter().cloned());
            compile_eduke_scoped_var_fields(cx, scope, is_set, &inline_fields);
            return;
        }
    }
    let first_command_end = inline_command_end(cx.img, fields, 0).max(1);
    if first_command_end < fields.len() && eduke_inline_command_token(&fields[first_command_end]) {
        compile_eduke_fields(cx, &fields[..first_command_end]);
        compile_eduke_field_sequence(cx, &fields[first_command_end..]);
        return;
    }
    match command.as_str() {
        "addammo" | "addammovar" | "addweapon" | "addweaponvar" if operands.len() == 2 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::PlayerWeapon {
                    add_weapon: command.starts_with("addweapon"),
                    weapon: eduke_operand(cx.img, &operands[0]),
                    ammo: eduke_operand(cx.img, &operands[1]),
                });
        }
        "addinventory" | "addinventoryvar" if operands.len() == 2 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::PlayerInventory {
                    item: eduke_operand(cx.img, &operands[0]),
                    amount: eduke_operand(cx.img, &operands[1]),
                });
        }
        "action" | "move" | "ai" => {
            compile_eduke_actor_script_fields(cx, &command, operands);
        }
        "strength" | "addkills" | "addstrength" | "cstat" | "cstator" | "cactor" | "spritepal"
        | "clipdist" | "count" | "sleeptime" | "sizeto" | "sizeat" | "fall"
        | "resetactioncount" | "resetcount" | "getlastpal" | "addphealth" => {
            compile_eduke_actor_command_fields(cx, &command, operands);
        }
        "killit" if operands.is_empty() => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::KillIt),
        "tip" if operands.is_empty() => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::PlayerTip),
        "guniqhudid" if operands.len() == 1 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SetHudUniqueId(eduke_operand(
                    cx.img,
                    &operands[0],
                )))
        }
        "rotatesprite" if operands.len() == 12 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::RotateSprite(
                    operands
                        .iter()
                        .map(|operand| eduke_operand(cx.img, operand))
                        .collect(),
                ))
        }
        "minitext" if operands.len() == 5 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Minitext {
                    x: eduke_operand(cx.img, &operands[0]),
                    y: eduke_operand(cx.img, &operands[1]),
                    quote: eduke_operand(cx.img, &operands[2]),
                    shade: eduke_operand(cx.img, &operands[3]),
                    palette: eduke_operand(cx.img, &operands[4]),
                })
        }
        "gametext" if operands.len() == 11 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::GameText(
                    operands
                        .iter()
                        .map(|operand| eduke_operand(cx.img, operand))
                        .collect(),
                ))
        }
        "gametextz" if operands.len() == 12 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::GameText(
                    operands
                        .iter()
                        .map(|operand| eduke_operand(cx.img, operand))
                        .collect(),
                ))
        }
        "screentext" if operands.len() == 20 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::ScreenText(
                    operands
                        .iter()
                        .map(|operand| eduke_operand(cx.img, operand))
                        .collect(),
                ))
        }
        "digitalnumber" if operands.len() == 11 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::DigitalNumber(
                    operands
                        .iter()
                        .map(|operand| eduke_operand(cx.img, operand))
                        .collect(),
                ))
        }
        "digitalnumberz" if operands.len() == 12 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::DigitalNumber(
                    operands
                        .iter()
                        .map(|operand| eduke_operand(cx.img, operand))
                        .collect(),
                ))
        }
        "guniqhudid" | "rotatesprite" | "minitext" | "gametext" | "gametextz" | "screentext"
        | "digitalnumber" | "digitalnumberz" => eduke_bad_arity(cx),
        "sound" | "soundvar" | "soundonce" | "globalsound" | "globalsoundvar"
            if operands.len() == 1 =>
        {
            let mode = match command.as_str() {
                "soundonce" => EdSoundMode::ActorOnce,
                "globalsound" | "globalsoundvar" => EdSoundMode::Global,
                _ => EdSoundMode::Actor,
            };
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::PlaySound {
                    sound: eduke_operand(cx.img, &operands[0]),
                    mode,
                    actor: None,
                });
        }
        "actorsound" if operands.len() == 2 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::PlaySound {
                    sound: eduke_operand(cx.img, &operands[1]),
                    mode: EdSoundMode::Actor,
                    actor: Some(eduke_operand(cx.img, &operands[0])),
                });
        }
        "mikesnd" if operands.is_empty() => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::PlaySound {
                    sound: eduke_actor_member("yvel", None),
                    mode: EdSoundMode::Actor,
                    actor: None,
                });
        }
        "stopsound" if operands.len() == 1 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::StopSound {
                    sound: eduke_operand(cx.img, &operands[0]),
                    actor: None,
                })
        }
        "stopactorsound" if operands.len() == 2 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::StopSound {
                    actor: Some(eduke_operand(cx.img, &operands[0])),
                    sound: eduke_operand(cx.img, &operands[1]),
                })
        }
        "setactorsoundpitch" if operands.len() == 3 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SetActorSoundPitch {
                    actor: eduke_operand(cx.img, &operands[0]),
                    sound: eduke_operand(cx.img, &operands[1]),
                    pitch_offset: eduke_operand(cx.img, &operands[2]),
                });
        }
        "sound" | "soundvar" | "soundonce" | "globalsound" | "globalsoundvar" | "stopsound"
        | "actorsound" | "stopactorsound" | "setactorsoundpitch" => eduke_bad_arity(cx),
        "rand" | "randvar" | "randvarvar" | "displayrand" | "displayrandvar"
        | "displayrandvarvar"
            if !operands.is_empty() =>
        {
            let display = command.starts_with("display");
            let maximum = if display && command == "displayrand" {
                if operands.len() != 1 {
                    eduke_bad_arity(cx);
                    return;
                }
                None
            } else if operands.len() >= 2 {
                Some(eduke_operand(cx.img, &operands[1]))
            } else {
                eduke_bad_arity(cx);
                return;
            };
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::RandomRange {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    maximum,
                    display,
                });
        }
        "ifrnd" if !operands.is_empty() => compile_eduke_ifrnd(cx, operands),
        "qsprintf" if operands.len() >= 2 => {
            let Some(output_quote) = eduke_constant(cx.img, &operands[0])
                .and_then(|value| usize::try_from(value).ok())
                .filter(|quote| *quote < EDUKE_MAX_QUOTES)
            else {
                eduke_bad_arity(cx);
                return;
            };
            let Some(format_quote) = eduke_constant(cx.img, &operands[1])
                .and_then(|value| usize::try_from(value).ok())
                .filter(|quote| *quote < EDUKE_MAX_QUOTES)
            else {
                eduke_bad_arity(cx);
                return;
            };
            let arguments = operands[2..]
                .iter()
                .map(|operand| eduke_operand(cx.img, operand))
                .collect();
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Qsprintf {
                    output_quote,
                    format_quote,
                    arguments,
                });
        }
        "set" if operands.len() >= 2 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Assign {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    value: eduke_operand(cx.img, &operands[1]),
                })
        }
        "add" | "sub" | "mul" | "div" | "mod" | "and" | "or" | "xor" | "shiftl" | "shiftr"
            if operands.len() >= 2 =>
        {
            let operator = match command.as_str() {
                "add" => EdBinaryOperator::Add,
                "sub" => EdBinaryOperator::Subtract,
                "mul" => EdBinaryOperator::Multiply,
                "div" => EdBinaryOperator::Divide,
                "mod" => EdBinaryOperator::Modulo,
                "and" => EdBinaryOperator::And,
                "or" => EdBinaryOperator::Or,
                "xor" => EdBinaryOperator::Xor,
                "shiftl" => EdBinaryOperator::ShiftLeft,
                _ => EdBinaryOperator::ShiftRight,
            };
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Binary {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    operator,
                    right: eduke_operand(cx.img, &operands[1]),
                });
        }
        "debris" if operands.len() == 2 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Debris {
                    tile: eduke_operand(cx.img, &operands[0]),
                    count: eduke_operand(cx.img, &operands[1]),
                });
        }
        "guts" if operands.len() == 2 => {
            cx.img.eduke_program.instructions.push(EdInstruction::Guts {
                tile: eduke_operand(cx.img, &operands[0]),
                count: eduke_operand(cx.img, &operands[1]),
            });
        }
        "hitradius" if operands.len() == 5 => {
            let radius = eduke_operand(cx.img, &operands[0]);
            let damage = std::array::from_fn(|index| eduke_operand(cx.img, &operands[index + 1]));
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::HitRadius { radius, damage });
        }
        "pkick" if operands.is_empty() => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::PlayerQuickKick),
        "tossweapon" if operands.is_empty() => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::TossWeapon),
        "abs" if !operands.is_empty() => {
            cx.img.eduke_program.instructions.push(EdInstruction::Abs {
                target: eduke_lvalue(cx.img, &operands[0]),
            })
        }
        "clamp" if operands.len() >= 3 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Clamp {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    minimum: eduke_operand(cx.img, &operands[1]),
                    maximum: eduke_operand(cx.img, &operands[2]),
                })
        }
        "calchypotenuse" if operands.len() >= 3 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::CalculateHypotenuse {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    x: eduke_operand(cx.img, &operands[1]),
                    y: eduke_operand(cx.img, &operands[2]),
                })
        }
        "getflorzofslope" | "getceilzofslope" if operands.len() >= 4 => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::GetSlopeZ {
                target: eduke_lvalue(cx.img, &operands[3]),
                sector: eduke_operand(cx.img, &operands[0]),
                x: eduke_operand(cx.img, &operands[1]),
                y: eduke_operand(cx.img, &operands[2]),
                ceiling: command == "getceilzofslope",
            }),
        "sectorofwall" if operands.len() >= 2 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SectorOfWall {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    wall: eduke_operand(cx.img, &operands[1]),
                });
        }
        "getticks" if operands.len() == 1 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::GetTicks {
                    target: eduke_lvalue(cx.img, &operands[0]),
                })
        }
        "setgamepalette" if operands.len() == 1 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SetGamePalette {
                    palette: eduke_operand(cx.img, &operands[0]),
                })
        }
        "inittimer" if operands.len() == 1 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SetTimerRate {
                    rate: eduke_operand(cx.img, &operands[0]),
                })
        }
        "endofgame" if operands.len() <= 1 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::EndOfGame {
                    delay: operands
                        .first()
                        .map_or(EdOperand::Immediate(52), |operand| {
                            eduke_operand(cx.img, operand)
                        }),
                })
        }
        "lotsofglass" if operands.len() == 1 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::LotsOfGlass {
                    count: eduke_operand(cx.img, &operands[0]),
                });
        }
        "spawnceilingglass" if operands.len() == 2 || operands.len() == 3 => {
            let (sprite, sector, count) = if operands.len() == 3 {
                (
                    eduke_operand(cx.img, &operands[0]),
                    eduke_operand(cx.img, &operands[1]),
                    eduke_operand(cx.img, &operands[2]),
                )
            } else {
                (
                    EdOperand::CurrentActor,
                    eduke_operand(cx.img, &operands[0]),
                    eduke_operand(cx.img, &operands[1]),
                )
            };
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SpawnCeilingGlass {
                    sprite,
                    sector,
                    count,
                });
        }
        "money" | "mail" | "paper" if operands.len() == 1 => {
            let kind = match command.as_str() {
                "money" => EdItemSprayKind::Money,
                "mail" => EdItemSprayKind::Mail,
                _ => EdItemSprayKind::Paper,
            };
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::ItemSpray {
                    kind,
                    count: eduke_operand(cx.img, &operands[0]),
                });
        }
        "operate" if operands.is_empty() => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::Operate),
        "operatemasterswitches" if operands.len() == 1 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::OperateMasterSwitches {
                    lotag: eduke_operand(cx.img, &operands[0]),
                })
        }
        "respawnhitag" if operands.is_empty() => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::RespawnHitag),
        "resetplayer" if operands.is_empty() => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::ResetPlayer),
        "cansee" if operands.len() >= 9 => {
            let target = eduke_lvalue(cx.img, &operands[8]);
            let start = std::array::from_fn(|index| eduke_operand(cx.img, &operands[index]));
            let end = std::array::from_fn(|index| eduke_operand(cx.img, &operands[index + 4]));
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::CanSee { target, start, end });
        }
        "hitscan" if operands.len() >= 14 => {
            let ray = std::array::from_fn(|index| eduke_operand(cx.img, &operands[index]));
            let outputs = std::array::from_fn(|index| eduke_lvalue(cx.img, &operands[index + 7]));
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::HitScan {
                    ray,
                    outputs,
                    clipmask: eduke_operand(cx.img, &operands[13]),
                });
        }
        "getzrange" if operands.len() >= 10 => {
            let point = std::array::from_fn(|index| eduke_operand(cx.img, &operands[index]));
            let outputs = std::array::from_fn(|index| eduke_lvalue(cx.img, &operands[index + 4]));
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::GetZRange {
                    point,
                    outputs,
                    wall_distance: eduke_operand(cx.img, &operands[8]),
                    clipmask: eduke_operand(cx.img, &operands[9]),
                });
        }
        "sqrt" if operands.len() >= 2 => {
            cx.img.eduke_program.instructions.push(EdInstruction::Sqrt {
                input: eduke_operand(cx.img, &operands[0]),
                target: eduke_lvalue(cx.img, &operands[1]),
            });
        }
        "updatesector" if operands.len() >= 3 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::UpdateSector {
                    x: eduke_operand(cx.img, &operands[0]),
                    y: eduke_operand(cx.img, &operands[1]),
                    target: eduke_lvalue(cx.img, &operands[2]),
                    z: None,
                });
        }
        "updatesectorz" if operands.len() >= 4 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::UpdateSector {
                    x: eduke_operand(cx.img, &operands[0]),
                    y: eduke_operand(cx.img, &operands[1]),
                    z: Some(eduke_operand(cx.img, &operands[2])),
                    target: eduke_lvalue(cx.img, &operands[3]),
                });
        }
        "ldist" | "dist" if operands.len() >= 3 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SpriteDistance {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    first: eduke_operand(cx.img, &operands[1]),
                    second: eduke_operand(cx.img, &operands[2]),
                    three_dimensional: command == "dist",
                });
        }
        "findnearactor3d" if operands.len() >= 3 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::FindNearActor3D {
                    tile: eduke_operand(cx.img, &operands[0]),
                    distance: eduke_operand(cx.img, &operands[1]),
                    target: eduke_lvalue(cx.img, &operands[2]),
                });
        }
        "lineintersect" if operands.len() == 14 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::LineIntersect {
                    origin: [
                        eduke_operand(cx.img, &operands[0]),
                        eduke_operand(cx.img, &operands[1]),
                        eduke_operand(cx.img, &operands[2]),
                    ],
                    destination: [
                        eduke_operand(cx.img, &operands[3]),
                        eduke_operand(cx.img, &operands[4]),
                        eduke_operand(cx.img, &operands[5]),
                    ],
                    line: [
                        eduke_operand(cx.img, &operands[6]),
                        eduke_operand(cx.img, &operands[7]),
                        eduke_operand(cx.img, &operands[8]),
                        eduke_operand(cx.img, &operands[9]),
                    ],
                    outputs: [
                        eduke_lvalue(cx.img, &operands[10]),
                        eduke_lvalue(cx.img, &operands[11]),
                        eduke_lvalue(cx.img, &operands[12]),
                        eduke_lvalue(cx.img, &operands[13]),
                    ],
                });
        }
        "headspritestat" | "nextspritestat" | "headspritesect" | "nextspritesect"
            if operands.len() >= 2 =>
        {
            let kind = match command.as_str() {
                "headspritestat" => EdSpriteListKind::HeadStatus,
                "nextspritestat" => EdSpriteListKind::NextStatus,
                "headspritesect" => EdSpriteListKind::HeadSector,
                _ => EdSpriteListKind::NextSector,
            };
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SpriteListRead {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    kind,
                    index: eduke_operand(cx.img, &operands[1]),
                });
        }
        "changespritestat" | "changespritesect" if operands.len() >= 2 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SpriteListChange {
                    sprite: eduke_operand(cx.img, &operands[0]),
                    index: eduke_operand(cx.img, &operands[1]),
                    status: command == "changespritestat",
                });
        }
        "setsprite" if operands.len() >= 4 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SetSpritePosition {
                    sprite: eduke_operand(cx.img, &operands[0]),
                    x: eduke_operand(cx.img, &operands[1]),
                    y: eduke_operand(cx.img, &operands[2]),
                    z: eduke_operand(cx.img, &operands[3]),
                });
        }
        "insertspriteq" if operands.is_empty() => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::InsertSpriteQueue {
                    sprite: EdOperand::CurrentActor,
                })
        }
        "spawn" | "spawnvar" | "espawn" | "espawnvar" | "qspawn" | "qspawnvar" | "eqspawn"
        | "eqspawnvar"
            if !operands.is_empty() =>
        {
            let variable_form = command.ends_with("var");
            let tile = if variable_form {
                EdOperand::Variable(operands[0].clone().into())
            } else {
                eduke_operand(cx.img, &operands[0])
            };
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SpawnActor {
                    parent: EdOperand::CurrentActor,
                    tile,
                    return_sprite: command.starts_with('e'),
                    queue_sprite: command.starts_with('q') || command.starts_with("eq"),
                });
        }
        "shoot" | "shootvar" | "eshoot" | "eshootvar" | "zshoot" | "zshootvar" | "ezshoot"
        | "ezshootvar" => {
            let has_z_velocity = command.starts_with('z') || command.starts_with("ezshoot");
            let expected = if has_z_velocity { 2 } else { 1 };
            if operands.len() != expected {
                eduke_bad_arity(cx);
                return;
            }
            let tile_index = usize::from(has_z_velocity);
            let vertical_velocity = has_z_velocity.then(|| eduke_operand(cx.img, &operands[0]));
            let tile = eduke_operand(cx.img, &operands[tile_index]);
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::ShootActor {
                    parent: EdOperand::CurrentActor,
                    tile,
                    vertical_velocity,
                    return_sprite: command.starts_with('e'),
                });
        }
        "setarray" if operands.len() >= 2 => {
            let (target, value) = if operands[0].contains('[') {
                (eduke_lvalue(cx.img, &operands[0]), &operands[1])
            } else if operands.len() >= 3 {
                (
                    EdLValue::ArrayElement {
                        name: operands[0].clone(),
                        index: Box::new(eduke_operand(cx.img, &operands[1])),
                    },
                    &operands[2],
                )
            } else {
                eduke_bad_arity(cx);
                return;
            };
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Assign {
                    target,
                    value: eduke_operand(cx.img, value),
                })
        }
        "resizearray" if operands.len() == 2 => {
            let length = eduke_operand(cx.img, &operands[1]);
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::ResizeArray {
                    name: operands[0].clone(),
                    length,
                });
        }
        "getarray" if operands.len() >= 3 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Assign {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    value: EdOperand::ArrayElement {
                        name: operands[1].clone(),
                        index: Box::new(eduke_operand(cx.img, &operands[2])),
                    },
                })
        }
        "mulscale" | "divscale" if operands.len() >= 4 => {
            let Some(shift) =
                eduke_constant(cx.img, &operands[3]).and_then(|value| u32::try_from(value).ok())
            else {
                eduke_bad_arity(cx);
                return;
            };
            let target = eduke_lvalue(cx.img, &operands[0]);
            let left = eduke_operand(cx.img, &operands[1]);
            let right = eduke_operand(cx.img, &operands[2]);
            cx.img
                .eduke_program
                .instructions
                .push(if command == "mulscale" {
                    EdInstruction::MulScale {
                        target,
                        left,
                        right,
                        shift,
                    }
                } else {
                    EdInstruction::DivScale {
                        target,
                        left,
                        right,
                        shift,
                    }
                });
        }
        "scalevar" if operands.len() >= 4 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::ScaleVar {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    value: eduke_operand(cx.img, &operands[1]),
                    numerator: eduke_operand(cx.img, &operands[2]),
                    denominator: eduke_operand(cx.img, &operands[3]),
                })
        }
        "sin" | "cos" if operands.len() >= 2 => {
            let target = eduke_lvalue(cx.img, &operands[0]);
            let angle = eduke_operand(cx.img, &operands[1]);
            cx.img.eduke_program.instructions.push(if command == "sin" {
                EdInstruction::BuildSin { target, angle }
            } else {
                EdInstruction::BuildCos { target, angle }
            });
        }
        "getangle" if operands.len() >= 3 => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::BuildGetAngle {
                    target: eduke_lvalue(cx.img, &operands[0]),
                    x: eduke_operand(cx.img, &operands[1]),
                    y: eduke_operand(cx.img, &operands[2]),
                })
        }
        "break" if operands.is_empty() => {
            if let Some(breaks) = cx.eduke_break_stack.last_mut() {
                let jump = cx.img.eduke_program.instructions.len();
                cx.img
                    .eduke_program
                    .instructions
                    .push(EdInstruction::Jump { target: 0 });
                breaks.push(jump);
            } else {
                emit_eduke_body_break(cx);
            }
        }
        "return" if operands.is_empty() => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::Return),
        "enda" if operands.is_empty() => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Return);
            cx.parsing_actor = None;
        }
        "ends" if operands.is_empty() => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::Return);
            finish_eduke_state_fragment(cx);
            cx.parsing_state = false;
        }
        "state" if !operands.is_empty() => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::CallState {
                    name: operands[0].clone().into(),
                })
        }
        "exit" if operands.is_empty() => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::Return),
        "echo" if operands.len() == 1 => {
            let quote = eduke_constant(cx.img, &operands[0])
                .and_then(|value| usize::try_from(value).ok())
                .filter(|quote| *quote < EDUKE_MAX_QUOTES);
            if let Some(quote) = quote {
                cx.img
                    .eduke_program
                    .instructions
                    .push(EdInstruction::EchoQuote { quote });
            } else {
                eduke_bad_arity(cx);
            }
        }
        "quote" if operands.len() == 1 => {
            let quote = eduke_constant(cx.img, &operands[0])
                .and_then(|value| usize::try_from(value).ok())
                .filter(|quote| *quote < EDUKE_MAX_QUOTES);
            if let Some(quote) = quote {
                cx.img
                    .eduke_program
                    .instructions
                    .push(EdInstruction::DisplayQuote { quote });
            } else {
                eduke_bad_arity(cx);
            }
        }
        "palfrom" if operands.len() <= 4 => {
            let values = std::array::from_fn(|index| {
                operands
                    .get(index)
                    .map_or(EdOperand::Immediate(0), |operand| {
                        eduke_operand(cx.img, operand)
                    })
            });
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::PaletteFrom { values });
        }
        "prependstate" if !operands.is_empty() => {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::CallState {
                    name: operands[0].clone().into(),
                })
        }
        "nullop" if operands.is_empty() => cx.emit_eduke_noop(),
        _ if eduke_builtin_if_command(&command) => {
            let Some((condition_operands, left, comparison, right)) =
                eduke_builtin_condition(cx.img, &command, operands)
            else {
                eduke_bad_arity(cx);
                return;
            };
            if operands.len() < condition_operands {
                eduke_bad_arity(cx);
                return;
            }
            compile_eduke_inline_condition(
                cx,
                operands,
                condition_operands,
                left,
                comparison,
                right,
            );
        }
        _ if keyword_index(&command).is_some()
            || matches!(
                command.as_str(),
                "sectorofwall"
                    | "setactorsoundpitch"
                    | "getzrange"
                    | "freezerhurtowner"
                    | "definegamefuncname"
                    | "exit"
                    | "ifactorsound"
                    | "prependstate"
            ) =>
        {
            cx.emit_eduke_noop()
        }
        _ if eduke_comparison(&command).is_some() && operands.len() >= 2 => {
            compile_eduke_inline_if(cx, &command, operands)
        }
        _ => eduke_bad_arity(cx),
    }
}

fn compile_eduke_inline_if(cx: &mut Cx<'_>, command: &str, operands: &[String]) {
    let Some(comparison) = eduke_comparison(command) else {
        eduke_bad_arity(cx);
        return;
    };
    if operands.len() < 2 {
        eduke_bad_arity(cx);
        return;
    }
    let (left, right, body_start) = eduke_condition(cx.img, operands);
    compile_eduke_inline_condition(cx, operands, body_start, left, comparison, right);
}

fn compile_eduke_ifrnd(cx: &mut Cx<'_>, operands: &[String]) {
    let Some(threshold) = operands.first() else {
        eduke_bad_arity(cx);
        return;
    };
    let instruction = cx.img.eduke_program.instructions.len();
    cx.img
        .eduke_program
        .instructions
        .push(EdInstruction::IfRandom {
            threshold: eduke_operand(cx.img, threshold),
            target: 0,
        });
    if let Some(else_index) = operands
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, field)| field.eq_ignore_ascii_case("else").then_some(index))
    {
        compile_eduke_field_sequence(cx, &operands[1..else_index]);
        let skip_else = cx.img.eduke_program.instructions.len();
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::Jump { target: 0 });
        let else_start = cx.img.eduke_program.instructions.len();
        if let Some(EdInstruction::IfRandom { target, .. }) =
            cx.img.eduke_program.instructions.get_mut(instruction)
        {
            *target = else_start;
        }
        compile_eduke_field_sequence(cx, &operands[else_index + 1..]);
        let end = cx.img.eduke_program.instructions.len();
        patch_eduke_jump(cx, skip_else, end);
    } else {
        compile_eduke_field_sequence(cx, &operands[1..]);
        let end = cx.img.eduke_program.instructions.len();
        if let Some(EdInstruction::IfRandom { target, .. }) =
            cx.img.eduke_program.instructions.get_mut(instruction)
        {
            *target = end;
        }
    }
}

fn eduke_struct_command(command: &str) -> Option<(EdStructKind, bool)> {
    Some(match command {
        "geta" | "getactor" => (EdStructKind::Actor, false),
        "seta" | "setactor" => (EdStructKind::Actor, true),
        "gets" | "getsector" => (EdStructKind::Sector, false),
        "sets" | "setsector" => (EdStructKind::Sector, true),
        "getw" | "getwall" => (EdStructKind::Wall, false),
        "setw" | "setwall" => (EdStructKind::Wall, true),
        "getp" | "getplayer" => (EdStructKind::Player, false),
        "setp" | "setplayer" => (EdStructKind::Player, true),
        "geti" | "getinput" => (EdStructKind::Input, false),
        "seti" | "setinput" => (EdStructKind::Input, true),
        "getu" | "getuserdef" => (EdStructKind::UserDef, false),
        "setu" | "setuserdef" => (EdStructKind::UserDef, true),
        "gettiledata" => (EdStructKind::TileData, false),
        "settiledata" => (EdStructKind::TileData, true),
        "gettspr" => (EdStructKind::Tspr, false),
        "settspr" => (EdStructKind::Tspr, true),
        "getprojectile" => (EdStructKind::Projectile, false),
        "setprojectile" => (EdStructKind::Projectile, true),
        "getthisprojectile" => (EdStructKind::ThisProjectile, false),
        "setthisprojectile" => (EdStructKind::ThisProjectile, true),
        _ => return None,
    })
}

/// Split EDuke32's compact member form (`seta[THISACTOR].x`) used inside
/// inline command bodies, where fields are separated by whitespace.
fn split_eduke_struct_command(command: &str) -> Option<(&'static str, &str)> {
    const COMMANDS: &[&str] = &[
        "getthisprojectile",
        "setthisprojectile",
        "getprojectile",
        "setprojectile",
        "gettiledata",
        "settiledata",
        "getuserdef",
        "setuserdef",
        "getactor",
        "setactor",
        "getsector",
        "setsector",
        "getplayer",
        "setplayer",
        "getinput",
        "setinput",
        "gettspr",
        "settspr",
        "geta",
        "seta",
        "gets",
        "sets",
        "getw",
        "setw",
        "getp",
        "setp",
        "geti",
        "seti",
        "getu",
        "setu",
    ];
    COMMANDS.iter().find_map(|base| {
        command
            .strip_prefix(base)
            .filter(|suffix| suffix.starts_with('[') || suffix.starts_with('.'))
            .map(|suffix| (*base, suffix))
    })
}

fn split_eduke_scoped_var_command(command: &str) -> Option<(&'static str, &str)> {
    const COMMANDS: &[&str] = &["getactorvar", "setactorvar", "getplayervar", "setplayervar"];
    COMMANDS.iter().find_map(|base| {
        command
            .strip_prefix(base)
            .filter(|suffix| suffix.starts_with('[') || suffix.starts_with('.'))
            .map(|suffix| (*base, suffix))
    })
}

fn eduke_scoped_var_command(command: &str) -> Option<(EdVariableScope, bool)> {
    Some(match command {
        "getactorvar" | "getav" => (EdVariableScope::Actor, false),
        "setactorvar" | "setav" => (EdVariableScope::Actor, true),
        "getplayervar" | "getpv" => (EdVariableScope::Player, false),
        "setplayervar" | "setpv" => (EdVariableScope::Player, true),
        _ => return None,
    })
}

fn parse_eduke_member_path(
    img: &ScriptImage,
    raw: &str,
    kind: EdStructKind,
) -> Option<(EdOperand, String)> {
    let (id, member) = if let Some(bracketed) = raw.strip_prefix('[') {
        let (id, member) = bracketed.split_once("].")?;
        (id, member)
    } else {
        raw.split_once('.')?
    };
    let member = member.trim();
    if member.is_empty() {
        return None;
    }
    let id = if id.is_empty() {
        default_struct_id(kind)
    } else if id.eq_ignore_ascii_case("THISACTOR")
        && matches!(kind, EdStructKind::Player | EdStructKind::Input)
    {
        EdOperand::CurrentPlayer
    } else {
        eduke_operand(img, id)
    };
    Some((id, member.to_owned()))
}

fn default_struct_id(kind: EdStructKind) -> EdOperand {
    match kind {
        EdStructKind::Player | EdStructKind::Input => EdOperand::CurrentPlayer,
        EdStructKind::UserDef => EdOperand::Immediate(0),
        _ => EdOperand::CurrentActor,
    }
}

fn compile_eduke_struct_fields(
    cx: &mut Cx<'_>,
    kind: EdStructKind,
    is_set: bool,
    fields: &[String],
) {
    let Some(first) = fields.first() else {
        eduke_bad_arity(cx);
        return;
    };
    let Some((id, member)) = parse_eduke_member_path(cx.img, first, kind) else {
        eduke_bad_arity(cx);
        return;
    };
    let indexed = eduke_struct_member_is_indexed(kind, &member);
    let command_fields = if indexed { 3 } else { 2 };
    if fields.len() < command_fields {
        eduke_bad_arity(cx);
        return;
    }
    if is_set {
        if indexed {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SetStructMember {
                    kind,
                    id,
                    member,
                    member_index: Some(eduke_operand(cx.img, &fields[1])),
                    value: eduke_operand(cx.img, &fields[2]),
                });
        } else {
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SetStructMember {
                    kind,
                    id,
                    member,
                    member_index: None,
                    value: eduke_operand(cx.img, &fields[1]),
                });
        }
    } else {
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::GetStructMember {
                kind,
                id,
                member,
                member_index: indexed.then(|| eduke_operand(cx.img, &fields[1])),
                target: eduke_lvalue(cx.img, &fields[command_fields - 1]),
            });
    }
    if fields.len() > command_fields {
        compile_eduke_field_sequence(cx, &fields[command_fields..]);
    }
}

fn eduke_struct_member_is_indexed(kind: EdStructKind, member: &str) -> bool {
    match kind {
        EdStructKind::Player => matches!(
            member.to_ascii_lowercase().as_str(),
            "ammo_amount" | "max_ammo_amount" | "gotweapon" | "inventory_amount" | "pals"
        ),
        EdStructKind::Actor => {
            matches!(member.to_ascii_lowercase().as_str(), "htg_t" | "temp_data")
        }
        EdStructKind::UserDef => member.eq_ignore_ascii_case("return"),
        _ => false,
    }
}

fn compile_eduke_scoped_var_fields(
    cx: &mut Cx<'_>,
    scope: EdVariableScope,
    is_set: bool,
    fields: &[String],
) {
    let kind = match scope {
        EdVariableScope::Actor => EdStructKind::Actor,
        EdVariableScope::Player => EdStructKind::Player,
        EdVariableScope::Global => {
            eduke_bad_arity(cx);
            return;
        }
    };
    let Some((id, name)) = fields
        .first()
        .and_then(|path| parse_eduke_member_path(cx.img, path, kind))
    else {
        eduke_bad_arity(cx);
        return;
    };
    if fields.len() < 2 {
        eduke_bad_arity(cx);
        return;
    }
    if is_set {
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::SetScopedVariable {
                scope,
                name,
                id,
                value: eduke_operand(cx.img, &fields[1]),
            });
    } else {
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::GetScopedVariable {
                scope,
                name,
                id,
                target: eduke_lvalue(cx.img, &fields[1]),
            });
    }
    if fields.len() > 2 {
        compile_eduke_field_sequence(cx, &fields[2..]);
    }
}

fn eduke_bad_arity(cx: &mut Cx<'_>) {
    cx.record_error();
    cx.emit_eduke_noop();
}

fn eduke_operand(img: &ScriptImage, token: &str) -> EdOperand {
    if token.eq_ignore_ascii_case("THISACTOR") {
        return EdOperand::CurrentActor;
    }
    if token.eq_ignore_ascii_case("THISPLAYER") {
        return EdOperand::CurrentPlayer;
    }
    if let Some((kind, id, member)) = eduke_struct_operand(img, token) {
        return EdOperand::StructMember {
            kind,
            id: Box::new(id),
            member,
            member_index: None,
        };
    }
    if let Some(value) = eduke_constant(img, token) {
        return EdOperand::Immediate(value);
    }
    // EDuke32 `actorvar[id].name` / `playervar[id].name` operands.
    let lower = token.to_ascii_lowercase();
    for (prefix, scope, kind) in [
        ("actorvar", EdVariableScope::Actor, EdStructKind::Actor),
        ("playervar", EdVariableScope::Player, EdStructKind::Player),
    ] {
        if lower.starts_with(prefix)
            && token[prefix.len()..].starts_with('[')
            && let Some((id, name)) = parse_eduke_member_path(img, &token[prefix.len()..], kind)
        {
            return EdOperand::ScopedVariable {
                scope,
                name,
                id: Box::new(id),
            };
        }
    }
    if let Some((name, index)) = token.split_once('[').and_then(|(name, rest)| {
        rest.strip_suffix(']')
            .map(|index| (name.to_owned(), index.to_owned()))
    }) {
        return EdOperand::ArrayElement {
            name,
            index: Box::new(eduke_operand(img, &index)),
        };
    }
    EdOperand::Variable(token.into())
}

fn eduke_pointer_value(img: &ScriptImage, token: &str) -> Option<i32> {
    find_label(img, token)
        .filter(|label| label.is_ptr)
        .map(|label| label.value)
        .or_else(|| eduke_constant(img, token))
}

fn compile_eduke_actor_script_fields(cx: &mut Cx<'_>, command: &str, operands: &[String]) {
    let Some(pointer) = operands
        .first()
        .and_then(|operand| eduke_pointer_value(cx.img, operand))
    else {
        eduke_bad_arity(cx);
        return;
    };
    let pointer = EdOperand::Immediate(pointer);
    match command {
        "action" if operands.len() == 1 => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::SetActorAction { action: pointer }),
        "move" => {
            let mut flags = 0;
            for operand in &operands[1..] {
                let Some(value) = eduke_constant(cx.img, operand) else {
                    eduke_bad_arity(cx);
                    return;
                };
                flags |= value;
            }
            cx.img
                .eduke_program
                .instructions
                .push(EdInstruction::SetActorMove {
                    movement: pointer,
                    flags: EdOperand::Immediate(flags),
                });
        }
        "ai" if operands.len() == 1 => cx
            .img
            .eduke_program
            .instructions
            .push(EdInstruction::SetActorAi { ai: pointer }),
        _ => eduke_bad_arity(cx),
    }
}

fn parse_eduke_actor_script_command(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, command: &str) {
    let Some(pointer) = lx.next_token() else {
        eduke_bad_arity(cx);
        return;
    };
    let mut operands = vec![pointer];
    while peek_keyword(lx, ConDialect::EDuke32).is_none() {
        let Some(flag) = lx.next_token() else {
            break;
        };
        operands.push(flag);
    }
    compile_eduke_actor_script_fields(cx, command, &operands);
}

fn compile_eduke_actor_command_fields(cx: &mut Cx<'_>, command: &str, operands: &[String]) {
    let instruction = match command {
        "strength" | "addstrength" | "cstat" | "cstator" | "cactor" | "spritepal" | "clipdist"
        | "count" | "sleeptime"
            if operands.len() == 1 =>
        {
            let member = match command {
                "strength" => EdActorMember::Strength,
                "addstrength" => EdActorMember::AddStrength,
                "cstat" => EdActorMember::Cstat,
                "cstator" => EdActorMember::CstatOr,
                "cactor" => EdActorMember::Tile,
                "spritepal" => EdActorMember::Palette,
                "clipdist" => EdActorMember::ClipDistance,
                "count" => EdActorMember::Count,
                _ => EdActorMember::SleepTime,
            };
            EdInstruction::SetActorMember {
                member,
                value: eduke_operand(cx.img, &operands[0]),
            }
        }
        "addkills" if operands.len() == 1 => EdInstruction::AddActorKills {
            amount: eduke_operand(cx.img, &operands[0]),
        },
        "sizeto" | "sizeat" if operands.len() == 2 => {
            let x = eduke_operand(cx.img, &operands[0]);
            let y = eduke_operand(cx.img, &operands[1]);
            if command == "sizeto" {
                EdInstruction::SetActorSizeTo { x, y }
            } else {
                EdInstruction::SetActorSize { x, y }
            }
        }
        "fall" if operands.is_empty() => EdInstruction::FallActor,
        "addphealth" if operands.len() == 1 => EdInstruction::AddPlayerHealth {
            amount: eduke_operand(cx.img, &operands[0]),
        },
        "getlastpal" if operands.is_empty() => EdInstruction::RestoreActorPalette,
        "resetactioncount" if operands.is_empty() => {
            EdInstruction::ResetActorCounter { action: true }
        }
        "resetcount" if operands.is_empty() => EdInstruction::ResetActorCounter { action: false },
        _ => {
            eduke_bad_arity(cx);
            return;
        }
    };
    cx.img.eduke_program.instructions.push(instruction);
}

fn eduke_struct_operand(
    img: &ScriptImage,
    token: &str,
) -> Option<(EdStructKind, EdOperand, String)> {
    const OBJECTS: &[(&str, EdStructKind)] = &[
        ("thisprojectile", EdStructKind::ThisProjectile),
        ("projectile", EdStructKind::Projectile),
        ("userdef", EdStructKind::UserDef),
        ("tiledata", EdStructKind::TileData),
        ("sprite", EdStructKind::Actor),
        ("actor", EdStructKind::Actor),
        ("sector", EdStructKind::Sector),
        ("wall", EdStructKind::Wall),
        ("player", EdStructKind::Player),
        ("input", EdStructKind::Input),
        ("tspr", EdStructKind::Tspr),
    ];
    OBJECTS.iter().find_map(|(prefix, kind)| {
        let suffix = token.strip_prefix(prefix)?;
        let (id, member) = if let Some(bracketed) = suffix.strip_prefix('[') {
            let (id, member) = bracketed.split_once("].")?;
            (
                if id.eq_ignore_ascii_case("THISACTOR")
                    && matches!(*kind, EdStructKind::Player | EdStructKind::Input)
                {
                    EdOperand::CurrentPlayer
                } else {
                    eduke_operand(img, id)
                },
                member,
            )
        } else if let Some(member) = suffix.strip_prefix('.') {
            (default_struct_id(*kind), member)
        } else {
            return None;
        };
        (!member.is_empty()).then(|| (*kind, id, member.to_owned()))
    })
}

fn eduke_condition(img: &ScriptImage, operands: &[String]) -> (EdOperand, EdOperand, usize) {
    let Some(first) = operands.first() else {
        return (EdOperand::Immediate(0), EdOperand::Immediate(0), 2);
    };
    let Some(second) = operands.get(1) else {
        return (eduke_operand(img, first), EdOperand::Immediate(0), 2);
    };
    if let (Some((kind, id, member)), Some(right)) =
        (eduke_struct_operand(img, first), operands.get(2))
    {
        if !eduke_inline_command_token(right) {
            return (
                EdOperand::StructMember {
                    kind,
                    id: Box::new(id),
                    member,
                    member_index: Some(Box::new(eduke_operand(img, second))),
                },
                eduke_operand(img, right),
                3,
            );
        }
    }
    (eduke_operand(img, first), eduke_operand(img, second), 2)
}

fn eduke_inline_command_token(token: &str) -> bool {
    let command = eduke_legacy_alias(&token.to_ascii_lowercase());
    eduke_comparison(&command).is_some()
        || eduke_while_comparison(&command).is_some()
        || eduke_struct_command(&command).is_some()
        || eduke_scoped_var_command(&command).is_some()
        || split_eduke_struct_command(&command).is_some()
        || split_eduke_scoped_var_command(&command).is_some()
        || keyword_index(&command).is_some()
        || matches!(
            command.as_str(),
            "set"
                | "add"
                | "sub"
                | "mul"
                | "div"
                | "mod"
                | "and"
                | "or"
                | "xor"
                | "shiftl"
                | "shiftr"
                | "setarray"
                | "getarray"
                | "state"
                | "sqrt"
                | "break"
                | "killit"
                | "debris"
                | "guts"
                | "lotsofglass"
                | "addphealth"
                | "getzrange"
                | "sound"
                | "soundonce"
                | "stopsound"
                | "globalsound"
                | "globalsoundvar"
                | "return"
                | "exit"
                | "nullop"
                | "rand"
                | "randvar"
                | "randvarvar"
                | "displayrand"
                | "guniqhudid"
                | "rotatesprite"
                | "minitext"
                | "gametext"
                | "gametextz"
                | "screentext"
                | "digitalnumber"
                | "digitalnumberz"
                | "soundvar"
                | "spawn"
                | "espawn"
                | "spawnvar"
                | "espawnvar"
                | "qspawn"
                | "qspawnvar"
                | "eqspawn"
                | "eqspawnvar"
                | "insertspriteq"
                | "changespritestat"
                | "changespritesect"
                | "setactorvar"
                | "getactorvar"
                | "setplayervar"
                | "getplayervar"
                | "addammo"
                | "addammovar"
                | "addweapon"
                | "addweaponvar"
                | "addinventory"
                | "addinventoryvar"
        )
}

fn eduke_statement_head(token: &str) -> bool {
    let command = eduke_legacy_alias(&token.to_ascii_lowercase());
    keyword_index(&command).is_some()
        || eduke_comparison(&command).is_some()
        || eduke_while_comparison(&command).is_some()
        || eduke_struct_command(&command).is_some()
        || eduke_scoped_var_command(&command).is_some()
        || split_eduke_struct_command(&command).is_some()
        || split_eduke_scoped_var_command(&command).is_some()
        || matches!(
            command.as_str(),
            "var"
                | "gamevar"
                | "array"
                | "gamearray"
                | "defstate"
                | "appendstate"
                | "prependstate"
                | "appendevent"
                | "onevent"
                | "endevent"
                | "endswitch"
                | "defineprojectile"
                | "definegamefuncname"
                | "damageeventtile"
                | "spritenoshade"
                | "spritenopal"
                | "spriteflags"
                | "definequote"
                | "redefinequote"
                | "switch"
                | "case"
                | "default"
                | "for"
                | "while"
                | "state"
                | "return"
                | "break"
                | "set"
                | "add"
                | "sub"
                | "mul"
                | "div"
                | "mod"
                | "and"
                | "or"
                | "xor"
                | "shiftl"
                | "shiftr"
                | "setarray"
                | "getarray"
                | "mulscale"
                | "divscale"
                | "scalevar"
                | "sin"
                | "cos"
                | "getangle"
                | "sqrt"
                | "abs"
                | "clamp"
                | "calchypotenuse"
                | "getflorzofslope"
                | "getceilzofslope"
                | "cansee"
                | "hitscan"
                | "getzrange"
                | "debris"
                | "guts"
                | "lotsofglass"
                | "updatesector"
                | "updatesectorz"
                | "ldist"
                | "dist"
                | "headspritestat"
                | "nextspritestat"
                | "headspritesect"
                | "nextspritesect"
                | "changespritestat"
                | "changespritesect"
                | "setsprite"
                | "spawn"
                | "spawnvar"
                | "espawn"
                | "espawnvar"
                | "qspawn"
                | "qspawnvar"
                | "eqspawn"
                | "eqspawnvar"
                | "insertspriteq"
                | "shoot"
                | "shootvar"
                | "eshoot"
                | "eshootvar"
                | "zshoot"
                | "zshootvar"
                | "ezshoot"
                | "ezshootvar"
                | "qsprintf"
                | "rand"
                | "randvar"
                | "displayrand"
                | "displayrandvar"
                | "randvarvar"
                | "displayrandvarvar"
                | "findnearactor3d"
                | "sectorofwall"
                | "inittimer"
                | "getticks"
                | "setgamepalette"
                | "actorsound"
                | "stopactorsound"
                | "guniqhudid"
                | "lineintersect"
                | "resizearray"
                | "setactorsoundpitch"
                | "rotatesprite"
                | "minitext"
                | "gametext"
                | "gametextz"
                | "screentext"
                | "digitalnumber"
                | "digitalnumberz"
                | "freezerhurtowner"
                | "echo"
                | "exit"
                | "ifactorsound"
                | "operatemasterswitches"
                | "soundvar"
                | "nullop"
                | "quote"
                | "playsound"
                | "ifpinventory"
                | "addphealth"
                | "addammo"
                | "addammovar"
                | "addweapon"
                | "addweaponvar"
                | "addinventory"
                | "addinventoryvar"
        )
}

fn eduke_lvalue(img: &ScriptImage, token: &str) -> EdLValue {
    if let Some((name, index)) = token.split_once('[').and_then(|(name, rest)| {
        rest.strip_suffix(']')
            .map(|index| (name.to_owned(), index.to_owned()))
    }) {
        EdLValue::ArrayElement {
            name,
            index: Box::new(eduke_operand(img, &index)),
        }
    } else {
        EdLValue::Variable(token.into())
    }
}

fn eduke_constant(img: &ScriptImage, token: &str) -> Option<i32> {
    token
        .parse::<i32>()
        .ok()
        .or_else(|| token.parse::<u32>().ok().map(|value| value as i32))
        .or_else(|| {
            find_label(img, token)
                .filter(|label| !label.is_ptr)
                .map(|label| label.value)
        })
        .or_else(|| match token.to_ascii_uppercase().as_str() {
            "TRUE" => Some(1),
            "FALSE" => Some(0),
            "SFLAG_NOEVENTS" => Some(128),
            "FACEPLAYER" => Some(1),
            "GETH" => Some(2),
            "GETV" => Some(4),
            "RANDOMANGLE" => Some(8),
            "FACEPLAYERSLOW" => Some(16),
            "SPIN" => Some(32),
            "FACEPLAYERSMART" => Some(64),
            "FLEEENEMY" => Some(128),
            "JUMPTOPLAYER" => Some(257),
            "SEEKPLAYER" => Some(512),
            "FURTHESTDIR" => Some(1024),
            "DODGEBULLET" => Some(4096),
            "MAXSPRITES" => Some(room_build_engine::world::MAXSPRITES as i32),
            "MAXPLAYERS" => Some(super::eduke_vm::EDUKE_MAXPLAYERS as i32),
            "MAXSECTORS" => Some(room_build_engine::world::MAXSECTORS as i32),
            "MAXWALLS" => Some(room_build_engine::world::MAXWALLS as i32),
            "MAXTILES" => Some(EDUKE_MAX_TILES as i32),
            "NOTENEMY" => Some(0),
            "ENEMY" => Some(1),
            "ENEMYSTAYPUT" => Some(2),
            _ => None,
        })
}

/// EDuke32's legacy gamevar arithmetic spellings (`setvar`, `addvarvar`,
/// `shiftvarl`…) compile as their modern forms (`set`, `add`, `shiftl`…):
/// both take a target variable and a constant or variable operand.
fn eduke_legacy_alias(command: &str) -> String {
    let modern = match command {
        "setvar" | "setvarvar" => "set",
        "addvar" | "addvarvar" => "add",
        "subvar" | "subvarvar" => "sub",
        "mulvar" | "mulvarvar" => "mul",
        "divvar" | "divvarvar" => "div",
        "modvar" | "modvarvar" => "mod",
        "andvar" | "andvarvar" => "and",
        "orvar" | "orvarvar" => "or",
        "xorvar" | "xorvarvar" => "xor",
        "shiftvarl" | "shiftvarvarl" => "shiftl",
        "shiftvarr" | "shiftvarvarr" => "shiftr",
        _ => command,
    };
    modern.to_owned()
}

fn eduke_comparison(command: &str) -> Option<EdComparison> {
    Some(match command {
        "ife" | "ifvare" | "ifvarvare" => EdComparison::Equal,
        "ifn" | "ifvarn" | "ifvarvarn" => EdComparison::NotEqual,
        "ifl" | "ifvarl" | "ifvarvarl" => EdComparison::Less,
        "ifle" | "ifvarle" | "ifvarvarle" => EdComparison::LessEqual,
        "ifg" | "ifvarg" | "ifvarvarg" | "ifvarvarvarg" => EdComparison::Greater,
        "ifge" | "ifvarge" | "ifvarvarge" => EdComparison::GreaterEqual,
        "ifa" | "ifvara" | "ifvarvara" => EdComparison::UnsignedGreater,
        "ifae" | "ifvarae" | "ifvarvarae" => EdComparison::UnsignedGreaterEqual,
        "ifb" | "ifvarb" | "ifvarvarb" => EdComparison::UnsignedLess,
        "ifbe" | "ifvarbe" | "ifvarvarbe" => EdComparison::UnsignedLessEqual,
        "ifand" | "ifvarand" | "ifvarvarand" => EdComparison::BitAndNonZero,
        "ifor" | "ifvaror" | "ifvarvaror" | "ifvareither" | "ifvarvareither"
        | "ifvarvarvareither" => EdComparison::EitherNonZero,
        "ifxor" | "ifvarxor" | "ifvarvarxor" => EdComparison::BitXorNonZero,
        "ifboth" | "ifvarboth" | "ifvarvarboth" => EdComparison::BothNonZero,
        _ => return None,
    })
}

fn eduke_while_comparison(command: &str) -> Option<EdComparison> {
    Some(match command {
        "whilee" | "whilevare" | "whilevarvare" => EdComparison::Equal,
        "whilen" | "whilevarn" | "whilevarvarn" => EdComparison::NotEqual,
        "whilel" | "whilevarl" | "whilevarvarl" => EdComparison::Less,
        "whilele" | "whilevarle" | "whilevarvarle" => EdComparison::LessEqual,
        "whileg" | "whilevarg" | "whilevarvarg" | "whilevarvarvarg" => EdComparison::Greater,
        "whilege" | "whilevarge" | "whilevarvarge" => EdComparison::GreaterEqual,
        _ => return None,
    })
}

fn invert_comparison(comparison: EdComparison) -> EdComparison {
    match comparison {
        EdComparison::Equal => EdComparison::NotEqual,
        EdComparison::NotEqual => EdComparison::Equal,
        EdComparison::Less => EdComparison::GreaterEqual,
        EdComparison::LessEqual => EdComparison::Greater,
        EdComparison::Greater => EdComparison::LessEqual,
        EdComparison::GreaterEqual => EdComparison::Less,
        EdComparison::BitAndNonZero => EdComparison::BitAndZero,
        EdComparison::BitAndZero => EdComparison::BitAndNonZero,
        EdComparison::EitherNonZero => EdComparison::BothZero,
        EdComparison::BothZero => EdComparison::EitherNonZero,
        EdComparison::BitXorNonZero => EdComparison::BitXorZero,
        EdComparison::BitXorZero => EdComparison::BitXorNonZero,
        EdComparison::BothNonZero => EdComparison::EitherZero,
        EdComparison::EitherZero => EdComparison::BothNonZero,
        EdComparison::UnsignedGreater => EdComparison::UnsignedLessEqual,
        EdComparison::UnsignedGreaterEqual => EdComparison::UnsignedLess,
        EdComparison::UnsignedLess => EdComparison::UnsignedGreaterEqual,
        EdComparison::UnsignedLessEqual => EdComparison::UnsignedGreater,
    }
}

fn patch_eduke_conditional(cx: &mut Cx<'_>, instruction: usize, target: usize) {
    match cx.img.eduke_program.instructions.get_mut(instruction) {
        Some(EdInstruction::JumpIf {
            target: jump_target,
            ..
        })
        | Some(EdInstruction::IfRandom {
            target: jump_target,
            ..
        }) => *jump_target = target,
        _ => {}
    }
}

fn patch_eduke_jump(cx: &mut Cx<'_>, instruction: usize, target: usize) {
    if let Some(EdInstruction::Jump {
        target: jump_target,
    }) = cx.img.eduke_program.instructions.get_mut(instruction)
    {
        *jump_target = target;
    }
}

fn peek_eduke_token(lx: &Lexer<'_>) -> Option<String> {
    let mut lookahead = lx.clone();
    lookahead.next_token()
}

fn skip_eduke_comments(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, includes: &mut dyn IncludeResolver) {
    while matches!(peek_keyword(lx, ConDialect::EDuke32), Some(20 | 39)) {
        if parse_command(cx, lx, includes) {
            break;
        }
    }
}

fn compile_eduke_block(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, includes: &mut dyn IncludeResolver) {
    cx.brace_depth += 1;
    let block_depth = cx.brace_depth;
    loop {
        if parse_command(cx, lx, includes) || cx.brace_depth < block_depth {
            break;
        }
    }
}

/// Parse EDuke32 metadata directives without changing Atomic opcode layout.
fn parse_eduke_directive(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, directive: &str) -> bool {
    let directive = directive.to_ascii_lowercase();
    if matches!(directive.as_str(), "definequote" | "redefinequote") {
        parse_eduke_quote(cx, lx, &directive);
        return true;
    }
    if !matches!(
        directive.as_str(),
        "var"
            | "gamevar"
            | "array"
            | "gamearray"
            | "defstate"
            | "appendstate"
            | "prependstate"
            | "appendevent"
            | "onevent"
            | "endevent"
            | "endswitch"
            | "defineprojectile"
            | "definegamefuncname"
            | "damageeventtile"
            | "spritenoshade"
            | "spritenopal"
            | "spriteflags"
    ) {
        return false;
    }

    let mut fields = eduke_line_fields(lx).into_iter();
    if matches!(directive.as_str(), "endevent" | "endswitch") {
        if directive == "endevent" {
            if cx.parsing_event {
                cx.img
                    .eduke_program
                    .instructions
                    .push(EdInstruction::EndEvent);
            }
            cx.parsing_event = false;
        }
        return true;
    }
    let Some(name) = fields.next() else {
        cx.record_error();
        return true;
    };

    match directive.as_str() {
        "var" | "gamevar" | "array" | "gamearray" => {
            let Some(value) = fields.next() else {
                cx.record_error();
                return true;
            };
            let flags = fields.collect::<Vec<_>>();
            if directive == "array" || directive == "gamearray" {
                if cx.img.eduke_arrays.iter().any(|entry| entry.name == name) {
                    cx.img.warnings += 1;
                } else {
                    cx.img.eduke_arrays.push(EdArrayDefinition {
                        name,
                        length: value,
                        flags,
                    });
                }
            } else if cx
                .img
                .eduke_variables
                .iter()
                .any(|entry| entry.name == name)
            {
                cx.img.warnings += 1;
            } else {
                cx.img.eduke_variables.push(EdVariableDefinition {
                    name,
                    initial_value: value,
                    flags,
                });
            }
        }
        "defstate" | "appendstate" | "prependstate" => {
            let kind = match directive.as_str() {
                "defstate" => EdStateFragmentKind::Define,
                "appendstate" => EdStateFragmentKind::Append,
                "prependstate" => EdStateFragmentKind::Prepend,
                _ => {
                    cx.record_error();
                    return true;
                }
            };
            begin_eduke_state_fragment(cx, name, kind, directive != "defstate");
        }
        "appendevent" | "onevent" => {
            let entry = cx.img.eduke_program.instructions.len();
            cx.img.eduke_events.push(EdEventDefinition {
                name: name.clone(),
                appended: directive == "appendevent",
            });
            cx.img
                .push_eduke_event_entry(name, entry, directive == "appendevent");
            cx.parsing_event = true;
        }
        "defineprojectile" => {
            let Some(property) = fields.next() else {
                cx.record_error();
                return true;
            };
            let Some(value) = fields.next() else {
                cx.record_error();
                return true;
            };
            cx.img.eduke_projectiles.push(EdProjectileProperty {
                projectile: name,
                property,
                value,
            });
        }
        "definegamefuncname" => {}
        "damageeventtile" => cx.img.eduke_damage_event_tiles.push(name),
        "spritenoshade" | "spritenopal" | "spriteflags" => {
            cx.img
                .eduke_sprite_directives
                .push((directive, std::iter::once(name).chain(fields).collect()));
        }
        _ => {}
    }
    true
}

fn begin_eduke_state_fragment(
    cx: &mut Cx<'_>,
    name: String,
    kind: EdStateFragmentKind,
    appended: bool,
) {
    let entry = cx.img.eduke_program.instructions.len();
    cx.img.eduke_states.push(EdStateDefinition {
        name: name.clone(),
        appended,
    });
    cx.img
        .eduke_state_entries
        .push((name.clone(), entry, appended));
    let key = label_key(cx.img.dialect, &name);
    let label_key = label_key(cx.img.dialect, &name);
    if !cx.img.labels.contains_key(&label_key) {
        cx.img.labels.insert(
            label_key.clone(),
            Label {
                value: entry as i32,
                is_ptr: true,
            },
        );
    }
    cx.eduke_current_state = Some(EdStateFragment {
        key,
        label_key,
        kind,
        entry,
        tail: entry,
    });
    cx.parsing_state = true;
}

fn finish_eduke_state_fragment(cx: &mut Cx<'_>) {
    let Some(mut fragment) = cx.eduke_current_state.take() else {
        return;
    };
    let tail = cx.img.eduke_program.instructions.len().saturating_sub(1);
    if !matches!(
        cx.img.eduke_program.instructions.get(tail),
        Some(EdInstruction::Return)
    ) {
        cx.record_error_as("state-without-return");
        return;
    }
    fragment.tail = tail;
    cx.eduke_state_fragments.push(fragment);
}

fn finalize_eduke_states(cx: &mut Cx<'_>) {
    let mut by_name = BTreeMap::<String, Vec<EdStateFragment>>::new();
    for fragment in cx.eduke_state_fragments.drain(..) {
        by_name
            .entry(fragment.key.clone())
            .or_default()
            .push(fragment);
    }

    for (key, fragments) in by_name {
        // A later `defstate` replaces earlier definitions, matching the
        // compiler's previous state-entry behavior. Extensions after that
        // definition are then linked around the selected body.
        let base_index = fragments
            .iter()
            .rposition(|fragment| fragment.kind == EdStateFragmentKind::Define)
            .unwrap_or(0);
        let base = &fragments[base_index];
        let mut entry = base.entry;
        let mut tail = base.tail;

        for fragment in fragments.iter().skip(base_index + 1) {
            match fragment.kind {
                EdStateFragmentKind::Define => unreachable!("base is the last definition"),
                EdStateFragmentKind::Append => {
                    if let Some(instruction) = cx.img.eduke_program.instructions.get_mut(tail) {
                        *instruction = EdInstruction::Jump {
                            target: fragment.entry,
                        };
                    }
                    tail = fragment.tail;
                }
                EdStateFragmentKind::Prepend => {
                    if let Some(instruction) =
                        cx.img.eduke_program.instructions.get_mut(fragment.tail)
                    {
                        *instruction = EdInstruction::Jump { target: entry };
                    }
                    entry = fragment.entry;
                }
            }
        }

        if let Some(label) = cx.img.labels.get_mut(&base.label_key)
            && label.is_ptr
            && fragments.iter().any(|fragment| {
                i32::try_from(fragment.entry).is_ok_and(|value| value == label.value)
            })
        {
            label.value = i32::try_from(entry).unwrap_or(i32::MAX);
        }
        cx.img.eduke_program.states.insert(key, entry);
    }
}

fn eduke_line_fields(lx: &mut Lexer<'_>) -> Vec<String> {
    let source = lx.rest_of_line();
    let source = source
        .split_once("//")
        .map_or(source.as_str(), |(before_comment, _)| before_comment);
    source
        .split_once("/*")
        .map_or(source, |(before_comment, _)| before_comment)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn parse_eduke_quote(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, directive: &str) {
    let raw_line = lx.rest_of_line();
    let line = trim_eduke_quote_comment(&raw_line).trim_start();
    let Some(split) = line.find(char::is_whitespace) else {
        cx.record_error();
        return;
    };
    let id = line[..split].trim();
    let text = line[split..].trim_start();
    let value = eduke_constant(cx.img, id);
    let Some(value) = value.filter(|value| (0..EDUKE_MAX_QUOTES as i32).contains(value)) else {
        cx.record_error();
        return;
    };
    let text = text
        .strip_prefix('"')
        .and_then(|quoted| quoted.strip_suffix('"'))
        .unwrap_or(text);
    if directive == "redefinequote" && in_body(cx) {
        if text.len() > 4096 {
            cx.record_error();
            return;
        }
        cx.img
            .eduke_program
            .instructions
            .push(EdInstruction::SetQuote {
                quote: value as usize,
                text: text.to_owned(),
            });
        return;
    }
    if text.len() > 4096 {
        cx.record_error();
        return;
    }
    if cx.img.quotes.len() <= value as usize {
        let target_len = value as usize + 1;
        let additional = target_len.saturating_sub(cx.img.quotes.len());
        if cx.img.quotes.try_reserve(additional).is_err() {
            cx.record_error();
            return;
        }
        cx.img.quotes.resize(target_len, String::new());
    }
    cx.img.quotes[value as usize] = text.to_owned();
}

fn trim_eduke_quote_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quoted = false;
    let mut escaped = false;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if quoted && !escaped => escaped = true,
            b'"' if !escaped => quoted = !quoted,
            b'/' if !quoted && bytes.get(index + 1) == Some(&b'/') => return &line[..index],
            b'/' if !quoted && bytes.get(index + 1) == Some(&b'*') => return &line[..index],
            _ => escaped = false,
        }
        index += 1;
    }
    line
}

/// Leave a fail-forward word, parse one body command, patch the slot.
fn parse_if_tail(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, includes: &mut dyn IncludeResolver) -> bool {
    let slot = cx.img.here() as usize;
    cx.img.emit(0);
    // Skip leading comments like Chocolate's keyword() loop.
    loop {
        match peek_keyword(lx, cx.img.dialect) {
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
        cx.record_error();
        return false;
    };
    for slot in 0..11 {
        lx.skip_ws();
        if peek_keyword(lx, cx.img.dialect).is_some() || lx.eof() {
            break;
        }
        let name = lx.raw_token();
        if name.is_empty() {
            break;
        }
        match volume {
            0 => cx.img.environment_music[slot] = name,
            1..=4 => cx.img.music_tracks[((volume - 1) as usize) * 11 + slot] = name,
            _ => cx.record_error(),
        }
    }
    false
}

fn parse_definesound(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    cx.img.rewind();
    let Some(k) = take_num(cx, lx) else {
        cx.record_error();
        return false;
    };
    let sound_limit = if cx.img.dialect == ConDialect::EDuke32 {
        EDUKE_MAX_SOUNDS
    } else {
        NUM_SOUNDS
    };
    if k < 0 || k as usize >= sound_limit {
        cx.record_error();
        return false;
    }
    if k as usize >= cx.img.sounds.len() {
        let target_len = k as usize + 1;
        let additional = target_len.saturating_sub(cx.img.sounds.len());
        if cx.img.sounds.try_reserve(additional).is_err() {
            cx.record_error();
            return false;
        }
        cx.img.sounds.resize(target_len, None);
    }
    let raw_file = lx.raw_token();
    let file = raw_file
        .strip_prefix('"')
        .and_then(|quoted| quoted.strip_suffix('"'))
        .unwrap_or(&raw_file)
        .to_owned();
    if file.is_empty() {
        cx.record_error();
        return false;
    }
    let Some(ps) = take_num(cx, lx) else {
        cx.record_error();
        return false;
    };
    let Some(pe) = take_num(cx, lx) else {
        cx.record_error();
        return false;
    };
    let Some(pr) = take_num(cx, lx) else {
        cx.record_error();
        return false;
    };
    let Some(m) = take_num(cx, lx) else {
        cx.record_error();
        return false;
    };
    let Some(vo) = take_num(cx, lx) else {
        cx.record_error();
        return false;
    };
    let volume = if cx.img.dialect == ConDialect::EDuke32 {
        match eduke_line_fields(lx).first() {
            Some(token) => match eduke_constant(cx.img, token) {
                Some(value) => value,
                None => {
                    cx.record_error();
                    0
                }
            },
            None => 0,
        }
    } else {
        0
    };
    cx.img.sounds[k as usize] = Some(SoundDef {
        file,
        ps,
        pe,
        pr,
        m,
        vo,
        volume,
    });
    false
}

fn parse_definequote(cx: &mut Cx<'_>, lx: &mut Lexer<'_>) -> bool {
    cx.img.rewind();
    let Some(k) = take_num(cx, lx) else {
        cx.record_error();
        return false;
    };
    if k < 0 || k as usize >= NUM_QUOTES {
        cx.record_error();
        return false;
    }
    lx.skip_spaces();
    let mut text = lx.rest_of_line();
    if text.len() > 63 {
        text.truncate(63);
        cx.record_error();
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
            if peek_keyword(lx, cx.img.dialect).is_some() {
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
        cx.record_error();
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
            cx.record_error();
            return false;
        }
        cx.img.volume_names[j as usize] = name;
    } else {
        if j < 0 || j as usize >= cx.img.skill_names.len() {
            cx.record_error();
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
        cx.record_error();
        return false;
    };
    let Some(lev) = take_num(cx, lx) else {
        cx.record_error();
        return false;
    };
    if vol < 0 || lev < 0 || (vol * 11 + lev) as usize >= cx.img.levels.len() {
        cx.record_error();
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
            cx.record_error_as("action:unresolved-reference");
        }
        return false;
    }
    cx.img.rewind();
    let name = lx.get_label();
    if name.is_empty() || keyword_index(&name).is_some() {
        cx.record_error();
        return false;
    }
    let bind = bind_label(cx, &name);
    let entry = cx.img.here();
    if bind {
        cx.img.labels.insert(
            label_key(cx.img.dialect, &name),
            Label {
                value: entry,
                is_ptr: true,
            },
        );
    }
    let mut got = 0;
    while got < 5 && peek_keyword(lx, cx.img.dialect).is_none() {
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
            cx.record_error_as("move:unresolved-reference");
            return false;
        }
        let mut flags = 0_i32;
        while peek_keyword(lx, cx.img.dialect).is_none() {
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
        cx.record_error();
        return false;
    }
    let bind = bind_label(cx, &name);
    let entry = cx.img.here();
    if bind {
        cx.img.labels.insert(
            label_key(cx.img.dialect, &name),
            Label {
                value: entry,
                is_ptr: true,
            },
        );
    }
    let mut got = 0;
    while got < 2 && peek_keyword(lx, cx.img.dialect).is_none() {
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
            cx.record_error_as("ai:unresolved-reference");
        }
        return false;
    }
    cx.img.rewind();
    let name = lx.get_label();
    if name.is_empty() || keyword_index(&name).is_some() {
        cx.record_error();
        return false;
    }
    let bind = bind_label(cx, &name);
    let entry = cx.img.here();
    if bind {
        cx.img.labels.insert(
            label_key(cx.img.dialect, &name),
            Label {
                value: entry,
                is_ptr: true,
            },
        );
    }
    let mut got = 0;
    while got < 3 && peek_keyword(lx, cx.img.dialect).is_none() {
        if got == 2 {
            let mut flags = 0_i32;
            while peek_keyword(lx, cx.img.dialect).is_none() {
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
            cx.record_error();
            return false;
        }
        let bind = bind_label(cx, &name);
        let entry = cx.img.here();
        if bind {
            cx.img.labels.insert(
                label_key(cx.img.dialect, &name),
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
    match find_label(cx.img, &name) {
        Some(lab) => {
            if lab.is_ptr {
                cx.img.emit_ptr(lab.value);
            } else {
                cx.img.emit(lab.value);
            }
        }
        None => {
            cx.record_error();
            cx.img.emit(0);
        }
    }
    false
}

fn parse_actor(cx: &mut Cx<'_>, lx: &mut Lexer<'_>, user: bool) -> bool {
    if cx.parsing_state {
        cx.record_error_as("actor:state-already-open");
    }
    if cx.parsing_actor.is_some() {
        cx.record_error_as("actor:already-open");
    }
    cx.img.rewind();
    let header = cx.img.here() as usize;
    cx.parsing_actor = Some(header);

    let mut actor_kind = 0_i32;
    if user {
        if !emit_trans_num(cx, lx) {
            cx.record_error_as("useractor:invalid-type");
            return false;
        }
        actor_kind = cx.img.pop_word().map(|(v, _)| v).unwrap_or(0);
    }

    if !emit_trans_num(cx, lx) {
        cx.record_error_as("actor:invalid-tile");
        return false;
    }
    let tile = cx.img.pop_word().map(|(v, _)| v).unwrap_or(0);
    let tile_limit = if cx.img.dialect == ConDialect::EDuke32 {
        EDUKE_MAX_TILES
    } else {
        MAX_TILES
    };
    if tile < 0 || tile as usize >= tile_limit {
        cx.record_error_as("actor:tile-out-of-range");
        return false;
    }
    if tile as usize >= cx.img.actorscrptr.len() {
        let target_len = tile as usize + 1;
        let additional = target_len.saturating_sub(cx.img.actorscrptr.len());
        if cx.img.actorscrptr.try_reserve(additional).is_err()
            || cx.img.actortype.try_reserve(additional).is_err()
        {
            cx.record_error_as("actor:tile-allocation-failed");
            return false;
        }
        cx.img.actorscrptr.resize(target_len, 0);
        cx.img.actortype.resize(target_len, 0);
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
            while peek_keyword(lx, cx.img.dialect).is_none() {
                if !emit_trans_num(cx, lx) {
                    break;
                }
                flags |= cx.img.pop_word().map(|(v, _)| v).unwrap_or(0);
            }
            cx.img.script[header + 3] = flags;
            break;
        }
        if peek_keyword(lx, cx.img.dialect).is_some() {
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
    if cx.img.dialect == ConDialect::EDuke32 {
        cx.img
            .eduke_actor_entries
            .insert(tile as i16, cx.img.eduke_program.instructions.len());
    }
    false
}

/// Returns whether to bind a new label (false on duplicate — keep first).
fn bind_label(cx: &mut Cx<'_>, name: &str) -> bool {
    if cx.img.labels.contains_key(&label_key(cx.img.dialect, name)) {
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
    if let Some(lab) = find_label(cx.img, &tok) {
        if lab.is_ptr {
            cx.img.emit_ptr(lab.value);
        } else {
            cx.img.emit(lab.value);
        }
        return true;
    }
    if cx.img.dialect == ConDialect::EDuke32 {
        if let Some(value) = eduke_constant(cx.img, &tok) {
            cx.img.emit(value);
            return true;
        }
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

fn label_key(_dialect: ConDialect, name: &str) -> String {
    name.to_owned()
}

fn find_label<'a>(img: &'a ScriptImage, name: &str) -> Option<&'a Label> {
    img.labels.get(&label_key(img.dialect, name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn legacy_var_arithmetic_spellings_compile_as_modern_forms() {
        let img = compile_with_options(
            b"var a 0 0\nvar b 3 0\nappendevent EVENT_GAME\nsetvar a 5\naddvarvar a b\nmulvar a 2\nshiftvarr a 1\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={:?}", img.eduke_error_heads);
    }

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

    #[test]
    fn eduke32_lexes_prefixed_comments_and_case_insensitive_keywords() {
        let src = b"/*---- header include missing.con\ncontinues */\nDEFINE FOO 7 //--- include also_missing.con\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.dialect, ConDialect::EDuke32);
        assert_eq!(img.errors, 0, "errors={}", img.errors);
        assert_eq!(img.labels.get("FOO").map(|label| label.value), Some(7));
        assert_eq!(img.con_size(), 0);
    }

    #[test]
    fn eduke32_ignores_an_orphan_else_branch() {
        let image = compile_with_options(
            b"var branch 0\ndefstate orphan_else\n{\nelse\nset branch 1\n}\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert!(
            image
                .eduke_program
                .instructions
                .iter()
                .all(|instruction| !matches!(instruction, EdInstruction::Assign { .. })),
            "an orphan else body must not enter the VM"
        );
    }

    #[test]
    fn eduke32_records_var_and_array_declarations() {
        let src = b"var actor_ticks 0 2\ngamevar player_mode FALSE 1\ngamearray sprite_cache MAXSPRITES GAMEARRAY_RESTORE\ndefstate idle\nends\nappendevent EVENT_GAME\nendevent\ndefineprojectile GRENADE PROJ_VEL 100\ndamageeventtile TROOPER\nspritenoshade SHADOW\nspritenopal SHADOW\nredefinequote 1 \"updated quote\"\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={}", img.errors);
        assert_eq!(
            img.eduke_variables,
            [
                EdVariableDefinition {
                    name: "actor_ticks".into(),
                    initial_value: "0".into(),
                    flags: vec!["2".into()],
                },
                EdVariableDefinition {
                    name: "player_mode".into(),
                    initial_value: "FALSE".into(),
                    flags: vec!["1".into()],
                },
            ]
        );
        assert_eq!(
            img.eduke_arrays,
            [EdArrayDefinition {
                name: "sprite_cache".into(),
                length: "MAXSPRITES".into(),
                flags: vec!["GAMEARRAY_RESTORE".into()],
            }]
        );
        assert_eq!(
            img.eduke_states,
            [EdStateDefinition {
                name: "idle".into(),
                appended: false,
            }]
        );
        assert_eq!(
            img.eduke_events,
            [EdEventDefinition {
                name: "EVENT_GAME".into(),
                appended: true,
            }]
        );
        assert_eq!(img.eduke_projectiles.len(), 1);
        assert_eq!(img.eduke_damage_event_tiles, ["TROOPER"]);
        assert_eq!(img.eduke_sprite_directives.len(), 2);
        assert_eq!(img.quotes[1], "updated quote");
    }

    #[test]
    fn eduke32_compiles_hitradius_arguments_as_a_live_instruction() {
        let image = compile_with_options(
            b"defstate blast\nhitradius 640 5 10 20 40\nends\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert_eq!(image.eduke_noop_heads.get("hitradius"), None);
        assert!(image.eduke_program.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                EdInstruction::HitRadius {
                    radius: EdOperand::Immediate(640),
                    damage: [
                        EdOperand::Immediate(5),
                        EdOperand::Immediate(10),
                        EdOperand::Immediate(20),
                        EdOperand::Immediate(40),
                    ],
                }
            )
        }));
    }

    #[test]
    fn eduke32_resizearray_grows_shrinks_and_zero_fills() {
        let src = b"gamearray entries 0 0\ngamevar requested 3 0\ndefstate resize_entries\nresizearray entries requested\nsetarray entries[0] 17\nset requested 1\nresizearray entries requested\nset requested 3\nresizearray entries requested\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={:?}", img.eduke_error_heads);
        let mut state = super::super::eduke_vm::EdukeVmState::from_image(&img)
            .expect("valid EDuke32 declarations");
        state
            .execute_state(&img.eduke_program, "resize_entries", 0, 0, 16, 2)
            .expect("resizearray instructions execute");
        assert_eq!(state.get_array("entries", 0), Ok(17));
        assert_eq!(state.get_array("entries", 1), Ok(0));
        assert!(matches!(
            state.get_array("entries", 3),
            Err(super::super::eduke_vm::EdukeVmError::IndexOutOfBounds { kind: "array", .. })
        ));
    }

    #[test]
    fn eduke32_resizearray_rejects_negative_length() {
        let src = b"gamearray entries 1 0\ngamevar requested -1 0\ndefstate resize_entries\nresizearray entries requested\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={:?}", img.eduke_error_heads);
        let mut state = super::super::eduke_vm::EdukeVmState::from_image(&img)
            .expect("valid EDuke32 declarations");
        assert!(matches!(
            state.execute_state(&img.eduke_program, "resize_entries", 0, 0, 8, 2),
            Err(super::super::eduke_vm::EdukeVmError::NegativeArraySize {
                name,
                length: -1
            }) if name == "entries"
        ));
    }

    #[test]
    fn eduke32_statement_heads_stop_legacy_header_and_if_operand_scans() {
        let src = b"define P_STANDING 1\ngamevar timer 0 0\nmove EV_TEST 100\ndefineprojectile GRENADE PROJ_VEL 200\nuseractor notenemy 100\nife timer 0\n{\nset timer 2\n}\nifp P_STANDING\nset timer 3\nenda\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={:?}", img.eduke_error_heads);
        assert_eq!(img.eduke_projectiles.len(), 1);
        assert!(img.actorscrptr[100] > 0);
    }

    #[test]
    fn eduke_else_if_comparison_consumes_braced_branch_and_fallback() {
        let src = b"gamevar selector 0 0\ngamevar result 0 0\ndefstate choose\nife selector 0\n{\nset result 1\n}\nelse ife selector 2\n{\nset result 2\n}\nelse\n{\nset result 3\n}\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={:?}", img.eduke_error_heads);
        let mut state = super::super::eduke_vm::EdukeVmState::from_image(&img)
            .expect("valid EDuke32 declarations");
        for (selector, expected) in [(0, 1), (2, 2), (5, 3)] {
            state
                .set_variable("selector", 0, 0, selector)
                .expect("selector gamevar");
            state
                .execute_state(&img.eduke_program, "choose", 0, 0, 64, 4)
                .expect("else-if chain executes");
            assert_eq!(state.get_variable("result", 0, 0), Ok(expected));
        }
    }

    #[test]
    fn eduke32_range_for_uses_a_saved_bound_and_next_line_brace_block() {
        let src = b"gamevar iterator 0 0\ngamevar total 0 0\ngamevar bound 3 0\ndefstate loop_range\nfor iterator range bound\n{\nadd total 1\nadd iterator 100\nset bound 100\n}\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={}", img.errors);

        let mut state = super::super::eduke_vm::EdukeVmState::from_image(&img)
            .expect("valid EDuke32 declarations");
        let entry = img.eduke_program.states["loop_range"];
        state
            .execute_at(&img.eduke_program, entry, 0, 0, 32, 4)
            .expect("range loop stays bounded by its initial value");
        assert_eq!(state.get_variable("total", 0, 0), Ok(3));
        assert_eq!(state.get_variable("iterator", 0, 0), Ok(102));
        assert_eq!(state.get_variable("bound", 0, 0), Ok(100));
    }

    #[test]
    fn eduke32_break_exits_state_when_no_loop_is_open() {
        let src =
            b"gamevar result 0 0\ndefstate stop_early\nset result 1\nbreak\nset result 2\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={}", img.errors);
        let mut state = super::super::eduke_vm::EdukeVmState::from_image(&img)
            .expect("valid EDuke32 declarations");
        let entry = img.eduke_program.states["stop_early"];
        state
            .execute_at(&img.eduke_program, entry, 0, 0, 32, 4)
            .expect("break returns from the state");
        assert_eq!(state.get_variable("result", 0, 0), Ok(1));
    }

    #[test]
    fn eduke32_inline_break_exits_the_enclosing_range_loop() {
        let src = b"gamevar iterator 0 0\ngamevar total 0 0\ndefstate stop_loop\nfor iterator range 3\n{\nife iterator 0 break\nadd total 1\n}\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={}", img.errors);
        assert_eq!(img.eduke_noop_heads.get("break"), None);
        let mut state = super::super::eduke_vm::EdukeVmState::from_image(&img)
            .expect("valid EDuke32 declarations");
        let entry = img.eduke_program.states["stop_loop"];
        state
            .execute_at(&img.eduke_program, entry, 0, 0, 64, 4)
            .expect("inline break exits the active loop");
        assert_eq!(state.get_variable("total", 0, 0), Ok(0));
        assert_eq!(state.get_variable("iterator", 0, 0), Ok(0));
    }

    #[test]
    fn eduke32_accepts_extended_sounds_useractor_and_indexed_setarray() {
        let src = b"define EXTRA_SOUND 597\ngamearray items 4 0\ndefinesound EXTRA_SOUND \"extra.ogg\" 0 0 3 0 0 400\ndefstate marker\nends\nuseractor notenemy 100 0 state marker enda\nuseractor enemy 101 0 state marker enda\nuseractor enemystayput 12000 0 enda\ndefstate fill_item\nsetarray items[2] 7\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={:?}", img.eduke_error_heads);
        assert!(img.sounds.len() > 597);
        assert_eq!(
            img.sounds[597].as_ref().map(|sound| sound.file.as_str()),
            Some("extra.ogg")
        );
        assert_eq!(
            img.sounds[597].as_ref().map(|sound| sound.volume),
            Some(400)
        );
        assert!(img.actorscrptr[100] > 0);
        assert!(img.actorscrptr[101] > 0);
        assert!(img.actorscrptr[12_000] > 0);

        let mut state = super::super::eduke_vm::EdukeVmState::from_image(&img)
            .expect("valid EDuke32 declarations");
        let entry = img.eduke_program.states["fill_item"];
        state
            .execute_at(&img.eduke_program, entry, 0, 0, 16, 4)
            .expect("indexed setarray executes");
        assert_eq!(state.get_array("items", 2), Ok(7));
    }

    #[test]
    fn eduke32_inline_else_compiles_both_branches() {
        let src = b"gamevar result 0 0\ndefstate inline_else\nife result 0 set result 1 else set result 2\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={}", img.errors);
        let mut state = super::super::eduke_vm::EdukeVmState::from_image(&img)
            .expect("valid EDuke32 declarations");
        let entry = img.eduke_program.states["inline_else"];
        state
            .execute_at(&img.eduke_program, entry, 0, 0, 16, 4)
            .expect("conditional branches execute");
        assert_eq!(state.get_variable("result", 0, 0), Ok(1));
    }

    #[test]
    fn eduke32_comparison_spellings_compile_to_conditional_branches() {
        let commands = [
            "ife",
            "ifn",
            "ifl",
            "ifle",
            "ifg",
            "ifge",
            "ifa",
            "ifae",
            "ifb",
            "ifbe",
            "ifand",
            "ifor",
            "ifxor",
            "ifboth",
            "ifvare",
            "ifvarn",
            "ifvarl",
            "ifvarle",
            "ifvarg",
            "ifvarge",
            "ifvara",
            "ifvarae",
            "ifvarb",
            "ifvarbe",
            "ifvarand",
            "ifvaror",
            "ifvareither",
            "ifvarxor",
            "ifvarboth",
            "ifvarvare",
            "ifvarvarn",
            "ifvarvarl",
            "ifvarvarle",
            "ifvarvarvarg",
            "ifvarvarge",
            "ifvarvara",
            "ifvarvarae",
            "ifvarvarb",
            "ifvarvarbe",
            "ifvarvarand",
            "ifvarvaror",
            "ifvarvarvareither",
            "ifvarvarxor",
            "ifvarvarboth",
        ];
        for command in commands {
            let source = format!(
                "gamevar left 0 0\ngamevar right 0 0\ngamevar result 0 0\ndefstate check\n{command} left right set result 1\nends\n"
            );
            let image = compile_with_options(
                source.as_bytes(),
                &mut NullIncludes,
                ConCompileOptions {
                    dialect: ConDialect::EDuke32,
                },
            );
            assert_eq!(image.errors, 0, "{command}: {:?}", image.eduke_error_heads);
            assert!(
                image
                    .eduke_program
                    .instructions
                    .iter()
                    .any(|instruction| matches!(instruction, EdInstruction::JumpIf { .. })),
                "{command} must compile to a VM conditional"
            );
        }
    }

    #[test]
    fn eduke32_switch_closes_after_nested_if_blocks_and_breaks() {
        let src = b"gamevar phase 0 0\ngamevar timer 0 0\ndefstate reload\nadd timer 1\nswitch phase\ncase 0\nifge timer 1\n{\nset timer 0\n}\nbreak\ncase 1\nifge timer 2\n{\nset timer 0\n}\nbreak\nendswitch\nset phase 2\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={:?}", img.eduke_error_heads);
    }

    #[test]
    fn eduke32_accepts_compact_struct_fields_inline_and_prepend_state_bodies() {
        let src = b"gamevar result 0 0\ndefstate base\nset result 1\nends\nprependstate base\nifg result 0 seta[THISACTOR].picnum 7\nsetactorvar[THISACTOR].guard TRUE\nends\ndefstate direct\nseta[THISACTOR].htextra 0\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={:?}", img.eduke_error_heads);
        assert!(
            img.eduke_states
                .iter()
                .any(|state| state.name == "base" && state.appended)
        );
        assert!(
            img.eduke_program
                .instructions
                .iter()
                .any(|instruction| matches!(
                    instruction,
                    EdInstruction::SetStructMember {
                        kind: EdStructKind::Actor,
                        member,
                        ..
                    } if member == "picnum"
                ))
        );
        assert!(
            img.eduke_program
                .instructions
                .iter()
                .any(|instruction| matches!(
                    instruction,
                    EdInstruction::SetScopedVariable {
                        scope: EdVariableScope::Actor,
                        name,
                        ..
                    } if name == "guard"
                ))
        );
        assert!(
            img.eduke_program
                .instructions
                .iter()
                .any(|instruction| matches!(
                    instruction,
                    EdInstruction::SetStructMember {
                        kind: EdStructKind::Actor,
                        member,
                        ..
                    } if member == "htextra"
                ))
        );
    }

    #[test]
    fn eduke32_actor_commands_compile_to_live_actor_instructions() {
        let image = compile_with_options(
            b"appendevent EVENT_GAME\naddkills 2\nstrength 100\naddstrength 5\ncstat 8\ncstator 4\ncactor 101\nspritepal 7\ngetlastpal\nclipdist 32\ncount 12\nsleeptime 9\nsizeat 20 30\nsizeto 22 10\nfall\nresetactioncount\nresetcount\nendevent\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        assert!(
            image
                .eduke_program
                .instructions
                .iter()
                .any(|instruction| matches!(instruction, EdInstruction::AddActorKills { .. }))
        );
        assert!(image.eduke_program.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                EdInstruction::SetActorMember {
                    member: EdActorMember::Strength,
                    ..
                }
            )
        }));
        for member in [
            EdActorMember::AddStrength,
            EdActorMember::CstatOr,
            EdActorMember::ClipDistance,
            EdActorMember::Count,
            EdActorMember::SleepTime,
        ] {
            assert!(image.eduke_program.instructions.iter().any(|instruction| {
                matches!(instruction, EdInstruction::SetActorMember { member: found, .. } if *found == member)
            }), "missing actor member instruction for {member:?}");
        }
        assert!(
            image
                .eduke_program
                .instructions
                .iter()
                .any(|instruction| { matches!(instruction, EdInstruction::RestoreActorPalette) })
        );
        assert!(image.eduke_program.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                EdInstruction::SetActorMember {
                    member: EdActorMember::Cstat,
                    ..
                }
            )
        }));
        assert!(image.eduke_program.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                EdInstruction::SetActorMember {
                    member: EdActorMember::Tile,
                    ..
                }
            )
        }));
        assert!(image.eduke_program.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                EdInstruction::SetActorMember {
                    member: EdActorMember::Palette,
                    ..
                }
            )
        }));
        assert!(
            image
                .eduke_program
                .instructions
                .iter()
                .any(|instruction| matches!(instruction, EdInstruction::SetActorSize { .. }))
        );
        assert!(
            image
                .eduke_program
                .instructions
                .iter()
                .any(|instruction| { matches!(instruction, EdInstruction::SetActorSizeTo { .. }) })
        );
        assert!(
            image
                .eduke_program
                .instructions
                .iter()
                .any(|instruction| matches!(instruction, EdInstruction::FallActor))
        );
        assert!(image.eduke_program.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                EdInstruction::ResetActorCounter { action: true }
            )
        }));
        assert!(image.eduke_program.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                EdInstruction::ResetActorCounter { action: false }
            )
        }));
    }

    #[test]
    fn eduke_actor_entry_points_to_the_compiled_body() {
        let image = compile_with_options(
            b"gamevar actor_ticks 0 2\nactor 100\nadd actor_ticks 1\nstrength 99\nenda\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let entry = image.eduke_actor_entries[&100];
        assert!(matches!(
            image.eduke_program.instructions.get(entry),
            Some(EdInstruction::Binary {
                target: EdLValue::Variable(name),
                operator: EdBinaryOperator::Add,
                ..
            }) if name == "actor_ticks"
        ));
        assert!(matches!(
            image.eduke_program.instructions.get(entry + 1),
            Some(EdInstruction::SetActorMember {
                member: EdActorMember::Strength,
                ..
            })
        ));
        assert!(matches!(
            image.eduke_program.instructions.get(entry + 2),
            Some(EdInstruction::Return)
        ));
    }

    #[test]
    fn eduke32_state_extensions_execute_in_prepend_then_base_then_append_order() {
        let src = b"gamevar result 0 0\ndefstate chain\nadd result 1\nends\nprependstate chain\nadd result 10\nends\nappendstate chain\nmul result 2\nends\nappendstate chain\nadd result 3\nends\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0, "errors={:?}", img.eduke_error_heads);
        let mut state = super::super::eduke_vm::EdukeVmState::from_image(&img)
            .expect("valid EDuke32 declarations");
        let entry = img.eduke_program.states["chain"];
        assert_eq!(img.labels["chain"].value, entry as i32);
        state
            .execute_at(&img.eduke_program, entry, 0, 0, 32, 8)
            .expect("state extensions execute");
        assert_eq!(state.get_variable("result", 0, 0), Ok(25));
    }

    #[test]
    fn eduke32_state_without_ends_is_diagnosed_and_kept_bounded() {
        let img = compile_with_options(
            b"defstate unfinished\nnullop\n",
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 1);
        assert!(img.eduke_error_heads.contains_key("state-missing-ends"));
        assert!(matches!(
            img.eduke_program.instructions.last(),
            Some(EdInstruction::Return)
        ));
    }

    #[test]
    fn eduke32_quote_definitions_preserve_unquoted_trailing_space() {
        let src = b"define PREFIX_QUOTE 668\ndefinequote PREFIX_QUOTE \"\"\nredefinequote PREFIX_QUOTE [DEBUG]: \n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0);
        assert_eq!(img.quotes[668], "[DEBUG]: ");
    }

    #[test]
    fn eduke32_symbols_preserve_case_and_index_states_events() {
        let src = b"define VALUE_X 1\nDEFINE value_x 4\ndefstate AddOne\nnullop\nends\nappendevent EVENT_GAME\nstate AddOne\nendevent\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 0);
        assert_eq!(img.warnings, 0);
        assert_eq!(img.labels.get("VALUE_X").map(|label| label.value), Some(1));
        assert_eq!(img.labels.get("value_x").map(|label| label.value), Some(4));
        assert_eq!(
            img.labels.get("AddOne").map(|label| label.is_ptr),
            Some(true)
        );
        assert!(img.eduke_program.states.contains_key("AddOne"));
        assert_eq!(img.eduke_state_entries.len(), 1);
        assert_eq!(img.eduke_event_entries.len(), 1);
        assert_eq!(img.eduke_event_entries[0].0, "EVENT_GAME");
        assert!(img.eduke_event_entries[0].1 < img.eduke_program.instructions.len());
    }

    #[test]
    fn eduke32_classic_state_blocks_are_registered_and_callable() {
        let src = b"action ACT_IDLE 0 4 1 1 10\nstate AddOne\nifaction ACT_IDLE nullop\nends\nactor 3\nstate AddOne\nenda\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );

        assert_eq!(img.errors, 0);
        assert_eq!(img.eduke_state_entries.len(), 1);
        assert!(img.eduke_program.states.contains_key("AddOne"));
        assert_eq!(eduke_constant(&img, "SFLAG_NOEVENTS"), Some(128));
        let action = img.labels.get("ACT_IDLE").expect("action label").value;
        assert!(img.eduke_program.instructions.iter().any(|instruction| {
            matches!(instruction, EdInstruction::CallState { name } if name == "AddOne")
        }));
        assert!(img.eduke_program.instructions.iter().any(|instruction| {
            matches!(instruction, EdInstruction::JumpIf { right: EdOperand::Immediate(value), .. } if *value == action)
        }));
    }

    #[test]
    fn eduke32_unsupported_statements_report_once_and_continue_by_line() {
        let src = b"set unknown_var 42\ndefine AFTER_ERROR 9\n";
        let img = compile_with_options(
            src,
            &mut NullIncludes,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(img.errors, 1);
        assert_eq!(img.eduke_error_heads.get("set"), Some(&1));
        assert_eq!(
            img.eduke_error_samples.get("set"),
            Some(&vec![
                "<root>:1 (state=false, event=false, actor=false)".to_owned()
            ])
        );
        assert_eq!(
            img.labels.get("AFTER_ERROR").map(|label| label.value),
            Some(9)
        );
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
        assert!(!img.labels.contains_key("B"));
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
