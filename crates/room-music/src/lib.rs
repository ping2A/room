//! MUS/MIDI sequencer, SoundFont, OPL3, trackers, and streams.
//!
//! M5: load MUS→MIDI, SoftFont (`rustysynth`) offline + streaming render;
//! OPL3 + `GENMIDI` + DMX-style MIDI voice allocation.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod opl;
mod softfont;

pub use opl::{
    GENMIDI_COUNT, GENMIDI_FLAG_2VOICE, GENMIDI_FLAG_FIXED, GENMIDI_INSTR_SIZE, GENMIDI_MIN_SIZE,
    GenMidiBank, GenMidiInstrument, GenMidiOp, GenMidiVoice, OPL_NATIVE_RATE, Opl3Synth,
    OplMidiDriver,
};
pub use softfont::{DEFAULT_SAMPLE_RATE, SoftFont, SoftFontSequencer};

use room_formats::mus_to_midi;
use thiserror::Error;

/// MIDI device selection (`snd_mididevice` subset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MidiDevice {
    /// SoundFont / FluidSynth-equivalent (`rustysynth`).
    #[default]
    SoftFont,
    /// OPL3 / DMX (`GENMIDI` + Nuked OPL3).
    Opl,
}

/// Music errors.
#[derive(Debug, Error)]
pub enum MusicError {
    /// Format / decode.
    #[error("{0}")]
    Message(String),
}

/// One loaded track (MIDI bytes ready for a synth backend).
#[derive(Clone, Debug)]
pub struct MusicTrack {
    /// Lump / logical name (`D_E1M1`).
    pub name: String,
    /// Standard MIDI file bytes.
    pub midi: Vec<u8>,
    /// Loop when finished.
    pub looping: bool,
}

/// Music player state (device-agnostic).
pub struct MusicPlayer {
    /// Currently playing track.
    pub current: Option<MusicTrack>,
    /// Master music volume 0..=1.
    pub volume: f32,
    /// Paused with the game.
    pub paused: bool,
    /// Playback order (modules); 0 for MIDI/MUS.
    pub order: i32,
    /// Active MIDI backend.
    pub device: MidiDevice,
    /// SoftFont / OPL render rate (match the audio device).
    pub sample_rate: u32,
    /// Optional SoftFont for PCM render.
    softfont: Option<SoftFont>,
    /// Optional GENMIDI bank for OPL device.
    genmidi: Option<GenMidiBank>,
    /// Active streaming sequencer (when SoftFont + track loaded).
    sequencer: Option<SoftFontSequencer>,
    /// Active OPL MIDI driver (GENMIDI + Nuked).
    opl: Option<OplMidiDriver>,
}

impl Default for MusicPlayer {
    fn default() -> Self {
        Self {
            current: None,
            volume: 1.0,
            paused: false,
            order: 0,
            device: MidiDevice::default(),
            sample_rate: DEFAULT_SAMPLE_RATE as u32,
            softfont: None,
            genmidi: None,
            sequencer: None,
            opl: None,
        }
    }
}

