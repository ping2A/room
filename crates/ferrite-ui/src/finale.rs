//! End-of-episode / cast finale text screen (M5).

use crate::draw::{Rect, Rgba, UiDrawList, Vec2};

/// Draw a full-screen finale text board into `list`.
///
/// `map_name` is the completed map lump (used to pick a short stub blurb).
pub fn draw_finale(list: &mut UiDrawList, map_name: &str, width: f32, height: f32) {
    list.virtual_w = width;
    list.virtual_h = height;
    list.fill(
        Rect::new(0.0, 0.0, width, height),
        Rgba::new(0.0, 0.0, 0.0, 1.0),
    );
    let name = map_name.trim().to_ascii_uppercase();
    let cast = name == "MAP30";
    let title = if cast {
        "CAST OF CHARACTERS"
    } else {
        "YOU HAVE WON!"
    };
    let body = finale_blurb(&name);
    let x = width * 0.12;
    let mut y = height * 0.18;
    list.text(
        "BigFont",
        Vec2::new(x, y),
        title,
        Rgba::new(1.0, 0.85, 0.2, 1.0),
        1.0,
    );
    y += 24.0;
    for line in body {
        list.text(
            "SmallFont",
            Vec2::new(x, y),
            *line,
            Rgba::new(0.85, 0.85, 0.85, 1.0),
            1.0,
        );
        y += if cast { 11.0 } else { 14.0 };
    }
    list.text(
        "SmallFont",
        Vec2::new(x, height * 0.92),
        "Press Enter",
        Rgba::new(0.65, 0.65, 0.7, 1.0),
        1.0,
    );
}

/// Doom II cast-order names (M5 text board; animated cast comes later).
fn doom2_cast_call() -> &'static [&'static str] {
    &[
        "Zombieman",
        "Shotgun Guy",
        "Heavy Weapon Dude",
        "Imp",
        "Demon",
        "Spectre",
        "Lost Soul",
        "Cacodemon",
        "Hell Knight",
        "Baron of Hell",
        "Arachnotron",
        "Pain Elemental",
        "Revenant",
        "Mancubus",
        "Arch-Vile",
        "The Spider Mastermind",
        "The Cyberdemon",
        "Our Hero",
    ]
}

fn finale_blurb(map_name: &str) -> &'static [&'static str] {
    match map_name {
        "MAP30" => doom2_cast_call(),
        "MAP31" | "MAP32" => &["Secret levels cleared.", "Returning to the war..."],
        "E1M8" => &[
            "You have defeated the barons",
            "of Hell and stopped the invasion.",
        ],
        "E2M8" => &["You have defeated Cyberdemon."],
        "E3M8" => &["You have defeated Spider Mastermind."],
        "E4M8" => &["You have defeated Spider Mastermind."],
        _ => &["Episode complete.", "Thanks for playing."],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_finale_commands() {
        let mut list = UiDrawList::new();
        draw_finale(&mut list, "MAP30", 320.0, 200.0);
        assert!(list.cmds.len() >= 4);
    }

    #[test]
    fn map30_lists_cast_call() {
        let cast = doom2_cast_call();
        assert!(cast.contains(&"Zombieman"));
        assert!(cast.contains(&"Our Hero"));
        assert!(cast.len() >= 16);
    }
}
