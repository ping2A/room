//! Module and function emission for [`crate::Target::Aot`] (`docs/37` §4–§5).

use crate::cfg::{self, Cfg, Insn, Term};
use crate::{Artifact, Options, Stats, Unsupported};
use room_zvm::aot_abi::{self as abi, ImageLayout, import_index};
use room_zvm::{CompiledFunction, CompiledProgram, Op};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, ElementSection, Elements, EntityType, ExportKind,
    ExportSection, Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    IndirectNameMap, Instruction as I, MemArg, MemorySection, MemoryType, Module, NameMap,
    NameSection, RefType, TableSection, TableType, TypeSection, ValType,
};

const BANK_TYPES: [ValType; 4] = [ValType::I64, ValType::F64, ValType::I32, ValType::I64];

/// Straight-line functions longer than this are entered through wasm under
/// the `auto` policy.
const HOT_INSNS: usize = 48;

const G_FUEL: u32 = 0;
const G_DEPTH: u32 = 1;
const G_LIMIT: u32 = 2;
const G_RQ_SET: u32 = 3;
const G_RQ_VAL: u32 = 4;

fn vt(s: &str) -> ValType {
    match s {
        "i64" => ValType::I64,
        "f64" => ValType::F64,
        _ => ValType::I32,
    }
}

fn mem(offset: u32, align: u32) -> MemArg {
    MemArg {
        offset: u64::from(offset),
        align,
        memory_index: 0,
    }
}

/// Build the whole module.
pub(crate) fn emit_aot(program: &CompiledProgram, opts: &Options) -> Artifact {
    let nfuncs = program.functions.len();
    // Compile every function first: a function is supported only if its own
    // body lowers. Callers fall back to `call_interp` for the rest.
    let mut cfgs: Vec<Option<Cfg>> = Vec::with_capacity(nfuncs);
    let mut unsupported = Vec::new();
    for (i, f) in program.functions.iter().enumerate() {
        match cfg::build(f).and_then(|c| precheck(&c).map(|()| c)) {
            Ok(c) => cfgs.push(Some(c)),
            Err(e) => {
                unsupported.push((i as u32, e));
                cfgs.push(None);
            }
        }
    }
    let compiled: Vec<bool> = cfgs.iter().map(Option::is_some).collect();
    // Worth entering from the host: has a loop, or is long enough that the
    // per-call host cost (~0.3 µs) is small next to its body.
    let hot: Vec<bool> = cfgs
        .iter()
        .map(|c| {
            c.as_ref().is_some_and(|c| {
                c.loop_header.iter().any(|h| *h)
                    || c.blocks.iter().map(|b| b.insns.len()).sum::<usize>() > HOT_INSNS
            })
        })
        .collect();
    let n_imports = abi::IMPORTS.len() as u32;
    // Wasm function index of each compiled function.
    let mut wasm_idx = vec![u32::MAX; nfuncs];
    let mut next = n_imports;
    for (i, c) in compiled.iter().enumerate() {
        if *c {
            wasm_idx[i] = next;
            next += 1;
        }
    }

    let mut types = TypeSection::new();
    // Type 0: () -> () for every compiled function.
    types.ty().function([], []);
    let mut imports = ImportSection::new();
    for (k, (name, params, results)) in abi::IMPORTS.iter().enumerate() {
        types
            .ty()
            .function(params.iter().map(|p| vt(p)), results.iter().map(|r| vt(r)));
        imports.import(abi::IMPORT_MODULE, name, EntityType::Function(k as u32 + 1));
    }
    let mut funcs = FunctionSection::new();
    let mut code = CodeSection::new();
    let mut stats = Stats {
        functions: nfuncs,
        ..Default::default()
    };
    let ctx = ModCtx {
        program,
        compiled: &compiled,
        wasm_idx: &wasm_idx,
    };
    for (i, c) in cfgs.iter().enumerate() {
        let Some(c) = c else { continue };
        funcs.function(0);
        stats.compiled += 1;
        stats.blocks += c.blocks.len();
        stats.insns += c.blocks.iter().map(|b| b.insns.len()).sum::<usize>();
        let body = FuncGen::new(&ctx, i as u32, c).run();
        code.function(&body);
    }

    let mut tables = TableSection::new();
    tables.table(TableType {
        element_type: RefType::FUNCREF,
        table64: false,
        minimum: nfuncs.max(1) as u64,
        maximum: Some(nfuncs.max(1) as u64),
        shared: false,
    });
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: abi::PAGES,
        maximum: Some(abi::PAGES),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    let mut globals = GlobalSection::new();
    let g = |ty: ValType| GlobalType {
        val_type: ty,
        mutable: true,
        shared: false,
    };
    globals.global(g(ValType::I64), &ConstExpr::i64_const(0));
    globals.global(g(ValType::I32), &ConstExpr::i32_const(0));
    globals.global(g(ValType::I32), &ConstExpr::i32_const(256));
    globals.global(g(ValType::I32), &ConstExpr::i32_const(0));
    globals.global(g(ValType::I64), &ConstExpr::i64_const(0));
    let mut exports = ExportSection::new();
    exports.export(abi::MEMORY, ExportKind::Memory, 0);
    exports.export(abi::TABLE, ExportKind::Table, 0);
    exports.export(abi::GLOBAL_FUEL, ExportKind::Global, G_FUEL);
    exports.export(abi::GLOBAL_DEPTH, ExportKind::Global, G_DEPTH);
    exports.export(abi::GLOBAL_LIMIT, ExportKind::Global, G_LIMIT);
    exports.export(abi::GLOBAL_RQ_SET, ExportKind::Global, G_RQ_SET);
    exports.export(abi::GLOBAL_RQ_VAL, ExportKind::Global, G_RQ_VAL);
    for (i, w) in wasm_idx.iter().enumerate() {
        if *w != u32::MAX {
            exports.export(&abi::func_export(i as u32), ExportKind::Func, *w);
        }
    }
    let mut elements = ElementSection::new();
    for (i, w) in wasm_idx.iter().enumerate() {
        if *w != u32::MAX {
            elements.active(
                Some(0),
                &ConstExpr::i32_const(i as i32),
                Elements::Functions(std::borrow::Cow::Owned(vec![*w])),
            );
        }
    }

    let mut module = Module::new();
    module.section(&types);
    module.section(&imports);
    module.section(&funcs);
    module.section(&tables);
    module.section(&memories);
    module.section(&globals);
    module.section(&exports);
    module.section(&elements);
    module.section(&code);
    if opts.emit_names {
        let mut names = NameSection::new();
        let mut fnames = NameMap::new();
        for (k, (name, _, _)) in abi::IMPORTS.iter().enumerate() {
            fnames.append(k as u32, &format!("zvm.{name}"));
        }
        for (i, w) in wasm_idx.iter().enumerate() {
            if *w != u32::MAX {
                fnames.append(*w, &program.functions[i].name);
            }
        }
        names.functions(&fnames);
        names.locals(&IndirectNameMap::new());
        module.section(&names);
    }
    let bytes = module.finish();
    stats.bytes = bytes.len();
    Artifact {
        bytes,
        compiled,
        hot,
        unsupported,
        stats,
    }
}

