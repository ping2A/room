//! Software mixer (M5 bootstrap: mono samples, linear attenuation).

use crate::event::{SoundEvent, SoundFlags, SoundOrigin};
use ferrite_core::{DVec3, SoundId};
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;

/// Default mixer / SoftFont rate (Hz). Doom DMX lumps are often 11025 Hz and
/// must be resampled to this (or the device rate) before playback.
pub const ENGINE_SAMPLE_RATE: u32 = 44_100;

/// Doom full-volume distance.
const CLOSE_DIST: f64 = 160.0;
/// Doom silence distance.
const CLIP_DIST: f64 = 1200.0;

/// Linear-resample mono PCM from `src_rate` to `dst_rate`.
#[must_use]
pub fn resample_mono(samples: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    let src_rate = src_rate.max(1);
    let dst_rate = dst_rate.max(1);
    if samples.is_empty() || src_rate == dst_rate {
        return samples.to_vec();
    }
    let out_len = ((samples.len() as u64) * u64::from(dst_rate) / u64::from(src_rate)).max(1);
    #[allow(clippy::cast_possible_truncation)]
    let out_len = out_len as usize;
    let mut out = Vec::with_capacity(out_len);
    let scale = f64::from(src_rate) / f64::from(dst_rate);
    for i in 0..out_len {
        let src_pos = f64::from(i as u32) * scale;
        let i0 = src_pos.floor() as usize;
        let i1 = (i0 + 1).min(samples.len().saturating_sub(1));
        let frac = (src_pos - src_pos.floor()) as f32;
        let a = samples.get(i0).copied().unwrap_or(0.0);
        let b = samples.get(i1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

/// Mixer errors.
#[derive(Debug, Error)]
pub enum AudioError {
    /// Missing sample.
    #[error("unknown sound id {0:?}")]
    UnknownSound(SoundId),
}

/// Decoded PCM bank (mono f32 at the mixer output rate).
#[derive(Clone, Debug, Default)]
pub struct SampleBank {
    samples: BTreeMap<u32, Arc<[f32]>>,
}

impl SampleBank {
    /// Empty bank.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert mono PCM for `id` (already at mixer rate).
    pub fn insert(&mut self, id: SoundId, pcm: Arc<[f32]>) {
        self.samples.insert(id.raw(), pcm);
    }

    /// Insert mono PCM resampled from `src_rate` to `dst_rate`.
    pub fn insert_resampled(&mut self, id: SoundId, samples: &[f32], src_rate: u32, dst_rate: u32) {
        let pcm: Arc<[f32]> = resample_mono(samples, src_rate, dst_rate).into();
        self.insert(id, pcm);
    }

    /// Lookup.
    #[must_use]
    pub fn get(&self, id: SoundId) -> Option<&Arc<[f32]>> {
        self.samples.get(&id.raw())
    }

    /// Copy any samples from `other` that are missing locally.
    pub fn merge_missing(&mut self, other: &SampleBank) {
        for (k, v) in &other.samples {
            self.samples.entry(*k).or_insert_with(|| Arc::clone(v));
        }
    }
}

/// One playing voice.
#[derive(Clone, Debug)]
pub struct Voice {
    /// Sample id.
    pub sound: SoundId,
    /// Cursor in frames.
    pub cursor: usize,
    /// Gain.
    pub volume: f32,
    /// Pitch (resampling step; 1.0 = 1 sample/frame).
    pub pitch: f32,
    /// Looping.
    pub loop_: bool,
    /// Origin for attenuation.
    pub origin: SoundOrigin,
    /// Channel.
    pub channel: i32,
    /// Attenuation scale.
    pub attenuation: f32,
}

/// Soft mixer: apply events, render interleaved stereo f32.
#[derive(Clone, Debug)]
pub struct Mixer {
    /// Sample data.
    pub bank: SampleBank,
    /// Active voices.
    pub voices: Vec<Voice>,
    /// Listener position.
    pub listener: DVec3,
    /// Master SFX gain.
    pub master: f32,
    /// Master music gain (applied by [`Self::mix_music`]).
    pub music_gain: f32,
    /// Output sample rate (Hz) — bank PCM and music must match.
    pub sample_rate: u32,
}

impl Default for Mixer {
    fn default() -> Self {
        Self {
            bank: SampleBank::default(),
            voices: Vec::new(),
            listener: DVec3::ZERO,
            master: 1.0,
            music_gain: 1.0,
            sample_rate: ENGINE_SAMPLE_RATE,
        }
    }
}

impl Mixer {
    /// New mixer with unity master at [`ENGINE_SAMPLE_RATE`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// New mixer targeting a device output rate.
    #[must_use]
    pub fn with_sample_rate(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1),
            ..Self::default()
        }
    }

    /// Apply a batch of sound events.
    pub fn apply(&mut self, events: &[SoundEvent]) {
        for ev in events {
            match *ev {
                SoundEvent::Start {
                    origin,
                    channel,
                    sound,
                    volume,
                    attenuation,
                    pitch,
                    flags,
                } => {
                    // Replace same origin+channel unless overlapping (not in M5).
                    self.voices
                        .retain(|v| !(v.channel == channel && origins_eq(v.origin, origin)));
                    self.voices.push(Voice {
                        sound,
                        cursor: 0,
                        volume,
                        pitch: pitch.max(0.01),
                        loop_: flags.0 & SoundFlags::LOOP != 0,
                        origin,
                        channel,
                        attenuation,
                    });
                }
                SoundEvent::Stop { origin, channel } => {
                    self.voices
                        .retain(|v| !(v.channel == channel && origins_eq(v.origin, origin)));
                }
                SoundEvent::StopAll => self.voices.clear(),
            }
        }
    }

    /// Render `frames` of stereo interleaved output into `out` (len = frames*2).
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let frames = out.len() / 2;
        let mut finished = Vec::new();
        for (vi, voice) in self.voices.iter_mut().enumerate() {
            let Some(pcm) = self.bank.get(voice.sound) else {
                finished.push(vi);
                continue;
            };
            if pcm.is_empty() {
                finished.push(vi);
                continue;
            }
            let atten = attenuate(voice.origin, self.listener, voice.attenuation);
            let gain = voice.volume * atten * self.master;
            let mut pos = voice.cursor as f32;
            for f in 0..frames {
                let idx = pos as usize;
                if idx >= pcm.len() {
                    if voice.loop_ {
                        pos = 0.0;
                        continue;
                    }
                    finished.push(vi);
                    break;
                }
                let s = pcm[idx] * gain;
                out[f * 2] += s;
                out[f * 2 + 1] += s;
                pos += voice.pitch;
            }
            voice.cursor = pos as usize;
            if !voice.loop_ && voice.cursor >= pcm.len() {
                finished.push(vi);
            }
        }
        // Remove finished (high indices first).
        finished.sort_unstable();
        finished.dedup();
        for vi in finished.into_iter().rev() {
            if vi < self.voices.len() {
                self.voices.swap_remove(vi);
            }
        }
    }

    /// Add interleaved stereo music PCM into `out` (same length), scaled by [`Self::music_gain`].
    ///
    /// The shell typically fills `music` via `ferrite_music::MusicPlayer::render_interleaved`.
    pub fn mix_music(&self, out: &mut [f32], music: &[f32]) {
        let gain = self.music_gain.clamp(0.0, 1.0);
        if gain <= 0.0 {
            return;
        }
        let n = out.len().min(music.len());
        for i in 0..n {
            out[i] += music[i] * gain;
        }
    }

    /// Render SFX then mix a music buffer in one step.
    pub fn render_with_music(&mut self, out: &mut [f32], music: &[f32]) {
        self.render(out);
        self.mix_music(out, music);
    }
}

