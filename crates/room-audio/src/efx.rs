//! EFX / EAX reverb: a port of OpenAL Soft's reverb (`alc/effects/reverb.cpp`,
//! Copyright 2008-2017 Chris Robinson and Christopher Fitzgerald,
//! LGPL-2.1-or-later, used here under the GPL-3.0-or-later as the LGPL allows).
//!
//! UZDoom hands every sound environment to OpenAL as an `AL_EFFECT_EAXREVERB`
//! (`oalsound.cpp` `LoadReverb`); OpenAL Soft renders it with this network,
//! so porting it gives the reverb UZDoom players hear:
//!
//! - input is first-order B-format (W, Y, Z, X), converted to a tetrahedral
//!   4-line A-format;
//! - master filters: a high shelf (`GainHF` at `HFReference`) and a low shelf
//!   (`GainLF` at `LFReference`) per line;
//! - early reflections: decorrelated taps on the main delay line, a 4-line
//!   all-pass, a reflected delay for the secondary reflections;
//! - late reverb: a 4-line feedback delay network with a cubic-interpolated
//!   modulated read (`ModulationTime` / `ModulationDepth`), 3-band T60
//!   shelving filters (`DecayTime`, `DecayHFRatio`, `DecayLFRatio`,
//!   `DecayHFLimit` with `AirAbsorptionGainHF`), a Gerzon vector all-pass for
//!   diffusion and a scattering matrix for feedback;
//! - `ReflectionsPan` / `LateReverbPan` focus the early and late output;
//! - the result is decoded to stereo with OpenAL Soft's first-order 2D stereo
//!   decoder;
//! - a change of density, diffusion, decay or modulation starts a second
//!   pipeline and cross-fades, as OpenAL Soft does.
//!
//! OpenAL Soft processes blocks of up to 256 samples; this runs per sample and
//! spreads the same tap and gain cross-fades over [`FADE_SAMPLES`]. As in
//! current OpenAL Soft, `EchoTime` / `EchoDepth` have no effect.

use crate::reverb::{ReverbProps, mb_to_gain};
use std::f32::consts::{PI, SQRT_2};

const NUM_LINES: usize = 4;
const MIN_DECAY_TIME: f32 = 0.1;
const MAX_DECAY_TIME: f32 = 20.0;
const MAX_MODULATION_TIME: f32 = 4.0;
const DEFAULT_MODULATION_TIME: f32 = 0.25;
const MAX_HF_REFERENCE: f32 = 20_000.0;
const MODULATION_DEPTH_COEFF: f32 = 0.05;
const DENSITY_SCALE: f32 = 1000.0;
/// −60 dB.
const DECAY_GAIN: f32 = 0.001;
const MAX_REFLECTIONS_DELAY: f32 = 0.3;
const MAX_LATE_REVERB_DELAY: f32 = 0.1;
const SPEED_OF_SOUND: f32 = 343.3;
const MOD_FRACBITS: u32 = 24;
const MOD_FRACONE: u32 = 1 << MOD_FRACBITS;
const MOD_FRACMASK: u32 = MOD_FRACONE - 1;
/// Cubic resampler table resolution (`gCubicTable`).
const CUBIC_BITS: u32 = 8;
const CUBIC_STEPS: usize = 1 << CUBIC_BITS;
/// Samples over which a tap, gain or pipeline change is cross-faded.
pub const FADE_SAMPLES: usize = 256;

const INV_SQRT2: f32 = 1.0 / SQRT_2;

/// B-Format (W Y Z X) → W-normalised A-Format.
const B2A: [[f32; 4]; 4] = [
    [0.5, 0.5, 0.5, 0.5],
    [0.5, -0.5, -0.5, 0.5],
    [0.5, 0.5, -0.5, -0.5],
    [0.5, -0.5, 0.5, -0.5],
];
/// A-Format → B-Format, early reflections (rows: W Y Z X).
const EARLY_A2B: [[f32; 4]; 4] = [
    [0.5, 0.5, 0.5, 0.5],
    [0.5, -0.5, 0.5, -0.5],
    [0.5, -0.5, -0.5, 0.5],
    [0.5, 0.5, -0.5, -0.5],
];
/// A-Format → B-Format, late reverb (front lines horizontal, rear vertical).
const LATE_A2B: [[f32; 4]; 4] = [
    [0.5, 0.5, 0.5, 0.5],
    [INV_SQRT2, -INV_SQRT2, 0.0, 0.0],
    [0.0, 0.0, -INV_SQRT2, INV_SQRT2],
    [0.5, 0.5, -0.5, -0.5],
];

const EARLY_TAP_LENGTHS: [f32; 4] = [0.0, 1.010_676e-3, 2.126_553e-3, 3.358_58e-3];
const EARLY_ALLPASS_LENGTHS: [f32; 4] = [4.854_84e-4, 5.360_178e-4, 5.918_117e-4, 6.534_13e-4];
const EARLY_LINE_LENGTHS: [f32; 4] = [2.992_52e-3, 5.456_575e-3, 7.688_329e-3, 9.709_681e-3];
const LATE_ALLPASS_LENGTHS: [f32; 4] = [8.0914e-4, 1.019_453e-3, 1.407_968e-3, 1.618_28e-3];
const LATE_LINE_LENGTHS: [f32; 4] = [9.709_681e-3, 1.223_343e-2, 1.689_561e-2, 1.941_936e-2];

fn late_allpass_average() -> f32 {
    LATE_ALLPASS_LENGTHS.iter().sum::<f32>() / NUM_LINES as f32
}

fn late_delay_average() -> f32 {
    LATE_LINE_LENGTHS.iter().sum::<f32>() / NUM_LINES as f32 + late_allpass_average()
}

/// OpenAL Soft's first-order 2D stereo decoder (N3D: W, Y, X per side).
const STEREO_DECODER: [[f32; 3]; 2] = [
    [5.0e-1, 2.886_751_35e-1, 5.523_056_43e-2],
    [5.0e-1, -2.886_751_35e-1, 5.523_056_43e-2],
];

