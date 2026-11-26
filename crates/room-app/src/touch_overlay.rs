//! Draw the on-screen touch controls (docs/41 M3) as RGBA sprites for the
//! HUD canvas. Shapes are rasterised on the CPU with anti-aliased edges and
//! cached by size and state, so a frame only copies a dozen small images.
//! Colours follow the web client's `touch.js`.

use room_input::touch::{Placed, Shape, TouchButton, TouchLayout, TouchState};
use std::collections::HashMap;

/// A positioned RGBA8 image (straight alpha).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Sprite {
    pub x: f32,
    pub y: f32,
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

type Rgba = [f32; 4];

const BORDER: Rgba = [220.0, 225.0, 235.0, 0.4];
const FILL: Rgba = [20.0, 24.0, 32.0, 0.35];
const TEXT: Rgba = [240.0, 242.0, 248.0, 0.9];
const ON_FILL: Rgba = [200.0, 70.0, 50.0, 0.55];
const ON_BORDER: Rgba = [255.0, 150.0, 130.0, 0.8];
const STICK_BORDER: Rgba = [220.0, 225.0, 235.0, 0.35];
const STICK_FILL: Rgba = [20.0, 24.0, 32.0, 0.25];
const KNOB: Rgba = [230.0, 235.0, 245.0, 0.45];
const KNOB_RUN: Rgba = [255.0, 190.0, 110.0, 0.7];
const PICKER_BG: Rgba = [10.0, 12.0, 18.0, 0.72];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SpriteKey {
    Button {
        label: TouchButton,
        size: u32,
        square: bool,
        on: bool,
        opacity: u8,
    },
    Ring {
        size: u32,
        idle: bool,
        opacity: u8,
    },
    Knob {
        size: u32,
        run: bool,
        opacity: u8,
    },
    Panel {
        w: u32,
        h: u32,
        opacity: u8,
    },
}

/// Sprite cache for the controls.
#[derive(Default)]
pub(crate) struct TouchOverlay {
    cache: HashMap<SpriteKey, Vec<u8>>,
}

impl TouchOverlay {
    /// Everything to draw this frame, back to front.
    pub(crate) fn sprites(&mut self, layout: &TouchLayout, state: &TouchState) -> Vec<Sprite> {
        let op = (layout.opacity * 255.0).round() as u8;
        let mut out = Vec::new();
        // Stick: under the thumb while held, faint at its resting place otherwise.
        let r = layout.stick_radius;
        let (centre, knob, idle) = match state.stick() {
            Some((c, (dx, dy))) => (c, (c.0 + dx * r, c.1 + dy * r), false),
            None => (layout.idle_stick, layout.idle_stick, true),
        };
        let ring = (r * 2.0).round().max(2.0) as u32;
        out.push(self.sprite(
            SpriteKey::Ring {
                size: ring,
                idle,
                opacity: op,
            },
            centre,
            ring,
            ring,
        ));
        let kn = (layout.knob_radius * 2.0).round().max(2.0) as u32;
        out.push(self.sprite(
            SpriteKey::Knob {
                size: kn,
                run: state.running(),
                opacity: if idle {
                    (f32::from(op) * 0.45) as u8
                } else {
                    op
                },
            },
            knob,
            kn,
            kn,
        ));
        // Weapon picker backdrop.
        let slots: Vec<&Placed> = layout
            .buttons
            .iter()
            .filter(|b| matches!(b.button, TouchButton::Slot(_)))
            .collect();
        if let (Some(first), Some(last)) = (slots.first(), slots.last()) {
            let pad = 10.0 * layout.scale;
            let x0 = first.cx - first.r - pad;
            let y0 = first.cy - first.r - pad;
            let w = (last.cx + last.r + pad - x0).round() as u32;
            let h = (first.r * 2.0 + pad * 2.0).round() as u32;
            out.push(self.sprite(
                SpriteKey::Panel { w, h, opacity: op },
                (x0 + w as f32 / 2.0, y0 + h as f32 / 2.0),
                w,
                h,
            ));
        }
        // The radar is drawn by the app (`radar.rs`), not as a button.
        for b in layout
            .buttons
            .iter()
            .filter(|b| b.button != TouchButton::Radar)
        {
            let size = (b.r * 2.0).round().max(2.0) as u32;
            out.push(self.sprite(
                SpriteKey::Button {
                    label: b.button,
                    size,
                    square: b.shape == Shape::Square,
                    on: state.is_held(b.button),
                    opacity: op,
                },
                (b.cx, b.cy),
                size,
                size,
            ));
        }
        out
    }

