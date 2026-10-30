//! Bindable mod buttons (`docs/28` §9 BE4): bits 16–19 of `TicCmd.buttons`.

use crate::{Key, TicCmdBits};

/// Bit mask for `+mod1` … `+mod4` (matches `room_sim::Buttons::MOD*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModButtonState(pub u32);

impl ModButtonState {
    /// `+mod1`.
    pub const MOD1: u32 = 1 << 16;
    /// `+mod2`.
    pub const MOD2: u32 = 1 << 17;
    /// `+mod3`.
    pub const MOD3: u32 = 1 << 18;
    /// `+mod4`.
    pub const MOD4: u32 = 1 << 19;

    /// Apply a console action (`+mod1`, `-mod2`, …). Returns `true` if recognized.
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

    /// OR bits for held [`Key::Mod1`] … [`Key::Mod4`] in the sample.
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

/// Default `bind` suggestions (shells may load these; desktop keeps Q as turn).
pub fn default_bind_pairs() -> [(&'static str, &'static str); 2] {
    [("g", "+mod1"), ("m", "+mod2")]
}

fn bit_for_name(name: &str) -> Option<u32> {
    match name.to_ascii_lowercase().as_str() {
        "mod1" => Some(ModButtonState::MOD1),
        "mod2" => Some(ModButtonState::MOD2),
        "mod3" => Some(ModButtonState::MOD3),
        "mod4" => Some(ModButtonState::MOD4),
        _ => None,
    }
}

fn key_bit(key: Key) -> Option<u32> {
    match key {
        Key::Mod1 => Some(ModButtonState::MOD1),
        Key::Mod2 => Some(ModButtonState::MOD2),
        Key::Mod3 => Some(ModButtonState::MOD3),
        Key::Mod4 => Some(ModButtonState::MOD4),
        _ => None,
    }
}

/// Attach mod-button bits to a built tic command.
pub fn apply_mod_buttons(cmd: &mut TicCmdBits, mods: ModButtonState, keys: &[Key]) {
    cmd.buttons |= mods.merge_keys(keys);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{InputSample, build_ticcmd};

    #[test]
    fn plus_minus_mod_cmds() {
        let mut m = ModButtonState::default();
        assert!(m.apply_cmd("+mod1"));
        assert_eq!(m.0, ModButtonState::MOD1);
        assert!(m.apply_cmd("+mod2"));
        assert_eq!(m.0, ModButtonState::MOD1 | ModButtonState::MOD2);
        assert!(m.apply_cmd("-mod1"));
        assert_eq!(m.0, ModButtonState::MOD2);
    }

    #[test]
    fn mod_bits_on_ticcmd() {
        let mut m = ModButtonState::default();
        m.apply_cmd("+mod1");
        let sample = InputSample {
            keys: vec![Key::Forward],
            ..InputSample::default()
        };
        let mut bits = build_ticcmd(&sample);
        apply_mod_buttons(&mut bits, m, &sample.keys);
        assert!(bits.forward_move > 0);
        assert_eq!(bits.buttons & ModButtonState::MOD1, ModButtonState::MOD1);
    }
}
