//! Sim → audio command events.

use ferrite_core::{DVec3, SoundId};

/// Where a sound is attached.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoundOrigin {
    /// 2D / UI / listener-local.
    None,
    /// World point.
    Point(DVec3),
}

/// Channel flags (`CHANF_*` subset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoundFlags(pub u32);

impl SoundFlags {
    /// Loop until stopped.
    pub const LOOP: u32 = 1 << 0;
    /// UI / no pause with game.
    pub const UI: u32 = 1 << 1;
    /// No attenuation.
    pub const ATTN_NONE: u32 = 1 << 2;
}

/// One audio command from the game shell (drained from sim events).
#[derive(Clone, Debug, PartialEq)]
pub enum SoundEvent {
    /// Start a sound.
    Start {
        /// Origin.
        origin: SoundOrigin,
        /// Channel index.
        channel: i32,
        /// Logical sound.
        sound: SoundId,
        /// Volume 0..=1.
        volume: f32,
        /// Attenuation scale (`ATTN_NORM` = 1).
        attenuation: f32,
        /// Pitch scale (1 = nominal).
        pitch: f32,
        /// Flags.
        flags: SoundFlags,
    },
    /// Stop a channel at an origin.
    Stop {
        /// Origin.
        origin: SoundOrigin,
        /// Channel.
        channel: i32,
    },
    /// Stop everything.
    StopAll,
}
