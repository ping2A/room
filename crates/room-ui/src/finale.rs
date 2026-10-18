//! End-of-episode / cast finale text screen (M5).

use crate::draw::{Rect, Rgba, UiDrawList, Vec2};

/// Draw a full-screen finale text board into `list`.
///
/// `map_name` is the completed map lump (used to pick a short stub blurb).
/// `pic` is an optional cinematic still (`E1AN00`…); when set, only that
/// picture is drawn (Duke episode-end ANM).
pub fn draw_finale(
    list: &mut UiDrawList,
    map_name: &str,
    width: f32,
    height: f32,
    pic: Option<&str>,
) {
    list.virtual_w = width;
    list.virtual_h = height;
    list.fill(
        Rect::new(0.0, 0.0, width, height),
        Rgba::new(0.0, 0.0, 0.0, 1.0),
    );
    if let Some(name) = pic {
        list.patch(name, Vec2::new(0.0, 0.0), Vec2::new(width, height));
        return;
    }
    list.patch("CREDIT", Vec2::new(0.0, 0.0), Vec2::new(width, height));
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

/// ANM prefix for a Duke episode-end map (`E1AN` + `E1ANDEF`).
#[must_use]
pub fn finale_anm_prefix(map_name: &str) -> Option<&'static str> {
    match map_name.trim().to_ascii_uppercase().as_str() {
        "E1M6" => Some("E1AN"),
        "E2M11" => Some("E2AN"),
        "E3M11" => Some("E3AN"),
        "E4M11" => Some("E4AN"),
        _ => None,
    }
}

fn finale_blurb(map_name: &str) -> &'static [&'static str] {
    match map_name {
        "MAP30" => doom2_cast_call(),
        "MAP31" | "MAP32" => &["Secret levels cleared.", "Returning to the war..."],
        "E1M8" => &[
            "You have defeated the barons",
            "of Hell and stopped the invasion.",
        ],
        "E1M6" => &["That's the way, baby.", "On to the Lunar Apocalypse."],
        "E2M8" => &["You have defeated Cyberdemon."],
        "E2M11" => &["Shiny. Let's hit the Shrapnel City."],
        "E3M8" => &["You have defeated Spider Mastermind."],
        "E3M11" => &["The alien queen is toast. One more ride."],
        "E4M8" => &["You have defeated Spider Mastermind."],
        "E4M11" => &["It's time to abort her whole ugly race."],
        _ => &["Episode complete.", "Thanks for playing."],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_finale_commands() {
        let mut list = UiDrawList::new();
        draw_finale(&mut list, "MAP30", 320.0, 200.0, None);
        assert!(list.cmds.len() >= 5);
        assert!(list.cmds.iter().any(|c| matches!(
            c,
            crate::draw::DrawCmd::Patch { name, .. } if name == "CREDIT"
        )));
        let mut cine = UiDrawList::new();
        draw_finale(&mut cine, "E1M6", 320.0, 200.0, Some("E1AN03"));
        assert!(cine.cmds.iter().any(|c| matches!(
            c,
            crate::draw::DrawCmd::Patch { name, .. } if name == "E1AN03"
        )));
        assert_eq!(finale_anm_prefix("E1M6"), Some("E1AN"));
        assert_eq!(finale_anm_prefix("MAP30"), None);
    }

    #[test]
    fn map30_lists_cast_call() {
        let cast = doom2_cast_call();
        assert!(cast.contains(&"Zombieman"));
        assert!(cast.contains(&"Our Hero"));
        assert!(cast.len() >= 16);
    }
}
