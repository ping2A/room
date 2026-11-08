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
/// Chocolate Duke's default Sound Blaster voice pool (`fx_man.c`).
const DUKE_VOICE_POOL: usize = 32;

const RVB_FB_SRC_A: usize = 0;
const RVB_FB_SRC_B: usize = 1;
const RVB_IIR_DEST_A0: usize = 2;
const RVB_IIR_DEST_A1: usize = 3;
const RVB_ACC_SRC_A0: usize = 4;
const RVB_ACC_SRC_A1: usize = 5;
const RVB_ACC_SRC_B0: usize = 6;
const RVB_ACC_SRC_B1: usize = 7;
const RVB_IIR_SRC_A0: usize = 8;
const RVB_IIR_SRC_A1: usize = 9;
const RVB_IIR_DEST_B0: usize = 10;
const RVB_IIR_DEST_B1: usize = 11;
const RVB_ACC_SRC_C0: usize = 12;
const RVB_ACC_SRC_C1: usize = 13;
const RVB_ACC_SRC_D0: usize = 14;
const RVB_ACC_SRC_D1: usize = 15;
const RVB_IIR_SRC_B1: usize = 16;
const RVB_IIR_SRC_B0: usize = 17;
const RVB_MIX_DEST_A0: usize = 18;
const RVB_MIX_DEST_A1: usize = 19;
const RVB_MIX_DEST_B0: usize = 20;
const RVB_MIX_DEST_B1: usize = 21;

/// Rust port of Chocolate's audiolib `mvreverb.c` stereo IIR/feedback network.
#[derive(Clone, Debug)]
struct DukeReverb {
    /// Interleaved internal delay samples; the algorithm addresses byte offsets
    /// in its original 16-bit stereo buffer, hence the four-sample tap stride.
    buffer: Vec<f64>,
    cursor: usize,
    offsets: [i32; 22],
}

