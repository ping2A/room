//! Oklab colour space helpers and PLAYPAL quantize (deterministic via `libm`).

// Oklab matrix coefficients are published to more digits than f32 can hold;
// keep the reference literals for readability / cross-checks.
#![allow(clippy::excessive_precision)]

/// Linear sRGB → Oklab (`L`, `a`, `b`).
#[must_use]
pub fn srgb8_to_oklab(r: u8, g: u8, b: u8) -> [f32; 3] {
    let lr = srgb_u8_to_linear(r);
    let lg = srgb_u8_to_linear(g);
    let lb = srgb_u8_to_linear(b);
    let l = 0.412_221_470_8 * lr + 0.536_332_536_3 * lg + 0.051_445_992_9 * lb;
    let m = 0.211_903_498_2 * lr + 0.680_699_545_1 * lg + 0.107_396_956_6 * lb;
    let s = 0.088_302_461_9 * lr + 0.281_718_837_6 * lg + 0.629_978_700_5 * lb;
    let l_ = cbrtf(l);
    let m_ = cbrtf(m);
    let s_ = cbrtf(s);
    [
        0.210_454_255_3 * l_ + 0.793_617_785_0 * m_ - 0.004_072_046_8 * s_,
        1.977_998_495_1 * l_ - 2.428_592_205_0 * m_ + 0.450_593_709_9 * s_,
        0.025_904_037_1 * l_ + 0.782_771_766_2 * m_ - 0.808_675_766_0 * s_,
    ]
}

/// Oklab → sRGB8 (clamped).
#[must_use]
pub fn oklab_to_srgb8(lab: [f32; 3]) -> [u8; 3] {
    let l_ = lab[0] + 0.396_337_777_4 * lab[1] + 0.215_803_757_3 * lab[2];
    let m_ = lab[0] - 0.105_561_345_8 * lab[1] - 0.063_854_172_8 * lab[2];
    let s_ = lab[0] - 0.089_484_177_5 * lab[1] - 1.291_485_548_0 * lab[2];
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    let lr = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
    let lg = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
    let lb = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s;
    [
        linear_to_srgb_u8(lr),
        linear_to_srgb_u8(lg),
        linear_to_srgb_u8(lb),
    ]
}

fn srgb_u8_to_linear(c: u8) -> f32 {
    let x = f32::from(c) / 255.0;
    if x <= 0.040_45 {
        x / 12.92
    } else {
        libm::powf((x + 0.055) / 1.055, 2.4)
    }
}

fn linear_to_srgb_u8(c: f32) -> u8 {
    let x = c.clamp(0.0, 1.0);
    let s = if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * libm::powf(x, 1.0 / 2.4) - 0.055
    };
    ((s * 255.0) + 0.5) as u8
}

fn cbrtf(x: f32) -> f32 {
    if x <= 0.0 {
        0.0
    } else {
        libm::powf(x, 1.0 / 3.0)
    }
}

/// Nearest PLAYPAL index in Oklab, skipping a transparent sentinel and optional banned range.
#[must_use]
pub fn nearest_oklab(palette: &[u8; 768], r: u8, g: u8, b: u8, skip: &[u8]) -> u8 {
    let target = srgb8_to_oklab(r, g, b);
    let mut best_i = 0u8;
    let mut best_d = f32::MAX;
    for i in 0u16..256 {
        let iu = i as u8;
        if skip.contains(&iu) {
            continue;
        }
        let o = (i as usize) * 3;
        let lab = srgb8_to_oklab(palette[o], palette[o + 1], palette[o + 2]);
        let dr = lab[0] - target[0];
        let da = lab[1] - target[1];
        let db = lab[2] - target[2];
        let d = dr * dr + da * da + db * db;
        if d < best_d {
            best_d = d;
            best_i = iu;
        }
    }
    best_i
}

/// Match face colour stats to the original head (strength in 0..=1).
#[must_use]
pub fn transfer_oklab(
    face: [f32; 3],
    face_mean: [f32; 3],
    face_std: [f32; 3],
    head_mean: [f32; 3],
    head_std: [f32; 3],
    strength: f32,
) -> [f32; 3] {
    let s = strength.clamp(0.0, 1.0);
    let mut out = [0.0; 3];
    for i in 0..3 {
        let fs = if face_std[i].abs() < 1e-6 {
            1.0
        } else {
            face_std[i]
        };
        let hs = if head_std[i].abs() < 1e-6 {
            fs
        } else {
            head_std[i]
        };
        let matched = (face[i] - face_mean[i]) * (hs / fs) + head_mean[i];
        out[i] = face[i] * (1.0 - s) + matched * s;
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn roundtrip_grey() {
        let lab = srgb8_to_oklab(128, 128, 128);
        let rgb = oklab_to_srgb8(lab);
        assert!((i32::from(rgb[0]) - 128).abs() <= 2);
    }

    #[test]
    fn nearest_picks_exact() {
        let mut pal = [0u8; 768];
        pal[3] = 200;
        pal[4] = 50;
        pal[5] = 50;
        assert_eq!(nearest_oklab(&pal, 200, 50, 50, &[]), 1);
    }
}
