//! Game controller → tic command (UZDoom joystick menu subset).
//!
//! Device-independent: the shell polls a backend (gilrs on desktop, the
//! Gamepad API on the web) into a [`GamepadState`], then
//! [`apply_gamepad`] adds analog movement / look and the bound buttons.
//! Sticks use a radial dead zone and a power response curve.

use crate::{Key, TicCmdBits};

/// Analog axes, `-1.0..=1.0` (triggers `0.0..=1.0`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GamepadAxes {
    /// Left stick X (right +).
    pub left_x: f32,
    /// Left stick Y (up +).
    pub left_y: f32,
    /// Right stick X (right +).
    pub right_x: f32,
    /// Right stick Y (up +).
    pub right_y: f32,
    /// Left trigger.
    pub left_trigger: f32,
    /// Right trigger.
    pub right_trigger: f32,
}

/// Digital controller buttons (SDL / gilrs naming).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum PadButton {
    /// Bottom face button (A / Cross).
    South,
    /// Right face button (B / Circle).
    East,
    /// Left face button (X / Square).
    West,
    /// Top face button (Y / Triangle).
    North,
    /// Left shoulder.
    LeftShoulder,
    /// Right shoulder.
    RightShoulder,
    /// Back / Select / View.
    Back,
    /// Start / Menu.
    Start,
    /// Left stick click.
    LeftStick,
    /// Right stick click.
    RightStick,
    /// D-pad up.
    DPadUp,
    /// D-pad down.
    DPadDown,
    /// D-pad left.
    DPadLeft,
    /// D-pad right.
    DPadRight,
    /// Left trigger past [`GamepadConfig::trigger_threshold`] (as a button).
    LeftTrigger,
    /// Right trigger past the threshold.
    RightTrigger,
    /// Extra paddles (Elite / DualSense Edge).
    Paddle1,
    /// Paddle 2.
    Paddle2,
    /// Paddle 3.
    Paddle3,
    /// Paddle 4.
    Paddle4,
}

impl PadButton {
    /// Every button, in bit order.
    pub const ALL: [Self; 20] = [
        Self::South,
        Self::East,
        Self::West,
        Self::North,
        Self::LeftShoulder,
        Self::RightShoulder,
        Self::Back,
        Self::Start,
        Self::LeftStick,
        Self::RightStick,
        Self::DPadUp,
        Self::DPadDown,
        Self::DPadLeft,
        Self::DPadRight,
        Self::LeftTrigger,
        Self::RightTrigger,
        Self::Paddle1,
        Self::Paddle2,
        Self::Paddle3,
        Self::Paddle4,
    ];

    /// Bind name, as in UZDoom's key list (`pad_a`, `lshoulder`, …).
    #[must_use]
    pub fn bind_name(self) -> &'static str {
        match self {
            Self::South => "pad_a",
            Self::East => "pad_b",
            Self::West => "pad_x",
            Self::North => "pad_y",
            Self::LeftShoulder => "lshoulder",
            Self::RightShoulder => "rshoulder",
            Self::Back => "pad_back",
            Self::Start => "pad_start",
            Self::LeftStick => "lthumb",
            Self::RightStick => "rthumb",
            Self::DPadUp => "dpadup",
            Self::DPadDown => "dpaddown",
            Self::DPadLeft => "dpadleft",
            Self::DPadRight => "dpadright",
            Self::LeftTrigger => "ltrigger",
            Self::RightTrigger => "rtrigger",
            Self::Paddle1 => "paddle1",
            Self::Paddle2 => "paddle2",
            Self::Paddle3 => "paddle3",
            Self::Paddle4 => "paddle4",
        }
    }

    /// Parse a bind name (case-insensitive).
    #[must_use]
    pub fn from_bind_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|b| b.bind_name().eq_ignore_ascii_case(name))
    }

    fn bit(self) -> u32 {
        1 << (self as u8)
    }
}

/// One controller's sampled state for this tic.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GamepadState {
    /// Sticks and triggers.
    pub axes: GamepadAxes,
    /// Held [`PadButton`] bits.
    pub buttons: u32,
}

impl GamepadState {
    /// Mark `b` held or released.
    pub fn set(&mut self, b: PadButton, down: bool) {
        if down {
            self.buttons |= b.bit();
        } else {
            self.buttons &= !b.bit();
        }
    }

    /// Whether `b` is held.
    #[must_use]
    pub fn held(&self, b: PadButton) -> bool {
        self.buttons & b.bit() != 0
    }
}

/// Joystick options (`joy_*` CVars).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GamepadConfig {
    /// Stick dead zone, `0..1` (`joy_deadzone`).
    pub deadzone: f32,
    /// Response curve exponent (1 = linear, 2 = quadratic) (`joy_curve`).
    pub curve: f32,
    /// Look sensitivity multiplier (`joy_sensitivity`).
    pub look_sensitivity: f32,
    /// Movement multiplier (`joy_speedmultiplier`).
    pub move_sensitivity: f32,
    /// Invert vertical look (`joy_invertpitch`).
    pub invert_pitch: bool,
    /// Trigger travel that counts as a press.
    pub trigger_threshold: f32,
}

impl Default for GamepadConfig {
    fn default() -> Self {
        Self {
            deadzone: 0.2,
            curve: 1.6,
            look_sensitivity: 1.0,
            move_sensitivity: 1.0,
            invert_pitch: false,
            trigger_threshold: 0.35,
        }
    }
}