    fn sprite(&mut self, key: SpriteKey, centre: (f32, f32), w: u32, h: u32) -> Sprite {
        let rgba = self.cache.entry(key).or_insert_with(|| raster(key)).clone();
        Sprite {
            x: (centre.0 - w as f32 / 2.0).round(),
            y: (centre.1 - h as f32 / 2.0).round(),
            w,
            h,
            rgba,
        }
    }
}

fn raster(key: SpriteKey) -> Vec<u8> {
    match key {
        SpriteKey::Ring {
            size,
            idle,
            opacity,
        } => {
            let o = f32::from(opacity) / 255.0 * if idle { 0.45 } else { 1.0 };
            let mut img = Image::new(size, size);
            img.disc(false, STICK_FILL, STICK_BORDER, 2.0, o);
            img.px
        }
        SpriteKey::Knob { size, run, opacity } => {
            let mut img = Image::new(size, size);
            let c = if run { KNOB_RUN } else { KNOB };
            img.disc(false, c, c, 0.0, f32::from(opacity) / 255.0);
            img.px
        }
        SpriteKey::Panel { w, h, opacity } => {
            let mut img = Image::new(w, h);
            img.rounded_rect(14.0, PICKER_BG, PICKER_BG, 0.0, f32::from(opacity) / 255.0);
            img.px
        }
        SpriteKey::Button {
            label,
            size,
            square,
            on,
            opacity,
        } => {
            let o = f32::from(opacity) / 255.0;
            let mut img = Image::new(size, size);
            let (fill, border) = if on {
                (ON_FILL, ON_BORDER)
            } else {
                (FILL, BORDER)
            };
            let line = (size as f32 / 40.0).max(1.5);
            if square {
                img.rounded_rect(size as f32 * 0.2, fill, border, line, o);
            } else {
                img.disc(false, fill, border, line, o);
            }
            img.text(&label.label(), TEXT, o);
            img.px
        }
    }
}

