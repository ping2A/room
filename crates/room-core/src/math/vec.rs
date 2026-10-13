//! `DVec2` / `DVec3` as `[f64; N]` wrappers with explicit ops (no FMA).

use crate::dmath;
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// 2D vector of sim coordinates (`double` in GZDoom).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DVec2(pub [f64; 2]);

/// 3D vector of sim coordinates (`DVector3` / `DVector` in GZDoom).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DVec3(pub [f64; 3]);

impl DVec2 {
    /// Origin.
    pub const ZERO: Self = Self([0.0, 0.0]);

    /// Construct from components.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self([x, y])
    }

    /// X component.
    #[must_use]
    pub const fn x(self) -> f64 {
        self.0[0]
    }

    /// Y component.
    #[must_use]
    pub const fn y(self) -> f64 {
        self.0[1]
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, rhs: Self) -> f64 {
        self.0[0] * rhs.0[0] + self.0[1] * rhs.0[1]
    }

    /// Euclidean length via [`dmath::sqrt`](crate::dmath::sqrt).
    #[must_use]
    pub fn length(self) -> f64 {
        dmath::sqrt(self.length_squared())
    }

    /// Squared length.
    #[must_use]
    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    /// Unit vector, or `ZERO` if length is 0.
    #[must_use]
    pub fn normalized(self) -> Self {
        let len = self.length();
        if len == 0.0 { Self::ZERO } else { self / len }
    }
}

impl DVec3 {
    /// Origin.
    pub const ZERO: Self = Self([0.0, 0.0, 0.0]);

    /// Construct from components.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self([x, y, z])
    }

    /// X component.
    #[must_use]
    pub const fn x(self) -> f64 {
        self.0[0]
    }

    /// Y component.
    #[must_use]
    pub const fn y(self) -> f64 {
        self.0[1]
    }

    /// Z component.
    #[must_use]
    pub const fn z(self) -> f64 {
        self.0[2]
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, rhs: Self) -> f64 {
        self.0[0] * rhs.0[0] + self.0[1] * rhs.0[1] + self.0[2] * rhs.0[2]
    }

    /// Cross product.
    #[must_use]
    pub fn cross(self, rhs: Self) -> Self {
        Self([
            self.0[1] * rhs.0[2] - self.0[2] * rhs.0[1],
            self.0[2] * rhs.0[0] - self.0[0] * rhs.0[2],
            self.0[0] * rhs.0[1] - self.0[1] * rhs.0[0],
        ])
    }

    /// Euclidean length via [`dmath::sqrt`](crate::dmath::sqrt).
    #[must_use]
    pub fn length(self) -> f64 {
        dmath::sqrt(self.length_squared())
    }

    /// Squared length.
    #[must_use]
    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    /// XY projection.
    #[must_use]
    pub const fn xy(self) -> DVec2 {
        DVec2([self.0[0], self.0[1]])
    }
}

macro_rules! impl_arith {
    ($t:ty) => {
        impl Add for $t {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self(core::array::from_fn(|i| self.0[i] + rhs.0[i]))
            }
        }
        impl Sub for $t {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self(core::array::from_fn(|i| self.0[i] - rhs.0[i]))
            }
        }
        impl Mul<f64> for $t {
            type Output = Self;
            fn mul(self, rhs: f64) -> Self {
                Self(core::array::from_fn(|i| self.0[i] * rhs))
            }
        }
        impl Div<f64> for $t {
            type Output = Self;
            fn div(self, rhs: f64) -> Self {
                Self(core::array::from_fn(|i| self.0[i] / rhs))
            }
        }
        impl Neg for $t {
            type Output = Self;
            fn neg(self) -> Self {
                Self(core::array::from_fn(|i| -self.0[i]))
            }
        }
        impl AddAssign for $t {
            fn add_assign(&mut self, rhs: Self) {
                *self = *self + rhs;
            }
        }
        impl SubAssign for $t {
            fn sub_assign(&mut self, rhs: Self) {
                *self = *self - rhs;
            }
        }
        impl MulAssign<f64> for $t {
            fn mul_assign(&mut self, rhs: f64) {
                *self = *self * rhs;
            }
        }
        impl DivAssign<f64> for $t {
            fn div_assign(&mut self, rhs: f64) {
                *self = *self / rhs;
            }
        }
    };
}

impl_arith!(DVec2);
impl_arith!(DVec3);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_mul_no_fma() {
        let a = DVec2::new(1.0, 2.0);
        let b = DVec2::new(3.0, 4.0);
        assert_eq!(a + b, DVec2::new(4.0, 6.0));
        assert_eq!(a * 2.0, DVec2::new(2.0, 4.0));
        assert_eq!(a.dot(b), 11.0);
    }
}
