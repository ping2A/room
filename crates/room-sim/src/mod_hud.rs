//! Retained Mod HUD widget store (`docs/27` §9 / MH0).
//!
//! Widgets are sim state owned by play modules; shells only draw them.
//! They are **not** part of [`crate::World::checksum`] (output-only).

use std::collections::BTreeMap;
use xxhash_rust::xxh3::xxh3_64;

/// Max widgets attributed to one seat (seat `0` = shared / all-seats).
pub const MAX_WIDGETS_PER_SEAT: usize = 96;
/// Max UTF-8 text bytes stored on one widget.
pub const MAX_TEXT_BYTES: usize = 160;
/// Max total text bytes across widgets for one seat.
pub const MAX_TEXT_PER_SEAT: usize = 8 * 1024;

/// Widget kind ids (`docs/27` §9.2).
pub mod kind {
    /// Text label.
    pub const TEXT: u8 = 0;
    /// Solid rectangle fill.
    pub const FILL: u8 = 1;
    /// Image / patch.
    pub const IMAGE: u8 = 2;
    /// Progress bar.
    pub const BAR: u8 = 3;
    /// Outline box.
    pub const BOX: u8 = 4;
    /// Panel (fill + border).
    pub const PANEL: u8 = 5;
    /// World-space marker.
    pub const MARKER: u8 = 6;
    /// Minimap / full map (`docs/28` BE2).
    pub const MAP: u8 = 7;
}

/// MAP widget packing (`HudWidgetV1` reuses bar/marker fields).
pub mod map {
    /// Layer: linedefs / automap geometry.
    pub const GEOMETRY: i32 = 1 << 0;
    /// Layer: BE1 zone rings.
    pub const ZONES: i32 = 1 << 1;
    /// Layer: MARKER widgets for this seat.
    pub const MARKERS: i32 = 1 << 2;
    /// Layer: squad teammates (no-op until BE5 teams).
    pub const TEAMMATES: i32 = 1 << 3;
    /// Layer: items-of-interest markers.
    pub const ITEMS: i32 = 1 << 4;
    /// Show the whole level, not only the lines the player has seen.
    pub const REVEAL: i32 = 1 << 5;
    /// Do not draw the viewer's arrow (pick screens).
    pub const HIDE_PLAYER: i32 = 1 << 6;
    /// Centre the map on the widget's `world_xyz` instead of the viewer.
    pub const FIXED_CENTER: i32 = 1 << 7;
    /// Default layer set for a minimap.
    pub const DEFAULT_LAYERS: i32 = GEOMETRY | ZONES | MARKERS;

    /// `value` → pixels per map unit (`1500` = `0.15`).
    #[must_use]
    pub fn scale_from_value(value: i32) -> f32 {
        (value as f32 / 10_000.0).clamp(0.01, 4.0)
    }

    /// Pixels per map unit → `value`.
    #[must_use]
    pub fn value_from_scale(scale: f32) -> i32 {
        (scale.clamp(0.01, 4.0) * 10_000.0).round() as i32
    }

    /// `font` stores follow seat: `0` = drawing seat, `1..=8` = follow that player slot + 1 encoding → slot = font - 1 when font > 0.
    #[must_use]
    pub fn follow_seat(font: u8, draw_seat: u8) -> u8 {
        if font == 0 {
            draw_seat
        } else {
            font.saturating_sub(1)
        }
    }
}

/// Draw layer (`docs/27` §9.2).
pub mod layer {
    /// Under the status bar / messages.
    pub const UNDER_HUD: u8 = 0;
    /// Over the status bar.
    pub const OVER_HUD: u8 = 1;
    /// Modal (dims the scene).
    pub const MODAL: u8 = 2;
}

