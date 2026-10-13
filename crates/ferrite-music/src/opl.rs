//! OPL3 chip + Doom `GENMIDI` bank + DMX-style MIDI driver (M5 subset).

mod midi;

pub use midi::OplMidiDriver;

use crate::MusicError;
use nuked_opl3::Opl3Chip;

/// Native YMF262 sample rate used by Nuked OPL3.
pub const OPL_NATIVE_RATE: u32 = 49_716;

/// Number of instruments in a Doom `GENMIDI` lump.
pub const GENMIDI_COUNT: usize = 175;
/// Bytes per `GENMIDI` instrument record (DMX).
pub const GENMIDI_INSTR_SIZE: usize = 36;
/// `#OPL_II#` magic + 175 × 36 instrument bytes (names may follow).
pub const GENMIDI_MIN_SIZE: usize = 8 + GENMIDI_COUNT * GENMIDI_INSTR_SIZE;

/// `GENMIDI` fixed-pitch flag.
pub const GENMIDI_FLAG_FIXED: u16 = 0x0001;
/// `GENMIDI` double-voice (OPL3) flag.
pub const GENMIDI_FLAG_2VOICE: u16 = 0x0004;

/// One DMX operator (`genmidi_op_t`).
#[derive(Clone, Copy, Debug, Default)]
pub struct GenMidiOp {
    /// Tremolo / vibrato / sustain / KSR / multiple.
    pub tremolo: u8,
    /// Attack / decay.
    pub attack: u8,
    /// Sustain / release.
    pub sustain: u8,
    /// Waveform select.
    pub waveform: u8,
    /// Key scale level (high 2 bits of level reg).
    pub scale: u8,
    /// Output level (low 6 bits).
    pub level: u8,
}

/// One DMX voice (`genmidi_voice_t`) — modulator + carrier.
#[derive(Clone, Copy, Debug, Default)]
pub struct GenMidiVoice {
    /// Modulator operator.
    pub modulator: GenMidiOp,
    /// Feedback / connection (bit0 = additive).
    pub feedback: u8,
    /// Carrier operator.
    pub carrier: GenMidiOp,
    /// MIDI note offset (semitones).
    pub base_note_offset: i16,
}

/// One DMX `GENMIDI` instrument.
#[derive(Clone, Debug)]
pub struct GenMidiInstrument {
    /// Flags (`FIXED`, `2VOICE`, …).
    pub flags: u16,
    /// Fine tuning for second voice (0–255, 128 = unison).
    pub fine_tuning: u8,
    /// Fixed MIDI note when `FIXED` is set.
    pub fixed_note: u8,
    /// Primary and optional secondary voice.
    pub voices: [GenMidiVoice; 2],
}

/// Parsed Doom `GENMIDI` bank.
#[derive(Clone, Debug)]
pub struct GenMidiBank {
    /// 175 instruments (0–127 melodic, 128–174 percussion).
    pub instruments: Vec<GenMidiInstrument>,
}

fn parse_op(b: &[u8]) -> GenMidiOp {
    GenMidiOp {
        tremolo: b[0],
        attack: b[1],
        sustain: b[2],
        waveform: b[3],
        scale: b[4],
        level: b[5],
    }
}

fn parse_voice(b: &[u8]) -> GenMidiVoice {
    // mod(6) + feedback(1) + car(6) + unused(1) + base_note_offset(2) = 16
    let modulator = parse_op(&b[0..6]);
    let feedback = b[6];
    let carrier = parse_op(&b[7..13]);
    let base_note_offset = i16::from_le_bytes([b[14], b[15]]);
    GenMidiVoice {
        modulator,
        feedback,
        carrier,
        base_note_offset,
    }
}

