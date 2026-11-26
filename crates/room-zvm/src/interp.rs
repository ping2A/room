//! Tier-0 register interpreter.

use crate::heap::{ObjHandle, ObjKind, StrHandle, ZvmHeap};
use crate::natives::{NativeRegs, NativeTable, ZvmCtx};
use crate::opcode::{Op, unpack};
use crate::ops::{self, HeapErr};
use crate::program::CompiledProgram;
use crate::trap::{Trap, TrapPolicy};

/// Default fuel per top-level call.
pub const DEFAULT_FUEL: u64 = 1_000_000;
/// Default recursion depth.
pub const DEFAULT_STACK_LIMIT: usize = 256;

/// VM configuration.
#[derive(Clone, Debug)]
pub struct ZvmConfig {
    /// Instruction budget per `call`.
    pub fuel: u64,
    /// Max call frames.
    pub stack_limit: usize,
    /// Trap policy.
    pub trap_policy: TrapPolicy,
}

impl Default for ZvmConfig {
    fn default() -> Self {
        Self {
            fuel: DEFAULT_FUEL,
            stack_limit: DEFAULT_STACK_LIMIT,
            trap_policy: TrapPolicy::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub(crate) func: u32,
    pub(crate) pc: usize,
    pub(crate) i: Vec<i64>,
    pub(crate) f: Vec<f64>,
    pub(crate) s: Vec<StrHandle>,
    pub(crate) a: Vec<ObjHandle>,
    /// Where to store return: (bank 0=i 1=s 2=a, reg)
    pub(crate) ret: Option<(u8, u8)>,
}

/// A copy of the VM for world snapshots (rollback): same program (shared),
/// heap, natives and registers. The AOT tier is not copied; the copy runs
/// the same code in the interpreter.
impl Clone for Zvm {
    fn clone(&self) -> Self {
        Self {
            program: std::sync::Arc::clone(&self.program),
            heap: self.heap.clone(),
            natives: self.natives.clone(),
            config: self.config.clone(),
            host: self.host,
            last_f: self.last_f,
            last_i: self.last_i,
            last_s: self.last_s.clone(),
            play: self.play.clone(),
            args: self.args.clone(),
            rets: self.rets,
            fuel_left: self.fuel_left,
            costs: self.costs.clone(),
            depth_base: self.depth_base,
            bridge_floor: self.bridge_floor,
            bridge_rq: self.bridge_rq,
            #[cfg(feature = "aot")]
            aot: None,
        }
    }
}

/// ZScript VM instance.
pub struct Zvm {
    /// Linked program, shared by copies of this VM ([`Clone`]); a change
    /// copies it first (`Arc::make_mut`).
    pub program: std::sync::Arc<CompiledProgram>,
    /// Heap.
    pub heap: ZvmHeap,
    /// Natives.
    pub natives: NativeTable,
    /// Config.
    pub config: ZvmConfig,
    /// Host cookie for natives.
    pub host: usize,
    /// `f[0]` of the last top-level return.
    pub last_f: f64,
    /// First four integer registers of the last top-level return.
    ///
    /// `out` parameters live past `i[0]`. Callers that need `newDamage` read
    /// `last_i[1]` after an instance call.
    pub last_i: [i64; 4],
    /// `s[0]` of the last top-level return, as text.
    pub last_s: String,
    /// Play-scope natives (skill, lumps, grants). Not shared across VMs.
    pub play: Box<crate::natives::ZvmPlayHost>,
    /// Argument stack for [`Op::Arg`] / [`Op::Call`].
    pub(crate) args: Vec<Value>,
    /// Return slots written by [`Op::Ret`]. Index 0 is the result.
    pub(crate) rets: [Option<Value>; 8],
    pub(crate) fuel_left: u64,
    /// Interpreter bridge from AOT code: frames at or below this depth belong to the caller.
    pub(crate) bridge_floor: usize,
    /// Set when a bridged callee left through `Return` (the caller's `i[0]`).
    pub(crate) bridge_rq: Option<i64>,
    /// Per function, per word: block fuel charge ([`crate::program::block_costs`]).
    costs: Vec<Vec<u32>>,
    /// Frames already live outside this interpreter run (AOT callers), counted
    /// against `stack_limit`.
    pub(crate) depth_base: usize,
    /// AOT tier (`docs/37` ZW3), when attached.
    #[cfg(feature = "aot")]
    pub(crate) aot: Option<Box<crate::aot::AotModule>>,
}

/// A register value on the argument / return stack (`docs/34` §14.3.3).
#[derive(Clone, Copy, Debug)]
pub enum Value {
    /// Integer bank.
    I(i64),
    /// Float bank.
    F(f64),
    /// String bank.
    S(StrHandle),
    /// Object bank.
    A(ObjHandle),
}

impl Zvm {
    /// Construct with a program and builtin natives.
    ///
    /// Remaps compile-time [`CompiledProgram::native_imports`] indices onto the
    /// runtime native table before the first call.
    #[must_use]
    pub fn new(program: CompiledProgram) -> Self {
        Self::linking(program, false)
    }

    /// Like [`new`](Self::new), and no-op stubs for imports the builtin table lacks.
    ///
    /// Used when loading the UZDoom stdlib, whose functions read engine globals
    /// before they reach `LineAttack`.
    #[must_use]
    pub fn new_with_stubs(program: CompiledProgram) -> Self {
        Self::linking(program, true)
    }

    fn linking(mut program: CompiledProgram, stub_missing: bool) -> Self {
        let mut natives = NativeTable::with_builtins();
        inherit_native_imports(&mut program, &natives);
        if stub_missing {
            natives.bind_imports(&program.native_imports);
        }
        remap_native_imports(&mut program, &natives);
        let object_layouts = program
            .classes
            .iter()
            .enumerate()
            .map(|(class_id, class)| {
                (
                    class.name.to_ascii_lowercase(),
                    (class_id as u32, class.n_i, class.n_a, class.n_s),
                )
            })
            .collect();
        let play = crate::natives::ZvmPlayHost {
            object_layouts,
            ..Default::default()
        };
        Self {
            program: std::sync::Arc::new(program),
            heap: ZvmHeap::new(),
            natives,
            config: ZvmConfig::default(),
            host: 0,
            last_f: 0.0,
            last_i: [0; 4],
            last_s: String::new(),
            play: Box::new(play),
            args: Vec::new(),
            rets: [None; 8],
            fuel_left: 0,
            costs: Vec::new(),
            depth_base: 0,
            bridge_floor: 0,
            bridge_rq: None,
            #[cfg(feature = "aot")]
            aot: None,
        }
    }

    /// Placeholder left in place while the AOT tier holds the VM.
    #[cfg(feature = "aot")]
    pub(crate) fn empty() -> Self {
        Self {
            program: std::sync::Arc::default(),
            heap: ZvmHeap::new(),
            natives: NativeTable::default(),
            config: ZvmConfig::default(),
            host: 0,
            last_f: 0.0,
            last_i: [0; 4],
            last_s: String::new(),
            play: Box::default(),
            args: Vec::new(),
            rets: [None; 8],
            fuel_left: 0,
            costs: Vec::new(),
            depth_base: 0,
            bridge_floor: 0,
            bridge_rq: None,
            aot: None,
        }
    }

    /// Call a function and return the float that was in `f[0]` at the top-level return.
    pub fn call_f64(&mut self, name: &str) -> Result<f64, Trap> {
        self.call(name)?;
        Ok(self.last_f)
    }

    /// Call a function by name with no args; returns `i[0]` / return value.
    pub fn call(&mut self, name: &str) -> Result<i64, Trap> {
        self.call_with(name, &[], &[])
    }

    /// Call with integer / string arguments placed in `i[0..]` / `s[0..]`.
    pub fn call_with(&mut self, name: &str, i_args: &[i64], s_args: &[&str]) -> Result<i64, Trap> {
        let idx = lookup_func(&self.program, name)?;
        self.fuel_left = self.config.fuel;
        let func = &self.program.functions[idx as usize];
        let mut frame = Frame {
            func: idx,
            pc: 0,
            i: vec![0; func.n_i.max(1) as usize],
            f: vec![0.0; func.n_f.max(1) as usize],
            s: vec![StrHandle::EMPTY; func.n_s.max(1) as usize],
            a: vec![ObjHandle::NULL; func.n_a.max(1) as usize],
            ret: None,
        };
        for (i, v) in i_args.iter().enumerate() {
            if let Some(slot) = frame.i.get_mut(i) {
                *slot = *v;
            }
        }
        for (i, s) in s_args.iter().enumerate() {
            let h = self.heap.intern_string(s);
            if let Some(slot) = frame.s.get_mut(i) {
                *slot = h;
            }
        }
        // Also mirror string handles into i banks when the method only has
        // string params (ZandroCompat.LumpOpen) — params are allocated as
        // locals; the compiler places the first string param in s[0] and the
        // matching local name shares that bank via MoveS at prologue.
        let mut stack = vec![frame];
        let result = self.run_with_result(&mut stack)?;
        Ok(result)
    }

    /// Push a frame for [`Op::Call`] / [`Op::CallVirt`], filling `params` from `args`.
    pub(crate) fn enter_call(
        &mut self,
        stack: &mut Vec<Frame>,
        func_idx: u32,
        args: Vec<Value>,
    ) -> Result<(), Trap> {
        let callee = self
            .program
            .functions
            .get(func_idx as usize)
            .ok_or_else(|| Trap::Native(format!("Call bad func index {func_idx}")))?;
        // Default arguments are often omitted (`A_FaceTarget()`). Missing ones stay 0.
        let params = callee.params.clone();
        // ZScript default arguments are often omitted at the call
        // (`A_FaceTarget()` has six defaults). Missing ones stay 0.
        let mut nf = Frame {
            func: func_idx,
            pc: 0,
            i: vec![0; callee.n_i.max(1) as usize],
            f: vec![0.0; callee.n_f.max(1) as usize],
            s: vec![StrHandle::EMPTY; callee.n_s.max(1) as usize],
            a: vec![ObjHandle::NULL; callee.n_a.max(1) as usize],
            ret: None,
        };
        for (p, v) in params.iter().zip(args) {
            write_value(&mut nf, p.bank, p.reg as u8, v);
        }
        stack.push(nf);
        Ok(())
    }

    /// True when an AOT module is attached (enabled or not).
    #[must_use]
    pub fn has_aot(&self) -> bool {
        #[cfg(feature = "aot")]
        {
            self.aot.is_some()
        }
        #[cfg(not(feature = "aot"))]
        {
            false
        }
    }

    /// Fuel left after the last top-level call.
    #[must_use]
    pub fn fuel_left(&self) -> u64 {
        self.fuel_left
    }

    /// Allocate an instance of script class `name` exactly as `new(...)` does
    /// (array fields pre-allocated). `None` when the class is unknown.
    pub fn new_object(&mut self, name: &str) -> Option<ObjHandle> {
        let class = self
            .program
            .classes
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))?;
        Some(ops::new_obj(&mut self.heap, &self.program, class as u32))
    }

