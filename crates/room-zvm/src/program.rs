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

/// Absolute target of a relative jump whose extension word ends at `pc_after`.
///
/// Mirrors the interpreter: `pc_after + ext` (signed), clamped at zero. A
/// target at or past the end of the code is an implicit return.
#[must_use]
pub fn jump_target(pc_after: usize, ext: u32) -> usize {
    (pc_after as isize + ext as i32 as isize).max(0) as usize
}

/// Word index of the instruction after the one at `pc` (skips its ext word).
#[must_use]
pub fn next_insn(code: &[u32], pc: usize) -> usize {
    let (op_b, _, _, _) = crate::opcode::unpack(code[pc]);
    let ext = Op::from_u8(op_b).is_some_and(crate::opcode::has_ext);
    pc + 1 + usize::from(ext)
}

/// Per-word fuel charge (`docs/37` §4.5): block leaders hold the number of
/// instructions in their basic block, every other word holds zero.
///
/// Leaders are word 0, jump targets, and the instruction after a jump,
/// `Ret`, `Return` or `Trap`. The interpreter and the WASM tier both charge a
/// block's cost on entry, so fuel runs out at the same block on every tier.
#[must_use]
pub fn block_costs(code: &[u32]) -> Vec<u32> {
    let n = code.len();
    let mut leader = vec![false; n];
    let mut start = vec![false; n];
    if n > 0 {
        leader[0] = true;
    }
    let mut pc = 0;
    while pc < n {
        start[pc] = true;
        let (op_b, _, _, _) = crate::opcode::unpack(code[pc]);
        let next = next_insn(code, pc);
        match Op::from_u8(op_b) {
            Some(Op::Jmp | Op::JmpIf | Op::JmpIfNot) => {
                if pc + 1 < n {
                    let t = jump_target(pc + 2, code[pc + 1]);
                    if t < n {
                        leader[t] = true;
                    }
                }
                if next < n {
                    leader[next] = true;
                }
            }
            Some(Op::Ret | Op::Return | Op::Trap) if next < n => {
                leader[next] = true;
            }
            _ => {}
        }
        pc = next;
    }
    let mut costs = vec![0u32; n];
    for l in 0..n {
        if !leader[l] {
            continue;
        }
        if !start[l] {
            // A jump into an extension word: charge one so loops still burn fuel.
            costs[l] = 1;
            continue;
        }
        let mut k = 0u32;
        let mut p = l;
        loop {
            k += 1;
            p = next_insn(code, p);
            if p >= n || leader[p] {
                break;
            }
        }
        costs[l] = k;
    }
    costs
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
    /// Number of `f` script fields (`double`, vector components).
    pub n_f: u16,
    /// `Array<int>` field indices in the object bank.
    pub array_i: Vec<u16>,
    /// `Array<string>` field indices in the object bank.
    pub array_s: Vec<u16>,
    /// Object-array field indices in the object bank (`Array<Actor>`,
    /// `Array<@Line>`).
    pub array_a: Vec<u16>,
    /// Struct-typed fields (`FLineTraceData data;`): object-bank index and the
    /// struct's class id. `new` allocates them, as GZDoom stores structs inline.
    pub structs: Vec<(u16, u32)>,
    /// Field name → (bank, index). bank: 0=i 1=a 2=s 3=f (vectors: first component)
    pub fields: BTreeMap<String, (u8, u16)>,
}

