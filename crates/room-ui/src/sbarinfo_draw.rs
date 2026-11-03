//! SBARINFO status-bar interpreter (M7 bootstrap).

use crate::draw::{Rgba, UiDrawList, Vec2};
use crate::hud::HudState;
use crate::statusbar::StatusBar;
use room_defs::{SbarCmd, SbarInfo};

/// Draws a compiled [`SbarInfo`] against [`HudState`].
#[derive(Clone, Debug)]
pub struct SbarInfoBar {
    /// Parsed SBARINFO.
    pub info: SbarInfo,
    /// Treat current game as deathmatch (`gamemode deathmatch`).
    pub deathmatch: bool,
}

impl SbarInfoBar {
    /// Wrap a parsed lump.
    #[must_use]
    pub fn new(info: SbarInfo) -> Self {
        Self {
            info,
            deathmatch: false,
        }
    }
}

impl StatusBar for SbarInfoBar {
    fn draw(&self, hud: &HudState, list: &mut UiDrawList) {
        let kind = if hud.fullscreen {
            "fullscreen"
        } else {
            "normal"
        };
        let Some(bar) = self.info.statusbars.get(kind) else {
            // Fall back to the other bar if the preferred one is missing.
            let alt = if hud.fullscreen {
                "normal"
            } else {
                "fullscreen"
            };
            if let Some(bar) = self.info.statusbars.get(alt) {
                draw_cmds(&bar.commands, hud, list, self.deathmatch, true);
            }
            return;
        };
        draw_cmds(&bar.commands, hud, list, self.deathmatch, true);
    }
}

fn draw_cmds(
    cmds: &[SbarCmd],
    hud: &HudState,
    list: &mut UiDrawList,
    deathmatch: bool,
    parent_active: bool,
) {
    if !parent_active {
        return;
    }
    let mut i = 0;
    while i < cmds.len() {
        match &cmds[i] {
            SbarCmd::DrawImage { image, x, y, flags } => {
                draw_image(list, hud, image, *x, *y, flags);
                i += 1;
            }
            SbarCmd::DrawNumber {
                digits,
                value,
                rest,
                ..
            } => {
                draw_number(list, hud, digits, value, rest);
                i += 1;
            }
            SbarCmd::DrawMugShot { args } => {
                draw_mugshot(list, hud, args);
                i += 1;
            }
            SbarCmd::DrawString { .. } | SbarCmd::DrawBar { .. } => {
                i += 1;
            }
            SbarCmd::Block { kind, args, body } => {
                let k = kind.to_ascii_lowercase();
                let active = eval_block(&k, args, hud, deathmatch);
                draw_cmds(body, hud, list, deathmatch, active);
                // Optional `else` follows immediately.
                if i + 1 < cmds.len() {
                    if let SbarCmd::Block {
                        kind: else_k,
                        body: else_body,
                        ..
                    } = &cmds[i + 1]
                    {
                        if else_k.eq_ignore_ascii_case("else") {
                            draw_cmds(else_body, hud, list, deathmatch, !active);
                            i += 2;
                            continue;
                        }
                    }
                }
                i += 1;
            }
            SbarCmd::Other { name, args } => {
                let n = name.to_ascii_lowercase();
                if n == "drawswitchableimage" {
                    draw_switchable_key(list, hud, args);
                }
                i += 1;
            }
        }
    }
}

fn eval_block(kind: &str, args: &[String], hud: &HudState, deathmatch: bool) -> bool {
    match kind {
        "else" => false, // handled with previous block
        "gamemode" => {
            args.iter()
                .any(|a| a.eq_ignore_ascii_case("deathmatch") && deathmatch)
                || args
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case("singleplayer") && !deathmatch)
                || args
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case("cooperative") && !deathmatch)
        }
        "usesammo" => hud.ammo_max > 0 || hud.ammo > 0,
        "usessecondaryammo" => hud.ammo2_max > 0 || hud.ammo2 > 0,
        "inventorybarnotvisible" => true,
        "weaponammo" | "ininventory" | "hasweaponpiece" | "isselected" | "playerclass"
        | "aspectratio" | "armortype" => true,
        _ => true,
    }
}

