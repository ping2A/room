//! Device abstraction, bindings, and ticcmd building.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod attack;
mod mod_buttons;
mod weapon_impulses;

pub use attack::{
    ATTACK_HOLD_TICS, AttackLatch, DeviceButtonOs, device_button_is_primary,
    device_button_is_primary_on,
};
pub use mod_buttons::{ModButtonState, apply_mod_buttons, default_bind_pairs};
pub use weapon_impulses::{WeaponImpulseState, apply_weapon_impulses, bd_impulse_bind_pairs};

/// Logical keys the binder cares about (M5 subset).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// Forward.
    Forward,
    /// Back.
    Back,
    /// Strafe left.
    MoveLeft,
    /// Strafe right.
    MoveRight,
    /// Turn left.
    Left,
    /// Turn right.
    Right,
    /// Attack.
    Attack,
    /// Alt / secondary attack (`docs/31` MA4).
    AltAttack,
    /// Reload (`+reload` / BD4).
    Reload,
    /// Zoom (`+zoom` / BD4).
    Zoom,
    /// User1 (`+user1` / BD4).
    User1,
    /// User2 (`+user2` / BD4).
    User2,
    /// User3 (`+user3` / BD4).
    User3,
    /// User4 (`+user4` / BD4).
    User4,
    /// Use.
    Use,
    /// Run / speed.
    Speed,
    /// Jump.
    Jump,
    /// Weapon slot 1 (fist / chainsaw).
    Weapon1,
    /// Weapon slot 2 (pistol).
    Weapon2,
    /// Weapon slot 3 (shotgun / SSG).
    Weapon3,
    /// Weapon slot 4 (chaingun).
    Weapon4,
    /// Weapon slot 5 (rocket).
    Weapon5,
    /// Weapon slot 6 (plasma).
    Weapon6,
    /// Weapon slot 7 (BFG).
    Weapon7,
    /// Weapon slot 8.
    Weapon8,
    /// Weapon slot 9.
    Weapon9,
    /// Weapon slot 0 (sent as `weapon_select` 10).
    Weapon0,
    /// Swim / fly up (`+moveup`).
    MoveUp,
    /// Swim / fly down (`+movedown`).
    MoveDown,
    /// Duke quick kick (`+quickkick`).
    QuickKick,
    /// Use carried inventory (`+invuse`).
    InvUse,
    /// Cycle to next owned weapon.
    NextWeapon,
    /// Cycle to previous owned weapon.
    PrevWeapon,
    /// Mod button 1 (`+mod1`).
    Mod1,
    /// Mod button 2 (`+mod2`).
    Mod2,
    /// Mod button 3 (`+mod3`).
    Mod3,
    /// Mod button 4 (`+mod4`).
    Mod4,
}

/// Per-tic key / axis sample before binding resolution.
#[derive(Clone, Debug, Default)]
pub struct InputSample {
    /// Keys held this tic.
    pub keys: Vec<Key>,
    /// Mouse yaw delta (counts).
    pub mouse_yaw: i32,
    /// Mouse pitch delta.
    pub mouse_pitch: i32,
}

/// Built command fields (maps into sim `TicCmd` in `room-game`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TicCmdBits {
    /// `BT_*` style mask.
    pub buttons: u32,
    /// Pitch delta.
    pub pitch: i16,
    /// Yaw delta.
    pub yaw: i16,
    /// Roll delta.
    pub roll: i16,
    /// Forward move.
    pub forward_move: i16,
    /// Side move.
    pub side_move: i16,
    /// Vertical move.
    pub up_move: i16,
    /// Weapon slot `1..=9`, `10` for slot 0, or `0` if none this tic.
    pub weapon_select: u8,
}

impl TicCmdBits {
    /// Attack button.
    pub const BT_ATTACK: u32 = 1 << 0;
    /// Use button.
    pub const BT_USE: u32 = 1 << 1;
    /// Jump.
    pub const BT_JUMP: u32 = 1 << 2;
    /// Speed.
    pub const BT_SPEED: u32 = 1 << 3;
    /// Next weapon.
    pub const BT_WEAPONNEXT: u32 = 1 << 5;
    /// Prev weapon.
    pub const BT_WEAPONPREV: u32 = 1 << 6;
    /// Duke quick kick.
    pub const BT_QUICKKICK: u32 = 1 << 7;
    /// Use carried inventory (Duke medkit / steroids).
    pub const BT_INVUSE: u32 = 1 << 8;
    /// Alt / secondary attack.
    pub const BT_ALTATTACK: u32 = 1 << 9;
    /// Reload (`docs/33` BD4).
    pub const BT_RELOAD: u32 = 1 << 10;
    /// Zoom.
    pub const BT_ZOOM: u32 = 1 << 11;
    /// User1.
    pub const BT_USER1: u32 = 1 << 12;
    /// User2.
    pub const BT_USER2: u32 = 1 << 13;
    /// User3.
    pub const BT_USER3: u32 = 1 << 14;
    /// User4.
    pub const BT_USER4: u32 = 1 << 15;
}

