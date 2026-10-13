//! Core primitives for Room: names, IDs, math, deterministic `dmath`, and RNG.
//!
//! Sim, ACS, and in-sim WASM code must use this crate's math and RNG — never
//! `f64::sin` from `std`, never wall-clock time, never `HashMap` iteration.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod arena;
pub mod conv;
pub mod dmath;
pub mod error;
pub mod ids;
pub mod math;
pub mod name;
pub mod rng;

pub use arena::{Arena, Handle};
pub use error::CoreError;
pub use ids::{ClassId, LineId, LumpId, SectorId, SideId, SoundId, StateId, TextureId, VertexId};
pub use math::{Angle, Bam, DVec2, DVec3, FRACBITS, FRACUNIT, Fixed};
pub use name::{Name, NameTable};
