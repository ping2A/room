//! Attack button latch for native shells.
//!
//! macOS Locked cursor delivers **both** `WindowEvent::MouseInput` and
//! `DeviceEvent::Button`, often out of order, and right-button ids must never
//! clear left. Each source is tracked separately so a release on one path
//! cannot cancel a press still held on another.

/// How many playsim tics to keep `+attack` after a short click.
///
/// Must be long enough that a 1-frame press still reaches `A_WeaponReady` →
/// Fire (macOS Locked cursor often drops the button before the next tic), but
/// **short enough that `A_ReFire` does not see a held attack**. Pistol reaches
/// `A_ReFire` after 14 tics; plasma after 3 — so values ≥4 make a tap fire
/// plasma twice / pistol twice. `3` covers grab-drop without auto-refire.
pub const ATTACK_HOLD_TICS: u8 = 3;

/// Platform used to interpret raw `DeviceEvent::Button` ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceButtonOs {
    /// NSEvent `buttonNumber`: 0 left, 1 right.
    Macos,
    /// Raw Input indices: 0 left (`RI_MOUSE_BUTTON_1`), 1 right.
    Windows,
    /// X11 button numbers: 1 left, 2 middle, 3 right. Linux `BTN_LEFT` = 0x110.
    Linux,
    /// Unknown / web: 0 = left (DOM `button`).
    Other,
}

impl DeviceButtonOs {
    /// Compile-target platform.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

/// True when `DeviceEvent::Button { button, .. }` is the primary / left button.
#[must_use]
pub fn device_button_is_primary(button: u32) -> bool {
    device_button_is_primary_on(DeviceButtonOs::current(), button)
}

/// [`device_button_is_primary`] for an explicit OS (tests).
#[must_use]
pub fn device_button_is_primary_on(os: DeviceButtonOs, button: u32) -> bool {
    match os {
        DeviceButtonOs::Macos | DeviceButtonOs::Windows | DeviceButtonOs::Other => button == 0,
        // X11 detail 1 = left. HID `BTN_LEFT` (0x110) if a backend forwards it.
        DeviceButtonOs::Linux => button == 1 || button == 0x110,
    }
}

/// Independent attack sources + a short post-click hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttackLatch {
    /// `WindowEvent::MouseInput { button: Left }`.
    window_left: bool,
    /// `DeviceEvent::Button` mapped as primary.
    device_left: bool,
    /// Ctrl / F (or other `+attack` keys).
    key: bool,
    /// Remaining tics after a press (short click / grab drop).
    hold_tics: u8,
}

impl AttackLatch {
    /// Start / extend the post-click hold.
    fn bump_hold(&mut self) {
        self.hold_tics = self.hold_tics.max(ATTACK_HOLD_TICS);
    }

    /// Window-path left button.
    pub fn set_window_left(&mut self, down: bool) {
        self.window_left = down;
        if down {
            self.bump_hold();
        }
    }

    /// Device-path left button. Never call this for right/middle ids.
    pub fn set_device_left(&mut self, down: bool) {
        self.device_left = down;
        if down {
            self.bump_hold();
        }
    }

    /// Keyboard `+attack`.
    pub fn set_key(&mut self, down: bool) {
        self.key = down;
        if down {
            self.bump_hold();
        }
    }

    /// Drop every source (menu, unfocus, end game).
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// True while any source is down or the hold is counting.
    #[must_use]
    pub fn active(&self) -> bool {
        self.window_left || self.device_left || self.key || self.hold_tics > 0
    }

