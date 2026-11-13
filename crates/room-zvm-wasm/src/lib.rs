//! ZScript VM bytecode → WebAssembly code generator (`docs/37-zscript-to-wasm.md`).
//!
//! Input is a linked [`CompiledProgram`]; output is one WebAssembly module in
//! which every supported function is a native wasm function. The interpreter
//! defines the semantics: each opcode is lowered to reproduce
//! `room-zvm/src/interp.rs` exactly, including traps, fuel (charged per basic
//! block), call depth and return slots.
//!
//! [`Target::Aot`] modules import the `"zvm"` runtime ([`room_zvm::aot_abi`])
//! and run against the engine's own heap and natives, so the interpreter and
//! the compiled code can be swapped mid-session (`ROOM_ZVM_TIER=aot`).

pub mod bd;
mod cfg;
pub mod report;
pub mod roommod;
pub use roommod::{RoomMod, read_room_mod, register_shipped, shipped_for};
pub mod check;
mod emit;

use room_zvm::CompiledProgram;
pub use room_zvm::aot_abi;
use thiserror::Error;

/// What kind of module to build.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// Tier 2 of the ZVM: imports the `"zvm"` runtime (`docs/37` §5).
    #[default]
    Aot,
    /// A converted mod's module (`docs/37` §6, engine-side heap design):
    /// the same code as [`Target::Aot`] compiled from the *unlinked* program
    /// (native ids = `native_imports` indices), plus the program image and the
    /// per-function flags in custom sections, so the game can load it without
    /// the ZScript compiler. See [`read_room_mod`].
    RoomMod,
}

/// Code generation options.
#[derive(Clone, Debug)]
pub struct Options {
    /// Output kind.
    pub target: Target,
    /// Emit a `name` section with `Class.method` names.
    pub emit_names: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            target: Target::Aot,
            emit_names: true,
        }
    }
}

/// Why a function stays interpreted. Calls into it go through `zvm.call_interp`.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum Unsupported {
    /// Legacy name-resolved `CallStatic` / register-window `CallNative`.
    #[error("legacy CallStatic/CallNative")]
    LegacyCall,
    /// An `Arg` and the call that consumes it are in different basic blocks.
    #[error("Arg pushed across a basic block")]
    ArgAcrossBlocks,
    /// Byte that is not an opcode.
    #[error("bad opcode {0}")]
    BadOpcode(u8),
    /// Op at the end of the code without its extension word.
    #[error("truncated extension word")]
    TruncatedExt,
    /// Jump into the middle of an instruction.
    #[error("jump into an extension word")]
    BadJumpTarget,
    /// Irreducible control flow (never produced by `room-zscript`).
    #[error("irreducible control flow")]
    Irreducible,
    /// More than [`aot_abi::MAX_ARGS`] arguments to one call.
    #[error("too many call arguments")]
    TooManyArgs,
}

/// Code generator failure (the whole module, not one function).
#[derive(Clone, Debug, Error)]
pub enum CodegenError {
    /// The requested target is not implemented.
    #[error("target {0:?} is not implemented yet (docs/37 ZW6)")]
    TargetUnavailable(Target),
    /// The emitted module failed validation (a code generator bug).
    #[error("invalid module: {0}")]
    Invalid(String),
}

/// Code size and coverage numbers.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    /// Functions in the program.
    pub functions: usize,
    /// Functions compiled to wasm.
    pub compiled: usize,
    /// Basic blocks in compiled functions.
    pub blocks: usize,
    /// Bytecode instructions in compiled functions.
    pub insns: usize,
    /// Module size in bytes.
    pub bytes: usize,
}

/// Generated module.
#[derive(Clone, Debug)]
pub struct Artifact {
    /// WebAssembly binary.
    pub bytes: Vec<u8>,
    /// Per [`CompiledProgram::functions`] entry: compiled to wasm.
    pub compiled: Vec<bool>,
    /// Per function: worth entering through wasm from the host under the
    /// `auto` policy (has a loop or a long body). Others are still compiled
    /// and called from wasm code, but top-level calls stay on the interpreter.
    pub hot: Vec<bool>,
    /// Functions left to the interpreter, with the reason.
    pub unsupported: Vec<(u32, Unsupported)>,
    /// Numbers for reports.
    pub stats: Stats,
}

