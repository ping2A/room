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
    /// `SpriteRotation` in degrees: added to the actor's (interpolated)
    /// facing when picking the rotation frame. UZDoom's hardware renderer
    /// uses it only there (`hw_sprites.cpp`), it does not spin the quad.
    pub rotation_offset_deg: f64,
    /// In-plane spin in radians: the actor's `roll`, with `+ROLLSPRITE` only.
    pub roll_rad: f64,
    /// `Scale` already applies; this is the actor's own light override
    /// (`LightLevel`), `None` = sector light.
    pub light_level: Option<i32>,
    /// `+SPRITEFLIP`: pick the mirrored rotation and draw it mirrored.
    pub sprite_flip: bool,
    /// `+XFLIP`: mirror left to right.
    pub flip_x: bool,
    /// `+YFLIP`: upside down.
    pub flip_y: bool,
    /// `+ROLLCENTER`: roll about the sprite centre instead of the origin.
    pub roll_center: bool,
    /// Extra sprite scale `(x, y)`: `(√1.2, 1/√1.2)` for `+ROLLSPRITE` or
    /// `+SQUAREPIXELS` without `+STRETCHPIXELS` (UZDoom keeps those square
    /// against the 1.2 pixel ratio), else `(1, 1)`.
    pub pixel_scale: (f64, f64),
    /// `+NODYNAMICLIGHTING`.
    pub no_dynlight: bool,
    /// `+ISOMETRICSPRITES`: drawn for an orthographic camera (see
    /// [`isometric_sprite`]).
    pub isometric: bool,
}

/// UZDoom's level pixel ratio (`Level->pixelstretch`, MAPINFO default).
pub const PIXEL_STRETCH: f64 = 1.2;

/// UZDoom `isoscaleY` / `isotheta` for a `+ISOMETRICSPRITES` actor seen by an
/// orthographic camera at `yaw_deg` / `pitch_deg` (UZDoom pitch: positive
/// looks down): returns the vertical scale and the tilt (degrees) toward the
/// camera. `(1, pitch)` unless a tilt keeps the sprite inside its radius.
/// `floordistfact` / `cotfloor` are `FRenderViewpoint::SetViewAngle`'s.
#[must_use]
pub fn isometric_sprite(
    radius: f64,
    width: f64,
    height: f64,
    yaw_deg: f64,
    pitch_deg: f64,
) -> (f64, f64) {
    let (ys, yc) = yaw_deg.to_radians().sin_cos();
    let (ps, pc) = pitch_deg.to_radians().sin_cos();
    let (big, small) = if yc.abs() > ys.abs() {
        (yc.abs(), ys.abs())
    } else {
        (ys.abs(), yc.abs())
    };
    let floordistfact = std::f64::consts::SQRT_2 + 1.0 / big.max(1e-9);
    let cotfloor = small / big.max(1e-9);
    let mut scale_y = 1.0;
    let mut theta = pitch_deg;
    if height > 0.0 {
        let floordist = radius * floordistfact - 0.5 * width * cotfloor;
        let sine = floordist / height;
        let scl = (1.0 + sine * sine - 2.0 * ps * sine).sqrt();
        if radius > 0.0 && scl > pc.abs() {
            scale_y = scl / pc.abs().max(0.01);
            theta = (sine / scale_y).clamp(-1.0, 1.0).asin().to_degrees();
        }
    }
    (scale_y, theta)
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
    let roll = if a.extra_flags.get_name("ROLLSPRITE") {
        a.roll.degrees()
    } else {
        0.0
    };
    let light_level = class.ext.stored_i32("lightlevel").filter(|&l| l >= 0);
    let flag = |n: &str| a.extra_flags.get_name(n);
    let square = (flag("ROLLSPRITE") || flag("SQUAREPIXELS")) && !flag("STRETCHPIXELS");
    let pixel_scale = if square {
        // `r.Scale(mult * ps, mult)` with `mult = 1/√ps`.
        let m = 1.0 / PIXEL_STRETCH.sqrt();
        (m * PIXEL_STRETCH, m)
    } else {
        (1.0, 1.0)
    };
    Some(SpriteView {
        rotation_angle_deg,
        fixed_rotation: flag("SPRITEANGLE"),
        rotation_offset_deg: a.ext.sprite_rotation,
        roll_rad: roll.to_radians(),
        light_level,
        sprite_flip: flag("SPRITEFLIP"),
        flip_x: flag("XFLIP"),
        flip_y: flag("YFLIP"),
        roll_center: flag("ROLLCENTER"),
        pixel_scale,
        no_dynlight: flag("NODYNAMICLIGHTING"),
        isometric: flag("ISOMETRICSPRITES"),
    })
}

