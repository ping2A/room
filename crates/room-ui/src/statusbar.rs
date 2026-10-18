//! Doom status bar draw (`STBAR` + `STTNUM*` patches).

use crate::doom_text::{draw_stt_percent, draw_stt_string, draw_stt_string_right, draw_stys_num};
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

/// Vanilla key boxes: one per colour, the skull key wins over the card.
fn key_box(keys: u8, row: u8) -> Option<String> {
    if keys & (8 << row) != 0 {
        Some(format!("STKEYS{}", row + 3))
    } else if keys & (1 << row) != 0 {
        Some(format!("STKEYS{row}"))
    } else {
        None
    }
}

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
            if hud.ammo >= 0 {
                draw_stt_string(list, 4.0, 160.0, &format!("{}", hud.ammo.min(999)), 1.0);
            }
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
                    list.picture(patch, Vec2::new(kx, 184.0));
                    kx -= 10.0;
                }
            }
            return;
        }

        // Classic STBAR, positions from `st_stuff.c` / GZDoom `sbarinfo/doom.txt`.
        // Number X coords are the **right** edge of each widget.
        list.fill(Rect::new(0.0, 168.0, 320.0, 32.0), Rgba::STBAR_BG);
        list.patch("STBAR", Vec2::new(0.0, 168.0), Vec2::new(320.0, 32.0));
        if hud.ammo >= 0 {
            draw_stt_string_right(list, 44.0, 171.0, &format!("{}", hud.ammo.min(999)), 1.0);
        }
        draw_stt_percent(list, 90.0, 171.0, hud.health.clamp(0, 999), 1.0);
        if hud.deathmatch {
            // `ST_FRAGSX` 138, two digits wide (`-9..=99`).
            draw_stt_string_right(
                list,
                138.0,
                171.0,
                &hud.frags.clamp(-9, 99).to_string(),
                1.0,
            );
        } else {
            list.picture("STARMS", Vec2::new(104.0, 168.0));
            for slot in 2u8..=7 {
                let i = f32::from(slot - 2);
                let font = if hud.arms & (1 << slot) != 0 {
                    "STYSNUM"
                } else {
                    "STGNUM"
                };
                list.picture(
                    &format!("{font}{slot}"),
                    Vec2::new(111.0 + (i % 3.0) * 12.0, 172.0 + (i / 3.0).floor() * 10.0),
                );
            }
        }
        if let Some(n) = hud.face_back {
            list.picture(&format!("STFB{}", n % 4), Vec2::new(143.0, 168.0));
        }
        if !hud.face.is_empty() {
            list.picture(&hud.face, Vec2::new(143.0, 168.0));
        }
        draw_stt_percent(list, 221.0, 171.0, hud.armor.clamp(0, 999), 1.0);
        for row in 0u8..3 {
            if let Some(name) = key_box(hud.keys, row) {
                list.picture(&name, Vec2::new(239.0, 171.0 + 10.0 * f32::from(row)));
            }
        }
        for (row, [count, max]) in hud.ammo_table.iter().enumerate() {
            let y = 173.0 + 6.0 * row as f32;
            draw_stys_num(list, 288.0, y, *count);
            draw_stys_num(list, 314.0, y, *max);
        }
    }
}

/// Duke Nukem 3D bottom bar from the `room-build` GRP conversion: `DUKEBAR`
/// (320×34 at y=166), `DKNUM*` big digits, `DKSM*` weapon numbers and the
/// `DKKEY[BYR]` access cards. Positions follow Duke's `coolgaugetext`.
#[derive(Clone, Debug, Default)]
pub struct DukeStatusBar;

impl DukeStatusBar {
    /// Lump that marks a converted Duke WAD.
    pub const MARKER: &'static str = "DUKEBAR";
}

