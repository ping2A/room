//! WASM script host bridge (`docs/12` / M11).
//!
//! Holds loaded play modules and feeds deterministic host imports from [`World`].

use crate::actor::ActorId;
use crate::class::{ClassError, ClassRegistry};
use crate::world::World;
use room_core::ClassId;
use room_script::{
    CompiledModule, GuestLifecycle, HostLinker, ModuleConfig, ModuleScope, RegisteredClass,
    RegisteredEventHandler, RegisteredStaticFunction, RegisteredThinkerClass, ScriptError,
    SessionKind, WasmBackend, WasmInstance, WasmiBackend, WasmiInstance, event_mask,
};
use room_script_abi::Budget;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use xxhash_rust::xxh3::xxh3_64;

/// Loaded guest module instance.
pub struct LoadedScript {
    /// Mod / module name (for logs / saves).
    pub name: String,
    /// Play vs UI scope.
    pub scope: ModuleScope,
    instance: WasmiInstance,
    compiled: CompiledModule,
}

/// Memory snapshot for one play module (`docs/12` §10).
#[derive(Clone, Debug)]
pub struct ModuleMemorySnapshot {
    /// Module name.
    pub name: String,
    /// xxh3 of wasm bytes (identity check on load).
    pub module_hash: u64,
    /// Full linear memory image.
    pub memory: Vec<u8>,
}

/// Playsim-facing script runtime (wasmi).
pub struct ScriptHost {
    backend: WasmiBackend,
    host: HostLinker,
    maptime: Arc<AtomicI32>,
    players: Arc<AtomicI32>,
    /// Live actor refs (`ActorRef` bits) for `actor_exists`.
    live_actors: Arc<Mutex<Vec<u64>>>,
    /// Intern table for `name_intern` (insertion order → NameRef).
    names: Arc<Mutex<BTreeMap<String, u32>>>,
    /// Class registry snapshot for `class_find`.
    classes: Arc<Mutex<Option<Arc<ClassRegistry>>>>,
    /// Queued `acs_execute` requests from WASM (drained each tic).
    #[allow(clippy::type_complexity)] // (script, map, args)
    acs_starts: Arc<Mutex<Vec<(i32, i32, Vec<i32>)>>>,
    modules: Vec<LoadedScript>,
}

/// Copy for world snapshots (rollback): a fresh host. Only a world with no
/// play modules loaded may be snapshotted ([`crate::World::snapshot`]).
impl Clone for ScriptHost {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptHost {
    /// No play modules are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Empty host with world-backed `level_time` / `actor_exists`.
    #[must_use]
    pub fn new() -> Self {
        let maptime = Arc::new(AtomicI32::new(0));
        let players = Arc::new(AtomicI32::new(0));
        let live_actors = Arc::new(Mutex::new(Vec::new()));
        let names = Arc::new(Mutex::new(BTreeMap::new()));
        let classes = Arc::new(Mutex::new(None));
        let acs_starts = Arc::new(Mutex::new(Vec::new()));
        let mt = Arc::clone(&maptime);
        let pc = Arc::clone(&players);
        let live = Arc::clone(&live_actors);
        let names_cb = Arc::clone(&names);
        let classes_cb = Arc::clone(&classes);
        let acs_cb = Arc::clone(&acs_starts);
        let mut host = HostLinker::default();
        let reg_find = Arc::clone(&host.reg);
        host.level_time = Arc::new(move || mt.load(Ordering::Relaxed));
        host.player_count = Arc::new(move || pc.load(Ordering::Relaxed));
        host.actor_exists = Arc::new(move |r| {
            if r == 0 {
                return 0;
            }
            live.lock().map(|g| i32::from(g.contains(&r))).unwrap_or(0)
        });
        host.name_intern = Arc::new(move |bytes| {
            if bytes.is_empty() {
                return 0;
            }
            let key = String::from_utf8_lossy(bytes).into_owned();
            let Ok(mut g) = names_cb.lock() else {
                return 0;
            };
            if let Some(&id) = g.get(&key) {
                return id;
            }
            let id = u32::try_from(g.len() + 1).unwrap_or(u32::MAX);
            g.insert(key, id);
            id
        });
        host.class_find = Arc::new(move |bytes| {
            if bytes.is_empty() {
                return 0;
            }
            if let Ok(g) = reg_find.lock() {
                if let Some(r) = g.find_class(bytes) {
                    return r;
                }
            }
            let name = String::from_utf8_lossy(bytes);
            let Ok(g) = classes_cb.lock() else {
                return 0;
            };
            g.as_ref()
                .and_then(|reg: &Arc<ClassRegistry>| reg.find_name(name.as_ref()))
                .map(|id| id.raw().saturating_add(1))
                .unwrap_or(0)
        });
        host.acs_execute = Arc::new(move |script, map, args| {
            if let Ok(mut g) = acs_cb.lock() {
                g.push((script, map, args.to_vec()));
            }
            1
        });
        Self {
            backend: WasmiBackend::new(),
            host,
            maptime,
            players,
            live_actors,
            names,
            classes,
            acs_starts,
            modules: Vec::new(),
        }
    }

