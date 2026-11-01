//! `wasmtime` Cranelift backend (native).

use crate::{
    CompiledModule, GameFn, GuestMemory, HostLinker, LogFn, ScriptError, SharedGameBridge,
    SharedRegistration, ValKind, WasmBackend, WasmInstance, validate_module,
};
use room_script_abi::{Budget, ModuleConfig, Trap, Val};
use std::sync::Arc;
use wasmtime::{
    Caller, Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, Val as WtVal,
};

struct HostState {
    limits: StoreLimits,
    log: LogFn,
    level_time: crate::LevelTimeFn,
    actor_exists: crate::ActorExistsFn,
    player_count: crate::PlayerCountFn,
    name_intern: crate::NameInternFn,
    class_find: crate::ClassFindFn,
    acs_execute: crate::AcsExecuteFn,
    lump_find: crate::LumpFindFn,
    lump_bytes: crate::LumpBytesFn,
    reg: SharedRegistration,
    game: Option<SharedGameBridge>,
}

/// Fuel-metered wasmtime engine.
pub struct WasmtimeBackend {
    engine: Engine,
}

impl WasmtimeBackend {
    /// Create a deterministic wasmtime engine (fuel + NaN canonicalization).
    ///
    /// # Errors
    ///
    /// Engine configuration failure.
    pub fn new() -> Result<Self, ScriptError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.cranelift_nan_canonicalization(true);
        config.wasm_relaxed_simd(false);
        let engine = Engine::new(&config).map_err(|e| ScriptError::Backend(e.to_string()))?;
        Ok(Self { engine })
    }
}

impl WasmBackend for WasmtimeBackend {
    type Instance = WasmtimeInstance;

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
            lump_find: Arc::clone(&host.lump_find),
            lump_bytes: Arc::clone(&host.lump_bytes),
            reg: Arc::clone(&host.reg),
            game: host.game.clone(),
        };
        let mut store = Store::new(&self.engine, state);
        store
            .set_fuel(module.cfg.fuel_per_call)
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        store.limiter(|s| &mut s.limits);

        let mut linker = Linker::new(&self.engine);
        linker
            .func_wrap(
                "room",
                "log",
                |mut caller: Caller<'_, HostState>, level: i32, ptr: i32, len: i32| {
                    let msg = read_guest_str(&mut caller, ptr, len);
                    (caller.data().log)(level, &msg);
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap("room", "level_time", |caller: Caller<'_, HostState>| {
                (caller.data().level_time)()
            })
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "room",
                "actor_exists",
                |caller: Caller<'_, HostState>, actor: i64| {
                    (caller.data().actor_exists)(actor as u64)
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap("room", "player_count", |caller: Caller<'_, HostState>| {
                (caller.data().player_count)()
            })
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "room",
                "name_intern",
                |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                    let bytes = read_guest_bytes(&mut caller, ptr, len);
                    (caller.data().name_intern)(&bytes)
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "room",
                "class_find",
                |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                    let bytes = read_guest_bytes(&mut caller, ptr, len);
                    (caller.data().class_find)(&bytes)
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "room",
                "acs_execute",
                |mut caller: Caller<'_, HostState>,
                 script: i32,
                 map: i32,
                 args_ptr: i32,
                 n: i32| {
                    let args = read_guest_i32s(&mut caller, args_ptr, n);
                    (caller.data().acs_execute)(script, map, &args)
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "room",
                "lump_find",
                |mut caller: Caller<'_, HostState>, ptr: i32, len: i32, namespace: i32| {
                    let name = read_guest_bytes(&mut caller, ptr, len);
                    (caller.data().lump_find)(&name, namespace)
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        linker
            .func_wrap(
                "room",
                "lump_read",
                |mut caller: Caller<'_, HostState>, index: i32, buf_ptr: i32, cap: i32| {
                    let Some(bytes) = (caller.data().lump_bytes)(index) else {
                        return -1;
                    };
                    if cap < 0 || buf_ptr < 0 {
                        return -1;
                    }
                    let n = (cap as usize).min(bytes.len());
                    let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                        return -1;
                    };
                    let data = mem.data_mut(&mut caller);
                    let start = buf_ptr as usize;
                    let end = start.saturating_add(n);
                    if end > data.len() {
                        return -1;
                    }
                    data[start..end].copy_from_slice(&bytes[..n]);
                    n as i32
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        link_reg_imports(&mut linker)?;
        link_game_imports(&self.engine, &mut linker)?;

        let instance = linker
            .instantiate(&mut store, &wasm)
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
        Ok(WasmtimeInstance { store, instance })
    }
}

/// Instantiated wasmtime module.
pub struct WasmtimeInstance {
    store: Store<HostState>,
    instance: wasmtime::Instance,
}

