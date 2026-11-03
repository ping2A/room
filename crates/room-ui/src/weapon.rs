//! First-person weapon overlay (`psp` Ready / Fire / Flash).

use crate::draw::{UiDrawList, Vec2};
use crate::patches::SoftPatch;

/// Doom `WEAPONTOP` — psprite Y origin in the *view* (not the full 320×200).
pub const WEAPON_TOP: f32 = 32.0;
/// Classic status bar height — view is 320×168 when the bar is shown.
pub const STATUS_BAR_HEIGHT: f32 = 32.0;

/// Doom ready-state patch for a weapon class name (`PISGA0`, …).
#[must_use]
pub fn weapon_ready_patch(class_name: &str) -> &'static str {
    match class_name {
        "Fist" => "PUNGA0",
        "Chainsaw" => "SAWGA0",
        "Pistol" => "PISGA0",
        "Shotgun" => "SHTGA0",
        "SuperShotgun" => "SGN2A0",
        "Chaingun" => "CHGGA0",
        "RocketLauncher" => "MISGA0",
        "PlasmaRifle" => "PLSGA0",
        "BFG9000" => "BFGGA0",
        _ => "PISGA0",
    }
}

/// Screen position of a weapon / flash psprite (320×200).
///
/// GZDoom overlay: `x = -leftoffset`, `y = WEAPONTOP - topoffset`, plus bob.
/// Patch offsets are authored for the **view** (168 tall with status bar), so
/// when the bar is shown we shift up by [`STATUS_BAR_HEIGHT`] — otherwise the
/// gun sits flush with y=200 and only a tip peeks above STBAR.
/// Fire and muzzle-flash frames share this origin so they stay locked to the
/// barrel. Centering by width independently made flash/fire jump.
#[must_use]
pub fn psprite_pos(soft: &SoftPatch, bob_x: f32, bob_y: f32, fullscreen: bool) -> Vec2 {
    let y_shift = if fullscreen { 0.0 } else { STATUS_BAR_HEIGHT };
    Vec2::new(
        bob_x - f32::from(soft.left_offset),
        WEAPON_TOP - f32::from(soft.top_offset) + bob_y - y_shift,
    )
}

/// Draw a weapon or muzzle-flash patch at the Doom psprite origin.
pub fn draw_weapon(
    list: &mut UiDrawList,
    patch: &str,
    fullscreen: bool,
    soft: Option<&SoftPatch>,
    bob_x: f32,
    bob_y: f32,
) {
    draw_weapon_tinted(
        list, patch, fullscreen, soft, bob_x, bob_y, 1.0, false, false, 1.0, 1.0, 1.0, false, 0.5,
        0.5, 0.0, 1.0, 1.0, 1.0, false, true,
    );
}

/// Draw a psprite with sector lighting / Bright / fuzz (docs/30 W6).
pub fn draw_weapon_lit(
    list: &mut UiDrawList,
    patch: &str,
    fullscreen: bool,
    soft: Option<&SoftPatch>,
    bob_x: f32,
    bob_y: f32,
    light: f32,
    bright: bool,
    fuzz: bool,
    alpha: f32,
    scale_x: f32,
    scale_y: f32,
    flip: bool,
    pivot_x: f32,
    pivot_y: f32,
    angle: f32,
) {
    draw_weapon_tinted(
        list, patch, fullscreen, soft, bob_x, bob_y, light, bright, fuzz, alpha, scale_x, scale_y,
        flip, pivot_x, pivot_y, angle, 1.0, 1.0, 1.0, false, true,
    );
}