/// Reject functions whose `Arg`s cross a block boundary or overflow the cells.
fn precheck(c: &Cfg) -> Result<(), Unsupported> {
    for b in &c.blocks {
        let mut pending = 0u32;
        for i in &b.insns {
            match i.op {
                Op::Arg => pending += 1,
                Op::Call | Op::CallVirt | Op::CallNat => {
                    if pending > abi::MAX_ARGS {
                        return Err(Unsupported::TooManyArgs);
                    }
                    pending = 0;
                }
                _ => {}
            }
        }
        if pending != 0 {
            return Err(Unsupported::ArgAcrossBlocks);
        }
    }
    Ok(())
}

struct ModCtx<'a> {
    program: &'a CompiledProgram,
    compiled: &'a [bool],
    wasm_idx: &'a [u32],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    If,
    Loop(usize),
    Follow(usize),
}

struct FuncGen<'a> {
    m: &'a ModCtx<'a>,
    func: &'a CompiledFunction,
    fidx: u32,
    cfg: &'a Cfg,
    layout: ImageLayout,
    /// Local index of register 0 per bank.
    base: [u32; 4],
    /// Extra locals by type after the register locals.
    temps: Vec<ValType>,
    free: Vec<(ValType, u32)>,
    out: Vec<I<'static>>,
    /// Pending `Arg` snapshots (bank, local).
    pending: Vec<(usize, u32)>,
}

impl<'a> FuncGen<'a> {
    fn new(m: &'a ModCtx<'a>, fidx: u32, cfg: &'a Cfg) -> Self {
        let func = &m.program.functions[fidx as usize];
        let layout = ImageLayout::of(func);
        let c = layout.counts;
        let base = [0, c[0], c[0] + c[1], c[0] + c[1] + c[2]];
        Self {
            m,
            func,
            fidx,
            cfg,
            layout,
            base,
            temps: Vec::new(),
            free: Vec::new(),
            out: Vec::new(),
            pending: Vec::new(),
        }
    }