    /// Like [`Self::new_object`] for a class index (`program.classes`).
    pub fn new_object_of(&mut self, class: u32) -> ObjHandle {
        ops::new_obj(&mut self.heap, &self.program, class)
    }

    /// Remap helper exposed for tests that swap `program` after construction.
    pub fn rebind_natives(&mut self) {
        remap_native_imports(std::sync::Arc::make_mut(&mut self.program), &self.natives);
    }

    /// Call an instance method with `self` placed in `a[0]`.
    pub fn call_method_self(&mut self, name: &str, self_h: ObjHandle) -> Result<i64, Trap> {
        self.call_method_self_args(name, self_h, &[])
    }

    /// Call an instance method with `self` in `a[0]` and extra object args in `a[1..]`.
    pub fn call_method_self_args(
        &mut self,
        name: &str,
        self_h: ObjHandle,
        obj_args: &[ObjHandle],
    ) -> Result<i64, Trap> {
        let idx = lookup_func(&self.program, name)?;
        // Compiled methods name their parameter registers: other locals
        // (array fields, global arrays) can be allocated ahead of them, so
        // `a[1]` is not always the first object parameter.
        if !self.program.functions[idx as usize].params.is_empty() {
            return self.call_method_banks(name, self_h, &[], &[], &[], obj_args);
        }
        self.fuel_left = self.config.fuel;
        let func = &self.program.functions[idx as usize];
        let mut frame = Frame {
            func: idx,
            pc: 0,
            i: vec![0; func.n_i.max(1) as usize],
            f: vec![0.0; func.n_f.max(1) as usize],
            s: vec![StrHandle::EMPTY; func.n_s.max(1) as usize],
            a: vec![ObjHandle::NULL; func.n_a.max(1) as usize],
            ret: None,
        };
        if let Some(slot) = frame.a.get_mut(0) {
            *slot = self_h;
        }
        for (i, h) in obj_args.iter().enumerate() {
            if let Some(slot) = frame.a.get_mut(i + 1) {
                *slot = *h;
            }
        }
        let mut stack = vec![frame];
        self.run_with_result(&mut stack)
    }

