//! Deterministic sim math types. Not `glam` — SIMD/`mul_add` paths vary by target.

mod angle;
mod finesine;
mod fixed;
mod tantoangle;
mod vec;

pub use angle::{Angle, BAM_PER_TURN, Bam, DEGREES_PER_TURN};
pub use finesine::{
    ANGLETOFINESHIFT, FINEANGLES, FINESINE, FINESINE_LEN, bam_to_fine, finecosine, finesine,
};
pub use fixed::{FRACBITS, FRACUNIT, Fixed};
pub use tantoangle::{
    SLOPERANGE, TANTOANGLE, aprox_distance, point_to_angle, point_to_angle2, slope_div,
};
pub use vec::{DVec2, DVec3};