/// EAX-reverb parameters as OpenAL sees them (`AL_EAXREVERB_*`, clamped).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EfxParams {
    /// `DENSITY` 0–1.
    pub density: f32,
    /// `DIFFUSION` 0–1.
    pub diffusion: f32,
    /// `GAIN` (Room) 0–1.
    pub gain: f32,
    /// `GAINHF` 0–1.
    pub gain_hf: f32,
    /// `GAINLF` 0–1.
    pub gain_lf: f32,
    /// `DECAY_TIME` 0.1–20 s.
    pub decay_time: f32,
    /// `DECAY_HFRATIO` 0.1–2.
    pub decay_hf_ratio: f32,
    /// `DECAY_LFRATIO` 0.1–2.
    pub decay_lf_ratio: f32,
    /// `REFLECTIONS_GAIN` 0–3.16.
    pub reflections_gain: f32,
    /// `REFLECTIONS_DELAY` 0–0.3 s.
    pub reflections_delay: f32,
    /// `REFLECTIONS_PAN`.
    pub reflections_pan: [f32; 3],
    /// `LATE_REVERB_GAIN` 0–10.
    pub late_reverb_gain: f32,
    /// `LATE_REVERB_DELAY` 0–0.1 s.
    pub late_reverb_delay: f32,
    /// `LATE_REVERB_PAN`.
    pub late_reverb_pan: [f32; 3],
    /// `MODULATION_TIME` 0.04–4 s.
    pub modulation_time: f32,
    /// `MODULATION_DEPTH` 0–1.
    pub modulation_depth: f32,
    /// `AIR_ABSORPTION_GAINHF` 0.892–1.
    pub air_absorption_gain_hf: f32,
    /// `HFREFERENCE` 1000–20000 Hz.
    pub hf_reference: f32,
    /// `LFREFERENCE` 20–1000 Hz.
    pub lf_reference: f32,
    /// `DECAY_HFLIMIT`.
    pub decay_hf_limit: bool,
}

impl EfxParams {
    /// UZDoom's `LoadReverb` mapping from a `REVERBS` entry.
    #[must_use]
    pub fn from_props(p: &ReverbProps) -> Self {
        Self {
            diffusion: p.env_diffusion.clamp(0.0, 1.0),
            density: (p.env_size.powi(3) * 0.0625).clamp(0.0, 1.0),
            gain: mb_to_gain(p.room as f32).clamp(0.0, 1.0),
            gain_hf: mb_to_gain(p.room_hf as f32).clamp(0.0, 1.0),
            gain_lf: mb_to_gain(p.room_lf as f32).clamp(0.0, 1.0),
            decay_time: p.decay_time.clamp(0.1, 20.0),
            decay_hf_ratio: p.decay_hf_ratio.clamp(0.1, 2.0),
            decay_lf_ratio: p.decay_lf_ratio.clamp(0.1, 2.0),
            reflections_gain: mb_to_gain(p.reflections as f32).clamp(0.0, 3.16),
            reflections_delay: p.reflections_delay.clamp(0.0, 0.3),
            reflections_pan: p.reflections_pan,
            late_reverb_gain: mb_to_gain(p.reverb as f32).clamp(0.0, 10.0),
            late_reverb_delay: p.reverb_delay.clamp(0.0, 0.1),
            late_reverb_pan: p.reverb_pan,
            modulation_time: p.modulation_time.clamp(0.04, 4.0),
            modulation_depth: p.modulation_depth.clamp(0.0, 1.0),
            air_absorption_gain_hf: mb_to_gain(p.air_absorption_hf).clamp(0.892, 1.0),
            hf_reference: p.hf_reference.clamp(1000.0, 20_000.0),
            lf_reference: p.lf_reference.clamp(20.0, 1000.0),
            decay_hf_limit: p.flags & (1 << 5) != 0,
        }
    }
}

// --- Filters -----------------------------------------------------------------

/// OpenAL Soft `BiquadFilter` (transposed direct form II), shelves only.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

impl Biquad {
    /// `setParamsFromSlope(type, f0norm, gain, slope)`; `high`: high shelf.
    pub(crate) fn set_shelf(&mut self, high: bool, f0norm: f32, gain: f32, slope: f32) {
        let gain = gain.max(0.001).max(0.00001);
        let rcp_q = ((gain + 1.0 / gain) * (1.0 / slope - 1.0) + 2.0).sqrt();
        let w0 = PI * 2.0 * f0norm.min(0.49);
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / 2.0 * rcp_q;
        let sa2 = 2.0 * gain.sqrt() * alpha;
        let (b, a) = if high {
            (
                [
                    gain * ((gain + 1.0) + (gain - 1.0) * cos_w0 + sa2),
                    -2.0 * gain * ((gain - 1.0) + (gain + 1.0) * cos_w0),
                    gain * ((gain + 1.0) + (gain - 1.0) * cos_w0 - sa2),
                ],
                [
                    (gain + 1.0) - (gain - 1.0) * cos_w0 + sa2,
                    2.0 * ((gain - 1.0) - (gain + 1.0) * cos_w0),
                    (gain + 1.0) - (gain - 1.0) * cos_w0 - sa2,
                ],
            )
        } else {
            (
                [
                    gain * ((gain + 1.0) - (gain - 1.0) * cos_w0 + sa2),
                    2.0 * gain * ((gain - 1.0) - (gain + 1.0) * cos_w0),
                    gain * ((gain + 1.0) - (gain - 1.0) * cos_w0 - sa2),
                ],
                [
                    (gain + 1.0) + (gain - 1.0) * cos_w0 + sa2,
                    -2.0 * ((gain - 1.0) + (gain + 1.0) * cos_w0),
                    (gain + 1.0) + (gain - 1.0) * cos_w0 - sa2,
                ],
            )
        };
        self.b0 = b[0] / a[0];
        self.b1 = b[1] / a[0];
        self.b2 = b[2] / a[0];
        self.a1 = a[1] / a[0];
        self.a2 = a[2] / a[0];
    }

