//! Duke Nukem 3D native runtime (CON compiler/VM, actors, player, sectors).
//!
//! Runs at **30 Hz**. Depends on `room-build` (formats) and `room-build-engine`
//! (world + primitives). **Must not** depend on `room-sim`.
//!
//! Architecture: `docs/25-duke3d-full-support.md` §6–§7.
//! ADR: `docs/decisions/0004-native-build-runtime.md`.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

/// DEMO1 trace fix-ups for code outside [`session::DukeSession`] (free
/// functions with `// TEMP` hacks keyed on DEMO1 tic or sprite numbers).
/// Set from [`session::DukeSession::trace_fixups`] at the start of each tick;
/// off by default.
pub mod fixups {
    use std::cell::Cell;
    thread_local! {
        static ON: Cell<bool> = const { Cell::new(false) };
    }
    /// True while a session with `trace_fixups` is ticking on this thread.
    #[must_use]
    pub fn on() -> bool {
        ON.with(Cell::get)
    }
    /// Set by the session each tick.
    pub fn set(on: bool) {
        ON.with(|c| c.set(on));
    }
}

pub mod actors;
pub mod audio;
pub mod badguy;
pub mod campaign;
pub mod con;
mod cyclers;
pub mod demo;
pub mod demo_sync;
pub(crate) mod effectors;
pub mod explosions;
pub(crate) mod fallers;
pub mod fta;
pub(crate) mod interactions;
pub mod moveplayers;
pub mod player;
pub mod prelevel;
pub mod session;
pub mod shoot;
pub mod spawn;
pub mod standables;
pub mod stat;
pub mod trace;
pub mod transports;
pub mod view;
pub(crate) mod wall_animations;
pub mod weapons;
pub mod weapons_move;

pub use actors::moveactors;
pub use badguy::badguy_pic;
pub use campaign::{DukeCampaignProgress, DukeCampaignTransition, DukeLevelBonusStats};
pub use con::{
    ActorConTemps, ConStop, ConVersion, ConVm, IncludeResolver, KEYWORDS, LevelDef, Lexer,
    MAX_TILES, NUM_KEYWORDS, NUM_QUOTES, NUM_SOUNDS, NullIncludes, ScriptImage, SoundDef, compile,
    execute, keyword_index,
};
pub use demo::{
    DukeAtomicDemoPlayer, DukeAtomicDemoRecorder, DukeDemoError, DukeDemoRecorder, DukeDemoTic,
};
pub use demo_sync::{
    DEMO1_PRE_TIC0_SEED, DEMO1_TIC0_EXTRA_KRANDS, align_rng_pre_tic0, demo1_frontier,
    duke_map_lump, krand_unstep, play_demo_trace, session_from_grp_demo,
    session_from_grp_demo_layers, session_from_grp_layers, session_from_grp_layers_with_con,
    session_from_grp_layers_with_con_and_includes, session_from_grp_map,
    session_from_grp_map_bytes, session_from_grp_map_bytes_with_con,
    session_from_grp_map_bytes_with_con_and_includes,
};
pub use explosions::moveexplosions;
pub use fta::{findplayer_dist, movefta};
pub use moveplayers::moveplayers;
pub use player::{
    DUKE_FRICTION, DUKE_GC, DukeInput, INPUT_AUTO_AIM, INPUT_INVENTORY_LEFT, INPUT_INVENTORY_RIGHT,
    INPUT_INVENTORY_USE, INPUT_OPEN, PHEIGHT, Player, TICS_PER_FRAME, apply_player_fall_z,
    processinput_move, sync_player_sprite,
};
pub use prelevel::{CYCLER, GPSPEED, prelevel_sprites};
pub use session::ENTERLEVEL_RANDOMSEED;
pub use session::{AtomicSaveGlobals, DUKE_TIC_RATE, DukeSaveError, DukeSession, DukeSoundEvent};
pub use spawn::{APLAYER, SECTOREFFECTOR, classify_map_sprites, spawn_player_body};
pub use standables::movestandables;
pub use stat::{
    STAT_ACTIVATOR, STAT_ACTOR, STAT_DEFAULT, STAT_DUMMY, STAT_EFFECTOR, STAT_FALLER, STAT_FX,
    STAT_LOCATOR, STAT_MISC, STAT_PLAYER, STAT_PROJECTILE, STAT_STANDABLE, STAT_TRANSPORT,
};
pub use trace::{
    HEADER, MAX_WEAPONS, TraceDiff, TraceFile, TracePlayer, TraceTic, diff_traces, format_trace,
    hash_sprites, load_trace, parse_trace, sha256_hex,
};
pub use transports::movetransports;
pub use view::DukeSoftRenderer;
