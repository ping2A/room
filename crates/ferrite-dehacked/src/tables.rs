//! Vanilla DeHackEd index tables (`ferrite.pk3/dehacked/`).

use std::collections::BTreeMap;
use std::sync::OnceLock;

const THINGS: &str = include_str!("../../../wadsrc/ferrite/dehacked/things.txt");
const SPRITES: &str = include_str!("../../../wadsrc/ferrite/dehacked/sprites.txt");
const SOUNDS: &str = include_str!("../../../wadsrc/ferrite/dehacked/sounds.txt");
const WEAPONS: &str = include_str!("../../../wadsrc/ferrite/dehacked/weapons.txt");
const AMMO: &str = include_str!("../../../wadsrc/ferrite/dehacked/ammo.txt");
const CODEPTRS: &str = include_str!("../../../wadsrc/ferrite/dehacked/codepointers.txt");
const STATES: &str = include_str!("../../../wadsrc/ferrite/dehacked/states.txt");

fn index_table(src: &'static str) -> BTreeMap<i32, &'static str> {
    let mut map = BTreeMap::new();
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((idx, name)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let Ok(idx) = idx.parse::<i32>() else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.eq_ignore_ascii_case("NULL") {
            continue;
        }
        map.insert(idx, name);
    }
    map
}

fn state_table(src: &'static str) -> BTreeMap<i32, (&'static str, &'static str, usize)> {
    let mut map = BTreeMap::new();
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(idx) = parts.next().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        let Some(class) = parts.next() else {
            continue;
        };
        let Some(label) = parts.next() else {
            continue;
        };
        let offset = parts
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        if class.eq_ignore_ascii_case("None") {
            continue;
        }
        map.insert(idx, (class, label, offset));
    }
    map
}

fn mnemonic_table(src: &'static str) -> BTreeMap<&'static str, &'static str> {
    let mut map = BTreeMap::new();
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(mnemonic) = parts.next() else {
            continue;
        };
        let action = parts.next().unwrap_or("");
        map.insert(mnemonic, action);
    }
    map
}

fn things() -> &'static BTreeMap<i32, &'static str> {
    static T: OnceLock<BTreeMap<i32, &'static str>> = OnceLock::new();
    T.get_or_init(|| index_table(THINGS))
}

fn sprites() -> &'static BTreeMap<i32, &'static str> {
    static T: OnceLock<BTreeMap<i32, &'static str>> = OnceLock::new();
    T.get_or_init(|| index_table(SPRITES))
}

fn sounds() -> &'static BTreeMap<i32, &'static str> {
    static T: OnceLock<BTreeMap<i32, &'static str>> = OnceLock::new();
    T.get_or_init(|| index_table(SOUNDS))
}

fn weapons() -> &'static BTreeMap<i32, &'static str> {
    static T: OnceLock<BTreeMap<i32, &'static str>> = OnceLock::new();
    T.get_or_init(|| index_table(WEAPONS))
}

fn ammo() -> &'static BTreeMap<i32, &'static str> {
    static T: OnceLock<BTreeMap<i32, &'static str>> = OnceLock::new();
    T.get_or_init(|| index_table(AMMO))
}

fn codeptrs() -> &'static BTreeMap<&'static str, &'static str> {
    static T: OnceLock<BTreeMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| mnemonic_table(CODEPTRS))
}

fn states() -> &'static BTreeMap<i32, (&'static str, &'static str, usize)> {
    static T: OnceLock<BTreeMap<i32, (&'static str, &'static str, usize)>> = OnceLock::new();
    T.get_or_init(|| state_table(STATES))
}

/// Vanilla `mobjinfo` index (1-based) → class name.
pub fn thing_name(index: i32) -> Option<&'static str> {
    things().get(&index).copied()
}

/// Resolve a thing index to a class name, allocating a DSDHacked placeholder when needed.
///
/// Indices absent from the vanilla/`things.txt` table map to `Deh_Actor_{N}`
/// (GZDoom DSDHacked free slots).
#[must_use]
pub fn thing_class_name(index: i32) -> Option<String> {
    if index <= 0 {
        return None;
    }
    if let Some(n) = thing_name(index) {
        return Some(n.to_owned());
    }
    Some(format!("Deh_Actor_{index}"))
}

/// Vanilla weapon index → class name.
pub fn weapon_name(index: i32) -> Option<&'static str> {
    weapons().get(&index).copied()
}

/// Vanilla ammo index → class name (`NULL` slots omitted).
pub fn ammo_name(index: i32) -> Option<&'static str> {
    ammo().get(&index).copied()
}

/// Vanilla sprite table index → 4-character name.
pub fn sprite_name(index: i32) -> Option<&'static str> {
    sprites().get(&index).copied()
}

/// DeHackEd sound index (`0` = none) → SNDINFO logical name.
pub fn sound_name(index: i32) -> Option<&'static str> {
    if index <= 0 {
        return None;
    }
    sounds().get(&index).copied()
}

/// Vanilla `states[]` index → `(class, label, offset)`.
pub fn frame_target(index: i32) -> Option<(&'static str, &'static str, usize)> {
    states().get(&index).copied()
}

/// DeHackEd action mnemonic → `A_*` function name (empty string = NULL).
pub fn codepointer_action(mnemonic: &str) -> Option<&'static str> {
    let m = mnemonic.trim();
    if m.is_empty() || m.eq_ignore_ascii_case("NULL") {
        return Some("");
    }
    if let Some(a) = codeptrs().get(m).copied() {
        return Some(a);
    }
    let upper = m.to_ascii_uppercase();
    for (k, v) in codeptrs() {
        if k.eq_ignore_ascii_case(&upper) {
            return Some(*v);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn things_sprites_sounds_cover_vanilla() {
        assert_eq!(thing_name(1), Some("DoomPlayer"));
        assert_eq!(thing_name(2), Some("ZombieMan"));
        assert_eq!(thing_name(16), Some("BaronOfHell"));
        assert!(things().values().any(|n| *n == "Cyberdemon"));
        assert_eq!(weapon_name(2), Some("Shotgun"));
        assert_eq!(ammo_name(0), Some("Clip"));
        assert_eq!(sprite_name(0), Some("TROO"));
        assert_eq!(sprite_name(29), Some("POSS"));
        assert_eq!(sound_name(0), None);
        assert_eq!(sound_name(1), Some("weapons/pistol"));
        assert!(sounds().values().any(|n| *n == "grunt/sight"));
        assert_eq!(codepointer_action("Chase"), Some("A_Chase"));
        assert_eq!(codepointer_action("NULL"), Some(""));
        assert_eq!(frame_target(174), Some(("ZombieMan", "Spawn", 0)));
        assert_eq!(frame_target(175), Some(("ZombieMan", "Spawn", 1)));
    }
}
