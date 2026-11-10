//! Sprite billboard emission (static actors for M4; live 8-dir for M5).

use crate::mesh::WallVertex;
use glam::{Vec2, Vec3};

/// One sprite instance in world space (pre-billboard verts filled by [`emit_sprite_quad`]).
#[derive(Clone, Debug)]
pub struct SpriteInstance {
    /// World position (sprite origin / feet).
    pub pos: Vec3,
    /// Width in map units.
    pub width: f32,
    /// Height in map units.
    pub height: f32,
    /// Light 0–1.
    pub light: f32,
    /// Opacity 0–1 (`RenderStyle` / class Alpha).
    pub alpha: f32,
    /// Apply spectre fuzz (`STYLE_Fuzzy` / `OptFuzzy`).
    pub fuzz: bool,
    /// Doom sprite lump (`POSSA1`) or empty for default albedo.
    pub texture: String,
    /// Mirror U when using a flipped rotation frame.
    pub flip_u: bool,
    /// Horizontal origin as a fraction of width (`leftoffset / width`).
    /// Doom sprite origin sits at this column; `0.5` centres the billboard.
    pub anchor_u: f32,
    /// Billboard, or a plane fixed to the actor's facing.
    pub orient: SpriteOrient,
    /// `RenderStyle Add`: colour × alpha is added to the scene (GZDoom
    /// `STYLE_Add`); black texels vanish.
    pub additive: bool,
    /// Rotation of the quad in its own plane, radians counter-clockwise
    /// (`SpriteRotation` / `A_SetSpriteRotation`).
    pub roll: f32,
    /// UZDoom sprite extras (flip, roll pivot, lighting, blend).
    pub fx: SpriteFx,
}

/// Per-sprite render flags from the actor (UZDoom `renderflags`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpriteFx {
    /// `+YFLIP`: upside down.
    pub flip_v: bool,
    /// Roll pivot in the world (the actor's origin; UZDoom rolls about it
    /// unless `+ROLLCENTER`). `None` = the quad centre.
    pub roll_pivot: Option<Vec3>,
    /// `+NODYNAMICLIGHTING`: dynamic lights skip this sprite.
    pub no_dynlight: bool,
    /// `STYLE_Subtract`: reverse-subtract blending.
    pub subtract: bool,
}

/// How a sprite quad is oriented in the world.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SpriteOrient {
    /// Turns to face the camera around Z (Doom).
    #[default]
    Billboard,
    /// Vertical plane facing `yaw` radians (`+WALLSPRITE`, Build wall sprites).
    Wall {
        /// Facing direction in radians (map XY).
        yaw: f32,
    },
    /// Horizontal plane centred on `pos`, its top edge toward `yaw` (`+FLATSPRITE`).
    Flat {
        /// Facing direction in radians (map XY).
        yaw: f32,
    },
    /// Fully camera-facing quad centred on `pos` (GZDoom particles):
    /// `right` / `up` span the quad.
    Facing {
        /// Unit right vector.
        right: Vec3,
        /// Unit up vector.
        up: Vec3,
    },
}

/// Doom sprite frame letter. `0..25` is `A`–`Z`; `26..28` is `[` `\` `]`
/// (GZDoom's extra frames). Clamping those to `Z` looks up a lump that is not
/// there, so the actor vanishes for that tic.
#[must_use]
pub fn sprite_frame_letter(frame: u8) -> char {
    match frame {
        0..=25 => char::from(b'A' + frame),
        26 => '[',
        27 => '\\',
        28 => ']',
        _ => 'A',
    }
}

/// Build candidate lump names for a DECORATE sprite + frame (rotation 0 then 1).
#[must_use]
pub fn sprite_lump_candidates(sprite: &str, frame: u8) -> [String; 2] {
    let s = sprite.trim().to_ascii_uppercase();
    let letter = sprite_frame_letter(frame);
    [format!("{s}{letter}0"), format!("{s}{letter}1")]
}

/// Doom rotation digit 1–8 from actor yaw and camera position (map XY).
///
/// Uses the classic 8-way octant relative to the actor's facing direction.
#[must_use]
pub fn sprite_rotation_digit(
    actor_x: f64,
    actor_y: f64,
    actor_angle_deg: f64,
    view_x: f64,
    view_y: f64,
) -> u8 {
    let dx = view_x - actor_x;
    let dy = view_y - actor_y;
    // Angle from actor toward viewer (Doom BAM-style degrees).
    let to_view = dy.atan2(dx).to_degrees();
    let rel = (to_view - actor_angle_deg).rem_euclid(360.0);
    // Round to nearest 45° octant → 0..7, then Doom digits 1..8.
    let octant = ((rel + 22.5) / 45.0).floor() as i32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        ((octant.rem_euclid(8) as u8) + 1).clamp(1, 8)
    }
}

