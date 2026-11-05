//! WASM host runtime (wasmtime / wasmi / browser backends).
//!
//! M11: validation, required ABI exports, `wasmi` backend with `room::log`
//! (`docs/12-wasm-scripting.md`).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use room_script_abi::{Budget, REQUIRED_EXPORTS, Trap, Val};
use std::sync::Arc;

pub mod game;
pub mod generated_wad;
pub mod manifest;
mod registration;

pub use game::{GameBridge, GameFn, GuestMemory, SharedGameBridge, ValKind};
pub use generated_wad::pack_generated_pwad;
pub use registration::{
    GeneratedLump, GeneratedSoundAlias, GeneratedWeaponSlot, PropId, RegCall, RegisteredClass,
    RegisteredEventHandler, RegisteredStaticFunction, RegisteredThinkerClass, RegistrationSession,
    SharedRegistration, VirtualSlot, event_handler_flags, event_mask, shared_registration,
};
pub use room_script_abi::{
    ABI_VERSION, ActorRef, ExportId, ModuleConfig, ModuleScope, SessionKind,
};

/// Errors from compile / instantiate / call.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// Module failed validation.
    #[error("invalid module: {0}")]
    Invalid(String),
    /// Backend trap.
    #[error("trap: {0:?}")]
    Trap(Trap),
    /// Backend-specific message.
    #[error("{0}")]
    Backend(String),
}

/// Compiled module bytes ready for instantiation.
#[derive(Clone)]
pub struct CompiledModule {
    bytes: Vec<u8>,
    cfg: ModuleConfig,
}

impl CompiledModule {
    /// Validated wasm bytes + config.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Config used at compile time.
    #[must_use]
    pub fn config(&self) -> &ModuleConfig {
        &self.cfg
    }
}

/// Host `room::log` callback.
pub type LogFn = Arc<dyn Fn(i32, &str) + Send + Sync>;
/// Host `room::level_time` → maptime tics.
pub type LevelTimeFn = Arc<dyn Fn() -> i32 + Send + Sync>;
/// Host `room::actor_exists(ActorRef bits)` → 0/1.
pub type ActorExistsFn = Arc<dyn Fn(u64) -> i32 + Send + Sync>;
/// Host `room::player_count` → connected player slots.
pub type PlayerCountFn = Arc<dyn Fn() -> i32 + Send + Sync>;
/// Host `room::name_intern(utf8) -> NameRef bits`.
pub type NameInternFn = Arc<dyn Fn(&[u8]) -> u32 + Send + Sync>;
/// Host `room::class_find(utf8) -> ClassRef bits`.
pub type ClassFindFn = Arc<dyn Fn(&[u8]) -> u32 + Send + Sync>;
/// Host `room::acs_execute(script, map, args) -> i32` (queued or immediate).
pub type AcsExecuteFn = Arc<dyn Fn(i32, i32, &[i32]) -> i32 + Send + Sync>;
/// Host `room::lump_find(name, namespace) -> index` (−1 absent). Indices are opaque ≥ 0.
pub type LumpFindFn = Arc<dyn Fn(&[u8], i32) -> i32 + Send + Sync>;
/// Host `room::lump_read` source: bytes for a `lump_find` index.
pub type LumpBytesFn = Arc<dyn Fn(i32) -> Option<Vec<u8>> + Send + Sync>;

/// Host callbacks bound into guest imports (module `"room"`).
#[derive(Clone)]
pub struct HostLinker {
    /// `log(level, utf8_message)`.
    pub log: LogFn,
    /// `level_time() -> i32`.
    pub level_time: LevelTimeFn,
    /// `actor_exists(ref) -> i32`.
    pub actor_exists: ActorExistsFn,
    /// `player_count() -> i32`.
    pub player_count: PlayerCountFn,
    /// `name_intern(ptr,len) -> NameRef`.
    pub name_intern: NameInternFn,
    /// `class_find(ptr,len) -> ClassRef`.
    pub class_find: ClassFindFn,
    /// `acs_execute(script, map, args)`.
    pub acs_execute: AcsExecuteFn,
    /// `lump_find(name, namespace) -> index`.
    pub lump_find: LumpFindFn,
    /// `lump_read` payload by index from [`Self::lump_find`].
    pub lump_bytes: LumpBytesFn,
    /// Registration-phase session (`reg_*` imports).
    pub reg: SharedRegistration,
    /// Game-control imports ([`GameFn`]); `None` → every call returns zero.
    pub game: Option<SharedGameBridge>,
}

