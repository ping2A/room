//! Weapon impulse buttons for BD4 (`+reload` / `+zoom` / `+user1`–`+user4`).
//!
//! Bits match [`room_sim::Buttons`] 10–15 so KEYCONF `defaultbind` commands
//! flow into the same `TicCmd.buttons` field as keyboard sampling.

use crate::{Key, TicCmdBits};

/// Held impulse bits from KEYCONF `+reload` / console (and optional [`Key`]s).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponImpulseState(pub u32);

impl WeaponImpulseState {
    /// `+reload`.
    pub const RELOAD: u32 = 1 << 10;
    /// `+zoom`.
    pub const ZOOM: u32 = 1 << 11;
    /// `+user1`.
    pub const USER1: u32 = 1 << 12;
    /// `+user2`.
    pub const USER2: u32 = 1 << 13;
    /// `+user3`.
    pub const USER3: u32 = 1 << 14;
    /// `+user4`.
    pub const USER4: u32 = 1 << 15;

    /// All impulse bits (for clearing / masking).
    pub const MASK: u32 =
        Self::RELOAD | Self::ZOOM | Self::USER1 | Self::USER2 | Self::USER3 | Self::USER4;

    /// Apply `+reload` / `-zoom` / … Returns `true` if recognized.
    pub fn apply_cmd(&mut self, cmd: &str) -> bool {
        let cmd = cmd.trim();
        if let Some(name) = cmd.strip_prefix('+') {
            if let Some(bit) = bit_for_name(name) {
                self.0 |= bit;
                return true;
            }
        } else if let Some(name) = cmd.strip_prefix('-') {
            if let Some(bit) = bit_for_name(name) {
                self.0 &= !bit;
                return true;
            }
        }
        false
    }

    /// OR bits for held [`Key::Reload`] … in the sample.
    #[must_use]
    pub fn merge_keys(self, keys: &[Key]) -> u32 {
        let mut m = self.0;
        for k in keys {
            if let Some(b) = key_bit(*k) {
                m |= b;
            }
        }
        m
    }
}

fn bit_for_name(name: &str) -> Option<u32> {
    match name.to_ascii_lowercase().as_str() {
        "reload" => Some(WeaponImpulseState::RELOAD),
        "zoom" => Some(WeaponImpulseState::ZOOM),
        "user1" => Some(WeaponImpulseState::USER1),
        "user2" => Some(WeaponImpulseState::USER2),
        "user3" => Some(WeaponImpulseState::USER3),
        "user4" => Some(WeaponImpulseState::USER4),
        _ => None,
    }
}

fn key_bit(key: Key) -> Option<u32> {
    match key {
        Key::Reload => Some(WeaponImpulseState::RELOAD),
        Key::Zoom => Some(WeaponImpulseState::ZOOM),
        Key::User1 => Some(WeaponImpulseState::USER1),
        Key::User2 => Some(WeaponImpulseState::USER2),
        Key::User3 => Some(WeaponImpulseState::USER3),
        Key::User4 => Some(WeaponImpulseState::USER4),
        _ => None,
    }
}

/// Attach weapon-impulse bits to a built tic command.
pub fn apply_weapon_impulses(cmd: &mut TicCmdBits, impulses: WeaponImpulseState, keys: &[Key]) {
    cmd.buttons |= impulses.merge_keys(keys);
}

/// BD-oriented default binds (shells / KEYCONF may override; Q stays turn on desktop).
pub fn bd_impulse_bind_pairs() -> [(&'static str, &'static str); 5] {
    [
        ("r", "+reload"),
        ("g", "+user1"),
        ("x", "+user3"),
        ("z", "+zoom"),
        ("v", "+user2"),
    ]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{InputSample, build_ticcmd};

    #[test]
    fn plus_minus_impulse_cmds() {
        let mut s = WeaponImpulseState::default();
        assert!(s.apply_cmd("+reload"));
        assert_eq!(s.0, WeaponImpulseState::RELOAD);
        assert!(s.apply_cmd("+user1"));
        assert_eq!(s.0, WeaponImpulseState::RELOAD | WeaponImpulseState::USER1);
        assert!(s.apply_cmd("-reload"));
        assert_eq!(s.0, WeaponImpulseState::USER1);
        assert!(!s.apply_cmd("+attack"));
    }

    #[test]
    fn impulse_bits_on_ticcmd() {
        let mut s = WeaponImpulseState::default();
        s.apply_cmd("+zoom");
        let sample = InputSample {
            keys: vec![Key::Reload],
            ..InputSample::default()
        };
        let mut bits = build_ticcmd(&sample);
        apply_weapon_impulses(&mut bits, s, &sample.keys);
        assert_eq!(
            bits.buttons & WeaponImpulseState::RELOAD,
            WeaponImpulseState::RELOAD
        );
        assert_eq!(
            bits.buttons & WeaponImpulseState::ZOOM,
            WeaponImpulseState::ZOOM
        );
    }
}