    fn copy_params(&mut self, o: &Self) {
        self.b0 = o.b0;
        self.b1 = o.b1;
        self.b2 = o.b2;
        self.a1 = o.a1;
        self.a2 = o.a2;
    }

    #[inline]
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let y = x * self.b0 + self.z1;
        self.z1 = x * self.b1 - y * self.a1 + self.z2;
        self.z2 = x * self.b2 - y * self.a2;
        y
    }

    pub(crate) fn clear(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Cubic (B-spline-like) resampler coefficients, `gCubicTable`.
#[derive(Clone)]
struct CubicTable {
    filter: Vec<f32>,
}

impl CubicTable {
    fn new() -> Self {
        let mut filter = vec![0.0f32; CUBIC_STEPS * 2 + 1];
        let third = 1.0 / 3.0;
        let sixth = 1.0 / 6.0;
        for i in 0..=CUBIC_STEPS / 2 {
            let mu = i as f64 / CUBIC_STEPS as f64;
            let (mu2, mu3) = (mu * mu, mu * mu * mu);
            let c0 = -third * mu + 0.5 * mu2 - sixth * mu3;
            let c1 = 1.0 - 0.5 * mu - mu2 + 0.5 * mu3;
            let c2 = mu + 0.5 * mu2 - 0.5 * mu3;
            let c3 = -sixth * mu + sixth * mu3;
            filter[CUBIC_STEPS + i] = c0 as f32;
            filter[i] = c1 as f32;
            filter[CUBIC_STEPS - i] = c2 as f32;
            filter[CUBIC_STEPS * 2 - i] = c3 as f32;
        }
        Self { filter }
    }
    #[inline]
    fn c0(&self, i: usize) -> f32 {
        self.filter[CUBIC_STEPS + i]
    }
    #[inline]
    fn c1(&self, i: usize) -> f32 {
        self.filter[i]
    }
    #[inline]
    fn c2(&self, i: usize) -> f32 {
        self.filter[CUBIC_STEPS - i]
    }
    #[inline]
    fn c3(&self, i: usize) -> f32 {
        self.filter[CUBIC_STEPS * 2 - i]
    }
}

// --- Delay lines ---------------------------------------------------------------

/// Four delay lines of one power-of-two length.
#[derive(Clone, Debug, Default)]
struct Lines {
    buf: [Vec<f32>; NUM_LINES],
    mask: usize,
}

impl Lines {
    fn new(seconds: f32, rate: f32, extra: usize) -> Self {
        let n = ((seconds * rate).ceil() as usize + extra).next_power_of_two().max(4);
        Self {
            buf: std::array::from_fn(|_| vec![0.0; n]),
            mask: n - 1,
        }
    }
    #[inline]
    fn get(&self, c: usize, pos: usize) -> f32 {
        self.buf[c][pos & self.mask]
    }
    #[inline]
    fn set(&mut self, c: usize, pos: usize, v: f32) {
        let m = self.mask;
        self.buf[c][pos & m] = v;
    }
    fn clear(&mut self) {
        for b in &mut self.buf {
            b.fill(0.0);
        }
    }
}

/// `VectorPartialScatter`.
#[inline]
fn scatter(i: [f32; 4], x: f32, y: f32) -> [f32; 4] {
    [
        x * i[0] + y * (i[1] - i[2] + i[3]),
        x * i[1] + y * (-i[0] + i[2] + i[3]),
        x * i[2] + y * (i[0] - i[1] + i[3]),
        x * i[3] + y * (-i[0] - i[1] - i[2]),
    ]
}

/// `pow(0.001, length / decay)`.
fn decay_coeff(length: f32, decay: f32) -> f32 {
    DECAY_GAIN.powf(length / decay)
}

fn decay_length(coeff: f32, decay: f32) -> f32 {
    coeff.log10() * decay / -3.0
}

fn density_gain(a: f32) -> f32 {
    (1.0 - a * a).sqrt()
}

fn matrix_coeffs(diffusion: f32) -> (f32, f32) {
    let n = 3f32.sqrt();
    let t = diffusion * n.atan();
    (t.cos(), t.sin() / n)
}

fn limited_hf_ratio(hf_ratio: f32, air_gain_hf: f32, decay: f32) -> f32 {
    let limit = 1.0 / SPEED_OF_SOUND / decay_length(air_gain_hf, decay);
    limit.min(hf_ratio)
}

fn delay_length_mult(density: f32) -> f32 {
    (density * DENSITY_SCALE).cbrt().max(1.0)
}

/// `GetTransformFromVector`: B-format focus matrix for a pan vector.
fn transform_from_vector(v: [f32; 3]) -> [[f32; 4]; 4] {
    let s3 = 3f32.sqrt();
    let mut mag = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let norm = if mag > 1.0 {
        let scale = s3 / mag;
        mag = 1.0;
        [-v[0] * scale, v[1] * scale, v[2] * scale]
    } else {
        [-v[0] * s3, v[1] * s3, v[2] * s3]
    };
    [
        [1.0, norm[0], norm[1], norm[2]],
        [0.0, 1.0 - mag, 0.0, 0.0],
        [0.0, 0.0, 1.0 - mag, 0.0],
        [0.0, 0.0, 0.0, 1.0 - mag],
    ]
}

/// Per-line stereo gains: A→B conversion, pan transform, stereo decode.
fn line_gains(a2b: &[[f32; 4]; 4], pan: [f32; 3], gain: f32) -> [[f32; 2]; 4] {
    let m = transform_from_vector(pan);
    let mut out = [[0.0; 2]; 4];
    for (i, o) in out.iter_mut().enumerate() {
        // ACN coefficients of A-line i after panning.
        let mut acn = [0.0f32; 4];
        for j in 0..4 {
            let a = a2b[j][i];
            for k in 0..4 {
                acn[k] += a * m[j][k];
            }
        }
        for (side, dec) in STEREO_DECODER.iter().enumerate() {
            o[side] = gain * (dec[0] * acn[0] + dec[1] * acn[1] + dec[2] * acn[3]);
        }
    }
    out
}

// --- Pipeline ---------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
struct T60 {
    mid: f32,
    hf: Biquad,
    lf: Biquad,
}

impl T60 {
    fn calc(&mut self, length: f32, lf_decay: f32, mf_decay: f32, hf_decay: f32, lf0: f32, hf0: f32) {
        let mf = decay_coeff(length, mf_decay);
        let lf = decay_coeff(length, lf_decay) / mf;
        let hf = decay_coeff(length, hf_decay) / mf;
        self.mid = mf;
        self.lf.set_shelf(false, lf0, lf, 1.0);
        self.hf.set_shelf(true, hf0, hf, 1.0);
    }
}

/// A value that glides to its target over [`FADE_SAMPLES`].
#[derive(Clone, Copy, Debug, Default)]
struct Glide {
    cur: f32,
    target: f32,
    step: f32,
}

impl Glide {
    fn set(&mut self, t: f32, instant: bool) {
        self.target = t;
        if instant {
            self.cur = t;
            self.step = 0.0;
        } else {
            self.step = (t - self.cur) / FADE_SAMPLES as f32;
        }
    }
    #[inline]
    fn next(&mut self) -> f32 {
        if self.step != 0.0 {
            self.cur += self.step;
            if (self.step > 0.0 && self.cur >= self.target) || (self.step < 0.0 && self.cur <= self.target) {
                self.cur = self.target;
                self.step = 0.0;
            }
        }
        self.cur
    }
}

/// A delay tap that cross-fades from its old to its new length.
#[derive(Clone, Copy, Debug, Default)]
struct Tap {
    old: usize,
    new: usize,
    left: usize,
}

impl Tap {
    fn set(&mut self, d: usize, instant: bool) {
        if instant {
            self.old = d;
            self.new = d;
            self.left = 0;
        } else if d != self.new {
            self.old = self.new;
            self.new = d;
            self.left = FADE_SAMPLES;
        }
    }
    /// `(old, new, weight of new)`.
    #[inline]
    fn next(&mut self) -> (usize, usize, f32) {
        if self.left == 0 {
            return (self.new, self.new, 1.0);
        }
        let w = 1.0 - self.left as f32 / FADE_SAMPLES as f32;
        self.left -= 1;
        if self.left == 0 {
            self.old = self.new;
        }
        (self.old, self.new, w)
    }
}

#[derive(Clone, Debug)]
struct Pipeline {
    lp: [Biquad; 4],
    hp: [Biquad; 4],
    late_in: Lines,
    early_tap: [Tap; 4],
    early_coeff: Glide,
    late_tap: [Tap; 4],
    mix_x: f32,
    mix_y: f32,
    // Early reflections.
    e_ap: Lines,
    e_ap_off: [usize; 4],
    e_ap_coeff: f32,
    e_delay: Lines,
    e_off: [usize; 4],
    e_coeff: f32,
    e_gains: [[Glide; 2]; 4],
    // Late reverb.
    l_delay: Lines,
    l_off: [usize; 4],
    l_density: f32,
    t60: [T60; 4],
    mod_index: u32,
    mod_step: u32,
    mod_depth: f32,
    l_ap: Lines,
    l_ap_off: [usize; 4],
    l_ap_coeff: f32,
    l_gains: [[Glide; 2]; 4],
    fade_samples: usize,
    fresh: bool,
}

impl Pipeline {
    fn new(rate: f32) -> Self {
        let m = delay_length_mult(1.0);
        let late_diff = (LATE_LINE_LENGTHS[3] - LATE_LINE_LENGTHS[0]) / NUM_LINES as f32;
        let max_mod_delay = MAX_MODULATION_TIME * MODULATION_DEPTH_COEFF / 2.0;
        Self {
            lp: [Biquad::default(); 4],
            hp: [Biquad::default(); 4],
            late_in: Lines::new(MAX_LATE_REVERB_DELAY + late_diff * m, rate, FADE_SAMPLES),
            early_tap: [Tap::default(); 4],
            early_coeff: Glide::default(),
            late_tap: [Tap::default(); 4],
            mix_x: 1.0,
            mix_y: 0.0,
            e_ap: Lines::new(EARLY_ALLPASS_LENGTHS[3] * m, rate, 0),
            e_ap_off: [1; 4],
            e_ap_coeff: 0.0,
            e_delay: Lines::new(EARLY_LINE_LENGTHS[3] * m, rate, 1),
            e_off: [1; 4],
            e_coeff: 0.0,
            e_gains: [[Glide::default(); 2]; 4],
            l_delay: Lines::new(LATE_LINE_LENGTHS[3] * m + max_mod_delay, rate, 4),
            l_off: [1; 4],
            l_density: 0.0,
            t60: [T60::default(); 4],
            mod_index: 0,
            mod_step: 1,
            mod_depth: 0.0,
            l_ap: Lines::new(LATE_ALLPASS_LENGTHS[3] * m, rate, 1),
            l_ap_off: [1; 4],
            l_ap_coeff: 0.0,
            l_gains: [[Glide::default(); 2]; 4],
            fade_samples: 1,
            fresh: true,
        }
    }

    fn clear(&mut self) {
        for f in self.lp.iter_mut().chain(self.hp.iter_mut()) {
            f.clear();
        }
        self.late_in.clear();
        self.e_ap.clear();
        self.e_delay.clear();
        self.l_delay.clear();
        self.l_ap.clear();
        for t in &mut self.t60 {
            t.hf.clear();
            t.lf.clear();
        }
        self.mod_index = 0;
        for g in self.e_gains.iter_mut().chain(self.l_gains.iter_mut()) {
            for s in g.iter_mut() {
                s.set(0.0, true);
            }
        }
        self.early_coeff.set(0.0, true);
        self.fresh = true;
    }

    fn update_delay_line(&mut self, gain: f32, early: f32, late: f32, dm: f32, rate: f32) {
        let instant = self.fresh;
        self.early_coeff.set(gain, instant);
        for j in 0..NUM_LINES {
            let d = ((EARLY_TAP_LENGTHS[j] * dm + early) * rate) as usize;
            self.early_tap[j].set(d, instant);
            let len = (LATE_LINE_LENGTHS[j] - LATE_LINE_LENGTHS[0]) / NUM_LINES as f32 * dm + late;
            self.late_tap[j].set((len * rate) as usize, instant);
        }
    }

    fn update_panning(&mut self, early_pan: [f32; 3], late_pan: [f32; 3], eg: f32, lg: f32) {
        let instant = self.fresh;
        let e = line_gains(&EARLY_A2B, early_pan, eg);
        let l = line_gains(&LATE_A2B, late_pan, lg);
        for i in 0..NUM_LINES {
            for s in 0..2 {
                self.e_gains[i][s].set(e[i][s], instant);
                self.l_gains[i][s].set(l[i][s], instant);
            }
        }
    }

    fn update_early(&mut self, dm: f32, diffusion: f32, decay: f32, rate: f32) {
        self.e_ap_coeff = diffusion * diffusion * INV_SQRT2;
        for j in 0..NUM_LINES {
            self.e_ap_off[j] = ((EARLY_ALLPASS_LENGTHS[j] * dm * rate) as usize).max(1);
            self.e_off[j] = ((EARLY_LINE_LENGTHS[j] * dm * rate) as usize).max(1);
        }
        let len = EARLY_LINE_LENGTHS.iter().sum::<f32>() / NUM_LINES as f32 * dm;
        self.e_coeff = decay_coeff(len, decay);
    }

    fn update_modulator(&mut self, time: f32, depth: f32, rate: f32) {
        self.mod_step = ((MOD_FRACONE as f32 / (rate * time)) as u32).max(1);
        self.mod_depth = if time >= DEFAULT_MODULATION_TIME {
            MODULATION_DEPTH_COEFF / 4.0 * DEFAULT_MODULATION_TIME * depth * rate
        } else {
            MODULATION_DEPTH_COEFF / 4.0 * time * depth * rate
        };
    }

    #[allow(clippy::too_many_arguments)]
    fn update_late(
        &mut self,
        dm: f32,
        diffusion: f32,
        lf_decay: f32,
        mf_decay: f32,
        hf_decay: f32,
        lf0: f32,
        hf0: f32,
        rate: f32,
    ) {
        let w = rate / MAX_HF_REFERENCE;
        let weighted = lf0 * w * lf_decay + (hf0 - lf0) * w * mf_decay + (1.0 - hf0 * w) * hf_decay;
        self.l_density = density_gain(decay_coeff(late_delay_average() * dm, weighted));
        self.l_ap_coeff = diffusion * diffusion * INV_SQRT2;
        let ap_avg = late_allpass_average();
        for j in 0..NUM_LINES {
            self.l_ap_off[j] = ((LATE_ALLPASS_LENGTHS[j] * dm * rate) as usize).max(1);
            let len = LATE_LINE_LENGTHS[j] * dm;
            self.l_off[j] = ((len * rate + 0.5) as usize).max(1) - 1;
            let ap = LATE_ALLPASS_LENGTHS[j] + (ap_avg - LATE_ALLPASS_LENGTHS[j]) * diffusion;
            let total = ap * dm + self.mod_depth / rate + len;
            self.t60[j].calc(total, lf_decay, mf_decay, hf_decay, lf0, hf0);
        }
        // Keep the read behind the write (one cubic sample of look-behind).
        for o in &mut self.l_off {
            *o = (*o).max(1);
        }
    }

    /// One sample through the early and late stages; returns stereo.
    #[inline]
    fn process(&mut self, main: &Lines, off: usize, cubic: &CubicTable) -> (f32, f32) {
        // Primary reflections from the main delay line, band-shelved.
        let coeff = self.early_coeff.next();
        let mut x = [0.0f32; 4];
        for (j, xj) in x.iter_mut().enumerate() {
            let (t0, t1, w) = self.early_tap[j].next();
            let a = main.get(j, off.wrapping_sub(t0));
            let b = main.get(j, off.wrapping_sub(t1));
            let v = (a + (b - a) * w) * coeff;
            let v = self.lp[j].process(v);
            *xj = self.hp[j].process(v);
        }
        // 4-line all-pass.
        for (j, xj) in x.iter_mut().enumerate() {
            let y = self.e_ap.get(j, off.wrapping_sub(self.e_ap_off[j])) - self.e_ap_coeff * *xj;
            self.e_ap.set(j, off, *xj + self.e_ap_coeff * y);
            *xj = y;
        }
        // Secondary reflections: reflected write, delayed read.
        let refl = [
            (x[0] - x[1] - x[2] - x[3]) * 0.5,
            (x[1] - x[0] - x[2] - x[3]) * 0.5,
            (x[2] - x[0] - x[1] - x[3]) * 0.5,
            (x[3] - x[0] - x[1] - x[2]) * 0.5,
        ];
        let mut early = [0.0f32; 4];
        for j in 0..NUM_LINES {
            self.e_delay.set(j, off, refl[j]);
            early[j] = self.e_delay.get(j, off.wrapping_sub(self.e_off[j])) * self.e_coeff + x[j];
        }
        // Scatter into the late input line.
        let s = scatter(x, self.mix_x, self.mix_y);
        for (j, v) in s.iter().enumerate() {
            self.late_in.set(j, off, *v);
        }

        // Late reverb: modulated feedback read.
        let xf = (self.mod_index & MOD_FRACMASK) as f32 * (1.0 / MOD_FRACONE as f32);
        let lfo = if self.mod_index & (MOD_FRACONE >> 1) == 0 {
            -16.0 * xf * xf + 8.0 * xf
        } else {
            16.0 * xf * xf - 8.0 * xf - 16.0 * xf + 8.0
        };
        self.mod_index = self.mod_index.wrapping_add(self.mod_step);
        let idelay = ((lfo + 1.0) * self.mod_depth * CUBIC_STEPS as f32).max(0.0) as usize;
        let whole = idelay >> CUBIC_BITS;
        let frac = idelay & (CUBIC_STEPS - 1);
        let mut late = [0.0f32; 4];
        for (j, lj) in late.iter_mut().enumerate() {
            let tap = off.wrapping_sub(self.l_off[j]).wrapping_sub(whole);
            let o0 = self.l_delay.get(j, tap);
            let o1 = self.l_delay.get(j, tap.wrapping_sub(1));
            let o2 = self.l_delay.get(j, tap.wrapping_sub(2));
            let o3 = self.l_delay.get(j, tap.wrapping_sub(3));
            let v = (o0 * cubic.c0(frac) + o1 * cubic.c1(frac) + o2 * cubic.c2(frac) + o3 * cubic.c3(frac))
                * self.t60[j].mid;
            let v = self.t60[j].hf.process(v);
            *lj = self.t60[j].lf.process(v);
        }
        // Plus the delayed early output.
        for (j, lj) in late.iter_mut().enumerate() {
            let (t0, t1, w) = self.late_tap[j].next();
            let a = self.late_in.get(j, off.wrapping_sub(t0));
            let b = self.late_in.get(j, off.wrapping_sub(t1));
            *lj += (a + (b - a) * w) * self.l_density;
        }
        // Vector all-pass, then scatter the stored inputs.
        for (j, lj) in late.iter_mut().enumerate() {
            let y = self.l_ap.get(j, off.wrapping_sub(self.l_ap_off[j])) - self.l_ap_coeff * *lj;
            self.l_ap.set(j, off, *lj + self.l_ap_coeff * y);
            *lj = y;
        }
        let stored = [
            self.l_ap.get(0, off),
            self.l_ap.get(1, off),
            self.l_ap.get(2, off),
            self.l_ap.get(3, off),
        ];
        let sc = scatter(stored, self.mix_x, self.mix_y);
        for (j, v) in sc.iter().enumerate() {
            self.l_ap.set(j, off, *v);
        }
        // Refeed: reversed lines, scattered.
        let fb = scatter([late[3], late[2], late[1], late[0]], self.mix_x, self.mix_y);
        for (j, v) in fb.iter().enumerate() {
            self.l_delay.set(j, off, *v);
        }

        let mut out = (0.0f32, 0.0f32);
        for j in 0..NUM_LINES {
            let (el, er) = (self.e_gains[j][0].next(), self.e_gains[j][1].next());
            let (ll, lr) = (self.l_gains[j][0].next(), self.l_gains[j][1].next());
            out.0 += early[j] * el + late[j] * ll;
            out.1 += early[j] * er + late[j] * lr;
        }
        out
    }
}

/// OpenAL Soft's EAX reverb for one sound environment (B-format in, stereo out).
#[derive(Clone)]
pub struct EfxReverb {
    rate: f32,
    params: Option<EfxParams>,
    /// Values that force a full update (and pipeline cross-fade) when changed.
    full_key: [f32; 9],
    main: Lines,
    pipes: [Pipeline; 2],
    cur: usize,
    /// Samples left until the old pipeline is cleared (`Fading`).
    old_left: usize,
    off: usize,
    cubic: CubicTable,
}

impl std::fmt::Debug for EfxReverb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EfxReverb")
            .field("rate", &self.rate)
            .field("params", &self.params)
            .finish_non_exhaustive()
    }
}

