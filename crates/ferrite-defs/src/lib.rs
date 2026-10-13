//! Compiles text lumps into immutable Definitions registries.
//!
//! M2: GAMEINFO, MAPINFO/UMAPINFO, SNDINFO, LANGUAGE, TEXTURES, ANIMDEFS, CVARINFO, FONTDEFS.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod animdefs;
pub mod builder;
pub mod complevel;
pub mod cvarinfo;
pub mod decaldef;
pub mod error;
pub mod fontdefs;
pub mod gameinfo;
pub mod gldefs;
pub mod keyconf;
pub mod language;
pub mod loadacs;
pub mod lockdefs;
pub mod mapinfo;
pub mod menudef;
pub mod modeldef;
pub mod sbarinfo;
pub mod sndinfo;
pub mod sndseq;
pub mod terrain;
pub mod textures;
pub mod umapinfo;
pub mod voxeldef;

pub use animdefs::{AnimDefs, CameraTextureDef, parse_animdefs};
pub use builder::{Definitions, DefinitionsBuilder};
pub use complevel::{Complevel, parse_complvl};
pub use cvarinfo::{CvarDecl, CvarFlags, CvarType, parse_cvarinfo};
pub use decaldef::{
    DecalAnimParams, DecalAnimator, DecalColorParams, DecalDef, DecalGroup, DecalInfo,
    DecalSlideParams, DecalStretchParams, parse_decaldef,
};
pub use error::DefsError;
pub use fontdefs::{FontDef, parse_fontdefs};
pub use gameinfo::{GameInfo, parse_gameinfo};
pub use gldefs::{
    BrightmapDef, DynLightDef, DynLightKind, GlDefs, GlTexKind, GlowTextureDef, HardwareShaderDef,
    HardwareShaderKind, MaterialDef, ObjectLightAssoc, SkyboxDef, parse_gldefs,
};
pub use keyconf::{KeyConf, KeyConfCmd, parse_keyconf};
pub use language::{Language, parse_language};
pub use loadacs::{LoadAcs, parse_loadacs};
pub use lockdefs::{LockDef, LockDefs, LockKeyReq, parse_lockdefs};
pub use mapinfo::{ClusterDef, EpisodeDef, MapDef, MapInfo, parse_mapinfo};
pub use menudef::{MenuBlock, MenuBlockKind, MenuCmd, MenuDef, parse_menudef};
pub use modeldef::{ModelDef, ModelDefs, ModelFrameBind, parse_modeldef};
pub use sbarinfo::{MugshotDef, SbarCmd, SbarInfo, StatusBarDef, parse_sbarinfo};
pub use sndinfo::{SoundDef, SoundRegistry, parse_sndinfo};
pub use sndseq::{SeqCommand, SoundSeqRegistry, SoundSequence, parse_sndseq};
pub use terrain::{
    SplashDef, TerrainDef, TerrainGame, TerrainInfo, parse_terrain, parse_terrain_for,
};
pub use textures::{
    PatchRef, TextureDef, TextureRegistry, TextureUse, merge_texturex, parse_pnames, parse_textures,
};
pub use umapinfo::{merge_umapinfo, parse_umapinfo};
pub use voxeldef::{VoxelDef, VoxelDefs, parse_voxeldef};

/// Dump a value as readable debug text (golden/CLI until RON is wired).
pub fn dump_debug(value: &impl core::fmt::Debug) -> String {
    format!("{value:#?}")
}
