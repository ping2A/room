//! Title / credit screen (M5/M13).

use crate::draw::{Rect, Rgba, UiDrawList, Vec2};

/// Draw the title board: `pic` when set, else a branded fallback.
pub fn draw_title(list: &mut UiDrawList, width: f32, height: f32, pic: Option<&str>) {
    list.virtual_w = width;
    list.virtual_h = height;
    if let Some(name) = pic {
        list.patch(name, Vec2::new(0.0, 0.0), Vec2::new(width, height));
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

/// Lump to show for a `TITLEDEF` sequence. Loops from the first still.
#[must_use]
pub fn title_anim_pic(frames: u32, fps: u16, elapsed_ms: u64) -> String {
    anim_pic_loop("TITL", frames, fps, elapsed_ms)
}

/// How long a splash sequence holds, in milliseconds.
#[must_use]
pub fn splash_ms(splash: Option<(u32, u16)>) -> u64 {
    splash
        .filter(|(n, _)| *n > 0)
        .map(|(n, fps)| (u64::from(n) * 1000).div_ceil(u64::from(fps.max(1))))
        .unwrap_or(0)
}

/// 3D Realms splash (`RADL`) once, then the looping Duke title (`TITL`).
/// Second value is `true` while the splash is still playing.
#[must_use]
pub fn title_screen_pic(
    splash: Option<(u32, u16)>,
    logo: Option<(u32, u16)>,
    elapsed_ms: u64,
) -> (String, bool) {
    if let Some((n, fps)) = splash.filter(|(n, _)| *n > 0) {
        let fps_u = u64::from(fps.max(1));
        let dur = (u64::from(n) * 1000).div_ceil(fps_u);
        if elapsed_ms < dur {
            return (anim_pic("RADL", n, fps, elapsed_ms), true);
        }
        let rest = elapsed_ms.saturating_sub(dur);
        if let Some((tn, tf)) = logo.filter(|(n, _)| *n > 0) {
            return (title_anim_pic(tn, tf, rest), false);
        }
        return ("TITLEPIC".into(), false);
    }
    if let Some((tn, tf)) = logo.filter(|(n, _)| *n > 0) {
        return (title_anim_pic(tn, tf, elapsed_ms), false);
    }
    ("TITLEPIC".into(), false)
}

/// `{prefix}00`… for a `TITLEDEF`-style sequence. Plays once, then holds.
#[must_use]
pub fn anim_pic(prefix: &str, frames: u32, fps: u16, elapsed_ms: u64) -> String {
    frame_pic(prefix, frames, fps, elapsed_ms, false)
}

/// `{prefix}00`… looping from the first still (Duke `LOGO.ANM` ring).
#[must_use]
pub fn anim_pic_loop(prefix: &str, frames: u32, fps: u16, elapsed_ms: u64) -> String {
    frame_pic(prefix, frames, fps, elapsed_ms, true)
}

fn frame_pic(prefix: &str, frames: u32, fps: u16, elapsed_ms: u64, looping: bool) -> String {
    if frames == 0 {
        return prefix.to_owned();
    }
    let fps = u64::from(fps.max(1));
    let n = u64::from(frames);
    let i = elapsed_ms.saturating_mul(fps) / 1000;
    let i = if looping { i % n } else { i.min(n - 1) };
    format!("{prefix}{i:02}")
}

/// `TITLEDEF` body: `frames N` and `fps F`.
#[must_use]
pub fn parse_titledef(text: &str) -> Option<(u32, u16)> {
    let mut frames = 0u32;
    let mut fps = 10u16;
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match (words.next(), words.next()) {
            (Some("frames"), Some(n)) => frames = n.parse().ok()?,
            (Some("fps"), Some(n)) => fps = n.parse().unwrap_or(10),
            _ => {}
        }
    }
    (frames > 0).then_some((frames, fps))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_emits_fill_and_text() {
        let mut list = UiDrawList::new();
        draw_title(&mut list, 320.0, 200.0, None);
        assert!(list.cmds.len() >= 3);
    }

    #[test]
    fn titlepic_emits_patch() {
        let mut list = UiDrawList::new();
        draw_title(&mut list, 320.0, 200.0, Some("TITLEPIC"));
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            crate::draw::DrawCmd::Patch { name, .. } if name == "TITLEPIC"
        )));
    }

    #[test]
    fn title_anim_loops_and_finale_holds() {
        assert_eq!(title_anim_pic(10, 10, 0), "TITL00");
        assert_eq!(title_anim_pic(10, 10, 100), "TITL01");
        assert_eq!(title_anim_pic(10, 10, 1_000), "TITL00");
        assert_eq!(anim_pic("E1AN", 8, 10, 200), "E1AN02");
        assert_eq!(anim_pic("E1AN", 8, 10, 50_000), "E1AN07");
        assert_eq!(parse_titledef("frames 63\nfps 10\n"), Some((63, 10)));
        let (pic, splash) = title_screen_pic(Some((5, 10)), Some((10, 10)), 200);
        assert_eq!(pic, "RADL02");
        assert!(splash);
        let (pic, splash) = title_screen_pic(Some((5, 10)), Some((10, 10)), 600);
        assert_eq!(pic, "TITL01");
        assert!(!splash);
        assert_eq!(splash_ms(Some((5, 10))), 500);
        let (_, splash) = title_screen_pic(Some((5, 10)), Some((10, 10)), splash_ms(Some((5, 10))));
        assert!(!splash);
    }
}