fn draw_image(
    list: &mut UiDrawList,
    hud: &HudState,
    image: &str,
    x: i32,
    y: i32,
    flags: &[String],
) {
    let lump = resolve_image(image, hud);
    if lump.is_empty() || lump.eq_ignore_ascii_case("nullimage") {
        return;
    }
    let (px, py) = adjust_xy(x, y, flags, hud.fullscreen);
    // Default patch size heuristic for status-bar graphics.
    let (w, h) = if lump.eq_ignore_ascii_case("STBAR") {
        (320.0, 32.0)
    } else if lump.starts_with("STKEYS") || lump.starts_with("STT") {
        (8.0, 16.0)
    } else {
        (16.0, 16.0)
    };
    list.patch(&lump, Vec2::new(px, py), Vec2::new(w, h));
}

fn draw_number(list: &mut UiDrawList, hud: &HudState, digits: &str, value: &str, rest: &[String]) {
    let when_not_zero = rest.iter().any(|t| t.eq_ignore_ascii_case("whennotzero"));
    let n = resolve_value(value, hud);
    if when_not_zero && n == 0 {
        return;
    }
    let max_digits: usize = digits.parse().unwrap_or(3).clamp(1, 12) as usize;
    let (x, y) = trailing_xy(rest).unwrap_or((0, 0));
    let (px, py) = adjust_xy(x, y, rest, hud.fullscreen);
    let text = if max_digits >= 7 {
        format!("{n}")
    } else {
        format!("{n:width$}", width = max_digits.min(6))
    };
    list.text("HudBig", Vec2::new(px, py), text, Rgba::WHITE, 1.0);
}

fn draw_mugshot(list: &mut UiDrawList, hud: &HudState, args: &[String]) {
    let (x, y) = trailing_xy(args).unwrap_or((143, 168));
    let (px, py) = adjust_xy(x, y, args, hud.fullscreen);
    if !hud.face.is_empty() {
        list.picture(&hud.face, Vec2::new(px, py));
    }
}

fn draw_switchable_key(list: &mut UiDrawList, hud: &HudState, args: &[String]) {
    // `keyslot A && B, null, card, skull, both, x, y`
    let mut key_bits = Vec::new();
    let mut images = Vec::new();
    let mut coords = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a.eq_ignore_ascii_case("keyslot") {
            i += 1;
            while i < args.len() {
                let t = &args[i];
                if t == "&&" {
                    i += 1;
                    continue;
                }
                if let Ok(slot) = t.parse::<u8>() {
                    key_bits.push(slot);
                    i += 1;
                    continue;
                }
                break;
            }
            continue;
        }
        if let Ok(n) = a.parse::<i32>() {
            coords.push(n);
            i += 1;
            continue;
        }
        if !a.eq_ignore_ascii_case("nullimage") && !a.is_empty() {
            images.push(a.clone());
        }
        i += 1;
    }
    if coords.len() < 2 || images.is_empty() {
        return;
    }
    let x = *coords.iter().rev().nth(1).unwrap_or(&0);
    let y = *coords.last().unwrap_or(&0);
    // Doom keyslot 1=red,2=blue,3=yellow cards; 4/5/6 skulls — map to hud.keys bits.
    let card = key_bits.first().copied().unwrap_or(0);
    let skull = key_bits.get(1).copied().unwrap_or(0);
    let card_bit = slot_to_bit(card);
    let skull_bit = slot_to_bit(skull);
    let has_card = card_bit != 0 && hud.keys & card_bit != 0;
    let has_skull = skull_bit != 0 && hud.keys & skull_bit != 0;
    let img = if has_card && has_skull {
        images.get(2).or(images.get(1)).or(images.first())
    } else if has_skull {
        images.get(1).or(images.first())
    } else if has_card {
        images.first()
    } else {
        None
    };
    if let Some(lump) = img {
        list.patch(lump, Vec2::new(x as f32, y as f32), Vec2::new(8.0, 16.0));
    }
}

