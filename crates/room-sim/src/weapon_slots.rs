//! KEYCONF-driven weapon slot table (`setslot` / `addslot` / `addslotdefault`).

use std::collections::BTreeMap;

/// Default Doom weapon names per slot 1..=7.
#[must_use]
pub fn default_slot_weapons(slot: u8) -> &'static [&'static str] {
    match slot {
        1 => &["Chainsaw", "Fist"],
        2 => &["Pistol"],
        3 => &["SuperShotgun", "Shotgun"],
        4 => &["Chaingun"],
        5 => &["RocketLauncher"],
        6 => &["PlasmaRifle"],
        7 => &["BFG9000"],
        _ => &[],
    }
}

/// One player class registered via KEYCONF.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerClassEntry {
    /// Class name.
    pub class: String,
    /// Hide from class select menu.
    pub nomenu: bool,
}

/// Runtime weapon slots + player classes from KEYCONF.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponSlots {
    /// Slot index → ordered weapon class names (KEYCONF overrides).
    pub slots: BTreeMap<u8, Vec<String>>,
    /// Active weaponsection name (informational).
    pub section: Option<String>,
    /// Player classes (empty = use engine default / DECORATE).
    pub player_classes: Vec<PlayerClassEntry>,
    /// Whether `clearplayerclasses` was seen.
    pub cleared_player_classes: bool,
}

impl WeaponSlots {
    /// Start from Doom defaults (slots 1–7 filled).
    #[must_use]
    pub fn with_doom_defaults() -> Self {
        let mut s = Self::default();
        for slot in 1u8..=7 {
            s.slots.insert(
                slot,
                default_slot_weapons(slot)
                    .iter()
                    .map(|n| (*n).to_owned())
                    .collect(),
            );
        }
        s
    }

    /// Weapons for a slot (KEYCONF override or Doom default).
    #[must_use]
    pub fn weapons_for(&self, slot: u8) -> Vec<String> {
        if let Some(list) = self.slots.get(&slot) {
            return list.clone();
        }
        default_slot_weapons(slot)
            .iter()
            .map(|n| (*n).to_owned())
            .collect()
    }

    /// `weaponsection name`.
    pub fn set_section(&mut self, name: impl Into<String>) {
        self.section = Some(name.into());
    }

    /// Whether `weapon` sits in any slot (GZDoom `LocateWeapon`).
    #[must_use]
    pub fn contains(&self, weapon: &str) -> bool {
        (1u8..=10).any(|s| {
            self.weapons_for(s)
                .iter()
                .any(|w| w.eq_ignore_ascii_case(weapon))
        })
    }

    /// Drop every slotted weapon for which `keep` is false (e.g. no sprites
    /// in the loaded IWAD, like the super shotgun in Doom 1).
    pub fn retain_weapons(&mut self, mut keep: impl FnMut(&str) -> bool) {
        for slot in 1u8..=10 {
            let mut list = self.weapons_for(slot);
            let before = list.len();
            list.retain(|w| keep(w));
            if list.len() != before {
                self.slots.insert(slot, list);
            }
        }
    }

    /// `setslot n weapon…` — replace slot contents.
    pub fn set_slot(&mut self, slot: i32, weapons: &[String]) {
        if !(1..=10).contains(&slot) {
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let s = slot as u8;
        self.slots.insert(s, weapons.to_vec());
    }

    /// `addslot n weapon` — append if not already present.
    pub fn add_slot(&mut self, slot: i32, weapon: &str) {
        if !(1..=10).contains(&slot) {
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let s = slot as u8;
        let list = self.slots.entry(s).or_default();
        if !list.iter().any(|w| w.eq_ignore_ascii_case(weapon)) {
            list.push(weapon.to_owned());
        }
    }

    /// `addslotdefault n weapon` — append only if slot is empty / missing the weapon.
    pub fn add_slot_default(&mut self, slot: i32, weapon: &str) {
        if !(1..=10).contains(&slot) {
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let s = slot as u8;
        let list = self.slots.entry(s).or_insert_with(|| {
            default_slot_weapons(s)
                .iter()
                .map(|n| (*n).to_owned())
                .collect()
        });
        if !list.iter().any(|w| w.eq_ignore_ascii_case(weapon)) {
            list.push(weapon.to_owned());
        }
    }

    /// `clearplayerclasses`.
    pub fn clear_player_classes(&mut self) {
        self.player_classes.clear();
        self.cleared_player_classes = true;
    }

    /// `addplayerclass Class [nomenu]`.
    pub fn add_player_class(&mut self, class: &str, nomenu: bool) {
        if self
            .player_classes
            .iter()
            .any(|e| e.class.eq_ignore_ascii_case(class))
        {
            return;
        }
        self.player_classes.push(PlayerClassEntry {
            class: class.to_owned(),
            nomenu,
        });
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn setslot_replaces_defaults() {
        let mut s = WeaponSlots::with_doom_defaults();
        s.set_slot(2, &["Shotgun".into()]);
        assert_eq!(s.weapons_for(2), vec!["Shotgun".to_owned()]);
        assert_eq!(s.weapons_for(1)[0], "Chainsaw");
    }

    #[test]
    fn retain_weapons_drops_from_defaults() {
        let mut s = WeaponSlots::with_doom_defaults();
        s.retain_weapons(|w| w != "SuperShotgun");
        assert_eq!(s.weapons_for(3), vec!["Shotgun".to_owned()]);
        assert!(!s.contains("supershotgun"));
        assert!(s.contains("BFG9000"));
    }

    #[test]
    fn addslot_appends() {
        let mut s = WeaponSlots::default();
        s.add_slot(3, "Shotgun");
        s.add_slot(3, "SuperShotgun");
        s.add_slot(3, "Shotgun"); // dup ignored
        assert_eq!(s.weapons_for(3).len(), 2);
    }
}
