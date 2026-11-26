//! Duke CON 1.5 compiler (D4) and VM (D5).
//!
//! Spec: `docs/25-duke3d-full-support.md` §6. Opcode numbering matches
//! Chocolate `GAMEDEF.C` `keyw[]`.

mod compile;
mod eduke_vm;
mod keywords;
mod lex;
pub mod vm;

pub use compile::{
    ConCompileOptions, ConDialect, ConVersion, EDUKE_MAX_QUOTES, EDUKE_MAX_SOUNDS, EDUKE_MAX_TILES,
    EdArrayDefinition, EdEventDefinition, EdProjectileProperty, EdStateDefinition,
    EdVariableDefinition, IncludeResolver, LevelDef, MAX_TILES, NUM_QUOTES, NUM_SOUNDS,
    NullIncludes, ScriptImage, SoundDef, compile, compile_with_options,
};
pub use eduke_vm::{
    EDUKE_MAXPLAYERS, EdActorMember, EdBinaryOperator, EdComparison, EdExecution, EdExecutions,
    EdInstruction, EdItemSprayKind, EdLValue, EdOperand, EdSoundMode, EdSpriteListKind,
    EdStructKind, EdVariableScope, EdukeProgram, EdukeStructAccess, EdukeVmError, EdukeVmState,
    build_cos_q14, build_divscale, build_getangle, build_mulscale, build_sin_q14, evaluate_binary,
};
pub use keywords::{KEYWORDS, NUM_KEYWORDS, keyword_index};
pub use lex::{Lexer, is_altok, is_special};
pub use vm::{
    ActorConTemps, ConProjectile, ConSpriteEnv, ConStop, ConVm, PendingDebris, PendingGuts,
    PendingHitRadius, actor_header_hitag, direct_hit_payload, execute, execute_actor,
    execute_with_sprite, execute_with_sprite_id, init_actor_temps_from_script,
};
pub(crate) use vm::{eduke_update_anim, tick_action_frames};