/// Doom walk / run forward units.
const FORWARD_WALK: i16 = 25 * 256; // scaled toward classic 2048-space
const FORWARD_RUN: i16 = 50 * 256;
const SIDE_WALK: i16 = 24 * 256;
const SIDE_RUN: i16 = 40 * 256;
/// GZDoom `flyspeed` (walk, run).
const FLY_WALK: i16 = 256;
const FLY_RUN: i16 = 3 * 256;
/// Mouse counts → `TicCmd` yaw/pitch units.
///
/// Doom's DOS path was roughly `mousex * 8` then `<< 8` into the short ticcmd
/// (≈ `* 2048` before BAM). Modern high-DPI mice need a milder scale; `256`
/// yields ≈1.4° per count at sensitivity 1 — snappy without spinning.
pub const MOUSE_YAW_SCALE: i32 = 256;

/// Degrees of yaw/pitch per mouse count at sensitivity 1 (before `m_sensitivity`).
pub const MOUSE_DEG_PER_COUNT: f64 = MOUSE_YAW_SCALE as f64 * 360.0 / 65536.0;

/// Build a tic command from a sampled input state (`G_BuildTiccmd` subset).
#[must_use]
pub fn build_ticcmd(sample: &InputSample) -> TicCmdBits {
    let speed = sample.keys.contains(&Key::Speed);
    let mut cmd = TicCmdBits {
        yaw: (sample.mouse_yaw * MOUSE_YAW_SCALE).clamp(i32::from(i16::MIN), i32::from(i16::MAX))
            as i16,
        pitch: (sample.mouse_pitch * MOUSE_YAW_SCALE)
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        ..TicCmdBits::default()
    };
    let fwd = if speed { FORWARD_RUN } else { FORWARD_WALK };
    let side = if speed { SIDE_RUN } else { SIDE_WALK };
    if sample.keys.contains(&Key::Forward) {
        cmd.forward_move = cmd.forward_move.saturating_add(fwd);
    }
    if sample.keys.contains(&Key::Back) {
        cmd.forward_move = cmd.forward_move.saturating_sub(fwd);
    }
    if sample.keys.contains(&Key::MoveRight) {
        cmd.side_move = cmd.side_move.saturating_add(side);
    }
    if sample.keys.contains(&Key::MoveLeft) {
        cmd.side_move = cmd.side_move.saturating_sub(side);
    }
    let fly = if speed { FLY_RUN } else { FLY_WALK };
    if sample.keys.contains(&Key::MoveUp) {
        cmd.up_move = cmd.up_move.saturating_add(fly);
    }
    if sample.keys.contains(&Key::MoveDown) {
        cmd.up_move = cmd.up_move.saturating_sub(fly);
    }
    // Keyboard turning: Doom `angleturn` is added to BAM angle (CCW+).
    // Left → +turn (CCW), Right → −turn (CW).
    if sample.keys.contains(&Key::Left) {
        cmd.yaw = cmd.yaw.saturating_add(640);
    }
    if sample.keys.contains(&Key::Right) {
        cmd.yaw = cmd.yaw.saturating_sub(640);
    }
    if sample.keys.contains(&Key::Attack) {
        cmd.buttons |= TicCmdBits::BT_ATTACK;
    }
    if sample.keys.contains(&Key::AltAttack) {
        cmd.buttons |= TicCmdBits::BT_ALTATTACK;
    }
    if sample.keys.contains(&Key::Reload) {
        cmd.buttons |= TicCmdBits::BT_RELOAD;
    }
    if sample.keys.contains(&Key::Zoom) {
        cmd.buttons |= TicCmdBits::BT_ZOOM;
    }
    if sample.keys.contains(&Key::User1) {
        cmd.buttons |= TicCmdBits::BT_USER1;
    }
    if sample.keys.contains(&Key::User2) {
        cmd.buttons |= TicCmdBits::BT_USER2;
    }
    if sample.keys.contains(&Key::User3) {
        cmd.buttons |= TicCmdBits::BT_USER3;
    }
    if sample.keys.contains(&Key::User4) {
        cmd.buttons |= TicCmdBits::BT_USER4;
    }
    if sample.keys.contains(&Key::Use) {
        cmd.buttons |= TicCmdBits::BT_USE;
    }
    if sample.keys.contains(&Key::Jump) {
        cmd.buttons |= TicCmdBits::BT_JUMP;
    }
    if speed {
        cmd.buttons |= TicCmdBits::BT_SPEED;
    }
    for (key, slot) in [
        (Key::Weapon1, 1u8),
        (Key::Weapon2, 2),
        (Key::Weapon3, 3),
        (Key::Weapon4, 4),
        (Key::Weapon5, 5),
        (Key::Weapon6, 6),
        (Key::Weapon7, 7),
        (Key::Weapon8, 8),
        (Key::Weapon9, 9),
        (Key::Weapon0, 10),
    ] {
        if sample.keys.contains(&key) {
            cmd.weapon_select = slot;
            break;
        }
    }
    if sample.keys.contains(&Key::NextWeapon) {
        cmd.buttons |= TicCmdBits::BT_WEAPONNEXT;
    }
    if sample.keys.contains(&Key::PrevWeapon) {
        cmd.buttons |= TicCmdBits::BT_WEAPONPREV;
    }
    if sample.keys.contains(&Key::QuickKick) {
        cmd.buttons |= TicCmdBits::BT_QUICKKICK;
    }
    if sample.keys.contains(&Key::InvUse) {
        cmd.buttons |= TicCmdBits::BT_INVUSE;
    }
    apply_mod_buttons(&mut cmd, ModButtonState::default(), &sample.keys);
    cmd
}

