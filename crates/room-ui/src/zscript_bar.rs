//! ZScript UI-scope status bar (`docs/34` ZS7).
//!
//! Runs a compiled `Draw` function that emits draw commands through natives
//! into a [`UiDrawList`]. Client-local only — never part of lockstep.

use crate::draw::{DrawCmd, Rect, Rgba, UiDrawList, Vec2};
use crate::hud::HudState;
use crate::statusbar::StatusBar;
use room_zvm::{CompiledFunction, CompiledProgram, Op, Zvm};
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    static PENDING: RefCell<Vec<DrawCmd>> = const { RefCell::new(Vec::new()) };
    static HUD: RefCell<HudSnap> = const { RefCell::new(HudSnap {
        health: 0,
        armor: 0,
        ammo: 0,
    }) };
}

#[derive(Clone, Copy, Default)]
struct HudSnap {
    health: i32,
    armor: i32,
    ammo: i32,
}

/// Status bar driven by a ZScript `Draw` method (bytecode).
pub struct ZscriptStatusBar {
    program: Arc<CompiledProgram>,
    draw_name: String,
}

impl ZscriptStatusBar {
    /// Wrap a program that exports `draw_name` (e.g. `MyStatusBar.Draw`).
    #[must_use]
    pub fn new(program: CompiledProgram, draw_name: impl Into<String>) -> Self {
        Self {
            program: Arc::new(program),
            draw_name: draw_name.into(),
        }
    }

    /// Build a tiny demo bar program: draws health as a filled rect width.
    #[must_use]
    pub fn demo_program() -> CompiledProgram {
        let mut f = CompiledFunction {
            name: "DemoBar.Draw".into(),
            n_i: 4,
            ..Default::default()
        };
        // Call native ui.draw_hud_numbers (patched at runtime).
        f.emit_ext(Op::CallNative, 0, 0, 0, 0);
        f.emit(Op::Return, 0, 0, 0);
        let mut p = CompiledProgram {
            functions: vec![f],
            ..Default::default()
        };
        p.finalize();
        p
    }
}

impl StatusBar for ZscriptStatusBar {
    fn draw(&self, hud: &HudState, list: &mut UiDrawList) {
        HUD.with(|h| {
            *h.borrow_mut() = HudSnap {
                health: hud.health,
                armor: hud.armor,
                ammo: hud.ammo,
            };
        });
        PENDING.with(|p| p.borrow_mut().clear());
        let mut vm = Zvm::new((*self.program).clone());
        let id = vm.natives.register("ui.draw_hud_numbers", native_draw_hud);
        if let Some(f) = std::sync::Arc::make_mut(&mut vm.program)
            .functions
            .first_mut()
        {
            let mut i = 0;
            while i < f.code.len() {
                let (op, _, _, _) = room_zvm::unpack(f.code[i]);
                if op == room_zvm::Op::CallNative as u8 && i + 1 < f.code.len() {
                    f.code[i + 1] = id;
                    break;
                }
                i += 1;
            }
        }
        let _ = vm.call(&self.draw_name);
        PENDING.with(|p| {
            for cmd in p.borrow_mut().drain(..) {
                list.cmds.push(cmd);
            }
        });
    }
}

