//! Playsim: World, thinkers, actors, physics, and specials.
//!
//! Determinism rules in `docs/02-conventions.md` apply to all code in this crate.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod acs;

static TRACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Turn the terminal traces of shots and line specials on or off (the app
/// follows the `developer` CVar or `ROOM_SIM_TRACE=1`). Off by default: on a
/// phone every line goes through logcat and the log file.
pub fn set_trace(on: bool) {
    TRACE.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether [`set_trace`] turned the terminal traces on.
#[must_use]
pub fn trace_on() -> bool {
    TRACE.load(std::sync::atomic::Ordering::Relaxed)
}
pub use acs::{
    puke_named, puke_script, push_acs_library, push_acs_library_tree, start_death_scripts,
    start_enter_scripts, start_library_open_scripts, start_respawn_scripts, start_return_scripts,
    start_unloading_scripts,
};
mod action;
mod hud_msg;
pub use hud_msg::{AcsHudMessage, HUDMSG_ADDBLEND, HUDMSG_ALPHA, HUDMSG_LOG, HUDMSG_NOWRAP};
mod action_known;
mod actor;
mod bot;
mod class;
mod compat;
mod cross_lines;
mod damage;
mod deco_eval;
mod deco_zvm;
mod flags;
mod render_pose;
pub use render_pose::{
    ModelFrameLerp, PIXEL_STRETCH, SpritePose, SpriteView, casts_sprite_shadow, isometric_sprite,
    model_frame_lerp, sprite_pose, sprite_view, view_eye,
};
pub mod fx_events;
mod hitscan;
pub mod invbar;
mod inventory;
mod predict;
pub use predict::PredictedView;
mod level;
mod locks;
pub mod mod_hud;
pub mod mod_zones;
mod mugshot;
mod physics;
mod pickup;
mod player;
mod player_names;
mod polyobj;
pub mod portal;
#[cfg(test)]
mod portal_tests;
pub mod powerups;
#[cfg(test)]
mod props_tests;
pub mod reach;
mod script_host;
pub mod script_mods;
mod sector_specials;
mod sight;
mod sight_portal;
mod sndseq;
pub mod sound_env;
mod special_names;
mod specials;
mod state;
mod terrain;
mod think_player;
mod thinker;
mod use_lines;
mod uz_constants;
mod uzact;
mod vanilla;
pub mod view;
mod weapon_slots;
mod world;
pub mod zs_native_ops;
pub mod zscript;

pub use action::{
    ActionResult, call_action, call_action_args, call_zscript_action, gender_name, parse_gender,
    sound_source_token,
};
pub use action_known::{
    canonical_action_name, is_known_action, is_known_action_ci, known_action_names,
};
pub use actor::{Actor, ActorArena, ActorId, ActorModelState, BoneComponent, BoneOffset, BonePart};
pub use bot::run_bot_brains;
pub use class::{
    ClassDef, ClassError, ClassRegistry, FLAG_AMBUSH, FLAG_BOSS, FLAG_BOUNCEONFLOORS,
    FLAG_BOUNCEONWALLS, FLAG_BUDDHA, FLAG_CORPSE, FLAG_COUNTITEM, FLAG_COUNTKILL, FLAG_DONTDRAW,
    FLAG_DONTTHRUST, FLAG_DORMANT, FLAG_DROPOFF, FLAG_DROPPED, FLAG_FLATSPRITE, FLAG_FLOAT,
    FLAG_FLOATBOB, FLAG_FLOORCLIP, FLAG_FRIENDLY, FLAG_INVULNERABLE, FLAG_MISSILE, FLAG_NOBLOCKMAP,
    FLAG_NOBLOOD, FLAG_NODAMAGE, FLAG_NOGRAVITY, FLAG_NOINTERACTION, FLAG_NORADIUSDMG,
    FLAG_NOTARGET, FLAG_RIPPER, FLAG_SHOOTABLE, FLAG_SOLID, FLAG_SPAWNCEILING, FLAG_USESPECIAL,
    FLAG_WALLSPRITE, RenderStyle,
};
pub use class::{PROPERTIES_PARSED_ONLY, STORED_PROPERTIES, is_known_flag, is_known_property};
pub use compat::{
    COMPATF_BOOMSCROLL, COMPATF_MBFMONSTERMOVE, COMPATF_TRACE, COMPATF2_NOMBF21, CompatFlags,
    CompatMode,
};
pub use cross_lines::cross_special_lines;
pub use damage::damage_mobj;
pub use flags::{ActorFlags, FLAG_WORDS, UZDOOM_FLAGS, is_uzdoom_flag};
pub use fx_events::{FxEvent, Rgb8 as FxRgb8};
pub use hitscan::{TraceHit, ViewTrace, line_attack, view_trace};
pub use inventory::{
    InvItem, InventoryBags, TravelState, apply_travel, capture_travel, cheat_give, count_inventory,
    give_inventory, inter_hub_amount, set_inventory, take_inventory,
};
pub use level::LevelState;
pub use locks::{LockKeyReq, LockSpec, LockTable};
pub use mugshot::{MugShot, palette_for, palette_index};
pub use physics::{
    CheckResult, actor_light_level, actor_waterlevel, ceiling_at_xy, check_position, floor_at_xy,
    sector_at_xy, try_move, xy_movement, z_movement,
};
pub use pickup::{hexen_armor_class, touch_specials, try_pickup};
pub use player::{
    Buttons, Frags, INVERSE_COLORMAP, LEGACY_SEATS, MAX_PLAYERS, Player, PlayerState, TicCmd,
    seat_bit,
};
pub use player_names::{PlayerNames, sanitize_display_name};
pub use polyobj::{PolyDoor, PolyMotion, PolyMotionResume, PolyobjDynamic, tick_polyobjs};
pub use script_host::{ModuleMemorySnapshot, ScriptHost};
pub use sight::check_sight;
pub use sndseq::{ActiveSndSeq, SndSeqDef, SndSeqStep, SndSeqTable, start_sequence, stop_sequence};
pub use special_names::{is_named_acs_action, is_special_action, special_number};
pub use specials::{
    ActivationType, CeilingMover, CeilingScroller, DoorMover, ElevatorMover, FloorMover,
    FloorScroller, FloorSkew, FloorSpinner, FloorStretch, FloorSubway, FloorTeeth, HeightTransfer,
    LightFade, LightFlicker, LightGlow, LightStrobe, LightTransfer, PlaneWaggle, PlatMover,
    PointForce, TextureScroller, activate_line, damage_linedef, execute_special, tick_demo_cameras,
    tick_doors, tick_plats, tick_skews, tick_spinners, tick_stretches, tick_subways, tick_teeth,
};
pub use state::{State, StateTable};
pub use terrain::{SplashSpec, TerrainSpec, TerrainTable};
pub use thinker::{StatNum, ThinkerLists};
pub use use_lines::use_lines;
pub use vanilla::{
    SimMathMode, approx_distance, check_position_fixed, math_mode_for_vanilla_demo, try_move_fixed,
    xy_movement_fixed, z_movement_fixed,
};
pub use weapon_slots::{PlayerClassEntry, WeaponSlots, default_slot_weapons};
pub use world::{
    BossAction, CameraTextureBind, DEFAULT_BODY_QUEUE_SIZE, DEFAULT_CORPSE_QUEUE_SIZE,
    DamageTypeInfo, HudClipRect, HudMessage, HudPspLayer, KillEvent, LastDamageInfo,
    MAX_KILL_EVENTS, MatchOver, MatchStanding, MessageKind, MessageTo, PlayerHud, Quake, ScoreRow,
    ScreenFade, SectorDynamic, ShotKind, ShotTrace, SimDecalEvent, SimError, SimParticleEvent,
    SimSoundAdjust, SimSoundEvent, Standing, StatusBarItems, World, psprite_scalelight,
};
pub use zscript::{ZscriptRuntime, ZscriptSaveBlob};
