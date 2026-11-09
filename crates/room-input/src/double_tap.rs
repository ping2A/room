//! Double-tap binds (`doublebind`, UZDoom `DoubleTapControl`).
//!
//! A key pressed twice within the double-click threshold (`m_doubleclicktime`,
//! default 250 ms) fires its double binding on the second press, as well as
//! its normal binding.

use std::collections::BTreeMap;

/// Default threshold (ms), as GZDoom's `doubleclicktime`.
pub const DEFAULT_DOUBLE_TAP_MS: u64 = 250;

/// Tracks recent presses per key name.
#[derive(Clone, Debug)]
pub struct DoubleTapDetector {
    /// Max gap between the two presses.
    pub threshold_ms: u64,
    last_press: BTreeMap<String, u64>,
}

impl Default for DoubleTapDetector {
    fn default() -> Self {
        Self::new(DEFAULT_DOUBLE_TAP_MS)
    }
}

impl DoubleTapDetector {
    /// New detector with `threshold_ms`.
    #[must_use]
    pub fn new(threshold_ms: u64) -> Self {
        Self {
            threshold_ms,
            last_press: BTreeMap::new(),
        }
    }

    /// Feed a key-down of `key` at `now_ms`; `true` when it completes a
    /// double tap (the pair is consumed, so a third press starts over).
    pub fn press(&mut self, key: &str, now_ms: u64) -> bool {
        let k = key.to_ascii_lowercase();
        match self.last_press.get(&k) {
            Some(&t) if now_ms.saturating_sub(t) <= self.threshold_ms => {
                self.last_press.remove(&k);
                true
            }
            _ => {
                self.last_press.insert(k, now_ms);
                false
            }
        }
    }
}

/// `doublebind <key> <command>` table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DoubleBindings {
    map: BTreeMap<String, String>,
}

impl DoubleBindings {
    /// Set (or with an empty command, clear) the double binding of `key`.
    pub fn bind(&mut self, key: &str, command: &str) {
        let k = key.to_ascii_lowercase();
        if command.trim().is_empty() {
            self.map.remove(&k);
        } else {
            self.map.insert(k, command.trim().to_owned());
        }
    }

    /// Command bound to a double tap of `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(&key.to_ascii_lowercase()).map(String::as_str)
    }

    /// All `(key, command)` pairs, for `doublebinds` / config saves.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_press_inside_the_window_is_a_double_tap() {
        let mut d = DoubleTapDetector::new(250);
        assert!(!d.press("w", 1000));
        assert!(d.press("W", 1200));
        assert!(!d.press("w", 1300), "pair consumed");
        assert!(!d.press("w", 1700), "too slow");
        assert!(!d.press("a", 1750), "other keys are independent");
    }

    #[test]
    fn doublebind_table() {
        let mut b = DoubleBindings::default();
        b.bind("W", "+speed");
        assert_eq!(b.get("w"), Some("+speed"));
        b.bind("w", "");
        assert_eq!(b.get("w"), None);
    }
}