/// Mirror rotation digit (2↔8, 3↔7, 4↔6); 1 and 5 have no mirror.
#[must_use]
pub fn mirror_rotation(digit: u8) -> Option<u8> {
    match digit {
        2 => Some(8),
        3 => Some(7),
        4 => Some(6),
        6 => Some(4),
        7 => Some(3),
        8 => Some(2),
        _ => None,
    }
}

/// Ordered `(lump_name, flip_u)` tries for a sprite frame at a view rotation.
#[must_use]
pub fn sprite_lump_candidates_rot(sprite: &str, frame: u8, rot_digit: u8) -> Vec<(String, bool)> {
    let s = sprite.trim().to_ascii_uppercase();
    if s.is_empty() {
        return Vec::new();
    }
    let letter = sprite_frame_letter(frame);
    let rot = rot_digit.clamp(1, 8);
    let mut out = Vec::with_capacity(8);
    out.push((format!("{s}{letter}{rot}"), false));
    if let Some(m) = mirror_rotation(rot) {
        // Exact mirror lump, flipped.
        out.push((format!("{s}{letter}{m}"), true));
        // Compound lumps `A2A8` / `A8A2`.
        out.push((format!("{s}{letter}{rot}{letter}{m}"), false));
        out.push((format!("{s}{letter}{m}{letter}{rot}"), true));
    }
    out.push((format!("{s}{letter}0"), false));
    out.push((format!("{s}{letter}1"), false));
    out
}

/// World shift for `A_SpriteOffset`. `ox` is to the camera's right, `oy` is up.
#[must_use]
pub fn sprite_offset_delta(view_forward: Vec3, ox: f32, oy: f32) -> Vec3 {
    let fwd = Vec3::new(view_forward.x, view_forward.y, 0.0);
    let right = if fwd.length_squared() < 1e-8 {
        Vec3::Y
    } else {
        let fwd = fwd.normalize();
        Vec3::new(-fwd.y, fwd.x, 0.0)
    };
    right * ox + Vec3::Z * oy
}