impl GenMidiBank {
    /// Parse a `GENMIDI` lump (`#OPL_II#` header).
    pub fn parse(bytes: &[u8]) -> Result<Self, MusicError> {
        if bytes.len() < GENMIDI_MIN_SIZE {
            return Err(MusicError::Message(format!(
                "GENMIDI too short: {} < {GENMIDI_MIN_SIZE}",
                bytes.len()
            )));
        }
        if &bytes[0..8] != b"#OPL_II#" {
            return Err(MusicError::Message(
                "GENMIDI missing #OPL_II# header".into(),
            ));
        }
        let mut instruments = Vec::with_capacity(GENMIDI_COUNT);
        let mut off = 8;
        for _ in 0..GENMIDI_COUNT {
            let chunk = &bytes[off..off + GENMIDI_INSTR_SIZE];
            let flags = u16::from_le_bytes([chunk[0], chunk[1]]);
            instruments.push(GenMidiInstrument {
                flags,
                fine_tuning: chunk[2],
                fixed_note: chunk[3],
                voices: [parse_voice(&chunk[4..20]), parse_voice(&chunk[20..36])],
            });
            off += GENMIDI_INSTR_SIZE;
        }
        Ok(Self { instruments })
    }

    /// Melodic instrument 0–127, or percussion 128–174.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&GenMidiInstrument> {
        self.instruments.get(index)
    }
}

/// Thin wrapper around Nuked OPL3 for mixer PCM.
pub struct Opl3Synth {
    chip: Opl3Chip,
}

impl Opl3Synth {
    /// Create a synth at `sample_rate`.
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let mut s = Self {
            chip: Opl3Chip::new(sample_rate.max(1)),
        };
        s.write_register(0x01, 0x20); // wave select enable
        s
    }

    /// Write an OPL register (`reg` may include the 0x100 bank bit).
    pub fn write_register(&mut self, reg: u16, value: u8) {
        self.chip.write_register(reg, value);
    }

    /// Enable OPL3 (4-op / stereo) mode.
    pub fn enable_opl3(&mut self) {
        self.write_register(0x105, 1);
    }

    /// Fill interleaved stereo `f32` PCM.
    pub fn render_interleaved(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        for i in 0..frames {
            let mut buf = [0i16; 2];
            let _ = self.chip.generate(&mut buf);
            out[i * 2] = f32::from(buf[0]) / 32768.0;
            out[i * 2 + 1] = f32::from(buf[1]) / 32768.0;
        }
    }

    /// Program a simple two-op tone on channel 0 (for smoke tests).
    pub fn program_test_tone(&mut self) {
        self.write_register(0x01, 0x20);
        self.write_register(0x20, 0x21);
        self.write_register(0x23, 0x21);
        self.write_register(0x40, 0x10);
        self.write_register(0x43, 0x00);
        self.write_register(0x60, 0xF0);
        self.write_register(0x63, 0xF0);
        self.write_register(0x80, 0xF0);
        self.write_register(0x83, 0xF0);
        self.write_register(0xE0, 0x00);
        self.write_register(0xE3, 0x00);
        self.write_register(0xC0, 0x01);
        self.write_register(0xB0, 0x00);
        self.write_register(0xA0, 0x98);
        self.write_register(0xB0, 0x32);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn genmidi_rejects_short() {
        assert!(GenMidiBank::parse(b"#OPL_II#").is_err());
    }

    #[test]
    fn genmidi_parses_minimal_bank() {
        let mut bytes = vec![0u8; GENMIDI_MIN_SIZE];
        bytes[0..8].copy_from_slice(b"#OPL_II#");
        bytes[8] = 0x04; // flags lo = 2VOICE
        bytes[9] = 0x00;
        bytes[10] = 128; // fine tune
        let bank = GenMidiBank::parse(&bytes).unwrap();
        assert_eq!(bank.instruments.len(), GENMIDI_COUNT);
        assert_eq!(bank.instruments[0].flags, GENMIDI_FLAG_2VOICE);
        assert_eq!(bank.instruments[0].fine_tuning, 128);
    }

    #[test]
    fn opl_test_tone_is_audible() {
        let mut synth = Opl3Synth::new(48_000);
        synth.program_test_tone();
        let mut out = vec![0.0f32; 4800 * 2];
        synth.render_interleaved(&mut out);
        let peak = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.01, "peak={peak}");
    }
}
