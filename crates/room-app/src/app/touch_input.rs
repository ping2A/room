//! On-screen touch controls in the app (docs/41 M3): feed winit touches to
//! [`TouchState`], turn each frame's [`TouchFrame`] into the same inputs a
//! keyboard, mouse or controller produces, and decide when the controls show.

use super::ViewerApp;
use crate::platform;
use room_game::GameState;
use room_input::touch::{
    Buzz, LayoutOptions, MenuAction, Rect, Screen, TouchEvent, TouchLayout, TouchMode, TouchPhase,
    TouchSettings,
};
use winit::event::Touch;
use winit::event_loop::ActiveEventLoop;

/// Parse `ROOM_TEST_TOUCH` (see [`ViewerApp::play_test_touches`]).
pub(super) fn parse_test_touches(spec: &str) -> Vec<(u64, u64, TouchPhase, f32, f32)> {
    spec.split(';')
        .filter_map(|ev| {
            let f: Vec<&str> = ev.split(',').map(str::trim).collect();
            let phase = match *f.get(2)? {
                "s" => TouchPhase::Started,
                "m" => TouchPhase::Moved,
                "e" => TouchPhase::Ended,
                "c" => TouchPhase::Cancelled,
                _ => return None,
            };
            Some((
                f.first()?.parse().ok()?,
                f.get(1)?.parse().ok()?,
                phase,
                f.get(3)?.parse().ok()?,
                f.get(4)?.parse().ok()?,
            ))
        })
        .collect()
}

/// Safe-area insets from the inner (safe) rectangle's offset and size
/// within the outer (full-screen) one, all in physical pixels.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub(super) fn safe_area_insets(
    offset: (i32, i32),
    inner: (u32, u32),
    outer: (u32, u32),
) -> platform::Insets {
    let left = u32::try_from(offset.0).unwrap_or(0);
    let top = u32::try_from(offset.1).unwrap_or(0);
    platform::Insets {
        top,
        left,
        right: outer.0.saturating_sub(inner.0).saturating_sub(left),
        bottom: outer.1.saturating_sub(inner.1).saturating_sub(top),
    }
}

/// Analog stick axis → `TicCmd` move units at `run` for a full push.
#[allow(clippy::cast_possible_truncation)]
pub(super) fn analog_units(v: f32, run: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * run).round() as i16
}

/// The 320×200 HUD frame inside a `w × h` framebuffer, letterboxed the
/// same way the HUD is drawn: `(scale, x offset, y offset)`.
fn hud_frame(w: f32, h: f32) -> (f32, f32, f32) {
    let scale = (w / 320.0).min(h / 200.0);
    (scale, (w - 320.0 * scale) * 0.5, (h - 200.0 * scale) * 0.5)
}

/// Status bar strip (`lines` of the 200-line HUD frame) in framebuffer pixels.
pub(super) fn status_bar_rect(w: f32, h: f32, lines: f32) -> Rect {
    let (scale, ox, oy) = hud_frame(w, h);
    Rect {
        x: ox,
        y: oy + (200.0 - lines) * scale,
        w: 320.0 * scale,
        h: lines * scale,
    }
}

impl ViewerApp {
    fn cvar_f(&self, name: &str, default: f32) -> f32 {
        match self.console.cvars.get(name) {
            Some(room_console::CvarValue::Float(v)) => *v as f32,
            Some(room_console::CvarValue::Int(v)) => *v as f32,
            _ => default,
        }
    }

    fn cvar_b(&self, name: &str, default: bool) -> bool {
        match self.console.cvars.get(name) {
            Some(room_console::CvarValue::Bool(v)) => *v,
            Some(room_console::CvarValue::Int(v)) => *v != 0,
            _ => default,
        }
    }

    /// `touch_*` CVars.
    pub(super) fn touch_settings(&self) -> TouchSettings {
        let d = TouchSettings::default();
        TouchSettings {
            look: self.cvar_f("touch_look", d.look).clamp(0.1, 5.0),
            look_y: self.cvar_f("touch_look_y", d.look_y).clamp(0.1, 5.0),
            accel: self.cvar_b("touch_accel", d.accel),
            tap_fire: self.cvar_b("touch_tapfire", d.tap_fire),
            invert_y: self.cvar_b("touch_invert", d.invert_y),
            size: self.cvar_f("touch_size", d.size).clamp(0.5, 2.0),
            opacity: self.cvar_f("touch_opacity", d.opacity).clamp(0.1, 1.0),
            lefty: self.cvar_b("touch_lefty", d.lefty),
            haptics: self.cvar_b("touch_haptics", d.haptics),
            jump_duck: self.cvar_b("touch_jumpduck", d.jump_duck),
        }
    }