impl EfxReverb {
    /// Reverb at `sample_rate`, silent until [`Self::set_params`].
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate.max(8000) as f32;
        let m = delay_length_mult(1.0);
        Self {
            rate,
            params: None,
            full_key: [f32::NAN; 9],
            main: Lines::new(MAX_REFLECTIONS_DELAY + EARLY_TAP_LENGTHS[3] * m, rate, FADE_SAMPLES),
            pipes: [Pipeline::new(rate), Pipeline::new(rate)],
            cur: 0,
            old_left: 0,
            off: 0,
            cubic: CubicTable::new(),
        }
    }

    /// Current parameters.
    #[must_use]
    pub fn params(&self) -> Option<&EfxParams> {
        self.params.as_ref()
    }

    /// `ReverbState::update`.
    pub fn set_params(&mut self, p: &EfxParams) {
        let rate = self.rate;
        let mut hf_ratio = p.decay_hf_ratio;
        if p.decay_hf_limit && p.air_absorption_gain_hf < 1.0 {
            hf_ratio = limited_hf_ratio(hf_ratio, p.air_absorption_gain_hf, p.decay_time);
        }
        let lf_decay = (p.decay_time * p.decay_lf_ratio).clamp(MIN_DECAY_TIME, MAX_DECAY_TIME);
        let hf_decay = (p.decay_time * hf_ratio).clamp(MIN_DECAY_TIME, MAX_DECAY_TIME);
        let key = [
            p.density,
            p.diffusion,
            p.decay_time,
            hf_decay,
            lf_decay,
            p.modulation_time,
            p.modulation_depth,
            p.hf_reference,
            p.lf_reference,
        ];
        let full = key != self.full_key;
        if full {
            self.full_key = key;
            if self.params.is_some() {
                // Fade the old pipeline out over its decay, start a fresh one.
                let old = self.cur;
                self.pipes[old].early_coeff.set(0.0, false);
                self.old_left = self.pipes[old].fade_samples.max(FADE_SAMPLES);
                self.cur = 1 - old;
            }
            self.pipes[self.cur].clear();
        }
        self.params = Some(*p);
        let dm = delay_length_mult(p.density);
        let pipe = &mut self.pipes[self.cur];
        pipe.update_delay_line(p.gain, p.reflections_delay, p.late_reverb_delay, dm, rate);
        pipe.update_panning(
            p.reflections_pan,
            p.late_reverb_pan,
            p.reflections_gain,
            p.late_reverb_gain,
        );
        let hf0 = (p.hf_reference / rate).min(0.49);
        let lf0 = (p.lf_reference / rate).min(0.49);
        pipe.lp[0].set_shelf(true, hf0, p.gain_hf, 1.0);
        pipe.hp[0].set_shelf(false, lf0, p.gain_lf, 1.0);
        let (lp0, hp0) = (pipe.lp[0], pipe.hp[0]);
        for j in 1..NUM_LINES {
            pipe.lp[j].copy_params(&lp0);
            pipe.hp[j].copy_params(&hp0);
        }
        if full {
            pipe.update_early(dm, p.diffusion, p.decay_time, rate);
            let (x, y) = matrix_coeffs(p.diffusion);
            pipe.mix_x = x;
            pipe.mix_y = y;
            pipe.update_modulator(p.modulation_time, p.modulation_depth, rate);
            pipe.update_late(dm, p.diffusion, lf_decay, p.decay_time, hf_decay, lf0, hf0, rate);
        }
        // Time for the late reverb to reach −60 dB, for the next fade-out.
        let base = p.gain * p.late_reverb_gain;
        let diff = DECAY_GAIN / base.max(DECAY_GAIN);
        let diff_time = if diff < 1.0 {
            diff.log10() * (20.0 / -60.0) * p.decay_time
        } else {
            0.0
        };
        let samples = (p.reflections_delay + p.late_reverb_delay + diff_time) * rate;
        pipe.fade_samples = samples.min(100_000.0) as usize;
        pipe.fresh = false;
    }

    /// Clear every tail.
    pub fn reset(&mut self) {
        self.main.clear();
        for p in &mut self.pipes {
            p.clear();
        }
        self.old_left = 0;
        if let Some(p) = self.params {
            self.full_key = [f32::NAN; 9];
            self.params = None;
            self.set_params(&p);
        }
    }

    /// One frame of first-order B-format send `[W, Y, Z, X]` (N3D, as OpenAL's
    /// effect slots take it); returns the wet stereo frame.
    #[inline]
    pub fn process_bformat(&mut self, b: [f32; 4]) -> (f32, f32) {
        let off = self.off;
        for c in 0..NUM_LINES {
            let v = B2A[c][0] * b[0] + B2A[c][1] * b[1] + B2A[c][2] * b[2] + B2A[c][3] * b[3];
            self.main.set(c, off, v);
        }
        let (mut l, mut r) = self.pipes[self.cur].process(&self.main, off, &self.cubic);
        if self.old_left > 0 {
            let old = 1 - self.cur;
            let (ol, or) = self.pipes[old].process(&self.main, off, &self.cubic);
            l += ol;
            r += or;
            self.old_left -= 1;
            if self.old_left == 0 {
                self.pipes[old].clear();
            }
        }
        self.off = off.wrapping_add(1);
        (l, r)
    }
}

