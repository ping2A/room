//! Mixer, SFX, positional audio, reverb, and output backends.
//!
//! Device I/O is injected via [`AudioOutput`]; the mixer never assumes a real device.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod event;
mod mixer;

pub use event::{SoundEvent, SoundFlags, SoundOrigin};
pub use mixer::{AudioError, ENGINE_SAMPLE_RATE, Mixer, SampleBank, Voice, resample_mono};