impl Default for HostLinker {
    fn default() -> Self {
        let reg = shared_registration();
        let reg_find = Arc::clone(&reg);
        Self {
            log: Arc::new(|_level, _msg| {}),
            level_time: Arc::new(|| 0),
            actor_exists: Arc::new(|_r| 0),
            player_count: Arc::new(|| 0),
            name_intern: Arc::new(default_name_intern),
            class_find: Arc::new(move |bytes| {
                reg_find
                    .lock()
                    .ok()
                    .and_then(|g| g.find_class(bytes))
                    .unwrap_or(0)
            }),
            acs_execute: Arc::new(|_script, _map, _args| 0),
            lump_find: Arc::new(|_name, _ns| -1),
            lump_bytes: Arc::new(|_index| None),
            reg,
            game: None,
        }
    }
}

/// Deterministic FNV-1a NameRef (never 0) for hosts without a name table.
fn default_name_intern(bytes: &[u8]) -> u32 {
    if bytes.is_empty() {
        return 0;
    }
    let mut h = 2_166_136_261_u32;
    for &b in bytes {
        h ^= u32::from(b);
        h = h.wrapping_mul(16_777_619);
    }
    h | 1
}

impl HostLinker {
    /// Linker that records log lines into `sink` (for tests).
    #[must_use]
    pub fn with_log_sink(sink: Arc<std::sync::Mutex<Vec<(i32, String)>>>) -> Self {
        Self {
            log: Arc::new(move |level, msg| {
                if let Ok(mut g) = sink.lock() {
                    g.push((level, msg.to_owned()));
                }
            }),
            ..Default::default()
        }
    }
}

/// Backend-agnostic compile / instantiate surface (`docs/12` §4).
pub trait WasmBackend {
    /// Instantiated module type.
    type Instance: WasmInstance;

    /// Validate + compile `bytes`.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError::Invalid`] when the module fails policy checks.
    fn compile(&self, bytes: &[u8], cfg: &ModuleConfig) -> Result<CompiledModule, ScriptError>;

    /// Instantiate a compiled module.
    ///
    /// # Errors
    ///
    /// Returns backend link/start failures.
    fn instantiate(
        &self,
        module: &CompiledModule,
        linker: &HostLinker,
    ) -> Result<Self::Instance, ScriptError>;
}

/// Instantiated guest with call / memory access.
pub trait WasmInstance {
    /// Call an export by name with typed args/results.
    ///
    /// # Errors
    ///
    /// Trap or missing export.
    fn call(
        &mut self,
        name: &str,
        args: &[Val],
        results: &mut [Val],
        budget: &mut Budget,
    ) -> Result<(), ScriptError>;

    /// Read-only view of guest linear memory.
    fn memory(&mut self) -> &[u8];

    /// Overwrite guest linear memory at `offset` (save restore).
    ///
    /// # Errors
    ///
    /// OOB or missing memory.
    fn write_memory(&mut self, offset: usize, data: &[u8]) -> Result<(), ScriptError>;

    /// Whether an export exists.
    fn has_export(&mut self, name: &str) -> bool;
}

/// Lifecycle helpers for required ABI exports.
pub trait GuestLifecycle: WasmInstance {
    /// `room_abi_version`.
    ///
    /// # Errors
    ///
    /// Call trap / missing export.
    fn abi_version(&mut self, budget: &mut Budget) -> Result<i32, ScriptError> {
        let mut results = [Val::I32(0)];
        self.call("room_abi_version", &[], &mut results, budget)?;
        match results[0] {
            Val::I32(v) => Ok(v),
            _ => Err(ScriptError::Backend("abi_version type".into())),
        }
    }

