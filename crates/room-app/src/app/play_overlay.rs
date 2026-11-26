//! In-game overlays laid out for any window shape: the automap rectangle and
//! the pause menu (phones open it with ☰).

use room_ui::{ListMenu, MenuItem};

/// The automap's rectangle in the HUD's 320×200 units (which are centred
/// and letterboxed in the window): the whole window, above the status bar
/// when `status_bar` is on. Negative / past-320 values reach the window
/// edges outside the 320×200 box.
pub(super) fn automap_window_rect(fb_w: f32, fb_h: f32, status_bar: bool) -> (f32, f32, f32, f32) {
    if fb_w <= 0.0 || fb_h <= 0.0 {
        return (0.0, 0.0, 320.0, 168.0);
    }
    let scale = (fb_w / 320.0).min(fb_h / 200.0);
    let left = -(fb_w - 320.0 * scale) * 0.5 / scale;
    let top = -(fb_h - 200.0 * scale) * 0.5 / scale;
    let bottom = if status_bar {
        168.0
    } else {
        top + fb_h / scale
    };
    (left, top, fb_w / scale, bottom - top)
}

/// The whole window in the HUD's 320×200 units (see
/// [`automap_window_rect`]).
pub(super) fn window_rect(fb_w: f32, fb_h: f32) -> (f32, f32, f32, f32) {
    automap_window_rect(fb_w, fb_h, false)
}

/// Pause-menu action that turns the frame-rate graph (`vid_fps`) on or off.
pub(super) const PAUSE_FPS: &str = "pause_fps";

/// The pause menu. `fps_on`: the frame-rate graph is shown (the item says
/// which way it is). `to_launcher`: the game was started from the
/// launcher, so leaving goes back to the game list rather than closing.
pub(super) fn pause_menu(fps_on: bool, to_launcher: bool) -> ListMenu {
    let mut menu = ListMenu::pause();
    menu.items = vec![
        MenuItem::text("RESUME", "menu_resume"),
        MenuItem::text(
            if fps_on {
                "FPS GRAPH ON"
            } else {
                "FPS GRAPH OFF"
            },
            PAUSE_FPS,
        ),
        MenuItem::text("SAVE GAME", "menu_savegame"),
        MenuItem::text("LOAD GAME", "menu_loadgame"),
        MenuItem::text("OPTIONS", "openmenu OptionsMenu"),
        // End the game for the title menu, where NEW GAME starts another.
        MenuItem::text("MAIN MENU", "menu_endgame"),
        MenuItem::text(if to_launcher { "GAME LIST" } else { "QUIT" }, "menu_quit"),
    ];
    // Seven rows of the big font: closer rows than the stock six, so the
    // last one stays above the status bar.
    menu.origin.y = 56.0;
    menu.line_spacing = 16.0;
    menu
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automap_fills_a_wide_phone() {
        // 2520×1080: the 320×200 box is 1728×1080 in the middle.
        let (x, y, w, h) = automap_window_rect(2520.0, 1080.0, false);
        let scale = 1080.0 / 200.0;
        assert!((x * scale + 396.0).abs() < 0.5, "left window edge: {x}");
        assert!(y.abs() < 1e-4);
        assert!(((x + w) * scale - 2124.0).abs() < 0.5, "right window edge");
        assert!((h - 200.0).abs() < 1e-3, "full height without a status bar");
        let (_, _, _, h) = automap_window_rect(2520.0, 1080.0, true);
        assert!((h - 168.0).abs() < 1e-3, "stops at the status bar");
    }

    #[test]
    fn automap_fills_a_tall_tablet() {
        // 4:3: the box is letterboxed top and bottom.
        let (x, y, w, h) = automap_window_rect(1600.0, 1200.0, false);
        assert!(x.abs() < 1e-4 && (w - 320.0).abs() < 1e-3);
        assert!(y < 0.0 && (y + h - (200.0 - y)).abs() < 1e-3, "{y} {h}");
    }

    #[test]
    fn pause_menu_offers_the_graph_and_the_way_out() {
        let menu = pause_menu(false, true);
        let labels: Vec<&str> = menu.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels[0], "RESUME");
        assert!(labels.contains(&"FPS GRAPH OFF"));
        assert!(labels.contains(&"MAIN MENU"));
        assert_eq!(labels.last(), Some(&"GAME LIST"));
        let on = pause_menu(true, false);
        assert!(on.items.iter().any(|i| i.label == "FPS GRAPH ON"));
        assert_eq!(on.items.last().map(|i| i.label.as_str()), Some("QUIT"));
        let main = menu
            .items
            .iter()
            .find(|i| i.label == "MAIN MENU")
            .expect("item");
        assert_eq!(
            main.action, "menu_endgame",
            "goes through the end-game confirm"
        );
    }
}
