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
    /// At most one active voice with this logical sound ID.
    pub const UNIQUE_SOUND: u32 = 1 << 9;
    /// `CHANF_SINGULAR`: skip when this sound already plays anywhere.
    pub const SINGULAR: u32 = 1 << 20;
    /// `CHANF_FORCE`: starts even while sound is paused.
    pub const FORCE: u32 = 1 << 21;
    /// Duke Atomic: allow one simultaneous voice for this logical sound.
    pub const DUKE_LIMIT_ONE: u32 = 1 << 10;
    /// Duke Atomic: allow at most four simultaneous voices for this sound.
    pub const DUKE_LIMIT_FOUR: u32 = 1 << 11;
    /// Duke Atomic: at most one active voice whose sound definition has m&4.
    pub const DUKE_EXCLUSIVE: u32 = 1 << 12;
    /// Marks a Duke voice and stores its 8-bit priority in bits 16..=23.
    pub const DUKE_PRIORITY_PRESENT: u32 = 1 << 13;
    /// Mask for the encoded Duke priority value.
    pub const DUKE_PRIORITY_MASK: u32 = 0xff << 16;
    /// Marks a signed Duke `definesound.vo` distance adjustment.
    pub const DUKE_DISTANCE_OFFSET_PRESENT: u32 = 1 << 14;
    /// Marks an Atomic `MUSICANDSFX` ambience voice for the Duke ambience toggle.
    pub const DUKE_AMBIENCE: u32 = 1 << 15;
    /// Mask for the signed eight-bit mixer-space Duke distance adjustment.
    pub const DUKE_DISTANCE_OFFSET_MASK: u32 = 0xff << 24;

    /// Encode a Duke `definesound.pr` value into flags.
    #[must_use]
    pub const fn with_duke_priority(priority: i32) -> Self {
        let value = if priority < 0 {
            0
        } else if priority > 255 {
            255
        } else {
            priority as u32
        };
        Self(Self::DUKE_PRIORITY_PRESENT | (value << 16))
    }

    /// Return the encoded Duke priority, if this is a Duke voice.
    #[must_use]
    pub const fn duke_priority(self) -> Option<u8> {
        if self.0 & Self::DUKE_PRIORITY_PRESENT == 0 {
            None
        } else {
            Some(((self.0 & Self::DUKE_PRIORITY_MASK) >> 16) as u8)
        }
    }

    /// Encode a Duke distance offset in mixer world units.
    #[must_use]
    pub fn with_duke_distance_offset(offset: i32) -> Self {
        let bounded = offset.clamp(i32::from(i8::MIN), i32::from(i8::MAX));
        let signed = i8::try_from(bounded).unwrap_or_default();
        let encoded = u32::from(u8::from_ne_bytes(signed.to_ne_bytes()));
        Self(Self::DUKE_DISTANCE_OFFSET_PRESENT | (encoded << 24))
    }

    /// Decode a Duke distance offset in mixer world units.
    #[must_use]
    pub fn duke_distance_offset(self) -> Option<i8> {
        if self.0 & Self::DUKE_DISTANCE_OFFSET_PRESENT == 0 {
            return None;
        }
        let encoded =
            u8::try_from((self.0 & Self::DUKE_DISTANCE_OFFSET_MASK) >> 24).unwrap_or_default();
        Some(i8::from_ne_bytes([encoded]))
    }
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
    /// Stop every active voice using this logical sound, regardless of origin.
    StopSound {
        /// Logical sound to stop.
        sound: SoundId,
    },
    /// Stop every sound owned by one actor.
    StopActor {
        /// Stable token of the actor whose sounds should stop.
        source_id: u64,
    },
    /// Stop one actor's voices on channels `min..=max` (`all` = any channel).
    StopActorChannels {
        /// Actor token.
        source_id: u64,
        /// Lowest channel.
        min: i32,
        /// Highest channel.
        max: i32,
        /// Every channel (`A_StopSounds(0, 0)`).
        all: bool,
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
    /// Add a cents offset to every active voice of a sound, optionally filtered by actor.
    AdjustSoundPitch {
        /// Logical sound to match.
        sound: SoundId,
        /// Actor token to match, or `None` for every source.
        source_id: Option<u64>,
        /// Additional pitch in 1/100 semitone units.
        cents: i32,
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