    /// Instance call with integer, string, and extra object arguments.
    ///
    /// `self` is `a[0]`. `obj_args` fill `a[1..]`. Integers fill `i[0..]` and
    /// strings fill `s[0..]`, matching the compiler's per-bank parameters.
    /// After the call, [`Self::last_i`] holds the callee's integer registers.
    pub fn call_method_full(
        &mut self,
        name: &str,
        self_h: ObjHandle,
        i_args: &[i64],
        s_args: &[&str],
        obj_args: &[ObjHandle],
    ) -> Result<i64, Trap> {
        let idx = lookup_func(&self.program, name)?;
        if !self.program.functions[idx as usize].params.is_empty() {
            return self.call_method_banks(name, self_h, i_args, &[], s_args, obj_args);
        }
        self.fuel_left = self.config.fuel;
        let func = &self.program.functions[idx as usize];
        let mut frame = Frame {
            func: idx,
            pc: 0,
            i: vec![0; func.n_i.max(1) as usize],
            f: vec![0.0; func.n_f.max(1) as usize],
            s: vec![StrHandle::EMPTY; func.n_s.max(1) as usize],
            a: vec![ObjHandle::NULL; func.n_a.max(1) as usize],
            ret: None,
        };
        if let Some(slot) = frame.a.get_mut(0) {
            *slot = self_h;
        }
        for (i, h) in obj_args.iter().enumerate() {
            if let Some(slot) = frame.a.get_mut(i + 1) {
                *slot = *h;
            }
        }
        for (i, v) in i_args.iter().enumerate() {
            if let Some(slot) = frame.i.get_mut(i) {
                *slot = *v;
            }
        }
        for (i, s) in s_args.iter().enumerate() {
            let h = self.heap.intern_string(s);
            if let Some(slot) = frame.s.get_mut(i) {
                *slot = h;
            }
        }
        let mut stack = vec![frame];
        self.run_with_result(&mut stack)
    }

    /// Instance call with every bank: `self` in `a[0]`, then `i`, `f`, `s` and
    /// further object arguments in declaration order per bank.
    pub fn call_method_banks(
        &mut self,
        name: &str,
        self_h: ObjHandle,
        i_args: &[i64],
        f_args: &[f64],
        s_args: &[&str],
        obj_args: &[ObjHandle],
    ) -> Result<i64, Trap> {
        let idx = lookup_func(&self.program, name)?;
        self.fuel_left = self.config.fuel;
        let func = &self.program.functions[idx as usize];
        let mut frame = Frame {
            func: idx,
            pc: 0,
            i: vec![0; func.n_i.max(1) as usize],
            f: vec![0.0; func.n_f.max(1) as usize],
            s: vec![StrHandle::EMPTY; func.n_s.max(1) as usize],
            a: vec![ObjHandle::NULL; func.n_a.max(1) as usize],
            ret: None,
        };
        // Arguments go to the declared parameter registers, in order per bank.
        let params = func.params.clone();
        let (mut ni, mut nf, mut ns, mut na) = (0usize, 0usize, 0usize, 0usize);
        let mut self_placed = false;
        for p in &params {
            let reg = p.reg as usize;
            match p.bank {
                3 if !self_placed => {
                    self_placed = true;
                    if let Some(slot) = frame.a.get_mut(reg) {
                        *slot = self_h;
                    }
                }
                3 => {
                    if let (Some(slot), Some(v)) = (frame.a.get_mut(reg), obj_args.get(na)) {
                        *slot = *v;
                    }
                    na += 1;
                }
                1 => {
                    if let (Some(slot), Some(v)) = (frame.f.get_mut(reg), f_args.get(nf)) {
                        *slot = *v;
                    }
                    nf += 1;
                }
                2 => {
                    if let Some(v) = s_args.get(ns) {
                        let h = self.heap.intern_string(v);
                        if let Some(slot) = frame.s.get_mut(reg) {
                            *slot = h;
                        }
                    }
                    ns += 1;
                }
                _ => {
                    if let (Some(slot), Some(v)) = (frame.i.get_mut(reg), i_args.get(ni)) {
                        *slot = *v;
                    }
                    ni += 1;
                }
            }
        }
        if !self_placed {
            if let Some(slot) = frame.a.get_mut(0) {
                *slot = self_h;
            }
        }
        let mut stack = vec![frame];
        self.run_with_result(&mut stack)
    }

    /// Run until the stack unwinds; returns the top-level integer result.
    fn run_with_result(&mut self, stack: &mut Vec<Frame>) -> Result<i64, Trap> {
        #[cfg(feature = "aot")]
        if self.bridge_floor == 0 && stack.len() == 1 {
            if let Some(r) = self.try_aot(&stack[0]) {
                return r;
            }
        }
        self.run_frames(stack)
    }

