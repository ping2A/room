//! SoundFont MIDI synth (`rustysynth` — FluidSynth-class path, `snd_mididevice -5`).

use crate::MusicError;
use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use std::io::Cursor;
use std::sync::Arc;

/// Default sample rate for SoftFont render (Hz).
pub const DEFAULT_SAMPLE_RATE: i32 = 44_100;

/// Loaded GM SoundFont (SF2/SF3).
#[derive(Clone)]
pub struct SoftFont {
    font: Arc<SoundFont>,
}

impl SoftFont {
    /// Parse SF2/SF3 bytes.
    pub fn load(bytes: &[u8]) -> Result<Self, MusicError> {
        let mut reader = Cursor::new(bytes);
        let font = SoundFont::new(&mut reader).map_err(|e| MusicError::Message(e.to_string()))?;
        Ok(Self {
            font: Arc::new(font),
        })
    }

    /// Offline-render a Standard MIDI File to stereo PCM (separate L/R buffers).
    pub fn render_midi(
        &self,
        midi: &[u8],
        sample_rate: i32,
        looping: bool,
    ) -> Result<(Vec<f32>, Vec<f32>), MusicError> {
        let mut reader = Cursor::new(midi);
        let midi_file =
            Arc::new(MidiFile::new(&mut reader).map_err(|e| MusicError::Message(e.to_string()))?);
        let settings = SynthesizerSettings::new(sample_rate);
        let synthesizer = Synthesizer::new(&self.font, &settings)
            .map_err(|e| MusicError::Message(e.to_string()))?;
        let mut sequencer = MidiFileSequencer::new(synthesizer);
        sequencer.play(&midi_file, looping);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let sample_count = (f64::from(sample_rate) * midi_file.get_length()).ceil() as usize;
        let mut left = vec![0.0_f32; sample_count];
        let mut right = vec![0.0_f32; sample_count];
        sequencer.render(&mut left, &mut right);
        Ok((left, right))
    }

    /// Build a realtime sequencer for streaming `render` into mixer buffers.
    pub fn sequencer(&self, sample_rate: i32) -> Result<SoftFontSequencer, MusicError> {
        let settings = SynthesizerSettings::new(sample_rate);
        let synthesizer = Synthesizer::new(&self.font, &settings)
            .map_err(|e| MusicError::Message(e.to_string()))?;
        Ok(SoftFontSequencer {
            inner: MidiFileSequencer::new(synthesizer),
            midi: None,
        })
    }
}

/// Streaming SoftFont MIDI player (mixer-thread friendly).
pub struct SoftFontSequencer {
    inner: MidiFileSequencer,
    midi: Option<Arc<MidiFile>>,
}

impl SoftFontSequencer {
    /// Start (or restart) a Standard MIDI File.
    pub fn play_midi(&mut self, midi: &[u8], looping: bool) -> Result<(), MusicError> {
        let mut reader = Cursor::new(midi);
        let midi_file =
            Arc::new(MidiFile::new(&mut reader).map_err(|e| MusicError::Message(e.to_string()))?);
        self.inner.play(&midi_file, looping);
        self.midi = Some(midi_file);
        Ok(())
    }

    /// Stop playback and silence voices.
    pub fn stop(&mut self) {
        self.inner.stop();
        self.midi = None;
    }

    /// Fill equal-length L/R buffers (sample frames).
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.inner.render(left, right);
    }

    /// True when the sequence has finished (non-looping).
    #[must_use]
    pub fn end_of_sequence(&self) -> bool {
        self.inner.end_of_sequence()
    }

    /// Interleaved stereo render (`[L,R,L,R,…]`).
    pub fn render_interleaved(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        if frames == 0 {
            return;
        }
        let mut left = vec![0.0_f32; frames];
        let mut right = vec![0.0_f32; frames];
        self.inner.render(&mut left, &mut right);
        for i in 0..frames {
            out[i * 2] = left[i];
            out[i * 2 + 1] = right[i];
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn load_rejects_garbage() {
        assert!(SoftFont::load(b"not a soundfont").is_err());
    }

    #[test]
    fn load_rejects_empty() {
        assert!(SoftFont::load(&[]).is_err());
    }
}
