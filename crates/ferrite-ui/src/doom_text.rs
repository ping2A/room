//! Doom UI text as picture lumps (`STTNUM*` / `STCFN###`).
//!
//! SoftFont tinting can fail silently in the canvas path; these emit
//! [`DrawCmd::Patch`] commands that share the proven STBAR blit path.

use crate::draw::{UiDrawList, Vec2};

/// Status-bar digit cell size (Freedoom / Doom `STTNUM*`).
const STT_W: f32 = 14.0;
const STT_H: f32 = 16.0;

/// Approximate `STCFN` cell (variable width; advance uses a stable pitch).
const STCFN_W: f32 = 8.0;
const STCFN_H: f32 = 8.0;

/// Draw a numeric / percent string with `STTNUM*` (+ `STTPRCNT` / `STTMINUS`).
///
/// `scale` is in virtual 320×200 pixels (2.0 ≈ twice Doom's native digit size).
/// `x` is the **left** edge of the first glyph.
pub fn draw_stt_string(list: &mut UiDrawList, mut x: f32, y: f32, text: &str, scale: f32) {
    let scale = scale.max(0.5);
    for ch in text.chars() {
        let (name, w) = match ch {
            '0'..='9' => (format!("STTNUM{ch}"), STT_W),
            '%' => ("STTPRCNT".into(), STT_W),
            '-' => ("STTMINUS".into(), 8.0),
            '/' | '|' => ("STTMINUS".into(), 8.0),
            ' ' => {
                // Match digit cell width so `{:3}` padding right-aligns like Doom.
                x += STT_W * scale;
                continue;
            }
            _ => continue,
        };
        list.patch(&name, Vec2::new(x, y), Vec2::new(w * scale, STT_H * scale));
        x += w * scale;
    }
}

/// Like [`draw_stt_string`], but `x` is the **right** edge (Doom `STlib_drawNum`).
pub fn draw_stt_string_right(list: &mut UiDrawList, x_right: f32, y: f32, text: &str, scale: f32) {
    let w = measure_stt_string(text, scale);
    draw_stt_string(list, x_right - w, y, text, scale);
}

/// Doom `STlib_updatePercent`: digits right-aligned to `x`, then `STTPRCNT` at `x`.
pub fn draw_stt_percent(list: &mut UiDrawList, x_right: f32, y: f32, value: i32, scale: f32) {
    let scale = scale.max(0.5);
    let text = format!("{}", value.clamp(0, 999));
    draw_stt_string_right(list, x_right, y, &text, scale);
    list.patch(
        "STTPRCNT",
        Vec2::new(x_right, y),
        Vec2::new(STT_W * scale, STT_H * scale),
    );
}

/// Width of a `STT` string at `scale` (for layout).
#[must_use]
pub fn measure_stt_string(text: &str, scale: f32) -> f32 {
    let scale = scale.max(0.5);
    let mut w = 0.0;
    for ch in text.chars() {
        w += match ch {
            '0'..='9' | '%' => STT_W * scale,
            '-' | '/' | '|' => 8.0 * scale,
            ' ' => STT_W * scale,
            _ => 0.0,
        };
    }
    w
}

/// Draw uppercase Doom small-font text via `STCFN###` patches.
///
/// `scale == 1` uses each lump's native size (no 8×8 stretch). Larger scales
/// request a scaled destination; [`crate::patches::SoftPatch::paint`] keeps
/// aspect via uniform min(sx, sy).
pub fn draw_stcfn_string(list: &mut UiDrawList, mut x: f32, y: f32, text: &str, scale: f32) {
    let scale = scale.max(0.5);
    let native = (scale - 1.0).abs() < 0.01;
    for ch in text.chars() {
        if ch == ' ' {
            x += (STCFN_W * 0.75) * scale;
            continue;
        }
        let code = (ch.to_ascii_uppercase() as u8).max(33);
        let name = format!("STCFN{code:03}");
        let size = if native {
            Vec2::new(0.0, 0.0)
        } else {
            Vec2::new(STCFN_W * scale, STCFN_H * scale)
        };
        list.patch(&name, Vec2::new(x, y), size);
        x += (STCFN_W + 1.0) * scale;
    }
}

/// Pixel width of an `STCFN` string at `scale`.
#[must_use]
pub fn measure_stcfn_string(text: &str, scale: f32) -> f32 {
    let scale = scale.max(0.5);
    let mut w = 0.0;
    for ch in text.chars() {
        if ch == ' ' {
            w += (STCFN_W * 0.75) * scale;
        } else {
            w += (STCFN_W + 1.0) * scale;
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawCmd;

    #[test]
    fn stt_emits_digit_patches() {
        let mut list = UiDrawList::new();
        draw_stt_string(&mut list, 10.0, 20.0, "100%", 2.0);
        let names: Vec<_> = list
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Patch { name, size, .. } => Some((name.as_str(), size.x, size.y)),
                _ => None,
            })
            .collect();
        assert_eq!(names.len(), 4);
        assert_eq!(names[0].0, "STTNUM1");
        assert_eq!(names[3].0, "STTPRCNT");
        assert!((names[0].1 - 28.0).abs() < 1e-3); // 14 * 2
        assert!((names[0].2 - 32.0).abs() < 1e-3); // 16 * 2
    }

    #[test]
    fn stcfn_emits_uppercase_patches() {
        let mut list = UiDrawList::new();
        draw_stcfn_string(&mut list, 0.0, 0.0, "Resume", 2.0);
        let names: Vec<_> = list
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Patch { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            [
                "STCFN082", "STCFN069", "STCFN083", "STCFN085", "STCFN077", "STCFN069"
            ]
        );
    }

    #[test]
    fn stcfn_native_scale_uses_zero_size() {
        let mut list = UiDrawList::new();
        draw_stcfn_string(&mut list, 0.0, 0.0, "A", 1.0);
        match &list.cmds[..] {
            [DrawCmd::Patch { size, .. }] => {
                assert_eq!(size.x, 0.0);
                assert_eq!(size.y, 0.0);
            }
            other => panic!("expected one native patch, got {other:?}"),
        }
    }
}