/// Compile `program` to a WebAssembly module.
///
/// The program must already be linked the way the VM runs it (native ids
/// remapped by [`room_zvm::Zvm::new`]); compile `vm.program`, not the raw
/// compiler output.
pub fn compile(program: &CompiledProgram, opts: &Options) -> Result<Artifact, CodegenError> {
    let mut art = emit::emit_aot(program, opts);
    if opts.target == Target::RoomMod {
        roommod::append_sections(&mut art, program);
        art.stats.bytes = art.bytes.len();
    }
    validate(&art.bytes)?;
    Ok(art)
}

/// Validate a module with exactly the features the code generator uses.
pub fn validate(bytes: &[u8]) -> Result<(), CodegenError> {
    let features = wasmparser::WasmFeatures::MUTABLE_GLOBAL
        | wasmparser::WasmFeatures::SATURATING_FLOAT_TO_INT
        | wasmparser::WasmFeatures::BULK_MEMORY
        | wasmparser::WasmFeatures::SIGN_EXTENSION
        | wasmparser::WasmFeatures::FLOATS;
    wasmparser::Validator::new_with_features(features)
        .validate_all(bytes)
        .map(|_| ())
        .map_err(|e| CodegenError::Invalid(e.to_string()))
}

/// Text form of a module (`--wat`).
pub fn to_wat(bytes: &[u8]) -> Result<String, CodegenError> {
    wasmprinter::print_bytes(bytes).map_err(|e| CodegenError::Invalid(e.to_string()))
}

/// Compile `vm.program` and attach the module as the VM's AOT tier.
///
/// Calls into compiled functions then run as wasm; unsupported functions stay
/// interpreted. Select the tier with [`room_zvm::Zvm::set_aot_enabled`] or the
/// `ROOM_ZVM_TIER` environment variable (`aot` / `interp`).
pub fn enable_aot(vm: &mut room_zvm::Zvm) -> Result<Stats, CodegenError> {
    let art = compile_cached(&vm.program)?;
    vm.attach_aot_with(&art.bytes, art.compiled.clone(), art.hot.clone())
        .map_err(|e| CodegenError::Invalid(e.to_string()))?;
    Ok(art.stats.clone())
}

/// [`compile`] with [`Options::default`], memoized per program content
/// (`docs/37` §1: "AOT-compiled, cached by program hash").
pub fn compile_cached(program: &CompiledProgram) -> Result<std::sync::Arc<Artifact>, CodegenError> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<(u64, u64), Arc<Artifact>>>> = OnceLock::new();
    let key = program_digest(program);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Ok(hit);
    }
    let art = Arc::new(compile(program, &Options::default())?);
    if let Ok(mut c) = cache.lock() {
        if c.len() >= 16 {
            c.clear();
        }
        c.insert(key, Arc::clone(&art));
    }
    Ok(art)
}

/// Digest of everything the generated code depends on: code words, integer
/// and float constants, register counts and parameters of every function.
#[must_use]
pub fn program_digest(program: &CompiledProgram) -> (u64, u64) {
    let mut a: u64 = 0xcbf2_9ce4_8422_2325;
    let mut b: u64 = 0x84222325_cbf29ce4;
    let mut mix = |v: u64| {
        a = (a ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        b = (b.rotate_left(17) ^ v).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    };
    mix(program.functions.len() as u64);
    for f in &program.functions {
        mix(f.code.len() as u64);
        for w in &f.code {
            mix(u64::from(*w));
        }
        mix(f.const_i.len() as u64);
        for v in &f.const_i {
            mix(*v as u64);
        }
        mix(f.const_f.len() as u64);
        for v in &f.const_f {
            mix(v.to_bits());
        }
        mix(u64::from(f.n_i)
            | u64::from(f.n_f) << 8
            | u64::from(f.n_s) << 16
            | u64::from(f.n_a) << 24);
        mix(f.params.len() as u64);
        for p in &f.params {
            mix(u64::from(p.bank) << 16 | u64::from(p.reg));
        }
    }
    (a, b)
}

#[cfg(test)]
mod tests;