/// UZDoom `R_ShouldDrawSpriteShadow` for `r_actorspriteshadow` `mode`: 1
/// draws `+CASTSPRITESHADOW` actors, 2 also monsters and players unless
/// `+NOSPRITESHADOW`. Wall and flat sprites never cast one, and unless the
/// actor forces it neither do additive, subtractive or other non-blending
/// styles.
#[must_use]
pub fn casts_sprite_shadow(world: &crate::world::World, id: crate::ActorId, mode: u8) -> bool {
    use crate::class::{
        FLAG_COUNTKILL, FLAG_FLATSPRITE, FLAG_ISMONSTER, FLAG_WALLSPRITE, RenderStyle,
    };
    let Some(a) = world.actors.get(id) else {
        return false;
    };
    let class_flags = world.classes.get(a.class).map_or(0, |c| c.flags);
    let flags = a.flags | class_flags;
    if flags & (FLAG_FLATSPRITE | FLAG_WALLSPRITE) != 0 {
        return false;
    }
    let forced = a.extra_flags.get_name("CASTSPRITESHADOW");
    let player = world.players.iter().flatten().any(|p| p.mo == Some(id));
    let doit = match mode {
        1 => forced,
        2 => {
            forced
                || (!a.extra_flags.get_name("NOSPRITESHADOW")
                    && (flags & (FLAG_ISMONSTER | FLAG_COUNTKILL) != 0 || player))
        }
        _ => false,
    };
    if !doit || forced {
        return doit;
    }
    // `BlendOp` Add or Shadow with `DestAlpha` Zero / InvSrc.
    matches!(
        a.render_style,
        RenderStyle::Normal
            | RenderStyle::Translucent
            | RenderStyle::SoulTrans
            | RenderStyle::Fuzzy
            | RenderStyle::OptFuzzy
            | RenderStyle::Stencil
            | RenderStyle::TranslucentStencil
            | RenderStyle::Shaded
            | RenderStyle::Shadow
    )
}

/// Camera eye of `a` for a view at `eye_z`, as UZDoom's `R_SetupFrame`:
/// `SetViewPos` offsets apply, the eye stays 4 units inside the view
/// sector's floor and ceiling unless `VPSF_ALLOWOUTOFBOUNDS` (8), and
/// `VPSF_ORTHOGRAPHIC` (16) returns the offset length for an orthographic
/// projection.
#[must_use]
pub fn view_eye(
    world: &crate::world::World,
    a: &crate::actor::Actor,
    eye_z: f64,
) -> ((f64, f64, f64), Option<f64>) {
    let (x, y, mut z) = a.camera_eye(eye_z);
    let flags = a.view_pos.map_or(0, |v| v.3);
    if a.view_pos.is_some() && flags & 8 == 0 {
        if let Some(s) =
            crate::physics::sector_at_xy(world, x, y).and_then(|i| world.sectors.get(i))
        {
            let (floor, ceil) = (s.floor_z + 4.0, s.ceiling_z - 4.0);
            if ceil >= floor {
                z = z.clamp(floor, ceil);
            }
        }
    }
    let ortho = a
        .view_pos
        .filter(|v| v.3 & 16 != 0)
        .map(|(ox, oy, oz, _)| (ox * ox + oy * oy + oz * oz).sqrt())
        .filter(|d| *d > 0.0);
    ((x, y, z), ortho)
}
