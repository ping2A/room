//! Intermission / end-of-level screen (`wi_stuff` subset).
//!
//! Doom 1 episodes use `WIMAP0`–`WIMAP2` with blood splats on finished nodes;
//! Doom II / commercial uses `INTERPIC`. Stats use `WI*` picture lumps.

use crate::doom_text::draw_stcfn_string;
use crate::draw::{Rect, Rgba, UiDrawList, Vec2};

/// End-of-level tallies shown on the intermission screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IntermissionStats {
    /// Map lump / episode label (`E1M1`, `MAP01`, …).
    pub map_name: String,
    /// Kills percent 0..=100.
    pub kills: u8,
    /// Items percent.
    pub items: u8,
    /// Secrets percent.
    pub secrets: u8,
    /// Time in tics.
    pub time_tics: u32,
    /// Par time in tics (0 = none).
    pub par_tics: u32,
}

/// Doom `lnodes[episode][map]` — blood-splat centres on `WIMAP*` (320×200).
///
/// Episode index is 0-based (`E1` → 0). Map index is 0-based (`M1` → 0).
const LNODES: &[[(i16, i16); 9]] = &[
    // Episode 1 — Knee-Deep in the Dead (`WIMAP0`)
    [
        (185, 164),
        (148, 143),
        (69, 122),
        (209, 102),
        (116, 89),
        (166, 55),
        (71, 56),
        (135, 29),
        (71, 24),
    ],
    // Episode 2 — The Shores of Hell (`WIMAP1`)
    [
        (254, 25),
        (97, 50),
        (188, 64),
        (128, 78),
        (214, 92),
        (133, 130),
        (208, 136),
        (148, 140),
        (235, 158),
    ],
    // Episode 3 — Inferno (`WIMAP2`)
    [
        (156, 168),
        (48, 154),
        (174, 95),
        (265, 75),
        (130, 48),
        (279, 23),
        (198, 48),
        (140, 25),
        (281, 136),
    ],
];

/// `WISPLAT` patch offsets (Doom picture header).
const SPLAT_LEFT: f32 = 15.0;
const SPLAT_TOP: f32 = 11.0;

/// Stats column (`SP_STATSX` / `SP_STATSY` in linuxdoom `wi_stuff.c`).
const SP_STATS_X: f32 = 50.0;
const SP_STATS_Y: f32 = 50.0;
const SP_TIME_X: f32 = 16.0;
const SP_TIME_Y: f32 = 168.0;

/// Title row (`WI_drawLF`).
const TITLE_Y: f32 = 2.0;
const FINISHED_Y: f32 = 24.0;

/// Draw a full-screen intermission board into `list` (virtual 320×200).
pub fn draw_intermission(
    list: &mut UiDrawList,
    stats: &IntermissionStats,
    width: f32,
    height: f32,
) {
    list.virtual_w = width;
    list.virtual_h = height;
    // Opaque base so letterbox / prior frames never show through.
    list.fill(
        Rect::new(0.0, 0.0, width, height),
        Rgba::new(0.0, 0.0, 0.0, 1.0),
    );

    let parsed = parse_map_ref(&stats.map_name);
    let bg = background_lump(parsed);
    list.patch(&bg, Vec2::new(0.0, 0.0), Vec2::new(width, height));

    if let Some((ep, map)) = parsed {
        if ep < 3 {
            // Blood splat on the level just finished.
            if let Some(&(nx, ny)) = LNODES.get(ep).and_then(|row| row.get(map)) {
                list.patch(
                    "WISPLAT",
                    Vec2::new(f32::from(nx) - SPLAT_LEFT, f32::from(ny) - SPLAT_TOP),
                    Vec2::new(0.0, 0.0),
                );
            }
            // Level name + "Finished".
            let wilv = format!("WILV{ep}{map}");
            list.patch(
                &wilv,
                Vec2::new(title_x(&wilv, width), TITLE_Y),
                Vec2::new(0.0, 0.0),
            );
            list.patch(
                "WIF",
                Vec2::new(title_x("WIF", width), FINISHED_Y),
                Vec2::new(0.0, 0.0),
            );
            draw_episode_stats(list, stats);
            return;
        }
    }

    // Doom II / unknown: INTERPIC + centered STCFN tallies (no episode map).
    draw_commercial_title(list, &stats.map_name, width);
    draw_commercial_stats(list, stats, width);
}

