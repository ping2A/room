//! AOT tier host (`docs/37` §5.3): runs `room-zvm-wasm` modules.
//!
//! The module imports the `"zvm"` runtime defined in [`crate::aot_abi`]. Heap,
//! strings, natives, traps and the interpreter bridge all live in the engine
//! (`ZvmHeap`, `NativeTable`, `ZvmPlayHost`), so the interpreter and the
//! compiled code share one state and can be swapped between calls.
//!
//! Two wasm engines run the same module through the [`Backend`] trait:
//! wasmi (feature `aot`, everywhere, no speed-up) and wasmtime/Cranelift
//! (feature `aot-wasmtime`, desktop JIT). Import bodies are engine-neutral and
//! reach the engine through [`HostCtx`].
//!
//! During a call the [`Zvm`] is moved into the wasm store (and back out
//! afterwards), so imports reach it without `unsafe` aliasing.

mod wasmi_backend;
#[cfg(feature = "aot-wasmtime")]
mod wasmtime_backend;
#[cfg(feature = "aot-web")]
pub mod web_backend;

use crate::aot_abi::{self as abi, ImageLayout, obj_bits, obj_from_bits};
use crate::heap::{ObjHandle, ObjKind, StrHandle};
use crate::interp::{Frame, Value, Zvm, heap_trap, write_value};
use crate::ops::{self, HeapErr};
use crate::trap::Trap;

/// Engine-neutral state reachable from imports.
#[derive(Default)]
pub(crate) struct HostCore {
    vm: Option<Zvm>,
    trap: Option<Trap>,
    compiled: Vec<bool>,
    /// Shipped modules (`docs/37` ZW6) carry `CallNat` ids as indices into
    /// `program.native_imports`; this maps them to this build's native ids.
    native_map: Option<Vec<u32>>,
}

/// Import bodies fail with `Err(Abort)` after recording the trap in [`HostCore`].
pub(crate) struct Abort;

type Hr<T> = Result<T, Abort>;

/// What an import needs from the engine's `Caller`.
pub(crate) trait HostCtx {
    fn core(&mut self) -> &mut HostCore;
    fn mem(&mut self) -> &mut [u8];
    fn fuel(&mut self) -> i64;
    fn set_fuel(&mut self, v: i64);
    fn depth(&mut self) -> i32;
    fn set_rq(&mut self, rq: Option<i64>);
}

/// One instantiated module on one engine.
pub(crate) trait Backend {
    fn core(&mut self) -> &mut HostCore;
    fn mem(&mut self) -> &mut [u8];
    /// Set fuel, depth, limit and clear `rq_set` before a top-level call.
    fn prepare(&mut self, fuel: i64, limit: i32);
    fn fuel(&mut self) -> i64;
    /// Call exported function `f{idx}`.
    fn call(&mut self, idx: usize) -> Result<(), String>;
}

/// Which top-level calls the AOT tier takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AotPolicy {
    /// Functions with a loop or a long body; short straight-line ones stay on
    /// the interpreter, whose entry is cheaper than a wasm call.
    #[default]
    Auto,
    /// Every compiled function (differential tests, `ROOM_ZVM_TIER=aot-all`).
    All,
}

/// The attached AOT tier.
pub(crate) struct AotModule {
    backend: Box<dyn Backend + Send>,
    compiled: Vec<bool>,
    hot: Vec<bool>,
    pub(crate) policy: AotPolicy,
    /// Run compiled functions through wasm (false = interpreter only).
    pub(crate) enabled: bool,
    /// Top-level calls served by wasm.
    pub(crate) calls: u64,
    /// Engine name for logs (`wasmi` / `wasmtime`).
    pub(crate) engine: &'static str,
    /// Per function: registers the prologue loads ([`abi::entry_mask`]), filled lazily.
    masks: Vec<Option<[Vec<bool>; 4]>>,
}

fn vm_of(core: &mut HostCore) -> &mut Zvm {
    match core.vm.as_mut() {
        Some(vm) => vm,
        // The store only runs while a VM is installed (see `AotModule::run`).
        None => unreachable_vm(),
    }
}

#[cold]
fn unreachable_vm() -> ! {
    panic!("zvm aot: import called without a VM in the store")
}

fn fail<T>(core: &mut HostCore, t: Trap) -> Hr<T> {
    core.trap = Some(t);
    Err(Abort)
}

