//! DMX-style MIDI → OPL voice allocation (Chocolate Doom `i_oplmusic` subset).

use super::{
    GENMIDI_FLAG_2VOICE, GENMIDI_FLAG_FIXED, GenMidiBank, GenMidiInstrument, GenMidiOp,
    GenMidiVoice, Opl3Synth,
};
use crate::MusicError;

const OPL_NUM_VOICES: usize = 9;
const OPL_REGS_TREMOLO: u16 = 0x20;
const OPL_REGS_LEVEL: u16 = 0x40;
const OPL_REGS_ATTACK: u16 = 0x60;
const OPL_REGS_SUSTAIN: u16 = 0x80;
const OPL_REGS_WAVEFORM: u16 = 0xE0;
const OPL_REGS_FEEDBACK: u16 = 0xC0;
const OPL_REGS_FREQ_1: u16 = 0xA0;
const OPL_REGS_FREQ_2: u16 = 0xB0;

/// Modulator / carrier operator indices per voice (AdLib layout).
const VOICE_OPERATORS: [[u8; OPL_NUM_VOICES]; 2] = [
    [0x00, 0x01, 0x02, 0x08, 0x09, 0x0a, 0x10, 0x11, 0x12],
    [0x03, 0x04, 0x05, 0x0b, 0x0c, 0x0d, 0x13, 0x14, 0x15],
];

/// F-numbers for C through B within an octave (AdLib-style).
const FNUMS: [u16; 12] = [
    0x16b, 0x181, 0x198, 0x1b0, 0x1ca, 0x1e5, 0x202, 0x220, 0x241, 0x263, 0x287, 0x2ae,
];

#[derive(Clone, Copy, Debug)]
struct ChanState {
    program: u8,
    volume: u8,
    /// MIDI pitch bend (0..=16383, center 8192).
    pitch_bend: u16,
}

