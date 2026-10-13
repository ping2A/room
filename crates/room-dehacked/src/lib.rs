//! DeHackEd/BEX/MBF21 patcher for ClassDef IR.
//!
//! Parses `Thing` / `Weapon` / `Ammo` / `Frame` / `Pointer` / `Sound` / `Sprite` /
//! `Misc` and BEX `[STRINGS]` / `[CODEPTR]` / `[SOUNDS]` / `[SPRITES]`. Apply uses
//! `wadsrc/room/dehacked/` tables (things/sprites/sounds/weapons/ammo/codepointers/
//! states). Frame patches resolve via `states.txt` class/label/offset onto the
//! containing DECORATE line (compressed multi-letter lines share one apply target).

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod apply;
mod error;
mod parse;
mod tables;

pub use apply::apply_patch;
pub use error::DehackedError;
pub use parse::{
    AmmoPatch, DehackedPatch, FramePatch, MiscPatch, PointerPatch, SoundPatch, SpritePatch,
    ThingPatch, WeaponPatch, parse_dehacked,
};
pub use tables::{
    ammo_name, frame_target, sound_name, sprite_name, thing_class_name, thing_name, weapon_name,
};