fn site(vm: &Zvm, func: i32, pc: i32) -> (String, u32) {
    let line = vm
        .program
        .functions
        .get(func as usize)
        .and_then(|f| f.lines.get(pc as usize))
        .copied()
        .unwrap_or(0);
    (vm.fname(func as u32), line)
}

fn heap_fail<T>(core: &mut HostCore, e: HeapErr, func: i32, pc: i32) -> Hr<T> {
    let (name, line) = site(vm_of(core), func, pc);
    fail(core, heap_trap(e, name, line))
}

fn rd_u32(m: &[u8], at: u32) -> u32 {
    let at = at as usize;
    u32::from_le_bytes([m[at], m[at + 1], m[at + 2], m[at + 3]])
}

fn rd_u64(m: &[u8], at: u32) -> u64 {
    let at = at as usize;
    let mut b = [0u8; 8];
    b.copy_from_slice(&m[at..at + 8]);
    u64::from_le_bytes(b)
}

fn wr_u32(m: &mut [u8], at: u32, v: u32) {
    let at = at as usize;
    m[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn wr_u64(m: &mut [u8], at: u32, v: u64) {
    let at = at as usize;
    m[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

fn read_cell(m: &[u8], at: u32) -> Option<Value> {
    let payload = rd_u64(m, at + 8);
    match rd_u32(m, at) {
        abi::TAG_I => Some(Value::I(payload as i64)),
        abi::TAG_F => Some(Value::F(f64::from_bits(payload))),
        abi::TAG_S => Some(Value::S(StrHandle(payload as u32))),
        abi::TAG_A => Some(Value::A(obj_from_bits(payload))),
        _ => None,
    }
}

fn write_cell(m: &mut [u8], at: u32, v: Option<Value>) {
    let (tag, payload) = match v {
        None => (abi::TAG_NONE, 0),
        Some(Value::I(x)) => (abi::TAG_I, x as u64),
        Some(Value::F(x)) => (abi::TAG_F, x.to_bits()),
        Some(Value::S(h)) => (abi::TAG_S, u64::from(h.0)),
        Some(Value::A(h)) => (abi::TAG_A, obj_bits(h)),
    };
    wr_u32(m, at, tag);
    wr_u32(m, at + 4, 0);
    wr_u64(m, at + 8, payload);
}

fn read_args(m: &[u8], n: i32) -> Vec<Value> {
    (0..n.clamp(0, abi::MAX_ARGS as i32) as u32)
        .filter_map(|k| read_cell(m, abi::ARGS + k * abi::CELL))
        .collect()
}

fn write_rets(m: &mut [u8], rets: &[Option<Value>; 8]) {
    for (k, v) in rets.iter().enumerate() {
        write_cell(m, abi::RETS + k as u32 * abi::CELL, *v);
    }
}

fn read_rets(m: &[u8]) -> [Option<Value>; 8] {
    let mut out = [None; 8];
    for (k, slot) in out.iter_mut().enumerate() {
        *slot = read_cell(m, abi::RETS + k as u32 * abi::CELL);
    }
    out
}

/// Serialize a frame's registers into the image at [`abi::IMG`].
fn write_image(m: &mut [u8], lay: &ImageLayout, frame: &Frame) {
    let end = (abi::IMG + lay.size) as usize;
    m[abi::IMG as usize..end].fill(0);
    for r in 0..lay.counts[0] {
        let v = frame.i.get(r as usize).copied().unwrap_or(0);
        wr_u64(m, lay.addr(0, r), v as u64);
    }
    for r in 0..lay.counts[1] {
        let v = frame.f.get(r as usize).copied().unwrap_or(0.0);
        wr_u64(m, lay.addr(1, r), v.to_bits());
    }
    for r in 0..lay.counts[2] {
        let v = frame.s.get(r as usize).copied().unwrap_or_default();
        wr_u32(m, lay.addr(2, r), v.0);
    }
    for r in 0..lay.counts[3] {
        let v = frame.a.get(r as usize).copied().unwrap_or_default();
        wr_u64(m, lay.addr(3, r), obj_bits(v));
    }
}

fn blank_frame(vm: &Zvm, func: u32) -> Frame {
    let f = &vm.program.functions[func as usize];
    Frame {
        func,
        pc: 0,
        i: vec![0; f.n_i.max(1) as usize],
        f: vec![0.0; f.n_f.max(1) as usize],
        s: vec![StrHandle::EMPTY; f.n_s.max(1) as usize],
        a: vec![ObjHandle::NULL; f.n_a.max(1) as usize],
        ret: None,
    }
}

// ---- import bodies (engine-neutral) ----

fn imp_trap(h: &mut impl HostCtx, kind: i32, func: i32, pc: i32, extra: i32) -> Hr<()> {
    let core = h.core();
    let vm = vm_of(core);
    let (name, line) = site(vm, func, pc);
    let t = match kind {
        abi::TRAP_DIV_ZERO => Trap::DivZero { func: name, line },
        abi::TRAP_OVERFLOW => Trap::Overflow { func: name, line },
        abi::TRAP_EXPLICIT => {
            let msg = vm
                .program
                .functions
                .get(func as usize)
                .and_then(|f| f.const_s.get(extra as u32 as usize))
                .cloned()
                .unwrap_or_else(|| "trap".into());
            Trap::Explicit {
                msg,
                func: name,
                line,
            }
        }
        abi::TRAP_FUEL => Trap::FuelExhausted { func: name },
        abi::TRAP_STACK => Trap::StackOverflow { func: name },
        _ => Trap::Native(format!("Call bad func index {}", extra as u32)),
    };
    fail(core, t)
}

fn imp_str_const(h: &mut impl HostCtx, func: i32, idx: i32) -> i32 {
    let vm = vm_of(h.core());
    let lit = vm
        .program
        .functions
        .get(func as usize)
        .and_then(|f| f.const_s.get(idx as u32 as usize))
        .cloned()
        .unwrap_or_default();
    vm.heap.intern_string(&lit).0 as i32
}

fn imp_concat(h: &mut impl HostCtx, a: i32, b: i32) -> i32 {
    let vm = vm_of(h.core());
    vm.heap.concat(StrHandle(a as u32), StrHandle(b as u32)).0 as i32
}

fn imp_new_obj(h: &mut impl HostCtx, class: i32) -> i64 {
    let vm = vm_of(h.core());
    obj_bits(ops::new_obj(&mut vm.heap, &vm.program, class as u32)) as i64
}

fn imp_is_class(h: &mut impl HostCtx, o: i64, class: i32) -> i64 {
    let vm = vm_of(h.core());
    i64::from(ops::is_class(
        &vm.heap,
        &vm.program,
        obj_from_bits(o as u64),
        class as u32,
    ))
}

fn imp_is_null(h: &mut impl HostCtx, o: i64) -> i64 {
    let vm = vm_of(h.core());
    i64::from(ops::is_null(&vm.heap, obj_from_bits(o as u64)))
}

/// Run a heap op on the VM; failures become traps at `(func, pc)`.
fn heap_op<T>(
    h: &mut impl HostCtx,
    func: i32,
    pc: i32,
    op: impl FnOnce(&mut Zvm) -> ops::HeapResult<T>,
) -> Hr<T> {
    let core = h.core();
    let r = op(vm_of(core));
    r.or_else(|e| heap_fail(core, e, func, pc))
}

fn imp_native(h: &mut impl HostCtx, id: i32, n: i32, b: i32) -> Hr<()> {
    let args = read_args(h.mem(), n);
    let core = h.core();
    let id = match &core.native_map {
        Some(map) => map.get(id as u32 as usize).copied().unwrap_or(u32::MAX) as i32,
        None => id,
    };
    let vm = vm_of(core);
    if let Err(t) = vm.native_call(id as u32, args, b as u8) {
        return fail(core, t);
    }
    let rets = vm.rets;
    write_rets(h.mem(), &rets);
    Ok(())
}

/// Run `callee` on the interpreter with `args`, as a `Call` from AOT code.
fn bridge(h: &mut impl HostCtx, callee: u32, args: Vec<Value>) -> Hr<()> {
    let depth = h.depth();
    let fuel = h.fuel() as u64;
    let core = h.core();
    let vm = vm_of(core);
    vm.rets = [None; 8];
    vm.fuel_left = fuel;
    vm.bridge_floor = 1;
    vm.bridge_rq = None;
    vm.depth_base = (depth.max(1) - 1) as usize;
    let dummy = blank_frame(vm, callee);
    let mut stack = vec![dummy];
    let r = vm
        .enter_call(&mut stack, callee, args)
        .and_then(|()| vm.run_frames(&mut stack));
    vm.bridge_floor = 0;
    vm.depth_base = 0;
    let rq = vm.bridge_rq.take();
    let fuel = vm.fuel_left;
    let rets = vm.rets;
    if let Err(t) = r {
        core.trap = Some(t);
    }
    let failed = core.trap.is_some();
    h.set_fuel(fuel as i64);
    write_rets(h.mem(), &rets);
    if failed {
        return Err(Abort);
    }
    h.set_rq(rq);
    Ok(())
}

fn imp_call_interp(h: &mut impl HostCtx, callee: i32, n: i32) -> Hr<()> {
    let args = read_args(h.mem(), n);
    bridge(h, callee as u32, args)
}

fn imp_vcall(h: &mut impl HostCtx, slot: i32, n: i32) -> Hr<i32> {
    let args = read_args(h.mem(), n);
    let core = h.core();
    let compiled = std::mem::take(&mut core.compiled);
    let vm = vm_of(core);
    let recv = args.iter().find_map(|v| match v {
        Value::A(h) => Some(*h),
        _ => None,
    });
    let class_id = recv
        .and_then(|h| match vm.heap.get(h) {
            Some(ObjKind::Object { class_id, .. }) => Some(*class_id),
            _ => None,
        })
        .unwrap_or(0);
    let slot = slot as u32 as usize;
    let target = vm
        .program
        .vtables
        .get(&class_id)
        .and_then(|vt| vt.get(slot).copied());
    let is_compiled = target.is_some_and(|t| compiled.get(t as usize).copied().unwrap_or(false));
    core.compiled = compiled;
    let Some(target) = target else {
        return fail(
            core,
            Trap::Native(format!("CallVirt class {class_id} slot {slot} is unbound")),
        );
    };
    if !is_compiled {
        bridge(h, target, args)?;
        return Ok(-1);
    }
    // `enter_call` semantics: zero frame, then `write_value` per param.
    let vm = vm_of(h.core());
    let mut frame = blank_frame(vm, target);
    let params = vm.program.functions[target as usize].params.clone();
    for (p, v) in params.iter().zip(args) {
        write_value(&mut frame, p.bank, p.reg as u8, v);
    }
    let lay = ImageLayout::of(&vm.program.functions[target as usize]);
    write_image(h.mem(), &lay, &frame);
    Ok(target as i32)
}

/// Link every `"zvm"` import into `$linker` for the engine's `Caller` type.
/// `$err` turns an [`Abort`] into the engine's trap error.
macro_rules! link_imports {
    ($linker:expr, $caller:ty, $err:expr) => {{
        use crate::aot as a;
        let l = $linker;
        let m = crate::aot_abi::IMPORT_MODULE;
        let e = $err;
        l.func_wrap(
            m,
            "trap",
            move |mut c: $caller, k: i32, f: i32, p: i32, x: i32| {
                a::imp_trap(&mut c, k, f, p, x).map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(m, "str_const", |mut c: $caller, f: i32, i: i32| {
            a::imp_str_const(&mut c, f, i)
        })
        .map_err(|e| e.to_string())?;
        l.func_wrap(m, "concat", |mut c: $caller, x: i32, y: i32| {
            a::imp_concat(&mut c, x, y)
        })
        .map_err(|e| e.to_string())?;
        l.func_wrap(m, "new_obj", |mut c: $caller, k: i32| {
            a::imp_new_obj(&mut c, k)
        })
        .map_err(|e| e.to_string())?;
        l.func_wrap(m, "is_class", |mut c: $caller, o: i64, k: i32| {
            a::imp_is_class(&mut c, o, k)
        })
        .map_err(|e| e.to_string())?;
        l.func_wrap(m, "is_null", |mut c: $caller, o: i64| {
            a::imp_is_null(&mut c, o)
        })
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "ld_i",
            move |mut c: $caller, o: i64, x: i32, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::ld_i(&vm.heap, a::obj(o), x as u32 as usize)
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "ld_f",
            move |mut c: $caller, o: i64, x: i32, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::ld_f(&vm.heap, a::obj(o), x as u32 as usize)
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "ld_s",
            move |mut c: $caller, o: i64, x: i32, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::ld_s(&vm.heap, a::obj(o), x as u32 as usize)
                })
                .map(|s| s.0 as i32)
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "ld_a",
            move |mut c: $caller, o: i64, x: i32, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::ld_a(&vm.heap, a::obj(o), x as u32 as usize)
                })
                .map(|h| crate::aot_abi::obj_bits(h) as i64)
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "st_i",
            move |mut c: $caller, o: i64, x: i32, v: i64, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::st_i(&mut vm.heap, a::obj(o), x as u32 as usize, v)
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "st_f",
            move |mut c: $caller, o: i64, x: i32, v: f64, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::st_f(&mut vm.heap, a::obj(o), x as u32 as usize, v)
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "st_s",
            move |mut c: $caller, o: i64, x: i32, v: i32, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::st_s(
                        &mut vm.heap,
                        a::obj(o),
                        x as u32 as usize,
                        crate::heap::StrHandle(v as u32),
                    )
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "st_a",
            move |mut c: $caller, o: i64, x: i32, v: i64, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::st_a(&mut vm.heap, a::obj(o), x as u32 as usize, a::obj(v))
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "arr_len",
            move |mut c: $caller, o: i64, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| crate::ops::arr_len(&vm.heap, a::obj(o)))
                    .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "arr_get_i",
            move |mut c: $caller, o: i64, i: i64, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::arr_get_i(&vm.heap, a::obj(o), i)
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "arr_set_i",
            move |mut c: $caller, o: i64, i: i64, v: i64, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::arr_set_i(&mut vm.heap, a::obj(o), i, v)
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "arr_push_i",
            move |mut c: $caller, o: i64, v: i64, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::arr_push_i(&mut vm.heap, a::obj(o), v)
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "arr_get_s",
            move |mut c: $caller, o: i64, i: i64, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::arr_get_s(&vm.heap, a::obj(o), i)
                })
                .map(|s| s.0 as i32)
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "arr_set_s",
            move |mut c: $caller, o: i64, i: i64, v: i32, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::arr_set_s(
                        &mut vm.heap,
                        a::obj(o),
                        i,
                        crate::heap::StrHandle(v as u32),
                    )
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "arr_push_s",
            move |mut c: $caller, o: i64, v: i32, f: i32, p: i32| {
                a::heap_op(&mut c, f, p, |vm| {
                    crate::ops::arr_push_s(
                        &mut vm.heap,
                        a::obj(o),
                        crate::heap::StrHandle(v as u32),
                    )
                })
                .map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "native",
            move |mut c: $caller, id: i32, n: i32, b: i32, _f: i32, _p: i32| {
                a::imp_native(&mut c, id, n, b).map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "call_interp",
            move |mut c: $caller, k: i32, n: i32, _f: i32, _p: i32| {
                a::imp_call_interp(&mut c, k, n).map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
        l.func_wrap(
            m,
            "vcall",
            move |mut c: $caller, s: i32, n: i32, _f: i32, _p: i32| {
                a::imp_vcall(&mut c, s, n).map_err(|_| e())
            },
        )
        .map_err(|e| e.to_string())?;
    }};
}
pub(crate) use link_imports;

pub(crate) fn obj(v: i64) -> ObjHandle {
    obj_from_bits(v as u64)
}

/// Two independent 64-bit FNV-style digests plus the length.
pub(crate) fn digest(bytes: &[u8]) -> (u64, u64, usize) {
    let mut a: u64 = 0xcbf2_9ce4_8422_2325;
    let mut b: u64 = 0x6c62_272e_07bb_0142;
    for &x in bytes {
        a = (a ^ u64::from(x)).wrapping_mul(0x0000_0100_0000_01b3);
        b = (b ^ u64::from(x))
            .wrapping_mul(0x0000_0001_0000_01b3)
            .rotate_left(5);
    }
    (a, b, bytes.len())
}

impl AotModule {
    /// Instantiate `bytes` on the preferred engine: wasmtime when built with
    /// `aot-wasmtime` (unless `ROOM_ZVM_ENGINE=wasmi`), else wasmi.
    pub(crate) fn new(bytes: &[u8], compiled: Vec<bool>) -> Result<Self, String> {
        // The store keeps a placeholder VM; calls swap the real one in and out.
        let core = HostCore {
            vm: Some(Zvm::empty()),
            trap: None,
            compiled: compiled.clone(),
            native_map: None,
        };
        let masks = vec![None; compiled.len()];
        let want_wasmi =
            std::env::var("ROOM_ZVM_ENGINE").is_ok_and(|v| v.eq_ignore_ascii_case("wasmi"));
        #[cfg(feature = "aot-wasmtime")]
        if !want_wasmi {
            let backend = wasmtime_backend::WtBackend::new(bytes, &compiled, core)?;
            return Ok(Self {
                backend: Box::new(backend),
                hot: compiled.clone(),
                policy: AotPolicy::All,
                compiled,
                enabled: true,
                calls: 0,
                engine: "wasmtime",
                masks,
            });
        }
        let _ = want_wasmi;
        let backend = wasmi_backend::WiBackend::new(bytes, &compiled, core)?;
        Ok(Self {
            backend: Box::new(backend),
            hot: compiled.clone(),
            policy: AotPolicy::All,
            compiled,
            enabled: true,
            calls: 0,
            engine: "wasmi",
            masks,
        })
    }

    /// True when function `idx` runs as wasm.
    pub(crate) fn serves(&self, idx: u32) -> bool {
        let i = idx as usize;
        self.enabled
            && self.compiled.get(i).copied().unwrap_or(false)
            && (self.policy == AotPolicy::All || self.hot.get(i).copied().unwrap_or(false))
    }

    /// True when every non-zero register of `frame` is one the prologue loads.
    fn entry_fits(&mut self, vm: &Zvm, frame: &Frame) -> bool {
        let Some(f) = vm.program.functions.get(frame.func as usize) else {
            return false;
        };
        let Some(slot) = self.masks.get_mut(frame.func as usize) else {
            return false;
        };
        let mask = slot.get_or_insert_with(|| abi::entry_mask(f));
        let ok = |b: usize, nonzero: &dyn Fn(usize) -> bool, len: usize| {
            (0..len).all(|r| !nonzero(r) || mask[b].get(r).copied().unwrap_or(false))
        };
        ok(0, &|r| frame.i[r] != 0, frame.i.len())
            && ok(1, &|r| frame.f[r].to_bits() != 0, frame.f.len())
            && ok(2, &|r| frame.s[r] != StrHandle::EMPTY, frame.s.len())
            && ok(3, &|r| frame.a[r] != ObjHandle::NULL, frame.a.len())
    }

    /// Run the outermost frame `frame` as wasm. Mirrors `run_with_result`.
    pub(crate) fn run(&mut self, vm: &mut Zvm, frame: &Frame) -> Result<i64, Trap> {
        let idx = frame.func as usize;
        if !self.compiled.get(idx).copied().unwrap_or(false) {
            return Err(Trap::Native(format!("aot: function {idx} not compiled")));
        }
        let lay = ImageLayout::of(&vm.program.functions[idx]);
        {
            let m = self.backend.mem();
            write_image(m, &lay, frame);
            write_rets(m, &vm.rets);
        }
        let limit =
            i32::try_from(vm.config.stack_limit.saturating_sub(vm.depth_base)).unwrap_or(i32::MAX);
        self.backend.prepare(vm.fuel_left as i64, limit);
        self.calls += 1;
        // Swap the VM into the store (the store's placeholder comes out).
        if let Some(slot) = self.backend.core().vm.as_mut() {
            std::mem::swap(vm, slot);
        }
        let r = self.backend.call(idx);
        if let Some(slot) = self.backend.core().vm.as_mut() {
            std::mem::swap(vm, slot);
        }
        vm.fuel_left = self.backend.fuel() as u64;
        vm.rets = read_rets(self.backend.mem());
        if let Err(e) = r {
            let t = self.backend.core().trap.take();
            return Err(t.unwrap_or_else(|| Trap::Native(format!("aot: {e}"))));
        }
        let m = self.backend.mem();
        let kind = rd_u32(m, abi::EXIT);
        let tag = rd_u32(m, abi::EXIT + 4);
        let payload = rd_u64(m, abi::EXIT + 8);
        for k in 0..4u32 {
            vm.last_i[k as usize] = rd_u64(m, abi::EXIT + 16 + 8 * k) as i64;
        }
        let s0 = StrHandle(rd_u32(m, abi::EXIT + 48));
        let f0 = f64::from_bits(rd_u64(m, abi::EXIT + 56));
        vm.last_s = vm.heap.str(s0).to_owned();
        let mut result = 0i64;
        match kind {
            abi::EXIT_RET => match tag {
                abi::TAG_I => result = payload as i64,
                abi::TAG_F => vm.last_f = f64::from_bits(payload),
                abi::TAG_S => vm.last_s = vm.heap.str(StrHandle(payload as u32)).to_owned(),
                _ => {}
            },
            abi::EXIT_RETURN => {
                result = payload as i64;
                vm.last_f = f0;
            }
            _ => result = vm.last_i[0],
        }
        Ok(result)
    }
}

impl Zvm {
    /// Attach a `room-zvm-wasm` module (`docs/37` ZW3) built from this VM's
    /// `program`. `compiled[i]` says which functions the module contains.
    ///
    /// The tier is on unless `ROOM_ZVM_TIER=interp` is set.
    pub fn attach_aot(&mut self, bytes: &[u8], compiled: Vec<bool>) -> Result<(), String> {
        let hot = compiled.clone();
        self.attach_aot_with(bytes, compiled, hot)
    }

    /// [`Self::attach_aot`] with per-function `hot` flags for the
    /// [`AotPolicy::Auto`] policy. `ROOM_ZVM_TIER=aot-all` selects
    /// [`AotPolicy::All`].
    pub fn attach_aot_with(
        &mut self,
        bytes: &[u8],
        compiled: Vec<bool>,
        hot: Vec<bool>,
    ) -> Result<(), String> {
        if compiled.len() != self.program.functions.len() || hot.len() != compiled.len() {
            return Err("aot module does not match the program".into());
        }
        let mut m = AotModule::new(bytes, compiled)?;
        let tier = std::env::var("ROOM_ZVM_TIER").unwrap_or_default();
        m.enabled = !tier.eq_ignore_ascii_case("interp");
        m.policy = if tier.eq_ignore_ascii_case("aot-all") {
            AotPolicy::All
        } else {
            AotPolicy::Auto
        };
        m.hot = hot;
        self.aot = Some(Box::new(m));
        Ok(())
    }

    /// Attach a module shipped with a converted mod (`docs/37` ZW6): it was
    /// compiled from the unlinked program, so its native calls name
    /// `program.native_imports` indices, mapped here onto this build's table.
    pub fn attach_shipped(
        &mut self,
        bytes: &[u8],
        compiled: Vec<bool>,
        hot: Vec<bool>,
    ) -> Result<(), String> {
        let map: Vec<u32> = self
            .program
            .native_imports
            .iter()
            .map(|n| self.natives.id_ci(n).unwrap_or(u32::MAX))
            .collect();
        self.attach_aot_with(bytes, compiled, hot)?;
        if let Some(m) = self.aot.as_mut() {
            m.backend.core().native_map = Some(map);
        }
        Ok(())
    }

    /// Choose which top-level calls the AOT tier takes.
    pub fn set_aot_policy(&mut self, policy: AotPolicy) {
        if let Some(m) = self.aot.as_mut() {
            m.policy = policy;
        }
    }

    /// Turn the attached AOT tier on or off (no-op without one).
    pub fn set_aot_enabled(&mut self, on: bool) {
        if let Some(m) = self.aot.as_mut() {
            m.enabled = on;
        }
    }

    /// True when an AOT module is attached and enabled.
    #[must_use]
    pub fn aot_active(&self) -> bool {
        self.aot.as_ref().is_some_and(|m| m.enabled)
    }

    /// Wasm engine running the AOT tier (`wasmtime` / `wasmi`), if attached.
    #[must_use]
    pub fn aot_engine(&self) -> Option<&'static str> {
        self.aot.as_ref().map(|m| m.engine)
    }

    /// Top-level calls served by the AOT tier so far.
    #[must_use]
    pub fn aot_calls(&self) -> u64 {
        self.aot.as_ref().map_or(0, |m| m.calls)
    }

    /// Run `frame` through the AOT tier when it serves that function.
    pub(crate) fn try_aot(&mut self, frame: &Frame) -> Option<Result<i64, Trap>> {
        if !self.aot.as_ref()?.serves(frame.func) {
            return None;
        }
        let mut m = self.aot.take()?;
        let r = if m.entry_fits(self, frame) {
            Some(m.run(self, frame))
        } else {
            None
        };
        self.aot = Some(m);
        r
    }
}
