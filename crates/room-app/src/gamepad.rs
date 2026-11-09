//! Desktop game-controller backend (gilrs) → [`room_input::GamepadState`].
//!
//! Built with the default `gamepad` feature; without it [`PadBackend`] is a
//! stub that never reports a controller.

#[cfg_attr(not(feature = "gamepad"), allow(unused_imports))]
use room_input::{GamepadState, PadButton};

/// Polls the first active controller.
pub struct PadBackend {
    #[cfg(feature = "gamepad")]
    gilrs: Option<gilrs::Gilrs>,
    #[cfg(feature = "gamepad")]
    active: Option<gilrs::GamepadId>,
    /// Name of the active controller (for the "connected" message).
    pub name: Option<String>,
}

impl PadBackend {
    /// Open the backend (a failing init just disables controllers).
    #[must_use]
    pub fn new() -> Self {
        #[cfg(feature = "gamepad")]
        {
            let gilrs = match gilrs::Gilrs::new() {
                Ok(g) => Some(g),
                Err(e) => {
                    eprintln!("room: gamepad: {e}");
                    None
                }
            };
            Self {
                gilrs,
                active: None,
                name: None,
            }
        }
        #[cfg(not(feature = "gamepad"))]
        {
            Self { name: None }
        }
    }

    /// Drain events and sample the active controller. Returns `None` when no
    /// controller is connected. The second value is `Some(name)` the first
    /// time a controller becomes active.
    pub fn poll(&mut self) -> (Option<GamepadState>, Option<String>) {
        #[cfg(feature = "gamepad")]
        {
            use gilrs::{Axis, Button, EventType};
            let Some(g) = self.gilrs.as_mut() else {
                return (None, None);
            };
            let mut newly = None;
            while let Some(ev) = g.next_event() {
                match ev.event {
                    EventType::Disconnected if Some(ev.id) == self.active => {
                        self.active = None;
                        self.name = None;
                    }
                    EventType::Connected | EventType::ButtonPressed(..) | EventType::AxisChanged(..)
                        if self.active.is_none() =>
                    {
                        self.active = Some(ev.id);
                    }
                    // The last controller a button was pressed on drives the game.
                    EventType::ButtonPressed(..) => self.active = Some(ev.id),
                    _ => {}
                }
            }
            if self.active.is_none() {
                self.active = g.gamepads().next().map(|(id, _)| id);
            }
            let Some(id) = self.active else {
                return (None, None);
            };
            let gp = g.gamepad(id);
            if !gp.is_connected() {
                self.active = None;
                return (None, None);
            }
            if self.name.is_none() {
                let n = gp.name().to_owned();
                self.name = Some(n.clone());
                newly = Some(n);
            }
            let mut s = GamepadState::default();
            s.axes.left_x = gp.value(Axis::LeftStickX);
            s.axes.left_y = gp.value(Axis::LeftStickY);
            s.axes.right_x = gp.value(Axis::RightStickX);
            s.axes.right_y = gp.value(Axis::RightStickY);
            let trig = |b: Button| gp.button_data(b).map_or(0.0, |d| d.value());
            s.axes.left_trigger = trig(Button::LeftTrigger2);
            s.axes.right_trigger = trig(Button::RightTrigger2);
            for (b, p) in [
                (Button::South, PadButton::South),
                (Button::East, PadButton::East),
                (Button::West, PadButton::West),
                (Button::North, PadButton::North),
                (Button::LeftTrigger, PadButton::LeftShoulder),
                (Button::RightTrigger, PadButton::RightShoulder),
                (Button::Select, PadButton::Back),
                (Button::Start, PadButton::Start),
                (Button::LeftThumb, PadButton::LeftStick),
                (Button::RightThumb, PadButton::RightStick),
                (Button::DPadUp, PadButton::DPadUp),
                (Button::DPadDown, PadButton::DPadDown),
                (Button::DPadLeft, PadButton::DPadLeft),
                (Button::DPadRight, PadButton::DPadRight),
            ] {
                s.set(p, gp.is_pressed(b));
            }
            (Some(s), newly)
        }
        #[cfg(not(feature = "gamepad"))]
        {
            (None, None)
        }
    }
}

impl Default for PadBackend {
    fn default() -> Self {
        Self::new()
    }
}
