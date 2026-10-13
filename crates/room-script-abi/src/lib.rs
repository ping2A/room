//! #![no_std] ABI types shared between host and WASM guests (`docs/12-wasm-scripting.md`).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![no_std]

/// ABI major version returned by guest `room_abi_version`.
pub const ABI_VERSION: i32 = 1;

/// Module load/runtime configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleConfig {
    /// `play` or `ui` scope.
    pub scope: ModuleScope,
    /// Max linear memory pages (64 KiB each).
    pub max_memory_pages: u32,
    /// Fuel (instruction budget) per top-level host→guest call.
    pub fuel_per_call: u64,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            scope: ModuleScope::Play,
            max_memory_pages: 4096,
            fuel_per_call: 50_000_000,
        }
    }
}

/// Guest module scope (`docs/12` §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleScope {
    /// Mutating, deterministic playsim module.
    Play,
    /// Frame/UI module (read-only world queries).
    Ui,
}

/// WASM value crossing the host↔guest boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Val {
    /// `i32`.
    I32(i32),
    /// `i64`.
    I64(i64),
    /// `f32` (bits preserved).
    F32(u32),
    /// `f64` (bits preserved).
    F64(u64),
}

/// Opaque export name / id for host calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExportId(pub u32);

/// Fuel / nesting budget for a call.
#[derive(Clone, Debug)]
pub struct Budget {
    /// Remaining fuel units.
    pub fuel: u64,
    /// Remaining host↔guest nesting depth.
    pub depth: u32,
}

impl Budget {
    /// New budget from [`ModuleConfig`].
    #[must_use]
    pub fn from_config(cfg: &ModuleConfig) -> Self {
        Self {
            fuel: cfg.fuel_per_call,
            depth: 64,
        }
    }
}

/// Trap reasons from guest execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trap {
    /// Unreachable / panic.
    Unreachable,
    /// Out-of-bounds memory.
    MemoryOutOfBounds,
    /// Fuel exhausted.
    OutOfFuel,
    /// Nesting depth exceeded.
    CallDepth,
    /// Missing required export.
    MissingExport(&'static str),
    /// Validation / link failure message (static).
    InvalidModule(&'static str),
    /// Other host-side error.
    Host(&'static str),
}

/// `room_init` session kind (`docs/12` §5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum SessionKind {
    /// Fresh game / map start.
    NewGame = 0,
    /// Restored from save.
    LoadGame = 1,
    /// Hot-reload of module bytes.
    HotReload = 2,
}

/// Required guest exports (`docs/12` §5.1).
pub const REQUIRED_EXPORTS: &[&str] = &[
    "room_abi_version",
    "room_alloc",
    "room_free",
    "room_register",
    "room_init",
    "room_dispatch",
];

/// Actor handle encoding (`index | generation << 32`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ActorRef(pub u64);

impl ActorRef {
    /// Null / destroyed handle.
    pub const NULL: Self = Self(0);

    /// Whether this is the null handle.
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// Interned name handle (`docs/12` §5.2; 0 = null).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NameRef(pub u32);

impl NameRef {
    /// Null name.
    pub const NULL: Self = Self(0);

    /// Whether this is null.
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// Class registry handle (`ClassId.raw() + 1`; 0 = null).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClassRef(pub u32);

impl ClassRef {
    /// Null / missing class.
    pub const NULL: Self = Self(0);

    /// Whether this is null.
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}
