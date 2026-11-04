//! Draw ACS `HudMessage`s (GZDoom `DHUDMessage::Draw` positioning) on the
//! letterboxed 320×200 HUD layer.

use crate::doom_text::{draw_stcfn_string, measure_stcfn_string};
use crate::draw::{UiDrawList, Vec2};
use room_sim::AcsHudMessage;

const VW: f32 = 320.0;
const VH: f32 = 200.0;
/// Small-font line height.
const LINE_H: f32 = 9.0;

/// Fonts drawn as text; any other `SetFont` name is an image drawn as-is.
fn is_text_font(font: &str) -> bool {
    matches!(
        font.to_ascii_lowercase().as_str(),
        "" | "smallfont"
            | "smallfont2"
            | "bigfont"
            | "dbigfont"
            | "confont"
            | "consolefont"
            | "newsmallfont"
            | "newconsolefont"
            | "originalsmallfont"
            | "originalbigfont"
            | "stcfn"
    )
}

/// Remove `\x1c` text-colour codes (`\x1cX` or `\x1c[Name]`).
#[must_use]
pub fn strip_color_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\u{1c}' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('[') => {
                for d in it.by_ref() {
                    if d == ']' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// GZDoom `EColorRange` order (`CR_BRICK` = 0 … `CR_TEAL` = 25) as
/// approximate dark → bright ramps (textcolors.txt).
const COLOR_RAMPS: [(&str, [u8; 3], [u8; 3]); 26] = [
    ("brick", [0x47, 0x00, 0x00], [0xff, 0xb8, 0xb8]),
    ("tan", [0x33, 0x2b, 0x13], [0xff, 0xeb, 0xdf]),
    ("gray", [0x27, 0x27, 0x27], [0xef, 0xef, 0xef]),
    ("green", [0x0b, 0x17, 0x07], [0x77, 0xff, 0x6f]),
    ("brown", [0x53, 0x3f, 0x2f], [0xbf, 0xa7, 0x8f]),
    ("gold", [0x73, 0x2b, 0x00], [0xff, 0xff, 0x73]),
    ("red", [0x3f, 0x00, 0x00], [0xff, 0x00, 0x00]),
    ("blue", [0x00, 0x00, 0x7f], [0x60, 0x60, 0xff]),
    ("orange", [0x20, 0x00, 0x00], [0xff, 0xff, 0x80]),
    ("white", [0x24, 0x24, 0x24], [0xff, 0xff, 0xff]),
    ("yellow", [0x27, 0x27, 0x27], [0xff, 0xff, 0x00]),
    ("untranslated", [0, 0, 0], [0, 0, 0]),
    ("black", [0x13, 0x13, 0x13], [0x50, 0x50, 0x50]),
    ("lightblue", [0x00, 0x00, 0x73], [0xb4, 0xb4, 0xff]),
    ("cream", [0xcf, 0x83, 0x53], [0xff, 0xd7, 0xbb]),
    ("olive", [0x2f, 0x37, 0x1f], [0x7b, 0x7f, 0x50]),
    ("darkgreen", [0x0b, 0x17, 0x07], [0x2b, 0x93, 0x27]),
    ("darkred", [0x2b, 0x00, 0x00], [0xa0, 0x00, 0x00]),
    ("darkbrown", [0x1f, 0x17, 0x0b], [0xa3, 0x73, 0x3f]),
    ("purple", [0x23, 0x00, 0x23], [0xff, 0x00, 0xff]),
    ("darkgray", [0x23, 0x23, 0x23], [0x8b, 0x8b, 0x8b]),
    ("cyan", [0x00, 0x1f, 0x1f], [0x00, 0xf0, 0xf0]),
    ("ice", [0x34, 0x34, 0x50], [0xe0, 0xe0, 0xff]),
    ("fire", [0x66, 0x00, 0x00], [0xff, 0xff, 0x00]),
    ("sapphire", [0x00, 0x04, 0x68], [0x50, 0xa6, 0xff]),
    ("teal", [0x00, 0x1f, 0x1f], [0x23, 0xff, 0xdf]),
];

/// `CR_UNTRANSLATED`.
const CR_UNTRANSLATED: i32 = 11;

fn ramp(cr: i32) -> Option<[[f32; 3]; 2]> {
    if cr == CR_UNTRANSLATED {
        return None;
    }
    let (_, lo, hi) = COLOR_RAMPS.get(usize::try_from(cr).ok()?)?;
    let f = |c: [u8; 3]| {
        [
            c[0] as f32 / 255.0,
            c[1] as f32 / 255.0,
            c[2] as f32 / 255.0,
        ]
    };
    Some([f(*lo), f(*hi)])
}

/// `CR_*` index of a colour name (`V_FindFontColor`), or untranslated.
#[must_use]
pub fn color_by_name(name: &str) -> i32 {
    let n = name.trim().to_ascii_lowercase().replace([' ', '_'], "");
    let n = match n.as_str() {
        "grey" => "gray".to_owned(),
        "darkgrey" => "darkgray".to_owned(),
        _ => n,
    };
    COLOR_RAMPS
        .iter()
        .position(|(k, _, _)| *k == n)
        .map_or(CR_UNTRANSLATED, |i| i as i32)
}

/// One run of text in a single colour.
#[derive(Clone, Debug, PartialEq)]
struct Run {
    color: i32,
    text: String,
}

/// Split a line at `\x1c` colour codes (`\cX`, `\c[Name]`, `\c-` = message colour).
fn color_runs(line: &str, base: i32) -> Vec<Run> {
    let mut runs = vec![Run {
        color: base,
        text: String::new(),
    }];
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\u{1c}' {
            if let Some(r) = runs.last_mut() {
                r.text.push(c);
            }
            continue;
        }
        let color = match it.next() {
            Some('[') => {
                let mut name = String::new();
                for d in it.by_ref() {
                    if d == ']' {
                        break;
                    }
                    name.push(d);
                }
                color_by_name(&name)
            }
            Some(ch @ ('a'..='z' | 'A'..='Z')) => i32::from(ch.to_ascii_lowercase() as u8 - b'a'),
            _ => base,
        };
        runs.push(Run {
            color,
            text: String::new(),
        });
    }
    runs.retain(|r| !r.text.is_empty());
    runs
}