    /// Show the controls: `touch_controls` 0 = never, 2 = always,
    /// 1 (default) = on touch-screen devices, or after the first touch.
    pub(super) fn touch_visible(&self) -> bool {
        match self.console.cvars.get("touch_controls") {
            Some(room_console::CvarValue::Int(0)) | Some(room_console::CvarValue::Bool(false)) => {
                false
            }
            Some(room_console::CvarValue::Int(2)) => true,
            _ => platform::current().touch_screen() || self.touch.seen,
        }
    }

    fn touch_mode(&self) -> TouchMode {
        if self.menu.is_some() || self.duke_menu.is_some() {
            TouchMode::Menu
        } else if self.duke_session.is_some()
            || self
                .play
                .as_ref()
                .is_some_and(|g| g.state == GameState::Level)
        {
            TouchMode::Play
        } else {
            TouchMode::TapToContinue
        }
    }

    /// Where the status bar is, so the controls stay off its numbers.
    fn touch_avoid(&self, w: f32, h: f32) -> Option<Rect> {
        if self.duke_session.is_some() {
            let full = !matches!(
                self.console.cvars.get("duke_statusbar"),
                Some(room_console::CvarValue::Int(0)) | Some(room_console::CvarValue::Bool(false))
            );
            return full.then(|| status_bar_rect(w, h, 34.0));
        }
        self.play
            .as_ref()
            .is_some_and(|g| g.state == GameState::Level)
            .then(|| status_bar_rect(w, h, 32.0))
    }

    fn touch_layout_now(&self) -> Option<TouchLayout> {
        let config = self.config.as_ref()?;
        let window = self.window.as_ref()?;
        let (w, h) = (config.width as f32, config.height as f32);
        #[cfg_attr(not(target_os = "ios"), allow(unused_mut))]
        let mut insets = platform::current().safe_area();
        // iOS: winit reports the safe area (notch, home indicator) as the
        // window's inner rectangle inside its outer one.
        #[cfg(target_os = "ios")]
        if insets == platform::Insets::default() {
            if let (Ok(inner), Ok(outer)) = (window.inner_position(), window.outer_position()) {
                insets = safe_area_insets(
                    (inner.x - outer.x, inner.y - outer.y),
                    (window.inner_size().width, window.inner_size().height),
                    (window.outer_size().width, window.outer_size().height),
                );
            }
        }
        let screen = Screen {
            inset_top: insets.top as f32,
            inset_right: insets.right as f32,
            inset_bottom: insets.bottom as f32,
            inset_left: insets.left as f32,
            ..Screen::new(w, h, window.scale_factor() as f32)
        };
        Some(TouchLayout::new(
            &screen,
            &self.touch_settings(),
            &LayoutOptions {
                duke: self.duke_session.is_some(),
                slots_open: self.touch.slots_open(),
                map_open: self.automap && self.menu.is_none() && self.duke_menu.is_none(),
                radar: self.radar_on(),
                avoid: self.touch_avoid(w, h),
            },
        ))
    }

    fn touch_now_ms(&self) -> f64 {
        self.start_instant.elapsed().as_secs_f64() * 1000.0
    }

    /// The minimap radar shows: a Doom level is running with the touch
    /// controls, no menu and no automap, and `touch_radar` is on.
    pub(super) fn radar_on(&self) -> bool {
        self.touch_shown
            && !self.automap
            && self.menu.is_none()
            && self.duke_menu.is_none()
            && self.duke_session.is_none()
            && self.launcher.is_none()
            && self.cvar_b("touch_radar", true)
            && self
                .play
                .as_ref()
                .is_some_and(|g| g.state == GameState::Level && g.world().is_some())
    }