fn origins_eq(a: SoundOrigin, b: SoundOrigin) -> bool {
    match (a, b) {
        (SoundOrigin::None, SoundOrigin::None) => true,
        (SoundOrigin::Point(p), SoundOrigin::Point(q)) => {
            (p.x() - q.x()).abs() < 1e-6
                && (p.y() - q.y()).abs() < 1e-6
                && (p.z() - q.z()).abs() < 1e-6
        }
        _ => false,
    }
}

fn attenuate(origin: SoundOrigin, listener: DVec3, scale: f32) -> f32 {
    if scale <= 0.0 {
        return 1.0;
    }
    let SoundOrigin::Point(p) = origin else {
        return 1.0;
    };
    let dx = p.x() - listener.x();
    let dy = p.y() - listener.y();
    let dz = p.z() - listener.z();
    let dist = ferrite_core::dmath::sqrt(dx * dx + dy * dy + dz * dz) * f64::from(scale);
    if dist <= CLOSE_DIST {
        return 1.0;
    }
    if dist >= CLIP_DIST {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)]
    {
        ((CLIP_DIST - dist) / (CLIP_DIST - CLOSE_DIST)) as f32
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn resample_mono_upsamples_length() {
        let src = vec![0.0f32, 1.0];
        let out = resample_mono(&src, 11025, 44100);
        assert!(out.len() >= 7 && out.len() <= 9, "len={}", out.len());
        assert!((out[0] - 0.0).abs() < 1e-4);
        assert!((out[out.len() - 1] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn plays_and_finishes_oneshot() {
        let mut mix = Mixer::new();
        let pcm: Arc<[f32]> = Arc::from(vec![0.5f32; 8]);
        mix.bank.insert(SoundId::from_raw(1), pcm);
        mix.apply(&[SoundEvent::Start {
            origin: SoundOrigin::None,
            channel: 1,
            sound: SoundId::from_raw(1),
            volume: 1.0,
            attenuation: 0.0,
            pitch: 1.0,
            flags: SoundFlags::default(),
        }]);
        let mut out = vec![0.0f32; 32]; // 16 frames
        mix.render(&mut out);
        assert!(out.iter().any(|s| *s > 0.0));
        // Second render exhausts sample.
        mix.render(&mut out);
        assert!(mix.voices.is_empty());
    }

    #[test]
    fn mix_music_adds_gain() {
        let mix = Mixer::new();
        let mut out = vec![0.5f32; 4];
        let music = vec![0.5f32; 4];
        mix.mix_music(&mut out, &music);
        assert!((out[0] - 1.0).abs() < 1e-5);
    }
}