    /// Interpreter loop until the stack unwinds to `bridge_floor`.
    pub(crate) fn run_frames(&mut self, stack: &mut Vec<Frame>) -> Result<i64, Trap> {
        let mut result = 0i64;
        while stack.len() > self.bridge_floor {
            if stack.len() + self.depth_base > self.config.stack_limit {
                let name = self.fname(stack[stack.len() - 1].func);
                return Err(Trap::StackOverflow { func: name });
            }
            let top = stack.len() - 1;
            let func_i = stack[top].func as usize;
            let pc = stack[top].pc;
            let code = &self.program.functions[func_i].code;
            if pc >= code.len() {
                // Implicit return.
                let ret_i = stack[top].i.first().copied().unwrap_or(0);
                if stack.len() == 1 {
                    self.note_last_i(&stack[top]);
                }
                stack.pop();
                if let Some(prev) = stack.last_mut() {
                    if let Some((bank, reg)) = prev.ret.take() {
                        if bank == 0 {
                            if let Some(slot) = prev.i.get_mut(reg as usize) {
                                *slot = ret_i;
                            }
                        }
                    }
                } else {
                    result = ret_i;
                }
                continue;
            }
            let cost = u64::from(self.block_cost(func_i, pc));
            if cost > 0 {
                if self.fuel_left < cost {
                    return Err(Trap::FuelExhausted {
                        func: self.fname(stack[top].func),
                    });
                }
                self.fuel_left -= cost;
            }

            let word = self.program.functions[func_i].code[pc];
            let (op_b, a, b, c) = unpack(word);
            let Some(op) = Op::from_u8(op_b) else {
                return Err(Trap::BadOpcode {
                    func: self.fname(stack[top].func),
                    op: op_b,
                });
            };
            let line = self.program.functions[func_i]
                .lines
                .get(pc)
                .copied()
                .unwrap_or(0);
            let fname = self.fname(stack[top].func);

            // Advance past the op word; ext consumed below.
            stack[top].pc = pc + 1;

            match op {
                Op::Nop => {}
                Op::MoveI => {
                    let v = stack[top].i.get(b as usize).copied().unwrap_or(0);
                    set_i(&mut stack[top], a, v);
                }
                Op::MoveF => {
                    let v = stack[top].f.get(b as usize).copied().unwrap_or(0.0);
                    set_f(&mut stack[top], a, v);
                }
                Op::MoveS => {
                    let v = stack[top].s.get(b as usize).copied().unwrap_or_default();
                    set_s(&mut stack[top], a, v);
                }
                Op::MoveA => {
                    let v = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    set_a(&mut stack[top], a, v);
                }
                Op::LoadConstI => {
                    let ext = self.take_ext(stack, func_i)?;
                    let v = self.program.functions[func_i]
                        .const_i
                        .get(ext as usize)
                        .copied()
                        .unwrap_or(0);
                    set_i(&mut stack[top], a, v);
                }
                Op::LoadConstF => {
                    let ext = self.take_ext(stack, func_i)?;
                    let v = self.program.functions[func_i]
                        .const_f
                        .get(ext as usize)
                        .copied()
                        .unwrap_or(0.0);
                    set_f(&mut stack[top], a, v);
                }
                Op::LoadConstS => {
                    let ext = self.take_ext(stack, func_i)?;
                    let lit = self.program.functions[func_i]
                        .const_s
                        .get(ext as usize)
                        .cloned()
                        .unwrap_or_default();
                    let h = self.heap.intern_string(&lit);
                    set_s(&mut stack[top], a, h);
                }
                Op::LoadNull => set_a(&mut stack[top], a, ObjHandle::NULL),
                Op::AddI
                | Op::SubI
                | Op::MulI
                | Op::DivI
                | Op::ModI
                | Op::LtI
                | Op::LeI
                | Op::EqI
                | Op::NeI
                | Op::GtI
                | Op::GeI => {
                    let lb = stack[top].i.get(b as usize).copied().unwrap_or(0);
                    let rc = stack[top].i.get(c as usize).copied().unwrap_or(0);
                    let v = match op {
                        Op::AddI => lb.wrapping_add(rc),
                        Op::SubI => lb.wrapping_sub(rc),
                        Op::MulI => lb.wrapping_mul(rc),
                        Op::DivI | Op::ModI => {
                            if rc == 0 {
                                return Err(Trap::DivZero { func: fname, line });
                            }
                            if lb == i64::MIN && rc == -1 {
                                return Err(Trap::Overflow { func: fname, line });
                            }
                            if op == Op::DivI { lb / rc } else { lb % rc }
                        }
                        Op::LtI => i64::from(lb < rc),
                        Op::LeI => i64::from(lb <= rc),
                        Op::EqI => i64::from(lb == rc),
                        Op::NeI => i64::from(lb != rc),
                        Op::GtI => i64::from(lb > rc),
                        Op::GeI => i64::from(lb >= rc),
                        _ => 0,
                    };
                    set_i(&mut stack[top], a, v);
                }
                Op::I2F => {
                    let v = stack[top].i.get(b as usize).copied().unwrap_or(0) as f64;
                    set_f(&mut stack[top], a, v);
                }
                Op::F2I => {
                    let v = stack[top].f.get(b as usize).copied().unwrap_or(0.0) as i64;
                    set_i(&mut stack[top], a, v);
                }
                Op::CmpF => {
                    let lb = stack[top].f.get(b as usize).copied().unwrap_or(0.0);
                    let rc = stack[top].f.get(c as usize).copied().unwrap_or(0.0);
                    // Match ZScript's three-way operator: NaN compares as equal
                    // because neither ordered comparison succeeds.
                    set_i(
                        &mut stack[top],
                        a,
                        if lb < rc {
                            -1
                        } else if lb > rc {
                            1
                        } else {
                            0
                        },
                    );
                }
                Op::AddF | Op::SubF | Op::MulF | Op::DivF => {
                    let lb = stack[top].f.get(b as usize).copied().unwrap_or(0.0);
                    let rc = stack[top].f.get(c as usize).copied().unwrap_or(0.0);
                    let v = match op {
                        Op::AddF => lb + rc,
                        Op::SubF => lb - rc,
                        Op::MulF => lb * rc,
                        Op::DivF => {
                            if rc == 0.0 {
                                return Err(Trap::DivZero { func: fname, line });
                            }
                            lb / rc
                        }
                        _ => 0.0,
                    };
                    set_f(&mut stack[top], a, v);
                }
                Op::Jmp => {
                    let ext = self.take_ext(stack, func_i)? as i32 as isize;
                    let new_pc = stack[top].pc as isize + ext;
                    stack[top].pc = new_pc.max(0) as usize;
                }
                Op::JmpIf | Op::JmpIfNot => {
                    let ext = self.take_ext(stack, func_i)? as i32 as isize;
                    let cond = stack[top].i.get(a as usize).copied().unwrap_or(0) != 0;
                    let take = if op == Op::JmpIf { cond } else { !cond };
                    if take {
                        let new_pc = stack[top].pc as isize + ext;
                        stack[top].pc = new_pc.max(0) as usize;
                    }
                }
                Op::Arg => {
                    let v = read_value(&stack[top], a, b);
                    self.args.push(v);
                }
                Op::Call => {
                    let func_idx = self.take_ext(stack, func_i)?;
                    self.rets = [None; 8];
                    let args = std::mem::take(&mut self.args);
                    self.enter_call(stack, func_idx, args)?;
                }
                Op::CallVirt => {
                    let slot = self.take_ext(stack, func_i)? as usize;
                    self.rets = [None; 8];
                    let args = std::mem::take(&mut self.args);
                    let recv = args.iter().find_map(|v| match v {
                        Value::A(h) => Some(*h),
                        _ => None,
                    });
                    let class_id = recv
                        .and_then(|h| match self.heap.get(h) {
                            Some(ObjKind::Object { class_id, .. }) => Some(*class_id),
                            _ => None,
                        })
                        .unwrap_or(0);
                    let func_idx = self
                        .program
                        .vtables
                        .get(&class_id)
                        .and_then(|vt| vt.get(slot).copied())
                        .ok_or_else(|| {
                            Trap::Native(format!(
                                "CallVirt class {class_id} slot {slot} is unbound"
                            ))
                        })?;
                    self.enter_call(stack, func_idx, args)?;
                }
                Op::CallNat => {
                    let ext = self.take_ext(stack, func_i)?;
                    let pushed = std::mem::take(&mut self.args);
                    self.native_call(ext, pushed, b)?;
                }
                Op::SetRet => {
                    // Extra results of `return a, b;` before the final `Ret`.
                    let v = read_value(&stack[top], a, b);
                    if (c as usize) < self.rets.len() {
                        self.rets[c as usize] = Some(v);
                    }
                }
                Op::Ret => {
                    let v = read_value(&stack[top], a, b);
                    if (c as usize) < self.rets.len() {
                        self.rets[c as usize] = Some(v);
                    }
                    if stack.len() == 1 {
                        self.note_last_i(&stack[top]);
                    }
                    stack.pop();
                    if stack.is_empty() {
                        match v {
                            Value::I(x) => result = x,
                            Value::F(x) => self.last_f = x,
                            Value::S(h) => self.last_s = self.heap.str(h).to_owned(),
                            _ => {}
                        }
                    }
                }
                Op::GetRet => {
                    let v = self.rets.get(c as usize).copied().flatten();
                    if let Some(frame) = stack.last_mut() {
                        write_value(frame, a, b, v.unwrap_or(Value::I(0)));
                    }
                }
                Op::CallStatic => {
                    let raw_ext = self.take_ext(stack, func_i)?;
                    let object_ret = raw_ext & 0x4000_0000 != 0;
                    let ext = raw_ext & !0x4000_0000;
                    // Unresolved compile-time placeholder: `0x8000_0000 | const_s_idx`.
                    let func_idx = if ext & 0x8000_0000 != 0 {
                        let sidx = (ext & 0x3fff_ffff) as usize;
                        let name = self
                            .program
                            .functions
                            .get(func_i)
                            .and_then(|f| f.const_s.get(sidx).cloned())
                            .unwrap_or_default();
                        let caller = self
                            .program
                            .functions
                            .get(func_i)
                            .map(|f| f.name.clone())
                            .unwrap_or_default();
                        let prefix = caller
                            .rsplit_once('.')
                            .map(|(c, _)| c.to_owned())
                            .unwrap_or_default();
                        let qualified = format!("{prefix}.{name}");
                        self.program
                            .func_by_name
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(&name))
                            .or_else(|| {
                                self.program
                                    .func_by_name
                                    .iter()
                                    .find(|(k, _)| k.eq_ignore_ascii_case(&qualified))
                            })
                            .or_else(|| {
                                self.program.func_by_name.iter().find(|(k, _)| {
                                    k.rsplit_once('.')
                                        .is_some_and(|(_, m)| m.eq_ignore_ascii_case(&name))
                                })
                            })
                            .map(|(_, &i)| i)
                            .ok_or_else(|| {
                                Trap::Native(format!("unresolved CallStatic '{name}'"))
                            })?
                    } else {
                        ext
                    };
                    let callee =
                        self.program
                            .functions
                            .get(func_idx as usize)
                            .ok_or_else(|| {
                                Trap::Native(format!("CallStatic bad func index {func_idx}"))
                            })?;
                    let mut nf = Frame {
                        func: func_idx,
                        pc: 0,
                        i: vec![0; callee.n_i.max(1) as usize],
                        f: vec![0.0; callee.n_f.max(1) as usize],
                        s: vec![StrHandle::EMPTY; callee.n_s.max(1) as usize],
                        a: vec![ObjHandle::NULL; callee.n_a.max(1) as usize],
                        ret: Some((0, a)),
                    };
                    // Copy up to `b` ints from caller starting at reg `c` as a simple ABI.
                    let n_args = (b & 0x1f) as usize;
                    let copy_self = b & 0x80 != 0;
                    let copy_s = b & 0x40 != 0;
                    let string_ret = b & 0x20 != 0;
                    if string_ret {
                        nf.ret = Some((1, a));
                    } else if object_ret {
                        nf.ret = Some((2, a));
                    }
                    let self_h = stack[top].a.first().copied().unwrap_or_default();
                    let s0 = stack[top].s.first().copied().unwrap_or_default();
                    for i in 0..n_args {
                        let src = c as usize + i;
                        let v = stack[top].i.get(src).copied().unwrap_or(0);
                        if let Some(slot) = nf.i.get_mut(i) {
                            *slot = v;
                        }
                    }
                    if copy_self {
                        if let Some(slot) = nf.a.first_mut() {
                            *slot = self_h;
                        }
                    }
                    if copy_s {
                        if let Some(slot) = nf.s.first_mut() {
                            *slot = s0;
                        }
                    }
                    stack.push(nf);
                }
                Op::CallNative => {
                    let ext = self.take_ext(stack, func_i)?;
                    let f = self
                        .natives
                        .get(ext)
                        .ok_or_else(|| Trap::Native(format!("unknown native id {ext}")))?;
                    let frame = &mut stack[top];
                    let mut regs = NativeRegs {
                        i: &mut frame.i,
                        f: &mut frame.f,
                        s: &mut frame.s,
                        a: &mut frame.a,
                    };
                    let host = self.host;
                    let mut ctx = ZvmCtx {
                        heap: &mut self.heap,
                        host,
                        play: &mut self.play,
                    };
                    f(&mut ctx, &mut regs)?;
                }
                Op::Return => {
                    let ret_i = stack[top].i.get(b as usize).copied().unwrap_or(0);
                    let ret_s = stack[top].s.first().copied().unwrap_or_default();
                    let ret_a = stack[top].a.first().copied().unwrap_or_default();
                    let ret_f = stack[top].f.first().copied().unwrap_or(0.0);
                    if stack.len() == 1 {
                        self.note_last_i(&stack[top]);
                    }
                    // Destination was recorded on this frame by CallStatic.
                    let dest = stack[top].ret.take();
                    stack.pop();
                    if self.bridge_floor > 0 && stack.len() == self.bridge_floor && dest.is_none() {
                        self.bridge_rq = Some(ret_i);
                    }
                    if let Some(prev) = stack.last_mut() {
                        if let Some((bank, reg)) = dest {
                            if bank == 0 {
                                set_i(prev, reg, ret_i);
                            } else if bank == 1 {
                                set_s(prev, reg, ret_s);
                            } else if bank == 2 {
                                set_a(prev, reg, ret_a);
                            }
                        } else {
                            set_i(prev, 0, ret_i);
                        }
                    } else {
                        result = ret_i;
                        self.last_f = ret_f;
                    }
                }
                Op::IsClass => {
                    let ext = self.take_ext(stack, func_i)?;
                    let h = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    let ok = ops::is_class(&self.heap, &self.program, h, ext);
                    set_i(&mut stack[top], a, i64::from(ok));
                }
                Op::NewObj => {
                    let ext = self.take_ext(stack, func_i)?;
                    let h = ops::new_obj(&mut self.heap, &self.program, ext);
                    set_a(&mut stack[top], a, h);
                }
                Op::LoadFieldI => {
                    let ext = self.take_ext(stack, func_i)? as usize;
                    let h = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    let v = ops::ld_i(&self.heap, h, ext).map_err(|e| heap_trap(e, fname, line))?;
                    set_i(&mut stack[top], a, v);
                }
                Op::StoreFieldI => {
                    let ext = self.take_ext(stack, func_i)? as usize;
                    let h = stack[top].a.get(a as usize).copied().unwrap_or_default();
                    let v = stack[top].i.get(b as usize).copied().unwrap_or(0);
                    ops::st_i(&mut self.heap, h, ext, v).map_err(|e| heap_trap(e, fname, line))?;
                }
                Op::LoadFieldA => {
                    let ext = self.take_ext(stack, func_i)? as usize;
                    let h = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    let v = ops::ld_a(&self.heap, h, ext).map_err(|e| heap_trap(e, fname, line))?;
                    set_a(&mut stack[top], a, v);
                }
                Op::StoreFieldA => {
                    let ext = self.take_ext(stack, func_i)? as usize;
                    let h = stack[top].a.get(a as usize).copied().unwrap_or_default();
                    let v = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    ops::st_a(&mut self.heap, h, ext, v).map_err(|e| heap_trap(e, fname, line))?;
                }
                Op::LoadFieldS => {
                    let ext = self.take_ext(stack, func_i)? as usize;
                    let h = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    let v = ops::ld_s(&self.heap, h, ext).map_err(|e| heap_trap(e, fname, line))?;
                    set_s(&mut stack[top], a, v);
                }
                Op::StoreFieldS => {
                    let ext = self.take_ext(stack, func_i)? as usize;
                    let h = stack[top].a.get(a as usize).copied().unwrap_or_default();
                    let v = stack[top].s.get(b as usize).copied().unwrap_or_default();
                    ops::st_s(&mut self.heap, h, ext, v).map_err(|e| heap_trap(e, fname, line))?;
                }
                Op::LoadFieldF => {
                    let ext = self.take_ext(stack, func_i)? as usize;
                    let h = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    let v = ops::ld_f(&self.heap, h, ext).map_err(|e| heap_trap(e, fname, line))?;
                    set_f(&mut stack[top], a, v);
                }
                Op::StoreFieldF => {
                    let ext = self.take_ext(stack, func_i)? as usize;
                    let h = stack[top].a.get(a as usize).copied().unwrap_or_default();
                    let v = stack[top].f.get(b as usize).copied().unwrap_or(0.0);
                    ops::st_f(&mut self.heap, h, ext, v).map_err(|e| heap_trap(e, fname, line))?;
                }
                Op::ArrayLen => {
                    let h = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    let n = ops::arr_len(&self.heap, h).map_err(|e| heap_trap(e, fname, line))?;
                    set_i(&mut stack[top], a, n);
                }
                Op::ArrayGetI => {
                    let h = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    let idx = stack[top].i.get(c as usize).copied().unwrap_or(0);
                    let v = ops::arr_get_i(&self.heap, h, idx)
                        .map_err(|e| heap_trap(e, fname, line))?;
                    set_i(&mut stack[top], a, v);
                }
                Op::ArraySetI => {
                    let h = stack[top].a.get(a as usize).copied().unwrap_or_default();
                    let idx = stack[top].i.get(b as usize).copied().unwrap_or(0);
                    let v = stack[top].i.get(c as usize).copied().unwrap_or(0);
                    ops::arr_set_i(&mut self.heap, h, idx, v)
                        .map_err(|e| heap_trap(e, fname, line))?;
                }
                Op::ArrayPushI => {
                    let h = stack[top].a.get(a as usize).copied().unwrap_or_default();
                    let v = stack[top].i.get(b as usize).copied().unwrap_or(0);
                    ops::arr_push_i(&mut self.heap, h, v).map_err(|e| heap_trap(e, fname, line))?;
                }
                Op::ArraySetS => {
                    let h = stack[top].a.get(a as usize).copied().unwrap_or_default();
                    let idx = stack[top].i.get(b as usize).copied().unwrap_or(0);
                    let v = stack[top].s.get(c as usize).copied().unwrap_or_default();
                    ops::arr_set_s(&mut self.heap, h, idx, v)
                        .map_err(|e| heap_trap(e, fname, line))?;
                }
                Op::ArrayGetS => {
                    let h = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    let idx = stack[top].i.get(c as usize).copied().unwrap_or(0);
                    let v = ops::arr_get_s(&self.heap, h, idx)
                        .map_err(|e| heap_trap(e, fname, line))?;
                    set_s(&mut stack[top], a, v);
                }
                Op::ArrayPushS => {
                    let h = stack[top].a.get(a as usize).copied().unwrap_or_default();
                    let v = stack[top].s.get(b as usize).copied().unwrap_or_default();
                    ops::arr_push_s(&mut self.heap, h, v).map_err(|e| heap_trap(e, fname, line))?;
                }
                Op::ConcatS => {
                    let lb = stack[top].s.get(b as usize).copied().unwrap_or_default();
                    let rc = stack[top].s.get(c as usize).copied().unwrap_or_default();
                    let h = self.heap.concat(lb, rc);
                    set_s(&mut stack[top], a, h);
                }
                Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr => {
                    let lb = stack[top].i.get(b as usize).copied().unwrap_or(0);
                    let rc = stack[top].i.get(c as usize).copied().unwrap_or(0);
                    let sh = (rc as u32) & 63;
                    let v = match op {
                        Op::BitAnd => lb & rc,
                        Op::BitOr => lb | rc,
                        Op::BitXor => lb ^ rc,
                        Op::Shl => lb << sh,
                        Op::Shr => lb >> sh,
                        _ => 0,
                    };
                    set_i(&mut stack[top], a, v);
                }
                Op::IsNull => {
                    let h = stack[top].a.get(b as usize).copied().unwrap_or_default();
                    let null = ops::is_null(&self.heap, h);
                    set_i(&mut stack[top], a, i64::from(null));
                }
                Op::Trap => {
                    let ext = self.take_ext(stack, func_i)?;
                    let msg = self.program.functions[func_i]
                        .const_s
                        .get(ext as usize)
                        .cloned()
                        .unwrap_or_else(|| "trap".into());
                    return Err(Trap::Explicit {
                        msg,
                        func: fname,
                        line,
                    });
                }
            }
        }
        Ok(result)
    }

    /// Run native `ext` on `pushed` arguments and fill the return slots.
    ///
    /// `b` is the result bank (0 = i, 1 = f, 2 = s, 3 = a, 4 = vector). Shared
    /// by the interpreter's `CallNat` and the AOT tier's `zvm.native` import.
    pub(crate) fn native_call(&mut self, ext: u32, pushed: Vec<Value>, b: u8) -> Result<(), Trap> {
        let f = self
            .natives
            .get(ext)
            .ok_or_else(|| Trap::Native(format!("unknown native id {ext}")))?;
        if let Some(name) = self.natives.name(ext) {
            self.play.native_name.clear();
            self.play.native_name.push_str(name);
        }
        // At least 8 / 4 / 4 / 4 slots (natives read missing
        // arguments as zero); more when a `vararg` call pushes more.
        let mut i: Vec<i64> = Vec::with_capacity(8);
        let mut fv: Vec<f64> = Vec::with_capacity(4);
        let mut s: Vec<StrHandle> = Vec::with_capacity(4);
        let mut ao: Vec<ObjHandle> = Vec::with_capacity(4);
        for v in pushed {
            match v {
                Value::I(x) => i.push(x),
                Value::F(x) => fv.push(x),
                Value::S(h) => s.push(h),
                Value::A(h) => ao.push(h),
            }
        }
        self.play.native_arg_counts = [i.len(), fv.len(), s.len(), ao.len()];
        i.resize(i.len().max(8), 0);
        fv.resize(fv.len().max(4), 0.0);
        s.resize(s.len().max(4), StrHandle::EMPTY);
        ao.resize(ao.len().max(4), ObjHandle::NULL);
        let mut regs = NativeRegs {
            i: &mut i,
            f: &mut fv,
            s: &mut s,
            a: &mut ao,
        };
        let host = self.host;
        let mut ctx = ZvmCtx {
            heap: &mut self.heap,
            host,
            play: &mut self.play,
        };
        ctx.play.multi_ret.clear();
        f(&mut ctx, &mut regs)?;
        self.rets = [None; 8];
        // `b == 4` — vector result in `f[0..4]`.
        if b == 4 {
            for (k, v) in fv.iter().take(4).enumerate() {
                self.rets[k] = Some(Value::F(*v));
            }
            return Ok(());
        }
        // `b == 2` — string natives write the result into `s[1]`.
        self.rets[0] = if b == 2 {
            Some(Value::S(s.get(1).copied().unwrap_or_default()))
        } else if b == 3 {
            Some(Value::A(ao.first().copied().unwrap_or_default()))
        } else if b == 1 {
            Some(Value::F(fv.first().copied().unwrap_or(0.0)))
        } else {
            Some(Value::I(i.first().copied().unwrap_or(0)))
        };
        // Multi-return natives (`int, int F()`) fill `play.multi_ret`.
        for (k, v) in std::mem::take(&mut self.play.multi_ret)
            .into_iter()
            .enumerate()
        {
            if k < self.rets.len() {
                self.rets[k] = Some(v);
            }
        }
        Ok(())
    }

    /// Fuel charged when executing word `pc` of function `func_i` (0 unless a block leader).
    fn block_cost(&mut self, func_i: usize, pc: usize) -> u32 {
        let code = &self.program.functions[func_i].code;
        if self.costs.len() < self.program.functions.len() {
            self.costs.resize(self.program.functions.len(), Vec::new());
        }
        let entry = &mut self.costs[func_i];
        if entry.len() != code.len() {
            *entry = crate::program::block_costs(code);
        }
        entry.get(pc).copied().unwrap_or(0)
    }

    fn take_ext(&self, stack: &mut [Frame], func_i: usize) -> Result<u32, Trap> {
        let top = stack.len() - 1;
        let pc = stack[top].pc;
        let code = &self.program.functions[func_i].code;
        if pc >= code.len() {
            return Err(Trap::BadOpcode {
                func: self.fname(stack[top].func),
                op: 0xff,
            });
        }
        let ext = code[pc];
        stack[top].pc = pc + 1;
        Ok(ext)
    }

    pub(crate) fn fname(&self, idx: u32) -> String {
        self.program
            .functions
            .get(idx as usize)
            .map(|f| f.name.clone())
            .unwrap_or_else(|| format!("#{idx}"))
    }

    pub(crate) fn note_last_i(&mut self, frame: &Frame) {
        self.last_i = [0; 4];
        for (i, slot) in self.last_i.iter_mut().enumerate() {
            *slot = frame.i.get(i).copied().unwrap_or(0);
        }
        let h = frame.s.first().copied().unwrap_or_default();
        self.last_s = self.heap.str(h).to_owned();
    }

    /// Run GC with current stack-less roots (call between tics).
    pub fn gc(&mut self, roots: &[ObjHandle], budget: usize) -> usize {
        self.heap.gc_step(roots, budget)
    }
}