    /// `room_register` — 0 = ok.
    ///
    /// # Errors
    ///
    /// Call trap / missing export.
    fn register(&mut self, budget: &mut Budget) -> Result<i32, ScriptError> {
        let mut results = [Val::I32(-1)];
        self.call("room_register", &[], &mut results, budget)?;
        match results[0] {
            Val::I32(v) => Ok(v),
            _ => Err(ScriptError::Backend("register type".into())),
        }
    }

    /// `room_init(session_kind)`.
    ///
    /// # Errors
    ///
    /// Call trap / missing export.
    fn init(&mut self, kind: SessionKind, budget: &mut Budget) -> Result<i32, ScriptError> {
        let mut results = [Val::I32(-1)];
        self.call("room_init", &[Val::I32(kind as i32)], &mut results, budget)?;
        match results[0] {
            Val::I32(v) => Ok(v),
            _ => Err(ScriptError::Backend("init type".into())),
        }
    }

    /// Optional `room_on_save` — returns blob length written at `buf_ptr`, or `None` if missing.
    ///
    /// # Errors
    ///
    /// Call trap.
    fn on_save(&mut self, buf_ptr: i32, budget: &mut Budget) -> Result<Option<i32>, ScriptError> {
        if !self.has_export("room_on_save") {
            return Ok(None);
        }
        let mut results = [Val::I32(0)];
        self.call("room_on_save", &[Val::I32(buf_ptr)], &mut results, budget)?;
        match results[0] {
            Val::I32(v) => Ok(Some(v)),
            _ => Err(ScriptError::Backend("on_save type".into())),
        }
    }

    /// Optional `room_on_load` after memory restore.
    ///
    /// # Errors
    ///
    /// Call trap.
    fn on_load(&mut self, budget: &mut Budget) -> Result<(), ScriptError> {
        if !self.has_export("room_on_load") {
            return Ok(());
        }
        self.call("room_on_load", &[], &mut [], budget)
    }

    /// `room_dispatch(callback_id, payload_ptr, payload_len)`.
    ///
    /// # Errors
    ///
    /// Call trap / missing export.
    fn dispatch(
        &mut self,
        callback_id: i32,
        payload_ptr: i32,
        payload_len: i32,
        budget: &mut Budget,
    ) -> Result<i64, ScriptError> {
        let mut results = [Val::I64(0)];
        self.call(
            "room_dispatch",
            &[
                Val::I32(callback_id),
                Val::I32(payload_ptr),
                Val::I32(payload_len),
            ],
            &mut results,
            budget,
        )?;
        match results[0] {
            Val::I64(v) => Ok(v),
            Val::I32(v) => Ok(i64::from(v)),
            _ => Err(ScriptError::Backend("dispatch type".into())),
        }
    }
}

impl<T: WasmInstance> GuestLifecycle for T {}

