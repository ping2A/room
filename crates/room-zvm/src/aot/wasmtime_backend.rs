//! wasmtime (Cranelift JIT) engine for the AOT tier (desktop).

use super::{Backend, HostCore, HostCtx, digest, link_imports};
use crate::aot_abi as abi;
use wasmtime::{Caller, Config, Engine, Global, Linker, Memory, Module, Store, TypedFunc, Val};

#[derive(Clone, Copy)]
struct Globals {
    fuel: Global,
    depth: Global,
    limit: Global,
    rq_set: Global,
    rq_val: Global,
}

pub(crate) struct WtData {
    core: HostCore,
    memory: Option<Memory>,
    g: Option<Globals>,
}

fn globals(d: &WtData) -> Globals {
    match d.g {
        Some(g) => g,
        None => panic!("zvm aot: globals not bound"),
    }
}

fn get_i64(g: Global, ctx: &mut Caller<'_, WtData>) -> i64 {
    match g.get(ctx) {
        Val::I64(v) => v,
        Val::I32(v) => i64::from(v),
        _ => 0,
    }
}

impl HostCtx for Caller<'_, WtData> {
    fn core(&mut self) -> &mut HostCore {
        &mut self.data_mut().core
    }

    fn mem(&mut self) -> &mut [u8] {
        match self.data().memory {
            Some(m) => m.data_mut(self),
            None => panic!("zvm aot: memory not bound"),
        }
    }

    fn fuel(&mut self) -> i64 {
        let g = globals(self.data());
        get_i64(g.fuel, self)
    }

    fn set_fuel(&mut self, v: i64) {
        let g = globals(self.data());
        let _ = g.fuel.set(&mut *self, Val::I64(v));
    }

    fn depth(&mut self) -> i32 {
        let g = globals(self.data());
        get_i64(g.depth, self) as i32
    }

    fn set_rq(&mut self, rq: Option<i64>) {
        let g = globals(self.data());
        let _ = g.rq_set.set(&mut *self, Val::I32(i32::from(rq.is_some())));
        let _ = g.rq_val.set(&mut *self, Val::I64(rq.unwrap_or(0)));
    }
}

fn trap_error() -> wasmtime::Error {
    wasmtime::Error::msg("zvm trap")
}

/// Process-wide engine and module cache keyed by module digest.
fn engine_and_module(bytes: &[u8]) -> Result<(Engine, Module), String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    static CACHE: OnceLock<Mutex<HashMap<(u64, u64, usize), Module>>> = OnceLock::new();
    let engine = ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            // 256 frames of the largest ZScript function, with headroom.
            config.max_wasm_stack(8 << 20);
            Engine::new(&config).unwrap_or_default()
        })
        .clone();
    let key = digest(bytes);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(m) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Ok((engine, m));
    }
    let module = Module::new(&engine, bytes).map_err(|e| e.to_string())?;
    if let Ok(mut c) = cache.lock() {
        if c.len() >= 16 {
            c.clear();
        }
        c.insert(key, module.clone());
    }
    Ok((engine, module))
}

pub(crate) struct WtBackend {
    store: Store<WtData>,
    memory: Memory,
    g: Globals,
    funcs: Vec<Option<TypedFunc<(), ()>>>,
}

impl WtBackend {
    pub(crate) fn new(bytes: &[u8], compiled: &[bool], core: HostCore) -> Result<Self, String> {
        let (engine, module) = engine_and_module(bytes)?;
        let mut store = Store::new(
            &engine,
            WtData {
                core,
                memory: None,
                g: None,
            },
        );
        let mut linker: Linker<WtData> = Linker::new(&engine);
        link_imports!(&mut linker, Caller<'_, WtData>, trap_error);
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| e.to_string())?;
        let memory = instance
            .get_memory(&mut store, abi::MEMORY)
            .ok_or("aot module has no memory")?;
        let mut global = |name: &str| {
            instance
                .get_global(&mut store, name)
                .ok_or_else(|| format!("aot module lacks {name}"))
        };
        let g = Globals {
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
                    instance
                        .get_typed_func::<(), ()>(&mut store, &abi::func_export(i as u32))
                        .map_err(|e| e.to_string())?,
                )
            } else {
                None
            });
        }
        store.data_mut().memory = Some(memory);
        store.data_mut().g = Some(g);
        Ok(Self {
            store,
            memory,
            g,
            funcs,
        })
    }
}

impl Backend for WtBackend {
    fn core(&mut self) -> &mut HostCore {
        &mut self.store.data_mut().core
    }

    fn mem(&mut self) -> &mut [u8] {
        self.memory.data_mut(&mut self.store)
    }

    fn prepare(&mut self, fuel: i64, limit: i32) {
        let _ = self.g.fuel.set(&mut self.store, Val::I64(fuel));
        let _ = self.g.depth.set(&mut self.store, Val::I32(0));
        let _ = self.g.limit.set(&mut self.store, Val::I32(limit));
        let _ = self.g.rq_set.set(&mut self.store, Val::I32(0));
    }

    fn fuel(&mut self) -> i64 {
        match self.g.fuel.get(&mut self.store) {
            Val::I64(v) => v,
            _ => 0,
        }
    }

    fn call(&mut self, idx: usize) -> Result<(), String> {
        let Some(Some(f)) = self.funcs.get(idx).cloned() else {
            return Err(format!("function {idx} not compiled"));
        };
        f.call(&mut self.store, ()).map_err(|e| e.to_string())
    }
}