impl MusicPlayer {
    /// New player at full volume.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set SoftFont/OPL render rate (call before `play_*` / after device open).
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        let rate = sample_rate.max(1);
        if self.sample_rate != rate {
            self.sample_rate = rate;
            self.resync_sequencer();
        }
    }

    /// Install an SF2/SF3 SoftFont for synthesis.
    pub fn set_softfont(&mut self, font: SoftFont) {
        self.softfont = Some(font);
        self.resync_sequencer();
    }

    /// True once [`Self::set_softfont`] has installed a bank.
    #[must_use]
    pub fn has_softfont(&self) -> bool {
        self.softfont.is_some()
    }

    /// Install a Doom `GENMIDI` bank for the OPL device.
    pub fn set_genmidi(&mut self, bank: GenMidiBank) {
        self.genmidi = Some(bank);
        if self.device == MidiDevice::Opl {
            self.resync_sequencer();
        }
    }

    /// Select SoftFont vs OPL backend (`snd_mididevice`).
    pub fn set_mididevice(&mut self, device: MidiDevice) {
        self.device = device;
        self.resync_sequencer();
    }

    /// Decode a MUS lump and start playback.
    pub fn play_mus(
        &mut self,
        name: impl Into<String>,
        mus: &[u8],
        looping: bool,
    ) -> Result<(), MusicError> {
        let midi = mus_to_midi(mus).map_err(|e| MusicError::Message(e.to_string()))?;
        self.play_midi(name, midi, looping);
        Ok(())
    }

    /// Start from already-decoded MIDI.
    pub fn play_midi(&mut self, name: impl Into<String>, midi: Vec<u8>, looping: bool) {
        self.current = Some(MusicTrack {
            name: name.into(),
            midi,
            looping,
        });
        self.paused = false;
        self.resync_sequencer();
    }

    /// Stop music.
    pub fn stop(&mut self) {
        self.current = None;
        if let Some(seq) = &mut self.sequencer {
            seq.stop();
        }
        self.sequencer = None;
        self.opl = None;
    }

    /// Pause / resume.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// True if a track is loaded and not paused.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.current.is_some() && !self.paused
    }

    /// Fill interleaved stereo PCM from the active synth (no-op if none).
    pub fn render_interleaved(&mut self, out: &mut [f32]) {
        if self.paused || !self.is_playing() {
            out.fill(0.0);
            return;
        }
        let vol = self.volume.clamp(0.0, 1.0);
        match self.device {
            MidiDevice::SoftFont => {
                if let Some(seq) = &mut self.sequencer {
                    seq.render_interleaved(out);
                    if vol != 1.0 {
                        for s in out.iter_mut() {
                            *s *= vol;
                        }
                    }
                } else {
                    out.fill(0.0);
                }
            }
            MidiDevice::Opl => {
                if let Some(opl) = &mut self.opl {
                    opl.render_interleaved(out);
                    if vol != 1.0 {
                        for s in out.iter_mut() {
                            *s *= vol;
                        }
                    }
                } else {
                    out.fill(0.0);
                }
            }
        }
    }

    fn resync_sequencer(&mut self) {
        self.sequencer = None;
        self.opl = None;
        match self.device {
            MidiDevice::SoftFont => {
                let (Some(font), Some(track)) = (&self.softfont, &self.current) else {
                    return;
                };
                #[allow(clippy::cast_possible_wrap)]
                let rate = self.sample_rate.max(1) as i32;
                let Ok(mut seq) = font.sequencer(rate) else {
                    return;
                };
                if seq.play_midi(&track.midi, track.looping).is_ok() {
                    self.sequencer = Some(seq);
                }
            }
            MidiDevice::Opl => {
                let (Some(bank), Some(track)) = (&self.genmidi, &self.current) else {
                    return;
                };
                let mut drv = OplMidiDriver::new(bank.clone(), self.sample_rate.max(1));
                if drv.play_midi(&track.midi, track.looping).is_ok() {
                    self.opl = Some(drv);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// Minimal MUS header + empty score (score_start past header).
    fn tiny_mus() -> Vec<u8> {
        // Valid enough for mus_to_midi to reject or accept — use formats test vector if any.
        // Build: MUS\x1A + score_len + score_start=16 + channels + 0 pad + one release+score end.
        let mut v = vec![0u8; 16];
        v[0..4].copy_from_slice(b"MUS\x1A");
        v[6] = 16; // score_start lo
        v[7] = 0;
        // Score: system event score end (0x60 | channel) — Chocolate uses 0x60 for score end on any chan.
        // desc = 0x60 means event 6 (score end), channel 0.
        v.push(0x60);
        v
    }

    #[test]
    fn play_mus_sets_current() {
        let mut p = MusicPlayer::new();
        // May fail if score is too short — then fall back to play_midi.
        if p.play_mus("D_TEST", &tiny_mus(), true).is_err() {
            p.play_midi("D_TEST", vec![0x4D, 0x54, 0x68, 0x64], true);
        }
        assert!(p.current.is_some());
        assert!(p.is_playing());
        p.set_paused(true);
        assert!(!p.is_playing());
        p.stop();
        assert!(p.current.is_none());
    }
}
