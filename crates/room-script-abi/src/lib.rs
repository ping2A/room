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
    /// `PlayerRespawned` (`docs/27` Track C C8).
    pub const PLAYER_RESPAWNED: u64 = 1 << 10;
    /// `WorldThingDestroyed` (actor removed without a death event; C8).
    pub const WORLD_THING_DESTROYED: u64 = 1 << 11;
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
        /// Screen red flash (`player->damagecount`).
        pub const DAMAGECOUNT: i32 = 7;
        /// Screen gold flash (`player->bonuscount`).
        pub const BONUSCOUNT: i32 = 8;
        /// Weapon muzzle / flash extralight.
        pub const EXTRALIGHT: i32 = 9;
        /// Ready weapon inventory class ([`ClassRef`] bits; BE10).
        pub const READY_WEAPON: i32 = 10;
        /// Total damage dealt to other players this match (BE10).
        pub const DAMAGE_DEALT: i32 = 11;
        /// Armor save type: `0` none, `1` green, `2` blue (BE10).
        pub const ARMOR_TYPE: i32 = 12;
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
        /// Last damaging weapon class ([`ClassRef`] bits) on a player pawn (BE10).
        pub const LAST_DAMAGE_TYPE: i32 = 11;
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
        /// Teammates may damage each other (`0` = off, default `1`; BE5).
        pub const TEAM_DAMAGE: i32 = 4;
        /// Self damage allowed (`0` = off, default `1`; BE5).
        pub const SELF_DAMAGE: i32 = 5;
        /// PvP lethal hits floor at 1 HP for squad DBNO (`0/1`; BR6).
        pub const SQUAD_DBNO: i32 = 6;
        /// Pause playsim thinkers / damage / clocks (`0/1`). Script `WorldTick`
        /// and Mod HUD still run so menus (e.g. Survival draft) can resolve;
        /// `maptime` does not advance while paused.
        pub const PAUSED: i32 = 7;
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
    /// NOPAIN (skip pain state; uses NODAMAGE bit for script toggles).
    pub const NOPAIN: u32 = 12;
    /// BUDDHA — health floors at 1 on lethal hits (BE6).
    pub const BUDDHA: u32 = 13;
}

/// `player_trace` result kind (`docs/28` BE7).
pub mod trace_kind {
    /// Open air / max range.
    pub const MISS: i32 = 0;
    /// Struck a shootable actor.
    pub const ACTOR: i32 = 1;
    /// Hit a blocking linedef.
    pub const LINE: i32 = 2;
    /// Hit a floor/ceiling plane.
    pub const SECTOR: i32 = 3;
}

/// Retained Mod HUD ABI (`docs/27` §9.2).
///
/// [`HudWidgetV1`] is `#[repr(C, packed)]` so the on-wire layout is the field
/// order in the spec with no compiler padding. `size` is the prefix a guest
/// wrote; the host reads fields that fit and ignores a longer tail.
pub mod hud {
    /// Widget kind ids.
    pub mod kind {
        /// Text label.
        pub const TEXT: u8 = 0;
        /// Solid fill.
        pub const FILL: u8 = 1;
        /// Image / patch.
        pub const IMAGE: u8 = 2;
        /// Progress bar.
        pub const BAR: u8 = 3;
        /// Outline box.
        pub const BOX: u8 = 4;
        /// Panel (fill + border).
        pub const PANEL: u8 = 5;
        /// World marker.
        pub const MARKER: u8 = 6;
        /// Minimap / map widget (`docs/28` BE2).
        pub const MAP: u8 = 7;
    }

    /// MAP widget layers (stored in [`HudWidgetV1::max`]).
    pub mod map_layers {
        /// Automap geometry.
        pub const GEOMETRY: i32 = 1 << 0;
        /// Zone rings (BE1).
        pub const ZONES: i32 = 1 << 1;
        /// Seat markers.
        pub const MARKERS: i32 = 1 << 2;
        /// Teammates (BE5).
        pub const TEAMMATES: i32 = 1 << 3;
        /// Items of interest.
        pub const ITEMS: i32 = 1 << 4;
    }

