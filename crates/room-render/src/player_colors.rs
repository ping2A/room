//! Seat colours for player sprites (Doom's green ramp, palette 112–127,
//! remapped per seat like vanilla's translation tables).
//!
//! A sprite drawn for a seat other than the first uses the texture name
//! `"<LUMP>@P<n>"` ([`player_color_texture`]); the material loader decodes the
//! lump and remaps its green ramp to colour `n` ([`recolor_player`]).

use room_formats::{Image, ImagePixels};

/// Doom's player colour ramp in PLAYPAL (the green the pawn sprites use).
pub const GREEN_RAMP: std::ops::RangeInclusive<u8> = 112..=127;

/// First palette index of each seat colour's 16-entry ramp. 0 is the
/// unchanged green; 1–3 are vanilla's indigo (gray), brown and red; 4–7 are
/// blue, orange, tan and light gray, so eight players all differ. Seats past
/// 8 reuse the list.
pub const COLOR_RAMPS: [u8; 8] = [112, 96, 64, 32, 192, 208, 128, 80];

/// Names for the colours above (scoreboard, chat).
pub const COLOR_NAMES: [&str; 8] = [
    "Green", "Indigo", "Brown", "Red", "Blue", "Orange", "Tan", "Gray",
];

/// Colour of `seat` (0-based).
#[must_use]
pub fn seat_color(seat: usize) -> u8 {
    u8::try_from(seat % COLOR_RAMPS.len()).unwrap_or(0)
}

/// Texture name of sprite `lump` in colour `color` (unchanged for 0).
#[must_use]
pub fn player_color_texture(lump: &str, color: u8) -> String {
    if color == 0 || lump.is_empty() {
        lump.to_owned()
    } else {
        format!("{lump}@P{color}")
    }
}

/// Split `"<LUMP>@P<n>"` into the lump and colour `n` (1–7).
#[must_use]
pub fn parse_player_color_name(name: &str) -> Option<(&str, u8)> {
    let (base, tail) = name.rsplit_once("@P")?;
    let color: u8 = tail.parse().ok()?;
    (!base.is_empty() && (1..8).contains(&color)).then_some((base, color))
}

/// Remap the green ramp of a paletted image to colour `color`.
pub fn recolor_player(image: &mut Image, color: u8) {
    let Some(&target) = COLOR_RAMPS.get(usize::from(color)) else {
        return;
    };
    if let ImagePixels::Paletted(p) = &mut image.pixels {
        for idx in &mut p.indices {
            if GREEN_RAMP.contains(idx) {
                *idx = target + (*idx - GREEN_RAMP.start());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_formats::PalettedImage;

    #[test]
    fn names_round_trip_and_green_becomes_the_seat_colour() {
        assert_eq!(player_color_texture("PLAYA1", 0), "PLAYA1");
        let red = player_color_texture("PLAYA1", 3);
        assert_eq!(parse_player_color_name(&red), Some(("PLAYA1", 3)));
        assert_eq!(parse_player_color_name("PLAYA1"), None);
        assert_eq!(parse_player_color_name("X@P9"), None);
        assert_eq!(seat_color(9), 1);
        let mut image = Image {
            left_offset: 0,
            top_offset: 0,
            has_translucency: false,
            pixels: ImagePixels::Paletted(PalettedImage {
                width: 3,
                height: 1,
                indices: vec![112, 120, 5],
                alpha: Vec::new(),
            }),
        };
        recolor_player(&mut image, 3);
        let ImagePixels::Paletted(p) = &image.pixels else {
            panic!()
        };
        assert_eq!(p.indices, vec![32, 40, 5], "green ramp to red, others kept");
    }
}
