//! Keyboard / mouse input that must not borrow the frame `RefCell`.
//!
//! The rAF draw loop holds `FrameState` for the whole frame. Key events that
//! try to `borrow_mut` that cell (or `try_borrow_mut` and give up) drop WASD.
//! This hub is a separate cell so input always lands.

use crate::play::PlaySession;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

thread_local! {
    static HELD: RefCell<BTreeSet<&'static str>> = const { RefCell::new(BTreeSet::new()) };
    static ATTACK: Cell<bool> = const { Cell::new(false) };
    static WEAPON: Cell<u8> = const { Cell::new(0) };
    /// Queued weapon cycles (positive = next). Fractional notches live in `WHEEL_FRAC`.
    static WHEEL: Cell<i32> = const { Cell::new(0) };
    static WHEEL_FRAC: Cell<f64> = const { Cell::new(0.0) };
    static VIEW_GRID: Cell<bool> = const { Cell::new(false) };
    static MOUSE_YAW: Cell<i32> = const { Cell::new(0) };
    static MOUSE_PITCH: Cell<i32> = const { Cell::new(0) };
}

/// Map `KeyboardEvent.code` into the play session token set.
pub fn set_key(code: &str, down: bool) {
    let token: Option<&'static str> = match code {
        "KeyW" | "ArrowUp" => Some("w"),
        "KeyS" | "ArrowDown" => Some("s"),
        "KeyA" => Some("a"),
        "KeyD" => Some("d"),
        "ArrowLeft" => Some("left"),
        "ArrowRight" => Some("right"),
        "ShiftLeft" | "ShiftRight" => Some("shift"),
        "KeyE" | "Space" => Some("use"),
        "Enter" | "NumpadEnter" => Some("enter"),
        "ControlLeft" | "ControlRight" => Some("attack"),
        "Tab" => Some("scores"),
        "KeyQ" => Some("up"),
        "KeyZ" | "KeyC" => Some("down"),
        "Slash" => Some("kick"),
        "Quote" => Some("invuse"),
        "KeyG" | "Digit0" | "Numpad0" => {
            if down {
                VIEW_GRID.set(true);
            }
            None
        }
        "Digit1" | "Numpad1" => {
            if down {
                WEAPON.set(1);
            }
            None
        }
        "Digit2" | "Numpad2" => {
            if down {
                WEAPON.set(2);
            }
            None
        }
        "Digit3" | "Numpad3" => {
            if down {
                WEAPON.set(3);
            }
            None
        }
        "Digit4" | "Numpad4" => {
            if down {
                WEAPON.set(4);
            }
            None
        }
        "Digit5" | "Numpad5" => {
            if down {
                WEAPON.set(5);
            }
            None
        }
        "Digit6" | "Numpad6" => {
            if down {
                WEAPON.set(6);
            }
            None
        }
        "Digit7" | "Numpad7" => {
            if down {
                WEAPON.set(7);
            }
            None
        }
        "Digit8" | "Numpad8" => {
            if down {
                WEAPON.set(8);
            }
            None
        }
        "Digit9" | "Numpad9" => {
            if down {
                WEAPON.set(9);
            }
            None
        }
        _ => None,
    };
    if let Some(t) = token {
        HELD.with(|h| {
            if down {
                h.borrow_mut().insert(t);
            } else {
                h.borrow_mut().remove(t);
            }
        });
        if t == "attack" {
            ATTACK.set(down || HELD.with(|h| h.borrow().contains("attack")));
        }
    }
}

/// Mouse button (0 = primary = attack).
pub fn set_mouse_button(button: i16, down: bool) {
    if button == 0 {
        ATTACK.set(down || HELD.with(|h| h.borrow().contains("attack")));
    }
}

/// Mouse wheel while looking. `delta_y` is a `WheelEvent` delta: positive is
/// scrolling down (previous weapon), matching GZDoom's `mwheelup` = next.
pub fn add_weapon_wheel(delta_y: f64) {
    let acc = WHEEL_FRAC.get() - delta_y;
    #[allow(clippy::cast_possible_truncation)]
    let steps = acc.trunc() as i32;
    WHEEL_FRAC.set(acc - f64::from(steps));
    if steps != 0 {
        WHEEL.set(WHEEL.get().saturating_add(steps));
    }
}

/// Pointer-lock mouse motion (CSS pixels), same sign as [`PlaySession::add_mouse_delta`].
pub fn add_mouse_delta(dx: f64, dy: f64) {
    MOUSE_YAW.set(MOUSE_YAW.get().saturating_add((-dx).round() as i32));
    MOUSE_PITCH.set(MOUSE_PITCH.get().saturating_add((-dy).round() as i32));
}

/// Copy hub state into the play session (call once per frame before ticking).
pub fn drain_into(play: &mut PlaySession) {
    HELD.with(|h| {
        play.held.clone_from(&h.borrow());
    });
    play.attack = ATTACK.get();
    let w = WEAPON.get();
    if w != 0 {
        play.weapon = w;
        WEAPON.set(0);
    }
    if VIEW_GRID.replace(false) {
        play.view_grid_toggle = true;
    }
    let wheel = WHEEL.replace(0);
    if wheel != 0 {
        play.weapon_cycle = play.weapon_cycle.saturating_add(wheel);
    }
    play.mouse_yaw = play.mouse_yaw.saturating_add(MOUSE_YAW.replace(0));
    play.mouse_pitch = play.mouse_pitch.saturating_add(MOUSE_PITCH.replace(0));
}

/// Clear everything (leaving play / soft reset).
#[allow(dead_code)]
pub fn reset() {
    HELD.with(|h| h.borrow_mut().clear());
    ATTACK.set(false);
    WEAPON.set(0);
    WHEEL.set(0);
    WHEEL_FRAC.set(0.0);
    VIEW_GRID.set(false);
    MOUSE_YAW.set(0);
    MOUSE_PITCH.set(0);
}

/// Comma-separated held tokens for debug.
#[must_use]
pub fn debug_held() -> String {
    HELD.with(|h| {
        let v: Vec<&str> = h.borrow().iter().copied().collect();
        if v.is_empty() {
            "-".into()
        } else {
            v.join(",")
        }
    })
}
