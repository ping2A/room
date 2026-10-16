//! Rasterize [`UiDrawList`] into renderer [`room_render::Canvas`] commands.
//!
//! Kept here so `room-ui` can stay GPU-free while shells share one path.
//! The renderer canvas only supports fill/text; lines become thin rects.

/// Minimal canvas sink (avoids a hard dep on room-render from unit tests).
pub trait CanvasSink {
    /// Filled axis-aligned rect in screen pixels.
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]);
    /// Text run (fallback when no [`crate::FontRegistry`] glyph is available).
    fn text(&mut self, x: f32, y: f32, text: &str, color: [f32; 4]);
}

use crate::draw::{DrawCmd, UiDrawList, Vec2};
use crate::font::FontRegistry;
use crate::patches::PatchCache;

/// Scale virtual 320×200 cmds into a pixel framebuffer of `fb_w`×`fb_h`.
pub fn blit_draw_list(list: &UiDrawList, fb_w: f32, fb_h: f32, sink: &mut impl CanvasSink) {
    blit_draw_list_with_fonts(list, fb_w, fb_h, None, sink);
}

/// Like [`blit_draw_list`], but paints [`DrawCmd::Text`] via soft fonts when present.
pub fn blit_draw_list_with_fonts(
    list: &UiDrawList,
    fb_w: f32,
    fb_h: f32,
    fonts: Option<&FontRegistry>,
    sink: &mut impl CanvasSink,
) {
    blit_draw_list_full(list, fb_w, fb_h, fonts, None, sink);
}

/// Doom `INVERSECOLORMAP` applied to a palette: white→black inverted luminance.
#[must_use]
pub fn inverse_palette(pal: &[[u8; 3]; 256]) -> [[u8; 3]; 256] {
    pal.map(|[r, g, b]| {
        let lum = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
        let v = 255 - lum.min(255) as u8;
        [v, v, v]
    })
}

/// Full blit path: soft fonts + optional Doom picture cache for [`DrawCmd::Patch`].
pub fn blit_draw_list_full(
    list: &UiDrawList,
    fb_w: f32,
    fb_h: f32,
    fonts: Option<&FontRegistry>,
    patches: Option<&PatchCache>,
    sink: &mut impl CanvasSink,
) {
    let sx = fb_w / list.virtual_w;
    let sy = fb_h / list.virtual_h;
    let uniform = sx.min(sy);
    for cmd in &list.cmds {
        match cmd {
            DrawCmd::Fill { rect, color } => {
                sink.fill_rect(
                    rect.x * sx,
                    rect.y * sy,
                    rect.w * sx,
                    rect.h * sy,
                    [color.r, color.g, color.b, color.a],
                );
            }
            DrawCmd::Text {
                font,
                pos,
                text,
                color,
                scale,
            } => {
                if let Some(soft) = fonts.and_then(|r| r.get(font)) {
                    let px = pos.x * sx;
                    let py = pos.y * sy;
                    let sc = (*scale) * uniform;
                    soft.paint_text(text, px, py, sc, *color, |x, y, w, h, rgba| {
                        sink.fill_rect(x, y, w, h, rgba);
                    });
                } else {
                    sink.text(
                        pos.x * sx,
                        pos.y * sy,
                        text,
                        [color.r, color.g, color.b, color.a],
                    );
                }
            }
            DrawCmd::Patch {
                name,
                pos,
                size,
                apply_offsets,
                inverse,
            } => {
                let dest_w = if size.x > 0.0 { size.x } else { 16.0 };
                let dest_h = if size.y > 0.0 { size.y } else { 16.0 };
                if let Some(cache) = patches {
                    if let Some(patch) = cache.get(name) {
                        let pos = if *apply_offsets {
                            Vec2::new(
                                pos.x - f32::from(patch.left_offset),
                                pos.y - f32::from(patch.top_offset),
                            )
                        } else {
                            *pos
                        };
                        let dest_w = if size.x > 0.0 {
                            size.x
                        } else {
                            f32::from(patch.width)
                        };
                        let dest_h = if size.y > 0.0 {
                            size.y
                        } else {
                            f32::from(patch.height)
                        };
                        let scale_x = (dest_w / f32::from(patch.width).max(1.0)) * sx;
                        let scale_y = (dest_h / f32::from(patch.height).max(1.0)) * sy;
                        let inverted;
                        let palette = if *inverse {
                            inverted = inverse_palette(cache.palette());
                            &inverted
                        } else {
                            cache.palette()
                        };
                        patch.paint(
                            pos.x * sx,
                            pos.y * sy,
                            scale_x,
                            scale_y,
                            palette,
                            |x, y, w, h, rgba| sink.fill_rect(x, y, w, h, rgba),
                        );
                        continue;
                    }
                }
                // Placeholder until the lump is cached.
                sink.fill_rect(
                    pos.x * sx,
                    pos.y * sy,
                    dest_w * sx,
                    dest_h * sy,
                    [0.4, 0.4, 0.45, 0.9],
                );
            }
            DrawCmd::Line {
                a,
                b,
                color,
                thickness,
            } => {
                // Proper segment (AABB made every diagonal a solid blob — unusable for automap).
                let x0 = a.x * sx;
                let y0 = a.y * sy;
                let x1 = b.x * sx;
                let y1 = b.y * sy;
                let t = (*thickness * sx.max(sy)).max(1.0);
                let rgba = [color.r, color.g, color.b, color.a];
                stroke_line(sink, x0, y0, x1, y1, t, rgba);
            }
        }
    }
}