    /// Copy playsim clocks / actor liveness / classes into host import state.
    pub fn sync_from_world(&self, world: &World) {
        self.maptime
            .store(world.level.maptime as i32, Ordering::Relaxed);
        let mut n = 0i32;
        if let Ok(mut g) = self.live_actors.lock() {
            g.clear();
            for slot in &world.players {
                let Some(p) = slot.as_ref() else {
                    continue;
                };
                n += 1;
                let Some(mo) = p.mo else {
                    continue;
                };
                if world.actors.get(mo).is_some_and(|a| !a.destroyed) {
                    g.push(actor_ref_bits(mo));
                }
            }
        }
        self.players.store(n, Ordering::Relaxed);
        if let Ok(mut g) = self.classes.lock() {
            *g = Some(Arc::clone(&world.classes));
        }
    }

    /// Compile, instantiate, `register` + `init(NewGame)` (play scope).
    ///
    /// # Errors
    ///
    /// Validation / link / guest trap.
    pub fn load_play_module(
        &mut self,
        name: impl Into<String>,
        bytes: &[u8],
    ) -> Result<(), ScriptError> {
        self.load_module(name, bytes, ModuleScope::Play, SessionKind::NewGame)
    }

    /// Load a UI-scope module (`docs/12` §3).
    ///
    /// # Errors
    ///
    /// Validation / link / guest trap.
    pub fn load_ui_module(
        &mut self,
        name: impl Into<String>,
        bytes: &[u8],
    ) -> Result<(), ScriptError> {
        self.load_module(name, bytes, ModuleScope::Ui, SessionKind::NewGame)
    }

    fn load_module(
        &mut self,
        name: impl Into<String>,
        bytes: &[u8],
        scope: ModuleScope,
        session: SessionKind,
    ) -> Result<(), ScriptError> {
        let cfg = ModuleConfig {
            scope,
            ..Default::default()
        };
        let compiled = self.backend.compile(bytes, &cfg)?;
        let mut instance = self.backend.instantiate(&compiled, &self.host)?;
        let mut budget = Budget::from_config(&cfg);
        let ver = instance.abi_version(&mut budget)?;
        if ver != room_script::ABI_VERSION {
            return Err(ScriptError::Invalid(format!(
                "abi version {ver} != {}",
                room_script::ABI_VERSION
            )));
        }
        let module_index = self.modules.len();
        if let Ok(mut g) = self.host.reg.lock() {
            g.set_current_module(module_index);
        }
        if instance.register(&mut budget)? != 0 {
            return Err(ScriptError::Backend("room_register failed".into()));
        }
        if instance.init(session, &mut budget)? != 0 {
            return Err(ScriptError::Backend("room_init failed".into()));
        }
        self.modules.push(LoadedScript {
            name: name.into(),
            scope,
            instance,
            compiled,
        });
        Ok(())
    }

