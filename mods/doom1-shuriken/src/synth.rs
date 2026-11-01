//! Integer PCM recipes → DMX lumps (`docs/32` §3.4).
#![deny(clippy::float_arithmetic)]

use alloc::vec::Vec;
use room_script_abi::lump_ns;
use room_script_sdk::reg;
use room_script_sdk::sound::encode_dmx;

use crate::fixed;
use crate::tables::{SOUND_ALIASES, SOUND_RATE};

/// One generated sound lump.
pub struct SoundLump {
    /// Eight-char lump name.
    pub name: &'static str,
    /// Encoded DMX bytes.
    pub data: Vec<u8>,
}

struct XorShift32(u32);

impl XorShift32 {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    fn noise_signed(&mut self) -> i32 {
        (self.next() as i32 >> 16) - 0x8000
    }
}

fn clamp_u8(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

fn samples(ms: u32) -> usize {
    (SOUND_RATE as u32 * ms / 1000) as usize
}

fn sine_q16(phase_q16: i32) -> i32 {
    // 256-entry table; phase in Q16.16 turns (1.0 = full circle).
    let idx = ((phase_q16 >> 8) & 0xff) as u8;
    fixed::sin(idx)
}

/// Band-passed noise whoosh with downward cutoff sweep.
fn synth_throw() -> Vec<u8> {
    let n = samples(180);
    let mut rng = XorShift32(0x51EED_u32);
    let mut lp = 0i32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as i32;
        // Cutoff coeff shrinks over the burst (Q16.16 fraction of noise).
        let cut = 0x6000 - (t * 0x6000 / n as i32);
        let x = rng.noise_signed() >> 2;
        lp += ((x - lp) * cut) >> 16;
        let hp = x - lp;
        let bp = ((hp - (hp >> 3)) * cut) >> 16;
        // Fast attack, exponential-ish decay.
        let env = if i < 8 {
            (i as i32 * 0x10000) / 8
        } else {
            0x10000 - ((t - 8) * 0x10000 / (n as i32 - 8).max(1))
        };
        let env = env.max(0);
        let s = bp.wrapping_mul(env) >> 16;
        out.push(clamp_u8(128 + (s >> 8)));
    }
    out
}

/// Three damped sine partials (clink).
fn synth_clink() -> Vec<u8> {
    let n = samples(250);
    let mut rng = XorShift32(0xC11C_u32);
    let freqs = [2100i32, 3700, 5900];
    let mut phases = [0i32; 3];
    for (i, f) in freqs.iter().enumerate() {
        let detune = (rng.next() % 41) as i32 - 20; // ±20 Hz, fixed seed
        let hz = f + detune;
        // phase step Q16.16 turns per sample: hz / rate
        phases[i] = (hz << 16) / i32::from(SOUND_RATE);
    }
    let mut pos = [0i32; 3];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let mut acc = 0i32;
        let decay = 0x10000 - ((i as i32 * 0x10000) / n as i32);
        for p in 0..3 {
            let s = sine_q16(pos[p]);
            acc += s.wrapping_mul(decay) >> 18;
            pos[p] = pos[p].wrapping_add(phases[p]);
        }
        out.push(clamp_u8(128 + (acc >> 7)));
    }
    out
}

/// Low noise burst + 140 Hz thump.
fn synth_flesh() -> Vec<u8> {
    let n = samples(90);
    let mut rng = XorShift32(0xF1E5_u32);
    let step = (140 << 16) / i32::from(SOUND_RATE);
    let mut phase = 0i32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let noise = rng.noise_signed() >> 5;
        let thump = sine_q16(phase).wrapping_mul(0x10000 - (i as i32 * 0x10000 / n as i32)) >> 17;
        phase = phase.wrapping_add(step);
        let env = if i < 4 {
            (i as i32 * 0x10000) / 4
        } else {
            0x10000 - ((i as i32 - 4) * 0x10000 / (n as i32 - 4).max(1))
        };
        let s = (noise + thump).wrapping_mul(env.max(0)) >> 16;
        out.push(clamp_u8(128 + (s >> 8)));
    }
    out
}