/// Emit a Y-axis billboard quad facing `view_forward` (XY), standing on `pos`.
#[must_use]
pub fn emit_sprite_quad(inst: SpriteInstance, view_forward: Vec3) -> [WallVertex; 6] {
    let (right, up) = match inst.orient {
        SpriteOrient::Billboard => {
            let fwd = Vec3::new(view_forward.x, view_forward.y, 0.0).normalize_or_zero();
            (Vec3::new(-fwd.y, fwd.x, 0.0), Vec3::Z)
        }
        // Build wall sprites face their angle (0 = +X). `right × up` is that
        // normal, so the lettering reads from the front. The opposite sign
        // showed the back of the quad and mirrored "POLICE LINE".
        SpriteOrient::Wall { yaw } => (Vec3::new(yaw.sin(), yaw.cos(), 0.0), Vec3::Z),
        SpriteOrient::Flat { yaw } => (
            Vec3::new(yaw.sin(), -yaw.cos(), 0.0),
            Vec3::new(yaw.cos(), yaw.sin(), 0.0),
        ),
        SpriteOrient::Facing { right, up } => (right, up),
    };
    let half_w = inst.width * 0.5;
    // Shift so column `anchor_u * width` stays on `inst.pos` (Doom leftoverset;
    // may lie outside [0,1] for some patches).
    let anchor = inst.anchor_u;
    let mut mid = inst.pos + right * (half_w - anchor * inst.width);
    if matches!(inst.orient, SpriteOrient::Flat { .. }) {
        // Centred on the actor and lifted off the plane it lies on.
        mid += Vec3::Z * 0.25 - up * (inst.height * 0.5);
    }
    if matches!(inst.orient, SpriteOrient::Facing { .. }) {
        mid -= up * (inst.height * 0.5);
    }
    let lo = mid;
    let hi = mid + up * inst.height;
    let mut a = lo - right * half_w;
    let mut b = lo + right * half_w;
    let mut c = hi + right * half_w;
    let mut d = hi - right * half_w;
    if inst.roll.abs() > 1e-6 {
        // Spin the quad in its own plane about its centre, or about the
        // actor's origin projected onto the quad (UZDoom without ROLLCENTER).
        let mid_q = (a + c) * 0.5;
        let centre = inst.fx.roll_pivot.map_or(mid_q, |p| {
            let o = p - mid_q;
            mid_q + right * o.dot(right) + up * o.dot(up)
        });
        let (sn, cs) = inst.roll.sin_cos();
        let spin = |p: Vec3| {
            let o = p - centre;
            let x = o.dot(right);
            let y = o.dot(up);
            centre + right * (x * cs - y * sn) + up * (x * sn + y * cs)
        };
        a = spin(a);
        b = spin(b);
        c = spin(c);
        d = spin(d);
    }
    let (u0, u1) = if inst.flip_u { (1.0, 0.0) } else { (0.0, 1.0) };
    let (v_lo, v_hi) = if inst.fx.flip_v { (0.0, 1.0) } else { (1.0, 0.0) };
    // Additive (and subtractive) sprites carry a negative alpha (decoded by
    // the shader as premultiplied colour, alpha 0).
    let alpha = if inst.additive || inst.fx.subtract {
        -inst.alpha.clamp(0.0, 1.0).max(1e-4)
    } else {
        inst.alpha.clamp(0.0, 1.0)
    };
    let va = WallVertex::new(a, Vec2::new(u0, v_lo), inst.light)
        .with_alpha(alpha)
        .with_masked()
        .with_fuzz(inst.fuzz);
    let vb = WallVertex::new(b, Vec2::new(u1, v_lo), inst.light)
        .with_alpha(alpha)
        .with_masked()
        .with_fuzz(inst.fuzz);
    let vc = WallVertex::new(c, Vec2::new(u1, v_hi), inst.light)
        .with_alpha(alpha)
        .with_masked()
        .with_fuzz(inst.fuzz);
    let vd = WallVertex::new(d, Vec2::new(u0, v_hi), inst.light)
        .with_alpha(alpha)
        .with_masked()
        .with_fuzz(inst.fuzz);
    [va, vb, vc, va, vc, vd]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_offset_moves_right_and_up() {
        // Looking toward -Y, camera-right is +X.
        let d = sprite_offset_delta(Vec3::new(0.0, -1.0, 0.0), 4.0, 6.0);
        assert!((d.x - 4.0).abs() < 1e-4, "{d:?}");
        assert!(d.y.abs() < 1e-4, "{d:?}");
        assert!((d.z - 6.0).abs() < 1e-4, "{d:?}");
    }

    #[test]
    fn billboard_has_area() {
        let q = emit_sprite_quad(
            SpriteInstance {
                pos: Vec3::new(0.0, 0.0, 0.0),
                width: 32.0,
                height: 56.0,
                light: 1.0,
                alpha: 1.0,
                fuzz: false,
                texture: String::new(),
                flip_u: false,
                anchor_u: 0.5,
                orient: crate::sprites::SpriteOrient::Billboard,
                additive: false,
                roll: 0.0,
                fx: Default::default(),
            },
            Vec3::X,
        );
        assert_eq!(sprite_lump_candidates("POSS", 0)[1], "POSSA1");
        assert_eq!(q.len(), 6);
        let ys: Vec<f32> = q.iter().map(|v| v.pos[1]).collect();
        assert!(ys.iter().any(|&y| (y - 16.0).abs() < 0.01));
        assert!(ys.iter().any(|&y| (y + 16.0).abs() < 0.01));
    }

    #[test]
    fn wall_sprite_front_faces_plus_x_at_yaw_zero() {
        let q = emit_sprite_quad(
            SpriteInstance {
                pos: Vec3::ZERO,
                width: 32.0,
                height: 8.0,
                light: 1.0,
                alpha: 1.0,
                fuzz: false,
                texture: String::new(),
                flip_u: false,
                anchor_u: 0.5,
                orient: SpriteOrient::Wall { yaw: 0.0 },
                additive: false,
                roll: 0.0,
                fx: Default::default(),
            },
            Vec3::Y,
        );
        let a = Vec3::from_array(q[0].pos);
        let b = Vec3::from_array(q[1].pos);
        let c = Vec3::from_array(q[2].pos);
        let n = (b - a).cross(c - a);
        assert!(n.x > 0.0, "front should face +X, normal {n:?}");
        // U grows along +right, which is +Y at yaw 0, so the left edge is -Y.
        assert!(a.y < b.y, "u=0 is the left edge when looking along +X");
    }

    #[test]
    fn leftoverset_shifts_billboard_along_right() {
        // Origin at left edge (anchor 0): one vertical edge stays on the actor.
        let q = emit_sprite_quad(
            SpriteInstance {
                pos: Vec3::ZERO,
                width: 20.0,
                height: 10.0,
                light: 1.0,
                alpha: 1.0,
                fuzz: false,
                texture: String::new(),
                flip_u: false,
                anchor_u: 0.0,
                orient: crate::sprites::SpriteOrient::Billboard,
                additive: false,
                roll: 0.0,
                fx: Default::default(),
            },
            Vec3::Y,
        );
        let on_origin = q
            .iter()
            .any(|v| v.pos[0].abs() < 0.01 && v.pos[1].abs() < 0.01);
        assert!(
            on_origin,
            "left-anchored sprite must keep an edge on the origin"
        );
        let span = q
            .iter()
            .map(|v| v.pos[0].abs() + v.pos[1].abs())
            .fold(0.0_f32, f32::max);
        assert!((span - 20.0).abs() < 0.01, "span={span}");
    }

    #[test]
    fn leftoverset_outside_unit_interval_is_honoured() {
        // leftoffset > width → anchor > 1 must not clamp (shifts further than centre).
        let centred = emit_sprite_quad(
            SpriteInstance {
                pos: Vec3::ZERO,
                width: 20.0,
                height: 10.0,
                light: 1.0,
                alpha: 1.0,
                fuzz: false,
                texture: String::new(),
                flip_u: false,
                anchor_u: 0.5,
                orient: crate::sprites::SpriteOrient::Billboard,
                additive: false,
                roll: 0.0,
                fx: Default::default(),
            },
            Vec3::Y,
        );
        let wide = emit_sprite_quad(
            SpriteInstance {
                pos: Vec3::ZERO,
                width: 20.0,
                height: 10.0,
                light: 1.0,
                alpha: 1.0,
                fuzz: false,
                texture: String::new(),
                flip_u: false,
                anchor_u: 1.25,
                orient: crate::sprites::SpriteOrient::Billboard,
                additive: false,
                roll: 0.0,
                fx: Default::default(),
            },
            Vec3::Y,
        );
        let mid_x = |q: &[WallVertex; 6]| {
            let xs: Vec<f32> = q.iter().map(|v| v.pos[0]).collect();
            (xs.iter().cloned().fold(f32::INFINITY, f32::min)
                + xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max))
                * 0.5
        };
        let shift = (mid_x(&wide) - mid_x(&centred)).abs();
        assert!(
            (shift - 15.0).abs() < 0.05,
            "anchor 1.25 vs 0.5 must shift by 0.75*width; got {shift}"
        );
    }

    #[test]
    fn rotation_front_is_one() {
        // Actor facing +X (0°); viewer further along +X → looking at back?
        // to_view from actor to viewer at +X is 0°, rel = 0 - 0 = 0 → octant 0 → digit 1 (front of actor faces +X, viewer in front).
        let d = sprite_rotation_digit(0.0, 0.0, 0.0, 100.0, 0.0);
        assert_eq!(d, 1);
    }

    #[test]
    fn rotation_side_is_three_or_seven() {
        // Actor facing +X; viewer at +Y → left/right side.
        let d = sprite_rotation_digit(0.0, 0.0, 0.0, 0.0, 100.0);
        assert!(d == 3 || d == 7, "d={d}");
    }

    #[test]
    fn extra_frames_keep_their_letters() {
        assert_eq!(sprite_lump_candidates("POSS", 26)[1], "POSS[1");
        assert_eq!(sprite_lump_candidates("POSS", 27)[0], "POSS\\0");
        assert_eq!(sprite_lump_candidates_rot("POSS", 28, 1)[0].0, "POSS]1");
    }

    fn candidates_include_mirror_compound() {
        let c = sprite_lump_candidates_rot("TROO", 0, 2);
        assert!(c.iter().any(|(n, flip)| n == "TROOA2" && !flip));
        assert!(c.iter().any(|(n, flip)| n == "TROOA8" && *flip));
        assert!(c.iter().any(|(n, _)| n == "TROOA2A8"));
    }

    #[test]
    fn flip_swaps_u() {
        let q = emit_sprite_quad(
            SpriteInstance {
                pos: Vec3::ZERO,
                width: 32.0,
                height: 56.0,
                light: 1.0,
                alpha: 1.0,
                fuzz: false,
                texture: String::new(),
                flip_u: true,
                anchor_u: 0.5,
                orient: crate::sprites::SpriteOrient::Billboard,
                additive: false,
                roll: 0.0,
                fx: Default::default(),
            },
            Vec3::X,
        );
        // First corner (left) should have u=1 when flipped.
        assert!((q[0].uv[0] - 1.0).abs() < 1e-5);
    }
}