/// [`draw_weapon_lit`] plus an RGB tint from `A_OverlayTranslation`.
pub fn draw_weapon_tinted(
    list: &mut UiDrawList,
    patch: &str,
    fullscreen: bool,
    soft: Option<&SoftPatch>,
    bob_x: f32,
    bob_y: f32,
    light: f32,
    bright: bool,
    fuzz: bool,
    alpha: f32,
    scale_x: f32,
    scale_y: f32,
    flip: bool,
    pivot_x: f32,
    pivot_y: f32,
    angle: f32,
    tint_r: f32,
    tint_g: f32,
    tint_b: f32,
    mirror: bool,
    pivot_percent: bool,
) {
    if patch.is_empty() {
        return;
    }
    let (mut pos, mut size) = if let Some(p) = soft {
        (
            psprite_pos(p, bob_x, bob_y, fullscreen),
            Vec2::new(f32::from(p.width), f32::from(p.height)),
        )
    } else {
        (
            Vec2::new(96.0 + bob_x, if fullscreen { 88.0 } else { 56.0 } + bob_y),
            Vec2::new(0.0, 0.0),
        )
    };
    let (pivot_x, pivot_y) = if pivot_percent || size.x <= 0.0 || size.y <= 0.0 {
        (pivot_x, pivot_y)
    } else {
        (pivot_x / size.x, pivot_y / size.y)
    };
    if size.x > 0.0 && size.y > 0.0 {
        let sx = scale_x;
        let sy = scale_y;
        let w = size.x * sx;
        let h = size.y * sy;
        pos.x -= (w - size.x) * pivot_x;
        pos.y -= (h - size.y) * pivot_y;
        size = Vec2::new(w, h);
    }
    let lit = if bright { 1.0 } else { light.clamp(0.05, 1.0) };
    let lit = lit * alpha.clamp(0.0, 1.0);
    list.patch_lit_flip(
        patch,
        pos,
        size,
        lit,
        fuzz,
        flip,
        mirror,
        angle,
        pivot_x,
        pivot_y,
        [tint_r, tint_g, tint_b],
    );
}

/// Gun bob shared with a weapon overlay, then adjusted for that layer's flags.
/// `gun_bob` already includes `weapon_ofs` when the gun itself adds it.
#[must_use]
pub fn overlay_layer_bob(gun_bob: f32, weapon_ofs: f32, add_bob: bool, add_weapon: bool) -> f32 {
    let mut v = if add_bob { gun_bob } else { 0.0 };
    if add_bob && !add_weapon {
        v -= weapon_ofs;
    }
    if !add_bob && add_weapon {
        v += weapon_ofs;
    }
    v
}

/// Gun offset between tics. `PSPF_INTERPOLATE` eases; otherwise the offset snaps.
#[must_use]
pub fn overlay_bob(prev: (f32, f32), curr: (f32, f32), frac: f32, interpolate: bool) -> (f32, f32) {
    if !interpolate {
        return curr;
    }
    let t = frac.clamp(0.0, 1.0);
    (
        prev.0 + (curr.0 - prev.0) * t,
        prev.1 + (curr.1 - prev.1) * t,
    )
}