fn native_draw_hud(
    _ctx: &mut room_zvm::ZvmCtx<'_>,
    _regs: &mut room_zvm::NativeRegs<'_>,
) -> Result<(), room_zvm::Trap> {
    let snap = HUD.with(|h| *h.borrow());
    let w = f32::from(snap.health.clamp(0, 200) as u16).max(1.0);
    PENDING.with(|p| {
        p.borrow_mut().push(DrawCmd::Fill {
            rect: Rect::new(4.0, 176.0, w, 8.0),
            color: Rgba::HEALTH,
        });
        p.borrow_mut().push(DrawCmd::Text {
            font: "SmallFont".into(),
            pos: Vec2::new(4.0, 160.0),
            text: format!("{}%", snap.health),
            color: Rgba::WHITE,
            scale: 1.0,
        });
    });
    let _ = (snap.armor, snap.ammo);
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::hud::HudState;

    fn cmds_fingerprint(list: &UiDrawList) -> String {
        let mut s = String::new();
        for cmd in &list.cmds {
            match cmd {
                DrawCmd::Fill { rect, color } => {
                    s.push_str(&format!(
                        "F:{:.0},{:.0},{:.0},{:.0},{:.2},{:.2},{:.2};",
                        rect.x, rect.y, rect.w, rect.h, color.r, color.g, color.b
                    ));
                }
                DrawCmd::Text {
                    font,
                    pos,
                    text,
                    color,
                    scale,
                } => {
                    s.push_str(&format!(
                        "T:{font},{:.0},{:.0},{text},{:.1},{:.2};",
                        pos.x, pos.y, scale, color.r
                    ));
                }
                other => s.push_str(&format!("O:{other:?};")),
            }
        }
        s
    }

    #[test]
    fn zscript_bar_emits_cmds() {
        let bar = ZscriptStatusBar::new(ZscriptStatusBar::demo_program(), "DemoBar.Draw");
        let hud = HudState {
            health: 80,
            ..HudState::default()
        };
        let mut list = UiDrawList::new();
        bar.draw(&hud, &mut list);
        assert!(
            !list.cmds.is_empty(),
            "expected draw commands from ZScript bar"
        );
    }

    #[test]
    fn zscript_bar_golden_cmd_fingerprint() {
        // ZS7 stand-in for a golden screenshot: stable draw-list fingerprint.
        let bar = ZscriptStatusBar::new(ZscriptStatusBar::demo_program(), "DemoBar.Draw");
        let mut list = UiDrawList::new();
        bar.draw(
            &HudState {
                health: 100,
                ..HudState::default()
            },
            &mut list,
        );
        let fp100 = cmds_fingerprint(&list);
        assert_eq!(
            fp100, "F:4,176,100,8,0.85,0.15,0.10;T:SmallFont,4,160,100%,1.0,1.00;",
            "health=100 golden fingerprint drifted: {fp100}"
        );
        list.cmds.clear();
        bar.draw(
            &HudState {
                health: 50,
                ..HudState::default()
            },
            &mut list,
        );
        let fp50 = cmds_fingerprint(&list);
        assert_eq!(
            fp50, "F:4,176,50,8,0.85,0.15,0.10;T:SmallFont,4,160,50%,1.0,1.00;",
            "health=50 golden fingerprint drifted: {fp50}"
        );
        assert_ne!(fp100, fp50);
    }

    /// Soft framebuffer for ZS7 golden pixel smoke (fills only; text skipped).
    struct RgbaFb {
        w: u32,
        h: u32,
        px: Vec<u8>,
    }

    impl RgbaFb {
        fn new(w: u32, h: u32) -> Self {
            Self {
                w,
                h,
                px: vec![0; (w as usize) * (h as usize) * 4],
            }
        }

        fn digest(&self) -> u64 {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for &b in &self.px {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
            h
        }
    }

    impl crate::canvas_bridge::CanvasSink for RgbaFb {
        fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
            let x0 = x.floor().max(0.0) as i32;
            let y0 = y.floor().max(0.0) as i32;
            let x1 = (x + w).ceil().min(self.w as f32) as i32;
            let y1 = (y + h).ceil().min(self.h as f32) as i32;
            let r = (color[0].clamp(0.0, 1.0) * 255.0) as u8;
            let g = (color[1].clamp(0.0, 1.0) * 255.0) as u8;
            let b = (color[2].clamp(0.0, 1.0) * 255.0) as u8;
            let a = (color[3].clamp(0.0, 1.0) * 255.0) as u8;
            for yy in y0.max(0)..y1.min(self.h as i32) {
                for xx in x0.max(0)..x1.min(self.w as i32) {
                    let i = ((yy as u32 * self.w + xx as u32) * 4) as usize;
                    self.px[i] = r;
                    self.px[i + 1] = g;
                    self.px[i + 2] = b;
                    self.px[i + 3] = a;
                }
            }
        }

        fn text(&mut self, _: f32, _: f32, _: &str, _: [f32; 4]) {
            // Soft golden ignores glyph raster; fill bar is the pixel contract.
        }
    }

    #[test]
    fn zscript_bar_golden_pixel_digest() {
        use crate::canvas_bridge::blit_draw_list;
        let bar = ZscriptStatusBar::new(ZscriptStatusBar::demo_program(), "DemoBar.Draw");
        let mut list = UiDrawList::new();
        bar.draw(
            &HudState {
                health: 100,
                ..HudState::default()
            },
            &mut list,
        );
        let mut fb = RgbaFb::new(320, 200);
        blit_draw_list(&list, 320.0, 200.0, &mut fb);
        let d100 = fb.digest();
        assert_ne!(d100, 0xcbf2_9ce4_8422_2325, "expected filled pixels");
        // Stable golden for 320×200 soft blit of health=100 bar (fills only).
        assert_eq!(
            d100, 0xdaf6_a130_5ef7_a5e5,
            "health=100 pixel digest drifted: {d100:#x}"
        );
        list.cmds.clear();
        bar.draw(
            &HudState {
                health: 50,
                ..HudState::default()
            },
            &mut list,
        );
        let mut fb50 = RgbaFb::new(320, 200);
        blit_draw_list(&list, 320.0, 200.0, &mut fb50);
        let d50 = fb50.digest();
        assert_ne!(d100, d50, "health width must change pixel digest");
    }
}
