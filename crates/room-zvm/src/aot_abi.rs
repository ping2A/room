//! Shared contract between the WASM code generator (`room-zvm-wasm`) and the
//! AOT host (`docs/37` §5): linear-memory layout, value cells, import names
//! and the register image layout. Both sides use these definitions, so they
//! cannot drift apart.

use crate::program::CompiledFunction;

/// Size of one tagged value cell in linear memory (`tag: u32`, pad, `payload: u64`).
pub const CELL: u32 = 16;
/// Return slots `rets[0..8]`, one cell each.
pub const RETS: u32 = 64;
/// Number of return slots (`Zvm::rets`).
pub const N_RETS: u32 = 8;
/// Exit record written when the outermost AOT frame returns.
///
/// Layout: `kind: u32 @0`, `tag: u32 @4`, `payload: u64 @8`, `i[0..4]: i64 @16`,
/// `s[0]: u32 @48`, `f[0]: f64 @56`.
pub const EXIT: u32 = 192;
/// Argument cells for natives, virtual calls and interpreter bridges.
pub const ARGS: u32 = 256;
/// Maximum number of argument cells per call.
pub const MAX_ARGS: u32 = 64;
/// Register image consumed by a function prologue.
pub const IMG: u32 = 2048;
/// Linear-memory size in 64 KiB pages.
pub const PAGES: u64 = 1;

/// Cell tag: empty return slot (`None`).
pub const TAG_NONE: u32 = 0;
/// Cell tag: integer.
pub const TAG_I: u32 = 1;
/// Cell tag: float (payload = IEEE bits).
pub const TAG_F: u32 = 2;
/// Cell tag: string handle (payload = `StrHandle.0`).
pub const TAG_S: u32 = 3;
/// Cell tag: object handle (payload = `index | generation << 32`).
pub const TAG_A: u32 = 4;

/// Exit-record kind: `Ret` op.
pub const EXIT_RET: u32 = 1;
/// Exit-record kind: `Return` op (payload = `i[b]`).
pub const EXIT_RETURN: u32 = 2;
/// Exit-record kind: fell off the end of the code.
pub const EXIT_IMPLICIT: u32 = 3;

/// `zvm.trap` kind: integer division by zero.
pub const TRAP_DIV_ZERO: i32 = 1;
/// `zvm.trap` kind: `i64::MIN / -1`.
pub const TRAP_OVERFLOW: i32 = 2;
/// `zvm.trap` kind: explicit `Trap` op (`extra` = const string index).
pub const TRAP_EXPLICIT: i32 = 3;
/// `zvm.trap` kind: fuel exhausted.
pub const TRAP_FUEL: i32 = 4;
/// `zvm.trap` kind: call depth over `stack_limit`.
pub const TRAP_STACK: i32 = 5;
/// `zvm.trap` kind: `Call` to a function index that does not exist (`extra` = index).
pub const TRAP_BAD_CALL: i32 = 6;

/// Import module name.
pub const IMPORT_MODULE: &str = "zvm";

/// Imports in declaration order: name, params, results. Value types are
/// written `i32`, `i64`, `f64`.
pub const IMPORTS: &[(&str, &[&str], &[&str])] = &[
    ("trap", &["i32", "i32", "i32", "i32"], &[]),
    ("str_const", &["i32", "i32"], &["i32"]),
    ("concat", &["i32", "i32"], &["i32"]),
    ("new_obj", &["i32"], &["i64"]),
    ("is_class", &["i64", "i32"], &["i64"]),
    ("is_null", &["i64"], &["i64"]),
    ("ld_i", &["i64", "i32", "i32", "i32"], &["i64"]),
    ("ld_f", &["i64", "i32", "i32", "i32"], &["f64"]),
    ("ld_s", &["i64", "i32", "i32", "i32"], &["i32"]),
    ("ld_a", &["i64", "i32", "i32", "i32"], &["i64"]),
    ("st_i", &["i64", "i32", "i64", "i32", "i32"], &[]),
    ("st_f", &["i64", "i32", "f64", "i32", "i32"], &[]),
    ("st_s", &["i64", "i32", "i32", "i32", "i32"], &[]),
    ("st_a", &["i64", "i32", "i64", "i32", "i32"], &[]),
    ("arr_len", &["i64", "i32", "i32"], &["i64"]),
    ("arr_get_i", &["i64", "i64", "i32", "i32"], &["i64"]),
    ("arr_set_i", &["i64", "i64", "i64", "i32", "i32"], &[]),
    ("arr_push_i", &["i64", "i64", "i32", "i32"], &[]),
    ("arr_get_s", &["i64", "i64", "i32", "i32"], &["i32"]),
    ("arr_set_s", &["i64", "i64", "i32", "i32", "i32"], &[]),
    ("arr_push_s", &["i64", "i32", "i32", "i32"], &[]),
    ("native", &["i32", "i32", "i32", "i32", "i32"], &[]),
    ("call_interp", &["i32", "i32", "i32", "i32"], &[]),
    ("vcall", &["i32", "i32", "i32", "i32"], &["i32"]),
];

