//! Strict vanilla / Boom / MBF demo math path (`docs/19-compatibility.md` §4).
//!
//! Fixed-point movement lives here so the double-precision playsim stays free of
//! `if vanilla` branches. Full `p_map` / `p_mobj` ports land incrementally.

mod movement;

pub use movement::{check_position_fixed, try_move_fixed, xy_movement_fixed, z_movement_fixed};

use room_core::Fixed;

/// Which numeric strategy the world uses for movement / collision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SimMathMode {
    /// Cross-platform deterministic `f64` (default Room / GZDoom-like).
    #[default]
    Double,
    /// Doom 16.16 fixed-point paths for vanilla-demo sync.
    VanillaFixed,
}

/// Vanilla `P_AproxDistance` (largest + half of smallest).
#[must_use]
pub fn approx_distance(dx: Fixed, dy: Fixed) -> Fixed {
    let mut dx = if dx.0 < 0 { Fixed(-dx.0) } else { dx };
    let mut dy = if dy.0 < 0 { Fixed(-dy.0) } else { dy };
    if dx.0 < dy.0 {
        core::mem::swap(&mut dx, &mut dy);
    }
    Fixed(dx.0.wrapping_add(dy.0 >> 1))
}

/// Map a `COMPATF` / complevel intent onto [`SimMathMode`].
#[must_use]
pub fn math_mode_for_vanilla_demo(strict_vanilla: bool) -> SimMathMode {
    if strict_vanilla {
        SimMathMode::VanillaFixed
    } else {
        SimMathMode::Double
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_core::FRACUNIT;

    #[test]
    fn approx_distance_matches_vanilla_shape() {
        // Largest + half smallest: max(3,4) + min/2 → 4 + 1.5 = 5.5 FRACUNITs.
        let d = approx_distance(Fixed(3 * FRACUNIT), Fixed(4 * FRACUNIT));
        assert_eq!(d.0, 4 * FRACUNIT + (3 * FRACUNIT) / 2);
        let d2 = approx_distance(Fixed(-3 * FRACUNIT), Fixed(4 * FRACUNIT));
        assert_eq!(d2, d);
    }
}
