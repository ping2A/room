//! Game state machine: startup, levels, hubs, intermission, saves, demos.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod demo;
mod engine_data;
mod headless;
mod music;
mod save;
mod sound;
mod vanilla_lmp;

pub use demo::{
    DemoBundle, DemoError, DemoPlayer, DemoRecorder, DemoVerifyResult, FDBN_FORMAT_VERSION,
    FDBN_MAGIC,
};
pub use engine_data::{
    EngineData, OPTIONAL_ENGINE_EXTRAS, embedded_engine_vfs, embedded_room_pk3,
    load_embedded_engine_data, load_engine_data, mount_embedded_engine,
    mount_optional_engine_extras,
};
pub use headless::{
    GameError, GameOptions, GameState, Headless, IntermissionTallies, Skill,
    apply_complevel_to_world, default_next_map_name, default_par_seconds, keyconf_alias_lines,
    keyconf_default_binds,
};
pub use music::{
    doom1_music_lump, doom2_music_lump, intermission_music_candidates, play_first_music_lump,
    play_map_music, play_music_lump, resolve_map_music, title_music_candidates,
};
pub use save::{
    SaveError, SaveInfo, load_world, read_save_info, save_world, validate_world_bin,
};
pub use sound::SoundSystem;
pub use vanilla_lmp::VanillaLmp;
