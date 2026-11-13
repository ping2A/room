//! Browser engine for the AOT tier (`docs/37` ZW4): the module is compiled
//! and instantiated by the browser's own `WebAssembly` (its JIT), and the
//! `"zvm"` imports are JavaScript closures that call back into the engine.
//!
//! Browsers refuse synchronous compilation of modules over 4 KiB on the main
//! thread, so [`instantiate`] is async; the host keeps interpreting until the
//! instance is ready and then attaches it with [`WebInstance::attach`].
//!
//! The guest memory is a separate `ArrayBuffer`. Imports that touch it work on
//! a Rust copy of its first [`SCRATCH`] bytes (cells, exit record, register
//! image) and write it back afterwards; a top-level call copies the scratch
//! in before and out after.

use super::{AotModule, AotPolicy, Backend, HostCore, HostCtx};
use crate::aot_abi as abi;
use crate::interp::Zvm;
use js_sys::{Function, Object, Reflect, Uint8Array, WebAssembly};
use std::cell::{RefCell, RefMut};
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

/// Bytes of guest memory mirrored on the Rust side (cells + image).
pub const SCRATCH: usize = 16 * 1024;

struct Globals {
    fuel: WebAssembly::Global,
    depth: WebAssembly::Global,
    limit: WebAssembly::Global,
    rq_set: WebAssembly::Global,
    rq_val: WebAssembly::Global,
}

#[derive(Default)]
struct WebState {
    core: Option<HostCore>,
    scratch: Vec<u8>,
    memory: Option<WebAssembly::Memory>,
    globals: Option<Globals>,
}

fn copy_in(memory: &WebAssembly::Memory, scratch: &mut [u8]) {
    let view = Uint8Array::new(&memory.buffer());
    view.subarray(0, scratch.len() as u32).copy_to(scratch);
}

fn copy_out(memory: &WebAssembly::Memory, scratch: &[u8]) {
    let view = Uint8Array::new(&memory.buffer());
    view.subarray(0, scratch.len() as u32).copy_from(scratch);
}

fn get_i64(g: &WebAssembly::Global) -> i64 {
    let v = g.value();
    if let Some(f) = v.as_f64() {
        return f as i64;
    }
    i64::try_from(v).unwrap_or(0)
}

fn set_i64(g: &WebAssembly::Global, v: i64) {
    g.set_value(&JsValue::from(js_sys::BigInt::from(v)));
}

fn set_i32(g: &WebAssembly::Global, v: i32) {
    g.set_value(&JsValue::from(v));
}

/// Import-side view of the state, alive for one import call.
struct Ctx<'a> {
    st: RefMut<'a, WebState>,
    loaded: bool,
}

impl Drop for Ctx<'_> {
    fn drop(&mut self) {
        if self.loaded {
            if let Some(m) = &self.st.memory {
                copy_out(m, &self.st.scratch);
            }
        }
    }
}

impl HostCtx for Ctx<'_> {
    fn core(&mut self) -> &mut HostCore {
        match self.st.core.as_mut() {
            Some(c) => c,
            None => panic!("zvm aot (web): import outside a call"),
        }
    }

    fn mem(&mut self) -> &mut [u8] {
        if !self.loaded {
            self.loaded = true;
            let st = &mut *self.st;
            if let Some(m) = &st.memory {
                copy_in(m, &mut st.scratch);
            }
        }
        &mut self.st.scratch
    }

    fn fuel(&mut self) -> i64 {
        self.st.globals.as_ref().map_or(0, |g| get_i64(&g.fuel))
    }

    fn set_fuel(&mut self, v: i64) {
        if let Some(g) = &self.st.globals {
            set_i64(&g.fuel, v);
        }
    }

    fn depth(&mut self) -> i32 {
        self.st.globals.as_ref().map_or(0, |g| get_i64(&g.depth) as i32)
    }

    fn set_rq(&mut self, rq: Option<i64>) {
        if let Some(g) = &self.st.globals {
            set_i32(&g.rq_set, i32::from(rq.is_some()));
            set_i64(&g.rq_val, rq.unwrap_or(0));
        }
    }
}

fn trap_value() -> JsValue {
    JsValue::from_str("zvm trap")
}