fn background_lump(parsed: Option<(usize, usize)>) -> String {
    match parsed {
        Some((ep, _)) if ep < 3 => format!("WIMAP{ep}"),
        _ => "INTERPIC".into(),
    }
}

/// `E1M1` → `(0, 0)`, `MAP01` → `None` (commercial).
fn parse_map_ref(name: &str) -> Option<(usize, usize)> {
    let n = name.trim().to_ascii_uppercase();
    if let Some(rest) = n.strip_prefix('E') {
        let mut chars = rest.chars();
        let ep = chars.next()?.to_digit(10)? as usize;
        if chars.next()? != 'M' {
            return None;
        }
        let map: String = chars.collect();
        let map = map.parse::<usize>().ok()?;
        if (1..=3).contains(&ep) && (1..=9).contains(&map) {
            return Some((ep - 1, map - 1));
        }
    }
    None
}

fn title_x(patch_hint: &str, width: f32) -> f32 {
    // Approximate centered titles; real patch widths vary — bias toward Doom's centre.
    let approx = match patch_hint {
        "WIF" => 92.0,
        s if s.starts_with("WILV") => 100.0,
        _ => 80.0,
    };
    ((width - approx) * 0.5).max(0.0)
}

fn draw_episode_stats(list: &mut UiDrawList, stats: &IntermissionStats) {
    let mut y = SP_STATS_Y;
    draw_stat_row(list, "WIOSTK", stats.kills, SP_STATS_X, y);
    y += 18.0;
    draw_stat_row(list, "WIOSTI", stats.items, SP_STATS_X, y);
    y += 18.0;
    draw_stat_row(list, "WIOSTS", stats.secrets, SP_STATS_X, y);

    list.patch(
        "WITIME",
        Vec2::new(SP_TIME_X, SP_TIME_Y),
        Vec2::new(0.0, 0.0),
    );
    draw_wi_time(list, SP_TIME_X + 44.0, SP_TIME_Y, stats.time_tics);
    if stats.par_tics > 0 {
        list.patch(
            "WIPAR",
            Vec2::new(SP_TIME_X + 120.0, SP_TIME_Y),
            Vec2::new(0.0, 0.0),
        );
        draw_wi_time(list, SP_TIME_X + 160.0, SP_TIME_Y, stats.par_tics);
    }
    // Small STCFN cue (vanilla advances on any key; keep an explicit prompt).
    draw_stcfn_string(list, 70.0, 188.0, "PRESS ENTER", 1.0);
}

fn draw_stat_row(list: &mut UiDrawList, label: &str, percent: u8, x: f32, y: f32) {
    list.patch(label, Vec2::new(x, y), Vec2::new(0.0, 0.0));
    draw_wi_percent(list, x + 100.0, y, percent);
}

fn draw_commercial_title(list: &mut UiDrawList, map_name: &str, width: f32) {
    let label = if map_name.is_empty() {
        "LEVEL COMPLETE"
    } else {
        map_name
    };
    let w = label.len() as f32 * 8.0;
    draw_stcfn_string(list, ((width - w) * 0.5).max(8.0), 12.0, label, 1.0);
    list.patch(
        "WIF",
        Vec2::new(title_x("WIF", width), 28.0),
        Vec2::new(0.0, 0.0),
    );
}

