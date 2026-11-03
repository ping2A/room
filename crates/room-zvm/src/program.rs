//! Compiled ZVM program.

use crate::opcode::Op;
use std::collections::BTreeMap;

/// One compiled function.
#[derive(Clone, Debug, Default)]
pub struct CompiledFunction {
    /// Qualified name (`Class.method` or static).
    pub name: String,
    /// Bytecode words (instructions + extension immediates inline after ops that need them).
    pub code: Vec<u32>,
    /// Integer register count.
    pub n_i: u8,
    /// Float register count.
    pub n_f: u8,
    /// String register count.
    pub n_s: u8,
    /// Object register count.
    pub n_a: u8,
    /// Integer constant pool.
    pub const_i: Vec<i64>,
    /// Float constant pool.
    pub const_f: Vec<f64>,
    /// String constant pool.
    pub const_s: Vec<String>,
    /// Source file for traps (optional).
    pub file: String,
    /// Rough line map: code word index → source line.
    pub lines: Vec<u32>,
    /// Parameter registers for the [`crate::Op::Call`] ABI. Bank: 0=i 1=f 2=s 3=a.
    pub params: Vec<FuncParam>,
}

/// One parameter slot filled by [`crate::Op::Call`].
#[derive(Clone, Copy, Debug)]
pub struct FuncParam {
    /// 0=i, 1=f, 2=s, 3=a.
    pub bank: u8,
    /// Register index in that bank.
    pub reg: u16,
}

impl CompiledFunction {
    /// Append a packed op with no extension word.
    pub fn emit(&mut self, op: Op, a: u8, b: u8, c: u8) {
        self.code.push(crate::opcode::pack(op, a, b, c));
    }

    /// Append op + extension word.
    pub fn emit_ext(&mut self, op: Op, a: u8, b: u8, c: u8, ext: u32) {
        self.emit(op, a, b, c);
        self.code.push(ext);
    }

    /// Intern an i64 constant; returns pool index.
    pub fn intern_i(&mut self, v: i64) -> u32 {
        if let Some(i) = self.const_i.iter().position(|&x| x == v) {
            return i as u32;
        }
        self.const_i.push(v);
        (self.const_i.len() - 1) as u32
    }

    /// Intern a string constant.
    pub fn intern_s(&mut self, v: &str) -> u32 {
        if let Some(i) = self.const_s.iter().position(|x| x == v) {
            return i as u32;
        }
        self.const_s.push(v.to_owned());
        (self.const_s.len() - 1) as u32
    }

    /// Intern an f64 constant; returns pool index.
    pub fn intern_f(&mut self, v: f64) -> u32 {
        if let Some(i) = self
            .const_f
            .iter()
            .position(|&x| x.to_bits() == v.to_bits())
        {
            return i as u32;
        }
        self.const_f.push(v);
        (self.const_f.len() - 1) as u32
    }
}

/// Class layout known to the VM (script fields).
#[derive(Clone, Debug, Default)]
pub struct ClassLayout {
    /// Class name.
    pub name: String,
    /// Parent class name (empty = none).
    pub parent: String,
    /// Number of `i` script fields.
    pub n_i: u16,
    /// Number of `a` script fields.
    pub n_a: u16,
    /// Number of `s` script fields.
    pub n_s: u16,
    /// `Array<int>` field indices in the object bank.
    pub array_i: Vec<u16>,
    /// `Array<string>` field indices in the object bank.
    pub array_s: Vec<u16>,
    /// Field name → (bank, index). bank: 0=i 1=a 2=s
    pub fields: BTreeMap<String, (u8, u16)>,
}

/// A fully linked program.
#[derive(Clone, Debug, Default)]
pub struct CompiledProgram {
    /// Functions in stable order.
    pub functions: Vec<CompiledFunction>,
    /// Name → function index.
    pub func_by_name: BTreeMap<String, u32>,
    /// Class layouts.
    pub classes: Vec<ClassLayout>,
    /// Class name → index.
    pub class_by_name: BTreeMap<String, u32>,
    /// Native names referenced by [`Op::CallNative`] extension indices.
    ///
    /// Compile-time `CallNative` ext words index this table; [`crate::Zvm::new`]
    /// remaps them onto the runtime [`crate::NativeTable`] ids.
    pub native_imports: Vec<String>,
    /// ZScript `States { … }` bodies keyed by class name (ZS3 → StateTable merge).
    pub class_states: BTreeMap<String, String>,
    /// ZScript `Default { … }` bodies keyed by class name (ZS3 → ClassDef props).
    pub class_defaults: BTreeMap<String, String>,
    /// Content hash for saves (FNV-ish over names + code lens).
    pub program_hash: u64,
    /// [`crate::Op::CallVirt`] targets: class id → function index per slot.
    pub vtables: BTreeMap<u32, Vec<u32>>,
}

impl CompiledProgram {
    /// Intern a native import name; returns the compile-time id for `CallNative`.
    pub fn intern_native(&mut self, name: &str) -> u32 {
        if let Some(i) = self
            .native_imports
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
        {
            return i as u32;
        }
        self.native_imports.push(name.to_owned());
        (self.native_imports.len() - 1) as u32
    }

    /// Recompute name maps and a stable hash.
    pub fn finalize(&mut self) {
        self.func_by_name.clear();
        for (i, f) in self.functions.iter().enumerate() {
            self.func_by_name.insert(f.name.clone(), i as u32);
        }
        self.class_by_name.clear();
        for (i, c) in self.classes.iter().enumerate() {
            self.class_by_name.insert(c.name.clone(), i as u32);
        }
        let mut h: u64 = 0xcbf29ce484222325;
        for f in &self.functions {
            for b in f.name.as_bytes() {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x100000001b3);
            }
            h ^= f.code.len() as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        for n in &self.native_imports {
            for b in n.as_bytes() {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        self.program_hash = h;
    }

    /// Look up a function by qualified name.
    #[must_use]
    pub fn func(&self, name: &str) -> Option<&CompiledFunction> {
        self.func_by_name
            .get(name)
            .and_then(|&i| self.functions.get(i as usize))
    }
}