/// Like [`build_ticcmd`] but merges held mod buttons from [`ModButtonState`].
#[must_use]
pub fn build_ticcmd_with_mods(sample: &InputSample, mods: ModButtonState) -> TicCmdBits {
    build_ticcmd_with_mods_and_impulses(sample, mods, WeaponImpulseState::default())
}

/// Like [`build_ticcmd_with_mods`] plus KEYCONF weapon impulses (`+reload`…).
#[must_use]
pub fn build_ticcmd_with_mods_and_impulses(
    sample: &InputSample,
    mods: ModButtonState,
    impulses: WeaponImpulseState,
) -> TicCmdBits {
    let mut cmd = build_ticcmd(sample);
    // `build_ticcmd` already merged keys; re-apply latched console bits.
    cmd.buttons &= !(ModButtonState::MOD1
        | ModButtonState::MOD2
        | ModButtonState::MOD3
        | ModButtonState::MOD4
        | WeaponImpulseState::MASK);
    apply_mod_buttons(&mut cmd, mods, &sample.keys);
    apply_weapon_impulses(&mut cmd, impulses, &sample.keys);
    cmd
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn forward_and_attack() {
        let sample = InputSample {
            keys: vec![Key::Forward, Key::Attack],
            mouse_yaw: 0,
            mouse_pitch: 0,
        };
        let cmd = build_ticcmd(&sample);
        assert!(cmd.forward_move > 0);
        assert_eq!(cmd.buttons & TicCmdBits::BT_ATTACK, TicCmdBits::BT_ATTACK);
    }

    #[test]
    fn weapon_slot_in_ticcmd() {
        let sample = InputSample {
            keys: vec![Key::Weapon3],
            mouse_yaw: 0,
            mouse_pitch: 0,
        };
        assert_eq!(build_ticcmd(&sample).weapon_select, 3);
        let zero = InputSample {
            keys: vec![Key::Weapon0],
            ..InputSample::default()
        };
        assert_eq!(build_ticcmd(&zero).weapon_select, 10);
    }

    #[test]
    fn move_up_down_fill_up_move() {
        let up = build_ticcmd(&InputSample {
            keys: vec![Key::MoveUp],
            ..InputSample::default()
        });
        let down = build_ticcmd(&InputSample {
            keys: vec![Key::MoveDown, Key::Speed],
            ..InputSample::default()
        });
        assert_eq!(up.up_move, 256);
        assert_eq!(down.up_move, -768);
    }

    #[test]
    fn mouse_pitch_scales_like_yaw() {
        let cmd = build_ticcmd(&InputSample {
            keys: vec![],
            mouse_yaw: 2,
            mouse_pitch: -3,
        });
        assert_eq!(cmd.yaw, (2 * MOUSE_YAW_SCALE) as i16);
        assert_eq!(cmd.pitch, (-3 * MOUSE_YAW_SCALE) as i16);
    }

    #[test]
    fn keyboard_turn_left_is_positive_angleturn() {
        let left = build_ticcmd(&InputSample {
            keys: vec![Key::Left],
            mouse_yaw: 0,
            mouse_pitch: 0,
        });
        let right = build_ticcmd(&InputSample {
            keys: vec![Key::Right],
            mouse_yaw: 0,
            mouse_pitch: 0,
        });
        assert!(left.yaw > 0, "left should increase Doom angle (CCW)");
        assert!(right.yaw < 0, "right should decrease Doom angle (CW)");
    }

    #[test]
    fn next_prev_weapon_buttons() {
        let next = build_ticcmd(&InputSample {
            keys: vec![Key::NextWeapon],
            mouse_yaw: 0,
            mouse_pitch: 0,
        });
        assert_eq!(
            next.buttons & TicCmdBits::BT_WEAPONNEXT,
            TicCmdBits::BT_WEAPONNEXT
        );
        let prev = build_ticcmd(&InputSample {
            keys: vec![Key::PrevWeapon],
            mouse_yaw: 0,
            mouse_pitch: 0,
        });
        assert_eq!(
            prev.buttons & TicCmdBits::BT_WEAPONPREV,
            TicCmdBits::BT_WEAPONPREV
        );
        let kick = build_ticcmd(&InputSample {
            keys: vec![Key::QuickKick],
            ..InputSample::default()
        });
        assert_eq!(
            kick.buttons & TicCmdBits::BT_QUICKKICK,
            TicCmdBits::BT_QUICKKICK
        );
        let inv = build_ticcmd(&InputSample {
            keys: vec![Key::InvUse],
            ..InputSample::default()
        });
        assert_eq!(inv.buttons & TicCmdBits::BT_INVUSE, TicCmdBits::BT_INVUSE);
    }
}