impl Default for ChanState {
    fn default() -> Self {
        Self {
            program: 0,
            volume: 100,
            pitch_bend: 8192,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    index: u8,
    op1: u8,
    op2: u8,
    channel: Option<u8>,
    key: u8,
    note: u8,
    instr_voice: u8,
    age: u32,
}

#[derive(Clone, Copy, Debug)]
enum MidiEv {
    NoteOn {
        ch: u8,
        key: u8,
        vel: u8,
    },
    NoteOff {
        ch: u8,
        key: u8,
    },
    Program {
        ch: u8,
        program: u8,
    },
    Control {
        ch: u8,
        ctrl: u8,
        value: u8,
    },
    PitchBend {
        ch: u8,
        value: u16,
    },
    /// Microseconds per quarter note.
    Tempo(u32),
}

#[derive(Clone, Debug)]
struct TimedEv {
    at_sample: u64,
    ev: MidiEv,
}

/// Streaming OPL MIDI player (GENMIDI + Nuked chip).
pub struct OplMidiDriver {
    synth: Opl3Synth,
    bank: GenMidiBank,
    channels: [ChanState; 16],
    voices: [Voice; OPL_NUM_VOICES],
    free: Vec<usize>,
    alloced: Vec<usize>,
    events: Vec<TimedEv>,
    cursor: usize,
    sample_pos: u64,
    sample_rate: u32,
    looping: bool,
    age_clock: u32,
}

impl OplMidiDriver {
    /// Build a driver from a parsed bank at `sample_rate`.
    #[must_use]
    pub fn new(bank: GenMidiBank, sample_rate: u32) -> Self {
        let mut voices = [Voice {
            index: 0,
            op1: 0,
            op2: 0,
            channel: None,
            key: 0,
            note: 0,
            instr_voice: 0,
            age: 0,
        }; OPL_NUM_VOICES];
        let mut free = Vec::with_capacity(OPL_NUM_VOICES);
        for i in 0..OPL_NUM_VOICES {
            voices[i] = Voice {
                index: i as u8,
                op1: VOICE_OPERATORS[0][i],
                op2: VOICE_OPERATORS[1][i],
                channel: None,
                key: 0,
                note: 0,
                instr_voice: 0,
                age: 0,
            };
            free.push(i);
        }
        Self {
            synth: Opl3Synth::new(sample_rate),
            bank,
            channels: [ChanState::default(); 16],
            voices,
            free,
            alloced: Vec::with_capacity(OPL_NUM_VOICES),
            events: Vec::new(),
            cursor: 0,
            sample_pos: 0,
            sample_rate: sample_rate.max(1),
            looping: false,
            age_clock: 0,
        }
    }

    /// Load a Standard MIDI File and arm playback.
    pub fn play_midi(&mut self, midi: &[u8], looping: bool) -> Result<(), MusicError> {
        self.stop_notes();
        self.channels = [ChanState::default(); 16];
        self.events = parse_smf(midi, self.sample_rate)?;
        self.cursor = 0;
        self.sample_pos = 0;
        self.looping = looping;
        Ok(())
    }

    /// Silence all voices and clear the sequence.
    pub fn stop(&mut self) {
        self.stop_notes();
        self.events.clear();
        self.cursor = 0;
        self.sample_pos = 0;
    }

    /// Fill interleaved stereo PCM, advancing the MIDI clock.
    pub fn render_interleaved(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        if frames == 0 {
            return;
        }
        const CHUNK: usize = 64;
        let mut written = 0;
        while written < frames {
            let n = (frames - written).min(CHUNK);
            self.advance_to(self.sample_pos + n as u64);
            let start = written * 2;
            let end = (written + n) * 2;
            self.synth.render_interleaved(&mut out[start..end]);
            self.sample_pos += n as u64;
            written += n;
        }
        if self.looping && self.cursor >= self.events.len() && !self.events.is_empty() {
            let last = self.events.last().map(|e| e.at_sample).unwrap_or(0);
            if self.sample_pos >= last.saturating_add(u64::from(self.sample_rate) / 4) {
                self.cursor = 0;
                self.sample_pos = 0;
                self.stop_notes();
            }
        }
    }

    fn advance_to(&mut self, until: u64) {
        while self.cursor < self.events.len() {
            let at = self.events[self.cursor].at_sample;
            if at > until {
                break;
            }
            let ev = self.events[self.cursor].ev;
            self.cursor += 1;
            self.dispatch(ev);
        }
    }

    fn dispatch(&mut self, ev: MidiEv) {
        match ev {
            MidiEv::NoteOn { ch, key, vel } => {
                if vel == 0 {
                    self.note_off(ch, key);
                } else {
                    self.note_on(ch, key, vel);
                }
            }
            MidiEv::NoteOff { ch, key } => self.note_off(ch, key),
            MidiEv::Program { ch, program } => {
                self.channels[ch as usize].program = program & 0x7f;
            }
            MidiEv::Control { ch, ctrl, value } => {
                if ctrl == 7 || ctrl == 39 {
                    self.channels[ch as usize].volume = value & 0x7f;
                } else if ctrl == 123 {
                    let keys: Vec<_> = self
                        .alloced
                        .iter()
                        .filter_map(|&vi| {
                            let v = self.voices[vi];
                            (v.channel == Some(ch)).then_some(v.key)
                        })
                        .collect();
                    for k in keys {
                        self.note_off(ch, k);
                    }
                }
            }
            MidiEv::PitchBend { ch, value } => {
                self.channels[ch as usize].pitch_bend = value.min(16383);
                self.apply_pitch_bend(ch);
            }
            MidiEv::Tempo(_) => {}
        }
    }

    fn instrument_for(&self, ch: u8, key: u8) -> Option<&GenMidiInstrument> {
        if ch == 9 {
            let idx = 128usize.saturating_add(usize::from(key).saturating_sub(35));
            self.bank.get(idx.min(174))
        } else {
            self.bank.get(self.channels[ch as usize].program as usize)
        }
    }

    fn note_on(&mut self, ch: u8, key: u8, vel: u8) {
        let Some(instr) = self.instrument_for(ch, key).cloned() else {
            return;
        };
        let voices_needed = if instr.flags & GENMIDI_FLAG_2VOICE != 0 {
            2
        } else {
            1
        };
        for iv in 0..voices_needed {
            if let Some(vi) = self.alloc_voice(iv == 0) {
                self.program_voice(vi, ch, key, vel, &instr, iv as u8);
            } else if iv == 0 {
                break;
            }
        }
    }

    fn note_off(&mut self, ch: u8, key: u8) {
        let mut i = 0;
        while i < self.alloced.len() {
            let vi = self.alloced[i];
            let v = self.voices[vi];
            if v.channel == Some(ch) && v.key == key {
                self.key_off(vi);
                self.voices[vi].channel = None;
                self.alloced.remove(i);
                self.free.push(vi);
            } else {
                i += 1;
            }
        }
    }

    fn alloc_voice(&mut self, steal: bool) -> Option<usize> {
        if let Some(vi) = self.free.pop() {
            self.alloced.push(vi);
            return Some(vi);
        }
        if !steal || self.alloced.is_empty() {
            return None;
        }
        let mut best = 0;
        for (i, &vi) in self.alloced.iter().enumerate() {
            let v = self.voices[vi];
            let b = self.voices[self.alloced[best]];
            let prefer_second = v.instr_voice != 0 && b.instr_voice == 0;
            let older_same_kind = v.instr_voice == b.instr_voice && v.age < b.age;
            if prefer_second || older_same_kind {
                best = i;
            }
        }
        let vi = self.alloced.remove(best);
        self.key_off(vi);
        self.voices[vi].channel = None;
        self.alloced.push(vi);
        Some(vi)
    }

    fn program_voice(
        &mut self,
        vi: usize,
        ch: u8,
        key: u8,
        vel: u8,
        instr: &GenMidiInstrument,
        instr_voice: u8,
    ) {
        self.age_clock = self.age_clock.wrapping_add(1);
        let voice_data = instr.voices[instr_voice as usize];
        let note = if instr.flags & GENMIDI_FLAG_FIXED != 0 {
            instr.fixed_note
        } else {
            let mut n = i16::from(key) + voice_data.base_note_offset;
            if instr_voice != 0 {
                let ft = i16::from(instr.fine_tuning) - 128;
                n += ft / 64;
            }
            while n < 0 {
                n += 12;
            }
            while n > 95 {
                n -= 12;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                n as u8
            }
        };
        {
            let v = &mut self.voices[vi];
            v.channel = Some(ch);
            v.key = key;
            v.note = note;
            v.instr_voice = instr_voice;
            v.age = self.age_clock;
        }
        self.load_instrument(vi, &voice_data);
        let chan_vol = self.channels[ch as usize].volume;
        let level = midi_to_opl_level(vel, chan_vol);
        self.set_voice_volume(vi, &voice_data, level);
        let bend = self.channels[ch as usize].pitch_bend;
        let (fnum, block) = note_to_fnum_block_bent(note, bend);
        let idx = u16::from(self.voices[vi].index);
        self.synth
            .write_register(OPL_REGS_FREQ_1 + idx, (fnum & 0xff) as u8);
        self.synth.write_register(
            OPL_REGS_FREQ_2 + idx,
            0x20 | (block << 2) | ((fnum >> 8) as u8 & 0x03),
        );
    }

    fn apply_pitch_bend(&mut self, ch: u8) {
        let bend = self.channels[ch as usize].pitch_bend;
        let sounding: Vec<(usize, u8)> = self
            .alloced
            .iter()
            .filter_map(|&vi| {
                let v = self.voices[vi];
                (v.channel == Some(ch)).then_some((vi, v.note))
            })
            .collect();
        for (vi, note) in sounding {
            let (fnum, block) = note_to_fnum_block_bent(note, bend);
            let idx = u16::from(self.voices[vi].index);
            self.synth
                .write_register(OPL_REGS_FREQ_1 + idx, (fnum & 0xff) as u8);
            self.synth.write_register(
                OPL_REGS_FREQ_2 + idx,
                0x20 | (block << 2) | ((fnum >> 8) as u8 & 0x03),
            );
        }
    }

    fn load_instrument(&mut self, vi: usize, data: &GenMidiVoice) {
        let op1 = u16::from(self.voices[vi].op1);
        let op2 = u16::from(self.voices[vi].op2);
        let modulating = data.feedback & 1 == 0;
        self.load_op(op2, &data.carrier, true);
        self.load_op(op1, &data.modulator, !modulating);
        let idx = u16::from(self.voices[vi].index);
        self.synth
            .write_register(OPL_REGS_FEEDBACK + idx, data.feedback | 0x30);
    }

    fn load_op(&mut self, op: u16, data: &GenMidiOp, max_level: bool) {
        let level = if max_level {
            data.scale | 0x3f
        } else {
            data.scale | (data.level & 0x3f)
        };
        self.synth.write_register(OPL_REGS_LEVEL + op, level);
        self.synth
            .write_register(OPL_REGS_TREMOLO + op, data.tremolo);
        self.synth.write_register(OPL_REGS_ATTACK + op, data.attack);
        self.synth
            .write_register(OPL_REGS_SUSTAIN + op, data.sustain);
        self.synth
            .write_register(OPL_REGS_WAVEFORM + op, data.waveform);
    }

    fn set_voice_volume(&mut self, vi: usize, data: &GenMidiVoice, car_tl: u8) {
        let op2 = u16::from(self.voices[vi].op2);
        let level = data.carrier.scale | (car_tl & 0x3f);
        self.synth.write_register(OPL_REGS_LEVEL + op2, level);
        if data.feedback & 1 != 0 && data.modulator.level != 0x3f {
            let op1 = u16::from(self.voices[vi].op1);
            let mut mod_tl = data.modulator.level & 0x3f;
            if mod_tl < car_tl {
                mod_tl = car_tl;
            }
            self.synth
                .write_register(OPL_REGS_LEVEL + op1, data.modulator.scale | mod_tl);
        }
    }

    fn key_off(&mut self, vi: usize) {
        let idx = u16::from(self.voices[vi].index);
        let (fnum, block) = note_to_fnum_block(self.voices[vi].note);
        self.synth
            .write_register(OPL_REGS_FREQ_1 + idx, (fnum & 0xff) as u8);
        self.synth.write_register(
            OPL_REGS_FREQ_2 + idx,
            (block << 2) | ((fnum >> 8) as u8 & 0x03),
        );
    }

    fn stop_notes(&mut self) {
        let alloced = self.alloced.clone();
        for vi in alloced {
            self.key_off(vi);
            self.voices[vi].channel = None;
            if !self.free.contains(&vi) {
                self.free.push(vi);
            }
        }
        self.alloced.clear();
    }
}

fn midi_to_opl_level(vel: u8, chan_vol: u8) -> u8 {
    let v = u32::from(vel) * u32::from(chan_vol);
    let amp = (v / 127).min(127);
    #[allow(clippy::cast_possible_truncation)]
    {
        (63 - (amp * 63 / 127)) as u8
    }
}

fn note_to_fnum_block(note: u8) -> (u16, u8) {
    let note = note.min(95);
    let block = (note / 12).min(7);
    let fnum = FNUMS[(note % 12) as usize];
    (fnum, block)
}

/// Note + channel pitch bend (±2 semitones around MIDI center 8192).
fn note_to_fnum_block_bent(note: u8, bend: u16) -> (u16, u8) {
    let bend = i32::from(bend.min(16383));
    // ±2 semitones across the full bend range.
    let delta = (bend - 8192) as f64 * 2.0 / 8192.0;
    let n = (f64::from(note.min(95)) + delta).clamp(0.0, 95.0);
    let block = (n / 12.0).floor().clamp(0.0, 7.0) as u8;
    let frac = n - f64::from(block) * 12.0;
    let i0 = frac.floor().clamp(0.0, 11.0) as usize;
    let i1 = (i0 + 1).min(11);
    let t = (frac - i0 as f64).clamp(0.0, 1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fnum = (f64::from(FNUMS[i0]) * (1.0 - t) + f64::from(FNUMS[i1]) * t).round() as u16;
    (fnum, block)
}

fn parse_smf(data: &[u8], sample_rate: u32) -> Result<Vec<TimedEv>, MusicError> {
    if data.len() < 14 || &data[0..4] != b"MThd" {
        return Err(MusicError::Message("not a Standard MIDI File".into()));
    }
    let header_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if data.len() < 8 + header_len {
        return Err(MusicError::Message("truncated MIDI header".into()));
    }
    let format = u16::from_be_bytes([data[8], data[9]]);
    let ntrks = u16::from_be_bytes([data[10], data[11]]) as usize;
    let division = u16::from_be_bytes([data[12], data[13]]);
    if division & 0x8000 != 0 {
        return Err(MusicError::Message(
            "SMPTE MIDI division unsupported".into(),
        ));
    }
    let tpq = u32::from(division).max(1);
    let mut off = 8 + header_len;
    let mut tick_events: Vec<(u32, MidiEv)> = Vec::new();
    for _ in 0..ntrks {
        if off + 8 > data.len() || &data[off..off + 4] != b"MTrk" {
            break;
        }
        let tlen = u32::from_be_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
            as usize;
        off += 8;
        if off + tlen > data.len() {
            return Err(MusicError::Message("truncated MIDI track".into()));
        }
        tick_events.extend(parse_track(&data[off..off + tlen])?);
        off += tlen;
        if format == 0 {
            break;
        }
    }
    tick_events.sort_by_key(|(t, _)| *t);

    let mut tempo_us = 500_000u32;
    let mut events = Vec::with_capacity(tick_events.len());
    let mut last_tick = 0u32;
    let mut sample_at = 0.0f64;
    let sr = f64::from(sample_rate);
    for (tick, ev) in tick_events {
        let dt = tick.saturating_sub(last_tick);
        last_tick = tick;
        sample_at += f64::from(dt) * f64::from(tempo_us) / f64::from(tpq) / 1_000_000.0 * sr;
        if let MidiEv::Tempo(us) = ev {
            tempo_us = us.max(1);
            continue;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        events.push(TimedEv {
            at_sample: sample_at as u64,
            ev,
        });
    }
    Ok(events)
}

fn parse_track(data: &[u8]) -> Result<Vec<(u32, MidiEv)>, MusicError> {
    let mut events = Vec::new();
    let mut off = 0;
    let mut tick = 0u32;
    let mut running: u8 = 0;
    while off < data.len() {
        let (dt, n) = read_vlq(&data[off..])?;
        off += n;
        tick = tick.saturating_add(dt);
        if off >= data.len() {
            break;
        }
        let mut status = data[off];
        if status < 0x80 {
            if running == 0 {
                return Err(MusicError::Message("MIDI running status missing".into()));
            }
            status = running;
        } else {
            off += 1;
            if status < 0xf0 {
                running = status;
            }
        }
        let kind = status & 0xf0;
        let ch = status & 0x0f;
        match kind {
            0x80 if off + 1 < data.len() => {
                let key = data[off] & 0x7f;
                off += 2;
                events.push((tick, MidiEv::NoteOff { ch, key }));
            }
            0x90 if off + 1 < data.len() => {
                let key = data[off] & 0x7f;
                let vel = data[off + 1] & 0x7f;
                off += 2;
                events.push((tick, MidiEv::NoteOn { ch, key, vel }));
            }
            0xb0 if off + 1 < data.len() => {
                let ctrl = data[off];
                let value = data[off + 1] & 0x7f;
                off += 2;
                events.push((tick, MidiEv::Control { ch, ctrl, value }));
            }
            0xc0 if off < data.len() => {
                let program = data[off] & 0x7f;
                off += 1;
                events.push((tick, MidiEv::Program { ch, program }));
            }
            0xe0 if off + 1 < data.len() => {
                let lo = data[off] & 0x7f;
                let hi = data[off + 1] & 0x7f;
                off += 2;
                let value = u16::from(lo) | (u16::from(hi) << 7);
                events.push((tick, MidiEv::PitchBend { ch, value }));
            }
            0xa0 if off + 1 < data.len() => {
                off += 2;
            }
            0xd0 if off < data.len() => {
                off += 1;
            }
            0xf0 => {
                if status == 0xff {
                    if off >= data.len() {
                        break;
                    }
                    let meta = data[off];
                    off += 1;
                    let (len, n) = read_vlq(&data[off..])?;
                    off += n;
                    let len = len as usize;
                    if meta == 0x51 && len == 3 && off + 3 <= data.len() {
                        let us = (u32::from(data[off]) << 16)
                            | (u32::from(data[off + 1]) << 8)
                            | u32::from(data[off + 2]);
                        events.push((tick, MidiEv::Tempo(us)));
                    }
                    if meta == 0x2f {
                        break;
                    }
                    off += len;
                } else {
                    let (len, n) = read_vlq(&data[off..])?;
                    off += n + len as usize;
                }
            }
            _ => break,
        }
    }
    Ok(events)
}

fn read_vlq(data: &[u8]) -> Result<(u32, usize), MusicError> {
    let mut value = 0u32;
    for (i, &b) in data.iter().enumerate().take(4) {
        value = (value << 7) | u32::from(b & 0x7f);
        if b & 0x80 == 0 {
            return Ok((value, i + 1));
        }
    }
    Err(MusicError::Message("invalid MIDI VLQ".into()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::opl::GENMIDI_MIN_SIZE;

    fn bank_with_beep() -> GenMidiBank {
        let mut bytes = vec![0u8; GENMIDI_MIN_SIZE];
        bytes[0..8].copy_from_slice(b"#OPL_II#");
        let base = 8;
        bytes[base + 2] = 128;
        let v = base + 4;
        bytes[v] = 0x21;
        bytes[v + 1] = 0xF0;
        bytes[v + 2] = 0xF0;
        bytes[v + 3] = 0x00;
        bytes[v + 4] = 0x00;
        bytes[v + 5] = 0x10;
        bytes[v + 6] = 0x01;
        bytes[v + 7] = 0x21;
        bytes[v + 8] = 0xF0;
        bytes[v + 9] = 0xF0;
        bytes[v + 10] = 0x00;
        bytes[v + 11] = 0x00;
        bytes[v + 12] = 0x00;
        GenMidiBank::parse(&bytes).unwrap()
    }

    fn tiny_smf() -> Vec<u8> {
        let track = [
            0x00, 0xC0, 0x00, // program 0
            0x00, 0x90, 60, 100, // note on
            0x83, 0x60, 0x80, 60, 0x00, // delta 480, note off
            0x00, 0xFF, 0x2F, 0x00, // end
        ];
        let mut smf = Vec::new();
        smf.extend_from_slice(b"MThd");
        smf.extend_from_slice(&6u32.to_be_bytes());
        smf.extend_from_slice(&0u16.to_be_bytes()); // format 0
        smf.extend_from_slice(&1u16.to_be_bytes()); // 1 track
        smf.extend_from_slice(&480u16.to_be_bytes()); // tpq
        smf.extend_from_slice(b"MTrk");
        smf.extend_from_slice(&(track.len() as u32).to_be_bytes());
        smf.extend_from_slice(&track);
        smf
    }

    #[test]
    fn midi_note_produces_audio() {
        let bank = bank_with_beep();
        let mut drv = OplMidiDriver::new(bank, 48_000);
        drv.play_midi(&tiny_smf(), false).unwrap();
        let mut out = vec![0.0f32; 48_000 * 2];
        drv.render_interleaved(&mut out);
        let peak = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.005, "peak={peak}");
    }

    #[test]
    fn pitch_bend_shifts_fnum() {
        let (f0, b0) = note_to_fnum_block_bent(60, 8192);
        let (f_up, b_up) = note_to_fnum_block_bent(60, 16383);
        assert_eq!((f0, b0), note_to_fnum_block(60));
        assert!(
            f_up != f0 || b_up != b0,
            "max bend should change frequency ({f0}/{b0} vs {f_up}/{b_up})"
        );
    }

    #[test]
    fn velocity_scales_opl_level() {
        let quiet = midi_to_opl_level(1, 127);
        let loud = midi_to_opl_level(127, 127);
        assert!(
            quiet > loud,
            "higher velocity → lower TL (quieter={quiet} louder={loud})"
        );
    }
}