/// A small RGBA canvas with anti-aliased shapes.
struct Image {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Image {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            px: vec![0; (w * h * 4) as usize],
        }
    }

    /// "Over" blend one pixel with coverage `cov`.
    fn blend(&mut self, x: u32, y: u32, c: Rgba, alpha: f32) {
        if alpha <= 0.0 || x >= self.w || y >= self.h {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        let da = f32::from(self.px[i + 3]) / 255.0;
        let out_a = alpha + da * (1.0 - alpha);
        if out_a <= 0.0 {
            return;
        }
        for (dst, src) in self.px[i..i + 3].iter_mut().zip(c) {
            let d = f32::from(*dst);
            let v = (src * alpha + d * da * (1.0 - alpha)) / out_a;
            *dst = v.round().clamp(0.0, 255.0) as u8;
        }
        self.px[i + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    /// Fill and outline a shape given its signed distance (negative inside).
    fn shape(
        &mut self,
        sdf: impl Fn(f32, f32) -> f32,
        fill: Rgba,
        border: Rgba,
        line: f32,
        o: f32,
    ) {
        for y in 0..self.h {
            for x in 0..self.w {
                let d = sdf(x as f32 + 0.5, y as f32 + 0.5);
                let inside = (0.5 - d).clamp(0.0, 1.0);
                if inside <= 0.0 {
                    continue;
                }
                self.blend(x, y, fill, fill[3] * o * inside);
                if line > 0.0 {
                    // Coverage of the band `-line..0` from the edge.
                    let band = inside * (d + line + 0.5).clamp(0.0, 1.0);
                    self.blend(x, y, border, border[3] * o * band);
                }
            }
        }
    }

    fn disc(&mut self, _square: bool, fill: Rgba, border: Rgba, line: f32, o: f32) {
        let (cx, cy) = (self.w as f32 / 2.0, self.h as f32 / 2.0);
        let r = cx.min(cy);
        self.shape(|x, y| (x - cx).hypot(y - cy) - r, fill, border, line, o);
    }

    fn rounded_rect(&mut self, radius: f32, fill: Rgba, border: Rgba, line: f32, o: f32) {
        let (hw, hh) = (self.w as f32 / 2.0, self.h as f32 / 2.0);
        let rad = radius.min(hw).min(hh);
        self.shape(
            |x, y| {
                let qx = (x - hw).abs() - (hw - rad);
                let qy = (y - hh).abs() - (hh - rad);
                qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - rad
            },
            fill,
            border,
            line,
            o,
        );
    }

    /// Centred label in the built-in 5×7 font, scaled to the button.
    fn text(&mut self, text: &str, c: Rgba, o: f32) {
        if text.is_empty() {
            return;
        }
        let cols = crate::ui_font::columns(text);
        // About a fifth of the button tall; narrow enough to fit four letters.
        let by_h = (self.h as f32 * 0.2 / 7.0).floor();
        let by_w = (self.w as f32 * 0.72 / cols as f32).floor();
        let k = by_h.min(by_w).max(1.0) as u32;
        let x0 = (self.w.saturating_sub(cols * k)) / 2;
        let y0 = (self.h.saturating_sub(7 * k)) / 2;
        crate::ui_font::paint(text, i64::from(x0), i64::from(y0), k, |x, y| {
            if let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) {
                self.blend(x, y, c, c[3] * o);
            }
        });
    }
}

/// Paint `sprites` onto an opaque `w × h` RGB background (tests, previews).
#[cfg(test)]
pub(crate) fn compose(sprites: &[Sprite], w: u32, h: u32, bg: [u8; 3]) -> Vec<u8> {
    let mut px: Vec<u8> = (0..w * h)
        .flat_map(|_| [bg[0], bg[1], bg[2], 255])
        .collect();
    for s in sprites {
        for sy in 0..s.h {
            for sx in 0..s.w {
                let x = s.x as i64 + i64::from(sx);
                let y = s.y as i64 + i64::from(sy);
                if x < 0 || y < 0 || x >= i64::from(w) || y >= i64::from(h) {
                    continue;
                }
                let si = ((sy * s.w + sx) * 4) as usize;
                let a = f32::from(s.rgba[si + 3]) / 255.0;
                let di = ((y as u32 * w + x as u32) * 4) as usize;
                for (dst, src) in px[di..di + 3].iter_mut().zip(&s.rgba[si..si + 3]) {
                    let v = f32::from(*src) * a + f32::from(*dst) * (1.0 - a);
                    *dst = v.round() as u8;
                }
            }
        }
    }
    px
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use room_input::touch::{LayoutOptions, Rect, Screen, TouchEvent, TouchPhase, TouchSettings};

    const BG: [u8; 3] = [0, 128, 0];

    fn bar(sc: &Screen) -> Rect {
        let k = (sc.width / 320.0).min(sc.height / 200.0);
        Rect {
            x: (sc.width - 320.0 * k) / 2.0,
            y: sc.height - 32.0 * k,
            w: 320.0 * k,
            h: 32.0 * k,
        }
    }

    fn render(sc: &Screen, duke: bool, state: &TouchState) -> (TouchLayout, Vec<u8>) {
        let l = TouchLayout::new(
            sc,
            &TouchSettings::default(),
            &LayoutOptions {
                duke,
                slots_open: state.slots_open(),
                map_open: false,
                radar: true,
                avoid: Some(bar(sc)),
            },
        );
        let sprites = TouchOverlay::default().sprites(&l, state);
        let px = compose(&sprites, sc.width as u32, sc.height as u32, BG);
        (l, px)
    }

    fn at(px: &[u8], w: u32, x: f32, y: f32) -> [u8; 3] {
        let i = ((y as u32 * w + x as u32) * 4) as usize;
        [px[i], px[i + 1], px[i + 2]]
    }

    #[test]
    fn nothing_is_drawn_on_the_status_bar() {
        for (w, h, s) in [
            (2400.0, 1080.0, 3.0),
            (2400.0, 1080.0, 2.625),
            (2796.0, 1290.0, 3.0),
        ] {
            let sc = Screen::new(w, h, s);
            for duke in [false, true] {
                let (_, px) = render(&sc, duke, &TouchState::new());
                let b = bar(&sc);
                let mut touched = 0;
                for y in (b.y as u32)..(b.bottom() as u32) {
                    for x in (b.x as u32)..(b.right() as u32) {
                        if at(&px, w as u32, x as f32, y as f32) != BG {
                            touched += 1;
                        }
                    }
                }
                assert_eq!(touched, 0, "{w}×{h}@{s} duke {duke}");
            }
        }
    }

    #[test]
    fn buttons_are_drawn_where_they_are_hit() {
        let sc = Screen::new(2400.0, 1080.0, 2.625);
        let (l, px) = render(&sc, true, &TouchState::new());
        assert!(
            l.get(TouchButton::Radar).is_some(),
            "the radar has a hit area"
        );
        // The radar is drawn by `radar.rs`, not by the overlay.
        for b in l.buttons.iter().filter(|b| b.button != TouchButton::Radar) {
            // The rim sits just inside the edge; the far outside is untouched.
            let rim = at(&px, sc.width as u32, b.cx, b.cy - b.r + 2.0);
            assert_ne!(rim, BG, "{:?} rim", b.button);
            let outside = at(&px, sc.width as u32, b.cx, b.cy - b.r - 3.0);
            assert!(
                outside == BG || l.hit(b.cx, b.cy - b.r - 3.0).is_some(),
                "{:?} bleeds outside",
                b.button
            );
        }
    }

    #[test]
    fn a_held_button_turns_red() {
        let sc = Screen::new(2400.0, 1080.0, 2.625);
        let s = TouchSettings::default();
        let mut state = TouchState::new();
        let (l, idle) = render(&sc, false, &state);
        let fire = *l.get(TouchButton::Fire).unwrap();
        // Sample the button fill between the label and the rim.
        let (x, y) = (fire.cx - fire.r * 0.6, fire.cy);
        state.event(
            TouchEvent {
                id: 1,
                phase: TouchPhase::Started,
                x: fire.cx,
                y: fire.cy,
                time_ms: 0.0,
            },
            &l,
            &s,
        );
        let (_, held) = render(&sc, false, &state);
        let a = at(&idle, sc.width as u32, x, y);
        let b = at(&held, sc.width as u32, x, y);
        assert!(b[0] > a[0] + 40, "red rises: {a:?} → {b:?}");
    }

    #[test]
    fn sprites_are_cached() {
        let sc = Screen::new(2400.0, 1080.0, 2.625);
        let l = TouchLayout::new(&sc, &TouchSettings::default(), &LayoutOptions::default());
        let mut o = TouchOverlay::default();
        let a = o.sprites(&l, &TouchState::new());
        let n = o.cache.len();
        let b = o.sprites(&l, &TouchState::new());
        assert_eq!(a, b);
        assert_eq!(o.cache.len(), n);
    }

    /// `ROOM_TOUCH_PREVIEW=dir cargo test -p room-app touch_overlay` writes PNGs.
    #[test]
    fn preview_pngs() {
        let Ok(dir) = std::env::var("ROOM_TOUCH_PREVIEW") else {
            return;
        };
        let s = TouchSettings::default();
        for (name, w, h, sc, duke, slots) in [
            ("doom-914x411", 2400.0, 1080.0, 2.625, false, false),
            ("duke-800x360", 2400.0, 1080.0, 3.0, true, false),
            ("doom-slots-932x430", 2796.0, 1290.0, 3.0, false, true),
        ] {
            let screen = Screen::new(w, h, sc);
            let mut state = TouchState::new();
            if slots {
                let l = TouchLayout::new(&screen, &s, &LayoutOptions::default());
                let p = *l.get(TouchButton::WeaponNext).unwrap();
                state.event(
                    TouchEvent {
                        id: 9,
                        phase: TouchPhase::Started,
                        x: p.cx,
                        y: p.cy,
                        time_ms: 0.0,
                    },
                    &l,
                    &s,
                );
                state.update(500.0, &s);
            }
            let (_, px) = render(&screen, duke, &state);
            let img = image::RgbaImage::from_raw(w as u32, h as u32, px).unwrap();
            img.save(format!("{dir}/{name}.png")).unwrap();
        }
    }
}