fn slot_to_bit(slot: u8) -> u8 {
    // HudState: bit0 blue, bit1 yellow, bit2 red; skulls +3.
    match slot {
        1 | 4 => 0b100, // red
        2 | 5 => 0b001, // blue
        3 | 6 => 0b010, // yellow
        _ => 0,
    }
}

fn resolve_image(image: &str, hud: &HudState) -> String {
    let lower = image.to_ascii_lowercase();
    match lower.as_str() {
        "armoricon" | "ammoicon1" | "ammoicon2" => String::new(),
        "playericon" => hud.face.clone(),
        _ => image.to_owned(),
    }
}

fn resolve_value(value: &str, hud: &HudState) -> i32 {
    match value.to_ascii_lowercase().as_str() {
        "health" => hud.health,
        "armor" => hud.armor,
        "ammo1" | "ammo" => hud.ammo,
        "ammo2" => hud.ammo2,
        "frags" => hud.frags,
        "ammo1capacity" | "ammocapacity" => hud.ammo_max,
        "ammo2capacity" => hud.ammo2_max,
        other => other.parse().unwrap_or(0),
    }
}

fn trailing_xy(tokens: &[String]) -> Option<(i32, i32)> {
    let nums: Vec<i32> = tokens.iter().filter_map(|t| t.parse().ok()).collect();
    if nums.len() >= 2 {
        Some((nums[nums.len() - 2], nums[nums.len() - 1]))
    } else {
        None
    }
}

fn adjust_xy(x: i32, y: i32, flags: &[String], fullscreen: bool) -> (f32, f32) {
    let mut px = x as f32;
    let mut py = y as f32;
    let fullscreen_offsets = flags
        .iter()
        .any(|f| f.eq_ignore_ascii_case("fullscreenoffsets"))
        || fullscreen;
    if fullscreen_offsets {
        // Negative coords count from the right / bottom of 320×200.
        if x < 0 {
            px = 320.0 + x as f32;
        }
        if y < 0 {
            py = 200.0 + y as f32;
        }
    }
    let centerbottom = flags.iter().any(|f| f.eq_ignore_ascii_case("centerbottom"));
    if centerbottom {
        px -= 8.0;
        py -= 16.0;
    }
    (px, py)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::draw::DrawCmd;
    use room_defs::parse_sbarinfo;

    #[test]
    fn draws_normal_stbar_from_fixture() {
        let text = include_str!("../../../testdata/sbarinfo/doom.txt");
        let info = parse_sbarinfo(text).unwrap();
        let bar = SbarInfoBar::new(info);
        let mut hud = HudState::doom_default();
        hud.fullscreen = false;
        let mut list = UiDrawList::default();
        bar.draw(&hud, &mut list);
        assert!(
            list.cmds
                .iter()
                .any(|c| matches!(c, DrawCmd::Patch { name, .. } if name == "STBAR")),
            "expected STBAR patch, got {:?}",
            list.cmds
        );
        assert!(
            list.cmds
                .iter()
                .any(|c| matches!(c, DrawCmd::Text { text, .. } if text.contains("100"))),
            "expected health number"
        );
    }

    #[test]
    fn resolve_ammo1_ammo2_and_usessecondary() {
        let mut hud = HudState::doom_default();
        hud.ammo = 12;
        hud.ammo_max = 20;
        hud.ammo2 = 50;
        hud.ammo2_max = 200;
        assert_eq!(resolve_value("ammo1", &hud), 12);
        assert_eq!(resolve_value("ammo2", &hud), 50);
        assert_eq!(resolve_value("ammo2capacity", &hud), 200);
        assert!(eval_block("usessecondaryammo", &[], &hud, false));
        hud.ammo2 = 0;
        hud.ammo2_max = 0;
        assert!(!eval_block("usessecondaryammo", &[], &hud, false));
    }
}
