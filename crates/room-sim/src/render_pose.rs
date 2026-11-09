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

/// How a live actor's sprite is drawn for one viewer (UZDoom sprite setup).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteView {
    /// Angle to pick the rotation frame with: the actor's angle, or with
    /// `+SPRITEANGLE` one that makes the view-relative angle `SpriteAngle`.
    pub rotation_angle_deg: f64,
    /// `+SPRITEANGLE`: use [`Self::rotation_angle_deg`] instead of the
    /// (interpolated) facing.
    pub fixed_rotation: bool,
    /// In-plane spin in radians (`SpriteRotation`, plus `roll` for `+ROLLSPRITE`).
    pub roll_rad: f64,
    /// `Scale` already applies; this is the actor's own light override
    /// (`LightLevel`), `None` = sector light.
    pub light_level: Option<i32>,
}

fn wrap180(mut d: f64) -> f64 {
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d
}

fn in_range(v: f64, (start, end): (f64, f64)) -> bool {
    let (s, e) = (wrap180(start), wrap180(end));
    if s <= e {
        v >= s && v <= e
    } else {
        v >= s || v <= e
    }
}

/// Sprite visibility and orientation for a viewer at `eye` whose
/// `ViewRenderFlags` are `viewer_flags` (0 for the normal player view).
///
/// Hidden when the class `RenderRequired` bits are not all in the viewer's
/// flags, when `RenderHidden` shares a bit with them, or when the viewer sits
/// outside `VisibleAngles` / `VisiblePitch` (relative to the actor's facing
/// unless `+ABSVIEWANGLES`). `RenderRadius` only widens which sectors an
/// actor links into for portal/sector culling; Room draws every live actor,
/// so it never hides anything.
#[must_use]
pub fn sprite_view(
    world: &crate::world::World,
    a: &crate::actor::Actor,
    eye: room_core::DVec3,
    viewer_flags: i32,
) -> Option<SpriteView> {
    let class = world.classes.get(a.class)?;
    let required = class.ext.stored_i32("renderrequired").unwrap_or(0);
    if required != 0 && viewer_flags & required != required {
        return None;
    }
    let hidden = class.ext.stored_i32("renderhidden").unwrap_or(0);
    if hidden & viewer_flags != 0 {
        return None;
    }
    let dx = eye.x() - a.pos.x();
    let dy = eye.y() - a.pos.y();
    let to_viewer = room_core::dmath::atan2(dy, dx).to_degrees();
    let facing = a.angle.degrees();
    if let Some(range) = a.ext.visible_angles {
        let rel = if a.extra_flags.get_name("ABSVIEWANGLES") {
            wrap180(to_viewer)
        } else {
            wrap180(to_viewer - facing)
        };
        if !in_range(rel, range) {
            return None;
        }
    }
    if let Some(range) = a.ext.visible_pitch {
        let dist = (dx * dx + dy * dy).sqrt();
        let mid = a.pos.z() + a.height * 0.5;
        // UZDoom pitch: positive looks down, so a viewer above is negative.
        let pitch = -room_core::dmath::atan2(eye.z() - mid, dist).to_degrees();
        let rel = if a.extra_flags.get_name("ABSVIEWANGLES") {
            pitch
        } else {
            pitch + a.pitch.degrees()
        };
        if !in_range(rel, range) {
            return None;
        }
    }
    let rotation_angle_deg = if a.extra_flags.get_name("SPRITEANGLE") {
        // Rotation frames compare the viewer's angle with the actor's. Pick an
        // actor angle that leaves `SpriteAngle` as that difference.
        to_viewer - a.ext.sprite_angle
    } else {
        facing
    };
    let mut roll = a.ext.sprite_rotation;
    if a.extra_flags.get_name("ROLLSPRITE") {
        roll += a.roll.degrees();
    }
    let light_level = class.ext.stored_i32("lightlevel").filter(|&l| l >= 0);
    Some(SpriteView {
        rotation_angle_deg,
        fixed_rotation: a.extra_flags.get_name("SPRITEANGLE"),
        roll_rad: roll.to_radians(),
        light_level,
    })
}