/// Duke `digitalnumber`: digits centred on `x`, 8 px advance.
fn duke_number(list: &mut UiDrawList, x: f32, y: f32, n: i32) {
    let s = n.clamp(0, 999).to_string();
    let mut dx = x - (s.len() as f32 * 8.0 - 1.0) / 2.0;
    for c in s.chars() {
        list.picture(&format!("DKNUM{c}"), Vec2::new(dx.floor(), y));
        dx += 8.0;
    }
}

fn duke_keys(list: &mut UiDrawList, keys: u8, at: [(f32, f32); 3]) {
    for ((bits, suffix), (x, y)) in [(1 | 8, 'B'), (2 | 16, 'Y'), (4 | 32, 'R')]
        .into_iter()
        .zip(at)
    {
        if keys & bits != 0 {
            list.picture(&format!("DKKEY{suffix}"), Vec2::new(x, y));
        }
    }
}

impl StatusBar for DukeStatusBar {
    fn draw(&self, hud: &HudState, list: &mut UiDrawList) {
        if hud.fullscreen {
            duke_number(list, 24.0, 186.0, hud.health);
            if hud.armor > 0 {
                duke_number(list, 56.0, 186.0, hud.armor);
            }
            if hud.ammo >= 0 {
                duke_number(list, 296.0, 186.0, hud.ammo);
            }
            duke_keys(
                list,
                hud.keys,
                [(296.0, 160.0), (306.0, 160.0), (301.0, 170.0)],
            );
            return;
        }
        list.fill(Rect::new(0.0, 166.0, 320.0, 34.0), Rgba::STBAR_BG);
        list.patch(Self::MARKER, Vec2::new(0.0, 166.0), Vec2::new(320.0, 34.0));
        duke_number(list, 32.0, 183.0, hud.health);
        duke_number(list, 64.0, 183.0, hud.armor);
        if hud.ammo >= 0 {
            duke_number(list, 207.0, 183.0, hud.ammo);
        }
        for slot in 2u8..=9 {
            if hud.arms & (1 << slot) != 0 {
                let i = f32::from(slot - 2);
                list.picture(
                    &format!("DKSM{slot}"),
                    Vec2::new(96.0 + (i / 3.0).floor() * 39.0, 182.0 + (i % 3.0) * 6.0),
                );
            }
        }
        duke_keys(
            list,
            hud.keys,
            [(275.0, 182.0), (288.0, 182.0), (281.0, 189.0)],
        );
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

    fn patch_names(list: &UiDrawList) -> Vec<(String, Vec2, bool)> {
        list.cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Patch {
                    name,
                    pos,
                    apply_offsets,
                    ..
                } => Some((name.clone(), *pos, *apply_offsets)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn doom_bar_shows_one_key_per_colour_skull_first() {
        let mut hud = HudState::doom_default();
        hud.keys = 0b001_011; // blue card+skull, yellow card
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        let keys: Vec<_> = patch_names(&list)
            .into_iter()
            .filter(|(n, ..)| n.starts_with("STKEYS"))
            .map(|(n, p, _)| (n, p.x, p.y))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("STKEYS3".to_string(), 239.0, 171.0),
                ("STKEYS1".to_string(), 239.0, 181.0)
            ]
        );
    }

    #[test]
    fn doom_bar_draws_arms_ammo_table_and_offset_face() {
        let mut hud = HudState::doom_default();
        hud.arms = (1 << 2) | (1 << 5);
        hud.ammo_table = [[50, 200], [8, 50], [0, 50], [120, 300]];
        hud.face = "STFTL20".into();
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        let names = patch_names(&list);
        let has = |n: &str, x: f32, y: f32| {
            names.iter().any(|(m, p, off)| {
                m == n && *off && (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3
            })
        };
        assert!(has("STARMS", 104.0, 168.0));
        assert!(has("STYSNUM2", 111.0, 172.0), "slot 2 owned: {names:?}");
        assert!(has("STGNUM3", 123.0, 172.0));
        assert!(has("STYSNUM5", 111.0, 182.0));
        assert!(has("STGNUM7", 135.0, 182.0));
        assert!(has("STFTL20", 143.0, 168.0), "face drawn with Doom offsets");
        // Shell row: "8" right-aligned at 288, "50" at 314.
        assert!(has("STYSNUM8", 284.0, 179.0));
        assert!(has("STYSNUM5", 306.0, 179.0));
        assert!(has("STYSNUM0", 310.0, 179.0));
        // Cell row (4th): 120 / 300.
        assert!(has("STYSNUM1", 276.0, 191.0));
    }

    #[test]
    fn deathmatch_shows_frags_instead_of_arms_and_face_background() {
        let mut hud = HudState::doom_default().with_netgame(true, -3, Some(2));
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        let names = patch_names(&list);
        assert!(!names.iter().any(|(n, ..)| n == "STARMS"));
        let frag_digits: Vec<_> = names
            .iter()
            .filter(|(n, p, _)| {
                (n.starts_with("STTNUM") || n == "STTMINUS") && p.x < 139.0 && p.x > 100.0
            })
            .map(|(n, ..)| n.as_str())
            .collect();
        assert_eq!(frag_digits, ["STTMINUS", "STTNUM3"]);
        let back = names.iter().position(|(n, ..)| n == "STFB2");
        let face = names.iter().position(|(n, ..)| n == "STFST00");
        assert!(
            matches!((back, face), (Some(b), Some(f)) if b < f),
            "background drawn under the face: {names:?}"
        );

        hud = HudState::doom_default().with_netgame(false, 0, Some(1));
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        let names = patch_names(&list);
        assert!(names.iter().any(|(n, ..)| n == "STARMS"), "coop keeps ARMS");
        assert!(names.iter().any(|(n, ..)| n == "STFB1"));
    }

    #[test]
    fn doom_bar_blanks_ammo_for_fist() {
        let mut hud = HudState::doom_default();
        hud.ammo = -1;
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        assert!(
            !patch_names(&list)
                .iter()
                .any(|(n, p, _)| n.starts_with("STTNUM") && p.x < 44.5),
            "no big ammo digits for am_noammo weapons"
        );
    }

    #[test]
    fn duke_bar_draws_numbers_weapons_and_cards() {
        let mut hud = HudState::doom_default();
        hud.health = 100;
        hud.armor = 0;
        hud.ammo = 48;
        hud.arms = (1 << 2) | (1 << 9);
        hud.keys = 1 | 4;
        let mut list = UiDrawList::new();
        DukeStatusBar.draw(&hud, &mut list);
        let names = patch_names(&list);
        let has = |n: &str, x: f32, y: f32| {
            names
                .iter()
                .any(|(m, p, _)| m == n && (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3)
        };
        assert!(has("DUKEBAR", 0.0, 166.0));
        // "100" centred on x=32: 23 px wide, starts at 20.
        assert!(has("DKNUM1", 20.0, 183.0), "{names:?}");
        assert!(has("DKNUM0", 36.0, 183.0));
        assert!(has("DKNUM4", 199.0, 183.0), "ammo 48 centred on 207");
        assert!(has("DKSM2", 96.0, 182.0));
        assert!(has("DKSM9", 174.0, 188.0), "slot 9 in the third column");
        assert!(!names.iter().any(|(n, ..)| n == "DKSM3"));
        assert!(has("DKKEYB", 275.0, 182.0));
        assert!(has("DKKEYR", 281.0, 189.0));
        assert!(!names.iter().any(|(n, ..)| n == "DKKEYY"));

        hud.fullscreen = true;
        let mut list = UiDrawList::new();
        DukeStatusBar.draw(&hud, &mut list);
        let names = patch_names(&list);
        assert!(!names.iter().any(|(n, ..)| n == "DUKEBAR"));
        assert!(names.iter().any(|(n, ..)| n.starts_with("DKNUM")));
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
                DrawCmd::Patch {
                    name, pos, size, ..
                } if name.starts_with("STTNUM") && pos.x + size.x <= 44.5 => Some(pos.x + size.x),
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