    /// The radar image for this frame (cached while nothing it shows
    /// changes), with where it goes: `(x, y, size, rgba)`.
    pub(super) fn radar_sprite(&mut self) -> Option<(f32, f32, u32, Vec<u8>)> {
        if !self.radar_on() {
            return None;
        }
        let placed = *self
            .touch_layout
            .as_ref()?
            .get(room_input::touch::TouchButton::Radar)?;
        let dp = self.touch_layout.as_ref()?.scale;
        let size = (placed.r * 2.0).round().max(8.0) as u32;
        let seat = usize::from(self.local_seat);
        let world = self.play.as_ref()?.world()?;
        let allmap = world
            .players
            .get(seat)
            .and_then(|p| p.as_ref())
            .is_some_and(|p| {
                p.allmap
                    || p.mo
                        .is_some_and(|mo| room_sim::powerups::reveals_map(world, mo))
            });
        let now = std::time::Instant::now();
        let cache = &mut self.radar_cache;
        // Explored lines change as the player walks: re-read a few times a
        // second, as the web radar does.
        if cache
            .lines_at
            .is_none_or(|at| now.duration_since(at).as_millis() > 350)
            || cache.map != world.map.name
        {
            cache.lines = crate::radar::lines(&world.map);
            cache.lines_at = Some(now);
            cache.map.clone_from(&world.map.name);
        }
        let pose = crate::radar::Pose {
            x: self.camera.eye.x,
            y: self.camera.eye.y,
            yaw_deg: self.yaw.to_degrees(),
        };
        let seen = cache.lines.iter().filter(|l| l.seen).count();
        // Half-unit / quarter-degree steps: standing still re-uses the image.
        #[allow(clippy::cast_possible_truncation)]
        let key = (
            (pose.x * 2.0).round() as i32,
            (pose.y * 2.0).round() as i32,
            (pose.yaw_deg * 4.0).round() as i32,
            seen,
            allmap,
            size,
        );
        if cache.key != Some(key) {
            cache.rgba = crate::radar::render(&cache.lines, pose, allmap, size, dp);
            cache.key = Some(key);
        }
        Some((
            (placed.cx - placed.r).round(),
            (placed.cy - placed.r).round(),
            size,
            cache.rgba.clone(),
        ))
    }

    /// A finger event from winit.
    pub(super) fn touch_event(&mut self, touch: Touch) {
        if self.touch_layout.is_none() {
            self.touch_layout = self.touch_layout_now();
        }
        let Some(layout) = self.touch_layout.clone() else {
            return;
        };
        let phase = match touch.phase {
            winit::event::TouchPhase::Started => TouchPhase::Started,
            winit::event::TouchPhase::Moved => TouchPhase::Moved,
            winit::event::TouchPhase::Ended => TouchPhase::Ended,
            winit::event::TouchPhase::Cancelled => TouchPhase::Cancelled,
        };
        let settings = self.touch_settings();
        let now = self.touch_now_ms();
        self.touch.event(
            TouchEvent {
                id: touch.id,
                phase,
                x: touch.location.x as f32,
                y: touch.location.y as f32,
                time_ms: now,
            },
            &layout,
            &settings,
        );
    }

    /// Test hook: `ROOM_TEST_TOUCH="frame,id,s|m|e,x,y;…"` plays finger
    /// events at those frames (x, y as fractions of the window), so a desktop
    /// run under Xvfb can drive the controls end to end.
    pub(super) fn play_test_touches(&mut self) {
        if self.touch_script.is_empty() {
            return;
        }
        let Some((w, h)) = self
            .config
            .as_ref()
            .map(|c| (c.width as f32, c.height as f32))
        else {
            return;
        };
        while let Some(&(frame, id, phase, fx, fy)) = self.touch_script.first() {
            if frame > self.frames {
                break;
            }
            self.touch_script.remove(0);
            if self.launcher.is_some() {
                self.launcher_finger(id, phase, fx * w, fy * h);
                continue;
            }
            if self.touch_layout.is_none() {
                self.touch_layout = self.touch_layout_now();
            }
            let Some(layout) = self.touch_layout.clone() else {
                continue;
            };
            let settings = self.touch_settings();
            let now = self.touch_now_ms();
            self.touch.event(
                TouchEvent {
                    id,
                    phase,
                    x: fx * w,
                    y: fy * h,
                    time_ms: now,
                },
                &layout,
                &settings,
            );
        }
    }

