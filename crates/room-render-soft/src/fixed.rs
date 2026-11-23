//! Doom 16.16 fixed point (`m_fixed.c`) and the angle constants the
//! renderer uses.

/// `FRACBITS`.
pub const FRACBITS: i32 = 16;
/// `FRACUNIT`.
pub const FRACUNIT: i32 = 1 << FRACBITS;
/// `ANG45`.
pub const ANG45: u32 = 0x2000_0000;
/// `ANG90`.
pub const ANG90: u32 = 0x4000_0000;
/// `ANG180`.
pub const ANG180: u32 = 0x8000_0000;
/// `ANG270`.
pub const ANG270: u32 = 0xc000_0000;
/// `ANGLETOFINESHIFT`.
pub const ANGLETOFINESHIFT: u32 = 19;
/// `FINEANGLES`.
pub const FINEANGLES: usize = 8192;

/// `FixedMul`.
#[inline]
#[must_use]
pub fn fixed_mul(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> FRACBITS) as i32
}

/// `FixedDiv` (saturating like Chocolate Doom's C version).
#[inline]
#[must_use]
pub fn fixed_div(a: i32, b: i32) -> i32 {
    if (a.unsigned_abs() >> 14) >= b.unsigned_abs() {
        if (a ^ b) < 0 { i32::MIN } else { i32::MAX }
    } else {
        ((i64::from(a) << 16) / i64::from(b)) as i32
    }
}

/// `finesine[i]`.
#[inline]
#[must_use]
pub fn finesine(i: usize) -> i32 {
    room_core::math::FINESINE[i]
}

/// `finecosine[i]`.
#[inline]
#[must_use]
pub fn finecosine(i: usize) -> i32 {
    room_core::math::FINESINE[i + FINEANGLES / 4]
}

/// `finetangent[i]`.
#[inline]
#[must_use]
pub fn finetangent(i: usize) -> i32 {
    crate::finetangent::FINETANGENT[i]
}

/// `tantoangle[i]`.
#[inline]
#[must_use]
pub fn tantoangle(i: usize) -> u32 {
    room_core::math::TANTOANGLE[i]
}

/// Map units (f64) to 16.16.
#[inline]
#[must_use]
pub fn to_fixed(v: f64) -> i32 {
    (v * f64::from(FRACUNIT)).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_div_match_doom() {
        assert_eq!(fixed_mul(3 * FRACUNIT, FRACUNIT / 2), 3 * FRACUNIT / 2);
        assert_eq!(fixed_div(FRACUNIT, 2 * FRACUNIT), FRACUNIT / 2);
        assert_eq!(fixed_div(i32::MAX / 2, 1), i32::MAX);
        assert_eq!(fixed_div(-(i32::MAX / 2), 1), i32::MIN);
        assert_eq!(finetangent(2048), 25);
        assert_eq!(finecosine(0), finesine(2048));
    }
}
