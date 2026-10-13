//! 16.16 fixed point for vanilla / `compat_strict` demo paths.

/// Fractional bits in [`Fixed`].
pub const FRACBITS: i32 = 16;
/// `1.0` as [`Fixed`].
pub const FRACUNIT: i32 = 1 << FRACBITS;

/// 16.16 signed fixed point. Equivalent of Doom `fixed_t`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed(pub i32);

impl Fixed {
    /// Zero.
    pub const ZERO: Self = Self(0);
    /// One.
    pub const ONE: Self = Self(FRACUNIT);

    /// From an integer (no fractional part).
    #[must_use]
    pub const fn from_int(i: i32) -> Self {
        Self(i.saturating_mul(FRACUNIT))
    }

    /// From `f64` sim units (truncates toward zero after scale).
    #[must_use]
    pub fn from_f64(v: f64) -> Self {
        let scaled = v * f64::from(FRACUNIT);
        Fixed(crate::conv::trunc_to_int(scaled))
    }

    /// To `f64` sim units.
    #[must_use]
    pub fn to_f64(self) -> f64 {
        f64::from(self.0) / f64::from(FRACUNIT)
    }

    /// Integer part toward zero.
    #[must_use]
    pub const fn to_int(self) -> i32 {
        self.0 / FRACUNIT
    }

    /// Absolute value (wrapping negate of negatives).
    #[must_use]
    pub const fn abs(self) -> Self {
        if self.0 < 0 {
            Self(self.0.wrapping_neg())
        } else {
            self
        }
    }

    /// Doom `FixedMul`: `(a * b) >> FRACBITS` with 64-bit intermediate.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub const fn mul(self, rhs: Self) -> Self {
        Self(((self.0 as i64 * rhs.0 as i64) >> FRACBITS) as i32)
    }
}

impl core::ops::Add for Fixed {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl core::ops::Sub for Fixed {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl core::ops::Neg for Fixed {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

impl core::ops::Mul for Fixed {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Fixed::mul(self, rhs)
    }
}

impl core::ops::Div for Fixed {
    type Output = Self;
    /// Doom `FixedDiv` (clamped; max i32 on overflow / zero divisor).
    #[allow(clippy::cast_possible_truncation)]
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 {
            return if self.0 < 0 {
                Self(i32::MIN)
            } else {
                Self(i32::MAX)
            };
        }
        let a = i64::from(self.0) << FRACBITS;
        let q = a / i64::from(rhs.0);
        Self(q.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_roundtrip() {
        assert_eq!(Fixed::from_int(3).to_int(), 3);
        assert!((Fixed::from_f64(1.5).to_f64() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn fixed_mul_div() {
        assert_eq!(Fixed::from_int(2).mul(Fixed::from_int(3)).to_int(), 6);
        assert_eq!((Fixed::from_int(6) / Fixed::from_int(2)).to_int(), 3);
        assert_eq!((Fixed::from_f64(1.5) * Fixed::from_f64(2.0)).to_f64(), 3.0);
    }
}