/// Where a program's `PlayerInfo` keeps `mo`, for natives that build one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerInfoLayout {
    /// Class id of `PlayerInfo`.
    pub class_id: u32,
    /// Int fields.
    pub n_i: u16,
    /// Object fields.
    pub n_a: u16,
    /// String fields.
    pub n_s: u16,
    /// Float fields.
    pub n_f: u16,
    /// Object-bank slot of `mo`.
    pub mo_slot: u16,
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
    /// `action` functions (`Class.Method`, as in `func_by_name`) → default
    /// of each user parameter as text (`""` = none), for the host to fill
    /// arguments a state left out (`A_DoPBWeaponAction` with no arguments).
    pub action_defaults: BTreeMap<String, Vec<String>>,
    /// Script field values a class's `Default` block sets (custom
    /// `Property`s and flagdefs), as `(field, value text)`; the host applies
    /// them, ancestors first, to each new actor object.
    pub field_defaults: BTreeMap<String, Vec<(String, String)>>,
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

    /// Replace the bodies of library functions the host implements as
    /// playsim actions (`Weapon.A_WeaponReady`, `StateProvider.A_FireProjectile`):
    /// each passes its parameters in order to the `Host.<Method>` native,
    /// which queues the action for the host ([`crate::ZsActorOp::ActionText`]).
    /// `classes` and `methods` match case-insensitively. Returns the count.
    pub fn route_to_host_actions(&mut self, classes: &[&str], methods: &[&str]) -> usize {
        let mut n = 0;
        for fi in 0..self.functions.len() {
            let name = self.functions[fi].name.clone();
            let Some((class, method)) = name.rsplit_once('.') else {
                continue;
            };
            if !classes.iter().any(|c| c.eq_ignore_ascii_case(class))
                || !methods.iter().any(|m| m.eq_ignore_ascii_case(method))
            {
                continue;
            }
            let id = self.intern_native(&format!("Host.{method}"));
            let f = &mut self.functions[fi];
            let line = f.lines.first().copied().unwrap_or(0);
            f.code.clear();
            for p in f.params.clone() {
                f.emit(Op::Arg, p.bank, u8::try_from(p.reg).unwrap_or(0), 0);
            }
            f.emit_ext(Op::CallNat, 1, 0, 0, id);
            // The native's integer result is the function's (`GetAirTime`).
            f.emit(Op::GetRet, 0, 0, 0);
            f.emit(Op::Ret, 0, 0, 0);
            f.n_i = f.n_i.max(1);
            f.lines = vec![line; f.code.len()];
            n += 1;
        }
        if n > 0 {
            self.finalize();
        }
        n
    }

    /// Make `classes`' `methods` return at once without doing anything (the
    /// host does that work itself, such as the parts of UZDoom's
    /// `PlayerPawn.PlayerThink` Room's playsim runs natively). Returns how
    /// many bodies were replaced.
    pub fn route_to_noops(&mut self, classes: &[&str], methods: &[&str]) -> usize {
        let mut n = 0;
        for f in &mut self.functions {
            let Some((class, method)) = f.name.rsplit_once('.') else {
                continue;
            };
            if !classes.iter().any(|c| c.eq_ignore_ascii_case(class))
                || !methods.iter().any(|m| m.eq_ignore_ascii_case(method))
            {
                continue;
            }
            let line = f.lines.first().copied().unwrap_or(0);
            f.code.clear();
            f.emit(Op::Ret, 0, 0, 0);
            f.lines = vec![line; f.code.len()];
            n += 1;
        }
        if n > 0 {
            self.finalize();
        }
        n
    }

    /// The function an [`crate::Op::CallVirt`] with extension word `ext`
    /// runs for an object of `class_id`: the class table's entry for the
    /// slot (low 12 bits), else the statically resolved function (the
    /// other 20 bits) — an object of a class without a table, such as a
    /// proxy for a DECORATE actor.
    #[must_use]
    pub fn virtual_target(&self, ext: u32, class_id: u32) -> Option<u32> {
        let slot = (ext & 0xfff) as usize;
        let fallback = ext >> 12;
        self.vtables
            .get(&class_id)
            .and_then(|vt| vt.get(slot).copied())
            .filter(|&i| (i as usize) < self.functions.len())
            .or_else(|| ((fallback as usize) < self.functions.len()).then_some(fallback))
    }

    /// Look up a function by qualified name.
    #[must_use]
    pub fn func(&self, name: &str) -> Option<&CompiledFunction> {
        self.func_by_name
            .get(name)
            .and_then(|&i| self.functions.get(i as usize))
    }
}