/// Validate wasm bytes against M11 policy (MVP subset).
///
/// # Errors
///
/// Unknown proposals, missing `memory`, oversized memory, or parse failure.
pub fn validate_module(bytes: &[u8], cfg: &ModuleConfig) -> Result<(), ScriptError> {
    let parser = wasmparser::Parser::new(0);
    let mut memories = 0u32;
    let mut max_pages = 0u32;
    let mut exports = std::collections::BTreeSet::new();
    for payload in parser.parse_all(bytes) {
        let payload = payload.map_err(|e| ScriptError::Invalid(e.to_string()))?;
        match payload {
            wasmparser::Payload::MemorySection(reader) => {
                for mem in reader {
                    let mem = mem.map_err(|e| ScriptError::Invalid(e.to_string()))?;
                    memories += 1;
                    let pages = u32::try_from(mem.initial).unwrap_or(u32::MAX);
                    max_pages = max_pages.max(pages);
                    if let Some(m) = mem.maximum {
                        max_pages = max_pages.max(u32::try_from(m).unwrap_or(u32::MAX));
                    }
                    if mem.memory64 {
                        return Err(ScriptError::Invalid("memory64 rejected".into()));
                    }
                    if mem.shared {
                        return Err(ScriptError::Invalid("shared memory rejected".into()));
                    }
                }
            }
            wasmparser::Payload::ExportSection(reader) => {
                for exp in reader {
                    let exp = exp.map_err(|e| ScriptError::Invalid(e.to_string()))?;
                    exports.insert(exp.name.to_owned());
                }
            }
            wasmparser::Payload::ImportSection(reader) => {
                for imp in reader {
                    let imp = imp.map_err(|e| ScriptError::Invalid(e.to_string()))?;
                    if imp.module != "room" {
                        return Err(ScriptError::Invalid(format!(
                            "import from {:?} rejected (only \"room\")",
                            imp.module
                        )));
                    }
                }
            }
            wasmparser::Payload::Version {
                encoding: wasmparser::Encoding::Module,
                ..
            } => {}
            wasmparser::Payload::Version { .. } => {
                return Err(ScriptError::Invalid("component modules rejected".into()));
            }
            _ => {}
        }
    }
    if memories != 1 {
        return Err(ScriptError::Invalid(
            "exactly one memory export required".into(),
        ));
    }
    if !exports.contains("memory") {
        return Err(ScriptError::Invalid("missing export memory".into()));
    }
    if max_pages > cfg.max_memory_pages {
        return Err(ScriptError::Invalid(format!(
            "memory max {max_pages} exceeds {}",
            cfg.max_memory_pages
        )));
    }
    for name in REQUIRED_EXPORTS {
        if !exports.contains(*name) {
            return Err(ScriptError::Invalid(format!("missing export {name}")));
        }
    }
    Ok(())
}

#[cfg(feature = "wasmi")]
mod wasmi_backend;

#[cfg(feature = "wasmi")]
pub use wasmi_backend::{WasmiBackend, WasmiInstance};

#[cfg(feature = "wasmtime")]
mod wasmtime_backend;

