//! No-allocation Mod HUD builder (`docs/27` §9.2 / E13).
//!
//! ```ignore
//! use room_script_sdk::hud::{Hud, anchor, font};
//! Hud::text(1)
//!     .anchor(anchor::TOP_CENTER)
//!     .at(0, 24)
//!     .font(font::BIG)
//!     .color("Gold")
//!     .text("ROUND 5")
//!     .seat(0)
//!     .show();
//! ```
//!
//! [`HudBuilder::seat`](crate::hud::HudBuilder::seat) takes a 0-based player slot; `-1` is every seat
//! (import `seat1` = 0). [`show`](crate::hud::HudBuilder::show) writes one stack buffer
//! and calls `hud_widget_set`.

use crate::game;
use room_script_abi::hud::HudWidgetV1;

pub use room_script_abi::hud::{align, anchor, flags, font, kind, layer, map_layers, patch};

const TEXT_CAP: usize = 160;
const NAME_CAP: usize = 64;
const PARAMS: usize = 320;

/// `0`-based slot → import `seat1` (`-1` = every seat).
#[must_use]
pub fn seat1_of(slot: i32) -> i32 {
    if slot < 0 { 0 } else { slot.saturating_add(1) }
}

/// TEXTCOLO range index. Unknown names and `"untranslated"` are `255`.
#[must_use]
pub fn text_color_index(name: &str) -> u8 {
    const NAMES: &[&str] = &[
        "brick",
        "tan",
        "gray",
        "green",
        "brown",
        "gold",
        "red",
        "blue",
        "orange",
        "white",
        "yellow",
        "untranslated",
        "black",
        "lightblue",
        "cream",
        "olive",
        "darkgreen",
        "darkred",
        "darkbrown",
        "purple",
        "darkgray",
        "cyan",
        "ice",
        "fire",
        "sapphire",
        "teal",
    ];
    let mut i = 0u8;
    for n in NAMES {
        if norm_eq(name, n) {
            return if *n == "untranslated" { 255 } else { i };
        }
        i = i.saturating_add(1);
    }
    if norm_eq(name, "grey") {
        return 2;
    }
    255
}

fn norm_eq(a: &str, b: &str) -> bool {
    let mut ia = a.bytes().filter(|c| *c != b' ' && *c != b'_');
    let mut ib = b.bytes().filter(|c| *c != b' ' && *c != b'_');
    loop {
        match (ia.next(), ib.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if x.eq_ignore_ascii_case(&y) => {}
            _ => return false,
        }
    }
}

fn copy_cap(dst: &mut [u8], s: &str) -> u16 {
    let mut n = s.len().min(dst.len());
    while n > 0 && !s.is_char_boundary(n) {
        n -= 1;
    }
    dst[..n].copy_from_slice(&s.as_bytes()[..n]);
    u16::try_from(n).unwrap_or(0)
}

/// Entry points for retained widgets.
pub struct Hud;

impl Hud {
    /// Text widget.
    pub fn text(id: u16) -> HudBuilder {
        HudBuilder::new(kind::TEXT, id)
    }

    /// Solid rectangle.
    pub fn fill(id: u16) -> HudBuilder {
        HudBuilder::new(kind::FILL, id)
    }

    /// Image / patch.
    pub fn image(id: u16) -> HudBuilder {
        HudBuilder::new(kind::IMAGE, id)
    }

    /// Progress bar.
    pub fn bar(id: u16) -> HudBuilder {
        HudBuilder::new(kind::BAR, id)
    }

    /// Outline box.
    pub fn frame(id: u16) -> HudBuilder {
        HudBuilder::new(kind::BOX, id)
    }

    /// Panel (fill + border).
    pub fn panel(id: u16) -> HudBuilder {
        HudBuilder::new(kind::PANEL, id)
    }

    /// World marker (drawn in MH4).
    pub fn marker(id: u16) -> HudBuilder {
        HudBuilder::new(kind::MARKER, id)
    }

