//! `wasmi` interpreter backend (reference / wasm32-capable).

use crate::{
    CompiledModule, HostLinker, LogFn, ScriptError, SharedRegistration, WasmBackend, WasmInstance,
    validate_module,
};
use ferrite_script_abi::{Budget, ModuleConfig, Trap, Val};
use std::sync::Arc;
use wasmi::{
    Caller, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, Val as WasmiVal,
};

/// Host state living in the wasmi store.
struct HostState {
    limits: StoreLimits,
    log: LogFn,
    level_time: crate::LevelTimeFn,
    actor_exists: crate::ActorExistsFn,
    player_count: crate::PlayerCountFn,
    name_intern: crate::NameInternFn,
    class_find: crate::ClassFindFn,
    acs_execute: crate::AcsExecuteFn,
    reg: SharedRegistration,
}

/// Fuel-metered wasmi engine.
pub struct WasmiBackend {
    engine: Engine,
}

impl WasmiBackend {
    /// Create a deterministic wasmi engine with fuel enabled.
    #[must_use]
    pub fn new() -> Self {
        let mut config = wasmi::Config::default();
        config.consume_fuel(true);
        Self {
            engine: Engine::new(&config),
        }
    }
}

impl Default for WasmiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmBackend for WasmiBackend {
    type Instance = WasmiInstance;

    fn compile(&self, bytes: &[u8], cfg: &ModuleConfig) -> Result<CompiledModule, ScriptError> {
        validate_module(bytes, cfg)?;
        Module::new(&self.engine, bytes).map_err(|e| ScriptError::Invalid(e.to_string()))?;
        Ok(CompiledModule {
            bytes: bytes.to_vec(),
            cfg: cfg.clone(),
        })
    }

    fn instantiate(
        &self,
        module: &CompiledModule,
        host: &HostLinker,
    ) -> Result<Self::Instance, ScriptError> {
        let wasm = Module::new(&self.engine, &module.bytes)
            .map_err(|e| ScriptError::Invalid(e.to_string()))?;
        let limits = StoreLimitsBuilder::new()
            .memory_size(module.cfg.max_memory_pages as usize * 64 * 1024)
            .build();
        let state = HostState {
            limits,
            log: Arc::clone(&host.log),
            level_time: Arc::clone(&host.level_time),
            actor_exists: Arc::clone(&host.actor_exists),
            player_count: Arc::clone(&host.player_count),
            name_intern: Arc::clone(&host.name_intern),
            class_find: Arc::clone(&host.class_find),
            acs_execute: Arc::clone(&host.acs_execute),
            reg: Arc::clone(&host.reg),
        };
        let mut store = Store::new(&self.engine, state);
        store
            .set_fuel(module.cfg.fuel_per_call)
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        store.limiter(|s| &mut s.limits);

        let mut linker = Linker::new(&self.engine);
        linker
            .func_wrap(
                "ferrite",
                "log",
                |caller: Caller<'_, HostState>, level: i32, ptr: i32, len: i32| {
                    let msg = read_guest_str(&caller, ptr, len);
                    (caller.data().log)(level, &msg);
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap("ferrite", "level_time", |caller: Caller<'_, HostState>| {
                (caller.data().level_time)()
            })
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "ferrite",
                "actor_exists",
                |caller: Caller<'_, HostState>, actor: i64| {
                    (caller.data().actor_exists)(actor as u64)
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "ferrite",
                "player_count",
                |caller: Caller<'_, HostState>| (caller.data().player_count)(),
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "ferrite",
                "name_intern",
                |caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                    let bytes = read_guest_bytes(&caller, ptr, len);
                    (caller.data().name_intern)(&bytes)
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "ferrite",
                "class_find",
                |caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                    let bytes = read_guest_bytes(&caller, ptr, len);
                    (caller.data().class_find)(&bytes)
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "ferrite",
                "acs_execute",
                |caller: Caller<'_, HostState>, script: i32, map: i32, args_ptr: i32, n: i32| {
                    let args = read_guest_i32s(&caller, args_ptr, n);
                    (caller.data().acs_execute)(script, map, &args)
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        link_reg_imports(&mut linker)?;

        let instance = linker
            .instantiate(&mut store, &wasm)
            .map_err(|e| ScriptError::Backend(e.to_string()))?
            .start(&mut store)
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        Ok(WasmiInstance { store, instance })
    }
}