#[cfg(feature = "wasmtime")]
pub use wasmtime_backend::WasmtimeBackend;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use room_script_abi::ModuleScope;
    use std::sync::Mutex;

    fn stub_module_wat() -> Vec<u8> {
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
  (func (export "room_register") (result i32)
    (call $log (i32.const 1) (i32.const 0) (i32.const 2))
    (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  ;; 1=level_time 2=actor_exists(ptr) 3=player_count else 42
  (func (export "room_dispatch") (param $id i32) (param $ptr i32) (param $len i32) (result i64)
    (if (result i64) (i32.eq (local.get $id) (i32.const 1))
      (then (i64.extend_i32_s (call $level_time)))
      (else
        (if (result i64) (i32.eq (local.get $id) (i32.const 2))
          (then (i64.extend_i32_s (call $actor_exists (i64.extend_i32_s (local.get $ptr)))))
          (else
            (if (result i64) (i32.eq (local.get $id) (i32.const 3))
              (then (i64.extend_i32_s (call $player_count)))
              (else (i64.const 42))))))))
  (data (i32.const 0) "hi")
)"#,
        )
        .expect("wat")
    }

    #[test]
    fn validate_accepts_stub_module() {
        validate_module(&stub_module_wat(), &ModuleConfig::default()).expect("valid");
    }

    #[test]
    fn validate_rejects_missing_export() {
        let bytes = wat::parse_str(
            r#"(module
  (memory (export "memory") 1)
  (func (export "room_abi_version") (result i32) (i32.const 1))
)"#,
        )
        .unwrap();
        let err = validate_module(&bytes, &ModuleConfig::default()).unwrap_err();
        assert!(matches!(err, ScriptError::Invalid(m) if m.contains("room_alloc")));
    }

    #[test]
    fn validate_rejects_foreign_import() {
        let bytes = wat::parse_str(
            r#"(module
  (import "env" "abort" (func (param i32)))
  (memory (export "memory") 1)
  (func (export "room_abi_version") (result i32) (i32.const 1))
  (func (export "room_alloc") (param i32 i32) (result i32) (i32.const 0))
  (func (export "room_free") (param i32 i32 i32))
  (func (export "room_register") (result i32) (i32.const 0))
  (func (export "room_init") (param i32) (result i32) (i32.const 0))
  (func (export "room_dispatch") (param i32 i32 i32) (result i64) (i64.const 0))
)"#,
        )
        .unwrap();
        let err = validate_module(&bytes, &ModuleConfig::default()).unwrap_err();
        assert!(matches!(err, ScriptError::Invalid(m) if m.contains("env")));
    }

    fn host_for_lifecycle(sink: Arc<Mutex<Vec<(i32, String)>>>) -> HostLinker {
        HostLinker {
            log: HostLinker::with_log_sink(sink).log,
            level_time: Arc::new(|| 1234),
            actor_exists: Arc::new(|r| i32::from(r == 0x1000_0001)),
            player_count: Arc::new(|| 1),
            ..Default::default()
        }
    }

    fn assert_lifecycle<B: WasmBackend>(backend: &B, linker: &HostLinker) {
        let cfg = ModuleConfig {
            scope: ModuleScope::Play,
            ..Default::default()
        };
        let compiled = backend.compile(&stub_module_wat(), &cfg).unwrap();
        let mut inst = backend.instantiate(&compiled, linker).unwrap();
        let mut budget = Budget::from_config(&cfg);
        assert_eq!(inst.abi_version(&mut budget).unwrap(), ABI_VERSION);
        assert_eq!(inst.register(&mut budget).unwrap(), 0);
        assert_eq!(inst.init(SessionKind::NewGame, &mut budget).unwrap(), 0);
        assert_eq!(inst.dispatch(7, 0, 0, &mut budget).unwrap(), 42);
        assert_eq!(inst.dispatch(1, 0, 0, &mut budget).unwrap(), 1234);
        assert_eq!(inst.dispatch(2, 0x1000_0001, 0, &mut budget).unwrap(), 1);
        assert_eq!(inst.dispatch(2, 0, 0, &mut budget).unwrap(), 0);
        assert_eq!(inst.dispatch(3, 0, 0, &mut budget).unwrap(), 1);
    }

    #[cfg(feature = "wasmi")]
    #[test]
    fn script_call_sample() {
        use std::sync::Mutex;
        let bytes = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/script_call.wat"
        ))
        .unwrap();
        let starts = Arc::new(Mutex::new(Vec::new()));
        let starts2 = Arc::clone(&starts);
        let linker = HostLinker {
            acs_execute: Arc::new(move |script, map, args| {
                starts2.lock().unwrap().push((script, map, args.to_vec()));
                1
            }),
            ..Default::default()
        };
        let backend = WasmiBackend::new();
        let compiled = backend.compile(&bytes, &ModuleConfig::default()).unwrap();
        let mut inst = backend.instantiate(&compiled, &linker).unwrap();
        let mut budget = Budget::from_config(&ModuleConfig::default());
        assert_eq!(inst.register(&mut budget).unwrap(), 0);
        assert_eq!(
            linker.reg.lock().unwrap().static_functions()[0].func_name,
            "Add"
        );
        assert_eq!(inst.dispatch(200, 10, 5, &mut budget).unwrap(), 15);
        assert_eq!(inst.dispatch(201, 0, 0, &mut budget).unwrap(), 1);
        let q = starts.lock().unwrap();
        assert_eq!(q[0].0, 1);
        assert_eq!(q[0].2, vec![7, 8, 9]);
    }

    #[cfg(feature = "wasmi")]
    #[test]
    fn event_tick_sample_registers_handler() {
        let bytes = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/event_tick.wat"
        ))
        .unwrap();
        let linker = HostLinker::default();
        let backend = WasmiBackend::new();
        let compiled = backend.compile(&bytes, &ModuleConfig::default()).unwrap();
        let mut inst = backend.instantiate(&compiled, &linker).unwrap();
        let mut budget = Budget::from_config(&ModuleConfig::default());
        assert_eq!(inst.register(&mut budget).unwrap(), 0);
        let handlers = linker.reg.lock().unwrap().event_handlers().to_vec();
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].name, "TickCounter");
        assert_eq!(handlers[0].callback_base, 100);
        assert_eq!(inst.dispatch(100, 0, 0, &mut budget).unwrap(), 1);
        assert_eq!(inst.dispatch(100, 0, 0, &mut budget).unwrap(), 2);
        assert_eq!(linker.reg.lock().unwrap().thinker_classes().len(), 1);
    }

    #[cfg(feature = "wasmi")]
    #[test]
    fn fireimp_reg_sample() {
        let bytes = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/fireimp_reg.wat"
        ))
        .unwrap();
        let linker = HostLinker::default();
        let backend = WasmiBackend::new();
        let compiled = backend.compile(&bytes, &ModuleConfig::default()).unwrap();
        let mut inst = backend.instantiate(&compiled, &linker).unwrap();
        let mut budget = Budget::from_config(&ModuleConfig::default());
        assert_eq!(inst.register(&mut budget).unwrap(), 0);
        let classes = linker.reg.lock().unwrap().classes().to_vec();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, "FireImp");
        assert_eq!(classes[0].props_i32, [(1, 90)]);
        assert_eq!(inst.dispatch(1, 0, 0, &mut budget).unwrap(), 1_000);
    }

    #[cfg(feature = "wasmi")]
    #[test]
    fn hello_mod_sample_loads() {
        let bytes = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/hello_mod.wat"
        ))
        .unwrap();
        validate_module(&bytes, &ModuleConfig::default()).unwrap();
        let sink = Arc::new(Mutex::new(Vec::new()));
        let linker = HostLinker::with_log_sink(Arc::clone(&sink));
        let backend = WasmiBackend::new();
        let compiled = backend.compile(&bytes, &ModuleConfig::default()).unwrap();
        let mut inst = backend.instantiate(&compiled, &linker).unwrap();
        let mut budget = Budget::from_config(&ModuleConfig::default());
        assert_eq!(inst.register(&mut budget).unwrap(), 0);
        assert_eq!(sink.lock().unwrap()[0].1, "hello");
        let name = inst.dispatch(4, 0, 0, &mut budget).unwrap();
        assert!(name > 0);
        assert_eq!(inst.dispatch(5, 0, 0, &mut budget).unwrap(), 0);
    }

    #[cfg(feature = "wasmi")]
    #[test]
    fn wasmi_lifecycle_and_host_imports() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let linker = host_for_lifecycle(Arc::clone(&sink));
        assert_lifecycle(&WasmiBackend::new(), &linker);
        let logs = sink.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].1, "hi");
    }

    #[cfg(feature = "wasmtime")]
    #[test]
    fn wasmtime_lifecycle_matches_wasmi() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let linker = host_for_lifecycle(Arc::clone(&sink));
        let backend = WasmtimeBackend::new().unwrap();
        assert_lifecycle(&backend, &linker);
        assert_eq!(sink.lock().unwrap()[0].1, "hi");
    }

    #[cfg(all(feature = "wasmi", feature = "wasmtime"))]
    #[test]
    fn fireimp_reg_fingerprint_matches_across_backends() {
        let bytes = wat::parse_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/fireimp_reg.wat"
        ))
        .unwrap();
        let cfg = ModuleConfig::default();
        let wasmi_fp = {
            let linker = HostLinker::default();
            let backend = WasmiBackend::new();
            let compiled = backend.compile(&bytes, &cfg).unwrap();
            let mut inst = backend.instantiate(&compiled, &linker).unwrap();
            let mut budget = Budget::from_config(&cfg);
            assert_eq!(inst.register(&mut budget).unwrap(), 0);
            linker.reg.lock().unwrap().fingerprint()
        };
        let wasmtime_fp = {
            let linker = HostLinker::default();
            let backend = WasmtimeBackend::new().unwrap();
            let compiled = backend.compile(&bytes, &cfg).unwrap();
            let mut inst = backend.instantiate(&compiled, &linker).unwrap();
            let mut budget = Budget::from_config(&cfg);
            assert_eq!(inst.register(&mut budget).unwrap(), 0);
            linker.reg.lock().unwrap().fingerprint()
        };
        assert_eq!(wasmi_fp, wasmtime_fp);
        assert_ne!(wasmi_fp, 0);
    }
}