    fn n_reg_locals(&self) -> u32 {
        self.layout.counts.iter().sum()
    }

    fn temp(&mut self, ty: ValType) -> u32 {
        if let Some(pos) = self.free.iter().position(|(t, _)| *t == ty) {
            return self.free.swap_remove(pos).1;
        }
        let idx = self.n_reg_locals() + self.temps.len() as u32;
        self.temps.push(ty);
        idx
    }

    fn release(&mut self, ty: ValType, local: u32) {
        self.free.push((ty, local));
    }

    fn e(&mut self, i: I<'static>) {
        self.out.push(i);
    }

    fn call_imp(&mut self, name: &str) {
        self.e(I::Call(import_index(name)));
    }

    fn fi(&self) -> i32 {
        self.fidx as i32
    }

    /// Push register `reg` of `bank`; registers past the bank read as zero.
    fn get(&mut self, bank: usize, reg: u8) {
        if u32::from(reg) < self.layout.counts[bank] {
            self.e(I::LocalGet(self.base[bank] + u32::from(reg)));
        } else {
            self.zero(bank);
        }
    }

    fn zero(&mut self, bank: usize) {
        match bank {
            1 => self.e(I::F64Const(0.0)),
            2 => self.e(I::I32Const(0)),
            _ => self.e(I::I64Const(0)),
        }
    }

    /// Pop into register `reg` of `bank`; writes past the bank are dropped.
    fn set(&mut self, bank: usize, reg: u8) {
        if u32::from(reg) < self.layout.counts[bank] {
            self.e(I::LocalSet(self.base[bank] + u32::from(reg)));
        } else {
            self.e(I::Drop);
        }
    }

    fn trap(&mut self, kind: i32, pc: u32, extra: i32) {
        self.e(I::I32Const(kind));
        self.e(I::I32Const(self.fi()));
        self.e(I::I32Const(pc as i32));
        self.e(I::I32Const(extra));
        self.call_imp("trap");
        self.e(I::Unreachable);
    }

    fn run(mut self) -> Function {
        // Prologue: depth check, then load the register image.
        self.e(I::GlobalGet(G_DEPTH));
        self.e(I::I32Const(1));
        self.e(I::I32Add);
        self.e(I::GlobalSet(G_DEPTH));
        self.e(I::GlobalGet(G_DEPTH));
        self.e(I::GlobalGet(G_LIMIT));
        self.e(I::I32GtS);
        self.e(I::If(BlockType::Empty));
        self.trap(abi::TRAP_STACK, 0, 0);
        self.e(I::End);
        let mask = abi::entry_mask(self.func);
        #[allow(clippy::needless_range_loop)] // `bank` indexes four parallel per-bank tables
        for bank in 0..4 {
            for r in 0..self.layout.counts[bank] {
                if !mask[bank][r as usize] {
                    continue;
                }
                let addr = self.layout.addr(bank, r);
                self.e(I::I32Const(0));
                self.e(match bank {
                    0 | 3 => I::I64Load(mem(addr, 3)),
                    1 => I::F64Load(mem(addr, 3)),
                    _ => I::I32Load(mem(addr, 2)),
                });
                self.e(I::LocalSet(self.base[bank] + r));
            }
        }
        let mut ctx = Vec::new();
        self.do_tree(0, &mut ctx);
        // Every path leaves through `return`, `br` or `unreachable`.
        self.e(I::Unreachable);
        self.e(I::End);
        let mut locals: Vec<(u32, ValType)> = Vec::new();
        for (count, ty) in self.layout.counts.iter().zip(BANK_TYPES) {
            locals.push((*count, ty));
        }
        for t in &self.temps {
            locals.push((1, *t));
        }
        let mut f = Function::new(locals);
        for i in &self.out {
            f.instruction(i);
        }
        f
    }

    // ---- structured control flow (Ramsey, "Beyond Relooper") ----

    fn is_merge(&self, b: usize) -> bool {
        self.cfg.forward_in[b] >= 2
    }

    fn do_tree(&mut self, x: usize, ctx: &mut Vec<Ctx>) {
        let mut merges: Vec<usize> = self.cfg.dom_children[x]
            .iter()
            .copied()
            .filter(|&c| self.is_merge(c))
            .collect();
        merges.sort_by_key(|&c| std::cmp::Reverse(self.cfg.rpo[c]));
        if self.cfg.loop_header[x] {
            self.e(I::Loop(BlockType::Empty));
            ctx.push(Ctx::Loop(x));
            self.node_within(x, &merges, ctx);
            ctx.pop();
            self.e(I::End);
        } else {
            self.node_within(x, &merges, ctx);
        }
    }