    /// Consume one tic of hold (once per playsim tic when sampling).
    pub fn tick(&mut self) {
        if self.hold_tics > 0 {
            self.hold_tics -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_primary_is_zero_not_one() {
        assert!(device_button_is_primary_on(DeviceButtonOs::Macos, 0));
        assert!(
            !device_button_is_primary_on(DeviceButtonOs::Macos, 1),
            "macOS button 1 is right — must not drive attack"
        );
        assert!(!device_button_is_primary_on(DeviceButtonOs::Macos, 2));
    }

    #[test]
    fn windows_primary_is_zero_not_one() {
        assert!(device_button_is_primary_on(DeviceButtonOs::Windows, 0));
        assert!(!device_button_is_primary_on(DeviceButtonOs::Windows, 1));
    }

    #[test]
    fn linux_primary_is_x11_one() {
        assert!(device_button_is_primary_on(DeviceButtonOs::Linux, 1));
        assert!(!device_button_is_primary_on(DeviceButtonOs::Linux, 0));
        assert!(!device_button_is_primary_on(DeviceButtonOs::Linux, 3));
        assert!(device_button_is_primary_on(DeviceButtonOs::Linux, 0x110));
    }

    #[test]
    fn current_os_matches_cfg() {
        assert_eq!(
            device_button_is_primary(0),
            device_button_is_primary_on(DeviceButtonOs::current(), 0)
        );
        assert_eq!(
            device_button_is_primary(1),
            device_button_is_primary_on(DeviceButtonOs::current(), 1)
        );
    }

    #[test]
    fn window_release_does_not_cancel_device_hold() {
        let mut a = AttackLatch::default();
        a.set_device_left(true);
        a.set_window_left(true);
        a.set_window_left(false);
        assert!(a.active());
        assert!(a.device_left);
        assert!(!a.window_left);
    }

    #[test]
    fn device_release_does_not_cancel_window_hold() {
        let mut a = AttackLatch::default();
        a.set_window_left(true);
        a.set_device_left(true);
        a.set_device_left(false);
        assert!(a.active());
        assert!(a.window_left);
    }

    #[test]
    fn right_device_id_never_reaches_latch() {
        let mut a = AttackLatch::default();
        a.set_window_left(true);
        if !device_button_is_primary_on(DeviceButtonOs::Macos, 1) {
            // shell must ignore — latch unchanged
        } else {
            a.set_device_left(false);
        }
        assert!(a.window_left);
    }

    #[test]
    fn short_click_keeps_attack_for_hold_tics() {
        let mut a = AttackLatch::default();
        a.set_window_left(true);
        a.set_window_left(false);
        assert!(a.active());
        for _ in 0..ATTACK_HOLD_TICS {
            assert!(a.active());
            a.tick();
        }
        assert!(!a.active());
    }

    #[test]
    fn dual_window_and_device_release_still_fires() {
        let mut a = AttackLatch::default();
        a.set_window_left(true);
        a.set_device_left(true);
        a.set_window_left(false);
        a.set_device_left(false);
        let mut fired = 0u32;
        for _ in 0..30 {
            if a.active() {
                fired += 1;
            }
            a.tick();
        }
        assert!(
            fired >= u32::from(ATTACK_HOLD_TICS),
            "expected ≥{} attack tics after dual release, got {fired}",
            ATTACK_HOLD_TICS
        );
    }

    #[test]
    fn hold_is_shorter_than_plasma_refire_window() {
        // Plasma: Fire A 3 tics then A_ReFire. Hold must end before that so a
        // tap is one bolt (see think_player weapon fire tests).
        assert!(
            ATTACK_HOLD_TICS <= 3,
            "ATTACK_HOLD_TICS={ATTACK_HOLD_TICS} would auto-refire plasma on a tap"
        );
    }

    #[test]
    fn key_and_mouse_are_independent() {
        let mut a = AttackLatch::default();
        a.set_key(true);
        a.set_window_left(true);
        a.set_window_left(false);
        assert!(a.active());
        a.set_key(false);
        assert!(a.active(), "hold should keep fire after both releases");
    }

    #[test]
    fn clear_drops_stuck_device_bit() {
        let mut a = AttackLatch::default();
        a.set_device_left(true);
        a.clear();
        assert!(!a.active());
    }
}
