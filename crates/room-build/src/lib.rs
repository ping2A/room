//! Build engine content (Duke Nukem 3D) for Room.
//!
//! Reads the original data — `.GRP` archives, `TILESnnn.ART` tile sets,
//! `PALETTE.DAT`, version 7 `.MAP` files and the level table in `USER.CON` —
//! and converts it into an in-memory Room PWAD: UDMF maps (`E1L1` → `E1M1`),
//! tiles as `TX_START` pictures named `BTnnnn`, a `PLAYPAL` / `COLORMAP` built
//! from the Build palette and shade tables, a `MAPINFO` (names, music, sky)
//! and the episode MIDI tracks. Every Room shell mounts that WAD in place of
//! a Doom IWAD, so the existing playsim, renderer, netplay and editor run Build
//! maps unchanged. See `docs/23-build-engine.md` for the coverage and plan.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod acs;
mod anm;
pub mod art;
pub mod con;
pub mod condata;
pub mod convert;
mod dmo;
pub mod duke;
pub mod error;
pub mod export;
mod fx;
pub mod grp;
pub mod lookup;
pub mod map;
pub mod mount;
pub mod palette;
mod polys;
pub mod rts;
mod sav;
pub mod sprites;
pub mod wad;

pub use art::{Tile, TileSet};
pub use con::{LevelInfo, parse_user_con};
pub use condata::{
    ConData, ConEvent, ConEvtOp, ConOp, MOVE_DODGEBULLET, MOVE_FACEPLAYER, MOVE_FACEPLAYERSLOW,
    MOVE_FACEPLAYERSMART, MOVE_FLEEENEMY, MOVE_FURTHESTDIR, MOVE_GETH, MOVE_GETV,
    MOVE_JUMPTOPLAYER, MOVE_RANDOMANGLE, MOVE_SEEKPLAYER, MOVE_SPIN, MoveDef, parse_conevt,
    run_conevt,
};
pub use convert::{
    ConvertedMap, GameTiles, MapThing, MoverSound, NoSprites, SpriteMapper, WallBreak, convert_map,
    convert_map_with, doom_map_name,
};
pub use dmo::{DmoDemo, DmoInput};
pub use duke::{Duke, DukeContent};
pub use error::BuildError;
pub use grp::{Grp, GrpStack, is_grp};
pub use lookup::Lookups;
pub use map::{BuildMap, Sector, Sprite, Wall, is_build_map, parse_map};
pub use palette::Palette;
pub use rts::DukeRts;
pub use sav::{
    LoadedSave, SAV_MAXPLAYERS, SavHeader, SavPlayer, SavPlayerTailOffsets, SavSector, SavSprite,
    SavWall, atomic_player_record_size, atomic_player_tail_offsets, identify_save,
};
pub use sprites::{Origin, SpriteLump, SpriteSet};
pub use wad::{convert_grp_with_maps, grp_to_wad};
