//! Build engine world model and clean-room primitives.
//!
//! No Duke game logic lives here — see `room-duke`. Specs for primitives:
//! `docs/build-engine-primitives/` (D2). Architecture: `docs/25-duke3d-full-support.md` §5.
//!
//! **Licensing:** do not translate Ken Silverman / BUILDLIC engine sources.
//! Implement from behavioural specs only (`docs/decisions/0004-native-build-runtime.md`).

#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![allow(missing_docs)] // public surface expands through D2–D3; document as APIs stabilize

pub mod fly;
pub mod geom;
pub mod helpers;
pub mod load;
pub mod numeric;
pub mod soft;
pub mod types;
pub mod view;
pub mod world;

pub use fly::{FreeFly, FreeFlyCmd};
pub use geom::{
    HIT_WALL_BOOTSTRAP, HitScan, SectorWalls, SectorZ, SpriteHit, SpriteZClip, WallXY, ZRange,
    cansee, clipinsidebox, clipmove_xy, clipmove_xy_at_z, getzrange, getzrange_with_sprites,
    getzsofslope, hitscan, hitscan_full, hitscan_with_sprites, inside, neartag_wall, pushmove,
    updatesector,
};
pub use helpers::{
    approx_sincos_q14, dragpoint, getangle, install_radarang, install_sintable, lastwall, length2d,
    radarang_from_tables_dat, rotatepoint, sectorofwall, sincos_q14, sintable_from_tables_dat,
};
pub use load::LoadError;
pub use numeric::{
    divscale, dmulscale, klabs, krand_step, ksqrt, msqrtasm, mulscale, nsqrtasm, scale,
};
pub use soft::{
    SoftFrame, SoftTiles, fill_fp_flats_sky, render_fp_solid, render_fp_sprites,
    render_fp_textured, render_fp_wire, render_topdown,
};
pub use types::{SectorT, SpriteT, WallT};
pub use view::BuildView;
pub use world::{BuildWorld, MAXSECTORS, MAXSPRITES, MAXSTATUS, MAXWALLS, SpriteLists};