/// Draw the live messages visible to `seat`.
///
/// `aspect` is the window width / height: GZDoom lays HUD messages out over
/// the whole window, so coordinates extend past the letterboxed 320×200 box.
pub fn draw_acs_hud_messages(
    list: &mut UiDrawList,
    msgs: &[AcsHudMessage],
    seat: usize,
    statusbar_visible: bool,
    aspect: f32,
    patch_size: &dyn Fn(&str) -> Option<(f32, f32)>,
) {
    let aspect = if aspect.is_finite() && aspect > 0.1 {
        aspect
    } else {
        VW / VH
    };
    let full_w = VW.max(VH * aspect);
    let full_h = VH.max(VW / aspect);
    let left = (VW - full_w) / 2.0;
    let top = (VH - full_h) / 2.0;
    let bottom = if statusbar_visible {
        168.0
    } else {
        top + full_h
    };
    for m in msgs {
        if m.seat.is_some_and(|s| s != seat) {
            continue;
        }
        let alpha = m.current_alpha();
        if alpha <= 0.004 {
            continue;
        }
        let base = if m.color_name.is_some() {
            m.color_name
                .as_deref()
                .map_or(CR_UNTRANSLATED, color_by_name)
        } else {
            m.color
        };
        let mut budget = m.typed_chars().unwrap_or(usize::MAX);
        let mut lines: Vec<Vec<Run>> = Vec::new();
        for line in m.text.split('\n') {
            let mut runs = color_runs(line, base);
            for r in &mut runs {
                let keep: String = r.text.chars().take(budget).collect();
                budget = budget.saturating_sub(keep.chars().count());
                r.text = keep;
            }
            lines.push(runs);
        }
        let image = !is_text_font(&m.font);
        let line_w = |runs: &[Run]| {
            runs.iter()
                .map(|r| measure_stcfn_string(&r.text, 1.0))
                .sum::<f32>()
        };
        // Message box size in the message's own units.
        let (w, h) = if image {
            patch_size(&m.font).unwrap_or((0.0, 0.0))
        } else {
            (
                lines.iter().map(|l| line_w(l)).fold(0.0, f32::max),
                LINE_H * lines.len() as f32,
            )
        };
        let (sx, sy, mut x, y, center_x) = if m.hud_w == 0 || m.hud_h == 0 {
            // GZDoom DHUDMessage: fractions of the screen (see constructor notes).
            let (lx, center_x) = if m.x.abs() > 2.0 {
                (0.5, true)
            } else {
                let l = if m.x < -1.0 {
                    m.x + 1.0
                } else if m.x > 1.0 {
                    m.x - 1.0
                } else {
                    m.x
                };
                (l, m.x.abs() > 1.0)
            };
            let area_h = bottom - top;
            let x = if lx > 0.0 {
                left + (full_w - w) * lx
            } else {
                left + full_w * -lx
            };
            let y = if m.y > 0.0 {
                top + (area_h - h) * m.y
            } else {
                top + area_h * -m.y
            };
            (1.0, 1.0, x, y, center_x)
        } else {
            // SetHudSize: pixels of a virtual screen stretched over the window;
            // a positive height stops at the status bar.
            let area_h = if m.hud_h < 0 { full_h } else { bottom - top };
            let hud_h = m.hud_h.unsigned_abs() as f32;
            let sx = full_w / m.hud_w as f32;
            let sy = area_h / hud_h.max(1.0);
            let frac = |v: f32| ((v.fract().abs() * 10.0) + 0.5) as i32;
            let fx = frac(m.x);
            let center_x = fx & 4 != 0;
            let mut x = m.x.trunc();
            match fx & 3 {
                0 => x -= w / 2.0,
                2 => x -= w,
                _ => {}
            }
            let fy = frac(m.y);
            let mut y = m.y.trunc();
            match fy & 3 {
                0 => y -= h / 2.0,
                2 => y -= h,
                _ => {}
            }
            (sx, sy, left + x * sx, top + y * sy, center_x)
        };
        if center_x {
            x += w * sx / 2.0;
        }
        if image {
            // An image "font" (SetFont("GRAPHIC")) draws the picture itself.
            let size = if w > 0.0 {
                Vec2::new(w * sx, h * sy)
            } else {
                Vec2::new(0.0, 0.0)
            };
            let x = if center_x { x - w * sx / 2.0 } else { x };
            let start = list.cmds.len();
            list.patch(m.font.as_str(), Vec2::new(x, y), size);
            list.style_since(start, alpha, None);
            continue;
        }
        let scale = sx.min(sy).clamp(0.25, 4.0);
        let mut ly = y;
        for runs in &lines {
            let mut lx = if center_x {
                x - line_w(runs) * sx / 2.0
            } else {
                x
            };
            for r in runs {
                let start = list.cmds.len();
                draw_stcfn_string(list, lx, ly, &r.text, scale);
                list.style_since(start, alpha, ramp(r.color));
                lx += measure_stcfn_string(&r.text, 1.0) * sx;
            }
            ly += LINE_H * sy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(text: &str, x: f32, y: f32) -> AcsHudMessage {
        AcsHudMessage::from_args(
            &room_acs::HudMessageArgs {
                text: text.into(),
                type_flags: 1,
                id: 7,
                color: 6,
                color_name: None,
                x,
                y,
                hold: 1.0,
                opt: vec![],
                hud_w: 0,
                hud_h: 0,
                font: String::new(),
                bold: false,
                activator: 0,
            },
            None,
        )
    }

    fn glyph_positions(list: &UiDrawList) -> Vec<(f32, f32)> {
        list.cmds
            .iter()
            .filter_map(|c| match c {
                crate::draw::DrawCmd::Patch { pos, .. } => Some((pos.x, pos.y)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn scouter_lines_center_at_the_top() {
        // Brutal Doom's scouter: HudMessage("\n40/40"; HUDMSG_FADEOUT, id, …, 0.5, 0.02, …).
        let mut list = UiDrawList::new();
        draw_acs_hud_messages(
            &mut list,
            &[msg("\n40/40", 0.5, 0.02)],
            0,
            true,
            1.6,
            &|_| None,
        );
        let g = glyph_positions(&list);
        assert_eq!(g.len(), 5, "no backslash glyphs: {g:?}");
        let w = measure_stcfn_string("40/40", 1.0);
        assert!((g[0].0 - (VW - w) / 2.0).abs() < 0.5, "{g:?}");
        assert!(g[0].1 > 0.0 && g[0].1 < 20.0, "{g:?}");
    }

    #[test]
    fn other_seats_messages_are_hidden() {
        let mut m = msg("hi", 0.5, 0.5);
        m.seat = Some(1);
        let mut list = UiDrawList::new();
        draw_acs_hud_messages(&mut list, &[m], 0, true, 1.6, &|_| None);
        assert!(list.cmds.is_empty());
    }

    #[test]
    fn color_codes_split_runs() {
        let runs = color_runs("\u{1c}gRed \u{1c}[Gold]gold\u{1c}-x", 9);
        let got: Vec<_> = runs.iter().map(|r| (r.color, r.text.as_str())).collect();
        assert_eq!(got, vec![(6, "Red "), (5, "gold"), (9, "x")]);
    }

    #[test]
    fn widescreen_messages_use_the_whole_window() {
        // x = 0.0 (left edge) on a 16:9 window starts left of the 4:3 box.
        let mut list = UiDrawList::new();
        draw_acs_hud_messages(
            &mut list,
            &[msg("hi", 0.0, 0.5)],
            0,
            true,
            16.0 / 9.0,
            &|_| None,
        );
        let g = glyph_positions(&list);
        let full_w = VH * 16.0 / 9.0;
        assert!((g[0].0 - (VW - full_w) / 2.0).abs() < 0.5, "{g:?}");
    }

    #[test]
    fn color_codes_are_stripped() {
        assert_eq!(strip_color_codes("\u{1c}Dok \u{1c}[Gold]go"), "ok go");
    }
}