/// Trap for a failed shared heap op.
pub(crate) fn heap_trap(e: HeapErr, func: String, line: u32) -> Trap {
    match e {
        HeapErr::Null => Trap::NullDeref { func, line },
        HeapErr::Oob => Trap::ArrayOob { func, line },
    }
}

/// Rewrite `CallNative` extension words from import indices → runtime ids.
fn lookup_func(program: &CompiledProgram, name: &str) -> Result<u32, Trap> {
    program
        .func_by_name
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, &i)| i)
        .ok_or_else(|| Trap::Native(format!("unknown function {name}")))
}

pub(crate) fn class_is(program: &CompiledProgram, mut class_id: u32, want: u32) -> bool {
    for _ in 0..64 {
        if class_id == want {
            return true;
        }
        let Some(layout) = program.classes.get(class_id as usize) else {
            return false;
        };
        if layout.parent.is_empty() {
            return false;
        }
        let Some(parent) = program
            .classes
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(&layout.parent))
        else {
            return false;
        };
        class_id = parent as u32;
    }
    false
}

/// `PlayerPawn.FindInventory` binds to `Actor.FindInventory` when only the
/// declaring class has the native (calls through a typed receiver).
fn inherit_native_imports(program: &mut CompiledProgram, natives: &NativeTable) {
    let parent_of = |class: &str| -> Option<String> {
        program
            .classes
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(class))
            .map(|c| c.parent.clone())
            .filter(|p| !p.is_empty())
    };
    let mut renamed = Vec::new();
    for (k, name) in program.native_imports.iter().enumerate() {
        if natives.id_ci(name).is_some() {
            continue;
        }
        let Some((class, method)) = name.split_once('.') else {
            continue;
        };
        let mut cur = parent_of(class);
        for _ in 0..64 {
            let Some(parent) = cur else {
                break;
            };
            let candidate = format!("{parent}.{method}");
            if natives.id_ci(&candidate).is_some() {
                renamed.push((k, candidate));
                break;
            }
            cur = parent_of(&parent);
        }
    }
    for (k, name) in renamed {
        program.native_imports[k] = name;
    }
}

