//! Render-only sprite pose. The playsim keeps the current tic; the shell
//! lerps from the previous sample. Checksums do not include this helper.

/// Interpolated sprite position and yaw (degrees).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpritePose {
    /// Map X.
    pub x: f64,
    /// Map Y.
    pub y: f64,
    /// Map Z.
    pub z: f64,
    /// Yaw in degrees.
    pub angle_deg: f64,
}

/// Position lerps unless `dont_interpolate` (`+DONTINTERPOLATE`).
/// Yaw stays on the current tic unless `interpolate_angles` (`+INTERPOLATEANGLES`).
#[must_use]
pub fn sprite_pose(
    curr_x: f64,
    curr_y: f64,
    curr_z: f64,
    curr_angle_deg: f64,
    prev_x: f64,
    prev_y: f64,
    prev_z: f64,
    prev_angle_deg: f64,
    frac: f64,
    dont_interpolate: bool,
    interpolate_angles: bool,
) -> SpritePose {
    let t = frac.clamp(0.0, 1.0);
    let (x, y, z) = if dont_interpolate {
        (curr_x, curr_y, curr_z)
    } else {
        (
            prev_x + (curr_x - prev_x) * t,
            prev_y + (curr_y - prev_y) * t,
            prev_z + (curr_z - prev_z) * t,
        )
    };
    let angle_deg = if interpolate_angles && !dont_interpolate {
        lerp_angle_deg(prev_angle_deg, curr_angle_deg, t)
    } else {
        curr_angle_deg
    };
    SpritePose { x, y, z, angle_deg }
}

fn lerp_angle_deg(from: f64, to: f64, t: f64) -> f64 {
    let mut delta = (to - from) % 360.0;
    if delta > 180.0 {
        delta -= 360.0;
    }
    if delta < -180.0 {
        delta += 360.0;
    }
    from + delta * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_lerps_and_dontinterpolate_snaps() {
        let mid = sprite_pose(10.0, 0.0, 0.0, 90.0, 0.0, 0.0, 0.0, 0.0, 0.5, false, false);
        assert!((mid.x - 5.0).abs() < 1e-9);
        assert!(
            (mid.angle_deg - 90.0).abs() < 1e-9,
            "yaw snaps without the flag"
        );
        let snap = sprite_pose(10.0, 0.0, 0.0, 90.0, 0.0, 0.0, 0.0, 0.0, 0.5, true, true);
        assert!((snap.x - 10.0).abs() < 1e-9);
        assert!((snap.angle_deg - 90.0).abs() < 1e-9);
    }

    #[test]
    fn interpolate_angles_takes_the_short_turn() {
        let mid = sprite_pose(0.0, 0.0, 0.0, 350.0, 0.0, 0.0, 0.0, 10.0, 0.5, false, true);
        let wrapped = (mid.angle_deg % 360.0 + 360.0) % 360.0;
        assert!(
            (wrapped - 0.0).abs() < 1e-6 || (wrapped - 360.0).abs() < 1e-6,
            "short turn 10→350 passes through 0, got {wrapped}"
        );
    }
}