    fn node_within(&mut self, x: usize, merges: &[usize], ctx: &mut Vec<Ctx>) {
        if let Some((&y, rest)) = merges.split_first() {
            self.e(I::Block(BlockType::Empty));
            ctx.push(Ctx::Follow(y));
            self.node_within(x, rest, ctx);
            ctx.pop();
            self.e(I::End);
            self.do_tree(y, ctx);
            return;
        }
        self.block_body(x);
        match self.cfg.blocks[x].term {
            Term::Goto(t) => self.do_branch(x, t, ctx),
            Term::Cond {
                reg,
                if_true,
                if_false,
            } => {
                self.get(0, reg);
                self.e(I::I64Const(0));
                self.e(I::I64Ne);
                self.e(I::If(BlockType::Empty));
                ctx.push(Ctx::If);
                self.do_branch(x, if_true, ctx);
                self.e(I::Else);
                self.do_branch(x, if_false, ctx);
                ctx.pop();
                self.e(I::End);
            }
            Term::Leave => {}
        }
    }

    fn do_branch(&mut self, src: usize, dst: usize, ctx: &mut Vec<Ctx>) {
        let backward = self.cfg.rpo[dst] <= self.cfg.rpo[src];
        if backward || self.is_merge(dst) {
            let want = if backward {
                Ctx::Loop(dst)
            } else {
                Ctx::Follow(dst)
            };
            let depth = ctx
                .iter()
                .rev()
                .position(|c| *c == want)
                .unwrap_or_else(|| panic!("unstructured branch {src}->{dst}"));
            self.e(I::Br(depth as u32));
        } else {
            self.do_tree(dst, ctx);
        }
    }

    // ---- straight-line code ----