impl WasmInstance for WasmtimeInstance {
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
        let wasm_args: Vec<WtVal> = args.iter().map(val_to_wt).collect();
        let mut wasm_results = vec![WtVal::I32(0); results.len()];
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
            *dst = wt_to_val(src);
        }
        Ok(())
    }

    fn memory(&mut self) -> &[u8] {
        let Some(mem) = self.instance.get_memory(&mut self.store, "memory") else {
            return &[];
        };
        mem.data(&self.store)
    }

    fn write_memory(&mut self, offset: usize, data: &[u8]) -> Result<(), ScriptError> {
        let mem = self
            .instance
            .get_memory(&mut self.store, "memory")
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
        self.instance.get_export(&mut self.store, name).is_some()
    }
}

fn link_reg_imports(linker: &mut Linker<HostState>) -> Result<(), ScriptError> {
    linker
        .func_wrap(
            "room",
            "reg_class_begin",
            |mut caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             parent: i32,
             replaces: i32,
             doomednum: i32,
             flags: i32| {
                let name = read_guest_bytes(&mut caller, name_ptr, name_len);
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
            "room",
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
            "room",
            "reg_class_property_str",
            |mut caller: Caller<'_, HostState>, prop: i32, ptr: i32, len: i32| {
                let value = read_guest_bytes(&mut caller, ptr, len);
                if let Ok(mut g) = caller.data().reg.lock() {
                    g.class_property_str(prop as u32, &value);
                }
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
            "reg_class_flag",
            |caller: Caller<'_, HostState>, flag: i32, on: i32| {
                if let Ok(mut g) = caller.data().reg.lock() {
                    g.class_flag(flag as u32, on);
                }
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap("room", "reg_class_end", |caller: Caller<'_, HostState>| {
            if let Ok(mut g) = caller.data().reg.lock() {
                g.class_end();
            }
        })
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
            "reg_state_label",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                let label = read_guest_bytes(&mut caller, ptr, len);
                if let Ok(mut g) = caller.data().reg.lock() {
                    g.state_label(&label);
                }
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
            "reg_state",
            |mut caller: Caller<'_, HostState>,
             sprite_ptr: i32,
             frame: i32,
             tics: i32,
             flags: i32,
             callback_id: i32| {
                let sprite = read_guest_bytes(&mut caller, sprite_ptr, 4);
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
            "room",
            "reg_state_flow",
            |mut caller: Caller<'_, HostState>,
             kind: i32,
             label_ptr: i32,
             label_len: i32,
             offset: i32| {
                let label = read_guest_bytes(&mut caller, label_ptr, label_len);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.state_flow(kind, &label, offset))
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
            "reg_state_action",
            |mut caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             args_ptr: i32,
             args_len: i32| {
                let name = read_guest_bytes(&mut caller, name_ptr, name_len);
                let args = read_guest_bytes(&mut caller, args_ptr, args_len);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.state_action(&name, &args))
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
            "reg_state_offset",
            |caller: Caller<'_, HostState>, x: i32, y: i32| {
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.state_offset(x, y))
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
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
            "room",
            "reg_event_handler",
            |mut caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             order: i32,
             flags: i32,
             mask_lo: i32,
             mask_hi: i32,
             callback_base: i32| {
                let name = read_guest_bytes(&mut caller, name_ptr, name_len);
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
            "room",
            "reg_thinker_class",
            |mut caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             statnum: i32,
             callback_id: i32| {
                let name = read_guest_bytes(&mut caller, name_ptr, name_len);
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
            "room",
            "reg_static_function",
            |mut caller: Caller<'_, HostState>,
             class_ptr: i32,
             class_len: i32,
             func_ptr: i32,
             func_len: i32,
             callback_id: i32| {
                let class_name = read_guest_bytes(&mut caller, class_ptr, class_len);
                let func_name = read_guest_bytes(&mut caller, func_ptr, func_len);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.static_function(&class_name, &func_name, callback_id))
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
            "reg_lump",
            |mut caller: Caller<'_, HostState>,
             name_ptr: i32,
             name_len: i32,
             namespace: i32,
             data_ptr: i32,
             data_len: i32| {
                let name = read_guest_bytes(&mut caller, name_ptr, name_len);
                let data = read_guest_bytes(&mut caller, data_ptr, data_len);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.lump(&name, namespace, &data))
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
            "reg_sound_alias",
            |mut caller: Caller<'_, HostState>,
             logical_ptr: i32,
             logical_len: i32,
             lump_ptr: i32,
             lump_len: i32,
             limit: i32| {
                let logical = read_guest_bytes(&mut caller, logical_ptr, logical_len);
                let lump = read_guest_bytes(&mut caller, lump_ptr, lump_len);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.sound_alias(&logical, &lump, limit))
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
            "reg_weapon_slot",
            |mut caller: Caller<'_, HostState>,
             slot: i32,
             class_ptr: i32,
             class_len: i32,
             priority: i32| {
                let class_name = read_guest_bytes(&mut caller, class_ptr, class_len);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|mut g| g.weapon_slot(slot, &class_name, priority))
                    .unwrap_or(-1)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
            "manifest_option_i32",
            |mut caller: Caller<'_, HostState>, key_ptr: i32, key_len: i32, default: i32| {
                let key = read_guest_bytes(&mut caller, key_ptr, key_len);
                caller
                    .data()
                    .reg
                    .lock()
                    .map(|g| g.option_i32(&key, default))
                    .unwrap_or(default)
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    linker
        .func_wrap(
            "room",
            "manifest_option_str",
            |mut caller: Caller<'_, HostState>,
             key_ptr: i32,
             key_len: i32,
             buf_ptr: i32,
             cap: i32| {
                let key = read_guest_bytes(&mut caller, key_ptr, key_len);
                if cap <= 0 || buf_ptr < 0 {
                    return -1;
                }
                let Ok(cap_u) = usize::try_from(cap) else {
                    return -1;
                };
                let mut tmp = vec![0u8; cap_u];
                let n = caller
                    .data()
                    .reg
                    .lock()
                    .map(|g| g.option_str(&key, &mut tmp))
                    .unwrap_or(-1);
                if n < 0 {
                    return n;
                }
                let write_n = (n as usize).min(cap_u);
                let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                    return -1;
                };
                let data = mem.data_mut(&mut caller);
                let start = buf_ptr as usize;
                let end = start.saturating_add(write_n);
                if end > data.len() {
                    return -1;
                }
                data[start..end].copy_from_slice(&tmp[..write_n]);
                n
            },
        )
        .map_err(|e| ScriptError::Backend(e.to_string()))?;
    Ok(())
}

fn wt_kind(k: ValKind) -> wasmtime::ValType {
    match k {
        ValKind::I32 => wasmtime::ValType::I32,
        ValKind::I64 => wasmtime::ValType::I64,
        ValKind::F64 => wasmtime::ValType::F64,
    }
}

struct CallerMem<'a, 'b> {
    caller: &'a mut Caller<'b, HostState>,
    mem: Option<wasmtime::Memory>,
}

impl GuestMemory for CallerMem<'_, '_> {
    fn read(&self, ptr: i32, len: i32) -> Option<Vec<u8>> {
        let mem = self.mem?;
        let data = mem.data(&*self.caller);
        let start = usize::try_from(ptr).ok()?;
        let end = start.checked_add(usize::try_from(len).ok()?)?;
        data.get(start..end).map(<[u8]>::to_vec)
    }

    fn write(&mut self, ptr: i32, bytes: &[u8]) -> bool {
        let Some(mem) = self.mem else {
            return false;
        };
        let data = mem.data_mut(&mut *self.caller);
        let Ok(start) = usize::try_from(ptr) else {
            return false;
        };
        let Some(dst) = start
            .checked_add(bytes.len())
            .and_then(|end| data.get_mut(start..end))
        else {
            return false;
        };
        dst.copy_from_slice(bytes);
        true
    }
}

fn link_game_imports(engine: &Engine, linker: &mut Linker<HostState>) -> Result<(), ScriptError> {
    for &f in GameFn::ALL {
        let ty = wasmtime::FuncType::new(
            engine,
            f.params().iter().copied().map(wt_kind),
            f.result().map(wt_kind),
        );
        linker
            .func_new(
                "room",
                f.name(),
                ty,
                move |mut caller: Caller<'_, HostState>, params, results| {
                    let args: Vec<Val> = params.iter().map(wt_to_val).collect();
                    let bridge = caller.data().game.clone();
                    let mem = caller.get_export("memory").and_then(|e| e.into_memory());
                    let out = bridge.and_then(|b| {
                        let mut view = CallerMem {
                            caller: &mut caller,
                            mem,
                        };
                        b.call(f, &args, &mut view)
                    });
                    if let (Some(slot), Some(kind)) = (results.first_mut(), f.result()) {
                        *slot = val_to_wt(&crate::game::coerce(out, kind));
                    }
                    Ok(())
                },
            )
            .map_err(|e| ScriptError::Backend(e.to_string()))?;
    }
    Ok(())
}

fn read_guest_bytes(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Vec<u8> {
    if ptr < 0 || len <= 0 {
        return Vec::new();
    }
    let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
        return Vec::new();
    };
    let data = mem.data(&*caller);
    let start = ptr as usize;
    let end = start.saturating_add(len as usize);
    if end > data.len() {
        return Vec::new();
    }
    data[start..end].to_vec()
}

fn read_guest_i32s(caller: &mut Caller<'_, HostState>, ptr: i32, n: i32) -> Vec<i32> {
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

fn read_guest_str(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> String {
    String::from_utf8_lossy(&read_guest_bytes(caller, ptr, len)).into_owned()
}

fn val_to_wt(v: &Val) -> WtVal {
    match *v {
        Val::I32(x) => WtVal::I32(x),
        Val::I64(x) => WtVal::I64(x),
        Val::F32(bits) => WtVal::F32(bits),
        Val::F64(bits) => WtVal::F64(bits),
    }
}

fn wt_to_val(v: &WtVal) -> Val {
    match *v {
        WtVal::I32(x) => Val::I32(x),
        WtVal::I64(x) => Val::I64(x),
        WtVal::F32(x) => Val::F32(x),
        WtVal::F64(x) => Val::F64(x),
        _ => Val::I32(0),
    }
}
