//! The launcher screen inside the app (docs/41 M4): input routing, CVars
//! for the settings page, and drawing onto the HUD canvas.

use super::ViewerApp;
use crate::launcher_screen::{Draw, Nav, Outcome, SETTING_CVARS};
use room_render::Canvas;
use std::collections::HashMap;
use winit::event::Touch;
use winit::keyboard::KeyCode;

impl ViewerApp {
    /// Current text of the settings-page CVars.
    fn launcher_cvars(&self) -> HashMap<&'static str, String> {
        SETTING_CVARS
            .iter()
            .map(|name| {
                let value = match self.console.cvars.get(name) {
                    Some(room_console::CvarValue::Bool(b)) => b.to_string(),
                    Some(room_console::CvarValue::Int(i)) => i.to_string(),
                    Some(room_console::CvarValue::Float(f)) => format!("{f}"),
                    Some(room_console::CvarValue::String(s)) => s.clone(),
                    None => String::new(),
                };
                (*name, value)
            })
            .collect()
    }

    fn launcher_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::None => {}
            Outcome::Start(config) => {
                eprintln!(
                    "room: launcher starts {}",
                    config
                        .iwad
                        .as_ref()
                        .map_or_else(String::new, |p| p.display().to_string())
                );
                self.start_request = Some(*config);
            }
            Outcome::FindLan => self.find_lan_request = true,
            Outcome::CancelLobby => crate::netplay::cancel_lobby(),
            Outcome::SetCvar(name, value) => {
                if let Err(e) = self.console.cvars.set(&name, &value) {
                    eprintln!("room: {name}: {e}");
                }
                self.write_user_config();
            }
        }
    }

    /// Keyboard on the launcher; `true` when the key was used.
    pub(super) fn launcher_key(&mut self, code: KeyCode) -> bool {
        let nav = match code {
            KeyCode::ArrowUp | KeyCode::KeyW => Nav::Up,
            KeyCode::ArrowDown | KeyCode::KeyS => Nav::Down,
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => Nav::Enter,
            KeyCode::Escape | KeyCode::Backspace | KeyCode::BrowserBack => Nav::Back,
            _ => return false,
        };
        self.launcher_nav(nav);
        true
    }

    /// Up / down / enter / back on the launcher.
    pub(super) fn launcher_nav(&mut self, nav: Nav) {
        let cvars = self.launcher_cvars();
        let get = |n: &str| cvars.get(n).cloned().unwrap_or_default();
        let outcome = match self.launcher.as_mut() {
            Some(l) => l.nav(nav, &get),
            None => return,
        };
        self.launcher_outcome(outcome);
    }

    /// A finger on the launcher.
    pub(super) fn launcher_touch(&mut self, touch: &Touch) {
        let phase = match touch.phase {
            winit::event::TouchPhase::Started => room_input::touch::TouchPhase::Started,
            winit::event::TouchPhase::Moved => room_input::touch::TouchPhase::Moved,
            winit::event::TouchPhase::Ended => room_input::touch::TouchPhase::Ended,
            winit::event::TouchPhase::Cancelled => room_input::touch::TouchPhase::Cancelled,
        };
        self.launcher_finger(
            touch.id,
            phase,
            touch.location.x as f32,
            touch.location.y as f32,
        );
    }

    /// A finger event in framebuffer pixels.
    pub(super) fn launcher_finger(
        &mut self,
        id: u64,
        phase: room_input::touch::TouchPhase,
        x: f32,
        y: f32,
    ) {
        let Some((w, h)) = self
            .config
            .as_ref()
            .map(|c| (c.width as f32, c.height as f32))
        else {
            return;
        };
        let scale = self
            .window
            .as_ref()
            .map_or(1.0, |w| w.scale_factor() as f32);
        let cvars = self.launcher_cvars();
        let get = |n: &str| cvars.get(n).cloned().unwrap_or_default();
        let outcome = match self.launcher.as_mut() {
            Some(l) => l.touch(id, phase, x, y, (w, h, scale), &get),
            None => return,
        };
        self.launcher_outcome(outcome);
    }

    /// Lay the launcher out for this frame (drawn by the HUD pass).
    pub(super) fn prepare_launcher_frame(&mut self) {
        let Some((w, h)) = self
            .config
            .as_ref()
            .map(|c| (c.width as f32, c.height as f32))
        else {
            return;
        };
        let scale = self
            .window
            .as_ref()
            .map_or(1.0, |w| w.scale_factor() as f32);
        let cvars = self.launcher_cvars();
        let get = |n: &str| cvars.get(n).cloned().unwrap_or_default();
        if let Some(launcher) = self.launcher.as_mut() {
            self.launcher_frame = launcher.draw(w, h, scale, &get);
        }
    }
}

/// A launcher backdrop for the GPU: where it goes, and its picture.
pub(super) struct BackdropQuad {
    pub quad: room_render::SpriteQuad,
    pub w: u32,
    pub h: u32,
    pub rgba: std::sync::Arc<Vec<u8>>,
}

/// Paint prepared launcher draws onto the HUD canvas; the backdrops are
/// returned for the GPU (drawn under the canvas, covering a `screen` of
/// `(w, h)` px, the picture's right edge on the screen's).
pub(super) fn paint_launcher(
    hud: &mut Canvas,
    draws: &[Draw],
    screen: (f32, f32),
) -> Vec<BackdropQuad> {
    let mut backdrops = Vec::new();
    for draw in draws {
        match draw {
            Draw::Rect { x, y, w, h, color } => hud.fill_rect(*x, *y, *w, *h, *color),
            Draw::Image { x, y, w, h, rgba } => hud.blit_rgba(*x, *y, *w, *h, rgba.clone()),
            Draw::Backdrop {
                key,
                w,
                h,
                rgba,
                alpha,
            } => {
                let (sw, sh) = screen;
                let k = (sw / *w as f32).max(sh / *h as f32);
                let (qw, qh) = ((*w as f32 * k).ceil(), (*h as f32 * k).ceil());
                backdrops.push(BackdropQuad {
                    quad: room_render::SpriteQuad {
                        key: key.clone(),
                        x: (sw - qw) as i32,
                        y: ((sh - qh) / 2.0) as i32,
                        w: qw as u32,
                        h: qh as u32,
                        light: 1.0,
                        tint: [1.0; 3],
                        alpha: *alpha,
                        flip: false,
                        mirror: false,
                        smooth: true,
                    },
                    w: *w,
                    h: *h,
                    rgba: rgba.clone(),
                });
            }
        }
    }
    backdrops
}