    /// Draw layer.
    pub mod layer {
        /// Under the status bar.
        pub const UNDER_HUD: u8 = 0;
        /// Over the status bar.
        pub const OVER_HUD: u8 = 1;
        /// Modal (dims the scene).
        pub const MODAL: u8 = 2;
    }

    /// Anchor 0..=8 of the virtual 320×200 screen.
    pub mod anchor {
        /// Top left.
        pub const TOP_LEFT: u8 = 0;
        /// Top center.
        pub const TOP_CENTER: u8 = 1;
        /// Top right.
        pub const TOP_RIGHT: u8 = 2;
        /// Mid left.
        pub const MID_LEFT: u8 = 3;
        /// Center.
        pub const CENTER: u8 = 4;
        /// Mid right.
        pub const MID_RIGHT: u8 = 5;
        /// Bottom left.
        pub const BOTTOM_LEFT: u8 = 6;
        /// Bottom center.
        pub const BOTTOM_CENTER: u8 = 7;
        /// Bottom right.
        pub const BOTTOM_RIGHT: u8 = 8;
    }

    /// Text alignment. [`WRAP`] combines with the low 2 bits.
    pub mod align {
        /// Left.
        pub const LEFT: u8 = 0;
        /// Center.
        pub const CENTER: u8 = 1;
        /// Right.
        pub const RIGHT: u8 = 2;
        /// Wrap at widget width.
        pub const WRAP: u8 = 1 << 2;
    }

    /// Font id.
    pub mod font {
        /// SmallFont.
        pub const SMALL: u8 = 0;
        /// BigFont.
        pub const BIG: u8 = 1;
        /// Console font.
        pub const CONSOLE: u8 = 2;
        /// NewSmallFont.
        pub const NEW_SMALL: u8 = 3;
    }

    /// Widget flags (same bits as `room_sim::mod_hud::flags`).
    pub mod flags {
        /// Not drawn.
        pub const HIDDEN: u8 = 1 << 0;
        /// Survive map change.
        pub const PERSIST_MAP: u8 = 1 << 1;
        /// Hide when occluded (MH4).
        pub const OCCLUDED_HIDE: u8 = 1 << 2;
        /// Distance label (MH4).
        pub const SHOW_DISTANCE: u8 = 1 << 3;
        /// Pulse alpha.
        pub const PULSE: u8 = 1 << 4;
        /// Keep image aspect.
        pub const KEEP_ASPECT: u8 = 1 << 5;
        /// MAP widget rotates with viewer yaw.
        pub const MAP_ROTATE: u8 = 1 << 6;
    }

    /// `hud_widget_patch` field ids (same values as `room_sim::mod_hud::patch_field`).
    pub mod patch {
        /// Bar / counter value.
        pub const VALUE: i32 = 0;
        /// Bar max.
        pub const MAX: i32 = 1;
        /// Alpha 0..=1000.
        pub const ALPHA: i32 = 2;
        /// X offset.
        pub const X: i32 = 3;
        /// Y offset.
        pub const Y: i32 = 4;
        /// RGBA as i32 bits.
        pub const COLOR: i32 = 5;
        /// TEXTCOLO index.
        pub const TEXT_COLOR: i32 = 6;
        /// Hidden flag (nonzero = hide).
        pub const HIDDEN: i32 = 7;
        /// Z order.
        pub const Z: i32 = 8;
    }