impl DukeReverb {
    fn new(sample_rate: u32) -> Self {
        const BASE_OFFSETS: [i32; 22] = [
            0xE3, 0xA9, 0xDFB, 0xB58, 0xD09, 0xA3C, 0xBD9, 0x973, 0xB59, 0x8DA, 0x8D9, 0x5E9,
            0x7EC, 0x4B0, 0x6EF, 0x3D2, 0x5EA, 0x31D, 0x31C, 0x238, 0x154, 0xAA,
        ];
        let rate = i64::from(sample_rate.max(1));
        let offsets = BASE_OFFSETS
            .map(|offset| i32::try_from(i64::from(offset) * rate / 22_050).unwrap_or(i32::MAX));
        let delay = usize::try_from(14_320_i64 * rate / 22_050)
            .unwrap_or(1)
            .max(1);
        Self {
            buffer: vec![0.0; delay],
            cursor: 0,
            offsets,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.cursor = 0;
    }

    fn index(&self, offset: i32, advance: usize) -> usize {
        let length = self.buffer.len() as i64;
        let byte_offset = i64::from(offset) * 4 + self.cursor as i64 + advance as i64;
        usize::try_from(byte_offset.rem_euclid(length)).unwrap_or_default()
    }

    fn read(&self, offset: usize) -> f64 {
        self.buffer[self.index(self.offsets[offset], 0)]
    }

    fn write(&mut self, offset: usize, value: f64, advance: usize) {
        let index = self.index(self.offsets[offset], advance);
        self.buffer[index] = value;
    }

    fn process_frame(&mut self, input_l: f64, input_r: f64) -> (f64, f64) {
        const IIR_ALPHA: f64 = 0.870_117_187_5;
        const ACC_COEF_A: f64 = 0.622_314_453_125;
        const ACC_COEF_B: f64 = -0.524_414_062_5;
        const ACC_COEF_C: f64 = 0.539_550_781_25;
        const ACC_COEF_D: f64 = -0.508_300_781_25;
        const IIR_COEF: f64 = -0.699_218_75;
        const FB_ALPHA: f64 = 0.675_781_25;
        const FB_X: f64 = 0.646_484_375;
        const IN_COEF_L: f64 = -2.0;
        const IN_COEF_R: f64 = -2.0;

        let iir_input_a0 = self.read(RVB_IIR_SRC_A0) * IIR_COEF + input_l * IN_COEF_L;
        let iir_input_a1 = self.read(RVB_IIR_SRC_A1) * IIR_COEF + input_r * IN_COEF_R;
        let iir_input_b0 = self.read(RVB_IIR_SRC_B0) * IIR_COEF + input_l * IN_COEF_L;
        let iir_input_b1 = self.read(RVB_IIR_SRC_B1) * IIR_COEF + input_r * IN_COEF_R;

        let iir_a0 = iir_input_a0 * IIR_ALPHA + self.read(RVB_IIR_DEST_A0) * (1.0 - IIR_ALPHA);
        let iir_a1 = iir_input_a1 * IIR_ALPHA + self.read(RVB_IIR_DEST_A1) * (1.0 - IIR_ALPHA);
        let iir_b0 = iir_input_b0 * IIR_ALPHA + self.read(RVB_IIR_DEST_B0) * (1.0 - IIR_ALPHA);
        let iir_b1 = iir_input_b1 * IIR_ALPHA + self.read(RVB_IIR_DEST_B1) * (1.0 - IIR_ALPHA);
        self.write(RVB_IIR_DEST_A0, iir_a0, 1);
        self.write(RVB_IIR_DEST_A1, iir_a1, 1);
        self.write(RVB_IIR_DEST_B0, iir_b0, 1);
        self.write(RVB_IIR_DEST_B1, iir_b1, 1);

        let acc0 = self.read(RVB_ACC_SRC_A0) * ACC_COEF_A
            + self.read(RVB_ACC_SRC_B0) * ACC_COEF_B
            + self.read(RVB_ACC_SRC_C0) * ACC_COEF_C
            + self.read(RVB_ACC_SRC_D0) * ACC_COEF_D;
        let acc1 = self.read(RVB_ACC_SRC_A1) * ACC_COEF_A
            + self.read(RVB_ACC_SRC_B1) * ACC_COEF_B
            + self.read(RVB_ACC_SRC_C1) * ACC_COEF_C
            + self.read(RVB_ACC_SRC_D1) * ACC_COEF_D;

        let feedback_a0 = self.read_relative(RVB_MIX_DEST_A0, -self.offsets[RVB_FB_SRC_A]);
        let feedback_a1 = self.read_relative(RVB_MIX_DEST_A1, -self.offsets[RVB_FB_SRC_A]);
        let feedback_b0 = self.read_relative(RVB_MIX_DEST_B0, -self.offsets[RVB_FB_SRC_B]);
        let feedback_b1 = self.read_relative(RVB_MIX_DEST_B1, -self.offsets[RVB_FB_SRC_B]);
        self.write(RVB_MIX_DEST_A0, acc0 - feedback_a0 * FB_ALPHA, 0);
        self.write(RVB_MIX_DEST_A1, acc1 - feedback_a1 * FB_ALPHA, 0);
        self.write(
            RVB_MIX_DEST_B0,
            FB_ALPHA * acc0 - feedback_a0 * (FB_ALPHA - 1.0) - feedback_b0 * FB_X,
            0,
        );
        self.write(
            RVB_MIX_DEST_B1,
            FB_ALPHA * acc1 - feedback_a1 * (FB_ALPHA - 1.0) - feedback_b1 * FB_X,
            0,
        );

        let left = (self.read(RVB_MIX_DEST_A0) + self.read(RVB_MIX_DEST_B0)) / 3.0;
        let right = (self.read(RVB_MIX_DEST_A1) + self.read(RVB_MIX_DEST_B1)) / 3.0;
        self.cursor += 1;
        if self.cursor == self.buffer.len() {
            self.cursor = 0;
        }
        (left, right)
    }

    fn read_relative(&self, offset: usize, delta: i32) -> f64 {
        let base = self.offsets[offset].saturating_add(delta);
        self.buffer[self.index(base, 0)]
    }
}

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
    /// Start flags retained for game-specific voice arbitration.
    pub flags: SoundFlags,
    /// Monotonic mixer-local start order. Vector order is unstable because
    /// finished voices are removed with `swap_remove`.
    pub start_sequence: u64,
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
    /// Reverse left/right panning for sound effects (Duke `ReverseStereo`).
    pub reverse_stereo: bool,
    /// Gain for active and future Duke sound effects, independent of other games.
    pub duke_sfx_gain: f32,
    /// Duke `MUSICANDSFX` reverb level, clamped to Chocolate's 0–255 range.
    pub reverb_level: u8,
    /// Output sample rate (Hz) — bank PCM and music must match.
    pub sample_rate: u32,
    next_voice_sequence: u64,
    reverb: DukeReverb,
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
            reverse_stereo: false,
            duke_sfx_gain: 1.0,
            reverb_level: 0,
            sample_rate: ENGINE_SAMPLE_RATE,
            next_voice_sequence: 0,
            reverb: DukeReverb::new(ENGINE_SAMPLE_RATE),
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
            reverb: DukeReverb::new(sample_rate.max(1)),
            ..Self::default()
        }
    }

    /// Place the listener at `pos` facing `angle` (radians, Doom convention:
    /// counter-clockwise from +X). Enables stereo panning.
    pub fn set_listener(&mut self, pos: DVec3, angle: f64) {
        self.listener = pos;
        self.listener_facing = Some((room_core::dmath::cos(angle), room_core::dmath::sin(angle)));
    }

    /// Set the Duke ambience reverb level (0 disables; values clamp to 0–255).
    pub fn set_reverb_level(&mut self, level: i32) {
        let level = level.clamp(0, 255) as u8;
        if self.reverb_level != level {
            self.reverb_level = level;
            if level == 0 {
                self.reverb.reset();
            }
        }
    }

    /// Return the first live actor owner for each active Duke sound ID.
    /// Chocolate's `ifnosounds` reads `SoundOwner[id][0]`, so later voices
    /// with the same ID do not count when the first voice belongs elsewhere.
    #[must_use]
    pub fn duke_active_sound_owners(&self) -> Vec<(u32, Option<u64>)> {
        let mut ordered: Vec<&Voice> = self
            .voices
            .iter()
            .filter(|voice| voice.flags.0 & SoundFlags::DUKE_PRIORITY_PRESENT != 0)
            .collect();
        ordered.sort_by_key(|voice| voice.start_sequence);
        let mut seen_sounds = Vec::new();
        let mut owners = Vec::new();
        for voice in ordered {
            let sound_id = voice.sound.raw();
            if sound_id == 0 || sound_id >= 450 || seen_sounds.contains(&sound_id) {
                continue;
            }
            seen_sounds.push(sound_id);
            owners.push((sound_id, voice.source_id));
        }
        owners
    }

    /// Apply Chocolate `xyzsound`'s same-ID voice replacement before a new
    /// actor-owned Duke sound is started. It stops only the most recent voice
    /// when the first owner repeats, when more than one voice already exists,
    /// or when a dead badguy emits the sound. The four-voice ceiling is tested
    /// first because `xyzsound` rejects a fifth voice without stopping one.
    pub fn duke_xyzsound_stop_latest(
        &mut self,
        sound: SoundId,
        source_id: Option<u64>,
        dead_badguy: bool,
    ) -> bool {
        let active: Vec<usize> = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, voice)| {
                voice.sound == sound && voice.flags.0 & SoundFlags::DUKE_PRIORITY_PRESENT != 0
            })
            .map(|(index, _)| index)
            .collect();
        if active.is_empty() || active.len() >= 4 {
            return false;
        }
        let Some(first) = active
            .iter()
            .copied()
            .min_by_key(|&index| self.voices[index].start_sequence)
        else {
            return false;
        };
        let Some(latest) = active
            .iter()
            .copied()
            .max_by_key(|&index| self.voices[index].start_sequence)
        else {
            return false;
        };
        let first_owner = self.voices[first].source_id;
        if first_owner == source_id || active.len() > 1 || dead_badguy {
            self.voices.remove(latest);
            return true;
        }
        false
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
                    if flags.0 & SoundFlags::DUKE_EXCLUSIVE != 0
                        && self
                            .voices
                            .iter()
                            .any(|voice| voice.flags.0 & SoundFlags::DUKE_EXCLUSIVE != 0)
                    {
                        continue;
                    }
                    if flags.0 & SoundFlags::UNIQUE_SOUND != 0
                        && self.voices.iter().any(|voice| voice.sound == sound)
                    {
                        continue;
                    }
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
                    let duke_voice_limit = if flags.0 & SoundFlags::DUKE_LIMIT_ONE != 0 {
                        Some(1)
                    } else if flags.0 & SoundFlags::DUKE_LIMIT_FOUR != 0 {
                        // Chocolate `xyzsound` rejects only once num > 3, so
                        // the fourth ordinary voice is still admitted.
                        Some(4)
                    } else {
                        None
                    };
                    if let Some(limit) = duke_voice_limit
                        && self
                            .voices
                            .iter()
                            .filter(|voice| voice.sound == sound)
                            .count()
                            >= limit
                    {
                        continue;
                    }
                    // Chocolate `movefx()` keeps ambience loops in a separate
                    // voice count. When every FX voice is already an ambience
                    // loop, a newly-entered, closer marker displaces the first
                    // farther active environment loop; otherwise it is skipped.
                    if flags.0 & SoundFlags::DUKE_AMBIENCE != 0
                        && self
                            .voices
                            .iter()
                            .filter(|voice| voice.flags.0 & SoundFlags::DUKE_AMBIENCE != 0)
                            .count()
                            >= DUKE_VOICE_POOL
                    {
                        let incoming_priority = flags.duke_priority().unwrap_or(0);
                        if self.voices.iter().all(|voice| {
                            incoming_priority < voice.flags.duke_priority().unwrap_or(0)
                        }) {
                            // Chocolate checks FX_VoiceAvailable before it
                            // stops an already-playing environment loop.
                            continue;
                        }
                        let incoming_distance = duke_ambience_distance(origin, self.listener);
                        let farther_voice = self.voices.iter().position(|voice| {
                            voice.flags.0 & SoundFlags::DUKE_AMBIENCE != 0
                                && duke_ambience_distance(voice.origin, self.listener)
                                    > incoming_distance
                        });
                        if let Some(index) = farther_voice {
                            self.voices.remove(index);
                        } else {
                            continue;
                        }
                    }
                    if let Some(priority) = flags.duke_priority()
                        && self.voices.len() >= DUKE_VOICE_POOL
                    {
                        let quietest = self
                            .voices
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, voice)| voice.flags.duke_priority().unwrap_or(0));
                        let Some((index, voice)) = quietest else {
                            continue;
                        };
                        if priority < voice.flags.duke_priority().unwrap_or(0) {
                            continue;
                        }
                        self.voices.remove(index);
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
                    let start_sequence = self.next_voice_sequence;
                    self.next_voice_sequence = self.next_voice_sequence.wrapping_add(1);
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
                        flags,
                        start_sequence,
                    });
                }
                SoundEvent::Stop { origin, channel } => {
                    self.voices
                        .retain(|v| !(v.channel == channel && origins_eq(v.origin, origin)));
                }
                SoundEvent::StopSound { sound } => {
                    self.voices.retain(|voice| voice.sound != sound);
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
            let is_duke = voice.flags.0 & SoundFlags::DUKE_PRIORITY_PRESENT != 0;
            let (gain_l, gain_r) = if is_duke {
                duke_stereo_gains(
                    voice.origin,
                    self.listener,
                    self.listener_facing,
                    voice.flags,
                    voice.attenuation,
                    self.duke_sfx_gain * voice.volume * self.master,
                )
            } else {
                let atten = if voice.flags.0 & SoundFlags::ATTN_NONE != 0 {
                    1.0
                } else {
                    attenuate(
                        voice.origin,
                        self.listener,
                        voice.attenuation,
                        voice.flags.duke_distance_offset().map_or(0.0, f64::from),
                    )
                };
                let gain = voice.volume * atten * self.master;
                let (pan_l, pan_r) = stereo_gains(
                    voice.origin,
                    voice.attenuation,
                    self.listener,
                    self.listener_facing,
                );
                (gain * pan_l, gain * pan_r)
            };
            let (gain_l, gain_r) = if self.reverse_stereo && is_duke {
                (gain_r, gain_l)
            } else {
                (gain_l, gain_r)
            };
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
        if self.reverb_level > 0 {
            let reverb_volume = ((i32::from(self.reverb_level) * 64) >> 8).min(63);
            let wet_gain = f64::from(reverb_volume) / 63.0;
            for frame in out.chunks_exact_mut(2) {
                let (wet_l, wet_r) = self
                    .reverb
                    .process_frame(f64::from(frame[0]), f64::from(frame[1]));
                frame[0] += (wet_l * wet_gain) as f32;
                frame[1] += (wet_r * wet_gain) as f32;
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

fn attenuate(origin: SoundOrigin, listener: DVec3, scale: f32, distance_offset: f64) -> f32 {
    if scale <= 0.0 {
        return 1.0;
    }
    let SoundOrigin::Point(p) = origin else {
        return 1.0;
    };
    let dx = p.x() - listener.x();
    let dy = p.y() - listener.y();
    let dz = p.z() - listener.z();
    let dist = (room_core::dmath::sqrt(dx * dx + dy * dy + dz * dz) * f64::from(scale)
        + distance_offset)
        .max(0.0);
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

/// Chocolate's `sounds.c::xyzsound` distance path and `MV_Play3D` gain tables.
fn duke_stereo_gains(
    origin: SoundOrigin,
    listener: DVec3,
    facing: Option<(f64, f64)>,
    flags: SoundFlags,
    attenuation: f32,
    gain: f32,
) -> (f32, f32) {
    // Chocolate `m&128` takes `sound()` directly: centered and full volume.
    if matches!(origin, SoundOrigin::None) {
        return (gain, gain);
    }
    let non_attenuated_position = flags.0 & SoundFlags::ATTN_NONE != 0;
    let raw_distance = if flags.0 & SoundFlags::ATTN_NONE != 0 {
        0
    } else if let SoundOrigin::Point(source) = origin {
        let to_build = |delta: f64| {
            let value = (delta * 16.0).round();
            if value <= f64::from(i32::MIN) {
                i32::MIN
            } else if value >= f64::from(i32::MAX) {
                i32::MAX
            } else {
                #[allow(clippy::cast_possible_truncation)]
                {
                    value as i32
                }
            }
        };
        duke_find_distance_3d(
            to_build(source.x() - listener.x()),
            to_build(source.y() - listener.y()),
            to_build(source.z() - listener.z()),
        )
    } else {
        0
    };
    let offset = if flags.0 & SoundFlags::ATTN_NONE != 0 {
        0
    } else {
        i32::from(flags.duke_distance_offset().unwrap_or(0)).saturating_mul(16)
    };
    let distance = if non_attenuated_position {
        // `xyzsound` zeroes sndist for `m&16` after minimum-distance handling.
        0
    } else {
        let mut distance = raw_distance.saturating_add(offset).max(0);
        if attenuation > 1.0 {
            distance = distance.saturating_add(distance >> 5);
        }
        let sound_distance = distance.max((255 - 150) << 6) >> 6;
        sound_distance.clamp(0, 255)
    };
    let pan_volume = ((distance * 64) >> 8).min(63);
    let table_level = (255 * (63 - pan_volume)) / 63;
    let pan_position = match (facing, origin) {
        (Some((fx, fy)), SoundOrigin::Point(source)) => {
            let source_angle = (source.y() - listener.y()).atan2(source.x() - listener.x());
            let facing_angle = fy.atan2(fx);
            // Chocolate: `sndang = 2048 + ca - getangle(cx-x, cy-y)`;
            // front and back are the symmetric table positions 16 and 0.
            let relative = (facing_angle - source_angle + std::f64::consts::PI)
                .rem_euclid(std::f64::consts::TAU);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                ((relative * 32.0 / std::f64::consts::TAU).floor() as usize) & 31
            }
        }
        _ => 16,
    };
    // Exact integer shape of Chocolate's 32-position `MV_PanTable` at the
    // computed volume row (MV_CalcPanTable, multivoc.c).
    let (left, right) = match pan_position {
        0..=8 => (table_level * (8 - pan_position as i32) / 8, table_level),
        9..=16 => (table_level * (pan_position as i32 - 8) / 8, table_level),
        17..=24 => (table_level, table_level * (24 - pan_position as i32) / 8),
        _ => (table_level, table_level * (pan_position as i32 - 24) / 8),
    };
    (gain * left as f32 / 255.0, gain * right as f32 / 255.0)
}

/// Chocolate `global.c::FindDistance3D` integer approximation.
fn duke_find_distance_3d(mut x: i32, mut y: i32, mut z: i32) -> i32 {
    x = x.saturating_abs();
    y = y.saturating_abs();
    z = z.saturating_abs();
    if x < y {
        std::mem::swap(&mut x, &mut y);
    }
    if x < z {
        std::mem::swap(&mut x, &mut z);
    }
    let others = y.saturating_add(z);
    x.saturating_sub(x >> 4)
        .saturating_add(others >> 2)
        .saturating_add(others >> 3)
}

/// Chocolate Build `dist()` approximation for choosing which positional
/// ambience loop to retain. Audio positions store X/Y in 1/16 Build units.
fn duke_ambience_distance(origin: SoundOrigin, listener: DVec3) -> f64 {
    let SoundOrigin::Point(point) = origin else {
        return f64::INFINITY;
    };
    let mut x = ((point.x() - listener.x()).abs() * 16.0).round();
    let mut y = ((point.y() - listener.y()).abs() * 16.0).round();
    if x < y {
        std::mem::swap(&mut x, &mut y);
    }
    x - (x / 16.0).floor() + (y / 4.0).floor() + (y / 8.0).floor()
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
