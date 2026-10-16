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
#[repr(transparent)]
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

/// `reg_event_handler` event_mask bits (`docs/12` §9).
pub mod event_mask {
    /// `WorldTick`.
    pub const WORLD_TICK: u64 = 1 << 0;
    /// `WorldThingDied`.
    pub const WORLD_THING_DIED: u64 = 1 << 1;
    /// `NewGame`.
    pub const NEW_GAME: u64 = 1 << 2;
    /// `WorldLoaded`.
    pub const WORLD_LOADED: u64 = 1 << 3;
    /// `WorldThingSpawned`.
    pub const WORLD_THING_SPAWNED: u64 = 1 << 4;
    /// `WorldThingDamaged`.
    pub const WORLD_THING_DAMAGED: u64 = 1 << 5;
    /// `PlayerEntered`.
    pub const PLAYER_ENTERED: u64 = 1 << 6;
    /// `PlayerDied`.
    pub const PLAYER_DIED: u64 = 1 << 7;
    /// `WorldUnloaded`.
    pub const WORLD_UNLOADED: u64 = 1 << 8;
    /// `WorldItemPickup` (Room extension).
    pub const WORLD_ITEM_PICKUP: u64 = 1 << 9;
    /// `UiTick` (UI-scope modules).
    pub const UI_TICK: u64 = 1 << 16;
    /// `RenderOverlay` (UI-scope).
    pub const RENDER_OVERLAY: u64 = 1 << 17;
}

/// `reg_event_handler` flags.
pub mod event_handler_flags {
    /// Survives map changes (`StaticEventHandler`).
    pub const STATIC: u32 = 1 << 0;
    /// Map-only handler.
    pub const MAP_ONLY: u32 = 1 << 1;
}

/// Game import selectors (`"room"` `actor_*` / `level_*` / `player_*`).
pub mod game {
    /// `level_stat` selectors.
    pub mod level_stat {
        /// Monsters killed.
        pub const KILLS: i32 = 0;
        /// Total countable monsters.
        pub const TOTAL_KILLS: i32 = 1;
        /// Items picked up.
        pub const ITEMS: i32 = 2;
        /// Total countable items.
        pub const TOTAL_ITEMS: i32 = 3;
        /// Secrets found.
        pub const SECRETS: i32 = 4;
        /// Total secrets.
        pub const TOTAL_SECRETS: i32 = 5;
        /// Tics since the map started.
        pub const MAPTIME: i32 = 6;
    }

    /// `player_get_i32` selectors.
    pub mod player_prop {
        /// Slot in game (0/1).
        pub const IN_GAME: i32 = 0;
        /// Body health.
        pub const HEALTH: i32 = 1;
        /// Armor points.
        pub const ARMOR: i32 = 2;
        /// Kill count.
        pub const KILLS: i32 = 3;
        /// Item count.
        pub const ITEMS: i32 = 4;
        /// Secret count.
        pub const SECRETS: i32 = 5;
        /// Frags: kills of other players minus suicides.
        pub const FRAGS: i32 = 6;
    }

    /// `actor_get_i32` / `actor_set_i32` selectors.
    pub mod actor_i32 {
        /// Health (settable).
        pub const HEALTH: i32 = 0;
        /// Class spawn health.
        pub const SPAWN_HEALTH: i32 = 1;
        /// Thing ID (settable).
        pub const TID: i32 = 2;
        /// Raw `MF_*`-style flag bits (settable).
        pub const FLAGS: i32 = 3;
        /// `COUNTKILL` monster.
        pub const IS_MONSTER: i32 = 4;
        /// Player body: slot + 1 (0 = not a player).
        pub const PLAYER: i32 = 5;
        /// Missile.
        pub const IS_MISSILE: i32 = 6;
        /// Health ≤ 0.
        pub const IS_DEAD: i32 = 7;
        /// Missile / melee damage (settable).
        pub const DAMAGE: i32 = 8;
        /// Reaction time (settable).
        pub const REACTION_TIME: i32 = 9;
        /// Friendly to players (settable 0/1).
        pub const FRIENDLY: i32 = 10;
    }