/// Widget flags.
pub mod flags {
    /// Not drawn.
    pub const HIDDEN: u8 = 1 << 0;
    /// Survive map change (`WorldUnloaded`).
    pub const PERSIST_MAP: u8 = 1 << 1;
    /// Hide when occluded (MH4).
    pub const OCCLUDED_HIDE: u8 = 1 << 2;
    /// Show distance on markers (MH4).
    pub const SHOW_DISTANCE: u8 = 1 << 3;
    /// Pulse alpha (draw-side).
    pub const PULSE: u8 = 1 << 4;
    /// Keep image aspect.
    pub const KEEP_ASPECT: u8 = 1 << 5;
    /// MAP widget rotates with viewer yaw (`docs/28` BE2).
    pub const MAP_ROTATE: u8 = 1 << 6;
    /// MARKER shown only on MAP widgets, not over the 3D view.
    pub const MAP_ONLY: u8 = 1 << 7;
}

/// Error codes returned to guests (`0` = ok).
pub mod err {
    /// Parameters invalid.
    pub const BAD_PARAMS: i32 = -1;
    /// Seat / module widget or text cap hit.
    pub const CAP: i32 = -2;
    /// Unknown widget id.
    pub const MISSING: i32 = -3;
}

/// Key: owning module index, seat (`0` = all seats, `1..` = seat n−1), widget id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetKey {
    /// Module index in [`crate::script_mods::ModRuntime`] (`u16::MAX` = host/test).
    pub module: u16,
    /// `0` = every seat; else `seat + 1`.
    pub seat1: u8,
    /// Guest-chosen id (unique per module+seat).
    pub id: u16,
}

/// One retained widget.
#[derive(Clone, Debug, PartialEq)]
pub struct HudWidget {
    /// [`kind`] id.
    pub kind: u8,
    /// [`layer`] id.
    pub layer: u8,
    /// Anchor 0..=8.
    pub anchor: u8,
    /// Text align / wrap bits.
    pub align: u8,
    /// Font id.
    pub font: u8,
    /// [`flags`] bits.
    pub flags: u8,
    /// Offset from anchor (virtual 320×200).
    pub x: i16,
    /// Offset from anchor.
    pub y: i16,
    /// Width (`0` = natural).
    pub w: i16,
    /// Height.
    pub h: i16,
    /// Z-order inside a layer.
    pub z: i16,
    /// RGBA fill / tint.
    pub color: u32,
    /// RGBA secondary.
    pub color2: u32,
    /// TEXTCOLO index (`255` = untranslated).
    pub text_color: u8,
    /// Bar current value.
    pub value: i32,
    /// Bar max.
    pub max: i32,
    /// Bar segments (`0` = solid).
    pub segments: u16,
    /// Alpha 0..=1000.
    pub alpha: u16,
    /// Fade-in tics.
    pub fade_in: u16,
    /// Hold tics (`0` = until removed).
    pub hold: u16,
    /// Fade-out tics.
    pub fade_out: u16,
    /// Marker actor bits (`0` = use [`Self::world_xyz`]).
    pub actor: u64,
    /// Marker world position (map units).
    pub world_xyz: [i32; 3],
    /// Graphic / sprite name (IMAGE / MARKER).
    pub name: String,
    /// UTF-8 label.
    pub text: String,
    /// Tic the widget was created.
    pub created_tic: u32,
    /// Tic of last update.
    pub updated_tic: u32,
    /// Tween: field id being animated (`255` = none).
    pub tween_field: u8,
    /// Tween target value.
    pub tween_target: i32,
    /// Tics remaining in the tween.
    pub tween_tics_left: u16,
    /// Tween start value.
    pub tween_start: i32,
    /// Total tween duration.
    pub tween_duration: u16,
}

impl Default for HudWidget {
    fn default() -> Self {
        Self {
            kind: kind::TEXT,
            layer: layer::OVER_HUD,
            anchor: 1, // top-center
            align: 0,
            font: 0,
            flags: 0,
            x: 0,
            y: 24,
            w: 0,
            h: 0,
            z: 0,
            color: 0xffff_ffff,
            color2: 0,
            text_color: 255,
            value: 0,
            max: 0,
            segments: 0,
            alpha: 1000,
            fade_in: 0,
            hold: 0,
            fade_out: 0,
            actor: 0,
            world_xyz: [0; 3],
            name: String::new(),
            text: String::new(),
            created_tic: 0,
            updated_tic: 0,
            tween_field: 255,
            tween_target: 0,
            tween_tics_left: 0,
            tween_start: 0,
            tween_duration: 0,
        }
    }
}

