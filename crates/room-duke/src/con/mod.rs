//! Duke CON 1.5 compiler (D4) and VM (D5).
//!
//! Spec: `docs/25-duke3d-full-support.md` §6. Opcode numbering matches
//! Chocolate `GAMEDEF.C` `keyw[]`.

mod compile;
mod keywords;
mod lex;
pub mod vm;

pub use compile::{
    ConVersion, IncludeResolver, LevelDef, MAX_TILES, NUM_QUOTES, NUM_SOUNDS, NullIncludes,
    ScriptImage, SoundDef, compile,
};
pub use keywords::{KEYWORDS, NUM_KEYWORDS, keyword_index};
pub use lex::{Lexer, is_altok, is_special};
pub(crate) use vm::tick_action_frames;
pub use vm::{
    ActorConTemps, ConSpriteEnv, ConStop, ConVm, actor_header_hitag, execute, execute_actor,
    execute_with_sprite, init_actor_temps_from_script,
};
