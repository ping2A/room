//! Software mixer (M5 bootstrap: mono samples, linear attenuation).

use crate::event::{SoundEvent, SoundFlags, SoundOrigin};
use room_core::{DVec3, SoundId};
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
/// GZDoom default `$limit`: copies of one sound allowed close together.
const NEAR_LIMIT: usize = 2;
/// GZDoom `LimitRange` default (256 map units).
const NEAR_LIMIT_RANGE: f64 = 256.0;
/// `CHAN_WEAPON` sounds belong to the listener's own weapon and must not be
/// discarded by the positional duplicate limiter.
const CHAN_WEAPON: i32 = 1;
/// Doom `S_STEREO_SWING` (separation units out of 256).
const STEREO_SWING: f64 = 96.0;
/// Sources closer than this to the listener play centred (own weapon, pain, …).
const PAN_MIN_DIST: f64 = 1.0;

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
    /// Source actor token, when actor-bound.
    pub source_id: Option<u64>,
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
    /// Listener facing as a unit XY vector `(cos, sin)`; `None` = mono (no panning).
    pub listener_facing: Option<(f64, f64)>,
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
            listener_facing: None,
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

    /// Place the listener at `pos` facing `angle` (radians, Doom convention:
    /// counter-clockwise from +X). Enables stereo panning.
    pub fn set_listener(&mut self, pos: DVec3, angle: f64) {
        self.listener = pos;
        self.listener_facing = Some((room_core::dmath::cos(angle), room_core::dmath::sin(angle)));
    }

    /// Apply a batch of sound events.
    pub fn apply(&mut self, events: &[SoundEvent]) {
        for ev in events {
            match *ev {
                SoundEvent::Start {
                    origin,
                    channel,
                    sound,
                    check_sound,
                    start_time,
                    volume,
                    attenuation,
                    pitch,
                    flags,
                    source_id,
                    no_restart,
                } => {
                    if check_sound.is_some_and(|check| {
                        self.voices.iter().any(|voice| {
                            voice.channel == channel
                                && source_id.map_or_else(
                                    || origins_eq(voice.origin, origin),
                                    |id| voice.source_id == Some(id),
                                )
                                && voice.sound == check
                        })
                    }) {
                        continue;
                    }
                    let overlap = flags.0 & SoundFlags::OVERLAP != 0;
                    let nostop = flags.0 & SoundFlags::NOSTOP != 0;
                    if nostop
                        && self.voices.iter().any(|v| {
                            v.channel == channel
                                && source_id.map_or_else(
                                    || origins_eq(v.origin, origin),
                                    |id| v.source_id == Some(id),
                                )
                        })
                    {
                        continue;
                    }
                    if let Some(source_id) = source_id {
                        let existing = self
                            .voices
                            .iter()
                            .position(|v| v.source_id == Some(source_id) && v.channel == channel);
                        if let Some(index) = existing {
                            if no_restart {
                                if self.voices[index].sound == sound {
                                    let voice = &mut self.voices[index];
                                    voice.origin = origin;
                                    voice.volume = volume;
                                    voice.pitch = pitch.max(0.01);
                                    voice.attenuation = attenuation;
                                    voice.loop_ |= flags.0 & SoundFlags::LOOP != 0;
                                }
                                continue;
                            }
                            if !overlap {
                                self.voices.remove(index);
                            }
                        }
                    } else if !overlap {
                        // Replace same origin+channel unless overlapping.
                        self.voices
                            .retain(|v| !(v.channel == channel && origins_eq(v.origin, origin)));
                    }
                    // GZDoom `S_CheckSoundLimit`: a sound plays at most
                    // `NEAR_LIMIT` times within `NEAR_LIMIT_RANGE` units of each
                    // other (SNDINFO `$limit`, default 2). Without it a room of
                    // Brutal Doom soldiers stacked every footstep and distant
                    // gunshot into one loud wall of sound from far away.
                    if let SoundOrigin::Point(p) = origin {
                        let dx = p.x() - self.listener.x();
                        let dy = p.y() - self.listener.y();
                        let dz = p.z() - self.listener.z();
                        let listener_local = dx * dx + dy * dy + dz * dz <= CLOSE_DIST * CLOSE_DIST;
                        if channel != CHAN_WEAPON && !listener_local {
                            let near = self
                                .voices
                                .iter()
                                .filter(|v| {
                                    v.sound == sound
                                        && matches!(v.origin, SoundOrigin::Point(q)
                                        if (q.x() - p.x()).powi(2) + (q.y() - p.y()).powi(2)
                                            + (q.z() - p.z()).powi(2)
                                            < NEAR_LIMIT_RANGE * NEAR_LIMIT_RANGE)
                                })
                                .count();
                            if near >= NEAR_LIMIT {
                                continue;
                            }
                        }
                    }
                    self.voices.push(Voice {
                        sound,
                        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                        cursor: (start_time.max(0.0) * self.sample_rate as f32) as usize,
                        volume,
                        pitch: pitch.max(0.01),
                        loop_: flags.0 & SoundFlags::LOOP != 0,
                        origin,
                        channel,
                        source_id,
                        attenuation,
                    });
                }
                SoundEvent::Stop { origin, channel } => {
                    self.voices
                        .retain(|v| !(v.channel == channel && origins_eq(v.origin, origin)));
                }
                SoundEvent::StopActor { source_id } => {
                    self.voices.retain(|v| v.source_id != Some(source_id));
                }
                SoundEvent::StopAll => self.voices.clear(),
                SoundEvent::SetPitch {
                    origin,
                    channel,
                    pitch,
                } => {
                    let pitch = pitch.max(0.01);
                    for voice in &mut self.voices {
                        if voice.channel == channel && origins_eq(voice.origin, origin) {
                            voice.pitch = pitch;
                        }
                    }
                }
                SoundEvent::SetVolume {
                    origin,
                    channel,
                    volume,
                } => {
                    for voice in &mut self.voices {
                        if voice.channel == channel && origins_eq(voice.origin, origin) {
                            voice.volume = volume;
                        }
                    }
                }
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
            let (pan_l, pan_r) = stereo_gains(
                voice.origin,
                voice.attenuation,
                self.listener,
                self.listener_facing,
            );
            let (gain_l, gain_r) = (gain * pan_l, gain * pan_r);
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
                let s = pcm[idx];
                out[f * 2] += s * gain_l;
                out[f * 2 + 1] += s * gain_r;
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
    /// The shell typically fills `music` via `room_music::MusicPlayer::render_interleaved`.
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
    let dist = room_core::dmath::sqrt(dx * dx + dy * dy + dz * dz) * f64::from(scale);
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

/// Left/right gains for a voice heard by a listener facing `facing`.
///
/// Doom separation curve (`sep = 128 ∓ swing·sin θ`, gain `1 − (sep/256)²`),
/// rescaled so a centred source keeps unity gain.
fn stereo_gains(
    origin: SoundOrigin,
    scale: f32,
    listener: DVec3,
    facing: Option<(f64, f64)>,
) -> (f32, f32) {
    let (Some((fx, fy)), SoundOrigin::Point(p)) = (facing, origin) else {
        return (1.0, 1.0);
    };
    if scale <= 0.0 {
        return (1.0, 1.0);
    }
    let dx = p.x() - listener.x();
    let dy = p.y() - listener.y();
    let dist = room_core::dmath::sqrt(dx * dx + dy * dy);
    if dist < PAN_MIN_DIST {
        return (1.0, 1.0);
    }
    // sin of the angle from facing to the source; > 0 = source on the left.
    let side = (fx * dy - fy * dx) / dist;
    let sep = 128.0 - STEREO_SWING * side.clamp(-1.0, 1.0);
    let curve = |s: f64| (1.0 - (s / 256.0) * (s / 256.0)) / 0.75;
    #[allow(clippy::cast_possible_truncation)]
    {
        (
            curve(sep).min(1.0) as f32,
            curve(256.0 - sep).min(1.0) as f32,
        )
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
    fn stop_all_clears_every_voice() {
        let mut mix = Mixer::new();
        let pcm: Arc<[f32]> = Arc::from(vec![0.5f32; 32]);
        mix.bank.insert(SoundId::from_raw(1), pcm);
        mix.apply(&[
            SoundEvent::Start {
                origin: SoundOrigin::None,
                channel: 1,
                sound: SoundId::from_raw(1),
                check_sound: None,
                start_time: 0.0,
                source_id: None,
                no_restart: false,
                volume: 1.0,
                attenuation: 0.0,
                pitch: 1.0,
                flags: SoundFlags::default(),
            },
            SoundEvent::Start {
                origin: SoundOrigin::None,
                channel: 2,
                sound: SoundId::from_raw(1),
                check_sound: None,
                start_time: 0.0,
                source_id: None,
                no_restart: false,
                volume: 1.0,
                attenuation: 0.0,
                pitch: 1.0,
                flags: SoundFlags::default(),
            },
        ]);
        assert_eq!(mix.voices.len(), 2);
        mix.apply(&[SoundEvent::StopAll]);
        assert!(mix.voices.is_empty());
    }

    #[test]
    fn set_pitch_changes_one_channel() {
        let mut mix = Mixer::new();
        let pcm: Arc<[f32]> = Arc::from(vec![0.5f32; 32]);
        mix.bank.insert(SoundId::from_raw(1), pcm);
        mix.apply(&[
            SoundEvent::Start {
                origin: SoundOrigin::None,
                channel: 1,
                sound: SoundId::from_raw(1),
                check_sound: None,
                start_time: 0.0,
                source_id: None,
                no_restart: false,
                volume: 1.0,
                attenuation: 0.0,
                pitch: 1.0,
                flags: SoundFlags::default(),
            },
            SoundEvent::Start {
                origin: SoundOrigin::None,
                channel: 2,
                sound: SoundId::from_raw(1),
                check_sound: None,
                start_time: 0.0,
                source_id: None,
                no_restart: false,
                volume: 1.0,
                attenuation: 0.0,
                pitch: 1.0,
                flags: SoundFlags::default(),
            },
            SoundEvent::SetPitch {
                origin: SoundOrigin::None,
                channel: 1,
                pitch: 2.0,
            },
        ]);
        let v1 = mix.voices.iter().find(|v| v.channel == 1).unwrap();
        let v2 = mix.voices.iter().find(|v| v.channel == 2).unwrap();
        assert!((v1.pitch - 2.0).abs() < 1e-6);
        assert!((v2.pitch - 1.0).abs() < 1e-6);
    }

    #[test]
    fn set_volume_changes_one_channel() {
        let mut mix = Mixer::new();
        let pcm: Arc<[f32]> = Arc::from(vec![0.5f32; 32]);
        mix.bank.insert(SoundId::from_raw(1), pcm);
        mix.apply(&[
            SoundEvent::Start {
                origin: SoundOrigin::None,
                channel: 1,
                sound: SoundId::from_raw(1),
                check_sound: None,
                start_time: 0.0,
                source_id: None,
                no_restart: false,
                volume: 1.0,
                attenuation: 0.0,
                pitch: 1.0,
                flags: SoundFlags::default(),
            },
            SoundEvent::Start {
                origin: SoundOrigin::None,
                channel: 2,
                sound: SoundId::from_raw(1),
                check_sound: None,
                start_time: 0.0,
                source_id: None,
                no_restart: false,
                volume: 1.0,
                attenuation: 0.0,
                pitch: 1.0,
                flags: SoundFlags::default(),
            },
            SoundEvent::SetVolume {
                origin: SoundOrigin::None,
                channel: 1,
                volume: 0.25,
            },
        ]);
        let v1 = mix.voices.iter().find(|v| v.channel == 1).unwrap();
        let v2 = mix.voices.iter().find(|v| v.channel == 2).unwrap();
        assert!((v1.volume - 0.25).abs() < 1e-6);
        assert!((v2.volume - 1.0).abs() < 1e-6);
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
            check_sound: None,
            start_time: 0.0,
            source_id: None,
            no_restart: false,
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

    fn render_point(mix: &mut Mixer, at: DVec3) -> (f32, f32) {
        mix.bank
            .insert(SoundId::from_raw(1), Arc::from(vec![0.5f32; 64]));
        mix.voices.clear();
        mix.apply(&[SoundEvent::Start {
            origin: SoundOrigin::Point(at),
            channel: 1,
            sound: SoundId::from_raw(1),
            check_sound: None,
            start_time: 0.0,
            source_id: None,
            no_restart: false,
            volume: 1.0,
            attenuation: 1.0,
            pitch: 1.0,
            flags: SoundFlags::default(),
        }]);
        let mut out = vec![0.0f32; 8];
        mix.render(&mut out);
        (out[0], out[1])
    }

    #[test]
    fn pans_relative_to_listener_facing() {
        let mut mix = Mixer::new();
        // Facing +X (east): +Y is to the left.
        mix.set_listener(DVec3::ZERO, 0.0);
        let (l, r) = render_point(&mut mix, DVec3::new(0.0, 100.0, 0.0));
        assert!(l > r * 2.0, "left source: l={l} r={r}");
        // Same source after turning to face +Y: dead ahead → centred.
        mix.set_listener(DVec3::ZERO, std::f64::consts::FRAC_PI_2);
        let (l, r) = render_point(&mut mix, DVec3::new(0.0, 100.0, 0.0));
        assert!((l - r).abs() < 1e-4, "ahead: l={l} r={r}");
        // Facing -Y: source is behind-ish → still centred; at +X it is on the left.
        mix.set_listener(DVec3::ZERO, -std::f64::consts::FRAC_PI_2);
        let (l, r) = render_point(&mut mix, DVec3::new(100.0, 0.0, 0.0));
        assert!(l > r * 2.0, "facing south, east is left: l={l} r={r}");
    }

    #[test]
    fn listener_own_sounds_stay_centred() {
        let mut mix = Mixer::new();
        mix.set_listener(DVec3::new(10.0, 20.0, 0.0), 1.0);
        let (l, r) = render_point(&mut mix, DVec3::new(10.0, 20.0, 0.0));
        assert!(
            (l - 0.5).abs() < 1e-4 && (r - 0.5).abs() < 1e-4,
            "l={l} r={r}"
        );
    }

    #[test]
    fn no_facing_is_mono() {
        let mut mix = Mixer::new();
        let (l, r) = render_point(&mut mix, DVec3::new(0.0, 100.0, 0.0));
        assert!((l - r).abs() < 1e-6);
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