fn remap_native_imports(program: &mut CompiledProgram, natives: &NativeTable) {
    if program.native_imports.is_empty() {
        return;
    }
    let map: Vec<u32> = program
        .native_imports
        .iter()
        .map(|name| natives.id_ci(name).unwrap_or(u32::MAX))
        .collect();
    for func in &mut program.functions {
        let mut pc = 0;
        while pc < func.code.len() {
            let word = func.code[pc];
            let (op_b, _, _, _) = unpack(word);
            pc += 1;
            let Some(op) = Op::from_u8(op_b) else {
                continue;
            };
            if op.needs_ext() {
                if pc >= func.code.len() {
                    break;
                }
                if op == Op::CallNative || op == Op::CallNat {
                    let import = func.code[pc] as usize;
                    if let Some(&runtime_id) = map.get(import) {
                        func.code[pc] = runtime_id;
                    }
                }
                pc += 1;
            }
        }
    }
}

fn read_value(f: &Frame, bank: u8, reg: u8) -> Value {
    match bank {
        1 => Value::F(f.f.get(reg as usize).copied().unwrap_or(0.0)),
        2 => Value::S(f.s.get(reg as usize).copied().unwrap_or_default()),
        3 => Value::A(f.a.get(reg as usize).copied().unwrap_or_default()),
        _ => Value::I(f.i.get(reg as usize).copied().unwrap_or(0)),
    }
}

