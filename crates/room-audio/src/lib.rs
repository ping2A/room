//! Mixer, SFX, positional audio, reverb, and output backends.
//!
//! Device I/O is injected via [`AudioOutput`]; the mixer never assumes a real device.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod event;
mod mixer;
pub mod efx;
pub mod reverb;

pub use event::{SoundEvent, SoundFlags, SoundOrigin};
pub use efx::{EfxParams, EfxReverb};
pub use reverb::{EnvironmentTable, ReverbEnv, ReverbProps};
pub use mixer::{AudioError, ENGINE_SAMPLE_RATE, Mixer, SampleBank, UNDERWATER_PITCH, Voice, resample_mono};