/// Instantiated wasmi module.
pub struct WasmiInstance {
    store: Store<HostState>,
    instance: wasmi::Instance,
}

impl WasmInstance for WasmiInstance {
    fn call(
        &mut self,
        name: &str,
        args: &[Val],
        results: &mut [Val],
        budget: &mut Budget,
    ) -> Result<(), ScriptError> {
        if budget.depth == 0 {
            return Err(ScriptError::Trap(Trap::CallDepth));
        }
        budget.depth -= 1;
        self.store
            .set_fuel(budget.fuel)
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or(ScriptError::Trap(Trap::MissingExport("export")))?;
        let wasm_args: Vec<WasmiVal> = args.iter().map(val_to_wasmi).collect();
        let mut wasm_results = vec![WasmiVal::I32(0); results.len()];
        let call_res = func.call(&mut self.store, &wasm_args, &mut wasm_results);
        budget.fuel = self.store.get_fuel().unwrap_or(0);
        budget.depth += 1;
        call_res.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("fuel") {
                ScriptError::Trap(Trap::OutOfFuel)
            } else {
                ScriptError::Backend(msg)
            }
        })?;
        for (dst, src) in results.iter_mut().zip(wasm_results.iter()) {
            *dst = wasmi_to_val(src);
        }
        Ok(())
    }

    fn memory(&mut self) -> &[u8] {
        let Some(mem) = self.instance.get_memory(&self.store, "memory") else {
            return &[];
        };
        mem.data(&self.store)
    }

    fn write_memory(&mut self, offset: usize, data: &[u8]) -> Result<(), ScriptError> {
        let mem = self
            .instance
            .get_memory(&self.store, "memory")
            .ok_or_else(|| ScriptError::Backend("missing memory".into()))?;
        let dst = mem.data_mut(&mut self.store);
        let end = offset.saturating_add(data.len());
        if end > dst.len() {
            return Err(ScriptError::Backend("memory write OOB".into()));
        }
        dst[offset..end].copy_from_slice(data);
        Ok(())
    }

    fn has_export(&mut self, name: &str) -> bool {
        self.instance.get_export(&self.store, name).is_some()
    }
}