    fn block_body(&mut self, x: usize) {
        let cost = self.cfg.blocks[x].cost;
        if cost > 0 {
            self.e(I::GlobalGet(G_FUEL));
            self.e(I::I64Const(i64::from(cost)));
            self.e(I::I64LtU);
            self.e(I::If(BlockType::Empty));
            let pc = self.cfg.blocks[x].insns.first().map_or(0, |i| i.pc);
            self.trap(abi::TRAP_FUEL, pc, 0);
            self.e(I::End);
            self.e(I::GlobalGet(G_FUEL));
            self.e(I::I64Const(i64::from(cost)));
            self.e(I::I64Sub);
            self.e(I::GlobalSet(G_FUEL));
        }
        if x == self.cfg.exit {
            self.leave(abi::EXIT_IMPLICIT, None);
            return;
        }
        let insns = self.cfg.blocks[x].insns.clone();
        for i in &insns {
            self.insn(i);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn insn(&mut self, i: &Insn) {
        let (a, b, c) = (i.a, i.b, i.c);
        match i.op {
            Op::Nop | Op::Jmp | Op::JmpIf | Op::JmpIfNot => {}
            Op::MoveI => self.mv(0, a, b),
            Op::MoveF => self.mv(1, a, b),
            Op::MoveS => self.mv(2, a, b),
            Op::MoveA => self.mv(3, a, b),
            Op::LoadConstI => {
                let v = self.func.const_i.get(i.ext as usize).copied().unwrap_or(0);
                self.e(I::I64Const(v));
                self.set(0, a);
            }
            Op::LoadConstF => {
                let v = self
                    .func
                    .const_f
                    .get(i.ext as usize)
                    .copied()
                    .unwrap_or(0.0);
                self.e(I::F64Const(v));
                self.set(1, a);
            }
            Op::LoadConstS => {
                self.e(I::I32Const(self.fi()));
                self.e(I::I32Const(i.ext as i32));
                self.call_imp("str_const");
                self.set(2, a);
            }
            Op::LoadNull => {
                self.e(I::I64Const(0));
                self.set(3, a);
            }
            Op::AddI | Op::SubI | Op::MulI => {
                self.get(0, b);
                self.get(0, c);
                self.e(match i.op {
                    Op::AddI => I::I64Add,
                    Op::SubI => I::I64Sub,
                    _ => I::I64Mul,
                });
                self.set(0, a);
            }
            Op::DivI | Op::ModI => {
                let lb = self.temp(ValType::I64);
                let rc = self.temp(ValType::I64);
                self.get(0, b);
                self.e(I::LocalSet(lb));
                self.get(0, c);
                self.e(I::LocalSet(rc));
                self.e(I::LocalGet(rc));
                self.e(I::I64Eqz);
                self.e(I::If(BlockType::Empty));
                self.trap(abi::TRAP_DIV_ZERO, i.pc, 0);
                self.e(I::End);
                self.e(I::LocalGet(lb));
                self.e(I::I64Const(i64::MIN));
                self.e(I::I64Eq);
                self.e(I::LocalGet(rc));
                self.e(I::I64Const(-1));
                self.e(I::I64Eq);
                self.e(I::I32And);
                self.e(I::If(BlockType::Empty));
                self.trap(abi::TRAP_OVERFLOW, i.pc, 0);
                self.e(I::End);
                self.e(I::LocalGet(lb));
                self.e(I::LocalGet(rc));
                self.e(if i.op == Op::DivI {
                    I::I64DivS
                } else {
                    I::I64RemS
                });
                self.set(0, a);
                self.release(ValType::I64, lb);
                self.release(ValType::I64, rc);
            }
            Op::LtI | Op::LeI | Op::EqI | Op::NeI | Op::GtI | Op::GeI => {
                self.get(0, b);
                self.get(0, c);
                self.e(match i.op {
                    Op::LtI => I::I64LtS,
                    Op::LeI => I::I64LeS,
                    Op::EqI => I::I64Eq,
                    Op::NeI => I::I64Ne,
                    Op::GtI => I::I64GtS,
                    _ => I::I64GeS,
                });
                self.e(I::I64ExtendI32U);
                self.set(0, a);
            }
            Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr => {
                self.get(0, b);
                self.get(0, c);
                self.e(match i.op {
                    Op::BitAnd => I::I64And,
                    Op::BitOr => I::I64Or,
                    Op::BitXor => I::I64Xor,
                    Op::Shl => I::I64Shl,
                    _ => I::I64ShrS,
                });
                self.set(0, a);
            }
            Op::I2F => {
                self.get(0, b);
                self.e(I::F64ConvertI64S);
                self.set(1, a);
            }
            Op::F2I => {
                self.get(1, b);
                self.e(I::I64TruncSatF64S);
                self.set(0, a);
            }
            Op::CmpF => {
                let lb = self.temp(ValType::F64);
                let rc = self.temp(ValType::F64);
                self.get(1, b);
                self.e(I::LocalSet(lb));
                self.get(1, c);
                self.e(I::LocalSet(rc));
                self.e(I::I64Const(-1));
                self.e(I::I64Const(1));
                self.e(I::I64Const(0));
                self.e(I::LocalGet(lb));
                self.e(I::LocalGet(rc));
                self.e(I::F64Gt);
                self.e(I::Select);
                self.e(I::LocalGet(lb));
                self.e(I::LocalGet(rc));
                self.e(I::F64Lt);
                self.e(I::Select);
                self.set(0, a);
                self.release(ValType::F64, lb);
                self.release(ValType::F64, rc);
            }
            Op::AddF | Op::SubF | Op::MulF => {
                self.get(1, b);
                self.get(1, c);
                self.e(match i.op {
                    Op::AddF => I::F64Add,
                    Op::SubF => I::F64Sub,
                    _ => I::F64Mul,
                });
                self.set(1, a);
            }
            Op::DivF => {
                let rc = self.temp(ValType::F64);
                self.get(1, c);
                self.e(I::LocalSet(rc));
                self.e(I::LocalGet(rc));
                self.e(I::F64Const(0.0));
                self.e(I::F64Eq);
                self.e(I::If(BlockType::Empty));
                self.trap(abi::TRAP_DIV_ZERO, i.pc, 0);
                self.e(I::End);
                self.get(1, b);
                self.e(I::LocalGet(rc));
                self.e(I::F64Div);
                self.set(1, a);
                self.release(ValType::F64, rc);
            }
            Op::IsNull => {
                self.get(3, b);
                self.call_imp("is_null");
                self.set(0, a);
            }
            Op::IsClass => {
                self.get(3, b);
                self.e(I::I32Const(i.ext as i32));
                self.call_imp("is_class");
                self.set(0, a);
            }
            Op::NewObj => {
                self.e(I::I32Const(i.ext as i32));
                self.call_imp("new_obj");
                self.set(3, a);
            }
            Op::LoadFieldI | Op::LoadFieldF | Op::LoadFieldS | Op::LoadFieldA => {
                let (name, bank) = match i.op {
                    Op::LoadFieldI => ("ld_i", 0),
                    Op::LoadFieldF => ("ld_f", 1),
                    Op::LoadFieldS => ("ld_s", 2),
                    _ => ("ld_a", 3),
                };
                self.get(3, b);
                self.e(I::I32Const(i.ext as i32));
                self.site(i.pc);
                self.call_imp(name);
                self.set(bank, a);
            }
            Op::StoreFieldI | Op::StoreFieldF | Op::StoreFieldS | Op::StoreFieldA => {
                let (name, bank) = match i.op {
                    Op::StoreFieldI => ("st_i", 0),
                    Op::StoreFieldF => ("st_f", 1),
                    Op::StoreFieldS => ("st_s", 2),
                    _ => ("st_a", 3),
                };
                self.get(3, a);
                self.e(I::I32Const(i.ext as i32));
                self.get(bank, b);
                self.site(i.pc);
                self.call_imp(name);
            }
            Op::ArrayLen => {
                self.get(3, b);
                self.site(i.pc);
                self.call_imp("arr_len");
                self.set(0, a);
            }
            Op::ArrayGetI | Op::ArrayGetS => {
                self.get(3, b);
                self.get(0, c);
                self.site(i.pc);
                if i.op == Op::ArrayGetI {
                    self.call_imp("arr_get_i");
                    self.set(0, a);
                } else {
                    self.call_imp("arr_get_s");
                    self.set(2, a);
                }
            }
            Op::ArraySetI | Op::ArraySetS => {
                self.get(3, a);
                self.get(0, b);
                if i.op == Op::ArraySetI {
                    self.get(0, c);
                    self.site(i.pc);
                    self.call_imp("arr_set_i");
                } else {
                    self.get(2, c);
                    self.site(i.pc);
                    self.call_imp("arr_set_s");
                }
            }
            Op::ArrayPushI | Op::ArrayPushS => {
                self.get(3, a);
                if i.op == Op::ArrayPushI {
                    self.get(0, b);
                    self.site(i.pc);
                    self.call_imp("arr_push_i");
                } else {
                    self.get(2, b);
                    self.site(i.pc);
                    self.call_imp("arr_push_s");
                }
            }
            Op::ConcatS => {
                self.get(2, b);
                self.get(2, c);
                self.call_imp("concat");
                self.set(2, a);
            }
            Op::Trap => {
                self.trap(abi::TRAP_EXPLICIT, i.pc, i.ext as i32);
            }
            Op::Arg => {
                let bank = abi::value_bank(a);
                let t = self.temp(BANK_TYPES[bank]);
                self.get(bank, b);
                self.e(I::LocalSet(t));
                self.pending.push((bank, t));
            }
            Op::Call => self.call_static(i),
            Op::CallVirt => {
                self.clear_rets();
                let n = self.store_arg_cells();
                self.e(I::I32Const(0));
                self.e(I::GlobalSet(G_RQ_SET));
                self.e(I::I32Const(i.ext as i32));
                self.e(I::I32Const(n as i32));
                self.site(i.pc);
                self.call_imp("vcall");
                let t = self.temp(ValType::I32);
                self.e(I::LocalTee(t));
                self.e(I::I32Const(0));
                self.e(I::I32GeS);
                self.e(I::If(BlockType::Empty));
                self.e(I::LocalGet(t));
                self.e(I::CallIndirect {
                    type_index: 0,
                    table_index: 0,
                });
                self.e(I::End);
                self.release(ValType::I32, t);
                self.apply_rq();
            }
            Op::CallNat => {
                let n = self.store_arg_cells();
                self.e(I::I32Const(i.ext as i32));
                self.e(I::I32Const(n as i32));
                self.e(I::I32Const(i32::from(b)));
                self.site(i.pc);
                self.call_imp("native");
            }
            Op::SetRet => {
                if u32::from(c) < abi::N_RETS {
                    self.store_cell(abi::RETS + u32::from(c) * abi::CELL, abi::value_bank(a), b);
                }
            }
            Op::Ret => {
                let bank = abi::value_bank(a);
                if u32::from(c) < abi::N_RETS {
                    self.store_cell(abi::RETS + u32::from(c) * abi::CELL, bank, b);
                }
                self.leave(abi::EXIT_RET, Some((bank, b)));
            }
            Op::GetRet => self.get_ret(a, b, c),
            Op::Return => self.leave(abi::EXIT_RETURN, Some((0, b))),
            Op::CallStatic | Op::CallNative => unreachable!("rejected by cfg::decode"),
        }
    }

    fn mv(&mut self, bank: usize, a: u8, b: u8) {
        self.get(bank, b);
        self.set(bank, a);
    }

    /// Push `(func, pc)` for imports that can trap.
    fn site(&mut self, pc: u32) {
        self.e(I::I32Const(self.fi()));
        self.e(I::I32Const(pc as i32));
    }

    fn clear_rets(&mut self) {
        self.e(I::I32Const(abi::RETS as i32));
        self.e(I::I32Const(0));
        self.e(I::I32Const((abi::N_RETS * abi::CELL) as i32));
        self.e(I::MemoryFill(0));
    }

    /// Write `(tag, payload)` for register `reg` of `bank` as a cell at `addr`.
    fn store_cell(&mut self, addr: u32, bank: usize, reg: u8) {
        self.store_tagged(addr, addr + 8, bank, reg);
    }

    fn store_tagged(&mut self, tag_addr: u32, payload_addr: u32, bank: usize, reg: u8) {
        let tag = [abi::TAG_I, abi::TAG_F, abi::TAG_S, abi::TAG_A][bank];
        self.e(I::I32Const(0));
        self.e(I::I32Const(tag as i32));
        self.e(I::I32Store(mem(tag_addr, 2)));
        self.e(I::I32Const(0));
        self.get(bank, reg);
        match bank {
            1 => self.e(I::F64Store(mem(payload_addr, 3))),
            2 => {
                self.e(I::I64ExtendI32U);
                self.e(I::I64Store(mem(payload_addr, 3)));
            }
            _ => self.e(I::I64Store(mem(payload_addr, 3))),
        }
    }

    /// Spill the pending `Arg` snapshots into the argument cells.
    fn store_arg_cells(&mut self) -> u32 {
        let pending = std::mem::take(&mut self.pending);
        for (k, (bank, t)) in pending.iter().enumerate() {
            let addr = abi::ARGS + k as u32 * abi::CELL;
            let tag = [abi::TAG_I, abi::TAG_F, abi::TAG_S, abi::TAG_A][*bank];
            self.e(I::I32Const(0));
            self.e(I::I32Const(tag as i32));
            self.e(I::I32Store(mem(addr, 2)));
            self.e(I::I32Const(0));
            self.e(I::LocalGet(*t));
            match bank {
                1 => self.e(I::F64Store(mem(addr + 8, 3))),
                2 => {
                    self.e(I::I64ExtendI32U);
                    self.e(I::I64Store(mem(addr + 8, 3)));
                }
                _ => self.e(I::I64Store(mem(addr + 8, 3))),
            }
            self.release(BANK_TYPES[*bank], *t);
        }
        pending.len() as u32
    }

    fn apply_rq(&mut self) {
        self.e(I::GlobalGet(G_RQ_SET));
        self.e(I::If(BlockType::Empty));
        self.e(I::GlobalGet(G_RQ_VAL));
        self.set(0, 0);
        self.e(I::End);
    }

    fn call_static(&mut self, i: &Insn) {
        let callee = i.ext as usize;
        self.clear_rets();
        let Some(cf) = self.m.program.functions.get(callee) else {
            let pending = std::mem::take(&mut self.pending);
            for (bank, t) in pending {
                self.release(BANK_TYPES[bank], t);
            }
            self.trap(abi::TRAP_BAD_CALL, i.pc, i.ext as i32);
            return;
        };
        self.e(I::I32Const(0));
        self.e(I::GlobalSet(G_RQ_SET));
        if !self.m.compiled[callee] {
            let n = self.store_arg_cells();
            self.e(I::I32Const(callee as i32));
            self.e(I::I32Const(n as i32));
            self.site(i.pc);
            self.call_imp("call_interp");
            self.apply_rq();
            return;
        }
        let lay = ImageLayout::of(cf);
        self.e(I::I32Const(abi::IMG as i32));
        self.e(I::I32Const(0));
        self.e(I::I32Const(lay.size as i32));
        self.e(I::MemoryFill(0));
        // Interpreter `write_value(frame, p.bank, p.reg as u8, arg)`.
        let pending = std::mem::take(&mut self.pending);
        for (p, (vbank, t)) in cf.params.iter().zip(pending.iter()) {
            let reg = u32::from(p.reg as u8);
            let dest = match (p.bank, *vbank) {
                (1, 1) => Some(1),
                (2, 2) => Some(2),
                (3, 3) => Some(3),
                (_, 0) => Some(0),
                _ => None,
            };
            if let Some(db) = dest {
                if reg < lay.counts[db] {
                    let addr = lay.addr(db, reg);
                    self.e(I::I32Const(0));
                    self.e(I::LocalGet(*t));
                    self.e(match db {
                        1 => I::F64Store(mem(addr, 3)),
                        2 => I::I32Store(mem(addr, 2)),
                        _ => I::I64Store(mem(addr, 3)),
                    });
                }
            }
        }
        for (bank, t) in pending {
            self.release(BANK_TYPES[bank], t);
        }
        self.e(I::Call(self.m.wasm_idx[callee]));
        self.apply_rq();
    }

    /// `write_value(frame, a, b, rets[c].unwrap_or(I(0)))`.
    fn get_ret(&mut self, bank_a: u8, reg: u8, c: u8) {
        if u32::from(c) >= abi::N_RETS {
            self.e(I::I64Const(0));
            self.set(0, reg);
            return;
        }
        let addr = abi::RETS + u32::from(c) * abi::CELL;
        let tag = self.temp(ValType::I32);
        self.e(I::I32Const(0));
        self.e(I::I32Load(mem(addr, 2)));
        self.e(I::LocalSet(tag));
        // Integer (or empty → 0) writes i[reg] whatever the target bank.
        self.e(I::LocalGet(tag));
        self.e(I::I32Const(abi::TAG_I as i32));
        self.e(I::I32Eq);
        self.e(I::If(BlockType::Empty));
        self.e(I::I32Const(0));
        self.e(I::I64Load(mem(addr + 8, 3)));
        self.set(0, reg);
        self.e(I::Else);
        self.e(I::LocalGet(tag));
        self.e(I::I32Eqz);
        self.e(I::If(BlockType::Empty));
        self.e(I::I64Const(0));
        self.set(0, reg);
        let own = match bank_a {
            1 => Some((1usize, abi::TAG_F)),
            2 => Some((2, abi::TAG_S)),
            3 => Some((3, abi::TAG_A)),
            _ => None,
        };
        if let Some((bank, want)) = own {
            self.e(I::Else);
            self.e(I::LocalGet(tag));
            self.e(I::I32Const(want as i32));
            self.e(I::I32Eq);
            self.e(I::If(BlockType::Empty));
            self.e(I::I32Const(0));
            match bank {
                1 => self.e(I::F64Load(mem(addr + 8, 3))),
                2 => self.e(I::I32Load(mem(addr + 8, 2))),
                _ => self.e(I::I64Load(mem(addr + 8, 3))),
            }
            self.set(bank, reg);
            self.e(I::End);
        }
        self.e(I::End);
        self.e(I::End);
        self.release(ValType::I32, tag);
    }

    /// Leave the function. `value` is the `Ret` register, or `i[b]` for `Return`.
    fn leave(&mut self, kind: u32, value: Option<(usize, u8)>) {
        // Outermost AOT frame: write the exit record for the host.
        self.e(I::GlobalGet(G_DEPTH));
        self.e(I::I32Const(1));
        self.e(I::I32Eq);
        self.e(I::If(BlockType::Empty));
        self.e(I::I32Const(0));
        self.e(I::I32Const(kind as i32));
        self.e(I::I32Store(mem(abi::EXIT, 2)));
        if let Some((bank, reg)) = value {
            self.store_tagged(abi::EXIT + 4, abi::EXIT + 8, bank, reg);
        } else {
            self.e(I::I32Const(0));
            self.e(I::I32Const(0));
            self.e(I::I32Store(mem(abi::EXIT + 4, 2)));
        }
        for k in 0..4u8 {
            self.e(I::I32Const(0));
            self.get(0, k);
            self.e(I::I64Store(mem(abi::EXIT + 16 + 8 * u32::from(k), 3)));
        }
        self.e(I::I32Const(0));
        self.get(2, 0);
        self.e(I::I32Store(mem(abi::EXIT + 48, 2)));
        self.e(I::I32Const(0));
        self.get(1, 0);
        self.e(I::F64Store(mem(abi::EXIT + 56, 3)));
        self.e(I::End);
        if kind == abi::EXIT_RETURN {
            if let Some((_, reg)) = value {
                self.get(0, reg);
                self.e(I::GlobalSet(G_RQ_VAL));
            }
            self.e(I::I32Const(1));
        } else {
            self.e(I::I32Const(0));
        }
        self.e(I::GlobalSet(G_RQ_SET));
        self.e(I::GlobalGet(G_DEPTH));
        self.e(I::I32Const(1));
        self.e(I::I32Sub);
        self.e(I::GlobalSet(G_DEPTH));
        self.e(I::Return);
    }
}