fn put(obj: &Object, name: &str, f: JsValue) -> Result<(), String> {
    Reflect::set(obj, &JsValue::from_str(name), &f)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

/// Build the `{ zvm: { … } }` import object bound to `st`.
#[allow(clippy::too_many_lines)]
fn imports(st: &Rc<RefCell<WebState>>) -> Result<Object, String> {
    use super as a;
    let zvm = Object::new();
    macro_rules! ctx {
        ($st:expr) => {
            Ctx {
                st: $st.borrow_mut(),
                loaded: false,
            }
        };
    }
    let s = Rc::clone(st);
    put(
        &zvm,
        "trap",
        Closure::<dyn Fn(i32, i32, i32, i32) -> Result<(), JsValue>>::new(move |k, f, p, x| {
            a::imp_trap(&mut ctx!(s), k, f, p, x).map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "str_const",
        Closure::<dyn Fn(i32, i32) -> i32>::new(move |f, i| a::imp_str_const(&mut ctx!(s), f, i))
            .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "concat",
        Closure::<dyn Fn(i32, i32) -> i32>::new(move |x, y| a::imp_concat(&mut ctx!(s), x, y))
            .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "new_obj",
        Closure::<dyn Fn(i32) -> i64>::new(move |k| a::imp_new_obj(&mut ctx!(s), k)).into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "is_class",
        Closure::<dyn Fn(i64, i32) -> i64>::new(move |o, k| a::imp_is_class(&mut ctx!(s), o, k))
            .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "is_null",
        Closure::<dyn Fn(i64) -> i64>::new(move |o| a::imp_is_null(&mut ctx!(s), o)).into_js_value(),
    )?;
    macro_rules! load {
        ($name:literal, $ret:ty, $op:path, $map:expr) => {{
            let s = Rc::clone(st);
            put(
                &zvm,
                $name,
                Closure::<dyn Fn(i64, i32, i32, i32) -> Result<$ret, JsValue>>::new(
                    move |o, x, f, p| {
                        a::heap_op(&mut ctx!(s), f, p, |vm| $op(&vm.heap, a::obj(o), x as u32 as usize))
                            .map($map)
                            .map_err(|_| trap_value())
                    },
                )
                .into_js_value(),
            )?;
        }};
    }
    load!("ld_i", i64, crate::ops::ld_i, |v| v);
    load!("ld_f", f64, crate::ops::ld_f, |v| v);
    load!("ld_s", i32, crate::ops::ld_s, |v: crate::heap::StrHandle| v.0 as i32);
    load!("ld_a", i64, crate::ops::ld_a, |v| abi::obj_bits(v) as i64);
    let s = Rc::clone(st);
    put(
        &zvm,
        "st_i",
        Closure::<dyn Fn(i64, i32, i64, i32, i32) -> Result<(), JsValue>>::new(move |o, x, v, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| crate::ops::st_i(&mut vm.heap, a::obj(o), x as u32 as usize, v))
                .map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "st_f",
        Closure::<dyn Fn(i64, i32, f64, i32, i32) -> Result<(), JsValue>>::new(move |o, x, v, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| crate::ops::st_f(&mut vm.heap, a::obj(o), x as u32 as usize, v))
                .map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "st_s",
        Closure::<dyn Fn(i64, i32, i32, i32, i32) -> Result<(), JsValue>>::new(move |o, x, v, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| {
                crate::ops::st_s(&mut vm.heap, a::obj(o), x as u32 as usize, crate::heap::StrHandle(v as u32))
            })
            .map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "st_a",
        Closure::<dyn Fn(i64, i32, i64, i32, i32) -> Result<(), JsValue>>::new(move |o, x, v, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| {
                crate::ops::st_a(&mut vm.heap, a::obj(o), x as u32 as usize, a::obj(v))
            })
            .map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "arr_len",
        Closure::<dyn Fn(i64, i32, i32) -> Result<i64, JsValue>>::new(move |o, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| crate::ops::arr_len(&vm.heap, a::obj(o))).map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "arr_get_i",
        Closure::<dyn Fn(i64, i64, i32, i32) -> Result<i64, JsValue>>::new(move |o, i, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| crate::ops::arr_get_i(&vm.heap, a::obj(o), i))
                .map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "arr_set_i",
        Closure::<dyn Fn(i64, i64, i64, i32, i32) -> Result<(), JsValue>>::new(move |o, i, v, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| crate::ops::arr_set_i(&mut vm.heap, a::obj(o), i, v))
                .map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "arr_push_i",
        Closure::<dyn Fn(i64, i64, i32, i32) -> Result<(), JsValue>>::new(move |o, v, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| crate::ops::arr_push_i(&mut vm.heap, a::obj(o), v))
                .map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "arr_get_s",
        Closure::<dyn Fn(i64, i64, i32, i32) -> Result<i32, JsValue>>::new(move |o, i, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| crate::ops::arr_get_s(&vm.heap, a::obj(o), i))
                .map(|h| h.0 as i32)
                .map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "arr_set_s",
        Closure::<dyn Fn(i64, i64, i32, i32, i32) -> Result<(), JsValue>>::new(move |o, i, v, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| {
                crate::ops::arr_set_s(&mut vm.heap, a::obj(o), i, crate::heap::StrHandle(v as u32))
            })
            .map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "arr_push_s",
        Closure::<dyn Fn(i64, i32, i32, i32) -> Result<(), JsValue>>::new(move |o, v, f, p| {
            a::heap_op(&mut ctx!(s), f, p, |vm| {
                crate::ops::arr_push_s(&mut vm.heap, a::obj(o), crate::heap::StrHandle(v as u32))
            })
            .map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "native",
        Closure::<dyn Fn(i32, i32, i32, i32, i32) -> Result<(), JsValue>>::new(move |id, n, b, _f, _p| {
            a::imp_native(&mut ctx!(s), id, n, b).map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "call_interp",
        Closure::<dyn Fn(i32, i32, i32, i32) -> Result<(), JsValue>>::new(move |k, n, _f, _p| {
            a::imp_call_interp(&mut ctx!(s), k, n).map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let s = Rc::clone(st);
    put(
        &zvm,
        "vcall",
        Closure::<dyn Fn(i32, i32, i32, i32) -> Result<i32, JsValue>>::new(move |sl, n, _f, _p| {
            a::imp_vcall(&mut ctx!(s), sl, n).map_err(|_| trap_value())
        })
        .into_js_value(),
    )?;
    let root = Object::new();
    put(&root, abi::IMPORT_MODULE, zvm.into())?;
    Ok(root)
}

/// An instantiated module shared by every VM of one program (calls never
/// overlap on the single browser thread; each top-level call resets the
/// guest's globals and image).
#[derive(Clone)]
pub struct WebInstance {
    st: Rc<RefCell<WebState>>,
    funcs: Rc<Vec<Option<Function>>>,
    compiled: Vec<bool>,
    hot: Vec<bool>,
    /// The module carries `native_imports` indices (`Target::RoomMod`).
    room_mod: bool,
}

/// Compile and instantiate `bytes` with the browser's `WebAssembly`.
pub async fn instantiate(
    bytes: Vec<u8>,
    compiled: Vec<bool>,
    hot: Vec<bool>,
    room_mod: bool,
) -> Result<WebInstance, String> {
    let st = Rc::new(RefCell::new(WebState {
        scratch: vec![0; SCRATCH],
        ..WebState::default()
    }));
    let imports = imports(&st)?;
    let promise: js_sys::Promise = WebAssembly::instantiate_buffer(&bytes, &imports).unchecked_into();
    let result = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("instantiate: {e:?}"))?;
    let get = |obj: &JsValue, key: &str| {
        Reflect::get(obj, &JsValue::from_str(key)).map_err(|e| format!("{key}: {e:?}"))
    };
    let instance = get(&result, "instance")?;
    let exports = get(&instance, "exports")?;
    let memory: WebAssembly::Memory = get(&exports, abi::MEMORY)?
        .dyn_into()
        .map_err(|_| "aot module has no memory".to_owned())?;
    let global = |name: &str| -> Result<WebAssembly::Global, String> {
        get(&exports, name)?
            .dyn_into()
            .map_err(|_| format!("aot module lacks {name}"))
    };
    let globals = Globals {
        fuel: global(abi::GLOBAL_FUEL)?,
        depth: global(abi::GLOBAL_DEPTH)?,
        limit: global(abi::GLOBAL_LIMIT)?,
        rq_set: global(abi::GLOBAL_RQ_SET)?,
        rq_val: global(abi::GLOBAL_RQ_VAL)?,
    };
    let mut funcs = Vec::with_capacity(compiled.len());
    for (i, c) in compiled.iter().enumerate() {
        funcs.push(if *c {
            Some(
                get(&exports, &abi::func_export(i as u32))?
                    .dyn_into::<Function>()
                    .map_err(|_| format!("f{i} is not a function"))?,
            )
        } else {
            None
        });
    }
    {
        let mut s = st.borrow_mut();
        s.memory = Some(memory);
        s.globals = Some(globals);
    }
    Ok(WebInstance {
        st,
        funcs: Rc::new(funcs),
        compiled,
        hot,
        room_mod,
    })
}

impl WebInstance {
    /// Attach this instance as `vm`'s AOT tier (`vm.program` must be the
    /// program the module was built from).
    pub fn attach(&self, vm: &mut Zvm) -> Result<(), String> {
        if self.compiled.len() != vm.program.functions.len() {
            return Err("web aot module does not match the program".into());
        }
        let native_map = self.room_mod.then(|| {
            vm.program
                .native_imports
                .iter()
                .map(|n| vm.natives.id_ci(n).unwrap_or(u32::MAX))
                .collect()
        });
        let core = HostCore {
            vm: Some(Zvm::empty()),
            trap: None,
            compiled: self.compiled.clone(),
            native_map,
        };
        let backend = WebBackend {
            inst: self.clone(),
            core: Some(core),
            scratch: vec![0; SCRATCH],
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = backend;
            return Err("the browser AOT engine only runs in wasm32 builds".into());
        }
        #[cfg(target_arch = "wasm32")]
        let backend: Box<dyn Backend + Send> = Box::new(backend);
        let tier = std::env::var("ROOM_ZVM_TIER").unwrap_or_default();
        #[cfg(target_arch = "wasm32")]
        let module = AotModule {
            backend,
            hot: self.hot.clone(),
            policy: if tier.eq_ignore_ascii_case("aot-all") {
                AotPolicy::All
            } else {
                AotPolicy::Auto
            },
            masks: vec![None; self.compiled.len()],
            compiled: self.compiled.clone(),
            enabled: !tier.eq_ignore_ascii_case("interp"),
            calls: 0,
            engine: "browser",
        };
        #[cfg(target_arch = "wasm32")]
        {
            vm.aot = Some(Box::new(module));
            Ok(())
        }
    }
}

struct WebBackend {
    inst: WebInstance,
    /// Owned between calls; moved into the shared state during a call.
    core: Option<HostCore>,
    scratch: Vec<u8>,
}

// The browser build is single-threaded (no wasm threads); `Zvm` is `Send`
// for native hosts, so the backend has to claim it. It is never sent.
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
unsafe impl Send for WebBackend {}

impl Backend for WebBackend {
    fn core(&mut self) -> &mut HostCore {
        match self.core.as_mut() {
            Some(c) => c,
            None => panic!("zvm aot (web): core is in use"),
        }
    }

    fn mem(&mut self) -> &mut [u8] {
        &mut self.scratch
    }

    fn prepare(&mut self, fuel: i64, limit: i32) {
        let st = self.inst.st.borrow();
        if let Some(g) = &st.globals {
            set_i64(&g.fuel, fuel);
            set_i32(&g.depth, 0);
            set_i32(&g.limit, limit);
            set_i32(&g.rq_set, 0);
        }
    }

    fn fuel(&mut self) -> i64 {
        self.inst
            .st
            .borrow()
            .globals
            .as_ref()
            .map_or(0, |g| get_i64(&g.fuel))
    }

    fn call(&mut self, idx: usize) -> Result<(), String> {
        let Some(Some(f)) = self.inst.funcs.get(idx).cloned() else {
            return Err(format!("function {idx} not compiled"));
        };
        {
            let mut st = self.inst.st.borrow_mut();
            st.core = self.core.take();
            if let Some(m) = &st.memory {
                copy_out(m, &self.scratch);
            }
        }
        let r = f.call0(&JsValue::NULL).map(|_| ()).map_err(|e| format!("{e:?}"));
        let mut st = self.inst.st.borrow_mut();
        self.core = st.core.take();
        if let Some(m) = &st.memory {
            copy_in(m, &mut self.scratch);
        }
        r
    }
}