fn link_reg_imports(linker: &mut Linker<HostState>) -> Result<(), ScriptError> {
    linker
        .func_wrap(
            "ferrite",
            "reg_class_begin",
            |caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             parent: i32,
             replaces: i32,
             doomednum: i32,
             flags: i32| {
                let name = read_guest_bytes(&caller, name_ptr, name_len);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| {
                        g.class_begin(
                            &name,
                            parent as u32,
                            replaces as u32,
                            doomednum,
                            flags as u32,
                        )
                    })
                    .unwrap_or(0) as i32
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "ferrite",
            "reg_class_property_i32",
            |caller: Caller<'_, HostState>, prop: i32, value: i32| {
                if let Ok(mut g) = caller.data().reg.lock() {
                    g.class_property_i32(prop as u32, value);
                }
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "ferrite",
            "reg_class_flag",
            |caller: Caller<'_, HostState>, flag: i32, on: i32| {
                if let Ok(mut g) = caller.data().reg.lock() {
                    g.class_flag(flag as u32, on);
                }
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "ferrite",
            "reg_class_end",
            |caller: Caller<'_, HostState>| {
                if let Ok(mut g) = caller.data().reg.lock() {
                    g.class_end();
                }
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "ferrite",
            "reg_state_label",
            |caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                let label = read_guest_bytes(&caller, ptr, len);
                if let Ok(mut g) = caller.data().reg.lock() {
                    g.state_label(&label);
                }
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "ferrite",
            "reg_state",
            |caller: Caller<'_, HostState>,
             sprite_ptr: i32,
             frame: i32,
             tics: i32,
             flags: i32,
             callback_id: i32| {
                let sprite = read_guest_bytes(&caller, sprite_ptr, 4);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.state(&sprite, frame, tics, flags as u32, callback_id))
                    .unwrap_or(0) as i32
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "ferrite",
            "reg_class_virtual",
            |caller: Caller<'_, HostState>, slot: i32, callback_id: i32| {
                if let Ok(mut g) = caller.data().reg.lock() {
                    g.class_virtual(slot as u32, callback_id);
                }
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "ferrite",
            "reg_event_handler",
            |caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             order: i32,
             flags: i32,
             mask_lo: i32,
             mask_hi: i32,
             callback_base: i32| {
                let name = read_guest_bytes(&caller, name_ptr, name_len);
                let mask = (mask_lo as u32 as u64) | ((mask_hi as u32 as u64) << 32);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.event_handler(&name, order, flags as u32, mask, callback_base))
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "ferrite",
            "reg_thinker_class",
            |caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             statnum: i32,
             callback_id: i32| {
                let name = read_guest_bytes(&caller, name_ptr, name_len);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.thinker_class(&name, statnum, callback_id))
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "ferrite",
            "reg_static_function",
            |caller: Caller<'_, HostState>,
             class_ptr: i32,
             class_len: i32,
             func_ptr: i32,
             func_len: i32,
             callback_id: i32| {
                let class_name = read_guest_bytes(&caller, class_ptr, class_len);
                let func_name = read_guest_bytes(&caller, func_ptr, func_len);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.static_function(&class_name, &func_name, callback_id))
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    Ok(())
}

fn read_guest_bytes(caller: &Caller<'_, HostState>, ptr: i32, len: i32) -> Vec<u8> {
    if ptr < 0 || len <= 0 {
        return Vec::new();
    }
    let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
        return Vec::new();
    };
    let data = mem.data(caller);
    let start = ptr as usize;
    let end = start.saturating_add(len as usize);
    if end > data.len() {
        return Vec::new();
    }
    data[start..end].to_vec()
}

fn read_guest_i32s(caller: &Caller<'_, HostState>, ptr: i32, n: i32) -> Vec<i32> {
    if ptr < 0 || n <= 0 {
        return Vec::new();
    }
    let n = n.min(16) as usize;
    let bytes = read_guest_bytes(caller, ptr, (n * 4) as i32);
    bytes
        .chunks_exact(4)
        .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn read_guest_str(caller: &Caller<'_, HostState>, ptr: i32, len: i32) -> String {
    String::from_utf8_lossy(&read_guest_bytes(caller, ptr, len)).into_owned()
}

fn val_to_wasmi(v: &Val) -> WasmiVal {
    match *v {
        Val::I32(x) => WasmiVal::I32(x),
        Val::I64(x) => WasmiVal::I64(x),
        Val::F32(bits) => WasmiVal::F32(wasmi::core::F32::from_bits(bits)),
        Val::F64(bits) => WasmiVal::F64(wasmi::core::F64::from_bits(bits)),
    }
}

fn wasmi_to_val(v: &WasmiVal) -> Val {
    match *v {
        WasmiVal::I32(x) => Val::I32(x),
        WasmiVal::I64(x) => Val::I64(x),
        WasmiVal::F32(x) => Val::F32(x.to_bits()),
        WasmiVal::F64(x) => Val::F64(x.to_bits()),
        WasmiVal::FuncRef(_) | WasmiVal::ExternRef(_) => Val::I32(0),
    }
}