/// Ordered widget store on [`crate::World`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModHud {
    widgets: BTreeMap<WidgetKey, HudWidget>,
}

impl ModHud {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of widgets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty()
    }

    /// Iterate in key order (determinism).
    pub fn iter(&self) -> impl Iterator<Item = (&WidgetKey, &HudWidget)> {
        self.widgets.iter()
    }

    /// Widgets visible to `seat` (0-based): shared (`seat1 == 0`) + that seat.
    pub fn for_seat(&self, seat: u8) -> impl Iterator<Item = (&WidgetKey, &HudWidget)> {
        let seat1 = seat.saturating_add(1);
        self.widgets
            .iter()
            .filter(move |(k, _)| k.seat1 == 0 || k.seat1 == seat1)
    }

    fn seat_widget_count(&self, module: u16, seat1: u8) -> usize {
        self.widgets
            .keys()
            .filter(|k| k.module == module && k.seat1 == seat1)
            .count()
    }

    fn seat_text_bytes(&self, module: u16, seat1: u8) -> usize {
        self.widgets
            .iter()
            .filter(|(k, _)| k.module == module && k.seat1 == seat1)
            .map(|(_, w)| w.text.len())
            .sum()
    }

    /// Create or replace a widget. Returns `0` or [`err`].
    pub fn set(
        &mut self,
        module: u16,
        seat1: u8,
        id: u16,
        mut widget: HudWidget,
        now_tic: u32,
    ) -> i32 {
        if seat1 > 8 {
            return err::BAD_PARAMS;
        }
        if widget.text.len() > MAX_TEXT_BYTES {
            widget.text.truncate(MAX_TEXT_BYTES);
        }
        let key = WidgetKey { module, seat1, id };
        let replacing = self.widgets.contains_key(&key);
        if !replacing && self.seat_widget_count(module, seat1) >= MAX_WIDGETS_PER_SEAT {
            return err::CAP;
        }
        let old_text = self.widgets.get(&key).map(|w| w.text.len()).unwrap_or(0);
        let new_total = self
            .seat_text_bytes(module, seat1)
            .saturating_sub(old_text)
            .saturating_add(widget.text.len());
        if new_total > MAX_TEXT_PER_SEAT {
            return err::CAP;
        }
        if replacing {
            if let Some(old) = self.widgets.get(&key) {
                widget.created_tic = old.created_tic;
            }
        } else {
            widget.created_tic = now_tic;
        }
        widget.updated_tic = now_tic;
        self.widgets.insert(key, widget);
        0
    }

    /// Patch a numeric field on an existing widget. Returns `0` or [`err`].
    pub fn patch(
        &mut self,
        module: u16,
        seat1: u8,
        id: u16,
        field: u8,
        value: i32,
        now_tic: u32,
    ) -> i32 {
        let key = WidgetKey { module, seat1, id };
        let Some(w) = self.widgets.get_mut(&key) else {
            return err::MISSING;
        };
        match field {
            patch_field::VALUE => w.value = value,
            patch_field::MAX => w.max = value,
            patch_field::ALPHA => w.alpha = value.clamp(0, 1000) as u16,
            patch_field::X => w.x = value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            patch_field::Y => w.y = value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            patch_field::COLOR => w.color = value as u32,
            patch_field::TEXT_COLOR => w.text_color = value.clamp(0, 255) as u8,
            patch_field::HIDDEN => {
                if value != 0 {
                    w.flags |= flags::HIDDEN;
                } else {
                    w.flags &= !flags::HIDDEN;
                }
            }
            patch_field::Z => w.z = value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            _ => return err::BAD_PARAMS,
        }
        w.updated_tic = now_tic;
        0
    }

    /// Start a linear tween of `field` toward `target` over `tics` (`docs/27` E12).
    pub fn animate(
        &mut self,
        module: u16,
        seat1: u8,
        id: u16,
        field: u8,
        target: i32,
        tics: u16,
        now_tic: u32,
    ) -> i32 {
        let key = WidgetKey { module, seat1, id };
        let Some(w) = self.widgets.get_mut(&key) else {
            return err::MISSING;
        };
        let start = match field {
            patch_field::VALUE => w.value,
            patch_field::MAX => w.max,
            patch_field::ALPHA => i32::from(w.alpha),
            patch_field::X => i32::from(w.x),
            patch_field::Y => i32::from(w.y),
            patch_field::Z => i32::from(w.z),
            _ => return err::BAD_PARAMS,
        };
        if tics == 0 {
            let _ = self.patch(module, seat1, id, field, target, now_tic);
            return 0;
        }
        w.tween_field = field;
        w.tween_target = target;
        w.tween_start = start;
        w.tween_duration = tics;
        w.tween_tics_left = tics;
        w.updated_tic = now_tic;
        0
    }

    /// Copy live actor positions into MARKER `world_xyz` (MH4 follow).
    pub fn sync_marker_actors<F>(&mut self, mut resolve: F)
    where
        F: FnMut(u64) -> Option<[i32; 3]>,
    {
        for w in self.widgets.values_mut() {
            if w.kind != kind::MARKER || w.actor == 0 {
                continue;
            }
            if let Some(xyz) = resolve(w.actor) {
                w.world_xyz = xyz;
            }
        }
    }

    /// Advance all tweens one tic; end values are exact when `tween_tics_left` hits 0.
    pub fn tick_tweens(&mut self, now_tic: u32) {
        let keys: Vec<WidgetKey> = self.widgets.keys().copied().collect();
        for key in keys {
            let Some(w) = self.widgets.get_mut(&key) else {
                continue;
            };
            if w.tween_field == 255 || w.tween_tics_left == 0 {
                continue;
            }
            w.tween_tics_left = w.tween_tics_left.saturating_sub(1);
            let dur = i32::from(w.tween_duration.max(1));
            let done = i32::from(w.tween_duration.saturating_sub(w.tween_tics_left));
            let value = if w.tween_tics_left == 0 {
                w.tween_target
            } else {
                w.tween_start + (w.tween_target - w.tween_start) * done / dur
            };
            let field = w.tween_field;
            match field {
                patch_field::VALUE => w.value = value,
                patch_field::MAX => w.max = value,
                patch_field::ALPHA => w.alpha = value.clamp(0, 1000) as u16,
                patch_field::X => {
                    w.x = value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                }
                patch_field::Y => {
                    w.y = value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                }
                patch_field::Z => {
                    w.z = value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                }
                _ => {}
            }
            if w.tween_tics_left == 0 {
                w.tween_field = 255;
            }
            w.updated_tic = now_tic;
        }
    }

    /// Remove one widget.
    pub fn remove(&mut self, module: u16, seat1: u8, id: u16) -> i32 {
        let key = WidgetKey { module, seat1, id };
        if self.widgets.remove(&key).is_some() {
            0
        } else {
            err::MISSING
        }
    }

    /// Clear widgets for a seat (`seat1 == 0` clears shared only for that module).
    pub fn clear_seat(&mut self, module: u16, seat1: u8) {
        self.widgets
            .retain(|k, _| !(k.module == module && k.seat1 == seat1));
    }

    /// Remove every widget owned by `module` (trap / hot-swap / unload).
    pub fn clear_module(&mut self, module: u16) {
        self.widgets.retain(|k, _| k.module != module);
    }

    /// Map change: drop widgets without [`flags::PERSIST_MAP`].
    pub fn clear_map(&mut self) {
        self.widgets
            .retain(|_, w| w.flags & flags::PERSIST_MAP != 0);
    }

    /// Clear everything.
    pub fn clear_all(&mut self) {
        self.widgets.clear();
    }

    /// Stable hash for tests (not used in desync checksum).
    #[must_use]
    pub fn hash(&self) -> u64 {
        let mut bytes = Vec::new();
        for (k, w) in &self.widgets {
            bytes.extend_from_slice(&k.module.to_le_bytes());
            bytes.push(k.seat1);
            bytes.extend_from_slice(&k.id.to_le_bytes());
            bytes.push(w.kind);
            bytes.push(w.layer);
            bytes.push(w.anchor);
            bytes.push(w.flags);
            bytes.extend_from_slice(&w.x.to_le_bytes());
            bytes.extend_from_slice(&w.y.to_le_bytes());
            bytes.extend_from_slice(&w.value.to_le_bytes());
            bytes.extend_from_slice(&w.alpha.to_le_bytes());
            bytes.extend_from_slice(&(w.text.len() as u32).to_le_bytes());
            bytes.extend_from_slice(w.text.as_bytes());
            bytes.extend_from_slice(&(w.name.len() as u32).to_le_bytes());
            bytes.extend_from_slice(w.name.as_bytes());
        }
        xxh3_64(&bytes)
    }

    /// Serialize for `modhud.bin` (little-endian, versioned).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"MHUD");
        out.extend_from_slice(&1u32.to_le_bytes()); // version
        out.extend_from_slice(&(self.widgets.len() as u32).to_le_bytes());
        for (k, w) in &self.widgets {
            out.extend_from_slice(&k.module.to_le_bytes());
            out.push(k.seat1);
            out.extend_from_slice(&k.id.to_le_bytes());
            out.push(w.kind);
            out.push(w.layer);
            out.push(w.anchor);
            out.push(w.align);
            out.push(w.font);
            out.push(w.flags);
            out.extend_from_slice(&w.x.to_le_bytes());
            out.extend_from_slice(&w.y.to_le_bytes());
            out.extend_from_slice(&w.w.to_le_bytes());
            out.extend_from_slice(&w.h.to_le_bytes());
            out.extend_from_slice(&w.z.to_le_bytes());
            out.extend_from_slice(&w.color.to_le_bytes());
            out.extend_from_slice(&w.color2.to_le_bytes());
            out.push(w.text_color);
            out.extend_from_slice(&w.value.to_le_bytes());
            out.extend_from_slice(&w.max.to_le_bytes());
            out.extend_from_slice(&w.segments.to_le_bytes());
            out.extend_from_slice(&w.alpha.to_le_bytes());
            out.extend_from_slice(&w.fade_in.to_le_bytes());
            out.extend_from_slice(&w.hold.to_le_bytes());
            out.extend_from_slice(&w.fade_out.to_le_bytes());
            out.extend_from_slice(&w.actor.to_le_bytes());
            for c in w.world_xyz {
                out.extend_from_slice(&c.to_le_bytes());
            }
            out.extend_from_slice(&w.created_tic.to_le_bytes());
            out.extend_from_slice(&w.updated_tic.to_le_bytes());
            write_str(&mut out, &w.name);
            write_str(&mut out, &w.text);
        }
        out
    }

    /// Deserialize [`Self::to_bytes`].
    pub fn from_bytes(bin: &[u8]) -> Result<Self, String> {
        if bin.len() < 12 || &bin[0..4] != b"MHUD" {
            return Err("bad modhud magic".into());
        }
        let ver = u32::from_le_bytes(
            bin[4..8]
                .try_into()
                .map_err(|_| "modhud version truncated")?,
        );
        if ver != 1 {
            return Err(format!("unsupported modhud version {ver}"));
        }
        let n = u32::from_le_bytes(
            bin[8..12]
                .try_into()
                .map_err(|_| "modhud count truncated")?,
        ) as usize;
        let mut o = 12usize;
        let mut hud = Self::new();
        for _ in 0..n {
            if o + 2 + 1 + 2 > bin.len() {
                return Err("modhud truncated".into());
            }
            let module = u16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            let seat1 = bin[o];
            o += 1;
            let id = u16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            let mut w = HudWidget::default();
            let need =
                1 + 1 + 1 + 1 + 1 + 1 + 2 * 5 + 4 + 4 + 1 + 4 + 4 + 2 + 2 * 3 + 8 + 4 * 3 + 4 + 4;
            if o + need > bin.len() {
                return Err("modhud widget truncated".into());
            }
            w.kind = bin[o];
            o += 1;
            w.layer = bin[o];
            o += 1;
            w.anchor = bin[o];
            o += 1;
            w.align = bin[o];
            o += 1;
            w.font = bin[o];
            o += 1;
            w.flags = bin[o];
            o += 1;
            w.x = i16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            w.y = i16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            w.w = i16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            w.h = i16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            w.z = i16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            w.color = u32::from_le_bytes(bin[o..o + 4].try_into().unwrap());
            o += 4;
            w.color2 = u32::from_le_bytes(bin[o..o + 4].try_into().unwrap());
            o += 4;
            w.text_color = bin[o];
            o += 1;
            w.value = i32::from_le_bytes(bin[o..o + 4].try_into().unwrap());
            o += 4;
            w.max = i32::from_le_bytes(bin[o..o + 4].try_into().unwrap());
            o += 4;
            w.segments = u16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            w.alpha = u16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            w.fade_in = u16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            w.hold = u16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            w.fade_out = u16::from_le_bytes(bin[o..o + 2].try_into().unwrap());
            o += 2;
            w.actor = u64::from_le_bytes(bin[o..o + 8].try_into().unwrap());
            o += 8;
            for c in &mut w.world_xyz {
                *c = i32::from_le_bytes(bin[o..o + 4].try_into().unwrap());
                o += 4;
            }
            w.created_tic = u32::from_le_bytes(bin[o..o + 4].try_into().unwrap());
            o += 4;
            w.updated_tic = u32::from_le_bytes(bin[o..o + 4].try_into().unwrap());
            o += 4;
            let (name, n) = read_str(bin, o)?;
            w.name = name;
            o = n;
            let (text, n) = read_str(bin, o)?;
            w.text = text;
            o = n;
            hud.widgets.insert(WidgetKey { module, seat1, id }, w);
        }
        let _ = o;
        Ok(hud)
    }
}

