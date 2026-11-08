//! Compatibility export pipeline from Build / Duke content to Room PWADs.
//!
//! Native Duke gameplay lives in `room-duke`; this module is the explicit
//! boundary for callers that need conversion for editors, interchange, or
//! legacy compatibility.

pub use crate::convert::{
    ConvertedMap, GameTiles, MapThing, MoverSound, NoSprites, SpriteMapper, WallBreak, convert_map,
    convert_map_with, doom_map_name,
};
pub use crate::wad::{convert_grp_with_maps, grp_to_wad};
