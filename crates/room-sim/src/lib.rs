//! Playsim: World, thinkers, actors, physics, and specials.
//!
//! Determinism rules in `docs/02-conventions.md` apply to all code in this crate.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod acs;
pub use acs::{
    push_acs_library, start_death_scripts, start_enter_scripts, start_respawn_scripts,
    start_return_scripts, start_unloading_scripts,
};
mod action;
mod actor;
mod bot;
mod class;
mod compat;
mod cross_lines;
mod damage;
mod deco_eval;
mod hitscan;
mod inventory;
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
mod script_host;
pub mod script_mods;
mod sector_specials;
mod sight;
mod sndseq;
mod specials;
mod state;
mod terrain;
mod think_player;
mod thinker;
mod use_lines;
mod vanilla;
pub mod view;
mod weapon_slots;
mod world;

pub use action::{ActionResult, call_action, call_action_args};
pub use actor::{Actor, ActorArena, ActorId};
pub use bot::run_bot_brains;
pub use class::{
    ClassDef, ClassError, ClassRegistry, FLAG_AMBUSH, FLAG_BOSS, FLAG_BOUNCEONFLOORS,
    FLAG_BOUNCEONWALLS, FLAG_BUDDHA, FLAG_CORPSE, FLAG_COUNTITEM, FLAG_COUNTKILL, FLAG_DONTDRAW,
    FLAG_DONTTHRUST, FLAG_DROPOFF, FLAG_DROPPED, FLAG_FLATSPRITE, FLAG_FLOAT, FLAG_FLOATBOB,
    FLAG_FLOORCLIP, FLAG_FRIENDLY, FLAG_INVULNERABLE, FLAG_MISSILE, FLAG_NOBLOCKMAP, FLAG_NOBLOOD,
    FLAG_NODAMAGE, FLAG_NOGRAVITY, FLAG_NORADIUSDMG, FLAG_NOTARGET, FLAG_RIPPER, FLAG_SHOOTABLE,
    FLAG_SOLID, FLAG_SPAWNCEILING, FLAG_USESPECIAL, FLAG_WALLSPRITE, RenderStyle,
};
pub use compat::{
    COMPATF_BOOMSCROLL, COMPATF_MBFMONSTERMOVE, COMPATF_TRACE, COMPATF2_NOMBF21, CompatFlags,
    CompatMode,
};
pub use cross_lines::cross_special_lines;
pub use damage::damage_mobj;
pub use hitscan::{TraceHit, ViewTrace, line_attack, view_trace};
pub use inventory::{
    InvItem, InventoryBags, cheat_give, count_inventory, give_inventory, set_inventory,
    take_inventory,
};
pub use level::LevelState;
pub use locks::{LockKeyReq, LockSpec, LockTable};
pub use mugshot::{MugShot, palette_for, palette_index};
pub use physics::{
    CheckResult, actor_light_level, actor_waterlevel, ceiling_at_xy, check_position, floor_at_xy,
    sector_at_xy, try_move, xy_movement, z_movement,
};
pub use pickup::{touch_specials, try_pickup};
pub use player::{Buttons, INVERSE_COLORMAP, MAX_PLAYERS, Player, PlayerState, TicCmd};
pub use player_names::{PlayerNames, sanitize_display_name};
pub use polyobj::{PolyDoor, PolyMotion, PolyMotionResume, PolyobjDynamic, tick_polyobjs};
pub use script_host::{ModuleMemorySnapshot, ScriptHost};
pub use sight::check_sight;
pub use sndseq::{ActiveSndSeq, SndSeqDef, SndSeqStep, SndSeqTable, start_sequence, stop_sequence};
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
    BossAction, CameraTextureBind, DEFAULT_BODY_QUEUE_SIZE, DEFAULT_CORPSE_QUEUE_SIZE, HudClipRect,
    HudMessage, KillEvent, LastDamageInfo, MAX_KILL_EVENTS, MatchOver, MatchStanding, MessageTo,
    PlayerHud, Quake, ScreenFade, SectorDynamic, ShotKind, ShotTrace, SimDecalEvent, SimError,
    SimParticleEvent, SimSoundEvent, Standing, World,
};
