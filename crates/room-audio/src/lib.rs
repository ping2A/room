//! Mixer, SFX, positional audio, reverb, and output backends.
//!
//! Device I/O is injected via [`AudioOutput`]; the mixer never assumes a real device.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod efx;
mod event;
mod mixer;
pub mod reverb;

pub use efx::{EfxParams, EfxReverb};
pub use event::{SoundEvent, SoundFlags, SoundOrigin};
pub use mixer::{
    AudioError, ENGINE_SAMPLE_RATE, Mixer, SampleBank, UNDERWATER_PITCH, Voice, resample_mono,
};
pub use reverb::{EnvironmentTable, ReverbEnv, ReverbProps};