    /// Once per loop: timers, layout, and this frame's touch input.
    pub(super) fn poll_touch(&mut self, event_loop: &ActiveEventLoop) {
        self.play_test_touches();
        self.touch_shown = self.touch_visible();
        if !self.touch_shown {
            self.touch_keys.clear();
            self.touch_analog = None;
            return;
        }
        let settings = self.touch_settings();
        self.touch.set_mode(self.touch_mode());
        self.touch.update(self.touch_now_ms(), &settings);
        self.touch_layout = self.touch_layout_now();
        let frame = self.touch.take_frame();
        self.touch_keys = frame.keys;
        self.touch_analog = frame.analog;
        if frame.look != (0, 0) {
            self.add_touch_look(frame.look.0, frame.look.1);
        }
        if frame.weapon_cycle != 0 {
            self.keys.weapon_cycle += frame.weapon_cycle;
        }
        if let Some(slot) = frame.weapon_slot {
            self.keys.weapon = if slot == 0 { 10 } else { slot };
        }
        if frame.toggle_map {
            self.automap = !self.automap;
        }
        if frame.map_zoom != 0 && self.automap {
            self.zoom_automap(1.25_f32.powi(frame.map_zoom));
        }
        if frame.toggle_menu {
            self.touch_menu_button(event_loop);
        }
        for action in frame.menu {
            self.touch_menu_action(action, event_loop);
        }
        for buzz in frame.buzz {
            platform::current().haptic(match buzz {
                Buzz::Light => platform::Haptic::Light,
                Buzz::Heavy => platform::Haptic::Heavy,
            });
        }
    }

    /// Look by whole mouse counts (`+x` right, `+y` down), as the mouse
    /// does, but without the mouse's own sensitivity and invert settings.
    fn add_touch_look(&mut self, dx: i32, dy: i32) {
        let rad_per_count =
            (room_input::MOUSE_DEG_PER_COUNT as f32) * (std::f32::consts::PI / 180.0);
        let yaw_add = -dx;
        let pitch_add = -dy;
        self.yaw += yaw_add as f32 * rad_per_count;
        self.pitch = (self.pitch + pitch_add as f32 * rad_per_count).clamp(-1.5, 1.5);
        self.mouse_yaw_accum = self
            .mouse_yaw_accum
            .saturating_add(yaw_add.clamp(-4096, 4096));
        self.mouse_pitch_accum = self
            .mouse_pitch_accum
            .saturating_add(pitch_add.clamp(-4096, 4096));
        self.sync_forward();
    }

    /// ☰: open the game menu, or go back / close it (Esc without quitting).
    fn touch_menu_button(&mut self, event_loop: &ActiveEventLoop) {
        if self.duke_session.is_some() {
            if self.duke_menu.is_some() {
                self.duke_menu_key(room_duke::menu::MenuKey::Escape, event_loop);
            } else if self.menu.is_some() {
                self.close_duke_menu();
            } else {
                self.open_duke_pause_menu();
            }
            return;
        }
        if self.menu.as_ref().is_some_and(|m| m.stack.len() > 1) {
            if let Some(sfx) = self.menu.as_mut().and_then(|m| m.backup().1) {
                self.play_menu_sfx(sfx);
            }
            return;
        }
        let in_level = self
            .play
            .as_ref()
            .is_some_and(|g| g.state == GameState::Level);
        if in_level {
            self.toggle_pause();
        }
    }

    /// Screen px to the menu's 320×200 space (letterboxed, centred).
    fn menu_virtual(&self, x: f32, y: f32) -> (f32, f32) {
        let (fb_w, fb_h) = self
            .config
            .as_ref()
            .map(|c| (c.width as f32, c.height as f32))
            .unwrap_or((1280.0, 720.0));
        let scale = (fb_w / 320.0).min(fb_h / 200.0);
        let ox = (fb_w - 320.0 * scale) * 0.5;
        let oy = (fb_h - 200.0 * scale) * 0.5;
        (
            ((x - ox) / scale).clamp(0.0, 320.0),
            ((y - oy) / scale).clamp(0.0, 200.0),
        )
    }