/// Index of an import by name (panics on a typo at codegen time only through tests).
#[must_use]
pub fn import_index(name: &str) -> u32 {
    IMPORTS
        .iter()
        .position(|(n, _, _)| *n == name)
        .map_or(u32::MAX, |i| i as u32)
}

/// Exported mutable globals.
pub const GLOBAL_FUEL: &str = "__fuel";
/// Current call depth (outermost AOT frame = 1).
pub const GLOBAL_DEPTH: &str = "__depth";
/// `ZvmConfig::stack_limit`.
pub const GLOBAL_LIMIT: &str = "__limit";
/// 1 when the last callee left through `Return` (caller `i[0]` = `__rq_val`).
pub const GLOBAL_RQ_SET: &str = "__rq_set";
/// Value a `Return` hands to the caller's `i[0]`.
pub const GLOBAL_RQ_VAL: &str = "__rq_val";
/// Exported funcref table indexed by function index.
pub const TABLE: &str = "__table";
/// Exported memory.
pub const MEMORY: &str = "memory";

/// Export name of compiled function `idx`.
#[must_use]
pub fn func_export(idx: u32) -> String {
    format!("f{idx}")
}

/// Byte layout of a function's register image at [`IMG`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageLayout {
    /// Register counts per bank (`max(n, 1)`, as the interpreter allocates).
    pub counts: [u32; 4],
    /// Byte offsets of the I, F, S, A banks from [`IMG`].
    pub offsets: [u32; 4],
    /// Total size in bytes.
    pub size: u32,
}

impl ImageLayout {
    /// Layout for `f`.
    #[must_use]
    pub fn of(f: &CompiledFunction) -> Self {
        let ni = u32::from(f.n_i.max(1));
        let nf = u32::from(f.n_f.max(1));
        let ns = u32::from(f.n_s.max(1));
        let na = u32::from(f.n_a.max(1));
        let off_f = ni * 8;
        let off_s = off_f + nf * 8;
        let off_a = (off_s + ns * 4 + 7) & !7;
        Self {
            counts: [ni, nf, ns, na],
            offsets: [0, off_f, off_s, off_a],
            size: off_a + na * 8,
        }
    }

    /// Absolute address of register `reg` of `bank` (0 = i, 1 = f, 2 = s, 3 = a).
    #[must_use]
    pub fn addr(&self, bank: usize, reg: u32) -> u32 {
        let width = if bank == 2 { 4 } else { 8 };
        IMG + self.offsets[bank] + reg * width
    }
}

/// Registers a host entry may set besides the parameters: `i[0..8]`,
/// `f[0..4]`, `s[0..4]`, `a[0..8]` (what `call_with` / `call_method_*` fill).
pub const ENTRY_WINDOW: [u32; 4] = [8, 4, 4, 8];

/// Registers a function prologue loads from the image: parameter registers
/// plus [`ENTRY_WINDOW`]. Every other register starts at zero, as in a fresh
/// interpreter frame. A host entry whose frame holds a non-zero value outside
/// this set runs on the interpreter instead.
#[must_use]
pub fn entry_mask(f: &CompiledFunction) -> [Vec<bool>; 4] {
    let lay = ImageLayout::of(f);
    let mut mask: [Vec<bool>; 4] =
        std::array::from_fn(|b| (0..lay.counts[b]).map(|r| r < ENTRY_WINDOW[b]).collect());
    for p in &f.params {
        let b = value_bank(p.bank);
        let r = usize::from(p.reg as u8);
        if let Some(slot) = mask[b].get_mut(r) {
            *slot = true;
        }
        // `write_value` puts integers into `i[reg]` whatever the declared bank.
        if let Some(slot) = mask[0].get_mut(r) {
            *slot = true;
        }
    }
    mask
}

/// Bank a `read_value`/`Arg` bank number selects: 1 = f, 2 = s, 3 = a, else i.
#[must_use]
pub fn value_bank(bank: u8) -> usize {
    match bank {
        1 => 1,
        2 => 2,
        3 => 3,
        _ => 0,
    }
}

/// Encode an object handle as a 64-bit payload.
#[must_use]
pub fn obj_bits(h: crate::heap::ObjHandle) -> u64 {
    u64::from(h.index) | (u64::from(h.generation) << 32)
}

/// Decode an object handle from a 64-bit payload.
#[must_use]
pub fn obj_from_bits(v: u64) -> crate::heap::ObjHandle {
    crate::heap::ObjHandle {
        index: v as u32,
        generation: (v >> 32) as u32,
    }
}
