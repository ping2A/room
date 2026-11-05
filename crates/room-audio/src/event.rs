//! Sim → audio command events.

use room_core::{DVec3, SoundId};

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
    /// Do not pause this voice when gameplay pauses.
    pub const NOPAUSE: u32 = 1 << 3;
    /// Do not replace another sound on the same source channel.
    pub const OVERLAP: u32 = 1 << 4;
    /// Suppress start if the source channel already has a voice.
    pub const NOSTOP: u32 = 1 << 5;
    /// Listener-local sound (no positional attenuation).
    pub const LOCAL: u32 = 1 << 6;
    /// Listener Z participates in spatialization.
    pub const LISTENER_Z: u32 = 1 << 7;
    /// Transient sound that should not be saved/restarted.
    pub const TRANSIENT: u32 = 1 << 8;
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
        /// When set, suppress this start if the source channel currently plays this sound.
        check_sound: Option<SoundId>,
        /// Initial playback offset in seconds.
        start_time: f32,
        /// Volume 0..=1.
        volume: f32,
        /// Attenuation scale (`ATTN_NORM` = 1).
        attenuation: f32,
        /// Pitch scale (1 = nominal).
        pitch: f32,
        /// Flags.
        flags: SoundFlags,
        /// Stable source actor token used to update moving loops.
        source_id: Option<u64>,
        /// Avoid restarting an already playing source channel.
        no_restart: bool,
    },
    /// Stop a channel at an origin.
    Stop {
        /// Origin.
        origin: SoundOrigin,
        /// Channel.
        channel: i32,
    },
    /// Stop every sound owned by one actor.
    StopActor {
        /// Stable token of the actor whose sounds should stop.
        source_id: u64,
    },
    /// Stop everything.
    StopAll,
    /// Change the rate of a voice already playing at `origin` on `channel`.
    SetPitch {
        /// Origin.
        origin: SoundOrigin,
        /// Channel.
        channel: i32,
        /// Pitch scale. 1 is nominal.
        pitch: f32,
    },
    /// Change the gain of a voice already playing at `origin` on `channel`.
    SetVolume {
        /// Origin.
        origin: SoundOrigin,
        /// Channel.
        channel: i32,
        /// Volume 0..=1.
        volume: f32,
    },
}