/// Screen-center crosshair for `A_SetCrosshair`. Index 0 draws nothing.
/// Index 2 is a wider mark than index 1.
pub fn draw_crosshair(list: &mut UiDrawList, index: i32) {
    if index <= 0 {
        return;
    }
    let arm = (2.0 + (index.min(8) as f32) * 2.0).min(20.0);
    let color = crate::draw::Rgba::new(1.0, 1.0, 1.0, 0.85);
    let cx = list.virtual_w * 0.5;
    let cy = list.virtual_h * 0.5;
    list.fill(
        crate::draw::Rect::new(cx - arm, cy - 0.5, arm * 2.0, 1.0),
        color,
    );
    list.fill(
        crate::draw::Rect::new(cx - 0.5, cy - arm, 1.0, arm * 2.0),
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawCmd;

    fn patch(w: u16, h: u16, lo: i16, to: i16) -> SoftPatch {
        let n = usize::from(w) * usize::from(h);
        SoftPatch {
            width: w,
            height: h,
            left_offset: lo,
            top_offset: to,
            indices: vec![1; n],
            alpha: vec![255; n],
            rgba: None,
        }
    }

    #[test]
    fn pistol_maps_to_pisga0() {
        assert_eq!(weapon_ready_patch("Pistol"), "PISGA0");
        assert_eq!(weapon_ready_patch("Shotgun"), "SHTGA0");
    }

    #[test]
    fn draw_weapon_emits_patch() {
        let mut list = UiDrawList::new();
        draw_weapon(&mut list, "PISGA0", false, None, 0.0, 0.0);
        assert!(matches!(
            list.cmds.last(),
            Some(DrawCmd::Patch { name, .. }) if name == "PISGA0"
        ));
    }

    #[test]
    fn psprite_uses_leftoffset_not_width_center() {
        // Freedoom PISGA0: lo=-125, to=-97.
        let gun = patch(82, 92, -125, -97);
        let pos = psprite_pos(&gun, 4.0, 2.0, true);
        assert!((pos.x - (4.0 - -125.0)).abs() < 1e-3);
        assert!((pos.y - (32.0 - -97.0 + 2.0)).abs() < 1e-3);
    }

    #[test]
    fn overlay_alpha_halves_the_weapon_light() {
        let mut list = UiDrawList::new();
        let gun = patch(8, 8, 0, 0);
        draw_weapon_lit(
            &mut list,
            "PISGA0",
            true,
            Some(&gun),
            0.0,
            0.0,
            1.0,
            false,
            false,
            0.5,
            1.0,
            1.0,
            false,
            0.5,
            0.5,
            0.0,
        );
        match list.cmds.last() {
            Some(DrawCmd::Patch { light, .. }) => assert!((light - 0.5).abs() < 1e-4),
            other => panic!("expected a patch, got {other:?}"),
        }
    }

    #[test]
    fn overlay_scale_grows_the_patch_about_its_center() {
        let mut list = UiDrawList::new();
        let gun = patch(8, 8, 0, 0);
        draw_weapon_lit(
            &mut list,
            "PISGA0",
            true,
            Some(&gun),
            0.0,
            0.0,
            1.0,
            true,
            false,
            1.0,
            2.0,
            3.0,
            false,
            0.5,
            0.5,
            0.0,
        );
        match list.cmds.last() {
            Some(DrawCmd::Patch { size, pos, .. }) => {
                assert!((size.x - 16.0).abs() < 1e-3);
                assert!((size.y - 24.0).abs() < 1e-3);
                assert!((pos.x - -4.0).abs() < 1e-3);
                assert!((pos.y - 24.0).abs() < 1e-3);
            }
            other => panic!("expected a patch, got {other:?}"),
        }
    }

    #[test]
    fn overlay_layer_bob_drops_the_weapon_offset_only() {
        assert!((overlay_layer_bob(13.0, 3.0, true, true) - 13.0).abs() < 1e-3);
        assert!((overlay_layer_bob(13.0, 3.0, true, false) - 10.0).abs() < 1e-3);
        assert!((overlay_layer_bob(13.0, 3.0, false, true) - 3.0).abs() < 1e-3);
        assert!((overlay_layer_bob(13.0, 3.0, false, false) - 0.0).abs() < 1e-3);
    }

    #[test]
    fn overlay_bob_eases_halfway_and_snaps_when_off() {
        assert_eq!(overlay_bob((0.0, 0.0), (10.0, 4.0), 0.5, true), (5.0, 2.0));
        assert_eq!(
            overlay_bob((0.0, 0.0), (10.0, 4.0), 0.5, false),
            (10.0, 4.0)
        );
    }

    #[test]
    fn overlay_pivot_pixels_match_the_center_fraction() {
        let gun = patch(8, 8, 0, 0);
        let mut pixels = UiDrawList::new();
        draw_weapon_tinted(
            &mut pixels,
            "PISGA0",
            true,
            Some(&gun),
            0.0,
            0.0,
            1.0,
            true,
            false,
            1.0,
            2.0,
            2.0,
            false,
            4.0,
            4.0,
            0.0,
            1.0,
            1.0,
            1.0,
            false,
            false,
        );
        let mut fraction = UiDrawList::new();
        draw_weapon_lit(
            &mut fraction,
            "PISGA0",
            true,
            Some(&gun),
            0.0,
            0.0,
            1.0,
            true,
            false,
            1.0,
            2.0,
            2.0,
            false,
            0.5,
            0.5,
            0.0,
        );
        let pos = |list: &UiDrawList| match list.cmds.last() {
            Some(DrawCmd::Patch { pos, .. }) => *pos,
            other => panic!("expected a patch, got {other:?}"),
        };
        let a = pos(&pixels);
        let b = pos(&fraction);
        assert!((a.x - b.x).abs() < 1e-3);
        assert!((a.y - b.y).abs() < 1e-3);
    }

    #[test]
    fn overlay_pivot_at_the_corner_keeps_the_top_left() {
        let mut list = UiDrawList::new();
        let gun = patch(8, 8, 0, 0);
        draw_weapon_lit(
            &mut list,
            "PISGA0",
            true,
            Some(&gun),
            0.0,
            0.0,
            1.0,
            true,
            false,
            1.0,
            2.0,
            3.0,
            false,
            0.0,
            0.0,
            0.0,
        );
        match list.cmds.last() {
            Some(DrawCmd::Patch { size, pos, .. }) => {
                assert!((size.x - 16.0).abs() < 1e-3);
                assert!((size.y - 24.0).abs() < 1e-3);
                assert!(pos.x.abs() < 1e-3);
                assert!((pos.y - 32.0).abs() < 1e-3);
            }
            other => panic!("expected a patch, got {other:?}"),
        }
    }

    #[test]
    fn overlay_pivot_align_right_grows_to_the_left() {
        let mut list = UiDrawList::new();
        let gun = patch(8, 8, 0, 0);
        draw_weapon_lit(
            &mut list,
            "PISGA0",
            true,
            Some(&gun),
            0.0,
            0.0,
            1.0,
            true,
            false,
            1.0,
            2.0,
            3.0,
            false,
            1.0,
            0.0,
            0.0,
        );
        match list.cmds.last() {
            Some(DrawCmd::Patch { size, pos, .. }) => {
                assert!((size.x - 16.0).abs() < 1e-3);
                assert!((size.y - 24.0).abs() < 1e-3);
                assert!((pos.x - -8.0).abs() < 1e-3);
                assert!((pos.y - 32.0).abs() < 1e-3);
            }
            other => panic!("expected a patch, got {other:?}"),
        }
    }

    #[test]
    fn overlay_flip_marks_the_weapon_patch() {
        let mut list = UiDrawList::new();
        let gun = patch(8, 8, 0, 0);
        draw_weapon_lit(
            &mut list,
            "PISGA0",
            true,
            Some(&gun),
            0.0,
            0.0,
            1.0,
            true,
            false,
            1.0,
            1.0,
            1.0,
            true,
            0.5,
            0.5,
            0.0,
        );
        match list.cmds.last() {
            Some(DrawCmd::Patch { flip: true, .. }) => {}
            other => panic!("expected a flipped patch, got {other:?}"),
        }
    }

    #[test]
    fn overlay_rotate_stores_ninety_degrees() {
        let mut list = UiDrawList::new();
        let gun = patch(8, 8, 0, 0);
        draw_weapon_lit(
            &mut list,
            "PISGA0",
            true,
            Some(&gun),
            0.0,
            0.0,
            1.0,
            true,
            false,
            1.0,
            1.0,
            1.0,
            false,
            0.5,
            0.5,
            90.0,
        );
        match list.cmds.last() {
            Some(DrawCmd::Patch { angle, .. }) => assert!((angle - 90.0).abs() < 1e-4),
            other => panic!("expected a patch, got {other:?}"),
        }
    }

    #[test]
    fn crosshair_two_is_wider_than_one() {
        let mut bare = UiDrawList::new();
        draw_crosshair(&mut bare, 0);
        assert!(bare.cmds.is_empty());
        let mut one = UiDrawList::new();
        draw_crosshair(&mut one, 1);
        let mut two = UiDrawList::new();
        draw_crosshair(&mut two, 2);
        let width = |list: &UiDrawList| match list.cmds.first() {
            Some(DrawCmd::Fill { rect, .. }) => rect.w,
            other => panic!("expected a fill, got {other:?}"),
        };
        assert!(width(&two) > width(&one));
    }

    #[test]
    fn overlay_mirror_stores_a_vertical_flip() {
        let mut list = UiDrawList::new();
        let gun = patch(8, 8, 0, 0);
        draw_weapon_tinted(
            &mut list,
            "PISGA0",
            true,
            Some(&gun),
            0.0,
            0.0,
            1.0,
            true,
            false,
            1.0,
            1.0,
            1.0,
            false,
            0.5,
            0.5,
            0.0,
            1.0,
            1.0,
            1.0,
            true,
            true,
        );
        match list.cmds.last() {
            Some(DrawCmd::Patch { mirror, .. }) => assert!(*mirror),
            other => panic!("expected a patch, got {other:?}"),
        }
    }

    #[test]
    fn overlay_translation_stores_a_green_tint() {
        let mut list = UiDrawList::new();
        let gun = patch(8, 8, 0, 0);
        draw_weapon_tinted(
            &mut list,
            "PISGA0",
            true,
            Some(&gun),
            0.0,
            0.0,
            1.0,
            true,
            false,
            1.0,
            1.0,
            1.0,
            false,
            0.5,
            0.5,
            0.0,
            0.25,
            1.0,
            0.25,
            false,
            true,
        );
        match list.cmds.last() {
            Some(DrawCmd::Patch { tint, .. }) => {
                assert!((tint[0] - 0.25).abs() < 1e-4);
                assert!((tint[1] - 1.0).abs() < 1e-4);
            }
            other => panic!("expected a patch, got {other:?}"),
        }
    }

    #[test]
    fn status_bar_raises_weapon_into_view() {
        // Doom1 PISGA0: without the shift, bottom sits at y=200 (hidden under STBAR).
        let gun = patch(57, 62, -126, -106);
        let full = psprite_pos(&gun, 0.0, 0.0, true);
        let bar = psprite_pos(&gun, 0.0, 0.0, false);
        assert!((full.y - 138.0).abs() < 1e-3);
        assert!((bar.y - 106.0).abs() < 1e-3);
        assert!((bar.y + 62.0 - 168.0).abs() < 1e-3, "bottom on view edge");
    }

    #[test]
    fn flash_stays_locked_to_weapon_leftoffset() {
        // Freedoom pistol vs muzzle flash (real lump offsets).
        let gun = patch(82, 92, -125, -97);
        let flash = patch(32, 30, -144, -69);
        let g = psprite_pos(&gun, 0.0, 0.0, true);
        let f = psprite_pos(&flash, 0.0, 0.0, true);
        // Relative placement equals leftoverset / topoffset delta.
        assert!(((f.x - g.x) - (-125.0 - -144.0)).abs() < 1e-3);
        assert!(((f.y - g.y) - (-97.0 - -69.0)).abs() < 1e-3);
        // Flash must sit on the barrel (right of the gun's left edge, not screen-centered).
        assert!(f.x > g.x);
        assert!(f.x < g.x + 82.0);
    }

    #[test]
    fn fire_frame_keeps_origin_when_width_changes() {
        // Shotgun ready vs fire: different widths; leftoverset keeps the grip fixed.
        let ready = patch(67, 62, -122, -107);
        let fire = patch(88, 128, -78, -40);
        let a = psprite_pos(&ready, 0.0, 0.0, true);
        let b = psprite_pos(&fire, 0.0, 0.0, true);
        assert!((a.x - 122.0).abs() < 1e-3);
        assert!((b.x - 78.0).abs() < 1e-3);
        // Width-centering would put both near 160-w/2 and look like a jump to the left.
        let center_jump = (160.0 - 67.0 / 2.0) - (160.0 - 88.0 / 2.0);
        let origin_jump = a.x - b.x;
        assert!(
            (origin_jump - center_jump).abs() > 1.0,
            "test should distinguish leftoverset from width-centering"
        );
    }
}
