//! Title / credit screen (M5/M13).

use crate::draw::{Rect, Rgba, UiDrawList, Vec2};

/// Draw the title board: `TITLEPIC` when present, else a branded fallback.
pub fn draw_title(list: &mut UiDrawList, width: f32, height: f32, has_titlepic: bool) {
    list.virtual_w = width;
    list.virtual_h = height;
    if has_titlepic {
        list.patch("TITLEPIC", Vec2::new(0.0, 0.0), Vec2::new(width, height));
    } else {
        list.fill(
            Rect::new(0.0, 0.0, width, height),
            Rgba::new(0.05, 0.05, 0.08, 1.0),
        );
        list.text(
            "HudBig",
            Vec2::new(width * 0.5 - 40.0, height * 0.28),
            "ROOM",
            Rgba::new(0.85, 0.2, 0.15, 1.0),
            2.0,
        );
        list.text(
            "SmallFont",
            Vec2::new(width * 0.5 - 78.0, height * 0.48),
            "Press Enter / click to select",
            Rgba::new(0.75, 0.75, 0.75, 1.0),
            1.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_emits_fill_and_text() {
        let mut list = UiDrawList::new();
        draw_title(&mut list, 320.0, 200.0, false);
        assert!(list.cmds.len() >= 3);
    }

    #[test]
    fn titlepic_emits_patch() {
        let mut list = UiDrawList::new();
        draw_title(&mut list, 320.0, 200.0, true);
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            crate::draw::DrawCmd::Patch { name, .. } if name == "TITLEPIC"
        )));
    }
}
