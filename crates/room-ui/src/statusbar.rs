//! Doom status bar draw (`STBAR` + `STTNUM*` patches).

use crate::doom_text::{draw_stt_percent, draw_stt_string, draw_stt_string_right};
use crate::draw::{Rect, Rgba, UiDrawList, Vec2};
use crate::hud::HudState;

/// Status bar drawer.
pub trait StatusBar {
    /// Append draw cmds for the current HUD state.
    fn draw(&self, hud: &HudState, list: &mut UiDrawList);
}

/// Classic Doom bottom bar (virtual 320×32 strip at y=168).
#[derive(Clone, Debug, Default)]
pub struct DoomStatusBar;

impl StatusBar for DoomStatusBar {
    fn draw(&self, hud: &HudState, list: &mut UiDrawList) {
        if hud.fullscreen {
            // Overlay numbers in the corners — no plate covering the weapon.
            draw_stt_string(
                list,
                4.0,
                176.0,
                &format!("{}%", hud.health.clamp(0, 999)),
                1.0,
            );
            draw_stt_string(
                list,
                4.0,
                160.0,
                &format!("{}", hud.ammo.clamp(0, 999)),
                1.0,
            );
            draw_stt_string(
                list,
                248.0,
                176.0,
                &format!("{}%", hud.armor.clamp(0, 999)),
                1.0,
            );
            let mut kx = 292.0;
            for (bit, patch) in [
                (1u8, "STKEYS0"),
                (2, "STKEYS1"),
                (4, "STKEYS2"),
                (8, "STKEYS3"),
                (16, "STKEYS4"),
                (32, "STKEYS5"),
            ] {
                if hud.keys & bit != 0 {
                    list.patch(patch, Vec2::new(kx, 184.0), Vec2::new(8.0, 16.0));
                    kx -= 10.0;
                }
            }
            return;
        }

        // Classic STBAR: background art, STTNUM in the stock slots, mugshot.
        // Doom `st_stuff.c` X coords are the **right** edge of each number widget.
        list.fill(Rect::new(0.0, 168.0, 320.0, 32.0), Rgba::STBAR_BG);
        list.patch("STBAR", Vec2::new(0.0, 168.0), Vec2::new(320.0, 32.0));
        draw_stt_string_right(
            list,
            44.0,
            171.0,
            &format!("{}", hud.ammo.clamp(0, 999)),
            1.0,
        );
        draw_stt_percent(list, 90.0, 171.0, hud.health.clamp(0, 999), 1.0);
        if hud.keys & 1 != 0 {
            list.patch("STKEYS0", Vec2::new(239.0, 171.0), Vec2::new(8.0, 16.0));
        }
        if hud.keys & 8 != 0 {
            list.patch("STKEYS3", Vec2::new(249.0, 171.0), Vec2::new(8.0, 16.0));
        }
        if hud.keys & 2 != 0 {
            list.patch("STKEYS1", Vec2::new(239.0, 181.0), Vec2::new(8.0, 16.0));
        }
        if hud.keys & 16 != 0 {
            list.patch("STKEYS4", Vec2::new(249.0, 181.0), Vec2::new(8.0, 16.0));
        }
        if hud.keys & 4 != 0 {
            list.patch("STKEYS2", Vec2::new(239.0, 191.0), Vec2::new(8.0, 16.0));
        }
        if hud.keys & 32 != 0 {
            list.patch("STKEYS5", Vec2::new(249.0, 191.0), Vec2::new(8.0, 16.0));
        }
        list.patch(&hud.face, Vec2::new(143.0, 168.0), Vec2::new(32.0, 32.0));
        draw_stt_percent(list, 221.0, 171.0, hud.armor.clamp(0, 999), 1.0);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::draw::DrawCmd;

    #[test]
    fn default_hud_is_classic_stbar() {
        let hud = HudState::doom_default();
        assert!(
            !hud.fullscreen,
            "classic STBAR is the default (F5 toggles overlay)"
        );
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        assert!(
            list.cmds
                .iter()
                .any(|c| matches!(c, DrawCmd::Patch { name, .. } if name == "STBAR")),
            "default HUD must draw STBAR"
        );
        assert!(
            list.cmds
                .iter()
                .any(|c| matches!(c, DrawCmd::Patch { name, .. } if name == "STFST00")),
            "default HUD must draw the mugshot"
        );
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            DrawCmd::Patch { name, .. } if name.starts_with("STTNUM")
        )));
        assert!(
            !list
                .cmds
                .iter()
                .any(|c| matches!(c, DrawCmd::Fill { rect, .. } if rect.h >= 40.0)),
            "must not cover the weapon with a HUD plate"
        );
    }

    #[test]
    fn fullscreen_hud_emits_sttnum_without_plate() {
        let bar = DoomStatusBar;
        let mut hud = HudState::doom_default();
        hud.fullscreen = true;
        hud.health = 100;
        hud.ammo = 50;
        hud.keys = 0b111;
        let mut list = UiDrawList::new();
        bar.draw(&hud, &mut list);
        assert!(
            !list.cmds.iter().any(|c| matches!(c, DrawCmd::Fill { .. })),
            "fullscreen overlay must not paint a plate over the gun"
        );
        let stt: Vec<_> = list
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Patch { name, size, .. }
                    if name.starts_with("STTNUM") || name == "STTPRCNT" =>
                {
                    Some((name.as_str(), size.y))
                }
                _ => None,
            })
            .collect();
        assert!(stt.len() >= 4, "expected STTNUM digits, got {stt:?}");
        assert!(
            stt.iter().all(|(_, h)| (*h - 16.0).abs() < 1e-3),
            "overlay digits stay native STTNUM size, got {stt:?}"
        );
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            DrawCmd::Patch { name, .. } if name == "STKEYS0"
        )));
    }

    #[test]
    fn doom_bar_draws_skull_key_patches() {
        let bar = DoomStatusBar;
        let mut hud = HudState::doom_default();
        hud.fullscreen = false;
        hud.keys = 0b111_000;
        let mut list = UiDrawList::new();
        bar.draw(&hud, &mut list);
        for name in ["STKEYS3", "STKEYS4", "STKEYS5"] {
            assert!(
                list.cmds
                    .iter()
                    .any(|c| matches!(c, DrawCmd::Patch { name: n, .. } if n == name)),
                "missing {name}"
            );
        }
    }

    #[test]
    fn doom_bar_emits_stbar_and_sttnum() {
        let bar = DoomStatusBar;
        let mut hud = HudState::doom_default();
        hud.fullscreen = false;
        let mut list = UiDrawList::new();
        bar.draw(&hud, &mut list);
        assert!(
            list.cmds
                .iter()
                .any(|c| matches!(c, DrawCmd::Patch { name, .. } if name == "STBAR"))
        );
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            DrawCmd::Patch { name, .. } if name.starts_with("STTNUM")
        )));
    }

    #[test]
    fn doom_bar_numbers_right_align_to_stock_x() {
        // ST_AMMOX=44, ST_HEALTHX=90, ST_ARMORX=221 — right edges in Doom.
        let bar = DoomStatusBar;
        let mut hud = HudState::doom_default();
        hud.fullscreen = false;
        hud.ammo = 50;
        hud.health = 100;
        hud.armor = 0;
        let mut list = UiDrawList::new();
        bar.draw(&hud, &mut list);
        // Ammo widget lives in the left slot (right edge 44); ignore health digits.
        let ammo_right_edges: Vec<f32> = list
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Patch { name, pos, size }
                    if name.starts_with("STTNUM") && pos.x + size.x <= 44.5 =>
                {
                    Some(pos.x + size.x)
                }
                _ => None,
            })
            .collect();
        assert!(!ammo_right_edges.is_empty(), "expected ammo digits");
        let right = ammo_right_edges
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (right - 44.0).abs() < 0.5,
            "ammo should end at x=44, got {right} ({ammo_right_edges:?})"
        );
        let pct_at_health = list.cmds.iter().any(|c| {
            matches!(
                c,
                DrawCmd::Patch { name, pos, .. }
                    if name == "STTPRCNT" && (pos.x - 90.0).abs() < 0.5
            )
        });
        assert!(pct_at_health, "health % should sit at x=90");
    }
}