fn draw_commercial_stats(list: &mut UiDrawList, stats: &IntermissionStats, width: f32) {
    let x = width * 0.22;
    let mut y = 60.0;
    draw_stat_row(list, "WIOSTK", stats.kills, x, y);
    y += 20.0;
    draw_stat_row(list, "WIOSTI", stats.items, x, y);
    y += 20.0;
    draw_stat_row(list, "WIOSTS", stats.secrets, x, y);
    y += 28.0;
    list.patch("WITIME", Vec2::new(x, y), Vec2::new(0.0, 0.0));
    draw_wi_time(list, x + 44.0, y, stats.time_tics);
    if stats.par_tics > 0 {
        y += 18.0;
        list.patch("WIPAR", Vec2::new(x, y), Vec2::new(0.0, 0.0));
        draw_wi_time(list, x + 44.0, y, stats.par_tics);
    }
    draw_stcfn_string(list, x, height_prompt(list), "PRESS ENTER TO CONTINUE", 1.0);
}

fn height_prompt(list: &UiDrawList) -> f32 {
    list.virtual_h * 0.88
}

/// `WINUM*` + `WIPCNT` percentage (right-aligned to `x_right` like Doom).
fn draw_wi_percent(list: &mut UiDrawList, x_right: f32, y: f32, percent: u8) {
    let text = format!("{percent}");
    let mut x = x_right;
    // Digits right-aligned, then percent sign at `x_right`.
    let digit_w = 12.0;
    let chars: Vec<char> = text.chars().collect();
    x -= digit_w * chars.len() as f32;
    for ch in chars {
        list.patch(&format!("WINUM{ch}"), Vec2::new(x, y), Vec2::new(0.0, 0.0));
        x += digit_w;
    }
    list.patch("WIPCNT", Vec2::new(x_right, y), Vec2::new(0.0, 0.0));
}

fn draw_wi_time(list: &mut UiDrawList, mut x: f32, y: f32, tics: u32) {
    let secs = tics / 35;
    let mins = secs / 60;
    let secs = secs % 60;
    let text = format!("{mins}:{secs:02}");
    for ch in text.chars() {
        match ch {
            '0'..='9' => {
                list.patch(&format!("WINUM{ch}"), Vec2::new(x, y), Vec2::new(0.0, 0.0));
                x += 12.0;
            }
            ':' => {
                // Doom uses a colon patch when present; approximate with gap + minus.
                list.patch("WIMINUS", Vec2::new(x + 2.0, y + 4.0), Vec2::new(0.0, 0.0));
                x += 8.0;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_wimap_and_stats_for_e1m1() {
        let mut list = UiDrawList::new();
        draw_intermission(
            &mut list,
            &IntermissionStats {
                map_name: "E1M1".into(),
                kills: 100,
                items: 16,
                secrets: 0,
                time_tics: 35 * 34,
                par_tics: 35 * 30,
            },
            320.0,
            200.0,
        );
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            crate::draw::DrawCmd::Patch { name, .. } if name == "WIMAP0"
        )));
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            crate::draw::DrawCmd::Patch { name, .. } if name == "WISPLAT"
        )));
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            crate::draw::DrawCmd::Patch { name, .. } if name == "WILV00"
        )));
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            crate::draw::DrawCmd::Patch { name, .. } if name == "WIOSTK"
        )));
        assert!(list.cmds.len() >= 10);
    }

    #[test]
    fn commercial_uses_interpic() {
        let mut list = UiDrawList::new();
        draw_intermission(
            &mut list,
            &IntermissionStats {
                map_name: "MAP01".into(),
                kills: 50,
                items: 50,
                secrets: 50,
                time_tics: 35,
                par_tics: 0,
            },
            320.0,
            200.0,
        );
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            crate::draw::DrawCmd::Patch { name, .. } if name == "INTERPIC"
        )));
    }

    #[test]
    fn parse_exmy() {
        assert_eq!(parse_map_ref("E1M1"), Some((0, 0)));
        assert_eq!(parse_map_ref("e2m9"), Some((1, 8)));
        assert_eq!(parse_map_ref("MAP01"), None);
    }
}