/// Thick line as stepped axis-aligned stamps along the segment.
fn stroke_line(
    sink: &mut impl CanvasSink,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    rgba: [f32; 4],
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = dx.hypot(dy);
    if len < 1e-3 {
        let half = thickness * 0.5;
        sink.fill_rect(x0 - half, y0 - half, thickness, thickness, rgba);
        return;
    }
    // Step ≈ half thickness so stamps overlap; caps stay round-ish.
    let step = (thickness * 0.5).max(1.0);
    let n = ((len / step).ceil() as i32).max(1);
    let half = thickness * 0.5;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let x = x0 + dx * t;
        let y = y0 + dy * t;
        sink.fill_rect(x - half, y - half, thickness, thickness, rgba);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::draw::{DrawCmd, Rgba, Vec2};
    use crate::font::{FontId, FontRegistry};
    use std::path::PathBuf;

    #[test]
    fn inverse_palette_is_inverted_greyscale() {
        let mut pal = [[0u8; 3]; 256];
        pal[1] = [255, 255, 255];
        pal[2] = [255, 0, 0];
        let inv = inverse_palette(&pal);
        assert_eq!(inv[0], [255, 255, 255]);
        assert_eq!(inv[1], [0, 0, 0]);
        assert_eq!(inv[2], [179, 179, 179]);
    }

    #[test]
    fn inverse_patch_paints_through_inverted_palette() {
        struct Colors(Vec<[f32; 4]>);
        impl CanvasSink for Colors {
            fn fill_rect(&mut self, _: f32, _: f32, _: f32, _: f32, c: [f32; 4]) {
                self.0.push(c);
            }
            fn text(&mut self, _: f32, _: f32, _: &str, _: [f32; 4]) {}
        }
        let mut cache = PatchCache::default();
        cache.insert(
            "GUN",
            crate::patches::SoftPatch {
                width: 1,
                height: 1,
                left_offset: 0,
                top_offset: 0,
                indices: vec![0],
                alpha: vec![255],
            },
        );
        let mut list = UiDrawList::new();
        list.patch("GUN", Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0));
        list.invert_since(0);
        let mut sink = Colors(Vec::new());
        blit_draw_list_full(&list, 320.0, 200.0, None, Some(&cache), &mut sink);
        assert_eq!(sink.0.len(), 1);
        let [r, g, b, _] = sink.0[0];
        assert!(r == g && g == b, "greyscale: {:?}", sink.0[0]);
        assert!(r > 0.9, "black palette index 0 inverts to white");
    }

    struct CountSink {
        fills: usize,
        texts: usize,
    }
    impl CanvasSink for CountSink {
        fn fill_rect(&mut self, _: f32, _: f32, _: f32, _: f32, _: [f32; 4]) {
            self.fills += 1;
        }
        fn text(&mut self, _: f32, _: f32, _: &str, _: [f32; 4]) {
            self.texts += 1;
        }
    }

    #[test]
    fn softfont_text_blits_as_fills() {
        let wad =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
        if !wad.is_file() {
            return;
        }
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let reg = FontRegistry::load_doom_fonts(&vfs);
        let font = reg.get(FontId::BIG).unwrap();
        let mut list = UiDrawList::new();
        font.draw(
            &mut list,
            FontId::BIG,
            Vec2::new(10.0, 10.0),
            "A",
            Rgba::WHITE,
            1.0,
        );
        let mut sink = CountSink { fills: 0, texts: 0 };
        blit_draw_list_with_fonts(&list, 320.0, 200.0, Some(&reg), &mut sink);
        assert!(sink.fills > 0, "expected glyph pixel fills");
        assert_eq!(sink.texts, 0, "soft font should not use text fallback");
    }

    #[test]
    fn fullscreen_hud_blits_visible_sttnum_pixels() {
        use crate::hud::HudState;
        use crate::patches::PatchCache;
        use crate::statusbar::{DoomStatusBar, StatusBar};

        let wad =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
        if !wad.is_file() {
            return;
        }
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let mut patches = PatchCache::from_vfs(&vfs);
        let mut hud = HudState::doom_default();
        hud.fullscreen = true;
        hud.health = 100;
        hud.ammo = 50;
        hud.armor = 0;
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        for cmd in &list.cmds {
            if let DrawCmd::Patch { name, .. } = cmd {
                let _ = patches.ensure(&vfs, name);
            }
        }
        let mut sink = CountSink { fills: 0, texts: 0 };
        blit_draw_list_full(&list, 640.0, 400.0, None, Some(&patches), &mut sink);
        assert!(
            sink.fills > 80,
            "fullscreen HUD must paint STTNUM pixels, got {} fills",
            sink.fills
        );
        assert_eq!(
            sink.texts, 0,
            "HUD must not rely on CanvasCmd::Text fallback"
        );
    }

    #[test]
    fn classic_stbar_blits_bar_digits_and_face() {
        use crate::hud::HudState;
        use crate::patches::PatchCache;
        use crate::statusbar::{DoomStatusBar, StatusBar};

        let wad =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
        if !wad.is_file() {
            return;
        }
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let mut patches = PatchCache::from_vfs(&vfs);
        let hud = HudState::doom_default();
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        for cmd in &list.cmds {
            if let DrawCmd::Patch { name, .. } = cmd {
                let _ = patches.ensure(&vfs, name);
            }
        }
        assert!(patches.get("STBAR").is_some());
        assert!(patches.get("STTNUM1").is_some());
        assert!(patches.get("STFST00").is_some());
        let mut sink = CountSink { fills: 0, texts: 0 };
        blit_draw_list_full(&list, 640.0, 400.0, None, Some(&patches), &mut sink);
        assert!(
            sink.fills > 400,
            "classic STBAR must paint bar + digits + face, got {} fills",
            sink.fills
        );
    }

    #[test]
    fn freedoom_pistol_flash_uses_leftoffset() {
        use crate::patches::PatchCache;
        use crate::weapon::{draw_weapon, psprite_pos};

        let wad =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
        if !wad.is_file() {
            return;
        }
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let mut patches = PatchCache::from_vfs(&vfs);
        let gun = patches.ensure(&vfs, "PISGA0").cloned().unwrap();
        let flash = patches.ensure(&vfs, "PISFA0").cloned().unwrap();
        let g = psprite_pos(&gun, 0.0, 0.0, false);
        let f = psprite_pos(&flash, 0.0, 0.0, false);
        let dx = f.x - g.x;
        let expect = f32::from(gun.left_offset) - f32::from(flash.left_offset);
        assert!(
            (dx - expect).abs() < 0.5,
            "flash dx={dx} leftoverset delta={expect} gun_lo={} flash_lo={}",
            gun.left_offset,
            flash.left_offset
        );
        assert!(
            f.x > g.x && f.x < g.x + f32::from(gun.width),
            "flash x={} must sit on gun [{}, {}]",
            f.x,
            g.x,
            g.x + f32::from(gun.width)
        );
        let mut list = UiDrawList::new();
        draw_weapon(&mut list, "PISGA0", false, Some(&gun), 0.0, 0.0);
        draw_weapon(&mut list, "PISFA0", false, Some(&flash), 0.0, 0.0);
        match &list.cmds[..] {
            [DrawCmd::Patch { pos: a, .. }, DrawCmd::Patch { pos: b, .. }] => {
                assert!((a.x - g.x).abs() < 1e-3);
                assert!((b.x - f.x).abs() < 1e-3);
            }
            other => panic!("expected gun+flash patches, got {other:?}"),
        }
    }

    #[test]
    fn pause_menu_blits_visible_bigfont_pixels() {
        use crate::font::FontRegistry;
        use crate::menu::ListMenu;
        use crate::patches::PatchCache;

        let wad =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
        if !wad.is_file() {
            return;
        }
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let fonts = FontRegistry::load_doom_fonts(&vfs);
        let mut patches = PatchCache::from_vfs(&vfs);
        let mut list = UiDrawList::new();
        ListMenu::pause().draw(&mut list, 0, Some(&fonts));
        for cmd in &list.cmds {
            if let DrawCmd::Patch { name, .. } = cmd {
                let _ = patches.ensure(&vfs, name);
            }
        }
        let mut sink = CountSink { fills: 0, texts: 0 };
        blit_draw_list_full(&list, 640.0, 400.0, Some(&fonts), Some(&patches), &mut sink);
        assert!(
            sink.fills > 200,
            "pause menu must paint BigFont glyphs + dim, got {} fills",
            sink.fills
        );
        assert_eq!(sink.texts, 0, "BigFont must not fall back to canvas text");
    }
}