    /// Minimap / full map (`docs/28` BE2).
    pub fn map(id: u16) -> HudBuilder {
        HudBuilder::new(kind::MAP, id)
    }

    /// `hud_widget_patch`. `slot` is 0-based; `-1` = every seat.
    pub fn patch(slot: i32, id: u16, field: i32, value: i32) -> i32 {
        game::hud_widget_patch(seat1_of(slot), id, field, value)
    }

    /// `hud_widget_remove`.
    pub fn remove(slot: i32, id: u16) -> i32 {
        game::hud_widget_remove(seat1_of(slot), id)
    }

    /// `hud_clear` for one seat (`-1` = shared / all-seats widgets only).
    pub fn clear(slot: i32) -> i32 {
        game::hud_clear(seat1_of(slot))
    }

    /// Tween a numeric field toward `target` over `tics` (linear; `easing` reserved).
    pub fn animate(slot: i32, id: u16, field: i32, target: i32, tics: u16) -> i32 {
        game::hud_widget_animate(seat1_of(slot), id, field, target, i32::from(tics), 0)
    }
}

/// Stack builder. Call [`show`](Self::show) to commit.
#[must_use]
pub struct HudBuilder {
    id: u16,
    seat1: i32,
    widget: HudWidgetV1,
    text: [u8; TEXT_CAP],
    text_len: u16,
    name: [u8; NAME_CAP],
    name_len: u16,
}

impl HudBuilder {
    fn new(kind_id: u8, id: u16) -> Self {
        let mut widget = HudWidgetV1::guest_default();
        widget.kind = kind_id;
        Self {
            id,
            seat1: 0,
            widget,
            text: [0; TEXT_CAP],
            text_len: 0,
            name: [0; NAME_CAP],
            name_len: 0,
        }
    }

    /// [`anchor`] id.
    pub fn anchor(&mut self, anchor_id: u8) -> &mut Self {
        self.widget.anchor = anchor_id;
        self
    }

    /// [`layer`] id.
    pub fn layer(&mut self, layer_id: u8) -> &mut Self {
        self.widget.layer = layer_id;
        self
    }

    /// Offset from the anchor, virtual 320×200 units.
    pub fn at(&mut self, x: i16, y: i16) -> &mut Self {
        self.widget.x = x;
        self.widget.y = y;
        self
    }

    /// Width and height (`0` = natural).
    pub fn size(&mut self, w: i16, h: i16) -> &mut Self {
        self.widget.w = w;
        self.widget.h = h;
        self
    }

    /// Z-order inside the layer.
    pub fn z(&mut self, z: i16) -> &mut Self {
        self.widget.z = z;
        self
    }

    /// [`font`] id.
    pub fn font(&mut self, font_id: u8) -> &mut Self {
        self.widget.font = font_id;
        self
    }

    /// [`align`] bits.
    pub fn align(&mut self, align_id: u8) -> &mut Self {
        self.widget.align = align_id;
        self
    }

    /// [`flags`] bits (replaces the set).
    pub fn flags(&mut self, flags: u8) -> &mut Self {
        self.widget.flags = flags;
        self
    }

    /// RGBA primary (`0xRRGGBBAA`).
    pub fn rgba(&mut self, color: u32) -> &mut Self {
        self.widget.color = color;
        self
    }

    /// RGBA secondary (bar background / panel border).
    pub fn rgba2(&mut self, color: u32) -> &mut Self {
        self.widget.color2 = color;
        self
    }

    /// TEXTCOLO name (`"Gold"`, `"Red"`, …). Unknown → untranslated (`255`).
    pub fn color(&mut self, name: &str) -> &mut Self {
        self.widget.text_color = text_color_index(name);
        self
    }

    /// UTF-8 label (truncated to 160 bytes).
    pub fn text(&mut self, text: &str) -> &mut Self {
        self.text_len = copy_cap(&mut self.text, text);
        self
    }

    /// Graphic or sprite name (`"M_SKULL1"`, `"PINVA0"`).
    pub fn graphic(&mut self, name: &str) -> &mut Self {
        self.name_len = copy_cap(&mut self.name, name);
        self
    }

