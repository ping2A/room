//! Keyboard / mouse input that must not borrow the frame `RefCell`.
//!
//! The rAF draw loop holds `FrameState` for the whole frame. Key events that
//! try to `borrow_mut` that cell (or `try_borrow_mut` and give up) drop WASD.
//! This hub is a separate cell so input always lands.
//!
//! Bindings and mouse sensitivity are overridable from the web Settings menu
//! (persisted in `localStorage` by `settings.js`).

use crate::play::PlaySession;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

thread_local! {
    static HELD: RefCell<BTreeSet<&'static str>> = const { RefCell::new(BTreeSet::new()) };
    static ATTACK: Cell<bool> = const { Cell::new(false) };
    static ALT_ATTACK: Cell<bool> = const { Cell::new(false) };
    static WEAPON: Cell<u8> = const { Cell::new(0) };
    /// Queued weapon cycles (positive = next). Fractional notches live in `WHEEL_FRAC`.
    static WHEEL: Cell<i32> = const { Cell::new(0) };
    static WHEEL_FRAC: Cell<f64> = const { Cell::new(0.0) };
    static VIEW_GRID: Cell<bool> = const { Cell::new(false) };
    static MOUSE_YAW: Cell<i32> = const { Cell::new(0) };
    static MOUSE_PITCH: Cell<i32> = const { Cell::new(0) };
    /// Extra scale on pointer-lock look (1.0 = default).
    static MOUSE_SENS: Cell<f64> = const { Cell::new(1.0) };
    /// Flip look up/down when true.
    static MOUSE_INVERT_Y: Cell<bool> = const { Cell::new(false) };
    /// `KeyboardEvent.code` → logical action (`w`, `attack`, …). Empty = built-in defaults.
    static BINDS: RefCell<BTreeMap<String, String>> = const { RefCell::new(BTreeMap::new()) };
}

/// Default `KeyboardEvent.code` → action map (mirrors historical hard-coding).
pub fn default_binds() -> BTreeMap<String, String> {
    let pairs = [
        ("KeyW", "w"),
        ("ArrowUp", "w"),
        ("KeyS", "s"),
        ("ArrowDown", "s"),
        ("KeyA", "a"),
        ("KeyD", "d"),
        ("ArrowLeft", "left"),
        ("ArrowRight", "right"),
        ("ShiftLeft", "shift"),
        ("ShiftRight", "shift"),
        ("KeyE", "use"),
        ("Space", "use"),
        ("Enter", "enter"),
        ("NumpadEnter", "enter"),
        ("ControlLeft", "attack"),
        ("ControlRight", "attack"),
        ("Tab", "scores"),
        ("KeyQ", "up"),
        ("KeyZ", "down"),
        ("KeyC", "down"),
        ("Slash", "kick"),
        ("Quote", "invuse"),
        ("KeyB", "mod1"),
        ("KeyM", "mod2"),
        ("KeyV", "mod3"),
        ("KeyG", "grid"),
        ("Digit0", "grid"),
        ("Numpad0", "grid"),
        ("Digit1", "weapon1"),
        ("Numpad1", "weapon1"),
        ("Digit2", "weapon2"),
        ("Numpad2", "weapon2"),
        ("Digit3", "weapon3"),
        ("Numpad3", "weapon3"),
        ("Digit4", "weapon4"),
        ("Numpad4", "weapon4"),
        ("Digit5", "weapon5"),
        ("Numpad5", "weapon5"),
        ("Digit6", "weapon6"),
        ("Numpad6", "weapon6"),
        ("Digit7", "weapon7"),
        ("Numpad7", "weapon7"),
        ("Digit8", "weapon8"),
        ("Numpad8", "weapon8"),
        ("Digit9", "weapon9"),
        ("Numpad9", "weapon9"),
    ];
    pairs
        .into_iter()
        .map(|(c, a)| (c.to_owned(), a.to_owned()))
        .collect()
}