/// Radial dead zone + response curve on one stick. Returns the shaped vector.
#[must_use]
pub fn shape_stick(x: f32, y: f32, cfg: &GamepadConfig) -> (f32, f32) {
    let mag = (x * x + y * y).sqrt();
    let dz = cfg.deadzone.clamp(0.0, 0.95);
    if mag <= dz || mag <= f32::EPSILON {
        return (0.0, 0.0);
    }
    let norm = ((mag.min(1.0) - dz) / (1.0 - dz)).clamp(0.0, 1.0);
    let shaped = norm.powf(cfg.curve.max(0.1));
    (x / mag * shaped, y / mag * shaped)
}

/// Full-tilt stick turn speed in `TicCmd` yaw units (about Doom's fast turn).
const PAD_TURN: f32 = 1280.0;
/// Full-tilt stick pitch speed.
const PAD_PITCH: f32 = 900.0;
/// Full-tilt forward / side (running speeds, as `build_ticcmd`).
const PAD_FORWARD: f32 = 50.0 * 256.0;
const PAD_SIDE: f32 = 40.0 * 256.0;

/// Default UZDoom-style controller layout: button → game action.
#[must_use]
pub fn default_pad_binds() -> Vec<(PadButton, Key)> {
    vec![
        (PadButton::RightTrigger, Key::Attack),
        (PadButton::LeftTrigger, Key::AltAttack),
        (PadButton::South, Key::Jump),
        (PadButton::West, Key::Use),
        (PadButton::East, Key::MoveDown),
        (PadButton::North, Key::Reload),
        (PadButton::RightShoulder, Key::NextWeapon),
        (PadButton::LeftShoulder, Key::PrevWeapon),
        (PadButton::LeftStick, Key::Speed),
        (PadButton::RightStick, Key::Zoom),
        (PadButton::DPadUp, Key::InvUse),
    ]
}

/// Keys held through the controller this tic (triggers count past the threshold).
#[must_use]
pub fn gamepad_keys(
    pad: &GamepadState,
    binds: &[(PadButton, Key)],
    cfg: &GamepadConfig,
) -> Vec<Key> {
    let mut held = *pad;
    held.set(
        PadButton::LeftTrigger,
        pad.held(PadButton::LeftTrigger) || pad.axes.left_trigger >= cfg.trigger_threshold,
    );
    held.set(
        PadButton::RightTrigger,
        pad.held(PadButton::RightTrigger) || pad.axes.right_trigger >= cfg.trigger_threshold,
    );
    binds
        .iter()
        .filter(|(b, _)| held.held(*b))
        .map(|(_, k)| *k)
        .collect()
}

/// Add analog movement (left stick) and look (right stick) to `cmd`.
pub fn apply_gamepad(cmd: &mut TicCmdBits, pad: &GamepadState, cfg: &GamepadConfig) {
    let (mx, my) = shape_stick(pad.axes.left_x, pad.axes.left_y, cfg);
    let (lx, ly) = shape_stick(pad.axes.right_x, pad.axes.right_y, cfg);
    let ms = cfg.move_sensitivity.max(0.0);
    let add = |v: i16, d: f32| -> i16 {
        (f32::from(v) + d).clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
    };
    cmd.forward_move = add(cmd.forward_move, my * PAD_FORWARD * ms);
    cmd.side_move = add(cmd.side_move, mx * PAD_SIDE * ms);
    // Stick right turns clockwise (negative angleturn).
    cmd.yaw = add(cmd.yaw, -lx * PAD_TURN * cfg.look_sensitivity);
    let pitch_sign = if cfg.invert_pitch { -1.0 } else { 1.0 };
    cmd.pitch = add(
        cmd.pitch,
        ly * PAD_PITCH * cfg.look_sensitivity * pitch_sign,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_zone_and_curve_shape_the_stick() {
        let cfg = GamepadConfig::default();
        assert_eq!(shape_stick(0.1, 0.1, &cfg), (0.0, 0.0));
        let (x, y) = shape_stick(1.0, 0.0, &cfg);
        assert!((x - 1.0).abs() < 1e-6 && y == 0.0);
        let (half, _) = shape_stick(0.6, 0.0, &cfg);
        assert!(half > 0.0 && half < 0.5, "curve softens mid travel: {half}");
    }

    #[test]
    fn sticks_move_and_turn() {
        let cfg = GamepadConfig::default();
        let mut pad = GamepadState::default();
        pad.axes.left_y = 1.0;
        pad.axes.right_x = 1.0;
        let mut cmd = TicCmdBits::default();
        apply_gamepad(&mut cmd, &pad, &cfg);
        assert_eq!(cmd.forward_move, 50 * 256);
        assert!(cmd.yaw < 0, "stick right turns clockwise");
        assert_eq!(cmd.side_move, 0);
    }

    #[test]
    fn triggers_and_buttons_map_to_keys() {
        let cfg = GamepadConfig::default();
        let mut pad = GamepadState::default();
        pad.axes.right_trigger = 0.9;
        pad.set(PadButton::West, true);
        let keys = gamepad_keys(&pad, &default_pad_binds(), &cfg);
        assert!(keys.contains(&Key::Attack));
        assert!(keys.contains(&Key::Use));
        assert!(!keys.contains(&Key::AltAttack));
        assert_eq!(PadButton::from_bind_name("PAD_A"), Some(PadButton::South));
    }
}
