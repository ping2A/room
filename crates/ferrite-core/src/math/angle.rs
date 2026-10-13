//! Angles: GZDoom `DAngle` (degrees as `f64`) and vanilla BAM (`u32`).

use crate::dmath;

/// Full circle in degrees.
pub const DEGREES_PER_TURN: f64 = 360.0;

/// `2^32` BAM units per turn (exact in `f64`).
pub const BAM_PER_TURN: f64 = 4_294_967_296.0;

/// Sim angle in degrees. Equivalent of GZDoom `DAngle`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Angle(pub f64);

impl Angle {
    /// Zero degrees.
    pub const ZERO: Self = Self(0.0);
    /// 90 degrees.
    pub const DEG_90: Self = Self(90.0);
    /// 180 degrees.
    pub const DEG_180: Self = Self(180.0);
    /// 270 degrees.
    pub const DEG_270: Self = Self(270.0);

    /// Construct from degrees.
    #[must_use]
    pub const fn from_degrees(deg: f64) -> Self {
        Self(deg)
    }

    /// Degrees, unwrapped.
    #[must_use]
    pub const fn degrees(self) -> f64 {
        self.0
    }

    /// Radians via π from `libm` (`libm::M_PI` through multiplication).
    #[must_use]
    pub fn radians(self) -> f64 {
        self.0 * (core::f64::consts::PI / 180.0)
    }

    /// From radians.
    #[must_use]
    pub fn from_radians(rad: f64) -> Self {
        Self(rad * (180.0 / core::f64::consts::PI))
    }

    /// Wrap into `[0, 360)`.
    #[must_use]
    pub fn normalized(self) -> Self {
        let t = self.0 % DEGREES_PER_TURN;
        if t < 0.0 {
            Self(t + DEGREES_PER_TURN)
        } else {
            Self(t)
        }
    }

    /// Convert to BAM. Exact for multiples of `1/2^32` turns (integer BAM).
    #[must_use]
    pub fn to_bam(self) -> Bam {
        let turns = self.0 / DEGREES_PER_TURN;
        let units = turns * BAM_PER_TURN;
        let wrapped = units.rem_euclid(BAM_PER_TURN);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        Bam(wrapped as u32)
    }

    /// Sine of this angle (degrees) via [`dmath`].
    #[must_use]
    pub fn sin(self) -> f64 {
        dmath::sin(self.radians())
    }

    /// Cosine of this angle via [`dmath`].
    #[must_use]
    pub fn cos(self) -> f64 {
        dmath::cos(self.radians())
    }
}

/// Binary angle measurement (`u32`, `2^32` = 360°). Vanilla / `compat_strict` paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Bam(pub u32);

impl Bam {
    /// Zero.
    pub const ZERO: Self = Self(0);
    /// 90 degrees (`0x4000_0000`).
    pub const DEG_90: Self = Self(0x4000_0000);
    /// 180 degrees.
    pub const DEG_180: Self = Self(0x8000_0000);
    /// 270 degrees.
    pub const DEG_270: Self = Self(0xC000_0000);

    /// Convert to degrees. Exact for every representable BAM (multiples of `1/2^32` turns).
    #[must_use]
    pub fn to_angle(self) -> Angle {
        #[allow(clippy::cast_precision_loss)]
        Angle(f64::from(self.0) * (DEGREES_PER_TURN / BAM_PER_TURN))
    }
}

impl From<Bam> for Angle {
    fn from(value: Bam) -> Self {
        value.to_angle()
    }
}

impl From<Angle> for Bam {
    fn from(value: Angle) -> Self {
        value.to_bam()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bam_roundtrip_cardinals() {
        for &a in &[Angle::ZERO, Angle::DEG_90, Angle::DEG_180, Angle::DEG_270] {
            let b = a.to_bam();
            let back = b.to_angle();
            assert!((back.0 - a.0).abs() < 1e-12, "{back:?} vs {a:?}");
        }
        assert_eq!(Angle::DEG_90.to_bam(), Bam::DEG_90);
        assert_eq!(Angle::DEG_180.to_bam(), Bam::DEG_180);
    }

    #[test]
    fn integer_bam_roundtrip() {
        for step in [1u32, 3, 7, 65537] {
            let mut v = 0u32;
            for _ in 0..64 {
                let bam = Bam(v);
                let ang = bam.to_angle();
                assert_eq!(ang.to_bam(), bam, "bam {v}");
                v = v.wrapping_add(step.wrapping_mul(1_000_003));
            }
        }
    }
}