/// `hud_widget_patch` field ids.
pub mod patch_field {
    /// Bar / counter value.
    pub const VALUE: u8 = 0;
    /// Bar max.
    pub const MAX: u8 = 1;
    /// Alpha 0..=1000.
    pub const ALPHA: u8 = 2;
    /// X offset.
    pub const X: u8 = 3;
    /// Y offset.
    pub const Y: u8 = 4;
    /// RGBA color as i32 bits.
    pub const COLOR: u8 = 5;
    /// TEXTCOLO index.
    pub const TEXT_COLOR: u8 = 6;
    /// Hidden flag (nonzero = hide).
    pub const HIDDEN: u8 = 7;
    /// Z order.
    pub const Z: u8 = 8;
}

fn write_str(out: &mut Vec<u8>, s: &str) {
    let n = u16::try_from(s.len().min(u16::MAX as usize)).unwrap_or(0);
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&s.as_bytes()[..usize::from(n)]);
}

fn read_str(bin: &[u8], o: usize) -> Result<(String, usize), String> {
    if o + 2 > bin.len() {
        return Err("modhud string truncated".into());
    }
    let n = u16::from_le_bytes(bin[o..o + 2].try_into().unwrap()) as usize;
    let start = o + 2;
    if start + n > bin.len() {
        return Err("modhud string body truncated".into());
    }
    let s = String::from_utf8_lossy(&bin[start..start + n]).into_owned();
    Ok((s, start + n))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn sample() -> HudWidget {
        let mut w = HudWidget::default();
        w.kind = kind::TEXT;
        w.text = "ROUND 5".into();
        w.value = 42;
        w
    }

    #[test]
    fn set_patch_remove_and_caps() {
        let mut hud = ModHud::new();
        assert_eq!(hud.set(0, 1, 1, sample(), 10), 0);
        assert_eq!(hud.len(), 1);
        assert_eq!(hud.patch(0, 1, 1, patch_field::VALUE, 99, 11), 0);
        assert_eq!(
            hud.widgets[&WidgetKey {
                module: 0,
                seat1: 1,
                id: 1
            }]
                .value,
            99
        );
        assert_eq!(hud.remove(0, 1, 1), 0);
        assert!(hud.is_empty());

        for i in 0..MAX_WIDGETS_PER_SEAT {
            assert_eq!(hud.set(0, 1, i as u16, sample(), 0), 0);
        }
        assert_eq!(hud.set(0, 1, 999, sample(), 0), err::CAP);
        // Other seat still ok.
        assert_eq!(hud.set(0, 2, 1, sample(), 0), 0);
    }

    #[test]
    fn clear_module_and_map_persist() {
        let mut hud = ModHud::new();
        let mut persist = sample();
        persist.flags |= flags::PERSIST_MAP;
        assert_eq!(hud.set(0, 0, 1, sample(), 0), 0);
        assert_eq!(hud.set(0, 0, 2, persist.clone(), 0), 0);
        assert_eq!(hud.set(1, 0, 1, sample(), 0), 0);
        hud.clear_module(0);
        assert_eq!(hud.len(), 1);
        assert!(hud.widgets.contains_key(&WidgetKey {
            module: 1,
            seat1: 0,
            id: 1
        }));
        hud.clear_map();
        assert!(hud.is_empty()); // module 1 widget had no PERSIST
        assert_eq!(hud.set(1, 0, 1, persist, 0), 0);
        hud.clear_map();
        assert_eq!(hud.len(), 1);
    }

    #[test]
    fn bytes_round_trip_and_hash_stable() {
        let mut hud = ModHud::new();
        assert_eq!(hud.set(3, 0, 7, sample(), 5), 0);
        let mut w2 = sample();
        w2.text = "Marks: 12".into();
        w2.name = "M_SKULL1".into();
        assert_eq!(hud.set(3, 1, 2, w2, 6), 0);
        let bin = hud.to_bytes();
        let restored = ModHud::from_bytes(&bin).unwrap();
        assert_eq!(hud, restored);
        assert_eq!(hud.hash(), restored.hash());
    }

    #[test]
    fn abi_ids_match_script_abi() {
        use room_script_abi::hud::{self, patch};
        assert_eq!(kind::TEXT, hud::kind::TEXT);
        assert_eq!(kind::MARKER, hud::kind::MARKER);
        assert_eq!(layer::MODAL, hud::layer::MODAL);
        assert_eq!(flags::HIDDEN, hud::flags::HIDDEN);
        assert_eq!(flags::PERSIST_MAP, hud::flags::PERSIST_MAP);
        assert_eq!(i32::from(patch_field::VALUE), patch::VALUE);
        assert_eq!(i32::from(patch_field::Z), patch::Z);
        assert_eq!(i32::from(patch_field::HIDDEN), patch::HIDDEN);
    }

    #[test]
    fn for_seat_includes_shared() {
        let mut hud = ModHud::new();
        assert_eq!(hud.set(0, 0, 1, sample(), 0), 0); // shared
        assert_eq!(hud.set(0, 1, 2, sample(), 0), 0); // seat 0
        assert_eq!(hud.set(0, 2, 3, sample(), 0), 0); // seat 1
        assert_eq!(hud.for_seat(0).count(), 2);
        assert_eq!(hud.for_seat(1).count(), 2);
    }

    #[test]
    fn tween_ends_exactly_at_target() {
        let mut hud = ModHud::new();
        let mut w = sample();
        w.value = 0;
        assert_eq!(hud.set(0, 1, 1, w, 0), 0);
        assert_eq!(hud.animate(0, 1, 1, patch_field::VALUE, 100, 10, 0), 0);
        for t in 1..=10 {
            hud.tick_tweens(t);
        }
        let v = hud.iter().next().unwrap().1.value;
        assert_eq!(v, 100);
    }

    #[test]
    fn sync_marker_actors_copies_world_xyz() {
        let mut hud = ModHud::new();
        let mut m = sample();
        m.kind = kind::MARKER;
        m.actor = 0x0000_0002_0000_0001;
        m.world_xyz = [0, 0, 0];
        assert_eq!(hud.set(0, 1, 9, m, 0), 0);
        hud.sync_marker_actors(|bits| {
            assert_eq!(bits, 0x0000_0002_0000_0001);
            Some([111, 222, 33])
        });
        let xyz = hud.iter().next().unwrap().1.world_xyz;
        assert_eq!(xyz, [111, 222, 33]);
    }
}
