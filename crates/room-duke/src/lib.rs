//! Duke Nukem 3D native runtime (CON compiler/VM, actors, player, sectors).
//!
//! Runs at **30 Hz**. Depends on `room-build` (formats) and `room-build-engine`
//! (world + primitives). **Must not** depend on `room-sim`.
//!
//! Architecture: `docs/25-duke3d-full-support.md` §6–§7.
//! ADR: `docs/decisions/0004-native-build-runtime.md`.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod actors;
pub mod badguy;
pub mod con;
pub mod demo_sync;
pub mod explosions;
pub mod fta;
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
pub mod weapons;
pub mod weapons_move;

pub use actors::moveactors;
pub use badguy::badguy_pic;
pub use con::{
    ActorConTemps, ConStop, ConVersion, ConVm, IncludeResolver, KEYWORDS, LevelDef, Lexer,
    MAX_TILES, NUM_KEYWORDS, NUM_QUOTES, NUM_SOUNDS, NullIncludes, ScriptImage, SoundDef, compile,
    execute, keyword_index,
};
pub use demo_sync::{
    DEMO1_PRE_TIC0_SEED, DEMO1_TIC0_EXTRA_KRANDS, align_rng_pre_tic0, demo1_frontier,
    duke_map_lump, krand_unstep, play_demo_trace, session_from_grp_demo,
};
pub use explosions::moveexplosions;
pub use fta::{findplayer_dist, movefta};
pub use moveplayers::moveplayers;
pub use player::{
    DUKE_FRICTION, DUKE_GC, DukeInput, PHEIGHT, Player, TICS_PER_FRAME, apply_player_fall_z,
    processinput_move, sync_player_sprite,
};
pub use prelevel::{CYCLER, GPSPEED, prelevel_sprites};
pub use session::ENTERLEVEL_RANDOMSEED;
pub use session::{DUKE_TIC_RATE, DukeSession};
pub use spawn::{APLAYER, SECTOREFFECTOR, classify_map_sprites, spawn_player_body};
pub use standables::movestandables;
pub use stat::{
    STAT_ACTIVATOR, STAT_ACTOR, STAT_DEFAULT, STAT_DUMMY, STAT_EFFECTOR, STAT_FX, STAT_LOCATOR,
    STAT_MISC, STAT_PLAYER, STAT_PROJECTILE, STAT_STANDABLE, STAT_TRANSPORT,
};
pub use trace::{
    HEADER, MAX_WEAPONS, TraceDiff, TraceFile, TracePlayer, TraceTic, diff_traces, format_trace,
    hash_sprites, load_trace, parse_trace, sha256_hex,
};
pub use transports::movetransports;