/// Replace custom bindings. Empty map restores built-in defaults.
pub fn set_binds(map: BTreeMap<String, String>) {
    BINDS.with(|b| *b.borrow_mut() = map);
}

/// Current effective binds (custom or defaults).
#[must_use]
pub fn effective_binds() -> BTreeMap<String, String> {
    BINDS.with(|b| {
        let custom = b.borrow();
        if custom.is_empty() {
            default_binds()
        } else {
            custom.clone()
        }
    })
}

/// Mouse look sensitivity (clamped).
pub fn set_mouse_sens(sens: f64) {
    MOUSE_SENS.set(sens.clamp(0.1, 5.0));
}

/// Invert vertical look.
pub fn set_mouse_invert_y(invert: bool) {
    MOUSE_INVERT_Y.set(invert);
}

fn intern_action(action: &str) -> Option<&'static str> {
    Some(match action {
        "w" => "w",
        "s" => "s",
        "a" => "a",
        "d" => "d",
        "left" => "left",
        "right" => "right",
        "shift" => "shift",
        "use" => "use",
        "enter" => "enter",
        "attack" => "attack",
        "scores" => "scores",
        "up" => "up",
        "down" => "down",
        "kick" => "kick",
        "invuse" => "invuse",
        "grid" => "grid",
        "weapon1" => "weapon1",
        "weapon2" => "weapon2",
        "weapon3" => "weapon3",
        "weapon4" => "weapon4",
        "weapon5" => "weapon5",
        "weapon6" => "weapon6",
        "weapon7" => "weapon7",
        "weapon8" => "weapon8",
        "weapon9" => "weapon9",
        "mod1" => "mod1",
        "mod2" => "mod2",
        "mod3" => "mod3",
        "mod4" => "mod4",
        _ => return None,
    })
}

fn resolve_action(code: &str) -> Option<&'static str> {
    BINDS.with(|b| {
        let custom = b.borrow();
        if custom.is_empty() {
            default_binds().get(code).and_then(|a| intern_action(a))
        } else {
            // Custom map is authoritative: unbound codes do nothing.
            custom.get(code).and_then(|a| intern_action(a))
        }
    })
}

/// Map `KeyboardEvent.code` into the play session token set.
pub fn set_key(code: &str, down: bool) {
    let Some(action) = resolve_action(code) else {
        return;
    };
    if action == "grid" {
        if down {
            VIEW_GRID.set(true);
        }
        return;
    }
    if let Some(rest) = action.strip_prefix("weapon") {
        if down {
            if let Ok(n) = rest.parse::<u8>() {
                WEAPON.set(n);
            }
        }
        return;
    }
    HELD.with(|h| {
        if down {
            h.borrow_mut().insert(action);
        } else {
            h.borrow_mut().remove(action);
        }
    });
    if action == "attack" {
        ATTACK.set(down || HELD.with(|h| h.borrow().contains("attack")));
    }
}

/// Mouse button (0 = primary = attack).
pub fn set_mouse_button(button: i16, down: bool) {
    if button == 0 {
        ATTACK.set(down || HELD.with(|h| h.borrow().contains("attack")));
    } else if button == 2 {
        ALT_ATTACK.set(down || HELD.with(|h| h.borrow().contains("altattack")));
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
    let s = MOUSE_SENS.get();
    let dy = if MOUSE_INVERT_Y.get() { -dy } else { dy };
    MOUSE_YAW.set(MOUSE_YAW.get().saturating_add((-dx * s).round() as i32));
    MOUSE_PITCH.set(MOUSE_PITCH.get().saturating_add((-dy * s).round() as i32));
}

/// Copy hub state into the play session (call once per frame before ticking).
pub fn drain_into(play: &mut PlaySession) {
    HELD.with(|h| {
        play.held.clone_from(&h.borrow());
    });
    play.attack = ATTACK.get();
    play.alt_attack = ALT_ATTACK.get();
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
    ALT_ATTACK.set(false);
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
