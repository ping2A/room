//! Float → int conversions matching GZDoom `xs_CRoundToInt` / `xs_FloorToInt` policy.
//!
//! Implemented with portable `libm` rounding plus saturating conversion to `i32`.
//! NaN maps to 0.

use crate::dmath;

fn sat_i32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    if x >= f64::from(i32::MAX) {
        i32::MAX
    } else if x <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        #[allow(clippy::cast_possible_truncation)]
        {
            x as i32
        }
    }
}

/// Equivalent of GZDoom `xs_FloorToInt`: floor toward −∞, then saturate to `i32`.
#[must_use]
pub fn floor_to_int(val: f64) -> i32 {
    sat_i32(dmath::floor(val))
}

/// Equivalent of GZDoom `xs_CRoundToInt`: round half away from zero (C `round`), saturate.
#[must_use]
pub fn c_round_to_int(val: f64) -> i32 {
    sat_i32(dmath::round(val))
}

/// Truncate toward zero, then saturate to `i32`.
#[must_use]
pub fn trunc_to_int(val: f64) -> i32 {
    sat_i32(dmath::trunc(val))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_negative() {
        assert_eq!(floor_to_int(1.9), 1);
        assert_eq!(floor_to_int(-1.1), -2);
    }

    #[test]
    fn round_half_away_from_zero() {
        assert_eq!(c_round_to_int(1.5), 2);
        assert_eq!(c_round_to_int(-1.5), -2);
    }

    #[test]
    fn saturates() {
        assert_eq!(c_round_to_int(1.0e20), i32::MAX);
        assert_eq!(floor_to_int(-1.0e20), i32::MIN);
        assert_eq!(c_round_to_int(f64::NAN), 0);
    }
}