    /// Number of loaded modules.
    #[must_use]
    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    /// Names of loaded modules (load order).
    #[must_use]
    pub fn module_names(&self) -> Vec<&str> {
        self.modules.iter().map(|m| m.name.as_str()).collect()
    }

    /// Interned name count (tests / diagnostics).
    #[must_use]
    pub fn interned_name_count(&self) -> usize {
        self.names.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// Classes registered by loaded modules (`reg_class_*`).
    #[must_use]
    pub fn registered_classes(&self) -> Vec<RegisteredClass> {
        self.host
            .reg
            .lock()
            .map(|g| g.classes().to_vec())
            .unwrap_or_default()
    }

    /// Event handlers from registration (unsorted).
    #[must_use]
    pub fn event_handlers(&self) -> Vec<RegisteredEventHandler> {
        self.host
            .reg
            .lock()
            .map(|g| g.event_handlers().to_vec())
            .unwrap_or_default()
    }

    /// Thinker classes from registration.
    #[must_use]
    pub fn thinker_classes(&self) -> Vec<RegisteredThinkerClass> {
        self.host
            .reg
            .lock()
            .map(|g| g.thinker_classes().to_vec())
            .unwrap_or_default()
    }

    /// ACS `ScriptCall` targets from registration.
    #[must_use]
    pub fn static_functions(&self) -> Vec<RegisteredStaticFunction> {
        self.host
            .reg
            .lock()
            .map(|g| g.static_functions().to_vec())
            .unwrap_or_default()
    }

    /// Drain queued WASM→ACS execute requests.
    pub fn drain_acs_starts(&self) -> Vec<(i32, i32, Vec<i32>)> {
        self.acs_starts
            .lock()
            .map(|mut g| std::mem::take(&mut *g))
            .unwrap_or_default()
    }

    /// Invoke a registered static function (ACS `ScriptCall` bridge).
    ///
    /// Bootstrap ABI: `payload_ptr`/`payload_len` carry the first two i32 args.
    ///
    /// # Errors
    ///
    /// Unknown target or guest trap.
    pub fn call_static(
        &mut self,
        class_name: &str,
        func_name: &str,
        args: &[i32],
    ) -> Result<i64, ScriptError> {
        let target = self
            .host
            .reg
            .lock()
            .ok()
            .and_then(|g| g.find_static_function(class_name, func_name).cloned())
            .ok_or_else(|| {
                ScriptError::Backend(format!("no ScriptCall target {class_name}.{func_name}"))
            })?;
        let Some(m) = self.modules.get_mut(target.module_index) else {
            return Err(ScriptError::Backend("static fn module missing".into()));
        };
        let mut budget = Budget::from_config(m.compiled.config());
        let a0 = args.first().copied().unwrap_or(0);
        let a1 = args.get(1).copied().unwrap_or(0);
        m.instance.dispatch(target.callback_id, a0, a1, &mut budget)
    }

    /// Dispatch `WorldTick` to registered handlers (order, then registration order).
    ///
    /// # Errors
    ///
    /// Guest trap / missing module.
    pub fn dispatch_world_tick(&mut self) -> Result<(), ScriptError> {
        self.dispatch_event_mask(event_mask::WORLD_TICK)
    }

    /// Dispatch `UiTick` to UI-scope handlers.
    ///
    /// # Errors
    ///
    /// Guest trap / missing module.
    pub fn dispatch_ui_tick(&mut self) -> Result<(), ScriptError> {
        self.dispatch_event_mask(event_mask::UI_TICK)
    }

    fn dispatch_event_mask(&mut self, mask: u64) -> Result<(), ScriptError> {
        let mut handlers = self.event_handlers();
        handlers.sort_by(|a, b| {
            a.order
                .cmp(&b.order)
                .then_with(|| a.module_index.cmp(&b.module_index))
                .then_with(|| a.name.cmp(&b.name))
        });
        for h in handlers {
            if h.event_mask & mask == 0 {
                continue;
            }
            let Some(m) = self.modules.get_mut(h.module_index) else {
                return Err(ScriptError::Backend(format!(
                    "event handler {} module {}",
                    h.name, h.module_index
                )));
            };
            let mut budget = Budget::from_config(m.compiled.config());
            m.instance.dispatch(h.callback_base, 0, 0, &mut budget)?;
        }
        Ok(())
    }

    /// Snapshot play-module linear memory for saves.
    pub fn snapshot_play_modules(&mut self) -> Vec<ModuleMemorySnapshot> {
        let mut out = Vec::new();
        for m in &mut self.modules {
            if m.scope != ModuleScope::Play {
                continue;
            }
            out.push(ModuleMemorySnapshot {
                name: m.name.clone(),
                module_hash: xxh3_64(m.compiled.bytes()),
                memory: m.instance.memory().to_vec(),
            });
        }
        out
    }

    /// Restore play-module memory from snapshots (`room_init(LoadGame)` + write + `on_load`).
    ///
    /// Modules must already be loaded with matching names/hashes.
    ///
    /// # Errors
    ///
    /// Missing module, hash mismatch, or guest trap.
    pub fn restore_play_modules(
        &mut self,
        snaps: &[ModuleMemorySnapshot],
    ) -> Result<(), ScriptError> {
        for snap in snaps {
            let Some(m) = self.modules.iter_mut().find(|m| m.name == snap.name) else {
                return Err(ScriptError::Backend(format!(
                    "save module {} not loaded",
                    snap.name
                )));
            };
            if m.scope != ModuleScope::Play {
                return Err(ScriptError::Backend(format!(
                    "module {} is not play scope",
                    snap.name
                )));
            }
            let hash = xxh3_64(m.compiled.bytes());
            if hash != snap.module_hash {
                return Err(ScriptError::Backend(format!(
                    "module {} hash mismatch",
                    snap.name
                )));
            }
            let mut budget = Budget::from_config(m.compiled.config());
            if m.instance.init(SessionKind::LoadGame, &mut budget)? != 0 {
                return Err(ScriptError::Backend("room_init(LoadGame) failed".into()));
            }
            m.instance.write_memory(0, &snap.memory)?;
            m.instance.on_load(&mut budget)?;
        }
        Ok(())
    }

    /// Registration fingerprint (identical across wasmi/wasmtime for the same module).
    #[must_use]
    pub fn registration_fingerprint(&self) -> u64 {
        self.host.reg.lock().map(|g| g.fingerprint()).unwrap_or(0)
    }

    /// Merge WASM-registered classes into a [`ClassRegistry`].
    ///
    /// Returns script ClassRef → [`ClassId`] mapping. Parent `0` → `Actor`.
    ///
    /// # Errors
    ///
    /// Unknown parent or [`ClassError`] from the registry.
    pub fn merge_into_registry(
        &self,
        registry: &mut ClassRegistry,
    ) -> Result<BTreeMap<u32, ClassId>, ScriptError> {
        let mut map = BTreeMap::new();
        for c in self.registered_classes() {
            let parent_id = if c.parent == 0 {
                registry.find_name("Actor").ok_or_else(|| {
                    ScriptError::Backend("Actor missing from ClassRegistry".into())
                })?
            } else if let Some(&id) = map.get(&c.parent) {
                id
            } else {
                // Parent already in registry as ClassRef (= ClassId.raw()+1).
                let raw = c.parent.saturating_sub(1);
                let id = ClassId::from_raw(raw);
                if registry.get(id).is_none() {
                    return Err(ScriptError::Backend(format!(
                        "unknown parent ClassRef {}",
                        c.parent
                    )));
                }
                id
            };
            let id = registry
                .import_script_class(&c, parent_id)
                .map_err(|e: ClassError| ScriptError::Backend(e.to_string()))?;
            map.insert(c.class_ref, id);
        }
        Ok(map)
    }

    /// Dispatch on the first loaded module (smoke / tests).
    ///
    /// # Errors
    ///
    /// No module or guest trap.
    pub fn dispatch_first(
        &mut self,
        callback_id: i32,
        payload_ptr: i32,
        payload_len: i32,
    ) -> Result<i64, ScriptError> {
        self.dispatch_module(0, callback_id, payload_ptr, payload_len)
    }

    /// Dispatch on a module by index.
    ///
    /// # Errors
    ///
    /// Missing module or guest trap.
    pub fn dispatch_module(
        &mut self,
        index: usize,
        callback_id: i32,
        payload_ptr: i32,
        payload_len: i32,
    ) -> Result<i64, ScriptError> {
        let Some(m) = self.modules.get_mut(index) else {
            return Err(ScriptError::Backend("no script module".into()));
        };
        let mut budget = Budget::from_config(m.compiled.config());
        m.instance
            .dispatch(callback_id, payload_ptr, payload_len, &mut budget)
    }
}

fn actor_ref_bits(id: ActorId) -> u64 {
    let index = u64::from(id.index());
    let generation = u64::from(id.generation());
    index | (generation << 32)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::state::StateTable;
    use room_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    fn stub_wasm() -> Vec<u8> {
        wat::parse_str(
            r#"(module
  (import "room" "log" (func $log (param i32 i32 i32)))
  (import "room" "level_time" (func $level_time (result i32)))
  (import "room" "actor_exists" (func $actor_exists (param i64) (result i32)))
  (import "room" "player_count" (func $player_count (result i32)))
  (memory (export "memory") 1)
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 16))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32) (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (if (result i64) (i32.eq (local.get $id) (i32.const 1))
      (then (i64.extend_i32_s (call $level_time)))
      (else (i64.extend_i32_s (call $player_count)))))
)"#,
        )
        .unwrap()
    }

    fn hello_mod_wasm() -> Vec<u8> {
        wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../room-script/testdata/hello_mod.wat"
        ))
        .expect("hello_mod.wat")
    }

    #[test]
    fn script_host_level_time_tracks_world() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let classes = Arc::new(ClassRegistry::new());
        let states = Arc::new(StateTable::default());
        let mut world = World::from_map_with_skill(map, classes, states, 3).unwrap();
        world
            .scripts
            .load_play_module("stub", &stub_wasm())
            .unwrap();
        world.level.maptime = 42;
        world.scripts.sync_from_world(&world);
        assert_eq!(world.scripts.dispatch_first(1, 0, 0).unwrap(), 42);
        assert_eq!(world.scripts.dispatch_first(3, 0, 0).unwrap(), 1);
        world.level.maptime = 99;
        world.scripts.sync_from_world(&world);
        assert_eq!(world.scripts.dispatch_first(1, 0, 0).unwrap(), 99);
    }

    #[test]
    fn ui_tick_and_memory_save_restore() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let classes = Arc::new(ClassRegistry::new());
        let states = Arc::new(StateTable::default());
        let mut world = World::from_map_with_skill(map, classes, states, 3).unwrap();
        let ui = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../room-script/testdata/ui_tick.wat"
        ))
        .unwrap();
        let play = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../room-script/testdata/save_counter.wat"
        ))
        .unwrap();
        world.scripts.load_play_module("counter", &play).unwrap();
        world.scripts.load_ui_module("hud", &ui).unwrap();
        world.scripts.dispatch_ui_tick().unwrap();
        world.scripts.dispatch_ui_tick().unwrap();
        assert_eq!(world.scripts.dispatch_module(1, 50, 0, 0).unwrap(), 3);
        assert_eq!(world.scripts.dispatch_first(1, 0, 0).unwrap(), 1);
        assert_eq!(world.scripts.dispatch_first(1, 0, 0).unwrap(), 2);
        let snaps = world.scripts.snapshot_play_modules();
        assert_eq!(snaps.len(), 1);
        assert_eq!(world.scripts.dispatch_first(1, 0, 0).unwrap(), 3);
        world.scripts.restore_play_modules(&snaps).unwrap();
        assert_eq!(world.scripts.dispatch_first(2, 0, 0).unwrap(), 2);
    }

    #[test]
    fn script_call_and_acs_execute_queue() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let classes = Arc::new(ClassRegistry::new());
        let states = Arc::new(StateTable::default());
        let mut world = World::from_map_with_skill(map, classes, states, 3).unwrap();
        let bytes = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../room-script/testdata/script_call.wat"
        ))
        .unwrap();
        world.scripts.load_play_module("bridge", &bytes).unwrap();
        assert_eq!(world.scripts.static_functions().len(), 1);
        assert_eq!(
            world.scripts.call_static("Util", "Add", &[3, 4]).unwrap(),
            7
        );
        assert_eq!(world.scripts.dispatch_first(201, 0, 0).unwrap(), 1);
        let q = world.scripts.drain_acs_starts();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].0, 1);
        assert_eq!(q[0].2, vec![7, 8, 9]);
    }

    #[test]
    fn event_tick_runs_on_world_tick() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let classes = Arc::new(ClassRegistry::new());
        let states = Arc::new(StateTable::default());
        let mut world = World::from_map_with_skill(map, classes, states, 3).unwrap();
        let bytes = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../room-script/testdata/event_tick.wat"
        ))
        .unwrap();
        world.scripts.load_play_module("events", &bytes).unwrap();
        assert_eq!(world.scripts.event_handlers().len(), 1);
        assert_eq!(world.scripts.thinker_classes().len(), 1);
        world.tick();
        world.tick();
        assert_eq!(world.scripts.dispatch_first(100, 0, 0).unwrap(), 3);
    }

    #[test]
    fn fireimp_reg_registers_class() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let classes = Arc::new(ClassRegistry::new());
        let states = Arc::new(StateTable::default());
        let mut world = World::from_map_with_skill(map, classes, states, 3).unwrap();
        let bytes = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../room-script/testdata/fireimp_reg.wat"
        ))
        .unwrap();
        world.scripts.load_play_module("fireimp", &bytes).unwrap();
        let regs = world.scripts.registered_classes();
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].name, "FireImp");
        assert_eq!(regs[0].props_i32, vec![(1, 90)]);
        assert_eq!(world.scripts.dispatch_first(1, 0, 0).unwrap(), 1_000);
        let fp = world.scripts.registration_fingerprint();
        assert_ne!(fp, 0);
        world.apply_script_classes().unwrap();
        let id = world.classes.find_name("FireImp").expect("merged");
        assert_eq!(world.classes.get(id).unwrap().health, 90);
        assert_ne!(fp, 0);
    }

    #[test]
    fn hello_mod_name_intern_and_class_find() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let classes = Arc::new(ClassRegistry::new());
        let actor_ref = classes
            .find_name("Actor")
            .expect("builtin Actor")
            .raw()
            .saturating_add(1);
        let states = Arc::new(StateTable::default());
        let mut world = World::from_map_with_skill(map, classes, states, 3).unwrap();
        world.scripts.sync_from_world(&world);
        world
            .scripts
            .load_play_module("hello", &hello_mod_wasm())
            .unwrap();
        let fire = world.scripts.dispatch_first(4, 0, 0).unwrap();
        assert!(fire > 0);
        assert_eq!(world.scripts.interned_name_count(), 1);
        let again = world.scripts.dispatch_first(4, 0, 0).unwrap();
        assert_eq!(fire, again);
        assert_eq!(world.scripts.interned_name_count(), 1);
        assert_eq!(
            world.scripts.dispatch_first(5, 0, 0).unwrap(),
            i64::from(actor_ref)
        );
    }
}
