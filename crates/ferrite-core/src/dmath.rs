//! Portable transcendental math for the playsim.
//!
//! All functions wrap [`libm`] (pure Rust, no target-specific libm). Never call
//! `f64::sin` and friends from `std` in sim code.

use xxhash_rust::xxh3::Xxh3;

/// Sine (radians).
#[must_use]
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

/// Cosine (radians).
#[must_use]
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

/// Tangent (radians).
#[must_use]
pub fn tan(x: f64) -> f64 {
    libm::tan(x)
}

/// `atan2(y, x)`.
#[must_use]
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

/// IEEE square root.
#[must_use]
pub fn sqrt(x: f64) -> f64 {
    libm::sqrt(x)
}

/// `x^y`.
#[must_use]
pub fn pow(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

/// `e^x`.
#[must_use]
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

/// Natural logarithm.
#[must_use]
pub fn log(x: f64) -> f64 {
    libm::log(x)
}

/// Floor toward −∞.
#[must_use]
pub fn floor(x: f64) -> f64 {
    libm::floor(x)
}

/// Round half away from zero (C `round`).
#[must_use]
pub fn round(x: f64) -> f64 {
    libm::round(x)
}

/// Truncate toward zero.
#[must_use]
pub fn trunc(x: f64) -> f64 {
    libm::trunc(x)
}

/// Number of `(sin, cos, tan, atan2, sqrt, pow, exp, log)` samples hashed by
/// [`sample_hash`].
pub const SAMPLE_COUNT: u32 = 1_000_000;

/// Golden xxh3 digest of [`sample_hash`]`(` [`SAMPLE_COUNT`] `)` for `libm` 0.2.
///
/// CI runs this on x86-64, aarch64, and wasm32; a mismatch is a determinism bug.
pub const SAMPLE_HASH: u64 = 0xf641_5625_f9bd_a08f;

/// Hash `count` samples of every required transcendental, little-endian `f64` bytes.
#[must_use]
pub fn sample_hash(count: u32) -> u64 {
    let mut h = Xxh3::new();
    for i in 0..count {
        let t = f64::from(i);
        let x = t * 1.0e-4 - 40.0;
        let y = t * 7.0e-5 - 10.0;
        h.update(&sin(x).to_le_bytes());
        h.update(&cos(x).to_le_bytes());
        // Avoid the exact tan poles at π/2 + kπ; sample a shifted argument.
        h.update(&tan(x * 0.125).to_le_bytes());
        h.update(&atan2(y, x).to_le_bytes());
        h.update(&sqrt(t * 0.5).to_le_bytes());
        h.update(&pow((t + 1.0) * 1.0e-6, 1.25).to_le_bytes());
        h.update(&exp(x * 0.01).to_le_bytes());
        h.update(&log(t + 1.0).to_le_bytes());
    }
    h.digest()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert!((sin(0.0)).abs() < 1e-15);
        assert!((cos(0.0) - 1.0).abs() < 1e-15);
        assert!((sqrt(4.0) - 2.0).abs() < 1e-15);
        assert!((exp(0.0) - 1.0).abs() < 1e-15);
        assert!((log(1.0)).abs() < 1e-15);
        assert!((atan2(0.0, 1.0)).abs() < 1e-15);
    }

    #[test]
    fn dmath_sample_hash_matches_golden() {
        let hash = sample_hash(SAMPLE_COUNT);
        assert_eq!(
            hash, SAMPLE_HASH,
            "dmath sample hash {hash:#x} != golden {SAMPLE_HASH:#x}"
        );
    }
}