/// Two rising metallic pings.
fn synth_pickup() -> Vec<u8> {
    let n = samples(200);
    let mut out = Vec::with_capacity(n);
    let ping = |out: &mut Vec<u8>, start: usize, len: usize, hz: i32| {
        let step = (hz << 16) / i32::from(SOUND_RATE);
        let mut phase = 0i32;
        for i in 0..len {
            let idx = start + i;
            if idx >= out.len() {
                break;
            }
            let env = 0x10000 - ((i as i32 * 0x10000) / len as i32);
            let s = sine_q16(phase).wrapping_mul(env) >> 17;
            let v = out[idx] as i32 - 128 + (s >> 8);
            out[idx] = clamp_u8(128 + v);
            phase = phase.wrapping_add(step);
        }
    };
    out.resize(n, 128);
    ping(&mut out, 0, samples(80), 1800);
    ping(&mut out, samples(60), samples(100), 2400);
    out
}

/// Wood knock + ping.
fn synth_box() -> Vec<u8> {
    let n = samples(220);
    let mut rng = XorShift32(0xB0_B0_u32);
    let mut lp = 0i32;
    let mut out = Vec::with_capacity(n);
    let step = (400 << 16) / i32::from(SOUND_RATE);
    let mut phase = 0i32;
    for i in 0..n {
        let x = rng.noise_signed() >> 3;
        // Resonant-ish one-pole toward 400 Hz carrier.
        lp += ((x - lp) * 0x2800) >> 16;
        let knock = if i < samples(40) {
            let env = 0x10000 - ((i as i32 * 0x10000) / samples(40) as i32);
            lp.wrapping_mul(env) >> 16
        } else {
            0
        };
        let ping = if i >= samples(30) && i < samples(120) {
            let j = i - samples(30);
            let env = 0x10000 - ((j as i32 * 0x10000) / samples(90) as i32);
            sine_q16(phase).wrapping_mul(env) >> 18
        } else {
            0
        };
        phase = phase.wrapping_add(step);
        out.push(clamp_u8(128 + ((knock + ping) >> 8)));
    }
    out
}

/// Soft cloth slide (low-passed noise).
fn synth_up() -> Vec<u8> {
    let n = samples(300);
    let mut rng = XorShift32(0x51DE_u32);
    let mut lp = 0i32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let x = rng.noise_signed() >> 4;
        lp += ((x - lp) * 0x1800) >> 16;
        let env = if i < samples(40) {
            (i as i32 * 0x10000) / samples(40) as i32
        } else {
            0x10000 - (((i - samples(40)) as i32 * 0x10000) / (n - samples(40)) as i32)
        };
        let s = lp.wrapping_mul(env.max(0)) >> 16;
        out.push(clamp_u8(128 + (s >> 9)));
    }
    out
}

fn recipe(lump: &str) -> Vec<u8> {
    match lump {
        "DSSHKTHR" => synth_throw(),
        "DSSHKCLK" => synth_clink(),
        "DSSHKFLS" => synth_flesh(),
        "DSSHKPKP" => synth_pickup(),
        "DSSHKBOX" => synth_box(),
        "DSSHKUP" => synth_up(),
        _ => alloc::vec![128u8; 64],
    }
}

/// Generate every sound lump (deterministic).
pub fn generate_all() -> Result<Vec<SoundLump>, &'static str> {
    let mut out = Vec::with_capacity(SOUND_ALIASES.len());
    for &(_, lump) in SOUND_ALIASES {
        let pcm = recipe(lump);
        let data = encode_dmx(SOUND_RATE, &pcm)?;
        out.push(SoundLump { name: lump, data });
    }
    Ok(out)
}

/// Register DMX lumps + SNDINFO aliases.
pub fn register() {
    if let Ok(sounds) = generate_all() {
        for s in &sounds {
            let _ = reg::lump(s.name, lump_ns::SOUNDS, &s.data);
        }
    }
    for &(logical, lump) in SOUND_ALIASES {
        let _ = reg::sound_alias(logical, lump, -1);
    }
}