    /// Packed little-endian widget header (`docs/27` §9.2).
    ///
    /// Text and image names are not inline: `text_off` / `name_off` are byte
    /// offsets from the start of this header into the same params buffer.
    #[repr(C, packed)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct HudWidgetV1 {
        /// Bytes of this header the guest wrote (`SIZE` for this version).
        pub size: u16,
        /// [`kind`] id.
        pub kind: u8,
        /// [`layer`] id.
        pub layer: u8,
        /// [`anchor`] 0..=8.
        pub anchor: u8,
        /// [`align`] bits.
        pub align: u8,
        /// [`font`] id.
        pub font: u8,
        /// [`flags`] bits.
        pub flags: u8,
        /// Offset from the anchor.
        pub x: i16,
        /// Offset from the anchor.
        pub y: i16,
        /// Width (`0` = natural).
        pub w: i16,
        /// Height.
        pub h: i16,
        /// Z-order inside the layer.
        pub z: i16,
        /// RGBA primary.
        pub color: u32,
        /// RGBA secondary.
        pub color2: u32,
        /// TEXTCOLO index (`255` = untranslated).
        pub text_color: u8,
        /// Alignment padding before `value`.
        pub _pad: [u8; 3],
        /// Bar value.
        pub value: i32,
        /// Bar max.
        pub max: i32,
        /// Bar segments (`0` = solid).
        pub segments: u16,
        /// Alpha 0..=1000.
        pub alpha: u16,
        /// Fade-in tics.
        pub fade_in: u16,
        /// Hold tics (`0` = until removed).
        pub hold: u16,
        /// Fade-out tics.
        pub fade_out: u16,
        /// Marker actor bits (`0` = use [`Self::world_xyz`]).
        pub actor: u64,
        /// Marker map position.
        pub world_xyz: [i32; 3],
        /// Graphic name offset from the header start.
        pub name_off: u16,
        /// Graphic name length.
        pub name_len: u16,
        /// UTF-8 text offset from the header start.
        pub text_off: u16,
        /// UTF-8 text length.
        pub text_len: u16,
    }

    const _: () = assert!(core::mem::size_of::<HudWidgetV1>() == 76);
    const _: () = assert!(core::mem::offset_of!(HudWidgetV1, color) == 18);
    const _: () = assert!(core::mem::offset_of!(HudWidgetV1, value) == 30);
    const _: () = assert!(core::mem::offset_of!(HudWidgetV1, actor) == 48);
    const _: () = assert!(core::mem::offset_of!(HudWidgetV1, text_off) == 72);
    const _: () = assert!(core::mem::offset_of!(HudWidgetV1, text_len) == 74);

    impl HudWidgetV1 {
        /// On-wire size of this version.
        pub const SIZE: u16 = 76;

        /// Defaults used when an older prefix omits the tail (`docs/27` §9.2).
        #[must_use]
        pub const fn guest_default() -> Self {
            Self {
                size: Self::SIZE,
                kind: kind::TEXT,
                layer: layer::OVER_HUD,
                anchor: anchor::TOP_CENTER,
                align: align::LEFT,
                font: font::SMALL,
                flags: 0,
                x: 0,
                y: 24,
                w: 0,
                h: 0,
                z: 0,
                color: 0xffff_ffff,
                color2: 0,
                text_color: 255,
                _pad: [0; 3],
                value: 0,
                max: 0,
                segments: 0,
                alpha: 1000,
                fade_in: 0,
                hold: 0,
                fade_out: 0,
                actor: 0,
                world_xyz: [0; 3],
                name_off: 0,
                name_len: 0,
                text_off: 0,
                text_len: 0,
            }
        }

        /// Decode a prefix. `None` when `size` is missing or larger than `params`.
        ///
        /// Fields past `size` stay at [`Self::guest_default`]. Bytes past the
        /// known 76-byte header are ignored (newer guests).
        #[must_use]
        pub fn decode_prefix(params: &[u8]) -> Option<Self> {
            if params.len() < 2 {
                return None;
            }
            let claimed = usize::from(u16::from_le_bytes([params[0], params[1]]));
            if claimed < 2 || claimed > params.len() {
                return None;
            }
            let mut w = Self::guest_default();
            w.size = claimed as u16;
            let lim = claimed;
            if let Some(v) = u8_at(params, 2, lim) {
                w.kind = v;
            }
            if let Some(v) = u8_at(params, 3, lim) {
                w.layer = v;
            }
            if let Some(v) = u8_at(params, 4, lim) {
                w.anchor = v;
            }
            if let Some(v) = u8_at(params, 5, lim) {
                w.align = v;
            }
            if let Some(v) = u8_at(params, 6, lim) {
                w.font = v;
            }
            if let Some(v) = u8_at(params, 7, lim) {
                w.flags = v;
            }
            if let Some(v) = i16_at(params, 8, lim) {
                w.x = v;
            }
            if let Some(v) = i16_at(params, 10, lim) {
                w.y = v;
            }
            if let Some(v) = i16_at(params, 12, lim) {
                w.w = v;
            }
            if let Some(v) = i16_at(params, 14, lim) {
                w.h = v;
            }
            if let Some(v) = i16_at(params, 16, lim) {
                w.z = v;
            }
            if let Some(v) = u32_at(params, 18, lim) {
                w.color = v;
            }
            if let Some(v) = u32_at(params, 22, lim) {
                w.color2 = v;
            }
            if let Some(v) = u8_at(params, 26, lim) {
                w.text_color = v;
            }
            if let Some(v) = i32_at(params, 30, lim) {
                w.value = v;
            }
            if let Some(v) = i32_at(params, 34, lim) {
                w.max = v;
            }
            if let Some(v) = u16_at(params, 38, lim) {
                w.segments = v;
            }
            if let Some(v) = u16_at(params, 40, lim) {
                w.alpha = v;
            }
            if let Some(v) = u16_at(params, 42, lim) {
                w.fade_in = v;
            }
            if let Some(v) = u16_at(params, 44, lim) {
                w.hold = v;
            }
            if let Some(v) = u16_at(params, 46, lim) {
                w.fade_out = v;
            }
            if let Some(v) = u64_at(params, 48, lim) {
                w.actor = v;
            }
            if let Some(v) = i32_at(params, 56, lim) {
                w.world_xyz[0] = v;
            }
            if let Some(v) = i32_at(params, 60, lim) {
                w.world_xyz[1] = v;
            }
            if let Some(v) = i32_at(params, 64, lim) {
                w.world_xyz[2] = v;
            }
            if let Some(v) = u16_at(params, 68, lim) {
                w.name_off = v;
            }
            if let Some(v) = u16_at(params, 70, lim) {
                w.name_len = v;
            }
            if let Some(v) = u16_at(params, 72, lim) {
                w.text_off = v;
            }
            if let Some(v) = u16_at(params, 74, lim) {
                w.text_len = v;
            }
            Some(w)
        }

        /// Write this header as little-endian bytes. False when `out` is short.
        pub fn write_le(self, out: &mut [u8]) -> bool {
            if out.len() < usize::from(Self::SIZE) {
                return false;
            }
            put_u16(out, 0, self.size);
            out[2] = self.kind;
            out[3] = self.layer;
            out[4] = self.anchor;
            out[5] = self.align;
            out[6] = self.font;
            out[7] = self.flags;
            put_i16(out, 8, self.x);
            put_i16(out, 10, self.y);
            put_i16(out, 12, self.w);
            put_i16(out, 14, self.h);
            put_i16(out, 16, self.z);
            put_u32(out, 18, self.color);
            put_u32(out, 22, self.color2);
            out[26] = self.text_color;
            let pad = self._pad;
            out[27] = pad[0];
            out[28] = pad[1];
            out[29] = pad[2];
            put_i32(out, 30, self.value);
            put_i32(out, 34, self.max);
            put_u16(out, 38, self.segments);
            put_u16(out, 40, self.alpha);
            put_u16(out, 42, self.fade_in);
            put_u16(out, 44, self.hold);
            put_u16(out, 46, self.fade_out);
            put_u64(out, 48, self.actor);
            let xyz = self.world_xyz;
            put_i32(out, 56, xyz[0]);
            put_i32(out, 60, xyz[1]);
            put_i32(out, 64, xyz[2]);
            put_u16(out, 68, self.name_off);
            put_u16(out, 70, self.name_len);
            put_u16(out, 72, self.text_off);
            put_u16(out, 74, self.text_len);
            true
        }
    }

    fn fits(off: usize, n: usize, lim: usize) -> bool {
        off.checked_add(n).is_some_and(|end| end <= lim)
    }

    fn u8_at(b: &[u8], off: usize, lim: usize) -> Option<u8> {
        fits(off, 1, lim).then(|| b[off])
    }

    fn u16_at(b: &[u8], off: usize, lim: usize) -> Option<u16> {
        fits(off, 2, lim).then(|| u16::from_le_bytes([b[off], b[off + 1]]))
    }

    fn i16_at(b: &[u8], off: usize, lim: usize) -> Option<i16> {
        u16_at(b, off, lim).map(|v| v as i16)
    }

    fn u32_at(b: &[u8], off: usize, lim: usize) -> Option<u32> {
        fits(off, 4, lim).then(|| u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]))
    }

    fn i32_at(b: &[u8], off: usize, lim: usize) -> Option<i32> {
        u32_at(b, off, lim).map(|v| v as i32)
    }

    fn u64_at(b: &[u8], off: usize, lim: usize) -> Option<u64> {
        fits(off, 8, lim).then(|| {
            u64::from_le_bytes([
                b[off],
                b[off + 1],
                b[off + 2],
                b[off + 3],
                b[off + 4],
                b[off + 5],
                b[off + 6],
                b[off + 7],
            ])
        })
    }

    fn put_u16(out: &mut [u8], off: usize, v: u16) {
        out[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn put_i16(out: &mut [u8], off: usize, v: i16) {
        put_u16(out, off, v as u16);
    }

    fn put_u32(out: &mut [u8], off: usize, v: u32) {
        out[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn put_i32(out: &mut [u8], off: usize, v: i32) {
        put_u32(out, off, v as u32);
    }

    fn put_u64(out: &mut [u8], off: usize, v: u64) {
        out[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn short_prefix_keeps_defaults_and_ignores_a_longer_tail() {
            let mut short = [0u8; 8];
            short[0] = 8;
            short[2] = kind::FILL;
            let w = HudWidgetV1::decode_prefix(&short).unwrap_or_else(HudWidgetV1::guest_default);
            let (kind_id, y, alpha, color, text_color) =
                (w.kind, w.y, w.alpha, w.color, w.text_color);
            assert_eq!(kind_id, kind::FILL);
            assert_eq!(y, 24);
            assert_eq!(alpha, 1000);
            assert_eq!(color, 0xffff_ffff);
            assert_eq!(text_color, 255);

            let mut full = HudWidgetV1::guest_default();
            full.kind = kind::BAR;
            full.color = 0x1122_3344;
            full.value = 7;
            let mut buf = [0u8; 84];
            assert!(full.write_le(&mut buf));
            buf[0] = 84;
            buf[1] = 0;
            buf[80] = 0xab;
            let back = HudWidgetV1::decode_prefix(&buf).unwrap_or_else(HudWidgetV1::guest_default);
            let (kind_id, color, value, size) = (back.kind, back.color, back.value, back.size);
            assert_eq!(kind_id, kind::BAR);
            assert_eq!(color, 0x1122_3344);
            assert_eq!(value, 7);
            assert_eq!(size, 84);
            assert!(HudWidgetV1::decode_prefix(&[1, 0]).is_none());
            assert!(HudWidgetV1::decode_prefix(&[]).is_none());
        }
    }
}

pub use hud::HudWidgetV1;

/// Battle royale zone ring metadata (`docs/28` §9 BE1).
pub mod zone {
    /// Zone draw flags (same bits as `room_sim::mod_zones::flags`).
    pub mod flags {
        /// Not drawn.
        pub const HIDDEN: u8 = 1 << 0;
        /// wgpu wall visible through geometry (reserved).
        pub const THROUGH_WALLS: u8 = 1 << 1;
    }

    /// Import error codes (same values as `room_sim::mod_zones::err`).
    pub mod err {
        /// Bad parameters.
        pub const BAD_PARAMS: i32 = -1;
        /// Zone cap hit.
        pub const CAP: i32 = -2;
        /// Unknown id on clear.
        pub const MISSING: i32 = -3;
    }
}

/// Per-tic player command snapshot for mods (`docs/27` §9.3).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlayerCmdV1 {
    /// `size_of::<PlayerCmdV1>()` at the guest's ABI.
    pub size: u16,
    /// Weapon slot request (`0` = none).
    pub weapon_select: u8,
    /// Padding.
    pub _pad: u8,
    /// Button bits (same layout as playsim `Buttons`).
    pub buttons: u32,
    /// Rising edges this tic (`buttons & !prev`).
    pub pressed: u32,
    /// Falling edges this tic.
    pub released: u32,
    /// Forward move.
    pub forward: i16,
    /// Side move.
    pub side: i16,
    /// Yaw delta.
    pub yaw: i16,
    /// Pitch delta.
    pub pitch: i16,
}

/// View hitscan trace for mods (`docs/28` BE7).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerTraceV1 {
    /// `size_of::<PlayerTraceV1>()` at the guest's ABI.
    pub size: u16,
    /// [`trace_kind`].
    pub kind: u8,
    /// Padding.
    pub _pad: u8,
    /// Impact X (map units).
    pub hit_x: f64,
    /// Impact Y.
    pub hit_y: f64,
    /// Impact Z.
    pub hit_z: f64,
    /// Struck actor (`ActorRef`, 0 = none).
    pub actor: u64,
    /// Linedef index when `kind == LINE`.
    pub line_index: u32,
    /// Sector index when `kind == SECTOR`.
    pub sector_index: u32,
    /// Horizontal travel distance.
    pub distance: f64,
}

impl PlayerTraceV1 {
    /// Bytes written by the host for the current layout.
    pub const SIZE: u16 = 56;

    /// Little-endian encode into `out` (needs ≥ 56 bytes).
    pub fn write_le(self, out: &mut [u8]) -> bool {
        if out.len() < 56 {
            return false;
        }
        out[0..2].copy_from_slice(&self.size.to_le_bytes());
        out[2] = self.kind;
        out[3] = self._pad;
        out[4..12].copy_from_slice(&self.hit_x.to_le_bytes());
        out[12..20].copy_from_slice(&self.hit_y.to_le_bytes());
        out[20..28].copy_from_slice(&self.hit_z.to_le_bytes());
        out[28..36].copy_from_slice(&self.actor.to_le_bytes());
        out[36..40].copy_from_slice(&self.line_index.to_le_bytes());
        out[40..44].copy_from_slice(&self.sector_index.to_le_bytes());
        out[44..52].copy_from_slice(&self.distance.to_le_bytes());
        true
    }
}

/// One seat row for `game_end_match_ex` (BE11).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchStandingV1 {
    /// `size_of::<MatchStandingV1>()` at the guest's ABI.
    pub size: u16,
    /// Player slot (0-based).
    pub seat: u8,
    /// 1-based placement.
    pub place: u8,
    /// Frags / player kills this match.
    pub kills: i32,
    /// Assist count (mod-defined; engine stores as given).
    pub assists: i32,
    /// Damage dealt (typically matches [`player_prop::DAMAGE_DEALT`]).
    pub damage: i32,
    /// Teammate revives credited to this seat.
    pub revives: i32,
    /// Tics alive from deploy to elimination or match end.
    pub survival_tics: i32,
}

impl MatchStandingV1 {
    /// Bytes per row in guest memory.
    pub const SIZE: u16 = 24;

    /// Decode one little-endian row.
    #[must_use]
    pub fn read_le(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 24 {
            return None;
        }
        let size = u16::from_le_bytes([bytes[0], bytes[1]]);
        if size != Self::SIZE {
            return None;
        }
        Some(Self {
            size,
            seat: bytes[2],
            place: bytes[3],
            kills: i32::from_le_bytes(bytes[4..8].try_into().ok()?),
            assists: i32::from_le_bytes(bytes[8..12].try_into().ok()?),
            damage: i32::from_le_bytes(bytes[12..16].try_into().ok()?),
            revives: i32::from_le_bytes(bytes[16..20].try_into().ok()?),
            survival_tics: i32::from_le_bytes(bytes[20..24].try_into().ok()?),
        })
    }

    /// Little-endian encode into `out` (needs ≥ 24 bytes).
    pub fn write_le(self, out: &mut [u8]) -> bool {
        if out.len() < 24 {
            return false;
        }
        out[0..2].copy_from_slice(&Self::SIZE.to_le_bytes());
        out[2] = self.seat;
        out[3] = self.place;
        out[4..8].copy_from_slice(&self.kills.to_le_bytes());
        out[8..12].copy_from_slice(&self.assists.to_le_bytes());
        out[12..16].copy_from_slice(&self.damage.to_le_bytes());
        out[16..20].copy_from_slice(&self.revives.to_le_bytes());
        out[20..24].copy_from_slice(&self.survival_tics.to_le_bytes());
        true
    }
}

impl PlayerCmdV1 {
    /// Bytes written by the host for the current layout.
    pub const SIZE: u16 = 24;

    /// Little-endian encode into `out` (needs ≥ 24 bytes).
    pub fn write_le(self, out: &mut [u8]) -> bool {
        if out.len() < 24 {
            return false;
        }
        out[0..2].copy_from_slice(&self.size.to_le_bytes());
        out[2] = self.weapon_select;
        out[3] = self._pad;
        out[4..8].copy_from_slice(&self.buttons.to_le_bytes());
        out[8..12].copy_from_slice(&self.pressed.to_le_bytes());
        out[12..16].copy_from_slice(&self.released.to_le_bytes());
        out[16..18].copy_from_slice(&self.forward.to_le_bytes());
        out[18..20].copy_from_slice(&self.side.to_le_bytes());
        out[20..22].copy_from_slice(&self.yaw.to_le_bytes());
        out[22..24].copy_from_slice(&self.pitch.to_le_bytes());
        true
    }
}

/// Session display names (`docs/28` §9 BE3).
pub mod player_name {
    /// Max UTF-8 chars kept (matches lobby / `room-server`).
    pub const MAX_CHARS: usize = 24;
    /// Max UTF-8 bytes stored per seat (`MAX_CHARS` × worst-case scalar width).
    pub const MAX_BYTES: usize = 96;
}

/// Input-capture mask bits for `player_capture_input` (`docs/27` §9.3).
pub mod capture {
    /// Slot keys 1..7 (`weapon_select`).
    pub const WEAPON_SELECT: u32 = 1 << 0;
    /// `weapnext` / `weapprev`.
    pub const WEAPON_CYCLE: u32 = 1 << 1;
    /// Attack.
    pub const ATTACK: u32 = 1 << 2;
    /// Use.
    pub const USE: u32 = 1 << 3;
    /// Forward / side / up.
    pub const MOVE: u32 = 1 << 4;
    /// Yaw / pitch.
    pub const TURN: u32 = 1 << 5;
    /// Jump.
    pub const JUMP: u32 = 1 << 6;
}
