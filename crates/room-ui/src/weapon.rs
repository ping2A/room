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
    draw_weapon_lit(list, patch, fullscreen, soft, bob_x, bob_y, 1.0, false, false);
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
) {
    if patch.is_empty() {
        return;
    }
    let (pos, size) = if let Some(p) = soft {
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
    let lit = if bright { 1.0 } else { light.clamp(0.05, 1.0) };
    list.patch_lit(patch, pos, size, lit, fuzz);
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