/// N3D first-order send coefficients for a source in direction `(front, left,
/// up)` relative to the listener (unit length, or zero for a sound with no
/// position: then only W).
#[must_use]
pub fn bformat_coeffs(front: f32, left: f32, up: f32) -> [f32; 4] {
    let s3 = 3f32.sqrt();
    [1.0, s3 * left, s3 * up, s3 * front]
}

/// OpenAL Soft's distance shaping of a reverb send (`alu.cpp`, `WetGainAuto`):
/// with a room rolloff of 0 (UZDoom), the send keeps the source gain but
/// decays toward the dry attenuation over `decay_time · 343.3` units.
/// `dry` is the direct path's distance attenuation (0–1) at `distance`.
#[must_use]
pub fn send_distance_gain(distance: f32, dry: f32, decay_time: f32) -> f32 {
    let dry = dry.clamp(0.0, 1.0);
    // UZDoom's manual rolloff sets the reference distance to `dry · distance`.
    let past_ref = distance * (1.0 - dry);
    if past_ref <= 0.0 || decay_time <= 0.0 {
        return 1.0;
    }
    let fact = past_ref / (decay_time * SPEED_OF_SOUND);
    DECAY_GAIN.powf(fact) * (1.0 - dry) + dry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reverb::{ENV_OFF, EnvironmentTable};

    fn params(id: u16) -> EfxParams {
        EfxParams::from_props(&EnvironmentTable::builtin_only().find(id).unwrap().props)
    }

    /// Impulse response (W-only impulse) for `seconds`.
    fn impulse(p: &EfxParams, seconds: f32) -> Vec<(f32, f32)> {
        let mut r = EfxReverb::new(44_100);
        r.set_params(p);
        let n = (seconds * 44_100.0) as usize;
        (0..n)
            .map(|i| r.process_bformat(if i == 0 { [1.0, 0.0, 0.0, 0.0] } else { [0.0; 4] }))
            .collect()
    }

    /// Schroeder backward-integrated decay; returns the T60 extrapolated from
    /// the −5 … −25 dB slope (T20 × 3).
    fn t60(ir: &[(f32, f32)]) -> f32 {
        let e: Vec<f64> = ir.iter().map(|(l, r)| f64::from(l * l + r * r)).collect();
        let mut tail = vec![0.0f64; e.len() + 1];
        for i in (0..e.len()).rev() {
            tail[i] = tail[i + 1] + e[i];
        }
        let total = tail[0];
        let db = |i: usize| 10.0 * (tail[i] / total).log10();
        let t5 = (0..e.len()).find(|&i| db(i) <= -5.0).unwrap();
        let t25 = (0..e.len()).find(|&i| db(i) <= -25.0).unwrap();
        ((t25 - t5) as f32 / 44_100.0) * 3.0
    }

    #[test]
    fn impulse_response_decays_at_the_preset_decay_time() {
        // Generic (1.49 s), Cave (2.91 s), Arena (7.24 s).
        // Flat HF/LF ratios so the broadband T60 is the mid decay time (a
        // ratio above 1 lengthens the HF tail by more than the ratio: OpenAL
        // Soft's shelf reaches `gain²`, see `shelf_reaches_gain_squared`).
        for (id, want) in [(0x0001, 1.49f32), (0x0800, 2.91), (0x0900, 7.24)] {
            let mut p = params(id);
            assert!((p.decay_time - want).abs() < 0.01, "{id:#x}: {}", p.decay_time);
            p.decay_hf_ratio = 1.0;
            p.decay_lf_ratio = 1.0;
            let ir = impulse(&p, want * 2.5);
            let got = t60(&ir);
            assert!(
                (got / want - 1.0).abs() < 0.15,
                "{id:#x}: measured T60 {got:.2} s vs {want} s"
            );
            assert!(ir.iter().all(|(l, r)| l.is_finite() && r.is_finite()));
        }
    }

    #[test]
    fn reflections_arrive_after_their_delay_and_off_is_silent() {
        let mut p = params(0x0001);
        p.reflections_delay = 0.1;
        let ir = impulse(&p, 0.3);
        let first = ir.iter().position(|(l, r)| l.abs() + r.abs() > 1e-6).unwrap();
        let at = first as f32 / 44_100.0;
        assert!((0.099..0.11).contains(&at), "first output at {at} s");
        let off = impulse(&params(ENV_OFF), 0.5);
        let peak = off.iter().map(|(l, r)| l.abs().max(r.abs())).fold(0.0, f32::max);
        assert!(peak < 1e-3, "Off peak {peak}");
    }

    #[test]
    fn pan_vectors_steer_the_reflections() {
        let mut right = params(0x0001);
        right.reflections_pan = [1.0, 0.0, 0.0];
        right.late_reverb_pan = [1.0, 0.0, 0.0];
        let ir = impulse(&right, 1.0);
        let (el, er) = ir.iter().fold((0.0, 0.0), |a, (l, r)| (a.0 + l * l, a.1 + r * r));
        assert!(er > el * 4.0, "right {er} vs left {el}");
        let centre = impulse(&params(0x0001), 1.0);
        let (cl, cr) = centre.iter().fold((0.0, 0.0), |a, (l, r)| (a.0 + l * l, a.1 + r * r));
        assert!((cl / cr - 1.0).abs() < 0.5, "centred {cl} vs {cr}");
    }

    #[test]
    fn hf_and_lf_ratios_shape_the_tail() {
        // Band energy of the tail with a sine burst at `hz`.
        let tail = |p: &EfxParams, hz: f32| {
            let mut r = EfxReverb::new(44_100);
            r.set_params(p);
            let mut e = 0.0;
            for i in 0..88_200 {
                let x = if i < 4410 {
                    (2.0 * PI * hz * i as f32 / 44_100.0).sin()
                } else {
                    0.0
                };
                let (l, rr) = r.process_bformat([x, 0.0, 0.0, 0.0]);
                if i > 44_100 {
                    e += l * l + rr * rr;
                }
            }
            e
        };
        let mut dark = params(0x0001);
        dark.decay_hf_ratio = 0.2;
        let mut bright = dark;
        bright.decay_hf_ratio = 1.5;
        assert!(tail(&bright, 8000.0) > tail(&dark, 8000.0) * 10.0);
        let mut boomy = params(0x0001);
        boomy.decay_lf_ratio = 2.0;
        let mut thin = boomy;
        thin.decay_lf_ratio = 0.3;
        assert!(tail(&boomy, 80.0) > tail(&thin, 80.0) * 10.0);
    }

    #[test]
    fn modulation_changes_the_tail_and_changing_environment_crossfades() {
        let mut still = params(0x0001);
        still.modulation_depth = 0.0;
        let mut wobbly = still;
        wobbly.modulation_depth = 1.0;
        wobbly.modulation_time = 0.1;
        let a = impulse(&still, 1.0);
        let b = impulse(&wobbly, 1.0);
        let diff: f32 = a.iter().zip(&b).map(|(x, y)| (x.0 - y.0).abs()).sum();
        assert!(diff > 1e-3, "modulation made no difference");
        // Switch Generic → Hangar mid-tail: no click (bounded step), no NaN.
        let mut r = EfxReverb::new(44_100);
        r.set_params(&params(0x0001));
        let mut prev = 0.0f32;
        let mut max_step = 0.0f32;
        for i in 0..44_100 {
            if i == 10_000 {
                r.set_params(&params(0x0A00));
            }
            let x = if i % 2205 == 0 { 0.5 } else { 0.0 };
            let (l, _) = r.process_bformat([x, 0.0, 0.0, 0.0]);
            assert!(l.is_finite());
            if i > 9_000 {
                max_step = max_step.max((l - prev).abs());
            }
            prev = l;
        }
        assert!(max_step < 0.5, "step {max_step}");
    }

    #[test]
    fn send_gain_follows_openal_distance_shaping() {
        assert!((send_distance_gain(0.0, 1.0, 1.49) - 1.0).abs() < 1e-6);
        // Far and quiet on the dry path: the send decays toward the dry gain.
        let near = send_distance_gain(100.0, 0.9, 1.49);
        let far = send_distance_gain(1000.0, 0.1, 1.49);
        assert!(near > far && far > 0.1, "near {near} far {far}");
        // A long decay keeps distant sends louder.
        assert!(send_distance_gain(1000.0, 0.1, 10.0) > far);
    }

    #[test]
    fn shelf_reaches_gain_squared() {
        // Steady-state magnitude of a shelf at DC and Nyquist.
        let mag = |high: bool, at_nyquist: bool| {
            let mut f = Biquad::default();
            f.set_shelf(high, 0.1, 0.5, 1.0);
            let mut peak = 0.0f32;
            for i in 0..4000 {
                let x = if at_nyquist { if i % 2 == 0 { 1.0 } else { -1.0 } } else { 1.0 };
                let y = f.process(x);
                if i > 3000 {
                    peak = peak.max(y.abs());
                }
            }
            peak
        };
        assert!((mag(true, true) - 0.25).abs() < 0.01);
        assert!((mag(true, false) - 1.0).abs() < 0.01);
        assert!((mag(false, false) - 0.25).abs() < 0.01);
        assert!((mag(false, true) - 1.0).abs() < 0.01);
    }
}
