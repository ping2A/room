//! Scoreboard (UZDoom `HU_DrawScores`): held with `+showscores`, and shown on
//! its own while the local player is dead in deathmatch.

use crate::doom_text::{draw_stcfn_string, measure_stcfn_string};
use crate::draw::{Rect, Rgba, UiDrawList};

/// One player's line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScoreLine {
    /// Player seat.
    pub seat: usize,
    /// Display name.
    pub name: String,
    /// Frags (deathmatch).
    pub frags: i32,
    /// Deaths (deathmatch).
    pub deaths: i32,
    /// Monsters killed.
    pub kills: u32,
    /// Items found.
    pub items: u32,
    /// Secrets found.
    pub secrets: u32,
}

/// Panel left edge, width and first row.
const LEFT: f32 = 24.0;
const WIDTH: f32 = 272.0;
const TOP: f32 = 40.0;
const ROW: f32 = 10.0;

fn right(list: &mut UiDrawList, x_right: f32, y: f32, text: &str) {
    draw_stcfn_string(
        list,
        x_right - measure_stcfn_string(text, 1.0),
        y,
        text,
        1.0,
    );
}

/// Draw `lines` (already in display order). Deathmatch shows frags and
/// deaths; cooperative shows kills, items and secrets out of the level
/// totals. The local player's line is highlighted.
pub fn draw_scoreboard(
    list: &mut UiDrawList,
    lines: &[ScoreLine],
    deathmatch: bool,
    local_seat: usize,
    totals: [u32; 3],
) {
    if lines.is_empty() {
        return;
    }
    let height = ROW * (lines.len() as f32 + 2.0) + 8.0;
    list.fill(
        Rect::new(LEFT, TOP - 4.0, WIDTH, height),
        Rgba::new(0.0, 0.0, 0.0, 0.6),
    );
    let cols: &[(&str, f32)] = if deathmatch {
        &[("FRAGS", 220.0), ("DEATHS", 284.0)]
    } else {
        &[("KILLS", 196.0), ("ITEMS", 240.0), ("SECRETS", 290.0)]
    };
    draw_stcfn_string(list, LEFT + 6.0, TOP, "NAME", 1.0);
    for (title, x) in cols {
        right(list, *x, TOP, title);
    }
    for (i, line) in lines.iter().enumerate() {
        let y = TOP + ROW * (i as f32 + 1.5);
        if line.seat == local_seat {
            list.fill(
                Rect::new(LEFT + 2.0, y - 1.0, WIDTH - 4.0, ROW),
                Rgba::new(0.85, 0.7, 0.2, 0.35),
            );
        }
        let name: String = line.name.chars().take(18).collect();
        draw_stcfn_string(list, LEFT + 6.0, y, &name, 1.0);
        if deathmatch {
            right(list, cols[0].1, y, &line.frags.to_string());
            right(list, cols[1].1, y, &line.deaths.to_string());
        } else {
            let pct = |n: u32, of: u32| match n.saturating_mul(100).checked_div(of) {
                Some(p) => format!("{p}%"),
                None => n.to_string(),
            };
            right(list, cols[0].1, y, &pct(line.kills, totals[0]));
            right(list, cols[1].1, y, &pct(line.items, totals[1]));
            right(list, cols[2].1, y, &pct(line.secrets, totals[2]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawCmd;

    fn glyph_rows(list: &UiDrawList) -> Vec<f32> {
        let mut ys: Vec<f32> = list
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Patch { name, pos, .. } if name.starts_with("STCFN") => Some(pos.y),
                _ => None,
            })
            .collect();
        ys.sort_by(f32::total_cmp);
        ys.dedup();
        ys
    }

    #[test]
    fn deathmatch_lists_every_player_with_the_local_line_marked() {
        let lines = vec![
            ScoreLine {
                seat: 1,
                name: "Bea".into(),
                frags: 7,
                deaths: 1,
                ..ScoreLine::default()
            },
            ScoreLine {
                seat: 0,
                name: "Al".into(),
                frags: 2,
                deaths: 5,
                ..ScoreLine::default()
            },
        ];
        let mut list = UiDrawList::new();
        draw_scoreboard(&mut list, &lines, true, 0, [0; 3]);
        // Header plus two lines.
        assert_eq!(glyph_rows(&list).len(), 3);
        // "7" right-aligned under FRAGS on the first line.
        let seven = list.cmds.iter().any(|c| {
            matches!(c,
            DrawCmd::Patch { name, pos, .. } if name == "STCFN055" && pos.y == TOP + ROW * 1.5)
        });
        assert!(seven);
        // The highlight sits on the second line (the local seat 0).
        let marked = list.cmds.iter().any(|c| {
            matches!(c,
            DrawCmd::Fill { rect, .. } if (rect.y - (TOP + ROW * 2.5 - 1.0)).abs() < 0.01)
        });
        assert!(marked);
    }

    #[test]
    fn cooperative_shows_percentages_of_the_level_totals() {
        let lines = vec![ScoreLine {
            seat: 0,
            name: "Al".into(),
            kills: 5,
            items: 1,
            secrets: 0,
            ..ScoreLine::default()
        }];
        let mut list = UiDrawList::new();
        draw_scoreboard(&mut list, &lines, false, 0, [10, 4, 0]);
        // 50% kills: '5', '0', '%' glyphs present on the player line.
        let y = TOP + ROW * 1.5;
        for code in ["STCFN053", "STCFN048", "STCFN037"] {
            assert!(
                list.cmds.iter().any(|c| matches!(c,
                DrawCmd::Patch { name, pos, .. } if name == code && pos.y == y)),
                "{code}"
            );
        }
    }
}