    fn touch_menu_action(&mut self, action: MenuAction, event_loop: &ActiveEventLoop) {
        if self.duke_menu.is_some() {
            let key = match action {
                MenuAction::Up => room_duke::menu::MenuKey::Up,
                MenuAction::Down => room_duke::menu::MenuKey::Down,
                MenuAction::Activate | MenuAction::ActivateAt { .. } => {
                    room_duke::menu::MenuKey::Enter
                }
                MenuAction::Point { .. } => return,
            };
            self.duke_menu_key(key, event_loop);
            return;
        }
        if self.menu.is_some() {
            match action {
                MenuAction::Up | MenuAction::Down => {
                    let step = if action == MenuAction::Up { -1 } else { 1 };
                    if let Some(sfx) = self.menu.as_mut().and_then(|m| m.move_cursor(step)) {
                        self.play_menu_sfx(sfx);
                    }
                }
                MenuAction::Activate => self.menu_activate(event_loop),
                // Touch the item you want: it lights up under the finger,
                // and a tap chooses it (a tap beside the items chooses the
                // lit one, as before).
                MenuAction::Point { x, y } => {
                    let (vx, vy) = self.menu_virtual(x as f32, y as f32);
                    self.menu_cursor = (vx, vy);
                    if let Some(sfx) = self.menu.as_mut().and_then(|m| m.hover(vx, vy)) {
                        self.play_menu_sfx(sfx);
                    }
                }
                MenuAction::ActivateAt { x, y } => {
                    let (vx, vy) = self.menu_virtual(x as f32, y as f32);
                    if let Some(sfx) = self.menu.as_mut().and_then(|m| m.hover(vx, vy)) {
                        self.play_menu_sfx(sfx);
                    }
                    self.menu_activate(event_loop);
                }
            }
            return;
        }
        if matches!(action, MenuAction::Activate | MenuAction::ActivateAt { .. })
            && self
                .play
                .as_ref()
                .is_some_and(|g| g.state != GameState::Level)
        {
            // Intermission / text screens: a tap continues, like Enter.
            self.play_advance();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_bar_follows_the_letterboxed_hud() {
        // 2400×1080: HUD 1728 wide, centred; bar is the bottom 32 lines.
        let r = status_bar_rect(2400.0, 1080.0, 32.0);
        assert_eq!((r.x, r.w), (336.0, 1728.0));
        assert!((r.y - 907.2).abs() < 0.01 && (r.bottom() - 1080.0).abs() < 0.01);
        // A 4:3 window letterboxes top and bottom instead.
        let r = status_bar_rect(1024.0, 768.0, 32.0);
        assert_eq!(r.x, 0.0);
        assert!((r.bottom() - (768.0 - (768.0 - 640.0) / 2.0)).abs() < 0.01);
    }

    #[test]
    fn test_touch_script_parses() {
        let t = parse_test_touches("10,1,s,0.2,0.7; 12,1,m,0.2,0.5;bad;20,1,e,0.2,0.5");
        assert_eq!(t.len(), 3);
        assert_eq!(t[0], (10, 1, TouchPhase::Started, 0.2, 0.7));
        assert_eq!(t[2].2, TouchPhase::Ended);
    }

    #[test]
    fn safe_area_from_inner_and_outer_rectangles() {
        // iPhone 15 landscape at 3×: 2556×1179 px, notch side 59 pt, home
        // indicator 21 pt.
        let i = safe_area_insets((177, 0), (2556 - 177 * 2, 1179 - 63), (2556, 1179));
        assert_eq!(
            i,
            platform::Insets {
                top: 0,
                left: 177,
                right: 177,
                bottom: 63
            }
        );
        // No safe area: nothing to keep clear; nonsense offsets clamp to zero.
        assert_eq!(
            safe_area_insets((0, 0), (800, 600), (800, 600)),
            platform::Insets::default()
        );
        assert_eq!(
            safe_area_insets((-5, -5), (900, 700), (800, 600)),
            platform::Insets::default()
        );
    }

    #[test]
    fn analog_units_clamp_and_round() {
        assert_eq!(analog_units(1.0, 12800.0), 12800);
        assert_eq!(analog_units(-2.0, 10240.0), -10240);
        assert_eq!(analog_units(0.5, 12800.0), 6400);
    }
}