pub(crate) fn write_value(f: &mut Frame, bank: u8, reg: u8, v: Value) {
    match (bank, v) {
        (1, Value::F(x)) => {
            if let Some(slot) = f.f.get_mut(reg as usize) {
                *slot = x;
            }
        }
        (2, Value::S(h)) => {
            if let Some(slot) = f.s.get_mut(reg as usize) {
                *slot = h;
            }
        }
        (3, Value::A(h)) => {
            if let Some(slot) = f.a.get_mut(reg as usize) {
                *slot = h;
            }
        }
        (_, Value::I(x)) => {
            if let Some(slot) = f.i.get_mut(reg as usize) {
                *slot = x;
            }
        }
        _ => {}
    }
}

fn set_i(f: &mut Frame, a: u8, v: i64) {
    if let Some(slot) = f.i.get_mut(a as usize) {
        *slot = v;
    }
}
fn set_f(f: &mut Frame, a: u8, v: f64) {
    if let Some(slot) = f.f.get_mut(a as usize) {
        *slot = v;
    }
}
fn set_s(f: &mut Frame, a: u8, v: StrHandle) {
    if let Some(slot) = f.s.get_mut(a as usize) {
        *slot = v;
    }
}
fn set_a(f: &mut Frame, a: u8, v: ObjHandle) {
    if let Some(slot) = f.a.get_mut(a as usize) {
        *slot = v;
    }
}