    /// `actor_get_f64` / `actor_set_f64` selectors.
    pub mod actor_f64 {
        /// Collision radius.
        pub const RADIUS: i32 = 0;
        /// Height.
        pub const HEIGHT: i32 = 1;
        /// Movement speed (settable).
        pub const SPEED: i32 = 2;
        /// Render alpha (settable).
        pub const ALPHA: i32 = 3;
        /// Sprite X scale (settable).
        pub const SCALE_X: i32 = 4;
        /// Sprite Y scale (settable).
        pub const SCALE_Y: i32 = 5;
        /// Gravity factor (settable).
        pub const GRAVITY: i32 = 6;
        /// Incoming damage factor (settable).
        pub const DAMAGE_FACTOR: i32 = 7;
        /// Floor height under the actor.
        pub const FLOOR_Z: i32 = 8;
    }

    /// `actor_get_ref` / `actor_set_ref` selectors.
    pub mod actor_ptr {
        /// `target`.
        pub const TARGET: i32 = 0;
        /// `master`.
        pub const MASTER: i32 = 1;
        /// `tracer`.
        pub const TRACER: i32 = 2;
    }

    /// `actor_spawn` flags.
    pub mod spawn_flags {
        /// Spawn teleport fog + sound at the spot.
        pub const TELEFOG: i32 = 1;
        /// Refuse (return null) when the spot is blocked.
        pub const CHECK_POSITION: i32 = 2;
        /// Mark friendly to players.
        pub const FRIENDLY: i32 = 4;
    }

    /// `game_get_rule` / `game_set_rule` selectors. Rules reset when a map
    /// starts; set them from `WorldLoaded`.
    pub mod rule {
        /// Deathmatch: players hurt each other, player kills count as frags
        /// and the status bar shows FRAG (0/1, default 0).
        pub const DEATHMATCH: i32 = 0;
        /// Dead players respawn with Use (0/1, default 1); 0 keeps them down
        /// until the next map.
        pub const RESPAWN: i32 = 1;
        /// Read-only: tics left before the room time limit ends the match
        /// (0 = no limit). Same value on every seat.
        pub const TIME_LEFT: i32 = 2;
        /// Read-only: 1 once the match is over (`game_end_match` or time up).
        pub const MATCH_OVER: i32 = 3;
    }

    /// Map thing editor numbers for `level_things`.
    pub mod thing_type {
        /// Player 1 start (2–4 are 2..=4).
        pub const PLAYER1_START: i32 = 1;
        /// Deathmatch start.
        pub const DEATHMATCH_START: i32 = 11;
    }
}

/// `reg_class_property_i32` / `reg_class_property_str` ids (host `PropId`).
pub mod class_prop {
    /// Health.
    pub const HEALTH: u32 = 1;
    /// Speed (map units per tic).
    pub const SPEED: u32 = 2;
    /// PainChance (0..=256).
    pub const PAIN_CHANCE: u32 = 3;
    /// Radius.
    pub const RADIUS: u32 = 4;
    /// Height.
    pub const HEIGHT: u32 = 5;
    /// Mass.
    pub const MASS: u32 = 6;
    /// ReactionTime (tics).
    pub const REACTION_TIME: u32 = 7;
    /// MeleeRange.
    pub const MELEE_RANGE: u32 = 8;
    /// DamageFactor in percent.
    pub const DAMAGE_FACTOR_PERCENT: u32 = 9;
    /// Scale in percent.
    pub const SCALE_PERCENT: u32 = 10;
    /// FastSpeed (Nightmare).
    pub const FAST_SPEED: u32 = 11;
    /// Alpha in percent.
    pub const ALPHA_PERCENT: u32 = 12;
    /// Translation string (str).
    pub const TRANSLATION: u32 = 20;
    /// Tag / display name (str).
    pub const TAG: u32 = 21;
    /// Obituary (str).
    pub const OBITUARY: u32 = 22;
}

/// `reg_class_flag` ids.
pub mod class_flag {
    /// SOLID.
    pub const SOLID: u32 = 1;
    /// SHOOTABLE.
    pub const SHOOTABLE: u32 = 2;
    /// COUNTKILL.
    pub const COUNTKILL: u32 = 3;
    /// FRIENDLY.
    pub const FRIENDLY: u32 = 4;
    /// NOGRAVITY.
    pub const NOGRAVITY: u32 = 5;
    /// FLOAT.
    pub const FLOAT: u32 = 6;
    /// NOBLOOD.
    pub const NOBLOOD: u32 = 7;
    /// BOSS.
    pub const BOSS: u32 = 8;
    /// NORADIUSDMG.
    pub const NORADIUSDMG: u32 = 9;
    /// INVULNERABLE.
    pub const INVULNERABLE: u32 = 10;
    /// DONTDRAW.
    pub const DONTDRAW: u32 = 11;
}