    /// Bar value.
    pub fn value(&mut self, value: i32) -> &mut Self {
        self.widget.value = value;
        self
    }

    /// Bar max.
    pub fn max(&mut self, max: i32) -> &mut Self {
        self.widget.max = max;
        self
    }

    /// Alpha 0..=1000.
    pub fn alpha(&mut self, alpha: u16) -> &mut Self {
        self.widget.alpha = alpha;
        self
    }

    /// 0-based player slot. `-1` shows on every seat.
    pub fn seat(&mut self, slot: i32) -> &mut Self {
        self.seat1 = seat1_of(slot);
        self
    }

    /// Marker world position (map units). Ignored when [`follow`](Self::follow) is set.
    pub fn world_at(&mut self, x: i32, y: i32, z: i32) -> &mut Self {
        self.widget.world_xyz = [x, y, z];
        self
    }

    /// Marker follows this actor (sim copies position into `world_xyz` each tic).
    pub fn follow(&mut self, actor: game::Actor) -> &mut Self {
        self.widget.actor = actor.0.0;
        self
    }

    /// Encode the params blob (header + name + text) into a stack buffer.
    #[must_use]
    pub fn encode(&self) -> ([u8; PARAMS], usize) {
        let mut buf = [0u8; PARAMS];
        let mut w = self.widget;
        w.size = HudWidgetV1::SIZE;
        let mut cursor = usize::from(HudWidgetV1::SIZE);
        if self.name_len > 0 {
            let n = usize::from(self.name_len);
            w.name_off = u16::try_from(cursor).unwrap_or(0);
            w.name_len = self.name_len;
            buf[cursor..cursor + n].copy_from_slice(&self.name[..n]);
            cursor += n;
        }
        if self.text_len > 0 {
            let n = usize::from(self.text_len);
            w.text_off = u16::try_from(cursor).unwrap_or(0);
            w.text_len = self.text_len;
            buf[cursor..cursor + n].copy_from_slice(&self.text[..n]);
            cursor += n;
        }
        let _ = w.write_le(&mut buf);
        (buf, cursor)
    }

    /// Create or replace the widget. `0` ok, `<0` error.
    pub fn show(&mut self) -> i32 {
        let (buf, n) = self.encode();
        game::hud_widget_set(self.seat1, self.id, &buf[..n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_script_abi::hud::HudWidgetV1;

    #[test]
    fn builder_matches_the_sketch_and_roundtrips() {
        let (buf, n) = Hud::text(1)
            .anchor(anchor::TOP_CENTER)
            .at(0, 24)
            .font(font::BIG)
            .color("Gold")
            .text("ROUND 5")
            .seat(0)
            .encode();
        assert!(n > usize::from(HudWidgetV1::SIZE));
        let w = HudWidgetV1::decode_prefix(&buf[..n]).unwrap_or_else(HudWidgetV1::guest_default);
        let (kind_id, anchor_id, font_id, y, text_color, text_off, text_len) = (
            w.kind,
            w.anchor,
            w.font,
            w.y,
            w.text_color,
            w.text_off,
            w.text_len,
        );
        assert_eq!(kind_id, kind::TEXT);
        assert_eq!(anchor_id, anchor::TOP_CENTER);
        assert_eq!(font_id, font::BIG);
        assert_eq!(y, 24);
        assert_eq!(text_color, 5);
        assert_eq!(text_off, HudWidgetV1::SIZE);
        assert_eq!(text_len, 7);
        let start = usize::from(text_off);
        let end = start + usize::from(text_len);
        assert_eq!(&buf[start..end], b"ROUND 5");
        assert_eq!(text_color_index("light blue"), 13);
        assert_eq!(text_color_index("nope"), 255);
        assert_eq!(seat1_of(-1), 0);
        assert_eq!(seat1_of(0), 1);
        let _ = align::LEFT;
        let _ = layer::OVER_HUD;
    }
}
